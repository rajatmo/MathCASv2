//! Functions added in round 29: `stieltjes_gamma` and `siegel_theta`.
//!
//! - **Stieltjes constants** `stieltjes_gamma(n)`, `stieltjes_gamma(n, a)`: the coefficients of the Laurent series
//!   zeta(s, a) = 1/(s - 1) + sum (-1)^n gamma_n(a) (s - 1)^n/n! (DLMF 25.11.?; Berndt 1972), from the limit
//!   gamma_n(a) = lim (sum_(k<=M) ln(k + a)^n/(k + a) - ln(M + a)^(n+1)/(n + 1)) by Euler-Maclaurin summation of
//!   f(x) = ln(x + a)^n/(x + a) from N on: gamma_n(a) = sum_(k<N) f(k) - ln(N + a)^(n+1)/(n + 1) + f(N)/2 - sum_j
//!   B_2j/(2j)! f^(2j-1)(N). The derivatives are f^(r) = t^(-1-r) P_r(ln t) with P_0 = L^n, P_(r+1) = -(1 + r) P_r +
//!   P_r' (t = x + a), exact integer polynomials. The correction terms decrease like (r/(2 pi e N))^r, the omitted
//!   ones are bounded by the first of them (added to the radius); the cancellation of the sum against the logarithm,
//!   about (n + 1) log2 ln N bits, is added to the working precision. Real a > 0.
//! - **Siegel theta** `siegel_theta(omega, s)` = sum over n in Z^g of exp(pi i n.omega.n + 2 pi i n.s) and
//!   `siegel_theta([nu1, nu2], omega, s)` = sum exp(pi i (n + nu1).omega.(n + nu1) + 2 pi i (n + nu1).(s + nu2)) (the
//!   Riemann theta function with characteristics; Mumford, Tata lectures I, II.1), for a symmetric omega with
//!   positive definite imaginary part Y. The terms have modulus exp(-pi (m.Y.m + 2 m.Im s)), m = n + nu1, largest at
//!   m = -Y^-1 Im s: the sum runs over the box around that center whose quadratic form exceeds the target (the
//!   smallest eigenvalue of Y bounds the form from below), with the bound of the omitted terms added to the radius.
use crate::cover::{mag, parts, work};
use mc_api::T;
use mc_arith::num::Ball;
use mc_arith::{Num, Real};
use mc_eval::Res;
use mc_numeric::formula::{Def, Fx};

type R<'h> = Res<Option<T<'h>>>;

// ---------------------------------------------------------------- Stieltjes constants

/// gamma_n(a) as a ball at wp bits.
fn stieltjes_ball(n: u64, a: &Ball, wp: u64) -> Option<Ball> {
    let af = crate::ball::f64_of(&a.mid);
    if af.is_nan() || af <= 0.0 {
        return None;
    }
    let nf = n as f64;
    // N large enough for the correction terms to reach 2^-wp
    let big_n = ((wp as f64 + 64.0) / 8.0 + 2.0 * nf + 10.0).ceil() as i64;
    let guard = ((nf + 1.0) * ((big_n as f64 + af).ln().max(1.0)).log2() + 2.0 * (nf + 1.0).log2() + 32.0) as u64;
    let w = wp + guard;
    let log_t = |k: i64| -> Option<Ball> { mc_numeric::elementary::log(&Ball::from_i64(k).add(a, w), w) };
    let mut sum = Ball::from_i64(0);
    for k in 0..big_n {
        let t = Ball::from_i64(k).add(a, w);
        let l = log_t(k)?;
        let mut ln = Ball::from_i64(1);
        for _ in 0..n {
            ln = ln.mul(&l, w);
        }
        sum = sum.add(&ln.div(&t, w)?, w);
    }
    let t = Ball::from_i64(big_n).add(a, w);
    let l = log_t(big_n)?;
    // powers of L up to n + 1
    let mut lp = vec![Ball::from_i64(1)];
    for j in 0..=n as usize {
        let next = lp[j].mul(&l, w);
        lp.push(next);
    }
    sum = sum.sub(&lp[n as usize + 1].div(&Ball::from_i64(n as i64 + 1), w)?, w);
    // f(N)/2
    let tinv = Ball::from_i64(1).div(&t, w)?;
    sum = sum.add(&lp[n as usize].mul(&tinv, w).mul(&Ball::from_ratio(1, 2, w), w), w);
    // P_r as exact integer coefficients (balls), r = 0, 1, ...
    let mut p: Vec<Ball> = vec![Ball::from_i64(0); n as usize + 1];
    p[n as usize] = Ball::from_i64(1);
    let mut tpow = tinv.clone(); // t^(-1-r)
    let mut fact = Ball::from_i64(1); // (2j)!
    let mut last: Option<f64> = None;
    let target = crate::ball::log2_up(&sum).max(-1e9) - wp as f64 - 8.0;
    for r in 0..4000u64 {
        // P_(r+1) = -(1 + r) P_r + P_r'
        let mut q = vec![Ball::from_i64(0); n as usize + 1];
        for j in 0..=n as usize {
            q[j] = p[j].mul(&Ball::from_i64(-(1 + r as i64)), w);
            if j < n as usize {
                q[j] = q[j].add(&p[j + 1].mul(&Ball::from_i64(j as i64 + 1), w), w);
            }
        }
        p = q;
        tpow = tpow.mul(&tinv, w);
        // r + 1 = 2j - 1: the term B_2j/(2j)! f^(2j-1)(N)
        let order = r + 1;
        if order % 2 == 1 {
            let j = order.div_ceil(2);
            fact = fact.mul(&Ball::from_i64((2 * j - 1) as i64), w).mul(&Ball::from_i64((2 * j) as i64), w);
            let mut pv = Ball::from_i64(0);
            for (k, c) in p.iter().enumerate() {
                pv = pv.add(&c.mul(&lp[k], w), w);
            }
            let b = crate::ball::rat_ball(&crate::bernoulli::b2n(j)?, w);
            let term = b.div(&fact, w)?.mul(&pv, w).mul(&tpow, w);
            let lt = crate::ball::log2_up(&term);
            sum = sum.sub(&term, w);
            if lt < target {
                // the omitted terms: at most the last one
                let m = crate::ball::mag_up(&term);
                return Some(crate::ball::widen(sum, &m));
            }
            if let Some(prev) = last {
                if lt > prev + 1.0 && r > 8 {
                    return None;
                }
            }
            last = Some(lt);
        }
    }
    None
}

/// stieltjes_gamma(n) and stieltjes_gamma(n, a): exact gamma_0(1) = euler_gamma; numbers by [`stieltjes_ball`].
fn stieltjes_gamma<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    let nt = a[0];
    let at = if a.len() == 2 { a[1] } else { fx.int(1) };
    if fx.is_int(nt, 0) && fx.is_int(at, 1) {
        return Ok(Some(fx.sym("euler_gamma")));
    }
    let Some(nf) = fx.f64_of(nt) else { return Ok(None) };
    if nf.is_nan() || nf < 0.0 || nf.fract() != 0.0 || nf > 2000.0 {
        return Ok(None);
    }
    let args: Vec<T<'h>> = if a.len() == 2 { vec![nt, at] } else { vec![nt] };
    if !crate::cover::numeric(fx, &args) {
        return Ok(None);
    }
    if !matches!(fx.num(at), Some(Num::Real(_))) {
        return Ok(None);
    }
    let Some((wk, xs)) = work(fx, &args, 0) else { return Ok(None) };
    let ab = if a.len() == 2 {
        match fx.num(xs[1]) {
            Some(Num::Real(Real::Ball(b, _))) => b,
            _ => return Ok(None),
        }
    } else {
        Ball::from_i64(1)
    };
    let mut wp = wk.p + 16;
    for _ in 0..4 {
        let Some(v) = stieltjes_ball(nf as u64, &ab, wp) else { return Ok(None) };
        let acc = crate::ball::accuracy(&v);
        let t = fx.ar.number(&Num::Real(Real::Ball(v, wk.w)));
        if acc >= wk.p as f64 || !ab.rad.is_zero() {
            return Ok(wk.out(fx, t, true));
        }
        wp *= 2;
    }
    Ok(None)
}

// ---------------------------------------------------------------- Siegel theta

/// The rows of a square matrix of numbers.
fn matrix<'h>(fx: &Fx<'_, '_, '_, 'h>, t: T<'h>) -> Option<Vec<Vec<T<'h>>>> {
    let rows = fx.as_list(t)?;
    let g = rows.len();
    let mut out = Vec::with_capacity(g);
    for &r in rows {
        let r = fx.as_list(r)?;
        if r.len() != g {
            return None;
        }
        out.push(r.to_vec());
    }
    (g > 0).then_some(out)
}

/// The Cholesky factor of a symmetric positive definite matrix (`None` otherwise) and its smallest eigenvalue bound
/// (the square of the smallest diagonal entry of the factor divided by the dimension: a crude lower bound is enough
/// for the box, the terms themselves decide).
fn smallest_eigen(y: &[Vec<f64>]) -> Option<f64> {
    let g = y.len();
    // power iteration on the inverse via Gaussian elimination
    let mut l = vec![vec![0.0; g]; g];
    for i in 0..g {
        for j in 0..=i {
            let s = y[i][j] - (0..j).map(|k| l[i][k] * l[j][k]).sum::<f64>();
            if i == j {
                if s <= 0.0 {
                    return None;
                }
                l[i][i] = s.sqrt();
            } else {
                l[i][j] = s / l[j][j];
            }
        }
    }
    // inverse power iteration: solve L L^T x = v
    let solve = |v: &[f64]| -> Vec<f64> {
        let mut z = v.to_vec();
        for i in 0..g {
            for k in 0..i {
                z[i] -= l[i][k] * z[k];
            }
            z[i] /= l[i][i];
        }
        for i in (0..g).rev() {
            for k in i + 1..g {
                z[i] -= l[k][i] * z[k];
            }
            z[i] /= l[i][i];
        }
        z
    };
    let mut v = vec![1.0; g];
    let mut lam = 0.0;
    for _ in 0..200 {
        let x = solve(&v);
        let n = x.iter().map(|a| a * a).sum::<f64>().sqrt();
        if n == 0.0 {
            return None;
        }
        lam = 1.0 / n;
        v = x.iter().map(|a| a / n).collect();
    }
    Some(lam * 0.9)
}

/// siegel_theta(omega, s), siegel_theta([nu1, nu2], omega, s).
fn siegel_theta<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    let (chars, om, s) = if a.len() == 3 { (Some(a[0]), a[1], a[2]) } else { (None, a[0], a[1]) };
    let Some(m) = matrix(fx, om) else { return Ok(None) };
    let g = m.len();
    let Some(sv) = fx.as_list(s).map(|l| l.to_vec()) else { return Ok(None) };
    if sv.len() != g {
        return Ok(None);
    }
    let (nu1, nu2) = match chars {
        Some(c) => {
            let Some([c1, c2]) = fx.as_list(c).map(|l| l.to_vec()).and_then(|l| <[T<'h>; 2]>::try_from(l).ok()) else { return Ok(None) };
            let (Some(v1), Some(v2)) = (fx.as_list(c1).map(|l| l.to_vec()), fx.as_list(c2).map(|l| l.to_vec())) else { return Ok(None) };
            if v1.len() != g || v2.len() != g {
                return Ok(None);
            }
            (v1, v2)
        }
        None => (vec![fx.int(0); g], vec![fx.int(0); g]),
    };
    // all entries numbers, one inexact
    let mut flat: Vec<T<'h>> = m.iter().flatten().copied().collect();
    flat.extend(sv.iter().copied());
    flat.extend(nu1.iter().copied());
    flat.extend(nu2.iter().copied());
    if !crate::cover::numeric(fx, &flat) {
        return Ok(None);
    }
    // symmetric with a positive definite imaginary part
    let mut y = vec![vec![0.0; g]; g];
    for i in 0..g {
        for j in 0..g {
            let (Some(p), Some(q)) = (parts(fx, m[i][j]), parts(fx, m[j][i])) else { return Ok(None) };
            if (p.0 - q.0).abs() + (p.1 - q.1).abs() > 1e-12 * (1.0 + p.0.abs() + p.1.abs()) {
                return Ok(None);
            }
            y[i][j] = p.1;
        }
    }
    let Some(lam) = smallest_eigen(&y) else { return Ok(None) };
    let Some((wk, xs)) = work(fx, &flat, 16) else { return Ok(None) };
    let w = wk.w;
    let mw: Vec<Vec<T<'h>>> = (0..g).map(|i| xs[i * g..(i + 1) * g].to_vec()).collect();
    let sw = &xs[g * g..g * g + g];
    let n1 = &xs[g * g + g..g * g + 2 * g];
    let n2 = &xs[g * g + 2 * g..g * g + 3 * g];
    // the center -Y^-1 Im s - nu1 and the box radius
    let ims: Vec<f64> = sw.iter().map(|&t| parts(fx, t).map_or(0.0, |p| p.1)).collect();
    let nu1f: Vec<f64> = n1.iter().map(|&t| parts(fx, t).map_or(0.0, |p| p.0)).collect();
    // the terms are exp(-pi (m - c).Y.(m - c) + peak) in modulus, c = -Y^-1 Im s (m = n + nu1), peak = pi c.Y.c
    let center_m: Vec<f64> = solve_dense(&y, &ims).iter().map(|c| -c).collect();
    let center: Vec<f64> = center_m.iter().zip(&nu1f).map(|(c, v)| c - v).collect();
    let peak = std::f64::consts::PI * quad(&y, &center_m);
    let target = (w as f64 + 16.0) * std::f64::consts::LN_2;
    let radius = (target / (std::f64::consts::PI * lam)).sqrt() + 1.5;
    if !radius.is_finite() || (2.0 * radius + 1.0).powi(g as i32) > 2e6 {
        return Ok(None);
    }
    let lo: Vec<i64> = center.iter().map(|c| (c - radius).floor() as i64).collect();
    let hi: Vec<i64> = center.iter().map(|c| (c + radius).ceil() as i64).collect();
    let pi = crate::cover::pi_at(fx, w)?;
    let ipi = fx.eval(fx.mul(&[fx.i(), pi]))?;
    let mut idx = lo.clone();
    let mut sum = fx.int(0);
    let mut count = 0u64;
    loop {
        // m = n + nu1
        let mut mv = Vec::with_capacity(g);
        for k in 0..g {
            mv.push(fx.add(&[fx.int(idx[k]), n1[k]]));
        }
        let mf: Vec<f64> = (0..g).map(|k| idx[k] as f64 + nu1f[k]).collect();
        let d: Vec<f64> = mf.iter().zip(&center_m).map(|(a, b)| a - b).collect();
        if quad(&y, &d) * std::f64::consts::PI <= target {
            let mut quadt = vec![];
            for i in 0..g {
                for j in 0..g {
                    quadt.push(fx.mul(&[mv[i], mw[i][j], mv[j]]));
                }
            }
            let mut lin = vec![];
            for k in 0..g {
                lin.push(fx.mul(&[fx.int(2), mv[k], fx.add(&[sw[k], n2[k]])]));
            }
            let e = fx.mul(&[ipi, fx.add(&[fx.add(&quadt), fx.add(&lin)])]);
            let t = fx.eval(fx.call("exp", &[e]))?;
            sum = fx.eval(fx.add(&[sum, t]))?;
            count += 1;
            if count.is_multiple_of(64) {
                fx.tick(1)?;
            }
        }
        // next index
        let mut k = 0;
        loop {
            if k == g {
                let v = fx.eval(sum)?;
                if fx.num(v).is_none() {
                    return Ok(None);
                }
                // the omitted terms: below 2^-(w + 16) of the largest
                let bound = (peak - target).exp() * (count as f64 + 1.0);
                let mb = mc_arith::num::Mag::from_float_up(&crate::ball::float_of(bound.max(1e-300)));
                let v = crate::cover::map_balls(fx, v, |b| crate::ball::widen(b.clone(), &mb));
                let real = matches!(fx.num(v), Some(Num::Complex(_, ref im)) if im.to_f64().abs() <= 2f64.powi(-(w.min(1000) as i32 - 24)) * mag(fx, v));
                return Ok(wk.out(fx, v, real));
            }
            idx[k] += 1;
            if idx[k] <= hi[k] {
                break;
            }
            idx[k] = lo[k];
            k += 1;
        }
    }
}

/// x.Y.x
fn quad(y: &[Vec<f64>], x: &[f64]) -> f64 {
    let g = x.len();
    let mut s = 0.0;
    for i in 0..g {
        for j in 0..g {
            s += x[i] * y[i][j] * x[j];
        }
    }
    s
}

/// Y^-1 b by Gaussian elimination with partial pivoting.
fn solve_dense(y: &[Vec<f64>], b: &[f64]) -> Vec<f64> {
    let g = b.len();
    let mut a: Vec<Vec<f64>> = y.iter().zip(b).map(|(r, &v)| r.iter().copied().chain(std::iter::once(v)).collect()).collect();
    for c in 0..g {
        let p = (c..g).max_by(|&i, &j| a[i][c].abs().total_cmp(&a[j][c].abs())).unwrap_or(c);
        a.swap(c, p);
        let d = a[c][c];
        if d == 0.0 {
            return vec![0.0; g];
        }
        let pivot = a[c].clone();
        for (r, row) in a.iter_mut().enumerate() {
            if r != c {
                let f = row[c] / d;
                for (x, p) in row.iter_mut().zip(&pivot).skip(c) {
                    *x -= f * p;
                }
            }
        }
    }
    (0..g).map(|i| a[i][g] / a[i][i]).collect()
}

// ---------------------------------------------------------------- spheroidal wave functions

/// The coefficients A_k, B_k, C_k of the recurrence A_k a_(k-1) + (B_k - lambda) a_k + C_k a_(k+1) = 0 of the
/// Legendre expansion Ps = sum (-1)^k a_k P^m_(n+2k) (DLMF 30.8.2-30.8.4), for integers n >= m >= 0 and g2 = gamma^2.
fn sph_abc<'h>(fx: &Fx<'_, '_, '_, 'h>, n: i64, m: i64, g2: T<'h>, k: i64) -> [T<'h>; 3] {
    let q = |num: i64, den: i64| fx.rat(num, den);
    let l = n + 2 * k;
    let a = fx.mul(&[g2, q(-(n - m + 2 * k - 1) * (n - m + 2 * k), (2 * n + 4 * k - 3) * (2 * n + 4 * k - 1))]);
    let b = fx.sub(fx.int(l * (l + 1)), fx.mul(&[g2, q(2 * (l * (l + 1) + m * m - 1), (2 * n + 4 * k - 1) * (2 * n + 4 * k + 3))]));
    let c = fx.mul(&[g2, q(-(n + m + 2 * k + 1) * (n + m + 2 * k + 2), (2 * n + 4 * k + 3) * (2 * n + 4 * k + 5))]);
    [a, b, c]
}

/// The number of terms above k = 0 kept in the continued fractions.
fn sph_terms(n: i64, gm: f64, w: u64) -> i64 {
    (w as f64 / 6.0) as i64 + (gm * 2.0) as i64 + n / 4 + 12
}

/// The characteristic function F(lambda) = B_0 - lambda + A_0 L_(-1) + C_0 R_1 (continued fractions downwards to
/// k0 = -floor((n - m)/2) and upwards to K) and the ratios a_k/a_0.
fn sph_cf<'h>(fx: &mut Fx<'_, '_, '_, 'h>, n: i64, m: i64, g2: T<'h>, lam: T<'h>, kk: i64) -> Res<Option<(T<'h>, Vec<T<'h>>)>> {
    let k0 = -((n - m) / 2);
    // upwards: R_k = a_k/a_(k-1) = -A_k/(B_k - lambda + C_k R_(k+1))
    let mut r = vec![fx.int(0); (kk + 2) as usize];
    for k in (1..=kk).rev() {
        let [a, b, c] = sph_abc(fx, n, m, g2, k);
        let den = fx.eval(fx.add(&[fx.sub(b, lam), fx.mul(&[c, r[(k + 1) as usize]])]))?;
        if fx.num(den).is_none_or(|v| v.is_zero()) {
            return Ok(None);
        }
        r[k as usize] = fx.eval(fx.neg(fx.div(a, den)))?;
    }
    // downwards: L_k = a_k/a_(k+1) = -C_k/(B_k - lambda + A_k L_(k-1)), L_(k0-1) = 0
    let mut lft = vec![fx.int(0); (-k0 + 1) as usize];
    let mut prev = fx.int(0);
    for (i, k) in (k0..0).enumerate() {
        let [a, b, c] = sph_abc(fx, n, m, g2, k);
        let den = fx.eval(fx.add(&[fx.sub(b, lam), fx.mul(&[a, prev])]))?;
        if fx.num(den).is_none_or(|v| v.is_zero()) {
            return Ok(None);
        }
        prev = fx.eval(fx.neg(fx.div(c, den)))?;
        lft[i] = prev;
    }
    let [a0, b0, c0] = sph_abc(fx, n, m, g2, 0);
    let lm1 = if k0 < 0 { prev } else { fx.int(0) };
    let f = fx.eval(fx.add(&[fx.sub(b0, lam), fx.mul(&[a0, lm1]), fx.mul(&[c0, r[1]])]))?;
    // a_k/a_0 for k0 <= k <= K
    let mut ratios = vec![fx.int(1)];
    let mut cur = fx.int(1);
    for k in 1..=kk {
        cur = fx.eval(fx.mul(&[cur, r[k as usize]]))?;
        ratios.push(cur);
    }
    let mut down = vec![];
    let mut cur = fx.int(1);
    for k in (k0..0).rev() {
        // a_k = L_k a_(k+1)
        cur = fx.eval(fx.mul(&[cur, lft[(k - k0) as usize]]))?;
        down.push(cur);
    }
    down.reverse();
    down.extend(ratios);
    Ok(fx.num(f).map(|_| (f, down)))
}

/// The starting value of lambda: the eigenvalue of index floor((n - m)/2) of the symmetrized tridiagonal matrix in
/// machine numbers for a real gamma^2, else B_0 with the second-order corrections.
fn sph_start(n: i64, m: i64, g2: (f64, f64)) -> (f64, f64) {
    let k0 = -((n - m) / 2);
    let abc = |k: i64| {
        let l = (n + 2 * k) as f64;
        let (nf, mf, kf) = (n as f64, m as f64, k as f64);
        let a = -(nf - mf + 2.0 * kf - 1.0) * (nf - mf + 2.0 * kf) / ((2.0 * nf + 4.0 * kf - 3.0) * (2.0 * nf + 4.0 * kf - 1.0));
        let b = 2.0 * (l * (l + 1.0) + mf * mf - 1.0) / ((2.0 * nf + 4.0 * kf - 1.0) * (2.0 * nf + 4.0 * kf + 3.0));
        let c = -(nf + mf + 2.0 * kf + 1.0) * (nf + mf + 2.0 * kf + 2.0) / ((2.0 * nf + 4.0 * kf + 3.0) * (2.0 * nf + 4.0 * kf + 5.0));
        (a, l * (l + 1.0), b, c)
    };
    if g2.1 == 0.0 {
        let kk = 40 + (g2.0.abs().sqrt() * 2.0) as i64;
        let diag: Vec<f64> = (k0..=kk).map(|k| {
            let (_, ll, b, _) = abc(k);
            ll - g2.0 * b
        }).collect();
        let off: Vec<f64> = (k0..kk).map(|k| {
            let (_, _, _, c) = abc(k);
            let (a1, _, _, _) = abc(k + 1);
            (a1 * c).max(0.0).sqrt() * g2.0.abs()
        }).collect();
        return (crate::mathieu::tridiag_eigen_pub(&diag, &off, (-k0) as usize), 0.0);
    }
    let (_, ll, b, _) = abc(0);
    (ll - g2.0 * b, -g2.1 * b)
}

/// lambda^m_n(gamma^2) at working precision w by the secant method on F at 64, 128, ..., w bits.
/// [`sph_solve`] with the last result kept: a plot of spheroidal_ps (or the radial functions) varies x at one gamma, and
/// the eigenvalue depends on (n, m, gamma^2, w) only.
fn sph_lambda<'h>(fx: &mut Fx<'_, '_, '_, 'h>, n: i64, m: i64, g2: T<'h>, w: u64) -> R<'h> {
    thread_local! {
        static LAST: std::cell::RefCell<Option<(String, Num)>> = const { std::cell::RefCell::new(None) };
    }
    let Some(gn) = fx.num(g2) else { return sph_solve(fx, n, m, g2, w) };
    let key = format!("{n} {m} {w} {gn:?}");
    if let Some(v) = LAST.with(|c| c.borrow().as_ref().filter(|(k, _)| *k == key).map(|(_, v)| v.clone())) {
        return Ok(Some(fx.ar.number(&v)));
    }
    let r = sph_solve(fx, n, m, g2, w)?;
    if let Some(v) = r.and_then(|t| fx.num(t)) {
        LAST.with(|c| *c.borrow_mut() = Some((key, v)));
    }
    Ok(r)
}

fn sph_solve<'h>(fx: &mut Fx<'_, '_, '_, 'h>, n: i64, m: i64, g2: T<'h>, w: u64) -> R<'h> {
    let Some(g2f) = parts(fx, g2) else { return Ok(None) };
    let gm = g2f.0.hypot(g2f.1).sqrt();
    // the continued fractions keep about 2|gamma| terms each and every secant step runs both: beyond |gamma| = 1.5e4
    // (a root finder that wandered far out) the value stays unevaluated instead of allocating without bound
    if gm.is_nan() || gm > 1.5e4 {
        return Ok(None);
    }
    let (s0, s1) = sph_start(n, m, g2f);
    let num_c = |fx: &Fx<'_, '_, '_, 'h>, re: f64, im: f64, ww: u64| {
        let b = |v: f64| Real::Ball(Ball::exact(crate::ball::float_of(v)), ww);
        if im == 0.0 {
            fx.ar.number(&Num::Real(b(re)))
        } else {
            fx.ar.number(&Num::Complex(b(re), b(im)))
        }
    };
    let mut x1 = num_c(fx, s0, s1, 64);
    let mut ww = 64u64;
    loop {
        // the midpoint of gamma^2: its radius would make the differences of F contain zero
        let g2w = retag(fx, crate::cover::mid_term(fx, g2), ww);
        x1 = retag(fx, crate::cover::mid_term(fx, x1), ww);
        let kk = sph_terms(n, gm, ww);
        let d = fx.pow(fx.int(2), fx.int(-((ww / 2) as i64)));
        let mut x0 = fx.eval(fx.add(&[x1, fx.mul(&[d, fx.add(&[fx.int(1), fx.call("abs", &[x1])])])]))?;
        let Some((mut f0, _)) = sph_cf(fx, n, m, g2w, x0, kk)? else { return Ok(None) };
        for _ in 0..60 {
            let Some((f1, _)) = sph_cf(fx, n, m, g2w, x1, kk)? else { return Ok(None) };
            let df = fx.eval(fx.sub(f1, f0))?;
            if fx.num(df).is_none_or(|v| v.is_zero()) {
                break;
            }
            let x2 = fx.eval(fx.sub(x1, fx.div(fx.mul(&[f1, fx.sub(x1, x0)]), df)))?;
            if fx.num(x2).is_none() {
                return Ok(None);
            }
            let x2 = retag(fx, crate::cover::mid_term(fx, x2), ww);
            let step = mag(fx, fx.sub(x2, x1));
            x0 = x1;
            f0 = f1;
            x1 = x2;
            if step <= mag(fx, x1).max(1.0) * 2f64.powi(-((ww as i32).min(1000) - 8)) {
                break;
            }
        }
        if ww >= w {
            break;
        }
        ww = (ww * 2).min(w);
    }
    Ok(Some(x1))
}

/// A number carried at w bits.
fn retag<'h>(fx: &Fx<'_, '_, '_, 'h>, t: T<'h>, w: u64) -> T<'h> {
    let g = |x: Real| match x {
        Real::Ball(b, _) => Real::Ball(b, w),
        Real::Exact(q) => Real::Exact(q).to_kind(mc_arith::Kind::Ball, w),
        Real::Machine(v) => Real::Ball(Ball::exact(crate::ball::float_of(v)), w),
    };
    match fx.num(t) {
        Some(Num::Real(x)) => fx.ar.number(&Num::Real(g(x))),
        Some(Num::Complex(x, y)) => fx.ar.number(&Num::Complex(g(x), g(y))),
        None => t,
    }
}

/// Integer parameters n >= |m| (as numbers of any kind).
fn int_nm<'h>(fx: &Fx<'_, '_, '_, 'h>, n: T<'h>, m: T<'h>) -> Option<(i64, i64)> {
    let (nf, mf) = (fx.f64_of(n)?, fx.f64_of(m)?);
    (nf.fract() == 0.0 && mf.fract() == 0.0 && nf >= mf.abs() && nf < 1e4).then_some((nf as i64, mf as i64))
}

/// spheroidal_eigenvalue(n, m, gamma) = lambda^m_n(gamma^2) of (1 - x^2) w'' - 2 x w' + (lambda + gamma^2 (1 - x^2) -
/// m^2/(1 - x^2)) w = 0 (DLMF 30.2.1, 30.3): integers n >= |m|, real or complex gamma, by the continued fraction of
/// the Legendre expansion (Bouwkamp's method).
fn spheroidal_eigenvalue<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    let (n, m, g) = (a[0], a[1], a[2]);
    if fx.is_int(g, 0) {
        return Ok(Some(fx.mul(&[n, fx.add(&[n, fx.int(1)])])));
    }
    let Some((ni, mi)) = int_nm(fx, n, m) else { return Ok(None) };
    if !crate::cover::numeric(fx, a) {
        return Ok(None);
    }
    let Some((wk, xs)) = work(fx, &[g], 24) else { return Ok(None) };
    let g2 = fx.eval(fx.mul(&[xs[0], xs[0]]))?;
    // a real gamma^2 (gamma real or imaginary) gives a real eigenvalue
    let (g2, real) = match fx.num(g2) {
        Some(Num::Complex(x, y)) if y.to_f64() == 0.0 => (fx.ar.number(&Num::Real(x)), true),
        Some(Num::Real(_)) => (g2, true),
        _ => (g2, false),
    };
    let Some(v) = sph_lambda(fx, ni, mi.abs(), g2, wk.w)? else { return Ok(None) };
    let mrad = mc_arith::num::Mag::from_float_up(&crate::ball::float_of((mag(fx, v).max(1.0) * 2f64.powi(-((wk.p as i32).min(1000) - 4))).max(1e-300)));
    let v = crate::cover::map_balls(fx, v, |b| crate::ball::widen(b.clone(), &mrad));
    Ok(wk.out(fx, v, real))
}

/// spheroidal_ps(n, m, gamma, x) for |x| < 1: sum (-1)^k a_k P^m_(n+2k)(x) (DLMF 30.8.1) with the Ferrers functions of
/// the Condon-Shortley phase and the normalization of DLMF 30.4.4, the integral of Ps^2 over [-1, 1] equal to
/// 2 (n + m)!/((2n + 1)(n - m)!), with a_0 > 0 at small gamma (the principal root).
fn spheroidal_ps<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    let (n, m, g, x) = (a[0], a[1], a[2], a[3]);
    let Some((ni, mi)) = int_nm(fx, n, m) else { return Ok(None) };
    if mi < 0 {
        return Ok(None);
    }
    if fx.is_int(g, 0) && fx.num(x).is_none_or(|v| v.is_exact()) {
        return Ok(Some(fx.call("legendre_p", &[n, m, x])));
    }
    if !crate::cover::numeric(fx, a) {
        return Ok(None);
    }
    let Some(xp) = parts(fx, x) else { return Ok(None) };
    if xp.1 == 0.0 && xp.0.abs() >= 1.0 {
        return Ok(None);
    }
    let Some((wk, xs)) = work(fx, &[g, x], 32) else { return Ok(None) };
    let w = wk.w;
    let g2 = fx.eval(fx.mul(&[xs[0], xs[0]]))?;
    let Some(lam) = sph_lambda(fx, ni, mi, g2, w)? else { return Ok(None) };
    let Some(g2f) = parts(fx, g2) else { return Ok(None) };
    let kk = sph_terms(ni, g2f.0.hypot(g2f.1).sqrt(), w);
    let g2w = retag(fx, g2, w);
    let Some((_, ratios)) = sph_cf(fx, ni, mi, g2w, lam, kk)? else { return Ok(None) };
    let k0 = -((ni - mi) / 2);
    // the norm: sum a_k^2 (l + m)!/((2l + 1)(l - m)!) = (n + m)!/((2n + 1)(n - m)!), l = n + 2k
    let wnorm = |fx: &Fx<'_, '_, '_, 'h>, l: i64| -> T<'h> {
        // (l + m)!/(l - m)! = prod_(j = l - m + 1)^(l + m) j
        let mut p = fx.int(1);
        for j in (l - mi + 1)..=(l + mi) {
            p = fx.mul(&[p, fx.int(j)]);
        }
        fx.div(p, fx.int(2 * l + 1))
    };
    let mut s2 = vec![];
    for (i, &rk) in ratios.iter().enumerate() {
        let k = k0 + i as i64;
        s2.push(fx.mul(&[rk, rk, wnorm(fx, ni + 2 * k)]));
    }
    let s2 = fx.eval(fx.add(&s2))?;
    let wn = wnorm(fx, ni);
    let a0 = fx.eval(fx.call("sqrt", &[fx.div(wn, s2)]))?;
    // the Ferrers functions P^m_l(x) for l = m, m + 1, ...: P^m_m = (-1)^m (2m - 1)!! (1 - x^2)^(m/2)
    let xw = xs[1];
    let lmax = ni + 2 * kk;
    let mut df = fx.int(1);
    for j in 1..=mi {
        df = fx.mul(&[df, fx.int(2 * j - 1)]);
    }
    let base = fx.eval(fx.mul(&[fx.int(if mi % 2 == 0 { 1 } else { -1 }), df, fx.call("sqrt", &[fx.pow(fx.sub(fx.int(1), fx.mul(&[xw, xw])), fx.int(mi))])]))?;
    let mut p = vec![base];
    if lmax > mi {
        p.push(fx.eval(fx.mul(&[xw, fx.int(2 * mi + 1), base]))?);
    }
    for l in (mi + 1)..lmax {
        let i = (l - mi) as usize;
        let next = fx.div(fx.sub(fx.mul(&[fx.int(2 * l + 1), xw, p[i]]), fx.mul(&[fx.int(l + mi), p[i - 1]])), fx.int(l - mi + 1));
        p.push(fx.eval(next)?);
    }
    let mut terms = vec![];
    for (i, &rk) in ratios.iter().enumerate() {
        let k = k0 + i as i64;
        let l = ni + 2 * k;
        let sign = if k.rem_euclid(2) == 0 { 1 } else { -1 };
        terms.push(fx.mul(&[fx.int(sign), rk, p[(l - mi) as usize]]));
    }
    let v = fx.eval(fx.mul(&[a0, fx.add(&terms)]))?;
    if fx.num(v).is_none() {
        return Ok(None);
    }
    Ok(wk.out(fx, v, wk.real))
}

/// The definitions of this module.
pub static DEFS: &[Def] = &[
    Def { name: "spheroidal_eigenvalue", arities: &[3], listable: true, exact: Some(spheroidal_eigenvalue), numeric: None, summary: "spheroidal_eigenvalue(n, m, gamma): the spheroidal eigenvalue lambda of degree n and order m, the separation constant of the spheroidal wave equation" },
    Def { name: "spheroidal_ps", arities: &[4], listable: true, exact: Some(spheroidal_ps), numeric: None, summary: "spheroidal_ps(n, m, gamma, x): the angular spheroidal function of the first kind Ps^m_n(x, gamma^2)" },
    Def { name: "stieltjes_gamma", arities: &[1, 2], listable: true, exact: Some(stieltjes_gamma), numeric: None, summary: "stieltjes_gamma(n): the Stieltjes constant gamma_n, the coefficients of the Laurent series of zeta at 1; stieltjes_gamma(n, a): the generalized constant of the Hurwitz zeta function" },
    Def { name: "siegel_theta", arities: &[2, 3], listable: false, exact: Some(siegel_theta), numeric: None, summary: "siegel_theta(omega, s): the Siegel (Riemann) theta function, the sum over integer vectors n of exp(pi i n.omega.n + 2 pi i n.s); siegel_theta([nu1, nu2], omega, s) with characteristics" },
];
