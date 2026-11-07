//! Building terms of arithmetic (left unevaluated until the result is evaluated once), and reading numbers.
use mc_api::names;
use mc_core::{Heap, Store, View, T};
use mc_eval::{NativeCx, Res};

/// A builder of terms on a heap.
#[derive(Clone, Copy)]
pub struct E<'h> {
    /// the heap
    pub h: &'h Heap,
}

impl<'h> E<'h> {
    /// `name(args...)`
    pub fn f(&self, name: &str, args: &[T<'h>]) -> T<'h> {
        self.h.call(self.h.sym(name), args)
    }
    /// A symbol.
    pub fn sym(&self, name: &str) -> T<'h> {
        self.h.sym_term(self.h.sym(name))
    }
    /// An integer.
    pub fn int(&self, v: i64) -> T<'h> {
        self.h.int(v)
    }
    /// p / q
    pub fn rat(&self, p: i64, q: i64) -> T<'h> {
        if q == 1 {
            return self.int(p);
        }
        self.mul(&[self.int(p), self.pow(self.int(q), self.int(-1))])
    }
    /// A sum (0 when empty).
    pub fn add(&self, a: &[T<'h>]) -> T<'h> {
        match a {
            [] => self.int(0),
            [x] => *x,
            _ => self.f(names::ADD, a),
        }
    }
    /// A product (1 when empty).
    pub fn mul(&self, a: &[T<'h>]) -> T<'h> {
        match a {
            [] => self.int(1),
            [x] => *x,
            _ => self.f(names::MUL, a),
        }
    }
    /// a^b
    pub fn pow(&self, a: T<'h>, b: T<'h>) -> T<'h> {
        self.f(names::POW, &[a, b])
    }
    /// -a
    pub fn neg(&self, a: T<'h>) -> T<'h> {
        self.mul(&[self.int(-1), a])
    }
    /// a - b
    pub fn sub(&self, a: T<'h>, b: T<'h>) -> T<'h> {
        self.add(&[a, self.neg(b)])
    }
    /// a / b
    pub fn div(&self, a: T<'h>, b: T<'h>) -> T<'h> {
        self.mul(&[a, self.pow(b, self.int(-1))])
    }
    /// a^2
    pub fn sq(&self, a: T<'h>) -> T<'h> {
        self.pow(a, self.int(2))
    }
    /// sqrt(a)
    pub fn sqrt(&self, a: T<'h>) -> T<'h> {
        self.f("sqrt", &[a])
    }
    /// A list.
    pub fn list(&self, a: &[T<'h>]) -> T<'h> {
        self.f(names::LIST, a)
    }
    /// The component-wise difference p - q.
    pub fn vsub(&self, p: &[T<'h>], q: &[T<'h>]) -> Vec<T<'h>> {
        p.iter().zip(q).map(|(&a, &b)| self.sub(a, b)).collect()
    }
    /// The dot product.
    pub fn dot(&self, p: &[T<'h>], q: &[T<'h>]) -> T<'h> {
        let terms: Vec<T<'h>> = p.iter().zip(q).map(|(&a, &b)| self.mul(&[a, b])).collect();
        self.add(&terms)
    }
    /// The squared length.
    pub fn norm2(&self, p: &[T<'h>]) -> T<'h> {
        let terms: Vec<T<'h>> = p.iter().map(|&a| self.sq(a)).collect();
        self.add(&terms)
    }
    /// The distance between two points.
    pub fn dist(&self, p: &[T<'h>], q: &[T<'h>]) -> T<'h> {
        self.sqrt(self.norm2(&self.vsub(p, q)))
    }
    /// c * p
    pub fn scale(&self, c: T<'h>, p: &[T<'h>]) -> Vec<T<'h>> {
        p.iter().map(|&a| self.mul(&[c, a])).collect()
    }
    /// p + q
    pub fn vadd(&self, p: &[T<'h>], q: &[T<'h>]) -> Vec<T<'h>> {
        p.iter().zip(q).map(|(&a, &b)| self.add(&[a, b])).collect()
    }
    /// The cross product of two vectors of three coordinates.
    pub fn cross(&self, a: &[T<'h>], b: &[T<'h>]) -> Vec<T<'h>> {
        let c = |i: usize, j: usize| self.sub(self.mul(&[a[i], b[j]]), self.mul(&[a[j], b[i]]));
        vec![c(1, 2), c(2, 0), c(0, 1)]
    }
    /// The 2D cross product (a determinant of two vectors).
    pub fn cross2(&self, a: &[T<'h>], b: &[T<'h>]) -> T<'h> {
        self.sub(self.mul(&[a[0], b[1]]), self.mul(&[a[1], b[0]]))
    }
    /// pi
    pub fn pi(&self) -> T<'h> {
        self.sym("pi")
    }
}

/// A machine real for a numeric literal.
pub fn lit<S: Store + ?Sized>(h: &S, t: T<'_>) -> Option<f64> {
    let v = match h.view(t) {
        View::Int(i) => i as f64,
        View::Real(x) => x,
        View::BigInt { neg, mag } => {
            let m = mag.iter().rev().fold(0.0f64, |acc, &limb| acc * 18_446_744_073_709_551_616.0 + limb as f64);
            if neg {
                -m
            } else {
                m
            }
        }
        View::Rational { num, den } => lit(h, num)? / lit(h, den)?,
        _ => return None,
    };
    Some(v)
}

/// The value of a closed numeric expression as a machine real (through `n`); `None` when it is not a real number.
pub fn num<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>) -> Res<Option<f64>> {
    if let Some(v) = lit(cx.heap(), t) {
        return Ok(Some(v));
    }
    let h = cx.heap();
    let v = cx.eval(h.call(h.sym("n"), &[t]))?;
    Ok(lit(cx.heap(), v).filter(|x| !x.is_nan()))
}

/// [`num`] of every coordinate.
pub fn nums<'h>(cx: &mut NativeCx<'_, '_, 'h>, p: &[T<'h>]) -> Res<Option<Vec<f64>>> {
    let mut out = Vec::with_capacity(p.len());
    for &x in p {
        match num(cx, x)? {
            Some(v) => out.push(v),
            None => return Ok(None),
        }
    }
    Ok(Some(out))
}

/// [`nums`] of a list of points.
pub fn points<'h>(cx: &mut NativeCx<'_, '_, 'h>, ps: &[Vec<T<'h>>]) -> Res<Option<Vec<Vec<f64>>>> {
    let mut out = Vec::with_capacity(ps.len());
    for p in ps {
        match nums(cx, p)? {
            Some(v) => out.push(v),
            None => return Ok(None),
        }
    }
    Ok(Some(out))
}

/// A machine real as a term: integers that are exactly integral stay integers.
pub fn real_term<'h>(h: &'h Heap, v: f64) -> T<'h> {
    h.real(v)
}

/// Whether a term contains a real (inexact) number: results are then numbers too.
pub fn inexact<'h>(h: &'h Heap, t: T<'h>) -> bool {
    match h.view(t) {
        View::Real(_) => true,
        View::App { head, args } => inexact(h, head) || args.iter().any(|&a| inexact(h, a)),
        _ => h.packed(t).is_some_and(|p| matches!(p.elem(), mc_api::Elem::Real | mc_api::Elem::Complex)),
    }
}
