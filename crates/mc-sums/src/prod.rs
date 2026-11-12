//! Products of rational functions in closed form.
//!
//! `prod_{k=lo}^{hi} f(k)` for a rational `f`: the numerator and denominator are factored over the integers (with
//! the other variables as coefficients); a factor `l k + m` gives `l^N pochhammer(lo + m / l, N)` with
//! `N = hi - lo + 1`, a quadratic factor its two roots by the quadratic formula, and the constants their `N`-th
//! power. To infinity the product converges (to a nonzero value) exactly when the leading coefficients cancel and
//! the roots of numerator and denominator have the same sum; it is then `prod gamma(lo - b) / prod gamma(lo - a)`
//! over the roots `a` of the numerator and `b` of the denominator, and pairs `gamma(u) gamma(s - u)` (s a positive
//! integer) become `pi / sin(pi u)` times a polynomial (Euler's reflection formula). Factors of degree three or more
//! are not handled.
use crate::cx::Sx;
use crate::rf::{coeff, deg};
use crate::{some, w};
use mc_api::{names, Store, View, T};
use mc_eval::Res;
use mc_poly::work::Meter;
use mc_poly::Poly;

/// One factor `(base, exponent)` of a product of powers.
pub type Pow<'h> = (T<'h>, i64);

/// The roots of the factors of `f` in `k` with multiplicities (negative for the denominator) and the constant
/// `c` with `f = c prod (k - r)^e`; `None` when a factor has degree above 2 or `f` is not rational in `k`.
pub fn roots<'h>(sx: &mut Sx<'_, '_, '_, 'h>, f: T<'h>, k: T<'h>, m: &mut Meter) -> Res<Option<(T<'h>, Vec<Pow<'h>>)>> {
    let ring = w!(sx, sx.ring(&[f], &[k], m));
    if !sx.clean(&ring, &[k]) {
        return Ok(None);
    }
    let v = some!(ring.var(k));
    let rf = some!(w!(sx, sx.read(f, &ring, m)));
    if rf.is_zero() {
        return Ok(None);
    }
    let mut consts: Vec<T<'h>> = vec![];
    let mut out: Vec<Pow<'h>> = vec![];
    for (p, sign) in [(&rf.num, 1i64), (&rf.den, -1i64)] {
        let fac = w!(sx, mc_poly::factor::factor(p, false, m));
        let unit = Poly::constant(p.nvars(), fac.unit.clone());
        let ut = w!(sx, sx.pterm(&unit, &ring, m));
        consts.push(sx.pow(ut, sx.int(sign))?);
        for (q, e) in &fac.factors {
            let e = *e as i64 * sign;
            let qt = w!(sx, sx.pterm(q, &ring, m));
            match deg(q, v) {
                Some(0) | None => {
                    consts.push(sx.pow(qt, sx.int(e))?);
                }
                Some(1) => {
                    let l = w!(sx, sx.pterm(&coeff(q, v, 1), &ring, m));
                    let c0 = w!(sx, sx.pterm(&coeff(q, v, 0), &ring, m));
                    let r = sx.div(c0, l)?;
                    let r = sx.mul(&[sx.int(-1), r])?;
                    out.push((r, e));
                    consts.push(sx.pow(l, sx.int(e))?);
                }
                Some(2) => {
                    let a = w!(sx, sx.pterm(&coeff(q, v, 2), &ring, m));
                    let b = w!(sx, sx.pterm(&coeff(q, v, 1), &ring, m));
                    let c = w!(sx, sx.pterm(&coeff(q, v, 0), &ring, m));
                    // (-b ± sqrt(b^2 - 4 a c)) / (2 a)
                    let b2 = sx.pow(b, sx.int(2))?;
                    let ac = sx.mul(&[sx.int(-4), a, c])?;
                    let disc = sx.add(&[b2, ac])?;
                    let sq = sx.ev("sqrt", &[disc])?;
                    let nb = sx.mul(&[sx.int(-1), b])?;
                    let a2 = sx.mul(&[sx.int(2), a])?;
                    for s in [1, -1] {
                        let t = sx.mul(&[sx.int(s), sq])?;
                        let num = sx.add(&[nb, t])?;
                        let r = sx.div(num, a2)?;
                        let r = sx.ev("expand", &[r])?;
                        out.push((r, e));
                    }
                    consts.push(sx.pow(a, sx.int(e))?);
                }
                _ => return Ok(None),
            }
        }
    }
    let c = sx.mul(&consts)?;
    Ok(Some((c, out)))
}

/// `prod_{k=lo}^{hi} f(k)` for a rational `f` (`hi = None`: to infinity).
pub fn rational_product<'h>(
    sx: &mut Sx<'_, '_, '_, 'h>,
    f: T<'h>,
    k: T<'h>,
    lo: T<'h>,
    hi: Option<T<'h>>,
    m: &mut Meter,
) -> Res<Option<T<'h>>> {
    let (c, rs) = some!(roots(sx, f, k, m)?);
    match hi {
        Some(hi) => {
            let n0 = sx.sub(hi, lo)?;
            let n = sx.plus(n0, 1)?;
            let mut fs = vec![sx.pow(c, n)?];
            let mut gs = vec![];
            for &(r, e) in &rs {
                let a = sx.sub(lo, r)?;
                let p = sx.ev("pochhammer", &[a, n])?;
                fs.push(sx.pow(p, sx.int(e))?);
                // pochhammer(a, n) = gamma(a + n) / gamma(a)
                let an = sx.add(&[a, n])?;
                gs.push((an, e));
                gs.push((a, -e));
            }
            let pochhammers = sx.mul(&fs)?;
            // when the gamma functions cancel to a rational function, that form
            let cn = sx.pow(c, n)?;
            let g = gamma_product(sx, gs)?;
            if !sx.contains_any(g, &["gamma"]) {
                let v = sx.mul(&[cn, g])?;
                return Ok(Some(crate::sumfn::tidy(sx, v)?));
            }
            Ok(Some(pochhammers))
        }
        None => {
            // convergence: c == 1, the multiplicities and the weighted roots sum to zero
            if c != sx.int(1) {
                return Ok(None);
            }
            if rs.iter().map(|r| r.1).sum::<i64>() != 0 {
                return Ok(None);
            }
            let mut ws = vec![];
            for &(r, e) in &rs {
                ws.push(sx.mul(&[sx.int(e), r])?);
            }
            let s = sx.add(&ws)?;
            let s = sx.ev("simplify", &[s])?;
            if s != sx.int(0) {
                return Ok(None);
            }
            let mut gs: Vec<Pow<'h>> = vec![];
            for (r, e) in rs {
                let a = sx.sub(lo, r)?;
                // a pole of gamma: the product has a zero factor
                if let Some(i) = sx.as_int(a) {
                    if i <= 0 {
                        return Ok(if e > 0 { Some(sx.int(0)) } else { None });
                    }
                }
                gs.push((a, -e));
            }
            let g = gamma_product(sx, gs)?;
            let g = crate::sumfn::tidy(sx, g)?;
            Ok(Some(csch(sx, g)?))
        }
    }
}

/// `t` with negative powers of `sinh` written as powers of `csch`.
pub fn csch<'h>(sx: &mut Sx<'_, '_, '_, 'h>, t: T<'h>) -> Res<T<'h>> {
    let h = sx.h();
    let View::App { head, args } = h.view(t) else { return Ok(t) };
    let mut new = Vec::with_capacity(args.len());
    for &a in args {
        new.push(csch(sx, a)?);
    }
    if head == sx.sym(names::POW) && new.len() == 2 {
        if let (Some(&[x]), Some(e)) = (sx.args_of(new[0], "sinh"), sx.as_int(new[1])) {
            if e < 0 {
                let c = sx.ev("csch", &[x])?;
                return sx.pow(c, sx.int(-e));
            }
        }
    }
    if new.as_slice() == args {
        return Ok(t);
    }
    sx.eval(h.app(head, &new))
}

/// `prod gamma(a)^e`, with pairs whose arguments differ by an integer turned into rational factors and pairs whose
/// arguments add up to a positive integer turned into `pi / sin(pi u)` by the reflection formula.
pub fn gamma_product<'h>(sx: &mut Sx<'_, '_, '_, 'h>, mut gs: Vec<Pow<'h>>) -> Res<T<'h>> {
    let mut fs: Vec<T<'h>> = vec![];
    // integer differences with opposite exponents: gamma(a + m) / gamma(a) = pochhammer(a, m)
    let mut i = 0;
    while i < gs.len() {
        let mut matched = false;
        for j in 0..gs.len() {
            if i == j || gs[i].1 == 0 || gs[j].1 == 0 || gs[i].1.signum() == gs[j].1.signum() {
                continue;
            }
            let d = sx.sub(gs[i].0, gs[j].0)?;
            let d = sx.ev("expand", &[d])?;
            let Some(md) = sx.as_int(d) else { continue };
            if md.abs() > 64 {
                continue;
            }
            let e = gs[i].1.abs().min(gs[j].1.abs()) * gs[i].1.signum();
            // gamma(a_i)^e gamma(a_j)^-e = pochhammer(a_j, a_i - a_j)^e, written as a product of linear factors
            let (lo, len, sgn) = if md >= 0 { (gs[j].0, md, 1) } else { (gs[i].0, -md, -1) };
            let mut lin = vec![];
            for t in 0..len {
                lin.push(sx.plus(lo, t)?);
            }
            let p = sx.mul(&lin)?;
            fs.push(sx.pow(p, sx.int(e * sgn))?);
            gs[i].1 -= e;
            gs[j].1 += e;
            matched = true;
            break;
        }
        if !matched || gs[i].1 == 0 {
            i += 1;
        }
    }
    gs.retain(|g| g.1 != 0);
    // reflection: gamma(u) gamma(s - u) = pi / sin(pi u) prod_{j=1}^{s-1} (j - u)
    let mut used = vec![false; gs.len()];
    let pi = sx.sym("pi");
    for i in 0..gs.len() {
        if used[i] {
            continue;
        }
        for j in i + 1..gs.len() {
            if used[j] || gs[i].1 != gs[j].1 {
                continue;
            }
            let s = sx.add(&[gs[i].0, gs[j].0])?;
            let s = sx.ev("expand", &[s])?;
            let Some(si) = sx.as_int(s) else { continue };
            if !(-8..=16).contains(&si) {
                continue;
            }
            let u = gs[i].0;
            // at an integer u both gammas are finite numbers (gamma(1) gamma(3) of (k^2 - 1)/(k^2 + 2)) and the
            // formula would divide by sin(pi u) = 0
            if sx.as_int(u).is_some() {
                continue;
            }
            let sn = sin_pi(sx, u)?;
            let mut parts = vec![pi, sx.pow(sn, sx.int(-1))?];
            // gamma(u) gamma(s - u) = pi / sin(pi u) prod_{j=1}^{s-1} (j - u), or / prod_{j=s}^{0} (j - u)
            for jj in 1..si {
                parts.push(sx.sub(sx.int(jj), u)?);
            }
            for jj in si..=0 {
                let x = sx.sub(sx.int(jj), u)?;
                parts.push(sx.pow(x, sx.int(-1))?);
            }
            let v = sx.mul(&parts)?;
            fs.push(sx.pow(v, sx.int(gs[i].1))?);
            used[i] = true;
            used[j] = true;
            break;
        }
    }
    for (g, u) in gs.iter().zip(&used) {
        if !u {
            let gt = sx.ev("gamma", &[g.0])?;
            fs.push(sx.pow(gt, sx.int(g.1))?);
        }
    }
    sx.mul(&fs)
}

/// `u` split into an integer `m` and the rest: the integer part of its rational constant term.
pub fn integer_part<'h>(sx: &mut Sx<'_, '_, '_, 'h>, u: T<'h>) -> Res<(i64, T<'h>)> {
    let h = sx.h();
    let u = sx.ev("expand", &[u])?;
    let c = match h.view(u) {
        View::App { head, args } if head == sx.sym(names::ADD) => args[0],
        _ => u,
    };
    let rc = sx.ev("re", &[c])?;
    let fl = sx.ev("floor", &[rc])?;
    let Some(m) = sx.as_int(fl) else { return Ok((0, u)) };
    // a complex constant: its real part
    if m == 0 {
        return Ok((0, u));
    }
    let x = sx.plus(u, -m)?;
    Ok((m, x))
}

/// `sin(pi u)`, reduced by integers (`sin(pi (m + x)) = (-1)^m sin(pi x)`) and with `sin(i pi y) = i sinh(pi y)`.
pub fn sin_pi<'h>(sx: &mut Sx<'_, '_, '_, 'h>, u: T<'h>) -> Res<T<'h>> {
    let pi = sx.sym("pi");
    let (m, x) = integer_part(sx, u)?;
    let im = sx.ev("sqrt", &[sx.int(-1)])?;
    let nim = sx.mul(&[sx.int(-1), im])?;
    let y = sx.mul(&[x, nim])?;
    let y = sx.ev("expand", &[y])?;
    let s = if sx.contains_any(x, &[names::COMPLEX]) && !sx.contains_any(y, &[names::COMPLEX]) {
        let py = sx.mul(&[pi, y])?;
        let sh = sx.ev("sinh", &[py])?;
        sx.mul(&[im, sh])?
    } else {
        let px = sx.mul(&[pi, x])?;
        sx.ev("sin", &[px])?
    };
    if m % 2 != 0 {
        return sx.mul(&[sx.int(-1), s]);
    }
    Ok(s)
}

/// The closed form of `prod_{k=lo}^{hi} f(k)` for a product of factors: rational factors together, the others
/// (`c^g(k)`, ...) through `product` itself; `None` when one stays unevaluated.
pub fn product<'h>(
    sx: &mut Sx<'_, '_, '_, 'h>,
    f: T<'h>,
    k: T<'h>,
    lo: T<'h>,
    hi: Option<T<'h>>,
    m: &mut Meter,
) -> Res<Option<T<'h>>> {
    let h = sx.h();
    if crate::verify::is_neg_inf(sx, lo) {
        // f(0) prod_{k>=1} f(k) prod_{k>=1} f(-k)
        let f0 = sx.subst(f, k, sx.int(0))?;
        if sx.bad(f0) {
            return Ok(None);
        }
        let nk = sx.mul(&[sx.int(-1), k])?;
        let fm = sx.subst(f, k, nk)?;
        let up = match hi {
            None => some!(product(sx, f, k, sx.int(1), None, m)?),
            Some(hi) => some!(product(sx, f, k, sx.int(1), Some(hi), m)?),
        };
        let down = some!(product(sx, fm, k, sx.int(1), None, m)?);
        let p = sx.mul(&[f0, up, down])?;
        return Ok(Some(crate::sumfn::tidy(sx, p)?));
    }
    let factors: Vec<T<'h>> = match h.view(f) {
        View::App { head, args } if head == sx.sym(names::MUL) => args.to_vec(),
        _ => vec![f],
    };
    let mut rational = vec![];
    let mut other = vec![];
    for &g in &factors {
        if crate::hyper::rational_in(sx, g, k)? {
            rational.push(g);
        } else {
            other.push(g);
        }
    }
    if rational.is_empty() {
        return Ok(None);
    }
    let rpart = sx.mul(&rational)?;
    let mut out = vec![some!(rational_product(sx, rpart, k, lo, hi, m)?)];
    if !other.is_empty() {
        let o = sx.mul(&other)?;
        let hi_t = hi.unwrap_or_else(|| sx.sym("inf"));
        let spec = sx.list(&[k, lo, hi_t]);
        let p = sx.ev("product", &[o, spec])?;
        if sx.contains_any(p, &["product"]) {
            return Ok(None);
        }
        out.push(p);
    }
    Ok(Some(sx.mul(&out)?))
}
