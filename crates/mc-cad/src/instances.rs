//! `find_instance` over subsets of the integers and over the primes, for conditions the polynomial methods do not
//! read.
//!
//! - **Integer subsets**: `positive_integers`, `nonnegative_integers`, `negative_integers` and `nonpositive_integers`
//!   are the integers with a sign condition on every variable; the problem goes to the integer methods with those
//!   conditions added.
//! - **Sums of squares** `x1^2 + ... + xk^2 == N` (N a nonnegative integer): a depth-first search, largest square
//!   first, in nonincreasing order; with one square left the remainder must be a square. Four squares always suffice
//!   (Lagrange), and the greedy choice nearly always leaves a representable remainder, so the search ends after a few
//!   steps even for N near 10^18 (Niven, Zuckerman and Montgomery, *An Introduction to the Theory of Numbers*,
//!   section 6.4). The signs follow the domain.
//! - **Bounded integer problems with other functions** (`euler_phi`, `power_mod`, ...): the box of the real solution
//!   set of the polynomial conjuncts (a superset of the solutions), then its integer points in order, the whole
//!   condition evaluated at each.
//! - **Primes**: the primes from the lower bound that a conjunct `p > c` (or `p >= c`) gives, in increasing order,
//!   the condition evaluated at each; an upper bound `p < c` ends the search (then it is complete).
//!
//! Every point is returned only when the whole condition evaluates to `true` there (exact substitution).
use crate::front::Front;
use crate::natives::metered;
use mc_api::{names, Store, View, T};
use mc_core::Heap;
use mc_eval::{NativeCx, Res};
use mc_poly::Z;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// The integer square root of `v` (the largest `s` with `s^2 <= v`).
fn isqrt_u128(v: u128) -> u128 {
    let mut s = (v as f64).sqrt() as u128;
    while s > 0 && s.checked_mul(s).is_none_or(|q| q > v) {
        s -= 1;
    }
    while (s + 1).checked_mul(s + 1).is_some_and(|q| q <= v) {
        s += 1;
    }
    s
}

/// The most nodes of the sum-of-squares search.
const MAX_SQUARE_NODES: u64 = 200_000;
/// The most candidates tried for one square while the remainder is large (above `FULL_SEARCH`).
const TOP_CANDIDATES: u128 = 64;
/// Remainders up to this are searched completely.
const FULL_SEARCH: u128 = 1 << 20;
/// The most integer points of a box evaluated one by one.
const MAX_BOX_POINTS: u64 = 40_000;
/// The most primes tried.
const MAX_PRIMES: usize = 20_000;

/// The sign condition of an integer subset: `>`, `>=`, `<`, `<=` against 0.
fn subset_relation(name: &str) -> Option<&'static str> {
    Some(match name {
        "positive_integers" => "greater",
        "nonnegative_integers" => "greater_equal",
        "negative_integers" => "less",
        "nonpositive_integers" => "less_equal",
        _ => return None,
    })
}

/// The sign condition `rel` of an integer variable `v`, as a bound that holds for integers: `v > 0` is `v >= 1`
/// and `v < 0` is `v <= -1` (so that the real relaxation keeps the bound).
fn sign_condition<'h>(h: &'h Heap, rel: &str, v: T<'h>) -> T<'h> {
    match rel {
        "greater" => h.call(h.sym("greater_equal"), &[v, h.int(1)]),
        "less" => h.call(h.sym("less_equal"), &[v, h.int(-1)]),
        _ => h.call(h.sym(rel), &[v, h.int(0)]),
    }
}

/// The count argument of `find_instance` (1 when absent); `None` when it is not a count.
fn count_of<'h>(h: &'h Heap, k: Option<&T<'h>>) -> Option<usize> {
    match k {
        None => Some(1),
        Some(&k) => match h.view(k) {
            View::Int(k) if (1..=10_000).contains(&k) => Some(k as usize),
            _ => None,
        },
    }
}

/// The conjuncts of a condition (a list of conditions is their conjunction), each once: a shared subterm is read
/// once, so a condition that is a graph of shared lists ends.
fn conjuncts<'h>(fr: &Front<'h>, c: T<'h>) -> Vec<T<'h>> {
    fn walk<'h>(fr: &Front<'h>, c: T<'h>, seen: &mut std::collections::HashSet<T<'h>>, out: &mut Vec<T<'h>>) {
        if !seen.insert(c) {
            return;
        }
        if let Some(items) = fr.list(c) {
            return items.iter().for_each(|&t| walk(fr, t, seen, out));
        }
        match fr.head(c) {
            Some((n, args)) if n == "and" => args.iter().for_each(|&t| walk(fr, t, seen, out)),
            _ => out.push(c),
        }
    }
    let mut out = vec![];
    walk(fr, c, &mut Default::default(), &mut out);
    out
}

/// `find_instance` by the methods of this module; `None` when none applies or none finds the points.
pub fn find<'h>(cx: &mut NativeCx<'_, '_, 'h>, stop: &Arc<AtomicBool>, args: &[T<'h>]) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    let fr = Front::new(h);
    let (cond, xs, dom, k) = match args {
        [c, x, d, rest @ ..] if rest.len() <= 1 && fr.sym_name(*d).is_some_and(|n| n.ends_with("integers") || n == "primes") => {
            (*c, *x, fr.sym_name(*d), rest.first())
        }
        [c, x, rest @ ..] if rest.len() <= 1 && !rest.first().is_some_and(|&d| fr.sym_name(d).is_some()) => (*c, *x, None, rest.first()),
        _ => return Ok(None),
    };
    let Some(n) = count_of(h, k) else { return Ok(None) };
    let Some(vars) = fr.qvars(xs) else { return Ok(None) };
    if fr.has_quantifier(cond) {
        return Ok(None);
    }
    let dom = dom.as_deref();
    // the primes: named as the domain, or `element(p, primes)` among the conjuncts of a one-variable condition
    let cs = conjuncts(&fr, cond);
    let prime_declared = |v: T<'h>| {
        cs.iter().any(|&c| matches!(fr.head(c), Some((e, &[a, d])) if e == "element" && a == v && fr.sym_name(d).as_deref() == Some("primes")))
    };
    if let [x] = vars.as_slice() {
        if dom == Some("primes") || (dom.is_none() && prime_declared(*x)) {
            return primes(cx, &cs, *x, n);
        }
    }
    let Some(dom) = dom else { return Ok(None) };
    if dom == "primes" {
        return Ok(None);
    }
    let sign = if dom == "integers" { None } else { Some(subset_relation(dom)) };
    if sign == Some(None) {
        return Ok(None);
    }
    let sign = sign.flatten();
    if let Some(r) = sum_of_squares(cx, stop, &cs, &vars, sign, n)? {
        return Ok(Some(r));
    }
    if let Some(rel) = sign {
        // the subset as integers with a sign condition on each variable
        let mut parts = cs.clone();
        parts.extend(vars.iter().map(|&v| sign_condition(h, rel, v)));
        let c2 = h.call(h.sym("and"), &parts);
        let mut a = vec![c2, xs, h.sym_term(h.sym("integers"))];
        a.extend(k.copied());
        let r = cx.eval(h.call(h.sym("find_instance"), &a))?;
        return Ok(cx.as_list(r).is_some().then_some(r));
    }
    if vars.len() >= 2 {
        return integer_box(cx, stop, cond, &cs, &vars, n);
    }
    Ok(None)
}

/// Does the condition hold at the point (the whole condition evaluated after substitution is `true`)?
fn holds_at<'h>(cx: &mut NativeCx<'_, '_, 'h>, cond: T<'h>, rules: T<'h>) -> Res<bool> {
    let h = cx.heap();
    let t = cx.eval(h.call(h.sym("rewrite"), &[cond, rules]))?;
    Ok(cx.is_true(t))
}

/// A nonnegative integer term as a `u128`.
fn as_u128<'h>(h: &'h Heap, t: T<'h>) -> Option<u128> {
    match h.view(t) {
        View::Int(v) if v >= 0 => Some(v as u128),
        View::BigInt { neg: false, mag } if mag.len() <= 2 => Some(mag.iter().rev().fold(0u128, |a, &w| (a << 64) | w as u128)),
        _ => None,
    }
}

/// `x1^2 + ... + xk^2 == N` with every listed variable squared once: up to `n` points (nonincreasing absolute
/// values in the order of the variables, signs from the domain), each checked by substitution.
fn sum_of_squares<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    stop: &Arc<AtomicBool>,
    cs: &[T<'h>],
    vars: &[T<'h>],
    sign: Option<&str>,
    n: usize,
) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    let fr = Front::new(h);
    let [c] = cs else { return Ok(None) };
    let Some((e, &[l, r])) = fr.head(*c) else { return Ok(None) };
    if e != "equal" || n != 1 {
        return Ok(None);
    }
    let squares_of = |t: T<'h>| -> Option<Vec<T<'h>>> {
        let terms = match fr.head(t) {
            Some((a, args)) if a == "add" => args.to_vec(),
            _ => vec![t],
        };
        terms
            .iter()
            .map(|&u| match fr.head(u) {
                Some((p, &[b, two])) if p == "pow" && matches!(h.view(two), View::Int(2)) => Some(b),
                _ => None,
            })
            .collect()
    };
    let (bases, total) = match (squares_of(l), as_u128(h, r), squares_of(r), as_u128(h, l)) {
        (Some(b), Some(t), _, _) | (_, _, Some(b), Some(t)) => (b, t),
        _ => return Ok(None),
    };
    if bases.len() != vars.len() || !vars.iter().all(|v| bases.contains(v)) || total >= 1u128 << 120 {
        return Ok(None);
    }
    let nonzero = matches!(sign, Some("greater" | "less"));
    let negative = matches!(sign, Some("less" | "less_equal"));
    let k = vars.len();
    let found = metered(cx, stop, |m| {
        let mut out = vec![];
        let mut nodes = 0u64;
        let ok = squares(total, k, u128::MAX, nonzero, &mut out, &mut nodes, m)?;
        Ok(ok.then_some(out))
    })?;
    let Some(Some(vals)) = found else { return Ok(None) };
    let rules: Vec<T<'h>> = vars
        .iter()
        .zip(&vals)
        .map(|(&x, &v)| {
            let z = Z::from_i128(if negative { -(v as i128) } else { v as i128 });
            h.call(h.sym(names::RULE), &[x, fr.ar.integer(&z.to_int())])
        })
        .collect();
    let pt = cx.list(&rules);
    if !holds_at(cx, *c, pt)? {
        return Ok(None);
    }
    if let Some(rel) = sign {
        for &rule in &rules {
            let View::App { args: &[x, v], .. } = h.view(rule) else { return Ok(None) };
            let t = cx.eval(h.call(h.sym(rel), &[v, h.int(0)]))?;
            if !cx.is_true(t) || !vars.contains(&x) {
                return Ok(None);
            }
        }
    }
    Ok(Some(cx.list(&[pt])))
}

/// `r` as a sum of `k` squares of integers at most `cap` (at least 1 when `nonzero`), largest first, into `out`;
/// `false` when the search finds none within its node budget.
fn squares(r: u128, k: usize, cap: u128, nonzero: bool, out: &mut Vec<u128>, nodes: &mut u64, m: &mut mc_poly::work::Meter) -> mc_poly::work::WorkResult<bool> {
    *nodes += 1;
    m.step(1)?;
    if *nodes > MAX_SQUARE_NODES {
        return Ok(false);
    }
    if k == 0 {
        return Ok(r == 0);
    }
    let hi = isqrt_u128(r).min(cap);
    if k == 1 {
        if hi * hi == r && (!nonzero || r > 0) {
            out.push(hi);
            return Ok(true);
        }
        return Ok(false);
    }
    // the largest square is at least r / k
    let lo_sq = r.div_ceil(k as u128);
    let mut lo = isqrt_u128(lo_sq);
    if lo * lo < lo_sq {
        lo += 1;
    }
    lo = lo.max(if nonzero { 1 } else { 0 });
    let lo = if r > FULL_SEARCH { lo.max(hi.saturating_sub(TOP_CANDIDATES)) } else { lo };
    let mut a = hi;
    while a >= lo {
        out.push(a);
        if squares(r - a * a, k - 1, a, nonzero, out, nodes, m)? {
            return Ok(true);
        }
        out.pop();
        if a == 0 || *nodes > MAX_SQUARE_NODES {
            break;
        }
        a -= 1;
    }
    Ok(false)
}

/// Is `c` a relation between polynomials in `vars` alone?
fn polynomial_in<'h>(fr: &Front<'h>, c: T<'h>, vars: &[T<'h>], m: &mut mc_poly::work::Meter) -> mc_poly::work::WorkResult<bool> {
    Ok(match fr.problem(c, vars, m)? {
        Ok(p) => p.prob.nv == vars.len() && p.prob.nfree == vars.len() && !p.quantified,
        Err(_) => false,
    })
}

/// Integer points of the box of the polynomial conjuncts' real solution set, in lexicographic order, at which the
/// whole condition is true: the first `n`; all of them (possibly none) when the box is searched completely.
fn integer_box<'h>(cx: &mut NativeCx<'_, '_, 'h>, stop: &Arc<AtomicBool>, cond: T<'h>, cs: &[T<'h>], vars: &[T<'h>], n: usize) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    let fr = Front::new(h);
    let bx = metered(cx, stop, |m| {
        let mut poly = vec![];
        let mut other = false;
        for &c in cs {
            if polynomial_in(&fr, c, vars, m)? {
                poly.push(c);
            } else {
                other = true;
            }
        }
        // only conditions that the polynomial methods do not read whole
        if poly.is_empty() || !other {
            return Ok(None);
        }
        let pc = if poly.len() == 1 { poly[0] } else { h.call(h.sym("and"), &poly) };
        crate::integers::box_of(&fr, pc, vars, m)
    })?;
    let Some(Some(bx)) = bx else { return Ok(None) };
    let Some(bx) = bx else { return Ok(Some(cx.list(&[]))) };
    let mut size: u64 = 1;
    for (lo, hi) in &bx {
        if hi < lo {
            return Ok(Some(cx.list(&[])));
        }
        size = size.saturating_mul((hi - lo + 1) as u64);
    }
    if size > MAX_BOX_POINTS {
        return Ok(None);
    }
    let mut pt: Vec<i64> = bx.iter().map(|b| b.0).collect();
    let mut out = vec![];
    loop {
        cx.tick(1)?;
        let rules: Vec<T<'h>> = vars.iter().zip(&pt).map(|(&x, &v)| h.call(h.sym(names::RULE), &[x, h.int(v)])).collect();
        let rl = cx.list(&rules);
        if holds_at(cx, cond, rl)? {
            out.push(rl);
            if out.len() == n {
                break;
            }
        }
        // the next point, the last variable fastest
        let mut i = pt.len();
        loop {
            if i == 0 {
                return Ok(Some(cx.list(&out)));
            }
            i -= 1;
            if pt[i] < bx[i].1 {
                pt[i] += 1;
                for j in i + 1..pt.len() {
                    pt[j] = bx[j].0;
                }
                break;
            }
        }
    }
    Ok(Some(cx.list(&out)))
}

/// `minimize(f, vars, integers)` (and the subsets of the integers, and `element(x | y, integers)` among the
/// conditions) for the other goals too: the real relaxation first (the conditions with the sign conditions of the
/// domain, minimized over the reals); a minimum attained at an integer point is the integer minimum. Otherwise, when
/// the real feasible set is bounded, every integer point of its box is tried (the condition and the objective
/// evaluated exactly) and the smallest value wins, the first point in lexicographic order among equal ones.
pub fn optimize_integers<'h>(cx: &mut NativeCx<'_, '_, 'h>, stop: &Arc<AtomicBool>, goal: crate::optimize::Goal, args: &[T<'h>]) -> Res<Option<T<'h>>> {
    use crate::optimize::Goal;
    let h = cx.heap();
    let fr = Front::new(h);
    let (fc, xs, dom) = match args {
        [fc, xs, d] => match fr.sym_name(*d) {
            Some(n) if n == "integers" || subset_relation(&n).is_some() => (*fc, *xs, n),
            _ => return Ok(None),
        },
        [fc, xs] => (*fc, *xs, String::new()),
        _ => return Ok(None),
    };
    let Some(vars) = fr.qvars(xs) else { return Ok(None) };
    let (f, cond) = match fr.list(fc) {
        Some([f]) => (*f, None),
        Some([f, c]) => (*f, Some(*c)),
        Some(_) => return Ok(None),
        None => (fc, None),
    };
    let mut cs = cond.map(|c| conjuncts(&fr, c)).unwrap_or_default();
    // `element(x | y, integers)` covering every variable stands for the domain
    let mut dom = dom;
    if dom.is_empty() {
        let mut ints = vec![];
        cs.retain(|&c| match fr.head(c) {
            Some((e, &[v, d])) if e == "element" && fr.sym_name(d).as_deref() == Some("integers") => {
                match fr.head(v) {
                    Some((a, vs)) if a == "alternatives" => ints.extend_from_slice(vs),
                    _ => ints.push(v),
                }
                false
            }
            _ => true,
        });
        if ints.is_empty() || !vars.iter().all(|v| ints.contains(v)) {
            return Ok(None);
        }
        dom = "integers".to_string();
    }
    if fr.has_quantifier(f) || cs.iter().any(|&c| fr.has_quantifier(c)) {
        return Ok(None);
    }
    if let Some(rel) = subset_relation(&dom) {
        cs.extend(vars.iter().map(|&v| sign_condition(h, rel, v)));
    }
    let cond = match cs.len() {
        0 => h.sym_term(h.sym(names::TRUE)),
        1 => cs[0],
        _ => h.call(h.sym("and"), &cs),
    };
    let is_int = |t: T<'h>| matches!(h.view(t), View::Int(_) | View::BigInt { .. });
    let is_exact = |t: T<'h>| matches!(h.view(t), View::Int(_) | View::BigInt { .. } | View::Rational { .. });
    let opt = if goal.max() { "maximize" } else { "minimize" };
    // the real relaxation
    let relax = cx.eval(h.call(h.sym(opt), &[cx.list(&[f, cond]), cx.list(&vars)]))?;
    let mut best: Option<(T<'h>, Vec<T<'h>>)> = None;
    if let Some(&[v, rules]) = cx.as_list(relax) {
        if let Some(rs) = cx.as_list(rules) {
            let vals: Vec<T<'h>> = vars
                .iter()
                .filter_map(|&x| rs.iter().find_map(|&r| match h.view(r) {
                    View::App { args: &[a, b], .. } if a == x => Some(b),
                    _ => None,
                }))
                .collect();
            if vals.len() == vars.len() && vals.iter().all(|&b| is_int(b)) && is_exact(v) {
                best = Some((v, vals));
            }
        }
    }
    if best.is_none() {
        // a bounded real feasible set: its integer points
        let bx = metered(cx, stop, |m| {
            let mut poly = true;
            for &c in &cs {
                poly &= polynomial_in(&fr, c, &vars, m)?;
            }
            if !poly || cs.is_empty() {
                return Ok(None);
            }
            crate::integers::box_of(&fr, cond, &vars, m)
        })?;
        let Some(Some(bx)) = bx else { return Ok(None) };
        let Some(bx) = bx else { return Ok(None) };
        let mut size: u64 = 1;
        for (lo, hi) in &bx {
            if hi < lo {
                return Ok(None);
            }
            size = size.saturating_mul((hi - lo + 1) as u64);
        }
        if size > MAX_BOX_POINTS {
            return Ok(None);
        }
        let better = if goal.max() { "greater" } else { "less" };
        let mut pt: Vec<i64> = bx.iter().map(|b| b.0).collect();
        'all: loop {
            cx.tick(1)?;
            let rules: Vec<T<'h>> = vars.iter().zip(&pt).map(|(&x, &v)| h.call(h.sym(names::RULE), &[x, h.int(v)])).collect();
            let rl = cx.list(&rules);
            if holds_at(cx, cond, rl)? {
                let v = cx.eval(h.call(h.sym("rewrite"), &[f, rl]))?;
                if !is_exact(v) {
                    return Ok(None);
                }
                let wins = match &best {
                    None => true,
                    Some((b, _)) => {
                        let t = cx.eval(h.call(h.sym(better), &[v, *b]))?;
                        cx.is_true(t)
                    }
                };
                if wins {
                    best = Some((v, pt.iter().map(|&k| h.int(k)).collect()));
                }
            }
            let mut i = pt.len();
            loop {
                if i == 0 {
                    break 'all;
                }
                i -= 1;
                if pt[i] < bx[i].1 {
                    pt[i] += 1;
                    for j in i + 1..pt.len() {
                        pt[j] = bx[j].0;
                    }
                    break;
                }
            }
        }
    }
    let Some((v, vals)) = best else { return Ok(None) };
    let rules: Vec<T<'h>> = vars.iter().zip(&vals).map(|(&x, &b)| h.call(h.sym(names::RULE), &[x, b])).collect();
    // the point satisfies the condition and gives the value (exact substitution)
    let rl = cx.list(&rules);
    if !holds_at(cx, cond, rl)? {
        return Ok(None);
    }
    let fv = cx.eval(h.call(h.sym("rewrite"), &[f, rl]))?;
    let same = cx.eval(h.call(h.sym("equal"), &[fv, v]))?;
    if !cx.is_true(same) {
        return Ok(None);
    }
    Ok(Some(match goal {
        Goal::Minimize | Goal::Maximize => cx.list(&[v, rl]),
        Goal::MinValue | Goal::MaxValue => v,
        Goal::ArgMin | Goal::ArgMax => {
            if fr.list(xs).is_some() {
                cx.list(&vals)
            } else {
                vals[0]
            }
        }
    }))
}

/// Primes satisfying the conjuncts `cs` in `x`, from the lower bound of a conjunct `x > c` or `x >= c` upward: the
/// first `n`; all of them when an upper bound `x < c` or `x <= c` ends the search first.
fn primes<'h>(cx: &mut NativeCx<'_, '_, 'h>, cs: &[T<'h>], x: T<'h>, n: usize) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    let fr = Front::new(h);
    let number = |cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>| -> Res<Option<T<'h>>> {
        let v = cx.eval(t)?;
        Ok(match h.view(v) {
            View::Int(_) | View::BigInt { .. } | View::Rational { .. } => Some(v),
            _ => None,
        })
    };
    let mut lower: Option<T<'h>> = None;
    let mut upper: Option<T<'h>> = None;
    for &c in cs {
        let Some((rel, &[a, b])) = fr.head(c) else { continue };
        let (lo_side, bound) = match rel.as_str() {
            "greater" | "greater_equal" if a == x => (true, b),
            "less" | "less_equal" if b == x => (true, a),
            "less" | "less_equal" if a == x => (false, b),
            "greater" | "greater_equal" if b == x => (false, a),
            _ => continue,
        };
        let Some(v) = number(cx, bound)? else { continue };
        if lo_side {
            // the larger of two lower bounds
            let pick = match lower {
                Some(l) => cx.eval(h.call(h.sym("max"), &[l, v]))?,
                None => v,
            };
            lower = Some(pick);
        } else {
            let pick = match upper {
                Some(u) => cx.eval(h.call(h.sym("min"), &[u, v]))?,
                None => v,
            };
            upper = Some(pick);
        }
    }
    // next_prime of the integer below the bound's ceiling is the first candidate (a prime bound itself included;
    // the condition decides a strict bound)
    let start = match lower {
        Some(l) => {
            let c = h.call(h.sym("ceiling"), &[l]);
            cx.eval(h.call(h.sym("add"), &[c, h.int(-1)]))?
        }
        None => h.int(1),
    };
    let start = cx.eval(h.call(h.sym("max"), &[start, h.int(1)]))?;
    let cond = h.call(h.sym("and"), cs);
    let mut p = start;
    let mut out = vec![];
    for _ in 0..MAX_PRIMES {
        cx.tick(1)?;
        p = cx.eval(h.call(h.sym("next_prime"), &[p]))?;
        if !matches!(h.view(p), View::Int(_) | View::BigInt { .. }) {
            return Ok(None);
        }
        if let Some(u) = upper {
            let past = cx.eval(h.call(h.sym("greater"), &[p, u]))?;
            if cx.is_true(past) {
                return Ok(Some(cx.list(&out)));
            }
        }
        let rl = cx.list(&[h.call(h.sym(names::RULE), &[x, p])]);
        let isp = cx.eval(h.call(h.sym("is_prime"), &[p]))?;
        if cx.is_true(isp) && holds_at(cx, cond, rl)? {
            out.push(rl);
            if out.len() == n {
                return Ok(Some(cx.list(&out)));
            }
        }
    }
    Ok(None)
}
