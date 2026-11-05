//! The polylogarithm Li_s(z) = sum_{k>=1} z^k / k^s for machine numbers, in double-double (decision SF.3).
//!
//! Real s and real z <= 1 ([`polylog`]); integer s and any complex z on the principal branch ([`polylog_complex`];
//! on the cut z > 1 the value is the limit from the lower half plane, so Li_1(2) = -i pi and
//! Li_2(2) = pi^2/4 - i pi ln 2). The regions:
//!
//! - **|z| <= 1/2** (and |z| <= 1 for s >= 120): the defining series. The term ratio is at most
//!   |z| max(1, (1 + 1/k)^-s), decreasing in k, so the tail after a term t with ratio bound r < 1 is at most
//!   t r / (1 - r); summation stops when that is below 2^-110 of the sum. k^-s is multiplicative: only primes
//!   need exp(-s log p).
//! - **Integers s = -n <= 0**: Li_-n(z) = z A_n(z) / (1 - z)^(n+1) with the Eulerian polynomial A_n (exact
//!   double-double coefficients, n <= 150); for |z| > 1 by the palindromic form (z/(1-z))^n A_n(1/z) / (1 - z).
//!   s = 1: -log(1 - z).
//! - **1/2 < z <= 1** (complex: 1/2 < |z| <= 1 with Re z >= 0), with mu = log z (|mu| < 2 pi):
//!   Li_s(e^mu) = Gamma(1 - s) (-mu)^(s-1) + sum_k zeta(s - k) mu^k / k! for non-integer s, and for integers
//!   s = n >= 2 the term k = n - 1 is mu^(n-1)/(n-1)! (H_(n-1) - log(-mu)); zeta at negative integers from exact
//!   Bernoulli numbers, at other arguments by a row of Euler-Maclaurin sums (zeta(a), zeta(a + 1), ... with shared
//!   powers) and the functional equation zeta(s - k) = 2 (2 pi)^(s-k-1) sin(pi (s-k)/2) Gamma(1-s+k) zeta(1-s+k).
//! - **z < 0 near -1** (real z in [-e^(0.9 pi), -1/2) for non-integer s, [-1, -1/2) for integers; complex
//!   1/2 < |z| <= 1 with Re z < 0), with lambda = log(-z) (|lambda| < pi): Li_s(-e^lambda) =
//!   -sum_k eta(s - k) lambda^k / k!, eta(x) = (1 - 2^(1-x)) zeta(x); analytic at lambda = 0, no singular term.
//! - **Integers n >= 1, |z| > 1** (inversion): Li_n(z) + (-1)^n Li_n(1/z) = -L^n/n! -
//!   2 sum_{k=1}^{n/2} eta(2k) L^(n-2k)/(n-2k)!, L = log(-z) (L = ln z + i pi on the cut).
//!
//! The series terms are summed until they fall below 2^-110 of the sum, so the result is within a few ulps where
//! the function is well conditioned. Known losses: non-integer s within about 2^-30 of an integer near z = 1 (the
//! singular term and a zeta pole cancel), large negative s (the series terms grow before they decay; s >= -20 is
//! measured), and zeros of Li_-n near roots of the Eulerian polynomials (absolute accuracy 2^-106 of the terms).
//! Non-integer s with z < -e^(0.9 pi) (about -16.9) gives NaN, as does z > 1 for real arguments (the value is
//! complex) and non-integer s for complex arguments.
use super::gamma::{em_coefficients, gamma_scaled};
use crate::bernoulli::dd_table;
use crate::dd::{exp, exp_scaled, log, sin_cos_pi_any, Scaled, DD, LN2, LN_2PI, PI};
use mc_arith::num::{Int, Nat, Rat};
use mc_numeric::machine::C64;
use std::ops::{Add, Div, Mul, Neg, Sub};
use std::sync::OnceLock;

/// The relative size of a negligible term (about 2^-113).
const TINY: f64 = 1e-34;
/// The largest |Li_-n| order with an Eulerian table.
const MAX_EULERIAN: u64 = 150;
/// From this order on (|z| <= 1) the defining series converges at once (2^-s per term).
const BIG_S: f64 = 120.0;
/// The largest |log(-z)| / pi of the eta expansion for z < -1 (non-integer s).
const ETA_REACH: f64 = 0.9;

// ---------------------------------------------------------------- complex double-double

/// A complex double-double re + i im.
#[derive(Clone, Copy, Debug)]
pub(crate) struct CD {
    re: DD,
    im: DD,
}

impl Add for CD {
    type Output = CD;
    fn add(self, o: CD) -> CD {
        CD { re: self.re + o.re, im: self.im + o.im }
    }
}

impl Sub for CD {
    type Output = CD;
    fn sub(self, o: CD) -> CD {
        CD { re: self.re - o.re, im: self.im - o.im }
    }
}

impl Mul for CD {
    type Output = CD;
    fn mul(self, o: CD) -> CD {
        CD { re: self.re * o.re - self.im * o.im, im: self.re * o.im + self.im * o.re }
    }
}

impl Div for CD {
    type Output = CD;
    fn div(self, o: CD) -> CD {
        let n = o.re.sqr() + o.im.sqr();
        CD { re: (self.re * o.re + self.im * o.im) / n, im: (self.im * o.re - self.re * o.im) / n }
    }
}

impl Neg for CD {
    type Output = CD;
    fn neg(self) -> CD {
        CD { re: -self.re, im: -self.im }
    }
}

impl CD {
    fn of(z: C64) -> CD {
        CD { re: DD::new(z.re), im: DD::new(z.im) }
    }
    fn to_c64(self) -> C64 {
        C64::new(fin(self.re), fin(self.im))
    }
}

/// The arithmetic the series share between real and complex double-doubles.
pub(crate) trait Num:
    Copy + Add<Output = Self> + Sub<Output = Self> + Mul<Output = Self> + Div<Output = Self> + Neg<Output = Self>
{
    /// A real value.
    fn re(x: DD) -> Self;
    /// x times a real.
    fn scale(self, x: DD) -> Self;
    /// An approximation of |x|.
    fn abs_f(self) -> f64;
    /// The principal logarithm.
    fn ln(self) -> Self;
}

impl Num for DD {
    fn re(x: DD) -> DD {
        x
    }
    fn scale(self, x: DD) -> DD {
        self * x
    }
    fn abs_f(self) -> f64 {
        self.hi.abs()
    }
    fn ln(self) -> DD {
        log(self)
    }
}

impl Num for CD {
    fn re(x: DD) -> CD {
        CD { re: x, im: DD::ZERO }
    }
    fn scale(self, x: DD) -> CD {
        CD { re: self.re * x, im: self.im * x }
    }
    fn abs_f(self) -> f64 {
        self.re.hi.hypot(self.im.hi)
    }
    fn ln(self) -> CD {
        // the argument by one Newton step on atan2: t + (y cos t - x sin t) / (x cos t + y sin t)
        let t = self.im.hi.atan2(self.re.hi);
        let (s, c) = sin_cos_pi_any(DD::new(t) / PI);
        let corr = (self.im * c - self.re * s) / (self.re * c + self.im * s);
        let arg = if corr.hi.is_finite() { DD::new(t) + corr } else { DD::new(t) };
        CD { re: log(self.re.sqr() + self.im.sqr()).mul_f(0.5), im: arg }
    }
}

/// The nearest double (an infinite leading part as itself).
fn fin(x: DD) -> f64 {
    if x.hi.is_finite() {
        x.to_f64()
    } else {
        x.hi
    }
}

/// A scaled double-double as a double-double (zero below the double range).
fn sd(s: Scaled) -> DD {
    if !s.m.hi.is_finite() {
        return s.m;
    }
    if s.e > 1100 {
        return DD::new(f64::INFINITY * s.m.hi.signum());
    }
    s.m.ldexp(s.e.max(-1200) as i32)
}

/// The integer value of s, if it is one.
fn int_of(s: f64) -> Option<i64> {
    (s == s.trunc() && s.abs() < 1e15).then_some(s as i64)
}

// ---------------------------------------------------------------- tables

/// B_2j / (2j)! for j = 1..=160.
fn bern_fact() -> &'static [DD] {
    static T: OnceLock<Vec<DD>> = OnceLock::new();
    dd_table(&T, 160, |k, b| {
        let mut f = Nat::from_u64(1);
        for i in 2..=2 * k {
            f = f.mul_u64(i);
        }
        b.mul(&Rat::new(Int::from_i64(1), Int::from_nat(f)))
    })
}

/// zeta(k) for k = 2..=121 (index k - 2), from one Euler-Maclaurin row.
fn zeta_table() -> &'static [DD] {
    static T: OnceLock<Vec<DD>> = OnceLock::new();
    T.get_or_init(|| {
        let mut row = ZetaRow::new(DD::new(2.0));
        (0..120).map(|_| row.next()).collect()
    })
}

/// zeta(k) for an integer k >= 2.
fn zeta_int(k: u64) -> DD {
    let t = zeta_table();
    if k - 2 < t.len() as u64 {
        t[(k - 2) as usize]
    } else {
        // 3^-k < 2^-190: 1 + 2^-k is exact to the double-double precision
        DD::sum(1.0, crate::dd::pow2(-(k.min(5000) as i32)))
    }
}

/// The Eulerian numbers A(n, k), k < n, for n <= 150 (exact while below 2^106).
fn eulerian_rows() -> &'static [Vec<DD>] {
    static T: OnceLock<Vec<Vec<DD>>> = OnceLock::new();
    T.get_or_init(|| {
        let mut rows: Vec<Vec<DD>> = vec![vec![], vec![DD::ONE]];
        for n in 2..=MAX_EULERIAN as usize {
            let prev = &rows[n - 1];
            let mut row = vec![DD::ZERO; n];
            for (k, r) in row.iter_mut().enumerate() {
                // A(n, k) = (k + 1) A(n-1, k) + (n - k) A(n-1, k-1)
                let a = if k < n - 1 { prev[k].mul_f((k + 1) as f64) } else { DD::ZERO };
                let b = if k > 0 { prev[k - 1].mul_f((n - k) as f64) } else { DD::ZERO };
                *r = a + b;
            }
            rows.push(row);
        }
        rows
    })
}

// ---------------------------------------------------------------- zeta rows

/// log2 of the Euler-Maclaurin remainder bound with 16 correction terms (as in [`super::zeta`]).
fn bound_log2(s: f64, n: f64) -> f64 {
    let m = 16.0;
    let mut l = 2.0;
    for i in 0..32 {
        l += (s + i as f64).abs().max(1e-300).log2();
    }
    l -= 2.0 * m * (2.0 * std::f64::consts::PI).log2();
    l += (1.0 - s - 2.0 * m) * n.log2();
    l - (s + 2.0 * m - 1.0).abs().max(1e-300).log2()
}

/// zeta(a), zeta(a + 1), zeta(a + 2), ... for a double-double a > 0 that is not an integer <= 1, by Euler-Maclaurin
/// sums sharing their powers n^-t (each step multiplies them by 1/n).
struct ZetaRow {
    t: DD,
    pw: Vec<DD>,
    inv: Vec<DD>,
    log2n: f64,
}

impl ZetaRow {
    fn new(a: DD) -> ZetaRow {
        let mut n = 16usize;
        while bound_log2(a.hi, n as f64) > -112.0 && n < 4096 {
            n += 4;
        }
        let mut pw = vec![DD::ONE; n + 1];
        let mut inv = vec![DD::ONE; n + 1];
        for k in 2..=n {
            pw[k] = exp(-(log(DD::new(k as f64)) * a));
            inv[k] = DD::new(k as f64).recip();
        }
        ZetaRow { t: a, pw, inv, log2n: (n as f64).log2() }
    }

    /// zeta(t) for the current t; then t + 1.
    fn next(&mut self) -> DD {
        let t = self.t;
        if (t.hi - 1.0) * self.log2n >= 120.0 {
            // the direct sum: drop the powers below 2^-120 (they only shrink)
            let mut n = self.pw.len() - 1;
            while n > 2 && (t.hi - 1.0) * ((n - 1) as f64).log2() >= 120.0 {
                n -= 1;
            }
            self.pw.truncate(n + 1);
            self.log2n = (n as f64).log2();
        }
        let n = self.pw.len() - 1;
        let mut sum = DD::ZERO;
        for k in (1..n).rev() {
            sum = sum + self.pw[k];
        }
        if (t.hi - 1.0) * self.log2n < 120.0 {
            let nn = self.pw[n];
            let nb = DD::new(n as f64);
            sum = sum + nb * nn / t.add_f(-1.0) + nn.mul_f(0.5);
            let r2 = self.inv[n].sqr();
            let mut poch = t;
            let mut w = nn * self.inv[n];
            for (i, c) in em_coefficients().iter().take(16).enumerate() {
                let term = *c * poch * w;
                sum = sum + term;
                if term.hi.abs() < TINY * sum.hi.abs() {
                    break;
                }
                let j = 2.0 * (i as f64 + 1.0);
                poch = poch * t.add_f(j - 1.0) * t.add_f(j);
                w = w * r2;
            }
        }
        for k in 2..=n {
            self.pw[k] = self.pw[k] * self.inv[k];
        }
        self.t = t.add_f(1.0);
        sum
    }
}

// ---------------------------------------------------------------- the defining series

/// k^-s for k = 1, 2, ..., by multiplicativity (exp(-s log p) at primes p only).
struct Powers {
    s: f64,
    s_int: Option<i64>,
    v: Vec<DD>,
}

impl Powers {
    fn new(s: f64) -> Powers {
        Powers { s, s_int: int_of(s).filter(|n| (0..=40).contains(n)), v: vec![DD::ZERO, DD::ONE] }
    }
    fn get(&mut self, k: usize) -> DD {
        while self.v.len() <= k {
            let m = self.v.len();
            let p = (2..).take_while(|d| d * d <= m).find(|d| m.is_multiple_of(*d));
            let x = match p {
                Some(d) => self.v[d] * self.v[m / d],
                None => match self.s_int {
                    Some(n) if (n as f64) * (m as f64).log2() < 900.0 => DD::new(m as f64).powi(n as u64).recip(),
                    _ => exp(-(log(DD::new(m as f64)).mul_f(self.s))),
                },
            };
            self.v.push(x);
        }
        self.v[k]
    }
}

/// sum_{k>=1} z^k k^-s for |z| <= 1 where it converges (`None` past 20000 terms).
fn direct<T: Num>(s: f64, z: T) -> Option<T> {
    let za = z.abs_f();
    let mut pw = Powers::new(s);
    let mut zk = z;
    let mut sum = z;
    for k in 2..20000usize {
        zk = zk * z;
        let t = zk.scale(pw.get(k));
        sum = sum + t;
        let kf = k as f64;
        let r = za * if s < 0.0 { (1.0 + 1.0 / kf).powf(-s) } else { 1.0 };
        if r < 1.0 && t.abs_f() * r / (1.0 - r) <= TINY * sum.abs_f() * 4.0 || t.abs_f() == 0.0 && r < 1.0 {
            return Some(sum);
        }
    }
    None
}

// ---------------------------------------------------------------- integer orders

/// Li_-n(z), n >= 0, as the rational function.
fn eulerian<T: Num>(n: u64, z: T) -> T {
    let one = T::re(DD::ONE);
    let omz = one - z;
    if n == 0 {
        return z / omz;
    }
    let row = &eulerian_rows()[n as usize];
    let horner = |x: T| row.iter().rev().fold(T::re(DD::ZERO), |acc, c| acc * x + T::re(*c));
    let w = z / omz;
    if z.abs_f() <= 1.0 {
        // z A(z) / (1 - z)^(n+1) = w A(z) / (1 - z)^n
        let mut p = w * horner(z);
        let r = one / omz;
        for _ in 0..n {
            p = p * r;
        }
        p
    } else {
        let mut p = horner(one / z) / omz;
        for _ in 0..n {
            p = p * w;
        }
        p
    }
}

/// H_m = 1 + 1/2 + ... + 1/m.
fn harmonic(m: u64) -> DD {
    (1..=m).fold(DD::ZERO, |h, i| h + DD::new(i as f64).recip())
}

/// 1/m!.
fn inv_fact(m: u64) -> DD {
    (2..=m).fold(DD::ONE, |f, i| f.div_f(i as f64))
}

/// Li_n(e^mu) for an integer n >= 2 and |mu| < 2 pi (mu not 0 and not on the positive real axis).
fn mu_int<T: Num>(n: u64, mu: T) -> T {
    let mut sum = T::re(DD::ZERO);
    let mut pk = T::re(DD::ONE); // mu^k / k!
    for k in 0..n - 1 {
        sum = sum + pk.scale(zeta_int(n - k));
        pk = (pk * mu).scale(DD::new((k + 1) as f64).recip());
    }
    sum = sum + pk * (T::re(harmonic(n - 1)) - (-mu).ln());
    pk = (pk * mu).scale(DD::new(n as f64).recip());
    sum = sum - pk.scale(DD::new(0.5));
    // k = n - 1 + 2j: zeta(1 - 2j) / k! = -B_2j/(2j)! (2j-1)!/k!
    let mu2 = mu * mu;
    let mut pw = (0..n + 1).fold(T::re(DD::ONE), |p, _| p * mu);
    let mut r = inv_fact(n + 1);
    for (j, b) in bern_fact().iter().enumerate() {
        let j = j as u64 + 1;
        let term = pw.scale(-(*b * r));
        sum = sum + term;
        if term.abs_f() <= TINY * sum.abs_f() && j > 1 {
            break;
        }
        pw = pw * mu2;
        r = r.mul_f((2 * j * (2 * j + 1)) as f64).div_f(((n + 2 * j) * (n + 2 * j + 1)) as f64);
    }
    sum
}

/// Li_n(-e^lambda) for an integer n >= 1 and |lambda| < pi: -sum_k eta(n - k) lambda^k / k!.
fn eta_int<T: Num>(n: u64, lam: T) -> T {
    let mut sum = T::re(DD::ZERO);
    let mut pk = T::re(DD::ONE);
    for k in 0..n - 1 {
        let e = DD::sum(1.0, -crate::dd::pow2(-((n - 1 - k).min(2000) as i32)));
        sum = sum + pk.scale(e * zeta_int(n - k));
        pk = (pk * lam).scale(DD::new((k + 1) as f64).recip());
    }
    sum = sum + pk.scale(LN2);
    pk = (pk * lam).scale(DD::new(n as f64).recip());
    sum = sum + pk.scale(DD::new(0.5));
    // k = n - 1 + 2j: eta(1 - 2j) = (2^2j - 1) B_2j / (2j)
    let l2 = lam * lam;
    let mut pw = (0..n + 1).fold(T::re(DD::ONE), |p, _| p * lam);
    let mut r = inv_fact(n + 1);
    for (j, b) in bern_fact().iter().enumerate() {
        let j = j as u64 + 1;
        let c = DD::sum(crate::dd::pow2(2 * j as i32), -1.0) * *b * r;
        let term = pw.scale(c);
        sum = sum + term;
        if term.abs_f() <= TINY * sum.abs_f() && j > 1 {
            break;
        }
        pw = pw * l2;
        r = r.mul_f((2 * j * (2 * j + 1)) as f64).div_f(((n + 2 * j) * (n + 2 * j + 1)) as f64);
    }
    -sum
}

/// The inversion term -L^n/n! - 2 sum_{k=1}^{n/2} eta(2k) L^(n-2k)/(n-2k)!.
fn inversion_term<T: Num>(n: u64, l: T) -> T {
    // Horner in L^2 from the top power down: sum_k c_k L^(n-2k)
    let mut sum = T::re(DD::ZERO);
    let mut pw = T::re(DD::ONE); // L^(n mod 2 + 2i)
    let base = n % 2;
    if base == 1 {
        pw = l;
    }
    let l2 = l * l;
    let mut m = base;
    while m <= n {
        let k = (n - m) / 2;
        let c = if k == 0 {
            DD::ONE
        } else {
            let e = DD::sum(1.0, -crate::dd::pow2(1 - 2 * k.min(1000) as i32));
            (e * zeta_int(2 * k)).mul_f(2.0)
        };
        sum = sum + pw.scale(c * inv_fact(m));
        pw = pw * l2;
        m += 2;
    }
    -sum
}

// ---------------------------------------------------------------- non-integer orders

/// The coefficients zeta(s - k) / k! for non-integer s, k = 0, 1, 2, ...: rows of Euler-Maclaurin sums for the
/// positive arguments, the functional equation for the negative ones.
struct ZetaCoeffs {
    s: f64,
    k: u64,
    k0: u64,
    pos: Vec<DD>,
    neg: Option<ZetaRow>,
    /// Gamma(1-s+k) (2 pi)^(s-k-1) / k! for k >= k0
    g: DD,
    sin_cos: (DD, DD),
}

impl ZetaCoeffs {
    /// `cap` the number of positive-argument coefficients worth computing.
    fn new(s: f64, cap: u64) -> ZetaCoeffs {
        let k0 = if s > 0.0 { s.floor() as u64 + 1 } else { 0 };
        let m = k0.min(cap);
        let mut pos = Vec::new();
        if m > 0 {
            let mut row = ZetaRow::new(DD::sum(s, -((m - 1) as f64)));
            let vals: Vec<DD> = (0..m).map(|_| row.next()).collect();
            // vals[i] = zeta(s - m + 1 + i); coefficient k = m - 1 - i
            let mut f = DD::ONE;
            for k in 0..m {
                pos.push(vals[(m - 1 - k) as usize] * f);
                f = f.div_f((k + 1) as f64);
            }
        }
        ZetaCoeffs { s, k: 0, k0, pos, neg: None, g: DD::ZERO, sin_cos: sin_cos_pi_any(DD::new(s * 0.5)) }
    }

    /// The next coefficient zeta(s - k)/k! and an upper bound of its size ignoring the sine factor (`None` when
    /// the positive part was capped and is exhausted).
    fn next(&mut self) -> Option<(DD, f64)> {
        let k = self.k;
        self.k += 1;
        if k < self.k0 {
            // beyond the cap the terms are negligible (mu^k / k! with |mu| < pi, k >= 80)
            let c = self.pos.get(k as usize).copied().unwrap_or(DD::ZERO);
            return Some((c, c.hi.abs()));
        }
        if self.neg.is_none() {
            let a = DD::sum(1.0 + k as f64, -self.s);
            self.neg = Some(ZetaRow::new(a));
            let gam = gamma_scaled(a);
            let pw = exp_scaled(DD::sum(self.s, -(k as f64) - 1.0) * LN_2PI);
            let f = gamma_scaled(DD::new(k as f64 + 1.0));
            self.g = sd(gam * pw / f);
        } else {
            // G_k = G_(k-1) (k - s) / (2 pi k)
            self.g = self.g * DD::sum(k as f64, -self.s) / (PI.mul_f(2.0 * k as f64));
        }
        let z = self.neg.as_mut()?.next();
        // sin(pi (s - k)/2) by k mod 4
        let (sn, cs) = self.sin_cos;
        let sf = match k % 4 {
            0 => sn,
            1 => -cs,
            2 => -sn,
            _ => cs,
        };
        let mag = self.g * z;
        Some((mag * sf.mul_f(2.0), mag.hi.abs() * 2.0))
    }
}

/// Li_s(e^mu) for non-integer s and real mu in [-ln 2, 0).
fn mu_frac(s: f64, mu: DD) -> DD {
    let gam = gamma_scaled(DD::sum(1.0, -s));
    let sing = sd(gam * exp_scaled(log(-mu).mul_f(s - 1.0)));
    let mut cs = ZetaCoeffs::new(s, 80);
    let mut sum = DD::ZERO;
    let mut pk = DD::ONE;
    for k in 0..4000u64 {
        let Some((c, mag)) = cs.next() else { return DD::new(f64::NAN) };
        sum = sum + c * pk;
        if k >= cs.k0.min(80) && mag * pk.hi.abs() <= TINY * (sum + sing).hi.abs() && k > 2 {
            return sum + sing;
        }
        pk = pk * mu;
    }
    DD::new(f64::NAN)
}

/// Li_s(-e^lambda) for non-integer s and real |lambda| < pi.
fn eta_frac(s: f64, lam: DD) -> DD {
    let q = exp(LN2.mul_f(1.0 - s)); // 2^(1-s)
    let mut cs = ZetaCoeffs::new(s, 160);
    let mut sum = DD::ZERO;
    let mut pk = DD::ONE;
    for k in 0..20000u64 {
        let Some((c, mag)) = cs.next() else { return DD::new(f64::NAN) };
        let two = q.ldexp(k.min(5000) as i32);
        let e = DD::ONE - two;
        sum = sum + c * e * pk;
        let bound = mag * e.hi.abs() * pk.hi.abs();
        if k >= cs.k0.min(160) && bound <= TINY * sum.hi.abs() && k > 2 {
            return -sum;
        }
        pk = pk * lam;
    }
    DD::new(f64::NAN)
}

// ---------------------------------------------------------------- entry points

/// Li_n(z) for an integer n >= 2 and a real double-double z <= 1, z != 1.
fn int_real(n: u64, z: DD) -> DD {
    let za = z.hi.abs();
    if za <= 0.5 || (n as f64 >= BIG_S && za <= 1.0) {
        return direct(n as f64, z).unwrap_or(DD::new(f64::NAN));
    }
    if z.hi > 0.0 {
        return mu_int(n, log(z));
    }
    if za <= 1.0 {
        return eta_int(n, log(-z));
    }
    // z < -1: inversion with L = ln(-z), 1/z in (-1, 0)
    let r = int_real(n, z.recip());
    let r = if n.is_multiple_of(2) { -r } else { r };
    r + inversion_term(n, log(-z))
}

/// Li_s(z) for real s and real z <= 1 (NaN for z > 1, where the value is complex). Li_s(1) = zeta(s) for s > 1
/// and +inf for s <= 1 (the limit from below).
pub fn polylog(s: f64, z: f64) -> f64 {
    if s.is_nan() || z.is_nan() || z > 1.0 || s.is_infinite() {
        return f64::NAN;
    }
    if z == 0.0 {
        return z;
    }
    if z == 1.0 {
        return if s > 1.0 { super::zeta::zeta(s) } else { f64::INFINITY };
    }
    if z == f64::NEG_INFINITY {
        return match int_of(s) {
            Some(n) if n >= 1 => f64::NEG_INFINITY,
            Some(0) => -1.0,
            Some(_) => 0.0,
            None => f64::NAN,
        };
    }
    let zd = DD::new(z);
    if let Some(n) = int_of(s) {
        if n <= 0 {
            let m = n.unsigned_abs();
            return if m <= MAX_EULERIAN { fin(eulerian(m, zd)) } else { f64::NAN };
        }
        if n == 1 {
            if z.abs() <= 0.5 {
                return fin(direct(1.0, zd).unwrap_or(DD::new(f64::NAN)));
            }
            return -fin(log(DD::sum(1.0, -z)));
        }
        return fin(int_real(n as u64, zd));
    }
    if z.abs() <= 0.5 || (s >= BIG_S && z.abs() <= 1.0) {
        return fin(direct(s, zd).unwrap_or(DD::new(f64::NAN)));
    }
    if z > 0.0 {
        return fin(mu_frac(s, log(zd)));
    }
    let lam = (-z).ln();
    if lam > ETA_REACH * std::f64::consts::PI {
        return f64::NAN;
    }
    fin(eta_frac(s, log(-zd)))
}

/// Li_n(z) for an integer n and complex z, |z| <= 1 or by inversion.
fn int_complex(n: i64, z: CD, on_cut: bool) -> CD {
    let za = z.abs_f();
    if n <= 0 {
        return eulerian(n.unsigned_abs(), z);
    }
    let n = n as u64;
    if za <= 0.5 || (n as f64 >= BIG_S && za <= 1.0) {
        return direct(n as f64, z).unwrap_or(CD::re(DD::new(f64::NAN)));
    }
    if n == 1 {
        // -log(1 - z); on the cut 1 - z = -(x - 1) + i0 from below: log = ln(x - 1) + i pi
        let w = CD::re(DD::ONE) - z;
        let l = if on_cut { CD { re: log(-w.re), im: PI } } else { w.ln() };
        return -l;
    }
    if za <= 1.0 {
        return if z.re.hi >= 0.0 { mu_int(n, z.ln()) } else { eta_int(n, (-z).ln()) };
    }
    let w = -z;
    let l = if on_cut { CD { re: log(-w.re), im: PI } } else { w.ln() };
    let zi = CD::re(DD::ONE) / z;
    let r = int_complex(n as i64, zi, false);
    let r = if n.is_multiple_of(2) { -r } else { r };
    r + inversion_term(n, l)
}

/// Li_s(z) for complex z: integer s on the principal branch (the cut z > 1 continuous with the lower half plane);
/// real z <= 1 for any real s as [`polylog`]; NaN otherwise.
pub fn polylog_complex(s: f64, z: C64) -> C64 {
    if z.im == 0.0 && z.re <= 1.0 {
        return C64::real(polylog(s, z.re));
    }
    let Some(n) = int_of(s) else { return C64::new(f64::NAN, f64::NAN) };
    if !(z.re.is_finite() && z.im.is_finite()) || n < -(MAX_EULERIAN as i64) {
        return C64::new(f64::NAN, f64::NAN);
    }
    int_complex(n, CD::of(z), z.im == 0.0).to_c64()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ulps(got: f64, want: f64) -> f64 {
        ((got - want) / want).abs() / f64::EPSILON
    }

    #[test]
    fn known_values() {
        let pi2 = std::f64::consts::PI.powi(2);
        let l2 = std::f64::consts::LN_2;
        assert!(ulps(polylog(2.0, 0.5), pi2 / 12.0 - l2 * l2 / 2.0) <= 2.0);
        assert!(ulps(polylog(2.0, -1.0), -pi2 / 12.0) <= 2.0);
        assert!(ulps(polylog(2.0, 1.0), pi2 / 6.0) <= 1.0);
        assert!(ulps(polylog(1.0, 0.75), 4f64.ln()) <= 2.0);
        assert!(ulps(polylog(0.0, -3.0), -0.75) <= 1.0);
        assert!(ulps(polylog(-1.0, 0.5), 2.0) <= 1.0);
        // Li_2(3/4) = 0.97846939293030610374..., Li_2(-2) = -1.43674636688368094636...
        assert!(ulps(polylog(2.0, 0.75), 0.978_469_392_930_306_1) <= 2.0);
        assert!(ulps(polylog(2.0, -2.0), -1.436_746_366_883_681) <= 2.0);
        assert!(polylog(2.0, 1.5).is_nan());
        assert_eq!(polylog(0.5, 1.0), f64::INFINITY);
        let c = polylog_complex(2.0, C64::new(2.0, 0.0));
        assert!(ulps(c.re, 2.467_401_100_272_339_7) <= 2.0 && ulps(c.im, -2.177_586_090_303_602) <= 2.0);
        let c = polylog_complex(1.0, C64::new(2.0, 0.0));
        assert!(c.re.abs() < 1e-15 && ulps(c.im, -std::f64::consts::PI) <= 1.0);
    }

    /// (s, z, Li_s(z)) with complex z and value as (re, im) pairs.
    type Case = (f64, (f64, f64), (f64, f64));

    #[test]
    #[allow(clippy::excessive_precision)] // the published digits
    fn complex_values() {
        // published values (20 digits)
        let cases: [Case; 10] = [
            (2.0, (0.3, 0.4), (0.266_596_866_742_740_43, 0.461_362_891_819_108_97)),
            (3.0, (-2.0, 1.0), (-1.709_763_874_368_360_2, 0.713_429_660_323_631_1)),
            (2.0, (0.0, 0.9), (-0.171_779_437_865_801_5, 0.835_988_285_725_505)),
            (4.0, (5.0, -3.0), (3.914_444_215_105_479, -4.838_544_976_258_129_6)),
            (1.0, (1.5, 0.2), (0.618_937_178_000_808_7, 2.761_086_276_477_428_4)),
            (-2.0, (2.0, 1.0), (0.0, 2.5)),
            (2.0, (3.0, 0.0), (2.320_180_423_313_098_4, -3.451_392_295_223_202_7)),
            (3.0, (1.5, 0.0), (2.060_877_507_320_280_8, -0.258_241_985_293_288_2)),
            (5.0, (-0.7, 0.7), (-0.697_919_982_143_071_6, 0.672_069_883_678_088_1)),
            (2.0, (0.5, 0.85), (0.282_534_255_083_687_8, 1.000_340_595_345_188_4)),
        ];
        for (s, (zr, zi), (wr, wi)) in cases {
            let v = polylog_complex(s, C64::new(zr, zi));
            let m = wr.hypot(wi);
            let err = (v.re - wr).hypot(v.im - wi) / m / f64::EPSILON;
            assert!(err <= 4.0, "Li_{s}({zr} + {zi} i) = {v:?}: {err} ulps");
        }
        // Li_3(-30) = -12.185531208679379850 (inversion, real)
        assert!(ulps(polylog(3.0, -30.0), -12.185_531_208_679_38) <= 2.0);
        assert!(polylog_complex(2.5, C64::new(0.3, 0.1)).re.is_nan());
    }
}
