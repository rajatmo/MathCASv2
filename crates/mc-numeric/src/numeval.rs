//! Numerical evaluation of terms (semantics 2.6): `n(e)`, `n(e, d)`, and the refinement behind comparisons and
//! sign decisions.
//!
//! A term is read once into a [`Graph`] (an explicit stack, shared subterms once): every subterm built only from
//! real numbers, constants, `add`, `mul`, `pow`, the elementary functions and the registered library functions
//! ([`crate::library`]) is a numeric node. A maximal numeric
//! subterm is an *island*; the rest of the term (free symbols, other heads, lists) is kept, with each island
//! replaced by its value, so `n` maps over lists and through any structure (decision L.6).
//!
//! An island is evaluated with balls at a working precision p, or in machine arithmetic when it contains a machine
//! real (contagion, 2.2) or the target is a machine real. `n(e, d)` doubles p from d log2(10) + 16 bits until the
//! ball has d correct digits or p passes the cap, then gives `failure(precision_not_reached, best)` (spike 3
//! decision 3.6, decision L.7).
use crate::cball::{self, CBall};
use crate::consts::Const;
use crate::elementary as el;
use crate::fun::{Fun, Mach};
use crate::library::{self, LibRef};
use mc_api::{names, Store, Sym, View, T};
use mc_arith::num::{stopped, Ball};
use mc_arith::{Arith, Cmp3, Kind, Num, Real};
use mc_eval::Res;
use rustc_hash::FxHashMap;
use std::cmp::Ordering;

/// Guard bits of `n(e, d)`: the first working precision is d log2(10) + GUARD_BITS.
pub const GUARD_BITS: u64 = 16;
/// The cap of `n(e, d)`: max(CAP_FACTOR p0, p0 + CAP_EXTRA) bits for a first precision p0.
pub const CAP_FACTOR: u64 = 16;
/// See [`CAP_FACTOR`].
pub const CAP_EXTRA: u64 = 4096;
/// Comparisons and sign decisions refine from 64 bits up to this precision (or 4 times the precision of a ball in
/// the operands, if larger) before they stay undecided.
pub const COMPARE_CAP: u64 = 1 << 13;

/// The symbols numerical evaluation recognises, resolved once per store.
#[derive(Clone, Debug)]
pub struct Names {
    add: Sym,
    mul: Sym,
    pow: Sym,
    abs: Sym,
    consts: [(Sym, Const); 4],
    funs: [(Sym, Fun); 14],
}

impl Names {
    /// The symbols of `st`.
    pub fn new<S: Store + ?Sized>(st: &S) -> Names {
        Names {
            add: st.sym(names::ADD),
            mul: st.sym(names::MUL),
            pow: st.sym(names::POW),
            abs: st.sym("abs"),
            consts: Const::ALL.map(|c| (st.sym(c.name()), c)),
            funs: Fun::ALL.map(|f| (st.sym(f.name()), f)),
        }
    }
    /// The constant named by `s`.
    pub fn constant(&self, s: Sym) -> Option<Const> {
        self.consts.iter().find(|(x, _)| *x == s).map(|(_, c)| *c)
    }
    /// The function named by `s`.
    pub fn fun(&self, s: Sym) -> Option<Fun> {
        self.funs.iter().find(|(x, _)| *x == s).map(|(_, f)| *f)
    }
    /// The symbol of a constant.
    pub fn sym_of(&self, c: Const) -> Sym {
        self.consts[c as usize].0
    }
    /// The symbol of a function.
    pub fn fun_sym(&self, f: Fun) -> Sym {
        self.funs.iter().find(|(_, g)| *g == f).expect("every function has a symbol").0
    }
    /// `pow`.
    pub fn pow(&self) -> Sym {
        self.pow
    }
    /// `mul`.
    pub fn mul(&self) -> Sym {
        self.mul
    }
}

/// A numeric node; children have smaller indices.
#[derive(Clone, Debug)]
enum Node {
    Num(Real),
    /// a complex number (package N0C: complex islands)
    CNum(Real, Real),
    Const(Const),
    Add(Vec<u32>),
    Mul(Vec<u32>),
    Pow(u32, u32),
    Fun(Fun, u32),
    /// |x| (the absolute value is part of numerical evaluation, so it is refined with the rest: package 1C)
    Abs(u32),
    /// the k-th variable of [`Graph::build_with_vars`]
    Var(u32),
    /// a registered library function of its arguments (package SF, [`crate::library`])
    Lib(LibRef, Vec<u32>),
}

/// The numeric nodes of a term.
pub struct Graph<'h> {
    nodes: Vec<Node>,
    /// every subterm visited: its numeric node, or `None`
    index: FxHashMap<T<'h>, Option<u32>>,
}

/// One island, ready to evaluate at any precision.
pub struct Island {
    /// the reachable nodes in increasing order (children first)
    order: Vec<u32>,
    root: u32,
    /// a machine real inside: machine arithmetic
    machine: bool,
    /// the largest precision of a ball inside (0 if none)
    ball_prec: u64,
    /// a complex number, or a negative exact base to a non-integer exact power, inside: complex balls
    /// (decision N0C.4)
    complex: bool,
}

impl<'h> Graph<'h> {
    /// Reads `t` (iteratively; shared subterms once).
    pub fn build<S: Store + ?Sized>(st: &'h S, ar: &Arith<'h, S>, nm: &Names, t: T<'h>) -> Graph<'h> {
        Graph::build_with_vars(st, ar, nm, t, &[])
    }

    /// Reads `t` with the symbols `vars` as numeric variables (node k for `vars[k]`): an expression in given
    /// variables, to evaluate at many points with [`Graph::eval_complex_at`] without building number terms (for
    /// solvers and integrators, roadmap 07 N2). Islands containing variables have no value in
    /// [`Graph::eval_ball`] and give NaN in [`Graph::eval_machine`].
    pub fn build_with_vars<S: Store + ?Sized>(
        st: &'h S,
        ar: &Arith<'h, S>,
        nm: &Names,
        t: T<'h>,
        vars: &[Sym],
    ) -> Graph<'h> {
        let mut g = Graph { nodes: vec![], index: FxHashMap::default() };
        let mut stack = vec![(t, false)];
        while let Some((t, expanded)) = stack.pop() {
            if g.index.contains_key(&t) {
                continue;
            }
            if let Some(x) = ar.num(t) {
                let node = Some(match x {
                    Num::Real(r) => g.push(Node::Num(r)),
                    Num::Complex(re, im) => g.push(Node::CNum(re, im)),
                });
                g.index.insert(t, node);
                continue;
            }
            match st.view(t) {
                View::Sym(s) => {
                    let node = match vars.iter().position(|&v| v == s) {
                        Some(k) => Some(g.push(Node::Var(k as u32))),
                        None => nm.constant(s).map(|c| g.push(Node::Const(c))),
                    };
                    g.index.insert(t, node);
                }
                View::App { args, .. } if !expanded => {
                    stack.push((t, true));
                    stack.extend(args.iter().filter(|a| !g.index.contains_key(a)).map(|&a| (a, false)));
                }
                View::App { head, args } => {
                    let node = g.compound(st, nm, head, args);
                    g.index.insert(t, node);
                }
                _ => {
                    g.index.insert(t, None);
                }
            }
        }
        g
    }

    fn push(&mut self, n: Node) -> u32 {
        self.nodes.push(n);
        self.nodes.len() as u32 - 1
    }

    fn compound<S: Store + ?Sized>(&mut self, st: &'h S, nm: &Names, head: T<'h>, args: &[T<'h>]) -> Option<u32> {
        let View::Sym(s) = st.view(head) else {
            return None;
        };
        let kids: Option<Vec<u32>> = args.iter().map(|a| self.index[a]).collect();
        let kids = kids?;
        let node = if s == nm.add {
            Node::Add(kids)
        } else if s == nm.mul {
            Node::Mul(kids)
        } else if s == nm.pow && kids.len() == 2 {
            // a negative base to a non-integer exact power (a unit like (-1)^(1/3)) makes the island complex
            Node::Pow(kids[0], kids[1])
        } else if let (Some(f), [k]) = (nm.fun(s), kids.as_slice()) {
            Node::Fun(f, *k)
        } else if let (true, [k]) = (s == nm.abs, kids.as_slice()) {
            Node::Abs(*k)
        } else {
            let f = library::any().then(|| library::lookup(&st.sym_name(s), kids.len())).flatten()?;
            Node::Lib(f, kids)
        };
        Some(self.push(node))
    }

    /// The numeric node of a visited subterm.
    pub fn node(&self, t: T<'h>) -> Option<u32> {
        self.index.get(&t).copied().flatten()
    }

    /// Is the node a plain number (no computation)?
    pub fn is_number(&self, i: u32) -> bool {
        matches!(self.nodes[i as usize], Node::Num(_))
    }

    /// The island rooted at node `root`.
    pub fn island(&self, root: u32) -> Island {
        let mut seen = vec![false; root as usize + 1];
        let mut stack = vec![root];
        let (mut machine, mut ball_prec, mut complex) = (false, 0, false);
        while let Some(i) = stack.pop() {
            if std::mem::replace(&mut seen[i as usize], true) {
                continue;
            }
            match &self.nodes[i as usize] {
                Node::Num(Real::Machine(_)) => machine = true,
                Node::Num(Real::Ball(_, p)) => ball_prec = ball_prec.max(*p),
                Node::CNum(re, im) => {
                    complex = true;
                    for r in [re, im] {
                        match r {
                            Real::Machine(_) => machine = true,
                            Real::Ball(_, p) => ball_prec = ball_prec.max(*p),
                            Real::Exact(_) => {}
                        }
                    }
                }
                Node::Num(_) | Node::Const(_) | Node::Var(_) => {}
                Node::Add(k) | Node::Mul(k) | Node::Lib(_, k) => stack.extend(k),
                Node::Pow(a, b) => {
                    if let (Node::Num(Real::Exact(x)), Node::Num(Real::Exact(e))) =
                        (&self.nodes[*a as usize], &self.nodes[*b as usize])
                    {
                        complex |= x.num.neg && !e.is_int();
                    }
                    stack.extend([a, b]);
                }
                Node::Fun(_, a) | Node::Abs(a) => stack.push(*a),
            }
        }
        let order = (0..=root).filter(|&i| seen[i as usize]).collect();
        Island { order, root, machine, ball_prec, complex }
    }

    /// The island's value as a ball at working precision `p`, or `None` (a domain or undecided case, a complex
    /// island, or a raised stop flag).
    pub fn eval_ball(&self, is: &Island, p: u64) -> Option<Ball> {
        if is.complex {
            return None;
        }
        let mut vals: FxHashMap<u32, Ball> = FxHashMap::default();
        for &i in &is.order {
            if stopped() {
                return None;
            }
            let v = match &self.nodes[i as usize] {
                Node::Num(Real::Ball(b, _)) => b.clone(),
                Node::Num(r) => match r.to_kind(Kind::Ball, p) {
                    Real::Ball(b, _) => b,
                    _ => return None,
                },
                Node::Const(c) => c.ball(p)?,
                Node::CNum(..) | Node::Var(_) => return None,
                Node::Add(k) => {
                    let mut s = Ball::from_i64(0);
                    for j in k {
                        s = s.add(&vals[j], p);
                    }
                    s
                }
                Node::Mul(k) => {
                    let mut s = Ball::from_i64(1);
                    for j in k {
                        s = s.mul(&vals[j], p);
                    }
                    s
                }
                Node::Pow(a, b) => self.pow_ball(&vals, *a, *b, p)?,
                Node::Fun(f, a) => f.ball(&vals[a], p)?,
                // |x| over [m - r, m + r] lies in [|m| - r, |m| + r], also when the ball contains zero
                Node::Abs(a) => {
                    let x = &vals[a];
                    Ball { mid: x.mid.abs(), rad: x.rad }
                }
                Node::Lib(f, k) => {
                    let args: Vec<Real> = k.iter().map(|j| self.lib_arg(*j, vals[j].clone(), p)).collect();
                    let z = f.0.ball(&args, p)?;
                    if !z.is_real() {
                        return None;
                    }
                    z.re
                }
            };
            vals.insert(i, v);
        }
        vals.remove(&is.root)
    }

    /// A library function's argument: an exact number as itself (the function rounds it at its own precision),
    /// anything else as its ball.
    fn lib_arg(&self, j: u32, b: Ball, p: u64) -> Real {
        match &self.nodes[j as usize] {
            Node::Num(Real::Exact(q)) => Real::Exact(q.clone()),
            _ => Real::Ball(b, p),
        }
    }

    fn pow_ball(&self, vals: &FxHashMap<u32, Ball>, a: u32, b: u32, p: u64) -> Option<Ball> {
        let x = &vals[&a];
        match &self.nodes[b as usize] {
            Node::Num(Real::Exact(q)) if q.is_int() => el::pow_int(x, &q.num, p),
            Node::Num(Real::Exact(q)) if q.den.limbs == [2] => el::pow_int(&el::sqrt(x, p + 8)?, &q.num, p),
            _ if matches!(self.nodes[a as usize], Node::Const(Const::E)) => el::exp(&vals[&b], p),
            _ => el::pow(x, &vals[&b], p),
        }
    }

    /// The island's value as a complex ball at working precision `p` (principal branches, [`crate::cball`]), or
    /// `None` (no value on the region: a point, pole or cut, or a raised stop flag). Real subterms inside their
    /// real domains keep exact zero imaginary parts, so a real island gives the same value as [`Graph::eval_ball`].
    pub fn eval_complex(&self, is: &Island, p: u64) -> Option<CBall> {
        self.eval_complex_at(is, p, &[])
    }

    /// [`Graph::eval_complex`] with the variables of [`Graph::build_with_vars`] bound to `args` (a variable
    /// without an argument: `None`). The result encloses f(z) for every z in the argument regions.
    pub fn eval_complex_at(&self, is: &Island, p: u64, args: &[CBall]) -> Option<CBall> {
        let ball = |r: &Real| match r {
            Real::Ball(b, _) => Some(b.clone()),
            r => match r.to_kind(Kind::Ball, p) {
                Real::Ball(b, _) => Some(b),
                _ => None,
            },
        };
        let mut vals: FxHashMap<u32, CBall> = FxHashMap::default();
        for &i in &is.order {
            if stopped() {
                return None;
            }
            let v = match &self.nodes[i as usize] {
                Node::Num(r) => CBall::real(ball(r)?),
                Node::CNum(re, im) => CBall::new(ball(re)?, ball(im)?),
                Node::Const(c) => CBall::real(c.ball(p)?),
                Node::Add(k) => {
                    let mut s = CBall::from_i64(0);
                    for j in k {
                        s = s.add(&vals[j], p);
                    }
                    s
                }
                Node::Mul(k) => {
                    let mut s = CBall::from_i64(1);
                    for j in k {
                        s = s.mul(&vals[j], p);
                    }
                    s
                }
                Node::Pow(a, b) => {
                    let z = &vals[a];
                    match &self.nodes[*b as usize] {
                        Node::Num(Real::Exact(q)) => cball::pow_rat(z, q, p)?,
                        _ if matches!(self.nodes[*a as usize], Node::Const(Const::E)) => cball::exp(&vals[b], p)?,
                        _ => cball::pow(z, &vals[b], p)?,
                    }
                }
                Node::Fun(f, a) => f.cball(&vals[a], p)?,
                Node::Abs(a) => CBall::real(cball::abs(&vals[a], p)?),
                Node::Var(k) => args.get(*k as usize)?.clone(),
                // library functions of real arguments only (complex arguments have no value here)
                Node::Lib(f, k) => {
                    let mut xs = Vec::with_capacity(k.len());
                    for j in k {
                        let z = &vals[j];
                        if !z.is_real() {
                            return None;
                        }
                        xs.push(self.lib_arg(*j, z.re.clone(), p));
                    }
                    f.0.ball(&xs, p)?
                }
            };
            vals.insert(i, v);
        }
        vals.remove(&is.root)
    }

    /// The island's value in machine arithmetic.
    pub fn eval_machine(&self, is: &Island) -> Mach {
        let mut vals: FxHashMap<u32, Mach> = FxHashMap::default();
        for &i in &is.order {
            let v = match &self.nodes[i as usize] {
                Node::Num(r) => Mach::Real(r.to_f64()),
                Node::CNum(re, im) => Mach::Complex(crate::machine::C64::new(re.to_f64(), im.to_f64())),
                Node::Var(_) => Mach::Real(f64::NAN),
                Node::Const(c) => Mach::Real(c.machine()),
                Node::Add(k) => fold(k.iter().map(|j| vals[j]), 0.0, |a, b| a + b, |a, b| a + b),
                Node::Mul(k) => fold(k.iter().map(|j| vals[j]), 1.0, |a, b| a * b, |a, b| a * b),
                Node::Pow(a, b) => {
                    let to_num = |m: Mach| match m {
                        Mach::Real(v) => Num::Real(Real::Machine(v)),
                        Mach::Complex(z) => Num::Complex(Real::Machine(z.re), Real::Machine(z.im)),
                    };
                    match to_num(vals[a]).pow_machine(&to_num(vals[b])) {
                        Num::Real(r) => Mach::Real(r.to_f64()),
                        Num::Complex(re, im) => Mach::Complex(crate::machine::C64::new(re.to_f64(), im.to_f64())),
                    }
                }
                Node::Fun(f, a) => f.machine(vals[a]),
                Node::Abs(a) => match vals[a] {
                    Mach::Real(v) => Mach::Real(v.abs()),
                    Mach::Complex(z) => Mach::Real(z.re.hypot(z.im)),
                },
                Node::Lib(f, k) => {
                    let xs: Option<Vec<f64>> = k
                        .iter()
                        .map(|j| match vals[j] {
                            Mach::Real(v) => Some(v),
                            Mach::Complex(_) => None,
                        })
                        .collect();
                    xs.map_or(Mach::Real(f64::NAN), |xs| f.0.machine(&xs))
                }
            };
            vals.insert(i, v);
        }
        vals[&is.root]
    }
}

/// A sum or product of machine numbers, real while every operand is.
fn fold(
    it: impl Iterator<Item = Mach>,
    unit: f64,
    real: fn(f64, f64) -> f64,
    complex: fn(crate::machine::C64, crate::machine::C64) -> crate::machine::C64,
) -> Mach {
    it.fold(Mach::Real(unit), |acc, x| match (acc, x) {
        (Mach::Real(a), Mach::Real(b)) => Mach::Real(real(a, b)),
        (a, b) => Mach::Complex(complex(a.c64(), b.c64())),
    })
}

impl Island {
    /// Does the island compute in machine arithmetic?
    pub fn is_machine(&self) -> bool {
        self.machine
    }
    /// Does the island compute with complex balls from the start (a complex number or a complex unit inside)?
    pub fn is_complex(&self) -> bool {
        self.complex
    }
}

/// A machine number as a term.
pub fn mach_term<'h, S: Store + ?Sized>(ar: &Arith<'h, S>, m: Mach) -> T<'h> {
    match m {
        Mach::Real(v) => ar.number(&Num::Real(Real::Machine(v))),
        Mach::Complex(z) => ar.number(&Num::Complex(Real::Machine(z.re), Real::Machine(z.im))),
    }
}

/// A ball at precision p as a term.
pub fn ball_term<'h, S: Store + ?Sized>(ar: &Arith<'h, S>, b: Ball, p: u64) -> T<'h> {
    ar.number(&Num::Real(Real::Ball(b, p)))
}

/// The principal power b^e of two numbers with at least one ball among their parts and no machine real, where
/// `mc-arith` has no algorithm (its decision B.13: a ball to a non-half-integer or ball exponent, a complex ball to
/// a non-integer exponent). Computed with complex balls at the largest ball precision of the operands ([`cball::pow`],
/// [`cball::pow_rat`] for an exact exponent); `None` when there is no enclosure (a base region touching the
/// negative real axis, 0 to a power whose real part is not positive) or a raised stop flag (decision N0C.5).
pub fn ball_power(b: &Num, e: &Num) -> Option<Num> {
    let parts = |x: &Num| match x {
        Num::Real(r) => [r.clone(), Real::int(0)],
        Num::Complex(re, im) => [re.clone(), im.clone()],
    };
    let all: Vec<Real> = parts(b).into_iter().chain(parts(e)).collect();
    if all.iter().any(|r| r.kind() == Kind::Machine) {
        return None;
    }
    let p = all.iter().filter_map(Real::prec).max()?;
    let ball = |r: &Real| match r.to_kind(Kind::Ball, p) {
        Real::Ball(x, _) => Some(x),
        _ => None,
    };
    let [a, c] = parts(b);
    let z = CBall::new(ball(&a)?, ball(&c)?);
    let r = match e.as_rat() {
        Some(q) => cball::pow_rat(&z, q, p)?,
        None => {
            let [x, y] = parts(e);
            cball::pow(&z, &CBall::new(ball(&x)?, ball(&y)?), p)?
        }
    };
    if stopped() {
        return None;
    }
    Some(if r.is_real() {
        Num::Real(Real::Ball(r.re, p))
    } else {
        Num::Complex(Real::Ball(r.re, p), Real::Ball(r.im, p))
    })
}

/// A complex ball at precision p as a term: a real ball when the imaginary part is an exact zero.
pub fn cball_term<'h, S: Store + ?Sized>(ar: &Arith<'h, S>, z: CBall, p: u64) -> T<'h> {
    if z.is_real() {
        return ball_term(ar, z.re, p);
    }
    ar.number(&Num::Complex(Real::Ball(z.re, p), Real::Ball(z.im, p)))
}

/// What `n` aims for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    /// `n(e)`: machine reals.
    Machine,
    /// `n(e, d)`: balls with at least d correct digits.
    Digits(u64),
}

/// The result of numerical evaluation: the term with every island replaced, and whether every island reached the
/// target precision.
pub struct Numeric<'h> {
    /// the evaluated term (with the best values found where the target was not reached)
    pub term: T<'h>,
    /// every island reached its target
    pub reached: bool,
    /// the term is a number
    pub is_number: bool,
}

/// Numerical evaluation of `t` (semantics 2.6). `tick(n)` charges fuel and polls the interrupt between rounds.
pub fn numericize<'h, S: Store + ?Sized>(
    st: &'h S,
    ar: &Arith<'h, S>,
    nm: &Names,
    t: T<'h>,
    target: Target,
    tick: &mut dyn FnMut(u64) -> Res<()>,
) -> Res<Numeric<'h>> {
    let g = Graph::build(st, ar, nm, t);
    let mut reached = true;
    let mut done: FxHashMap<T<'h>, T<'h>> = FxHashMap::default();
    let mut stack = vec![(t, false)];
    while let Some((u, expanded)) = stack.pop() {
        if done.contains_key(&u) {
            continue;
        }
        if let Some(i) = g.node(u) {
            let (v, ok) = island_value(&g, ar, i, u, target, tick)?;
            reached &= ok;
            done.insert(u, v);
            continue;
        }
        let v = match st.view(u) {
            View::App { args, .. } if !expanded => {
                stack.push((u, true));
                stack.extend(args.iter().filter(|a| !done.contains_key(a)).map(|&a| (a, false)));
                continue;
            }
            View::App { head, args } => {
                let mut new: Vec<T<'h>> = args.iter().map(|a| done[a]).collect();
                // an exact exponent outside an island stays exact: x^-1 with x a ball is a ball, x^(-1.000~) is
                // not evaluated (package 1C)
                if let (true, [_, e]) = (head == st.sym_term(nm.pow), args) {
                    if ar.num(*e).is_some_and(|x| x.is_exact()) {
                        new[1] = *e;
                    }
                }
                if new.as_slice() == args {
                    u
                } else {
                    st.app(head, &new)
                }
            }
            _ => u,
        };
        done.insert(u, v);
    }
    let term = done[&t];
    Ok(Numeric { term, reached, is_number: ar.num(term).is_some() })
}

/// d log2(10) + guard bits.
pub fn first_precision(d: u64) -> u64 {
    mc_arith::text::digits_to_bits(d) + GUARD_BITS
}

/// The cap for a first precision p0, never beyond the size limit of exact numbers (but at least p0).
pub fn cap(p0: u64) -> u64 {
    p0.saturating_mul(CAP_FACTOR).max(p0 + CAP_EXTRA).min(mc_arith::DEFAULT_MAX_BITS.max(p0))
}

/// The value of island `i` (the subterm `u`) and whether it reached the target.
fn island_value<'h, S: Store + ?Sized>(
    g: &Graph<'h>,
    ar: &Arith<'h, S>,
    i: u32,
    u: T<'h>,
    target: Target,
    tick: &mut dyn FnMut(u64) -> Res<()>,
) -> Res<(T<'h>, bool)> {
    let is = g.island(i);
    let d = match target {
        Target::Digits(d) if !is.machine => d,
        _ => {
            tick(is.order.len() as u64)?;
            return Ok((mach_term(ar, g.eval_machine(&is)), true));
        }
    };
    let p0 = first_precision(d);
    let cap = cap(p0);
    let digits = d.min(u32::MAX as u64) as u32;
    let mut p = p0;
    let mut best = None;
    // real balls first; an island that is complex, or whose real evaluation fails where the complex one succeeds
    // (an argument provably outside a real domain: log(-2), asin(2)), continues with complex balls (N0C.4)
    let mut complex = is.complex;
    loop {
        tick(is.order.len() as u64 + p / 1024)?;
        let v = if complex {
            g.eval_complex(&is, p).map(Err)
        } else {
            match g.eval_ball(&is, p) {
                Some(b) => Some(Ok(b)),
                None if stopped() => None,
                None => {
                    let z = g.eval_complex(&is, p);
                    complex = z.is_some();
                    z.map(Err)
                }
            }
        };
        if stopped() {
            // the evaluator's tick turns the interrupt into its failure; this value is discarded
            tick(0)?;
            return Ok((u, false));
        }
        match v {
            Some(Ok(b)) => {
                if b.has_digits(digits) {
                    let (b, q) = if p > p0 { (el::round(b, p0), p0) } else { (b, p) };
                    return Ok((ball_term(ar, b, q), true));
                }
                best = Some(ball_term(ar, b, p));
            }
            Some(Err(z)) => {
                if z.has_digits(digits) {
                    let (z, q) = if p > p0 { (z.round(p0), p0) } else { (z, p) };
                    return Ok((cball_term(ar, z, q), true));
                }
                best = Some(cball_term(ar, z, p));
            }
            None => {}
        }
        if p.saturating_mul(2) > cap {
            return Ok((best.unwrap_or(u), false));
        }
        p *= 2;
    }
}

/// Three-valued comparison of two closed numeric terms by refinement (semantics 2.5, decision L.8): `None` unless
/// both are numeric and at least one needs computing; `Some(Cmp3::Undecided)` when the balls still overlap at the
/// cap. Machine reals inside mean machine arithmetic; balls inside fix the accuracy that refinement can reach.
pub fn compare<'h, S: Store + ?Sized>(st: &'h S, ar: &Arith<'h, S>, nm: &Names, a: T<'h>, b: T<'h>) -> Option<Cmp3> {
    let pair = st.app(st.sym_term(st.sym(names::LIST)), &[a, b]);
    let g = Graph::build(st, ar, nm, pair);
    let (ia, ib) = (g.node(a)?, g.node(b)?);
    if g.is_number(ia) && g.is_number(ib) {
        return None;
    }
    let (xa, xb) = (g.island(ia), g.island(ib));
    if xa.complex || xb.complex {
        return None;
    }
    if xa.machine || xb.machine {
        return match (g.eval_machine(&xa), g.eval_machine(&xb)) {
            (Mach::Real(x), Mach::Real(y)) => Some(x.partial_cmp(&y).map_or(Cmp3::Unordered, from_ordering)),
            _ => None,
        };
    }
    let fixed = xa.ball_prec.max(xb.ball_prec);
    let cap = COMPARE_CAP.max(4 * fixed);
    let mut p = 64u64.max(fixed);
    loop {
        if let (Some(x), Some(y)) = (g.eval_ball(&xa, p), g.eval_ball(&xb, p)) {
            if let Some(o) = x.cmp3(&y) {
                return Some(from_ordering(o));
            }
        }
        if stopped() || 2 * p > cap {
            return Some(Cmp3::Undecided);
        }
        p *= 2;
    }
}

fn from_ordering(o: Ordering) -> Cmp3 {
    match o {
        Ordering::Less => Cmp3::Less,
        Ordering::Equal => Cmp3::Equal,
        Ordering::Greater => Cmp3::Greater,
    }
}

/// The sign of a closed real numeric term, decided by refinement: `Some(Ordering)` against zero, `None` when it
/// is not a real numeric term or not decided.
pub fn sign<'h, S: Store + ?Sized>(st: &'h S, ar: &Arith<'h, S>, nm: &Names, t: T<'h>) -> Option<Ordering> {
    if let Some(Num::Real(x)) = ar.num(t) {
        return match mc_arith::number::compare(&x, &Real::int(0)) {
            Cmp3::Less => Some(Ordering::Less),
            Cmp3::Equal => Some(Ordering::Equal),
            Cmp3::Greater => Some(Ordering::Greater),
            _ => None,
        };
    }
    match compare(st, ar, nm, t, st.int(0))? {
        Cmp3::Less => Some(Ordering::Less),
        Cmp3::Equal => Some(Ordering::Equal),
        Cmp3::Greater => Some(Ordering::Greater),
        _ => None,
    }
}

/// Is `t` a real number or a closed numeric term with a real value? `Some(false)` for a complex number with a
/// nonzero imaginary part; `None` when not decided.
pub fn is_real<'h, S: Store + ?Sized>(st: &'h S, ar: &Arith<'h, S>, nm: &Names, t: T<'h>) -> Option<bool> {
    match ar.num(t) {
        Some(Num::Real(_)) => return Some(true),
        Some(Num::Complex(_, im)) => return (!im.is_zero()).then_some(false),
        None => {}
    }
    let g = Graph::build(st, ar, nm, t);
    let is = g.island(g.node(t)?);
    if is.machine {
        return matches!(g.eval_machine(&is), Mach::Real(v) if v.is_finite()).then_some(true);
    }
    // a real value exists when evaluation succeeds at some precision (every node operation is real); a complex
    // value whose imaginary part excludes 0 is not real (N0C.4)
    let mut p = 64;
    while p <= COMPARE_CAP {
        if g.eval_ball(&is, p).is_some() {
            return Some(true);
        }
        if stopped() {
            return None;
        }
        if let Some(z) = g.eval_complex(&is, p) {
            if z.is_real() {
                return Some(true);
            }
            if !z.im.contains_zero() {
                return Some(false);
            }
        }
        p *= 2;
    }
    None
}
