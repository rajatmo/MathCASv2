//! The native of `integrate`: options, ranges, lists, the hand-back to the evaluator (moved from `mc-calculus`'s
//! natives in round 34).
use mc_api::{names, Store, View, T};
use mc_arith::num::StopScope;
use mc_calculus::common::{named_failure, variable};
use mc_calculus::term::Build;
use mc_core::Heap;
use mc_eval::{Native, NativeCx, Reply, Res};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// `integrate(f, x)`: an antiderivative; `integrate(f, [x, a, b])`: the definite integral. A list `f` integrates
/// elementwise. When no method applies the call stays as it is.
pub struct Integrate {
    /// the flag `mc-arith` polls in long numeric loops
    pub stop: Arc<AtomicBool>,
}

impl Native for Integrate {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        // `generated_parameters: c`: an antiderivative gets the constant of integration c(1) (item 14A)
        if let Some((&last, rest @ [_, spec])) = args.split_last() {
            if let Some(c) = generated_parameter(h, last) {
                let r = cx.eval(h.call(h.sym("integrate"), rest))?;
                let indefinite = !matches!(h.view(*spec), View::App { head, .. } if head == h.sym_term(h.sym(names::LIST)));
                if matches!(h.view(r), View::App { head, .. } if head == h.sym_term(h.sym("integrate"))) || !indefinite {
                    return Ok(if indefinite { Reply::Decline } else { Reply::Done(r) });
                }
                return Ok(Reply::Again(h.call(h.sym(names::ADD), &[h.app(c, &[h.int(1)]), r])));
            }
        }
        // `working_precision: p` only matters to numeric methods: a symbolic integral is exact, so the option is
        // consumed (it must not remain as a factor of the result)
        if let Some((&last, rest)) = args.split_last() {
            if let View::App { head, args: rules } = h.view(last) {
                let key = h.sym_term(h.sym("working_precision"));
                let is_wp = |r: &T<'h>| matches!(h.view(*r), View::App { args: &[k, _], .. } if k == key);
                if head == h.sym_term(h.sym("options")) && rules.iter().any(is_wp) {
                    let mut call = rest.to_vec();
                    let kept: Vec<T<'h>> = rules.iter().copied().filter(|r| !is_wp(r)).collect();
                    if !kept.is_empty() {
                        call.push(h.call(h.sym("options"), &kept));
                    }
                    return Ok(Reply::Again(h.call(h.sym("integrate"), &call)));
                }
            }
        }
        // the options of definite integration (item 14D): `generate_conditions`, `principal_value`,
        // `assumptions`; the options term is passed on to inner and outer integrals
        let (args, opt_term, dopts) = match args.split_last() {
            Some((&last, rest)) => match crate::definite::options(h, last) {
                Some((o, a)) => {
                    if let Some(a) = a {
                        // `assumptions: a`: the integral evaluated under `a` (the option removed)
                        let mut call = rest.to_vec();
                        if let Some(t) = crate::definite::options_term(h, &o) {
                            call.push(t);
                        }
                        return Ok(Reply::Done(cx.eval_assuming(&[a], h.call(h.sym("integrate"), &call))?));
                    }
                    (rest, Some(last), o)
                }
                None => (args, None, crate::definite::Opts::default()),
            },
            None => (args, None, crate::definite::Opts::default()),
        };
        let with_opts = |mut v: Vec<T<'h>>| {
            if let Some(o) = opt_term {
                v.push(o);
            }
            h.call(h.sym("integrate"), &v)
        };
        // several specifications: iterated, the last one innermost (decision IOSYM.9)
        if let [f, specs @ ..] = args {
            if specs.len() > 1 {
                let inner = cx.eval(with_opts(vec![*f, specs[specs.len() - 1]]))?;
                if matches!(h.view(inner), View::App { head, .. } if head == h.sym_term(h.sym("integrate"))) {
                    return Ok(Reply::Decline);
                }
                let mut outer = vec![inner];
                outer.extend_from_slice(&specs[..specs.len() - 1]);
                return Ok(Reply::Again(with_opts(outer)));
            }
        }
        let [f, spec] = args else { return Ok(Reply::Decline) };
        // sinc(u) is sin(u)/u away from u = 0, where its value 1 changes no integral: integrate that instead
        if let Some(g) = without_sinc(h, *f) {
            return Ok(Reply::Again(with_opts(vec![g, *spec])));
        }
        let _scope = StopScope::enter(Some(&self.stop));
        let b = Build::new(h, &self.stop);
        // [x, a, b]: a definite integral
        // an integral over a region, `element([x, y], reg)`: the region integrator
        if matches!(h.view(*spec), View::App { head, args: [_, _] } if b.sym_of(head) == Some(h.sym("element"))) {
            return Ok(Reply::Again(h.call(h.sym("region_integrate"), &[*f, *spec])));
        }
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
            Some((_, lo, hi)) => crate::definite::integrate(cx, &b, &self.stop, f, x, lo, hi, dopts),
            None => match crate::definite::delta::antiderivative(cx, &b, f, x)? {
                Some(r) => Ok(Some(r)),
                None => crate::antiderivative(cx, &b, f, x, 0),
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
                use crate::definite::sign;
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
            // nested lists (a matrix): element by element through the evaluator
            View::App { head, args: items }
                if b.sym_of(head) == Some(b.hd.list)
                    && items.iter().any(|&g| matches!(h.view(g), View::App { head, .. } if b.sym_of(head) == Some(b.hd.list))) =>
            {
                let out: Vec<T<'h>> = items.iter().map(|&g| with_opts(vec![g, *spec])).collect();
                return Ok(Reply::Again(h.call(b.hd.list, &out)));
            }
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

/// `t` with every `sinc(u)` written as `sin(u)/u`, or `None` when it has none.
fn without_sinc<'h>(h: &'h Heap, t: T<'h>) -> Option<T<'h>> {
    let sinc = h.sym_term(h.sym("sinc"));
    fn walk<'h>(h: &'h Heap, sinc: T<'h>, t: T<'h>) -> Option<T<'h>> {
        let View::App { head, args } = h.view(t) else { return None };
        if head == sinc && args.len() == 1 {
            let u = walk(h, sinc, args[0]).unwrap_or(args[0]);
            let sin = h.call(h.sym("sin"), &[u]);
            return Some(h.call(h.sym(names::MUL), &[sin, h.call(h.sym(names::POW), &[u, h.int(-1)])]));
        }
        let new_head = walk(h, sinc, head);
        let new_args: Vec<Option<T<'h>>> = args.iter().map(|&a| walk(h, sinc, a)).collect();
        if new_head.is_none() && new_args.iter().all(Option::is_none) {
            return None;
        }
        let args: Vec<T<'h>> = args.iter().zip(new_args).map(|(&a, n)| n.unwrap_or(a)).collect();
        Some(h.app(new_head.unwrap_or(head), &args))
    }
    walk(h, sinc, t)
}

/// The `c` of `options(generated_parameters -> c)` (that option alone).
fn generated_parameter<'h>(h: &'h Heap, t: T<'h>) -> Option<T<'h>> {
    let View::App { head, args: [r] } = h.view(t) else { return None };
    if head != h.sym_term(h.sym("options")) {
        return None;
    }
    match h.view(*r) {
        View::App { args: &[k, v], .. } if k == h.sym_term(h.sym("generated_parameters")) => Some(v),
        _ => None,
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
