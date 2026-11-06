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
        Ok(h.call(h.sym(f), &[*a]))
    }
    fn pow_rat(&self, a: &T<'h>, q: &Rat) -> SRes<T<'h>> {
        Ok(self.env.ar.pow(*a, self.rat(q)))
    }
    fn log_t(&self) -> SRes<T<'h>> {
        self.logt.get().ok_or(SErr::Unsupported)
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
    fn split(&self, a: &Ser<R::C>) -> SRes<(R::C, Ser<R::C>)> {
        if a.val < 0 {
            return Err(SErr::Unsupported);
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
        let (c0, u) = self.split(a)?;
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
        if a.val != 0 {
            // log(t^v b) = v log(t) + log(b): log(t) is a coefficient (a logarithmic series)
            let lt = self.r.log_t()?;
            let b = Ser { val: 0, c: a.c.clone(), prec: a.prec - a.val };
            let lb = self.log(&b, m)?;
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
        let (c0, u) = self.split(a)?;
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
        let (c0, _) = self.split(a)?;
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
}

/// The Taylor table of one series call: the function terms the series kernels do not know, with coefficients.
pub(crate) type TaylorTable<'h> = FxHashMap<T<'h>, Taylor<'h>>;

/// The head of an order term `O((x - x0)^p)` (decision 9.3 of the library-gaps package).
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
    let sa = Arithm { r, cap };
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
                        c[req.den as usize] = r.int(1);
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
    let [e, x, x0, n] = args else {
        return Ok(env.arg_failure("series(e, x, x0, n) takes four arguments"));
    };
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
    loop {
        let req = Request { x: *x, x0: *x0, order_x: n + 1, den, inf, taylor };
        match attempt(env, &req, *e, m) {
            Err(SErr::Ramify(d)) if d > 1 && den * d <= MAX_DEN => den *= d,
            Ok(out) => return Ok(out),
            Err(err) => return finish_error(env, err),
        }
    }
}

/// One expansion with the ramification of `req`: rational coefficients first, terms when needed.
fn attempt<'h>(env: &Env<'h>, req: &Request<'h, '_>, e: T<'h>, m: &mut Meter) -> SRes<Out<'h>> {
    let q = QRing { ar: &env.ar };
    match run(env, &q, req, e, m) {
        Ok(s) => {
            let cs: Vec<T<'h>> = s.c.iter().map(|c| env.ar.number(&Num::rat(c.clone()))).collect();
            Ok(result_term(env, req, cs, s.val, s.prec, false))
        }
        Err(SErr::NeedTerms) => {
            let tr = TRing { env, logt: std::cell::Cell::new(Some(log_t(env, req))) };
            let s = run(env, &tr, req, e, m)?;
            let mut cs = Vec::with_capacity(s.c.len());
            for c in &s.c {
                cs.push(match tr.expanded(*c, m) {
                    Ok(t) => t,
                    Err(SErr::Halt(h)) => return Err(SErr::Halt(h)),
                    Err(_) => *c,
                });
            }
            Ok(result_term(env, req, cs, s.val, s.prec, true))
        }
        Err(err) => Err(err),
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

/// The value of a series call: the packed `series_data` term, or (ramified, at an infinity) the sum with an order
/// term.
fn result_term<'h>(env: &Env<'h>, req: &Request<'h, '_>, cs: Vec<T<'h>>, val: i64, prec: i64, again: bool) -> Out<'h> {
    // a ramified series whose exponents are all multiples of g/den is one in t^g (cos(sqrt(x)) is a power series)
    let zero = env.h().int(0);
    let mut g = req.den;
    for (i, c) in cs.iter().enumerate() {
        if *c != zero {
            g = gcd_i64(g, val + i as i64);
        }
    }
    if g > 1 {
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
        let t = data_term(env, req, cs, val, prec);
        return if again { Out::Again(t) } else { Out::Done(t) };
    }
    let h = env.h();
    let base = if req.inf != 0 { env.ar.mul(&[h.int(req.inf), req.x]) } else { env.ar.sub(req.x, req.x0) };
    let sign = if req.inf != 0 { -1 } else { 1 };
    let power = |k: i64| env.ar.pow(base, env.ar.rational(h.int(sign * k), h.int(req.den)));
    let mut terms: Vec<T<'h>> = cs.iter().enumerate().map(|(i, &c)| env.ar.mul(&[c, power(val + i as i64)])).collect();
    terms.push(h.call(h.sym(ORDER), &[power(prec)]));
    Out::Again(env.ar.add(&terms))
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
    for _ in 0..6 {
        let cap = want + extra;
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
            Err(SErr::Zero) => extra = extra * 2 + 4,
            Err(err) => return Err(err),
        }
    }
    Err(SErr::Zero)
}

fn data_term<'h>(env: &Env<'h>, req: &Request<'h, '_>, cs: Vec<T<'h>>, val: i64, prec: i64) -> T<'h> {
    let h = env.h();
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
    let [e, x, x0, n] = args else { return Ok(table) };
    let (View::Sym(_), View::Int(n)) = (h.view(*x), h.view(*n)) else { return Ok(table) };
    if !(0..=64).contains(&n) || mentions(h, *x0, h.sym_term(h.sym("inf"))) {
        return Ok(table);
    }
    let y = h.sym_term(h.sym("series·u"));
    let rule = |a: T<'h>, b: T<'h>| h.call(h.sym(mc_api::names::RULE), &[a, b]);
    let mut stack = vec![*e];
    let mut seen = rustc_hash::FxHashSet::default();
    while let Some(t) = stack.pop() {
        if !seen.insert(t) {
            continue;
        }
        let View::App { head, args: fargs } = h.view(t) else { continue };
        stack.extend(fargs.iter().copied());
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
        let mut g_args = fargs.to_vec();
        g_args[*i] = y;
        let mut g = h.app(head, &g_args);
        let mut c = Vec::new();
        let mut fact = Int::from_i64(1);
        for k in 0..=(n + 3) {
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
        if c.len() as i64 == n + 4 {
            table.insert(t, Taylor { arg, u0, c });
        }
    }
    Ok(table)
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

/// `add`, `mul` or `pow` (`op`) with a `series_data` argument (package io-sym, decision IOSYM.1): the series of the
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
