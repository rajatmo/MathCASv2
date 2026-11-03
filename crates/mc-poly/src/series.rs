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
use rustc_hash::FxHashMap;

/// Why a series evaluation did not produce a value.
enum SErr {
    /// the coefficients need terms (retry in the term ring)
    NeedTerms,
    /// not expandable here (an unknown function of x, a branch point, an essential singularity)
    Unsupported,
    /// a division by a series that is zero to the working precision
    Zero,
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
}

impl<'h> TRing<'_, 'h> {
    fn h(&self) -> &'h Heap {
        self.env.h()
    }
    /// The expanded form (rational coefficients over the kernels), which decides zero.
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
        Ok(h.call(h.sym(f), &[*a]))
    }
    fn pow_rat(&self, a: &T<'h>, q: &Rat) -> SRes<T<'h>> {
        Ok(self.env.ar.pow(*a, self.rat(q)))
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
        if a.val != 0 || a.c.is_empty() {
            // log of t^v: not a power series
            return Err(SErr::Unsupported);
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
            return Err(SErr::Unsupported);
        }
        let shift = vq.num.to_i64().ok_or(SErr::Unsupported)?;
        let lead = a.c[0].clone();
        let li = self.r.inv(&lead)?;
        // f = a / (a_v t^v), f_0 = 1: g = f^q with k g_k = sum_j ((q + 1) j - k) f_j g_(k-j)
        let f: Vec<R::C> = a.c.iter().map(|x| self.r.mul(x, &li)).collect();
        let n = f.len();
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
enum Kind<'h> {
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
}

/// The head of an order term `O((x - x0)^p)` (decision 9.3 of the library-gaps package).
pub(crate) const ORDER: &str = "O";

const FUNCS: [&str; 10] = ["exp", "log", "sin", "cos", "tan", "sinh", "cosh", "tanh", "atan", "sqrt"];

/// The pieces of the request.
struct Request<'h> {
    x: T<'h>,
    x0: T<'h>,
    /// the absolute order wanted: terms below t^order
    order: i64,
}

/// Evaluate e as a series over ring R with working precision cap.
fn evaluate<'h, R: Ring<'h>>(
    env: &Env<'h>,
    r: &R,
    req: &Request<'h>,
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
    let kind = |t: T<'h>| -> Kind<'h> {
        if let View::App { head, args: &[a] } = h.view(t) {
            if head == order_head {
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
        if head == env.sym("series_data") && args.len() == 5 && args[0] == req.x && args[1] == req.x0 {
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
    }
    let mut tasks = vec![Task::Visit(e)];
    let mut vals: Vec<Ser<R::C>> = vec![];
    while let Some(task) = tasks.pop() {
        m.step(1)?;
        match task {
            Task::Visit(t) => match kind(t) {
                Kind::Const(c) => vals.push(sa.constant(r.constant(c)?, m)?),
                Kind::Var => {
                    let x0 = r.constant(req.x0)?;
                    vals.push(sa.make(0, vec![x0, r.int(1)], cap, m)?);
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
                let b = vals.pop().expect("a base");
                vals.push(sa.pow_int(&b, n, m)?);
            }
            Task::PowRat(q) => {
                let b = vals.pop().expect("a base");
                vals.push(sa.pow_rat(&b, &q, m)?);
            }
            Task::PowGeneral => {
                let x = vals.pop().expect("an exponent");
                let b = vals.pop().expect("a base");
                let l = sa.log(&b, m)?;
                vals.push(sa.exp(&sa.mul(&x, &l, m)?, m)?);
            }
            Task::Func(f) => {
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
                    _ => unreachable!("FUNCS lists the functions"),
                };
                vals.push(v);
            }
        }
    }
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

/// `series(e, x, x0, n)`: the expansion of e in powers of (x - x0) up to and including the power n.
pub(crate) fn series_native<'h>(env: &Env<'h>, args: &[T<'h>], m: &mut Meter) -> WorkResult<Out<'h>> {
    let [e, x, x0, n] = args else {
        return Ok(env.arg_failure("series(e, x, x0, n) takes four arguments"));
    };
    if !matches!(env.h().view(*x), View::Sym(_)) {
        return Ok(env.arg_failure("the variable must be a symbol"));
    }
    let Some(n) = env.int(*n).filter(|n| n.unsigned_abs() < 1 << 24) else {
        return Ok(env.arg_failure("the order must be a machine integer"));
    };
    let req = Request { x: *x, x0: *x0, order: n + 1 };
    let q = QRing { ar: &env.ar };
    match run(env, &q, &req, *e, m) {
        Ok(s) => {
            let cs: Vec<T<'h>> = s.c.iter().map(|c| env.ar.number(&Num::rat(c.clone()))).collect();
            Ok(Out::Done(data_term(env, &req, cs, s.val, s.prec)))
        }
        Err(SErr::NeedTerms) => {
            let tr = TRing { env };
            match run(env, &tr, &req, *e, m) {
                Ok(s) => {
                    let mut cs = Vec::with_capacity(s.c.len());
                    for c in &s.c {
                        cs.push(match tr.expanded(*c, m) {
                            Ok(t) => t,
                            Err(SErr::Halt(h)) => return Err(h),
                            Err(_) => *c,
                        });
                    }
                    Ok(Out::Again(data_term(env, &req, cs, s.val, s.prec)))
                }
                Err(err) => finish_error(env, err),
            }
        }
        Err(err) => finish_error(env, err),
    }
}

fn finish_error<'h>(env: &Env<'h>, err: SErr) -> WorkResult<Out<'h>> {
    match err {
        SErr::Halt(h) => Err(h),
        SErr::Unsupported | SErr::NeedTerms => Ok(Out::Decline),
        SErr::Zero => Ok(env.arg_failure("division by a series that vanishes to the working precision")),
    }
}

/// Evaluate with growing working precision until the result reaches the requested order.
fn run<'h, R: Ring<'h>>(env: &Env<'h>, r: &R, req: &Request<'h>, e: T<'h>, m: &mut Meter) -> SRes<Ser<R::C>> {
    let mut extra = 2i64;
    let mut last: Option<i64> = None;
    for _ in 0..6 {
        let cap = req.order + extra;
        match evaluate(env, r, req, e, cap, m) {
            Ok(mut s) => {
                // enough, or no better than the last attempt (the input itself is truncated: a series_data term)
                if s.prec >= req.order || last.is_some_and(|p| s.prec <= p) {
                    let order = s.prec.min(req.order);
                    let keep = (order - s.val).max(0) as usize;
                    s.c.truncate(keep);
                    s.prec = order;
                    if s.c.is_empty() {
                        s.val = order;
                    }
                    return Ok(s);
                }
                last = Some(s.prec);
                extra += req.order - s.prec + 2;
            }
            Err(SErr::Zero) => extra = extra * 2 + 4,
            Err(err) => return Err(err),
        }
    }
    Err(SErr::Zero)
}

fn data_term<'h>(env: &Env<'h>, req: &Request<'h>, cs: Vec<T<'h>>, val: i64, prec: i64) -> T<'h> {
    let h = env.h();
    let l = env.list(&cs);
    h.call(h.sym("series_data"), &[req.x, req.x0, l, h.int(val), h.int(prec)])
}

/// `normal(e)`: a `series_data` term as its sum, a sum with order terms `O(...)` without them (other terms unchanged;
/// lists element by element).
pub(crate) fn normal_native<'h>(env: &Env<'h>, args: &[T<'h>], m: &mut Meter) -> WorkResult<Out<'h>> {
    let [e] = args else {
        return Ok(env.arg_failure("normal(e) takes one argument"));
    };
    let h = env.h();
    let one = |t: T<'h>, m: &mut Meter| -> WorkResult<T<'h>> {
        let View::App { head, args } = h.view(t) else {
            return Ok(t);
        };
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
