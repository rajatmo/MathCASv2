//! Root objects `root(x => p(x), k)`: the k-th root of a polynomial in the order real roots ascending, then the
//! others by real part and imaginary part (decision 3B.4). With exact coefficients the object stays (degree one gives
//! its rational root); with inexact coefficients or an inexact index (what `n` makes of it) it is the k-th root at
//! that precision, computed by [`crate::numroots`].
use crate::cx::{Env, Out};
use crate::numroots::{numeric_roots, Precision};
use mc_api::{Store, View, T};
use mc_algebra::qpoly::{self, QPoly};
use mc_arith::num::Rat;
use mc_arith::{Num, Real};
use mc_poly::work::{Meter, WorkResult};

/// The bound variable and the body of a root object's function: `x => body`, or a polynomial in one variable.
pub(crate) fn function_parts<'h>(env: &Env<'h>, f: T<'h>, m: &mut Meter) -> WorkResult<Option<(T<'h>, T<'h>)>> {
    if let Some(&[params, body]) = env.args_of(f, "fn") {
        return Ok(match env.as_list(params) {
            Some(&[x]) => Some((x, body)),
            _ => None,
        });
    }
    let ring = env.conv().ring(&[f], &[], m)?.0;
    let vars: Vec<T<'h>> = ring.kernels.iter().copied().filter(|&k| env.ar.num(k).is_none()).collect();
    // the variable must be a symbol: `root(f_, k)` in a rewrite rule is a pattern, not the polynomial `f_`
    Ok(match vars.as_slice() {
        [x] if matches!(env.ar.store().view(*x), View::Sym(_)) && f != *x => Some((*x, f)),
        _ => None,
    })
}

/// The coefficients (index = degree) of `body` as a polynomial in `x` whose coefficients are numbers; `None` when
/// another symbol occurs.
pub(crate) fn numeric_coefficients<'h>(
    env: &Env<'h>,
    x: T<'h>,
    body: T<'h>,
    m: &mut Meter,
) -> WorkResult<Option<Vec<Num>>> {
    let c = env.conv();
    let (ring, shared) = c.ring(&[body], &[x], m)?;
    if ring.kernels.iter().any(|&k| k != x && env.ar.num(k).is_none()) {
        return Ok(None);
    }
    let p = c.to_poly(body, &ring, &shared, m)?;
    let v = ring.var(x).expect("x is in the ring");
    let kv: Vec<Option<Num>> = ring.kernels.iter().map(|&k| env.ar.num(k)).collect();
    // one coefficient per degree: a huge degree is a memory limit, not an allocation failure
    if p.num.degree(v).unwrap_or(0) > mc_poly::poly::MAX_DENSE_DEGREE {
        return Err(mc_poly::work::Halt::Memory);
    }
    let d = p.num.degree(v).unwrap_or(0) as usize;
    let mut out = vec![Num::int(0); d + 1];
    for i in 0..p.num.len() {
        m.step(1)?;
        let e = p.num.exps(i);
        let mut term = Num::rat(p.coef(i));
        for (j, &k) in e.iter().enumerate() {
            if j != v && k > 0 {
                let base = kv[j].as_ref().expect("numeric kernels");
                let Ok(pw) = base.pow_int(&mc_arith::num::Int::from_i64(k as i64), mc_arith::DEFAULT_MAX_BITS) else {
                    return Ok(None);
                };
                term = term.mul(&pw);
            }
        }
        let k = e[v] as usize;
        out[k] = out[k].add(&term);
    }
    Ok(Some(out))
}

/// The precision of a set of numbers: machine if any is a machine number, else the least ball precision; `None`
/// when all are exact.
pub(crate) fn precision_of(xs: &[Num]) -> Option<Precision> {
    let mut prec: Option<u64> = None;
    for x in xs {
        let parts: Vec<&Real> = match x {
            Num::Real(r) => vec![r],
            Num::Complex(a, b) => vec![a, b],
        };
        for r in parts {
            match r {
                Real::Machine(_) => return Some(Precision::Machine),
                Real::Ball(_, p) => prec = Some(prec.map_or(*p, |q| q.min(*p))),
                Real::Exact(_) => {}
            }
        }
    }
    prec.map(Precision::Bits)
}

/// `root(f, k)`.
pub(crate) fn root<'h>(env: &Env<'h>, args: &[T<'h>], m: &mut Meter) -> WorkResult<Out<'h>> {
    // `root(f, k, 0)`: the full form carries a third argument 0 (the root is not inexact)
    let (f, k) = match *args {
        [f, k] => (f, k),
        [f, k, z] if matches!(env.h().view(z), View::Int(0)) => (f, k),
        _ => return Ok(env.arg_failure("root takes a function and an index")),
    };
    let Some((x, body)) = function_parts(env, f, m)? else { return Ok(Out::Decline) };
    let Some(coefs) = numeric_coefficients(env, x, body, m)? else { return Ok(Out::Decline) };
    let Some(kn) = env.ar.num(k) else { return Ok(Out::Decline) };
    let d = coefs.len() - 1;
    if d == 0 {
        return Ok(env.arg_failure("the polynomial of a root object must have positive degree"));
    }
    // the index: an integer, or an inexact number with an integer value (from n)
    let kv = match env.h().view(k) {
        View::Int(i) => Some(i),
        _ => match &kn {
            Num::Real(r) => {
                let v = r.to_f64();
                (v.fract() == 0.0 && v.abs() < 1e9).then_some(v as i64)
            }
            _ => None,
        },
    };
    let Some(kv) = kv else { return Ok(env.arg_failure("the index of a root object must be an integer")) };
    if kv < 1 || kv as usize > d {
        return Ok(env.arg_failure("the index of a root object is out of range"));
    }
    let mut all = coefs.clone();
    all.push(kn.clone());
    let Some(prec) = precision_of(&all) else {
        if d == 1 {
            let r = coefs[0].neg();
            return Ok(match coefs[1].recip() {
                Ok(inv) => Out::Done(env.ar.number(&r.mul(&inv))),
                Err(_) => Out::Decline,
            });
        }
        let rats: Option<Vec<Rat>> = coefs
            .iter()
            .map(|c| match c {
                Num::Real(Real::Exact(q)) => Some(q.clone()),
                _ => None,
            })
            .collect();
        let Some(rats) = rats else { return Ok(Out::Decline) };
        return Ok(match canonical_root(env, &rats, kv as usize, m)? {
            Some(t) if t != env.call("root", &[f, k]) => Out::Again(t),
            _ => Out::Decline,
        });
    };
    match numeric_roots(&coefs, prec, m)? {
        Some(rs) => Ok(Out::Done(env.ar.number(&rs[kv as usize - 1]))),
        None => Ok(Out::Decline),
    }
}

/// The canonical form of the k-th root (in root order) of the polynomial with rational coefficients `p`: the root of
/// the irreducible factor it belongs to, as a rational number (degree one), radicals `(-b +- sqrt(b^2 - 4ac))/(2a)`
/// (degree two) or the root object `root(&(q(&1)), j)` of the primitive irreducible factor `q` (decision i19.root:
/// the normal form of root objects). `None` when the roots could not be ordered.
fn canonical_root<'h>(env: &Env<'h>, p: &[Rat], k: usize, m: &mut Meter) -> WorkResult<Option<T<'h>>> {
    let factors = qpoly::factor(p, m)?;
    let (g, j) = match factors.as_slice() {
        [(g, 1)] if g.len() == p.len() => (g.clone(), k),
        _ => {
            // every root of every factor, repeated by multiplicity, in root order: the k-th
            let mut all: Vec<(Num, usize, usize)> = vec![];
            for (fi, (g, mult)) in factors.iter().enumerate() {
                if g.len() < 2 {
                    continue;
                }
                let cs: Vec<Num> = g.iter().map(|c| Num::rat(c.clone())).collect();
                let Some(rs) = numeric_roots(&cs, Precision::Machine, m)? else { return Ok(None) };
                for (ri, r) in rs.into_iter().enumerate() {
                    for _ in 0..*mult {
                        all.push((r.clone(), fi, ri + 1));
                    }
                }
            }
            all.sort_by(|a, b| root_order(&a.0, &b.0));
            let Some((_, fi, ri)) = all.get(k - 1) else { return Ok(None) };
            (factors[*fi].0.clone(), *ri)
        }
    };
    Ok(Some(irreducible_root(env, &g, j)))
}

/// The order of root objects: real roots ascending, then the others by real part, then imaginary part.
fn root_order(a: &Num, b: &Num) -> std::cmp::Ordering {
    let parts = |x: &Num| match x {
        Num::Real(r) => (false, r.to_f64(), 0.0),
        Num::Complex(re, im) => (true, re.to_f64(), im.to_f64()),
    };
    let (ca, ra, ia) = parts(a);
    let (cb, rb, ib) = parts(b);
    ca.cmp(&cb).then(ra.total_cmp(&rb)).then(ia.total_cmp(&ib))
}

/// Root `j` of the irreducible polynomial `g` over Q.
fn irreducible_root<'h>(env: &Env<'h>, g: &QPoly, j: usize) -> T<'h> {
    let ar = &env.ar;
    let q = |x: &Rat| ar.number(&Num::rat(x.clone()));
    let z: Vec<Rat> = qpoly::from_ints(&qpoly::primitive(g));
    match z.len() {
        2 => q(&z[0].neg().mul(&z[1].recip().expect("degree one"))),
        3 => {
            // (-b -+ sqrt(b^2 - 4ac)) / (2a), root 1 with the minus sign (the smaller real root, or the one with the
            // negative imaginary part)
            let (c, b, a) = (&z[0], &z[1], &z[2]);
            let disc = b.mul(b).sub(&Rat::from_i64(4).mul(a).mul(c));
            let s = ar.pow(q(&disc), ar.rational(ar.int(1), ar.int(2)));
            let s = if j == 1 { ar.neg(s) } else { s };
            let two_a = Rat::from_i64(2).mul(a);
            ar.mul(&[q(&two_a.recip().expect("degree two")), ar.add(&[q(&b.neg()), s])])
        }
        _ => {
            let x = env.sym("&1");
            let parts: Vec<T<'h>> = z
                .iter()
                .enumerate()
                .filter(|(_, c)| !c.is_zero())
                .map(|(i, c)| ar.mul(&[q(c), ar.pow(x, ar.int(i as i64))]))
                .collect();
            let body = ar.add(&parts);
            env.call("root", &[env.call("fn", &[env.list(&[x]), body]), env.int(j as i64)])
        }
    }
}
