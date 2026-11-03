//! Conversion between terms and packed polynomials (semantics 11: a packed polynomial is observably its expanded
//! term form).
//!
//! A term is read as a polynomial in its **kernels**: the maximal subterms that are not sums, products, integer
//! powers or rational numbers (symbols, calls such as `sin(x)`, non-integer powers such as `x^(1/2)`, inexact and
//! complex numbers). A negative integer power `b^-n` is the kernel `b^-1` to the power n. Reading a term expands
//! it: sums, products and positive integer powers are computed in the packed form (decision K.2).
//!
//! The kernels of a ring are sorted by the canonical order; kernel i is variable i, and variable 0 is the most
//! significant in the monomial order. Writing a polynomial back builds every monomial through `mc-arith` and the
//! sum directly in canonical order when the kernels cannot interact (decision K.4); when they can (a kernel that
//! is a number or a power, whose products the canonicaliser may combine, as in `(x^(1/2))^2 = x`), the sum goes
//! through the canonicaliser, which collects the terms again.
use crate::poly::Poly;
use crate::qpoly::{rat_parts, QPoly};
use crate::work::{Halt, Meter, WorkResult};
use crate::z::Z;
use mc_api::{names, Store, View, T};
use mc_arith::num::Rat;
use mc_arith::{Arith, Num};
use rustc_hash::FxHashMap;

/// The heads conversion recognises.
pub struct Heads<'h> {
    /// `add`
    pub add: T<'h>,
    /// `mul`
    pub mul: T<'h>,
    /// `pow`
    pub pow: T<'h>,
    /// `list`
    pub list: T<'h>,
}

impl<'h> Heads<'h> {
    /// The heads of a store.
    pub fn new<S: Store + ?Sized>(st: &'h S) -> Heads<'h> {
        let s = |n: &str| st.sym_term(st.sym(n));
        Heads { add: s(names::ADD), mul: s(names::MUL), pow: s(names::POW), list: s(names::LIST) }
    }
}

/// How a term is read.
pub(crate) enum Node<'h> {
    Const(Rat),
    Add(&'h [T<'h>]),
    Mul(&'h [T<'h>]),
    Pow(T<'h>, u32),
    /// a negative integer power b^-n (only when reading rational functions)
    Inv(T<'h>, u32),
    /// a kernel to a positive power
    Kernel(T<'h>, u32),
}

/// The variables of a polynomial ring: kernel terms in canonical order.
#[derive(Clone, Debug)]
pub struct Ring<'h> {
    /// kernel i is variable i
    pub kernels: Vec<T<'h>>,
    index: FxHashMap<T<'h>, usize>,
    /// can products of kernels simplify (a numeric or power kernel)?
    interacting: bool,
}

impl<'h> Ring<'h> {
    /// The number of variables.
    pub fn nvars(&self) -> usize {
        self.kernels.len()
    }
    /// Can products of kernels simplify (so that the canonicaliser must collect the terms again)?
    pub fn interacting(&self) -> bool {
        self.interacting
    }
    /// The variable of a kernel term.
    pub fn var(&self, t: T<'h>) -> Option<usize> {
        self.index.get(&t).copied()
    }
    /// Is kernel `i` a number (an inexact or complex constant, not a variable)?
    pub fn is_numeric<S: Store + ?Sized>(&self, ar: &Arith<'h, S>, i: usize) -> bool {
        ar.num(self.kernels[i]).is_some()
    }
}

/// Reads terms as polynomials in one ring, and writes polynomials of that ring back.
pub struct Conv<'a, 'h, S: Store + ?Sized> {
    /// the canonicaliser
    pub ar: &'a Arith<'h, S>,
    /// the heads
    pub hd: &'a Heads<'h>,
    /// read negative powers as denominators (rational functions) instead of kernels
    rational: bool,
}

impl<'a, 'h, S: Store + ?Sized> Conv<'a, 'h, S> {
    /// A converter.
    pub fn new(ar: &'a Arith<'h, S>, hd: &'a Heads<'h>) -> Self {
        Conv { ar, hd, rational: false }
    }
    /// A converter for rational functions: `b^-n` is a denominator, not a kernel.
    pub fn rational(ar: &'a Arith<'h, S>, hd: &'a Heads<'h>) -> Self {
        Conv { ar, hd, rational: true }
    }

    fn st(&self) -> &'h S {
        self.ar.store()
    }

    /// The classification of one term.
    pub(crate) fn node(&self, t: T<'h>) -> Node<'h> {
        if let Some(x) = self.ar.num(t) {
            return match x.as_rat() {
                Some(q) => Node::Const(q.clone()),
                None => Node::Kernel(t, 1),
            };
        }
        let View::App { head, args } = self.st().view(t) else {
            return Node::Kernel(t, 1);
        };
        if head == self.hd.add {
            Node::Add(args)
        } else if head == self.hd.mul {
            Node::Mul(args)
        } else if head == self.hd.pow && args.len() == 2 {
            match self.st().view(args[1]) {
                View::Int(n) if n > 0 && n <= u32::MAX as i64 => Node::Pow(args[0], n as u32),
                View::Int(n) if n < 0 && n >= -(u32::MAX as i64) && self.rational => {
                    Node::Inv(args[0], n.unsigned_abs() as u32)
                }
                View::Int(n) if n < 0 && n >= -(u32::MAX as i64) => {
                    // b^-n is the kernel b^-1 to the power n
                    let k = self.ar.pow(args[0], self.st().int(-1));
                    Node::Kernel(k, n.unsigned_abs() as u32)
                }
                _ => Node::Kernel(t, 1),
            }
        } else {
            Node::Kernel(t, 1)
        }
    }

    /// The ring of the kernels of `terms` plus the kernels `extra` (variables the caller names). Also counts how
    /// often each compound subterm is reached, so that shared subterms are converted once.
    pub fn ring(
        &self,
        terms: &[T<'h>],
        extra: &[T<'h>],
        m: &mut Meter,
    ) -> WorkResult<(Ring<'h>, FxHashMap<T<'h>, u32>)> {
        let mut seen: FxHashMap<T<'h>, u32> = FxHashMap::default();
        let mut kernels: Vec<T<'h>> = vec![];
        let mut kset: FxHashMap<T<'h>, ()> = FxHashMap::default();
        let mut stack: Vec<T<'h>> = terms.to_vec();
        let mut add_kernel = |k: T<'h>, kernels: &mut Vec<T<'h>>| {
            if kset.insert(k, ()).is_none() {
                kernels.push(k);
            }
        };
        for &k in extra {
            add_kernel(k, &mut kernels);
        }
        while let Some(t) = stack.pop() {
            m.step(1)?;
            let count = seen.entry(t).or_insert(0);
            *count += 1;
            if *count > 1 {
                continue;
            }
            match self.node(t) {
                Node::Const(_) => {}
                Node::Kernel(k, _) => add_kernel(k, &mut kernels),
                Node::Add(xs) | Node::Mul(xs) => stack.extend_from_slice(xs),
                Node::Pow(b, _) | Node::Inv(b, _) => stack.push(b),
            }
        }
        kernels.sort_by(|&a, &b| self.ar.cmp(a, b));
        let index = kernels.iter().enumerate().map(|(i, &k)| (k, i)).collect();
        let interacting = kernels.iter().any(|&k| {
            self.ar.num(k).is_some() || matches!(self.st().view(k), View::App { head, .. } if head == self.hd.pow)
        });
        Ok((Ring { kernels, index, interacting }, seen))
    }

    /// The expanded polynomial of `t` in `ring` (which must contain its kernels); `shared` from [`Conv::ring`]
    /// marks subterms reached more than once (pass an empty map to convert without sharing).
    pub fn to_poly(
        &self,
        t: T<'h>,
        ring: &Ring<'h>,
        shared: &FxHashMap<T<'h>, u32>,
        m: &mut Meter,
    ) -> WorkResult<QPoly> {
        enum Task<'h> {
            Visit(T<'h>),
            Add(usize),
            Mul(usize),
            Pow(u32),
            Keep(T<'h>),
        }
        let nv = ring.nvars();
        let mut memo: FxHashMap<T<'h>, QPoly> = FxHashMap::default();
        let mut tasks = vec![Task::Visit(t)];
        let mut vals: Vec<QPoly> = vec![];
        while let Some(task) = tasks.pop() {
            m.step(1)?;
            match task {
                Task::Visit(t) => {
                    if let Some(v) = memo.get(&t) {
                        vals.push(v.clone());
                        continue;
                    }
                    if let Some(v) = self.monomial(t, ring) {
                        vals.push(v);
                        continue;
                    }
                    let keep = shared.get(&t).is_some_and(|&c| c > 1);
                    match self.node(t) {
                        Node::Const(q) => vals.push(QPoly::constant(nv, &q)),
                        Node::Kernel(k, e) => vals.push(self.kernel_power(ring, k, e)),
                        Node::Add(xs) => {
                            if let Some(v) = self.monomial_sum(xs, ring, m)? {
                                vals.push(v);
                                continue;
                            }
                            if keep {
                                tasks.push(Task::Keep(t));
                            }
                            tasks.push(Task::Add(xs.len()));
                            tasks.extend(xs.iter().rev().map(|&x| Task::Visit(x)));
                        }
                        Node::Mul(xs) => {
                            if keep {
                                tasks.push(Task::Keep(t));
                            }
                            tasks.push(Task::Mul(xs.len()));
                            tasks.extend(xs.iter().rev().map(|&x| Task::Visit(x)));
                        }
                        Node::Inv(..) => unreachable!("polynomial reading has no denominators"),
                        Node::Pow(b, e) => {
                            if keep {
                                tasks.push(Task::Keep(t));
                            }
                            tasks.push(Task::Pow(e));
                            tasks.push(Task::Visit(b));
                        }
                    }
                }
                Task::Add(0) => vals.push(QPoly::zero(nv)),
                Task::Mul(0) => vals.push(QPoly::from_poly(Poly::constant(nv, Z::ONE))),
                Task::Add(n) => {
                    let xs = vals.split_off(vals.len() - n);
                    vals.push(sum(xs));
                }
                Task::Mul(n) => {
                    let mut xs = vals.split_off(vals.len() - n);
                    // the small factors first: the intermediate products stay small
                    xs.sort_by_key(|p| p.num.len());
                    let mut acc = xs[0].clone();
                    for x in &xs[1..] {
                        acc = acc.mul(x, m)?;
                    }
                    vals.push(acc);
                }
                Task::Pow(e) => {
                    let b = vals.pop().expect("a base");
                    vals.push(b.pow(e as u64, m)?);
                }
                Task::Keep(t) => {
                    memo.insert(t, vals.last().expect("a value").clone());
                }
            }
        }
        Ok(vals.pop().expect("one value"))
    }

    fn kernel_power(&self, ring: &Ring<'h>, k: T<'h>, e: u32) -> QPoly {
        let mut ex = vec![0; ring.nvars()];
        ex[ring.var(k).expect("the ring holds every kernel")] = e;
        QPoly::from_poly(Poly::monomial(&ex, Z::ONE))
    }

    /// A product of a rational and kernel powers, read directly (the common shape of a term of a sum).
    fn monomial(&self, t: T<'h>, ring: &Ring<'h>) -> Option<QPoly> {
        let (e, n, d) = self.monomial_parts(t, ring)?;
        Some(QPoly { num: Poly::monomial(&e, n), den: d })
    }

    fn monomial_parts(&self, t: T<'h>, ring: &Ring<'h>) -> Option<(Vec<u32>, Z, Z)> {
        let mut e = vec![0u32; ring.nvars()];
        let (mut n, mut d) = (Z::ONE, Z::ONE);
        let factors: &[T<'h>] = match self.st().view(t) {
            View::App { head, args } if head == self.hd.mul => args,
            _ => std::slice::from_ref(&t),
        };
        for &f in factors {
            match self.node(f) {
                Node::Const(q) => {
                    let (a, b) = rat_parts(&q);
                    n = n.mul(&a);
                    d = d.mul(&b);
                }
                Node::Kernel(k, x) => {
                    let v = ring.var(k)?;
                    e[v] = e[v].checked_add(x)?;
                }
                Node::Pow(b, x) => match self.node(b) {
                    Node::Kernel(k, y) => {
                        let v = ring.var(k)?;
                        e[v] = e[v].checked_add(x.checked_mul(y)?)?;
                    }
                    _ => return None,
                },
                _ => return None,
            }
        }
        if n.is_zero() {
            return None;
        }
        // canonical products hold one reduced coefficient, so n/d is already reduced
        Some((e, n, d))
    }

    /// The terms of `t` when it is already a monomial or a sum of monomials: (exponents, numerator, denominator)
    /// per term, without a common denominator (reading P12's ten thousand rational coefficients needs no lcm).
    /// The monomials of a canonical sum are distinct when the kernels cannot interact.
    pub fn monomials(&self, t: T<'h>, ring: &Ring<'h>, m: &mut Meter) -> WorkResult<Option<Terms>> {
        let xs: &[T<'h>] = match self.st().view(t) {
            View::App { head, args } if head == self.hd.add => args,
            _ => std::slice::from_ref(&t),
        };
        let mut items = Vec::with_capacity(xs.len());
        for &x in xs {
            m.step(1)?;
            if self.ar.num(x).is_some_and(|q| q.is_exact_zero()) {
                continue;
            }
            match self.monomial_parts(x, ring) {
                Some(p) => items.push(p),
                None => return Ok(None),
            }
        }
        Ok(Some(Terms { nv: ring.nvars(), items }))
    }

    /// A sum whose arguments are all monomials, collected by one sort (P11's million-term sums).
    fn monomial_sum(&self, xs: &[T<'h>], ring: &Ring<'h>, m: &mut Meter) -> WorkResult<Option<QPoly>> {
        if xs.len() < 8 {
            return Ok(None);
        }
        let nv = ring.nvars();
        let mut exps: Vec<u32> = Vec::with_capacity(xs.len() * nv);
        let mut nums: Vec<Z> = Vec::with_capacity(xs.len());
        let mut dens: Vec<Z> = Vec::with_capacity(xs.len());
        for &x in xs {
            m.step(1)?;
            let Some((e, n, d)) = self.monomial_parts(x, ring) else {
                return Ok(None);
            };
            exps.extend_from_slice(&e);
            nums.push(n);
            dens.push(d);
        }
        // the common denominator
        let mut den = Z::ONE;
        for d in &dens {
            if !d.is_one() && !den.divisible(d) {
                den = den.mul(&d.div_exact(&den.gcd(d)));
            }
        }
        let mut order: Vec<usize> = (0..xs.len()).collect();
        let ex = |i: usize| &exps[i * nv..(i + 1) * nv];
        order.sort_unstable_by(|&i, &j| ex(j).cmp(ex(i)));
        m.step(xs.len() as u64 * 8)?;
        let mut oexps = Vec::with_capacity(exps.len());
        let mut ocoefs = Vec::with_capacity(xs.len());
        let mut k = 0;
        while k < order.len() {
            let i = order[k];
            let mut c = Z::ZERO;
            while k < order.len() && ex(order[k]) == ex(i) {
                let j = order[k];
                let scaled = if dens[j].is_one() { nums[j].mul(&den) } else { nums[j].mul(&den.div_exact(&dens[j])) };
                c = c.add(&scaled);
                k += 1;
            }
            if !c.is_zero() {
                oexps.extend_from_slice(ex(i));
                ocoefs.push(c);
            }
        }
        Ok(Some(QPoly::new(Poly::from_sorted(nv, oexps, ocoefs), den)))
    }

    // ------------------------------------------------------------------ back to terms

    /// The canonical term of a rational number.
    pub fn rat_term(&self, q: &Rat) -> T<'h> {
        self.ar.number(&Num::rat(q.clone()))
    }

    /// The canonical term of a polynomial.
    pub fn to_term(&self, p: &QPoly, ring: &Ring<'h>, m: &mut Meter) -> WorkResult<T<'h>> {
        let w = Writer::new(self, ring);
        let mut terms = Vec::with_capacity(p.num.len());
        for i in 0..p.num.len() {
            m.step(4)?;
            let num = p.num.coef(i);
            let q = if p.den.is_one() { Rat::from_int(num.to_int()) } else { p.coef(i) };
            let c = self.rat_term(&q);
            terms.push((w.monomial(p.num.exps(i), c), num));
        }
        m.step(p.num.len() as u64 * 16)?;
        if mc_arith::num::stopped() {
            return Err(Halt::Stopped);
        }
        Ok(w.sum(terms, &p.den))
    }

    /// The canonical term of an integer polynomial.
    pub fn int_poly_term(&self, p: &Poly, ring: &Ring<'h>, m: &mut Meter) -> WorkResult<T<'h>> {
        self.to_term(&QPoly::from_poly(p.clone()), ring, m)
    }
}

/// Builds the canonical terms of the monomials of one ring (decision K.4).
///
/// When the kernels cannot interact, the canonical product `c * x_i^e_i * ...` is built directly: the factors'
/// canonical order depends only on which kernels occur and whether each has exponent one or more (a power
/// `k^e` compares with every other factor as `k^2` does), so two ranks per kernel, computed once by the
/// canonical order, sort the factors of every monomial. The sum is sorted with keys that decide most
/// comparisons without walking the terms: the class and head of each monomial and its coefficient (all
/// coefficients share one denominator, so their numerators order them), falling back to the canonical order.
/// Debug builds check both against the canonicaliser.
struct Writer<'c, 'a, 'h, S: Store + ?Sized> {
    c: &'c Conv<'a, 'h, S>,
    ring: &'c Ring<'h>,
    /// `[rank of k_i, rank of k_i^2]` among all factors
    rank: Vec<[u32; 2]>,
    pows: std::cell::RefCell<FxHashMap<(usize, u32), T<'h>>>,
}

/// The sort key of a monomial term.
struct Key<'h, 'z> {
    t: T<'h>,
    /// 0 number, 2 symbol, 3 compound (the classes of the canonical order)
    class: u8,
    head: Option<T<'h>>,
    arity: usize,
    /// the numerator of a numeric first argument
    coef: Option<&'z Z>,
}

impl<'c, 'a, 'h, S: Store + ?Sized> Writer<'c, 'a, 'h, S> {
    fn new(c: &'c Conv<'a, 'h, S>, ring: &'c Ring<'h>) -> Self {
        let mut rank = vec![[0u32; 2]; ring.nvars()];
        if !ring.interacting {
            let mut cand: Vec<(T<'h>, usize, usize)> = Vec::with_capacity(2 * ring.nvars());
            for (i, &k) in ring.kernels.iter().enumerate() {
                cand.push((k, i, 0));
                cand.push((c.ar.pow(k, c.st().int(2)), i, 1));
            }
            cand.sort_by(|a, b| c.ar.cmp(a.0, b.0));
            for (r, &(_, i, j)) in cand.iter().enumerate() {
                rank[i][j] = r as u32;
            }
        }
        Writer { c, ring, rank, pows: Default::default() }
    }

    fn st(&self) -> &'h S {
        self.c.st()
    }

    /// The canonical monomial `coef * prod kernel_i^e_i`.
    fn monomial(&self, e: &[u32], coef: T<'h>) -> T<'h> {
        let one = self.st().int(1);
        if self.ring.interacting {
            let mut f = vec![coef];
            for (i, &x) in e.iter().enumerate() {
                if x > 0 {
                    f.push(self.c.ar.pow(self.ring.kernels[i], self.st().int(x as i64)));
                }
            }
            return self.c.ar.mul(&f);
        }
        let mut f: Vec<(u32, T<'h>)> = Vec::with_capacity(e.len());
        for (i, &x) in e.iter().enumerate() {
            if x == 1 {
                f.push((self.rank[i][0], self.ring.kernels[i]));
            } else if x > 1 {
                let p = *self
                    .pows
                    .borrow_mut()
                    .entry((i, x))
                    .or_insert_with(|| self.c.ar.pow(self.ring.kernels[i], self.st().int(x as i64)));
                f.push((self.rank[i][1], p));
            }
        }
        f.sort_unstable_by_key(|p| p.0);
        let mut args: Vec<T<'h>> = Vec::with_capacity(f.len() + 1);
        if coef != one || f.is_empty() {
            args.push(coef);
        }
        args.extend(f.iter().map(|p| p.1));
        let t = if args.len() == 1 { args[0] } else { self.st().app(self.c.hd.mul, &args) };
        debug_assert_eq!(t, {
            let mut g = vec![coef];
            g.extend(f.iter().map(|p| p.1));
            self.c.ar.mul(&g)
        });
        t
    }

    fn key<'z>(&self, t: T<'h>, num: &'z Z, den: &Z) -> Key<'h, 'z> {
        if self.c.ar.num(t).is_some() {
            return Key { t, class: 0, head: None, arity: 0, coef: Some(num) };
        }
        match self.st().view(t) {
            View::App { head, args } => {
                let coef = (head == self.c.hd.mul && num != den).then_some(num);
                Key { t, class: 3, head: Some(head), arity: args.len(), coef }
            }
            _ => Key { t, class: 2, head: None, arity: 0, coef: None },
        }
    }

    fn cmp(&self, a: &Key<'h, '_>, b: &Key<'h, '_>) -> std::cmp::Ordering {
        use std::cmp::Ordering;
        let o = a.class.cmp(&b.class);
        if o != Ordering::Equal || a.class != 3 {
            return o.then_with(|| self.c.ar.cmp(a.t, b.t));
        }
        let (ha, hb) = (a.head.expect("a compound"), b.head.expect("a compound"));
        if ha != hb {
            return self.c.ar.cmp(ha, hb);
        }
        let o = a.arity.cmp(&b.arity);
        if o != Ordering::Equal {
            return o;
        }
        match (a.coef, b.coef) {
            // numbers first; equal coefficients fall through to the full order
            (Some(x), Some(y)) => x.cmp_z(y).then_with(|| self.c.ar.cmp(a.t, b.t)),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => self.c.ar.cmp(a.t, b.t),
        }
    }

    /// The canonical sum of distinct monomials (with the numerators of their coefficients over `den`).
    fn sum(&self, terms: Vec<(T<'h>, &Z)>, den: &Z) -> T<'h> {
        if self.ring.interacting {
            let ts: Vec<T<'h>> = terms.iter().map(|p| p.0).collect();
            return self.c.ar.add(&ts);
        }
        match terms.len() {
            0 => self.st().int(0),
            1 => terms[0].0,
            _ => {
                let mut keys: Vec<Key<'h, '_>> = terms.iter().map(|&(t, n)| self.key(t, n, den)).collect();
                keys.sort_by(|a, b| self.cmp(a, b));
                let ts: Vec<T<'h>> = keys.iter().map(|k| k.t).collect();
                let t = self.st().app(self.c.hd.add, &ts);
                debug_assert!(ts.len() > 2000 || t == self.c.ar.add(&ts), "the direct sum is canonical");
                t
            }
        }
    }
}

/// A polynomial as a list of terms with their own denominators.
pub struct Terms {
    /// the number of variables
    pub nv: usize,
    /// (exponents, numerator, positive denominator)
    pub items: Vec<(Vec<u32>, Z, Z)>,
}

impl Terms {
    /// The terms of a packed polynomial.
    pub fn of_qpoly(p: &QPoly) -> Terms {
        Terms { nv: p.nvars(), items: p.num.terms().map(|(e, c)| (e.to_vec(), c.clone(), p.den.clone())).collect() }
    }
    /// The packed polynomial of the terms `keep` selects, each with exponents mapped by `strip`.
    pub fn select(&self, keep: impl Fn(&[u32]) -> bool, strip: impl Fn(&[u32]) -> Vec<u32>) -> QPoly {
        let chosen: Vec<&(Vec<u32>, Z, Z)> = self.items.iter().filter(|t| keep(&t.0)).collect();
        // the common denominator of the chosen terms only
        let mut den = Z::ONE;
        for (_, _, d) in &chosen {
            if *d != den && !den.divisible(d) {
                den = den.mul(&d.div_exact(&den.gcd(d)));
            }
        }
        let terms = chosen
            .iter()
            .map(|(e, n, d)| (strip(e), if *d == den { n.clone() } else { n.mul(&den.div_exact(d)) }))
            .collect();
        QPoly::new(Poly::from_terms(self.nv, terms), den)
    }
}

/// The sum of polynomials, added in a balanced tree (a long sum costs n log n, not n^2).
pub fn sum(mut xs: Vec<QPoly>) -> QPoly {
    while xs.len() > 1 {
        let mut next = Vec::with_capacity(xs.len().div_ceil(2));
        let mut it = xs.into_iter();
        while let Some(a) = it.next() {
            next.push(match it.next() {
                Some(b) => a.add(&b),
                None => a,
            });
        }
        xs = next;
    }
    xs.pop().expect("a sum has arguments")
}
