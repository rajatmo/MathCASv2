//! Proved images of a real interval under a function of one real argument (round 41), for `interval` and
//! `centered_interval` arguments: the result is a promise that every value lies inside, so it rests on enclosures,
//! never on samples.
//!
//! [`image`] covers [a, b] by cells (bisection from the whole interval) and settles each cell by one of:
//!
//! 1. *monotone*: f' evaluated on the ball of the cell excludes 0 (the mean-value or monotonicity test of interval
//!    analysis), so the image of the cell lies between the values at its ends;
//! 2. *one extremum*: f'' evaluated on the ball excludes 0, so f' is strictly monotone on the cell (f convex or
//!    concave); the far extreme is at an end, and the near one lies in a bracket [xl, xr] narrowed by bisection on
//!    proved signs of f' (f' < 0 left of xl, > 0 right of xr for a convex f), enclosed by f on the ball of the
//!    bracket, or exactly f(q) when f'(q) is exactly 0 at a simple rational q of the bracket (the unique zero);
//! 3. *enclosed*: f on the ball of a cell of the smallest width (2^-30 of the interval).
//!
//! Other cells are split. The ball arithmetic and the special-function balls bound every point of a ball argument
//! (decision SF.4), so each step is an enclosure; values at points are taken on point balls and rounded outwards,
//! exact ends keep their exact values. A cell of the smallest width on which neither f nor f' has a ball (a pole, a
//! singularity) makes the image the whole line; at an end where f is infinite exactly (a pole at the end, as csch at
//! 0) or finite with a monotone neighbour cell (a branch point at the end, as sqrt at 0), the last cell (of width
//! 2^-30 of the interval) is taken as monotone, as f is next to a pole of a meromorphic function (f ~ c (x - a)^-m)
//! and at the algebraic and logarithmic branch points of the library. Complex values, or more cells than the budget,
//! give no image (the call stays).
//!
//! Round 42: a cell without an enclosure inside the interval at an exact pole q (a rational with a small denominator
//! or such a multiple of pi, where f is infinite, or stays unevaluated with |f| tending to infinity by `limit`)
//! splits the interval at q, and the image is the union of the images of the two sides (two rays for a simple
//! pole: acsch over [-4, csch 2] is (-inf, -acsch 4] and [2, inf)). A function whose derivative has no closed form
//! takes that of its `function_expand` form; a constant brought into a value by the function's own formula is made
//! a ball too. Branch and bound on values has a budget of enclosures per end and then returns the least lower bound
//! of its cells, proved but looser than asked (a ball form whose enclosure shrinks only in proportion to the width).
//!
//! Literature: R. E. Moore, R. B. Kearfott, M. J. Cloud, "Introduction to Interval Analysis", SIAM 2009, chapters 5
//! and 6 (monotonicity test, subdivision); F. Johansson, "Arb: efficient arbitrary-precision midpoint-radius interval
//! arithmetic", IEEE Transactions on Computers 66 (2017) 1281-1292 (ball arithmetic).
use crate::formula::Fx;
use mc_api::{Store, T};
use mc_arith::num::float::{dyadic_to_f64, f64_to_dyadic};
use mc_arith::num::{Ball, Float, Mag};
use mc_arith::{Num, Real};
use mc_eval::Res;
use std::collections::HashMap;

/// The precision of the balls (bits).
const BITS: u64 = 64;
/// The precision of the balls of cells (bits).
const CELL_BITS: u64 = 64;
/// The smallest cell is 2^-MAX_DEPTH of the interval.
const MAX_DEPTH: u32 = 30;
/// At most this many cells are settled.
const MAX_CELLS: usize = 600;
/// At most this many enclosures for each end in branch and bound.
const BOUNDS_CELLS: usize = 60;

/// An enclosure of a real value set: lower and upper machine bounds, or complex values, or none.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Enc {
    Real(f64, f64),
    Complex,
    No,
}

fn float_of(v: f64) -> Float {
    match f64_to_dyadic(v) {
        Some((neg, man, exp)) => Float::new(neg, man, exp),
        None => Float::zero(),
    }
}

fn f64_of(f: &Float) -> f64 {
    dyadic_to_f64(f.neg, &f.man, f.exp)
}

/// The ball of [lo, hi] (machine ends taken exactly).
fn ball_term<'h>(fx: &Fx<'_, '_, '_, 'h>, lo: f64, hi: f64) -> T<'h> {
    let (l, r) = (float_of(lo), float_of(hi));
    let s = l.add_exact(&r);
    let mid = Float::new(s.neg, s.man.clone(), s.exp - 1);
    let rad = Mag::from_float_up(&r.sub_exact(&mid));
    // a point at the full precision; a cell at a low one (its width limits the result's accuracy, and a function
    // evaluated in balls stops refining at the precision of its arguments)
    let bits = if lo == hi { BITS } else { CELL_BITS };
    fx.ar.number(&Num::Real(Real::Ball(Ball { mid, rad }, bits)))
}

/// The ball of midpoint `c` and radius `r` (machine numbers taken exactly, the radius rounded up).
pub(crate) fn ball_mid_rad<'h>(fx: &Fx<'_, '_, '_, 'h>, c: f64, r: f64) -> T<'h> {
    let rad = Mag::from_float_up(&float_of(r.abs()));
    fx.ar.number(&Num::Real(Real::Ball(Ball { mid: float_of(c), rad }, BITS)))
}

fn real_bounds(r: &Real) -> Option<(f64, f64)> {
    match r {
        Real::Ball(b, _) => {
            let rad = b.rad.to_float();
            let (l, h) = (b.mid.sub_exact(&rad), b.mid.add_exact(&rad));
            let (lo, hi) = (f64_of(&l), f64_of(&h));
            // a bound that is a machine number exactly stays; others move outwards past the rounding
            let exact = |v: f64, f: &Float| float_of(v).cmp_exact(f) == std::cmp::Ordering::Equal;
            let lo = if exact(lo, &l) { lo } else { lo.next_down() };
            let hi = if exact(hi, &h) { hi } else { hi.next_up() };
            (lo.is_finite() && hi.is_finite()).then_some((lo, hi))
        }
        Real::Exact(q) => {
            let v = r.to_f64();
            // an integer that a machine number holds exactly stays a point
            if q.is_int() && v.abs() < 9.0e15 {
                return Some((v, v));
            }
            v.is_finite().then(|| (v.next_down(), v.next_up()))
        }
        // a machine value carries no error bound
        Real::Machine(_) => None,
    }
}

/// Machine bounds of a real value with an error bound (an exact real, a ball, a complex number whose imaginary part
/// is 0 or a ball around 0), rounded outwards; `None` for a machine number, a complex value or no number.
pub(crate) fn bounds<'h>(fx: &Fx<'_, '_, '_, 'h>, t: T<'h>) -> Option<(f64, f64)> {
    match enc_of(&fx.num(t)?) {
        Enc::Real(a, b) => Some((a, b)),
        _ => None,
    }
}

/// `e` with its symbolic constants as 30-digit balls (for evaluation on balls).
pub(crate) fn constants_as_balls<'h>(fx: &mut Fx<'_, '_, '_, 'h>, e: T<'h>) -> Res<T<'h>> {
    let rules = constant_rules(fx, e);
    if rules.is_empty() {
        return Ok(e);
    }
    fx.eval(fx.call("rewrite", &[e, fx.list(&rules)]))
}

fn enc_of(n: &Num) -> Enc {
    match n {
        Num::Real(r) => real_bounds(r).map_or(Enc::No, |(a, b)| Enc::Real(a, b)),
        Num::Complex(re, im) => {
            let im_zero = match im {
                Real::Exact(q) => q.is_zero(),
                Real::Ball(b, _) => b.contains_zero(),
                Real::Machine(_) => return Enc::No,
            };
            if !im_zero {
                return Enc::Complex;
            }
            real_bounds(re).map_or(Enc::No, |(a, b)| Enc::Real(a, b))
        }
    }
}

/// A finite real value of a closed numeric term, as an `f64` (by `n` when it is not a number).
fn approx<'h>(fx: &mut Fx<'_, '_, '_, 'h>, t: T<'h>) -> Res<Option<f64>> {
    if let Some(n) = fx.num(t) {
        return Ok(match n {
            Num::Real(r) => Some(r.to_f64()).filter(|v| v.is_finite()),
            _ => None,
        });
    }
    let v = fx.eval(fx.call("n", &[t]))?;
    Ok(fx.f64_of(v).filter(|v| v.is_finite()))
}

/// Is `t` (an exact or machine real whose nearest double is `v`) exactly `v`?
fn exactly<'h>(fx: &mut Fx<'_, '_, '_, 'h>, t: T<'h>, v: f64) -> Res<bool> {
    match fx.num(t) {
        Some(Num::Real(Real::Machine(_))) => Ok(true),
        Some(Num::Real(Real::Exact(_))) => {
            let e = fx.eval(fx.call("equal", &[t, fx.call("set_precision", &[fx.real(v), fx.inf()])]))?;
            Ok(e == fx.bool(true))
        }
        _ => Ok(false),
    }
}

/// Is the value at an exact point infinite (`complex_infinity`, a real directed infinity, a machine infinity, a
/// `failure(pole, ...)`, or a closed form whose `n` is one of these, as csch(0))?
fn infinite<'h>(fx: &mut Fx<'_, '_, '_, 'h>, v: T<'h>) -> Res<bool> {
    let direct = |fx: &Fx<'_, '_, '_, 'h>, v: T<'h>| {
        v == fx.sym("complex_infinity")
            || fx.infinity(v).is_some_and(|d| d == 0 || d == 2)
            || fx.machine_of(v).is_some_and(f64::is_infinite)
            || fx.call_of(v, "failure").is_some_and(|a| a.first() == Some(&fx.sym("pole")))
    };
    if direct(fx, v) {
        return Ok(true);
    }
    if fx.num(v).is_some() || fx.call_of(v, "failure").is_some() {
        return Ok(false);
    }
    let w = fx.eval(fx.call("n", &[v]))?;
    Ok(direct(fx, w))
}

/// A candidate end of the image: an exact term, or a machine bound already rounded outwards.
#[derive(Clone, Copy)]
enum Cand<'h> {
    Exact(T<'h>, f64),
    Machine(f64),
    Inf,
}

/// How a cell was settled.
#[derive(Clone, Copy, Debug)]
enum Cell {
    /// f' of one sign (+1, -1) on [l, r]
    Mono(f64, f64, i8),
    /// one extremum inside [l, r] (its value among the candidates), the other extreme at an end
    Ext(f64, f64),
    /// lower and upper bounds (the last two) over [l, r] given directly
    Bounds(f64, f64, f64, f64),
    /// no enclosure on [l, r] at the smallest width
    Fail(f64, f64),
}

struct Ctx<'h> {
    var: T<'h>,
    g: T<'h>,
    dg: Option<T<'h>>,
    d2g: Option<T<'h>>,
    cells: usize,
    /// the expressions with their symbolic constants as balls
    constants_as_balls: HashMap<T<'h>, T<'h>>,
}

/// The rules that replace the symbolic constants of `e` by balls of 30 digits.
fn constant_rules<'h>(fx: &Fx<'_, '_, '_, 'h>, e: T<'h>) -> Vec<T<'h>> {
    ["pi", "euler_gamma", "catalan", "golden_ratio", "degree", "glaisher", "khinchin", "e"]
        .iter()
        .filter(|s| fx.contains(e, s))
        .map(|s| {
            let c = fx.sym(s);
            fx.call("rule", &[c, fx.call("n", &[c, fx.int(30)])])
        })
        .collect()
}

fn enc<'h>(fx: &mut Fx<'_, '_, '_, 'h>, cx: &mut Ctx<'h>, e: T<'h>, lo: f64, hi: f64) -> Res<Enc> {
    fx.tick(1)?;
    cx.cells += 1;
    let b = ball_term(fx, lo, hi);
    // the symbolic constants as balls (pi next to a ball stays a product: pi b^2 in the derivative of fresnel_s)
    let e = match cx.constants_as_balls.get(&e) {
        Some(&c) => c,
        None => {
            let rules = constant_rules(fx, e);
            let c = if rules.is_empty() { e } else { fx.eval(fx.call("rewrite", &[e, fx.list(&rules)]))? };
            cx.constants_as_balls.insert(e, c);
            c
        }
    };
    let mut v =fx.eval(fx.call("rewrite", &[e, fx.call("rule", &[cx.var, b])]))?;
    // a symbolic constant brought in by the function's own formula (catalan_number of a ball is a ball over
    // sqrt(pi)): the constants as balls in the value too
    if fx.num(v).is_none() {
        let rules = constant_rules(fx, v);
        if !rules.is_empty() {
            v = fx.eval(fx.call("rewrite", &[v, fx.list(&rules)]))?;
        }
    }
    Ok(match fx.num(v) {
        Some(n) => enc_of(&n),
        None => Enc::No,
    })
}

/// The sign an enclosure proves: 1 for >= 0, -1 for <= 0 (monotone, not strictly: enough for the image), 0 else.
fn sign(e: Enc) -> i8 {
    match e {
        Enc::Real(a, _) if a >= 0.0 => 1,
        Enc::Real(_, b) if b <= 0.0 => -1,
        _ => 0,
    }
}

/// A piece [lo, hi] (in units) where a function is monotone, with the sign.
type Piece = (i64, i64, i8);

/// Functions of one argument whose derivative in closed form cannot be enclosed at a point (a removable
/// singularity: sinc' = cos(x)/x - sin(x)/x^2 cancels to -x/3 at 0; a kink: abs' = sign(x) jumps at 0): the pieces
/// [lo, hi] (integer multiples of the unit, pi when the flag is set; `i64::MIN` and `i64::MAX` stand for -inf and inf)
/// where the function is known monotone, with the sign. sinc is increasing on [-pi, 0] and decreasing on [0, pi]: its
/// derivative is (x cos x - sin x)/x^2, and x cos x - sin x has the derivative -x sin x, of the opposite sign to x on
/// [-pi, pi]. abs and real_abs decrease on (-inf, 0] and increase on [0, inf).
const KNOWN_MONOTONE: &[(&str, bool, &[Piece])] = &[
    ("sinc", true, &[(-1, 0, 1), (0, 1, -1)]),
    ("abs", false, &[(i64::MIN, 0, -1), (0, i64::MAX, 1)]),
    ("real_abs", false, &[(i64::MIN, 0, -1), (0, i64::MAX, 1)]),
];

/// The image of [a, b] (finite ends, the argument `i` of `head(args)`): its pieces, `[[-inf, inf]]` when a pole lies
/// inside, `None` when it is not proved (the call then stays). The pieces of [a, b] where the function is known
/// monotone ([`KNOWN_MONOTONE`]) take their end values; the rest is settled by cells ([`image_by_cells`]).
pub(crate) fn image<'h>(
    fx: &mut Fx<'_, '_, '_, 'h>,
    head: T<'h>,
    args: &[T<'h>],
    i: usize,
    (a, b): (T<'h>, T<'h>),
) -> Res<Option<Vec<(T<'h>, T<'h>)>>> {
    let known = match fx.h.view(head) {
        mc_api::View::Sym(s) if args.len() == 1 => {
            let name = fx.h.sym_name(s);
            KNOWN_MONOTONE.iter().find(|(n, _, _)| *n == name).map(|(_, u, p)| (*u, *p))
        }
        _ => None,
    };
    let Some((by_pi, known)) = known else { return image_by_cells(fx, head, args, i, (a, b)) };
    let unit = if by_pi { fx.sym("pi") } else { fx.int(1) };
    let end = |fx: &mut Fx<'_, '_, '_, 'h>, k: i64| -> Res<T<'h>> {
        Ok(match k {
            i64::MIN => fx.neg(fx.inf()),
            i64::MAX => fx.inf(),
            _ => fx.eval(fx.mul(&[fx.int(k), unit]))?,
        })
    };
    let less = |fx: &mut Fx<'_, '_, '_, 'h>, x: T<'h>, y: T<'h>| -> Res<Option<bool>> {
        let v = fx.eval(fx.call("less", &[x, y]))?;
        Ok(if v == fx.bool(true) {
            Some(true)
        } else if v == fx.bool(false) {
            Some(false)
        } else {
            None
        })
    };
    let max_of = |fx: &mut Fx<'_, '_, '_, 'h>, x: T<'h>, y: T<'h>| -> Res<Option<T<'h>>> {
        Ok(less(fx, x, y)?.map(|lt| if lt { y } else { x }))
    };
    let min_of = |fx: &mut Fx<'_, '_, '_, 'h>, x: T<'h>, y: T<'h>| -> Res<Option<T<'h>>> {
        Ok(less(fx, x, y)?.map(|lt| if lt { x } else { y }))
    };
    let mut parts: Vec<(T<'h>, T<'h>, i8)> = vec![];
    for &(p, q, s) in known {
        let (p, q) = (end(fx, p)?, end(fx, q)?);
        let (Some(l), Some(r)) = (max_of(fx, a, p)?, min_of(fx, b, q)?) else { return Ok(None) };
        match less(fx, r, l)? {
            Some(false) => parts.push((l, r, s)),
            Some(true) => {}
            None => return Ok(None),
        }
    }
    // the rest of [a, b] outside the known pieces: below the first and above the last
    let (first, last) = (end(fx, known[0].0)?, end(fx, known[known.len() - 1].1)?);
    let mut rest = vec![];
    if less(fx, a, first)? != Some(false) {
        let Some(r) = min_of(fx, b, first)? else { return Ok(None) };
        rest.push((a, r));
    }
    if less(fx, last, b)? != Some(false) {
        let Some(l) = max_of(fx, a, last)? else { return Ok(None) };
        rest.push((l, b));
    }
    let mut lows = vec![];
    let mut highs = vec![];
    for (l, r, _) in parts {
        for x in [l, r] {
            let mut ys = args.to_vec();
            ys[i] = x;
            let v = fx.eval(fx.h.app(head, &ys))?;
            let Some(w) = approx(fx, v)? else { return Ok(None) };
            let c = if fx.num(v).is_none_or(|n| n.is_exact()) {
                Cand::Exact(v, w)
            } else {
                Cand::Machine(w)
            };
            let (lo_c, hi_c) = match c {
                Cand::Machine(w) => (Cand::Machine(w.next_down()), Cand::Machine(w.next_up())),
                c => (c, c),
            };
            lows.push(lo_c);
            highs.push(hi_c);
        }
    }
    for piece in rest {
        let Some(ps) = image_by_cells(fx, head, args, i, piece)? else { return Ok(None) };
        for (l, r) in ps {
            for (t, out) in [(l, &mut lows), (r, &mut highs)] {
                let c = if fx.infinity(t).is_some() {
                    Cand::Inf
                } else {
                    let Some(w) = approx(fx, t)? else { return Ok(None) };
                    if fx.num(t).is_none_or(|n| n.is_exact()) {
                        Cand::Exact(t, w)
                    } else {
                        Cand::Machine(w)
                    }
                };
                out.push(c);
            }
        }
    }
    let Some(lo_t) = pick(fx, &lows, false)? else { return Ok(None) };
    let Some(hi_t) = pick(fx, &highs, true)? else { return Ok(None) };
    Ok(Some(vec![(lo_t, hi_t)]))
}

/// The image of [a, b] settled by cells (the module comment).
fn image_by_cells<'h>(
    fx: &mut Fx<'_, '_, '_, 'h>,
    head: T<'h>,
    args: &[T<'h>],
    i: usize,
    ab: (T<'h>, T<'h>),
) -> Res<Option<Vec<(T<'h>, T<'h>)>>> {
    image_split(fx, head, args, i, ab, 0, (false, false))
}

/// At most this many poles inside an interval are split off.
const MAX_POLES: u32 = 8;

/// An exact point inside the cell [l, r] (a rational with a denominator up to 12, or such a multiple of pi), strictly
/// between the ends a and b, where the function is infinite: a pole found exactly.
fn exact_pole<'h>(
    fx: &mut Fx<'_, '_, '_, 'h>,
    head: T<'h>,
    args: &[T<'h>],
    i: usize,
    (a, b): (T<'h>, T<'h>),
    (l, r): (f64, f64),
) -> Res<Option<T<'h>>> {
    let w = (r - l).max(f64::EPSILON * l.abs().max(r.abs()));
    let mut seen: Vec<T<'h>> = vec![];
    for by_pi in [false, true] {
        let unit = if by_pi { std::f64::consts::PI } else { 1.0 };
        for d in 1..=12i64 {
            let n = ((l + r) * 0.5 * d as f64 / unit).round();
            if n.abs() > 1e12 {
                continue;
            }
            let q = n * unit / d as f64;
            if !(q >= l - w && q <= r + w) {
                continue;
            }
            let qt = fx.rat(n as i64, d);
            let qt = if by_pi { fx.eval(fx.mul(&[qt, fx.sym("pi")]))? } else { qt };
            if seen.contains(&qt) {
                continue;
            }
            seen.push(qt);
            let inside = |fx: &mut Fx<'_, '_, '_, 'h>, x: T<'h>, y: T<'h>| -> Res<bool> {
                Ok(fx.eval(fx.call("less", &[x, y]))? == fx.bool(true))
            };
            if !(inside(fx, a, qt)? && inside(fx, qt, b)?) {
                continue;
            }
            let mut ys = args.to_vec();
            ys[i] = qt;
            let v = fx.eval(fx.h.app(head, &ys))?;
            if infinite(fx, v)? || unevaluated_pole(fx, head, args, i, qt, v)? {
                return Ok(Some(qt));
            }
        }
    }
    Ok(None)
}

/// Is the value `v` of the function at the exact point `q` the call itself, left unevaluated at a pole (tan(pi/2),
/// whose `n` is a large finite number): |f| tends to infinity at q from both sides (by `limit`)?
fn unevaluated_pole<'h>(fx: &mut Fx<'_, '_, '_, 'h>, head: T<'h>, args: &[T<'h>], i: usize, q: T<'h>, v: T<'h>) -> Res<bool> {
    let mut ys = args.to_vec();
    ys[i] = q;
    if fx.num(v).is_some() || v != fx.h.app(head, &ys) {
        return Ok(false);
    }
    // only a value whose numeric approximation is huge is asked about (the limit is the proof)
    if approx(fx, v)?.is_some_and(|w| w.abs() < 1e10) {
        return Ok(false);
    }
    let x = fx.sym("interval·p");
    ys[i] = x;
    let f = fx.call("abs", &[fx.h.app(head, &ys)]);
    let l = fx.eval(fx.call("limit", &[f, fx.call("rule", &[x, q])]))?;
    Ok(fx.infinity(l) == Some(0))
}

fn image_split<'h>(
    fx: &mut Fx<'_, '_, '_, 'h>,
    head: T<'h>,
    args: &[T<'h>],
    i: usize,
    (a, b): (T<'h>, T<'h>),
    depth: u32,
    pole_ends: (bool, bool),
) -> Res<Option<Vec<(T<'h>, T<'h>)>>> {
    let (Some(fa), Some(fb)) = (approx(fx, a)?, approx(fx, b)?) else { return Ok(None) };
    if fa > fb {
        return Ok(None);
    }
    let lo = if exactly(fx, a, fa)? { fa } else { fa.next_down() };
    let hi = if exactly(fx, b, fb)? { fb } else { fb.next_up() };
    let at = |fx: &mut Fx<'_, '_, '_, 'h>, x: T<'h>| -> Res<T<'h>> {
        let mut ys = args.to_vec();
        ys[i] = x;
        fx.eval(fx.h.app(head, &ys))
    };
    // exact values at the ends only when every number of the call is exact (a machine number anywhere makes the
    // result machine)
    let machine_number = |fx: &Fx<'_, '_, '_, 'h>, t: T<'h>| fx.num(t).is_some_and(|n| !n.is_exact());
    let all_exact = !(machine_number(fx, a) || machine_number(fx, b) || args.iter().any(|&t| machine_number(fx, t)));
    let var = fx.sym("intervalÂ·t");
    // the other arguments: a machine number stands for its exact binary value (so that balls stay balls)
    let mut xs = args.to_vec();
    for (j, x) in xs.iter_mut().enumerate() {
        if j != i && machine_number(fx, *x) {
            *x = fx.eval(fx.call("set_precision", &[*x, fx.inf()]))?;
        }
    }
    xs[i] = var;
    let mut cx = Ctx { var, g: fx.h.app(head, &xs), dg: None, d2g: None, cells: 0, constants_as_balls: HashMap::new() };
    // no ball value at a point: the closed form of the function (binomial and catalan_number through gamma)
    let g0 = cx.g;
    if matches!(enc(fx, &mut cx, g0, lo, lo)?, Enc::No) {
        let e = fx.eval(fx.call("function_expand", &[cx.g]))?;
        if e != cx.g && matches!(enc(fx, &mut cx, e, lo, lo)?, Enc::Real(..)) {
            cx.g = e;
        }
    }
    let g = cx.g;
    let closed = |fx: &Fx<'_, '_, '_, 'h>, e: T<'h>| !(fx.contains(e, "derivative") || fx.contains(e, "d"));
    let mut dg = fx.eval(fx.call("d", &[g, var]))?;
    // no derivative in closed form: that of the function's closed form (binomial through gamma), if it has one
    if !closed(fx, dg) {
        let e = fx.eval(fx.call("function_expand", &[g]))?;
        if e != g {
            let de = fx.eval(fx.call("d", &[e, var]))?;
            if closed(fx, de) {
                dg = de;
            }
        }
    }
    cx.dg = closed(fx, dg).then_some(dg);
    cx.d2g = match cx.dg {
        Some(dg) => {
            let e = fx.eval(fx.call("d", &[dg, var]))?;
            closed(fx, e).then_some(e)
        }
        None => None,
    };
    // the values at the ends (exact for exact ends)
    // (an end split off at a pole found by its limit, as tan at pi/2, stands for the infinity there)
    // (so is an end where the function stays unevaluated at a pole)
    let mut ga = if pole_ends.0 { fx.sym("complex_infinity") } else { at(fx, a)? };
    let mut gb = if pole_ends.1 { fx.sym("complex_infinity") } else { at(fx, b)? };
    if unevaluated_pole(fx, head, args, i, a, ga)? {
        ga = fx.sym("complex_infinity");
    }
    if unevaluated_pole(fx, head, args, i, b, gb)? {
        gb = fx.sym("complex_infinity");
    }
    // the values at the ends must be real numbers or infinite (a pole): otherwise the function is not a real
    // numeric function there (a symbolic parameter, a complex value) and nothing is claimed
    for v in [ga, gb] {
        if approx(fx, v)?.is_none() && !infinite(fx, v)? {
            return Ok(None);
        }
    }
    // a value infinite at an end (a pole there): only the cells settle it (branch and bound has no infinite bound)
    let pole_end = infinite(fx, ga)? || infinite(fx, gb)?;
    if lo == hi {
        // a point: its exact value, else the point ball
        if all_exact && approx(fx, ga)?.is_some() && fx.num(ga).is_none_or(|n| n.is_exact()) {
            return Ok(Some(vec![(ga, ga)]));
        }
        return Ok(match enc(fx, &mut cx, g, lo, lo)? {
            Enc::Real(l, r) => Some(vec![(fx.real(l), fx.real(r))]),
            _ => None,
        });
    }
    // no derivative in closed form: the least and greatest values by branch and bound on the values
    // (a pole at an end leaves the cells next to it without a bound, so that end of the image is infinite)
    if cx.dg.is_none() {
        return by_bounds(fx, &mut cx, lo, hi);
    }
    // the cells, left to right
    let mut cells: Vec<Cell> = vec![];
    let mut lows: Vec<Cand<'h>> = vec![];
    let mut highs: Vec<Cand<'h>> = vec![];
    let mut stack = vec![(lo, hi, 0u32)];
    let tiny = (hi - lo) * 0.5f64.powi(MAX_DEPTH as i32);
    while let Some((l, r, depth)) = stack.pop() {
        if cx.cells > MAX_CELLS {
            // the derivative does not settle the cells (cancellation in its closed form): the values alone
            cx.cells = 0;
            return if pole_end { Ok(None) } else { by_bounds(fx, &mut cx, lo, hi) };
        }
        let (s, has_d) = match cx.dg {
            Some(dg) => {
                let d = enc(fx, &mut cx, dg, l, r)?;
                if d == Enc::Complex {
                    return Ok(None);
                }
                (sign(d), d != Enc::No)
            }
            None => (0, false),
        };
        if s != 0 {
            cells.push(Cell::Mono(l, r, s));
            continue;
        }
        // f'' only where f' has an enclosure (next to a pole neither has)
        if let Some(d2g) = cx.d2g.filter(|_| has_d) {
            let d2 = enc(fx, &mut cx, d2g, l, r)?;
            let s2 = sign(d2);
            if s2 != 0 {
                if let Some(c) = one_extremum(fx, &mut cx, (l, r), s2, &mut lows, &mut highs)? {
                    cells.push(c);
                    continue;
                }
            }
        }
        let m = 0.5 * (l + r);
        if depth >= MAX_DEPTH || r - l <= tiny || m <= l || m >= r {
            match enc(fx, &mut cx, g, l, r)? {
                Enc::Real(vl, vr) => cells.push(Cell::Bounds(l, r, vl, vr)),
                Enc::Complex => return Ok(None),
                Enc::No => cells.push(Cell::Fail(l, r)),
            }
            continue;
        }
        // the right half below the left one: the left is settled first
        stack.push((m, r, depth + 1));
        stack.push((l, m, depth + 1));
    }
    // monotone runs of one sign merge; their ends are the points where values are needed
    let mut runs: Vec<Cell> = vec![];
    for c in cells {
        if let (Some(Cell::Mono(l0, _, s0)), Cell::Mono(_, r1, s1)) = (runs.last().copied(), c) {
            if s0 == s1 && s0 != 0 {
                *runs.last_mut().expect("a run") = Cell::Mono(l0, r1, s0);
                continue;
            }
        }
        runs.push(c);
    }
    // the value at a point: exact at the ends of [a, b], else a point ball
    let mut memo: HashMap<u64, (Cand<'h>, Cand<'h>)> = HashMap::new();
    let ends_exact = [(lo, a, ga), (hi, b, gb)];
    let mut point = |fx: &mut Fx<'_, '_, '_, 'h>, cx: &mut Ctx<'h>, x: f64| -> Res<Option<(Cand<'h>, Cand<'h>)>> {
        if let Some(&v) = memo.get(&x.to_bits()) {
            return Ok(Some(v));
        }
        for &(e, t, gt) in &ends_exact {
            if all_exact && x == e && fx.num(t).is_none_or(|n| n.is_exact()) {
                if let Some(v) = approx(fx, gt)? {
                    if fx.num(gt).is_none_or(|n| n.is_exact()) {
                        let gt = simplest(fx, gt)?;
                        let c = (Cand::Exact(gt, v), Cand::Exact(gt, v));
                        memo.insert(x.to_bits(), c);
                        return Ok(Some(c));
                    }
                }
            }
        }
        let g = cx.g;
        // a point with a short binary fraction (0, 1/2, 3/4, ...): its exact value when that is a rational number
        let scaled = x * 1024.0;
        if scaled == scaled.round() && scaled.abs() < 1e9 {
            let q = fx.rat(scaled as i64, 1024);
            let v = fx.eval(fx.call("rewrite", &[g, fx.call("rule", &[cx.var, q])]))?;
            if let Some(Num::Real(Real::Exact(r))) = fx.num(v) {
                let w = Real::Exact(r).to_f64();
                let c = (Cand::Exact(v, w), Cand::Exact(v, w));
                memo.insert(x.to_bits(), c);
                return Ok(Some(c));
            }
        }
        let c = match enc(fx, cx, g, x, x)? {
            Enc::Real(l, r) => (Cand::Machine(l), Cand::Machine(r)),
            _ => return Ok(None),
        };
        memo.insert(x.to_bits(), c);
        Ok(Some(c))
    };
    // a cell without an enclosure inside the interval at an exact pole: the interval splits there, and the image is
    // the union of the images of the two sides (each with the pole at an end: acsch over [-4, csch 2] is the two rays
    // (-inf, acsch(-4)] and [2, inf))
    if depth < MAX_POLES {
        let fails: Vec<(f64, f64)> = runs.iter().filter_map(|c| if let Cell::Fail(l, r) = *c { Some((l, r)) } else { None }).collect();
        for (l, r) in fails {
            if let Some(q) = exact_pole(fx, head, args, i, (a, b), (l, r))? {
                let Some(mut left) = image_split(fx, head, args, i, (a, q), depth + 1, (pole_ends.0, true))? else { return Ok(None) };
                let Some(right) = image_split(fx, head, args, i, (q, b), depth + 1, (true, pole_ends.1))? else { return Ok(None) };
                left.extend(right);
                return Ok(Some(left));
            }
        }
    }
    // a cell without an enclosure is a singularity only when f has enclosures on narrow balls elsewhere (else the
    // function has no ball form there, and nothing is claimed)
    let mut ball_form = true;
    if runs.iter().any(|c| matches!(c, Cell::Fail(..))) {
        ball_form = runs.iter().any(|c| matches!(c, Cell::Bounds(..)));
        if !ball_form {
            if let Some(&Cell::Mono(l, r, _) | &Cell::Ext(l, r)) = runs.iter().find(|c| matches!(c, Cell::Mono(..) | Cell::Ext(..))) {
                let g = cx.g;
                ball_form = matches!(enc(fx, &mut cx, g, l, l + (r - l).min(4.0 * tiny))?, Enc::Real(..));
            }
        }
    }
    let line = |fx: &Fx<'_, '_, '_, 'h>| Ok(ball_form.then(|| vec![(fx.neg(fx.inf()), fx.inf())]));
    // a singular end: the cells at the end that are not monotone (one with no enclosure, and the loose enclosures
    // next to it, together at most 64 smallest widths) are taken as monotone towards the end, with the sign of the
    // first monotone run after them
    let n = runs.len();
    // an end where the value is infinite must have given no enclosure next to it; a finite enclosure there came from
    // a ball form that is not valid up to the singularity, and nothing is claimed
    if infinite(fx, ga)? && !matches!(runs.first(), Some(Cell::Fail(..))) || infinite(fx, gb)? && !matches!(runs.last(), Some(Cell::Fail(..))) {
        return Ok(None);
    }
    let reach = 64.0 * tiny;
    let mut first = 0;
    let mut last = n;
    let mut ends: Vec<(f64, T<'h>, i8, bool)> = vec![];
    if matches!(runs.first(), Some(Cell::Fail(l, _)) if *l == lo) {
        let mut j = 0;
        while j < n && !matches!(runs[j], Cell::Mono(..)) {
            match runs[j] {
                Cell::Fail(_, r) | Cell::Bounds(_, r, _, _) | Cell::Ext(_, r) if r - lo > reach => return line(fx),
                _ => {}
            }
            j += 1;
        }
        let Some(&Cell::Mono(inner, _, s)) = runs.get(j) else { return line(fx) };
        ends.push((inner, ga, s, true));
        first = j;
    }
    if matches!(runs.last(), Some(Cell::Fail(_, r)) if *r == hi) && last > first {
        let mut j = n;
        while j > first && !matches!(runs[j - 1], Cell::Mono(..)) {
            if let Cell::Fail(l, _) | Cell::Bounds(l, _, _, _) | Cell::Ext(l, _) = runs[j - 1] {
                if hi - l > reach {
                    return line(fx);
                }
            }
            j -= 1;
        }
        let Some(&Cell::Mono(_, inner, s)) = runs.get(j.wrapping_sub(1)).filter(|_| j > first) else { return line(fx) };
        ends.push((inner, gb, s, false));
        last = j;
    }
    for run in &runs[first..last] {
        match *run {
            Cell::Mono(l, r, _) | Cell::Ext(l, r) => {
                for x in [l, r] {
                    let Some((p, q)) = point(fx, &mut cx, x)? else { return Ok(None) };
                    lows.push(p);
                    highs.push(q);
                }
            }
            Cell::Bounds(_, _, vl, vr) => {
                lows.push(Cand::Machine(vl));
                highs.push(Cand::Machine(vr));
            }
            // no enclosure inside (a pole): the whole line
            Cell::Fail(..) => return line(fx),
        }
    }
    for (inner, end_value, s, at_lo) in ends {
        let Some((p, q)) = point(fx, &mut cx, inner)? else { return Ok(None) };
        lows.push(p);
        highs.push(q);
        // towards the end the values rise when f decreases to the right of a left end (or increases to the left of a
        // right end)
        let rising = if at_lo { s < 0 } else { s > 0 };
        if infinite(fx, end_value)? {
            if rising {
                highs.push(Cand::Inf);
            } else {
                lows.push(Cand::Inf);
            }
            continue;
        }
        // a finite end value (a branch point) on the side the monotone run leads to
        let Some(v) = approx(fx, end_value)? else { return line(fx) };
        let inner_v = match if rising { q } else { p } {
            Cand::Exact(_, w) | Cand::Machine(w) => w,
            Cand::Inf => return Ok(None),
        };
        if rising && v < inner_v || !rising && v > inner_v {
            return line(fx);
        }
        // the value at the end itself (exact, or a point ball)
        let Some((p2, q2)) = point(fx, &mut cx, if at_lo { lo } else { hi })? else { return line(fx) };
        lows.push(p2);
        highs.push(q2);
    }
    let Some(lo_t) = pick(fx, &lows, false)? else { return Ok(None) };
    let Some(hi_t) = pick(fx, &highs, true)? else { return Ok(None) };
    Ok(Some(vec![(lo_t, hi_t)]))
}

/// The least and greatest values of f over [lo, hi] by branch and bound on enclosures of f over cells (no
/// derivative needed; Moore, Kearfott and Cloud, chapter 6.1, the Moore-Skelboe algorithm): the cell with the least
/// lower bound is split until that bound is within 2^-44 (relative) of a value attained at a point, so the result's
/// lower end is the least lower bound of the remaining cells (and alike for the upper end). A cell of the smallest
/// width without an enclosure makes that end infinite; complex values give `None`. After [`BOUNDS_CELLS`]
/// enclosures for one end, that end is the least lower bound (greatest upper bound) of the cells left, infinite when
/// one of them has none.
fn by_bounds<'h>(fx: &mut Fx<'_, '_, '_, 'h>, cx: &mut Ctx<'h>, lo: f64, hi: f64) -> Res<Option<Vec<(T<'h>, T<'h>)>>> {
    let g = cx.g;
    let tiny = (hi - lo) * 0.5f64.powi(MAX_DEPTH as i32);
    let mut ends = [0.0f64; 2];
    for (k, max) in [false, true].into_iter().enumerate() {
        // the best value attained so far (at points), and the cells with their bounds
        let mut best = if max { f64::NEG_INFINITY } else { f64::INFINITY };
        for x in [lo, hi] {
            match enc(fx, cx, g, x, x)? {
                Enc::Real(a, b) => best = if max { best.max(a) } else { best.min(b) },
                Enc::Complex => return Ok(None),
                Enc::No => {}
            }
        }
        let bound = |e: Enc| match e {
            Enc::Real(a, b) => Some(if max { b } else { a }),
            _ => None,
        };
        let first = enc(fx, cx, g, lo, hi)?;
        if first == Enc::Complex {
            return Ok(None);
        }
        let mut cells: Vec<(f64, f64, Option<f64>)> = vec![(lo, hi, bound(first))];
        let start = cx.cells;
        loop {
            // the budget spent: the least lower bound of the cells is proved (every cell has a bound), only looser
            // than asked (a ball form whose enclosures shrink only in proportion to the width of the cell)
            if cx.cells - start > BOUNDS_CELLS {
                let mut v = if max { f64::NEG_INFINITY } else { f64::INFINITY };
                // (a cell still without a bound, next to a pole, makes that end infinite)
                for c in &cells {
                    v = match c.2 {
                        Some(b) if max => v.max(b),
                        Some(b) => v.min(b),
                        None if max => f64::INFINITY,
                        None => f64::NEG_INFINITY,
                    };
                    if v.is_infinite() && c.2.is_none() {
                        break;
                    }
                }
                ends[k] = v;
                break;
            }
            // the cell with the extreme bound (no bound counts as infinite)
            let key = |c: &(f64, f64, Option<f64>)| match c.2 {
                Some(v) => if max { -v } else { v },
                None => f64::NEG_INFINITY,
            };
            let Some(j) = (0..cells.len()).min_by(|&p, &q| key(&cells[p]).total_cmp(&key(&cells[q]))) else { return Ok(None) };
            let (l, r, bd) = cells[j];
            // no enclosure on any cell after a few splits: the function has no ball form here
            if cells.len() > 8 && !cells.iter().any(|c| c.2.is_some()) {
                return Ok(None);
            }
            let tol = 2f64.powi(-44) * best.abs().max(f64::MIN_POSITIVE);
            match bd {
                Some(v) if (max && v <= best + tol) || (!max && v >= best - tol) => {
                    ends[k] = v;
                    break;
                }
                None if r - l <= tiny => {
                    // a singularity only when the function has enclosures elsewhere (else it has no ball form here)
                    if !cells.iter().any(|c| c.2.is_some()) {
                        return Ok(None);
                    }
                    ends[k] = if max { f64::INFINITY } else { f64::NEG_INFINITY };
                    break;
                }
                _ => {}
            }
            let m = 0.5 * (l + r);
            if m <= l || m >= r {
                match bd {
                    Some(v) => {
                        ends[k] = v;
                        break;
                    }
                    None => return Ok(None),
                }
            }
            cells.swap_remove(j);
            if let Enc::Real(a, b) = enc(fx, cx, g, m, m)? {
                best = if max { best.max(a) } else { best.min(b) };
            }
            for (p, q) in [(l, m), (m, r)] {
                let e = enc(fx, cx, g, p, q)?;
                if e == Enc::Complex {
                    return Ok(None);
                }
                cells.push((p, q, bound(e)));
            }
            fx.tick(1)?;
        }
    }
    let term = |fx: &Fx<'_, '_, '_, 'h>, v: f64| {
        if v == f64::INFINITY {
            fx.inf()
        } else if v == f64::NEG_INFINITY {
            fx.neg(fx.inf())
        } else {
            fx.real(v)
        }
    };
    Ok(Some(vec![(term(fx, ends[0]), term(fx, ends[1]))]))
}

/// An exact end value in its simplest form (`full_simplify` when that gives a plain number, as csch(log(3)) = 3/4).
fn simplest<'h>(fx: &mut Fx<'_, '_, '_, 'h>, t: T<'h>) -> Res<T<'h>> {
    if fx.num(t).is_some() {
        return Ok(t);
    }
    let s = fx.eval(fx.call("full_simplify", &[t]))?;
    Ok(if fx.num(s).is_some() { s } else { t })
}

/// The least (`max` false) or greatest candidate as a term: an exact one when it is proved beyond every machine bound,
/// else a machine bound (exact candidates entering rounded outwards).
fn pick<'h>(fx: &mut Fx<'_, '_, '_, 'h>, cs: &[Cand<'h>], max: bool) -> Res<Option<T<'h>>> {
    if cs.iter().any(|c| matches!(c, Cand::Inf)) {
        return Ok(Some(if max { fx.inf() } else { fx.neg(fx.inf()) }));
    }
    let better = |x: f64, y: f64| if max { x > y } else { x < y };
    let mut machine: Option<f64> = None;
    let mut exact: Option<(T<'h>, f64)> = None;
    for c in cs {
        match *c {
            Cand::Machine(v) => {
                if machine.is_none_or(|m| better(v, m)) {
                    machine = Some(v);
                }
            }
            Cand::Exact(t, v) => {
                let replace = match exact {
                    None => true,
                    Some((u, w)) if u != t => {
                        if better(v, w) {
                            true
                        } else if better(w, v) {
                            false
                        } else {
                            // close: decided exactly
                            let rel = if max { "greater" } else { "less" };
                            match fx.eval(fx.call(rel, &[t, u]))? {
                                x if x == fx.bool(true) => true,
                                x if x == fx.bool(false) => false,
                                _ => {
                                    let w2 = if max { v.max(w).next_up() } else { v.min(w).next_down() };
                                    machine = Some(machine.map_or(w2, |m| if better(w2, m) { w2 } else { m }));
                                    false
                                }
                            }
                        }
                    }
                    _ => false,
                };
                if replace {
                    exact = Some((t, v));
                }
            }
            Cand::Inf => {}
        }
    }
    Ok(match (exact, machine) {
        (None, None) => None,
        (None, Some(m)) => Some(fx.real(m)),
        (Some((t, _)), None) => Some(t),
        (Some((t, v)), Some(m)) => {
            // the exact candidate stands when it is proved at or beyond the machine bound
            let rel = if max { "greater_equal" } else { "less_equal" };
            let proved = better(v, m) && fx.eval(fx.call(rel, &[t, fx.real(m)]))? == fx.bool(true);
            if proved {
                Some(t)
            } else {
                let w = if max { v.next_up() } else { v.next_down() };
                Some(fx.real(if better(w, m) { w } else { m }))
            }
        }
    })
}

/// A cell [l, r] on which f'' has the sign `s2` (f' strictly monotone): monotone when f' keeps one sign at the ends,
/// else the extremum inside, bracketed by bisection on proved signs of f'. Pushes the extreme value to `cands` (a low
/// at an even place for a convex f, a high at an odd place for a concave one) and returns the cell (with the ends
/// standing for the far extreme), or `None` when the signs at the ends are not decided.
fn one_extremum<'h>(
    fx: &mut Fx<'_, '_, '_, 'h>,
    cx: &mut Ctx<'h>,
    (l, r): (f64, f64),
    s2: i8,
    lows: &mut Vec<Cand<'h>>,
    highs: &mut Vec<Cand<'h>>,
) -> Res<Option<Cell>> {
    let Some(dg) = cx.dg else { return Ok(None) };
    let dl = enc(fx, cx, dg, l, l)?;
    let dr = enc(fx, cx, dg, r, r)?;
    // f' increasing (convex): f' >= 0 at l means f' > 0 inside; f' <= 0 at r means f' < 0 inside
    let nonneg = |e: Enc| matches!(e, Enc::Real(a, _) if a >= 0.0);
    let nonpos = |e: Enc| matches!(e, Enc::Real(_, b) if b <= 0.0);
    if s2 > 0 && nonneg(dl) || s2 < 0 && nonpos(dl) {
        return Ok(Some(Cell::Mono(l, r, if s2 > 0 { 1 } else { -1 })));
    }
    if s2 > 0 && nonpos(dr) || s2 < 0 && nonneg(dr) {
        return Ok(Some(Cell::Mono(l, r, if s2 > 0 { -1 } else { 1 })));
    }
    // the bracket of the zero of f': for a convex f, f' < 0 left of xl and > 0 right of xr (f' increasing), so the
    // least value over the cell is the least over [xl, xr]
    let (mut xl, mut xr) = (l, r);
    for _ in 0..80 {
        let m = 0.5 * (xl + xr);
        if m <= xl || m >= xr {
            break;
        }
        let sm = sign(enc(fx, cx, dg, m, m)?);
        // convex: f'(m) < 0 moves the left end; concave: f'(m) > 0 does
        if sm == -s2 {
            xl = m;
        } else if sm == s2 {
            xr = m;
        } else {
            break;
        }
    }
    // the extreme value: exact at a simple rational zero of f'
    let mut exact = None;
    let mid = 0.5 * (xl + xr);
    for d in 1..=12i64 {
        let n = (mid * d as f64).round();
        if n.abs() > 1e15 {
            break;
        }
        let q = n / d as f64;
        if !(q >= xl && q <= xr) {
            continue;
        }
        let qt = fx.rat(n as i64, d);
        let dq = fx.eval(fx.call("rewrite", &[dg, fx.call("rule", &[cx.var, qt])]))?;
        if fx.is_int(dq, 0) {
            let v = fx.eval(fx.call("rewrite", &[cx.g, fx.call("rule", &[cx.var, qt])]))?;
            if let Some(w) = approx(fx, v)? {
                if fx.num(v).is_none_or(|n| n.is_exact()) {
                    exact = Some(Cand::Exact(v, w));
                    break;
                }
            }
        }
    }
    let g = cx.g;
    let ext = match exact {
        Some(c) => c,
        None => {
            // f on the bracket's ball only when the bracket is narrow (a wide ball is slow for some special functions,
            // and the mean value form below is tighter anyway)
            let (a, b) = if xr - xl <= (r - l) * 2f64.powi(-20) {
                match enc(fx, cx, g, xl, xr)? {
                    Enc::Real(a, b) => (a, b),
                    _ => return Ok(None),
                }
            } else {
                (f64::NEG_INFINITY, f64::INFINITY)
            };
            // the mean value form f(m) + f'([xl, xr]) [xl - m, xr - m], tight where f' is small (Moore, Kearfott and
            // Cloud, section 6.4), met with the plain enclosure
            let m = 0.5 * (xl + xr);
            let h = (xr - m).max(m - xl);
            let mvf = match (enc(fx, cx, g, m, m)?, enc(fx, cx, dg, xl, xr)?) {
                (Enc::Real(p, q), Enc::Real(dl, dh)) => {
                    let spread = (dl.abs().max(dh.abs()) * h) * (1.0 + 4.0 * f64::EPSILON);
                    Some(((p - spread).next_down(), (q + spread).next_up()))
                }
                _ => None,
            };
            let (a, b) = match mvf {
                Some((p, q)) => (a.max(p), b.min(q)),
                None => (a, b),
            };
            if !(a.is_finite() && b.is_finite()) {
                return Ok(None);
            }
            Cand::Machine(if s2 > 0 { a } else { b })
        }
    };
    // the near extreme: a low of a convex f, a high of a concave one; the far one is at the ends of the cell
    if s2 > 0 {
        lows.push(ext);
    } else {
        highs.push(ext);
    }
    Ok(Some(Cell::Ext(l, r)))
}
