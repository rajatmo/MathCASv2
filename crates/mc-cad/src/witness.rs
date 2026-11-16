//! Existential sentences over the reals that are not polynomial: `resolve(exists(x, f(x) == g(x)))` is
//! `true` when a sign change of `h = f - g` is proved. Two points `u < v` (inside the bounds the sentence puts on
//! `x`) where the signs of `h` are decided (exact closed values, refinement in `mc-numeric`) and opposite, and an
//! enclosure of `h` over the whole interval `[u, v]` (interval arithmetic, `interval([u, v])`) that is a finite real
//! interval: the functions allowed in `h` are continuous wherever they are finite and real (sums, products, powers,
//! `exp`, `log`, the circular and hyperbolic functions and their inverses, ...), so `h` is continuous on `[u, v]`
//! and has a zero there (the intermediate value theorem). With several variables the earlier ones are fixed at a
//! few rational values and the last is searched. Only `true` is ever answered; a search that finds nothing leaves
//! the sentence as it was.
use mc_api::{names, Store, View, T};
use mc_eval::{NativeCx, Res};

/// The functions that are continuous wherever their value is finite and real.
const CONTINUOUS: &[&str] = &[
    names::ADD, names::MUL, names::POW, "exp", "log", "sin", "cos", "tan", "cot", "sec", "csc", "sinh", "cosh", "tanh",
    "coth", "sech", "csch", "atan", "asin", "acos", "asinh", "acosh", "atanh", "sqrt", "erf", "erfc", "bessel_j",
    "bessel_y", "bessel_i", "bessel_k", "airy_ai", "airy_bi", "gamma", "log_gamma", "fresnel_s", "fresnel_c",
];

/// The fixed values tried for the earlier variables.
const FIXED: &[(i64, i64)] = &[(0, 1), (1, 1), (-1, 1), (1, 2)];
/// The sample points on an unbounded line.
const LINE: &[(i64, i64)] = &[
    (-10, 1), (-5, 1), (-3, 1), (-2, 1), (-1, 1), (-1, 2), (-1, 10), (0, 1), (1, 10), (1, 4), (1, 2), (3, 4), (1, 1),
    (3, 2), (2, 1), (3, 1), (5, 1), (10, 1),
];

/// `exists(vars, body)` or `exists(vars, cond, body)` without free variables, proved by a sign change; `None` when
/// the form does not fit or nothing is found.
pub fn exists_by_sign_change<'h>(cx: &mut NativeCx<'_, '_, 'h>, f: T<'h>) -> Res<Option<bool>> {
    let h = cx.heap();
    let Some((name, args)) = call(h, f) else { return Ok(None) };
    if name != "exists" {
        return Ok(None);
    }
    let (vs, parts) = match args {
        [v, b] => (*v, vec![*b]),
        [v, c, b] => (*v, vec![*c, *b]),
        _ => return Ok(None),
    };
    let vars: Vec<T<'h>> = match cx.as_list(vs) {
        Some(xs) => xs.to_vec(),
        None => vec![vs],
    };
    if vars.is_empty() || vars.iter().any(|&x| !matches!(h.view(x), View::Sym(_))) {
        return Ok(None);
    }
    let mut conj = vec![];
    for p in parts {
        flatten_and(h, p, &mut conj);
    }
    // one equation; the rest bounds of the last variable by numbers
    let x = *vars.last().expect("a variable");
    let mut eq = None;
    let (mut lo, mut hi) = (None::<f64>, None::<f64>);
    for c in conj {
        let Some((rel, [l, r])) = call(h, c) else { return Ok(None) };
        match rel.as_str() {
            "equal" if eq.is_none() => eq = Some(h.call(h.sym(names::ADD), &[*l, h.call(h.sym(names::MUL), &[h.int(-1), *r])])),
            "less" | "less_equal" | "greater" | "greater_equal" => {
                let (b, upper) = match (*l == x, *r == x) {
                    (true, false) => (number(cx, *r)?, matches!(rel.as_str(), "less" | "less_equal")),
                    (false, true) => (number(cx, *l)?, matches!(rel.as_str(), "greater" | "greater_equal")),
                    _ => return Ok(None),
                };
                let Some(b) = b else { return Ok(None) };
                if upper {
                    hi = Some(hi.map_or(b, |v: f64| v.min(b)));
                } else {
                    lo = Some(lo.map_or(b, |v: f64| v.max(b)));
                }
            }
            _ => return Ok(None),
        }
    }
    let Some(eq) = eq else { return Ok(None) };
    let eq = cx.eval(eq)?;
    if !continuous_heads(h, eq, &vars) {
        return Ok(None);
    }
    // the points of the search, strictly inside the bounds
    let pts: Vec<T<'h>> = match (lo, hi) {
        (Some(a), Some(b)) if a < b => (1..32).map(|k| rational(cx, a + (b - a) * k as f64 / 32.0)).collect::<Res<Vec<_>>>()?,
        _ => {
            let mut v = vec![];
            for &(p, q) in LINE {
                let t = p as f64 / q as f64;
                if lo.is_some_and(|a| t <= a) || hi.is_some_and(|b| t >= b) {
                    continue;
                }
                v.push(ratio(h, p, q));
            }
            v.into_iter().map(|t| cx.eval(t)).collect::<Res<Vec<_>>>()?
        }
    };
    let earlier = &vars[..vars.len() - 1];
    let combos = FIXED.len().pow(earlier.len() as u32).min(16);
    for c in 0..combos {
        cx.tick(1)?;
        // the earlier variables at fixed values
        let mut g = eq;
        let mut k = c;
        for &y in earlier {
            let (p, q) = FIXED[k % FIXED.len()];
            k /= FIXED.len();
            let val = cx.eval(ratio(h, p, q))?;
            g = substitute(cx, g, y, val)?;
        }
        // the decided signs at the points
        let mut signs: Vec<(T<'h>, i64)> = vec![];
        for &p in &pts {
            let v = substitute(cx, g, x, p)?;
            let s = cx.eval(h.call(h.sym("sign"), &[v]))?;
            if let View::Int(s @ (-1 | 1)) = h.view(s) {
                signs.push((p, s));
            }
        }
        for w in signs.windows(2) {
            let ((u, su), (v, sv)) = (w[0], w[1]);
            if su == sv {
                continue;
            }
            let box_ = h.call(h.sym("interval"), &[cx.list(&[u, v])]);
            let e = substitute(cx, g, x, box_)?;
            if finite_interval(cx, e)? {
                return Ok(Some(true));
            }
        }
    }
    Ok(None)
}

/// The name and arguments of a call with a symbol head.
fn call<'h>(h: &'h impl Store, t: T<'h>) -> Option<(String, &'h [T<'h>])> {
    match h.view(t) {
        View::App { head, args } => match h.view(head) {
            View::Sym(s) => Some((h.sym_name(s).to_string(), args)),
            _ => None,
        },
        _ => None,
    }
}

fn flatten_and<'h>(h: &'h impl Store, t: T<'h>, out: &mut Vec<T<'h>>) {
    match call(h, t) {
        Some((n, args)) if n == "and" => {
            for &a in args {
                flatten_and(h, a, out);
            }
        }
        _ => out.push(t),
    }
}

/// A real number term as a machine number.
fn number<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>) -> Res<Option<f64>> {
    let h = cx.heap();
    let v = cx.eval(h.call(h.sym("n"), &[t]))?;
    Ok(match h.view(v) {
        View::Real(x) if x.is_finite() => Some(x),
        View::Int(k) => Some(k as f64),
        _ => None,
    })
}

/// The term p/q.
fn ratio<'h>(h: &'h impl Store, p: i64, q: i64) -> T<'h> {
    h.call(h.sym(names::MUL), &[h.int(p), h.call(h.sym(names::POW), &[h.int(q), h.int(-1)])])
}

/// A rational near `v` (a point of the search: any point will do, it need only be exact).
fn rational<'h>(cx: &mut NativeCx<'_, '_, 'h>, v: f64) -> Res<T<'h>> {
    let h = cx.heap();
    let k = (v * 1024.0).round() as i64;
    cx.eval(ratio(h, k, 1024))
}

fn substitute<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, x: T<'h>, v: T<'h>) -> Res<T<'h>> {
    let h = cx.heap();
    cx.eval(h.call(h.sym("rewrite"), &[t, h.call(h.sym(names::RULE), &[x, v])]))
}

/// Is every call in `t` (with a variable inside) one of [`CONTINUOUS`]?
fn continuous_heads<'h>(h: &'h impl Store, t: T<'h>, vars: &[T<'h>]) -> bool {
    let mentions = |u: T<'h>| {
        let mut st = mc_api::walk::Walk::new(u);
        while let Some(w) = st.pop() {
            if vars.contains(&w) {
                return true;
            }
            if let View::App { args, .. } = h.view(w) {
                st.extend(args.iter().copied());
            }
        }
        false
    };
    let mut stack = mc_api::walk::Walk::new(t);
    while let Some(u) = stack.pop() {
        if let View::App { head, args } = h.view(u) {
            if !mentions(u) {
                continue;
            }
            let ok = matches!(h.view(head), View::Sym(s) if CONTINUOUS.contains(&h.sym_name(s).as_str()));
            if !ok {
                return false;
            }
            stack.extend(args.iter().copied());
        }
    }
    true
}

/// Is `e` an interval with finite real ends (the enclosure of a function defined and real on the whole interval)?
fn finite_interval<'h>(cx: &mut NativeCx<'_, '_, 'h>, e: T<'h>) -> Res<bool> {
    let h = cx.heap();
    let Some((n, &[l])) = call(h, e) else { return Ok(false) };
    if n != "interval" {
        return Ok(false);
    }
    let Some(&[a, b]) = cx.as_list(l) else { return Ok(false) };
    Ok(number(cx, a)?.is_some() && number(cx, b)?.is_some())
}
