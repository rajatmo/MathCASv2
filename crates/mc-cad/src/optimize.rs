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
    fn max(self) -> bool {
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
        match self.inner.call(cx, t, args)? {
            Reply::Decline => {}
            r => return Ok(r),
        }
        Ok(match optimize(cx, &self.stop, self.goal, args)? {
            Some(r) => Reply::Done(r),
            None => Reply::Decline,
        })
    }
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
