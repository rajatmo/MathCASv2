//! The inverse Mellin transform of gamma ratios as a Meijer G-function (round 22).
//!
//! `F(s) = C w^s prod gamma(alpha_j s + beta_j)^(e_j)` with rational alpha_j (and rational factors `(a s + c)^-1`,
//! written `gamma(s + c/a)/gamma(s + c/a + 1)/a` with the pole on the side of the strip it lies on) is brought to
//! one unit u = delta s (delta the largest rational with every alpha_j/delta an integer) by Gauss's
//! multiplication formula (DLMF 5.5.6)
//! `gamma(n v) = (2 pi)^((1 - n)/2) n^(n v - 1/2) prod_{k<n} gamma(v + k/n)`; then
//! `1/(2 pi i) integral F(s) x^-s ds = (1/delta) G^{m,n}_{p,q}(x^(1/delta)/W | a; b)` by the definition of G
//! (DLMF 16.17.1 with s -> -s), where `gamma(u + b)` gives a b among the first m, `gamma(1 - a - u)` an a among
//! the first n, and the reciprocals the other parameters (Luke 1969, 5.2; Paris and Kaminski 2001, 3.4). The
//! contour must separate the poles of the two families: the strip's lower edge (given, or the rightmost pole of
//! the left family) must lie left of every right pole. The G-function is then reduced by `meijer_g`; a piecewise
//! value on both sides of |x| = 1 becomes a sum with `heaviside_theta`. Double poles (`gamma(s)^2`) need no
//! special treatment: they are the confluent cases of the G-function, reduced to Bessel K and the like.
use super::{call_of, factors, linear};
use mc_limit::bounds::sign;
use mc_calculus::common::{args_of, is_free};
use mc_calculus::term::Build;
use mc_api::{Store, View, T};
use mc_eval::{NativeCx, Res};

fn gcd(a: i64, b: i64) -> i64 {
    if b == 0 {
        a.abs()
    } else {
        gcd(b, a % b)
    }
}

/// An exact rational as (num, den).
fn ratio<'h>(b: &Build<'h>, t: T<'h>) -> Option<(i64, i64)> {
    let h = b.heap();
    match h.view(t) {
        View::Int(v) => Some((v, 1)),
        View::Rational { num, den } => match (h.view(num), h.view(den)) {
            (View::Int(n), View::Int(d)) => Some((n, d)),
            _ => None,
        },
        _ => None,
    }
}

/// One gamma factor `gamma(alpha s + beta)^e`.
struct Fac<'h> {
    alpha: (i64, i64),
    beta: T<'h>,
    e: i64,
}

/// The inverse Mellin transform of `f` in `s` at `x` as a G-function (module comment), or `None`. `lo` is the
/// lower edge of the strip when the caller knows it.
pub(crate) fn inverse<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, s: T<'h>, x: T<'h>, lo: Option<T<'h>>) -> Res<Option<T<'h>>> {
    let h = b.heap();
    let mut consts = vec![];
    let mut facs: Vec<Fac<'h>> = vec![];
    let mut rational: Vec<(T<'h>, T<'h>, i64)> = vec![];
    let mut common: Option<T<'h>> = None;
    let mut plain = 0;
    for g in factors(b, f) {
        if is_free(b, g, s) {
            consts.push(g);
            continue;
        }
        let (base, e) = match args_of(b, g, b.hd.pow) {
            Some(&[base, e]) => match h.view(e) {
                View::Int(n) => (base, n),
                _ => return Ok(None),
            },
            _ => (g, 1),
        };
        if let Some((n, [u])) = call_of(b, base) {
            if n == "gamma" {
                let Some((k, d)) = linear(cx, b, *u, s)? else { return Ok(None) };
                // a symbolic coefficient k = r K (r rational, K one common term, taken positive, as K in
                // gamma(s/K)^2): the unit is then K times the rational unit
                let alpha = match ratio(b, k) {
                    Some(a) => {
                        plain += 1;
                        a
                    }
                    None => {
                        let (r, kk) = split_ratio(cx, b, k)?;
                        match common {
                            None => common = Some(kk),
                            Some(c) if c == kk => {}
                            _ => return Ok(None),
                        }
                        r
                    }
                };
                facs.push(Fac { alpha, beta: d, e });
                continue;
            }
            return Ok(None);
        }
        // a linear factor (a s + c)^e
        let Some((k, d)) = linear(cx, b, base, s)? else { return Ok(None) };
        if ratio(b, k).is_none() {
            return Ok(None);
        }
        rational.push((k, d, e));
    }
    if facs.is_empty() {
        return Ok(None);
    }
    // linear factors as gamma ratios: (k s + d)^-1 = gamma(s + d/k)/gamma(s + d/k + 1)/k (pole left of the strip)
    // or = -gamma(-s - d/k)/gamma(1 - s - d/k)/k (pole right of it); (k s + d) the reciprocal
    let had_rational = !rational.is_empty();
    for (k, d, e) in rational {
        let p = cx.eval(b.neg(b.div(d, k)))?;
        let left = match lo {
            None => true,
            Some(l) => sign(cx, b, b.sub(l, p))? == Some(1),
        };
        let (kn, kd) = ratio(b, k).unwrap_or((1, 1));
        let c = cx.eval(b.div(d, k))?;
        for _ in 0..e.unsigned_abs() {
            let inv = e < 0;
            consts.push(b.powi(k, if inv { -1 } else { 1 }));
            let sg = if inv { 1 } else { -1 };
            if left {
                facs.push(Fac { alpha: (1, 1), beta: c, e: sg });
                facs.push(Fac { alpha: (1, 1), beta: b.add(&[c, b.one()]), e: -sg });
            } else {
                consts.push(b.int(-1));
                facs.push(Fac { alpha: (-1, 1), beta: b.neg(c), e: sg });
                facs.push(Fac { alpha: (-1, 1), beta: b.sub(b.one(), c), e: -sg });
            }
            let _ = (kn, kd);
        }
    }
    // Legendre's duplication in reverse: gamma(c s + b) gamma(c s + b + 1/2) = 2^(1 - 2 b) sqrt(pi) 2^(-2 c s)
    // gamma(2 c s + 2 b), so that parameters in steps of 1/2 do not double the order of the G-function
    let mut xscale = vec![];
    'merge: loop {
        for i in 0..facs.len() {
            for j in 0..facs.len() {
                if i == j || facs[i].alpha != facs[j].alpha || facs[i].e != facs[j].e || facs[i].e.abs() != 1 {
                    continue;
                }
                let d = cx.eval(b.sub(facs[j].beta, facs[i].beta))?;
                if ratio(b, d) != Some((1, 2)) {
                    continue;
                }
                let (al, be, e) = (facs[i].alpha, facs[i].beta, facs[i].e);
                if al.0.abs() > 6 {
                    continue;
                }
                let sq = b.pow(b.sym(h.sym("pi")), b.rat(1, 2));
                consts.push(b.powi(b.mul(&[b.pow(b.int(2), b.sub(b.one(), b.mul(&[b.int(2), be]))), sq]), e));
                // 2^(-2 alpha e s): x^-s 2^(-2 alpha e s) = (x 2^(2 alpha e))^-s
                xscale.push(b.pow(b.int(2), b.mul(&[b.int(-2 * e), b.rat(al.0, al.1)])));
                let beta2 = cx.eval(b.mul(&[b.int(2), be]))?;
                let (hi, lo2) = (i.max(j), i.min(j));
                facs.remove(hi);
                facs.remove(lo2);
                facs.push(Fac { alpha: (2 * al.0, al.1), beta: beta2, e });
                continue 'merge;
            }
        }
        break;
    }
    let x0 = x;
    let x = if xscale.is_empty() { x } else { cx.eval(b.div(x, b.mul(&xscale)))? };
    // delta: gcd of the numerators over the lcm of the denominators
    let mut gn = 0;
    let mut l = 1;
    for fc in &facs {
        gn = gcd(gn, fc.alpha.0);
        l = l / gcd(l, fc.alpha.1) * fc.alpha.1;
        if l > 1000 {
            return Ok(None);
        }
    }
    if gn == 0 {
        return Ok(None);
    }
    // a symbolic unit for every gamma factor or none
    if common.is_some() && (plain > 0 || had_rational) {
        return Ok(None);
    }
    let delta = match common {
        None => b.rat(gn, l),
        Some(kk) => {
            if sign(cx, b, kk)? == Some(-1) {
                return Ok(None);
            }
            cx.eval(b.mul(&[kk, b.rat(gn, l)]))?
        }
    };
    let u = h.sym_term(h.sym("transform·mu"));
    // in u: gamma(alpha s + beta) = gamma(n u + beta), n = alpha/delta, by the multiplication formula
    let (mut an, mut ap, mut bm, mut bq) = (vec![], vec![], vec![], vec![]);
    let mut w_pow = vec![];
    let two_pi = b.mul(&[b.int(2), b.sym(h.sym("pi"))]);
    for fc in &facs {
        let n = fc.alpha.0 * l / (gn * fc.alpha.1);
        let na = n.abs();
        if na > 12 {
            return Ok(None);
        }
        // gamma(n u + beta) with n = sg na: v = sg u + beta/na
        let sg = n.signum();
        // the constant (2 pi)^((1 - na)/2) na^(beta - 1/2), and na^(na v) = na^(n u) as a power of the unit
        let cst = b.mul(&[b.pow(two_pi, b.rat(1 - na, 2)), b.pow(b.int(na), b.sub(fc.beta, b.rat(1, 2)))]);
        consts.push(b.powi(cst, fc.e));
        w_pow.push(b.pow(b.int(na), b.int(n * fc.e)));
        for j in 0..na {
            let c = cx.eval(b.div(b.add(&[fc.beta, b.int(j)]), b.int(na)))?;
            for _ in 0..fc.e.unsigned_abs() {
                match (sg > 0, fc.e > 0) {
                    // gamma(u + c): b_(<=m) = c; 1/gamma(u + c): a_(>n) = c
                    (true, true) => bm.push(c),
                    (true, false) => ap.push(c),
                    // gamma(-u + c) = gamma(1 - a - u), a = 1 - c; 1/gamma(-u + c) = 1/gamma(1 - b - u), b = 1 - c
                    (false, true) => an.push(cx.eval(b.sub(b.one(), c))?),
                    (false, false) => bq.push(cx.eval(b.sub(b.one(), c))?),
                }
            }
        }
    }
    let _ = u;
    // the contour: every left pole -b - k left of every right pole 1 - a + k, and of the strip's edge
    let edge = match lo {
        Some(l) => Some(cx.eval(b.mul(&[l, delta]))?),
        None => None,
    };
    for &bb in &bm {
        for &aa in &an {
            // need -b < 1 - a
            if sign(cx, b, b.sub(b.sub(b.one(), aa), b.neg(bb)))? != Some(1) {
                return Ok(None);
            }
        }
        if let Some(e) = edge {
            if sign(cx, b, b.sub(e, b.neg(bb)))? == Some(-1) {
                return Ok(None);
            }
        }
    }
    if let Some(e) = edge {
        for &aa in &an {
            if sign(cx, b, b.sub(b.sub(b.one(), aa), e))? != Some(1) {
                return Ok(None);
            }
        }
    }
    // W^u from the multiplication formula: x^-s W^u = (x^(1/delta)/W)^-u
    let wfac = cx.eval(b.mul(&w_pow))?;
    let y = cx.eval(b.div(b.pow(x, b.powi(delta, -1)), wfac))?;
    let list = |xs: &[T<'h>]| h.call(b.hd.list, xs);
    let g = b.call("meijer_g", &[list(&[list(&an), list(&ap)]), list(&[list(&bm), list(&bq)]), y]);
    let g = cx.eval(g)?;
    let g = heaviside_form(cx, b, g, y)?;
    let c = cx.eval(b.mul(&consts))?;
    let v = cx.eval(b.mul(&[c, b.powi(delta, -1), g]))?;
    // erfc(u) written 1 - erf(u), the form of the tables (Oberhettinger 1974)
    let v = erfc_as_erf(cx, b, v)?;
    // x > 0: (x^2)^(1/2) is x
    let pos = h.call(h.sym("greater"), &[x0, b.zero()]);
    let r = mc_calculus::common::apply(cx, b, "refine", &[v, pos])?;
    Ok(Some(if mc_calculus::common::is_failure(b, r) { v } else { r }))
}

/// `piecewise([[v1, abs(y) < 1], [v2, abs(y) > 1]])` as `v1 heaviside_theta(1 - y) + v2 heaviside_theta(y - 1)`
/// (y > 0 here).
fn heaviside_form<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, g: T<'h>, y: T<'h>) -> Res<T<'h>> {
    let Some((n, args)) = call_of(b, g) else { return Ok(g) };
    if n != "piecewise" {
        return Ok(g);
    }
    let Some(cases) = args_of(b, args[0], b.hd.list) else { return Ok(g) };
    let mut out = vec![];
    for &c in cases {
        let Some(&[v, cond]) = args_of(b, c, b.hd.list) else { return Ok(g) };
        let Some((rel, _)) = call_of(b, cond) else { return Ok(g) };
        let step = match rel.as_str() {
            "less" => b.call("heaviside_theta", &[b.sub(b.one(), y)]),
            "greater" => b.call("heaviside_theta", &[b.sub(y, b.one())]),
            _ => return Ok(g),
        };
        out.push(b.mul(&[v, step]));
    }
    cx.eval(b.add(&out))
}

/// k = r K with r rational and K the rest (r = 1 when k has no rational factor).
fn split_ratio<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, k: T<'h>) -> Res<((i64, i64), T<'h>)> {
    let fs = factors(b, k);
    let mut r = (1, 1);
    let mut rest = vec![];
    for f in fs {
        match ratio(b, f) {
            Some(q) => r = (r.0 * q.0, r.1 * q.1),
            None => rest.push(f),
        }
    }
    Ok((r, cx.eval(b.mul(&rest))?))
}

/// `erfc(u)` replaced by `1 - erf(u)`.
fn erfc_as_erf<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, v: T<'h>) -> Res<T<'h>> {
    let h = b.heap();
    if is_free(b, v, h.sym_term(h.sym("erfc"))) {
        return Ok(v);
    }
    fn walk<'h>(b: &Build<'h>, t: T<'h>) -> T<'h> {
        let h = b.heap();
        let View::App { head, args } = h.view(t) else { return t };
        let new: Vec<T<'h>> = args.iter().map(|&a| walk(b, a)).collect();
        if b.sym_of(head) == Some(h.sym("erfc")) && new.len() == 1 {
            return b.sub(b.one(), b.call("erf", &[new[0]]));
        }
        h.app(head, &new)
    }
    let w = walk(b, v);
    cx.eval(w)
}
