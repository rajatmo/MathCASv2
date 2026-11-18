//! The written form of a `series` result, after the expansion:
//!
//! - **trigonometric functions of inverse trigonometric functions** in the coefficients are written as algebraic
//!   functions (`cos(asec(z)) = 1/z`, `sin(acot(x)) = 1/(x sqrt(1 + 1/x^2))`), identities of the principal branches
//!   that hold for every complex argument (DLMF 4.24.11-4.24.17 with `acot(z) = atan(1/z)`, `asec(z) = acos(1/z)`,
//!   `acsc(z) = asin(1/z)`, DLMF 4.23.9);
//! - **the logarithm at an infinity.** The expansion at `inf` is one in `t = 1/x > 0`, so its logarithmic terms come
//!   out as `log(1/x)`. For `x > 0` this is `-log(x)` and also `i pi - log(-x)`; the three forms differ only by
//!   multiples of `2 pi i` on one side of the real axis (DLMF 4.2.3 and 4.2.7). The form written is the one that
//!   agrees with the function at the most test points off the direction of expansion (on the negative axis and on
//!   both sides of it, on the imaginary axis), compared numerically to 30 digits; among forms that agree equally
//!   often the one with the fewest leaves, `log(x)` before `log(-x)` before `log(1/x)`. Every candidate is equal to
//!   the computed series for `x > 0`, so the result stays the expansion of the input; when the input cannot be
//!   evaluated at the test points (symbolic parameters), `log(1/x)` stays.
use mc_api::{Store, View, T};
use mc_arith::{Arith, Num};
use mc_core::Heap;
use mc_eval::{NativeCx, Res};
use rustc_hash::FxHashMap;

fn call<'h>(h: &'h Heap, f: &str, a: &[T<'h>]) -> T<'h> {
    h.call(h.sym(f), a)
}

/// `t` with every subterm equal to a key of `map` replaced by its value.
fn replace<'h>(h: &'h Heap, t: T<'h>, map: &FxHashMap<T<'h>, T<'h>>, memo: &mut FxHashMap<T<'h>, T<'h>>) -> T<'h> {
    if let Some(&v) = map.get(&t) {
        return v;
    }
    if let Some(&v) = memo.get(&t) {
        return v;
    }
    let r = match h.view(t) {
        View::App { head, args } => {
            let na: Vec<T<'h>> = args.iter().map(|&a| replace(h, a, map, memo)).collect();
            if na.as_slice() == args {
                t
            } else {
                h.app(head, &na)
            }
        }
        _ => t,
    };
    memo.insert(t, r);
    r
}

/// Does `t` contain a key of `map`?
fn mentions_any<'h>(h: &'h Heap, t: T<'h>, map: &FxHashMap<T<'h>, T<'h>>) -> bool {
    let mut w = mc_api::walk::Walk::new(t);
    while let Some(u) = w.pop() {
        if map.contains_key(&u) {
            return true;
        }
        if let View::App { head, args } = h.view(u) {
            w.push(head);
            w.extend_from_slice(args);
        }
    }
    false
}

/// The distinct exact complex numbers in `t`.
fn complex_atoms<'h>(h: &'h Heap, t: T<'h>) -> Vec<T<'h>> {
    let ar = Arith::new(h);
    let mut out = vec![];
    let mut w = mc_api::walk::Walk::new(t);
    while let Some(u) = w.pop() {
        // a complex number is the term complex(re, im)
        if let Some(n @ Num::Complex(..)) = ar.num(u) {
            if n.is_exact() && !out.contains(&u) {
                out.push(u);
            }
            continue;
        }
        if let View::App { head, args } = h.view(u) {
            w.push(head);
            w.extend_from_slice(args);
        }
    }
    out
}

/// The head name of `t` when it is a call with one argument, and the argument.
fn unary<'h>(h: &'h Heap, t: T<'h>) -> Option<(String, T<'h>)> {
    match h.view(t) {
        View::App { head, args: &[a] } => match h.view(head) {
            View::Sym(s) => Some((h.sym_name(s).to_string(), a)),
            _ => None,
        },
        _ => None,
    }
}

/// The algebraic value of `f(g(u))` for f in {sin, cos} and g an inverse trigonometric function.
fn trig_of_inverse<'h>(h: &'h Heap, f: &str, g: &str, u: T<'h>) -> Option<T<'h>> {
    let ar = Arith::new(h);
    let one = h.int(1);
    let half = ar.rational(h.int(1), h.int(2));
    let inv = |v: T<'h>| ar.pow(v, h.int(-1));
    // sqrt(1 + s w^2) for s = +-1
    let root = |s: i64, w: T<'h>| ar.pow(ar.add(&[one, ar.mul(&[h.int(s), ar.pow(w, h.int(2))])]), half);
    // sqrt(1 + s/w^2) = sqrt((w^2 + s)/w^2), the radicand as one fraction
    let root_inv = |s: i64, w: T<'h>| {
        let w2 = ar.pow(w, h.int(2));
        ar.pow(ar.mul(&[ar.add(&[h.int(s), w2]), ar.pow(w2, h.int(-1))]), half)
    };
    let v = match (f, g) {
        ("cos", "asin") | ("sin", "acos") => root(-1, u),
        ("sin", "atan") => ar.mul(&[u, inv(root(1, u))]),
        ("cos", "atan") => inv(root(1, u)),
        ("sin", "acot") => inv(ar.mul(&[u, root_inv(1, u)])),
        ("cos", "acot") => inv(root_inv(1, u)),
        ("sin", "asec") | ("cos", "acsc") => root_inv(-1, u),
        ("cos", "asec") | ("sin", "acsc") => inv(u),
        _ => return None,
    };
    Some(v)
}

/// The tidied value of a `series(e, x, x0, n)` call whose evaluated result is `r`.
pub(crate) fn tidy<'h>(cx: &mut NativeCx<'_, '_, 'h>, args: &[T<'h>], r: T<'h>) -> Res<T<'h>> {
    let h = cx.heap();
    let [e, x, x0, _, ..] = args else { return Ok(r) };
    if !matches!(h.view(*x), View::Sym(_)) {
        return Ok(r);
    }
    let r = inverse_trig(cx, *x, r)?;
    let r = gamma_forms(cx, r)?;
    let ar = Arith::new(h);
    let inf = h.sym_term(h.sym("inf"));
    let dir = if *x0 == inf {
        1
    } else if *x0 == ar.neg(inf) {
        -1
    } else {
        return Ok(r);
    };
    log_branch(cx, *e, *x, dir, r)
}

/// Trigonometric functions of inverse trigonometric functions written algebraically.
fn inverse_trig<'h>(cx: &mut NativeCx<'_, '_, 'h>, x: T<'h>, r: T<'h>) -> Res<T<'h>> {
    let h = cx.heap();
    let mut map = FxHashMap::default();
    let mut w = mc_api::walk::Walk::new(r);
    while let Some(u) = w.pop() {
        if let Some((f, a)) = unary(h, u) {
            if f == "sin" || f == "cos" {
                if let Some((g, v)) = unary(h, a) {
                    if let Some(val) = trig_of_inverse(h, &f, &g, v) {
                        map.insert(u, val);
                    }
                }
            }
        }
        if let View::App { head, args } = h.view(u) {
            w.push(head);
            w.extend_from_slice(args);
        }
    }
    if map.is_empty() {
        return Ok(r);
    }
    if crate::series::series_data_parts(h, r).is_none() {
        return rewrite(cx, r, x, &map, "together");
    }
    // the pairs sin(g(v)), cos(g(v)) of the angles present
    let mut angles: Vec<(T<'h>, T<'h>)> = vec![];
    for &u in map.keys() {
        if let Some((_, a)) = unary(h, u) {
            let p = (call(h, "sin", &[a]), call(h, "cos", &[a]));
            if !angles.contains(&p) {
                angles.push(p);
            }
        }
    }
    map_coefficients(cx, r, &mut |cx, c| {
        if !mentions_any(h, c, &map) {
            return Ok(c);
        }
        let mut memo = FxHashMap::default();
        let direct = cx.eval(call(h, "together", &[replace(h, c, &map, &mut memo)]))?;
        // the coefficient reduced by cos^2 = 1 - sin^2 first (cos(asec)^3/sin(asec) has the factor sin(asec)^2)
        let Some(red) = reduce_sin_cos(cx, c, &angles)? else { return Ok(direct) };
        let reduced = cx.eval(call(h, "together", &[replace(h, red, &map, &mut memo)]))?;
        Ok(if mc_api::walk::leaf_count(h, reduced) < mc_api::walk::leaf_count(h, direct) { reduced } else { direct })
    })
}

/// The gamma functions at rational arguments in the coefficients of a `series_data` term: each coefficient is
/// written in the shortest form (by `leaf_count`, the coefficient as computed among equals) among those given by
/// shifting each `gamma(q)` to `gamma(f)`, `f` the fractional part of `q` (`gamma(q + 1) = q gamma(q)`, DLMF 5.5.1).
/// The reflection `gamma(f) gamma(1 - f) = pi/sin(pi f)` is not used: it moves gamma functions between numerator and
/// denominator, which the written form of a coefficient keeps.
fn gamma_forms<'h>(cx: &mut NativeCx<'_, '_, 'h>, r: T<'h>) -> Res<T<'h>> {
    let h = cx.heap();
    if crate::series::series_data_parts(h, r).is_none() {
        return Ok(r);
    }
    map_coefficients(cx, r, &mut |cx, c| {
        let ar = Arith::new(h);
        let atoms = gamma_atoms(h, c);
        if atoms.is_empty() || atoms.len() > 4 {
            return Ok(c);
        }
        // the forms of each gamma(q): itself and P gamma(f)
        let mut options: Vec<Vec<T<'h>>> = vec![];
        for &(g, ref q) in &atoms {
            let (n, f) = q.floor_fract();
            let Some(n) = n.to_i64().filter(|n| n.abs() <= 30) else { return Ok(c) };
            let mut p = mc_arith::num::Rat::from_i64(1);
            for k in n.min(0)..n.max(0) {
                p = p.mul(&f.add(&mc_arith::num::Rat::from_i64(k)));
            }
            if n < 0 {
                let Some(inv) = p.recip() else { return Ok(c) };
                p = inv;
            }
            let pt = ar.number(&Num::rat(p));
            let fr = ar.number(&Num::rat(f.clone()));
            let mut o = vec![g];
            if n != 0 {
                o.push(ar.mul(&[pt, call(h, "gamma", &[fr])]));
            }
            options.push(o);
        }
        let mut best = c;
        let mut best_leaves = mc_api::walk::leaf_count(h, c);
        let total: usize = options.iter().map(Vec::len).product();
        for code in 1..total {
            let mut m = FxHashMap::default();
            let mut rest = code;
            for (k, o) in options.iter().enumerate() {
                let j = rest % o.len();
                rest /= o.len();
                if j != 0 {
                    m.insert(atoms[k].0, o[j]);
                }
            }
            let cand = cx.eval(replace(h, c, &m, &mut FxHashMap::default()))?;
            let l = mc_api::walk::leaf_count(h, cand);
            if l < best_leaves {
                best = cand;
                best_leaves = l;
            }
        }
        Ok(best)
    })
}

/// The distinct terms `gamma(q)` in `t` with `q` an exact rational that is not an integer, with q.
fn gamma_atoms<'h>(h: &'h Heap, t: T<'h>) -> Vec<(T<'h>, mc_arith::num::Rat)> {
    let ar = Arith::new(h);
    let mut out: Vec<(T<'h>, mc_arith::num::Rat)> = vec![];
    let mut w = mc_api::walk::Walk::new(t);
    while let Some(u) = w.pop() {
        if let Some((f, a)) = unary(h, u) {
            if f == "gamma" {
                if let Some(q) = ar.num(a).and_then(|n| n.as_rat().cloned()) {
                    if !q.is_int() && !out.iter().any(|(g, _)| *g == u) {
                        out.push((u, q));
                    }
                    continue;
                }
            }
        }
        if let View::App { head, args } = h.view(u) {
            w.push(head);
            w.extend_from_slice(args);
        }
    }
    out
}

/// `r` (a `series_data` term) with every coefficient c replaced by `f(c)`.
fn map_coefficients<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    r: T<'h>,
    f: &mut dyn FnMut(&mut NativeCx<'_, '_, 'h>, T<'h>) -> Res<T<'h>>,
) -> Res<T<'h>> {
    let h = cx.heap();
    let Some((x, x0, cs, v, p)) = crate::series::series_data_parts(h, r) else { return Ok(r) };
    let mut out = Vec::with_capacity(cs.len());
    let mut changed = false;
    for &c in cs {
        let c2 = f(cx, c)?;
        changed |= c2 != c;
        out.push(c2);
    }
    if !changed {
        return Ok(r);
    }
    let l = cx.list(&out);
    cx.eval(call(h, "series_data", &[x, x0, l, h.int(v), h.int(p)]))
}

/// `c` as a rational function of the sines and cosines of `angles`, with each numerator and denominator reduced
/// to degree at most 1 in every cosine by `cos^2 = 1 - sin^2`, the common factors cancelled; `None` when nothing
/// changes.
fn reduce_sin_cos<'h>(cx: &mut NativeCx<'_, '_, 'h>, c: T<'h>, angles: &[(T<'h>, T<'h>)]) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    let ar = Arith::new(h);
    let mut to_sym = FxHashMap::default();
    let mut back = FxHashMap::default();
    let mut rels = vec![];
    for (k, &(s, co)) in angles.iter().enumerate() {
        let (ss, cs) = (h.sym_term(h.sym(&format!("series`sin{k}"))), h.sym_term(h.sym(&format!("series`cos{k}"))));
        to_sym.insert(s, ss);
        to_sym.insert(co, cs);
        back.insert(ss, s);
        back.insert(cs, co);
        // cos^2 + sin^2 - 1
        rels.push((cs, ar.add(&[ar.pow(cs, h.int(2)), ar.pow(ss, h.int(2)), h.int(-1)])));
    }
    let t = replace(h, c, &to_sym, &mut FxHashMap::default());
    let t = cx.eval(call(h, "together", &[t]))?;
    let mut num = cx.eval(call(h, "numerator", &[t]))?;
    let mut den = cx.eval(call(h, "denominator", &[t]))?;
    for &(cs, rel) in &rels {
        num = cx.eval(call(h, "polynomial_remainder", &[num, rel, cs]))?;
        den = cx.eval(call(h, "polynomial_remainder", &[den, rel, cs]))?;
    }
    let q = cx.eval(call(h, "cancel", &[ar.mul(&[num, ar.pow(den, h.int(-1))])]))?;
    if q == t || ar.num(den).is_some_and(|d| d.is_zero()) {
        return Ok(None);
    }
    Ok(Some(replace(h, q, &back, &mut FxHashMap::default())))
}

/// `r` with the replacements of `map`, evaluated; in a `series_data` term only the coefficients that change, each
/// passed through `tidy_fn` (`expand` or `together`).
fn rewrite<'h>(cx: &mut NativeCx<'_, '_, 'h>, r: T<'h>, x: T<'h>, map: &FxHashMap<T<'h>, T<'h>>, tidy_fn: &str) -> Res<T<'h>> {
    let h = cx.heap();
    let mut memo = FxHashMap::default();
    if let Some((x, x0, cs, v, p)) = crate::series::series_data_parts(h, r) {
        let mut out = Vec::with_capacity(cs.len());
        for &c in cs {
            if mentions_any(h, c, map) {
                let c2 = replace(h, c, map, &mut memo);
                out.push(cx.eval(call(h, tidy_fn, &[c2]))?);
            } else {
                out.push(c);
            }
        }
        let l = cx.list(&out);
        return cx.eval(call(h, "series_data", &[x, x0, l, h.int(v), h.int(p)]));
    }
    // a sum with an order term (a series at an infinity, a Puiseux series): each term c x^k with its coefficient c
    // rewritten
    let add = h.sym_term(h.sym(mc_api::names::ADD));
    let mul = h.sym_term(h.sym(mc_api::names::MUL));
    let pow = h.sym_term(h.sym(mc_api::names::POW));
    let View::App { head, args: terms } = h.view(r) else { return cx.eval(replace(h, r, map, &mut memo)) };
    if head != add {
        return cx.eval(replace(h, r, map, &mut memo));
    }
    let is_power = |f: T<'h>| f == x || matches!(h.view(f), View::App { head, args: &[b, _] } if head == pow && b == x);
    let mut out = Vec::with_capacity(terms.len());
    for &t in terms {
        if !mentions_any(h, t, map) {
            out.push(t);
            continue;
        }
        let factors: Vec<T<'h>> = match h.view(t) {
            View::App { head, args } if head == mul => args.to_vec(),
            _ => vec![t],
        };
        let (powers, rest): (Vec<T<'h>>, Vec<T<'h>>) = factors.into_iter().partition(|&f| is_power(f));
        let c = replace(h, h.app(mul, &rest), map, &mut memo);
        let c = cx.eval(call(h, tidy_fn, &[c]))?;
        let mut parts = vec![c];
        parts.extend(powers);
        out.push(h.app(mul, &parts));
    }
    let out = expand_constant(cx, &out, x)?;
    cx.eval(h.app(add, &out))
}

/// The terms with the terms free of `x` added and expanded into one (`-i pi log(2) + i pi (log(2) + i pi) + pi^2`
/// is 0) when there are several.
fn expand_constant<'h>(cx: &mut NativeCx<'_, '_, 'h>, terms: &[T<'h>], x: T<'h>) -> Res<Vec<T<'h>>> {
    let h = cx.heap();
    let (free, rest): (Vec<T<'h>>, Vec<T<'h>>) = terms.iter().partition(|&&t| free_of(h, t, x));
    if free.len() < 2 {
        return Ok(terms.to_vec());
    }
    let add = h.sym_term(h.sym(mc_api::names::ADD));
    let k = cx.eval(call(h, "expand", &[h.app(add, &free)]))?;
    let mut out = vec![k];
    out.extend(rest);
    Ok(out)
}

/// Is `t` free of `x`?
fn free_of<'h>(h: &'h Heap, t: T<'h>, x: T<'h>) -> bool {
    let mut w = mc_api::walk::Walk::new(t);
    while let Some(u) = w.pop() {
        if u == x {
            return false;
        }
        if let View::App { head, args } = h.view(u) {
            w.push(head);
            w.extend_from_slice(args);
        }
    }
    true
}

/// `r` with the exact complex numbers in its terms free of `x` written with `i = s` (`s = dir sqrt(-x^2)/x`, equal
/// to `i` on the direction of expansion), or `None` when there are none.
fn unit_form<'h>(cx: &mut NativeCx<'_, '_, 'h>, r: T<'h>, x: T<'h>, s: T<'h>) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    let ar = Arith::new(h);
    let add = h.sym_term(h.sym(mc_api::names::ADD));
    let View::App { head, args: terms } = h.view(r) else { return Ok(None) };
    if head != add {
        return Ok(None);
    }
    let mut changed = false;
    let mut out = Vec::with_capacity(terms.len());
    for &t in terms {
        let atoms = if free_of(h, t, x) { complex_atoms(h, t) } else { vec![] };
        if atoms.is_empty() {
            out.push(t);
            continue;
        }
        let mut m = FxHashMap::default();
        for c in atoms {
            if let Some(Num::Complex(a, b)) = ar.num(c) {
                m.insert(c, ar.add(&[ar.number(&Num::Real(a)), ar.mul(&[ar.number(&Num::Real(b)), s])]));
            }
        }
        let t2 = replace(h, t, &m, &mut FxHashMap::default());
        out.push(cx.eval(call(h, "together", &[t2]))?);
        changed = true;
    }
    if !changed {
        return Ok(None);
    }
    Ok(Some(cx.eval(h.app(add, &out))?))
}

/// The form of the logarithms of a series at `inf` (dir 1) or `-inf` (dir -1); see the module docs.
fn log_branch<'h>(cx: &mut NativeCx<'_, '_, 'h>, e: T<'h>, x: T<'h>, dir: i64, r: T<'h>) -> Res<T<'h>> {
    let h = cx.heap();
    let ar = Arith::new(h);
    let inv_x = ar.pow(x, h.int(-1));
    let lp = cx.eval(call(h, "log", &[inv_x]))?;
    let ln = cx.eval(call(h, "log", &[ar.neg(inv_x)]))?;
    let mut probe = FxHashMap::default();
    probe.insert(lp, lp);
    probe.insert(ln, ln);
    // the exact complex constants of the series when the input has none: i = dir sqrt(-x^2)/x on the direction
    let units = complex_atoms(h, e).is_empty() && !complex_atoms(h, r).is_empty();
    let has_logs = mentions_any(h, r, &probe);
    if !has_logs && !units {
        return Ok(r);
    }
    let ipi = |k: i64| ar.mul(&[h.int(k), ar.number(&Num::i()), h.sym_term(h.sym("pi"))]);
    let log_of = |u: T<'h>| call(h, "log", &[u]);
    let (lx, lmx) = (log_of(x), log_of(ar.neg(x)));
    // x > 0 (dir 1): log(1/x) = -log(x) = i pi - log(-x), log(-1/x) = i pi - log(x) = 2 i pi - log(-x);
    // x < 0 (dir -1): log(-1/x) = -log(-x) = i pi - log(x), log(1/x) = i pi - log(-x) = 2 i pi - log(x)
    let (near, far) = if dir == 1 { (lx, lmx) } else { (lmx, lx) };
    let (lt, lnt) = if dir == 1 { (lp, ln) } else { (ln, lp) };
    let maps: [[(T<'h>, T<'h>); 2]; 2] = [
        [(lt, ar.neg(near)), (lnt, ar.sub(ipi(1), near))],
        [(lt, ar.sub(ipi(1), far)), (lnt, ar.sub(ipi(2), far))],
    ];
    let mut cands = vec![];
    if has_logs {
        for m in maps {
            let map: FxHashMap<T<'h>, T<'h>> = m.into_iter().collect();
            cands.push(rewrite(cx, r, x, &map, "expand")?);
        }
    }
    cands.push(r);
    if units {
        let s = ar.mul(&[h.int(dir), ar.pow(ar.neg(ar.pow(x, h.int(2))), ar.rational(h.int(1), h.int(2))), ar.pow(x, h.int(-1))]);
        for k in 0..cands.len() {
            if let Some(c) = unit_form(cx, cands[k], x, s)? {
                cands.push(c);
            }
        }
    }
    // the original series last among equals
    let orig = cands.iter().position(|&c| c == r).unwrap_or(0);
    let last = cands.len() - 1;
    cands.swap(orig, last);
    // the test points, opposite to the direction of expansion: on that axis, on each side of it, on the imaginary axis
    let i = ar.number(&Num::i());
    let big = h.int(-100 * dir);
    let pts = [big, ar.add(&[big, i]), ar.sub(big, i), ar.mul(&[big, i]), ar.neg(ar.mul(&[big, i]))];
    let mut normals = vec![];
    for &c in &cands {
        normals.push(cx.eval(call(h, "normal", &[c]))?);
    }
    let digits = h.int(30);
    let mut score = vec![0usize; cands.len()];
    for &p in &pts {
        let mut sub = FxHashMap::default();
        sub.insert(x, p);
        let fv = cx.eval(call(h, "n", &[replace(h, e, &sub, &mut FxHashMap::default()), digits]))?;
        if ar.num(fv).is_none() {
            return Ok(r);
        }
        let mut errs = vec![];
        for &nm in &normals {
            let sv = replace(h, nm, &sub, &mut FxHashMap::default());
            // both sides as numbers first: an exact difference of 0 has no relative precision to reach
            let svn = cx.eval(call(h, "n", &[sv, digits]))?;
            if ar.num(svn).is_none() {
                return Ok(r);
            }
            let d = cx.eval(call(h, "abs", &[ar.sub(fv, svn)]))?;
            match ar.num(d).and_then(|n| n.as_real().map(|r| r.to_f64())) {
                Some(v) if v.is_finite() => errs.push(v),
                _ => return Ok(r),
            }
        }
        let mn = errs.iter().copied().fold(f64::INFINITY, f64::min);
        let mx = errs.iter().copied().fold(0.0, f64::max);
        for (k, &v) in errs.iter().enumerate() {
            // no discrimination at this point: every form agrees with the function equally well
            if mx <= 1e4 * mn + 1e-25 || v <= (mn * mx).sqrt() {
                score[k] += 1;
            }
        }
    }
    let mut best = cands.len() - 1;
    let key = |k: usize| (std::cmp::Reverse(score[k]), mc_api::walk::leaf_count(h, cands[k]), k);
    for k in 0..cands.len() {
        if key(k) < key(best) {
            best = k;
        }
    }
    let r = cands[best];
    let add = h.sym_term(h.sym(mc_api::names::ADD));
    match h.view(r) {
        View::App { head, args } if head == add => {
            let out = expand_constant(cx, args, x)?;
            if out.len() == args.len() {
                return Ok(r);
            }
            cx.eval(h.app(add, &out))
        }
        _ => Ok(r),
    }
}
