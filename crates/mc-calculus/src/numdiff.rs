//! Numerical values of derivatives that have no rule (round 37): `derivative(n1, ..., nk)(f)(x1, ..., xk)` at
//! inexact numbers, for a named function `f` that evaluates at numbers but whose derivative in one of the arguments
//! has no closed form here (the order of a Bessel-type function, the parameter of a Mathieu characteristic value, the
//! argument of `riemann_siegel_z` or `factorial2`).
//!
//! The method is the central difference quotient with Richardson extrapolation (Dahlquist and Bjorck, *Numerical
//! Methods in Scientific Computing*, 2008, sections 3.3.4-3.3.5): for each differentiated argument the n-th central
//! difference `delta_h^n f(x) / h^n = sum_j (-1)^j C(n, j) f(x + (n/2 - j) h) / h^n` has an error series in even
//! powers of h, so the table `T(k, m) = T(k, m-1) + (T(k, m-1) - T(k-1, m-1)) / (4^m - 1)` over the steps
//! `h_k = h_0 / 2^k` removes them one by one. Mixed partial derivatives use the product of the one-dimensional
//! differences (the same even error series). The function is evaluated at exact dyadic points in balls at the
//! target precision plus 12 + n (10 + levels) log10 2 digits, the cancellation of the n-th difference at the
//! smallest step; levels are added (at most 4 + digits/8) until the table converges.
//!
//! A value is returned only when it is confirmed by two independent checks; otherwise the call stays unevaluated:
//!
//! 1. the last two diagonal entries of the table agree to the target precision plus two digits relative to the
//!    value (for machine targets also absolutely, to 10^-(digits+4) of max |f| / scale^n: a nearly vanishing
//!    derivative, as at the points `find_root` seeks), and every function value carries that many correct digits;
//! 2. a one-sided (forward) difference quotient with one Richardson step agrees with it to three digits, which
//!    rules out a kink or a jump at the point (where the central quotient would give the mean of two slopes).
//!
//! Every evaluation charges a tick and the loop polls the stop flag through the evaluator.
use mc_api::{Store, View, T};
use mc_arith::{Arith, Kind, Num, Real};
use mc_eval::{NativeCx, Res};

/// The first step is 2^-STEP_BITS times the scale of the argument.
const STEP_BITS: i64 = 10;

fn abs_f64(n: &Num) -> f64 {
    match n {
        Num::Real(r) => r.to_f64().abs(),
        Num::Complex(a, b) => a.to_f64().hypot(b.to_f64()),
    }
}

/// An upper bound of the ball radius of a number (machine numbers count their rounding unit).
fn radius(n: &Num) -> f64 {
    let r = |x: &Real| match x {
        Real::Exact(_) => 0.0,
        Real::Ball(b, _) => {
            if b.rad.is_zero() {
                0.0
            } else {
                2f64.powf(b.rad.log2())
            }
        }
        Real::Machine(v) => v.abs() * f64::EPSILON,
    };
    match n {
        Num::Real(a) => r(a),
        Num::Complex(a, b) => r(a) + r(b),
    }
}

/// The function at the base point moved by offsets times the steps of a level.
type EvalAt<'a, 'h> = dyn FnMut(&mut NativeCx<'_, '_, 'h>, &[T<'h>], i64) -> Res<Option<Num>> + 'a;

fn binomial(n: i64, k: i64) -> i64 {
    (0..k).fold(1i64, |acc, i| acc * (n - i) / (i + 1))
}

/// The numerical value of `derivative(orders)(f)(args)`, or `None` (unevaluated) when the arguments are not inexact
/// numbers, the function does not evaluate to numbers there, or the checks of the module documentation fail.
pub fn value<'h>(cx: &mut NativeCx<'_, '_, 'h>, f: T<'h>, orders: &[i64], args: &[T<'h>]) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    if !matches!(h.view(f), View::Sym(_)) || orders.len() != args.len() || orders.iter().all(|&n| n == 0) {
        return Ok(None);
    }
    let ar = Arith::new(h);
    let nums: Vec<Option<Num>> = args.iter().map(|&a| ar.num(a)).collect();
    // every differentiated argument a number; at least one inexact argument fixes the precision
    if orders.iter().zip(&nums).any(|(&n, x)| n > 0 && x.is_none()) {
        return Ok(None);
    }
    let kinds: Vec<Kind> = nums.iter().flatten().map(Num::kind).collect();
    if !kinds.iter().any(|&k| k != Kind::Exact) {
        return Ok(None);
    }
    let machine = kinds.contains(&Kind::Machine);
    let digits: u64 = if machine {
        16
    } else {
        let bits = nums
            .iter()
            .flatten()
            .filter_map(|n| match n {
                Num::Real(r) => r.prec(),
                Num::Complex(a, b) => a.prec().max(b.prec()),
            })
            .min()
            .unwrap_or(53);
        ((bits as f64) * std::f64::consts::LOG10_2).floor().max(2.0) as u64
    };
    let total: i64 = orders.iter().sum();
    // levels until the extrapolated error is below the target (each removes two powers of h); the working precision
    // covers the cancellation of the n-th difference at the smallest step, 2^-(STEP_BITS + levels) times the scale
    let levels = 4 + digits as usize / 8;
    let wp = digits + 12 + ((total as f64) * (STEP_BITS as f64 + levels as f64) * std::f64::consts::LOG10_2).ceil() as u64;
    // the arguments at the working precision: inexact numbers become balls of wp digits (exact ones stay)
    let mut base = Vec::with_capacity(args.len());
    for (i, &a) in args.iter().enumerate() {
        base.push(match &nums[i] {
            Some(n) if n.kind() != Kind::Exact => {
                let b = cx.eval(h.call(h.sym("set_precision"), &[a, h.int(wp as i64)]))?;
                if ar.num(b).is_none_or(|n| n.kind() != Kind::Ball) {
                    return Ok(None);
                }
                b
            }
            _ => a,
        });
    }
    // the step of each differentiated argument: 2^-STEP_BITS times the power of two above max(1, |x|)
    let mut step_exp = vec![0i64; args.len()];
    for (i, &n) in orders.iter().enumerate() {
        if n > 0 {
            let m = abs_f64(nums[i].as_ref().expect("a number")).max(1.0);
            if !m.is_finite() {
                return Ok(None);
            }
            step_exp[i] = m.log2().ceil() as i64 - STEP_BITS;
        }
    }
    let pow2 = |e: i64| ar.pow(h.int(2), h.int(e));
    let scale = std::cell::Cell::new(0.0f64);
    let worst_digits = std::cell::Cell::new(f64::INFINITY);
    // f at the base point moved by offsets[i] * h_i (offsets exact rationals)
    let mut eval_at = |cx: &mut NativeCx<'_, '_, 'h>, offs: &[T<'h>], level: i64| -> Res<Option<Num>> {
        let mut xs = base.clone();
        for (i, &o) in offs.iter().enumerate() {
            if orders[i] > 0 {
                xs[i] = ar.add(&[base[i], ar.mul(&[o, pow2(step_exp[i] - level)])]);
            }
        }
        cx.tick(1)?;
        let v = cx.eval(h.app(f, &xs))?;
        let Some(n) = ar.num(v).filter(|n| n.kind() == Kind::Ball) else { return Ok(None) };
        let a = abs_f64(&n);
        if !a.is_finite() {
            return Ok(None);
        }
        scale.set(scale.get().max(a));
        let r = radius(&n);
        if r > 0.0 {
            let d = if a > 0.0 { -(r / a).log10() } else { f64::NEG_INFINITY };
            worst_digits.set(worst_digits.get().min(d));
        }
        Ok(Some(n))
    };
    // the multi-indices j (0 <= j_i <= n_i) of the product difference
    let mut indices: Vec<Vec<i64>> = vec![vec![]];
    for &n in orders {
        let mut next = vec![];
        for p in &indices {
            for j in 0..=n {
                let mut q = p.clone();
                q.push(j);
                next.push(q);
            }
        }
        indices = next;
    }
    let difference = |cx: &mut NativeCx<'_, '_, 'h>,
                      eval_at: &mut EvalAt<'_, 'h>,
                      level: i64,
                      central: bool|
     -> Res<Option<T<'h>>> {
        let mut terms = vec![];
        for js in &indices {
            let mut coef = 1i64;
            let mut offs = vec![];
            for (i, &j) in js.iter().enumerate() {
                let n = orders[i];
                let sign = if central { if j % 2 == 0 { 1 } else { -1 } } else if (n - j) % 2 == 0 { 1 } else { -1 };
                coef *= sign * binomial(n, j);
                // central: offset n/2 - j; forward: offset j
                offs.push(if central { ar.div(h.int(n - 2 * j), h.int(2)) } else { h.int(j) });
            }
            let Some(v) = eval_at(cx, &offs, level)? else { return Ok(None) };
            terms.push(ar.mul(&[h.int(coef), ar.number(&v)]));
        }
        // divided by prod h_i^n_i
        let mut den = vec![];
        for (i, &n) in orders.iter().enumerate() {
            if n > 0 {
                den.push(ar.pow(pow2(step_exp[i] - level), h.int(-n)));
            }
        }
        den.push(ar.add(&terms));
        Ok(Some(ar.mul(&den)))
    };
    let unit: f64 = orders.iter().enumerate().map(|(i, &n)| 2f64.powi(-((step_exp[i] + STEP_BITS) * n) as i32)).product();
    let tol = 10f64.powi(-(digits as i32 + 2));
    // the Richardson table, one level more until its last two diagonal entries agree
    let mut table: Vec<Vec<T<'h>>> = vec![];
    let mut found = None;
    for k in 0..levels {
        let Some(d) = difference(cx, &mut eval_at, k as i64, true)? else { return Ok(None) };
        let mut row = vec![d];
        for m in 1..=k {
            let prev = &table[k - 1][m - 1];
            let cur = row[m - 1];
            let corr = ar.div(ar.sub(cur, *prev), h.int(4i64.pow(m as u32) - 1));
            row.push(ar.add(&[cur, corr]));
        }
        table.push(row);
        if k < 2 {
            continue;
        }
        let (last, before) = (table[k][k], table[k - 1][k - 1]);
        let (Some(v), Some(e)) = (ar.num(last), ar.num(ar.sub(last, before))) else { return Ok(None) };
        let size = abs_f64(&v);
        let err = abs_f64(&e) + radius(&v);
        // an absolute floor for machine values that are nearly zero beside the function's size
        let floor = if machine { scale.get() * unit * 1e-2 } else { 0.0 };
        if !(err.is_finite() && size.is_finite()) {
            return Ok(None);
        }
        if err <= tol * size.max(floor) {
            found = Some((last, v, size));
            break;
        }
    }
    let Some((last, v, size)) = found else { return Ok(None) };
    if worst_digits.get() < (digits + 2) as f64 {
        return Ok(None);
    }
    let scale = scale.get();
    // the one-sided check: forward differences at h0 and h0/2 with one Richardson step (error O(h0^2))
    let (Some(f0), Some(f1)) = (difference(cx, &mut eval_at, 0, false)?, difference(cx, &mut eval_at, 1, false)?) else {
        return Ok(None);
    };
    let forward = ar.sub(ar.mul(&[h.int(2), f1]), f0);
    let Some(gap) = ar.num(ar.sub(forward, last)) else { return Ok(None) };
    if abs_f64(&gap) > 1e-3 * size.max(scale * unit) {
        return Ok(None);
    }
    // the value at the target precision: a machine number, or the ball rounded to `digits` digits
    Ok(Some(if machine {
        ar.number(&v.to_kind(Kind::Machine, 53))
    } else {
        cx.eval(h.call(h.sym("n"), &[last, h.int(digits as i64)]))?
    }))
}
