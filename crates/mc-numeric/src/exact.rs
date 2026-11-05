//! Exact identities of the elementary functions (semantics 15): special values at rational multiples of pi, the
//! inverse functions at those values, parity, and compositions that hold for every complex argument. Each returns
//! the replacement term (evaluated again by the caller) or `None`; an argument without an identity stays as the
//! call (decision L.2).
use crate::consts::Const;
use crate::fun::Fun;
use crate::numeval::Names;
use mc_api::{Store, View, T};
use mc_arith::num::{Int, Rat};
use mc_arith::{Arith, Num, Real};

/// The identities for one store.
pub struct Exact<'a, 'h, S: Store + ?Sized> {
    st: &'h S,
    ar: &'a Arith<'h, S>,
    nm: &'a Names,
}

fn q(n: i64, d: i64) -> Rat {
    Rat::new(Int::from_i64(n), Int::from_i64(d))
}

impl<'a, 'h, S: Store + ?Sized> Exact<'a, 'h, S> {
    /// The identities for terms of `st`.
    pub fn new(st: &'h S, ar: &'a Arith<'h, S>, nm: &'a Names) -> Self {
        Exact { st, ar, nm }
    }

    fn rat_term(&self, r: &Rat) -> T<'h> {
        self.ar.number(&Num::Real(Real::Exact(r.clone())))
    }

    fn rat(&self, t: T<'h>) -> Option<Rat> {
        match self.ar.num(t)? {
            Num::Real(Real::Exact(r)) => Some(r),
            _ => None,
        }
    }

    fn constant(&self, c: Const) -> T<'h> {
        self.st.sym_term(self.nm.sym_of(c))
    }

    fn call(&self, f: Fun, x: T<'h>) -> T<'h> {
        self.st.call(self.nm.fun_sym(f), &[x])
    }

    /// q pi, canonical.
    fn times_pi(&self, r: &Rat) -> T<'h> {
        self.ar.mul(&[self.rat_term(r), self.constant(Const::Pi)])
    }

    /// The arguments of `mul(c, k)` with `k` the term `k_term` and c a number; the number.
    fn coefficient_of(&self, t: T<'h>, k_term: T<'h>) -> Option<Num> {
        if t == k_term {
            return Some(Num::int(1));
        }
        let View::App { head, args: [a, b] } = self.st.view(t) else {
            return None;
        };
        if head != self.st.sym_term(self.nm.mul()) {
            return None;
        }
        match (self.ar.num(*a), self.ar.num(*b)) {
            (Some(c), None) if *b == k_term => Some(c),
            (None, Some(c)) if *a == k_term => Some(c),
            _ => None,
        }
    }

    /// q with t = q pi (q rational, t = 0 included).
    fn pi_multiple(&self, t: T<'h>) -> Option<Rat> {
        if let Some(r) = self.rat(t) {
            return r.is_zero().then_some(r);
        }
        match self.coefficient_of(t, self.constant(Const::Pi))? {
            Num::Real(Real::Exact(r)) => Some(r),
            _ => None,
        }
    }

    /// -t when t has a negative exact coefficient (a negative rational, or a product with one).
    fn negated(&self, t: T<'h>) -> Option<T<'h>> {
        let negative = match self.st.view(t) {
            View::App { head, args } if head == self.st.sym_term(self.nm.mul()) => {
                args.iter().any(|&a| self.rat(a).is_some_and(|r| r.num.neg))
            }
            _ => self.rat(t).is_some_and(|r| r.num.neg),
        };
        negative.then(|| self.ar.neg(t))
    }

    /// sin(q pi) where it is a rational or a square root: q reduced modulo 2 to [0, 1/2] by symmetry.
    fn sin_pi(&self, r: &Rat) -> Option<T<'h>> {
        let (_, frac) = r.mul(&q(1, 2)).floor_fract();
        let mut x = frac.mul(&Rat::from_i64(2));
        let mut neg = false;
        if x.cmp_value(&Rat::from_i64(1)).is_ge() {
            x = x.sub(&Rat::from_i64(1));
            neg = true;
        }
        if x.cmp_value(&q(1, 2)).is_gt() {
            x = Rat::from_i64(1).sub(&x);
        }
        let half_root =
            |n: i64| self.ar.mul(&[self.rat_term(&q(1, 2)), self.ar.pow(self.st.int(n), self.rat_term(&q(1, 2)))]);
        let v = if x.is_zero() {
            self.st.int(0)
        } else if x == q(1, 6) {
            self.rat_term(&q(1, 2))
        } else if x == q(1, 4) {
            half_root(2)
        } else if x == q(1, 3) {
            half_root(3)
        } else if x == q(1, 2) {
            self.st.int(1)
        } else {
            return None;
        };
        Some(if neg { self.ar.neg(v) } else { v })
    }

    fn cos_pi(&self, r: &Rat) -> Option<T<'h>> {
        self.sin_pi(&r.add(&q(1, 2)))
    }

    fn tan_pi(&self, r: &Rat) -> Option<T<'h>> {
        let (s, c) = (self.sin_pi(r)?, self.cos_pi(r)?);
        (c != self.st.int(0)).then(|| self.ar.div(s, c))
    }

    /// r pi with f(r pi) = x, for the table of f's special values on [0, 1/2]; `inverse(sin, x)` and so on.
    fn inverse(&self, value: impl Fn(&Rat) -> Option<T<'h>>, x: T<'h>) -> Option<Rat> {
        [q(0, 1), q(1, 6), q(1, 4), q(1, 3), q(1, 2)].into_iter().find(|r| value(r) == Some(x))
    }

    /// The identity for f(x), if one applies.
    pub fn simplify(&self, f: Fun, x: T<'h>) -> Option<T<'h>> {
        // parity: f(-x) = -f(x) or f(x)
        if let (Some(odd), Some(y)) = (f.parity(), self.negated(x)) {
            let fy = self.call(f, y);
            return Some(if odd { self.ar.neg(fy) } else { fy });
        }
        let zero = self.st.int(0);
        let one = self.st.int(1);
        // f(g(y)) = y for g the inverse of f
        if let View::App { head, args: [y] } = self.st.view(x) {
            let inner = match self.st.view(head) {
                View::Sym(s) => self.nm.fun(s),
                _ => None,
            };
            let inverse = matches!(
                (f, inner),
                (Fun::Exp, Some(Fun::Log))
                    | (Fun::Sin, Some(Fun::Asin))
                    | (Fun::Cos, Some(Fun::Acos))
                    | (Fun::Tan, Some(Fun::Atan))
            );
            if inverse {
                return Some(*y);
            }
            // log(exp(y)) = y for real y
            if f == Fun::Log && inner == Some(Fun::Exp) && matches!(self.ar.num(*y), Some(Num::Real(_))) {
                return Some(*y);
            }
        }
        match f {
            Fun::Sin => self.sin_pi(&self.pi_multiple(x)?),
            Fun::Cos => self.cos_pi(&self.pi_multiple(x)?),
            Fun::Tan => self.tan_pi(&self.pi_multiple(x)?),
            Fun::Asin => self.inverse(|r| self.sin_pi(r), x).map(|r| self.times_pi(&r)),
            Fun::Acos => {
                // acos(x) = pi/2 - asin(x); acos(-x) = pi - acos(x)
                if let Some(y) = self.negated(x) {
                    let r = self.inverse(|r| self.sin_pi(r), y)?;
                    return Some(self.times_pi(&q(1, 2).add(&r)));
                }
                let r = self.inverse(|r| self.sin_pi(r), x)?;
                Some(self.times_pi(&q(1, 2).sub(&r)))
            }
            Fun::Atan => [q(0, 1), q(1, 6), q(1, 4), q(1, 3)]
                .into_iter()
                .find(|r| self.tan_pi(r) == Some(x))
                .map(|r| self.times_pi(&r)),
            Fun::Exp => self.exp(x),
            Fun::Log => self.log(x),
            Fun::Sinh | Fun::Tanh => (x == zero).then_some(zero),
            Fun::Cosh => (x == zero).then_some(one),
            Fun::Sqrt => None,
            Fun::Log1p | Fun::Expm1 => (x == zero).then_some(zero),
        }
    }

    fn exp(&self, x: T<'h>) -> Option<T<'h>> {
        if x == self.st.int(0) {
            return Some(self.st.int(1));
        }
        if x == self.st.int(1) {
            return Some(self.constant(Const::E));
        }
        // exp(q pi i) = cos(q pi) + i sin(q pi)
        let Num::Complex(re, Real::Exact(r)) = self.coefficient_of(x, self.constant(Const::Pi))? else {
            return None;
        };
        if !re.is_exact_zero() {
            return None;
        }
        let (c, s) = (self.cos_pi(&r)?, self.sin_pi(&r)?);
        let i = self.ar.complex(self.st.int(0), self.st.int(1));
        Some(self.ar.add(&[c, self.ar.mul(&[i, s])]))
    }

    fn log(&self, x: T<'h>) -> Option<T<'h>> {
        let e = self.constant(Const::E);
        if x == self.st.int(1) {
            return Some(self.st.int(0));
        }
        if x == e {
            return Some(self.st.int(1));
        }
        if x == self.st.int(-1) {
            let i = self.ar.complex(self.st.int(0), self.st.int(1));
            return Some(self.ar.mul(&[i, self.constant(Const::Pi)]));
        }
        // log(e^y) = y for real y
        let View::App { head, args: [b, y] } = self.st.view(x) else {
            return None;
        };
        (head == self.st.sym_term(self.nm.pow()) && *b == e && matches!(self.ar.num(*y), Some(Num::Real(_))))
            .then_some(*y)
    }
}
