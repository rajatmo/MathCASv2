//! The natives `n_dsolve`, `n_dsolve_value` and `interpolating_function` (the evaluation of a solution at a
//! time): argument forms, the two precision tiers, results and failures.
//!
//! | call | result |
//! |---|---|
//! | `n_dsolve(eqs, y, [t, t0, t1])`, `n_dsolve(eqs, [y, z], ...)` | `[y -> interpolating_function(...), ...]` |
//! | `n_dsolve_value(eqs, y(t), [t, t0, t1])` | the closure `fn([t], ...)` of the expression |
//! | `n_dsolve_value(eqs, y(t1), ...)`, `[x(t1), y(t1)]` | the number(s) |
//! | `f(t)` for `f = interpolating_function(...)` | the value, `failure(out_of_range, t, [lo, hi])` outside |
//!
//! The domain covers the range and the point of the initial conditions: the integration runs from that point to
//! both ends (decision N3.11).
use crate::driver::{self, End, Settings};
use crate::events::Events;
use crate::model::{self, Model};
use crate::options::{self, Method, OptionError, Options};
use crate::solution::{self, Data, Solution, Store, Value};
use crate::parametric;
use crate::system::{self, printed_name, System};
use crate::taylor;
use crate::util::{bad_argument, failure, mentions, number, real, rules};
use mc_api::names;
use mc_arith::num::StopScope;
use mc_arith::text::digits_to_bits;
use mc_arith::{Arith, Num};
use mc_core::{Heap, Store as _, Sym, View, T};
use mc_eval::{Native, NativeCx, Reply, Res};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// What a [`Solve`] native gives.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Kind {
    /// `n_dsolve`: the rules of the unknowns
    Rules,
    /// `n_dsolve_value`: the value of an expression
    Value,
    /// `parametric_n_dsolve_value`: a `parametric_function` of the parameters, reduced once
    Parametric,
}

/// `n_dsolve(...)`, `n_dsolve_value(...)` and `parametric_n_dsolve_value(...)`.
pub(crate) struct Solve {
    /// the flag `mc-arith` polls in long numeric loops
    pub stop: Arc<AtomicBool>,
    /// the session's solutions
    pub store: Arc<Store>,
    /// what the call gives
    pub kind: Kind,
}

/// `parametric_function(params, expr, data)(v1, ..., vm)`: the value of `expr` for the parameters' values.
pub(crate) struct ParametricCall {
    /// the solver, with kind `Parametric` (its name in failures)
    pub solve: Solve,
}

/// `interpolating_function(...)(t)`.
pub(crate) struct Interpolating {
    /// the flag `mc-arith` polls in long numeric loops
    pub stop: Arc<AtomicBool>,
    /// the session's solutions
    pub store: Arc<Store>,
}

/// The extra digits of the working precision of the Taylor method, and of the tolerances of its fine and
/// coarse integrations, over the asked digits (decision N3.10).
const TAYLOR_GUARD: u64 = 20;
const FINE_DIGITS: u64 = 10;
const COARSE_DIGITS: u64 = 4;

/// The working precision of the Taylor method in bits.
fn taylor_bits(opts: &Options<'_>) -> u64 {
    digits_to_bits(opts.taylor_digits() + TAYLOR_GUARD)
}

/// A solved system: the solution's key and data, and the events found.
struct Solved<'h> {
    sys: System<'h>,
    key: u64,
    sol: Arc<Solution>,
    events: Vec<(f64, usize)>,
}

/// The range `[t, a, b]`: the variable and the ends as numbers.
fn range<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    ar: &Arith<'h, Heap>,
    r: T<'h>,
    digits: Option<u64>,
) -> Res<Option<(Sym, Num, Num)>> {
    let Some(&[t, a, b]) = cx.as_list(r) else { return Ok(None) };
    let View::Sym(t) = cx.heap().view(t) else { return Ok(None) };
    let (Some(a), Some(b)) = (number(cx, ar, a, digits)?, number(cx, ar, b, digits)?) else { return Ok(None) };
    if real(&a).is_none() || real(&b).is_none() {
        return Ok(None);
    }
    Ok(Some((t, a, b)))
}

/// `interpolating_function("y'", [lo, hi], steps, "key")`.
fn function_term<'h>(h: &'h Heap, sol: &Solution, key: u64, name: &str) -> T<'h> {
    let dom = h.call(h.sym(names::LIST), &[h.real(sol.lo), h.real(sol.hi)]);
    h.call(h.sym("interpolating_function"), &[h.string(name), dom, h.int(sol.steps as i64), h.string(&format!("{key:016x}"))])
}

/// The point `[t -> tp, y -> v, ...]` of a failure (the unknowns' values; their derivatives are left out).
fn point<'h>(h: &'h Heap, sys: &System<'h>, tp: f64, y: &[f64]) -> T<'h> {
    let mut pairs = vec![(h.sym_term(sys.t), h.real(tp))];
    for u in &sys.unknowns {
        if let Some(&v) = y.get(u.at) {
            pairs.push((h.sym_term(u.sym), h.real(v)));
        }
    }
    rules(h, &pairs)
}

/// The unknowns' rules `[y -> interpolating_function(...), ...]` of a solution.
fn solution_rules<'h>(h: &'h Heap, s: &Solved<'h>) -> T<'h> {
    let pairs: Vec<(T<'h>, T<'h>)> =
        s.sys.unknowns.iter().map(|u| (h.sym_term(u.sym), function_term(h, &s.sol, s.key, &h.sym_name(u.sym)))).collect();
    rules(h, &pairs)
}

impl Solve {
    fn name(&self) -> &'static str {
        match self.kind {
            Kind::Rules => "n_dsolve",
            Kind::Value => "n_dsolve_value",
            Kind::Parametric => "parametric_n_dsolve_value",
        }
    }

    /// The range and the reduced system (`params` are the parameters of `parametric_n_dsolve_value`); `Err` with
    /// the failure term to return.
    #[allow(clippy::too_many_arguments)]
    fn reduce<'h>(
        &self,
        cx: &mut NativeCx<'_, '_, 'h>,
        ar: &Arith<'h, Heap>,
        eqs: T<'h>,
        ys: &[Sym],
        range_term: T<'h>,
        opts: &Options<'h>,
        params: &[Sym],
    ) -> Res<Result<(System<'h>, Num, Num), T<'h>>> {
        let h = cx.heap();
        let name = self.name();
        // under digits: closed initial values and ends at the working precision of the Taylor method
        let digits = opts.high_precision().then(|| taylor::digits_of(taylor_bits(opts)));
        let Some((t, a, b)) = range(cx, ar, range_term, digits)? else { return Ok(Err(bad_argument(h, name, h.int(3)))) };
        Ok(match system::reduce(cx, ar, eqs, ys, t, digits, params)? {
            Ok(s) => Ok((s, a, b)),
            Err(m) => Err(bad_argument(h, name, h.string(&m))),
        })
    }

    /// Integrate a reduced system over the range [a, b] (and the initial point); `Err` with the failure term to
    /// return.
    fn integrate<'h>(
        &self,
        cx: &mut NativeCx<'_, '_, 'h>,
        ar: &Arith<'h, Heap>,
        sys: System<'h>,
        opts: &Options<'h>,
        a: Num,
        b: Num,
    ) -> Res<Result<Solved<'h>, T<'h>>> {
        let h = cx.heap();
        let name = self.name();
        let Some(s) = ar.num(sys.start).and_then(|v| real(&v)) else {
            return Ok(Err(bad_argument(h, name, h.string("the initial point is not real"))));
        };
        let (af, bf) = (real(&a).expect("checked"), real(&b).expect("checked"));
        let (lo, hi) = (af.min(bf).min(s), af.max(bf).max(s));
        if opts.high_precision() {
            // the ends as given (3/2 stays exact)
            let start = ar.num(sys.start).expect("a number");
            let pick = |v: f64| if v == s { start.clone() } else if v == af { a.clone() } else { b.clone() };
            return self.taylor(cx, ar, sys, opts, (pick(lo), lo), (pick(hi), hi), start);
        }
        let Some(y0) = sys.initial.iter().map(real).collect::<Option<Vec<f64>>>() else {
            return Ok(Err(bad_argument(h, name, h.string("the initial values are not real"))));
        };
        let mut model = Model::new(cx, &sys, opts.policy)?;
        let mut events = match opts.events {
            None => None,
            Some(e) => match self.events(cx, ar, &sys, e, opts)? {
                Ok(ev) => Some(ev),
                Err(f) => return Ok(Err(f)),
            },
        };
        let method = match opts.method {
            Method::Taylor => unreachable!("the Taylor method is high precision"),
            m => m,
        };
        let settings =
            Settings { method, tol: opts.tolerance(), hmax: opts.max_step_size, max_steps: opts.step_limit() };
        let fwd = driver::integrate(cx, &mut model, events.as_mut(), s, &y0, hi, &settings)?;
        let bwd = driver::integrate(cx, &mut model, events.as_mut(), s, &y0, lo, &settings)?;
        for run in [&fwd, &bwd] {
            if let End::NotNumeric(tp, yp) = &run.end {
                return Ok(Err(failure(h, "not_numeric", &[h.sym_term(h.sym(name)), point(h, &sys, *tp, yp)])));
            }
        }
        let n = sys.n();
        let unknowns = sys
            .unknowns
            .iter()
            .map(|u| solution::Unknown { name: h.sym_name(u.sym), order: u.order, at: u.at })
            .collect();
        let steps = fwd.steps.len() + bwd.steps.len();
        let sol = Solution {
            unknowns,
            n,
            lo: bwd.t.min(s),
            hi: fwd.t.max(s),
            steps,
            data: Data::Machine(crate::dense::Piecewise::new(n, &bwd.steps, &fwd.steps)),
        };
        let mut found = events.map(|e| e.found).unwrap_or_default();
        found.sort_by(|p, q| p.0.total_cmp(&q.0).then(p.1.cmp(&q.1)));
        let (sol, key) = self.keep(sol);
        let solved = Solved { sys, key, sol, events: found };
        // a blow-up or the step limit: the part computed is the solution (its domain ends where the run stopped),
        // with a warning (semantics 10: a value with a warning is still the value)
        for run in [&fwd, &bwd] {
            if matches!(run.end, End::MaxSteps | End::StepSize) {
                let why = if run.end == End::MaxSteps { "the step limit was reached" } else { "the step size vanished (a singularity?)" };
                let msg = format!("{why} at t = {}; the solution covers the part computed", run.t);
                // a warning (the engine's `message`): scripts print it as `name: text`, notebooks as a message
                cx.eval(h.call(h.sym("message"), &[h.sym_term(h.sym(name)), h.string(&msg)]))?;
                break;
            }
        }
        Ok(Ok(solved))
    }

    fn keep(&self, sol: Solution) -> (Arc<Solution>, u64) {
        let key = self.store.insert(sol);
        (self.store.get(key).expect("just inserted"), key)
    }

    /// The event functions of `events: [g == 0, ...]` (an expression g stands for g == 0).
    fn events<'h>(
        &self,
        cx: &mut NativeCx<'_, '_, 'h>,
        ar: &Arith<'h, Heap>,
        sys: &System<'h>,
        e: T<'h>,
        opts: &Options<'h>,
    ) -> Res<Result<Events<'h>, T<'h>>> {
        let h = cx.heap();
        let bad = |m: &str| Ok(Err(bad_argument(h, self.name(), h.string(m))));
        let items: Vec<T<'h>> = match cx.as_list(e) {
            Some(items) => items.to_vec(),
            None => vec![e],
        };
        if items.is_empty() {
            return bad("no events");
        }
        let equal = h.sym_term(h.sym("equal"));
        let mut gs = Vec::with_capacity(items.len());
        for g in items {
            let g = match h.view(g) {
                View::App { head, args: &[l, r] } if head == equal => ar.sub(l, r),
                _ => g,
            };
            match sys.in_state(cx, g)? {
                Ok(v) => gs.push(v),
                Err(m) => return bad(&m),
            }
        }
        let m = gs.len();
        let body = cx.list(&gs);
        let g = model::function(cx, body, &model::vars(sys), m, opts.policy)?;
        Ok(Ok(Events::new(g, m, sys.n(), opts.event_action)))
    }

    /// The Taylor method under `digits:` (decision NUM.4): a fine and a coarse integration from the initial
    /// point to both ends.
    #[allow(clippy::too_many_arguments)]
    fn taylor<'h>(
        &self,
        cx: &mut NativeCx<'_, '_, 'h>,
        ar: &Arith<'h, Heap>,
        sys: System<'h>,
        opts: &Options<'h>,
        lo: (Num, f64),
        hi: (Num, f64),
        start: Num,
    ) -> Res<Result<Solved<'h>, T<'h>>> {
        let h = cx.heap();
        let name = self.name();
        if opts.events.is_some() {
            return Ok(Err(bad_argument(h, name, h.string("events need machine precision"))));
        }
        let d = opts.taylor_digits();
        let p = taylor_bits(opts);
        let tape = match taylor::tape(cx, ar, sys.rhs, sys.t, &sys.state, p)? {
            Ok(t) => t,
            Err(m) => return Ok(Err(bad_argument(h, name, h.string(&m)))),
        };
        let ball = |v: &Num| taylor::ball(v, p);
        let (Some(y0), Some(t0), Some(a), Some(b)) =
            (sys.initial.iter().map(ball).collect::<Option<Vec<_>>>(), ball(&start), ball(&lo.0), ball(&hi.0))
        else {
            return Ok(Err(bad_argument(h, name, h.string("the initial values are not real"))));
        };
        let limit = opts.step_limit();
        let mut runs = vec![];
        for digits in [d + FINE_DIGITS, d + COARSE_DIGITS] {
            let mut segs = vec![];
            let mut reached = (lo.1, hi.1);
            for (end, forward) in [(&a, false), (&b, true)] {
                let run = taylor::integrate(cx, &tape, &t0, &y0, end, digits as f64, limit)?;
                match run.end {
                    taylor::End::Done => {}
                    taylor::End::NotNumeric(tp) => {
                        let at = rules(h, &[(h.sym_term(sys.t), h.real(tp))]);
                        return Ok(Err(failure(h, "not_numeric", &[h.sym_term(h.sym(name)), at])));
                    }
                    taylor::End::MaxSteps | taylor::End::StepSize => {
                        return Ok(Err(failure(h, "not_converged", &[h.sym_term(h.sym(name)), cx.null(), h.real(run.t)])));
                    }
                }
                if forward {
                    reached.1 = run.t;
                    segs.extend(run.segments);
                } else {
                    reached.0 = run.t;
                    segs.extend(run.segments.into_iter().rev());
                }
            }
            runs.push((segs, reached));
        }
        let (coarse, _) = runs.pop().expect("two runs");
        let (fine, (rlo, rhi)) = runs.pop().expect("two runs");
        let n = sys.n();
        let unknowns =
            sys.unknowns.iter().map(|u| solution::Unknown { name: h.sym_name(u.sym), order: u.order, at: u.at }).collect();
        let steps = fine.len();
        let out = digits_to_bits(d + 1);
        let sol = Solution {
            unknowns,
            n,
            lo: rlo.min(lo.1),
            hi: rhi.max(hi.1),
            steps,
            data: Data::Taylor(solution::Taylor { p, out, fine, coarse }),
        };
        let (sol, key) = self.keep(sol);
        Ok(Ok(Solved { sys, key, sol, events: vec![] }))
    }

    /// `expr` with the unknowns replaced by the solution: `y^(d)(x)` by the dense output of the component (or its
    /// derivative), the highest derivative by the right-hand side at the solution.
    fn substitute<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, s: &Solved<'h>, expr: T<'h>) -> Res<T<'h>> {
        let h = cx.heap();
        let ys: Vec<Sym> = s.sys.unknowns.iter().map(|u| u.sym).collect();
        let derivative = h.sym_term(h.sym("derivative"));
        let rhs = cx.as_list(s.sys.rhs).expect("the right-hand side is a list").to_vec();
        let component = |j: usize| {
            let (y, d) = s.sys.component(j);
            function_term(h, &s.sol, s.key, &printed_name(h, y, d))
        };
        let tt = h.sym_term(s.sys.t);
        let out = mc_numfn::replace(h, expr, &mut |u| {
            let (i, d, x) = system::occurrence(h, &ys, derivative, u)?;
            let un = &s.sys.unknowns[i];
            if d == un.order {
                // the right-hand side at the dense output: as accurate as the solution itself
                let body = rhs[un.at + un.order - 1];
                return Some(mc_numfn::replace(h, body, &mut |v| match h.view(v) {
                    View::Sym(q) if q == s.sys.t => Some(x),
                    View::Sym(q) => s.sys.state.iter().position(|&st| st == q).map(|j| h.app(component(j), &[x])),
                    _ => None,
                }));
            }
            let f = if d < un.order { component(un.at + d) } else { function_term(h, &s.sol, s.key, &printed_name(h, un.sym, d)) };
            Some(h.app(f, &[x]))
        });
        let v = cx.eval(out)?;
        if mentions(cx, v, &[s.sys.t])? {
            let params = cx.list(&[tt]);
            return Ok(h.call(h.sym("fn"), &[params, v]));
        }
        Ok(v)
    }
}

impl Native for Solve {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, all: &[T<'h>]) -> Res<Reply<'h>> {
        let name = self.name();
        let h = cx.heap();
        let ar = Arith::new(h).with_stop(self.stop.clone());
        let (args, opts) = match options::split(h, &ar, all) {
            Ok(v) => v,
            Err(OptionError::Unknown(s)) => return Ok(Reply::Done(failure(h, "unknown_option", &[h.sym_term(s)]))),
            Err(OptionError::Bad(s)) => return Ok(Reply::Done(bad_argument(h, name, h.sym_term(s)))),
        };
        let (eqs, second, range_term, params) = match (self.kind, args) {
            (Kind::Parametric, &[eqs, expr, r, ps]) => (eqs, expr, r, Some(ps)),
            (Kind::Rules | Kind::Value, &[eqs, second, r]) => (eqs, second, r, None),
            _ => return Ok(Reply::Decline),
        };
        // the parameters: distinct symbols
        let ps: Vec<Sym> = match params {
            None => vec![],
            Some(p) => match system::unknowns(cx, p) {
                Some((ps, true)) if cx.as_list(p).is_some_and(|items| items.iter().all(|&u| matches!(h.view(u), View::Sym(_)))) => ps,
                _ => return Ok(Reply::Done(bad_argument(h, name, h.int(4)))),
            },
        };
        let _scope = StopScope::enter(Some(&self.stop));
        let ys = if self.kind == Kind::Rules {
            match system::unknowns(cx, second) {
                Some((ys, _)) => ys,
                None => return Ok(Reply::Done(bad_argument(h, name, h.int(2)))),
            }
        } else {
            let ys = system::inferred_unknowns(cx, eqs)?;
            if ys.is_empty() {
                return Ok(Reply::Done(bad_argument(h, name, h.string("no derivative of an unknown"))));
            }
            ys
        };
        if ps.iter().any(|p| ys.contains(p)) {
            return Ok(Reply::Done(bad_argument(h, name, h.string("a parameter is also an unknown"))));
        }
        let (sys, a, b) = match self.reduce(cx, &ar, eqs, &ys, range_term, &opts, &ps)? {
            Ok(v) => v,
            Err(f) => return Ok(Reply::Done(f)),
        };
        if let Some(p) = params {
            let options = if args.len() < all.len() { all[all.len() - 1] } else { cx.null() };
            return Ok(Reply::Done(parametric::term(h, &ar, &sys, p, second, (&a, &b), options)));
        }
        let solved = match self.integrate(cx, &ar, sys, &opts, a, b)? {
            Ok(s) => s,
            Err(f) => return Ok(Reply::Done(f)),
        };
        if self.kind == Kind::Value {
            return Ok(Reply::Done(self.substitute(cx, &solved, second)?));
        }
        let mut out = solution_rules(h, &solved);
        if opts.events.is_some() {
            let list = |items: &[T<'h>]| h.call(h.sym(names::LIST), items);
            let found: Vec<T<'h>> = solved.events.iter().map(|&(t, k)| list(&[h.real(t), h.int(k as i64 + 1)])).collect();
            let rule = h.call(h.sym(names::RULE), &[h.sym_term(h.sym("events")), list(&found)]);
            let View::App { args: rs, .. } = h.view(out) else { unreachable!("a list") };
            let mut all = rs.to_vec();
            all.push(rule);
            out = list(&all);
        }
        Ok(Reply::Done(out))
    }
}

impl Native for ParametricCall {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let solve = &self.solve;
        let name = solve.name();
        // only the curried call f(v1, ..., vm) of f = parametric_function(params, expr, data)
        let View::App { head, .. } = h.view(t) else { return Ok(Reply::Decline) };
        let View::App { args: &[params, expr, data], .. } = h.view(head) else { return Ok(Reply::Decline) };
        let ar = Arith::new(h).with_stop(solve.stop.clone());
        let Some(p) = parametric::Parts::read(cx, params, data) else {
            return Ok(Reply::Done(bad_argument(h, "parametric_function", h.string("not a parametric function"))));
        };
        if args.len() != p.params.len() {
            return Ok(Reply::Done(bad_argument(h, name, h.string(&format!("{} parameter values expected", p.params.len())))));
        }
        // numeric values only (a symbolic call stays as it is)
        let mut values = Vec::with_capacity(args.len());
        for &v in args {
            match ar.num(v) {
                Some(n) if n.as_real().is_some() => values.push(v),
                Some(_) => return Ok(Reply::Done(bad_argument(h, name, h.string("the parameter values must be real")))),
                // a closed numeric value (pi/2) through n; a symbolic one leaves the call as it is
                None => match number(cx, &ar, v, None)? {
                    Some(n) if n.as_real().is_some() => values.push(ar.number(&n)),
                    _ => return Ok(Reply::Decline),
                },
            }
        }
        let _scope = StopScope::enter(Some(&solve.stop));
        let subst = |u: T<'h>| {
            mc_numfn::replace(h, u, &mut |x| match h.view(x) {
                View::Sym(s) => p.params.iter().position(|&q| q == s).map(|i| values[i]),
                _ => None,
            })
        };
        let opts = match p.options {
            None => options::split(h, &ar, &[]).ok().expect("no options").1,
            Some(o) => {
                let o = subst(o);
                match options::split(h, &ar, &[o]) {
                    Ok((_, opts)) => opts,
                    Err(OptionError::Unknown(s)) => return Ok(Reply::Done(failure(h, "unknown_option", &[h.sym_term(s)]))),
                    Err(OptionError::Bad(s)) => return Ok(Reply::Done(bad_argument(h, name, h.sym_term(s)))),
                }
            }
        };
        let digits = opts.high_precision().then(|| taylor::digits_of(taylor_bits(&opts)));
        let rhs = cx.eval(subst(p.rhs))?;
        let mut initial = Vec::with_capacity(p.initial.len());
        for &v in &p.initial {
            let v = if ar.num(v).is_some() { v } else { subst(v) };
            match number(cx, &ar, v, digits)? {
                Some(n) => initial.push(n),
                None => return Ok(Reply::Done(bad_argument(h, name, h.string("the initial values are not numeric")))),
            }
        }
        let sys = System { t: p.t, unknowns: p.unknowns, state: p.state, rhs, start: p.start, initial, initial_terms: vec![] };
        let solved = match solve.integrate(cx, &ar, sys, &opts, p.a, p.b)? {
            Ok(s) => s,
            Err(f) => return Ok(Reply::Done(f)),
        };
        Ok(Reply::Done(solve.substitute(cx, &solved, subst(expr))?))
    }

    fn curried(&self) -> bool {
        true
    }
}

impl Interpolating {
    /// The value of `f` at one argument; `None` to leave the call as it is (a symbolic argument).
    fn at<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, ar: &Arith<'h, Heap>, sol: &Solution, i: usize, order: usize, x: T<'h>) -> Res<Option<T<'h>>> {
        let h = cx.heap();
        if let Some(items) = cx.as_list(x) {
            let mut out = Vec::with_capacity(items.len());
            for &v in items {
                cx.tick(1)?;
                match self.at(cx, ar, sol, i, order, v)? {
                    Some(r) => out.push(r),
                    None => return Ok(None),
                }
            }
            return Ok(Some(cx.list(&out)));
        }
        // a closed numeric time (`pi/2`, `sqrt(2)`) through `n`: at machine precision, or at the working
        // precision of a ball solution (decision C.7); a symbolic time leaves the call as it is
        let digits = match &sol.data {
            Data::Machine(_) => None,
            Data::Taylor(tl) => Some(taylor::digits_of(tl.p)),
        };
        let Some(v) = number(cx, ar, x, digits)? else { return Ok(None) };
        Ok(Some(match sol.value(i, order, &v) {
            Some(Value::Machine(r)) => h.real(r),
            Some(Value::Number(r)) => ar.number(&r),
            None => {
                let dom = cx.list(&[h.real(sol.lo), h.real(sol.hi)]);
                failure(h, "out_of_range", &[x, dom])
            }
        }))
    }
}

impl Native for Interpolating {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        // only the curried call f(x) of f = interpolating_function(name, domain, steps, key)
        let View::App { head, .. } = h.view(t) else { return Ok(Reply::Decline) };
        let View::App { args: &[name, _, _, key], .. } = h.view(head) else { return Ok(Reply::Decline) };
        let &[x] = args else { return Ok(Reply::Decline) };
        let (View::Str(name), View::Str(key)) = (h.view(name), h.view(key)) else { return Ok(Reply::Decline) };
        let Some(sol) = u64::from_str_radix(key, 16).ok().and_then(|k| self.store.get(k)) else {
            return Ok(Reply::Done(bad_argument(h, "interpolating_function", h.string("no such solution in this session"))));
        };
        let Some((i, order)) = sol.component(name) else {
            return Ok(Reply::Done(bad_argument(h, "interpolating_function", h.int(1))));
        };
        let _scope = StopScope::enter(Some(&self.stop));
        let ar = Arith::new(h).with_stop(self.stop.clone());
        Ok(match self.at(cx, &ar, &sol, i, order, x)? {
            Some(v) => Reply::Done(v),
            None => Reply::Decline,
        })
    }

    fn curried(&self) -> bool {
        true
    }
}
