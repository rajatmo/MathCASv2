//! Extrema on the whole line of a sum of two periodic parts with incommensurable periods
//! (`minimize(sin(sqrt(3) x) - exp(2 + cos(3 x)), x)`).
//!
//! The objective is `F1(x) + F2(x)`, where x occurs only in linear arguments `a x + b` of `sin`, `cos`, `tan`, `sec`,
//! `csc`, `cot` (each part proved continuous on the line), and the terms fall into two classes: within a class the slopes are in rational ratio (the class
//! is periodic, with the period `function_period` gives), across the classes the periods P1, P2 have the ratio
//! r = P1/P2 = c sqrt(D), c rational and D > 1 a squarefree integer, so r is irrational.
//!
//! By Kronecker's theorem the line `x -> (x/P1, x/P2)` is dense in the torus (R/Z)^2 when P1/P2 is irrational
//! (Hardy and Wright, *An Introduction to the Theory of Numbers*, ch. XXIII), so F1 + F2 comes arbitrarily close to
//! every value F1(s) + F2(t): the supremum is max F1 + max F2 and the infimum min F1 + min F2 (each over one
//! period, exact by the one-variable methods). It is attained exactly when a point s1 + k P1 where F1 is extreme
//! equals a point s2 + m P2 where F2 is (k, m integers): with d = (s1 - s2)/P2 = u + v sqrt(D) (u, v rational,
//! read from the expanded radical form), the equation `d + k r = m` holds exactly when `k = -v/c` and `m = u` are
//! integers, as 1 and sqrt(D) are linearly independent over the rationals. The extreme points in one period come
//! from `solve` (complete for the equation `F_i == extreme` on the period). Otherwise the extreme is not attained
//! and the point is `indeterminate`.
use crate::front::Front;
use mc_api::{names, Store, View, T};
use mc_eval::{NativeCx, Res};

const TRIG: [&str; 6] = ["sin", "cos", "tan", "sec", "csc", "cot"];

/// The extreme of `f` in `x` on the whole line (`max` for the maximum): the value and the point
/// (`indeterminate` when not attained); `None` when the objective is not of this shape or a step has no answer.
pub(crate) fn two_periods<'h>(cx: &mut NativeCx<'_, '_, 'h>, fr: &Front<'h>, max: bool, f: T<'h>, x: T<'h>) -> Res<Option<(T<'h>, T<'h>)>> {
    let h = cx.heap();
    let terms: Vec<T<'h>> = match fr.head(f) {
        Some((n, args)) if n == names::ADD => args.to_vec(),
        _ => return Ok(None),
    };
    // each term's slope (the slopes of one term must be in rational ratio); terms free of x join any class
    let mut classes: Vec<(T<'h>, Vec<T<'h>>)> = vec![];
    let mut constant = vec![];
    for &t in &terms {
        let mut slopes = vec![];
        if !linear_trig(cx, fr, t, x, &mut slopes)? {
            return Ok(None);
        }
        let Some(&s) = slopes.first() else {
            constant.push(t);
            continue;
        };
        for &o in &slopes[1..] {
            if !rational_ratio(cx, o, s)? {
                return Ok(None);
            }
        }
        let mut placed = false;
        for c in classes.iter_mut() {
            if rational_ratio(cx, s, c.0)? {
                c.1.push(t);
                placed = true;
                break;
            }
        }
        if !placed {
            classes.push((s, vec![t]));
        }
    }
    let [(_, a), (_, b)] = classes.as_slice() else { return Ok(None) };
    let mut f1 = a.clone();
    f1.extend(constant.iter().copied());
    let f1 = cx.eval(h.call(h.sym(names::ADD), &f1))?;
    let f2 = if b.len() == 1 { b[0] } else { cx.eval(h.call(h.sym(names::ADD), b))? };
    // the density argument needs both parts continuous on the line (a pole, as in tan(x), makes the extreme
    // infinite or the values jump): proved by function_continuous, otherwise no answer here
    for part in [f1, f2] {
        let ok = cx.eval(h.call(h.sym("function_continuous"), &[part, x]))?;
        if !cx.is_true(ok) {
            return Ok(None);
        }
    }
    // the periods and their ratio c sqrt(D)
    let Some(p1) = period(cx, fr, f1, x)? else { return Ok(None) };
    let Some(p2) = period(cx, fr, f2, x)? else { return Ok(None) };
    let r = cx.eval(h.call(h.sym("expand"), &[h.call(h.sym(names::MUL), &[p1, h.call(h.sym(names::POW), &[p2, h.int(-1)])])]))?;
    // r = c sqrt(D): irrational
    let Some(Quadratic { u: None, v: Some(c), root: Some(d) }) = quadratic(fr, r) else { return Ok(None) };
    // the extremes over one period
    let Some((m1, s1)) = extreme_points(cx, fr, max, f1, x, p1)? else { return Ok(None) };
    let Some((m2, s2)) = extreme_points(cx, fr, max, f2, x, p2)? else { return Ok(None) };
    let value = cx.eval(h.call(h.sym(names::ADD), &[m1, m2]))?;
    // attained where s1 + k p1 = s2 + m p2
    for &a1 in &s1 {
        for &a2 in &s2 {
            let dt = h.call(h.sym(names::MUL), &[h.call(h.sym(names::ADD), &[a1, h.call(h.sym(names::MUL), &[h.int(-1), a2])]), h.call(h.sym(names::POW), &[p2, h.int(-1)])]);
            let dt = cx.eval(h.call(h.sym("expand"), &[dt]))?;
            // d = u + v sqrt(D): d + k c sqrt(D) = m exactly when v + k c = 0 and u = m
            let Some(q) = quadratic(fr, dt) else { return Ok(None) };
            if q.root.is_some_and(|rt| rt != d) {
                // another square root: d + k r is irrational for every k, never an integer
                continue;
            }
            let v = q.v.unwrap_or(h.int(0));
            let u = q.u.unwrap_or(h.int(0));
            let k = cx.eval(h.call(h.sym(names::MUL), &[h.int(-1), v, h.call(h.sym(names::POW), &[c, h.int(-1)])]))?;
            let ints = matches!(h.view(k), View::Int(_)) && matches!(h.view(u), View::Int(_));
            if ints {
                let pt = cx.eval(h.call(h.sym(names::ADD), &[a1, h.call(h.sym(names::MUL), &[k, p1])]))?;
                let pt = cx.eval(h.call(h.sym("simplify"), &[pt]))?;
                return Ok(Some((value, pt)));
            }
        }
    }
    Ok(Some((value, h.sym_term(h.sym("indeterminate")))))
}

/// Does `x` occur in `t` only in linear arguments of the trigonometric functions? Their slopes go to `slopes`.
fn linear_trig<'h>(cx: &mut NativeCx<'_, '_, 'h>, fr: &Front<'h>, t: T<'h>, x: T<'h>, slopes: &mut Vec<T<'h>>) -> Res<bool> {
    let h = cx.heap();
    if t == x {
        return Ok(false);
    }
    let View::App { args, .. } = h.view(t) else { return Ok(true) };
    if let Some((n, &[a])) = fr.head(t) {
        if TRIG.contains(&n.as_str()) && contains(h, a, x) {
            let s = cx.eval(h.call(h.sym("d"), &[a, x]))?;
            let zero = cx.eval(h.call(h.sym("equal"), &[s, h.int(0)]))?;
            if contains(h, s, x) || cx.is_true(zero) {
                return Ok(false);
            }
            slopes.push(s);
            return Ok(true);
        }
    }
    for &a in args {
        if !linear_trig(cx, fr, a, x, slopes)? {
            return Ok(false);
        }
    }
    Ok(true)
}

fn contains<'h>(h: &'h mc_core::Heap, t: T<'h>, x: T<'h>) -> bool {
    let mut stack = mc_api::walk::Walk::new(t);
    while let Some(u) = stack.pop() {
        if u == x {
            return true;
        }
        if let View::App { args, .. } = h.view(u) {
            stack.extend(args.iter().copied());
        }
    }
    false
}

/// Is `a / b` a rational number?
fn rational_ratio<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: T<'h>, b: T<'h>) -> Res<bool> {
    let h = cx.heap();
    let q = cx.eval(h.call(h.sym("together"), &[h.call(h.sym(names::MUL), &[a, h.call(h.sym(names::POW), &[b, h.int(-1)])])]))?;
    Ok(matches!(h.view(q), View::Int(_) | View::Rational { .. }))
}

/// The positive period `function_period` gives, when it is one closed term.
fn period<'h>(cx: &mut NativeCx<'_, '_, 'h>, fr: &Front<'h>, f: T<'h>, x: T<'h>) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    let p = cx.eval(h.call(h.sym("function_period"), &[f, x]))?;
    if fr.head(p).is_some_and(|(n, _)| n == "function_period" || n == names::FAILURE || n == "list") || contains(h, p, x) {
        return Ok(None);
    }
    let pos = cx.eval(h.call(h.sym("greater"), &[p, h.int(0)]))?;
    Ok(cx.is_true(pos).then_some(p))
}

/// A rational number term?
fn is_rational<'h>(h: &'h mc_core::Heap, t: T<'h>) -> bool {
    matches!(h.view(t), View::Int(_) | View::Rational { .. })
}

/// An expanded `u + v sqrt(D)`: u and v rational (u absent when 0, v absent without an irrational part), D > 1 an
/// integer in its canonical (squarefree) form.
struct Quadratic<'h> {
    u: Option<T<'h>>,
    v: Option<T<'h>>,
    root: Option<T<'h>>,
}

/// `t` read as `u + v sqrt(D)`; `None` when a part is neither rational nor a rational times one `D^(1/2)`.
fn quadratic<'h>(fr: &Front<'h>, t: T<'h>) -> Option<Quadratic<'h>> {
    let h = fr.ar.store();
    let parts: Vec<T<'h>> = match fr.head(t) {
        Some((n, args)) if n == names::ADD => args.to_vec(),
        _ => vec![t],
    };
    let mut q = Quadratic { u: None, v: None, root: None };
    for p in parts {
        if is_rational(h, p) {
            if q.u.is_some() {
                return None;
            }
            q.u = Some(p);
            continue;
        }
        let (coef, rad) = match fr.head(p) {
            Some((n, [a, b])) if n == names::MUL && is_rational(h, *a) => (*a, *b),
            Some((n, _)) if n == names::POW => (h.int(1), p),
            _ => return None,
        };
        let Some((n, [base, e])) = fr.head(rad) else { return None };
        let half = matches!(h.view(*e), View::Rational { num, den } if matches!(h.view(num), View::Int(1)) && matches!(h.view(den), View::Int(2)));
        if n != names::POW || !matches!(h.view(*base), View::Int(k) if k > 1) || !half || q.v.is_some() {
            return None;
        }
        q.root = Some(rad);
        q.v = Some(coef);
    }
    Some(q)
}

/// The extreme value of `f` over one period [0, p] and every point of [0, p) where it is taken.
#[allow(clippy::type_complexity)]
fn extreme_points<'h>(cx: &mut NativeCx<'_, '_, 'h>, fr: &Front<'h>, max: bool, f: T<'h>, x: T<'h>, p: T<'h>) -> Res<Option<(T<'h>, Vec<T<'h>>)>> {
    let h = cx.heap();
    let lo = h.call(h.sym("less_equal"), &[h.int(0), x]);
    let hi = h.call(h.sym("less_equal"), &[x, p]);
    let dom = h.call(h.sym("and"), &[lo, hi]);
    let name = if max { "max_value" } else { "min_value" };
    let pair = cx.list(&[f, dom]);
    let m = cx.eval(h.call(h.sym(name), &[pair, x]))?;
    if fr.head(m).is_some_and(|(n, _)| n == name || n == names::FAILURE) || contains(h, m, x) {
        return Ok(None);
    }
    // a finite real value
    let nm = cx.eval(h.call(h.sym("n"), &[m]))?;
    let finite = match h.view(nm) {
        View::Real(v) => v.is_finite(),
        View::Int(_) => true,
        _ => false,
    };
    if !finite {
        return Ok(None);
    }
    // every point of [0, p) where f takes m
    let below = h.call(h.sym("less"), &[x, p]);
    let eq = h.call(h.sym("equal"), &[f, m]);
    let sys = h.call(h.sym("and"), &[eq, lo, below]);
    let s = cx.eval(h.call(h.sym("solve"), &[sys, x, h.sym_term(h.sym("reals"))]))?;
    let Some(items) = fr.list(s) else { return Ok(None) };
    let mut pts = vec![];
    for &it in items {
        let Some([r]) = fr.list(it) else { return Ok(None) };
        match fr.head(*r) {
            Some((n, [y, v])) if n == names::RULE && *y == x && !contains(h, *v, x) && !has_parameter(fr, *v) => pts.push(*v),
            _ => return Ok(None),
        }
    }
    if pts.is_empty() {
        return Ok(None);
    }
    Ok(Some((m, pts)))
}

/// Does `t` hold a generated parameter or a condition (`c(1)`, `conditional_expression`)?
fn has_parameter<'h>(fr: &Front<'h>, t: T<'h>) -> bool {
    let mut stack = mc_api::walk::Walk::new(t);
    while let Some(u) = stack.pop() {
        if let Some((n, args)) = fr.head(u) {
            if n == "c" || n == "conditional_expression" {
                return true;
            }
            stack.extend(args.iter().copied());
        }
    }
    false
}
