//! The natives: argument checking, the bridge from the evaluator's fuel and interrupt to the kernels' work budget,
//! and the functions that are thin layers over the solver (`roots`, `n_solve`, `eliminate`, `groebner_basis`).
//!
//! An interrupt or fuel exhaustion returns the evaluator's abort; a result beyond the memory limit is
//! `failure(limit, memory)`; a malformed call is `failure(argument, name, message)`; equations no method solves give
//! `failure(unsolved, name, message)`.
use crate::cx::{Env, Out, Tick};
use crate::numroots::{numeric_roots, Precision};
use crate::rootobj::{numeric_coefficients, precision_of};
use crate::solve::{finish, read_equations, read_unknowns, result_term, solve_eqs, symbols_of};
use mc_api::{names, Store, View, T};
use mc_arith::num::StopScope;
use mc_core::Heap;
use mc_eval::{Native, NativeCx, Reply, Res};
use mc_poly::groebner::{groebner, GPoly, MonomialOrder};
use mc_poly::ratfun;
use mc_poly::work::{Halt, Meter, WorkResult};
use mc_poly::Poly;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// The functions of this crate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SolveFn {
    Solve,
    SolveValues,
    NSolveValues,
    Roots,
    NSolve,
    Eliminate,
    GroebnerBasis,
    Reduce,
    Root,
}

/// A function of this crate as a native.
pub(crate) struct SolveNative {
    pub f: SolveFn,
    pub name: &'static str,
    pub stop: Arc<AtomicBool>,
}

impl Native for SolveNative {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h: &'h Heap = cx.heap();
        // the value forms: the solutions of solve and n_solve without the rules
        if matches!(self.f, SolveFn::SolveValues | SolveFn::NSolveValues) {
            let inner = if self.f == SolveFn::SolveValues { "solve" } else { "n_solve" };
            let Some(&xs) = args.get(1) else {
                return Ok(Reply::Done(Env::new(h, self.stop.clone(), self.name).arg_failure_term("expected equations and unknowns")));
            };
            let r = cx.eval(h.call(h.sym(inner), args))?;
            return Ok(Reply::Done(crate::front::values(h, r, xs)));
        }
        // a power of degree beyond 100000 (x^(10^6) == 1): no method builds such a polynomial; left unevaluated at once
        if matches!(self.f, SolveFn::Solve | SolveFn::Roots | SolveFn::NSolve | SolveFn::Reduce) && args.iter().any(|&a| huge_degree(h, a)) {
            return Ok(Reply::Decline);
        }
        // the options and the settings they give, in force for this call and the calls it makes (decision 11C.1)
        let mut own: Vec<T<'h>>;
        let mut opts = crate::front::Options::default();
        let mut args: &[T<'h>] = args;
        if matches!(self.f, SolveFn::Solve | SolveFn::Roots | SolveFn::NSolve) {
            match crate::front::split(h, self.name, args) {
                Ok((a, o)) => {
                    own = a;
                    opts = o;
                }
                Err(msg) => return Ok(Reply::Done(Env::new(h, self.stop.clone(), self.name).arg_failure_term(&msg))),
            }
            // vector and matrix equations as scalar ones
            if let Some(e) = own.first().and_then(|&e| crate::front::scalar_equations(h, e)) {
                own[0] = e;
            }
            if opts.no_facts {
                own.push(crate::domain::no_facts_option(h));
            }
            args = &own;
        }
        let _settings = crate::front::Scope::enter(&opts.settings);
        if let Some(p) = opts.modulus {
            let env = Env::new(h, self.stop.clone(), self.name);
            let _scope = StopScope::enter(Some(&self.stop));
            let mut tick = Tick { cx, stop: &self.stop, abort: None };
            let r = {
                let mut m = Meter::new(&mut tick);
                crate::modular::solve_mod(&env, self.f, args, p, &mut m)
            };
            return finish_reply(&env, r, tick.abort);
        }
        // a domain or conditions: solve the equations, then keep the solutions in the domain that meet them
        if self.f == SolveFn::Solve {
            // `element(points, region)` as the region's membership condition (as reduce does)
            if let Some((&c, rest)) = args.split_first() {
                if let Some(c2) = crate::reduce::regions(cx, c)? {
                    let mut a = vec![c2];
                    a.extend_from_slice(rest);
                    return Ok(Reply::Again(h.call(h.sym("solve"), &a)));
                }
            }
            if let Some(r) = crate::domain::solve(cx, args)? {
                return Ok(Reply::Done(r));
            }
        }
        // several unknowns, or the integers: through the solutions
        if self.f == SolveFn::Reduce {
            if let Some(r) = crate::reduce::modulus(cx, args)? {
                return Ok(Reply::Done(r));
            }
            if let Some(r) = crate::reduce::several(cx, &self.stop, args)? {
                return Ok(Reply::Again(r));
            }
            if let Some(r) = crate::domain::reduce(cx, args)? {
                return Ok(Reply::Again(r));
            }
        }
        let (args, _) = crate::domain::strip_no_facts(h, args);
        let env = Env::new(h, self.stop.clone(), self.name);
        let _scope = StopScope::enter(Some(&self.stop));
        let mut tick = Tick { cx, stop: &self.stop, abort: None };
        let r = {
            let mut m = Meter::new(&mut tick);
            dispatch(self.f, &env, args, &opts, &mut m)
        };
        let abort = tick.abort.take();
        let _ = &tick;
        if let Ok(Out::Sorted(t)) = r {
            // the values evaluated (`asin(1/2)` is `pi/6`), then ordered
            let v = cx.eval(t)?;
            let Some(items) = cx.as_list(v) else { return Ok(Reply::Done(v)) };
            let mut keyed: Vec<(Vec<T<'h>>, T<'h>)> = vec![];
            for &s in items {
                let vals: Vec<T<'h>> = cx
                    .as_list(s)
                    .unwrap_or(&[])
                    .iter()
                    .map(|&r| match h.view(r) {
                        mc_api::View::App { args: &[_, b], .. } => b,
                        _ => r,
                    })
                    .collect();
                keyed.push((vals, s));
            }
            let o = crate::order::SolutionOrder::new(&env);
            keyed.sort_by(|a, b| o.cmp_values(&a.0, &b.0));
            let sorted: Vec<T<'h>> = keyed.into_iter().map(|k| k.1).collect();
            return Ok(Reply::Done(cx.list(&sorted)));
        }
        if let Ok(Out::Numeric(t)) = r {
            // n_solve: the exact solutions made numbers, in the order of solutions
            let v = cx.eval(t)?;
            let max_roots = crate::front::current().max_roots;
            let v = crate::nsolve::finish(cx, v, env.gen, opts.all_roots, max_roots)?;
            // fewer than max_roots for one equation in one unknown: the others by a numeric search
            if let (Some(k), Some(items), [eq, x]) = (max_roots, cx.as_list(v).map(<[T<'h>]>::to_vec), args) {
                if items.len() < k && matches!(h.view(*x), mc_api::View::Sym(_)) {
                    if let mc_api::View::App { head, args: &[l, rr] } = h.view(*eq) {
                        if head == h.sym_term(h.sym("equal")) {
                            let known: Vec<T<'h>> = items
                                .iter()
                                .filter_map(|&s| match cx.as_list(s) {
                                    Some(&[r1]) => match h.view(r1) {
                                        mc_api::View::App { args: &[_, b], .. } => Some(b),
                                        _ => None,
                                    },
                                    _ => None,
                                })
                                .collect();
                            if known.len() == items.len() {
                                let f = cx.eval(h.call(h.sym(names::ADD), &[l, h.call(h.sym(names::MUL), &[h.int(-1), rr])]))?;
                                let more = crate::nsolve::more_roots(cx, f, *x, &known, k - items.len())?;
                                let mut all = items.clone();
                                for z in more {
                                    let z = cx.eval(z)?;
                                    all.push(cx.list(&[h.call(h.sym(names::RULE), &[*x, z])]));
                                }
                                return Ok(Reply::Done(cx.list(&all)));
                            }
                        }
                    }
                }
            }
            return Ok(Reply::Done(v));
        }
        finish_reply(&env, r, abort)
    }
}

/// The reply for what a function gave: an interrupt is the evaluator's abort, memory exhaustion `failure(limit,
/// memory)`.
fn finish_reply<'h>(env: &Env<'h>, r: WorkResult<Out<'h>>, abort: Option<mc_eval::Abort>) -> Res<Reply<'h>> {
    match r {
        Ok(Out::Done(t)) => Ok(Reply::Done(t)),
        Ok(Out::Again(t)) => Ok(Reply::Again(t)),
        Ok(Out::Decline) => Ok(Reply::Decline),
        Ok(Out::Numeric(t)) | Ok(Out::Sorted(t)) => Ok(Reply::Again(t)),
        Err(Halt::Stopped) => match abort {
            Some(a) => Err(a),
            None => Ok(Reply::Done(env.call(names::FAILURE, &[env.sym(names::failure::INTERRUPTED)]))),
        },
        Err(Halt::Memory) => {
            let tags = [env.sym(names::failure::LIMIT), env.sym(names::failure::MEMORY)];
            Ok(Reply::Done(env.call(names::FAILURE, &tags)))
        }
    }
}

fn dispatch<'h>(f: SolveFn, env: &Env<'h>, args: &[T<'h>], opts: &crate::front::Options<'h>, m: &mut Meter) -> WorkResult<Out<'h>> {
    match f {
        SolveFn::Solve => solve(env, args, m),
        SolveFn::SolveValues | SolveFn::NSolveValues => Ok(Out::Decline),
        SolveFn::Roots => roots(env, args, opts, m),
        SolveFn::NSolve => n_solve(env, args, opts, m),
        SolveFn::Eliminate => eliminate(env, args, m),
        SolveFn::GroebnerBasis => groebner_basis(env, args, m),
        SolveFn::Reduce => crate::reduce::reduce(env, args, m),
        SolveFn::Root => crate::rootobj::root(env, args, m),
    }
}

/// The equations and unknowns of `f(eqs, xs)` or `f(eqs)`: `Err` with the reply for malformed calls or `false`.
/// Equations and unknowns, or the reply.
pub(crate) type Read<'h> = Result<(Vec<T<'h>>, Vec<T<'h>>), Out<'h>>;

pub(crate) fn equations_and_unknowns<'h>(env: &Env<'h>, args: &[T<'h>], m: &mut Meter) -> WorkResult<Read<'h>> {
    let (eqs, xs) = match args {
        [e] => (*e, None),
        [e, x] => (*e, Some(*x)),
        _ => return Ok(Err(env.arg_failure("expected equations and unknowns"))),
    };
    let eqs = match read_equations(env, eqs) {
        Ok(Some(e)) => e,
        Ok(None) => return Ok(Err(Out::Done(env.list(&[])))),
        Err(msg) => return Ok(Err(env.arg_failure(msg))),
    };
    let xs = match xs {
        Some(x) => match read_unknowns(env, x) {
            Ok(xs) => xs,
            Err(msg) => return Ok(Err(env.arg_failure(msg))),
        },
        None => symbols_of(env, &eqs, m)?,
    };
    Ok(Ok((eqs, xs)))
}

/// `solve(eqs, xs)`.
fn solve<'h>(env: &Env<'h>, args: &[T<'h>], m: &mut Meter) -> WorkResult<Out<'h>> {
    let (eqs, xs) = match equations_and_unknowns(env, args, m)? {
        Ok(p) => p,
        Err(out) => return Ok(out),
    };
    match solve_eqs(env, &eqs, &xs, m, 0)? {
        Ok(sols) => {
            let mut sols = finish(env, &eqs, &xs, sols, m)?;
            if let Some(k) = crate::front::current().max_roots {
                sols.truncate(k);
            }
            Ok(Out::Sorted(result_term(env, sols)))
        }
        Err(why) => Ok(env.failure("unsolved", why)),
    }
}

/// The single unknown of `roots(eq, x)` and the polynomial (numerator) of the equation, in its ring.
fn univariate<'h>(
    env: &Env<'h>,
    args: &[T<'h>],
    m: &mut Meter,
) -> WorkResult<Result<(T<'h>, Poly, mc_poly::convert::Ring<'h>), Out<'h>>> {
    let (eqs, xs) = match equations_and_unknowns(env, args, m)? {
        Ok(p) => p,
        Err(out) => return Ok(Err(out)),
    };
    let (&[e], &[x]) = (eqs.as_slice(), xs.as_slice()) else {
        return Ok(Err(env.arg_failure("expected one equation in one unknown")));
    };
    let ring = env.ring(&[e], &[x], m)?;
    if ring.kernels.iter().any(|&k| k != x && env.contains_any(k, &[x])) {
        return Ok(Err(env.arg_failure("the equation is not polynomial in the unknown")));
    }
    match ratfun::read(&env.rconv(), e, &ring, m)? {
        Ok(f) => Ok(Ok((x, f.num, ring))),
        Err(_) => Ok(Err(env.arg_failure("the equation divides by zero"))),
    }
}

/// `roots(p == q, x)`: `x == r1 or x == r2 or ...` with multiplicity (decision 3B.12), in the order of
/// decision 11C.7: a polynomial in `x^g` by the roots of its factors in `y = x^g`; otherwise the factors by
/// increasing multiplicity, those of degree above one before the linear ones; inexact coefficients by real part.
/// `multiplicity: k` repeats every root k times as often, `equated_to: e` writes `e == r` for `x == r`.
fn roots<'h>(env: &Env<'h>, args: &[T<'h>], opts: &crate::front::Options<'h>, m: &mut Meter) -> WorkResult<Out<'h>> {
    let (x, p, ring) = match univariate(env, args, m)? {
        Ok(v) => v,
        Err(out) => return Ok(out),
    };
    let v = ring.var(x).expect("x is in the ring");
    if p.is_zero() {
        return Ok(Out::Done(env.sym(names::TRUE)));
    }
    let low = p.low_degree(v).unwrap_or(0);
    let g = p.terms().fold(0u32, |g, (e, _)| num_gcd(g, e[v] - low));
    let rs: Vec<T<'h>> = if g > 1 {
        match crate::univariate::poly_roots(env, &p, &ring, v, m)? {
            None => return Ok(Out::Done(env.sym(names::TRUE))),
            Some(mut rs) => {
                // rational roots only (x^2 - 1): ascending
                if rs.iter().all(|&r| env.ar.num(r).is_some_and(|n| n.as_rat().is_some())) {
                    let o = crate::order::SolutionOrder::new(env);
                    rs.sort_by(|a, b| o.cmp(*a, *b));
                }
                rs
            }
        }
    } else {
        if p.degree(v).unwrap_or(0) > crate::univariate::MAX_FACTOR_DEGREE {
            return Err(Halt::Memory);
        }
        let fs = mc_poly::factor::factor(&p, false, m)?;
        let fs: Vec<(Poly, u32)> = fs.factors.into_iter().filter(|(f, _)| f.degree(v).unwrap_or(0) > 0).collect();
        // the factors of degree above one first, in the factorisation's order, then the linear ones by their root
        let mut out = vec![];
        let mut linear: Vec<(T<'h>, u32)> = vec![];
        for (f, e) in &fs {
            let rs = crate::univariate::irreducible_roots(env, f, &ring, v, m)?;
            if f.degree(v) == Some(1) {
                linear.push((rs[0], *e));
                continue;
            }
            for _ in 0..*e {
                out.extend_from_slice(&rs);
            }
        }
        let o = crate::order::SolutionOrder::new(env);
        linear.sort_by(|a, b| o.cmp(a.0, b.0));
        for (r, e) in linear {
            for _ in 0..e {
                out.push(r);
            }
        }
        out
    };
    let mut keyed: Vec<(Option<(f64, f64)>, T<'h>)> = rs.into_iter().map(|r| (env.machine(r), r)).collect();
    let inexact = keyed.iter().any(|&(_, r)| env.ar.num(r).is_some_and(|n| !n.is_exact()));
    if inexact && keyed.iter().all(|k| k.0.is_some()) {
        keyed.sort_by(|a, b| {
            let (p, q) = (a.0.expect("a value"), b.0.expect("a value"));
            p.0.total_cmp(&q.0).then(p.1.total_cmp(&q.1))
        });
    }
    let lhs = opts.equated_to.unwrap_or(x);
    let times = opts.multiplicity.unwrap_or(1) as usize;
    let mut eqs: Vec<T<'h>> = vec![];
    for &(_, r) in &keyed {
        for _ in 0..times {
            eqs.push(env.call("equal", &[lhs, r]));
        }
    }
    if times > 1 {
        // each root's copies together
        let mut grouped: Vec<T<'h>> = vec![];
        for &e in &eqs {
            if !grouped.contains(&e) {
                let n = eqs.iter().filter(|&&f| f == e).count();
                grouped.extend(std::iter::repeat_n(e, n));
            }
        }
        eqs = grouped;
    }
    Ok(Out::Again(match eqs.as_slice() {
        [] => env.sym(names::FALSE),
        [one] => *one,
        _ => env.call("or", &eqs),
    }))
}

fn num_gcd(a: u32, b: u32) -> u32 {
    if b == 0 {
        a
    } else {
        num_gcd(b, a % b)
    }
}

/// The digits of a trailing `options(digits -> d)`: (arguments before it, digits).
fn digits_option<'a, 'h>(env: &Env<'h>, args: &'a [T<'h>]) -> Result<(&'a [T<'h>], Option<u64>), &'static str> {
    let Some((&last, rest)) = args.split_last() else { return Ok((args, None)) };
    let Some(rules) = env.args_of(last, "options") else { return Ok((args, None)) };
    let mut digits = None;
    for &r in rules {
        let Some(&[k, v]) = env.args_of(r, names::RULE) else { return Err("malformed options") };
        if !env.is_sym(k, "digits") {
            return Err("n_solve knows only the option digits");
        }
        match env.h().view(v) {
            View::Int(d @ 1..=100_000) => digits = Some(d as u64),
            _ => return Err("digits must be a positive integer"),
        }
    }
    Ok((rest, digits))
}

/// `n_solve(eqs, xs)`: one polynomial equation with numeric coefficients by the numeric root finder, everything
/// else as `n(solve(eqs, xs))`.
fn n_solve<'h>(env: &Env<'h>, args: &[T<'h>], opts: &crate::front::Options<'h>, m: &mut Meter) -> WorkResult<Out<'h>> {
    let (args, legacy) = match digits_option(env, args) {
        Ok(v) => v,
        Err(msg) => return Ok(env.arg_failure(msg)),
    };
    let digits = opts.digits.or(legacy);
    let numeric = |t: T<'h>| match digits {
        Some(d) => env.call("n", &[t, env.int(d as i64)]),
        None => env.call("n", &[t]),
    };
    // a domain: reals keeps the real roots of a polynomial
    let (poly_args, reals) = match args {
        [e, x, d] if env.is_sym(*d, "reals") => (vec![*e, *x], true),
        [e, x, d] if env.is_sym(*d, "complexes") => (vec![*e, *x], false),
        _ => (args.to_vec(), false),
    };
    if let Ok((x, p, ring)) = univariate(env, &poly_args, m)? {
        if ring.var(x).and_then(|v| p.degree(v)).unwrap_or(0) > crate::univariate::MAX_FACTOR_DEGREE {
            return Err(Halt::Memory);
        }
        let body = env.conv().int_poly_term(&p, &ring, m)?;
        if let Some(coefs) = numeric_coefficients(env, x, body, m)? {
            if coefs.len() < 2 {
                return Ok(Out::Numeric(numeric(env.call("solve", args))));
            }
            let prec = match (digits, precision_of(&coefs)) {
                (_, Some(Precision::Machine)) | (None, _) => Precision::Machine,
                (Some(d), Some(Precision::Bits(b))) => {
                    Precision::Bits((mc_arith::text::digits_to_bits(d) + 16).min(b))
                }
                (Some(d), None) => Precision::Bits(mc_arith::text::digits_to_bits(d) + 16),
            };
            if let Some(mut rs) = numeric_roots(&coefs, prec, m)? {
                if reals {
                    rs.retain(|r| matches!(r, mc_arith::Num::Real(_)));
                }
                // by real part, then imaginary part
                let key = |r: &mc_arith::Num| match r {
                    mc_arith::Num::Real(a) => (a.to_f64(), 0.0),
                    mc_arith::Num::Complex(a, b) => (a.to_f64(), b.to_f64()),
                };
                rs.sort_by(|a, b| {
                    let (p, q) = (key(a), key(b));
                    p.0.total_cmp(&q.0).then(p.1.total_cmp(&q.1))
                });
                if let Some(k) = crate::front::current().max_roots {
                    rs.truncate(k);
                }
                let h = env.h();
                let rule = h.sym(names::RULE);
                let items: Vec<T<'h>> =
                    rs.iter().map(|r| env.list(&[h.call(rule, &[x, env.ar.number(r)])])).collect();
                return Ok(Out::Done(env.list(&items)));
            }
        }
    }
    Ok(Out::Numeric(numeric(env.call("solve", args))))
}

/// The polynomials of equations (numerators of the differences) in one ring.
fn numerators<'h>(
    env: &Env<'h>,
    eqs: &[T<'h>],
    extra: &[T<'h>],
    m: &mut Meter,
) -> WorkResult<Result<(Vec<Poly>, mc_poly::convert::Ring<'h>), &'static str>> {
    let ring = env.ring(eqs, extra, m)?;
    let rc = env.rconv();
    let mut out = vec![];
    for &e in eqs {
        match ratfun::read(&rc, e, &ring, m)? {
            Ok(f) => {
                if !f.num.is_zero() {
                    out.push(f.num)
                }
            }
            Err(_) => return Ok(Err("an equation divides by zero")),
        }
    }
    Ok(Ok((out, ring)))
}

/// `eliminate(eqs, xs)`: the equations `p == 0` implied by eqs and free of xs (a lex Groebner basis with xs first,
/// every other kernel a variable after them): their conjunction, `true` when there is none, `false` when eqs are
/// inconsistent.
fn eliminate<'h>(env: &Env<'h>, args: &[T<'h>], m: &mut Meter) -> WorkResult<Out<'h>> {
    let [_, _] = args else { return Ok(env.arg_failure("expected equations and the unknowns to eliminate")) };
    let (eqs, xs) = match equations_and_unknowns(env, args, m)? {
        Ok(p) => p,
        Err(out) => return Ok(if matches!(out, Out::Done(t) if env.as_list(t) == Some(&[])) {
            Out::Done(env.sym(names::FALSE))
        } else {
            out
        }),
    };
    let (nums, ring) = match numerators(env, &eqs, &xs, m)? {
        Ok(v) => v,
        Err(msg) => return Ok(env.arg_failure(msg)),
    };
    let nv = ring.nvars();
    let elim: Vec<usize> = xs.iter().map(|&x| ring.var(x).expect("in the ring")).collect();
    let mut main = elim.clone();
    let mut params = vec![];
    for i in 0..nv {
        if elim.contains(&i) {
            continue;
        }
        if env.ar.num(ring.kernels[i]).is_some() {
            params.push(i);
        } else {
            main.push(i);
        }
    }
    let gs: Vec<GPoly> = nums.iter().map(|p| GPoly::from_poly(p, &main, &params, MonomialOrder::Lex)).collect();
    let basis = groebner(&gs, MonomialOrder::Lex, m)?;
    if basis.len() == 1 && basis[0].is_constant() {
        return Ok(Out::Done(env.sym(names::FALSE)));
    }
    let c = env.conv();
    let mut out = vec![];
    for g in &basis {
        let p = g.to_poly(nv, &main, &params);
        if elim.iter().all(|&v| p.degree(v).unwrap_or(0) == 0) {
            let t = c.int_poly_term(&p, &ring, m)?;
            out.push(env.call("equal", &[t, env.int(0)]));
        }
    }
    Ok(Out::Again(match out.as_slice() {
        [] => env.sym(names::TRUE),
        [one] => *one,
        _ => env.call("and", &out),
    }))
}

/// `groebner_basis(polys, xs)`: the reduced lex basis of the ideal of Q[xs, others] (every other symbolic kernel
/// a variable ranked after xs, in canonical order; `parameter_variables: ps` ranks ps last), elements primitive.
/// So the elements free of xs generate the elimination ideal (Cox, Little and O'Shea, *Ideals, Varieties, and
/// Algorithms*, 3.1, the elimination theorem). `groebner_basis(polys)` takes every kernel as a variable.
fn groebner_basis<'h>(env: &Env<'h>, args: &[T<'h>], m: &mut Meter) -> WorkResult<Out<'h>> {
    let h = env.h();
    // the options at the end: parameter_variables, monomial_order, modulus, coefficient_domain, method
    let mut order_opt: Option<T<'h>> = None;
    let mut modulus: Option<u64> = None;
    let (args, last_vars): (&[T<'h>], Vec<T<'h>>) = match args.split_last() {
        Some((&o, rest)) if matches!(h.view(o), View::App { head, .. } if head == h.sym_term(h.sym("options"))) => {
            let View::App { args: rules, .. } = h.view(o) else { unreachable!() };
            let mut pv = vec![];
            for &r in rules {
                match h.view(r) {
                    View::App { args: &[k, v], .. } if k == env.sym("parameter_variables") => {
                        pv = env.as_list(v).map(|l| l.to_vec()).unwrap_or_else(|| vec![v]);
                    }
                    View::App { args: &[k, v], .. } if k == env.sym("monomial_order") => order_opt = Some(v),
                    View::App { args: &[k, v], .. } if k == env.sym("modulus") => match h.view(v) {
                        View::Int(0) => {}
                        View::Int(p) if p > 1 && mc_poly::modp::is_prime(p as u64) => modulus = Some(p as u64),
                        _ => return Ok(env.arg_failure("the modulus must be a prime (or 0)")),
                    },
                    // the reduced basis does not depend on the method that computes it
                    View::App { args: &[k, _], .. } if k == env.sym("method") => {}
                    View::App { args: &[k, v], .. } if k == env.sym("coefficient_domain") => {
                        if v != env.sym("rationals") {
                            return Ok(env.arg_failure("the coefficient domain is rationals (the others are not supported)"));
                        }
                    }
                    _ => return Ok(env.arg_failure("unknown option")),
                }
            }
            (rest, pv)
        }
        _ => (args, vec![]),
    };
    let (ps, xs, elims) = match args {
        [ps, xs, el] => (*ps, Some(*xs), Some(*el)),
        [ps, xs] => (*ps, Some(*xs), None),
        [ps] => (*ps, None, None),
        _ => return Ok(env.arg_failure("expected polynomials and variables")),
    };
    if elims.is_some() || order_opt.is_some() || modulus.is_some() {
        return groebner_general(env, ps, xs, elims, order_opt, modulus, &last_vars, m);
    }
    let ps = match read_equations(env, ps) {
        Ok(Some(e)) => e,
        Ok(None) => return Ok(Out::Done(env.list(&[env.int(1)]))),
        Err(msg) => return Ok(env.arg_failure(msg)),
    };
    let xs = match xs.map(|x| read_unknowns(env, x)) {
        Some(Ok(xs)) => xs,
        Some(Err(msg)) => return Ok(env.arg_failure(msg)),
        None => vec![],
    };
    let mut extra = xs.clone();
    extra.extend(last_vars.iter().copied());
    let (nums, ring) = match numerators(env, &ps, &extra, m)? {
        Ok(v) => v,
        Err(msg) => return Ok(env.arg_failure(msg)),
    };
    let nv = ring.nvars();
    let given: Vec<usize> = xs.iter().map(|&x| ring.var(x).expect("in the ring")).collect();
    let lowest: Vec<usize> = last_vars.iter().map(|&x| ring.var(x).expect("in the ring")).collect();
    let mut main = given.clone();
    let mut params = vec![];
    for i in 0..nv {
        if given.contains(&i) || lowest.contains(&i) {
            continue;
        }
        if env.ar.num(ring.kernels[i]).is_some() {
            // numeric kernels (sqrt(2), pi) stay in the coefficients
            params.push(i);
        } else {
            main.push(i);
        }
    }
    main.extend(lowest.iter().copied());
    let c = env.conv();
    if main.len() == given.len() {
        // no symbolic parameters: the solver's lex basis (FGLM for zero-dimensional ideals)
        let mut out = vec![];
        for p in &crate::system::lex_basis(&nums, &ring, &given, m)? {
            out.push(c.int_poly_term(p, &ring, m)?);
        }
        return Ok(Out::Done(env.list(&out)));
    }
    let gs: Vec<GPoly> = nums.iter().map(|p| GPoly::from_poly(p, &main, &params, MonomialOrder::Lex)).collect();
    let basis = groebner(&gs, MonomialOrder::Lex, m)?;
    let mut out = vec![];
    for g in &basis {
        let p = g.to_poly(nv, &main, &params);
        out.push(c.int_poly_term(&p, &ring, m)?);
    }
    Ok(Out::Done(env.list(&out)))
}

/// `groebner_basis(polys, xs, elims, monomial_order: o, modulus: p)`: the reduced basis in a general monomial order
/// ([`mc_poly::groebner::MatrixOrder`]) over Q or `F_p`.
///
/// The variables are ranked `elims`, then `xs`, then every other symbolic kernel (canonical order), then
/// `parameter_variables`. The order is lex by default; `degree_lexicographic` and `degree_reverse_lexicographic` on
/// all the variables; `elimination_order` the block order with `elims` in one degree reverse lexicographic block and
/// the rest in another; a weight matrix (a list of rows, one column per variable of `xs`) on `xs` with the other
/// variables in a lex block after them. With `elims` given, only the elements free of them are returned: they
/// generate the elimination ideal, whatever block order puts `elims` first (Cox, Little and O'Shea 3.1). Over `F_p`
/// every element is monic with coefficients in `[0, p)`; numeric kernels (`sqrt(2)`) are then not allowed.
#[allow(clippy::too_many_arguments)]
fn groebner_general<'h>(
    env: &Env<'h>,
    ps: T<'h>,
    xs: Option<T<'h>>,
    elims: Option<T<'h>>,
    order_opt: Option<T<'h>>,
    modulus: Option<u64>,
    last_vars: &[T<'h>],
    m: &mut Meter,
) -> WorkResult<Out<'h>> {
    use mc_poly::groebner::{groebner_in, MatrixOrder, OrderKind};
    let ps = match read_equations(env, ps) {
        Ok(Some(e)) => e,
        Ok(None) => return Ok(Out::Done(env.list(&[env.int(1)]))),
        Err(msg) => return Ok(env.arg_failure(msg)),
    };
    let read = |t: Option<T<'h>>| match t.map(|x| read_unknowns(env, x)) {
        Some(Ok(v)) => Ok(v),
        Some(Err(msg)) => Err(msg),
        None => Ok(vec![]),
    };
    let xs = match read(xs) {
        Ok(v) => v,
        Err(msg) => return Ok(env.arg_failure(msg)),
    };
    let elims = match read(elims) {
        Ok(v) => v,
        Err(msg) => return Ok(env.arg_failure(msg)),
    };
    let mut extra = elims.clone();
    extra.extend(xs.iter().copied());
    extra.extend(last_vars.iter().copied());
    let (nums, ring) = match numerators(env, &ps, &extra, m)? {
        Ok(v) => v,
        Err(msg) => return Ok(env.arg_failure(msg)),
    };
    let nv = ring.nvars();
    let idx = |v: &[T<'h>]| -> Vec<usize> { v.iter().map(|&x| ring.var(x).expect("in the ring")).collect() };
    let (e_idx, x_idx, l_idx) = (idx(&elims), idx(&xs), idx(last_vars));
    let mut main = e_idx.clone();
    for &i in &x_idx {
        if !main.contains(&i) {
            main.push(i);
        }
    }
    let mut params = vec![];
    let mut others = vec![];
    for i in 0..nv {
        if main.contains(&i) || l_idx.contains(&i) {
            continue;
        }
        if env.ar.num(ring.kernels[i]).is_some() {
            params.push(i);
        } else {
            others.push(i);
        }
    }
    main.extend(others.iter().copied());
    main.extend(l_idx.iter().copied().filter(|i| !e_idx.contains(i) && !x_idx.contains(i)));
    if modulus.is_some() && !params.is_empty() {
        return Ok(env.arg_failure("with a modulus the polynomials must have rational coefficients"));
    }
    let n = main.len();
    let ne = e_idx.len();
    let rows = match order_opt {
        None => MatrixOrder::of(OrderKind::Lex, n),
        Some(o) if o == env.sym("lexicographic") => MatrixOrder::of(OrderKind::Lex, n),
        Some(o) if o == env.sym("degree_lexicographic") => MatrixOrder::of(OrderKind::DegLex, n),
        Some(o) if o == env.sym("degree_reverse_lexicographic") => MatrixOrder::of(OrderKind::DegRevLex, n),
        Some(o) if o == env.sym("elimination_order") => {
            if ne == 0 {
                return Ok(env.arg_failure("elimination_order needs the variables to eliminate"));
            }
            let a = MatrixOrder::of(OrderKind::DegRevLex, ne);
            let b = MatrixOrder::of(OrderKind::DegRevLex, n - ne);
            MatrixOrder::block(&a, ne, &b, n - ne)
        }
        Some(o) => {
            // a weight matrix on xs (after elims, before the other variables)
            let Some(rs) = env.as_list(o) else { return Ok(env.arg_failure("unknown monomial order")) };
            let mut w = vec![];
            for &r in rs {
                let Some(r) = env.as_list(r) else { return Ok(env.arg_failure("a weight matrix is a list of integer rows")) };
                let row: Option<Vec<i64>> =
                    r.iter().map(|&x| match env.h().view(x) { View::Int(v) => Some(v), _ => None }).collect();
                let Some(row) = row else { return Ok(env.arg_failure("a weight matrix is a list of integer rows")) };
                w.push(row);
            }
            if ne > 0 || w.first().map_or(0, |r| r.len()) != x_idx.len() || main.len() < x_idx.len() {
                return Ok(env.arg_failure("the weight matrix needs one column per variable"));
            }
            let rest = n - x_idx.len();
            MatrixOrder::block(&w, x_idx.len(), &MatrixOrder::of(OrderKind::Lex, rest), rest)
        }
    };
    let Some(order) = MatrixOrder::new(rows, n, modulus) else {
        return Ok(env.arg_failure("the weight matrix is not a monomial order (full rank, first nonzero entry of each column positive)"));
    };
    let gs: Vec<GPoly> = nums.iter().map(|p| GPoly::from_poly_in(p, &main, &params, &order)).collect();
    let basis = groebner_in(&gs, &order, m)?;
    let c = env.conv();
    let mut out = vec![];
    for g in &basis {
        // the elimination ideal: the elements free of the first ne variables
        if g.terms.iter().any(|(e, _)| e[..ne].iter().any(|&k| k > 0)) {
            continue;
        }
        let p = g.to_poly(nv, &main, &params);
        out.push(c.int_poly_term(&p, &ring, m)?);
    }
    Ok(Out::Done(env.list(&out)))
}

/// Does `t` hold an integer power whose exponent exceeds 100000 in magnitude?
fn huge_degree<'h>(h: &'h Heap, t: T<'h>) -> bool {
    let mut stack = vec![t];
    let mut seen = 0usize;
    while let Some(u) = stack.pop() {
        seen += 1;
        if seen > 50_000 {
            return false;
        }
        if let View::App { head, args } = h.view(u) {
            if args.len() == 2 && matches!(h.view(head), View::Sym(s) if h.sym_name(s) == names::POW) {
                match h.view(args[1]) {
                    View::Int(k) if k.unsigned_abs() > 100_000 => return true,
                    View::BigInt { .. } => return true,
                    _ => {}
                }
            }
            stack.push(head);
            stack.extend(args.iter().copied());
        }
    }
    false
}
