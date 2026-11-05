//! The natives: argument checking, the bridge from the evaluator's fuel and interrupt to the kernels' work budget,
//! and the functions that are thin layers over the solver (`roots`, `n_solve`, `eliminate`, `groebner_basis`).
//!
//! An interrupt or fuel exhaustion returns the evaluator's abort; a result beyond the memory limit is
//! `failure(limit, memory)`; a malformed call is `failure(argument, name, message)`; equations no method solves give
//! `failure(unsolved, name, message)`.
use crate::cx::{Env, Out, Tick};
use crate::numroots::{numeric_roots, Precision};
use crate::rootobj::{numeric_coefficients, precision_of};
use crate::solve::{read_equations, read_unknowns, result_term, solve_eqs, symbols_of};
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
        let env = Env::new(h, self.stop.clone(), self.name);
        let _scope = StopScope::enter(Some(&self.stop));
        let mut tick = Tick { cx, stop: &self.stop, abort: None };
        let r = {
            let mut m = Meter::new(&mut tick);
            dispatch(self.f, &env, args, &mut m)
        };
        match r {
            Ok(Out::Done(t)) => Ok(Reply::Done(t)),
            Ok(Out::Again(t)) => Ok(Reply::Again(t)),
            Ok(Out::Decline) => Ok(Reply::Decline),
            Err(Halt::Stopped) => match tick.abort {
                Some(a) => Err(a),
                None => Ok(Reply::Done(env.call(names::FAILURE, &[env.sym(names::failure::INTERRUPTED)]))),
            },
            Err(Halt::Memory) => {
                let tags = [env.sym(names::failure::LIMIT), env.sym(names::failure::MEMORY)];
                Ok(Reply::Done(env.call(names::FAILURE, &tags)))
            }
        }
    }
}

fn dispatch<'h>(f: SolveFn, env: &Env<'h>, args: &[T<'h>], m: &mut Meter) -> WorkResult<Out<'h>> {
    match f {
        SolveFn::Solve => solve(env, args, m),
        SolveFn::Roots => roots(env, args, m),
        SolveFn::NSolve => n_solve(env, args, m),
        SolveFn::Eliminate => eliminate(env, args, m),
        SolveFn::GroebnerBasis => groebner_basis(env, args, m),
        SolveFn::Reduce => crate::reduce::reduce(env, args, m),
        SolveFn::Root => crate::rootobj::root(env, args, m),
    }
}

/// The equations and unknowns of `f(eqs, xs)` or `f(eqs)`: `Err` with the reply for malformed calls or `false`.
/// Equations and unknowns, or the reply.
type Read<'h> = Result<(Vec<T<'h>>, Vec<T<'h>>), Out<'h>>;

fn equations_and_unknowns<'h>(env: &Env<'h>, args: &[T<'h>], m: &mut Meter) -> WorkResult<Read<'h>> {
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
        Ok(sols) => Ok(Out::Again(result_term(env, sols))),
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

/// `roots(p == q, x)`: `x == r1 or x == r2 or ...` with multiplicity (decision 3B.12).
fn roots<'h>(env: &Env<'h>, args: &[T<'h>], m: &mut Meter) -> WorkResult<Out<'h>> {
    let (x, p, ring) = match univariate(env, args, m)? {
        Ok(v) => v,
        Err(out) => return Ok(out),
    };
    let v = ring.var(x).expect("x is in the ring");
    let rs = match crate::univariate::poly_roots(env, &p, &ring, v, m)? {
        None => return Ok(Out::Done(env.sym(names::TRUE))),
        Some(rs) => rs,
    };
    let mut keyed: Vec<(Option<(f64, f64)>, T<'h>)> = rs.into_iter().map(|r| (env.machine(r), r)).collect();
    if keyed.iter().all(|k| k.0.is_some()) {
        keyed.sort_by(|a, b| {
            let (p, q) = (a.0.expect("a value"), b.0.expect("a value"));
            p.0.total_cmp(&q.0).then(p.1.total_cmp(&q.1))
        });
    }
    let eqs: Vec<T<'h>> = keyed.iter().map(|&(_, r)| env.call("equal", &[x, r])).collect();
    Ok(Out::Again(match eqs.as_slice() {
        [] => env.sym(names::FALSE),
        [one] => *one,
        _ => env.call("or", &eqs),
    }))
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
fn n_solve<'h>(env: &Env<'h>, args: &[T<'h>], m: &mut Meter) -> WorkResult<Out<'h>> {
    let (args, digits) = match digits_option(env, args) {
        Ok(v) => v,
        Err(msg) => return Ok(env.arg_failure(msg)),
    };
    let numeric = |t: T<'h>| match digits {
        Some(d) => env.call("n", &[t, env.int(d as i64)]),
        None => env.call("n", &[t]),
    };
    if let Ok((x, p, ring)) = univariate(env, args, m)? {
        let body = env.conv().int_poly_term(&p, &ring, m)?;
        if let Some(coefs) = numeric_coefficients(env, x, body, m)? {
            if coefs.len() < 2 {
                return Ok(Out::Again(numeric(env.call("solve", args))));
            }
            let prec = match (digits, precision_of(&coefs)) {
                (_, Some(Precision::Machine)) | (None, _) => Precision::Machine,
                (Some(d), Some(Precision::Bits(b))) => {
                    Precision::Bits((mc_arith::text::digits_to_bits(d) + 16).min(b))
                }
                (Some(d), None) => Precision::Bits(mc_arith::text::digits_to_bits(d) + 16),
            };
            if let Some(rs) = numeric_roots(&coefs, prec, m)? {
                let h = env.h();
                let rule = h.sym(names::RULE);
                let items: Vec<T<'h>> =
                    rs.iter().map(|r| env.list(&[h.call(rule, &[x, env.ar.number(r)])])).collect();
                return Ok(Out::Done(env.list(&items)));
            }
        }
    }
    Ok(Out::Again(numeric(env.call("solve", args))))
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

/// `groebner_basis(polys, xs)`: the reduced lex basis over Q(other kernels), elements primitive.
fn groebner_basis<'h>(env: &Env<'h>, args: &[T<'h>], m: &mut Meter) -> WorkResult<Out<'h>> {
    let [ps, xs] = args else { return Ok(env.arg_failure("expected polynomials and variables")) };
    let ps = match read_equations(env, *ps) {
        Ok(Some(e)) => e,
        Ok(None) => return Ok(Out::Done(env.list(&[env.int(1)]))),
        Err(msg) => return Ok(env.arg_failure(msg)),
    };
    let xs = match read_unknowns(env, *xs) {
        Ok(xs) => xs,
        Err(msg) => return Ok(env.arg_failure(msg)),
    };
    let (nums, ring) = match numerators(env, &ps, &xs, m)? {
        Ok(v) => v,
        Err(msg) => return Ok(env.arg_failure(msg)),
    };
    let vars: Vec<usize> = xs.iter().map(|&x| ring.var(x).expect("in the ring")).collect();
    let basis = crate::system::lex_basis(&nums, &ring, &vars, m)?;
    let c = env.conv();
    let mut out = vec![];
    for p in &basis {
        out.push(c.int_poly_term(p, &ring, m)?);
    }
    Ok(Out::Done(env.list(&out)))
}
