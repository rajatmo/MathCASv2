//! Functions defined by cases (decision IOSYM.2): the switching calls of a variable (`abs`, `sign`,
//! `unit_step`, `max`, `min`, `if`, `piecewise`, and the integer-valued step functions `floor`, `ceiling`, `round`,
//! `integer_part`, `fractional_part` of a linear argument), the real points where they switch (the real zeros of
//! the switching expressions, found by `solve(g == 0, x, reals)`, at most 64), and their branches at a point.
//!
//! Definite integrals by cases (`mc-integrals`, `definite::split`) cut their interval at these points and integrate
//! piece by piece; one-sided limits resolve the switches just beside the point ([`resolve_near`]). Moved here from
//! the definite integrals in round 34, when limits and integration became crates of their own.
use crate::bounds::{cmp, point, sign, subst, Pt};
use mc_calculus::diff::piecewise_parts;
use mc_calculus::common::{apply, is_free};
use mc_calculus::term::Build;
use mc_api::{names, Store, View, T};
use mc_eval::{NativeCx, Res};
use rustc_hash::FxHashMap;
use std::cmp::Ordering;

/// The most cut points tried (beyond that the call stays).
const MAX_CUTS: usize = 64;

/// The kinds of switching calls.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Switch {
    /// `abs(u)`
    Abs,
    /// `sign(u)`
    Sign,
    /// `unit_step(u)`
    UnitStep,
    /// `max(a, b, ...)`
    Max,
    /// `min(a, b, ...)`
    Min,
    /// `if(c, a, b)`
    If,
    /// `piecewise(cases)`, `piecewise(cases, default)`
    Piecewise,
    /// `floor`, `ceiling`, `round`, `integer_part`, `fractional_part`: constant (or the argument minus a constant)
    /// between the points where the argument crosses an integer
    Step,
}

fn switch_of(name: &str, arity: usize) -> Option<Switch> {
    Some(match (name, arity) {
        ("abs", 1) => Switch::Abs,
        ("sign", 1) => Switch::Sign,
        // heaviside_theta differs from unit_step only at 0, which a one-sided limit does not see
        ("unit_step" | "heaviside_theta", 1) => Switch::UnitStep,
        ("max", 2..) => Switch::Max,
        ("min", 2..) => Switch::Min,
        ("if", 3) => Switch::If,
        ("piecewise", 1 | 2) => Switch::Piecewise,
        ("floor" | "ceiling" | "round" | "integer_part" | "fractional_part", 1) => Switch::Step,
        _ => return None,
    })
}

/// The switch of a call with a symbol head.
pub fn switch<'h>(b: &Build<'h>, t: T<'h>) -> Option<(Switch, &'h [T<'h>])> {
    let h = b.heap();
    let View::App { head, args } = h.view(t) else { return None };
    let s = b.sym_of(head)?;
    switch_of(&h.sym_name(s), args.len()).map(|k| (k, args))
}

/// The switching calls of `f` that depend on `x`.
pub fn switches_of<'h>(b: &Build<'h>, f: T<'h>, x: T<'h>) -> Vec<T<'h>> {
    let mut switches = vec![];
    let mut stack = vec![f];
    let mut seen = rustc_hash::FxHashSet::default();
    while let Some(u) = stack.pop() {
        if !seen.insert(u) || is_free(b, u, x) {
            continue;
        }
        if let View::App { args, .. } = b.heap().view(u) {
            if switch(b, u).is_some() {
                switches.push(u);
            }
            stack.extend(args.iter().copied());
        }
    }
    switches
}

/// The expressions whose zeros are the cut points of the switches; `None` for a condition of another shape.
pub fn zero_exprs<'h>(b: &Build<'h>, switches: &[T<'h>]) -> Option<Vec<T<'h>>> {
    let mut gs = vec![];
    for &s in switches {
        let Some((kind, args)) = switch(b, s) else { continue };
        match kind {
            Switch::Abs | Switch::Sign | Switch::UnitStep => gs.push(args[0]),
            // the cuts of the step functions are not zeros of an expression ([`step_cuts`])
            Switch::Step => {}
            Switch::Max | Switch::Min => {
                for i in 0..args.len() {
                    for j in i + 1..args.len() {
                        gs.push(b.sub(args[i], args[j]));
                    }
                }
            }
            Switch::If => {
                if !comparisons(b, args[0], &mut gs) {
                    return None;
                }
            }
            Switch::Piecewise => {
                let (pieces, _) = piecewise_parts(b, args)?;
                for (_, c) in pieces {
                    if !comparisons(b, c, &mut gs) {
                        return None;
                    }
                }
            }
        }
    }
    Some(gs)
}

/// The zeros of `gs` strictly inside `(p, q)` (an ordered pair of points), in order; `None` when not all are found.
pub fn cut_points<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, gs: &[T<'h>], x: T<'h>, p: Pt<'h>, q: Pt<'h>) -> Res<Option<Vec<Pt<'h>>>> {
    let mut cuts: Vec<Pt<'h>> = vec![];
    let h = b.heap();
    for &g in gs {
        if is_free(b, g, x) {
            continue;
        }
        // a switching expression must be continuous: its sign changes only at its zeros (`round(x) - x` changes
        // sign at the jumps of `round` too)
        if has_jumps(b, g, x) {
            return Ok(None);
        }
        cx.tick(1)?;
        let mut eq = h.call(h.sym("equal"), &[g, b.zero()]);
        if !rational_in(b, g, x) {
            // not a rational function: solve finds every zero only on a bounded interval (its numeric search)
            let (Pt::Fin(l), Pt::Fin(r)) = (p, q) else { return Ok(None) };
            let inside = [h.call(h.sym("less"), &[l, x]), h.call(h.sym("less"), &[x, r])];
            eq = h.call(h.sym("and"), &[eq, inside[0], inside[1]]);
        }
        let sols = apply(cx, b, "solve", &[eq, x, h.sym_term(h.sym("reals"))])?;
        let View::App { head, args: sols } = h.view(sols) else { return Ok(None) };
        if b.sym_of(head) != Some(b.hd.list) {
            return Ok(None);
        }
        for &sol in sols {
            let v = match h.view(sol) {
                View::App { head, args: &[r] } if b.sym_of(head) == Some(b.hd.list) => match h.view(r) {
                    View::App { args: &[k, v], .. } if k == x => v,
                    _ => return Ok(None),
                },
                _ => return Ok(None),
            };
            let Some(c @ Pt::Fin(_)) = point(cx, b, v)? else { return Ok(None) };
            if cmp(cx, b, c, p)? != Some(Ordering::Greater) || cmp(cx, b, c, q)? != Some(Ordering::Less) {
                continue;
            }
            // insert in order, once
            let mut at = cuts.len();
            for (i, &d) in cuts.iter().enumerate() {
                match cmp(cx, b, c, d)? {
                    Some(Ordering::Equal) => {
                        at = usize::MAX;
                        break;
                    }
                    Some(Ordering::Less) => {
                        at = i;
                        break;
                    }
                    Some(Ordering::Greater) => {}
                    None => return Ok(None),
                }
            }
            if at != usize::MAX {
                cuts.insert(at, c);
            }
            if cuts.len() > MAX_CUTS {
                return Ok(None);
            }
        }
    }
    Ok(Some(cuts))
}

/// Adds to `cuts` (ordered points strictly inside `(p, q)`) the jumps of the step functions among `switches`: the
/// points where their argument `a x + c` (a, c rational numbers) crosses an integer (a half-integer for `round`).
/// `false` when that is not possible: an argument that is not linear with rational coefficients, an unbounded
/// interval, more than [`MAX_CUTS`] cuts, or an undecided order.
pub fn step_cuts<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    b: &Build<'h>,
    switches: &[T<'h>],
    x: T<'h>,
    p: Pt<'h>,
    q: Pt<'h>,
    cuts: &mut Vec<Pt<'h>>,
) -> Res<bool> {
    let h = b.heap();
    let steps: Vec<(T<'h>, bool)> = switches
        .iter()
        .filter_map(|&s| match (switch(b, s), b.sym_of(match h.view(s) {
            View::App { head, .. } => head,
            _ => s,
        })) {
            (Some((Switch::Step, args)), Some(name)) => Some((args[0], h.sym_name(name) == "round")),
            _ => None,
        })
        .collect();
    if steps.is_empty() {
        return Ok(true);
    }
    let (Pt::Fin(l), Pt::Fin(r)) = (p, q) else { return Ok(false) };
    let float = |cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>| -> Res<Option<f64>> {
        let v = apply(cx, b, "n", &[t])?;
        Ok(match h.view(v) {
            View::Real(y) if y.is_finite() => Some(y),
            View::Int(k) => Some(k as f64),
            _ => None,
        })
    };
    for (u, half) in steps {
        // u = a x + c with rational a != 0, c
        let c = subst(cx, b, u, x, b.zero())?;
        let u1 = subst(cx, b, u, x, b.one())?;
        let a = cx.eval(b.sub(u1, c))?;
        let rational = |t: T<'h>| matches!(h.view(t), View::Int(_) | View::Rational { .. });
        if !rational(a) || !rational(c) || a == b.zero() {
            return Ok(false);
        }
        let back = cx.eval(b.sub(u, b.add(&[b.mul(&[a, x]), c])))?;
        let back = apply(cx, b, "expand", &[back])?;
        if back != b.zero() {
            return Ok(false);
        }
        let (tl, tr) = (subst(cx, b, u, x, l)?, subst(cx, b, u, x, r)?);
        let (Some(ul), Some(ur)) = (float(cx, tl)?, float(cx, tr)?) else { return Ok(false) };
        let (lo, hi) = (ul.min(ur), ul.max(ur));
        let shift = if half { 0.5 } else { 0.0 };
        let (k0, k1) = ((lo - shift).floor() as i64 - 1, (hi - shift).ceil() as i64 + 1);
        if k1 - k0 > 4 * MAX_CUTS as i64 {
            return Ok(false);
        }
        for k in k0..=k1 {
            cx.tick(1)?;
            // the argument's value at the jump, then x
            let jump = if half { b.add(&[b.int(k), b.rat(1, 2)]) } else { b.int(k) };
            let xv = cx.eval(b.div(b.sub(jump, c), a))?;
            let pt = Pt::Fin(xv);
            if cmp(cx, b, pt, p)? != Some(Ordering::Greater) || cmp(cx, b, pt, q)? != Some(Ordering::Less) {
                continue;
            }
            let mut at = cuts.len();
            for (i, &d) in cuts.iter().enumerate() {
                match cmp(cx, b, pt, d)? {
                    Some(Ordering::Equal) => {
                        at = usize::MAX;
                        break;
                    }
                    Some(Ordering::Less) => {
                        at = i;
                        break;
                    }
                    Some(Ordering::Greater) => {}
                    None => return Ok(false),
                }
            }
            if at != usize::MAX {
                cuts.insert(at, pt);
            }
            if cuts.len() > MAX_CUTS {
                return Ok(false);
            }
        }
    }
    Ok(true)
}

/// Is `t` a rational function of `x` (sums, products, integer powers; other subterms free of `x`)?
fn rational_in<'h>(b: &Build<'h>, t: T<'h>, x: T<'h>) -> bool {
    let h = b.heap();
    let mut stack = vec![t];
    while let Some(u) = stack.pop() {
        if u == x || is_free(b, u, x) {
            continue;
        }
        let View::App { head, args } = h.view(u) else { return false };
        match b.sym_of(head) {
            Some(s) if s == b.hd.add || s == b.hd.mul => stack.extend(args.iter().copied()),
            Some(s) if s == b.hd.pow && matches!(args, [_, e] if matches!(h.view(*e), View::Int(_))) => stack.push(args[0]),
            _ => return false,
        }
    }
    true
}

/// A point as a term.
pub fn term<'h>(b: &Build<'h>, p: Pt<'h>) -> T<'h> {
    let inf = b.heap().sym_term(b.heap().sym("inf"));
    match p {
        Pt::NegInf => b.neg(inf),
        Pt::Fin(t) => t,
        Pt::PosInf => inf,
    }
}

/// The differences `l - r` of the comparisons in a condition (`and`, `or`, `not` of comparisons); false for another
/// shape.
fn comparisons<'h>(b: &Build<'h>, c: T<'h>, out: &mut Vec<T<'h>>) -> bool {
    let h = b.heap();
    let mut stack = vec![c];
    while let Some(u) = stack.pop() {
        if u == h.sym_term(h.sym(names::TRUE)) || u == h.sym_term(h.sym(names::FALSE)) {
            continue;
        }
        let View::App { head, args } = h.view(u) else { return false };
        let Some(s) = b.sym_of(head) else { return false };
        match (h.sym_name(s).as_str(), args) {
            ("and" | "or" | "not", _) => stack.extend(args.iter().copied()),
            ("less" | "greater" | "less_equal" | "greater_equal" | "equal" | "unequal", [l, r]) => out.push(b.sub(*l, *r)),
            _ => return false,
        }
    }
    true
}

/// Is `c` true at `x = m`? `None` when not decided.
fn truth<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, c: T<'h>, x: T<'h>, m: T<'h>) -> Res<Option<bool>> {
    let h = b.heap();
    let (t, f) = (h.sym_term(h.sym(names::TRUE)), h.sym_term(h.sym(names::FALSE)));
    let mut v = subst(cx, b, c, x, m)?;
    if v != t && v != f {
        v = apply(cx, b, "n", &[v])?;
    }
    Ok(if v == t {
        Some(true)
    } else if v == f {
        Some(false)
    } else {
        None
    })
}

/// `f` with every switch of `x` replaced by its branch at `x = m` (bottom-up on an explicit stack), the step
/// functions too when `steps` (the caller cut at their jumps); `None` when a branch is not decided.
pub fn resolve<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, x: T<'h>, m: T<'h>, steps: bool) -> Res<Option<T<'h>>> {
    let h = b.heap();
    let mut memo: FxHashMap<T<'h>, T<'h>> = FxHashMap::default();
    let mut stack = vec![(f, false)];
    while let Some((u, ready)) = stack.pop() {
        if memo.contains_key(&u) {
            continue;
        }
        if is_free(b, u, x) {
            memo.insert(u, u);
            continue;
        }
        let View::App { head, args } = h.view(u) else {
            memo.insert(u, u);
            continue;
        };
        let kind = switch(b, u).map(|s| s.0).filter(|&k| steps || k != Switch::Step);
        // the conditions of `if` and `piecewise` are decided on the original terms, not rebuilt
        let children: Vec<T<'h>> = match kind {
            Some(Switch::If) => args[1..].to_vec(),
            Some(Switch::Piecewise) => match piecewise_parts(b, args) {
                Some((pieces, d)) => pieces.iter().map(|p| p.0).chain(d).collect(),
                None => return Ok(None),
            },
            _ => args.to_vec(),
        };
        if !ready {
            stack.push((u, true));
            stack.extend(children.iter().filter(|c| !memo.contains_key(c)).map(|&c| (c, false)));
            continue;
        }
        let na: Vec<T<'h>> = args.iter().map(|a| memo.get(a).copied().unwrap_or(*a)).collect();
        let r = match kind {
            None => {
                if na.as_slice() == args {
                    u
                } else {
                    b.app(head, &na)
                }
            }
            Some(Switch::Abs | Switch::Sign | Switch::UnitStep) => {
                let v = subst(cx, b, na[0], x, m)?;
                let Some(s) = sign(cx, b, v)? else { return Ok(None) };
                match kind {
                    Some(Switch::Abs) => {
                        if s < 0 {
                            b.neg(na[0])
                        } else {
                            na[0]
                        }
                    }
                    Some(Switch::Sign) => b.int(s as i64),
                    _ => b.int(i64::from(s >= 0)),
                }
            }
            Some(k @ (Switch::Max | Switch::Min)) => {
                let mut best = na[0];
                for &a in &na[1..] {
                    let d = subst(cx, b, b.sub(a, best), x, m)?;
                    let Some(s) = sign(cx, b, d)? else { return Ok(None) };
                    if (k == Switch::Max && s > 0) || (k == Switch::Min && s < 0) {
                        best = a;
                    }
                }
                best
            }
            Some(Switch::Step) => {
                // the value at the sample point; fractional_part(u) = u - integer_part(u)
                let View::App { head: sh, .. } = h.view(u) else { return Ok(None) };
                let name = b.sym_of(sh).map(|s| h.sym_name(s)).unwrap_or_default();
                let v = subst(cx, b, na[0], x, m)?;
                if name == "fractional_part" {
                    let k = apply(cx, b, "integer_part", &[v])?;
                    if !matches!(h.view(k), View::Int(_)) {
                        return Ok(None);
                    }
                    b.sub(na[0], k)
                } else {
                    let k = apply(cx, b, &name, &[v])?;
                    if !matches!(h.view(k), View::Int(_)) {
                        return Ok(None);
                    }
                    k
                }
            }
            Some(Switch::If) => match truth(cx, b, args[0], x, m)? {
                Some(true) => memo[&args[1]],
                Some(false) => memo[&args[2]],
                None => return Ok(None),
            },
            Some(Switch::Piecewise) => {
                let Some((pieces, d)) = piecewise_parts(b, args) else { return Ok(None) };
                let mut chosen = None;
                for (v, c) in pieces {
                    match truth(cx, b, c, x, m)? {
                        Some(true) => {
                            chosen = Some(memo[&v]);
                            break;
                        }
                        Some(false) => {}
                        None => return Ok(None),
                    }
                }
                chosen.unwrap_or_else(|| d.map_or(b.zero(), |d| memo[&d]))
            }
        };
        memo.insert(u, r);
    }
    Ok(Some(memo[&f]))
}

/// `f` with its switches of `x` replaced by their branches just beside `pt` (from above for `side` 1, from below
/// for -1; an infinity from its finite side), for one-sided limits; `None` when `f` has no switches or the branches
/// there are not decided.
pub fn resolve_near<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, x: T<'h>, pt: Pt<'h>, side: i8) -> Res<Option<T<'h>>> {
    // the step functions stay (their jumps are not cut points here)
    let switches: Vec<T<'h>> = switches_of(b, f, x).into_iter().filter(|&s| !matches!(switch(b, s), Some((Switch::Step, _)))).collect();
    if switches.is_empty() {
        return Ok(None);
    }
    let Some(gs) = zero_exprs(b, &switches) else { return Ok(None) };
    let m = match pt {
        Pt::Fin(c) => {
            let (p, q) = if side > 0 { (c, b.add(&[c, b.one()])) } else { (b.sub(c, b.one()), c) };
            let (p, q) = (cx.eval(p)?, cx.eval(q)?);
            let Some(cuts) = cut_points(cx, b, &gs, x, Pt::Fin(p), Pt::Fin(q))? else { return Ok(None) };
            let near = if side > 0 { cuts.first() } else { cuts.last() };
            let other = match near {
                Some(Pt::Fin(d)) => *d,
                Some(_) => return Ok(None),
                None if side > 0 => q,
                None => p,
            };
            b.mul(&[b.rat(1, 2), b.add(&[c, other])])
        }
        Pt::PosInf | Pt::NegInf => {
            // every zero of rational expressions; the sample beyond them all
            if gs.iter().any(|&g| !rational_in(b, g, x)) {
                return Ok(None);
            }
            let Some(cuts) = cut_points(cx, b, &gs, x, Pt::NegInf, Pt::PosInf)? else { return Ok(None) };
            match (pt, cuts.first(), cuts.last()) {
                (Pt::PosInf, _, Some(Pt::Fin(d))) => b.add(&[*d, b.one()]),
                (Pt::NegInf, Some(Pt::Fin(d)), _) => b.sub(*d, b.one()),
                (_, None, None) => b.zero(),
                _ => return Ok(None),
            }
        }
    };
    let m = cx.eval(m)?;
    match resolve(cx, b, f, x, m, false)? {
        Some(g) => Ok(Some(cx.eval(g)?)),
        None => Ok(None),
    }
}

/// `f` with the step-like functions of `x` written with `unit_step`, `abs` and `if` (which [`by_cases`] splits):
/// `heaviside_theta(u)` as `unit_step(u)`, `unit_box(u)` and `heaviside_pi(u)` as `unit_step(u + 1/2)
/// unit_step(1/2 - u)`, `unit_triangle(u)` and `heaviside_lambda(u)` as `(1 - abs(u)) unit_step(u + 1) unit_step(1 - u)`,
/// `boole(c)` as `if(c, 1, 0)`; values on the jumps (a set of measure zero) do not matter. `None` when there is
/// none.
pub fn as_steps<'h>(b: &Build<'h>, f: T<'h>, x: T<'h>) -> Option<T<'h>> {
    let mut memo = FxHashMap::default();
    let r = steps(b, f, x, &mut memo, 0);
    (r != f).then_some(r)
}

fn steps<'h>(b: &Build<'h>, t: T<'h>, x: T<'h>, memo: &mut FxHashMap<T<'h>, T<'h>>, depth: u32) -> T<'h> {
    let h = b.heap();
    if is_free(b, t, x) || depth > 100 {
        return t;
    }
    if let Some(&r) = memo.get(&t) {
        return r;
    }
    let View::App { head, args } = h.view(t) else { return t };
    let args: Vec<T<'h>> = args.iter().map(|&a| steps(b, a, x, memo, depth + 1)).collect();
    let name = b.sym_of(head).map(|s| h.sym_name(s));
    let half = b.rat(1, 2);
    let r = match (name.as_deref(), args.as_slice()) {
        (Some("heaviside_theta"), [u]) => b.call("unit_step", &[*u]),
        (Some("unit_box" | "heaviside_pi"), [u]) => {
            b.mul(&[b.call("unit_step", &[b.add(&[*u, half])]), b.call("unit_step", &[b.sub(half, *u)])])
        }
        (Some("unit_triangle" | "heaviside_lambda"), [u]) => b.mul(&[
            b.sub(b.one(), b.call("abs", &[*u])),
            b.call("unit_step", &[b.add(&[*u, b.one()])]),
            b.call("unit_step", &[b.sub(b.one(), *u)]),
        ]),
        (Some("boole"), [c]) => b.call("if", &[*c, b.one(), b.zero()]),
        _ => b.app(head, &args),
    };
    memo.insert(t, r);
    r
}

/// Does `t` hold a function of `x` with jumps (`round`, `floor`, steps, ...)?
fn has_jumps<'h>(b: &Build<'h>, t: T<'h>, x: T<'h>) -> bool {
    let h = b.heap();
    let mut stack = vec![t];
    while let Some(u) = stack.pop() {
        if is_free(b, u, x) {
            continue;
        }
        if let View::App { head, args } = h.view(u) {
            if let Some(s) = b.sym_of(head) {
                if matches!(
                    h.sym_name(s).as_str(),
                    "round" | "floor" | "ceiling" | "integer_part" | "fractional_part" | "mod" | "quotient" | "sign"
                        | "unit_step" | "heaviside_theta" | "boole" | "if" | "piecewise" | "prime_pi" | "real_sign"
                ) {
                    return true;
                }
            }
            stack.extend_from_slice(args);
        }
    }
    false
}
