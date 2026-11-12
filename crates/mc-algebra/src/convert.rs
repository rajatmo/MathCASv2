//! Terms and algebraic numbers.
//!
//! **Reading** (decision F1.1): a closed term built from exact rationals and Gaussian rationals, sums, products,
//! integer powers, radicals `b^(p/n)` of algebraic `b` (the principal value, as the canonicaliser and `n` mean it)
//! and root objects `root(x => p(x), k)` is an algebraic number. Each atom is a root of a polynomial over Q
//! (`x^2 + 1` for `im`, `m_b(y^n)` for `b^(1/n)`, `p` for a root object); the factor it is a root of and which
//! root it is are decided with ball enclosures of its value, so no atom is ever confused with a conjugate.
//!
//! **Writing** (decision F1.2): results keep the terms the numbers were given as (the generators of an extension,
//! the radicals of quadratic roots): an element of a field is written as a combination of products of those terms,
//! never as a polynomial in an internal primitive element.
use crate::algnum::{choose_factor, common_field, AlgNum};
use crate::cx::Env;
use crate::embed::{self, Embedding};
use crate::qpoly::{self, q, qq, QPoly};
use mc_api::{Store, View, T};
use mc_arith::num::{Int, Rat};
use mc_arith::{Num, Real};
use mc_numeric::cball::{self, CBall};
use mc_poly::work::{Meter, WorkResult};
use rustc_hash::FxHashMap;

/// A memo of converted subterms.
pub(crate) type Memo<'h> = FxHashMap<T<'h>, Option<AlgNum>>;

/// The algebraic number of a closed exact term; `None` when the term is not one (or its atoms could not be
/// isolated).
pub(crate) fn to_alg<'h>(env: &Env<'h>, t: T<'h>, memo: &mut Memo<'h>, m: &mut Meter) -> WorkResult<Option<AlgNum>> {
    if let Some(v) = memo.get(&t) {
        return Ok(v.clone());
    }
    let v = to_alg_uncached(env, t, memo, m)?;
    memo.insert(t, v.clone());
    Ok(v)
}

fn to_alg_uncached<'h>(env: &Env<'h>, t: T<'h>, memo: &mut Memo<'h>, m: &mut Meter) -> WorkResult<Option<AlgNum>> {
    m.step(1)?;
    if let Some(x) = env.ar.num(t) {
        return Ok(match x {
            Num::Real(Real::Exact(c)) => Some(AlgNum::rational(&c)),
            Num::Complex(Real::Exact(a), Real::Exact(b)) => {
                let i = imaginary_unit();
                let k = i.k.clone();
                Some(AlgNum { a: k.add(&k.of(&a), &k.scale(&k.gen(), &b)), k })
            }
            _ => None,
        });
    }
    let h = env.h();
    let View::App { head, args } = h.view(t) else { return Ok(None) };
    if head == env.hd.add || head == env.hd.mul {
        let mut xs = Vec::with_capacity(args.len());
        for &a in args {
            match to_alg(env, a, memo, m)? {
                Some(x) => xs.push(x),
                None => return Ok(None),
            }
        }
        let Some((k, es)) = common_field(&xs, m)? else { return Ok(None) };
        let mut acc = if head == env.hd.add { k.zero() } else { k.one() };
        for e in &es {
            acc = if head == env.hd.add { k.add(&acc, e) } else { k.mul(&acc, e) };
        }
        return Ok(Some(AlgNum { k, a: acc }));
    }
    let exp_arg = if head == env.hd.pow && args.len() == 2 && args[0] == env.sym("e") { Some(args[1]) } else { env.args_of(t, "exp").and_then(|a| (a.len() == 1).then(|| a[0])) };
    // e^(r pi im) with r rational: the root of unity (-1)^r
    if let Some(arg) = exp_arg {
        let over_pi = env.ar.mul(&[arg, env.ar.pow(env.sym("pi"), env.int(-1))]);
        if let Some(Num::Complex(Real::Exact(re), Real::Exact(r))) = env.ar.num(over_pi) {
            if re.is_zero() {
                return power(&AlgNum::rational(&q(-1)), &r, m);
            }
        }
        return Ok(None);
    }
    if head == env.hd.pow && args.len() == 2 {
        let Some(b) = to_alg(env, args[0], memo, m)? else { return Ok(None) };
        let Some(e) = env.ar.num(args[1]).and_then(|e| e.as_rat().cloned()) else { return Ok(None) };
        return power(&b, &e, m);
    }
    if let Some(&[f, k]) = env.args_of(t, "root") {
        return root_object(env, f, k, m);
    }
    // algebraic_number(theta, [c0, c1, ...]) is c0 + c1 theta + ...
    if let Some(&[theta, cs]) = env.args_of(t, "algebraic_number") {
        let Some(cs) = env.as_list(cs) else { return Ok(None) };
        let terms: Vec<T<'h>> = cs.iter().enumerate().map(|(i, &c)| env.call("mul", &[c, env.call("pow", &[theta, env.int(i as i64)])])).collect();
        return to_alg(env, env.call("add", &terms), memo, m);
    }
    // cos, sin, tan of a rational multiple r pi: through the root of unity z = (-1)^r, cos = (z + 1/z)/2,
    // sin = (z - 1/z)/(2 i), tan = sin/cos
    for f in ["cos", "sin", "tan"] {
        let Some(&[arg]) = env.args_of(t, f) else { continue };
        let over_pi = env.ar.mul(&[arg, env.ar.pow(env.sym("pi"), env.int(-1))]);
        let Some(r) = env.ar.num(over_pi).and_then(|n| n.as_rat().cloned()) else { return Ok(None) };
        if r.den.to_u128().is_none_or(|d| d > 64) {
            return Ok(None);
        }
        let z = env.call("pow", &[env.int(-1), env.ar.number(&Num::rat(r.clone()))]);
        let zi = env.call("pow", &[env.int(-1), env.ar.number(&Num::rat(r.neg()))]);
        let half = env.ar.number(&Num::rat(qq(1, 2)));
        let i = env.ar.number(&Num::i());
        let c = env.call("mul", &[half, env.call("add", &[z, zi])]);
        let sn = env.call("mul", &[half, env.call("pow", &[i, env.int(-1)]), env.call("add", &[z, env.call("mul", &[env.int(-1), zi])])]);
        let form = match f {
            "cos" => c,
            "sin" => sn,
            _ => env.call("mul", &[sn, env.call("pow", &[c, env.int(-1)])]),
        };
        return to_alg(env, form, memo, m);
    }
    Ok(None)
}

/// `i`, the root of `x^2 + 1` with positive imaginary part (the second root object).
pub fn imaginary_unit() -> AlgNum {
    let f = vec![q(1), q(0), q(1)];
    AlgNum::root(&f, Embedding { index: 2, real: false, re: q(0), im: q(1), radius: qq(1, 2) })
}

/// `b^e` for a rational exponent: the principal value.
pub fn power(b: &AlgNum, e: &Rat, m: &mut Meter) -> WorkResult<Option<AlgNum>> {
    let n = e.den.to_u128().and_then(|v| u64::try_from(v).ok()).filter(|&v| v <= 64);
    let Some(n) = n else { return Ok(None) };
    let p = e.num.to_i64().filter(|v| v.unsigned_abs() <= 1 << 20);
    let Some(p) = p else { return Ok(None) };
    if b.a.is_empty() {
        return Ok((p > 0).then(|| AlgNum::rational(&q(0))));
    }
    let root = if n == 1 {
        b.clone()
    } else {
        // y = b^(1/n): a root of m_b(y^n), the principal one
        let mb = b.minpoly(m)?;
        let mut poly = vec![q(0); (mb.len() - 1) * n as usize + 1];
        for (i, c) in mb.iter().enumerate() {
            poly[i * n as usize] = c.clone();
        }
        let inv_n = Rat::new(Int::from_i64(1), Int::from_i64(n as i64));
        let mut z = |bits: u64, m: &mut Meter| -> WorkResult<Option<CBall>> {
            let Some(zb) = b.enclose(bits + 16, m)? else { return Ok(None) };
            Ok(principal_root(&zb, &inv_n, bits.max(64)))
        };
        match atom(&poly, &mut z, m)? {
            Some(r) => r,
            None => return Ok(None),
        }
    };
    let Some(a) = root.k.pow(&root.a, p) else { return Ok(None) };
    Ok(Some(AlgNum { k: root.k, a }))
}

/// The principal value `z^e` as a ball (`None` when `z` meets the branch cut too closely to decide).
fn principal_root(z: &CBall, e: &Rat, w: u64) -> Option<CBall> {
    if z.is_real() && !z.re.contains_zero() && !z.re.mid.neg {
        // a positive real: the real root
        return cball::pow_rat(z, e, w);
    }
    cball::pow_rat(z, e, w)
}

/// The algebraic number that is a root of `poly` (nonzero, over Q) and lies in the enclosures `z(bits)`.
pub fn atom(
    poly: &[Rat],
    z: &mut dyn FnMut(u64, &mut Meter) -> WorkResult<Option<CBall>>,
    m: &mut Meter,
) -> WorkResult<Option<AlgNum>> {
    let fs: Vec<QPoly> = qpoly::factor(poly, m)?.into_iter().map(|(f, _)| f).collect();
    let Some(f) = choose_factor(&fs, z, m)? else { return Ok(None) };
    if f.len() == 2 {
        return Ok(Some(AlgNum::rational(&f[0].neg())));
    }
    let Some(rs) = embed::roots(&f, m)? else { return Ok(None) };
    let Some(e) = embed::locate(&f, &rs, z, m)? else { return Ok(None) };
    Ok(Some(AlgNum::root(&f, e)))
}

/// The rational coefficients (index = degree) of the body of `x => body` as a polynomial in `x`; `None` when it is
/// not one.
pub(crate) fn rational_coefficients<'h>(env: &Env<'h>, x: T<'h>, body: T<'h>, m: &mut Meter) -> WorkResult<Option<QPoly>> {
    let c = env.conv();
    let (ring, shared) = c.ring(&[body], &[x], m)?;
    if ring.kernels.iter().any(|&k| k != x) {
        return Ok(None);
    }
    let p = c.to_poly(body, &ring, &shared, m)?;
    let Some(v) = ring.var(x) else { return Ok(None) };
    if p.num.degree(v).unwrap_or(0) > mc_poly::poly::MAX_DENSE_DEGREE {
        return Err(mc_poly::work::Halt::Memory);
    }
    let d = p.num.degree(v).unwrap_or(0) as usize;
    let mut out = vec![q(0); d + 1];
    for i in 0..p.num.len() {
        let e = p.num.exps(i)[v] as usize;
        out[e] = out[e].add(&p.coef(i));
    }
    Ok(Some(qpoly::trim(out)))
}

/// `root(x => p(x), k)` as an algebraic number (the k-th root counted with multiplicity in the order of root
/// objects).
fn root_object<'h>(env: &Env<'h>, f: T<'h>, k: T<'h>, m: &mut Meter) -> WorkResult<Option<AlgNum>> {
    let Some((x, body)) = env.function_parts(f) else { return Ok(None) };
    let Some(p) = rational_coefficients(env, x, body, m)? else { return Ok(None) };
    let View::Int(k) = env.h().view(k) else { return Ok(None) };
    if p.len() < 2 || k < 1 || k as usize > p.len() - 1 {
        return Ok(None);
    }
    let sf = qpoly::square_free(&p, m)?;
    let Some(rs) = embed::roots(&sf, m)? else { return Ok(None) };
    let fs = qpoly::factor(&p, m)?;
    // the roots with multiplicity, in order
    let mut idx = 0usize;
    for e in &rs {
        let mut z = |bits: u64, m: &mut Meter| embed::enclose(&sf, e, bits, m);
        let polys: Vec<QPoly> = fs.iter().map(|(f, _)| f.clone()).collect();
        let Some(fac) = choose_factor(&polys, &mut z, m)? else { return Ok(None) };
        let mult = fs.iter().find(|(f, _)| *f == fac).map_or(1, |(_, e)| *e as usize);
        idx += mult;
        if idx >= k as usize {
            return atom(&fac, &mut z, m);
        }
    }
    Ok(None)
}

// ------------------------------------------------------------------------------------------- writing

/// `sum c_i x^i`.
pub(crate) fn poly_term<'h>(env: &Env<'h>, p: &[Rat], x: T<'h>) -> T<'h> {
    let parts: Vec<T<'h>> = p
        .iter()
        .enumerate()
        .filter(|(_, c)| !c.is_zero())
        .map(|(i, c)| env.ar.mul(&[env.ar.number(&Num::rat(c.clone())), env.ar.pow(x, env.int(i as i64))]))
        .collect();
    env.ar.add(&parts)
}

/// The root object `root(x => f(x), k)` with `f` made primitive over the integers.
pub(crate) fn root_object_term<'h>(env: &Env<'h>, f: &[Rat], index: usize) -> T<'h> {
    let x = env.sym("x");
    let body = poly_term(env, &qpoly::from_ints(&qpoly::primitive(f)), x);
    env.call("root", &[env.function(x, body), env.int(index as i64)])
}
