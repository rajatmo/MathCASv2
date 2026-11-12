//! The natives: `resolve`, `exists`, `for_all`, `cylindrical_decomposition`, the several-variable path of
//! `find_instance`, and [`reduce_reals`] for `mc-solve`'s `reduce`.
use crate::cad::{decompose, Coord, Limits, Outcome};
use crate::front::Front;
use crate::output::{leaves, Writer};
use mc_algebra::realroot::{rational_real_roots, RealAlg};
use mc_api::{names, Store, View, T};
use mc_arith::num::{Int, Rat, StopScope};
use mc_eval::{Abort, Native, NativeCx, Reply, Res};
use mc_poly::work::{Halt, Meter, Work, WorkResult};
use std::cmp::Ordering;
use std::sync::atomic::{AtomicBool, Ordering as AO};
use std::sync::Arc;

/// The evaluator's fuel and interrupt as a work budget, also polling `mc-arith`'s stop flag.
pub(crate) struct Tick<'c, 'a, 'k, 'h> {
    pub cx: &'c mut NativeCx<'a, 'k, 'h>,
    pub stop: &'c AtomicBool,
    pub abort: Option<Abort>,
}

impl Work for Tick<'_, '_, '_, '_> {
    fn charge(&mut self, units: u64) -> WorkResult<()> {
        if self.stop.load(AO::Relaxed) {
            // raised with the evaluator's interrupt, which the fuel poll has not seen yet: the stop is the abort (a
            // caller that sees no abort tries its next method with the number kernels returning arbitrary values)
            if self.cx.stopped() {
                self.abort = Some(mc_eval::Abort::Limit);
            }
            return Err(Halt::Stopped);
        }
        match self.cx.tick(units) {
            Ok(()) => Ok(()),
            Err(a) => {
                self.abort = Some(a);
                Err(Halt::Stopped)
            }
        }
    }
}

/// Run `f` with a meter on the evaluator's fuel; an interrupt is the evaluator's abort, an exceeded memory limit
/// `None`.
pub(crate) fn metered<'h, X>(
    cx: &mut NativeCx<'_, '_, 'h>,
    stop: &Arc<AtomicBool>,
    f: impl FnOnce(&mut Meter) -> WorkResult<X>,
) -> Res<Option<X>> {
    let _scope = StopScope::enter(Some(stop));
    // decompositions inside lift in parallel under this budget (roadmap item 16A)
    let _offer = crate::par::Offer::new(cx.budget(&[stop]), stop);
    let mut tick = Tick { cx, stop, abort: None };
    let r = {
        let mut m = Meter::new(&mut tick);
        f(&mut m)
    };
    match r {
        Ok(x) => Ok(Some(x)),
        Err(Halt::Stopped) => match tick.abort {
            Some(a) => Err(a),
            // stopped by the number kernels' flag, which is raised with the evaluator's interrupt before the fuel
            // refill sees it: the evaluation stops too (an answer `None` would send the caller on to its next method
            // with the kernels returning arbitrary values, which may never end)
            None if tick.cx.stopped() => Err(Abort::Limit),
            None => Ok(None),
        },
        Err(Halt::Memory) => Ok(None),
    }
}

/// How a decomposition is written.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Style {
    /// `reduce`: cylindrical, single points substituted
    Reduce,
    /// `cylindrical_decomposition`: cylindrical
    Cylindrical,
    /// `resolve`: sign conditions when they separate, else cylindrical
    Resolve,
}

/// The decomposition of `cond` with the free variables `free` (after any parameters), written in `style`;
/// `None` when the formula is not a real polynomial formula or the decomposition did not finish. The answer is
/// checked at the rational sample points of the free cells before it is returned (decision CAD.7).
pub fn solve_formula<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    stop: &Arc<AtomicBool>,
    cond: T<'h>,
    free: &[T<'h>],
    style: Style,
) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    let r = metered(cx, stop, |m| -> WorkResult<Option<(T<'h>, Vec<T<'h>>, Outcome)>> {
        let fr = Front::new(h);
        let Ok(mut p) = fr.problem(cond, free, m)? else { return Ok(None) };
        // for_all of one polynomial inequality: a sum-of-squares certificate first (decision CAD.9)
        if let Some(true) = crate::sos::sentence(&p.prob, m)? {
            return Ok(Some((Writer::new(h, &[], m)?.bool(true), vec![], Outcome::closed(&p.prob, true))));
        }
        if let Some(q) = crate::vs::eliminate(&p.prob, m)? {
            p.vars.truncate(q.nv);
            p.prob = q;
        }
        let Ok(o) = decompose(&p.prob, &Limits::default(), &mut |_, _| {}, m)? else { return Ok(None) };
        let w = Writer::new(h, &p.vars, m)?;
        let t = match style {
            Style::Resolve => match w.sign_formula(&o, m)? {
                Some(t) => t,
                None => w.cylindrical(&o, m)?,
            },
            _ => w.cylindrical(&o, m)?,
        };
        Ok(Some((t, p.vars[..p.prob.nfree].to_vec(), o)))
    })?;
    let Some(Some((t, vars, o))) = r else { return Ok(None) };
    let mut t = cx.eval(t)?;
    // reduce of a finite set of two or more rational points without parameters: each point written out (decision
    // CAD.23)
    if style == Style::Reduce && o.truth.is_none() && o.problem.nfree == free.len() && vars.len() == free.len() {
        let mut pts = vec![];
        let rational = |pts: &Vec<Vec<Coord>>| pts.iter().all(|p| p.iter().all(|c| c.as_rat().is_some()));
        if points(&o.cells, &mut vec![], vars.len(), &mut pts) && pts.len() >= 2 && pts.len() <= 64 && rational(&pts) {
            let mut cases = vec![];
            for pt in &pts {
                let mut eqs = vec![];
                for (&x, c) in vars.iter().zip(pt) {
                    let Some(v) = coord_term(cx, stop, c)? else { return Ok(Some(t)) };
                    eqs.push(h.call(h.sym("equal"), &[x, v]));
                }
                cases.push(h.call(h.sym("and"), &eqs));
            }
            t = cx.eval(h.call(h.sym("or"), &cases))?;
        }
    }
    if !check(cx, t, &vars, &o)? {
        return Ok(None);
    }
    Ok(Some(t))
}

/// Does the answer `t` have the truth of each free cell at its sample point (the cells whose coordinates are all
/// rational; at most 64)? Undecided evaluations are skipped.
fn check<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, vars: &[T<'h>], o: &Outcome) -> Res<bool> {
    let h = cx.heap();
    if o.truth.is_some() || vars.is_empty() {
        return Ok(true);
    }
    let w = Writer::new_unmetered(h, vars);
    // the points first, then their evaluations in parallel (roadmap item 16A), in order until a mismatch
    let mut pts: Vec<(T<'h>, bool)> = vec![];
    for (pt, truth) in leaves(o) {
        if pts.len() >= 64 {
            break;
        }
        let Some(vals) = pt.iter().map(Coord::as_rat).collect::<Option<Vec<Rat>>>() else { continue };
        let rules: Vec<T<'h>> = vars.iter().zip(&vals).map(|(&x, q)| h.call(h.sym(names::RULE), &[x, w.rat(q)])).collect();
        pts.push((cx.list(&rules), truth));
    }
    let ok = cx.par_until(pts.len(), &|ok: &bool| !*ok, &|cx, i| {
        let (rl, truth) = pts[i];
        let h = cx.heap();
        let v = cx.eval(h.call(h.sym("rewrite"), &[t, rl]))?;
        Ok(!((truth && cx.is_false(v)) || (!truth && cx.is_true(v))))
    })?;
    Ok(ok.iter().all(|&k| k))
}

/// `reduce(cond, [x, y, ...], reals)` and `reduce` of quantified formulas: the cylindrical form of the solution set.
pub fn reduce_reals<'h>(cx: &mut NativeCx<'_, '_, 'h>, stop: &Arc<AtomicBool>, cond: T<'h>, vars: &[T<'h>]) -> Res<Option<T<'h>>> {
    solve_formula(cx, stop, cond, vars, Style::Reduce)
}

/// The functions of this crate.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    /// `resolve`
    Resolve,
    /// `exists`
    Exists,
    /// `for_all`
    ForAll,
    /// `cylindrical_decomposition`
    Cylindrical,
    /// `cad$stats(formula, free variables, method, virtual substitution)`: measurements (internal)
    Stats,
}

/// A native of this crate.
pub struct CadNative {
    /// which function
    pub kind: Kind,
    /// `mc-arith`'s stop flag
    pub stop: Arc<AtomicBool>,
}

impl Native for CadNative {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        match self.kind {
            Kind::Exists | Kind::ForAll => quantifier(cx, self.kind == Kind::Exists, args),
            Kind::Resolve => resolve(cx, &self.stop, args),
            Kind::Stats => stats(cx, &self.stop, args),
            Kind::Cylindrical => {
                let [c, xs] = args else { return Ok(Reply::Decline) };
                let fr = Front::new(cx.heap());
                let Some(vs) = fr.qvars(*xs) else { return Ok(Reply::Decline) };
                Ok(match solve_formula(cx, &self.stop, *c, &vs, Style::Cylindrical)? {
                    Some(t) => Reply::Done(t),
                    None => Reply::Decline,
                })
            }
        }
    }
}

/// `exists(x, body)`, `exists([x, y], body)`, `exists(x, cond, body)`: variables that do not occur are dropped, a
/// quantifier over nothing is its body; otherwise the formula stays (it is resolved by `resolve` and `reduce`).
fn quantifier<'h>(cx: &mut NativeCx<'_, '_, 'h>, exists: bool, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let fr = Front::new(h);
    let (vs, rest) = match args {
        [v, b] => (*v, vec![*b]),
        [v, c, b] => (*v, vec![*c, *b]),
        _ => return Ok(Reply::Decline),
    };
    let Some(vars) = fr.qvars(vs) else { return Ok(Reply::Decline) };
    let body = *rest.last().expect("a body");
    if cx.is_true(body) || cx.is_false(body) {
        // exists(x, false) and for_all(x, true) are themselves; over a nonempty domain the other two are too
        return Ok(Reply::Done(body));
    }
    let used: Vec<T<'h>> = vars.iter().copied().filter(|&x| rest.iter().any(|&b| mentions(h, b, x))).collect();
    if used.len() == vars.len() {
        return Ok(Reply::Decline);
    }
    if used.is_empty() {
        let b = if rest.len() == 2 {
            if exists {
                h.call(h.sym("and"), &rest)
            } else {
                h.call(h.sym("implies"), &rest)
            }
        } else {
            body
        };
        return Ok(Reply::Again(b));
    }
    let name = if exists { "exists" } else { "for_all" };
    let v = if used.len() == 1 { used[0] } else { cx.list(&used) };
    let mut a = vec![v];
    a.extend(rest);
    Ok(Reply::Again(h.call(h.sym(name), &a)))
}

fn mentions<'h>(h: &'h impl Store, t: T<'h>, x: T<'h>) -> bool {
    let mut stack = vec![t];
    while let Some(u) = stack.pop() {
        if u == x {
            return true;
        }
        if let View::App { head, args } = h.view(u) {
            stack.push(head);
            stack.extend(args.iter().copied());
        }
    }
    false
}

/// Does a formula have an inequality (which makes its variables real)?
fn has_inequality<'h>(fr: &Front<'h>, t: T<'h>) -> bool {
    let mut stack = vec![t];
    let h = fr.ar.store();
    while let Some(u) = stack.pop() {
        if let Some((n, args)) = fr.head(u) {
            if matches!(n.as_str(), "less" | "less_equal" | "greater" | "greater_equal") {
                return true;
            }
            if n == "element" && args.len() == 2 && fr.sym_name(args[1]).as_deref() == Some("reals") {
                return true;
            }
            stack.extend(args.iter().copied());
        } else if let View::App { args, .. } = h.view(u) {
            stack.extend(args.iter().copied());
        }
    }
    false
}

/// `resolve(formula)`, `resolve(formula, reals)`, `resolve(formula, complexes)`: the formula without quantifiers.
fn resolve<'h>(cx: &mut NativeCx<'_, '_, 'h>, stop: &Arc<AtomicBool>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let fr = Front::new(h);
    let (f, dom) = match args {
        [f] => (*f, None),
        [f, d] => (*f, fr.sym_name(*d)),
        _ => return Ok(Reply::Decline),
    };
    if !fr.has_quantifier(f) {
        return Ok(Reply::Done(f));
    }
    if dom.as_deref() == Some("integers") {
        return Ok(match crate::integers::resolve_integers(cx, stop, f)? {
            Some(t) => Reply::Done(t),
            None => Reply::Decline,
        });
    }
    let reals = match dom.as_deref() {
        Some("reals") => true,
        Some("complexes") => false,
        None => has_inequality(&fr, f),
        _ => return Ok(Reply::Decline),
    };
    if !reals {
        return Ok(match crate::complex::resolve(cx, stop, f)? {
            Some(t) => Reply::Done(t),
            None => Reply::Done(unresolved(cx, f)),
        });
    }
    Ok(match solve_formula(cx, stop, f, &[], Style::Resolve)? {
        Some(t) => Reply::Done(t),
        None => Reply::Done(unresolved(cx, f)),
    })
}

/// A formula `resolve` could not eliminate: itself, with the variables of each quantifier as a list (the
/// reference's form of an unresolved formula).
fn unresolved<'h>(cx: &mut NativeCx<'_, '_, 'h>, f: T<'h>) -> T<'h> {
    let h = cx.heap();
    match h.view(f) {
        View::App { head, args } => {
            let new: Vec<T<'h>> = args.iter().map(|&a| unresolved(cx, a)).collect();
            let fr = Front::new(h);
            let quant = matches!(fr.head(f).map(|x| x.0).as_deref(), Some("exists" | "for_all"));
            let mut new = new;
            if quant && !new.is_empty() && fr.list(new[0]).is_none() {
                new[0] = cx.list(&[new[0]]);
            }
            // nested conjunctions and disjunctions flattened
            if let Some((name @ ("and" | "or"), _)) = fr.head(f).as_ref().map(|(n, a)| (n.as_str(), a)) {
                let mut flat = vec![];
                for &a in &new {
                    match fr.head(a) {
                        Some((n2, inner)) if n2 == name => flat.extend_from_slice(inner),
                        _ => flat.push(a),
                    }
                }
                return h.app(head, &flat);
            }
            h.app(head, &new)
        }
        _ => f,
    }
}

/// `find_instance(cond, [x, y, ...])` and `find_instance(cond, vars, reals)` for real polynomial conditions: the
/// sample point of a true cell of the decomposition (the first one found; every coordinate certified); `[]` when
/// there is none. Other calls go to the wrapped native.
pub struct FindInstance {
    /// the previous native
    pub inner: Box<dyn Native>,
    /// `mc-arith`'s stop flag
    pub stop: Arc<AtomicBool>,
}

impl Native for FindInstance {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        if let Some(r) = find_instance(cx, &self.stop, args)? {
            return Ok(Reply::Done(r));
        }
        self.inner.call(cx, t, args)
    }
}

fn find_instance<'h>(cx: &mut NativeCx<'_, '_, 'h>, stop: &Arc<AtomicBool>, args: &[T<'h>]) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    let fr = Front::new(h);
    let count = |t: T<'h>| match h.view(t) {
        View::Int(k) if (1..=10_000).contains(&k) => Some(k as usize),
        _ => None,
    };
    // `... and element(x | y, integers)` covering every variable: the integer search, the element conjunct dropped
    if let [c, x] = args {
        if let (View::App { head, args: cs }, Some(vars)) = (h.view(*c), fr.qvars(*x)) {
            if fr.sym_name(head).as_deref() == Some("and") {
                let mut ints = Vec::new();
                let mut rest = Vec::new();
                for &k in cs {
                    match h.view(k) {
                        View::App { head: e, args: &[v, d] } if fr.sym_name(e).as_deref() == Some("element") && fr.sym_name(d).as_deref() == Some("integers") => {
                            match h.view(v) {
                                View::App { head: alt, args: vs } if fr.sym_name(alt).as_deref() == Some("alternatives") => ints.extend_from_slice(vs),
                                _ => ints.push(v),
                            }
                        }
                        _ => rest.push(k),
                    }
                }
                if !ints.is_empty() && !rest.is_empty() && vars.iter().all(|v| ints.contains(v)) {
                    let c2 = if rest.len() == 1 { rest[0] } else { h.app(head, &rest) };
                    return find_instance(cx, stop, &[c2, *x, h.sym_term(h.sym("integers"))]);
                }
            }
        }
    }
    let (cond, xs, n) = match args {
        [c, x] if has_inequality(&fr, *c) => (*c, *x, 1),
        [c, x, d] if fr.sym_name(*d).as_deref() == Some("reals") => (*c, *x, 1),
        [c, x, k] if has_inequality(&fr, *c) && count(*k).is_some() => (*c, *x, count(*k).unwrap_or(1)),
        [c, x, d, k] if fr.sym_name(*d).as_deref() == Some("reals") && count(*k).is_some() => (*c, *x, count(*k).unwrap_or(1)),
        [c, x, d] | [c, x, d, _] if fr.sym_name(*d).as_deref() == Some("integers") => {
            let n = match args.get(3) {
                Some(&k) => match count(k) {
                    Some(n) => n,
                    None => return Ok(None),
                },
                None => 1,
            };
            let Some(vars) = fr.qvars(*x) else { return Ok(None) };
            if vars.len() < 2 || fr.has_quantifier(*c) {
                return Ok(None);
            }
            return crate::integers::find_integers(cx, stop, *c, &vars, n);
        }
        _ => return Ok(None),
    };
    let Some(vars) = fr.qvars(xs) else { return Ok(None) };
    if n > 1 {
        return find_several(cx, stop, cond, &vars, n);
    }
    if vars.len() < 2 && fr.list(xs).is_none() && !fr.has_quantifier(cond) {
        return Ok(None);
    }
    let r = metered(cx, stop, |m| -> WorkResult<Option<Option<Vec<Coord>>>> {
        let Ok(p) = fr.problem(cond, &vars, m)? else { return Ok(None) };
        // every listed variable must be in the problem, no parameter
        if p.prob.nfree != vars.len() {
            return Ok(None);
        }
        let mut prob = p.prob.clone();
        // a variable that an equation determines linearly goes last: its sections over rational points are
        // rational (decision CAD.11)
        let nf = prob.nfree;
        let linear = (0..nf).rev().find(|&j| {
            prob.atoms.iter().any(|a| {
                a.rel == crate::formula::Rel::Eq && {
                    let cs = crate::poly::coeffs(&a.num, j);
                    cs.len() == 2 && cs[1].is_constant()
                }
            })
        });
        let mut order: Vec<usize> = (0..prob.nv).collect();
        if let Some(j) = linear.filter(|&j| j + 1 < nf) {
            order.remove(j);
            order.insert(nf - 1, j);
            let mut perm = vec![0; prob.nv];
            for (newi, &old) in order.iter().enumerate() {
                perm[old] = newi;
            }
            for a in prob.atoms.iter_mut() {
                a.num = a.num.permute(&perm);
                a.den = a.den.permute(&perm);
            }
        }
        // the free variables become existential (outermost); the witness is the first true cell's sample point
        let mut q = vec![crate::formula::Quant::Exists; prob.nfree];
        q.extend(prob.quants.iter().copied());
        prob.quants = q;
        let nfree = prob.nfree;
        prob.nfree = 0;
        let lim = Limits::default();
        let Ok(o) = crate::cad::decompose_witness(&prob, &lim, nfree, m)? else { return Ok(None) };
        // back to the variables' order
        Ok(Some(o.map(|w| {
            let mut out = vec![Coord::Rat(Rat::from_i64(0)); nfree];
            for (newi, c) in w.into_iter().enumerate() {
                if newi < nfree {
                    out[order[newi]] = c;
                }
            }
            out
        })))
    })?;
    let Some(Some(w)) = r else { return Ok(None) };
    let Some(coords) = w else { return Ok(Some(cx.list(&[]))) };
    let mut rules = vec![];
    for (i, &x) in vars.iter().enumerate() {
        let v = match coords.get(i) {
            Some(c) => coord_term(cx, stop, c)?,
            None => Some(h.int(0)),
        };
        let Some(v) = v else { return Ok(None) };
        rules.push(h.call(h.sym(names::RULE), &[x, v]));
    }
    let r = cx.list(&rules);
    Ok(Some(cx.list(&[r])))
}

/// `find_instance(cond, vars, n)` over the reals: the sample points of up to `n` true cells, when every true cell
/// is a point (a finite solution set, listed completely in the order of the decomposition); `None` otherwise.
fn find_several<'h>(cx: &mut NativeCx<'_, '_, 'h>, stop: &Arc<AtomicBool>, cond: T<'h>, vars: &[T<'h>], n: usize) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    let fr = Front::new(h);
    let r = metered(cx, stop, |m| -> WorkResult<Option<Vec<Vec<Coord>>>> {
        let Ok(p) = fr.problem(cond, vars, m)? else { return Ok(None) };
        if p.prob.nfree != vars.len() {
            return Ok(None);
        }
        let Ok(o) = decompose(&p.prob, &Limits::default(), &mut |_, _| {}, m)? else { return Ok(None) };
        if o.truth == Some(false) {
            return Ok(Some(vec![]));
        }
        let mut pts = vec![];
        if !points(&o.cells, &mut vec![], vars.len(), &mut pts) {
            return Ok(None);
        }
        Ok(Some(pts))
    })?;
    let Some(Some(pts)) = r else { return Ok(None) };
    let mut out = vec![];
    for pt in pts.into_iter().take(n) {
        let mut rules = vec![];
        for (&x, c) in vars.iter().zip(&pt) {
            let Some(v) = coord_term(cx, stop, c)? else { return Ok(None) };
            rules.push(h.call(h.sym(names::RULE), &[x, v]));
        }
        out.push(cx.list(&rules));
    }
    Ok(Some(cx.list(&out)))
}

/// The sample points of the true cells when each is a point (sections at every level); `false` when a true cell
/// has positive dimension.
fn points(cells: &[crate::cad::Cell], pre: &mut Vec<Coord>, n: usize, out: &mut Vec<Vec<Coord>>) -> bool {
    use crate::cad::Truth;
    for c in cells {
        if c.truth == Truth::False {
            continue;
        }
        if !c.section {
            return false;
        }
        pre.push(c.coord.clone());
        let ok = match c.truth {
            Truth::True => {
                if pre.len() == n {
                    out.push(pre.clone());
                    true
                } else {
                    false
                }
            }
            _ => points(&c.children, pre, n, out),
        };
        pre.pop();
        if !ok {
            return false;
        }
    }
    true
}

/// A coordinate as a term: a rational, a quadratic radical, or a root object.
pub(crate) fn coord_term<'h>(cx: &mut NativeCx<'_, '_, 'h>, stop: &Arc<AtomicBool>, c: &Coord) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    let w = Writer::new_unmetered(h, &[]);
    match c {
        Coord::Rat(q) => Ok(Some(w.rat(q))),
        Coord::Alg(a) => {
            if let Some(q) = a.as_rat() {
                return Ok(Some(w.rat(&q)));
            }
            let a = a.clone();
            let r = metered(cx, stop, |m| -> WorkResult<Option<T<'h>>> { real_alg_term(&w, &a, m) })?;
            match r.flatten() {
                Some(t) => Ok(Some(cx.eval(t)?)),
                None => Ok(None),
            }
        }
    }
}

/// A real algebraic number as a term: its index among the real roots of its minimal polynomial.
pub(crate) fn real_alg_term<'h>(w: &Writer<'h>, a: &RealAlg, m: &mut Meter) -> WorkResult<Option<T<'h>>> {
    let p: Vec<Rat> = a.poly.iter().map(|c| Rat::from_int(c.to_int())).collect();
    let roots = rational_real_roots(&p, m)?;
    let mut me = a.clone();
    let mut k = None;
    for (j, (r, _)) in roots.into_iter().enumerate() {
        let mut r = r;
        if me.compare(&mut r, m)? == Ordering::Equal {
            k = Some(j + 1);
            break;
        }
    }
    let Some(k) = k else { return Ok(None) };
    let x = w.slot();
    let nv = 1;
    let poly = mc_poly::Poly::from_dense(nv, 0, &a.poly);
    let w1 = Writer::new(w.ar.store(), &[x], m)?;
    let info = crate::cad::FacInfo { fac: 0, nroots: if a.poly.len() == 3 { 2 } else { 0 }, lc_sign: 1, degree: a.poly.len() - 1 };
    let _ = Int::from_i64(0);
    Ok(Some(w1.root(&poly, 0, k, Some(&info), m)?))
}

/// `not(exists(x, phi))` is `for_all(x, not phi)` and `not(for_all(x, phi))` is `exists(x, not phi)`; other calls
/// go to the wrapped native.
pub struct NotQuantifier {
    /// the previous native
    pub inner: Box<dyn Native>,
}

impl Native for NotQuantifier {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        if let [a] = args {
            let h = cx.heap();
            let fr = Front::new(h);
            if let Some((name, qa)) = fr.head(*a) {
                let flip = match name.as_str() {
                    "exists" => Some("for_all"),
                    "for_all" => Some("exists"),
                    _ => None,
                };
                if let (Some(other), true) = (flip, qa.len() >= 2) {
                    let mut b = qa.to_vec();
                    let last = b.len() - 1;
                    b[last] = h.call(h.sym("not"), &[b[last]]);
                    return Ok(Reply::Again(h.call(h.sym(other), &b)));
                }
            }
        }
        self.inner.call(cx, t, args)
    }
}

/// `cad$stats(formula, [x, ...], "lazard" | "brown", true | false)`: `[factors, cells, microseconds, truth]` of the
/// decomposition with the given operator, with or without virtual substitution first (for measurements; the
/// truth is `true`, `false` or `null` for formulas with free variables).
fn stats<'h>(cx: &mut NativeCx<'_, '_, 'h>, stop: &Arc<AtomicBool>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [c, xs, meth, vs] = args else { return Ok(Reply::Decline) };
    let fr = Front::new(h);
    let free = match fr.list(*xs) {
        Some(v) => v.to_vec(),
        None => return Ok(Reply::Decline),
    };
    let method = match h.view(*meth) {
        View::Str("brown") => crate::project::Method::Brown,
        _ => crate::project::Method::Lazard,
    };
    let use_vs = cx.is_true(*vs);
    let t0 = std::time::Instant::now();
    let r = metered(cx, stop, |m| -> WorkResult<Option<(usize, usize, Option<bool>)>> {
        let Ok(mut p) = fr.problem(*c, &free, m)? else { return Ok(None) };
        if use_vs {
            if let Some(q) = crate::vs::eliminate(&p.prob, m)? {
                p.prob = q;
            }
        }
        let lim = Limits { method, max_cells: 20_000, max_factors: 120, ..Limits::default() };
        let Ok(o) = decompose(&p.prob, &lim, &mut |_, _| {}, m)? else { return Ok(None) };
        Ok(Some((o.basis.facs.len(), o.ncells, o.truth)))
    })?;
    let us = t0.elapsed().as_micros() as i64;
    let Some(Some((f, n, t))) = r else { return Ok(Reply::Decline) };
    let tt = match t {
        Some(b) => cx.bool(b),
        None => cx.null(),
    };
    Ok(Reply::Done(cx.list(&[h.int(f as i64), h.int(n as i64), h.int(us), tt])))
}
