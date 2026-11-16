//! The inverse nome m(q) = theta_2(q)^4/theta_3(q)^4 (DLMF 20.9.1 with 22.2.2) on real balls in (-1, 1), as
//! m = 16 q A^4/B^4 with A = sum_(n>=0) q^(n(n+1)) (theta_2(0, q) = 2 q^(1/4) A, DLMF 20.2.2) and
//! B = 1 + 2 sum_(n>=1) q^(n^2) (20.2.3). Both sums are real for every real q (the factor q^(1/4) appears to the
//! fourth power), B = theta_3 > 0 by the product (20.4.4), and with r >= |q| the tails after N terms are at most
//! r^(N(N+1))/(1 - r) and 2 r^(N^2)/(1 - r) (the exponents grow by at least 2N + 1 a term). The ball arithmetic encloses
//! the value at every point of the argument ball, so this is the ball form over the whole interval (-1, 1), negative
//! nomes included (there the theta quotient of complex balls gave a complex value with a zero imaginary ball).
use super::{f64_of, mag_up, widen};
use mc_arith::num::{stopped, Ball, Mag};

/// m(q) for a real ball q with |q| < 1 (`None` near 1 in modulus, where the series needs more than 20000 terms).
pub fn inverse_nome(q: &Ball, wp: u64) -> Option<Ball> {
    let r = f64_of(&mag_up(q).to_float());
    if !(r.is_finite() && r < 1.0) {
        return None;
    }
    if r == 0.0 {
        return Some(Ball::from_i64(0));
    }
    let w = wp + 32;
    let lr = -r.log2();
    // r^(N^2) <= 2^-(w + 8)
    let n = (((w + 8) as f64 / lr).sqrt().ceil() as u64).max(2);
    if n > 20_000 {
        return None;
    }
    let q2 = q.mul(q, w);
    // A: q^(k(k+1)), ratio q^(2(k+1)); B: q^(k^2), ratio q^(2k+1)
    let (mut a_sum, mut a_term, mut a_step) = (Ball::from_i64(1), Ball::from_i64(1), q2.clone());
    let (mut b_sum, mut b_term, mut b_step) = (Ball::from_i64(1), Ball::from_i64(1), q.clone());
    for k in 1..n {
        if k % 64 == 0 && stopped() {
            return None;
        }
        a_term = a_term.mul(&a_step, w);
        a_sum = a_sum.add(&a_term, w);
        a_step = a_step.mul(&q2, w);
        b_term = b_term.mul(&b_step, w);
        b_sum = b_sum.add(&b_term.mul(&Ball::from_i64(2), w), w);
        b_step = b_step.mul(&q2, w);
    }
    // the tails, as powers of two above r^(N(N+1))/(1 - r) and 2 r^(N^2)/(1 - r) (one bit of margin for the rounding
    // of the logarithms)
    let nf = n as f64;
    let l1r = (1.0 - r).log2();
    let ta = Mag::pow2((-(nf * (nf + 1.0)) * lr - l1r + 1.0).ceil() as i64);
    let tb = Mag::pow2((-(nf * nf) * lr - l1r + 2.0).ceil() as i64);
    let a = widen(a_sum, &ta);
    let b = widen(b_sum, &tb);
    let a2 = a.mul(&a, w);
    let b2 = b.mul(&b, w);
    let ratio = a2.mul(&a2, w).div(&b2.mul(&b2, w), w)?;
    Some(ratio.mul(q, w).mul(&Ball::from_i64(16), w))
}
