//! The relatives of `limit` (item 13C): `max_limit` and `min_limit` (the limits superior and inferior),
//! `discrete_limit` (the limit of a sequence) and `asymptotic` (the leading term of the asymptotic expansion).
//!
//! - **max_limit, min_limit**: the limit when it exists; else a sum `L + Σ c_k h_k(u)` with `h_k` in `sin`, `cos`,
//!   one argument `u → ±inf` and coefficients with finite limits `C_k`, whose values fill `L ± sqrt(Σ C_k^2)` (for
//!   one argument `a sin u + b cos u` runs over `[-sqrt(a^2 + b^2), sqrt(a^2 + b^2)]`), or `±inf` when a
//!   coefficient grows. A two-sided limit point takes the larger (smaller) of the two sides.
//! - **discrete_limit**: `fibonacci` and `lucas_l` by Binet's formulas, `(-c)^n` as `(-1)^n c^n`; a sequence with
//!   `(-1)^n` is split into its even and odd subsequences, which have a common limit or none; then the limit of the
//!   real function (when that exists, the sequence has it too).
//! - **asymptotic**: at a finite point the leading term of the series (Puiseux and logarithmic terms included); at
//!   an infinity `c ω^e` from Gruntz's algorithm, with the coefficient `c` taken asymptotically in turn and ω's
//!   exponent stripped of the terms that tend to 0 (`exp(s) ~ exp(s0)` when `s - s0 → 0`).
use super::gruntz::{self, Gruntz, Lead};
use super::walk::Lim;
use super::{one_sided, Direction, Outcome};
use crate::bounds::{sign, subst, Pt};
use mc_calculus::common::{apply, args_of, call_of, is_failure, is_free};
use mc_calculus::term::Build;
use mc_api::{Store, View, T};
use mc_eval::{NativeCx, Res};
use std::time::{Duration, Instant};

/// A limit value: finite, or an infinity.
#[derive(Clone, Copy, Debug)]
enum Ext<'h> {
    Fin(T<'h>),
    Inf(i8),
}

/// The points and sides of a limit `x → a` from `direction`.
fn sides<'h>(b: &Build<'h>, a: T<'h>, direction: Direction) -> Vec<(Pt<'h>, i8)> {
    let inf = b.heap().sym_term(b.heap().sym("inf"));
    if a == inf {
        return vec![(Pt::PosInf, 1)];
    }
    if a == b.neg(inf) {
        return vec![(Pt::NegInf, 1)];
    }
    match direction {
        Direction::FromAbove => vec![(Pt::Fin(a), 1)],
        Direction::FromBelow => vec![(Pt::Fin(a), -1)],
        Direction::TwoSided => vec![(Pt::Fin(a), 1), (Pt::Fin(a), -1)],
    }
}

/// `g` at the point as a function of `t → +inf` (x = a + side/t, t, -t), with the symbol `t`.
fn at_infinity<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, g: T<'h>, x: T<'h>, pt: Pt<'h>, side: i8) -> Res<Option<(T<'h>, T<'h>)>> {
    let h = b.heap();
    let t = ["t%more", "t%more2"].into_iter().map(|n| h.sym_term(h.sym(n))).find(|&t| is_free(b, g, t));
    let Some(t) = t else { return Ok(None) };
    let at = match pt {
        Pt::Fin(c) => b.add(&[c, b.mul(&[b.int(side.into()), b.powi(t, -1)])]),
        Pt::PosInf => t,
        Pt::NegInf => b.neg(t),
    };
    let gt = subst(cx, b, g, x, at)?;
    Ok((!is_failure(b, gt)).then_some((gt, t)))
}

/// The limit of `e` as `t → +inf`: the walk, then Gruntz's algorithm.
fn lim_inf<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, e: T<'h>, t: T<'h>, deadline: Instant) -> Res<Option<Lim<'h>>> {
    if let Some(l) = super::walk::limit(cx, b, e, t, Pt::PosInf, 1)? {
        return Ok(Some(l));
    }
    gruntz::at_infinity(cx, b, e, t, deadline)
}

/// `max_limit` (`upper`) or `min_limit` of `f` as `x → a`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn extreme<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    b: &Build<'h>,
    f: T<'h>,
    x: T<'h>,
    a: T<'h>,
    direction: Direction,
    budget: Duration,
    upper: bool,
) -> Res<Option<T<'h>>> {
    if is_free(b, f, x) {
        return Ok(Some(f));
    }
    let deadline = Instant::now() + budget;
    let mut best: Option<Ext<'h>> = None;
    for (pt, side) in sides(b, a, direction) {
        let g = crate::switches::resolve_near(cx, b, f, x, pt, side)?.unwrap_or(f);
        let e = match one_sided(cx, b, g, x, pt, side, deadline, budget, true)? {
            Some(Lim::Fin(v, _)) => Ext::Fin(v),
            Some(Lim::Inf(s)) => Ext::Inf(s),
            None => match oscillation(cx, b, g, x, pt, side, deadline, upper)? {
                Some(e) => e,
                None => return Ok(None),
            },
        };
        best = Some(match best {
            None => e,
            Some(p) => match pick(cx, b, p, e, upper)? {
                Some(q) => q,
                None => return Ok(None),
            },
        });
    }
    Ok(best.map(|e| value(b, e)))
}

/// The larger (`upper`) or smaller of two values, or `None` when not decided.
fn pick<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, p: Ext<'h>, q: Ext<'h>, upper: bool) -> Res<Option<Ext<'h>>> {
    let key = |e: Ext<'h>| match e {
        Ext::Inf(s) => Some(s),
        Ext::Fin(_) => None,
    };
    let want: i8 = if upper { 1 } else { -1 };
    Ok(match (key(p), key(q)) {
        (Some(s), _) if s == want => Some(p),
        (_, Some(s)) if s == want => Some(q),
        (Some(_), _) => Some(q),
        (_, Some(_)) => Some(p),
        _ => {
            let (Ext::Fin(u), Ext::Fin(v)) = (p, q) else { return Ok(None) };
            if u == v {
                return Ok(Some(p));
            }
            let d = apply(cx, b, "expand", &[b.sub(u, v)])?;
            match sign(cx, b, d)? {
                Some(s) if s * want >= 0 => Some(p),
                Some(_) => Some(q),
                None => None,
            }
        }
    })
}

fn value<'h>(b: &Build<'h>, e: Ext<'h>) -> T<'h> {
    let inf = b.heap().sym_term(b.heap().sym("inf"));
    match e {
        Ext::Fin(v) => v,
        Ext::Inf(1) => inf,
        Ext::Inf(_) => b.neg(inf),
    }
}

/// The limit superior (`upper`) or inferior of a sum with bounded oscillating terms (module comment).
#[allow(clippy::too_many_arguments)]
fn oscillation<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    b: &Build<'h>,
    g: T<'h>,
    x: T<'h>,
    pt: Pt<'h>,
    side: i8,
    deadline: Instant,
    upper: bool,
) -> Res<Option<Ext<'h>>> {
    let Some((gt, t)) = at_infinity(cx, b, g, x, pt, side)? else { return Ok(None) };
    let terms: Vec<T<'h>> = args_of(b, gt, b.hd.add).map_or_else(|| vec![gt], |a| a.to_vec());
    let (mut fixed, mut arg, mut amp_sin, mut amp_cos) = (vec![], None, vec![], vec![]);
    let mut infinite: Option<i8> = None;
    for &term in &terms {
        if let Some(l) = lim_inf(cx, b, term, t, deadline)? {
            match l {
                Lim::Fin(v, _) => fixed.push(v),
                Lim::Inf(s) => match infinite {
                    Some(q) if q != s => return Ok(None),
                    _ => infinite = Some(s),
                },
            }
            continue;
        }
        // c * sin(u) or c * cos(u)
        let fs: Vec<T<'h>> = args_of(b, term, b.hd.mul).map_or_else(|| vec![term], |a| a.to_vec());
        let at: Vec<usize> = (0..fs.len()).filter(|&i| matches!(call_of(b, fs[i]), Some((n, [_])) if n == "sin" || n == "cos")).collect();
        let [i] = at.as_slice() else { return Ok(None) };
        let Some((name, &[u])) = call_of(b, fs[*i]) else { return Ok(None) };
        match arg {
            None => {
                if !matches!(lim_inf(cx, b, u, t, deadline)?, Some(Lim::Inf(_))) {
                    return Ok(None);
                }
                arg = Some(u);
            }
            Some(v) if v == u => {}
            Some(_) => return Ok(None),
        }
        let c = cx.eval(b.mul(&fs.iter().enumerate().filter(|&(j, _)| j != *i).map(|(_, &f)| f).collect::<Vec<_>>()))?;
        match lim_inf(cx, b, c, t, deadline)? {
            Some(Lim::Fin(v, _)) => {
                if name == "sin" {
                    amp_sin.push(v)
                } else {
                    amp_cos.push(v)
                }
            }
            // a growing amplitude: the values reach both infinities
            Some(Lim::Inf(_)) => return Ok(Some(Ext::Inf(if upper { 1 } else { -1 }))),
            None => return Ok(None),
        }
    }
    if let Some(s) = infinite {
        return Ok(Some(Ext::Inf(s)));
    }
    if arg.is_none() {
        return Ok(None);
    }
    let (a, c) = (cx.eval(b.add(&amp_sin))?, cx.eval(b.add(&amp_cos))?);
    let amp = apply(cx, b, "sqrt", &[b.add(&[b.powi(a, 2), b.powi(c, 2)])])?;
    let l = cx.eval(b.add(&fixed))?;
    let v = cx.eval(if upper { b.add(&[l, amp]) } else { b.sub(l, amp) })?;
    Ok(Some(Ext::Fin(v)))
}

/// `discrete_limit(f, n -> inf)` (module comment).
pub(crate) fn discrete<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    b: &Build<'h>,
    f: T<'h>,
    n: T<'h>,
    a: T<'h>,
    opts: &super::Options,
) -> Res<Outcome<'h>> {
    let h = b.heap();
    let inf = h.sym_term(h.sym("inf"));
    if a != inf && a != b.neg(inf) {
        return Ok(Outcome::Unknown);
    }
    let sigma = h.sym_term(h.sym("sigma%discrete"));
    let mut memo = rustc_hash::FxHashMap::default();
    let g = cx.eval(binet(b, f, n, sigma, &mut memo))?;
    if is_free(b, g, sigma) {
        return super::limit_with(cx, b, g, n, a, Direction::TwoSided, opts);
    }
    // even and odd terms
    let mut vals = vec![];
    for s in [1, -1] {
        let gs = subst(cx, b, g, sigma, b.int(s))?;
        match super::limit_with(cx, b, gs, n, a, Direction::TwoSided, opts)? {
            Outcome::Value(v) => vals.push(v),
            _ => return Ok(Outcome::Unknown),
        }
    }
    if vals[0] == vals[1] {
        return Ok(Outcome::Value(vals[0]));
    }
    let inf_like = |v: T<'h>| v == inf || v == b.neg(inf);
    if inf_like(vals[0]) || inf_like(vals[1]) {
        return Ok(Outcome::NoLimit);
    }
    let d = apply(cx, b, "expand", &[b.sub(vals[0], vals[1])])?;
    Ok(match sign(cx, b, d)? {
        Some(0) => Outcome::Value(vals[0]),
        Some(_) => Outcome::NoLimit,
        None => Outcome::Unknown,
    })
}

/// `f` with `fibonacci(m)`, `lucas_l(m)` by Binet's formulas, `(-c)^m` as `(-1)^m c^m`, and `(-1)^(k n + j)`
/// (integers k, j) as `(-1)^j σ^k` for the symbol σ = (-1)^n.
fn binet<'h>(b: &Build<'h>, f: T<'h>, n: T<'h>, sigma: T<'h>, memo: &mut rustc_hash::FxHashMap<T<'h>, T<'h>>) -> T<'h> {
    let h = b.heap();
    if is_free(b, f, n) {
        return f;
    }
    if let Some(&r) = memo.get(&f) {
        return r;
    }
    let View::App { head, args } = h.view(f) else { return f };
    let na: Vec<T<'h>> = args.iter().map(|&a| binet(b, a, n, sigma, memo)).collect();
    let five = b.int(5);
    let phi = b.mul(&[b.rat(1, 2), b.add(&[b.one(), b.pow(five, b.rat(1, 2))])]);
    let hs = b.sym_of(head);
    let r = if hs == Some(b.hd.add) {
        b.add(&na)
    } else if hs == Some(b.hd.mul) {
        b.mul(&na)
    } else {
        match (hs.map(|s| h.sym_name(s)).as_deref(), na.as_slice()) {
            (Some("fibonacci"), [m]) => {
                let alt = minus_one_power(b, *m, n, sigma).unwrap_or_else(|| b.pow(b.int(-1), *m));
                b.mul(&[b.pow(five, b.rat(-1, 2)), b.sub(b.pow(phi, *m), b.mul(&[alt, b.pow(phi, b.neg(*m))]))])
            }
            (Some("lucas_l"), [m]) => {
                let alt = minus_one_power(b, *m, n, sigma).unwrap_or_else(|| b.pow(b.int(-1), *m));
                b.add(&[b.pow(phi, *m), b.mul(&[alt, b.pow(phi, b.neg(*m))])])
            }
            (Some("pow"), [base, m]) if !is_free(b, *m, n) && is_free(b, *base, n) => match b.ar.num(*base) {
                Some(mc_arith::Num::Real(r)) if r.is_negative() => {
                    let c = b.neg(*base);
                    let alt = minus_one_power(b, *m, n, sigma).unwrap_or_else(|| b.pow(b.int(-1), *m));
                    b.mul(&[alt, b.pow(c, *m)])
                }
                _ => b.pow(*base, *m),
            },
            (Some("pow"), [p, q]) => b.pow(*p, *q),
            _ => b.app(head, &na),
        }
    };
    memo.insert(f, r);
    r
}

/// `(-1)^m` as `(-1)^j σ^k` when `m = k n + j` with integers k, j.
fn minus_one_power<'h>(b: &Build<'h>, m: T<'h>, n: T<'h>, sigma: T<'h>) -> Option<T<'h>> {
    let h = b.heap();
    let terms: Vec<T<'h>> = args_of(b, m, b.hd.add).map_or_else(|| vec![m], |a| a.to_vec());
    let (mut k, mut j) = (0i64, 0i64);
    for &u in &terms {
        if u == n {
            k += 1;
            continue;
        }
        match h.view(u) {
            View::Int(v) => j += v,
            _ => match args_of(b, u, b.hd.mul) {
                Some([c, v]) if *v == n => match h.view(*c) {
                    View::Int(c) => k += c,
                    _ => return None,
                },
                _ => return None,
            },
        }
    }
    Some(b.mul(&[b.int(if j.rem_euclid(2) == 0 { 1 } else { -1 }), b.powi(sigma, k.rem_euclid(2))]))
}

/// `asymptotic(f, x -> a)` (module comment): the leading term, or `None`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn asymptotic<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    b: &Build<'h>,
    stop: &std::sync::Arc<std::sync::atomic::AtomicBool>,
    f: T<'h>,
    x: T<'h>,
    a: T<'h>,
    direction: Direction,
    budget: Duration,
) -> Res<Option<T<'h>>> {
    let h = b.heap();
    if is_free(b, f, x) {
        return Ok(Some(f));
    }
    let inf = h.sym_term(h.sym("inf"));
    if a == inf || a == b.neg(inf) {
        let neg = a != inf;
        let t = h.sym_term(h.sym("t%asymptotic"));
        let g = subst(cx, b, f, x, if neg { b.neg(t) } else { t })?;
        let deadline = Instant::now() + budget;
        let mut results = vec![];
        for terms in [gruntz::TERMS.0, gruntz::TERMS.1] {
            let mut gr = Gruntz::new(b, t, deadline, terms);
            let Some(p) = gr.prepare(cx, g)? else { return Ok(None) };
            let Some(r) = leading(cx, b, &mut gr, p, 0)? else { return Ok(None) };
            let r = cx.eval(gr.unwrap_g(r))?;
            if !gr.truncated {
                results.push(r);
                break;
            }
            results.push(r);
        }
        if results.len() == 2 && results[0] != results[1] {
            return Ok(None);
        }
        let r = subst(cx, b, results[0], t, if neg { b.neg(x) } else { x })?;
        return Ok(Some(r));
    }
    // a finite point: the first nonzero term of the expansion in t = ±(x - a)
    let side: i64 = if direction == Direction::FromBelow { -1 } else { 1 };
    let t = h.sym_term(h.sym("t%asymptotic"));
    let g = subst(cx, b, f, x, b.add(&[a, b.mul(&[b.int(side), t])]))?;
    if is_failure(b, g) {
        return Ok(None);
    }
    let dx = b.mul(&[b.int(side), b.sub(x, a)]);
    for n in [2i64, 6, 14] {
        let Some(ex) = mc_poly::expansion(cx, stop, g, t, n, true, None)? else { return Ok(None) };
        for (i, &c) in ex.coeffs.iter().enumerate() {
            if c == b.zero() || is_failure(b, c) {
                if is_failure(b, c) {
                    return Ok(None);
                }
                continue;
            }
            let c = subst(cx, b, c, t, dx)?;
            let r = cx.eval(b.mul(&[c, b.pow(dx, b.rat(ex.val + i as i64, ex.den))]))?;
            return Ok(Some(r));
        }
    }
    Ok(None)
}

/// The asymptotic leading term of `e` (in the variable of `gr`, tending to +inf), recursively.
fn leading<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, gr: &mut Gruntz<'_, 'h>, e: T<'h>, depth: u32) -> Res<Option<T<'h>>> {
    let x = gr.variable();
    if is_free(b, e, x) {
        return Ok(Some(e));
    }
    if depth > 24 {
        return Ok(None);
    }
    let Some(Lead::Term { c, e: ex, w }) = gr.leadterm(cx, e)? else { return Ok(None) };
    let Some(cl) = leading(cx, b, gr, c, depth + 1)? else { return Ok(None) };
    if ex == b.zero() {
        return Ok(Some(cl));
    }
    let Some(w) = principal(cx, b, gr, w, ex)? else { return Ok(None) };
    Ok(Some(b.mul(&[cl, w])))
}

/// ω^ex with the terms of ω's exponent that tend to 0 removed: `exp(s) ~ exp(s0)` when `s - s0 → 0`.
fn principal<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, gr: &mut Gruntz<'_, 'h>, w: T<'h>, ex: T<'h>) -> Res<Option<T<'h>>> {
    let Some(s) = gr.exp_argument(w) else {
        // a power v^k (x moved down): v^(k ex)
        let w = gr.unwrap_g(w);
        if let Some([v, k]) = args_of(b, w, b.hd.pow) {
            return Ok(Some(cx.eval(b.pow(*v, b.mul(&[*k, ex])))?));
        }
        return Ok(Some(cx.eval(b.pow(w, ex))?));
    };
    let s = b.mul(&[s, ex]);
    let s = apply(cx, b, "expand", &[gr.unwrap_g(s)])?;
    let s = gr.to_g(s);
    let terms: Vec<T<'h>> = args_of(b, s, b.hd.add).map_or_else(|| vec![s], |a| a.to_vec());
    let mut keep = vec![];
    for &u in &terms {
        match gr.limit(cx, u)? {
            Some(Lim::Fin(v, _)) if v == b.zero() => {}
            Some(_) => keep.push(u),
            None => return Ok(None),
        }
    }
    // e^(s1 + s2 + ...) as e^s1 e^s2 ..., with e^(c log u) as u^c
    let mut factors: Vec<T<'h>> = vec![];
    for &u in &keep {
        let u = gr.unwrap_g(u);
        let fs: Vec<T<'h>> = args_of(b, u, b.hd.mul).map_or_else(|| vec![u], |a| a.to_vec());
        let at: Vec<usize> = (0..fs.len()).filter(|&i| matches!(call_of(b, fs[i]), Some((n, [_])) if n == "log")).collect();
        match at.as_slice() {
            [i] => {
                let Some((_, &[v])) = call_of(b, fs[*i]) else { unreachable!("a log call") };
                let c = b.mul(&fs.iter().enumerate().filter(|&(j, _)| j != *i).map(|(_, &f)| f).collect::<Vec<_>>());
                factors.push(b.pow(v, c));
            }
            _ => factors.push(b.pow(b.sym(b.hd.e), u)),
        }
    }
    Ok(Some(cx.eval(b.mul(&factors))?))
}
