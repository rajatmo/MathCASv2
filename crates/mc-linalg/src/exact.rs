//! Exact and symbolic elimination without fractions (decision LA.3).
//!
//! A matrix of rationals becomes a matrix of integers ([`Z`]), a matrix with symbolic entries a matrix of integer
//! polynomials ([`Poly`]) in the kernels of its entries (`mc-poly`'s reading: symbols and non-polynomial subterms
//! are variables, rational functions are read as quotients). Each row is multiplied by the least common multiple
//! of its denominators, then fraction-free Gauss-Jordan elimination (Bareiss's integer-preserving scheme, extended
//! to the rows above the pivot) keeps every entry a minor of the scaled matrix, so every division is exact and no
//! gcd is needed until the results are read back. The final entries are quotients reduced by one gcd each.
//!
//! Elimination decides zero by the polynomial being zero, which is right for independent kernels. When kernels
//! can interact (radicals, numeric kernels: `Ring::interacting`), a candidate pivot is also rejected when its term
//! is zero; the remaining gap is recorded in the notes (decision LA.4).
use mc_api::T;
use mc_arith::num::Rat;
use mc_arith::{Arith, Num};
use mc_core::Heap;
use mc_poly::convert::{Conv, Heads, Ring};
use mc_poly::gcd::gcd;
use mc_poly::poly::Poly;
use mc_poly::qpoly::QPoly;
use mc_poly::ratfun::{self, RatFun};
use mc_poly::work::{self, Meter, WorkResult};
use mc_poly::z::Z;

/// An entry of a fraction-free elimination: an integer, or an integer polynomial (never mixed in one matrix).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum E {
    /// an integer
    Z(Z),
    /// a polynomial
    P(Poly),
}

impl E {
    /// Is it zero (as a polynomial)?
    pub fn is_zero(&self) -> bool {
        match self {
            E::Z(z) => z.is_zero(),
            E::P(p) => p.is_zero(),
        }
    }
    /// The size that pivoting minimises.
    fn size(&self) -> u64 {
        match self {
            E::Z(z) => z.bits(),
            E::P(p) => p.len() as u64 * 64 + p.max_bits(),
        }
    }
    fn zero_like(&self) -> E {
        match self {
            E::Z(_) => E::Z(Z::ZERO),
            E::P(p) => E::P(Poly::zero(p.nvars())),
        }
    }
    fn neg(&self) -> E {
        match self {
            E::Z(z) => E::Z(z.neg()),
            E::P(p) => E::P(p.neg()),
        }
    }
    fn mul(&self, o: &E, m: &mut Meter) -> WorkResult<E> {
        Ok(match (self, o) {
            (E::Z(a), E::Z(b)) => {
                m.step(1 + (a.bits() + b.bits()) / 64)?;
                E::Z(a.mul(b))
            }
            (E::P(a), E::P(b)) => E::P(a.mul(b, m)?),
            // invariant: a matrix's entries are all integers or all polynomials (`E` is chosen once per matrix)
            _ => unreachable!("one kind of entry per matrix"),
        })
    }
    fn sub(&self, o: &E) -> E {
        match (self, o) {
            (E::Z(a), E::Z(b)) => E::Z(a.sub(b)),
            (E::P(a), E::P(b)) => E::P(a.sub(b)),
            // invariant: a matrix's entries are all integers or all polynomials (`E` is chosen once per matrix)
            _ => unreachable!("one kind of entry per matrix"),
        }
    }
    fn div_exact(&self, d: &E, m: &mut Meter) -> WorkResult<E> {
        Ok(match (self, d) {
            (E::Z(a), E::Z(b)) => {
                if b.is_one() {
                    return Ok(self.clone());
                }
                m.step(1 + a.bits() / 64)?;
                E::Z(a.div_exact(b))
            }
            (E::P(a), E::P(b)) => {
                if b.is_constant() && b.as_constant() == Some(Z::ONE) {
                    return Ok(self.clone());
                }
                E::P(work::exact(a.div_exact(b, m)?, "fraction-free elimination divides exactly")?)
            }
            // invariant: a matrix's entries are all integers or all polynomials (`E` is chosen once per matrix)
            _ => unreachable!("one kind of entry per matrix"),
        })
    }
    fn gcd(&self, o: &E, m: &mut Meter) -> WorkResult<E> {
        Ok(match (self, o) {
            (E::Z(a), E::Z(b)) => E::Z(a.gcd(b)),
            (E::P(a), E::P(b)) => E::P(gcd(a, b, m)?),
            // invariant: a matrix's entries are all integers or all polynomials (`E` is chosen once per matrix)
            _ => unreachable!("one kind of entry per matrix"),
        })
    }
    /// Is the sign (of the leading coefficient) negative?
    fn is_neg(&self) -> bool {
        match self {
            E::Z(z) => z.is_neg(),
            E::P(p) => !p.is_zero() && p.lc().is_neg(),
        }
    }
    fn is_one(&self) -> bool {
        match self {
            E::Z(z) => z.is_one(),
            E::P(p) => p.is_constant() && p.as_constant() == Some(Z::ONE),
        }
    }
}

/// A matrix ready for fraction-free elimination: row i is the input row times `scale[i]`.
#[derive(Clone, Debug)]
pub struct ExactMat {
    /// the scaled entries, row-major
    pub a: Vec<E>,
    /// rows
    pub rows: usize,
    /// columns
    pub cols: usize,
    /// the factor each row was multiplied by
    pub scale: Vec<E>,
}

impl ExactMat {
    /// Entry (i, j).
    pub fn at(&self, i: usize, j: usize) -> &E {
        &self.a[i * self.cols + j]
    }
}

/// The context of exact work: the canonicaliser, and for symbolic matrices the polynomial ring of the kernels.
pub struct Exact<'a, 'h> {
    /// the canonicaliser
    pub ar: &'a Arith<'h, Heap>,
    hd: Heads<'h>,
    /// the kernels, for symbolic matrices
    pub ring: Option<Ring<'h>>,
}

impl<'a, 'h> Exact<'a, 'h> {
    /// A context for the entries `terms` (and the extra variables `extra`, such as the variable of a
    /// characteristic polynomial): integers when every entry is rational and there are no extras, otherwise
    /// polynomials in the kernels.
    pub fn new(ar: &'a Arith<'h, Heap>, terms: &[T<'h>], extra: &[T<'h>], m: &mut Meter) -> WorkResult<Self> {
        let hd = Heads::new(ar.store());
        let rational = extra.is_empty() && terms.iter().all(|&t| ar.num(t).is_some_and(|x| x.as_rat().is_some()));
        let ring = if rational {
            None
        } else {
            let conv = Conv::rational(ar, &hd);
            Some(conv.ring(terms, extra, m)?.0)
        };
        Ok(Exact { ar, hd, ring })
    }

    fn conv(&self) -> Conv<'_, 'h, Heap> {
        Conv::rational(self.ar, &self.hd)
    }

    /// The number of polynomial variables (0 for integer matrices).
    pub fn nvars(&self) -> usize {
        self.ring.as_ref().map_or(0, |r| r.nvars())
    }

    /// The variable of a kernel term.
    pub fn var(&self, t: T<'h>) -> Option<usize> {
        self.ring.as_ref().and_then(|r| r.var(t))
    }

    /// Can products of kernels simplify?
    pub fn interacting(&self) -> bool {
        self.ring.as_ref().is_some_and(|r| r.interacting())
    }

    /// One entry as a quotient (numerator, denominator); `None` for a division by zero.
    fn quotient_of(&self, t: T<'h>, m: &mut Meter) -> WorkResult<Option<(E, E)>> {
        match &self.ring {
            None => {
                // invariant: without a ring the entries are rationals (how the matrix was read)
                let q = self.ar.num(t).and_then(|x| x.as_rat().cloned()).expect("rational entries");
                Ok(Some((E::Z(Z::from_int(q.num.clone())), E::Z(Z::from_nat(q.den.clone())))))
            }
            Some(ring) => match ratfun::read(&self.conv(), t, ring, m)? {
                Ok(f) => Ok(Some((E::P(f.num), E::P(f.den)))),
                Err(_) => Ok(None),
            },
        }
    }

    /// The `rows x cols` matrix of terms (row-major) with each row cleared of denominators; `None` if an entry
    /// divides by zero.
    pub fn read(&self, terms: &[T<'h>], rows: usize, cols: usize, m: &mut Meter) -> WorkResult<Option<ExactMat>> {
        let mut a = Vec::with_capacity(rows * cols);
        let mut scale = Vec::with_capacity(rows);
        for i in 0..rows {
            let mut row = Vec::with_capacity(cols);
            for &t in &terms[i * cols..(i + 1) * cols] {
                match self.quotient_of(t, m)? {
                    Some(q) => row.push(q),
                    None => return Ok(None),
                }
            }
            // the least common multiple of the denominators
            let mut l = self.one();
            for (_, d) in &row {
                if d.is_one() {
                    continue;
                }
                let g = l.gcd(d, m)?;
                l = l.mul(&d.div_exact(&g, m)?, m)?;
            }
            for (n, d) in row {
                a.push(if l.is_one() { n } else { n.mul(&l.div_exact(&d, m)?, m)? });
            }
            scale.push(l);
        }
        Ok(Some(ExactMat { a, rows, cols, scale }))
    }

    /// One of the entry kind of this context.
    pub fn one(&self) -> E {
        match &self.ring {
            None => E::Z(Z::ONE),
            Some(r) => E::P(Poly::constant(r.nvars(), Z::ONE)),
        }
    }

    /// The term of `num / den` (den nonzero), reduced.
    pub fn quotient(&self, num: &E, den: &E, m: &mut Meter) -> WorkResult<T<'h>> {
        if num.is_zero() {
            return Ok(self.ar.int(0));
        }
        let g = num.gcd(den, m)?;
        let (mut n, mut d) = if g.is_one() { (num.clone(), den.clone()) } else { (num.div_exact(&g, m)?, den.div_exact(&g, m)?) };
        if d.is_neg() {
            (n, d) = (n.neg(), d.neg());
        }
        match (n, d) {
            (E::Z(n), E::Z(d)) => Ok(self.ar.number(&Num::rat(Rat::new(n.to_int(), d.to_int())))),
            (E::P(n), E::P(d)) => {
                // invariant: polynomial entries are made only with a ring
                let ring = self.ring.as_ref().expect("polynomial entries have a ring");
                let conv = self.conv();
                match d.as_constant() {
                    Some(c) => conv.to_term(&QPoly::new(n, c), ring, m),
                    None => ratfun::to_term(&conv, &RatFun { num: n, den: d }, ring, m),
                }
            }
            // invariant: a matrix's entries are all integers or all polynomials (`E` is chosen once per matrix)
            _ => unreachable!("one kind of entry per matrix"),
        }
    }

    /// The product of `xs` (the row scales of a determinant).
    pub fn product(&self, xs: &[E], m: &mut Meter) -> WorkResult<E> {
        let mut p = self.one();
        for x in xs {
            if !x.is_one() {
                p = p.mul(x, m)?;
            }
        }
        Ok(p)
    }

    /// The integer polynomial of a polynomial entry with its variables as terms (for factoring).
    pub fn poly_term(&self, p: &Poly, m: &mut Meter) -> WorkResult<T<'h>> {
        // invariant: polynomial entries are made only with a ring
        let ring = self.ring.as_ref().expect("polynomial entries have a ring");
        self.conv().int_poly_term(p, ring, m)
    }

    /// Is the polynomial entry zero as a term (kernels that interact can make a nonzero polynomial zero)?
    fn value_is_zero(&self, e: &E, m: &mut Meter) -> WorkResult<bool> {
        match e {
            E::P(p) if self.interacting() => {
                let t = self.poly_term(p, m)?;
                Ok(self.ar.num(t).is_some_and(|x| x.is_exact_zero()))
            }
            _ => Ok(e.is_zero()),
        }
    }

    /// The store.
    pub fn heap(&self) -> &'h Heap {
        self.ar.store()
    }
}

/// The result of a fraction-free elimination.
#[derive(Clone, Debug)]
pub struct Ff {
    /// the eliminated entries; in Gauss-Jordan form entry (i, j) of the reduced row echelon form is
    /// `a[i][j] / pivot`
    pub a: Vec<E>,
    /// rows
    pub rows: usize,
    /// columns
    pub cols: usize,
    /// the pivot column of each of the first `pivots.len()` rows
    pub pivots: Vec<usize>,
    /// the last pivot: the common denominator (and, for a full-rank square matrix, the determinant up to sign
    /// and row scales)
    pub pivot: E,
    /// whether the rows were permuted an odd number of times
    pub neg: bool,
}

impl Ff {
    /// Entry (i, j).
    pub fn at(&self, i: usize, j: usize) -> &E {
        &self.a[i * self.cols + j]
    }
}

/// Fraction-free elimination of `x`, pivoting in the first `pivot_cols` columns: all rows (`jordan`: the reduced
/// row echelon form up to the common denominator) or only the rows below each pivot (Bareiss's determinant).
pub fn eliminate(ex: &Exact<'_, '_>, x: &ExactMat, pivot_cols: usize, jordan: bool, m: &mut Meter) -> WorkResult<Ff> {
    let (rows, cols) = (x.rows, x.cols);
    let mut a = x.a.clone();
    let mut prev = ex.one();
    let mut pivots = Vec::new();
    let mut neg = false;
    let mut r = 0;
    for c in 0..pivot_cols {
        if r == rows {
            break;
        }
        m.step((rows * cols) as u64 + 1)?;
        let mut best: Option<(usize, u64)> = None;
        for i in r..rows {
            let e = &a[i * cols + c];
            if e.is_zero() || best.is_some_and(|(_, s)| e.size() >= s) {
                continue;
            }
            if ex.value_is_zero(e, m)? {
                continue;
            }
            best = Some((i, e.size()));
        }
        let Some((p, _)) = best else { continue };
        if p != r {
            for j in 0..cols {
                a.swap(r * cols + j, p * cols + j);
            }
            neg = !neg;
        }
        let piv = a[r * cols + c].clone();
        let others: Vec<usize> = if jordan { (0..rows).filter(|&i| i != r).collect() } else { (r + 1..rows).collect() };
        for i in others {
            let f = a[i * cols + c].clone();
            for j in 0..cols {
                if j == c {
                    continue;
                }
                let x = &a[i * cols + j];
                let y = &a[r * cols + j];
                if x.is_zero() && (f.is_zero() || y.is_zero()) {
                    continue;
                }
                let mut v = piv.mul(x, m)?;
                if !f.is_zero() && !y.is_zero() {
                    v = v.sub(&f.mul(y, m)?);
                }
                a[i * cols + j] = v.div_exact(&prev, m)?;
            }
            a[i * cols + c] = piv.zero_like();
        }
        prev = piv;
        pivots.push(c);
        r += 1;
    }
    Ok(Ff { a, rows, cols, pivots, pivot: prev, neg })
}

/// The determinant of a square matrix of terms as a term.
pub fn det<'h>(ex: &Exact<'_, 'h>, x: &ExactMat, m: &mut Meter) -> WorkResult<T<'h>> {
    let n = x.rows;
    let f = eliminate(ex, x, n, false, m)?;
    if f.pivots.len() < n {
        return Ok(ex.ar.int(0));
    }
    let num = if f.neg { f.pivot.neg() } else { f.pivot.clone() };
    let den = ex.product(&x.scale, m)?;
    ex.quotient(&num, &den, m)
}

/// The reduced row echelon form entries `rows x cols` as terms, with the pivot columns.
pub fn rref_terms<'h>(ex: &Exact<'_, 'h>, f: &Ff, m: &mut Meter) -> WorkResult<Vec<T<'h>>> {
    let mut out = Vec::with_capacity(f.rows * f.cols);
    let rank = f.pivots.len();
    for i in 0..f.rows {
        for j in 0..f.cols {
            out.push(if i < rank { ex.quotient(f.at(i, j), &f.pivot, m)? } else { ex.ar.int(0) });
        }
    }
    Ok(out)
}

/// A null-space basis of the first `n` columns from a Gauss-Jordan elimination: one vector per free column (that
/// entry one), from the last free column to the first.
pub fn null_basis<'h>(ex: &Exact<'_, 'h>, f: &Ff, n: usize, m: &mut Meter) -> WorkResult<Vec<Vec<T<'h>>>> {
    let mut free: Vec<usize> = (0..n).filter(|c| !f.pivots.contains(c)).collect();
    free.reverse();
    let mut out = Vec::with_capacity(free.len());
    for &fc in &free {
        let mut v = vec![ex.ar.int(0); n];
        v[fc] = ex.ar.int(1);
        for (i, &p) in f.pivots.iter().enumerate() {
            v[p] = ex.quotient(&f.at(i, fc).neg(), &f.pivot, m)?;
        }
        out.push(v);
    }
    Ok(out)
}

/// The identity matrix in the entry kind of `ex` appended to the right of `x` (`[x | I]`).
pub fn augment_identity(ex: &Exact<'_, '_>, x: &ExactMat) -> ExactMat {
    let n = x.rows;
    let cols = x.cols + n;
    let zero = ex.one().zero_like();
    let mut a = Vec::with_capacity(n * cols);
    for i in 0..n {
        a.extend_from_slice(&x.a[i * x.cols..(i + 1) * x.cols]);
        for j in 0..n {
            a.push(if i == j { x.scale[i].clone() } else { zero.clone() });
        }
    }
    ExactMat { a, rows: n, cols, scale: x.scale.clone() }
}
