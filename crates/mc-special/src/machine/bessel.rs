//! K_nu(x) and I_nu(x) for machine reals by Temme's method, in double-double (decision SF.3).
//!
//! With nu = n + mu, |mu| <= 1/2: K_mu(x) and K_mu+1(x) come from Temme's series for x <= 2 (the gamma-function
//! factors gam1, gam2 from the Taylor coefficients of 1/Gamma(1 + z)) and from Steed's continued fraction CF2
//! (Thompson and Barnett) for x > 2; the forward recurrence K_nu+1 = 2 nu/x K_nu + K_nu-1 (stable) reaches K_nu.
//! I_nu: the continued fraction CF1 for I'_nu/I_nu, the downward recurrence to mu and the Wronskian
//! I_mu K'_mu - I'_mu K_mu = -1/x. Negative orders: K_-nu = K_nu, I_-nu = I_nu + (2/pi) sin(nu pi) K_nu. All
//! intermediate values are scaled double-doubles, so only the final rounding overflows or underflows.
use super::rgamma_coefficients;
use crate::dd::{exp, exp_scaled, log, sin_cos_pi, sin_cos_pi_any, Scaled, DD, PI};
use std::ops::{Add, Div, Mul, Neg, Sub};

const EPS: f64 = 1e-33;
const MAXIT: u32 = 2_000_000;
/// The largest order handled (the recurrence takes nu steps).
const MAX_ORDER: f64 = 1e6;

/// (K_mu(x), K_mu+1(x)) for |mu| <= 1/2, x > 0.
fn k_pair(mu: f64, x: DD) -> (Scaled, Scaled) {
    if x.hi <= 2.0 {
        temme(mu, x)
    } else {
        steed(mu, x)
    }
}

/// Temme's series for x <= 2.
fn temme(mu: f64, x: DD) -> (Scaled, Scaled) {
    let g = rgamma_coefficients();
    let m = DD::new(mu);
    let x2 = x.mul_f(0.5);
    let fact = if mu == 0.0 { DD::ONE } else { PI.mul(m).div(sin_cos_pi(m).0) };
    let d = log(x2).neg();
    let e = m.mul(d);
    let (ep, em) = (exp(e), exp(e.neg()));
    let fact2 = if e.hi.abs() < 1e-3 {
        // sinh(e)/e = 1 + e^2/6 + e^4/120 + ...
        let e2 = e.sqr();
        let mut s = DD::ONE;
        let mut t = DD::ONE;
        for k in 1..=8 {
            t = t.mul(e2).div_f(((2 * k) * (2 * k + 1)) as f64);
            s = s.add(t);
        }
        s
    } else {
        ep.sub(em).div(e.mul_f(2.0))
    };
    let cosh = ep.add(em).mul_f(0.5);
    // gam1 = (1/Gamma(1-mu) - 1/Gamma(1+mu))/(2 mu) = -sum_{k odd} g_k mu^(k-1); gam2 = sum_{k even} g_k mu^k
    let m2 = m.sqr();
    let (mut gam1, mut gam2) = (DD::ZERO, DD::ZERO);
    for k in (0..g.len()).rev() {
        if k % 2 == 1 {
            gam1 = gam1.mul(m2).add(g[k]);
        } else {
            gam2 = gam2.mul(m2).add(g[k]);
        }
    }
    let gam1 = gam1.neg();
    let (mut gampl, mut gammi) = (DD::ZERO, DD::ZERO);
    for c in g.iter().rev() {
        gampl = gampl.mul(m).add(*c);
        gammi = gammi.mul(m.neg()).add(*c);
    }
    let mut ff = fact.mul(gam1.mul(cosh).add(gam2.mul(fact2).mul(d)));
    let mut sum = ff;
    let mut p = ep.mul_f(0.5).div(gampl);
    let mut q = em.mul_f(0.5).div(gammi);
    let mut c = DD::ONE;
    let dq = x2.sqr();
    let mut sum1 = p;
    let mu2 = m2;
    for i in 1..MAXIT {
        let fi = i as f64;
        ff = ff.mul_f(fi).add(p).add(q).div(DD::new(fi * fi).sub(mu2));
        c = c.mul(dq).div_f(fi);
        p = p.div(DD::new(fi).sub(m));
        q = q.div(DD::new(fi).add(m));
        let del = c.mul(ff);
        sum = sum.add(del);
        let del1 = c.mul(p.sub(ff.mul_f(fi)));
        sum1 = sum1.add(del1);
        if del.hi.abs() < sum.hi.abs() * EPS {
            break;
        }
    }
    (Scaled::new(sum), Scaled::new(sum1.mul_f(2.0).div(x)))
}

/// Steed's continued fraction CF2 for x > 2.
fn steed(mu: f64, x: DD) -> (Scaled, Scaled) {
    let m = DD::new(mu);
    let mut b = x.add_f(1.0).mul_f(2.0);
    let mut d = b.recip();
    let mut delh = d;
    let mut h = d;
    let (mut q1, mut q2) = (DD::ZERO, DD::ONE);
    let a1 = DD::new(0.25).sub(m.sqr());
    let mut q = a1;
    let mut c = a1;
    let mut a = a1.neg();
    let mut s = DD::ONE.add(q.mul(delh));
    for i in 2..MAXIT {
        let fi = i as f64;
        a = a.add_f(-2.0 * (fi - 1.0));
        c = a.neg().mul(c).div_f(fi);
        let qnew = q1.sub(b.mul(q2)).div(a);
        q1 = q2;
        q2 = qnew;
        q = q.add(c.mul(qnew));
        if q2.hi.abs() < 1e-200 {
            // q_k decay while c_k grow; only the products c_k q_k matter
            q1 = q1.mul_f(1e200);
            q2 = q2.mul_f(1e200);
            c = c.mul_f(1e-200);
        }
        b = b.add_f(2.0);
        d = b.add(a.mul(d)).recip();
        delh = b.mul(d).add_f(-1.0).mul(delh);
        h = h.add(delh);
        let dels = q.mul(delh);
        s = s.add(dels);
        if dels.hi.abs() < s.hi.abs() * EPS {
            break;
        }
    }
    let h = a1.mul(h);
    let pre = PI.div(x.mul_f(2.0)).sqrt();
    let k0 = exp_scaled(x.neg()).mul_dd(pre.div(s));
    let k1 = k0.mul_dd(m.add(x).add_f(0.5).sub(h).div(x));
    (k0, k1)
}

/// The forward recurrence from (K_mu, K_mu+1) to (K_nu, K_nu+1), nu = mu + n.
fn k_up(mu: f64, n: u64, x: DD, (mut k0, mut k1): (Scaled, Scaled)) -> (Scaled, Scaled) {
    let xi2 = DD::new(2.0).div(x);
    for i in 1..=n {
        let t = k1.mul_dd(xi2.mul(DD::new(mu).add_f(i as f64))).add(k0);
        k0 = k1;
        k1 = t;
    }
    (k0, k1)
}

/// Splits nu >= 0 into its nearest integer n and mu = nu - n.
fn split(nu: f64) -> (u64, f64) {
    let n = (nu + 0.5).floor();
    (n as u64, nu - n)
}

/// K_nu(x) for machine reals (NaN for x < 0, +inf at x = 0).
pub fn bessel_k(nu: f64, x: f64) -> f64 {
    if nu.is_nan() || x.is_nan() || x < 0.0 {
        return f64::NAN;
    }
    if x == 0.0 || nu.is_infinite() {
        return f64::INFINITY;
    }
    if x == f64::INFINITY {
        return 0.0;
    }
    let nu = nu.abs();
    if nu > MAX_ORDER {
        return f64::NAN;
    }
    let (n, mu) = split(nu);
    let x = DD::new(x);
    k_up(mu, n, x, k_pair(mu, x)).0.to_f64()
}

/// e^x K_nu(x) for machine reals x > 0 (no underflow at large x: the scaled K times e^x through logarithms in
/// double-double).
pub fn bessel_k_scaled(nu: f64, x: f64) -> f64 {
    if nu.is_nan() || x.is_nan() || x <= 0.0 || nu.is_infinite() {
        return if x == 0.0 { f64::INFINITY } else { f64::NAN };
    }
    let nu = nu.abs();
    if nu > MAX_ORDER || x == f64::INFINITY {
        return if x == f64::INFINITY { 0.0 } else { f64::NAN };
    }
    let (n, mu) = split(nu);
    let xd = DD::new(x);
    let k = k_up(mu, n, xd, k_pair(mu, xd)).0;
    exp(k.ln_abs().add_f(x)).to_f64()
}

/// e^-x I_nu(x) for machine reals x > 0 and nu >= 0.
pub fn bessel_i_scaled(nu: f64, x: f64) -> f64 {
    if nu < 0.0 && x > 0.0 && x < 700.0 {
        // negative orders through I_nu itself (no overflow below x = 700)
        return bessel_i(nu, x) * (-x).exp();
    }
    if nu.is_nan() || x.is_nan() || x <= 0.0 || nu < 0.0 || nu > MAX_ORDER {
        return if x == 0.0 && nu >= 0.0 { if nu == 0.0 { 1.0 } else { 0.0 } } else { f64::NAN };
    }
    let xd = DD::new(x);
    let i = i_and_k(nu, xd).0;
    exp(i.ln_abs().add_f(-x)).to_f64()
}

/// I_nu(x) and K_nu(x) for nu >= 0 and x > 0, scaled.
fn i_and_k(nu: f64, x: DD) -> (Scaled, Scaled) {
    let (n, mu) = split(nu);
    let xi = x.recip();
    let xi2 = xi.mul_f(2.0);
    // CF1 for f = I'_nu / I_nu (modified Lentz)
    let tiny = 1e-300;
    let mut h = xi.mul_f(nu);
    if h.hi < tiny {
        h = DD::new(tiny);
    }
    let mut b = xi2.mul_f(nu);
    let mut d = DD::ZERO;
    let mut c = h;
    for _ in 0..MAXIT {
        b = b.add(xi2);
        d = b.add(d).recip();
        c = b.add(c.recip());
        let del = c.mul(d);
        h = del.mul(h);
        if del.add_f(-1.0).hi.abs() < EPS {
            break;
        }
    }
    // downward recurrence from nu to mu with I_nu = 1 (scaled), I'_nu = f
    let mut ril = Scaled::new(DD::ONE);
    let mut ripl = Scaled::new(h);
    let mut fact = xi.mul_f(nu);
    for _ in 0..n {
        let t = ril.mul_dd(fact).add(ripl);
        fact = fact.sub(xi);
        ripl = t.mul_dd(fact).add(ril);
        ril = t;
    }
    let f = ripl.div(ril);
    let (k_mu, k_mu1) = k_pair(mu, x);
    // K'_mu = mu/x K_mu - K_mu+1; I_mu = (1/x) / (f K_mu - K'_mu)
    let kp = k_mu.mul_dd(xi.mul_f(mu)).add(k_mu1.neg());
    let i_mu = Scaled::new(xi).div(f.mul(k_mu).add(kp.neg()));
    let i_nu = i_mu.div(ril);
    let k_nu = k_up(mu, n, x, (k_mu, k_mu1)).0;
    (i_nu, k_nu)
}

/// I_nu(x) for machine reals (NaN for x < 0 with a non-integer order).
pub fn bessel_i(nu: f64, x: f64) -> f64 {
    if nu.is_nan() || x.is_nan() || nu.is_infinite() {
        return f64::NAN;
    }
    let int_order = nu == nu.floor();
    if x < 0.0 {
        if !int_order {
            return f64::NAN;
        }
        let v = bessel_i(nu, -x);
        return if (nu.abs() % 2.0) == 1.0 { -v } else { v };
    }
    if x == 0.0 {
        return if nu == 0.0 {
            1.0
        } else if nu > 0.0 || int_order {
            0.0
        } else {
            // I_nu(x) ~ (x/2)^nu / Gamma(nu + 1): the sign of 1/Gamma(nu + 1)
            let g = super::gamma::gamma(nu + 1.0);
            f64::INFINITY * g.signum()
        };
    }
    if x == f64::INFINITY {
        return f64::INFINITY;
    }
    let a = nu.abs();
    if a > MAX_ORDER {
        return f64::NAN;
    }
    if x > 1e5 && a < 0.5 * x {
        return f64::INFINITY;
    }
    let (i, k) = i_and_k(a, DD::new(x));
    if nu >= 0.0 || int_order {
        return i.to_f64();
    }
    // I_-a = I_a + (2/pi) sin(a pi) K_a
    let (s, _) = sin_cos_pi_any(DD::new(a));
    i.add(k.mul_dd(s.mul_f(2.0).div(PI))).to_f64()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ulps(got: f64, want: f64) -> f64 {
        ((got - want) / want).abs() / f64::EPSILON
    }

    #[test]
    fn known_values() {
        assert!(ulps(bessel_k(0.0, 1.0), 0.421_024_438_240_708_3) <= 2.0);
        assert!(ulps(bessel_k(1.0, 2.0), 0.139_865_881_816_522_43) <= 2.0);
        assert!(ulps(bessel_i(0.0, 1.0), 1.266_065_877_752_008_4) <= 2.0);
        assert!(ulps(bessel_i(1.0, 1.0), 0.565_159_103_992_485_1) <= 2.0);
        let k = |x: f64| (std::f64::consts::PI / (2.0 * x)).sqrt() * (-x).exp();
        assert!(ulps(bessel_k(0.5, 3.0), k(3.0)) <= 4.0);
        assert!(ulps(bessel_k(-0.5, 0.7), k(0.7)) <= 4.0);
        assert_eq!(bessel_k(1.0, 800.0), 0.0);
        assert_eq!(bessel_k(2.0, 0.0), f64::INFINITY);
        assert!(bessel_k(2.0, -1.0).is_nan());
        assert_eq!(bessel_i(0.0, 0.0), 1.0);
        assert!(ulps(bessel_i(-0.5, 1.0), (2.0 / std::f64::consts::PI).sqrt() * 1f64.cosh()) <= 4.0);
    }

    #[test]
    fn subnormal_orders_are_order_zero() {
        // a subnormal order once scaled a double-double to infinity, and splitting it recursed without end
        for nu in [5e-324, 1e-320, -1e-320, 1e-310] {
            assert!(ulps(bessel_i(nu, 5.0), 27.239_871_823_604_446) <= 2.0);
            assert!(ulps(bessel_k(nu, 5.0), 0.003_691_098_334_042_594) <= 2.0);
            assert!(ulps(bessel_i(nu, 0.5), 1.063_483_370_741_323_6) <= 2.0);
        }
    }
}
