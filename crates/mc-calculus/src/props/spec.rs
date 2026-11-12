//! The call of a function property read once (decision R25P.1): the function (a scalar or a list of components),
//! the condition on its variables, the condition on the codomain variables, the variables, the domain and the
//! options `generate_conditions`, `assumptions`, `strict_inequalities` and `performance_goal`.
//!
//! - `f`, `[f, cond]` or `[f, cond, ycond]`; `f` itself may be a list of components (a map to several values). A
//!   list whose second and later elements are all conditions (relations, connectives, `element`, `true`, `false`)
//!   is a function with conditions; any other list is a list of components.
//! - Variables: one symbol or a list; codomain variables (for `function_surjective` and `function_bijective`) follow
//!   as the third argument when it is not a domain.
//! - Domains: `reals` (default), `complexes`, `integers`, and `positive_reals`, `negative_reals`,
//!   `nonnegative_reals`, `nonpositive_reals` (the reals with the condition on every variable).
//! - Assumptions (from `assuming` or `assumptions:`): the facts about the variables join the condition; the others
//!   are facts about the parameters.
use crate::common::is_free;
use crate::term::Build;
use mc_api::{names, Store, View, T};
use mc_eval::{NativeCx, Res};

/// The domain of the variables.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Dom {
    /// the reals
    Reals,
    /// the complex numbers
    Complexes,
    /// the integers
    Integers,
}

/// `generate_conditions`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Gc {
    /// `true` (default): conditions on the parameters as a `conditional_expression`
    Yes,
    /// `false`: the value that holds for generic parameters, without the condition
    No,
    /// `automatic`: only the conditions that fail on a set of measure zero are left out
    Auto,
    /// `none`: no answer when a condition would be needed
    Never,
}

/// The pieces of one call.
pub struct Spec<'h> {
    /// the function: one term, or the list of components
    pub f: T<'h>,
    /// the components (one for a scalar function)
    pub comps: Vec<T<'h>>,
    /// whether `f` was a list of components
    pub vector: bool,
    /// the condition on the variables (with the facts about them), if any
    pub cond: Option<T<'h>>,
    /// the condition on the codomain variables, if any
    pub ycond: Option<T<'h>>,
    /// the variables
    pub vars: Vec<T<'h>>,
    /// the codomain variables, if given
    pub yvars: Vec<T<'h>>,
    /// the domain
    pub dom: Dom,
    /// whether the domain was given
    pub dom_given: bool,
    /// `generate_conditions`
    pub gc: Gc,
    /// facts about the parameters
    pub facts: Vec<T<'h>>,
    /// `strict_inequalities: true`
    pub strict: bool,
    /// `performance_goal: "speed"`
    pub speed: bool,
}

impl<'h> Spec<'h> {
    /// Over the complex numbers?
    pub fn complex(&self) -> bool {
        self.dom == Dom::Complexes
    }
}

/// Is `t` a condition (a relation, a connective, `element`, `true`, `false`)?
pub fn is_condition<'h>(b: &Build<'h>, t: T<'h>) -> bool {
    let h = b.heap();
    match h.view(t) {
        View::Sym(s) => matches!(h.sym_name(s).as_str(), "true" | "false"),
        View::App { head, .. } => b.sym_of(head).is_some_and(|s| {
            matches!(
                h.sym_name(s).as_str(),
                "equal" | "unequal" | "less" | "less_equal" | "greater" | "greater_equal" | "and" | "or" | "not" | "xor"
                    | "implies" | "element" | "not_element" | "inequality"
            )
        }),
        _ => false,
    }
}

fn sym_name<'h>(cx: &NativeCx<'_, '_, 'h>, t: T<'h>) -> Option<String> {
    let h = cx.heap();
    match h.view(t) {
        View::Sym(s) => Some(h.sym_name(s).to_string()),
        _ => None,
    }
}

/// The domain named by `t` and the condition it puts on each variable (`positive_reals`: `x > 0`).
fn domain_name(name: &str) -> Option<(Dom, Option<&'static str>)> {
    Some(match name {
        "reals" => (Dom::Reals, None),
        "complexes" => (Dom::Complexes, None),
        "integers" => (Dom::Integers, None),
        "positive_reals" => (Dom::Reals, Some("greater")),
        "negative_reals" => (Dom::Reals, Some("less")),
        "nonnegative_reals" => (Dom::Reals, Some("greater_equal")),
        "nonpositive_reals" => (Dom::Reals, Some("less_equal")),
        _ => return None,
    })
}

/// Read the call; `None` when the arguments do not have one of the shapes of the module comment.
pub fn parse<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, args: &[T<'h>]) -> Res<Option<Spec<'h>>> {
    let h = b.heap();
    let mut args = args.to_vec();
    let (mut gc, mut strict, mut speed) = (Gc::Yes, false, false);
    let mut assumed: Vec<T<'h>> = vec![];
    if let Some(&last) = args.last() {
        if let View::App { head, args: rules } = h.view(last) {
            if b.sym_of(head) == Some(h.sym("options")) {
                args.pop();
                for &r in rules {
                    let View::App { args: &[k, v], .. } = h.view(r) else { return Ok(None) };
                    let Some(k) = sym_name(cx, k) else { return Ok(None) };
                    let vs = sym_name(cx, v);
                    match k.as_str() {
                        "generate_conditions" => {
                            gc = match vs.as_deref() {
                                Some("false") => Gc::No,
                                Some("automatic") => Gc::Auto,
                                Some("none") => Gc::Never,
                                _ => Gc::Yes,
                            }
                        }
                        "assumptions" => assumed.push(v),
                        "strict_inequalities" => strict = cx.is_true(v),
                        "performance_goal" => {
                            speed = matches!(h.view(v), View::Str(s) if s.eq_ignore_ascii_case("speed"))
                                || vs.as_deref() == Some("speed")
                        }
                        _ => {}
                    }
                }
            }
        }
    }
    let (fc, xs, rest) = match args.as_slice() {
        [f, x, rest @ ..] if rest.len() <= 2 => (*f, *x, rest.to_vec()),
        _ => return Ok(None),
    };
    let mut dom = Dom::Reals;
    let mut dom_given = false;
    let mut dom_rel = None;
    let mut yvars_t = None;
    for (i, &r) in rest.iter().enumerate() {
        if let Some((d, rel)) = sym_name(cx, r).as_deref().and_then(domain_name) {
            if i + 1 != rest.len() {
                return Ok(None);
            }
            dom = d;
            dom_rel = rel;
            dom_given = true;
        } else if i == 0 {
            yvars_t = Some(r);
        } else {
            return Ok(None);
        }
    }
    let as_vars = |t: T<'h>| -> Option<Vec<T<'h>>> {
        let v = match cx.as_list(t) {
            Some(v) => v.to_vec(),
            None => vec![t],
        };
        v.iter().all(|&x| matches!(h.view(x), View::Sym(_))).then_some(v)
    };
    let Some(vars) = as_vars(xs) else { return Ok(None) };
    let yvars = match yvars_t {
        Some(t) => match as_vars(t) {
            Some(v) => v,
            None => return Ok(None),
        },
        None => vec![],
    };
    // the function and its conditions
    let (f, mut cond, ycond) = match cx.as_list(fc) {
        Some([f, c]) if is_condition(b, *c) => (*f, Some(*c), None),
        Some([f, c, d]) if is_condition(b, *c) && is_condition(b, *d) => (*f, Some(*c), Some(*d)),
        _ => (fc, None, None),
    };
    let (comps, vector) = match cx.as_list(f) {
        Some(cs) if !cs.is_empty() => (cs.to_vec(), true),
        _ => (vec![f], false),
    };
    if cond.is_some_and(|c| cx.is_true(c)) {
        cond = None;
    }
    // the facts of the context and of assumptions:
    let mut facts_all: Vec<T<'h>> = cx.assumptions().to_vec();
    for a in assumed {
        match h.view(a) {
            View::App { head, args } if b.sym_of(head) == Some(h.sym("and")) => facts_all.extend(args.iter().copied()),
            _ => facts_all.push(a),
        }
    }
    let mut on_vars = vec![];
    let mut facts = vec![];
    for fct in facts_all {
        if cx.is_true(fct) {
            continue;
        }
        if vars.iter().any(|&x| !is_free(b, fct, x)) {
            on_vars.push(fct);
        } else {
            facts.push(fct);
        }
    }
    if let Some(rel) = dom_rel {
        for &x in &vars {
            on_vars.push(b.call(rel, &[x, b.zero()]));
        }
    }
    if !on_vars.is_empty() {
        if let Some(c) = cond {
            on_vars.insert(0, c);
        }
        cond = Some(match on_vars.as_slice() {
            [c] => *c,
            _ => cx.eval(h.call(h.sym("and"), &on_vars))?,
        });
    }
    let _ = names::LIST;
    Ok(Some(Spec { f, comps, vector, cond, ycond, vars, yvars, dom, dom_given, gc, facts, strict, speed }))
}

/// The symbols of `t` other than the constants and the given variables (the parameters), sorted by name.
pub fn params<'h>(b: &Build<'h>, ts: &[T<'h>], vars: &[T<'h>]) -> Vec<T<'h>> {
    let h = b.heap();
    let mut out: Vec<T<'h>> = vec![];
    let mut stack: Vec<T<'h>> = ts.to_vec();
    let mut seen = rustc_hash::FxHashSet::default();
    while let Some(u) = stack.pop() {
        if !seen.insert(u) {
            continue;
        }
        match h.view(u) {
            View::Sym(s) => {
                let n = h.sym_name(s);
                let constant = matches!(
                    n.as_str(),
                    "pi" | "e" | "euler_gamma" | "degree" | "im" | "inf" | "true" | "false" | "catalan" | "golden_ratio" | "reals"
                        | "complexes" | "integers" | "rationals" | "indeterminate"
                );
                if !constant && !vars.contains(&u) && !out.contains(&u) {
                    out.push(u);
                }
            }
            View::App { head, args } => {
                // `c(1)` constants of families are not parameters
                if b.sym_of(head) == Some(h.sym("c")) {
                    continue;
                }
                if !matches!(h.view(head), View::Sym(_)) {
                    stack.push(head);
                }
                stack.extend(args.iter().copied());
            }
            _ => {}
        }
    }
    out.sort_by_key(|&t| match h.view(t) {
        View::Sym(s) => h.sym_name(s).to_string(),
        _ => String::new(),
    });
    out
}

/// `value` under the condition `c` on the parameters (decision R25P.2): `value` itself when `c` is `true` or
/// `generate_conditions` is `false`; `conditional_expression(value, element(params, reals) and c)` otherwise (the
/// realness only over the reals); under `automatic` the conditions `p != q` are left out. `None` (no answer) when
/// `c` is `false`, or under `generate_conditions: none`.
pub fn conditional<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, s: &Spec<'h>, value: T<'h>, c: T<'h>, ps: &[T<'h>]) -> Res<Option<T<'h>>> {
    let h = b.heap();
    if cx.is_true(c) || s.gc == Gc::No {
        return Ok(Some(value));
    }
    if cx.is_false(c) || s.gc == Gc::Never {
        return Ok(None);
    }
    let mut c = c;
    if s.gc == Gc::Auto {
        let parts: Vec<T<'h>> = match h.view(c) {
            View::App { head, args } if b.sym_of(head) == Some(h.sym("and")) => args.to_vec(),
            _ => vec![c],
        };
        let kept: Vec<T<'h>> = parts.into_iter().filter(|&p| !matches!(h.view(p), View::App { head, .. } if b.sym_of(head) == Some(h.sym("unequal")))).collect();
        c = match kept.as_slice() {
            [] => cx.bool(true),
            [k] => *k,
            _ => h.call(h.sym("and"), &kept),
        };
    }
    let real_ps: Vec<T<'h>> = if s.dom == Dom::Complexes { vec![] } else { ps.to_vec() };
    let cond = if real_ps.is_empty() {
        c
    } else {
        let alt = match real_ps.as_slice() {
            [p] => *p,
            _ => h.call(h.sym("alternatives"), &real_ps),
        };
        let el = h.call(h.sym("element"), &[alt, h.sym_term(h.sym("reals"))]);
        if cx.is_true(c) {
            el
        } else {
            h.call(h.sym("and"), &[el, c])
        }
    };
    if s.gc == Gc::Auto && real_ps.is_empty() && cx.is_true(c) {
        return Ok(Some(value));
    }
    let r = cx.eval(h.call(h.sym(mc_decide::CONDITIONAL), &[value, cond]))?;
    Ok(Some(r))
}
