//! Two moves of `simplify` on products of powers (round 17, decisions r17fix.1 and r17fix.2):
//!
//! - [`power_split`]: `b^(k + u)` as `b^k b^u` for an integer constant `k` of the exponent (`|k| <=` [`MAX_SHIFT`]).
//!   The canonical arithmetic keeps `2*2^(1 + n)` and `2^(2 + n)` apart (a number times a power of a number is not
//!   merged), so the terms of `2*2^(1 + n) - 2^(2 + n)` do not cancel; with the constant parts of the exponents
//!   moved into the coefficients both terms are `4*2^n` and the sum collects to `0`. The move is valid for every
//!   value (`b^(k + u) = b^k b^u` for an integer `k` and any `b != 0`; for `b = 0` both sides are undefined or `0`
//!   alike when `k > 0`, and the move is only made for nonzero number bases and symbols).
//! - [`gamma_ratio`]: `gamma(z + k)/gamma(z)` as the rising factorial `z (z + 1) ... (z + k - 1)` for an integer
//!   `k` (and the reciprocal for `k < 0`), with `factorial(u) = gamma(u + 1)`; past [`MAX_PRODUCT`] factors as
//!   `pochhammer(z, k)`. This is the recurrence `gamma(z + 1) = z gamma(z)` (DLMF 5.5.1) applied `k` times, valid
//!   wherever both sides are defined. The pairing works on the factors of one product: a numerator and a
//!   denominator gamma whose arguments differ by an integer. The same move writes `z gamma(z)` as `gamma(z + 1)`
//!   and `exp(log_gamma(u))` as `gamma(u)` (valid for every `u`: `log_gamma` is a logarithm of `gamma`).
use crate::term::{rewrite, B};
use mc_api::{Store, View, T};
use mc_eval::{NativeCx, Res};

/// The largest integer constant moved out of an exponent.
pub const MAX_SHIFT: i64 = 64;
/// Up to this many factors a gamma ratio is written as a product; above, as `pochhammer`.
pub const MAX_PRODUCT: i64 = 4;
/// The largest integer difference of gamma arguments paired.
pub const MAX_GAMMA_SHIFT: i64 = 1000;

/// The integer constant term of a sum (or of a lone integer), with the rest.
fn split_constant<'h>(b: &B<'h>, e: T<'h>) -> Option<(i64, T<'h>)> {
    let h = b.heap();
    let terms = b.terms(e);
    if terms.len() < 2 {
        return None;
    }
    let mut k = None;
    let mut rest = Vec::with_capacity(terms.len());
    for u in terms {
        match (k, h.view(u)) {
            (None, View::Int(v)) => k = Some(v),
            _ => rest.push(u),
        }
    }
    let k = k?;
    (k != 0 && k.abs() <= MAX_SHIFT).then(|| (k, b.add(&rest)))
}

/// `b^(k + u)` as `b^k b^u` everywhere in `t`, for integer constants `k` (see the module documentation).
pub fn power_split<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, t: T<'h>) -> Res<T<'h>> {
    let h = b.heap();
    if h.mask(t) & h.mask(h.sym_term(b.hd.pow)) == 0 {
        return Ok(t);
    }
    let r = rewrite(cx, b, t, &mut |_cx, b, node| {
        let Some((s, &[base, ex])) = b.call_of(node) else { return Ok(None) };
        if s != b.hd.pow || b.is_number(ex) {
            return Ok(None);
        }
        let h = b.heap();
        let nonzero_number = b.is_number(base) && !matches!(h.view(base), View::Int(0));
        if !(nonzero_number || matches!(h.view(base), View::Sym(_))) {
            return Ok(None);
        }
        let Some((k, rest)) = split_constant(b, ex) else { return Ok(None) };
        Ok(Some(b.mul(&[b.powi(base, k), b.pow(base, rest)])))
    })?;
    if r == t {
        Ok(t)
    } else {
        cx.eval(r)
    }
}

/// The gamma argument of a factor `gamma(a)^p` or `factorial(u)^p` (`a = u + 1`), with the integer power `p`.
fn gamma_factor<'h>(b: &B<'h>, f: T<'h>, gamma: mc_api::Sym, factorial: mc_api::Sym) -> Option<(T<'h>, i64)> {
    let (base, p) = b.int_power(f);
    let Some((s, &[u])) = b.call_of(base) else { return None };
    if s == gamma {
        Some((u, p))
    } else if s == factorial {
        Some((b.add(&[u, b.int(1)]), p))
    } else {
        None
    }
}

/// `z (z + 1) ... (z + k - 1)` for `k > 0`, as a product or `pochhammer(z, k)`.
fn rising<'h>(b: &B<'h>, z: T<'h>, k: i64) -> T<'h> {
    if k > MAX_PRODUCT {
        return b.call_named("pochhammer", &[z, b.int(k)]);
    }
    let fs: Vec<T<'h>> = (0..k).map(|i| b.add(&[z, b.int(i)])).collect();
    b.mul(&fs)
}

/// The integer `a - c`, if it is one (within [`MAX_GAMMA_SHIFT`]).
pub(crate) fn int_difference<'h>(b: &B<'h>, a: T<'h>, c: T<'h>) -> Option<i64> {
    // term by term: the canonical product -1*(u + v) is not distributed
    let mut ts = b.terms(a);
    ts.extend(b.terms(c).into_iter().map(|u| b.neg(u)));
    match b.heap().view(b.add(&ts)) {
        View::Int(k) if k.abs() <= MAX_GAMMA_SHIFT => Some(k),
        _ => None,
    }
}

/// Every product in `t` with a numerator `gamma(z + k)` and a denominator `gamma(z)` (integer `k`) rewritten by
/// the recurrence (see the module documentation).
pub fn gamma_ratio<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, t: T<'h>) -> Res<T<'h>> {
    let h = b.heap();
    let (gamma, factorial, log_gamma) = (h.sym("gamma"), h.sym("factorial"), h.sym("log_gamma"));
    let m = h.mask(h.sym_term(gamma)) | h.mask(h.sym_term(factorial)) | h.mask(h.sym_term(log_gamma));
    if h.mask(t) & m == 0 {
        return Ok(t);
    }
    let r = rewrite(cx, b, t, &mut |_cx, b, node| {
        // exp(k log_gamma(u) + v) = gamma(u)^k exp(v) for an integer k: log_gamma is a logarithm of gamma (it
        // differs from log(gamma(u)) by multiples of 2 pi i only), so exp(log_gamma(u)) = gamma(u) for every u
        if let Some(arg) = b.exp_arg(node) {
            let mut kept = vec![];
            let mut factors = vec![];
            for u in b.terms(arg) {
                let (k, l) = match b.call_of(u) {
                    Some((m, &[k, l])) if m == b.hd.mul => match b.heap().view(k) {
                        View::Int(k) => (k, l),
                        _ => (0, u),
                    },
                    _ => (1, u),
                };
                match b.unary(l, log_gamma) {
                    Some(z) if k != 0 => factors.push(b.powi(b.call(gamma, &[z]), k)),
                    _ => kept.push(u),
                }
            }
            if factors.is_empty() {
                return Ok(None);
            }
            factors.push(b.exp(b.add(&kept)));
            return Ok(Some(b.mul(&factors)));
        }
        let Some((s, args)) = b.call_of(node) else { return Ok(None) };
        if s != b.hd.mul {
            return Ok(None);
        }
        // z gamma(z) = gamma(z + 1) (DLMF 5.5.1), for a factor equal to the argument of a gamma in the product
        if let Some((i, j, z)) = args.iter().enumerate().find_map(|(i, &f)| match b.call_of(f) {
            Some((g, &[z])) if g == gamma => args.iter().position(|&x| x == z).map(|j| (i, j, z)),
            _ => None,
        }) {
            let mut out: Vec<T<'h>> = args.iter().enumerate().filter(|&(k, _)| k != i && k != j).map(|(_, &x)| x).collect();
            out.push(b.call(gamma, &[b.add(&[z, b.int(1)])]));
            return Ok(Some(b.mul(&out)));
        }
        let mut fs: Vec<Option<(T<'h>, i64)>> = args.iter().map(|&f| gamma_factor(b, f, gamma, factorial)).collect();
        let mut out: Vec<T<'h>> = Vec::new();
        let mut changed = false;
        for i in 0..fs.len() {
            let Some((a, p)) = fs[i] else { continue };
            if p != 1 {
                continue;
            }
            // a denominator gamma(c) with a - c an integer
            let j = (0..fs.len()).find(|&j| matches!(fs[j], Some((c, -1)) if int_difference(b, a, c).is_some()));
            let Some(j) = j else { continue };
            let Some((c, _)) = fs[j] else { continue };
            let Some(k) = int_difference(b, a, c) else { continue };
            // gamma(c + k)/gamma(c) = c (c + 1) ... (c + k - 1); for k < 0, 1/((c + k) ... (c - 1))
            out.push(if k >= 0 { rising(b, c, k) } else { b.pow(rising(b, b.add(&[c, b.int(k)]), -k), b.int(-1)) });
            fs[i] = None;
            fs[j] = None;
            changed = true;
        }
        if !changed {
            return Ok(None);
        }
        for (f, g) in args.iter().zip(&fs) {
            let used = gamma_factor(b, *f, gamma, factorial).is_some() && g.is_none();
            if !used {
                out.push(*f);
            }
        }
        Ok(Some(b.mul(&out)))
    })?;
    if r == t {
        Ok(t)
    } else {
        cx.eval(r)
    }
}
