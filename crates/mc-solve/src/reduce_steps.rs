//! `reduce(cond, x, reals)` for conditions on a bounded interval of x with step functions of polynomials in x:
//! `floor`, `ceiling`, `integer_part` (and `mod(a, m)`, `quotient(a, m)`, `fractional_part(a)` for a number m, written
//! through them: `mod(a, m) = a - m floor(a/m)`, `quotient(a, m) = floor(a/m)`, `fractional_part(a) = a -
//! integer_part(a)`).
//!
//! The numeric bounds lo < x < hi of the condition bound each argument g(x) (interval Horner evaluation, widened), so
//! each step function takes finitely many integer values k. For each choice of the values the condition with the
//! step functions replaced by them, together with the conditions that the steps take those values (`k <= g < k + 1`
//! for floor, `k - 1 < g <= k` for ceiling, and for integer_part the floor's condition when k > 0, the ceiling's when
//! k < 0 and `-1 < g < 1` when k = 0), is a polynomial condition reduced by the one-variable algorithm; the union of
//! the pieces is reduced once more to merge them. A value range that is too wide (more than [`MAX_CASES`] choices),
//! nested step functions or a non-polynomial argument give no answer here.
use mc_api::{names, Store, View, T};
use mc_eval::{NativeCx, Res};

/// The most choices of the values of the step functions.
const MAX_CASES: usize = 4000;

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

const STEPS: &[&str] = &["floor", "ceiling", "integer_part"];

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

/// The answer when the condition has step functions on a bounded interval (module comment); `None` otherwise.
pub(crate) fn reduce<'h>(cx: &mut NativeCx<'_, '_, 'h>, args: &[T<'h>]) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    let (cond, x) = match args {
        [c, x] => (*c, *x),
        [c, x, d] if head(h, *d).is_none() && matches!(h.view(*d), View::Sym(s) if h.sym_name(s).as_str() == "reals") => (*c, *x),
        _ => return Ok(None),
    };
    if !matches!(h.view(x), View::Sym(_)) {
        return Ok(None);
    }
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
    let (Some(lo), Some(hi)) = bounds(cx, cond, x)? else { return Ok(None) };
    if lo > hi {
        return Ok(None);
    }
    // the integer values each step may take
    let mut values: Vec<(i64, i64)> = vec![];
    let mut cases: usize = 1;
    for &s in &st {
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
