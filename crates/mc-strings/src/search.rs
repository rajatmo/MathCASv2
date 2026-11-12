//! Searching strings: `string_split`, `string_replace`, `string_contains`, `string_starts`, `string_ends`.
//!
//! Search strings are literal (no patterns). Several search strings are tried at each position in the order given,
//! the leftmost occurrence wins and occurrences do not overlap (decision S.3).
use crate::ctx::{charge, take, too_large, Lib, MAX_BYTES};
use crate::scan;
use mc_core::{Store, View, T};
use mc_eval::{NativeCx, Reply, Res};

/// Bytes scanned between two polls of the interrupt.
const POLL: usize = 1 << 16;

/// The next occurrence of one of `needles` (nonempty) in `s` at or after byte `from`: its byte position and the
/// index of the needle (the first in order that matches at the leftmost position).
fn next_match(cx: &mut NativeCx<'_, '_, '_>, s: &str, from: usize, needles: &[&str]) -> Res<Option<(usize, usize)>> {
    if let [one] = needles {
        charge(cx, s.len() - from)?;
        return Ok(s[from..].find(one).map(|p| (from + p, 0)));
    }
    let mut scanned = 0;
    for (i, _) in s[from..].char_indices() {
        scanned += needles.len();
        if scanned >= POLL {
            scanned = 0;
            charge(cx, POLL)?;
        }
        let rest = &s[from + i..];
        if let Some(k) = needles.iter().position(|n| rest.starts_with(n)) {
            return Ok(Some((from + i, k)));
        }
    }
    Ok(None)
}

/// `string_split(s)`: the words of s (runs of characters that are not white space); `string_split(s, sep)` with a
/// string or a list of strings: the pieces between occurrences, empty pieces included (k occurrences give k + 1
/// pieces); `string_split(s, sep, n)`: at most n pieces, the last holding the rest.
pub(crate) fn string_split<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    if args.len() > 1 && !literal(lib, h, args[1], false) || has_options(lib, h, args) {
        return scan::split(lib, cx, t, args);
    }
    let s = take!(lib.string(h, t, args[0], 1));
    let (seps, limit) = match *args {
        [_] => {
            charge(cx, s.len())?;
            let words: Vec<T<'h>> = s.split_whitespace().map(|w| h.string(w)).collect();
            return Ok(Reply::Done(h.call(lib.s.list, &words)));
        }
        [_, sep] => (take!(lib.strings(h, t, sep, 2, true)), usize::MAX),
        [_, sep, n] => {
            let seps = take!(lib.strings(h, t, sep, 2, true));
            match take!(lib.int(h, t, n, 3)) {
                n @ 1.. => (seps, usize::try_from(n).unwrap_or(usize::MAX)),
                _ => return Ok(lib.bad_arg(h, t, 3)),
            }
        }
        _ => return Ok(Reply::Decline),
    };
    let mut pieces = vec![];
    let mut start = 0;
    while pieces.len() + 1 < limit {
        let Some((p, k)) = next_match(cx, s, start, &seps)? else { break };
        pieces.push(h.string(&s[start..p]));
        start = p + seps[k].len();
    }
    pieces.push(h.string(&s[start..]));
    Ok(Reply::Done(h.call(lib.s.list, &pieces)))
}

/// Whether a search argument is literal: a string, a list of strings, or with `rules` a literal rule `"a" -> "b"`
/// or a list of them (the literal forms keep their fast paths; anything else is read as a string pattern).
fn literal(lib: &Lib, h: &mc_core::Heap, t: T<'_>, rules: bool) -> bool {
    let one = |x: T<'_>| match h.view(x) {
        View::Str(_) => !rules,
        View::App { head, args: &[a, b] }
            if rules && (head == h.sym_term(lib.s.rule) || head == h.sym_term(lib.s.rule_delayed)) =>
        {
            matches!((h.view(a), h.view(b)), (View::Str(_), View::Str(_)))
        }
        // a symbol stays a pending value (the call stays unevaluated)
        View::Sym(s) => !crate::strpat::is_class_name(&h.sym_name(s)),
        _ => false,
    };
    match lib.as_list(h, t) {
        Some(xs) => xs.iter().all(|&x| one(x)),
        None => one(t),
    }
}

/// Whether the last argument is `options(...)`.
fn has_options(lib: &Lib, h: &mc_core::Heap, args: &[T<'_>]) -> bool {
    args.len() > 2 && matches!(h.view(args[args.len() - 1]), View::App { head, .. } if head == h.sym_term(lib.s.options))
}

/// The replacement rules of argument 2: `a -> b`, `a :> b` or a list of them, with nonempty strings `a` and string
/// values `b`; or, in the three-argument form with a string second argument, the pair itself.
fn rules<'h>(lib: &Lib, h: &'h mc_core::Heap, t: T<'h>, r: T<'h>) -> Result<Vec<(&'h str, &'h str)>, Reply<'h>> {
    let one = |x: T<'h>| match h.view(x) {
        View::App { head, args: &[a, b] }
            if head == h.sym_term(lib.s.rule) || head == h.sym_term(lib.s.rule_delayed) =>
        {
            match (h.view(a), h.view(b)) {
                (View::Str(a), View::Str(b)) if !a.is_empty() => Ok((a, b)),
                _ => Err(lib.bad_arg(h, t, 2)),
            }
        }
        View::Sym(_) => Err(Reply::Decline),
        _ => Err(lib.bad_arg(h, t, 2)),
    };
    match lib.as_list(h, r) {
        Some(items) => items.iter().map(|&x| one(x)).collect(),
        None => Ok(vec![one(r)?]),
    }
}

/// `string_replace(s, a -> b)`, `string_replace(s, [rules])`, `string_replace(s, rules, n)` (at most n
/// replacements) and `string_replace(s, a, b)`: every occurrence replaced, left to right, without overlaps.
pub(crate) fn string_replace<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let pattern = match *args {
        [_, a, _] if matches!(h.view(a), View::Str(_)) => false,
        [_, r] | [_, r, _] => !literal(lib, h, r, true),
        _ => false,
    };
    if pattern || has_options(lib, h, args) {
        return scan::replace(lib, cx, t, args, false);
    }
    let s = take!(lib.string(h, t, args[0], 1));
    let (rs, limit) = match *args {
        [_, r] => (take!(rules(lib, h, t, r)), usize::MAX),
        [_, a, b] if matches!(h.view(a), View::Str(_)) => {
            let a = take!(lib.needle(h, t, a, 2));
            (vec![(a, take!(lib.string(h, t, b, 3)))], usize::MAX)
        }
        [_, r, n] => {
            let rs = take!(rules(lib, h, t, r));
            match take!(lib.int(h, t, n, 3)) {
                n @ 0.. => (rs, usize::try_from(n).unwrap_or(usize::MAX)),
                _ => return Ok(lib.bad_arg(h, t, 3)),
            }
        }
        _ => return Ok(Reply::Decline),
    };
    let needles: Vec<&str> = rs.iter().map(|r| r.0).collect();
    let mut out = String::new();
    let (mut start, mut count) = (0, 0);
    while count < limit {
        let Some((p, k)) = next_match(cx, s, start, &needles)? else { break };
        out.push_str(&s[start..p]);
        out.push_str(rs[k].1);
        if out.len() > MAX_BYTES {
            return Ok(too_large(lib, h, t));
        }
        start = p + rs[k].0.len();
        count += 1;
    }
    if count == 0 {
        return Ok(Reply::Done(args[0]));
    }
    out.push_str(&s[start..]);
    charge(cx, out.len())?;
    Ok(Reply::Done(h.string(&out)))
}

/// Which test of a string against a search string.
#[derive(Clone, Copy)]
enum Test {
    Contains,
    Starts,
    Ends,
}

fn test<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>], test: Test) -> Res<Reply<'h>> {
    let h = cx.heap();
    if args.len() == 2 && !literal(lib, h, args[1], false) || has_options(lib, h, args) {
        let w = match test {
            Test::Contains => scan::Where::Anywhere,
            Test::Starts => scan::Where::Start,
            Test::Ends => scan::Where::End,
        };
        return scan::test(lib, cx, t, args, w);
    }
    let &[s, sub] = args else { return Ok(Reply::Decline) };
    let s = take!(lib.string(h, t, s, 1));
    let subs = take!(lib.strings(h, t, sub, 2, false));
    charge(cx, s.len() * subs.len().max(1))?;
    let any = subs.iter().any(|x| match test {
        Test::Contains => s.contains(x),
        Test::Starts => s.starts_with(x),
        Test::Ends => s.ends_with(x),
    });
    Ok(Reply::Done(cx.bool(any)))
}

/// `string_contains(s, sub)`: whether s contains sub (or one of a list of strings); the empty string is in every
/// string.
pub(crate) fn string_contains<'h>(
    lib: &Lib,
    cx: &mut NativeCx<'_, '_, 'h>,
    t: T<'h>,
    args: &[T<'h>],
) -> Res<Reply<'h>> {
    test(lib, cx, t, args, Test::Contains)
}

/// `string_starts(s, prefix)`: whether s starts with prefix (or one of a list of strings).
pub(crate) fn string_starts<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    test(lib, cx, t, args, Test::Starts)
}

/// `string_ends(s, suffix)`: whether s ends with suffix (or one of a list of strings).
pub(crate) fn string_ends<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    test(lib, cx, t, args, Test::Ends)
}
