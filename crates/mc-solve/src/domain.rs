//! `solve(eqs, xs, domain)` and equations combined with conditions (decision 3B.20).
//!
//! The conjuncts of `eqs` (an `and` or a list) are split into equations and conditions (inequalities, `!=`,
//! `element(...)`). The equations are solved as by `solve(eqs, xs)`, then each solution is kept when its values are
//! in the domain (`reals`, `integers`, `rationals`, `complexes`, decided by `element`) and the conditions hold at it;
//! a condition or membership that is not decided keeps the solution only over the reals and complexes (a solution
//! with parameters). Over the integers, when the equations cannot be solved (or there are none) and the conditions
//! bound every unknown, the box is searched (at most [`MAX_BOX`] points).
use mc_api::{names, Store, View, T};
use mc_eval::{NativeCx, Res};

thread_local! {
    /// Set while `solve` over the integers asks `reduce` (which may call `solve` again).
    static IN_INTEGER_REDUCE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// The most points of a box searched over the integers.
pub const MAX_BOX: i64 = 200_000;

/// The domains `solve` accepts as a third argument.
const DOMAINS: [&str; 4] = ["reals", "integers", "rationals", "complexes"];

/// The conjuncts of an `and` or a list (nested ones flattened).
fn conjuncts<'h>(h: &'h impl Store, t: T<'h>, out: &mut Vec<T<'h>>) {
    match h.view(t) {
        View::App { head, args } if matches!(h.view(head), View::Sym(s) if h.sym_name(s) == "and" || h.sym_name(s) == names::LIST) => {
            for &a in args {
                conjuncts(h, a, out);
            }
        }
        _ => out.push(t),
    }
}

/// The name of the head of a call with a symbol head.
fn head_name<'h>(h: &'h impl Store, t: T<'h>) -> Option<String> {
    match h.view(t) {
        View::App { head, .. } => match h.view(head) {
            View::Sym(s) => Some(h.sym_name(s).to_string()),
            _ => None,
        },
        _ => None,
    }
}

/// `solve` with a domain or conditions; `None` when the call is a plain one (for the solver itself).
pub fn solve<'h>(cx: &mut NativeCx<'_, '_, 'h>, args: &[T<'h>]) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    let (args, no_facts) = strip_no_facts(h, args);
    let (eqs, xs, dom) = match args {
        [e, x, d] => match h.view(*d) {
            View::Sym(s) if DOMAINS.contains(&h.sym_name(s).as_str()) => (*e, *x, Some(h.sym_name(s).to_string())),
            _ => return Ok(None),
        },
        [e, x] => (*e, *x, None),
        _ => return Ok(None),
    };
    let mut parts = vec![];
    conjuncts(h, eqs, &mut parts);
    // quantified formulas: reduce eliminates the quantifiers (item 11B), its answer as rules
    if parts.iter().any(|&p| matches!(head_name(h, p).as_deref(), Some("exists" | "for_all"))) {
        let vars: Vec<T<'h>> = match cx.as_list(xs) {
            Some(v) => v.to_vec(),
            None => vec![xs],
        };
        let mut a = vec![eqs, xs];
        if let Some(d) = &dom {
            a.push(h.sym_term(h.sym(d)));
        }
        let r = cx.eval(h.call(h.sym("reduce"), &a))?;
        return Ok(Some(condition_as_rules(cx, r, &vars).unwrap_or_else(|| failure_unsolved(h, "the quantified formula does not reduce to equations"))));
    }
    // a disjunction (under `and` or alone): the union of the solutions of its branches (disjunctive normal form)
    if parts.iter().any(|&p| head_name(h, p).as_deref() == Some("or")) {
        let mut branches: Vec<Vec<T<'h>>> = vec![vec![]];
        for &p in &parts {
            let alts: Vec<T<'h>> = match h.view(p) {
                View::App { args, .. } if head_name(h, p).as_deref() == Some("or") => args.to_vec(),
                _ => vec![p],
            };
            let mut next = vec![];
            for br in &branches {
                for &a in &alts {
                    let mut b = br.clone();
                    conjuncts(h, a, &mut b);
                    next.push(b);
                }
            }
            branches = next;
            if branches.len() > 64 {
                return Ok(None);
            }
        }
        let mut out: Vec<T<'h>> = vec![];
        for br in branches {
            let cond = h.call(h.sym("and"), &br);
            let mut a = vec![cond, xs];
            if let Some(d) = &dom {
                a.push(h.sym_term(h.sym(d)));
            }
            if no_facts {
                a.push(no_facts_option(h));
            }
            let r = cx.eval(h.call(h.sym("solve"), &a))?;
            let Some(items) = cx.as_list(r) else { return Ok(None) };
            for &s in items {
                if !out.contains(&s) {
                    out.push(s);
                }
            }
        }
        sort_by_value(cx, &mut out)?;
        return Ok(Some(cx.list(&out)));
    }
    // an expression that is no relation or logical formula stands for `expression == 0` (as `solve` reads it
    // without assumptions), so that the assumptions filter its solutions too
    let vars: Vec<T<'h>> = match cx.as_list(xs) {
        Some(v) => v.to_vec(),
        None => vec![xs],
    };
    let parts: Vec<T<'h>> = parts
        .into_iter()
        .map(|p| {
            let relation = matches!(
                head_name(h, p).as_deref(),
                Some("equal" | "unequal" | "less" | "less_equal" | "greater" | "greater_equal" | "or" | "not" | "xor" | "implies" | "element" | "not_element" | "exists" | "for_all" | "boole" | "and")
            ) || matches!(h.view(p), View::Sym(s) if matches!(h.sym_name(s).as_str(), "true" | "false"));
            if relation || !mentions(h, p, &vars) {
                p
            } else {
                h.call(h.sym("equal"), &[p, h.int(0)])
            }
        })
        .collect();
    let (equations, mut conds): (Vec<T<'h>>, Vec<T<'h>>) = parts.into_iter().partition(|&p| head_name(h, p).as_deref() == Some("equal"));
    // inside assuming: the facts about the unknowns are conditions too
    let facts = if no_facts { vec![] } else { cx.assumptions().to_vec() };
    for f in facts {
        let mut fs = vec![];
        conjuncts(h, f, &mut fs);
        conds.extend(fs.into_iter().filter(|&c| mentions(h, c, &vars) && head_name(h, c).as_deref() != Some("equal")));
    }
    if vars.iter().any(|v| !matches!(h.view(*v), View::Sym(_))) {
        return Ok(None);
    }
    // radicals of the unknowns: the solver's squaring may add roots; each is checked against the equations
    let radical = !no_facts && equations.iter().any(|&e| has_radical(h, e, &vars));
    if dom.is_none() && conds.is_empty() {
        if !radical {
            return Ok(None);
        }
        let eq_list = cx.list(&equations);
        let var_list = cx.list(&vars);
        let r = cx.eval(h.call(h.sym("solve"), &[eq_list, var_list, no_facts_option(h)]))?;
        let Some(items) = cx.as_list(r) else { return Ok(Some(r)) };
        let items = items.to_vec();
        let mut kept = vec![];
        for s in items {
            if !spurious(cx, &equations, &vars, s)? {
                kept.push(s);
            }
        }
        return Ok(Some(cx.list(&kept)));
    }
    let dom = dom.unwrap_or_else(|| "complexes".to_string());
    let strict = dom == "integers" || dom == "rationals";
    // the inner calls solve over the reals: real branches of the inverse functions (decision 11C.9)
    // an order relation on an unknown (0 < x) makes it real as well: the complex branches (acoth(c) + i pi c(1))
    // would ignore the condition
    let ordered = conds.iter().any(|&c| {
        matches!(head_name(h, c).as_deref(), Some("less" | "greater" | "less_equal" | "greater_equal")) && mentions(h, c, &vars)
    });
    let real = crate::front::Settings { reals: Some(dom != "complexes" || ordered), ..Default::default() };
    let _real = crate::front::Scope::enter(&real);
    let mut sols: Option<Vec<T<'h>>> = None;
    // fewer equations than unknowns with conditions p != q: the Rabinowitsch variable z with (p - q) z == 1 removes
    // the components where p == q from the solution (decision 11C.15); z is dropped from the solutions
    let mut extra_vars = vec![];
    let mut equations = equations;
    if !equations.is_empty() && equations.len() < vars.len() {
        for (i, &c) in conds.iter().enumerate() {
            if head_name(h, c).as_deref() != Some("unequal") {
                continue;
            }
            let View::App { args: &[l, r], .. } = h.view(c) else { continue };
            if !mentions(h, c, &vars) {
                continue;
            }
            let z = h.sym_term(h.sym(&format!("solve`z{i}")));
            let d = h.call(h.sym(names::ADD), &[l, h.call(h.sym(names::MUL), &[h.int(-1), r])]);
            equations.push(h.call(h.sym("equal"), &[h.call(h.sym(names::MUL), &[d, z]), h.int(1)]));
            extra_vars.push(z);
        }
    }
    if !equations.is_empty() {
        // the Rabinowitsch variables first: they are eliminated, the unknowns stay free
        let mut all_vars = extra_vars.clone();
        all_vars.extend(vars.iter().copied());
        let eq_list = cx.list(&equations);
        let var_list = cx.list(&all_vars);
        let r = cx.eval(h.call(h.sym("solve"), &[eq_list, var_list, no_facts_option(h)]))?;
        if let Some(items) = cx.as_list(r) {
            if items.iter().all(|s| cx.as_list(*s).is_some()) {
                let mut kept = vec![];
                for &s in items {
                    let rules: Vec<T<'h>> = cx
                        .as_list(s)
                        .unwrap_or(&[])
                        .iter()
                        .copied()
                        .filter(|&r| !matches!(h.view(r), View::App { args: &[a, _], .. } if extra_vars.contains(&a)))
                        .collect();
                    kept.push(cx.list(&rules));
                }
                sols = Some(kept);
            }
        }
        equations.truncate(equations.len() - extra_vars.len());
    }
    // families with a generated parameter, one unknown bounded to a finite real interval: the members in it
    if let ([x], Some(ss)) = (vars.as_slice(), sols.as_ref()) {
        if let (Some(a), Some(b)) = real_bounds(cx, *x, &conds)? {
            let ss = ss.clone();
            sols = Some(expand_families(cx, ss, a, b, false, &conds)?);
        } else {
            // a finite strip of the real part (0 <= re(x) < pi): the members whose real part lies in it
            let rex = h.call(h.sym("re"), &[*x]);
            if let (Some(a), Some(b)) = real_bounds(cx, rex, &conds)? {
                let ss = sols.clone().unwrap_or_default();
                sols = Some(expand_families(cx, ss, a, b, true, &conds)?);
            }
        }
    }
    // one equation that is not polynomial in one unknown the conditions bound to a finite real interval: the real
    // roots there, numerically (decision IOSYM.3), with the solutions found
    let mut sort_real = false;
    if let ([x], [eq], false) = (vars.as_slice(), equations.as_slice(), strict) {
        if let View::App { args: &[l, r], .. } = h.view(*eq) {
            let f = cx.eval(h.call(h.sym(names::ADD), &[l, h.call(h.sym(names::MUL), &[h.int(-1), r])]))?;
            if transcendental(h, f, *x) {
                if let (Some(a), Some(b)) = real_bounds(cx, *x, &conds)? {
                    let found = crate::interval::roots(cx, f, *x, a, b)?;
                    let mut all = sols.take().unwrap_or_default();
                    let mut known = vec![];
                    for &s in &all {
                        known.push(numeric_value(cx, s)?);
                    }
                    for v in found {
                        let rule = h.call(h.sym(names::RULE), &[*x, v]);
                        let s = cx.list(&[rule]);
                        let nv = numeric_value(cx, s)?;
                        // (a numeric root of a double root is off by about the square root of the precision)
                        if nv.is_some() && known.iter().any(|k| k.is_some_and(|k| (k - nv.unwrap()).abs() <= 1e-7 * (1.0 + k.abs()))) {
                            continue;
                        }
                        known.push(nv);
                        all.push(s);
                    }
                    sols = Some(all);
                    sort_real = true;
                }
            }
        }
    }
    // a parametric solution (fewer rules than unknowns, or values in the unknowns) over the integers: the box
    if strict {
        if let Some(ss) = &sols {
            let parametric = ss.iter().any(|&s| {
                let rules = cx.as_list(s).unwrap_or(&[]);
                rules.len() < vars.len() || rules.iter().any(|&r| matches!(h.view(r), View::App { args: &[_, v], .. } if mentions(h, v, &vars)))
            });
            if parametric {
                sols = None;
            }
        }
    }
    let Some(sols) = sols else {
        // over the integers: a search of the box the conditions and equations give, else reduce over the integers
        // (through its public entry; item 11B) with its answer as rules
        if dom == "integers" {
            if let Some(r) = search_box(cx, &vars, &equations, &conds)? {
                return Ok(Some(r));
            }
            // (once: reduce over the integers may come back here through solve)
            if !IN_INTEGER_REDUCE.with(|f| f.get()) {
                let all: Vec<T<'h>> = equations.iter().chain(conds.iter()).copied().collect();
                let conj = if all.len() == 1 { all[0] } else { h.call(h.sym("and"), &all) };
                let var_list = cx.list(&vars);
                IN_INTEGER_REDUCE.with(|f| f.set(true));
                let r = cx.eval(h.call(h.sym("reduce"), &[conj, var_list, h.sym_term(h.sym("integers"))]));
                IN_INTEGER_REDUCE.with(|f| f.set(false));
                let r = r?;
                if let Some(rules) = condition_as_rules(cx, r, &vars) {
                    return Ok(Some(rules));
                }
                // families with a parameter (the Pell family x + y sqrt(d) = (x1 + y1 sqrt(d))^c(1)): the conditions
                // on the parameter as conditional_expression on each value
                if let Some(rules) = condition_as_conditional_rules(cx, r, &vars)? {
                    return Ok(Some(rules));
                }
            }
            return Ok(Some(failure_unsolved(h, "no method found for these equations over the integers")));
        }
        // over the reals: an absolute value split into its two cases
        if dom == "reals" {
            if let Some(r) = split_abs(cx, eqs, xs)? {
                return Ok(Some(r));
            }
        }
        // the plain solve's answer (a failure says why)
        if equations.is_empty() {
            return Ok(None);
        }
        let eq_list = cx.list(&equations);
        let var_list = cx.list(&vars);
        return Ok(Some(cx.eval(h.call(h.sym("solve"), &[eq_list, var_list, no_facts_option(h)]))?));
    };
    let mut kept = vec![];
    // one decider for every solution's conditions (its memo is shared); its arithmetic flag is never raised, the
    // native's stop scope polls the engine's
    let flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let mut decider: Option<mc_decide::Decider<'h>> = None;
    'sol: for s in sols {
        cx.tick(1)?;
        // the values in the domain
        if dom != "complexes" {
            for &rule in cx.as_list(s).unwrap_or(&[]) {
                let View::App { args: &[_, v], .. } = h.view(rule) else { continue };
                // a root object of a polynomial with numbers for coefficients: real exactly when its value is (the
                // real roots are isolated exactly, decision 3B.8)
                if dom == "reals" && is_root_object(h, v) {
                    let nv = cx.eval(h.call(h.sym("n"), &[v]))?;
                    match h.view(nv) {
                        View::Real(_) | View::Int(_) => continue,
                        View::Ball { .. } => continue,
                        _ => {
                            if head_name(h, nv).as_deref() == Some("root") {
                                // not evaluated: undecided
                            } else {
                                continue 'sol;
                            }
                        }
                    }
                }
                let e = cx.eval(h.call(h.sym("element"), &[v, h.sym_term(h.sym(&dom))]))?;
                if cx.is_false(e) || strict && !cx.is_true(e) {
                    continue 'sol;
                }
            }
        }
        for &c in &conds {
            let v = cx.eval(h.call(h.sym("rewrite"), &[c, s]))?;
            // a condition undefined at the solution (abs(1/0) < 7 at x = 1) does not hold there
            if ["complex_infinity", "indeterminate", names::FAILURE].iter().any(|n| mentions(h, v, &[h.sym_term(h.sym(n))])) {
                continue 'sol;
            }
            let mut truth = if cx.is_true(v) {
                Some(true)
            } else if cx.is_false(v) {
                Some(false)
            } else {
                None
            };
            if truth.is_none() && nonreal_order(cx, v)? {
                // an order relation between numbers that are not real (im > 0 at x = im) does not hold: the
                // relation makes the unknown real
                truth = Some(false);
            }
            if truth.is_none() && !no_facts {
                // decided by the facts of the context through the decision service (G51: -sqrt(positive) > 0 is
                // false; item F3)
                if decider.is_none() {
                    decider = Some(mc_decide::Decider::of_context(cx, &flag)?);
                }
                if let Some(d) = decider.as_mut() {
                    truth = d.truth(cx, v)?;
                }
                if truth.is_none() {
                    // refine's rewritings (a radical of a square under a sign fact) may still decide it
                    let r = cx.eval(h.call(h.sym("refine"), &[v]))?;
                    truth = if cx.is_true(r) { Some(true) } else if cx.is_false(r) { Some(false) } else { None };
                }
            }
            if truth == Some(false) || strict && truth != Some(true) {
                continue 'sol;
            }
        }
        if radical && spurious(cx, &equations, &vars, s)? {
            continue;
        }
        // over the reals, values with square roots of the parameters: real (generically) where the radicands are
        // positive, the condition from reduce in the parameters (decision 11C.12)
        let s = if dom == "reals" && !no_facts {
            match real_conditions(cx, s, &vars)? {
                Some(t) => t,
                None => continue,
            }
        } else {
            s
        };
        kept.push(s);
    }
    if sort_real {
        // inexact numbers in the equation or the bounds: inexact roots
        if equations.iter().chain(conds.iter()).any(|&t| has_real(h, t)) {
            for s in kept.iter_mut() {
                *s = cx.eval(h.call(h.sym("n"), &[*s]))?;
            }
        }
        sort_by_value(cx, &mut kept)?;
    }
    Ok(Some(cx.list(&kept)))
}

/// The solutions `[x -> conditional_expression(v(c(1)), element(c(1), integers))]` replaced by their members
/// `[x -> v(k)]` with machine values in [a, b] (at most 1000 of them; a family whose values are not real, or with
/// more parameters, stays as it is). With `re` the real parts of the members are bounded instead (the members may
/// be complex).
fn expand_families<'h>(cx: &mut NativeCx<'_, '_, 'h>, sols: Vec<T<'h>>, a: f64, b: f64, re: bool, conds: &[T<'h>]) -> Res<Vec<T<'h>>> {
    let h = cx.heap();
    let gen = h.sym_term(h.sym(crate::front::current().gen.as_deref().unwrap_or("c")));
    let mut out = vec![];
    for s in sols {
        let member = (|| -> Option<(T<'h>, T<'h>)> {
            let &[rule] = cx.as_list(s)? else { return None };
            let View::App { args: &[x, v], .. } = h.view(rule) else { return None };
            let View::App { head, args: &[w, _] } = h.view(v) else { return None };
            (head_name(h, v).as_deref() == Some("conditional_expression") && !matches!(h.view(head), View::Int(_))).then_some((x, w))
        })();
        let Some((x, w)) = member else {
            out.push(s);
            continue;
        };
        let c1 = h.app(gen, &[h.int(1)]);
        let c2 = h.app(gen, &[h.int(2)]);
        if mentions(h, w, &[c2]) {
            // two parameters (a family nested in a family): the members with |c(k)| <= 6 in [a, b]
            let mut seen: Vec<f64> = vec![];
            for k1 in -6..=6i64 {
                for k2 in -6..=6i64 {
                    cx.tick(1)?;
                    let rs = cx.list(&[h.call(h.sym(names::RULE), &[c1, h.int(k1)]), h.call(h.sym(names::RULE), &[c2, h.int(k2)])]);
                    let e = cx.eval(h.call(h.sym("rewrite"), &[w, rs]))?;
                    let n = cx.eval(h.call(h.sym("n"), &[if re { h.call(h.sym("re"), &[e]) } else { e }]))?;
                    let f = match h.view(n) {
                        View::Real(f) => f,
                        View::Int(i) => i as f64,
                        _ => continue,
                    };
                    if f >= a - 1e-12 && f <= b + 1e-12 && !seen.iter().any(|g| (g - f).abs() <= 1e-10 * (1.0 + f.abs())) {
                        seen.push(f);
                        out.push(cx.list(&[h.call(h.sym(names::RULE), &[x, e])]));
                    }
                }
            }
            continue;
        }
        let at = |cx: &mut NativeCx<'_, '_, 'h>, k: i64| -> Res<(T<'h>, Option<f64>)> {
            let r = h.call(h.sym(names::RULE), &[c1, h.int(k)]);
            let e = cx.eval(h.call(h.sym("rewrite"), &[w, r]))?;
            let n = cx.eval(h.call(h.sym("n"), &[if re { h.call(h.sym("re"), &[e]) } else { e }]))?;
            Ok((e, match h.view(n) {
                View::Real(f) => Some(f),
                View::Int(i) => Some(i as f64),
                _ => None,
            }))
        };
        // the members from c(1) = 0 outwards in both directions; a direction ends when three real values in a row
        // lie outside [a, b] moving away from it (the families of solve are monotone in c(1) far out: linear, or
        // a root of a linear function), or after 50 non-real values in a row (sqrt(-pi/2 + 2 pi c) for c <= 0);
        // more than 1000 steps keeps the family as it is
        let dist = |f: f64| if f < a - 1e-12 { a - f } else if f > b + 1e-12 { f - b } else { 0.0 };
        let mut members = vec![];
        let mut complete = true;
        for dir in [1i64, -1] {
            let (mut away, mut nonreal, mut last, mut failing) = (0, 0, None::<f64>, 0);
            let start = if dir == 1 { 0 } else { 1 };
            let mut ended = false;
            for j in start..=1000i64 {
                cx.tick(1)?;
                let (e, f) = at(cx, dir * j)?;
                match f.filter(|f| f.is_finite()) {
                    Some(f) => {
                        nonreal = 0;
                        let d = dist(f);
                        if d == 0.0 {
                            let m = cx.list(&[h.call(h.sym(names::RULE), &[x, e])]);
                            // members that converge inside [a, b] (sqrt(1 + 1/c) -> 1): the other conditions
                            // end the direction once 20 in a row are false there (they are filtered later)
                            if !conds.is_empty() {
                                let all = if conds.len() == 1 { conds[0] } else { h.call(h.sym("and"), conds) };
                                let v = cx.eval(h.call(h.sym("n"), &[h.call(h.sym("rewrite"), &[all, m])]))?;
                                failing = if cx.is_false(v) { failing + 1 } else { 0 };
                            }
                            members.push(m);
                            away = 0;
                            if failing >= 20 {
                                ended = true;
                                break;
                            }
                        } else if last.is_some_and(|l| d > dist(l)) {
                            away += 1;
                        } else {
                            away = 0;
                        }
                        last = Some(f);
                        if away >= 3 {
                            ended = true;
                            break;
                        }
                    }
                    None => {
                        nonreal += 1;
                        if nonreal > 50 {
                            ended = true;
                            break;
                        }
                    }
                }
            }
            complete &= ended;
        }
        if complete {
            out.extend(members);
        } else {
            out.push(s);
        }
    }
    Ok(out)
}

/// Is `v` an order relation `a < b` (or `<=`, `>`, `>=`) between numeric values whose difference is not real?
/// (Its imaginary part is a nonzero number at machine precision, well above rounding.)
fn nonreal_order<'h>(cx: &mut NativeCx<'_, '_, 'h>, v: T<'h>) -> Res<bool> {
    let h = cx.heap();
    if !matches!(head_name(h, v).as_deref(), Some("less" | "greater" | "less_equal" | "greater_equal")) {
        return Ok(false);
    }
    let View::App { args: &[a, b], .. } = h.view(v) else { return Ok(false) };
    let d = h.call(h.sym(names::ADD), &[a, h.call(h.sym(names::MUL), &[h.int(-1), b])]);
    let nd = cx.eval(h.call(h.sym("n"), &[d]))?;
    let im = cx.eval(h.call(h.sym("im_part"), &[nd]))?;
    let re = cx.eval(h.call(h.sym("re"), &[nd]))?;
    Ok(match (h.view(im), h.view(re)) {
        (View::Real(i), View::Real(r)) => i.is_finite() && i.abs() > 1e-10 * (1.0 + r.abs()),
        (View::Real(i), View::Int(r)) => i.is_finite() && i.abs() > 1e-10 * (1.0 + (r as f64).abs()),
        (View::Int(i), _) => i != 0,
        _ => false,
    })
}

/// The machine value of the single rule's value of a solution `[x -> v]`.
fn numeric_value<'h>(cx: &mut NativeCx<'_, '_, 'h>, s: T<'h>) -> Res<Option<f64>> {
    let h = cx.heap();
    let Some(&[rule]) = cx.as_list(s) else { return Ok(None) };
    let View::App { args: &[_, v], .. } = h.view(rule) else { return Ok(None) };
    let nv = cx.eval(h.call(h.sym("n"), &[v]))?;
    Ok(match h.view(nv) {
        View::Real(f) => Some(f),
        View::Int(k) => Some(k as f64),
        _ => None,
    })
}

/// Solutions `[x -> v]` with real values in increasing order (others keep their place at the end).
fn sort_by_value<'h>(cx: &mut NativeCx<'_, '_, 'h>, sols: &mut Vec<T<'h>>) -> Res<()> {
    let mut keyed = Vec::with_capacity(sols.len());
    for &s in sols.iter() {
        keyed.push((numeric_value(cx, s)?.unwrap_or(f64::INFINITY), s));
    }
    keyed.sort_by(|a, b| a.0.total_cmp(&b.0));
    *sols = keyed.into_iter().map(|k| k.1).collect();
    Ok(())
}

/// Does `t` have a machine or ball real?
fn has_real<'h>(h: &'h impl Store, t: T<'h>) -> bool {
    let mut stack = vec![t];
    while let Some(u) = stack.pop() {
        match h.view(u) {
            View::Real(_) | View::Ball { .. } => return true,
            View::App { args, .. } => stack.extend(args.iter().copied()),
            _ => {}
        }
    }
    false
}

/// Does `t` have a function of `x` other than sums, products and powers with constant exponents?
fn transcendental<'h>(h: &'h impl Store, t: T<'h>, x: T<'h>) -> bool {
    let mut stack = vec![t];
    while let Some(u) = stack.pop() {
        let View::App { head, args } = h.view(u) else { continue };
        if !mentions(h, u, &[x]) {
            continue;
        }
        let name = match h.view(head) {
            View::Sym(s) => h.sym_name(s).to_string(),
            _ => return true,
        };
        match name.as_str() {
            n if n == names::ADD || n == names::MUL => stack.extend(args.iter().copied()),
            n if n == names::POW => {
                if args.len() != 2 || mentions(h, args[1], &[x]) {
                    return true;
                }
                stack.push(args[0]);
            }
            _ => return true,
        }
    }
    false
}

/// The real bounds `(lo, hi)` of `x` the conditions `x < v`, `v <= x`, ... give (numeric values).
fn real_bounds<'h>(cx: &mut NativeCx<'_, '_, 'h>, x: T<'h>, conds: &[T<'h>]) -> Res<(Option<f64>, Option<f64>)> {
    let h = cx.heap();
    let (mut lo, mut hi) = (None::<f64>, None::<f64>);
    for &c in conds {
        let Some(name) = head_name(h, c) else { continue };
        let View::App { args: &[l, r], .. } = h.view(c) else { continue };
        let (upper, v) = match (name.as_str(), l == x, r == x) {
            ("less" | "less_equal", true, false) => (true, r),
            ("greater" | "greater_equal", true, false) => (false, r),
            ("less" | "less_equal", false, true) => (false, l),
            ("greater" | "greater_equal", false, true) => (true, l),
            _ => continue,
        };
        let nv = cx.eval(h.call(h.sym("n"), &[v]))?;
        let f = match h.view(nv) {
            View::Real(f) if f.is_finite() => f,
            View::Int(k) => k as f64,
            _ => continue,
        };
        if upper {
            hi = Some(hi.map_or(f, |b: f64| b.min(f)));
        } else {
            lo = Some(lo.map_or(f, |b: f64| b.max(f)));
        }
    }
    Ok((lo, hi))
}

/// The integer bounds of `x` the conditions give: `(lo, hi)` (inclusive).
fn bounds<'h>(cx: &mut NativeCx<'_, '_, 'h>, x: T<'h>, conds: &[T<'h>]) -> Res<(Option<i64>, Option<i64>)> {
    let h = cx.heap();
    let (mut lo, mut hi) = (None::<i64>, None::<i64>);
    for &c in conds {
        let Some(name) = head_name(h, c) else { continue };
        let View::App { args: &[l, r], .. } = h.view(c) else { continue };
        // x op v or v op x, with v a real number
        let (op, v) = if l == x {
            (name.clone(), r)
        } else if r == x {
            let flipped = match name.as_str() {
                "less" => "greater",
                "less_equal" => "greater_equal",
                "greater" => "less",
                "greater_equal" => "less_equal",
                _ => continue,
            };
            (flipped.to_string(), l)
        } else {
            continue;
        };
        let nv = cx.eval(h.call(h.sym("n"), &[v]))?;
        let View::Real(f) = h.view(nv) else {
            if let View::Int(k) = h.view(nv) {
                let f = k as f64;
                apply_bound(&op, f, &mut lo, &mut hi);
            }
            continue;
        };
        apply_bound(&op, f, &mut lo, &mut hi);
    }
    Ok((lo, hi))
}

fn apply_bound(op: &str, f: f64, lo: &mut Option<i64>, hi: &mut Option<i64>) {
    if !f.is_finite() || f.abs() > 1e12 {
        return;
    }
    let (new_lo, new_hi) = match op {
        "greater" => (Some(f.floor() as i64 + 1), None),
        "greater_equal" => (Some(f.ceil() as i64), None),
        "less" => (None, Some(f.ceil() as i64 - 1)),
        "less_equal" => (None, Some(f.floor() as i64)),
        _ => (None, None),
    };
    if let Some(a) = new_lo {
        *lo = Some(lo.map_or(a, |b| b.max(a)));
    }
    if let Some(a) = new_hi {
        *hi = Some(hi.map_or(a, |b| b.min(a)));
    }
}

/// The integer points of the box the conditions give at which every equation and condition is `true`.
fn search_box<'h>(cx: &mut NativeCx<'_, '_, 'h>, vars: &[T<'h>], equations: &[T<'h>], conds: &[T<'h>]) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    let mut ranges = vec![];
    let mut size: i64 = 1;
    let mut los = vec![];
    for &x in vars {
        los.push(bounds(cx, x, conds)?.0);
    }
    // bounds the equations give (a sum of nonnegative terms equal to a constant)
    let eq_bounds = crate::bounds::equation_bounds(h, vars, &los, equations);
    for (i, &x) in vars.iter().enumerate() {
        let (lo, hi) = bounds(cx, x, conds)?;
        let (lo, hi) = match eq_bounds[i] {
            Some(b) => (Some(lo.map_or(-b, |l| l.max(-b))), Some(hi.map_or(b, |u| u.min(b)))),
            None => (lo, hi),
        };
        let (Some(lo), Some(hi)) = (lo, hi) else { return Ok(None) };
        if hi < lo {
            return Ok(Some(cx.list(&[])));
        }
        size = size.saturating_mul(hi - lo + 1);
        if size > MAX_BOX {
            return Ok(None);
        }
        ranges.push((lo, hi));
    }
    let all: Vec<T<'h>> = equations.iter().chain(conds).copied().collect();
    let test = h.call(h.sym("and"), &all);
    let mut point: Vec<i64> = ranges.iter().map(|r| r.0).collect();
    let mut out = vec![];
    loop {
        cx.tick(1)?;
        let rules: Vec<T<'h>> = vars.iter().zip(&point).map(|(&x, &v)| h.call(h.sym(names::RULE), &[x, h.int(v)])).collect();
        let rl = cx.list(&rules);
        let v = cx.eval(h.call(h.sym("rewrite"), &[test, rl]))?;
        if cx.is_true(v) {
            out.push(rl);
        }
        // the next point (the last unknown fastest)
        let mut k = point.len();
        loop {
            if k == 0 {
                return Ok(Some(cx.list(&out)));
            }
            k -= 1;
            if point[k] < ranges[k].1 {
                point[k] += 1;
                break;
            }
            point[k] = ranges[k].0;
        }
    }
}

/// `reduce` for what the one-variable algorithm does not take: several unknowns (equations only, or over the
/// integers), or one unknown over the integers; the solutions of [`solve`] as `x == a and y == b or ...`. A list of
/// one unknown is that unknown. `None` leaves the call to the one-variable algorithm.
pub fn reduce<'h>(cx: &mut NativeCx<'_, '_, 'h>, args: &[T<'h>]) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    let (cond, xs, dom) = match args {
        [c, x] => (*c, *x, None),
        [c, x, d] => (*c, *x, Some(*d)),
        _ => return Ok(None),
    };
    let dom_name = dom.and_then(|d| match h.view(d) {
        View::Sym(s) => Some(h.sym_name(s).to_string()),
        _ => None,
    });
    // conditions on the real and imaginary parts of one complex variable z (re(z), im_part(z)): the region in the
    // real plane (x, y) = (re z, im z), the answer written back in re(z) and im_part(z)
    if matches!(h.view(xs), View::Sym(_)) && matches!(dom_name.as_deref(), None | Some("complexes")) {
        let re_z = h.call(h.sym("re"), &[xs]);
        let im_z = h.call(h.sym("im_part"), &[xs]);
        if mentions(h, cond, &[re_z]) || mentions(h, cond, &[im_z]) {
            let (px, py) = (h.sym_term(h.sym("reduce`re")), h.sym_term(h.sym("reduce`im")));
            let fwd = cx.list(&[h.call(h.sym(names::RULE), &[re_z, px]), h.call(h.sym(names::RULE), &[im_z, py])]);
            let c = cx.eval(h.call(h.sym("rewrite"), &[cond, fwd]))?;
            if !mentions(h, c, &[xs]) {
                let r = cx.eval(h.call(h.sym("reduce"), &[c, cx.list(&[px, py]), h.sym_term(h.sym("reals"))]))?;
                if !matches!(head_name(h, r).as_deref(), Some("failure" | "reduce")) {
                    let back = cx.list(&[h.call(h.sym(names::RULE), &[px, re_z]), h.call(h.sym(names::RULE), &[py, im_z])]);
                    return Ok(Some(cx.eval(h.call(h.sym("rewrite"), &[r, back]))?));
                }
            }
        }
    }
    if let Some([x]) = cx.as_list(xs) {
        let mut a = vec![cond, *x];
        a.extend(dom);
        return Ok(Some(cx.eval(h.call(h.sym("reduce"), &a))?));
    }
    // a signed domain (item F3): the unsigned domain and the sign condition on each variable
    if let Some((base, rel)) = dom_name.as_deref().and_then(signed_domain) {
        let vars: Vec<T<'h>> = match cx.as_list(xs) {
            Some(v) => v.to_vec(),
            None => vec![xs],
        };
        let mut conj = vec![cond];
        conj.extend(vars.iter().map(|&v| h.call(h.sym(rel), &[v, h.int(0)])));
        let c = h.call(h.sym("and"), &conj);
        return Ok(Some(cx.eval(h.call(h.sym("reduce"), &[c, xs, h.sym_term(h.sym(base))]))?));
    }
    // a signed rational domain: over the rationals, then the sign condition over the reals
    if let Some(rel) = dom_name.as_deref().and_then(signed_rationals) {
        if matches!(h.view(xs), View::Sym(_)) {
            let r = cx.eval(h.call(h.sym("reduce"), &[cond, xs, h.sym_term(h.sym("rationals"))]))?;
            if matches!(head_name(h, r).as_deref(), Some("failure" | "reduce")) {
                return Ok(None);
            }
            let c = h.call(h.sym("and"), &[r, h.call(h.sym(rel), &[xs, h.int(0)])]);
            return Ok(Some(cx.eval(h.call(h.sym("reduce"), &[c, xs, h.sym_term(h.sym("reals"))]))?));
        }
    }
    let several = cx.as_list(xs).is_some();
    let integers = dom_name.as_deref() == Some("integers");
    // one variable over the rationals or the complexes: equations only, through solve with the domain; without a
    // domain, equations alone are over the complexes too (the default, round 19 Q19-37: x^2 + 1 == 0
    // has the roots -i and i, not none)
    let equations_only = || {
        let mut parts = vec![];
        conjuncts(h, cond, &mut parts);
        !parts.is_empty() && parts.iter().all(|&p| head_name(h, p).as_deref() == Some("equal"))
    };
    if !several && (matches!(dom_name.as_deref(), Some("rationals" | "complexes")) || dom.is_none() && equations_only()) {
        if !equations_only() {
            return Ok(None);
        }
        let d = dom.unwrap_or_else(|| h.sym_term(h.sym("complexes")));
        let Some(sols) = solve(cx, &[cond, xs, d, no_facts_option(h)])? else { return Ok(None) };
        return Ok(Some(rules_as_condition(cx, sols).unwrap_or(sols)));
    }
    if !several && !integers {
        return Ok(None);
    }
    let mut parts = vec![];
    conjuncts(h, cond, &mut parts);
    let only_equations = parts.iter().all(|&p| head_name(h, p).as_deref() == Some("equal"));
    if !only_equations && !integers {
        return Ok(None);
    }
    let d = dom.unwrap_or_else(|| h.sym_term(h.sym("complexes")));
    let sols = match solve(cx, &[cond, xs, d, no_facts_option(h)])? {
        Some(s) => s,
        None if only_equations => cx.eval(h.call(h.sym("solve"), &[cond, xs]))?,
        None => return Ok(None),
    };
    Ok(rules_as_condition(cx, sols))
}

/// The sign relation of a signed rational domain (`nonnegative_rationals` is `greater_equal`).
fn signed_rationals(name: &str) -> Option<&'static str> {
    Some(match name {
        "positive_rationals" => "greater",
        "negative_rationals" => "less",
        "nonnegative_rationals" => "greater_equal",
        "nonpositive_rationals" => "less_equal",
        _ => return None,
    })
}

/// The unsigned domain and the sign relation of a signed domain name (`positive_reals` is `reals` and `greater`).
fn signed_domain(name: &str) -> Option<(&'static str, &'static str)> {
    let (sign, base) = name.split_once('_')?;
    // the signed integer and rational domains stay with the unsigned path's limits: its integer search does not yet
    // stop on an interrupt for very high degrees (`x^999997`)
    let base = match base {
        "reals" => "reals",
        _ => return None,
    };
    let rel = match sign {
        "positive" => "greater",
        "negative" => "less",
        "nonnegative" => "greater_equal",
        "nonpositive" => "less_equal",
        _ => return None,
    };
    Some((base, rel))
}

/// A list of solutions `[[x -> a, ...], ...]` as a disjunction of conjunctions of equations; `None` when it is not
/// one.
fn rules_as_condition<'h>(cx: &mut NativeCx<'_, '_, 'h>, sols: T<'h>) -> Option<T<'h>> {
    let h = cx.heap();
    let items = cx.as_list(sols)?;
    let mut ors = vec![];
    for &s in items {
        let rules = cx.as_list(s)?;
        let eqs: Vec<T<'h>> = rules
            .iter()
            .filter_map(|&r| match h.view(r) {
                View::App { args: &[a, b], .. } => Some(h.call(h.sym("equal"), &[a, b])),
                _ => None,
            })
            .collect();
        ors.push(if eqs.len() == 1 { eqs[0] } else { h.call(h.sym("and"), &eqs) });
    }
    Some(match ors.len() {
        0 => cx.bool(false),
        1 => ors[0],
        _ => h.call(h.sym("or"), &ors),
    })
}

/// A solution `[x -> v, ...]` over the reals with each value v that has square roots of parameter expressions
/// written `conditional_expression(expand(v), c)`, c the condition (from `reduce` in the parameters) that the
/// radicands are positive; `None` when that condition is `false` (no real solution for any parameters).
fn real_conditions<'h>(cx: &mut NativeCx<'_, '_, 'h>, s: T<'h>, vars: &[T<'h>]) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    let Some(rules) = cx.as_list(s).map(|r| r.to_vec()) else { return Ok(Some(s)) };
    let mut out = vec![];
    for r in rules {
        let View::App { args: &[x, v], .. } = h.view(r) else { return Ok(Some(s)) };
        let mut radicands = vec![];
        even_radicands(h, v, &mut radicands);
        let mut params = vec![];
        for &u in &radicands {
            symbols(h, u, vars, &mut params);
        }
        if radicands.is_empty() || params.is_empty() || head_name(h, v).as_deref() == Some("conditional_expression") {
            out.push(r);
            continue;
        }
        let conds: Vec<T<'h>> = radicands.iter().map(|&u| h.call(h.sym("greater"), &[u, h.int(0)])).collect();
        let conj = if conds.len() == 1 { conds[0] } else { h.call(h.sym("and"), &conds) };
        let ps = if params.len() == 1 { params[0] } else { cx.list(&params) };
        let c = cx.eval(h.call(h.sym("reduce"), &[conj, ps, h.sym_term(h.sym("reals"))]))?;
        if cx.is_false(c) {
            return Ok(None);
        }
        let relational = matches!(head_name(h, c).as_deref(), Some("less" | "less_equal" | "greater" | "greater_equal" | "unequal" | "and" | "or"));
        if cx.is_true(c) || !relational {
            out.push(r);
            continue;
        }
        let e = cx.eval(h.call(h.sym("expand"), &[v]))?;
        let cv = h.call(h.sym("conditional_expression"), &[e, c]);
        out.push(h.call(h.sym(names::RULE), &[x, cv]));
    }
    Ok(Some(cx.list(&out)))
}

/// The radicands u of the powers `u^(p/q)` with q even in `t`.
fn even_radicands<'h>(h: &'h impl Store, t: T<'h>, out: &mut Vec<T<'h>>) {
    let mut stack = vec![t];
    while let Some(u) = stack.pop() {
        if let View::App { head, args } = h.view(u) {
            if let (View::Sym(sy), [b, e]) = (h.view(head), args) {
                if h.sym_name(sy) == names::POW {
                    if let View::Rational { den, .. } = h.view(*e) {
                        if matches!(h.view(den), View::Int(d) if d % 2 == 0) && !out.contains(b) {
                            out.push(*b);
                        }
                    }
                }
            }
            stack.extend(args.iter().copied());
        }
    }
}

/// The symbols of `t` other than `vars` and the constants.
fn symbols<'h>(h: &'h impl Store, t: T<'h>, vars: &[T<'h>], out: &mut Vec<T<'h>>) {
    let mut stack = vec![t];
    while let Some(u) = stack.pop() {
        match h.view(u) {
            View::Sym(sy) => {
                let n = h.sym_name(sy);
                if !vars.contains(&u) && !out.contains(&u) && !matches!(n.as_str(), "pi" | "e" | "im" | "euler_gamma" | "catalan") {
                    out.push(u);
                }
            }
            View::App { args, .. } => stack.extend(args.iter().copied()),
            _ => {}
        }
    }
}

/// `failure(unsolved, solve, message)`.
fn failure_unsolved<'h>(h: &'h impl Store, message: &str) -> T<'h> {
    h.call(h.sym(names::FAILURE), &[h.sym_term(h.sym("unsolved")), h.sym_term(h.sym("solve")), h.string(message)])
}

/// A condition `reduce` gives (`false`, `x == a and y == b`, a disjunction of those, values possibly conditional) as
/// a list of solutions, the unknowns in the order given; `None` when it has another form (inequalities left).
pub fn condition_as_rules<'h>(cx: &mut NativeCx<'_, '_, 'h>, c: T<'h>, vars: &[T<'h>]) -> Option<T<'h>> {
    let h = cx.heap();
    if cx.is_false(c) {
        return Some(cx.list(&[]));
    }
    let mut disj = vec![];
    match h.view(c) {
        View::App { head, args } if head == h.sym_term(h.sym("or")) => disj.extend_from_slice(args),
        _ => disj.push(c),
    }
    let mut out = vec![];
    for d in disj {
        let mut eqs = vec![];
        conjuncts(h, d, &mut eqs);
        let mut rules = vec![];
        for &x in vars {
            let v = eqs.iter().find_map(|&e| match h.view(e) {
                View::App { head, args: &[a, b] } if head == h.sym_term(h.sym("equal")) && a == x => Some(b),
                _ => None,
            })?;
            rules.push(h.call(h.sym(names::RULE), &[x, v]));
        }
        if eqs.len() != vars.len() {
            return None;
        }
        out.push(cx.list(&rules));
    }
    Some(cx.list(&out))
}

/// `c` as solution rules when it is `common and (d1 or d2 or ...)` (or one conjunction) with each `d_i` the equations
/// `x == v` of every unknown and conditions free of the unknowns: those conditions (with `common`, also free of the
/// unknowns) become `conditional_expression(v, conditions)` on each value. `None` for another form.
pub fn condition_as_conditional_rules<'h>(cx: &mut NativeCx<'_, '_, 'h>, c: T<'h>, vars: &[T<'h>]) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    let or = h.sym_term(h.sym("or"));
    let mut top = vec![];
    conjuncts(h, c, &mut top);
    let (common, rest): (Vec<T<'h>>, Vec<T<'h>>) = top.into_iter().partition(|&t| !mentions(h, t, vars));
    let disj: Vec<Vec<T<'h>>> = match rest.as_slice() {
        [d] if matches!(h.view(*d), View::App { head, .. } if head == or) => {
            let View::App { args, .. } = h.view(*d) else { unreachable!() };
            args.iter()
                .map(|&a| {
                    let mut v = vec![];
                    conjuncts(h, a, &mut v);
                    v
                })
                .collect()
        }
        _ => vec![rest.clone()],
    };
    let mut out = vec![];
    for d in disj {
        cx.tick(1)?;
        let (conds, eqs): (Vec<T<'h>>, Vec<T<'h>>) = d.into_iter().partition(|&t| !mentions(h, t, vars));
        if eqs.len() != vars.len() {
            return Ok(None);
        }
        let all: Vec<T<'h>> = common.iter().chain(conds.iter()).copied().collect();
        let cond = match all.as_slice() {
            [] => None,
            [one] => Some(*one),
            _ => Some(h.call(h.sym("and"), &all)),
        };
        let mut rules = vec![];
        for &x in vars {
            let Some(v) = eqs.iter().find_map(|&e| match h.view(e) {
                View::App { head, args: &[a, b] } if head == h.sym_term(h.sym("equal")) && a == x && !mentions(h, b, vars) => Some(b),
                _ => None,
            }) else {
                return Ok(None);
            };
            let v = match cond {
                Some(cd) => cx.eval(h.call(h.sym("conditional_expression"), &[v, cd]))?,
                None => v,
            };
            rules.push(h.call(h.sym(names::RULE), &[x, v]));
        }
        out.push(cx.list(&rules));
    }
    Ok(Some(cx.list(&out)))
}

/// Is `t` a root object `root(f, k)`?
fn is_root_object<'h>(h: &'h impl Store, t: T<'h>) -> bool {
    head_name(h, t).as_deref() == Some("root")
}

/// Does `t` mention one of `vars`?
fn mentions<'h>(h: &'h impl Store, t: T<'h>, vars: &[T<'h>]) -> bool {
    let mut stack = vec![t];
    while let Some(u) = stack.pop() {
        if vars.contains(&u) {
            return true;
        }
        if let View::App { head, args } = h.view(u) {
            stack.push(head);
            stack.extend(args.iter().copied());
        }
    }
    false
}

/// The first `abs(u)` of `t` with `u` mentioning the unknowns.
fn first_abs<'h>(h: &'h impl Store, t: T<'h>, vars: &[T<'h>]) -> Option<(T<'h>, T<'h>)> {
    let mut stack = vec![t];
    while let Some(u) = stack.pop() {
        if let View::App { head, args } = h.view(u) {
            if let (View::Sym(s), [a]) = (h.view(head), args) {
                if h.sym_name(s) == "abs" && mentions(h, *a, vars) {
                    return Some((u, *a));
                }
            }
            stack.extend(args.iter().copied());
        }
    }
    None
}

/// Over the reals: `abs(u)` replaced by `u` under `u >= 0` and by `-u` under `u < 0`, each case solved with the
/// domain; the solutions of both.
fn split_abs<'h>(cx: &mut NativeCx<'_, '_, 'h>, eqs: T<'h>, xs: T<'h>) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    let vars: Vec<T<'h>> = match cx.as_list(xs) {
        Some(v) => v.to_vec(),
        None => vec![xs],
    };
    let Some((a, u)) = first_abs(h, eqs, &vars) else { return Ok(None) };
    let mut out = vec![];
    for (sign, rel) in [(1, "greater_equal"), (-1, "less")] {
        let val = cx.eval(h.call(h.sym(names::MUL), &[h.int(sign), u]))?;
        let rule = h.call(h.sym(names::RULE), &[a, val]);
        let replaced = cx.eval(h.call(h.sym("rewrite"), &[eqs, rule]))?;
        let cond = h.call(h.sym(rel), &[u, h.int(0)]);
        let both = h.call(h.sym("and"), &[replaced, cond]);
        let r = solve(cx, &[both, xs, h.sym_term(h.sym("reals")), no_facts_option(h)])?;
        let Some(r) = r else { return Ok(None) };
        let Some(items) = cx.as_list(r) else { return Ok(None) };
        out.extend_from_slice(items);
    }
    if vars.len() == 1 {
        sort_by_value(cx, &mut out)?;
    }
    Ok(Some(cx.list(&out)))
}

/// `options(assumptions -> false)`: the trailing option of the inner calls of this module, which must not read the
/// context again.
pub fn no_facts_option<'h>(h: &'h impl Store) -> T<'h> {
    let r = h.call(h.sym(names::RULE), &[h.sym_term(h.sym("assumptions")), h.sym_term(h.sym(names::FALSE))]);
    h.call(h.sym("options"), &[r])
}

/// The arguments without a trailing [`no_facts_option`], and whether it was there.
pub fn strip_no_facts<'a, 'h>(h: &'h impl Store, args: &'a [T<'h>]) -> (&'a [T<'h>], bool) {
    match args.split_last() {
        Some((&last, rest)) if last == no_facts_option(h) => (rest, true),
        _ => (args, false),
    }
}

/// Does `t` have a power with a non-integer exponent whose base mentions the unknowns?
fn has_radical<'h>(h: &'h impl Store, t: T<'h>, vars: &[T<'h>]) -> bool {
    let mut stack = vec![t];
    while let Some(u) = stack.pop() {
        if let View::App { head, args } = h.view(u) {
            if let (View::Sym(s), [b, e]) = (h.view(head), args) {
                if h.sym_name(s) == names::POW && matches!(h.view(*e), View::Rational { .. }) && mentions(h, *b, vars) {
                    return true;
                }
            }
            stack.extend(args.iter().copied());
        }
    }
    false
}

/// Sample values of the parameters for [`spurious`].
const SAMPLES: [[f64; 6]; 2] = [[0.7, 1.3, 2.1, 0.37, 3.3, 1.9], [1.7, 0.45, 2.9, 1.15, 0.8, 2.6]];

/// Is the solution `s` (rules) spurious: an equation's residual clearly nonzero, numerically, at every sample of
/// the other symbols? (A solution that is right at one sample is kept.)
fn spurious<'h>(cx: &mut NativeCx<'_, '_, 'h>, equations: &[T<'h>], vars: &[T<'h>], s: T<'h>) -> Res<bool> {
    let h = cx.heap();
    let mut params = vec![];
    let mut stack: Vec<T<'h>> = equations.to_vec();
    stack.push(s);
    while let Some(u) = stack.pop() {
        match h.view(u) {
            View::Sym(sy) => {
                let n = h.sym_name(sy);
                if !vars.contains(&u) && !params.contains(&u) && !matches!(n.as_str(), "pi" | "e" | "im" | "euler_gamma") {
                    params.push(u);
                }
            }
            // the bound variable of a root object's function is not a parameter
            View::App { args, .. } if !is_root_object(h, u) && head_name(h, u).as_deref() != Some("fn") => stack.extend(args.iter().copied()),
            _ => {}
        }
    }
    'sample: for sample in SAMPLES {
        let mut rules: Vec<T<'h>> = params.iter().enumerate().map(|(i, &p)| h.call(h.sym(names::RULE), &[p, h.real(sample[i % 6] * (1.0 + (i / 6) as f64))])).collect();
        let at = cx.eval(h.call(h.sym("rewrite"), &[s, cx.list(&rules)]))?;
        rules.extend(cx.as_list(at).unwrap_or(&[]).iter().copied());
        let rl = cx.list(&rules);
        for &e in equations {
            let View::App { args: &[l, r], .. } = h.view(e) else { continue 'sample };
            let lv = cx.eval(h.call(h.sym("n"), &[h.call(h.sym("rewrite"), &[l, rl])]))?;
            let rv = cx.eval(h.call(h.sym("n"), &[h.call(h.sym("rewrite"), &[r, rl])]))?;
            let mag = |v: T<'h>, cx: &mut NativeCx<'_, '_, 'h>| -> Res<Option<f64>> {
                let a = cx.eval(h.call(h.sym("abs"), &[v]))?;
                Ok(match h.view(a) {
                    View::Real(x) if x.is_finite() => Some(x),
                    View::Int(k) => Some(k as f64),
                    _ => None,
                })
            };
            let d = cx.eval(h.call(h.sym(names::ADD), &[lv, h.call(h.sym(names::MUL), &[h.int(-1), rv])]))?;
            let (Some(dm), Some(lm), Some(rm)) = (mag(d, cx)?, mag(lv, cx)?, mag(rv, cx)?) else { continue 'sample };
            if dm > 1e-6 * (1.0 + lm + rm) {
                continue 'sample;
            }
        }
        // every equation holds at this sample
        return Ok(false);
    }
    Ok(true)
}
