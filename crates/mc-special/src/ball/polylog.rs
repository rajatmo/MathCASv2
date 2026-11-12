//! The polylogarithm Li_s(z) = sum_{k>=1} z^k / k^s on balls, real s and real z <= 1 (decision SF.4); z > 1 (a
//! complex value) and z = 1 with s <= 1 (a pole) have no value.
//!
//! - **The defining series** for |z| <= 0.6, and for |z| <= 1 when s >= max(wp/4, 2). Two tail bounds, the better
//!   one used: the term ratio |z| (k/(k+1))^s is at most rho = |z| max(1, (1 + 1/K)^-s) for k >= K, so for rho < 1
//!   the tail from t_K on is at most |t_K| / (1 - rho); and for s > 1, sum_{k>=K} |z|^k k^-s <=
//!   |z|^K K^-s (1 + K/(s-1)) (the integral bound). k^-s is multiplicative: exp(-s log p) at primes only.
//! - **Integers s = -n <= 0**: z A_n(z) / (1 - z)^(n+1) with the exact Eulerian numbers (n <= 300); s = 1:
//!   -log1p(-z).
//! - **1/2 < z < 1**, mu = log z: Li_s(e^mu) = Gamma(1 - s) (-mu)^(s-1) + sum_k zeta(s - k) mu^k / k! for
//!   non-integer s; for integers n >= 2 the term k = n - 1 is mu^(n-1)/(n-1)! (H_(n-1) - log(-mu)) and
//!   zeta(1 - 2j) = -B_2j/(2j) is exact. Tails: for k >= K >= s, the functional equation gives
//!   |zeta(s - k)| <= 2 (2 pi)^(s-k-1) Gamma(1-s+k) zeta(1-s+k), so the terms are bounded by
//!   2 G_k zeta(1-s+K) |mu|^k with G_k = Gamma(1-s+k)(2 pi)^(s-k-1)/k!, a geometric sequence of ratio
//!   (|mu|/2 pi) max(1, (K+1-s)/(K+1)) from k = K on. For integers |B_2j|/(2j)! <= 2 zeta(2)/(2 pi)^2j <= 4/(2 pi)^2j
//!   and (2j-1)!/(n-1+2j)! decreases in j.
//! - **z < -1/2** (z >= -1 for integers, z >= -e^(0.9 pi) otherwise), lambda = log(-z):
//!   Li_s(-e^lambda) = -sum_k eta(s - k) lambda^k / k!, eta(x) = (1 - 2^(1-x)) zeta(x), with the same bounds
//!   (|1 - 2^(1-x)| <= 1 + 2^(1-x): ratio (|lambda|/pi) max(1, (K+1-s)/(K+1)); integers:
//!   (2^2j - 1)|B_2j|/(2j)! <= 4/pi^2j).
//! - **Integers n >= 1, z < -1** (inversion): Li_n(z) = -(-1)^n Li_n(1/z) - L^n/n! -
//!   2 sum_{k=1}^{n/2} eta(2k) L^(n-2k)/(n-2k)!, L = ln(-z), zeta(2k) from exact Bernoulli numbers.
//!
//! zeta(s - k) for the non-integer orders: Euler-Maclaurin sums with the remainder bound of [`super::zeta`]
//! evaluated rigorously at each argument, the arguments in rows a, a + 1, a + 2, ... sharing the powers n^-t (each
//! step multiplies them by 1/n); once N^(1-t)/(t-1) < 2^-wp the direct sum with the integral tail bound
//! sum_{n>=N} n^-t <= N^-t (1 + N/(t-1)), over a shrinking range of n.
use super::gamma::gamma_ball;
use super::{
    cos_pi, exact_int, f64_of, float_of, int, log2_up, lower, mag_up, pi, rat_ball, real, real_ball, refine, sin_pi,
    upper, widen,
};
use crate::bernoulli::b2n;
use mc_arith::num::{stopped, Ball, Int, Mag, Nat, Rat};
use mc_arith::Real;
use mc_numeric::cball::CBall;
use mc_numeric::elementary as el;

/// The largest number of series terms before giving up.
const MAX_TERMS: u64 = 1 << 20;
/// The largest |n| of Li_-n with exact Eulerian numbers.
const MAX_EULERIAN: u64 = 300;
/// The largest |log(-z)| / pi of the eta expansion (non-integer s).
const ETA_REACH: f64 = 0.9;

/// An upper bound of a ball's magnitude as a double (rounded up).
fn up_f(b: &Ball) -> f64 {
    let v = f64_of(&mag_up(b).to_float());
    v * (1.0 + 1e-12) + f64::MIN_POSITIVE
}

/// A double rounded up as a magnitude.
fn mag_f(v: f64) -> Mag {
    Mag::from_float_up(&float_of(v * (1.0 + 1e-12)))
}

fn fact(n: u64) -> Nat {
    (2..=n).fold(Nat::from_u64(1), |f, k| f.mul_u64(k))
}

/// B_2j / (2j)! as an exact rational.
fn bern_fact(j: u64) -> Option<Rat> {
    Some(b2n(j)?.mul(&Rat::new(Int::from_i64(1), Int::from_nat(fact(2 * j)))))
}

/// The (N, M) choice of [`super::zeta`]: the cheapest Euler-Maclaurin parameters with a remainder below 2^-target
/// (followed in machine arithmetic only to choose; the bound itself is computed with balls).
fn choose(sigma: f64, a: f64, target: f64) -> Option<(u64, u64)> {
    let l2pi = (2.0 * std::f64::consts::PI).log2();
    let mut best: Option<(u64, u64, f64)> = None;
    let mut n = 2u64;
    while n < 1 << 22 {
        if stopped() {
            return None;
        }
        let ln = (n as f64).log2();
        let mut poch = 0.0;
        let mut prev = f64::INFINITY;
        for m in 1..=(4 * target as u64 + 64) {
            let mf = m as f64;
            poch += (a + 2.0 * mf - 2.0).max(1e-300).log2() + (a + 2.0 * mf - 1.0).max(1e-300).log2();
            let den = sigma + 2.0 * mf - 1.0;
            if den <= 0.0 {
                continue;
            }
            let b = 2.0 + poch - 2.0 * mf * l2pi + (1.0 - sigma - 2.0 * mf) * ln - den.log2();
            if b < -target {
                let cost = 4.0 * n as f64 + mf;
                if best.is_none_or(|(_, _, c)| cost < c) {
                    best = Some((n, m, cost));
                }
                break;
            }
            if b > prev || best.is_some_and(|(_, _, c)| 4.0 * n as f64 + mf > c) {
                break;
            }
            prev = b;
        }
        if best.is_some_and(|(_, _, c)| 4.0 * n as f64 > c) {
            break;
        }
        n = (n * 5).div_ceil(4).max(n + 1);
    }
    best.map(|(n, m, _)| (n, m))
}

/// k^-s for k = 1, 2, ... by multiplicativity.
struct Powers {
    s: Ball,
    s_int: Option<Int>,
    v: Vec<Ball>,
    wp: u64,
}

impl Powers {
    fn new(s: &Ball, wp: u64) -> Powers {
        let s_int = super::float_int(&s.mid).filter(|_| s.rad.is_zero());
        Powers { s: s.clone(), s_int, v: vec![int(0), int(1)], wp }
    }
    fn get(&mut self, k: usize) -> Option<Ball> {
        let wp = self.wp;
        while self.v.len() <= k {
            let m = self.v.len();
            let x = match (2..).take_while(|d| d * d <= m).find(|d| m.is_multiple_of(*d)) {
                Some(d) => self.v[d].mul(&self.v[m / d], wp),
                None => match &self.s_int {
                    Some(n) => el::pow_int(&int(m as i64), &n.neg(), wp)?,
                    None => el::exp(&el::log(&int(m as i64), wp)?.mul(&self.s, wp).neg(), wp)?,
                },
            };
            self.v.push(x);
        }
        Some(self.v[k].clone())
    }
}

// ---------------------------------------------------------------- zeta rows

/// zeta(a), zeta(a + 1), ... for a ball a with lower end > 0 (excluding 1): Euler-Maclaurin with shared powers.
struct ZetaRow {
    t: Ball,
    m: u64,
    /// n^-t for n = 0..=N (index 0 unused); the active length shrinks once the direct sum suffices
    pw: Vec<Ball>,
    inv: Vec<Ball>,
    coef: Vec<Ball>,
    twopi2m: Ball,
    wp: u64,
}

impl ZetaRow {
    fn new(a: &Ball, wp: u64) -> Option<ZetaRow> {
        let sigma = f64_of(&lower(a));
        if sigma <= 0.0 {
            return None;
        }
        let amax = f64_of(&upper(a)).abs();
        let (n, m) = choose(sigma, amax, wp as f64 + 8.0)?;
        let mut pows = Powers::new(a, wp);
        let mut pw = Vec::with_capacity(n as usize + 1);
        let mut inv = Vec::with_capacity(n as usize + 1);
        for k in 0..=n as usize {
            if k % 32 == 0 && stopped() {
                return None;
            }
            pw.push(if k == 0 { int(0) } else { pows.get(k)? });
            inv.push(if k == 0 { int(0) } else { int(1).div(&int(k as i64), wp)? });
        }
        let coef = (1..=m).map(|j| Some(rat_ball(&bern_fact(j)?, wp))).collect::<Option<Vec<_>>>()?;
        let twopi2m = el::pow_int(&pi(wp)?.mul(&int(2), wp), &Int::from_u64(2 * m), wp)?;
        Some(ZetaRow { t: a.clone(), m, pw, inv, coef, twopi2m, wp })
    }

    /// zeta(t) for the current t, then t + 1.
    fn next(&mut self) -> Option<Ball> {
        let wp = self.wp;
        let t = self.t.clone();
        let t_lo = f64_of(&lower(&t));
        let n = self.pw.len() - 1;
        let nf = n as f64;
        let mut sum = int(0);
        let out = if t_lo > 2.0 && (t_lo - 1.0) * nf.log2() > wp as f64 + 16.0 {
            // the direct sum over k < K with K^(1-t)/(t-1) < 2^-wp - 16, K <= N
            let mut k_end = n;
            while k_end > 2 && (t_lo - 1.0) * ((k_end - 1) as f64).log2() > wp as f64 + 16.0 {
                k_end -= 1;
            }
            for k in (1..k_end).rev() {
                sum = sum.add(&self.pw[k], wp);
            }
            let tail = mag_up(&self.pw[k_end]).mul(&mag_f(1.0 + k_end as f64 / (t_lo - 1.0)));
            self.pw.truncate(k_end + 1);
            widen(sum, &tail)
        } else {
            for k in (1..n).rev() {
                sum = sum.add(&self.pw[k], wp);
            }
            let nn = &self.pw[n];
            let nb = int(n as i64);
            let one = int(1);
            sum = sum.add(&nb.mul(nn, wp).div(&t.sub(&one, wp), wp)?, wp);
            sum = sum.add(&nn.mul(&Ball::from_ratio(1, 2, wp), wp), wp);
            let r2 = self.inv[n].mul(&self.inv[n], wp);
            let mut poch = t.clone();
            let mut w = nn.mul(&self.inv[n], wp);
            for j in 1..=self.m {
                sum = sum.add(&self.coef[j as usize - 1].mul(&poch, wp).mul(&w, wp), wp);
                poch = poch.mul(&t.add(&int(2 * j as i64 - 1), wp), wp).mul(&t.add(&int(2 * j as i64), wp), wp);
                w = w.mul(&r2, wp);
            }
            // 4 |(t)_2M| N^(1-2M) max|N^-t| / ((2 pi)^2M (sigma + 2M - 1))
            let mut pm = int(1);
            for i in 0..2 * self.m {
                pm = pm.mul(&Ball::exact(mag_up(&t.add(&int(i as i64), wp)).to_float()), wp);
            }
            let npow = el::pow_int(&nb, &Int::from_i64(1 - 2 * self.m as i64), wp)?;
            let den = Ball::exact(lower(&t)).add(&int(2 * self.m as i64 - 1), wp);
            if !super::is_pos(&den) {
                return None;
            }
            let b = int(4).mul(&pm, wp).mul(&npow, wp).mul(&Ball::exact(mag_up(nn).to_float()), wp);
            let b = b.div(&self.twopi2m.mul(&den, wp), wp)?;
            widen(sum, &mag_up(&b))
        };
        for k in 2..self.pw.len() {
            self.pw[k] = self.pw[k].mul(&self.inv[k], wp);
        }
        self.t = t.add(&int(1), wp);
        Some(out)
    }
}

// ---------------------------------------------------------------- the defining series

/// sum_{k>=1} z^k k^-s with a rigorous tail bound.
fn direct(s: &Ball, z: &Ball, wp: u64) -> Option<Ball> {
    let za = up_f(z);
    let s_lo = f64_of(&lower(s));
    let mut pw = Powers::new(s, wp);
    let mut zk = z.clone();
    let mut sum = z.clone();
    let mut big = log2_up(&sum);
    for k in 2..MAX_TERMS {
        if k % 32 == 0 && stopped() {
            return None;
        }
        zk = zk.mul(z, wp);
        let t = zk.mul(&pw.get(k as usize)?, wp);
        let kf = k as f64;
        let target = big - wp as f64 - 4.0;
        // the geometric bound from term k on
        let rho = za * if s_lo < 0.0 { (1.0 + 1.0 / kf).powf(-s_lo) * (1.0 + 1e-12) } else { 1.0 };
        if rho < 1.0 && log2_up(&t) - (1.0 - rho).log2() < target {
            return Some(widen(sum, &mag_up(&t).mul(&mag_f(1.0 / (1.0 - rho)))));
        }
        // the integral bound |z|^k k^-s (1 + k/(s-1)) for s > 1
        if s_lo > 1.0 && za <= 1.0 {
            let lb = za.log2() * kf - s_lo * kf.log2() + (1.0 + kf / (s_lo - 1.0)).log2() + 1e-9;
            if lb < target {
                // |z|^k k^-s over the ball: bounded by the term's own magnitude bound when the term is exact
                let tb = mag_up(&t).mul(&mag_f(1.0 + kf / (s_lo - 1.0)));
                let tb2 = mag_f(2f64.powf(lb));
                return Some(widen(sum, &if tb.log2() < tb2.log2() { tb } else { tb2 }));
            }
        }
        sum = sum.add(&t, wp);
        big = big.max(log2_up(&sum));
    }
    None
}

// ---------------------------------------------------------------- integer orders

/// The Eulerian numbers A(n, 0..n) (the row of Li_-n's numerator), n >= 1, by the recurrence.
fn eulerian_row(n: u64) -> Option<Vec<Nat>> {
    let mut row = vec![Nat::from_u64(1)];
    for m in 2..=n as usize {
        if stopped() {
            return None;
        }
        let mut next = vec![Nat::from_u64(0); m];
        for (k, r) in next.iter_mut().enumerate() {
            let mut v = Nat::from_u64(0);
            if k < m - 1 {
                v = v.add(&row[k].mul_u64(k as u64 + 1));
            }
            if k > 0 {
                v = v.add(&row[k - 1].mul_u64((m - k) as u64));
            }
            *r = v;
        }
        row = next;
    }
    Some(row)
}

/// Li_-n(z) on complex balls, n <= 300, z != 1.
pub(crate) fn eulerian_c(n: u64, z: &CBall, wp: u64) -> Option<CBall> {
    if n > MAX_EULERIAN {
        return None;
    }
    let omz = CBall::from_i64(1).sub(z, wp);
    if n == 0 {
        return z.div(&omz, wp);
    }
    let row = eulerian_row(n)?;
    let mut p = CBall::from_i64(0);
    for c in row.iter().rev() {
        p = p.mul(z, wp).add(&CBall::real(Ball::from_nat(c.clone())), wp);
    }
    let d = mc_numeric::cball::pow_int(&omz, &Int::from_u64(n + 1), wp)?;
    z.mul(&p, wp).div(&d, wp)
}

/// Li_-n(z) = z A_n(z) / (1 - z)^(n+1), n <= 300, z != 1.
fn eulerian(n: u64, z: &Ball, wp: u64) -> Option<Ball> {
    let one = int(1);
    let omz = one.sub(z, wp);
    if n == 0 {
        return z.div(&omz, wp);
    }
    // the row A(n, 0..n) by the recurrence
    let mut row = vec![Nat::from_u64(1)];
    for m in 2..=n as usize {
        if stopped() {
            return None;
        }
        let mut next = vec![Nat::from_u64(0); m];
        for (k, r) in next.iter_mut().enumerate() {
            let mut v = Nat::from_u64(0);
            if k < m - 1 {
                v = v.add(&row[k].mul_u64(k as u64 + 1));
            }
            if k > 0 {
                v = v.add(&row[k - 1].mul_u64((m - k) as u64));
            }
            *r = v;
        }
        row = next;
    }
    let mut p = int(0);
    for c in row.iter().rev() {
        p = p.mul(z, wp).add(&Ball::from_nat(c.clone()), wp);
    }
    let d = el::pow_int(&omz, &Int::from_u64(n + 1), wp)?;
    z.mul(&p, wp).div(&d, wp)
}

/// zeta(k) for an integer k >= 2: exact Bernoulli numbers for even k, the Euler-Maclaurin ball otherwise.
fn zeta_int(k: u64, wp: u64) -> Option<Ball> {
    if k.is_multiple_of(2) {
        // |B_k| (2 pi)^k / (2 k!)
        let b = b2n(k / 2)?;
        let b = Rat::new(Int::new(false, b.num.mag.clone()), Int::from_nat(b.den.clone()));
        let c = rat_ball(&b.mul(&Rat::new(Int::from_i64(1), Int::from_nat(fact(k).mul_u64(2)))), wp);
        let tp = el::pow_int(&pi(wp)?.mul(&int(2), wp), &Int::from_u64(k), wp)?;
        return Some(c.mul(&tp, wp));
    }
    super::zeta::zeta_ball(&int(k as i64), wp)
}

/// The sum over j >= 1 of c_j R_j x^(n-1+2j), R_j = (2j-1)!/(n-1+2j)!, with c_j = sign B_2j/(2j)! (mu expansion,
/// `eta` false) or (2^2j - 1) B_2j/(2j)! (eta expansion), and its tail bound.
fn bernoulli_part(n: u64, x: &Ball, eta: bool, wp: u64) -> Option<Ball> {
    if x.mid.is_zero() && x.rad.is_zero() {
        return Some(int(0));
    }
    let x2 = x.mul(x, wp);
    let mut pw = el::pow_int(x, &Int::from_u64(n + 1), wp)?;
    let mut r = int(1).div(&Ball::from_nat(fact(n + 1)), wp)?;
    let xa = up_f(x);
    let q = if eta { xa / std::f64::consts::PI } else { xa / (2.0 * std::f64::consts::PI) };
    let q2 = q * q * (1.0 + 1e-12);
    if q2 >= 0.9 {
        return None;
    }
    let mut sum = int(0);
    let mut big = f64::NEG_INFINITY;
    for j in 1..MAX_TERMS {
        if stopped() {
            return None;
        }
        // bound of the terms from j on: 4 R_j |x|^(n-1+2j) / (2 pi or pi)^2j / (1 - q^2)
        let qj = (2 * j) as f64 * q.log2();
        let lb = 2.0 + log2_up(&r) + log2_up(&pw) - (2 * j) as f64 * xa.log2() + qj - (1.0 - q2).log2() + 1e-9;
        if lb < big.max(log2_up(&sum)) - wp as f64 - 4.0 && j > 1 {
            let tb = mag_up(&r).mul(&mag_up(&pw)).mul(&mag_f(4.0 * 2f64.powf(qj - (2 * j) as f64 * xa.log2())));
            return Some(widen(sum, &tb.mul(&mag_f(1.0 / (1.0 - q2)))));
        }
        let mut c = bern_fact(j)?;
        if eta {
            let four = Rat::from_int(Int::from_nat(Nat::pow2(2 * j)).sub(&Int::from_i64(1)));
            c = c.mul(&four);
        } else {
            c = c.neg();
        }
        let t = rat_ball(&c, wp).mul(&r, wp).mul(&pw, wp);
        sum = sum.add(&t, wp);
        big = big.max(log2_up(&sum));
        pw = pw.mul(&x2, wp);
        let num = (2 * j * (2 * j + 1)) as i64;
        let den = ((n + 2 * j) * (n + 2 * j + 1)) as i64;
        r = r.mul(&int(num), wp).div(&int(den), wp)?;
    }
    None
}

/// Li_n(e^mu), integer n >= 2, mu < 0 real.
fn mu_int(n: u64, mu: &Ball, wp: u64) -> Option<Ball> {
    let mut sum = int(0);
    let mut pk = int(1);
    for k in 0..n - 1 {
        sum = sum.add(&pk.mul(&zeta_int(n - k, wp)?, wp), wp);
        pk = pk.mul(mu, wp).div(&int(k as i64 + 1), wp)?;
    }
    let mut h = int(0);
    for i in 1..n {
        h = h.add(&int(1).div(&int(i as i64), wp)?, wp);
    }
    sum = sum.add(&pk.mul(&h.sub(&el::log(&mu.neg(), wp)?, wp), wp), wp);
    pk = pk.mul(mu, wp).div(&int(n as i64), wp)?;
    sum = sum.sub(&pk.mul(&Ball::from_ratio(1, 2, wp), wp), wp);
    Some(sum.add(&bernoulli_part(n, mu, false, wp)?, wp))
}

/// Li_n(-e^lambda), integer n >= 1, |lambda| < pi.
fn eta_int(n: u64, lam: &Ball, wp: u64) -> Option<Ball> {
    let mut sum = int(0);
    let mut pk = int(1);
    for k in 0..n.saturating_sub(1) {
        let e = int(1).sub(&Ball::exact(mc_arith::num::Float::new(false, Nat::from_u64(1), -((n - 1 - k) as i64))), wp);
        sum = sum.add(&pk.mul(&e, wp).mul(&zeta_int(n - k, wp)?, wp), wp);
        pk = pk.mul(lam, wp).div(&int(k as i64 + 1), wp)?;
    }
    sum = sum.add(&pk.mul(&el::log(&int(2), wp)?, wp), wp);
    pk = pk.mul(lam, wp).div(&int(n as i64), wp)?;
    sum = sum.add(&pk.mul(&Ball::from_ratio(1, 2, wp), wp), wp);
    Some(sum.add(&bernoulli_part(n, lam, true, wp)?, wp).neg())
}

/// -L^n/n! - 2 sum_{k=1}^{n/2} eta(2k) L^(n-2k)/(n-2k)!.
fn inversion_term(n: u64, l: &Ball, wp: u64) -> Option<Ball> {
    let mut sum = el::pow_int(l, &Int::from_u64(n), wp)?.div(&Ball::from_nat(fact(n)), wp)?;
    for k in 1..=n / 2 {
        let e = int(1).sub(&Ball::exact(mc_arith::num::Float::new(false, Nat::from_u64(1), 1 - 2 * k as i64)), wp);
        let lp = el::pow_int(l, &Int::from_u64(n - 2 * k), wp)?.div(&Ball::from_nat(fact(n - 2 * k)), wp)?;
        sum = sum.add(&e.mul(&zeta_int(2 * k, wp)?, wp).mul(&lp, wp).mul(&int(2), wp), wp);
    }
    Some(sum.neg())
}

/// Li_n(z) for an integer n >= 2 and a real ball z < 1 (not containing 1).
fn int_point(n: u64, z: &Ball, wp: u64) -> Option<Ball> {
    let za = up_f(z);
    let lo = f64_of(&lower(z));
    let hi = f64_of(&upper(z));
    if za <= 0.6 || (n as f64 >= (wp as f64 / 4.0).max(2.0) && za <= 1.0) {
        return direct(&int(n as i64), z, wp);
    }
    if lo > 0.5 {
        return mu_int(n, &el::log(z, wp)?, wp);
    }
    if hi < -0.5 && lo > -1.5 {
        return eta_int(n, &el::log(&z.neg(), wp)?, wp);
    }
    if hi < -1.0 {
        let zi = int(1).div(z, wp)?;
        let r = int_point(n, &zi, wp)?;
        let r = if n.is_multiple_of(2) { r.neg() } else { r };
        return Some(r.add(&inversion_term(n, &el::log(&z.neg(), wp)?, wp)?, wp));
    }
    None
}

// ---------------------------------------------------------------- non-integer orders

/// sum_k w_k zeta(s - k) x^k / k! for a non-integer ball s, w_k = 1 (`eta` false) or 1 - 2^(1-s+k), with the tail
/// bound of the module documentation.
fn zeta_series(s: &Ball, x: &Ball, eta: bool, wp: u64) -> Option<Ball> {
    let one = int(1);
    let s_lo = f64_of(&lower(s));
    let s_hi = f64_of(&upper(s));
    if s_lo.floor() != s_hi.floor() || s_lo == s_lo.floor() {
        return None;
    }
    let k0 = if s_lo > 0.0 { s_lo.floor() as u64 + 1 } else { 0 };
    let two_1ms = if eta { Some(el::exp(&el::log(&int(2), wp)?.mul(&one.sub(s, wp), wp), wp)?) } else { None };
    let weight = |k: u64| -> Option<Ball> {
        Some(match &two_1ms {
            Some(q) => one.sub(&q.mul(&el::pow_int(&int(2), &Int::from_u64(k), wp)?, wp), wp),
            None => one.clone(),
        })
    };
    let mut sum = int(0);
    let mut xk = int(1);
    let mut big = f64::NEG_INFINITY;
    if k0 > 0 {
        let mut row = ZetaRow::new(&s.sub(&int(k0 as i64 - 1), wp), wp)?;
        let mut vals = Vec::with_capacity(k0 as usize);
        for _ in 0..k0 {
            vals.push(row.next()?);
        }
        let mut f = int(1);
        for k in 0..k0 {
            let c = vals[(k0 - 1 - k) as usize].mul(&f, wp).mul(&weight(k)?, wp);
            sum = sum.add(&c.mul(&xk, wp), wp);
            big = big.max(log2_up(&sum));
            xk = xk.mul(x, wp);
            f = f.div(&int(k as i64 + 1), wp)?;
        }
    }
    // k >= k0: zeta(s-k)/k! = 2 sin(pi (s-k)/2) G_k zeta(1-s+k)
    let a = int(k0 as i64 + 1).sub(s, wp);
    let mut row = ZetaRow::new(&a, wp)?;
    let twopi = pi(wp)?.mul(&int(2), wp);
    let gam = gamma_ball(&a, wp)?;
    let pw = el::exp(&s.sub(&int(k0 as i64 + 1), wp).mul(&el::log(&twopi, wp)?, wp), wp)?;
    let mut g = gam.mul(&pw, wp).div(&Ball::from_nat(fact(k0)), wp)?;
    let half = s.mul(&Ball::from_ratio(1, 2, wp), wp);
    let (sn, cs) = (sin_pi(&half, wp)?, cos_pi(&half, wp)?);
    let xa = up_f(x);
    let base = if eta { xa / std::f64::consts::PI } else { xa / (2.0 * std::f64::consts::PI) };
    for k in k0..MAX_TERMS {
        if stopped() {
            return None;
        }
        let z = row.next()?;
        let w = weight(k)?;
        let kf = k as f64;
        let rho = base * ((kf + 1.0 - s_lo) / (kf + 1.0)).max(1.0) * (1.0 + 1e-12);
        // the size of the terms from k on (the sine factor at most 1)
        let tb = mag_up(&g).mul(&mag_up(&z)).mul(&mag_up(&w)).mul(&mag_up(&xk)).mul_2exp(1);
        if tb.is_zero() && k > k0 {
            // x = 0: only the constant term
            return Some(sum);
        }
        if rho < 1.0 && k > k0 + 1 && tb.log2() - (1.0 - rho).log2() < big - wp as f64 - 4.0 {
            return Some(widen(sum, &tb.mul(&mag_f(1.0 / (1.0 - rho)))));
        }
        let sf = match k % 4 {
            0 => sn.clone(),
            1 => cs.neg(),
            2 => sn.neg(),
            _ => cs.clone(),
        };
        let t = g.mul(&z, wp).mul(&sf, wp).mul(&w, wp).mul(&xk, wp).mul(&int(2), wp);
        sum = sum.add(&t, wp);
        big = big.max(log2_up(&sum));
        xk = xk.mul(x, wp);
        // G_(k+1) = G_k (k + 1 - s) / ((k + 1) 2 pi)
        g = g.mul(&int(k as i64 + 1).sub(s, wp), wp).div(&twopi.mul(&int(k as i64 + 1), wp), wp)?;
    }
    None
}

/// sum_k zeta(s - k) mu^k / k! for a non-integer real ball s and a complex ball mu (|mu| < 2 pi): the series of
/// [`zeta_series`] (eta false) with a complex variable; the coefficients are real and the tail bound depends on the
/// modulus of mu only, so it is the same.
fn zeta_series_c(s: &Ball, mu: &CBall, wp: u64) -> Option<CBall> {
    let s_lo = f64_of(&lower(s));
    let s_hi = f64_of(&upper(s));
    if s_lo.floor() != s_hi.floor() || s_lo == s_lo.floor() {
        return None;
    }
    let cmod = |z: &CBall| up_f(&z.re) + up_f(&z.im);
    let k0 = if s_lo > 0.0 { s_lo.floor() as u64 + 1 } else { 0 };
    let mut sum = CBall::from_i64(0);
    let mut xk = CBall::from_i64(1);
    let mut big = f64::NEG_INFINITY;
    if k0 > 0 {
        let mut row = ZetaRow::new(&s.sub(&int(k0 as i64 - 1), wp), wp)?;
        let mut vals = Vec::with_capacity(k0 as usize);
        for _ in 0..k0 {
            vals.push(row.next()?);
        }
        let mut f = int(1);
        for k in 0..k0 {
            let c = vals[(k0 - 1 - k) as usize].mul(&f, wp);
            sum = sum.add(&xk.mul_real(&c, wp), wp);
            big = big.max(cmod(&sum).log2());
            xk = xk.mul(mu, wp);
            f = f.div(&int(k as i64 + 1), wp)?;
        }
    }
    // k >= k0: zeta(s-k)/k! = 2 sin(pi (s-k)/2) G_k zeta(1-s+k)
    let a = int(k0 as i64 + 1).sub(s, wp);
    let mut row = ZetaRow::new(&a, wp)?;
    let twopi = pi(wp)?.mul(&int(2), wp);
    let gam = gamma_ball(&a, wp)?;
    let pw = el::exp(&s.sub(&int(k0 as i64 + 1), wp).mul(&el::log(&twopi, wp)?, wp), wp)?;
    let mut g = gam.mul(&pw, wp).div(&Ball::from_nat(fact(k0)), wp)?;
    let half = s.mul(&Ball::from_ratio(1, 2, wp), wp);
    let (sn, cs) = (sin_pi(&half, wp)?, cos_pi(&half, wp)?);
    let base = cmod(mu) / (2.0 * std::f64::consts::PI);
    for k in k0..MAX_TERMS {
        if stopped() {
            return None;
        }
        let z = row.next()?;
        let kf = k as f64;
        let rho = base * ((kf + 1.0 - s_lo) / (kf + 1.0)).max(1.0) * (1.0 + 1e-12);
        let tb = mag_up(&g).mul(&mag_up(&z)).mul(&mag_f(cmod(&xk))).mul_2exp(1);
        if tb.is_zero() && k > k0 {
            return Some(sum);
        }
        if rho < 1.0 && k > k0 + 1 && tb.log2() - (1.0 - rho).log2() < big - wp as f64 - 4.0 {
            let m = tb.mul(&mag_f(1.0 / (1.0 - rho)));
            return Some(CBall::new(widen(sum.re, &m), widen(sum.im, &m)));
        }
        let sf = match k % 4 {
            0 => sn.clone(),
            1 => cs.neg(),
            2 => sn.neg(),
            _ => cs.clone(),
        };
        let t = g.mul(&z, wp).mul(&sf, wp).mul(&int(2), wp);
        sum = sum.add(&xk.mul_real(&t, wp), wp);
        big = big.max(cmod(&sum).log2());
        xk = xk.mul(mu, wp);
        g = g.mul(&int(k as i64 + 1).sub(s, wp), wp).div(&twopi.mul(&int(k as i64 + 1), wp), wp)?;
    }
    None
}

/// Li_s(z) for a non-integer real ball s and a complex ball z with |log z| <= 4.5, by the expansion about z = 1,
/// Li_s(e^mu) = Gamma(1 - s) (-mu)^(s-1) + sum_k zeta(s - k) mu^k/k! (|mu| < 2 pi; DLMF 25.12.12), on the principal
/// branch; `on_cut` (z real and above 1) takes the limit from below, arg(-mu) = +pi.
pub(crate) fn frac_point_c(s: &Ball, z: &CBall, on_cut: bool, wp: u64) -> Option<CBall> {
    let one = int(1);
    let mu = if on_cut { CBall::new(el::log(&z.re, wp)?, int(0)) } else { mc_numeric::cball::log(z, wp)? };
    if up_f(&mu.re) + up_f(&mu.im) > 4.5 {
        return None;
    }
    let lg = if on_cut {
        CBall::new(el::log(&mu.re, wp)?, pi(wp)?)
    } else {
        mc_numeric::cball::log(&mu.neg(), wp)?
    };
    let gam = gamma_ball(&one.sub(s, wp), wp)?;
    let sing = mc_numeric::cball::exp(&lg.mul_real(&s.sub(&one, wp), wp), wp)?.mul_real(&gam, wp);
    Some(sing.add(&zeta_series_c(s, &mu, wp)?, wp))
}

/// Li_s(z) for a non-integer ball s and a real ball z < 1.
fn frac_point(s: &Ball, z: &Ball, wp: u64) -> Option<Ball> {
    let za = up_f(z);
    let lo = f64_of(&lower(z));
    let hi = f64_of(&upper(z));
    let s_lo = f64_of(&lower(s));
    if za <= 0.6 || (s_lo >= (wp as f64 / 4.0).max(2.0) && za <= 1.0) {
        return direct(s, z, wp);
    }
    if lo > 0.5 && hi < 1.0 {
        let mu = el::log(z, wp)?;
        let one = int(1);
        let gam = gamma_ball(&one.sub(s, wp), wp)?;
        let sing = gam.mul(&el::exp(&s.sub(&one, wp).mul(&el::log(&mu.neg(), wp)?, wp), wp)?, wp);
        return Some(sing.add(&zeta_series(s, &mu, false, wp)?, wp));
    }
    if hi < -0.5 {
        let lam = el::log(&z.neg(), wp)?;
        if up_f(&lam) > ETA_REACH * std::f64::consts::PI {
            return None;
        }
        return Some(zeta_series(s, &lam, true, wp)?.neg());
    }
    None
}

/// Li_s(z) on balls (`s_int` the exact integer value of s, if any).
fn point(s: &Ball, s_int: Option<i64>, z: &Ball, wp: u64) -> Option<Ball> {
    let one = int(1);
    if z.rad.is_zero() && z.mid.is_zero() {
        return Some(int(0));
    }
    match z.cmp3(&one) {
        Some(std::cmp::Ordering::Less) => {}
        Some(std::cmp::Ordering::Equal) => {
            // zeta(s) for s > 1
            return if super::is_pos(&s.sub(&one, wp)) { super::zeta::zeta_ball(s, wp) } else { None };
        }
        _ => return None,
    }
    match s_int {
        Some(n) if n <= 0 => {
            if n.unsigned_abs() > MAX_EULERIAN {
                return None;
            }
            eulerian(n.unsigned_abs(), z, wp)
        }
        Some(1) => Some(el::log1p(&z.neg(), wp)?.neg()),
        Some(n) => int_point(n as u64, z, wp),
        None => frac_point(s, z, wp),
    }
}

/// Li_s(z) to p bits for real s and real z <= 1 (`None` for z > 1, a pole, or an unsupported region: non-integer s
/// with z < -e^(0.9 pi), integer s < -300).
pub fn polylog(s: &Real, z: &Real, p: u64) -> Option<CBall> {
    let s_int = exact_int(s).and_then(|n| n.to_i64());
    let zb = real_ball(z, p)?;
    let guard = 24 + log2_up(&zb).max(0.0) as u64;
    refine(p, guard, |wp| {
        let sb = real_ball(s, wp)?;
        let zb = real_ball(z, wp)?;
        Some(real(point(&sb, s_int, &zb, wp)?))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q(n: i64, d: i64) -> Real {
        Real::Exact(Rat::new(Int::from_i64(n), Int::from_i64(d)))
    }

    fn digits(b: &Ball, d: usize) -> String {
        let (s, e) = b.mid.to_sci(d);
        format!("{}{s}e{e}", if b.mid.neg { "-" } else { "" })
    }

    #[test]
    fn published_values() {
        let p = 200;
        // Li_2(1/2) = pi^2/12 - ln(2)^2/2 = 0.58224052646501250590265632015968010874419847480...
        let v = polylog(&q(2, 1), &q(1, 2), p).unwrap().re;
        assert_eq!(digits(&v, 45), "582240526465012505902656320159680108744198475e-1");
        // Li_2(3/4) = 0.97846939293030610374306666652456149776148427461949...
        let v = polylog(&q(2, 1), &q(3, 4), p).unwrap().re;
        assert_eq!(digits(&v, 40), "9784693929303061037430666665245614977615e-1");
        // Li_2(-1) = -pi^2/12 = -0.82246703342411321823620758332301259460947495060...
        let v = polylog(&q(2, 1), &q(-1, 1), p).unwrap().re;
        assert_eq!(digits(&v, 40), "-8224670334241132182362075833230125946095e-1");
        assert!(polylog(&q(2, 1), &q(3, 2), p).is_none());
        assert!(polylog(&q(1, 2), &q(1, 1), p).is_none());
    }
}
