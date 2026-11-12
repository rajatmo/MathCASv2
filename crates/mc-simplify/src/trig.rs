//! Trigonometric and exponential rewritings: `trig_to_exp`, `exp_to_trig`, `trig_expand`, `trig_reduce`, and the
//! substitutions [`crate::simplify`] searches with (Pythagorean identities, `tan` as `sin/cos`).
//!
//! The identities, with `E± = exp(±i u)`:
//!
//! | function | as exponentials |
//! |---|---|
//! | `sin(u)` | `(E+ - E-)/(2i)` |
//! | `cos(u)` | `(E+ + E-)/2` |
//! | `tan(u)` | `-i (E+ - E-)/(E+ + E-)` |
//! | `sinh(u)`, `cosh(u)` | `(exp(u) ∓ exp(-u))/2` |
//!
//! and the inverse direction is Euler's formula `exp(a + b i) = exp(a) (cos(b) + i sin(b))`.
use crate::term::{apply, fixed, rewrite, B};
use mc_api::{Store, Sym, View, T};
use mc_eval::{NativeCx, Res};
use mc_arith::{Num, Real};

/// Most rounds of a rewriting run to its fixed point.
const ROUNDS: usize = 64;

/// The one-argument call of `s` whose argument is `u`, if `t` is a call of one of `of`.
fn which<'h>(b: &B<'h>, t: T<'h>, of: &[Sym]) -> Option<(Sym, T<'h>)> {
    match b.call_of(t) {
        Some((s, [u])) if of.contains(&s) => Some((s, *u)),
        _ => None,
    }
}

/// `trig_to_exp(e)`: trigonometric and hyperbolic functions as exponentials, their inverses as logarithms; the
/// result is expanded (sums distributed, a fraction split over its numerator's terms).
pub fn trig_to_exp<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, t: T<'h>) -> Res<T<'h>> {
    let hd = &b.hd;
    let h = b.heap();
    let inv: Vec<Sym> = ["asin", "acos", "atan", "acot", "asec", "acsc", "asinh", "acosh", "atanh", "acoth", "asech", "acsch"]
        .iter()
        .map(|n| h.sym(n))
        .collect();
    let of = [hd.sin, hd.cos, hd.tan, hd.cot, hd.sec, hd.csc, hd.sinh, hd.cosh, hd.tanh];
    let r = rewrite(cx, b, t, &mut |_, b, node| {
        if let Some((s, u)) = which(b, node, &inv) {
            return Ok(Some(inverse_as_log(b, h.sym_name(s).as_str(), u)));
        }
        let Some((s, u)) = which(b, node, &of) else { return Ok(None) };
        let hd = &b.hd;
        let iu = b.mul(&[b.i(), u]);
        let (ep, em) = (b.exp(iu), b.exp(b.neg(iu)));
        let (sum, diff) = (b.add(&[ep, em]), b.sub(ep, em));
        let r = if s == hd.sin {
            b.mul(&[b.complex(b.int(0), b.rat(-1, 2)), diff])
        } else if s == hd.cos {
            b.mul(&[b.rat(1, 2), sum])
        } else if s == hd.tan {
            b.mul(&[b.complex(b.int(0), b.int(-1)), diff, b.powi(sum, -1)])
        } else if s == hd.cot {
            b.mul(&[b.i(), sum, b.powi(diff, -1)])
        } else if s == hd.sec {
            b.mul(&[b.int(2), b.powi(sum, -1)])
        } else if s == hd.csc {
            b.mul(&[b.complex(b.int(0), b.int(2)), b.powi(diff, -1)])
        } else {
            let (ep, em) = (b.exp(u), b.exp(b.neg(u)));
            let (sum, diff) = (b.add(&[ep, em]), b.sub(ep, em));
            if s == hd.sinh {
                b.mul(&[b.rat(1, 2), diff])
            } else if s == hd.cosh {
                b.mul(&[b.rat(1, 2), sum])
            } else {
                b.mul(&[diff, b.powi(sum, -1)])
            }
        };
        Ok(Some(r))
    })?;
    let r = cx.eval(r)?;
    if r == t {
        return Ok(r);
    }
    apply(cx, b, "expand", r)
}

/// The logarithmic form of an inverse trigonometric or hyperbolic function of `u`.
fn inverse_as_log<'h>(b: &B<'h>, name: &str, u: T<'h>) -> T<'h> {
    let log = |x: T<'h>| b.call(b.hd.log, &[x]);
    let sqrt = |x: T<'h>| b.pow(x, b.rat(1, 2));
    let i = b.i();
    let half_i = b.complex(b.int(0), b.rat(1, 2));
    let half = b.rat(1, 2);
    let inv_u = b.powi(u, -1);
    // asin(v) = -i log(i v + sqrt(1 - v^2)), atan(v) = i/2 (log(1 - i v) - log(1 + i v))
    let asin = |v: T<'h>| b.mul(&[b.neg(i), log(b.add(&[b.mul(&[i, v]), sqrt(b.sub(b.int(1), b.powi(v, 2)))]))]);
    let atan = |v: T<'h>| {
        let iv = b.mul(&[i, v]);
        b.sub(b.mul(&[half_i, log(b.sub(b.int(1), iv))]), b.mul(&[half_i, log(b.add(&[b.int(1), iv]))]))
    };
    let pi_half = b.mul(&[half, b.heap().sym_term(b.heap().sym("pi"))]);
    let asinh = |v: T<'h>| log(b.add(&[v, sqrt(b.add(&[b.int(1), b.powi(v, 2)]))]));
    let acosh = |v: T<'h>| log(b.add(&[v, b.mul(&[sqrt(b.add(&[v, b.int(-1)])), sqrt(b.add(&[v, b.int(1)]))])]));
    let atanh = |v: T<'h>| b.sub(b.mul(&[half, log(b.add(&[b.int(1), v]))]), b.mul(&[half, log(b.sub(b.int(1), v))]));
    match name {
        "asin" => asin(u),
        "acos" => b.sub(pi_half, asin(u)),
        "atan" => atan(u),
        "acot" => atan(inv_u),
        "asec" => b.sub(pi_half, asin(inv_u)),
        "acsc" => asin(inv_u),
        "asinh" => asinh(u),
        "acosh" => acosh(u),
        "atanh" => atanh(u),
        "acoth" => atanh(inv_u),
        "asech" => acosh(inv_u),
        _ => asinh(inv_u),
    }
}

/// The real and imaginary parts of `u` read off its terms' complex coefficients: `(re, im)`, `im` zero when no term
/// has one. Symbols are taken as real (the split is formal).
fn split<'h>(b: &B<'h>, u: T<'h>) -> (T<'h>, T<'h>) {
    let (mut re, mut im) = (Vec::new(), Vec::new());
    for term in b.terms(u) {
        if let Some((p, q)) = b.complex_parts(term) {
            re.push(p);
            im.push(q);
            continue;
        }
        let fs = b.factors(term);
        match fs.iter().position(|&f| b.complex_parts(f).is_some()) {
            Some(k) => {
                let (p, q) = b.complex_parts(fs[k]).expect("a complex factor");
                let rest: Vec<T<'h>> = fs.iter().enumerate().filter(|&(j, _)| j != k).map(|(_, &f)| f).collect();
                let rest = b.mul(&rest);
                re.push(b.mul(&[p, rest]));
                im.push(b.mul(&[q, rest]));
            }
            None => re.push(term),
        }
    }
    (b.add(&re), b.add(&im))
}

/// `exp_to_trig(e)`: exponentials with an imaginary part in the exponent by Euler's formula.
pub fn exp_to_trig<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, t: T<'h>) -> Res<T<'h>> {
    let r = exp_to_trig_with(cx, b, t, false)?;
    // logarithms of the forms that are inverse functions (the search of `simplify` does not do this)
    let s = rewrite(cx, b, r, &mut |cx, b, node| log_to_inverse(cx, b, node))?;
    if s == r {
        Ok(r)
    } else {
        cx.eval(s)
    }
}

/// [`exp_to_trig`]; `real`: also exp(x) = cosh(x) + sinh(x) for a real exponent. Off everywhere for now: the
/// transforms, d_solve and simplify call exp_to_trig expecting exponentials of real arguments to stay (exp(-t)
/// would become cosh(t) - sinh(t)); a user-facing option can turn it on later (round 28 note)
pub fn exp_to_trig_with<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, t: T<'h>, real: bool) -> Res<T<'h>> {
    let r = rewrite(cx, b, t, &mut |_, b, node| {
        let Some(u) = b.exp_arg(node) else { return Ok(None) };
        let (re, im) = split(b, u);
        let hd = &b.hd;
        if im == b.int(0) {
            // a real exponent: exp(x) = cosh(x) + sinh(x) (numbers stay numbers)
            return Ok((real && !b.is_number(re)).then(|| b.add(&[b.call(hd.cosh, &[re]), b.call(hd.sinh, &[re])])));
        }
        let rot = b.add(&[b.call(hd.cos, &[im]), b.mul(&[b.i(), b.call(hd.sin, &[im])])]);
        Ok(Some(if re == b.int(0) { rot } else { b.mul(&[b.exp(re), rot]) }))
    })?;
    let r = cx.eval(r)?;
    apply(cx, b, "expand", r)
}

/// An angle split for the addition theorems: `u = a + r` (the first term of a sum and the rest), or `u = n x`
/// with an integer `n >= 2` as `x + (n - 1) x`.
fn angle_split<'h>(b: &B<'h>, u: T<'h>) -> Option<(T<'h>, T<'h>)> {
    let terms = b.terms(u);
    if terms.len() >= 2 {
        return Some((terms[0], b.add(&terms[1..])));
    }
    let fs = b.factors(u);
    match b.heap().view(*fs.first()?) {
        View::Int(n) if fs.len() >= 2 && n >= 2 => {
            let x = b.mul(&fs[1..]);
            Some((x, b.mul(&[b.int(n - 1), x])))
        }
        _ => None,
    }
}

/// `trig_expand(e)`: sines and cosines (circular and hyperbolic) of sums and integer multiples by the addition
/// theorems, `tan`, `cot`, `sec`, `csc` and `tanh` of such angles first written with sines and cosines; then
/// expanded.
pub fn trig_expand<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, t: T<'h>) -> Res<T<'h>> {
    let hd = &b.hd;
    let of = [hd.sin, hd.cos, hd.tan, hd.cot, hd.sec, hd.csc, hd.sinh, hd.cosh, hd.tanh];
    let r = fixed(cx, b, t, ROUNDS, &mut |_, b, node| {
        let Some((s, u)) = which(b, node, &of) else { return Ok(None) };
        let Some((a, r)) = angle_split(b, u) else { return Ok(None) };
        let hd = &b.hd;
        let f = |g: Sym, x: T<'h>| b.call(g, &[x]);
        Ok(Some(if s == hd.sin {
            b.add(&[b.mul(&[f(hd.sin, a), f(hd.cos, r)]), b.mul(&[f(hd.cos, a), f(hd.sin, r)])])
        } else if s == hd.cos {
            b.sub(b.mul(&[f(hd.cos, a), f(hd.cos, r)]), b.mul(&[f(hd.sin, a), f(hd.sin, r)]))
        } else if s == hd.sinh {
            b.add(&[b.mul(&[f(hd.sinh, a), f(hd.cosh, r)]), b.mul(&[f(hd.cosh, a), f(hd.sinh, r)])])
        } else if s == hd.cosh {
            b.add(&[b.mul(&[f(hd.cosh, a), f(hd.cosh, r)]), b.mul(&[f(hd.sinh, a), f(hd.sinh, r)])])
        } else {
            sin_cos_form(b, s, u)
        }))
    })?;
    let r = apply(cx, b, "expand", r)?;
    let r = rewrite(cx, b, r, &mut |_, b, node| Ok(pythagorean_pairs(b, node)))?;
    cx.eval(r)
}

/// A sum with terms `c sin(u)^2` and `c cos(u)^2` (the same `c` and `u`): the pair is `c`; with `c cosh(u)^2` and
/// `-c sinh(u)^2` it is `c` too.
fn pythagorean_pairs<'h>(b: &B<'h>, node: T<'h>) -> Option<T<'h>> {
    let hd = &b.hd;
    let (g, terms) = b.call_of(node)?;
    if g != hd.add {
        return None;
    }
    // each term as (the factors besides one squared function, the function, its argument)
    let split = |t: T<'h>| -> Option<(T<'h>, Sym, T<'h>)> {
        let fs = b.factors(t);
        for (i, &f) in fs.iter().enumerate() {
            let (base, k) = b.int_power(f);
            if k != 2 {
                continue;
            }
            if let Some((s, u)) = which(b, base, &[hd.sin, hd.cos, hd.sinh, hd.cosh]) {
                let mut rest: Vec<T<'h>> = fs.to_vec();
                rest.remove(i);
                return Some((b.mul(&rest), s, u));
            }
        }
        None
    };
    let parts: Vec<Option<(T<'h>, Sym, T<'h>)>> = terms.iter().map(|&t| split(t)).collect();
    for i in 0..terms.len() {
        for j in (i + 1)..terms.len() {
            let (Some((ci, si, ui)), Some((cj, sj, uj))) = (parts[i], parts[j]) else { continue };
            if ui != uj {
                continue;
            }
            let sin_cos = (si == hd.sin && sj == hd.cos || si == hd.cos && sj == hd.sin) && ci == cj;
            // c cosh^2 - c sinh^2: the coefficients are opposite
            let hyp = (si == hd.cosh && sj == hd.sinh || si == hd.sinh && sj == hd.cosh) && b.mul(&[b.int(-1), ci]) == cj;
            if sin_cos || hyp {
                let c = if hyp && si == hd.sinh { cj } else { ci };
                let mut out: Vec<T<'h>> = terms.iter().enumerate().filter(|&(k, _)| k != i && k != j).map(|(_, &t)| t).collect();
                out.push(c);
                return Some(b.add(&out));
            }
        }
    }
    None
}

/// `tan`, `cot`, `sec`, `csc`, `tanh` of `u` written with sines and cosines.
fn sin_cos_form<'h>(b: &B<'h>, s: Sym, u: T<'h>) -> T<'h> {
    let hd = &b.hd;
    let f = |g: Sym| b.call(g, &[u]);
    if s == hd.tan {
        b.div(f(hd.sin), f(hd.cos))
    } else if s == hd.cot {
        b.div(f(hd.cos), f(hd.sin))
    } else if s == hd.sec {
        b.powi(f(hd.cos), -1)
    } else if s == hd.csc {
        b.powi(f(hd.sin), -1)
    } else {
        b.div(f(hd.sinh), f(hd.cosh))
    }
}

/// `tan`, `cot`, `sec`, `csc` and `tanh` written with sines and cosines, evaluated.
pub fn to_sin_cos<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, t: T<'h>) -> Res<T<'h>> {
    let hd = &b.hd;
    let of = [hd.tan, hd.cot, hd.sec, hd.csc, hd.tanh];
    let r = rewrite(cx, b, t, &mut |_, b, node| Ok(which(b, node, &of).map(|(s, u)| sin_cos_form(b, s, u))))?;
    cx.eval(r)
}

/// `trig_reduce(e)`: products and powers of sines and cosines as sums of sines and cosines of multiple angles, by
/// the product-to-sum identities, after expanding.
pub fn trig_reduce<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, t: T<'h>) -> Res<T<'h>> {
    let mut t = apply(cx, b, "expand", t)?;
    for _ in 0..ROUNDS {
        let r = rewrite(cx, b, t, &mut |cx, b, node| Ok(product_to_sum(b, node).or_else(|| odd_even(cx, b, node))))?;
        if r == t {
            break;
        }
        let r = cx.eval(r)?;
        let r = apply(cx, b, "expand", r)?;
        if r == t {
            break;
        }
        t = r;
    }
    Ok(t)
}

/// `trig_reduce(t, modulus: p)`: the reduction with each rational coefficient taken modulo the
/// integer p >= 2 (n/d as n d^-1 mod p; a coefficient whose denominator p divides stays), like terms combined and
/// reduced again, the common integer factor of the coefficients written outside (`4*(cos(48*x) + cos(50*x))`).
/// `None` when p is not an integer >= 2 (the call then returns `t` unchanged, as for a modulus that is no integer).
pub fn trig_reduce_mod<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, t: T<'h>, p: T<'h>) -> Res<Option<T<'h>>> {
    let h = b.heap();
    if !matches!(h.view(p), View::Int(k) if k >= 2) {
        return Ok(None);
    }
    let mut cur = trig_reduce(cx, b, t)?;
    let rational = |c: T<'h>| matches!(b.ar.num(c), Some(Num::Real(Real::Exact(_))));
    for _ in 0..2 {
        let mut out = Vec::new();
        for term in b.terms(cur) {
            let (nums, rest): (Vec<T<'h>>, Vec<T<'h>>) = b.factors(term).into_iter().partition(|&f| b.is_number(f));
            let c = cx.eval(b.mul(&nums))?;
            if !rational(c) {
                out.push(term);
                continue;
            }
            let inv = cx.eval(b.call_named("power_mod", &[b.call_named("denominator", &[c]), b.int(-1), p]))?;
            if !rational(inv) {
                out.push(term);
                continue;
            }
            let cm = cx.eval(b.call_named("mod", &[b.mul(&[b.call_named("numerator", &[c]), inv]), p]))?;
            out.push(b.mul(&[cm, b.mul(&rest)]));
        }
        cur = cx.eval(b.add(&out))?;
    }
    // the common integer factor of the coefficients
    let mut g: Option<T<'h>> = None;
    for term in b.terms(cur) {
        let nums: Vec<T<'h>> = b.factors(term).into_iter().filter(|&f| b.is_number(f)).collect();
        let c = cx.eval(b.mul(&nums))?;
        if !matches!(h.view(c), View::Int(_)) {
            return Ok(Some(cur));
        }
        g = Some(match g {
            None => c,
            Some(g0) => cx.eval(b.call_named("gcd", &[g0, c]))?,
        });
    }
    Ok(Some(match g {
        Some(g) if !matches!(h.view(g), View::Int(0 | 1)) => {
            let inner = cx.eval(b.call_named("expand", &[b.div(cur, g)]))?;
            b.mul(&[g, inner])
        }
        _ => cur,
    }))
}

/// The most sine and cosine factors (counted with their powers) [`product_to_sum`] rewrites: each step halves one
/// pair, so a product of n factors ends as about 2^(n/2) terms after `expand`; beyond this the rewrite only grows.
const MAX_TRIG_UNITS: u64 = 64;

/// One product-to-sum step on a product (or power) with at least two sine or cosine factors.
fn product_to_sum<'h>(b: &B<'h>, node: T<'h>) -> Option<T<'h>> {
    let hd = &b.hd;
    let is_mul = matches!(b.call_of(node), Some((s, _)) if s == hd.mul);
    let is_pow = matches!(b.call_of(node), Some((s, _)) if s == hd.pow);
    if !is_mul && !is_pow {
        return None;
    }
    // the sine and cosine factors (circular, then hyperbolic: the first family with two of them is rewritten), one
    // entry per unit of power, and the other factors; products of more than MAX_TRIG_UNITS sines and cosines are
    // left alone (sin(x)^(2^31) would need a list of 2^31 entries, 16 GB, and the allocation failure aborts the
    // process; found by the round 21 fuzz campaign)
    for (sn, cs, circular) in [(hd.sin, hd.cos, true), (hd.sinh, hd.cosh, false)] {
        let mut trig: Vec<(Sym, T<'h>)> = Vec::new();
        let mut rest: Vec<T<'h>> = Vec::new();
        for f in b.factors(node) {
            let (base, k) = b.int_power(f);
            match which(b, base, &[sn, cs]) {
                Some(su) if k >= 1 => {
                    if k as u64 + trig.len() as u64 > MAX_TRIG_UNITS {
                        return None;
                    }
                    trig.extend(std::iter::repeat_n(su, k as usize));
                }
                _ => rest.push(f),
            }
        }
        if trig.len() < 2 {
            continue;
        }
        let (s1, a) = trig[0];
        let (s2, c) = trig[1];
        rest.extend(trig[2..].iter().map(|&(s, u)| b.call(s, &[u])));
        let f = |g: Sym, x: T<'h>| b.call(g, &[x]);
        let (dif, sum) = (b.sub(a, c), b.add(&[a, c]));
        let half = b.rat(1, 2);
        let p2s = match (s1 == sn, s2 == sn) {
            // sin a sin c = (cos(a - c) - cos(a + c))/2; sinh a sinh c = (cosh(a + c) - cosh(a - c))/2
            (true, true) if circular => b.sub(f(cs, dif), f(cs, sum)),
            (true, true) => b.sub(f(cs, sum), f(cs, dif)),
            // cos a cos c = (cos(a - c) + cos(a + c))/2, and the same for cosh
            (false, false) => b.add(&[f(cs, dif), f(cs, sum)]),
            // sin a cos c = (sin(a + c) + sin(a - c))/2, and the same for sinh, cosh
            (true, false) => b.add(&[f(sn, sum), f(sn, dif)]),
            // cos a sin c = (sin(a + c) - sin(a - c))/2, and the same for cosh, sinh
            (false, true) => b.sub(f(sn, sum), f(sn, dif)),
        };
        rest.push(half);
        rest.push(p2s);
        return Some(b.mul(&rest));
    }
    None
}

/// Which Pythagorean substitution: write even powers of cosines in sines (`cos^2 = 1 - sin^2`, `cosh^2 = 1 +
/// sinh^2`), or of sines in cosines.
#[derive(Clone, Copy)]
pub enum Pythagoras {
    /// `cos^2 -> 1 - sin^2`, `cosh^2 -> 1 + sinh^2`
    ToSin,
    /// `sin^2 -> 1 - cos^2`, `sinh^2 -> cosh^2 - 1`
    ToCos,
}

/// Powers `f(u)^k` (k >= 2) of the replaced function written with the other one, evaluated.
pub fn pythagoras<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, t: T<'h>, way: Pythagoras) -> Res<T<'h>> {
    let r = rewrite(cx, b, t, &mut |_, b, node| {
        let (base, k) = b.int_power(node);
        if k < 2 {
            return Ok(None);
        }
        let hd = &b.hd;
        let (from, hfrom) = match way {
            Pythagoras::ToSin => (hd.cos, hd.cosh),
            Pythagoras::ToCos => (hd.sin, hd.sinh),
        };
        let Some((s, u)) = which(b, base, &[from, hfrom]) else { return Ok(None) };
        let sq = |g: Sym| b.powi(b.call(g, &[u]), 2);
        // the square of the replaced function in terms of the other one
        let square = match (way, s == from) {
            (Pythagoras::ToSin, true) => b.sub(b.int(1), sq(hd.sin)),
            (Pythagoras::ToSin, false) => b.add(&[b.int(1), sq(hd.sinh)]),
            (Pythagoras::ToCos, true) => b.sub(b.int(1), sq(hd.cos)),
            (Pythagoras::ToCos, false) => b.sub(sq(hd.cosh), b.int(1)),
        };
        let mut r = b.powi(square, k / 2);
        if k % 2 == 1 {
            r = b.mul(&[r, base]);
        }
        Ok(Some(r))
    })?;
    cx.eval(r)
}

/// The sign convention for sines and cosines of sums (decision 2B.4): of `f(u)` and `f(-u)` the one whose angle comes
/// first in the canonical order, so `sin(y - x)` becomes `-sin(x - y)` and the two cancel or collect. The evaluator
/// already normalises angles that are products with a negative coefficient (`sin(-x)` is `-sin(x)`); sums it leaves
/// as they are, so the two conventions never disagree.
pub fn odd_even<'h>(cx: &NativeCx<'_, '_, 'h>, b: &B<'h>, node: T<'h>) -> Option<T<'h>> {
    let hd = &b.hd;
    let odd = [hd.sin, hd.tan, hd.cot, hd.csc, hd.sinh, hd.tanh];
    let even = [hd.cos, hd.sec, hd.cosh];
    let (s, u) = match b.call_of(node) {
        Some((s, [u])) if odd.contains(&s) || even.contains(&s) => (s, *u),
        _ => return None,
    };
    let terms = b.terms(u);
    if terms.len() < 2 {
        return None;
    }
    // negated term by term: `-(x - y)` as the sum `-x + y`, not the product `-1*(x - y)`
    let negated: Vec<T<'h>> = terms.iter().map(|&a| b.neg(a)).collect();
    let nu = b.add(&negated);
    if cx.cmp(nu, u) != std::cmp::Ordering::Less {
        return None;
    }
    let f = b.call(s, &[nu]);
    Some(if odd.contains(&s) { b.neg(f) } else { f })
}

/// Sines and cosines of sums with the sign convention of [`odd_even`], evaluated.
pub fn normal_signs<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, t: T<'h>) -> Res<T<'h>> {
    let r = rewrite(cx, b, t, &mut |cx, b, node| Ok(odd_even(cx, b, node)))?;
    cx.eval(r)
}

/// Products of exponentials as one exponential: `exp(a)^k exp(b) = exp(k a + b)` (valid for all values, so
/// `simplify` may use it), evaluated.
pub fn exp_combine<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, t: T<'h>) -> Res<T<'h>> {
    let r = rewrite(cx, b, t, &mut |_, b, node| {
        if !matches!(b.call_of(node), Some((s, _)) if s == b.hd.mul) {
            return Ok(None);
        }
        let (mut exps, mut rest) = (Vec::new(), Vec::new());
        for f in b.factors(node) {
            let (base, k) = b.int_power(f);
            match b.exp_arg(base) {
                Some(u) => exps.push(b.mul(&[b.int(k), u])),
                None => rest.push(f),
            }
        }
        if exps.len() < 2 {
            return Ok(None);
        }
        rest.push(b.exp(b.add(&exps)));
        Ok(Some(b.mul(&rest)))
    })?;
    cx.eval(r)
}

/// Logarithms written as inverse functions (what `exp_to_trig` does with `log`): `log(u + sqrt(u^2 + 1))` is
/// `asinh(u)`; `c (log(s + w) - log(s - w))` with a positive number `s` is `2 c atanh(w/s)`, which is
/// `2 i c atan(y)` when `w/s = i y` (the definition `atanh(z) = (log(1 + z) - log(1 - z))/2`, DLMF 4.37.24, and
/// `atanh(i y) = i atan(y)`, DLMF 4.37.6).
fn log_to_inverse<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, node: T<'h>) -> Res<Option<T<'h>>> {
    let hd = &b.hd;
    if let Some(a) = b.unary(node, hd.log) {
        // log(u + sqrt(u^2 + 1)) = asinh(u)
        let ts = b.terms(a);
        if let [t0, t1] = ts.as_slice() {
            for (u, q) in [(*t0, *t1), (*t1, *t0)] {
                let Some((p, &[base, e])) = b.call_of(q) else {
                    continue;
                };
                if p != hd.pow || e != b.rat(1, 2) {
                    continue;
                }
                let d = apply(cx, b, "expand", b.sub(base, b.add(&[b.mul(&[u, u]), b.int(1)])))?;
                if d == b.int(0) {
                    return Ok(Some(b.call_named("asinh", &[u])));
                }
            }
        }
        return Ok(None);
    }
    let Some((s, terms)) = b.call_of(node) else { return Ok(None) };
    if s != hd.add || terms.len() < 2 {
        return Ok(None);
    }
    let split = |t: T<'h>| -> Option<(T<'h>, T<'h>)> {
        let fs = b.factors(t);
        let i = fs.iter().position(|&f| b.unary(f, hd.log).is_some())?;
        let arg = b.unary(fs[i], hd.log)?;
        let mut rest = fs.clone();
        rest.remove(i);
        Some((b.mul(&rest), arg))
    };
    let parts: Vec<Option<(T<'h>, T<'h>)>> = terms.iter().map(|&t| split(t)).collect();
    for i in 0..terms.len() {
        for j in 0..terms.len() {
            let (Some((ci, ai)), Some((cj, aj))) = (parts[i], parts[j]) else { continue };
            if i == j || b.neg(ci) != cj {
                continue;
            }
            let half = b.rat(1, 2);
            let sum = apply(cx, b, "expand", b.mul(&[half, b.add(&[ai, aj])]))?;
            let diff = apply(cx, b, "expand", b.mul(&[half, b.sub(ai, aj)]))?;
            let positive = b.ar.num(sum).is_some() && b.ar.compare(sum, b.int(0)).is_some_and(|o| o == mc_arith::number::Cmp3::Greater);
            if !positive || diff == b.int(0) {
                continue;
            }
            let z = cx.eval(b.mul(&[diff, b.powi(sum, -1)]))?;
            let imag = b.factors(z).into_iter().find(|&f| matches!(b.complex_parts(f), Some((re, _)) if re == b.int(0)));
            let inv = match imag {
                Some(_) => {
                    let y = cx.eval(b.mul(&[z, b.complex(b.int(0), b.int(-1))]))?;
                    b.mul(&[b.i(), b.call_named("atan", &[y])])
                }
                None => b.call_named("atanh", &[z]),
            };
            let new = b.mul(&[b.int(2), ci, inv]);
            let mut out: Vec<T<'h>> = terms.iter().enumerate().filter(|&(k, _)| k != i && k != j).map(|(_, &t)| t).collect();
            out.push(new);
            return Ok(Some(b.add(&out)));
        }
    }
    Ok(None)
}
