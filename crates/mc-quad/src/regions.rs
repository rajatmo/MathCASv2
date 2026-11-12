//! Symbolic preparation of two kinds of integrands before the numeric rules (round 19, bugs Q19-01, Q19-16,
//! Q19-17):
//!
//! - **Indicator functions** `boole(cond) g` of semialgebraic regions. Adaptive rules see a discontinuous
//!   integrand that is zero at most of their nodes: a small region can be missed entirely (an estimate of 0 with
//!   a small error) and a disk in a square makes the error heap refine along the whole boundary. Instead the region
//!   `cond` intersected with the box of the ranges is given to `cylindrical_decomposition` (Collins' CAD, mc-cad);
//!   each full-dimensional cell `l_1 < x_1 < u_1, l_2(x_1) < x_2 < u_2(x_1), ...` is a nested integral of g with
//!   these limits, and the integral is their sum (cells of lower dimension have measure zero). The boundary is then
//!   integrated exactly by the limits, and the rules see smooth integrands with algebraic endpoint singularities,
//!   which the double-exponential rule takes (Collins 1975 for the decomposition; its use for integration over
//!   semialgebraic sets as in Strzeboński 2006).
//! - **Squares of oscillating functions on half-lines** (`sin(a x)^2/x^2`): such an integrand oscillates without
//!   changing sign, so the method of zeros (Longman) does not apply and the adaptive rules converge slowly on the
//!   oscillating tail. The integrand is reduced (`trig_reduce`: `sin(u)^2 = (1 - cos(2 u))/2`) and split into its
//!   oscillating terms, which change sign and go to the method of zeros, and the others; both are integrated over
//!   the tail beyond one unit from the finite end, the integrand as given over that first unit.
//!
//! - **Logarithms on a half-line** `[a, inf)`, `a > 0`: an integrand with `log(x)` often decays like a power of
//!   `1/log(x)` (`1/(x (1 + log(x)^2))`), which no rational map of the half-line resolves; the error estimate of
//!   the rules was then far too small (an estimate off by 1.4e-3 with an error of 2e-7). With `x = e^u` the
//!   integrand `e^u f(e^u)` decays like a power of `1/u` on `[log(a), inf)`, which the ordinary map handles.
//!
//! All return `None` when they do not apply (the ordinary path follows).
use mc_api::names;
use mc_core::{Heap, Store, Sym, View, T};
use mc_eval::{NativeCx, Res};

/// The most cells of a decomposition integrated one by one.
const MAX_CELLS: usize = 256;

/// The most variables given to the decomposition.
const MAX_VARS: usize = 4;

fn is_head<'h>(h: &'h Heap, t: T<'h>, name: &str) -> bool {
    matches!(h.view(t), View::App { head, .. } if head == h.sym_term(h.sym(name)))
}

/// Does `t` contain one of `syms`?
fn mentions<'h>(h: &'h Heap, t: T<'h>, syms: &[T<'h>]) -> bool {
    let mut stack = vec![t];
    while let Some(u) = stack.pop() {
        if syms.contains(&u) {
            return true;
        }
        if let View::App { head, args } = h.view(u) {
            stack.push(head);
            stack.extend_from_slice(args);
        }
    }
    false
}

/// `n_integrate(g, ranges..., options)` evaluated.
fn n_integrate<'h>(cx: &mut NativeCx<'_, '_, 'h>, g: T<'h>, ranges: &[T<'h>], opt: Option<T<'h>>) -> Res<T<'h>> {
    let h = cx.heap();
    let mut args = vec![g];
    args.extend_from_slice(ranges);
    args.extend(opt);
    cx.eval(h.call(h.sym("n_integrate"), &args))
}

/// Is `t` a number (a machine real, a ball or a complex number)?
fn numeric<'h>(h: &'h Heap, t: T<'h>) -> bool {
    match h.view(t) {
        View::Int(_) | View::Real(_) | View::Ball { .. } | View::Rational { .. } => true,
        View::App { head, .. } => head == h.sym_term(h.sym(names::COMPLEX)) || head == h.sym_term(h.sym("around")),
        _ => false,
    }
}

/// The sum of numeric values, or the first failure among them.
fn total<'h>(cx: &mut NativeCx<'_, '_, 'h>, vals: &[T<'h>]) -> Res<T<'h>> {
    let h = cx.heap();
    if vals.is_empty() {
        return Ok(h.real(0.0));
    }
    cx.eval(h.call(h.sym(names::ADD), vals))
}

/// The integral of `boole(cond) g` over the ranges by the cells of the region (module comment), or `None`.
pub(crate) fn boole_cells<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    f: T<'h>,
    vars: &[Sym],
    pts: &[Vec<T<'h>>],
    opt: Option<T<'h>>,
) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    if vars.len() > MAX_VARS {
        return Ok(None);
    }
    let xs: Vec<T<'h>> = vars.iter().map(|&s| h.sym_term(s)).collect();
    // f = boole(c_1) ... boole(c_k) g
    let factors: Vec<T<'h>> = match h.view(f) {
        View::App { head, args } if head == h.sym_term(h.sym(names::MUL)) => args.to_vec(),
        _ => vec![f],
    };
    let (mut conds, mut rest) = (vec![], vec![]);
    for t in factors {
        match h.view(t) {
            View::App { head, args: &[c] } if head == h.sym_term(h.sym("boole")) && mentions(h, c, &xs) => conds.push(c),
            _ => rest.push(t),
        }
    }
    if conds.is_empty() || rest.iter().any(|&t| is_head(h, t, "boole")) {
        return Ok(None);
    }
    let g = match rest.as_slice() {
        [] => h.int(1),
        [t] => *t,
        _ => h.call(h.sym(names::MUL), &rest),
    };
    // the region: the conditions and the box
    let inf = h.sym_term(h.sym("inf"));
    let finite = |t: T<'h>| !mentions(h, t, &[inf]);
    let mut region = conds.clone();
    for (k, ps) in pts.iter().enumerate() {
        let (lo, hi) = (ps[0], ps[ps.len() - 1]);
        if finite(lo) {
            region.push(h.call(h.sym("less_equal"), &[lo, xs[k]]));
        }
        if finite(hi) {
            region.push(h.call(h.sym("less_equal"), &[xs[k], hi]));
        }
    }
    let region = h.call(h.sym("and"), &region);
    let cad = cx.eval(h.call(h.sym("cylindrical_decomposition"), &[region, cx.list(&xs)]))?;
    if is_head(h, cad, "cylindrical_decomposition") || is_head(h, cad, names::FAILURE) {
        return Ok(None);
    }
    if cad == h.sym_term(h.sym(names::FALSE)) {
        return Ok(Some(h.real(0.0)));
    }
    let Some(cells) = dnf(h, cad, MAX_CELLS) else { return Ok(None) };
    let mut vals = vec![];
    'cells: for cell in cells {
        cx.tick(1)?;
        let mut lo: Vec<Option<T<'h>>> = vec![None; xs.len()];
        let mut hi: Vec<Option<T<'h>>> = vec![None; xs.len()];
        for atom in cell {
            if atom == h.sym_term(h.sym(names::TRUE)) {
                continue;
            }
            let View::App { head, args: &[l, r] } = h.view(atom) else { return Ok(None) };
            let View::Sym(rel) = h.view(head) else { return Ok(None) };
            let rel = h.sym_name(rel);
            let level = |v: T<'h>, other: T<'h>| xs.iter().position(|&x| x == v).filter(|&k| !mentions(h, other, &xs[k..]));
            // (level, the bound, is it a lower bound)
            let (k, bound, lower) = match (level(l, r), level(r, l)) {
                (Some(k), _) => (k, r, matches!(rel.as_str(), "greater" | "greater_equal")),
                (None, Some(k)) => (k, l, matches!(rel.as_str(), "less" | "less_equal")),
                _ => return Ok(None),
            };
            match rel.as_str() {
                // a cell of lower dimension: measure zero
                "equal" => continue 'cells,
                "unequal" => continue,
                "less" | "less_equal" | "greater" | "greater_equal" => {}
                _ => return Ok(None),
            }
            let slot = if lower { &mut lo[k] } else { &mut hi[k] };
            if slot.is_some() {
                return Ok(None);
            }
            *slot = Some(bound);
        }
        let ranges: Vec<T<'h>> = (0..xs.len())
            .map(|k| cx.list(&[xs[k], lo[k].unwrap_or(pts[k][0]), hi[k].unwrap_or(pts[k][pts[k].len() - 1])]))
            .collect();
        let v = n_integrate(cx, g, &ranges, opt)?;
        if !numeric(h, v) {
            // a failure of a cell (not converged, not numeric) is the failure of the whole
            return Ok(Some(v));
        }
        vals.push(v);
    }
    Ok(Some(total(cx, &vals)?))
}

/// The disjunctive normal form of a formula of `and`, `or` and atoms: the conjunctions as lists of atoms; `None`
/// past `cap` conjunctions.
fn dnf<'h>(h: &'h Heap, t: T<'h>, cap: usize) -> Option<Vec<Vec<T<'h>>>> {
    match h.view(t) {
        View::App { head, args } if head == h.sym_term(h.sym("or")) => {
            let mut out = vec![];
            for &a in args {
                out.extend(dnf(h, a, cap)?);
                if out.len() > cap {
                    return None;
                }
            }
            Some(out)
        }
        View::App { head, args } if head == h.sym_term(h.sym("and")) => {
            let mut acc: Vec<Vec<T<'h>>> = vec![vec![]];
            for &a in args {
                let d = dnf(h, a, cap)?;
                let mut next = Vec::with_capacity(acc.len() * d.len());
                for c in &acc {
                    for e in &d {
                        let mut x = c.clone();
                        x.extend_from_slice(e);
                        next.push(x);
                    }
                }
                if next.len() > cap {
                    return None;
                }
                acc = next;
            }
            Some(acc)
        }
        _ => Some(vec![vec![t]]),
    }
}

/// Does `t` contain `sin(u)^k` or `cos(u)^k` (k >= 2) with u depending on `x`?
fn has_trig_power<'h>(h: &'h Heap, t: T<'h>, x: T<'h>) -> bool {
    let mut stack = vec![t];
    while let Some(u) = stack.pop() {
        if let View::App { head, args } = h.view(u) {
            if head == h.sym_term(h.sym(names::POW)) {
                if let [b, e] = args {
                    if matches!(h.view(*e), View::Int(k) if k >= 2) && (is_head(h, *b, "sin") || is_head(h, *b, "cos")) && mentions(h, *b, &[x]) {
                        return true;
                    }
                }
            }
            stack.extend_from_slice(args);
        }
    }
    false
}

/// The integral over a half-line or the whole line of an integrand with squares of sines or cosines (module
/// comment), or `None`.
pub(crate) fn squared_trig_tail<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    f: T<'h>,
    vars: &[Sym],
    pts: &[Vec<T<'h>>],
    opt: Option<T<'h>>,
) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    let ([x], [ps]) = (vars, pts) else { return Ok(None) };
    let x = h.sym_term(*x);
    if ps.len() != 2 || !has_trig_power(h, f, x) {
        return Ok(None);
    }
    let inf = h.sym_term(h.sym("inf"));
    let (lo, hi) = (ps[0], ps[1]);
    let (lo_inf, hi_inf) = (mentions(h, lo, &[inf]), mentions(h, hi, &[inf]));
    if !lo_inf && !hi_inf {
        return Ok(None);
    }
    // the oscillating and the other terms of the reduced integrand
    let r = cx.eval(h.call(h.sym("expand"), &[h.call(h.sym("trig_reduce"), &[f])]))?;
    let terms: Vec<T<'h>> = match h.view(r) {
        View::App { head, args } if head == h.sym_term(h.sym(names::ADD)) => args.to_vec(),
        _ => vec![r],
    };
    let oscillates = |t: T<'h>| {
        let mut stack = vec![t];
        while let Some(u) = stack.pop() {
            if let View::App { head, args } = h.view(u) {
                if (head == h.sym_term(h.sym("sin")) || head == h.sym_term(h.sym("cos"))) && mentions(h, u, &[x]) {
                    return true;
                }
                stack.extend_from_slice(args);
            }
        }
        false
    };
    let (osc, other): (Vec<T<'h>>, Vec<T<'h>>) = terms.iter().partition(|&&t| oscillates(t));
    if osc.is_empty() || other.is_empty() {
        return Ok(None);
    }
    let osc = cx.eval(h.call(h.sym(names::ADD), &osc))?;
    let other = cx.eval(h.call(h.sym(names::ADD), &other))?;
    let add = |cx: &mut NativeCx<'_, '_, 'h>, a: T<'h>, k: i64| cx.eval(h.call(h.sym(names::ADD), &[a, h.int(k)]));
    let (m_lo, m_hi) = match (lo_inf, hi_inf) {
        (true, true) => (h.int(-1), h.int(1)),
        (false, true) => (lo, add(cx, lo, 1)?),
        _ => (add(cx, hi, -1)?, hi),
    };
    // the first unit (two halves on the whole line: a rule's middle node at 0 would meet 0/0 in sin(x)^2/x^2)
    let mut vals = vec![];
    if lo_inf && hi_inf {
        for (l, r) in [(m_lo, h.int(0)), (h.int(0), m_hi)] {
            vals.push(n_integrate(cx, f, &[cx.list(&[x, l, r])], opt)?);
        }
    } else {
        vals.push(n_integrate(cx, f, &[cx.list(&[x, m_lo, m_hi])], opt)?);
    }
    if hi_inf {
        for g in [osc, other] {
            vals.push(n_integrate(cx, g, &[cx.list(&[x, m_hi, hi])], opt)?);
        }
    }
    if lo_inf {
        for g in [osc, other] {
            vals.push(n_integrate(cx, g, &[cx.list(&[x, lo, m_lo])], opt)?);
        }
    }
    if vals.iter().any(|&v| !numeric(h, v)) {
        return Ok(None);
    }
    Ok(Some(total(cx, &vals)?))
}

/// Does `t` contain `log(u)` with u depending on `x`?
fn has_log_of<'h>(h: &'h Heap, t: T<'h>, x: T<'h>) -> bool {
    let mut stack = vec![t];
    while let Some(u) = stack.pop() {
        if let View::App { head, args } = h.view(u) {
            if head == h.sym_term(h.sym("log")) && args.len() == 1 && mentions(h, args[0], &[x]) {
                return true;
            }
            stack.extend_from_slice(args);
        }
    }
    false
}

/// The integral over `[a, inf)` (`a > 0`) of an integrand with a logarithm of the variable after `x = e^u` (module
/// comment), or `None`.
pub(crate) fn log_half_line<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    f: T<'h>,
    vars: &[Sym],
    pts: &[Vec<T<'h>>],
    opt: Option<T<'h>>,
) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    let ([x], [ps]) = (vars, pts) else { return Ok(None) };
    let x = h.sym_term(*x);
    let inf = h.sym_term(h.sym("inf"));
    if ps.len() != 2 || ps[1] != inf || mentions(h, ps[0], &[inf]) || !has_log_of(h, f, x) {
        return Ok(None);
    }
    let a = ps[0];
    let pos = cx.eval(h.call(h.sym("greater"), &[a, h.int(0)]))?;
    if pos != h.sym_term(h.sym(names::TRUE)) {
        return Ok(None);
    }
    // only for a decay slower than every power: x^1.1 |f(x)| not falling from 10^4 to 10^8
    let at = |cx: &mut NativeCx<'_, '_, 'h>, v: f64| -> Res<Option<f64>> {
        let y = cx.eval(h.call(h.sym("rewrite"), &[f, h.call(h.sym(names::RULE), &[x, h.real(v)])]))?;
        let y = cx.eval(h.call(h.sym("abs"), &[y]))?;
        let y = cx.eval(h.call(h.sym("n"), &[y]))?;
        Ok(match h.view(y) {
            View::Real(r) if r.is_finite() => Some(r * v.powf(1.1)),
            _ => None,
        })
    };
    let (Some(g4), Some(g8)) = (at(cx, 1e4)?, at(cx, 1e8)?) else { return Ok(None) };
    if !(g8 >= 0.5 * g4 && g8 > 0.0) {
        return Ok(None);
    }
    let ex = h.call(h.sym(names::POW), &[h.sym_term(h.sym("e")), x]);
    let g = cx.eval(h.call(h.sym("rewrite"), &[f, h.call(h.sym(names::RULE), &[x, ex])]))?;
    let g = cx.eval(h.call(h.sym(names::MUL), &[g, ex]))?;
    let la = cx.eval(h.call(h.sym("log"), &[a]))?;
    let v = n_integrate(cx, g, &[cx.list(&[x, la, inf])], opt)?;
    Ok(numeric(h, v).then_some(v))
}

// ---------------------------------------------------------------- steps and delta functions

/// The machine value of `t` (`inf`, `-inf`, or a real number through `n`).
fn real_of<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>) -> Res<Option<f64>> {
    let h = cx.heap();
    let inf = h.sym_term(h.sym("inf"));
    if t == inf {
        return Ok(Some(f64::INFINITY));
    }
    if let View::App { head, args: &[c, i] } = h.view(t) {
        if head == h.sym_term(h.sym(names::MUL)) && i == inf {
            if let View::Int(c) = h.view(c) {
                return Ok(Some(if c > 0 { f64::INFINITY } else { f64::NEG_INFINITY }));
            }
        }
    }
    let v = cx.eval(h.call(h.sym("n"), &[t]))?;
    Ok(match h.view(v) {
        View::Real(r) if r.is_finite() => Some(r),
        View::Int(i) => Some(i as f64),
        _ => None,
    })
}

/// `g` at `x = v` as a machine real.
fn at_real<'h>(cx: &mut NativeCx<'_, '_, 'h>, g: T<'h>, x: T<'h>, v: f64) -> Res<Option<f64>> {
    let h = cx.heap();
    let y = cx.eval(h.call(h.sym("rewrite"), &[g, h.call(h.sym(names::RULE), &[x, h.real(v)])]))?;
    real_of(cx, y)
}

/// `u = c1 x + c0` (checked at four points): `(root, c1)`.
fn linear_root<'h>(cx: &mut NativeCx<'_, '_, 'h>, u: T<'h>, x: T<'h>) -> Res<Option<(f64, f64)>> {
    let mut v = [0.0; 4];
    for (i, p) in [0.0, 1.0, 2.0, -3.0].into_iter().enumerate() {
        let Some(y) = at_real(cx, u, x, p)? else { return Ok(None) };
        v[i] = y;
    }
    let c1 = v[1] - v[0];
    let scale = v.iter().fold(1e-300f64, |m, y| m.max(y.abs()));
    if c1 == 0.0 || (v[2] - (v[0] + 2.0 * c1)).abs() > 1e-10 * scale || (v[3] - (v[0] - 3.0 * c1)).abs() > 1e-10 * scale {
        return Ok(None);
    }
    Ok(Some((-v[0] / c1, c1)))
}

/// The root of the linear `u` as an exact term (`-u(0)/(u(1) - u(0))` evaluated), or the machine real `r` when the
/// coefficients are not exact.
fn root_term<'h>(cx: &mut NativeCx<'_, '_, 'h>, u: T<'h>, x: T<'h>, r: f64) -> Res<T<'h>> {
    let h = cx.heap();
    let at = |cx: &mut NativeCx<'_, '_, 'h>, v: i64| cx.eval(h.call(h.sym("rewrite"), &[u, h.call(h.sym(names::RULE), &[x, h.int(v)])]));
    let (u0, u1) = (at(cx, 0)?, at(cx, 1)?);
    let c1 = cx.eval(h.call(h.sym(names::ADD), &[u1, h.call(h.sym(names::MUL), &[h.int(-1), u0])]))?;
    let q = cx.eval(h.call(h.sym(names::POW), &[c1, h.int(-1)]))?;
    let t = cx.eval(h.call(h.sym(names::MUL), &[h.int(-1), u0, q]))?;
    let exact = !mentions(h, t, &[x]) && !matches!(h.view(t), View::Real(_) | View::Ball { .. }) && !mentions(h, t, &[h.sym_term(h.sym("failure"))]);
    Ok(if exact { t } else if r == 0.0 { h.int(0) } else { h.real(r) })
}

/// The arguments of the one-argument calls of `heads` in `t` that depend on `x`.
fn calls_of<'h>(h: &'h Heap, t: T<'h>, heads: &[&str], x: T<'h>) -> Vec<T<'h>> {
    let heads: Vec<T<'h>> = heads.iter().map(|n| h.sym_term(h.sym(n))).collect();
    let mut out = vec![];
    let mut stack = vec![t];
    let mut seen = 0;
    while let Some(u) = stack.pop() {
        seen += 1;
        if seen > 50_000 {
            break;
        }
        if let View::App { head, args } = h.view(u) {
            if args.len() == 1 && heads.contains(&head) && mentions(h, args[0], &[x]) {
                out.push(args[0]);
            }
            stack.push(head);
            stack.extend_from_slice(args);
        }
    }
    out
}

/// Step functions and delta functions of a linear argument on one range:
///
/// - `unit_step(u)` and `heaviside_theta(u)` jump at the root of `u`; a rule that samples a function with a jump
///   sees it only by chance (all the nodes of a transformed infinite range may fall on one side: an estimate of 0
///   with no error). The root becomes a break point of the range, so every piece is smooth.
/// - `g(x) dirac_delta(u(x))` integrates to `g(x0)/|u'|` at the root `x0` of `u` inside the range (the sifting
///   property), 0 outside; a sum of such terms is taken term by term.
///
/// `None` when the integrand has neither or the argument is not linear.
pub(crate) fn steps_and_deltas<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    f: T<'h>,
    vars: &[Sym],
    pts: &[Vec<T<'h>>],
    opt: Option<T<'h>>,
) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    let ([xs], [ps]) = (vars, pts) else { return Ok(None) };
    if ps.len() < 2 {
        return Ok(None);
    }
    let x = h.sym_term(*xs);
    let deltas = calls_of(h, f, &["dirac_delta"], x);
    // the arguments of functions with a jump or a kink where the argument vanishes: steps, abs, sign (also max and min
    // of a linear difference are kinks, not handled)
    let steps = calls_of(h, f, &["unit_step", "heaviside_theta", "abs", "sign"], x);
    if deltas.is_empty() && steps.is_empty() {
        return Ok(None);
    }
    let mut ends = Vec::with_capacity(ps.len());
    for &p in ps {
        let Some(v) = real_of(cx, p)? else { return Ok(None) };
        ends.push(v);
    }
    let (first, last) = (ends[0], ends[ends.len() - 1]);
    let (lo, hi) = (first.min(last), first.max(last));
    let sign = if first <= last { 1.0 } else { -1.0 };
    if !deltas.is_empty() {
        let mut spec = vec![x];
        spec.extend_from_slice(ps);
        let range = cx.list(&spec);
        let dirac = h.sym_term(h.sym("dirac_delta"));
        if let View::App { head, args } = h.view(f) {
            // a sum: term by term
            if head == h.sym_term(h.sym(names::ADD)) {
                let mut parts = Vec::with_capacity(args.len());
                for &a in args {
                    let v = n_integrate(cx, a, &[range], opt)?;
                    if !numeric(h, v) {
                        return Ok(Some(v));
                    }
                    parts.push(v);
                }
                return Ok(Some(total(cx, &parts)?));
            }
            // a product with one delta factor
            if head == h.sym_term(h.sym(names::MUL)) {
                let (dl, rest): (Vec<T<'h>>, Vec<T<'h>>) =
                    args.iter().partition(|&&a| matches!(h.view(a), View::App { head, .. } if head == dirac));
                if let [d] = dl.as_slice() {
                    if let View::App { args: &[u], .. } = h.view(*d) {
                        if let Some((r, c1)) = linear_root(cx, u, x)? {
                            let g = if rest.is_empty() { h.int(1) } else { cx.eval(h.call(h.sym(names::MUL), &rest))? };
                            if !mentions(h, g, &[dirac]) {
                                if r < lo || r > hi || (r == lo && lo.is_finite()) || (r == hi && hi.is_finite()) {
                                    return Ok(Some(h.real(0.0)));
                                }
                                let rt = h.real(r);
                                let val = cx.eval(h.call(h.sym("rewrite"), &[g, h.call(h.sym(names::RULE), &[x, rt])]))?;
                                let val = cx.eval(h.call(h.sym("n"), &[val]))?;
                                let w = cx.eval(h.call(h.sym(names::MUL), &[val, h.real(sign / c1.abs())]))?;
                                return Ok(Some(w));
                            }
                        }
                    }
                }
            }
            // dirac_delta(u) alone
            if head == dirac {
                if let [u] = args {
                    if let Some((r, c1)) = linear_root(cx, *u, x)? {
                        let inside = r > lo && r < hi;
                        return Ok(Some(h.real(if inside { sign / c1.abs() } else { 0.0 })));
                    }
                }
            }
        }
        // not integrated: say so (the rules see 0 almost everywhere)
        cx.eval(h.call(
            h.sym("message"),
            &[
                h.sym_term(h.sym("n_integrate")),
                h.string("dirac_delta of a non-linear argument, or in a product with other generalized functions, is not integrated numerically; the value is not reliable"),
            ],
        ))?;
        return Ok(None);
    }
    // break points at the roots of step arguments
    let mut new: Vec<(f64, T<'h>)> = vec![];
    for u in steps {
        if let Some((r, _)) = linear_root(cx, u, x)? {
            let near = |p: f64| p.is_finite() && (p - r).abs() <= 1e-12 * p.abs().max(1.0);
            if r > lo && r < hi && !ends.iter().any(|&p| near(p)) && !new.iter().any(|&(p, _)| near(p)) {
                let t = root_term(cx, u, x, r)?;
                new.push((r, t));
            }
        }
    }
    if new.is_empty() {
        return Ok(None);
    }
    // the points in the order of the range (increasing or decreasing)
    let mut all: Vec<(f64, T<'h>)> = ends.iter().copied().zip(ps.iter().copied()).collect();
    all.extend(new);
    all.sort_by(|a, b| a.0.total_cmp(&b.0));
    if sign < 0.0 {
        all.reverse();
    }
    let mut spec = vec![x];
    spec.extend(all.iter().map(|p| p.1));
    let v = n_integrate(cx, f, &[cx.list(&spec)], opt)?;
    Ok(numeric(h, v).then_some(v))
}
