//! Global extrema of a function of one real variable on the real line or on a closed half-line, reduced with proof
//! to the closed bounded interval method of `mc_algebra::transroot::minimize_interval`.
//!
//! - **Monotone outer functions**: for f = H(u) with H strictly increasing on the line (exp, sinh, asinh, atan,
//!   tanh, erf, sinh_integral) the minimizers of f are those of u and min f = H(min u); for a decreasing H (erfc)
//!   the maximum of u is taken.
//! - **Reflection symmetry**: when f(2c - x) - f(x) expands to 0 (c = 0, or the axis -b/(2a) of a quadratic
//!   polynomial inside f), every value is taken on x <= c, and the search is restricted to that half-line. Two
//!   minimizers that mirror each other then do not tie, and the least one is found.
//! - **Tails**: with g = f (g = -f for a maximum), a sample s of g at a point of the bounded part [a, b] bounds min g
//!   from above. A tail [b, inf) (or (-inf, a], read as x -> -x) is harmless when an enclosure of g over the whole
//!   tail lies above s, or when an enclosure of g' over the tail is >= 0 (g increases away from the bounded part,
//!   whose end is a candidate). Then min g over the domain is min g over [a, b].
//! - **Enclosures over a half-line** [T, inf] are extended intervals with exact dyadic ends: sums and products of
//!   the ends are exact, monotone functions are applied to the ends (enclosed by `mc_numeric` balls, rounded
//!   outward) with their limits at infinity, sin and cos give [-1, 1], a rational function of x beyond the Cauchy
//!   bounds of the roots of its denominator and of the numerator of its derivative is monotone, so it lies between
//!   its value at T and its limit. Two special bounds: |J_nu(t)| <= M_nu(t) = (J_nu(t)^2 + Y_nu(t)^2)^(1/2) for
//!   real nu and t > 0, and M_nu decreases on t > 0 by Nicholson's integral (DLMF 10.9.30, the kernel K_0(2 t
//!   sinh s) decreases in t), so |J_nu| <= M_nu(T) on [T, inf); Dawson's integral F has F'' = -2 F - 2 t F', so every
//!   critical point on t > 0 (where F > 0) is a strict maximum and there is exactly one, in (0, 1) since F'(0) = 1
//!   and F'(1) = 1 - 2 F(1) < 0 (F(1) = 0.5380...); hence 0 < F(t) <= F(T) on [T, inf) for T >= 1, and F is odd.
use mc_algebra::transroot::{self, Goal};
use mc_api::{names, Store, View, T};
use mc_arith::num::{Ball, Float, Mag, Nat, StopScope};
use mc_arith::Arith;
use mc_core::Heap;
use mc_eval::{NativeCx, Res};
use mc_numeric::cball::CBall;
use mc_numeric::numeval::{Graph, Names};
use std::cmp::Ordering;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// Working precision of the enclosures (bits).
const P: u64 = 128;
/// The half-widths of the bounded part tried in turn, in quarters (the tail checks are cheap, the bounded interval
/// method is not: the narrowest part that is proved to suffice is taken).
const WIDTHS: [i64; 21] = [8, 10, 12, 14, 16, 20, 24, 28, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256];
/// Sample points of g in the bounded part.
const SAMPLES: i64 = 64;

/// An end of an extended interval: exact dyadic or infinite.
#[derive(Clone, Debug)]
enum Ex {
    NegInf,
    Fin(Float),
    PosInf,
}

impl Ex {
    fn cmp(&self, o: &Ex) -> Ordering {
        match (self, o) {
            (Ex::NegInf, Ex::NegInf) | (Ex::PosInf, Ex::PosInf) => Ordering::Equal,
            (Ex::NegInf, _) | (_, Ex::PosInf) => Ordering::Less,
            (_, Ex::NegInf) | (Ex::PosInf, _) => Ordering::Greater,
            (Ex::Fin(a), Ex::Fin(b)) => a.cmp_exact(b),
        }
    }
    fn neg(&self) -> Ex {
        match self {
            Ex::NegInf => Ex::PosInf,
            Ex::PosInf => Ex::NegInf,
            Ex::Fin(a) => Ex::Fin(a.neg()),
        }
    }
    fn add(&self, o: &Ex) -> Option<Ex> {
        Some(match (self, o) {
            (Ex::NegInf, Ex::PosInf) | (Ex::PosInf, Ex::NegInf) => return None,
            (Ex::NegInf, _) | (_, Ex::NegInf) => Ex::NegInf,
            (Ex::PosInf, _) | (_, Ex::PosInf) => Ex::PosInf,
            (Ex::Fin(a), Ex::Fin(b)) => Ex::Fin(a.add_exact(b)),
        })
    }
    /// The sign: -1, 0, 1.
    fn sign(&self) -> i32 {
        match self {
            Ex::NegInf => -1,
            Ex::PosInf => 1,
            Ex::Fin(a) if a.is_zero() => 0,
            Ex::Fin(a) => {
                if a.neg {
                    -1
                } else {
                    1
                }
            }
        }
    }
    fn mul(&self, o: &Ex) -> Option<Ex> {
        match (self, o) {
            (Ex::Fin(a), Ex::Fin(b)) => Some(Ex::Fin(a.mul_exact(b))),
            _ => {
                let s = self.sign() * o.sign();
                // 0 times an infinite end: not determined by the ends
                match s {
                    0 => None,
                    1 => Some(Ex::PosInf),
                    _ => Some(Ex::NegInf),
                }
            }
        }
    }
    fn is_fin_at_least(&self, v: i64) -> bool {
        matches!(self, Ex::Fin(a) if a.cmp_exact(&Float::from_i64(v)) != Ordering::Less)
    }
}

/// An extended interval [lo, hi].
#[derive(Clone, Debug)]
struct Iv {
    lo: Ex,
    hi: Ex,
}

impl Iv {
    fn ball(b: &Ball) -> Iv {
        let (l, h) = b.ends();
        Iv { lo: Ex::Fin(l), hi: Ex::Fin(h) }
    }
    fn add(&self, o: &Iv) -> Option<Iv> {
        Some(Iv { lo: self.lo.add(&o.lo)?, hi: self.hi.add(&o.hi)? })
    }
    fn mul(&self, o: &Iv) -> Option<Iv> {
        let ps = [self.lo.mul(&o.lo)?, self.lo.mul(&o.hi)?, self.hi.mul(&o.lo)?, self.hi.mul(&o.hi)?];
        let lo = ps.iter().min_by(|a, b| a.cmp(b))?.clone();
        let hi = ps.iter().max_by(|a, b| a.cmp(b))?.clone();
        Some(Iv { lo, hi })
    }
    fn neg(&self) -> Iv {
        Iv { lo: self.hi.neg(), hi: self.lo.neg() }
    }
    fn hull(&self, o: &Iv) -> Iv {
        let lo = if self.lo.cmp(&o.lo) == Ordering::Less { self.lo.clone() } else { o.lo.clone() };
        let hi = if self.hi.cmp(&o.hi) == Ordering::Greater { self.hi.clone() } else { o.hi.clone() };
        Iv { lo, hi }
    }
}

/// The limit of a monotone function at an infinite end.
#[derive(Clone, Copy)]
enum Lim {
    NegInf,
    PosInf,
    /// num/den
    Val(i64, i64),
    /// num/den times pi
    Pi(i64, i64),
}

/// The functions of one argument that are strictly monotone on the real line (or on (0, inf) for log): name,
/// increasing, limits at -inf and at +inf.
const MONOTONE: [(&str, bool, Lim, Lim); 9] = [
    ("exp", true, Lim::Val(0, 1), Lim::PosInf),
    ("sinh", true, Lim::NegInf, Lim::PosInf),
    ("asinh", true, Lim::NegInf, Lim::PosInf),
    ("sinh_integral", true, Lim::NegInf, Lim::PosInf),
    ("atan", true, Lim::Pi(-1, 2), Lim::Pi(1, 2)),
    ("tanh", true, Lim::Val(-1, 1), Lim::Val(1, 1)),
    ("erf", true, Lim::Val(-1, 1), Lim::Val(1, 1)),
    ("erfc", false, Lim::Val(2, 1), Lim::Val(0, 1)),
    ("log", true, Lim::NegInf, Lim::PosInf),
];

fn head_name(h: &Heap, t: T<'_>) -> Option<String> {
    match h.view(t) {
        View::App { head, .. } => match h.view(head) {
            View::Sym(s) => Some(h.sym_name(s).to_string()),
            _ => None,
        },
        _ => None,
    }
}

fn args_of<'h>(h: &'h Heap, t: T<'h>) -> &'h [T<'h>] {
    match h.view(t) {
        View::App { args, .. } => args,
        _ => &[],
    }
}

fn contains(h: &Heap, t: T<'_>, x: T<'_>) -> bool {
    let mut stack = vec![t];
    while let Some(u) = stack.pop() {
        if u == x {
            return true;
        }
        if let View::App { head, args } = h.view(u) {
            stack.push(head);
            stack.extend_from_slice(args);
        }
    }
    false
}

/// Does `t` hold a symbol other than `x` and the numeric constants?
fn has_other_symbols<'h>(h: &'h Heap, t: T<'h>, x: T<'h>) -> bool {
    let mut stack = vec![t];
    while let Some(u) = stack.pop() {
        match h.view(u) {
            View::Sym(s) if u != x && !matches!(h.sym_name(s).as_str(), "pi" | "e" | "euler_gamma" | "catalan" | "golden_ratio") => return true,
            View::App { args, .. } => stack.extend_from_slice(args),
            _ => {}
        }
    }
    false
}

/// An exact rational number term as (num, den), both fitting i64.
fn small_rational(h: &Heap, t: T<'_>) -> Option<(i64, i64)> {
    match h.view(t) {
        View::Int(n) => Some((n, 1)),
        View::Rational { num, den } => match (h.view(num), h.view(den)) {
            (View::Int(n), View::Int(d)) => Some((n, d)),
            _ => None,
        },
        _ => None,
    }
}

/// Numeric enclosures of terms.
struct Encl<'a, 'h> {
    h: &'h Heap,
    ar: &'a Arith<'h, Heap>,
    nm: &'a Names,
    x: T<'h>,
}

impl<'h> Encl<'_, 'h> {
    /// The ball of a closed real numeric term.
    fn constant(&self, t: T<'h>) -> Option<Ball> {
        let g = Graph::build(self.h, self.ar, self.nm, t);
        let is = g.island(g.node(t)?);
        g.eval_ball(&is, P)
    }

    /// The ball of `t` (a term in `x`) at x in the real ball `at`.
    fn at(&self, t: T<'h>, at: &Ball) -> Option<Ball> {
        let View::Sym(xs) = self.h.view(self.x) else { return None };
        let g = Graph::build_with_vars(self.h, self.ar, self.nm, t, &[xs]);
        let is = g.island(g.node(t)?);
        let z = g.eval_complex_at(&is, P, &[CBall::real(at.clone())])?;
        z.is_real().then_some(z.re)
    }

    /// `head(pre..., x)` at the exact point `e`, as a ball.
    fn call_at(&self, head: &str, pre: &[T<'h>], e: &Float) -> Option<Ball> {
        let h = self.h;
        let mut args = pre.to_vec();
        args.push(self.x);
        self.at(h.call(h.sym(head), &args), &Ball::exact(e.clone()))
    }

    /// A limit as an interval.
    fn limit(&self, l: Lim) -> Option<Iv> {
        let h = self.h;
        Some(match l {
            Lim::NegInf => Iv { lo: Ex::NegInf, hi: Ex::NegInf },
            Lim::PosInf => Iv { lo: Ex::PosInf, hi: Ex::PosInf },
            Lim::Val(n, d) => Iv::ball(&Ball::from_ratio(n, d, P)),
            Lim::Pi(n, d) => {
                let pi = self.constant(h.sym_term(h.sym("pi")))?;
                Iv::ball(&pi.mul(&Ball::from_ratio(n, d, P), P))
            }
        })
    }

    /// A monotone function at the ends of `iv` (with its limits at infinite ends).
    fn monotone(&self, head: &str, pre: &[T<'h>], incr: bool, neg: Lim, pos: Lim, iv: &Iv) -> Option<Iv> {
        let end = |e: &Ex, up: bool| -> Option<Ex> {
            match e {
                Ex::NegInf => {
                    let l = self.limit(neg)?;
                    Some(if up { l.hi } else { l.lo })
                }
                Ex::PosInf => {
                    let l = self.limit(pos)?;
                    Some(if up { l.hi } else { l.lo })
                }
                Ex::Fin(a) => {
                    let b = self.call_at(head, pre, a)?;
                    let (l, u) = b.ends();
                    Some(Ex::Fin(if up { u } else { l }))
                }
            }
        };
        if incr {
            Some(Iv { lo: end(&iv.lo, false)?, hi: end(&iv.hi, true)? })
        } else {
            Some(Iv { lo: end(&iv.hi, false)?, hi: end(&iv.lo, true)? })
        }
    }
}

/// Enclosures over the half-line x in [t0, inf).
struct Tail<'a, 'b, 'c, 'h> {
    cx: &'a mut NativeCx<'b, 'c, 'h>,
    en: Encl<'a, 'h>,
    t0: Float,
}

impl<'h> Tail<'_, '_, '_, 'h> {
    fn eval(&mut self, t: T<'h>) -> Res<T<'h>> {
        self.cx.eval(t)
    }

    /// Is `t` a rational function of x (numbers and x-free terms as coefficients)?
    fn rational(&self, t: T<'h>) -> bool {
        let h = self.en.h;
        if !contains(h, t, self.en.x) || t == self.en.x {
            return true;
        }
        match head_name(h, t).as_deref() {
            Some("add" | "mul") => args_of(h, t).iter().all(|&a| self.rational(a)),
            Some("pow") => {
                let a = args_of(h, t);
                matches!(h.view(a[1]), View::Int(_)) && self.rational(a[0])
            }
            _ => false,
        }
    }

    /// The coefficients of a polynomial in x as balls (constant term first).
    fn coefficients(&mut self, p: T<'h>) -> Res<Option<Vec<Ball>>> {
        let h = self.en.h;
        let cl = self.eval(h.call(h.sym("coefficient_list"), &[p, self.en.x]))?;
        let Some(cs) = self.cx.as_list(cl) else { return Ok(None) };
        let mut out = vec![];
        for &c in cs {
            if contains(h, c, self.en.x) {
                return Ok(None);
            }
            match self.en.constant(c) {
                Some(b) => out.push(b),
                None => return Ok(None),
            }
        }
        Ok(Some(out))
    }

    /// Is every root of the polynomial `p` below t0 in modulus (Cauchy's bound 1 + max |a_k / a_n|)? The zero
    /// polynomial does not qualify.
    fn roots_below(&mut self, p: T<'h>) -> Res<bool> {
        let Some(cs) = self.coefficients(p)? else { return Ok(false) };
        let Some(lead) = cs.last() else { return Ok(false) };
        if lead.contains_zero() {
            return Ok(false);
        }
        let la = Ball::exact(lead.mid.abs());
        let la = Ball { mid: la.mid, rad: lead.rad };
        let mut bound = Ball::from_i64(1);
        for c in &cs[..cs.len() - 1] {
            let Some(q) = Ball { mid: c.mid.abs(), rad: c.rad }.div(&la, P) else { return Ok(false) };
            let (_, qu) = q.ends();
            let (_, bu) = bound.ends();
            if qu.cmp_exact(&bu.sub_exact(&Float::from_i64(1))) == Ordering::Greater {
                bound = Ball::exact(qu.add_exact(&Float::from_i64(1)));
            }
        }
        let (_, bu) = bound.ends();
        Ok(bu.cmp_exact(&self.t0) == Ordering::Less)
    }

    /// A rational function of x on [t0, inf): monotone there when t0 exceeds the roots of its denominator and of
    /// the numerator of its derivative, so between r(t0) and its limit.
    fn rational_bound(&mut self, r: T<'h>) -> Res<Option<Iv>> {
        let h = self.en.h;
        let x = self.en.x;
        let tg = self.eval(h.call(h.sym("together"), &[r]))?;
        let num = self.eval(h.call(h.sym("numerator"), &[tg]))?;
        let den = self.eval(h.call(h.sym("denominator"), &[tg]))?;
        let dr = self.eval(h.call(h.sym("d"), &[r, x]))?;
        let dt = self.eval(h.call(h.sym("together"), &[dr]))?;
        let dn = self.eval(h.call(h.sym("numerator"), &[dt]))?;
        if contains(h, den, x) && !self.roots_below(den)? {
            return Ok(None);
        }
        let constant = matches!(h.view(dn), View::Int(0));
        if !constant && contains(h, dn, x) && !self.roots_below(dn)? {
            return Ok(None);
        }
        let Some(v0) = self.en.at(r, &Ball::exact(self.t0.clone())) else { return Ok(None) };
        let at0 = Iv::ball(&v0);
        if constant {
            return Ok(Some(at0));
        }
        let (Some(cn), Some(cd)) = (self.coefficients(num)?, self.coefficients(den)?) else { return Ok(None) };
        let (Some(ln), Some(ld)) = (cn.last(), cd.last()) else { return Ok(None) };
        let Some(ratio) = ln.div(ld, P) else { return Ok(None) };
        let lim = match cn.len().cmp(&cd.len()) {
            Ordering::Less => Iv { lo: Ex::Fin(Float::zero()), hi: Ex::Fin(Float::zero()) },
            Ordering::Equal => Iv::ball(&ratio),
            Ordering::Greater => {
                if ratio.contains_zero() {
                    return Ok(None);
                }
                let e = if ratio.mid.neg { Ex::NegInf } else { Ex::PosInf };
                Iv { lo: e.clone(), hi: e }
            }
        };
        Ok(Some(at0.hull(&lim)))
    }

    /// An enclosure of `t` over x in [t0, inf), or `None`.
    fn bound(&mut self, t: T<'h>) -> Res<Option<Iv>> {
        self.cx.tick(1)?;
        let h = self.en.h;
        let x = self.en.x;
        if !contains(h, t, x) {
            return Ok(self.en.constant(t).map(|b| Iv::ball(&b)));
        }
        if t == x {
            return Ok(Some(Iv { lo: Ex::Fin(self.t0.clone()), hi: Ex::PosInf }));
        }
        if self.rational(t) {
            if let Some(iv) = self.rational_bound(t)? {
                return Ok(Some(iv));
            }
        }
        let name = head_name(h, t);
        let args = args_of(h, t);
        match name.as_deref() {
            Some(op @ ("add" | "mul")) => {
                // the rational parts together (their sum or product is bounded as one rational function)
                let (rat, rest): (Vec<T<'h>>, Vec<T<'h>>) = args.iter().partition(|&&a| contains(h, a, x) && self.rational(a));
                // (all parts rational: the whole term was tried as one, so the parts are bounded one by one)
                let mut parts = rest;
                if rat.len() > 1 && !parts.is_empty() {
                    parts.push(self.eval(h.call(h.sym(op), &rat))?);
                } else {
                    parts.extend(rat);
                }
                let mut acc: Option<Iv> = None;
                for p in parts {
                    let Some(b) = self.bound(p)? else { return Ok(None) };
                    acc = Some(match acc {
                        None => b,
                        Some(a) => match if op == "add" { a.add(&b) } else { a.mul(&b) } {
                            Some(v) => v,
                            None => return Ok(None),
                        },
                    });
                }
                Ok(acc)
            }
            Some("pow") => {
                let (b, e) = (args[0], args[1]);
                if !contains(h, e, x) {
                    let Some(base) = self.bound(b)? else { return Ok(None) };
                    return Ok(self.power(&base, e));
                }
                if contains(h, b, x) {
                    return Ok(None);
                }
                // b^u with a constant base: monotone in u
                let Some(bb) = self.en.constant(b) else { return Ok(None) };
                let one = Ball::from_i64(1);
                let Some(u) = self.bound(e)? else { return Ok(None) };
                let incr = match bb.cmp3(&one) {
                    Some(Ordering::Greater) => true,
                    Some(Ordering::Less) if bb.cmp3(&Ball::from_i64(0)) == Some(Ordering::Greater) => false,
                    _ => return Ok(None),
                };
                let (neg, pos) = if incr { (Lim::Val(0, 1), Lim::PosInf) } else { (Lim::PosInf, Lim::Val(0, 1)) };
                Ok(self.pow_base(b, incr, neg, pos, &u))
            }
            Some("sin" | "cos") if args.len() == 1 => {
                let Some(_) = self.bound(args[0])? else { return Ok(None) };
                Ok(Some(Iv { lo: Ex::Fin(Float::from_i64(-1)), hi: Ex::Fin(Float::from_i64(1)) }))
            }
            Some("bessel_j") if args.len() == 2 && !contains(h, args[0], x) => {
                // |J_nu(t)| <= M_nu(T) for t >= T > 0 (Nicholson's integral, DLMF 10.9.30)
                if self.en.constant(args[0]).is_none() {
                    return Ok(None);
                }
                let Some(u) = self.bound(args[1])? else { return Ok(None) };
                // |J_n(t)| <= 1 for an integer order and real t (DLMF 10.14.1, J_-n = (-1)^n J_n)
                let unit = Iv { lo: Ex::Fin(Float::from_i64(-1)), hi: Ex::Fin(Float::from_i64(1)) };
                let integer = matches!(h.view(args[0]), View::Int(_));
                let Ex::Fin(lo) = &u.lo else { return Ok(integer.then_some(unit)) };
                if lo.is_zero() || lo.neg {
                    return Ok(integer.then_some(unit));
                }
                let j = h.call(h.sym("bessel_j"), &[args[0], x]);
                let y = h.call(h.sym("bessel_y"), &[args[0], x]);
                let two = h.int(2);
                let m = h.call(h.sym("add"), &[h.call(h.sym("pow"), &[j, two]), h.call(h.sym("pow"), &[y, two])]);
                let Some(m2) = self.en.at(m, &Ball::exact(lo.clone())) else { return Ok(None) };
                let Some(m) = Ball::exact(m2.ends().1).sqrt(P) else { return Ok(None) };
                let mu = m.ends().1;
                Ok(Some(Iv { lo: Ex::Fin(mu.neg()), hi: Ex::Fin(mu) }))
            }
            Some("cosh") if args.len() == 1 => {
                // even, increasing on [0, inf), least value 1 at 0
                let Some(u) = self.bound(args[0])? else { return Ok(None) };
                if u.lo.sign() >= 0 {
                    return Ok(self.en.monotone("cosh", &[], true, Lim::PosInf, Lim::PosInf, &u));
                }
                if u.hi.sign() <= 0 {
                    return Ok(self.en.monotone("cosh", &[], false, Lim::PosInf, Lim::PosInf, &u));
                }
                let m = if u.lo.neg().cmp(&u.hi) == Ordering::Greater { u.lo.neg() } else { u.hi.clone() };
                let Some(top) = self.en.monotone("cosh", &[], true, Lim::PosInf, Lim::PosInf, &Iv { lo: m.clone(), hi: m }) else { return Ok(None) };
                Ok(Some(Iv { lo: Ex::Fin(Float::from_i64(1)), hi: top.hi }))
            }
            Some("dawson_f") if args.len() == 1 => {
                let Some(u) = self.bound(args[0])? else { return Ok(None) };
                if u.lo.is_fin_at_least(1) {
                    // decreasing and positive on [1, inf)
                    let Ex::Fin(l) = &u.lo else { return Ok(None) };
                    let Some(v) = self.en.call_at("dawson_f", &[], l) else { return Ok(None) };
                    return Ok(Some(Iv { lo: Ex::Fin(Float::zero()), hi: Ex::Fin(v.ends().1) }));
                }
                if u.hi.neg().is_fin_at_least(1) {
                    let Ex::Fin(r) = &u.hi else { return Ok(None) };
                    let Some(v) = self.en.call_at("dawson_f", &[], r) else { return Ok(None) };
                    return Ok(Some(Iv { lo: Ex::Fin(v.ends().0), hi: Ex::Fin(Float::zero()) }));
                }
                Ok(None)
            }
            Some(n) if args.len() == 1 => {
                let Some(&(_, incr, neg, pos)) = MONOTONE.iter().find(|m| m.0 == n) else { return Ok(None) };
                let Some(u) = self.bound(args[0])? else { return Ok(None) };
                if n == "log" && !matches!(&u.lo, Ex::Fin(a) if !a.is_zero() && !a.neg) {
                    return Ok(None);
                }
                Ok(self.en.monotone(n, &[], incr, neg, pos, &u))
            }
            _ => Ok(None),
        }
    }

    /// b^u for a constant base b > 0 (monotone in u).
    fn pow_base(&self, b: T<'h>, incr: bool, neg: Lim, pos: Lim, u: &Iv) -> Option<Iv> {
        let h = self.en.h;
        let x = self.en.x;
        let t = h.call(h.sym("pow"), &[b, x]);
        let end = |e: &Ex, up: bool| -> Option<Ex> {
            match e {
                Ex::NegInf | Ex::PosInf => {
                    let l = self.en.limit(if matches!(e, Ex::NegInf) { neg } else { pos })?;
                    Some(if up { l.hi } else { l.lo })
                }
                Ex::Fin(a) => {
                    let (l, hgh) = self.en.at(t, &Ball::exact(a.clone()))?.ends();
                    Some(Ex::Fin(if up { hgh } else { l }))
                }
            }
        };
        if incr {
            Some(Iv { lo: end(&u.lo, false)?, hi: end(&u.hi, true)? })
        } else {
            Some(Iv { lo: end(&u.hi, false)?, hi: end(&u.lo, true)? })
        }
    }

    /// base^e for a constant rational exponent e.
    fn power(&self, base: &Iv, e: T<'h>) -> Option<Iv> {
        let h = self.en.h;
        let (n, d) = small_rational(h, e)?;
        if n == 0 {
            return Some(Iv { lo: Ex::Fin(Float::from_i64(1)), hi: Ex::Fin(Float::from_i64(1)) });
        }
        let x = self.en.x;
        let t = h.call(h.sym("pow"), &[x, e]);
        // the power of a nonnegative end, rounded down or up
        let end = |v: &Ex, up: bool| -> Option<Ex> {
            match v {
                Ex::PosInf => Some(if n > 0 { Ex::PosInf } else { Ex::Fin(Float::zero()) }),
                Ex::NegInf => None,
                Ex::Fin(a) if a.is_zero() => (n > 0).then(|| Ex::Fin(Float::zero())),
                Ex::Fin(a) => {
                    let (l, u) = self.en.at(t, &Ball::exact(a.clone()))?.ends();
                    Some(Ex::Fin(if up { u } else { l }))
                }
            }
        };
        let nonneg = |iv: &Iv| iv.lo.sign() >= 0;
        let on_nonneg = |iv: &Iv| -> Option<Iv> {
            if n > 0 {
                Some(Iv { lo: end(&iv.lo, false)?, hi: end(&iv.hi, true)? })
            } else {
                if iv.lo.sign() <= 0 {
                    return None;
                }
                Some(Iv { lo: end(&iv.hi, false)?, hi: end(&iv.lo, true)? })
            }
        };
        if nonneg(base) {
            return on_nonneg(base);
        }
        if d != 1 {
            return None;
        }
        let even = n % 2 == 0;
        if base.hi.sign() <= 0 {
            let r = on_nonneg(&base.neg())?;
            return Some(if even { r } else { r.neg() });
        }
        if even && n > 0 {
            let m = if base.lo.neg().cmp(&base.hi) == Ordering::Greater { base.lo.neg() } else { base.hi.clone() };
            return Some(Iv { lo: Ex::Fin(Float::zero()), hi: end(&m, true)? });
        }
        None
    }
}

/// The ends of a closed half-line condition on x (`x >= a`, `a <= x`, `x <= b`, `b >= x`): (lower, upper) with one
/// of them `None`.
#[allow(clippy::type_complexity)]
fn half_line<'h>(h: &'h Heap, cond: T<'h>, x: T<'h>) -> Option<(Option<T<'h>>, Option<T<'h>>)> {
    let name = head_name(h, cond)?;
    let a = args_of(h, cond);
    if a.len() != 2 {
        return None;
    }
    let (l, r) = match name.as_str() {
        "less_equal" => (a[0], a[1]),
        "greater_equal" => (a[1], a[0]),
        _ => return None,
    };
    if r == x && !contains(h, l, x) {
        Some((Some(l), None))
    } else if l == x && !contains(h, r, x) {
        Some((None, Some(r)))
    } else {
        None
    }
}

/// The exact dyadic value of a number term.
fn dyadic<'h>(en: &Encl<'_, 'h>, t: T<'h>) -> Option<Float> {
    let b = en.constant(t)?;
    b.rad.is_zero().then_some(b.mid)
}

/// The exact rational term of a dyadic number (an odd mantissa below 2^62).
fn dyadic_term<'h>(cx: &mut NativeCx<'_, '_, 'h>, v: &Float) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    if v.is_zero() {
        return Ok(Some(h.int(0)));
    }
    let tz = v.man.trailing_zeros();
    let Some(m) = v.man.shr(tz).to_u128().filter(|&m| m < (1u128 << 62)) else { return Ok(None) };
    let m = if v.neg { -(m as i64) } else { m as i64 };
    let t = h.call(h.sym("mul"), &[h.int(m), h.call(h.sym("pow"), &[h.int(2), h.int(v.exp + tz as i64)])]);
    Ok(Some(cx.eval(t)?))
}

/// The global minimum (maximum when `max`) of `f` over the real line (`cond` = `None`) or a closed half-line, as
/// the bounded interval method gives it on a part [a, b] that is proved to hold it; `None` when not proved.
pub(crate) fn minimize_line<'h>(cx: &mut NativeCx<'_, '_, 'h>, stop: &Arc<AtomicBool>, f: T<'h>, cond: Option<T<'h>>, x: T<'h>, max: bool, goal: Goal) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    if !matches!(h.view(x), View::Sym(_)) || !contains(h, f, x) || has_other_symbols(h, f, x) {
        return Ok(None);
    }
    let (lo, hi) = match cond {
        None => (None, None),
        Some(c) => match half_line(h, c, x) {
            Some(v) => v,
            None => return Ok(None),
        },
    };
    let _scope = StopScope::enter(Some(stop));
    // a monotone outer function
    if let (Some(n), [u]) = (head_name(h, f), args_of(h, f)) {
        if let Some(&(_, incr, _, _)) = MONOTONE.iter().find(|m| m.0 == n && m.0 != "log") {
            let inner = minimize_line(cx, stop, *u, cond, x, max == incr, Goal::Both)?;
            let Some(r) = inner else { return Ok(None) };
            let Some(&[v, pt]) = cx.as_list(r) else { return Ok(None) };
            let Some(&[rule]) = cx.as_list(pt) else { return Ok(None) };
            let point = args_of(h, rule).get(1).copied();
            let Some(point) = point else { return Ok(None) };
            let value = cx.eval(h.call(h.sym(&n), &[v]))?;
            return Ok(Some(answer(h, goal, value, x, point)));
        }
    }
    let ar = Arith::new(h);
    let nm = Names::new(h);
    let en = Encl { h, ar: &ar, nm: &nm, x };
    let lo_f = match lo {
        Some(t) => match dyadic(&en, t) {
            Some(v) => Some((t, v)),
            None => return Ok(None),
        },
        None => None,
    };
    let mut hi_f = match hi {
        Some(t) => match dyadic(&en, t) {
            Some(v) => Some((t, v)),
            None => return Ok(None),
        },
        None => None,
    };
    // a reflection symmetry on the whole line: search x <= c only
    let mut restricted = false;
    if lo_f.is_none() && hi_f.is_none() {
        for c in axes(cx, f, x)? {
            let Some(cf) = dyadic(&en, c) else { continue };
            let refl = h.call(h.sym("add"), &[h.call(h.sym("mul"), &[h.int(2), c]), h.call(h.sym("mul"), &[h.int(-1), x])]);
            let fr = cx.eval(h.call(h.sym("rewrite"), &[f, h.call(h.sym(names::RULE), &[x, refl])]))?;
            let diff = cx.eval(h.call(h.sym("expand_all"), &[h.call(h.sym("add"), &[fr, h.call(h.sym("mul"), &[h.int(-1), f])])]))?;
            if matches!(h.view(diff), View::Int(0)) {
                hi_f = Some((c, cf));
                restricted = true;
                break;
            }
        }
    }
    let r = search(cx, stop, &en, f, x, max, goal, lo_f.clone(), hi_f)?;
    if r.is_none() && restricted {
        // the extremum may sit on the axis itself (an end where f' vanishes is not accepted): the whole line
        return search(cx, stop, &en, f, x, max, goal, lo_f, None);
    }
    Ok(r)
}

fn answer<'h>(h: &'h Heap, goal: Goal, value: T<'h>, x: T<'h>, point: T<'h>) -> T<'h> {
    match goal {
        Goal::Value => value,
        Goal::Point => point,
        Goal::Both => h.call(h.sym(names::LIST), &[value, h.call(h.sym(names::LIST), &[h.call(h.sym(names::RULE), &[x, point])])]),
    }
}

/// Candidate axes of a reflection symmetry: 0 and the vertices -b/(2a) of the quadratic polynomials in f.
fn axes<'h>(cx: &mut NativeCx<'_, '_, 'h>, f: T<'h>, x: T<'h>) -> Res<Vec<T<'h>>> {
    let h = cx.heap();
    let mut out = vec![h.int(0)];
    let mut stack = vec![f];
    let mut seen = 0;
    while let Some(u) = stack.pop() {
        if !contains(h, u, x) {
            continue;
        }
        let View::App { args, .. } = h.view(u) else { continue };
        if head_name(h, u).as_deref() == Some("add") && seen < 8 {
            seen += 1;
            let cl = cx.eval(h.call(h.sym("coefficient_list"), &[u, x]))?;
            if let Some(&[_, b, a]) = cx.as_list(cl) {
                if small_rational(h, a).is_some_and(|q| q.0 != 0) && small_rational(h, b).is_some() {
                    let c = cx.eval(h.call(h.sym("mul"), &[h.int(-1), b, h.call(h.sym("pow"), &[h.call(h.sym("mul"), &[h.int(2), a]), h.int(-1)])]))?;
                    if !out.contains(&c) {
                        out.push(c);
                    }
                }
            }
        }
        stack.extend_from_slice(args);
    }
    Ok(out)
}

/// The bounded part [a, b] for growing widths, the tails proved harmless, then the bounded interval method.
#[allow(clippy::too_many_arguments)]
fn search<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    stop: &Arc<AtomicBool>,
    en: &Encl<'_, 'h>,
    f: T<'h>,
    x: T<'h>,
    max: bool,
    goal: Goal,
    lo: Option<(T<'h>, Float)>,
    hi: Option<(T<'h>, Float)>,
) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    let g = if max { cx.eval(h.call(h.sym("mul"), &[h.int(-1), f]))? } else { f };
    let dg = cx.eval(h.call(h.sym("d"), &[g, x]))?;
    // g and g' reflected for the left tail: g(-x), d/dx g(-x)
    let neg_x = h.call(h.sym("mul"), &[h.int(-1), x]);
    let gl = cx.eval(h.call(h.sym("rewrite"), &[g, h.call(h.sym(names::RULE), &[x, neg_x])]))?;
    let dgl = cx.eval(h.call(h.sym("d"), &[gl, x]))?;
    for w in WIDTHS {
        cx.tick(SAMPLES as u64)?;
        if cx.stopped() {
            return Ok(None);
        }
        let wf = Float::new(false, Nat::from_u64(w as u64), -2);
        let a = match &lo {
            Some((_, v)) => v.clone(),
            None => match &hi {
                Some((_, v)) => {
                    let m = v.sub_exact(&wf);
                    if m.cmp_exact(&wf.neg()) == Ordering::Less {
                        m
                    } else {
                        wf.neg()
                    }
                }
                None => wf.neg(),
            },
        };
        let b = match &hi {
            Some((_, v)) => v.clone(),
            None => {
                let m = a.add_exact(&wf);
                if m.cmp_exact(&wf) == Ordering::Greater {
                    m
                } else {
                    wf.clone()
                }
            }
        };
        // s >= min g on [a, b]: the least upper end of g at sample points
        let mut s: Option<Float> = None;
        // SAMPLES = 2^6 steps of (b - a)/2^6
        let step = b.sub_exact(&a).mul_exact(&Float::new(false, Nat::from_u64(1), -6));
        for k in 0..=SAMPLES {
            let pt = a.add_exact(&step.mul_exact(&Float::from_i64(k)));
            if let Some(v) = en.at(g, &Ball::exact(pt)) {
                let u = v.ends().1;
                if s.as_ref().is_none_or(|q| u.cmp_exact(q) == Ordering::Less) {
                    s = Some(u);
                }
            }
        }
        let Some(s) = s else { return Ok(None) };
        let right = hi.is_some() || tail_ok(cx, en, g, dg, b.clone(), &s)?;
        let left = right && (lo.is_some() || tail_ok(cx, en, gl, dgl, a.neg(), &s)?);
        if right && left {
            let ta = match &lo {
                Some((t, _)) => *t,
                None => match dyadic_term(cx, &a)? {
                    Some(t) => t,
                    None => return Ok(None),
                },
            };
            let tb = match &hi {
                Some((t, _)) => *t,
                None => match dyadic_term(cx, &b)? {
                    Some(t) => t,
                    None => return Ok(None),
                },
            };
            // f must be defined on all of [a, b] (a singular point inside leaves the extremum open, and the
            // bounded method would search around it at length)
            if !defined_on(en, g, &a, &b) {
                return Ok(None);
            }
            let cond = h.call(h.sym("and"), &[h.call(h.sym("less_equal"), &[ta, x]), h.call(h.sym("less_equal"), &[x, tb])]);
            return transroot::minimize_interval(cx, stop, f, cond, x, max, goal);
        }
    }
    Ok(None)
}

/// Has g a real enclosure on each of 256 equal pieces of [a, b]?
fn defined_on<'h>(en: &Encl<'_, 'h>, g: T<'h>, a: &Float, b: &Float) -> bool {
    let step = b.sub_exact(a).mul_exact(&Float::new(false, Nat::from_u64(1), -8));
    let half = Mag::from_float_up(&step.mul_exact(&Float::new(false, Nat::from_u64(1), -1)));
    (0..256).all(|k| {
        let l = a.add_exact(&step.mul_exact(&Float::from_i64(k)));
        let mid = l.add_exact(&step.mul_exact(&Float::new(false, Nat::from_u64(1), -1)));
        en.at(g, &Ball { mid, rad: half }).is_some()
    })
}

/// Is g harmless on [t0, inf): an enclosure above s, or g' >= 0 there?
fn tail_ok<'h>(cx: &mut NativeCx<'_, '_, 'h>, en: &Encl<'_, 'h>, g: T<'h>, dg: T<'h>, t0: Float, s: &Float) -> Res<bool> {
    if t0.cmp_exact(&Float::from_i64(1)) != Ordering::Greater {
        return Ok(false);
    }
    let mut tail = Tail { cx, en: Encl { h: en.h, ar: en.ar, nm: en.nm, x: en.x }, t0 };
    if let Some(iv) = tail.bound(g)? {
        if iv.lo.cmp(&Ex::Fin(s.clone())) == Ordering::Greater {
            return Ok(true);
        }
    }
    if let Some(iv) = tail.bound(dg)? {
        if iv.lo.sign() >= 0 {
            return Ok(true);
        }
    }
    Ok(false)
}
