//! Random variates of the families on a session stream (decision DI.6). Every method is exact for its
//! distribution (no approximation beyond machine rounding) and draws a number of words that depends only on the
//! stream, so a seed reproduces the values:
//!
//! - normal, log-normal: Box-Muller pairs (`Stream::normal_pair`), both values used;
//! - uniform, exponential, geometric: inversion of one uniform;
//! - gamma: Marsaglia and Tsang (ACM TOMS 2000) for k >= 1, boosted by U^(1/k) for k < 1; chi-square through
//!   gamma(k/2, 2); beta as G1/(G1 + G2); Student t as Z / sqrt(V/n) with V chi-square;
//! - Poisson: multiplication of uniforms for m < 10, the transformed rejection PTRS of Hormann (1993) beyond;
//! - binomial: inversion (sequential search from 0) when n min(p, 1-p) < 10, else Knuth's recursion through the
//!   order statistics of uniforms (a beta variate halves n each time) down to that case.
use crate::family::Family;
use crate::machine::Mach;
use crate::special::lgamma;
use mc_random::Stream;

/// A draw source: the stream, and a spare normal from the last Box-Muller pair.
pub struct Draws<'s> {
    s: &'s mut Stream,
    spare: Option<f64>,
}

impl<'s> Draws<'s> {
    /// Draws from `s`.
    pub fn new(s: &'s mut Stream) -> Draws<'s> {
        Draws { s, spare: None }
    }

    /// A uniform real in [0, 1).
    fn u(&mut self) -> f64 {
        self.s.uniform()
    }

    /// A uniform real in (0, 1] (safe for logarithms).
    fn u_pos(&mut self) -> f64 {
        1.0 - self.s.uniform()
    }

    /// A standard normal real.
    fn normal(&mut self) -> f64 {
        if let Some(z) = self.spare.take() {
            return z;
        }
        let (a, b) = self.s.normal_pair();
        self.spare = Some(b);
        a
    }

    /// A gamma variate with shape k > 0 and unit scale.
    fn gamma(&mut self, k: f64) -> f64 {
        if k < 1.0 {
            // G(k) = G(k + 1) U^(1/k)
            let g = self.gamma(k + 1.0);
            return g * self.u_pos().powf(1.0 / k);
        }
        let d = k - 1.0 / 3.0;
        let c = 1.0 / (9.0 * d).sqrt();
        loop {
            let (mut x, mut v);
            loop {
                x = self.normal();
                v = 1.0 + c * x;
                if v > 0.0 {
                    break;
                }
            }
            v = v * v * v;
            let u = self.u_pos();
            if u < 1.0 - 0.0331 * x * x * x * x || u.ln() < 0.5 * x * x + d * (1.0 - v + v.ln()) {
                return d * v;
            }
        }
    }

    /// A Poisson variate with mean m > 0.
    fn poisson(&mut self, m: f64) -> f64 {
        if m < 10.0 {
            // multiply uniforms until the product falls below e^-m
            let limit = (-m).exp();
            let mut k = 0.0;
            let mut prod = self.u_pos();
            while prod > limit {
                k += 1.0;
                prod *= self.u_pos();
            }
            return k;
        }
        // PTRS (Hormann 1993)
        let slam = m.sqrt();
        let loglam = m.ln();
        let b = 0.931 + 2.53 * slam;
        let a = -0.059 + 0.02483 * b;
        let inv_alpha = 1.1239 + 1.1328 / (b - 3.4);
        let vr = 0.9277 - 3.6224 / (b - 2.0);
        loop {
            let u = self.u() - 0.5;
            let v = self.u_pos();
            let us = 0.5 - u.abs();
            let k = ((2.0 * a / us + b) * u + m + 0.43).floor();
            if us >= 0.07 && v <= vr {
                return k;
            }
            if k < 0.0 || (us < 0.013 && v > us) {
                continue;
            }
            let lhs = v.ln() + inv_alpha.ln() - (a / (us * us) + b).ln();
            let rhs = -m + k * loglam - lgamma(k + 1.0).to_f64();
            if lhs <= rhs {
                return k;
            }
        }
    }

    /// A binomial variate: n trials (an integer-valued real), success probability p.
    fn binomial(&mut self, n0: f64, mut p: f64) -> f64 {
        // count successes, or failures for p > 1/2 (the sequential search stays short)
        let mut n = n0;
        let flip = p > 0.5;
        if flip {
            p = 1.0 - p;
        }
        let mut acc = 0.0;
        // Knuth's recursion: the i-th smallest of n uniforms is beta(i, n + 1 - i) distributed
        while n * p >= 10.0 {
            let i = (n / 2.0).floor() + 1.0;
            let ga = self.gamma(i);
            let gb = self.gamma(n + 1.0 - i);
            let x = ga / (ga + gb);
            if x <= p {
                // the i smallest uniforms are successes; the rest succeed with probability (p - x)/(1 - x)
                acc += i;
                n -= i;
                p = (p - x) / (1.0 - x);
            } else {
                // only uniforms below the i-th can succeed: i - 1 of them, with probability p/x
                n = i - 1.0;
                p /= x;
            }
        }
        // inversion: P(0) = (1-p)^n, P(k+1)/P(k) = (n-k)/(k+1) p/(1-p)
        let mut k = 0.0;
        if n > 0.0 && p > 0.0 {
            let r = p / (1.0 - p);
            let mut f = (n * (-p).ln_1p()).exp();
            let mut u = self.u();
            while u > f && k < n {
                u -= f;
                f *= (n - k) / (k + 1.0) * r;
                k += 1.0;
            }
        }
        let total = acc + k;
        if flip {
            // the successes of the original p are the failures counted here
            n0 - total
        } else {
            total
        }
    }

    /// One variate of `d`.
    pub fn variate(&mut self, d: &Mach) -> f64 {
        let [a, b] = d.p;
        match d.fam {
            Family::Normal => a + b * self.normal(),
            Family::LogNormal => (a + b * self.normal()).exp(),
            Family::Uniform => a + (b - a) * self.u(),
            Family::Exponential => -self.u_pos().ln() / a,
            Family::Gamma => b * self.gamma(a),
            Family::ChiSquare => 2.0 * self.gamma(a / 2.0),
            Family::Beta => {
                let x = self.gamma(a);
                let y = self.gamma(b);
                x / (x + y)
            }
            Family::StudentT => {
                let z = self.normal();
                let v = 2.0 * self.gamma(a / 2.0);
                z / (v / a).sqrt()
            }
            Family::Binomial => {
                if b == 0.0 || a == 0.0 {
                    return 0.0;
                }
                if b == 1.0 {
                    return a;
                }
                self.binomial(a, b)
            }
            Family::Poisson => self.poisson(a),
            Family::Geometric => {
                if a == 1.0 {
                    return 0.0;
                }
                // the failures before the first success: floor(ln U / ln(1 - p))
                (self.u_pos().ln() / (-a).ln_1p()).floor()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sample mean and variance of n draws.
    fn moments(d: &Mach, n: usize, seed: u64) -> (f64, f64) {
        let mut s = Stream::from_key(seed);
        let mut g = Draws::new(&mut s);
        let xs: Vec<f64> = (0..n).map(|_| g.variate(d)).collect();
        let m = xs.iter().sum::<f64>() / n as f64;
        let v = xs.iter().map(|x| (x - m) * (x - m)).sum::<f64>() / (n as f64 - 1.0);
        (m, v)
    }

    #[test]
    fn sample_moments_match() {
        // (distribution, mean, variance)
        let cases = [
            (Mach::new(Family::Gamma, &[0.4, 2.0]).unwrap(), 0.8, 1.6),
            (Mach::new(Family::Gamma, &[5.0, 1.5]).unwrap(), 7.5, 11.25),
            (Mach::new(Family::Beta, &[2.0, 3.0]).unwrap(), 0.4, 0.04),
            (Mach::new(Family::StudentT, &[6.0]).unwrap(), 0.0, 1.5),
            (Mach::new(Family::Poisson, &[3.0]).unwrap(), 3.0, 3.0),
            (Mach::new(Family::Poisson, &[250.0]).unwrap(), 250.0, 250.0),
            (Mach::new(Family::Binomial, &[12.0, 0.3]).unwrap(), 3.6, 2.52),
            (Mach::new(Family::Binomial, &[10000.0, 0.35]).unwrap(), 3500.0, 2275.0),
            (Mach::new(Family::Binomial, &[1000.0, 0.9]).unwrap(), 900.0, 90.0),
            (Mach::new(Family::Geometric, &[0.25]).unwrap(), 3.0, 12.0),
            (Mach::new(Family::Exponential, &[2.0]).unwrap(), 0.5, 0.25),
        ];
        let n = 200_000;
        for (d, mean, var) in cases {
            let (m, v) = moments(&d, n, 7);
            // within 5 standard errors of the mean, and the variance within 5%
            assert!((m - mean).abs() < 5.0 * (var / n as f64).sqrt(), "{d:?}: mean {m}");
            assert!((v - var).abs() < 0.05 * var, "{d:?}: variance {v}");
        }
    }

    #[test]
    fn binomial_values_are_in_range() {
        let d = Mach::new(Family::Binomial, &[40.0, 0.97]).unwrap();
        let mut s = Stream::from_key(3);
        let mut g = Draws::new(&mut s);
        for _ in 0..10_000 {
            let k = g.variate(&d);
            assert!((0.0..=40.0).contains(&k) && k == k.floor(), "{k}");
        }
    }
}
