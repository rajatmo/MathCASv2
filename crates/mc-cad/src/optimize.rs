//! Exact optimisation through the decomposition (roadmap item 11D, first part): `minimize`, `maximize`,
//! `min_value`, `max_value`, `arg_min`, `arg_max` of a polynomial or rational objective under real polynomial
//! constraints (with square roots, `abs`, `max` and `min` through the front end), in any number of variables.
//!
//! The objective becomes a new variable: `t == f(x) and cond(x)` is decomposed with `t` first (the base of the
//! cylinders), so the stack over the `t` axis describes the set of values `f` takes on the feasible set (the
//! projection of its graph). The lowest base cell with a true cell above it decides the answer:
//!
//! - none: the constraints are infeasible, the minimum is `inf` and the point `indeterminate`;
//! - an unbounded sector: `-inf`, the point `indeterminate`;
//! - a section (a real algebraic `t = r`): the minimum `r`, attained; the point is the sample point of a true cell
//!   above it (a rational one when there is one), so it is exact (rational, a quadratic radical, or a root object);
//! - a bounded sector: its lower end `r` is the infimum, not attained (`indeterminate` point).
//!
//! This is the textbook reduction of polynomial optimisation to quantifier elimination (Basu, Pollack and Roy,
//! *Algorithms in Real Algebraic Geometry*, section 14.1; Collins 1975 for the decomposition). Every answer is
//! checked before it is returned: the point satisfies the constraints and gives the value (decision CAD.16).
use crate::cad::{decompose, Cell, Coord, Limits, Truth};
use crate::front::Front;
use crate::natives::{coord_term, metered};
use mc_api::{names, Store, View, T};
use mc_eval::{Native, NativeCx, Reply, Res};
use mc_algebra::realroot::RealAlg;
use mc_arith::num::Rat;
use mc_poly::work::{Meter, WorkResult};
use std::cmp::Ordering;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// Which function.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Goal {
    /// `minimize`
    Minimize,
    /// `maximize`
    Maximize,
    /// `min_value`
    MinValue,
    /// `max_value`
    MaxValue,
    /// `arg_min`
    ArgMin,
    /// `arg_max`
    ArgMax,
}

impl Goal {
    pub(crate) fn max(self) -> bool {
        matches!(self, Goal::Maximize | Goal::MaxValue | Goal::ArgMax)
    }
}

/// The wrapper: the previous native first (it has the one-variable calculus methods and their printed forms);
/// what it leaves unevaluated goes through the decomposition.
pub struct Optimize {
    /// the previous native
    pub inner: Box<dyn Native>,
    /// which function
    pub goal: Goal,
    /// `mc-arith`'s stop flag
    pub stop: Arc<AtomicBool>,
}

impl Native for Optimize {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        // quantified constraints: their quantifiers eliminated first (an equivalent formula in the variables), then
        // the problem again; the objective's graph would add a variable to every decomposition of the quantifiers
        if let Some(r) = quantifier_free_constraints(cx, &self.stop, t, args)? {
            return Ok(Reply::Again(r));
        }
        match self.inner.call(cx, t, args)? {
            Reply::Decline => {}
            Reply::Done(r) => return Ok(Reply::Done(reduced_value(cx, self.goal, args, r)?)),
            r => return Ok(r),
        }
        if let Some(r) = crate::instances::optimize_integers(cx, &self.stop, self.goal, args)? {
            return Ok(Reply::Done(r));
        }
        if let Some(r) = optimize(cx, &self.stop, self.goal, args)? {
            return Ok(Reply::Done(reduced_value(cx, self.goal, args, r)?));
        }
        // regions as constraints, objectives with cases, variables the objective does not hold
        Ok(match crate::recast::recast(cx, self.goal, args)? {
            Some(r) => Reply::Done(reduced_value(cx, self.goal, args, r)?),
            None => Reply::Decline,
        })
    }
}

/// The optimum value of a problem in several variables as one algebraic number: a value written as a polynomial
/// in a root object (the objective at a point whose coordinates are polynomials in the root of one coordinate)
/// becomes `root(p, k)` with `p` the minimal polynomial of the value (`root_reduce`: resultants, exact). Values
/// that are already a root object, numbers, radicals or numeric roots are kept; so is the value of a problem in
/// one variable (the objective at the root of its derivative is the clearer form there). A maximum is written as
/// minus the root of the negated value (the minimum of the negated objective).
fn reduced_value<'h>(cx: &mut NativeCx<'_, '_, 'h>, goal: Goal, args: &[T<'h>], r: T<'h>) -> Res<T<'h>> {
    let h = cx.heap();
    let fr = Front::new(h);
    let nvars = args.get(1).and_then(|&x| fr.list(x)).map_or(1, |v| v.len());
    if nvars < 2 {
        return Ok(r);
    }
    let (v, rest) = match goal {
        Goal::MinValue | Goal::MaxValue => (r, None),
        Goal::Minimize | Goal::Maximize => match fr.list(r) {
            Some([v, pts]) => (*v, Some(*pts)),
            _ => return Ok(r),
        },
        Goal::ArgMin | Goal::ArgMax => return Ok(r),
    };
    let negated_root = |t: T<'h>| matches!(fr.head(t), Some((n, &[k, u])) if n == names::MUL && matches!(h.view(k), View::Int(-1)) && is_algebraic_root(h, u));
    if is_algebraic_root(h, v) || negated_root(v) || !has_algebraic_root(h, v) {
        return Ok(r);
    }
    // a maximum is minus the minimum of the negated objective: minus the root of the negated value
    let neg = |t: T<'h>| h.call(h.sym(names::MUL), &[h.int(-1), t]);
    let w = cx.eval(h.call(h.sym("root_reduce"), &[if goal.max() { neg(v) } else { v }]))?;
    let h = cx.heap();
    if !(is_algebraic_root(h, w) || matches!(h.view(w), View::Int(_) | View::BigInt { .. } | View::Rational { .. })) {
        return Ok(r);
    }
    let w = if goal.max() { cx.eval(neg(w))? } else { w };
    Ok(match rest {
        None => w,
        Some(pts) => cx.list(&[w, pts]),
    })
}

/// `root(p, k)` with an integer `k` (an exact real algebraic number, not a numeric root `root([f, x0])`).
fn is_algebraic_root(h: &mc_core::Heap, t: T<'_>) -> bool {
    match h.view(t) {
        View::App { head, args } => {
            matches!(h.view(head), View::Sym(s) if h.sym_name(s) == "root") && matches!(args, [_, k] if matches!(h.view(*k), View::Int(_)))
        }
        _ => false,
    }
}

/// A number built from root objects and rationals by arithmetic (no symbols, constants or other functions), with
/// at least one root object.
fn has_algebraic_root(h: &mc_core::Heap, t: T<'_>) -> bool {
    let mut stack = vec![t];
    let mut found = false;
    while let Some(u) = stack.pop() {
        if is_algebraic_root(h, u) {
            found = true;
            continue;
        }
        match h.view(u) {
            View::App { head, args } => {
                if !matches!(h.view(head), View::Sym(s) if matches!(h.sym_name(s).as_str(), names::ADD | names::MUL | names::POW)) {
                    return false;
                }
                stack.extend(args.iter().copied());
            }
            View::Int(_) | View::BigInt { .. } | View::Rational { .. } => {}
            _ => return false,
        }
    }
    found
}

/// `goal([f, cond], vars)` with the quantifiers of `cond` eliminated over the reals, when `f` has none and the
/// elimination finishes.
fn quantifier_free_constraints<'h>(cx: &mut NativeCx<'_, '_, 'h>, stop: &Arc<AtomicBool>, t: T<'h>, args: &[T<'h>]) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    let fr = Front::new(h);
    let [fc, xs] = args else { return Ok(None) };
    let Some([f, c]) = fr.list(*fc) else { return Ok(None) };
    if !fr.has_quantifier(*c) || fr.has_quantifier(*f) {
        return Ok(None);
    }
    let Some(q) = crate::natives::solve_formula(cx, stop, *c, &[], crate::natives::Style::Resolve)? else { return Ok(None) };
    if fr.has_quantifier(q) {
        return Ok(None);
    }
    let View::App { head, .. } = h.view(t) else { return Ok(None) };
    let fc2 = cx.list(&[*f, q]);
    Ok(Some(h.app(head, &[fc2, *xs])))
}

/// The extreme of the objective: its value (`None` for an infinite one, with the sign in `inf_sign`) and the
/// point when attained.
pub(crate) enum Extreme {
    /// the constraints hold nowhere
    Infeasible,
    /// unbounded (below for a minimum)
    Unbounded,
    /// the infimum, not attained
    Infimum(Coord),
    /// the minimum and a point where it is attained
    Attained(Coord, Vec<Coord>),
}

/// `minimize([f, cond], [x, y, ...])` and the others through the decomposition; `None` when the problem is not a
/// real polynomial problem without parameters or the decomposition did not finish.
pub fn optimize<'h>(cx: &mut NativeCx<'_, '_, 'h>, stop: &Arc<AtomicBool>, goal: Goal, args: &[T<'h>]) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    let fr = Front::new(h);
    let [fc, xs] = args else { return Ok(None) };
    let listed = fr.list(*xs).is_some();
    let Some(vars) = fr.qvars(*xs) else { return Ok(None) };
    if vars.is_empty() {
        return Ok(None);
    }
    let (f, cond) = match fr.list(*fc) {
        Some([f, c]) => (*f, Some(*c)),
        Some(_) => return Ok(None),
        None => (*fc, None),
    };
    if fr.has_quantifier(f) {
        return Ok(None);
    }
    // |u|^(2k) = u^(2k) for a real polynomial u of the (real) variables: no case split of the abs (the squared
    // norm of a residual, `norm(m . x - b)`, had 2^rows cases)
    let f = even_abs_powers(h, f, &vars);
    let cond = cond.map(|c| even_abs_powers(h, c, &vars));
    let g = if goal.max() { fr.ar.neg(f) } else { f };
    let tv = h.sym_term(h.sym("cad$objective"));
    let graph = h.call(h.sym("equal"), &[tv, g]);
    let full = match cond {
        Some(c) => h.call(h.sym("and"), &[graph, c]),
        None => graph,
    };
    let mut free = vec![tv];
    free.extend(vars.iter().copied());
    let n = vars.len();
    // abs, max and min outside quantifiers: one decomposition per case, the best of them (decision CAD.17)
    let branches = metered(cx, stop, |m| if fr.has_quantifier(full) { Ok(Some(vec![full])) } else { fr.term_cases(full, m) })?;
    let Some(Some(branches)) = branches else { return Ok(None) };
    // the cases' decompositions in parallel (roadmap item 16A), each on its own meter: the first case without an
    // answer ends the run, as the loop did
    let exts = cx.par_until(branches.len(), &|e: &Option<Option<Extreme>>| !matches!(e, Some(Some(_))), &|cx, i| {
        let fr = Front::new(cx.heap());
        metered(cx, stop, |m| extreme(&fr, branches[i], &free, n, m))
    })?;
    let mut found = Vec::with_capacity(exts.len());
    for e in exts {
        let Some(Some(e)) = e else { return Ok(None) };
        found.push(e);
    }
    let r = metered(cx, stop, |m| -> WorkResult<Option<Extreme>> {
        let mut best: Option<Extreme> = None;
        for e in found {
            best = Some(match best {
                None => e,
                Some(o) => better(o, e, m)?,
            });
        }
        Ok(best)
    })?;
    let Some(Some(ext)) = r else { return Ok(None) };
    let sgn = |cx: &mut NativeCx<'_, '_, 'h>, v: T<'h>| -> Res<T<'h>> {
        if goal.max() {
            let t = fr.ar.neg(v);
            cx.eval(t)
        } else {
            Ok(v)
        }
    };
    let inf = h.sym_term(h.sym("inf"));
    let ind = h.sym_term(h.sym("indeterminate"));
    let (value, point): (T<'h>, Option<Vec<T<'h>>>) = match ext {
        Extreme::Infeasible => (sgn(cx, inf)?, None),
        Extreme::Unbounded => {
            let v = fr.ar.neg(inf);
            let v = cx.eval(v)?;
            (sgn(cx, v)?, None)
        }
        Extreme::Infimum(r) => {
            let Some(v) = coord_term(cx, stop, &r)? else { return Ok(None) };
            (sgn(cx, v)?, None)
        }
        Extreme::Attained(r, pt) => {
            let Some(v) = coord_term(cx, stop, &r)? else { return Ok(None) };
            let mut ps = vec![];
            for c in &pt {
                let Some(t) = coord_term(cx, stop, c)? else { return Ok(None) };
                ps.push(t);
            }
            // the check: the constraints hold at the point and the objective takes the value there
            let rules: Vec<T<'h>> = vars.iter().zip(&ps).map(|(&x, &p)| h.call(h.sym(names::RULE), &[x, p])).collect();
            let rl = cx.list(&rules);
            if let Some(c) = cond {
                let ok = cx.eval(h.call(h.sym("rewrite"), &[c, rl]))?;
                if cx.is_false(ok) {
                    return Ok(None);
                }
            }
            let at = cx.eval(h.call(h.sym("rewrite"), &[g, rl]))?;
            let d = cx.eval(fr.ar.sub(at, v))?;
            let dn = cx.eval(h.call(h.sym("n"), &[d, h.int(30)]))?;
            let small = match h.view(dn) {
                View::Int(0) => true,
                View::Real(x) => x.abs() < 1e-20,
                _ => {
                    let a = cx.eval(h.call(h.sym("abs"), &[dn]))?;
                    let lt = cx.eval(h.call(h.sym("less"), &[a, h.call(h.sym(names::POW), &[h.int(10), h.int(-20)])]))?;
                    cx.is_true(lt)
                }
            };
            if !small {
                return Ok(None);
            }
            (sgn(cx, v)?, Some(ps))
        }
    };
    let ps = point.unwrap_or_else(|| vec![ind; n]);
    let rules: Vec<T<'h>> = vars.iter().zip(&ps).map(|(&x, &p)| h.call(h.sym(names::RULE), &[x, p])).collect();
    let out = match goal {
        Goal::Minimize | Goal::Maximize => {
            let rl = cx.list(&rules);
            cx.list(&[value, rl])
        }
        Goal::MinValue | Goal::MaxValue => value,
        Goal::ArgMin | Goal::ArgMax => {
            if listed {
                cx.list(&ps)
            } else {
                ps[0]
            }
        }
    };
    Ok(Some(cx.eval(out)?))
}

/// The extreme of the base variable over one formula (the objective's variable first in `free`).
pub(crate) fn extreme<'h>(fr: &Front<'h>, full: T<'h>, free: &[T<'h>], n: usize, m: &mut Meter) -> WorkResult<Option<Extreme>> {
    let Ok(p) = fr.problem(full, free, m)? else { return Ok(None) };
    // no parameters: every symbol is a listed variable (or bound)
    if p.prob.nfree != n + 1 {
        return Ok(None);
    }
    let Ok(o) = decompose(&p.prob, &Limits::default(), &mut |_, _| {}, m)? else { return Ok(None) };
    if let Some(t) = o.truth {
        return Ok(if t { None } else { Some(Extreme::Infeasible) });
    }
    let Some(i) = o.cells.iter().position(has_true) else { return Ok(Some(Extreme::Infeasible)) };
    let c = &o.cells[i];
    if c.section {
        let mut pts = vec![];
        if c.truth == Truth::True {
            pts.push(vec![c.coord.clone()]);
        }
        witnesses(&c.children, &mut vec![c.coord.clone()], &mut pts);
        // a rational point first
        let best = pts.iter().position(|p| p.iter().all(|c| c.as_rat().is_some())).unwrap_or(0);
        let Some(pt) = pts.get(best) else { return Ok(None) };
        let mut pt = pt[1..].to_vec();
        // a cylinder decided below the last level: any value of the rest
        pt.resize(n, Coord::Rat(Rat::from_i64(0)));
        return Ok(Some(Extreme::Attained(c.coord.clone(), pt)));
    }
    if c.lo.is_none() || i == 0 {
        return Ok(Some(Extreme::Unbounded));
    }
    Ok(Some(Extreme::Infimum(o.cells[i - 1].coord.clone())))
}

/// `t` with every `abs(u)^n`, n an even integer, replaced by `u^n` where `u` is a polynomial with real (not
/// complex) number coefficients in the variables `vars` (real throughout an optimisation), so `|u|^n = u^n`.
fn even_abs_powers<'h>(h: &'h mc_core::Heap, t: T<'h>, vars: &[T<'h>]) -> T<'h> {
    let View::App { head, args } = h.view(t) else { return t };
    let name = match h.view(head) {
        View::Sym(s) => h.sym_name(s),
        _ => String::new(),
    };
    if name == names::POW {
        if let [b, e] = args {
            if matches!(h.view(*e), View::Int(k) if k > 0 && k % 2 == 0) {
                if let View::App { head: bh, args: &[u] } = h.view(*b) {
                    if matches!(h.view(bh), View::Sym(s) if h.sym_name(s) == "abs") && real_polynomial(h, u, vars) {
                        return h.call(h.sym(names::POW), &[u, *e]);
                    }
                }
            }
        }
    }
    let new: Vec<T<'h>> = args.iter().map(|&a| even_abs_powers(h, a, vars)).collect();
    if new.iter().zip(args).all(|(a, b)| a == b) {
        t
    } else {
        h.app(head, &new)
    }
}

/// Whether `u` is built from the variables and real numbers by sums, products and powers with natural exponents.
fn real_polynomial<'h>(h: &'h mc_core::Heap, u: T<'h>, vars: &[T<'h>]) -> bool {
    // shared subterms are decided once (a term can be a tree exponentially larger than itself)
    real_polynomial_in(h, u, vars, &mut mc_api::walk::memo())
}

fn real_polynomial_in<'h>(h: &'h mc_core::Heap, u: T<'h>, vars: &[T<'h>], memo: &mut mc_api::walk::Memo<T<'h>, bool>) -> bool {
    mc_api::walk::cached(memo, u, |memo| {
        match h.view(u) {
            View::Int(_) | View::BigInt { .. } | View::Rational { .. } | View::Real(_) | View::Ball { .. } => true,
            View::Sym(_) => vars.contains(&u),
            View::App { head, args } => {
                let View::Sym(s) = h.view(head) else { return false };
                let n = h.sym_name(s);
                if n == names::ADD || n == names::MUL {
                    args.iter().all(|&a| real_polynomial_in(h, a, vars, memo))
                } else if n == names::POW {
                    matches!(args, [b, e] if matches!(h.view(*e), View::Int(k) if k >= 0) && real_polynomial_in(h, *b, vars, memo))
                } else {
                    false
                }
            }
            View::Str(_) => false,
        }
    })
}

fn alg(c: &Coord) -> RealAlg {
    match c {
        Coord::Rat(q) => RealAlg::rational(q),
        Coord::Alg(a) => a.clone(),
    }
}

/// The smaller of two extremes (an attained value before an infimum equal to it).
fn better(a: Extreme, b: Extreme, m: &mut Meter) -> WorkResult<Extreme> {
    let rank = |e: &Extreme| match e {
        Extreme::Unbounded => 0,
        Extreme::Infimum(_) | Extreme::Attained(..) => 1,
        Extreme::Infeasible => 2,
    };
    let (ra, rb) = (rank(&a), rank(&b));
    if ra != rb {
        return Ok(if ra < rb { a } else { b });
    }
    if ra != 1 {
        return Ok(a);
    }
    let va = match &a {
        Extreme::Infimum(v) | Extreme::Attained(v, _) => v.clone(),
        _ => unreachable!("a finite extreme"),
    };
    let vb = match &b {
        Extreme::Infimum(v) | Extreme::Attained(v, _) => v.clone(),
        _ => unreachable!("a finite extreme"),
    };
    let ord = alg(&va).compare(&mut alg(&vb), m)?;
    Ok(match ord {
        Ordering::Less => a,
        Ordering::Greater => b,
        Ordering::Equal => {
            if matches!(a, Extreme::Attained(..)) {
                a
            } else {
                b
            }
        }
    })
}

/// Is a cell true, or true somewhere above?
fn has_true(c: &Cell) -> bool {
    match c.truth {
        Truth::True => true,
        Truth::False => false,
        Truth::Mixed => c.children.iter().any(has_true),
    }
}

/// The sample points of the true leaves above a cell (`pre` holds the coordinates so far).
fn witnesses(cells: &[Cell], pre: &mut Vec<Coord>, out: &mut Vec<Vec<Coord>>) {
    for c in cells {
        if out.len() >= 64 {
            return;
        }
        pre.push(c.coord.clone());
        match c.truth {
            Truth::True => out.push(pre.clone()),
            Truth::False => {}
            Truth::Mixed => witnesses(&c.children, pre, out),
        }
        pre.pop();
    }
}
