//! `sum` in closed form: the methods this crate adds to the list library's `sum`.
//!
//! - **indefinite** `sum(f, k)` of a hypergeometric `f` that is not a polynomial: Gosper's antidifference;
//! - **finite** `sum(f, [k, lo, hi])` with symbolic bounds: Gosper (`z(hi + 1) - z(lo)`, the upper end written as
//!   `(1 + R(hi)) t(hi)` so that no term outside the range is evaluated), rational functions by their partial
//!   fractions (harmonic numbers, `digamma` and Hurwitz `zeta`), then Zeilberger's algorithm with the recurrence
//!   solved (order one by a product, higher orders by Hyper) and fitted to the first values;
//! - **infinite** sums: rational functions (Hurwitz `zeta` and `digamma`, the sum of the `digamma` coefficients
//!   zero for convergence; bilateral sums by `pi cot(pi z)`), Gosper with the limit of the antidifference, and the
//!   hypergeometric series `t(lo) pFq(a; b; z)` read off the term ratio, with Gauss's theorem for `2F1` at 1 and
//!   the elementary cases of `0F0`, `1F0`, `1F1` and `2F1`;
//! - **numeric** sums when the summand or the bounds are inexact (what `n` makes of a sum): see `numsum`.
//!
//! Every closed form of a finite sum with symbolic bounds or parameters is checked against the terms added one by
//! one at several points (`verify::check_sum`) before it is returned.
use crate::cx::{Budget, Sx};
use crate::rf::Rf;
use crate::{gosper, hyper, prod, recur, some, verify, w, zeil};
use mc_api::{names, Store, View, T};
use mc_eval::Res;
use mc_poly::convert::Ring;
use mc_poly::work::Meter;

/// The seconds one closed-form attempt may take.
pub const SECS: u64 = 20;

/// The ratio of `t` in `k` as a rational function in a ring with `vars` first (`None` when not hypergeometric).
pub fn ratio_rf<'h>(sx: &mut Sx<'_, '_, '_, 'h>, t: T<'h>, vars: &[T<'h>], k: T<'h>, extra: &[T<'h>], m: &mut Meter) -> Res<Option<(Ring<'h>, Rf)>> {
    let r = some!(hyper::ratio(sx, t, k)?);
    let mut terms = vec![r];
    terms.extend_from_slice(extra);
    let ring = w!(sx, sx.ring(&terms, vars, m));
    if !sx.clean(&ring, vars) {
        return Ok(None);
    }
    let rf = some!(w!(sx, sx.read(r, &ring, m)));
    if rf.is_zero() {
        return Ok(None);
    }
    Ok(Some((ring, rf)))
}

/// Is `f` a polynomial in `k`?
pub fn is_polynomial<'h>(sx: &mut Sx<'_, '_, '_, 'h>, f: T<'h>, k: T<'h>) -> Res<bool> {
    let stop = sx.stop.clone();
    let mut b = Budget::new(&stop, 5);
    let mut m = Meter::new(&mut b);
    let Ok(ring) = sx.ring(&[f], &[k], &mut m) else { return Ok(false) };
    if !sx.clean(&ring, &[k]) {
        return Ok(false);
    }
    let Some(v) = ring.var(k) else { return Ok(true) };
    Ok(match sx.read(f, &ring, &mut m) {
        Ok(Some(r)) => !r.den.degree(v).is_some_and(|d| d > 0),
        _ => false,
    })
}

/// `t` with powers `(p/q)^e` of rationals to symbolic exponents written `p^e q^-e` (so that they combine with
/// other powers of `p` and `q`).
pub fn split_powers<'h>(sx: &mut Sx<'_, '_, '_, 'h>, t: T<'h>) -> Res<T<'h>> {
    let h = sx.h();
    match h.view(t) {
        View::App { head, args } => {
            if head == sx.sym(names::POW) && args.len() == 2 {
                if let View::Rational { num, den } = h.view(args[0]) {
                    if sx.as_int(args[1]).is_none() {
                        let e = split_powers(sx, args[1])?;
                        let a = sx.pow(num, e)?;
                        let ne = sx.mul(&[sx.int(-1), e])?;
                        let b = sx.pow(den, ne)?;
                        return sx.mul(&[a, b]);
                    }
                }
            }
            let mut new = Vec::with_capacity(args.len());
            for &a in args {
                new.push(split_powers(sx, a)?);
            }
            if new.as_slice() == args {
                return Ok(t);
            }
            sx.eval(h.app(head, &new))
        }
        _ => Ok(t),
    }
}

/// `t` with products `c b^e` (`c` a rational, `b` an integer, `e` symbolic) written `(c / b^j) b^(e + j)` for the
/// largest power `b^j` dividing `c` (or its denominator), so `16 2^(n - 1)` becomes `2^(n + 3)`.
pub fn merge_powers<'h>(sx: &mut Sx<'_, '_, '_, 'h>, t: T<'h>) -> Res<T<'h>> {
    let h = sx.h();
    let View::App { head, args } = h.view(t) else { return Ok(t) };
    let mut new = Vec::with_capacity(args.len());
    for &a in args {
        new.push(merge_powers(sx, a)?);
    }
    if head == sx.sym(names::MUL) {
        // the numeric coefficient and the integer-base powers
        let c = new[0];
        let (cn, cd) = match h.view(c) {
            View::Int(v) => (v, 1),
            View::Rational { num, den } => match (h.view(num), h.view(den)) {
                (View::Int(a), View::Int(b)) => (a, b),
                _ => (0, 0),
            },
            _ => (0, 0),
        };
        if cn != 0 {
            for i in 1..new.len() {
                let View::App { head: ph, args: &[b, e] } = h.view(new[i]) else { continue };
                if ph != sx.sym(names::POW) || sx.as_int(e).is_some() {
                    continue;
                }
                let Some(bv) = sx.as_int(b) else { continue };
                if bv < 2 {
                    continue;
                }
                let (mut n, mut d, mut j) = (cn, cd, 0i64);
                while n % bv == 0 && n != 0 {
                    n /= bv;
                    j += 1;
                }
                while j == 0 && d % bv == 0 && d > 1 {
                    d /= bv;
                    j -= 1;
                }
                if j == 0 {
                    continue;
                }
                let ej = sx.plus(e, j)?;
                new[i] = sx.pow(b, ej)?;
                new[0] = sx.div(sx.int(n), sx.int(d))?;
                break;
            }
        }
    }
    if new.as_slice() == args {
        return Ok(t);
    }
    sx.eval(h.app(head, &new))
}

/// The smallest of a few equal forms of `t`.
pub fn tidy<'h>(sx: &mut Sx<'_, '_, '_, 'h>, t: T<'h>) -> Res<T<'h>> {
    let mut cands = vec![t];
    let s = split_powers(sx, t)?;
    let mut bases = vec![t];
    if s != t {
        cands.push(s);
        bases.push(s);
        let e = sx.ev("expand", &[s])?;
        if !sx.bad(e) {
            cands.push(e);
            bases.push(e);
        }
    }
    let mp = merge_powers(sx, *bases.last().expect("one base"))?;
    if !bases.contains(&mp) {
        cands.push(mp);
        bases.push(mp);
    }
    for b in bases {
        for f in ["together", "factor", "simplify"] {
            let c = sx.ev(f, &[b])?;
            if !sx.bad(c) {
                cands.push(c);
            }
        }
    }
    let best = sx.smallest(&cands);
    // a rational function of symbols: factored, as a closed form is usually written
    let stop = sx.stop.clone();
    let mut b = Budget::new(&stop, 5);
    let mut m = Meter::new(&mut b);
    if let Ok(ring) = sx.ring(&[best], &[], &mut m) {
        let h = sx.h();
        if ring.nvars() > 0 && ring.kernels.iter().all(|&kk| matches!(h.view(kk), View::Sym(_))) {
            let f = sx.ev("factor", &[best])?;
            if !sx.bad(f) {
                return sign_normal(sx, f);
            }
        }
    }
    merge_powers(sx, best)
}

/// `-p^e` with `e` a negative odd integer written `(-p)^e`, as `(1 - a)^-1` for `-1/(-1 + a)`.
pub fn sign_normal<'h>(sx: &mut Sx<'_, '_, '_, 'h>, t: T<'h>) -> Res<T<'h>> {
    if let Some(&[c, pw]) = sx.args_of(t, names::MUL) {
        if c == sx.int(-1) {
            if let Some(&[p, e]) = sx.args_of(pw, names::POW) {
                if sx.as_int(e).is_some_and(|e| e < 0 && e % 2 != 0) {
                    let np = sx.mul(&[sx.int(-1), p])?;
                    let np = sx.ev("expand", &[np])?;
                    return sx.pow(np, e);
                }
            }
        }
    }
    Ok(t)
}

/// The antidifference of a telescoping term: `g(k)` for `f = g(k + 1) - g(k)` (or `g(k + 1) / g(k)` for a
/// product), else `None`.
pub fn telescoping<'h>(sx: &mut Sx<'_, '_, '_, 'h>, f: T<'h>, k: T<'h>, product: bool) -> Res<Option<T<'h>>> {
    let k1 = sx.plus(k, 1)?;
    if product {
        let num = sx.ev("numerator", &[f])?;
        let den = sx.ev("denominator", &[f])?;
        if sx.free(den, k) {
            return Ok(None);
        }
        let d1 = sx.subst(den, k, k1)?;
        return Ok(if d1 == num { Some(den) } else { None });
    }
    let h = sx.h();
    let View::App { head, args: &[a, b] } = h.view(f) else { return Ok(None) };
    if head != sx.sym(names::ADD) {
        return Ok(None);
    }
    for (x, y) in [(a, b), (b, a)] {
        let g = sx.mul(&[sx.int(-1), y])?;
        if sx.free(g, k) {
            continue;
        }
        let g1 = sx.subst(g, k, k1)?;
        if g1 == x {
            return Ok(Some(g));
        }
    }
    Ok(None)
}

/// Gosper's antidifference `z(k) = R(k) t(k)` with its certificate checked: `(R as a term, ratio)`.
fn antidifference<'h>(sx: &mut Sx<'_, '_, '_, 'h>, t: T<'h>, k: T<'h>, extra: &[T<'h>], m: &mut Meter) -> Res<Option<T<'h>>> {
    let (ring, r) = some!(ratio_rf(sx, t, &[k], k, extra, m)?);
    let v = some!(ring.var(k));
    let rr = some!(w!(sx, gosper::gosper(&r, v, m)));
    Ok(Some(w!(sx, sx.term(&rr, &ring, m))))
}

/// The indefinite sum `sum(f, k)`: Gosper's antidifference (non-polynomial hypergeometric `f`).
pub fn indefinite<'h>(sx: &mut Sx<'_, '_, '_, 'h>, f: T<'h>, k: T<'h>, m: &mut Meter) -> Res<Option<T<'h>>> {
    if sx.free(f, k) || is_polynomial(sx, f, k)? {
        return Ok(None);
    }
    if let Some(g) = telescoping(sx, f, k, false)? {
        return Ok(Some(g));
    }
    if let Some(g) = periodic(sx, f, k)? {
        return Ok(Some(g));
    }
    if let Some(rr) = antidifference(sx, f, k, &[], m)? {
        let z = sx.mul(&[rr, f])?;
        return Ok(Some(tidy(sx, z)?));
    }
    // a rational function: 1/(k - r)^j is the difference of (-1)^(j-1) polygamma(j - 1, k - r) / (j - 1)!
    if hyper::rational_in(sx, f, k)? {
        let (pp, fr) = some!(partial_fractions(sx, f, k, m)?);
        let mut out = vec![];
        if pp != sx.int(0) {
            let s = sx.ev("sum", &[pp, k])?;
            if sx.contains_any(s, &["sum"]) {
                return Ok(None);
            }
            out.push(s);
        }
        for (c, r, j) in fr {
            let u = sx.sub(k, r)?;
            let piece = if j == 1 {
                sx.ev("digamma", &[u])?
            } else {
                let pg = sx.ev("polygamma", &[sx.int(j - 1), u])?;
                let fact = (1..j).product::<i64>();
                let sign = if (j - 1) % 2 == 0 { 1 } else { -1 };
                let c = sx.div(sx.int(sign), sx.int(fact))?;
                sx.mul(&[c, pg])?
            };
            out.push(sx.mul(&[c, piece])?);
        }
        let s = sx.add(&out)?;
        return Ok(Some(s));
    }
    Ok(None)
}

/// `sum(f, [k, lo, hi])` by the methods of this crate (`hi` may be `inf`, `lo` may be `-inf`).
pub fn definite<'h>(sx: &mut Sx<'_, '_, '_, 'h>, f: T<'h>, k: T<'h>, lo: T<'h>, hi: T<'h>, m: &mut Meter) -> Res<Option<T<'h>>> {
    let inf = sx.sym("inf");
    if verify::is_neg_inf(sx, lo) {
        let nk = sx.mul(&[sx.int(-1), k])?;
        let g = sx.subst(f, k, nk)?;
        if hi == inf {
            return bilateral(sx, f, g, k, m);
        }
        let nhi = sx.mul(&[sx.int(-1), hi])?;
        return definite(sx, g, k, nhi, inf, m);
    }
    if hi == inf {
        return infinite(sx, f, k, lo, m);
    }
    finite(sx, f, k, lo, hi, m)
}

/// The finite sum with symbolic bounds.
fn finite<'h>(sx: &mut Sx<'_, '_, '_, 'h>, f: T<'h>, k: T<'h>, lo: T<'h>, hi: T<'h>, m: &mut Meter) -> Res<Option<T<'h>>> {
    if let Some(g) = telescoping(sx, f, k, false)? {
        let hi1 = sx.plus(hi, 1)?;
        let a = sx.subst(g, k, hi1)?;
        let b = sx.subst(g, k, lo)?;
        return Ok(Some(sx.sub(a, b)?));
    }
    if let Some(s) = finite_term(sx, f, k, lo, hi, m)? {
        return Ok(Some(s));
    }
    // a sum of terms, each summed alone
    let h = sx.h();
    if let View::App { head, args } = h.view(f) {
        if head == sx.sym(names::ADD) && args.len() <= 12 {
            let ex = sx.ev("expand", &[f])?;
            let parts: Vec<T<'h>> = match h.view(ex) {
                View::App { head, args } if head == sx.sym(names::ADD) => args.to_vec(),
                _ => args.to_vec(),
            };
            // rational parts are summed together (their partial fractions may cancel)
            let mut rational = vec![];
            let mut other = vec![];
            for &p in &parts {
                if hyper::rational_in(sx, p, k)? {
                    rational.push(p);
                } else {
                    other.push(p);
                }
            }
            if other.is_empty() {
                return Ok(None);
            }
            let mut out = vec![];
            if !rational.is_empty() {
                let r = sx.add(&rational)?;
                let spec = sx.list(&[k, lo, hi]);
                let s = sx.ev("sum", &[r, spec])?;
                if sx.contains_any(s, &["sum"]) {
                    return Ok(None);
                }
                out.push(s);
            }
            for p in other {
                out.push(some!(finite_term(sx, p, k, lo, hi, m)?));
            }
            return Ok(Some(sx.add(&out)?));
        }
    }
    Ok(None)
}

/// One hypergeometric term summed over a finite symbolic range.
fn finite_term<'h>(sx: &mut Sx<'_, '_, '_, 'h>, f: T<'h>, k: T<'h>, lo: T<'h>, hi: T<'h>, m: &mut Meter) -> Res<Option<T<'h>>> {
    // Gosper
    if let Some(rr) = antidifference(sx, f, k, &[lo, hi], m)? {
        if let Some(s) = gosper_definite(sx, f, rr, k, lo, hi)? {
            if verify::check_sum(sx, f, k, lo, hi, s)? {
                return Ok(Some(s));
            }
        }
    }
    // rational functions by partial fractions
    if hyper::rational_in(sx, f, k)? {
        if let Some(s) = rational_finite(sx, f, k, lo, hi, m)? {
            if verify::check_sum(sx, f, k, lo, hi, s)? {
                return Ok(Some(s));
            }
        }
        return Ok(None);
    }
    // Zeilberger in a variable of the upper bound
    let syms = verify::symbols(sx, hi, &[k]);
    if let [n] = syms.as_slice() {
        if sx.free(lo, *n) {
            if let Some(s) = zeilberger_sum(sx, f, k, *n, lo, hi, m)? {
                return Ok(Some(s));
            }
        }
    }
    Ok(None)
}

/// `z(hi + 1) - z(lo)` with `z = R t`: the upper end as `(1 + R(hi)) t(hi)`; when `z(lo)` is singular, the first
/// terms are added separately (up to 4).
fn gosper_definite<'h>(sx: &mut Sx<'_, '_, '_, 'h>, f: T<'h>, rr: T<'h>, k: T<'h>, lo: T<'h>, hi: T<'h>) -> Res<Option<T<'h>>> {
    let r_hi = sx.subst(rr, k, hi)?;
    let one_r = sx.plus(r_hi, 1)?;
    let t_hi = sx.subst(f, k, hi)?;
    let upper = sx.mul(&[one_r, t_hi])?;
    if sx.bad(upper) {
        return Ok(None);
    }
    let mut head = vec![];
    for s in 0..4 {
        let a = sx.plus(lo, s)?;
        let r_a = sx.subst(rr, k, a)?;
        let t_a = sx.subst(f, k, a)?;
        let za = sx.mul(&[r_a, t_a])?;
        if !sx.bad(za) {
            let nza = sx.mul(&[sx.int(-1), za])?;
            let mut parts = vec![upper, nza];
            parts.extend(head.iter().copied());
            let s = sx.add(&parts)?;
            return Ok(Some(tidy(sx, s)?));
        }
        if sx.bad(t_a) {
            return Ok(None);
        }
        head.push(t_a);
    }
    Ok(None)
}

/// The partial fractions `[(c, r, j)]` of a proper rational `f = sum c / (k - r)^j` (roots from factors of degree
/// at most 2, the quadratic ones with simple poles only), and the polynomial part; `None` otherwise.
fn partial_fractions<'h>(sx: &mut Sx<'_, '_, '_, 'h>, f: T<'h>, k: T<'h>, m: &mut Meter) -> Res<Option<Fractions<'h>>> {
    let ap = sx.ev("apart", &[f, k])?;
    let h = sx.h();
    let parts: Vec<T<'h>> = match h.view(ap) {
        View::App { head, args } if head == sx.sym(names::ADD) => args.to_vec(),
        _ => vec![ap],
    };
    let mut poly = vec![];
    let mut out = vec![];
    for p in parts {
        let ring = w!(sx, sx.ring(&[p], &[k], m));
        if !sx.clean(&ring, &[k]) {
            return Ok(None);
        }
        let v = some!(ring.var(k));
        let rf = some!(w!(sx, sx.read(p, &ring, m)));
        if !rf.den.degree(v).is_some_and(|d| d > 0) {
            poly.push(p);
            continue;
        }
        // den = c q^j with q irreducible of degree 1 or 2
        let fac = w!(sx, mc_poly::factor::factor(&rf.den, false, m));
        let qs: Vec<_> = fac.factors.iter().filter(|q| q.0.degree(v).is_some_and(|d| d > 0)).collect();
        let [(q, j)] = qs.as_slice() else { return Ok(None) };
        let j = *j as i64;
        let (c, rs) = some!(prod::roots(sx, p, k, m)?);
        // p = c (k - r)^-j or c (k - a)^-1 (k - b)^-1 times a numerator: recompute each coefficient
        match q.degree(v) {
            Some(1) => {
                let [(r, _)] = rs.as_slice() else { return Ok(None) };
                // p (k - r)^j is free of k
                let kr = sx.sub(k, *r)?;
                let krj = sx.pow(kr, sx.int(j))?;
                let cc = sx.mul(&[p, krj])?;
                let cc = sx.ev("cancel", &[cc])?;
                if !sx.free(cc, k) {
                    return Ok(None);
                }
                out.push((cc, *r, j));
            }
            Some(2) if j == 1 => {
                // (A k + B) / (a (k - r1)(k - r2)): residues (A r + B) / (a (r - r'))
                let den_roots: Vec<T<'h>> = rs.iter().filter(|r| r.1 < 0).map(|r| r.0).collect();
                let [r1, r2] = den_roots.as_slice() else { return Ok(None) };
                let num_t = w!(sx, sx.pterm(&rf.num, &ring, m));
                let a = w!(sx, sx.pterm(&mc_poly::Poly::constant(rf.den.nvars(), fac.unit.clone()), &ring, m));
                let lc = w!(sx, sx.pterm(&crate::rf::lcoeff(q, v), &ring, m));
                let _ = c;
                for (x, y) in [(*r1, *r2), (*r2, *r1)] {
                    let nx = sx.subst(num_t, k, x)?;
                    let dxy = sx.sub(x, y)?;
                    let den = sx.mul(&[a, lc, dxy])?;
                    let cc = sx.div(nx, den)?;
                    let cc = sx.ev("simplify", &[cc])?;
                    out.push((cc, x, 1));
                }
            }
            _ => return Ok(None),
        }
    }
    let pp = sx.add(&poly)?;
    Ok(Some((pp, out)))
}

/// A polynomial part and the partial fractions `(c, r, j)`, `c / (k - r)^j`.
type Fractions<'h> = (T<'h>, Vec<(T<'h>, T<'h>, i64)>);

/// `sum_{k=lo}^{hi} f(k)` for a rational `f`: `1/(k - r)^j` sums to harmonic numbers when `lo - r` is a positive
/// integer, else to `digamma` (j = 1) and Hurwitz `zeta` (j > 1) differences; the polynomial part through `sum`.
fn rational_finite<'h>(sx: &mut Sx<'_, '_, '_, 'h>, f: T<'h>, k: T<'h>, lo: T<'h>, hi: T<'h>, m: &mut Meter) -> Res<Option<T<'h>>> {
    let (pp, fr) = some!(partial_fractions(sx, f, k, m)?);
    let mut out = vec![];
    if pp != sx.int(0) {
        let spec = sx.list(&[k, lo, hi]);
        let s = sx.ev("sum", &[pp, spec])?;
        if sx.contains_any(s, &["sum"]) {
            return Ok(None);
        }
        out.push(s);
    }
    for (c, r, j) in fr {
        let a = sx.sub(lo, r)?;
        let b = sx.sub(hi, r)?;
        let piece = match sx.as_int(a) {
            Some(ai) if ai >= 1 => {
                // H(hi - r, j) - H(lo - r - 1, j)
                let hb = if j == 1 { sx.ev("harmonic_number", &[b])? } else { sx.ev("harmonic_number", &[b, sx.int(j)])? };
                let am1 = sx.int(ai - 1);
                let ha = if j == 1 { sx.ev("harmonic_number", &[am1])? } else { sx.ev("harmonic_number", &[am1, sx.int(j)])? };
                sx.sub(hb, ha)?
            }
            _ => {
                let b1 = sx.plus(b, 1)?;
                if j == 1 {
                    let x = sx.ev("digamma", &[b1])?;
                    let y = sx.ev("digamma", &[a])?;
                    sx.sub(x, y)?
                } else {
                    let x = sx.ev("zeta", &[sx.int(j), a])?;
                    let y = sx.ev("zeta", &[sx.int(j), b1])?;
                    sx.sub(x, y)?
                }
            }
        };
        out.push(sx.mul(&[c, piece])?);
    }
    let s = sx.add(&out)?;
    Ok(Some(tidy(sx, s)?))
}

/// `sum_{k=lo}^inf f(k)`.
fn infinite<'h>(sx: &mut Sx<'_, '_, '_, 'h>, f: T<'h>, k: T<'h>, lo: T<'h>, m: &mut Meter) -> Res<Option<T<'h>>> {
    if hyper::rational_in(sx, f, k)? {
        return rational_infinite(sx, f, k, lo, m);
    }
    // Gosper: lim z - z(lo)
    if let Some(rr) = antidifference(sx, f, k, &[lo], m)? {
        let z = sx.mul(&[rr, f])?;
        let to = sx.rule(k, sx.sym("inf"));
        let lim = sx.ev("limit", &[z, to])?;
        if !sx.bad(lim) && !sx.contains_any(lim, &["limit"]) {
            let zl = sx.subst(z, k, lo)?;
            if !sx.bad(zl) {
                let s = sx.sub(lim, zl)?;
                return Ok(Some(tidy(sx, s)?));
            }
        }
    }
    if let Some(s) = poly_geometric(sx, f, k, lo, m)? {
        return Ok(Some(s));
    }
    if let Some(s) = series_sum(sx, f, k, lo, m)? {
        return Ok(Some(s));
    }
    // a sum of terms
    let h = sx.h();
    if let View::App { head, args } = h.view(f) {
        if head == sx.sym(names::ADD) && args.len() <= 8 {
            let mut out = vec![];
            let inf = sx.sym("inf");
            for &p in args {
                let spec = sx.list(&[k, lo, inf]);
                let s = sx.ev("sum", &[p, spec])?;
                if sx.contains_any(s, &["sum"]) || sx.bad(s) {
                    return Ok(None);
                }
                out.push(s);
            }
            return Ok(Some(sx.add(&out)?));
        }
    }
    Ok(None)
}

/// `sum_{k=lo}^inf f(k)` for a rational `f`: Hurwitz `zeta(j, lo - r)` for `j > 1`, `-digamma(lo - r)` for simple
/// poles whose coefficients add up to zero; an even `f` summed from 0 or 1 through the bilateral sum.
fn rational_infinite<'h>(sx: &mut Sx<'_, '_, '_, 'h>, f: T<'h>, k: T<'h>, lo: T<'h>, m: &mut Meter) -> Res<Option<T<'h>>> {
    let (pp, fr) = some!(partial_fractions(sx, f, k, m)?);
    if pp != sx.int(0) {
        return Ok(None);
    }
    // pole at an integer in the range: divergent
    for (_, r, _) in &fr {
        let a = sx.sub(lo, *r)?;
        if let Some(ai) = sx.as_int(a) {
            if ai <= 0 {
                return Ok(None);
            }
        }
    }
    let simple: Vec<T<'h>> = fr.iter().filter(|x| x.2 == 1).map(|x| x.0).collect();
    let cs = sx.add(&simple)?;
    let cs = sx.ev("simplify", &[cs])?;
    if cs != sx.int(0) {
        return Ok(None);
    }
    // an even function from 0 or 1 with complex poles: (bilateral - f(0)) / 2 (+ f(0))
    let complex_poles = fr.iter().any(|x| sx.contains_any(x.1, &[names::COMPLEX]));
    if complex_poles && (lo == sx.int(0) || lo == sx.int(1)) {
        let nk = sx.mul(&[sx.int(-1), k])?;
        let fm = sx.subst(f, k, nk)?;
        let d = sx.sub(f, fm)?;
        let d = sx.ev("together", &[d])?;
        if d == sx.int(0) {
            if let Some(b) = bilateral_rational(sx, f, k, m)? {
                let f0 = sx.subst(f, k, sx.int(0))?;
                let bf = sx.sub(b, f0)?;
                let half = sx.div(bf, sx.int(2))?;
                let s = if lo == sx.int(0) { sx.add(&[half, f0])? } else { half };
                return Ok(Some(tidy(sx, s)?));
            }
        }
    }
    let mut out = vec![];
    for (c, r, j) in fr {
        let a = sx.sub(lo, r)?;
        let piece = if j == 1 {
            let d = sx.ev("digamma", &[a])?;
            sx.mul(&[sx.int(-1), d])?
        } else {
            sx.ev("zeta", &[sx.int(j), a])?
        };
        out.push(sx.mul(&[c, piece])?);
    }
    let s = sx.add(&out)?;
    Ok(Some(tidy(sx, s)?))
}

/// `sin(pi u)` with `sin(i pi x) = i sinh(pi x)`.
fn cot_pi<'h>(sx: &mut Sx<'_, '_, '_, 'h>, u: T<'h>) -> Res<T<'h>> {
    let pi = sx.sym("pi");
    let im = sx.ev("sqrt", &[sx.int(-1)])?;
    let nim = sx.mul(&[sx.int(-1), im])?;
    let q = sx.mul(&[u, nim])?;
    let q = sx.ev("expand", &[q])?;
    if sx.contains_any(u, &[names::COMPLEX]) && !sx.contains_any(q, &[names::COMPLEX]) {
        // cot(i pi q) = -i coth(pi q)
        let pq = sx.mul(&[pi, q])?;
        let c = sx.ev("coth", &[pq])?;
        return sx.mul(&[sx.int(-1), im, c]);
    }
    let pu = sx.mul(&[pi, u])?;
    sx.ev("cot", &[pu])
}

/// `sum_{k=-inf}^inf f(k)` for a rational `f` with simple poles off the integers: `-pi sum_r c_r cot(pi r)`.
fn bilateral_rational<'h>(sx: &mut Sx<'_, '_, '_, 'h>, f: T<'h>, k: T<'h>, m: &mut Meter) -> Res<Option<T<'h>>> {
    let (pp, fr) = some!(partial_fractions(sx, f, k, m)?);
    if pp != sx.int(0) {
        return Ok(None);
    }
    let mut out = vec![];
    for (c, r, j) in fr {
        if sx.as_int(r).is_some() {
            return Ok(None);
        }
        // sum 1/(k - r)^j = 1/(j-1)! d^(j-1)/dr^(j-1) (-pi cot(pi r))
        let piece = if j == 1 {
            cot_pi(sx, r)?
        } else {
            let x = sx.sym("bilateral$r");
            let cx = cot_pi(sx, x)?;
            let spec = sx.list(&[x, sx.int(j - 1)]);
            let dj = sx.ev("d", &[cx, spec])?;
            let dj = sx.subst(dj, x, r)?;
            let fact = sx.ev("factorial", &[sx.int(j - 1)])?;
            sx.div(dj, fact)?
        };
        let pi = sx.sym("pi");
        out.push(sx.mul(&[sx.int(-1), pi, c, piece])?);
    }
    let s = sx.add(&out)?;
    Ok(Some(tidy(sx, s)?))
}

/// The bilateral sum: rational by `pi cot`, else `sum_{k>=0} f(k) + sum_{k>=1} f(-k)`.
fn bilateral<'h>(sx: &mut Sx<'_, '_, '_, 'h>, f: T<'h>, fneg: T<'h>, k: T<'h>, m: &mut Meter) -> Res<Option<T<'h>>> {
    if hyper::rational_in(sx, f, k)? {
        return bilateral_rational(sx, f, k, m);
    }
    let a = some!(infinite(sx, f, k, sx.int(0), m)?);
    let b = some!(infinite(sx, fneg, k, sx.int(1), m)?);
    Ok(Some(sx.add(&[a, b])?))
}

/// `sum_{k=lo}^inf P(k) b^k` for a polynomial `P`: `P(theta) 1/(1 - w)` at `w = b` with `theta = w d/dw`, less the
/// terms below `lo`.
fn poly_geometric<'h>(sx: &mut Sx<'_, '_, '_, 'h>, f: T<'h>, k: T<'h>, lo: T<'h>, m: &mut Meter) -> Res<Option<T<'h>>> {
    let lo_i = some!(sx.as_int(lo));
    if !(0..=64).contains(&lo_i) {
        return Ok(None);
    }
    let r = some!(hyper::ratio(sx, f, k)?);
    let ring = w!(sx, sx.ring(&[r], &[k], m));
    if !sx.clean(&ring, &[k]) {
        return Ok(None);
    }
    let v = some!(ring.var(k));
    let rf = some!(w!(sx, sx.read(r, &ring, m)));
    let (dn, dd) = (some!(crate::rf::deg(&rf.num, v)), some!(crate::rf::deg(&rf.den, v)));
    if dn != dd || dn == 0 {
        return Ok(None);
    }
    let ln = w!(sx, sx.pterm(&crate::rf::lcoeff(&rf.num, v), &ring, m));
    let ld = w!(sx, sx.pterm(&crate::rf::lcoeff(&rf.den, v), &ring, m));
    let b = sx.div(ln, ld)?;
    let bk = sx.pow(b, k)?;
    // convergence: |b| < 1, assumed when undecided
    if abs_at_least_one(sx, b)? {
        return Ok(None);
    }
    let q = sx.div(f, bk)?;
    let mut poly = None;
    for how in ["expand", "power_expand", "simplify"] {
        let p = sx.ev(how, &[q])?;
        let p = sx.ev("expand", &[p])?;
        if is_polynomial(sx, p, k)? {
            poly = Some(p);
            break;
        }
    }
    let p = some!(poly);
    let cl = sx.ev("coefficient_list", &[p, k])?;
    let cs = some!(sx.as_list(cl)).to_vec();
    if cs.len() > 21 {
        return Ok(None);
    }
    let w = sx.sym("series$w");
    let one_w = sx.sub(sx.int(1), w)?;
    let mut g = sx.pow(one_w, sx.int(-1))?;
    let mut acc = vec![];
    for (j, &c) in cs.iter().enumerate() {
        if j > 0 {
            let dg = sx.ev("d", &[g, w])?;
            g = sx.mul(&[w, dg])?;
            g = sx.ev("together", &[g])?;
        }
        acc.push(sx.mul(&[c, g])?);
    }
    let total = sx.add(&acc)?;
    let mut s = sx.subst(total, w, b)?;
    for j in 0..lo_i {
        let tj = sx.subst(f, k, sx.int(j))?;
        s = sx.sub(s, tj)?;
    }
    let fs = sx.ev("factor", &[s])?;
    Ok(Some(if sx.bad(fs) { s } else { sign_normal(sx, fs)? }))
}

/// `sum_{k=lo}^inf t(k) = t(lo) pFq(a; b; z)` from the ratio `t(k + 1) / t(k) = z prod (k + a_i) / prod (k + b_j)`
/// (linear factors), when the series converges; Gauss's theorem for `2F1(a, b; c; 1)` and the elementary series.
fn series_sum<'h>(sx: &mut Sx<'_, '_, '_, 'h>, f: T<'h>, k: T<'h>, lo: T<'h>, m: &mut Meter) -> Res<Option<T<'h>>> {
    let t0 = sx.subst(f, k, lo)?;
    if sx.bad(t0) {
        return Ok(None);
    }
    // a zero first term: the series from the next one
    if t0 == sx.int(0) {
        let lo1 = sx.plus(lo, 1)?;
        if sx.as_int(lo1).is_some_and(|v| v < 64) {
            return series_sum(sx, f, k, lo1, m);
        }
        return Ok(None);
    }
    let r = some!(hyper::ratio(sx, f, k)?);
    let j = sx.sym("series$j");
    let jl = sx.add(&[j, lo])?;
    let rj = sx.subst(r, k, jl)?;
    let (c, rs) = some!(prod::roots(sx, rj, j, m)?);
    // rs: (root, multiplicity): factor (j - root)
    let mut a = vec![];
    let mut b = vec![];
    for (root, e) in rs {
        let x = sx.mul(&[sx.int(-1), root])?;
        for _ in 0..e.abs() {
            if e > 0 {
                a.push(x);
            } else {
                b.push(x);
            }
        }
    }
    // the factorial: a b_j == 1, else 1 joins both lists
    if let Some(p) = b.iter().position(|&x| x == sx.int(1)) {
        b.remove(p);
    } else {
        a.push(sx.int(1));
    }
    // a pole: some b_j a nonpositive integer
    if b.iter().any(|&x| sx.as_int(x).is_some_and(|v| v <= 0)) {
        return Ok(None);
    }
    let z = c;
    let (p, q) = (a.len(), b.len());
    // convergence: p <= q, or p == q + 1 with |z| < 1 (decided), or a terminating series
    let terminating = a.iter().any(|&x| sx.as_int(x).is_some_and(|v| v <= 0));
    if !terminating && p > q + 1 {
        return Ok(None);
    }

    let value = some!(pfq(sx, &a, &b, z, terminating)?);
    let s = sx.mul(&[t0, value])?;
    Ok(Some(tidy(sx, s)?))
}

/// `pFq(a; b; z)` in closed form where one is known, else as `hypergeometric_pfq` when it converges.
fn pfq<'h>(sx: &mut Sx<'_, '_, '_, 'h>, a: &[T<'h>], b: &[T<'h>], z: T<'h>, terminating: bool) -> Res<Option<T<'h>>> {
    let one = sx.int(1);
    match (a, b) {
        ([], []) => return Ok(Some(sx.ev("exp", &[z])?)),
        ([x], []) => {
            // (1 - z)^-a for |z| < 1 (the generic answer when |z| < 1 is not decided)
            if !terminating && abs_at_least_one(sx, z)? {
                return Ok(None);
            }
            let nz = sx.sub(one, z)?;
            let nx = sx.mul(&[sx.int(-1), *x])?;
            return Ok(Some(sx.pow(nz, nx)?));
        }
        ([], [b]) => {
            // 0F1(; b; z) = gamma(b) z^((1 - b)/2) bessel_i(b - 1, 2 sqrt(z))
            let sq = sx.ev("sqrt", &[z])?;
            let arg = sx.mul(&[sx.int(2), sq])?;
            let b1 = sx.plus(*b, -1)?;
            let bi = sx.ev("bessel_i", &[b1, arg])?;
            let g = sx.ev("gamma", &[*b])?;
            let one_b = sx.sub(sx.int(1), *b)?;
            let e = sx.div(one_b, sx.int(2))?;
            let ze = sx.pow(z, e)?;
            return Ok(Some(sx.mul(&[g, ze, bi])?));
        }
        ([x], [w]) if *x == one && *w == sx.int(2) => {
            // 1F1(1; 2; z) = (exp(z) - 1) / z
            let ez = sx.ev("exp", &[z])?;
            let d = sx.plus(ez, -1)?;
            return Ok(Some(sx.div(d, z)?));
        }
        ([x, y], [w]) if *x == one && *y == one && *w == sx.int(2) && z != one => {
            // 2F1(1, 1; 2; z) = -log(1 - z) / z, for |z| <= 1 only: the series diverges beyond the unit circle, and
            // its analytic continuation is not the value of the sum (B23-7: sum(2^k/k) is not -im*pi)
            if !terminating && abs_greater_one(sx, z)? {
                return Ok(None);
            }
            let oz = sx.sub(one, z)?;
            let l = sx.ev("log", &[oz])?;
            let nl = sx.mul(&[sx.int(-1), l])?;
            return Ok(Some(sx.div(nl, z)?));
        }
        ([x, y], [w]) if z == one => {
            // Gauss: gamma(c) gamma(c - a - b) / (gamma(c - a) gamma(c - b)), Re(c - a - b) > 0
            let xy = sx.add(&[*x, *y])?;
            let cab = sx.sub(*w, xy)?;
            let re = sx.ev("re", &[cab])?;
            let nonpos = sx.ev("less_equal", &[re, sx.int(0)])?;
            if sx.is_true(nonpos) && !terminating {
                return Ok(None);
            }
            let ca = sx.sub(*w, *x)?;
            let cb = sx.sub(*w, *y)?;
            let gs = vec![(*w, 1), (cab, 1), (ca, -1), (cb, -1)];
            // a pole of gamma(c - a) or gamma(c - b) makes the value zero
            for &(g, e) in &gs {
                if let Some(v) = sx.as_int(g) {
                    if v <= 0 {
                        return Ok(if e < 0 { Some(sx.int(0)) } else { None });
                    }
                }
            }
            return Ok(Some(prod::gamma_product(sx, gs)?));
        }
        _ => {}
    }
    let (p, q) = (a.len(), b.len());
    if p == q + 1 && !terminating && abs_at_least_one(sx, z)? {
        return Ok(None);
    }
    let al = sx.list(a);
    let bl = sx.list(b);
    let v = sx.ev("hypergeometric_pfq", &[al, bl, z])?;
    if sx.bad(v) {
        return Ok(None);
    }
    Ok(Some(v))
}

/// `|z| >= 1` decided?
fn abs_at_least_one<'h>(sx: &mut Sx<'_, '_, '_, 'h>, z: T<'h>) -> Res<bool> {
    let a = sx.ev("abs", &[z])?;
    let l = sx.ev("greater_equal", &[a, sx.int(1)])?;
    Ok(sx.is_true(l))
}

/// `|z| > 1` decided?
fn abs_greater_one<'h>(sx: &mut Sx<'_, '_, '_, 'h>, z: T<'h>) -> Res<bool> {
    let a = sx.ev("abs", &[z])?;
    let l = sx.ev("greater", &[a, sx.int(1)])?;
    Ok(sx.is_true(l))
}

/// The definite sum by Zeilberger's algorithm in `n` (the variable of the upper bound `hi`, `lo` free of `n`).
fn zeilberger_sum<'h>(sx: &mut Sx<'_, '_, '_, 'h>, f: T<'h>, k: T<'h>, n: T<'h>, lo: T<'h>, hi: T<'h>, m: &mut Meter) -> Res<Option<T<'h>>> {
    let rk = some!(hyper::ratio(sx, f, k)?);
    let rn = some!(hyper::ratio(sx, f, n)?);
    let zv = sx.sym("hyper$z");
    let ring = w!(sx, sx.ring(&[rk, rn], &[k, n, zv], m));
    if !sx.clean(&ring, &[k, n]) {
        return Ok(None);
    }
    let (kv, nvar) = (some!(ring.var(k)), some!(ring.var(n)));
    let rkf = some!(w!(sx, sx.read(rk, &ring, m)));
    let rnf = some!(w!(sx, sx.read(rn, &ring, m)));
    if rkf.is_zero() || rnf.is_zero() {
        return Ok(None);
    }
    sx.trace("zeilberger summand", f);
    let tel = some!(w!(sx, zeil::zeilberger(&rkf, &rnf, kv, nvar, 3, m)));
    for c in &tel.coefs {
        let ct = w!(sx, sx.pterm(c, &ring, m));
        sx.trace("zeilberger coefficient", ct);
    }
    let mut coefs = tel.coefs;
    while coefs.first().is_some_and(|c| c.is_zero()) {
        coefs.remove(0);
    }
    if coefs.len() < 2 {
        return Ok(None);
    }
    let sols = some!(solve_recurrence(sx, &coefs, nvar, &ring, n, m)?);
    for &y in &sols {
        sx.trace("zeilberger solution", y);
    }
    // fit S(n) = sum c_i y_i(n) to the values at n0, n0 + 1, ...
    let direct = |sx: &mut Sx<'_, '_, '_, 'h>, nv: i64| -> Res<Option<T<'h>>> {
        let fv = sx.subst(f, n, sx.int(nv))?;
        let hv = sx.subst(hi, n, sx.int(nv))?;
        let (Some(l), Some(u)) = (sx.as_int(lo), sx.as_int(hv)) else { return Ok(None) };
        if u - l > 400 {
            return Ok(None);
        }
        let mut acc = sx.int(0);
        for j in l..=u {
            let tj = sx.subst(fv, k, sx.int(j))?;
            if sx.bad(tj) {
                return Ok(None);
            }
            acc = sx.add(&[acc, tj])?;
        }
        Ok(Some(acc))
    };
    let order = coefs.len() - 1;
    let nsols = sols.len();
    // the first n where the bound is at least lo
    let lo_i = some!(sx.as_int(lo));
    let mut n0 = 0i64;
    for cand in -5..=40 {
        let hv = sx.subst(hi, n, sx.int(cand))?;
        if sx.as_int(hv).is_some_and(|u| u >= lo_i) {
            n0 = cand.max(0);
            break;
        }
    }
    let _ = order;
    let cs: Vec<T<'h>> = (0..nsols).map(|i| sx.sym(&format!("fit${i}"))).collect();
    let mut eqs = vec![];
    let mut n_at = n0;
    while eqs.len() < nsols + 2 && n_at < n0 + 12 {
        let s_val = some!(direct(sx, n_at)?);
        let s_val = normalize_values(sx, s_val)?;
        let mut terms = vec![];
        let mut ok = true;
        for (c, y) in cs.iter().zip(&sols) {
            let yv = sx.subst(*y, n, sx.int(n_at))?;
            let yv = normalize_values(sx, yv)?;
            if sx.bad(yv) {
                ok = false;
                break;
            }
            terms.push(sx.mul(&[*c, yv])?);
        }
        n_at += 1;
        if !ok {
            continue;
        }
        let lhs = sx.add(&terms)?;
        eqs.push(sx.call("equal", &[lhs, s_val]));
    }
    if eqs.len() < nsols {
        return Ok(None);
    }
    let el = sx.list(&eqs);
    let cl = sx.list(&cs);
    let sol = sx.ev("solve", &[el, cl])?;
    let rules = some!(sx.as_list(sol).and_then(|s| s.first().copied()));
    let rules = some!(sx.as_list(rules)).to_vec();
    let mut terms = vec![];
    for (c, y) in cs.iter().zip(&sols) {
        terms.push(sx.mul(&[*c, *y])?);
    }
    let comb = sx.add(&terms)?;
    let closed = sx.subst_all(comb, &rules)?;
    if !cs.iter().all(|&c| sx.free(closed, c)) {
        return Ok(None);
    }
    let closed = tidy(sx, closed)?;
    let closed = binomial_form(sx, closed)?;
    sx.trace("zeilberger closed form", closed);
    if verify::check_sum(sx, f, k, lo, hi, closed)? {
        return Ok(Some(closed));
    }
    Ok(None)
}

/// `t` with `binomial`, `factorial`, ... of symbolic arguments written through `gamma` and shifted `gamma` values
/// reduced, so that `binomial(m, 1)` is `m`.
pub fn normalize_values<'h>(sx: &mut Sx<'_, '_, '_, 'h>, t: T<'h>) -> Res<T<'h>> {
    if !sx.contains_any(t, &["binomial", "factorial", "gamma", "pochhammer"]) {
        return Ok(t);
    }
    let mut u = sx.ev("function_expand", &[t])?;
    for s in verify::symbols(sx, u, &[]) {
        u = crate::shift::reduce(sx, u, s)?;
    }
    let v = sx.ev("together", &[u])?;
    Ok(if sx.bad(v) { t } else { v })
}

/// Hypergeometric solutions of `sum_i coefs[i](n) y(n + i) = 0` as terms in `n` (up to constant factors).
pub fn solve_recurrence<'h>(
    sx: &mut Sx<'_, '_, '_, 'h>,
    coefs: &[mc_poly::Poly],
    nvar: usize,
    ring: &Ring<'h>,
    n: T<'h>,
    m: &mut Meter,
) -> Res<Option<Vec<T<'h>>>> {
    let zv = some!(ring.var(sx.sym("hyper$z")));
    let sols = w!(sx, recur::hyper(coefs, nvar, zv, m));
    if sols.is_empty() {
        return Ok(None);
    }
    let mut out = vec![];
    for s in sols {
        out.push(some!(hyper_term(sx, &s, nvar, ring, n, m)?));
    }
    Ok(Some(out))
}

/// The term `y(n)` of a hypergeometric solution: `(Z c_A / c_B)^n prod gamma(n - a) / prod gamma(n - b) C(n)` over
/// the roots `a` of `A` and `b` of `B`.
pub fn hyper_term<'h>(sx: &mut Sx<'_, '_, '_, 'h>, s: &recur::HyperSol, nvar: usize, ring: &Ring<'h>, n: T<'h>, m: &mut Meter) -> Res<Option<T<'h>>> {
    let _ = nvar;
    let ab = w!(sx, Rf::new(s.a.clone(), s.b.clone(), m));
    let abt = w!(sx, sx.term(&ab, ring, m));
    let zt = w!(sx, sx.term(&s.z, ring, m));
    let ct = w!(sx, sx.pterm(&s.c, ring, m));
    let (c0, rs) = some!(prod::roots(sx, abt, n, m)?);
    let base = sx.mul(&[zt, c0])?;
    let mut fs = vec![sx.pow(base, n)?, ct];
    let mut gs = vec![];
    for (r, e) in rs {
        gs.push((sx.sub(n, r)?, e));
    }
    fs.push(prod::gamma_product(sx, gs)?);
    let y = sx.mul(&fs)?;
    Ok(Some(tidy(sx, y)?))
}

/// A product of `gamma` values written with binomial coefficients where it can: `(-1)^j gamma(j + c) / gamma(c)`
/// is `gamma(1 - c) / gamma(1 - c - j)` (the reflection of a falling product), `gamma(u + 1/2)` is
/// `sqrt(pi) gamma(2 u + 1) / (4^u gamma(u + 1))` (Legendre's duplication), and `gamma(x) / (gamma(y) gamma(z))`
/// with `x = y + z - 1` is `binomial(x - 1, y - 1)`. The form is kept only when it is smaller.
pub fn binomial_form<'h>(sx: &mut Sx<'_, '_, '_, 'h>, t: T<'h>) -> Res<T<'h>> {
    let h = sx.h();
    let View::App { head, args } = h.view(t) else { return Ok(t) };
    if head != sx.sym(names::MUL) || !sx.contains_any(t, &["gamma"]) {
        return Ok(t);
    }
    // the factors as (base, exponent)
    let mut fs: Vec<(T<'h>, T<'h>)> = vec![];
    for &a in args {
        match sx.args_of(a, names::POW) {
            Some(&[b, e]) => fs.push((b, e)),
            _ => fs.push((a, sx.int(1))),
        }
    }
    let gamma_arg = |sx: &Sx<'_, '_, '_, 'h>, b: T<'h>| sx.args_of(b, "gamma").and_then(|x| x.first().copied());
    // (-1)^j gamma(j + c) / gamma(c) -> gamma(1 - c) / gamma(1 - c - j)
    if let Some(si) = fs.iter().position(|(b, _)| *b == sx.int(-1)) {
        let j = fs[si].1;
        let mut done = false;
        for a in 0..fs.len() {
            if done {
                break;
            }
            let (Some(u), true) = (gamma_arg(sx, fs[a].0), fs[a].1 == sx.int(1)) else { continue };
            let c = sx.sub(u, j)?;
            let c = sx.ev("expand", &[c])?;
            if !sx.free(c, j) {
                continue;
            }
            for b in 0..fs.len() {
                let Some(gb) = gamma_arg(sx, fs[b].0) else { continue };
                let gb = sx.ev("expand", &[gb])?;
                if gb == c && fs[b].1 == sx.int(-1) {
                    let one_c = sx.sub(sx.int(1), c)?;
                    let x = sx.sub(one_c, j)?;
                    fs[a] = (sx.ev("gamma", &[one_c])?, sx.int(1));
                    fs[b] = (sx.ev("gamma", &[x])?, sx.int(-1));
                    fs[si] = (sx.int(1), sx.int(1));
                    done = true;
                    break;
                }
            }
        }
    }
    // gamma(u + 1/2) by the duplication formula
    let half = sx.div(sx.int(1), sx.int(2))?;
    for fi in fs.iter_mut() {
        let Some(u) = gamma_arg(sx, fi.0) else { continue };
        let (v, _) = crate::shift::offset(sx, u)?;
        let w = sx.sub(u, half)?;
        let w = sx.ev("expand", &[w])?;
        if v == u && crate::verify::symbols(sx, w, &[]).len() == 1 && fi.1 == sx.int(1) {
            let rest = sx.sub(u, w)?;
            if rest != half {
                continue;
            }
            let w2 = sx.mul(&[sx.int(2), w])?;
            let w21 = sx.plus(w2, 1)?;
            let w1 = sx.plus(w, 1)?;
            let g = sx.ev("gamma", &[w21])?;
            let gd = sx.ev("gamma", &[w1])?;
            let four = sx.pow(sx.int(4), w)?;
            let sq = sx.ev("sqrt", &[sx.sym("pi")])?;
            let gdi = sx.pow(gd, sx.int(-1))?;
            let finv = sx.pow(four, sx.int(-1))?;
            *fi = (sx.mul(&[sq, g, gdi, finv])?, sx.int(1));
        }
    }
    let mut parts = vec![];
    for (b, e) in &fs {
        parts.push(sx.pow(*b, *e)?);
    }
    let u = sx.mul(&parts)?;
    // gamma(x) / (gamma(y) gamma(z)) with x = y + z - 1
    let View::App { head: _, args: uargs } = h.view(u) else { return Ok(t) };
    let mut num = vec![];
    let mut den = vec![];
    let mut other = vec![];
    for &a in uargs {
        match (sx.args_of(a, names::POW), gamma_arg(sx, a)) {
            (Some(&[b, e]), _) if gamma_arg(sx, b).is_some() && sx.as_int(e).is_some_and(|e| e < 0) => {
                for _ in 0..-sx.as_int(e).unwrap_or(0) {
                    den.push(gamma_arg(sx, b).expect("a gamma"));
                }
            }
            (_, Some(x)) => num.push(x),
            _ => other.push(a),
        }
    }
    let mut out = other;
    let mut used = vec![false; den.len()];
    for x in num {
        let mut found = None;
        'pairs: for i in 0..den.len() {
            for j in i + 1..den.len() {
                if used[i] || used[j] {
                    continue;
                }
                let s = sx.add(&[den[i], den[j]])?;
                let s = sx.plus(s, -1)?;
                let d = sx.sub(s, x)?;
                if sx.ev("expand", &[d])? == sx.int(0) {
                    found = Some((i, j));
                    break 'pairs;
                }
            }
        }
        match found {
            Some((i, j)) => {
                used[i] = true;
                used[j] = true;
                let a = sx.plus(x, -1)?;
                let a = sx.ev("expand", &[a])?;
                let b = sx.plus(den[i], -1)?;
                let b = sx.ev("expand", &[b])?;
                out.push(sx.ev("binomial", &[a, b])?);
            }
            None => out.push(sx.ev("gamma", &[x])?),
        }
    }
    for (i, d) in den.iter().enumerate() {
        if !used[i] {
            let g = sx.ev("gamma", &[*d])?;
            out.push(sx.pow(g, sx.int(-1))?);
        }
    }
    let v = sx.mul(&out)?;
    Ok(if !sx.contains_any(v, &["gamma"]) && sx.cost(v) < sx.cost(t) { v } else { t })
}

/// The period of `f` in `k` when every occurrence of `k` is inside `mod(p, m)` with an integer m > 0 and p a
/// polynomial in k with integer coefficients (p(k + m) = p(k) mod m): the lcm of the moduli (at most 1000).
fn period<'h>(sx: &mut Sx<'_, '_, '_, 'h>, f: T<'h>, k: T<'h>) -> Res<Option<i64>> {
    let h = sx.h();
    let modh = sx.sym("mod");
    let mut p: i64 = 1;
    let mut stack = vec![f];
    let mut seen_mod = false;
    while let Some(u) = stack.pop() {
        if u == k {
            return Ok(None);
        }
        let View::App { head, args } = h.view(u) else { continue };
        if head == modh && args.len() == 2 && !sx.free(u, k) {
            let View::Int(m) = h.view(args[1]) else { return Ok(None) };
            if m <= 0 || !is_polynomial(sx, args[0], k)? {
                return Ok(None);
            }
            // integer coefficients: the coefficient list free of non-integers
            let cl = sx.ev("coefficient_list", &[args[0], k])?;
            let Some(cs) = sx.as_list(cl) else { return Ok(None) };
            if !cs.iter().all(|&c| matches!(h.view(c), View::Int(_) | View::BigInt { .. })) {
                return Ok(None);
            }
            let g = gcd_i64(p, m);
            p = p / g * m;
            if p > 1000 {
                return Ok(None);
            }
            seen_mod = true;
            continue;
        }
        stack.push(head);
        stack.extend_from_slice(args);
    }
    Ok(seen_mod.then_some(p))
}

fn gcd_i64(a: i64, b: i64) -> i64 {
    if b == 0 {
        a.abs()
    } else {
        gcd_i64(b, a % b)
    }
}

/// The indefinite sum (F(k + 1) - F(k) = f(k)) of a summand periodic in k with period P, from its values
/// c_r = f(r), r < P, and their total C: F(k) = C floor(k/P) + sum_r c_r floor((mod(k, P) + P - 1 - r)/P) (the
/// second floor is 1 exactly when mod(k, P) > r). Checked by differencing at 3P + 2 consecutive integers.
fn periodic<'h>(sx: &mut Sx<'_, '_, '_, 'h>, f: T<'h>, k: T<'h>) -> Res<Option<T<'h>>> {
    let Some(pp) = period(sx, f, k)? else { return Ok(None) };
    let pt = sx.int(pp);
    let mut cs = Vec::with_capacity(pp as usize);
    for r in 0..pp {
        let v = sx.subst(f, k, sx.int(r))?;
        if sx.ar.num(v).is_none() {
            return Ok(None);
        }
        cs.push(v);
    }
    let total = sx.add(&cs)?;
    let q = sx.div(k, pt)?;
    let fl = sx.ev("floor", &[q])?;
    let mut parts = vec![sx.mul(&[total, fl])?];
    let mk = sx.ev("mod", &[k, pt])?;
    for (r, &c) in cs.iter().enumerate() {
        // r = P - 1: mod(k, P) > r never holds
        if c == sx.int(0) || r as i64 == pp - 1 {
            continue;
        }
        let shifted = sx.add(&[mk, sx.int(pp - 1 - r as i64)])?;
        let q = sx.div(shifted, pt)?;
        let fl = sx.ev("floor", &[q])?;
        parts.push(sx.mul(&[c, fl])?);
    }
    let big = sx.add(&parts)?;
    // the check: F(j + 1) - F(j) = f(j)
    for j in -pp..2 * pp + 2 {
        let a = sx.subst(big, k, sx.int(j + 1))?;
        let b = sx.subst(big, k, sx.int(j))?;
        let d = sx.sub(a, b)?;
        let fj = sx.subst(f, k, sx.int(j))?;
        let z = sx.sub(d, fj)?;
        if z != sx.int(0) {
            return Ok(None);
        }
    }
    Ok(Some(big))
}
