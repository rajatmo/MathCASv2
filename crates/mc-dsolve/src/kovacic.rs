//! Kovacic's algorithm, case 1: Liouvillian solutions `exp(integrate(omega))` of `z'' = r z`, r a rational function.
//!
//! `y'' + a y' + b y = 0` becomes `z'' = r z` with `r = a^2/4 + a'/2 - b` and `y = z exp(-integrate(a)/2)`. For case 1
//! the poles of r have order 1 or even order, and the order at infinity is even or above 2. At each pole c and at
//! infinity the truncated Laurent series `[sqrt r]` and the exponents `alpha^+-` are computed (Kovacic 1986, section
//! 3.1); for every choice of signs with `d = alpha_inf - sum alpha_c` a nonnegative integer,
//! `omega = sum (s_c [sqrt r]_c + alpha_c/(x - c)) + s_inf [sqrt r]_inf`, and a monic polynomial P of degree d with
//! `P'' + 2 omega P' + (omega' + omega^2 - r) P = 0` gives the solution `z = P exp(integrate(omega))`. The second
//! solution comes from reduction of order.
//!
//! Cases 2 and 3 (solutions algebraic over the rational functions of degree 2, or of degree 4, 6, 12) are not
//! implemented: such equations stay unevaluated unless another method solves them.
//!
//! Literature: J. J. Kovacic, "An algorithm for solving second order linear homogeneous differential equations",
//! J. Symb. Comp. 2 (1986) 3-43; M. Bronstein, "Linear ordinary differential equations: breaking through the order 2
//! barrier", ISSAC 1992; F. Ulmer and J.-A. Weil, "Note on Kovacic's algorithm", J. Symb. Comp. 22 (1996).
use super::Cx;
use mc_calculus::common::is_free;
use mc_api::{Store, View, T};
use mc_eval::Res;

/// The most poles tried (2^(poles + 1) sign choices).
const MAX_POLES: usize = 5;

/// One point: `[sqrt r]` and the two exponents.
struct Local<'h> {
    /// the pole (None at infinity)
    at: Option<T<'h>>,
    sq: T<'h>,
    alpha: [T<'h>; 2],
}

/// A basis from Kovacic's case 1.
pub(crate) fn basis<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, coefs: &[T<'h>], x: T<'h>) -> Res<Option<Vec<T<'h>>>> {
    let b = c.b;
    if coefs.len() != 3 {
        return Ok(None);
    }
    let a = c.simp(b.div(coefs[1], coefs[2]))?;
    let bb = c.simp(b.div(coefs[0], coefs[2]))?;
    // rational in x?
    for t in [a, bb] {
        let tt = c.together(t)?;
        let num = c.ev("numerator", &[tt])?;
        let den = c.ev("denominator", &[tt])?;
        for p in [num, den] {
            let cl = c.ev("coefficient_list", &[p, x])?;
            let Some(items) = c.items(cl) else { return Ok(None) };
            if items.iter().any(|&i| !is_free(b, i, x)) {
                return Ok(None);
            }
        }
    }
    let da = c.d(a, x)?;
    let r = c.eval(b.add(&[b.mul(&[b.rat(1, 4), b.powi(a, 2)]), b.mul(&[b.rat(1, 2), da]), b.neg(bb)]))?;
    let r = c.together(r)?;
    let Some(z) = case1(c, r, x)? else { return Ok(None) };
    let Some(ia) = c.integrate(a, x)? else { return Ok(None) };
    let y1 = c.simp(b.mul(&[z, b.call("exp", &[b.mul(&[b.rat(-1, 2), ia])])]))?;
    let Some(y2) = super::linear::reduction(c, coefs, y1, x)? else { return Ok(None) };
    Ok(Some(vec![y1, y2]))
}

/// The degree of the polynomial `p` in x.
fn degree<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, p: T<'h>, x: T<'h>) -> Res<Option<i64>> {
    let cl = c.ev("coefficient_list", &[p, x])?;
    let Some(items) = c.items(cl) else { return Ok(None) };
    Ok(Some(items.len() as i64 - 1))
}

/// A solution `P exp(integrate(omega))` of `z'' = r z`.
fn case1<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, r: T<'h>, x: T<'h>) -> Res<Option<T<'h>>> {
    let b = c.b;
    let num = c.ev("numerator", &[r])?;
    let den = c.ev("denominator", &[r])?;
    let num = c.expand(num)?;
    let den = c.expand(den)?;
    let (Some(ds), Some(dt)) = (degree(c, num, x)?, degree(c, den, x)?) else { return Ok(None) };
    let oinf = dt - ds;
    // the poles with their orders
    let mut poles: Vec<(T<'h>, usize)> = vec![];
    if dt > 0 {
        let cl = c.ev("coefficient_list", &[den, x])?;
        let Some(dc) = c.items(cl) else { return Ok(None) };
        let Some(rts) = super::linear::roots(c, &dc)? else { return Ok(None) };
        poles = rts;
    }
    if poles.len() > MAX_POLES {
        return Ok(None);
    }
    // necessary conditions of case 1
    if poles.iter().any(|&(_, o)| o != 1 && o % 2 == 1) || (oinf < 2 && oinf % 2 != 0) {
        return Ok(None);
    }
    let mut locals = vec![];
    for &(p, o) in &poles {
        let dx = b.sub(x, p);
        let l = match o {
            1 => Local { at: Some(p), sq: b.zero(), alpha: [b.one(), b.one()] },
            2 => {
                let bq = lead_at(c, r, x, p, 2)?;
                let s = c.ev("power_expand", &[b.pow(b.add(&[b.one(), b.mul(&[b.int(4), bq])]), b.rat(1, 2))])?;
                let ap = c.simp(b.add(&[b.rat(1, 2), b.mul(&[b.rat(1, 2), s])]))?;
                let am = c.simp(b.sub(b.rat(1, 2), b.mul(&[b.rat(1, 2), s])))?;
                Local { at: Some(p), sq: b.zero(), alpha: [ap, am] }
            }
            _ => {
                let nu = (o / 2) as i64;
                // [sqrt r]_c: the terms (x - c)^-i, 2 <= i <= nu, of the Laurent series of sqrt(r)
                let ser = c.ev("series", &[b.pow(r, b.rat(1, 2)), x, p, b.int(-2)])?;
                if c.stuck(ser, "series") {
                    return Ok(None);
                }
                let nrm = c.ev("normal", &[ser])?;
                let t = c.sym("d_solve·kv");
                let nt = c.subst(nrm, &[c.rule(x, b.add(&[p, b.powi(t, -1)]))])?;
                let nt = c.expand(nt)?;
                let mut sq = vec![];
                let mut anu = b.zero();
                for i in 2..=nu {
                    let ai = c.ev("coefficient", &[nt, t, b.int(i)])?;
                    let ai = c.simp(ai)?;
                    if i == nu {
                        anu = ai;
                    }
                    sq.push(b.mul(&[ai, b.powi(dx, -i)]));
                }
                if anu == b.zero() {
                    return Ok(None);
                }
                let sqv = c.eval(b.add(&sq))?;
                let rest = c.together(b.sub(r, b.powi(sqv, 2)))?;
                let bq = coefficient_at(c, rest, x, p, nu + 1)?;
                let q = c.simp(b.div(bq, anu))?;
                let ap = c.simp(b.mul(&[b.rat(1, 2), b.add(&[q, b.int(nu)])]))?;
                let am = c.simp(b.mul(&[b.rat(1, 2), b.add(&[b.neg(q), b.int(nu)])]))?;
                Local { at: Some(p), sq: sqv, alpha: [ap, am] }
            }
        };
        locals.push(l);
    }
    // infinity
    let inf = if oinf > 2 {
        Local { at: None, sq: b.zero(), alpha: [b.zero(), b.one()] }
    } else if oinf == 2 {
        // r ~ bq / x^2
        let lr = c.simp(b.mul(&[r, b.powi(x, 2)]))?;
        let bq = c.ev("limit", &[lr, c.rule(x, c.sym("infinity"))])?;
        if c.bad(bq) {
            return Ok(None);
        }
        let s = c.ev("power_expand", &[b.pow(b.add(&[b.one(), b.mul(&[b.int(4), bq])]), b.rat(1, 2))])?;
        let ap = c.simp(b.add(&[b.rat(1, 2), b.mul(&[b.rat(1, 2), s])]))?;
        let am = c.simp(b.sub(b.rat(1, 2), b.mul(&[b.rat(1, 2), s])))?;
        Local { at: None, sq: b.zero(), alpha: [ap, am] }
    } else {
        // o_inf = -2 nu: [sqrt r]_inf the polynomial part of sqrt(r) at infinity
        let nu = -oinf / 2;
        let t = c.sym("d_solve·kv");
        let rt = c.subst(r, &[c.rule(x, b.powi(t, -1))])?;
        let ser = c.ev("series", &[b.pow(rt, b.rat(1, 2)), t, b.zero(), b.int(0)])?;
        if c.stuck(ser, "series") {
            return Ok(None);
        }
        let nrm = c.ev("normal", &[ser])?;
        let nrm = c.expand(nrm)?;
        let mut sq = vec![];
        let mut anu = b.zero();
        for i in 0..=nu {
            let ai = c.ev("coefficient", &[nrm, t, b.int(-i)])?;
            let ai = c.simp(ai)?;
            if i == nu {
                anu = ai;
            }
            sq.push(b.mul(&[ai, b.powi(x, i)]));
        }
        if anu == b.zero() {
            return Ok(None);
        }
        let sqv = c.eval(b.add(&sq))?;
        let rest = c.together(b.sub(r, b.powi(sqv, 2)))?;
        // the coefficient of x^(nu - 1) of the expansion at infinity
        let rest_t = c.subst(rest, &[c.rule(x, b.powi(t, -1))])?;
        let ser2 = c.ev("series", &[rest_t, t, b.zero(), b.int(1 - nu)])?;
        let n2 = c.ev("normal", &[ser2])?;
        let n2 = c.expand(n2)?;
        let bq = c.ev("coefficient", &[n2, t, b.int(1 - nu)])?;
        let q = c.simp(b.div(bq, anu))?;
        let ap = c.simp(b.mul(&[b.rat(1, 2), b.sub(q, b.int(nu))]))?;
        let am = c.simp(b.mul(&[b.rat(1, 2), b.sub(b.neg(q), b.int(nu))]))?;
        Local { at: None, sq: sqv, alpha: [ap, am] }
    };
    let k = locals.len();
    for mask in 0..(1u32 << (k + 1)) {
        if c.late() {
            return Ok(None);
        }
        let sinf = (mask >> k) & 1;
        let mut d = inf.alpha[sinf as usize];
        let mut omega = vec![if sinf == 0 { inf.sq } else { b.neg(inf.sq) }];
        for (i, l) in locals.iter().enumerate() {
            let s = (mask >> i) & 1;
            d = b.sub(d, l.alpha[s as usize]);
            let sq = if s == 0 { l.sq } else { b.neg(l.sq) };
            let Some(p) = l.at else { continue };
            omega.push(sq);
            omega.push(b.div(l.alpha[s as usize], b.sub(x, p)));
        }
        let d = c.simp(d)?;
        let View::Int(dn) = b.heap().view(d) else { continue };
        if !(0..=12).contains(&dn) {
            continue;
        }
        let w = c.eval(b.add(&omega))?;
        let Some(pp) = polynomial(c, w, r, x, dn as usize)? else { continue };
        let Some(iw) = c.integrate(w, x)? else { continue };
        let z = c.simp(b.mul(&[pp, b.call("exp", &[iw])]))?;
        return Ok(Some(z));
    }
    Ok(None)
}

/// The coefficient of `(x - p)^-k` in the Laurent series of `r` at p (r with a pole of order at most k there).
fn lead_at<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, r: T<'h>, x: T<'h>, p: T<'h>, k: i64) -> Res<T<'h>> {
    let b = c.b;
    let t = c.simp(b.mul(&[r, b.powi(b.sub(x, p), k)]))?;
    let at = c.subst(t, &[c.rule(x, p)])?;
    if c.bad(at) {
        return c.ev("limit", &[t, c.rule(x, p)]);
    }
    c.simp(at)
}

/// The coefficient of `(x - p)^-k` in the Laurent series of `r` at p.
fn coefficient_at<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, r: T<'h>, x: T<'h>, p: T<'h>, k: i64) -> Res<T<'h>> {
    let b = c.b;
    let ser = c.ev("series", &[r, x, p, b.int(-k)])?;
    let nrm = c.ev("normal", &[ser])?;
    let t = c.sym("d_solve·kv");
    let nt = c.subst(nrm, &[c.rule(x, b.add(&[p, b.powi(t, -1)]))])?;
    let nt = c.expand(nt)?;
    let v = c.ev("coefficient", &[nt, t, b.int(k)])?;
    c.simp(v)
}

/// A monic polynomial P of degree d with `P'' + 2 w P' + (w' + w^2 - r) P = 0`.
fn polynomial<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, w: T<'h>, r: T<'h>, x: T<'h>, d: usize) -> Res<Option<T<'h>>> {
    let b = c.b;
    let a: Vec<T<'h>> = (0..d).map(|j| c.sym(&format!("d_solve·kp·{j}"))).collect();
    let mut terms: Vec<T<'h>> = (0..d).map(|j| b.mul(&[a[j], b.powi(x, j as i64)])).collect();
    terms.push(b.powi(x, d as i64));
    let p = c.eval(b.add(&terms))?;
    let dp = c.d(p, x)?;
    let d2p = c.d(dp, x)?;
    let dw = c.d(w, x)?;
    let e = b.add(&[d2p, b.mul(&[b.int(2), w, dp]), b.mul(&[b.add(&[dw, b.powi(w, 2), b.neg(r)]), p])]);
    let e = c.eval(e)?;
    let e = c.together(e)?;
    let num = c.ev("numerator", &[e])?;
    let num = c.expand(num)?;
    let cl = c.ev("coefficient_list", &[num, x])?;
    let Some(eqs) = c.items(cl) else { return Ok(None) };
    if eqs.iter().any(|&q| !is_free(b, q, x)) {
        return Ok(None);
    }
    let eqs: Vec<T<'h>> = eqs.into_iter().filter(|&q| q != b.zero()).map(|q| c.equal(q, b.zero())).collect();
    if eqs.is_empty() {
        return Ok(Some(p));
    }
    if a.is_empty() {
        return Ok(None);
    }
    let s = c.ev("solve", &[c.list(&eqs), c.list(&a)])?;
    let Some(first) = c.items(s).and_then(|v| v.first().copied()) else { return Ok(None) };
    let Some(rules) = c.items(first) else { return Ok(None) };
    let v = c.subst(p, &rules)?;
    if a.iter().any(|&ai| !c.free(v, ai)) {
        return Ok(None);
    }
    Ok(Some(v))
}
