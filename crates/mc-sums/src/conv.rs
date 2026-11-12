//! `sum_convergence(f, n)`: does `sum f(n)` converge? `true`, `false`, a condition on the parameters, or the call
//! unevaluated when no test decides.
//!
//! The tests, in order: a pole of the summand at an integer of the range (`false`); the term test (`limit f != 0`
//! means `false`); for a hypergeometric term the ratio `f(n + 1) / f(n) = z (1 - s/n + O(1/n^2))`: `|z| < 1`
//! converges, `|z| > 1` diverges, and on `|z| = 1` Raabe's test (`s > 1`, or `s > 0` when `z != 1`, Dirichlet's
//! test); a symbolic `z` gives the condition `abs(z) < 1`, solved for one parameter in the simple cases
//! (`abs(x) < 2` for `(x/2)^n`); `c n^e` with a symbolic exponent gives `re(-e) > 1`; otherwise the comparison
//! with `n^p log(n)^q log(log(n))^r log(log(log(n)))^s` through the limits `p = lim log|f| / log n` and so on
//! (Cauchy's condensation test, Bertrand's series), and the ratio test with `lim |f(n + 1) / f(n)|`.
use crate::cx::Sx;
use crate::{hyper, prod, some};
use mc_api::{names, Store, View, T};
use mc_eval::Res;
use mc_poly::work::Meter;

/// The verdict as a term (`true`, `false`, a condition), or `None`.
pub fn convergence<'h>(sx: &mut Sx<'_, '_, '_, 'h>, f: T<'h>, n: T<'h>, lo: Option<i64>, m: &mut Meter) -> Res<Option<T<'h>>> {
    let tru = sx.sym(names::TRUE);
    let fal = sx.sym(names::FALSE);
    if sx.free(f, n) {
        return Ok(Some(if f == sx.int(0) { tru } else { fal }));
    }
    // a pole in the range
    if let Some(l) = lo {
        if hyper::rational_in(sx, f, n)? {
            let den = sx.ev("denominator", &[f])?;
            if let Some((_, rs)) = prod::roots(sx, den, n, m)? {
                for (r, _) in rs {
                    if sx.as_int(r).is_some_and(|ri| ri >= l) {
                        return Ok(Some(fal));
                    }
                }
            }
        }
    }
    let to = sx.rule(n, sx.sym("inf"));
    // a hypergeometric term: the ratio
    if let Some(r) = hyper::ratio(sx, f, n)? {
        if let Some(v) = ratio_test(sx, r, n, m)? {
            return Ok(Some(v));
        }
    }
    // the term test
    let lim = sx.ev("limit", &[f, to])?;
    if is_number(sx, lim) && lim != sx.int(0) {
        return Ok(Some(fal));
    }
    // c n^e with a symbolic exponent
    if let Some(&[b, e]) = sx.args_of(f, names::POW) {
        if b == n && sx.free(e, n) {
            let ne = sx.mul(&[sx.int(-1), e])?;
            let re = sx.ev("re", &[ne])?;
            return Ok(Some(sx.ev("greater", &[re, sx.int(1)])?));
        }
    }
    // an alternating sign times a term tending to zero
    if let View::App { head, args } = sx.h().view(f) {
        if head == sx.sym(names::MUL) {
            let sign = args.iter().position(|&a| sx.args_of(a, names::POW).is_some_and(|x| x[0] == sx.int(-1) && x[1] == n));
            if let Some(i) = sign {
                let rest: Vec<T<'h>> = args.iter().enumerate().filter(|(j, _)| *j != i).map(|(_, &a)| a).collect();
                let g = sx.mul(&rest)?;
                let lg = sx.ev("limit", &[g, to])?;
                if lg == sx.int(0) {
                    return Ok(Some(tru));
                }
            }
        }
    }
    // comparison with n^p log(n)^q log(log(n))^r
    let af = sx.ev("abs", &[f])?;
    let mut g = sx.ev("log", &[af])?;
    let mut scale = n;
    for k in 0..4 {
        let ls = sx.ev("log", &[scale])?;
        let q = sx.div(g, ls)?;
        let p = scaled_limit(sx, q, n, k)?;
        if !is_number(sx, p) {
            break;
        }
        let lt = sx.ev("less", &[p, sx.int(-1)])?;
        if sx.is_true(lt) {
            return Ok(Some(tru));
        }
        let gt = sx.ev("greater", &[p, sx.int(-1)])?;
        if sx.is_true(gt) {
            return Ok(Some(fal));
        }
        // p == -1: divide by the scale and go one logarithm deeper
        let lsc = sx.ev("log", &[scale])?;
        g = sx.add(&[g, lsc])?;
        scale = lsc;
    }
    // the ratio test by limits
    let n1 = sx.plus(n, 1)?;
    let f1 = sx.subst(f, n, n1)?;
    let q = sx.div(f1, f)?;
    let q = sx.ev("abs", &[q])?;
    let l = sx.ev("limit", &[q, to])?;
    if is_number(sx, l) {
        let lt = sx.ev("less", &[l, sx.int(1)])?;
        if sx.is_true(lt) {
            return Ok(Some(tru));
        }
        let gt = sx.ev("greater", &[l, sx.int(1)])?;
        if sx.is_true(gt) {
            return Ok(Some(fal));
        }
    }
    Ok(None)
}

/// `lim q` as `n -> inf`, where `q` has the iterated logarithm `log^(k + 1)(n)` in its denominator: the limit
/// itself, else after the change of variable `n = exp^(k + 1)(v)` (`power_expand` writes `log(exp(v))` as `v`), in
/// which the iterated logarithms become powers of `v` and the limit as `v -> inf` is one of the scale of logarithms
/// (`log(log(log(n)))/log(log(n))` is `log(v)/v` with `n = exp(exp(v))`).
fn scaled_limit<'h>(sx: &mut Sx<'_, '_, '_, 'h>, q: T<'h>, n: T<'h>, k: usize) -> Res<T<'h>> {
    let to = sx.rule(n, sx.sym("inf"));
    let l = sx.ev("limit", &[q, to])?;
    if is_number(sx, l) {
        return Ok(l);
    }
    let v = sx.sym("sum·v");
    let mut e = v;
    for _ in 0..=k {
        e = sx.ev("exp", &[e])?;
    }
    let qs = sx.subst(q, n, e)?;
    let qs = sx.ev("power_expand", &[qs])?;
    let tv = sx.rule(v, sx.sym("inf"));
    sx.ev("limit", &[qs, tv])
}

/// Is `t` a number (real, possibly infinite)?
fn is_number<'h>(sx: &mut Sx<'_, '_, '_, 'h>, t: T<'h>) -> bool {
    if sx.contains_any(t, &["limit", "indeterminate", names::FAILURE, "interval"]) {
        return false;
    }
    crate::verify::symbols(sx, t, &[]).is_empty()
}

/// The ratio test with Raabe's refinement for a rational ratio `r`.
fn ratio_test<'h>(sx: &mut Sx<'_, '_, '_, 'h>, r: T<'h>, n: T<'h>, m: &mut Meter) -> Res<Option<T<'h>>> {
    let tru = sx.sym(names::TRUE);
    let fal = sx.sym(names::FALSE);
    let ring = crate::w!(sx, sx.ring(&[r], &[n], m));
    if !sx.clean(&ring, &[n]) {
        return Ok(None);
    }
    let v = some!(ring.var(n));
    let rf = some!(crate::w!(sx, sx.read(r, &ring, m)));
    let (dn, dd) = (some!(crate::rf::deg(&rf.num, v)), some!(crate::rf::deg(&rf.den, v)));
    if dn < dd {
        return Ok(Some(tru));
    }
    if dn > dd {
        return Ok(Some(fal));
    }
    let c = |p: &mc_poly::Poly, e: u32| crate::rf::coeff(p, v, e);
    let ln = crate::w!(sx, sx.pterm(&c(&rf.num, dn), &ring, m));
    let ld = crate::w!(sx, sx.pterm(&c(&rf.den, dd), &ring, m));
    let z = sx.div(ln, ld)?;
    let az = sx.ev("abs", &[z])?;
    if !crate::verify::symbols(sx, z, &[]).is_empty() {
        return Ok(Some(condition(sx, z)?));
    }
    let lt = sx.ev("less", &[az, sx.int(1)])?;
    if sx.is_true(lt) {
        return Ok(Some(tru));
    }
    let gt = sx.ev("greater", &[az, sx.int(1)])?;
    if sx.is_true(gt) {
        return Ok(Some(fal));
    }
    if dn == 0 {
        // |z| = 1, constant ratio: the terms do not tend to 0
        return Ok(Some(fal));
    }
    // r / z = 1 - s / n + ...: s = den_{d-1}/lc_den - num_{d-1}/lc_num
    let n1 = crate::w!(sx, sx.pterm(&c(&rf.num, dn - 1), &ring, m));
    let d1 = crate::w!(sx, sx.pterm(&c(&rf.den, dd - 1), &ring, m));
    let a = sx.div(d1, ld)?;
    let b = sx.div(n1, ln)?;
    let s = sx.sub(a, b)?;
    let bound = if z == sx.int(1) { 1 } else { 0 };
    let gt = sx.ev("greater", &[s, sx.int(bound)])?;
    if sx.is_true(gt) {
        return Ok(Some(tru));
    }
    let le = sx.ev("less_equal", &[s, sx.int(bound)])?;
    if sx.is_true(le) {
        return Ok(Some(fal));
    }
    Ok(None)
}

/// `abs(z) < 1`, solved for the parameter when `z = c x^p`.
fn condition<'h>(sx: &mut Sx<'_, '_, '_, 'h>, z: T<'h>) -> Res<T<'h>> {
    let syms = crate::verify::symbols(sx, z, &[]);
    if let [x] = syms.as_slice() {
        // z = c x^p: |x| < |c|^(-1/p) (p > 0) or |x| > |c|^(-1/p) (p < 0)
        let lz = sx.ev("log", &[z])?;
        let lx = sx.ev("log", &[*x])?;
        let d = sx.ev("d", &[lz, *x])?;
        let p = sx.mul(&[d, *x])?;
        let p = sx.ev("simplify", &[p])?;
        if let Some(pi) = sx.as_int(p) {
            if pi != 0 {
                let xp = sx.pow(*x, p)?;
                let c = sx.div(z, xp)?;
                let c = sx.ev("simplify", &[c])?;
                if crate::verify::symbols(sx, c, &[]).is_empty() {
                    let ac = sx.ev("abs", &[c])?;
                    let e = sx.div(sx.int(-1), p)?;
                    let bnd = sx.pow(ac, e)?;
                    let ax = sx.ev("abs", &[*x])?;
                    let _ = lx;
                    return sx.ev(if pi > 0 { "less" } else { "greater" }, &[ax, bnd]);
                }
            }
        }
    }
    let az = sx.ev("abs", &[z])?;
    sx.ev("less", &[az, sx.int(1)])
}
