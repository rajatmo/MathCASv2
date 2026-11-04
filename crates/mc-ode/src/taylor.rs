//! The Taylor series method on balls, the method under `digits:` (decision NUM.4; Jorba and Zou, *A software
//! package for the numerical integration of ODEs by means of high-order Taylor methods*, 2005).
//!
//! The right-hand side is read once into a **tape** of elementary operations on Taylor series (`+`, `*`, `/`,
//! powers, `exp`, `log`, `sin`/`cos`, `sinh`/`cosh`, `sqrt`, `atan`); the coefficients of every node follow from
//! automatic-differentiation recurrences, order by order, for example
//!
//! ```text
//! (a b)_k = sum_{j=0..k} a_j b_{k-j}        exp: e_k = (1/k) sum_{j=1..k} j a_j e_{k-j}
//! y' = F(t, y):  y_{k+1} = F(y)_k / (k + 1)
//! ```
//!
//! A step of order K = ceil(ln(10^g) / 2) + 1 for a tolerance of g digits has the size `ρ / e²` where ρ estimates
//! the radius of convergence from the last two coefficients (Jorba–Zou), so the local error is about 10^-g. The
//! arithmetic is ball arithmetic at the working precision; every step restarts from the midpoints (the radii
//! would grow by the dependency effect), and the error is estimated by a second integration (decision N3.10).
use crate::util::{self, mentions};
use mc_api::names;
use mc_arith::num::float::f64_to_dyadic;
use mc_arith::num::{Ball, Float, Int, Mag, Rat};
use mc_arith::{Arith, Kind, Num, Real};
use mc_core::{Heap, Store, Sym, View, T};
use mc_eval::{NativeCx, Res};
use mc_numeric::elementary as el;
use std::collections::HashMap;

/// One operation of the tape; operands are earlier nodes.
#[derive(Clone, Debug)]
enum Op {
    Const(Ball),
    Time,
    State(usize),
    Add(Vec<usize>),
    Mul(usize, usize),
    Div(usize, usize),
    Exp(usize),
    Log(usize),
    /// sin of the operand; the node of its cos
    Sin(usize, usize),
    /// cos of the operand; the node of its sin
    Cos(usize, usize),
    /// sinh and cosh, likewise
    Sinh(usize, usize),
    Cosh(usize, usize),
    Sqrt(usize),
    /// a^c for a constant c that is not a small integer
    Pow(usize, Ball),
    /// atan(a), with the node of 1 + a^2
    Atan(usize, usize),
}

/// The right-hand side as a tape: the nodes and the node of each component.
pub(crate) struct Tape {
    ops: Vec<Op>,
    outputs: Vec<usize>,
    /// the working precision in bits
    p: u64,
}

/// Why the right-hand side has no tape.
pub(crate) type Unsupported = String;

/// The tape builder: a node per distinct subterm.
struct Builder {
    ops: Vec<Op>,
    one: Option<usize>,
}

impl Builder {
    fn push(&mut self, op: Op) -> usize {
        self.ops.push(op);
        self.ops.len() - 1
    }

    fn constant(&mut self, b: Ball) -> usize {
        self.push(Op::Const(b))
    }

    fn one(&mut self) -> usize {
        match self.one {
            Some(k) => k,
            None => {
                let k = self.constant(Ball::from_i64(1));
                self.one = Some(k);
                k
            }
        }
    }

    /// a^k for an integer k by binary powering.
    fn int_pow(&mut self, a: usize, k: i64) -> usize {
        if k == 0 {
            return self.one();
        }
        let mut e = k.unsigned_abs();
        let mut base = a;
        let mut acc: Option<usize> = None;
        while e > 0 {
            if e & 1 == 1 {
                acc = Some(match acc {
                    None => base,
                    Some(x) => self.push(Op::Mul(x, base)),
                });
            }
            e >>= 1;
            if e > 0 {
                base = self.push(Op::Mul(base, base));
            }
        }
        let r = acc.expect("k != 0");
        if k < 0 {
            let one = self.one();
            self.push(Op::Div(one, r))
        } else {
            r
        }
    }
}

/// A number as a ball at `p` bits.
fn ball_of(n: &Num, p: u64) -> Option<Ball> {
    match n.as_real()? {
        // a machine real is a dyadic number: exactly that ball
        Real::Machine(x) => x.is_finite().then(|| ball_f64(*x)),
        r => match r.to_kind(Kind::Ball, p) {
            Real::Ball(b, _) => Some(b),
            _ => None,
        },
    }
}

/// The exact ball of a machine real.
pub(crate) fn ball_f64(x: f64) -> Ball {
    match f64_to_dyadic(x) {
        Some((neg, man, exp)) => Ball::exact(Float::new(neg, man, exp)),
        None => Ball::from_i64(0),
    }
}

/// The digits of the working precision for `bits` bits, for `n(_, digits)`.
pub(crate) fn digits_of(bits: u64) -> u64 {
    (bits as f64 * std::f64::consts::LOG10_2).ceil() as u64 + 2
}

/// Read the right-hand side `rhs` (a list of n expressions in `t` and the state symbols) into a tape at `p` bits.
/// Subexpressions free of the variables are constants, evaluated by `n` to the working precision.
pub(crate) fn tape<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    ar: &Arith<'h, Heap>,
    rhs: T<'h>,
    t: Sym,
    state: &[Sym],
    p: u64,
) -> Res<Result<Tape, Unsupported>> {
    let h = cx.heap();
    let Some(items) = cx.as_list(rhs) else { return Ok(Err("the right-hand side is not a list".into())) };
    let mut b = Builder { ops: vec![], one: None };
    let mut vars = state.to_vec();
    vars.push(t);
    let mut memo: HashMap<T<'h>, usize> = HashMap::new();
    let name = |s: &str| h.sym_term(h.sym(s));
    let (add, mul, pow) = (name(names::ADD), name(names::MUL), name(names::POW));
    let mut outputs = Vec::with_capacity(items.len());
    for &root in items {
        // post-order on an explicit stack: (term, children pushed)
        let mut stack = vec![(root, false)];
        while let Some((u, ready)) = stack.pop() {
            if memo.contains_key(&u) {
                continue;
            }
            cx.tick(1)?;
            let view = h.view(u);
            // constants: numbers and closed subterms
            if let View::Sym(s) = view {
                if s == t {
                    let k = b.push(Op::Time);
                    memo.insert(u, k);
                    continue;
                }
                if let Some(i) = state.iter().position(|&x| x == s) {
                    let k = b.push(Op::State(i));
                    memo.insert(u, k);
                    continue;
                }
            }
            if !mentions(cx, u, &vars)? {
                let Some(v) = util::number(cx, ar, u, Some(digits_of(p)))? else {
                    return Ok(Err(format!("{} is not numeric", mc_core_print(h, u))));
                };
                let Some(c) = ball_of(&v, p) else { return Ok(Err("a complex constant".into())) };
                let k = b.constant(c);
                memo.insert(u, k);
                continue;
            }
            let View::App { head, args } = view else { return Ok(Err("an unsupported term".into())) };
            if !ready {
                stack.push((u, true));
                // the exponent of a power is read as a number, not as a node
                let children: &[T<'h>] = if head == pow && args.len() == 2 { &args[..1] } else { args };
                stack.extend(children.iter().map(|&a| (a, false)));
                if head == pow && args.len() == 2 && mentions(cx, args[1], &vars)? {
                    stack.push((args[1], false));
                }
                continue;
            }
            let node = |m: &HashMap<T<'h>, usize>, a: T<'h>| m[&a];
            let k = if head == add {
                let xs = args.iter().map(|&a| node(&memo, a)).collect();
                b.push(Op::Add(xs))
            } else if head == mul {
                let mut acc = node(&memo, args[0]);
                for &a in &args[1..] {
                    let x = node(&memo, a);
                    acc = b.push(Op::Mul(acc, x));
                }
                acc
            } else if head == pow && args.len() == 2 {
                let a = node(&memo, args[0]);
                match memo.get(&args[1]) {
                    // a variable exponent: exp(e log a)
                    Some(&e) => {
                        let l = b.push(Op::Log(a));
                        let m = b.push(Op::Mul(e, l));
                        b.push(Op::Exp(m))
                    }
                    None => {
                        let Some(e) = util::number(cx, ar, args[1], Some(digits_of(p)))? else {
                            return Ok(Err("a power with a non-numeric exponent".into()));
                        };
                        match exponent(&e) {
                            Exponent::Int(k) => b.int_pow(a, k),
                            Exponent::Half => b.push(Op::Sqrt(a)),
                            Exponent::Real => match ball_of(&e, p) {
                                Some(c) => b.push(Op::Pow(a, c)),
                                None => return Ok(Err("a complex exponent".into())),
                            },
                        }
                    }
                }
            } else {
                let View::Sym(f) = h.view(head) else { return Ok(Err("an unsupported function".into())) };
                let fname = h.sym_name(f);
                let &[a] = args else { return Ok(Err(format!("{fname} with {} arguments", args.len()))) };
                let a = node(&memo, a);
                match fname.as_str() {
                    "exp" => b.push(Op::Exp(a)),
                    "log" => b.push(Op::Log(a)),
                    "sqrt" => b.push(Op::Sqrt(a)),
                    "sin" | "cos" | "tan" => {
                        let s = b.push(Op::Sin(a, 0));
                        let c = b.push(Op::Cos(a, s));
                        b.ops[s] = Op::Sin(a, c);
                        match fname.as_str() {
                            "sin" => s,
                            "cos" => c,
                            _ => b.push(Op::Div(s, c)),
                        }
                    }
                    "sinh" | "cosh" | "tanh" => {
                        let s = b.push(Op::Sinh(a, 0));
                        let c = b.push(Op::Cosh(a, s));
                        b.ops[s] = Op::Sinh(a, c);
                        match fname.as_str() {
                            "sinh" => s,
                            "cosh" => c,
                            _ => b.push(Op::Div(s, c)),
                        }
                    }
                    "atan" => {
                        let sq = b.push(Op::Mul(a, a));
                        let one = b.one();
                        let w = b.push(Op::Add(vec![one, sq]));
                        b.push(Op::Atan(a, w))
                    }
                    _ => return Ok(Err(format!("the Taylor method does not support {fname}"))),
                }
            };
            memo.insert(u, k);
        }
        outputs.push(memo[&root]);
    }
    Ok(Ok(Tape { ops: b.ops, outputs, p }))
}

/// A short printed form of a term for messages.
fn mc_core_print(h: &Heap, t: T<'_>) -> String {
    match h.view(t) {
        View::Sym(s) => h.sym_name(s),
        View::App { head, .. } => match h.view(head) {
            View::Sym(s) => format!("{}(...)", h.sym_name(s)),
            _ => "a term".into(),
        },
        _ => "a constant".into(),
    }
}

enum Exponent {
    Int(i64),
    Half,
    Real,
}

/// The kind of a constant exponent: a small exact (or exactly integral) integer, exactly 1/2, or any other.
fn exponent(e: &Num) -> Exponent {
    if let Some(i) = e.as_int().and_then(|i| i.to_i64()) {
        if i.abs() <= 1 << 20 {
            return Exponent::Int(i);
        }
    }
    let half = Num::rat(Rat::new(Int::from_i64(1), Int::from_i64(2)));
    if e.as_rat().is_some() && mc_arith::number::equal(e, &half) == Some(true) {
        return Exponent::Half;
    }
    Exponent::Real
}

/// The Taylor coefficients of every node up to some order, filled order by order.
struct Series {
    c: Vec<Vec<Ball>>,
}

fn zero() -> Ball {
    Ball::from_i64(0)
}

/// sum_{j=lo..=hi} w(j) a_j b_{k-j}
fn conv(a: &[Ball], b: &[Ball], k: usize, lo: usize, hi: usize, w: impl Fn(usize) -> i64, p: u64) -> Ball {
    let mut s = zero();
    for j in lo..=hi {
        let f = w(j);
        if f == 0 {
            continue;
        }
        let mut term = a[j].mul(&b[k - j], p);
        if f != 1 {
            term = term.mul(&Ball::from_i64(f), p);
        }
        s = s.add(&term, p);
    }
    s
}

impl Tape {
    /// The coefficient of order `k` of every node, given the state's coefficients up to `k` (in `series` for the
    /// `State` nodes) and the time `t` at the expansion point; `None` when an operation is undefined (a division
    /// by a ball containing zero, the logarithm of a non-positive number).
    fn order(&self, s: &mut Series, k: usize, t: &Ball, state: &[Vec<Ball>]) -> Option<()> {
        let p = self.p;
        for (i, op) in self.ops.iter().enumerate() {
            let kb = Ball::from_i64(k as i64);
            let v = match op {
                Op::Const(c) => {
                    if k == 0 {
                        c.clone()
                    } else {
                        zero()
                    }
                }
                Op::Time => match k {
                    0 => t.clone(),
                    1 => Ball::from_i64(1),
                    _ => zero(),
                },
                Op::State(j) => state[*j][k].clone(),
                Op::Add(xs) => {
                    let mut acc = zero();
                    for &x in xs {
                        acc = acc.add(&s.c[x][k], p);
                    }
                    acc
                }
                Op::Mul(a, b) => conv(&s.c[*a], &s.c[*b], k, 0, k, |_| 1, p),
                Op::Div(a, b) => {
                    // q_k = (a_k - sum_{j=1..k} b_j q_{k-j}) / b_0
                    let num = s.c[*a][k].sub(&conv(&s.c[*b], &s.c[i], k, 1, k, |_| 1, p), p);
                    num.div(&s.c[*b][0], p)?
                }
                Op::Exp(a) => {
                    if k == 0 {
                        el::exp(&s.c[*a][0], p)?
                    } else {
                        conv(&s.c[*a], &s.c[i], k, 1, k, |j| j as i64, p).div(&kb, p)?
                    }
                }
                Op::Log(a) => {
                    if k == 0 {
                        el::log(&s.c[*a][0], p)?
                    } else {
                        // l_k = (a_k - (1/k) sum_{j=1..k-1} j l_j a_{k-j}) / a_0
                        let sum = conv(&s.c[i], &s.c[*a], k, 1, k - 1, |j| j as i64, p).div(&kb, p)?;
                        s.c[*a][k].sub(&sum, p).div(&s.c[*a][0], p)?
                    }
                }
                Op::Sin(a, c) | Op::Sinh(a, c) => {
                    if k == 0 {
                        if matches!(op, Op::Sin(..)) {
                            el::sin(&s.c[*a][0], p)?
                        } else {
                            el::sinh(&s.c[*a][0], p)?
                        }
                    } else {
                        conv(&s.c[*a], &s.c[*c], k, 1, k, |j| j as i64, p).div(&kb, p)?
                    }
                }
                Op::Cos(a, sn) | Op::Cosh(a, sn) => {
                    if k == 0 {
                        if matches!(op, Op::Cos(..)) {
                            el::cos(&s.c[*a][0], p)?
                        } else {
                            el::cosh(&s.c[*a][0], p)?
                        }
                    } else {
                        let v = conv(&s.c[*a], &s.c[*sn], k, 1, k, |j| j as i64, p).div(&kb, p)?;
                        if matches!(op, Op::Cos(..)) {
                            v.neg()
                        } else {
                            v
                        }
                    }
                }
                Op::Sqrt(a) => {
                    if k == 0 {
                        el::sqrt(&s.c[*a][0], p)?
                    } else {
                        // s_k = (a_k - sum_{j=1..k-1} s_j s_{k-j}) / (2 s_0)
                        let sum = conv(&s.c[i], &s.c[i], k, 1, k - 1, |_| 1, p);
                        let two_s0 = s.c[i][0].mul(&Ball::from_i64(2), p);
                        s.c[*a][k].sub(&sum, p).div(&two_s0, p)?
                    }
                }
                Op::Pow(a, e) => {
                    if k == 0 {
                        el::pow(&s.c[*a][0], e, p)?
                    } else {
                        // p_k = (1/(k a_0)) sum_{j=0..k-1} (e (k - j) - j) a_{k-j} p_j
                        let mut sum = zero();
                        for j in 0..k {
                            let w = e.mul(&Ball::from_i64((k - j) as i64), p).sub(&Ball::from_i64(j as i64), p);
                            sum = sum.add(&w.mul(&s.c[*a][k - j], p).mul(&s.c[i][j], p), p);
                        }
                        sum.div(&kb.mul(&s.c[*a][0], p), p)?
                    }
                }
                Op::Atan(a, w) => {
                    if k == 0 {
                        el::atan(&s.c[*a][0], p)?
                    } else {
                        // z_k = (k a_k - sum_{j=1..k-1} j z_j w_{k-j}) / (k w_0)
                        let sum = conv(&s.c[i], &s.c[*w], k, 1, k - 1, |j| j as i64, p);
                        let num = s.c[*a][k].mul(&kb, p).sub(&sum, p);
                        num.div(&kb.mul(&s.c[*w][0], p), p)?
                    }
                }
            };
            s.c[i].push(v);
        }
        Some(())
    }
}

/// One step of a Taylor integration: the expansion point and the coefficients `c[k][i]` of the components.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Segment {
    /// the expansion point (an exact ball)
    pub t0: Ball,
    /// the signed step
    pub h: Ball,
    /// the left and right ends as machine reals (to find the step of a time)
    pub lo: f64,
    pub hi: f64,
    /// coefficients, `order + 1` rows of n
    pub coefs: Vec<Ball>,
}

/// How a Taylor integration ended.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum End {
    Done,
    MaxSteps,
    StepSize,
    NotNumeric(f64),
}

/// The result of one Taylor integration.
pub(crate) struct Run {
    /// the steps in the order taken
    pub segments: Vec<Segment>,
    /// how it ended
    pub end: End,
    /// the time reached (a machine approximation)
    pub t: f64,
}

/// log2 |b| (about; minus infinity for zero).
fn log2(b: &Ball) -> f64 {
    if b.mid.is_zero() {
        f64::NEG_INFINITY
    } else {
        b.mid.top() as f64
    }
}

/// Integrate the tape's system from `(t0, y0)` to `t1` with a local error of about `10^-digits` relative to
/// `max(1, |y|)`, at most `max_steps` steps.
pub(crate) fn integrate(
    cx: &mut NativeCx<'_, '_, '_>,
    tape: &Tape,
    t0: &Ball,
    y0: &[Ball],
    t1: &Ball,
    digits: f64,
    max_steps: usize,
) -> Res<Run> {
    let p = tape.p;
    let n = y0.len();
    let ln_tol = digits * std::f64::consts::LN_10;
    let order = (ln_tol / 2.0).ceil() as usize + 1;
    let mut run = Run { segments: vec![], end: End::Done, t: t0.mid_f64() };
    let total = t1.sub(t0, p);
    let dir = if total.mid.neg { -1.0 } else { 1.0 };
    if total.mid.is_zero() {
        return Ok(run);
    }
    let mut t = t0.clone();
    let mut y: Vec<Ball> = y0.iter().map(|b| Ball::exact(b.mid.clone())).collect();
    loop {
        cx.tick(1)?;
        if run.segments.len() >= max_steps {
            run.end = End::MaxSteps;
            return Ok(run);
        }
        // the coefficients: y_{k+1} = F_k / (k + 1)
        let mut state: Vec<Vec<Ball>> = y.iter().map(|v| vec![v.clone()]).collect();
        let mut series = Series { c: vec![Vec::with_capacity(order + 1); tape.ops.len()] };
        for k in 0..order {
            cx.tick(1)?;
            if tape.order(&mut series, k, &t, &state).is_none() {
                run.end = End::NotNumeric(t.mid_f64());
                return Ok(run);
            }
            let kb = Ball::from_i64(k as i64 + 1);
            for (i, &o) in tape.outputs.iter().enumerate() {
                let Some(v) = series.c[o][k].div(&kb, p) else {
                    run.end = End::NotNumeric(t.mid_f64());
                    return Ok(run);
                };
                state[i].push(v);
            }
        }
        // the step size from the decay of the last two coefficients (in log2)
        let scale = y.iter().map(log2).fold(0.0f64, f64::max);
        let norm = |k: usize| state.iter().map(|c| log2(&c[k])).fold(f64::NEG_INFINITY, f64::max);
        let (a, b) = (norm(order - 1), norm(order));
        let rho_log2 = ((scale - a) / (order - 1) as f64).min((scale - b) / order as f64);
        let rest = t1.sub(&t, p);
        let rest_log2 = log2(&rest);
        let h_log2 = rho_log2 - 2.0 * std::f64::consts::LOG2_E - 0.7 * std::f64::consts::LOG2_E / (order - 1) as f64;
        let (h, last) = if !h_log2.is_finite() || h_log2 >= rest_log2 - 0.01 {
            (rest, true)
        } else {
            if h_log2 < log2(&t).max(rest_log2) - p as f64 + 8.0 || h_log2 < -1000.0 {
                run.end = End::StepSize;
                return Ok(run);
            }
            (ball_f64(dir * h_log2.exp2()), false)
        };
        // y(t + h) by Horner
        let mut ynew = Vec::with_capacity(n);
        for c in &state {
            let mut acc = c[order].clone();
            for k in (0..order).rev() {
                acc = acc.mul(&h, p).add(&c[k], p);
            }
            ynew.push(Ball::exact(acc.mid));
        }
        let mut coefs = Vec::with_capacity((order + 1) * n);
        for k in 0..=order {
            for c in &state {
                coefs.push(Ball::exact(c[k].mid.clone()));
            }
        }
        let tnew = if last { t1.clone() } else { t.add(&h, p) };
        let (ta, tb) = (t.mid_f64(), tnew.mid_f64());
        run.segments.push(Segment { t0: t.clone(), h: h.clone(), lo: ta.min(tb), hi: ta.max(tb), coefs });
        t = Ball::exact(tnew.mid);
        run.t = tb;
        y = ynew;
        if last {
            return Ok(run);
        }
    }
}

/// The midpoint of a ball as a machine real.
trait MidF64 {
    fn mid_f64(&self) -> f64;
}

impl MidF64 for Ball {
    fn mid_f64(&self) -> f64 {
        mc_arith::num::float::dyadic_to_f64(self.mid.neg, &self.mid.man, self.mid.exp)
    }
}

/// The value (or the derivative of order `order`) of component `i` of a segment at `t`.
pub(crate) fn eval(seg: &Segment, n: usize, i: usize, t: &Ball, order: usize, p: u64) -> Ball {
    let rows = seg.coefs.len() / n;
    if order >= rows {
        return zero();
    }
    let d = t.sub(&seg.t0, p);
    let mut acc = zero();
    for k in (order..rows).rev() {
        let falling: i64 = (k - order + 1..=k).map(|v| v as i64).product();
        let c = seg.coefs[k * n + i].mul(&Ball::from_i64(falling), p);
        acc = acc.mul(&d, p).add(&c, p);
    }
    acc
}

/// |a - b| as a radius.
pub(crate) fn distance(a: &Ball, b: &Ball, p: u64) -> Mag {
    let d = a.sub(b, p);
    Mag::from_float_up(&d.mid).add(&d.rad)
}

/// The step of a run containing the machine time `t` (the steps ascending), if any.
pub(crate) fn find(segments: &[Segment], t: f64) -> Option<&Segment> {
    let k = segments.partition_point(|s| s.lo <= t);
    let s = segments.get(k.checked_sub(1)?)?;
    (t <= s.hi).then_some(s)
}

/// The ball `x` as a `Num` at `p` bits.
pub(crate) fn num_of(x: Ball, p: u64) -> Num {
    Num::Real(Real::Ball(el::round(x, p), p))
}

/// A number as a ball at `p` bits, if real.
pub(crate) fn ball(n: &Num, p: u64) -> Option<Ball> {
    ball_of(n, p)
}
