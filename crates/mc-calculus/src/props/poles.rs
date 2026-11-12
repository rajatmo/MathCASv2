//! `function_poles` (decision R25P.6): the poles of a meromorphic function of one complex variable with their
//! orders, `[[p1, m1], [p2, m2], ...]`.
//!
//! - The function must be meromorphic where the condition holds ([`super::smooth`]): otherwise the call stays
//!   unevaluated.
//! - **Candidates** come from the pole atoms of [`super::sets`]: the solutions of `u == 0` (negative powers),
//!   `cos(u) == 0`, `sin(u) == 0`, ... (families `p(c(1))` with `c(1)` an integer), `u == 1` (`zeta`) and
//!   `u == -k`, `k = 0, 1, 2, ...` (the gamma family, only inside a bounded condition).
//! - **Order**: the smallest `m >= 0` with `limit(t^m f(p + t), t -> 0)` finite; `m = 0` is a removable point
//!   and no pole. The limit value is checked numerically against `t^m f(p + t)` at `t = 10^-12` (40 digits).
//!   A family is checked at `c(1) = 0`, `1`, `-1` (one order for all three, or no answer).
//! - **Condition**: with a condition the points are kept when it holds there (decided by `refine` under the
//!   facts, or numerically); families are enumerated for `c(1)` from -40 to 40 when the condition bounds `x`.
//! - Two families `a + P c(1)` and `a + P/2 + P c(1)` are merged into `a' + (P/2) c(1)` with the smaller
//!   nonnegative offset. Points are sorted by the degree of the factor they come from (1 for a rational point), then by their imaginary and real parts.
use super::sets::{self, Kind, Mode};
use super::spec::{Dom, Spec};
use crate::common::is_free;
use crate::term::Build;
use mc_api::{names, Store, View, T};
use mc_eval::{NativeCx, Res};

/// A candidate: the point (or a family in `c(1)`), whether it is a family, and the degree of its factor.
struct Cand<'h> {
    p: T<'h>,
    family: bool,
    degree: u32,
    /// a family over `c(1) >= 0` only (the gamma poles)
    nonneg: bool,
}

/// `function_poles` for the call `s`.
pub fn poles<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, s: &Spec<'h>) -> Res<Option<T<'h>>> {
    let h = b.heap();
    let [x] = s.vars.as_slice() else { return Ok(None) };
    let x = *x;
    if s.vector || s.dom == Dom::Integers {
        return Ok(None);
    }
    let mut s2 = Spec { dom: Dom::Complexes, ..clone_spec(s) };
    s2.dom = Dom::Complexes;
    // meromorphic where the condition holds
    let mut facts = s.facts.clone();
    if let Some(c) = s.cond {
        facts.push(c);
    }
    let Some(atoms) = sets::atoms(cx, b, &s.comps, &s.vars, true, Mode::Sing, &facts)? else { return Ok(None) };
    let others: Vec<T<'h>> = atoms.iter().filter(|a| a.kind != Kind::Pole).map(|a| a.t).collect();
    if !others.is_empty() && !matches!(sets::meets(cx, b, &s2, &others, s.cond)?, sets::Meet::No) {
        return Ok(None);
    }
    let f = s.f;
    let c1 = b.call("c", &[b.one()]);
    let bounded = s.cond.is_some_and(|c| bounds(b, c, x));
    // candidates
    let mut cands: Vec<Cand<'h>> = vec![];
    for a in atoms.iter().filter(|a| a.kind == Kind::Pole) {
        cx.tick(1)?;
        let (eqs, degree): (Vec<T<'h>>, u32) = match equation_of(cx, b, a.t)? {
            Some(GammaOrEq::Eq(e)) => {
                let d = match h.view(e) {
                    View::App { args: &[l, r], .. } => sets::poly_degree(b, cx.eval(b.sub(l, r))?, &[x]).unwrap_or(1000),
                    _ => 1000,
                };
                (vec![e], d)
            }
            Some(GammaOrEq::Gamma(u)) => {
                if !bounded {
                    // the poles `u = -k`, `k = 0, 1, 2, ...`, of a linear `u`: the family `(-k - u0)/a` (F27.5)
                    let a = cx.eval(b.call("d", &[u, x]))?;
                    if !is_free(b, a, x) || a == b.zero() {
                        return Ok(None);
                    }
                    let u0 = cx.eval(h.call(h.sym("rewrite"), &[u, h.call(h.sym(names::RULE), &[x, b.zero()])]))?;
                    let v = cx.eval(b.div(b.sub(b.neg(c1), u0), a))?;
                    let cond = b.call("and", &[b.call("element", &[c1, h.sym_term(h.sym("integers"))]), b.call("greater_equal", &[c1, b.zero()])]);
                    let fam = cx.eval(h.call(h.sym(mc_decide::CONDITIONAL), &[v, cond]))?;
                    if !cands.iter().any(|c| c.p == fam) {
                        cands.push(Cand { p: fam, family: true, degree: 1, nonneg: true });
                    }
                    continue;
                }
                let d = sets::poly_degree(b, u, &[x]).unwrap_or(1000);
                ((0..=40).map(|k| b.call("equal", &[u, b.int(-k)])).collect(), d)
            }
            None => return Ok(None),
        };
        for e in eqs {
            let sol = cx.eval(b.call("solve", &[e, x]))?;
            let Some(rows) = cx.as_list(sol) else { return Ok(None) };
            for &row in rows {
                let Some(&[rule]) = cx.as_list(row) else { return Ok(None) };
                let View::App { args: &[_, v], .. } = h.view(rule) else { return Ok(None) };
                let family = sets::is_head(b, v, mc_decide::CONDITIONAL);
                if !family && !is_free(b, v, x) {
                    return Ok(None);
                }
                if !cands.iter().any(|c| c.p == v) {
                    cands.push(Cand { p: v, family, degree, nonneg: false });
                }
            }
        }
    }
    merge_families(cx, b, &mut cands)?;
    // enumerate families inside a bounded condition
    let mut points: Vec<Cand<'h>> = vec![];
    for c in cands {
        if c.family && bounded {
            let View::App { args, .. } = h.view(c.p) else { continue };
            for k in -40..=40 {
                cx.tick(1)?;
                let v = cx.eval(h.call(h.sym("rewrite"), &[args[0], h.call(h.sym(names::RULE), &[c1, b.int(k)])]))?;
                if !points.iter().any(|q| q.p == v) {
                    points.push(Cand { p: v, family: false, degree: c.degree, nonneg: false });
                }
            }
        } else {
            points.push(c);
        }
    }
    // the condition at the points
    let mut kept = vec![];
    for c in points {
        cx.tick(1)?;
        if let Some(k) = s.cond {
            if c.family {
                return Ok(None);
            }
            let at = cx.eval(h.call(h.sym("rewrite"), &[k, h.call(h.sym(names::RULE), &[x, c.p])]))?;
            let r = cx.eval_assuming(&s.facts, b.call("refine", &[at]))?;
            if cx.is_false(r) {
                continue;
            }
            if !cx.is_true(r) {
                let rn = cx.eval(b.call("n", &[r]))?;
                if cx.is_false(rn) {
                    continue;
                }
                if !cx.is_true(rn) {
                    return Ok(None);
                }
            }
        }
        kept.push(c);
    }
    // orders: a rational function by the multiplicities of the roots of its reduced denominator
    let t = h.sym_term(h.sym("t$pole"));
    let rational = if sets::poly_degree(b, f, &[x]).is_none() && is_rational(b, f, x) {
        let tf = cx.eval(b.call("cancel", &[b.call("together", &[f])]))?;
        Some(cx.eval(b.call("denominator", &[tf]))?)
    } else {
        None
    };
    let mut out: Vec<(u32, (f64, f64), T<'h>, i64)> = vec![];
    for c in kept {
        let m = if c.family {
            let View::App { args, .. } = h.view(c.p) else { continue };
            let mut ms = vec![];
            for k in if c.nonneg { [0, 1, 2] } else { [0, 1, -1] } {
                let v = cx.eval(h.call(h.sym("rewrite"), &[args[0], h.call(h.sym(names::RULE), &[c1, b.int(k)])]))?;
                ms.push(order(cx, b, f, x, t, v)?);
            }
            if ms.iter().any(|m| m.is_none() || *m != ms[0]) {
                return Ok(None);
            }
            ms[0].unwrap()
        } else {
            let r = match &rational {
                Some(den) => rational_order(cx, b, *den, x, c.p)?,
                None => None,
            };
            match r {
                Some(m) => m,
                None => order(cx, b, f, x, t, c.p)?.unwrap_or(-1),
            }
        };
        if m < 0 {
            return Ok(None);
        }
        if m == 0 {
            continue;
        }
        let key = if c.family { (0.0, 0.0) } else { numeric_key(cx, b, c.p)? };
        let degree = if sets::real_rational(b, c.p) { 1 } else { c.degree };
        out.push((degree, key, c.p, m));
    }
    out.sort_by(|a, c| a.0.cmp(&c.0).then(a.1 .1.total_cmp(&c.1 .1)).then(a.1 .0.total_cmp(&c.1 .0)));
    let items: Vec<T<'h>> = out.iter().map(|(_, _, p, m)| cx.list(&[*p, b.int(*m)])).collect();
    Ok(Some(cx.list(&items)))
}

fn clone_spec<'h>(s: &Spec<'h>) -> Spec<'h> {
    Spec {
        f: s.f,
        comps: s.comps.clone(),
        vector: s.vector,
        cond: s.cond,
        ycond: s.ycond,
        vars: s.vars.clone(),
        yvars: s.yvars.clone(),
        dom: s.dom,
        dom_given: s.dom_given,
        gc: s.gc,
        facts: s.facts.clone(),
        strict: s.strict,
        speed: s.speed,
    }
}

/// A pole atom as an equation in `x`, or the argument `u` of a gamma pole (`u` a nonpositive integer).
enum GammaOrEq<'h> {
    Eq(T<'h>),
    Gamma(T<'h>),
}

fn equation_of<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, a: T<'h>) -> Res<Option<GammaOrEq<'h>>> {
    let h = b.heap();
    let _ = cx;
    if sets::is_head(b, a, "equal") {
        return Ok(Some(GammaOrEq::Eq(a)));
    }
    if sets::is_head(b, a, "and") {
        // sin(pi u) == 0 and re(u) <= 0
        let View::App { args, .. } = h.view(a) else { return Ok(None) };
        if let View::App { args: &[l, _], .. } = h.view(args[0]) {
            if let View::App { head, args: &[arg] } = h.view(l) {
                if b.sym_of(head) == Some(h.sym("sin")) {
                    let pi = h.sym_term(h.sym("pi"));
                    let u = cx.eval(b.div(arg, pi))?;
                    return Ok(Some(GammaOrEq::Gamma(u)));
                }
            }
        }
    }
    Ok(None)
}

/// Does the condition bound `x` (`abs(x) < r`, `abs(x - a) <= r`, `-a < x < b` and conjunctions with such)?
fn bounds<'h>(b: &Build<'h>, c: T<'h>, x: T<'h>) -> bool {
    let h = b.heap();
    let View::App { head, args } = h.view(c) else { return false };
    let Some(s) = b.sym_of(head) else { return false };
    match h.sym_name(s).as_str() {
        "and" => args.iter().any(|&a| bounds(b, a, x)),
        "less" | "less_equal" => sets::is_head(b, args[0], "abs") && !is_free(b, args[0], x) && is_free(b, args[1], x),
        "greater" | "greater_equal" => sets::is_head(b, args[1], "abs") && !is_free(b, args[1], x) && is_free(b, args[0], x),
        _ => false,
    }
}

/// Merge `a + P c(1)` and `a + P/2 + P c(1)` into one family with the period `P/2`.
fn merge_families<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, cands: &mut Vec<Cand<'h>>) -> Res<()> {
    let h = b.heap();
    let c1 = b.call("c", &[b.one()]);
    let parts = |cx: &mut NativeCx<'_, '_, 'h>, p: T<'h>| -> Res<Option<(T<'h>, T<'h>, T<'h>)>> {
        let View::App { args: &[v, cond], .. } = h.view(p) else { return Ok(None) };
        let per = cx.eval(b.call("d", &[v, c1]))?;
        let off = cx.eval(h.call(h.sym("rewrite"), &[v, h.call(h.sym(names::RULE), &[c1, b.zero()])]))?;
        Ok(Some((off, per, cond)))
    };
    let mut i = 0;
    while i < cands.len() {
        let mut merged = false;
        if cands[i].family {
            for j in i + 1..cands.len() {
                if !cands[j].family {
                    continue;
                }
                let (Some((o1, p1, k1)), Some((o2, p2, _))) = (parts(cx, cands[i].p)?, parts(cx, cands[j].p)?) else { continue };
                if p1 != p2 {
                    continue;
                }
                let half = cx.eval(b.div(p1, b.int(2)))?;
                let d = cx.eval(b.call("simplify", &[b.sub(o2, o1)]))?;
                let nd = cx.eval(b.call("simplify", &[b.neg(d)]))?;
                if d != half && nd != half {
                    continue;
                }
                // the offset in [0, P/2)
                let mut o = o1;
                for j in [0, 1, -1, 2, -2] {
                    let c = cx.eval(b.add(&[o1, b.mul(&[b.int(j), half])]))?;
                    let lo = cx.eval(b.call("greater_equal", &[c, b.zero()]))?;
                    let hi = cx.eval(b.call("less", &[c, half]))?;
                    if cx.is_true(lo) && cx.is_true(hi) {
                        o = c;
                        break;
                    }
                }
                let v = cx.eval(b.add(&[o, b.mul(&[half, c1])]))?;
                let fam = cx.eval(h.call(h.sym(mc_decide::CONDITIONAL), &[v, k1]))?;
                cands[i].p = fam;
                cands.remove(j);
                merged = true;
                break;
            }
        }
        if !merged {
            i += 1;
        }
    }
    Ok(())
}

/// The value of `p` as a pair of machine numbers (real and imaginary parts) for sorting.
fn numeric_key<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, p: T<'h>) -> Res<(f64, f64)> {
    let h = b.heap();
    let rd = |cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>| -> Res<f64> {
        let v = cx.eval(b.call("n", &[t]))?;
        Ok(match h.view(v) {
            View::Real(y) => y,
            View::Int(k) => k as f64,
            _ => 0.0,
        })
    };
    let re = rd(cx, b.call("re", &[p]))?;
    let im = rd(cx, b.call("im_part", &[p]))?;
    Ok((re, im))
}

/// Is `f` a rational function of `x` (sums, products, integer powers of polynomials)?
fn is_rational<'h>(b: &Build<'h>, f: T<'h>, x: T<'h>) -> bool {
    let h = b.heap();
    match h.view(f) {
        View::App { head, args } => {
            let Some(s) = b.sym_of(head) else { return false };
            if s == b.hd.add || s == b.hd.mul {
                args.iter().all(|&a| is_rational(b, a, x))
            } else if s == b.hd.pow {
                matches!(h.view(args[1]), View::Int(_)) && is_rational(b, args[0], x)
            } else {
                is_free(b, f, x)
            }
        }
        _ => true,
    }
}

/// The multiplicity of the root `p` of the polynomial `den`: the number of its derivatives (from the 0th) that
/// vanish at `p` (to 40 digits, |value| < 10^-30); `None` beyond 12.
fn rational_order<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, den: T<'h>, x: T<'h>, p: T<'h>) -> Res<Option<i64>> {
    let h = b.heap();
    let mut g = den;
    for m in 0..=12i64 {
        cx.tick(1)?;
        let v = cx.eval(h.call(h.sym("rewrite"), &[g, h.call(h.sym(names::RULE), &[x, p])]))?;
        let v = cx.eval(b.call("n", &[v, b.int(40)]))?;
        let re = cx.eval(b.call("re", &[v]))?;
        let im = cx.eval(b.call("im_part", &[v]))?;
        let small = cx.eval(b.call("less", &[b.add(&[b.powi(re, 2), b.powi(im, 2)]), b.pow(b.int(10), b.int(-60))]))?;
        if !cx.is_true(small) {
            return Ok(Some(m));
        }
        g = cx.eval(b.call("d", &[g, x]))?;
    }
    Ok(None)
}

/// The order of the pole of `f` at `p` (0 for a regular point), checked numerically; `None` when not decided.
fn order<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, x: T<'h>, t: T<'h>, p: T<'h>) -> Res<Option<i64>> {
    let h = b.heap();
    let g = cx.eval(h.call(h.sym("rewrite"), &[f, h.call(h.sym(names::RULE), &[x, b.add(&[p, t])])]))?;
    for m in 0..=8i64 {
        cx.tick(1)?;
        let e = b.mul(&[b.pow(t, b.int(m)), g]);
        let l = cx.eval(b.call("limit", &[e, h.call(h.sym(names::RULE), &[t, b.zero()])]))?;
        if sets::is_head(b, l, names::FAILURE) {
            // an unbounded function on both sides (an odd order): not finite
            let no_limit = matches!(h.view(l), View::App { args, .. } if args.first().is_some_and(|&a| a == h.sym_term(h.sym("no_limit"))));
            if no_limit {
                continue;
            }
            return Ok(None);
        }
        if sets::is_head(b, l, "limit") {
            // no limit by the engine: the growth of |f| towards the point decides (F27.5)
            return numeric_order(cx, b, f, x, p);
        }
        let ln = cx.eval(b.call("n", &[l]))?;
        let finite = match h.view(ln) {
            View::Real(y) => y.is_finite(),
            View::Int(_) => true,
            View::App { head, .. } => b.sym_of(head) == Some(h.sym("complex")),
            _ => false,
        };
        if !finite {
            continue;
        }
        // numeric check: t^m f(p + t) at t = 10^-12 near the limit
        let at = cx.eval(h.call(h.sym("rewrite"), &[e, h.call(h.sym(names::RULE), &[t, b.pow(b.int(10), b.int(-12))])]))?;
        let sq = |cx: &mut NativeCx<'_, '_, 'h>, u: T<'h>| -> Res<T<'h>> {
            let v = cx.eval(b.call("n", &[u, b.int(40)]))?;
            let re = cx.eval(b.call("re", &[v]))?;
            let im = cx.eval(b.call("im_part", &[v]))?;
            cx.eval(b.add(&[b.powi(re, 2), b.powi(im, 2)]))
        };
        let d = sq(cx, b.sub(at, l))?;
        let scale = sq(cx, l)?;
        let ok = cx.eval(b.call("less_equal", &[d, b.mul(&[b.add(&[b.one(), scale]), b.pow(b.int(10), b.int(-12))])]))?;
        if !cx.is_true(ok) {
            // the limit contradicts the values (a wrong limit): the growth of |f| decides
            return numeric_order(cx, b, f, x, p);
        }
        return Ok(Some(m));
    }
    Ok(None)
}

/// The order of the pole of `f` at `p` from the growth of `|f|` along a ray: `|f(p + t)| ~ t^-m` at
/// `t = 10^-6 e^(0.6 i)` and `10^-9 e^(0.6 i)`; `m = 0` when the values stay bounded; `None` for a growth that is
/// not near an integer.
fn numeric_order<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, x: T<'h>, p: T<'h>) -> Res<Option<i64>> {
    let h = b.heap();
    let imu = h.sym_term(h.sym("im"));
    let mut sizes = vec![];
    for e in [-6i64, -9, -12] {
        let dir = b.add(&[b.rat(4, 5), b.mul(&[b.rat(3, 5), imu])]);
        let pt = cx.eval(b.add(&[p, b.mul(&[b.pow(b.int(10), b.int(e)), dir])]))?;
        let v = cx.eval(h.call(h.sym("rewrite"), &[f, h.call(h.sym(names::RULE), &[x, pt])]))?;
        let m = cx.eval(h.call(h.sym("n"), &[b.call("abs", &[v])]))?;
        match h.view(m) {
            View::Real(r) if r.is_finite() => sizes.push(r),
            View::Int(k) => sizes.push(k as f64),
            _ => return Ok(None),
        }
    }
    if sizes[2] < 1e3 * (1.0 + sizes[0]) && sizes[2] < 10.0 * (1.0 + sizes[0]) {
        return Ok(Some(0));
    }
    let m1 = (sizes[1] / sizes[0]).log10() / 3.0;
    let m2 = (sizes[2] / sizes[1]).log10() / 3.0;
    let m = m2.round();
    if m >= 1.0 && (m2 - m).abs() < 0.08 && (m1 - m).abs() < 0.15 {
        return Ok(Some(m as i64));
    }
    Ok(None)
}
