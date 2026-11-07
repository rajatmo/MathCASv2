//! Complex machine values of the special functions (package p6-numeric, decision P6N.9): plain double precision
//! (relative accuracy about 1e-13 in the tested ranges, not the 0.5 ulp of the real paths).
//!
//! - log gamma: Stirling's series (Bernoulli terms to B_30) after shifting to Re z >= 16, with the shift as a sum of
//!   principal logarithms, which is the analytic log gamma (log Gamma(z + 1) = log Gamma(z) + log z); gamma as its
//!   exponential, digamma by the shifted asymptotic series;
//! - Hurwitz zeta (and zeta, polygamma of integer order) by Euler-Maclaurin summation with N = 15 + |s| terms and
//!   twelve Bernoulli corrections; zeta at Re s < 1/2 by the functional equation;
//! - power series (with Kummer's transformation for Re z < 0 in 1F1) for 0F1, 1F1, |z| < 0.9 for 2F1, the
//!   exponential, sine and cosine integrals for |z| <= 40, Bessel J and I for |z| <= 25, Airy functions for
//!   |z| <= 8; Y and K for non-integer orders from J and I;
//! - Carlson's symmetric integrals R_F, R_D by duplication for the complete and incomplete elliptic integrals.
//!
//! Every function returns `None` outside the ranges where these methods are accurate (the call stays unevaluated).
use mc_numeric::machine::C64;

fn c(re: f64, im: f64) -> C64 {
    C64::new(re, im)
}
fn r(x: f64) -> C64 {
    C64::real(x)
}
fn abs(z: C64) -> f64 {
    z.re.hypot(z.im)
}
fn scale(z: C64, s: f64) -> C64 {
    c(z.re * s, z.im * s)
}
fn neg(z: C64) -> C64 {
    c(-z.re, -z.im)
}
/// z^w = exp(w log z)
fn powc(z: C64, w: C64) -> C64 {
    if z.re == 0.0 && z.im == 0.0 {
        return r(0.0);
    }
    (w * z.log()).exp()
}
fn finite(z: C64) -> Option<C64> {
    (z.re.is_finite() && z.im.is_finite()).then_some(z)
}

/// B_2k/(2k (2k - 1)) for k = 1..15 (Stirling's series).
const STIRLING: [f64; 15] = [
    1.0 / 12.0,
    -1.0 / 360.0,
    1.0 / 1260.0,
    -1.0 / 1680.0,
    1.0 / 1188.0,
    -691.0 / 360360.0,
    1.0 / 156.0,
    -3617.0 / 122400.0,
    43867.0 / 244188.0,
    -174611.0 / 125400.0,
    77683.0 / 5796.0,
    -236364091.0 / 1506960.0,
    657931.0 / 300.0,
    -3392780147.0 / 93960.0,
    1723168255201.0 / 2492028.0,
];

/// B_2k for k = 1..12.
const BERNOULLI: [f64; 12] = [
    1.0 / 6.0,
    -1.0 / 30.0,
    1.0 / 42.0,
    -1.0 / 30.0,
    5.0 / 66.0,
    -691.0 / 2730.0,
    7.0 / 6.0,
    -3617.0 / 510.0,
    43867.0 / 798.0,
    -174611.0 / 330.0,
    854513.0 / 138.0,
    -236364091.0 / 2730.0,
];

fn is_pole(z: C64) -> bool {
    z.im == 0.0 && z.re <= 0.0 && z.re == z.re.floor()
}

/// The analytic log gamma (principal at real z > 0).
pub fn log_gamma(z: C64) -> Option<C64> {
    if is_pole(z) || abs(z) > 1e15 {
        return None;
    }
    let mut shift = r(0.0);
    let mut w = z;
    let mut n = 0;
    while w.re < 16.0 {
        shift = shift + w.log();
        w = w + r(1.0);
        n += 1;
        if n > 200_000 {
            return None;
        }
    }
    // (w - 1/2) log w - w + log(2 pi)/2 + sum B_2k/(2k(2k-1) w^(2k-1))
    let lw = w.log();
    let mut s = (w - r(0.5)) * lw - w + r(0.918_938_533_204_672_8);
    let w2 = r(1.0) / (w * w);
    let mut t = r(1.0) / w;
    for b in STIRLING.iter() {
        s = s + scale(t, *b);
        t = t * w2;
    }
    finite(s - shift)
}

/// `gamma` at complex machine arguments (`None` outside the method's range).
pub fn gamma(z: C64) -> Option<C64> {
    finite(log_gamma(z)?.exp())
}

/// `digamma` at complex machine arguments (`None` outside the method's range).
pub fn digamma(z: C64) -> Option<C64> {
    if is_pole(z) {
        return None;
    }
    let mut acc = r(0.0);
    let mut w = z;
    let mut n = 0;
    while w.re < 16.0 {
        acc = acc - r(1.0) / w;
        w = w + r(1.0);
        n += 1;
        if n > 200_000 {
            return None;
        }
    }
    // log w - 1/(2w) - sum B_2k/(2k w^2k)
    let mut s = w.log() - r(0.5) / w;
    let w2 = r(1.0) / (w * w);
    let mut t = w2;
    for (k, b) in BERNOULLI.iter().enumerate() {
        s = s - scale(t, b / (2.0 * (k + 1) as f64));
        t = t * w2;
    }
    finite(s + acc)
}

/// The Hurwitz zeta function zeta(s, a) for Re a > 0 (a > 0 real) by Euler-Maclaurin.
pub fn hurwitz_zeta(s: C64, a: C64) -> Option<C64> {
    if s.re == 1.0 && s.im == 0.0 {
        return None;
    }
    if a.re <= 0.0 && a.im == 0.0 && a.re == a.re.floor() {
        return None;
    }
    let n = 15 + abs(s).ceil() as usize + (if a.re < 0.0 { (-a.re).ceil() as usize } else { 0 });
    if n > 100_000 {
        return None;
    }
    let mut sum = r(0.0);
    for k in 0..n {
        sum = sum + powc(a + r(k as f64), neg(s));
    }
    let an = a + r(n as f64);
    // tail: an^(1-s)/(s-1) + an^-s/2 + sum_j B_2j/(2j)! (s)_(2j-1) an^(-s-2j+1)
    let mut tail = powc(an, r(1.0) - s) / (s - r(1.0)) + scale(powc(an, neg(s)), 0.5);
    let mut poch = s; // (s)_(2j-1)
    let mut fact = 2.0; // (2j)!
    let mut p = powc(an, neg(s) - r(1.0)); // an^(-s-2j+1)
    let inv2 = r(1.0) / (an * an);
    for (j, b) in BERNOULLI.iter().enumerate() {
        let term = scale(poch * p, b / fact);
        tail = tail + term;
        let j2 = 2.0 * (j + 1) as f64;
        poch = poch * (s + r(j2 - 1.0)) * (s + r(j2));
        fact *= (j2 + 1.0) * (j2 + 2.0);
        p = p * inv2;
        if abs(term) < 1e-17 * abs(tail) {
            break;
        }
    }
    finite(sum + tail)
}

/// The Riemann zeta function, with the functional equation for Re s < 1/2.
pub fn zeta(s: C64) -> Option<C64> {
    if s.re < 0.5 {
        // zeta(s) = 2^s pi^(s-1) sin(pi s/2) gamma(1-s) zeta(1-s)
        let one_s = r(1.0) - s;
        let f = powc(r(2.0), s) * powc(r(std::f64::consts::PI), s - r(1.0)) * scale(s, std::f64::consts::FRAC_PI_2).sin();
        return finite(f * gamma(one_s)? * hurwitz_zeta(one_s, r(1.0))?);
    }
    hurwitz_zeta(s, r(1.0))
}

/// polygamma(m, z) = (-1)^(m+1) m! zeta(m + 1, z) for integer m >= 1.
pub fn polygamma(m: f64, z: C64) -> Option<C64> {
    if m == 0.0 {
        return digamma(z);
    }
    if m < 0.0 || m != m.floor() || m > 100.0 {
        return None;
    }
    let fact: f64 = (1..=m as u64).map(|k| k as f64).product();
    let sign = if (m as u64).is_multiple_of(2) { -1.0 } else { 1.0 };
    // zeta(m + 1, z) needs Re z > 0: the recurrence psi^(m)(z) = psi^(m)(z + 1) - (-1)^m m!/z^(m+1)
    let mut w = z;
    let mut acc = r(0.0);
    while w.re < 1.0 {
        acc = acc + scale(r(1.0) / powc(w, r(m + 1.0)), -sign * fact);
        w = w + r(1.0);
    }
    finite(scale(hurwitz_zeta(r(m + 1.0), w)?, sign * fact) + acc)
}

/// `beta` at complex machine arguments (`None` outside the method's range).
pub fn beta(a: C64, b: C64) -> Option<C64> {
    finite((log_gamma(a)? + log_gamma(b)? - log_gamma(a + b)?).exp())
}

/// sum_k term_k with term_(k+1) = term_k * ratio(k), until the terms are negligible.
fn series(first: C64, ratio: impl Fn(usize) -> C64, max: usize) -> Option<C64> {
    let mut t = first;
    let mut s = first;
    let mut big = abs(first);
    for k in 0..max {
        t = t * ratio(k);
        s = s + t;
        big = big.max(abs(t));
        if abs(t) <= 1e-17 * abs(s) && k > 2 {
            // cancellation: at most 4 digits lost
            return (big <= 1e4 * abs(s).max(1e-300)).then_some(s);
        }
    }
    None
}

/// `hyp0f1` at complex machine arguments (`None` outside the method's range).
pub fn hyp0f1(b: C64, z: C64) -> Option<C64> {
    if abs(z) > 900.0 || is_pole(b) {
        return None;
    }
    series(r(1.0), |k| z / ((b + r(k as f64)) * r((k + 1) as f64)), 2000)
}

/// `hyp1f1` at complex machine arguments (`None` outside the method's range).
pub fn hyp1f1(a: C64, b: C64, z: C64) -> Option<C64> {
    if abs(z) > 600.0 || is_pole(b) {
        return None;
    }
    if z.re < 0.0 {
        // Kummer: e^z 1F1(b - a; b; -z)
        let v = series(r(1.0), |k| (b - a + r(k as f64)) * neg(z) / ((b + r(k as f64)) * r((k + 1) as f64)), 5000)?;
        return finite(z.exp() * v);
    }
    series(r(1.0), |k| (a + r(k as f64)) * z / ((b + r(k as f64)) * r((k + 1) as f64)), 5000)
}

/// `hyp2f1` at complex machine arguments (`None` outside the method's range).
pub fn hyp2f1(a: C64, b: C64, cc: C64, z: C64) -> Option<C64> {
    let term = |a: C64, b: C64, z: C64| move |k: usize| (a + r(k as f64)) * (b + r(k as f64)) * z / ((cc + r(k as f64)) * r((k + 1) as f64));
    // a terminating series (a or b a non-positive integer before c reaches a pole): the polynomial, any z
    let ends = |p: C64| (is_pole(p) && p.re >= -10_000.0).then(|| -p.re as usize);
    if let Some(n) = [ends(a), ends(b)].into_iter().flatten().min() {
        if !(is_pole(cc) && -cc.re < n as f64) {
            let f = term(a, b, z);
            let (mut t, mut s) = (r(1.0), r(1.0));
            for k in 0..n {
                t = t * f(k);
                s = s + t;
            }
            return finite(s);
        }
    }
    if is_pole(cc) {
        return None;
    }
    if abs(z) < 0.9 {
        return series(r(1.0), term(a, b, z), 2000);
    }
    // Pfaff: (1 - z)^-a 2F1(a, c - b; c; z/(z - 1))
    let one = r(1.0);
    let w = z / (z - one);
    if abs(w) < 0.9 {
        let f = series(one, term(a, cc - b, w), 2000)?;
        return finite(powc(one - z, neg(a)) * f);
    }
    None
}

/// `exp_integral_ei` at complex machine arguments (`None` outside the method's range).
pub fn exp_integral_ei(z: C64) -> Option<C64> {
    if abs(z) > 40.0 || abs(z) == 0.0 {
        return None;
    }
    // gamma + log z + sum z^k/(k k!)
    let s = series(z, |k| z * r((k + 1) as f64) / r(((k + 2) * (k + 2)) as f64), 500)?;
    // the principal branch of Ei for non-real z: (log z - log(1/z))/2 - log(-z) + log z ... in the form of the
    // reference convention Ei(z) = gamma + log z + sum
    finite(r(std::f64::consts::EULER_GAMMA) + z.log() + s)
}

/// `log_integral` at complex machine arguments (`None` outside the method's range).
pub fn log_integral(z: C64) -> Option<C64> {
    exp_integral_ei(z.log())
}

/// Si, Ci, Shi, Chi.
pub fn trig_integral(z: C64, which: u8) -> Option<C64> {
    if abs(z) > 40.0 || abs(z) == 0.0 {
        return None;
    }
    let z2 = z * z;
    let eg = r(std::f64::consts::EULER_GAMMA);
    match which {
        // Si: sum (-1)^k z^(2k+1)/((2k+1)(2k+1)!)
        0 => series(z, |k| {
            let k = k as f64;
            neg(z2) * r((2.0 * k + 1.0) / ((2.0 * k + 3.0) * (2.0 * k + 2.0) * (2.0 * k + 3.0)))
        }, 500),
        // Ci: gamma + log z + sum_(k>=1) (-1)^k z^2k/(2k (2k)!)
        1 => {
            let first = scale(neg(z2), 0.25);
            let s = series(first, |k| {
                let k = (k + 1) as f64;
                neg(z2) * r((2.0 * k) / ((2.0 * k + 2.0) * (2.0 * k + 1.0) * (2.0 * k + 2.0)))
            }, 500)?;
            finite(eg + z.log() + s)
        }
        // Shi
        2 => series(z, |k| {
            let k = k as f64;
            z2 * r((2.0 * k + 1.0) / ((2.0 * k + 3.0) * (2.0 * k + 2.0) * (2.0 * k + 3.0)))
        }, 500),
        // Chi
        _ => {
            let first = scale(z2, 0.25);
            let s = series(first, |k| {
                let k = (k + 1) as f64;
                z2 * r((2.0 * k) / ((2.0 * k + 2.0) * (2.0 * k + 1.0) * (2.0 * k + 2.0)))
            }, 500)?;
            finite(eg + z.log() + s)
        }
    }
}

/// J_nu(z) (sign -1) or I_nu(z) (sign +1) by the power series (z/2)^nu sum (s z^2/4)^k/(k! gamma(nu + k + 1)).
fn bessel_series(nu: C64, z: C64, sign: f64) -> Option<C64> {
    if abs(z) > 25.0 {
        return None;
    }
    let q = scale(z * z, sign * 0.25);
    let first = if is_pole(nu + r(1.0)) {
        // 1/gamma at a pole is 0: the series starts at the first nonzero term (integer nu < 0)
        return None;
    } else {
        powc(scale(z, 0.5), nu) / gamma(nu + r(1.0))?
    };
    series(first, |k| q / (r((k + 1) as f64) * (nu + r((k + 1) as f64))), 2000)
}

/// `bessel_j` at complex machine arguments (`None` outside the method's range).
pub fn bessel_j(nu: C64, z: C64) -> Option<C64> {
    if nu.im == 0.0 && nu.re < 0.0 && nu.re == nu.re.floor() {
        // J_-n = (-1)^n J_n
        let v = bessel_series(neg(nu), z, -1.0)?;
        return Some(if (-nu.re) as i64 % 2 == 0 { v } else { neg(v) });
    }
    bessel_series(nu, z, -1.0)
}

/// `bessel_i` at complex machine arguments (`None` outside the method's range).
pub fn bessel_i(nu: C64, z: C64) -> Option<C64> {
    if nu.im == 0.0 && nu.re < 0.0 && nu.re == nu.re.floor() {
        return bessel_series(neg(nu), z, 1.0);
    }
    bessel_series(nu, z, 1.0)
}

fn integer(nu: C64) -> bool {
    nu.im == 0.0 && nu.re == nu.re.floor()
}

/// `bessel_y` at complex machine arguments (`None` outside the method's range).
pub fn bessel_y(nu: C64, z: C64) -> Option<C64> {
    if integer(nu) {
        return None;
    }
    // (J_nu cos(nu pi) - J_-nu)/sin(nu pi)
    let a = scale(nu, std::f64::consts::PI);
    finite((bessel_j(nu, z)? * a.cos() - bessel_j(neg(nu), z)?) / a.sin())
}

/// `bessel_k` at complex machine arguments (`None` outside the method's range).
pub fn bessel_k(nu: C64, z: C64) -> Option<C64> {
    if integer(nu) {
        return None;
    }
    // pi/2 (I_-nu - I_nu)/sin(nu pi)
    let a = scale(nu, std::f64::consts::PI);
    finite(scale(bessel_i(neg(nu), z)? - bessel_i(nu, z)?, std::f64::consts::FRAC_PI_2) / a.sin())
}

/// Ai, Bi, Ai', Bi' by the Maclaurin series f, g (DLMF 9.4.1-9.4.4).
pub fn airy(z: C64, which: u8) -> Option<C64> {
    if abs(z) > 8.0 {
        return None;
    }
    let z3 = z * z * z;
    // f = sum 3^k (1/3)_k z^3k/(3k)!, g = sum 3^k (2/3)_k z^(3k+1)/(3k+1)!
    let f = series(r(1.0), |k| {
        let k = k as f64;
        z3 / r((3.0 * k + 2.0) * (3.0 * k + 3.0))
    }, 500)?;
    let g = series(z, |k| {
        let k = k as f64;
        z3 / r((3.0 * k + 3.0) * (3.0 * k + 4.0))
    }, 500)?;
    // f' = sum_(k>=1) 3^k (1/3)_k z^(3k-1)/(3k-1)!, g' = sum 3^k (2/3)_k z^3k/(3k)!
    let fp = series(scale(z * z, 0.5), |k| {
        let k = k as f64;
        z3 / r((3.0 * k + 3.0) * (3.0 * k + 5.0))
    }, 500)?;
    let gp = series(r(1.0), |k| {
        let k = k as f64;
        z3 / r((3.0 * k + 1.0) * (3.0 * k + 3.0))
    }, 500)?;
    let c1 = 0.355_028_053_887_817_2;
    let c2 = 0.258_819_403_792_806_8;
    let s3 = 3f64.sqrt();
    Some(match which {
        0 => scale(f, c1) - scale(g, c2),
        1 => scale(scale(f, c1) + scale(g, c2), s3),
        2 => scale(fp, c1) - scale(gp, c2),
        _ => scale(scale(fp, c1) + scale(gp, c2), s3),
    })
}

/// Carlson's R_F by duplication (complex arguments off the negative real axis).
pub fn carlson_rf(x: C64, y: C64, z: C64) -> Option<C64> {
    let (mut x, mut y, mut z) = (x, y, z);
    for _ in 0..100 {
        let (sx, sy, sz) = (x.sqrt(), y.sqrt(), z.sqrt());
        let l = sx * sy + sx * sz + sy * sz;
        x = scale(x + l, 0.25);
        y = scale(y + l, 0.25);
        z = scale(z + l, 0.25);
        let a = scale(x + y + z, 1.0 / 3.0);
        let d = abs(x - a).max(abs(y - a)).max(abs(z - a));
        if d < 1e-4 * abs(a) {
            let (dx, dy, dz) = ((a - x) / a, (a - y) / a, (a - z) / a);
            let e2 = dx * dy - dz * dz;
            let e3 = dx * dy * dz;
            let s = r(1.0) - scale(e2, 0.1) + scale(e3, 1.0 / 14.0) + scale(e2 * e2, 1.0 / 24.0)
                - scale(e2 * e3, 3.0 / 44.0);
            return finite(s / a.sqrt());
        }
    }
    None
}

/// Carlson's R_D by duplication (the series of Carlson 1995 after convergence).
pub fn carlson_rd(x: C64, y: C64, z: C64) -> Option<C64> {
    let (mut x, mut y, mut z) = (x, y, z);
    let mut sum = r(0.0);
    let mut fac = 1.0;
    for _ in 0..100 {
        let (sx, sy, sz) = (x.sqrt(), y.sqrt(), z.sqrt());
        let l = sx * (sy + sz) + sy * sz;
        sum = sum + scale(r(1.0) / (sz * (z + l)), fac);
        fac *= 0.25;
        x = scale(x + l, 0.25);
        y = scale(y + l, 0.25);
        z = scale(z + l, 0.25);
        let a = scale(x + y + scale(z, 3.0), 0.2);
        let (dx, dy, dz) = ((a - x) / a, (a - y) / a, (a - z) / a);
        if abs(dx).max(abs(dy)).max(abs(dz)) < 1e-3 {
            let ea = dx * dy;
            let eb = dz * dz;
            let ec = ea - eb;
            let ed = ea - scale(eb, 6.0);
            let ee = ed + ec + ec;
            let (c1, c2, c3, c4, c5, c6) = (3.0 / 14.0, 1.0 / 6.0, 9.0 / 22.0, 3.0 / 26.0, 3.0 / 56.0, 9.0 / 52.0);
            let inner1 = ed * (r(-c1) + scale(ed, c5) - scale(dz * ee, c6));
            let inner2 = dz * (scale(ee, c2) + dz * (scale(ec, -c3) + scale(dz * ea, c4)));
            let s = r(1.0) + inner1 + inner2;
            return finite(scale(sum, 3.0) + scale(s, fac) / (a * a.sqrt()));
        }
    }
    None
}

/// `elliptic_k` at complex machine arguments (`None` outside the method's range).
pub fn elliptic_k(m: C64) -> Option<C64> {
    carlson_rf(r(0.0), r(1.0) - m, r(1.0))
}

/// `elliptic_e` at complex machine arguments (`None` outside the method's range).
pub fn elliptic_e(m: C64) -> Option<C64> {
    let k1 = r(1.0) - m;
    finite(carlson_rf(r(0.0), k1, r(1.0))? - scale(m * carlson_rd(r(0.0), k1, r(1.0))?, 1.0 / 3.0))
}

/// F(phi | m) for |Re phi| <= pi/2 (reduced by periodicity, F(phi + k pi) = F(phi) + 2 k K).
pub fn elliptic_f(phi: C64, m: C64) -> Option<C64> {
    let k = (phi.re / std::f64::consts::PI).round();
    let p = phi - r(k * std::f64::consts::PI);
    let (s, cphi) = (p.sin(), p.cos());
    let v = s * carlson_rf(cphi * cphi, r(1.0) - m * s * s, r(1.0))?;
    if k == 0.0 {
        return finite(v);
    }
    finite(v + scale(elliptic_k(m)?, 2.0 * k))
}

/// E(phi | m) likewise.
pub fn elliptic_e_inc(phi: C64, m: C64) -> Option<C64> {
    let k = (phi.re / std::f64::consts::PI).round();
    let p = phi - r(k * std::f64::consts::PI);
    let (s, cphi) = (p.sin(), p.cos());
    let (x, y) = (cphi * cphi, r(1.0) - m * s * s);
    let v = s * carlson_rf(x, y, r(1.0))? - scale(m * s * s * s * carlson_rd(x, y, r(1.0))?, 1.0 / 3.0);
    if k == 0.0 {
        return finite(v);
    }
    finite(v + scale(elliptic_e(m)?, 2.0 * k))
}
