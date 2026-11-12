//! The native `n_integrate`: argument forms, the choice of method and precision, results and failures.
//!
//! | form | meaning |
//! |---|---|
//! | `n_integrate(f, [x, a, b])` | the integral of f from a to b (reals, `-inf`, `inf`, numeric expressions such as `pi`) |
//! | `n_integrate(f, [x, a, s1, ..., b])` | split at the points s1, ... (singularities inside the range) |
//! | `n_integrate(f, [x, a, b], [y, c, d], ...)` | a multiple integral, x outermost; c and d may depend on x |
//!
//! Machine precision (decision N2.1): adaptive Gauss–Kronrod, or the double-exponential rule for endpoint
//! singularities, on the integrand compiled by `mc_numfn` when it pays (interpreted otherwise); complex values
//! switch the whole integral to complex machine numbers (interpreted). `digits: d`: the tanh-sinh family on balls
//! of d plus guard digits, the result a ball whose radius is the error estimate; `rigorous: true` adds
//! Clenshaw–Curtis bounds from complex-ball evaluation on Bernstein ellipses, and the ball contains the integral.
use crate::machine::{End, Outcome};
use crate::multi::{self, Kind as MKind};
use crate::nested::{Bound, Plan, Problem};
use crate::options::{self, Method, OptionError, Options};
use crate::{gk, hp, rigorous};
use mc_api::names;
use mc_arith::num::{Ball, Mag, StopScope};
use mc_arith::{Arith, Kind, Num, Real};
use mc_core::{Heap, Store, Sym, View, T};
use mc_eval::{Native, NativeCx, Reply, Res};
use mc_numeric::machine::C64;
use mc_numfn::{Interpreted, NumFn, Prep, Scalar, Var};
use std::collections::HashSet;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

const NAME: &str = "n_integrate";

/// The most ranges of a multiple integral by nested one-dimensional rules (the work grows as a power of their
/// number).
const MAX_RANGES: usize = 6;

/// The most ranges of any multiple integral (the sampling methods).
const MAX_DIMS: usize = 64;

/// `n_integrate(...)`.
pub(crate) struct NIntegrate {
    /// the flag `mc-arith` polls in long numeric loops
    pub stop: Arc<AtomicBool>,
}

/// `failure(tag, details...)`.
fn failure<'h>(h: &'h Heap, tag: &str, details: &[T<'h>]) -> T<'h> {
    let mut a = vec![h.sym_term(h.sym(tag))];
    a.extend_from_slice(details);
    h.call(h.sym(names::FAILURE), &a)
}

/// `failure(argument, n_integrate, k)`.
fn bad_argument<'h>(h: &'h Heap, k: T<'h>) -> Reply<'h> {
    Reply::Done(failure(h, "argument", &[h.sym_term(h.sym(NAME)), k]))
}

/// A point of a range as written.
#[derive(Clone, Copy)]
enum Point<'h> {
    /// `inf` (`true`) or `-inf`
    Inf(bool),
    /// a real number: the term (for balls) and its machine value
    Num(T<'h>, f64),
    /// an expression in the outer variables
    Expr(T<'h>),
}

/// `inf` or `-inf` (`mul(-1, inf)`, also after `n`)?
fn infinity<'h>(h: &'h Heap, ar: &Arith<'h, Heap>, t: T<'h>) -> Option<bool> {
    let inf = h.sym_term(h.sym("inf"));
    if t == inf {
        return Some(true);
    }
    match h.view(t) {
        View::App { head, args: &[c, i] } if head == h.sym_term(h.sym("mul")) && i == inf => {
            let c = ar.num(c)?.as_real()?.to_f64();
            (c != 0.0 && c.is_finite()).then_some(c > 0.0)
        }
        _ => None,
    }
}

/// A point of range `k`: an infinity, a real number (directly or through `n`), or (for an inner range) an
/// expression; `None` for anything else (a complex number, an unknown symbol in the outermost range).
fn point<'h>(cx: &mut NativeCx<'_, '_, 'h>, ar: &Arith<'h, Heap>, t: T<'h>, inner: bool) -> Res<Option<Point<'h>>> {
    let h = cx.heap();
    if let Some(s) = infinity(h, ar, t) {
        return Ok(Some(Point::Inf(s)));
    }
    let real = |v: &Num| match v {
        Num::Real(r) => Some(r.to_f64()),
        Num::Complex(..) => None,
    };
    if let Some(v) = ar.num(t) {
        return Ok(real(&v).filter(|x| x.is_finite()).map(|x| Point::Num(t, x)));
    }
    let v = cx.eval(h.call(h.sym("n"), &[t]))?;
    if let Some(s) = infinity(h, ar, v) {
        return Ok(Some(Point::Inf(s)));
    }
    if let Some(n) = ar.num(v) {
        return Ok(real(&n).filter(|x| x.is_finite()).map(|x| Point::Num(t, x)));
    }
    Ok(inner.then_some(Point::Expr(t)))
}

/// The variables of the ranges and the points of each.
type Ranges<'h> = (Vec<Sym>, Vec<Vec<Point<'h>>>);

/// The ranges (arguments 2, 3, ...): a variable and at least two points each; infinities only at the ends;
/// distinct variables. `Err(k)`: argument k is malformed.
fn ranges<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    ar: &Arith<'h, Heap>,
    specs: &[T<'h>],
) -> Res<Result<Ranges<'h>, usize>> {
    let mut vars = Vec::new();
    let mut out = Vec::new();
    for (i, &s) in specs.iter().enumerate() {
        let k = i + 2;
        let Some(items) = cx.as_list(s) else { return Ok(Err(k)) };
        let [x, pts @ ..] = items else { return Ok(Err(k)) };
        let View::Sym(x) = cx.heap().view(*x) else { return Ok(Err(k)) };
        if pts.len() < 2 || vars.contains(&x) {
            return Ok(Err(k));
        }
        let mut ps = Vec::with_capacity(pts.len());
        for (j, &p) in pts.iter().enumerate() {
            match point(cx, ar, p, i > 0)? {
                Some(Point::Inf(_)) if j != 0 && j != pts.len() - 1 => return Ok(Err(k)),
                Some(v) => ps.push(v),
                None => return Ok(Err(k)),
            }
        }
        vars.push(x);
        out.push(ps);
    }
    Ok(Ok((vars, out)))
}

/// The points of each range as terms (`inf`, `-inf`, numbers, expressions).
fn points_terms<'h>(h: &'h Heap, points: &[Vec<Point<'h>>]) -> Vec<Vec<T<'h>>> {
    let inf = h.sym_term(h.sym("inf"));
    points
        .iter()
        .map(|ps| {
            ps.iter()
                .map(|p| match *p {
                    Point::Inf(true) => inf,
                    Point::Inf(false) => h.call(h.sym("mul"), &[h.int(-1), inf]),
                    Point::Num(t, _) | Point::Expr(t) => t,
                })
                .collect()
        })
        .collect()
}

/// Is `t` a failure?
fn is_failure(h: &Heap, t: T<'_>) -> bool {
    matches!(h.view(t), View::App { head, .. } if head == h.sym_term(h.sym(names::FAILURE)))
}

/// A complex point of a path: a complex number, or a direction to infinity `c * inf` with c complex (`im * inf`).
enum CPoint<'h> {
    At(T<'h>),
    Toward(T<'h>),
}

/// The integral along a path through complex points (decision quad-2.1): `[z, a, b, c, ...]` with some point
/// complex is the sum over the straight segments, segment p to q being the integral of f(p + t (q - p)) (q - p)
/// for t from 0 to 1, and a last point `d * inf` the ray p + t d/|d| for t from 0 to inf. `None` when every point
/// is real (the ordinary path).
fn contour<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    ar: &Arith<'h, Heap>,
    f: T<'h>,
    spec: T<'h>,
    all: &[T<'h>],
    nargs: usize,
) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    let Some(items) = cx.as_list(spec) else { return Ok(None) };
    let [x, pts @ ..] = items else { return Ok(None) };
    let View::Sym(_) = h.view(*x) else { return Ok(None) };
    if pts.len() < 2 {
        return Ok(None);
    }
    let inf = h.sym_term(h.sym("inf"));
    let mut cps = Vec::with_capacity(pts.len());
    let mut complex = false;
    for (j, &p) in pts.iter().enumerate() {
        let v = cx.eval(h.call(h.sym("n"), &[p]))?;
        // c * inf with c complex
        if let View::App { head, args: &[c, i] } = h.view(v) {
            if head == h.sym_term(h.sym("mul")) && i == inf && j == pts.len() - 1 {
                if let Some(Num::Complex(..)) = ar.num(c) {
                    complex = true;
                    cps.push(CPoint::Toward(c));
                    continue;
                }
            }
        }
        match ar.num(v) {
            Some(Num::Complex(_, im)) => {
                complex |= !im.is_zero();
                cps.push(CPoint::At(p));
            }
            Some(_) => cps.push(CPoint::At(p)),
            None => return Ok(None),
        }
    }
    if !complex {
        return Ok(None);
    }
    let t = h.sym_term(h.sym("quad·t"));
    let opt_term = (nargs < all.len()).then(|| all[all.len() - 1]);
    let mut total = Vec::new();
    for w in cps.windows(2) {
        let CPoint::At(p) = w[0] else { return Ok(None) };
        let (z, jac, hi) = match w[1] {
            CPoint::At(q) => {
                let d = ar.sub(q, p);
                (ar.add(&[p, ar.mul(&[t, d])]), d, h.int(1))
            }
            CPoint::Toward(c) => {
                let d = ar.div(c, h.call(h.sym("abs"), &[c]));
                (ar.add(&[p, ar.mul(&[t, d])]), d, inf)
            }
        };
        let g = mc_numfn::replace(h, f, &mut |u| (u == *x).then_some(z));
        let g = cx.eval(ar.mul(&[g, jac]))?;
        let mut call = vec![g, cx.list(&[t, h.int(0), hi])];
        call.extend(opt_term);
        let v = cx.eval(h.call(h.sym(NAME), &call))?;
        if is_failure(h, v) || ar.num(v).is_none() {
            return Ok(Some(v));
        }
        total.push(v);
    }
    Ok(Some(cx.eval(ar.add(&total))?))
}

/// `[x -> v, ...]` for the first `values.len()` variables.
fn rules<'h>(h: &'h Heap, ar: &Arith<'h, Heap>, vars: &[Sym], values: &[Num]) -> T<'h> {
    let rule = h.sym(names::RULE);
    let rs: Vec<T<'h>> = vars.iter().zip(values).map(|(&x, v)| h.call(rule, &[h.sym_term(x), ar.number(v)])).collect();
    h.call(h.sym(names::LIST), &rs)
}

/// The number term of a machine scalar.
fn scalar_term<'h, S: Scalar>(ar: &Arith<'h, Heap>, v: S) -> T<'h> {
    ar.number(&v.to_num())
}

impl Native for NIntegrate {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let ar = Arith::new(h).with_stop(self.stop.clone());
        let all = args;
        let (args, opts) = match options::split(h, &ar, args) {
            Ok(v) => v,
            Err(OptionError::Unknown(s)) => return Ok(Reply::Done(failure(h, "unknown_option", &[h.sym_term(s)]))),
            Err(OptionError::Bad(s)) => return Ok(bad_argument(h, h.sym_term(s))),
        };
        let [f, specs @ ..] = args else { return Ok(Reply::Decline) };
        if specs.is_empty() {
            return Ok(Reply::Decline);
        }
        let _scope = StopScope::enter(Some(&self.stop));
        // a list of integrands (a vector or matrix): each element integrated
        if let Some(items) = cx.as_list(*f) {
            let opt_term = (args.len() < all.len()).then(|| all[all.len() - 1]);
            let mut out = Vec::with_capacity(items.len());
            for &it in items {
                let mut call = vec![it];
                call.extend_from_slice(specs);
                call.extend(opt_term);
                let v = cx.eval(h.call(h.sym(NAME), &call))?;
                if is_failure(h, v) {
                    return Ok(Reply::Done(v));
                }
                out.push(v);
            }
            return Ok(Reply::Done(cx.list(&out)));
        }
        // a path through complex points: the integral along each straight segment
        if specs.len() == 1 {
            if let Some(r) = contour(cx, &ar, *f, specs[0], all, args.len())? {
                return Ok(Reply::Done(r));
            }
        }
        if specs.len() > MAX_DIMS || cx.as_list(*f).is_some() {
            return Ok(bad_argument(h, h.int(if cx.as_list(*f).is_some() { 1 } else { MAX_DIMS as i64 + 2 })));
        }
        let (vars, points) = match ranges(cx, &ar, specs)? {
            Ok(v) => v,
            Err(k) => return Ok(bad_argument(h, h.int(k as i64))),
        };
        // an inner range may use only the variables of the ranges outside it
        for (k, ps) in points.iter().enumerate() {
            for p in ps {
                if let Point::Expr(t) = p {
                    if mentions(cx, *t, &vars[k..])? {
                        return Ok(bad_argument(h, h.int(k as i64 + 2)));
                    }
                }
            }
        }
        // precision contagion (the precision contract): ends or integrand numbers that are balls of d digits, with
        // no machine real anywhere, ask for the integral at d digits (what `n(integrate(...), d)` leaves of an
        // integral without a closed form)
        let mut opts = opts;
        if opts.digits.is_none() && !opts.rigorous && points.len() == 1 && opts.method.is_none() {
            if let Some(d) = ball_digits(h, &ar, *f, &points[0]) {
                opts.digits = Some(d);
            }
        }
        // indicator functions of semialgebraic regions: the region's cells as integration limits
        if opts.method.is_none() {
            let opt_term = (args.len() < all.len()).then(|| all[all.len() - 1]);
            if let Some(r) = crate::regions::steps_and_deltas(cx, *f, &vars, &points_terms(h, &points), opt_term)? {
                return Ok(Reply::Done(r));
            }
            if let Some(r) = crate::regions::boole_cells(cx, *f, &vars, &points_terms(h, &points), opt_term)? {
                return Ok(Reply::Done(r));
            }
            if let Some(r) = crate::regions::squared_trig_tail(cx, *f, &vars, &points_terms(h, &points), opt_term)? {
                return Ok(Reply::Done(r));
            }
            if let Some(r) = crate::regions::log_half_line(cx, *f, &vars, &points_terms(h, &points), opt_term)? {
                return Ok(Reply::Done(r));
            }
        }
        let kind = opts.method.and_then(MKind::of);
        if opts.digits.is_some() || opts.rigorous {
            if kind.is_some() {
                return Ok(bad_argument(h, h.sym_term(h.sym("method"))));
            }
            return ball_call(cx, &ar, *f, &vars, &points, &opts);
        }
        // the method (decision quad-1.2): nested one-dimensional rules for one variable or when a one-dimensional
        // method is named; the multidimensional methods otherwise
        let (kind, auto) = match (opts.method, kind) {
            (Some(_), Some(k)) => (Some(k), false),
            (Some(_), None) => (None, false),
            (None, _) if vars.len() >= 2 => (Some(multi::automatic(cx, *f, vars.len())?), true),
            (None, _) => (None, true),
        };
        match kind {
            Some(k) => multi_call(cx, &ar, *f, &vars, &points, &opts, k, auto),
            None if vars.len() > MAX_RANGES => Ok(bad_argument(h, h.int(MAX_RANGES as i64 + 2))),
            None => machine_call(cx, &ar, *f, &vars, &points, &opts),
        }
    }
}

/// With the environment variable `MC_QUAD_TRACE` naming a file, every machine-precision integration appends a
/// line to it: method, variables, evaluations of the integrand, estimate, error estimate, converged (a measurement
/// aid for the benchmarks).
fn trace<S: Scalar>(method: &str, dims: usize, calls: u64, o: &Outcome<S>) {
    static PATH: std::sync::OnceLock<Option<std::ffi::OsString>> = std::sync::OnceLock::new();
    if let Some(p) = PATH.get_or_init(|| std::env::var_os("MC_QUAD_TRACE")) {
        use std::io::Write;
        let line = match o {
            Outcome::Done { value, err, converged } => {
                let v = match value.to_num() {
                    Num::Real(r) => format!("{:e}", r.to_f64()),
                    other => format!("{other:?}"),
                };
                format!("{method} {dims} {calls} {v} {err:e} {converged}")
            }
            Outcome::NotNumeric(_) => format!("{method} {dims} {calls} not_numeric"),
        };
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(p) {
            let _ = writeln!(f, "{line}");
        }
    }
}

/// Print `text` as a message of `n_integrate` (the engine's `message`: scripts print `n_integrate: text`,
/// notebooks a message).
fn message<'h>(cx: &mut NativeCx<'_, '_, 'h>, text: &str) -> Res<()> {
    let h = cx.heap();
    cx.eval(h.call(h.sym("message"), &[h.sym_term(h.sym(NAME)), h.string(text)]))?;
    Ok(())
}

/// A multiple (or single) integral by a multidimensional method; `auto`: chosen automatically, so a point where
/// the integrand is not real, or (in 2 or 3 variables) a missed goal, also tries the nested one-dimensional rules.
#[allow(clippy::too_many_arguments)]
fn multi_call<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    ar: &Arith<'h, Heap>,
    f: T<'h>,
    vars: &[Sym],
    points: &[Vec<Point<'h>>],
    opts: &Options,
    kind: MKind,
    auto: bool,
) -> Res<Reply<'h>> {
    let h = cx.heap();
    if vars.len() > kind.max_dims() {
        return Ok(bad_argument(h, h.int(kind.max_dims() as i64 + 2)));
    }
    let nopts = mc_numfn::Options { policy: opts.policy(), ..mc_numfn::Options::default() };
    let ranges = bounds(cx, vars, points, &nopts)?;
    let g = multi::integrand(cx, f, vars, opts.policy())?;
    if g.outputs() != 1 {
        return Ok(bad_argument(h, h.int(1)));
    }
    let mut est = match multi::run(cx, g, ranges, kind, opts)? {
        Ok(e) => e,
        Err(pt) => {
            if auto && vars.len() <= MAX_RANGES {
                // complex values, or a failure the nested rules report with its point
                return machine_call(cx, ar, f, vars, points, opts);
            }
            let nums: Vec<Num> = pt.iter().map(|&x| Num::Real(Real::Machine(x))).collect();
            return Ok(Reply::Done(failure(h, "not_numeric", &[f, rules(h, ar, vars, &nums)])));
        }
    };
    trace(
        kind.name(),
        vars.len(),
        est.calls,
        &Outcome::Done { value: est.value, err: est.err, converged: est.converged },
    );
    let mut kind = kind;
    if auto && kind == MKind::Lattice && !est.converged {
        // a smooth-looking integrand the lattice rule did not resolve (a peak, a singularity): VEGAS with the same
        // budget, keeping the smaller error estimate
        let ranges = bounds(cx, vars, points, &nopts)?;
        let g = multi::integrand(cx, f, vars, opts.policy())?;
        if let Ok(v) = multi::run(cx, g, ranges, MKind::Vegas, opts)? {
            trace("vegas", vars.len(), v.calls, &Outcome::Done { value: v.value, err: v.err, converged: v.converged });
            if v.err < est.err {
                est = v;
                kind = MKind::Vegas;
            }
        }
    }
    if auto && kind == MKind::Cubature && !est.converged && vars.len() <= 3 {
        // the nested rules handle singular inner integrals better; keep the better estimate
        if let Outcome::Done { value, err, converged } = real_outcome(cx, f, vars, points, opts)? {
            if err < est.err {
                est = multi::Estimate { value, err, converged, worst: None, ..est };
            }
        }
    }
    let usable = multi::usable(&est, kind);
    if usable {
        for m in multi::messages(h, &est, kind, vars) {
            message(cx, &m)?;
        }
    }
    let v = scalar_term(ar, est.value);
    Ok(Reply::Done(if !usable {
        failure(h, "not_converged", &[h.sym_term(h.sym(NAME)), v, h.real(est.err)])
    } else if kind.statistical() || opts.error_estimate {
        multi::around(h, est.value, est.err)
    } else {
        v
    }))
}

/// The plan of the nested one-dimensional rules (`max_points` caps the subdivisions of each level).
fn nested_plan(opts: &Options) -> Plan {
    let mut max_subdivisions = opts.max_subdivisions();
    if let Some(n) = opts.max_points {
        // a subdivision evaluates 30 points (two 15-point intervals)
        max_subdivisions = max_subdivisions.min((n / 30).max(1) as usize);
    }
    Plan {
        method: opts.method.filter(|m| matches!(m, Method::GaussKronrod | Method::DoubleExponential)),
        goals: opts.machine_goals(),
        limits: gk::Limits { max_subdivisions, max_recursion: opts.max_recursion() },
    }
}

/// The real machine-precision integral by nested one-dimensional rules.
fn real_outcome<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    f: T<'h>,
    vars: &[Sym],
    points: &[Vec<Point<'h>>],
    opts: &Options,
) -> Res<Outcome<f64>> {
    let all: Vec<Var> = vars.iter().map(|&s| Var::scalar(s)).collect();
    let nopts = mc_numfn::Options { policy: opts.policy(), ..mc_numfn::Options::default() };
    let real = mc_numfn::compile_expr(cx, f, &all, &nopts)?;
    if real.outputs() != 1 {
        return Ok(Outcome::NotNumeric(vec![]));
    }
    let bounds = bounds(cx, vars, points, &nopts)?;
    let mut p = Problem { f: real, ranges: bounds, plan: nested_plan(opts), calls: 0 };
    let o = p.solve(cx)?;
    trace("nested", vars.len(), p.calls, &o);
    Ok(o)
}

/// Does `t` contain one of the symbols `syms`? Each shared subterm is visited once.
fn mentions<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, syms: &[Sym]) -> Res<bool> {
    let h = cx.heap();
    let mut seen = HashSet::new();
    let mut stack = vec![t];
    while let Some(u) = stack.pop() {
        if !seen.insert(u) {
            continue;
        }
        if seen.len().is_multiple_of(1024) {
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

/// The machine-precision integral: real first; complex (interpreted) when a value is complex.
fn machine_call<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    ar: &Arith<'h, Heap>,
    f: T<'h>,
    vars: &[Sym],
    points: &[Vec<Point<'h>>],
    opts: &Options,
) -> Res<Reply<'h>> {
    let h = cx.heap();
    let all: Vec<Var> = vars.iter().map(|&s| Var::scalar(s)).collect();
    let nopts = mc_numfn::Options { policy: opts.policy(), ..mc_numfn::Options::default() };
    let plan = nested_plan(opts);
    let real = mc_numfn::compile_expr(cx, f, &all, &nopts)?;
    if real.outputs() != 1 {
        return Ok(bad_argument(h, h.int(1)));
    }
    let bounds = bounds(cx, vars, points, &nopts)?;
    let mut p = Problem { f: real, ranges: bounds, plan, calls: 0 };
    let outcome = p.solve(cx)?;
    trace("nested", vars.len(), p.calls, &outcome);
    if let Outcome::NotNumeric(pt) = &outcome {
        // a complex value there: the whole integral in complex machine numbers
        if pt.len() == vars.len() && complex_at(cx, f, &all, pt)? {
            let cf: Box<dyn NumFn<'h, C64> + 'h> = Box::new(Interpreted::new(cx, f, &all, Prep::Machine)?);
            let bounds = self::bounds(cx, vars, points, &nopts)?;
            let mut p = Problem { f: cf, ranges: bounds, plan, calls: 0 };
            let outcome = p.solve(cx)?;
            return reply(cx, ar, f, vars, outcome, opts);
        }
    }
    reply(cx, ar, f, vars, outcome, opts)
}

/// Estimates whose error estimate is below this fraction of their magnitude are returned with a warning when the
/// goals are not met (semantics 10: a value with a warning is still the value); worse ones are failures. Divergent
/// integrals can show deceptively small estimates (about 1e-4 for 1/x on [1, inf)), hence the tight bound.
const USABLE: f64 = 1e-6;

/// The reply for a machine outcome; an unconverged but usable estimate prints a warning and is the value.
fn reply<'h, S: Scalar>(
    cx: &mut NativeCx<'_, '_, 'h>,
    ar: &Arith<'h, Heap>,
    f: T<'h>,
    vars: &[Sym],
    o: Outcome<S>,
    opts: &Options,
) -> Res<Reply<'h>> {
    let h = cx.heap();
    // `error_estimate: true`: the value with its error estimate
    let shown = |value: S, err: f64| match value.to_num() {
        Num::Real(r) if opts.error_estimate => multi::around(h, r.to_f64(), err),
        _ => scalar_term(ar, value),
    };
    if let Outcome::Done { value, err, converged: false, .. } = &o {
        let mag = value.abs();
        if err.is_finite() && mag.is_finite() && *err <= USABLE * mag {
            let msg = format!(
                "the goals were not met; the estimate's relative error is about {:.1e} (pass precision_goal: or accuracy_goal: to accept it silently)",
                err / mag
            );
            // a warning (the engine's `message`): scripts print it as `n_integrate: text`, notebooks as a message
            message(cx, &msg)?;
            return Ok(Reply::Done(shown(*value, *err)));
        }
    }
    if let Outcome::Done { value, err, converged: true } = &o {
        return Ok(Reply::Done(shown(*value, *err)));
    }
    Ok(Reply::Done(result(h, ar, f, vars, o)))
}

/// Is `f` a finite complex number at the point?
fn complex_at<'h>(cx: &mut NativeCx<'_, '_, 'h>, f: T<'h>, vars: &[Var], pt: &[f64]) -> Res<bool> {
    let mut c = Interpreted::new(cx, f, vars, Prep::Machine)?;
    let args: Vec<C64> = pt.iter().map(|&x| C64::real(x)).collect();
    let mut y = [C64::real(0.0)];
    Ok(NumFn::<C64>::eval(&mut c, cx, &args, &mut y)? && y[0].im != 0.0)
}

/// The machine bounds of every range: constants, or functions of the outer variables.
fn bounds<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    vars: &[Sym],
    points: &[Vec<Point<'h>>],
    nopts: &mc_numfn::Options,
) -> Res<Vec<Vec<Bound<'h>>>> {
    let mut out = Vec::with_capacity(points.len());
    for (k, ps) in points.iter().enumerate() {
        let outer: Vec<Var> = vars[..k].iter().map(|&s| Var::scalar(s)).collect();
        let mut bs = Vec::with_capacity(ps.len());
        for p in ps {
            bs.push(match *p {
                Point::Inf(s) => Bound::Const(End::Inf(s)),
                Point::Num(_, x) => Bound::Const(End::At(x)),
                Point::Expr(t) => Bound::Expr(mc_numfn::compile_expr(cx, t, &outer, nopts)?),
            });
        }
        out.push(bs);
    }
    Ok(out)
}

/// The value of a machine outcome: the number, or `failure(not_converged, n_integrate, estimate, error)`, or
/// `failure(not_numeric, f, [x -> point, ...])`.
fn result<'h, S: Scalar>(h: &'h Heap, ar: &Arith<'h, Heap>, f: T<'h>, vars: &[Sym], o: Outcome<S>) -> T<'h> {
    match o {
        Outcome::Done { value, converged: true, .. } => scalar_term(ar, value),
        Outcome::Done { value, err, .. } => {
            failure(h, "not_converged", &[h.sym_term(h.sym(NAME)), scalar_term(ar, value), h.real(err)])
        }
        Outcome::NotNumeric(pt) => {
            let nums: Vec<Num> = pt.iter().map(|&x| Num::Real(Real::Machine(x))).collect();
            failure(h, "not_numeric", &[f, rules(h, ar, vars, &nums)])
        }
    }
}

/// A real number as a ball at precision `p`; `None` if it is not real.
fn ball_of(n: &Num, p: u64) -> Option<Ball> {
    match n {
        Num::Real(r) => match r.to_kind(Kind::Ball, p) {
            Real::Ball(b, _) => Some(b),
            _ => None,
        },
        Num::Complex(..) => None,
    }
}

/// The least precision in decimal digits of the balls among the ends and the integrand's numbers, when there is a
/// ball and no machine real; `None` otherwise.
fn ball_digits<'h>(h: &'h Heap, ar: &Arith<'h, Heap>, f: T<'h>, ends: &[Point<'h>]) -> Option<u64> {
    let mut bits: Option<u64> = None;
    let mut machine = false;
    let mut note = |t: T<'h>| match ar.num(t) {
        Some(mc_arith::Num::Real(mc_arith::Real::Ball(_, p))) => bits = Some(bits.map_or(p, |b| b.min(p))),
        Some(mc_arith::Num::Complex(a, b)) => {
            for r in [a, b] {
                match r {
                    mc_arith::Real::Ball(_, p) => bits = Some(bits.map_or(p, |q| q.min(p))),
                    mc_arith::Real::Machine(_) => machine = true,
                    mc_arith::Real::Exact(_) => {}
                }
            }
        }
        Some(mc_arith::Num::Real(mc_arith::Real::Machine(_))) => machine = true,
        _ => {}
    };
    for e in ends {
        match *e {
            Point::Num(t, _) => note(t),
            Point::Expr(_) => return None,
            Point::Inf(_) => {}
        }
    }
    let mut stack = vec![f];
    let mut seen = 0usize;
    while let Some(u) = stack.pop() {
        seen += 1;
        if seen > 100_000 {
            return None;
        }
        match h.view(u) {
            View::Real(_) | View::Ball { .. } => note(u),
            View::App { head, args } => {
                if matches!(h.view(head), View::Sym(_)) && ar.num(u).is_some() {
                    note(u);
                    continue;
                }
                stack.push(head);
                stack.extend_from_slice(args);
            }
            _ => {}
        }
    }
    if machine {
        return None;
    }
    // bits to whole decimal digits (rounded down), at least 17 (else machine precision serves)
    let d = (bits? as f64 * std::f64::consts::LOG10_2).floor() as u64;
    (d > 16 && d <= options::MAX_DIGITS).then_some(d)
}

/// `2^e` as a machine real (an error estimate), clamped to the machine range.
fn pow2(e: f64) -> f64 {
    e.clamp(-1074.0, 1023.0).exp2()
}

/// The integral under `digits:` (and `rigorous:`): one range, the tanh-sinh family on balls.
fn ball_call<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    ar: &Arith<'h, Heap>,
    f: T<'h>,
    vars: &[Sym],
    points: &[Vec<Point<'h>>],
    opts: &Options,
) -> Res<Reply<'h>> {
    let h = cx.heap();
    let d = opts.digits.unwrap_or(options::RIGOROUS_DIGITS);
    if points.len() != 1 {
        // multiple integrals run at machine precision only
        return Ok(bad_argument(h, h.sym_term(h.sym(if opts.digits.is_some() { "digits" } else { "rigorous" }))));
    }
    if opts.method == Some(Method::GaussKronrod) {
        return Ok(bad_argument(h, h.sym_term(h.sym("method"))));
    }
    let (p, p0) = opts.ball_precision(d);
    let work = d + options::GUARD_DIGITS;
    // the ends as balls of the working precision
    let mut ends = Vec::with_capacity(points[0].len());
    for pt in &points[0] {
        ends.push(match *pt {
            Point::Inf(s) => hp::BEnd::Inf(s),
            Point::Num(t, _) => {
                let v = cx.eval(h.call(h.sym("n"), &[t, h.int(work as i64)]))?;
                match ar.num(v).and_then(|n| ball_of(&n, p)) {
                    Some(b) => hp::BEnd::At(b),
                    None => return Ok(bad_argument(h, h.int(2))),
                }
            }
            Point::Expr(_) => return Ok(bad_argument(h, h.int(2))),
        });
    }
    let all = [Var::scalar(vars[0])];
    // the far nodes of a half-line: exp of a ball below -2^40 as a tiny ball, not a failure
    let f_tail = if points[0].iter().any(|p| matches!(p, Point::Inf(_))) { tail_safe(h, f) } else { f };
    let mut fb = Interpreted::new(cx, f_tail, &all, Prep::Digits(work))?;
    let goals = opts.ball_goals_log2(d);
    // an oscillating infinite tail: the oscillatory method on balls first (as at machine precision)
    let mut osc = None;
    if !opts.rigorous && ends.iter().any(|e| matches!(e, hp::BEnd::Inf(_))) {
        let mut fm = Interpreted::new(cx, f, &all, Prep::Machine)?;
        osc = crate::oscb::integrate(cx, &mut fm, &mut fb, &ends, p, goals, hp::max_level(d))?;
    }
    let estimate = match osc {
        Some(o) => o,
        None => hp::integrate(cx, &mut fb, &ends, p, goals, hp::max_level(d))?,
    };
    let (value, err_log2, converged) = match estimate {
        hp::BallOutcome::NotNumeric(x) => {
            return Ok(Reply::Done(failure(h, "not_numeric", &[f, rules(h, ar, vars, &[x])])));
        }
        hp::BallOutcome::Done { value, err_log2, converged } => (value, err_log2, converged),
    };
    let approx = hp::widen(&value, err_log2, p);
    if !opts.rigorous {
        let shown = ar.number(&hp::round(&approx, p0));
        if !converged {
            return Ok(Reply::Done(failure(h, "not_converged", &[h.sym_term(h.sym(NAME)), shown, h.real(pow2(err_log2))])));
        }
        return Ok(Reply::Done(shown));
    }
    // the guaranteed ball: Clenshaw–Curtis with Bernstein-ellipse bounds, aiming at the estimate's accuracy
    let target = hp::log2_abs(&value) + goals.1;
    let mut fr = Interpreted::new(cx, f, &all, Prep::Digits(work))?;
    match rigorous::integrate(cx, &mut fr, f, &ends, p, target)? {
        rigorous::Verdict::Enclosure(b) => Ok(Reply::Done(ar.number(&hp::round(&b, p0)))),
        rigorous::Verdict::Unverified => {
            Ok(Reply::Done(failure(h, "not_verified", &[h.sym_term(h.sym(NAME)), ar.number(&hp::round(&approx, p0))])))
        }
    }
}

/// The name of the integrator-private exponential for far tails.
pub(crate) const TAIL_EXP: &str = "quad$tail_exp";

/// `quad$tail_exp(u)`: `exp(u)`, except that a real ball `u` below -2^40 gives the ball `0 +- 2^-(2^40)` (which
/// contains `exp(u)`, as `exp(u) < 2^(-1.44 * 2^40)`) instead of no value. The generic `exp` stays as it is (round 20
/// tried an enclosure of that kind there and reverted it, because a radius that small in a sum such as
/// `100 - exp(-exp(100))` claims millions of digits that the sum does not have); here only the integrand of the
/// ball integrator at the far nodes of a half-line uses it, where such a term is below every precision asked, and
/// the radius of the integral is the error estimate of the rule, not the radii of its terms.
pub(crate) struct TailExp;

impl Native for TailExp {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let [u] = args else { return Ok(Reply::Decline) };
        let ar = Arith::new(h);
        if let Some(Num::Real(Real::Ball(b, p))) = ar.num(*u) {
            // the whole ball below -2^40: midpoint and its radius
            if !b.mid.is_zero() && b.mid.top() >= 41 && b.mid.neg && b.rad.log2() < b.mid.top() as f64 - 2.0 {
                let zero = Ball { mid: Ball::from_i64(0).mid, rad: Mag::pow2(-(1i64 << 40)) };
                return Ok(Reply::Done(ar.number(&Num::Real(Real::Ball(zero, p)))));
            }
        }
        // a number: the generic exp; anything else stays as it is (the call is evaluated at the nodes)
        if ar.num(*u).is_none() {
            return Ok(Reply::Decline);
        }
        Ok(Reply::Done(cx.eval(h.call(h.sym("exp"), &[*u]))?))
    }
}

/// `f` with every `exp(u)` and `e^u` replaced by `quad$tail_exp(u)`.
pub(crate) fn tail_safe<'h>(h: &'h Heap, t: T<'h>) -> T<'h> {
    let View::App { head, args } = h.view(t) else { return t };
    let new: Vec<T<'h>> = args.iter().map(|&a| tail_safe(h, a)).collect();
    let (exp, pow, e) = (h.sym_term(h.sym("exp")), h.sym_term(h.sym(names::POW)), h.sym_term(h.sym("e")));
    if head == exp && new.len() == 1 {
        return h.call(h.sym(TAIL_EXP), &new);
    }
    if head == pow && new.len() == 2 && new[0] == e {
        return h.call(h.sym(TAIL_EXP), &new[1..]);
    }
    if new.as_slice() == args {
        t
    } else {
        h.app(head, &new)
    }
}
