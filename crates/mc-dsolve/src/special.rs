//! Special-function bases of second-order linear homogeneous equations `c2 y'' + c1 y' + c0 y = 0`, recognised
//! from the normalised coefficients `p = c1/c2`, `q = c0/c2`:
//!
//! - Airy: `y'' = (a x + b) y`: `airy_ai(t)`, `airy_bi(t)` with `t = a^(1/3) (x + b/a)`.
//! - Bessel (Bowman's form): `x^2 y'' + (1 - 2 alpha) x y' + (beta^2 gamma^2 x^(2 gamma) + alpha^2 - nu^2 gamma^2) y
//!   = 0`: `x^alpha bessel_j(nu, beta x^gamma)`, `x^alpha bessel_y(nu, beta x^gamma)` (`bessel_i`, `bessel_k` when
//!   `beta^2` is negative).
//! - Gauss: `x (1 - x) y'' + (c - (a + b + 1) x) y' - a b y = 0`: `hypergeometric_2f1(a, b, c, x)` and
//!   `x^(1 - c) hypergeometric_2f1(a - c + 1, b - c + 1, 2 - c, x)` (the second by reduction of order when c is an
//!   integer).
//! - Legendre: `(1 - x^2) y'' - 2 x y' + nu (nu + 1) y = 0`: `legendre_p(nu, x)`, `legendre_q(nu, x)`.
//! - Kummer: `x y'' + (b - x) y' - a y = 0`: `hypergeometric_1f1(a, b, x)`, `hypergeometric_u(a, b, x)`.
//! - Hermite: `y'' - 2 x y' + 2 nu y = 0`: `hermite_h(nu, x)`, `hypergeometric_1f1(-nu/2, 1/2, x^2)`.
//!
//! Literature: F. Bowman, *Introduction to Bessel Functions* (1938), section 3.6; NIST DLMF chapters 9, 10, 13, 14,
//! 15, 18.
use super::Cx;
use mc_calculus::common::is_free;
use mc_api::{Store, T};
use mc_eval::Res;

/// A special-function basis, if the equation is one of the recognised forms.
pub(crate) fn basis<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, coefs: &[T<'h>], x: T<'h>) -> Res<Option<Vec<T<'h>>>> {
    let b = c.b;
    if coefs.len() != 3 {
        return Ok(None);
    }
    let p = c.simp(b.div(coefs[1], coefs[2]))?;
    let q = c.simp(b.div(coefs[0], coefs[2]))?;
    if let Some(v) = airy(c, p, q, x)? {
        return Ok(Some(v));
    }
    if let Some(v) = bessel(c, p, q, x)? {
        return Ok(Some(v));
    }
    if let Some(v) = gauss(c, coefs, x)? {
        return Ok(Some(v));
    }
    if let Some(v) = legendre(c, coefs, x)? {
        return Ok(Some(v));
    }
    if let Some(v) = kummer(c, coefs, x)? {
        return Ok(Some(v));
    }
    if let Some(v) = mathieu(c, p, q, x)? {
        return Ok(Some(v));
    }
    if let Some(v) = heun_confluent(c, p, q, x)? {
        return Ok(Some(v));
    }
    hermite(c, p, q, x)
}

/// Coefficients `[c0, ..., cn]` of `t * x^shift` as a polynomial of degree at most `n` in `x`, `None` when it is
/// not one.
fn poly_coefs<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, t: T<'h>, x: T<'h>, shift: i64, n: i64) -> Res<Option<Vec<T<'h>>>> {
    let b = c.b;
    let e = c.expand(b.mul(&[t, b.powi(x, shift)]))?;
    let mut out = vec![];
    let mut back = vec![];
    for i in 0..=n {
        let k = c.ev("coefficient", &[e, x, b.int(i)])?;
        if !c.free(k, x) || c.bad(k) {
            return Ok(None);
        }
        back.push(b.mul(&[k, b.powi(x, i)]));
        out.push(k);
    }
    let diff = b.sub(e, b.add(&back));
    Ok(if c.is_zero(diff)? { Some(out) } else { None })
}

/// Is `g` a number that is an integer (the second solution of the equation is then not independent or not defined)?
fn integer_valued<'h>(c: &Cx<'_, '_, '_, '_, 'h>, g: T<'h>) -> bool {
    matches!(c.b.heap().view(g), mc_api::View::Int(_))
}

/// The confluent Heun equations with their second solutions.
///
/// - biconfluent `y'' + (g/z + d + e z) y' + (a z - q)/z y = 0`: `heun_b(q, a, g, d, e, z)` and, from
///   `y = z^(1 - g) w` (which gives the equation of the same kind with `q - (1 - g) d`, `a + (1 - g) e`, `2 - g`),
///   `z^(1 - g) heun_b(q - (1 - g) d, a + (1 - g) e, 2 - g, d, e, z)`;
/// - double confluent `y'' + (g/z^2 + d/z + e) y' + (a z - q)/z^2 y = 0`: `heun_d(q, a, g, d, e, z)` and, from
///   `y = exp(-e z + g/z) z^(2 - d) w`, `exp(-e z + g/z) z^(2 - d) heun_d(q + d - 2, a - 2 e, -g, 4 - d, -e, z)`;
/// - triconfluent `y'' + (g + d z + e z^2) y' + (a z - q) y = 0`: `heun_t(q, a, g, d, e, z)` and, from
///   `y = exp(-g z - d z^2/2 - e z^3/3) w`, that factor times `heun_t(q + d, a - 2 e, -g, -d, -e, z)`.
///
/// Literature: A. Ronveaux (ed.), *Heun's Differential Equations* (1995), parts B to E; NIST DLMF 31.12.
fn heun_confluent<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, p: T<'h>, q: T<'h>, x: T<'h>) -> Res<Option<Vec<T<'h>>>> {
    let b = c.b;
    // (shift of p, shift of q): z p and z q, z^2 p and z^2 q, p and q
    for (kind, ps, qs) in [("heun_b", 1, 1), ("heun_d", 2, 2), ("heun_t", 0, 0)] {
        let Some(pc) = poly_coefs(c, p, x, ps, 2)? else { continue };
        let Some(qc) = poly_coefs(c, q, x, qs, 1)? else { continue };
        let (g, d, e) = (pc[0], pc[1], pc[2]);
        let (qq, a) = (c.simp(b.neg(qc[0]))?, qc[1]);
        let y1 = c.ev(kind, &[qq, a, g, d, e, x])?;
        let y2 = match kind {
            "heun_b" => {
                if integer_valued(c, g) {
                    continue;
                }
                let s = c.simp(b.sub(b.one(), g))?;
                let q2 = c.simp(b.sub(qq, b.mul(&[s, d])))?;
                let a2 = c.simp(b.add(&[a, b.mul(&[s, e])]))?;
                let g2 = c.simp(b.sub(b.int(2), g))?;
                let f = c.ev(kind, &[q2, a2, g2, d, e, x])?;
                c.eval(b.mul(&[b.pow(x, s), f]))?
            }
            "heun_d" => {
                let m = c.simp(b.sub(b.int(2), d))?;
                let q2 = c.simp(b.add(&[qq, d, b.int(-2)]))?;
                let a2 = c.simp(b.sub(a, b.mul(&[b.int(2), e])))?;
                let d2 = c.simp(b.sub(b.int(4), d))?;
                let f = c.ev(kind, &[q2, a2, b.neg(g), d2, b.neg(e), x])?;
                let ex = c.ev("exp", &[b.add(&[b.mul(&[b.neg(e), x]), b.div(g, x)])])?;
                c.eval(b.mul(&[f, ex, b.pow(x, m)]))?
            }
            _ => {
                let q2 = c.simp(b.add(&[qq, d]))?;
                let a2 = c.simp(b.sub(a, b.mul(&[b.int(2), e])))?;
                let f = c.ev(kind, &[q2, a2, b.neg(g), b.neg(d), b.neg(e), x])?;
                let arg = b.neg(b.add(&[
                    b.mul(&[g, x]),
                    b.mul(&[d, b.powi(x, 2), b.rat(1, 2)]),
                    b.mul(&[e, b.powi(x, 3), b.rat(1, 3)]),
                ]));
                let ex = c.ev("exp", &[arg])?;
                c.eval(b.mul(&[f, ex]))?
            }
        };
        return Ok(Some(vec![y1, y2]));
    }
    Ok(None)
}

/// The arguments of `t` when it is a call of `name`.
fn args_of<'h>(b: &mc_calculus::term::Build<'h>, t: T<'h>, name: &str) -> Option<&'h [T<'h>]> {
    let h = b.heap();
    match h.view(t) {
        mc_api::View::App { head, args } if b.sym_of(head).map(|s| h.sym_name(s)).as_deref() == Some(name) => Some(args),
        _ => None,
    }
}

/// Mathieu: `y'' + (A + B cos(k x + phi)) y = 0` (a sine as the cosine with phi - pi/2). With
/// `t = (k x + phi)/2` it is `y_tt + (a - 2 q cos(2 t)) y = 0`, `a = 4 A/k^2`, `q = -2 B/k^2` (DLMF 28.2.1):
/// `mathieu_c(a, q, t)`, `mathieu_s(a, q, t)`.
fn mathieu<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, p: T<'h>, q: T<'h>, x: T<'h>) -> Res<Option<Vec<T<'h>>>> {
    let b = c.b;
    if p != b.zero() {
        return Ok(None);
    }
    let q = c.expand(q)?;
    let mut consts = vec![];
    let mut wave: Option<(T<'h>, T<'h>)> = None;
    for t in c.terms(q) {
        if is_free(b, t, x) {
            consts.push(t);
            continue;
        }
        // B cos(arg) or B sin(arg) with B free of x
        let fs: Vec<T<'h>> = match args_of(b, t, "mul") {
            Some(a) => a.to_vec(),
            None => vec![t],
        };
        let (dep, free): (Vec<T<'h>>, Vec<T<'h>>) = fs.iter().partition(|&&f| !is_free(b, f, x));
        let [f] = dep[..] else { return Ok(None) };
        // B sin(u) = B cos(u - pi/2)
        let arg = match (args_of(b, f, "cos"), args_of(b, f, "sin")) {
            (Some(&[u]), _) => u,
            (_, Some(&[u])) => b.sub(u, b.mul(&[b.rat(1, 2), b.heap().sym_term(b.heap().sym("pi"))])),
            _ => return Ok(None),
        };
        if wave.is_some() {
            return Ok(None);
        }
        wave = Some((b.mul(&free), arg));
    }
    let Some((bb, arg)) = wave else { return Ok(None) };
    let k = c.d(arg, x)?;
    let k = c.simp(k)?;
    if k == b.zero() || !is_free(b, k, x) {
        return Ok(None);
    }
    let aa = b.add(&consts);
    let k2 = b.powi(k, 2);
    let a = c.simp(b.div(b.mul(&[b.int(4), aa]), k2))?;
    let qq = c.simp(b.div(b.mul(&[b.int(-2), bb]), k2))?;
    let t = c.simp(b.mul(&[b.rat(1, 2), arg]))?;
    let t = c.expand(t)?;
    Ok(Some(vec![c.ev("mathieu_c", &[a, qq, t])?, c.ev("mathieu_s", &[a, qq, t])?]))
}

/// `a^(1/3)`, real for negative a.
fn cbrt<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, a: T<'h>) -> Res<T<'h>> {
    let b = c.b;
    if super::linear::negative_like(c, a) {
        let r = c.ev("power_expand", &[b.pow(b.neg(a), b.rat(1, 3))])?;
        return c.simp(b.neg(r));
    }
    let r = c.ev("power_expand", &[b.pow(a, b.rat(1, 3))])?;
    c.simp(r)
}

fn airy<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, p: T<'h>, q: T<'h>, x: T<'h>) -> Res<Option<Vec<T<'h>>>> {
    let b = c.b;
    if p != b.zero() {
        return Ok(None);
    }
    // q = -(a x + b0)
    let dq = c.d(q, x)?;
    let a = c.simp(b.neg(dq))?;
    if a == b.zero() || !is_free(b, a, x) {
        return Ok(None);
    }
    let b0 = c.simp(b.neg(b.add(&[q, b.mul(&[a, x])])))?;
    if !is_free(b, b0, x) {
        return Ok(None);
    }
    let k = cbrt(c, a)?;
    let t = c.simp(b.mul(&[k, b.add(&[x, b.div(b0, a)])]))?;
    let t = c.expand(t)?;
    Ok(Some(vec![c.ev("airy_ai", &[t])?, c.ev("airy_bi", &[t])?]))
}

fn bessel<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, p: T<'h>, q: T<'h>, x: T<'h>) -> Res<Option<Vec<T<'h>>>> {
    let b = c.b;
    let pp = c.simp(b.mul(&[x, p]))?;
    if !is_free(b, pp, x) {
        return Ok(None);
    }
    let qq = c.simp(b.mul(&[b.powi(x, 2), q]))?;
    let qq = c.expand(qq)?;
    let mut bconst = vec![];
    let mut rest = vec![];
    for t in c.terms(qq) {
        if is_free(b, t, x) {
            bconst.push(t);
        } else {
            rest.push(t);
        }
    }
    let r = c.simp(b.add(&rest))?;
    if r == b.zero() {
        return Ok(None);
    }
    // r = A x^m
    let dr = c.d(r, x)?;
    let m = c.simp(b.div(b.mul(&[x, dr]), r))?;
    if !is_free(b, m, x) || m == b.zero() || b.ar.num(m).is_none() {
        return Ok(None);
    }
    let aa = c.simp(b.div(r, b.pow(x, m)))?;
    if !is_free(b, aa, x) {
        return Ok(None);
    }
    let bb = c.simp(b.add(&bconst))?;
    let gamma = c.simp(b.div(m, b.int(2)))?;
    let alpha = c.simp(b.div(b.sub(b.one(), pp), b.int(2)))?;
    let nu2 = c.simp(b.div(b.sub(b.powi(alpha, 2), bb), b.powi(gamma, 2)))?;
    let nu = c.ev("power_expand", &[b.pow(nu2, b.rat(1, 2))])?;
    let nu = c.simp(nu)?;
    let modified = super::linear::negative_like(c, aa);
    let a2 = if modified { b.neg(aa) } else { aa };
    let beta = c.ev("power_expand", &[b.pow(a2, b.rat(1, 2))])?;
    let beta = c.simp(b.div(beta, gamma))?;
    let arg = c.simp(b.mul(&[beta, b.pow(x, gamma)]))?;
    let pre = c.eval(b.pow(x, alpha))?;
    let (f1, f2) = if modified { ("bessel_i", "bessel_k") } else { ("bessel_j", "bessel_y") };
    let y1 = c.ev(f1, &[nu, arg])?;
    let y2 = c.ev(f2, &[nu, arg])?;
    Ok(Some(vec![c.eval(b.mul(&[pre, y1]))?, c.eval(b.mul(&[pre, y2]))?]))
}

/// `coefs` scaled so that the coefficient of y'' is `form`: the scaled `(c1, c0)`.
fn scaled<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, coefs: &[T<'h>], form: T<'h>, x: T<'h>) -> Res<Option<(T<'h>, T<'h>)>> {
    let b = c.b;
    let k = c.simp(b.div(coefs[2], form))?;
    if !is_free(b, k, x) || k == b.zero() {
        return Ok(None);
    }
    let c1 = c.simp(b.div(coefs[1], k))?;
    let c0 = c.simp(b.div(coefs[0], k))?;
    Ok(Some((c.expand(c1)?, c.expand(c0)?)))
}

/// `t = t0 + t1 x`: `(t0, t1)` when `t` is linear in x.
fn linear_in<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, t: T<'h>, x: T<'h>) -> Res<Option<(T<'h>, T<'h>)>> {
    let b = c.b;
    let t1 = c.d(t, x)?;
    let t1 = c.simp(t1)?;
    if !is_free(b, t1, x) {
        return Ok(None);
    }
    let t0 = c.simp(b.sub(t, b.mul(&[t1, x])))?;
    Ok(if is_free(b, t0, x) { Some((t0, t1)) } else { None })
}

fn gauss<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, coefs: &[T<'h>], x: T<'h>) -> Res<Option<Vec<T<'h>>>> {
    let b = c.b;
    let form = b.sub(x, b.powi(x, 2));
    let Some((c1, c0)) = scaled(c, coefs, form, x)? else { return Ok(None) };
    let Some((cc, s)) = linear_in(c, c1, x)? else { return Ok(None) };
    if !is_free(b, c0, x) {
        return Ok(None);
    }
    // a + b = -s - 1, a b = -c0
    let t = c.sym("d_solve·hg");
    let eq = c.equal(b.add(&[b.powi(t, 2), b.mul(&[b.add(&[s, b.one()]), t]), b.neg(c0)]), b.zero());
    let Some(ab) = c.solve_for(eq, t)? else { return Ok(None) };
    let (a1, b1) = match ab.as_slice() {
        [a1, b1] => (*a1, *b1),
        [a1] => (*a1, *a1),
        _ => return Ok(None),
    };
    let y1 = c.ev("hypergeometric_2f1", &[a1, b1, cc, x])?;
    let integer = matches!(b.heap().view(cc), mc_api::View::Int(_));
    let y2 = if integer {
        match super::linear::reduction(c, coefs, y1, x)? {
            Some(y2) => y2,
            None => return Ok(None),
        }
    } else {
        let a2 = c.eval(b.add(&[a1, b.neg(cc), b.one()]))?;
        let b2 = c.eval(b.add(&[b1, b.neg(cc), b.one()]))?;
        let c2 = c.eval(b.sub(b.int(2), cc))?;
        let f = c.ev("hypergeometric_2f1", &[a2, b2, c2, x])?;
        c.eval(b.mul(&[b.pow(x, b.sub(b.one(), cc)), f]))?
    };
    Ok(Some(vec![y1, y2]))
}

fn legendre<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, coefs: &[T<'h>], x: T<'h>) -> Res<Option<Vec<T<'h>>>> {
    let b = c.b;
    let form = b.sub(b.one(), b.powi(x, 2));
    let Some((c1, c0)) = scaled(c, coefs, form, x)? else { return Ok(None) };
    if !c.is_zero(b.add(&[c1, b.mul(&[b.int(2), x])]))? || !is_free(b, c0, x) {
        return Ok(None);
    }
    // nu (nu + 1) = c0
    let s = c.ev("power_expand", &[b.pow(b.add(&[b.one(), b.mul(&[b.int(4), c0])]), b.rat(1, 2))])?;
    let nu = c.simp(b.div(b.sub(s, b.one()), b.int(2)))?;
    Ok(Some(vec![c.ev("legendre_p", &[nu, x])?, c.ev("legendre_q", &[nu, x])?]))
}

fn kummer<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, coefs: &[T<'h>], x: T<'h>) -> Res<Option<Vec<T<'h>>>> {
    let b = c.b;
    let Some((c1, c0)) = scaled(c, coefs, x, x)? else { return Ok(None) };
    let Some((bb, s)) = linear_in(c, c1, x)? else { return Ok(None) };
    if s != b.int(-1) || !is_free(b, c0, x) {
        return Ok(None);
    }
    let a = c.simp(b.neg(c0))?;
    Ok(Some(vec![c.ev("hypergeometric_1f1", &[a, bb, x])?, c.ev("hypergeometric_u", &[a, bb, x])?]))
}

fn hermite<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, p: T<'h>, q: T<'h>, x: T<'h>) -> Res<Option<Vec<T<'h>>>> {
    let b = c.b;
    if !c.is_zero(b.add(&[p, b.mul(&[b.int(2), x])]))? || !is_free(b, q, x) {
        return Ok(None);
    }
    let nu = c.simp(b.div(q, b.int(2)))?;
    let h = c.ev("hermite_h", &[nu, x])?;
    let f = c.ev("hypergeometric_1f1", &[b.mul(&[b.rat(-1, 2), nu]), b.rat(1, 2), b.powi(x, 2)])?;
    Ok(Some(vec![h, f]))
}
