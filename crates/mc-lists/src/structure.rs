#![allow(clippy::needless_borrow, clippy::redundant_guards)]
//! Parts by position and by pattern (decision p6core.15): `match_q`, `head`, `dimensions`, `array_depth`,
//! `extract`, `replace_part`, `delete`, `map_at`, `level`, `replace`, `delete_cases`, `first_case`,
//! `reverse_sort`, `take_largest`, `take_smallest`, and the display wrappers `full_form` and `input_form`.
//!
//! A position is a list of indices `[i, j, ...]` (as `position` gives them; a single index `i` is `[i]`), each
//! 1-based, negative from the end, 0 for the head. Several positions are a list of such lists.
use crate::ctx::{take, Lib, Meter};
use crate::levels::{level_spec, walk, Levels};
use crate::matching::Matcher;
use crate::measure::is_complex_number;
use mc_core::{Heap, Store, View, T};
use mc_eval::{NativeCx, Reply, Res};

/// The arguments of `t` for positions: a compound term other than a complex number.
fn args_of<'h>(h: &'h Heap, t: T<'h>) -> Option<(T<'h>, &'h [T<'h>])> {
    match h.view(t) {
        View::App { .. } if is_complex_number(h, t) => None,
        View::App { head, args } => Some((head, args)),
        _ => None,
    }
}

/// The index `i` among `n` arguments: 0 the head, `1..=n`, `-n..=-1`.
fn index(i: i64, n: usize) -> Option<usize> {
    let n = n as i64;
    match i {
        0..=i64::MAX if i <= n => Some(i as usize),
        i64::MIN..=-1 if i >= -n => Some((n + 1 + i) as usize),
        _ => None,
    }
}

/// The positions a specification names: `i`, `[i, j, ...]`, or a list of such lists. `None`: not one.
fn positions<'h>(h: &'h Heap, cx: &NativeCx<'_, '_, 'h>, spec: T<'h>) -> Option<Vec<Vec<i64>>> {
    let one = |t: T<'h>| -> Option<Vec<i64>> {
        match h.view(t) {
            View::Int(i) => Some(vec![i]),
            _ => cx.as_list(t)?.iter().map(|&x| if let View::Int(i) = h.view(x) { Some(i) } else { None }).collect(),
        }
    };
    if let Some(xs) = cx.as_list(spec) {
        if !xs.is_empty() && xs.iter().all(|&x| cx.as_list(x).is_some()) {
            return xs.iter().map(|&x| one(x)).collect();
        }
    }
    one(spec).map(|p| vec![p])
}

/// The positions of `spec` in an association with entries `es` (decision io-core.4): a key (any atom other than an
/// integer, or a term) stands for the position of its entry; integers are positions. Also whether `spec` names
/// several positions.
fn assoc_positions<'h>(
    h: &'h Heap,
    cx: &NativeCx<'_, '_, 'h>,
    es: &[(T<'h>, T<'h>)],
    spec: T<'h>,
) -> Option<(Vec<Vec<i64>>, bool)> {
    let comp = |x: T<'h>| -> Option<i64> {
        match h.view(x) {
            // `key(k)` names the entry of the key k; an integer is a position
            View::App { head, args: &[k] } if matches!(h.view(head), View::Sym(s) if h.sym_name(s) == "key") => {
                es.iter().position(|&(kk, _)| kk == k).map(|i| i as i64 + 1)
            }
            View::Int(i) => Some(i),
            _ => es.iter().position(|&(k, _)| k == x).map(|i| i as i64 + 1),
        }
    };
    let path = |t: T<'h>| -> Option<Vec<i64>> {
        match cx.as_list(t) {
            Some(xs) if !xs.is_empty() => {
                let mut p = vec![comp(xs[0])?];
                for &x in &xs[1..] {
                    let View::Int(i) = h.view(x) else { return None };
                    p.push(i);
                }
                Some(p)
            }
            Some(_) => None,
            None => Some(vec![comp(t)?]),
        }
    };
    if let Some(xs) = cx.as_list(spec) {
        if !xs.is_empty() && xs.iter().all(|&x| cx.as_list(x).is_some()) {
            return Some((xs.iter().map(|&x| path(x)).collect::<Option<_>>()?, true));
        }
    }
    Some((vec![path(spec)?], false))
}

/// The association with the keys of `es` and the values `vs` (as many).
fn rekeyed<'h>(lib: &Lib, h: &'h Heap, es: &[(T<'h>, T<'h>)], vs: &[T<'h>]) -> T<'h> {
    let rule = h.sym_term(lib.s.rule);
    let rules: Vec<T<'h>> = es.iter().zip(vs).map(|(&(k, _), &v)| h.app(rule, &[k, v])).collect();
    h.app(h.sym_term(lib.s.assoc), &rules)
}

/// The part of `t` at `pos`, or the first index that is outside.
fn part_at<'h>(h: &'h Heap, t: T<'h>, pos: &[i64]) -> Result<T<'h>, i64> {
    let mut cur = t;
    for &i in pos {
        let Some((head, args)) = args_of(h, cur) else { return Err(i) };
        cur = match index(i, args.len()) {
            Some(0) => head,
            Some(k) => args[k - 1],
            None => return Err(i),
        };
    }
    Ok(cur)
}

/// What [`rebuild_at`] does at a position.
#[derive(Clone, Copy)]
enum Edit<'h> {
    /// replace the part by this term
    Set(T<'h>),
    /// replace the part `p` by `f(p)`
    Apply(T<'h>),
    /// remove the part
    Delete,
}

/// `t` with the edit applied at `pos`; `Err(i)` for an index outside.
fn rebuild_at<'h>(h: &'h Heap, t: T<'h>, pos: &[i64], edit: Edit<'h>) -> Result<Option<T<'h>>, i64> {
    let Some((&i, rest)) = pos.split_first() else {
        return Ok(match edit {
            Edit::Set(v) => Some(v),
            Edit::Apply(f) => Some(h.app(f, &[t])),
            Edit::Delete => None,
        });
    };
    let Some((head, args)) = args_of(h, t) else { return Err(i) };
    // an association's parts are its values (part 0 its head, applied to the values)
    if let Some(es) = crate::levels::assoc_entries(h, t) {
        let Some(k) = index(i, es.len()) else { return Err(i) };
        if k == 0 {
            let vals: Vec<T<'h>> = es.iter().map(|&(_, v)| v).collect();
            return Ok(Some(match rebuild_at(h, head, rest, edit)? {
                Some(nh) => h.app(nh, &vals),
                None => h.app(h.sym_term(h.sym("sequence")), &vals),
            }));
        }
        let mut out = args.to_vec();
        match rebuild_at(h, es[k - 1].1, rest, edit)? {
            Some(v) => {
                // the entry keeps its kind of rule
                let rule_head = match h.view(args[k - 1]) {
                    View::App { head, .. } => head,
                    _ => h.sym_term(h.sym("rule")),
                };
                out[k - 1] = h.app(rule_head, &[es[k - 1].0, v]);
            }
            None => {
                out.remove(k - 1);
            }
        }
        return Ok(Some(h.app(head, &out)));
    }
    let Some(k) = index(i, args.len()) else { return Err(i) };
    if k == 0 {
        // the head
        return Ok(Some(match rebuild_at(h, head, rest, edit)? {
            Some(nh) => h.app(nh, args),
            None => h.app(h.sym_term(h.sym("sequence")), args),
        }));
    }
    let mut out = args.to_vec();
    match rebuild_at(h, args[k - 1], rest, edit)? {
        Some(v) => out[k - 1] = v,
        None => {
            out.remove(k - 1);
        }
    }
    Ok(Some(h.app(head, &out)))
}

/// A rebuilt term, evaluated (replaced parts may be calls to evaluate).
fn settle<'h>(_lib: &Lib, _h: &'h Heap, v: T<'h>) -> Reply<'h> {
    Reply::Again(v)
}

/// `match_q(x, p)`: whether `x` matches the pattern `p`.
pub(crate) fn match_q<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [x, p] = *args else { return Ok(Reply::Decline) };
    let Some(mut m) = Matcher::new(lib, h, p) else { return Ok(Reply::Done(lib.bad_arg(h, t, 2))) };
    let r = m.matches(cx, x)?;
    Ok(Reply::Done(cx.bool(r)))
}

/// `head(x)`: the head of a compound term; for an atom the name of its type (`Int`, `Rat`, `Float`, `Complex`,
/// `Str`, `Sym`).
pub(crate) fn head<'h>(_lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [x] = *args else { return Ok(Reply::Decline) };
    if h.packed(x).is_some() {
        return Ok(Reply::Done(h.sym_term(h.sym("list"))));
    }
    let ty = |n: &str| Ok(Reply::Done(h.sym_term(h.sym(n))));
    match h.view(x) {
        View::App { .. } if is_complex_number(h, x) => ty("Complex"),
        View::App { head, .. } => Ok(Reply::Done(head)),
        View::Int(_) | View::BigInt { .. } => ty("Int"),
        View::Rational { .. } => ty("Rat"),
        View::Real(_) | View::Ball { .. } => ty("Float"),
        View::Str(_) => ty("Str"),
        View::Sym(_) => ty("Sym"),
    }
}

/// `dimensions(x)`: the lengths of the levels of `x` down to where its parts stop having one head and one length
/// (`[2, 3]` for a 2 by 3 matrix, `[]` for an atom).
/// `dimensions(x, n)`: at most `n` levels; `allowed_heads: list` (or `"ListLike"`): only levels of lists
/// (`all` any one head per level; the default, the top level's head at every level).
pub(crate) fn dimensions<'h>(_lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let (args, allowed) = crate::ctx::option(h, args, 1, "allowed_heads");
    let list = h.sym_term(h.sym("list"));
    // the default: every level has the head of the top level (`dimensions(f(f(1, 2), f(3, 4)))` is [2, 2], an
    // array of calls `a(i, j)` adds no level); `allowed_heads: all` any one head per level
    let top = |x: T<'h>| if h.packed(x).is_some() { Some(list) } else { args_of(h, x).map(|(g, _)| g) };
    // `allowed_heads: full`: any heads, also different ones within a level
    let full = allowed.is_some_and(|a| matches!(h.view(a), View::Sym(s) if h.sym_name(s) == "full"));
    // a set of heads: a list (one head per level) or alternatives (mixed within a level)
    if let Some((set, mixed)) = allowed.and_then(|a| head_set(cx, a)) {
        let (x, most) = match *args {
            [x] => (x, usize::MAX),
            [x, n] => match h.view(n) {
                View::Int(n) if n >= 0 => (x, n as usize),
                _ => return Ok(Reply::Decline),
            },
            _ => return Ok(Reply::Decline),
        };
        let mut dims = dims_in_set(cx, x, &set, mixed)?;
        dims.truncate(most);
        let v: Vec<T<'h>> = dims.iter().map(|&d| h.int(d as i64)).collect();
        return Ok(Reply::Done(cx.list(&v)));
    }
    let only = match allowed.map(|a| h.view(a)) {
        None => args.first().and_then(|&x| top(x)),
        Some(View::Sym(s)) if h.sym_name(s) == "all" || h.sym_name(s) == "full" => None,
        Some(View::Sym(s)) if h.sym_name(s) == "list" => Some(list),
        Some(View::Str("ListLike")) => Some(list),
        Some(_) => return Ok(Reply::Decline),
    };
    let (x, most) = match *args {
        [x] => (x, usize::MAX),
        [x, n] => match h.view(n) {
            View::Int(n) if n >= 0 => (x, n as usize),
            _ => return Ok(Reply::Decline),
        },
        _ => return Ok(Reply::Decline),
    };
    let mut dims = dims_by(cx, x, only, full)?;
    dims.truncate(most);
    let v: Vec<T<'h>> = dims.iter().map(|&d| h.int(d as i64)).collect();
    Ok(Reply::Done(cx.list(&v)))
}

pub(crate) fn dims_of<'h>(cx: &mut NativeCx<'_, '_, 'h>, x: T<'h>) -> Res<Vec<usize>> {
    dims_with(cx, x, None)
}

/// The dimensions of the levels of lists of `x` (calls of other heads are elements).
pub(crate) fn list_dims<'h>(cx: &mut NativeCx<'_, '_, 'h>, x: T<'h>) -> Res<Vec<usize>> {
    let list = cx.heap().sym_term(cx.heap().sym("list"));
    dims_with(cx, x, Some(list))
}

/// The dimensions of `x`, with every level's head `only` when given.
fn dims_with<'h>(cx: &mut NativeCx<'_, '_, 'h>, x: T<'h>, only: Option<T<'h>>) -> Res<Vec<usize>> {
    dims_by(cx, x, only, false)
}

/// The dimensions of `x`; `full`: the parts of a level may have different heads (`allowed_heads: full`, round 37),
/// otherwise one head per level (`only` when given).
fn dims_by<'h>(cx: &mut NativeCx<'_, '_, 'h>, x: T<'h>, only: Option<T<'h>>, full: bool) -> Res<Vec<usize>> {
    let h = cx.heap();
    if let Some(p) = h.packed(x) {
        return Ok(p.dims().iter().map(|&d| d as usize).collect());
    }
    let mut dims = vec![];
    let mut level: Vec<T<'h>> = vec![x];
    let mut m = Meter::default();
    while let Some((head, first)) = args_of(h, level[0]) {
        if only.is_some_and(|o| o != head) {
            break;
        }
        let n = first.len();
        let mut next = Vec::with_capacity(level.len() * n);
        for &u in &level {
            m.step(cx)?;
            match args_of(h, u) {
                Some((g, a)) if (full || g == head) && a.len() == n => next.extend_from_slice(a),
                _ => return Ok(dims),
            }
        }
        dims.push(n);
        if n == 0 {
            break;
        }
        level = next;
    }
    Ok(dims)
}

/// A set of allowed heads: `[h1, h2, ...]` (each level of one head of the set) or `h1 | h2 | ...` (the parts of
/// a level may have different heads of the set); `"ListLike"` stands for `list`. `None` for another value.
fn head_set<'h>(cx: &NativeCx<'_, '_, 'h>, spec: T<'h>) -> Option<(Vec<T<'h>>, bool)> {
    let h = cx.heap();
    let (items, mixed): (Vec<T<'h>>, bool) = match cx.as_list(spec) {
        Some(xs) => (xs.to_vec(), false),
        None => match h.view(spec) {
            View::App { head, args } if matches!(h.view(head), View::Sym(s) if h.sym_name(s) == "alternatives") => (args.to_vec(), true),
            _ => return None,
        },
    };
    let list = h.sym_term(h.sym("list"));
    let mut set = Vec::with_capacity(items.len());
    for x in items {
        match h.view(x) {
            View::Sym(_) => set.push(x),
            View::Str("ListLike") => set.push(list),
            _ => return None,
        }
    }
    (!set.is_empty()).then_some((set, mixed))
}

/// The head and the elements of `u` as an array level: a compound term's arguments; `numeric_array(data, type)`
/// and `byte_array(data)` hold the elements of their data (rows of a nested data list stay arrays of that head).
fn array_parts<'h>(cx: &NativeCx<'_, '_, 'h>, u: T<'h>) -> Option<(T<'h>, Vec<T<'h>>)> {
    let h = cx.heap();
    if let View::App { head, args } = h.view(u) {
        let name = match h.view(head) {
            View::Sym(s) => h.sym_name(s),
            _ => String::new(),
        };
        if (name == "numeric_array" && args.len() == 2) || (name == "byte_array" && args.len() == 1) {
            let rows = cx.as_list(args[0])?;
            let elems = rows
                .iter()
                .map(|&r| {
                    if cx.as_list(r).is_some() {
                        let mut v = vec![r];
                        v.extend_from_slice(&args[1..]);
                        h.app(head, &v)
                    } else {
                        r
                    }
                })
                .collect();
            return Some((head, elems));
        }
    }
    if h.packed(u).is_some() {
        return cx.as_list(u).map(|xs| (h.sym_term(h.sym("list")), xs.to_vec()));
    }
    args_of(h, u).map(|(g, a)| (g, a.to_vec()))
}

/// The dimensions of `x` counting levels whose heads are in `set` (`mixed`: different heads of the set within a
/// level).
fn dims_in_set<'h>(cx: &mut NativeCx<'_, '_, 'h>, x: T<'h>, set: &[T<'h>], mixed: bool) -> Res<Vec<usize>> {
    let mut dims = vec![];
    let mut level: Vec<T<'h>> = vec![x];
    let mut m = Meter::default();
    loop {
        let Some((head, first)) = array_parts(cx, level[0]) else { return Ok(dims) };
        if !set.contains(&head) {
            return Ok(dims);
        }
        let n = first.len();
        let mut next = Vec::with_capacity(level.len() * n);
        for &u in &level {
            m.step(cx)?;
            match array_parts(cx, u) {
                Some((g, a)) if (g == head || (mixed && set.contains(&g))) && a.len() == n => next.extend(a),
                _ => return Ok(dims),
            }
        }
        dims.push(n);
        if n == 0 {
            return Ok(dims);
        }
        level = next;
    }
}

/// `array_depth(x)`: the number of levels of `x` that form a full array (the length of `dimensions(x)`).
/// `allowed_heads: list` (or `"ListLike"`) counts only levels of lists, `allowed_heads: all` levels of any one head
/// (the default, P9B.6).
pub(crate) fn array_depth<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let (args, heads) = crate::ctx::option(h, args, 1, "allowed_heads");
    let [x] = *args else { return Ok(Reply::Decline) };
    let list = h.sym_term(lib.s.list);
    let full = heads.is_some_and(|a| matches!(h.view(a), View::Sym(s) if h.sym_name(s) == "full"));
    if let Some((set, mixed)) = heads.and_then(|a| head_set(cx, a)) {
        let d = dims_in_set(cx, x, &set, mixed)?.len();
        return Ok(Reply::Done(h.int(d as i64)));
    }
    let only = match heads.map(|v| h.view(v)) {
        None if h.packed(x).is_some() => Some(list),
        None => args_of(h, x).map(|(g, _)| g),
        Some(View::Sym(s)) if h.sym_name(s) == "all" || h.sym_name(s) == "full" => None,
        Some(View::Str(s)) if s == "ListLike" => Some(list),
        Some(_) if heads == Some(list) => Some(list),
        Some(_) => return Ok(Reply::Decline),
    };
    let d = dims_by(cx, x, only, full)?.len();
    Ok(Reply::Done(h.int(d as i64)))
}

/// `extract(x, pos)`: the part at a position; `extract(x, [pos1, pos2, ...])`: the list of parts at several. The
/// parts are evaluated as they are returned (extract(quote(1 + 2), [1, 1]) is 3).
pub(crate) fn extract<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    Ok(match extract_parts(lib, cx, t, args)? {
        Reply::Done(v) if args.len() == 2 && !lib.is_failure(h, v) => Reply::Again(v),
        r => r,
    })
}

fn extract_parts<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    // extract(x, pos, f): each part wrapped in f before it is evaluated, so that a held f (`quote`) keeps a part of
    // a held term unevaluated (P9B.4)
    if let [x, spec, f] = *args {
        let several = cx.as_list(spec).is_some_and(|ps| !ps.is_empty() && ps.iter().all(|&p| cx.as_list(p).is_some()));
        return Ok(match extract_parts(lib, cx, _t, &[x, spec])? {
            Reply::Done(v) if lib.is_failure(h, v) => Reply::Done(v),
            Reply::Done(v) if several => match cx.as_list(v) {
                Some(vs) => Reply::Again(cx.list(&vs.iter().map(|&u| h.app(f, &[u])).collect::<Vec<_>>())),
                None => Reply::Decline,
            },
            Reply::Done(v) => Reply::Again(h.app(f, &[v])),
            _ => Reply::Decline,
        });
    }
    let [x, spec] = *args else { return Ok(Reply::Decline) };
    // an association: keys or positions of its values (decision io-core.4)
    if let Some(es) = crate::assocs::entries(lib, h, x) {
        if let Some((ps, several)) = assoc_positions(h, cx, &es, spec) {
            let vs = cx.list(&es.iter().map(|&(_, v)| v).collect::<Vec<_>>());
            let out: Result<Vec<T<'h>>, i64> = ps.iter().map(|p| part_at(h, vs, p)).collect();
            if let Ok(out) = out {
                return Ok(Reply::Done(if several { cx.list(&out) } else { out[0] }));
            }
        }
        // positions in the term itself (as `position` gives them): the structural parts below
    }
    let Some(ps) = positions(h, cx, spec) else {
        // positions with `all` or lists of indices: `part` of each (decision cov-core.10)
        let part = h.sym_term(h.sym("part"));
        let one = |p: T<'h>| -> Option<T<'h>> {
            let mut a = vec![x];
            match cx.as_list(p) {
                Some(is) => a.extend_from_slice(is),
                None => a.push(p),
            }
            Some(h.app(part, &a))
        };
        return Ok(match cx.as_list(spec) {
            Some(xs) if !xs.is_empty() && xs.iter().all(|&p| cx.as_list(p).is_some()) => {
                let v: Option<Vec<T<'h>>> = xs.iter().map(|&p| one(p)).collect();
                v.map_or(Reply::Decline, |v| Reply::Again(cx.list(&v)))
            }
            Some(_) => one(spec).map_or(Reply::Decline, Reply::Again),
            None => Reply::Decline,
        });
    };
    let several = cx.as_list(spec).is_some_and(|xs| xs.first().is_some_and(|&y| cx.as_list(y).is_some()));
    let mut out = Vec::with_capacity(ps.len());
    for p in &ps {
        cx.tick(1)?;
        match part_at(h, x, p) {
            Ok(v) => out.push(v),
            Err(i) => return Ok(Reply::Done(lib.bad_index(h, h.int(i), 0))),
        }
    }
    Ok(Reply::Done(if several { cx.list(&out) } else { out[0] }))
}

/// The edits of `replace_part`: `pos -> v` or a list of such rules.
fn rules_of<'h>(lib: &Lib, h: &'h Heap, cx: &NativeCx<'_, '_, 'h>, r: T<'h>) -> Option<Vec<(Vec<Vec<i64>>, T<'h>)>> {
    let rule = |t: T<'h>| match h.view(t) {
        View::App { head, args: [p, v] } if head == h.sym_term(lib.s.rule) || head == h.sym_term(lib.s.rule_delayed) => {
            Some((positions(h, cx, *p)?, *v))
        }
        _ => None,
    };
    match cx.as_list(r) {
        Some(xs) => xs.iter().map(|&x| rule(x)).collect(),
        None => rule(r).map(|r| vec![r]),
    }
}

/// `replace_part(x, pos -> v)`, `replace_part(x, [pos1 -> v1, ...])`: `x` with the parts at the positions
/// replaced (a rule `[p1, p2] -> v` with a list of positions replaces each).
pub(crate) fn replace_part<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    // heads: true lets position patterns reach the heads (index 0); heads: false keeps every head as it is
    let Some((plain, flag)) = crate::ctx::heads_flag(h, args, 2) else { return Ok(Reply::Decline) };
    let heads = (plain.len() < args.len()).then_some(flag);
    let [x, r] = *plain else { return Ok(Reply::Decline) };
    if heads.is_none() {
        if let Some(v) = replace_keys(lib, cx, x, r)? {
            return Ok(Reply::Done(v));
        }
    }
    let Some(rules) = rules_of(lib, h, cx, r) else {
        if let Some(v) = replace_matching_parts(lib, cx, x, r, heads)? {
            return Ok(settle(lib, h, v));
        }
        return Ok(Reply::Done(lib.bad_arg(h, t, 2)));
    };
    let mut cur = x;
    for (ps, v) in rules {
        for p in ps {
            if heads == Some(false) && p.contains(&0) {
                continue;
            }
            cx.tick(1)?;
            match rebuild_at(h, cur, &p, Edit::Set(v)) {
                Ok(Some(u)) => cur = u,
                Ok(None) => {}
                // a position outside leaves the term as it is
                Err(_) => {}
            }
        }
    }
    Ok(settle(lib, h, cur))
}

/// `replace_part(a, k -> v)` on an association `a` whose key `k` exists: the entry's value replaced, in place
/// (`[k, i...] -> v` replaces inside the value). `None` unless every rule's position starts with a key of `a`, so
/// integer positions and patterns keep their structural meaning.
fn replace_keys<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, x: T<'h>, r: T<'h>) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    let Some(mut es) = crate::assocs::entries(lib, h, x) else { return Ok(None) };
    let rules: Vec<T<'h>> = match cx.as_list(r) {
        Some(xs) => xs.to_vec(),
        None => vec![r],
    };
    let (rule, delayed) = (h.sym_term(lib.s.rule), h.sym_term(lib.s.rule_delayed));
    let mut edits = Vec::with_capacity(rules.len());
    for &q in &rules {
        let View::App { head, args: &[p, v] } = h.view(q) else { return Ok(None) };
        if head != rule && head != delayed {
            return Ok(None);
        }
        let (k, rest): (T<'h>, Vec<T<'h>>) = match cx.as_list(p) {
            Some([k, rest @ ..]) => (*k, rest.to_vec()),
            Some([]) => return Ok(None),
            None => (p, Vec::new()),
        };
        // a key, or else an integer: the position of a value (from the end when negative)
        let n = es.len() as i64;
        let i = match es.iter().position(|&(key, _)| key == k) {
            Some(i) => i,
            None => match h.view(k) {
                View::Int(j) if (1..=n).contains(&j) => (j - 1) as usize,
                View::Int(j) if (-n..=-1).contains(&j) => (n + j) as usize,
                _ => return Ok(None),
            },
        };
        edits.push((i, rest, v));
    }
    let replace_part = h.sym_term(h.sym("replace_part"));
    for (i, rest, v) in edits {
        cx.tick(1)?;
        es[i].1 = if rest.is_empty() {
            v
        } else {
            let pos = if rest.len() == 1 { rest[0] } else { cx.list(&rest) };
            cx.eval(h.app(replace_part, &[es[i].1, h.app(rule, &[pos, v])]))?
        };
    }
    let out: Vec<T<'h>> = es.iter().map(|&(k, v)| h.app(rule, &[k, v])).collect();
    Ok(Some(h.app(h.sym_term(lib.s.assoc), &out)))
}

/// `replace_part(x, p -> v)` where the position `p` is a pattern (decision cov-core.5): a list pattern is matched
/// against the position of every part at every level (`[i_, i_]` is the diagonal of a matrix; a negative index in
/// the pattern counts from the end at the last level), any other pattern against the index of each element
/// (`1 | 3 | 5`, `_`). A part whose position matches is replaced by the rule's right side with the pattern's
/// variables substituted, and its own parts are not visited. `None` when a rule's left side does not compile.
fn replace_matching_parts<'h>(
    lib: &Lib,
    cx: &mut NativeCx<'_, '_, 'h>,
    x: T<'h>,
    r: T<'h>,
    with_heads: Option<bool>,
) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    let rules: Vec<T<'h>> = match cx.as_list(r) {
        Some(xs) => xs.to_vec(),
        None => vec![r],
    };
    let mut compiled = Vec::with_capacity(rules.len());
    let mut heads = Vec::with_capacity(rules.len());
    for &rule in &rules {
        let (p, v) = match h.view(rule) {
            View::App { head, args: [p, v] } if head == h.sym_term(lib.s.rule) || head == h.sym_term(lib.s.rule_delayed) => {
                (*p, *v)
            }
            _ => return Ok(None),
        };
        // a pattern for one index is matched as the one-element position `[p]` (so `i__` matches too)
        // a list pattern, possibly under a condition (`[x_, y_] when is_coprime(x, y)`)
        let conditioned = matches!(h.view(p), View::App { head, args } if args.len() == 2
            && matches!(h.view(head), View::Sym(s) if h.sym_name(s) == "condition")
            && cx.as_list(args[0]).is_some());
        let list = cx.as_list(p).is_some() || conditioned;
        heads.push(with_heads.unwrap_or(list && mentions_zero(h, p)));
        let Some(m) = Matcher::new(lib, h, if list { p } else { cx.list(&[p]) }) else { return Ok(None) };
        compiled.push((m, list, conditioned, v));
    }
    let mut pos = Vec::new();
    Ok(Some(replace_walk(lib, cx, x, &mut pos, &mut compiled, &heads)?))
}

/// Does a rule of `replace_part` with the position pattern compiled in `rule` hit the part at position `pos`
/// (`neg_last`: the last index counted from the end, tried for patterns without a condition)?
fn position_hit<'l, 'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    rule: &mut (Matcher<'l, 'h>, bool, bool, T<'h>),
    pos: &[T<'h>],
    neg_last: Option<T<'h>>,
) -> Res<bool> {
    let (m, list, conditioned, _) = rule;
    if *list {
        if m.matches(cx, cx.list(pos))? {
            return Ok(true);
        }
        if let (false, Some(neg)) = (*conditioned, neg_last) {
            let mut v = pos.to_vec();
            *v.last_mut().expect("a position has an index") = neg;
            return m.matches(cx, cx.list(&v));
        }
        Ok(false)
    } else {
        Ok(pos.len() == 1 && m.matches(cx, cx.list(pos))?)
    }
}

fn replace_walk<'l, 'h>(
    lib: &Lib,
    cx: &mut NativeCx<'_, '_, 'h>,
    x: T<'h>,
    pos: &mut Vec<T<'h>>,
    rules: &mut [(Matcher<'l, 'h>, bool, bool, T<'h>)],
    heads: &[bool],
) -> Res<T<'h>> {
    let h = cx.heap();
    // an association: its entries are at the positions of their keys
    if let Some(es) = crate::assocs::entries(lib, h, x) {
        let rule = h.sym_term(lib.s.rule);
        let mut out = Vec::with_capacity(es.len());
        let mut changed = false;
        for &(key, val) in &es {
            cx.tick(1)?;
            pos.push(key);
            let mut done = None;
            for r in rules.iter_mut() {
                if position_hit(cx, r, pos, None)? {
                    let v = r.0.substitute(cx, r.3)?;
                    done = Some(v);
                    break;
                }
            }
            let y = match done {
                Some(y) => y,
                None => replace_walk(lib, cx, val, pos, rules, heads)?,
            };
            changed |= y != val;
            out.push(h.app(rule, &[key, y]));
            pos.pop();
        }
        return Ok(if changed { h.app(h.sym_term(lib.s.assoc), &out) } else { x });
    }
    let Some((mut head, args)) = args_of(h, x) else { return Ok(x) };
    let n = args.len() as i64;
    let mut out = Vec::with_capacity(args.len());
    let mut changed = false;
    // the head is the part at index 0, visited by a list pattern that names 0 (`[___, 0] -> list`, round 36)
    pos.push(h.int(0));
    for (r, &hd) in rules.iter_mut().zip(heads) {
        if hd && position_hit(cx, r, pos, None)? {
            head = r.0.substitute(cx, r.3)?;
            changed = true;
            break;
        }
    }
    pos.pop();
    for (k, &a) in args.iter().enumerate() {
        cx.tick(1)?;
        pos.push(h.int(k as i64 + 1));
        let mut done = None;
        for r in rules.iter_mut() {
            if position_hit(cx, r, pos, Some(h.int(k as i64 - n)))? {
                done = Some(r.0.substitute(cx, r.3)?);
                break;
            }
        }
        let y = match done {
            Some(y) => y,
            None => replace_walk(lib, cx, a, pos, rules, heads)?,
        };
        changed |= y != a;
        out.push(y);
        pos.pop();
    }
    Ok(if changed { h.app(head, &out) } else { x })
}

/// Does the pattern contain the integer 0 (a position pattern for heads)?
fn mentions_zero(h: &Heap, t: T<'_>) -> bool {
    let mut stack = vec![t];
    while let Some(u) = stack.pop() {
        match h.view(u) {
            View::Int(0) => return true,
            View::App { args, .. } if stack.len() < 10_000 => stack.extend_from_slice(args),
            _ => {}
        }
    }
    false
}

/// `replace_at(x, rules, pos)`: `x` with the part at the position (an index, `[i, j, ...]`, or a list of
/// positions; negative indices count from the end) replaced by `replace(part, rules)`; a position named twice is
/// rewritten twice.
pub(crate) fn replace_at<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [x, rules, spec] = *args else { return Ok(Reply::Decline) };
    let with_all = if crate::assocs::entries(lib, h, x).is_none() { positions_with_all(cx, x, spec) } else { None };
    let Some(ps) = with_all.or_else(|| positions(h, cx, spec)) else { return Ok(Reply::Decline) };
    // an association of replacements is the list of its rules
    let rules = match crate::assocs::entries(lib, h, rules) {
        Some(es) => {
            let rule = h.sym_term(lib.s.rule);
            cx.list(&es.iter().map(|&(k, v)| h.app(rule, &[k, v])).collect::<Vec<_>>())
        }
        None => rules,
    };
    let replace = h.sym_term(h.sym("replace"));
    let mut cur = x;
    for p in ps {
        cx.tick(1)?;
        let Ok(part) = part_at(h, cur, &p) else { return Ok(Reply::Done(lib.bad_index(h, h.int(p[0]), 0))) };
        let v = cx.eval(h.app(replace, &[part, rules]))?;
        match rebuild_at(h, cur, &p, Edit::Set(v)) {
            Ok(Some(u)) => cur = u,
            _ => return Ok(Reply::Decline),
        }
    }
    Ok(settle(lib, h, cur))
}

/// The edit at each of several positions, the deepest and last first so earlier positions stay valid.
fn edit_all<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, x: T<'h>, mut ps: Vec<Vec<i64>>, edit: Edit<'h>) -> Res<Reply<'h>> {
    let h = cx.heap();
    // resolve negative indices against the term before editing, then edit from the back
    let mut abs: Vec<Vec<i64>> = Vec::with_capacity(ps.len());
    for p in ps.drain(..) {
        let mut cur = x;
        let mut q = Vec::with_capacity(p.len());
        for &i in &p {
            let Some((head, args)) = args_of(h, cur) else { return Ok(Reply::Done(lib.bad_index(h, h.int(i), 0))) };
            let Some(k) = index(i, args.len()) else { return Ok(Reply::Done(lib.bad_index(h, h.int(i), args.len()))) };
            q.push(k as i64);
            cur = if k == 0 { head } else { args[k - 1] };
        }
        abs.push(q);
    }
    abs.sort();
    // a position given twice is edited twice by `map_at` (`f(f(part))`), once by the other edits
    if !matches!(edit, Edit::Apply(_)) {
        abs.dedup();
    }
    let mut cur = x;
    for p in abs.iter().rev() {
        cx.tick(1)?;
        match rebuild_at(h, cur, p, edit) {
            Ok(Some(u)) => cur = u,
            Ok(None) => cur = h.sym_term(h.sym("null")),
            Err(i) => return Ok(Reply::Done(lib.bad_index(h, h.int(i), 0))),
        }
    }
    Ok(settle(lib, h, cur))
}

/// `delete(x, pos)`, `delete(x, [pos1, ...])`: `x` without the parts at the positions.
pub(crate) fn delete<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [x, spec] = *args else { return Ok(Reply::Decline) };
    // an empty list of positions deletes nothing (`[[]]` is the position of the whole term)
    if cx.as_list(spec).is_some_and(<[T<'h>]>::is_empty) {
        return Ok(Reply::Done(x));
    }
    if let Some(es) = crate::assocs::entries(lib, h, x) {
        let Some((ps, _)) = assoc_positions(h, cx, &es, spec) else { return Ok(Reply::Decline) };
        let n = es.len() as i64;
        // whole entries: the association without them
        if ps.iter().all(|p| p.len() == 1) {
            let gone: Vec<usize> = ps.iter().filter_map(|p| index(p[0], es.len()).filter(|&k| k > 0).map(|k| k - 1)).collect();
            if gone.len() != ps.len() {
                return Ok(Reply::Done(lib.bad_index(h, h.int(ps[0][0]), n as usize)));
            }
            let rule = h.sym_term(lib.s.rule);
            let kept: Vec<T<'h>> =
                es.iter().enumerate().filter(|(i, _)| !gone.contains(i)).map(|(_, &(k, v))| h.app(rule, &[k, v])).collect();
            return Ok(Reply::Done(h.app(h.sym_term(lib.s.assoc), &kept)));
        }
        return in_values(lib, cx, &es, ps, Edit::Delete);
    }
    let Some(ps) = positions(h, cx, spec) else { return Ok(Reply::Decline) };
    edit_all(lib, cx, x, ps, Edit::Delete)
}

/// An edit inside the values of an association (each position starts with an entry's position).
fn in_values<'h>(
    lib: &Lib,
    cx: &mut NativeCx<'_, '_, 'h>,
    es: &[(T<'h>, T<'h>)],
    ps: Vec<Vec<i64>>,
    edit: Edit<'h>,
) -> Res<Reply<'h>> {
    let h = cx.heap();
    let vs = cx.list(&es.iter().map(|&(_, v)| v).collect::<Vec<_>>());
    let v = match edit_all(lib, cx, vs, ps, edit)? {
        Reply::Done(v) => v,
        Reply::Again(u) => cx.eval(u)?,
        Reply::Decline => return Ok(Reply::Decline),
    };
    Ok(match cx.as_list(v) {
        Some(ys) if ys.len() == es.len() => Reply::Again(rekeyed(lib, h, es, ys)),
        _ => Reply::Done(v),
    })
}

/// `map_at(f, x, pos)`, `map_at(f, x, [pos1, ...])`: `x` with each part `p` at the positions replaced by `f(p)`.
/// The positions one position with `all` among its indices names in `x` (`[all, 2]`: the second part of every
/// part; `all` alone: every part); `None` without an `all`, or when an index lies outside a part.
fn positions_with_all<'h>(cx: &NativeCx<'_, '_, 'h>, x: T<'h>, spec: T<'h>) -> Option<Vec<Vec<i64>>> {
    let h = cx.heap();
    let is_all = |t: T<'h>| matches!(h.view(t), View::Sym(s) if h.sym_name(s) == "all");
    let items: Vec<T<'h>> = if is_all(spec) { vec![spec] } else { cx.as_list(spec)?.to_vec() };
    if !items.iter().any(|&i| is_all(i)) || !items.iter().all(|&i| is_all(i) || matches!(h.view(i), View::Int(_))) {
        return None;
    }
    let mut out = vec![];
    let mut todo: Vec<(T<'h>, Vec<i64>)> = vec![(x, vec![])];
    while let Some((u, pos)) = todo.pop() {
        let k = pos.len();
        if k == items.len() {
            out.push(pos);
            continue;
        }
        let (_, args) = args_of(h, u)?;
        let n = args.len() as i64;
        let picks: Vec<i64> = match h.view(items[k]) {
            View::Int(i) => {
                let j = if i < 0 { n + 1 + i } else { i };
                if !(1..=n).contains(&j) {
                    return None;
                }
                vec![j]
            }
            _ => (1..=n).collect(),
        };
        for j in picks.into_iter().rev() {
            let mut p = pos.clone();
            p.push(j);
            todo.push((args[j as usize - 1], p));
        }
    }
    Some(out)
}

pub(crate) fn map_at<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [f, x, spec] = *args else { return Ok(Reply::Decline) };
    if let Some(ps) = positions_with_all(cx, x, spec) {
        if crate::assocs::entries(lib, h, x).is_none() {
            return edit_all(lib, cx, x, ps, Edit::Apply(f));
        }
    }
    // an empty list of positions edits nothing (`[[]]` is the position of the whole term)
    if cx.as_list(spec).is_some_and(<[T<'h>]>::is_empty) {
        return Ok(Reply::Done(x));
    }
    if let Some(es) = crate::assocs::entries(lib, h, x) {
        let Some((ps, _)) = assoc_positions(h, cx, &es, spec) else { return Ok(Reply::Decline) };
        return in_values(lib, cx, &es, ps, Edit::Apply(f));
    }
    let Some(ps) = positions(h, cx, spec) else { return Ok(Reply::Decline) };
    edit_all(lib, cx, x, ps, Edit::Apply(f))
}

/// `level(x, levels)`: the list of the parts of `x` at the levels, in the order a depth-first walk finishes them.
/// With `heads: true` the heads are parts too (each one level below its call, visited before the arguments).
pub(crate) fn level<'h>(_lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let (args, heads) = crate::ctx::option(h, args, 2, "heads");
    let [x, spec] = *args else { return Ok(Reply::Decline) };
    let Some(levels) = level_spec(h, spec) else { return Ok(Reply::Decline) };
    if heads == Some(h.sym_term(h.sym("true"))) {
        let mut out = vec![];
        if crate::levels::with_heads(cx, x, 0, levels, &mut out)?.is_none() {
            return Ok(Reply::Decline);
        }
        return Ok(Reply::Done(cx.list(&out)));
    }
    if heads.is_some_and(|v| v != h.sym_term(h.sym("false"))) {
        return Ok(Reply::Decline);
    }
    let mut out = vec![];
    let mut visit = |_: &mut NativeCx<'_, '_, 'h>, u: T<'h>, _: &[i64]| -> Res<Option<T<'h>>> {
        out.push(u);
        Ok(None)
    };
    if walk(cx, x, levels, &mut visit)?.is_none() {
        return Ok(Reply::Decline);
    }
    Ok(Reply::Done(cx.list(&out)))
}

/// The rules of `replace`: one `p -> r` or a list; each compiled.
fn matchers<'l, 'h>(lib: &'l Lib, cx: &NativeCx<'_, '_, 'h>, r: T<'h>) -> Option<Vec<(Matcher<'l, 'h>, T<'h>)>> {
    let h = cx.heap();
    let one = |t: T<'h>| match h.view(t) {
        View::App { head, args: [p, v] } if head == h.sym_term(lib.s.rule) || head == h.sym_term(lib.s.rule_delayed) => {
            Some((Matcher::new(lib, h, *p)?, *v))
        }
        _ => None,
    };
    match cx.as_list(r) {
        Some(xs) => xs.iter().map(|&x| one(x)).collect(),
        None => one(r).map(|m| vec![m]),
    }
}

/// `replace(x, rules)`: the first rule matching the whole of `x` applied (else `x`); `replace(x, rules, levels)`:
/// applied to each part at the levels, inner parts first.
pub(crate) fn replace<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let (x, r, levels) = match *args {
        [x, r] => (x, r, Levels::only(0)),
        [x, r, spec] => match level_spec(h, spec) {
            Some(l) => (x, r, l),
            None => return Ok(Reply::Decline),
        },
        _ => return Ok(Reply::Decline),
    };
    // a list of rule lists (`solve`'s solutions): one result per list, as `rewrite` (decision io-core.2)
    if let Some(sets) = cx.as_list(r).filter(|rs| !rs.is_empty() && rs.iter().all(|&s| cx.as_list(s).is_some())) {
        let calls: Vec<T<'h>> = sets
            .iter()
            .map(|&set| {
                let mut a = args.to_vec();
                a[1] = set;
                h.app(h.sym_term(h.sym("replace")), &a)
            })
            .collect();
        cx.tick(calls.len() as u64)?;
        return Ok(Reply::Again(cx.list(&calls)));
    }
    let Some(mut ms) = matchers(lib, cx, r) else { return Ok(Reply::Done(lib.bad_arg(h, t, 2))) };
    let mut visit = |cx: &mut NativeCx<'_, '_, 'h>, u: T<'h>, _: &[i64]| -> Res<Option<T<'h>>> {
        for (m, rhs) in ms.iter_mut() {
            if m.matches(cx, u)? {
                let v = m.substitute(cx, *rhs)?;
                return Ok(Some(cx.eval(v)?));
            }
        }
        Ok(None)
    };
    Ok(match walk(cx, x, levels, &mut visit)? {
        Some(v) => settle(lib, h, v),
        None => Reply::Decline,
    })
}

/// `delete_cases(xs, p)`: the elements of `xs` that do not match `p`; `delete_cases(x, p, levels)`: the parts at
/// the levels removed where they match.
pub(crate) fn delete_cases<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let (x, p, levels) = match *args {
        [x, p] => (x, p, None),
        [x, p, spec] => match level_spec(h, spec) {
            Some(l) => (x, p, Some(l)),
            None => return Ok(Reply::Decline),
        },
        _ => return Ok(Reply::Decline),
    };
    let Some(mut m) = Matcher::new(lib, h, p) else { return Ok(Reply::Done(lib.bad_arg(h, t, 2))) };
    let Some(levels) = levels else {
        let (head, xs) = take!(lib.operand(h, t, x, 1));
        let mut out = Vec::with_capacity(xs.len());
        for &y in xs {
            if !m.matches(cx, y)? {
                out.push(y);
            }
        }
        return Ok(lib.rebuilt(h, head, &out));
    };
    // matching parts become the deletion marker, which the walk drops from their parent
    let gone = crate::levels::gone(h);
    let mut visit = |cx: &mut NativeCx<'_, '_, 'h>, u: T<'h>, pos: &[i64]| -> Res<Option<T<'h>>> {
        Ok((!pos.is_empty() && m.matches(cx, u)?).then_some(gone))
    };
    Ok(match walk(cx, x, levels, &mut visit)? {
        Some(v) => settle(lib, h, v),
        None => Reply::Decline,
    })
}

/// `first_case(xs, p)`, `first_case(xs, p, default)`: the first element matching `p` (`p -> r`: its `r`), else
/// `default` or `failure(not_found, p)`.
pub(crate) fn first_case<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    // `heads: true`: the head counts as a part (the first case of `cases(x, p, levels, heads: true)`)
    let (args, heads) = crate::ctx::option(h, args, 2, "heads");
    if heads.is_some_and(|v| v == cx.bool(true)) {
        let (x, p, d, spec) = match *args {
            [x, p] => (x, p, None, h.int(1)),
            [x, p, d] => (x, p, Some(d), h.int(1)),
            [x, p, d, s] => (x, p, Some(d), s),
            _ => return Ok(Reply::Decline),
        };
        let opts = h.call(h.sym("options"), &[h.call(h.sym("rule"), &[h.sym_term(h.sym("heads")), cx.bool(true)])]);
        let found = cx.eval(h.call(h.sym("cases"), &[x, p, spec, h.int(1), opts]))?;
        return Ok(match cx.as_list(found) {
            Some(&[first, ..]) => Reply::Done(first),
            Some(&[]) => Reply::Done(match d {
                Some(d) => d,
                None => lib.fail(h, h.sym("not_found"), &[p]),
            }),
            _ => Reply::Decline,
        });
    }
    let (xs, p, default) = match *args {
        [xs, p] => (xs, p, None),
        [xs, p, d] => (xs, p, Some(d)),
        // `first_case(x, p, d, levels)`: the first match at the levels (decision io-core.11)
        [x, p, d, spec] => {
            let Some(levels) = crate::levels::level_spec(h, spec) else { return Ok(Reply::Decline) };
            return Ok(match crate::select::cases_at(lib, cx, t, x, p, levels, 1)? {
                Ok(found) => Reply::Done(found.first().copied().unwrap_or(d)),
                Err(r) => r,
            });
        }
        _ => return Ok(Reply::Decline),
    };
    let (_, xs) = take!(lib.operand(h, t, xs, 1));
    let (pat, rhs) = match h.view(p) {
        View::App { head, args: [l, r] } if head == h.sym_term(lib.s.rule) || head == h.sym_term(lib.s.rule_delayed) => {
            (*l, Some(*r))
        }
        _ => (p, None),
    };
    let Some(mut m) = Matcher::new(lib, h, pat) else { return Ok(Reply::Done(lib.bad_arg(h, t, 2))) };
    for &x in xs {
        if m.matches(cx, x)? {
            return Ok(match rhs {
                None => Reply::Done(x),
                Some(r) => Reply::Again(m.substitute(cx, r)?),
            });
        }
    }
    Ok(Reply::Done(match default {
        Some(d) => d,
        None => lib.fail(h, h.sym("not_found"), &[p]),
    }))
}

/// `reverse_sort(xs)`: the elements in reverse canonical order.
pub(crate) fn reverse_sort<'h>(_lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    // `reverse_sort(xs, p)`: `sort(xs, p)` reversed (decision io-core.13)
    let sorted = match *args {
        [xs] => h.app(h.sym_term(h.sym("sort")), &[xs]),
        [xs, p] => h.app(h.sym_term(h.sym("sort")), &[xs, p]),
        _ => return Ok(Reply::Decline),
    };
    Ok(Reply::Again(h.app(h.sym_term(h.sym("reverse")), &[sorted])))
}

/// `take_largest(xs, n)`: the `n` largest elements, largest first.
pub(crate) fn take_largest<'h>(_lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    take_extreme(cx, args, true)
}

/// `take_smallest(xs, n)`: the `n` smallest elements, smallest first.
pub(crate) fn take_smallest<'h>(_lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    take_extreme(cx, args, false)
}

fn take_extreme<'h>(cx: &mut NativeCx<'_, '_, 'h>, args: &[T<'h>], largest: bool) -> Res<Reply<'h>> {
    let h = cx.heap();
    // `take_largest(xs, n, p)`: by the ordering function p (as `ordering(xs, all, p)` orders), the last n in
    // reverse (the first n for `take_smallest`); `up_to(n)` as below
    if let [xs, n, p] = *args {
        if !matches!(h.view(p), View::App { head, .. } if matches!(h.view(head), View::Sym(s) if h.sym_name(s) == "options")) {
            let Some(items) = cx.as_list(xs) else { return Ok(Reply::Decline) };
            let len = items.len() as i64;
            let k = match h.view(n) {
                View::Int(k) if (0..=len).contains(&k) => k,
                _ => match crate::ctx::up_to_arg(h, n).map(|k| h.view(k)) {
                    Some(View::Int(k)) if k >= 0 => k.min(len),
                    _ => return Ok(Reply::Decline),
                },
            };
            let order = h.call(h.sym("ordering"), &[xs, h.sym_term(h.sym("all")), p]);
            let picked = if largest {
                h.call(h.sym("reverse"), &[h.call(h.sym("take"), &[order, h.int(-k)])])
            } else {
                h.call(h.sym("take"), &[order, h.int(k)])
            };
            return Ok(Reply::Again(h.call(h.sym("part"), &[xs, picked])));
        }
    }
    // `excluded_forms: [...]`: elements left out before ordering; by default `none`, `indeterminate` and
    // `missing(...)` (round 17)
    let (args, excluded) = crate::ctx::option(h, args, 2, "excluded_forms");
    let [xs, n] = *args else { return Ok(Reply::Decline) };
    let excluded: Option<Vec<T<'h>>> = match excluded {
        None => None,
        Some(e) => match cx.as_list(e) {
            Some(es) => Some(es.to_vec()),
            None => Some(vec![e]),
        },
    };
    let missing = h.sym_term(h.sym("missing"));
    let dropped = |x: T<'h>| match &excluded {
        Some(es) => es.contains(&x),
        None => {
            matches!(h.view(x), View::Sym(s) if matches!(h.sym_name(s).as_str(), "none" | "indeterminate"))
                || matches!(h.view(x), View::App { head, .. } if head == missing)
        }
    };
    let xs = match cx.as_list(xs) {
        Some(es) if es.iter().any(|&x| dropped(x)) => {
            let kept: Vec<T<'h>> = es.iter().copied().filter(|&x| !dropped(x)).collect();
            cx.list(&kept)
        }
        _ => xs,
    };
    let count = match h.view(n) {
        View::Int(k @ 0..) => k,
        _ => match crate::more::up_to_count(h, n, usize::MAX) {
            // `up_to(n)`: at most the number of elements
            Some(k) => k.min(cx.as_list(xs).map_or(0, |v| v.len() as i64)),
            None => return Ok(Reply::Decline),
        },
    };
    let n = h.int(count);
    // `take_largest(xs -> prop, n)`: "Element", "Index", a list of them, or `all` (an association of both)
    let (xs, prop) = match h.view(xs) {
        View::App { head, args: [l, p] } if head == h.sym_term(h.sym("rule")) => (*l, Some(*p)),
        _ => (xs, None),
    };
    let sym = |s: &str| h.sym_term(h.sym(s));
    if let Some(elems) = cx.as_list(xs) {
        if let Some(mut ix) = by_value(cx, elems, largest)? {
            if count as usize > ix.len() {
                return Ok(Reply::Decline);
            }
            ix.truncate(count as usize);
            let shown = |cx: &mut NativeCx<'_, '_, 'h>, i: usize, p: T<'h>| -> Option<T<'h>> {
                match h.view(p) {
                    View::Str(s) if s == "Element" => Some(elems[i]),
                    View::Str(s) if s == "Index" => Some(h.int(i as i64 + 1)),
                    View::Sym(s) if h.sym_name(s) == "all" => {
                        let rule = h.sym_term(h.sym("rule"));
                        let e = h.app(rule, &[h.string("Element"), elems[i]]);
                        let k = h.app(rule, &[h.string("Index"), h.int(i as i64 + 1)]);
                        Some(h.app(sym("assoc"), &[e, k]))
                    }
                    _ => {
                        let ps = cx.as_list(p)?;
                        let mut out = Vec::with_capacity(ps.len());
                        for &q in ps {
                            match h.view(q) {
                                View::Str(s) if s == "Element" => out.push(elems[i]),
                                View::Str(s) if s == "Index" => out.push(h.int(i as i64 + 1)),
                                _ => return None,
                            }
                        }
                        Some(cx.list(&out))
                    }
                }
            };
            let mut out = Vec::with_capacity(ix.len());
            for &i in &ix {
                match prop {
                    None => out.push(elems[i]),
                    Some(p) => match shown(cx, i, p) {
                        Some(v) => out.push(v),
                        None => return Ok(Reply::Decline),
                    },
                }
            }
            return Ok(Reply::Again(cx.list(&out)));
        }
    }
    if prop.is_some() {
        return Ok(Reply::Decline);
    }
    let mut sorted = h.app(sym("sort"), &[xs]);
    if largest {
        sorted = h.app(sym("reverse"), &[sorted]);
    }
    Ok(Reply::Again(h.app(sym("take"), &[sorted, n])))
}

/// The positions of `xs` in increasing order of value (stable), when every element is a real number or a closed
/// numeric expression with a machine value (`exp(pi)`, `7*pi`: numerical order, decision cov-core.6) or a
/// number-like value such as a quantity (`crate::sort::value_order`). `None` otherwise.
fn by_value<'h>(cx: &mut NativeCx<'_, '_, 'h>, xs: &[T<'h>], largest: bool) -> Res<Option<Vec<usize>>> {
    let h = cx.heap();
    if xs.is_empty() || xs.len() > 100_000 {
        return Ok(None);
    }
    if let Some(mut ix) = crate::sort::value_order(cx, xs)? {
        if largest {
            ix.reverse();
        }
        return Ok(Some(ix));
    }
    let n = h.sym_term(h.sym("n"));
    let inf = h.sym_term(h.sym("inf"));
    let minus_inf = h.call(h.sym("mul"), &[h.int(-1), inf]);
    let mut vals = Vec::with_capacity(xs.len());
    for &x in xs {
        cx.tick(1)?;
        let v = match h.view(x) {
            View::Int(v) => v as f64,
            View::Real(v) => v,
            View::Str(_) => return Ok(None),
            // the infinities order with the reals
            _ if x == inf => f64::INFINITY,
            _ if x == minus_inf => f64::NEG_INFINITY,
            _ => match h.view(cx.eval(h.app(n, &[x]))?) {
                View::Real(v) => v,
                View::Int(v) => v as f64,
                _ => return Ok(None),
            },
        };
        if v.is_nan() {
            return Ok(None);
        }
        vals.push(v);
    }
    // stable both ways: equal values keep their order
    let mut ix: Vec<usize> = (0..xs.len()).collect();
    if largest {
        ix.sort_by(|&a, &b| vals[b].total_cmp(&vals[a]));
    } else {
        ix.sort_by(|&a, &b| vals[a].total_cmp(&vals[b]));
    }
    Ok(Some(ix))
}

/// `take_largest_by(xs, f, n)`: the `n` elements with the largest `f(x)` (numerical order; equal values in
/// order); `take_smallest_by(xs, f, n)`; `n` may be `up_to(n)`. An association keeps the chosen entries.
pub(crate) fn take_largest_by<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    take_extreme_by(lib, cx, t, args, true)
}

/// `take_smallest_by(xs, f, n)`: the `n` elements with the smallest `f(x)`.
pub(crate) fn take_smallest_by<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    take_extreme_by(lib, cx, t, args, false)
}

fn take_extreme_by<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>], largest: bool) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [x, f, n] = *args else { return Ok(Reply::Decline) };
    // `xs -> prop`: the property of each chosen element instead ("Element", "Index", "Value", a list of them, or
    // all for an association of the three; P9B.5)
    let (x, prop) = match h.view(x) {
        View::App { head, args: &[l, p] } if head == h.sym_term(lib.s.rule) && cx.as_list(l).is_some() => (l, Some(p)),
        _ => (x, None),
    };
    let es = crate::assocs::entries(lib, h, x);
    let elems: Vec<T<'h>> = match &es {
        Some(es) => es.iter().map(|&(_, v)| v).collect(),
        None => take!(lib.list_operand(h, t, x, 1)).to_vec(),
    };
    let count = match h.view(n) {
        View::Int(k @ 0..) => k as usize,
        _ => match crate::more::up_to_count(h, n, elems.len()) {
            Some(k) => k as usize,
            None => return Ok(Reply::Decline),
        },
    };
    if count > elems.len() {
        return Ok(Reply::Decline);
    }
    let mut keys = Vec::with_capacity(elems.len());
    for &e in &elems {
        cx.tick(1)?;
        keys.push(cx.eval(h.app(f, &[e]))?);
    }
    let Some(mut ix) = by_value(cx, &keys, largest)? else { return Ok(Reply::Decline) };
    ix.truncate(count);
    if let Some(p) = prop {
        let one = |i: usize, q: T<'h>| -> Option<T<'h>> {
            match h.view(q) {
                View::Str(s) if s == "Element" => Some(elems[i]),
                View::Str(s) if s == "Index" => Some(h.int(i as i64 + 1)),
                View::Str(s) if s == "Value" => Some(keys[i]),
                _ => None,
            }
        };
        let all = matches!(h.view(p), View::Sym(s) if h.sym_name(s) == "all");
        let mut out = Vec::with_capacity(ix.len());
        for &i in &ix {
            let v = if all {
                let r = |k: &str, v: T<'h>| lib.rule(h, h.string(k), v);
                Some(h.app(h.sym_term(lib.s.assoc), &[r("Element", elems[i]), r("Index", h.int(i as i64 + 1)), r("Value", keys[i])]))
            } else {
                match cx.as_list(p) {
                    Some(qs) => qs.iter().map(|&q| one(i, q)).collect::<Option<Vec<_>>>().map(|v| cx.list(&v)),
                    None => one(i, p),
                }
            };
            match v {
                Some(v) => out.push(v),
                None => return Ok(Reply::Done(lib.bad_arg(h, t, 1))),
            }
        }
        return Ok(Reply::Again(cx.list(&out)));
    }
    Ok(match es {
        Some(es) => {
            let rule = h.sym_term(lib.s.rule);
            let out: Vec<T<'h>> = ix.iter().map(|&i| h.app(rule, &[es[i].0, es[i].1])).collect();
            Reply::Done(h.app(h.sym_term(lib.s.assoc), &out))
        }
        None => Reply::Done(cx.list(&ix.iter().map(|&i| elems[i]).collect::<Vec<_>>())),
    })
}

/// `full_form(x)`: `x`, shown with every part as a plain call (`add(x, pow(y, 2))`); `input_form(x)`: `x` shown in
/// the input syntax. Both stay as wrappers around their (evaluated) argument; the printer reads them.
pub(crate) fn display_wrapper<'h>(_lib: &Lib, _cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, _args: &[T<'h>]) -> Res<Reply<'h>> {
    Ok(Reply::Decline)
}

/// A run `start..end` of a list and the rule's right side for it.
type Run<'h> = (usize, usize, Option<T<'h>>);

/// Which runs a sequence function reports (option `overlaps`, C9B.8).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Overlaps {
    /// runs that do not overlap, scanning from the left (each the longest at its start)
    None,
    /// at most one run starting at each position, the longest
    Starts,
    /// every run that matches, at each start the longer ones first
    All,
}

/// The settings of a sequence function: the pattern argument and the trailing `n` and `overlaps: ...`.
struct SeqArgs<'h> {
    xs: &'h [T<'h>],
    p: T<'h>,
    limit: usize,
    overlaps: Overlaps,
}

/// `(xs, p)`, `(xs, p, n)`, each with an optional `overlaps:` option (`true`, `false`, `all`); `None` when the
/// arguments have another shape. `default` is the function's own setting.
fn seq_args<'h>(lib: &Lib, h: &'h Heap, t: T<'h>, args: &[T<'h>], default: Overlaps) -> Option<Result<SeqArgs<'h>, Reply<'h>>> {
    let (args, ov) = crate::ctx::option(h, args, 2, "overlaps");
    let overlaps = match ov.map(|v| h.view(v)) {
        None => default,
        Some(View::Sym(s)) if h.sym_name(s) == "true" => Overlaps::Starts,
        Some(View::Sym(s)) if h.sym_name(s) == "false" => Overlaps::None,
        Some(View::Sym(s)) if h.sym_name(s) == "all" => Overlaps::All,
        Some(_) => return Some(Err(Reply::Done(lib.bad_arg(h, t, args.len() + 1)))),
    };
    let (xs, p, limit) = match *args {
        [xs, p] => (xs, p, usize::MAX),
        [xs, p, n] => match h.view(n) {
            View::Int(n) if n >= 0 => (xs, p, n as usize),
            View::Sym(s) if h.sym_name(s) == "inf" => (xs, p, usize::MAX),
            _ => return Some(Err(Reply::Done(lib.bad_arg(h, t, 3)))),
        },
        _ => return None,
    };
    match lib.list_operand(h, t, xs, 1) {
        Ok(xs) => Some(Ok(SeqArgs { xs, p, limit, overlaps })),
        Err(e) => Some(Err(e)),
    }
}

/// The runs of `xs` that match the list pattern `p` (`[a, b]`, `[x_, x_]`, `[__, 0]`; `p -> r` gives each run's
/// right side; a list of such rules tries them in order at each start, the first rule that matches winning),
/// scanning from the left; at each start the longest run first. `None` when a pattern does not compile. Runs are
/// tried up to 4096 elements long; at most `limit` runs.
fn sequence_runs<'h>(
    lib: &Lib,
    cx: &mut NativeCx<'_, '_, 'h>,
    xs: &[T<'h>],
    p: T<'h>,
    overlaps: Overlaps,
    limit: usize,
) -> Res<Option<Vec<Run<'h>>>> {
    let h = cx.heap();
    let split = |q: T<'h>| match h.view(q) {
        View::App { head, args: [l, r] } if head == h.sym_term(lib.s.rule) || head == h.sym_term(lib.s.rule_delayed) => (*l, Some(*r)),
        _ => (q, None),
    };
    // a list with a rule whose other elements are rules or list patterns is a list of alternatives (a plain list
    // pattern among them matches without a right side), not one list pattern
    let alts: Vec<(T<'h>, Option<T<'h>>)> = match cx.as_list(p) {
        Some(qs)
            if !qs.is_empty()
                && qs.iter().any(|&q| split(q).1.is_some())
                && qs.iter().all(|&q| split(q).1.is_some() || cx.as_list(q).is_some()) =>
        {
            qs.iter().map(|&q| split(q)).collect()
        }
        _ => vec![split(p)],
    };
    // a list of plain integers and strings (digit strings of a million entries): compared in place, no run built
    if let [(pat, None)] = alts.as_slice() {
        if let Some(lits) = cx.as_list(*pat) {
            if !lits.is_empty() && lits.iter().all(|&q| matches!(h.view(q), View::Int(_) | View::Str(_))) {
                let k = lits.len();
                let mut out = vec![];
                let mut i = 0;
                while i + k <= xs.len() && out.len() < limit {
                    // first element first: most starts fail at once
                    if xs[i] == lits[0] && xs[i + 1..i + k] == lits[1..] {
                        out.push((i, i + k, None));
                        i += if overlaps == Overlaps::None { k } else { 1 };
                    } else {
                        i += 1;
                    }
                    if i % 65536 == 0 {
                        cx.tick(64)?;
                    }
                }
                return Ok(Some(out));
            }
        }
    }
    let mut ms = Vec::with_capacity(alts.len());
    for &(pat, rhs) in &alts {
        let Some(m) = Matcher::new(lib, h, pat) else { return Ok(None) };
        ms.push((m, rhs, run_lengths(cx, pat)));
    }
    let mut out = vec![];
    let mut i = 0;
    while i < xs.len() && out.len() < limit {
        let top = xs.len().min(i + 4096);
        let mut next = i + 1;
        for (m, rhs, (lo, hi)) in ms.iter_mut() {
            let mut any = false;
            // only the run lengths the list pattern can match (a run of three single elements is three long)
            let (first, last) = (i + (*lo).max(1), top.min(i.saturating_add(*hi)));
            for j in (first..=last).rev() {
                cx.tick(1)?;
                let run = cx.list(&xs[i..j]);
                if m.matches(cx, run)? {
                    let r = match rhs {
                        Some(r) => Some(m.substitute(cx, *r)?),
                        None => None,
                    };
                    out.push((i, j, r));
                    any = true;
                    if overlaps == Overlaps::None {
                        next = j;
                    }
                    if overlaps != Overlaps::All || out.len() >= limit {
                        break;
                    }
                }
            }
            if any {
                break;
            }
        }
        i = next;
    }
    Ok(Some(out))
}

/// The shortest and longest runs a list pattern `[p1, p2, ...]` can match (`usize::MAX`: no bound): each element
/// matches one element (a blank, a typed or named or conditioned single blank, a literal), a run (`__`: one or
/// more, `___`: none or more, `repeated(p, ...)`: as many copies of `p` as its spec allows), or none or one
/// (`optional`); `alternatives` the widest bounds of its choices. Anything that is not a list, or a pattern head
/// not listed, gives no bound.
fn run_lengths<'h>(cx: &NativeCx<'_, '_, 'h>, p: T<'h>) -> (usize, usize) {
    const INF: usize = usize::MAX;
    fn one<'h>(cx: &NativeCx<'_, '_, 'h>, q: T<'h>) -> (usize, usize) {
        let h = cx.heap();
        let View::App { head, args } = h.view(q) else { return (1, 1) };
        let View::Sym(s) = h.view(head) else { return (1, 1) };
        let mul = |a: usize, k: usize| a.saturating_mul(k);
        match (h.sym_name(s).as_str(), args) {
            ("any", _) => (1, 1),
            ("seq1", _) => (1, INF),
            ("seq0", _) => (0, INF),
            ("typed" | "condition", [p, _]) => one(cx, *p),
            ("named", [_, p]) => one(cx, *p),
            ("alternatives", qs) if !qs.is_empty() => {
                let bs: Vec<(usize, usize)> = qs.iter().map(|&a| one(cx, a)).collect();
                (bs.iter().map(|b| b.0).min().unwrap_or(0), bs.iter().map(|b| b.1).max().unwrap_or(INF))
            }
            ("repeated", [p]) => (one(cx, *p).0, INF),
            ("repeated", [p, spec]) => {
                let (a, b) = one(cx, *p);
                let int = |t: T<'h>| match h.view(t) {
                    View::Int(k) if k >= 0 => Some(k as usize),
                    View::Sym(s) if h.sym_name(s) == "inf" => Some(INF),
                    _ => None,
                };
                match (int(*spec), cx.as_list(*spec)) {
                    (Some(n), _) => (a, mul(b, n)),
                    (None, Some(&[n])) => match int(n) {
                        Some(n) => (mul(a, n), mul(b, n)),
                        None => (0, INF),
                    },
                    (None, Some(&[m, n])) => match (int(m), int(n)) {
                        (Some(m), Some(n)) => (mul(a, m), mul(b, n)),
                        _ => (0, INF),
                    },
                    _ => (0, INF),
                }
            }
            ("optional", [p, ..]) => (0, one(cx, *p).1),
            // other pattern constructs: no bound
            (n, _) if matches!(n, "held" | "longest" | "shortest" | "key_value_pattern" | "verbatim" | "hold_pattern") => (0, INF),
            // a call that is no pattern construct matches one element
            _ => (1, 1),
        }
    }
    let Some(items) = cx.as_list(p) else { return (0, INF) };
    items.iter().fold((0usize, 0usize), |(lo, hi), &q| {
        let (a, b) = one(cx, q);
        (lo.saturating_add(a), hi.saturating_add(b))
    })
}

macro_rules! seq_args {
    ($lib:expr, $h:expr, $t:expr, $args:expr, $default:expr) => {
        match seq_args($lib, $h, $t, $args, $default) {
            None => return Ok(Reply::Decline),
            Some(Err(r)) => return Ok(r),
            Some(Ok(a)) => a,
        }
    };
}

/// `sequence_cases(xs, p)`, `sequence_cases(xs, p, n)`: the runs of `xs` matching the list pattern `p` (`p -> r`:
/// each run's `r`), at most `n`; runs do not overlap unless `overlaps: true` (one a start) or `all`.
pub(crate) fn sequence_cases<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let a = seq_args!(lib, h, t, args, Overlaps::None);
    let Some(runs) = sequence_runs(lib, cx, a.xs, a.p, a.overlaps, a.limit)? else { return Ok(Reply::Done(lib.bad_arg(h, t, 2))) };
    let out: Vec<T<'h>> = runs.into_iter().map(|(i, j, r)| r.unwrap_or_else(|| cx.list(&a.xs[i..j]))).collect();
    Ok(Reply::Again(cx.list(&out)))
}

/// `sequence_position(xs, p)`: the `[start, end]` positions of the runs of `xs` matching the list pattern `p`
/// (overlapping runs too, the longest at each start; `overlaps: false` or `all` to change it).
pub(crate) fn sequence_position<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let a = seq_args!(lib, h, t, args, Overlaps::Starts);
    let Some(runs) = sequence_runs(lib, cx, a.xs, a.p, a.overlaps, a.limit)? else { return Ok(Reply::Done(lib.bad_arg(h, t, 2))) };
    let out: Vec<T<'h>> = runs.into_iter().map(|(i, j, _)| cx.list(&[h.int(i as i64 + 1), h.int(j as i64)])).collect();
    Ok(Reply::Done(cx.list(&out)))
}

/// `sequence_count(xs, p)`: how many runs of `xs` match the list pattern `p` (not overlapping, unless
/// `overlaps: true` or `all`).
pub(crate) fn sequence_count<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let a = seq_args!(lib, h, t, args, Overlaps::None);
    let Some(runs) = sequence_runs(lib, cx, a.xs, a.p, a.overlaps, a.limit)? else { return Ok(Reply::Done(lib.bad_arg(h, t, 2))) };
    Ok(Reply::Done(h.int(runs.len() as i64)))
}

/// `sequence_replace(xs, p -> r)`, `sequence_replace(xs, [rules], n)`: each run of `xs` matching a rule's list
/// pattern replaced by the one element `r` (at most `n` runs).
pub(crate) fn sequence_replace<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let a = seq_args!(lib, h, t, args, Overlaps::None);
    let xs = a.xs;
    let Some(runs) = sequence_runs(lib, cx, xs, a.p, Overlaps::None, a.limit)? else { return Ok(Reply::Done(lib.bad_arg(h, t, 2))) };
    if runs.iter().any(|r| r.2.is_none()) {
        return Ok(Reply::Done(lib.bad_arg(h, t, 2)));
    }
    let mut out = Vec::with_capacity(xs.len());
    let mut k = 0;
    for (i, j, r) in runs {
        out.extend_from_slice(&xs[k..i]);
        out.extend(r);
        k = j;
    }
    out.extend_from_slice(&xs[k..]);
    Ok(Reply::Again(cx.list(&out)))
}

/// `sequence_split(xs, p)`, `sequence_split(xs, p, n)`: the pieces of `xs` between the runs matching the list
/// pattern `p` (empty pieces left out; at most `n` pieces: the first `n - 1` runs split); with `p -> r` (or a list of rules), each run's `r`
/// stays between the pieces.
pub(crate) fn sequence_split<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let a = seq_args!(lib, h, t, args, Overlaps::None);
    let xs = a.xs;
    // n pieces at most: n - 1 runs split
    let Some(runs) = sequence_runs(lib, cx, xs, a.p, Overlaps::None, a.limit.saturating_sub(1))? else { return Ok(Reply::Done(lib.bad_arg(h, t, 2))) };
    let mut out = vec![];
    let mut k = 0;
    for (i, j, r) in runs {
        if i > k {
            out.push(cx.list(&xs[k..i]));
        }
        out.extend(r);
        k = j;
    }
    if k < xs.len() {
        out.push(cx.list(&xs[k..]));
    }
    Ok(Reply::Again(cx.list(&out)))
}
