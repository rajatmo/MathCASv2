//! The native of `d`: the variable specifications, gradients over lists, and the hand-back to the evaluator.
//!
//! `d(f, s1, s2, ...)` applies the specifications from left to right: a variable `x` differentiates once, `[x, n]`
//! (n a non-negative integer) n times, and a list of variables `[x, y, ...]` makes a gradient: every scalar part of
//! `f` (a list maps elementwise) becomes the list of its derivatives, so the gradient of a list of functions is its
//! Jacobian (decision 2A.2). The result is handed back with [`Reply::Again`], so the calls it contains (`cos(0)`,
//! `log(e)`, ...) are evaluated (decision 2A.1).
use crate::diff::{nth, Halt};
use crate::term::Build;
use mc_api::{names, Store, View, T};
use mc_arith::num::StopScope;
use mc_core::Heap;
use mc_eval::{Native, NativeCx, Reply, Res};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// One specification of `d`.
enum Spec<'h> {
    /// differentiate `n` times in the variable
    Var(T<'h>, i64),
    /// the gradient in these variables, `n` times (`[[x, y], n]`)
    Grad(Vec<T<'h>>, i64),
}

/// `d(f, specs...)`.
pub struct D {
    /// the flag `mc-arith` polls in long numeric loops
    pub stop: Arc<AtomicBool>,
}

/// `failure(argument, d, message)` (the style of the other library packages).
fn arg_failure<'h>(h: &'h Heap, message: &str) -> T<'h> {
    let tags = [h.sym_term(h.sym("argument")), h.sym_term(h.sym("d")), h.string(message)];
    h.call(h.sym(names::FAILURE), &tags)
}

impl Native for D {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        // fewer than two arguments is no derivative: the call stays as it is, so `d` remains usable as a plain
        // symbol in programs that do not differentiate (decision 2A.6)
        let [f, specs @ ..] = args else { return Ok(Reply::Decline) };
        if specs.is_empty() {
            return Ok(Reply::Decline);
        }
        let _scope = StopScope::enter(Some(&self.stop));
        let b = Build::new(h, &self.stop);
        let mut parsed = Vec::with_capacity(specs.len());
        for &s in specs {
            match spec(&b, s) {
                Ok(p) => parsed.push(p),
                Err(message) => return Ok(Reply::Done(arg_failure(h, message))),
            }
        }
        // a truncated series: term by term (decision IOSYM.1)
        if mc_poly::series_data_parts(h, *f).is_some() {
            let mut t = *f;
            for s in &parsed {
                let Spec::Var(x, n) = s else { return Ok(Reply::Decline) };
                for _ in 0..*n {
                    match mc_poly::series_derivative(cx, t, *x)? {
                        Some(r) => t = r,
                        None => return Ok(Reply::Decline),
                    }
                }
            }
            return Ok(Reply::Done(t));
        }
        let mut tick = |n: u64| cx.tick(n);
        let mut t = *f;
        for s in &parsed {
            let r = match s {
                Spec::Var(x, n) => nth(&b, t, *x, *n, &mut tick),
                Spec::Grad(xs, n) => {
                    let mut r = Ok(t);
                    for _ in 0..*n {
                        r = r.and_then(|u| gradient(&b, u, xs, &mut tick));
                    }
                    r
                }
            };
            t = match r {
                Ok(t) => t,
                Err(Halt::Abort(a)) => return Err(a),
                Err(Halt::Unsupported) => return Ok(Reply::Decline),
            };
        }
        Ok(Reply::Again(t))
    }
}

/// Is `t` something `d` can differentiate in: not a number, string, list, sum, product or power?
pub(crate) fn variable<'h>(b: &Build<'h>, t: T<'h>) -> Result<T<'h>, &'static str> {
    let h = b.heap();
    if b.ar.num(t).is_some() {
        return Err("a number is not a variable");
    }
    match h.view(t) {
        View::Str(_) => Err("a string is not a variable"),
        View::App { head, .. } => match b.sym_of(head) {
            Some(s) if s == b.hd.list => Err("a gradient takes a list of variables"),
            Some(s) if s == b.hd.add || s == b.hd.mul || s == b.hd.pow => {
                Err("a sum, product or power is not a variable")
            }
            _ => Ok(t),
        },
        _ => Ok(t),
    }
}

/// Read one specification: `x`, `[x, n]` or `[x, y, ...]`.
fn spec<'h>(b: &Build<'h>, s: T<'h>) -> Result<Spec<'h>, &'static str> {
    let h = b.heap();
    let items = match h.view(s) {
        View::App { head, args } if b.sym_of(head) == Some(b.hd.list) => args,
        _ => return Ok(Spec::Var(variable(b, s)?, 1)),
    };
    if let [x, n] = items {
        if b.ar.num(*n).is_some() {
            // [[x, y, ...], n]: the n-th gradient (decision IOSYM.9)
            if let (View::App { head, args: xs }, View::Int(n @ 0..=64)) = (h.view(*x), h.view(*n)) {
                if b.sym_of(head) == Some(b.hd.list) {
                    let xs = xs.iter().map(|&x| variable(b, x)).collect::<Result<Vec<_>, _>>()?;
                    return Ok(Spec::Grad(xs, n));
                }
            }
            return match h.view(*n) {
                View::Int(n @ 0..) => Ok(Spec::Var(variable(b, *x)?, n)),
                _ => Err("the order of a derivative must be a non-negative machine integer"),
            };
        }
    }
    let xs = items.iter().map(|&x| variable(b, x)).collect::<Result<Vec<_>, _>>()?;
    Ok(Spec::Grad(xs, 1))
}

/// The gradient of every scalar part of `t` in `xs`: lists map, every other term becomes `[d(t, x1), ...]`.
fn gradient<'h>(b: &Build<'h>, t: T<'h>, xs: &[T<'h>], tick: &mut crate::diff::Tick<'_>) -> Result<T<'h>, Halt> {
    let h = b.heap();
    // post-order over the list structure (an explicit stack: lists may nest deeply)
    let mut out: Vec<T<'h>> = Vec::new();
    let mut stack = vec![(t, false)];
    while let Some((u, ready)) = stack.pop() {
        let items = match h.view(u) {
            View::App { head, args } if b.sym_of(head) == Some(b.hd.list) => args,
            _ => {
                let mut g = Vec::with_capacity(xs.len());
                for &x in xs {
                    g.push(nth(b, u, x, 1, tick)?);
                }
                out.push(h.call(b.hd.list, &g));
                continue;
            }
        };
        if ready {
            let at = out.len() - items.len();
            let l = h.call(b.hd.list, &out[at..]);
            out.truncate(at);
            out.push(l);
        } else {
            stack.push((u, true));
            stack.extend(items.iter().rev().map(|&i| (i, false)));
        }
    }
    // invariant: the walk of one term leaves exactly one result
    Ok(out.pop().expect("one result"))
}

/// `derivative(n1, ..., nk)(f)(a1, ..., ak)` for a known function `f`: its derivative at the arguments, so `sin'(x)`
/// is `cos(x)` outside `d` too (decision 2A.7). A curried call of the native; an unknown `f`, a wrong number of
/// arguments or orders that are not non-negative integers leave the call as it is.
pub struct DerivativeAt {
    /// the flag `mc-arith` polls in long numeric loops
    pub stop: Arc<AtomicBool>,
}

impl Native for DerivativeAt {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let View::App { head, .. } = h.view(t) else { return Ok(Reply::Decline) };
        let _scope = StopScope::enter(Some(&self.stop));
        let b = Build::new(h, &self.stop);
        let Some((orders, f)) = b.as_derivative(head) else {
            // the operator alone, `f'` for a function with a definition: a slot function `&(...)`
            if let Some((orders, f)) = b.as_derivative(t) {
                if let Some(r) = user_derivative(cx, &b, f, &orders, None)? {
                    return Ok(Reply::Done(r));
                }
            }
            return Ok(Reply::Decline);
        };
        if orders.len() != args.len() || orders.iter().any(|&n| n < 0) {
            return Ok(Reply::Decline);
        }
        if let Some(g) = interpolating_derivative(&b, f, &orders) {
            return Ok(Reply::Again(h.app(g, args)));
        }
        let known = b.sym_of(f).is_some_and(|s| crate::rules::resolvable(&h.sym_name(s), &orders));
        if !known {
            // a function with a definition (`rule f(x_) := ...`, `fn f(x) = ...`): differentiate its body
            if let Some(r) = user_derivative(cx, &b, f, &orders, Some(args))? {
                return Ok(Reply::Again(r));
            }
            return Ok(Reply::Decline);
        }
        let mut tick = |n: u64| cx.tick(n);
        match crate::diff::resolve(&b, f, &orders, args, &mut tick) {
            Ok(r) => Ok(Reply::Again(r)),
            Err(Halt::Abort(a)) => Err(a),
            Err(Halt::Unsupported) => Ok(Reply::Decline),
        }
    }

    fn curried(&self) -> bool {
        true
    }
}

/// The highest derivative order written as primes on an interpolating function's name.
const MAX_PRIMES: i64 = 1000;

/// `derivative(n)(interpolating_function("y", rest...))` is the interpolating function of the n-th derivative,
/// `interpolating_function("y" + n primes, rest...)`: the solution store of `n_dsolve` reads the primes as the
/// derivative order (decision C.6). `None` for anything else.
fn interpolating_derivative<'h>(b: &Build<'h>, f: T<'h>, orders: &[i64]) -> Option<T<'h>> {
    let h = b.heap();
    let &[n @ 0..=MAX_PRIMES] = orders else { return None };
    let View::App { head, args: [name, rest @ ..] } = h.view(f) else { return None };
    if b.sym_of(head).map(|s| h.sym_name(s)).as_deref() != Some("interpolating_function") {
        return None;
    }
    let View::Str(name) = h.view(*name) else { return None };
    let mut primed = String::with_capacity(name.len() + n as usize);
    primed.push_str(name);
    primed.extend(std::iter::repeat_n('\'', n as usize));
    let mut items = vec![h.string(&primed)];
    items.extend_from_slice(rest);
    Some(h.app(head, &items))
}

/// `integrate(f, x)`: an antiderivative; `integrate(f, [x, a, b])`: the definite integral. A list `f` integrates
/// elementwise. When no method applies the call stays as it is.
pub struct Integrate {
    /// the flag `mc-arith` polls in long numeric loops
    pub stop: Arc<AtomicBool>,
}

/// `failure(argument, name, message)`.
fn named_failure<'h>(h: &'h Heap, name: &str, message: &str) -> T<'h> {
    let tags = [h.sym_term(h.sym("argument")), h.sym_term(h.sym(name)), h.string(message)];
    h.call(h.sym(names::FAILURE), &tags)
}

impl Native for Integrate {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        // `generate_conditions: v` is accepted (conditions are never generated), and `principal_value: false`
        let args = match args.split_last() {
            Some((&last, rest)) if ignorable_options(h, last) => rest,
            _ => args,
        };
        // several specifications: iterated, the last one innermost (decision IOSYM.9)
        if let [f, specs @ ..] = args {
            if specs.len() > 1 {
                let inner = cx.eval(h.call(h.sym("integrate"), &[*f, specs[specs.len() - 1]]))?;
                if matches!(h.view(inner), View::App { head, .. } if head == h.sym_term(h.sym("integrate"))) {
                    return Ok(Reply::Decline);
                }
                let mut outer = vec![inner];
                outer.extend_from_slice(&specs[..specs.len() - 1]);
                return Ok(Reply::Again(h.call(h.sym("integrate"), &outer)));
            }
        }
        let [f, spec] = args else { return Ok(Reply::Decline) };
        let _scope = StopScope::enter(Some(&self.stop));
        let b = Build::new(h, &self.stop);
        // [x, a, b]: a definite integral
        let bounds = match h.view(*spec) {
            View::App { head, args: [x, lo, hi] } if b.sym_of(head) == Some(b.hd.list) => Some((*x, *lo, *hi)),
            View::App { head, .. } if b.sym_of(head) == Some(b.hd.list) => {
                return Ok(Reply::Done(named_failure(h, "integrate", "the range is [x, a, b]")))
            }
            _ => None,
        };
        // a truncated series in its variable: term by term (decision IOSYM.1)
        if bounds.is_none() && mc_poly::series_data_parts(h, *f).is_some() {
            return Ok(mc_poly::series_integral(cx, *f, *spec)?.map_or(Reply::Decline, Reply::Done));
        }
        let x = bounds.map_or(*spec, |(x, _, _)| x);
        let x = match variable(&b, x) {
            Ok(x) => x,
            Err(message) => return Ok(Reply::Done(named_failure(h, "integrate", message))),
        };
        let plain = |cx: &mut NativeCx<'_, '_, 'h>, f: T<'h>| match bounds {
            Some((_, lo, hi)) => crate::integrate::definite::integrate(cx, &b, f, x, lo, hi),
            None => match crate::integrate::definite::delta::antiderivative(cx, &b, f, x)? {
                Some(r) => Ok(Some(r)),
                None => crate::integrate::antiderivative(cx, &b, f, x, 0),
            },
        };
        // inside assuming, an integrand the facts simplify (powers of products split, `sqrt(x^2)`): refined (with
        // the sign of the variable when both bounds have it) and tried again (decision 2C4.6)
        let one = |cx: &mut NativeCx<'_, '_, 'h>, f: T<'h>| -> Res<Option<T<'h>>> {
            if let Some(r) = plain(cx, f)? {
                return Ok(Some(r));
            }
            if cx.assumptions().is_empty() {
                return Ok(None);
            }
            let mut facts = vec![];
            if let Some((_, lo, hi)) = bounds {
                use crate::integrate::definite::sign;
                match (sign(cx, &b, lo)?, sign(cx, &b, hi)?) {
                    (Some(1), Some(1)) => facts.push(h.call(h.sym("greater"), &[x, b.zero()])),
                    (Some(-1), Some(-1)) => facts.push(h.call(h.sym("less"), &[x, b.zero()])),
                    _ => {}
                }
            }
            let refined = h.call(h.sym("refine"), &[f]);
            let g = if facts.is_empty() { cx.eval(refined)? } else { cx.eval_assuming(&facts, refined)? };
            if g == f {
                return Ok(None);
            }
            plain(cx, g)
        };
        let r = match h.view(*f) {
            View::App { head, args: items } if b.sym_of(head) == Some(b.hd.list) => {
                let mut out = Vec::with_capacity(items.len());
                for &g in items {
                    match one(cx, g)? {
                        Some(r) => out.push(r),
                        None => return Ok(Reply::Decline),
                    }
                }
                Some(h.call(b.hd.list, &out))
            }
            _ => one(cx, *f)?,
        };
        if r.is_none() {
            // no closed form, inexact bounds and an integrand with no other symbols (what `n` leaves of an unevaluated
            // integral): the numerical integral (decision 2C4.7)
            if let Some((_, lo, hi)) = bounds {
                let inexact = |t: T<'h>| matches!(h.view(t), View::Real(_) | View::Ball { .. });
                // an interpolating function (an n_dsolve solution) has only a numerical integral (decision IOSYM.15)
                let interp = !is_free_head(&b, *f, "interpolating_function");
                if (inexact(lo) || inexact(hi) || interp) && closed_in(&b, *f, x) && closed_in(&b, lo, x) && closed_in(&b, hi, x) {
                    let r = cx.eval(h.call(h.sym("n_integrate"), &[*f, *spec]))?;
                    if !matches!(h.view(r), View::App { head, .. } if b.sym_of(head) == Some(h.sym(names::FAILURE)) || b.sym_of(head) == Some(h.sym("n_integrate"))) {
                        return Ok(Reply::Done(r));
                    }
                }
            }
        }
        Ok(r.map_or(Reply::Decline, Reply::Again))
    }
}

/// Is `t` `options(...)` of only `generate_conditions: v` and `principal_value: false`?
fn ignorable_options<'h>(h: &'h Heap, t: T<'h>) -> bool {
    let View::App { head, args } = h.view(t) else { return false };
    if head != h.sym_term(h.sym("options")) || args.is_empty() {
        return false;
    }
    args.iter().all(|&r| match h.view(r) {
        View::App { args: &[k, v], .. } => {
            k == h.sym_term(h.sym("generate_conditions"))
                || k == h.sym_term(h.sym("principal_value")) && v == h.sym_term(h.sym(names::FALSE))
        }
        _ => false,
    })
}

/// `apart(e)`, `apart(e, x)`: partial fractions.
pub struct Apart {
    /// the flag `mc-arith` polls in long numeric loops
    pub stop: Arc<AtomicBool>,
}

impl Native for Apart {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        if let Some(r) = mc_poly::thread_relation(h, "apart", args) {
            return Ok(Reply::Again(r));
        }
        let _scope = StopScope::enter(Some(&self.stop));
        let b = Build::new(h, &self.stop);
        // a number is its own partial fraction decomposition
        if let [e, ..] = args {
            if b.ar.num(*e).is_some() {
                return Ok(Reply::Done(*e));
            }
        }
        let (e, x) = match args {
            [e] => (*e, None),
            [e, x] => match variable(&b, *x) {
                Ok(x) => (*e, Some(x)),
                Err(message) => return Ok(Reply::Done(named_failure(h, "apart", message))),
            },
            _ => return Ok(Reply::Decline),
        };
        Ok(crate::apart::apart(cx, &b, e, x)?.map_or(Reply::Decline, Reply::Again))
    }
}

/// Does `t` have no call whose head is the call `name(...)` (`interpolating_function(...)(t)`)?
fn is_free_head<'h>(b: &Build<'h>, t: T<'h>, name: &str) -> bool {
    let h = b.heap();
    let s = h.sym(name);
    let mut stack = vec![t];
    while let Some(u) = stack.pop() {
        if let View::App { head, args } = h.view(u) {
            if matches!(h.view(head), View::App { head: inner, .. } if b.sym_of(inner) == Some(s)) {
                return false;
            }
            stack.push(head);
            stack.extend(args.iter().copied());
        }
    }
    true
}

/// Does `f` mention no symbol but `x` and the numeric constants (interpolating functions count as constants)?
fn closed_in<'h>(b: &Build<'h>, f: T<'h>, x: T<'h>) -> bool {
    let h = b.heap();
    let interp = h.sym("interpolating_function");
    let mut stack = vec![f];
    while let Some(u) = stack.pop() {
        if matches!(h.view(u), View::App { head, .. } if b.sym_of(head) == Some(interp)) {
            continue;
        }
        match h.view(u) {
            View::Sym(s) => {
                if u != x && !matches!(h.sym_name(s).as_str(), "pi" | "e" | "euler_gamma" | "degree" | "catalan") {
                    return false;
                }
            }
            View::App { args, .. } => stack.extend(args.iter().copied()),
            View::Str(_) => return false,
            _ => {}
        }
    }
    true
}

/// The derivative of a function with a definition (decision 2A.8): `f` applied to fresh variables evaluates to its
/// body, which is differentiated `orders` times in them; with `args` the result at the arguments (`rewrite`), else
/// the slot function `&(...)` of the derivative. `None` when `f` has no definition for such a call or the body is
/// not differentiable here.
fn user_derivative<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    b: &Build<'h>,
    f: T<'h>,
    orders: &[i64],
    args: Option<&[T<'h>]>,
) -> Res<Option<T<'h>>> {
    let h = b.heap();
    if orders.iter().any(|&n| !(0..=64).contains(&n)) {
        return Ok(None);
    }
    match b.sym_of(f) {
        Some(fs) => {
            if crate::rules::lookup(&h.sym_name(fs), orders.len()).is_some() {
                return Ok(None);
            }
        }
        // a pure function `(x, ...) => body` with as many parameters as orders (a d_solve solution, decision
        // IOSYM.6)
        None => {
            let View::App { head, args: &[params, _] } = h.view(f) else { return Ok(None) };
            if b.sym_of(head) != Some(h.sym("fn")) {
                return Ok(None);
            }
            let n = match h.view(params) {
                View::App { head, args } if b.sym_of(head) == Some(b.hd.list) => args.len(),
                _ => 1,
            };
            if n != orders.len() {
                return Ok(None);
            }
        }
    }
    let vars: Vec<T<'h>> = (1..=orders.len())
        .map(|i| h.sym_term(h.sym(&if args.is_some() { format!("derivative·{i}") } else { format!("&{i}") })))
        .collect();
    let call = h.app(f, &vars);
    let mut g = cx.eval(call)?;
    if g == call || crate::integrate::definite::is_failure(b, g) {
        return Ok(None);
    }
    for (i, &n) in orders.iter().enumerate() {
        for _ in 0..n {
            g = cx.eval(h.call(h.sym("d"), &[g, vars[i]]))?;
        }
    }
    let unresolved = |t: T<'h>| !crate::integrate::is_free(b, t, h.sym_term(h.sym("d")));
    if unresolved(g) {
        return Ok(None);
    }
    Ok(Some(match args {
        Some(a) => {
            let rules: Vec<T<'h>> = vars.iter().zip(a).map(|(&v, &x)| h.call(h.sym(names::RULE), &[v, x])).collect();
            h.call(h.sym("rewrite"), &[g, h.call(b.hd.list, &rules)])
        }
        None => h.call(h.sym("fn"), &[h.call(b.hd.list, &vars), g]),
    }))
}
