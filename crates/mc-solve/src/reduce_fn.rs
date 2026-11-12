//! `reduce` for conditions that hold one function of the unknown (`exp`, `b^x`, `log`, `sin`, `cos`, `tan` of `x`
//! itself) next to polynomial and rational atoms (rounds 21 and 22: `exp(x) > 1`, `sin(x) > 1/2 and 0 < x < 2 pi`).
//!
//! Every atom depends on `x` alone or on the function `y = F(x)` alone (a mixed atom such as `x < sin(x)` is not
//! handled). The set of real `x` of an atom is
//!
//! - for an atom in `x`: the intervals of the rational algorithm of [`crate::reduce`];
//! - for an atom in `y`: the intervals of the same algorithm in `y`, mapped back through the branches of `F` on
//!   which it is monotone (the inverse functions of Abramowitz and Stegun 4.4): `exp`, `log` and `b^x` have one
//!   branch, `tan` one per period `((k - 1/2) pi, (k + 1/2) pi)` with the inverse `k pi + atan(y)`, `sin` the pieces
//!   `[(k - 1/2) pi, (k + 1/2) pi]` with the inverse `k pi + (-1)^k asin(y)`, `cos` the pieces `[k pi, (k + 1) pi]`
//!   with the inverse `k pi + acos(y)` (k even) or `(k + 1) pi - acos(y)` (k odd).
//!
//! The sets are combined by the condition's `and`, `or`, `not` as unions of intervals whose end points are exact
//! terms ordered by their machine values (equal within 1e-12). A periodic function has infinitely many pieces, so
//! the conjuncts of the condition on `x` alone must bound the unknown to a window with at most [`MAX_PIECES`] pieces;
//! otherwise the call is left to the failure it gave before. `log` restricts the unknown to `x > 0`.
use crate::cx::{Env, Out};
use crate::reduce::{Cond, Rel};
use mc_api::T;
use mc_poly::work::{Meter, WorkResult};

/// Most monotone pieces enumerated for a periodic function.
const MAX_PIECES: i64 = 64;

/// An end point: a term (`None` for an infinity), its machine value, and whether it belongs to the set.
#[derive(Clone, Copy)]
pub(crate) struct End<'h> {
    pub(crate) term: Option<T<'h>>,
    pub(crate) val: f64,
    pub(crate) closed: bool,
}

/// An interval of reals.
#[derive(Clone, Copy)]
pub(crate) struct Iv<'h> {
    pub(crate) lo: End<'h>,
    pub(crate) hi: End<'h>,
}

pub(crate) fn same(a: f64, b: f64) -> bool {
    a == b || (a.is_finite() && b.is_finite() && (a - b).abs() <= 1e-12 * a.abs().max(b.abs()).max(1.0))
}

pub(crate) fn neg_inf<'h>() -> End<'h> {
    End { term: None, val: f64::NEG_INFINITY, closed: false }
}

pub(crate) fn pos_inf<'h>() -> End<'h> {
    End { term: None, val: f64::INFINITY, closed: false }
}

fn all<'h>() -> Vec<Iv<'h>> {
    vec![Iv { lo: neg_inf(), hi: pos_inf() }]
}

/// Is the interval empty?
fn empty(i: &Iv<'_>) -> bool {
    if same(i.lo.val, i.hi.val) && i.lo.val.is_finite() {
        return !(i.lo.closed && i.hi.closed);
    }
    i.lo.val > i.hi.val
}

/// Sorted, empty ones dropped, overlapping or touching ones merged.
pub(crate) fn normal<'h>(mut v: Vec<Iv<'h>>) -> Vec<Iv<'h>> {
    v.retain(|i| !empty(i));
    v.sort_by(|a, b| a.lo.val.total_cmp(&b.lo.val).then((!a.lo.closed).cmp(&!b.lo.closed)));
    let mut out: Vec<Iv<'h>> = vec![];
    for i in v {
        if let Some(last) = out.last_mut() {
            let touch = same(last.hi.val, i.lo.val) && (last.hi.closed || i.lo.closed);
            if (i.lo.val < last.hi.val && !same(i.lo.val, last.hi.val)) || touch {
                if i.hi.val > last.hi.val && !same(i.hi.val, last.hi.val) {
                    last.hi = i.hi;
                } else if same(i.hi.val, last.hi.val) && i.hi.closed {
                    last.hi.closed = true;
                }
                continue;
            }
        }
        out.push(i);
    }
    out
}

fn intersect<'h>(a: &[Iv<'h>], b: &[Iv<'h>]) -> Vec<Iv<'h>> {
    let mut out = vec![];
    for x in a {
        for y in b {
            let lo = if x.lo.val > y.lo.val && !same(x.lo.val, y.lo.val) {
                x.lo
            } else if y.lo.val > x.lo.val && !same(x.lo.val, y.lo.val) {
                y.lo
            } else {
                End { closed: x.lo.closed && y.lo.closed, ..x.lo }
            };
            let hi = if x.hi.val < y.hi.val && !same(x.hi.val, y.hi.val) {
                x.hi
            } else if y.hi.val < x.hi.val && !same(x.hi.val, y.hi.val) {
                y.hi
            } else {
                End { closed: x.hi.closed && y.hi.closed, ..x.hi }
            };
            out.push(Iv { lo, hi });
        }
    }
    normal(out)
}

/// The complement of `a` in `domain`.
fn complement<'h>(a: &[Iv<'h>], domain: &[Iv<'h>]) -> Vec<Iv<'h>> {
    let a = normal(a.to_vec());
    let mut gaps = vec![];
    let mut cursor = neg_inf();
    let mut first = true;
    for i in &a {
        let gap_lo = if first { cursor } else { End { closed: !cursor.closed, ..cursor } };
        gaps.push(Iv { lo: gap_lo, hi: End { closed: !i.lo.closed, ..i.lo } });
        cursor = i.hi;
        first = false;
    }
    let tail_lo = if first { cursor } else { End { closed: !cursor.closed, ..cursor } };
    gaps.push(Iv { lo: tail_lo, hi: pos_inf() });
    // an infinite end is never closed
    for g in &mut gaps {
        if g.lo.val.is_infinite() {
            g.lo.closed = false;
        }
        if g.hi.val.is_infinite() {
            g.hi.closed = false;
        }
    }
    intersect(&normal(gaps), domain)
}

/// The machine value of a term.
pub(crate) fn value<'h>(env: &Env<'h>, t: T<'h>) -> Option<f64> {
    match env.machine(t) {
        Some((v, 0.0)) => Some(v),
        _ => None,
    }
}

fn end<'h>(env: &Env<'h>, t: T<'h>, closed: bool) -> Option<End<'h>> {
    Some(End { term: Some(t), val: value(env, t)?, closed })
}

/// The intervals of a condition on the variable `v` written by [`crate::reduce::reduce`]: `true`, `false`, `or`,
/// `and` of comparisons with `v` on one side, `v == r`.
fn read<'h>(env: &Env<'h>, t: T<'h>, v: T<'h>) -> Option<Vec<Iv<'h>>> {
    if env.is_sym(t, "true") {
        return Some(all());
    }
    if env.is_sym(t, "false") {
        return Some(vec![]);
    }
    let (name, args) = env.head_name(t)?;
    match name.as_str() {
        "or" => {
            let mut out = vec![];
            for &a in args {
                out.extend(read(env, a, v)?);
            }
            Some(normal(out))
        }
        "and" => {
            let mut out = all();
            for &a in args {
                out = intersect(&out, &read(env, a, v)?);
            }
            Some(out)
        }
        "equal" => {
            let [l, r] = args else { return None };
            let r = if *l == v {
                *r
            } else if *r == v {
                *l
            } else {
                return None;
            };
            let e = end(env, r, true)?;
            Some(vec![Iv { lo: e, hi: e }])
        }
        "less" | "less_equal" | "greater" | "greater_equal" => {
            let [l, r] = args else { return None };
            // normalise to `v rel c`
            let (rel, c) = if *l == v {
                (name.as_str(), *r)
            } else if *r == v {
                (
                    match name.as_str() {
                        "less" => "greater",
                        "less_equal" => "greater_equal",
                        "greater" => "less",
                        _ => "less_equal",
                    },
                    *l,
                )
            } else {
                return None;
            };
            let closed = rel.ends_with("equal");
            let e = end(env, c, closed)?;
            Some(vec![if rel.starts_with("less") { Iv { lo: neg_inf(), hi: e } } else { Iv { lo: e, hi: pos_inf() } }])
        }
        _ => None,
    }
}

/// Which function the conditions hold.
#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Exp,
    Log,
    Sin,
    Cos,
    Tan,
}

/// The conditions `cond` (atoms in `diffs`, each `diff rel 0`) of one real unknown `x`, with a function of `x`.
pub(crate) fn reduce<'h>(env: &Env<'h>, cond: &Cond, diffs: &[T<'h>], x: T<'h>, m: &mut Meter) -> WorkResult<Option<Out<'h>>> {
    if let Some(out) = reduce_kernel(env, cond, diffs, x, m)? {
        return Ok(Some(out));
    }
    // what the kernel algorithm does not take: the sign chart (B23-15)
    crate::chart::chart(env, cond, diffs, x, m)
}

fn reduce_kernel<'h>(env: &Env<'h>, cond: &Cond, diffs: &[T<'h>], x: T<'h>, m: &mut Meter) -> WorkResult<Option<Out<'h>>> {
    // powers b^(c x) of one base with rational c (exp(-x), exp(2 x), 2^(x/2)) as integer powers of the one kernel
    // b'^x with b' = b^(g/L) the smallest of them: exp(x) + exp(-x) has the one kernel exp(x) (D23-25)
    let diffs_owned: Vec<T<'h>>;
    let diffs = match exponent_family(env, diffs, x, m)? {
        Some(v) => {
            diffs_owned = v;
            &diffs_owned[..]
        }
        None => diffs,
    };
    // the function: the one non-polynomial kernel
    let ring = env.ring(diffs, &[x], m)?;
    let kernels: Vec<T<'h>> = ring.kernels.iter().copied().filter(|&k| k != x && env.contains_any(k, &[x])).collect();
    let [k] = kernels.as_slice() else { return Ok(None) };
    let k = *k;
    let Some((name, args)) = env.head_name(k) else { return Ok(None) };
    let (kind, base) = match (name.as_str(), args) {
        ("pow", [b, e]) if *e == x && !env.contains_any(*b, &[x]) => {
            let Some((bv, im)) = env.machine(*b) else { return Ok(None) };
            if im != 0.0 || bv <= 0.0 || same(bv, 1.0) {
                return Ok(None);
            }
            (Kind::Exp, Some((*b, bv)))
        }
        ("log", [a]) if *a == x => (Kind::Log, None),
        ("sin", [a]) if *a == x => (Kind::Sin, None),
        ("cos", [a]) if *a == x => (Kind::Cos, None),
        ("tan", [a]) if *a == x => (Kind::Tan, None),
        _ => return Ok(None),
    };
    let y = env.sym("reduce`y");
    let mut rels: Vec<Option<Rel>> = vec![None; diffs.len()];
    collect_rels(cond, &mut rels);
    // the atoms: x alone or y alone
    let mut y_atoms: Vec<Option<T<'h>>> = vec![];
    let mut atom_sets: Vec<Option<Vec<Iv<'h>>>> = vec![None; diffs.len()];
    for &d in diffs.iter() {
        let dy = env.subst(d, &[(k, y)], m)?;
        let (in_x, in_y) = (env.contains_any(dy, &[x]), env.contains_any(dy, &[y]));
        if in_x && in_y {
            return Ok(None);
        }
        y_atoms.push(in_y.then_some(dy));
    }
    // the window and domain
    let mut window = all();
    if kind == Kind::Log {
        let zero = End { term: Some(env.int(0)), val: 0.0, closed: false };
        window = vec![Iv { lo: zero, hi: pos_inf() }];
    }
    // x-only atoms first (they bound the window)
    for i in 0..diffs.len() {
        if y_atoms[i].is_some() {
            continue;
        }
        let Some(rel) = rels[i] else { return Ok(None) };
        let Some(s) = solve_atom(env, diffs[i], rel, x, m)? else { return Ok(None) };
        atom_sets[i] = Some(s);
    }
    // the window: the top-level conjuncts on x alone
    let mut top = vec![];
    top_atoms(cond, &mut top);
    for i in top {
        if let Some(s) = &atom_sets[i] {
            window = intersect(&window, s);
        }
    }
    if matches!(kind, Kind::Sin | Kind::Cos | Kind::Tan) {
        let bounded = !window.is_empty() && window.iter().all(|i| i.lo.val.is_finite() && i.hi.val.is_finite());
        if !bounded {
            return Ok(None);
        }
    }
    for i in 0..diffs.len() {
        let Some(dy) = y_atoms[i] else { continue };
        let Some(rel) = rels[i] else { return Ok(None) };
        let Some(ys) = solve_atom(env, dy, rel, y, m)? else { return Ok(None) };
        let Some(s) = map_back(env, kind, base, &ys, &window) else { return Ok(None) };
        atom_sets[i] = Some(s);
    }
    let Some(result) = combine(cond, &atom_sets, &window) else { return Ok(None) };
    let result = intersect(&result, &window);
    Ok(Some(Out::Again(write(env, &normal(result), x))))
}

fn collect_rels(c: &Cond, out: &mut [Option<Rel>]) {
    match c {
        Cond::Atom(i, r) => out[*i] = Some(*r),
        Cond::And(cs) | Cond::Or(cs) => cs.iter().for_each(|c| collect_rels(c, out)),
        Cond::Not(c) => collect_rels(c, out),
        Cond::Const(_) => {}
    }
}

/// The atoms that every solution must satisfy: the top-level conjuncts.
fn top_atoms(c: &Cond, out: &mut Vec<usize>) {
    match c {
        Cond::Atom(i, _) => out.push(*i),
        Cond::And(cs) => cs.iter().for_each(|c| top_atoms(c, out)),
        _ => {}
    }
}

/// The relation `d rel 0` as a comparison term.
fn relation<'h>(env: &Env<'h>, d: T<'h>, rel: Rel) -> T<'h> {
    let name = match rel {
        Rel::Lt => "less",
        Rel::Le => "less_equal",
        Rel::Gt => "greater",
        Rel::Ge => "greater_equal",
        Rel::Eq => "equal",
        Rel::Ne => "unequal",
    };
    env.call(name, &[d, env.int(0)])
}

/// The set of real `v` where `d rel 0`, from the rational algorithm.
fn solve_atom<'h>(env: &Env<'h>, d: T<'h>, rel: Rel, v: T<'h>, m: &mut Meter) -> WorkResult<Option<Vec<Iv<'h>>>> {
    if let Out::Again(r) = crate::reduce::reduce(env, &[relation(env, d, rel), v], m)? {
        return Ok(read(env, r, v));
    }
    linear_atom(env, d, rel, v, m)
}

/// `d rel 0` for `d = c1 v + c0` with closed constants `c0`, `c1` (such as `x - 2 pi`), which the rational
/// algorithm does not take.
fn linear_atom<'h>(env: &Env<'h>, d: T<'h>, rel: Rel, v: T<'h>, m: &mut Meter) -> WorkResult<Option<Vec<Iv<'h>>>> {
    let ar = &env.ar;
    let c0 = env.subst(d, &[(v, env.int(0))], m)?;
    let c1 = ar.sub(env.subst(d, &[(v, env.int(1))], m)?, c0);
    if env.contains_any(c0, &[v]) || env.contains_any(c1, &[v]) {
        return Ok(None);
    }
    let rest = ar.sub(d, ar.add(&[c0, ar.mul(&[c1, v])]));
    if !env.is_zero(rest, m)? {
        return Ok(None);
    }
    let (Some(a), Some(b)) = (value(env, c1), value(env, c0)) else { return Ok(None) };
    if a == 0.0 {
        let holds = match rel {
            Rel::Lt => b < 0.0,
            Rel::Le => b <= 0.0,
            Rel::Gt => b > 0.0,
            Rel::Ge => b >= 0.0,
            Rel::Eq => b == 0.0,
            Rel::Ne => b != 0.0,
        };
        return Ok(Some(if holds { all() } else { vec![] }));
    }
    let t = ar.neg(ar.div(c0, c1));
    let closed = matches!(rel, Rel::Le | Rel::Ge | Rel::Eq);
    let Some(e) = end(env, t, closed) else { return Ok(None) };
    // c1 (v - t) rel 0: below t when the relation is `<` and c1 > 0, or `>` and c1 < 0
    let below = matches!(rel, Rel::Lt | Rel::Le) == (a > 0.0);
    Ok(Some(match rel {
        Rel::Eq => vec![Iv { lo: e, hi: e }],
        Rel::Ne => vec![Iv { lo: neg_inf(), hi: End { closed: false, ..e } }, Iv { lo: End { closed: false, ..e }, hi: pos_inf() }],
        _ if below => vec![Iv { lo: neg_inf(), hi: e }],
        _ => vec![Iv { lo: e, hi: pos_inf() }],
    }))
}

/// The truth of the condition tree from the sets of its atoms.
fn combine<'h>(c: &Cond, sets: &[Option<Vec<Iv<'h>>>], domain: &[Iv<'h>]) -> Option<Vec<Iv<'h>>> {
    Some(match c {
        Cond::Const(true) => domain.to_vec(),
        Cond::Const(false) => vec![],
        Cond::Atom(i, _) => sets[*i].clone()?,
        Cond::And(cs) => {
            let mut out = domain.to_vec();
            for c in cs {
                out = intersect(&out, &combine(c, sets, domain)?);
            }
            out
        }
        Cond::Or(cs) => {
            let mut out = vec![];
            for c in cs {
                out.extend(combine(c, sets, domain)?);
            }
            normal(out)
        }
        Cond::Not(c) => complement(&combine(c, sets, domain)?, domain),
    })
}

/// The `x` intervals where the function takes a value of `ys`, within `window`.
fn map_back<'h>(env: &Env<'h>, kind: Kind, base: Option<(T<'h>, f64)>, ys: &[Iv<'h>], window: &[Iv<'h>]) -> Option<Vec<Iv<'h>>> {
    let pi = env.sym("pi");
    let ar = &env.ar;
    let call = |n: &str, t: T<'h>| env.call(n, &[t]);
    let mut out: Vec<Iv<'h>> = vec![];
    match kind {
        Kind::Exp | Kind::Log => {
            let image = if kind == Kind::Exp {
                vec![Iv { lo: End { term: Some(env.int(0)), val: 0.0, closed: false }, hi: pos_inf() }]
            } else {
                all()
            };
            let increasing = base.is_none_or(|(_, bv)| bv > 1.0);
            for i in intersect(ys, &image) {
                let g = |e: End<'h>| -> Option<End<'h>> {
                    match (kind, e.term) {
                        (Kind::Exp, None) => Some(if (e.val > 0.0) == increasing { pos_inf() } else { neg_inf() }),
                        (Kind::Exp, Some(_)) if e.val == 0.0 => Some(if increasing { neg_inf() } else { pos_inf() }),
                        (Kind::Exp, Some(t)) => {
                            let l = match base {
                                Some((b, _)) if !env.is_sym(b, "e") => ar.div(call("log", t), call("log", b)),
                                _ => call("log", t),
                            };
                            end(env, l, e.closed)
                        }
                        // log: y -> inf is x -> inf, y -> -inf is x -> 0 (open)
                        (_, None) => Some(if e.val > 0.0 { pos_inf() } else { End { term: Some(env.int(0)), val: 0.0, closed: false } }),
                        (_, Some(t)) => end(env, ar.pow(env.sym("e"), t), e.closed),
                    }
                };
                let (a, b) = (g(i.lo)?, g(i.hi)?);
                out.push(if increasing { Iv { lo: a, hi: b } } else { Iv { lo: b, hi: a } });
            }
            Some(intersect(&normal(out), window))
        }
        Kind::Sin | Kind::Cos | Kind::Tan => {
            let lo = window.iter().map(|i| i.lo.val).fold(f64::INFINITY, f64::min);
            let hi = window.iter().map(|i| i.hi.val).fold(f64::NEG_INFINITY, f64::max);
            let pi_v = std::f64::consts::PI;
            let (k0, k1) = match kind {
                Kind::Cos => ((lo / pi_v).floor() as i64, (hi / pi_v).ceil() as i64 - 1),
                _ => ((lo / pi_v - 0.5).floor() as i64, (hi / pi_v - 0.5).ceil() as i64),
            };
            if k1 - k0 > MAX_PIECES {
                return None;
            }
            let image = if kind == Kind::Tan {
                all()
            } else {
                let e = |v: i64| End { term: Some(env.int(v)), val: v as f64, closed: true };
                vec![Iv { lo: e(-1), hi: e(1) }]
            };
            let ys = intersect(ys, &image);
            let half = ar.div(env.int(1), env.int(2));
            for k in k0..=k1 {
                let kk = env.int(k);
                let kpi = ar.mul(&[kk, pi]);
                let (piece_lo, piece_hi, increasing): (T<'h>, T<'h>, bool) = match kind {
                    Kind::Cos => (kpi, ar.mul(&[env.int(k + 1), pi]), k % 2 != 0),
                    _ => (ar.mul(&[ar.sub(kk, half), pi]), ar.mul(&[ar.add(&[kk, half]), pi]), k % 2 == 0 || kind == Kind::Tan),
                };
                let open = kind == Kind::Tan;
                let (pl, ph) = (end(env, piece_lo, !open)?, end(env, piece_hi, !open)?);
                let piece = vec![Iv { lo: pl, hi: ph }];
                let mut found = vec![];
                for i in &ys {
                    let g = |e: End<'h>| -> Option<End<'h>> {
                        let Some(t) = e.term else {
                            // tan at +-inf: the piece's ends (open)
                            return Some(if e.val > 0.0 { End { closed: false, ..ph } } else { End { closed: false, ..pl } });
                        };
                        let r = match kind {
                            Kind::Sin => ar.add(&[kpi, ar.mul(&[env.int(if k % 2 == 0 { 1 } else { -1 }), call("asin", t)])]),
                            Kind::Cos if k % 2 == 0 => ar.add(&[kpi, call("acos", t)]),
                            Kind::Cos => ar.sub(ar.mul(&[env.int(k + 1), pi]), call("acos", t)),
                            _ => ar.add(&[kpi, call("atan", t)]),
                        };
                        end(env, r, e.closed)
                    };
                    let (a, b) = (g(i.lo)?, g(i.hi)?);
                    found.push(if increasing { Iv { lo: a, hi: b } } else { Iv { lo: b, hi: a } });
                }
                out.extend(intersect(&normal(found), &piece));
            }
            Some(intersect(&normal(out), window))
        }
    }
}

/// The set as a condition on `x`.
pub(crate) fn write<'h>(env: &Env<'h>, set: &[Iv<'h>], x: T<'h>) -> T<'h> {
    let mut parts = vec![];
    for i in set {
        let (lo, hi) = (i.lo, i.hi);
        if lo.val.is_infinite() && hi.val.is_infinite() {
            return env.sym("true");
        }
        if let (Some(a), Some(_)) = (lo.term, hi.term) {
            if same(lo.val, hi.val) && lo.closed && hi.closed {
                parts.push(env.call("equal", &[x, a]));
                continue;
            }
        }
        let l = lo.term.filter(|_| lo.val.is_finite()).map(|a| env.call(if lo.closed { "less_equal" } else { "less" }, &[a, x]));
        let u = hi.term.filter(|_| hi.val.is_finite()).map(|b| env.call(if hi.closed { "less_equal" } else { "less" }, &[x, b]));
        parts.push(match (l, u) {
            (Some(a), Some(b)) => env.call("and", &[a, b]),
            (Some(a), None) => a,
            (None, Some(b)) => b,
            (None, None) => env.sym("true"),
        });
    }
    match parts.as_slice() {
        [] => env.sym("false"),
        [one] => *one,
        _ => env.call("or", &parts),
    }
}

/// `diffs` with every power `b^(c x)` (one free base `b`, rational `c`) written as an integer power of `U = b'^x`,
/// `b' = b^(g/L)` for the gcd `g` of the numerators and the lcm `L` of the denominators of the `c`; `None` when there
/// is no such power, one kernel `b^x` alone, or the powers do not share the base.
fn exponent_family<'h>(env: &Env<'h>, diffs: &[T<'h>], x: T<'h>, m: &mut Meter) -> WorkResult<Option<Vec<T<'h>>>> {
    let mut found: Vec<(T<'h>, T<'h>, mc_arith::num::Rat)> = vec![];
    for d in diffs {
        let mut stack = vec![*d];
        while let Some(u) = stack.pop() {
            let Some((name, args)) = env.head_name(u) else { continue };
            if let ("pow", [b, e]) = (name.as_str(), args) {
                if !env.contains_any(*b, &[x]) && env.contains_any(*e, &[x]) {
                    let ratio = env.ar.div(*e, x);
                    if let Some(q) = env.ar.num(ratio).and_then(|n| n.as_rat().cloned()) {
                        if q.num.to_i64().is_some_and(|n| n != 0) && q.den.to_u128().is_some_and(|d| d <= 1000) {
                            found.push((u, *b, q));
                            continue;
                        }
                    }
                }
            }
            stack.extend(args.iter().copied());
        }
    }
    let Some(&(_, base, _)) = found.first() else { return Ok(None) };
    if found.iter().any(|f| f.1 != base) || found.iter().all(|f| f.2.num.to_i64() == Some(1) && f.2.den.to_u128() == Some(1)) {
        return Ok(None);
    }
    let (mut g, mut l): (u128, u128) = (0, 1);
    for (_, _, q) in &found {
        let (Some(n), Some(d)) = (q.num.to_i64(), q.den.to_u128()) else { return Ok(None) };
        let (mut a, mut b) = (g, n.unsigned_abs() as u128);
        while b != 0 {
            (a, b) = (b, a % b);
        }
        g = a;
        let (mut a, mut b) = (l, d);
        while b != 0 {
            (a, b) = (b, a % b);
        }
        l = l / a * d;
    }
    let step = mc_arith::Num::Real(mc_arith::Real::Exact(mc_arith::num::Rat::new(mc_arith::num::Int::from_i64(g as i64), mc_arith::num::Int::from_i64(l as i64))));
    let unit = env.call("pow", &[env.ar.pow(base, env.ar.number(&step)), x]);
    let mut rules: Vec<(T<'h>, T<'h>)> = vec![];
    for (u, _, q) in &found {
        let k = env.ar.div(env.ar.number(&mc_arith::Num::Real(mc_arith::Real::Exact(q.clone()))), env.ar.number(&step));
        // built as a plain call: the arithmetic would fold (e^x)^(-1) back into e^(-x)
        rules.push((*u, env.call("pow", &[unit, k])));
    }
    let mut out = Vec::with_capacity(diffs.len());
    for &d in diffs {
        out.push(env.subst(d, &rules, m)?);
    }
    Ok(Some(out))
}
