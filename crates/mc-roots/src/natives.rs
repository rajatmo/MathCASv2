//! The natives `find_root`, `find_minimum` and `find_maximum`: argument forms, the choice of method, the hand-off
//! to the ball refinement, results and failures.
//!
//! | form | method |
//! |---|---|
//! | `find_root(f, [x, x0])` | damped Newton with the symbolic derivative; the secant method without one |
//! | `find_root(f, [x, x0, x1])` | the secant method from two starts |
//! | `find_root(f, [x, x0, a, b])` | Brent's method if f(a), f(b) differ in sign; else Newton constrained to [a, b] |
//! | `find_root([f1, ...], [[x, x0], ...])` | damped Newton with the symbolic Jacobian; Broyden without one |
//! | `find_minimum(f, [x, x0])`, `find_maximum` | a bracket, then Brent's method; polished by Newton on f' |
//! | `find_minimum(f, [[x, x0], ...])` | BFGS with the symbolic gradient; polished by Newton on the gradient |
//!
//! An equation `lhs == rhs` stands for `lhs - rhs` (decision R.1). Complex starting values make Newton's method
//! (and Broyden's) run on complex numbers. `digits: d` refines the machine result with balls; `verify: true` asks for
//! the interval Newton proof of a simple real root in one variable.
use crate::func::{CentralGradient, Machine, NumFn, TermFn};
use crate::options::{self, Method, OptionError, Options};
use crate::refine::{self, Refine};
use crate::scalar::Scalar;
use crate::{bracket, minimize, newton, Outcome};
use mc_api::names;
use mc_arith::num::StopScope;
use mc_arith::{Arith, Num};
use mc_core::{Heap, Store, Sym, View, T};
use mc_eval::{Native, NativeCx, Reply, Res};
use mc_numeric::machine::C64;
use mc_numfn::Policy;
use std::collections::HashSet;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// `find_root(...)`.
pub(crate) struct FindRoot {
    /// the flag `mc-arith` polls in long numeric loops
    pub stop: Arc<AtomicBool>,
}

/// `find_minimum(...)` and `find_maximum(...)`.
pub(crate) struct FindExtremum {
    /// the flag `mc-arith` polls in long numeric loops
    pub stop: Arc<AtomicBool>,
    /// `find_maximum`: minimise -f
    pub maximum: bool,
}

/// `failure(tag, details...)`.
fn failure<'h>(h: &'h Heap, tag: &str, details: &[T<'h>]) -> T<'h> {
    let mut a = vec![h.sym_term(h.sym(tag))];
    a.extend_from_slice(details);
    h.call(h.sym(names::FAILURE), &a)
}

/// `failure(argument, f, k)`: argument (or option) `k` of the function `f` is wrong.
fn bad_argument<'h>(h: &'h Heap, f: &str, k: T<'h>) -> Reply<'h> {
    Reply::Done(failure(h, "argument", &[h.sym_term(h.sym(f)), k]))
}

/// Is `t` a failure?
fn is_failure(h: &Heap, t: T<'_>) -> bool {
    matches!(h.view(t), View::App { head, .. } if head == h.sym_term(h.sym(names::FAILURE)))
}

/// The arguments and options of a call, or the reply refusing them.
fn split_options<'a, 'h>(
    h: &'h Heap,
    ar: &Arith<'h, Heap>,
    name: &str,
    args: &'a [T<'h>],
) -> Result<(&'a [T<'h>], Options), Reply<'h>> {
    match options::split(h, ar, args) {
        Ok(v) => Ok(v),
        Err(OptionError::Unknown(s)) => Err(Reply::Done(failure(h, "unknown_option", &[h.sym_term(s)]))),
        Err(OptionError::Bad(s)) => Err(bad_argument(h, name, h.sym_term(s))),
    }
}

/// One variable with its starting value and, in one variable, the further numbers of the specification.
struct Var {
    sym: Sym,
    start: Num,
    more: Vec<Num>,
}

/// The variable specification: one variable `[x, x0, ...]`, or several `[[x, x0], [y, y0], ...]` (a system).
struct Spec {
    vars: Vec<Var>,
    system: bool,
}

impl Spec {
    fn syms(&self) -> Vec<Sym> {
        self.vars.iter().map(|v| v.sym).collect()
    }
}

/// The numeric value of a starting value or bound: a number, or a closed expression through `n` (`pi`, `sqrt(2)`).
fn number<'h>(cx: &mut NativeCx<'_, '_, 'h>, ar: &Arith<'h, Heap>, t: T<'h>) -> Res<Option<Num>> {
    if let Some(v) = ar.num(t) {
        return Ok(Some(v));
    }
    let h = cx.heap();
    let v = cx.eval(h.call(h.sym("n"), &[t]))?;
    Ok(ar.num(v))
}

/// One `[x, x0, extra...]` with at most `max_more` extra numbers; `None` if malformed.
fn var<'h>(cx: &mut NativeCx<'_, '_, 'h>, ar: &Arith<'h, Heap>, t: T<'h>, max_more: usize) -> Res<Option<Var>> {
    let Some(items) = cx.as_list(t) else { return Ok(None) };
    let [x, x0, more @ ..] = items else { return Ok(None) };
    let View::Sym(sym) = cx.heap().view(*x) else { return Ok(None) };
    if more.len() > max_more {
        return Ok(None);
    }
    let Some(start) = number(cx, ar, *x0)? else { return Ok(None) };
    let mut extra = Vec::with_capacity(more.len());
    for &m in more {
        match number(cx, ar, m)? {
            Some(v) => extra.push(v),
            None => return Ok(None),
        }
    }
    Ok(Some(Var { sym, start, more: extra }))
}

/// The specification (argument 2); `None` if malformed or a variable repeats.
fn spec<'h>(cx: &mut NativeCx<'_, '_, 'h>, ar: &Arith<'h, Heap>, t: T<'h>, max_more: usize) -> Res<Option<Spec>> {
    let Some(items) = cx.as_list(t) else { return Ok(None) };
    let system = items.first().is_some_and(|&i| cx.as_list(i).is_some());
    let mut vars = Vec::new();
    if system {
        for &i in items {
            match var(cx, ar, i, 0)? {
                Some(v) => vars.push(v),
                None => return Ok(None),
            }
        }
    } else {
        match var(cx, ar, t, max_more)? {
            Some(v) => vars.push(v),
            None => return Ok(None),
        }
    }
    let distinct: HashSet<Sym> = vars.iter().map(|v| v.sym).collect();
    Ok((distinct.len() == vars.len()).then_some(Spec { vars, system }))
}

/// `lhs == rhs` as `lhs - rhs`; any other term as it is.
fn residual<'h>(cx: &mut NativeCx<'_, '_, 'h>, ar: &Arith<'h, Heap>, t: T<'h>) -> Res<T<'h>> {
    let h = cx.heap();
    match h.view(t) {
        View::App { head, args: &[l, r] } if head == h.sym_term(h.sym("equal")) => cx.eval(ar.sub(l, r)),
        _ => Ok(t),
    }
}

/// The machine-real view of a closure: interpreted, or compiled by `mc-jit` as the `compile:` option says
/// (`mc_numfn::Policy`; packages N0 and N1: the code comes from the process-wide cache, so a family of bodies
/// differing in their constants compiles once), with the interpreter evaluating every call the compiled code
/// declines. Constants are not folded by the compiler (`consts` false): after `n` none are left, and if `n` failed
/// the interpreter keeps them exact.
fn auto<'r, 'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    f: &'r mut TermFn<'h>,
    ar: &'r Arith<'h, Heap>,
    policy: Policy,
) -> mc_numfn::Auto<'h, Machine<'r, 'h>> {
    let (vars, body) = (f.vars().to_vec(), f.body());
    mc_numfn::Auto::new(cx, Machine { f, ar }, &vars, body, false, policy)
}

/// The expression with its numeric constants evaluated: by `n` at machine precision, or to `digits` digits
/// (decision R.9). The variables stay symbols; a failure of `n` leaves the expression as it is.
fn numeric<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, digits: Option<u64>) -> Res<T<'h>> {
    let h = cx.heap();
    let call = match digits {
        None => h.call(h.sym("n"), &[t]),
        Some(d) => h.call(h.sym("n"), &[t, h.int(d as i64)]),
    };
    let v = cx.eval(call)?;
    Ok(if is_failure(h, v) { t } else { v })
}

/// The symbolic derivative `d(t, x)` or Jacobian `d(t, [x, ...])` through the evaluator (one call, decision R.5);
/// `None` when `d` cannot differentiate (the result still contains `d` or `derivative`, or is a failure).
fn derivative<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, vars: &[Sym], list: bool) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    let xs: Vec<T<'h>> = vars.iter().map(|&v| h.sym_term(v)).collect();
    let spec = if list { cx.list(&xs) } else { xs[0] };
    let r = cx.eval(h.call(h.sym("d"), &[t, spec]))?;
    if is_failure(h, r) || mentions(cx, r, &[h.sym("d"), h.sym("derivative")])? {
        return Ok(None);
    }
    Ok(Some(r))
}

/// Does `t` contain one of the symbols `syms` (as a head or anywhere)? Each shared subterm is visited once.
fn mentions<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, syms: &[Sym]) -> Res<bool> {
    let h = cx.heap();
    let mut seen = HashSet::new();
    let mut stack = vec![t];
    let mut visited = 0u64;
    while let Some(u) = stack.pop() {
        if !seen.insert(u) {
            continue;
        }
        visited += 1;
        if visited.is_multiple_of(1024) {
            cx.tick(1)?;
        }
        match h.view(u) {
            View::Sym(s) if syms.contains(&s) => return Ok(true),
            View::App { head, args } => {
                stack.push(head);
                stack.extend_from_slice(args);
            }
            _ => {}
        }
    }
    Ok(false)
}

/// `[x -> v, ...]`.
fn rules<'h>(h: &'h Heap, ar: &Arith<'h, Heap>, vars: &[Sym], values: &[Num]) -> T<'h> {
    let rule = h.sym(names::RULE);
    let rs: Vec<T<'h>> = vars.iter().zip(values).map(|(&x, v)| h.call(rule, &[h.sym_term(x), ar.number(v)])).collect();
    h.call(h.sym(names::LIST), &rs)
}

/// Is the number complex with a nonzero imaginary part?
fn is_complex(n: &Num) -> bool {
    matches!(n, Num::Complex(_, im) if !im.is_zero())
}

/// The machine values of numbers as scalars of a kind (exact and ball numbers rounded).
fn scalars<S: Scalar>(v: &[Num]) -> Option<Vec<S>> {
    v.iter().map(S::of_num).collect()
}

/// An outcome on numbers.
fn to_nums<S: Scalar>(o: Outcome<S>) -> Outcome<Num> {
    match o {
        Outcome::Done { x, err, converged, iterations } => {
            Outcome::Done { x: x.into_iter().map(S::to_num).collect(), err, converged, iterations }
        }
        Outcome::NotNumeric(x) => Outcome::NotNumeric(x.into_iter().map(S::to_num).collect()),
    }
}

/// `2^e` as a machine real (the error estimate of a refined result), clamped to the machine range.
fn pow2(e: f64) -> f64 {
    e.clamp(-1074.0, 1023.0).exp2()
}

/// How the one-variable root search runs.
enum RootMethod {
    /// Newton's method (with `symbolic` derivative when it exists)
    Newton { symbolic: bool },
    /// the secant method from the start and this second point
    Secant(f64),
    /// a bracket: Brent's method, or Newton constrained to it
    Bracket(f64, f64),
}

impl Native for FindRoot {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        const NAME: &str = "find_root";
        let h = cx.heap();
        let ar = Arith::new(h).with_stop(self.stop.clone());
        let (args, opts) = match split_options(h, &ar, NAME, args) {
            Ok(v) => v,
            Err(r) => return Ok(r),
        };
        let &[f, s] = args else { return Ok(Reply::Decline) };
        let _scope = StopScope::enter(Some(&self.stop));
        let Some(sp) = spec(cx, &ar, s, 2)? else { return Ok(bad_argument(h, NAME, h.int(2))) };
        if opts.verify && opts.digits.is_none() {
            return Ok(bad_argument(h, NAME, h.sym_term(h.sym("verify"))));
        }
        let n = sp.vars.len();
        // the residuals: a list of n for a system, one term otherwise
        let body = if sp.system {
            let Some(fs) = cx.as_list(f) else { return Ok(bad_argument(h, NAME, h.int(1))) };
            if fs.len() != n {
                return Ok(bad_argument(h, NAME, h.int(1)));
            }
            let mut rs = Vec::with_capacity(n);
            for &fi in fs {
                if cx.as_list(fi).is_some() {
                    return Ok(bad_argument(h, NAME, h.int(1)));
                }
                rs.push(residual(cx, &ar, fi)?);
            }
            cx.list(&rs)
        } else {
            if cx.as_list(f).is_some() {
                return Ok(bad_argument(h, NAME, h.int(1)));
            }
            residual(cx, &ar, f)?
        };
        let vars = sp.syms();
        let starts: Vec<Num> = sp.vars.iter().map(|v| v.start.clone()).collect();
        let complex = starts.iter().any(is_complex);
        let more = &sp.vars[0].more;
        let real = |v: &Num| <f64 as Scalar>::of_num(v);
        // the method (decision R.6)
        let method = match (opts.method, more.len()) {
            (None | Some(Method::Newton), 0) => RootMethod::Newton { symbolic: true },
            (Some(Method::Broyden), 0) => RootMethod::Newton { symbolic: false },
            (Some(Method::Secant), 0) if !sp.system => {
                let x0 = real(&starts[0]).unwrap_or(0.0);
                RootMethod::Secant(x0 + 1e-4 * x0.abs().max(1.0))
            }
            (None | Some(Method::Secant), 1) => match real(&more[0]) {
                Some(x1) => RootMethod::Secant(x1),
                None => return Ok(bad_argument(h, NAME, h.int(2))),
            },
            (None | Some(Method::Brent), 2) => match (real(&more[0]), real(&more[1])) {
                (Some(a), Some(b)) if a != b => RootMethod::Bracket(a.min(b), a.max(b)),
                _ => return Ok(bad_argument(h, NAME, h.int(2))),
            },
            // a bracketing method with two values [x, a, b]: the bracket [a, b] (gap G71)
            (Some(Method::Brent), 1) if !sp.system => match (real(&starts[0]), real(&more[0])) {
                (Some(a), Some(b)) if a != b => RootMethod::Bracket(a.min(b), a.max(b)),
                _ => return Ok(bad_argument(h, NAME, h.int(2))),
            },
            (Some(Method::Secant), 2) => match real(&more[0]) {
                Some(x1) => RootMethod::Secant(x1),
                None => return Ok(bad_argument(h, NAME, h.int(2))),
            },
            (Some(Method::Newton), _) => RootMethod::Newton { symbolic: true },
            (Some(Method::Broyden), _) => RootMethod::Newton { symbolic: false },
            _ => return Ok(bad_argument(h, NAME, h.sym_term(h.sym("method")))),
        };
        if complex && !matches!(method, RootMethod::Newton { .. }) {
            return Ok(bad_argument(h, NAME, h.int(2)));
        }
        let goals = opts.machine_goals();
        let mut fm = TermFn::new(h, &vars, numeric(cx, body, None)?);
        // every residual a scalar (a nested list would give more outputs than unknowns)
        if fm.outputs() != n {
            return Ok(bad_argument(h, NAME, h.int(1)));
        }
        let symbolic = derivative(cx, body, &vars, sp.system)?;
        let mut jm = match (&method, symbolic) {
            (RootMethod::Newton { symbolic: true }, Some(d)) => Some(TermFn::new(h, &vars, numeric(cx, d, None)?)),
            _ => None,
        }
        .filter(|j| j.outputs() == n * n);
        // the machine iteration
        let outcome = if complex {
            let Some(x0) = scalars::<C64>(&starts) else { return Ok(bad_argument(h, NAME, h.int(2))) };
            let mut fv = Machine { f: &mut fm, ar: &ar };
            let mut jv = jm.as_mut().map(|j| Machine { f: j, ar: &ar });
            let jd = jv.as_mut().map(|j| j as &mut dyn NumFn<'h, C64>);
            to_nums(newton::solve(cx, &mut fv, jd, x0, &goals, None)?)
        } else {
            let Some(x0) = scalars::<f64>(&starts) else { return Ok(bad_argument(h, NAME, h.int(2))) };
            // machine reals: compiled when the policy says, the interpreter for refused and declined calls
            // (package N0; the results are the interpreter's)
            let mut fv = auto(cx, &mut fm, &ar, opts.compile);
            let mut jv = jm.as_mut().map(|j| auto(cx, j, &ar, opts.compile));
            let jd = jv.as_mut().map(|j| j as &mut dyn NumFn<'h, f64>);
            to_nums(match method {
                RootMethod::Newton { symbolic: true } if jd.is_none() && n == 1 => {
                    // no symbolic derivative in one variable: the secant method (decision R.5)
                    bracket::secant(cx, &mut fv, x0[0], x0[0] + 1e-4 * x0[0].abs().max(1.0), &goals)?
                }
                RootMethod::Newton { .. } => newton::solve(cx, &mut fv, jd, x0, &goals, None)?,
                RootMethod::Secant(x1) => bracket::secant(cx, &mut fv, x0[0], x1, &goals)?,
                RootMethod::Bracket(a, b) => match bracket::ends(cx, &mut fv, a, b)? {
                    Err(p) => Outcome::NotNumeric(vec![p]),
                    Ok((fa, fb)) if (fa <= 0.0) != (fb <= 0.0) || fa == 0.0 || fb == 0.0 => {
                        bracket::brent(cx, &mut fv, (a, b), (fa, fb), &goals)?
                    }
                    // no sign change: Newton constrained to the bracket (decision R.7)
                    Ok(_) => {
                        let start = x0[0].clamp(a, b);
                        newton::solve(cx, &mut fv, jd, vec![start], &goals, Some(&[(a, b)]))?
                    }
                },
            })
        };
        let x = match outcome {
            Outcome::NotNumeric(p) => {
                return Ok(Reply::Done(failure(h, "not_numeric", &[f, rules(h, &ar, &vars, &p)])));
            }
            Outcome::Done { x, err, converged: false, .. } => {
                let tags = [h.sym_term(h.sym(NAME)), rules(h, &ar, &vars, &x), h.real(err)];
                return Ok(Reply::Done(failure(h, "not_converged", &tags)));
            }
            Outcome::Done { x, .. } => x,
        };
        let Some((p, p0)) = opts.ball_precision() else { return Ok(Reply::Done(rules(h, &ar, &vars, &x))) };
        // refinement with balls (decision R.9), with the derivative for Newton steps and the verification
        let digits = opts.digits.unwrap_or(0);
        let d = digits + options::GUARD_DIGITS;
        let mut fb = TermFn::new(h, &vars, numeric(cx, body, Some(d))?);
        let mut jb = match symbolic {
            Some(dt) => Some(TermFn::new(h, &vars, numeric(cx, dt, Some(d))?)),
            None => None,
        }
        .filter(|j| j.outputs() == n * n);
        let r = refine::refine(cx, &ar, &mut fb, jb.as_mut(), &x, p, opts.goals_log2(), goals.max_iterations, true)?;
        let r = match r {
            Refine::Done(r) => r,
            Refine::NotNumeric(pt) => {
                return Ok(Reply::Done(failure(h, "not_numeric", &[f, rules(h, &ar, &vars, &pt)])));
            }
            Refine::Interrupted => return Ok(Reply::Done(failure(h, names::failure::INTERRUPTED, &[]))),
        };
        let xs: Vec<Num> = r.x.iter().map(|v| refine::result_num(v, p0, digits)).collect();
        let found = rules(h, &ar, &vars, &xs);
        if !r.converged {
            let tags = [h.sym_term(h.sym(NAME)), found, h.real(pow2(r.err_log2))];
            return Ok(Reply::Done(failure(h, "not_converged", &tags)));
        }
        if opts.verify && !r.verified {
            return Ok(Reply::Done(failure(h, "not_verified", &[h.sym_term(h.sym(NAME)), found])));
        }
        Ok(Reply::Done(found))
    }
}

impl Native for FindExtremum {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let name = if self.maximum { "find_maximum" } else { "find_minimum" };
        let h = cx.heap();
        let ar = Arith::new(h).with_stop(self.stop.clone());
        let (args, opts) = match split_options(h, &ar, name, args) {
            Ok(v) => v,
            Err(r) => return Ok(r),
        };
        let &[f, s] = args else { return Ok(Reply::Decline) };
        let _scope = StopScope::enter(Some(&self.stop));
        let Some(sp) = spec(cx, &ar, s, 0)? else { return Ok(bad_argument(h, name, h.int(2))) };
        if sp.vars.iter().any(|v| is_complex(&v.start)) {
            return Ok(bad_argument(h, name, h.int(2)));
        }
        if opts.verify {
            return Ok(Reply::Done(failure(h, "unknown_option", &[h.sym_term(h.sym("verify"))])));
        }
        let is_eq = matches!(h.view(f), View::App { head, .. } if head == h.sym_term(h.sym("equal")));
        if cx.as_list(f).is_some() || is_eq {
            return Ok(bad_argument(h, name, h.int(1)));
        }
        let n = sp.vars.len();
        let vars = sp.syms();
        // the objective: f, or -f for a maximum
        let obj = if self.maximum { cx.eval(ar.neg(f))? } else { f };
        let Some(x0) = scalars::<f64>(&sp.vars.iter().map(|v| v.start.clone()).collect::<Vec<_>>()) else {
            return Ok(bad_argument(h, name, h.int(2)));
        };
        let goals = opts.machine_goals();
        let list = sp.system;
        let grad = derivative(cx, obj, &vars, list)?;
        let hess = match grad {
            Some(g) => derivative(cx, g, &vars, list)?,
            None => None,
        };
        let mobj = numeric(cx, obj, None)?;
        let mut fm = TermFn::new(h, &vars, mobj);
        // a gradient and Hessian of the right shapes only (an objective that is no scalar function has neither)
        let (grad, hess) = (grad.filter(|&g| TermFn::new(h, &vars, g).outputs() == n), hess);
        let hess = hess.filter(|&t| grad.is_some() && TermFn::new(h, &vars, t).outputs() == n * n);
        let mut gm = match grad {
            Some(g) => Some(TermFn::new(h, &vars, numeric(cx, g, None)?)),
            None => None,
        };
        let mut hm = match hess {
            Some(t) => Some(TermFn::new(h, &vars, numeric(cx, t, None)?)),
            None => None,
        };
        // machine reals: compiled as the `compile:` option says (package N1), the interpreter for refused and
        // declined calls, so the results are the interpreter's
        let pol = opts.compile;
        let coarse = match (opts.method, n) {
            (Some(Method::Newton), _) => match (gm.as_mut(), hm.as_mut()) {
                (Some(g), Some(hs)) => {
                    let mut gv = auto(cx, g, &ar, pol);
                    let mut hv = auto(cx, hs, &ar, pol);
                    newton::solve(cx, &mut gv, Some(&mut hv), x0, &goals, None)?
                }
                _ => return Ok(bad_argument(h, name, h.sym_term(h.sym("method")))),
            },
            (None | Some(Method::Brent), 1) if !list => {
                let mut fv = auto(cx, &mut fm, &ar, pol);
                minimize::brent_min(cx, &mut fv, x0[0], &goals)?
            }
            (None | Some(Method::Bfgs), _) => {
                let mut fv = auto(cx, &mut fm, &ar, pol);
                match gm.as_mut() {
                    Some(g) => {
                        let mut gv = auto(cx, g, &ar, pol);
                        minimize::bfgs(cx, &mut fv, &mut gv, x0, &goals)?
                    }
                    None => {
                        // forward the objective twice: once for values, once for the differences
                        let mut fd = TermFn::new(h, &vars, mobj);
                        let mut g = CentralGradient { f: auto(cx, &mut fd, &ar, pol), n };
                        minimize::bfgs(cx, &mut fv, &mut g, x0, &goals)?
                    }
                }
            }
            _ => return Ok(bad_argument(h, name, h.sym_term(h.sym("method")))),
        };
        let (mut x, mut converged, mut err) = match coarse {
            Outcome::NotNumeric(p) => {
                let pt = p.into_iter().map(|v| v.to_num()).collect::<Vec<_>>();
                return Ok(Reply::Done(failure(h, "not_numeric", &[f, rules(h, &ar, &vars, &pt)])));
            }
            Outcome::Done { x, converged, err, .. } => (x, converged, err),
        };
        // polishing by Newton on the gradient (decision R.12): after a converged search, accepted if it converges
        // without raising f
        if let (true, Some(g), Some(hs)) = (converged, gm.as_mut(), hm.as_mut()) {
            let mut gv = auto(cx, g, &ar, pol);
            let mut hv = auto(cx, hs, &ar, pol);
            if let Outcome::Done { x: xp, converged: true, err: ep, .. } =
                newton::solve(cx, &mut gv, Some(&mut hv), x.clone(), &goals, None)?
            {
                let mut fv = auto(cx, &mut fm, &ar, pol);
                let (mut a, mut b) = ([0.0], [0.0]);
                if fv.eval(cx, &x, &mut a)? && fv.eval(cx, &xp, &mut b)? && b[0] <= a[0] + 4.0 * f64::EPSILON * a[0].abs()
                {
                    (x, converged, err) = (xp, true, ep);
                }
            }
        }
        let mut fx = [0.0];
        let fval = |v: f64| if self.maximum { -v } else { v };
        let mut fv = auto(cx, &mut fm, &ar, pol);
        if !fv.eval(cx, &x, &mut fx)? {
            let pt: Vec<Num> = x.iter().map(|v| v.to_num()).collect();
            return Ok(Reply::Done(failure(h, "not_numeric", &[f, rules(h, &ar, &vars, &pt)])));
        }
        let xs: Vec<Num> = x.iter().map(|v| v.to_num()).collect();
        let found = cx.list(&[h.real(fval(fx[0])), rules(h, &ar, &vars, &xs)]);
        if !converged {
            return Ok(Reply::Done(failure(h, "not_converged", &[h.sym_term(h.sym(name)), found, h.real(err)])));
        }
        let Some((p, p0)) = opts.ball_precision() else { return Ok(Reply::Done(found)) };
        // refinement: Newton on the gradient with balls (decision R.13)
        let (Some(g), Some(hs)) = (grad, hess) else {
            return Ok(bad_argument(h, name, h.sym_term(h.sym("digits"))));
        };
        let digits = opts.digits.unwrap_or(0);
        let d = digits + options::GUARD_DIGITS;
        let mut gb = TermFn::new(h, &vars, numeric(cx, g, Some(d))?);
        let mut hb = TermFn::new(h, &vars, numeric(cx, hs, Some(d))?);
        let mut ob = TermFn::new(h, &vars, numeric(cx, obj, Some(d))?);
        let r = refine::refine(cx, &ar, &mut gb, Some(&mut hb), &xs, p, opts.goals_log2(), goals.max_iterations, false)?;
        let r = match r {
            Refine::Done(r) => r,
            Refine::NotNumeric(pt) => {
                return Ok(Reply::Done(failure(h, "not_numeric", &[f, rules(h, &ar, &vars, &pt)])));
            }
            Refine::Interrupted => return Ok(Reply::Done(failure(h, names::failure::INTERRUPTED, &[]))),
        };
        let at: Vec<T<'h>> = r.x.iter().map(|v| ar.number(v)).collect();
        let Some(v) = ob.eval_terms(cx, &ar, &at)?.and_then(|v| v.into_iter().next()) else {
            return Ok(Reply::Done(failure(h, "not_numeric", &[f, rules(h, &ar, &vars, &r.x)])));
        };
        let v = if self.maximum { v.neg() } else { v };
        let xs: Vec<Num> = r.x.iter().map(|v| refine::result_num(v, p0, digits)).collect();
        let found = cx.list(&[ar.number(&refine::result_num(&v, p0, digits)), rules(h, &ar, &vars, &xs)]);
        if !r.converged {
            let tags = [h.sym_term(h.sym(name)), found, h.real(pow2(r.err_log2))];
            return Ok(Reply::Done(failure(h, "not_converged", &tags)));
        }
        Ok(Reply::Done(found))
    }
}
