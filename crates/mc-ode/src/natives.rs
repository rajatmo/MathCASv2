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
    /// the solution was new to the store (no equal one was kept before)
    fresh: bool,
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
    // `[t, t1]`: from the initial point to t1 (the domain always covers the initial point, decision N3.11)
    let (t, a, b) = match cx.as_list(r) {
        Some(&[t, a, b]) => (t, a, b),
        Some(&[t, b]) => (t, b, b),
        _ => return Ok(None),
    };
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
        if opts.high_precision() && !sys.targets.is_empty() {
            return Ok(Err(bad_argument(h, name, h.string("boundary value problems are solved at machine precision only"))));
        }
        if opts.high_precision() {
            // the ends as given (3/2 stays exact)
            let start = ar.num(sys.start).expect("a number");
            let pick = |v: f64| if v == s { start.clone() } else if v == af { a.clone() } else { b.clone() };
            return self.taylor(cx, ar, sys, opts, (pick(lo), lo), (pick(hi), hi), start);
        }
        let Some(mut y0) = sys.initial.iter().map(real).collect::<Option<Vec<f64>>>() else {
            return Ok(Err(bad_argument(h, name, h.string("the initial values are not real"))));
        };
        let mut model = Model::new(cx, &sys, opts.policy)?;
        if !sys.targets.is_empty() {
            let method = if opts.method == Method::Taylor { Method::Auto } else { opts.method };
            let settings = Settings { method, tol: opts.tolerance(), hmax: opts.hmax(hi - lo), max_steps: opts.step_limit() };
            match shoot(cx, &mut model, &sys, s, &mut y0, &settings)? {
                Ok(()) => {}
                Err(m) => return Ok(Err(failure(h, "no_convergence", &[h.sym_term(h.sym(name)), h.string(&m)]))),
            }
        }
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
            Settings { method, tol: opts.tolerance(), hmax: opts.hmax(hi - lo), max_steps: opts.step_limit() };
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
        let (sol, key, fresh) = self.keep(sol);
        let solved = Solved { sys, key, fresh, sol, events: found };
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

    fn keep(&self, sol: Solution) -> (Arc<Solution>, u64, bool) {
        let (sol, key, fresh) = self.store.insert(sol);
        (sol, key, fresh)
    }

    /// The value of `expr` along a solution, for `n_dsolve_value` and parametric functions: a new solution that the
    /// value does not name (a number, not a closure of t) is forgotten at once, so that a loop of solves does not
    /// fill the store (decision N3.13).
    fn value<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, s: &Solved<'h>, expr: T<'h>) -> Res<T<'h>> {
        let v = self.substitute(cx, s, expr)?;
        if s.fresh && !mentions(cx, v, &[cx.heap().sym("interpolating_function")])? {
            self.store.forget(s.key, &s.sol);
        }
        Ok(v)
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
        let (sol, key, fresh) = self.keep(sol);
        Ok(Ok(Solved { sys, key, fresh, sol, events: vec![] }))
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
            // a bare unknown `y` (or `y'`, `derivative(n)(y)`) is its interpolating function (gap G70)
            if let Some((i, d)) = bare_unknown(h, &ys, derivative, u) {
                let un = &s.sys.unknowns[i];
                return Some(if d < un.order { component(un.at + d) } else { function_term(h, &s.sol, s.key, &printed_name(h, un.sym, d)) });
            }
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
            return Ok(Reply::Done(self.value(cx, &solved, second)?));
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
                View::Sym(s) => p.params.iter().position(|&q| q == Some(s)).map(|i| values[i]),
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
        let sys = System {
            t: p.t,
            unknowns: p.unknowns,
            state: p.state,
            rhs,
            start: p.start,
            initial,
            initial_terms: vec![],
            free: vec![],
            targets: vec![],
        };
        let solved = match solve.integrate(cx, &ar, sys, &opts, p.a, p.b)? {
            Ok(s) => s,
            Err(f) => return Ok(Reply::Done(f)),
        };
        Ok(Reply::Done(solve.value(cx, &solved, subst(expr))?))
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

/// Shooting for a boundary value problem (decision N3.20): the free initial components `sys.free` adjusted by
/// Newton's method (finite-difference Jacobian, step halving) until the state at the other point meets
/// `sys.targets`; `y0` holds the result.
fn shoot<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    model: &mut Model<'h>,
    sys: &System<'h>,
    start: f64,
    y0: &mut [f64],
    settings: &Settings,
) -> Res<Result<(), String>> {
    let m = sys.free.len();
    let targets: Vec<(usize, f64, f64)> = sys
        .targets
        .iter()
        .filter_map(|(k, x, v)| Some((*k, real(x)?, real(v)?)))
        .collect();
    if targets.len() != m {
        return Ok(Err("the boundary conditions are not real".into()));
    }
    let n = sys.n();
    let end = targets.iter().map(|t| t.1).fold(start, |a, b| if (b - start).abs() > (a - start).abs() { b } else { a });
    // the residuals at the free values s
    let residual = |cx: &mut NativeCx<'_, '_, 'h>, model: &mut Model<'h>, s: &[f64]| -> Res<Option<Vec<f64>>> {
        let mut y = y0.to_vec();
        for (j, &k) in sys.free.iter().enumerate() {
            y[k] = s[j];
        }
        let run = driver::integrate(cx, model, None, start, &y, end, settings)?;
        if run.end != End::Done {
            return Ok(None);
        }
        let none = Default::default();
        let pw = if end < start {
            crate::dense::Piecewise::new(n, &run.steps, &none)
        } else {
            crate::dense::Piecewise::new(n, &none, &run.steps)
        };
        let mut r = Vec::with_capacity(m);
        for &(k, x, v) in &targets {
            let Some(val) = (if x == start { Some(y[k]) } else { pw.value_clamped(k, 0, x) }) else { return Ok(None) };
            r.push(val - v);
        }
        Ok(Some(r))
    };
    let mut s: Vec<f64> = sys.free.iter().map(|&k| y0[k]).collect();
    let Some(mut r) = residual(cx, model, &s)? else { return Ok(Err("the first trajectory does not reach the end".into())) };
    let norm = |r: &[f64]| r.iter().map(|x| x * x).sum::<f64>().sqrt();
    for _ in 0..60 {
        cx.tick(1)?;
        if norm(&r) <= 1e-10 * (1.0 + norm(&s)) {
            for (j, &k) in sys.free.iter().enumerate() {
                y0[k] = s[j];
            }
            return Ok(Ok(()));
        }
        // the Jacobian by forward differences
        let mut jac = vec![0.0; m * m];
        for j in 0..m {
            let dh = 1e-7 * (1.0 + s[j].abs());
            let mut sp = s.clone();
            sp[j] += dh;
            let Some(rp) = residual(cx, model, &sp)? else { return Ok(Err("a trajectory does not reach the end".into())) };
            for i in 0..m {
                jac[i * m + j] = (rp[i] - r[i]) / dh;
            }
        }
        let Some(delta) = solve_dense(m, &mut jac, &r) else { return Ok(Err("the shooting Jacobian is singular".into())) };
        // damped Newton step
        let mut lambda = 1.0;
        loop {
            let trial: Vec<f64> = s.iter().zip(&delta).map(|(a, d)| a - lambda * d).collect();
            if let Some(rt) = residual(cx, model, &trial)? {
                if norm(&rt) < norm(&r) || lambda < 1e-3 {
                    s = trial;
                    r = rt;
                    break;
                }
            }
            lambda *= 0.5;
            if lambda < 1e-6 {
                return Ok(Err("the shooting iteration does not converge".into()));
            }
        }
    }
    Ok(Err("the shooting iteration does not converge".into()))
}

/// The solution x of the dense system `a x = b` (m by m, row-major) by Gaussian elimination with partial pivoting.
fn solve_dense(m: usize, a: &mut [f64], b: &[f64]) -> Option<Vec<f64>> {
    let mut x = b.to_vec();
    for c in 0..m {
        let p = (c..m).max_by(|&i, &j| a[i * m + c].abs().total_cmp(&a[j * m + c].abs()))?;
        if a[p * m + c].abs() < 1e-300 {
            return None;
        }
        if p != c {
            for k in 0..m {
                a.swap(c * m + k, p * m + k);
            }
            x.swap(c, p);
        }
        for i in c + 1..m {
            let f = a[i * m + c] / a[c * m + c];
            for k in c..m {
                a[i * m + k] -= f * a[c * m + k];
            }
            x[i] -= f * x[c];
        }
    }
    for c in (0..m).rev() {
        let mut v = x[c];
        for k in c + 1..m {
            v -= a[c * m + k] * x[k];
        }
        x[c] = v / a[c * m + c];
    }
    Some(x)
}

/// A bare unknown `y` (index i, order 0) or `derivative(n)(y)` (order n) among `ys`.
fn bare_unknown<'h>(h: &'h Heap, ys: &[Sym], derivative: T<'h>, u: T<'h>) -> Option<(usize, usize)> {
    match h.view(u) {
        View::Sym(s) => ys.iter().position(|&y| y == s).map(|i| (i, 0)),
        View::App { head, args: &[y] } => {
            let View::App { head: dh, args: &[n] } = h.view(head) else { return None };
            if dh != derivative {
                return None;
            }
            let (View::Sym(s), View::Int(n @ 0..=1000)) = (h.view(y), h.view(n)) else { return None };
            ys.iter().position(|&q| q == s).map(|i| (i, n as usize))
        }
        _ => None,
    }
}

/// `domain(f)`: the interval `[lo, hi]` of an interpolating function (or of `t => f(t)`, the closure
/// `n_dsolve_value` gives), where it has values (gap G70).
pub(crate) struct DomainOf;

impl Native for DomainOf {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let [f] = args else { return Ok(Reply::Decline) };
        let interp = h.sym_term(h.sym("interpolating_function"));
        let mut u = *f;
        // a closure t => g(t): its function
        if let View::App { head, args: &[_, body] } = h.view(u) {
            if head == h.sym_term(h.sym("fn")) {
                if let View::App { head: g, .. } = h.view(body) {
                    u = g;
                }
            }
        }
        match h.view(u) {
            View::App { head, args } if head == interp && args.len() >= 2 => Ok(Reply::Done(args[1])),
            _ => Ok(Reply::Decline),
        }
    }

    fn reads_term(&self) -> bool {
        false
    }
}
