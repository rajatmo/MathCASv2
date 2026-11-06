//! Vectors and matrices as arguments and results: reading a list (packed or not) into a shape, its entries and
//! their arithmetic domain (decision LA.1), and building result terms (packed whenever the store's rule packs).
use crate::dense::Mat;
use crate::scalar::Cx;
use mc_api::{names, Store, View};
use mc_arith::{Arith, Num, Real};
use mc_core::{Heap, T};
use std::borrow::Cow;

/// The arithmetic every entry of an argument can be computed in, from most to least specific. Mixed entries take
/// the least exact kind, as numbers do (exact < ball < machine), and any symbolic entry makes the whole matrix
/// symbolic.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Domain {
    /// integers and rationals
    Rational,
    /// exact complex numbers (Gaussian rationals) among them
    ExactComplex,
    /// balls (real or complex) and exact numbers
    Ball,
    /// machine reals (with exact numbers or balls, converted)
    Real,
    /// machine complex numbers among them
    Complex,
    /// some entry is not a number
    Symbolic,
}

impl Domain {
    /// Is it machine arithmetic?
    pub fn machine(self) -> bool {
        matches!(self, Domain::Real | Domain::Complex)
    }
    /// The domain of an operation on two arguments: symbolic if either is, machine if either is (complex when
    /// the other may hold complex numbers), otherwise the less exact of the two.
    pub fn join(self, o: Domain) -> Domain {
        use Domain::*;
        match (self, o) {
            (Symbolic, _) | (_, Symbolic) => Symbolic,
            (Complex, _) | (_, Complex) => Complex,
            (Real, Real) | (Real, Rational) | (Rational, Real) => Real,
            (Real, _) | (_, Real) => Complex,
            (a, b) => a.max(b),
        }
    }
    /// Is it exact numbers (no symbols)?
    pub fn exact_numeric(self) -> bool {
        matches!(self, Domain::Rational | Domain::ExactComplex)
    }
}

/// The shape of a tensor argument.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    /// a vector of n entries
    Vector(usize),
    /// a matrix of rows x columns
    Matrix(usize, usize),
}

/// The entries, row-major.
#[derive(Clone, Debug)]
pub enum Data<'h> {
    /// machine reals (a packed payload, or read from the terms)
    Reals(Cow<'h, [f64]>),
    /// a packed payload of machine integers
    Ints(&'h [i64]),
    /// the entry terms
    Terms(Vec<T<'h>>),
}

/// A vector or matrix argument.
#[derive(Clone, Debug)]
pub struct Arr<'h> {
    /// the shape
    pub shape: Shape,
    /// the entries, row-major
    pub data: Data<'h>,
    /// their domain
    pub domain: Domain,
}

/// Shared state of one native call: the canonicaliser and the name of the function (for failures).
pub struct Env<'h> {
    /// the canonicaliser (with the engine's stop flag)
    pub ar: Arith<'h, Heap>,
    /// the function's name
    pub name: &'static str,
    list: T<'h>,
}

impl<'h> Env<'h> {
    /// The environment of a call of `name`.
    pub fn new(ar: Arith<'h, Heap>, name: &'static str) -> Env<'h> {
        let h = ar.store();
        let list = h.sym_term(h.sym(names::LIST));
        Env { ar, name, list }
    }
    /// The store.
    pub fn h(&self) -> &'h Heap {
        self.ar.store()
    }
    /// A symbol term.
    pub fn sym(&self, name: &str) -> T<'h> {
        let h = self.h();
        h.sym_term(h.sym(name))
    }
    /// The call `f(args)`.
    pub fn call(&self, f: &str, args: &[T<'h>]) -> T<'h> {
        let h = self.h();
        h.call(h.sym(f), args)
    }
    /// The list of `items`.
    pub fn list(&self, items: &[T<'h>]) -> T<'h> {
        self.h().app(self.list, items)
    }
    /// The elements of a list (the payload is unpacked by the view only if it is packed).
    pub fn as_list(&self, t: T<'h>) -> Option<&'h [T<'h>]> {
        match self.h().view(t) {
            View::App { head, args } if head == self.list => Some(args),
            _ => None,
        }
    }
    /// `failure(tag, f, details...)`.
    pub fn failure(&self, tag: &str, details: &[T<'h>]) -> T<'h> {
        let mut a = vec![self.sym(tag), self.sym(self.name)];
        a.extend_from_slice(details);
        self.call(names::FAILURE, &a)
    }
    /// `failure(argument, f, k)`: argument k (1-based) is not what the function takes.
    pub fn bad_argument(&self, k: usize) -> T<'h> {
        self.failure("argument", &[self.h().int(k as i64)])
    }
    /// A machine integer argument.
    pub fn int(&self, t: T<'h>) -> Option<i64> {
        match self.h().view(t) {
            View::Int(v) => Some(v),
            _ => None,
        }
    }

    /// The term of a number.
    pub fn num(&self, x: &Num) -> T<'h> {
        self.ar.number(x)
    }
    /// The term of a machine complex number (a real when the imaginary part is zero).
    pub fn cx(&self, z: Cx) -> T<'h> {
        if z.im == 0.0 {
            self.h().real(z.re)
        } else {
            self.ar.number(&Num::Complex(Real::Machine(z.re), Real::Machine(z.im)))
        }
    }
    /// The matrix (list of rows) of `rows x cols` terms.
    pub fn matrix(&self, rows: usize, cols: usize, a: &[T<'h>]) -> T<'h> {
        let rs: Vec<T<'h>> = (0..rows).map(|i| self.list(&a[i * cols..(i + 1) * cols])).collect();
        self.list(&rs)
    }
    /// A matrix of machine reals (packed when large enough).
    pub fn real_matrix(&self, m: &Mat<f64>) -> T<'h> {
        if m.rows == 0 || m.cols == 0 {
            return self.matrix(m.rows, 0, &[]);
        }
        self.h().real_array(&[m.rows, m.cols], &m.a)
    }
    /// A matrix of machine complex numbers.
    pub fn cx_matrix(&self, m: &Mat<Cx>) -> T<'h> {
        if m.a.iter().all(|z| z.im == 0.0) {
            let re = Mat { rows: m.rows, cols: m.cols, a: m.a.iter().map(|z| z.re).collect() };
            return self.real_matrix(&re);
        }
        let a: Vec<T<'h>> = m.a.iter().map(|&z| self.cx(z)).collect();
        self.matrix(m.rows, m.cols, &a)
    }
    /// A vector of machine reals.
    pub fn real_vector(&self, v: &[f64]) -> T<'h> {
        self.h().reals(v)
    }
    /// A vector of machine complex numbers.
    pub fn cx_vector(&self, v: &[Cx]) -> T<'h> {
        if v.iter().all(|z| z.im == 0.0) {
            return self.real_vector(&v.iter().map(|z| z.re).collect::<Vec<f64>>());
        }
        let a: Vec<T<'h>> = v.iter().map(|&z| self.cx(z)).collect();
        self.list(&a)
    }

    /// Read a vector or matrix argument; `None` if `t` is not a list, is ragged, or has list entries.
    pub fn read(&self, t: T<'h>) -> Option<Arr<'h>> {
        let h = self.h();
        if let Some(p) = h.packed(t) {
            let dims = p.dims();
            let shape = match dims.len() {
                1 => Shape::Vector(dims[0] as usize),
                2 => Shape::Matrix(dims[0] as usize, dims[1] as usize),
                _ => return None,
            };
            return Some(match (p.reals(), p.ints()) {
                (Some(r), _) => Arr { shape, data: Data::Reals(Cow::Borrowed(r)), domain: Domain::Real },
                (_, Some(i)) => Arr { shape, data: Data::Ints(i), domain: Domain::Rational },
                _ => return None,
            });
        }
        let items = self.as_list(t)?;
        let rows: Option<Vec<&'h [T<'h>]>> = items.iter().map(|&x| self.as_list(x)).collect();
        let (shape, terms) = match rows {
            Some(rows) if !rows.is_empty() => {
                let c = rows[0].len();
                if rows.iter().any(|r| r.len() != c) {
                    return None;
                }
                (Shape::Matrix(rows.len(), c), rows.concat())
            }
            _ => (Shape::Vector(items.len()), items.to_vec()),
        };
        let mut domain = Domain::Rational;
        let mut machine = false;
        let mut complex = false;
        let mut ball = false;
        for &x in &terms {
            match self.ar.num(x) {
                Some(Num::Real(Real::Exact(_))) => {}
                Some(Num::Real(Real::Ball(..))) => ball = true,
                Some(Num::Real(Real::Machine(_))) => machine = true,
                Some(Num::Complex(Real::Exact(_), _)) => complex = true,
                Some(Num::Complex(Real::Ball(..), _)) => (ball, complex) = (true, true),
                Some(Num::Complex(Real::Machine(_), _)) => (machine, complex) = (true, true),
                None => {
                    if self.as_list(x).is_some() {
                        return None;
                    }
                    domain = Domain::Symbolic;
                }
            }
        }
        if domain != Domain::Symbolic {
            domain = match (machine, complex, ball) {
                (true, true, _) => Domain::Complex,
                (true, false, _) => Domain::Real,
                (false, _, true) => Domain::Ball,
                (false, true, false) => Domain::ExactComplex,
                (false, false, false) => Domain::Rational,
            };
        }
        if domain == Domain::Real {
            let v: Vec<f64> = terms.iter().map(|&x| self.ar.num(x).and_then(|n| n.as_real().map(|r| r.to_f64())).unwrap_or(f64::NAN)).collect();
            return Some(Arr { shape, data: Data::Reals(Cow::Owned(v)), domain });
        }
        Some(Arr { shape, data: Data::Terms(terms), domain })
    }
}

impl<'h> Arr<'h> {
    /// Rows (1 for a vector).
    pub fn rows(&self) -> usize {
        match self.shape {
            Shape::Vector(_) => 1,
            Shape::Matrix(r, _) => r,
        }
    }
    /// Columns (the length of a vector).
    pub fn cols(&self) -> usize {
        match self.shape {
            Shape::Vector(n) => n,
            Shape::Matrix(_, c) => c,
        }
    }
    /// Is it a matrix?
    pub fn is_matrix(&self) -> bool {
        matches!(self.shape, Shape::Matrix(..))
    }
    /// Is it a square matrix?
    pub fn is_square(&self) -> bool {
        matches!(self.shape, Shape::Matrix(r, c) if r == c)
    }
    /// The entries as machine reals (machine real domain or exact real entries converted), row-major.
    pub fn reals(&self, env: &Env<'h>) -> Mat<f64> {
        let a: Vec<f64> = match &self.data {
            Data::Reals(r) => r.to_vec(),
            Data::Ints(i) => i.iter().map(|&v| v as f64).collect(),
            Data::Terms(ts) => ts.iter().map(|&x| env.ar.num(x).map_or(f64::NAN, |n| n.as_real().map_or(f64::NAN, |r| r.to_f64()))).collect(),
        };
        Mat { rows: self.rows(), cols: self.cols(), a }
    }
    /// The entries as machine complex numbers.
    pub fn complex(&self, env: &Env<'h>) -> Mat<Cx> {
        let a: Vec<Cx> = match &self.data {
            Data::Reals(r) => r.iter().map(|&v| Cx::new(v, 0.0)).collect(),
            Data::Ints(i) => i.iter().map(|&v| Cx::new(v as f64, 0.0)).collect(),
            Data::Terms(ts) => ts
                .iter()
                .map(|&x| match env.ar.num(x) {
                    Some(Num::Real(r)) => Cx::new(r.to_f64(), 0.0),
                    Some(Num::Complex(a, b)) => Cx::new(a.to_f64(), b.to_f64()),
                    None => Cx::new(f64::NAN, f64::NAN),
                })
                .collect(),
        };
        Mat { rows: self.rows(), cols: self.cols(), a }
    }
    /// The entries as numbers (numeric domains only).
    pub fn nums(&self, env: &Env<'h>) -> Vec<Num> {
        match &self.data {
            Data::Reals(r) => r.iter().map(|&v| Num::Real(Real::Machine(v))).collect(),
            Data::Ints(i) => i.iter().map(|&v| Num::int(v)).collect(),
            // invariant: called only for numeric domains, whose entries are numbers
            Data::Terms(ts) => ts.iter().map(|&x| env.ar.num(x).expect("a numeric domain")).collect(),
        }
    }
    /// The entries as terms.
    pub fn terms(&self, env: &Env<'h>) -> Vec<T<'h>> {
        let h = env.h();
        match &self.data {
            Data::Reals(r) => r.iter().map(|&v| h.real(v)).collect(),
            Data::Ints(i) => i.iter().map(|&v| h.int(v)).collect(),
            Data::Terms(ts) => ts.clone(),
        }
    }
}
