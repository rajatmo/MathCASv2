//! Basic and bilateral hypergeometric series:
//!
//! - `q_hypergeometric_pfq([a1, ..., ar], [b1, ..., bs], q, z)` = sum_n (a1; q)_n ... (ar; q)_n/((b1; q)_n ... (bs; q)_n
//!   (q; q)_n) ((-1)^n q^(n(n-1)/2))^(1 + s - r) z^n (the basic hypergeometric series r phi s, DLMF 17.4.1), summed
//!   with the term ratio prod (1 - a_i q^n)/prod (1 - b_j q^n)/(1 - q^(n+1)) ((-1) q^n)^(1 + s - r) z at the working
//!   precision;
//! - `bilateral_hypergeometric_pfq([a1, ..., ap], [b1, ..., bq], z)` = sum over all integers n of prod (a_i)_n/prod
//!   (b_j)_n z^n with (a)_(-n) = (-1)^n/(1 - a)_n: the sum of (p+1)F(q)([a, 1], [b], z) and (q+1)F(p)([1 - b, 1], [1 -
//!   a], (-1)^(p-q)/z) - 1, each continued as `hypergeometric_pfq` continues it (DLMF 16.2, 16.4(iv)).
use crate::cover::{mag, work};
use mc_api::T;
use mc_arith::{Kind, Num};
use mc_eval::Res;
use mc_numeric::formula::{Def, Fx};

type R<'h> = Res<Option<T<'h>>>;

/// Every term a number and one inexact?
fn inexact_numbers<'h>(fx: &Fx<'_, '_, '_, 'h>, ts: &[T<'h>]) -> bool {
    let nums: Option<Vec<Num>> = ts.iter().map(|&t| fx.num(t)).collect();
    nums.is_some_and(|ns| ns.iter().any(|n| n.kind() != Kind::Exact))
}

fn q_hypergeometric_pfq<'h>(fx: &mut Fx<'_, '_, '_, 'h>, args: &[T<'h>]) -> R<'h> {
    let (Some(a), Some(b)) = (fx.as_list(args[0]), fx.as_list(args[1])) else { return Ok(None) };
    let (q, z) = (args[2], args[3]);
    if let Some(zs) = fx.as_list(z) {
        let items: Vec<T<'h>> = zs.iter().map(|&x| fx.call("q_hypergeometric_pfq", &[args[0], args[1], q, x])).collect();
        return Ok(Some(fx.list(&items)));
    }
    if fx.is_int(z, 0) {
        return Ok(Some(fx.int(1)));
    }
    let (r, s) = (a.len(), b.len());
    let mut all: Vec<T<'h>> = a.to_vec();
    all.extend_from_slice(b);
    all.push(q);
    all.push(z);
    if !inexact_numbers(fx, &all) {
        return Ok(None);
    }
    // machine arguments: the series in complex machine arithmetic (round 37), else the ball path
    if let Some((ms, real)) = crate::fast29::machine_args(fx, &all) {
        if let Some(v) = crate::fast29::q_hyper(&ms[..r], &ms[r..r + s], ms[r + s], ms[r + s + 1]) {
            if let Some(t) = crate::fast29::machine_out(fx, v, real) {
                return Ok(Some(t));
            }
        }
    }
    let Some((wk, xs)) = work(fx, &all, 32) else { return Ok(None) };
    let (aw, bw) = (&xs[..r], &xs[r..r + s]);
    let (qw, zw) = (xs[r + s], xs[r + s + 1]);
    let e = 1 + s as i64 - r as i64;
    let eps = 2f64.powi(-(wk.w.min(1000) as i32));
    let mut sum = fx.int(1);
    let mut t = fx.int(1);
    let mut qn = fx.int(1); // q^n
    let mut small = 0;
    for n in 0..20_000usize {
        if n % 64 == 63 {
            fx.tick(1)?;
        }
        let mut num = vec![t, zw];
        for &ai in aw {
            num.push(fx.sub(fx.int(1), fx.mul(&[ai, qn])));
        }
        let qn1 = fx.eval(fx.mul(&[qn, qw]))?;
        let mut den = vec![fx.sub(fx.int(1), qn1)];
        for &bj in bw {
            den.push(fx.sub(fx.int(1), fx.mul(&[bj, qn])));
        }
        if e != 0 {
            // ((-1) q^n)^e
            let f = fx.pow(fx.neg(qn), fx.int(e));
            num.push(f);
        }
        let d = fx.eval(fx.mul(&den))?;
        if fx.num(d).is_none_or(|v| v.is_zero()) {
            return Ok(None);
        }
        t = fx.eval(fx.div(fx.mul(&num), d))?;
        if fx.num(t).is_none() {
            return Ok(None);
        }
        sum = fx.eval(fx.add(&[sum, t]))?;
        qn = qn1;
        if fx.num(t).is_some_and(|v| v.is_zero()) {
            // a terminating series ((a_i; q)_n = 0)
            break;
        }
        if mag(fx, t) <= eps * mag(fx, sum).max(1e-300) {
            small += 1;
            if small >= 3 {
                break;
            }
        } else {
            small = 0;
        }
        if n == 19_999 {
            return Ok(None);
        }
    }
    let real = wk.real;
    Ok(wk.out(fx, sum, real))
}

fn bilateral_hypergeometric_pfq<'h>(fx: &mut Fx<'_, '_, '_, 'h>, args: &[T<'h>]) -> R<'h> {
    let (Some(a), Some(b)) = (fx.as_list(args[0]), fx.as_list(args[1])) else { return Ok(None) };
    let z = args[2];
    if let Some(zs) = fx.as_list(z) {
        let items: Vec<T<'h>> = zs.iter().map(|&x| fx.call("bilateral_hypergeometric_pfq", &[args[0], args[1], x])).collect();
        return Ok(Some(fx.list(&items)));
    }
    if fx.is_int(z, 0) {
        return Ok(Some(fx.int(0)));
    }
    let mut all: Vec<T<'h>> = a.to_vec();
    all.extend_from_slice(b);
    all.push(z);
    if !inexact_numbers(fx, &all) {
        return Ok(None);
    }
    let (p, q) = (a.len() as i64, b.len() as i64);
    let one = fx.int(1);
    let mut up_a: Vec<T<'h>> = a.to_vec();
    up_a.push(one);
    let b = b.to_vec();
    let mut dn_a: Vec<T<'h>> = b.iter().map(|&x| fx.sub(fx.int(1), x)).collect();
    dn_a.push(one);
    let dn_b: Vec<T<'h>> = a.iter().map(|&x| fx.sub(fx.int(1), x)).collect();
    let w = if (p - q) % 2 == 0 { fx.div(fx.int(1), z) } else { fx.div(fx.int(-1), z) };
    let mut dn_a_e = vec![];
    for t in dn_a {
        dn_a_e.push(fx.eval(t)?);
    }
    let mut dn_b_e = vec![];
    for t in dn_b {
        dn_b_e.push(fx.eval(t)?);
    }
    let up = pfq(fx, &up_a, &b, z)?;
    let w = fx.eval(w)?;
    let dn = pfq(fx, &dn_a_e, &dn_b_e, w)?;
    if fx.num(up).is_none() || fx.num(dn).is_none() {
        return Ok(None);
    }
    let v = fx.eval(fx.sub(fx.add(&[up, dn]), fx.int(1)))?;
    Ok(fx.num(v).map(|_| v))
}

/// pFq(a; b; z) evaluated; for p = q + 1 and |z| > 1 (where the series diverges) the continuation of DLMF 16.8.8,
/// sum_j (prod Gamma(b)/prod Gamma(a)) Gamma(a_j) prod_(k != j) Gamma(a_k - a_j)/prod Gamma(b_k - a_j) (-z)^(-a_j)
/// (q+1)F(q)(a_j, 1 - b + a_j; 1 - a_k + a_j (k != j); 1/z), when no two a differ by an integer.
fn pfq<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>], b: &[T<'h>], z: T<'h>) -> Res<T<'h>> {
    let direct = |fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>], b: &[T<'h>], z: T<'h>| -> Res<T<'h>> {
        let c = fx.call("hypergeometric_pfq", &[fx.list(a), fx.list(b), z]);
        let v = fx.eval(c)?;
        if fx.num(v).is_some() {
            return Ok(v);
        }
        crate::cover::numify(fx, v, &[z])
    };
    if a.len() != b.len() + 1 || mag(fx, z) <= 1.0 {
        return direct(fx, a, b, z);
    }
    let fs: Vec<Option<f64>> = a.iter().map(|&t| fx.f64_of(t)).collect();
    for i in 0..a.len() {
        for j in 0..i {
            if let (Some(x), Some(y)) = (fs[i], fs[j]) {
                if ((x - y) - (x - y).round()).abs() < 1e-12 {
                    return direct(fx, a, b, z);
                }
            }
        }
    }
    let g = |fx: &Fx<'_, '_, '_, 'h>, t: T<'h>| fx.call("gamma", &[t]);
    let mut pre = vec![];
    for &bk in b {
        pre.push(g(fx, bk));
    }
    for &ak in a {
        pre.push(fx.pow(g(fx, ak), fx.int(-1)));
    }
    let pre = fx.mul(&pre);
    let mz = fx.neg(z);
    let iz = fx.div(fx.int(1), z);
    let mut terms = vec![];
    for j in 0..a.len() {
        let aj = a[j];
        let mut c = vec![g(fx, aj)];
        let mut na = vec![aj];
        let mut nb = vec![];
        for (k, &ak) in a.iter().enumerate() {
            if k != j {
                c.push(g(fx, fx.sub(ak, aj)));
                nb.push(fx.add(&[fx.sub(fx.int(1), ak), aj]));
            }
        }
        for &bk in b {
            c.push(fx.pow(g(fx, fx.sub(bk, aj)), fx.int(-1)));
            na.push(fx.add(&[fx.sub(fx.int(1), bk), aj]));
        }
        c.push(fx.pow(mz, fx.neg(aj)));
        let mut nae = vec![];
        for t in na {
            nae.push(fx.eval(t)?);
        }
        let mut nbe = vec![];
        for t in nb {
            nbe.push(fx.eval(t)?);
        }
        let f = direct(fx, &nae, &nbe, iz)?;
        c.push(f);
        terms.push(fx.mul(&c));
    }
    let v = fx.eval(fx.mul(&[pre, fx.add(&terms)]))?;
    if fx.num(v).is_some() {
        return Ok(v);
    }
    crate::cover::numify(fx, v, &[z])
}

/// The definitions of this module.
pub static DEFS: &[Def] = &[
    Def { name: "q_hypergeometric_pfq", arities: &[4], listable: false, exact: Some(q_hypergeometric_pfq), numeric: None, summary: "q_hypergeometric_pfq([a1, ..., ar], [b1, ..., bs], q, z): the basic hypergeometric series r phi s" },
    Def { name: "bilateral_hypergeometric_pfq", arities: &[3], listable: false, exact: Some(bilateral_hypergeometric_pfq), numeric: None, summary: "bilateral_hypergeometric_pfq([a1, ..., ap], [b1, ..., bq], z): the bilateral hypergeometric series, the sum over all integers n of prod (a_i)_n/prod (b_j)_n z^n" },
];
