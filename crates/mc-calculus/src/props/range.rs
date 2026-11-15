//! `function_range(f, x, y)` and `function_range([f, cond], x, y)` (decision F27.1): the values of a real
//! function of one variable as a condition on `y`, with open and closed ends told apart.
//!
//! - **Rational functions** (parameters allowed): `resolve(exists(x, cond and den != 0 and num == y den))`
//!   by quantifier elimination (Collins; the `mc-cad` decomposition), reduced in `y`: `1/x` gives `y != 0`,
//!   `1/(1 + x^2)` gives `0 < y <= 1`, `x^2 + a` gives `y >= a`.
//! - **Other continuous functions** (no parameters): the domain (`cond` with the definedness conditions of
//!   [`super::conditions`]) is reduced to intervals; on each interval the extreme values are among the values
//!   at the critical points (`solve(f' == 0)`), the zeros of the arguments of `abs` (kinks), the closed finite
//!   ends, and the limits at the open and infinite ends. A family of critical points (`c(1)`) is read only on the
//!   whole line and when f is proved constant along it (f at the member c(1) minus f at the member 0 simplifies to
//!   0 for integer c(1)); a limit at an infinity that does not exist is passed over only when f is proved periodic
//!   (f(x + step) - f(x) simplifies to 0). Membership of a candidate in an interval and the comparison of two
//!   candidate values are decided exactly (the proved sign of the difference); undecided, the call stays. A bound
//!   is closed only when a point of the domain takes the value; a limit that is not taken (an infinity, an open
//!   end, `exp` at `-inf`) is an open bound (`y > 0` for `exp`). The ranges of the intervals are united and reduced.
//! - **Cross-check**: the function is sampled on a wide grid; a sample outside the answer, or a function with
//!   jumps (`floor`, `sign`, ...), gives no answer rather than a wrong one (samples only ever remove an answer).
use super::proof;
use super::sets::{is_head, poly_degree};
use super::spec;
use crate::term::Build;
use mc_api::{names, Store, View, T};
use mc_eval::{NativeCx, Res};

/// One end value: the exact term and whether a point of the domain takes it.
#[derive(Clone, Copy)]
struct Ext<'h> {
    v: T<'h>,
    taken: bool,
}

/// `function_range` for the arguments of the call.
pub fn range<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, args: &[T<'h>]) -> Res<Option<T<'h>>> {
    let h = b.heap();
    let (fc, x, y, complex) = match args {
        [fc, x, y] => (fc, x, y, false),
        [fc, x, y, d] => match h.view(*d) {
            View::Sym(s) if h.sym_name(s) == "complexes" => (fc, x, y, true),
            View::Sym(s) if h.sym_name(s) == "reals" => (fc, x, y, false),
            _ => return Ok(None),
        },
        _ => return Ok(None),
    };
    if !matches!(h.view(*x), View::Sym(_)) || !matches!(h.view(*y), View::Sym(_)) {
        return Ok(None);
    }
    let (f, cond) = match cx.as_list(*fc) {
        Some([f, k]) => (*f, Some(*k)),
        Some(_) => return Ok(None),
        None => (*fc, None),
    };
    let (f, x, y) = (f, *x, *y);
    if complex {
        return match cond {
            None => complex_range(cx, b, f, x, y),
            Some(_) => Ok(None),
        };
    }
    let r = match qe(cx, b, f, cond, x, y)? {
        Some(r) => Some(r),
        None => match harmonic(cx, b, f, cond, x, y)? {
            Some(r) => Some(r),
            None => intervals(cx, b, f, cond, x, y)?,
        },
    };
    if let Some(r) = r {
        if checks_out(cx, b, f, cond, x, y, r)? {
            return Ok(Some(r));
        }
        return Ok(None);
    }
    // a special function of the variable: its range from the table, checked against samples
    if cond.is_none() {
        if let Some(r) = table_range(cx, b, f, x, y, false)? {
            let mut r = r;
            if !cx.is_true(r) && !cx.is_false(r) && spec::params(b, &[r], &[y]).is_empty() {
                let rr = cx.eval(b.call("reduce", &[r, y, h.sym_term(h.sym("reals"))]))?;
                if !is_head(b, rr, "reduce") && !is_head(b, rr, names::FAILURE) {
                    r = rr;
                }
            }
            let r = super::sets::tidy_cond_split(cx, b, r)?;
            if checks_out_with(cx, b, f, cond, x, y, r, true)? {
                return Ok(Some(r));
            }
            return Ok(None);
        }
    }
    Ok(None)
}

/// `f = c g + d` with `c`, `d` free of `x` and `c` a nonzero real number: `(g, c, d)`; `(f, 1, 0)`
/// otherwise.
fn outer_affine<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, x: T<'h>) -> Res<(T<'h>, T<'h>, T<'h>)> {
    let h = b.heap();
    let mut g = f;
    let mut d = b.zero();
    if let View::App { head, args } = h.view(g) {
        if b.sym_of(head) == Some(b.hd.add) {
            let (free, dep): (Vec<T<'h>>, Vec<T<'h>>) = args.iter().partition(|&&a| is_free(b, a, x));
            if dep.len() == 1 && !free.is_empty() {
                g = dep[0];
                d = cx.eval(b.add(&free))?;
            }
        }
    }
    let mut c = b.one();
    if let View::App { head, args } = h.view(g) {
        if b.sym_of(head) == Some(b.hd.mul) {
            let (free, dep): (Vec<T<'h>>, Vec<T<'h>>) = args.iter().partition(|&&a| is_free(b, a, x));
            if dep.len() == 1 && !free.is_empty() {
                let k = cx.eval(b.mul(&free))?;
                let n = cx.eval(b.call("n", &[k]))?;
                let ok = match h.view(n) {
                    View::Real(v) => v != 0.0,
                    View::Int(v) => v != 0,
                    _ => false,
                };
                if ok {
                    g = dep[0];
                    c = k;
                }
            }
        }
    }
    Ok((g, c, d))
}

/// Is `u` affine in `x` with a nonzero real slope (a bijection of the line and of the plane)?
pub fn affine_in<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, u: T<'h>, x: T<'h>) -> Res<bool> {
    if u == x {
        return Ok(true);
    }
    if poly_degree(b, u, &[x]) != Some(1) {
        return Ok(false);
    }
    let s = cx.eval(b.call("d", &[u, x]))?;
    let n = cx.eval(b.call("n", &[s]))?;
    let h = b.heap();
    Ok(spec::params(b, &[u], &[x]).is_empty() && matches!(h.view(n), View::Real(v) if v != 0.0) || matches!(h.view(n), View::Int(v) if v != 0))
}

/// The range of a function of the table applied to (an affine function of) `x`, with an affine outer map; over the
/// reals or (`complex`) the plane. `None` when not known.
pub fn table_range<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, x: T<'h>, y: T<'h>, complex: bool) -> Res<Option<T<'h>>> {
    let (g, c, d) = outer_affine(cx, b, f, x)?;
    let Some(hit) = super::table::lookup(cx, b, g, &[x], &[])? else { return Ok(None) };
    if !affine_in(cx, b, hit.arg, x)? {
        return Ok(None);
    }
    // the other arguments free of x
    let h = b.heap();
    if let View::App { args, .. } = h.view(g) {
        if args.iter().filter(|&&a| a != hit.arg).any(|&a| !is_free(b, a, x)) {
            return Ok(None);
        }
    }
    let yy = cx.eval(b.div(b.sub(y, d), c))?;
    if complex {
        hit.crange(cx, b, yy)
    } else {
        hit.range(cx, b, yy)
    }
}

/// Is `f` (an affine image of a function of the table applied to an affine function of `x`) bounded above or
/// below on the reals by the table? Such a function is not onto the reals.
pub fn table_bounded<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, x: T<'h>) -> Res<bool> {
    Ok(table_call(cx, b, f, x, &[])?.is_some_and(|hit| hit.bound() != super::table::Bound::No))
}

/// The call of the table in `f` (an affine image of a function applied to an affine function of `x`, its other
/// arguments free of `x`) looked up under the parameter facts.
pub fn table_call<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, x: T<'h>, facts: &[T<'h>]) -> Res<Option<super::table::Hit<'h>>> {
    let (g, _, _) = outer_affine(cx, b, f, x)?;
    let Some(hit) = super::table::lookup(cx, b, g, &[x], facts)? else { return Ok(None) };
    if !affine_in(cx, b, hit.arg, x)? {
        return Ok(None);
    }
    let h = b.heap();
    if let View::App { args, .. } = h.view(g) {
        if args.iter().filter(|&&a| a != hit.arg).any(|&a| !is_free(b, a, x)) {
            return Ok(None);
        }
    }
    Ok(Some(hit))
}

/// The range in the complex plane (no condition): a constant takes its value; a nonconstant polynomial every value
/// (the fundamental theorem of algebra); the principal square root the right half plane with the nonnegative
/// imaginary axis, `e^u` every nonzero value (DLMF 4.2.(i), 4.2.(iii)); functions of the table their recorded
/// ranges. Samples of the function at complex points are checked against the answer.
fn complex_range<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, x: T<'h>, y: T<'h>) -> Res<Option<T<'h>>> {
    let h = b.heap();
    if is_free(b, f, x) {
        return Ok(Some(cx.eval(b.call("equal", &[y, f]))?));
    }
    let r = if poly_degree(b, f, &[x]).is_some_and(|d| d >= 1) {
        let cl = cx.eval(b.call("coefficient_list", &[f, x]))?;
        let Some(cs) = cx.as_list(cl).map(<[T<'h>]>::to_vec) else { return Ok(None) };
        let nz: Vec<T<'h>> = cs.iter().skip(1).map(|&k| b.call("unequal", &[k, b.zero()])).collect();
        let c = cx.eval(b.call("or", &nz))?;
        if !cx.is_true(c) {
            return Ok(None);
        }
        cx.bool(true)
    } else {
        let (g, c, d) = outer_affine(cx, b, f, x)?;
        let yy = cx.eval(b.div(b.sub(y, d), c))?;
        let pow = match h.view(g) {
            View::App { head, args: &[base, ex] } if b.sym_of(head) == Some(b.hd.pow) => Some((base, ex)),
            _ => None,
        };
        match pow {
            Some((base, ex)) if ex == b.rat(1, 2) && affine_in(cx, b, base, x)? => {
                let (re, im) = (b.call("re", &[yy]), b.call("im_part", &[yy]));
                cx.eval(b.call("or", &[b.call("and", &[b.call("equal", &[re, b.zero()]), b.call("greater_equal", &[im, b.zero()])]), b.call("greater", &[re, b.zero()])]))?
            }
            Some((base, ex)) if base == h.sym_term(b.hd.e) && affine_in(cx, b, ex, x)? => cx.eval(b.call("unequal", &[yy, b.zero()]))?,
            _ => match table_range(cx, b, f, x, y, true)? {
                Some(r) => r,
                None => return Ok(None),
            },
        }
    };
    // samples: the value at a few complex points satisfies the answer
    if !spec::params(b, &[f, r], &[x, y]).is_empty() {
        return Ok(Some(r));
    }
    for (re, im) in [(0.37, 0.21), (-1.3, 0.8), (2.9, -1.7), (-0.6, -2.3), (0.5, 0.0), (-2.0, 0.0)] {
        cx.tick(1)?;
        let z = cx.eval(b.add(&[h.real(re), b.mul(&[h.real(im), h.sym_term(h.sym("im"))])]))?;
        let v = cx.eval(h.call(h.sym("rewrite"), &[f, h.call(h.sym(names::RULE), &[x, z])]))?;
        let v = cx.eval(b.call("n", &[v]))?;
        if !matches!(h.view(v), View::Real(_) | View::Int(_)) && b.ar.num(v).is_none() {
            continue;
        }
        let t = cx.eval(h.call(h.sym("rewrite"), &[r, h.call(h.sym(names::RULE), &[y, v])]))?;
        if cx.is_false(t) {
            return Ok(None);
        }
    }
    Ok(Some(r))
}

/// Rational functions by quantifier elimination.
fn qe<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, cond: Option<T<'h>>, x: T<'h>, y: T<'h>) -> Res<Option<T<'h>>> {
    let h = b.heap();
    let ps = spec::params(b, &[f], &[x]);
    let mut syms = vec![x];
    syms.extend(ps.iter().copied());
    let t = cx.eval(b.call("together", &[f]))?;
    let num = cx.eval(b.call("numerator", &[t]))?;
    let den = cx.eval(b.call("denominator", &[t]))?;
    if poly_degree(b, num, &syms).is_none() || poly_degree(b, den, &syms).is_none() {
        return Ok(None);
    }
    let eq = b.call("equal", &[num, b.mul(&[y, den])]);
    let mut body = vec![eq];
    if den != b.one() {
        body.push(b.call("unequal", &[den, b.zero()]));
    }
    if let Some(k) = cond {
        body.push(k);
    }
    let e = b.call("exists", &[cx.list(&[x]), b.call("and", &body)]);
    let r = cx.eval(b.call("resolve", &[e, h.sym_term(h.sym("reals"))]))?;
    if is_head(b, r, "resolve") || is_head(b, r, names::FAILURE) || is_head(b, r, "exists") || is_head(b, r, "for_all") {
        return Ok(None);
    }
    if !is_free(b, r, x) {
        return Ok(None);
    }
    if !cx.is_true(r) && !cx.is_false(r) {
        let rr = cx.eval(b.call("reduce", &[r, y, h.sym_term(h.sym("reals"))]))?;
        if !is_head(b, rr, "reduce") && !is_head(b, rr, names::FAILURE) {
            return Ok(Some(super::sets::tidy_cond(cx, b, rr)?));
        }
    }
    Ok(Some(super::sets::tidy_cond(cx, b, r)?))
}

fn is_free<'h>(b: &Build<'h>, t: T<'h>, x: T<'h>) -> bool {
    crate::common::is_free(b, t, x)
}

/// `c0 + A sin(w) + B cos(w)` for one linear `w` over the whole line: `c0 - R <= y <= c0 + R` with
/// `R = sqrt(A^2 + B^2)` (the amplitude of a harmonic; `A`, `B`, `c0` free of `x`).
fn harmonic<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, cond: Option<T<'h>>, x: T<'h>, y: T<'h>) -> Res<Option<T<'h>>> {
    let h = b.heap();
    if cond.is_some() {
        return Ok(None);
    }
    let terms: Vec<T<'h>> = match h.view(f) {
        View::App { head, args } if b.sym_of(head) == Some(b.hd.add) => args.to_vec(),
        _ => vec![f],
    };
    let (mut c0, mut a, mut bb) = (vec![], vec![], vec![]);
    let mut arg: Option<T<'h>> = None;
    for t in terms {
        if is_free(b, t, x) {
            c0.push(t);
            continue;
        }
        let factors: Vec<T<'h>> = match h.view(t) {
            View::App { head, args } if b.sym_of(head) == Some(b.hd.mul) => args.to_vec(),
            _ => vec![t],
        };
        let (coef, trig): (Vec<T<'h>>, Vec<T<'h>>) = factors.into_iter().partition(|&u| is_free(b, u, x));
        let [tr] = trig.as_slice() else { return Ok(None) };
        let View::App { head, args: &[w] } = h.view(*tr) else { return Ok(None) };
        let Some(sn) = b.sym_of(head).map(|s| h.sym_name(s).to_string()) else { return Ok(None) };
        if sn != "sin" && sn != "cos" {
            return Ok(None);
        }
        // w linear in x
        let dw = cx.eval(b.call("d", &[w, x]))?;
        if !is_free(b, dw, x) || dw == b.zero() {
            return Ok(None);
        }
        match arg {
            None => arg = Some(w),
            Some(w0) if w0 == w => {}
            _ => return Ok(None),
        }
        let c = b.mul(&coef);
        if sn == "sin" {
            a.push(c);
        } else {
            bb.push(c);
        }
    }
    if arg.is_none() {
        return Ok(None);
    }
    let (sa, sb) = (b.add(&a), b.add(&bb));
    let radius = cx.eval(b.call("sqrt", &[b.add(&[b.powi(sa, 2), b.powi(sb, 2)])]))?;
    let base = cx.eval(b.add(&c0))?;
    let lo = cx.eval(b.sub(base, radius))?;
    let hi = cx.eval(b.add(&[base, radius]))?;
    let r = cx.eval(b.call("and", &[b.call("less_equal", &[lo, y]), b.call("less_equal", &[y, hi])]))?;
    Ok(Some(r))
}

/// An interval of the domain: ends with closedness, `None` for an infinite end.
pub(super) struct Piece<'h> {
    pub(super) lo: Option<(T<'h>, bool)>,
    pub(super) hi: Option<(T<'h>, bool)>,
}

/// The intervals of a reduced condition on `x` (`None` when its shape is not read).
pub(super) fn pieces<'h>(b: &Build<'h>, r: T<'h>, x: T<'h>) -> Option<Vec<Piece<'h>>> {
    let h = b.heap();
    let View::App { head, args } = h.view(r) else { return None };
    let name = h.sym_name(b.sym_of(head)?).to_string();
    match name.as_str() {
        "or" => {
            let mut out = vec![];
            for &a in args {
                out.extend(pieces(b, a, x)?);
            }
            Some(out)
        }
        "and" => {
            let mut p = Piece { lo: None, hi: None };
            for &a in args {
                let q = pieces(b, a, x)?;
                let [q] = q.as_slice() else { return None };
                if q.lo.is_some() {
                    if p.lo.is_some() {
                        return None;
                    }
                    p.lo = q.lo;
                }
                if q.hi.is_some() {
                    if p.hi.is_some() {
                        return None;
                    }
                    p.hi = q.hi;
                }
            }
            Some(vec![p])
        }
        "less" | "less_equal" | "greater" | "greater_equal" | "equal" if args.len() >= 2 => {
            // chains `a < x < b` and single relations `x op a`
            let closed = name.ends_with("equal");
            let mut p = Piece { lo: None, hi: None };
            let incr = matches!(name.as_str(), "less" | "less_equal");
            if name == "equal" {
                let [l, rr] = args else { return None };
                let v = if *l == x { *rr } else if *rr == x { *l } else { return None };
                return Some(vec![Piece { lo: Some((v, true)), hi: Some((v, true)) }]);
            }
            for w in args.windows(2) {
                let (small, big) = if incr { (w[0], w[1]) } else { (w[1], w[0]) };
                if big == x && small != x {
                    p.lo = Some((small, closed));
                } else if small == x && big != x {
                    p.hi = Some((big, closed));
                } else {
                    return None;
                }
            }
            Some(vec![p])
        }
        _ => None,
    }
}

/// The machine value of a closed real term.
fn num<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, t: T<'h>) -> Res<Option<f64>> {
    let h = b.heap();
    let v = cx.eval(b.call("n", &[t]))?;
    Ok(match h.view(v) {
        View::Real(x) if x.is_finite() => Some(x),
        View::Int(k) => Some(k as f64),
        _ => None,
    })
}

fn at<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, x: T<'h>, v: T<'h>) -> Res<T<'h>> {
    let h = b.heap();
    cx.eval(h.call(h.sym("rewrite"), &[f, h.call(h.sym(names::RULE), &[x, v])]))
}

/// The members of the solutions of `f' == 0` (of a family in `c(1)` the members at -4..4, the family itself in
/// `fams` with its parameter: the caller proves that the function is constant along it), or `None` when not read.
fn critical<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, g: T<'h>, x: T<'h>, out: &mut Vec<T<'h>>, fams: &mut Vec<(T<'h>, T<'h>)>) -> Res<Option<()>> {
    let h = b.heap();
    let reals = h.sym_term(h.sym("reals"));
    // a factor that never vanishes on the line (`sech(x)`, `exp(x)`) has no critical point
    let real_x = b.call("element", &[x, reals]);
    let ne = cx.eval_assuming(&[real_x], b.call("refine", &[b.call("unequal", &[g, b.zero()])]))?;
    if cx.is_true(ne) {
        return Ok(Some(()));
    }
    let eq = b.call("equal", &[g, b.zero()]);
    let s = cx.eval(b.call("solve", &[eq, x, reals]))?;
    let Some(items) = cx.as_list(s).map(<[T<'h>]>::to_vec) else {
        // the zeros of a product are those of its factors, the zeros of a positive power those of its base
        if let View::App { head, args } = h.view(g) {
            if b.sym_of(head) == Some(b.hd.mul) {
                for &a in args.iter().filter(|&&a| !is_free(b, a, x)) {
                    if critical(cx, b, a, x, out, fams)?.is_none() {
                        return Ok(None);
                    }
                }
                return Ok(Some(()));
            }
            if let (Some(p), [base, ex]) = (b.sym_of(head), args) {
                if p == b.hd.pow && is_free(b, *ex, x) {
                    let n = cx.eval(b.call("n", &[*ex]))?;
                    match h.view(n) {
                        View::Real(v) if v < 0.0 => return Ok(Some(())),
                        View::Int(v) if v < 0 => return Ok(Some(())),
                        View::Real(_) | View::Int(_) => return critical(cx, b, *base, x, out, fams),
                        _ => {}
                    }
                }
            }
        }
        return Ok(None);
    };
    let gen = h.sym("c");
    for it in items {
        let Some(&[r]) = cx.as_list(it) else { return Ok(None) };
        let View::App { args: &[_, v], .. } = h.view(r) else { return Ok(None) };
        match find_c(h, v, gen) {
            Some(k) => {
                if !matches!(h.view(k), View::App { args: [a], .. } if *a == h.int(1)) {
                    return Ok(None);
                }
                fams.push((v, k));
                for j in -4..=4i64 {
                    let m = at(cx, b, v, k, b.int(j))?;
                    if is_free(b, m, x) {
                        out.push(m);
                    }
                }
            }
            None if is_free(b, v, x) => out.push(v),
            None => return Ok(None),
        }
    }
    Ok(Some(()))
}

fn find_c<'h>(h: &'h mc_core::Heap, t: T<'h>, s: mc_core::Sym) -> Option<T<'h>> {
    match h.view(t) {
        View::App { head, args } => {
            if h.sym_term(s) == head {
                return Some(t);
            }
            args.iter().find_map(|&a| find_c(h, a, s))
        }
        _ => None,
    }
}

/// The arguments of the `abs` calls of `t` that depend on `x`.
fn abs_args<'h>(b: &Build<'h>, t: T<'h>, x: T<'h>, out: &mut Vec<T<'h>>) {
    let h = b.heap();
    if let View::App { head, args } = h.view(t) {
        if b.sym_of(head).is_some_and(|s| h.sym_name(s) == "abs") && !is_free(b, t, x) {
            out.extend(args.iter().copied().filter(|&a| !is_free(b, a, x)));
        }
        for &a in args {
            abs_args(b, a, x, out);
        }
    }
}

/// `t` with the calls `abs(u)` for `u` in `us` replaced by `u` or `-u` (bit `i` of `mask` set: `-u`).
fn drop_abs<'h>(b: &Build<'h>, t: T<'h>, us: &[T<'h>], mask: u32) -> T<'h> {
    let h = b.heap();
    let View::App { head, args } = h.view(t) else { return t };
    if b.sym_of(head).is_some_and(|s| h.sym_name(s) == "abs") {
        if let [u] = args {
            if let Some(i) = us.iter().position(|&w| w == *u) {
                let inner = drop_abs(b, *u, us, mask);
                return if mask & (1 << i) != 0 { b.neg(inner) } else { inner };
            }
        }
    }
    let new: Vec<T<'h>> = args.iter().map(|&a| drop_abs(b, a, us, mask)).collect();
    h.app(head, &new)
}

/// Does `t` contain a jump function (the range of a step function is not an interval)?
pub(super) fn has_jumps<'h>(b: &Build<'h>, t: T<'h>) -> bool {
    let h = b.heap();
    match h.view(t) {
        View::App { head, args } => {
            b.sym_of(head).is_some_and(|s| {
                matches!(h.sym_name(s).as_str(), "floor" | "ceiling" | "round" | "integer_part" | "sign" | "unit_step" | "mod" | "fractional_part" | "heaviside_theta" | "unit_box" | "piecewise" | "if" | "boole" | "real_sign" | "square_wave" | "sawtooth_wave" | "kronecker_delta" | "prime_pi")
            }) || args.iter().any(|&a| has_jumps(b, a))
        }
        _ => false,
    }
}

/// Smaller (`min`) or larger value of the current one and `new`, compared by the proved sign of their difference; a
/// tie (exactly equal) is taken when either is. `None` when the comparison is not decided.
fn better<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, cur: Option<Ext<'h>>, new: Ext<'h>, min: bool) -> Res<Option<Ext<'h>>> {
    let Some(c) = cur else { return Ok(Some(new)) };
    Ok(match super::proof::closed_sign(cx, b, b.sub(new.v, c.v))? {
        Some(0) => Some(Ext { taken: c.taken || new.taken, ..c }),
        Some(s) => Some(if (s < 0) == min { new } else { c }),
        None => None,
    })
}

/// Continuous functions of one variable (no parameters) by the extreme values on the intervals of the domain.
fn intervals<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, cond: Option<T<'h>>, x: T<'h>, y: T<'h>) -> Res<Option<T<'h>>> {
    let h = b.heap();
    if !spec::params(b, &[f], &[x]).is_empty() || has_jumps(b, f) {
        return Ok(None);
    }
    // the domain as intervals
    let reals = h.sym_term(h.sym("reals"));
    let Some(defs) = super::conditions(cx, b, f, &[x], false)? else { return Ok(None) };
    let mut all = defs;
    if let Some(k) = cond {
        all.push(k);
    }
    let dom = match all.as_slice() {
        [] => cx.bool(true),
        [c] => *c,
        _ => cx.eval(b.call("and", &all))?,
    };
    let ps: Vec<Piece<'h>> = if cx.is_true(dom) {
        vec![Piece { lo: None, hi: None }]
    } else {
        let r = cx.eval(b.call("reduce", &[dom, x, reals]))?;
        if is_head(b, r, "reduce") || is_head(b, r, names::FAILURE) || cx.is_false(r) {
            return Ok(None);
        }
        let Some(p) = pieces(b, r, x) else { return Ok(None) };
        p
    };
    // the candidates: critical points and kinks
    let mut cands = vec![];
    let mut fams = vec![];
    let mut kinks = vec![];
    abs_args(b, f, x, &mut kinks);
    kinks.dedup();
    if kinks.len() > 2 {
        return Ok(None);
    }
    // each sign pattern of the `abs` arguments gives a smooth function whose critical points are candidates
    for mask in 0..(1u32 << kinks.len()) {
        let g = if kinks.is_empty() { f } else { cx.eval(drop_abs(b, f, &kinks, mask))? };
        let dg = cx.eval(b.call("d", &[g, x]))?;
        if is_head(b, dg, "d") {
            return Ok(None);
        }
        if is_free(b, dg, x) {
            // a constant piece (its limits at the ends are its value, never taken there): a point inside each
            // interval of the domain takes the value
            if b.is_zero(dg) {
                for p in &ps {
                    let t = match (p.lo, p.hi) {
                        (Some((l, _)), Some((u, _))) => b.div(b.add(&[l, u]), b.int(2)),
                        (Some((l, _)), None) => b.add(&[l, b.one()]),
                        (None, Some((u, _))) => b.sub(u, b.one()),
                        (None, None) => b.zero(),
                    };
                    cands.push(cx.eval(t)?);
                }
            }
            continue;
        }
        if critical(cx, b, dg, x, &mut cands, &mut fams)?.is_none() {
            return Ok(None);
        }
    }
    for u in kinks {
        if critical(cx, b, u, x, &mut cands, &mut fams)?.is_none() {
            return Ok(None);
        }
    }
    // a family of critical points: its members -4..4 stand for all only on the whole line and when f is constant
    // along the family (proved: f at the member c(1) minus f at the member 0 simplifies to 0 for integer c(1))
    let whole = matches!(ps.as_slice(), [p] if p.lo.is_none() && p.hi.is_none());
    let mut periodic = false;
    if !fams.is_empty() {
        if !whole {
            return Ok(None);
        }
        for &(v, k) in &fams {
            let fv = at(cx, b, f, x, v)?;
            let f0 = at(cx, b, fv, k, b.zero())?;
            let ints = b.call("element", &[k, h.sym_term(h.sym("integers"))]);
            let d = cx.eval_assuming(&[ints], b.call("simplify", &[b.sub(fv, f0)]))?;
            if d != b.zero() {
                return Ok(None);
            }
        }
        // f periodic with the step of the first family: its extremes are attained at the critical points
        let (v, k) = fams[0];
        let (m1, m0) = (at(cx, b, v, k, b.one())?, at(cx, b, v, k, b.zero())?);
        let step = cx.eval(b.sub(m1, m0))?;
        let shifted = at(cx, b, f, x, b.add(&[x, step]))?;
        let d = cx.eval(b.call("simplify", &[b.sub(shifted, f)]))?;
        periodic = d == b.zero() && step != b.zero();
    }
    let inf = h.sym_term(h.sym("inf"));
    let mut conds = vec![];
    for p in &ps {
        cx.tick(1)?;
        let mut lo: Option<Ext<'h>> = None;
        let mut hi: Option<Ext<'h>> = None;
        let (mut lo_inf, mut hi_inf) = (false, false);
        let mut points: Vec<T<'h>> = cands.clone();
        // finite closed ends
        for e in [p.lo, p.hi].into_iter().flatten() {
            if e.1 {
                points.push(e.0);
            }
        }
        for t in points {
            // inside the piece, decided exactly (the sign of the distance to each end, proved); undecided: no answer
            let inside = |cx: &mut NativeCx<'_, '_, 'h>, v: T<'h>| -> Res<Option<bool>> {
                if num(cx, b, v)?.is_none() {
                    return Ok(Some(false));
                }
                for (end, lower) in [(p.lo, true), (p.hi, false)] {
                    let Some((e, closed)) = end else { continue };
                    let d = if lower { b.sub(v, e) } else { b.sub(e, v) };
                    match proof::closed_sign(cx, b, d)? {
                        Some(1) => {}
                        Some(0) if closed => {}
                        Some(_) => return Ok(Some(false)),
                        None => return Ok(None),
                    }
                }
                Ok(Some(true))
            };
            match inside(cx, t)? {
                Some(true) => {}
                Some(false) => continue,
                None => return Ok(None),
            }
            let v = at(cx, b, f, x, t)?;
            if num(cx, b, v)?.is_none() {
                continue;
            }
            let v = cx.eval(b.call("simplify", &[v]))?;
            let e = Ext { v, taken: true };
            let (Some(l), Some(u)) = (better(cx, b, lo, e, true)?, better(cx, b, hi, e, false)?) else { return Ok(None) };
            lo = Some(l);
            hi = Some(u);
        }
        // limits at the ends not taken (open, infinite, or a pole at a closed end)
        for (end, is_lo) in [(p.lo, true), (p.hi, false)] {
            let (pt, closed) = match end {
                Some((t, c)) => (t, c),
                None => (if is_lo { b.neg(inf) } else { inf }, false),
            };
            let dir = h.call(h.sym("options"), &[h.call(h.sym(names::RULE), &[h.sym_term(h.sym("direction")), h.sym_term(h.sym(if is_lo { "from_above" } else { "from_below" }))])]);
            let l = cx.eval(b.call("limit", &[f, h.call(h.sym(names::RULE), &[x, pt]), dir]))?;
            if is_head(b, l, "limit") || is_head(b, l, names::FAILURE) {
                // an oscillating function at an infinity: a periodic one (proved) has its extremes at the critical points
                if periodic && end.is_none() {
                    continue;
                }
                return Ok(None);
            }
            if l == inf {
                hi_inf = true;
                continue;
            }
            if l == b.neg(inf) {
                lo_inf = true;
                continue;
            }
            if num(cx, b, l)?.is_none() {
                return Ok(None);
            }
            let e = Ext { v: l, taken: false };
            // a closed finite end takes its value (when finite there)
            let _ = closed;
            let (Some(a), Some(u)) = (better(cx, b, lo, e, true)?, better(cx, b, hi, e, false)?) else { return Ok(None) };
            lo = Some(a);
            hi = Some(u);
        }
        let mut parts = vec![];
        if !lo_inf {
            let Some(l) = lo else { return Ok(None) };
            parts.push(b.call(if l.taken { "greater_equal" } else { "greater" }, &[y, l.v]));
        }
        if !hi_inf {
            let Some(u) = hi else { return Ok(None) };
            parts.push(b.call(if u.taken { "less_equal" } else { "less" }, &[y, u.v]));
        }
        conds.push(match parts.as_slice() {
            [] => cx.bool(true),
            [a] => *a,
            _ => b.call("and", &parts),
        });
    }
    let r = match conds.as_slice() {
        [c] => *c,
        _ => b.call("or", &conds),
    };
    let r = cx.eval(r)?;
    if cx.is_true(r) {
        return Ok(Some(r));
    }
    let rr = cx.eval(b.call("reduce", &[r, y, reals]))?;
    if !is_head(b, rr, "reduce") && !is_head(b, rr, names::FAILURE) {
        return Ok(Some(super::sets::tidy_cond(cx, b, rr)?));
    }
    Ok(Some(super::sets::tidy_cond(cx, b, r)?))
}

/// The cross-check: sampled values of `f` on the domain satisfy the answer `r`, and the answer is not much
/// larger than the sampled range for a bound that was found (a bound far from every sample is suspect only
/// when the samples go beyond it).
fn checks_out<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, cond: Option<T<'h>>, x: T<'h>, y: T<'h>, r: T<'h>) -> Res<bool> {
    checks_out_with(cx, b, f, cond, x, y, r, false)
}

/// [`checks_out`]; `light`: fewer samples (a range read from the table, recorded from the literature).
#[allow(clippy::too_many_arguments)]
fn checks_out_with<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, cond: Option<T<'h>>, x: T<'h>, y: T<'h>, r: T<'h>, light: bool) -> Res<bool> {
    let h = b.heap();
    if !spec::params(b, &[f, r], &[x, y]).is_empty() || cx.is_true(r) {
        return Ok(true);
    }
    let step = if light { 8 } else { 1 };
    let mut xs: Vec<f64> = (-120..=120).step_by(step).map(|k| f64::from(k) * 0.25 + 0.0123).collect();
    // a light check stays within 10^-3..10^3 (some functions evaluate slowly far out)
    let (kmax, kstep) = if light { (12, 2) } else { (24, 1) };
    for k in (-kmax..=kmax).step_by(kstep) {
        let p = 10f64.powf(f64::from(k) / 4.0);
        xs.push(p);
        xs.push(-p);
    }
    for v in xs {
        cx.tick(1)?;
        if let Some(k) = cond {
            let ok = cx.eval(h.call(h.sym("rewrite"), &[k, h.call(h.sym(names::RULE), &[x, h.real(v)])]))?;
            if !cx.is_true(ok) {
                continue;
            }
        }
        let Some(fv) = super::value_at(cx, b, f, x, v)? else { continue };
        // the answer at the value, with a relative slack for rounding at an end
        for s in [0.0, 1e-9 * (1.0 + fv.abs()), -1e-9 * (1.0 + fv.abs())] {
            let t = cx.eval(h.call(h.sym("rewrite"), &[r, h.call(h.sym(names::RULE), &[y, h.real(fv + s)])]))?;
            if cx.is_true(t) {
                break;
            }
            if s < 0.0 {
                return Ok(false);
            }
        }
    }
    Ok(true)
}
