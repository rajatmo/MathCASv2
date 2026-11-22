//! Optimization problems recast for the methods that read them, when neither the one-variable calculus methods
//! nor the decomposition took the problem as given (`minimize`, `maximize` and the others):
//!
//! - **Regions as constraints**: `element([x, y, z], region)` in the constraints becomes the region's membership
//!   condition (`region_member`: an infinite plane is its equations, a polygon its inequalities); the statements
//!   that the coordinates are real are dropped (the variables of an optimization are real).
//! - **A vector variable in a region** (`minimize(f, element(v, ball()))`): `v` becomes the list of its
//!   coordinates (as many as the region's embedding dimension), the membership a constraint.
//! - **Objectives with cases** (`boole`, `piecewise`): `piecewise_expand` writes the objective as values on
//!   disjoint conditions `c1`, `not c1 and c2`, ..., and the default on the rest; each case is a problem of its own
//!   (its value on its condition and the constraints), and the best case wins. Values are compared exactly
//!   (`less` and `equal` must decide); among equal values an attained one wins.
//! - **Two periodic parts with incommensurable periods** on the whole line: [`crate::oscillation`].
//! - **Variables absent from the objective**: they are projected out of the constraints first (`resolve` of
//!   `exists(rest, cond)`, by the decomposition), the problem in the objective's variables is solved, and a point
//!   for the rest comes from `find_instance` of the constraints at the point found. The extreme over the
//!   projection equals the extreme over the feasible set, as the objective does not see the projected variables.
//!
//! Each sub-problem is a call of the same family, so its answer carries that method's own proof; this module only
//! composes them (the minimum over a finite union of sets is the least of the minima; Basu, Pollack and Roy,
//! *Algorithms in Real Algebraic Geometry*, section 14.1, for the projection).
use crate::front::Front;
use crate::optimize::Goal;
use mc_api::{names, Store, View, T};
use mc_eval::{NativeCx, Res};

/// The best value found and its point (`indeterminate` coordinates when the extreme is not attained).
struct Found<'h> {
    value: T<'h>,
    point: Vec<T<'h>>,
}

/// `goal(args)` recast (see the module documentation); `None` when no recasting applies or a part has no answer.
pub(crate) fn recast<'h>(cx: &mut NativeCx<'_, '_, 'h>, goal: Goal, args: &[T<'h>]) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    let fr = Front::new(h);
    let [fc, xs] = args else { return Ok(None) };
    if let Some(r) = vector_variable(cx, &fr, goal, *fc, *xs)? {
        return Ok(Some(r));
    }
    let listed = fr.list(*xs).is_some();
    let Some(vars) = fr.qvars(*xs) else { return Ok(None) };
    if vars.is_empty() {
        return Ok(None);
    }
    let (f, cond) = match fr.list(*fc) {
        Some([f, c]) => (*f, Some(*c)),
        Some(_) => return Ok(None),
        None => (*fc, None),
    };
    // regions as constraints: the same call with the membership conditions
    if let Some(c) = cond {
        if let Some(c2) = regions(cx, &fr, c)? {
            let name = goal_name(goal);
            let pair = cx.list(&[f, c2]);
            let r = cx.eval(h.call(h.sym(name), &[pair, *xs]))?;
            return Ok(answered(&fr, r, name).then_some(r));
        }
    }
    let found = if cond.is_none() && vars.len() == 1 && !has_cases(&fr, f) {
        // two periodic parts with incommensurable periods on the whole line
        crate::oscillation::two_periods(cx, &fr, goal.max(), f, vars[0])?.map(|(value, p)| Found { value, point: vec![p] })
    } else if has_cases(&fr, f) {
        cases(cx, &fr, goal.max(), f, cond, &vars)?
    } else {
        project(cx, &fr, goal.max(), f, cond, &vars)?
    };
    let Some(found) = found else { return Ok(None) };
    let rules: Vec<T<'h>> = vars.iter().zip(&found.point).map(|(&x, &p)| h.call(h.sym(names::RULE), &[x, p])).collect();
    let out = match goal {
        Goal::Minimize | Goal::Maximize => {
            let rl = cx.list(&rules);
            cx.list(&[found.value, rl])
        }
        Goal::MinValue | Goal::MaxValue => found.value,
        Goal::ArgMin | Goal::ArgMax => {
            if listed {
                cx.list(&found.point)
            } else {
                found.point[0]
            }
        }
    };
    Ok(Some(cx.eval(out)?))
}

/// `goal(f, element(v, region))` and `goal([f, cond], element(v, region))` with a symbol `v`: a vector variable
/// of the region's embedding dimension n. The problem is solved in n coordinates (`v` replaced by their list, so
/// `indexed(v, i)` and `norm(v)` evaluate) with the membership as a constraint; the point is the vector.
fn vector_variable<'h>(cx: &mut NativeCx<'_, '_, 'h>, fr: &Front<'h>, goal: Goal, fc: T<'h>, xs: T<'h>) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    let Some((n, [v, region])) = fr.head(xs) else { return Ok(None) };
    if n != "element" || fr.head(*region).is_none() {
        return Ok(None);
    }
    let Some(name) = fr.sym_name(*v) else { return Ok(None) };
    let dim = cx.eval(h.call(h.sym("region_embedding_dimension"), &[*region]))?;
    let dim = match h.view(dim) {
        View::Int(d) if (1..=6).contains(&d) => d as usize,
        _ => return Ok(None),
    };
    let coords: Vec<T<'h>> = (1..=dim).map(|i| h.sym_term(h.sym(&format!("{name}$coord{i}")))).collect();
    let vec = cx.list(&coords);
    let fc = cx.eval(put_vector(h, fc, *v, vec))?;
    let member = h.call(h.sym("element"), &[vec, *region]);
    let (f, cond) = match fr.list(fc) {
        Some([f, c]) => (*f, h.call(h.sym("and"), &[*c, member])),
        Some(_) => return Ok(None),
        None => (fc, member),
    };
    if !free_of(h, f, *v) || !free_of(h, cond, *v) {
        return Ok(None);
    }
    let Some(found) = sub(cx, fr, goal.max(), f, Some(cond), &coords)? else { return Ok(None) };
    let point = if attained(fr, &found) { cx.list(&found.point) } else { h.sym_term(h.sym("indeterminate")) };
    let out = match goal {
        Goal::Minimize | Goal::Maximize => {
            let r = h.call(h.sym(names::RULE), &[*v, point]);
            let rl = cx.list(&[r]);
            cx.list(&[found.value, rl])
        }
        Goal::MinValue | Goal::MaxValue => found.value,
        Goal::ArgMin | Goal::ArgMax => point,
    };
    Ok(Some(cx.eval(out)?))
}

/// `t` with the vector `vec` for the symbol `v`; `abs(v)` (which `norm(v)` of a symbol evaluates to) becomes
/// `norm(vec)`, the length of the vector.
fn put_vector<'h>(h: &'h mc_core::Heap, t: T<'h>, v: T<'h>, vec: T<'h>) -> T<'h> {
    if t == v {
        return vec;
    }
    let View::App { head, args } = h.view(t) else { return t };
    if args == [v] && matches!(h.view(head), View::Sym(s) if h.sym_name(s) == "abs") {
        return h.call(h.sym("norm"), &[vec]);
    }
    let new: Vec<T<'h>> = args.iter().map(|&a| put_vector(h, a, v, vec)).collect();
    if new.iter().zip(args).all(|(a, b)| a == b) {
        t
    } else {
        h.app(head, &new)
    }
}

fn goal_name(goal: Goal) -> &'static str {
    match goal {
        Goal::Minimize => "minimize",
        Goal::Maximize => "maximize",
        Goal::MinValue => "min_value",
        Goal::MaxValue => "max_value",
        Goal::ArgMin => "arg_min",
        Goal::ArgMax => "arg_max",
    }
}

/// Did a call of `name` give an answer (not itself unevaluated, not a failure)?
fn answered<'h>(fr: &Front<'h>, r: T<'h>, name: &str) -> bool {
    !matches!(fr.head(r), Some((n, _)) if n == name || n == names::FAILURE)
}

/// Is `x` absent from `t`?
fn free_of<'h>(h: &'h mc_core::Heap, t: T<'h>, x: T<'h>) -> bool {
    let mut stack = mc_api::walk::Walk::new(t);
    while let Some(u) = stack.pop() {
        if u == x {
            return false;
        }
        if let View::App { head, args } = h.view(u) {
            stack.push(head);
            stack.extend(args.iter().copied());
        }
    }
    true
}

/// Does `t` hold `boole` or `piecewise`?
fn has_cases<'h>(fr: &Front<'h>, t: T<'h>) -> bool {
    let h = fr.ar.store();
    let mut stack = mc_api::walk::Walk::new(t);
    while let Some(u) = stack.pop() {
        if let Some((n, args)) = fr.head(u) {
            if n == "boole" || n == "piecewise" {
                return true;
            }
            stack.extend(args.iter().copied());
        } else if let View::App { args, .. } = h.view(u) {
            stack.extend(args.iter().copied());
        }
    }
    false
}

/// The constraints with every `element(points, region)` of a region replaced by its membership condition, and
/// `element(_, reals)` dropped (also inside `not`, `implies` and quantifiers); `None` when there is no region or a
/// region gives no condition.
pub(crate) fn regions<'h>(cx: &mut NativeCx<'_, '_, 'h>, fr: &Front<'h>, c: T<'h>) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    let Some((name, args)) = fr.head(c) else { return Ok(None) };
    match (name.as_str(), args) {
        ("element", [pts, region]) if fr.list(*pts).is_some() && fr.head(*region).is_some() => {
            let m = cx.eval(h.call(h.sym("region_member"), &[*region, *pts]))?;
            if !answered(fr, m, "region_member") || has_region(fr, m) {
                return Ok(None);
            }
            Ok(Some(drop_reals(cx, fr, m)?))
        }
        ("and" | "or" | "not" | "implies" | "exists" | "for_all", _) => {
            let mut any = false;
            let mut new = vec![];
            for &a in args {
                match regions(cx, fr, a)? {
                    Some(b) => {
                        any = true;
                        new.push(b);
                    }
                    None => new.push(a),
                }
            }
            if !any {
                return Ok(None);
            }
            let t = h.call(h.sym(&name), &new);
            Ok(Some(cx.eval(t)?))
        }
        _ => Ok(None),
    }
}

/// Does the condition still hold a membership in a region (not a number domain)?
fn has_region<'h>(fr: &Front<'h>, t: T<'h>) -> bool {
    let mut stack = mc_api::walk::Walk::new(t);
    while let Some(u) = stack.pop() {
        if let Some((n, args)) = fr.head(u) {
            if n == "element" && args.len() == 2 && fr.head(args[1]).is_some() {
                return true;
            }
            stack.extend(args.iter().copied());
        }
    }
    false
}

/// A membership condition without its conjuncts `element(_, reals)`.
fn drop_reals<'h>(cx: &mut NativeCx<'_, '_, 'h>, fr: &Front<'h>, m: T<'h>) -> Res<T<'h>> {
    let h = cx.heap();
    let reals = |t: T<'h>| matches!(fr.head(t), Some((n, [_, d])) if n == "element" && fr.sym_name(*d).as_deref() == Some("reals"));
    if reals(m) {
        return Ok(h.sym_term(h.sym(names::TRUE)));
    }
    match fr.head(m) {
        Some((n, args)) if n == "and" => {
            let kept: Vec<T<'h>> = args.iter().copied().filter(|&a| !reals(a)).collect();
            let t = h.call(h.sym("and"), &kept);
            cx.eval(t)
        }
        _ => Ok(m),
    }
}

/// `maximize`/`minimize` of `[f, cond]` in `vars`: the value and the point (`None` without an answer).
fn sub<'h>(cx: &mut NativeCx<'_, '_, 'h>, fr: &Front<'h>, max: bool, f: T<'h>, cond: Option<T<'h>>, vars: &[T<'h>]) -> Res<Option<Found<'h>>> {
    let h = cx.heap();
    let name = if max { "maximize" } else { "minimize" };
    let fc = match cond {
        Some(c) => cx.list(&[f, c]),
        None => f,
    };
    let xs = cx.list(vars);
    let r = cx.eval(h.call(h.sym(name), &[fc, xs]))?;
    let Some([value, rules]) = fr.list(r) else { return Ok(None) };
    let Some(rules) = fr.list(*rules) else { return Ok(None) };
    let mut point = vec![];
    for (&x, &rl) in vars.iter().zip(rules) {
        match fr.head(rl) {
            Some((n, [y, v])) if n == names::RULE && *y == x => point.push(*v),
            _ => return Ok(None),
        }
    }
    if point.len() != vars.len() {
        return Ok(None);
    }
    Ok(Some(Found { value: *value, point }))
}

/// Is the extreme attained (no `indeterminate` coordinate)?
fn attained<'h>(fr: &Front<'h>, p: &Found<'h>) -> bool {
    p.point.iter().all(|&c| fr.sym_name(c).as_deref() != Some("indeterminate"))
}

/// Is `a` strictly better than `b` (smaller for a minimum)? `None` when the comparison is not decided.
fn strictly_better<'h>(cx: &mut NativeCx<'_, '_, 'h>, max: bool, a: T<'h>, b: T<'h>) -> Res<Option<bool>> {
    let h = cx.heap();
    if a == b {
        return Ok(Some(false));
    }
    let (lo, hi) = if max { (b, a) } else { (a, b) };
    let lt = cx.eval(h.call(h.sym("less"), &[lo, hi]))?;
    if cx.is_true(lt) {
        return Ok(Some(true));
    }
    if cx.is_false(lt) {
        return Ok(Some(false));
    }
    Ok(None)
}

/// The best of two answers (an attained one among equal values).
fn best_of<'h>(cx: &mut NativeCx<'_, '_, 'h>, fr: &Front<'h>, max: bool, a: Found<'h>, b: Found<'h>) -> Res<Option<Found<'h>>> {
    let Some(ab) = strictly_better(cx, max, a.value, b.value)? else { return Ok(None) };
    if ab {
        return Ok(Some(a));
    }
    let Some(ba) = strictly_better(cx, max, b.value, a.value)? else { return Ok(None) };
    if ba {
        return Ok(Some(b));
    }
    // equal values
    Ok(Some(if attained(fr, &a) || !attained(fr, &b) { a } else { b }))
}

/// The objective's cases from `piecewise_expand`, each a problem of its own; the best wins.
fn cases<'h>(cx: &mut NativeCx<'_, '_, 'h>, fr: &Front<'h>, max: bool, f: T<'h>, cond: Option<T<'h>>, vars: &[T<'h>]) -> Res<Option<Found<'h>>> {
    let h = cx.heap();
    let pw = cx.eval(h.call(h.sym("piecewise_expand"), &[f]))?;
    let Some((n, args)) = fr.head(pw) else { return Ok(None) };
    if n != "piecewise" {
        return Ok(None);
    }
    let (pairs, default) = match args {
        [p] => (*p, h.int(0)),
        [p, d] => (*p, *d),
        _ => return Ok(None),
    };
    let Some(pairs) = fr.list(pairs) else { return Ok(None) };
    if pairs.len() > 8 {
        return Ok(None);
    }
    let mut items: Vec<(T<'h>, T<'h>)> = vec![];
    let mut before: Vec<T<'h>> = vec![];
    for &p in pairs {
        let Some([v, c]) = fr.list(p) else { return Ok(None) };
        let mut conj: Vec<T<'h>> = before.iter().map(|&b| h.call(h.sym("not"), &[b])).collect();
        conj.push(*c);
        items.push((*v, h.call(h.sym("and"), &conj)));
        before.push(*c);
    }
    let rest: Vec<T<'h>> = before.iter().map(|&b| h.call(h.sym("not"), &[b])).collect();
    items.push((default, h.call(h.sym("and"), &rest)));
    let mut best: Option<Found<'h>> = None;
    for (v, c) in items {
        // a case must not hold cases again (no endless recasting)
        if has_cases(fr, v) || has_cases(fr, c) {
            return Ok(None);
        }
        let c = match cond {
            Some(k) => h.call(h.sym("and"), &[c, k]),
            None => c,
        };
        let c = cx.eval(c)?;
        if cx.is_false(c) {
            continue;
        }
        let Some(found) = sub(cx, fr, max, v, Some(c), vars)? else { return Ok(None) };
        best = match best {
            None => Some(found),
            Some(b) => match best_of(cx, fr, max, b, found)? {
                Some(x) => Some(x),
                None => return Ok(None),
            },
        };
    }
    Ok(best)
}

/// The variables the objective does not hold projected out of the constraints first.
fn project<'h>(cx: &mut NativeCx<'_, '_, 'h>, fr: &Front<'h>, max: bool, f: T<'h>, cond: Option<T<'h>>, vars: &[T<'h>]) -> Res<Option<Found<'h>>> {
    let h = cx.heap();
    let Some(cond) = cond else { return Ok(None) };
    let (inner, rest): (Vec<T<'h>>, Vec<T<'h>>) = vars.iter().partition(|&&x| !free_of(h, f, x));
    if inner.is_empty() || rest.is_empty() || fr.has_quantifier(cond) {
        return Ok(None);
    }
    let rl = cx.list(&rest);
    let ex = h.call(h.sym("exists"), &[rl, cond]);
    let proj = cx.eval(h.call(h.sym("resolve"), &[ex]))?;
    if !answered(fr, proj, "resolve") || fr.has_quantifier(proj) || rest.iter().any(|&y| !free_of(h, proj, y)) {
        return Ok(None);
    }
    let Some(found) = sub(cx, fr, max, f, Some(proj), &inner)? else { return Ok(None) };
    let ind = h.sym_term(h.sym("indeterminate"));
    let mut rest_point = vec![ind; rest.len()];
    if attained(fr, &found) {
        // a point of the projected variables over the point found
        let rules: Vec<T<'h>> = inner.iter().zip(&found.point).map(|(&x, &p)| h.call(h.sym(names::RULE), &[x, p])).collect();
        let rules = cx.list(&rules);
        let at = cx.eval(h.call(h.sym("rewrite"), &[cond, rules]))?;
        let fi = cx.eval(h.call(h.sym("find_instance"), &[at, rl]))?;
        let Some([sol]) = fr.list(fi) else { return Ok(None) };
        let Some(sol) = fr.list(*sol) else { return Ok(None) };
        for (k, &y) in rest.iter().enumerate() {
            let Some(v) = sol.iter().find_map(|&r| match fr.head(r) {
                Some((n, [a, v])) if n == names::RULE && *a == y => Some(*v),
                _ => None,
            }) else {
                return Ok(None);
            };
            rest_point[k] = v;
        }
        // the check: the constraints hold at the whole point
        let all: Vec<T<'h>> = inner
            .iter()
            .zip(&found.point)
            .chain(rest.iter().zip(&rest_point))
            .map(|(&x, &p)| h.call(h.sym(names::RULE), &[x, p]))
            .collect();
        let all = cx.list(&all);
        let ok = cx.eval(h.call(h.sym("rewrite"), &[cond, all]))?;
        if !cx.is_true(ok) {
            return Ok(None);
        }
    }
    let point = vars
        .iter()
        .map(|x| match inner.iter().position(|y| y == x) {
            Some(i) => found.point[i],
            None => rest_point[rest.iter().position(|y| y == x).expect("a variable is inner or rest")],
        })
        .collect();
    Ok(Some(Found { value: found.value, point }))
}
