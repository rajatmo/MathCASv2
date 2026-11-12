//! Truncated power series (decision K.11): `series(e, x, x0, n)` and `normal(s)`.
//!
//! A series in t = x - x0 is a valuation v, coefficients of t^v, t^(v+1), ... and an absolute precision p (the
//! series is known modulo t^p). The expression is evaluated bottom-up (explicit stack) with series arithmetic:
//! sums, products, inverses, integer and rational powers, and exp, log, sin, cos, tan, sinh, cosh, tanh, atan
//! and sqrt through their differential equations (each coefficient by an O(n) recurrence, so O(n^2) per
//! function). Every operation tracks the precision it can guarantee; when the result falls short of the request
//! (poles in denominators eat precision), the evaluation is repeated with more working precision.
//!
//! Coefficients are exact rationals when the expansion stays rational (the common case, P12); otherwise
//! (symbolic constants, `exp(1)`, inexact numbers, a symbolic expansion point) the whole evaluation is redone
//! with coefficients as canonical terms, expanded at the end, and the result is evaluated again by the
//! evaluator (so that `exp(1)` meets the rules of `exp`).
//!
//! The result is the term `series_data(x, x0, [c_v, ..., c_(p-1)], v, p)`, the packed form of
//! `sum c_k (x - x0)^k + O((x - x0)^p)`; `normal` turns it into that sum.
use crate::natives::{Env, Out};
use crate::qpoly::rat_one;
use crate::work::{Halt, Meter, WorkResult};
use mc_api::{Store, View, T};
use mc_arith::num::{gcd, Int, Nat, Rat};
use mc_arith::{Arith, Num};
use mc_core::Heap;
use mc_eval::NativeCx;
use rustc_hash::FxHashMap;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// Why a series evaluation did not produce a value.
enum SErr {
    /// the coefficients need terms (retry in the term ring)
    NeedTerms,
    /// not expandable here (an unknown function of x, a branch point, an essential singularity)
    Unsupported,
    /// a division by a series that is zero to the working precision
    Zero,
    /// a fractional power needs the variable ramified: retry with x - x0 = t^(den * d) (Puiseux series)
    Ramify(i64),
    /// stopped or out of memory
    Halt(Halt),
}

impl From<Halt> for SErr {
    fn from(h: Halt) -> SErr {
        SErr::Halt(h)
    }
}

type SRes<X> = Result<X, SErr>;

/// (sine, cosine) of one argument.
type Pair<X> = (X, X);

/// The coefficient ring of a series evaluation.
trait Ring<'h> {
    type C: Clone;
    fn rat(&self, q: &Rat) -> Self::C;
    fn int(&self, v: i64) -> Self::C {
        self.rat(&Rat::from_i64(v))
    }
    fn add(&self, a: &Self::C, b: &Self::C) -> Self::C;
    fn sub(&self, a: &Self::C, b: &Self::C) -> Self::C;
    fn mul(&self, a: &Self::C, b: &Self::C) -> Self::C;
    fn neg(&self, a: &Self::C) -> Self::C;
    fn is_zero(&self, a: &Self::C, m: &mut Meter) -> SRes<bool>;
    /// 1/a for a nonzero a.
    fn inv(&self, a: &Self::C) -> SRes<Self::C>;
    /// A constant subterm (free of x).
    fn constant(&self, t: T<'h>) -> SRes<Self::C>;
    /// f(a) for f in exp, log, sin, cos, atan at a nonzero constant a.
    fn func(&self, f: &str, a: &Self::C) -> SRes<Self::C>;
    /// a^q for a rational q.
    fn pow_rat(&self, a: &Self::C, q: &Rat) -> SRes<Self::C>;
    /// log(t) for the series variable t, as a coefficient (logarithmic series need terms).
    fn log_t(&self) -> SRes<Self::C> {
        Err(SErr::NeedTerms)
    }
    /// log(-t) as a coefficient when `c` is a negative real number (log(c t) = log(-c) + log(-t) holds for every
    /// real t, log(c) + log(t) only for t > 0).
    fn log_neg_t(&self, _c: &Self::C) -> Option<Self::C> {
        None
    }
    /// The sign (-1 or 1) of `c` when it is a nonzero real number, else `None`.
    fn real_sign(&self, _c: &Self::C) -> Option<i64> {
        None
    }
    /// The coefficients of exp(f) (f_0 = 0) by a faster method of the ring, if it has one.
    fn fast_exp0(&self, _f: &[Self::C], _m: &mut Meter) -> SRes<Option<Vec<Self::C>>> {
        Ok(None)
    }
    /// The coefficients of (sin f, cos f) or (sinh f, cosh f) (f_0 = 0) by a faster method, if any.
    fn fast_sincos0(&self, _f: &[Self::C], _hyperbolic: bool, _m: &mut Meter) -> SRes<Option<Pair<Vec<Self::C>>>> {
        Ok(None)
    }
}

/// The exponential-generating-function form of the recurrences of exp, sin and cos over Q, which needs no gcd
/// in its inner loop (decision K.12). With f_j = a_j / (L j!) for integers a_j (L the common denominator of the
/// j! f_j), a solution g of k g_k = sum_j j f_j h_(k-j) is g_k = G_k / (L^k k!) with integers
/// G_k = sum_j C(k-1, j-1) a_j L^(j-1) H_(k-j). Each output coefficient is reduced once, at the end.
struct Egf {
    /// (j, a_j L^(j-1)) for the nonzero a_j
    w: Vec<(usize, Int)>,
    l: Int,
    n: usize,
}

impl Egf {
    fn new(f: &[Rat], m: &mut Meter) -> SRes<Egf> {
        let n = f.len();
        let mut fact = Int::from_i64(1);
        let mut a: Vec<(usize, Rat)> = vec![];
        for (j, fj) in f.iter().enumerate().skip(1) {
            m.step(1)?;
            fact = fact.mul(&Int::from_i64(j as i64));
            if !fj.is_zero() {
                a.push((j, fj.mul(&Rat::from_int(fact.clone()))));
            }
        }
        // the common denominator of the j! f_j
        let mut l = Nat::from_u64(1);
        for (_, q) in &a {
            if !gcd::is_one(&q.den) {
                let g = gcd::gcd(&l, &q.den);
                l = l.mul(&gcd::div_exact(&q.den, &g));
            }
        }
        let l = Int::from_nat(l);
        let mut w = Vec::with_capacity(a.len());
        let mut lp = Int::from_i64(1);
        let mut last = 1;
        for (j, q) in a {
            m.step(1)?;
            while last < j {
                lp = lp.mul(&l);
                last += 1;
            }
            let aj = Int::new(q.num.neg, gcd::div_exact(&l.mag.mul(&q.num.mag), &q.den));
            w.push((j, aj.mul(&lp)));
        }
        Ok(Egf { w, l, n })
    }
    /// Run the coupled recurrences G_k = sign_i * sum_j C(k-1, j-1) w_j H_(k-j), where `rec[i] = (source, sign)`
    /// names the sequence H feeding sequence i; `init[i]` is G_0.
    fn run(&self, init: &[i64], rec: &[(usize, bool)], m: &mut Meter) -> SRes<Vec<Vec<Int>>> {
        let mut seqs: Vec<Vec<Int>> = init.iter().map(|&v| vec![Int::from_i64(v)]).collect();
        // row k - 1 of Pascal's triangle
        let mut row: Vec<Int> = vec![Int::from_i64(1)];
        for k in 1..self.n {
            // the products cost about the size of the last values in limbs
            let size = seqs.iter().map(|sq| sq[k - 1].mag.limbs.len() as u64 + 1).sum::<u64>();
            m.step((self.w.len() as u64 + k as u64) * size)?;
            let mut next = Vec::with_capacity(seqs.len());
            for &(src, negate) in rec {
                let mut s = Int::default();
                for (j, wj) in self.w.iter().take_while(|(j, _)| *j <= k) {
                    let h = &seqs[src][k - j];
                    if h.is_zero() {
                        continue;
                    }
                    // the weights of sin and cos are +-1: skip that product
                    let t = if wj.mag.limbs == [1] { row[j - 1].mul(h) } else { row[j - 1].mul(wj).mul(h) };
                    s = if wj.neg && wj.mag.limbs == [1] { s.sub(&t) } else { s.add(&t) };
                }
                next.push(if negate { s.neg() } else { s });
            }
            for (sq, v) in seqs.iter_mut().zip(next) {
                sq.push(v);
            }
            // row k from row k - 1
            let mut nr = Vec::with_capacity(k + 1);
            nr.push(Int::from_i64(1));
            for i in 1..k {
                nr.push(row[i - 1].add(&row[i]));
            }
            nr.push(Int::from_i64(1));
            row = nr;
        }
        Ok(seqs)
    }
    /// g_k = G_k / (L^k k!), reduced.
    fn finish(&self, g: &[Int], m: &mut Meter) -> SRes<Vec<Rat>> {
        let mut den = Int::from_i64(1);
        let mut out = Vec::with_capacity(g.len());
        for (k, gk) in g.iter().enumerate() {
            // a gcd costs about the square of the size
            let l = den.mag.limbs.len() as u64 + 1;
            m.step(l * l / 4)?;
            if k > 0 {
                den = den.mul(&self.l).mul(&Int::from_i64(k as i64));
            }
            out.push(Rat::new(gk.clone(), den.clone()));
        }
        Ok(out)
    }
}

/// Exact rational coefficients.
struct QRing<'a, 'h> {
    ar: &'a Arith<'h, Heap>,
}

impl<'h> Ring<'h> for QRing<'_, 'h> {
    type C = Rat;
    fn rat(&self, q: &Rat) -> Rat {
        q.clone()
    }
    fn add(&self, a: &Rat, b: &Rat) -> Rat {
        a.add(b)
    }
    fn sub(&self, a: &Rat, b: &Rat) -> Rat {
        a.sub(b)
    }
    fn mul(&self, a: &Rat, b: &Rat) -> Rat {
        a.mul(b)
    }
    fn neg(&self, a: &Rat) -> Rat {
        a.neg()
    }
    fn is_zero(&self, a: &Rat, _m: &mut Meter) -> SRes<bool> {
        Ok(a.is_zero())
    }
    fn inv(&self, a: &Rat) -> SRes<Rat> {
        a.recip().ok_or(SErr::Zero)
    }
    fn constant(&self, t: T<'h>) -> SRes<Rat> {
        match self.ar.num(t) {
            Some(x) => x.as_rat().cloned().ok_or(SErr::NeedTerms),
            None => Err(SErr::NeedTerms),
        }
    }
    fn func(&self, _f: &str, _a: &Rat) -> SRes<Rat> {
        Err(SErr::NeedTerms)
    }
    fn real_sign(&self, c: &Rat) -> Option<i64> {
        if c.is_zero() {
            None
        } else if c.num.neg {
            Some(-1)
        } else {
            Some(1)
        }
    }
    fn fast_exp0(&self, f: &[Rat], m: &mut Meter) -> SRes<Option<Vec<Rat>>> {
        let e = Egf::new(f, m)?;
        let g = e.run(&[1], &[(0, false)], m)?;
        Ok(Some(e.finish(&g[0], m)?))
    }
    fn fast_sincos0(&self, f: &[Rat], hyperbolic: bool, m: &mut Meter) -> SRes<Option<Pair<Vec<Rat>>>> {
        let e = Egf::new(f, m)?;
        // S from C, C from S (negated for the circular functions)
        let g = e.run(&[0, 1], &[(1, false), (0, !hyperbolic)], m)?;
        Ok(Some((e.finish(&g[0], m)?, e.finish(&g[1], m)?)))
    }
    fn pow_rat(&self, a: &Rat, q: &Rat) -> SRes<Rat> {
        let b = self.ar.number(&Num::rat(a.clone()));
        let e = self.ar.number(&Num::rat(q.clone()));
        let r = self.ar.pow(b, e);
        match self.ar.num(r) {
            Some(x) => x.as_rat().cloned().ok_or(SErr::NeedTerms),
            None => Err(SErr::NeedTerms),
        }
    }
}

/// Coefficients as canonical terms.
struct TRing<'a, 'h> {
    env: &'a Env<'h>,
    /// log(t) in terms of x (set per evaluation)
    logt: std::cell::Cell<Option<T<'h>>>,
    /// log(-t) in terms of x, when known
    logneg: std::cell::Cell<Option<T<'h>>>,
}

impl<'h> TRing<'_, 'h> {
    fn h(&self) -> &'h Heap {
        self.env.h()
    }
    /// The expanded form (rational coefficients over the kernels), which decides zero.
    /// sin, cos, exp, sinh, cosh at a multiple of pi/2 (real for sin and cos, imaginary for sinh and cosh, either
    /// for exp): its exact value, so a zero there is seen as zero (poles of tan, csch, ... at such points;
    /// decision IOSYM.13).
    fn special_value(&self, f: &str, a: T<'h>) -> Option<T<'h>> {
        let ar = &self.env.ar;
        let h = self.h();
        let pi = h.sym_term(h.sym("pi"));
        let q = ar.mul(&[a, ar.pow(pi, h.int(-1))]);
        // 2q as an integer
        let half = |q: T<'h>| -> Option<i64> { ar.num(q)?.mul(&Num::int(2)).as_int()?.to_i64() };
        // i^k
        let ipow = |k: i64| match k.rem_euclid(4) {
            0 => h.int(1),
            1 => ar.number(&Num::i()),
            2 => h.int(-1),
            _ => ar.neg(ar.number(&Num::i())),
        };
        let sin = |k: i64| [0, 1, 0, -1][k.rem_euclid(4) as usize];
        let cos = |k: i64| [1, 0, -1, 0][k.rem_euclid(4) as usize];
        if let Some(k) = half(q) {
            return match f {
                "sin" => Some(h.int(sin(k))),
                "cos" => Some(h.int(cos(k))),
                "exp" if k == 0 => Some(h.int(1)),
                _ => None,
            };
        }
        let qi = ar.mul(&[q, ar.neg(ar.number(&Num::i()))]);
        let k = half(qi)?;
        match f {
            "sinh" => Some(ar.mul(&[ar.number(&Num::i()), h.int(sin(k))])),
            "cosh" => Some(h.int(cos(k))),
            "exp" => Some(ipow(k)),
            _ => None,
        }
    }

    fn expanded(&self, t: T<'h>, m: &mut Meter) -> SRes<T<'h>> {
        let (ring, ps) = self.env.read(&[t], &[], m)?;
        Ok(self.env.conv().to_term(&ps[0], &ring, m)?)
    }
}

impl<'h> Ring<'h> for TRing<'_, 'h> {
    type C = T<'h>;
    fn rat(&self, q: &Rat) -> T<'h> {
        self.env.ar.number(&Num::rat(q.clone()))
    }
    fn add(&self, a: &T<'h>, b: &T<'h>) -> T<'h> {
        self.env.ar.add(&[*a, *b])
    }
    fn sub(&self, a: &T<'h>, b: &T<'h>) -> T<'h> {
        self.env.ar.sub(*a, *b)
    }
    fn mul(&self, a: &T<'h>, b: &T<'h>) -> T<'h> {
        self.env.ar.mul(&[*a, *b])
    }
    fn neg(&self, a: &T<'h>) -> T<'h> {
        self.env.ar.neg(*a)
    }
    fn is_zero(&self, a: &T<'h>, m: &mut Meter) -> SRes<bool> {
        Ok(self.expanded(*a, m)? == self.h().int(0))
    }
    fn inv(&self, a: &T<'h>) -> SRes<T<'h>> {
        Ok(self.env.ar.pow(*a, self.h().int(-1)))
    }
    fn constant(&self, t: T<'h>) -> SRes<T<'h>> {
        Ok(t)
    }
    fn func(&self, f: &str, a: &T<'h>) -> SRes<T<'h>> {
        let h = self.h();
        // exp(c log(u)) is u^c (the principal power): the logarithmic coefficients of x^k read as powers
        if f == "exp" {
            let log = h.sym_term(h.sym("log"));
            let factors: Vec<T<'h>> = match h.view(*a) {
                View::App { head, args } if head == self.env.hd.mul => args.to_vec(),
                _ => vec![*a],
            };
            let at = factors.iter().position(|&g| matches!(h.view(g), View::App { head, args: &[_] } if head == log));
            if let Some(i) = at {
                let View::App { args: &[u], .. } = h.view(factors[i]) else { return Ok(h.call(h.sym(f), &[*a])) };
                let rest: Vec<T<'h>> = factors.iter().enumerate().filter(|&(j, _)| j != i).map(|(_, &g)| g).collect();
                return Ok(self.env.ar.pow(u, self.env.ar.mul(&rest)));
            }
        }
        if let Some(v) = self.special_value(f, *a) {
            return Ok(v);
        }
        if f == "exp" {
            // the canonical e^a, so that it cancels against an e^a of the input (item 13C)
            return Ok(self.env.ar.pow(h.sym_term(h.sym("e")), *a));
        }
        Ok(h.call(h.sym(f), &[*a]))
    }
    fn pow_rat(&self, a: &T<'h>, q: &Rat) -> SRes<T<'h>> {
        Ok(self.env.ar.pow(*a, self.rat(q)))
    }
    fn log_t(&self) -> SRes<T<'h>> {
        self.logt.get().ok_or(SErr::Unsupported)
    }
    fn real_sign(&self, c: &T<'h>) -> Option<i64> {
        let n = self.env.ar.num(*c)?;
        let r = n.as_real()?;
        if r.is_positive() {
            Some(1)
        } else if r.is_negative() {
            Some(-1)
        } else {
            None
        }
    }
    fn log_neg_t(&self, c: &T<'h>) -> Option<T<'h>> {
        let neg = self.env.ar.num(*c).is_some_and(|n| n.as_real().is_some_and(|r| r.is_negative()));
        if neg {
            self.logneg.get()
        } else {
            None
        }
    }
}

/// A truncated series: c[i] is the coefficient of t^(val + i); known modulo t^prec (prec >= val + c.len()).
#[derive(Clone)]
struct Ser<C> {
    val: i64,
    c: Vec<C>,
    prec: i64,
}

/// Series arithmetic over a ring, truncated at the working precision `cap` (absolute).
struct Arithm<'r, R> {
    r: &'r R,
    cap: i64,
    /// the series variable t is known to be positive (at infinity, x = ±t^-den with t -> 0+)
    pos_t: bool,
}

impl<'h, R: Ring<'h>> Arithm<'_, R> {
    fn coef(&self, s: &Ser<R::C>, k: i64) -> R::C {
        if k < s.val || k - s.val >= s.c.len() as i64 {
            self.r.int(0)
        } else {
            s.c[(k - s.val) as usize].clone()
        }
    }
    /// A series from coefficients, with leading zeros removed and the length fitted to the precision.
    fn make(&self, val: i64, mut c: Vec<R::C>, prec: i64, m: &mut Meter) -> SRes<Ser<R::C>> {
        let prec = prec.min(self.cap);
        let mut val = val;
        let keep = (prec - val).max(0) as usize;
        c.truncate(keep);
        let mut lead = 0;
        while lead < c.len() && self.r.is_zero(&c[lead], m)? {
            lead += 1;
        }
        c.drain(..lead);
        val += lead as i64;
        if c.is_empty() {
            val = prec;
        }
        Ok(Ser { val, c, prec })
    }
    fn constant(&self, a: R::C, m: &mut Meter) -> SRes<Ser<R::C>> {
        self.make(0, vec![a], self.cap, m)
    }
    fn add(&self, a: &Ser<R::C>, b: &Ser<R::C>, m: &mut Meter) -> SRes<Ser<R::C>> {
        let prec = a.prec.min(b.prec);
        let val = a.val.min(b.val);
        let mut c = Vec::with_capacity((prec - val).max(0) as usize);
        for k in val..prec {
            m.step(1)?;
            c.push(self.r.add(&self.coef(a, k), &self.coef(b, k)));
        }
        self.make(val, c, prec, m)
    }
    fn neg(&self, a: &Ser<R::C>) -> Ser<R::C> {
        Ser { val: a.val, c: a.c.iter().map(|x| self.r.neg(x)).collect(), prec: a.prec }
    }
    fn scale(&self, a: &Ser<R::C>, k: &R::C) -> Ser<R::C> {
        Ser { val: a.val, c: a.c.iter().map(|x| self.r.mul(x, k)).collect(), prec: a.prec }
    }
    fn mul(&self, a: &Ser<R::C>, b: &Ser<R::C>, m: &mut Meter) -> SRes<Ser<R::C>> {
        let val = a.val + b.val;
        let prec = (a.prec + b.val).min(b.prec + a.val).min(self.cap);
        let n = (prec - val).max(0) as usize;
        let mut c = Vec::with_capacity(n);
        for k in 0..n {
            m.step(k as u64 + 1)?;
            let mut s = self.r.int(0);
            for i in 0..=k.min(a.c.len().saturating_sub(1)) {
                if k - i < b.c.len() {
                    s = self.r.add(&s, &self.r.mul(&a.c[i], &b.c[k - i]));
                }
            }
            c.push(s);
        }
        self.make(val, c, prec, m)
    }
    /// 1/a.
    fn inv(&self, a: &Ser<R::C>, m: &mut Meter) -> SRes<Ser<R::C>> {
        if a.c.is_empty() {
            return Err(SErr::Zero);
        }
        let n = (a.prec - a.val).max(0) as usize;
        let i0 = self.r.inv(&a.c[0])?;
        let mut b: Vec<R::C> = Vec::with_capacity(n);
        b.push(i0.clone());
        for k in 1..n {
            m.step(k as u64)?;
            let mut s = self.r.int(0);
            for j in 1..=k.min(a.c.len() - 1) {
                s = self.r.add(&s, &self.r.mul(&a.c[j], &b[k - j]));
            }
            b.push(self.r.neg(&self.r.mul(&i0, &s)));
        }
        self.make(-a.val, b, a.prec - 2 * a.val, m)
    }
    fn pow_int(&self, a: &Ser<R::C>, n: i64, m: &mut Meter) -> SRes<Ser<R::C>> {
        if n == 0 {
            return self.constant(self.r.int(1), m);
        }
        let base = if n < 0 { self.inv(a, m)? } else { a.clone() };
        let mut e = n.unsigned_abs();
        let mut result: Option<Ser<R::C>> = None;
        let mut b = base;
        loop {
            if e & 1 == 1 {
                result = Some(match result {
                    None => b.clone(),
                    Some(r) => self.mul(&r, &b, m)?,
                });
            }
            e >>= 1;
            if e == 0 {
                break;
            }
            b = self.mul(&b, &b, m)?;
        }
        // invariant: n != 0 (returned above), so the loop multiplied once
        Ok(result.expect("n != 0"))
    }
    /// The series split as c0 + u with u of positive valuation (a must have valuation >= 0).
    fn split(&self, a: &Ser<R::C>, m: &mut Meter) -> SRes<(R::C, Ser<R::C>)> {
        if a.val < 0 {
            // leading coefficients that cancelled (1/w - 1/w): drop them first (item 13C); a negative valuation
            // with nothing known below 0 wants more working precision
            let mut b = a.clone();
            while b.val < 0 && !b.c.is_empty() && self.r.is_zero(&b.c[0], m)? {
                b.c.remove(0);
                b.val += 1;
            }
            if b.c.is_empty() {
                b.val = b.prec;
            }
            if b.val < 0 {
                return Err(SErr::Unsupported);
            }
            if b.prec <= 0 {
                return Err(SErr::Zero);
            }
            return self.split(&b, m);
        }
        let c0 = self.coef(a, 0);
        let mut u = a.clone();
        if a.val == 0 && !u.c.is_empty() {
            u.c[0] = self.r.int(0);
        }
        Ok((c0, u))
    }
    /// The derivative coefficients k u_k of a series of valuation >= 0, as a dense vector from k = 0.
    fn dense(&self, u: &Ser<R::C>) -> Vec<R::C> {
        let n = (u.prec.max(0)) as usize;
        (0..n as i64).map(|k| self.coef(u, k)).collect()
    }
    /// The indices k >= 1 of the nonzero entries (the recurrences skip the zeros, half of sin's coefficients).
    fn nonzero(&self, f: &[R::C], m: &mut Meter) -> SRes<Vec<usize>> {
        let mut out = vec![];
        for (k, c) in f.iter().enumerate().skip(1) {
            if !self.r.is_zero(c, m)? {
                out.push(k);
            }
        }
        Ok(out)
    }
    /// exp(u) for u with u_0 = 0: n g_n = sum_k k u_k g_(n-k).
    fn exp0(&self, u: &Ser<R::C>, m: &mut Meter) -> SRes<Ser<R::C>> {
        let f = self.dense(u);
        if let Some(g) = self.r.fast_exp0(&f, m)? {
            return self.make(0, g, u.prec, m);
        }
        let n = f.len();
        let nz = self.nonzero(&f, m)?;
        let mut g = vec![self.r.int(1)];
        for k in 1..n {
            m.step(nz.len() as u64 + 1)?;
            let mut s = self.r.int(0);
            for &j in nz.iter().take_while(|&&j| j <= k) {
                s = self.r.add(&s, &self.r.mul(&self.r.mul(&self.r.int(j as i64), &f[j]), &g[k - j]));
            }
            g.push(self.r.mul(&s, &self.r.rat(&Rat::new(Int::from_i64(1), Int::from_i64(k as i64)))));
        }
        self.make(0, g, u.prec, m)
    }
    /// (sin u, cos u) for u with u_0 = 0: n S_n = sum k u_k C_(n-k), n C_n = -sum k u_k S_(n-k).
    fn sincos0(&self, u: &Ser<R::C>, hyperbolic: bool, m: &mut Meter) -> SRes<Pair<Ser<R::C>>> {
        let f = self.dense(u);
        if let Some((s, c)) = self.r.fast_sincos0(&f, hyperbolic, m)? {
            return Ok((self.make(0, s, u.prec, m)?, self.make(0, c, u.prec, m)?));
        }
        let n = f.len();
        let nz = self.nonzero(&f, m)?;
        let mut s = vec![self.r.int(0)];
        let mut c = vec![self.r.int(1)];
        for k in 1..n {
            m.step(2 * nz.len() as u64 + 1)?;
            let (mut a, mut b) = (self.r.int(0), self.r.int(0));
            for &j in nz.iter().take_while(|&&j| j <= k) {
                let w = self.r.mul(&self.r.int(j as i64), &f[j]);
                a = self.r.add(&a, &self.r.mul(&w, &c[k - j]));
                b = self.r.add(&b, &self.r.mul(&w, &s[k - j]));
            }
            let inv = self.r.rat(&Rat::new(Int::from_i64(1), Int::from_i64(k as i64)));
            s.push(self.r.mul(&a, &inv));
            let cb = self.r.mul(&b, &inv);
            c.push(if hyperbolic { cb } else { self.r.neg(&cb) });
        }
        Ok((self.make(0, s, u.prec, m)?, self.make(0, c, u.prec, m)?))
    }
    /// The series whose derivative is d (valuation >= 0) with constant term c0.
    fn integrate(&self, d: &Ser<R::C>, c0: R::C, m: &mut Meter) -> SRes<Ser<R::C>> {
        if d.val < 0 {
            return Err(SErr::Unsupported);
        }
        let n = (d.prec + 1).max(1) as usize;
        let mut c = vec![c0];
        for k in 1..n {
            m.step(1)?;
            let q = Rat::new(Int::from_i64(1), Int::from_i64(k as i64));
            c.push(self.r.mul(&self.coef(d, k as i64 - 1), &self.r.rat(&q)));
        }
        self.make(0, c, d.prec + 1, m)
    }
    fn derivative(&self, a: &Ser<R::C>, m: &mut Meter) -> SRes<Ser<R::C>> {
        let c: Vec<R::C> = a.c.iter().enumerate().map(|(i, x)| self.r.mul(x, &self.r.int(a.val + i as i64))).collect();
        self.make(a.val - 1, c, a.prec - 1, m)
    }
    fn exp(&self, a: &Ser<R::C>, m: &mut Meter) -> SRes<Ser<R::C>> {
        let (c0, u) = self.split(a, m)?;
        let e = self.exp0(&u, m)?;
        if self.r.is_zero(&c0, m)? {
            return Ok(e);
        }
        Ok(self.scale(&e, &self.r.func("exp", &c0)?))
    }
    fn log(&self, a: &Ser<R::C>, m: &mut Meter) -> SRes<Ser<R::C>> {
        if a.c.is_empty() {
            return Err(SErr::Unsupported);
        }
        // c t (1 + ...) with c < 0: (-c)(-t)(1 + ...), the principal log of which is log(-c) + log(-t) + ...
        if a.val == 1 {
            if let Some(ln) = self.r.log_neg_t(&a.c[0]) {
                return self.log_with(&self.neg(a), ln, m);
            }
        }
        if a.val != 0 {
            let lt = self.r.log_t()?;
            return self.log_with(a, lt, m);
        }
        self.log_with(a, self.r.int(0), m)
    }
    /// log(a) with `lt` the value taken by log(t) when a has a valuation.
    fn log_with(&self, a: &Ser<R::C>, lt: R::C, m: &mut Meter) -> SRes<Ser<R::C>> {
        if a.val != 0 {
            // log(t^v b) = v log(t) + log(b): log(t) is a coefficient (a logarithmic series)
            let b = Ser { val: 0, c: a.c.clone(), prec: a.prec - a.val };
            let lb = self.log_with(&b, lt.clone(), m)?;
            let k = self.r.mul(&self.r.int(a.val), &lt);
            return self.add(&lb, &self.constant(k, m)?, m);
        }
        // log(a) = log(a0) + integral(a' / a)
        let d = self.mul(&self.derivative(a, m)?, &self.inv(a, m)?, m)?;
        let a0 = a.c[0].clone();
        let one = self.r.int(1);
        let l0 = if self.r.is_zero(&self.r.sub(&a0, &one), m)? { self.r.int(0) } else { self.r.func("log", &a0)? };
        let mut s = self.integrate(&d, l0, m)?;
        s.prec = s.prec.min(a.prec);
        Ok(s)
    }
    fn sincos(&self, a: &Ser<R::C>, hyperbolic: bool, m: &mut Meter) -> SRes<Pair<Ser<R::C>>> {
        let (c0, u) = self.split(a, m)?;
        let (s, c) = self.sincos0(&u, hyperbolic, m)?;
        if self.r.is_zero(&c0, m)? {
            return Ok((s, c));
        }
        if hyperbolic {
            // sinh(c0 + u) and cosh(c0 + u) through exp(c0)
            let e = self.r.func("exp", &c0)?;
            let ei = self.r.inv(&e)?;
            let half = self.r.rat(&Rat::new(Int::from_i64(1), Int::from_i64(2)));
            let ch = self.r.mul(&self.r.add(&e, &ei), &half);
            let sh = self.r.mul(&self.r.sub(&e, &ei), &half);
            let sinh = self.add(&self.scale(&s, &ch), &self.scale(&c, &sh), m)?;
            let cosh = self.add(&self.scale(&c, &ch), &self.scale(&s, &sh), m)?;
            return Ok((sinh, cosh));
        }
        let (sc, cc) = (self.r.func("sin", &c0)?, self.r.func("cos", &c0)?);
        let sin = self.add(&self.scale(&c, &sc), &self.scale(&s, &cc), m)?;
        let cos = self.add(&self.scale(&c, &cc), &self.neg(&self.scale(&s, &sc)), m)?;
        Ok((sin, cos))
    }
    fn atan(&self, a: &Ser<R::C>, m: &mut Meter) -> SRes<Ser<R::C>> {
        // a pole of the argument: atan(a) = s pi/2 - atan(1/a) for Re(a) of sign s (DLMF section 4.24); with
        // t > 0 the leading term c t^v (v < 0) decides the sign of Re(a) when c is a nonzero real number
        let mut b = a.clone();
        while b.val < 0 && !b.c.is_empty() && self.r.is_zero(&b.c[0], m)? {
            b.c.remove(0);
            b.val += 1;
        }
        if b.val < 0 && !b.c.is_empty() && self.pos_t {
            let s = self.r.real_sign(&b.c[0]).ok_or(SErr::NeedTerms)?;
            let inner = self.atan(&self.inv(&b, m)?, m)?;
            // pi/2 = 2 atan(1)
            let quarter = self.r.func("atan", &self.r.int(1))?;
            let c = self.r.mul(&self.r.int(2 * s), &quarter);
            let mut out = self.add(&self.constant(c, m)?, &self.neg(&inner), m)?;
            out.prec = out.prec.min(inner.prec);
            return Ok(out);
        }
        let (c0, _) = self.split(a, m)?;
        // atan(a) = atan(a0) + integral(a' / (1 + a^2))
        let one = self.constant(self.r.int(1), m)?;
        let den = self.add(&one, &self.mul(a, a, m)?, m)?;
        let d = self.mul(&self.derivative(a, m)?, &self.inv(&den, m)?, m)?;
        let l0 = if self.r.is_zero(&c0, m)? { self.r.int(0) } else { self.r.func("atan", &c0)? };
        let mut s = self.integrate(&d, l0, m)?;
        s.prec = s.prec.min(a.prec);
        Ok(s)
    }
    /// a^q for a rational q: a = a_v t^v (1 + w), with v q an integer.
    fn pow_rat(&self, a: &Ser<R::C>, q: &Rat, m: &mut Meter) -> SRes<Ser<R::C>> {
        if a.c.is_empty() {
            return Err(SErr::Zero);
        }
        let vq = Rat::from_i64(a.val).mul(q);
        if !vq.is_int() {
            return Err(SErr::Ramify(Int::from_nat(vq.den.clone()).to_i64().unwrap_or(0)));
        }
        let shift = vq.num.to_i64().ok_or(SErr::Unsupported)?;
        let lead = a.c[0].clone();
        let li = self.r.inv(&lead)?;
        // f = a / (a_v t^v), f_0 = 1: g = f^q with k g_k = sum_j ((q + 1) j - k) f_j g_(k-j)
        let mut f: Vec<R::C> = a.c.iter().map(|x| self.r.mul(x, &li)).collect();
        // as many terms as the input's precision allows (up to the working precision): a base with few nonzero
        // terms (x = x0 + t) still has a power with many
        let n = (a.prec - a.val).min(self.cap - shift).max(f.len() as i64) as usize;
        f.resize(n, self.r.int(0));
        let q1 = self.r.rat(&q.add(&rat_one()));
        let mut g = vec![self.r.int(1)];
        for k in 1..n {
            m.step(k as u64)?;
            let mut s = self.r.int(0);
            for (j, fj) in f.iter().enumerate().take(k + 1).skip(1) {
                let w = self.r.sub(&self.r.mul(&q1, &self.r.int(j as i64)), &self.r.int(k as i64));
                s = self.r.add(&s, &self.r.mul(&self.r.mul(&w, fj), &g[k - j]));
            }
            g.push(self.r.mul(&s, &self.r.rat(&Rat::new(Int::from_i64(1), Int::from_i64(k as i64)))));
        }
        let c0 = self.r.pow_rat(&lead, q)?;
        let g: Vec<R::C> = g.iter().map(|x| self.r.mul(x, &c0)).collect();
        self.make(shift, g, shift + (a.prec - a.val), m)
    }
}

/// How a subterm of the expression is expanded.
enum Kind<'h, 't> {
    /// free of x: a constant coefficient
    Const(T<'h>),
    /// x itself
    Var,
    Add(&'h [T<'h>]),
    Mul(&'h [T<'h>]),
    /// base, exponent (either may involve x)
    Pow(T<'h>, T<'h>),
    /// a function of one argument: exp, log, sin, ...
    Func(&'static str, T<'h>),
    /// a series_data term in x at the same point
    Data(T<'h>),
    /// `O((x - x0)^p)`: an unknown remainder of order p (the printed form of a series reads back with one)
    Order(i64),
    /// another function with Taylor coefficients at its argument's value (computed by the native)
    Taylor(&'t Taylor<'h>),
}

/// The Taylor coefficients c_k = f^(k)(u0)/k! of a function f of the argument `arg`, at u0, the value of `arg` at the
/// expansion point (decision K.13: the native differentiates through the evaluator; the series multiplies out).
pub(crate) struct Taylor<'h> {
    pub arg: T<'h>,
    pub u0: T<'h>,
    pub c: Vec<T<'h>>,
    /// a term equal to this one near the point that the kernels can expand (item 13C: `gamma(u)` at a pole as
    /// `gamma(u + k + 1)/(u (u + 1) ... (u + k))`, `acos(u)` at 1 as `2 asin(sqrt((1 - u)/2))`)
    pub subst: Option<T<'h>>,
}

/// The Taylor table of one series call: the function terms the series kernels do not know, with coefficients.
pub(crate) type TaylorTable<'h> = FxHashMap<T<'h>, Taylor<'h>>;

/// The head of an order term `O((x - x0)^p)` (decision 9.3 of the library and kernel gaps).
pub(crate) const ORDER: &str = "O";

const FUNCS: [&str; 10] = ["exp", "log", "sin", "cos", "tan", "sinh", "cosh", "tanh", "atan", "sqrt"];

/// The pieces of the request.
#[derive(Clone, Copy)]
struct Request<'h, 't> {
    x: T<'h>,
    x0: T<'h>,
    /// the absolute order wanted in x - x0 (or 1/x): terms below (x - x0)^order_x
    order_x: i64,
    /// ramification: x - x0 = t^den (at infinity, x = t^-den)
    den: i64,
    /// the expansion point is inf (+1) or -inf (-1); 0 for a finite point
    inf: i64,
    /// the Taylor table of the native
    taylor: &'t TaylorTable<'h>,
    /// the value of log(x - x0) when the caller gives one (item 13C: Gruntz's log ω), else the term itself
    logx: Option<T<'h>>,
    /// expand from below: x = x0 - t^den (item 13C: real Puiseux series left of a branch point)
    reflect: bool,
}

impl Request<'_, '_> {
    /// The absolute order wanted in t.
    fn order(&self) -> i64 {
        // the terms up to (x - x0)^n, then the order term at the next exponent n + 1/den
        (self.order_x - 1) * self.den + 1
    }
}

/// Evaluate e as a series over ring R with working precision cap.
fn evaluate<'h, R: Ring<'h>>(
    env: &Env<'h>,
    r: &R,
    req: &Request<'h, '_>,
    e: T<'h>,
    cap: i64,
    m: &mut Meter,
) -> SRes<Ser<R::C>> {
    let h = env.h();
    let sa = Arithm { r, cap, pos_t: req.inf != 0 };
    let order_head = env.sym(ORDER);
    // which subterms involve x
    let mut dep: FxHashMap<T<'h>, bool> = FxHashMap::default();
    {
        let mut stack = vec![(e, false)];
        while let Some((t, done)) = stack.pop() {
            m.step(1)?;
            if dep.contains_key(&t) && !done {
                continue;
            }
            match h.view(t) {
                View::App { head, args } => {
                    if done {
                        // an order term involves x even when it is O(1)
                        let d = head == order_head || args.iter().any(|a| dep[a]) || dep.get(&head).copied().unwrap_or(false);
                        dep.insert(t, d);
                    } else {
                        stack.push((t, true));
                        for &a in args {
                            stack.push((a, false));
                        }
                        if let Some(sub) = req.taylor.get(&t).and_then(|tay| tay.subst) {
                            stack.push((sub, false));
                        }
                    }
                }
                _ => {
                    dep.insert(t, t == req.x);
                }
            }
        }
    }
    // the base of the powers in an order term O((x - x0)^p), in the canonical form the printed text reads back as
    let base = if req.x0 == h.int(0) { req.x } else { env.ar.sub(req.x, req.x0) };
    let order = |a: T<'h>| -> Option<i64> {
        if a == base {
            return Some(1);
        }
        match h.view(a) {
            View::App { head, args: &[b, p] } if head == env.hd.pow && b == base => env.int(p),
            _ => None,
        }
    };
    let plain = req.den == 1 && req.inf == 0;
    let kind = |t: T<'h>| -> Kind<'h, '_> {
        if let View::App { head, args: &[a] } = h.view(t) {
            if head == order_head && plain {
                if let Some(p) = if a == h.int(1) { Some(0) } else { order(a) } {
                    return Kind::Order(p);
                }
            }
        }
        if !dep[&t] {
            return Kind::Const(t);
        }
        if t == req.x {
            return Kind::Var;
        }
        let View::App { head, args } = h.view(t) else {
            return Kind::Const(t);
        };
        if head == env.hd.add {
            return Kind::Add(args);
        }
        if head == env.hd.mul {
            return Kind::Mul(args);
        }
        if head == env.hd.pow && args.len() == 2 {
            if args[0] == env.sym("e") {
                return Kind::Func("exp", args[1]);
            }
            return Kind::Pow(args[0], args[1]);
        }
        if plain && head == env.sym("series_data") && args.len() == 5 && args[0] == req.x && args[1] == req.x0 {
            return Kind::Data(t);
        }
        if args.len() == 1 {
            if let View::Sym(s) = h.view(head) {
                let name = h.sym_name(s);
                if let Some(f) = FUNCS.iter().find(|&&f| f == name) {
                    return Kind::Func(f, args[0]);
                }
            }
        }
        if let Some(tay) = req.taylor.get(&t) {
            return Kind::Taylor(tay);
        }
        Kind::Func("", t)
    };
    enum Task<'h> {
        Visit(T<'h>),
        Add(usize),
        Mul(usize),
        PowInt(i64),
        PowRat(Rat),
        PowGeneral,
        Func(&'static str),
        Taylor(usize),
    }
    let mut taylors: Vec<&Taylor<'h>> = vec![];
    let mut tasks = vec![Task::Visit(e)];
    let mut vals: Vec<Ser<R::C>> = vec![];
    while let Some(task) = tasks.pop() {
        m.step(1)?;
        match task {
            Task::Visit(t) => match kind(t) {
                Kind::Const(c) => vals.push(sa.constant(r.constant(c)?, m)?),
                Kind::Var => {
                    if req.inf != 0 {
                        // x = ±t^-den
                        vals.push(Ser { val: -req.den, c: vec![r.int(req.inf)], prec: cap });
                    } else {
                        let x0 = r.constant(req.x0)?;
                        let mut c = vec![r.int(0); req.den as usize + 1];
                        c[0] = x0;
                        c[req.den as usize] = r.int(if req.reflect { -1 } else { 1 });
                        vals.push(sa.make(0, c, cap, m)?);
                    }
                }
                Kind::Add(xs) => {
                    tasks.push(Task::Add(xs.len()));
                    tasks.extend(xs.iter().rev().map(|&x| Task::Visit(x)));
                }
                Kind::Mul(xs) => {
                    tasks.push(Task::Mul(xs.len()));
                    tasks.extend(xs.iter().rev().map(|&x| Task::Visit(x)));
                }
                Kind::Pow(b, x) => {
                    if dep[&x] {
                        // b^x = exp(x log b)
                        tasks.push(Task::PowGeneral);
                        tasks.push(Task::Visit(x));
                        tasks.push(Task::Visit(b));
                        continue;
                    }
                    match env.ar.num(x).and_then(|n| n.as_rat().cloned()) {
                        Some(q) if q.is_int() => match q.num.to_i64() {
                            Some(n) if n.unsigned_abs() < 1 << 20 => tasks.push(Task::PowInt(n)),
                            _ => return Err(SErr::Unsupported),
                        },
                        Some(q) => tasks.push(Task::PowRat(q)),
                        None => {
                            // a symbolic constant exponent: exp(x log b)
                            tasks.push(Task::PowGeneral);
                            tasks.push(Task::Visit(x));
                        }
                    }
                    tasks.push(Task::Visit(b));
                }
                Kind::Func("", _) => return Err(SErr::Unsupported),
                Kind::Taylor(Taylor { subst: Some(sub), .. }) => tasks.push(Task::Visit(*sub)),
                Kind::Taylor(tay) => {
                    tasks.push(Task::Taylor(taylors.len()));
                    taylors.push(tay);
                    tasks.push(Task::Visit(tay.arg));
                }
                Kind::Func(f, a) => {
                    tasks.push(Task::Func(f));
                    tasks.push(Task::Visit(a));
                }
                Kind::Data(t) => vals.push(read_data(env, r, &sa, t, m)?),
                Kind::Order(p) => vals.push(sa.make(p, vec![], p, m)?),
            },
            Task::Add(n) => {
                let xs = vals.split_off(vals.len() - n);
                let mut acc = xs[0].clone();
                for x in &xs[1..] {
                    acc = sa.add(&acc, x, m)?;
                }
                vals.push(acc);
            }
            Task::Mul(n) => {
                let xs = vals.split_off(vals.len() - n);
                let mut acc = xs[0].clone();
                for x in &xs[1..] {
                    acc = sa.mul(&acc, x, m)?;
                }
                vals.push(acc);
            }
            Task::PowInt(n) => {
                // invariant: every power task follows the visit of its base, which pushed one value
                let b = vals.pop().expect("a base");
                vals.push(sa.pow_int(&b, n, m)?);
            }
            Task::PowRat(q) => {
                // invariant: every power task follows the visit of its base, which pushed one value
                let b = vals.pop().expect("a base");
                vals.push(sa.pow_rat(&b, &q, m)?);
            }
            Task::PowGeneral => {
                // invariant: the exponent's visit pushed one value
                let x = vals.pop().expect("an exponent");
                // invariant: the base's visit pushed one value
                let b = vals.pop().expect("a base");
                let l = sa.log(&b, m)?;
                vals.push(sa.exp(&sa.mul(&x, &l, m)?, m)?);
            }
            Task::Taylor(i) => {
                // invariant: the argument's visit pushed one value
                let a = vals.pop().expect("an argument");
                let tay = taylors[i];
                let d = sa.add(&a, &sa.neg(&sa.constant(r.constant(tay.u0)?, m)?), m)?;
                if d.val < 1 || tay.c.is_empty() {
                    return Err(SErr::Unsupported);
                }
                let n = tay.c.len();
                // Horner: c_0 + d (c_1 + d (c_2 + ...)), known modulo d^n
                let mut acc = sa.constant(r.constant(tay.c[n - 1])?, m)?;
                for k in (0..n - 1).rev() {
                    acc = sa.add(&sa.mul(&acc, &d, m)?, &sa.constant(r.constant(tay.c[k])?, m)?, m)?;
                }
                let prec = acc.prec.min(n as i64 * d.val);
                vals.push(sa.make(acc.val, acc.c, prec, m)?);
            }
            Task::Func(f) => {
                // invariant: every function task follows the visit of its argument
                let a = vals.pop().expect("an argument");
                let v = match f {
                    "exp" => sa.exp(&a, m)?,
                    "log" => sa.log(&a, m)?,
                    "sin" => sa.sincos(&a, false, m)?.0,
                    "cos" => sa.sincos(&a, false, m)?.1,
                    "sinh" => sa.sincos(&a, true, m)?.0,
                    "cosh" => sa.sincos(&a, true, m)?.1,
                    "tan" | "tanh" => {
                        let (s, c) = sa.sincos(&a, f == "tanh", m)?;
                        sa.mul(&s, &sa.inv(&c, m)?, m)?
                    }
                    "atan" => sa.atan(&a, m)?,
                    "sqrt" => sa.pow_rat(&a, &Rat::new(Int::from_i64(1), Int::from_i64(2)), m)?,
                    // invariant: FUNCS lists exactly the functions matched here
                    _ => unreachable!("FUNCS lists the functions"),
                };
                vals.push(v);
            }
        }
    }
    // invariant: the tasks of one term leave exactly one value
    let s = vals.pop().expect("one value");
    Ok(s)
}

/// A `series_data(x, x0, [c...], v, p)` term as a series.
fn read_data<'h, R: Ring<'h>>(env: &Env<'h>, r: &R, sa: &Arithm<'_, R>, t: T<'h>, m: &mut Meter) -> SRes<Ser<R::C>> {
    let View::App { args, .. } = env.h().view(t) else {
        return Err(SErr::Unsupported);
    };
    let (Some(cs), Some(v), Some(p)) = (env.as_list(args[2]), env.int(args[3]), env.int(args[4])) else {
        return Err(SErr::Unsupported);
    };
    let c = cs.iter().map(|&c| r.constant(c)).collect::<SRes<Vec<R::C>>>()?;
    sa.make(v, c, p, m)
}

/// The most ramification a Puiseux series may need (x - x0 = t^den).
const MAX_DEN: i64 = 48;

/// `series(e, x, x0, n)`: the expansion of e in powers of (x - x0) up to and including the power n. `x0` may be
/// `inf` or `-inf` (powers of 1/x); fractional powers (Puiseux series) and powers of `log(x - x0)` in the
/// coefficients (logarithmic series) are found when needed (decision K.14). Such results, which `series_data` cannot
/// hold, are the sum with an order term `O(...)`.
pub(crate) fn series_native<'h>(env: &Env<'h>, args: &[T<'h>], taylor: &TaylorTable<'h>, m: &mut Meter) -> WorkResult<Out<'h>> {
    // the specification lists [x, x0, n]: one is series(e, x, x0, n); several, the iterated series in several
    // variables, are the library's (lib/calculus.mc)
    let spec3 = |t: &T<'h>| env.as_list(*t).is_some_and(|l| l.len() == 3);
    if args.len() >= 2 && args[1..].iter().all(spec3) {
        if let [e, spec] = args {
            let l = env.as_list(*spec).unwrap_or(&[]);
            let h = env.h();
            return Ok(Out::Again(h.call(h.sym("series"), &[*e, l[0], l[1], l[2]])));
        }
        return Ok(Out::Decline);
    }
    if let [e, spec] = args {
        return leading_series(env, *e, *spec, taylor, m);
    }
    let [e, x, x0, n] = args else {
        return Ok(env.arg_failure("series(e, x, x0, n) takes four arguments"));
    };
    if let Some(exact) = exact_machine(env, *e) {
        // machine numbers: the series of their exact binary values, each coefficient rounded once at the end
        // (so 2.5^5/120 is the double nearest to it, not the result of four rounded products; item 13C)
        if let Out::Done(t) | Out::Again(t) = series_native(env, &[exact, *x, *x0, *n], taylor, m)? {
            if let Some(r) = to_machine(env, t) {
                return Ok(Out::Again(r));
            }
        }
    }
    if let Some(items) = env.as_list(*e) {
        // a list: the series of each element (item 13C)
        let mut out = Vec::with_capacity(items.len());
        for &it in items {
            match series_native(env, &[it, *x, *x0, *n], taylor, m)? {
                Out::Done(t) | Out::Again(t) => out.push(t),
                Out::Decline => return Ok(Out::Decline),
            }
        }
        return Ok(Out::Again(env.list(&out)));
    }
    if !matches!(env.h().view(*x), View::Sym(_)) {
        return Ok(env.arg_failure("the variable must be a symbol"));
    }
    let Some(n) = env.int(*n).filter(|n| n.unsigned_abs() < 1 << 24) else {
        return Ok(env.arg_failure("the order must be a machine integer"));
    };
    let inf_t = env.sym("inf");
    let inf = if *x0 == inf_t {
        1
    } else if *x0 == env.ar.neg(inf_t) {
        -1
    } else {
        0
    };
    // sec, csc, cot and their hyperbolic forms as quotients of the kernels' functions (their poles are Laurent
    // series of those quotients; decision IOSYM.13)
    let e = &reciprocals(env, *e, *x, 0).unwrap_or(*e);
    let mut den = 1i64;
    let reflect = false;
    loop {
        let req = Request { x: *x, x0: *x0, order_x: n + 1, den, inf, taylor, logx: None, reflect };
        match attempt_raw(env, &req, *e, m) {
            Err(SErr::Ramify(d)) if d > 1 && den * d <= MAX_DEN => den *= d,
            Ok((cs, val, prec, again)) => {
                // a Puiseux series at a real point with complex coefficients: the function is real on the other
                // side (asin at 1), whose real expansion in powers of (x0 - x) is the one given (item 13C)
                let side = taylor.get(x).filter(|t| t.subst.is_none() && t.c.is_empty()).and_then(|t| env.int(t.u0));
                if den == 2 && side == Some(-1) {
                    // x < x0 assumed: the expansion from below, written in powers of (x - x0)^(1/2) with
                    // (x0 - x)^(k/2) = (-i)^k (x - x0)^(k/2) (principal values)
                    let r2 = Request { x: *x, x0: *x0, order_x: n + 1, den, inf, taylor, logx: None, reflect: true };
                    if let Ok((cs2, v2, p2, _)) = attempt_raw(env, &r2, *e, m) {
                        let h = env.h();
                        let mi = env.ar.neg(env.ar.number(&Num::i()));
                        let cs3: Vec<T<'h>> = cs2.iter().enumerate().map(|(i, &c)| env.ar.mul(&[c, env.ar.pow(mi, h.int(v2 + i as i64))])).collect();
                        return Ok(result_term(env, &req, cs3, v2, p2, true));
                    }
                }
                if den > 1 && inf == 0 && !reflect && side != Some(1) && cs.iter().any(|&c| complex(env.h(), c)) && is_real_point(env, *x0) {
                    let saved = (cs, val, prec, again);
                    let mut d2 = 1i64;
                    let r = loop {
                        let r2 = Request { x: *x, x0: *x0, order_x: n + 1, den: d2, inf, taylor, logx: None, reflect: true };
                        match attempt_raw(env, &r2, *e, m) {
                            Err(SErr::Ramify(d)) if d > 1 && d2 * d <= MAX_DEN => d2 *= d,
                            other => break (other, d2),
                        }
                    };
                    if let (Ok((cs2, v2, p2, ag2)), d2) = r {
                        if !cs2.iter().any(|&c| complex(env.h(), c)) {
                            let r2 = Request { x: *x, x0: *x0, order_x: n + 1, den: d2, inf, taylor, logx: None, reflect: true };
                            return Ok(result_term(env, &r2, cs2, v2, p2, ag2));
                        }
                    }
                    let (cs, val, prec, again) = saved;
                    let r1 = Request { x: *x, x0: *x0, order_x: n + 1, den: req.den, inf, taylor, logx: None, reflect: false };
                    return Ok(result_term(env, &r1, cs, val, prec, again));
                }
                return Ok(result_term(env, &req, cs, val, prec, again));
            }
            Err(err) => return finish_error(env, err),
        }
    }
}

/// `t` with its machine reals replaced by their exact values, when it has some and no other inexact numbers.
fn exact_machine<'h>(env: &Env<'h>, t: T<'h>) -> Option<T<'h>> {
    let h = env.h();
    let mut found = false;
    let mut memo: FxHashMap<T<'h>, T<'h>> = FxHashMap::default();
    fn walk<'h>(env: &Env<'h>, t: T<'h>, found: &mut bool, memo: &mut FxHashMap<T<'h>, T<'h>>) -> Option<T<'h>> {
        let h = env.h();
        if let Some(&r) = memo.get(&t) {
            return Some(r);
        }
        let r = match h.view(t) {
            View::Real(v) => {
                let (neg, man, e) = mc_arith::num::float::f64_to_dyadic(v)?;
                *found = true;
                let mut num = Int::from_nat(man);
                if neg {
                    num = num.neg();
                }
                let q = if e >= 0 { Rat::from_int(num.shl(e as u64)) } else { Rat::new(num, Int::from_nat(Nat::pow2((-e) as u64))) };
                env.ar.number(&Num::rat(q))
            }
            View::Ball { .. } => return None,
            View::App { head, args } => {
                let mut na = Vec::with_capacity(args.len());
                for &a in args {
                    na.push(walk(env, a, found, memo)?);
                }
                if head == env.hd.add {
                    env.ar.add(&na)
                } else if head == env.hd.mul {
                    env.ar.mul(&na)
                } else if head == env.hd.pow && na.len() == 2 {
                    env.ar.pow(na[0], na[1])
                } else {
                    h.app(head, &na)
                }
            }
            _ => t,
        };
        memo.insert(t, r);
        Some(r)
    }
    let r = walk(env, t, &mut found, &mut memo)?;
    let _ = h;
    found.then_some(r)
}

/// A series result with every exact rational coefficient as the nearest machine real (zeros dropped); `None` when a
/// coefficient is not a rational number.
fn to_machine<'h>(env: &Env<'h>, t: T<'h>) -> Option<T<'h>> {
    let h = env.h();
    let conv = |c: T<'h>| -> Option<T<'h>> {
        let q = env.ar.num(c)?.as_rat()?.clone();
        Some(if q.is_zero() { h.int(0) } else { env.ar.number(&Num::Real(mc_arith::number::Real::Machine(mc_arith::number::Real::Exact(q).to_f64()))) })
    };
    if let Some((x, x0, cs, v, p)) = series_data_parts(h, t) {
        let mut out = Vec::with_capacity(cs.len());
        for &c in cs {
            out.push(conv(c)?);
        }
        return Some(h.call(h.sym("series_data"), &[x, x0, env.list(&out), h.int(v), h.int(p)]));
    }
    None
}

/// Does `t` hold a non-real number (an `im` part)?
fn complex<'h>(h: &'h Heap, t: T<'h>) -> bool {
    let ar = Arith::new(h);
    let mut stack = vec![t];
    while let Some(u) = stack.pop() {
        if let Some(Num::Complex(..)) = ar.num(u) {
            return true;
        }
        if let View::App { head, args } = h.view(u) {
            stack.push(head);
            stack.extend_from_slice(args);
        }
    }
    false
}

/// Is `x0` a real number (exact or inexact)?
fn is_real_point<'h>(env: &Env<'h>, x0: T<'h>) -> bool {
    matches!(env.ar.num(x0), Some(Num::Real(_)))
}

/// `series(e, x -> x0)`: the leading term of the expansion with the order of the next nonzero term,
/// `c (x - x0)^v + O((x - x0)^w)` (item 13C).
fn leading_series<'h>(env: &Env<'h>, e: T<'h>, spec: T<'h>, taylor: &TaylorTable<'h>, m: &mut Meter) -> WorkResult<Out<'h>> {
    let h = env.h();
    let View::App { head, args: &[x, x0] } = h.view(spec) else {
        return Ok(env.arg_failure("series(e, x, x0, n) takes four arguments"));
    };
    if head != h.sym_term(h.sym(mc_api::names::RULE)) || !matches!(h.view(x), View::Sym(_)) {
        return Ok(env.arg_failure("series(e, x -> x0) takes a rule x -> x0"));
    }
    let inf_t = env.sym("inf");
    if x0 == inf_t || x0 == env.ar.neg(inf_t) {
        return Ok(Out::Decline);
    }
    let e = reciprocals(env, e, x, 0).unwrap_or(e);
    for n in [2i64, 6, 14] {
        let mut den = 1i64;
        let found = loop {
            let req = Request { x, x0, order_x: n + 1, den, inf: 0, taylor, logx: None, reflect: false };
            match attempt_raw(env, &req, e, m) {
                Err(SErr::Ramify(d)) if d > 1 && den * d <= MAX_DEN => den *= d,
                Ok(r) => break Some((r, den)),
                Err(SErr::Halt(halt)) => return Err(halt),
                Err(_) => break None,
            }
        };
        let Some(((cs, val, prec, again), den)) = found else { return Ok(Out::Decline) };
        let zero = h.int(0);
        let nz: Vec<usize> = (0..cs.len()).filter(|&i| cs[i] != zero).collect();
        if nz.len() >= 2 || (nz.len() == 1 && n == 14) {
            let i = nz[0];
            let next = nz.get(1).map_or(prec, |&j| val + j as i64);
            let req = Request { x, x0, order_x: n + 1, den, inf: 0, taylor, logx: None, reflect: false };
            if den == 1 {
                let mut c = vec![zero; (next - val - i as i64) as usize];
                c[0] = cs[i];
                let t = data_term(env, &req, c, val + i as i64, next);
                return Ok(if again { Out::Again(t) } else { Out::Done(t) });
            }
            let base = env.ar.sub(x, x0);
            let power = |k: i64| env.ar.pow(base, env.ar.rational(h.int(k), h.int(den)));
            let terms = [env.ar.mul(&[cs[i], power(val + i as i64)]), h.call(h.sym(ORDER), &[power(next)])];
            return Ok(Out::Again(env.ar.add(&terms)));
        }
    }
    Ok(Out::Decline)
}

/// One expansion with the ramification of `req` (rational coefficients first, terms when needed), before the
/// result term is built: the coefficients of t^val, t^(val+1), ..., the precision, and
/// whether the coefficients are terms that want evaluating.
fn attempt_raw<'h>(env: &Env<'h>, req: &Request<'h, '_>, e: T<'h>, m: &mut Meter) -> SRes<(Vec<T<'h>>, i64, i64, bool)> {
    let q = QRing { ar: &env.ar };
    match run(env, &q, req, e, m) {
        Ok(s) => {
            let cs: Vec<T<'h>> = s.c.iter().map(|c| env.ar.number(&Num::rat(c.clone()))).collect();
            Ok((cs, s.val, s.prec, false))
        }
        Err(SErr::NeedTerms) => {
            let lt = match req.logx {
                Some(v) => env.ar.mul(&[env.ar.rational(env.h().int(1), env.h().int(req.den)), v]),
                None => log_t(env, req),
            };
            let ln = if req.logx.is_some() || req.den != 1 { None } else { Some(log_neg_t(env, req)) };
            let tr = TRing { env, logt: std::cell::Cell::new(Some(lt)), logneg: std::cell::Cell::new(ln) };
            let s = run(env, &tr, req, e, m)?;
            let mut cs = Vec::with_capacity(s.c.len());
            for c in &s.c {
                cs.push(match tr.expanded(*c, m) {
                    Ok(t) => t,
                    Err(SErr::Halt(h)) => return Err(SErr::Halt(h)),
                    Err(_) => *c,
                });
            }
            Ok((cs, s.val, s.prec, true))
        }
        Err(err) => Err(err),
    }
}

/// A truncated generalized power series at t = 0 (item 13C, for the limit code): `coeffs[i]` is the coefficient
/// of t^((val + i)/den), the series is known modulo t^(prec/den). Coefficients are evaluated terms free of t except
/// for powers of `log(t)` (a logarithmic series).
#[derive(Clone, Debug)]
pub struct Expansion<'h> {
    /// the ramification: the exponents are multiples of 1/den
    pub den: i64,
    /// the exponent of the first coefficient, times den
    pub val: i64,
    /// the coefficients (some may be 0)
    pub coeffs: Vec<T<'h>>,
    /// the precision, times den
    pub prec: i64,
}

/// The expansion of `e` in powers of the symbol `t` at 0 with every term below t^(n + 1) (Puiseux and logarithmic
/// series included, as [`series`](series_native) finds them), for Rust callers (item 13C: the leading terms
/// of Gruntz's algorithm). Subterms free of `t` are constants, whatever other symbols they hold. `derivatives`
/// allows functions the kernels do not know to be expanded through their derivatives; `log_value`, when given, is
/// the value `log(t)` takes in the coefficients (so they combine with the rest of the expansion). `None` when the expansion is
/// not found (an essential singularity, an unknown function, a vanishing denominator).
pub fn expansion<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    stop: &Arc<AtomicBool>,
    e: T<'h>,
    t: T<'h>,
    n: i64,
    derivatives: bool,
    log_value: Option<T<'h>>,
) -> mc_eval::Res<Option<Expansion<'h>>> {
    let h = cx.heap();
    if !matches!(h.view(t), View::Sym(_)) || n.unsigned_abs() >= 1 << 20 {
        return Ok(None);
    }
    let zero = h.int(0);
    let taylor = if derivatives { taylor_table(cx, &[e, t, zero, h.int(n.clamp(0, 60))])? } else { Default::default() };
    let env = Env { ar: Arith::new(h).with_stop(stop.clone()), hd: crate::convert::Heads::new(h), name: "series" };
    let _scope = mc_arith::num::StopScope::enter(Some(stop));
    let mut tick = crate::natives::Tick { cx, stop, abort: None };
    let r = {
        let mut m = Meter::new(&mut tick);
        let e = reciprocals(&env, e, t, 0).unwrap_or(e);
        let mut den = 1i64;
        loop {
            let req = Request { x: t, x0: zero, order_x: n + 1, den, inf: 0, taylor: &taylor, logx: log_value, reflect: false };
            match attempt_raw(&env, &req, e, &mut m) {
                Err(SErr::Ramify(d)) if d > 1 && den * d <= MAX_DEN => den *= d,
                Ok((coeffs, val, prec, again)) => break Ok(Some((Expansion { den, val, coeffs, prec }, again))),
                Err(SErr::Halt(halt)) => break Err(halt),
                Err(_) => break Ok(None),
            }
        }
    };
    let abort = tick.abort;
    match r {
        Ok(Some((mut x, again))) => {
            if again {
                for c in x.coeffs.iter_mut() {
                    *c = cx.eval(*c)?;
                }
            }
            Ok(Some(x))
        }
        Ok(None) => Ok(None),
        Err(Halt::Stopped) => match abort {
            Some(a) => Err(a),
            None => Ok(None),
        },
        Err(Halt::Memory) => Ok(None),
    }
}

/// The greatest common divisor of two machine integers (nonnegative).
fn gcd_i64(a: i64, b: i64) -> i64 {
    let (mut a, mut b) = (a.abs(), b.abs());
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

/// log(t) in terms of x: log(x - x0)/den, or log(±1/x)/den at an infinity.
fn log_t<'h>(env: &Env<'h>, req: &Request<'h, '_>) -> T<'h> {
    let h = env.h();
    let inner = if req.inf != 0 { env.ar.pow(env.ar.mul(&[h.int(req.inf), req.x]), h.int(-1)) } else { env.ar.sub(req.x, req.x0) };
    let l = h.call(h.sym("log"), &[inner]);
    env.ar.mul(&[env.ar.rational(h.int(1), h.int(req.den)), l])
}

/// log(-t) in terms of x: log(x0 - x)/den, or log(-(±1/x))/den at an infinity.
fn log_neg_t<'h>(env: &Env<'h>, req: &Request<'h, '_>) -> T<'h> {
    let h = env.h();
    let inner = if req.inf != 0 {
        env.ar.neg(env.ar.pow(env.ar.mul(&[h.int(req.inf), req.x]), h.int(-1)))
    } else {
        env.ar.sub(req.x0, req.x)
    };
    let l = h.call(h.sym("log"), &[inner]);
    env.ar.mul(&[env.ar.rational(h.int(1), h.int(req.den)), l])
}

/// The value of a series call: the packed `series_data` term, or (ramified, at an infinity) the sum with an order
/// term.
fn result_term<'h>(env: &Env<'h>, req: &Request<'h, '_>, cs: Vec<T<'h>>, val: i64, prec: i64, again: bool) -> Out<'h> {
    // a symbolic power of the variable (x^(m + 1) with m free) came out as a coefficient: it is a factor of every
    // term, so it belongs to the exponents and to the order term (x^(m + 1) x^k, O(x^(m + 1 + p)))
    if req.den == 1 && req.inf == 0 && !req.reflect && req.x0 == env.h().int(0) {
        match symbolic_shift(env, req.x, &cs) {
            Shift::None => {}
            Shift::Mixed => return Out::Decline,
            Shift::Common(s, rests) => {
                let h = env.h();
                let mut terms: Vec<T<'h>> = vec![];
                for (i, &c) in rests.iter().enumerate() {
                    if c != h.int(0) {
                        terms.push(env.ar.mul(&[c, env.ar.pow(req.x, env.ar.add(&[s, h.int(val + i as i64)]))]));
                    }
                }
                terms.push(h.call(h.sym(ORDER), &[env.ar.pow(req.x, env.ar.add(&[s, h.int(prec)]))]));
                return Out::Again(env.ar.add(&terms));
            }
        }
    }
    // a ramified series whose exponents are all multiples of g/den is one in t^g (cos(sqrt(x)) is a power series)
    let zero = env.h().int(0);
    let mut g = req.den;
    for (i, c) in cs.iter().enumerate() {
        if *c != zero {
            g = gcd_i64(g, val + i as i64);
        }
    }
    if g > 1 && cs.iter().any(|c| *c != zero) {
        let den = req.den / g;
        let r2 = Request { den, ..*req };
        let mut cs2 = vec![];
        let mut v2 = None;
        for (i, &c) in cs.iter().enumerate() {
            let k = val + i as i64;
            if k % g == 0 {
                v2.get_or_insert(k / g);
                cs2.push(c);
            }
        }
        let v2 = v2.unwrap_or(r2.order());
        return result_term(env, &r2, cs2, v2, r2.order(), again);
    }
    if req.den == 1 && req.inf == 0 {
        // an expansion from below in powers of (x0 - x): c (x0 - x)^k = (-1)^k c (x - x0)^k
        let cs = if req.reflect {
            cs.iter().enumerate().map(|(i, &c)| if (val + i as i64) % 2 == 0 { c } else { env.ar.neg(c) }).collect()
        } else {
            cs
        };
        let t = data_term(env, req, cs, val, prec);
        return if again { Out::Again(t) } else { Out::Done(t) };
    }
    let h = env.h();
    let base = if req.inf != 0 {
        env.ar.mul(&[h.int(req.inf), req.x])
    } else if req.reflect {
        env.ar.sub(req.x0, req.x)
    } else {
        env.ar.sub(req.x, req.x0)
    };
    let sign = if req.inf != 0 { -1 } else { 1 };
    let power = |k: i64| env.ar.pow(base, env.ar.rational(h.int(sign * k), h.int(req.den)));
    let mut terms: Vec<T<'h>> = cs.iter().enumerate().map(|(i, &c)| env.ar.mul(&[c, power(val + i as i64)])).collect();
    terms.push(h.call(h.sym(ORDER), &[power(prec)]));
    Out::Again(env.ar.add(&terms))
}

/// The symbolic powers of the series variable among the coefficients.
enum Shift<'h> {
    /// none
    None,
    /// some terms have one and some do not, or the exponents differ
    Mixed,
    /// every term has x^s: s, and the coefficients without it
    Common(T<'h>, Vec<T<'h>>),
}

/// Split `x^s` (s free of x and not a number) off every term of every coefficient.
fn symbolic_shift<'h>(env: &Env<'h>, x: T<'h>, cs: &[T<'h>]) -> Shift<'h> {
    let h = env.h();
    let zero = h.int(0);
    let mut shift: Option<T<'h>> = None;
    let (mut with, mut without) = (0usize, 0usize);
    let mut rests = Vec::with_capacity(cs.len());
    for &c in cs {
        if c == zero {
            rests.push(c);
            continue;
        }
        let addends: Vec<T<'h>> = match h.view(c) {
            View::App { head, args } if head == env.hd.add => args.to_vec(),
            _ => vec![c],
        };
        let mut acc = vec![];
        for a in addends {
            let factors: Vec<T<'h>> = match h.view(a) {
                View::App { head, args } if head == env.hd.mul => args.to_vec(),
                _ => vec![a],
            };
            let mut s = None;
            let mut rest = vec![];
            for f in factors {
                if let View::App { head, args: &[b, e] } = h.view(f) {
                    if head == env.hd.pow && b == x && env.ar.num(e).is_none() && !mentions(h, e, x) {
                        s = Some(e);
                        continue;
                    }
                }
                rest.push(f);
            }
            match s {
                Some(e) => {
                    with += 1;
                    if shift.is_some_and(|p| p != e) {
                        return Shift::Mixed;
                    }
                    shift = Some(e);
                }
                None => without += 1,
            }
            acc.push(if rest.is_empty() { h.int(1) } else { env.ar.mul(&rest) });
        }
        rests.push(env.ar.add(&acc));
    }
    match (shift, with, without) {
        (None, _, _) => Shift::None,
        (Some(s), _, 0) => Shift::Common(s, rests),
        _ => Shift::Mixed,
    }
}

fn finish_error<'h>(env: &Env<'h>, err: SErr) -> WorkResult<Out<'h>> {
    match err {
        SErr::Halt(h) => Err(h),
        SErr::Unsupported | SErr::NeedTerms | SErr::Ramify(_) => Ok(Out::Decline),
        SErr::Zero => Ok(env.arg_failure("division by a series that vanishes to the working precision")),
    }
}

/// Evaluate with growing working precision until the result reaches the requested order.
fn run<'h, R: Ring<'h>>(env: &Env<'h>, r: &R, req: &Request<'h, '_>, e: T<'h>, m: &mut Meter) -> SRes<Ser<R::C>> {
    let mut extra = 2i64 * req.den;
    let mut last: Option<i64> = None;
    let want = req.order();
    for _ in 0..12 {
        // functions of the subterms need a positive working precision even for a negative order (13C)
        let cap = (want + extra).max(extra.max(2));
        match evaluate(env, r, req, e, cap, m) {
            Ok(mut s) => {
                // enough, or no better than the last attempt (the input itself is truncated: a series_data term)
                if s.prec >= want || last.is_some_and(|p| s.prec <= p) {
                    let order = s.prec.min(want);
                    let keep = (order - s.val).max(0) as usize;
                    s.c.truncate(keep);
                    s.prec = order;
                    if s.c.is_empty() {
                        s.val = order;
                    }
                    return Ok(s);
                }
                last = Some(s.prec);
                extra += want - s.prec + 2;
            }
            // a divisor that vanishes identically (a step function that is 0 on the side expanded) never shows a
            // term: the doublings stop at 512 extra orders instead of computing thousands of terms
            Err(SErr::Zero) if extra < 512 => extra = extra * 2 + 4,
            Err(err) => return Err(err),
        }
    }
    Err(SErr::Zero)
}

fn data_term<'h>(env: &Env<'h>, req: &Request<'h, '_>, cs: Vec<T<'h>>, val: i64, prec: i64) -> T<'h> {
    let h = env.h();
    // an inexact zero coefficient is a zero term (sin(2.5 x) has no 0.0 x^2 term; item 13C)
    let cs: Vec<T<'h>> = cs.into_iter().map(|c| if env.ar.num(c).is_some_and(|n| n.is_zero()) { h.int(0) } else { c }).collect();
    let l = env.list(&cs);
    h.call(h.sym("series_data"), &[req.x, req.x0, l, h.int(val), h.int(prec)])
}

/// `normal(e)`: a `series_data` term as its sum, a sum with order terms `O(...)` without them (other terms unchanged;
/// lists element by element).
pub(crate) fn normal_native<'h>(env: &Env<'h>, args: &[T<'h>], m: &mut Meter) -> WorkResult<Out<'h>> {
    let h = env.h();
    // normal(e, heads): only the heads named (a symbol or a list of them) are converted (decision IOSYM.7)
    let (e, only) = match args {
        [e] => (e, None),
        [e, hs] => {
            let names: Vec<String> = match env.as_list(*hs) {
                Some(l) => l.to_vec(),
                None => vec![*hs],
            }
            .into_iter()
            .filter_map(|t| match h.view(t) {
                View::Sym(s) => Some(h.sym_name(s).to_string()),
                _ => None,
            })
            .collect();
            (e, Some(names))
        }
        _ => return Ok(env.arg_failure("normal(e) takes one argument, or e and the heads to convert")),
    };
    let wanted = |name: &str| only.as_ref().is_none_or(|ns| ns.iter().any(|n| n == name));
    let (series_ok, assoc_ok) = (wanted("series") || wanted("series_data"), wanted("assoc"));
    let one = |t: T<'h>, m: &mut Meter| -> WorkResult<T<'h>> {
        let View::App { head, args } = h.view(t) else {
            return Ok(t);
        };
        // an association: the list of its rules
        if assoc_ok && head == env.sym("assoc") {
            return Ok(env.list(args));
        }
        if !series_ok {
            return Ok(t);
        }
        // a product, or a sum of products, with factors that are sums with order terms (the expansion at infinity of a
        // function with an exponential or oscillating factor kept outside): those factors without the order terms
        if head == env.hd.mul || head == env.hd.add && args.iter().any(|&a| matches!(h.view(a), View::App { head, .. } if head == env.hd.mul)) {
            let r = strip_order(env, t);
            if r != t {
                return Ok(r);
            }
        }
        // a sum with order terms (a printed series read back): the sum without them
        if head == env.hd.add && args.iter().any(|&a| matches!(h.view(a), View::App { head, .. } if head == env.sym(ORDER))) {
            let rest: Vec<T<'h>> = args.iter().copied().filter(|&a| !matches!(h.view(a), View::App { head, .. } if head == env.sym(ORDER))).collect();
            return Ok(env.ar.add(&rest));
        }
        if head != env.sym("series_data") || args.len() != 5 {
            return Ok(t);
        }
        let (Some(cs), Some(v)) = (env.as_list(args[2]), env.int(args[3])) else {
            return Ok(t);
        };
        let (x, x0) = (args[0], args[1]);
        let base = if x0 == h.int(0) { x } else { env.ar.sub(x, x0) };
        let mut terms = Vec::with_capacity(cs.len());
        for (i, &c) in cs.iter().enumerate() {
            m.step(1)?;
            let k = v + i as i64;
            let p = if k == 0 {
                h.int(1)
            } else if k == 1 {
                base
            } else {
                env.ar.pow(base, h.int(k))
            };
            terms.push(env.ar.mul(&[c, p]));
        }
        Ok(env.ar.add(&terms))
    };
    match env.as_list(*e) {
        Some(xs) => {
            let mut out = Vec::with_capacity(xs.len());
            for &x in xs {
                out.push(one(x, m)?);
            }
            Ok(Out::Done(env.list(&out)))
        }
        None => Ok(Out::Done(one(*e, m)?)),
    }
}

/// `t` with the order terms `O(...)` of the sums inside its products (and sums of products) removed.
fn strip_order<'h>(env: &Env<'h>, t: T<'h>) -> T<'h> {
    let h = env.h();
    let is_o = |a: T<'h>| matches!(h.view(a), View::App { head, .. } if head == env.sym(ORDER));
    let View::App { head, args } = h.view(t) else { return t };
    if head != env.hd.add && head != env.hd.mul {
        return t;
    }
    let mut out: Vec<T<'h>> = Vec::with_capacity(args.len());
    for &a in args {
        if head == env.hd.add && is_o(a) {
            continue;
        }
        out.push(strip_order(env, a));
    }
    if out.iter().zip(args.iter()).all(|(a, b)| a == b) && out.len() == args.len() {
        return t;
    }
    if head == env.hd.add {
        env.ar.add(&out)
    } else {
        env.ar.mul(&out)
    }
}

/// Heads the series kernels expand themselves (no Taylor table entry).
fn kernel_head(name: &str) -> bool {
    FUNCS.contains(&name) || matches!(name, "add" | "mul" | "pow" | "list" | "series_data" | "O" | "complex")
}

/// Does `t` mention `x`?
fn mentions<'h>(h: &'h Heap, t: T<'h>, x: T<'h>) -> bool {
    let mut stack = vec![t];
    while let Some(u) = stack.pop() {
        if u == x {
            return true;
        }
        if let View::App { head, args } = h.view(u) {
            stack.push(head);
            stack.extend(args.iter().copied());
        }
    }
    false
}

/// Is `v` a usable coefficient: no infinity, failure, indeterminate, unresolved derivative or `d` call?
fn finite_value<'h>(h: &'h Heap, v: T<'h>) -> bool {
    let bad = ["inf", "complex_infinity", "indeterminate", "failure", "d", "nan", "limit", "series"];
    let mut stack = vec![v];
    while let Some(u) = stack.pop() {
        match h.view(u) {
            View::Sym(s) if bad.contains(&h.sym_name(s).as_str()) => return false,
            View::Real(r) if !r.is_finite() => return false,
            View::App { head, args } => {
                stack.push(head);
                stack.extend(args.iter().copied());
            }
            _ => {}
        }
    }
    true
}

/// The Taylor table of `series(e, x, x0, n)`: for every call `f(..., u, ...)` in `e` whose head the series kernels do
/// not know and in which exactly one argument `u` mentions `x`, the coefficients f^(k)(u0)/k!, k = 0..n+3, with u0
/// the value of `u` at `x0` (finite points only), by differentiating with the evaluator. Coefficients stop at the
/// first that is not a finite closed value (a pole, an unknown derivative); no entry when the value itself is not.
pub(crate) fn taylor_table<'h>(
    cx: &mut mc_eval::NativeCx<'_, '_, 'h>,
    args: &[T<'h>],
) -> mc_eval::Res<TaylorTable<'h>> {
    let mut table = TaylorTable::default();
    let h = cx.heap();
    // series(e, x -> x0): the leading term, from a table of a few orders
    let two;
    let args = match args {
        [e, spec] => match h.view(*spec) {
            View::App { args: &[x, x0], .. } => {
                two = [*e, x, x0, h.int(16)];
                &two[..]
            }
            _ => return Ok(table),
        },
        _ => args,
    };
    let [e, x, x0, n] = args else { return Ok(table) };
    let (View::Sym(_), View::Int(n)) = (h.view(*x), h.view(*n)) else { return Ok(table) };
    if !(-64..=64).contains(&n) {
        return Ok(table);
    }
    // the coefficients wanted: up to the order, plus the orders a pole of the expression lowers them by (the
    // negative powers of terms in x: bessel_i(3, 2 x)/x^7 to order -4 needs bessel_i to order 3; round 37)
    let mut extra = 0i64;
    {
        let mut stack = vec![*e];
        while let Some(t) = stack.pop() {
            let View::App { head, args: fargs } = h.view(t) else { continue };
            if let (true, &[b, k]) = (head == h.sym_term(h.sym("pow")), fargs) {
                if let View::Int(k) = h.view(k) {
                    if k < 0 && mentions(h, b, *x) {
                        extra = (extra + k.saturating_neg()).min(64);
                    }
                }
            }
            stack.extend(fargs.iter().copied());
        }
    }
    let n = n.max(0) + extra;
    if mentions(h, *x0, h.sym_term(h.sym("inf"))) {
        // at infinity no Taylor coefficients; the inverse hyperbolic functions by their logarithmic forms, which
        // hold on the principal branches everywhere
        let mut stack = vec![*e];
        let mut seen = rustc_hash::FxHashSet::default();
        while let Some(t) = stack.pop() {
            cx.tick(1)?;
            if !seen.insert(t) {
                continue;
            }
            let View::App { head, args: fargs } = h.view(t) else { continue };
            stack.extend(fargs.iter().copied());
            let View::Sym(fs) = h.view(head) else { continue };
            if let [u] = fargs {
                if mentions(h, *u, *x) {
                    if let Some(sub) = log_form(h, &h.sym_name(fs), *u) {
                        stack.push(sub);
                        table.insert(t, Taylor { arg: *u, u0: h.int(0), c: vec![], subst: Some(sub) });
                    }
                }
            }
        }
        return Ok(table);
    }
    let y = h.sym_term(h.sym("series·u"));
    let rule = |a: T<'h>, b: T<'h>| h.call(h.sym(mc_api::names::RULE), &[a, b]);
    // (term, coefficients wanted beyond the order): a singular form brings in a square root (x - x0 = t^2), so the
    // calls inside it need twice the terms
    let mut stack = vec![(*e, n + 3)];
    let mut seen = rustc_hash::FxHashSet::default();
    while let Some((t, want)) = stack.pop() {
        if !seen.insert(t) {
            continue;
        }
        let View::App { head, args: fargs } = h.view(t) else { continue };
        stack.extend(fargs.iter().map(|&a| (a, want)));
        let View::Sym(fs) = h.view(head) else { continue };
        if kernel_head(&h.sym_name(fs)) {
            continue;
        }
        let dep: Vec<usize> = (0..fargs.len()).filter(|&i| mentions(h, fargs[i], *x)).collect();
        let [i] = dep.as_slice() else { continue };
        let arg = fargs[*i];
        let u0 = cx.eval(h.call(h.sym("rewrite"), &[arg, rule(*x, *x0)]))?;
        if !finite_value(h, u0) {
            continue;
        }
        // a step function whose argument sits on a jump at x0: constant on each side, so a series only for a side
        // given by the assumptions (and a known side of the argument); otherwise no entry and no series
        if fargs.len() == 1 && on_jump(h, &h.sym_name(fs), u0) {
            if let Some(s) = arg_side(cx, *x, *x0, arg)? {
                if let Some(sub) = step_value(h, &h.sym_name(fs), arg, u0, s) {
                    stack.push((sub, want));
                    table.insert(t, Taylor { arg, u0, c: vec![], subst: Some(sub) });
                }
            }
            continue;
        }
        if let Some(sub) = singular_form(h, &h.sym_name(fs), fargs, *i, u0) {
            side_marker(cx, &mut table, *x, *x0)?;
            // the asin/acos forms take a square root of the argument
            let ramified = matches!(&*h.sym_name(fs), "asin" | "acos");
            stack.push((sub, if ramified { 2 * want } else { want }));
            table.insert(t, Taylor { arg, u0, c: vec![], subst: Some(sub) });
            continue;
        }
        let mut g_args = fargs.to_vec();
        g_args[*i] = y;
        let mut g = h.app(head, &g_args);
        let mut c = Vec::new();
        let mut fact = Int::from_i64(1);
        for k in 0..=want {
            cx.tick(1)?;
            if k > 0 {
                fact = fact.mul(&Int::from_i64(k));
                g = cx.eval(h.call(h.sym("d"), &[g, y]))?;
            }
            let v = cx.eval(h.call(h.sym("rewrite"), &[g, rule(y, u0)]))?;
            if !finite_value(h, v) {
                break;
            }
            let ar = Arith::new(h);
            let f = ar.number(&Num::rat(Rat::new(Int::from_i64(1), fact.clone())));
            c.push(cx.eval(ar.mul(&[f, v]))?);
        }
        // only a complete table: a truncated one would cut the series short
        if c.len() as i64 == want + 1 {
            table.insert(t, Taylor { arg, u0, c, subst: None });
        }
    }
    Ok(table)
}

/// Under assumptions that put `x` on one side of `x0` (`x < x0` or `x > x0`), an entry under the key `x` whose `u0`
/// is -1 or 1 (item 13C: the side a Puiseux series at a branch point is for).
fn side_marker<'h>(cx: &mut mc_eval::NativeCx<'_, '_, 'h>, table: &mut TaylorTable<'h>, x: T<'h>, x0: T<'h>) -> mc_eval::Res<()> {
    if cx.assumptions().is_empty() || table.contains_key(&x) {
        return Ok(());
    }
    let h = cx.heap();
    for (rel, side) in [("less", -1), ("greater", 1)] {
        let r = cx.eval(h.call(h.sym("refine"), &[h.call(h.sym(rel), &[x, x0])]))?;
        if cx.is_true(r) {
            table.insert(x, Taylor { arg: x, u0: h.int(side), c: vec![], subst: None });
            return Ok(());
        }
    }
    Ok(())
}

/// Is `u0` a jump of the step function `f` of one argument: an integer for `floor`, `ceiling`, `integer_part`,
/// `fractional_part`; a half-integer for `round`; 0 for `unit_step`, `heaviside_theta`, `sign`, `real_sign`?
fn on_jump<'h>(h: &'h Heap, f: &str, u0: T<'h>) -> bool {
    match f {
        "floor" | "ceiling" | "integer_part" | "fractional_part" => matches!(h.view(u0), View::Int(_)),
        "round" => matches!(h.view(u0), View::Rational { den, .. } if matches!(h.view(den), View::Int(2))),
        "unit_step" | "heaviside_theta" | "sign" | "real_sign" => matches!(h.view(u0), View::Int(0)),
        _ => false,
    }
}

/// The side of `u0 = arg(x0)` that `arg` takes for `x` on the side of `x0` the assumptions give (`x < x0` or
/// `x > x0`): the sign of the first nonzero derivative at `x0` times the side to its order; `None` when the side or
/// the sign is not known (the first six derivatives).
fn arg_side<'h>(cx: &mut mc_eval::NativeCx<'_, '_, 'h>, x: T<'h>, x0: T<'h>, arg: T<'h>) -> mc_eval::Res<Option<i64>> {
    let h = cx.heap();
    let rule = h.call(h.sym(mc_api::names::RULE), &[x, x0]);
    let mut g = arg;
    for k in 1..=6 {
        cx.tick(1)?;
        g = cx.eval(h.call(h.sym("d"), &[g, x]))?;
        let v = cx.eval(h.call(h.sym("rewrite"), &[g, rule]))?;
        let sg = cx.eval(h.call(h.sym("sign"), &[v]))?;
        let s = match h.view(sg) {
            View::Int(0) => continue,
            View::Int(s @ (1 | -1)) => s,
            _ => return Ok(None),
        };
        // an even order: the same side of u0 on both sides of x0
        if k % 2 == 0 {
            return Ok(Some(s));
        }
        if cx.assumptions().is_empty() {
            return Ok(None);
        }
        for (rel, side) in [("less", -1i64), ("greater", 1)] {
            let r = cx.eval(h.call(h.sym("refine"), &[h.call(h.sym(rel), &[x, x0])]))?;
            if cx.is_true(r) {
                return Ok(Some(s * side));
            }
        }
        return Ok(None);
    }
    Ok(None)
}

/// The value of the step function `f(u)` just to side `s` (+1 above, -1 below) of its jump at `u0`.
fn step_value<'h>(h: &'h Heap, f: &str, u: T<'h>, u0: T<'h>, s: i64) -> Option<T<'h>> {
    let ar = Arith::new(h);
    let up = s > 0;
    let int_part = |k: i64| -> i64 {
        // truncation toward zero just beside the integer k
        if k > 0 {
            if up { k } else { k - 1 }
        } else if k < 0 {
            if up { k + 1 } else { k }
        } else {
            0
        }
    };
    match (f, h.view(u0)) {
        ("floor", View::Int(k)) => Some(h.int(if up { k } else { k - 1 })),
        ("ceiling", View::Int(k)) => Some(h.int(if up { k + 1 } else { k })),
        ("integer_part", View::Int(k)) => Some(h.int(int_part(k))),
        ("fractional_part", View::Int(k)) => Some(ar.sub(u, h.int(int_part(k)))),
        ("round", View::Rational { num, .. }) => match h.view(num) {
            // u0 = num/2 with num odd: the integers beside it are (num - 1)/2 and (num + 1)/2
            View::Int(n) => Some(h.int(if up { (n + 1) / 2 } else { (n - 1) / 2 })),
            _ => None,
        },
        ("unit_step" | "heaviside_theta", _) => Some(h.int(if up { 1 } else { 0 })),
        ("sign" | "real_sign", _) => Some(h.int(s)),
        _ => None,
    }
}

/// `f(args)` near a singular point of f where the argument `args[i]` has the value `u0`, written with terms the
/// series kernels expand (item 13C), or `None`: `gamma(u)` at u0 = -k (k = 0, 1, ...) as
/// `gamma(u + k + 1) / (u (u + 1) ... (u + k))`; `acos(u)` at 1 as `2 asin(sqrt((1 - u)/2))` and at -1 as
/// `pi - 2 asin(sqrt((1 + u)/2))`; `asin(u)` at ±1 as `pi/2 - acos(u)`, `-pi/2 + acos(-u)`.
fn singular_form<'h>(h: &'h Heap, f: &str, args: &[T<'h>], i: usize, u0: T<'h>) -> Option<T<'h>> {
    let ar = Arith::new(h);
    let u = args[i];
    let call = |n: &str, a: &[T<'h>]| h.call(h.sym(n), a);
    let pi = h.sym_term(h.sym("pi"));
    let half = ar.rational(h.int(1), h.int(2));
    match (f, args.len(), h.view(u0)) {
        ("gamma", 1, View::Int(k)) if (-1000..=0).contains(&k) => {
            let k = -k;
            let den: Vec<T<'h>> = (0..=k).map(|j| ar.add(&[u, h.int(j)])).collect();
            Some(ar.mul(&[call("gamma", &[ar.add(&[u, h.int(k + 1)])]), ar.pow(ar.mul(&den), h.int(-1))]))
        }
        ("acos", 1, View::Int(1)) => {
            let r = ar.pow(ar.mul(&[half, ar.sub(h.int(1), u)]), half);
            Some(ar.mul(&[h.int(2), call("asin", &[r])]))
        }
        ("acos", 1, View::Int(-1)) => {
            let r = ar.pow(ar.mul(&[half, ar.add(&[h.int(1), u])]), half);
            Some(ar.sub(pi, ar.mul(&[h.int(2), call("asin", &[r])])))
        }
        ("asin", 1, View::Int(s @ (1 | -1))) => {
            let r = ar.pow(ar.mul(&[half, ar.sub(h.int(1), ar.mul(&[h.int(s), u]))]), half);
            // asin(s u) = pi/2 - acos(s u) = pi/2 - 2 asin(sqrt((1 - s u)/2)), and asin is odd
            let a = ar.sub(ar.mul(&[half, pi]), ar.mul(&[h.int(2), call("asin", &[r])]));
            Some(ar.mul(&[h.int(s), a]))
        }
        // the branch points of atanh and acosh: their logarithmic forms
        ("atanh" | "acosh", 1, View::Int(1 | -1)) => log_form(h, f, u),
        _ => None,
    }
}

/// The logarithmic form of an inverse hyperbolic function of `u`, equal to it on the principal branches in the whole
/// plane (DLMF section 4.37): asinh(u) = log(u + (u^2 + 1)^(1/2)), acosh(u) = log(u + (u + 1)^(1/2)
/// (u - 1)^(1/2)), atanh(u) = (log(1 + u) - log(1 - u))/2.
fn log_form<'h>(h: &'h Heap, f: &str, u: T<'h>) -> Option<T<'h>> {
    let ar = Arith::new(h);
    let half = ar.rational(h.int(1), h.int(2));
    let log = |a: T<'h>| h.call(h.sym("log"), &[a]);
    match f {
        "asinh" => Some(log(ar.add(&[u, ar.pow(ar.add(&[ar.pow(u, h.int(2)), h.int(1)]), half)]))),
        "acosh" => {
            let r = ar.mul(&[ar.pow(ar.add(&[u, h.int(1)]), half), ar.pow(ar.sub(u, h.int(1)), half)]);
            Some(log(ar.add(&[u, r])))
        }
        "atanh" => Some(ar.mul(&[half, ar.sub(log(ar.add(&[h.int(1), u])), log(ar.sub(h.int(1), u)))])),
        _ => None,
    }
}

/// The arguments of `series` without a trailing `options(derivatives -> v)`, and whether functions may be expanded
/// through their derivatives (`v` not `false`; the default).
pub(crate) fn derivatives_option<'a, 'h>(h: &'h Heap, args: &'a [T<'h>]) -> (&'a [T<'h>], bool) {
    let Some((&last, rest)) = args.split_last() else { return (args, true) };
    let View::App { head, args: rules } = h.view(last) else { return (args, true) };
    if head != h.sym_term(h.sym("options")) {
        return (args, true);
    }
    let mut on = true;
    for &r in rules {
        if let View::App { args: &[k, v], .. } = h.view(r) {
            if k == h.sym_term(h.sym("derivatives")) {
                on = v != h.sym_term(h.sym(mc_api::names::FALSE));
            }
        }
    }
    (rest, on)
}

/// The parts `(x, x0, coefficients, valuation, precision)` of a `series_data(x, x0, [c...], v, p)` term.
pub fn series_data_parts<'h>(h: &'h Heap, t: T<'h>) -> Option<(T<'h>, T<'h>, &'h [T<'h>], i64, i64)> {
    let View::App { head, args: &[x, x0, cs, v, p] } = h.view(t) else { return None };
    if head != h.sym_term(h.sym("series_data")) {
        return None;
    }
    let View::App { head: lh, args: cs } = h.view(cs) else { return None };
    if lh != h.sym_term(h.sym(mc_api::names::LIST)) {
        return None;
    }
    match (h.view(v), h.view(p)) {
        (View::Int(v), View::Int(p)) => Some((x, x0, cs, v, p)),
        _ => None,
    }
}

/// Is one of `args` a `series_data` term? A cheap test for the engine's `add`, `mul` and `pow` before
/// [`series_arith`].
pub fn any_series(h: &Heap, args: &[T<'_>]) -> bool {
    let mut sd = None;
    for &a in args {
        if let View::App { head, args: inner } = h.view(a) {
            if inner.len() == 5 {
                let s = *sd.get_or_insert_with(|| h.sym_term(h.sym("series_data")));
                if head == s {
                    return true;
                }
            }
        }
    }
    false
}

/// The most terms a series operation asks for (the inputs' precision caps the result anyway).
const MAX_ARITH_ORDER: i64 = 4096;

/// `add`, `mul` or `pow` (`op`) with a `series_data` argument (decision IOSYM.1): the series of the
/// result at the same point, to the precision the inputs determine (`series(sin(x), x, 0, 5)^2` is the series of
/// sin(x)^2 to x^6); `None` when the series are at different points or variables or the other arguments do not
/// expand there (the canonical arithmetic then keeps the call).
pub fn series_arith<'h>(cx: &mut NativeCx<'_, '_, 'h>, stop: &Arc<AtomicBool>, op: &str, args: &[T<'h>]) -> mc_eval::Res<Option<T<'h>>> {
    let h = cx.heap();
    let mut point = None;
    let mut order = 2i64;
    for &a in args {
        if let Some((x, x0, _, v, p)) = series_data_parts(h, a) {
            match point {
                None => point = Some((x, x0)),
                Some(pt) if pt == (x, x0) => {}
                Some(_) => return Ok(None),
            }
            order = order.saturating_add(p.abs()).saturating_add(v.abs());
        }
    }
    let Some((x, x0)) = point else { return Ok(None) };
    if op == mc_api::names::POW {
        if let [_, e] = args {
            if let View::Int(k) = h.view(*e) {
                order = order.saturating_mul(k.abs().max(1));
            }
        }
    }
    let n = h.int(order.min(MAX_ARITH_ORDER));
    let e = h.call(h.sym(op), args);
    let sargs = [e, x, x0, n];
    let taylor = taylor_table(cx, &sargs)?;
    let env = Env { ar: Arith::new(h).with_stop(stop.clone()), hd: crate::convert::Heads::new(h), name: "series" };
    let _scope = mc_arith::num::StopScope::enter(Some(stop));
    let mut tick = crate::natives::Tick { cx, stop, abort: None };
    let r = {
        let mut m = Meter::new(&mut tick);
        series_native(&env, &sargs, &taylor, &mut m)
    };
    let abort = tick.abort;
    match r {
        Ok(Out::Done(t)) => Ok(Some(t)),
        Ok(Out::Again(t)) => Ok(Some(cx.eval(t)?)),
        Ok(Out::Decline) => Ok(None),
        Err(Halt::Stopped) => match abort {
            Some(a) => Err(a),
            None => Ok(None),
        },
        Err(Halt::Memory) => Ok(None),
    }
}

/// `d(s, y)` of a `series_data` term `s` in x: the term by term derivative, a series to one order less when `y` is
/// x, the coefficients differentiated when `y` is another variable (decision IOSYM.1); `None` when x0 depends on
/// `y`.
pub fn series_derivative<'h>(cx: &mut NativeCx<'_, '_, 'h>, s: T<'h>, y: T<'h>) -> mc_eval::Res<Option<T<'h>>> {
    let h = cx.heap();
    let Some((x, x0, cs, v, p)) = series_data_parts(h, s) else { return Ok(None) };
    let ar = Arith::new(h);
    if y != x {
        if mentions(h, x0, y) {
            return Ok(None);
        }
        let mut out = Vec::with_capacity(cs.len());
        for &c in cs {
            out.push(cx.eval(h.call(h.sym("d"), &[c, y]))?);
        }
        return Ok(Some(normalized(cx, x, x0, out, v, p)?));
    }
    let out: Vec<T<'h>> = cs.iter().enumerate().map(|(i, &c)| ar.mul(&[h.int(v + i as i64), c])).collect();
    Ok(Some(normalized(cx, x, x0, out, v - 1, p - 1)?))
}

/// `integrate(s, x)` of a `series_data` term in x: the term by term antiderivative, a series to one order more;
/// `None` when a term is `1/(x - x0)` (its integral is a logarithm, which a series does not hold).
pub fn series_integral<'h>(cx: &mut NativeCx<'_, '_, 'h>, s: T<'h>, y: T<'h>) -> mc_eval::Res<Option<T<'h>>> {
    let h = cx.heap();
    let Some((x, x0, cs, v, p)) = series_data_parts(h, s) else { return Ok(None) };
    if y != x {
        return Ok(None);
    }
    let ar = Arith::new(h);
    let mut out = Vec::with_capacity(cs.len());
    for (i, &c) in cs.iter().enumerate() {
        let k = v + i as i64 + 1;
        if k == 0 {
            if c != h.int(0) {
                return Ok(None);
            }
            out.push(c);
            continue;
        }
        out.push(ar.mul(&[ar.rational(h.int(1), h.int(k)), c]));
    }
    Ok(Some(normalized(cx, x, x0, out, v + 1, p + 1)?))
}

/// `series_data(x, x0, cs, v, p)` with the coefficients evaluated and the leading zeros dropped.
fn normalized<'h>(cx: &mut NativeCx<'_, '_, 'h>, x: T<'h>, x0: T<'h>, cs: Vec<T<'h>>, mut v: i64, p: i64) -> mc_eval::Res<T<'h>> {
    let h = cx.heap();
    let zero = h.int(0);
    let mut out = Vec::with_capacity(cs.len());
    for c in cs {
        let c = cx.eval(c)?;
        if out.is_empty() && c == zero {
            v += 1;
            continue;
        }
        out.push(c);
    }
    let keep = (p - v).max(0) as usize;
    out.truncate(keep);
    if out.is_empty() {
        v = p;
    }
    let l = h.call(h.sym(mc_api::names::LIST), &out);
    Ok(h.call(h.sym("series_data"), &[x, x0, l, h.int(v), h.int(p)]))
}

/// `t` with `sec(u)`, `csc(u)`, `cot(u)`, `sech(u)`, `csch(u)`, `coth(u)` of terms mentioning `x` written with `sin`,
/// `cos`, `sinh`, `cosh`; `None` beyond a nesting depth.
fn reciprocals<'h>(env: &Env<'h>, t: T<'h>, x: T<'h>, depth: u32) -> Option<T<'h>> {
    let h = env.h();
    if depth > 200 {
        return None;
    }
    let View::App { head, args } = h.view(t) else { return Some(t) };
    if !mentions(h, t, x) {
        return Some(t);
    }
    let mut out = Vec::with_capacity(args.len());
    for &a in args {
        out.push(reciprocals(env, a, x, depth + 1)?);
    }
    if let (View::Sym(s), [u]) = (h.view(head), out.as_slice()) {
        let call = |f: &str| h.call(h.sym(f), &[*u]);
        let inv = |t: T<'h>| env.ar.pow(t, h.int(-1));
        let r = match h.sym_name(s).as_str() {
            "sec" => Some(inv(call("cos"))),
            "csc" => Some(inv(call("sin"))),
            "cot" => Some(env.ar.mul(&[call("cos"), inv(call("sin"))])),
            "sech" => Some(inv(call("cosh"))),
            "csch" => Some(inv(call("sinh"))),
            "coth" => Some(env.ar.mul(&[call("cosh"), inv(call("sinh"))])),
            _ => None,
        };
        if let Some(r) = r {
            return Some(r);
        }
    }
    Some(if out.as_slice() == args { t } else { h.app(head, &out) })
}
