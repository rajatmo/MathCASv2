//! `root_sum(x => p(x), y => g(y))`: the sum of `g` over the roots of `p`, counted with multiplicity.
//!
//! Evaluation (decision F1.3):
//!
//! - inexact coefficients (what `n` makes of the term): the numeric sum `g(root(p, 1)) + ... + g(root(p, d))`;
//! - exact coefficients: `p` is factored over Q. Linear factors give `g(r)`, quadratic factors `g(r1) + g(r2)` with
//!   radicals. A factor of degree 3 or more is summed exactly when `g(y)` is a rational function of `y` and at most
//!   one other kernel `x`: then the sum is a trace in `Q(β)` (no other kernel), or a rational function of `x`
//!   found by evaluating the traces at integer points and interpolating `P = S Res_y(f, D)`, with one extra point
//!   as a check; otherwise it stays `root_sum` over that irreducible factor, the polynomial written primitive with
//!   integer coefficients. So `d` of an antiderivative written with `root_sum` gives the integrand back.
//! - `to_radicals` writes the sums over cubic and quartic factors with the radicals `solve` gives.
use crate::cx::{Env, Out};
use crate::convert::{poly_term, rational_coefficients};
use crate::field::NumberField;
use crate::qpoly::{self, q, qq, QPoly};
use mc_api::{Store, View, T};
use mc_arith::num::Rat;
use mc_arith::{Num, Real};
use mc_poly::convert::Ring;
use mc_poly::ratfun::{self, RatFun};
use mc_poly::work::{Meter, WorkResult};
use mc_poly::{Poly, Z};

/// The most interpolation points used for a sum depending on another kernel.
const MAX_POINTS: usize = 4000;

/// `root_sum(f, g)`.
pub(crate) fn root_sum<'h>(env: &Env<'h>, args: &[T<'h>], m: &mut Meter) -> WorkResult<Out<'h>> {
    let &[f, g] = args else { return Ok(env.arg_failure("root_sum takes a polynomial function and a function")) };
    let Some((x, body)) = env.function_parts(f) else { return Ok(Out::Decline) };
    // the polynomial: exact rational, or with inexact numbers (numeric evaluation), else unevaluated
    let c = env.conv();
    let (ring, _) = c.ring(&[body], &[x], m)?;
    let others: Vec<T<'h>> = ring.kernels.iter().copied().filter(|&k| k != x).collect();
    if !others.is_empty() {
        if others.iter().all(|&k| env.ar.num(k).is_some()) {
            return numeric(env, f, x, body, g, m);
        }
        // parameters: power sums by Newton's identities when the summand is a polynomial in the root
        return symbolic_power_sums(env, x, body, g, m);
    }
    let Some(p) = rational_coefficients(env, x, body, m)? else { return Ok(Out::Decline) };
    let Some(d) = qpoly::deg(&p) else { return Ok(Out::Done(env.int(0))) };
    if d == 0 {
        return Ok(Out::Done(env.int(0)));
    }
    let fs = qpoly::factor(&p, m)?;
    let mut parts: Vec<T<'h>> = vec![];
    let mut changed = !(fs.len() == 1 && fs[0].1 == 1);
    for (fac, e) in &fs {
        let mult = env.int(*e as i64);
        match fac.len() - 1 {
            1 => {
                let r = env.ar.number(&Num::rat(fac[0].neg()));
                parts.push(env.ar.mul(&[mult, apply(env, g, r, m)?]));
                changed = true;
            }
            2 => {
                for r in quadratic_roots(env, fac) {
                    parts.push(env.ar.mul(&[mult, apply(env, g, r, m)?]));
                }
                changed = true;
            }
            _ => match exact_sum(env, fac, g, m)? {
                Some(s) => {
                    parts.push(env.ar.mul(&[mult, s]));
                    changed = true;
                }
                None => {
                    let pt = poly_term(env, &qpoly::from_ints(&qpoly::primitive(fac)), x);
                    if pt != body {
                        changed = true;
                    }
                    let rs = env.call("root_sum", &[env.function(x, pt), g]);
                    parts.push(env.ar.mul(&[mult, rs]));
                }
            },
        }
    }
    if !changed {
        return Ok(Out::Decline);
    }
    Ok(Out::Again(env.ar.add(&parts)))
}

/// `g(r)`: the body with the parameter replaced, or the call `g(r)`.
pub(crate) fn apply<'h>(env: &Env<'h>, g: T<'h>, r: T<'h>, m: &mut Meter) -> WorkResult<T<'h>> {
    match env.function_parts(g) {
        Some((y, body)) => env.subst(body, y, r, m),
        None => Ok(env.h().app(g, &[r])),
    }
}

/// The two roots `(-b +- sqrt(b^2 - 4c)) / 2` of a monic quadratic as terms.
fn quadratic_roots<'h>(env: &Env<'h>, f: &[Rat]) -> [T<'h>; 2] {
    let (c, b) = (&f[0], &f[1]);
    let half_b = b.mul(&qq(-1, 2));
    let disc = b.mul(b).mul(&qq(1, 4)).sub(c);
    let sq = env.ar.pow(env.ar.number(&Num::rat(disc)), env.ar.number(&Num::rat(qq(1, 2))));
    let u = env.ar.number(&Num::rat(half_b));
    [env.ar.add(&[u, env.ar.neg(sq)]), env.ar.add(&[u, sq])]
}

/// The numeric sum over `root(f, k)`.
fn numeric<'h>(env: &Env<'h>, f: T<'h>, x: T<'h>, body: T<'h>, g: T<'h>, m: &mut Meter) -> WorkResult<Out<'h>> {
    let c = env.conv();
    let (ring, shared) = c.ring(&[body], &[x], m)?;
    let p = c.to_poly(body, &ring, &shared, m)?;
    let Some(v) = ring.var(x) else { return Ok(Out::Decline) };
    let d = p.num.degree(v).unwrap_or(0);
    if d > 4000 {
        return Err(mc_poly::work::Halt::Memory);
    }
    let mut parts = Vec::with_capacity(d as usize);
    for k in 1..=d {
        let r = env.call("root", &[f, env.int(k as i64)]);
        parts.push(apply(env, g, r, m)?);
    }
    // a real polynomial and a function without complex constants: conjugate roots give conjugate values (away
    // from branch cuts, where the imaginary parts do not cancel and stay visible)
    let real = !has_complex(env, body) && !has_complex(env, g);
    Ok(Out::Numeric(env.ar.add(&parts), real))
}

/// Does `t` contain a non-real number?
fn has_complex<'h>(env: &Env<'h>, t: T<'h>) -> bool {
    let h = env.h();
    let mut stack = vec![t];
    let mut seen = rustc_hash::FxHashSet::default();
    while let Some(u) = stack.pop() {
        if !seen.insert(u) {
            continue;
        }
        if let Some(Num::Complex(..)) = env.ar.num(u) {
            return true;
        }
        if let View::App { head, args } = h.view(u) {
            stack.push(head);
            stack.extend_from_slice(args);
        }
    }
    false
}

/// The real part of a complex number whose imaginary part is a ball containing zero or a machine number below
/// `10^-12` of the size of the value; `None` otherwise.
pub(crate) fn real_part_if_zero_imaginary<'h>(env: &Env<'h>, v: T<'h>) -> Option<T<'h>> {
    let Some(Num::Complex(re, im)) = env.ar.num(v) else { return None };
    let zero = match &im {
        Real::Ball(b, _) => b.contains_zero(),
        Real::Machine(x) => x.abs() <= 1e-12 * re.to_f64().abs().max(1.0),
        Real::Exact(c) => c.is_zero(),
    };
    zero.then(|| env.ar.number(&Num::Real(re)))
}

/// The exact sum of `g` over the roots of the monic irreducible `f` (degree 3 or more) when `g(y)` is a rational
/// function of `y` and at most one other kernel.
fn exact_sum<'h>(env: &Env<'h>, f: &[Rat], g: T<'h>, m: &mut Meter) -> WorkResult<Option<T<'h>>> {
    let Some((y, body)) = env.function_parts(g) else { return Ok(None) };
    let c = env.rconv();
    let (ring, _) = c.ring(&[body], &[y], m)?;
    let others: Vec<T<'h>> = ring.kernels.iter().copied().filter(|&k| k != y).collect();
    if others.len() > 1 || others.iter().any(|&k| env.contains(k, y) || env.ar.num(k).is_some()) {
        return Ok(None);
    }
    let Ok(rf) = ratfun::read(&c, body, &ring, m)? else { return Ok(None) };
    let k = NumberField::new(f.to_vec(), None);
    let Some(yi) = ring.var(y) else { return Ok(None) };
    match others.first() {
        None => {
            let num = dense(&rf.num, yi);
            let den = dense(&rf.den, yi);
            let Some(v) = k.div(&k.reduce(&num), &k.reduce(&den)) else { return Ok(None) };
            Ok(Some(env.ar.number(&Num::rat(k.trace(&v)))))
        }
        Some(&xk) => {
            let xi = ring.var(xk).expect("a kernel of the ring");
            interpolated_sum(env, &k, &rf, yi, xi, xk, m)
        }
    }
}

/// The dense coefficients in variable `v` of a polynomial in that variable only.
fn dense(p: &Poly, v: usize) -> QPoly {
    qpoly::trim(p.to_dense(v).iter().map(|z| Rat::from_int(z.to_int())).collect())
}

/// `p(y, x0)` as a polynomial in `y` (the ring has the two variables `yi`, `xi`).
fn at_x(p: &Poly, yi: usize, xi: usize, x0: &Rat) -> QPoly {
    let n = p.degree(yi).map_or(0, |d| d as usize + 1);
    let mut out = vec![q(0); n];
    for (e, c) in p.terms() {
        let v = Rat::from_int(c.to_int()).mul(&x0.pow_u64(e[xi] as u64));
        out[e[yi] as usize] = out[e[yi] as usize].add(&v);
    }
    qpoly::trim(out)
}

/// `S(x) = sum N(β, x) / D(β, x)` by interpolation.
fn interpolated_sum<'h>(
    env: &Env<'h>,
    k: &NumberField,
    rf: &RatFun,
    yi: usize,
    xi: usize,
    xk: T<'h>,
    m: &mut Meter,
) -> WorkResult<Option<T<'h>>> {
    let n = k.degree();
    let dn = rf.num.degree(xi).unwrap_or(0) as usize;
    let dd = rf.den.degree(xi).unwrap_or(0) as usize;
    // deg P <= dn + (n - 1) dd, deg Res <= n dd
    let need = (dn + (n - 1) * dd).max(n * dd) + 1;
    if need + 1 > MAX_POINTS {
        return Ok(None);
    }
    let (mut xs, mut ps, mut rs) = (vec![], vec![], vec![]);
    let mut x0 = 0i64;
    while xs.len() < need + 1 {
        m.step((n * n) as u64 + 1)?;
        let xr = q(x0);
        x0 += 1;
        if x0 > (4 * MAX_POINTS) as i64 {
            return Ok(None);
        }
        let den = k.reduce(&at_x(&rf.den, yi, xi, &xr));
        if den.is_empty() {
            continue;
        }
        let num = k.reduce(&at_x(&rf.num, yi, xi, &xr));
        let Some(v) = k.div(&num, &den) else { continue };
        let nd = k.norm(&den, m)?;
        xs.push(xr);
        ps.push(k.trace(&v).mul(&nd));
        rs.push(nd);
    }
    // the last point checks the interpolants
    let pp = interpolate(&xs[..need], &ps[..need]);
    let rr = interpolate(&xs[..need], &rs[..need]);
    let last = need;
    if qpoly::eval(&pp, &xs[last]) != ps[last] || qpoly::eval(&rr, &xs[last]) != rs[last] {
        return Ok(None);
    }
    // P / R in lowest terms
    let g = qpoly::gcd(&pp, &rr, m)?;
    let (num, _) = qpoly::divrem(&pp, &g, m)?;
    let (den, _) = qpoly::divrem(&rr, &g, m)?;
    // as one fraction with integer coefficients
    let ring = ring_of(env, xk, m)?;
    let lc = den.last().cloned().unwrap_or_else(|| q(1));
    let num = qpoly::scale(&num, &lc.recip().unwrap_or_else(|| q(1)));
    let den = qpoly::monic(&den);
    let (cn, ni) = qpoly::clear(&num);
    let (cd, di) = qpoly::clear(&den);
    // num/den = (ni / cn) / (di / cd) = (ni cd) / (di cn)
    let zn: Vec<Z> = ni.iter().map(|c| Z::from_int(c.mul(&mc_arith::num::Int::from_nat(cd.clone())))).collect();
    let zd: Vec<Z> = di.iter().map(|c| Z::from_int(c.mul(&mc_arith::num::Int::from_nat(cn.clone())))).collect();
    let f = RatFun { num: Poly::from_dense(1, 0, &zn), den: Poly::from_dense(1, 0, &zd) };
    let f = lowest(f, m)?;
    Ok(Some(ratfun::to_term(&env.rconv(), &f, &ring, m)?))
}

/// The one-variable ring of the kernel `x`.
fn ring_of<'h>(env: &Env<'h>, x: T<'h>, m: &mut Meter) -> WorkResult<Ring<'h>> {
    Ok(env.rconv().ring(&[x], &[x], m)?.0)
}

/// A fraction of integer polynomials with the integer content cancelled and a positive denominator.
fn lowest(f: RatFun, _m: &mut Meter) -> WorkResult<RatFun> {
    let (cn, pn) = f.num.primitive();
    let (cd, pd) = f.den.primitive();
    let g = cn.gcd(&cd);
    let (cn, cd) = (cn.div_exact(&g), cd.div_exact(&g));
    let (cn, cd) = if cd.is_neg() { (cn.neg(), cd.neg()) } else { (cn, cd) };
    Ok(RatFun { num: pn.scale(&cn), den: pd.scale(&cd) })
}

/// The polynomial through `(xs[i], ys[i])` (Newton's divided differences).
fn interpolate(xs: &[Rat], ys: &[Rat]) -> QPoly {
    let n = xs.len();
    let mut dd = ys.to_vec();
    for j in 1..n {
        for i in (j..n).rev() {
            dd[i] = qpoly::rdiv(&dd[i].sub(&dd[i - 1]), &xs[i].sub(&xs[i - j]));
        }
    }
    let mut p: QPoly = vec![];
    for i in (0..n).rev() {
        let mut np = vec![q(0); p.len() + 1];
        for (kk, c) in p.iter().enumerate() {
            np[kk + 1] = np[kk + 1].add(c);
            np[kk] = np[kk].sub(&c.mul(&xs[i]));
        }
        np[0] = np[0].add(&dd[i]);
        p = qpoly::trim(np);
    }
    p
}

/// `to_radicals(e)`: the root sums over irreducible cubics and quartics in `e` replaced by sums over the roots
/// `roots` gives (the caller evaluates the `solve` calls first and passes their results).
pub(crate) fn radical_sum<'h>(
    env: &Env<'h>,
    g: T<'h>,
    roots: &[T<'h>],
    m: &mut Meter,
) -> WorkResult<T<'h>> {
    let mut parts = vec![];
    for &r in roots {
        parts.push(apply(env, g, r, m)?);
    }
    Ok(env.ar.add(&parts))
}

/// The irreducible polynomial and function of a `root_sum` term of degree 3 or 4 with rational coefficients.
pub(crate) fn radical_candidate<'h>(env: &Env<'h>, t: T<'h>, m: &mut Meter) -> WorkResult<Option<(T<'h>, T<'h>, T<'h>)>> {
    let Some(&[f, g]) = env.args_of(t, "root_sum") else { return Ok(None) };
    let Some((x, body)) = env.function_parts(f) else { return Ok(None) };
    let Some(p) = rational_coefficients(env, x, body, m)? else { return Ok(None) };
    if !matches!(qpoly::deg(&p), Some(3 | 4)) {
        return Ok(None);
    }
    Ok(Some((x, body, g)))
}

/// The degree in `v` and the coefficient terms `coefficient(e, v, i)` of a polynomial `e` in `v` whose other kernels
/// are free of `v`; `None` otherwise.
fn symbolic_coefficients<'h>(env: &Env<'h>, v: T<'h>, e: T<'h>, m: &mut Meter) -> WorkResult<Option<Vec<T<'h>>>> {
    let c = env.conv();
    let (ring, shared) = c.ring(&[e], &[v], m)?;
    if ring.kernels.iter().any(|&k| k != v && env.contains(k, v)) {
        return Ok(None);
    }
    let p = c.to_poly(e, &ring, &shared, m)?;
    let Some(vi) = ring.var(v) else { return Ok(None) };
    let n = p.num.degree(vi).unwrap_or(0) as usize;
    if n > 64 {
        return Ok(None);
    }
    Ok(Some((0..=n).map(|i| env.call("coefficient", &[e, v, env.int(i as i64)])).collect()))
}

/// `root_sum` over a polynomial with parameters and a summand polynomial in the root: `sum_j g_j P_j` with the power
/// sums `P_k` of the roots from Newton's identities, as one fraction (the coefficients are `coefficient` calls the
/// evaluator works out).
fn symbolic_power_sums<'h>(env: &Env<'h>, x: T<'h>, body: T<'h>, g: T<'h>, m: &mut Meter) -> WorkResult<Out<'h>> {
    let Some((y, gbody)) = env.function_parts(g) else { return Ok(Out::Decline) };
    let (Some(cs), Some(gs)) = (symbolic_coefficients(env, x, body, m)?, symbolic_coefficients(env, y, gbody, m)?) else {
        return Ok(Out::Decline);
    };
    let n = cs.len() - 1;
    if n == 0 {
        return Ok(Out::Done(env.int(0)));
    }
    // a_i = c_i / c_n
    let inv = env.ar.pow(cs[n], env.int(-1));
    let a: Vec<T<'h>> = cs.iter().map(|&c| env.ar.mul(&[c, inv])).collect();
    let mut p: Vec<T<'h>> = vec![env.int(n as i64)];
    for k in 1..gs.len() {
        m.step(k as u64)?;
        let mut terms = vec![];
        for i in 1..=k.min(n) {
            if i < k {
                terms.push(env.ar.mul(&[a[n - i], p[k - i]]));
            } else {
                terms.push(env.ar.mul(&[env.int(k as i64), a[n - k]]));
            }
        }
        p.push(env.ar.neg(env.ar.add(&terms)));
    }
    let sum: Vec<T<'h>> = gs.iter().zip(&p).map(|(&gj, &pj)| env.ar.mul(&[gj, pj])).collect();
    Ok(Out::Again(env.call("together", &[env.ar.add(&sum)])))
}
