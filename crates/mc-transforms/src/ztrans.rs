//! The Z transform `Z[f](z) = sum(f(n) z^-n, [n, 0, inf])` and its inverse.
//!
//! Forward, in order: linearity (undefined functions stay as transforms); the rules for undefined functions (the
//! shifts `f(n + k)` and `f(n - k)`, `conjugate`); `discrete_delta(n - k)` and `kronecker_delta(n, k)`; steps
//! `unit_step(n - k) g(n)` as `z^-k Z[g(n + k)]`; the multiplication rules `Z[a^n g] = G(z/a)`, `Z[exp(i w n) g]
//! = G(exp(-i w) z)` and `Z[n g] = -z G'(z)`; trigonometric functions of linear arguments as exponentials;
//! factorial powers and Pochhammer symbols of `n` as polynomials; then `sum`. Jury, *Theory and Application of the
//! z-Transform Method* (1964), chapter 1; the conditions of convergence `abs(z) > r` are those of the geometric
//! series of the factors `a^n`.
//!
//! Inverse: rational functions of `z` by the residues of `F(z) z^(n - 1)` at the nonzero poles (Jury, section
//! 1.6) and, at `z = 0`, the Laurent coefficients of `F(z)/z` as `discrete_delta(n - j)`; linearity; the inverse
//! of a transform of an undefined function.
use super::verify;
use super::{factors, held, linear, split_constant, terms, unknown_function, with_condition, Cx, Kind, MAX_DEPTH};
use super::call_of;
use mc_calculus::common::is_failure;
use mc_limit::bounds::{sign, subst};
use mc_calculus::common::{apply, args_of, is_free};
use mc_calculus::term::Build;
use mc_api::{Store, View, T};
use mc_eval::{NativeCx, Res};

/// The Z transform of `f` in `n` at `z`, or `None`.
pub fn z_transform<'h>(cx: &mut NativeCx<'_, '_, 'h>, tc: &Cx<'_, 'h>, f: T<'h>, n: T<'h>, z: T<'h>) -> Res<Option<T<'h>>> {
    let b = tc.b;
    let h = b.heap();
    // a term at n = 0 that is infinite (1/n): the sum starts at n = 0, so there is no transform (D23-15)
    if !unknown_function(cx, b, f, n) {
        let f0 = subst(cx, b, f, n, b.zero())?;
        let f0 = cx.eval(f0)?;
        if is_failure(b, f0) || ["complex_infinity", "indeterminate", "infinity"].iter().any(|s| !is_free(b, f0, h.sym_term(h.sym(s)))) {
            return Ok(None);
        }
    }
    let mut radius = vec![];
    let mut checked = true;
    let Some(v) = zt(cx, tc, f, n, z, &mut radius, &mut checked, 0)? else { return Ok(None) };
    let v = cx.eval(v)?;
    if is_failure(b, v) || !super::free_outside(b, v, n) {
        return Ok(None);
    }
    let v = super::laplace::tidy(cx, b, v)?;
    if !checked && verify::z(cx, b, f, n, z, v)? == verify::Verdict::Refuted {
        return Ok(None);
    }
    let cond = if radius.is_empty() || super::has_held(b, v, Kind::Z) {
        None
    } else {
        let r = if radius.len() == 1 { radius[0] } else { b.call("max", &radius) };
        Some(cx.eval(h.call(h.sym("greater"), &[b.call("abs", &[z]), r]))?)
    };
    Ok(Some(with_condition(cx, tc, v, cond)?))
}

#[allow(clippy::too_many_arguments)]
fn zt<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    tc: &Cx<'_, 'h>,
    f: T<'h>,
    n: T<'h>,
    z: T<'h>,
    radius: &mut Vec<T<'h>>,
    checked: &mut bool,
    depth: u32,
) -> Res<Option<T<'h>>> {
    let b = tc.b;
    let h = b.heap();
    if depth > MAX_DEPTH {
        return Ok(None);
    }
    cx.tick(1)?;
    if is_free(b, f, n) {
        push_radius(cx, b, radius, b.one())?;
        return Ok(Some(cx.eval(b.mul(&[f, b.div(z, b.sub(z, b.one()))]))?));
    }
    // linearity
    let ts = terms(b, f);
    if ts.len() > 1 {
        let mut out = vec![];
        for g in ts {
            match zt(cx, tc, g, n, z, radius, checked, depth + 1)? {
                Some(r) => out.push(r),
                None => {
                    let (c, rest) = split_constant(b, g, n);
                    if unknown_function(cx, b, rest, n) {
                        out.push(b.mul(&[c, held(b, Kind::Z, rest, n, z)]));
                    } else {
                        return Ok(None);
                    }
                }
            }
        }
        return Ok(Some(b.add(&out)));
    }
    let (c, rest) = split_constant(b, f, n);
    if c != b.one() {
        return Ok(zt(cx, tc, rest, n, z, radius, checked, depth + 1)?.map(|r| b.mul(&[c, r])));
    }
    // products of polynomials in n: expanded
    if super::inverse::rational_in(b, f, n) && args_of(b, f, b.hd.mul).is_some() {
        let e = apply(cx, b, "expand", &[f])?;
        if e != f && args_of(b, e, b.hd.add).is_some() {
            return zt(cx, tc, e, n, z, radius, checked, depth + 1);
        }
    }
    let fs = factors(b, f);
    // undefined functions: f(n), the shifts f(n + k), conjugate(f(n))
    if fs.len() == 1 {
        if let Some(r) = undefined(cx, tc, f, n, z, radius, checked, depth)? {
            return Ok(Some(r));
        }
    }
    // named sequences (D21-16): fibonacci(n), harmonic_number(n), 1/n and sin(n)/n (the last two from n = 1, their
    // n = 0 terms being undefined), from the generating functions (Graham, Knuth and Patashnik, *Concrete
    // Mathematics*, 7.1-7.3)
    {
        let w = b.powi(z, -1);
        let lz = b.call("log", &[b.div(z, b.sub(z, b.one()))]);
        let one = b.one();
        let named = match call_of(b, f) {
            Some((name, [u])) if *u == n && name == "fibonacci" => {
                let phi = b.div(b.add(&[one, b.pow(b.int(5), b.rat(1, 2))]), b.int(2));
                Some((b.div(z, b.add(&[b.pow(z, b.int(2)), b.neg(z), b.int(-1)])), phi, true))
            }
            Some((name, [u])) if *u == n && name == "harmonic_number" => Some((b.mul(&[b.div(z, b.sub(z, one)), lz]), one, true)),
            _ => None,
        };
        let named = named.or_else(|| {
            if args_of(b, f, b.hd.pow) == Some(&[n, b.int(-1)][..]) {
                return Some((lz, one, false));
            }
            let fs = factors(b, f);
            let is_sin_n = |g: T<'h>| matches!(call_of(b, g), Some((nm, [u])) if nm == "sin" && *u == n);
            if fs.len() == 2 && fs.iter().any(|&g| is_sin_n(g)) && fs.iter().any(|&g| args_of(b, g, b.hd.pow) == Some(&[n, b.int(-1)][..])) {
                let s1 = b.call("sin", &[one]);
                let c1 = b.call("cos", &[one]);
                let _ = w;
                return Some((b.call("atan", &[b.div(s1, b.sub(z, c1))]), one, false));
            }
            None
        });
        if let Some((v, r, check)) = named {
            push_radius(cx, b, radius, r)?;
            if check {
                *checked = false;
            }
            return Ok(Some(cx.eval(v)?));
        }
    }
    // deltas
    if let Some((name, args)) = call_of(b, f) {
        let k = match (name.as_str(), args) {
            ("discrete_delta", [u]) => linear(cx, b, *u, n)?.and_then(|(a, d)| (a == b.one()).then(|| b.neg(d))),
            ("kronecker_delta", [u, v]) | ("kronecker_delta", [v, u]) if is_free(b, *v, n) && linear(cx, b, *u, n)?.is_some_and(|(a, _)| a == b.one()) => {
                let (_, d) = linear(cx, b, *u, n)?.unwrap();
                Some(b.sub(*v, d))
            }
            ("kronecker_delta", [u]) => linear(cx, b, *u, n)?.and_then(|(a, d)| (a == b.one()).then(|| b.neg(d))),
            _ => None,
        };
        if let Some(k) = k {
            let k = cx.eval(k)?;
            return Ok(match h.view(k) {
                View::Int(j) if j >= 0 => Some(b.powi(z, -j)),
                View::Int(_) => Some(b.zero()),
                _ => None,
            });
        }
    }
    // steps unit_step(n - k): z^-k Z[g(n + k)]
    for (i, &g) in fs.iter().enumerate() {
        let Some((name, [u])) = call_of(b, g) else { continue };
        if name != "unit_step" {
            continue;
        }
        let Some((a, d)) = linear(cx, b, *u, n)? else { continue };
        if a != b.one() {
            continue;
        }
        let k = cx.eval(b.neg(d))?;
        let mut rest = fs.clone();
        rest.remove(i);
        let rest = cx.eval(b.mul(&rest))?;
        return match h.view(k) {
            View::Int(j) if j <= 0 => zt(cx, tc, rest, n, z, radius, checked, depth + 1),
            View::Int(j) => {
                let shifted = subst(cx, b, rest, n, b.add(&[n, b.int(j)]))?;
                Ok(zt(cx, tc, shifted, n, z, radius, checked, depth + 1)?.map(|r| b.mul(&[b.powi(z, -j), r])))
            }
            _ => Ok(None),
        };
    }
    // a^n g(n): G(z/a); exp(c n) g(n): G(exp(-c) z)
    for (i, &g) in fs.iter().enumerate() {
        let base = match args_of(b, g, b.hd.pow) {
            Some(&[a, e]) if is_free(b, a, n) => {
                let Some((k, d)) = linear(cx, b, e, n)? else { continue };
                Some((cx.eval(b.pow(a, k))?, cx.eval(b.pow(a, d))?))
            }
            _ => match call_of(b, g) {
                Some((name, [u])) if name == "exp" => {
                    let Some((k, d)) = linear(cx, b, *u, n)? else { continue };
                    Some((cx.eval(b.call("exp", &[k]))?, cx.eval(b.call("exp", &[d]))?))
                }
                _ => None,
            },
        };
        let Some((a, c0)) = base else { continue };
        let mut rest = fs.clone();
        rest.remove(i);
        let rest = cx.eval(b.mul(&rest))?;
        let zv = h.sym_term(h.sym("transform·z"));
        let mut inner_r = vec![];
        let Some(r) = zt(cx, tc, rest, n, zv, &mut inner_r, checked, depth + 1)? else { return Ok(None) };
        let r = cx.eval(r)?;
        let r = subst(cx, b, r, zv, b.div(z, a))?;
        let aa = cx.eval(b.call("abs", &[a]))?;
        for q in inner_r {
            push_radius(cx, b, radius, b.mul(&[q, aa]))?;
        }
        *checked = false;
        return Ok(Some(b.mul(&[c0, apply(cx, b, "together", &[r])?])));
    }
    // n^k alone, k > 10: sum_j j! S(k, j) z/(z - 1)^(j + 1) (Stirling numbers of the second kind: Graham, Knuth and
    // Patashnik, *Concrete Mathematics*, 6.10 and 7.4), instead of k derivatives
    if fs.len() == 1 {
        if let Some(&[base, e]) = args_of(b, f, b.hd.pow) {
            if base == n {
                if let View::Int(k @ 11..=400) = h.view(e) {
                    let k = k as usize;
                    // row k of the Stirling numbers by S(m, j) = j S(m - 1, j) + S(m - 1, j - 1)
                    let mut row: Vec<T<'h>> = vec![b.one()];
                    for _ in 1..=k {
                        let mut next = Vec::with_capacity(row.len() + 1);
                        for j in 0..=row.len() {
                            let a = if j < row.len() { b.mul(&[b.int(j as i64), row[j]]) } else { b.zero() };
                            let c = if j > 0 { row[j - 1] } else { b.zero() };
                            next.push(cx.eval(b.add(&[a, c]))?);
                        }
                        row = next;
                    }
                    let zm = b.sub(z, b.one());
                    let mut fact = b.one();
                    let mut num = vec![];
                    for (j, &s) in row.iter().enumerate() {
                        if j > 0 {
                            fact = cx.eval(b.mul(&[fact, b.int(j as i64)]))?;
                        }
                        if j >= 1 || k == 0 {
                            num.push(b.mul(&[fact, s, b.powi(zm, (k - j) as i64)]));
                        }
                    }
                    let num = apply(cx, b, "expand", &[b.mul(&[z, b.add(&num)])])?;
                    *checked = false;
                    push_radius(cx, b, radius, b.one())?;
                    return Ok(Some(cx.eval(b.div(num, b.powi(zm, k as i64 + 1)))?));
                }
            }
        }
    }
    // n^k g(n): (-z d/dz)^k G
    for (i, &g) in fs.iter().enumerate() {
        let k = if g == n {
            1
        } else {
            match args_of(b, g, b.hd.pow) {
                Some(&[base, e]) if base == n => match h.view(e) {
                    View::Int(k @ 1..=10) => k,
                    _ => continue,
                },
                _ => continue,
            }
        };
        let mut rest = fs.clone();
        rest.remove(i);
        let rest = cx.eval(b.mul(&rest))?;
        let Some(mut r) = zt(cx, tc, rest, n, z, radius, checked, depth + 1)? else { return Ok(None) };
        for _ in 0..k {
            let re = cx.eval(r)?;
            let d = apply(cx, b, "d", &[re, z])?;
            r = b.neg(b.mul(&[z, d]));
        }
        *checked = false;
        return Ok(Some(apply(cx, b, "together", &[r])?));
    }
    // trigonometric functions of linear arguments as exponentials
    if fs.iter().any(|&g| matches!(call_of(b, g), Some((name, [_])) if matches!(name.as_str(), "sin" | "cos" | "sinh" | "cosh"))) {
        let e = apply(cx, b, "trig_to_exp", &[f])?;
        let e = apply(cx, b, "expand", &[e])?;
        if e != f {
            let Some(r) = zt(cx, tc, e, n, z, radius, checked, depth + 1)? else { return Ok(None) };
            let r = cx.eval(r)?;
            let r = apply(cx, b, "together", &[r])?;
            return Ok(Some(super::inverse::trig_form(cx, b, r)?));
        }
    }
    // factorial powers and Pochhammer symbols of n with integer orders: polynomials
    if let Some((name, [u, m])) = call_of(b, f) {
        if matches!(name.as_str(), "factorial_power" | "pochhammer") && *u == n && matches!(h.view(*m), View::Int(0..=12)) {
            let View::Int(m) = h.view(*m) else { unreachable!() };
            let p: Vec<T<'h>> = (0..m).map(|j| if name == "pochhammer" { b.add(&[n, b.int(j)]) } else { b.sub(n, b.int(j)) }).collect();
            let poly = apply(cx, b, "expand", &[b.mul(&p)])?;
            return zt(cx, tc, poly, n, z, radius, checked, depth + 1);
        }
    }
    if unknown_function(cx, b, f, n) {
        return Ok(Some(held(b, Kind::Z, f, n, z)));
    }
    // the sum
    let inf = h.sym_term(h.sym("inf"));
    let g = cx.eval(b.mul(&[f, b.pow(z, b.neg(n))]))?;
    let r = apply(cx, b, "sum", &[g, h.call(b.hd.list, &[n, b.zero(), inf])])?;
    if !is_free(b, r, h.sym_term(h.sym("sum"))) || is_failure(b, r) {
        return Ok(None);
    }
    *checked = false;
    Ok(Some(r))
}

fn push_radius<'h>(cx: &mut NativeCx<'_, '_, 'h>, _b: &Build<'h>, radius: &mut Vec<T<'h>>, r: T<'h>) -> Res<()> {
    let r = cx.eval(r)?;
    if !radius.contains(&r) {
        radius.push(r);
    }
    Ok(())
}

/// The rules for an undefined `f`: `f(n)` stays, `f(n + k)` is `z^k (F - sum(f(j) z^-j, j < k))`, `f(n - k)` is
/// `z^-k F + sum(f(-j) z^(j - k), j = 1..k)`; `conjugate(g)` is `conjugate(G(conjugate(z)))`.
#[allow(clippy::too_many_arguments)]
fn undefined<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    tc: &Cx<'_, 'h>,
    f: T<'h>,
    n: T<'h>,
    z: T<'h>,
    radius: &mut Vec<T<'h>>,
    checked: &mut bool,
    depth: u32,
) -> Res<Option<T<'h>>> {
    let b = tc.b;
    let h = b.heap();
    if let Some((name, [g])) = call_of(b, f) {
        if name == "conjugate" {
            let zv = h.sym_term(h.sym("transform·z"));
            let Some(r) = zt(cx, tc, *g, n, zv, radius, checked, depth + 1)? else { return Ok(None) };
            let r = cx.eval(r)?;
            let r = subst(cx, b, r, zv, b.call("conjugate", &[z]))?;
            return Ok(Some(b.call("conjugate", &[r])));
        }
    }
    let View::App { head, args: &[u] } = h.view(f) else { return Ok(None) };
    let View::Sym(sy) = h.view(head) else { return Ok(None) };
    if cx.is_defined(sy) {
        return Ok(None);
    }
    if u == n {
        return Ok(Some(held(b, Kind::Z, f, n, z)));
    }
    let Some((a, d)) = linear(cx, b, u, n)? else { return Ok(None) };
    let View::Int(k) = h.view(d) else { return Ok(None) };
    if a != b.one() {
        return Ok(None);
    }
    let base = held(b, Kind::Z, b.app(head, &[n]), n, z);
    if k > 0 {
        let mut out = vec![b.mul(&[b.powi(z, k), base])];
        for j in 0..k {
            out.push(b.neg(b.mul(&[b.powi(z, k - j), b.app(head, &[b.int(j)])])));
        }
        return Ok(Some(b.add(&out)));
    }
    let k = -k;
    let mut out = vec![b.mul(&[b.powi(z, -k), base])];
    for j in 1..=k {
        out.push(b.mul(&[b.powi(z, j - k), b.app(head, &[b.int(-j)])]));
    }
    let _ = sign;
    Ok(Some(b.add(&out)))
}

/// The inverse Z transform of `big` in `z` at `n`, or `None`.
pub fn inverse_z<'h>(cx: &mut NativeCx<'_, '_, 'h>, tc: &Cx<'_, 'h>, big: T<'h>, z: T<'h>, n: T<'h>) -> Res<Option<T<'h>>> {
    let b = tc.b;
    let Some(v) = izt(cx, tc, big, z, n, 0)? else { return Ok(None) };
    // a non-causal shifted delta (from c z^k) has no causal forward transform to check against
    if !is_free(b, v, b.heap().sym_term(b.heap().sym("discrete_delta"))) && super::inverse::rational_in(b, big, z) {
        return Ok(Some(cx.eval(v)?));
    }
    let v = cx.eval(v)?;
    if is_failure(b, v) || !super::free_outside(b, v, z) {
        return Ok(None);
    }
    let v = super::laplace::tidy(cx, b, v)?;
    // the forward transform of the value at a sample point
    if verify::z(cx, b, v, n, z, big)? == verify::Verdict::Refuted {
        return Ok(None);
    }
    Ok(Some(v))
}

fn izt<'h>(cx: &mut NativeCx<'_, '_, 'h>, tc: &Cx<'_, 'h>, big: T<'h>, z: T<'h>, n: T<'h>, depth: u32) -> Res<Option<T<'h>>> {
    let b = tc.b;
    let h = b.heap();
    if depth > MAX_DEPTH {
        return Ok(None);
    }
    cx.tick(1)?;
    // c z^k (k >= 1): the non-causal sequence c discrete_delta(n + k); exp(c/z): c^n/n!; log(z/(z - 1)):
    // unit_step(n - 1)/n (D21-16)
    {
        let (c, rest) = split_constant(b, big, z);
        let k = if rest == z {
            Some(1)
        } else {
            match args_of(b, rest, b.hd.pow) {
                Some(&[base, e]) if base == z => match h.view(e) {
                    View::Int(k) if k >= 1 => Some(k),
                    _ => None,
                },
                _ => None,
            }
        };
        if let Some(k) = k {
            return Ok(Some(b.mul(&[c, b.call("discrete_delta", &[b.add(&[n, b.int(k)])])])));
        }
        if let Some((name, [u])) = call_of(b, rest) {
            if name == "exp" {
                let a = cx.eval(b.mul(&[*u, z]))?;
                if is_free(b, a, z) {
                    return Ok(Some(cx.eval(b.mul(&[c, b.pow(a, n), b.powi(b.call("factorial", &[n]), -1)]))?));
                }
            }
        }
        let lz = cx.eval(b.call("log", &[b.div(z, b.sub(z, b.one()))]))?;
        if rest == lz {
            return Ok(Some(b.mul(&[c, b.call("unit_step", &[b.sub(n, b.one())]), b.powi(n, -1)])));
        }
    }
    if super::inverse::rational_in(b, big, z) {
        return rational(cx, b, big, z, n);
    }
    let ts = terms(b, big);
    if ts.len() > 1 {
        let mut out = vec![];
        for g in ts {
            let Some(r) = izt(cx, tc, g, z, n, depth + 1)? else { return Ok(None) };
            out.push(r);
        }
        return Ok(Some(b.add(&out)));
    }
    let (c, rest) = split_constant(b, big, z);
    if c != b.one() {
        return Ok(izt(cx, tc, rest, z, n, depth + 1)?.map(|r| b.mul(&[c, r])));
    }
    if let Some((name, [g, x, y])) = call_of(b, big) {
        if name == "z_transform" && *y == z {
            return Ok(Some(subst(cx, b, *g, *x, n)?));
        }
    }
    if unknown_function(cx, b, big, z) {
        return Ok(Some(held(b, Kind::InverseZ, big, z, n)));
    }
    let _ = h;
    Ok(None)
}

/// A rational function of `z`: residues of `F(z) z^(n - 1)` at the nonzero poles, delta functions at 0.
fn rational<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, big: T<'h>, z: T<'h>, n: T<'h>) -> Res<Option<T<'h>>> {
    let h = b.heap();
    // G = F/z = num/den
    let g = apply(cx, b, "together", &[b.div(big, z)])?;
    let num = apply(cx, b, "numerator", &[g])?;
    let den = apply(cx, b, "denominator", &[g])?;
    let (Some(dn), Some(dd)) = (super::inverse::degree(cx, b, num, z)?, super::inverse::degree(cx, b, den, z)?) else { return Ok(None) };
    // a polynomial part of G: z^j terms of F z^-1 are anticausal; not handled
    if dn >= dd {
        return Ok(None);
    }
    let Some(ps) = super::inverse::poles(cx, b, den, z)? else { return Ok(None) };
    let mut out = vec![];
    for (p, m, q) in ps {
        cx.tick(1)?;
        if b.is_zero(p) {
            // the Laurent coefficients a_(-1-j) of G at 0, j < m: num/q at 0 and its derivatives
            let hz = b.mul(&[num, b.powi(q, -1)]);
            let mut cur = cx.eval(hz)?;
            let mut derivs = vec![cur];
            for _ in 1..m {
                cur = apply(cx, b, "d", &[cur, z])?;
                derivs.push(cur);
            }
            for j in 0..m {
                // a_(-(j+1)) = h^(m-1-j)(0)/(m-1-j)!
                let r = (m - 1 - j) as usize;
                let fact: i64 = (1..=r as i64).product();
                let v = subst(cx, b, derivs[r], z, b.zero())?;
                let v = cx.eval(b.div(v, b.int(fact.max(1))))?;
                if !b.is_zero(v) {
                    out.push(b.mul(&[v, b.call("discrete_delta", &[b.sub(n, b.int(j))])]));
                }
            }
            continue;
        }
        // the residue of num/((z - p)^m q) z^n at p
        let kern = b.pow(z, n);
        let gz = b.mul(&[num, b.powi(q, -1), kern]);
        let gz = if m > 1 {
            let dg = apply(cx, b, "d", &[gz, h.call(b.hd.list, &[z, b.int(m - 1)])])?;
            let fact: i64 = (1..m).product();
            b.div(dg, b.int(fact))
        } else {
            gz
        };
        let gz = cx.eval(gz)?;
        let r = subst(cx, b, gz, z, p)?;
        out.push(r);
    }
    let v = cx.eval(b.add(&out))?;
    let v = apply(cx, b, "simplify", &[v])?;
    Ok(Some(super::inverse::trig_form(cx, b, v)?))
}

/// Is `big` a rational function of `z` bounded at infinity (its inverse is a causal sequence: 0 for `n < 0`)?
pub(crate) fn causal_rational<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, big: T<'h>, z: T<'h>) -> Res<bool> {
    if !super::inverse::rational_in(b, big, z) {
        return Ok(false);
    }
    let g = apply(cx, b, "together", &[big])?;
    let num = apply(cx, b, "numerator", &[g])?;
    let den = apply(cx, b, "denominator", &[g])?;
    Ok(matches!((super::inverse::degree(cx, b, num, z)?, super::inverse::degree(cx, b, den, z)?), (Some(dn), Some(dd)) if dn <= dd))
}
