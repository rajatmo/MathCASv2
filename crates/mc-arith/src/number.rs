//! Numbers as values (semantics 2): the three kinds of reals, complex numbers over them, contagion, the value
//! order used by the canonical order, three-valued comparison and numeric powers.
//!
//! A [`Num`] is what a number term means; `crate::term` converts between the two. Arithmetic on mixed kinds gives
//! the least exact kind present, `exact < ball < machine` (2.2). A ball carries its working precision: the bit
//! width of its midpoint mantissa (decision B.2), and an operation on balls works at the largest precision among
//! its ball operands (B.3).
use crate::num::float::{dyadic_to_f64, f64_to_dyadic, ratio_to_f64};
use crate::num::{Ball, Float, Int, Mag, Nat, Rat};
use std::cmp::Ordering;

/// The kind of a real number, in contagion order (semantics 2.2).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Kind {
    /// Integers and rationals.
    Exact,
    /// Balls.
    Ball,
    /// Machine reals.
    Machine,
}

/// The smallest working precision of a ball, in bits.
pub const MIN_PREC: u64 = 2;

/// A real number of one kind.
#[derive(Clone, Debug)]
pub enum Real {
    /// An integer or a rational.
    Exact(Rat),
    /// A ball and its working precision in bits (the midpoint is rounded to it).
    Ball(Ball, u64),
    /// A machine real.
    Machine(f64),
}

/// A number: a real, or a complex number whose parts have the same kind. An exact complex number never has a zero
/// imaginary part (semantics 2.4); an inexact one may.
#[derive(Clone, Debug)]
pub enum Num {
    /// A real number.
    Real(Real),
    /// A complex number: real and imaginary parts of one kind.
    Complex(Real, Real),
}

/// Why a numeric operation has no numeric result.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NumError {
    /// Exact division by zero (semantics 2.4: `failure(division_by_zero)`).
    DivisionByZero,
    /// An exact result beyond the size limit.
    TooLarge,
    /// No numeric value is computed here: a ball divisor that may contain zero (decision 3.5), or a power with no
    /// ball or exact algorithm. The term stays unevaluated.
    Unevaluated,
}

/// The outcome of a three-valued comparison of two real numbers (semantics 2.5).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Cmp3 {
    /// Decided: less.
    Less,
    /// Decided: equal.
    Equal,
    /// Decided: greater.
    Greater,
    /// A machine NaN is involved: unordered, and not equal.
    Unordered,
    /// Overlapping balls: the comparison stays unevaluated.
    Undecided,
}

pub(crate) fn rat_int(v: i64) -> Rat {
    Rat::from_i64(v)
}

/// A ball around an exact value at precision p: exact when the value fits p bits.
fn rat_to_ball(q: &Rat, p: u64) -> Ball {
    if q.is_int() {
        let (mid, rad) = Float::round(q.num.neg, q.num.mag.clone(), 0, p);
        return Ball { mid, rad };
    }
    let num = Ball::exact(Float::new(q.num.neg, q.num.mag.clone(), 0));
    num.div(&Ball::from_nat(q.den.clone()), p).expect("a positive denominator excludes zero")
}

fn ball_to_f64(b: &Ball) -> f64 {
    dyadic_to_f64(b.mid.neg, &b.mid.man, b.mid.exp)
}

impl Real {
    /// An exact integer.
    pub fn int(v: i64) -> Real {
        Real::Exact(rat_int(v))
    }
    /// The kind of the number.
    pub fn kind(&self) -> Kind {
        match self {
            Real::Exact(_) => Kind::Exact,
            Real::Ball(..) => Kind::Ball,
            Real::Machine(_) => Kind::Machine,
        }
    }
    /// The working precision of a ball.
    pub fn prec(&self) -> Option<u64> {
        match self {
            Real::Ball(_, p) => Some(*p),
            _ => None,
        }
    }
    /// The same number in a kind at least as inexact (balls at precision p).
    pub fn to_kind(&self, k: Kind, p: u64) -> Real {
        match (self, k) {
            (Real::Exact(q), Kind::Ball) => Real::Ball(rat_to_ball(q, p), p),
            (Real::Exact(q), Kind::Machine) => Real::Machine(ratio_to_f64(q.num.neg, &q.num.mag, &q.den)),
            (Real::Ball(b, _), Kind::Machine) => Real::Machine(ball_to_f64(b)),
            _ => self.clone(),
        }
    }
    /// The nearest machine real.
    pub fn to_f64(&self) -> f64 {
        match self.to_kind(Kind::Machine, MIN_PREC) {
            Real::Machine(v) => v,
            _ => unreachable!("conversion to machine"),
        }
    }
    /// Exact zero, a machine zero of either sign, or a ball with zero midpoint and radius.
    pub fn is_zero(&self) -> bool {
        match self {
            Real::Exact(q) => q.is_zero(),
            Real::Ball(b, _) => b.mid.is_zero() && b.rad.is_zero(),
            Real::Machine(v) => *v == 0.0,
        }
    }
    /// Is it exact zero?
    pub fn is_exact_zero(&self) -> bool {
        matches!(self, Real::Exact(q) if q.is_zero())
    }
    /// Is it exact one?
    pub fn is_exact_one(&self) -> bool {
        matches!(self, Real::Exact(q) if q.is_int() && !q.num.neg && q.num.mag.limbs == [1])
    }
    /// Provably positive (a ball entirely above zero).
    pub fn is_positive(&self) -> bool {
        match self {
            Real::Exact(q) => !q.num.neg && !q.is_zero(),
            Real::Ball(b, _) => !b.mid.neg && !b.contains_zero(),
            Real::Machine(v) => *v > 0.0,
        }
    }
    /// Provably negative.
    pub fn is_negative(&self) -> bool {
        match self {
            Real::Exact(q) => q.num.neg,
            Real::Ball(b, _) => b.mid.neg && !b.contains_zero(),
            Real::Machine(v) => *v < 0.0,
        }
    }
    /// -self, exactly.
    pub fn neg(&self) -> Real {
        match self {
            Real::Exact(q) => Real::Exact(q.neg()),
            Real::Ball(b, p) => Real::Ball(b.neg(), *p),
            Real::Machine(v) => Real::Machine(-v),
        }
    }

    fn common(a: &Real, b: &Real) -> (Kind, u64) {
        let k = a.kind().max(b.kind());
        let p = a.prec().unwrap_or(0).max(b.prec().unwrap_or(0)).max(MIN_PREC);
        (k, p)
    }

    /// self + o with contagion.
    pub fn add(&self, o: &Real) -> Real {
        let (k, p) = Real::common(self, o);
        match (self.to_kind(k, p), o.to_kind(k, p)) {
            (Real::Exact(a), Real::Exact(b)) => Real::Exact(a.add(&b)),
            (Real::Ball(a, _), Real::Ball(b, _)) => Real::Ball(a.add(&b, p), p),
            (Real::Machine(a), Real::Machine(b)) => Real::Machine(a + b),
            _ => unreachable!("operands converted to one kind"),
        }
    }
    /// self - o with contagion.
    pub fn sub(&self, o: &Real) -> Real {
        self.add(&o.neg())
    }
    /// self o with contagion.
    pub fn mul(&self, o: &Real) -> Real {
        let (k, p) = Real::common(self, o);
        match (self.to_kind(k, p), o.to_kind(k, p)) {
            (Real::Exact(a), Real::Exact(b)) => Real::Exact(a.mul(&b)),
            (Real::Ball(a, _), Real::Ball(b, _)) => Real::Ball(a.mul(&b, p), p),
            (Real::Machine(a), Real::Machine(b)) => Real::Machine(a * b),
            _ => unreachable!("operands converted to one kind"),
        }
    }
    /// self / o.
    pub fn div(&self, o: &Real) -> Result<Real, NumError> {
        let (k, p) = Real::common(self, o);
        match (self.to_kind(k, p), o.to_kind(k, p)) {
            (Real::Exact(a), Real::Exact(b)) => Ok(Real::Exact(a.mul(&b.recip().ok_or(NumError::DivisionByZero)?))),
            (Real::Ball(a, _), Real::Ball(b, _)) => a.div(&b, p).map(|q| Real::Ball(q, p)).ok_or(NumError::Unevaluated),
            (Real::Machine(a), Real::Machine(b)) => Ok(Real::Machine(a / b)),
            _ => unreachable!("operands converted to one kind"),
        }
    }

    /// The exact value for ordering: a machine real's own binary value, a ball's midpoint.
    fn value(&self) -> Val {
        match self {
            Real::Exact(q) => Val::signed(q.num.neg, q.num.mag.clone(), q.den.clone(), 0),
            Real::Ball(b, _) => Val::signed(b.mid.neg, b.mid.man.clone(), Nat::from_u64(1), b.mid.exp),
            Real::Machine(v) => Val::of_f64(*v),
        }
    }
    /// The lower and upper ends of the set the number stands for (a point unless it is a ball).
    fn ends(&self) -> (Val, Val) {
        match self {
            Real::Ball(b, _) if !b.rad.is_zero() => {
                let r = b.rad.to_float();
                (Val::of_float(&b.mid.sub_exact(&r)), Val::of_float(&b.mid.add_exact(&r)))
            }
            _ => (self.value(), self.value()),
        }
    }
}

/// An exact real value for ordering, with the infinities and NaN of machine reals. `Fin` is num / den 2^sh with
/// num, den > 0.
#[derive(Clone, Debug)]
enum Val {
    NegInf,
    Neg(Nat, Nat, i64),
    Zero,
    Pos(Nat, Nat, i64),
    PosInf,
    NaN,
}

impl Val {
    fn signed(neg: bool, num: Nat, den: Nat, sh: i64) -> Val {
        if num.is_zero() {
            Val::Zero
        } else if neg {
            Val::Neg(num, den, sh)
        } else {
            Val::Pos(num, den, sh)
        }
    }
    fn of_f64(v: f64) -> Val {
        if v.is_nan() {
            return Val::NaN;
        }
        if v.is_infinite() {
            return if v > 0.0 { Val::PosInf } else { Val::NegInf };
        }
        let (neg, man, exp) = f64_to_dyadic(v).expect("finite");
        Val::signed(neg, man, Nat::from_u64(1), exp)
    }
    fn of_float(f: &Float) -> Val {
        Val::signed(f.neg, f.man.clone(), Nat::from_u64(1), f.exp)
    }
    fn rank(&self) -> u8 {
        match self {
            Val::NegInf => 0,
            Val::Neg(..) => 1,
            Val::Zero => 2,
            Val::Pos(..) => 3,
            Val::PosInf => 4,
            Val::NaN => 5,
        }
    }
    /// Total order: the reals in value order, then NaN (all NaNs equal here).
    fn cmp(&self, o: &Val) -> Ordering {
        match (self, o) {
            (Val::Pos(a, b, s), Val::Pos(c, d, t)) => cmp_mag(a, b, *s, c, d, *t),
            (Val::Neg(a, b, s), Val::Neg(c, d, t)) => cmp_mag(c, d, *t, a, b, *s),
            _ => self.rank().cmp(&o.rank()),
        }
    }
}

/// Compares a/b 2^s with c/d 2^t (all positive). The binary exponents bound the values within a factor of two, so
/// the exact cross-multiplication only runs when they are close, and its shift is then bounded by the operand
/// sizes (a ball with exponent -10^6 is compared with a fraction of about 10^6 bits, never with a 10^6-bit shift of
/// a small number).
fn cmp_mag(a: &Nat, b: &Nat, s: i64, c: &Nat, d: &Nat, t: i64) -> Ordering {
    // a/b in (2^(la - 1), 2^(la + 1)) with la = bits(a) - bits(b)
    let la = a.bits() as i64 - b.bits() as i64 + s;
    let lc = c.bits() as i64 - d.bits() as i64 + t;
    if la + 2 <= lc {
        return Ordering::Less;
    }
    if lc + 2 <= la {
        return Ordering::Greater;
    }
    let (mut x, mut y) = (a.mul(d), c.mul(b));
    if s >= t {
        x = x.shl((s - t) as u64);
    } else {
        y = y.shl((t - s) as u64);
    }
    x.cmp(&y)
}

/// The order of two balls' radii, then precisions, for ties at equal midpoints.
fn cmp_ball_repr(a: &Ball, pa: u64, b: &Ball, pb: u64) -> Ordering {
    let key = |m: &Mag| (!m.is_zero(), if m.is_zero() { 0 } else { m.e }, m.m);
    key(&a.rad).cmp(&key(&b.rad)).then(pa.cmp(&pb))
}

/// Ties between reals of equal value: by kind (exact before inexact), then by representation (machine reals in
/// IEEE total order, which separates -0.0 from 0.0 and NaNs by payload; balls by radius, then precision).
fn cmp_real_repr(a: &Real, b: &Real) -> Ordering {
    a.kind().cmp(&b.kind()).then_with(|| match (a, b) {
        (Real::Machine(x), Real::Machine(y)) => x.total_cmp(y),
        (Real::Ball(x, px), Real::Ball(y, py)) => cmp_ball_repr(x, *px, y, *py),
        _ => Ordering::Equal,
    })
}

impl Num {
    /// An exact integer.
    pub fn int(v: i64) -> Num {
        Num::Real(Real::int(v))
    }
    /// An exact rational.
    pub fn rat(q: Rat) -> Num {
        Num::Real(Real::Exact(q))
    }
    /// The exact imaginary unit.
    pub fn i() -> Num {
        Num::Complex(Real::int(0), Real::int(1))
    }
    /// A complex number from parts, with contagion between the parts (decision B.4) and an exact zero imaginary
    /// part dropped (semantics 2.4).
    pub fn complex(re: Real, im: Real) -> Num {
        let (k, p) = Real::common(&re, &im);
        let (re, im) = (re.to_kind(k, p), im.to_kind(k, p));
        if im.is_exact_zero() {
            Num::Real(re)
        } else {
            Num::Complex(re, im)
        }
    }
    /// The least exact kind of the parts.
    pub fn kind(&self) -> Kind {
        match self {
            Num::Real(r) => r.kind(),
            Num::Complex(a, b) => a.kind().max(b.kind()),
        }
    }
    fn prec(&self) -> u64 {
        match self {
            Num::Real(r) => r.prec().unwrap_or(0),
            Num::Complex(a, b) => a.prec().unwrap_or(0).max(b.prec().unwrap_or(0)),
        }
    }
    /// The same number in a kind at least as inexact (balls at precision p).
    pub fn to_kind(&self, k: Kind, p: u64) -> Num {
        match self {
            Num::Real(r) => Num::Real(r.to_kind(k, p)),
            Num::Complex(a, b) => Num::Complex(a.to_kind(k, p), b.to_kind(k, p)),
        }
    }
    /// Are all parts exact?
    pub fn is_exact(&self) -> bool {
        self.kind() == Kind::Exact
    }
    /// A zero of any kind (complex zeros included): absorbing in products (decision B.6).
    pub fn is_zero(&self) -> bool {
        match self {
            Num::Real(r) => r.is_zero(),
            Num::Complex(a, b) => a.is_zero() && b.is_zero(),
        }
    }
    /// Is it exact zero?
    pub fn is_exact_zero(&self) -> bool {
        matches!(self, Num::Real(r) if r.is_exact_zero())
    }
    /// Is it exact one?
    pub fn is_exact_one(&self) -> bool {
        matches!(self, Num::Real(r) if r.is_exact_one())
    }
    /// The real number, unless it is complex.
    pub fn as_real(&self) -> Option<&Real> {
        match self {
            Num::Real(r) => Some(r),
            Num::Complex(..) => None,
        }
    }
    /// The exact rational value, if the number is one.
    pub fn as_rat(&self) -> Option<&Rat> {
        match self {
            Num::Real(Real::Exact(q)) => Some(q),
            _ => None,
        }
    }
    /// The exact integer value, if the number is one.
    pub fn as_int(&self) -> Option<&Int> {
        self.as_rat().filter(|q| q.is_int()).map(|q| &q.num)
    }
    fn parts(&self) -> (Real, Real) {
        match self {
            Num::Real(r) => (r.clone(), Real::int(0)),
            Num::Complex(a, b) => (a.clone(), b.clone()),
        }
    }

    /// -self, exactly.
    pub fn neg(&self) -> Num {
        match self {
            Num::Real(r) => Num::Real(r.neg()),
            Num::Complex(a, b) => Num::Complex(a.neg(), b.neg()),
        }
    }
    /// self + o with contagion.
    pub fn add(&self, o: &Num) -> Num {
        match (self, o) {
            (Num::Real(a), Num::Real(b)) => Num::Real(a.add(b)),
            (Num::Real(a), Num::Complex(c, d)) | (Num::Complex(c, d), Num::Real(a)) => Num::complex(a.add(c), d.clone()),
            (Num::Complex(a, b), Num::Complex(c, d)) => Num::complex(a.add(c), b.add(d)),
        }
    }
    /// self o with contagion.
    pub fn mul(&self, o: &Num) -> Num {
        match (self, o) {
            (Num::Real(a), Num::Real(b)) => Num::Real(a.mul(b)),
            (Num::Real(a), Num::Complex(c, d)) | (Num::Complex(c, d), Num::Real(a)) => Num::complex(a.mul(c), a.mul(d)),
            (Num::Complex(a, b), Num::Complex(c, d)) => {
                Num::complex(a.mul(c).sub(&b.mul(d)), a.mul(d).add(&b.mul(c)))
            }
        }
    }
    /// 1 / self: (a - bi) / (a^2 + b^2) for complex numbers.
    pub fn recip(&self) -> Result<Num, NumError> {
        match self {
            Num::Real(r) => Real::int(1).div(r).map(Num::Real),
            Num::Complex(a, b) => {
                let d = a.mul(a).add(&b.mul(b));
                Ok(Num::complex(a.div(&d)?, b.neg().div(&d)?))
            }
        }
    }

    /// self^n for an integer exponent. Exact results larger than about `max_bits` bits are refused before they are
    /// computed; exact zero to a negative power is a division by zero.
    pub fn pow_int(&self, n: &Int, max_bits: u64) -> Result<Num, NumError> {
        if n.is_zero() {
            return Ok(Num::int(1));
        }
        if self.is_exact_one() {
            return Ok(Num::int(1));
        }
        if let Some(q) = self.as_rat() {
            if q.is_zero() {
                return if n.neg { Err(NumError::DivisionByZero) } else { Ok(Num::int(0)) };
            }
            if q.is_int() && q.num.mag.limbs == [1] {
                // -1: the parity of n decides
                return Ok(Num::int(if n.mag.bit(0) { -1 } else { 1 }));
            }
        }
        let Some(e) = n.mag.to_u128().and_then(|v| u64::try_from(v).ok()) else {
            return match self.kind() {
                Kind::Machine => Ok(self.pow_machine(&Num::Real(Real::Exact(Rat::from_int(n.clone()))))),
                Kind::Exact => Err(NumError::TooLarge),
                Kind::Ball => Err(NumError::Unevaluated),
            };
        };
        let base = if n.neg { self.recip()? } else { self.clone() };
        match &base {
            Num::Real(Real::Exact(q)) => {
                let bits = q.num.mag.bits().max(q.den.bits());
                if bits.saturating_mul(e) > max_bits {
                    return Err(NumError::TooLarge);
                }
                Ok(Num::rat(q.pow_u64(e)))
            }
            Num::Real(Real::Machine(v)) => Ok(Num::Real(Real::Machine(v.powf(if n.neg { -(e as f64) } else { e as f64 })))),
            Num::Real(Real::Ball(b, p)) => {
                // the exponent grows linearly in e; keep it far from overflowing i64
                let scale = b.mid.top().unsigned_abs() + b.rad.e.unsigned_abs() + p + 64;
                if (scale as u128) * (e as u128) > 1 << 60 {
                    return Err(NumError::Unevaluated);
                }
                Ok(base.pow_binary(e))
            }
            Num::Complex(..) => {
                if base.is_exact() {
                    let (a, b) = base.parts();
                    let bits = match (a, b) {
                        (Real::Exact(x), Real::Exact(y)) => x.num.mag.bits().max(x.den.bits()).max(y.num.mag.bits()).max(y.den.bits()),
                        _ => unreachable!("exact parts"),
                    };
                    if (bits + 1).saturating_mul(e) > max_bits {
                        return Err(NumError::TooLarge);
                    }
                }
                Ok(base.pow_binary(e))
            }
        }
    }

    /// self^e by binary powering (e > 0).
    fn pow_binary(&self, e: u64) -> Num {
        let mut r = self.clone();
        let mut i = 63 - e.leading_zeros();
        while i > 0 {
            i -= 1;
            if crate::num::stopped() {
                break;
            }
            r = r.mul(&r);
            if (e >> i) & 1 == 1 {
                r = r.mul(self);
            }
        }
        r
    }

    /// The principal power self^e in machine arithmetic (both converted to machine numbers): a real result for a
    /// non-negative real base, or a real base and an integral real exponent; otherwise exp(e log self) in complex
    /// machine arithmetic.
    pub fn pow_machine(&self, e: &Num) -> Num {
        let (a, b) = self.parts();
        let (c, d) = e.parts();
        let (a, b, c, d) = (a.to_f64(), b.to_f64(), c.to_f64(), d.to_f64());
        let real_args = matches!(self, Num::Real(_)) && matches!(e, Num::Real(_));
        if real_args && (a >= 0.0 || a.is_nan() || a.is_infinite() || c.fract() == 0.0 || !c.is_finite()) {
            return Num::Real(Real::Machine(a.powf(c)));
        }
        let m = |x: f64| Real::Machine(x);
        if a == 0.0 && b == 0.0 {
            // 0^w: zero when Re(w) > 0; the logarithm formula would give NaN
            if c > 0.0 {
                return Num::Complex(m(0.0), m(0.0));
            }
            return Num::Complex(m(f64::NAN), m(f64::NAN));
        }
        let (lr, th) = (a.hypot(b).ln(), b.atan2(a));
        // w log z = (c lr - d th) + (d lr + c th) i
        let (x, y) = (c * lr - d * th, d * lr + c * th);
        let r = x.exp();
        Num::Complex(m(r * y.cos()), m(r * y.sin()))
    }

    /// The principal power self^e for numbers, except an exact real base with a non-integer exact exponent (the
    /// radicals of `crate::radical`), which the caller handles.
    pub fn pow(&self, e: &Num, max_bits: u64) -> Result<Num, NumError> {
        if self.is_exact_one() {
            return Ok(Num::int(1));
        }
        if let Some(n) = e.as_int() {
            return self.pow_int(n, max_bits);
        }
        if self.kind() == Kind::Machine || e.kind() == Kind::Machine {
            return Ok(self.pow_machine(e));
        }
        match (self, e.as_rat()) {
            // a ball to a half-integer power: a square root, then an integer power
            (Num::Real(Real::Ball(b, p)), Some(q)) if q.den.limbs == [2] => {
                let mag = Ball { mid: b.mid.abs(), rad: b.rad };
                let s = if b.mid.is_zero() && b.rad.is_zero() { mag } else { mag.sqrt(*p).ok_or(NumError::Unevaluated)? };
                let root = if b.mid.neg {
                    if b.contains_zero() {
                        return Err(NumError::Unevaluated);
                    }
                    Num::Complex(Real::Ball(Ball::from_i64(0), *p), Real::Ball(s, *p))
                } else {
                    Num::Real(Real::Ball(s, *p))
                };
                root.pow_int(&q.num, max_bits)
            }
            _ => Err(NumError::Unevaluated),
        }
    }

    /// The canonical order on numbers (semantics 8): by real part, then imaginary part (zero for reals); at equal
    /// value reals before complex numbers, then exact before inexact and the representation (decision B.7).
    pub fn cmp_canonical(&self, o: &Num) -> Ordering {
        let (a, b) = self.parts();
        let (c, d) = o.parts();
        a.value()
            .cmp(&c.value())
            .then_with(|| b.value().cmp(&d.value()))
            .then_with(|| matches!(self, Num::Complex(..)).cmp(&matches!(o, Num::Complex(..))))
            .then_with(|| cmp_real_repr(&a, &c))
            .then_with(|| cmp_real_repr(&b, &d))
    }

    /// Sum of numbers with contagion. Exact operands are summed first (exactly, so in any order); inexact ones are
    /// then added in canonical order, which makes the result independent of the order of the operands (B.5).
    pub fn sum(nums: Vec<Num>) -> Num {
        Num::fold(nums, Num::int(0), Num::add)
    }

    /// Product of numbers with contagion, in the same order as [`Num::sum`].
    pub fn product(nums: Vec<Num>) -> Num {
        Num::fold(nums, Num::int(1), Num::mul)
    }

    fn fold(nums: Vec<Num>, unit: Num, op: fn(&Num, &Num) -> Num) -> Num {
        let (exact, mut inexact): (Vec<Num>, Vec<Num>) = nums.into_iter().partition(Num::is_exact);
        let exact = exact.iter().fold(None, |acc: Option<Num>, x| Some(acc.map_or_else(|| x.clone(), |a| op(&a, x))));
        if inexact.is_empty() {
            return exact.unwrap_or(unit);
        }
        inexact.sort_by(Num::cmp_canonical);
        let k = inexact.iter().map(Num::kind).max().expect("nonempty");
        let p = inexact.iter().map(Num::prec).max().expect("nonempty").max(MIN_PREC);
        let mut it = inexact.into_iter();
        let mut acc = match exact {
            Some(x) => x.to_kind(k, p),
            None => it.next().expect("nonempty").to_kind(k, p),
        };
        for x in it {
            acc = op(&acc, &x.to_kind(k, p));
        }
        acc
    }
}

/// Three-valued comparison of real numbers (semantics 2.5): machine reals decide by IEEE comparison (both operands
/// converted to machine reals); balls decide only when disjoint, or when both are exact points; exact numbers always
/// decide.
pub fn compare(a: &Real, b: &Real) -> Cmp3 {
    let from = |o: Ordering| match o {
        Ordering::Less => Cmp3::Less,
        Ordering::Equal => Cmp3::Equal,
        Ordering::Greater => Cmp3::Greater,
    };
    if a.kind() == Kind::Machine || b.kind() == Kind::Machine {
        return a.to_f64().partial_cmp(&b.to_f64()).map_or(Cmp3::Unordered, from);
    }
    let ((alo, ahi), (blo, bhi)) = (a.ends(), b.ends());
    if ahi.cmp(&blo) == Ordering::Less {
        return Cmp3::Less;
    }
    if alo.cmp(&bhi) == Ordering::Greater {
        return Cmp3::Greater;
    }
    if alo.cmp(&ahi) == Ordering::Equal && blo.cmp(&bhi) == Ordering::Equal {
        // two points that are not apart
        return Cmp3::Equal;
    }
    Cmp3::Undecided
}

/// Mathematical equality of numbers, three-valued: `Some(true)`, `Some(false)` or `None` (undecided, overlapping
/// balls). NaN is not equal to anything.
pub fn equal(a: &Num, b: &Num) -> Option<bool> {
    let (ar, ai) = a.parts();
    let (br, bi) = b.parts();
    match (compare(&ar, &br), compare(&ai, &bi)) {
        (Cmp3::Equal, Cmp3::Equal) => Some(true),
        (Cmp3::Unordered, _) | (_, Cmp3::Unordered) => Some(false),
        (Cmp3::Less | Cmp3::Greater, _) | (_, Cmp3::Less | Cmp3::Greater) => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q(n: i64, d: i64) -> Real {
        Real::Exact(Rat::new(Int::from_i64(n), Int::from_i64(d)))
    }
    fn ball(n: i64, d: i64, p: u64) -> Real {
        Real::Exact(Rat::new(Int::from_i64(n), Int::from_i64(d))).to_kind(Kind::Ball, p)
    }

    #[test]
    fn contagion() {
        assert_eq!(q(1, 3).add(&q(2, 3)).kind(), Kind::Exact);
        assert!(q(1, 3).add(&q(2, 3)).is_exact_one());
        assert_eq!(q(1, 3).add(&ball(1, 3, 100)).kind(), Kind::Ball);
        assert_eq!(ball(1, 3, 100).add(&Real::Machine(0.5)).kind(), Kind::Machine);
        match q(1, 2).add(&Real::Machine(0.25)) {
            Real::Machine(v) => assert_eq!(v, 0.75),
            r => panic!("{r:?}"),
        }
        // exact parts first: 1/3 + 2/3 is exactly 1 before it meets the machine real
        match Num::sum(vec![Num::Real(q(1, 3)), Num::Real(Real::Machine(0.5)), Num::Real(q(2, 3))]) {
            Num::Real(Real::Machine(v)) => assert_eq!(v, 1.5),
            r => panic!("{r:?}"),
        }
    }

    #[test]
    fn comparisons_are_three_valued() {
        let third = ball(1, 3, 100);
        let x = third.mul(&Real::int(3));
        assert_eq!(compare(&x, &Real::int(1)), Cmp3::Undecided);
        assert_eq!(compare(&x, &Real::int(2)), Cmp3::Less);
        assert_eq!(compare(&q(1, 3), &q(1, 2)), Cmp3::Less);
        assert_eq!(compare(&q(1, 2), &Real::Machine(0.5)), Cmp3::Equal);
        assert_eq!(compare(&Real::Machine(f64::NAN), &Real::int(1)), Cmp3::Unordered);
        assert_eq!(compare(&ball(1, 2, 64), &q(1, 2)), Cmp3::Equal);
        assert_eq!(equal(&Num::i(), &Num::i()), Some(true));
        assert_eq!(equal(&Num::i(), &Num::int(1)), Some(false));
    }

    #[test]
    fn canonical_number_order() {
        let nums = [
            Num::Real(Real::Machine(f64::NEG_INFINITY)),
            Num::Real(q(-1, 2)),
            Num::int(0),
            Num::Real(Real::Machine(-0.0)),
            Num::Real(Real::Machine(0.0)),
            Num::Complex(Real::int(0), Real::int(1)),
            Num::Real(q(1, 2)),
            Num::Real(ball(1, 2, 64)),
            Num::Real(ball(1, 2, 128)),
            Num::Real(Real::Machine(0.5)),
            Num::Complex(q(1, 2), Real::int(1)),
            Num::Real(Real::Machine(f64::INFINITY)),
            Num::Real(Real::Machine(f64::NAN)),
        ];
        for (i, a) in nums.iter().enumerate() {
            for (j, b) in nums.iter().enumerate() {
                assert_eq!(a.cmp_canonical(b), i.cmp(&j), "{a:?} {b:?}");
            }
        }
    }

    #[test]
    fn powers() {
        let m = 1 << 20;
        let two = Num::int(2);
        let p = two.pow_int(&Int::from_i64(-3), m).unwrap();
        assert_eq!(p.as_rat(), Some(&Rat::new(Int::from_i64(1), Int::from_i64(8))));
        assert_eq!(Num::int(0).pow_int(&Int::from_i64(-1), m).unwrap_err(), NumError::DivisionByZero);
        assert_eq!(two.pow_int(&Int::from_i64(1 << 40), m).unwrap_err(), NumError::TooLarge);
        // i^2 = -1, exactly real
        assert!(matches!(Num::i().pow_int(&Int::from_i64(2), m).unwrap(), Num::Real(Real::Exact(_))));
        // (-8.0)^(1/3) is the principal complex root
        match Num::Real(Real::Machine(-8.0)).pow(&Num::Real(q(1, 3)), m).unwrap() {
            Num::Complex(Real::Machine(a), Real::Machine(b)) => {
                assert!((a - 1.0).abs() < 1e-12 && (b - 3f64.sqrt()).abs() < 1e-12)
            }
            r => panic!("{r:?}"),
        }
        // a ball to the power 1/2 is its square root; of a negative ball, an imaginary ball
        let r = Num::Real(ball(4, 1, 64)).pow(&Num::Real(q(1, 2)), m).unwrap();
        assert_ne!(equal(&r, &Num::int(2)), Some(false), "an enclosure of 2");
        let r = Num::Real(ball(-4, 1, 64)).pow(&Num::Real(q(3, 2)), m).unwrap();
        assert_ne!(equal(&r, &Num::Complex(Real::int(0), Real::int(-8))), Some(false));
        assert_eq!(r.kind(), Kind::Ball);
        assert_eq!(Num::Real(ball(2, 1, 64)).pow(&Num::Real(q(1, 3)), m).unwrap_err(), NumError::Unevaluated);
    }
}
