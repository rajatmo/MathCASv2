//! More special functions defined by formulas (`mc_numeric::formula`, package p6-numeric): combinatorial
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
    fx.int_of(t).filter(|v| v.abs() <= 100_000)
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

/// The polynomial families of one argument at an integer degree, or their hypergeometric forms at numeric degrees.
fn family<'h>(fx: &mut Fx<'_, '_, '_, 'h>, f: Family, a: &[T<'h>]) -> R<'h> {
    let (n, x) = (a[0], a[1]);
    if let Some(k) = small_nat(fx, n) {
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
    let half_1mx = fx.mul(&[fx.rat(1, 2), fx.sub(fx.int(1), x)]);
    Ok(Some(match f {
        Family::ChebyshevT => fx.call("cos", &[fx.mul(&[n, fx.call("acos", &[x])])]),
        Family::ChebyshevU => {
            let t = fx.call("acos", &[x]);
            fx.div(fx.call("sin", &[fx.mul(&[fx.add(&[n, fx.int(1)]), t])]), fx.call("sin", &[t]))
        }
        Family::Legendre => fx.call("hypergeometric_2f1", &[fx.neg(n), fx.add(&[n, fx.int(1)]), fx.int(1), half_1mx]),
        Family::Laguerre => fx.call("hypergeometric_1f1", &[fx.neg(n), fx.int(1), x]),
        Family::Hermite => {
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
    let (l, m, x) = (a[0], a[1], a[2]);
    let (Some(l), Some(m)) = (small_nat(fx, l), small_int(fx, m)) else { return Ok(None) };
    let am = m.unsigned_abs();
    if am > l {
        return Ok(Some(fx.int(0)));
    }
    let c = derivative(&coefficients(Family::Legendre, l), am);
    let w = fx.pow(fx.sub(fx.int(1), fx.pow(x, fx.int(2))), fx.rat(am as i64, 2));
    let sign = if am % 2 == 1 { -1 } else { 1 };
    let mut v = fx.mul(&[fx.int(sign), w, poly_at(fx, &c, x)]);
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

fn laguerre_l<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    if a.len() == 2 {
        return family(fx, Family::Laguerre, a);
    }
    let (n, al, x) = (a[0], a[1], a[2]);
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
    let k = fx.mul(&[fx.sqrt(fx.q(&ratio)), fx.pow(fx.sym("pi"), fx.rat(-1, 2))]);
    let c = derivative(&coefficients(Family::Legendre, l), am);
    let cos = fx.call("cos", &[th]);
    let sign = if am % 2 == 1 && m > 0 { -1 } else { 1 };
    let e = fx.call("exp", &[fx.mul(&[fx.i(), fx.int(m), ph])]);
    let s = fx.pow(fx.call("sin", &[th]), fx.int(am as i64));
    Ok(Some(fx.mul(&[fx.int(sign), k, s, poly_at(fx, &c, cos), e])))
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
        let Some(k) = small_nat(fx, n) else { return Ok(None) };
        return Ok(Some(poly_at(fx, &fib_poly(k, false), a[1])));
    }
    if let Some(k) = fx.int_of(n).filter(|k| k.abs() <= 10_000_000) {
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
    if let Some(k) = fx.int_of(n).filter(|k| k.abs() <= 10_000_000) {
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
    if let Some(k) = small_int(fx, n).filter(|k| k.abs() <= 10_000) {
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
    let Some(k) = small_nat(fx, a[0]) else { return Ok(None) };
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

fn harmonic_number<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    let n = a[0];
    let r = if a.len() == 2 { Some(a[1]) } else { None };
    if let Some(k) = small_nat(fx, n) {
        match r.map(|r| fx.int_of(r)) {
            None => {
                let mut s = Rat::from_i64(0);
                for j in 1..=k {
                    s = s.add(&qi(1, j as i64));
                    if j.is_multiple_of(256) {
                        fx.tick(1)?;
                    }
                }
                return Ok(Some(fx.q(&s)));
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
    let Some(k) = small_nat(fx, a[0]).filter(|k| *k <= 2000) else { return Ok(None) };
    let mut p = Int::from_i64(1);
    for j in 1..=k {
        p = p.mul(&Int::from_u64(j).pow_u64_int(j));
        fx.tick(1)?;
    }
    Ok(Some(fx.integer(&p)))
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
    } else if k == 0 && cabs(z) < 3.0 {
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

fn product_log_n<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    let (k, z) = if a.len() == 2 { (fx.int_of(a[0]), a[1]) } else { (Some(0), a[0]) };
    let Some(k) = k else { return Ok(None) };
    let zc = match fx.num(z) {
        Some(Num::Real(r)) => C64::real(r.to_f64()),
        Some(Num::Complex(x, y)) => C64::new(x.to_f64(), y.to_f64()),
        None => return Ok(None),
    };
    // only machine precision (balls stay)
    let machine = matches!(fx.num(z), Some(Num::Real(Real::Machine(_))) | Some(Num::Complex(Real::Machine(_), _)));
    if !machine {
        return Ok(None);
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
    let sp = fx.sqrt(fx.sym("pi"));
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

/// The spherical Bessel functions at integer order n >= 0 by the recurrences from j_0, j_1 (y_0, y_1), elementary
/// in sin and cos; numerically sqrt(pi/(2 z)) J_{n+1/2}(z) for other orders.
fn spherical<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>], first: bool) -> R<'h> {
    let (n, z) = (a[0], a[1]);
    if let Some(k) = small_nat(fx, n).filter(|k| *k <= 60) {
        if fx.is_int(z, 0) {
            return Ok(if first { Some(fx.int(if k == 0 { 1 } else { 0 })) } else { None });
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
    let f = if first { "bessel_j" } else { "bessel_y" };
    let b = fx.call(f, &[fx.add(&[n, fx.rat(1, 2)]), z]);
    Ok(Some(fx.mul(&[fx.sqrt(fx.div(fx.sym("pi"), fx.mul(&[fx.int(2), z]))), b])))
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
    let j = fx.call("bessel_j", a);
    let y = fx.call("bessel_y", a);
    Ok(Some(fx.add(&[j, fx.mul(&[fx.ar.complex(fx.int(0), fx.int(s)), y])])))
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
    // q = exp(-pi K(1 - m)/K(m))
    let k1 = fx.call("elliptic_k", &[fx.sub(fx.int(1), m)]);
    let k = fx.call("elliptic_k", &[m]);
    Ok(Some(fx.call("exp", &[fx.neg(fx.mul(&[fx.sym("pi"), fx.div(k1, k)]))])))
}

/// K_nu(x) e^x (I_nu(x) e^-x): machine reals by the package's scaled algorithms (no underflow at large x), the
/// product form otherwise.
fn scaled<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>], k: bool) -> R<'h> {
    let (nu, x) = (a[0], a[1]);
    if let (Some(n), Some(v)) = (fx.f64_of(nu), fx.machine_of(x).or_else(|| fx.machine_of(nu).and(fx.f64_of(x)))) {
        let r = if k { machine::bessel::bessel_k_scaled(n, v) } else { machine::bessel::bessel_i_scaled(n, v) };
        if !r.is_nan() {
            return Ok(Some(fx.real(r)));
        }
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
    def!("multinomial", &[0, 1, 2, 3, 4, 5, 6, 7, 8], Some(multinomial), None, "multinomial(n1, n2, ...): the multinomial coefficient (n1 + n2 + ...)!/(n1! n2! ...)"),
    def!("harmonic_number", &[1, 2], Some(harmonic_number), None, "harmonic_number(n): the harmonic number H_n = 1 + 1/2 + ... + 1/n; harmonic_number(n, r): the sum of 1/k^r"),
    def!("hyperfactorial", &[1], Some(hyperfactorial), None, "hyperfactorial(n): the product 1^1 2^2 ... n^n"),
    def!("chebyshev_t", &[2], Some(chebyshev_t), None, "chebyshev_t(n, x): the Chebyshev polynomial of the first kind T_n(x) = cos(n acos(x))"),
    def!("chebyshev_u", &[2], Some(chebyshev_u), None, "chebyshev_u(n, x): the Chebyshev polynomial of the second kind U_n(x)"),
    def!("legendre_p", &[2, 3], Some(legendre_p), None, "legendre_p(n, x): the Legendre polynomial P_n(x); legendre_p(n, m, x): the associated Legendre function P_n^m(x)"),
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
