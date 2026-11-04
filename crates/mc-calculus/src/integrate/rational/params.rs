//! The coefficient field of an integrand with parameters: rational functions over Q of the kernels other than `x`
//! (symbols such as `a`, or calls and radicals free of `x` such as `sqrt(2)`, all taken as independent
//! transcendentals), in the polynomial ring `mc-poly` read the integrand in.
//!
//! An element is `num/den` in lowest terms (gcd 1, positive leading coefficient in the denominator), so equal
//! elements are equal structs. Signs are known only for constants; square roots are found through the square-free
//! factorisation of numerator and denominator. The `mc-poly` kernels inside the field operations poll the
//! arithmetic stop flag; a kernel that fails marks the field as failed and the integral is given up.
use super::logpart::Base;
use super::upoly::{monic, Fld, Stop, Ticker, P, R};
use super::Budget;
use mc_arith::num::{Int, Rat};
use mc_core::Heap;
use mc_api::{Store, T};
use mc_poly::convert::{Conv, Ring};
use mc_poly::work::{Halt, Meter, Work, WorkResult};
use mc_poly::{Poly, Z};
use std::cell::Cell;

/// Polls the arithmetic stop flag only (field operations cannot reach the evaluator's fuel).
struct StopPoll;

impl Work for StopPoll {
    fn charge(&mut self, _units: u64) -> WorkResult<()> {
        if mc_arith::num::stopped() {
            Err(Halt::Stopped)
        } else {
            Ok(())
        }
    }
}

/// An element: `num/den` in lowest terms.
#[derive(Clone, PartialEq, Debug)]
pub struct Elem {
    num: Poly,
    den: Poly,
}

/// The field of rational functions of the parameters.
pub struct Params<'c, 'a, 'h> {
    conv: &'c Conv<'a, 'h, Heap>,
    ring: &'c Ring<'h>,
    xi: usize,
    failed: Cell<bool>,
}

impl<'c, 'a, 'h> Params<'c, 'a, 'h> {
    /// The field of the kernels of `ring` other than variable `xi` (x).
    pub fn new(conv: &'c Conv<'a, 'h, Heap>, ring: &'c Ring<'h>, xi: usize) -> Self {
        Params { conv, ring, xi, failed: Cell::new(false) }
    }
    /// Did a kernel fail inside a field operation (the results are then meaningless)?
    pub fn failed(&self) -> bool {
        self.failed.get()
    }
    fn nv(&self) -> usize {
        self.ring.nvars()
    }
    fn ok<X>(&self, r: WorkResult<X>, fallback: impl FnOnce() -> X) -> X {
        r.unwrap_or_else(|_| {
            self.failed.set(true);
            fallback()
        })
    }
    fn pmul(&self, a: &Poly, b: &Poly) -> Poly {
        if a.is_zero() || b.is_zero() {
            return Poly::zero(self.nv());
        }
        let mut w = StopPoll;
        let r = a.mul(b, &mut Meter::new(&mut w));
        self.ok(r, || Poly::zero(self.nv()))
    }
    fn pgcd(&self, a: &Poly, b: &Poly) -> Poly {
        let mut w = StopPoll;
        let r = mc_poly::gcd::gcd(a, b, &mut Meter::new(&mut w));
        self.ok(r, || Poly::constant(self.nv(), Z::ONE))
    }
    fn pdiv(&self, a: &Poly, b: &Poly) -> Poly {
        let mut w = StopPoll;
        let r = a.div_exact(b, &mut Meter::new(&mut w));
        match r {
            Ok(Some(q)) => q,
            _ => {
                self.failed.set(true);
                Poly::zero(self.nv())
            }
        }
    }
    fn one_poly(&self) -> Poly {
        Poly::constant(self.nv(), Z::ONE)
    }
    /// `num/den` in lowest terms (den nonzero).
    fn reduce(&self, num: Poly, den: Poly) -> Elem {
        if num.is_zero() || den.is_zero() {
            if den.is_zero() {
                self.failed.set(true);
            }
            return Elem { num: Poly::zero(self.nv()), den: self.one_poly() };
        }
        let g = self.pgcd(&num, &den);
        let (mut n, mut d) =
            if g.as_constant() == Some(Z::ONE) { (num, den) } else { (self.pdiv(&num, &g), self.pdiv(&den, &g)) };
        if d.lc().is_neg() {
            n = n.neg();
            d = d.neg();
        }
        if d.is_zero() {
            self.failed.set(true);
            d = self.one_poly();
        }
        Elem { num: n, den: d }
    }
    /// A polynomial of the ring as a polynomial in x over this field.
    pub fn dense_in_x(&self, p: &Poly) -> P<Elem> {
        let cs = p.coeffs_in(self.xi);
        let n = cs.first().map_or(0, |c| c.0 as usize + 1);
        let mut out = vec![self.zero(); n];
        for (e, c) in cs {
            out[e as usize] = Elem { num: c, den: self.one_poly() };
        }
        super::upoly::norm(self, out)
    }
    /// The term of an element.
    pub fn term(&self, c: &Elem) -> T<'h> {
        let mut w = StopPoll;
        let mut m = Meter::new(&mut w);
        let r = self.conv.int_poly_term(&c.num, self.ring, &mut m).and_then(|n| {
            if c.den.as_constant() == Some(Z::ONE) {
                return Ok(n);
            }
            let d = self.conv.int_poly_term(&c.den, self.ring, &mut m)?;
            let ar = self.conv.ar;
            Ok(ar.mul(&[n, ar.pow(d, ar.store().int(-1))]))
        });
        self.ok(r, || self.conv.ar.store().int(0))
    }
    /// `(s, m)` with `p = s^2 m` from the square-free factorisation (the integer content counts as a square only
    /// when it is one).
    fn psplit(&self, p: &Poly) -> (Poly, Poly) {
        let mut w = StopPoll;
        let Ok(f) = mc_poly::factor::factor(p, true, &mut Meter::new(&mut w)) else {
            self.failed.set(true);
            return (self.one_poly(), p.clone());
        };
        let unit = Rat::from_int(f.unit.to_int());
        let (mut s, mut m) = match super::upoly::qsqrt(&unit) {
            Some(u) => (Poly::constant(self.nv(), Z::from_int(u.num)), self.one_poly()),
            None => (self.one_poly(), Poly::constant(self.nv(), f.unit.clone())),
        };
        for (g, e) in &f.factors {
            for _ in 0..e / 2 {
                s = self.pmul(&s, g);
            }
            if e % 2 == 1 {
                m = self.pmul(&m, g);
            }
        }
        (s, m)
    }
    /// The square root of a polynomial that is a square (over Z).
    fn psqrt(&self, p: &Poly) -> Option<Poly> {
        let mut w = StopPoll;
        let f = mc_poly::factor::factor(p, true, &mut Meter::new(&mut w)).ok()?;
        if f.unit.is_neg() {
            return None;
        }
        let u = super::upoly::qsqrt(&Rat::from_int(f.unit.to_int()))?;
        let mut out = Poly::constant(self.nv(), Z::from_int(u.num));
        for (g, e) in &f.factors {
            if e % 2 == 1 {
                return None;
            }
            for _ in 0..e / 2 {
                out = self.pmul(&out, g);
            }
        }
        Some(out)
    }
}

impl Fld for Params<'_, '_, '_> {
    type E = Elem;
    fn zero(&self) -> Elem {
        Elem { num: Poly::zero(self.nv()), den: self.one_poly() }
    }
    fn of(&self, c: &Rat) -> Elem {
        let n = Poly::constant(self.nv(), Z::from_int(c.num.clone()));
        let d = Poly::constant(self.nv(), Z::from_int(Int::from_nat(c.den.clone())));
        Elem { num: n, den: d }
    }
    fn is_zero(&self, a: &Elem) -> bool {
        a.num.is_zero()
    }
    fn add(&self, a: &Elem, b: &Elem) -> Elem {
        if a.num.is_zero() {
            return b.clone();
        }
        if b.num.is_zero() {
            return a.clone();
        }
        if a.den == b.den {
            return self.reduce(a.num.add(&b.num), a.den.clone());
        }
        let n = self.pmul(&a.num, &b.den).add(&self.pmul(&b.num, &a.den));
        self.reduce(n, self.pmul(&a.den, &b.den))
    }
    fn neg(&self, a: &Elem) -> Elem {
        Elem { num: a.num.neg(), den: a.den.clone() }
    }
    fn mul(&self, a: &Elem, b: &Elem) -> Elem {
        if a.num.is_zero() || b.num.is_zero() {
            return self.zero();
        }
        self.reduce(self.pmul(&a.num, &b.num), self.pmul(&a.den, &b.den))
    }
    fn inv(&self, a: &Elem) -> Option<Elem> {
        if a.num.is_zero() {
            return None;
        }
        let (mut n, mut d) = (a.den.clone(), a.num.clone());
        if d.lc().is_neg() {
            n = n.neg();
            d = d.neg();
        }
        Some(Elem { num: n, den: d })
    }
    fn sign(&self, a: &Elem) -> Option<i32> {
        let r = self.as_rat(a)?;
        super::upoly::Q.sign(&r)
    }
    fn sqrt(&self, a: &Elem) -> Option<Elem> {
        if a.num.is_zero() {
            return Some(self.zero());
        }
        let n = self.psqrt(&a.num)?;
        let d = self.psqrt(&a.den)?;
        Some(self.reduce(n, d))
    }
    fn square_split(&self, a: &Elem) -> (Elem, Elem) {
        if a.num.is_zero() {
            return (self.one(), self.zero());
        }
        // N/D = (sN/(sD D'))^2 N' D' with N = sN^2 N', D = sD^2 D'
        let (sn, n1) = self.psplit(&a.num);
        let (sd, d1) = self.psplit(&a.den);
        let s = self.reduce(sn, self.pmul(&sd, &d1));
        let m = self.reduce(self.pmul(&n1, &d1), self.one_poly());
        (s, m)
    }
    fn as_rat(&self, a: &Elem) -> Option<Rat> {
        let n = a.num.as_constant()?;
        let d = a.den.as_constant()?;
        Some(Rat::new(n.to_int(), d.to_int()))
    }
}

impl Base for Params<'_, '_, '_> {
    fn factor(&self, p: &[Elem], tk: &mut dyn Ticker) -> R<Vec<P<Elem>>> {
        // clear the denominators: p * lcm(dens) has coefficients in Z[params]; t takes the place of x
        let mut l = self.one_poly();
        for c in p {
            let g = self.pgcd(&l, &c.den);
            l = self.pmul(&self.pdiv(&l, &g), &c.den);
        }
        let mut poly = Poly::zero(self.nv());
        let mut e = vec![0u32; self.nv()];
        for (i, c) in p.iter().enumerate() {
            e[self.xi] = i as u32;
            let ci = self.pmul(&c.num, &self.pdiv(&l, &c.den));
            poly = poly.add(&ci.mul_term(&e, &Z::ONE));
        }
        if self.failed() {
            return Err(Stop::GiveUp);
        }
        let mut budget = Budget::new(tk);
        let res = {
            let mut m = Meter::new(&mut budget);
            mc_poly::factor::factor(&poly, false, &mut m)
        };
        let fs = res.map_err(|_| budget.stop())?;
        let mut out = vec![];
        for (f, _) in &fs.factors {
            if f.degree(self.xi).unwrap_or(0) == 0 {
                continue;
            }
            out.push(monic(self, &self.dense_in_x(f))?);
        }
        if self.failed() {
            return Err(Stop::GiveUp);
        }
        Ok(out)
    }
    fn looks_negative(&self, a: &Elem) -> bool {
        // the sign of the leading numeric coefficient (the denominator's is positive)
        a.num.lc().is_neg()
    }
}
