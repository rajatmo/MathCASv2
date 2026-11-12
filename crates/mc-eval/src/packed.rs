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
//! - **Complex operands** (gap G48, decision sp-core.2). Packed arrays of machine complex numbers, complex machine
//!   numbers and exact Gaussian integers (`im`, `2 - 3*im`) join `add` and `mul` too ([`complex_fold`]): per element
//!   the exact operands are folded exactly, the inexact ones are sorted by the canonical order of numbers and folded
//!   with the arithmetic's own formulas (`(a + bi)(c + di) = (ac - bd) + (ad + bc)i`, a real times a complex number
//!   part by part), so every element is bitwise what the general path gives.
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
    /// the parts of a packed array of machine complex numbers
    Complexes(&'h [f64]),
    /// a machine complex number
    Complex(f64, f64),
    /// an exact complex number with integer parts (the imaginary part nonzero)
    Gauss(i64, i64),
}

impl Operand<'_> {
    fn is_int(&self) -> bool {
        matches!(self, Operand::Ints(_) | Operand::Int(_))
    }

    fn is_complex(&self) -> bool {
        matches!(self, Operand::Complexes(_) | Operand::Complex(..) | Operand::Gauss(..))
    }

    /// Element `i` as a machine real (the caller checked [`Operand::clean_as_reals`]).
    fn real_at(&self, i: usize) -> f64 {
        match *self {
            Operand::Reals(v) => v[i],
            Operand::Ints(v) => v[i] as f64,
            Operand::Real(x) => x,
            Operand::Int(k) => k as f64,
            // invariant: complex operands take the complex path before reals are read
            _ => unreachable!("a real operand"),
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
            _ => false,
        }
    }
}

/// The operand `t` stands for: a packed array's payload (its dimensions checked against `dims`, set by the first
/// array), or a machine number, a machine integer, a machine complex number or a Gaussian integer.
fn operand<'h>(h: &'h Heap, t: T<'h>, dims: &mut Option<&'h [u64]>) -> Option<Operand<'h>> {
    if let Some(p) = h.packed(t) {
        if dims.is_some_and(|d| d != p.dims()) {
            return None;
        }
        *dims = Some(p.dims());
        return Some(match p {
            Packed::Reals { data, .. } => Operand::Reals(data),
            Packed::Ints { data, .. } => Operand::Ints(data),
            Packed::Complexes { data, .. } => Operand::Complexes(data),
        });
    }
    match h.view(t) {
        View::Real(x) => Some(Operand::Real(x)),
        View::Int(k) => Some(Operand::Int(k)),
        View::App { head, args: &[re, im] } => {
            if !matches!(h.view(head), View::Sym(s) if h.sym_name(s) == mc_api::names::COMPLEX) {
                return None;
            }
            match (h.view(re), h.view(im)) {
                (View::Real(a), View::Real(b)) => Some(Operand::Complex(a, b)),
                (View::Int(a), View::Int(b)) if b != 0 => Some(Operand::Gauss(a, b)),
                _ => None,
            }
        }
        _ => None,
    }
}

/// A machine number of the folds: real, or complex with its parts.
#[derive(Clone, Copy)]
struct Mz {
    re: f64,
    im: f64,
    complex: bool,
}

impl Mz {
    /// The canonical order of two machine numbers (`mc_arith`'s `Num::cmp_canonical`): by real part, then
    /// imaginary part (a real's is 0), reals before complex numbers, then by representation. Both are finite.
    fn cmp(&self, o: &Mz) -> std::cmp::Ordering {
        let num = |a: f64, b: f64| a.partial_cmp(&b).unwrap_or(std::cmp::Ordering::Equal);
        num(self.re, o.re)
            .then_with(|| num(if self.complex { self.im } else { 0.0 }, if o.complex { o.im } else { 0.0 }))
            .then_with(|| self.complex.cmp(&o.complex))
            .then_with(|| self.re.total_cmp(&o.re))
            .then_with(|| if self.complex && o.complex { self.im.total_cmp(&o.im) } else { std::cmp::Ordering::Equal })
    }

    /// `Num::add` of machine numbers.
    fn add(self, o: Mz) -> Mz {
        match (self.complex, o.complex) {
            (false, false) => Mz { re: self.re + o.re, im: 0.0, complex: false },
            (false, true) => Mz { re: self.re + o.re, im: o.im, complex: true },
            (true, false) => Mz { re: o.re + self.re, im: self.im, complex: true },
            (true, true) => Mz { re: self.re + o.re, im: self.im + o.im, complex: true },
        }
    }

    /// `Num::mul` of machine numbers.
    fn mul(self, o: Mz) -> Mz {
        match (self.complex, o.complex) {
            (false, false) => Mz { re: self.re * o.re, im: 0.0, complex: false },
            (false, true) => Mz { re: self.re * o.re, im: self.re * o.im, complex: true },
            (true, false) => Mz { re: o.re * self.re, im: o.re * self.im, complex: true },
            (true, true) => {
                let (a, b, c, d) = (self.re, self.im, o.re, o.im);
                Mz { re: a * c + -(b * d), im: a * d + b * c, complex: true }
            }
        }
    }
}

/// `add` or `mul` of up to eight operands of one shape when one of them is complex (see the module docs): per
/// element, the exact operands (machine integers and Gaussian integers) are folded exactly, the inexact ones sorted
/// by the canonical order and folded onto them as `Num::sum` and `Num::product` do. Declined for non-finite parts,
/// exact parts beyond 2^53, an overflow, or elements of different kinds (a real result among complex ones).
fn complex_fold<'h>(
    h: &'h Heap,
    op: Op,
    ops: &[Operand<'h>],
    dims: &[u64],
    tick: &mut dyn FnMut(u64) -> Res<()>,
) -> Res<Option<T<'h>>> {
    if op == Op::Pow || ops.len() > 8 {
        return Ok(None);
    }
    let shape: Vec<usize> = dims.iter().map(|&d| d as usize).collect();
    let n: usize = shape.iter().product();
    let product = op == Op::Mul;
    let mut out: Vec<f64> = Vec::with_capacity(2 * n);
    let mut kind: Option<bool> = None;
    let mut items = [Mz { re: 0.0, im: 0.0, complex: false }; 8];
    for start in (0..n).step_by(CHUNK) {
        let end = (start + CHUNK).min(n);
        tick(((end - start) * ops.len() / PER_UNIT) as u64)?;
        for i in start..end {
            // the exact part as a Gaussian integer
            let (mut er, mut ei): (i128, i128) = (i128::from(product), 0);
            let mut has_exact = false;
            let mut k = 0;
            for o in ops {
                let exact = match *o {
                    Operand::Ints(v) => Some((v[i] as i128, 0)),
                    Operand::Int(v) => Some((v as i128, 0)),
                    Operand::Gauss(a, b) => Some((a as i128, b as i128)),
                    Operand::Reals(v) => {
                        items[k] = Mz { re: v[i], im: 0.0, complex: false };
                        None
                    }
                    Operand::Real(x) => {
                        items[k] = Mz { re: x, im: 0.0, complex: false };
                        None
                    }
                    Operand::Complexes(v) => {
                        items[k] = Mz { re: v[2 * i], im: v[2 * i + 1], complex: true };
                        None
                    }
                    Operand::Complex(a, b) => {
                        items[k] = Mz { re: a, im: b, complex: true };
                        None
                    }
                };
                match exact {
                    Some((c, d)) => {
                        has_exact = true;
                        let r = if product {
                            (|| Some((er.checked_mul(c)?.checked_sub(ei.checked_mul(d)?)?, er.checked_mul(d)?.checked_add(ei.checked_mul(c)?)?)))()
                        } else {
                            (|| Some((er.checked_add(c)?, ei.checked_add(d)?)))()
                        };
                        let Some((r0, r1)) = r else { return Ok(None) };
                        (er, ei) = (r0, r1);
                    }
                    None => k += 1,
                }
            }
            let xs = &mut items[..k];
            if xs.is_empty() || xs.iter().any(|z| !z.re.is_finite() || !z.im.is_finite()) {
                return Ok(None);
            }
            xs.sort_by(Mz::cmp);
            let (mut acc, rest) = if has_exact {
                if er.unsigned_abs() > EXACT_INT as u128 || ei.unsigned_abs() > EXACT_INT as u128 {
                    return Ok(None);
                }
                // an exact zero imaginary part is no complex number (`Num::complex`)
                (Mz { re: er as f64, im: ei as f64, complex: ei != 0 }, &xs[..])
            } else {
                (xs[0], &xs[1..])
            };
            for &x in rest {
                acc = if product { acc.mul(x) } else { acc.add(x) };
            }
            if *kind.get_or_insert(acc.complex) != acc.complex {
                return Ok(None);
            }
            out.push(acc.re);
            if acc.complex {
                out.push(acc.im);
            }
        }
    }
    Ok(Some(if kind == Some(true) { h.complex_array(&shape, &out) } else { h.real_array(&shape, &out) }))
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
        let Some(o) = operand(h, t, &mut dims) else { return Ok(None) };
        ops.push(o);
    }
    let Some(dims) = dims else { return Ok(None) };
    if ops.iter().any(Operand::is_complex) {
        return complex_fold(h, op, &ops, dims, tick);
    }
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
                    // invariant: complex operands took the complex path
                    Operand::Complexes(_) | Operand::Complex(..) | Operand::Gauss(..) => unreachable!("a real operand"),
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
    let (Some(x), Some(y)) = (operand(h, a, &mut dims), operand(h, b, &mut dims)) else { return Ok(None) };
    let Some(dims) = dims else { return Ok(None) };
    if x.is_complex() || y.is_complex() {
        return complex_fold(h, op, &[x, y], dims, tick);
    }
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
