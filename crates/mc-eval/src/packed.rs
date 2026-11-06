//! Packed arrays in the evaluator (semantics 11; decisions N4a.6 and N4a.7).
//!
//! A packed array is observably its compound term, so the evaluator needs no case for it; two shortcuts keep it
//! from unpacking needlessly:
//!
//! - **Inert arrays.** While `list` has no definitions, a packed array is its own normal form (an inert head and
//!   number arguments), so it is returned at once instead of being viewed and its arguments scanned
//!   (`Eval::is_inert_packed`).
//! - **Element-wise arithmetic.** A listable `add` or `mul` of two operands, at least one a packed array and the
//!   other a packed array of the same shape or a machine number, is computed on the payloads ([`elementwise`]). The
//!   result is what threading gives element by element under the canonical arithmetic (semantics 3, 2.2): machine
//!   integers add and multiply exactly (an overflow leaves the shortcut), a machine integer meets a machine real as
//!   its exact `f64` value (only integers of magnitude at most 2^53, which convert exactly), and two machine reals
//!   give the IEEE sum or product, which is commutative, so the canonical operand order does not matter. Operands
//!   holding a NaN leave the shortcut (the canonical order of NaNs and their payload propagation are the general
//!   path's business). Any other shape falls through to ordinary threading.
use crate::Res;
use mc_core::{Heap, Packed, Store, View, T};

/// Leaves per fuel unit and per charge, as the list library's natives (decision I.4).
const PER_UNIT: usize = 16;
const CHUNK: usize = 1 << 16;

/// Integers that convert to machine reals exactly.
const EXACT_INT: i64 = 1 << 53;

/// The element-wise operation.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Op {
    Add,
    Mul,
    /// `base^exponent` with a machine-real result (decision p6core.27): computed as the general path computes a
    /// machine power, `a.powf(c)`; declined when an element would be complex or an exact integer power
    Pow,
}

/// One operand: the leaves of a packed array, or a machine number broadcast to every element.
#[derive(Clone, Copy)]
enum Operand<'h> {
    Reals(&'h [f64]),
    Ints(&'h [i64]),
    Real(f64),
    Int(i64),
}

impl Operand<'_> {
    fn is_int(&self) -> bool {
        matches!(self, Operand::Ints(_) | Operand::Int(_))
    }

    /// Element `i` as a machine real (the caller checked [`Operand::clean_as_reals`]).
    fn real_at(&self, i: usize) -> f64 {
        match *self {
            Operand::Reals(v) => v[i],
            Operand::Ints(v) => v[i] as f64,
            Operand::Real(x) => x,
            Operand::Int(k) => k as f64,
        }
    }

    fn int_at(&self, i: usize) -> i64 {
        match *self {
            Operand::Ints(v) => v[i],
            Operand::Int(k) => k,
            // invariant: integer operands are read only when both operands are integer kinds
            _ => unreachable!("an integer operand"),
        }
    }

    /// Every value converts to a machine real exactly and none is a NaN.
    fn clean_as_reals(&self) -> bool {
        match *self {
            Operand::Reals(v) => !v.iter().any(|x| x.is_nan()),
            Operand::Ints(v) => v.iter().all(|k| k.unsigned_abs() <= EXACT_INT as u64),
            Operand::Real(x) => !x.is_nan(),
            Operand::Int(k) => k.unsigned_abs() <= EXACT_INT as u64,
        }
    }
}

/// `add` or `mul` of three to eight operands, packed arrays of one shape and machine numbers, element by element as
/// the arithmetic folds machine numbers (decision p6core.27): the integers exactly first, then the reals in their
/// total order. Declined for anything else (an overflow, a non-finite element, other operands).
fn fold_many<'h>(h: &'h Heap, op: Op, vals: &[T<'h>], tick: &mut dyn FnMut(u64) -> Res<()>) -> Res<Option<T<'h>>> {
    if vals.len() > 8 {
        return Ok(None);
    }
    let mut dims: Option<&'h [u64]> = None;
    let mut ops = Vec::with_capacity(vals.len());
    for &t in vals {
        let o = if let Some(p) = h.packed(t) {
            if dims.is_some_and(|d| d != p.dims()) {
                return Ok(None);
            }
            dims = Some(p.dims());
            match p {
                Packed::Reals { data, .. } => Operand::Reals(data),
                Packed::Ints { data, .. } => Operand::Ints(data),
            }
        } else {
            match h.view(t) {
                View::Real(x) => Operand::Real(x),
                View::Int(k) => Operand::Int(k),
                _ => return Ok(None),
            }
        };
        ops.push(o);
    }
    let Some(dims) = dims else { return Ok(None) };
    let shape: Vec<usize> = dims.iter().map(|&d| d as usize).collect();
    let n: usize = shape.iter().product();
    let product = op == Op::Mul;
    if ops.iter().all(Operand::is_int) {
        let mut out = Vec::with_capacity(n);
        for start in (0..n).step_by(CHUNK) {
            let end = (start + CHUNK).min(n);
            tick(((end - start) * ops.len() / PER_UNIT) as u64)?;
            for i in start..end {
                let mut acc: i64 = i64::from(product);
                for o in &ops {
                    let v = o.int_at(i);
                    let Some(r) = (if product { acc.checked_mul(v) } else { acc.checked_add(v) }) else { return Ok(None) };
                    acc = r;
                }
                out.push(acc);
            }
        }
        return Ok(Some(h.int_array(&shape, &out)));
    }
    let mut out = Vec::with_capacity(n);
    let mut reals = [0.0f64; 8];
    for start in (0..n).step_by(CHUNK) {
        let end = (start + CHUNK).min(n);
        tick(((end - start) * ops.len() / PER_UNIT) as u64)?;
        for i in start..end {
            let mut exact: i128 = i128::from(product);
            let mut has_exact = false;
            let mut k = 0;
            for o in &ops {
                match *o {
                    Operand::Ints(v) => {
                        let v = v[i] as i128;
                        let Some(r) = (if product { exact.checked_mul(v) } else { exact.checked_add(v) }) else { return Ok(None) };
                        exact = r;
                        has_exact = true;
                    }
                    Operand::Int(v) => {
                        let v = v as i128;
                        let Some(r) = (if product { exact.checked_mul(v) } else { exact.checked_add(v) }) else { return Ok(None) };
                        exact = r;
                        has_exact = true;
                    }
                    Operand::Reals(v) => {
                        reals[k] = v[i];
                        k += 1;
                    }
                    Operand::Real(x) => {
                        reals[k] = x;
                        k += 1;
                    }
                }
            }
            let rs = &mut reals[..k];
            if rs.iter().any(|x| !x.is_finite()) {
                return Ok(None);
            }
            rs.sort_by(f64::total_cmp);
            let (mut acc, rest) = if has_exact { (exact as f64, &rs[..]) } else { (rs[0], &rs[1..]) };
            for &x in rest {
                acc = if product { acc * x } else { acc + x };
            }
            out.push(acc);
        }
    }
    Ok(Some(h.real_array(&shape, &out)))
}

/// `op(a, b)` threaded over packed arrays, computed on their payloads; `Ok(None)` when the shortcut does not apply
/// (see the module docs). `tick(n)` charges fuel and polls the interrupt, once per chunk of leaves.
pub(crate) fn elementwise<'h>(
    h: &'h Heap,
    op: Op,
    vals: &[T<'h>],
    tick: &mut dyn FnMut(u64) -> Res<()>,
) -> Res<Option<T<'h>>> {
    if vals.len() > 2 {
        return Ok(if op == Op::Pow { None } else { fold_many(h, op, vals, tick)? });
    }
    let [a, b] = *vals else { return Ok(None) };
    let mut dims: Option<&'h [u64]> = None;
    let mut operand = |t: T<'h>| -> Option<Operand<'h>> {
        if let Some(p) = h.packed(t) {
            if dims.is_some_and(|d| d != p.dims()) {
                return None;
            }
            dims = Some(p.dims());
            return Some(match p {
                Packed::Reals { data, .. } => Operand::Reals(data),
                Packed::Ints { data, .. } => Operand::Ints(data),
            });
        }
        match h.view(t) {
            View::Real(x) => Some(Operand::Real(x)),
            View::Int(k) => Some(Operand::Int(k)),
            _ => None,
        }
    };
    let (Some(x), Some(y)) = (operand(a), operand(b)) else { return Ok(None) };
    let Some(dims) = dims else { return Ok(None) };
    let shape: Vec<usize> = dims.iter().map(|&d| d as usize).collect();
    let n: usize = shape.iter().product();
    if op == Op::Pow {
        if x.is_int() && y.is_int() || !x.clean_as_reals() || !y.clean_as_reals() {
            return Ok(None);
        }
        let mut out = Vec::with_capacity(n);
        for start in (0..n).step_by(CHUNK) {
            let end = (start + CHUNK).min(n);
            tick(((end - start) / PER_UNIT) as u64)?;
            for i in start..end {
                let (a, c) = (x.real_at(i), y.real_at(i));
                // a negative base with a fractional exponent is complex, a zero base with a negative exponent may
                // be a division by zero: the general path decides those
                if !(a > 0.0 || (a.is_infinite() && a > 0.0) || (c.fract() == 0.0 && a != 0.0) || (a == 0.0 && c > 0.0)) {
                    return Ok(None);
                }
                out.push(a.powf(c));
            }
        }
        return Ok(Some(h.real_array(&shape, &out)));
    }
    if x.is_int() && y.is_int() {
        let mut out = Vec::with_capacity(n);
        for start in (0..n).step_by(CHUNK) {
            let end = (start + CHUNK).min(n);
            tick(((end - start) / PER_UNIT) as u64)?;
            for i in start..end {
                let (p, q) = (x.int_at(i), y.int_at(i));
                let Some(v) = (match op {
                    Op::Add => p.checked_add(q),
                    Op::Mul => p.checked_mul(q),
                    Op::Pow => None,
                }) else {
                    return Ok(None);
                };
                out.push(v);
            }
        }
        return Ok(Some(h.int_array(&shape, &out)));
    }
    if !x.clean_as_reals() || !y.clean_as_reals() {
        return Ok(None);
    }
    let mut out = Vec::with_capacity(n);
    for start in (0..n).step_by(CHUNK) {
        let end = (start + CHUNK).min(n);
        tick(((end - start) / PER_UNIT) as u64)?;
        match op {
            Op::Add => out.extend((start..end).map(|i| x.real_at(i) + y.real_at(i))),
            Op::Mul => out.extend((start..end).map(|i| x.real_at(i) * y.real_at(i))),
            Op::Pow => return Ok(None),
        }
    }
    Ok(Some(h.real_array(&shape, &out)))
}
