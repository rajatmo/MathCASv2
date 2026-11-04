//! `refine(e)`: `e` rewritten by the identities that hold under the facts of the assumption context (semantics 13),
//! bottom up and evaluated, to a fixed point.
//!
//! | form | becomes | when |
//! |---|---|---|
//! | `(x^a)^c` | `x^(a c)` | `x >= 0`, `a`, `c` real; or `a` a real number in `(-1, 1]` (any `x`) |
//! | `(x^a)^c` | `abs(x)^(a c)` | `x` real, `a` an even integer, `c` real (so `sqrt(x^2)` is `abs(x)`) |
//! | `(p q)^c` | `p^c q^c` | for the factors `p >= 0` (`c` not an integer) |
//! | `abs(x)^n` | `x^n` | `x` real, `n` an even integer |
//! | `(-1)^k` | `1`, `-1` | `k` an even, odd integer (so `(-1)^(2 n)` is `1` for an integer `n`) |
//! | `abs(x)` | `x`, `-x` | `x >= 0`, `x <= 0`; factors of known sign leave a product |
//! | `sign(x)` | `1`, `-1`, `0` | `x > 0`, `x < 0`, `x == 0`; factors of known sign leave a product |
//! | `log(x^a)` | `a log(x)` | `x > 0` and `a` real; or `a` a real number in `(-1, 1]` |
//! | `log(p q)` | `log(p) + log(q)` | for the factors `p > 0` |
//! | `log(exp(u))` | `u` | `u` real |
//! | `sin(k pi)`, `tan(k pi)` | `0` | `k` an integer |
//! | `cos(k pi)` | `(-1)^k` | `k` an integer |
//! | `cos(k pi)`, `sin(k pi)` | `0`, `(-1)^(k - 1/2)` | `k - 1/2` an integer |
//! | `a < b`, `a <= b`, `a > b`, `a >= b`, `a == b`, `a != b` | `true`, `false` | decided by the facts |
//!
//! `exp(log(x)) = x` needs no fact: the evaluator applies it already.
use crate::facts::{Facts, Info, Kind};
use crate::term::{apply, fixed, B};
use mc_api::{Store, View, T};
use mc_eval::{NativeCx, Res};

/// Most rounds of rewriting and evaluation.
pub const ROUNDS: usize = 16;

/// `refine(e)` under `facts`.
pub fn refine<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, f: &mut Facts<'_, 'h>, t: T<'h>) -> Res<T<'h>> {
    fixed(cx, b, t, ROUNDS, &mut |cx, b, node| node_rule(cx, b, f, node))
}

/// Is `a` a real number in `(-1, 1]` (the exponents for which `(z^a)^c = z^(a c)` and `log(z^a) = a log(z)` hold
/// for every complex `z`)?
fn principal_exponent<'h>(b: &B<'h>, a: T<'h>) -> bool {
    let lt = |x: T<'h>, y: T<'h>| b.ar.compare(x, y).is_some_and(|c| c == mc_arith::number::Cmp3::Less);
    let le = |x: T<'h>, y: T<'h>| b.ar.compare(x, y).is_some_and(|c| matches!(c, mc_arith::number::Cmp3::Less | mc_arith::number::Cmp3::Equal));
    lt(b.int(-1), a) && le(a, b.int(1))
}

/// The replacement of one node, if a rule of the table applies.
fn node_rule<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, f: &mut Facts<'_, 'h>, node: T<'h>) -> Res<Option<T<'h>>> {
    let Some((s, args)) = b.call_of(node) else { return Ok(None) };
    let hd = &b.hd;
    let sy = f.syms();
    if let (true, [base, c]) = (s == hd.pow, args) {
        return power(cx, b, f, *base, *c);
    }
    if let (Some(_), [l, r]) = (sy.relation(s), args) {
        return Ok(sy.compare(f, cx, s, *l, *r)?.map(|v| cx.bool(v)));
    }
    let [u] = args else { return Ok(None) };
    let u = *u;
    if s == sy.abs {
        return signed_factors(cx, b, f, u, Outer::Abs);
    }
    if s == sy.sign {
        return signed_factors(cx, b, f, u, Outer::Sign);
    }
    if s == hd.log {
        return log(cx, b, f, u);
    }
    if s == hd.sin || s == hd.cos || s == hd.tan {
        return trig(cx, b, f, s, u);
    }
    Ok(None)
}

/// The rules for `base^c`.
fn power<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, f: &mut Facts<'_, 'h>, base: T<'h>, c: T<'h>) -> Res<Option<T<'h>>> {
    let hd = &b.hd;
    let h = b.heap();
    if matches!(h.view(base), View::Int(-1)) {
        return Ok(match f.parity(cx, c, 0)? {
            Some(true) => Some(b.int(1)),
            Some(false) => Some(b.int(-1)),
            None => None,
        });
    }
    let ic = f.info(cx, c)?;
    match b.call_of(base) {
        Some((g, [x, a])) if g == hd.pow => {
            let (x, a) = (*x, *a);
            let ix = f.info(cx, x)?;
            let ia = f.info(cx, a)?;
            if f.nonneg(&ix) && ia.is_real() && ic.is_real() || principal_exponent(b, a) {
                return Ok(Some(b.pow(x, b.mul(&[a, c]))));
            }
            if ix.is_real() && ia.kind == Kind::Integer && ic.is_real() && f.parity(cx, a, 0)? == Some(true) {
                let abs = b.call(f.syms().abs, &[x]);
                return Ok(Some(b.pow(abs, b.mul(&[a, c]))));
            }
            Ok(None)
        }
        Some((g, fs)) if g == hd.mul && ic.kind != Kind::Integer => {
            let mut out = vec![];
            let mut rest = vec![];
            for &p in fs {
                let ip = f.info(cx, p)?;
                if f.nonneg(&ip) {
                    out.push(b.pow(p, c));
                } else {
                    rest.push(p);
                }
            }
            if out.is_empty() {
                return Ok(None);
            }
            if !rest.is_empty() {
                out.push(b.pow(b.mul(&rest), c));
            }
            Ok(Some(b.mul(&out)))
        }
        // a sum under a fractional power: its factored form may be a power or a product the rules above take
        // (`sqrt(x^2 + 2*x + 1)` is `1 + x` for `x > -1`)
        Some((g, _)) if g == hd.add && ic.kind != Kind::Integer => {
            let factored = apply(cx, b, "factor", base)?;
            match b.call_of(factored) {
                Some((g, _)) if g == hd.pow || g == hd.mul => power(cx, b, f, factored, c),
                _ => Ok(None),
            }
        }
        _ => {
            let sy = f.syms();
            match b.unary(base, sy.abs) {
                Some(x) if ic.kind == Kind::Integer && f.info(cx, x)?.is_real() && f.parity(cx, c, 0)? == Some(true) => {
                    Ok(Some(b.pow(x, c)))
                }
                _ => Ok(None),
            }
        }
    }
}

/// `abs` or `sign` of a product.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Outer {
    Abs,
    Sign,
}

/// `abs(u)` or `sign(u)`: decided by the sign of `u`, or with the factors of known sign taken out of a product.
fn signed_factors<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, f: &mut Facts<'_, 'h>, u: T<'h>, outer: Outer) -> Res<Option<T<'h>>> {
    let iu = f.info(cx, u)?;
    let whole = match outer {
        Outer::Abs if f.nonneg(&iu) => Some(u),
        Outer::Abs if f.nonpos(&iu) => Some(negate(b, u)),
        Outer::Sign if f.positive(&iu) => Some(b.int(1)),
        Outer::Sign if f.negative(&iu) => Some(b.int(-1)),
        Outer::Sign if f.is_zero(&iu) => Some(b.int(0)),
        _ => None,
    };
    if whole.is_some() {
        return Ok(whole);
    }
    let Some((g, fs)) = b.call_of(u) else { return Ok(None) };
    if g != b.hd.mul {
        return Ok(None);
    }
    let mut out = vec![];
    let mut rest = vec![];
    let mut flips = 0u32;
    for &p in fs {
        let ip: Info<'h> = f.info(cx, p)?;
        let pos = if outer == Outer::Abs { f.nonneg(&ip) } else { f.positive(&ip) };
        let neg = if outer == Outer::Abs { f.nonpos(&ip) } else { f.negative(&ip) };
        if pos {
            if outer == Outer::Abs {
                out.push(p);
            }
        } else if neg {
            flips += 1;
            if outer == Outer::Abs {
                out.push(negate(b, p));
            }
        } else {
            rest.push(p);
        }
    }
    if rest.len() == fs.len() {
        return Ok(None);
    }
    let head = if outer == Outer::Abs { f.syms().abs } else { f.syms().sign };
    if !rest.is_empty() {
        out.push(b.call(head, &[b.mul(&rest)]));
    }
    if outer == Outer::Sign && flips % 2 == 1 {
        out.push(b.int(-1));
    }
    Ok(Some(b.mul(&out)))
}

/// The rules for `log(u)`.
fn log<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, f: &mut Facts<'_, 'h>, u: T<'h>) -> Res<Option<T<'h>>> {
    let hd = &b.hd;
    if let Some(v) = b.exp_arg(u) {
        return Ok(f.info(cx, v)?.is_real().then_some(v));
    }
    match b.call_of(u) {
        Some((g, [x, a])) if g == hd.pow => {
            let (x, a) = (*x, *a);
            let ix = f.info(cx, x)?;
            if f.positive(&ix) && f.info(cx, a)?.is_real() || principal_exponent(b, a) {
                return Ok(Some(b.mul(&[a, b.call(hd.log, &[x])])));
            }
            Ok(None)
        }
        Some((g, fs)) if g == hd.mul => {
            let mut out = vec![];
            let mut rest = vec![];
            for &p in fs {
                let ip = f.info(cx, p)?;
                if f.positive(&ip) {
                    out.push(b.call(hd.log, &[p]));
                } else {
                    rest.push(p);
                }
            }
            if out.is_empty() {
                return Ok(None);
            }
            if !rest.is_empty() {
                out.push(b.call(hd.log, &[b.mul(&rest)]));
            }
            Ok(Some(b.add(&out)))
        }
        _ => Ok(None),
    }
}

/// `sin`, `cos`, `tan` at integer and half-integer multiples of `pi`.
fn trig<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, f: &mut Facts<'_, 'h>, s: mc_api::Sym, u: T<'h>) -> Res<Option<T<'h>>> {
    let hd = &b.hd;
    let h = b.heap();
    let pi = f.syms().pi;
    // the symbol mask is a quick test that `u` mentions `pi` (bit `i % 64` for symbol `i`)
    if h.mask(u) & 1u64 << (pi.0 % 64) == 0 {
        return Ok(None);
    }
    // the multiple of pi, expanded so that `(1/2)*(1 + 2*n)` reads as `1/2 + n`
    let k = apply(cx, b, "expand", b.div(u, h.sym_term(pi)))?;
    if f.info(cx, k)?.kind == Kind::Integer {
        return Ok(Some(if s == hd.cos { b.pow(b.int(-1), k) } else { b.int(0) }));
    }
    let m = b.sub(k, b.rat(1, 2));
    if s != hd.tan && f.info(cx, m)?.kind == Kind::Integer {
        return Ok(Some(if s == hd.cos { b.int(0) } else { b.pow(b.int(-1), m) }));
    }
    Ok(None)
}

/// `-u`, distributed over a sum (`-(x - 1)` is `1 - x`).
fn negate<'h>(b: &B<'h>, u: T<'h>) -> T<'h> {
    let terms: Vec<T<'h>> = b.terms(u).into_iter().map(|t| b.neg(t)).collect();
    b.add(&terms)
}
