//! Dense output of the machine methods: every accepted step stores its continuous extension as a polynomial of
//! degree 4 in the normalised time θ = (t - origin) / h, by its monomial coefficients (decision N3.4). The
//! Dormand–Prince extension (order 4) and the Rodas one (order 3) both have this form:
//!
//! ```text
//! y(θ) = y0 + θ r1 + θ(1-θ) r2 + θ²(1-θ) r3 + θ²(1-θ)² r4
//!      = c0 + c1 θ + c2 θ² + c3 θ³ + c4 θ⁴,   c0 = y0, c1 = r1 + r2, c2 = r3 + r4 - r2, c3 = -r3 - 2 r4, c4 = r4
//! ```
//!
//! so one evaluation routine serves both, and derivatives come from the same coefficients.

/// The number of coefficient vectors per step.
pub(crate) const COEFS: usize = 5;

/// Monomial coefficients `[c0, c1, c2, c3, c4]` (each n long) from the nested form `y0, r1, r2, r3, r4`.
pub(crate) fn monomials(y0: &[f64], r: [&[f64]; 4], out: &mut Vec<f64>) {
    let n = y0.len();
    out.extend_from_slice(y0);
    out.extend((0..n).map(|i| r[0][i] + r[1][i]));
    out.extend((0..n).map(|i| r[2][i] + r[3][i] - r[1][i]));
    out.extend((0..n).map(|i| -r[2][i] - 2.0 * r[3][i]));
    out.extend_from_slice(r[3]);
}

/// The accepted steps of an integration in the order taken (forward or backward in time).
#[derive(Clone, Debug, Default)]
pub(crate) struct Steps {
    /// the start of each step
    pub origin: Vec<f64>,
    /// the signed step size
    pub h: Vec<f64>,
    /// `COEFS * n` coefficients per step
    pub coefs: Vec<f64>,
}

impl Steps {
    /// The number of steps.
    pub fn len(&self) -> usize {
        self.origin.len()
    }

    /// Record a step from `origin` of size `h` with its coefficients (`COEFS * n` values).
    pub fn push(&mut self, origin: f64, h: f64, coefs: &[f64]) {
        self.origin.push(origin);
        self.h.push(h);
        self.coefs.extend_from_slice(coefs);
    }

    /// Shorten the last step so that it ends at θ = `theta` (an event stopped the integration there): the
    /// polynomial is re-expressed in the shorter step's normalised time.
    pub fn truncate_last(&mut self, n: usize, theta: f64) {
        let Some(h) = self.h.last_mut() else { return };
        *h *= theta;
        let k = self.coefs.len() - COEFS * n;
        let mut s = 1.0;
        for j in 0..COEFS {
            for v in &mut self.coefs[k + j * n..k + (j + 1) * n] {
                *v *= s;
            }
            s *= theta;
        }
    }
}

/// The value (`order` 0) or a derivative of component `i` of one step's polynomial at θ, in time units of the
/// step size `h`.
pub(crate) fn eval(coefs: &[f64], n: usize, i: usize, h: f64, theta: f64, order: usize) -> f64 {
    if order >= COEFS {
        return 0.0;
    }
    // d^k/dθ^k of sum c_j θ^j = sum_{j >= k} j!/(j-k)! c_j θ^(j-k), by Horner from the top
    let mut acc = 0.0;
    for j in (order..COEFS).rev() {
        let falling: f64 = (j - order + 1..=j).map(|v| v as f64).product();
        acc = acc * theta + falling * coefs[j * n + i];
    }
    acc / h.powi(order as i32)
}

/// A machine solution over `[lo, hi]`: the steps of one or two integrations from the initial point, ordered by
/// time.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Piecewise {
    /// the state size
    pub n: usize,
    /// the left end of every step, ascending
    lo: Vec<f64>,
    /// the right end of every step
    hi: Vec<f64>,
    /// the origin and step size of every step
    origin: Vec<f64>,
    h: Vec<f64>,
    coefs: Vec<f64>,
}

impl Piecewise {
    /// The steps of a backward integration (taken in decreasing time, may be empty) and of a forward one,
    /// joined in increasing time.
    pub fn new(n: usize, backward: &Steps, forward: &Steps) -> Piecewise {
        let m = backward.len() + forward.len();
        let mut p = Piecewise {
            n,
            lo: Vec::with_capacity(m),
            hi: Vec::with_capacity(m),
            origin: Vec::with_capacity(m),
            h: Vec::with_capacity(m),
            coefs: Vec::with_capacity(backward.coefs.len() + forward.coefs.len()),
        };
        let w = COEFS * n;
        let mut add = |s: &Steps, k: usize| {
            let (o, h) = (s.origin[k], s.h[k]);
            p.lo.push(o.min(o + h));
            p.hi.push(o.max(o + h));
            p.origin.push(o);
            p.h.push(h);
            p.coefs.extend_from_slice(&s.coefs[k * w..(k + 1) * w]);
        };
        for k in (0..backward.len()).rev() {
            add(backward, k);
        }
        for k in 0..forward.len() {
            add(forward, k);
        }
        p
    }

    /// The value at `t` clamped into the covered range (for a point at the end that rounding put just outside).
    pub fn value_clamped(&self, i: usize, order: usize, t: f64) -> Option<f64> {
        let m = self.lo.len();
        if m == 0 {
            return None;
        }
        self.value(i, order, t.clamp(self.lo[0], self.hi[m - 1]))
    }

    /// Component `i` (or its derivative of order `order`) at `t`; `None` outside the steps.
    pub fn value(&self, i: usize, order: usize, t: f64) -> Option<f64> {
        let m = self.lo.len();
        if m == 0 || !(self.lo[0] <= t && t <= self.hi[m - 1]) {
            return None;
        }
        // the last step starting at or before t
        let k = self.lo.partition_point(|&a| a <= t).max(1) - 1;
        let w = COEFS * self.n;
        let theta = (t - self.origin[k]) / self.h[k];
        Some(eval(&self.coefs[k * w..(k + 1) * w], self.n, i, self.h[k], theta, order))
    }

    /// Every number of the solution, for its content key.
    pub fn words(&self) -> impl Iterator<Item = f64> + '_ {
        self.lo.iter().chain(&self.hi).chain(&self.origin).chain(&self.h).chain(&self.coefs).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_and_monomial_forms_agree() {
        let (y0, r) = ([1.0], [[0.5], [-0.25], [0.125], [0.0625]]);
        let mut c = vec![];
        monomials(&y0, [&r[0], &r[1], &r[2], &r[3]], &mut c);
        for theta in [0.0, 0.3, 0.7, 1.0] {
            let t1 = 1.0 - theta;
            let nested = y0[0] + theta * (r[0][0] + t1 * (r[1][0] + theta * (r[2][0] + t1 * r[3][0])));
            assert!((eval(&c, 1, 0, 2.0, theta, 0) - nested).abs() < 1e-15);
        }
        // the derivative in t is the θ-derivative over h
        let d = (eval(&c, 1, 0, 2.0, 0.5 + 1e-6, 0) - eval(&c, 1, 0, 2.0, 0.5 - 1e-6, 0)) / (2e-6 * 2.0);
        assert!((eval(&c, 1, 0, 2.0, 0.5, 1) - d).abs() < 1e-8);
        // a truncated step describes the same polynomial
        let mut s = Steps::default();
        s.push(0.0, 2.0, &c);
        s.truncate_last(1, 0.5);
        assert_eq!(s.h[0], 1.0);
        assert!((eval(&s.coefs, 1, 0, 1.0, 0.6, 0) - eval(&c, 1, 0, 2.0, 0.3, 0)).abs() < 1e-15);
    }
}
