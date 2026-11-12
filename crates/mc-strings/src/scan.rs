//! The functions that search a string with string patterns ([`crate::strpat`]): `string_cases`, `string_position`,
//! `string_count`, `string_match_q`, `string_free_q`, `string_delete`, and the pattern forms of `string_replace`,
//! `string_split`, `string_contains`, `string_starts` and `string_ends` (decision S.10).
//!
//! A search spec is a pattern, a rule `p -> r` / `p :> r`, or a list of them (a list of patterns only is one
//! alternative pattern). At each position the items are tried in order; the first that matches gives the match.
//! Matches do not overlap unless `overlaps: true` (each start position, the first match there) or `overlaps: all`
//! (each start position, every distinct end) is given; `string_position` overlaps by default. `ignore_case: true`
//! compares characters case-insensitively. The right side of a rule is evaluated with the pattern's variables
//! replaced by their texts; for a regular expression, `$1`, `$2`, ... in a string right side are its groups' texts
//! and `$0` the whole match.
use crate::ctx::{charge, take, Lib};
use crate::strpat::{compile, first_char, Class, Hit, Node, Vars, M};
use mc_core::{Heap, Store, View, T};
use mc_eval::{NativeCx, Reply, Res};

/// One pattern of a search spec, with its variables and the right side of its rule.
pub(crate) struct Item<'h> {
    node: Node<'h>,
    vars: Vec<mc_core::Sym>,
    rhs: Option<T<'h>>,
    regex: bool,
    first: Option<char>,
}

fn is_rule(lib: &Lib, h: &Heap, t: T<'_>) -> bool {
    matches!(h.view(t), View::App { head, args: [_, _] }
        if head == h.sym_term(lib.s.rule) || head == h.sym_term(lib.s.rule_delayed))
}

fn item<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, rules: bool) -> Res<Option<Item<'h>>> {
    let h = cx.heap();
    let (p, rhs) = match h.view(t) {
        View::App { args: &[p, r], .. } if rules && is_rule(lib, h, t) => (p, Some(r)),
        _ => (t, None),
    };
    let mut vars = Vars { syms: vec![] };
    let Some(node) = compile(h, p, &mut vars, &mut |x| cx.eval(x))? else { return Ok(None) };
    let regex = matches!(h.view(p), View::App { head, .. } if matches!(h.view(head), View::Sym(s) if h.sym_name(s) == "regular_expression"));
    let first = first_char(&node);
    Ok(Some(Item { node, vars: vars.syms, rhs, regex, first }))
}

/// The items of a search spec (rules allowed when `rules`); `None` when it is not a string pattern.
pub(crate) fn items<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, rules: bool) -> Res<Option<Vec<Item<'h>>>> {
    let h = cx.heap();
    let parts: Vec<T<'h>> = match lib.as_list(h, t) {
        Some(xs) if xs.iter().any(|&x| is_rule(lib, h, x)) && rules => xs.to_vec(),
        _ => vec![t],
    };
    let mut out = Vec::with_capacity(parts.len());
    for x in parts {
        match item(lib, cx, x, rules)? {
            Some(it) => out.push(it),
            None => return Ok(None),
        }
    }
    Ok(Some(out))
}

/// The `string_match_q` reading of a literal string: `*` any run of characters, `@` a run of characters that are
/// not capital letters (`\*`, `\@` the characters themselves).
fn wildcard<'h>(s: &str) -> Node<'h> {
    let mut parts = vec![];
    let mut lit = vec![];
    let mut cs = s.chars();
    while let Some(c) = cs.next() {
        let run = match c {
            '\\' => {
                lit.push(cs.next().unwrap_or('\\'));
                continue;
            }
            '*' => Node::Run { class: Class::Any, min: 0, max: usize::MAX },
            '@' => Node::Run { class: Class::Not(vec![Class::Upper]), min: 1, max: usize::MAX },
            c => {
                lit.push(c);
                continue;
            }
        };
        if !lit.is_empty() {
            parts.push(Node::Lit(std::mem::take(&mut lit)));
        }
        parts.push(run);
    }
    if !lit.is_empty() {
        parts.push(Node::Lit(lit));
    }
    Node::Cat(parts)
}

/// How matches may overlap.
#[derive(Clone, Copy, PartialEq)]
enum Overlap {
    No,
    Yes,
    All,
}

/// The options of a search: overlaps and case.
struct Opts {
    overlap: Overlap,
    ic: bool,
}

fn opts<'a, 'h>(lib: &Lib, h: &'h Heap, args: &'a [T<'h>], overlap: Overlap) -> Result<(&'a [T<'h>], Opts), Reply<'h>> {
    let (args, given) = lib.options(h, args);
    let mut o = Opts { overlap, ic: false };
    for (name, v) in given {
        let val = match h.view(v) {
            View::Sym(s) => h.sym_name(s),
            _ => return Err(lib.unknown_option(h, name)),
        };
        match (&*h.sym_name(name), &*val) {
            ("overlaps", "true") => o.overlap = Overlap::Yes,
            ("overlaps", "false") => o.overlap = Overlap::No,
            ("overlaps", "all") => o.overlap = Overlap::All,
            ("ignore_case", "true") => o.ic = true,
            ("ignore_case", "false") => o.ic = false,
            _ => return Err(lib.unknown_option(h, name)),
        }
    }
    Ok((args, o))
}

/// The matches of `items` in `text`, in order, at most `limit`; with the index of the item that matched.
fn hits<'x, 'a, 'b, 'h>(
    cx: &'x mut NativeCx<'a, 'b, 'h>,
    text: &'x [char],
    items: &'x [Item<'h>],
    o: &Opts,
    limit: usize,
    whole: bool,
) -> Res<Result<Vec<(usize, Hit)>, ()>> {
    let mut out = vec![];
    let mut m = M::new(text, o.ic, &[], cx);
    let mut from = 0;
    'outer: while from <= text.len() && out.len() < limit {
        let mut found = None;
        for start in from..=text.len() {
            for (i, it) in items.iter().enumerate() {
                if let (Some(c), false) = (it.first, o.ic) {
                    if text.get(start) != Some(&c) {
                        continue;
                    }
                }
                m.vars = &it.vars;
                m.binds = vec![None; it.vars.len()];
                if o.overlap == Overlap::All {
                    let all = m.all_at(&it.node, start)?;
                    if !all.is_empty() {
                        for hit in all {
                            if out.len() < limit {
                                out.push((i, hit));
                            }
                        }
                        found = Some(start + 1);
                        break;
                    }
                } else if let Some(hit) = m.at(&it.node, start, whole)? {
                    let next = if o.overlap == Overlap::Yes || hit.end == start { start + 1 } else { hit.end };
                    out.push((i, hit));
                    found = Some(next);
                    break;
                }
                if m.over {
                    return Ok(Err(()));
                }
            }
            if found.is_some() || whole {
                break;
            }
        }
        match found {
            Some(next) => from = next,
            None => break 'outer,
        }
        if whole {
            break;
        }
    }
    Ok(Ok(out))
}

/// `failure(limit, string_pattern)`: a repetition nested too deep.
fn too_deep<'h>(lib: &Lib, h: &'h Heap) -> Reply<'h> {
    Reply::Done(lib.fail(h, h.sym("limit"), &[h.sym_term(h.sym("string_pattern"))]))
}

/// The value of a match: the matched text, or the rule's right side with the bindings.
fn value<'h>(cx: &mut NativeCx<'_, '_, 'h>, text: &[char], it: &Item<'h>, hit: &Hit) -> Res<T<'h>> {
    let h = cx.heap();
    let whole: String = text[hit.start..hit.end].iter().collect();
    let Some(rhs) = it.rhs else { return Ok(h.string(&whole)) };
    if it.regex {
        if let View::Str(r) = h.view(rhs) {
            return Ok(h.string(&dollars(h, r, text, it, hit, &whole)));
        }
    }
    let mut m = M::new(text, false, &it.vars, cx);
    m.binds = hit.binds.clone();
    m.apply(rhs)
}

/// `$0` (the match) and `$k` (group k) in a regular expression's replacement text.
fn dollars(h: &Heap, r: &str, text: &[char], it: &Item<'_>, hit: &Hit, whole: &str) -> String {
    let mut out = String::new();
    let mut cs = r.chars().peekable();
    while let Some(c) = cs.next() {
        if c == '$' {
            if let Some(d) = cs.peek().and_then(|d| d.to_digit(10)) {
                cs.next();
                if d == 0 {
                    out.push_str(whole);
                } else {
                    let g = h.sym(&format!("${d}"));
                    if let Some(i) = it.vars.iter().position(|&v| v == g) {
                        if let Some((a, b)) = hit.binds[i] {
                            out.extend(&text[a..b]);
                        }
                    }
                }
                continue;
            }
        }
        out.push(c);
    }
    out
}

/// The text of argument 1 and the items of argument 2 of a search, or the reply when they are not that.
#[allow(clippy::type_complexity)]
fn setup<'h>(
    lib: &Lib,
    cx: &mut NativeCx<'_, '_, 'h>,
    t: T<'h>,
    args: &[T<'h>],
    rules: bool,
) -> Res<Result<(Vec<char>, Vec<Item<'h>>), Reply<'h>>> {
    let h = cx.heap();
    let s = match lib.string(h, t, args[0], 1) {
        Ok(s) => s,
        Err(r) => return Ok(Err(r)),
    };
    let Some(its) = items(lib, cx, args[1], rules)? else {
        // a number or another atom that is no pattern is the wrong kind of argument; a symbolic term stays pending
        let atom = |x: T<'h>| !matches!(h.view(x), View::Sym(_) | View::App { .. });
        let bad = match lib.as_list(h, args[1]) {
            Some(xs) => xs.iter().any(|&x| atom(x)),
            None => atom(args[1]),
        };
        return Ok(Err(if bad { lib.bad_arg(h, t, 2) } else { Reply::Decline }));
    };
    Ok(Ok((s.chars().collect(), its)))
}

fn count_arg<'h>(lib: &Lib, h: &'h Heap, t: T<'h>, a: Option<&T<'h>>, k: usize) -> Result<usize, Reply<'h>> {
    match a {
        None => Ok(usize::MAX),
        Some(&n) => match lib.int(h, t, n, k)? {
            n @ 0.. => Ok(usize::try_from(n).unwrap_or(usize::MAX)),
            _ => Err(lib.bad_arg(h, t, k)),
        },
    }
}

/// `string_cases(s, p)`: the substrings matching `p`, left to right without overlaps; `string_cases(s, p -> r)`,
/// `(s, p :> r)`: the right side for each match; a list of patterns and rules; `(s, spec, n)`: at most n.
pub(crate) fn string_cases<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let (args, o) = take!(opts(lib, h, args, Overlap::No));
    if !(2..=3).contains(&args.len()) {
        return Ok(Reply::Decline);
    }
    let (text, its) = take!(setup(lib, cx, t, args, true)?);
    let limit = take!(count_arg(lib, h, t, args.get(2), 3));
    charge(cx, text.len())?;
    let Ok(found) = hits(cx, &text, &its, &o, limit, false)? else { return Ok(too_deep(lib, h)) };
    let mut out = Vec::with_capacity(found.len());
    for (i, hit) in &found {
        out.push(value(cx, &text, &its[*i], hit)?);
    }
    Ok(Reply::Done(h.call(lib.s.list, &out)))
}

/// `string_position(s, p)`: the `[start, end]` character positions (1-based, inclusive) of the matches of `p`,
/// overlapping by default; `(s, p, n)`: at most n.
pub(crate) fn string_position<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let (args, o) = take!(opts(lib, h, args, Overlap::Yes));
    if !(2..=3).contains(&args.len()) {
        return Ok(Reply::Decline);
    }
    let (text, its) = take!(setup(lib, cx, t, args, false)?);
    let limit = take!(count_arg(lib, h, t, args.get(2), 3));
    charge(cx, text.len())?;
    let Ok(found) = hits(cx, &text, &its, &o, limit, false)? else { return Ok(too_deep(lib, h)) };
    let out: Vec<T<'h>> = found
        .iter()
        .map(|(_, x)| h.call(lib.s.list, &[h.int(x.start as i64 + 1), h.int(x.end as i64)]))
        .collect();
    Ok(Reply::Done(h.call(lib.s.list, &out)))
}

/// `string_count(s, p)`: the number of matches of `p` (without overlaps by default).
pub(crate) fn string_count<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let (args, o) = take!(opts(lib, h, args, Overlap::No));
    if args.len() != 2 {
        return Ok(Reply::Decline);
    }
    let (text, its) = take!(setup(lib, cx, t, args, false)?);
    charge(cx, text.len())?;
    let Ok(found) = hits(cx, &text, &its, &o, usize::MAX, false)? else { return Ok(too_deep(lib, h)) };
    Ok(Reply::Done(h.int(found.len() as i64)))
}

/// `string_match_q(s, p)`: whether the whole of `s` matches `p` (a literal string: `*` and `@` are wildcards).
pub(crate) fn string_match_q<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let (args, o) = take!(opts(lib, h, args, Overlap::No));
    let &[s, p] = args else { return Ok(Reply::Decline) };
    let its = match h.view(p) {
        View::Str(w) => vec![Item { node: wildcard(w), vars: vec![], rhs: None, regex: false, first: None }],
        _ => take!(setup(lib, cx, t, args, false)?).1,
    };
    let text: Vec<char> = take!(lib.string(h, t, s, 1)).chars().collect();
    charge(cx, text.len())?;
    let Ok(found) = hits(cx, &text, &its, &o, 1, true)? else { return Ok(too_deep(lib, h)) };
    Ok(Reply::Done(cx.bool(!found.is_empty())))
}

/// `string_free_q(s, p)`: whether no substring of `s` matches `p`.
pub(crate) fn string_free_q<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let (args, o) = take!(opts(lib, h, args, Overlap::No));
    if args.len() != 2 {
        return Ok(Reply::Decline);
    }
    let (text, its) = take!(setup(lib, cx, t, args, false)?);
    charge(cx, text.len())?;
    let Ok(found) = hits(cx, &text, &its, &o, 1, false)? else { return Ok(too_deep(lib, h)) };
    Ok(Reply::Done(cx.bool(found.is_empty())))
}

/// Which test of a string against a pattern.
#[derive(Clone, Copy)]
pub(crate) enum Where {
    Anywhere,
    Start,
    End,
}

/// `string_contains`, `string_starts`, `string_ends` with a pattern.
pub(crate) fn test<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>], w: Where) -> Res<Reply<'h>> {
    let h = cx.heap();
    let (args, o) = take!(opts(lib, h, args, Overlap::No));
    if args.len() != 2 {
        return Ok(Reply::Decline);
    }
    let (text, its) = take!(setup(lib, cx, t, args, false)?);
    charge(cx, text.len())?;
    let found = match w {
        Where::Anywhere => hits(cx, &text, &its, &o, 1, false)?.map(|f| !f.is_empty()),
        Where::Start => {
            let mut m = M::new(&text, o.ic, &[], cx);
            let mut any = false;
            for it in &its {
                m.vars = &it.vars;
                m.binds = vec![None; it.vars.len()];
                if m.at(&it.node, 0, false)?.is_some() {
                    any = true;
                    break;
                }
            }
            if m.over {
                Err(())
            } else {
                Ok(any)
            }
        }
        Where::End => {
            let mut m = M::new(&text, o.ic, &[], cx);
            let mut any = false;
            'search: for start in 0..=text.len() {
                for it in &its {
                    m.vars = &it.vars;
                    m.binds = vec![None; it.vars.len()];
                    if !m.all_at(&it.node, start)?.iter().all(|x| x.end != text.len()) {
                        any = true;
                        break 'search;
                    }
                }
            }
            if m.over {
                Err(())
            } else {
                Ok(any)
            }
        }
    };
    let Ok(found) = found else { return Ok(too_deep(lib, h)) };
    Ok(Reply::Done(cx.bool(found)))
}

/// `string_replace(s, rules[, n])` with patterns: each match (without overlaps, left to right, at most n) replaced
/// by its rule's right side; the pieces joined with `string_expression` (one string when every piece is one).
pub(crate) fn replace<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>], delete: bool) -> Res<Reply<'h>> {
    let h = cx.heap();
    let (args, o) = take!(opts(lib, h, args, Overlap::No));
    if !(2..=3).contains(&args.len()) {
        return Ok(Reply::Decline);
    }
    let (text, mut its) = take!(setup(lib, cx, t, args, !delete)?);
    if delete {
        for it in &mut its {
            it.rhs = Some(h.string(""));
        }
    } else if its.iter().any(|it| it.rhs.is_none()) {
        return Ok(lib.bad_arg(h, t, 2));
    }
    let limit = take!(count_arg(lib, h, t, args.get(2), 3));
    charge(cx, text.len())?;
    let Ok(found) = hits(cx, &text, &its, &o, limit, false)? else { return Ok(too_deep(lib, h)) };
    if found.is_empty() {
        return Ok(Reply::Done(args[0]));
    }
    let mut parts = vec![];
    let mut at = 0;
    for (i, hit) in &found {
        if hit.start < at {
            continue;
        }
        parts.push(h.string(&text[at..hit.start].iter().collect::<String>()));
        parts.push(value(cx, &text, &its[*i], hit)?);
        at = hit.end;
    }
    parts.push(h.string(&text[at..].iter().collect::<String>()));
    // empty pieces are no part of the result (one empty string stays when nothing else is left)
    let mut parts: Vec<T<'h>> = parts.into_iter().filter(|&p| !matches!(h.view(p), View::Str(""))).collect();
    if parts.is_empty() {
        parts.push(h.string(""));
    }
    let e = h.call(h.sym("string_expression"), &parts);
    Ok(Reply::Done(cx.eval(e)?))
}

/// `string_split(s, p)` with a pattern: the pieces between matches (empty pieces kept, decision S.3);
/// `string_split(s, p -> v)`: `v` (evaluated with the bindings) between the pieces; `(s, spec, n)`: at most n pieces.
pub(crate) fn split<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let (args, o) = take!(opts(lib, h, args, Overlap::No));
    if !(2..=3).contains(&args.len()) {
        return Ok(Reply::Decline);
    }
    let (text, its) = take!(setup(lib, cx, t, args, true)?);
    let limit = match take!(count_arg(lib, h, t, args.get(2), 3)) {
        0 => return Ok(lib.bad_arg(h, t, 3)),
        n => n,
    };
    charge(cx, text.len())?;
    let Ok(found) = hits(cx, &text, &its, &o, limit.saturating_sub(1), false)? else { return Ok(too_deep(lib, h)) };
    let mut out = vec![];
    let mut at = 0;
    for (i, hit) in &found {
        if hit.end == hit.start && (hit.start == 0 || hit.start == text.len()) {
            // an empty match at an end splits nothing off
            continue;
        }
        out.push(h.string(&text[at..hit.start].iter().collect::<String>()));
        if its[*i].rhs.is_some() {
            out.push(value(cx, &text, &its[*i], hit)?);
        }
        at = hit.end;
    }
    out.push(h.string(&text[at..].iter().collect::<String>()));
    Ok(Reply::Done(h.call(lib.s.list, &out)))
}
