//! `sum(f, [k, a, b])` (finite, closed-form and infinite sums) and `piecewise`.
//!
//! Sums: integer bounds with at most `MAX_TERMS` terms are added exactly (`table` then `sum`). Otherwise, in this
//! order: a term free of k gives `(b - a + 1) f`; a polynomial term (degree at most `MAX_DEGREE`, found by
//! differentiating until zero) gives the closed form by interpolating the exact partial sums `S(n)` at
//! n = 0, ..., degree + 1 and taking `S(b) - S(a - 1)`, factored; a geometric term (`f(k + 1)/f(k)` free of k)
//! gives `f(a)(r^(b-a+1) - 1)/(r - 1)`, or `f(a)/(1 - r)` to infinity when `|r| < 1`; `c k^-s` to infinity with a
//! numeric `s > 1` gives `c zeta(s)` (minus the first terms when a > 1). Anything else stays unevaluated.
//! The helpers call library functions (`d`, `expand`, `factor`, `simplify`, `rewrite`, `free_of`) through the
//! evaluator, so this module depends on no other package.
use crate::ctx::Lib;
use mc_api::names;
use mc_core::{Store, Sym, View, T};
use mc_eval::{NativeCx, Reply, Res};

/// Terms added one by one at most.
const MAX_TERMS: i64 = 1_000_000;
/// The highest polynomial degree summed in closed form.
const MAX_DEGREE: usize = 12;

fn call<'h>(cx: &mut NativeCx<'_, '_, 'h>, name: &str, args: &[T<'h>]) -> Res<T<'h>> {
    let h = cx.heap();
    cx.eval(h.call(h.sym(name), args))
}

fn is_true<'h>(cx: &NativeCx<'_, '_, 'h>, t: T<'h>) -> bool {
    let h = cx.heap();
    t == h.sym_term(h.sym(names::TRUE))
}

fn free_of<'h>(cx: &mut NativeCx<'_, '_, 'h>, e: T<'h>, k: T<'h>) -> Res<bool> {
    let r = call(cx, "free_of", &[e, k])?;
    Ok(is_true(cx, r))
}

/// `f` with `k` replaced by `v`, evaluated.
fn at<'h>(cx: &mut NativeCx<'_, '_, 'h>, f: T<'h>, k: T<'h>, v: T<'h>) -> Res<T<'h>> {
    let h = cx.heap();
    let rules = h.call(h.sym(names::LIST), &[h.call(h.sym(names::RULE), &[k, v])]);
    call(cx, "rewrite", &[f, rules])
}

fn add<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: &[T<'h>]) -> Res<T<'h>> {
    call(cx, names::ADD, a)
}

fn mul<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: &[T<'h>]) -> Res<T<'h>> {
    call(cx, names::MUL, a)
}

/// `sum(f, [k, a, b])`, `sum(f, [k, n])` (k from 1 to n).
pub(crate) fn iterated<'h>(_lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [f, spec] = *args else { return Ok(Reply::Decline) };
    // `sum(f, [k, [x1, x2, ...]])`: k takes the values of the list (decision io-core.15)
    if let Some(&[kv, vals]) = cx.as_list(spec) {
        if cx.as_list(vals).is_some() && matches!(h.view(kv), View::Sym(_)) {
            let terms = call(cx, "table", &[f, kv, vals])?;
            return Ok(Reply::Done(call(cx, "sum", &[terms])?));
        }
    }
    let (kv, a, b) = match cx.as_list(spec) {
        Some(&[kv, a, b]) => (kv, a, b),
        Some(&[kv, b]) => (kv, h.int(1), b),
        _ => return Ok(Reply::Decline),
    };
    let View::Sym(k) = h.view(kv) else { return Ok(Reply::Decline) };
    let inf = h.sym_term(h.sym("inf"));
    // integer bounds: the terms one by one
    if let (View::Int(ia), View::Int(ib)) = (h.view(a), h.view(b)) {
        if ib < ia {
            return Ok(Reply::Done(h.int(0)));
        }
        if ib - ia < MAX_TERMS {
            let terms = call(cx, "table", &[f, kv, h.call(h.sym("range"), &[a, b])])?;
            return Ok(Reply::Done(call(cx, "sum", &[terms])?));
        }
    }
    if b != inf && free_of(cx, f, kv)? {
        let one = h.int(1);
        let na = mul(cx, &[h.int(-1), a])?;
        let count = add(cx, &[b, na, one])?;
        return Ok(Reply::Done(mul(cx, &[count, f])?));
    }
    if b != inf {
        if let Some(s) = polynomial(cx, f, k, a, b)? {
            return Ok(Reply::Done(s));
        }
    }
    if let Some(s) = geometric(cx, f, kv, a, b, b == inf)? {
        return Ok(Reply::Done(s));
    }
    if b == inf {
        if let Some(s) = zeta_like(cx, f, kv, a)? {
            return Ok(Reply::Done(s));
        }
    }
    let _ = k;
    Ok(Reply::Decline)
}

/// The closed form of a polynomial sum, or `None` when `f` is not a polynomial in `k` of degree at most
/// `MAX_DEGREE`.
fn polynomial<'h>(cx: &mut NativeCx<'_, '_, 'h>, f: T<'h>, k: Sym, a: T<'h>, b: T<'h>) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    let kv = h.sym_term(k);
    let zero = h.int(0);
    // the degree: the number of derivatives until zero
    let mut g = f;
    let mut degree = None;
    for m in 0..=MAX_DEGREE {
        cx.tick(1)?;
        let dg = call(cx, "d", &[g, kv])?;
        g = call(cx, "expand", &[dg])?;
        if g == zero {
            degree = Some(m);
            break;
        }
    }
    let Some(m) = degree else { return Ok(None) };
    // exact partial sums y_j = f(0) + ... + f(j), j = 0..=m+1
    let mut ys = Vec::with_capacity(m + 2);
    let mut acc = zero;
    for j in 0..=(m as i64 + 1) {
        let fj = at(cx, f, kv, h.int(j))?;
        acc = add(cx, &[acc, fj])?;
        ys.push(acc);
    }
    // S(n) by Lagrange interpolation through (j, y_j)
    let lagrange = |cx: &mut NativeCx<'_, '_, 'h>, n: T<'h>| -> Res<T<'h>> {
        let mut terms = Vec::with_capacity(ys.len());
        for (j, &y) in ys.iter().enumerate() {
            let mut factors = vec![y];
            let mut den: i128 = 1;
            for i in 0..ys.len() {
                if i != j {
                    factors.push(add(cx, &[n, h.int(-(i as i64))])?);
                    den *= j as i128 - i as i128;
                }
            }
            let inv = call(cx, names::POW, &[h.int(den as i64), h.int(-1)])?;
            factors.push(inv);
            terms.push(mul(cx, &factors)?);
        }
        add(cx, &terms)
    };
    let sb = lagrange(cx, b)?;
    let am1 = add(cx, &[a, h.int(-1)])?;
    let sa = lagrange(cx, am1)?;
    let nsa = mul(cx, &[h.int(-1), sa])?;
    let diff = add(cx, &[sb, nsa])?;
    let e = call(cx, "expand", &[diff])?;
    Ok(Some(call(cx, "factor", &[e])?))
}

/// A geometric sum (`f(k + 1)/f(k)` free of `k`, not 1), or `None`.
fn geometric<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    f: T<'h>,
    kv: T<'h>,
    a: T<'h>,
    b: T<'h>,
    infinite: bool,
) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    let k1 = add(cx, &[kv, h.int(1)])?;
    let next = at(cx, f, kv, k1)?;
    let inv = call(cx, names::POW, &[f, h.int(-1)])?;
    let q = mul(cx, &[next, inv])?;
    let r = call(cx, "simplify", &[q])?;
    if r == h.int(1) || !free_of(cx, r, kv)? {
        return Ok(None);
    }
    let fa = at(cx, f, kv, a)?;
    let one_minus_r = {
        let nr = mul(cx, &[h.int(-1), r])?;
        add(cx, &[h.int(1), nr])?
    };
    if infinite {
        // converges only for |r| < 1, decided numerically
        let ar = call(cx, "abs", &[r])?;
        let lt = call(cx, "less", &[ar, h.int(1)])?;
        if !is_true(cx, lt) {
            return Ok(None);
        }
        let inv = call(cx, names::POW, &[one_minus_r, h.int(-1)])?;
        return Ok(Some(mul(cx, &[fa, inv])?));
    }
    // f(a) (1 - r^(b - a + 1)) / (1 - r)
    let na = mul(cx, &[h.int(-1), a])?;
    let count = add(cx, &[b, na, h.int(1)])?;
    let rn = call(cx, names::POW, &[r, count])?;
    let nrn = mul(cx, &[h.int(-1), rn])?;
    let top = add(cx, &[h.int(1), nrn])?;
    let inv = call(cx, names::POW, &[one_minus_r, h.int(-1)])?;
    let s = mul(cx, &[fa, top, inv])?;
    Ok(Some(call(cx, "simplify", &[s])?))
}

/// `c k^-s` summed to infinity with a numeric `s > 1`: `c (zeta(s) - (1^-s + ... + (a - 1)^-s))`, or `None`.
fn zeta_like<'h>(cx: &mut NativeCx<'_, '_, 'h>, f: T<'h>, kv: T<'h>, a: T<'h>) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    let View::Int(ia) = h.view(a) else { return Ok(None) };
    if !(1..=MAX_TERMS).contains(&ia) {
        return Ok(None);
    }
    // -s = k f'(k) / f(k), free of k for a power of k
    let df = call(cx, "d", &[f, kv])?;
    let inv = call(cx, names::POW, &[f, h.int(-1)])?;
    let q = mul(cx, &[kv, df, inv])?;
    let q = call(cx, "simplify", &[q])?;
    if !free_of(cx, q, kv)? {
        return Ok(None);
    }
    let s = mul(cx, &[h.int(-1), q])?;
    let gt = call(cx, "greater", &[s, h.int(1)])?;
    if !is_true(cx, gt) {
        return Ok(None);
    }
    let ks = call(cx, names::POW, &[kv, s])?;
    let c = mul(cx, &[f, ks])?;
    let c = call(cx, "simplify", &[c])?;
    if !free_of(cx, c, kv)? {
        return Ok(None);
    }
    let mut z = call(cx, "zeta", &[s])?;
    if ia > 1 {
        let ns = mul(cx, &[h.int(-1), s])?;
        let term = call(cx, names::POW, &[kv, ns])?;
        let first = call(cx, "table", &[term, kv, h.call(h.sym("range"), &[h.int(1), h.int(ia - 1)])])?;
        let head = call(cx, "sum", &[first])?;
        let nh = mul(cx, &[h.int(-1), head])?;
        z = add(cx, &[z, nh])?;
    }
    Ok(Some(mul(cx, &[c, z])?))
}

/// `piecewise([[v1, c1], [v2, c2], ...])`, `piecewise(cases, default)` (held): the value of the first case whose
/// condition is `true`, the default (0 without one) when all are `false`; while a condition is undecided, the
/// call with the cases decided `false` dropped and the rest evaluated.
pub(crate) fn piecewise<'h>(_lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let (cases, default) = match *args {
        [c] => (c, None),
        [c, d] => (c, Some(d)),
        _ => return Ok(Reply::Decline),
    };
    // the cases are held: evaluate the list structure, not its contents
    let Some(cs) = cx.as_list(cases) else { return Ok(Reply::Decline) };
    let fals = h.sym_term(h.sym(names::FALSE));
    let mut rest: Vec<T<'h>> = vec![];
    for &case in cs {
        let Some(&[v, c]) = cx.as_list(case) else { return Ok(Reply::Decline) };
        cx.tick(1)?;
        if rest.is_empty() {
            let cv = cx.eval(c)?;
            if is_true(cx, cv) {
                return Ok(Reply::Done(cx.eval(v)?));
            }
            if cv == fals {
                continue;
            }
            let vv = cx.eval(v)?;
            rest.push(h.call(h.sym(names::LIST), &[vv, cv]));
        } else {
            let cv = cx.eval(c)?;
            if cv == fals {
                continue;
            }
            let vv = cx.eval(v)?;
            rest.push(h.call(h.sym(names::LIST), &[vv, cv]));
        }
    }
    if rest.is_empty() {
        return Ok(Reply::Done(match default {
            Some(d) => cx.eval(d)?,
            None => h.int(0),
        }));
    }
    let mut out = vec![h.call(h.sym(names::LIST), &rest)];
    if let Some(d) = default {
        out.push(cx.eval(d)?);
    }
    let u = h.call(h.sym("piecewise"), &out);
    Ok(if u == t { Reply::Decline } else { Reply::Done(u) })
}

/// `product(f, [k, a, b])`, `product(f, [k, n])`, `product(f, [k, [x1, ...]])`: the product of the terms, for
/// integer bounds (at most `MAX_TERMS` terms) or a list of values (decision io-core.15); otherwise unevaluated.
pub(crate) fn iterated_product<'h>(_lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [f, spec] = *args else { return Ok(Reply::Decline) };
    let range = match cx.as_list(spec) {
        Some(&[kv, vals]) if cx.as_list(vals).is_some() => (kv, vals),
        Some(&[kv, a, b]) => match (h.view(a), h.view(b)) {
            (View::Int(ia), View::Int(ib)) if ib - ia < MAX_TERMS => (kv, h.call(h.sym("range"), &[a, b])),
            _ => return Ok(Reply::Decline),
        },
        Some(&[kv, b]) => match h.view(b) {
            View::Int(ib) if ib < MAX_TERMS => (kv, h.call(h.sym("range"), &[b])),
            _ => return Ok(Reply::Decline),
        },
        _ => return Ok(Reply::Decline),
    };
    if !matches!(h.view(range.0), View::Sym(_)) {
        return Ok(Reply::Decline);
    }
    let terms = call(cx, "table", &[f, range.0, range.1])?;
    if cx.as_list(terms).is_none() {
        return Ok(Reply::Done(terms));
    }
    Ok(Reply::Done(call(cx, "product", &[terms])?))
}
