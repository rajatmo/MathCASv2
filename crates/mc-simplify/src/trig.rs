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

/// Most rounds of a rewriting run to its fixed point.
const ROUNDS: usize = 64;

/// The one-argument call of `s` whose argument is `u`, if `t` is a call of one of `of`.
fn which<'h>(b: &B<'h>, t: T<'h>, of: &[Sym]) -> Option<(Sym, T<'h>)> {
    match b.call_of(t) {
        Some((s, [u])) if of.contains(&s) => Some((s, *u)),
        _ => None,
    }
}

/// `trig_to_exp(e)`: trigonometric and hyperbolic functions as exponentials.
pub fn trig_to_exp<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, t: T<'h>) -> Res<T<'h>> {
    let hd = &b.hd;
    let of = [hd.sin, hd.cos, hd.tan, hd.cot, hd.sec, hd.csc, hd.sinh, hd.cosh, hd.tanh];
    let r = rewrite(cx, b, t, &mut |_, b, node| {
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
    cx.eval(r)
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
    let r = rewrite(cx, b, t, &mut |_, b, node| {
        let Some(u) = b.exp_arg(node) else { return Ok(None) };
        let (re, im) = split(b, u);
        if im == b.int(0) {
            return Ok(None);
        }
        let hd = &b.hd;
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
    apply(cx, b, "expand", r)
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

/// One product-to-sum step on a product (or power) with at least two sine or cosine factors.
fn product_to_sum<'h>(b: &B<'h>, node: T<'h>) -> Option<T<'h>> {
    let hd = &b.hd;
    let is_mul = matches!(b.call_of(node), Some((s, _)) if s == hd.mul);
    let is_pow = matches!(b.call_of(node), Some((s, _)) if s == hd.pow);
    if !is_mul && !is_pow {
        return None;
    }
    // the sine and cosine factors, one entry per unit of power, and the other factors
    let mut trig: Vec<(Sym, T<'h>)> = Vec::new();
    let mut rest: Vec<T<'h>> = Vec::new();
    for f in b.factors(node) {
        let (base, k) = b.int_power(f);
        match which(b, base, &[hd.sin, hd.cos]) {
            Some(su) if k >= 1 => trig.extend(std::iter::repeat_n(su, k as usize)),
            _ => rest.push(f),
        }
    }
    if trig.len() < 2 {
        return None;
    }
    let (s1, a) = trig[0];
    let (s2, c) = trig[1];
    rest.extend(trig[2..].iter().map(|&(s, u)| b.call(s, &[u])));
    let f = |g: Sym, x: T<'h>| b.call(g, &[x]);
    let (dif, sum) = (b.sub(a, c), b.add(&[a, c]));
    let half = b.rat(1, 2);
    let p2s = match (s1 == hd.sin, s2 == hd.sin) {
        // sin a sin c = (cos(a - c) - cos(a + c))/2
        (true, true) => b.sub(f(hd.cos, dif), f(hd.cos, sum)),
        // cos a cos c = (cos(a - c) + cos(a + c))/2
        (false, false) => b.add(&[f(hd.cos, dif), f(hd.cos, sum)]),
        // sin a cos c = (sin(a + c) + sin(a - c))/2
        (true, false) => b.add(&[f(hd.sin, sum), f(hd.sin, dif)]),
        // cos a sin c = (sin(a + c) - sin(a - c))/2
        (false, true) => b.sub(f(hd.sin, sum), f(hd.sin, dif)),
    };
    rest.push(half);
    rest.push(p2s);
    Some(b.mul(&rest))
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
