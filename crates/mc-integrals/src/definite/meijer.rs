//! The Meijer G stage of definite integration (round 22; the hook of decision 14D.1).
//!
//! An integrand on `(0, inf)` that is `C x^alpha f1(e1 x^k) f2(e2 x^k)` with f1, f2 functions with a Meijer G
//! representation (exponentials, `sin`, `cos`, Bessel J and K, `erfc`, `(1 + t)^-a`; Prudnikov, Brychkov and
//! Marichev, *Integrals and Series* 3, 1990, section 8.4) is integrated by Parseval's formula for the Mellin
//! transform: with M[G1](s) = Phi1(s) and u = x^k,
//!
//! ```text
//! integral_0^inf G1(e1 u) G2(e2 u) du = 1/(2 pi i) integral e1^-s Phi1(s) e2^(s-1) Phi2(1 - s) ds
//!   = (1/e2) G^{m+nu, n+mu}_{p+tau, q+sigma}(e1/e2 | a_(<=n), -d_(<=mu); a_(>n), -d_(>mu);
//!                                                     b_(<=m), -c_(<=nu); b_(>m), -c_(>nu))
//! ```
//!
//! (PBM 3, 2.24.1.1; Adamchik and Marichev, ISSAC 1990, section 2; the power u^(beta - 1) is absorbed into G1 by
//! z^beta G(z | a; b) = G(z | a + beta; b + beta)). One G-representable factor alone gives its Mellin transform,
//! a gamma ratio. The resulting G function is reduced by `meijer_g` itself (named functions, Slater's sums).
//! The validity conditions of the formula (PBM 2.24.1, 16 cases) are not decided symbolically: the value is
//! returned only when the numeric check of the driver (14D.3) confirms it, so a formula applied outside its
//! conditions, or an integral that diverges, is never answered.
use super::check::{self, Verdict};
use super::{call_of, sign, Pt};
use crate::{apply, args_of, is_free};
use mc_calculus::term::Build;
use mc_api::{Store, T};
use mc_eval::{NativeCx, Res};

/// A factor `coef G(eta x^k | an, ap; bm, bq)`.
struct Rep<'h> {
    coef: T<'h>,
    eta: T<'h>,
    k: T<'h>,
    an: Vec<T<'h>>,
    ap: Vec<T<'h>>,
    bm: Vec<T<'h>>,
    bq: Vec<T<'h>>,
}

/// `c x^d` as `(c, d)` (one factor depending on x, a power of x).
fn monomial<'h>(b: &Build<'h>, u: T<'h>, x: T<'h>) -> Option<(T<'h>, T<'h>)> {
    let fs: Vec<T<'h>> = match args_of(b, u, b.hd.mul) {
        Some(fs) => fs.to_vec(),
        None => vec![u],
    };
    let mut c = vec![];
    let mut d = None;
    for g in fs {
        if is_free(b, g, x) {
            c.push(g);
        } else if g == x && d.is_none() {
            d = Some(b.one());
        } else if let (Some(&[base, e]), None) = (args_of(b, g, b.hd.pow), d) {
            if base != x || !is_free(b, e, x) {
                return None;
            }
            d = Some(e);
        } else {
            return None;
        }
    }
    d.map(|d| (b.mul(&c), d))
}

/// The G representation of one factor (module comment), or `None`.
fn represent<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, g: T<'h>, x: T<'h>) -> Res<Option<Rep<'h>>> {
    let h = b.heap();
    let half = b.rat(1, 2);
    let quarter = b.rat(1, 4);
    let sqrt_pi = b.pow(b.sym(h.sym("pi")), half);
    let rep = |coef, eta, k, an: Vec<T<'h>>, ap: Vec<T<'h>>, bm: Vec<T<'h>>, bq: Vec<T<'h>>| Rep { coef, eta, k, an, ap, bm, bq };
    // (A + B x^k)^-a
    if let Some(&[base, e]) = args_of(b, g, b.hd.pow) {
        // e^u is exp(u)
        if base == b.sym(h.sym("e")) {
            let nu = cx.eval(b.neg(e))?;
            let Some((c, k)) = monomial(b, nu, x) else { return Ok(None) };
            let c = cx.eval(c)?;
            return Ok(Some(rep(b.one(), c, k, vec![], vec![], vec![b.zero()], vec![])));
        }
        let Some(ts) = args_of(b, base, b.hd.add) else { return Ok(None) };
        if ts.len() != 2 || !is_free(b, e, x) {
            return Ok(None);
        }
        let (c0, t1) = if is_free(b, ts[0], x) { (ts[0], ts[1]) } else { (ts[1], ts[0]) };
        if !is_free(b, c0, x) {
            return Ok(None);
        }
        let Some((c1, k)) = monomial(b, t1, x) else { return Ok(None) };
        let a = cx.eval(b.neg(e))?;
        let eta = cx.eval(b.div(c1, c0))?;
        let coef = cx.eval(b.div(b.pow(c0, e), b.call("gamma", &[a])))?;
        let one_minus = cx.eval(b.sub(b.one(), a))?;
        return Ok(Some(rep(coef, eta, k, vec![one_minus], vec![], vec![b.zero()], vec![])));
    }
    let Some((name, args)) = call_of(b, g) else { return Ok(None) };
    let mono2 = |cx: &mut NativeCx<'_, '_, 'h>, u: T<'h>| -> Res<Option<(T<'h>, T<'h>)>> {
        let Some((c, k)) = monomial(b, u, x) else { return Ok(None) };
        Ok(Some((cx.eval(b.mul(&[quarter, c, c]))?, cx.eval(b.mul(&[b.int(2), k]))?)))
    };
    Ok(match (name.as_str(), args) {
        ("exp", [u]) => {
            let nu = cx.eval(b.neg(*u))?;
            let Some((c, k)) = monomial(b, nu, x) else { return Ok(None) };
            let c = cx.eval(c)?;
            Some(rep(b.one(), c, k, vec![], vec![], vec![b.zero()], vec![]))
        }
        ("bessel_j" | "bessel_k", [nu, u]) if is_free(b, *nu, x) => {
            let Some((eta, k)) = mono2(cx, *u)? else { return Ok(None) };
            let p = cx.eval(b.mul(&[half, *nu]))?;
            let m = cx.eval(b.mul(&[b.rat(-1, 2), *nu]))?;
            if name == "bessel_j" {
                Some(rep(b.one(), eta, k, vec![], vec![], vec![p], vec![m]))
            } else {
                Some(rep(half, eta, k, vec![], vec![], vec![p, m], vec![]))
            }
        }
        ("sin", [u]) => {
            let Some((eta, k)) = mono2(cx, *u)? else { return Ok(None) };
            Some(rep(sqrt_pi, eta, k, vec![], vec![], vec![half], vec![b.zero()]))
        }
        ("cos", [u]) => {
            let Some((eta, k)) = mono2(cx, *u)? else { return Ok(None) };
            Some(rep(sqrt_pi, eta, k, vec![], vec![], vec![b.zero()], vec![half]))
        }
        ("erfc", [u]) => {
            let Some((c, k)) = monomial(b, *u, x) else { return Ok(None) };
            let eta = cx.eval(b.mul(&[c, c]))?;
            let k = cx.eval(b.mul(&[b.int(2), k]))?;
            let coef = b.pow(b.sym(h.sym("pi")), b.rat(-1, 2));
            Some(rep(coef, eta, k, vec![], vec![b.one()], vec![b.zero(), half], vec![]))
        }
        _ => None,
    })
}

fn list<'h>(b: &Build<'h>, xs: &[T<'h>]) -> T<'h> {
    b.heap().call(b.hd.list, xs)
}

fn g_call<'h>(b: &Build<'h>, an: &[T<'h>], ap: &[T<'h>], bm: &[T<'h>], bq: &[T<'h>], z: T<'h>) -> T<'h> {
    b.call("meijer_g", &[list(b, &[list(b, an), list(b, ap)]), list(b, &[list(b, bm), list(b, bq)]), z])
}

/// The integral of `f` over `(lo, hi)` by the Meijer G method (module comment), checked numerically, or `None`.
pub(super) fn integrate<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, x: T<'h>, lo: Pt<'h>, hi: Pt<'h>) -> Res<Option<T<'h>>> {
    let (Pt::Fin(z0), Pt::PosInf) = (lo, hi) else { return Ok(None) };
    if z0 != b.zero() {
        return Ok(None);
    }
    let fs: Vec<T<'h>> = match args_of(b, f, b.hd.mul) {
        Some(fs) => fs.to_vec(),
        None => vec![f],
    };
    let mut consts = vec![];
    let mut alpha = vec![];
    let mut reps = vec![];
    for g in fs {
        if is_free(b, g, x) {
            consts.push(g);
        } else if let Some((c, d)) = monomial(b, g, x) {
            consts.push(c);
            alpha.push(d);
        } else {
            let Some(r) = represent(cx, b, g, x)? else { return Ok(None) };
            reps.push(r);
        }
    }
    if reps.is_empty() || reps.len() > 2 {
        return Ok(None);
    }
    // e^(-t) = pi^(-1/2) G^{2,0}_{0,2}(t^2/4 | 0, 1/2) (Gauss's duplication), so an exponential of x^k meets a
    // Bessel function of x^k, whose representation is in x^(2k)
    if reps.len() == 2 && reps[0].k != reps[1].k {
        for i in 0..2 {
            let other = reps[1 - i].k;
            let double = cx.eval(b.mul(&[b.int(2), reps[i].k]))?;
            let is_exp = reps[i].bm.len() == 1 && reps[i].bm[0] == b.zero() && reps[i].an.is_empty() && reps[i].ap.is_empty() && reps[i].bq.is_empty();
            if is_exp && double == other {
                let r = &mut reps[i];
                r.eta = cx.eval(b.mul(&[b.rat(1, 4), r.eta, r.eta]))?;
                r.k = double;
                r.coef = cx.eval(b.mul(&[r.coef, b.pow(b.sym(b.heap().sym("pi")), b.rat(-1, 2))]))?;
                r.bm = vec![b.zero(), b.rat(1, 2)];
                break;
            }
        }
    }
    let k = reps[0].k;
    if reps.iter().any(|r| r.k != k) || sign(cx, b, k)? != Some(1) {
        return Ok(None);
    }
    for r in &reps {
        if sign(cx, b, r.eta)? != Some(1) {
            return Ok(None);
        }
    }
    let alpha = cx.eval(b.add(&alpha))?;
    // u = x^k: integral of x^alpha F(x) dx = (1/k) integral of u^(beta - 1) F du, beta = (alpha + 1)/k
    let beta = cx.eval(b.div(b.add(&[alpha, b.one()]), k))?;
    let c = cx.eval(b.mul(&consts))?;
    let pre = cx.eval(b.mul(&[c, b.powi(k, -1)]))?;
    let value = if reps.len() == 1 {
        // the Mellin transform at beta: eta^-beta Phi(beta)
        let r = &reps[0];
        let gm = |t: T<'h>| b.call("gamma", &[t]);
        let mut num = vec![];
        let mut den = vec![];
        for &bj in &r.bm {
            num.push(gm(b.add(&[bj, beta])));
        }
        for &aj in &r.an {
            num.push(gm(b.sub(b.sub(b.one(), aj), beta)));
        }
        for &bj in &r.bq {
            den.push(gm(b.sub(b.sub(b.one(), bj), beta)));
        }
        for &aj in &r.ap {
            den.push(gm(b.add(&[aj, beta])));
        }
        b.mul(&[pre, r.coef, b.pow(r.eta, b.neg(beta)), b.div(b.mul(&num), b.mul(&den))])
    } else {
        let (r1, r2) = (&reps[0], &reps[1]);
        // u^(beta-1) G1(e1 u) = e1^(1-beta) G1'(e1 u), the parameters shifted by beta - 1
        let sh = cx.eval(b.sub(beta, b.one()))?;
        let shift = |cx: &mut NativeCx<'_, '_, 'h>, v: &[T<'h>]| -> Res<Vec<T<'h>>> { v.iter().map(|&t| cx.eval(b.add(&[t, sh]))).collect() };
        let (an1, ap1, bm1, bq1) = (shift(cx, &r1.an)?, shift(cx, &r1.ap)?, shift(cx, &r1.bm)?, shift(cx, &r1.bq)?);
        let negs = |cx: &mut NativeCx<'_, '_, 'h>, v: &[T<'h>]| -> Res<Vec<T<'h>>> { v.iter().map(|&t| cx.eval(b.neg(t))).collect() };
        // G2 = G^{mu,nu}_{sigma,tau}(e2 u | c; d): c = a's of G2, d = b's of G2
        let an: Vec<T<'h>> = an1.iter().copied().chain(negs(cx, &r2.bm)?).collect();
        let ap: Vec<T<'h>> = ap1.iter().copied().chain(negs(cx, &r2.bq)?).collect();
        let bm: Vec<T<'h>> = bm1.iter().copied().chain(negs(cx, &r2.an)?).collect();
        let bq: Vec<T<'h>> = bq1.iter().copied().chain(negs(cx, &r2.ap)?).collect();
        let ratio = cx.eval(b.div(r1.eta, r2.eta))?;
        let g = g_call(b, &an, &ap, &bm, &bq, ratio);
        b.mul(&[pre, r1.coef, r2.coef, b.pow(r1.eta, b.sub(b.one(), beta)), b.powi(r2.eta, -1), g])
    };
    let v = cx.eval(value)?;
    let h = b.heap();
    if super::is_failure(b, v) || !is_free(b, v, h.sym_term(h.sym("complex_infinity"))) {
        return Ok(None);
    }
    let v = apply(cx, b, "simplify", &[v])?;
    let inf = h.sym_term(h.sym("inf"));
    if check::verify(cx, b, f, x, b.zero(), inf, v, &[], false)? != Verdict::Confirmed {
        return Ok(None);
    }
    Ok(Some(v))
}
