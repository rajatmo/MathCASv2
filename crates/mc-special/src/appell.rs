//! The Appell hypergeometric functions of two variables, DLMF 16.13:
//!
//! - `appell_f1(a, b1, b2, c, x, y)` = sum (a)_(m+n) (b1)_m (b2)_n/((c)_(m+n) m! n!) x^m y^n;
//! - `appell_f2(a, b1, b2, c1, c2, x, y)` = sum (a)_(m+n) (b1)_m (b2)_n/((c1)_m (c2)_n m! n!) x^m y^n;
//! - `appell_f3(a1, a2, b1, b2, c, x, y)` = sum (a1)_m (a2)_n (b1)_m (b2)_n/((c)_(m+n) m! n!) x^m y^n;
//! - `appell_f4(a, b, c1, c2, x, y)` = sum (a)_(m+n) (b)_(m+n)/((c1)_m (c2)_n m! n!) x^m y^n.
//!
//! Numerical values: the sum over m of the row coefficients times a Gauss function in the other variable (DLMF
//! 16.13 summed over n first): F1 = sum_m (a)_m (b1)_m/((c)_m m!) x^m 2F1(a + m, b2; c + m; y), F2 = sum_m (a)_m
//! (b1)_m/((c1)_m m!) x^m 2F1(a + m, b2; c2; y), F3 = sum_m (a1)_m (b1)_m/((c)_m m!) x^m 2F1(a2, b2; c + m; y), F4 =
//! sum_m (a)_m (b)_m/((c1)_m m!) x^m 2F1(a + m, b + m; c2; y), with the variable of smaller modulus outside (the
//! functions are symmetric under the exchange of the two variables with their parameters). The 2F1 continues beyond
//! the unit disk, so F1 and F3 are evaluated for |x| < 1 or |y| < 1 and any other variable off the cut; F2 and F4 where
//! the row sum converges. At x = 0 or y = 0 the functions are Gauss functions.
use crate::cover::{mag, numeric, work};
use mc_api::T;
use mc_eval::Res;
use mc_numeric::formula::{Def, Fx};

type R<'h> = Res<Option<T<'h>>>;

#[derive(Clone, Copy, PartialEq)]
enum Which {
    F1,
    F2,
    F3,
    F4,
}

/// The parameters (without x, y) with the two variables exchanged.
fn swapped<'h>(which: Which, p: &[T<'h>]) -> Vec<T<'h>> {
    match which {
        // a, b1, b2, c
        Which::F1 => vec![p[0], p[2], p[1], p[3]],
        // a, b1, b2, c1, c2
        Which::F2 => vec![p[0], p[2], p[1], p[4], p[3]],
        // a1, a2, b1, b2, c
        Which::F3 => vec![p[1], p[0], p[3], p[2], p[4]],
        // a, b, c1, c2
        Which::F4 => vec![p[0], p[1], p[3], p[2]],
    }
}

/// The row coefficient ratio r_(m+1)/r_m (as a term in m) and the Gauss parameters of row m.
fn row<'h>(fx: &Fx<'_, '_, '_, 'h>, which: Which, p: &[T<'h>], m: i64) -> (T<'h>, [T<'h>; 3]) {
    let k = |t: T<'h>, d: i64| fx.add(&[t, fx.int(d)]);
    let m1 = fx.int(m + 1);
    match which {
        Which::F1 => (fx.div(fx.mul(&[k(p[0], m), k(p[1], m)]), fx.mul(&[k(p[3], m), m1])), [k(p[0], m), p[2], k(p[3], m)]),
        Which::F2 => (fx.div(fx.mul(&[k(p[0], m), k(p[1], m)]), fx.mul(&[k(p[3], m), m1])), [k(p[0], m), p[2], p[4]]),
        Which::F3 => (fx.div(fx.mul(&[k(p[0], m), k(p[2], m)]), fx.mul(&[k(p[4], m), m1])), [p[1], p[3], k(p[4], m)]),
        Which::F4 => (fx.div(fx.mul(&[k(p[0], m), k(p[1], m)]), fx.mul(&[k(p[2], m), m1])), [k(p[0], m), k(p[1], m), p[3]]),
    }
}

/// The value at working-precision numbers.
fn appell_at<'h>(fx: &mut Fx<'_, '_, '_, 'h>, which: Which, p: &[T<'h>], x: T<'h>, y: T<'h>, w: u64) -> R<'h> {
    let eps = 2f64.powi(-(w.min(1000) as i32));
    let mut sum = fx.int(0);
    let mut coef = fx.int(1); // the row coefficient times x^m
    let mut small = 0;
    let mut growing = 0;
    let mut last = f64::INFINITY;
    // the Gauss functions at the midpoint of y (a ball y of radius 2^-w gave real parts whose radius grows with w at
    // integer c - a - b beyond the unit disk); the radius of y is far below the target precision
    let y = crate::cover::mid_term(fx, y);
    for m in 0..4000i64 {
        if m % 16 == 15 {
            fx.tick(1)?;
        }
        let (ratio, g) = row(fx, which, p, m);
        let f = fx.eval(fx.call("hypergeometric_2f1", &[g[0], g[1], g[2], y]))?;
        let f = if fx.num(f).is_some() { f } else { crate::cover::numify(fx, f, &[y])? };
        if fx.num(f).is_none() {
            return Ok(None);
        }
        let t = fx.eval(fx.mul(&[coef, f]))?;
        sum = fx.eval(fx.add(&[sum, t]))?;
        let mt = mag(fx, t);
        if mt <= eps * mag(fx, sum).max(1e-300) {
            small += 1;
            if small >= 3 {
                return Ok(fx.num(sum).map(|_| sum));
            }
        } else {
            small = 0;
        }
        if m > 50 && mt > last {
            growing += 1;
            if growing > 50 {
                return Ok(None);
            }
        } else {
            growing = 0;
        }
        last = mt;
        coef = fx.eval(fx.mul(&[coef, ratio, x]))?;
        if fx.num(coef).is_some_and(|c| c.is_zero()) {
            return Ok(fx.num(sum).map(|_| sum));
        }
    }
    Ok(None)
}

fn appell<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>], which: Which) -> R<'h> {
    let n = a.len();
    let (x, y) = (a[n - 2], a[n - 1]);
    let p = &a[..n - 2];
    // a zero variable: the Gauss function of the other one
    let gauss = |fx: &Fx<'_, '_, '_, 'h>, p: &[T<'h>], y: T<'h>| {
        let (_, g) = row(fx, which, p, 0);
        fx.call("hypergeometric_2f1", &[g[0], g[1], g[2], y])
    };
    if fx.is_int(x, 0) {
        return Ok(Some(gauss(fx, p, y)));
    }
    if fx.is_int(y, 0) {
        return Ok(Some(gauss(fx, &swapped(which, p), x)));
    }
    // c = a: the ratio (a)_(m+n)/(c)_(m+n) is 1 and the double series is the product of two binomial series,
    // F1(a; b1, b2; a; x, y) = (1 - x)^-b1 (1 - y)^-b2 (DLMF 16.16.1 at c = a, from the definition 16.13.1)
    if which == Which::F1 {
        let d = fx.eval(fx.sub(p[0], p[3]))?;
        if fx.is_int(d, 0) {
            let one = fx.int(1);
            return Ok(Some(fx.mul(&[fx.pow(fx.sub(one, x), fx.neg(p[1])), fx.pow(fx.sub(one, y), fx.neg(p[2]))])));
        }
    }
    if !numeric(fx, a) {
        return Ok(None);
    }
    // machine arguments: the double-double kernel (round 29), the ball path when it declines
    if let Some((ms, real)) = crate::fast29::machine_args(fx, a) {
        let code = match which {
            Which::F1 => 0,
            Which::F2 => 1,
            Which::F3 => 2,
            Which::F4 => 3,
        };
        if let Some(v) = crate::fast29::appell(code, &ms[..n - 2], ms[n - 2], ms[n - 1]) {
            return Ok(crate::fast29::machine_out(fx, v, real));
        }
    }
    // machine arguments (no balls): the value at their exact (dyadic) values to 17 digits, rounded to machine. The
    // ball loop below would retry at growing precision until it has 16 digits, and high-precision Gauss functions
    // are slow (2 s for appell_f1(2, 1, 1, 3, 0.7, 0.3), round 28)
    let kinds: Vec<Option<mc_arith::Kind>> = a.iter().map(|&t| fx.num(t).map(|v| v.kind())).collect();
    if kinds.contains(&Some(mc_arith::Kind::Machine)) && !kinds.contains(&Some(mc_arith::Kind::Ball)) {
        let name = match which {
            Which::F1 => "appell_f1",
            Which::F2 => "appell_f2",
            Which::F3 => "appell_f3",
            Which::F4 => "appell_f4",
        };
        let mut ex = Vec::with_capacity(n);
        for &t in a {
            ex.push(if fx.num(t).is_some_and(|v| v.kind() == mc_arith::Kind::Machine) {
                fx.eval(fx.call("set_precision", &[t, fx.sym("inf")]))?
            } else {
                t
            });
        }
        if ex.iter().all(|&t| fx.num(t).is_some_and(|v| v.is_exact())) {
            let v = fx.eval(fx.call("n", &[fx.call(name, &ex), fx.int(17)]))?;
            if fx.num(v).is_some() {
                return Ok(Some(fx.eval(fx.call("n", &[v]))?));
            }
        }
    }
    // the working precision raised while the value lacks the digits of the arguments' kind (the Gauss functions beyond
    // the unit disk lose bits in one part)
    let mut extra = 32;
    let mut best = None;
    for _ in 0..4 {
        let Some((wk, xs)) = work(fx, a, extra) else { return Ok(None) };
        let (xw, yw) = (xs[n - 2], xs[n - 1]);
        // exact parameters stay exact (the Gauss function of exact parameters keeps its digits beyond the unit disk;
        // with ball parameters it loses them in one part)
        let pv: Vec<T<'h>> = (0..n - 2).map(|i| if fx.num(a[i]).is_some_and(|v| v.is_exact()) { a[i] } else { xs[i] }).collect();
        let pw = &pv[..];
        let v = if mag(fx, xw) <= mag(fx, yw) {
            appell_at(fx, which, pw, xw, yw, wk.w)?
        } else {
            appell_at(fx, which, &swapped(which, pw), yw, xw, wk.w)?
        };
        let Some(v) = v else { return Ok(None) };
        let goal = if wk.machine { 16 } else { ((wk.p as f64 * std::f64::consts::LOG10_2) as u32).saturating_sub(2) };
        // the digits of the sum itself decide a retry: the widening below does not depend on the working precision,
        // and tested after it the loop retried up to 818 bits whenever 2^-(p - 8) fell below the goal (6 s for
        // n(appell_f1(1, 1, 3/5, 3, 2/3, 2/5), 22), round 29)
        let ok = crate::cover::accurate(fx, v, goal);
        // the Gauss functions were taken at the midpoint of y: the radius 2^-(p - 8) |v| for the error of the
        // arguments at the target precision and of the real part of the ball Gauss function (see NOTES.md)
        let mv = mag(fx, v) * 2f64.powi(-((wk.p.saturating_sub(8)).min(1000) as i32));
        let m = mc_arith::num::Mag::from_float_up(&crate::ball::float_of(mv.max(1e-300)));
        let v = crate::cover::map_balls(fx, v, |b| crate::ball::widen(b.clone(), &m));
        let real = wk.real && crate::cover::parts(fx, v).is_some_and(|z| z.1 == 0.0);
        best = wk.out(fx, v, real);
        if ok {
            break;
        }
        extra = extra * 2 + 64;
    }
    Ok(best)
}

fn appell_f1<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    appell(fx, a, Which::F1)
}
fn appell_f2<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    appell(fx, a, Which::F2)
}
fn appell_f3<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    appell(fx, a, Which::F3)
}
fn appell_f4<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    appell(fx, a, Which::F4)
}

macro_rules! def {
    ($name:literal, $ar:expr, $e:expr, $s:literal) => {
        Def { name: $name, arities: $ar, listable: true, exact: Some($e), numeric: None, summary: $s }
    };
}

/// The definitions of this module.
pub static DEFS: &[Def] = &[
    def!("appell_f1", &[6], appell_f1, "appell_f1(a, b1, b2, c, x, y): the Appell function F1, the sum of (a)_(m+n) (b1)_m (b2)_n/((c)_(m+n) m! n!) x^m y^n"),
    def!("appell_f2", &[7], appell_f2, "appell_f2(a, b1, b2, c1, c2, x, y): the Appell function F2, the sum of (a)_(m+n) (b1)_m (b2)_n/((c1)_m (c2)_n m! n!) x^m y^n"),
    def!("appell_f3", &[7], appell_f3, "appell_f3(a1, a2, b1, b2, c, x, y): the Appell function F3, the sum of (a1)_m (a2)_n (b1)_m (b2)_n/((c)_(m+n) m! n!) x^m y^n"),
    def!("appell_f4", &[6], appell_f4, "appell_f4(a, b, c1, c2, x, y): the Appell function F4, the sum of (a)_(m+n) (b)_(m+n)/((c1)_m (c2)_n m! n!) x^m y^n"),
];
