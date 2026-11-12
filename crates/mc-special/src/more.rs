//! More special functions defined by formulas (`mc_numeric::formula`): combinatorial
//! functions (`fibonacci`, `lucas_l`, `pochhammer`, `factorial_power`, `factorial2`, `subfactorial`,
//! `multinomial`, `harmonic_number`, `hyperfactorial`), the classical orthogonal polynomials, spherical harmonics,
//! the Lambert W function `product_log`, `dawson_f`, the Fresnel integrals, spherical Bessel and Hankel functions,
//! the remaining Jacobi elliptic functions, the regularized hypergeometric functions, the elliptic nome and the
//! exponentially scaled Bessel K and I.
//!
//! Conventions (decision P6N.6): a polynomial family at a non-negative integer degree is the expanded polynomial in
//! its argument (exact for exact arguments, evaluated for numbers); at other numeric degrees the hypergeometric
//! representation (DLMF 18.5.7, 18.5.12, 14.3.1, 18.5.13 and 13.2.42 for Hermite) evaluates it; symbolic degrees
//! stay. Machine-only algorithms (Lambert W by Halley's iteration; the scaled Bessel functions) leave balls to the
//! formula forms where one exists.
use crate::machine;
use mc_api::T;
use mc_arith::num::{Int, Nat, Rat};
use mc_arith::{Num, Real};
use mc_eval::Res;
use mc_numeric::formula::{Def, Fx};
use mc_numeric::machine::C64;

type R<'h> = Res<Option<T<'h>>>;

fn qi(n: i64, d: i64) -> Rat {
    Rat::new(Int::from_i64(n), Int::from_i64(d))
}

/// A non-negative integer argument small enough for loops (at most 10^5).
fn small_nat<'h>(fx: &Fx<'_, '_, '_, 'h>, t: T<'h>) -> Option<u64> {
    fx.int_of(t).filter(|v| (0..=100_000).contains(v)).map(|v| v as u64)
}

/// An integer argument (any size) as an `i64` in [-10^5, 10^5].
fn small_int<'h>(fx: &Fx<'_, '_, '_, 'h>, t: T<'h>) -> Option<i64> {
    fx.int_of(t).filter(|v| v.unsigned_abs() <= 100_000)
}

/// Does any argument carry a machine or ball number (the numeric mode of the formulas)?
fn any_inexact<'h>(fx: &Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> bool {
    a.iter().any(|&x| fx.num(x).is_some_and(|n| mc_numeric::formula::inexact(&n)))
}

/// Evaluates the polynomial with rational coefficients `c` (lowest first) at x by Horner's rule.
fn horner<'h>(fx: &Fx<'_, '_, '_, 'h>, c: &[Rat], x: T<'h>) -> T<'h> {
    let mut acc = fx.int(0);
    for q in c.iter().rev() {
        acc = fx.add(&[fx.mul(&[acc, x]), fx.q(q)]);
    }
    acc
}

/// The expanded polynomial sum c_k x^k (for symbolic x).
fn expanded<'h>(fx: &Fx<'_, '_, '_, 'h>, c: &[Rat], x: T<'h>) -> T<'h> {
    let terms: Vec<T<'h>> = c
        .iter()
        .enumerate()
        .filter(|(_, q)| !q.is_zero())
        .map(|(k, q)| fx.mul(&[fx.q(q), fx.pow(x, fx.int(k as i64))]))
        .collect();
    fx.add(&terms)
}

/// The polynomial at x: expanded for a symbolic x, by Horner's rule for a number.
fn poly_at<'h>(fx: &Fx<'_, '_, '_, 'h>, c: &[Rat], x: T<'h>) -> T<'h> {
    if fx.num(x).is_some() {
        horner(fx, c, x)
    } else {
        expanded(fx, c, x)
    }
}

/// p*a + q*b for coefficient vectors, with x*a shifted: the step of a three-term recurrence
/// out = (s1 + s2 x) cur + s3 prev, all scaled by 1/d.
fn step(cur: &[Rat], prev: &[Rat], s1: &Rat, s2: &Rat, s3: &Rat, d: &Rat) -> Vec<Rat> {
    let n = cur.len() + 1;
    let mut out = vec![Rat::from_i64(0); n];
    for (k, c) in cur.iter().enumerate() {
        out[k] = out[k].add(&c.mul(s1));
        out[k + 1] = out[k + 1].add(&c.mul(s2));
    }
    for (k, c) in prev.iter().enumerate() {
        out[k] = out[k].add(&c.mul(s3));
    }
    let inv = d.recip().unwrap_or_else(|| Rat::from_i64(1));
    out.iter().map(|c| c.mul(&inv)).collect()
}

#[derive(Clone, Copy)]
enum Family {
    ChebyshevT,
    ChebyshevU,
    Legendre,
    Hermite,
    Laguerre,
}

/// The coefficients of the degree-n polynomial of a family without parameters.
fn coefficients(f: Family, n: u64) -> Vec<Rat> {
    let r = Rat::from_i64;
    let z = r(0);
    let (p0, p1): (Vec<Rat>, Vec<Rat>) = match f {
        Family::ChebyshevT | Family::Legendre => (vec![r(1)], vec![z.clone(), r(1)]),
        Family::ChebyshevU | Family::Hermite => (vec![r(1)], vec![z.clone(), r(2)]),
        Family::Laguerre => (vec![r(1)], vec![r(1), r(-1)]),
    };
    if n == 0 {
        return p0;
    }
    let (mut prev, mut cur) = (p0, p1);
    for k in 1..n {
        let k = k as i64;
        let next = match f {
            Family::ChebyshevT | Family::ChebyshevU => step(&cur, &prev, &z, &r(2), &r(-1), &r(1)),
            Family::Legendre => step(&cur, &prev, &z, &r(2 * k + 1), &r(-k), &r(k + 1)),
            Family::Hermite => step(&cur, &prev, &z, &r(2), &r(-2 * k), &r(1)),
            Family::Laguerre => step(&cur, &prev, &r(2 * k + 1), &r(-1), &r(-k), &r(k + 1)),
        };
        prev = cur;
        cur = next;
    }
    cur
}

fn derivative(c: &[Rat], m: u64) -> Vec<Rat> {
    let mut c = c.to_vec();
    for _ in 0..m {
        if c.len() <= 1 {
            return vec![Rat::from_i64(0)];
        }
        c = c.iter().enumerate().skip(1).map(|(k, q)| q.mul(&Rat::from_i64(k as i64))).collect();
    }
    c
}

// ---------------------------------------------------------------- machine recurrences (round 24, X23-1..3)
//
// The explicit polynomial (Horner on exact coefficients) in double precision loses every digit from degree about 25
// (the coefficients are huge and alternate). At a machine argument the classical three-term recurrences are used
// instead (DLMF 18.9.1 for the Jacobi polynomials and their special cases, 14.10.3 for the associated Legendre
// functions, 18.9.13 for Laguerre, 18.9.3 for Hermite): forward recurrence is stable for the oscillatory region
// and has a relative error growing like n u elsewhere (Gautschi, Orthogonal Polynomials, 2004, section 1.3).

/// Degrees up to this stay on the exact-coefficient Horner form (its values are bit-for-bit those of earlier releases).
const RECURRENCE_FROM: u64 = 9;

fn csc(c: f64, z: C64) -> C64 {
    C64::new(c * z.re, c * z.im)
}

/// The argument as a machine number: (value, is real), for a machine real or a complex with a machine part.
fn machine_x<'h>(fx: &Fx<'_, '_, '_, 'h>, x: T<'h>) -> Option<(C64, bool)> {
    match fx.num(x)? {
        Num::Real(Real::Machine(v)) => Some((C64::real(v), true)),
        Num::Complex(re, im) if matches!(re, Real::Machine(_)) || matches!(im, Real::Machine(_)) => {
            Some((C64::new(re.to_f64(), im.to_f64()), false))
        }
        _ => None,
    }
}

/// A finite machine value as a term (`None` on overflow or NaN: the caller falls back).
fn machine_term<'h>(fx: &Fx<'_, '_, '_, 'h>, v: C64, real: bool) -> Option<T<'h>> {
    if !v.re.is_finite() || !v.im.is_finite() {
        return None;
    }
    Some(if real { fx.real(v.re) } else { fx.ar.complex(fx.real(v.re), fx.real(v.im)) })
}

/// The degree-n polynomial of a parameterless family at a machine x by the three-term recurrence.
fn machine_family<'h>(fx: &mut Fx<'_, '_, '_, 'h>, f: Family, n: u64, x: T<'h>) -> Res<Option<T<'h>>> {
    if n < RECURRENCE_FROM {
        return Ok(None);
    }
    let Some((xv, real)) = machine_x(fx, x) else { return Ok(None) };
    let one = C64::real(1.0);
    let (mut prev, mut cur) = match f {
        Family::ChebyshevT | Family::Legendre => (one, xv),
        Family::ChebyshevU | Family::Hermite => (one, csc(2.0, xv)),
        Family::Laguerre => (one, one - xv),
    };
    let mut rescaled = 0i32;
    for k in 1..n {
        if k % 4096 == 0 {
            fx.tick(1)?;
        }
        let kf = k as f64;
        let next = match f {
            Family::ChebyshevT | Family::ChebyshevU => csc(2.0, xv * cur) - prev,
            Family::Legendre => (csc(2.0 * kf + 1.0, xv * cur) - csc(kf, prev)) * C64::real(1.0 / (kf + 1.0)),
            Family::Hermite => csc(2.0, xv * cur) - csc(2.0 * kf, prev),
            Family::Laguerre => (csc(2.0 * kf + 1.0, cur) - xv * cur - csc(kf, prev)) * C64::real(1.0 / (kf + 1.0)),
        };
        prev = cur;
        cur = next;
        // keep the pair in range: a result beyond the machine range is an overflow (a real value of the right sign)
        if cur.re.abs().max(cur.im.abs()) > 1e250 {
            cur = csc(1e-250, cur);
            prev = csc(1e-250, prev);
            rescaled += 1;
        }
    }
    if rescaled > 0 {
        // the true value is cur 10^(250 rescaled): in range only when cur was brought back below 1e58
        let lg = cur.re.abs().max(cur.im.abs()).log10() + 250.0 * rescaled as f64;
        if lg < 307.0 {
            let f = 10f64.powi(250 * rescaled);
            return Ok(machine_term(fx, csc(f, cur), real));
        }
        return Ok(if real { Some(fx.real(f64::INFINITY.copysign(cur.re))) } else { None });
    }
    Ok(machine_term(fx, cur, real))
}

/// d^m/dx^m P_l(x) at a machine x for 0 <= m <= l by the recurrence of the associated functions with the factor
/// (1 - x^2)^(m/2) divided out: R_m = (2m - 1)!!, R_(m+1) = (2m + 1) x R_m, (j - m) R_j = (2j - 1) x R_(j-1) -
/// (j + m - 1) R_(j-2) (DLMF 14.10.3 and 14.7.15).
fn legendre_derivative_machine<'h>(fx: &mut Fx<'_, '_, '_, 'h>, l: u64, m: u64, x: T<'h>) -> Res<Option<T<'h>>> {
    if l < RECURRENCE_FROM || m > 100 {
        return Ok(None);
    }
    let Some((xv, real)) = machine_x(fx, x) else { return Ok(None) };
    let mut rm = 1.0f64;
    for j in 1..=m {
        rm *= (2 * j - 1) as f64;
    }
    let mut prev = C64::real(rm);
    if l == m {
        return Ok(machine_term(fx, prev, real));
    }
    let mut cur = csc((2 * m + 1) as f64, xv * prev);
    for j in (m + 2)..=l {
        if j % 4096 == 0 {
            fx.tick(1)?;
        }
        let jf = j as f64;
        let mf = m as f64;
        let next = (csc(2.0 * jf - 1.0, xv * cur) - csc(jf + mf - 1.0, prev)) * C64::real(1.0 / (jf - mf));
        prev = cur;
        cur = next;
    }
    Ok(machine_term(fx, cur, real))
}

/// A three-term recurrence run at machine precision with the pair rescaled when it nears the range limit (round 27,
/// B26-14, B26-15, X26-7): `step(k, prev, cur)` is the value of degree k + 1, or `None` when a recurrence
/// coefficient is not finite. A result beyond the machine range is a signed infinity for a real argument.
fn machine_recurrence<'h>(
    fx: &mut Fx<'_, '_, '_, 'h>,
    n: u64,
    real: bool,
    start: (C64, C64),
    step: impl Fn(f64, C64, C64) -> Option<C64>,
) -> Res<Option<T<'h>>> {
    let (mut prev, mut cur) = start;
    let mut rescaled = 0i32;
    for k in 1..n {
        if k % 4096 == 0 {
            fx.tick(1)?;
        }
        let Some(next) = step(k as f64, prev, cur) else { return Ok(None) };
        prev = cur;
        cur = next;
        if cur.re.abs().max(cur.im.abs()) > 1e250 {
            cur = csc(1e-250, cur);
            prev = csc(1e-250, prev);
            rescaled += 1;
        }
    }
    if rescaled > 0 {
        let lg = cur.re.abs().max(cur.im.abs()).log10() + 250.0 * rescaled as f64;
        if lg < 307.0 {
            let f = 10f64.powi(250 * rescaled);
            return Ok(machine_term(fx, csc(f, cur), real));
        }
        return Ok(if real && cur.re != 0.0 { Some(fx.real(f64::INFINITY.copysign(cur.re))) } else { None });
    }
    Ok(machine_term(fx, cur, real))
}

/// The degree of a polynomial argument: an exact integer, or an integer-valued machine number (as `n(...)` makes of
/// an exact degree), at most 10^5.
fn degree_of<'h>(fx: &Fx<'_, '_, '_, 'h>, n: T<'h>) -> Option<u64> {
    small_nat(fx, n).or_else(|| fx.machine_of(n).filter(|v| v.fract() == 0.0 && (0.0..=100_000.0).contains(v)).map(|v| v as u64))
}

/// P_n^(a, b)(x) at a machine x, a and b exact or machine reals, by the recurrence of DLMF 18.9.2. `None` when a
/// recurrence coefficient vanishes (the caller falls back).
fn jacobi_machine<'h>(fx: &mut Fx<'_, '_, '_, 'h>, n: u64, al: T<'h>, be: T<'h>, x: T<'h>) -> Res<Option<T<'h>>> {
    if n < RECURRENCE_FROM {
        return Ok(None);
    }
    let Some((xv, real)) = machine_x(fx, x) else { return Ok(None) };
    let (Some(a), Some(b)) = (fx.f64_of(al), fx.f64_of(be)) else { return Ok(None) };
    let one = C64::real(1.0);
    let start = (one, C64::real(a + 1.0) + csc((a + b + 2.0) * 0.5, xv - one));
    machine_recurrence(fx, n, real, start, |kf, prev, cur| {
        let s = 2.0 * kf + a + b;
        let denom = 2.0 * (kf + 1.0) * (kf + a + b + 1.0) * s;
        if denom == 0.0 || !denom.is_finite() {
            return None;
        }
        let c1 = (s + 1.0) * ((s + 2.0) * s);
        let c0 = (s + 1.0) * (a * a - b * b);
        let c2 = 2.0 * (kf + a) * (kf + b) * (s + 2.0);
        Some((csc(c1, xv * cur) + csc(c0, cur) - csc(c2, prev)) * C64::real(1.0 / denom))
    })
}

/// L_n^(a)(x) at a machine x by (k + 1) L_(k+1) = (2k + 1 + a - x) L_k - (k + a) L_(k-1) (DLMF 18.9.13).
fn laguerre_machine<'h>(fx: &mut Fx<'_, '_, '_, 'h>, n: u64, al: T<'h>, x: T<'h>) -> Res<Option<T<'h>>> {
    if n < RECURRENCE_FROM {
        return Ok(None);
    }
    let Some((xv, real)) = machine_x(fx, x) else { return Ok(None) };
    let Some(a) = fx.f64_of(al) else { return Ok(None) };
    let start = (C64::real(1.0), C64::real(1.0 + a) - xv);
    machine_recurrence(fx, n, real, start, |kf, prev, cur| {
        Some((((C64::real(2.0 * kf + 1.0 + a) - xv) * cur) - csc(kf + a, prev)) * C64::real(1.0 / (kf + 1.0)))
    })
}

/// C_n^(l)(x) at a machine x by (k + 1) C_(k+1) = 2 (k + l) x C_k - (k + 2 l - 1) C_(k-1) (DLMF 18.9.7); stable where the
/// gamma-ratio form of the hypergeometric function overflows (degree 250 and up).
fn gegenbauer_machine<'h>(fx: &mut Fx<'_, '_, '_, 'h>, n: u64, la: T<'h>, x: T<'h>) -> Res<Option<T<'h>>> {
    if n < RECURRENCE_FROM {
        return Ok(None);
    }
    let Some((xv, real)) = machine_x(fx, x) else { return Ok(None) };
    let Some(l) = fx.f64_of(la) else { return Ok(None) };
    if l == 0.0 {
        return Ok(None);
    }
    let start = (C64::real(1.0), csc(2.0 * l, xv));
    machine_recurrence(fx, n, real, start, |kf, prev, cur| {
        Some((csc(2.0 * (kf + l), xv * cur) - csc(kf + 2.0 * l - 1.0, prev)) * C64::real(1.0 / (kf + 1.0)))
    })
}

/// Q_n(x) for a machine real |x| < 1 by the forward recurrence (Q_0 = atanh x, Q_1 = x Q_0 - 1; P_n and Q_n are
/// both oscillatory there, so it is stable).
fn legendre_q_machine<'h>(fx: &mut Fx<'_, '_, '_, 'h>, n: u64, x: T<'h>) -> Res<Option<T<'h>>> {
    let Some(Num::Real(Real::Machine(xv))) = fx.num(x) else { return Ok(None) };
    if n < RECURRENCE_FROM || xv.abs() >= 1.0 {
        return Ok(None);
    }
    let mut prev = 0.5 * (xv.ln_1p() - (-xv).ln_1p());
    let mut cur = xv * prev - 1.0;
    for k in 1..n {
        if k % 4096 == 0 {
            fx.tick(1)?;
        }
        let kf = k as f64;
        let next = ((2.0 * kf + 1.0) * xv * cur - kf * prev) / (kf + 1.0);
        prev = cur;
        cur = next;
    }
    Ok(machine_term(fx, C64::real(cur), true))
}

/// Does a numeric degree or argument exceed what any method here can evaluate (the hypergeometric forms hang)?
fn absurd<'h>(fx: &Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> bool {
    a.iter().any(|&t| fx.f64_of(t).is_some_and(|v| v.abs() > 1e9))
}

/// The polynomial families of one argument at an integer degree, or their hypergeometric forms at numeric degrees.
fn family<'h>(fx: &mut Fx<'_, '_, '_, 'h>, f: Family, a: &[T<'h>]) -> R<'h> {
    let (n, x) = (a[0], a[1]);
    if let Some(k) = small_nat(fx, n) {
        if let Some(v) = machine_family(fx, f, k, x)? {
            return Ok(Some(v));
        }
        return Ok(Some(poly_at(fx, &coefficients(f, k), x)));
    }
    // negative integer degrees: T_{-n} = T_n, U_{-n} = -U_{n-2}, P_{-n} = P_{n-1}
    if let Some(k) = small_int(fx, n).filter(|k| *k < 0) {
        let m = (-k) as u64;
        return Ok(match f {
            Family::ChebyshevT => Some(poly_at(fx, &coefficients(f, m), x)),
            Family::ChebyshevU if m == 1 => Some(fx.int(0)),
            Family::ChebyshevU => Some(fx.neg(poly_at(fx, &coefficients(f, m - 2), x))),
            Family::Legendre => Some(poly_at(fx, &coefficients(f, m - 1), x)),
            _ => None,
        });
    }
    if !(fx.num(n).is_some() && fx.num(x).is_some() && any_inexact(fx, a)) {
        return Ok(None);
    }
    // a machine degree with an integer value: the polynomial at x as a machine number (the hypergeometric forms
    // below divide by gamma at its poles there)
    if absurd(fx, &[n]) {
        return Ok(None);
    }
    if let Some(k) = fx.machine_of(n).filter(|v| v.fract() == 0.0 && (0.0..=10_000.0).contains(v)) {
        let xm = fx.eval(fx.call("n", &[x]))?;
        if fx.num(xm).is_some() {
            if let Some(v) = machine_family(fx, f, k as u64, xm)? {
                return Ok(Some(v));
            }
            return Ok(Some(poly_at(fx, &coefficients(f, k as u64), xm)));
        }
    }
    let half_1mx = fx.mul(&[fx.rat(1, 2), fx.sub(fx.int(1), x)]);
    Ok(Some(match f {
        // a real x > 1: cosh(n acosh x) and sinh((n + 1) acosh x)/sqrt(x^2 - 1) are real (cos(n acos x) is a complex ball
        // with an imaginary part 0 +- 2^-p, B26-26)
        Family::ChebyshevT if fx.f64_of(x).is_some_and(|v| v > 1.0) => fx.call("cosh", &[fx.mul(&[n, fx.call("acosh", &[x])])]),
        Family::ChebyshevU if fx.f64_of(x).is_some_and(|v| v > 1.0) => {
            let t = fx.call("acosh", &[x]);
            fx.div(fx.call("sinh", &[fx.mul(&[fx.add(&[n, fx.int(1)]), t])]), fx.call("sinh", &[t]))
        }
        Family::ChebyshevT => fx.call("cos", &[fx.mul(&[n, fx.call("acos", &[x])])]),
        Family::ChebyshevU => {
            let t = fx.call("acos", &[x]);
            fx.div(fx.call("sin", &[fx.mul(&[fx.add(&[n, fx.int(1)]), t])]), fx.call("sin", &[t]))
        }
        Family::Legendre => fx.call("hypergeometric_2f1", &[fx.neg(n), fx.add(&[n, fx.int(1)]), fx.int(1), half_1mx]),
        Family::Laguerre => fx.call("hypergeometric_1f1", &[fx.neg(n), fx.int(1), x]),
        Family::Hermite => {
            // x > 0 beyond the reach of the two 1F1 (they overflow: `hermite_h(2.5, 100.)` was `indeterminate`):
            // H_n(x) = 2^n U(-n/2, 1/2, x^2) (DLMF 18.5.? with 13.2), U by its asymptotic series; the old forms follow
            if fx.f64_of(x).is_some_and(|v| v > 27.2) && fx.f64_of(n).is_some() {
                let x2 = fx.eval(fx.pow(x, fx.int(2)))?;
                let u = fx.eval(fx.call("hypergeometric_u", &[fx.mul(&[fx.rat(-1, 2), n]), fx.rat(1, 2), x2]))?;
                if fx.num(u).is_some() {
                    return Ok(Some(fx.mul(&[fx.pow(fx.int(2), n), u])));
                }
            }
            if let Some(v) = hermite_high_precision(fx, n, x)? {
                return Ok(Some(v));
            }
            // DLMF 18.5.13 continued to any degree: 2^n sqrt(pi) (M(-n/2, 1/2, x^2)/gamma((1-n)/2) - 2 x
            // M((1-n)/2, 3/2, x^2)/gamma(-n/2))
            let x2 = fx.pow(x, fx.int(2));
            let h = fx.mul(&[fx.rat(-1, 2), n]);
            let h1 = fx.mul(&[fx.rat(1, 2), fx.sub(fx.int(1), n)]);
            let m1 = fx.call("hypergeometric_1f1", &[h, fx.rat(1, 2), x2]);
            let m2 = fx.call("hypergeometric_1f1", &[h1, fx.rat(3, 2), x2]);
            let t1 = fx.div(m1, fx.call("gamma", &[h1]));
            let t2 = fx.mul(&[fx.int(-2), x, fx.div(m2, fx.call("gamma", &[h]))]);
            fx.mul(&[fx.pow(fx.int(2), n), fx.sqrt(fx.sym("pi")), fx.add(&[t1, t2])])
        }
    }))
}

/// The exact rational of a real number of any kind (a machine number or a ball's midpoint exactly).
fn exact_of_real<'h>(fx: &Fx<'_, '_, '_, 'h>, r: &Real) -> Option<T<'h>> {
    let (neg, man, exp) = match r {
        Real::Exact(q) => return Some(fx.q(q)),
        Real::Machine(v) => mc_arith::num::float::f64_to_dyadic(*v)?,
        Real::Ball(b, _) => (b.mid.neg, b.mid.man.clone(), b.mid.exp),
    };
    let q = if exp >= 0 {
        Rat::new(Int::new(neg, man.shl(exp as u64)), Int::from_i64(1))
    } else {
        Rat::new(Int::new(neg, man), Int::from_nat(Nat::from_u64(1).shl((-exp) as u64)))
    };
    Some(fx.q(&q))
}

/// A number (real or complex, machine or ball) as an exact term, with its magnitude and whether it is a machine
/// number and the largest ball precision.
fn exact_of<'h>(fx: &Fx<'_, '_, '_, 'h>, t: T<'h>) -> Option<(T<'h>, f64, bool, u64)> {
    let n = fx.num(t)?;
    let (re, im) = match &n {
        Num::Real(r) => (r.clone(), None),
        Num::Complex(a, b) => (a.clone(), Some(b.clone())),
    };
    let prec = |r: &Real| match r {
        Real::Ball(_, p) => *p,
        _ => 0,
    };
    let machine = matches!(re, Real::Machine(_)) || matches!(&im, Some(Real::Machine(_)));
    let mag = re.to_f64().hypot(im.as_ref().map_or(0.0, Real::to_f64));
    let p = prec(&re).max(im.as_ref().map_or(0, prec));
    let e = exact_of_real(fx, &re)?;
    let e = match im {
        Some(b) => fx.add(&[e, fx.mul(&[exact_of_real(fx, &b)?, fx.i()])]),
        None => e,
    };
    Some((e, mag, machine, p))
}

/// H_n(x) for a non-integer degree and an inexact argument: the hypergeometric form on the exact value of the
/// argument at the precision the cancellation between its two terms needs (about 1.45 |x|^2 bits), rounded to the
/// kind of the argument (a machine number, or the ball precision it came with). The radius of an input ball is not
/// propagated (its midpoint is used).
fn hermite_high_precision<'h>(fx: &mut Fx<'_, '_, '_, 'h>, n: T<'h>, x: T<'h>) -> R<'h> {
    let Some((xe, mag, machine, p)) = exact_of(fx, x) else { return Ok(None) };
    let Some((ne, nmag, nmachine, np)) = exact_of(fx, n) else { return Ok(None) };
    // a machine result beyond the range (about e^(x^2) for a non-integer degree) is not computed: 1F1 at 14000 bits took minutes
    if !(machine || nmachine || p > 0 || np > 0) || !mag.is_finite() || mag * mag > 20_000.0 || nmag > 1e6 || ((machine || nmachine) && mag * mag > 740.0) {
        return Ok(None);
    }
    let digits_wanted = if machine || nmachine { 17 } else { ((p.max(np) as f64) * std::f64::consts::LOG10_2) as i64 + 2 };
    let boost = (mag * mag * 0.44) as i64 + (nmag * 0.5) as i64 + 12;
    // the exact square as a number (a power node of a complex exact base is slow at high precision)
    let x2 = fx.eval(fx.pow(xe, fx.int(2)))?;
    let h = fx.mul(&[fx.rat(-1, 2), ne]);
    let h1 = fx.mul(&[fx.rat(1, 2), fx.sub(fx.int(1), ne)]);
    let m1 = fx.call("hypergeometric_1f1", &[h, fx.rat(1, 2), x2]);
    let m2 = fx.call("hypergeometric_1f1", &[h1, fx.rat(3, 2), x2]);
    let t1 = fx.div(m1, fx.call("gamma", &[h1]));
    let t2 = fx.mul(&[fx.int(-2), xe, fx.div(m2, fx.call("gamma", &[h]))]);
    let e = fx.mul(&[fx.pow(fx.int(2), ne), fx.sqrt(fx.sym("pi")), fx.add(&[t1, t2])]);
    let v = fx.eval(fx.call("n", &[e, fx.int(digits_wanted + boost)]))?;
    if fx.num(v).is_none() {
        return Ok(None);
    }
    if machine || nmachine {
        return Ok(Some(fx.eval(fx.call("n", &[v]))?));
    }
    Ok(Some(fx.eval(fx.call("n", &[v, fx.int(digits_wanted)]))?))
}

macro_rules! fam {
    ($name:ident, $f:expr) => {
        fn $name<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
            family(fx, $f, a)
        }
    };
}
fam!(chebyshev_t, Family::ChebyshevT);
fam!(chebyshev_u, Family::ChebyshevU);
fam!(hermite_h, Family::Hermite);

fn legendre_p<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    if a.len() == 2 {
        return family(fx, Family::Legendre, a);
    }
    // the associated function P_l^m(x) = (-1)^m (1 - x^2)^(m/2) d^m/dx^m P_l(x) (type 2, |x| < 1)
    if a.len() == 4 {
        // legendre_p(n, m, type, z): type 2 is the three-argument function, type 3 the function on the cut plane
        if fx.is_int(a[2], 2) {
            return Ok(Some(fx.call("legendre_p", &[a[0], a[1], a[3]])));
        }
        if !fx.is_int(a[2], 3) || !any_inexact(fx, a) || a.iter().any(|&t| fx.num(t).is_none()) {
            return Ok(None);
        }
        // ((z + 1)/(z - 1))^(m/2) 2F1~(-n, n + 1; 1 - m; (1 - z)/2)
        let (n, m, z) = (a[0], a[1], a[3]);
        let w = fx.pow(fx.div(fx.add(&[z, fx.int(1)]), fx.sub(z, fx.int(1))), fx.mul(&[m, fx.rat(1, 2)]));
        let f = fx.call(
            "hypergeometric_2f1_regularized",
            &[fx.neg(n), fx.add(&[n, fx.int(1)]), fx.sub(fx.int(1), m), fx.mul(&[fx.rat(1, 2), fx.sub(fx.int(1), z)])],
        );
        return Ok(Some(fx.mul(&[w, f])));
    }
    let (l, m, x) = (a[0], a[1], a[2]);
    let (Some(l), Some(m)) = (small_nat(fx, l), small_int(fx, m)) else {
        if !any_inexact(fx, a) || a.iter().any(|&t| fx.num(t).is_none()) || absurd(fx, a) {
            return Ok(None);
        }
        // any degree and order:((1 + x)/(1 - x))^(m/2) 2F1~(-l, l + 1; 1 - m; (1 - x)/2) (type 2)
        let w = fx.pow(fx.div(fx.add(&[fx.int(1), x]), fx.sub(fx.int(1), x)), fx.mul(&[m, fx.rat(1, 2)]));
        let f = fx.call(
            "hypergeometric_2f1_regularized",
            &[fx.neg(l), fx.add(&[l, fx.int(1)]), fx.sub(fx.int(1), m), fx.mul(&[fx.rat(1, 2), fx.sub(fx.int(1), x)])],
        );
        return Ok(Some(fx.mul(&[w, f])));
    };
    let am = m.unsigned_abs();
    if am > l {
        return Ok(Some(fx.int(0)));
    }
    let w = fx.pow(fx.sub(fx.int(1), fx.pow(x, fx.int(2))), fx.rat(am as i64, 2));
    let sign = if am % 2 == 1 { -1 } else { 1 };
    let dp = match legendre_derivative_machine(fx, l, am, x)? {
        Some(r) => r,
        None => poly_at(fx, &derivative(&coefficients(Family::Legendre, l), am), x),
    };
    let mut v = fx.mul(&[fx.int(sign), w, dp]);
    if m < 0 {
        // P_l^-m = (-1)^m (l - m)!/(l + m)! P_l^m
        let ratio = falling_ratio(l, am);
        v = fx.mul(&[fx.int(sign), fx.q(&ratio), v]);
    }
    Ok(Some(v))
}

/// (l - m)!/(l + m)!
fn falling_ratio(l: u64, m: u64) -> Rat {
    let mut den = Nat::from_u64(1);
    for k in (l - m + 1)..=(l + m) {
        den = den.mul_u64(k);
    }
    Rat::new(Int::from_i64(1), Int::from_nat(den))
}

fn legendre_q<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    let (n, x) = (a[0], a[1]);
    if let Some(k) = small_nat(fx, n) {
        if let Some(v) = legendre_q_machine(fx, k, x)? {
            return Ok(Some(v));
        }
    }
    let Some(n) = small_nat(fx, n).filter(|n| *n <= 200) else { return Ok(None) };
    // Q_n = P_n (log(1 + x) - log(1 - x))/2 - sum_{k=1}^n P_{k-1} P_{n-k} / k
    let p = |k: u64| coefficients(Family::Legendre, k);
    let pn = poly_at(fx, &p(n), x);
    let l = fx.mul(&[
        fx.rat(1, 2),
        fx.sub(fx.call("log", &[fx.add(&[fx.int(1), x])]), fx.call("log", &[fx.sub(fx.int(1), x)])),
    ]);
    let mut w = vec![Rat::from_i64(0); n as usize];
    for k in 1..=n {
        let (a1, b1) = (p(k - 1), p(n - k));
        for (i, ca) in a1.iter().enumerate() {
            for (j, cb) in b1.iter().enumerate() {
                w[i + j] = w[i + j].add(&ca.mul(cb).mul(&qi(1, k as i64)));
            }
        }
    }
    Ok(Some(fx.sub(fx.mul(&[pn, l]), poly_at(fx, &w, x))))
}

/// Term recurrences for the families with parameters, expanded at the end.
fn expand<'h>(fx: &mut Fx<'_, '_, '_, 'h>, t: T<'h>) -> Res<T<'h>> {
    fx.eval(fx.call("expand", &[t]))
}

/// Largest degree of the exact rational recurrences (the values grow by about log10(denominator) digits a step).
const EXACT_RECURRENCE_MAX: u64 = 3000;

/// The exact rational value of the degree-n Laguerre (`which` 0, `p = [a, x]`), Gegenbauer (1, `[l, x]`) or Jacobi (2,
/// `[a, b, x]`) polynomial for rational parameters, by the three-term recurrence in rational arithmetic (DLMF
/// 18.9.13, 18.9.7, 18.9.2): no cancellation, so `n` of the result is correctly rounded at any degree (round 27,
/// B26-14 to B26-16, X26-7). `None` when a recurrence coefficient is zero (the generic formulas do not apply).
fn exact_recurrence<'h>(fx: &mut Fx<'_, '_, '_, 'h>, which: u8, n: u64, p: &[Rat]) -> Res<Option<Rat>> {
    let r = Rat::from_i64;
    let ratio = |a: &Rat, b: &Rat| -> Option<Rat> { b.recip().map(|i| a.mul(&i)) };
    let (mut prev, mut cur) = match which {
        0 => (r(1), r(1).add(&p[0]).sub(&p[1])),
        1 => (r(1), r(2).mul(&p[0]).mul(&p[1])),
        _ => {
            let (a, b, x) = (&p[0], &p[1], &p[2]);
            let half = Rat::new(Int::from_i64(1), Int::from_i64(2));
            (r(1), a.add(&r(1)).add(&a.add(b).add(&r(2)).mul(&x.sub(&r(1))).mul(&half)))
        }
    };
    if n == 0 {
        return Ok(Some(prev));
    }
    for k in 1..n {
        if k % 32 == 0 {
            fx.tick(1)?;
        }
        let kk = r(k as i64);
        let next = match which {
            0 => {
                // ((2k + 1 + a - x) L_k - (k + a) L_(k-1))/(k + 1)
                let c = r(2 * k as i64 + 1).add(&p[0]).sub(&p[1]);
                c.mul(&cur).sub(&kk.add(&p[0]).mul(&prev)).mul(&qi(1, k as i64 + 1))
            }
            1 => {
                // (2 (k + l) x C_k - (k + 2 l - 1) C_(k-1))/(k + 1)
                let c = r(2).mul(&kk.add(&p[0])).mul(&p[1]);
                let d = kk.add(&r(2).mul(&p[0])).sub(&r(1));
                c.mul(&cur).sub(&d.mul(&prev)).mul(&qi(1, k as i64 + 1))
            }
            _ => {
                let (a, b, x) = (&p[0], &p[1], &p[2]);
                let s = r(2).mul(&kk).add(a).add(b);
                let denom = r(2).mul(&kk.add(&r(1))).mul(&kk.add(a).add(b).add(&r(1))).mul(&s);
                if denom.is_zero() {
                    return Ok(None);
                }
                let s1 = s.add(&r(1));
                let s2 = s.add(&r(2));
                let c1 = s1.mul(&s2).mul(&s);
                let c0 = s1.mul(&a.mul(a).sub(&b.mul(b)));
                let c2 = r(2).mul(&kk.add(a)).mul(&kk.add(b)).mul(&s2);
                let Some(v) = ratio(&c1.mul(x).add(&c0).mul(&cur).sub(&c2.mul(&prev)), &denom) else { return Ok(None) };
                v
            }
        };
        prev = cur;
        cur = next;
    }
    Ok(Some(cur))
}

/// The rational values of exact numeric arguments.
fn rats_of<'h>(fx: &Fx<'_, '_, '_, 'h>, ts: &[T<'h>]) -> Option<Vec<Rat>> {
    ts.iter().map(|&t| fx.rat_of(t)).collect()
}

fn laguerre_l<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    if a.len() == 2 {
        return family(fx, Family::Laguerre, a);
    }
    let (n, al, x) = (a[0], a[1], a[2]);
    if let (Some(k), Some(ps)) = (small_nat(fx, n).filter(|k| *k <= EXACT_RECURRENCE_MAX), rats_of(fx, &[al, x])) {
        if let Some(v) = exact_recurrence(fx, 0, k, &ps)? {
            return Ok(Some(fx.q(&v)));
        }
    }
    if let Some(k) = degree_of(fx, n).filter(|_| fx.num(al).is_some() && any_inexact(fx, a)) {
        if let Some(v) = laguerre_machine(fx, k, al, x)? {
            return Ok(Some(v));
        }
    }
    if let Some(k) = small_nat(fx, n).filter(|k| *k <= 200) {
        // (k + 1) L_{k+1} = (2k + 1 + a - x) L_k - (k + a) L_{k-1}
        let mut prev = fx.int(1);
        if k == 0 {
            return Ok(Some(prev));
        }
        let mut cur = fx.sub(fx.add(&[fx.int(1), al]), x);
        for j in 1..k as i64 {
            let next = fx.mul(&[
                fx.rat(1, j + 1),
                fx.sub(fx.mul(&[fx.sub(fx.add(&[fx.int(2 * j + 1), al]), x), cur]), fx.mul(&[fx.add(&[fx.int(j), al]), prev])),
            ]);
            prev = cur;
            cur = expand(fx, next)?;
        }
        return Ok(Some(if fx.num(x).is_some() && fx.num(al).is_some() { cur } else { expand(fx, cur)? }));
    }
    if !(a.iter().all(|&t| fx.num(t).is_some()) && any_inexact(fx, a)) {
        return Ok(None);
    }
    // binomial(n + a, n) 1F1(-n; a + 1; x)
    let b = fx.call("binomial", &[fx.add(&[n, al]), n]);
    Ok(Some(fx.mul(&[b, fx.call("hypergeometric_1f1", &[fx.neg(n), fx.add(&[al, fx.int(1)]), x])])))
}

fn gegenbauer_c<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    if a.len() == 2 {
        // C_n(x) = (2/n) T_n(x) for n >= 1
        let (n, x) = (a[0], a[1]);
        let Some(k) = small_nat(fx, n).filter(|k| *k >= 1) else { return Ok(None) };
        return Ok(Some(fx.mul(&[fx.rat(2, k as i64), poly_at(fx, &coefficients(Family::ChebyshevT, k), x)])));
    }
    let (n, al, x) = (a[0], a[1], a[2]);
    if let (Some(k), Some(ps)) = (small_nat(fx, n).filter(|k| *k <= EXACT_RECURRENCE_MAX), rats_of(fx, &[al, x])) {
        if !ps[0].is_zero() {
            if let Some(v) = exact_recurrence(fx, 1, k, &ps)? {
                return Ok(Some(fx.q(&v)));
            }
        }
    }
    if let Some(k) = degree_of(fx, n).filter(|_| fx.num(al).is_some() && any_inexact(fx, a)) {
        if let Some(v) = gegenbauer_machine(fx, k, al, x)? {
            return Ok(Some(v));
        }
    }
    if let Some(k) = small_nat(fx, n).filter(|k| *k <= 200) {
        // (j + 1) C_{j+1} = 2 (j + a) x C_j - (j + 2a - 1) C_{j-1}
        let mut prev = fx.int(1);
        if k == 0 {
            return Ok(Some(prev));
        }
        let mut cur = fx.mul(&[fx.int(2), al, x]);
        for j in 1..k as i64 {
            let next = fx.mul(&[
                fx.rat(1, j + 1),
                fx.sub(
                    fx.mul(&[fx.int(2), fx.add(&[fx.int(j), al]), x, cur]),
                    fx.mul(&[fx.add(&[fx.int(j - 1), fx.mul(&[fx.int(2), al])]), prev]),
                ),
            ]);
            prev = cur;
            cur = expand(fx, next)?;
        }
        return Ok(Some(cur));
    }
    if !(a.iter().all(|&t| fx.num(t).is_some()) && any_inexact(fx, a)) {
        return Ok(None);
    }
    // gamma(n + 2a)/(gamma(n + 1) gamma(2a)) 2F1(-n, n + 2a; a + 1/2; (1 - x)/2)
    let two_a = fx.mul(&[fx.int(2), al]);
    // a = -1/2 - m: gamma(2a) and the 2F1 lower parameter a + 1/2 = -m are both poles; the product is finite (round 27,
    // B26-19): gamma(a + 1/2)/gamma(2a) -> -2 (-1)^m (2m + 1)!/m! and 2F1/gamma(c) is the regularized function
    if let Some(m) = fx.rat_of(al).and_then(|q| {
        let m2 = q.neg().sub(&qi(1, 2));
        (m2.is_int() && !m2.num.neg).then(|| m2.num.mag.to_u128().unwrap_or(u128::MAX))
    }).filter(|m| *m <= 20) {
        let m = m as u64;
        let fact = |k: u64| (1..=k).fold(Nat::from_u64(1), |a, j| a.mul_u64(j));
        let kq = Rat::new(Int::from_nat(fact(2 * m + 1)), Int::from_nat(fact(m)));
        let kq = if m.is_multiple_of(2) { kq.neg() } else { kq };
        let kq = fx.q(&kq.mul(&Rat::from_i64(2)));
        let z = fx.mul(&[fx.rat(1, 2), fx.sub(fx.int(1), x)]);
        let f = fx.call("hypergeometric_2f1_regularized", &[fx.neg(n), fx.add(&[n, two_a]), fx.add(&[al, fx.rat(1, 2)]), z]);
        let c = fx.div(fx.call("gamma", &[fx.add(&[n, two_a])]), fx.call("gamma", &[fx.add(&[n, fx.int(1)])]));
        return Ok(Some(fx.mul(&[c, kq, f])));
    }
    let c = fx.div(
        fx.call("gamma", &[fx.add(&[n, two_a])]),
        fx.mul(&[fx.call("gamma", &[fx.add(&[n, fx.int(1)])]), fx.call("gamma", &[two_a])]),
    );
    let z = fx.mul(&[fx.rat(1, 2), fx.sub(fx.int(1), x)]);
    let f = fx.call("hypergeometric_2f1", &[fx.neg(n), fx.add(&[n, two_a]), fx.add(&[al, fx.rat(1, 2)]), z]);
    Ok(Some(fx.mul(&[c, f])))
}

/// binomial(t, k) as the product t (t - 1) ... (t - k + 1)/k! (a polynomial in t).
fn binom_poly<'h>(fx: &Fx<'_, '_, '_, 'h>, t: T<'h>, k: u64) -> T<'h> {
    let mut fs = vec![];
    let mut f = Nat::from_u64(1);
    for j in 0..k {
        fs.push(fx.sub(t, fx.int(j as i64)));
        f = f.mul_u64(j + 1);
    }
    fs.push(fx.q(&Rat::new(Int::from_i64(1), Int::from_nat(f))));
    fx.mul(&fs)
}

fn jacobi_p<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    let (n, al, be, x) = (a[0], a[1], a[2], a[3]);
    if let (Some(k), Some(ps)) = (small_nat(fx, n).filter(|k| *k <= EXACT_RECURRENCE_MAX), rats_of(fx, &[al, be, x])) {
        if let Some(v) = exact_recurrence(fx, 2, k, &ps)? {
            return Ok(Some(fx.q(&v)));
        }
    }
    if let Some(k) = degree_of(fx, n) {
        if let Some(v) = jacobi_machine(fx, k, al, be, x)? {
            return Ok(Some(v));
        }
    }
    if let Some(k) = small_nat(fx, n).filter(|k| *k <= 100) {
        // sum_s binomial(n + a, n - s) binomial(n + b, s) ((x - 1)/2)^s ((x + 1)/2)^(n - s)
        let xm = fx.mul(&[fx.rat(1, 2), fx.sub(x, fx.int(1))]);
        let xp = fx.mul(&[fx.rat(1, 2), fx.add(&[x, fx.int(1)])]);
        let na = fx.add(&[n, al]);
        let nb = fx.add(&[n, be]);
        let terms: Vec<T<'h>> = (0..=k)
            .map(|s| {
                fx.mul(&[
                    binom_poly(fx, na, k - s),
                    binom_poly(fx, nb, s),
                    fx.pow(xm, fx.int(s as i64)),
                    fx.pow(xp, fx.int((k - s) as i64)),
                ])
            })
            .collect();
        let t = fx.add(&terms);
        return Ok(Some(expand(fx, t)?));
    }
    if !(a.iter().all(|&t| fx.num(t).is_some()) && any_inexact(fx, a)) {
        return Ok(None);
    }
    // gamma(n + a + 1)/(gamma(n + 1) gamma(a + 1)) 2F1(-n, n + a + b + 1; a + 1; (1 - x)/2)
    let c = fx.div(
        fx.call("gamma", &[fx.add(&[n, al, fx.int(1)])]),
        fx.mul(&[fx.call("gamma", &[fx.add(&[n, fx.int(1)])]), fx.call("gamma", &[fx.add(&[al, fx.int(1)])])]),
    );
    let z = fx.mul(&[fx.rat(1, 2), fx.sub(fx.int(1), x)]);
    let f = fx.call("hypergeometric_2f1", &[fx.neg(n), fx.add(&[n, al, be, fx.int(1)]), fx.add(&[al, fx.int(1)]), z]);
    Ok(Some(fx.mul(&[c, f])))
}

fn zernike_r<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    // R_n^m(r) = sum_k (-1)^k (n - k)!/(k! ((n + m)/2 - k)! ((n - m)/2 - k)!) r^(n - 2k) for n - m even
    let (Some(n), Some(m)) = (small_nat(fx, a[0]), small_int(fx, a[1])) else { return Ok(None) };
    let m = m.unsigned_abs();
    if m > n || (n - m) % 2 == 1 {
        return Ok(Some(fx.int(0)));
    }
    let fact = |k: u64| (1..=k).fold(Nat::from_u64(1), |a, j| a.mul_u64(j));
    let mut c = vec![Rat::from_i64(0); n as usize + 1];
    for k in 0..=(n - m) / 2 {
        let num = Int::from_nat(fact(n - k));
        let den = Int::from_nat(fact(k).mul(&fact((n + m) / 2 - k)).mul(&fact((n - m) / 2 - k)));
        let q = Rat::new(if k % 2 == 1 { num.neg() } else { num }, den);
        c[(n - 2 * k) as usize] = q;
    }
    Ok(Some(poly_at(fx, &c, a[2])))
}

/// sqrt((2l + 1)/(4 pi) (l - m)!/(l + m)!) sin^m(theta) d^m P_l/dx^m at x = cos(theta), by the recurrence of the fully
/// normalised associated Legendre functions (start sqrt((2m + 1)!!/(2m)!!/(4 pi)) sin^m, then
/// P_l = a_(l,m) (x P_(l-1) - b_(l,m) P_(l-2)) with a = sqrt((4 l^2 - 1)/(l^2 - m^2)), b = sqrt(((l - 1)^2 - m^2)/(4 (l - 1)^2 - 1));
/// Holmes and Featherstone, J. Geodesy 76 (2002) 279, and DLMF 14.10.3). Every quantity stays of order one; the
/// start is carried with a power of 1e200 when sin^m underflows. `None` outside 0 <= m <= l.
fn normalized_legendre_machine(l: u64, m: u64, theta: f64) -> Option<f64> {
    if m > l {
        return None;
    }
    let (s, x) = (theta.sin(), theta.cos());
    let mut p = (0.25 / std::f64::consts::PI).sqrt();
    let mut shifts = 0i32;
    for i in 1..=m {
        let i = i as f64;
        p *= s * ((2.0 * i + 1.0) / (2.0 * i)).sqrt();
        if p != 0.0 && p < 1e-200 {
            p *= 1e200;
            shifts += 1;
        }
    }
    if l > m {
        let mut p0 = p;
        let mut p1 = x * (2.0 * m as f64 + 3.0).sqrt() * p;
        for ll in (m + 2)..=l {
            let (lf, mf) = (ll as f64, m as f64);
            let a = ((4.0 * lf * lf - 1.0) / (lf * lf - mf * mf)).sqrt();
            let b = (((lf - 1.0).powi(2) - mf * mf) / (4.0 * (lf - 1.0).powi(2) - 1.0)).sqrt();
            let next = a * (x * p1 - b * p0);
            p0 = p1;
            p1 = next;
        }
        p = p1;
    }
    for _ in 0..shifts {
        p *= 1e-200;
    }
    Some(p)
}

fn spherical_harmonic_y<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    let (th, ph) = (a[2], a[3]);
    let (Some(l), Some(m)) = (small_nat(fx, a[0]), small_int(fx, a[1])) else { return Ok(None) };
    let am = m.unsigned_abs();
    if am > l {
        return Ok(Some(fx.int(0)));
    }
    // sqrt((2l + 1)/(4 pi) (l - |m|)!/(l + |m|)!) (-1)^|m| sin^|m| P_l^(|m|)(cos) e^(i m phi), and the sign
    // (-1)^|m| once more for m < 0 (Y_l^-m = (-1)^m conj(Y_l^m))
    let ratio = falling_ratio(l, am).mul(&qi(2 * l as i64 + 1, 4));
    // a machine polar angle: the normalised recurrence (no (2m - 1)!! overflow), sign (-1)^m for m > 0
    if l >= RECURRENCE_FROM && fx.machine_of(th).is_some_and(f64::is_finite) {
        if let Some(pbar) = normalized_legendre_machine(l, am, fx.machine_of(th).unwrap_or(0.0)) {
            let sign = if am % 2 == 1 && m > 0 { -1 } else { 1 };
            let e = fx.call("exp", &[fx.mul(&[fx.i(), fx.int(m), ph])]);
            return Ok(Some(fx.mul(&[fx.int(sign), fx.real(pbar), e])));
        }
    }
    let k = fx.mul(&[fx.sqrt(fx.q(&ratio)), fx.pow(fx.sym("pi"), fx.rat(-1, 2))]);
    let mut cos = fx.call("cos", &[th]);
    let mut dp = None;
    if machine_x(fx, th).is_some() {
        // a machine angle: cos as a number and the recurrence for the derivative of P_l
        let cv = fx.eval(cos)?;
        if fx.num(cv).is_some() {
            cos = cv;
            dp = legendre_derivative_machine(fx, l, am, cv)?;
        }
    }
    let dp = match dp {
        Some(v) => v,
        None => poly_at(fx, &derivative(&coefficients(Family::Legendre, l), am), cos),
    };
    let sign = if am % 2 == 1 && m > 0 { -1 } else { 1 };
    let e = fx.call("exp", &[fx.mul(&[fx.i(), fx.int(m), ph])]);
    let s = fx.pow(fx.call("sin", &[th]), fx.int(am as i64));
    Ok(Some(fx.mul(&[fx.int(sign), k, s, dp, e])))
}

// ---------------------------------------------------------------- combinatorial

/// (F_n, F_{n+1}) by fast doubling.
fn fib_pair(n: u64) -> (Int, Int) {
    if n == 0 {
        return (Int::from_i64(0), Int::from_i64(1));
    }
    let (a, b) = fib_pair(n / 2);
    // F_2k = F_k (2 F_{k+1} - F_k), F_{2k+1} = F_k^2 + F_{k+1}^2
    let c = a.mul(&b.shl(1).sub(&a));
    let d = a.mul(&a).add(&b.mul(&b));
    if n.is_multiple_of(2) {
        (c, d)
    } else {
        let e = c.add(&d);
        (d, e)
    }
}

/// F_n for any integer n (F_-n = (-1)^(n+1) F_n).
fn fib(n: i64) -> Int {
    let f = fib_pair(n.unsigned_abs()).0;
    if n < 0 && n % 2 == 0 {
        f.neg()
    } else {
        f
    }
}

/// L_n = F_{n-1} + F_{n+1}.
fn lucas(n: i64) -> Int {
    fib(n - 1).add(&fib(n + 1))
}

fn golden<'h>(fx: &Fx<'_, '_, '_, 'h>) -> T<'h> {
    fx.mul(&[fx.rat(1, 2), fx.add(&[fx.int(1), fx.sqrt(fx.int(5))])])
}

/// The polynomial F_n(x) (F_0 = 0, F_1 = 1, F_{n+1} = x F_n + F_{n-1}) or the Lucas L_n(x) (L_0 = 2, L_1 = x).
fn fib_poly(n: u64, lucas: bool) -> Vec<Rat> {
    let r = Rat::from_i64;
    let (mut prev, mut cur) = if lucas { (vec![r(2)], vec![r(0), r(1)]) } else { (vec![r(0)], vec![r(1)]) };
    if n == 0 {
        return prev;
    }
    for _ in 1..n {
        let next = step(&cur, &prev, &r(0), &r(1), &r(1), &r(1));
        prev = cur;
        cur = next;
    }
    cur
}

fn fibonacci<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    let n = a[0];
    if a.len() == 2 {
        let Some(k) = small_nat(fx, n) else {
            if !any_inexact(fx, a) || a.iter().any(|&t| fx.num(t).is_none()) {
                return Ok(None);
            }
            // any order: (phi^n - cos(pi n) phi^-n)/sqrt(x^2 + 4), phi = (x + sqrt(x^2 + 4))/2
            let x = a[1];
            let r = fx.sqrt(fx.add(&[fx.pow(x, fx.int(2)), fx.int(4)]));
            let phi = fx.mul(&[fx.rat(1, 2), fx.add(&[x, r])]);
            let c = fx.call("cos", &[fx.mul(&[fx.sym("pi"), n])]);
            let v = fx.sub(fx.pow(phi, n), fx.mul(&[c, fx.pow(phi, fx.neg(n))]));
            return Ok(Some(fx.div(v, r)));
        };
        return Ok(Some(poly_at(fx, &fib_poly(k, false), a[1])));
    }
    if let Some(k) = fx.int_of(n).filter(|k| k.unsigned_abs() <= 10_000_000) {
        return Ok(Some(fx.integer(&fib(k))));
    }
    if !any_inexact(fx, a) {
        return Ok(None);
    }
    // (phi^n - cos(pi n) phi^-n)/sqrt(5)
    let phi = golden(fx);
    let c = fx.call("cos", &[fx.mul(&[fx.sym("pi"), n])]);
    let v = fx.sub(fx.pow(phi, n), fx.mul(&[c, fx.pow(phi, fx.neg(n))]));
    Ok(Some(fx.mul(&[v, fx.pow(fx.int(5), fx.rat(-1, 2))])))
}

fn lucas_l<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    let n = a[0];
    if a.len() == 2 {
        let Some(k) = small_nat(fx, n) else { return Ok(None) };
        return Ok(Some(poly_at(fx, &fib_poly(k, true), a[1])));
    }
    if let Some(k) = fx.int_of(n).filter(|k| k.unsigned_abs() <= 10_000_000) {
        return Ok(Some(fx.integer(&lucas(k))));
    }
    if !any_inexact(fx, a) {
        return Ok(None);
    }
    let phi = golden(fx);
    let c = fx.call("cos", &[fx.mul(&[fx.sym("pi"), n])]);
    Ok(Some(fx.add(&[fx.pow(phi, n), fx.mul(&[c, fx.pow(phi, fx.neg(n))])])))
}

fn pochhammer<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    let (x, n) = (a[0], a[1]);
    if let Some(k) = small_int(fx, n).filter(|k| k.unsigned_abs() <= 10_000) {
        if k >= 0 {
            let fs: Vec<T<'h>> = (0..k).map(|j| fx.add(&[x, fx.int(j)])).collect();
            return Ok(Some(fx.mul(&fs)));
        }
        // (x)_-k = 1/((x - 1)(x - 2)...(x - k))
        let fs: Vec<T<'h>> = (1..=-k).map(|j| fx.sub(x, fx.int(j))).collect();
        return Ok(Some(fx.pow(fx.mul(&fs), fx.int(-1))));
    }
    let numeric = a.iter().all(|&t| fx.num(t).is_some());
    if !numeric {
        return Ok(None);
    }
    let g = fx.div(fx.call("gamma", &[fx.add(&[x, n])]), fx.call("gamma", &[x]));
    if any_inexact(fx, a) {
        return Ok(Some(g));
    }
    // exact rationals: a closed gamma ratio when the gammas have exact values
    let v = fx.eval(g)?;
    Ok((!fx.contains(v, "gamma")).then_some(v))
}

fn factorial_power<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    let (x, n) = (a[0], a[1]);
    let h = if a.len() == 3 { a[2] } else { fx.int(1) };
    if let Some(k) = small_nat(fx, n).filter(|k| *k <= 10_000) {
        let fs: Vec<T<'h>> = (0..k as i64).map(|j| fx.sub(x, fx.mul(&[fx.int(j), h]))).collect();
        return Ok(Some(fx.mul(&fs)));
    }
    if a.len() == 2 && a.iter().all(|&t| fx.num(t).is_some()) && any_inexact(fx, a) {
        let g = fx.div(
            fx.call("gamma", &[fx.add(&[x, fx.int(1)])]),
            fx.call("gamma", &[fx.add(&[fx.sub(x, n), fx.int(1)])]),
        );
        return Ok(Some(g));
    }
    Ok(None)
}

fn factorial2<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    let n = a[0];
    if let Some(k) = fx.int_of(n).filter(|k| (-1..=100_000).contains(k)) {
        let mut p = Nat::from_u64(1);
        let mut j = k;
        while j > 1 {
            p = p.mul_u64(j as u64);
            j -= 2;
        }
        return Ok(Some(fx.integer(&Int::from_nat(p))));
    }
    if !any_inexact(fx, a) {
        return Ok(None);
    }
    // 2^((1 + 2x - cos(pi x))/4) pi^((cos(pi x) - 1)/4) gamma(1 + x/2)
    let c = fx.call("cos", &[fx.mul(&[fx.sym("pi"), n])]);
    let e2 = fx.mul(&[fx.rat(1, 4), fx.add(&[fx.int(1), fx.mul(&[fx.int(2), n]), fx.neg(c)])]);
    let ep = fx.mul(&[fx.rat(1, 4), fx.sub(c, fx.int(1))]);
    let g = fx.call("gamma", &[fx.add(&[fx.int(1), fx.mul(&[fx.rat(1, 2), n])])]);
    Ok(Some(fx.mul(&[fx.pow(fx.int(2), e2), fx.pow(fx.sym("pi"), ep), g])))
}

fn subfactorial<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    let Some(k) = small_nat(fx, a[0]) else {
        // an inexact argument: !x = Gamma(x + 1, -1)/e
        if fx.num(a[0]).is_some() && any_inexact(fx, a) {
            // an integer-valued machine real: the count itself, as a machine number
            if let Some(v) = fx.machine_of(a[0]).filter(|v| v.fract() == 0.0 && (0.0..=170.0).contains(v)) {
                let exact = subfactorial(fx, &[fx.int(v as i64)])?;
                return Ok(match exact {
                    Some(e) => Some(fx.eval(fx.call("n", &[e]))?),
                    None => None,
                });
            }
            let g = fx.call("gamma", &[fx.add(&[a[0], fx.int(1)]), fx.int(-1)]);
            // a ball: e as a ball of the argument's precision, so the result carries its own radius
            let ball = match fx.num(a[0]) {
                Some(Num::Real(x)) if x.kind() == mc_arith::Kind::Ball => Some(x.prec()),
                Some(Num::Complex(x, y)) if x.kind() == mc_arith::Kind::Ball || y.kind() == mc_arith::Kind::Ball => Some(x.prec().max(y.prec())),
                _ => None,
            };
            if let Some(w) = ball {
                let d = fx.int((w.unwrap_or(53) as f64 * std::f64::consts::LOG10_2).ceil() as i64 + 6);
                let einv = fx.eval(fx.call("n", &[fx.call("exp", &[fx.int(-1)]), d]))?;
                let gv = fx.eval(g)?;
                let prod = fx.mul(&[gv, einv]);
                return Ok(Some(fx.eval(prod)?));
            }
            let v = fx.div(g, fx.sym("e"));
            return Ok(Some(crate::cover::numify(fx, v, a)?));
        }
        return Ok(None);
    };
    // !n = n !(n-1) + (-1)^n
    let mut d = Int::from_i64(1);
    for j in 1..=k {
        let s = if j.is_multiple_of(2) { 1 } else { -1 };
        d = d.mul(&Int::from_u64(j)).add(&Int::from_i64(s));
        if j.is_multiple_of(64) {
            fx.tick(1)?;
        }
    }
    Ok(Some(fx.integer(&d)))
}

fn multinomial<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    if a.is_empty() {
        return Ok(Some(fx.int(1)));
    }
    let ks: Option<Vec<u64>> = a.iter().map(|&t| small_nat(fx, t)).collect();
    if let Some(ks) = ks {
        // the product of binomials binomial(k1 + ... + kj, kj)
        let mut total = 0u64;
        let mut acc = Rat::from_i64(1);
        for k in ks {
            for j in 1..=k {
                acc = acc.mul(&qi((total + j) as i64, j as i64));
            }
            total += k;
        }
        return Ok(Some(fx.q(&acc)));
    }
    if !(a.iter().all(|&t| fx.num(t).is_some()) && any_inexact(fx, a)) {
        return Ok(None);
    }
    let s = fx.add(a);
    let mut den = vec![];
    for &x in a {
        den.push(fx.call("gamma", &[fx.add(&[x, fx.int(1)])]));
    }
    Ok(Some(fx.div(fx.call("gamma", &[fx.add(&[s, fx.int(1)])]), fx.mul(&den))))
}

/// H(a, b) = sum_(a <= j < b) 1/j by binary splitting: P/Q of the halves combined as (P1 Q2 + P2 Q1)/(Q1 Q2), one
/// reduction at the end (quasi-linear, where adding the fractions one by one is quadratic).
pub(crate) fn harmonic_split(a: u64, b: u64) -> Rat {
    fn go(a: u64, b: u64) -> (Int, Int) {
        if b - a <= 8 {
            let (mut p, mut q) = (Int::from_i64(0), Int::from_i64(1));
            for j in a..b {
                let jj = Int::from_u64(j);
                p = p.mul(&jj).add(&q);
                q = q.mul(&jj);
            }
            return (p, q);
        }
        let m = a + (b - a) / 2;
        let (p1, q1) = go(a, m);
        let (p2, q2) = go(m, b);
        (p1.mul(&q2).add(&p2.mul(&q1)), q1.mul(&q2))
    }
    if b <= a {
        return Rat::from_i64(0);
    }
    let (p, q) = go(a, b);
    Rat::new(p, q)
}

fn harmonic_number<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    let n = a[0];
    let r = if a.len() == 2 { Some(a[1]) } else { None };
    if let Some(k) = small_nat(fx, n) {
        match r.map(|r| fx.int_of(r)) {
            None => {
                fx.tick(k / 256)?;
                return Ok(Some(fx.q(&harmonic_split(1, k + 1))));
            }
            Some(Some(p)) if p >= 0 && k <= 10_000 => {
                let mut s = Rat::from_i64(0);
                for j in 1..=k {
                    s = s.add(&Rat::from_i64(1).mul(&Rat::new(Int::from_i64(1), Int::from_u64(j).pow_u64_int(p as u64))));
                }
                return Ok(Some(fx.q(&s)));
            }
            _ => {
                let r = r.expect("two arguments");
                let terms: Vec<T<'h>> = (1..=k.min(10_000)).map(|j| fx.pow(fx.int(j as i64), fx.neg(r))).collect();
                return Ok(Some(fx.add(&terms)));
            }
        }
    }
    if r.is_none() && fx.num(n).is_some() && any_inexact(fx, a) {
        // H(x) = digamma(x + 1) + euler_gamma
        return Ok(Some(fx.add(&[fx.call("digamma", &[fx.add(&[n, fx.int(1)])]), fx.sym("euler_gamma")])));
    }
    if let Some(r) = r {
        // H(x, r) = zeta(r) - zeta(r, x + 1) at numbers, digamma(x + 1) + euler_gamma for r = 1
        if fx.num(n).is_some() && fx.num(r).is_some() && any_inexact(fx, a) {
            let x1 = fx.add(&[n, fx.int(1)]);
            let one = fx.machine_of(r).is_some_and(|v| v == 1.0);
            let v = if one {
                fx.add(&[fx.call("digamma", &[x1]), fx.sym("euler_gamma")])
            } else {
                fx.sub(fx.call("zeta", &[r]), fx.call("zeta", &[r, x1]))
            };
            // balls: evaluated as they are (the result carries its radius; the caller's n judges it)
            let ball = a.iter().any(|&t| match fx.num(t) {
                Some(Num::Real(x)) => x.kind() == mc_arith::Kind::Ball,
                Some(Num::Complex(x, y)) => x.kind() == mc_arith::Kind::Ball || y.kind() == mc_arith::Kind::Ball,
                None => false,
            });
            if ball && !one {
                return Ok(Some(fx.eval(v)?));
            }
            return Ok(Some(crate::cover::numify(fx, v, a)?));
        }
        // H(n, -m) = 1^m + ... + n^m: the sum's closed form (a polynomial in n)
        if let Some(m) = fx.int_of(r).filter(|m| (-64..=0).contains(m)) {
            if fx.num(n).is_none() {
                let j = fx.sym("harmonic$j");
                let it = fx.list(&[j, fx.int(1), n]);
                return Ok(Some(fx.eval(fx.call("sum", &[fx.pow(j, fx.int(-m)), it]))?));
            }
        }
    }
    Ok(None)
}

/// i^p for the product: helper trait for integer powers of `Int`.
trait PowInt {
    fn pow_u64_int(&self, e: u64) -> Int;
}
impl PowInt for Int {
    fn pow_u64_int(&self, e: u64) -> Int {
        let mut r = Int::from_i64(1);
        for _ in 0..e {
            r = r.mul(self);
        }
        r
    }
}

fn hyperfactorial<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    // a negative integer -n: H(-n) = K(1 - n) = (-1)^(n (n - 1)/2) H(n - 1), the terms (-m) log(-m) = -m (log m + pi i)
    // of the sum below (round 37: hyperfactorial(-2) stayed unevaluated, the value is -1)
    if let Some(n) = fx.int_of(a[0]).filter(|n| (-2001..0).contains(n)) {
        let m = -n;
        let h = fx.call("hyperfactorial", &[fx.int(m - 1)]);
        let s = if (m * (m - 1) / 2) % 2 == 0 { 1 } else { -1 };
        return Ok(Some(fx.mul(&[fx.int(s), h])));
    }
    let Some(k) = small_nat(fx, a[0]).filter(|k| *k <= 2000) else {
        if fx.num(a[0]).is_some() && any_inexact(fx, a) {
            return hyperfactorial_numeric(fx, a);
        }
        return Ok(None);
    };
    let mut p = Int::from_i64(1);
    for j in 1..=k {
        p = p.mul(&Int::from_u64(j).pow_u64_int(j));
        fx.tick(1)?;
    }
    Ok(Some(fx.integer(&p)))
}

/// H(z) at an inexact z as the K-function K(z + 1) = exp(zeta'(-1, z + 1) - zeta'(-1)) (DLMF 5.17; the definition behind
/// the real domain z >= -1 and the cut z <= -1 of the function's property table). With a = z + 1: for Re a > 0,
/// log K(a) = (a - 1) log Gamma(a) - log G(a) (both sides analytic there and equal on a > 0); elsewhere the defining
/// sum zeta(s, a) = sum_j (a + j)^(-s) gives zeta'(-1, a) = -sum_(j<N) (a + j) log(a + j) + zeta'(-1, a + N) with
/// principal logarithms, N the first shift with Re(a + N) > 0. On the cut (a real < 0) the logarithms of the negative
/// terms carry pi i: H(z) = |H(z)| e^(pi i P), P = the sum of |a + j| over the negative terms (round 37: Gamma(z + 1)^z/G(z
/// + 1) gave the real -0.98686 at z = -2.2, the definition gives 0.98686 e^(1.4 pi i)).
fn hyperfactorial_numeric<'h>(fx: &mut Fx<'_, '_, '_, 'h>, args: &[T<'h>]) -> R<'h> {
    let Some((re, im)) = crate::cover::parts(fx, args[0]) else { return Ok(None) };
    if !re.is_finite() || !im.is_finite() {
        return Ok(None);
    }
    let re = re + 1.0;
    // log H is about |z|^2 log|z|/2: that many more bits keep the digits of H after exp
    let size = (re * re + im * im).sqrt() + 2.0;
    let extra = (size * size * size.ln()).log2().max(0.0) as u64 + 8;
    let Some((wk, xs)) = crate::cover::work(fx, args, extra) else { return Ok(None) };
    let a = fx.eval(fx.add(&[xs[0], fx.int(1)]))?;
    let log_k = |fx: &mut Fx<'_, '_, '_, 'h>, b: T<'h>| -> Res<T<'h>> {
        let lg = fx.call("log_gamma", &[b]);
        let lgb = fx.call("log_barnes_g", &[b]);
        fx.eval(fx.sub(fx.mul(&[fx.sub(b, fx.int(1)), lg]), lgb))
    };
    if re > 0.0 {
        let l = log_k(fx, a)?;
        let v = fx.eval(fx.call("exp", &[l]))?;
        return Ok(wk.out(fx, v, wk.real));
    }
    let n = (1.0 - re).ceil();
    if n > 1e7 {
        // the sum would take more than 10^7 terms: decline rather than run for minutes
        return Ok(None);
    }
    let n = n as i64;
    let real = wk.real;
    // the sum of (a + j) log|a + j| (real a) or (a + j) log(a + j), and P
    let mut s = fx.int(0);
    let mut p = fx.int(0);
    for j in 0..n {
        if j % 64 == 63 {
            fx.tick(1)?;
        }
        let t = fx.eval(fx.add(&[a, fx.int(j)]))?;
        if fx.num(t).is_some_and(|v| v.is_zero()) {
            continue;
        }
        if real {
            let neg = fx.f64_of(t).is_some_and(|v| v < 0.0);
            let l = fx.call("log", &[fx.call("abs", &[t])]);
            s = fx.eval(fx.add(&[s, fx.mul(&[t, l])]))?;
            if neg {
                p = fx.eval(fx.sub(p, t))?;
            }
        } else {
            s = fx.eval(fx.add(&[s, fx.mul(&[t, fx.call("log", &[t])])]))?;
        }
    }
    let b = fx.eval(fx.add(&[a, fx.int(n)]))?;
    let lk = log_k(fx, b)?;
    let l = fx.eval(fx.sub(lk, s))?;
    if !real {
        let v = fx.eval(fx.call("exp", &[l]))?;
        return Ok(wk.out(fx, v, false));
    }
    let m = fx.call("exp", &[l]);
    // P an integer (z an exact integer as a machine number or a ball of radius 0): the phase is +-1 exactly (H(-2.) =
    // -1, not -1 + 1e-16 i)
    if let Some(pv) = fx.f64_of(p).filter(|v| v.fract() == 0.0 && v.abs() < 9e15 && !crate::cover::has_radius(fx, p)) {
        let sign = if (pv as i64) % 2 == 0 { 1 } else { -1 };
        let v = fx.eval(fx.mul(&[fx.int(sign), m]))?;
        return Ok(wk.out(fx, v, true));
    }
    let ph = fx.call("exp", &[fx.mul(&[fx.sym("pi"), fx.i(), p])]);
    let v = fx.eval(fx.mul(&[m, ph]))?;
    Ok(wk.out(fx, v, false))
}

// ---------------------------------------------------------------- Lambert W and others

fn cabs(z: C64) -> f64 {
    z.re.hypot(z.im)
}

/// The branch k of W at a complex machine number by Halley's iteration from the usual starting values.
fn lambert_w(k: i64, z: C64) -> Option<C64> {
    if z.re == 0.0 && z.im == 0.0 {
        return (k == 0).then_some(C64::real(0.0));
    }
    let e = std::f64::consts::E;
    // starting value: the branch-point series near -1/e, else the asymptotic log form
    let p2 = C64::real(2.0 * e) * z + C64::real(2.0);
    let near_branch = cabs(z + C64::real(1.0 / e)) < 0.3 && (k == 0 || (k == -1 && z.im <= 0.0) || (k == 1 && z.im > 0.0));
    let mut w = if near_branch {
        let p = p2.sqrt();
        let p = if k == 0 { p } else { C64::real(0.0) - p };
        C64::real(-1.0) + p - p * p / C64::real(3.0)
    } else if k == 0 && cabs(z) < 3.0 && cabs(z + C64::real(1.0)) > 0.5 {
        // log(1 + z) is a good start on the principal branch
        (C64::real(1.0) + z).log()
    } else {
        let l1 = z.log() + C64::new(0.0, 2.0 * std::f64::consts::PI * k as f64);
        l1 - l1.log()
    };
    for _ in 0..100 {
        let ew = w.exp();
        let f = w * ew - z;
        let w1 = w + C64::real(1.0);
        let denom = ew * w1 - (w + C64::real(2.0)) * f / (C64::real(2.0) * w1);
        let dw = f / denom;
        w = w - dw;
        if cabs(dw) <= 1e-16 * cabs(w).max(1e-300) {
            break;
        }
    }
    (w.re.is_finite() && w.im.is_finite()).then_some(w)
}

fn product_log_e<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    let z = *a.last().expect("one or two arguments");
    if a.len() == 2 && !fx.is_int(a[0], 0) {
        return Ok(None);
    }
    if fx.is_int(z, 0) {
        return Ok(Some(z));
    }
    if z == fx.sym("e") {
        return Ok(Some(fx.int(1)));
    }
    let minus_inv_e = fx.neg(fx.pow(fx.sym("e"), fx.int(-1)));
    if z == minus_inv_e {
        return Ok(Some(fx.int(-1)));
    }
    if fx.infinity(z) == Some(0) {
        return Ok(Some(fx.inf()));
    }
    Ok(None)
}

/// The branch index of `product_log(k, z)`: an exact integer, or an inexact number with an integer value (`n`
/// turns the exact index into a float; B23-14).
fn branch_index<'h>(fx: &Fx<'_, '_, '_, 'h>, t: T<'h>) -> Option<i64> {
    fx.int_of(t).or_else(|| fx.f64_of(t).filter(|v| v.fract() == 0.0 && v.abs() < 1e9).map(|v| v as i64))
}

/// The branch k of W(z) at a ball z of `p` bits: Halley's iteration w <- w - f/(e^w (w + 1) - (w + 2) f/(2 w + 2)),
/// f = w e^w - z (cubic convergence), from the machine value, in ball arithmetic at p + 32 bits. The input radius
/// propagates through the last step; the convergence error is added as the square of the last correction (the
/// iteration is not an interval method: the enclosure is a good one, not a proof; NOTES.md).
fn lambert_ball<'h>(fx: &mut Fx<'_, '_, '_, 'h>, k: i64, z: T<'h>, p: u64) -> R<'h> {
    let zc = match fx.num(z) {
        Some(Num::Real(r)) => C64::real(r.to_f64()),
        Some(Num::Complex(x, y)) => C64::new(x.to_f64(), y.to_f64()),
        None => return Ok(None),
    };
    let Some(w0) = lambert_w(k, zc) else { return Ok(None) };
    let real_out = zc.im == 0.0 && zc.re >= -1.0 / std::f64::consts::E && (k == 0 || (k == -1 && zc.re < 0.0));
    let wp = p + 32;
    let ball = |v: f64| Real::Ball(mc_arith::num::Ball::exact(crate::ball::float_of(v)), wp);
    let mut w = if real_out { fx.ar.number(&Num::Real(ball(w0.re))) } else { fx.ar.number(&Num::Complex(ball(w0.re), ball(w0.im))) };
    // z at the working precision (its radius kept)
    let zb = match fx.num(z) {
        Some(Num::Real(Real::Ball(b, _))) => fx.ar.number(&Num::Real(Real::Ball(b, wp))),
        Some(Num::Complex(x, y)) => {
            let up = |r: Real| match r {
                Real::Ball(b, _) => Real::Ball(b, wp),
                other => other.to_kind(mc_arith::Kind::Ball, wp),
            };
            fx.ar.number(&Num::Complex(up(x), up(y)))
        }
        _ => z,
    };
    let target = 2f64.powf(-(wp as f64) / 2.5);
    let mut last = 1.0f64;
    for it in 0..40 {
        fx.tick(1)?;
        let e = fx.eval(fx.call("exp", &[w]))?;
        let f = fx.eval(fx.sub(fx.mul(&[w, e]), zb))?;
        let w1 = fx.add(&[w, fx.int(1)]);
        let denom = fx.sub(fx.mul(&[e, w1]), fx.div(fx.mul(&[fx.add(&[w, fx.int(2)]), f]), fx.mul(&[fx.int(2), w1])));
        let delta = fx.eval(fx.div(f, denom))?;
        let Some(dn) = fx.num(delta) else { return Ok(None) };
        let dm = match &dn {
            Num::Real(r) => r.to_f64().abs(),
            Num::Complex(x, y) => x.to_f64().hypot(y.to_f64()),
        };
        if !dm.is_finite() {
            return Ok(None);
        }
        w = fx.eval(fx.sub(w, delta))?;
        last = dm;
        if dm <= target && it >= 1 {
            break;
        }
    }
    if last > target * 4.0 {
        return Ok(None);
    }
    let Some(wn) = fx.num(w) else { return Ok(None) };
    let wmag = match &wn {
        Num::Real(r) => r.to_f64().abs(),
        Num::Complex(x, y) => x.to_f64().hypot(y.to_f64()),
    };
    let conv = if last * last > 1e-300 {
        mc_arith::num::Mag::from_float_up(&crate::ball::float_of(last * last * 2.0))
    } else {
        mc_arith::num::Mag::pow2((wmag.max(1e-300).log2().ceil() as i64) - wp as i64 + 8)
    };
    let fin = |r: Real| match r {
        Real::Ball(b, _) => Real::Ball(crate::ball::widen(b, &conv), p),
        other => other,
    };
    Ok(Some(fx.ar.number(&match wn {
        Num::Real(r) => Num::Real(fin(r)),
        Num::Complex(x, y) => Num::Complex(fin(x), fin(y)),
    })))
}

fn product_log_n<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    let (k, z) = if a.len() == 2 { (branch_index(fx, a[0]), a[1]) } else { (Some(0), a[0]) };
    let Some(k) = k else { return Ok(None) };
    let zc = match fx.num(z) {
        Some(Num::Real(r)) => C64::real(r.to_f64()),
        Some(Num::Complex(x, y)) => C64::new(x.to_f64(), y.to_f64()),
        None => return Ok(None),
    };
    let kind_machine = |t: T<'h>| {
        matches!(fx.num(t), Some(Num::Real(Real::Machine(_))) | Some(Num::Complex(Real::Machine(_), _)) | Some(Num::Complex(_, Real::Machine(_))))
    };
    let machine = a.iter().any(|&t| kind_machine(t));
    if !machine {
        // balls: the branch index (a ball) does not matter, the argument's precision does
        let prec = match fx.num(z) {
            Some(Num::Real(r)) => r.prec(),
            Some(Num::Complex(x, y)) => x.prec().max(y.prec()),
            None => None,
        };
        let Some(p) = prec.or_else(|| a.iter().filter_map(|&t| match fx.num(t) { Some(Num::Real(r)) => r.prec(), _ => None }).max()) else {
            return Ok(None);
        };
        return lambert_ball(fx, k, z, p);
    }
    let Some(w) = lambert_w(k, zc) else { return Ok(None) };
    let on_real_branch = zc.im == 0.0 && zc.re >= -1.0 / std::f64::consts::E && (k == 0 || (k == -1 && zc.re < 0.0));
    let real = on_real_branch;
    Ok(Some(if real { fx.real(w.re) } else { fx.ar.complex(fx.real(w.re), fx.real(w.im)) }))
}

fn dawson_e<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    let x = a[0];
    if fx.is_int(x, 0) {
        return Ok(Some(x));
    }
    if fx.infinity(x).is_some_and(|d| d % 2 == 0) {
        return Ok(Some(fx.int(0)));
    }
    Ok(fx.negated(x).map(|y| fx.neg(fx.call("dawson_f", &[y]))))
}

fn dawson_n<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    // sqrt(pi)/2 e^(-x^2) erfi(x)
    let x = a[0];
    let e = fx.call("exp", &[fx.neg(fx.pow(x, fx.int(2)))]);
    Ok(Some(fx.mul(&[fx.rat(1, 2), fx.sqrt(fx.sym("pi")), e, fx.call("erfi", &[x])])))
}

/// S(z) or C(z): (1 +- i)/4 (erf((1 + i) sqrt(pi) z/2) -+ i erf((1 - i) sqrt(pi) z/2)).
fn fresnel<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>], sine: bool) -> R<'h> {
    let z = a[0];
    let me = if sine { "fresnel_s" } else { "fresnel_c" };
    if fx.num(z).is_none() || !any_inexact(fx, a) {
        if fx.is_int(z, 0) {
            return Ok(Some(z));
        }
        if fx.infinity(z) == Some(0) {
            return Ok(Some(fx.rat(1, 2)));
        }
        if fx.infinity(z) == Some(2) {
            return Ok(Some(fx.rat(-1, 2)));
        }
        return Ok(fx.negated(z).map(|y| fx.neg(fx.call(me, &[y]))));
    }
    let i = fx.i();
    // a ball argument: pi at its precision (the symbolic sqrt(pi) next to a ball does not evaluate)
    let ball_prec = match fx.num(z) {
        Some(Num::Real(Real::Ball(_, p))) => Some(p),
        Some(Num::Complex(a, b)) => match (a, b) {
            (Real::Ball(_, p), _) | (_, Real::Ball(_, p)) => Some(p),
            _ => None,
        },
        _ => None,
    };
    let pi_t = match ball_prec {
        Some(p) => fx.eval(fx.call("n", &[fx.sym("pi"), fx.int((p as f64 * std::f64::consts::LOG10_2) as i64 + 8)]))?,
        None => fx.sym("pi"),
    };
    let sp = fx.sqrt(pi_t);
    let up = fx.call("erf", &[fx.mul(&[fx.rat(1, 2), fx.add(&[fx.int(1), i]), sp, z])]);
    let dn = fx.call("erf", &[fx.mul(&[fx.rat(1, 2), fx.sub(fx.int(1), i), sp, z])]);
    let v = if sine {
        fx.mul(&[fx.rat(1, 4), fx.add(&[fx.int(1), i]), fx.sub(up, fx.mul(&[i, dn]))])
    } else {
        fx.mul(&[fx.rat(1, 4), fx.sub(fx.int(1), i), fx.add(&[up, fx.mul(&[i, dn])])])
    };
    let v = fx.eval(v)?;
    // a real argument has a real value
    Ok(Some(if fx.num(z).is_some_and(|n| n.as_real().is_some()) { fx.call("re", &[v]) } else { v }))
}

fn fresnel_s<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    fresnel(fx, a, true)
}
fn fresnel_c<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    fresnel(fx, a, false)
}

/// j_k(x) = x^k/(2k+1)!! sum_m (-x^2/2)^m / (m! (2k+3)(2k+5)...(2k+2m+1)) for a machine real x.
fn sph_j_series(k: u32, x: f64) -> f64 {
    let mut t = 1.0 / (2 * k + 1) as f64;
    for i in 1..=k {
        t *= x / (2 * i - 1) as f64;
    }
    let (mut sum, q) = (t, -x * x / 2.0);
    for m in 1..200u32 {
        t *= q / (m as f64 * (2 * k + 2 * m + 1) as f64);
        sum += t;
        if t.abs() <= 1e-18 * sum.abs() {
            break;
        }
    }
    sum
}

/// The spherical Bessel functions at integer order n >= 0 by the recurrences from j_0, j_1 (y_0, y_1), elementary
/// in sin and cos; numerically sqrt(pi/(2 z)) J_{n+1/2}(z) for other orders.
fn spherical<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>], first: bool) -> R<'h> {
    let (n, z) = (a[0], a[1]);
    if let Some(k) = small_nat(fx, n).filter(|k| *k <= 60) {
        if fx.is_int(z, 0) {
            return Ok(if first { Some(fx.int(if k == 0 { 1 } else { 0 })) } else { None });
        }
        // j_k at a machine |z| < k: the upward recurrence loses everything (growth like (k/z)^k); the power series
        // converges fast there (Abramowitz-Stegun 10.1.2)
        if first && k >= 1 {
            if let Some(x) = fx.machine_of(z).filter(|x| x.is_finite() && x.abs() < k as f64) {
                return Ok(Some(fx.real(sph_j_series(k as u32, x))));
            }
        }
        let (s, c) = (fx.call("sin", &[z]), fx.call("cos", &[z]));
        let zi = fx.pow(z, fx.int(-1));
        let zi2 = fx.pow(z, fx.int(-2));
        let (f0, f1) = if first {
            (fx.mul(&[s, zi]), fx.sub(fx.mul(&[s, zi2]), fx.mul(&[c, zi])))
        } else {
            (fx.neg(fx.mul(&[c, zi])), fx.sub(fx.neg(fx.mul(&[c, zi2])), fx.mul(&[s, zi])))
        };
        if k == 0 {
            return Ok(Some(f0));
        }
        let (mut prev, mut cur) = (f0, f1);
        for j in 1..k as i64 {
            let next = fx.sub(fx.mul(&[fx.int(2 * j + 1), zi, cur]), prev);
            prev = cur;
            cur = if fx.num(z).is_some() { next } else { expand(fx, next)? };
        }
        return Ok(Some(cur));
    }
    if !(a.iter().all(|&t| fx.num(t).is_some()) && any_inexact(fx, a)) {
        return Ok(None);
    }
    // a machine order with an integer value: the elementary form at z as a machine number (real for real z, where
    // the Bessel form's sqrt(pi/(2 z)) and Y_(n+1/2) are complex for z < 0)
    if let Some(k) = fx.machine_of(n).filter(|v| v.fract() == 0.0 && (0.0..=60.0).contains(v)) {
        let zm = fx.eval(fx.call("n", &[z]))?;
        if fx.num(zm).is_some() {
            return spherical(fx, &[fx.int(k as i64), zm], first);
        }
    }
    let f = if first { "bessel_j" } else { "bessel_y" };
    let b = fx.call(f, &[fx.add(&[n, fx.rat(1, 2)]), z]);
    // sqrt(pi/2) z^(-1/2): on the cut z < 0 the value continuous from above (sqrt(pi/(2 z)) would take the other
    // side's sign there)
    let pre = fx.mul(&[fx.sqrt(fx.div(fx.sym("pi"), fx.int(2))), fx.pow(z, fx.rat(-1, 2))]);
    Ok(Some(fx.mul(&[pre, b])))
}

fn spherical_bessel_j<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    spherical(fx, a, true)
}
fn spherical_bessel_y<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    spherical(fx, a, false)
}

fn hankel<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>], s: i64) -> R<'h> {
    if !(a.iter().all(|&t| fx.num(t).is_some()) && any_inexact(fx, a)) {
        return Ok(None);
    }
    crate::cover::hankel_bessel(fx, a, s)
}
fn hankel_h1<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    hankel(fx, a, 1)
}
fn hankel_h2<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    hankel(fx, a, -1)
}

/// The Jacobi elliptic function pq = p/q for p, q in {s, c, d, n} (n = 1).
fn jacobi_ratio<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>], p: char, q: char) -> R<'h> {
    let f = |c: char, fx: &Fx<'_, '_, '_, 'h>| -> T<'h> {
        match c {
            's' => fx.call("jacobi_sn", a),
            'c' => fx.call("jacobi_cn", a),
            'd' => fx.call("jacobi_dn", a),
            _ => fx.int(1),
        }
    };
    // ball arguments: the quotient at a precision where it has its digits (the theta sums cancel, B23-23)
    let name = |c: char| match c {
        's' => Some("jacobi_sn"),
        'c' => Some("jacobi_cn"),
        'd' => Some("jacobi_dn"),
        _ => None,
    };
    let ball_args = a.iter().all(|&t| fx.num(t).is_some())
        && a.iter().any(|&t| fx.num(t).is_some_and(|n| n.kind() == mc_arith::Kind::Ball))
        && !a.iter().any(|&t| fx.num(t).is_some_and(|n| n.kind() == mc_arith::Kind::Machine));
    // m = 0 (circular) and m = 1 (hyperbolic): the ratio of two of sin/cos/1 or tanh/sech/1 in its own name, DLMF 22.5.1-2
    if a.len() == 2 && (fx.is_int(a[1], 0) || fx.is_int(a[1], 1)) {
        let one = fx.is_int(a[1], 1);
        let u = a[0];
        let name = match (p, q, one) {
            ('s', 'n', false) | ('s', 'd', false) => "sin",
            ('c', 'n', false) | ('c', 'd', false) => "cos",
            ('d', 'c', false) | ('n', 'c', false) => "sec",
            ('s', 'c', false) => "tan",
            ('n', 's', false) | ('d', 's', false) => "csc",
            ('c', 's', false) => "cot",
            ('s', 'n', true) => "tanh",
            ('c', 'n', true) | ('d', 'n', true) => "sech",
            ('s', 'd', true) | ('s', 'c', true) => "sinh",
            ('n', 'd', true) | ('n', 'c', true) => "cosh",
            ('n', 's', true) => "coth",
            ('d', 's', true) | ('c', 's', true) => "csch",
            _ => "",
        };
        if !name.is_empty() {
            return Ok(Some(fx.call(name, &[u])));
        }
        // dn and nd at m = 0, cd and dc at m = 1 are 1
        if matches!((p, q, one), ('d', 'n', false) | ('n', 'd', false) | ('c', 'd', true) | ('d', 'c', true)) {
            return Ok(Some(fx.int(1)));
        }
    }
    // x + a K(m) + b i K(1 - m): a constant times another Jacobi function of x (round 33)
    if a.len() == 2 && fx.num(a[0]).is_none() {
        if let Some(v) = crate::jshift::reduce(fx.h, &fx.ar, p, q, a[0], a[1]) {
            return Ok(Some(v));
        }
    }
    // machine arguments at a complex u: the complex double-double theta kernel (round 33)
    if a.len() == 2 {
        if let Some((ms, false)) = crate::fast29::machine_args(fx, a) {
            if let Some(v) = crate::fast33::ratio(p, q, ms[0], ms[1]) {
                return Ok(crate::fast29::machine_out(fx, v, false));
            }
        }
    }
    if ball_args {
        if let Some(v) = crate::cover::refined_ratio(fx, a, name(p), name(q))? {
            return Ok(Some(v));
        }
    }
    let (num, den) = (fx.eval(f(p, fx))?, fx.eval(f(q, fx))?);
    let symbolic = |t: T<'h>| fx.contains(t, "jacobi_sn") || fx.contains(t, "jacobi_cn") || fx.contains(t, "jacobi_dn");
    if symbolic(num) || symbolic(den) {
        return Ok(None);
    }
    if fx.is_int(den, 0) {
        return Ok(None);
    }
    Ok(Some(fx.div(num, den)))
}

macro_rules! jac {
    ($name:ident, $p:literal, $q:literal) => {
        fn $name<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
            jacobi_ratio(fx, a, $p, $q)
        }
    };
}
jac!(jacobi_cd, 'c', 'd');
jac!(jacobi_cs, 'c', 's');
jac!(jacobi_dc, 'd', 'c');
jac!(jacobi_ds, 'd', 's');
jac!(jacobi_nc, 'n', 'c');
jac!(jacobi_nd, 'n', 'd');
jac!(jacobi_ns, 'n', 's');
jac!(jacobi_sc, 's', 'c');
jac!(jacobi_sd, 's', 'd');

fn hyp_reg<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>], name: &str, b: usize) -> R<'h> {
    // a lower parameter -n at a non-positive integer: the limit
    // F~(as; -n; z) = (a1)_(n+1) ... z^(n+1) F~(as + n + 1; n + 2; z)
    // (an inexact real lower parameter with an integer value -n, 0., -1., ..., is the same limit: the regularized
    // function is entire in it; the result is then evaluated numerically at the precision of the arguments)
    let inexact_int = fx
        .num(a[b])
        .filter(mc_numeric::formula::inexact)
        .and_then(|_| fx.f64_of(a[b]))
        .filter(|v| *v <= 0.0 && v.fract() == 0.0 && v.abs() <= 100_000.0)
        .map(|v| v as i64);
    if let Some(n) = small_int(fx, a[b]).filter(|n| *n <= 0).map(|n| -n).or(inexact_int.map(|v| -v)) {
        // machine arguments: the limit evaluated in balls from the exact values of the machine numbers and rounded once
        // (2F1~(1, 2; -3; 4.5) = 120 z^4 (1 - z)^-6 was 1 ulp off in machine products)
        let machine = a.iter().any(|&t| fx.num(t).is_some_and(|x| x.kind() == mc_arith::Kind::Machine));
        if inexact_int.is_none() && machine {
            if let Some((wk, mut xs)) = crate::cover::work(fx, a, 24) {
                // the exact pole parameter stays exact (the limit above is then taken with exact shifts)
                xs[b] = a[b];
                let v = fx.eval(fx.call(&format!("{name}_regularized"), &xs))?;
                if let Some(out) = wk.out(fx, v, false) {
                    return Ok(Some(out));
                }
            }
        }
        let k = fx.int(n + 1);
        let mut factors = vec![fx.pow(a[b + 1], k)];
        let mut shifted = vec![];
        for &p in &a[..b] {
            factors.push(fx.call("pochhammer", &[p, k]));
            shifted.push(fx.add(&[p, k]));
        }
        shifted.push(fx.int(n + 2));
        shifted.push(a[b + 1]);
        factors.push(fx.call(&format!("{name}_regularized"), &shifted));
        let v = fx.mul(&factors);
        if inexact_int.is_some() {
            return Ok(Some(crate::cover::numify(fx, v, a)?));
        }
        return Ok(Some(v));
    }
    let f = fx.eval(fx.call(name, a))?;
    if fx.contains(f, name) && !any_inexact(fx, a) {
        return Ok(None);
    }
    Ok(Some(fx.div(f, fx.call("gamma", &[a[b]]))))
}
fn hyp0f1_reg<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    hyp_reg(fx, a, "hypergeometric_0f1", 0)
}
fn hyp1f1_reg<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    hyp_reg(fx, a, "hypergeometric_1f1", 1)
}
fn hyp2f1_reg<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    hyp_reg(fx, a, "hypergeometric_2f1", 2)
}

fn nome_e<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    let m = a[0];
    if fx.is_int(m, 0) {
        return Ok(Some(m));
    }
    if fx.is_int(m, 1) {
        return Ok(Some(fx.int(1)));
    }
    if fx.num(m).is_none() || !any_inexact(fx, a) {
        return Ok(None);
    }
    // an inexact 1: q = 1 in its kind (K(1) is infinite; the quotient below looped on a ball 1)
    if fx.f64_of(m) == Some(1.0) {
        return Ok(Some(m));
    }
    // a negative real m: q(m) = -q(m/(m - 1)) (from K(m) = K(m/(m-1))/sqrt(1 - m), DLMF 19.7.5), real
    if let Some(v) = fx.f64_of(m).filter(|v| *v < 0.0) {
        let _ = v;
        let mp = fx.div(m, fx.sub(m, fx.int(1)));
        return Ok(Some(fx.neg(fx.call("elliptic_nome_q", &[mp]))));
    }
    // q = exp(-pi K(1 - m)/K(m))
    let k1 = fx.call("elliptic_k", &[fx.sub(fx.int(1), m)]);
    let k = fx.call("elliptic_k", &[m]);
    Ok(Some(fx.call("exp", &[fx.neg(fx.mul(&[fx.sym("pi"), fx.div(k1, k)]))])))
}

/// K_nu(x) e^x (I_nu(x) e^-x): machine reals by the crate's scaled algorithms (no underflow at large x), the
/// product form otherwise.
fn scaled<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>], k: bool) -> R<'h> {
    let (nu, x) = (a[0], a[1]);
    if let (Some(n), Some(v)) = (fx.f64_of(nu), fx.machine_of(x).or_else(|| fx.machine_of(nu).and(fx.f64_of(x)))) {
        let r = if k { machine::bessel::bessel_k_scaled(n, v) } else { machine::bessel::bessel_i_scaled(n, v) };
        if !r.is_nan() {
            return Ok(Some(fx.real(r)));
        }
    }
    if fx.num(x).is_none() || fx.num(nu).is_none() {
        // symbolic: stays (the product form would lose the scaling)
        return Ok(None);
    }
    let (f, s) = if k { ("bessel_k", x) } else { ("bessel_i", fx.neg(x)) };
    let v = fx.mul(&[fx.call(f, &[nu, x]), fx.call("exp", &[s])]);
    if !any_inexact(fx, a) {
        let e = fx.eval(v)?;
        return Ok((!fx.contains(e, f)).then_some(e));
    }
    Ok(Some(v))
}
fn bessel_k_scaled<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    scaled(fx, a, true)
}
fn bessel_i_scaled<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    scaled(fx, a, false)
}

macro_rules! def {
    ($name:literal, $ar:expr, $e:expr, $n:expr, $s:literal) => {
        Def { name: $name, arities: $ar, listable: true, exact: $e, numeric: $n, summary: $s }
    };
}

/// The definitions of this module.
pub static DEFS: &[Def] = &[
    def!("fibonacci", &[1, 2], Some(fibonacci), None, "fibonacci(n): the Fibonacci number F_n (any real n by Binet's formula); fibonacci(n, x): the Fibonacci polynomial"),
    def!("lucas_l", &[1, 2], Some(lucas_l), None, "lucas_l(n): the Lucas number L_n; lucas_l(n, x): the Lucas polynomial"),
    def!("pochhammer", &[2], Some(pochhammer), None, "pochhammer(a, n): the rising factorial a (a + 1) ... (a + n - 1) = gamma(a + n)/gamma(a)"),
    def!("factorial_power", &[2, 3], Some(factorial_power), None, "factorial_power(x, n): the falling factorial x (x - 1) ... (x - n + 1); factorial_power(x, n, h) with step h"),
    def!("factorial2", &[1], Some(factorial2), None, "factorial2(n): the double factorial n (n - 2) (n - 4) ..."),
    def!("subfactorial", &[1], Some(subfactorial), None, "subfactorial(n): the number of derangements of n objects"),
    def!("multinomial", &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32], Some(multinomial), None, "multinomial(n1, n2, ...): the multinomial coefficient (n1 + n2 + ...)!/(n1! n2! ...)"),
    def!("harmonic_number", &[1, 2], Some(harmonic_number), None, "harmonic_number(n): the harmonic number H_n = 1 + 1/2 + ... + 1/n; harmonic_number(n, r): the sum of 1/k^r"),
    def!("hyperfactorial", &[1], Some(hyperfactorial), None, "hyperfactorial(n): the product 1^1 2^2 ... n^n"),
    def!("chebyshev_t", &[2], Some(chebyshev_t), None, "chebyshev_t(n, x): the Chebyshev polynomial of the first kind T_n(x) = cos(n acos(x))"),
    def!("chebyshev_u", &[2], Some(chebyshev_u), None, "chebyshev_u(n, x): the Chebyshev polynomial of the second kind U_n(x)"),
    def!("legendre_p", &[2, 3, 4], Some(legendre_p), None, "legendre_p(n, x): the Legendre polynomial P_n(x); legendre_p(n, m, x): the associated Legendre function P_n^m(x) (any degree and order for numbers); legendre_p(n, m, a, z): of type a (2 as legendre_p(n, m, z), 3 on the plane cut along (-inf, 1])"),
    def!("legendre_q", &[2], Some(legendre_q), None, "legendre_q(n, x): the Legendre function of the second kind Q_n(x) at integer n"),
    def!("hermite_h", &[2], Some(hermite_h), None, "hermite_h(n, x): the Hermite polynomial H_n(x) (physicists' normalization)"),
    def!("laguerre_l", &[2, 3], Some(laguerre_l), None, "laguerre_l(n, x): the Laguerre polynomial L_n(x); laguerre_l(n, a, x): the generalized Laguerre polynomial"),
    def!("gegenbauer_c", &[2, 3], Some(gegenbauer_c), None, "gegenbauer_c(n, a, x): the Gegenbauer polynomial C_n^(a)(x); gegenbauer_c(n, x) = (2/n) T_n(x)"),
    def!("jacobi_p", &[4], Some(jacobi_p), None, "jacobi_p(n, a, b, x): the Jacobi polynomial P_n^(a, b)(x)"),
    def!("zernike_r", &[3], Some(zernike_r), None, "zernike_r(n, m, r): the radial Zernike polynomial R_n^m(r)"),
    def!("spherical_harmonic_y", &[4], Some(spherical_harmonic_y), None, "spherical_harmonic_y(l, m, theta, phi): the spherical harmonic Y_l^m(theta, phi)"),
    def!("product_log", &[1, 2], Some(product_log_e), Some(product_log_n), "product_log(z): the principal branch of the Lambert W function (w e^w = z); product_log(k, z): branch k"),
    def!("dawson_f", &[1], Some(dawson_e), Some(dawson_n), "dawson_f(x): Dawson's integral e^(-x^2) times the integral of e^(t^2) from 0 to x"),
    def!("fresnel_s", &[1], Some(fresnel_s), None, "fresnel_s(z): the Fresnel integral S(z), the integral of sin(pi t^2/2) from 0 to z"),
    def!("fresnel_c", &[1], Some(fresnel_c), None, "fresnel_c(z): the Fresnel integral C(z), the integral of cos(pi t^2/2) from 0 to z"),
    def!("spherical_bessel_j", &[2], Some(spherical_bessel_j), None, "spherical_bessel_j(n, z): the spherical Bessel function j_n(z) = sqrt(pi/(2 z)) J_(n+1/2)(z)"),
    def!("spherical_bessel_y", &[2], Some(spherical_bessel_y), None, "spherical_bessel_y(n, z): the spherical Bessel function y_n(z) = sqrt(pi/(2 z)) Y_(n+1/2)(z)"),
    def!("hankel_h1", &[2], Some(hankel_h1), None, "hankel_h1(nu, z): the Hankel function of the first kind J_nu(z) + i Y_nu(z)"),
    def!("hankel_h2", &[2], Some(hankel_h2), None, "hankel_h2(nu, z): the Hankel function of the second kind J_nu(z) - i Y_nu(z)"),
    def!("jacobi_cd", &[2], Some(jacobi_cd), None, "jacobi_cd(u, m): the Jacobi elliptic function cn/dn"),
    def!("jacobi_cs", &[2], Some(jacobi_cs), None, "jacobi_cs(u, m): the Jacobi elliptic function cn/sn"),
    def!("jacobi_dc", &[2], Some(jacobi_dc), None, "jacobi_dc(u, m): the Jacobi elliptic function dn/cn"),
    def!("jacobi_ds", &[2], Some(jacobi_ds), None, "jacobi_ds(u, m): the Jacobi elliptic function dn/sn"),
    def!("jacobi_nc", &[2], Some(jacobi_nc), None, "jacobi_nc(u, m): the Jacobi elliptic function 1/cn"),
    def!("jacobi_nd", &[2], Some(jacobi_nd), None, "jacobi_nd(u, m): the Jacobi elliptic function 1/dn"),
    def!("jacobi_ns", &[2], Some(jacobi_ns), None, "jacobi_ns(u, m): the Jacobi elliptic function 1/sn"),
    def!("jacobi_sc", &[2], Some(jacobi_sc), None, "jacobi_sc(u, m): the Jacobi elliptic function sn/cn"),
    def!("jacobi_sd", &[2], Some(jacobi_sd), None, "jacobi_sd(u, m): the Jacobi elliptic function sn/dn"),
    def!("hypergeometric_0f1_regularized", &[2], Some(hyp0f1_reg), None, "hypergeometric_0f1_regularized(b, z): 0F1(; b; z)/gamma(b)"),
    def!("hypergeometric_1f1_regularized", &[3], Some(hyp1f1_reg), None, "hypergeometric_1f1_regularized(a, b, z): 1F1(a; b; z)/gamma(b)"),
    def!("hypergeometric_2f1_regularized", &[4], Some(hyp2f1_reg), None, "hypergeometric_2f1_regularized(a, b, c, z): 2F1(a, b; c; z)/gamma(c)"),
    def!("elliptic_nome_q", &[1], Some(nome_e), None, "elliptic_nome_q(m): the nome q = exp(-pi K(1 - m)/K(m))"),
    def!("bessel_k_scaled", &[2], Some(bessel_k_scaled), None, "bessel_k_scaled(nu, x): e^x K_nu(x), without underflow at large x"),
    def!("bessel_i_scaled", &[2], Some(bessel_i_scaled), None, "bessel_i_scaled(nu, x): e^-x I_nu(x), without overflow at large x"),
];

/// The scaled Bessel functions as library functions of numerical evaluation (gap queue G42: `n_dsolve` and `n`
/// evaluate them inside numeric islands without forming K or I).
pub struct Scaled {
    k: bool,
}

/// e^x K_nu(x) and e^-x I_nu(x).
pub static SCALED: [Scaled; 2] = [Scaled { k: true }, Scaled { k: false }];

impl mc_numeric::library::LibraryFn for Scaled {
    fn name(&self) -> &'static str {
        if self.k {
            "bessel_k_scaled"
        } else {
            "bessel_i_scaled"
        }
    }
    fn arity(&self) -> usize {
        2
    }
    fn ball(&self, args: &[Real], p: u64) -> Option<mc_numeric::cball::CBall> {
        let f = if self.k { crate::functions::Special::BesselK } else { crate::functions::Special::BesselI };
        let v = f.ball_value(args, p + 16)?;
        let x = match args[1].to_kind(mc_arith::Kind::Ball, p + 16) {
            Real::Ball(b, _) => b,
            _ => return None,
        };
        let e = mc_numeric::elementary::exp(&if self.k { x } else { x.neg() }, p + 16)?;
        Some(v.mul_real(&e, p))
    }
    fn machine(&self, args: &[f64]) -> mc_numeric::fun::Mach {
        let v = if self.k {
            machine::bessel::bessel_k_scaled(args[0], args[1])
        } else {
            machine::bessel::bessel_i_scaled(args[0], args[1])
        };
        mc_numeric::fun::Mach::Real(v)
    }
}

/// The principal Lambert W as a library function: machine values by Halley's iteration, balls by Newton's
/// iteration at the working precision from the machine value, the radius the last correction (doubled) plus the
/// argument's radius times |W'(x)| = |W/(x (1 + W))| (real x > -1/e).
pub struct ProductLog;

impl mc_numeric::library::LibraryFn for ProductLog {
    fn name(&self) -> &'static str {
        "product_log"
    }
    fn arity(&self) -> usize {
        1
    }
    fn ball(&self, args: &[Real], p: u64) -> Option<mc_numeric::cball::CBall> {
        use mc_arith::num::{Ball, Float, Mag};
        use mc_numeric::elementary as el;
        let w = p + 32;
        let x = match args[0].to_kind(mc_arith::Kind::Ball, w) {
            Real::Ball(b, _) => b,
            _ => return None,
        };
        let xf = Real::Ball(x.clone(), w).to_f64();
        if xf.is_nan() {
            return None;
        }
        if xf <= -1.0 / std::f64::consts::E + 1e-12 {
            // beyond the branch point the principal value is complex (the side Im x = +0)
            return self.ball_complex(&[mc_numeric::cball::CBall::real(x)], p);
        }
        let w0 = lambert_w(0, C64::real(xf))?.re;
        let mut y = match Real::Exact(mc_numeric::value::machine_rat(w0)?).to_kind(mc_arith::Kind::Ball, w) {
            Real::Ball(b, _) => Ball::exact(b.mid),
            _ => return None,
        };
        let xm = Ball::exact(x.mid.clone());
        // sizes are kept as base-2 logarithms: past about 1074 bits a machine float underflows to 0, whose log2
        // would become a huge exponent (and an impossible allocation)
        let mut last_l = 0.0f64;
        for _ in 0..(w as f64).log2().ceil() as usize + 4 {
            let e = el::exp(&y, w)?;
            let f = y.mul(&e, w).sub(&xm, w);
            let fp = e.mul(&y.add(&Ball::from_i64(1), w), w);
            let step = f.div(&fp, w)?;
            last_l = crate::ball::log2_up(&step);
            y = Ball::exact(y.sub(&step, w).mid);
            if last_l < -(w as f64) {
                break;
            }
        }
        // radius: the last correction (doubled), the argument's radius times the slope, and the target's last bits
        let wf = Real::Ball(y.clone(), w).to_f64();
        let slope = (wf / (xf * (1.0 + wf))).abs();
        let slope = if slope.is_finite() && slope > 0.0 { slope } else { 1.0 };
        let xr_l = if x.rad.is_zero() { f64::NEG_INFINITY } else { x.rad.log2() };
        let terms = [1.0 + last_l, slope.log2() + xr_l + 0.002, -(p as f64) - 8.0];
        let top = terms.iter().copied().filter(|t| t.is_finite()).fold(-(p as f64) - 8.0, f64::max);
        // a sum of three terms is below 4 times the largest
        let k = (top + 2.0).ceil() as i64;
        let b = Ball { mid: y.mid, rad: Mag::pow2(k) };
        let _ = Float::zero();
        Some(mc_numeric::cball::CBall::real(b))
    }
    fn machine(&self, args: &[f64]) -> mc_numeric::fun::Mach {
        mc_numeric::fun::Mach::Real(lambert_w(0, C64::real(args[0])).map_or(f64::NAN, |z| if z.im == 0.0 { z.re } else { f64::NAN }))
    }
    fn machine_complex(&self, args: &[C64]) -> Option<C64> {
        lambert_w(0, args[0])
    }
    /// Newton's iteration on complex balls from the machine value; the radius is the last correction (doubled)
    /// plus the argument's radius times |W'(z)|.
    fn ball_complex(&self, args: &[mc_numeric::cball::CBall], p: u64) -> Option<mc_numeric::cball::CBall> {
        use mc_arith::num::{Ball, Mag};
        use mc_numeric::cball::{self, CBall};
        let w = p + 32;
        let z = args.first()?;
        let zf = C64::new(Real::Ball(z.re.clone(), w).to_f64(), Real::Ball(z.im.clone(), w).to_f64());
        if !zf.re.is_finite() || !zf.im.is_finite() || cabs(zf + C64::real(1.0 / std::f64::consts::E)) < 1e-9 {
            return None;
        }
        let w0 = lambert_w(0, zf)?;
        let exact = |v: f64| -> Option<Ball> {
            match Real::Exact(mc_numeric::value::machine_rat(v)?).to_kind(mc_arith::Kind::Ball, w) {
                Real::Ball(b, _) => Some(Ball::exact(b.mid)),
                _ => None,
            }
        };
        let mut y = CBall::new(exact(w0.re)?, exact(w0.im)?);
        let zm = CBall::new(Ball::exact(z.re.mid.clone()), Ball::exact(z.im.mid.clone()));
        let one = CBall::from_i64(1);
        let mut last_l = 0.0f64;
        for _ in 0..(w as f64).log2().ceil() as usize + 4 {
            if mc_arith::num::stopped() {
                return None;
            }
            let e = cball::exp(&y, w)?;
            let f = y.mul(&e, w).sub(&zm, w);
            let fp = e.mul(&y.add(&one, w), w);
            let step = f.div(&fp, w)?;
            last_l = crate::ball::cseries::log2_up(&step);
            let ny = y.sub(&step, w);
            y = CBall::new(Ball::exact(ny.re.mid), Ball::exact(ny.im.mid));
            if last_l < -(w as f64) {
                break;
            }
        }
        let yf = C64::new(Real::Ball(y.re.clone(), w).to_f64(), Real::Ball(y.im.clone(), w).to_f64());
        let slope = (cabs(yf) / (cabs(zf) * cabs(yf + C64::real(1.0)))).max(1e-300);
        let zr = z.re.rad.log2().max(z.im.rad.log2());
        let zr = if z.re.rad.is_zero() && z.im.rad.is_zero() { f64::NEG_INFINITY } else { zr };
        let terms = [1.0 + last_l, slope.log2() + zr + 0.002, -(p as f64) - 8.0];
        let top = terms.iter().copied().filter(|t| t.is_finite()).fold(-(p as f64) - 8.0, f64::max);
        let m = Mag::pow2((top + 2.0).ceil() as i64);
        Some(CBall::new(Ball { mid: y.re.mid, rad: m }, Ball { mid: y.im.mid, rad: m }))
    }
}

/// Dawson's integral and the Fresnel integrals as library functions (balls from the error-function balls, so
/// `n` refines them): 0 dawson_f, 1 fresnel_s, 2 fresnel_c.
pub struct ErfFamily(pub u8);

/// The three functions.
pub static ERF_FAMILY: [ErfFamily; 3] = [ErfFamily(0), ErfFamily(1), ErfFamily(2)];

impl mc_numeric::library::LibraryFn for ErfFamily {
    fn name(&self) -> &'static str {
        ["dawson_f", "fresnel_s", "fresnel_c"][self.0 as usize]
    }
    fn arity(&self) -> usize {
        1
    }
    fn ball(&self, args: &[Real], p: u64) -> Option<mc_numeric::cball::CBall> {
        use mc_arith::num::Ball;
        use mc_numeric::cball::{self, CBall};
        use mc_numeric::consts::Const;
        use mc_numeric::elementary as el;
        let w = p + 32;
        let x = match args[0].to_kind(mc_arith::Kind::Ball, w) {
            Real::Ball(b, _) => b,
            _ => return None,
        };
        let sp = Const::Pi.ball(w)?.sqrt(w)?;
        if self.0 == 0 {
            // sqrt(pi)/2 e^(-x^2) erfi(x)
            let e = el::exp(&x.mul(&x, w).neg(), w)?;
            let f = crate::ball::erf::erfi(&Real::Ball(x, w), w)?;
            let half = Ball::from_ratio(1, 2, w);
            return Some(f.mul_real(&e.mul(&sp, w).mul(&half, w), w).round(p));
        }
        // (1 +- i)/4 (erf((1 + i) sqrt(pi) x/2) -+ i erf((1 - i) sqrt(pi) x/2)), real part
        let a = x.mul(&sp, w).mul(&Ball::from_ratio(1, 2, w), w);
        let up = crate::ball::complex::erf(&CBall::new(a.clone(), a.clone()), w)?;
        let dn = crate::ball::complex::erf(&CBall::new(a.clone(), a.neg()), w)?;
        let q = Ball::from_ratio(1, 4, w);
        let v = if self.0 == 1 {
            CBall::new(Ball::from_i64(1), Ball::from_i64(1)).mul(&up.sub(&dn.mul_i(), w), w)
        } else {
            CBall::new(Ball::from_i64(1), Ball::from_i64(-1)).mul(&up.add(&dn.mul_i(), w), w)
        };
        let v = v.mul_real(&q, w);
        let _ = cball::abs;
        Some(CBall::real(v.re).round(p))
    }
    fn machine(&self, args: &[f64]) -> mc_numeric::fun::Mach {
        let x = args[0];
        let sp = std::f64::consts::PI.sqrt();
        let v = if self.0 == 0 {
            0.5 * sp * (-x * x).exp() * machine::erf::erfi(x)
        } else {
            let a = 0.5 * sp * x;
            let up = machine::erf::erf_complex(C64::new(a, a));
            let dn = machine::erf::erf_complex(C64::new(a, -a));
            let i = C64::new(0.0, 1.0);
            let q = C64::real(0.25);
            let v = if self.0 == 1 {
                C64::new(1.0, 1.0) * (up - i * dn) * q
            } else {
                C64::new(1.0, -1.0) * (up + i * dn) * q
            };
            v.re
        };
        mc_numeric::fun::Mach::Real(v)
    }
    fn machine_complex(&self, args: &[C64]) -> Option<C64> {
        let z = args[0];
        let sp = std::f64::consts::PI.sqrt();
        let i = C64::new(0.0, 1.0);
        let v = if self.0 == 0 {
            // sqrt(pi)/2 e^(-z^2) erfi(z), erfi(z) = -i erf(i z)
            let erfi = C64::new(0.0, -1.0) * machine::erf::erf_complex(i * z);
            C64::real(0.5 * sp) * (C64::real(0.0) - z * z).exp() * erfi
        } else {
            // the same formulas as for real arguments, analytic in z (DLMF 7.5.5)
            let a = C64::real(0.5 * sp) * z;
            let up = machine::erf::erf_complex(C64::new(1.0, 1.0) * a);
            let dn = machine::erf::erf_complex(C64::new(1.0, -1.0) * a);
            let q = C64::real(0.25);
            if self.0 == 1 {
                C64::new(1.0, 1.0) * (up - i * dn) * q
            } else {
                C64::new(1.0, -1.0) * (up + i * dn) * q
            }
        };
        (v.re.is_finite() && v.im.is_finite()).then_some(v)
    }
}
