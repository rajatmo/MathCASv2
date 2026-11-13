//! Numbers with uncertainty (decision P6N.10): `around(x, dx)`, `interval([a, b], ...)` and
//! `centered_interval(c, r)`, carried through arithmetic and through the functions of this crate and of the
//! special-function crates.
//!
//! - `around(x, dx)`: a machine value with a standard uncertainty, propagated to first order assuming independent
//!   arguments: f(x1 +- d1, ...) is f(x) +- sqrt(sum (df/dxi di)^2), the derivatives by `d`.
//! - `interval([a, b], ...)`: a union of closed intervals (sorted, overlapping ones merged); the image of an
//!   interval under a function is spanned by its values at the ends and at the critical points inside (found from
//!   the derivative's sign changes on a grid of 128 cells and refined by bisection), exact when the function is
//!   monotone there and the ends are exact; sums, products, powers and quotients by interval arithmetic (a
//!   quotient by an interval containing 0 splits).
//! - `centered_interval(c, r)`: a midpoint and a radius: f(c +- r) of one spread is the image spanned by the ends and
//!   the critical points (the tight hull); where that fails, f(c) +- r max |f'| over c - r, c, c + r (an estimate);
//!   functions of several spreads other than sums and products are evaluated on balls (rigorous), or stay
//!   unevaluated when the function does not give a ball (round 36); sums, products and
//!   powers with a complex part, an `interval` mixed in or an uncertain exponent go through the rigorous box
//!   arithmetic of `uncertain_boxes` (a complex radius rx + i ry is the box of the two parts);
//!   `centered_interval([[m, e, rm, re], p])` reads the mantissa/exponent form m 2^e +- rm 2^re.
//!
//! The engine's `add`, `mul` and `pow` call [`arith`] first when an argument is one of these; the natives of the
//! elementary and special functions call [`apply`].
use crate::formula::{Def, Fx};
use mc_api::{names, Store, View, T};
use mc_arith::{Arith, Kind, Num, Real};
use mc_core::Heap;
use mc_eval::{NativeCx, Res};

/// The heads carried through arithmetic and functions.
pub const HEADS: [&str; 4] = ["around", "interval", "centered_interval", "quantity"];

/// Is one of the arguments an uncertain number?
pub fn any(h: &Heap, args: &[T<'_>]) -> bool {
    args.iter().any(|&a| head_of(h, a).is_some())
}

fn head_of(h: &Heap, t: T<'_>) -> Option<usize> {
    let View::App { head, .. } = h.view(t) else { return None };
    let View::Sym(s) = h.view(head) else { return None };
    let n = h.sym_name(s);
    HEADS.iter().position(|x| *x == n)
}

/// The arithmetic operations.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Op {
    /// a sum
    Add,
    /// a product
    Mul,
    /// a power
    Pow,
}

fn op_name(op: Op) -> &'static str {
    match op {
        Op::Add => names::ADD,
        Op::Mul => names::MUL,
        Op::Pow => names::POW,
    }
}

/// `add`, `mul` or `pow` with an uncertain argument: the uncertain result, or `None` to leave the call to the
/// canonical arithmetic.
pub fn arith<'h>(cx: &mut NativeCx<'_, '_, 'h>, op: Op, args: &[T<'h>]) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    let call = h.call(h.sym(op_name(op)), args);
    let mut fx = Fx { cx, ar: Arith::new(h), h };
    apply_in(&mut fx, call, args)
}

/// f(args) with an uncertain argument (the call `t`): the uncertain result or `None`.
pub fn apply<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    let mut fx = Fx { cx, ar: Arith::new(h), h };
    apply_in(&mut fx, t, args)
}

fn apply_in<'h>(fx: &mut Fx<'_, '_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Option<T<'h>>> {
    let kinds: Vec<Option<usize>> = args.iter().map(|&a| head_of(fx.h, a)).collect();
    let Some(kind) = kinds.iter().flatten().copied().next() else { return Ok(None) };
    // sums, products and powers of centered intervals with complex parts, with intervals, or with an uncertain
    // exponent: rectangular box arithmetic (rigorous)
    if let Some(r) = crate::uncertain_boxes::arith(fx, t, args, &kinds)? {
        return Ok(Some(r));
    }
    // the real and imaginary parts, conjugate, modulus and argument of a centered interval with a complex part
    if let Some(r) = crate::uncertain_boxes::part(fx, t, args)? {
        return Ok(Some(r));
    }
    // one kind at a time; other arguments must be numbers or closed numeric terms
    if kinds.iter().flatten().any(|&k| k != kind) {
        return Ok(None);
    }
    // an indeterminate argument makes the whole result indeterminate
    let indeterminate = fx.sym("indeterminate");
    if args.contains(&indeterminate) {
        return Ok(Some(indeterminate));
    }
    let View::App { head, .. } = fx.h.view(t) else { return Ok(None) };
    match kind {
        0 => around_apply(fx, head, args),
        1 => interval_apply(fx, head, args),
        2 => centered_apply(fx, head, args),
        _ => {
            let View::Sym(s) = fx.h.view(head) else { return Ok(None) };
            let name = fx.h.sym_name(s);
            crate::quantity::arith(fx, &name, args)
        }
    }
}

// ---------------------------------------------------------------- around

fn machine<'h>(fx: &mut Fx<'_, '_, '_, 'h>, t: T<'h>) -> Res<Option<f64>> {
    let v = fx.eval(fx.call("n", &[t]))?;
    Ok(fx.f64_of(v))
}

/// A list (of lists) of numbers, possibly empty.
fn numeric_list<'h>(fx: &Fx<'_, '_, '_, 'h>, t: T<'h>) -> bool {
    match fx.h.view(t) {
        View::App { head, args } if head == fx.h.sym_term(fx.h.sym(names::LIST)) => {
            args.iter().all(|&a| fx.num(a).is_some() || numeric_list(fx, a))
        }
        _ => false,
    }
}

fn around_apply<'h>(fx: &mut Fx<'_, '_, '_, 'h>, head: T<'h>, args: &[T<'h>]) -> Res<Option<T<'h>>> {
    propagate(fx, head, args, "around")
}

/// First-order propagation for `around` (root sum of squares of the slopes times the uncertainties) and for
/// `centered_interval` (the sum of the largest slope at c - r, c, c + r times r, a midpoint-radius estimate).
fn propagate<'h>(fx: &mut Fx<'_, '_, '_, 'h>, head: T<'h>, args: &[T<'h>], kind: &str) -> Res<Option<T<'h>>> {
    let centered = kind == "centered_interval";
    let mut centers = vec![];
    let mut unc = vec![];
    for &a in args {
        let parts = match fx.call_of(a, kind) {
            Some(&[x, d]) => Some((x, d)),
            _ => None,
        };
        match parts {
            Some((x, d)) => {
                centers.push(x);
                unc.push(Some(d));
            }
            None => {
                // a parameter list of numbers (meijer_g([[], []], [[5], [4]], around(0.2, 0.01))) is a constant
                // argument; without it the function became its closed form and its correlated factors were
                // propagated as independent (radius 6.5e-5 for the true 7.08e-5)
                if fx.num(a).is_none() && !numeric_list(fx, a) && machine(fx, a)?.is_none() {
                    return Ok(None);
                }
                centers.push(a);
                unc.push(None);
            }
        }
    }
    // functions constant between jumps: the value where it is the same over the whole spread, else indeterminate
    if let View::Sym(s) = fx.h.view(head) {
        if STEP_FUNCTIONS.contains(&fx.h.sym_name(s).as_str()) {
            return step(fx, head, &centers, &unc);
        }
    }
    // the conjugate of x -+ dx is conj(x) -+ dx (conjugation keeps distances)
    if !centered && centers.len() == 1 && matches!(fx.h.view(head), View::Sym(s) if fx.h.sym_name(s) == "conjugate") {
        if let Some(d) = unc[0] {
            let c = match (machine(fx, fx.call("re", &[centers[0]]))?, machine(fx, fx.call("im_part", &[centers[0]]))?) {
                (Some(a), Some(b)) if b != 0.0 => fx.ar.complex(fx.real(a), fx.real(-b)),
                _ => fx.eval(fx.call("conjugate", &[centers[0]]))?,
            };
            return Ok(Some(fx.call(kind, &[c, d])));
        }
    }
    let value = fx.eval(fx.h.app(head, &centers))?;
    // `around` of a real argument with a complex value (acoth(0.9), the inverse Gudermannian beyond pi/2): the value
    // with the spread |f'(x)| dx of the first order
    let complex_value = if centered || machine(fx, value)?.is_some() {
        None
    } else {
        let (re, im) = (machine(fx, fx.call("re", &[value]))?, machine(fx, fx.call("im_part", &[value]))?);
        match (re, im) {
            (Some(a), Some(b)) if a.is_finite() && b.is_finite() && unc.iter().flatten().count() == 1 => Some(fx.eval(fx.call("n", &[value]))?),
            _ => None,
        }
    };
    let v = match (machine(fx, value)?, complex_value) {
        (Some(v), _) => v,
        (None, Some(_)) => 0.0,
        (None, None) => return Ok(None),
    };
    if centered {
        if let Some(r) = centered_image(fx, head, &centers, &unc)? {
            return Ok(Some(r));
        }
    }
    // sums and products of several centered intervals: the exact ball rules (radii add; (c1, r1)(c2, r2) has the
    // radius |c1| r2 + |c2| r1 + r1 r2), rigorous where the slope estimate is not
    if centered && unc.iter().filter(|d| d.is_some()).count() > 1 {
        if let View::Sym(sy) = fx.h.view(head) {
            let name = fx.h.sym_name(sy);
            if name == names::ADD || name == names::MUL {
                let mut acc: Option<(f64, f64)> = None;
                for (i, d) in unc.iter().enumerate() {
                    let (Some(c), r) = (machine(fx, centers[i])?, match d {
                        Some(d) => machine(fx, *d)?,
                        None => Some(0.0),
                    }) else { return Ok(None) };
                    let Some(r) = r else { return Ok(None) };
                    acc = Some(match acc {
                        None => (c, r),
                        Some((c0, r0)) if name == names::ADD => (c0 + c, r0 + r),
                        Some((c0, r0)) => (c0 * c, c0.abs() * r + c.abs() * r0 + r0 * r),
                    });
                }
                if let Some((c, r)) = acc {
                    if c.is_finite() && r.is_finite() {
                        let r = (r + 4.0 * f64::EPSILON * c.abs()) * (1.0 + 8.0 * f64::EPSILON);
                        return Ok(Some(fx.call(kind, &[fx.real(c), fx.real(r)])));
                    }
                }
            }
        }
        // other functions of several centered intervals: the corners of the box where the function is monotone in
        // each argument, else the function of the balls (the special-function natives evaluate ball arguments with
        // rigorous error bounds), else unevaluated; the slope estimate below is not an enclosure when the slopes vary
        // over the spreads (round 36)
        if let Some(r) = centered_corners(fx, head, &centers, &unc)? {
            return Ok(Some(r));
        }
        return centered_by_balls(fx, head, &centers, &unc);
    }
    let var = fx.sym("around·t");
    let mut var2 = 0.0;
    for (i, d) in unc.iter().enumerate() {
        let Some(d) = d else { continue };
        let Some(d) = machine(fx, *d)? else { return Ok(None) };
        let mut xs = centers.clone();
        xs[i] = var;
        let g = fx.h.app(head, &xs);
        let dg = fx.eval(fx.call("d", &[g, var]))?;
        let Some(c) = machine(fx, centers[i])? else { return Ok(None) };
        let points: &[f64] = if centered { &[-1.0, 0.0, 1.0] } else { &[0.0] };
        let mut slope: f64 = 0.0;
        for &k in points {
            let at = fx.eval(fx.call("rewrite", &[dg, fx.call("rule", &[var, fx.real(c + k * d)])]))?;
            let s = match machine(fx, at)? {
                Some(s) if s.is_finite() => s,
                // a complex slope (a real argument, a complex value): its modulus
                _ if complex_value.is_some() => match machine(fx, fx.call("abs", &[at]))? {
                    Some(s) if s.is_finite() => s,
                    _ => return Ok(None),
                },
                // no derivative in closed form: a five-point difference quotient of the values
                _ => match numeric_slope(fx, head, &centers, i, c + k * d)? {
                    Some(s) => s,
                    None => return Ok(None),
                },
            };
            slope = slope.max(s.abs());
        }
        if centered {
            var2 += slope * d;
        } else {
            var2 += (slope * d) * (slope * d);
        }
    }
    let spread = if centered { var2 } else { var2.sqrt() };
    let value = complex_value.unwrap_or_else(|| fx.real(v));
    Ok(Some(fx.call(kind, &[value, fx.real(spread)])))
}

/// `head` of several real centered intervals (at most four) as the hull of its values at two corners of the box,
/// when every partial derivative (in closed form, else a central difference within the spread) keeps its sign on a
/// grid over the box (5 points a side for two
/// spreads, 3 for three or four): the function is then monotone in each argument, its least value is at the corner
/// of the lower ends of the increasing arguments and the upper ends of the decreasing ones, its largest at the
/// opposite corner (as [`centered_image`] for one spread). The half-width is rounded up; `None` otherwise.
fn centered_corners<'h>(fx: &mut Fx<'_, '_, '_, 'h>, head: T<'h>, centers: &[T<'h>], unc: &[Option<T<'h>>]) -> Res<Option<T<'h>>> {
    let idx: Vec<usize> = (0..unc.len()).filter(|&i| unc[i].is_some()).collect();
    let k = idx.len();
    if !(2..=4).contains(&k) {
        return Ok(None);
    }
    let mut spreads = vec![];
    for &i in &idx {
        let (Some(c), Some(d)) = (machine(fx, centers[i])?, machine(fx, unc[i].expect("uncertain"))?) else { return Ok(None) };
        if !(c.is_finite() && d.is_finite()) {
            return Ok(None);
        }
        spreads.push((c, d.abs()));
    }
    let vars: Vec<T<'h>> = (0..k).map(|j| fx.sym(&format!("around·t{j}"))).collect();
    let mut xs = centers.to_vec();
    for (j, &i) in idx.iter().enumerate() {
        xs[i] = vars[j];
    }
    let g = fx.h.app(head, &xs);
    // the partial derivatives in closed form; `None` where there is none (a central difference of the values then)
    let mut partials = vec![];
    for &v in &vars {
        let dg = fx.eval(fx.call("d", &[g, v]))?;
        partials.push((!(fx.contains(dg, "derivative") || fx.contains(dg, "d"))).then_some(dg));
    }
    let at_point = |fx: &mut Fx<'_, '_, '_, 'h>, e: T<'h>, pt: &[f64]| -> Res<Option<f64>> {
        let rules: Vec<T<'h>> = pt.iter().enumerate().map(|(j, &x)| fx.call("rule", &[vars[j], fx.real(x)])).collect();
        let rl = fx.list(&rules);
        let v = fx.eval(fx.call("rewrite", &[e, rl]))?;
        machine(fx, v)
    };
    let side: &[f64] = if k == 2 { &[-1.0, -0.5, 0.0, 0.5, 1.0] } else { &[-1.0, 0.0, 1.0] };
    let mut signs = vec![0i32; k];
    let total = side.len().pow(k as u32);
    for p in 0..total {
        let mut pt = vec![];
        let mut q = p;
        for &(c, d) in &spreads {
            let t = side[q % side.len()];
            q /= side.len();
            pt.push(c + t * d);
        }
        for (j, dg) in partials.iter().enumerate() {
            let s = match dg {
                Some(dg) => at_point(fx, *dg, &pt)?,
                None => {
                    // f(x + h) - f(x - h) with h within the spread (its sign is the slope's)
                    let h = (spreads[j].1 / 8.0).min(1e-4 * pt[j].abs().max(1.0));
                    let (mut up, mut dn) = (pt.clone(), pt.clone());
                    up[j] += h;
                    dn[j] -= h;
                    match (at_point(fx, g, &up)?, at_point(fx, g, &dn)?) {
                        (Some(a), Some(b)) => Some(a - b),
                        _ => None,
                    }
                }
            };
            let Some(s) = s else { return Ok(None) };
            if !s.is_finite() {
                return Ok(None);
            }
            let sg = if s > 0.0 { 1 } else if s < 0.0 { -1 } else { 0 };
            if sg != 0 {
                if signs[j] != 0 && signs[j] != sg {
                    return Ok(None);
                }
                signs[j] = sg;
            }
        }
        fx.tick(1)?;
    }
    let mut ends = [0.0f64; 2];
    for (e, dir) in [-1.0, 1.0].into_iter().enumerate() {
        let mut ys = centers.to_vec();
        for (j, &i) in idx.iter().enumerate() {
            let (c, d) = spreads[j];
            let s = if signs[j] < 0 { -dir } else { dir };
            ys[i] = fx.real(c + s * d);
        }
        let v = fx.eval(fx.h.app(head, &ys))?;
        match machine(fx, v)? {
            Some(y) if y.is_finite() => ends[e] = y,
            _ => return Ok(None),
        }
    }
    let (lo, hi) = (ends[0].min(ends[1]), ends[0].max(ends[1]));
    let mid = 0.5 * (lo + hi);
    let rad = (0.5 * (hi - lo) + 4.0 * f64::EPSILON * (lo.abs() + hi.abs())) * (1.0 + 4.0 * f64::EPSILON);
    Ok(Some(fx.call("centered_interval", &[fx.real(mid), fx.real(rad)])))
}

/// `head` of several centered intervals as the function of balls: each centred interval c +- r becomes the real ball
/// of midpoint c and radius r (exact arguments stay exact), and a real ball value is the result as a centered interval;
/// `None` (unevaluated) when an argument is not a real machine or exact spread or the value is not a ball.
fn centered_by_balls<'h>(fx: &mut Fx<'_, '_, '_, 'h>, head: T<'h>, centers: &[T<'h>], unc: &[Option<T<'h>>]) -> Res<Option<T<'h>>> {
    let mut xs = Vec::with_capacity(centers.len());
    for (i, d) in unc.iter().enumerate() {
        let Some(d) = d else {
            xs.push(centers[i]);
            continue;
        };
        let real = |fx: &Fx<'_, '_, '_, 'h>, t: T<'h>| fx.num(t).is_some_and(|n| n.as_real().is_some());
        if !(real(fx, centers[i]) && real(fx, *d)) {
            return Ok(None);
        }
        let r = fx.eval(fx.call("abs", &[*d]))?;
        let Some(b) = fx.ar.ball_with_radius(centers[i], r, BALL_BITS as u64) else { return Ok(None) };
        xs.push(b);
    }
    let v = fx.eval(fx.h.app(head, &xs))?;
    if !matches!(fx.num(v), Some(Num::Real(Real::Ball(..)))) {
        return Ok(None);
    }
    Ok(from_ball(fx, v))
}

/// `centered_interval` result of a function of one uncertain argument as the tight image of the spread: the midpoint
/// and half-width of the values at the two ends (the function monotone on the spread), or of the interval image
/// when the derivative changes sign on a nine-point grid; `None` when the argument is not a single non-degenerate
/// spread or a value is not machine (the first-order estimate then applies). The half-width is rounded up.
fn centered_image<'h>(fx: &mut Fx<'_, '_, '_, 'h>, head: T<'h>, centers: &[T<'h>], unc: &[Option<T<'h>>]) -> Res<Option<T<'h>>> {
    let mut which = unc.iter().enumerate().filter(|(_, d)| d.is_some());
    let (Some((i, Some(d))), None) = (which.next(), which.next()) else { return Ok(None) };
    let (Some(c), Some(d)) = (machine(fx, centers[i])?, machine(fx, *d)?) else { return Ok(None) };
    if d.is_nan() || d <= 0.0 || !d.is_finite() || !c.is_finite() {
        return Ok(None);
    }
    let var = fx.sym("around·t");
    let mut xs = centers.to_vec();
    xs[i] = var;
    let g = fx.h.app(head, &xs);
    let dg = fx.eval(fx.call("d", &[g, var]))?;
    if fx.contains(dg, "derivative") || fx.contains(dg, "d") {
        return Ok(None);
    }
    let mut sign = 0i32;
    let mut monotone = true;
    for k in 0..=8 {
        let x = c - d + 2.0 * d * (k as f64) / 8.0;
        let at = fx.eval(fx.call("rewrite", &[dg, fx.call("rule", &[var, fx.real(x)])]))?;
        let Some(s) = machine(fx, at)? else { return Ok(None) };
        if !s.is_finite() {
            return Ok(None);
        }
        let sg = if s > 0.0 { 1 } else if s < 0.0 { -1 } else { 0 };
        if sg != 0 {
            if sign != 0 && sg != sign {
                monotone = false;
            }
            sign = sg;
        }
    }
    let (lo, hi) = if monotone {
        let mut vals = [0.0; 2];
        for (k, x) in [c - d, c + d].into_iter().enumerate() {
            let mut ys = centers.to_vec();
            ys[i] = fx.real(x);
            let v = fx.eval(fx.h.app(head, &ys))?;
            match machine(fx, v)? {
                Some(y) if y.is_finite() => vals[k] = y,
                _ => return Ok(None),
            }
        }
        (vals[0].min(vals[1]), vals[0].max(vals[1]))
    } else {
        let Some((a, b)) = image(fx, head, centers, i, (fx.real(c - d), fx.real(c + d)))? else { return Ok(None) };
        match (machine(fx, a)?, machine(fx, b)?) {
            (Some(a), Some(b)) if a.is_finite() && b.is_finite() => (a.min(b), a.max(b)),
            _ => return Ok(None),
        }
    };
    let mid = 0.5 * (lo + hi);
    let rad = (0.5 * (hi - lo) + 4.0 * f64::EPSILON * (lo.abs() + hi.abs())) * (1.0 + 4.0 * f64::EPSILON);
    Ok(Some(fx.call("centered_interval", &[fx.real(mid), fx.real(rad)])))
}

/// The functions that are constant between jumps (their derivative is 0 almost everywhere).
const STEP_FUNCTIONS: &[&str] =
    &["floor", "ceiling", "round", "integer_part", "sign", "unit_step", "real_sign", "unit_box", "heaviside_theta", "kronecker_delta", "boole"];

/// A step function of uncertain arguments: its value when the ends of every spread and the centre give the same value,
/// otherwise `indeterminate`.
fn step<'h>(fx: &mut Fx<'_, '_, '_, 'h>, head: T<'h>, centers: &[T<'h>], unc: &[Option<T<'h>>]) -> Res<Option<T<'h>>> {
    let value = fx.eval(fx.h.app(head, centers))?;
    let Some(v) = machine(fx, value)? else { return Ok(None) };
    for (i, d) in unc.iter().enumerate() {
        let Some(d) = d else { continue };
        let (Some(c), Some(d)) = (machine(fx, centers[i])?, machine(fx, *d)?) else { return Ok(None) };
        for x in [c - d, c + d] {
            let mut xs = centers.to_vec();
            xs[i] = fx.real(x);
            let w = fx.eval(fx.h.app(head, &xs))?;
            if machine(fx, w)? != Some(v) {
                return Ok(Some(fx.sym("indeterminate")));
            }
        }
    }
    // the exact value (an integer) when it is one
    Ok(Some(match fx.num(value) {
        Some(_) if v.fract() == 0.0 && v.abs() < 2f64.powi(53) => fx.int(v as i64),
        _ => value,
    }))
}

/// The derivative of `head(args)` in argument `i` at the machine value `x` by the five-point difference
/// (f(x - 2h) - 8 f(x - h) + 8 f(x + h) - f(x + 2h)) / 12h, h = 10^-3 max(1, |x|) (error O(h^4)).
fn numeric_slope<'h>(fx: &mut Fx<'_, '_, '_, 'h>, head: T<'h>, args: &[T<'h>], i: usize, x: f64) -> Res<Option<f64>> {
    let h = 1e-3 * x.abs().max(1.0);
    let mut f = [0.0; 4];
    for (k, off) in [-2.0, -1.0, 1.0, 2.0].iter().enumerate() {
        let mut xs = args.to_vec();
        xs[i] = fx.real(x + off * h);
        let v = fx.eval(fx.h.app(head, &xs))?;
        match machine(fx, v)? {
            Some(y) if y.is_finite() => f[k] = y,
            _ => return Ok(None),
        }
    }
    Ok(Some((f[0] - 8.0 * f[1] + 8.0 * f[2] - f[3]) / (12.0 * h)))
}

// ---------------------------------------------------------------- interval

/// The pieces [a, b] of an interval.
fn pieces<'h>(fx: &Fx<'_, '_, '_, 'h>, t: T<'h>) -> Option<Vec<(T<'h>, T<'h>)>> {
    let ps = fx.call_of(t, "interval")?;
    ps.iter()
        .map(|&p| match fx.as_list(p)? {
            &[a, b] => Some((a, b)),
            _ => None,
        })
        .collect()
}

/// a < b for real numbers or closed real terms (by refinement), `None` when undecided.
fn less<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: T<'h>, b: T<'h>) -> Res<Option<bool>> {
    let v = fx.eval(fx.call("less", &[a, b]))?;
    Ok(if v == fx.bool(true) {
        Some(true)
    } else if v == fx.bool(false) {
        Some(false)
    } else {
        None
    })
}

/// The interval spanned by the values (smallest and largest, compared by refinement).
fn span<'h>(fx: &mut Fx<'_, '_, '_, 'h>, vals: &[T<'h>]) -> Res<Option<(T<'h>, T<'h>)>> {
    let (mut lo, mut hi) = (vals[0], vals[0]);
    for &v in &vals[1..] {
        if less(fx, v, lo)? == Some(true) {
            lo = v;
        } else if less(fx, lo, v)?.is_none() {
            return Ok(None);
        }
        if less(fx, hi, v)? == Some(true) {
            hi = v;
        } else if less(fx, v, hi)?.is_none() && v != hi {
            return Ok(None);
        }
    }
    Ok(Some((lo, hi)))
}

/// `interval(pieces...)` with the pieces sorted and overlapping ones merged.
fn make_interval<'h>(fx: &mut Fx<'_, '_, '_, 'h>, mut ps: Vec<(T<'h>, T<'h>)>) -> Res<Option<T<'h>>> {
    // sort by the lower ends
    let n = ps.len();
    for i in 1..n {
        let mut j = i;
        while j > 0 {
            match less(fx, ps[j].0, ps[j - 1].0)? {
                Some(true) => ps.swap(j, j - 1),
                Some(false) => break,
                None => return Ok(None),
            }
            j -= 1;
        }
    }
    let mut out: Vec<(T<'h>, T<'h>)> = vec![];
    for p in ps {
        if let Some(last) = out.last_mut() {
            if less(fx, last.1, p.0)? == Some(false) {
                if less(fx, last.1, p.1)? == Some(true) {
                    last.1 = p.1;
                }
                continue;
            }
        }
        out.push(p);
    }
    let items: Vec<T<'h>> = out.iter().map(|&(a, b)| fx.list(&[a, b])).collect();
    Ok(Some(fx.call("interval", &items)))
}

fn neg_inf<'h>(fx: &Fx<'_, '_, '_, 'h>) -> T<'h> {
    fx.neg(fx.inf())
}

/// The machine ends of computed pieces moved outwards by one unit in the last place (round 36): a machine result is
/// rounded to nearest, so the computed end can fall inside the true range (pi^3. gave 31.006276680299816 below
/// pi^3 = 31.00627668029982...); exact ends stay as they are.
fn outward<'h>(fx: &Fx<'_, '_, '_, 'h>, ps: Vec<(T<'h>, T<'h>)>) -> Vec<(T<'h>, T<'h>)> {
    let step = |t: T<'h>, up: bool| match fx.machine_of(t) {
        Some(v) if v.is_finite() && v != 0.0 => fx.real(if up { v.next_up() } else { v.next_down() }),
        _ => t,
    };
    ps.into_iter().map(|(a, b)| (step(a, false), step(b, true))).collect()
}

/// Interval arithmetic of two pieces.
fn piece_op<'h>(fx: &mut Fx<'_, '_, '_, 'h>, op: Op, x: (T<'h>, T<'h>), y: (T<'h>, T<'h>)) -> Res<Option<Vec<(T<'h>, T<'h>)>>> {
    match op {
        Op::Add => {
            let lo = fx.eval(fx.add(&[x.0, y.0]))?;
            let hi = fx.eval(fx.add(&[x.1, y.1]))?;
            Ok(Some(vec![(lo, hi)]))
        }
        Op::Mul => {
            let mut vals = vec![];
            for a in [x.0, x.1] {
                for b in [y.0, y.1] {
                    vals.push(fx.eval(fx.mul(&[a, b]))?);
                }
            }
            Ok(span(fx, &vals)?.map(|s| vec![s]))
        }
        Op::Pow => Ok(None),
    }
}

/// x^k for an interval piece and an exact integer or rational k.
fn piece_pow<'h>(fx: &mut Fx<'_, '_, '_, 'h>, x: (T<'h>, T<'h>), k: T<'h>) -> Res<Option<Vec<(T<'h>, T<'h>)>>> {
    let zero = fx.int(0);
    let contains_zero = less(fx, x.0, zero)? != Some(false) && less(fx, zero, x.1)? != Some(false);
    let negative_k = fx.num(k).is_some_and(|n| n.as_real().is_some_and(Real::is_negative));
    if negative_k && contains_zero {
        // 1/x over an interval with 0 inside splits (odd integer powers) or reaches infinity
        let e = fx.int_of(k);
        if x.0 == zero || x.1 == zero {
            let other = if x.0 == zero { x.1 } else { x.0 };
            let v = fx.eval(fx.pow(other, k))?;
            return Ok(Some(vec![if x.0 == zero { (v, fx.inf()) } else { (neg_inf(fx), v) }]));
        }
        if let Some(e) = e.filter(|e| e % 2 != 0) {
            let a = fx.eval(fx.pow(x.0, fx.int(e)))?;
            let b = fx.eval(fx.pow(x.1, fx.int(e)))?;
            return Ok(Some(vec![(neg_inf(fx), a), (b, fx.inf())]));
        }
        return Ok(None);
    }
    let a = fx.eval(fx.pow(x.0, k))?;
    let b = fx.eval(fx.pow(x.1, k))?;
    let even = fx.int_of(k).is_some_and(|e| e % 2 == 0);
    if even && contains_zero {
        let (_, hi) = match span(fx, &[a, b])? {
            Some(s) => s,
            None => return Ok(None),
        };
        return Ok(Some(vec![(zero, hi)]));
    }
    Ok(span(fx, &[a, b])?.map(|s| vec![s]))
}

/// The image of the interval [a, b] (argument `i` of the call) under the function `head` with the other arguments
/// fixed.
fn image<'h>(
    fx: &mut Fx<'_, '_, '_, 'h>,
    head: T<'h>,
    args: &[T<'h>],
    i: usize,
    (a, b): (T<'h>, T<'h>),
) -> Res<Option<(T<'h>, T<'h>)>> {
    let var = fx.sym("interval·t");
    let mut xs = args.to_vec();
    xs[i] = var;
    let g = fx.h.app(head, &xs);
    let at = |fx: &mut Fx<'_, '_, '_, 'h>, x: T<'h>| -> Res<T<'h>> {
        let mut ys = args.to_vec();
        ys[i] = x;
        fx.eval(fx.h.app(head, &ys))
    };
    let (Some(fa), Some(fb)) = (machine(fx, a)?, machine(fx, b)?) else { return Ok(None) };
    // an end where the function has a pole (csch at 0, acoth at 1): the image reaches the infinity on the side the
    // values just inside the end have; the grid leaves that end out
    let ga0 = at(fx, a)?;
    let gb0 = at(fx, b)?;
    let complex_infinity = fx.sym("complex_infinity");
    // a pole: an infinite value, or no value where the values grow without bound towards the end (at least a
    // hundredfold as the distance shrinks a hundredfold, from beyond 10^6)
    let pole = |fx: &mut Fx<'_, '_, '_, 'h>, v: T<'h>, x: f64, inward: f64| -> Res<bool> {
        if v == complex_infinity || fx.infinity(v).is_some_and(|d| d == 0 || d == 2) {
            return Ok(true);
        }
        match machine(fx, v)? {
            // a finite value: no pole
            Some(y) if y.is_finite() => return Ok(false),
            // an infinite numeric value (n(csch(0)) is inf): the side is read from the values inside
            Some(_) => return Ok(true),
            None => {}
        }
        if !inward.is_finite() || inward == 0.0 {
            return Ok(false);
        }
        let mut ys = args.to_vec();
        let mut near = |fx: &mut Fx<'_, '_, '_, 'h>, d: f64| -> Res<Option<f64>> {
            ys[i] = fx.real(x + inward * d);
            let w = fx.eval(fx.h.app(head, &ys))?;
            machine(fx, w)
        };
        Ok(match (near(fx, 1e-8)?, near(fx, 1e-10)?) {
            (Some(p), Some(q)) => p.is_finite() && q.is_finite() && p.abs() > 1e6 && q.abs() >= 99.0 * p.abs() && p.signum() == q.signum(),
            _ => false,
        })
    };
    let width = fb - fa;
    let (pole_a, pole_b) = (pole(fx, ga0, fa, width)?, pole(fx, gb0, fb, -width)?);
    if pole_a && pole_b {
        return Ok(None);
    }
    let dg = fx.eval(fx.call("d", &[g, var]))?;
    let derivative_known = !fx.contains(dg, "derivative") && !fx.contains(dg, "d");
    // the derivative (or, without one, the function) on a grid
    let cells = 128;
    let f_at = |fx: &mut Fx<'_, '_, '_, 'h>, e: T<'h>, x: f64| -> Res<Option<f64>> {
        let v = fx.eval(fx.call("rewrite", &[e, fx.call("rule", &[var, fx.real(x)])]))?;
        Ok(fx.f64_of(v))
    };
    let mut critical = vec![];
    let probe = if derivative_known { dg } else { g };
    // the last nonzero sample (a zero of the derivative on the grid is passed over)
    let mut prev: Option<(f64, f64)> = None;
    for k in 0..=cells {
        if (k == 0 && pole_a) || (k == cells && pole_b) {
            continue;
        }
        let x = fa + (fb - fa) * k as f64 / cells as f64;
        let Some(v) = f_at(fx, probe, x)? else { return Ok(None) };
        if let Some((px, pv)) = prev {
            if derivative_known && v != 0.0 && (pv < 0.0) != (v < 0.0) {
                // bisect the sign change of the derivative
                let (mut l, mut r, mut lv) = (px, x, pv);
                for _ in 0..60 {
                    let m = 0.5 * (l + r);
                    let Some(mv) = f_at(fx, dg, m)? else { break };
                    if (mv < 0.0) == (lv < 0.0) {
                        l = m;
                        lv = mv;
                    } else {
                        r = m;
                    }
                }
                critical.push(0.5 * (l + r));
            } else if !derivative_known {
                critical.push(x);
            }
        }
        if v != 0.0 || !derivative_known {
            prev = Some((x, v));
        }
        fx.tick(1)?;
    }
    if pole_a || pole_b {
        // monotone between the pole and the other end: from that end's value to the infinity of the side
        if !critical.is_empty() {
            return Ok(None);
        }
        let inside = if pole_a { fa + (fb - fa) * 1e-9 } else { fb - (fb - fa) * 1e-9 };
        let Some(v) = f_at(fx, g, inside)? else { return Ok(None) };
        let finite = if pole_a { gb0 } else { ga0 };
        let finite = fx.eval(fx.call("simplify", &[finite]))?;
        if machine(fx, finite)?.is_none_or(|x| !x.is_finite()) || !v.is_finite() || v == 0.0 {
            return Ok(None);
        }
        return Ok(Some(if v > 0.0 { (finite, fx.inf()) } else { (neg_inf(fx), finite) }));
    }
    let ga = ga0;
    let gb = gb0;
    let exact_ends = [a, b].iter().all(|&e| fx.num(e).is_none_or(|n| n.kind() == Kind::Exact));
    if critical.is_empty() && exact_ends {
        return span(fx, &[ga, gb]);
    }
    if exact_ends {
        // exact ends: critical points that are simple rationals (0, 1/2, ...) are taken exactly, the rest as
        // machine values; the span is compared by refinement
        let mut terms = vec![ga, gb];
        let width = (fb - fa).abs().max(1e-300);
        for &x in &critical {
            let snapped = (1..=12i64).find_map(|d| {
                let n = (x * d as f64).round();
                ((x - n / d as f64).abs() <= 1e-9 * width).then(|| fx.rat(n as i64, d))
            });
            let at_x = match snapped {
                Some(q) => at(fx, q)?,
                None => at(fx, fx.real(x))?,
            };
            terms.push(at_x);
        }
        if let Some(s) = span(fx, &terms)? {
            return Ok(Some(s));
        }
    }
    let mut vals = vec![];
    for v in [ga, gb] {
        let Some(m) = machine(fx, v)? else { return Ok(None) };
        vals.push(m);
    }
    for x in critical {
        let v = at(fx, fx.real(x))?;
        let Some(m) = machine(fx, v)? else { return Ok(None) };
        vals.push(m);
    }
    let lo = vals.iter().copied().fold(f64::INFINITY, f64::min);
    let hi = vals.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    Ok(Some((fx.real(lo), fx.real(hi))))
}

fn interval_apply<'h>(fx: &mut Fx<'_, '_, '_, 'h>, head: T<'h>, args: &[T<'h>]) -> Res<Option<T<'h>>> {
    let name = match fx.h.view(head) {
        View::Sym(s) => fx.h.sym_name(s),
        _ => return Ok(None),
    };
    let op = match name.as_str() {
        "add" => Some(Op::Add),
        "mul" => Some(Op::Mul),
        "pow" => Some(Op::Pow),
        _ => None,
    };
    if let Some(op) = op {
        // fold the arguments: numbers become point intervals
        let to_pieces = |fx: &Fx<'_, '_, '_, 'h>, t: T<'h>| pieces(fx, t).unwrap_or_else(|| vec![(t, t)]);
        if op == Op::Pow {
            let mut out = vec![];
            match (pieces(fx, args[0]), pieces(fx, args[1])) {
                (Some(xs), None) => {
                    for x in xs {
                        let Some(mut p) = piece_pow(fx, x, args[1])? else { return Ok(None) };
                        out.append(&mut p);
                    }
                }
                (xs, Some(ys)) => {
                    // an interval exponent: x^y = exp(y log x) is monotone in each argument for x > 0, so the image
                    // of a box is spanned by its four corners
                    let xs = xs.unwrap_or_else(|| vec![(args[0], args[0])]);
                    let zero = fx.int(0);
                    for &x in &xs {
                        if less(fx, zero, x.0)? != Some(true) {
                            return Ok(None);
                        }
                        for &y in &ys {
                            let mut vals = vec![];
                            for a in [x.0, x.1] {
                                for b in [y.0, y.1] {
                                    vals.push(fx.eval(fx.pow(a, b))?);
                                }
                            }
                            let Some(s) = span(fx, &vals)? else { return Ok(None) };
                            out.push(s);
                        }
                    }
                }
                (None, None) => return Ok(None),
            }
            return make_interval(fx, outward(fx, out));
        }
        let mut acc = to_pieces(fx, args[0]);
        for &a in &args[1..] {
            let ys = to_pieces(fx, a);
            let mut next = vec![];
            for &x in &acc {
                for &y in &ys {
                    let Some(mut p) = piece_op(fx, op, x, y)? else { return Ok(None) };
                    next.append(&mut p);
                }
            }
            acc = next;
            fx.tick(acc.len() as u64)?;
        }
        return make_interval(fx, outward(fx, acc));
    }
    // a function of one interval argument
    let at: Vec<usize> = (0..args.len()).filter(|&i| pieces(fx, args[i]).is_some()).collect();
    let [i] = at[..] else { return Ok(None) };
    let Some(ps) = pieces(fx, args[i]) else { return Ok(None) };
    let mut out = vec![];
    for p in ps {
        if let Some(s) = periodic_cover(fx, &name, args.len(), p)? {
            out.push(s);
            continue;
        }
        let Some(s) = image(fx, head, args, i, p)? else { return Ok(None) };
        out.push(s);
    }
    make_interval(fx, outward(fx, out))
}

/// The image of a piece at least one period long (or unbounded) under sin or cos (period 2 pi, range [-1, 1]) or
/// tan (period pi, every real value): the whole range, since a full period attains it.
fn periodic_cover<'h>(fx: &mut Fx<'_, '_, '_, 'h>, name: &str, arity: usize, (a, b): (T<'h>, T<'h>)) -> Res<Option<(T<'h>, T<'h>)>> {
    let period = match (name, arity) {
        ("sin" | "cos", 1) => 2.0 * std::f64::consts::PI,
        ("tan", 1) => std::f64::consts::PI,
        _ => return Ok(None),
    };
    let unbounded = fx.infinity(a).is_some() || fx.infinity(b).is_some();
    if !unbounded {
        let (Some(lo), Some(hi)) = (machine(fx, a)?, machine(fx, b)?) else { return Ok(None) };
        // a margin for the rounding of the ends: a piece that is only about one period long goes to the sampler
        if hi - lo <= period * (1.0 + 1e-9) || (hi - lo).is_nan() {
            return Ok(None);
        }
    }
    Ok(Some(if name == "tan" { (neg_inf(fx), fx.inf()) } else { (fx.int(-1), fx.int(1)) }))
}

// ---------------------------------------------------------------- centered intervals

const BALL_BITS: i64 = 100;

/// A ball as `centered_interval(mid, rad)` with machine numbers (the radius rounded up).
fn from_ball<'h>(fx: &Fx<'_, '_, '_, 'h>, t: T<'h>) -> Option<T<'h>> {
    match fx.num(t)? {
        Num::Real(Real::Ball(b, _)) => {
            let mid = Real::Ball(mc_arith::num::Ball::exact(b.mid.clone()), BALL_BITS as u64).to_f64();
            let rad = (b.rad.m as f64) * 2f64.powi(b.rad.e as i32);
            let rad = rad * (1.0 + 4.0 * f64::EPSILON) + f64::from_bits(1);
            Some(fx.call("centered_interval", &[fx.real(mid), fx.real(rad)]))
        }
        Num::Real(Real::Exact(_)) => Some(fx.call("centered_interval", &[t, fx.int(0)])),
        _ => None,
    }
}

fn centered_apply<'h>(fx: &mut Fx<'_, '_, '_, 'h>, head: T<'h>, args: &[T<'h>]) -> Res<Option<T<'h>>> {
    propagate(fx, head, args, "centered_interval")
}

// ---------------------------------------------------------------- constructors

fn around_e<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> Res<Option<T<'h>>> {
    if a.len() == 1 {
        // around(list): the mean and the standard deviation
        let Some(xs) = fx.as_list(a[0]) else { return Ok(None) };
        if xs.len() < 2 {
            return Ok(None);
        }
        let m = fx.eval(fx.call("mean", &[a[0]]))?;
        let s = fx.eval(fx.call("std_dev", &[a[0]]))?;
        return Ok(Some(fx.call("around", &[m, s])));
    }
    let (x, mut d) = (a[0], a[1]);
    // a relative uncertainty: scaled(s) is s |x|
    if let Some(&[s]) = fx.call_of(d, "scaled") {
        let base = match fx.call_of(x, "quantity") {
            Some(&[m, _]) => m,
            _ => x,
        };
        d = fx.eval(fx.mul(&[s, fx.call("abs", &[base])]))?;
    }
    // a quantity with an uncertainty: the uncertain magnitude in its unit (the uncertainty converted to that unit)
    if let Some(&[m, u]) = fx.call_of(x, "quantity") {
        let dm = match fx.call_of(d, "quantity") {
            Some(_) => {
                let v = fx.eval(fx.call("quantity_magnitude", &[d, u]))?;
                if fx.num(v).is_none() {
                    return Ok(None);
                }
                v
            }
            None => d,
        };
        return Ok(Some(fx.call("quantity", &[fx.call("around", &[m, dm]), u])));
    }
    let (Some(xm), Some(dm)) = (machine(fx, x)?, machine(fx, d)?) else { return Ok(None) };
    let (xr, dr) = (fx.real(xm), fx.real(dm.abs()));
    if xr == x && dr == d {
        return Ok(None);
    }
    Ok(Some(fx.call("around", &[xr, dr])))
}

fn interval_e<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> Res<Option<T<'h>>> {
    let mut ps = vec![];
    for &p in a {
        match fx.as_list(p) {
            Some(&[x, y]) => {
                // ends in order
                match less(fx, y, x)? {
                    Some(true) => ps.push((y, x)),
                    Some(false) => ps.push((x, y)),
                    None => return Ok(None),
                }
            }
            _ => {
                if fx.num(p).is_none() {
                    return Ok(None);
                }
                ps.push((p, p));
            }
        }
    }
    if ps.is_empty() {
        return Ok(None);
    }
    let v = make_interval(fx, ps)?;
    let t = fx.call("interval", a);
    Ok(v.filter(|&v| v != t))
}

fn centered_e<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> Res<Option<T<'h>>> {
    if let [p] = a {
        // a real number (a ball, a machine real, an exact number): the midpoint and the radius of its enclosure
        if fx.as_list(*p).is_none() {
            return Ok(match fx.num(*p) {
                Some(Num::Real(Real::Ball(..) | Real::Exact(_))) => from_ball(fx, *p),
                Some(Num::Real(_)) => Some(fx.call("centered_interval", &[*p, fx.int(0)])),
                _ => None,
            });
        }
        // the mantissa/exponent form [[m, e, rm, re], prec]
        let Some(&[parts, _]) = fx.as_list(*p) else { return Ok(None) };
        // a complex one: [[re part, im part], prec], each part [m, e, rm, re]; the box of the two parts
        if let Some(&[pr, pi]) = fx.as_list(parts) {
            let mut mids = vec![];
            let mut rads = vec![];
            for part in [pr, pi] {
                let Some(&[m, e, rm, re]) = fx.as_list(part) else { return Ok(None) };
                let two = fx.int(2);
                let c = fx.eval(fx.mul(&[m, fx.pow(two, e)]))?;
                let r = fx.eval(fx.mul(&[rm, fx.pow(two, re)]))?;
                let (Some(c), Some(r)) = (fx.f64_of(c), fx.f64_of(r)) else { return Ok(None) };
                mids.push(fx.real(c));
                rads.push(fx.real(r * (1.0 + 4.0 * f64::EPSILON) + f64::from_bits(1)));
            }
            let c = fx.ar.complex(mids[0], mids[1]);
            let r = fx.ar.complex(rads[0], rads[1]);
            return Ok(Some(fx.call("centered_interval", &[c, r])));
        }
        let Some(&[m, e, rm, re]) = fx.as_list(parts) else { return Ok(None) };
        let two = fx.int(2);
        let c = fx.eval(fx.mul(&[m, fx.pow(two, e)]))?;
        let r = fx.eval(fx.mul(&[rm, fx.pow(two, re)]))?;
        let b = fx.ar.ball_with_radius(c, r, BALL_BITS as u64);
        return Ok(b.and_then(|b| from_ball(fx, b)));
    }
    let (c, r) = (a[0], a[1]);
    let real = |fx: &Fx<'_, '_, '_, 'h>, x: T<'h>| fx.num(x).is_some_and(|n| n.as_real().is_some());
    if !(real(fx, c) && real(fx, r)) {
        return Ok(None);
    }
    // machine centre and radius are kept as given; exact ones stay exact
    Ok(None)
}

/// The constructors.
pub static DEFS: &[Def] = &[
    Def {
        name: "around",
        arities: &[1, 2],
        listable: false,
        exact: Some(around_e),
        numeric: Some(around_e),
        summary: "around(x, dx): the value x with standard uncertainty dx, propagated to first order through arithmetic and functions; around(list): mean and standard deviation",
    },
    Def {
        name: "interval",
        arities: &[1, 2, 3, 4, 5, 6, 7, 8],
        listable: false,
        exact: Some(interval_e),
        numeric: None,
        summary: "interval([a, b], ...): the union of closed intervals, carried through arithmetic and functions (images by critical points)",
    },
    Def {
        name: "centered_interval",
        arities: &[1, 2],
        listable: false,
        exact: Some(centered_e),
        numeric: Some(centered_e),
        summary: "centered_interval(c, r): the interval c +- r, carried through arithmetic and functions as a rigorous ball enclosure",
    },
];

/// The overall ends [lo, hi] of an `interval` term (its first piece's lower end, its last piece's upper end), of
/// `around(x, dx)` (x -+ dx) and of `centered_interval(c, r)` (c -+ r).
fn ends<'h>(st: &'h Heap, ar: &Arith<'h, Heap>, t: T<'h>) -> Option<(T<'h>, T<'h>)> {
    let View::App { head, args } = st.view(t) else { return None };
    let View::Sym(s) = st.view(head) else { return None };
    let name = st.sym_name(s);
    if (name == "around" || name == "centered_interval") && args.len() == 2 {
        let (x, d) = (args[0], args[1]);
        ar.num(x)?;
        ar.num(d)?;
        return Some((ar.sub(x, d), ar.add(&[x, d])));
    }
    if name != "interval" || args.is_empty() {
        return None;
    }
    let end = |p: T<'h>, i: usize| match st.view(p) {
        View::App { args: &[a, b], .. } => Some(if i == 0 { a } else { b }),
        _ => None,
    };
    Some((end(args[0], 0)?, end(*args.last()?, 1)?))
}

/// `max` (`max` true) or `min` of arguments (lists already flattened) when one is an uncertain number, for the list
/// functions (gap queue): intervals and centered intervals give the interval of the extremes
/// of the ends (a number counts as a point), `max(interval([1, 3]), interval([-3, 5]))` is `interval([1, 5])`;
/// `around` values and quantities give the extreme argument itself when every comparison with it is decided
/// (`min([1, around(-1.1, 0.01)])` is the around, quantities compare in a common unit). `None` for mixed kinds,
/// undecided comparisons, or no uncertain argument.
pub fn extreme<'h>(cx: &mut NativeCx<'_, '_, 'h>, max: bool, args: &[T<'h>]) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    let kinds: Vec<Option<usize>> = args.iter().map(|&a| head_of(h, a)).collect();
    let Some(kind) = kinds.iter().flatten().copied().next() else { return Ok(None) };
    if kinds.iter().flatten().any(|&k| k != kind) {
        return Ok(None);
    }
    let ar = Arith::new(h);
    let better = |c: mc_arith::Cmp3| matches!((max, c), (true, mc_arith::Cmp3::Greater) | (false, mc_arith::Cmp3::Less));
    if kind == 1 || kind == 2 {
        // the ends of each argument, then the extreme lower end and the extreme upper end
        let mut lo: Option<T<'h>> = None;
        let mut hi: Option<T<'h>> = None;
        for (&a, k) in args.iter().zip(&kinds) {
            let (l, u) = match k {
                Some(_) => match ends(h, &ar, a) {
                    Some(e) => e,
                    None => return Ok(None),
                },
                None if ar.num(a).is_some() => (a, a),
                None => return Ok(None),
            };
            for (slot, v) in [(&mut lo, l), (&mut hi, u)] {
                *slot = Some(match *slot {
                    None => v,
                    Some(w) => match ar.compare(v, w) {
                        Some(c) if better(c) => v,
                        Some(_) => w,
                        None => return Ok(None),
                    },
                });
            }
        }
        let (Some(lo), Some(hi)) = (lo, hi) else { return Ok(None) };
        let v = if kind == 1 {
            // max/min over the members of the intervals (and numbers): a number, the extreme end
            return Ok(Some(if max { hi } else { lo }));
        } else {
            let half = ar.rational(h.int(1), h.int(2));
            h.call(h.sym("centered_interval"), &[ar.mul(&[half, ar.add(&[lo, hi])]), ar.mul(&[half, ar.sub(hi, lo)])])
        };
        return Ok(Some(cx.eval(v)?));
    }
    // around and quantities: the argument every other one compares below (above)
    let rel = h.sym(if max { "greater" } else { "less" });
    let truth = h.sym_term(h.sym(names::TRUE));
    let falsity = h.sym_term(h.sym(names::FALSE));
    let mut best = args[0];
    for &a in &args[1..] {
        let c = cx.eval(h.call(rel, &[a, best]))?;
        if c == truth {
            best = a;
        } else if c != falsity {
            // undecided, unless the two are the same
            if a != best {
                return Ok(None);
            }
        }
    }
    Ok(Some(best))
}

/// The comparisons of an interval (or an `around`, a `centered_interval`, as the interval of its spread) with a
/// number or another interval (decision P6N.12): `(hi(a) vs lo(b),
/// lo(a) vs hi(b))`, from which every relation is decided where the sets do not overlap (`None` without intervals).
pub fn compare_intervals<'h>(
    h: &'h Heap,
    ar: &Arith<'h, Heap>,
    nm: &crate::numeval::Names,
    a: T<'h>,
    b: T<'h>,
) -> Option<(mc_arith::Cmp3, mc_arith::Cmp3)> {
    let (ea, eb) = (ends(h, ar, a), ends(h, ar, b));
    if ea.is_none() && eb.is_none() {
        return None;
    }
    let (alo, ahi) = ea.unwrap_or((a, a));
    let (blo, bhi) = eb.unwrap_or((b, b));
    let cmp = |x: T<'h>, y: T<'h>| match (ar.num(x), ar.num(y)) {
        (Some(_), Some(_)) => ar.compare(x, y),
        _ if ar.inf_sign(x).is_some() || ar.inf_sign(y).is_some() => ar.compare(x, y),
        _ => crate::numeval::compare(h, ar, nm, x, y),
    };
    Some((cmp(ahi, blo)?, cmp(alo, bhi)?))
}
