//! Exact values (decision SF.1, SF.2): exact arguments give exact results where a closed form exists, a pole gives
//! [`Exact::Pole`] (the native turns it into `failure(pole, call)`), anything else stays unevaluated.
//!
//! | function | closed forms |
//! |---|---|
//! | gamma | (n-1)! at positive integers; Gamma(k + 1/2) = (2k)!/(4^k k!) sqrt(pi), Gamma(1/2 - k) = (-4)^k k!/(2k)! sqrt(pi) |
//! | log_gamma | the logarithms of those (`log((n-1)!)`, `log(c) + log(pi)/2 - k pi im` for negative arguments) |
//! | digamma | H_(n-1) - euler_gamma; psi(k + 1/2) = psi(1/2 - k) = -euler_gamma - 2 log 2 + sum_(j<=k) 2/(2j-1) |
//! | polygamma | polygamma(0, x) = digamma(x); at positive integers and half-integers through zeta(n + 1) |
//! | beta | the quotient of exact gammas (0 where only a + b is a pole) |
//! | zeta | zeta(0) = -1/2, zeta(2n) = (-1)^(n+1) B_2n (2 pi)^2n / (2 (2n)!), zeta(-n) = (-1)^n B_(n+1)/(n+1) |
//! | bessel_k | K_(n+1/2)(x) = sqrt(pi/(2x)) e^-x sum_(k<=n) (n+k)!/(k! (n-k)!) (2x)^-k, also for symbolic x |
//! | bessel_i | I_nu(0); I_(1/2)(x) = sqrt(2/(pi x)) sinh x, I_(-1/2)(x) = sqrt(2/(pi x)) cosh x |
//!
//! Sizes are capped (integers up to 2^20 for factorials, 10^4 terms for harmonic numbers and sums, B_k up to
//! k = 500, half-integer Bessel orders up to 200); beyond, the call stays unevaluated and `n` gives its value.
use crate::bernoulli::bernoulli;
use mc_arith::num::{Int, Nat, Rat};
use mc_arith::{Arith, Num};
use mc_core::{Heap, Store, T};

/// The result of an exact rule.
pub enum Exact<'h> {
    /// the value
    Value(T<'h>),
    /// the argument is a pole
    Pole,
}

const FACT_MAX: i64 = 1 << 20;
const SUM_MAX: i64 = 10_000;
const ZETA_MAX: i64 = 500;
const BESSEL_MAX: i64 = 200;

fn fact(n: u64) -> Nat {
    // a balanced product tree keeps the multiplications even-sized
    fn range(a: u64, b: u64) -> Nat {
        if b - a < 16 {
            let mut p = Nat::from_u64(1);
            for k in a..=b {
                p = p.mul_u64(k);
            }
            return p;
        }
        let m = (a + b) / 2;
        range(a, m).mul(&range(m + 1, b))
    }
    if n < 2 {
        Nat::from_u64(1)
    } else {
        range(2, n)
    }
}

fn rat(n: Int, d: Int) -> Rat {
    Rat::new(n, d)
}

fn int_of(q: &Rat) -> Option<i64> {
    if q.is_int() {
        q.num.to_i64()
    } else {
        None
    }
}

/// k with q = k + 1/2.
fn half_int(q: &Rat) -> Option<i64> {
    if q.den.bits() == 2 && q.den.to_u128() == Some(2) {
        let k = q.num.to_i64()?;
        Some((k - 1).div_euclid(2))
    } else {
        None
    }
}

/// The term builder of the rules.
pub struct Rules<'a, 'h> {
    h: &'h Heap,
    ar: &'a Arith<'h, Heap>,
}

impl<'a, 'h> Rules<'a, 'h> {
    /// Rules building with `ar`.
    pub fn new(h: &'h Heap, ar: &'a Arith<'h, Heap>) -> Rules<'a, 'h> {
        Rules { h, ar }
    }

    fn q(&self, q: &Rat) -> T<'h> {
        self.ar.number(&Num::rat(q.clone()))
    }
    fn n(&self, v: i64) -> T<'h> {
        self.ar.int(v)
    }
    fn sym(&self, s: &str) -> T<'h> {
        self.h.sym_term(self.h.sym(s))
    }
    fn call(&self, f: &str, args: &[T<'h>]) -> T<'h> {
        self.h.call(self.h.sym(f), args)
    }
    fn sqrt_pi(&self) -> T<'h> {
        self.ar.pow(self.sym("pi"), self.q(&rat(Int::from_i64(1), Int::from_i64(2))))
    }

    /// Gamma(x) for an exact x as c sqrt(pi)^h, with h = 0 or 1: (c, h).
    fn gamma_parts(x: &Rat) -> Option<Result<(Rat, bool), ()>> {
        if let Some(n) = int_of(x) {
            if n <= 0 {
                return Some(Err(()));
            }
            if n > FACT_MAX {
                return None;
            }
            return Some(Ok((Rat::from_int(Int::from_nat(fact(n as u64 - 1))), false)));
        }
        let k = half_int(x)?;
        if k.abs() > SUM_MAX {
            return None;
        }
        Some(Ok(if k >= 0 {
            let k = k as u64;
            let num = fact(2 * k);
            let den = Nat::pow2(2 * k).mul(&fact(k));
            (rat(Int::from_nat(num), Int::from_nat(den)), true)
        } else {
            let j = (-k) as u64;
            let num = Int::new(j % 2 == 1, Nat::pow2(2 * j).mul(&fact(j)));
            (rat(num, Int::from_nat(fact(2 * j))), true)
        }))
    }

    fn gamma_term(&self, c: &Rat, sqrt_pi: bool) -> T<'h> {
        if sqrt_pi {
            self.ar.mul(&[self.q(c), self.sqrt_pi()])
        } else {
            self.q(c)
        }
    }

    /// gamma(x).
    pub fn gamma(&self, x: &Rat) -> Option<Exact<'h>> {
        Some(match Self::gamma_parts(x)? {
            Ok((c, h)) => Exact::Value(self.gamma_term(&c, h)),
            Err(()) => Exact::Pole,
        })
    }

    /// log_gamma(x).
    pub fn log_gamma(&self, x: &Rat) -> Option<Exact<'h>> {
        if int_of(x).is_some_and(|n| n > 1000) {
            return None;
        }
        let (c, h) = match Self::gamma_parts(x)? {
            Ok(v) => v,
            Err(()) => return Some(Exact::Pole),
        };
        let mut terms = vec![];
        let abs = Rat { num: Int::from_nat(c.num.mag.clone()), den: c.den.clone() };
        if !(abs.num.mag.bits() == 1 && abs.den.bits() == 1) {
            terms.push(self.call("log", &[self.q(&abs)]));
        }
        if h {
            let half = self.q(&rat(Int::from_i64(1), Int::from_i64(2)));
            terms.push(self.ar.mul(&[half, self.call("log", &[self.sym("pi")])]));
        }
        if x.num.neg {
            // the principal value: -pi ceil(-x) im
            let k = x.neg().floor_fract().0.add(&Int::from_i64(1));
            let im = self.ar.number(&Num::i());
            terms.push(self.ar.mul(&[self.ar.integer(&k.neg()), self.sym("pi"), im]));
        }
        Some(Exact::Value(self.ar.add(&terms)))
    }

    /// The harmonic number H_n.
    fn harmonic(n: u64) -> Rat {
        let mut s = Rat::from_i64(0);
        for k in 1..=n {
            s = s.add(&rat(Int::from_i64(1), Int::from_u64(k)));
        }
        s
    }

    /// digamma(x).
    pub fn digamma(&self, x: &Rat) -> Option<Exact<'h>> {
        let eg = self.sym("euler_gamma");
        if let Some(n) = int_of(x) {
            if n <= 0 {
                return Some(Exact::Pole);
            }
            if n > SUM_MAX {
                return None;
            }
            let h = self.q(&Self::harmonic(n as u64 - 1));
            return Some(Exact::Value(self.ar.sub(h, eg)));
        }
        let k = half_int(x)?;
        // psi(k + 1/2) = psi(1/2 - k) (the reflection's cotangent vanishes): negative k as -k
        let k = k.abs();
        if k > SUM_MAX {
            return None;
        }
        let mut s = Rat::from_i64(0);
        for j in 1..=k {
            s = s.add(&rat(Int::from_i64(2), Int::from_i64(2 * j - 1)));
        }
        let log2 = self.ar.mul(&[self.n(-2), self.call("log", &[self.n(2)])]);
        Some(Exact::Value(self.ar.add(&[self.q(&s), self.ar.neg(eg), log2])))
    }

    /// polygamma(n, x) for an exact integer n >= 1 and an exact x.
    pub fn polygamma(&self, n: i64, x: &Rat) -> Option<Exact<'h>> {
        if !(1..=1000).contains(&n) {
            return None;
        }
        let e = n as u64 + 1;
        let sign_fact = Int::new(n % 2 == 0, fact(n as u64));
        let zeta = self.call("zeta", &[self.n(e as i64)]);
        if let Some(m) = int_of(x) {
            if m <= 0 {
                return Some(Exact::Pole);
            }
            if m > SUM_MAX {
                return None;
            }
            // (-1)^(n+1) n! (zeta(n+1) - sum_{k<m} k^-(n+1))
            let mut s = Rat::from_i64(0);
            for k in 1..m as u64 {
                s = s.add(&rat(Int::from_i64(1), Int::from_nat(Nat::from_u64(k).pow_u64(e))));
            }
            let inner = self.ar.sub(zeta, self.q(&s));
            return Some(Exact::Value(self.ar.mul(&[self.ar.integer(&sign_fact), inner])));
        }
        let k = half_int(x)?;
        if !(0..=SUM_MAX).contains(&k) {
            return None;
        }
        // psi^(n)(1/2) = (-1)^(n+1) n! (2^(n+1) - 1) zeta(n+1), and psi^(n)(x+1) = psi^(n)(x) + (-1)^n n! x^-(n+1)
        let c = Int::from_nat(Nat::pow2(e).sub_u64(1));
        let base = self.ar.mul(&[self.ar.integer(&sign_fact.mul(&c)), zeta]);
        let mut s = Rat::from_i64(0);
        for j in 0..k as u64 {
            // (j + 1/2)^-(n+1) = 2^(n+1) / (2j + 1)^(n+1)
            s = s.add(&rat(Int::from_nat(Nat::pow2(e)), Int::from_nat(Nat::from_u64(2 * j + 1).pow_u64(e))));
        }
        let corr = s.mul(&Rat::from_int(sign_fact.neg()));
        Some(Exact::Value(self.ar.add(&[base, self.q(&corr)])))
    }

    /// beta(a, b).
    pub fn beta(&self, a: &Rat, b: &Rat) -> Option<Exact<'h>> {
        let ab = a.add(b);
        let (ga, gb, gab) = (Self::gamma_parts(a), Self::gamma_parts(b), Self::gamma_parts(&ab));
        match (ga?, gb?, gab?) {
            (Ok((ca, ha)), Ok((cb, hb)), Ok((cab, hab))) => {
                let num = self.ar.mul(&[self.gamma_term(&ca, ha), self.gamma_term(&cb, hb)]);
                Some(Exact::Value(self.ar.div(num, self.gamma_term(&cab, hab))))
            }
            (Ok(_), Ok(_), Err(())) => Some(Exact::Value(self.n(0))),
            (_, _, Err(())) => None,
            _ => Some(Exact::Pole),
        }
    }

    /// zeta(s).
    pub fn zeta(&self, s: &Rat) -> Option<Exact<'h>> {
        let n = int_of(s)?;
        match n {
            1 => Some(Exact::Pole),
            0 => Some(Exact::Value(self.q(&rat(Int::from_i64(-1), Int::from_i64(2))))),
            n if n > 0 && n % 2 == 0 && n <= ZETA_MAX => {
                // (-1)^(m+1) B_2m (2 pi)^2m / (2 (2m)!) = |B_2m| 2^(2m-1) / (2m)! pi^2m
                let b = bernoulli(n as u64)?;
                let c = rat(Int::from_nat(b.num.mag.shl(n as u64 - 1)), Int::from_nat(b.den.mul(&fact(n as u64))));
                Some(Exact::Value(self.ar.mul(&[self.q(&c), self.ar.pow(self.sym("pi"), self.n(n))])))
            }
            n if n < 0 && -n < ZETA_MAX => {
                let m = (-n) as u64;
                let b = bernoulli(m + 1)?;
                let v = b.mul(&rat(Int::from_i64(if m % 2 == 1 { -1 } else { 1 }), Int::from_u64(m + 1)));
                Some(Exact::Value(self.q(&v)))
            }
            _ => None,
        }
    }

    /// bessel_k(nu, x) for an exact nu: the closed form at half-integer orders (any x but an exact number <= 0),
    /// the pole at x = 0.
    pub fn bessel_k(&self, nu: &Rat, x: T<'h>) -> Option<Exact<'h>> {
        if let Some(Num::Real(r)) = self.ar.num(x) {
            if r.is_exact_zero() {
                return Some(Exact::Pole);
            }
            if r.is_negative() || !matches!(r, mc_arith::Real::Exact(_)) {
                return None;
            }
        } else if self.ar.num(x).is_some() {
            return None;
        }
        let k = half_int(nu)?;
        // K_-nu = K_nu: n + 1/2 = |nu|
        let n = if k < 0 { -k - 1 } else { k };
        if n > BESSEL_MAX {
            return None;
        }
        let n = n as u64;
        // sum_k (n+k)!/(k! (n-k)!) (2x)^-k
        let two_x = self.ar.mul(&[self.n(2), x]);
        let mut terms = vec![];
        for j in 0..=n {
            let c = fact(n + j).div(&fact(j).mul(&fact(n - j)));
            let pw = self.ar.pow(two_x, self.n(-(j as i64)));
            terms.push(self.ar.mul(&[self.ar.integer(&Int::from_nat(c)), pw]));
        }
        let poly = self.ar.add(&terms);
        let half = self.q(&rat(Int::from_i64(-1), Int::from_i64(2)));
        let pre = self.ar.mul(&[self.sqrt_pi(), self.ar.pow(two_x, half)]);
        let e = self.call("exp", &[self.ar.neg(x)]);
        Some(Exact::Value(self.ar.mul(&[pre, e, poly])))
    }

    /// bessel_i(nu, x) for an exact nu: I_nu(0) and the orders ±1/2.
    pub fn bessel_i(&self, nu: &Rat, x: T<'h>) -> Option<Exact<'h>> {
        if self.ar.num(x).is_some_and(|v| v.is_exact_zero()) {
            return Some(if nu.is_zero() {
                Exact::Value(self.n(1))
            } else if !nu.num.neg || nu.is_int() {
                Exact::Value(self.n(0))
            } else {
                Exact::Pole
            });
        }
        if self.ar.num(x).is_some_and(|v| !v.is_exact() || v.as_real().is_some_and(|r| r.is_negative())) {
            return None;
        }
        let k = half_int(nu)?;
        let f = match k {
            0 => "sinh",
            -1 => "cosh",
            _ => return None,
        };
        // sqrt(2/(pi x)) = (pi x / 2)^(-1/2)
        let arg = self.ar.mul(&[self.q(&rat(Int::from_i64(1), Int::from_i64(2))), self.sym("pi"), x]);
        let pre = self.ar.pow(arg, self.q(&rat(Int::from_i64(-1), Int::from_i64(2))));
        Some(Exact::Value(self.ar.mul(&[pre, self.call(f, &[x])])))
    }
}
