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
pub(crate) fn dimensions<'h>(_lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [x] = *args else { return Ok(Reply::Decline) };
    let dims = dims_of(cx, x)?;
    let v: Vec<T<'h>> = dims.iter().map(|&d| h.int(d as i64)).collect();
    Ok(Reply::Done(cx.list(&v)))
}

fn dims_of<'h>(cx: &mut NativeCx<'_, '_, 'h>, x: T<'h>) -> Res<Vec<usize>> {
    let h = cx.heap();
    if let Some(p) = h.packed(x) {
        return Ok(p.dims().iter().map(|&d| d as usize).collect());
    }
    let mut dims = vec![];
    let mut level: Vec<T<'h>> = vec![x];
    let mut m = Meter::default();
    while let Some((head, first)) = args_of(h, level[0]) {
        let n = first.len();
        let mut next = Vec::with_capacity(level.len() * n);
        for &u in &level {
            m.step(cx)?;
            match args_of(h, u) {
                Some((g, a)) if g == head && a.len() == n => next.extend_from_slice(a),
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

/// `array_depth(x)`: the number of levels of `x` that form a full array (the length of `dimensions(x)`).
pub(crate) fn array_depth<'h>(_lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [x] = *args else { return Ok(Reply::Decline) };
    let d = dims_of(cx, x)?.len();
    Ok(Reply::Done(h.int(d as i64)))
}

/// `extract(x, pos)`: the part at a position; `extract(x, [pos1, pos2, ...])`: the list of parts at several.
pub(crate) fn extract<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [x, spec] = *args else { return Ok(Reply::Decline) };
    // an association: keys or positions of its values (decision io-core.4)
    if let Some(es) = crate::assocs::entries(lib, h, x) {
        let Some((ps, several)) = assoc_positions(h, cx, &es, spec) else { return Ok(Reply::Decline) };
        let vs = cx.list(&es.iter().map(|&(_, v)| v).collect::<Vec<_>>());
        let mut out = Vec::with_capacity(ps.len());
        for p in &ps {
            match part_at(h, vs, p) {
                Ok(v) => out.push(v),
                Err(i) => return Ok(Reply::Done(lib.bad_index(h, h.int(i), 0))),
            }
        }
        return Ok(Reply::Done(if several { cx.list(&out) } else { out[0] }));
    }
    let Some(ps) = positions(h, cx, spec) else { return Ok(Reply::Decline) };
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
    let [x, r] = *args else { return Ok(Reply::Decline) };
    let Some(rules) = rules_of(lib, h, cx, r) else { return Ok(Reply::Done(lib.bad_arg(h, t, 2))) };
    let mut cur = x;
    for (ps, v) in rules {
        for p in ps {
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
    abs.dedup();
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
pub(crate) fn map_at<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [f, x, spec] = *args else { return Ok(Reply::Decline) };
    if let Some(es) = crate::assocs::entries(lib, h, x) {
        let Some((ps, _)) = assoc_positions(h, cx, &es, spec) else { return Ok(Reply::Decline) };
        return in_values(lib, cx, &es, ps, Edit::Apply(f));
    }
    let Some(ps) = positions(h, cx, spec) else { return Ok(Reply::Decline) };
    edit_all(lib, cx, x, ps, Edit::Apply(f))
}

/// `level(x, levels)`: the list of the parts of `x` at the levels, in the order a depth-first walk finishes them.
pub(crate) fn level<'h>(_lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [x, spec] = *args else { return Ok(Reply::Decline) };
    let Some(levels) = level_spec(h, spec) else { return Ok(Reply::Decline) };
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
    let [xs, n] = *args else { return Ok(Reply::Decline) };
    if !matches!(h.view(n), View::Int(0..)) {
        return Ok(Reply::Decline);
    }
    let sym = |s: &str| h.sym_term(h.sym(s));
    let mut sorted = h.app(sym("sort"), &[xs]);
    if largest {
        sorted = h.app(sym("reverse"), &[sorted]);
    }
    Ok(Reply::Again(h.app(sym("take"), &[sorted, n])))
}

/// `full_form(x)`: `x`, shown with every part as a plain call (`add(x, pow(y, 2))`); `input_form(x)`: `x` shown in
/// the input syntax. Both stay as wrappers around their (evaluated) argument; the printer reads them.
pub(crate) fn display_wrapper<'h>(_lib: &Lib, _cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, _args: &[T<'h>]) -> Res<Reply<'h>> {
    Ok(Reply::Decline)
}

/// A run `start..end` of a list and the rule's right side for it.
type Run<'h> = (usize, usize, Option<T<'h>>);

/// The non-overlapping runs of `xs` that match the list pattern `p` (`[a, b]`, `[x_, x_]`, `[__, 0]`), scanning
/// from the left and taking at each start the longest run that matches: `(start, end)` pairs with the rule's
/// right side for each, or `None` when the pattern does not compile. Runs are tried up to 4096 elements long.
fn sequence_runs<'h>(
    lib: &Lib,
    cx: &mut NativeCx<'_, '_, 'h>,
    xs: &[T<'h>],
    p: T<'h>,
) -> Res<Option<Vec<Run<'h>>>> {
    let h = cx.heap();
    let (pat, rhs) = match h.view(p) {
        View::App { head, args: [l, r] } if head == h.sym_term(lib.s.rule) || head == h.sym_term(lib.s.rule_delayed) => {
            (*l, Some(*r))
        }
        _ => (p, None),
    };
    let Some(mut m) = Matcher::new(lib, h, pat) else { return Ok(None) };
    let mut out = vec![];
    let mut i = 0;
    while i < xs.len() {
        let mut found = None;
        let top = xs.len().min(i + 4096);
        for j in (i + 1..=top).rev() {
            cx.tick(1)?;
            let run = cx.list(&xs[i..j]);
            if m.matches(cx, run)? {
                let r = match rhs {
                    Some(r) => Some(m.substitute(cx, r)?),
                    None => None,
                };
                found = Some((j, r));
                break;
            }
        }
        match found {
            Some((j, r)) => {
                out.push((i, j, r));
                i = j;
            }
            None => i += 1,
        }
    }
    Ok(Some(out))
}

/// `sequence_cases(xs, p)`: the runs of `xs` matching the list pattern `p` (`p -> r`: each run's `r`).
pub(crate) fn sequence_cases<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [xs, p] = *args else { return Ok(Reply::Decline) };
    let xs = take!(lib.list_operand(h, t, xs, 1));
    let Some(runs) = sequence_runs(lib, cx, xs, p)? else { return Ok(Reply::Done(lib.bad_arg(h, t, 2))) };
    let out: Vec<T<'h>> = runs.into_iter().map(|(i, j, r)| r.unwrap_or_else(|| cx.list(&xs[i..j]))).collect();
    Ok(Reply::Again(cx.list(&out)))
}

/// `sequence_position(xs, p)`: the `[start, end]` positions of the runs of `xs` matching the list pattern `p`.
pub(crate) fn sequence_position<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [xs, p] = *args else { return Ok(Reply::Decline) };
    let xs = take!(lib.list_operand(h, t, xs, 1));
    let Some(runs) = sequence_runs(lib, cx, xs, p)? else { return Ok(Reply::Done(lib.bad_arg(h, t, 2))) };
    let out: Vec<T<'h>> = runs.into_iter().map(|(i, j, _)| cx.list(&[h.int(i as i64 + 1), h.int(j as i64)])).collect();
    Ok(Reply::Done(cx.list(&out)))
}

/// `sequence_count(xs, p)`: how many runs of `xs` match the list pattern `p`.
pub(crate) fn sequence_count<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [xs, p] = *args else { return Ok(Reply::Decline) };
    let xs = take!(lib.list_operand(h, t, xs, 1));
    let Some(runs) = sequence_runs(lib, cx, xs, p)? else { return Ok(Reply::Done(lib.bad_arg(h, t, 2))) };
    Ok(Reply::Done(h.int(runs.len() as i64)))
}

/// `sequence_replace(xs, p -> r)`: each run of `xs` matching the list pattern `p` replaced by the one element `r`.
pub(crate) fn sequence_replace<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [xs, p] = *args else { return Ok(Reply::Decline) };
    let xs = take!(lib.list_operand(h, t, xs, 1));
    let Some(runs) = sequence_runs(lib, cx, xs, p)? else { return Ok(Reply::Done(lib.bad_arg(h, t, 2))) };
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

/// `sequence_split(xs, p)`: the pieces of `xs` between the runs matching the list pattern `p` (empty pieces left
/// out); with `p -> r`, each run's `r` stays between the pieces.
pub(crate) fn sequence_split<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [xs, p] = *args else { return Ok(Reply::Decline) };
    let xs = take!(lib.list_operand(h, t, xs, 1));
    let Some(runs) = sequence_runs(lib, cx, xs, p)? else { return Ok(Reply::Done(lib.bad_arg(h, t, 2))) };
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
