//! Certified root counts of functions that are not polynomials: `count_roots(f, [x, a, b])` on a real interval or
//! on a rectangle of the complex plane, and `count_roots(f, x)`, `count_roots(f, [x, a, inf])` for sums of real
//! powers of x. Roots are counted with multiplicity.
//!
//! Every count is proved from enclosures (ball arithmetic, `mc_numeric::cball`: an enclosure holds the function's
//! values on the whole box), so no count depends on a tolerance or on sampling; where the proof is not reached the
//! call stays unevaluated. Only the elementary functions are used (their box enclosures are rigorous by
//! construction); an expression with another function is left alone.
//!
//! - **Interval** (the exclusion and monotonicity tests of interval analysis: Moore, Kearfott and Cloud,
//!   *Introduction to Interval Analysis*, SIAM 2009, ch. 8; Kearfott, *Rigorous Global Search*, Kluwer 1996,
//!   ch. 5): a subinterval on which the enclosure of f excludes 0 has no root; one on which the enclosure of f'
//!   excludes 0 has exactly one root when f has opposite signs at its ends and none otherwise; any other is bisected
//!   at a point where f is proved nonzero. A root at an exact end of multiplicity k (f(a) = ... = f^(k-1)(a) = 0
//!   exactly and f^(k) nonzero on a one-sided neighbourhood of a, so that f = f^(k)(xi) (x - a)^k/k! there) counts
//!   k. A multiple root inside the interval is not proved: the call stays.
//! - **Rectangle** (the argument principle, Henrici, *Applied and Computational Complex Analysis* I, Wiley 1974,
//!   section 4.10; rigorous winding numbers as in Ying and Katz, J. Comput. Phys. 76 (1988), and Johnson and Tucker,
//!   *Enclosing all zeros of an analytic function*, 2009): f must be analytic on the closed rectangle, which is
//!   proved by covering it with boxes on which f has an enclosure (a pole or a branch cut touching a box leaves it
//!   without one). The boundary is cut into pieces on which the enclosure of f lies in an open half-plane; there the
//!   change of arg f is the principal angle between the values at the ends, and the changes add up to 2 pi times
//!   the number of zeros inside. A zero on (or too near) the boundary leaves the call unevaluated.
//! - **Sums of real powers** `sum c_i x^(a_i)` on (0, inf) (real exponents a_0 < ... < a_n): for 0 < x <= eps the
//!   term of least exponent outweighs the others, |c_0| > sum_(i>0) |c_i| eps^(a_1 - a_0), and for x >= R >= 1 the
//!   term of largest exponent, |c_n| > sum_(i<n) |c_i| R^(a_(n-1) - a_n); the interval method counts on [eps, R].
//!   The count is checked against the rule of signs for real exponents (Laguerre 1883; Jameson, Math. Gazette 90
//!   (2006)): at most the number of sign changes of the coefficients, with the same parity. A polynomial on the
//!   whole line adds the count of f(-x) on (0, inf) and the order of its root at 0.
use crate::cx::{Env, Out};
use mc_api::{names, Store, View, T};
use mc_arith::num::{Ball, Float, Mag};
use mc_arith::number::Real;
use mc_arith::Num;
use mc_core::Heap;
use mc_eval::{NativeCx, Reply, Res};
use mc_numeric::cball::CBall;
use mc_numeric::numeval::{Graph, Island, Names};
use mc_poly::work::{Meter, WorkResult};
use std::cell::Cell;
use std::cmp::Ordering;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// The highest derivative used for a root at an end.
const MAX_ORDER: usize = 5;
/// The library functions a count may use.
const COUNT_LIBRARY: &[&str] = &["gamma", "digamma", "polygamma", "bessel_j", "bessel_i", "airy_ai", "airy_bi"];
/// The most boxes one count may examine.
const MAX_BOXES: u64 = 400_000;
/// The working precisions tried in turn (bits).
const PRECISIONS: [u64; 3] = [64, 128, 256];

/// An end of the counting range, read from the call.
#[derive(Clone)]
enum EndT<'h> {
    PosInf,
    NegInf,
    Num(T<'h>),
}

/// `count_roots(f, x)` and `count_roots(f, [x, a, b])` for a function that is not a polynomial (or one the exact
/// method declined); `Reply::Decline` when no count is proved.
pub(crate) fn count_roots<'h>(cx: &mut NativeCx<'_, '_, 'h>, stop: &Arc<AtomicBool>, name: &'static str, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h: &'h Heap = cx.heap();
    let env = Env::new(h, stop.clone(), name);
    let [f, spec] = args else { return Ok(Reply::Decline) };
    let (x, range) = match env.as_list(*spec) {
        Some(&[x, a, b]) => (x, Some((a, b))),
        Some(_) => return Ok(Reply::Decline),
        None => (*spec, None),
    };
    if !matches!(h.view(x), View::Sym(_)) || !env.contains(*f, x) {
        return Ok(Reply::Decline);
    }
    let read_end = |t: T<'h>| -> Option<EndT<'h>> {
        if t == env.sym("inf") {
            return Some(EndT::PosInf);
        }
        if let View::App { head, args: &[c, i] } = h.view(t) {
            if head == env.hd.mul && i == env.sym("inf") && env.ar.num(c).is_some_and(|n| n.as_rat().is_some_and(|r| r.num.neg)) {
                return Some(EndT::NegInf);
            }
        }
        (!env.contains(t, x)).then_some(EndT::Num(t))
    };
    let ends = match range {
        None => (EndT::NegInf, EndT::PosInf),
        Some((a, b)) => match (read_end(a), read_end(b)) {
            (Some(ea), Some(eb)) => (ea, eb),
            _ => return Ok(Reply::Decline),
        },
    };
    // a meromorphic f on a bounded range: its zeros are those of the numerator N of f = N/D, once N and D are proved
    // to have no common zero there
    let (f, den) = match &ends {
        (EndT::Num(a), EndT::Num(b)) => match clear_poles(cx, &env, *f, x, *a, *b)? {
            Some((n, d)) => (n, d),
            None => (*f, None),
        },
        _ => (*f, None),
    };
    // f and its first derivatives (the evaluator differentiates)
    let mut ds = vec![f];
    for _ in 0..MAX_ORDER {
        let last = *ds.last().unwrap_or(&f);
        ds.push(cx.eval(env.call("d", &[last, x]))?);
    }
    // a real interval is cut at the integers inside it where f is exactly 0 (a multiple root there counts by its
    // exact derivatives, at the end of each side, and once in the total)
    let mut pieces: Vec<(EndT<'h>, EndT<'h>)> = vec![ends.clone()];
    let mut shared = 0u64;
    if let (EndT::Num(a), EndT::Num(b)) = &ends {
        match interior_zeros(cx, &env, &ds, x, *a, *b)? {
            Some(cuts) if !cuts.is_empty() => {
                let mut pts = vec![*a];
                for (c, k) in cuts {
                    pts.push(c);
                    shared += k;
                }
                pts.push(*b);
                pieces = pts.windows(2).map(|w| (EndT::Num(w[0]), EndT::Num(w[1]))).collect();
            }
            Some(_) => {}
            None => return Ok(Reply::Decline),
        }
    }
    let mut zeros_of: Vec<Vec<Vec<bool>>> = vec![];
    for piece in &pieces {
        zeros_of.push(end_zeros(cx, &env, &ds, x, piece)?);
    }
    // one budget of boxes for all precisions: a count that runs out of boxes is not retried
    let bud = Cell::new(MAX_BOXES);
    let r = crate::natives::run(cx, stop, name, |env, m| {
        if let Some(d) = den {
            let mut proved = false;
            for &p in &PRECISIONS {
                if no_common_zero(env, x, f, d, &ends, Prec { p, bud: &bud }, m)? == Some(true) {
                    proved = true;
                    break;
                }
            }
            if !proved {
                return Ok(Out::Decline);
            }
        }
        let mut total = 0u64;
        for (piece, zeros) in pieces.iter().zip(&zeros_of) {
            let mut got = None;
            for &p in &PRECISIONS {
                if let Some(n) = count_at(env, x, &ds, piece, zeros, Prec { p, bud: &bud }, m)? {
                    got = Some(n);
                    break;
                }
            }
            let Some(n) = got else { return Ok(Out::Decline) };
            total += n;
        }
        // each cut point was counted at the end of both of its sides
        if total < shared {
            return Ok(Out::Decline);
        }
        Ok(Out::Done(env.int((total - shared) as i64)))
    })?;
    Ok(r)
}

/// Whether f is exactly 0 at the number a, and the exact zeros of f, f', ... there (as `count_at` reads them).
fn exact_zeros<'h>(cx: &mut NativeCx<'_, '_, 'h>, env: &Env<'h>, ds: &[T<'h>], x: T<'h>, a: T<'h>) -> Res<Vec<bool>> {
    let mut z = vec![];
    if nonzero_at(env, ds[0], x, a) {
        return Ok(z);
    }
    for &dj in ds {
        let v = cx.eval(env.call("rewrite", &[dj, env.call(names::RULE, &[x, a])]))?;
        z.push(matches!(env.ar.num(v), Some(Num::Real(Real::Exact(r))) if r.is_zero()));
        if !z.last().copied().unwrap_or(false) {
            break;
        }
    }
    Ok(z)
}

/// The exact zeros of f, f', ... at the finite ends of a piece.
fn end_zeros<'h>(cx: &mut NativeCx<'_, '_, 'h>, env: &Env<'h>, ds: &[T<'h>], x: T<'h>, piece: &(EndT<'h>, EndT<'h>)) -> Res<Vec<Vec<bool>>> {
    let mut zeros = vec![];
    for e in [&piece.0, &piece.1] {
        match e {
            EndT::Num(a) => zeros.push(exact_zeros(cx, env, ds, x, *a)?),
            _ => zeros.push(vec![]),
        }
    }
    Ok(zeros)
}

/// The integers strictly inside the real interval (a, b) where f is exactly 0, in increasing order, each with its
/// multiplicity (the number of leading exact zeros of f, f', ...); `Some(vec![])` for a complex range or none,
/// `None` when a multiplicity exceeds the derivatives at hand.
fn interior_zeros<'h>(cx: &mut NativeCx<'_, '_, 'h>, env: &Env<'h>, ds: &[T<'h>], x: T<'h>, a: T<'h>, b: T<'h>) -> Res<Option<Vec<(T<'h>, u64)>>> {
    let real = |cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>| -> Res<Option<f64>> {
        let v = cx.eval(env.call("n", &[t]))?;
        Ok(match env.ar.num(v) {
            Some(Num::Real(r)) => Some(r.to_f64()),
            _ => None,
        })
    };
    let (Some(fa), Some(fb)) = (real(cx, a)?, real(cx, b)?) else { return Ok(Some(vec![])) };
    let (lo, hi) = if fa < fb { (fa, fb) } else { (fb, fa) };
    if !(hi - lo).is_finite() || hi - lo > 64.0 {
        return Ok(Some(vec![]));
    }
    let mut out = vec![];
    let mut k = lo.ceil() as i64;
    while (k as f64) <= hi {
        let kt = env.int(k);
        // strictly inside, decided exactly
        let inside = cx.eval(env.call("less", &[env.call("min", &[a, b]), kt]))?;
        let inside2 = cx.eval(env.call("less", &[kt, env.call("max", &[a, b])]))?;
        if inside == env.sym("true") && inside2 == env.sym("true") {
            let z = exact_zeros(cx, env, ds, x, kt)?;
            let mult = z.iter().take_while(|&&b| b).count();
            if mult > 0 {
                if mult >= z.len() || mult > MAX_ORDER {
                    return Ok(None);
                }
                out.push((kt, mult as u64));
            }
        }
        k += 1;
    }
    Ok(Some(out))
}

/// f written as N/D with the poles of gamma and the circular functions in D: tan, cot, sec, csc as quotients of sin
/// and cos, and gamma(u) of an affine u with a real slope as gamma(u + m)/(u (u + 1) ... (u + m - 1)) with m the least
/// shift that puts re(u + m) >= 1/2 on the range (so gamma(u + m) has no pole there). `None` when D is free of x
/// (nothing to clear) or a gamma's argument has another form.
fn clear_poles<'h>(cx: &mut NativeCx<'_, '_, 'h>, env: &Env<'h>, f: T<'h>, x: T<'h>, a: T<'h>, b: T<'h>) -> Res<Option<(T<'h>, Option<T<'h>>)>> {
    let mut failed = false;
    // a long range holds many poles, and the boxes between them cost time to no purpose: f is left as it is there
    let width = cx.eval(env.call("n", &[env.call("abs", &[env.ar.add(&[b, env.ar.mul(&[env.int(-1), a])])])]))?;
    if !matches!(env.ar.num(width), Some(Num::Real(r)) if r.to_f64() <= 1000.0) {
        return Ok(None);
    }
    let g = rewrite_poles(cx, env, f, x, a, b, &mut failed)?;
    if failed {
        return Ok(None);
    }
    let t = cx.eval(env.call("together", &[g]))?;
    let n = cx.eval(env.call("numerator", &[t]))?;
    let d = cx.eval(env.call("denominator", &[t]))?;
    if !env.contains(d, x) {
        return Ok(Some((g, None)));
    }
    // factored, D has tight enclosures away from its zeros (the expanded polynomial loses them to cancellation)
    let fd = cx.eval(env.call("factor", &[d]))?;
    Ok(Some((n, Some(fd))))
}

fn rewrite_poles<'h>(cx: &mut NativeCx<'_, '_, 'h>, env: &Env<'h>, t: T<'h>, x: T<'h>, a: T<'h>, b: T<'h>, failed: &mut bool) -> Res<T<'h>> {
    let h = env.h();
    let View::App { head, args } = h.view(t) else { return Ok(t) };
    if !env.contains(t, x) {
        return Ok(t);
    }
    let mut na = Vec::with_capacity(args.len());
    for &u in args {
        na.push(rewrite_poles(cx, env, u, x, a, b, failed)?);
    }
    let name = match h.view(head) {
        View::Sym(s) => h.sym_name(s),
        _ => String::new(),
    };
    if let [u] = na.as_slice() {
        let u = *u;
        let sin = || env.call("sin", &[u]);
        let cos = || env.call("cos", &[u]);
        let one = env.int(1);
        let q = |n: T<'h>, d: T<'h>| env.ar.mul(&[n, env.ar.pow(d, env.int(-1))]);
        match name.as_str() {
            "tan" => return Ok(q(sin(), cos())),
            "cot" => return Ok(q(cos(), sin())),
            "sec" => return Ok(q(one, cos())),
            "csc" => return Ok(q(one, sin())),
            "gamma" => {
                let slope = cx.eval(env.call("d", &[u, x]))?;
                let real_slope = matches!(env.ar.num(slope), Some(Num::Real(Real::Exact(_))));
                if !real_slope || env.contains(slope, x) {
                    *failed = true;
                    return Ok(t);
                }
                let mut least = f64::INFINITY;
                for e in [a, b] {
                    let v = cx.eval(env.call("n", &[env.call("re", &[env.call("rewrite", &[u, env.call(names::RULE, &[x, e])])])]))?;
                    match env.ar.num(v) {
                        Some(Num::Real(r)) => least = least.min(r.to_f64()),
                        _ => {
                            *failed = true;
                            return Ok(t);
                        }
                    }
                }
                let m = (0.5 - least).ceil().max(0.0);
                if !m.is_finite() || m > 60.0 {
                    *failed = true;
                    return Ok(t);
                }
                let m = m as i64;
                if m == 0 {
                    return Ok(env.call("gamma", &[u]));
                }
                let mut den = vec![];
                for j in 0..m {
                    den.push(env.ar.add(&[u, env.int(j)]));
                }
                let shifted = env.call("gamma", &[env.ar.add(&[u, env.int(m)])]);
                return Ok(q(shifted, env.ar.mul(&den)));
            }
            _ => {}
        }
    }
    Ok(if na.as_slice() == args { t } else { env.rebuild(head, &na) })
}

/// Proves that N and D have no common zero on the range (the real interval or the rectangle of its ends): a cover
/// by boxes on each of which the enclosure of N or that of D excludes 0. `Some(true)` when proved, `None` when not
/// at this precision.
fn no_common_zero<'h>(env: &Env<'h>, x: T<'h>, n: T<'h>, d: T<'h>, range: &(EndT<'h>, EndT<'h>), w: Prec, m: &mut Meter) -> WorkResult<Option<bool>> {
    let (p, bud) = (w.p, w.bud);
    let h = env.h();
    let View::Sym(xs) = h.view(x) else { return Ok(None) };
    let (EndT::Num(a), EndT::Num(b)) = range else { return Ok(None) };
    let nm = Names::new(h);
    let both = env.list(&[n, d]);
    let g = Graph::build_with_vars(h, &env.ar, &nm, both, &[xs]);
    let (Some(i_n), Some(i_d)) = (g.node(n), g.node(d)) else { return Ok(None) };
    let (isn, isd) = (g.island(i_n), g.island(i_d));
    let num = |t: T<'h>| -> Option<CBall> {
        let gg = Graph::build(h, &env.ar, &nm, t);
        let i = gg.node(t)?;
        gg.eval_complex(&gg.island(i), p + 16)
    };
    let (Some(za), Some(zb)) = (num(*a), num(*b)) else { return Ok(None) };
    // the hull of the two ends (balls widened to their ends)
    let lohi = |u: &Ball, v: &Ball| -> (Float, Float) {
        let (ul, uh) = self::ends(u);
        let (vl, vh) = self::ends(v);
        let lo = if ul.cmp_exact(&vl) == Ordering::Greater { vl } else { ul };
        let hi = if uh.cmp_exact(&vh) == Ordering::Less { vh } else { uh };
        (lo, hi)
    };
    let (x0, x1) = lohi(&za.re, &zb.re);
    let (y0, y1) = lohi(&za.im, &zb.im);
    let size = x1.sub_exact(&x0).top().max(y1.sub_exact(&y0).top());
    let mut stack = vec![(x0, x1, y0, y1)];
    while let Some((xl, xr, yl, yr)) = stack.pop() {
        m.step(4)?;
        if !take(bud) {
            return Ok(None);
        }
        let z = CBall::new(span(&xl, &xr), span(&yl, &yr));
        let excl = |is: &Island| g.eval_complex_at(is, p, std::slice::from_ref(&z)).is_some_and(|v| !v.contains_zero());
        // D first: a polynomial or a few circular functions, cheap; N only near the zeros of D
        if excl(&isd) || excl(&isn) {
            continue;
        }
        let (wx, wy) = (xr.sub_exact(&xl), yr.sub_exact(&yl));
        let scale = xl.top().max(xr.top()).max(yl.top()).max(yr.top()).max(size);
        if wx.top().max(wy.top()) < (size - 40).max(scale - p as i64 + 12) {
            return Ok(None);
        }
        if wx.cmp_exact(&wy) != Ordering::Less {
            let s = split_point(&xl, &xr, 8);
            stack.push((xl, s.clone(), yl.clone(), yr.clone()));
            stack.push((s, xr, yl, yr));
        } else {
            let s = split_point(&yl, &yr, 8);
            stack.push((xl.clone(), xr.clone(), yl, s.clone()));
            stack.push((xl, xr, s, yr));
        }
    }
    Ok(Some(true))
}

/// Is f proved nonzero at the number a (an enclosure at 128 bits)?
fn nonzero_at<'h>(env: &Env<'h>, f: T<'h>, x: T<'h>, a: T<'h>) -> bool {
    let h = env.h();
    let View::Sym(xs) = h.view(x) else { return false };
    let nm = Names::new(h);
    let ga = Graph::build(h, &env.ar, &nm, a);
    let Some(za) = ga.node(a).and_then(|i| ga.eval_complex(&ga.island(i), 144)) else { return false };
    let g = Graph::build_with_vars(h, &env.ar, &nm, f, &[xs]);
    let Some(i) = g.node(f) else { return false };
    g.eval_complex_at(&g.island(i), 128, &[za]).is_some_and(|v| !v.contains_zero())
}

/// The working precision (bits) and the box budget of one count.
#[derive(Clone, Copy)]
struct Prec<'a> {
    p: u64,
    bud: &'a Cell<u64>,
}

/// A range enclosure of a real function or of its derivative (`usize` 0 or 1) over a ball: balls for the lower and
/// the upper end.
type Encl<'a> = &'a dyn Fn(&Ball, usize) -> Option<(Ball, Ball)>;

/// The functions f, f', ... as numeric graphs in x.
struct Fns<'h> {
    g: Graph<'h>,
    is: Vec<Island>,
}

impl Fns<'_> {
    /// An enclosure of f^(j) over the region z, or `None`.
    fn at(&self, j: usize, z: &CBall, p: u64) -> Option<CBall> {
        self.g.eval_complex_at(&self.is[j], p, std::slice::from_ref(z))
    }
    /// An enclosure of the real function f^(j) over the real ball x, or `None` (no real value there).
    fn real(&self, j: usize, x: &Ball, p: u64) -> Option<Ball> {
        let z = self.at(j, &CBall::real(x.clone()), p)?;
        z.is_real().then_some(z.re)
    }
    /// The sign of f at the exact point x when proved.
    fn sign(&self, x: &Float, p: u64) -> Option<bool> {
        let v = self.real(0, &Ball::exact(x.clone()), p)?;
        (!v.contains_zero()).then_some(!v.mid.neg)
    }
}

/// The count at working precision `p`; `None` when not proved at this precision.
fn count_at<'h>(
    env: &Env<'h>,
    x: T<'h>,
    ds: &[T<'h>],
    ends: &(EndT<'h>, EndT<'h>),
    zeros: &[Vec<bool>],
    w: Prec,
    m: &mut Meter,
) -> WorkResult<Option<u64>> {
    let p = w.p;
    let h = env.h();
    let View::Sym(xs) = h.view(x) else { return Ok(None) };
    let nm = Names::new(h);
    let all = env.list(ds);
    let g = Graph::build_with_vars(h, &env.ar, &nm, all, &[xs]);
    let mut is = vec![];
    for &d in ds {
        match g.node(d) {
            Some(i) => {
                let island = g.island(i);
                // library functions whose enclosures over wide real intervals and complex boxes are quick (others,
                // erfc of a complex box, can take minutes per box)
                if g.library_names(&island).iter().any(|n| !COUNT_LIBRARY.contains(n)) {
                    return Ok(None);
                }
                is.push(island);
            }
            // f' must exist; higher derivatives only serve the ends
            None if is.len() >= 2 => break,
            None => return Ok(None),
        }
    }
    let fns = Fns { g, is };
    let num = |t: T<'h>| -> Option<CBall> {
        let gg = Graph::build(h, &env.ar, &nm, t);
        let i = gg.node(t)?;
        gg.eval_complex(&gg.island(i), p + 16)
    };
    match ends {
        (EndT::Num(a), EndT::Num(b)) => {
            let (Some(za), Some(zb)) = (num(*a), num(*b)) else { return Ok(None) };
            if za.is_real() && zb.is_real() {
                // a real interval: ordered ends
                let (lo, hi, zl, zh) = match za.re.cmp3(&zb.re) {
                    Some(Ordering::Greater) => (zb.re, za.re, &zeros[1], &zeros[0]),
                    Some(_) => (za.re, zb.re, &zeros[0], &zeros[1]),
                    None => return Ok(None),
                };
                interval(&fns, &lo, &hi, zl, zh, w, m)
            } else {
                // a rectangle with corners a and b: exact dyadic corners only
                let parts = [&za.re, &za.im, &zb.re, &zb.im];
                if parts.iter().any(|b| !b.rad.is_zero()) {
                    return Ok(None);
                }
                let order = |u: &Ball, v: &Ball| if u.mid.cmp_exact(&v.mid) == Ordering::Greater { (v.mid.clone(), u.mid.clone()) } else { (u.mid.clone(), v.mid.clone()) };
                let (x0, x1) = order(&za.re, &zb.re);
                let (y0, y1) = order(&za.im, &zb.im);
                // abs is not analytic: its enclosures do not show where the argument principle fails
                if x0.cmp_exact(&x1) == Ordering::Equal || y0.cmp_exact(&y1) == Ordering::Equal || fns.g.has_abs(&fns.is[0]) {
                    return Ok(None);
                }
                rectangle(&fns, [&x0, &x1, &y0, &y1], w, m)
            }
        }
        (lo, hi) => {
            // half-lines and the whole line: sums of real powers only
            let Some(terms) = powers(env, ds[0], x, p) else { return Ok(None) };
            let pos = |lo: &EndT<'h>| -> Option<bool> {
                match lo {
                    EndT::NegInf => Some(false),
                    EndT::Num(a) => {
                        let z = num(*a)?;
                        (z.is_real() && z.re.rad.is_zero() && z.re.mid.is_zero()).then_some(true)
                    }
                    EndT::PosInf => None,
                }
            };
            if !matches!(hi, EndT::PosInf) {
                return Ok(None);
            }
            match pos(lo) {
                // [0, inf): a root at 0 only when the least exponent is positive, which is declined
                Some(true) => {
                    if terms[0].1.mid.is_zero() && terms[0].1.rad.is_zero() {
                        positive(&terms, w, m)
                    } else {
                        Ok(None)
                    }
                }
                // the whole line: integer exponents >= 0
                Some(false) => {
                    let mut ints = vec![];
                    for (c, a) in &terms {
                        let Some(k) = small_int(a) else { return Ok(None) };
                        if k < 0 {
                            return Ok(None);
                        }
                        ints.push((c.clone(), a.clone(), k));
                    }
                    let order0 = ints[0].2 as u64;
                    let Some(np) = positive(&terms, w, m)? else { return Ok(None) };
                    let mirrored: Vec<(Ball, Ball)> = ints.iter().map(|(c, a, k)| (if k % 2 == 1 { c.neg() } else { c.clone() }, a.clone())).collect();
                    let Some(nn) = positive(&mirrored, w, m)? else { return Ok(None) };
                    Ok(Some(np + nn + order0))
                }
                None => Ok(None),
            }
        }
    }
}

/// A small integer equal to the exact ball `a`.
fn small_int(a: &Ball) -> Option<i64> {
    if !a.rad.is_zero() || a.mid.exp < 0 || a.mid.top() > 40 {
        return if a.mid.is_zero() && a.rad.is_zero() { Some(0) } else { None };
    }
    let mut v: i64 = 0;
    for i in (0..a.mid.man.bits()).rev() {
        v = 2 * v + i64::from(a.mid.man.bit(i));
    }
    v <<= a.mid.exp;
    Some(if a.mid.neg { -v } else { v })
}

/// The terms (coefficient, exponent) of f as a sum of c x^a with real numeric c and a, by increasing exponent (all
/// exponents proved distinct); `None` when f is not such a sum.
fn powers<'h>(env: &Env<'h>, f: T<'h>, x: T<'h>, p: u64) -> Option<Vec<(Ball, Ball)>> {
    let h = env.h();
    let nm = Names::new(h);
    let real = |t: T<'h>| -> Option<Ball> {
        let g = Graph::build(h, &env.ar, &nm, t);
        let i = g.node(t)?;
        let z = g.eval_complex(&g.island(i), p + 16)?;
        z.is_real().then_some(z.re)
    };
    let terms: Vec<T<'h>> = match h.view(f) {
        View::App { head, args } if head == env.hd.add => args.to_vec(),
        _ => vec![f],
    };
    let mut out: Vec<(Ball, Ball)> = vec![];
    for t in terms {
        let (coef, power): (Vec<T<'h>>, Option<T<'h>>) = {
            let factors: Vec<T<'h>> = match h.view(t) {
                View::App { head, args } if head == env.hd.mul => args.to_vec(),
                _ => vec![t],
            };
            let mut coef = vec![];
            let mut power = None;
            for u in factors {
                if !env.contains(u, x) {
                    coef.push(u);
                } else if u == x {
                    if power.is_some() {
                        return None;
                    }
                    power = Some(env.int(1));
                } else if let View::App { head, args: &[b, e] } = h.view(u) {
                    if head != env.hd.pow || b != x || env.contains(e, x) || power.is_some() {
                        return None;
                    }
                    power = Some(e);
                } else {
                    return None;
                }
            }
            (coef, power)
        };
        let c = if coef.is_empty() { Ball::from_i64(1) } else { real(env.ar.mul(&coef))? };
        let a = match power {
            None => Ball::from_i64(0),
            Some(e) => real(e)?,
        };
        if c.contains_zero() {
            return None;
        }
        out.push((c, a));
    }
    // order by exponent; every pair must be decided
    for i in 0..out.len() {
        for j in i + 1..out.len() {
            out[i].1.cmp3(&out[j].1)?;
        }
    }
    out.sort_by(|u, v| u.1.cmp3(&v.1).unwrap_or(Ordering::Equal));
    (out.len() >= 2).then_some(out)
}

/// The number of roots on (0, inf) of the sum of c x^a (terms by increasing exponent).
fn positive(terms: &[(Ball, Ball)], w: Prec, m: &mut Meter) -> WorkResult<Option<u64>> {
    let p = w.p;
    let n = terms.len() - 1;
    let changes = terms.windows(2).filter(|w| w[0].0.mid.neg != w[1].0.mid.neg).count() as u64;
    // no sign change: no root; one: exactly one (the count has the parity of the changes)
    if changes <= 1 {
        return Ok(Some(changes));
    }
    let sum_abs = |r: std::ops::Range<usize>| -> Ball {
        let mut s = Ball::from_i64(0);
        for (c, _) in &terms[r] {
            s = s.add(&Ball { mid: c.mid.abs(), rad: c.rad }, p);
        }
        s
    };
    // eps = 2^-k with |c_0| > S eps^d, d = a_1 - a_0 > 0
    let d0 = terms[1].1.sub(&terms[0].1, p);
    let s0 = sum_abs(1..n + 1);
    let c0 = Ball { mid: terms[0].0.mid.abs(), rad: terms[0].0.rad };
    let mut eps = None;
    for k in 0..4096i64 {
        m.step(1)?;
        let e = Ball::exact(Float::new(false, mc_arith::num::Nat::from_u64(1), -k));
        let Some(pw) = mc_numeric::elementary::pow(&e, &d0, p) else { continue };
        if c0.lt(&s0.mul(&pw, p)) == Some(false) && !c0.sub(&s0.mul(&pw, p), p).contains_zero() {
            eps = Some(e.mid);
            break;
        }
    }
    // R = 2^k with |c_n| > S R^(a_(n-1) - a_n)
    let d1 = terms[n - 1].1.sub(&terms[n].1, p);
    let s1 = sum_abs(0..n);
    let cn = Ball { mid: terms[n].0.mid.abs(), rad: terms[n].0.rad };
    let mut big = None;
    for k in 0..1 << 20 {
        m.step(1)?;
        let e = Ball::exact(Float::new(false, mc_arith::num::Nat::from_u64(1), k));
        let Some(pw) = mc_numeric::elementary::pow(&e, &d1, p) else { continue };
        let rest = s1.mul(&pw, p);
        if rest.lt(&cn) == Some(true) && !cn.sub(&rest, p).contains_zero() {
            big = Some(e.mid);
            break;
        }
    }
    let (Some(eps), Some(big)) = (eps, big) else { return Ok(None) };
    // the sum as a function on balls: sum c x^a, and its derivative sum c a x^(a - 1)
    // x^a by binary powering for an integer a (exp(a log x) loses a's size in bits and is slow)
    let point = |x: &Ball, a: &Ball, shift: i64| -> Option<Ball> {
        match small_int(a) {
            Some(k) => mc_numeric::elementary::pow_int(x, &mc_arith::num::Int::from_i64(k - shift), p),
            None => mc_numeric::elementary::pow(x, &a.sub(&Ball::from_i64(shift), p), p),
        }
    };
    // c x^b is monotone in x > 0 for each b: over a ball of positive x it lies between the values at the ends, and
    // the sum between the sums of the lower and of the upper ends (a ball, m +- r, cannot hold the range of
    // x^1000000 on [1.5, 1.6]: its lower end is lost to rounding)
    let f = |x: &Ball, j: usize| -> Option<(Ball, Ball)> {
        let (xl, xh) = ends(x);
        if xl.neg || xl.is_zero() {
            return None;
        }
        let (mut lo, mut hi) = (Ball::from_i64(0), Ball::from_i64(0));
        for (c, a) in terms {
            let coef = if j == 0 { c.clone() } else { c.mul(a, p) };
            let u = coef.mul(&point(&Ball::exact(xl.clone()), a, j as i64)?, p);
            let v = if x.rad.is_zero() { u.clone() } else { coef.mul(&point(&Ball::exact(xh.clone()), a, j as i64)?, p) };
            let ((ul, uh), (vl, vh)) = (ends(&u), ends(&v));
            let low = if ul.cmp_exact(&vl) == Ordering::Less { ul } else { vl };
            let high = if uh.cmp_exact(&vh) == Ordering::Greater { uh } else { vh };
            lo = lo.add(&Ball::exact(low), p);
            hi = hi.add(&Ball::exact(high), p);
        }
        Some((lo, hi))
    };
    let Some(count) = bisect(&|x: &Ball, j: usize| f(x, j), &eps, &big, w, m)? else { return Ok(None) };
    // the rule of signs: at most the sign changes, with the same parity
    if count > changes || !(changes - count).is_multiple_of(2) {
        return Ok(None);
    }
    Ok(Some(count))
}

/// One box from the budget; false when it is spent.
fn take(bud: &Cell<u64>) -> bool {
    let left = bud.get();
    bud.set(left.saturating_sub(1));
    left > 0
}

/// The ends of a ball, exactly.
fn ends(b: &Ball) -> (Float, Float) {
    if b.rad.is_zero() {
        return (b.mid.clone(), b.mid.clone());
    }
    let r = b.rad.to_float();
    (b.mid.sub_exact(&r), b.mid.add_exact(&r))
}

/// The sign of a range [lo, hi] (given by a ball for each end) when it excludes zero: `Some(true)` for positive.
fn range_sign(lo: &Ball, hi: &Ball) -> Option<bool> {
    if !lo.contains_zero() && !lo.mid.neg {
        Some(true)
    } else if !hi.contains_zero() && hi.mid.neg {
        Some(false)
    } else {
        None
    }
}

/// The roots on the real interval [lo, hi] (balls of the ends; `zl`, `zh` the exact zeros of f, f', ... there).
fn interval(fns: &Fns, lo: &Ball, hi: &Ball, zl: &[bool], zh: &[bool], w: Prec, m: &mut Meter) -> WorkResult<Option<u64>> {
    let p = w.p;
    let mut count = 0;
    // a root at an end: its multiplicity from the exact zeros, and a one-sided neighbourhood where f^(k) has a sign
    let order = |z: &[bool]| z.iter().take_while(|&&b| b).count();
    let width = hi.mid.sub_exact(&lo.mid);
    if width.neg || width.is_zero() {
        return Ok(None);
    }
    let mut a = lo.mid.clone();
    let mut b = hi.mid.clone();
    for (end, z, toward) in [(lo, zl, 1i64), (hi, zh, -1i64)] {
        let k = order(z);
        if k == 0 {
            // f nonzero on the whole ball of the end: no root there, the count runs between the midpoints
            let Some(v) = fns.real(0, end, p) else { return Ok(None) };
            if v.contains_zero() {
                return Ok(None);
            }
            continue;
        }
        if k > MAX_ORDER || fns.is.len() <= k {
            return Ok(None);
        }
        // f^(k) nonzero on [lo - d, hi + d] around the ball of the end: by Rolle's theorem f has at most k roots
        // there with multiplicity, and the end is one of order k, so it is the only one
        let (el, eh) = ends(end);
        let mut done = false;
        for s in 3..60 {
            m.step(1)?;
            let step = Float::new(width.neg, width.man.clone(), width.exp - s);
            let (l, r) = (el.sub_exact(&step), eh.add_exact(&step));
            let other = if toward > 0 { r.clone() } else { l.clone() };
            let Some(v) = fns.real(k, &span(&l, &r), p) else { continue };
            if !v.contains_zero() && fns.sign(&other, p).is_some() {
                count += k as u64;
                if toward > 0 {
                    a = other;
                } else {
                    b = other;
                }
                done = true;
                break;
            }
        }
        if !done {
            return Ok(None);
        }
    }
    if a.cmp_exact(&b) != Ordering::Less {
        return Ok(None);
    }
    let f = |x: &Ball, j: usize| fns.real(j, x, p).map(|v| (v.clone(), v));
    let Some(n) = bisect(&f, &a, &b, w, m)? else { return Ok(None) };
    Ok(Some(count + n))
}

/// The ball of the dyadic interval [l, r].
fn span(l: &Float, r: &Float) -> Ball {
    let s = l.add_exact(r);
    let w = r.sub_exact(l);
    Ball { mid: Float::new(s.neg, s.man, s.exp - 1), rad: Mag::from_float_up(&Float::new(false, w.man, w.exp - 1)) }
}

/// l + (r - l) k/16, exactly.
fn split_point(l: &Float, r: &Float, k: i64) -> Float {
    let w = r.sub_exact(l).mul_exact(&Float::from_i64(k));
    l.add_exact(&Float::new(w.neg, w.man, w.exp - 4))
}

/// The roots of a real function on [a, b] (exact dyadic ends where it is nonzero), simple roots only: `f(x, 0)`
/// encloses the values of the function over the ball x in a range [lo, hi] (a ball for each end), `f(x, 1)` those of
/// its derivative.
fn bisect(f: Encl, a: &Float, b: &Float, w: Prec, m: &mut Meter) -> WorkResult<Option<u64>> {
    let (p, bud) = (w.p, w.bud);
    let sign = |x: &Float| -> Option<bool> {
        let (lo, hi) = f(&Ball::exact(x.clone()), 0)?;
        range_sign(&lo, &hi)
    };
    let (Some(sa), Some(sb)) = (sign(a), sign(b)) else { return Ok(None) };
    let mut stack = vec![(a.clone(), b.clone(), sa, sb)];
    let size = b.sub_exact(a).top();
    let mut count = 0u64;
    while let Some((l, r, sl, sr)) = stack.pop() {
        m.step(4)?;
        if !take(bud) {
            return Ok(None);
        }
        let x = span(&l, &r);
        let w = r.sub_exact(&l);
        match f(&x, 0) {
            Some((lo, hi)) => {
                if range_sign(&lo, &hi).is_some() {
                    continue;
                }
            }
            // no enclosure on a box this small: a pole or a point outside the domain; not retried
            None if w.top() < size - 40 => {
                bud.set(0);
                return Ok(None);
            }
            None => {}
        }
        if let Some((lo, hi)) = f(&x, 1) {
            if range_sign(&lo, &hi).is_some() {
                // monotone: one root when the signs at the ends differ
                if sl != sr {
                    count += 1;
                }
                continue;
            }
        }
        // too narrow to split at this precision: a multiple root or a cluster
        let scale = l.top().max(r.top()).max(size);
        if w.is_zero() || w.top() < scale - p as i64 + 12 {
            return Ok(None);
        }
        let mut cut = None;
        for k in [8, 7, 9, 6, 10, 5, 11] {
            let s = split_point(&l, &r, k);
            if let Some(ss) = sign(&s) {
                cut = Some((s, ss));
                break;
            }
        }
        let Some((s, ss)) = cut else { return Ok(None) };
        stack.push((s.clone(), r, ss, sr));
        stack.push((l, s, sl, ss));
    }
    Ok(Some(count))
}

/// The zeros of f in the closed rectangle [x0, x1] x [y0, y1] (exact dyadic corners), by the argument principle.
fn rectangle(fns: &Fns, [x0, x1, y0, y1]: [&Float; 4], w: Prec, m: &mut Meter) -> WorkResult<Option<u64>> {
    let (p, bud) = (w.p, w.bud);
    // analytic on the rectangle: a cover by boxes with enclosures
    let size = x1.sub_exact(x0).top().max(y1.sub_exact(y0).top());
    let mut stack = vec![(x0.clone(), x1.clone(), y0.clone(), y1.clone())];
    while let Some((xl, xr, yl, yr)) = stack.pop() {
        m.step(4)?;
        if !take(bud) {
            return Ok(None);
        }
        let z = CBall::new(span(&xl, &xr), span(&yl, &yr));
        if fns.at(0, &z, p).is_some() {
            continue;
        }
        let (wx, wy) = (xr.sub_exact(&xl), yr.sub_exact(&yl));
        let scale = xl.top().max(xr.top()).max(yl.top()).max(yr.top()).max(size);
        // a box this small without an enclosure: a pole or a cut (or enclosures too coarse to show otherwise); not
        // retried at a higher precision
        if wx.top().max(wy.top()) < (size - 12).max(scale - p as i64 + 12) {
            bud.set(0);
            return Ok(None);
        }
        if wx.cmp_exact(&wy) != Ordering::Less {
            let s = split_point(&xl, &xr, 8);
            stack.push((xl, s.clone(), yl.clone(), yr.clone()));
            stack.push((s, xr, yl, yr));
        } else {
            let s = split_point(&yl, &yr, 8);
            stack.push((xl.clone(), xr.clone(), yl, s.clone()));
            stack.push((xl, xr, s, yr));
        }
    }
    // the winding of f along the boundary, counterclockwise
    let corners = [(x0, y0), (x1, y0), (x1, y1), (x0, y1)];
    let mut total = 0.0f64;
    for i in 0..4 {
        let (za, zb) = (corners[i], corners[(i + 1) % 4]);
        let horizontal = za.1.cmp_exact(zb.1) == Ordering::Equal;
        let point = |t: &Float| -> CBall {
            if horizontal {
                CBall::new(Ball::exact(t.clone()), Ball::exact(za.1.clone()))
            } else {
                CBall::new(Ball::exact(za.0.clone()), Ball::exact(t.clone()))
            }
        };
        let (t0, t1) = if horizontal { (za.0.clone(), zb.0.clone()) } else { (za.1.clone(), zb.1.clone()) };
        let value = |t: &Float| -> Option<(f64, f64)> {
            let v = fns.at(0, &point(t), p)?;
            // the value to 2^-30 of its modulus, so that the angles (at most MAX_BOXES of them) add up to within
            // a small fraction of a turn
            let size = v.re.mid.top().max(v.im.mid.top());
            if [&v.re, &v.im].iter().any(|b| !b.rad.is_zero() && b.rad.log2() > (size - 32) as f64) {
                return None;
            }
            // scaled by 2^-size (the angle of each value does not depend on its scale): no overflow
            let to = |b: &Ball| mc_arith::num::float::dyadic_to_f64(b.mid.neg, &b.mid.man, b.mid.exp - size);
            Some((to(&v.re), to(&v.im)))
        };
        let mut stack = vec![(t0, t1)];
        while let Some((ta, tb)) = stack.pop() {
            m.step(4)?;
            if !take(bud) {
                return Ok(None);
            }
            let (lo, hi) = if ta.cmp_exact(&tb) == Ordering::Less { (&ta, &tb) } else { (&tb, &ta) };
            let seg = if horizontal {
                CBall::new(span(lo, hi), Ball::exact(za.1.clone()))
            } else {
                CBall::new(Ball::exact(za.0.clone()), span(lo, hi))
            };
            let half_plane = fns.at(0, &seg, p).is_some_and(|v| !v.re.contains_zero() || !v.im.contains_zero());
            if half_plane {
                if let (Some(wa), Some(wb)) = (value(&ta), value(&tb)) {
                    // the principal angle from wa to wb
                    let cross = wa.0 * wb.1 - wa.1 * wb.0;
                    let dot = wa.0 * wb.0 + wa.1 * wb.1;
                    let angle = cross.atan2(dot);
                    if angle.is_finite() && angle.abs() < 3.0 && (wa.0 != 0.0 || wa.1 != 0.0) && (wb.0 != 0.0 || wb.1 != 0.0) {
                        total += angle;
                        continue;
                    }
                }
            }
            let w = hi.sub_exact(lo);
            let scale = lo.top().max(hi.top()).max(size);
            if w.top() < scale - p as i64 + 12 {
                return Ok(None);
            }
            let s = split_point(&ta, &tb, 8);
            stack.push((s.clone(), tb));
            stack.push((ta, s));
        }
    }
    let turns = total / std::f64::consts::TAU;
    let n = turns.round();
    if (turns - n).abs() > 0.25 || n < 0.0 {
        return Ok(None);
    }
    Ok(Some(n as u64))
}
