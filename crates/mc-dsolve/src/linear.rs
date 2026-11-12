//! Linear ordinary differential equations `sum coefs[k] y^(k) = f`.
//!
//! - Constant coefficients, any order: a basis from the roots of the characteristic polynomial (a root of
//!   multiplicity m gives `x^j exp(r x)`, j < m; a complex pair `a ± b i` gives `exp(a x) cos(b x)` and
//!   `exp(a x) sin(b x)`). The constants are numbered in the order of the roots: for a binomial `a m^k + b` the k-th
//!   roots of `-b/a` from the principal one counterclockwise, otherwise ascending (real part, then imaginary part);
//!   the cosine of a pair takes the number of the root with the positive imaginary part.
//! - A particular solution: by undetermined coefficients when `f` is an exponential polynomial (sums of
//!   `x^k exp(a x) cos(b x)`, `sin`; the ansatz `x^s Q(x) exp(lambda x)` with s the multiplicity of lambda as a root,
//!   solved as a polynomial identity after the shift `L(D) (Q e^(lambda x)) = e^(lambda x) L(D + lambda) Q`), and by
//!   variation of parameters (the Wronskian system, any order) otherwise.
//! - Euler equations `sum a_k x^k y^(k) = f`, any order, through the indicial polynomial (`x^r log(x)^j`,
//!   `x^a cos(b log x)`).
//! - First order with variable coefficients: the integrating factor.
//! - Equations without `y, ..., y^(j-1)`: the order lowered by `u = y^(j)`, then j integrations.
//! - Second order with variable coefficients: a solution from an ansatz (`exp(lambda x)`, `x^r`, a polynomial up to
//!   degree 8) and the second by reduction of order `y2 = y1 integrate(exp(-integrate(p)) / y1^2)`; else a special
//!   function basis ([`super::special`]); else Kovacic's algorithm ([`super::kovacic`]).
use super::{combination, Cx, Ode};
use mc_calculus::common::is_free;
use mc_api::{names, Store, View, T};
use mc_eval::Res;

/// The coefficients `d(diff, y^(k))` and the right-hand side when `ode` is linear.
pub(crate) fn coefficients<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, ode: &Ode<'h>) -> Res<Option<(Vec<T<'h>>, T<'h>)>> {
    let b = c.b;
    let mut coefs = Vec::with_capacity(ode.n + 1);
    for &yk in &ode.ys {
        let ck = c.d(ode.diff, yk)?;
        let ck = c.simp(ck)?;
        if !c.free_all(ck, &ode.ys) {
            return Ok(None);
        }
        coefs.push(ck);
    }
    let zeros: Vec<T<'h>> = ode.ys.iter().map(|&v| c.rule(v, b.zero())).collect();
    let rest = c.subst(ode.diff, &zeros)?;
    let f = c.simp(b.neg(rest))?;
    Ok(Some((coefs, f)))
}

/// The general solution of `sum coefs[k] y^(k) = f`.
pub(crate) fn solve<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, coefs: &[T<'h>], f: T<'h>, x: T<'h>) -> Res<Option<T<'h>>> {
    let b = c.b;
    let n = coefs.len() - 1;
    if coefs[n] == b.zero() {
        return Ok(None);
    }
    let constant = coefs.iter().all(|&ck| is_free(b, ck, x));
    if constant {
        return constant_coefficients(c, coefs, f, x);
    }
    if n == 1 {
        return first_order_linear(c, coefs[0], coefs[1], f, x);
    }
    // without y: lower the order
    let j = coefs.iter().take_while(|&&ck| ck == b.zero()).count();
    if j >= 1 {
        return missing(c, coefs, j, f, x);
    }
    let Some(ys) = basis(c, coefs, x)? else { return Ok(None) };
    if ys.len() != n {
        return Ok(None);
    }
    let hom = combination(c, &ys, 1)?;
    if f == b.zero() {
        return Ok(Some(hom));
    }
    let g = b.div(f, coefs[n]);
    let Some(yp) = variation(c, &ys, g, x)? else { return Ok(None) };
    Ok(Some(c.eval(b.add(&[hom, yp]))?))
}

/// A basis of the homogeneous equation (variable coefficients).
pub(crate) fn basis<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, coefs: &[T<'h>], x: T<'h>) -> Res<Option<Vec<T<'h>>>> {
    let b = c.b;
    let n = coefs.len() - 1;
    if coefs.iter().all(|&ck| is_free(b, ck, x)) {
        let Some(rts) = char_roots(c, coefs)? else { return Ok(None) };
        return Ok(Some(exp_basis(c, &rts, x, false)?));
    }
    if let Some(ys) = euler(c, coefs, x)? {
        return Ok(Some(ys));
    }
    if n == 1 {
        let a = c.simp(b.neg(b.div(coefs[0], coefs[1])))?;
        let Some(ia) = c.integrate(a, x)? else { return Ok(None) };
        return Ok(Some(vec![c.simp(b.call("exp", &[ia]))?]));
    }
    if n != 2 {
        return Ok(None);
    }
    // the three methods for variable coefficients at once (roadmap item 16A): the first basis in this order is kept
    let found = c.par_until(3, &|r: &Option<Vec<T<'h>>>| r.is_some(), &|c, i| match i {
        0 => {
            c.trace("ansatz");
            let Some(y1) = ansatz(c, coefs, x)? else { return Ok(None) };
            Ok(reduction(c, coefs, y1, x)?.map(|y2| vec![y1, y2]))
        }
        _ if c.late() => Ok(None),
        1 => {
            c.trace("special functions");
            super::special::basis(c, coefs, x)
        }
        _ => {
            c.trace("kovacic");
            super::kovacic::basis(c, coefs, x)
        }
    })?;
    Ok(found.into_iter().last().flatten())
}

/// A root of the characteristic polynomial: real and imaginary parts (imaginary 0 for a real root), multiplicity.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Root<'h> {
    pub(crate) re: T<'h>,
    pub(crate) im: T<'h>,
    /// the sign of the imaginary part: 0 real, 1 positive, -1 negative
    pub(crate) sign: i8,
    pub(crate) mult: usize,
}

/// Does `t` look negative: a negative number, or a product with a negative numeric factor?
pub(crate) fn negative_like<'h>(c: &Cx<'_, '_, '_, '_, 'h>, t: T<'h>) -> bool {
    let b = c.b;
    if let Some(mc_arith::Num::Real(r)) = b.ar.num(t) {
        return r.to_f64() < 0.0;
    }
    let fs = c.factors(t);
    if fs.len() > 1 {
        return fs.iter().any(|&f| matches!(b.ar.num(f), Some(mc_arith::Num::Real(r)) if r.to_f64() < 0.0));
    }
    false
}

/// The roots of the characteristic polynomial with coefficients `coefs` (lowest first), in the order of the
/// constants (module documentation).
pub(crate) fn char_roots<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, coefs: &[T<'h>]) -> Res<Option<Vec<Root<'h>>>> {
    let b = c.b;
    let mut out = vec![];
    // the root 0
    let j = coefs.iter().take_while(|&&ck| ck == b.zero()).count();
    if j > 0 {
        out.push(Root { re: b.zero(), im: b.zero(), sign: 0, mult: j });
    }
    let q = &coefs[j..];
    let k = q.len() - 1;
    if k == 0 {
        return Ok(Some(out));
    }
    let nonzero = q.iter().filter(|&&t| t != b.zero()).count();
    if nonzero == 2 {
        // a binomial a m^k + b0: the k-th roots of s = -b0/a
        let s = c.simp(b.neg(b.div(q[0], q[k])))?;
        let se = c.expand(s)?;
        let neg = negative_like(c, se);
        let r0 = if neg { b.neg(s) } else { s };
        // power_expand for monomials (sqrt(w^2) = w); a sum keeps its principal root (power_expand would write
        // (1 - a)^(1/2) as i (a - 1)^(1/2))
        let r0 = if c.terms(se).len() > 1 { c.eval(b.pow(r0, b.rat(1, k as i64)))? } else { c.ev("power_expand", &[b.pow(r0, b.rat(1, k as i64))])? };
        let r0 = c.simp(r0)?;
        for i in 0..k {
            // the angle of the root: (2 pi i + (pi if s < 0)) / k
            let num = 2 * i as i64 + i64::from(neg);
            let ang = b.mul(&[b.rat(num, k as i64), c.sym("pi")]);
            let cs = c.ev("cos", &[ang])?;
            let sn = c.ev("sin", &[ang])?;
            let re = c.simp(b.mul(&[r0, cs]))?;
            let im = c.simp(b.mul(&[r0, sn]))?;
            let snv = c.ev("n", &[sn])?;
            let sign = match b.ar.num(snv) {
                Some(mc_arith::Num::Real(v)) if v.to_f64() > 1e-12 => 1,
                Some(mc_arith::Num::Real(v)) if v.to_f64() < -1e-12 => -1,
                _ => 0,
            };
            let im = if sign == 0 { b.zero() } else if sign < 0 { c.simp(b.neg(im))? } else { im };
            out.push(Root { re, im, sign, mult: 1 });
        }
        return Ok(Some(out));
    }
    if k == 2 {
        // a quadratic with a symbolic discriminant: a complex pair when it looks negative
        let disc = c.simp(b.sub(b.powi(q[1], 2), b.mul(&[b.int(4), q[0], q[2]])))?;
        let de = c.expand(disc)?;
        if b.ar.num(disc).is_none() && negative_like(c, de) {
            let re = c.simp(b.div(b.neg(q[1]), b.mul(&[b.int(2), q[2]])))?;
            let im = c.ev("power_expand", &[b.pow(b.neg(disc), b.rat(1, 2))])?;
            let im = c.simp(b.div(im, b.mul(&[b.int(2), q[2]])))?;
            out.push(Root { re, im, sign: -1, mult: 1 });
            out.push(Root { re, im, sign: 1, mult: 1 });
            return Ok(Some(out));
        }
    }
    let Some(rts) = roots(c, q)? else { return Ok(None) };
    // numeric values: to order the roots and pair complex conjugates
    let mut vals = vec![];
    for &(r, _) in &rts {
        let nv = c.ev("n", &[r])?;
        vals.push(match b.ar.num(nv) {
            Some(mc_arith::Num::Real(v)) => Some((v.to_f64(), 0.0)),
            Some(mc_arith::Num::Complex(a, bb)) => Some((a.to_f64(), bb.to_f64())),
            None => None,
        });
    }
    let mut idx: Vec<usize> = (0..rts.len()).collect();
    if vals.iter().all(|v| v.is_some()) {
        idx.sort_by(|&i, &j| {
            let (a, bb) = (vals[i].unwrap_or_default(), vals[j].unwrap_or_default());
            a.0.partial_cmp(&bb.0).unwrap_or(std::cmp::Ordering::Equal).then(a.1.partial_cmp(&bb.1).unwrap_or(std::cmp::Ordering::Equal))
        });
    }
    for &i in &idx {
        let (r, mult) = rts[i];
        let Some((rv, iv)) = vals[i] else {
            out.push(Root { re: r, im: b.zero(), sign: 0, mult });
            continue;
        };
        if iv.abs() <= 1e-12 * (1.0 + rv.abs()) {
            out.push(Root { re: r, im: b.zero(), sign: 0, mult });
            continue;
        }
        // the conjugate
        let Some(k) = (0..rts.len()).find(|&k| k != i && vals[k].is_some_and(|(a, bb)| (a - rv).abs() <= 1e-9 * (1.0 + rv.abs()) && (bb + iv).abs() <= 1e-9 * (1.0 + iv.abs()))) else {
            return Ok(None);
        };
        let s = rts[k].0;
        let (p, m) = if iv > 0.0 { (r, s) } else { (s, r) };
        let re = c.expand(b.mul(&[b.rat(1, 2), b.add(&[p, m])]))?;
        let i2 = b.call(names::COMPLEX, &[b.zero(), b.int(2)]);
        let im = c.expand(b.div(b.sub(p, m), i2))?;
        let re = c.simp(re)?;
        let im = c.simp(im)?;
        out.push(Root { re, im, sign: if iv > 0.0 { 1 } else { -1 }, mult });
    }
    Ok(Some(out))
}

/// The roots of the polynomial with coefficients `coefs` (lowest first) in a fresh variable, with multiplicities
/// (the order `roots` gives).
pub(crate) fn roots<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, coefs: &[T<'h>]) -> Res<Option<Vec<(T<'h>, usize)>>> {
    let b = c.b;
    let m = c.sym("d_solve·m");
    let terms: Vec<T<'h>> = coefs.iter().enumerate().map(|(k, &ck)| b.mul(&[ck, b.powi(m, k as i64)])).collect();
    let poly = b.add(&terms);
    let eq = b.call("equal", &[poly, b.zero()]);
    // root objects for cubics and quartics, not radicals
    let opts = b.call("options", &[c.rule(c.sym("cubics"), c.sym("false")), c.rule(c.sym("quartics"), c.sym("false"))]);
    let r = c.ev("roots", &[eq, m, opts])?;
    if c.stuck(r, "roots") {
        return Ok(None);
    }
    let h = b.heap();
    let mut ors = vec![];
    match h.view(r) {
        View::App { head, args } if b.sym_of(head) == Some(h.sym("or")) => ors.extend_from_slice(args),
        _ => ors.push(r),
    }
    // the variable bound in root objects renamed to a plain name free of the coefficients (it is bound, so a value
    // of that name does not matter)
    let plain = ["m", "z", "t", "s", "u"].iter().map(|n| c.sym(n)).find(|&v| coefs.iter().all(|&k| is_free(b, k, v)));
    let mut out: Vec<(T<'h>, usize)> = vec![];
    for e in ors {
        let View::App { args: &[_, v], .. } = h.view(e) else { return Ok(None) };
        let v = match plain {
            Some(p) if !is_free(b, v, m) => super::replace_terms(b, v, &[(m, p)]),
            _ => v,
        };
        match out.iter_mut().find(|(r, _)| *r == v) {
            Some(e) => e.1 += 1,
            None => out.push((v, 1)),
        }
    }
    if out.iter().map(|o| o.1).sum::<usize>() != coefs.len() - 1 {
        return Ok(None);
    }
    Ok(Some(out))
}

/// The basis from roots, in the order of the constants: `x^j exp(r x)`; a pair as `exp(a x) cos(b x)` (the root
/// with the positive imaginary part) and `exp(a x) sin(b x)` (its conjugate). For an Euler equation (`log`) the
/// variable of the exponentials is `log(x)`.
pub(crate) fn exp_basis<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, rts: &[Root<'h>], x: T<'h>, log: bool) -> Res<Vec<T<'h>>> {
    let b = c.b;
    let v = if log { b.call("log", &[x]) } else { x };
    let mut out = vec![];
    for r in rts {
        for j in 0..r.mult {
            let pw = b.powi(v, j as i64);
            let e = b.call("exp", &[b.mul(&[r.re, v])]);
            let t = match r.sign {
                0 => b.mul(&[pw, e]),
                1 => b.mul(&[pw, e, b.call("cos", &[b.mul(&[r.im, v])])]),
                _ => b.mul(&[pw, e, b.call("sin", &[b.mul(&[r.im, v])])]),
            };
            out.push(c.eval(t)?);
        }
    }
    Ok(out)
}

/// Linear with constant coefficients: `sum coefs[k] y^(k) = f`.
fn constant_coefficients<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, coefs: &[T<'h>], f: T<'h>, x: T<'h>) -> Res<Option<T<'h>>> {
    let b = c.b;
    let n = coefs.len() - 1;
    let Some(rts) = char_roots(c, coefs)? else { return Ok(None) };
    let ys = exp_basis(c, &rts, x, false)?;
    if ys.len() != n {
        return Ok(None);
    }
    let hom = combination(c, &ys, 1)?;
    if f == b.zero() {
        return Ok(Some(hom));
    }
    let lead = coefs[n];
    let mut yp = None;
    if n >= 2 {
        yp = undetermined(c, coefs, f, x)?;
    }
    if yp.is_none() {
        yp = variation(c, &ys, b.div(f, lead), x)?;
    }
    let Some(yp) = yp else { return Ok(None) };
    Ok(Some(c.eval(b.add(&[hom, yp]))?))
}

/// `t` as `(coefficient, lambda)` with `t = coefficient * exp(lambda x)`, the coefficient a polynomial in `x`.
fn exp_poly_term<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, t: T<'h>, x: T<'h>) -> Res<Option<(T<'h>, T<'h>)>> {
    let b = c.b;
    let h = b.heap();
    let mut lam = b.zero();
    let mut rest = vec![];
    for f in c.factors(t) {
        match h.view(f) {
            View::App { head, args: &[base, e] } if b.sym_of(head) == Some(b.hd.pow) && b.sym_of(base) == Some(b.hd.e) && !is_free(b, e, x) => {
                let l = c.d(e, x)?;
                if !is_free(b, l, x) {
                    return Ok(None);
                }
                lam = b.add(&[lam, l]);
                let k = c.simp(b.sub(e, b.mul(&[l, x])))?;
                if k != b.zero() {
                    rest.push(b.call("exp", &[k]));
                }
            }
            View::App { head, args: &[e] } if b.sym_of(head) == Some(h.sym("exp")) && !is_free(b, e, x) => {
                let l = c.d(e, x)?;
                if !is_free(b, l, x) {
                    return Ok(None);
                }
                lam = b.add(&[lam, l]);
                let k = c.simp(b.sub(e, b.mul(&[l, x])))?;
                if k != b.zero() {
                    rest.push(b.call("exp", &[k]));
                }
            }
            _ => rest.push(f),
        }
    }
    let coef = c.eval(b.mul(&rest))?;
    let cl = c.ev("coefficient_list", &[coef, x])?;
    let Some(items) = c.items(cl) else { return Ok(None) };
    if items.is_empty() || items.iter().any(|&t| !is_free(b, t, x)) {
        return Ok(None);
    }
    Ok(Some((coef, c.simp(lam)?)))
}

/// A particular solution by undetermined coefficients when `f` is an exponential polynomial.
fn undetermined<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, coefs: &[T<'h>], f: T<'h>, x: T<'h>) -> Res<Option<T<'h>>> {
    let b = c.b;
    let trig = !c.free(f, c.sym("sin")) || !c.free(f, c.sym("cos")) || !c.free(f, c.sym("sinh")) || !c.free(f, c.sym("cosh"));
    let g = if trig { c.ev("trig_to_exp", &[f])? } else { f };
    let g = c.expand(g)?;
    // group the terms by lambda
    let mut groups: Vec<(T<'h>, Vec<T<'h>>)> = vec![];
    for t in c.terms(g) {
        let Some((p, lam)) = exp_poly_term(c, t, x)? else { return Ok(None) };
        match groups.iter_mut().find(|gr| gr.0 == lam) {
            Some(gr) => gr.1.push(p),
            None => groups.push((lam, vec![p])),
        }
    }
    if groups.is_empty() || groups.len() > 8 {
        return Ok(None);
    }
    let n = coefs.len() - 1;
    let mut parts = vec![];
    for (lam, ps) in groups {
        if c.late() {
            return Ok(None);
        }
        let p = c.expand(b.add(&ps))?;
        let cl = c.ev("coefficient_list", &[p, x])?;
        let Some(pc) = c.items(cl) else { return Ok(None) };
        let d = pc.len().saturating_sub(1);
        // the multiplicity of lambda as a root of sum coefs[k] m^k
        let mut s = 0;
        let mut dcoefs = coefs.to_vec();
        while s < n {
            let terms: Vec<T<'h>> = dcoefs.iter().enumerate().map(|(k, &ck)| b.mul(&[ck, b.powi(lam, k as i64)])).collect();
            let v = c.eval(b.add(&terms))?;
            if !c.is_zero(v)? {
                break;
            }
            s += 1;
            // the derivative of the polynomial
            dcoefs = (1..dcoefs.len()).map(|k| b.mul(&[b.int(k as i64), dcoefs[k]])).collect();
            if dcoefs.is_empty() {
                break;
            }
        }
        let a: Vec<T<'h>> = (0..=d).map(|j| c.sym(&format!("d_solve·a·{j}"))).collect();
        let qterms: Vec<T<'h>> = (0..=d).map(|j| b.mul(&[a[j], b.powi(x, (j + s) as i64)])).collect();
        let q = c.eval(b.add(&qterms))?;
        // L(D + lambda) q
        let mut lq = vec![];
        let mut ders = vec![q];
        for i in 1..=n {
            let prev = ders[i - 1];
            ders.push(c.d(prev, x)?);
        }
        for (k, &ck) in coefs.iter().enumerate() {
            if ck == b.zero() {
                continue;
            }
            for (i, &di) in ders.iter().enumerate().take(k + 1) {
                let bin = binomial(k, i);
                lq.push(b.mul(&[ck, b.int(bin), b.powi(lam, (k - i) as i64), di]));
            }
        }
        let r = c.expand(b.sub(b.add(&lq), p))?;
        let rl = c.ev("coefficient_list", &[r, x])?;
        let Some(eqs) = c.items(rl) else { return Ok(None) };
        let eqs: Vec<T<'h>> = eqs.into_iter().map(|e| c.equal(e, b.zero())).collect();
        let sol = c.ev("solve", &[c.list(&eqs), c.list(&a)])?;
        let Some(first) = c.items(sol).and_then(|v| v.first().copied()) else { return Ok(None) };
        let Some(rules) = c.items(first) else { return Ok(None) };
        let qv = c.subst(q, &rules)?;
        if a.iter().any(|&ai| !c.free(qv, ai)) {
            return Ok(None);
        }
        parts.push(b.mul(&[qv, b.call("exp", &[b.mul(&[lam, x])])]));
    }
    let yp = c.eval(b.add(&parts))?;
    let yp = if trig {
        let t = c.ev("exp_to_trig", &[yp])?;
        c.expand(t)?
    } else {
        yp
    };
    Ok(Some(c.simp(yp)?))
}

fn binomial(n: usize, k: usize) -> i64 {
    let mut r: i64 = 1;
    for i in 0..k {
        r = r * (n - i) as i64 / (i + 1) as i64;
    }
    r
}

/// A particular solution of `y^(n) + ... = g` by variation of parameters over the basis `ys`.
pub(crate) fn variation<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, ys: &[T<'h>], g: T<'h>, x: T<'h>) -> Res<Option<T<'h>>> {
    let b = c.b;
    let n = ys.len();
    if n == 0 || n > 6 {
        return Ok(None);
    }
    let r = match ys {
        [y1] => {
            let q = c.simp(b.div(g, *y1))?;
            let Some(i) = c.integrate(q, x)? else { return Ok(None) };
            b.mul(&[*y1, i])
        }
        [y1, y2] => {
            let d1 = c.d(*y1, x)?;
            let d2 = c.d(*y2, x)?;
            let w = c.simp(b.sub(b.mul(&[*y1, d2]), b.mul(&[d1, *y2])))?;
            let a = c.simp(b.div(b.mul(&[*y2, g]), w))?;
            let Some(a) = c.integrate(a, x)? else { return Ok(None) };
            let e = c.simp(b.div(b.mul(&[*y1, g]), w))?;
            let Some(e) = c.integrate(e, x)? else { return Ok(None) };
            b.add(&[b.neg(b.mul(&[*y1, a])), b.mul(&[*y2, e])])
        }
        _ => {
            // the Wronskian system W u' = (0, ..., 0, g)
            let mut rows = vec![ys.to_vec()];
            for k in 1..n {
                let prev = rows[k - 1].clone();
                let mut row = vec![];
                for p in prev {
                    row.push(c.d(p, x)?);
                }
                rows.push(row);
            }
            let rl: Vec<T<'h>> = rows.iter().map(|r| c.list(r)).collect();
            let mut rhs = vec![b.zero(); n];
            rhs[n - 1] = g;
            let u = c.ev("linear_solve", &[c.list(&rl), c.list(&rhs)])?;
            let Some(us) = c.items(u) else { return Ok(None) };
            if us.len() != n {
                return Ok(None);
            }
            let mut terms = vec![];
            for (i, ui) in us.into_iter().enumerate() {
                let ui = c.simp(ui)?;
                let Some(iu) = c.integrate(ui, x)? else { return Ok(None) };
                terms.push(b.mul(&[ys[i], iu]));
            }
            b.add(&terms)
        }
    };
    Ok(Some(c.simp(r)?))
}

/// The basis of an Euler equation `sum a_k x^k y^(k) = 0` (a_k constants), `None` for other equations.
fn euler<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, coefs: &[T<'h>], x: T<'h>) -> Res<Option<Vec<T<'h>>>> {
    let b = c.b;
    let n = coefs.len() - 1;
    let mut a = vec![];
    for (k, &ck) in coefs.iter().enumerate() {
        let ak = c.simp(b.div(ck, b.powi(x, k as i64)))?;
        if !is_free(b, ak, x) {
            return Ok(None);
        }
        a.push(ak);
    }
    // the indicial polynomial sum a_k m (m - 1) ... (m - k + 1), as coefficients in m
    let m = c.sym("d_solve·m");
    let mut terms = vec![];
    for (k, &ak) in a.iter().enumerate() {
        let mut f = vec![ak];
        for i in 0..k {
            f.push(b.sub(m, b.int(i as i64)));
        }
        terms.push(b.mul(&f));
    }
    let poly = c.expand(b.add(&terms))?;
    let cl = c.ev("coefficient_list", &[poly, m])?;
    let Some(ind) = c.items(cl) else { return Ok(None) };
    if ind.len() != n + 1 {
        return Ok(None);
    }
    // ascending order, without the binomial rule of constant coefficients
    let Some(rts) = roots(c, &ind)? else { return Ok(None) };
    let mut list = vec![];
    let mut vals = vec![];
    for &(r, mult) in &rts {
        let nv = c.ev("n", &[r])?;
        vals.push(match b.ar.num(nv) {
            Some(mc_arith::Num::Real(v)) => (v.to_f64(), 0.0),
            Some(mc_arith::Num::Complex(p, q)) => (p.to_f64(), q.to_f64()),
            None => (f64::NAN, 0.0),
        });
        list.push((r, mult));
    }
    let mut idx: Vec<usize> = (0..list.len()).collect();
    if vals.iter().all(|v| v.0.is_finite()) {
        idx.sort_by(|&i, &j| vals[i].0.partial_cmp(&vals[j].0).unwrap_or(std::cmp::Ordering::Equal).then(vals[i].1.partial_cmp(&vals[j].1).unwrap_or(std::cmp::Ordering::Equal)));
    }
    let mut out = vec![];
    for &i in &idx {
        let (r, mult) = list[i];
        let (rv, iv) = vals[i];
        if iv.abs() <= 1e-12 * (1.0 + rv.abs()) || !iv.is_finite() {
            out.push(Root { re: r, im: b.zero(), sign: 0, mult });
            continue;
        }
        let Some(k) = (0..list.len()).find(|&k| k != i && (vals[k].0 - rv).abs() <= 1e-9 * (1.0 + rv.abs()) && (vals[k].1 + iv).abs() <= 1e-9 * (1.0 + iv.abs())) else {
            return Ok(None);
        };
        let s = list[k].0;
        let (p, q) = if iv > 0.0 { (r, s) } else { (s, r) };
        let re = c.simp(b.mul(&[b.rat(1, 2), b.add(&[p, q])]))?;
        let i2 = b.call(names::COMPLEX, &[b.zero(), b.int(2)]);
        let im = c.simp(b.div(b.sub(p, q), i2))?;
        out.push(Root { re, im, sign: if iv > 0.0 { 1 } else { -1 }, mult });
    }
    Ok(Some(exp_basis(c, &out, x, true)?))
}

/// `c1 y' + c0 y = f` with variable coefficients: the integrating factor.
fn first_order_linear<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, c0: T<'h>, c1: T<'h>, f: T<'h>, x: T<'h>) -> Res<Option<T<'h>>> {
    let b = c.b;
    let a = c.simp(b.neg(b.div(c0, c1)))?;
    let q = c.simp(b.div(f, c1))?;
    let Some(ia) = c.integrate(a, x)? else { return Ok(None) };
    let mu = c.simp(b.call("exp", &[ia]))?;
    let inner = if q == b.zero() {
        c.constant(1)
    } else {
        let w = c.simp(b.div(q, mu))?;
        let Some(i) = c.integrate(w, x)? else { return Ok(None) };
        b.add(&[i, c.constant(1)])
    };
    Ok(Some(c.expand(b.mul(&[mu, inner]))?))
}

/// `y, ..., y^(j-1)` absent: `u = y^(j)` solved, then integrated j times (constants after those of u).
fn missing<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, coefs: &[T<'h>], j: usize, f: T<'h>, x: T<'h>) -> Res<Option<T<'h>>> {
    let b = c.b;
    let n = coefs.len() - 1;
    let Some(mut u) = solve(c, &coefs[j..], f, x)? else { return Ok(None) };
    for i in 0..j {
        let Some(iu) = c.integrate(u, x)? else { return Ok(None) };
        u = c.eval(b.add(&[iu, c.constant(n - j + i + 1)]))?;
    }
    Ok(Some(c.expand(u)?))
}

/// A solution of the second-order homogeneous equation from an ansatz: `exp(lambda x)`, `x^r`, a polynomial.
fn ansatz<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, coefs: &[T<'h>], x: T<'h>) -> Res<Option<T<'h>>> {
    let b = c.b;
    let l = c.sym("d_solve·l");
    let apply = |c: &mut Cx<'_, '_, '_, '_, 'h>, y: T<'h>| -> Res<T<'h>> {
        let d1 = c.d(y, x)?;
        let d2 = c.d(d1, x)?;
        c.eval(b.add(&[b.mul(&[coefs[0], y]), b.mul(&[coefs[1], d1]), b.mul(&[coefs[2], d2])]))
    };
    // exp(l x) and x^l: l from the residual, kept when free of x and the residual vanishes
    for cand in [b.call("exp", &[b.mul(&[l, x])]), b.pow(x, l)] {
        if c.late() {
            return Ok(None);
        }
        let r = apply(c, cand)?;
        let r = c.simp(b.div(r, cand))?;
        let r = c.together(r)?;
        let num = c.ev("numerator", &[r])?;
        let num = c.expand(num)?;
        let cl = c.ev("coefficient_list", &[num, x])?;
        let Some(eqs) = c.items(cl) else { continue };
        if eqs.is_empty() || eqs.iter().any(|&e| !is_free(b, e, x)) {
            continue;
        }
        let eqs: Vec<T<'h>> = eqs.into_iter().filter(|&e| e != b.zero()).map(|e| c.equal(e, b.zero())).collect();
        if eqs.is_empty() {
            continue;
        }
        let Some(vals) = c.solve_for(c.list(&eqs), l)? else { continue };
        for v in vals {
            if !is_free(b, v, x) {
                continue;
            }
            let y = c.subst(cand, &[c.rule(l, v)])?;
            let y = c.simp(y)?;
            if y == b.zero() || is_free(b, y, x) && v != b.zero() {
                continue;
            }
            let r = apply(c, y)?;
            if c.is_zero(r)? {
                return Ok(Some(y));
            }
        }
    }
    // polynomials of degree 1..8 (monic)
    for d in 1..=8usize {
        if c.late() {
            return Ok(None);
        }
        let a: Vec<T<'h>> = (0..d).map(|j| c.sym(&format!("d_solve·p·{j}"))).collect();
        let mut terms: Vec<T<'h>> = (0..d).map(|j| b.mul(&[a[j], b.powi(x, j as i64)])).collect();
        terms.push(b.powi(x, d as i64));
        let p = c.eval(b.add(&terms))?;
        let r = apply(c, p)?;
        let r = c.together(r)?;
        let num = c.ev("numerator", &[r])?;
        let num = c.expand(num)?;
        let cl = c.ev("coefficient_list", &[num, x])?;
        let Some(eqs) = c.items(cl) else { continue };
        if eqs.iter().any(|&e| !is_free(b, e, x)) {
            continue;
        }
        let eqs: Vec<T<'h>> = eqs.into_iter().filter(|&e| e != b.zero()).map(|e| c.equal(e, b.zero())).collect();
        if eqs.is_empty() {
            return Ok(Some(p));
        }
        let s = c.ev("solve", &[c.list(&eqs), c.list(&a)])?;
        let Some(first) = c.items(s).and_then(|v| v.first().copied()) else { continue };
        let Some(rules) = c.items(first) else { continue };
        let y = c.subst(p, &rules)?;
        if a.iter().any(|&ai| !c.free(y, ai)) {
            continue;
        }
        let r = apply(c, y)?;
        if c.is_zero(r)? {
            return Ok(Some(c.expand(y)?));
        }
    }
    Ok(None)
}

/// The second solution from `y1`: `y1 integrate(exp(-integrate(p)) / y1^2)`.
pub(crate) fn reduction<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, coefs: &[T<'h>], y1: T<'h>, x: T<'h>) -> Res<Option<T<'h>>> {
    let b = c.b;
    let p = c.simp(b.div(coefs[1], coefs[2]))?;
    let Some(ip) = c.integrate(p, x)? else { return Ok(None) };
    let w = c.simp(b.call("exp", &[b.neg(ip)]))?;
    let q = c.simp(b.div(w, b.powi(y1, 2)))?;
    let Some(iq) = c.integrate(q, x)? else { return Ok(None) };
    let y2 = c.simp(b.mul(&[y1, iq]))?;
    if is_free(b, y2, x) {
        return Ok(None);
    }
    Ok(Some(y2))
}
