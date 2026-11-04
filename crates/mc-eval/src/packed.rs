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

/// `op(a, b)` threaded over packed arrays, computed on their payloads; `Ok(None)` when the shortcut does not apply
/// (see the module docs). `tick(n)` charges fuel and polls the interrupt, once per chunk of leaves.
pub(crate) fn elementwise<'h>(
    h: &'h Heap,
    op: Op,
    vals: &[T<'h>],
    tick: &mut dyn FnMut(u64) -> Res<()>,
) -> Res<Option<T<'h>>> {
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
        }
    }
    Ok(Some(h.real_array(&shape, &out)))
}
