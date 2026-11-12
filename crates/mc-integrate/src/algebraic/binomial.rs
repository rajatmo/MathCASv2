//! Chebyshev's theorem on binomial differentials (P. L. Chebyshev, 1853).
//!
//! `x^m (a + b x^n)^p dx` with `m`, `n`, `p` rational, `a`, `b` nonzero and `n` nonzero has an elementary
//! antiderivative exactly when one of `p`, `(m + 1)/n`, `(m + 1)/n + p` is an integer. With `u = x^n` the
//! differential is `(1/n) u^r (a + b u)^p du`, `r = (m + 1)/n - 1`, and the substitutions are:
//!
//! 1. `p` an integer: `x = t^N`, `N` the common denominator of `m` and `n` (the integrand becomes rational);
//! 2. `r` an integer (`(m + 1)/n` an integer): `t^s = a + b x^n`, `s` the denominator of `p`;
//! 3. `r + p` an integer: `t^s = b + a x^(-n)`.
//!
//! Otherwise the theorem proves that no elementary antiderivative exists; [`decide`] returns that proof as a
//! sentence. The classical proof reduces to the integrability of `u^r (1 - u)^p` and Liouville's theorem
//! (Chebyshev's argument; see also Marchisotto and Zakeri, "An invitation to integration in finite terms", College
//! Math. J. 25 (1994)).

/// A rational `n/d` with machine-sized parts, `d > 0`, reduced.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Q(pub i64, pub i64);

impl Q {
    /// The reduced rational `n/d` (`d` nonzero).
    pub fn new(n: i64, d: i64) -> Q {
        let g = gcd(n.unsigned_abs(), d.unsigned_abs()).max(1) as i64;
        let s = if d < 0 { -1 } else { 1 };
        Q(s * n / g, s * d / g)
    }
    /// Is it an integer?
    pub fn is_int(self) -> bool {
        self.1 == 1
    }
    fn add(self, o: Q) -> Option<Q> {
        Some(Q::new(self.0.checked_mul(o.1)?.checked_add(o.0.checked_mul(self.1)?)?, self.1.checked_mul(o.1)?))
    }
    fn div(self, o: Q) -> Option<Q> {
        if o.0 == 0 {
            return None;
        }
        Some(Q::new(self.0.checked_mul(o.1)?, self.1.checked_mul(o.0)?))
    }
}

impl std::fmt::Display for Q {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.1 == 1 {
            write!(f, "{}", self.0)
        } else {
            write!(f, "{}/{}", self.0, self.1)
        }
    }
}

fn gcd(a: u64, b: u64) -> u64 {
    if b == 0 {
        a
    } else {
        gcd(b, a % b)
    }
}

/// The lcm of two positive integers.
pub fn lcm(a: i64, b: i64) -> i64 {
    a / gcd(a.unsigned_abs(), b.unsigned_abs()) as i64 * b
}

/// The case of Chebyshev's theorem a binomial differential falls in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Case {
    /// `p` an integer: substitute `x = t^n` with this `n`
    Integer(i64),
    /// `(m + 1)/n` an integer: `t^s = a + b x^n`, with `s`
    Second(i64),
    /// `(m + 1)/n + p` an integer: `t^s = b + a x^(-n)`, with `s`
    Third(i64),
    /// none of the three: no elementary antiderivative (the reason)
    NonElementary(String),
}

/// The case of `x^m (a + b x^n)^p` (a, b nonzero, n nonzero); `None` on overflow or `n = 0`.
pub fn decide(m: Q, n: Q, p: Q) -> Option<Case> {
    if n.0 == 0 {
        return None;
    }
    if p.is_int() {
        return Some(Case::Integer(lcm(m.1, n.1)));
    }
    let k = m.add(Q(1, 1))?.div(n)?;
    if k.is_int() {
        return Some(Case::Second(p.1));
    }
    if k.add(p)?.is_int() {
        return Some(Case::Third(p.1));
    }
    Some(Case::NonElementary(format!(
        "Chebyshev's theorem: x^m (a + b x^n)^p with m = {m}, n = {n}, p = {p}: none of p, (m + 1)/n = {k}, (m + 1)/n + p = {} is an integer",
        k.add(p)?
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_three_cases_and_the_proof() {
        // x^3 (1 + x^2)^(1/2): (m + 1)/n = 2
        assert_eq!(decide(Q(3, 1), Q(2, 1), Q(1, 2)), Some(Case::Second(2)));
        // (1 + x^4)^(-1/4): 1/4 - 1/4 = 0
        assert_eq!(decide(Q(0, 1), Q(4, 1), Q(-1, 4)), Some(Case::Third(4)));
        // x^(-2/3) (1 + x^(1/3))^(-1): p an integer, x = t^3
        assert_eq!(decide(Q(-2, 3), Q(1, 3), Q(-1, 1)), Some(Case::Integer(3)));
        // (1 - x^4)^(-1/2) and (1 + x^3)^(1/2): elliptic, not elementary
        assert!(matches!(decide(Q(0, 1), Q(4, 1), Q(-1, 2)), Some(Case::NonElementary(_))));
        assert!(matches!(decide(Q(0, 1), Q(3, 1), Q(1, 2)), Some(Case::NonElementary(_))));
        // x^(1/2) (1 + x)^(1/3): not elementary
        assert!(matches!(decide(Q(1, 2), Q(1, 1), Q(1, 3)), Some(Case::NonElementary(_))));
    }
}
