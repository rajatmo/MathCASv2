//! `function_poles` (decision R25P.6): the poles of a meromorphic function of one complex variable with their
//! orders, `[[p1, m1], [p2, m2], ...]`.
//!
//! - The function must be meromorphic where the condition holds ([`super::smooth`]): otherwise the call stays
//!   unevaluated.
//! - **Candidates** come from the pole atoms of [`super::sets`]: the solutions of `u == 0` (negative powers),
//!   `cos(u) == 0`, `sin(u) == 0`, ... (families `p(c(1))` with `c(1)` an integer), `u == 1` (`zeta`) and
//!   `u == -k`, `k = 0, 1, 2, ...` (the gamma family, only inside a bounded condition).
//! - **Order**: the smallest `m >= 0` with `limit(t^m f(p + t), t -> 0)` finite; `m = 0` is a removable point
//!   and no pole. The limit value is checked numerically against `t^m f(p + t)` at `t = 10^-12` (40 digits): a
//!   disagreement leaves the call (values near p never give an order). A family is taken only when f is proved
//!   periodic with its step (then the orders at `c(1) = 0`, `1`, `-1` are those of every member); the gamma family
//!   (`c(1) >= 0`) only inside a bounded condition. A root of a rational function's denominator has the multiplicity
//!   of the derivatives that vanish there, each decided exactly (simplified to 0, or a ball excluding 0).
//! - **Condition**: with a condition the points are kept when it holds there (decided by `refine` under the
//!   facts; undecided, the call stays); families are enumerated for `c(1)` from -40 to 40 when the condition
//!   bounds `x`.
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
    match composed(cx, b, s, x)? {
        Comp::Done(t) => return Ok(Some(t)),
        Comp::Decline => return Ok(None),
        Comp::Other => {}
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
                // the poles u = -k inside the condition have k <= max |u| over its disc: that bound must be proved
                // and small, or the list could miss poles beyond the ones enumerated
                let Some(c) = s.cond else { return Ok(None) };
                let Some(kmax) = (match disc(cx, b, c, x)? {
                    Some((a, r, _)) => abs_bound(cx, b, u, x, a, r)?,
                    None => None,
                }) else {
                    return Ok(None);
                };
                if kmax > 40.0 {
                    return Ok(None);
                }
                ((0..=kmax.floor() as i64).map(|k| b.call("equal", &[u, b.int(-k)])).collect(), d)
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
                return Ok(None);
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
            // one order for every member only when f is periodic with the family's step (proved: f(x + step) - f(x)
            // simplifies to 0); the orders at three members prove nothing about the others
            if c.nonneg {
                // gamma(u) of an affine u: every pole is simple (DLMF 5.2.1, 5.5.1); other functions of the family
                // are not read
                let simple = matches!(h.view(f), View::App { head, args: &[u] } if b.sym_of(head) == Some(h.sym("gamma")) && sets::poly_degree(b, u, &[x]) == Some(1));
                if !simple {
                    return Ok(None);
                }
                out.push((1, (0.0, 0.0), c.p, 1));
                continue;
            }
            let step = {
                let m1 = cx.eval(h.call(h.sym("rewrite"), &[args[0], h.call(h.sym(names::RULE), &[c1, b.one()])]))?;
                let m0 = cx.eval(h.call(h.sym("rewrite"), &[args[0], h.call(h.sym(names::RULE), &[c1, b.zero()])]))?;
                cx.eval(b.sub(m1, m0))?
            };
            let shifted = cx.eval(h.call(h.sym("rewrite"), &[f, h.call(h.sym(names::RULE), &[x, b.add(&[x, step])])]))?;
            if step == b.zero() {
                return Ok(None);
            }
            if cx.eval(b.call("simplify", &[b.sub(shifted, f)]))? != b.zero() {
                // or the step is a multiple of the period proved by `function_period` (tanh: i pi)
                let p = cx.eval(b.call("function_period", &[f, x, h.sym_term(h.sym("complexes"))]))?;
                let multiple = match cx.as_list(p) {
                    Some(&[p]) if p != b.zero() => {
                        let r = cx.eval(b.call("simplify", &[b.div(step, p)]))?;
                        matches!(h.view(r), View::Int(k) if k != 0)
                    }
                    _ => false,
                };
                if !multiple {
                    return Ok(None);
                }
            }
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
        let degree = if c.family { c.degree } else { point_degree(cx, b, c.p, c.degree)? };
        out.push((degree, key, c.p, m));
    }
    Ok(Some(sorted_list(cx, b, out)))
}

/// The list `[[p1, m1], ...]` of points (degree of the point, (re, im), point, order), ordered by the degree, then
/// by the modulus, the imaginary and the real part (moduli equal to 9 digits count as equal).
fn sorted_list<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, mut out: Vec<(u32, (f64, f64), T<'h>, i64)>) -> T<'h> {
    let modulus = |k: &(f64, f64)| {
        let a = k.0.hypot(k.1);
        if a == 0.0 {
            0
        } else {
            (a.ln() * 1e9).round() as i64
        }
    };
    out.sort_by(|a, c| {
        a.0.cmp(&c.0)
            .then((a.1 .0.hypot(a.1 .1) != 0.0).cmp(&(c.1 .0.hypot(c.1 .1) != 0.0)))
            .then(modulus(&a.1).cmp(&modulus(&c.1)))
            .then(a.1 .1.total_cmp(&c.1 .1))
            .then(a.1 .0.total_cmp(&c.1 .0))
    });
    let items: Vec<T<'h>> = out.iter().map(|(_, _, p, m)| cx.list(&[*p, b.int(*m)])).collect();
    cx.list(&items)
}

/// The degree used to order a point: 1 for a Gaussian rational, else the degree of its minimal polynomial over Q
/// when it is an algebraic number, else `fallback` (the degree of the equation it solves).
fn point_degree<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, p: T<'h>, fallback: u32) -> Res<u32> {
    let h = b.heap();
    let rational = |t: T<'h>| matches!(h.view(t), View::Int(_) | View::BigInt { .. } | View::Rational { .. });
    let re = cx.eval(b.call("re", &[p]))?;
    let im = cx.eval(b.call("im_part", &[p]))?;
    if rational(re) && rational(im) {
        return Ok(1);
    }
    let t = h.sym_term(h.sym("t$pole"));
    let mp = cx.eval(b.call("minimal_polynomial", &[p, t]))?;
    if sets::is_head(b, mp, "minimal_polynomial") || sets::is_head(b, mp, names::FAILURE) {
        return Ok(fallback);
    }
    Ok(sets::poly_degree(b, mp, &[t]).filter(|&d| d >= 1).unwrap_or(fallback))
}

/// The disc of a condition `abs(x - a) <= r` (`<`, `>=`, `>` with the sides exchanged): (a, r, strict), with a
/// and r numbers.
fn disc<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, c: T<'h>, x: T<'h>) -> Res<Option<(T<'h>, T<'h>, bool)>> {
    let h = b.heap();
    let View::App { head, args: &[l, r] } = h.view(c) else { return Ok(None) };
    let Some(hs) = b.sym_of(head) else { return Ok(None) };
    let (ab, rad, strict) = match h.sym_name(hs).as_str() {
        "less" => (l, r, true),
        "less_equal" => (l, r, false),
        "greater" => (r, l, true),
        "greater_equal" => (r, l, false),
        _ => return Ok(None),
    };
    let View::App { head: ha, args: &[inner] } = h.view(ab) else { return Ok(None) };
    if b.sym_of(ha) != Some(h.sym("abs")) || !is_free(b, rad, x) {
        return Ok(None);
    }
    // inner = x - a with a free of x
    let slope = cx.eval(b.call("d", &[inner, x]))?;
    let a = cx.eval(b.call("expand", &[b.sub(x, inner)]))?;
    if slope != b.one() || !is_free(b, a, x) {
        return Ok(None);
    }
    let numeric = |cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>| -> Res<bool> {
        let v = cx.eval(b.call("n", &[t]))?;
        Ok(b.ar.num(v).is_some())
    };
    if !numeric(cx, a)? || !numeric(cx, rad)? {
        return Ok(None);
    }
    Ok(Some((a, rad, strict)))
}

/// An upper bound of |u(x)| over the disc |x - a| <= r for a polynomial u: sum |d_j| r^j over the coefficients of
/// u(a + t), each term bounded above from its 30-digit value; `None` when u is not such a polynomial.
fn abs_bound<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, u: T<'h>, x: T<'h>, a: T<'h>, r: T<'h>) -> Res<Option<f64>> {
    let h = b.heap();
    let a = cx.eval(a)?;
    if !is_free(b, a, x) || sets::poly_degree(b, u, &[x]).is_none() {
        return Ok(None);
    }
    let t = h.sym_term(h.sym("t$pole"));
    let shifted = cx.eval(b.call("expand", &[h.call(h.sym("rewrite"), &[u, h.call(h.sym(names::RULE), &[x, b.add(&[a, t])])])]))?;
    let cl = cx.eval(b.call("coefficient_list", &[shifted, t]))?;
    let Some(cs) = cx.as_list(cl).map(|c| c.to_vec()) else { return Ok(None) };
    let mut total = 0.0f64;
    for (j, c) in cs.into_iter().enumerate() {
        let term = b.mul(&[b.call("abs", &[c]), b.powi(r, j as i64)]);
        let v = cx.eval(b.call("n", &[term, b.int(30)]))?;
        let y = match h.view(v) {
            View::Int(k) => k as f64,
            _ => match super::proof::bounds(b, v) {
                Some((_, hi)) => hi,
                None => return Ok(None),
            },
        };
        if !y.is_finite() || y < 0.0 {
            return Ok(None);
        }
        total += y * (1.0 + 1e-12) + 1e-300;
    }
    Ok(Some(total * (1.0 + 1e-12) + 1e-12))
}

/// The answer with the parameters `c(j)` of its families renamed by the option `generated_parameters: g` (a
/// function of j, as `j => subscript(k, j)`).
pub fn with_parameters<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, args: &[T<'h>], r: Option<T<'h>>) -> Res<Option<T<'h>>> {
    let h = b.heap();
    let Some(r) = r else { return Ok(None) };
    let Some(&last) = args.last() else { return Ok(Some(r)) };
    let View::App { head, args: rules } = h.view(last) else { return Ok(Some(r)) };
    if b.sym_of(head) != Some(h.sym("options")) {
        return Ok(Some(r));
    }
    for &rule in rules {
        let View::App { args: &[k, g], .. } = h.view(rule) else { continue };
        if k != h.sym_term(h.sym("generated_parameters")) {
            continue;
        }
        let mut subs = vec![];
        for j in 1..=4 {
            let cj = b.call("c", &[b.int(j)]);
            if !is_free(b, r, cj) {
                let v = cx.eval(h.app(g, &[b.int(j)]))?;
                subs.push(h.call(h.sym(names::RULE), &[cj, v]));
            }
        }
        if subs.is_empty() {
            return Ok(Some(r));
        }
        return Ok(Some(cx.eval(h.call(h.sym("rewrite"), &[r, cx.list(&subs)]))?));
    }
    Ok(Some(r))
}

/// The outcome of [`composed`].
enum Comp<'h> {
    /// the poles
    Done(T<'h>),
    /// the form is recognised but no answer is proved: the call stays
    Decline,
    /// not this form
    Other,
}

/// Where a library function has its poles (all simple): gamma at the integers `c <= 0`, the circular functions at
/// `pi (c + h)` and the hyperbolic ones at `i pi (c + h)` (h = 1/2 for tan, sec, tanh, sech; 0 for the others).
#[derive(Clone, Copy)]
enum PoleSet {
    Gamma,
    Periodic { half: bool, imaginary: bool },
}

impl PoleSet {
    fn of(name: &str) -> Option<PoleSet> {
        Some(match name {
            "gamma" => PoleSet::Gamma,
            "tan" | "sec" => PoleSet::Periodic { half: true, imaginary: false },
            "cot" | "csc" => PoleSet::Periodic { half: false, imaginary: false },
            "tanh" | "sech" => PoleSet::Periodic { half: true, imaginary: true },
            "coth" | "csch" => PoleSet::Periodic { half: false, imaginary: true },
            _ => return None,
        })
    }
    /// The pole for the integer (or the symbol) c.
    fn value<'h>(self, b: &Build<'h>, c: T<'h>) -> T<'h> {
        let h = b.heap();
        match self {
            PoleSet::Gamma => c,
            PoleSet::Periodic { half, imaginary } => {
                let off = if half { b.add(&[c, b.rat(1, 2)]) } else { c };
                let pi = h.sym_term(h.sym("pi"));
                if imaginary {
                    b.mul(&[h.sym_term(h.sym("im")), pi, off])
                } else {
                    b.mul(&[pi, off])
                }
            }
        }
    }
}

/// The poles of `g(u)` for g with a known set of simple poles ([`PoleSet`]) and u a polynomial in x with rational
/// coefficients (degree 2 or more; degree 1 with a bounded condition for gamma): the points where u takes a pole
/// of g. The order at such a point x0 is the multiplicity of x0 as a root of u - v, v the pole of g:
///
/// - for the circular and hyperbolic functions every nonzero pole v is a nonzero rational multiple of pi (or i pi),
///   a transcendental number (Lindemann 1882), while the critical values u(xi), u'(xi) = 0, of a polynomial with
///   rational coefficients are algebraic: every root of u - v is simple. At v = 0 the roots of u must be simple
///   (gcd(u, u') constant), else the call stays;
/// - for gamma a root of u + k is multiple exactly when -k is a critical value: without a condition the critical
///   values (the roots of the resultant of u - y and u' in y) must not be integers <= 0; inside a disc each root's
///   multiplicity is decided exactly.
///
/// Without a condition the answer is the family of the solutions of `u == v(c(1))` (with `c(1) <= 0` for gamma);
/// a square root of `kappa L` with kappa > 0 and L linear in c(1) with coprime integer coefficients is written
/// `sqrt(kappa) sqrt(L)` (and `i sqrt(-L)` when L < 0 on the whole family). Inside a disc `abs(x - a) <= r` the
/// poles v with |v| <= max |u| on the disc (a proved bound: the sum of |d_j| r^j over the coefficients of
/// u(a + t)) are enumerated and each root of u - v kept when its distance to a is proved at most r.
fn composed<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, s: &Spec<'h>, x: T<'h>) -> Res<Comp<'h>> {
    let h = b.heap();
    let View::App { head, args: &[u] } = h.view(s.f) else { return Ok(Comp::Other) };
    let Some(set) = b.sym_of(head).and_then(|g| PoleSet::of(&h.sym_name(g))) else { return Ok(Comp::Other) };
    let Some(deg) = sets::poly_degree(b, u, &[x]) else { return Ok(Comp::Other) };
    if deg == 0 || !s.facts.is_empty() {
        return Ok(Comp::Other);
    }
    let cl = cx.eval(b.call("coefficient_list", &[u, x]))?;
    let rational = |t: T<'h>| matches!(h.view(t), View::Int(_) | View::BigInt { .. } | View::Rational { .. });
    if !cx.as_list(cl).is_some_and(|cs| cs.iter().all(|&c| rational(c))) {
        return Ok(Comp::Other);
    }
    let gamma = matches!(set, PoleSet::Gamma);
    let c1 = b.call("c", &[b.one()]);
    let du = cx.eval(b.call("d", &[u, x]))?;
    // the roots of u at a pole 0 of the circular and hyperbolic functions must be simple
    let zero_pole = matches!(set, PoleSet::Periodic { half: false, .. });
    if zero_pole && deg >= 2 {
        let g = cx.eval(b.call("polynomial_gcd", &[u, du]))?;
        if !is_free(b, g, x) {
            return Ok(Comp::Decline);
        }
    }
    match s.cond {
        None => {
            if deg < 2 {
                return Ok(Comp::Other);
            }
            // a family in closed form: the two square roots of a quadratic (both roots whatever the branch);
            // the radical forms of a cubic or quartic pair their roots by branch choices not proved for every c
            if deg > 2 {
                return Ok(Comp::Decline);
            }
            if gamma {
                // no critical value of u is an integer <= 0
                let y = h.sym_term(h.sym("y$pole"));
                let res = cx.eval(b.call("resultant", &[b.sub(u, y), du, x]))?;
                let sol = cx.eval(b.call("solve", &[b.call("equal", &[res, b.zero()]), y]))?;
                let Some(rows) = cx.as_list(sol).map(|r| r.to_vec()) else { return Ok(Comp::Decline) };
                for row in rows {
                    let Some(&[rule]) = cx.as_list(row) else { return Ok(Comp::Decline) };
                    let View::App { args: &[_, v], .. } = h.view(rule) else { return Ok(Comp::Decline) };
                    match h.view(v) {
                        View::Int(k) if k <= 0 => return Ok(Comp::Decline),
                        View::BigInt { .. } => return Ok(Comp::Decline),
                        _ => {}
                    }
                }
            }
            // solved in a plain symbol, renamed c(1) at the end
            let cs = h.sym_term(h.sym("c$pole"));
            let sol = cx.eval(b.call("solve", &[b.call("equal", &[u, set.value(b, cs)]), x]))?;
            let Some(rows) = cx.as_list(sol).map(|r| r.to_vec()) else { return Ok(Comp::Decline) };
            let ints = b.call("element", &[c1, h.sym_term(h.sym("integers"))]);
            let cond = if gamma { b.call("and", &[ints, b.call("less_equal", &[c1, b.zero()])]) } else { ints };
            let mut items = vec![];
            for row in rows {
                cx.tick(1)?;
                let Some(&[rule]) = cx.as_list(row) else { return Ok(Comp::Decline) };
                let View::App { args: &[_, v], .. } = h.view(rule) else { return Ok(Comp::Decline) };
                if !is_free(b, v, x) || sets::is_head(b, v, mc_decide::CONDITIONAL) {
                    return Ok(Comp::Decline);
                }
                let v = split_roots(cx, b, v, cs, gamma)?;
                let v = cx.eval(h.call(h.sym("rewrite"), &[v, h.call(h.sym(names::RULE), &[cs, c1])]))?;
                let fam = cx.eval(h.call(h.sym(mc_decide::CONDITIONAL), &[v, cond]))?;
                if !items.contains(&fam) {
                    items.push(fam);
                }
            }
            let items: Vec<T<'h>> = items.into_iter().map(|p| cx.list(&[p, b.one()])).collect();
            Ok(Comp::Done(cx.list(&items)))
        }
        Some(c) => {
            if deg < 2 && !gamma {
                return Ok(Comp::Other);
            }
            let Some((a, r, strict)) = disc(cx, b, c, x)? else { return Ok(Comp::Other) };
            let Some(kmax) = abs_bound(cx, b, u, x, a, r)? else { return Ok(Comp::Decline) };
            // the poles v of g with |v| <= kmax
            let range: Vec<i64> = if gamma {
                if kmax > 40.0 {
                    return Ok(Comp::Decline);
                }
                (0..=kmax.floor() as i64).map(|k| -k).collect()
            } else {
                let m = (kmax / std::f64::consts::PI).ceil() as i64 + 1;
                if m > 40 {
                    return Ok(Comp::Decline);
                }
                (-m..=m).collect()
            };
            let mut out: Vec<(u32, (f64, f64), T<'h>, i64)> = vec![];
            for k in range {
                cx.tick(1)?;
                let v = cx.eval(set.value(b, b.int(k)))?;
                let p = cx.eval(b.sub(u, v))?;
                let sol = cx.eval(b.call("solve", &[b.call("equal", &[p, b.zero()]), x]))?;
                let Some(rows) = cx.as_list(sol).map(|r| r.to_vec()) else { return Ok(Comp::Decline) };
                for row in rows {
                    cx.tick(1)?;
                    let Some(&[rule]) = cx.as_list(row) else { return Ok(Comp::Decline) };
                    let View::App { args: &[_, z], .. } = h.view(rule) else { return Ok(Comp::Decline) };
                    if !is_free(b, z, x) || sets::is_head(b, z, mc_decide::CONDITIONAL) {
                        return Ok(Comp::Decline);
                    }
                    if out.iter().any(|q| q.2 == z) {
                        continue;
                    }
                    // |z - a|^2 - r^2: negative inside, 0 on the circle
                    let w = b.sub(z, a);
                    let d2 = b.sub(b.add(&[b.powi(b.call("re", &[w]), 2), b.powi(b.call("im_part", &[w]), 2)]), b.powi(r, 2));
                    let inside = match super::proof::closed_sign(cx, b, d2)? {
                        Some(-1) => true,
                        Some(0) => !strict,
                        Some(_) => false,
                        None => return Ok(Comp::Decline),
                    };
                    if !inside {
                        continue;
                    }
                    // the multiplicity of z as a root of u - v (simple for a transcendental v)
                    let m = if gamma || v == b.zero() {
                        match rational_order(cx, b, p, x, z)? {
                            Some(m) if m >= 1 => m,
                            _ => return Ok(Comp::Decline),
                        }
                    } else {
                        1
                    };
                    let key = numeric_key(cx, b, z)?;
                    let degree = point_degree(cx, b, z, deg)?;
                    out.push((degree, key, z, m));
                }
            }
            Ok(Comp::Done(sorted_list(cx, b, out)))
        }
    }
}

/// `v` with each square root of `kappa L` (kappa > 0, L = n c + m with coprime integers, n > 0) written
/// `sqrt(kappa) sqrt(L)`, and `sqrt(L)` as `i sqrt(-L)` when `nonpos` (c <= 0) makes L negative on the family.
fn split_roots<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, v: T<'h>, c: T<'h>, nonpos: bool) -> Res<T<'h>> {
    let h = b.heap();
    let mut roots = vec![];
    collect_sqrt(b, v, &mut roots);
    let mut rules = vec![];
    for rt in roots {
        let View::App { args: &[radicand, _], .. } = h.view(rt) else { continue };
        if is_free(b, radicand, c) {
            continue;
        }
        // linear in c: a nonzero derivative free of c
        let alpha = cx.eval(b.call("d", &[radicand, c]))?;
        if alpha == b.zero() || !is_free(b, alpha, c) {
            continue;
        }
        let beta = cx.eval(h.call(h.sym("rewrite"), &[radicand, h.call(h.sym(names::RULE), &[c, b.zero()])]))?;
        let q = cx.eval(b.call("simplify", &[b.div(beta, alpha)]))?;
        let (num, den) = match h.view(q) {
            View::Int(k) => (k, 1),
            View::Rational { .. } => {
                let n = cx.eval(b.call("numerator", &[q]))?;
                let d = cx.eval(b.call("denominator", &[q]))?;
                match (h.view(n), h.view(d)) {
                    (View::Int(n), View::Int(d)) if d > 0 => (n, d),
                    _ => continue,
                }
            }
            _ => continue,
        };
        let kappa = cx.eval(b.call("simplify", &[b.div(alpha, b.int(den))]))?;
        if super::proof::closed_sign(cx, b, kappa)? != Some(1) {
            continue;
        }
        let l = b.add(&[b.mul(&[b.int(den), c]), b.int(num)]);
        let half = b.rat(1, 2);
        // L <= num < 0 for every c <= 0
        let root_l = if nonpos && num < 0 { b.mul(&[h.sym_term(h.sym("im")), b.pow(b.neg(l), half)]) } else { b.pow(l, half) };
        let new = b.mul(&[b.pow(kappa, half), root_l]);
        rules.push(h.call(h.sym(names::RULE), &[rt, new]));
    }
    if rules.is_empty() {
        return Ok(v);
    }
    cx.eval(h.call(h.sym("rewrite"), &[v, cx.list(&rules)]))
}

fn collect_sqrt<'h>(b: &Build<'h>, t: T<'h>, out: &mut Vec<T<'h>>) {
    // shared subterms are visited once (a term can be a tree exponentially larger than itself)
    collect_sqrt_in(b, t, out, &mut mc_api::walk::seen())
}

fn collect_sqrt_in<'h>(b: &Build<'h>, t: T<'h>, out: &mut Vec<T<'h>>, seen: &mut mc_api::walk::Seen<T<'h>>) {
    if !seen.insert(t) {
        return;
    }
    let h = b.heap();
    if let View::App { head, args } = h.view(t) {
        if b.sym_of(head) == Some(b.hd.pow) && args.len() == 2 && args[1] == b.rat(1, 2) {
            if !out.contains(&t) {
                out.push(t);
            }
            return;
        }
        for &a in args {
            collect_sqrt_in(b, a, out, seen);
        }
    }
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
    // shared subterms are decided once (a term can be a tree exponentially larger than itself)
    bounds_in(b, c, x, &mut mc_api::walk::memo())
}

fn bounds_in<'h>(b: &Build<'h>, c: T<'h>, x: T<'h>, memo: &mut mc_api::walk::Memo<T<'h>, bool>) -> bool {
    mc_api::walk::cached(memo, c, |memo| {
        let h = b.heap();
        let View::App { head, args } = h.view(c) else { return false };
        let Some(s) = b.sym_of(head) else { return false };
        match h.sym_name(s).as_str() {
            "and" => args.iter().any(|&a| bounds_in(b, a, x, memo)),
            "less" | "less_equal" => sets::is_head(b, args[0], "abs") && !is_free(b, args[0], x) && is_free(b, args[1], x),
            "greater" | "greater_equal" => sets::is_head(b, args[1], "abs") && !is_free(b, args[1], x) && is_free(b, args[0], x),
            _ => false,
        }
    })
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
    // shared subterms are decided once (a term can be a tree exponentially larger than itself)
    is_rational_in(b, f, x, &mut mc_api::walk::memo())
}

fn is_rational_in<'h>(b: &Build<'h>, f: T<'h>, x: T<'h>, memo: &mut mc_api::walk::Memo<T<'h>, bool>) -> bool {
    mc_api::walk::cached(memo, f, |memo| {
        let h = b.heap();
        match h.view(f) {
            View::App { head, args } => {
                let Some(s) = b.sym_of(head) else { return false };
                if s == b.hd.add || s == b.hd.mul {
                    args.iter().all(|&a| is_rational_in(b, a, x, memo))
                } else if s == b.hd.pow {
                    matches!(h.view(args[1]), View::Int(_)) && is_rational_in(b, args[0], x, memo)
                } else {
                    is_free(b, f, x)
                }
            }
            _ => true,
        }
    })
}

/// The multiplicity of the root `p` of the polynomial `den`: the number of its derivatives (from the 0th) that
/// vanish at `p`. A derivative vanishes when its value simplifies to 0 and does not when the 40-digit ball of
/// |value|^2 excludes 0; undecided (or beyond 12): `None`.
fn rational_order<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, den: T<'h>, x: T<'h>, p: T<'h>) -> Res<Option<i64>> {
    let h = b.heap();
    let mut g = den;
    for m in 0..=12i64 {
        cx.tick(1)?;
        let v = cx.eval(h.call(h.sym("rewrite"), &[g, h.call(h.sym(names::RULE), &[x, p])]))?;
        let w = cx.eval(b.call("n", &[v, b.int(40)]))?;
        let re = cx.eval(b.call("re", &[w]))?;
        let im = cx.eval(b.call("im_part", &[w]))?;
        let sq = b.add(&[b.powi(re, 2), b.powi(im, 2)]);
        if super::proof::closed_sign(cx, b, sq)? == Some(1) {
            return Ok(Some(m));
        }
        let s = cx.eval(b.call("simplify", &[v]))?;
        if s != b.zero() {
            let r = cx.eval(b.call("root_reduce", &[v]))?;
            if r != b.zero() {
                return Ok(None);
            }
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
            // no limit by the engine: not decided (the growth of |f| at points near p proves no order)
            return Ok(None);
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
            // the limit contradicts the values (a wrong limit): not decided
            return Ok(None);
        }
        return Ok(Some(m));
    }
    Ok(None)
}

