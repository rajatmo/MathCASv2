//! A finite field F_q, q = p^n, as F_p[x]/(f) for an irreducible f of degree n, and the arithmetic of its elements.
//!
//! Elements are coefficient vectors of their residues modulo f (index = power of x, no trailing zeros). Sums are
//! coefficientwise, products are polynomial products reduced modulo f, inverses come from Euclid's algorithm with
//! cofactors (Lidl and Niederreiter, Finite Fields, sections 1.4 and 2.1; von zur Gathen and Gerhard, Modern
//! Computer Algebra, section 4.2). The trace and norm to F_p, the minimal and characteristic polynomials, element
//! orders and primitivity follow Lidl and Niederreiter, sections 2.2-2.3 and 3.1.
//!
//! Discrete logarithms are by Pohlig and Hellman's reduction to the prime-order subgroups (Lidl and Niederreiter,
//! section 9.4), each solved by Shanks's baby-step giant-step method.
use crate::fp::{self, Fp, Poly};
use mc_arith::num::{Int, Nat};
use mc_eval::{Abort, Res};
use mc_numtheory::budget::Budget;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

/// An element: the coefficients of its residue modulo the defining polynomial.
pub type El = Vec<u64>;

/// How elements are written: by their coefficients, or by an index into the powers of the generator.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Repr {
    /// `finite_field_element(F, [c0, c1, ...])`
    Polynomial,
    /// `finite_field_element(F, k)`: 0 is zero, k >= 1 is x^(k - 1) (the defining polynomial is primitive)
    Exponential,
}

impl Repr {
    /// The name the language writes.
    pub fn name(self) -> &'static str {
        match self {
            Repr::Polynomial => "Polynomial",
            Repr::Exponential => "Exponential",
        }
    }
}

/// A finite field with its defining polynomial and cached data.
pub struct Field {
    /// the prime field
    pub fp: Fp,
    /// the extension degree
    pub n: usize,
    /// the monic defining polynomial (degree n)
    pub modulus: Poly,
    /// the defining polynomial as given (coefficients reduced modulo p; possibly not monic), which the field term
    /// shows
    pub given: Poly,
    /// the element representation
    pub repr: Repr,
    /// q = p^n
    pub q: Nat,
    /// q - 1, the order of the multiplicative group
    pub order: Nat,
    /// Tr(x^i) for i < n (Newton's identities on the defining polynomial)
    trace_vec: Vec<u64>,
    /// the factorisation of q - 1 when it was found
    factors: OnceLock<Option<Vec<(Nat, u64)>>>,
    /// whether the field was checked (p prime, the polynomial irreducible, primitive when exponential)
    checked: AtomicBool,
}

/// A budget with a cap: past it the computation gives up (`exhausted` tells a cap from a real interrupt).
pub struct Capped<'a> {
    /// the evaluator's budget
    pub inner: &'a mut dyn Budget,
    /// units left
    pub left: u64,
    /// whether the cap was reached
    pub exhausted: bool,
}

impl Budget for Capped<'_> {
    fn charge(&mut self, units: u64) -> Res<()> {
        if units > self.left {
            self.exhausted = true;
            return Err(Abort::Limit);
        }
        self.left -= units;
        self.inner.charge(units)
    }
}

impl<'a> Capped<'a> {
    /// A cap of `left` units over `inner`.
    pub fn new(inner: &'a mut dyn Budget, left: u64) -> Capped<'a> {
        Capped { inner, left, exhausted: false }
    }
    /// `Ok(None)` when the result is an error caused by the cap, the error when it is a real stop.
    pub fn settle<X>(&self, r: Res<X>) -> Res<Option<X>> {
        match r {
            Ok(v) => Ok(Some(v)),
            Err(_) if self.exhausted => Ok(None),
            Err(e) => Err(e),
        }
    }
}

/// The work units the factorisation of q - 1 may take before the field does without it.
pub const FACTOR_CAP: u64 = 2_000_000;

/// The shared fields by (p, defining polynomial, representation).
type Registry = Mutex<HashMap<(u64, Poly, Repr), Arc<Field>>>;

static REGISTRY: OnceLock<Registry> = OnceLock::new();

impl Field {
    /// The field F_p[x]/(f) (f irreducible of degree >= 1 modulo p; the caller checks). Fields are shared: the same
    /// prime, polynomial and representation give the same field.
    pub fn get(p: u64, given: &[u64], repr: Repr) -> Arc<Field> {
        let fp = Fp::new(p);
        let mut given = given.iter().map(|&c| c % p).collect::<Poly>();
        fp::trim(&mut given);
        let key = (p, given.clone(), repr);
        let reg = REGISTRY.get_or_init(|| Mutex::new(HashMap::new()));
        // invariant: the registry lock is never held across a panic-prone call
        if let Some(f) = reg.lock().unwrap().get(&key) {
            return f.clone();
        }
        let modulus = fp::monic(fp, &given);
        let n = modulus.len() - 1;
        let q = Nat::pow(p, n as u64);
        let order = q.sub_u64(1);
        let trace_vec = newton_traces(fp, &modulus);
        let f = Arc::new(Field {
            fp,
            n,
            modulus,
            given,
            repr,
            q,
            order,
            trace_vec,
            factors: OnceLock::new(),
            checked: AtomicBool::new(false),
        });
        let mut reg = reg.lock().unwrap();
        if reg.len() > 4096 {
            reg.clear();
        }
        reg.entry(key).or_insert(f).clone()
    }

    /// The checked field with these data, if it is in the registry.
    pub fn lookup(p: u64, given: &[u64], repr: Repr) -> Option<Arc<Field>> {
        let reg = REGISTRY.get_or_init(|| Mutex::new(HashMap::new()));
        let mut g: Poly = given.iter().map(|&c| c % p).collect();
        fp::trim(&mut g);
        reg.lock().unwrap().get(&(p, g, repr)).filter(|f| f.checked.load(Ordering::Relaxed)).cloned()
    }

    /// Record that the field passed the checks of `terms::checked_field`.
    pub fn mark_checked(f: &Field) {
        f.checked.store(true, Ordering::Relaxed);
    }

    /// Whether two fields are the same (characteristic, defining polynomial and representation).
    pub fn same(&self, o: &Field) -> bool {
        self.fp.p == o.fp.p && self.given == o.given && self.repr == o.repr
    }

    /// Whether two fields have the same elements (they may differ in the representation).
    pub fn same_elements(&self, o: &Field) -> bool {
        self.fp.p == o.fp.p && self.modulus == o.modulus
    }

    /// Tr(x^i) for i < n.
    pub fn trace_vector(&self) -> &[u64] {
        &self.trace_vec
    }

    /// The characteristic.
    pub fn p(&self) -> u64 {
        self.fp.p
    }

    /// The factorisation of q - 1 (cached), `None` when it was not found within [`FACTOR_CAP`].
    pub fn factors(&self, b: &mut dyn Budget) -> Res<Option<&[(Nat, u64)]>> {
        if let Some(f) = self.factors.get() {
            return Ok(f.as_deref());
        }
        let r = factor_capped(&self.order, b)?;
        let _ = self.factors.set(r);
        // invariant: set just above (or by a concurrent caller)
        Ok(self.factors.get().unwrap().as_deref())
    }

    /// Reduce a coefficient vector modulo the defining polynomial.
    pub fn reduce(&self, a: &[u64]) -> El {
        let mut v: Poly = a.iter().map(|&c| c % self.fp.p).collect();
        fp::trim(&mut v);
        if v.len() <= self.n {
            return v;
        }
        fp::rem(self.fp, &v, &self.modulus)
    }

    /// The element c of the prime field.
    pub fn constant(&self, c: u64) -> El {
        self.reduce(&[c % self.fp.p])
    }

    /// 1
    pub fn one(&self) -> El {
        vec![1]
    }

    /// The generator x mod f.
    pub fn gen(&self) -> El {
        self.reduce(&[0, 1])
    }

    /// a + b
    pub fn add(&self, a: &El, b: &El) -> El {
        fp::add(self.fp, a, b)
    }

    /// a - b
    pub fn sub(&self, a: &El, b: &El) -> El {
        fp::sub(self.fp, a, b)
    }

    /// -a
    pub fn neg(&self, a: &El) -> El {
        a.iter().map(|&c| self.fp.neg(c)).collect()
    }

    /// c a for c in F_p
    pub fn scale(&self, a: &El, c: u64) -> El {
        fp::scale(self.fp, a, c % self.fp.p)
    }

    /// a b
    pub fn mul(&self, a: &El, b: &El) -> El {
        let m = fp::mul(self.fp, a, b);
        if m.len() <= self.n {
            m
        } else {
            fp::rem(self.fp, &m, &self.modulus)
        }
    }

    /// 1/a (None for 0)
    pub fn inv(&self, a: &El) -> Option<El> {
        if a.is_empty() {
            return None;
        }
        let (g, s, _) = fp::xgcd(self.fp, a, &self.modulus);
        if g != [1] {
            return None;
        }
        Some(self.reduce(&s))
    }

    /// a^e for a natural exponent
    pub fn pow(&self, a: &El, e: &Nat) -> El {
        if a.is_empty() {
            return if e.is_zero() { self.one() } else { vec![] };
        }
        // a^(q-1) = 1: reduce big exponents
        let e = if e.bits() > self.order.bits() { e.divrem(&self.order).1 } else { e.clone() };
        let mut r = self.one();
        for i in (0..e.bits()).rev() {
            r = self.mul(&r, &r);
            if e.bit(i) {
                r = self.mul(&r, a);
            }
        }
        r
    }

    /// a^e for a machine exponent
    pub fn pow_u64(&self, a: &El, e: u64) -> El {
        self.pow(a, &Nat::from_u64(e))
    }

    /// a^e for a signed exponent (None for 0^negative)
    pub fn pow_int(&self, a: &El, e: &Int) -> Option<El> {
        if e.neg {
            let i = self.inv(a)?;
            Some(self.pow(&i, &e.mag))
        } else {
            Some(self.pow(a, &e.mag))
        }
    }

    /// a^(p^k), the k-th power of the Frobenius automorphism
    pub fn frobenius(&self, a: &El, k: usize) -> El {
        let mut r = a.clone();
        let p = Nat::from_u64(self.fp.p);
        for _ in 0..(k % self.n.max(1)) {
            r = self.pow(&r, &p);
        }
        r
    }

    /// The trace to F_p.
    pub fn trace(&self, a: &El) -> u64 {
        a.iter().zip(&self.trace_vec).fold(0, |s, (&c, &t)| self.fp.add(s, self.fp.mul(c, t)))
    }

    /// The norm to F_p: a^((q - 1)/(p - 1)).
    pub fn norm(&self, a: &El) -> u64 {
        if a.is_empty() {
            return 0;
        }
        let e = self.order.divrem_u64(self.fp.p - 1).0;
        let r = self.pow(a, &e);
        r.first().copied().unwrap_or(0)
    }

    /// The conjugates a, a^p, a^(p^2), ... until they repeat (as many as the degree of a over F_p).
    pub fn conjugates(&self, a: &El) -> Vec<El> {
        let p = Nat::from_u64(self.fp.p);
        let mut out = vec![a.clone()];
        loop {
            // invariant: out is nonempty
            let next = self.pow(out.last().unwrap(), &p);
            if next == *a || out.len() >= self.n {
                break;
            }
            out.push(next);
        }
        out
    }

    /// The minimal polynomial of a over F_p (monic, coefficients in F_p): the product of x - c over the conjugates.
    pub fn minimal_polynomial(&self, a: &El) -> Poly {
        let cs = self.conjugates(a);
        self.product_of_linear(&cs)
    }

    /// The characteristic polynomial of a over F_p (degree n): the minimal polynomial to the power n/deg.
    pub fn characteristic_polynomial(&self, a: &El) -> Poly {
        let m = self.minimal_polynomial(a);
        let k = self.n / (m.len() - 1);
        let mut r = vec![1u64];
        for _ in 0..k {
            r = fp::mul(self.fp, &r, &m);
        }
        r
    }

    /// prod (X - c_i) over F_q, whose coefficients lie in F_p (the c_i a full set of conjugates)
    fn product_of_linear(&self, cs: &[El]) -> Poly {
        let mut prod: Vec<El> = vec![self.one()];
        for c in cs {
            let mut next: Vec<El> = vec![vec![]; prod.len() + 1];
            for (i, t) in prod.iter().enumerate() {
                next[i + 1] = self.add(&next[i + 1], t);
                next[i] = self.sub(&next[i], &self.mul(t, c));
            }
            prod = next;
        }
        prod.iter().map(|e| e.first().copied().unwrap_or(0)).collect()
    }

    /// The degree of a over F_p (the degree of the smallest subfield containing it).
    pub fn degree_of(&self, a: &El) -> usize {
        self.conjugates(a).len()
    }

    /// The multiplicative order of a != 0, when q - 1 could be factored.
    pub fn element_order(&self, a: &El, b: &mut dyn Budget) -> Res<Option<Nat>> {
        if a.is_empty() {
            return Ok(None);
        }
        let Some(fs) = self.factors(b)? else { return Ok(None) };
        let mut ord = self.order.clone();
        for (r, e) in fs {
            for _ in 0..*e {
                let (cand, rem) = ord.divrem(r);
                debug_assert!(rem.is_zero());
                if self.pow(a, &cand) == self.one() {
                    ord = cand;
                } else {
                    break;
                }
            }
            b.charge(self.n as u64 * self.n as u64)?;
        }
        Ok(Some(ord))
    }

    /// Whether a generates the multiplicative group (None when q - 1 could not be factored).
    pub fn is_primitive(&self, a: &El, b: &mut dyn Budget) -> Res<Option<bool>> {
        if a.is_empty() {
            return Ok(Some(false));
        }
        let Some(fs) = self.factors(b)? else { return Ok(None) };
        for (r, _) in fs {
            let e = self.order.div(r);
            if self.pow(a, &e) == self.one() {
                return Ok(Some(false));
            }
        }
        Ok(Some(true))
    }

    /// The element with polynomial index k: the coefficients are the base-p digits of k (k < q).
    pub fn from_index(&self, k: &Nat) -> El {
        let mut v = vec![];
        let mut k = k.clone();
        while !k.is_zero() {
            let (q, r) = k.divrem_u64(self.fp.p);
            v.push(r);
            k = q;
        }
        self.reduce(&v)
    }

    /// The polynomial index sum c_i p^i.
    pub fn poly_index(&self, a: &El) -> Nat {
        a.iter().rev().fold(Nat::zero(), |acc, &c| acc.mul_u64(self.fp.p).add_u64(c))
    }

    /// The element of exponential index k: 0 for k = 0, x^(k - 1) otherwise.
    pub fn from_exp_index(&self, k: &Nat) -> El {
        if k.is_zero() {
            return vec![];
        }
        self.pow(&self.gen(), &k.sub_u64(1))
    }

    /// The exponential index of a (0 for zero, 1 + log_x a otherwise); None when the logarithm was not found.
    pub fn exp_index(&self, a: &El, b: &mut dyn Budget) -> Res<Option<Nat>> {
        if a.is_empty() {
            return Ok(Some(Nat::zero()));
        }
        Ok(self.log(a, &self.gen(), b)?.map(|k| k.add_u64(1)))
    }

    /// The k with g^k = a, 0 <= k < ord(g), for a generator g of the whole group, or more generally when a is a
    /// power of g; None when there is none or it was not found.
    pub fn log(&self, a: &El, g: &El, b: &mut dyn Budget) -> Res<Option<Nat>> {
        if a.is_empty() || g.is_empty() {
            return Ok(None);
        }
        let Some(fs) = self.factors(b)?.map(|f| f.to_vec()) else { return Ok(None) };
        let Some(ord) = self.element_order(g, b)? else { return Ok(None) };
        // the factorisation of ord(g) from that of q - 1
        let mut parts = vec![];
        let mut rest = ord.clone();
        for (r, _) in &fs {
            let mut e = 0u64;
            loop {
                let (qq, rm) = rest.divrem(r);
                if !rm.is_zero() {
                    break;
                }
                rest = qq;
                e += 1;
            }
            if e > 0 {
                parts.push((r.clone(), e));
            }
        }
        // a must lie in <g>: a^ord(g) = 1
        if self.pow(a, &ord) != self.one() {
            return Ok(None);
        }
        let mut residues: Vec<(Nat, Nat)> = vec![];
        for (r, e) in &parts {
            let Some(r64) = r.to_u128().filter(|&v| v < 1 << 46).map(|v| v as u64) else { return Ok(None) };
            let re = Nat::pow(r64, *e);
            let cof = ord.div(&re);
            // gamma of order r; h = a^(ord/r^e) in the subgroup of order r^e
            let gamma = self.pow(g, &ord.div(r));
            let g_e = self.pow(g, &cof);
            let h = self.pow(a, &cof);
            let mut x = Nat::zero();
            let mut rk = Nat::from_u64(1);
            for k in 0..*e {
                // (h g_e^(-x))^(r^(e-1-k)) = gamma^(d_k)
                let gx = self.pow(&g_e, &x);
                // invariant: g_e is a power of g != 0
                let t = self.mul(&h, &self.inv(&gx).expect("a unit"));
                let t = self.pow(&t, &Nat::pow(r64, e - 1 - k));
                let Some(d) = self.bsgs(&t, &gamma, r64, b)? else { return Ok(None) };
                x = x.add(&rk.mul_u64(d));
                rk = rk.mul_u64(r64);
            }
            residues.push((x, re));
        }
        Ok(crt(&residues, b)?.map(|(x, _)| x))
    }

    /// The d < r with gamma^d = t, gamma of prime order r (baby steps, giant steps).
    fn bsgs(&self, t: &El, gamma: &El, r: u64, b: &mut dyn Budget) -> Res<Option<u64>> {
        if r <= 64 {
            let mut c = self.one();
            for d in 0..r {
                if c == *t {
                    return Ok(Some(d));
                }
                c = self.mul(&c, gamma);
            }
            return Ok(None);
        }
        let m = ((r as f64).sqrt().ceil() as u64).max(1);
        let mut table: HashMap<El, u64> = HashMap::with_capacity(m as usize);
        let mut c = self.one();
        for j in 0..m {
            table.entry(c.clone()).or_insert(j);
            c = self.mul(&c, gamma);
            if j % 1024 == 0 {
                b.charge(self.n as u64 * self.n as u64)?;
            }
        }
        // invariant: gamma has order r, a unit
        let step = self.inv(&self.pow_u64(gamma, m)).expect("a unit");
        let mut y = t.clone();
        for i in 0..m {
            if let Some(&j) = table.get(&y) {
                return Ok(Some((i * m + j) % r));
            }
            y = self.mul(&y, &step);
            if i % 1024 == 0 {
                b.charge(self.n as u64 * self.n as u64)?;
            }
        }
        Ok(None)
    }
}

/// Tr(x^i), i < n, for the roots of the monic f: Newton's identities s_k = -(k c_{n-k} + sum_{i<k} c_{n-i} s_{k-i}).
fn newton_traces(f: Fp, m: &[u64]) -> Vec<u64> {
    let n = m.len() - 1;
    let c = |i: usize| m[i];
    let mut s = vec![0u64; n.max(1)];
    s[0] = f.of_i64(n as i64);
    for k in 1..n {
        let mut v = f.mul((k as u64) % f.p, c(n - k));
        for i in 1..k {
            v = f.add(v, f.mul(c(n - i), s[k - i]));
        }
        s[k] = f.neg(v);
    }
    s
}

/// The factorisation of n >= 1 within [`FACTOR_CAP`] units.
pub fn factor_capped(n: &Nat, b: &mut dyn Budget) -> Res<Option<Vec<(Nat, u64)>>> {
    if *n == Nat::from_u64(1) {
        return Ok(Some(vec![]));
    }
    let mut c = Capped::new(b, FACTOR_CAP);
    let r = mc_numtheory::factor::factor(n, &mut c);
    c.settle(r)
}

/// The solution of x = a_i mod m_i (moduli not necessarily coprime): (x, lcm) or None when inconsistent.
pub fn crt(rs: &[(Nat, Nat)], b: &mut dyn Budget) -> Res<Option<(Nat, Nat)>> {
    let mut x = Nat::zero();
    let mut m = Nat::from_u64(1);
    for (a, mi) in rs {
        let (g, s, _) = mc_numtheory::modular::extended_gcd(&Int::from_nat(m.clone()), &Int::from_nat(mi.clone()), b)?;
        // x + m t = a mod mi: m t = a - x mod mi, solvable when g | a - x
        let diff = Int::from_nat(a.clone()).sub(&Int::from_nat(x.clone()));
        let (dq, dr) = diff.divrem_floor(&g);
        if !dr.is_zero() {
            return Ok(None);
        }
        let mg = mi.div(&g);
        let t = dq.mul(&s).divrem_floor(&mg).1;
        x = x.add(&m.mul(&t));
        m = m.mul(&mg);
        x = x.divrem(&m).1;
    }
    Ok(Some((x, m)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mc_numtheory::budget::Unlimited;

    fn f17_3() -> Arc<Field> {
        Field::get(17, &[14, 1, 0, 1], Repr::Polynomial)
    }

    #[test]
    fn arithmetic_and_indices() {
        let f = f17_3();
        let a = vec![1, 2, 3];
        let b = f.from_index(&Nat::from_u64(123));
        assert_eq!(b, vec![4, 7]);
        assert_eq!(f.poly_index(&b), Nat::from_u64(123));
        let ia = f.inv(&a).unwrap();
        assert_eq!(f.mul(&a, &ia), vec![1]);
        // (a^2 + 2 b)/(5 a b - 7)
        let num = f.add(&f.mul(&a, &a), &f.scale(&b, 2));
        let den = f.sub(&f.scale(&f.mul(&a, &b), 5), &f.constant(7));
        assert_eq!(f.mul(&num, &f.inv(&den).unwrap()), vec![13, 10, 2]);
    }

    #[test]
    fn trace_norm_minpoly() {
        let f = f17_3();
        let x = f.gen();
        assert_eq!(f.minimal_polynomial(&x), vec![14, 1, 0, 1]);
        assert_eq!(f.trace(&x), 0);
        assert_eq!(f.norm(&x), 3);
        let a = vec![1, 2, 3];
        let s = f.conjugates(&a).into_iter().fold(vec![], |s, c| f.add(&s, &c));
        assert_eq!(s, f.constant(f.trace(&a)));
        assert_eq!(f.minimal_polynomial(&f.constant(5)), vec![12, 1]);
    }

    #[test]
    fn discrete_logs() {
        let f = f17_3();
        let x = f.gen();
        assert_eq!(f.is_primitive(&x, &mut Unlimited).unwrap(), Some(true));
        for k in [0u64, 1, 2, 77, 4911, 1234] {
            let a = f.pow_u64(&x, k);
            assert_eq!(f.log(&a, &x, &mut Unlimited).unwrap(), Some(Nat::from_u64(k)));
        }
        let g = Field::get(7, &[4, 0, 6, 1], Repr::Exponential);
        assert_eq!(g.exp_index(&g.pow_u64(&g.gen(), 77), &mut Unlimited).unwrap(), Some(Nat::from_u64(78)));
    }

    #[test]
    fn chinese_remainders() {
        let r = crt(&[(Nat::from_u64(2), Nat::from_u64(6)), (Nat::from_u64(5), Nat::from_u64(9))], &mut Unlimited)
            .unwrap()
            .unwrap();
        assert_eq!(r, (Nat::from_u64(14), Nat::from_u64(18)));
        assert!(crt(&[(Nat::from_u64(1), Nat::from_u64(6)), (Nat::from_u64(2), Nat::from_u64(4))], &mut Unlimited)
            .unwrap()
            .is_none());
    }
}
