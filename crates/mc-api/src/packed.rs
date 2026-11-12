//! Packed numeric arrays (semantics 11): a store may hold a rectangular `list` of machine numbers as one node with a
//! contiguous payload. A packed term is **observably its unpacked compound form**: it is the same handle as the
//! `list(...)` built element by element (hash-consing is canonical across both constructions), it has the same
//! structural hash, and [`Store::view`] shows it as [`View::App`] with head `list` and its elements (or rows) as
//! arguments. Natives that know about packing read the payload without unpacking through [`Store::packed`].
//!
//! **The packing rule** (decision N4a.2), the same for every store that packs: `list(a1, ..., an)` is packed exactly
//! when it is a rectangular array of rank `r` (1 to [`MAX_RANK`]) whose leaves are all machine reals, all machine
//! integers (integers that fit `i64`), or all machine complex numbers (`complex(re, im)` with machine-real parts,
//! decision sp-core.1), with at least [`PACK_MIN`] leaves. Ranks above 1 need every argument to be a
//! list of the same shape; an empty level makes an array with no leaves, which is never packed. [`pack_shape`] is
//! the rule written against views, the model the real store is tested against.
use crate::{names, Store, View, T};

/// The fewest leaves a packed array has (decision N4a.3, by measurement: below it the payload saves little and
/// natives that view the elements pay for unpacking).
pub const PACK_MIN: usize = 64;

/// The highest rank of a packed array; deeper rectangular lists stay compound terms.
pub const MAX_RANK: usize = 16;

/// The leaves of a packed array.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Elem {
    /// machine reals (IEEE binary64, identified by bit pattern like [`View::Real`])
    Real,
    /// machine integers: integers that fit `i64` ([`View::Int`])
    Int,
    /// machine complex numbers: `complex(re, im)` with machine-real parts, two payload words per leaf (the real part
    /// first)
    Complex,
}

impl Elem {
    /// Payload words per leaf.
    pub fn words(self) -> usize {
        match self {
            Elem::Real | Elem::Int => 1,
            Elem::Complex => 2,
        }
    }
}

/// A read-only view of a packed array's payload, borrowed from the store for `'h`: its dimensions (outermost first)
/// and its leaves in row-major order.
#[derive(Clone, Copy, Debug)]
pub enum Packed<'h> {
    /// an array of machine reals
    Reals {
        /// the dimensions, outermost first (all at least 1)
        dims: &'h [u64],
        /// the leaves, row-major
        data: &'h [f64],
    },
    /// an array of machine integers
    Ints {
        /// the dimensions, outermost first (all at least 1)
        dims: &'h [u64],
        /// the leaves, row-major
        data: &'h [i64],
    },
    /// an array of machine complex numbers
    Complexes {
        /// the dimensions, outermost first (all at least 1)
        dims: &'h [u64],
        /// the leaves, row-major, each as its real part followed by its imaginary part (twice as many as leaves)
        data: &'h [f64],
    },
}

impl<'h> Packed<'h> {
    /// The dimensions, outermost first.
    pub fn dims(&self) -> &'h [u64] {
        match *self {
            Packed::Reals { dims, .. } | Packed::Ints { dims, .. } | Packed::Complexes { dims, .. } => dims,
        }
    }
    /// The rank (number of dimensions, at least 1).
    pub fn rank(&self) -> usize {
        self.dims().len()
    }
    /// The number of arguments of the list: its first dimension.
    pub fn len(&self) -> usize {
        self.dims()[0] as usize
    }
    /// Never true: a packed array has at least [`PACK_MIN`] leaves.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    /// The number of leaves.
    pub fn count(&self) -> usize {
        match *self {
            Packed::Reals { data, .. } => data.len(),
            Packed::Ints { data, .. } => data.len(),
            Packed::Complexes { data, .. } => data.len() / 2,
        }
    }
    /// The kind of the leaves.
    pub fn elem(&self) -> Elem {
        match self {
            Packed::Reals { .. } => Elem::Real,
            Packed::Ints { .. } => Elem::Int,
            Packed::Complexes { .. } => Elem::Complex,
        }
    }
    /// The leaves of an array of machine reals.
    pub fn reals(&self) -> Option<&'h [f64]> {
        match *self {
            Packed::Reals { data, .. } => Some(data),
            _ => None,
        }
    }
    /// The leaves of an array of machine integers.
    pub fn ints(&self) -> Option<&'h [i64]> {
        match *self {
            Packed::Ints { data, .. } => Some(data),
            _ => None,
        }
    }
    /// The parts of an array of machine complex numbers: the real and imaginary part of each leaf in turn.
    pub fn complexes(&self) -> Option<&'h [f64]> {
        match *self {
            Packed::Complexes { data, .. } => Some(data),
            _ => None,
        }
    }
    /// The payload words of the leaves: a real by its bit pattern, an integer in two's complement, a complex
    /// number as its two parts.
    pub fn words(&self) -> &'h [u64] {
        match *self {
            Packed::Reals { data, .. } | Packed::Complexes { data, .. } => real_words(data),
            Packed::Ints { data, .. } => int_words(data),
        }
    }
    /// The leaves of a vector of machine reals (rank 1).
    pub fn real_vector(&self) -> Option<&'h [f64]> {
        self.reals().filter(|_| self.rank() == 1)
    }
    /// The leaves of a vector of machine integers (rank 1).
    pub fn int_vector(&self) -> Option<&'h [i64]> {
        self.ints().filter(|_| self.rank() == 1)
    }
}

/// Leaves as payload words: a real by its bit pattern, an integer in two's complement.
pub fn real_words(data: &[f64]) -> &[u64] {
    // SAFETY: `f64` and `u64` have the same size and alignment, and every bit pattern is valid for both.
    unsafe { std::slice::from_raw_parts(data.as_ptr() as *const u64, data.len()) }
}

/// See [`real_words`].
pub fn int_words(data: &[i64]) -> &[u64] {
    // SAFETY: as in `real_words`.
    unsafe { std::slice::from_raw_parts(data.as_ptr() as *const u64, data.len()) }
}

/// Payload words as reals (the inverse of [`real_words`]).
pub fn words_reals(w: &[u64]) -> &[f64] {
    // SAFETY: as in `real_words`.
    unsafe { std::slice::from_raw_parts(w.as_ptr() as *const f64, w.len()) }
}

/// Payload words as integers (the inverse of [`int_words`]).
pub fn words_ints(w: &[u64]) -> &[i64] {
    // SAFETY: as in `real_words`.
    unsafe { std::slice::from_raw_parts(w.as_ptr() as *const i64, w.len()) }
}

/// The shape and leaves of `list(args)` when the packing rule packs it: the kind of the leaves, the dimensions and
/// the leaves as payload words ([`real_words`], [`int_words`]). The arguments' own packed payloads are used when the
/// store has them; other arguments are walked with an explicit stack.
pub fn pack_shape<'h, S: Store + ?Sized>(st: &'h S, args: &[T<'h>]) -> Option<(Elem, Vec<u64>, Vec<u64>)> {
    let list = st.sym_term(st.sym(names::LIST));
    let first = *args.first()?;
    let (elem, inner, complex) = shape_of(st, list, first)?;
    if inner.len() + 1 > MAX_RANK {
        return None;
    }
    let per: u64 = inner.iter().product();
    let total = (args.len() as u64).checked_mul(per)?;
    if total < PACK_MIN as u64 {
        return None;
    }
    let mut data = Vec::with_capacity(total as usize * elem.words());
    let cx = Leaves { list, complex, elem };
    for &a in args {
        if !leaves_into(st, &cx, a, &inner, &mut data) {
            return None;
        }
    }
    let mut dims = Vec::with_capacity(inner.len() + 1);
    dims.push(args.len() as u64);
    dims.extend_from_slice(&inner);
    Some((elem, dims, data))
}

/// What a walk of an array's leaves looks for: the head `list`, the head `complex` (a complex leaf's, when the
/// first leaf is one; no symbol is made for it), and the kind of the leaves.
struct Leaves<'h> {
    list: T<'h>,
    complex: Option<T<'h>>,
    elem: Elem,
}

/// The head of `t` if `t` is a machine complex number `complex(re, im)`: both parts machine reals, the head the
/// symbol `complex`.
pub fn complex_head<'h, S: Store + ?Sized>(st: &'h S, t: T<'h>) -> Option<T<'h>> {
    match st.view(t) {
        View::App { head, args: &[re, im] } if matches!((st.view(re), st.view(im)), (View::Real(_), View::Real(_))) => {
            matches!(st.view(head), View::Sym(s) if st.sym_name(s) == names::COMPLEX).then_some(head)
        }
        _ => None,
    }
}

/// A leaf's payload words appended to `out` if it is of kind `cx.elem`.
fn leaf_into<'h, S: Store + ?Sized>(st: &'h S, cx: &Leaves<'h>, t: T<'h>, out: &mut Vec<u64>) -> bool {
    match (st.view(t), cx.elem) {
        (View::Real(v), Elem::Real) => out.push(v.to_bits()),
        (View::Int(v), Elem::Int) => out.push(v as u64),
        (View::App { head, args: &[re, im] }, Elem::Complex) if Some(head) == cx.complex => match (st.view(re), st.view(im)) {
            (View::Real(a), View::Real(b)) => out.extend_from_slice(&[a.to_bits(), b.to_bits()]),
            _ => return false,
        },
        _ => return false,
    }
    true
}

/// The kind of the leaves, the dimensions of the array `t` (empty for a leaf) and the head of complex leaves,
/// following first arguments only.
fn shape_of<'h, S: Store + ?Sized>(st: &'h S, list: T<'h>, mut t: T<'h>) -> Option<(Elem, Vec<u64>, Option<T<'h>>)> {
    let mut dims = vec![];
    loop {
        if let Some(p) = st.packed(t) {
            dims.extend_from_slice(p.dims());
            let complex = if p.elem() == Elem::Complex {
                // the head of the first leaf, reached through first arguments
                let mut u = t;
                loop {
                    match st.view(u) {
                        View::App { head, args } if head == list => u = args[0],
                        _ => break complex_head(st, u),
                    }
                }
            } else {
                None
            };
            return Some((p.elem(), dims, complex));
        }
        match st.view(t) {
            View::App { head, args } if head == list && !args.is_empty() && dims.len() < MAX_RANK => {
                dims.push(args.len() as u64);
                t = args[0];
            }
            View::App { .. } => return complex_head(st, t).map(|c| (Elem::Complex, dims, Some(c))),
            View::Real(_) => return Some((Elem::Real, dims, None)),
            View::Int(_) => return Some((Elem::Int, dims, None)),
            _ => return None,
        }
    }
}

/// Append the leaves of `t` to `out` if `t` is an array of kind `cx.elem` with dimensions `dims`.
fn leaves_into<'h, S: Store + ?Sized>(st: &'h S, cx: &Leaves<'h>, t: T<'h>, dims: &[u64], out: &mut Vec<u64>) -> bool {
    // (term, level) pairs still to visit, in reverse order
    let mut stack = vec![(t, 0usize)];
    while let Some((t, level)) = stack.pop() {
        if let Some(p) = st.packed(t) {
            if p.elem() != cx.elem || p.dims() != &dims[level..] {
                return false;
            }
            out.extend_from_slice(p.words());
            continue;
        }
        if level == dims.len() {
            if !leaf_into(st, cx, t, out) {
                return false;
            }
            continue;
        }
        match st.view(t) {
            View::App { head, args } if head == cx.list && args.len() as u64 == dims[level] => {
                stack.extend(args.iter().rev().map(|&a| (a, level + 1)));
            }
            _ => return false,
        }
    }
    true
}

/// `list(...)` of dimensions `dims` (outermost first) with the leaves `leaf(0..n)` in row-major order, built level by
/// level from the innermost (the default construction of [`Store::real_array`] and [`Store::int_array`]). Panics
/// unless the product of `dims` is `n`; an empty `dims` is the leaf itself (n = 1).
pub fn build_nested<'h, S: Store + ?Sized>(st: &'h S, dims: &[usize], n: usize, leaf: impl Fn(usize) -> T<'h>) -> T<'h> {
    let product = dims.iter().try_fold(1usize, |a, &d| a.checked_mul(d));
    assert_eq!(product, Some(n), "the dimensions {dims:?} do not hold {n} leaves");
    let list = st.sym(names::LIST);
    if dims.is_empty() {
        return leaf(0);
    }
    if let Some(z) = dims.iter().position(|&d| d == 0) {
        // no leaves: the levels above the first empty one still have their lengths
        let mut row = st.call(list, &[]);
        for &d in dims[..z].iter().rev() {
            row = st.call(list, &vec![row; d]);
        }
        return row;
    }
    let mut level: Vec<T<'h>> = (0..n).map(leaf).collect();
    for &d in dims[1..].iter().rev() {
        level = level.chunks(d).map(|c| st.call(list, c)).collect();
    }
    st.call(list, &level)
}
