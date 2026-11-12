//! Rational functions: `together` and `cancel` (decision K.10).
//!
//! A term is read as a quotient of integer polynomials in its kernels (negative integer powers are denominators
//! here, not kernels), combined over common denominators with gcds at every step, so the result is in lowest
//! terms: numerator and denominator coprime, the denominator with a positive leading coefficient.
use crate::convert::{Conv, Node, Ring};
use crate::gcd::gcd;
use crate::poly::Poly;
use crate::qpoly::rat_parts;
use crate::work::{Meter, WorkResult};
use crate::z::Z;
use mc_api::{Store, T};

/// `num / den` in lowest terms (den nonzero with a positive leading coefficient).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RatFun {
    /// the numerator
    pub num: Poly,
    /// the denominator
    pub den: Poly,
}

/// Division by the zero polynomial.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DivisionByZero;

impl RatFun {
    /// The constant `n / d`.
    pub fn constant(nv: usize, n: Z, d: Z) -> RatFun {
        RatFun { num: Poly::constant(nv, n), den: Poly::constant(nv, d) }
    }
    fn div(a: &Poly, b: &Poly, m: &mut Meter) -> WorkResult<Poly> {
        crate::work::exact(a.div_exact(b, m)?, "a gcd divides")
    }
    /// Lowest terms with a positive leading coefficient in the denominator.
    pub fn reduced(num: Poly, den: Poly, m: &mut Meter) -> WorkResult<RatFun> {
        let nv = num.nv;
        if num.is_zero() {
            return Ok(RatFun::constant(nv, Z::ZERO, Z::ONE));
        }
        let g = gcd(&num, &den, m)?;
        let (mut n, mut d) = if g.is_constant() && g.as_constant() == Some(Z::ONE) {
            (num, den)
        } else {
            (Self::div(&num, &g, m)?, Self::div(&den, &g, m)?)
        };
        if d.lc().is_neg() {
            n = n.neg();
            d = d.neg();
        }
        Ok(RatFun { num: n, den: d })
    }
    /// The sum.
    pub fn add(&self, o: &RatFun, m: &mut Meter) -> WorkResult<RatFun> {
        let g = gcd(&self.den, &o.den, m)?;
        let (a, b) = (Self::div(&o.den, &g, m)?, Self::div(&self.den, &g, m)?);
        let num = self.num.mul(&a, m)?.add(&o.num.mul(&b, m)?);
        let den = self.den.mul(&a, m)?;
        Self::reduced(num, den, m)
    }
    /// The product.
    pub fn mul(&self, o: &RatFun, m: &mut Meter) -> WorkResult<RatFun> {
        let g1 = gcd(&self.num, &o.den, m)?;
        let g2 = gcd(&o.num, &self.den, m)?;
        let num = Self::div(&self.num, &g1, m)?.mul(&Self::div(&o.num, &g2, m)?, m)?;
        let den = Self::div(&self.den, &g2, m)?.mul(&Self::div(&o.den, &g1, m)?, m)?;
        Self::reduced(num, den, m)
    }
    /// The power `self^e`.
    pub fn pow(&self, e: u32, m: &mut Meter) -> WorkResult<RatFun> {
        Ok(RatFun { num: self.num.pow(e as u64, m)?, den: self.den.pow(e as u64, m)? })
    }
    /// The power `self^-e`.
    pub fn inv(&self, e: u32, m: &mut Meter) -> WorkResult<Result<RatFun, DivisionByZero>> {
        if self.num.is_zero() {
            return Ok(Err(DivisionByZero));
        }
        let (mut n, mut d) = (self.den.pow(e as u64, m)?, self.num.pow(e as u64, m)?);
        if d.lc().is_neg() {
            n = n.neg();
            d = d.neg();
        }
        Ok(Ok(RatFun { num: n, den: d }))
    }
}

/// The rational function of `t` in the kernels of `ring` (from [`Conv::ring`] of a rational converter).
pub fn read<'h, S: Store + ?Sized>(
    c: &Conv<'_, 'h, S>,
    t: T<'h>,
    ring: &Ring<'h>,
    m: &mut Meter,
) -> WorkResult<Result<RatFun, DivisionByZero>> {
    enum Task<'h> {
        Visit(T<'h>),
        Add(usize),
        Mul(usize),
        Pow(u32),
        Inv(u32),
    }
    let nv = ring.nvars();
    let mut tasks = vec![Task::Visit(t)];
    let mut vals: Vec<RatFun> = vec![];
    while let Some(task) = tasks.pop() {
        m.step(1)?;
        match task {
            Task::Visit(t) => match c.node(t) {
                Node::Const(q) => {
                    let (n, d) = rat_parts(&q);
                    vals.push(RatFun::constant(nv, n, d));
                }
                Node::Kernel(k, e) => {
                    let mut ex = vec![0; nv];
                    // invariant: the ring was built from every kernel the reader found
                    ex[ring.var(k).expect("the ring holds every kernel")] = e;
                    vals.push(RatFun { num: Poly::monomial(&ex, Z::ONE), den: Poly::constant(nv, Z::ONE) });
                }
                Node::Add(xs) => {
                    tasks.push(Task::Add(xs.len()));
                    tasks.extend(xs.iter().rev().map(|&x| Task::Visit(x)));
                }
                Node::Mul(xs) => {
                    tasks.push(Task::Mul(xs.len()));
                    tasks.extend(xs.iter().rev().map(|&x| Task::Visit(x)));
                }
                Node::Pow(b, e) => {
                    tasks.push(Task::Pow(e));
                    tasks.push(Task::Visit(b));
                }
                Node::Inv(b, e) => {
                    tasks.push(Task::Inv(e));
                    tasks.push(Task::Visit(b));
                }
            },
            Task::Add(n) => {
                let xs = vals.split_off(vals.len() - n);
                let mut acc = RatFun::constant(nv, Z::ZERO, Z::ONE);
                for x in &xs {
                    acc = acc.add(x, m)?;
                }
                vals.push(acc);
            }
            Task::Mul(n) => {
                let xs = vals.split_off(vals.len() - n);
                let mut acc = RatFun::constant(nv, Z::ONE, Z::ONE);
                for x in &xs {
                    acc = acc.mul(x, m)?;
                }
                vals.push(acc);
            }
            Task::Pow(e) => {
                // invariant: every Pow task follows the visit of its base, which pushed one value
                let b = vals.pop().expect("a base");
                vals.push(b.pow(e, m)?);
            }
            Task::Inv(e) => {
                // invariant: every Pow task follows the visit of its base, which pushed one value
                let b = vals.pop().expect("a base");
                match b.inv(e, m)? {
                    Ok(v) => vals.push(v),
                    Err(z) => return Ok(Err(z)),
                }
            }
        }
    }
    // invariant: the tasks of one term leave exactly one value
    Ok(Ok(vals.pop().expect("one value")))
}

/// The term `num / den`, both expanded.
pub fn to_term<'h, S: Store + ?Sized>(
    c: &Conv<'_, 'h, S>,
    f: &RatFun,
    ring: &Ring<'h>,
    m: &mut Meter,
) -> WorkResult<T<'h>> {
    let n = c.int_poly_term(&f.num, ring, m)?;
    if f.den.as_constant() == Some(Z::ONE) {
        return Ok(n);
    }
    let d = c.int_poly_term(&f.den, ring, m)?;
    let inv = c.ar.pow(d, c.ar.store().int(-1));
    Ok(c.ar.mul(&[n, inv]))
}
