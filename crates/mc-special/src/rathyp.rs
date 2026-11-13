//! Closed forms of (q+1)Fq(a; b; z) at a symbolic argument when all but one upper parameter differ from the lower
//! parameters by integers.
//!
//! Pair one upper parameter a0 with the k! of the series and every other upper parameter a with a lower one b,
//! a - b an integer n. Each pair is a rational function of k: (a)_k/(b)_k = (b + k)_n/(b)_n for n >= 0 and
//! (a)_m/(a + k)_m for n = -m < 0, so the series is sum_k (a0)_k/k! R(k) z^k with R rational, its poles at
//! k = -beta (beta > 0 here). Partial fractions R = P(k) + sum c/(k + beta)^j split it into
//!
//! - sum (a0)_k/k! k^r z^k = sum_s S(r, s) (a0)_s z^s (1 - z)^(-a0 - s) (S the Stirling numbers of the second kind:
//!   (z d/dz)^r applied to the binomial series (1 - z)^-a0);
//! - sum (a0)_k/k! z^k/(k + beta) = z^-beta int_0^z t^(beta - 1) (1 - t)^-a0 dt (DLMF 15.6.1 in its simplest case),
//!   elementary for an integer beta (t^(beta - 1) = (1 - (1 - t))^(beta - 1) expanded) and, with t = u^2, for a
//!   half-integer beta and a0 in (1/2)Z (the integrals of u^(2m) (1 - u^2)^s by the reduction formulas);
//! - for a0 = 1 the Lerch sums sum z^k/(k + beta)^j = z^-beta (Li_j(z) - sum_(i < beta) z^i/i^j) at an integer beta
//!   (Li_1(z) = -log(1 - z)) and 2 z^-beta (atanh(sqrt z) - ...) at a half-integer beta with j = 1.
//!
//! Every piece is the principal branch on the plane cut along [1, inf), as the function is; the even functions of
//! sqrt(z) that a half-integer beta gives are analytic at 0. Other cases (a pole of order two or more with a0 != 1, a
//! beta that is not a positive integer or half-integer, a non-positive pole) give `None`.
use mc_arith::num::{Int, Rat};
use mc_arith::{Arith, Num};
use mc_core::{Heap, Store, T};

/// The most a parameter difference may be (the degree of R).
const MAX_SHIFT: i64 = 40;

fn r(v: i64) -> Rat {
    Rat::from_i64(v)
}
fn half() -> Rat {
    Rat::new(Int::from_i64(1), Int::from_i64(2))
}
fn div(a: &Rat, b: &Rat) -> Option<Rat> {
    Some(a.mul(&b.recip()?))
}
fn small_int(q: &Rat) -> Option<i64> {
    if q.is_int() {
        q.num.to_i64()
    } else {
        None
    }
}
fn non_positive_int(q: &Rat) -> bool {
    q.is_int() && (q.is_zero() || q.num.neg)
}
fn positive(q: &Rat) -> bool {
    q.cmp_value(&r(0)).is_gt()
}
/// q = m + 1/2 with m >= 0: m.
fn half_index(q: &Rat) -> Option<i64> {
    let m = q.sub(&half());
    if positive(q) && m.is_int() {
        small_int(&m)
    } else {
        None
    }
}

/// A polynomial in k with rational coefficients, lowest degree first.
type Poly = Vec<Rat>;
/// Partial fractions: the polynomial part and, per pole beta, the coefficients of 1/(k + beta)^j, j = 1, 2, ...
type Fractions = (Poly, Vec<(Rat, Vec<Rat>)>);

fn poly_mul_linear(p: &Poly, c: &Rat) -> Poly {
    // p (k + c)
    let mut out = vec![r(0); p.len() + 1];
    for (i, a) in p.iter().enumerate() {
        out[i] = out[i].add(&a.mul(c));
        out[i + 1] = out[i + 1].add(a);
    }
    out
}

fn poly_trim(p: &mut Poly) {
    while p.len() > 1 && p.last().is_some_and(Rat::is_zero) {
        p.pop();
    }
}

/// The value at k = x.
#[cfg(test)]
fn poly_at(p: &Poly, x: &Rat) -> Rat {
    p.iter().rev().fold(r(0), |acc, a| acc.mul(x).add(a))
}

/// p(x + s) as a polynomial in s.
fn poly_shift(p: &Poly, x: &Rat) -> Poly {
    let mut out = vec![r(0)];
    for a in p.iter().rev() {
        out = poly_mul_linear(&out, x);
        out[0] = out[0].add(a);
    }
    poly_trim(&mut out);
    out
}

/// Quotient and remainder of p by the monic q.
fn poly_divmod(p: &Poly, q: &Poly) -> (Poly, Poly) {
    let mut rem = p.clone();
    poly_trim(&mut rem);
    let dq = q.len() - 1;
    if rem.len() <= dq {
        return (vec![r(0)], rem);
    }
    let mut quo = vec![r(0); rem.len() - dq];
    for i in (0..quo.len()).rev() {
        let c = rem[i + dq].clone();
        quo[i] = c.clone();
        for (j, b) in q.iter().enumerate() {
            rem[i + j] = rem[i + j].sub(&c.mul(b));
        }
    }
    rem.truncate(dq.max(1));
    poly_trim(&mut rem);
    (quo, rem)
}

/// The series of 1/(s + d)^m in s to order `n` (d != 0).
fn inverse_power_series(d: &Rat, m: usize, n: usize) -> Option<Vec<Rat>> {
    // 1/(s + d) = (1/d) sum (-s/d)^i
    let inv = d.recip()?;
    let base: Vec<Rat> = (0..n).map(|i| inv.mul(&inv.neg().pow_u64(i as u64))).collect();
    let mut out = vec![r(0); n];
    out[0] = r(1);
    for _ in 0..m {
        out = series_mul(&out, &base, n);
    }
    Some(out)
}

fn series_mul(a: &[Rat], b: &[Rat], n: usize) -> Vec<Rat> {
    let mut out = vec![r(0); n];
    for (i, x) in a.iter().enumerate().take(n) {
        if x.is_zero() {
            continue;
        }
        for (j, y) in b.iter().enumerate().take(n - i) {
            out[i + j] = out[i + j].add(&x.mul(y));
        }
    }
    out
}

/// R(k) = c N(k) / prod (k + beta)^m: the constant c, the numerator N and the poles (beta, m).
struct Rational {
    num: Poly,
    poles: Vec<(Rat, usize)>,
}

impl Rational {
    /// Partial fractions: the polynomial part and the coefficients c[beta][j - 1] of 1/(k + beta)^j.
    fn partial_fractions(&self) -> Option<Fractions> {
        let mut den: Poly = vec![r(1)];
        for (b, m) in &self.poles {
            for _ in 0..*m {
                den = poly_mul_linear(&den, b);
            }
        }
        let (quo, _) = poly_divmod(&self.num, &den);
        let mut out = vec![];
        for (i, (b, m)) in self.poles.iter().enumerate() {
            // the Taylor coefficients at k = -beta of N(k) / prod_(other) (k + gamma)^mg, to order m
            let at = b.neg();
            let mut g = poly_shift(&self.num, &at);
            g.resize(*m, r(0));
            g.truncate(*m);
            for (j, (c, mc)) in self.poles.iter().enumerate() {
                if i != j {
                    let s = inverse_power_series(&c.sub(b), *mc, *m)?;
                    g = series_mul(&g, &s, *m);
                }
            }
            // c_j is the coefficient of s^(m - j)
            let cs: Vec<Rat> = (1..=*m).map(|j| g[m - j].clone()).collect();
            out.push((b.clone(), cs));
        }
        Some((quo, out))
    }
}

/// A pairing of the upper parameters (but `a0`) with the lower ones, integer differences: the rational factor R.
fn rational_factor(pairs: &[(Rat, Rat)]) -> Option<Rational> {
    let mut c = r(1);
    let mut roots: Vec<Rat> = vec![];
    let mut poles: Vec<Rat> = vec![];
    for (a, b) in pairs {
        let n = small_int(&a.sub(b))?;
        if n.abs() > MAX_SHIFT {
            return None;
        }
        if n >= 0 {
            for i in 0..n {
                let bi = b.add(&r(i));
                c = div(&c, &bi)?;
                roots.push(bi);
            }
        } else {
            for i in 0..-n {
                let ai = a.add(&r(i));
                if !positive(&ai) {
                    return None;
                }
                c = c.mul(&ai);
                poles.push(ai);
            }
        }
    }
    // cancel common linear factors
    let mut kept_poles = vec![];
    for p in poles {
        if let Some(i) = roots.iter().position(|x| *x == p) {
            roots.remove(i);
        } else {
            kept_poles.push(p);
        }
    }
    let mut num: Poly = vec![c];
    for x in &roots {
        num = poly_mul_linear(&num, x);
    }
    let mut grouped: Vec<(Rat, usize)> = vec![];
    for p in kept_poles {
        match grouped.iter_mut().find(|(b, _)| *b == p) {
            Some(g) => g.1 += 1,
            None => grouped.push((p, 1)),
        }
    }
    Some(Rational { num, poles: grouped })
}

/// The pairings: for each choice of a0 (a0 = 1 first), a matching of the others with the lowers by integer
/// differences.
fn pairings(ups: &[Rat], lows: &[Rat]) -> Vec<(Rat, Vec<(Rat, Rat)>)> {
    fn matchings(rest: &[Rat], lows: &[Rat], used: &mut Vec<bool>, cur: &mut Vec<(Rat, Rat)>, out: &mut Vec<Vec<(Rat, Rat)>>) {
        if out.len() >= 8 {
            return;
        }
        let Some((a, more)) = rest.split_first() else {
            out.push(cur.clone());
            return;
        };
        for (j, b) in lows.iter().enumerate() {
            if !used[j] && a.sub(b).is_int() {
                used[j] = true;
                cur.push((a.clone(), b.clone()));
                matchings(more, lows, used, cur, out);
                cur.pop();
                used[j] = false;
            }
        }
    }
    let mut order: Vec<usize> = (0..ups.len()).collect();
    order.sort_by_key(|&i| ups[i] != r(1));
    let mut out = vec![];
    for i in order {
        let rest: Vec<Rat> = ups.iter().enumerate().filter(|&(j, _)| j != i).map(|(_, a)| a.clone()).collect();
        let mut ms = vec![];
        matchings(&rest, lows, &mut vec![false; lows.len()], &mut vec![], &mut ms);
        for m in ms {
            out.push((ups[i].clone(), m));
        }
    }
    out
}

/// The terms built with canonical arithmetic.
struct B<'a, 'h> {
    h: &'h Heap,
    ar: &'a Arith<'h, Heap>,
    z: T<'h>,
}

impl<'h> B<'_, 'h> {
    fn q(&self, q: &Rat) -> T<'h> {
        self.ar.number(&Num::rat(q.clone()))
    }
    fn n(&self, v: i64) -> T<'h> {
        self.ar.int(v)
    }
    fn call(&self, f: &str, args: &[T<'h>]) -> T<'h> {
        self.h.call(self.h.sym(f), args)
    }
    fn omz(&self) -> T<'h> {
        self.ar.sub(self.n(1), self.z)
    }
    fn zpow(&self, e: &Rat) -> T<'h> {
        self.ar.pow(self.z, self.q(e))
    }
    fn omz_pow(&self, e: &Rat) -> T<'h> {
        self.ar.pow(self.omz(), self.q(e))
    }
    fn log_omz(&self) -> T<'h> {
        self.call("log", &[self.omz()])
    }
    fn sqrt_z(&self) -> T<'h> {
        self.zpow(&half())
    }

    /// sum (a0)_k/k! k^m z^k.
    fn moment(&self, a0: &Rat, m: usize) -> T<'h> {
        // Stirling numbers S(m, s)
        let mut st = vec![vec![r(0); m + 1]; m + 1];
        st[0][0] = r(1);
        for i in 1..=m {
            let prev = st[i - 1].clone();
            for (s, slot) in st[i].iter_mut().enumerate().take(i + 1).skip(1) {
                *slot = prev[s - 1].add(&r(s as i64).mul(&prev[s]));
            }
        }
        let mut terms = vec![];
        let mut poch = r(1);
        for (s, sm) in st[m].iter().enumerate() {
            if s > 0 {
                poch = poch.mul(&a0.add(&r(s as i64 - 1)));
            }
            let c = sm.mul(&poch);
            if !c.is_zero() {
                terms.push(self.ar.mul(&[self.q(&c), self.ar.pow(self.z, self.n(s as i64)), self.omz_pow(&a0.add(&r(s as i64)).neg())]));
            }
        }
        self.ar.add(&terms)
    }

    /// int_0^z (1 - t)^s dt.
    fn j(&self, s: &Rat) -> T<'h> {
        let s1 = s.add(&r(1));
        if s1.is_zero() {
            return self.ar.neg(self.log_omz());
        }
        self.ar.div(self.ar.sub(self.n(1), self.omz_pow(&s1)), self.q(&s1))
    }

    /// int_0^w (1 - u^2)^s du with w = sqrt(z) (s in (1/2)Z, |s| <= MAX_SHIFT + 1).
    fn i0(&self, s: &Rat) -> Option<T<'h>> {
        let w = self.sqrt_z();
        let twice = small_int(&s.mul(&r(2)))?;
        if twice.abs() > 2 * MAX_SHIFT + 4 {
            return None;
        }
        if s.is_int() && !s.num.neg {
            // a polynomial: sum_i C(s, i) (-1)^i w^(2i + 1)/(2i + 1)
            let n = small_int(s)?;
            let mut terms = vec![];
            let mut c = r(1);
            for i in 0..=n {
                let t = div(&c, &r(2 * i + 1))?;
                terms.push(self.ar.mul(&[self.q(&t), self.ar.pow(w, self.n(2 * i + 1))]));
                c = c.mul(&r(-(n - i))).mul(&r(i + 1).recip()?);
            }
            return Some(self.ar.add(&terms));
        }
        let (base_s, base) = if s.is_int() { (r(-1), self.call("atanh", &[w])) } else { (half().neg(), self.call("asin", &[w])) };
        let mut cur_s = base_s.clone();
        let mut cur = base;
        // up: I(s) = (w (1 - z)^s + 2 s I(s - 1))/(2 s + 1); down: I(s) = ((2 s + 3) I(s + 1) - w (1 - z)^(s + 1))/(2 (s + 1))
        while cur_s.cmp_value(s).is_lt() {
            let ns = cur_s.add(&r(1));
            let d = ns.mul(&r(2)).add(&r(1));
            let top = self.ar.add(&[self.ar.mul(&[w, self.omz_pow(&ns)]), self.ar.mul(&[self.q(&ns.mul(&r(2))), cur])]);
            cur = self.ar.div(top, self.q(&d));
            cur_s = ns;
        }
        while cur_s.cmp_value(s).is_gt() {
            let ns = cur_s.sub(&r(1));
            let d = ns.add(&r(1)).mul(&r(2));
            if d.is_zero() {
                return None;
            }
            let top = self.ar.sub(self.ar.mul(&[self.q(&ns.mul(&r(2)).add(&r(3))), cur]), self.ar.mul(&[w, self.omz_pow(&cur_s)]));
            cur = self.ar.div(top, self.q(&d));
            cur_s = ns;
        }
        Some(cur)
    }

    /// int_0^w u^(2m) (1 - u^2)^s du = I(m - 1, s) - I(m - 1, s + 1).
    fn im(&self, m: i64, s: &Rat) -> Option<T<'h>> {
        if m == 0 {
            return self.i0(s);
        }
        let a = self.im(m - 1, s)?;
        let b = self.im(m - 1, &s.add(&r(1)))?;
        Some(self.ar.sub(a, b))
    }

    /// sum (a0)_k/k! z^k/(k + beta)^j.
    fn pole(&self, a0: &Rat, beta: &Rat, j: usize) -> Option<T<'h>> {
        if *a0 == r(1) {
            if let Some(b) = small_int(beta).filter(|b| (1..=MAX_SHIFT + 1).contains(b)) {
                let li = if j == 1 { self.ar.neg(self.log_omz()) } else { self.call("polylog", &[self.n(j as i64), self.z]) };
                let mut terms = vec![li];
                for i in 1..b {
                    let c = r(i).pow_u64(j as u64).recip()?;
                    terms.push(self.ar.neg(self.ar.mul(&[self.q(&c), self.ar.pow(self.z, self.n(i))])));
                }
                return Some(self.ar.mul(&[self.zpow(&beta.neg()), self.ar.add(&terms)]));
            }
            if j == 1 {
                let m = half_index(beta)?;
                let mut terms = vec![self.ar.mul(&[self.n(2), self.call("atanh", &[self.sqrt_z()])])];
                for i in 0..m {
                    let e = r(i).add(&half());
                    terms.push(self.ar.neg(self.ar.div(self.zpow(&e), self.q(&e))));
                }
                return Some(self.ar.mul(&[self.zpow(&beta.neg()), self.ar.add(&terms)]));
            }
            return None;
        }
        if j != 1 {
            return None;
        }
        if let Some(b) = small_int(beta).filter(|b| (1..=MAX_SHIFT + 1).contains(b)) {
            // z^-b sum_i C(b - 1, i) (-1)^i int_0^z (1 - t)^(i - a0) dt
            let mut terms = vec![];
            let mut c = r(1);
            for i in 0..b {
                terms.push(self.ar.mul(&[self.q(&c), self.j(&r(i).sub(a0))]));
                c = c.mul(&r(-(b - 1 - i))).mul(&r(i + 1).recip()?);
            }
            return Some(self.ar.mul(&[self.zpow(&beta.neg()), self.ar.add(&terms)]));
        }
        let m = half_index(beta)?;
        if !a0.mul(&r(2)).is_int() || m > MAX_SHIFT {
            return None;
        }
        let i = self.im(m, &a0.neg())?;
        Some(self.ar.mul(&[self.n(2), self.zpow(&beta.neg()), i]))
    }
}

/// (q+1)Fq(ups; lows; z) in closed form for a symbolic z (see the module documentation); `None` otherwise.
pub fn closed_form<'h>(h: &'h Heap, ar: &Arith<'h, Heap>, ups: &[Rat], lows: &[Rat], z: T<'h>) -> Option<T<'h>> {
    if ups.len() != lows.len() + 1 || ups.len() > 6 || ar.num(z).is_some() {
        return None;
    }
    if ups.iter().any(non_positive_int) || lows.iter().any(non_positive_int) {
        return None;
    }
    let b = B { h, ar, z };
    'pairing: for (a0, pairs) in pairings(ups, lows) {
        let Some(rf) = rational_factor(&pairs) else { continue };
        let Some((poly, poles)) = rf.partial_fractions() else { continue };
        let mut terms = vec![];
        for (m, c) in poly.iter().enumerate() {
            if !c.is_zero() {
                terms.push(ar.mul(&[b.q(c), b.moment(&a0, m)]));
            }
        }
        for (beta, cs) in &poles {
            for (j, c) in cs.iter().enumerate() {
                if c.is_zero() {
                    continue;
                }
                let Some(p) = b.pole(&a0, beta, j + 1) else { continue 'pairing };
                terms.push(ar.mul(&[b.q(c), p]));
            }
        }
        return Some(ar.add(&terms));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_fractions_of_a_pair_product() {
        // (1)_k/(3)_k (2)_k/(3)_k = 4/((k + 1)(k + 2)^2) = 4/(k + 1) - 4/(k + 2) - 4/(k + 2)^2
        let rf = rational_factor(&[(r(1), r(3)), (r(2), r(3))]).unwrap();
        let (p, poles) = rf.partial_fractions().unwrap();
        assert!(p.iter().all(Rat::is_zero));
        let at = |b: i64| poles.iter().find(|(x, _)| *x == r(b)).unwrap().1.clone();
        assert_eq!(at(1), vec![r(4)]);
        assert_eq!(at(2), vec![r(-4), r(-4)]);
        // the value at k = 3 by both routes: 4/(4 * 25) = 1/25
        let k = r(3);
        let direct = div(&poly_at(&rf.num, &k), &r(4 * 25)).unwrap();
        assert_eq!(direct, Rat::new(Int::from_i64(1), Int::from_i64(25)));
        let sum = r(4).mul(&r(4).recip().unwrap()).sub(&r(4).mul(&r(5).recip().unwrap())).sub(&r(4).mul(&r(25).recip().unwrap()));
        assert_eq!(sum, direct);
    }
}
