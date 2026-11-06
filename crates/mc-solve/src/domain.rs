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
    let (equations, mut conds): (Vec<T<'h>>, Vec<T<'h>>) = parts.into_iter().partition(|&p| head_name(h, p).as_deref() == Some("equal"));
    let vars: Vec<T<'h>> = match cx.as_list(xs) {
        Some(v) => v.to_vec(),
        None => vec![xs],
    };
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
    let mut sols: Option<Vec<T<'h>>> = None;
    if !equations.is_empty() {
        let eq_list = cx.list(&equations);
        let var_list = cx.list(&vars);
        let r = cx.eval(h.call(h.sym("solve"), &[eq_list, var_list, no_facts_option(h)]))?;
        if let Some(items) = cx.as_list(r) {
            if items.iter().all(|s| cx.as_list(*s).is_some()) {
                sols = Some(items.to_vec());
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
        // over the integers: a search of the box the conditions give
        if dom == "integers" {
            return search_box(cx, &vars, &equations, &conds);
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
    'sol: for s in sols {
        cx.tick(1)?;
        // the values in the domain
        if dom != "complexes" {
            for &rule in cx.as_list(s).unwrap_or(&[]) {
                let View::App { args: &[_, v], .. } = h.view(rule) else { continue };
                let e = cx.eval(h.call(h.sym("element"), &[v, h.sym_term(h.sym(&dom))]))?;
                if cx.is_false(e) || strict && !cx.is_true(e) {
                    continue 'sol;
                }
            }
        }
        for &c in &conds {
            let v = cx.eval(h.call(h.sym("rewrite"), &[c, s]))?;
            if cx.is_false(v) || strict && !cx.is_true(v) {
                continue 'sol;
            }
        }
        if radical && spurious(cx, &equations, &vars, s)? {
            continue;
        }
        kept.push(s);
    }
    Ok(Some(cx.list(&kept)))
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
    for &x in vars {
        let (Some(lo), Some(hi)) = bounds(cx, x, conds)? else { return Ok(None) };
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
    if let Some([x]) = cx.as_list(xs) {
        let mut a = vec![cond, *x];
        a.extend(dom);
        return Ok(Some(cx.eval(h.call(h.sym("reduce"), &a))?));
    }
    let several = cx.as_list(xs).is_some();
    let integers = dom_name.as_deref() == Some("integers");
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
    let Some(items) = cx.as_list(sols) else { return Ok(None) };
    let mut ors = vec![];
    for &s in items {
        let Some(rules) = cx.as_list(s) else { return Ok(None) };
        let eqs: Vec<T<'h>> = rules
            .iter()
            .filter_map(|&r| match h.view(r) {
                View::App { args: &[a, b], .. } => Some(h.call(h.sym("equal"), &[a, b])),
                _ => None,
            })
            .collect();
        ors.push(if eqs.len() == 1 { eqs[0] } else { h.call(h.sym("and"), &eqs) });
    }
    Ok(Some(match ors.len() {
        0 => cx.bool(false),
        1 => ors[0],
        _ => h.call(h.sym("or"), &ors),
    }))
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
            View::App { args, .. } => stack.extend(args.iter().copied()),
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
