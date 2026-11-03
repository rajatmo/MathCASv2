//! The canonicaliser of `add`, `mul` and `pow` (semantics 3).
//!
//! Every arithmetic construction goes through [`Arith`]. The arguments must already be canonical (built through
//! this canonicaliser, or atoms); only the top level is canonicalised, as evaluation builds terms bottom-up (B.1).
//!
//! A sum or product is flattened, its numbers are folded with contagion, like terms are collected (`x + 2x` is
//! `3x`, `x * x^2` is `x^3`), identities are removed, a single argument is unwrapped and the arguments are sorted
//! by the canonical order. `pow` combines only where the result holds for all complex values (B.9):
//! `(x^a)^n = x^(a n)` for an integer n, or for a real number a in (-1, 1]; `(x y)^n = x^n y^n` for an integer n;
//! `(c x)^a = c^a x^a` for a positive real number c. Exact numeric powers stay exact (radicals in canonical form,
//! `crate::radical`).
use crate::num::{Int, Rat, StopScope};
use crate::number::{Cmp3, Num, NumError, Real};
use crate::order::Order;
use crate::radical;
use crate::term::{int_term, num_term, rat_term, real_of, shape, Names, Shape};
use mc_api::{Store, T};
use rustc_hash::FxHashMap;
use std::cell::Cell;
use std::cmp::Ordering;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// Nesting depth of canonicaliser calls (an exponent sum or product inside a product) beyond which the result is
/// built without combining (B.10). Bounds the recursion on pathological exponent towers.
const MAX_DEPTH: u32 = 64;

/// Default limit on the size of an exact numeric power, in bits (2^28 bits is about 80 million digits).
pub const DEFAULT_MAX_BITS: u64 = 1 << 28;

/// The canonical arithmetic of one store, for one thread (it caches symbol names and tracks its own nesting).
pub struct Arith<'h, S: Store + ?Sized> {
    st: &'h S,
    n: Names<'h>,
    order: Order<'h, S>,
    stop: Option<Arc<AtomicBool>>,
    max_bits: u64,
    depth: Cell<u32>,
}

/// Collected factors of a product under construction.
struct Product<'h> {
    /// numeric factors, folded at the end
    coef: Vec<Num>,
    /// exact positive rational bases with exact rational exponents, and the exponents of (-1)
    rad_pos: Vec<(Rat, Rat)>,
    rad_units: Vec<Rat>,
    /// base -> exponents, in first-seen order
    groups: Vec<(T<'h>, Vec<T<'h>>)>,
    index: FxHashMap<T<'h>, usize>,
    /// (term, exponent) items still to be taken apart
    work: Vec<(T<'h>, T<'h>)>,
}

impl<'h> Product<'h> {
    fn group(&mut self, base: T<'h>, e: T<'h>) {
        match self.index.get(&base) {
            Some(&i) => self.groups[i].1.push(e),
            None => {
                self.index.insert(base, self.groups.len());
                self.groups.push((base, vec![e]));
            }
        }
    }
}

/// Restores the nesting depth when a canonicaliser call returns.
struct Depth<'a>(&'a Cell<u32>);

impl Drop for Depth<'_> {
    fn drop(&mut self) {
        self.0.set(self.0.get() - 1);
    }
}

impl<'h, S: Store + ?Sized> Arith<'h, S> {
    /// The canonicaliser for a store, with no stop flag and the default size limit.
    pub fn new(st: &'h S) -> Arith<'h, S> {
        Arith { st, n: Names::new(st), order: Order::new(st), stop: None, max_bits: DEFAULT_MAX_BITS, depth: Cell::new(0) }
    }
    /// Polls `flag` in long numeric loops; a raised flag gives `failure(interrupted, partial)`.
    pub fn with_stop(mut self, flag: Arc<AtomicBool>) -> Arith<'h, S> {
        self.stop = Some(flag);
        self
    }
    /// The size limit of exact powers, in bits; a larger result gives `failure(limit, memory, partial)`.
    pub fn with_max_bits(mut self, bits: u64) -> Arith<'h, S> {
        self.max_bits = bits;
        self
    }
    /// The store terms are built in.
    pub fn store(&self) -> &'h S {
        self.st
    }

    // ------------------------------------------------------------------ numbers

    /// The value of a number term (complex numbers normalised), or `None` for any other term.
    pub fn num(&self, t: T<'h>) -> Option<Num> {
        match shape(self.st, &self.n, t) {
            Shape::Number(x) => Some(x),
            _ => None,
        }
    }
    /// The canonical term of a number.
    pub fn number(&self, x: &Num) -> T<'h> {
        match x {
            Num::Complex(a, b) => num_term(self.st, &self.n, &Num::complex(a.clone(), b.clone())),
            _ => num_term(self.st, &self.n, x),
        }
    }
    /// The integer term v.
    pub fn int(&self, v: i64) -> T<'h> {
        self.st.int(v)
    }
    /// The canonical term of an integer.
    pub fn integer(&self, x: &Int) -> T<'h> {
        int_term(self.st, x)
    }
    /// The integer of a decimal digit string (an optional leading `-`), or `None` if it is not one.
    pub fn int_from_decimal(&self, s: &str) -> Option<T<'h>> {
        let (neg, digits) = match s.strip_prefix('-') {
            Some(d) => (true, d),
            None => (false, s),
        };
        Some(int_term(self.st, &Int::new(neg, crate::num::Nat::from_decimal(digits)?)))
    }
    /// num / den reduced; `failure(division_by_zero)` for den = 0.
    pub fn rational(&self, num: T<'h>, den: T<'h>) -> T<'h> {
        self.div(num, den)
    }
    /// A ball at `prec` bits around an exact real or complex number, or `None` for another term.
    pub fn ball(&self, t: T<'h>, prec: u64) -> Option<T<'h>> {
        let x = self.num(t).filter(Num::is_exact)?;
        Some(self.number(&x.to_kind(crate::number::Kind::Ball, prec.max(crate::number::MIN_PREC))))
    }
    /// The ball `ball(mid, rad)` at `prec` bits: the midpoint and radius are exact reals (the radius is rounded up,
    /// and the midpoint's rounding error is added to it). `None` unless both are exact reals and rad >= 0.
    pub fn ball_with_radius(&self, mid: T<'h>, rad: T<'h>, prec: u64) -> Option<T<'h>> {
        let p = prec.max(crate::number::MIN_PREC);
        let (Some(Real::Exact(m)), Some(Real::Exact(r))) = (real_of(self.st, mid), real_of(self.st, rad)) else {
            return None;
        };
        if r.num.neg {
            return None;
        }
        let Real::Ball(mb, _) = Real::Exact(m).to_kind(crate::number::Kind::Ball, p) else { unreachable!("a ball") };
        // an upper bound of r: r rounded to 30 bits plus its rounding error
        let Real::Ball(rb, _) = Real::Exact(r).to_kind(crate::number::Kind::Ball, 30) else { unreachable!("a ball") };
        let up = crate::num::Mag::from_float_up(&rb.mid).add(&rb.rad);
        let b = crate::num::Ball { mid: mb.mid, rad: mb.rad.add(&up) };
        Some(self.number(&Num::Real(Real::Ball(b, p))))
    }
    /// The ball of a decimal literal (`1.25`, `-2e-10`, `3.`, `7`) at `digits` decimal digits of working precision
    /// (04-syntax section 2, `1.25~40`): the exact decimal value rounded to ceil(digits log2 10) bits, with the
    /// rounding error as the radius. `None` for a malformed text, or a decimal exponent whose exact value would pass
    /// the size limit.
    pub fn ball_from_decimal(&self, text: &str, digits: u64) -> Option<T<'h>> {
        let q = decimal_value(text, self.max_bits)?;
        let p = crate::text::digits_to_bits(digits).max(crate::number::MIN_PREC);
        Some(self.number(&Num::Real(Real::Exact(q).to_kind(crate::number::Kind::Ball, p))))
    }

    /// `complex(re, im)`: a number when both parts are real numbers (with contagion between the parts and an exact
    /// zero imaginary part dropped), otherwise the term as given.
    pub fn complex(&self, re: T<'h>, im: T<'h>) -> T<'h> {
        match (real_of(self.st, re), real_of(self.st, im)) {
            (Some(a), Some(b)) => self.number(&Num::complex(a, b)),
            _ => self.st.app(self.n.complex, &[re, im]),
        }
    }

    /// The decimal text of a real number term ([`crate::text::real`]); `None` for other terms (the printer lays
    /// out complex numbers from their parts).
    pub fn text(&self, t: T<'h>) -> Option<String> {
        real_of(self.st, t).map(|x| crate::text::real(&x))
    }

    // ------------------------------------------------------------------ order and comparison

    /// The canonical order (semantics 8).
    pub fn cmp(&self, a: T<'h>, b: T<'h>) -> Ordering {
        self.order.cmp(a, b)
    }
    /// Three-valued comparison of two real numbers (semantics 2.5); `None` unless both are real numbers.
    pub fn compare(&self, a: T<'h>, b: T<'h>) -> Option<Cmp3> {
        Some(crate::number::compare(&real_of(self.st, a)?, &real_of(self.st, b)?))
    }
    /// Mathematical equality of two numbers: `Some(Some(b))` when decided, `Some(None)` when undecided (overlapping
    /// balls), `None` unless both are numbers.
    pub fn equal(&self, a: T<'h>, b: T<'h>) -> Option<Option<bool>> {
        Some(crate::number::equal(&self.num(a)?, &self.num(b)?))
    }

    // ------------------------------------------------------------------ the canonicaliser

    /// `add(args...)`.
    pub fn add(&self, args: &[T<'h>]) -> T<'h> {
        if let Some(t) = self.small_fold(args, i64::checked_add, 0) {
            return t;
        }
        self.entry(|| self.add_core(args), || self.st.app(self.n.add, args))
    }
    /// `mul(args...)`.
    pub fn mul(&self, args: &[T<'h>]) -> T<'h> {
        if let Some(t) = self.small_fold(args, i64::checked_mul, 1) {
            return t;
        }
        self.entry(|| self.mul_core(args.iter().map(|&a| (a, self.n.one)).collect()), || self.st.app(self.n.mul, args))
    }
    /// `pow(base, exp)`.
    pub fn pow(&self, base: T<'h>, exp: T<'h>) -> T<'h> {
        self.entry(|| self.mul_core(vec![(base, exp)]), || self.st.app(self.n.pow, &[base, exp]))
    }
    /// `a - b` is `add(a, mul(-1, b))`.
    pub fn sub(&self, a: T<'h>, b: T<'h>) -> T<'h> {
        let nb = self.neg(b);
        self.add(&[a, nb])
    }
    /// `a / b` is `mul(a, pow(b, -1))`.
    pub fn div(&self, a: T<'h>, b: T<'h>) -> T<'h> {
        let r = self.pow(b, self.n.minus_one);
        self.mul(&[a, r])
    }
    /// `-a` is `mul(-1, a)`.
    pub fn neg(&self, a: T<'h>) -> T<'h> {
        self.mul(&[self.n.minus_one, a])
    }

    /// The fast path of a sum or product of machine-word integers that does not overflow.
    fn small_fold(&self, args: &[T<'h>], op: fn(i64, i64) -> Option<i64>, unit: i64) -> Option<T<'h>> {
        let mut acc = unit;
        for &t in args {
            match self.st.view(t) {
                mc_api::View::Int(v) => acc = op(acc, v)?,
                _ => return None,
            }
        }
        Some(self.st.int(acc))
    }

    /// Runs one canonicaliser call: installs the stop flag at the outermost call, bounds the nesting, and turns a
    /// raised stop flag into `failure(interrupted, partial)` with the uncanonicalised term.
    fn entry(&self, build: impl FnOnce() -> T<'h>, raw: impl FnOnce() -> T<'h>) -> T<'h> {
        let d = self.depth.get();
        if d >= MAX_DEPTH {
            return raw();
        }
        self.depth.set(d + 1);
        let _depth = Depth(&self.depth);
        let _scope = (d == 0).then(|| StopScope::enter(self.stop.as_ref()));
        let r = build();
        if d == 0 && crate::num::stopped() {
            let partial = raw();
            return self.st.app(self.n.failure, &[self.n.interrupted, partial]);
        }
        r
    }

    fn shape(&self, t: T<'h>) -> Shape<'h> {
        shape(self.st, &self.n, t)
    }

    fn is_number(&self, t: T<'h>) -> bool {
        matches!(self.shape(t), Shape::Number(_))
    }

    fn failure(&self, e: NumError, partial: impl FnOnce() -> T<'h>) -> T<'h> {
        match e {
            NumError::DivisionByZero => self.st.app(self.n.failure, &[self.n.division_by_zero]),
            NumError::TooLarge => self.st.app(self.n.failure, &[self.n.limit, self.n.memory, partial()]),
            NumError::Unevaluated => partial(),
        }
    }

    fn sort(&self, v: &mut [T<'h>]) {
        v.sort_by(|&a, &b| self.order.cmp(a, b));
    }

    fn add_core(&self, args: &[T<'h>]) -> T<'h> {
        let mut nums: Vec<Num> = vec![];
        let mut terms: Vec<(T<'h>, Vec<Num>)> = vec![];
        let mut index: FxHashMap<T<'h>, usize> = FxHashMap::default();
        let mut stack: Vec<T<'h>> = args.iter().rev().copied().collect();
        // bounded by the size of the arguments
        while let Some(t) = stack.pop() {
            if crate::num::stopped() {
                break;
            }
            let (coef, rest) = match self.shape(t) {
                Shape::Failure => return t,
                Shape::Number(x) => {
                    nums.push(x);
                    continue;
                }
                Shape::Add(xs) => {
                    stack.extend(xs.iter().rev());
                    continue;
                }
                Shape::Mul(xs) if xs.len() >= 2 && self.is_number(xs[0]) => {
                    let c = self.num(xs[0]).expect("a number");
                    // the other factors of a canonical product form a canonical product
                    let rest = if xs.len() == 2 { xs[1] } else { self.st.app(self.n.mul, &xs[1..]) };
                    (c, rest)
                }
                _ => (Num::int(1), t),
            };
            match index.get(&rest) {
                Some(&i) => terms[i].1.push(coef),
                None => {
                    index.insert(rest, terms.len());
                    terms.push((rest, vec![coef]));
                }
            }
        }
        let mut out = Vec::with_capacity(terms.len() + 1);
        for (rest, coefs) in terms {
            let c = Num::sum(coefs);
            if c.is_exact_zero() {
                continue;
            }
            if c.is_zero() {
                // an inexact zero coefficient makes the term that zero (B.6), which joins the numbers
                nums.push(c);
                continue;
            }
            if c.is_exact_one() {
                out.push(rest);
                continue;
            }
            let ct = self.number(&c);
            out.push(match self.shape(rest) {
                Shape::Mul(xs) => {
                    let mut f = Vec::with_capacity(xs.len() + 1);
                    f.push(ct);
                    f.extend_from_slice(xs);
                    self.st.app(self.n.mul, &f)
                }
                _ => self.st.app(self.n.mul, &[ct, rest]),
            });
        }
        self.sort(&mut out);
        let total = Num::sum(nums);
        if !total.is_exact_zero() || out.is_empty() {
            out.insert(0, self.number(&total));
        }
        if out.len() == 1 {
            return out[0];
        }
        self.st.app(self.n.add, &out)
    }

    /// An exact integer exponent?
    fn int_exp(&self, k: T<'h>) -> Option<Int> {
        self.num(k).and_then(|x| x.as_int().cloned())
    }

    /// Absorbs the numeric power x^k into the product: a number, radical parts, or `Ok(false)` when it stays a term.
    fn absorb_numeric(&self, p: &mut Product<'h>, x: &Num, k: &Num) -> Result<bool, NumError> {
        if let (Some(b), Some(e)) = (x.as_rat(), k.as_rat()) {
            if !e.is_int() {
                if b.is_zero() {
                    // 0^e is 0 for e > 0 and a division by zero for e < 0
                    if e.num.neg {
                        return Err(NumError::DivisionByZero);
                    }
                    p.coef.push(Num::int(0));
                } else {
                    radical::split_base(b, e, &mut p.rad_pos, &mut p.rad_units);
                }
                return Ok(true);
            }
        }
        match x.pow(k, self.max_bits) {
            Ok(v) => {
                p.coef.push(v);
                Ok(true)
            }
            Err(NumError::Unevaluated) => Ok(false),
            Err(e) => Err(e),
        }
    }

    /// Takes one (term, exponent) item apart into the product.
    fn mul_item(&self, p: &mut Product<'h>, t: T<'h>, k: T<'h>) -> Result<(), NumError> {
        let one = k == self.n.one;
        match self.shape(t) {
            Shape::Failure => unreachable!("failures are returned before an item is taken apart"),
            Shape::Number(x) => {
                if one {
                    p.coef.push(x);
                } else if let Some(kn) = self.num(k) {
                    if !self.absorb_numeric(p, &x, &kn)? {
                        p.group(t, k);
                    }
                } else if !x.is_exact_one() {
                    p.group(t, k);
                }
            }
            Shape::Mul(xs) => {
                if one || self.int_exp(k).is_some() {
                    p.work.extend(xs.iter().rev().map(|&x| (x, k)));
                } else if xs.len() >= 2 && self.num(xs[0]).and_then(|c| c.as_real().map(Real::is_positive)) == Some(true) {
                    // (c x)^k = c^k x^k for a positive real number c
                    let rest = if xs.len() == 2 { xs[1] } else { self.st.app(self.n.mul, &xs[1..]) };
                    p.work.push((rest, k));
                    p.work.push((xs[0], k));
                } else {
                    p.group(t, k);
                }
            }
            Shape::Pow(b, a) => {
                if one {
                    p.work.push((b, a));
                } else if self.int_exp(k).is_some() || self.in_unit_range(a) {
                    let e = self.mul(&[a, k]);
                    p.work.push((b, e));
                } else {
                    p.group(t, k);
                }
            }
            _ => p.group(t, k),
        }
        Ok(())
    }

    /// Is the term a real number in (-1, 1]?
    fn in_unit_range(&self, a: T<'h>) -> bool {
        let Some(x) = real_of(self.st, a) else { return false };
        let lo = crate::number::compare(&Real::int(-1), &x) == Cmp3::Less;
        let hi = matches!(crate::number::compare(&x, &Real::int(1)), Cmp3::Less | Cmp3::Equal);
        lo && hi
    }

    fn mul_core(&self, items: Vec<(T<'h>, T<'h>)>) -> T<'h> {
        let raw = |items: &[(T<'h>, T<'h>)]| {
            let f: Vec<T<'h>> = items.iter().map(|&(t, k)| if k == self.n.one { t } else { self.st.app(self.n.pow, &[t, k]) }).collect();
            if f.len() == 1 {
                f[0]
            } else {
                self.st.app(self.n.mul, &f)
            }
        };
        let mut work = items.clone();
        work.reverse();
        let mut p = Product { coef: vec![], rad_pos: vec![], rad_units: vec![], groups: vec![], index: FxHashMap::default(), work };
        // every round either ends or takes a group's base apart into strict subterms, so the rounds are bounded
        loop {
            while let Some((t, k)) = p.work.pop() {
                if crate::num::stopped() {
                    return raw(&items);
                }
                if matches!(self.shape(k), Shape::Failure) {
                    return k;
                }
                if matches!(self.shape(t), Shape::Failure) {
                    return t;
                }
                if let Err(e) = self.mul_item(&mut p, t, k) {
                    return self.failure(e, || raw(&items));
                }
            }
            let mut again = false;
            let groups = std::mem::take(&mut p.groups);
            p.index.clear();
            for (base, exps) in groups {
                let e = if exps.len() == 1 { exps[0] } else { self.add(&exps) };
                if matches!(self.shape(e), Shape::Failure) {
                    return e;
                }
                if e == self.n.zero {
                    continue;
                }
                let reducible = match self.shape(base) {
                    Shape::Number(x) => match self.num(e) {
                        Some(en) => match self.absorb_numeric(&mut p, &x, &en) {
                            Ok(true) => continue,
                            Ok(false) => false,
                            Err(err) => return self.failure(err, || raw(&items)),
                        },
                        None => false,
                    },
                    Shape::Mul(_) => self.int_exp(e).is_some(),
                    Shape::Pow(_, a) => self.int_exp(e).is_some() || self.in_unit_range(a),
                    _ => false,
                };
                if reducible {
                    p.work.push((base, e));
                    again = true;
                } else {
                    p.group(base, e);
                }
            }
            if !again {
                break;
            }
        }
        self.finish_product(p, || raw(&items))
    }

    fn finish_product(&self, p: Product<'h>, raw: impl FnOnce() -> T<'h>) -> T<'h> {
        let mut c = Num::product(p.coef);
        if c.is_zero() {
            return self.number(&c);
        }
        let mut factors: Vec<T<'h>> = Vec::with_capacity(p.groups.len() + p.rad_pos.len() + 1);
        if !p.rad_pos.is_empty() || !p.rad_units.is_empty() {
            let r = match radical::normalize(&p.rad_pos, &p.rad_units, self.max_bits) {
                Ok(r) => r,
                Err(e) => return self.failure(e, raw),
            };
            c = Num::product(vec![c, r.coef]);
            for (m, f) in &r.rads {
                factors.push(self.st.app(self.n.pow, &[int_term(self.st, &Int::from_nat(m.clone())), rat_term(self.st, f)]));
            }
            if let Some(u) = &r.unit {
                factors.push(self.st.app(self.n.pow, &[self.n.minus_one, rat_term(self.st, u)]));
            }
        }
        for (base, exps) in p.groups {
            debug_assert_eq!(exps.len(), 1);
            let e = exps[0];
            factors.push(if e == self.n.one { base } else { self.st.app(self.n.pow, &[base, e]) });
        }
        self.sort(&mut factors);
        if !c.is_exact_one() || factors.is_empty() {
            factors.insert(0, self.number(&c));
        }
        if factors.len() == 1 {
            return factors[0];
        }
        self.st.app(self.n.mul, &factors)
    }
}

/// The exact value of a decimal text `[-]digits[.digits][e[+-]digits]` (at least one digit before the exponent).
/// `None` if malformed, or if 10^|exponent| would need more than `max_bits` bits.
fn decimal_value(text: &str, max_bits: u64) -> Option<Rat> {
    let (neg, rest) = match text.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, text),
    };
    let (mantissa, exp) = match rest.find(['e', 'E']) {
        Some(i) => {
            let e = &rest[i + 1..];
            let e = e.strip_prefix('+').unwrap_or(e);
            (&rest[..i], e.parse::<i64>().ok()?)
        }
        None => (rest, 0),
    };
    let (int, frac) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    let digits = format!("{int}{frac}");
    let num = crate::num::Nat::from_decimal(&digits)?;
    let e10 = exp.checked_sub(frac.len() as i64)?;
    // 10^k has about 3.33 k bits
    if e10.unsigned_abs().saturating_mul(10) / 3 > max_bits {
        return None;
    }
    let scale = crate::num::Nat::from_u64(10).pow_u64(e10.unsigned_abs());
    let num = Int::new(neg, num);
    Some(if e10 >= 0 { Rat::from_int(num.mul(&Int::from_nat(scale))) } else { Rat::new(num, Int::from_nat(scale)) })
}

#[cfg(test)]
mod tests {
    use super::decimal_value;
    use crate::num::{Int, Rat};

    #[test]
    fn decimal_literals() {
        let q = |n, d| Rat::new(Int::from_i64(n), Int::from_i64(d));
        assert_eq!(decimal_value("1.25", 1 << 20), Some(q(5, 4)));
        assert_eq!(decimal_value("-2e-3", 1 << 20), Some(q(-1, 500)));
        assert_eq!(decimal_value("3.", 1 << 20), Some(q(3, 1)));
        assert_eq!(decimal_value("1.5E+2", 1 << 20), Some(q(150, 1)));
        assert_eq!(decimal_value("7", 1 << 20), Some(q(7, 1)));
        assert_eq!(decimal_value("1e100000", 1000), None);
        assert_eq!(decimal_value("1.2.3", 1 << 20), None);
        assert_eq!(decimal_value("e5", 1 << 20), None);
    }
}
