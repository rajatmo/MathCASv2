//! The driver of indefinite integration (roadmap phase 14): a portfolio of methods, each with its own time
//! budget, and a check of every answer by differentiation.
//!
//! 1. **Quick table**: linearity, the base table, the rational integrator (Hermite reduction and
//!    Lazard–Rioboo–Trager), one function of a linear argument, the derivative of an unknown function.
//! 2. **The tower-based engine** (`mc-integrate`): the Risch decision procedure for towers of height one, which
//!    answers or proves that no elementary antiderivative exists; then the parallel Risch–Norman ansatz over the
//!    tower with the special functions as candidates (`erf`, `Ei`, `li`, `Si`, `Ci`, Fresnel, incomplete gamma,
//!    elliptic integrals).
//! 3. **The elementary rules** (substitution, parts, trigonometric reductions): with the full budget when the engine
//!    found nothing, with a short one otherwise; the smaller of the two answers (by `mc-simplify`'s cost) is kept,
//!    the established form on a tie.
//!
//! Every answer is checked before it is returned: `d(F)/dx - f` evaluated at three points (and the parameters at
//! fixed rational values) to 30 digits must vanish; when that is inconclusive, `simplify(d(F)/dx - f)` must be 0.
//! An answer that fails is dropped and the next method runs. A proof of non-elementarity from the decision
//! procedure leaves the integral unevaluated when no special-function answer is found; `risch_integrate` reports it.
//!
//! **In parallel** (roadmap item 16A, decision n16a.4): with more than one thread the methods after the quick table
//! run at once on the session's pool, each from a clean state (a fresh budget of the elementary rules, no
//! deadline left over) and each checking its own answer, and their answers are combined exactly as the sequence
//! above combines them: the same answer for any number of threads. The run is cut as soon as the answers in order
//! settle it (a proof of non-elementarity cancels the elementary rules; an answer of the engine cancels the rule
//! table, which only runs when the engine has none). The elementary rules start with the short budget; when the
//! engine found nothing they are run again with the full one if the short one ran out, as the sequence would
//! have given them the full one from the start. The three points of a check are evaluated in parallel too.
use super::{args_of, base, classic, elementary, is_free, rational};
use mc_calculus::term::Build;
use mc_api::{Store, View, T};
use mc_arith::Num;
use mc_eval::{NativeCx, Res};
use rustc_hash::FxHashSet;
use std::time::Duration;

/// The time of the decision procedure.
pub const RISCH_TIME: Duration = Duration::from_millis(1500);
/// The time of the ansatz.
pub const NORMAN_TIME: Duration = Duration::from_millis(2500);
/// The time of the elementary rules when nothing else answered.
pub const ELEMENTARY_TIME: Duration = Duration::from_millis(4000);
/// The time of the elementary rules after the engine answered.
pub const ELEMENTARY_SHORT: Duration = Duration::from_millis(600);

/// The points of the numerical check, inside (0, 1), and the values of the parameters.
const POINTS: [(i64, i64); 3] = [(3, 7), (5, 11), (7, 13)];
const PARAMS: [(i64, i64); 6] = [(4, 3), (7, 5), (9, 7), (11, 8), (13, 9), (17, 12)];

/// An antiderivative of `f` by the portfolio, checked; `None` leaves the integral unevaluated.
pub fn integrate<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, x: T<'h>) -> Res<Option<T<'h>>> {
    if is_free(b, f, x) {
        return Ok(Some(b.mul(&[f, x])));
    }
    // x^(2^31) times a factor the table does not know: no method can expand it, and the quick table would try the
    // tabular parts (a 240 GB allocation, X23-9; 60 s, X23-10); a monomial alone is the table's
    if super::has_huge_power(b, f) && !is_monomial(b, f, x) {
        return Ok(None);
    }
    if let Some(r) = quick(cx, b, f, x)? {
        if verified(cx, b, f, r, x)? {
            return Ok(Some(r));
        }
    }
    if cx.parallel() {
        return in_parallel(cx, b, f, x);
    }
    let mut found: Vec<T<'h>> = vec![];
    let Out::Engine(r, proven) = method(cx, b, Method::Engine, f, x)? else { unreachable!("the engine's answer") };
    found.extend(r);
    // integrands with roots (14B): substitutions, Hermite reduction, Chebyshev's theorem, Legendre's forms
    let Out::Algebraic(alg) = method(cx, b, Method::Algebraic, f, x)? else { unreachable!("the algebraic answer") };
    found.extend(alg.found);
    // the rule table (mc-integrate-rules) for what the engine does not reach, before the elementary rules
    if found.is_empty() {
        let Out::Found(r, _) = method(cx, b, Method::Rules, f, x)? else { unreachable!("an answer") };
        found.extend(r);
    }
    // no elementary antiderivative (proven): the elementary rules cannot find one
    if proven || alg.proof.is_some() {
        return Ok(smallest(b, found));
    }
    let m = if found.is_empty() { Method::Elementary(ELEMENTARY_TIME) } else { Method::Elementary(ELEMENTARY_SHORT) };
    let Out::Found(r, _) = method(cx, b, m, f, x)? else { unreachable!("an answer") };
    found.extend(r);
    Ok(smallest(b, found))
}

/// A method of the portfolio after the quick table.
#[derive(Clone, Copy, Debug)]
enum Method {
    /// the tower-based engine
    Engine,
    /// the algebraic stage (integrands with roots)
    Algebraic,
    /// the rule table
    Rules,
    /// the elementary rules under this budget
    Elementary(Duration),
}

/// What a method found: checked answers (and for the engine, whether it proved that none is elementary; for the
/// elementary rules, whether their budget ran out).
enum Out<'h> {
    Engine(Option<T<'h>>, bool),
    Algebraic(elementary::algebraic::Answer<'h>),
    Found(Option<T<'h>>, bool),
}

/// Run one method from a clean state (a fresh budget of the elementary rules, no deadline), its answer checked.
fn method<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, m: Method, f: T<'h>, x: T<'h>) -> Res<Out<'h>> {
    let saved = elementary::fresh_budget();
    elementary::set_deadline(None);
    let r = run_method(cx, b, m, f, x);
    elementary::set_deadline(None);
    elementary::restore_budget(saved);
    r
}

fn run_method<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, m: Method, f: T<'h>, x: T<'h>) -> Res<Out<'h>> {
    Ok(match m {
        Method::Engine => {
            let (r, proven) = engine(cx, b, f, x)?;
            let r = match r {
                Some(r) if verified(cx, b, f, r, x)? => Some(r),
                _ => None,
            };
            Out::Engine(r, proven)
        }
        Method::Algebraic => Out::Algebraic(elementary::algebraic::top(cx, b, f, x)?),
        Method::Rules => {
            let rules = mc_integrate_rules::integrate(cx, f, x, mc_integrate_rules::Limits::default(), &mut |cx, u, x| quick(cx, b, u, x))?;
            let r = match rules.answer {
                Some(r) if verified(cx, b, f, r, x)? => Some(r),
                _ => None,
            };
            Out::Found(r, false)
        }
        Method::Elementary(time) => {
            let t0 = std::time::Instant::now();
            elementary::set_deadline(Some(t0 + time));
            let r = classic(cx, b, f, x, 0)?;
            let late = t0.elapsed() >= time;
            elementary::set_deadline(None);
            let r = match r {
                Some(r) if verified(cx, b, f, r, x)? => Some(r),
                _ => None,
            };
            Out::Found(r, late)
        }
    })
}

/// The methods after the quick table at once (decision n16a.4), combined as [`integrate`] combines them.
fn in_parallel<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, x: T<'h>) -> Res<Option<T<'h>>> {
    // computed in this order; the elementary rules before the rule table, which an answer of the first two
    // makes unnecessary
    const ORDER: [Method; 4] = [Method::Engine, Method::Algebraic, Method::Elementary(ELEMENTARY_SHORT), Method::Rules];
    let stop = b.stop().clone();
    let (mut proven, mut any) = (false, false);
    let outs = cx.par_scan(
        ORDER.len(),
        &|_: &Out<'h>| false,
        &mut |o: &Out<'h>| match o {
            Out::Engine(r, p) => {
                proven = *p;
                any = r.is_some();
                false
            }
            // a proof ends the run; so does an answer of the first two once the elementary rules have run
            Out::Algebraic(a) => {
                any |= a.found.is_some();
                proven || a.proof.is_some()
            }
            Out::Found(..) => any,
        },
        &|cx, i| {
            let _scope = mc_arith::num::StopScope::enter(Some(&stop));
            let b = Build::new(cx.heap(), &stop);
            method(cx, &b, ORDER[i], f, x)
        },
    )?;
    let mut outs = outs.into_iter();
    let Some(Out::Engine(r, proven)) = outs.next() else { unreachable!("the engine's answer") };
    let mut found: Vec<T<'h>> = r.into_iter().collect();
    let Some(Out::Algebraic(alg)) = outs.next() else { unreachable!("the algebraic answer") };
    found.extend(alg.found);
    if proven || alg.proof.is_some() {
        return Ok(smallest(b, found));
    }
    let Some(Out::Found(short, late)) = outs.next() else { unreachable!("the elementary answer") };
    if found.is_empty() {
        let Some(Out::Found(r, _)) = outs.next() else { unreachable!("the rule table's answer") };
        found.extend(r);
    }
    let elementary = if found.is_empty() && late && short.is_none() {
        // the sequence gives the elementary rules the full budget when nothing else answered
        let Out::Found(r, _) = method(cx, b, Method::Elementary(ELEMENTARY_TIME), f, x)? else { unreachable!("an answer") };
        r
    } else {
        short
    };
    found.extend(elementary);
    Ok(smallest(b, found))
}

/// The smallest answer by `mc-simplify`'s cost; on a tie the later (the established elementary form).
fn smallest<'h>(b: &Build<'h>, found: Vec<T<'h>>) -> Option<T<'h>> {
    // the established (later) form stays unless an earlier one is clearly smaller: by a fifth of its cost
    let mut cost = mc_simplify::cost::Cost::new(b.heap());
    let mut best: Option<(u64, T<'h>)> = None;
    for t in found {
        let c = cost.size(t);
        if best.is_none_or(|(bc, _)| 4 * c <= 5 * bc) {
            best = Some((c, t));
        }
    }
    best.map(|(_, t)| t)
}

/// The quick table: linearity over the base table, the rational integrator, one function of a linear argument and
/// the derivative of an unknown function; all terms of a sum must be found.
fn quick<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, x: T<'h>) -> Res<Option<T<'h>>> {
    cx.tick(1)?;
    if is_free(b, f, x) {
        return Ok(Some(b.mul(&[f, x])));
    }
    if let Some(terms) = args_of(b, f, b.hd.add) {
        let mut out = Vec::with_capacity(terms.len());
        for &t in terms {
            match quick(cx, b, t, x)? {
                Some(r) => out.push(r),
                None => return Ok(None),
            }
        }
        return Ok(Some(b.add(&out)));
    }
    if let Some(fs) = args_of(b, f, b.hd.mul) {
        let (free, dep): (Vec<T<'h>>, Vec<T<'h>>) = fs.iter().partition(|&&g| is_free(b, g, x));
        if !free.is_empty() {
            let rest = b.mul(&dep);
            return Ok(quick(cx, b, rest, x)?.map(|r| b.mul(&[b.mul(&free), r])));
        }
    }
    if let Some(r) = base(b, f, x) {
        return Ok(Some(r));
    }
    if let Some(r) = unknown_derivative(b, f, x) {
        return Ok(Some(r));
    }
    if let Some(r) = rational::integrate(cx, b, f, x, 1)? {
        return Ok(Some(r));
    }
    elementary::table::single(cx, b, elementary::normalize(b, f), x)
}

/// `derivative(n)(g)(x)` integrates to `derivative(n - 1)(g)(x)` (`g(x)` for n = 1).
fn unknown_derivative<'h>(b: &Build<'h>, f: T<'h>, x: T<'h>) -> Option<T<'h>> {
    let h = b.heap();
    let View::App { head, args: [u] } = h.view(f) else { return None };
    if *u != x {
        return None;
    }
    let (orders, g) = b.as_derivative(head)?;
    let [n] = orders.as_slice() else { return None };
    if *n == 1 {
        return Some(h.app(g, &[x]));
    }
    Some(h.app(b.derivative(&[n - 1], g), &[x]))
}

/// The tower-based engine: the decision procedure, then the ansatz with special-function candidates; with the
/// answer, whether the decision procedure proved that no elementary antiderivative exists.
fn engine<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, x: T<'h>) -> Res<(Option<T<'h>>, bool)> {
    let g = elementary::normalize(b, f);
    match mc_integrate::risch(cx, &b.ar, g, x, RISCH_TIME)? {
        mc_integrate::Outcome::Found { part, rest } => {
            if b.ar.num(rest).is_some_and(|n| n.is_exact_zero()) {
                return Ok((Some(part), false));
            }
            if let Some(r) = classic(cx, b, rest, x, 1)? {
                return Ok((Some(b.add(&[part, r])), false));
            }
        }
        mc_integrate::Outcome::NonElementary(_) => return Ok((mc_integrate::norman(cx, &b.ar, g, x, NORMAN_TIME, true)?, true)),
        mc_integrate::Outcome::Unknown => {}
    }
    Ok((mc_integrate::norman(cx, &b.ar, g, x, NORMAN_TIME, true)?, false))
}

/// The decision procedure alone, for `risch_integrate`: the antiderivative, or the reason there is no elementary
/// one, or `None` (outside the procedure).
pub fn risch<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, x: T<'h>) -> Res<Option<Result<T<'h>, String>>> {
    let g = elementary::normalize(b, f);
    match mc_integrate::risch(cx, &b.ar, g, x, RISCH_TIME * 4)? {
        mc_integrate::Outcome::Found { part, rest } => {
            let r = if b.ar.num(rest).is_some_and(|n| n.is_exact_zero()) {
                Some(part)
            } else {
                rational::integrate(cx, b, rest, x, 1)?.map(|r| b.add(&[part, r]))
            };
            match r {
                Some(r) if verified(cx, b, f, r, x)? => Ok(Some(Ok(r))),
                _ => Ok(None),
            }
        }
        mc_integrate::Outcome::NonElementary(why) => Ok(Some(Err(why))),
        mc_integrate::Outcome::Unknown => Ok(elementary::algebraic::top(cx, b, f, x)?.proof.map(Err)),
    }
}

/// Is `F` an antiderivative of `f`? `d(F)/dx - f` vanishes at the points (numerically to 30 digits) or simplifies
/// to 0.
pub fn verified<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, big: T<'h>, x: T<'h>) -> Res<bool> {
    let h = b.heap();
    if !is_free_head(b, big, "integrate") {
        return Ok(false);
    }
    let Some(df) = elementary::deriv(cx, b, big, x)? else { return Ok(false) };
    let e = b.sub(df, f);
    if b.is_zero(e) {
        return Ok(true);
    }
    // the parameters: symbols other than x without a numerical value
    let mut params: Vec<T<'h>> = vec![];
    collect_symbols(b, e, x, &mut params);
    let mut rules: Vec<(T<'h>, T<'h>)> = vec![];
    for (k, &s) in params.iter().enumerate() {
        let v = cx.eval(h.call(h.sym("n"), &[s]))?;
        if b.ar.num(v).is_some() {
            continue;
        }
        let (p, q) = PARAMS[k % PARAMS.len()];
        rules.push((s, b.rat(p + k as i64 / PARAMS.len() as i64, q)));
    }
    // the points in parallel (roadmap item 16A), in order, until one is not a clear zero
    let stop = b.stop().clone();
    let small = cx.par_until(POINTS.len(), &|ok: &bool| !*ok, &|cx, i| {
        let _scope = mc_arith::num::StopScope::enter(Some(&stop));
        let b = Build::new(cx.heap(), &stop);
        let h = b.heap();
        let (p, q) = POINTS[i];
        let mut t = elementary::subst(&b, e, x, b.rat(p, q));
        let mut ft = elementary::subst(&b, f, x, b.rat(p, q));
        for &(s, v) in &rules {
            t = elementary::subst(&b, t, s, v);
            ft = elementary::subst(&b, ft, s, v);
        }
        let digits = b.int(30);
        let ev = cx.eval(h.call(h.sym("n"), &[t, digits]))?;
        let fv = cx.eval(h.call(h.sym("n"), &[ft, digits]))?;
        // a clear nonzero value, or no value: the symbolic check decides (a branch of a root or a logarithm may
        // differ at this point)
        Ok(matches!((magnitude(&b, ev), magnitude(&b, fv)), (Some(a), Some(s)) if a <= 1e-15 * (1.0 + s)))
    })?;
    let numeric = small.len() == POINTS.len() && small.iter().all(|&ok| ok);
    if numeric {
        return Ok(true);
    }
    let s = cx.eval(h.call(h.sym("simplify"), &[e]))?;
    Ok(b.ar.num(s).is_some_and(|n| n.is_exact_zero()))
}

/// The absolute value of a number term (real, ball or complex), as a float.
fn magnitude<'h>(b: &Build<'h>, t: T<'h>) -> Option<f64> {
    // an exact zero evaluates to failure(precision_not_reached, ball(0, r)): zero
    if let Some([tag, v]) = args_of(b, t, b.heap().sym(mc_api::names::FAILURE)) {
        if b.sym_of(*tag).map(|s| b.heap().sym_name(s)).as_deref() == Some("precision_not_reached") {
            return magnitude(b, *v);
        }
    }
    let v = match b.ar.num(t)? {
        Num::Real(r) => r.to_f64().abs(),
        Num::Complex(re, im) => re.to_f64().hypot(im.to_f64()),
    };
    v.is_finite().then_some(v)
}

/// The symbols of `t` in argument positions (not heads), other than `x`.
fn collect_symbols<'h>(b: &Build<'h>, t: T<'h>, x: T<'h>, out: &mut Vec<T<'h>>) {
    let h = b.heap();
    let mut seen: FxHashSet<T<'h>> = FxHashSet::default();
    let mut stack = vec![t];
    while let Some(u) = stack.pop() {
        if !seen.insert(u) {
            continue;
        }
        match h.view(u) {
            View::Sym(_) if u != x => {
                if !out.contains(&u) {
                    out.push(u);
                }
            }
            View::App { args, .. } => stack.extend_from_slice(args),
            _ => {}
        }
    }
}

/// Does `t` contain no call of `name`?
pub(crate) fn is_free_head<'h>(b: &Build<'h>, t: T<'h>, name: &str) -> bool {
    let h = b.heap();
    let s = h.sym_term(h.sym(name));
    let mut seen: FxHashSet<T<'h>> = FxHashSet::default();
    let mut stack = vec![t];
    while let Some(u) = stack.pop() {
        if !seen.insert(u) {
            continue;
        }
        if let View::App { head, args } = h.view(u) {
            if head == s {
                return false;
            }
            stack.push(head);
            stack.extend_from_slice(args);
        }
    }
    true
}

/// `risch_integrate(f, x)`: the decision procedure alone. The antiderivative when it is elementary,
/// `failure(nonelementary, risch_integrate, reason)` when the procedure proves that it is not, unevaluated when `f`
/// is outside the procedure (a tower of height more than one, algebraic functions, a residue polynomial it does not
/// solve).
pub struct RischIntegrate {
    /// the flag `mc-arith` polls in long numeric loops
    pub stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl mc_eval::Native for RischIntegrate {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<mc_eval::Reply<'h>> {
        let h = cx.heap();
        let [f, x] = args else { return Ok(mc_eval::Reply::Decline) };
        let _scope = mc_arith::num::StopScope::enter(Some(&self.stop));
        let b = Build::new(h, &self.stop);
        let x = match mc_calculus::common::variable(&b, *x) {
            Ok(x) => x,
            Err(message) => {
                let tags = [h.sym_term(h.sym("argument")), h.sym_term(h.sym("risch_integrate")), h.string(message)];
                return Ok(mc_eval::Reply::Done(h.call(h.sym(mc_api::names::FAILURE), &tags)));
            }
        };
        if is_free(&b, *f, x) {
            return Ok(mc_eval::Reply::Again(b.mul(&[*f, x])));
        }
        Ok(match risch(cx, &b, *f, x)? {
            Some(Ok(r)) => mc_eval::Reply::Again(r),
            Some(Err(why)) => {
                let tags = [h.sym_term(h.sym("nonelementary")), h.sym_term(h.sym("risch_integrate")), h.string(&why)];
                mc_eval::Reply::Done(h.call(h.sym(mc_api::names::FAILURE), &tags))
            }
            None => mc_eval::Reply::Decline,
        })
    }
}

/// Is `f` a constant times a power of `x`?
fn is_monomial<'h>(b: &Build<'h>, f: T<'h>, x: T<'h>) -> bool {
    let fs = match args_of(b, f, b.hd.mul) {
        Some(fs) => fs.to_vec(),
        None => vec![f],
    };
    fs.iter().filter(|&&g| !is_free(b, g, x)).all(|&g| g == x || matches!(args_of(b, g, b.hd.pow), Some(&[base, e]) if base == x && is_free(b, e, x)))
}
