//! The one-sided limit walk: the limit of a term as its variable tends to a point from one side.
//!
//! The point is moved to `t → 0⁺` (`x = c + t`, `x = c - t`, `x = 1/t`, `x = -1/t`), and the limit is computed by
//! a walk over the term: constants, sums, products, constant powers, `exp`, `log`, `atan`, `sin`, `cos`, `tan`,
//! `sinh`, `cosh`, `tanh`, `asin`, `acos`, `abs`. Indeterminate forms are resolved, in this order, by
//!
//! 1. the leading term of `series(g, t, 0, n)` (n = 4, 8, 16);
//! 2. in products, two dominance rules: `exp(u)` with `u → ±∞` algebraic beats algebraic and logarithmic
//!    factors, and an algebraic factor tending to 0 beats logarithms (decision 2C4.7);
//! 3. a bounded factor (`sin(u)`, `cos(u)`, `atan(u)`, `tanh(u)` of a real `u`) times factors tending to 0 tends to
//!    0, and a bounded term does not change the infinite limit of a sum (decision C.4);
//! 4. in a product `0·∞`, `abs(u)` replaced by `±u` when the sign of `u` near the point is known and `sinh`,
//!    `cosh` by exponentials, else a sum factor distributed over the others; then L'Hospital's rule, written `0/0` and else `∞/∞` (decision C.5);
//! 5. a sum that is still indeterminate (`∞ - ∞`): its logarithms of positive arguments combined into one, else
//!    the sum brought over a common denominator with `together` and the quotient's limit taken.
//!
//! Every step is exact; when none applies the result is `None` (no answer is better than a wrong one). Steps 4 and
//! 5 charge [`REWRITE_COST`] levels of the depth budget, so they nest a bounded number of times.
use crate::diff::{Diff, Halt};
use crate::integrate::definite::{call_of, is_failure, sign, subst, Pt};
use crate::integrate::{apply, args_of, is_free};
use crate::term::Build;
use mc_api::{names, Store, View, T};
use mc_eval::{NativeCx, Res};
use std::cmp::Ordering;

/// A limit.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Lim<'h> {
    /// A finite value (evaluated). When it is 0 the second field is the side it is approached from: 1 from above,
    /// -1 from below, 0 not known.
    Fin(T<'h>, i8),
    /// `inf` (1) or `-inf` (-1).
    Inf(i8),
}

type R<'h> = Res<Option<Lim<'h>>>;

/// The deepest nesting the walk follows.
const MAX_DEPTH: u32 = 200;

/// The depth a rewriting step (L'Hospital's rule, a common denominator) costs: at most four of them nest.
const REWRITE_COST: u32 = 50;

/// The limit of `g` as `x` tends to `pt` from the side `side` (1: from above, -1: from below; ignored at an
/// infinity), or `None`.
pub(crate) fn limit<'h>(
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
                Lim::Fin(v, _) => Some(fin(cx, b.call("log", &[v]), 0)?),
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
                    Some(fin(cx, b.call(f, &[v]), s)?)
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
        ("tan", [u]) => {
            // continuous where cos does not vanish; at a pole, sin(u)/cos(u) (the side of cos(u) → 0 decides)
            let Some(Lim::Fin(v, s)) = lim(cx, b, *u, t, d)? else { return Ok(None) };
            let c = cx.eval(b.call("cos", &[v]))?;
            match sign(cx, b, c)? {
                Some(1 | -1) => Ok(Some(fin(cx, b.call("tan", &[v]), if v == b.zero() { s } else { 0 })?)),
                Some(_) => {
                    let q = b.mul(&[b.call("sin", &[*u]), b.powi(b.call("cos", &[*u]), -1)]);
                    lim(cx, b, q, t, d)
                }
                None => Ok(None),
            }
        }
        ("sign", [u]) => Ok(side_of(cx, b, *u, t, d)?.map(|s| Lim::Fin(b.int(s.into()), 0))),
        ("abs", [u]) => {
            let Some(l) = lim(cx, b, *u, t, d)? else { return Ok(None) };
            Ok(Some(match l {
                Lim::Fin(v, _) if v == b.zero() => Lim::Fin(v, 1),
                Lim::Fin(v, _) => fin(cx, b.call("abs", &[v]), 0)?,
                Lim::Inf(_) => Lim::Inf(1),
            }))
        }
        (f @ ("asin" | "acos"), [u]) => {
            // continuous on [-1, 1]; elsewhere (and at infinities) not decided here
            let Some(Lim::Fin(v, s)) = lim(cx, b, *u, t, d)? else { return Ok(None) };
            if sign(cx, b, b.sub(b.one(), b.powi(v, 2)))?.is_none_or(|s| s < 0) {
                return Ok(None);
            }
            let s = if v == b.zero() && f == "asin" { s } else { 0 };
            Ok(Some(fin(cx, b.call(f, &[v]), s)?))
        }
        _ => series(cx, b, g, t),
    }
}

/// `Lim::Fin` of the evaluated `v`.
fn fin<'h>(cx: &mut NativeCx<'_, '_, 'h>, v: T<'h>, s: i8) -> Res<Lim<'h>> {
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

/// The sign (1 or -1) of `u` for small positive t, when its limit decides it: a nonzero real limit, an
/// infinity, or 0 approached from a known side.
fn side_of<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, u: T<'h>, t: T<'h>, d: u32) -> Res<Option<i8>> {
    Ok(match lim(cx, b, u, t, d)? {
        Some(Lim::Fin(v, s)) if v == b.zero() => Some(approach(cx, b, u, t, s)?).filter(|&s| s != 0),
        Some(Lim::Fin(v, _)) => sign(cx, b, v)?.filter(|&s| s != 0),
        Some(Lim::Inf(s)) => Some(s),
        None => None,
    })
}

/// A product rewritten so the walk can take it: `abs(u)` (or a power of it) as `±u` when the sign of `u` near
/// the point is known and `sinh`, `cosh` as exponentials, else one sum factor distributed over the others.
/// `None` when nothing applies.
fn rewrite_product<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    b: &Build<'h>,
    args: &[T<'h>],
    t: T<'h>,
    d: u32,
) -> Res<Option<T<'h>>> {
    let mut out = Vec::with_capacity(args.len());
    let mut changed = false;
    for &a in args {
        let (base, p) = match args_of(b, a, b.hd.pow) {
            Some([base, p]) if is_free(b, *p, t) => (*base, Some(*p)),
            _ => (a, None),
        };
        let u = match call_of(b, base) {
            Some((n, [u])) if n == "abs" => *u,
            Some((n, [u])) if n == "sinh" || n == "cosh" => {
                // exponentials, so the dominance rules and the product of exponentials apply
                let sg = if n == "sinh" { -1 } else { 1 };
                let ex = |v: T<'h>| b.call("exp", &[v]);
                let half = b.mul(&[b.rat(1, 2), b.add(&[ex(*u), b.mul(&[b.int(sg), ex(b.neg(*u))])])]);
                out.push(p.map_or(half, |p| b.pow(half, p)));
                changed = true;
                continue;
            }
            _ => {
                out.push(a);
                continue;
            }
        };
        match side_of(cx, b, u, t, d)? {
            Some(s) => {
                let signed = b.mul(&[b.int(s.into()), u]);
                out.push(p.map_or(signed, |p| b.pow(signed, p)));
                changed = true;
            }
            None => out.push(a),
        }
    }
    if changed {
        return Ok(Some(cx.eval(b.mul(&out))?));
    }
    let Some(at) = args.iter().position(|&a| args_of(b, a, b.hd.add).is_some()) else { return Ok(None) };
    let others: Vec<T<'h>> = args.iter().enumerate().filter(|&(i, _)| i != at).map(|(_, &a)| a).collect();
    // invariant: `at` is the position of an argument that `args_of` accepted (found just above)
    let terms = args_of(b, args[at], b.hd.add).expect("a sum");
    let mut distributed = Vec::with_capacity(terms.len());
    for &s in terms {
        let mut fs = others.clone();
        fs.push(s);
        distributed.push(b.mul(&fs));
    }
    Ok(Some(cx.eval(b.add(&distributed))?))
}

/// The limit of a sum.
fn sum<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, g: T<'h>, args: &[T<'h>], t: T<'h>, d: u32) -> R<'h> {
    let mut fins = Vec::with_capacity(args.len());
    let mut infs = Vec::new();
    let mut inf = 0i8;
    let mut mixed = false;
    let mut bounded_terms = false;
    for &a in args {
        match lim(cx, b, a, t, d)? {
            None if bounded(b, a, t) => bounded_terms = true,
            None => return indeterminate_sum(cx, b, g, t, d),
            Some(Lim::Fin(v, _)) => fins.push(v),
            Some(Lim::Inf(s)) => {
                infs.push((a, s));
                mixed |= inf != 0 && inf != s;
                inf = s;
            }
        }
    }
    if mixed {
        if let Some(r) = dominant_term(cx, b, g, args, &infs, t, d)? {
            return Ok(Some(r));
        }
        if let Some(r) = combined_logs(cx, b, args, t, d)? {
            return Ok(Some(r));
        }
        return common_denominator(cx, b, g, t, d);
    }
    if inf != 0 {
        // a bounded term does not change an infinite limit
        return Ok(Some(Lim::Inf(inf)));
    }
    if bounded_terms {
        return indeterminate_sum(cx, b, g, t, d);
    }
    Ok(Some(fin(cx, b.add(&fins), 0)?))
}

/// `∞ - ∞` between logarithms: the terms `c·log(u)` (c rational, u positive near the point) of a sum combined
/// into one `log(Π u^c)`, which holds for positive u; the limit of the new sum, or `None`.
fn combined_logs<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, args: &[T<'h>], t: T<'h>, d: u32) -> R<'h> {
    if d + REWRITE_COST > MAX_DEPTH {
        return Ok(None);
    }
    let (mut powers, mut rest) = (Vec::new(), Vec::new());
    for &a in args {
        let (c, l) = match args_of(b, a, b.hd.mul) {
            Some([c, l]) if matches!(b.heap().view(*c), View::Int(_) | View::Rational { .. }) => (*c, *l),
            _ => (b.one(), a),
        };
        match call_of(b, l) {
            Some((n, [u])) if n == "log" && side_of(cx, b, *u, t, d)? == Some(1) => powers.push(b.pow(*u, c)),
            _ => rest.push(a),
        }
    }
    if powers.len() < 2 {
        return Ok(None);
    }
    let product = apply(cx, b, "together", &[b.mul(&powers)])?;
    rest.push(b.call("log", &[product]));
    let s = cx.eval(b.add(&rest))?;
    lim(cx, b, s, t, d + REWRITE_COST)
}

/// A sum with a term of unknown limit: the series, else a common denominator.
fn indeterminate_sum<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, g: T<'h>, t: T<'h>, d: u32) -> R<'h> {
    if let Some(r) = series(cx, b, g, t)? {
        return Ok(Some(r));
    }
    common_denominator(cx, b, g, t, d)
}

/// The limit of `together(g)` when that is a different term (a quotient whose factors the product rules and
/// L'Hospital's rule can take), within the depth budget.
fn common_denominator<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, g: T<'h>, t: T<'h>, d: u32) -> R<'h> {
    if d + REWRITE_COST > MAX_DEPTH {
        return Ok(None);
    }
    let q = apply(cx, b, "together", &[g])?;
    if q == g || is_failure(b, q) || args_of(b, q, b.hd.add).is_some() {
        return Ok(None);
    }
    lim(cx, b, q, t, d + REWRITE_COST)
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
    let (mut small, mut big, mut finite, mut bnd) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let (mut zero_side, mut inf_sign) = (1i8, 1i8);
    for &a in args {
        match lim(cx, b, a, t, d)? {
            None if bounded(b, a, t) => bnd.push(a),
            None => return dominance(cx, b, g, args, t, d),
            Some(Lim::Fin(v, s)) if v == b.zero() => {
                small.push(a);
                zero_side *= s;
            }
            Some(Lim::Fin(v, _)) => {
                fins.push(v);
                finite.push(a);
            }
            Some(Lim::Inf(s)) => {
                big.push(a);
                inf_sign *= s;
            }
        }
    }
    if !bnd.is_empty() {
        // bounded factors: the product tends to 0 when the other factors do (decision C.4)
        let others: Vec<T<'h>> = small.iter().chain(&big).chain(&finite).copied().collect();
        let tends_to_zero = match (small.is_empty(), big.is_empty()) {
            (true, _) => false,
            (false, true) => true,
            (false, false) => matches!(lim(cx, b, b.mul(&others), t, d)?, Some(Lim::Fin(v, _)) if v == b.zero()),
        };
        return Ok(tends_to_zero.then(|| Lim::Fin(b.zero(), 0)));
    }
    if !small.is_empty() && !big.is_empty() {
        if let Some(r) = dominance(cx, b, g, args, t, d)? {
            return Ok(Some(r));
        }
        if d + REWRITE_COST <= MAX_DEPTH {
            if let Some(q) = rewrite_product(cx, b, args, t, d)?.filter(|&q| q != g) {
                if let Some(r) = lim(cx, b, q, t, d + REWRITE_COST)? {
                    return Ok(Some(r));
                }
            }
        }
        let mut num = small;
        num.extend_from_slice(&finite);
        return lhospital(cx, b, &num, &big, t, d);
    }
    let rest = cx.eval(b.mul(&fins))?;
    if !big.is_empty() {
        return Ok(match sign(cx, b, rest)? {
            Some(s @ (1 | -1)) => Some(Lim::Inf(s * inf_sign)),
            _ => None,
        });
    }
    if !small.is_empty() {
        let s = sign(cx, b, rest)?.unwrap_or(0);
        return Ok(Some(Lim::Fin(b.zero(), zero_side * s)));
    }
    Ok(Some(Lim::Fin(rest, 0)))
}

/// L'Hospital's rule on a product `0·∞`: `small` (factors tending to 0, with the finite ones) and `big` (factors
/// tending to an infinity). First as `0/0`, `small / (1/big)`, then as `∞/∞`, `big / (1/small)`: the limit of the
/// quotient of the derivatives in t, when it exists, is the limit (decision C.5).
fn lhospital<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    b: &Build<'h>,
    small: &[T<'h>],
    big: &[T<'h>],
    t: T<'h>,
    d: u32,
) -> R<'h> {
    if d + REWRITE_COST > MAX_DEPTH {
        return Ok(None);
    }
    let inverse = |fs: &[T<'h>]| b.mul(&fs.iter().map(|&f| b.powi(f, -1)).collect::<Vec<_>>());
    for (num, den) in [(b.mul(small), inverse(big)), (b.mul(big), inverse(small))] {
        let (num, den) = (cx.eval(num)?, cx.eval(den)?);
        let derivatives = {
            let mut tick = |n: u64| cx.tick(n);
            let dn = Diff::new(b, t).of(num, &mut tick);
            dn.and_then(|dn| Ok((dn, Diff::new(b, t).of(den, &mut tick)?)))
        };
        let (dn, dd) = match derivatives {
            Ok(p) => p,
            Err(Halt::Abort(a)) => return Err(a),
            Err(Halt::Unsupported) => continue,
        };
        if b.is_zero(dd) {
            continue;
        }
        let q = cx.eval(b.div(dn, dd))?;
        if is_failure(b, q) {
            continue;
        }
        if let Some(r) = lim(cx, b, q, t, d + REWRITE_COST)? {
            return Ok(Some(r));
        }
    }
    Ok(None)
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

/// Is `a` bounded for small positive t: `sin(u)`, `cos(u)`, `atan(u)` or `tanh(u)` with `u` real there?
fn bounded<'h>(b: &Build<'h>, a: T<'h>, t: T<'h>) -> bool {
    matches!(call_of(b, a), Some((n, [u])) if matches!(n.as_str(), "sin" | "cos" | "atan" | "tanh") && real(b, *u, t))
}

/// Is `u` real for every positive t: built from `t`, real numbers, `pi` and `e` by sums, products, integer
/// powers and the real functions `exp`, `sin`, `cos`, `atan`, `sinh`, `cosh`, `tanh`? (Other symbols may be
/// complex, so they are not accepted.)
fn real<'h>(b: &Build<'h>, u: T<'h>, t: T<'h>) -> bool {
    let h = b.heap();
    let (pi, e) = (h.sym("pi"), b.hd.e);
    let mut stack = vec![u];
    let mut visited = 0;
    while let Some(v) = stack.pop() {
        visited += 1;
        if visited > 10_000 {
            return false;
        }
        match h.view(v) {
            View::Int(_) | View::BigInt { .. } | View::Rational { .. } | View::Real(_) => continue,
            View::Sym(s) if v == t || s == pi || s == e => continue,
            _ => {}
        }
        if let Some(args) = args_of(b, v, b.hd.add).or_else(|| args_of(b, v, b.hd.mul)) {
            stack.extend_from_slice(args);
            continue;
        }
        match call_of(b, v) {
            Some((n, [base, p])) if n == "pow" && matches!(h.view(*p), View::Int(_)) => stack.push(*base),
            Some((n, [base, p])) if n == "pow" && b.sym_of(*base) == Some(e) => stack.push(*p),
            Some((n, [a])) if matches!(n.as_str(), "exp" | "sin" | "cos" | "atan" | "sinh" | "cosh" | "tanh") => {
                stack.push(*a)
            }
            _ => return false,
        }
    }
    true
}

/// The limit of `exp(u)`.
fn exp<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, u: T<'h>, t: T<'h>, d: u32) -> R<'h> {
    Ok(match lim(cx, b, u, t, d)? {
        None => None,
        Some(Lim::Fin(v, _)) => Some(fin(cx, b.call("exp", &[v]), 0)?),
        Some(Lim::Inf(1)) => Some(Lim::Inf(1)),
        Some(Lim::Inf(_)) => Some(Lim::Fin(b.zero(), 1)),
    })
}

/// The limit of `u^p`, `p` free of `t`.
fn power<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, u: T<'h>, p: T<'h>, t: T<'h>, d: u32) -> R<'h> {
    let Some(l) = lim(cx, b, u, t, d)? else { return Ok(None) };
    if let Lim::Fin(v, _) = l {
        if v != b.zero() {
            return Ok(Some(fin(cx, b.pow(v, p), 0)?));
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
    let data = h.sym("series_data");
    for n in [4, 8, 16] {
        cx.tick(1)?;
        // no Taylor expansion of functions through their derivatives here: an unknown function need not be
        // continuous at the point
        let opt = h.call(h.sym(names::RULE), &[h.sym_term(h.sym("derivatives")), h.sym_term(h.sym(names::FALSE))]);
        let opts = h.call(h.sym("options"), &[opt]);
        let s = apply(cx, b, "series", &[g, t, b.zero(), b.int(n), opts])?;
        // the packed form series_data(t, 0, [c_v, c_(v+1), ...], v, p): the first coefficient that is not 0
        let Some([_, _, cs, v, _]) = args_of(b, s, data) else { return Ok(None) };
        let (Some(cs), View::Int(v)) = (args_of(b, *cs, b.hd.list), h.view(*v)) else { return Ok(None) };
        for (i, &c) in cs.iter().enumerate() {
            // a failure, or a logarithmic coefficient (log(t)): not a leading power term
            if is_failure(b, c) || !is_free(b, c, t) {
                return Ok(None);
            }
            let sc = sign(cx, b, c)?;
            if sc == Some(0) {
                continue;
            }
            return Ok(match ((v + i as i64).cmp(&0), sc) {
                (Ordering::Less, None) => None,
                (Ordering::Less, Some(s)) => Some(Lim::Inf(s)),
                (Ordering::Equal, _) => Some(Lim::Fin(c, 0)),
                (Ordering::Greater, s) => Some(Lim::Fin(b.zero(), s.unwrap_or(0))),
            });
        }
    }
    Ok(None)
}
