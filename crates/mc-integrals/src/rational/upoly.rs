//! Dense univariate polynomials over an exact field: the rationals [`Q`], rational functions of parameters
//! ([`super::params::Params`]) or a quadratic extension [`Quad`] of a field.
//!
//! A polynomial is a coefficient vector, index = degree, without trailing zeros (the zero polynomial is empty).
//! Every operation counts its elementary coefficient operations on a [`Ticker`], so long computations can be
//! interrupted; an operation that cannot proceed (a zero divisor, which only happens after an arithmetic stop or for
//! an extension that is not a field) gives [`Stop::GiveUp`] instead of panicking.
#![allow(clippy::type_complexity)]
use mc_arith::num::{Int, Nat, Rat};
use mc_eval::Abort;

/// Why a computation ended early.
pub enum Stop {
    /// the evaluator's fuel or interrupt: propagated to the caller
    Abort(Abort),
    /// the method does not apply or cannot go on (stop flag, zero divisor): the integral stays unevaluated
    GiveUp,
}

/// The result of an interruptible step.
pub type R<X> = Result<X, Stop>;

/// A budget elementary operations are charged to.
pub trait Ticker {
    /// Count `n` elementary operations.
    fn step(&mut self, n: u64) -> R<()>;
}

/// A rational number from machine integers.
pub fn q(n: i64) -> Rat {
    Rat::from_i64(n)
}

/// The rational `n / d` (d nonzero).
pub fn qq(n: i64, d: i64) -> Rat {
    Rat::new(Int::from_i64(n), Int::from_i64(d))
}

/// The rational square root of `a` when `a >= 0` is the square of a rational.
pub fn qsqrt(a: &Rat) -> Option<Rat> {
    if a.num.neg {
        return None;
    }
    let n = a.num.mag.isqrt();
    let d = a.den.isqrt();
    if n.mul(&n) == a.num.mag && d.mul(&d) == a.den {
        Some(Rat::new(Int::from_nat(n), Int::from_nat(d)))
    } else {
        None
    }
}

/// An exact field of characteristic 0 (its elements have real signs when the field lies in the reals).
pub trait Fld {
    /// an element
    type E: Clone + PartialEq;
    /// `0`
    fn zero(&self) -> Self::E;
    /// the rational `c` as an element
    fn of(&self, c: &Rat) -> Self::E;
    /// is `a` zero?
    fn is_zero(&self, a: &Self::E) -> bool;
    /// `a + b`
    fn add(&self, a: &Self::E, b: &Self::E) -> Self::E;
    /// `-a`
    fn neg(&self, a: &Self::E) -> Self::E;
    /// `a * b`
    fn mul(&self, a: &Self::E, b: &Self::E) -> Self::E;
    /// `1 / a`, `None` for a zero divisor
    fn inv(&self, a: &Self::E) -> Option<Self::E>;
    /// the sign of `a` as a real number, when it is known (-1, 0, 1)
    fn sign(&self, _a: &Self::E) -> Option<i32> {
        None
    }
    /// a square root of `a` in the field, when there is one
    fn sqrt(&self, _a: &Self::E) -> Option<Self::E> {
        None
    }
    /// `a` as a rational, when it is one
    fn as_rat(&self, _a: &Self::E) -> Option<Rat> {
        None
    }
    /// `(s, m)` with `a = s^2 m` and m as simple as found (for writing `√a = s √m`)
    fn square_split(&self, a: &Self::E) -> (Self::E, Self::E) {
        (self.one(), a.clone())
    }
    /// `1`
    fn one(&self) -> Self::E {
        self.of(&q(1))
    }
    /// `a - b`
    fn sub(&self, a: &Self::E, b: &Self::E) -> Self::E {
        self.add(a, &self.neg(b))
    }
    /// `a / b`, `None` for a zero divisor
    fn div(&self, a: &Self::E, b: &Self::E) -> Option<Self::E> {
        Some(self.mul(a, &self.inv(b)?))
    }
    /// `a / b`, giving up for a zero divisor
    fn fdiv(&self, a: &Self::E, b: &Self::E) -> R<Self::E> {
        self.div(a, b).ok_or(Stop::GiveUp)
    }
}

/// The rationals.
pub struct Q;

impl Fld for Q {
    type E = Rat;
    fn zero(&self) -> Rat {
        q(0)
    }
    fn of(&self, c: &Rat) -> Rat {
        c.clone()
    }
    fn is_zero(&self, a: &Rat) -> bool {
        a.is_zero()
    }
    fn add(&self, a: &Rat, b: &Rat) -> Rat {
        a.add(b)
    }
    fn neg(&self, a: &Rat) -> Rat {
        a.neg()
    }
    fn mul(&self, a: &Rat, b: &Rat) -> Rat {
        a.mul(b)
    }
    fn inv(&self, a: &Rat) -> Option<Rat> {
        a.recip()
    }
    fn sign(&self, a: &Rat) -> Option<i32> {
        Some(if a.is_zero() {
            0
        } else if a.num.neg {
            -1
        } else {
            1
        })
    }
    fn sqrt(&self, a: &Rat) -> Option<Rat> {
        qsqrt(a)
    }
    fn as_rat(&self, a: &Rat) -> Option<Rat> {
        Some(a.clone())
    }
    fn square_split(&self, a: &Rat) -> (Rat, Rat) {
        // n/d = (1/d)^2 (n d)
        let d = Int::from_nat(a.den.clone());
        (Rat::new(Int::from_i64(1), d.clone()), Rat::from_int(a.num.mul(&d)))
    }
}

/// `K(θ)` with `θ^2 = s θ + n` irreducible over K: elements `a0 + a1 θ` as `[a0, a1]`. When `real`, s = 0, n > 0
/// and θ is the positive root `√n` (elements then have signs when K's elements do).
pub struct Quad<'a, K: Fld> {
    /// the base field
    pub k: &'a K,
    /// the coefficient of θ in θ^2
    pub s: K::E,
    /// the constant in θ^2
    pub n: K::E,
    /// θ = +√n, a real number
    pub real: bool,
}

impl<K: Fld> Fld for Quad<'_, K> {
    type E = [K::E; 2];
    fn zero(&self) -> Self::E {
        [self.k.zero(), self.k.zero()]
    }
    fn of(&self, c: &Rat) -> Self::E {
        [self.k.of(c), self.k.zero()]
    }
    fn is_zero(&self, a: &Self::E) -> bool {
        self.k.is_zero(&a[0]) && self.k.is_zero(&a[1])
    }
    fn add(&self, a: &Self::E, b: &Self::E) -> Self::E {
        [self.k.add(&a[0], &b[0]), self.k.add(&a[1], &b[1])]
    }
    fn neg(&self, a: &Self::E) -> Self::E {
        [self.k.neg(&a[0]), self.k.neg(&a[1])]
    }
    fn mul(&self, a: &Self::E, b: &Self::E) -> Self::E {
        // (a0 + a1 θ)(b0 + b1 θ) = a0 b0 + n a1 b1 + (a0 b1 + a1 b0 + s a1 b1) θ
        let k = self.k;
        let p = k.mul(&a[1], &b[1]);
        [
            k.add(&k.mul(&a[0], &b[0]), &k.mul(&self.n, &p)),
            k.add(&k.add(&k.mul(&a[0], &b[1]), &k.mul(&a[1], &b[0])), &k.mul(&self.s, &p)),
        ]
    }
    fn inv(&self, a: &Self::E) -> Option<Self::E> {
        // the conjugate uses θ' = s - θ; the norm a * conj(a) lies in K
        let k = self.k;
        let conj = [k.add(&a[0], &k.mul(&self.s, &a[1])), k.neg(&a[1])];
        let norm = self.mul(a, &conj);
        if !k.is_zero(&norm[1]) {
            return None;
        }
        let r = k.inv(&norm[0])?;
        Some([k.mul(&conj[0], &r), k.mul(&conj[1], &r)])
    }
    fn sign(&self, a: &Self::E) -> Option<i32> {
        let k = self.k;
        if k.is_zero(&a[1]) {
            return k.sign(&a[0]);
        }
        if !self.real {
            return None;
        }
        let (su, sv) = (k.sign(&a[0])?, k.sign(&a[1])?);
        if su == 0 || su == sv {
            return Some(sv);
        }
        // opposite signs: the larger of |u| and |v| √n wins
        let d = k.sub(&k.mul(&a[0], &a[0]), &k.mul(&self.n, &k.mul(&a[1], &a[1])));
        match k.sign(&d)? {
            1 => Some(su),
            -1 => Some(sv),
            _ => None,
        }
    }
    fn sqrt(&self, a: &Self::E) -> Option<Self::E> {
        let k = self.k;
        if k.is_zero(&a[1]) {
            if let Some(r) = k.sqrt(&a[0]) {
                return Some([r, k.zero()]);
            }
        }
        if !k.is_zero(&self.s) {
            return None;
        }
        // (r + w θ)^2 = r^2 + n w^2 + 2 r w θ
        if k.is_zero(&a[1]) {
            let w = k.sqrt(&k.div(&a[0], &self.n)?)?;
            return Some([k.zero(), w]);
        }
        let disc = k.sqrt(&k.sub(&k.mul(&a[0], &a[0]), &k.mul(&self.n, &k.mul(&a[1], &a[1]))))?;
        let half = k.of(&qq(1, 2));
        for d in [disc.clone(), k.neg(&disc)] {
            let r2 = k.mul(&half, &k.add(&a[0], &d));
            if let Some(r) = k.sqrt(&r2) {
                if !k.is_zero(&r) {
                    let w = k.div(&k.mul(&half, &a[1]), &r)?;
                    return Some([r, w]);
                }
            }
        }
        None
    }
    fn as_rat(&self, a: &Self::E) -> Option<Rat> {
        if self.k.is_zero(&a[1]) {
            self.k.as_rat(&a[0])
        } else {
            None
        }
    }
    fn square_split(&self, a: &Self::E) -> (Self::E, Self::E) {
        if self.k.is_zero(&a[1]) {
            let (s, m) = self.k.square_split(&a[0]);
            ([s, self.k.zero()], [m, self.k.zero()])
        } else {
            (self.one(), a.clone())
        }
    }
}

/// A polynomial: coefficients by degree, no trailing zeros.
pub type P<E> = Vec<E>;

/// Strip trailing zeros.
pub fn norm<K: Fld>(k: &K, mut p: P<K::E>) -> P<K::E> {
    while p.last().is_some_and(|c| k.is_zero(c)) {
        p.pop();
    }
    p
}

/// The degree (`None` for zero).
pub fn deg<E>(p: &[E]) -> Option<usize> {
    p.len().checked_sub(1)
}

/// The constant `c`.
pub fn constant<K: Fld>(k: &K, c: K::E) -> P<K::E> {
    norm(k, vec![c])
}

/// A polynomial over the base field of `k` lifted into `k`.
pub fn lift<K: Fld>(k: &Quad<'_, K>, p: &[K::E]) -> P<[K::E; 2]> {
    p.iter().map(|c| [c.clone(), k.k.zero()]).collect()
}

/// The two components of a polynomial over a quadratic field: `p = p0 + θ p1`.
pub fn split<K: Fld>(k: &Quad<'_, K>, p: &[[K::E; 2]]) -> (P<K::E>, P<K::E>) {
    (norm(k.k, p.iter().map(|e| e[0].clone()).collect()), norm(k.k, p.iter().map(|e| e[1].clone()).collect()))
}

/// `a + b`
pub fn add<K: Fld>(k: &K, a: &[K::E], b: &[K::E]) -> P<K::E> {
    let n = a.len().max(b.len());
    let z = k.zero();
    let out = (0..n).map(|i| k.add(a.get(i).unwrap_or(&z), b.get(i).unwrap_or(&z))).collect();
    norm(k, out)
}

/// `-a`
pub fn neg<K: Fld>(k: &K, a: &[K::E]) -> P<K::E> {
    a.iter().map(|c| k.neg(c)).collect()
}

/// `a - b`
pub fn sub<K: Fld>(k: &K, a: &[K::E], b: &[K::E]) -> P<K::E> {
    add(k, a, &neg(k, b))
}

/// `c * a`
pub fn scale<K: Fld>(k: &K, c: &K::E, a: &[K::E]) -> P<K::E> {
    norm(k, a.iter().map(|x| k.mul(c, x)).collect())
}

/// `a * b`
pub fn mul<K: Fld>(k: &K, a: &[K::E], b: &[K::E], tk: &mut dyn Ticker) -> R<P<K::E>> {
    if a.is_empty() || b.is_empty() {
        return Ok(vec![]);
    }
    tk.step((a.len() * b.len()) as u64)?;
    let mut out = vec![k.zero(); a.len() + b.len() - 1];
    for (i, x) in a.iter().enumerate() {
        if k.is_zero(x) {
            continue;
        }
        for (j, y) in b.iter().enumerate() {
            out[i + j] = k.add(&out[i + j], &k.mul(x, y));
        }
    }
    Ok(norm(k, out))
}

/// `a^e`
pub fn pow<K: Fld>(k: &K, a: &[K::E], e: u32, tk: &mut dyn Ticker) -> R<P<K::E>> {
    let mut out = constant(k, k.one());
    for _ in 0..e {
        out = mul(k, &out, a, tk)?;
    }
    Ok(out)
}

/// Quotient and remainder of `a` by `b` (b nonzero).
pub fn divrem<K: Fld>(k: &K, a: &[K::E], b: &[K::E], tk: &mut dyn Ticker) -> R<(P<K::E>, P<K::E>)> {
    let db = deg(b).ok_or(Stop::GiveUp)?;
    let inv = k.inv(&b[db]).ok_or(Stop::GiveUp)?;
    let mut r: P<K::E> = a.to_vec();
    if r.len() <= db {
        return Ok((vec![], r));
    }
    tk.step(((r.len() - db) * b.len()) as u64)?;
    let mut quo = vec![k.zero(); r.len() - db];
    for i in (db..r.len()).rev() {
        if k.is_zero(&r[i]) {
            continue;
        }
        let c = k.mul(&r[i], &inv);
        for j in 0..db {
            r[i - db + j] = k.sub(&r[i - db + j], &k.mul(&c, &b[j]));
        }
        r[i] = k.zero();
        quo[i - db] = c;
    }
    Ok((norm(k, quo), norm(k, r)))
}

/// The remainder of `a` by `b`.
pub fn rem<K: Fld>(k: &K, a: &[K::E], b: &[K::E], tk: &mut dyn Ticker) -> R<P<K::E>> {
    Ok(divrem(k, a, b, tk)?.1)
}

/// `a / b` when `b` divides `a`; gives up otherwise.
pub fn exact<K: Fld>(k: &K, a: &[K::E], b: &[K::E], tk: &mut dyn Ticker) -> R<P<K::E>> {
    let (qq, r) = divrem(k, a, b, tk)?;
    if r.is_empty() {
        Ok(qq)
    } else {
        Err(Stop::GiveUp)
    }
}

/// `a` divided by its leading coefficient (zero stays zero).
pub fn monic<K: Fld>(k: &K, a: &[K::E]) -> R<P<K::E>> {
    match a.last() {
        None => Ok(vec![]),
        Some(l) => {
            let inv = k.inv(l).ok_or(Stop::GiveUp)?;
            Ok(scale(k, &inv, a))
        }
    }
}

/// The derivative.
pub fn derivative<K: Fld>(k: &K, a: &[K::E]) -> P<K::E> {
    let out = a.iter().enumerate().skip(1).map(|(i, c)| k.mul(&k.of(&q(i as i64)), c)).collect();
    norm(k, out)
}

/// The monic gcd (zero for two zeros).
pub fn gcd<K: Fld>(k: &K, a: &[K::E], b: &[K::E], tk: &mut dyn Ticker) -> R<P<K::E>> {
    let (mut a, mut b) = (a.to_vec(), b.to_vec());
    while !b.is_empty() {
        let r = rem(k, &a, &b, tk)?;
        a = b;
        b = monic(k, &r)?;
    }
    monic(k, &a)
}

/// `(s, t, g)` with `s a + t b = g`, g the monic gcd (a, b not both zero).
pub fn ext_gcd<K: Fld>(k: &K, a: &[K::E], b: &[K::E], tk: &mut dyn Ticker) -> R<(P<K::E>, P<K::E>, P<K::E>)> {
    let (mut r0, mut r1) = (a.to_vec(), b.to_vec());
    let (mut s0, mut s1) = (constant(k, k.one()), vec![]);
    let (mut t0, mut t1) = (vec![], constant(k, k.one()));
    while !r1.is_empty() {
        let (qq, r) = divrem(k, &r0, &r1, tk)?;
        let s = sub(k, &s0, &mul(k, &qq, &s1, tk)?);
        let t = sub(k, &t0, &mul(k, &qq, &t1, tk)?);
        r0 = std::mem::replace(&mut r1, r);
        s0 = std::mem::replace(&mut s1, s);
        t0 = std::mem::replace(&mut t1, t);
    }
    let l = r0.last().ok_or(Stop::GiveUp)?;
    let inv = k.inv(l).ok_or(Stop::GiveUp)?;
    Ok((scale(k, &inv, &s0), scale(k, &inv, &t0), scale(k, &inv, &r0)))
}

/// `(s, t)` with `s a + t b = c` and `deg s < deg b` (gcd(a, b) must divide c).
pub fn diophantine<K: Fld>(
    k: &K,
    a: &[K::E],
    b: &[K::E],
    c: &[K::E],
    tk: &mut dyn Ticker,
) -> R<(P<K::E>, P<K::E>)> {
    let (s, _, g) = ext_gcd(k, a, b, tk)?;
    let c = exact(k, c, &g, tk)?;
    let s = rem(k, &mul(k, &s, &c, tk)?, b, tk)?;
    let t = exact(k, &sub(k, &c, &mul(k, &s, a, tk)?), b, tk)?;
    Ok((s, t))
}

/// `∏ g(α)` over the roots α of the monic `f` (with multiplicity): the resultant of f and g.
pub fn resultant_monic<K: Fld>(k: &K, f: &[K::E], g: &[K::E], tk: &mut dyn Ticker) -> R<K::E> {
    let mut f = f.to_vec();
    let mut g = rem(k, g, &f, tk)?;
    let mut acc = k.one();
    loop {
        let n = deg(&f).ok_or(Stop::GiveUp)?;
        if n == 0 {
            return Ok(acc);
        }
        let Some(m) = deg(&g) else { return Ok(k.zero()) };
        tk.step(n as u64)?;
        for _ in 0..n {
            acc = k.mul(&acc, &g[m]);
        }
        if m == 0 {
            return Ok(acc);
        }
        // ∏_α c ∏_β (α - β) = c^n (-1)^(n m) ∏_β f(β), with g = c ∏ (x - β)
        if (n * m) % 2 == 1 {
            acc = k.neg(&acc);
        }
        let gm = monic(k, &g)?;
        let r = rem(k, &f, &gm, tk)?;
        f = gm;
        g = r;
    }
}

/// The polynomial of degree below `ys.len()` through the points `(i, ys[i])` (Newton's divided differences).
pub fn interpolate<K: Fld>(k: &K, ys: &[K::E], tk: &mut dyn Ticker) -> R<P<K::E>> {
    let n = ys.len();
    let mut c = ys.to_vec();
    tk.step((n * n) as u64)?;
    for j in 1..n {
        let inv = k.of(&qq(1, j as i64));
        for i in (j..n).rev() {
            c[i] = k.mul(&k.sub(&c[i], &c[i - 1]), &inv);
        }
    }
    // Horner on the Newton form: p = c0 + (t - 0)(c1 + (t - 1)(c2 + ...))
    let mut p: P<K::E> = vec![];
    for i in (0..n).rev() {
        p = mul(k, &p, &[k.of(&q(-(i as i64))), k.one()], tk)?;
        p = add(k, &p, &constant(k, c[i].clone()));
    }
    Ok(p)
}

/// The least common multiple of the denominators of `p`, times `p` (an integer polynomial).
pub fn clear_denominators(p: &[Rat]) -> Vec<Int> {
    let mut l = Nat::from_u64(1);
    for c in p {
        let g = mc_arith::num::gcd::gcd(&l, &c.den);
        l = l.div(&g).mul(&c.den);
    }
    p.iter().map(|c| c.num.mul(&Int::from_nat(l.div(&c.den)))).collect()
}
