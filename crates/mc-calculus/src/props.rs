//! Properties of functions (decision 2C.11): `function_domain`, `function_singularities`, `function_analytic`,
//! `function_sign`, `function_monotonicity`, `function_convexity`, `function_injective`.
//!
//! Each takes `f` or `[f, condition]`, a variable or a list of variables, and optionally a domain (`reals`, the
//! default, or `complexes`; `integers` for the sign and the monotonicity of a sequence); trailing options are
//! accepted and ignored.
//!
//! - **Domain**: the conditions under which every subterm is defined: bases of negative powers nonzero, of fractional
//!   powers with an even denominator nonnegative (positive under a negative power), arguments of `log` positive
//!   (nonzero over the complexes), of `asin`/`acos` in [-1, 1], `acosh` at least 1, `atanh` in (-1, 1), `cos(u) != 0`
//!   under `tan`/`sec`, `sin(u) != 0` under `cot`/`csc`. Over the reals with one variable the conjunction is given
//!   to `reduce`; otherwise it is the conjunction itself. A function this module does not know leaves the call
//!   unevaluated.
//! - **Singularities**: the negations of the domain conditions (`x <= 0` for `sqrt(x)`, `cos(x) == 0` for `tan(x)`);
//!   **analytic**: `true` when there are none (or the condition excludes them), `false` otherwise.
//! - **Sign, monotonicity, convexity** of the function, its derivative, its second derivative: `1` when the value
//!   is at least 0 everywhere on the domain, `-1` when at most 0, `0` when it vanishes, `indeterminate` otherwise.
//!   Decided by `refine` under the domain's facts when it can; otherwise, for one variable without parameters, by
//!   sampling: a change of sign among the samples is a proof of `indeterminate`, samples of one sign are taken as
//!   the answer (the sampling covers the domain from 10^-6 to 10^6 in both directions; decision 2C.12).
//! - **Injective**: strictly monotonic (by the derivative's sign) is `true`; two equal values found among the
//!   samples is `false`.
use crate::integrate::is_free;
use crate::term::Build;
use mc_api::{names, Store, View, T};
use mc_arith::num::StopScope;
use mc_eval::{Native, NativeCx, Reply, Res};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// The property a native computes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Prop {
    /// `function_domain`
    Domain,
    /// `function_singularities`
    Singularities,
    /// `function_analytic`
    Analytic,
    /// `function_sign`
    Sign,
    /// `function_monotonicity`
    Monotonicity,
    /// `function_convexity`
    Convexity,
    /// `function_injective`
    Injective,
}

/// The native of one property.
pub struct PropNative {
    /// the property
    pub p: Prop,
    /// the flag `mc-arith` polls in long numeric loops
    pub stop: Arc<AtomicBool>,
}

/// Functions defined (and analytic) for every real argument.
const ENTIRE: &[&str] = &[
    "sin", "cos", "exp", "sinh", "cosh", "tanh", "atan", "erf", "erfc", "erfi", "airy_ai", "airy_bi", "sech",
    "airy_ai_prime", "airy_bi_prime", "dawson_f", "fresnel_c", "fresnel_s", "sin_integral", "sinh_integral", "sinc",
    "gudermannian",
];
/// Functions of several arguments analytic in the last one for every real value of it, when the others are free of
/// the variables (polynomial families, entire hypergeometric functions; decision IOSYM.5).
const ENTIRE_LAST: &[&str] = &[
    "hermite_h", "laguerre_l", "chebyshev_t", "chebyshev_u", "lucas_l", "fibonacci", "hypergeometric_0f1",
    "hypergeometric_0f1_regularized", "hypergeometric_1f1_regularized",
];
/// Functions of an integer order and a variable analytic in the variable for an integer order.
const ENTIRE_INTEGER_ORDER: &[&str] = &["bessel_j", "bessel_i", "spherical_bessel_j"];
/// Functions defined for every real argument but not analytic everywhere.
const CONTINUOUS: &[&str] = &[
    "abs", "max", "min", "unit_step", "sign", "floor", "ceiling", "round", "re", "im_part", "conjugate", "integer_part",
    "fractional_part", "clip", "ramp", "real_abs", "real_sign", "unit_box", "unit_triangle", "heaviside_theta", "arg",
];

/// The call's pieces.
struct Call<'h> {
    f: T<'h>,
    cond: Option<T<'h>>,
    vars: Vec<T<'h>>,
    complex: bool,
    integers: bool,
}

fn parse<'h>(cx: &mut NativeCx<'_, '_, 'h>, args: &[T<'h>]) -> Option<Call<'h>> {
    let h = cx.heap();
    let sym = |t: T<'h>| match h.view(t) {
        View::Sym(s) => Some(h.sym_name(s).to_string()),
        _ => None,
    };
    let mut args = args.to_vec();
    if let Some(&last) = args.last() {
        if matches!(h.view(last), View::App { head, .. } if sym(head).as_deref() == Some("options")) {
            args.pop();
        }
    }
    let (fc, xs, dom) = match args.as_slice() {
        [f, x] => (*f, *x, None),
        [f, x, d] => (*f, *x, sym(*d)),
        _ => return None,
    };
    let (f, cond) = match cx.as_list(fc) {
        Some([f, c]) => (*f, Some(*c)),
        Some(_) => return None,
        None => (fc, None),
    };
    let vars = match cx.as_list(xs) {
        Some(v) => v.to_vec(),
        None => vec![xs],
    };
    if vars.iter().any(|v| sym(*v).is_none()) {
        return None;
    }
    let (complex, integers) = match dom.as_deref() {
        None | Some("reals") => (false, false),
        Some("complexes") => (true, false),
        Some("integers") => (false, true),
        Some(_) => return None,
    };
    Some(Call { f, cond, vars, complex, integers })
}

impl Native for PropNative {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let Some(mut c) = parse(cx, args) else { return Ok(Reply::Decline) };
        // inside assuming (or with assumptions: facts): the facts about the variables restrict the domain
        if c.cond.is_none() {
            let facts: Vec<T<'h>> = cx.assumptions().iter().copied().filter(|&f| c.vars.iter().any(|&x| !is_free(&Build::new(h, &self.stop), f, x))).collect();
            match facts.as_slice() {
                [] => {}
                [f] => c.cond = Some(*f),
                _ => c.cond = Some(h.call(h.sym("and"), &facts)),
            }
        }
        let _scope = StopScope::enter(Some(&self.stop));
        let b = Build::new(h, &self.stop);
        let r = match self.p {
            Prop::Domain => domain(cx, &b, &c)?,
            Prop::Singularities => singularities(cx, &b, &c)?,
            Prop::Analytic => analytic(cx, &b, &c)?,
            Prop::Sign | Prop::Monotonicity | Prop::Convexity => {
                let order = match self.p {
                    Prop::Sign => 0,
                    Prop::Monotonicity => 1,
                    _ => 2,
                };
                sign_of(cx, &b, &c, order)?
            }
            Prop::Injective => injective(cx, &b, &c)?,
        };
        Ok(r.map_or(Reply::Decline, Reply::Done))
    }
}

/// The definedness conditions of `t` (see the module comment), or `None` for a function not known here.
fn conditions<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, t: T<'h>, vars: &[T<'h>], complex: bool) -> Res<Option<Vec<T<'h>>>> {
    conditions_of(cx, b, t, vars, complex, false)
}

/// [`conditions`]; with `analytic`, the conditions of analyticity (a fractional power's base positive, not only
/// nonnegative).
fn conditions_of<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    b: &Build<'h>,
    t: T<'h>,
    vars: &[T<'h>],
    complex: bool,
    analytic: bool,
) -> Res<Option<Vec<T<'h>>>> {
    let h = b.heap();
    let mut out: Vec<T<'h>> = vec![];
    let mut stack = vec![t];
    let mut seen = rustc_hash::FxHashSet::default();
    let depends = |u: T<'h>| vars.iter().any(|&x| !is_free(b, u, x));
    let rel = |name: &str, l: T<'h>, r: T<'h>| h.call(h.sym(name), &[l, r]);
    while let Some(u) = stack.pop() {
        cx.tick(1)?;
        if !seen.insert(u) || !depends(u) {
            continue;
        }
        let View::App { head, args } = h.view(u) else { continue };
        let Some(s) = b.sym_of(head) else { return Ok(None) };
        stack.extend(args.iter().copied());
        let name = h.sym_name(s);
        let zero = b.zero();
        if s == b.hd.pow {
            let [base, ex] = args else { return Ok(None) };
            if !depends(*base) {
                // c^u: defined for a positive base (and any other constant base by the principal branch)
                continue;
            }
            match h.view(*ex) {
                View::Int(k) if k < 0 => out.push(rel("unequal", *base, zero)),
                View::Int(_) => {}
                View::Rational { .. } => {
                    let neg = b.ar.compare(*ex, zero).is_some_and(|c| c == mc_arith::Cmp3::Less);
                    let even_den = {
                        let d = cx.eval(b.call("denominator", &[*ex]))?;
                        matches!(h.view(d), View::Int(k) if k % 2 == 0)
                    };
                    if complex {
                        if neg {
                            out.push(rel("unequal", *base, zero));
                        }
                        if analytic {
                            // the branch cut of the principal root along the nonpositive reals
                            out.push(branch_cut(b, *base));
                        }
                    } else if even_den {
                        out.push(rel(if neg || analytic { "greater" } else { "greater_equal" }, *base, zero));
                    } else if neg {
                        out.push(rel("unequal", *base, zero));
                    }
                }
                _ => {
                    if depends(*ex) || !complex {
                        out.push(rel("greater", *base, zero));
                    } else {
                        out.push(rel("unequal", *base, zero));
                    }
                }
            }
            continue;
        }
        if s == b.hd.add || s == b.hd.mul || s == b.hd.list {
            continue;
        }
        let [a] = args else {
            if CONTINUOUS.contains(&name.as_str()) {
                continue;
            }
            if let Some((last, params)) = args.split_last() {
                let fixed = params.iter().all(|&p| !depends(p));
                let integer_order = matches!(params, [n] if matches!(h.view(*n), View::Int(_)));
                if fixed && depends(*last) && (ENTIRE_LAST.contains(&name.as_str()) || ENTIRE_INTEGER_ORDER.contains(&name.as_str()) && integer_order) {
                    continue;
                }
            }
            return Ok(None);
        };
        let one = b.one();
        match name.as_str() {
            n if ENTIRE.contains(&n) || CONTINUOUS.contains(&n) => {}
            "log" if complex && analytic => out.push(branch_cut(b, *a)),
            "log" => out.push(rel(if complex { "unequal" } else { "greater" }, *a, zero)),
            "asin" | "acos" if !complex => {
                out.push(rel("greater_equal", *a, b.neg(one)));
                out.push(rel("less_equal", *a, one));
            }
            "asin" | "acos" | "asinh" => {}
            "acosh" if !complex => out.push(rel("greater_equal", *a, one)),
            "atanh" if !complex => {
                out.push(rel("greater", *a, b.neg(one)));
                out.push(rel("less", *a, one));
            }
            "atanh" => {
                out.push(rel("unequal", *a, one));
                out.push(rel("unequal", *a, b.neg(one)));
            }
            "tan" | "sec" => out.push(rel("unequal", b.call("cos", &[*a]), zero)),
            "cot" | "csc" => out.push(rel("unequal", b.call("sin", &[*a]), zero)),
            "coth" | "csch" => out.push(rel("unequal", *a, zero)),
            _ => return Ok(None),
        }
    }
    // evaluate, drop the ones that hold everywhere, and order them
    let mut done = vec![];
    for c in out {
        let v = cx.eval(c)?;
        if cx.is_true(v) {
            continue;
        }
        if cx.is_false(v) {
            return Ok(Some(vec![v]));
        }
        if !done.contains(&v) {
            done.push(v);
        }
    }
    Ok(Some(done))
}

/// `not (im_part(u) == 0 and re(u) <= 0)`: off the branch cut of the principal root and logarithm.
fn branch_cut<'h>(b: &Build<'h>, u: T<'h>) -> T<'h> {
    let on = b.call("and", &[b.call("equal", &[b.call("im_part", &[u]), b.zero()]), b.call("less_equal", &[b.call("re", &[u]), b.zero()])]);
    b.call("not", &[on])
}

/// `a op b` for an inequality with a constant term: the constant moved to the right (`x^2 + y^2 >= 9`).
fn tidy<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, c: T<'h>) -> Res<T<'h>> {
    let h = b.heap();
    let View::App { head, args: &[l, r] } = h.view(c) else { return Ok(c) };
    let Some(s) = b.sym_of(head) else { return Ok(c) };
    if !matches!(h.sym_name(s).as_str(), "greater" | "greater_equal" | "less" | "less_equal") || r != b.zero() {
        return Ok(c);
    }
    let View::App { head: ah, args: terms } = h.view(l) else { return Ok(c) };
    if b.sym_of(ah) != Some(b.hd.add) {
        return Ok(c);
    }
    let (num, rest): (Vec<T<'h>>, Vec<T<'h>>) = terms.iter().partition(|&&t| b.ar.num(t).is_some());
    if num.is_empty() || rest.is_empty() {
        return Ok(c);
    }
    let k = b.neg(b.add(&num));
    cx.eval(h.call(s, &[b.add(&rest), k]))
}

/// The conjunction of conditions (`true` for none).
fn conj<'h>(cx: &mut NativeCx<'_, '_, 'h>, cs: &[T<'h>]) -> Res<T<'h>> {
    let h = cx.heap();
    Ok(match cs {
        [] => cx.bool(true),
        [c] => *c,
        _ => cx.eval(h.call(h.sym("and"), cs))?,
    })
}

/// Is `t` a call of `name` (left unevaluated)?
fn is_call<'h>(b: &Build<'h>, t: T<'h>, name: &str) -> bool {
    let h = b.heap();
    matches!(h.view(t), View::App { head, .. } if b.sym_of(head) == Some(h.sym(name)))
}

fn domain<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, c: &Call<'h>) -> Res<Option<T<'h>>> {
    let h = b.heap();
    let Some(mut cs) = conditions(cx, b, c.f, &c.vars, c.complex)? else { return Ok(None) };
    if let Some(k) = c.cond {
        cs.push(k);
    }
    if !c.complex && c.vars.len() == 1 && !cs.is_empty() {
        let all = conj(cx, &cs)?;
        let r = cx.eval(h.call(h.sym("reduce"), &[all, c.vars[0], h.sym_term(h.sym("reals"))]))?;
        if !is_call(b, r, "reduce") && !is_call(b, r, names::FAILURE) {
            return Ok(Some(r));
        }
    }
    let mut tidied = Vec::with_capacity(cs.len());
    for k in cs {
        tidied.push(tidy(cx, b, k)?);
    }
    Ok(Some(conj(cx, &tidied)?))
}

/// The negation of a definedness condition.
fn negate<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, k: T<'h>) -> Res<T<'h>> {
    let h = b.heap();
    if let View::App { head, args: &[l, r] } = h.view(k) {
        if let Some(s) = b.sym_of(head) {
            let flip = match h.sym_name(s).as_str() {
                "unequal" => Some("equal"),
                "greater" => Some("less_equal"),
                "greater_equal" => Some("less"),
                "less" => Some("greater_equal"),
                "less_equal" => Some("greater"),
                _ => None,
            };
            if let Some(n) = flip {
                return cx.eval(h.call(h.sym(n), &[l, r]));
            }
        }
    }
    cx.eval(b.call("not", &[k]))
}

fn singularities<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, c: &Call<'h>) -> Res<Option<T<'h>>> {
    let h = b.heap();
    let Some(cs) = conditions_of(cx, b, c.f, &c.vars, c.complex, true)? else { return Ok(None) };
    let mut out = vec![];
    for k in cs {
        out.push(negate(cx, b, k)?);
    }
    Ok(Some(match out.as_slice() {
        [] => cx.bool(false),
        [k] => *k,
        _ => cx.eval(h.call(h.sym("or"), &out))?,
    }))
}

fn analytic<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, c: &Call<'h>) -> Res<Option<T<'h>>> {
    let h = b.heap();
    // a non-analytic continuous function of a variable itself (`floor(x)`, `abs(x)`): not analytic
    if let View::App { head, args: &[a, ..] } = h.view(c.f) {
        if b.sym_of(head).is_some_and(|s| CONTINUOUS.contains(&h.sym_name(s).as_str())) && c.vars.contains(&a) && c.cond.is_none() {
            return Ok(Some(cx.bool(false)));
        }
    }
    // non-analytic continuous functions of the variables: not decided here
    let mut stack = vec![c.f];
    while let Some(u) = stack.pop() {
        if let View::App { head, args } = h.view(u) {
            if b.sym_of(head).is_some_and(|s| CONTINUOUS.contains(&h.sym_name(s).as_str())) && c.vars.iter().any(|&x| !is_free(b, u, x)) {
                return Ok(None);
            }
            stack.extend(args.iter().copied());
        }
    }
    let Some(cs) = conditions_of(cx, b, c.f, &c.vars, c.complex, true)? else { return Ok(None) };
    if cs.is_empty() {
        return Ok(Some(cx.bool(true)));
    }
    let Some(k) = c.cond else { return Ok(Some(cx.bool(false))) };
    // every condition implied by the given one
    for d in cs {
        let r = cx.eval(h.call(h.sym("refine"), &[d, k]))?;
        if !cx.is_true(r) {
            return Ok(Some(cx.bool(false)));
        }
    }
    Ok(Some(cx.bool(true)))
}

/// The machine value of `f` at `x = v` (one variable), if finite and real.
fn value_at<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, x: T<'h>, v: f64) -> Res<Option<f64>> {
    let h = b.heap();
    let rule = h.call(h.sym(names::RULE), &[x, h.real(v)]);
    let r = cx.eval(h.call(h.sym("rewrite"), &[f, rule]))?;
    let r = cx.eval(h.call(h.sym("n"), &[r]))?;
    Ok(match h.view(r) {
        View::Real(y) if y.is_finite() => Some(y),
        View::Int(k) => Some(k as f64),
        _ => None,
    })
}

/// Sample points of the real line (or of the integers): 0, ±10^k at quarter decades from 10^-6 to 10^6, and the
/// integers -50..50.
fn samples(integers: bool) -> Vec<f64> {
    let mut v: Vec<f64> = (-50..=50).map(f64::from).collect();
    if !integers {
        for k in -24..=24 {
            let p = 10f64.powf(k as f64 / 4.0);
            v.push(p);
            v.push(-p);
        }
        for k in 0..400 {
            v.push(-10.0 + 0.05 * k as f64 + 0.0123);
        }
    }
    v.sort_by(f64::total_cmp);
    v.dedup();
    v
}

/// The sign of `f`, `f'` or `f''` on the domain (module comment): `1`, `-1`, `0`, `indeterminate`, or `None`.
fn sign_of<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, c: &Call<'h>, order: usize) -> Res<Option<T<'h>>> {
    let h = b.heap();
    let [x] = c.vars.as_slice() else { return sign_multi(cx, b, c, order) };
    let x = *x;
    let mut g = c.f;
    if c.integers && order > 0 {
        // a sequence: differences instead of derivatives
        for _ in 0..order {
            let next = cx.eval(h.call(h.sym("rewrite"), &[g, h.call(h.sym(names::RULE), &[x, b.add(&[x, b.one()])])]))?;
            g = cx.eval(b.sub(next, g))?;
        }
    } else {
        for _ in 0..order {
            g = cx.eval(b.call("d", &[g, x]))?;
        }
    }
    if is_call(b, g, "d") {
        return Ok(None);
    }
    let Some(cs) = conditions(cx, b, c.f, &c.vars, false)? else { return Ok(None) };
    let mut facts = cs.clone();
    facts.push(h.call(h.sym("element"), &[x, h.sym_term(h.sym(if c.integers { "integers" } else { "reals" }))]));
    if let Some(k) = c.cond {
        facts.push(k);
    }
    // a proof by refine under the domain's facts
    let fact = conj(cx, &facts)?;
    let zero = b.zero();
    let ge = cx.eval_assuming(&[fact], h.call(h.sym("refine"), &[h.call(h.sym("greater_equal"), &[g, zero])]))?;
    let le = cx.eval_assuming(&[fact], h.call(h.sym("refine"), &[h.call(h.sym("less_equal"), &[g, zero])]))?;
    let indet = h.sym_term(h.sym("indeterminate"));
    match (cx.is_true(ge), cx.is_true(le)) {
        (true, true) => return Ok(Some(zero)),
        (true, false) => return Ok(Some(b.one())),
        (false, true) => return Ok(Some(b.int(-1))),
        _ => {}
    }
    // sampling: only without parameters
    let mut params = rustc_hash::FxHashSet::default();
    symbols(b, g, &mut params);
    if params.iter().any(|&s| s != x) {
        return Ok(None);
    }
    let (mut pos, mut neg, mut any) = (false, false, false);
    for v in samples(c.integers) {
        cx.tick(1)?;
        // inside the domain and the condition
        if let Some(k) = c.cond {
            let rule = h.call(h.sym(names::RULE), &[x, h.real(v)]);
            let ok = cx.eval(h.call(h.sym("rewrite"), &[k, rule]))?;
            if !cx.is_true(ok) {
                continue;
            }
        }
        if value_at(cx, b, c.f, x, v)?.is_none() {
            continue;
        }
        let Some(y) = value_at(cx, b, g, x, v)? else { continue };
        any = true;
        let tol = 1e-12 * (1.0 + y.abs());
        if y > tol {
            pos = true;
        } else if y < -tol {
            neg = true;
        }
        if pos && neg {
            return Ok(Some(indet));
        }
    }
    if !any {
        return Ok(None);
    }
    Ok(Some(match (pos, neg) {
        (true, false) => b.one(),
        (false, true) => b.int(-1),
        (false, false) => zero,
        _ => indet,
    }))
}

/// Several variables: only what refine proves.
fn sign_multi<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, c: &Call<'h>, order: usize) -> Res<Option<T<'h>>> {
    if order > 0 {
        return Ok(None);
    }
    let h = b.heap();
    let Some(cs) = conditions(cx, b, c.f, &c.vars, false)? else { return Ok(None) };
    let mut facts = cs;
    let reals = h.sym_term(h.sym("reals"));
    facts.push(h.call(h.sym("element"), &[cx.list(&c.vars), reals]));
    if let Some(k) = c.cond {
        facts.push(k);
    }
    let fact = conj(cx, &facts)?;
    let zero = b.zero();
    let ge = cx.eval_assuming(&[fact], h.call(h.sym("refine"), &[h.call(h.sym("greater_equal"), &[c.f, zero])]))?;
    let le = cx.eval_assuming(&[fact], h.call(h.sym("refine"), &[h.call(h.sym("less_equal"), &[c.f, zero])]))?;
    Ok(match (cx.is_true(ge), cx.is_true(le)) {
        (true, true) => Some(zero),
        (true, false) => Some(b.one()),
        (false, true) => Some(b.int(-1)),
        _ => None,
    })
}

/// The symbols of `t` (not heads).
fn symbols<'h>(b: &Build<'h>, t: T<'h>, out: &mut rustc_hash::FxHashSet<T<'h>>) {
    let h = b.heap();
    let mut stack = vec![t];
    while let Some(u) = stack.pop() {
        match h.view(u) {
            View::Sym(s) => {
                let n = h.sym_name(s);
                if !matches!(n.as_str(), "pi" | "e" | "euler_gamma" | "degree" | "im" | "inf") {
                    out.insert(u);
                }
            }
            View::App { args, .. } => stack.extend(args.iter().copied()),
            _ => {}
        }
    }
}

fn injective<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, c: &Call<'h>) -> Res<Option<T<'h>>> {
    let h = b.heap();
    let [x] = c.vars.as_slice() else { return Ok(None) };
    if c.complex || cx.as_list(c.f).is_some() {
        return Ok(None);
    }
    let x = *x;
    let mut params = rustc_hash::FxHashSet::default();
    symbols(b, c.f, &mut params);
    if params.iter().any(|&s| s != x) {
        return Ok(None);
    }
    // strictly monotonic: the derivative of one sign (zero at isolated points only); not for functions with jumps or
    // flat pieces (`floor`, `fractional_part`, `ramp`, ...), whose derivative says nothing about that
    let jumps = {
        let mut stack = vec![c.f];
        let mut found = false;
        while let Some(u) = stack.pop() {
            if let View::App { head, args } = h.view(u) {
                if b.sym_of(head).is_some_and(|s| CONTINUOUS.contains(&h.sym_name(s).as_str()) && h.sym_name(s) != "abs") && !is_free(b, u, x) {
                    found = true;
                    break;
                }
                stack.extend(args.iter().copied());
            }
        }
        found
    };
    if !c.integers && !jumps {
        let m = sign_of(cx, b, c, 1)?;
        if let Some(m) = m {
            match h.view(m) {
                View::Int(1 | -1) => return Ok(Some(cx.bool(true))),
                View::Int(0) => return Ok(Some(cx.bool(false))),
                _ => {}
            }
        }
    }
    // two equal values among the samples (of a function that is not constant)
    let mut vals: Vec<(f64, f64)> = vec![];
    for v in samples(c.integers).into_iter().filter(|v| v.abs() <= 50.0) {
        cx.tick(1)?;
        if let Some(k) = c.cond {
            let rule = h.call(h.sym(names::RULE), &[x, h.real(v)]);
            let ok = cx.eval(h.call(h.sym("rewrite"), &[k, rule]))?;
            if !cx.is_true(ok) {
                continue;
            }
        }
        if let Some(y) = value_at(cx, b, c.f, x, v)? {
            vals.push((v, y));
        }
    }
    // strictly monotonic samples: injective when the derivative has one sign (or the sequence is strictly monotonic)
    let increasing = vals.windows(2).all(|w| w[1].1 > w[0].1);
    let decreasing = vals.windows(2).all(|w| w[1].1 < w[0].1);
    if vals.len() >= 2 && (increasing || decreasing) {
        return Ok(Some(cx.bool(true)));
    }
    let mut sorted: Vec<f64> = vals.iter().map(|p| p.1).collect();
    sorted.sort_by(f64::total_cmp);
    let tie = sorted.windows(2).any(|w| (w[1] - w[0]).abs() <= 1e-12 * (1.0 + w[0].abs()));
    if tie || !(increasing || decreasing) && vals.len() >= 3 {
        // values not monotonic along the line: some value is taken twice (intermediate values of a continuous
        // function; a sequence that is not monotonic may still be injective: only a tie decides it)
        if tie || !c.integers {
            return Ok(Some(cx.bool(false)));
        }
    }
    Ok(None)
}
