//! `reduce(cond, x, reals)` (and over the integers) for conditions with step functions of polynomials in x:
//! `floor`, `ceiling`, `integer_part`, `round` (and `mod(a, m)`, `quotient(a, m)`, `fractional_part(a)` for a number
//! m, written through them: `mod(a, m) = a - m floor(a/m)`, `quotient(a, m) = floor(a/m)`, `fractional_part(a) = a -
//! integer_part(a)`).
//!
//! **Bounded.** The numeric bounds lo < x < hi of the condition bound each argument g(x) (interval Horner evaluation,
//! widened), so each step function takes finitely many integer values k. For each choice of the values the condition
//! with the step functions replaced by them, together with the conditions that the steps take those values
//! (`k <= g < k + 1` for floor, `k - 1 < g <= k` for ceiling, for integer_part the floor's condition when k > 0, the
//! ceiling's when k < 0 and `-1 < g < 1` when k = 0, and `k - 1/2 <= g <= k + 1/2` for round with the ends only when
//! k is even, as round takes halves to the even neighbour), is a polynomial condition reduced by the one-variable
//! algorithm; the union of the pieces is reduced once more to merge them. A value range that is too wide (more than
//! [`MAX_CASES`] choices), nested step functions or a non-polynomial argument give no answer here.
//!
//! **Unbounded.** Each step value is its argument minus a bounded error: `floor(g) = g - t` with 0 <= t < 1 (ceiling
//! -1 < t <= 0, integer_part -1 < t < 1, round -1/2 <= t <= 1/2). With the errors as new variables in their closed
//! boxes the condition becomes a polynomial one, and quantifier elimination of the errors gives two sets of x: S,
//! where the condition may hold, and S', where it may fail. Outside S it fails, outside S' it holds, so only
//! U = S and S' is undecided. When U is bounded the answer is the complement of S' together with the bounded
//! algorithm on an interval around U. Otherwise, when every step argument is affine, `a x + b` with rational a, the
//! condition may be periodic: with T the least common multiple of the periods 1/|a| of the steps (2/|a| for round,
//! which moves by n only for even n; integer_part, which rounds towards 0, never), x -> x + T moves every step value
//! by the integer a T, and when that leaves each relation unchanged (checked symbolically, with the step values as
//! new symbols) the condition is T-periodic. The answer is then the one on [0, T) shifted by T c(1)
//! with c(1) an integer (`element(c(1), integers) and x == 7 + 15 c(1)`). Over the integers the points of [0, T)
//! (or of the interval around U) are tried one by one.
use mc_api::{names, Store, View, T};
use mc_eval::{NativeCx, Res};

/// The most choices of the values of the step functions.
const MAX_CASES: usize = 4000;
/// The most integers tried over the integers (points of a period or of a bounded interval).
const MAX_POINTS: i64 = 20_000;
/// The most step functions whose errors become variables of the quantifier elimination.
const MAX_ERRORS: usize = 3;

fn head<'h>(h: &'h impl Store, t: T<'h>) -> Option<String> {
    match h.view(t) {
        View::App { head, .. } => match h.view(head) {
            View::Sym(s) => Some(h.sym_name(s).to_string()),
            _ => None,
        },
        _ => None,
    }
}

fn mentions<'h>(h: &'h impl Store, t: T<'h>, x: T<'h>) -> bool {
    let mut stack = vec![t];
    while let Some(u) = stack.pop() {
        if u == x {
            return true;
        }
        if let View::App { args, .. } = h.view(u) {
            stack.extend(args.iter().copied());
        }
    }
    false
}

const STEPS: &[&str] = &["floor", "ceiling", "integer_part", "round"];

/// `mod`, `quotient`, `fractional_part` of terms in x written through the step functions.
fn normal<'h>(h: &'h impl Store, t: T<'h>, x: T<'h>) -> T<'h> {
    let View::App { head: hd, args } = h.view(t) else { return t };
    let args: Vec<T<'h>> = args.iter().map(|&a| normal(h, a, x)).collect();
    let call = |n: &str, a: &[T<'h>]| h.call(h.sym(n), a);
    let name = head(h, t);
    match (name.as_deref(), &args[..]) {
        (Some("mod"), &[a, m]) if mentions(h, a, x) && !mentions(h, m, x) => {
            let q = call("floor", &[call(names::MUL, &[a, call(names::POW, &[m, h.int(-1)])])]);
            call(names::ADD, &[a, call(names::MUL, &[h.int(-1), m, q])])
        }
        (Some("quotient"), &[a, m]) if mentions(h, a, x) && !mentions(h, m, x) => {
            call("floor", &[call(names::MUL, &[a, call(names::POW, &[m, h.int(-1)])])])
        }
        (Some("fractional_part"), &[a]) if mentions(h, a, x) => call(names::ADD, &[a, call(names::MUL, &[h.int(-1), call("integer_part", &[a])])]),
        _ => h.app(hd, &args),
    }
}

/// The step-function subterms in x (each once); `None` when one is nested in another.
fn steps<'h>(h: &'h impl Store, t: T<'h>, x: T<'h>, out: &mut Vec<T<'h>>) -> Option<()> {
    let View::App { args, .. } = h.view(t) else { return Some(()) };
    if head(h, t).is_some_and(|n| STEPS.contains(&n.as_str())) && args.len() == 1 && mentions(h, args[0], x) {
        let mut inner = vec![];
        steps(h, args[0], x, &mut inner)?;
        if !inner.is_empty() {
            return None;
        }
        if !out.contains(&t) {
            out.push(t);
        }
        return Some(());
    }
    for &a in args {
        steps(h, a, x, out)?;
    }
    Some(())
}

/// The numeric bounds of x from the top-level conjuncts `x < v`, `v <= x`, ... (v a number).
fn bounds<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, x: T<'h>) -> Res<(Option<f64>, Option<f64>)> {
    let h = cx.heap();
    let mut parts = vec![];
    let mut stack = vec![t];
    while let Some(u) = stack.pop() {
        match h.view(u) {
            View::App { args, .. } if head(h, u).as_deref() == Some("and") => stack.extend(args.iter().copied()),
            _ => parts.push(u),
        }
    }
    let (mut lo, mut hi) = (None::<f64>, None::<f64>);
    for c in parts {
        let View::App { args: &[l, r], .. } = h.view(c) else { continue };
        let Some(name) = head(h, c) else { continue };
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

/// An enclosure of the polynomial g(x) for x in [lo, hi] (interval Horner, widened); `None` when g is not a
/// polynomial in x with numeric coefficients.
fn range<'h>(cx: &mut NativeCx<'_, '_, 'h>, g: T<'h>, x: T<'h>, lo: f64, hi: f64) -> Res<Option<(f64, f64)>> {
    let h = cx.heap();
    let cl = cx.eval(h.call(h.sym("coefficient_list"), &[g, x]))?;
    let Some(cs) = cx.as_list(cl).map(<[T<'h>]>::to_vec) else { return Ok(None) };
    let mut nums = vec![];
    for c in cs {
        let v = cx.eval(h.call(h.sym("n"), &[c]))?;
        match h.view(v) {
            View::Real(f) if f.is_finite() => nums.push(f),
            View::Int(k) => nums.push(k as f64),
            _ => return Ok(None),
        }
    }
    let mul = |a: (f64, f64), b: (f64, f64)| {
        let p = [a.0 * b.0, a.0 * b.1, a.1 * b.0, a.1 * b.1];
        (p.iter().copied().fold(f64::INFINITY, f64::min), p.iter().copied().fold(f64::NEG_INFINITY, f64::max))
    };
    let mut acc = (0.0, 0.0);
    for &c in nums.iter().rev() {
        acc = mul(acc, (lo, hi));
        acc = (acc.0 + c, acc.1 + c);
    }
    let pad = |v: f64| 1e-9 * (1.0 + v.abs());
    Ok(Some((acc.0 - pad(acc.0) - 1.0, acc.1 + pad(acc.1) + 1.0)))
}

/// The answer when the condition has step functions (module comment); `None` otherwise.
pub(crate) fn reduce<'h>(cx: &mut NativeCx<'_, '_, 'h>, args: &[T<'h>]) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    let domain = |d: T<'h>| match h.view(d) {
        View::Sym(s) => Some(h.sym_name(s).to_string()),
        _ => None,
    };
    let (cond, x, integers) = match args {
        [c, x] => (*c, *x, false),
        [c, x, d] if domain(*d).as_deref() == Some("reals") => (*c, *x, false),
        [c, x, d] if domain(*d).as_deref() == Some("integers") => (*c, *x, true),
        _ => return Ok(None),
    };
    if !matches!(h.view(x), View::Sym(_)) {
        return Ok(None);
    }
    // a list of conditions is their conjunction
    let cond = match cx.as_list(cond) {
        Some(cs) => h.call(h.sym("and"), cs),
        None => cond,
    };
    // only conditions with a step function
    let mut stack = vec![cond];
    let mut any = false;
    while let Some(u) = stack.pop() {
        if let View::App { args, .. } = h.view(u) {
            if head(h, u).is_some_and(|n| STEPS.contains(&n.as_str()) || matches!(n.as_str(), "mod" | "quotient" | "fractional_part")) {
                any = true;
                break;
            }
            stack.extend(args.iter().copied());
        }
    }
    if !any {
        return Ok(None);
    }
    let cond = cx.eval(normal(h, cond, x))?;
    let mut st = vec![];
    if steps(h, cond, x, &mut st).is_none() || st.is_empty() {
        return Ok(None);
    }
    let (lo, hi) = bounds(cx, cond, x)?;
    if let (Some(lo), Some(hi)) = (lo, hi) {
        if lo > hi {
            return Ok(None);
        }
        if integers {
            return integer_points(cx, cond, x, lo.ceil(), hi.floor());
        }
        return bounded(cx, cond, x, &st, lo, hi);
    }
    unbounded(cx, cond, x, &st, integers)
}

/// The bounded algorithm (module comment) on lo <= x <= hi, the bounds of the condition.
fn bounded<'h>(cx: &mut NativeCx<'_, '_, 'h>, cond: T<'h>, x: T<'h>, st: &[T<'h>], lo: f64, hi: f64) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    // the integer values each step may take
    let mut values: Vec<(i64, i64)> = vec![];
    let mut cases: usize = 1;
    for &s in st {
        let View::App { args: &[g], .. } = h.view(s) else { return Ok(None) };
        let Some((a, b)) = range(cx, g, x, lo, hi)? else { return Ok(None) };
        if a.abs() > 1e9 || b.abs() > 1e9 {
            return Ok(None);
        }
        let (k0, k1) = (a.floor() as i64, b.ceil() as i64);
        cases = cases.saturating_mul((k1 - k0 + 1) as usize);
        if cases > MAX_CASES {
            return Ok(None);
        }
        values.push((k0, k1));
    }
    let mut choice: Vec<i64> = values.iter().map(|v| v.0).collect();
    let mut pieces = vec![];
    let rel = |n: &str, a: T<'h>, b: T<'h>| h.call(h.sym(n), &[a, b]);
    loop {
        cx.tick(1)?;
        let mut rules = vec![];
        let mut defs = vec![];
        for (i, &s) in st.iter().enumerate() {
            let k = choice[i];
            let View::App { args: &[g], .. } = h.view(s) else { return Ok(None) };
            let (kk, k1, km1) = (h.int(k), h.int(k + 1), h.int(k - 1));
            let kind = head(h, s).unwrap_or_default();
            let floor_def = |defs: &mut Vec<T<'h>>| {
                defs.push(rel("less_equal", kk, g));
                defs.push(rel("less", g, k1));
            };
            let ceil_def = |defs: &mut Vec<T<'h>>| {
                defs.push(rel("less", km1, g));
                defs.push(rel("less_equal", g, kk));
            };
            match kind.as_str() {
                "floor" => floor_def(&mut defs),
                "ceiling" => ceil_def(&mut defs),
                "round" => {
                    // halves go to the even neighbour: the ends belong to k when k is even
                    let ar = mc_arith::Arith::new(h);
                    let (lo_k, hi_k) = (ar.rational(h.int(2 * k - 1), h.int(2)), ar.rational(h.int(2 * k + 1), h.int(2)));
                    let r = if k % 2 == 0 { "less_equal" } else { "less" };
                    defs.push(rel(r, lo_k, g));
                    defs.push(rel(r, g, hi_k));
                }
                _ if k > 0 => floor_def(&mut defs),
                _ if k < 0 => ceil_def(&mut defs),
                _ => {
                    defs.push(rel("less", h.int(-1), g));
                    defs.push(rel("less", g, h.int(1)));
                }
            }
            rules.push(h.call(h.sym(names::RULE), &[s, kk]));
        }
        let rl = cx.list(&rules);
        let c = cx.eval(h.call(h.sym("rewrite"), &[cond, rl]))?;
        if c != h.sym_term(h.sym(names::FALSE)) {
            let mut all = vec![c];
            all.extend(defs);
            let r = cx.eval(h.call(h.sym("reduce"), &[h.call(h.sym("and"), &all), x, h.sym_term(h.sym("reals"))]))?;
            if matches!(head(h, r).as_deref(), Some("reduce" | names::FAILURE)) {
                return Ok(None);
            }
            if r != h.sym_term(h.sym(names::FALSE)) {
                pieces.push(r);
            }
        }
        // the next choice
        let mut i = choice.len();
        loop {
            if i == 0 {
                let all = match pieces.len() {
                    0 => return Ok(Some(h.sym_term(h.sym(names::FALSE)))),
                    1 => pieces[0],
                    _ => h.call(h.sym("or"), &pieces),
                };
                let r = cx.eval(h.call(h.sym("reduce"), &[all, x, h.sym_term(h.sym("reals"))]))?;
                return Ok((!matches!(head(h, r).as_deref(), Some("reduce" | names::FAILURE))).then_some(r));
            }
            i -= 1;
            if choice[i] < values[i].1 {
                choice[i] += 1;
                break;
            }
            choice[i] = values[i].0;
        }
    }
}

/// Is `r` an answer of `reduce` (not the call itself or a failure)?
fn decided<'h>(h: &'h impl Store, r: T<'h>) -> bool {
    !matches!(head(h, r).as_deref(), Some("reduce" | names::FAILURE))
}

/// `reduce(c, x, reals)`.
fn reduce_reals<'h>(cx: &mut NativeCx<'_, '_, 'h>, c: T<'h>, x: T<'h>) -> Res<T<'h>> {
    let h = cx.heap();
    cx.eval(h.call(h.sym("reduce"), &[c, x, h.sym_term(h.sym("reals"))]))
}

/// The integers lo <= k <= hi at which the condition holds, `x == k1 or x == k2 ...` (`false` for none); `None` when
/// there are too many or the condition is not decided at one of them.
fn integer_points<'h>(cx: &mut NativeCx<'_, '_, 'h>, cond: T<'h>, x: T<'h>, lo: f64, hi: f64) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    if !(lo.is_finite() && hi.is_finite()) || hi - lo + 1.0 > MAX_POINTS as f64 {
        return Ok(None);
    }
    let Some(ks) = holding_integers(cx, cond, x, lo as i64, hi as i64)? else { return Ok(None) };
    let eqs: Vec<T<'h>> = ks.into_iter().map(|k| h.call(h.sym("equal"), &[x, h.int(k)])).collect();
    Ok(Some(match eqs.len() {
        0 => h.sym_term(h.sym(names::FALSE)),
        1 => eqs[0],
        _ => h.call(h.sym("or"), &eqs),
    }))
}

/// The integers of [lo, hi] at which the condition is `true`; `None` when it is undecided at one of them.
fn holding_integers<'h>(cx: &mut NativeCx<'_, '_, 'h>, cond: T<'h>, x: T<'h>, lo: i64, hi: i64) -> Res<Option<Vec<i64>>> {
    let h = cx.heap();
    let mut out = vec![];
    for k in lo..=hi {
        cx.tick(1)?;
        let v = cx.eval(h.call(h.sym("rewrite"), &[cond, h.call(h.sym(names::RULE), &[x, h.int(k)])]))?;
        if v == h.sym_term(h.sym(names::TRUE)) {
            out.push(k);
        } else if v != h.sym_term(h.sym(names::FALSE)) {
            return Ok(None);
        }
    }
    Ok(Some(out))
}

/// The step functions on an unbounded line (module comment): the enclosure by bounded errors, else the period.
fn unbounded<'h>(cx: &mut NativeCx<'_, '_, 'h>, cond: T<'h>, x: T<'h>, st: &[T<'h>], integers: bool) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    let rel = |n: &str, a: T<'h>, b: T<'h>| h.call(h.sym(n), &[a, b]);
    let ar = mc_arith::Arith::new(h);
    // each error is one more variable of the quantifier elimination: a few steps only
    if st.len() > MAX_ERRORS {
        return periodic(cx, cond, x, st, integers);
    }
    // the condition with each step value written as its argument minus an error in a closed box
    let mut errs = vec![];
    let mut boxed = vec![];
    let mut rules = vec![];
    for (i, &s) in st.iter().enumerate() {
        let View::App { args: &[g], .. } = h.view(s) else { return Ok(None) };
        let t = h.sym_term(h.sym(&format!("reduce`t{i}")));
        let (a, b) = match head(h, s).as_deref() {
            Some("floor") => (h.int(0), h.int(1)),
            Some("ceiling") => (h.int(-1), h.int(0)),
            Some("integer_part") => (h.int(-1), h.int(1)),
            _ => (ar.rational(h.int(-1), h.int(2)), ar.rational(h.int(1), h.int(2))),
        };
        boxed.push(rel("less_equal", a, t));
        boxed.push(rel("less_equal", t, b));
        rules.push(h.call(h.sym(names::RULE), &[s, ar.sub(g, t)]));
        errs.push(t);
    }
    let e = cx.eval(h.call(h.sym("rewrite"), &[cond, cx.list(&rules)]))?;
    let errs = cx.list(&errs);
    let in_box = |c: T<'h>| {
        let mut all = boxed.clone();
        all.push(c);
        h.call(h.sym("exists"), &[errs, h.call(h.sym("and"), &all)])
    };
    // S (may hold) and S' (may fail)
    let may_hold = reduce_reals(cx, in_box(e), x)?;
    let may_fail = reduce_reals(cx, in_box(h.call(h.sym("not"), &[e])), x)?;
    if decided(h, may_hold) && decided(h, may_fail) {
        if may_hold == h.sym_term(h.sym(names::FALSE)) {
            return Ok(Some(may_hold));
        }
        let open = reduce_reals(cx, h.call(h.sym("and"), &[may_hold, may_fail]), x)?;
        if decided(h, open) {
            if let Some((lo, hi)) = hull(cx, open, x)? {
                let holds = h.call(h.sym("not"), &[may_fail]);
                let outside = h.call(h.sym("or"), &[rel("less", x, h.int(lo)), rel("greater", x, h.int(hi))]);
                if integers {
                    // only when the condition holds at no integer outside [lo, hi]
                    let tail = reduce_reals(cx, h.call(h.sym("and"), &[holds, outside]), x)?;
                    if tail == h.sym_term(h.sym(names::FALSE)) {
                        return integer_points(cx, cond, x, lo as f64, hi as f64);
                    }
                } else {
                    let inside = h.call(h.sym("and"), &[cond, rel("less_equal", h.int(lo), x), rel("less_equal", x, h.int(hi))]);
                    let part = reduce_reals(cx, inside, x)?;
                    if decided(h, part) {
                        let all = reduce_reals(cx, h.call(h.sym("or"), &[holds, part]), x)?;
                        return Ok(decided(h, all).then_some(all));
                    }
                }
            }
        }
    }
    periodic(cx, cond, x, st, integers)
}

/// [lo, hi] (integers) with the set `u` (an answer of `reduce` in x) inside: the numbers it compares x with,
/// widened by 1, checked to bound it; `None` when it is unbounded.
fn hull<'h>(cx: &mut NativeCx<'_, '_, 'h>, u: T<'h>, x: T<'h>) -> Res<Option<(i64, i64)>> {
    let h = cx.heap();
    if u == h.sym_term(h.sym(names::FALSE)) {
        return Ok(Some((0, 0)));
    }
    let mut vals: Vec<f64> = vec![];
    let mut stack = vec![u];
    while let Some(t) = stack.pop() {
        let View::App { args, .. } = h.view(t) else { continue };
        match (head(h, t).as_deref(), args) {
            (Some("and" | "or"), _) => stack.extend(args.iter().copied()),
            (Some("equal" | "unequal" | "less" | "less_equal" | "greater" | "greater_equal"), &[l, r]) => {
                let other = if l == x { r } else if r == x { l } else { return Ok(None) };
                let nv = cx.eval(h.call(h.sym("n"), &[other]))?;
                match h.view(nv) {
                    View::Real(f) if f.is_finite() => vals.push(f),
                    View::Int(k) => vals.push(k as f64),
                    _ => return Ok(None),
                }
            }
            _ => return Ok(None),
        }
    }
    let (Some(min), Some(max)) = (vals.iter().copied().reduce(f64::min), vals.iter().copied().reduce(f64::max)) else { return Ok(None) };
    if min.abs() > 1e12 || max.abs() > 1e12 {
        return Ok(None);
    }
    let (lo, hi) = (min.floor() as i64 - 1, max.ceil() as i64 + 1);
    for (name, k) in [("greater", hi), ("less", lo)] {
        let r = reduce_reals(cx, h.call(h.sym("and"), &[u, h.call(h.sym(name), &[x, h.int(k)])]), x)?;
        if r != h.sym_term(h.sym(names::FALSE)) {
            return Ok(None);
        }
    }
    Ok(Some((lo, hi)))
}

fn gcd(a: i64, b: i64) -> i64 {
    if b == 0 {
        a.abs()
    } else {
        gcd(b, a % b)
    }
}

/// The relations at the leaves of a formula of `and`, `or`, `not`, `implies`, `xor`; `None` for another leaf.
fn relations<'h>(h: &'h impl Store, t: T<'h>, out: &mut Vec<T<'h>>) -> Option<()> {
    let View::App { args, .. } = h.view(t) else {
        return matches!(h.view(t), View::Sym(s) if matches!(h.sym_name(s).as_str(), "true" | "false")).then_some(());
    };
    match head(h, t).as_deref() {
        Some("and" | "or" | "not" | "implies" | "xor") => {
            for &a in args {
                relations(h, a, out)?;
            }
            Some(())
        }
        Some("equal" | "unequal" | "less" | "less_equal" | "greater" | "greater_equal") if args.len() == 2 => {
            out.push(t);
            Some(())
        }
        _ => None,
    }
}

/// The answer of a periodic condition (module comment); `None` when the steps are not affine with rational slopes or
/// the shift by the period changes a relation.
fn periodic<'h>(cx: &mut NativeCx<'_, '_, 'h>, cond: T<'h>, x: T<'h>, st: &[T<'h>], integers: bool) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    let ar = mc_arith::Arith::new(h);
    // the slopes p/q and the period T = lcm(q)/gcd(p)
    let mut slopes = vec![];
    let (mut l, mut g) = (1i64, 0i64);
    for &s in st {
        let View::App { args: &[arg], .. } = h.view(s) else { return Ok(None) };
        let cl = cx.eval(h.call(h.sym("coefficient_list"), &[arg, x]))?;
        let Some(&[_, a]) = cx.as_list(cl) else { return Ok(None) };
        let (p, q) = match h.view(a) {
            View::Int(p) if p != 0 => (p, 1),
            View::Rational { num, den } => match (h.view(num), h.view(den)) {
                (View::Int(p), View::Int(q)) => (p, q),
                _ => return Ok(None),
            },
            _ => return Ok(None),
        };
        if p.abs() > 1_000_000 || q > 1_000_000 {
            return Ok(None);
        }
        // floor and ceiling move by n when the argument does; integer_part does not (it rounds towards 0), and round
        // only by even n (halves go to the even neighbour): its argument must move by an even integer
        let (p, q) = match head(h, s).as_deref() {
            Some("integer_part") => return Ok(None),
            Some("round") => {
                let d = gcd(p, 2);
                (p / d, 2 * q / d)
            }
            _ => (p, q),
        };
        l = l / gcd(l, q) * q;
        g = gcd(g, p);
        if l > MAX_POINTS {
            return Ok(None);
        }
        slopes.push(a);
    }
    let period = ar.rational(h.int(l), h.int(g));
    // each relation unchanged by x -> x + T with the step values moved by a T
    let mut rels = vec![];
    if relations(h, cond, &mut rels).is_none() {
        return Ok(None);
    }
    let mut same = vec![];
    let mut moved = vec![];
    for (i, (&s, &a)) in st.iter().zip(&slopes).enumerate() {
        let k = h.sym_term(h.sym(&format!("reduce`k{i}")));
        same.push(h.call(h.sym(names::RULE), &[s, k]));
        moved.push(h.call(h.sym(names::RULE), &[s, ar.add(&[k, ar.mul(&[a, period])])]));
    }
    let (same, moved) = (cx.list(&same), cx.list(&moved));
    let shift = h.call(h.sym(names::RULE), &[x, ar.add(&[x, period])]);
    for r in rels {
        cx.tick(1)?;
        let View::App { args: &[lt, rt], .. } = h.view(r) else { return Ok(None) };
        let d = ar.sub(lt, rt);
        let d0 = cx.eval(h.call(h.sym("rewrite"), &[d, same]))?;
        let d1 = cx.eval(h.call(h.sym("rewrite"), &[d, moved]))?;
        let d1 = cx.eval(h.call(h.sym("rewrite"), &[d1, shift]))?;
        let diff = cx.eval(h.call(h.sym("expand"), &[ar.sub(d1, d0)]))?;
        if diff != h.int(0) {
            return Ok(None);
        }
    }
    let gen = h.sym(crate::front::current().gen.as_deref().unwrap_or("c"));
    let c1 = h.app(h.sym_term(gen), &[h.int(1)]);
    let member = h.call(h.sym("element"), &[c1, h.sym_term(h.sym("integers"))]);
    if integers {
        // the integer period: the least integer multiple l of T = l/g
        let Some(ks) = holding_integers(cx, cond, x, 0, l - 1)? else { return Ok(None) };
        if ks.is_empty() {
            return Ok(Some(h.sym_term(h.sym(names::FALSE))));
        }
        let lc = ar.mul(&[h.int(l), c1]);
        let eqs: Vec<T<'h>> = ks.into_iter().map(|k| h.call(h.sym("equal"), &[x, ar.add(&[h.int(k), lc])])).collect();
        let any = if eqs.len() == 1 { eqs[0] } else { h.call(h.sym("or"), &eqs) };
        return Ok(Some(cx.eval(h.call(h.sym("and"), &[member, any]))?));
    }
    let one = h.call(h.sym("and"), &[cond, h.call(h.sym("less_equal"), &[h.int(0), x]), h.call(h.sym("less"), &[x, period])]);
    let base = reduce_reals(cx, one, x)?;
    if !decided(h, base) {
        return Ok(None);
    }
    if base == h.sym_term(h.sym(names::FALSE)) {
        return Ok(Some(base));
    }
    let Some(moved) = shifted(h, &ar, base, x, ar.mul(&[period, c1])) else { return Ok(None) };
    Ok(Some(cx.eval(h.call(h.sym("and"), &[member, moved]))?))
}

/// The answer `b` (relations of x with values) with every value moved by `by`; `None` for another form.
fn shifted<'h>(h: &'h mc_core::Heap, ar: &mc_arith::Arith<'h, mc_core::Heap>, b: T<'h>, x: T<'h>, by: T<'h>) -> Option<T<'h>> {
    let View::App { args, .. } = h.view(b) else { return None };
    match head(h, b).as_deref() {
        Some("and" | "or") => {
            let mut new = vec![];
            for &a in args {
                new.push(shifted(h, ar, a, x, by)?);
            }
            Some(h.call(h.sym(&head(h, b)?), &new))
        }
        Some(n @ ("equal" | "unequal" | "less" | "less_equal" | "greater" | "greater_equal")) => match *args {
            [l, r] if l == x => Some(h.call(h.sym(n), &[x, ar.add(&[r, by])])),
            [l, r] if r == x => Some(h.call(h.sym(n), &[ar.add(&[l, by]), x])),
            _ => None,
        },
        _ => None,
    }
}
