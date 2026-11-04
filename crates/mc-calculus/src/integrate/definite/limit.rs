//! A minimal, conservative one-sided limit: the value of an antiderivative at an endpoint or a critical point.
//!
//! The point is moved to `t → 0⁺` (`x = c + t`, `x = c - t`, `x = 1/t`, `x = -1/t`), and the limit is computed by
//! a walk over the term: constants, sums, products, constant powers, `exp`, `log`, `atan`, `sin`, `cos`, `sinh`,
//! `cosh`, `tanh`. Indeterminate forms (`∞ - ∞`, `0·∞`, a sum that tends to 0 under a negative power) go to the
//! leading term of `series(g, t, 0, n)`, and in products to two dominance rules: `exp(u)` with `u → ±∞` algebraic
//! beats algebraic and logarithmic factors, and an algebraic factor tending to 0 beats logarithms. Everything else
//! gives `None` (decision 2C4.7).
use super::{call_of, is_failure, sign, subst, Pt};
use crate::integrate::{apply, args_of, is_free};
use crate::term::Build;
use mc_api::{Store, View, T};
use mc_eval::{NativeCx, Res};

/// A limit.
#[derive(Clone, Copy, Debug)]
pub(super) enum Lim<'h> {
    /// A finite value (evaluated). When it is 0 the second field is the side it is approached from: 1 from above,
    /// -1 from below, 0 not known.
    Fin(T<'h>, i8),
    /// `inf` (1) or `-inf` (-1).
    Inf(i8),
}

type R<'h> = Res<Option<Lim<'h>>>;

/// The deepest nesting the walk follows.
const MAX_DEPTH: u32 = 200;

/// The limit of `g` as `x` tends to `pt` from the side `side` (1: from above, -1: from below; ignored at an
/// infinity), or `None`.
pub(super) fn limit<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    b: &Build<'h>,
    g: T<'h>,
    x: T<'h>,
    pt: Pt<'h>,
    side: i8,
) -> R<'h> {
    let h = b.heap();
    let t = ["t%limit", "t%limit2", "t%limit3"].into_iter().map(|n| h.sym_term(h.sym(n))).find(|&t| is_free(b, g, t));
    let Some(t) = t else { return Ok(None) };
    let at = match pt {
        Pt::Fin(c) => b.add(&[c, b.mul(&[b.int(side.into()), t])]),
        Pt::PosInf => b.powi(t, -1),
        Pt::NegInf => b.neg(b.powi(t, -1)),
    };
    let gt = subst(cx, b, g, x, at)?;
    if is_failure(b, gt) {
        return Ok(None);
    }
    lim(cx, b, gt, t, 0)
}

/// The limit of `g` as `t → 0⁺`.
fn lim<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, g: T<'h>, t: T<'h>, depth: u32) -> R<'h> {
    cx.tick(1)?;
    if is_free(b, g, t) {
        return Ok(Some(Lim::Fin(g, 0)));
    }
    if g == t {
        return Ok(Some(Lim::Fin(b.zero(), 1)));
    }
    if depth > MAX_DEPTH {
        return Ok(None);
    }
    let d = depth + 1;
    if let Some(args) = args_of(b, g, b.hd.add) {
        return sum(cx, b, g, args, t, d);
    }
    if let Some(args) = args_of(b, g, b.hd.mul) {
        return product(cx, b, g, args, t, d);
    }
    let Some((name, args)) = call_of(b, g) else { return Ok(None) };
    match (name.as_str(), args) {
        ("pow", [base, u]) if b.sym_of(*base) == Some(b.hd.e) => exp(cx, b, *u, t, d),
        ("pow", [u, p]) if is_free(b, *p, t) => power(cx, b, *u, *p, t, d),
        ("pow", [u, v]) => {
            let e = b.call("exp", &[b.mul(&[*v, b.call("log", &[*u])])]);
            lim(cx, b, e, t, d)
        }
        ("exp", [u]) => exp(cx, b, *u, t, d),
        ("log", [u]) => {
            let Some(l) = lim(cx, b, *u, t, d)? else { return Ok(None) };
            Ok(match l {
                Lim::Fin(v, s) if v == b.zero() => match approach(cx, b, *u, t, s)? {
                    1 => Some(Lim::Inf(-1)),
                    _ => None,
                },
                Lim::Fin(v, _) => Some(fin(cx, b, b.call("log", &[v]), 0)?),
                Lim::Inf(1) => Some(Lim::Inf(1)),
                Lim::Inf(_) => None,
            })
        }
        (f @ ("atan" | "sin" | "cos" | "sinh" | "cosh" | "tanh"), [u]) => {
            let Some(l) = lim(cx, b, *u, t, d)? else { return Ok(None) };
            Ok(match l {
                // these functions keep 0 at 0 and its side, except cos and cosh (value 1)
                Lim::Fin(v, s) => {
                    let s = if v == b.zero() && f != "cos" && f != "cosh" { s } else { 0 };
                    Some(fin(cx, b, b.call(f, &[v]), s)?)
                }
                Lim::Inf(sg) => match f {
                    "atan" => Some(Lim::Fin(b.mul(&[b.rat(sg.into(), 2), b.sym(b.heap().sym("pi"))]), 0)),
                    "sinh" => Some(Lim::Inf(sg)),
                    "cosh" => Some(Lim::Inf(1)),
                    "tanh" => Some(Lim::Fin(b.int(sg.into()), 0)),
                    _ => None,
                },
            })
        }
        (f @ ("asin" | "acos"), [u]) => {
            // continuous on [-1, 1]; elsewhere (and at infinities) not decided here
            let Some(Lim::Fin(v, s)) = lim(cx, b, *u, t, d)? else { return Ok(None) };
            if sign(cx, b, b.sub(b.one(), b.powi(v, 2)))?.is_none_or(|s| s < 0) {
                return Ok(None);
            }
            let s = if v == b.zero() && f == "asin" { s } else { 0 };
            Ok(Some(fin(cx, b, b.call(f, &[v]), s)?))
        }
        _ => series(cx, b, g, t),
    }
}

/// `Lim::Fin` of the evaluated `v`.
fn fin<'h>(cx: &mut NativeCx<'_, '_, 'h>, _b: &Build<'h>, v: T<'h>, s: i8) -> Res<Lim<'h>> {
    Ok(Lim::Fin(cx.eval(v)?, s))
}

/// The side from which `u → 0` is approached: `s` when known, else the sign of the leading term of its series;
/// 0 when not known.
fn approach<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, u: T<'h>, t: T<'h>, s: i8) -> Res<i8> {
    if s != 0 {
        return Ok(s);
    }
    Ok(match series(cx, b, u, t)? {
        Some(Lim::Fin(v, s)) if v == b.zero() => s,
        _ => 0,
    })
}

/// The limit of a sum.
fn sum<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, g: T<'h>, args: &[T<'h>], t: T<'h>, d: u32) -> R<'h> {
    let mut fins = Vec::with_capacity(args.len());
    let mut infs = Vec::new();
    let mut inf = 0i8;
    let mut mixed = false;
    for &a in args {
        match lim(cx, b, a, t, d)? {
            None => return series(cx, b, g, t),
            Some(Lim::Fin(v, _)) => fins.push(v),
            Some(Lim::Inf(s)) => {
                infs.push((a, s));
                mixed |= inf != 0 && inf != s;
                inf = s;
            }
        }
    }
    if mixed {
        return dominant_term(cx, b, g, args, &infs, t, d);
    }
    if inf != 0 {
        return Ok(Some(Lim::Inf(inf)));
    }
    Ok(Some(fin(cx, b, b.add(&fins), 0)?))
}

/// `∞ - ∞` in a sum: the series first; else for an infinite term `a`, `g = a·(1 + Σ others/a)`, and when the
/// second factor tends to a nonzero finite value the sum tends to an infinity of the sign of the product.
fn dominant_term<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    b: &Build<'h>,
    g: T<'h>,
    args: &[T<'h>],
    infs: &[(T<'h>, i8)],
    t: T<'h>,
    d: u32,
) -> R<'h> {
    if let Some(r) = series(cx, b, g, t)? {
        return Ok(Some(r));
    }
    for &(a, s) in infs {
        let mut ratio = vec![b.one()];
        ratio.extend(args.iter().filter(|&&o| o != a).map(|&o| b.div(o, a)));
        let r = cx.eval(b.add(&ratio))?;
        if let Some(Lim::Fin(v, _)) = lim(cx, b, r, t, d)? {
            if let Some(sv @ (1 | -1)) = sign(cx, b, v)? {
                return Ok(Some(Lim::Inf(s * sv)));
            }
        }
    }
    Ok(None)
}

/// The limit of a product.
fn product<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, g: T<'h>, args: &[T<'h>], t: T<'h>, d: u32) -> R<'h> {
    let mut fins = Vec::new();
    let mut zero_side = 1i8;
    let (mut zeros, mut infs, mut inf_sign) = (0, 0, 1i8);
    for &a in args {
        match lim(cx, b, a, t, d)? {
            None => return dominance(cx, b, g, args, t, d),
            Some(Lim::Fin(v, s)) if v == b.zero() => {
                zeros += 1;
                zero_side *= s;
            }
            Some(Lim::Fin(v, _)) => fins.push(v),
            Some(Lim::Inf(s)) => {
                infs += 1;
                inf_sign *= s;
            }
        }
    }
    if zeros > 0 && infs > 0 {
        return dominance(cx, b, g, args, t, d);
    }
    let rest = cx.eval(b.mul(&fins))?;
    if infs > 0 {
        return Ok(match sign(cx, b, rest)? {
            Some(s @ (1 | -1)) => Some(Lim::Inf(s * inf_sign)),
            _ => None,
        });
    }
    if zeros > 0 {
        let s = sign(cx, b, rest)?.unwrap_or(0);
        return Ok(Some(Lim::Fin(b.zero(), zero_side * s)));
    }
    Ok(Some(Lim::Fin(rest, 0)))
}

/// A product with an indeterminate form: the dominance rules (module comment), else the series.
fn dominance<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, g: T<'h>, args: &[T<'h>], t: T<'h>, d: u32) -> R<'h> {
    let (mut exps, mut alg, mut logs) = (Vec::new(), Vec::new(), Vec::new());
    for &a in args {
        if algebraic(b, a, t) {
            alg.push(a);
        } else if let Some(u) = exp_arg(b, a) {
            exps.push(u);
        } else if log_factor(b, a, t) {
            logs.push(a);
        } else {
            return series(cx, b, g, t);
        }
    }
    let rest = b.mul(&[b.mul(&alg), b.mul(&logs)]);
    if !exps.is_empty() {
        let u = b.add(&exps);
        if !algebraic(b, u, t) {
            return series(cx, b, g, t);
        }
        let Some(Lim::Inf(sg)) = lim(cx, b, u, t, d)? else { return series(cx, b, g, t) };
        let lr = lim(cx, b, rest, t, d)?;
        let rs = match lr {
            Some(Lim::Fin(v, s)) if v == b.zero() => s,
            Some(Lim::Fin(v, _)) => sign(cx, b, v)?.unwrap_or(0),
            Some(Lim::Inf(s)) => s,
            None => 0,
        };
        return Ok(if sg < 0 {
            // a polynomially bounded factor times exp(u) with u → -inf tends to 0
            Some(Lim::Fin(b.zero(), rs))
        } else if rs != 0 && lr.is_some() {
            Some(Lim::Inf(rs))
        } else {
            None
        });
    }
    if !logs.is_empty() && !alg.is_empty() {
        let Some(Lim::Fin(v, s)) = lim(cx, b, b.mul(&alg), t, d)? else { return series(cx, b, g, t) };
        if v == b.zero() {
            // an algebraic factor tending to 0 beats any product of logarithms of algebraic functions
            let mut side = s;
            for &l in &logs {
                side *= match lim(cx, b, l, t, d)? {
                    Some(Lim::Inf(s)) => s,
                    Some(Lim::Fin(v, _)) => sign(cx, b, v)?.unwrap_or(0),
                    None => 0,
                };
            }
            return Ok(Some(Lim::Fin(b.zero(), side)));
        }
    }
    series(cx, b, g, t)
}

/// The exponent of `exp(u)` or `e^u`.
fn exp_arg<'h>(b: &Build<'h>, a: T<'h>) -> Option<T<'h>> {
    match call_of(b, a)? {
        (n, [u]) if n == "exp" => Some(*u),
        (n, [base, u]) if n == "pow" && b.sym_of(*base) == Some(b.hd.e) => Some(*u),
        _ => None,
    }
}

/// Is `a` `log(u)` or a constant power of it, with `u` algebraic in `t`?
fn log_factor<'h>(b: &Build<'h>, a: T<'h>, t: T<'h>) -> bool {
    let a = match args_of(b, a, b.hd.pow) {
        Some([l, p]) if is_free(b, *p, t) => *l,
        _ => a,
    };
    matches!(call_of(b, a), Some((n, [u])) if n == "log" && algebraic(b, *u, t))
}

/// Is `u` built from `t` and constants by sums, products and constant numeric powers?
fn algebraic<'h>(b: &Build<'h>, u: T<'h>, t: T<'h>) -> bool {
    let mut stack = vec![u];
    while let Some(v) = stack.pop() {
        if v == t || is_free(b, v, t) {
            continue;
        }
        if let Some(args) = args_of(b, v, b.hd.add).or_else(|| args_of(b, v, b.hd.mul)) {
            stack.extend_from_slice(args);
            continue;
        }
        match args_of(b, v, b.hd.pow) {
            Some([base, p]) if b.ar.num(*p).is_some() && b.sym_of(*base) != Some(b.hd.e) => stack.push(*base),
            _ => return false,
        }
    }
    true
}

/// The limit of `exp(u)`.
fn exp<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, u: T<'h>, t: T<'h>, d: u32) -> R<'h> {
    Ok(match lim(cx, b, u, t, d)? {
        None => None,
        Some(Lim::Fin(v, _)) => Some(fin(cx, b, b.call("exp", &[v]), 0)?),
        Some(Lim::Inf(1)) => Some(Lim::Inf(1)),
        Some(Lim::Inf(_)) => Some(Lim::Fin(b.zero(), 1)),
    })
}

/// The limit of `u^p`, `p` free of `t`.
fn power<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, u: T<'h>, p: T<'h>, t: T<'h>, d: u32) -> R<'h> {
    let Some(l) = lim(cx, b, u, t, d)? else { return Ok(None) };
    if let Lim::Fin(v, _) = l {
        if v != b.zero() {
            return Ok(Some(fin(cx, b, b.pow(v, p), 0)?));
        }
    }
    // the sign of p, and its parity when it is an integer: (-1)^p
    let Some(ps) = (if b.ar.num(p).is_some() { sign(cx, b, p)? } else { None }) else { return Ok(None) };
    let parity = match b.heap().view(p) {
        View::Int(k) => Some(if k % 2 == 0 { 1i8 } else { -1 }),
        _ => None,
    };
    Ok(match l {
        Lim::Fin(_, s) => {
            let s = approach(cx, b, u, t, s)?;
            match (ps > 0, s, parity) {
                (_, 0, _) => None,
                (true, 1, _) => Some(Lim::Fin(b.zero(), 1)),
                (true, _, Some(q)) => Some(Lim::Fin(b.zero(), q)),
                (true, _, None) => Some(Lim::Fin(b.zero(), 0)),
                (false, 1, _) => Some(Lim::Inf(1)),
                (false, _, Some(q)) => Some(Lim::Inf(q)),
                (false, _, None) => None,
            }
        }
        Lim::Inf(sg) => match (ps > 0, sg, parity) {
            (true, 1, _) => Some(Lim::Inf(1)),
            (true, _, Some(q)) => Some(Lim::Inf(q)),
            (true, _, None) => None,
            (false, 1, _) => Some(Lim::Fin(b.zero(), 1)),
            (false, _, Some(q)) => Some(Lim::Fin(b.zero(), q)),
            (false, _, None) => Some(Lim::Fin(b.zero(), 0)),
        },
    })
}

/// The limit from the leading term `c t^k` of `series(g, t, 0, n)` (n = 4, 8, 16 until a term shows).
fn series<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, g: T<'h>, t: T<'h>) -> R<'h> {
    let h = b.heap();
    let o = h.sym("O");
    for n in [4, 8, 16] {
        cx.tick(1)?;
        let s = apply(cx, b, "series", &[g, t, b.zero(), b.int(n)])?;
        if is_failure(b, s) || !is_free(b, s, h.sym_term(h.sym("series"))) {
            return Ok(None);
        }
        let terms: Vec<T<'h>> = match args_of(b, s, b.hd.add) {
            Some(ts) => ts.to_vec(),
            None => vec![s],
        };
        let mut lead: Option<(i64, T<'h>)> = None;
        for term in terms {
            if args_of(b, term, o).is_some() {
                continue;
            }
            let Some((k, c)) = monomial(b, term, t) else { return Ok(None) };
            match lead {
                Some((k0, _)) if k0 <= k => {}
                _ => lead = Some((k, c)),
            }
        }
        let Some((k, c)) = lead else { continue };
        let sc = sign(cx, b, c)?;
        return Ok(match (k.cmp(&0), sc) {
            (_, None | Some(0)) if k <= 0 => None,
            (std::cmp::Ordering::Less, Some(s)) => Some(Lim::Inf(s)),
            (std::cmp::Ordering::Equal, _) => Some(Lim::Fin(c, 0)),
            (_, s) => Some(Lim::Fin(b.zero(), s.unwrap_or(0))),
        });
    }
    Ok(None)
}

/// A term `c t^k` (integer k, c free of t) as `(k, c)`.
fn monomial<'h>(b: &Build<'h>, term: T<'h>, t: T<'h>) -> Option<(i64, T<'h>)> {
    let fs: Vec<T<'h>> = match args_of(b, term, b.hd.mul) {
        Some(fs) => fs.to_vec(),
        None => vec![term],
    };
    let (mut k, mut cs) = (0i64, Vec::new());
    for f in fs {
        if f == t {
            k += 1;
        } else if let Some([base, e]) = args_of(b, f, b.hd.pow).filter(|a| a[0] == t) {
            let _ = base;
            match b.heap().view(*e) {
                View::Int(e) => k += e,
                _ => return None,
            }
        } else if is_free(b, f, t) {
            cs.push(f);
        } else {
            return None;
        }
    }
    Some((k, b.mul(&cs)))
}
