//! The order in which solutions are listed (decision 11C.5).
//!
//! Solutions are sorted by an order of their own, which differs from v2's term order (`mc-arith`'s, by head
//! first): a product is compared through its factors from the last one backwards with its numeric coefficient
//! last (so `-sqrt(2)`, `-i sqrt(2)`, `i sqrt(2)`, `sqrt(2)` sort together, by coefficient), a sum through its terms
//! from the last one backwards, a power through its base and then its exponent (`x` counting as `x^1`), numbers come
//! first by value. This module implements that order on v2's terms, so that `solve` lists related solutions next
//! to each other (the documentation's examples show them in that order). The unit factors `i (-1)^r` and `-i (-1)^r` that
//! v2's canonical form makes of `(-1)^(r + 1/2)` are compared as the latter (the roots of unity `-(-1)^(1/5)`,
//! `(-1)^(2/5)`, `-(-1)^(3/5)`, ... by their exponent). Ties are broken by v2's order, so the order is total.
use crate::cx::Env;
use mc_api::{Store, View, T};
use mc_arith::num::Rat;
use mc_arith::Num;
use std::cmp::Ordering;

/// The order of solutions.
pub(crate) struct SolutionOrder<'e, 'h> {
    env: &'e Env<'h>,
    tie: mc_arith::Order<'h, mc_core::Heap>,
}

/// A term as the order sees it.
enum Shape<'h> {
    Number(Num),
    /// numeric coefficient, the other factors (in this order's ascending order) and the exponent of a factor
    /// `(-1)^r` (which sorts before the others)
    Product(Num, Vec<T<'h>>, Option<Rat>),
    Sum(Vec<T<'h>>),
    Power(T<'h>, T<'h>),
    Symbol(String),
    Call(String, Vec<T<'h>>),
    Other,
}

impl<'e, 'h> SolutionOrder<'e, 'h> {
    pub fn new(env: &'e Env<'h>) -> SolutionOrder<'e, 'h> {
        SolutionOrder { env, tie: mc_arith::Order::new(env.h()) }
    }

    fn shape(&self, t: T<'h>) -> Shape<'h> {
        let e = self.env;
        let h = e.h();
        if let Some(n) = e.ar.num(t) {
            return Shape::Number(n);
        }
        match h.view(t) {
            View::Sym(s) => Shape::Symbol(h.sym_name(s)),
            View::App { head, args } if head == e.hd.mul => {
                let mut c = Num::int(1);
                let mut fs = vec![];
                let mut unit = None;
                for &a in args {
                    match e.ar.num(a) {
                        Some(n) => c = c.mul(&n),
                        None => match self.unit_exponent(a) {
                            Some(r) if unit.is_none() => unit = Some(r),
                            _ => fs.push(a),
                        },
                    }
                }
                // i (-1)^r and -i (-1)^r as (-1)^(r + 1/2) and -(-1)^(r + 1/2)
                if let (Some(r), Num::Complex(re, mc_arith::Real::Exact(im))) = (&unit, &c) {
                    let sign = im.num.to_i64().filter(|v| v.abs() == 1 && im.den.to_u128() == Some(1));
                    if let (true, Some(sign)) = (re.is_zero(), sign) {
                        let half = Rat::new(mc_arith::num::Int::from_i64(1), mc_arith::num::Int::from_i64(2));
                        let r2 = r.add(&half);
                        if r2.cmp_value(&Rat::from_i64(1)).is_le() {
                            unit = Some(r2);
                            c = Num::int(sign);
                        } else {
                            unit = Some(r.sub(&half));
                            c = Num::int(-sign);
                        }
                    }
                }
                fs.sort_by(|&a, &b| self.cmp(a, b));
                Shape::Product(c, fs, unit)
            }
            View::App { head, args } if head == e.hd.add => {
                let mut ts = args.to_vec();
                ts.sort_by(|&a, &b| self.cmp(a, b));
                Shape::Sum(ts)
            }
            View::App { head, args: &[b, x] } if head == e.hd.pow => Shape::Power(b, x),
            View::App { head, args } => match h.view(head) {
                View::Sym(s) => Shape::Call(h.sym_name(s), args.to_vec()),
                _ => Shape::Other,
            },
            _ => Shape::Other,
        }
    }

    /// r for `(-1)^r` with r rational.
    fn unit_exponent(&self, t: T<'h>) -> Option<Rat> {
        let e = self.env;
        match e.h().view(t) {
            View::App { head, args: &[b, x] } if head == e.hd.pow && e.ar.num(b).is_some_and(|n| n.cmp_canonical(&Num::int(-1)).is_eq()) => {
                e.ar.num(x)?.as_rat().cloned()
            }
            _ => None,
        }
    }

    /// Compare two terms.
    pub fn cmp(&self, a: T<'h>, b: T<'h>) -> Ordering {
        if a == b {
            return Ordering::Equal;
        }
        let o = self.cmp_shapes(a, b);
        if o != Ordering::Equal {
            return o;
        }
        self.tie.cmp(a, b)
    }

    fn cmp_shapes(&self, a: T<'h>, b: T<'h>) -> Ordering {
        let (sa, sb) = (self.shape(a), self.shape(b));
        match (&sa, &sb) {
            (Shape::Number(x), Shape::Number(y)) => return x.cmp_canonical(y),
            (Shape::Number(_), _) => return Ordering::Less,
            (_, Shape::Number(_)) => return Ordering::Greater,
            _ => {}
        }
        if matches!(sa, Shape::Product(..)) || matches!(sb, Shape::Product(..)) || self.unit_exponent(a).is_some() || self.unit_exponent(b).is_some() {
            let (ca, fa, ua) = self.as_product(sa, a);
            let (cb, fb, ub) = self.as_product(sb, b);
            let units = match (ua, ub) {
                (None, None) => Ordering::Equal,
                (None, Some(_)) => Ordering::Less,
                (Some(_), None) => Ordering::Greater,
                (Some(x), Some(y)) => x.cmp_value(&y),
            };
            return self.cmp_backwards(&fa, &fb).then(units).then_with(|| ca.cmp_canonical(&cb));
        }
        if matches!(sa, Shape::Sum(_)) || matches!(sb, Shape::Sum(_)) {
            let ta = if let Shape::Sum(t) = sa { t } else { vec![a] };
            let tb = if let Shape::Sum(t) = sb { t } else { vec![b] };
            return self.cmp_backwards(&ta, &tb);
        }
        if matches!(sa, Shape::Power(..)) || matches!(sb, Shape::Power(..)) {
            let one = self.env.int(1);
            let (ba, ea) = if let Shape::Power(x, y) = sa { (x, y) } else { (a, one) };
            let (bb, eb) = if let Shape::Power(x, y) = sb { (x, y) } else { (b, one) };
            if (ba, ea) == (a, one) && (bb, eb) == (b, one) {
                return Ordering::Equal;
            }
            return self.cmp(ba, bb).then_with(|| self.cmp(ea, eb));
        }
        match (sa, sb) {
            (Shape::Symbol(x), Shape::Symbol(y)) => x.cmp(&y),
            (Shape::Symbol(_), _) => Ordering::Less,
            (_, Shape::Symbol(_)) => Ordering::Greater,
            (Shape::Call(f, xs), Shape::Call(g, ys)) => f.cmp(&g).then_with(|| {
                for (x, y) in xs.iter().zip(&ys) {
                    let o = self.cmp(*x, *y);
                    if o != Ordering::Equal {
                        return o;
                    }
                }
                xs.len().cmp(&ys.len())
            }),
            _ => Ordering::Equal,
        }
    }

    fn as_product(&self, s: Shape<'h>, t: T<'h>) -> (Num, Vec<T<'h>>, Option<Rat>) {
        match s {
            Shape::Product(c, fs, u) => (c, fs, u),
            _ => match self.unit_exponent(t) {
                Some(r) => (Num::int(1), vec![], Some(r)),
                None => (Num::int(1), vec![t], None),
            },
        }
    }

    /// Compare two ascending lists from their last elements backwards; a list that runs out first is smaller.
    fn cmp_backwards(&self, xs: &[T<'h>], ys: &[T<'h>]) -> Ordering {
        for (x, y) in xs.iter().rev().zip(ys.iter().rev()) {
            let o = self.cmp(*x, *y);
            if o != Ordering::Equal {
                return o;
            }
        }
        xs.len().cmp(&ys.len())
    }

    /// Compare two solutions value by value.
    pub fn cmp_values(&self, a: &[T<'h>], b: &[T<'h>]) -> Ordering {
        for (x, y) in a.iter().zip(b) {
            let o = self.cmp(*x, *y);
            if o != Ordering::Equal {
                return o;
            }
        }
        a.len().cmp(&b.len())
    }
}
