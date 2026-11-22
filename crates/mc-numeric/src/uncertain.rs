//! Numbers with uncertainty: `around(x, dx)`, `interval([a, b], ...)` and
//! `centered_interval(c, r)`, carried through arithmetic and through the functions of this crate and of the
//! special-function crates.
//!
//! - `around(x, dx)`: a machine value with a standard uncertainty, propagated to first order assuming independent
//!   arguments: f(x1 +- d1, ...) is f(x) +- sqrt(sum (df/dxi di)^2), the derivatives by `d`.
//! - `interval([a, b], ...)`: a union of closed intervals (sorted, overlapping ones merged); the image of an
//!   interval under a function of one argument is proved by [`crate::enclose`] (monotonicity from an enclosure of
//!   the derivative over cells, extrema by the sign of the second derivative, branch and bound on enclosures of the
//!   values; a pole inside gives the whole line), exact when the function is monotone and the ends are exact (round
//!   41: the grid of 128 derivative samples it replaces missed poles and extrema); sums, products, powers and
//!   quotients by interval arithmetic (a quotient by an interval containing 0 splits).
//! - `centered_interval(c, r)`: a midpoint and a radius: f(c +- r) of one spread is the hull of the proved image of
//!   [c - r, c + r]; where that is not proved (a pole inside) the call stays, a radius 0 takes the point ball (the
//!   slope estimate f(c) +- r max |f'| at three points is not an enclosure); functions of several spreads
//!   (and of several `interval`s) other than sums and products are enclosed over the box: the hull of two corners
//!   when every partial derivative's sign is proved by its enclosure over the box, else the function of the balls
//!   met with the mean value form (no grid of sample points decides the signs), or
//!   stay unevaluated when no enclosure is found; the elementary functions of a complex spread go through the box arithmetic of
//!   `uncertain_boxes`; sums, products and
//!   powers with a complex part, an `interval` mixed in or an uncertain exponent go through the rigorous box
//!   arithmetic of `uncertain_boxes` (a complex radius rx + i ry is the box of the two parts);
//!   `centered_interval([[m, e, rm, re], p])` reads the mantissa/exponent form m 2^e +- rm 2^re.
//!
//! The engine's `add`, `mul` and `pow` call [`arith`] first when an argument is one of these; the natives of the
//! elementary and special functions call [`apply`].
use crate::formula::{Def, Fx};
use mc_api::{names, Store, View, T};
use mc_arith::{Arith, Num, Real};
use mc_core::Heap;
use mc_eval::{NativeCx, Res};

/// The heads carried through arithmetic and functions.
pub const HEADS: [&str; 4] = ["around", "interval", "centered_interval", "quantity"];

/// Is one of the arguments an uncertain number?
pub fn any(h: &Heap, args: &[T<'_>]) -> bool {
    args.iter().any(|&a| head_of(h, a).is_some())
}

/// Is a centered interval an element of a list argument (the invariants of the Weierstrass functions)?
fn in_list(h: &Heap, args: &[T<'_>]) -> bool {
    args.iter().any(|&a| match h.view(a) {
        View::App { head, args: xs } => {
            matches!(h.view(head), View::Sym(s) if h.sym_name(s) == "list") && xs.iter().any(|&x| head_of(h, x) == Some(2))
        }
        _ => false,
    })
}

/// [`any`], or a centered interval inside a list argument.
pub fn any_or_listed(h: &Heap, args: &[T<'_>]) -> bool {
    any(h, args) || in_list(h, args)
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
    if kinds.iter().all(Option::is_none) && in_list(fx.h, args) {
        // centered intervals inside a list argument: the registered complex-ball enclosures
        return crate::uncertain_cball::apply(fx, t, args, false);
    }
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
    // the elementary functions of a centered interval with a complex part: box arithmetic on the parts
    if let Some(r) = crate::uncertain_boxes::function(fx, t, args)? {
        return Ok(Some(r));
    }
    // other functions of centered intervals with a complex part: the registered complex-ball enclosures
    if kind == 2 {
        if let Some(r) = crate::uncertain_cball::apply(fx, t, args, true)? {
            return Ok(Some(r));
        }
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
        2 => match centered_apply(fx, head, args)? {
            Some(r) => Ok(Some(r)),
            // real spreads without a proved real image (a complex value: elliptic_k(centered_interval(4, 1/100))):
            // the complex-ball enclosure of the box
            None => crate::uncertain_cball::apply(fx, t, args, false),
        },
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
    // A machine value with an imaginary part need not be a complex value: a real function evaluated through complex
    // intermediates (a hypergeometric function beyond its branch point that is a rational function there, e^(i pi)
    // in a spherical harmonic) gives an imaginary part of rounding size. Whether the value is real is decided at the
    // exact centre, at 30 digits: an imaginary part that is exactly 0 makes the value real, one that is decided
    // nonzero keeps it complex, and an undecided one leaves the call unevaluated.
    let mut real_value = None;
    let complex_value = if centered || machine(fx, value)?.is_some() {
        None
    } else {
        let (re, im) = (machine(fx, fx.call("re", &[value]))?, machine(fx, fx.call("im_part", &[value]))?);
        match (re, im) {
            (Some(a), Some(b)) if a.is_finite() && b.is_finite() && unc.iter().flatten().count() == 1 => {
                match real_at_exact_centre(fx, head, &centers)? {
                    Some(Ok(r)) => {
                        real_value = Some(r);
                        None
                    }
                    Some(Err(())) => Some(fx.eval(fx.call("n", &[value]))?),
                    None => return Ok(None),
                }
            }
            _ => None,
        }
    };
    let v = match (machine(fx, value)?, real_value, complex_value) {
        (Some(v), _, _) => v,
        (None, Some(r), _) => r,
        (None, None, Some(_)) => 0.0,
        (None, None, None) => return Ok(None),
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
                // every centre and radius rounded to a machine number (an exact 1/3, a radius 1/100) and every
                // midpoint operation widens the radius by the rounding: |x| eps each (half an ulp is |x| eps/2 at most),
                // the float operations on the radii by the factor (1 + 4 (k + 1) eps) at the end
                let eps = f64::EPSILON;
                let mut acc: Option<(f64, f64)> = None;
                for (i, d) in unc.iter().enumerate() {
                    let (Some(c), r) = (machine(fx, centers[i])?, match d {
                        Some(d) => machine(fx, *d)?,
                        None => Some(0.0),
                    }) else { return Ok(None) };
                    let Some(r) = r else { return Ok(None) };
                    let exact_c = fx.num(centers[i]).is_some_and(|n| !n.is_exact()) || fx.int_of(centers[i]).is_some_and(|v| v.unsigned_abs() < 1 << 53);
                    let r = r.abs() * (1.0 + eps) + if exact_c { 0.0 } else { c.abs() * eps };
                    acc = Some(match acc {
                        None => (c, r),
                        Some((c0, r0)) if name == names::ADD => {
                            let s = c0 + c;
                            (s, r0 + r + s.abs() * eps)
                        }
                        Some((c0, r0)) => {
                            let p = c0 * c;
                            (p, c0.abs() * r + c.abs() * r0 + r0 * r + p.abs() * eps)
                        }
                    });
                }
                if let Some((c, r)) = acc {
                    if c.is_finite() && r.is_finite() {
                        let k = unc.len() as f64;
                        let r = r * (1.0 + 4.0 * (k + 1.0) * eps) + f64::from_bits(1);
                        return Ok(Some(fx.call(kind, &[fx.real(c), fx.real(r)])));
                    }
                }
            }
        }
        // other functions of several centered intervals: the corners of the box where the function is monotone in
        // each argument, else the function of the balls (the special-function natives evaluate ball arguments with
        // rigorous error bounds), else unevaluated; the slope estimate below is not an enclosure when the slopes vary
        // over the spreads
        if let Some(r) = centered_corners(fx, head, &centers, &unc)? {
            return Ok(Some(r));
        }
        return centered_by_balls(fx, head, &centers, &unc);
    }
    // one spread whose image is not proved: a point (radius 0) is the function of the point ball; a spread stays (the
    // slope estimate at three points is not an enclosure, and the function of a wide ball is a loose one)
    if centered {
        let point = match unc.iter().flatten().next() {
            Some(&d) => machine(fx, d)? == Some(0.0),
            None => true,
        };
        return if point { centered_by_balls(fx, head, &centers, &unc) } else { Ok(None) };
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
                // (for a real value, the slope's imaginary part is of rounding size too)
                _ if complex_value.is_some() || real_value.is_some() => match machine(fx, fx.call("abs", &[at]))? {
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

/// Whether `head` at the exact values of the centres (machine numbers taken as the binary rationals they are) is
/// real, at 30 digits: `Ok(value)` (rounded to a machine number) when its imaginary part is exactly 0, `Err(())`
/// when that part is decided nonzero, `None` when it is not decided (a ball containing 0) or there is no value.
fn real_at_exact_centre<'h>(fx: &mut Fx<'_, '_, '_, 'h>, head: T<'h>, centers: &[T<'h>]) -> Res<Option<Result<f64, ()>>> {
    let exact: Vec<T<'h>> = centers
        .iter()
        .map(|&c| if fx.num(c).is_some_and(|n| !n.is_exact()) { fx.call("set_precision", &[c, fx.inf()]) } else { c })
        .collect();
    let w = fx.eval(fx.call("n", &[fx.h.app(head, &exact), fx.int(30)]))?;
    let im = fx.eval(fx.call("im_part", &[w]))?;
    if fx.is_int(im, 0) {
        return Ok(machine(fx, w)?.filter(|r| r.is_finite()).map(Ok));
    }
    let nonzero = fx.eval(fx.call("greater", &[fx.call("abs", &[im]), fx.int(0)]))?;
    Ok((nonzero == fx.sym("true")).then_some(Err(())))
}

/// `head` of several real centered intervals (at most four) as a centered interval enclosing its image over the
/// box ([`box_image`]); `None` when no enclosure is found.
fn centered_corners<'h>(fx: &mut Fx<'_, '_, '_, 'h>, head: T<'h>, centers: &[T<'h>], unc: &[Option<T<'h>>]) -> Res<Option<T<'h>>> {
    let Some((lo, hi)) = box_image(fx, head, centers, unc)? else { return Ok(None) };
    let mid = 0.5 * (lo + hi);
    let rad = (0.5 * (hi - lo) + 4.0 * f64::EPSILON * (lo.abs() + hi.abs())) * (1.0 + 4.0 * f64::EPSILON);
    Ok(Some(fx.call("centered_interval", &[fx.real(mid), fx.real(rad)])))
}

/// A machine number as the exact binary rational it is; exact numbers stay.
fn exact_of<'h>(fx: &mut Fx<'_, '_, '_, 'h>, t: T<'h>) -> Res<T<'h>> {
    if fx.num(t).is_some_and(|n| !n.is_exact()) {
        return fx.eval(fx.call("set_precision", &[t, fx.inf()]));
    }
    Ok(t)
}

/// The partial derivative in argument `j` of a function whose `d` gives no closed form, from its series where the
/// literature gives one: the Lerch transcendent Phi(z, s, a) = sum z^n/(n + a)^s (DLMF 25.14.1), so dPhi/dz =
/// (Phi(z, s - 1, a) - a Phi(z, s, a))/z and dPhi/da = -s Phi(z, s + 1, a); Carlson's R_F, whose partials are
/// -R_D/6 with the variable last (DLMF 19.18.1, R_F symmetric). `None` otherwise.
fn known_partial<'h>(fx: &mut Fx<'_, '_, '_, 'h>, head: T<'h>, args: &[T<'h>], j: usize) -> Res<Option<T<'h>>> {
    let View::Sym(s) = fx.h.view(head) else { return Ok(None) };
    let name = fx.h.sym_name(s);
    let e = match (name.as_str(), args, j) {
        ("lerch_phi", &[z, s, a], 0) => {
            let lower = fx.call("lerch_phi", &[z, fx.add(&[s, fx.int(-1)]), a]);
            let same = fx.call("lerch_phi", &[z, s, a]);
            fx.mul(&[fx.add(&[lower, fx.neg(fx.mul(&[a, same]))]), fx.pow(z, fx.int(-1))])
        }
        ("lerch_phi", &[z, s, a], 2) => fx.neg(fx.mul(&[s, fx.call("lerch_phi", &[z, fx.add(&[s, fx.int(1)]), a])])),
        ("carlson_rf", &[x, y, z], _) => {
            let order = match j {
                0 => [y, z, x],
                1 => [x, z, y],
                _ => [x, y, z],
            };
            fx.mul(&[fx.ar.rational(fx.int(-1), fx.int(6)), fx.call("carlson_rd", &order)])
        }
        _ => return Ok(None),
    };
    Ok(Some(fx.eval(e)?))
}

/// An enclosure of `head` over the box of several real spreads c_i +- r_i (2 to 4; the other arguments numbers),
/// as outward-rounded machine bounds (proved, never sampled). Each sub-box (made by halving) is enclosed
/// by the monotonicity test of interval analysis: a spread in which the partial derivative (in closed form, or a
/// known series form, [`known_partial`]) has one proved sign over the sub-box is fixed at the end that gives the
/// least (largest) value; the least value over the sub-box is then the least over that face, enclosed exactly at a
/// corner when every spread is fixed, otherwise by interval arithmetic in the free spreads: the natural extension (the
/// function of the balls, the expression as given and in its `function_expand` form, met) and, when the free
/// partials are known, the mean value form f(c) + sum_i f_i(B) [-r_i, r_i] met with it. The hull over the sub-boxes
/// encloses the image. The sub-boxes that give the least and the largest value are halved in a free spread while a
/// budget of sub-boxes ([`MAX_BOX_CELLS`]) lasts (adaptive bisection; a sub-box without an enclosure is halved as
/// well).
///
/// Literature: R. E. Moore, R. B. Kearfott, M. J. Cloud, "Introduction to Interval Analysis", SIAM 2009, sections
/// 6.1-6.4 (mean value form, subdivision) and the monotonicity test of E. Hansen, G. W. Walster, "Global
/// Optimization Using Interval Analysis", 2nd ed., Marcel Dekker 2004, section 12.4.
///
/// `None` when no enclosure is found (a function without a ball form, a pole or a cut inside the box).
fn box_image<'h>(fx: &mut Fx<'_, '_, '_, 'h>, head: T<'h>, centers: &[T<'h>], unc: &[Option<T<'h>>]) -> Res<Option<(f64, f64)>> {
    let idx: Vec<usize> = (0..unc.len()).filter(|&i| unc[i].is_some()).collect();
    let k = idx.len();
    if !(2..=4).contains(&k) {
        return Ok(None);
    }
    // the exact centres and radii, the exact other arguments
    let mut exact = Vec::with_capacity(centers.len());
    for &c in centers {
        exact.push(exact_of(fx, c)?);
    }
    let mut radii = vec![];
    for &i in &idx {
        let d = exact_of(fx, unc[i].expect("uncertain"))?;
        let d = fx.eval(fx.call("abs", &[d]))?;
        let real = |fx: &Fx<'_, '_, '_, 'h>, t: T<'h>| fx.num(t).is_some_and(|n| n.as_real().is_some());
        if !(real(fx, exact[i]) && real(fx, d)) {
            return Ok(None);
        }
        radii.push(d);
    }
    let vars: Vec<T<'h>> = (0..k).map(|j| fx.sym(&format!("around·t{j}"))).collect();
    let mut xs = exact.clone();
    for (j, &i) in idx.iter().enumerate() {
        xs[i] = vars[j];
    }
    let g = fx.h.app(head, &xs);
    // an expression and its `function_expand` form (beta through gamma functions: the ball form of the expanded
    // expression is often far tighter), their constants as balls
    let forms = |fx: &mut Fx<'_, '_, '_, 'h>, e: T<'h>| -> Res<(T<'h>, Option<T<'h>>)> {
        let a = crate::enclose::constants_as_balls(fx, e)?;
        let x = fx.eval(fx.call("function_expand", &[e]))?;
        let x = if x == e { None } else { Some(crate::enclose::constants_as_balls(fx, x)?) };
        Ok((a, x))
    };
    // the partial derivatives in closed form
    let mut dgs: Vec<Option<(T<'h>, Option<T<'h>>)>> = vec![];
    for (j, &v) in vars.iter().enumerate() {
        let mut dg = fx.eval(fx.call("d", &[g, v]))?;
        if fx.contains(dg, "derivative") || fx.contains(dg, "d") {
            match known_partial(fx, head, &xs, idx[j])? {
                Some(p) => dg = p,
                None => {
                    dgs.push(None);
                    continue;
                }
            }
        }
        dgs.push(Some(forms(fx, dg)?));
    }
    // an expression (both forms, the enclosures met) at the values `bs` of the spread arguments (balls or numbers)
    let on_balls = |fx: &mut Fx<'_, '_, '_, 'h>, (e, alt): (T<'h>, Option<T<'h>>), bs: &[T<'h>]| -> Res<Option<(f64, f64)>> {
        let rules: Vec<T<'h>> = vars.iter().zip(bs).map(|(&v, &bl)| fx.call("rule", &[v, bl])).collect();
        let rl = fx.list(&rules);
        let mut out: Option<(f64, f64)> = None;
        for form in std::iter::once(e).chain(alt) {
            fx.tick(1)?;
            let v = fx.eval(fx.call("rewrite", &[form, rl]))?;
            let v = crate::enclose::constants_as_balls(fx, v)?;
            if let Some((a, b)) = crate::enclose::bounds(fx, v) {
                out = Some(match out {
                    Some((c, d)) if a.max(c) <= b.min(d) => (a.max(c), b.min(d)),
                    Some(o) => o,
                    None => (a, b),
                });
            }
        }
        Ok(out)
    };
    // the value at an exact point, enclosed
    let at_point = |fx: &mut Fx<'_, '_, '_, 'h>, args: &[T<'h>]| -> Res<Option<(f64, f64)>> {
        fx.tick(1)?;
        // the spread arguments as balls of radius 0 (a ball value is far cheaper than 30 digits)
        let mut bargs = args.to_vec();
        for &i in &idx {
            if let Some(b) = fx.ar.ball_with_radius(args[i], fx.int(0), CELL_BITS) {
                bargs[i] = b;
            }
        }
        let v = fx.eval(fx.h.app(head, &bargs))?;
        let v = crate::enclose::constants_as_balls(fx, v)?;
        if let Some(bd) = crate::enclose::bounds(fx, v) {
            return Ok(Some(bd));
        }
        let v = fx.eval(fx.h.app(head, args))?;
        if let Some(bd) = crate::enclose::bounds(fx, v) {
            return Ok(Some(bd));
        }
        let w = fx.eval(fx.call("n", &[v, fx.int(30)]))?;
        Ok(crate::enclose::bounds(fx, w))
    };
    let mut gb = forms(fx, g)?;
    // a sub-box: the exact centre and radius of each spread
    type SubBox<'h> = Vec<(T<'h>, T<'h>)>;
    let root: SubBox<'h> = idx.iter().enumerate().map(|(j, &i)| (exact[i], radii[j])).collect();
    let ball = |fx: &mut Fx<'_, '_, '_, 'h>, (c, r): (T<'h>, T<'h>)| -> Option<T<'h>> { fx.ar.ball_with_radius(c, r, CELL_BITS) };
    // halve the box in spread j
    let split = |fx: &mut Fx<'_, '_, '_, 'h>, bx: &SubBox<'h>, j: usize| -> Res<(SubBox<'h>, SubBox<'h>)> {
        let (c, r) = bx[j];
        let half = fx.eval(fx.mul(&[r, fx.ar.rational(fx.int(1), fx.int(2))]))?;
        let (mut a, mut b) = (bx.clone(), bx.clone());
        a[j] = (fx.eval(fx.add(&[c, fx.neg(half)]))?, half);
        b[j] = (fx.eval(fx.add(&[c, half]))?, half);
        Ok((a, b))
    };
    let widest = |fx: &Fx<'_, '_, '_, 'h>, bx: &SubBox<'h>, among: &[usize]| -> usize {
        let w = |j: usize| fx.f64_of(bx[j].1).unwrap_or(0.0);
        among.iter().copied().max_by(|&a, &b| w(a).total_cmp(&w(b))).unwrap_or(0)
    };
    let sign_of = |e: Option<(f64, f64)>| match e {
        Some((a, b)) if a == 0.0 && b == 0.0 => Some(0i8),
        Some((a, _)) if a >= 0.0 => Some(1),
        Some((_, b)) if b <= 0.0 => Some(-1),
        _ => None,
    };
    // one sub-box: its least and largest value enclosed, with the spreads left free on each side
    struct Cell<'h> {
        bx: Vec<(T<'h>, T<'h>)>,
        lo: f64,
        hi: f64,
        free_lo: Vec<usize>,
        free_hi: Vec<usize>,
        /// the partial signs proved on this sub-box (they hold on its halves too)
        signs: Vec<Option<i8>>,
        /// how many sub-boxes in a row on the way here a partial's sign was not proved
        misses: Vec<u32>,
    }
    // of the two forms, the one with the narrower enclosure over the whole box alone (each enclosure is valid; this
    // halves the work on every sub-box)
    if let (Some(alt), Some(bs)) = (gb.1, root.iter().map(|&e| ball(fx, e)).collect::<Option<Vec<T<'h>>>>()) {
        let width = |e: Option<(f64, f64)>| e.map_or(f64::INFINITY, |(a, b)| b - a);
        let w0 = width(on_balls(fx, (gb.0, None), &bs)?);
        let w1 = width(on_balls(fx, (alt, None), &bs)?);
        if w0.is_finite() || w1.is_finite() {
            gb = (if w1 < w0 { alt } else { gb.0 }, None);
        }
    }
    let eval_cell = |fx: &mut Fx<'_, '_, '_, 'h>, bx: &SubBox<'h>, known: &[Option<i8>], missed: &[u32]| -> Res<Option<Cell<'h>>> {
        let mut bs = vec![];
        for &e in bx {
            let Some(b) = ball(fx, e) else { return Ok(None) };
            bs.push(b);
        }
        let mut signs = vec![];
        let mut misses = vec![];
        for (j, dg) in dgs.iter().enumerate() {
            let m = missed.get(j).copied().unwrap_or(0);
            // a sign missed on the way here is tried again on every other halving (a slow partial is not enclosed
            // on every sub-box)
            let s = match (known.get(j).copied().flatten(), dg) {
                (Some(s), _) => Some(s),
                (None, Some(_)) if m % 2 == 1 => None,
                (None, Some(dg)) => sign_of(on_balls(fx, *dg, &bs)?),
                (None, None) => None,
            };
            misses.push(if s.is_some() { 0 } else { m + 1 });
            signs.push(s);
        }
        let mut sides: Vec<((f64, f64), Vec<usize>)> = vec![];
        for dir in [-1i64, 1] {
            // no spread fixed: both sides are the same enclosure
            if dir == 1 && signs.iter().all(Option::is_none) {
                let first = sides[0].clone();
                sides.push(first);
                break;
            }
            // the face: a spread of proved sign at its end (the centre when constant in it), the others free
            let mut vals = bs.clone();
            let mut point = exact.clone();
            let mut free = vec![];
            for j in 0..k {
                let (c, r) = bx[j];
                match signs[j] {
                    Some(0) => vals[j] = c,
                    Some(s) => vals[j] = fx.eval(fx.add(&[c, fx.mul(&[fx.int(dir * i64::from(s)), r])]))?,
                    None => {
                        free.push(j);
                        point[idx[j]] = c;
                        continue;
                    }
                }
                point[idx[j]] = vals[j];
            }
            let e = if free.is_empty() {
                at_point(fx, &point)?
            } else {
                let natural = on_balls(fx, gb, &vals)?;
                // the mean value form in the free spreads, when their partials are known
                let mut mean_value = None;
                if free.iter().all(|&j| dgs[j].is_some()) {
                    if let Some((mut a, mut b)) = at_point(fx, &point)? {
                        let mut ok = true;
                        for &j in &free {
                            let (Some(p), Some(r)) = (on_balls(fx, dgs[j].expect("a partial"), &vals)?, fx.f64_of(bx[j].1)) else {
                                ok = false;
                                break;
                            };
                            // [pa, pb] times [-r, r] lies in [-w, w], w = max(|pa|, |pb|) r, rounded up
                            let w = (p.0.abs().max(p.1.abs()) * r.next_up()).next_up();
                            a = (a - w).next_down();
                            b = (b + w).next_up();
                        }
                        if ok && a.is_finite() && b.is_finite() {
                            mean_value = Some((a, b));
                        }
                    }
                }
                match (natural, mean_value) {
                    (Some((a, b)), Some((c, d))) if a.max(c) <= b.min(d) => Some((a.max(c), b.min(d))),
                    (Some(e), _) | (None, Some(e)) => Some(e),
                    (None, None) => None,
                }
            };
            let Some(e) = e else { return Ok(None) };
            sides.push((e, free));
        }
        let ((lo_e, free_lo), (hi_e, free_hi)) = (sides[0].clone(), sides[1].clone());
        Ok(Some(Cell { bx: bx.clone(), lo: lo_e.0, hi: hi_e.1, free_lo, free_hi, signs, misses }))
    };
    let all: Vec<usize> = (0..k).collect();
    // cover the box by sub-boxes with enclosures
    let mut cells: Vec<Cell<'h>> = vec![];
    let mut pending = vec![root];
    let mut count = 0usize;
    let none = vec![None; k];
    while let Some(bx) = pending.pop() {
        count += 1;
        if count > MAX_BOX_CELLS {
            return Ok(None);
        }
        match eval_cell(fx, &bx, &none, &[])? {
            Some(c) => cells.push(c),
            None => {
                let j = widest(fx, &bx, &all);
                let (a, b) = split(fx, &bx, j)?;
                pending.push(a);
                pending.push(b);
            }
        }
    }
    // refine the sub-boxes that give the least and the largest value, in a free spread
    // a side is done when its extreme sub-box has no free spread, or when 2 k + 1 halvings in a row (k spreads) moved its
    // bound by less than 1/500 of the hull's width
    let hull = |cells: &[Cell<'h>]| {
        (cells.iter().map(|c| c.lo).fold(f64::INFINITY, f64::min), cells.iter().map(|c| c.hi).fold(f64::NEG_INFINITY, f64::max))
    };
    let mut done = [false, false];
    let mut small = [0usize, 0];
    let mut side = 0usize;
    while count + 2 <= MAX_BOX_CELLS && !(done[0] && done[1]) {
        if done[side] {
            side = 1 - side;
            continue;
        }
        let pick = if side == 0 {
            cells.iter().enumerate().min_by(|a, b| a.1.lo.total_cmp(&b.1.lo)).map(|(i, c)| (i, c.free_lo.clone()))
        } else {
            cells.iter().enumerate().max_by(|a, b| a.1.hi.total_cmp(&b.1.hi)).map(|(i, c)| (i, c.free_hi.clone()))
        };
        let this = side;
        side = 1 - side;
        let Some((i, free)) = pick else { break };
        if free.is_empty() {
            done[this] = true;
            continue;
        }
        let before = hull(&cells);
        let cell = cells.swap_remove(i);
        let j = widest(fx, &cell.bx, &free);
        let (a, b) = split(fx, &cell.bx, j)?;
        let mut parts = vec![];
        for bx in [a, b] {
            count += 1;
            match eval_cell(fx, &bx, &cell.signs, &cell.misses)? {
                Some(c) => parts.push(c),
                None => {
                    parts.clear();
                    break;
                }
            }
        }
        if parts.len() == 2 {
            cells.extend(parts);
        } else {
            // a half without an enclosure: keep the cell as it was, and this side as it is
            cells.push(cell);
            done[this] = true;
            continue;
        }
        let after = hull(&cells);
        let gain = if this == 0 { after.0 - before.0 } else { before.1 - after.1 };
        if gain <= 2e-3 * (after.1 - after.0) {
            small[this] += 1;
            done[this] |= small[this] > 2 * k;
        } else {
            small[this] = 0;
        }
    }
    let lo = cells.iter().map(|c| c.lo).fold(f64::INFINITY, f64::min);
    let hi = cells.iter().map(|c| c.hi).fold(f64::NEG_INFINITY, f64::max);
    // an enclosure a hundred times wider than the value at the centre says nothing useful (a loose ball form):
    // the call stays
    let Some((c0, c1)) = at_point(fx, &exact)? else { return Ok(None) };
    let scale = 1.0 + c0.abs().max(c1.abs());
    Ok((lo <= hi && hi - lo <= 1e2 * scale).then_some((lo, hi)))
}

/// `head` of several centered intervals as the function of balls: each centred interval c +- r becomes the real ball
/// of midpoint c and radius r (exact arguments stay exact), and a real ball value is the result as a centered interval;
/// `None` (unevaluated) when an argument is not a real machine or exact spread or the value is not a ball.
fn centered_by_balls<'h>(fx: &mut Fx<'_, '_, '_, 'h>, head: T<'h>, centers: &[T<'h>], unc: &[Option<T<'h>>]) -> Res<Option<T<'h>>> {
    let mut xs = Vec::with_capacity(centers.len());
    for (i, d) in unc.iter().enumerate() {
        let Some(d) = d else {
            // a machine number stands for its exact binary value (so that balls stay balls)
            let c = centers[i];
            xs.push(if fx.num(c).is_some_and(|n| !n.is_exact()) { fx.eval(fx.call("set_precision", &[c, fx.inf()]))? } else { c });
            continue;
        };
        let real = |fx: &Fx<'_, '_, '_, 'h>, t: T<'h>| fx.num(t).is_some_and(|n| n.as_real().is_some());
        if !(real(fx, centers[i]) && real(fx, *d)) {
            return Ok(None);
        }
        let r = fx.eval(fx.call("abs", &[*d]))?;
        let b = match fx.ar.ball_with_radius(centers[i], r, BALL_BITS as u64) {
            Some(b) => b,
            // machine ends: the ball of exactly that centre and radius
            None => match (fx.machine_of(centers[i]), fx.f64_of(r)) {
                (Some(c), Some(r)) if c.is_finite() && r.is_finite() => crate::enclose::ball_mid_rad(fx, c, r.next_up()),
                _ => return Ok(None),
            },
        };
        xs.push(b);
    }
    let v = fx.eval(fx.h.app(head, &xs))?;
    if !matches!(fx.num(v), Some(Num::Real(Real::Ball(..)))) {
        return Ok(None);
    }
    Ok(from_ball(fx, v))
}

/// `centered_interval` result of a function of one uncertain argument as the proved image of the spread
/// ([`crate::enclose::image`] over [c - r, c + r], the ends rounded outwards): the midpoint and half-width of its hull,
/// the half-width rounded up; `None` when the argument is not a single non-degenerate real spread, or the image is not
/// proved or is unbounded (a pole inside: the call stays).
fn centered_image<'h>(fx: &mut Fx<'_, '_, '_, 'h>, head: T<'h>, centers: &[T<'h>], unc: &[Option<T<'h>>]) -> Res<Option<T<'h>>> {
    let mut which = unc.iter().enumerate().filter(|(_, d)| d.is_some());
    let (Some((i, Some(d))), None) = (which.next(), which.next()) else { return Ok(None) };
    let (Some(c), Some(d)) = (machine(fx, centers[i])?, machine(fx, *d)?) else { return Ok(None) };
    if d.is_nan() || d <= 0.0 || !d.is_finite() || !c.is_finite() {
        return Ok(None);
    }
    // the spread as machine ends that contain it (c -+ d rounded outwards)
    let (a, b) = ((c - d).next_down(), (c + d).next_up());
    let Some(ps) = image(fx, head, centers, i, (fx.real(a), fx.real(b)))? else { return Ok(None) };
    // the hull of the pieces (a union of rays across a pole is unbounded: the call stays)
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for &(l, h) in &ps {
        let (Some(l), Some(h)) = (machine(fx, l)?, machine(fx, h)?) else { return Ok(None) };
        lo = lo.min(l);
        hi = hi.max(h);
    }
    if !(lo.is_finite() && hi.is_finite() && lo <= hi) {
        return Ok(None);
    }
    let mid = 0.5 * (lo + hi);
    let rad = (0.5 * (hi - lo) + 4.0 * f64::EPSILON * (lo.abs() + hi.abs())) * (1.0 + 4.0 * f64::EPSILON);
    Ok(Some(fx.call("centered_interval", &[fx.real(mid), fx.real(rad)])))
}

/// The functions that are constant between jumps (their derivative is 0 almost everywhere).
const STEP_FUNCTIONS: &[&str] =
    &["floor", "ceiling", "round", "integer_part", "sign", "unit_step", "real_sign", "unit_box", "heaviside_theta", "kronecker_delta", "boole"];

/// The step functions that are monotone (non-decreasing): equal values at the two ends of a spread make them constant
/// on it. `unit_box`, `kronecker_delta` and `boole` are not, and their value is taken only for a point.
const MONOTONE_STEPS: &[&str] = &["floor", "ceiling", "round", "integer_part", "sign", "unit_step", "real_sign", "heaviside_theta"];

/// A step function of uncertain arguments: its value when the ends of every spread and the centre give the same value
/// (a monotone step function is then constant over the spread), `indeterminate` when they differ; a step function
/// that is not monotone stays unevaluated over a spread (equal end values do not exclude a different value inside,
/// as unit_box(2 -+ 3) has 1 at 0).
fn step<'h>(fx: &mut Fx<'_, '_, '_, 'h>, head: T<'h>, centers: &[T<'h>], unc: &[Option<T<'h>>]) -> Res<Option<T<'h>>> {
    let monotone =
        centers.len() == 1 && matches!(fx.h.view(head), View::Sym(s) if MONOTONE_STEPS.contains(&fx.h.sym_name(s).as_str()));
    let spread = |fx: &mut Fx<'_, '_, '_, 'h>, d: T<'h>| -> Res<bool> { Ok(machine(fx, d)?.is_none_or(|v| v != 0.0)) };
    if !monotone {
        for d in unc.iter().flatten() {
            if spread(fx, *d)? {
                return Ok(None);
            }
        }
    }
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
    // a real infinity against a closed real term (-inf < tan(1) stays unevaluated as a relation)
    match (fx.infinity(a), fx.infinity(b)) {
        (Some(2), Some(2)) | (Some(0), _) | (_, Some(2)) => return Ok(Some(false)),
        (Some(2), _) | (_, Some(0)) => return Ok(Some(true)),
        _ => {}
    }
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

/// The machine ends of computed pieces moved outwards by one unit in the last place: a machine result is
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
/// fixed: its pieces, proved by [`crate::enclose::image`] (`None`: not proved, the call stays).
fn image<'h>(
    fx: &mut Fx<'_, '_, '_, 'h>,
    head: T<'h>,
    args: &[T<'h>],
    i: usize,
    piece: (T<'h>, T<'h>),
) -> Res<Option<Vec<(T<'h>, T<'h>)>>> {
    crate::enclose::image(fx, head, args, i, piece)
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
    if at.len() >= 2 {
        return several_intervals(fx, head, args, &at);
    }
    let [i] = at[..] else { return Ok(None) };
    let Some(ps) = pieces(fx, args[i]) else { return Ok(None) };
    let mut out = vec![];
    for p in ps {
        // a step function that is not monotone (unit_box, kronecker_delta): its derivative is 0 where it exists, which
        // proves nothing across a jump; the call stays
        if STEP_FUNCTIONS.contains(&name.as_str()) && !(args.len() == 1 && MONOTONE_STEPS.contains(&name.as_str())) {
            return Ok(None);
        }
        // a monotone step function: the hull of its values at the ends
        if args.len() == 1 && MONOTONE_STEPS.contains(&name.as_str()) {
            let u = fx.eval(fx.h.app(head, &[p.0]))?;
            let v = fx.eval(fx.h.app(head, &[p.1]))?;
            if fx.num(u).is_none() || fx.num(v).is_none() {
                return Ok(None);
            }
            out.push((u, v));
            continue;
        }
        if let Some(s) = periodic_cover(fx, &name, args.len(), p)? {
            out.push(s);
            continue;
        }
        let Some(mut s) = image(fx, head, args, i, p)? else { return Ok(None) };
        out.append(&mut s);
    }
    // the proved images are already rounded outwards
    make_interval(fx, out)
}

/// A function of several intervals (one piece each, finite numeric ends): the box is the product of the pieces (the
/// exact centre (a + b)/2 and radius (b - a)/2 of each), and its image is enclosed by [`box_image`] (the corner hull
/// when every partial derivative's sign is proved over the box, else interval arithmetic on the expression); the ends
/// are rounded outwards. `None` otherwise (the call stays).
fn several_intervals<'h>(fx: &mut Fx<'_, '_, '_, 'h>, head: T<'h>, args: &[T<'h>], at: &[usize]) -> Res<Option<T<'h>>> {
    let mut centers = args.to_vec();
    let mut unc = vec![None; args.len()];
    for &i in at {
        let Some(ps) = pieces(fx, args[i]) else { return Ok(None) };
        let [(a, b)] = ps[..] else { return Ok(None) };
        let (Some(lo), Some(hi)) = (machine(fx, a)?, machine(fx, b)?) else { return Ok(None) };
        if !(lo.is_finite() && hi.is_finite() && lo <= hi) {
            return Ok(None);
        }
        // exact ends: numbers (machine ends as their binary values); a closed form end through 30 digits is not exact
        let (Some(_), Some(_)) = (fx.num(a), fx.num(b)) else { return Ok(None) };
        let (a, b) = (exact_of(fx, a)?, exact_of(fx, b)?);
        centers[i] = fx.eval(fx.mul(&[fx.add(&[a, b]), fx.ar.rational(fx.int(1), fx.int(2))]))?;
        unc[i] = Some(fx.eval(fx.mul(&[fx.add(&[b, fx.neg(a)]), fx.ar.rational(fx.int(1), fx.int(2))]))?);
    }
    // the other arguments must be numbers
    for (i, &c) in centers.iter().enumerate() {
        if unc[i].is_none() && fx.num(c).is_none() && machine(fx, c)?.is_none() {
            return Ok(None);
        }
    }
    let Some((lo, hi)) = box_image(fx, head, &centers, &unc)? else { return Ok(None) };
    let ps = outward(fx, vec![(fx.real(lo), fx.real(hi))]);
    make_interval(fx, ps)
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
/// The precision of the balls of the sub-boxes of a box (bits; a wide ball needs no more).
const CELL_BITS: u64 = 64;
/// At most this many sub-boxes (each enclosed once) in [`box_image`].
const MAX_BOX_CELLS: usize = 96;

/// A ball as `centered_interval(mid, rad)` with machine numbers (the radius rounded up).
fn from_ball<'h>(fx: &Fx<'_, '_, '_, 'h>, t: T<'h>) -> Option<T<'h>> {
    match fx.num(t)? {
        Num::Real(Real::Ball(b, _)) => {
            let (mid, rad) = machine_ball(&b);
            Some(fx.call("centered_interval", &[fx.real(mid), fx.real(rad)]))
        }
        Num::Real(Real::Exact(_)) => Some(fx.call("centered_interval", &[t, fx.int(0)])),
        _ => None,
    }
}

/// The machine midpoint of a ball and a machine radius that encloses the ball (rounded up).
fn machine_ball(b: &mc_arith::num::Ball) -> (f64, f64) {
    let mid = Real::Ball(mc_arith::num::Ball::exact(b.mid.clone()), BALL_BITS as u64).to_f64();
    let rad = (b.rad.m as f64) * 2f64.powi(b.rad.e as i32);
    // the rounding of the midpoint to a machine number (half an ulp) belongs to the radius too
    let rad = (rad + 0.5 * mid.abs() * f64::EPSILON) * (1.0 + 4.0 * f64::EPSILON) + f64::from_bits(1);
    (mid, rad)
}

/// `centered_interval(x)` of an exact number or a numeric constant (`1/3`, `e + i pi`): the machine enclosure of a
/// ball of x (a box for a complex x), so that the interval holds x itself.
fn exact_enclosure<'h>(fx: &mut Fx<'_, '_, '_, 'h>, p: T<'h>) -> Res<Option<T<'h>>> {
    let b = fx.eval(fx.call("n", &[p, fx.int(40)]))?;
    Ok(match fx.num(b) {
        Some(Num::Real(Real::Ball(rb, _))) => {
            let (m, r) = machine_ball(&rb);
            Some(fx.call("centered_interval", &[fx.real(m), fx.real(r)]))
        }
        Some(Num::Complex(Real::Ball(re, _), Real::Ball(im, _))) => {
            let ((mr, rr), (mi, ri)) = (machine_ball(&re), machine_ball(&im));
            let c = fx.ar.complex(fx.real(mr), fx.real(mi));
            let r = fx.ar.complex(fx.real(rr), fx.real(ri));
            Some(fx.call("centered_interval", &[c, r]))
        }
        _ => None,
    })
}

fn centered_apply<'h>(fx: &mut Fx<'_, '_, '_, 'h>, head: T<'h>, args: &[T<'h>]) -> Res<Option<T<'h>>> {
    propagate(fx, head, args, "centered_interval")
}

// ---------------------------------------------------------------- constructors

fn around_e<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> Res<Option<T<'h>>> {
    if a.len() == 1 {
        // around(list): the mean and the standard deviation
        let Some(xs) = fx.as_list(a[0]) else {
            // around(distribution): its mean and standard deviation, when both are numbers
            let is_dist = matches!(fx.h.view(a[0]), View::App { head, .. }
                if matches!(fx.h.view(head), View::Sym(s) if fx.h.sym_name(s).ends_with("_distribution")));
            if !is_dist {
                return Ok(None);
            }
            let m = fx.eval(fx.call("mean", &[a[0]]))?;
            let s = fx.eval(fx.call("std_dev", &[a[0]]))?;
            return Ok((fx.num(m).is_some() && fx.num(s).is_some()).then(|| fx.call("around", &[m, s])));
        };
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
        // a dimensionless quantity (`scaled(quantity(5, "Percent"))`) is its number
        if fx.call_of(d, "quantity").is_some() {
            let v = fx.eval(fx.call("unit_convert", &[d]))?;
            if fx.num(v).is_some() {
                d = v;
            }
        }
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
    // the call as given (a relative uncertainty `scaled(s)` is replaced by the absolute one even when that is the
    // machine number already)
    if xr == x && dr == a[1] {
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
            // an interval [lo, hi]: its midpoint and half-width, enclosed in machine numbers
            if let Some(&[iv]) = fx.call_of(*p, "interval") {
                if let Some(&[lo, hi]) = fx.as_list(iv) {
                    let (Some(l), Some(u)) = (fx.f64_of(lo), fx.f64_of(hi)) else { return Ok(None) };
                    if !(l.is_finite() && u.is_finite()) {
                        return Ok(None);
                    }
                    // the ends rounded to machine numbers (half an ulp each), the midpoint and the half-width
                    // rounded once more: all of it goes into the radius
                    let (m, h) = (0.5 * l + 0.5 * u, 0.5 * u - 0.5 * l);
                    let slack = (l.abs() + u.abs() + m.abs()) * f64::EPSILON;
                    let rad = (h + slack) * (1.0 + 4.0 * f64::EPSILON) + f64::from_bits(1);
                    return Ok(Some(fx.call("centered_interval", &[fx.real(m), fx.real(rad)])));
                }
                return Ok(None);
            }
            return match fx.num(*p) {
                Some(Num::Real(Real::Ball(..))) => Ok(from_ball(fx, *p)),
                // a machine number stands for the numbers that round to it and to its neighbours: |x| eps
                Some(Num::Real(Real::Machine(v))) => {
                    let r = if v == 0.0 { fx.int(0) } else { fx.real(v.abs() * f64::EPSILON) };
                    Ok(Some(fx.call("centered_interval", &[*p, r])))
                }
                Some(Num::Real(Real::Exact(_))) | Some(Num::Complex(Real::Exact(_), _)) => exact_enclosure(fx, *p),
                // a complex machine number or ball: the box of the enclosures of its two parts
                Some(Num::Complex(re, im)) => {
                    let (Some((mr, rr)), Some((mi, ri))) = (part_enclosure(&re), part_enclosure(&im)) else {
                        return Ok(None);
                    };
                    let c = fx.ar.complex(fx.real(mr), fx.real(mi));
                    let r = fx.ar.complex(fx.real(rr), fx.real(ri));
                    Ok(Some(fx.call("centered_interval", &[c, r])))
                }
                // a numeric constant (e + i pi): the enclosure of its value; a symbol stays
                None => exact_enclosure(fx, *p),
            };
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
                // the midpoint rounded to a machine number: half an ulp of it belongs to the radius
                rads.push(fx.real((r + 0.5 * c.abs() * f64::EPSILON) * (1.0 + 4.0 * f64::EPSILON) + f64::from_bits(1)));
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
    // a centre and a radius: machine numbers, the centre rounded to the nearest and the radius rounded up with
    // the rounding of the centre added to it, so that the interval holds the one given; a complex centre takes
    // a complex radius (the half-widths of the box in the real and the imaginary direction)
    let (Some(c), Some(r)) = (fx.num(a[0]), fx.num(a[1])) else { return Ok(None) };
    let machine_only = |n: &Num| match n {
        Num::Real(x) => matches!(x, Real::Machine(_)),
        Num::Complex(x, y) => matches!((x, y), (Real::Machine(_), Real::Machine(_))),
    };
    if machine_only(&c) && machine_only(&r) {
        return Ok(None);
    }
    let parts = |n: &Num| match n {
        Num::Real(x) => (x.clone(), None),
        Num::Complex(x, y) => (x.clone(), Some(y.clone())),
    };
    let ((cr, mut ci), (rr, ri)) = (parts(&c), parts(&r));
    // a real centre with a complex radius is the box around c + 0 i
    if ci.is_none() && ri.is_some() {
        ci = Some(Real::int(0));
    }
    if ci.is_some() != ri.is_some() || rr.is_negative() || ri.as_ref().is_some_and(|y| y.is_negative()) {
        return Ok(None);
    }
    let mut mids = vec![];
    let mut rads = vec![];
    for (cp, rp) in std::iter::once((cr, rr)).chain(ci.zip(ri)) {
        let (Some((m, cm)), Some((rm, rrad))) = (part_enclosure(&cp), part_enclosure(&rp)) else {
            return Ok(None);
        };
        // centre and radius both machine numbers exactly (0 and 1/2): the radius as it is, nothing to round
        let rad = if cm == 0.0 && rrad == 0.0 { rm.abs() } else { (cm + rm.abs() + rrad) * (1.0 + 4.0 * f64::EPSILON) + f64::from_bits(1) };
        if !(m.is_finite() && rad.is_finite()) {
            return Ok(None);
        }
        mids.push(fx.real(m));
        rads.push(fx.real(rad));
    }
    Ok(Some(if mids.len() == 2 {
        let c = fx.ar.complex(mids[0], mids[1]);
        let r = fx.ar.complex(rads[0], rads[1]);
        fx.call("centered_interval", &[c, r])
    } else {
        fx.call("centered_interval", &[mids[0], rads[0]])
    }))
}

/// A machine midpoint of a real number and a radius that holds the number around it: a machine number is
/// itself, a ball its enclosure, an exact number the enclosure of a 100-bit ball of it.
fn part_enclosure(x: &Real) -> Option<(f64, f64)> {
    match x {
        Real::Machine(v) => Some((*v, 0.0)),
        Real::Ball(b, _) => Some(machine_ball(b)),
        // a machine number exactly (0, 1/2, 3/8): itself
        Real::Exact(q)
            if (1..=1000).contains(&q.den.bits())
                && q.num.mag.bits() <= 53
                && q.den == mc_arith::num::Nat::from_u64(1).shl(q.den.bits() - 1) =>
        {
            Some((x.to_f64(), 0.0))
        }
        Real::Exact(_) => match x.to_kind(mc_arith::Kind::Ball, BALL_BITS as u64) {
            Real::Ball(b, _) => Some(machine_ball(&b)),
            _ => None,
        },
    }
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
/// number or another interval: `(hi(a) vs lo(b),
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
