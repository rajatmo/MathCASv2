//! Symbolic differential equations and recurrences (decision 2C.13, item 13A): `d_solve`, `d_solve_value`,
//! `asymptotic_d_solve_value`, `r_solve`, `r_solve_value`.
//!
//! `d_solve(eqs, y, x)` for unknown functions of one variable `x` (`y` or `y(x)`, or a list of them, as the second
//! argument; both arguments may be left out and are then read off the equations), with conditions `y(x0) == v`,
//! `y'(x0) == v`, ... in `eqs`. The methods, each in its own module:
//!
//! - [`linear`]: linear equations. Constant coefficients of any order (the roots of the characteristic polynomial,
//!   a particular solution by undetermined coefficients for exponential-polynomial right-hand sides and by
//!   variation of parameters otherwise, any order); Euler equations of any order; first order by the integrating
//!   factor; equations without `y` reduced in order; second order with variable coefficients by a solution found
//!   from an ansatz (exponential, power, polynomial) and reduction of order, by [`special`] functions (Airy, Bessel,
//!   the hypergeometric and confluent hypergeometric families, Hermite, Laguerre, Legendre) recognised after a
//!   change of variables, and by [`kovacic`]'s algorithm for Liouvillian solutions.
//! - [`first`]: nonlinear first order: separable, Bernoulli, Riccati (through the linear second-order equation),
//!   homogeneous and isobaric (scaling) equations, exact equations with integrating factors in x or y, integrating
//!   factors from Lie point symmetries found from a linear ansatz, Clairaut equations; second-order equations without
//!   `y` or without `x` reduced to first order.
//! - [`system`]: linear systems with constant coefficients through the matrix exponential (built from the scalar
//!   basis of the characteristic polynomial, Cayley-Hamilton), algebraic equations eliminated first.
//! - [`pde`]: first-order linear partial differential equations in two variables by characteristics, and
//!   second-order homogeneous equations with constant coefficients (`u_tt = c^2 u_xx`) by factoring.
//! - [`delay`]: delay equations `y'(t) = f(t, y(t), y(t - tau))` with a history, by the method of steps.
//! - [`series`]: `asymptotic_d_solve_value`, power-series solutions at ordinary points (Taylor, with or without
//!   conditions), Frobenius series at regular singular points, and regular perturbation in a parameter.
//!
//! Every explicit solution is substituted back into the equation ([`verify`]): the residual is simplified to 0, or,
//! when that does not succeed, evaluated at random points (random values for the constants and parameters too).
//! A solution that fails is dropped; when every method fails the call stays unevaluated. Each call has a time budget
//! and polls the stop flag.
//!
//! The constants are `c(1)`, `c(2)`, ... (the head given by `generated_parameters: f` instead); the conditions fix
//! them (`solve` of the system). The result is `[[y -> fn([x], sol)]]` (`[[y(x) -> sol]]` for `y(x)` as the second
//! argument), one rule list per solution; `d_solve_value` gives `sol` (`fn([x], sol)` for `y`, the target with the
//! solution put in for any other expression such as `y(3)` or `y'(x)`). An implicit solution that cannot be solved
//! for `y` is returned as `solve(eq, y(x))`.
//!
//! `r_solve(eqs, a, n)` for linear recurrences with constant coefficients: [`recur`].
//!
//! Literature: E. L. Ince, *Ordinary Differential Equations* (1926); D. Zwillinger, *Handbook of Differential
//! Equations* (1997); G. W. Bluman and S. Kumei, *Symmetries and Differential Equations* (1989); J. Kovacic, "An
//! algorithm for solving second order linear homogeneous differential equations", J. Symb. Comp. 2 (1986); E. S.
//! Cheb-Terrab, L. G. S. Duarte, L. A. C. P. da Mota, "Computer algebra solving of first order ODEs using
//! symmetry methods", Comp. Phys. Comm. 101 (1997); E. J. Putzer, "Avoiding the Jordan canonical form in the
//! discussion of linear systems with constant coefficients", Amer. Math. Monthly 73 (1966).
//!
//! A crate of its own since round 34 (split out of `mc-calculus`).
#![warn(missing_docs)]
mod delay;
mod first;
mod kovacic;
mod linear;
mod pde;
mod recur;
mod series;
mod special;
mod system;
mod verify;

use mc_calculus::common::is_free;
use mc_calculus::term::Build;
use mc_api::{names, Store, View, T};
use mc_arith::num::StopScope;
use mc_eval::{Native, NativeCx, Reply, Res};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Which function.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    /// `d_solve`
    DSolve,
    /// `d_solve_value`
    DSolveValue,
    /// `asymptotic_d_solve_value`
    AsymptoticValue,
    /// `r_solve`
    RSolve,
    /// `r_solve_value`
    RSolveValue,
}

/// The native.
pub struct DSolveNative {
    /// which function
    pub kind: Kind,
    /// the flag `mc-arith` polls in long numeric loops
    pub stop: Arc<AtomicBool>,
}

/// The highest order handled.
pub(crate) const MAX_ORDER: usize = 12;

/// The time budget of one call.
const BUDGET: Duration = Duration::from_secs(5);

/// The context of one call: the evaluator, the builder, the head of the constants and the deadline.
pub(crate) struct Cx<'a, 'b, 'c, 'k, 'h> {
    pub(crate) cx: &'a mut NativeCx<'c, 'k, 'h>,
    pub(crate) b: &'b Build<'h>,
    /// the head of the constants: `c`, or the `generated_parameters` option
    pub(crate) gen: T<'h>,
    deadline: Instant,
    /// nesting of the solver (methods that call it again for a reduced equation)
    pub(crate) depth: usize,
}

impl<'h> Cx<'_, '_, '_, '_, 'h> {
    pub(crate) fn ev(&mut self, name: &str, args: &[T<'h>]) -> Res<T<'h>> {
        let t = self.b.call(name, args);
        self.cx.eval(t)
    }
    pub(crate) fn eval(&mut self, t: T<'h>) -> Res<T<'h>> {
        self.cx.eval(t)
    }
    pub(crate) fn sym(&self, n: &str) -> T<'h> {
        let h = self.b.heap();
        h.sym_term(h.sym(n))
    }
    pub(crate) fn rule(&self, a: T<'h>, v: T<'h>) -> T<'h> {
        self.b.call(names::RULE, &[a, v])
    }
    pub(crate) fn list(&self, xs: &[T<'h>]) -> T<'h> {
        self.b.heap().call(self.b.hd.list, xs)
    }
    pub(crate) fn equal(&self, l: T<'h>, r: T<'h>) -> T<'h> {
        self.b.call("equal", &[l, r])
    }
    pub(crate) fn subst(&mut self, e: T<'h>, rules: &[T<'h>]) -> Res<T<'h>> {
        let l = self.list(rules);
        self.ev("rewrite", &[e, l])
    }
    pub(crate) fn simp(&mut self, t: T<'h>) -> Res<T<'h>> {
        self.ev("simplify", &[t])
    }
    pub(crate) fn expand(&mut self, t: T<'h>) -> Res<T<'h>> {
        self.ev("expand", &[t])
    }
    pub(crate) fn together(&mut self, t: T<'h>) -> Res<T<'h>> {
        self.ev("together", &[t])
    }
    pub(crate) fn d(&mut self, t: T<'h>, x: T<'h>) -> Res<T<'h>> {
        self.ev("d", &[t, x])
    }
    pub(crate) fn dn(&mut self, t: T<'h>, x: T<'h>, k: usize) -> Res<T<'h>> {
        match k {
            0 => Ok(t),
            1 => self.d(t, x),
            _ => {
                let spec = self.list(&[x, self.b.int(k as i64)]);
                self.ev("d", &[t, spec])
            }
        }
    }
    pub(crate) fn free(&self, t: T<'h>, v: T<'h>) -> bool {
        is_free(self.b, t, v)
    }
    pub(crate) fn free_all(&self, t: T<'h>, vs: &[T<'h>]) -> bool {
        vs.iter().all(|&v| is_free(self.b, t, v))
    }
    /// Is `t` an unevaluated call of `name` (or does it contain a failure)?
    pub(crate) fn stuck(&self, t: T<'h>, name: &str) -> bool {
        let h = self.b.heap();
        !is_free(self.b, t, h.sym_term(h.sym(name))) || !is_free(self.b, t, h.sym_term(h.sym(names::FAILURE)))
    }
    /// Does `t` hold a failure, an indeterminate or an infinity?
    pub(crate) fn bad(&self, t: T<'h>) -> bool {
        ["indeterminate", "complex_infinity", names::FAILURE, "infinity"].iter().any(|n| self.stuck(t, n))
    }
    /// The constant `c(i)`.
    pub(crate) fn constant(&self, i: usize) -> T<'h> {
        self.b.heap().app(self.gen, &[self.b.int(i as i64)])
    }
    /// The index of the constant `t`, if it is one.
    pub(crate) fn constant_index(&self, t: T<'h>) -> Option<usize> {
        match self.b.heap().view(t) {
            View::App { head, args: &[k] } if head == self.gen => match self.b.heap().view(k) {
                View::Int(i @ 1..) => Some(i as usize),
                _ => None,
            },
            _ => None,
        }
    }
    /// The constants `c(1)`, ..., `c(n)` that occur in `t`.
    pub(crate) fn constants_in(&self, t: T<'h>, n: usize) -> Vec<T<'h>> {
        (1..=n).map(|i| self.constant(i)).filter(|&ci| !is_free(self.b, t, ci)).collect()
    }
    /// The integral of `f` in `x`, `None` when it stays unevaluated.
    pub(crate) fn integrate(&mut self, f: T<'h>, x: T<'h>) -> Res<Option<T<'h>>> {
        if f == self.b.zero() {
            return Ok(Some(f));
        }
        if self.late() {
            return Ok(None);
        }
        let i = self.ev("integrate", &[f, x])?;
        Ok(if self.stuck(i, "integrate") { None } else { Some(i) })
    }
    /// The values of `v` from `solve(eq, v)` (generic: the conditions of conditional expressions dropped), `None`
    /// when `solve` fails.
    pub(crate) fn solve_for(&mut self, eq: T<'h>, v: T<'h>) -> Res<Option<Vec<T<'h>>>> {
        if self.late() {
            return Ok(None);
        }
        let s = self.ev("solve", &[eq, v])?;
        let Some(items) = self.items(s) else { return Ok(None) };
        let h = self.b.heap();
        let mut out = vec![];
        for it in items {
            let Some(rules) = self.items(it) else { return Ok(None) };
            let Some(&r) = rules.first() else { return Ok(None) };
            let View::App { args: &[_, val], .. } = h.view(r) else { return Ok(None) };
            let g = self.generic(val);
            out.push(if g == val { g } else { self.eval(g)? });
        }
        Ok(Some(out))
    }
    /// `e` without `conditional_expression` wrappers (the generic solution; an integer parameter of a family of
    /// solutions, `element(c(k), integers)`, set to 0: the principal branch).
    pub(crate) fn generic(&self, e: T<'h>) -> T<'h> {
        let b = self.b;
        let h = b.heap();
        match h.view(e) {
            View::App { head, args: &[v, cond] } if b.sym_of(head) == Some(h.sym("conditional_expression")) => {
                let mut pairs = vec![];
                let mut conds = vec![];
                flatten(b, cond, &mut conds);
                for cd in conds {
                    if let View::App { head: eh, args: &[k, dom] } = h.view(cd) {
                        if b.sym_of(eh) == Some(h.sym("element")) && b.sym_of(dom) == Some(h.sym("integers")) {
                            pairs.push((k, b.zero()));
                        }
                    }
                }
                let v = if pairs.is_empty() { v } else { replace_terms(b, v, &pairs) };
                self.generic(v)
            }
            _ => e,
        }
    }
    pub(crate) fn items(&self, t: T<'h>) -> Option<Vec<T<'h>>> {
        self.cx.as_list(t).map(|v| v.to_vec())
    }
    /// The summands of `t`.
    pub(crate) fn terms(&self, t: T<'h>) -> Vec<T<'h>> {
        let h = self.b.heap();
        match h.view(t) {
            View::App { head, args } if self.b.sym_of(head) == Some(self.b.hd.add) => args.to_vec(),
            _ => vec![t],
        }
    }
    /// The factors of `t`.
    pub(crate) fn factors(&self, t: T<'h>) -> Vec<T<'h>> {
        let h = self.b.heap();
        match h.view(t) {
            View::App { head, args } if self.b.sym_of(head) == Some(self.b.hd.mul) => args.to_vec(),
            _ => vec![t],
        }
    }
    /// Is `simplify(t) == 0`?
    pub(crate) fn is_zero(&mut self, t: T<'h>) -> Res<bool> {
        if t == self.b.zero() {
            return Ok(true);
        }
        let s = self.simp(t)?;
        if s == self.b.zero() {
            return Ok(true);
        }
        let s = self.ev("together", &[s])?;
        Ok(s == self.b.zero())
    }
    /// With `MC_DSOLVE_TRACE` set: the time left and `what` on stderr (development aid).
    pub(crate) fn trace(&self, what: &str) {
        if std::env::var_os("MC_DSOLVE_TRACE").is_some() {
            let left = self.deadline.saturating_duration_since(Instant::now());
            eprintln!("d_solve [{:.3}s left, depth {}] {what}", left.as_secs_f64(), self.depth);
        }
    }
    /// `f(0), f(1), ...` on the session's pool (roadmap item 16A), each on its own context (a builder of its own, the
    /// call's constants, deadline and depth), in order until `stop` holds of a value: the methods of a cascade, the
    /// checks of the solutions. The values are those of the sequential loop for any number of threads.
    pub(crate) fn par_until<R: Send>(&mut self, n: usize, stop: &(dyn Fn(&R) -> bool + Sync), f: &(dyn for<'a, 'b, 'c, 'k> Fn(&mut Cx<'a, 'b, 'c, 'k, 'h>, usize) -> Res<R> + Sync)) -> Res<Vec<R>> {
        let (gen, deadline, depth) = (self.gen, self.deadline, self.depth);
        let flag = self.b.stop().clone();
        self.cx.par_until(n, stop, &|cx, i| {
            let _scope = StopScope::enter(Some(&flag));
            let b = Build::new(cx.heap(), &flag);
            let mut c = Cx { cx, b: &b, gen, deadline, depth };
            f(&mut c, i)
        })
    }

    /// Past the time budget (or stopped)?
    pub(crate) fn late(&self) -> bool {
        Instant::now() > self.deadline || self.cx.stopped()
    }
    /// The head symbol name of `t`.
    pub(crate) fn head_is(&self, t: T<'h>, name: &str) -> bool {
        let h = self.b.heap();
        matches!(h.view(t), View::App { head, .. } if self.b.sym_of(head) == Some(h.sym(name)))
    }
}

/// The equations, flattened from lists and `and`.
pub(crate) fn flatten<'h>(b: &Build<'h>, t: T<'h>, out: &mut Vec<T<'h>>) {
    let h = b.heap();
    match h.view(t) {
        View::App { head, args } if b.sym_of(head) == Some(b.hd.list) || b.sym_of(head) == Some(h.sym("and")) => {
            for &a in args {
                flatten(b, a, out);
            }
        }
        _ => out.push(t),
    }
}

/// `(k, arg)` when `t` is `y(arg)` (k = 0) or `derivative(k)(y)(arg)`.
pub(crate) fn occurrence<'h>(b: &Build<'h>, t: T<'h>, y: T<'h>) -> Option<(usize, T<'h>)> {
    let h = b.heap();
    let View::App { head, args: &[arg] } = h.view(t) else { return None };
    if head == y {
        return Some((0, arg));
    }
    let (orders, f) = b.as_derivative(head)?;
    match orders.as_slice() {
        &[k] if f == y && k >= 1 => Some((k as usize, arg)),
        _ => None,
    }
}

/// Every occurrence of `y` in `t`: `(order, argument, term)`.
pub(crate) fn occurrences<'h>(b: &Build<'h>, t: T<'h>, y: T<'h>, out: &mut Vec<(usize, T<'h>, T<'h>)>) {
    let h = b.heap();
    let mut stack = vec![t];
    while let Some(u) = stack.pop() {
        if let Some((k, a)) = occurrence(b, u, y) {
            out.push((k, a, u));
            continue;
        }
        if let View::App { head, args } = h.view(u) {
            stack.push(head);
            stack.extend(args.iter().copied());
        }
    }
}

/// The unknown functions of `eqs` (heads of derivatives, in order of appearance) with the argument lists they are
/// applied to.
fn unknowns<'h>(b: &Build<'h>, eqs: &[T<'h>]) -> Vec<(T<'h>, Vec<T<'h>>)> {
    let h = b.heap();
    let mut out: Vec<(T<'h>, Vec<T<'h>>)> = vec![];
    fn walk<'h>(b: &Build<'h>, t: T<'h>, out: &mut Vec<(T<'h>, Vec<T<'h>>)>) {
        let h = b.heap();
        if let View::App { head, args } = h.view(t) {
            if let Some((_, f)) = b.as_derivative(head) {
                if matches!(h.view(f), View::Sym(_)) && !out.iter().any(|o| o.0 == f) {
                    out.push((f, args.to_vec()));
                }
            }
            walk(b, head, out);
            for &a in args {
                walk(b, a, out);
            }
        }
    }
    for &e in eqs {
        walk(b, e, &mut out);
    }
    // functions applied in the equations that are differentiated nowhere (an algebraic unknown of a system) are not
    // unknowns here: the caller names them
    let _ = h;
    out
}

/// The unknown of an equation in operator form: `f` of `derivative(k)(f)` occurring unapplied.
fn operator_unknown<'h>(b: &Build<'h>, eqs: &[T<'h>]) -> Option<T<'h>> {
    let h = b.heap();
    let mut stack: Vec<T<'h>> = eqs.to_vec();
    while let Some(t) = stack.pop() {
        if let Some((orders, f)) = b.as_derivative(t) {
            if orders.len() == 1 && matches!(h.view(f), View::Sym(_)) {
                return Some(f);
            }
        }
        if let View::App { head, args } = h.view(t) {
            stack.push(head);
            stack.extend(args.iter().copied());
        }
    }
    None
}

/// `t` with `derivative(k)(f)` and a bare `f` applied to `x`.
fn apply_operator<'h>(b: &Build<'h>, t: T<'h>, f: T<'h>, x: T<'h>) -> T<'h> {
    let h = b.heap();
    if t == f || b.as_derivative(t).is_some_and(|d| d.1 == f) {
        return h.app(t, &[x]);
    }
    match h.view(t) {
        View::App { head, args } => {
            let new: Vec<T<'h>> = args.iter().map(|&a| apply_operator(b, a, f, x)).collect();
            h.app(head, &new)
        }
        _ => t,
    }
}

/// What the second argument asks for.
struct Spec<'h> {
    /// the unknown functions
    funs: Vec<T<'h>>,
    /// per function: given as `y(x)` (true) or `y` (false)
    applied: Vec<bool>,
    /// a list was given
    listed: bool,
    /// `d_solve_value` of another expression (`y(3)`, `y'(x)`, ...)
    target: Option<T<'h>>,
}

impl Native for DSolveNative {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let (plain, opts) = split_options(h, args);
        let _scope = StopScope::enter(Some(&self.stop));
        let b = Build::new(h, &self.stop);
        let gen = option(h, &opts, "generated_parameters").unwrap_or_else(|| h.sym_term(h.sym("c")));
        if !matches!(h.view(gen), View::Sym(_)) {
            return Ok(Reply::Decline);
        }
        let mut c = Cx { cx, b: &b, gen, deadline: Instant::now() + BUDGET, depth: 0 };
        match self.kind {
            Kind::RSolve | Kind::RSolveValue => {
                let [eqs, yspec, x] = plain.as_slice() else { return Ok(Reply::Decline) };
                if !matches!(h.view(*x), View::Sym(_)) {
                    return Ok(Reply::Decline);
                }
                let (y, applied) = match h.view(*yspec) {
                    View::Sym(_) => (*yspec, false),
                    View::App { head, args: &[a] } if a == *x && matches!(h.view(head), View::Sym(_)) => (head, true),
                    _ => return Ok(Reply::Decline),
                };
                let Some(sols) = recur::r_solve(&mut c, *eqs, y, *x)? else { return Ok(Reply::Decline) };
                if self.kind == Kind::RSolveValue {
                    return Ok(Reply::Done(match sols.as_slice() {
                        [s] => *s,
                        _ => c.list(&sols),
                    }));
                }
                let mut out = vec![];
                for s in sols {
                    let r = if applied { c.rule(*yspec, s) } else { c.rule(y, b.call("fn", &[c.list(&[*x]), s])) };
                    out.push(c.list(&[r]));
                }
                Ok(Reply::Done(c.list(&out)))
            }
            Kind::AsymptoticValue => Ok(match series::asymptotic(&mut c, &plain)? {
                Some(v) => Reply::Done(v),
                None => Reply::Decline,
            }),
            Kind::DSolve | Kind::DSolveValue => {
                let value = self.kind == Kind::DSolveValue;
                Ok(match d_solve(&mut c, &plain, value)? {
                    Some(v) => Reply::Done(v),
                    None => Reply::Decline,
                })
            }
        }
    }
}

/// The trailing `options(...)` (or rules) of `args`, split off.
fn split_options<'h>(h: &'h mc_core::Heap, args: &[T<'h>]) -> (Vec<T<'h>>, Vec<T<'h>>) {
    let mut plain = args.to_vec();
    let mut opts = vec![];
    while let Some(&last) = plain.last() {
        match h.view(last) {
            View::App { head, args: rules } if head == h.sym_term(h.sym("options")) => {
                opts.extend_from_slice(rules);
                plain.pop();
            }
            View::App { head, args: &[k, _] } if head == h.sym_term(h.sym(names::RULE)) && k == h.sym_term(h.sym("generated_parameters")) && plain.len() > 1 => {
                opts.push(last);
                plain.pop();
            }
            _ => break,
        }
    }
    (plain, opts)
}

/// The option `name` among `rules`.
fn option<'h>(h: &'h mc_core::Heap, rules: &[T<'h>], name: &str) -> Option<T<'h>> {
    let key = h.sym_term(h.sym(name));
    rules.iter().find_map(|&r| match h.view(r) {
        View::App { args: &[k, v], .. } if k == key => Some(v),
        _ => None,
    })
}

/// A solution: explicit `y = value` or implicit `F(x, y) == 0` (an equation in the symbol of `y`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Sol<'h> {
    /// `y = value`
    E(T<'h>),
    /// an equation in `x` and the symbol of `y`
    I(T<'h>),
}

/// What a solver returns.
pub(crate) enum Out<'h> {
    /// solutions: per solution, one value per unknown
    Sols(Vec<Vec<T<'h>>>),
    /// an implicit solution: the equation in `y(x)`
    Implicit(T<'h>),
}

/// `d_solve` and `d_solve_value`.
fn d_solve<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, plain: &[T<'h>], value: bool) -> Res<Option<T<'h>>> {
    let b = c.b;
    let h = b.heap();
    let Some(&eqs) = plain.first() else { return Ok(None) };
    if plain.len() > 3 {
        return Ok(None);
    }
    let mut all = vec![];
    flatten(b, eqs, &mut all);
    if all.is_empty() || all.iter().any(|&e| !c.head_is(e, "equal")) {
        return Ok(None);
    }
    let mut found = unknowns(b, &all);
    // operator form `y' == y`: the functions applied to the formal variable
    let mut formal = false;
    if found.is_empty() && plain.len() == 1 {
        let Some(f) = operator_unknown(b, &all) else { return Ok(None) };
        let fx = c.sym("FormalX");
        let mut eqs2 = vec![];
        for &e in &all {
            eqs2.push(apply_operator(b, e, f, fx));
        }
        all = eqs2;
        found = vec![(f, vec![fx])];
        formal = true;
    }
    // the unknowns
    let spec = match plain.get(1) {
        None => {
            if found.is_empty() {
                return Ok(None);
            }
            if formal {
                Spec { funs: vec![found[0].0], applied: vec![false], listed: false, target: None }
            } else {
            Spec { funs: found.iter().map(|f| f.0).collect(), applied: vec![true; found.len()], listed: found.len() > 1, target: None }
            }
        }
        Some(&s) => match spec_of(c, s, &found, value) {
            Some(sp) => sp,
            None => return Ok(None),
        },
    };
    // the variables (and a range)
    let vars: Vec<T<'h>> = match plain.get(2) {
        Some(&x) if matches!(h.view(x), View::Sym(_)) => vec![x],
        Some(&l) => {
            let Some(items) = c.items(l) else { return Ok(None) };
            if items.is_empty() || !matches!(h.view(items[0]), View::Sym(_)) {
                return Ok(None);
            }
            if items.iter().all(|&v| matches!(h.view(v), View::Sym(_))) {
                items
            } else if items.len() == 3 {
                vec![items[0]]
            } else {
                return Ok(None);
            }
        }
        None => {
            // from the arguments of the unknowns: y(x) in the spec, or the derivatives in the equations
            let mut vs: Option<Vec<T<'h>>> = None;
            if let Some(&s) = plain.get(1) {
                for item in c.items(s).unwrap_or_else(|| vec![s]) {
                    if let View::App { head, args } = h.view(item) {
                        if spec.funs.contains(&head) && args.iter().all(|&a| matches!(h.view(a), View::Sym(_))) {
                            vs = Some(args.to_vec());
                        }
                    }
                }
            }
            if vs.is_none() {
                vs = found.iter().find(|f| spec.funs.contains(&f.0)).map(|f| f.1.clone());
            }
            match vs {
                Some(v) if v.iter().all(|&a| matches!(h.view(a), View::Sym(_))) => v,
                _ => return Ok(None),
            }
        }
    };
    let out = if vars.len() == 1 {
        let x = vars[0];
        if spec.funs.len() == 1 {
            let y = spec.funs[0];
            if delay::is_delay(c, &all, y, x) {
                delay::solve(c, &all, y, x, plain.get(2).copied())?
            } else {
                single(c, &all, y, x)?
            }
        } else {
            system::solve(c, &all, &spec.funs, x)?
        }
    } else if vars.len() == 2 && spec.funs.len() == 1 {
        pde::solve(c, &all, spec.funs[0], &vars)?
    } else {
        None
    };
    let Some(out) = out else { return Ok(None) };
    if value && vars.len() == 1 {
        if let (Some(target), Out::Sols(sols)) = (spec.target, &out) {
            if let Some(f) = blow_up_before(c, &all, &spec, target, sols, vars[0])? {
                return Ok(Some(f));
            }
        }
    }
    Ok(Some(output(c, out, &spec, &vars, value)?))
}

/// A solution found for an initial value problem lives on the interval around the initial point that holds no
/// singularity of the solution; the closed form continues analytically past a pole (`1/(1 - x)` for `y' = y^2`,
/// `y(0) = 1` has the value -1 at 2, but the solution ends at 1). When the target is at a real point on the far
/// side of a real zero of a denominator or a logarithm's argument in the solution, the answer is
/// `failure(blow_up, d_solve_value, x0, x1)` instead of the continued value.
fn blow_up_before<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, all: &[T<'h>], spec: &Spec<'h>, target: T<'h>, sols: &[Vec<T<'h>>], x: T<'h>) -> Res<Option<T<'h>>> {
    let b = c.b;
    let real = |c: &mut Cx<'_, '_, '_, '_, 'h>, t: T<'h>| -> Res<Option<f64>> {
        let v = c.ev("n", &[t])?;
        Ok(match c.b.ar.num(v) {
            Some(mc_arith::Num::Real(r)) => Some(r.to_f64()),
            _ => None,
        })
    };
    // the initial points and the target points
    let (mut inits, mut points) = (vec![], vec![]);
    for &f in &spec.funs {
        let mut occ = vec![];
        for &e in all {
            occurrences(b, e, f, &mut occ);
        }
        for (_, a, _) in occ {
            if let Some(v) = real(c, a)? {
                inits.push(v);
            }
        }
        let mut occ = vec![];
        occurrences(b, target, f, &mut occ);
        for (_, a, _) in occ {
            if let Some(v) = real(c, a)? {
                points.push(v);
            }
        }
    }
    if inits.is_empty() || points.is_empty() {
        return Ok(None);
    }
    for s in sols {
        for &body in s {
            // candidates: bases of negative powers and logarithm arguments
            // kind 0: a zero of the argument is a singularity; 1: tan (poles at pi/2 + k pi); 2: cot (poles at
            // k pi); 3: a fractional power (the solution turns complex where the base changes sign)
            let mut cands: Vec<(u8, T<'h>)> = vec![];
            let h = b.heap();
            let mut stack = vec![body];
            while let Some(u) = stack.pop() {
                if let View::App { head, args } = h.view(u) {
                    if c.head_is(u, "pow") && args.len() == 2 {
                        let neg = real(c, args[1])?.map(|e| e < 0.0).unwrap_or(false);
                        let frac = real(c, args[1])?.map(|e| e.fract() != 0.0).unwrap_or(false);
                        if !is_free(b, args[0], x) {
                            if frac {
                                cands.push((3, args[0]));
                            } else if neg {
                                cands.push((0, args[0]));
                            }
                        }
                    } else if args.len() == 1 && !is_free(b, args[0], x) {
                        if c.head_is(u, "log") {
                            cands.push((0, args[0]));
                        } else if c.head_is(u, "tan") {
                            cands.push((1, args[0]));
                        } else if c.head_is(u, "cot") {
                            cands.push((2, args[0]));
                        }
                    }
                    stack.push(head);
                    stack.extend(args.iter().copied());
                }
            }
            let mut uniq: Vec<(u8, T<'h>)> = vec![];
            for g in cands {
                if !uniq.contains(&g) {
                    uniq.push(g);
                }
            }
            let cands = uniq;
            for &x0 in &inits {
                for &x1 in &points {
                    if x0 == x1 {
                        continue;
                    }
                    for &(kind, g) in &cands {
                        const N: usize = 240;
                        let mut prev: Option<f64> = None;
                        for k in 0..=N {
                            if c.late() {
                                return Ok(None);
                            }
                            let xv = x0 + (x1 - x0) * (k as f64) / (N as f64);
                            let pt = c.rule(x, h.real(xv));
                            let at = c.subst(g, &[pt])?;
                            let Some(v) = real(c, at)? else { prev = None; continue };
                            // tan and cot: the argument's position among the poles (units of pi, tan shifted by half)
                            let v = match kind {
                                1 => v / std::f64::consts::PI - 0.5,
                                2 => v / std::f64::consts::PI,
                                _ => v,
                            };
                            let hit = match kind {
                                1 | 2 => prev.is_some_and(|p| p.floor() != v.floor()) || (k > 0 && v == v.floor()),
                                3 => prev.is_some_and(|p| p * v < 0.0),
                                _ => (k > 0 && v == 0.0) || prev.is_some_and(|p| p * v < 0.0),
                            };
                            if hit {
                                let t = b.call("failure", &[h.sym_term(h.sym("blow_up")), h.sym_term(h.sym("d_solve_value")), h.real(x0), h.real(x1)]);
                                return Ok(Some(t));
                            }
                            prev = Some(v);
                        }
                    }
                }
            }
        }
    }
    Ok(None)
}

/// The unknowns named by the second argument.
fn spec_of<'h>(c: &Cx<'_, '_, '_, '_, 'h>, s: T<'h>, found: &[(T<'h>, Vec<T<'h>>)], value: bool) -> Option<Spec<'h>> {
    let h = c.b.heap();
    let one = |t: T<'h>| -> Option<(T<'h>, bool)> {
        match h.view(t) {
            View::Sym(_) => Some((t, false)),
            View::App { head, args } if matches!(h.view(head), View::Sym(_)) && args.iter().all(|&a| matches!(h.view(a), View::Sym(_))) => Some((head, true)),
            _ => None,
        }
    };
    if let Some(items) = c.items(s) {
        let mut funs = vec![];
        let mut applied = vec![];
        for it in items {
            let (f, a) = one(it)?;
            funs.push(f);
            applied.push(a);
        }
        return Some(Spec { funs, applied, listed: true, target: None });
    }
    if let Some((f, a)) = one(s) {
        // a symbol that is no unknown of the equations (d_solve_value(eqs, y(3), x) is handled below)
        if found.iter().any(|g| g.0 == f) || found.is_empty() {
            return Some(Spec { funs: vec![f], applied: vec![a], listed: false, target: None });
        }
    }
    if !value {
        return None;
    }
    // d_solve_value of an expression in the unknowns
    let funs: Vec<T<'h>> = found.iter().map(|f| f.0).filter(|&f| !is_free(c.b, s, f)).collect();
    if funs.is_empty() {
        return None;
    }
    let n = funs.len();
    Some(Spec { funs, applied: vec![true; n], listed: n > 1, target: Some(s) })
}

/// The result terms.
fn output<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, out: Out<'h>, spec: &Spec<'h>, vars: &[T<'h>], value: bool) -> Res<T<'h>> {
    let b = c.b;
    let h = b.heap();
    let sols = match out {
        Out::Implicit(eq) => {
            let y = spec.funs[0];
            let yx = h.app(y, vars);
            return Ok(b.call("solve", &[eq, yx]));
        }
        Out::Sols(s) => s,
    };
    let fns = |c: &Cx<'_, '_, '_, '_, 'h>, s: &[T<'h>]| -> Vec<T<'h>> { s.iter().map(|&v| b.call("fn", &[c.list(vars), display(c, v, vars)])).collect() };
    if value {
        let mut vals = vec![];
        for s in &sols {
            if let Some(t) = spec.target {
                let rules: Vec<T<'h>> = spec.funs.iter().zip(fns(c, s)).map(|(&f, g)| c.rule(f, g)).collect();
                let v = c.subst(t, &rules)?;
                vals.push(v);
                continue;
            }
            let per: Vec<T<'h>> = s.iter().zip(fns(c, s)).zip(&spec.applied).map(|((&v, g), &a)| if a { v } else { g }).collect();
            vals.push(if spec.listed { c.list(&per) } else { per[0] });
        }
        return Ok(match vals.as_slice() {
            [v] => *v,
            _ => c.list(&vals),
        });
    }
    let mut out = vec![];
    for s in &sols {
        let gs = fns(c, s);
        let rules: Vec<T<'h>> = spec
            .funs
            .iter()
            .enumerate()
            .map(|(i, &f)| if spec.applied[i] { c.rule(h.app(f, vars), s[i]) } else { c.rule(f, gs[i]) })
            .collect();
        out.push(c.list(&rules));
    }
    Ok(c.list(&out))
}

/// The body of a pure function solution in the conventional written form (the body is held, so it prints as
/// built): `(...)/k` for `(1/k) (...)`, and the arguments of arbitrary functions `c(1)(-3 x + y)` with their terms
/// in the order of the variables.
fn display<'h>(c: &Cx<'_, '_, '_, '_, 'h>, v: T<'h>, vars: &[T<'h>]) -> T<'h> {
    let b = c.b;
    let h = b.heap();
    // arbitrary functions of sums
    let mut pairs = vec![];
    let mut stack = vec![v];
    while let Some(t) = stack.pop() {
        if let View::App { head, args } = h.view(t) {
            if c.constant_index(head).is_some() {
                if let [arg] = args {
                    let ts = c.terms(*arg);
                    if ts.len() > 1 {
                        // a term's rank: the alphabetical rank of the first variable it holds (constants last)
                        let mut names: Vec<(String, usize)> = vars.iter().enumerate().map(|(j, &w)| (sym_name(c, w), j)).collect();
                        names.sort();
                        let rank = |u: T<'h>| -> usize {
                            names.iter().position(|n| !is_free(b, u, vars[n.1])).unwrap_or(usize::MAX)
                        };
                        let mut ts = ts.clone();
                        ts.sort_by_key(|&u| rank(u));
                        let raw = h.app(h.sym_term(b.hd.add), &ts);
                        pairs.push((t, h.app(head, &[raw])));
                    }
                }
                continue;
            }
            stack.push(head);
            stack.extend(args.iter().copied());
        }
    }
    let v = if pairs.is_empty() { v } else { replace_terms(b, v, &pairs) };
    // constants after the other factors of a product: exp(x)*c(1)
    let v = constants_last(c, v);
    // (1/k) (sum) as (sum)/k
    if let View::App { head, args: &[r, rest] } = h.view(v) {
        if b.sym_of(head) == Some(b.hd.mul) && c.terms(rest).len() > 1 {
            if let View::Rational { num, den } = h.view(r) {
                if num == b.one() {
                    let inv = h.app(h.sym_term(b.hd.pow), &[den, b.int(-1)]);
                    return h.app(head, &[rest, inv]);
                }
            }
        }
    }
    v
}

/// Products with their constant factors `c(k)` (or `c(k)(...)`) moved last, rebuilt as written.
fn constants_last<'h>(c: &Cx<'_, '_, '_, '_, 'h>, t: T<'h>) -> T<'h> {
    let b = c.b;
    let h = b.heap();
    let View::App { head, args } = h.view(t) else { return t };
    let new: Vec<T<'h>> = args.iter().map(|&a| constants_last(c, a)).collect();
    let is_const = |u: T<'h>| c.constant_index(u).is_some() || matches!(h.view(u), View::App { head: hh, .. } if c.constant_index(hh).is_some());
    if b.sym_of(head) == Some(b.hd.mul) && new.iter().any(|&u| is_const(u)) {
        let mut v: Vec<T<'h>> = new.iter().copied().filter(|&u| !is_const(u)).collect();
        v.extend(new.iter().copied().filter(|&u| is_const(u)));
        return h.app(head, &v);
    }
    if new.as_slice() == args {
        t
    } else {
        h.app(head, &new)
    }
}

/// The name of a symbol term.
fn sym_name<'h>(c: &Cx<'_, '_, '_, '_, 'h>, t: T<'h>) -> String {
    let h = c.b.heap();
    match h.view(t) {
        View::Sym(s) => h.sym_name(s),
        _ => String::new(),
    }
}

/// The differential equation and the conditions `(order, point, value)`.
pub(crate) struct Problem<'h> {
    pub(crate) eq: T<'h>,
    pub(crate) conds: Vec<(usize, T<'h>, T<'h>)>,
}

/// Split `all` into the equation in `x` and the conditions at points.
pub(crate) fn problem<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, all: &[T<'h>], y: T<'h>, x: T<'h>) -> Option<Problem<'h>> {
    let b = c.b;
    let h = b.heap();
    let mut eq = None;
    let mut conds = vec![];
    for &e in all {
        let View::App { head, args: &[l, r] } = h.view(e) else { return None };
        if b.sym_of(head) != Some(h.sym("equal")) {
            return None;
        }
        let mut occ = vec![];
        occurrences(b, e, y, &mut occ);
        if occ.is_empty() {
            return None;
        }
        if occ.iter().all(|o| o.1 == x) {
            if eq.is_some() {
                return None;
            }
            eq = Some(e);
            continue;
        }
        // a condition: one side an occurrence at a point free of x
        let (o, v) = match (occurrence(b, l, y), occurrence(b, r, y)) {
            (Some(o), _) if is_free(b, r, y) => (o, r),
            (_, Some(o)) if is_free(b, l, y) => (o, l),
            _ => return None,
        };
        if !is_free(b, o.1, x) {
            return None;
        }
        conds.push((o.0, o.1, v));
    }
    Some(Problem { eq: eq?, conds })
}

/// An equation in the symbols `ys[k]` for `y^(k)`: `diff == 0`.
#[derive(Clone)]
pub(crate) struct Ode<'h> {
    /// the expanded difference of the two sides
    pub(crate) diff: T<'h>,
    /// the symbols for y, y', ..., y^(n)
    pub(crate) ys: Vec<T<'h>>,
    pub(crate) x: T<'h>,
    pub(crate) n: usize,
}

/// The symbols for `y, y', ...` at nesting `depth`.
pub(crate) fn ysyms<'h>(c: &Cx<'_, '_, '_, '_, 'h>, n: usize) -> Vec<T<'h>> {
    (0..=n).map(|k| c.sym(&format!("d_solve·{}·{k}", c.depth))).collect()
}

/// The equation `eq` in `y(x)` as an [`Ode`].
pub(crate) fn ode_of<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, eq: T<'h>, y: T<'h>, x: T<'h>) -> Res<Option<Ode<'h>>> {
    let b = c.b;
    let mut occ = vec![];
    occurrences(b, eq, y, &mut occ);
    if occ.iter().any(|o| o.1 != x) {
        return Ok(None);
    }
    let n = occ.iter().map(|o| o.0).max().unwrap_or(0);
    if n == 0 || n > MAX_ORDER {
        return Ok(None);
    }
    let ys = ysyms(c, n);
    let pairs: Vec<(T<'h>, T<'h>)> = occ.iter().map(|&(k, _, term)| (term, ys[k])).collect();
    let View::App { args: &[l, r], .. } = b.heap().view(eq) else { return Ok(None) };
    let diff = c.eval(replace_terms(b, b.sub(l, r), &pairs))?;
    let diff = c.expand(diff)?;
    Ok(Some(Ode { diff, ys, x, n }))
}

/// One ODE for one unknown, with its conditions.
pub(crate) fn single<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, all: &[T<'h>], y: T<'h>, x: T<'h>) -> Res<Option<Out<'h>>> {
    let Some(p) = problem(c, all, y, x) else { return Ok(None) };
    let Some(ode) = ode_of(c, p.eq, y, x)? else { return Ok(None) };
    let Some(general) = general(c, &ode)? else { return quadrature(c, &ode, &p.conds) };
    let yx = c.b.heap().app(y, &[x]);
    finish(c, &ode, general, &p.conds, yx)
}

/// `y' = f(x)` whose integral has no closed form: the solution is the integral itself, `c(1) + integrate(f(x), x)`,
/// or with `y(x0) = y0` the definite integral `y0 + integrate(f(t), [t, x0, x])` (the fundamental theorem of
/// calculus; the integrand is left as the user wrote it, a function symbol such as `f(x)` included).
fn quadrature<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, ode: &Ode<'h>, conds: &[(usize, T<'h>, T<'h>)]) -> Res<Option<Out<'h>>> {
    let b = c.b;
    let h = b.heap();
    if ode.n != 1 {
        return Ok(None);
    }
    let (y, p, x) = (ode.ys[0], ode.ys[1], ode.x);
    let eq = c.equal(ode.diff, b.zero());
    let Some(branches) = c.solve_for(eq, p)? else { return Ok(None) };
    let [g] = branches.as_slice() else { return Ok(None) };
    let g = *g;
    if !c.free(g, p) || !c.free(g, y) {
        return Ok(None);
    }
    let sol = match conds {
        [] => b.add(&[c.constant(1), b.call("integrate", &[g, x])]),
        [(0, x0, y0)] if is_free(b, *x0, x) && is_free(b, *y0, x) => {
            // a dummy variable that g does not hold
            let t = ["t", "s", "u", "w", "v"].iter().map(|n| h.sym_term(h.sym(n))).find(|&t| is_free(b, g, t) && is_free(b, *x0, t));
            let Some(t) = t else { return Ok(None) };
            let rule = c.rule(x, t);
            let gt = c.subst(g, &[rule])?;
            b.add(&[*y0, b.call("integrate", &[gt, h.call(b.hd.list, &[t, *x0, x])])])
        }
        _ => return Ok(None),
    };
    // the integral stays as written: it must not be evaluated again into something else
    Ok(Some(Out::Sols(vec![vec![sol]])))
}

/// Verified solutions with the conditions applied.
pub(crate) fn finish<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, ode: &Ode<'h>, general: Vec<Sol<'h>>, conds: &[(usize, T<'h>, T<'h>)], yx: T<'h>) -> Res<Option<Out<'h>>> {
    let b = c.b;
    let mut sols = vec![];
    let mut implicit = None;
    c.trace("verify");
    // the explicit solutions checked in parallel (roadmap item 16A)
    let explicit: Vec<T<'h>> = general.iter().filter_map(|g| if let Sol::E(s) = g { Some(*s) } else { None }).collect();
    let verdicts = c.par_until(explicit.len(), &|_: &verify::Verdict| false, &|c, i| verify::check(c, ode, explicit[i]))?;
    let mut verdicts = verdicts.into_iter();
    for g in general {
        match g {
            Sol::E(s) => {
                if verdicts.next() != Some(verify::Verdict::Fail) {
                    sols.push(s);
                }
            }
            Sol::I(e) => {
                if implicit.is_none() {
                    implicit = Some(e);
                }
            }
        }
    }
    if sols.is_empty() {
        let Some(e) = implicit else { return Ok(None) };
        let e = if conds.is_empty() { e } else {
            match implicit_conditions(c, e, conds, ode)? {
                Some(e) => e,
                None => return Ok(None),
            }
        };
        let e = replace_terms(b, e, &[(ode.ys[0], yx)]);
        return Ok(Some(Out::Implicit(e)));
    }
    if conds.is_empty() {
        return Ok(Some(Out::Sols(sols.into_iter().map(|s| vec![s]).collect())));
    }
    let mut out = vec![];
    for g in sols {
        match conditions(c, g, conds, ode.x, ode.n)? {
            Some(v) => out.extend(v),
            None => return Ok(None),
        }
    }
    // a branch that does not meet the conditions (a sign of a root) is dropped
    let mut kept = vec![];
    for s in out {
        if meets(c, s, conds, ode.x)? && !kept.contains(&s) {
            kept.push(s);
        }
    }
    Ok(Some(Out::Sols(kept.into_iter().map(|s| vec![s]).collect())))
}

/// The general solutions of `ode`.
pub(crate) fn general<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, ode: &Ode<'h>) -> Res<Option<Vec<Sol<'h>>>> {
    if c.late() || c.depth > 4 {
        return Ok(None);
    }
    if let Some((coefs, f)) = linear::coefficients(c, ode)? {
        return Ok(linear::solve(c, &coefs, f, ode.x)?.map(|s| vec![Sol::E(s)]));
    }
    if ode.n == 1 {
        return first::solve(c, ode);
    }
    first::reduce(c, ode)
}

/// The solution `g` (with constants) under the conditions: the constants solved for (one solution per solution of
/// the system).
pub(crate) fn conditions<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, g: T<'h>, conds: &[(usize, T<'h>, T<'h>)], x: T<'h>, n: usize) -> Res<Option<Vec<T<'h>>>> {
    let b = c.b;
    if conds.is_empty() {
        return Ok(Some(vec![g]));
    }
    let mut eqs = vec![];
    for &(k, x0, v) in conds {
        let dk = c.dn(g, x, k)?;
        let at = c.subst(dk, &[c.rule(x, x0)])?;
        let at = if c.bad(at) { c.ev("limit", &[dk, c.rule(x, x0)])? } else { at };
        eqs.push(b.call("equal", &[at, v]));
    }
    let used = c.constants_in(g, n.max(MAX_ORDER));
    if used.is_empty() {
        return Ok(Some(vec![g]));
    }
    let s = c.ev("solve", &[c.list(&eqs), c.list(&used)])?;
    if c.stuck(s, "solve") || c.items(s).is_none() {
        // root objects and the like: the conditions met with every constant 0 (a linear equation whose particular
        // solution meets them; the solution is then unique when the system is regular)
        let zeros: Vec<T<'h>> = used.iter().map(|&k| c.rule(k, b.zero())).collect();
        let mut all_zero = true;
        for &e in &eqs {
            let View::App { args: &[l, r], .. } = b.heap().view(e) else { return Ok(None) };
            let d = c.subst(b.sub(l, r), &zeros)?;
            if !c.is_zero(d)? {
                all_zero = false;
                break;
            }
        }
        if all_zero {
            let v = c.subst(g, &zeros)?;
            return Ok(Some(vec![tidy(c, v, x)?]));
        }
        return Ok(None);
    }
    let Some(items) = c.items(s) else { return Ok(None) };
    if items.is_empty() {
        return Ok(Some(vec![]));
    }
    let mut out = vec![];
    for rs in items {
        let Some(rules) = c.items(rs) else { return Ok(None) };
        let rules: Vec<T<'h>> = rules.into_iter().map(|r| c.generic(r)).collect();
        let v = c.subst(g, &rules)?;
        let v = c.generic(v);
        out.push(tidy(c, v, x)?);
    }
    Ok(Some(out))
}

/// A solution with its constants fixed: the shorter of the simplified form and its `together` form, with decaying
/// exponentials `exp(-k x)` taken out as a factor when they mix with other terms (`exp(-x) (...)`, the `together`
/// form written without `1/exp(x)` as a denominator).
pub(crate) fn tidy<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, v: T<'h>, x: T<'h>) -> Res<T<'h>> {
    let b = c.b;
    let s = c.simp(v)?;
    let t = c.together(s)?;
    let ls = c.ev("leaf_count", &[s])?;
    let lt = c.ev("leaf_count", &[t])?;
    let h = b.heap();
    let best = match (h.view(ls), h.view(lt)) {
        (View::Int(a), View::Int(bb)) if bb <= a + 2 => t,
        _ => s,
    };
    // the most negative rate k of the exponentials exp(k x + ...)
    let e = c.expand(best)?;
    let terms = c.terms(e);
    if terms.len() < 2 {
        return Ok(best);
    }
    let mut rates = vec![];
    for &tm in &terms {
        let mut r = 0.0f64;
        let mut rt = b.zero();
        for f in c.factors(tm) {
            if let View::App { head, args: &[base, ex] } = h.view(f) {
                if b.sym_of(head) == Some(b.hd.pow) && b.sym_of(base) == Some(b.hd.e) && !is_free(b, ex, x) {
                    let k = c.d(ex, x)?;
                    if let Some(mc_arith::Num::Real(kv)) = b.ar.num(k) {
                        r += kv.to_f64();
                        rt = b.add(&[rt, k]);
                    }
                }
            }
        }
        rates.push((r, rt));
    }
    let Some(&(mr, mt)) = rates.iter().min_by(|a, bb| a.0.partial_cmp(&bb.0).unwrap_or(std::cmp::Ordering::Equal)) else { return Ok(best) };
    if mr >= 0.0 || rates.iter().all(|r| r.0 == mr) {
        return Ok(best);
    }
    let shifted = c.expand(b.mul(&[e, b.call("exp", &[b.neg(b.mul(&[mt, x]))])]))?;
    let f = c.ev("factor", &[shifted])?;
    let out = b.mul(&[f, b.call("exp", &[b.mul(&[mt, x])])]);
    c.eval(out)
}

/// Does the solution `s` meet the conditions (exactly, or numerically when that is undecided)?
fn meets<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, s: T<'h>, conds: &[(usize, T<'h>, T<'h>)], x: T<'h>) -> Res<bool> {
    for &(k, x0, v) in conds {
        let dk = c.dn(s, x, k)?;
        let at = c.subst(dk, &[c.rule(x, x0)])?;
        if c.bad(at) {
            continue;
        }
        let diff = c.b.sub(at, v);
        if c.is_zero(diff)? {
            continue;
        }
        if let Some(m) = verify::magnitude(c, diff)? {
            let scale = verify::magnitude(c, v)?.unwrap_or(1.0).max(1.0);
            if m > 1e-9 * scale {
                return Ok(false);
            }
        }
    }
    Ok(true)
}

/// An implicit solution `F(x, y, c(1)) == 0` with a condition `y(x0) == v`: `c(1)` solved for.
fn implicit_conditions<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, e: T<'h>, conds: &[(usize, T<'h>, T<'h>)], ode: &Ode<'h>) -> Res<Option<T<'h>>> {
    let [(0, x0, v)] = conds else { return Ok(None) };
    let at = c.subst(e, &[c.rule(ode.x, *x0), c.rule(ode.ys[0], *v)])?;
    let c1 = c.constant(1);
    let Some(vals) = c.solve_for(at, c1)? else { return Ok(None) };
    let Some(&k) = vals.first() else { return Ok(None) };
    let r = c.subst(e, &[c.rule(c1, k)])?;
    Ok(Some(r))
}

/// `t` with each subterm equal to a `from` of `pairs` replaced by its `to` (exact terms, not patterns: the
/// occurrences `y'(x)` are calls with compound heads, which rules cannot match).
pub(crate) fn replace_terms<'h>(b: &Build<'h>, t: T<'h>, pairs: &[(T<'h>, T<'h>)]) -> T<'h> {
    if let Some(&(_, to)) = pairs.iter().find(|p| p.0 == t) {
        return to;
    }
    let h = b.heap();
    match h.view(t) {
        View::App { head, args } => {
            let nh = replace_terms(b, head, pairs);
            let new: Vec<T<'h>> = args.iter().map(|&a| replace_terms(b, a, pairs)).collect();
            if new.as_slice() == args && nh == head {
                t
            } else {
                h.app(nh, &new)
            }
        }
        _ => t,
    }
}

/// `sum c(i) y_i` over a basis, the constants numbered from `first`.
pub(crate) fn combination<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, ys: &[T<'h>], first: usize) -> Res<T<'h>> {
    let b = c.b;
    let terms: Vec<T<'h>> = ys.iter().enumerate().map(|(i, &y)| b.mul(&[c.constant(i + first), y])).collect();
    c.eval(b.add(&terms))
}

/// Register the natives of this crate with `k`. `stop` is the flag `mc-arith`'s number kernels poll (the
/// engine's arithmetic stop flag).
pub fn install(k: &mut mc_eval::Kernel, h: &mc_core::Heap, stop: &Arc<AtomicBool>) {
    for (name, kind) in [
        ("d_solve", Kind::DSolve),
        ("d_solve_value", Kind::DSolveValue),
        ("asymptotic_d_solve_value", Kind::AsymptoticValue),
        ("r_solve", Kind::RSolve),
        ("r_solve_value", Kind::RSolveValue),
    ] {
        k.register(h.sym(name), mc_eval::Attributes::default(), mc_calculus::common::wrap(name, DSolveNative { kind, stop: stop.clone() }));
    }
}
