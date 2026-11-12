//! Oscillatory integrals over half-lines at arbitrary precision (round 20, the precision contract: `working_precision:`
//! and `digits:` reach the oscillatory stage). The method of [`crate::osc`] carried to balls: the integral is the sum of
//! the integrals between consecutive zeros of the oscillating part (Longman 1956), each computed by the tanh-sinh rule
//! on balls ([`crate::hp`]), and the partial sums are extrapolated by Wynn's epsilon algorithm (Wynn 1956) in
//! floating-point arithmetic of the working precision.
//!
//! The zeros are located in machine arithmetic (sampling, then regula falsi) and refined by the secant method on the
//! ball integrand to about half the working precision: a partition point off a zero by delta changes a partial sum
//! by about f'(z) delta^2 / 2, so half the bits suffice for the extrapolated value. The error estimate is the spread
//! of the last three extrapolations plus the terms' error estimates; it is an estimate, not a bound.
use crate::hp::{self, BEnd, BallOutcome};
use mc_arith::num::{Ball, Float};
use mc_arith::{Kind, Num, Real};
use mc_eval::{NativeCx, Res};
use mc_numeric::machine::C64;
use mc_numfn::NumFn;

/// The most intervals between zeros.
const MAX_TERMS: usize = 600;

/// The most machine samples of the zero search.
const MAX_SAMPLES: usize = 400_000;

/// The fewest terms before the extrapolation may be accepted.
const MIN_TERMS: usize = 10;

/// A complex floating-point number of the working precision (the midpoints; radii are dropped).
#[derive(Clone, Debug)]
struct F2 {
    re: Float,
    im: Float,
}

impl F2 {
    fn zero() -> F2 {
        F2 { re: Float::zero(), im: Float::zero() }
    }
    fn add(&self, o: &F2, p: u64) -> F2 {
        F2 { re: self.re.add(&o.re, p).0, im: self.im.add(&o.im, p).0 }
    }
    fn sub(&self, o: &F2, p: u64) -> F2 {
        F2 { re: self.re.sub(&o.re, p).0, im: self.im.sub(&o.im, p).0 }
    }
    /// 1/z, `None` for 0.
    fn recip(&self, p: u64) -> Option<F2> {
        if self.re.is_zero() && self.im.is_zero() {
            return None;
        }
        let w = p + 16;
        let n = self.re.mul(&self.re, w).0.add(&self.im.mul(&self.im, w).0, w).0;
        Some(F2 { re: self.re.div(&n, p).0, im: self.im.neg().div(&n, p).0 })
    }
    /// log2 |z| (to within half a bit; -inf for 0).
    fn log2(&self) -> f64 {
        let t = |f: &Float| if f.is_zero() { f64::NEG_INFINITY } else { f.top() as f64 };
        t(&self.re).max(t(&self.im))
    }
    fn of(n: &Num, p: u64) -> Option<F2> {
        let mid = |r: &Real| match r.to_kind(Kind::Ball, p) {
            Real::Ball(b, _) => Some(b.mid),
            _ => None,
        };
        Some(match n {
            Num::Real(r) => F2 { re: mid(r)?, im: Float::zero() },
            Num::Complex(a, b) => F2 { re: mid(a)?, im: mid(b)? },
        })
    }
    fn to_num(&self, p: u64) -> Num {
        let re = Real::Ball(Ball::exact(self.re.clone()), p);
        if self.im.is_zero() {
            Num::Real(re)
        } else {
            Num::Complex(re, Real::Ball(Ball::exact(self.im.clone()), p))
        }
    }
}

/// Wynn's epsilon algorithm on `s`: the last even-column estimate (`None` for fewer than three terms).
fn epsilon(s: &[F2], p: u64) -> Option<F2> {
    let n = s.len();
    if n < 3 {
        return None;
    }
    let mut prev: Vec<F2> = vec![F2::zero(); n + 1];
    let mut cur: Vec<F2> = s.to_vec();
    let mut est = s[n - 1].clone();
    let mut k = 0;
    while cur.len() > 1 {
        let mut next = Vec::with_capacity(cur.len() - 1);
        for i in 0..cur.len() - 1 {
            let d = cur[i + 1].sub(&cur[i], p);
            // the table breaks down where the sequence has converged in this column
            let Some(r) = d.recip(p) else { return Some(est) };
            next.push(prev[i + 1].add(&r, p));
        }
        prev = cur;
        cur = next;
        k += 1;
        if k % 2 == 0 {
            est = cur.last().expect("nonempty").clone();
        }
    }
    Some(est)
}

/// The real or imaginary part of the ball integrand (as `Re f`, or `i Im f`).
struct Part<'a, 'h> {
    f: &'a mut dyn NumFn<'h, Num>,
    imag: bool,
}

impl Part<'_, '_> {
    fn project(&self, v: &Num) -> Num {
        match (v, self.imag) {
            (Num::Real(_), false) => v.clone(),
            (Num::Real(_), true) => Num::int(0),
            (Num::Complex(a, _), false) => Num::Real(a.clone()),
            (Num::Complex(_, b), true) => Num::Complex(Real::Exact(mc_arith::num::Rat::from_i64(0)), b.clone()),
        }
    }
}

impl<'h> NumFn<'h, Num> for Part<'_, 'h> {
    fn eval(&mut self, cx: &mut NativeCx<'_, '_, 'h>, args: &[Num], out: &mut [Num]) -> Res<bool> {
        if !self.f.eval(cx, args, out)? {
            return Ok(false);
        }
        for v in out.iter_mut() {
            *v = self.project(v);
        }
        Ok(true)
    }
    fn eval_flat(&mut self, cx: &mut NativeCx<'_, '_, 'h>, args: &[Num], out: &mut [Num]) -> Res<bool> {
        if !self.f.eval_flat(cx, args, out)? {
            return Ok(false);
        }
        for v in out.iter_mut() {
            *v = self.project(v);
        }
        Ok(true)
    }
    fn inputs(&self) -> usize {
        self.f.inputs()
    }
    fn outputs(&self) -> usize {
        self.f.outputs()
    }
}

/// The integral over `ends` at `p` bits when a half-line end oscillates: the half-lines by [`half_line`], the
/// finite part between them by the tanh-sinh rule; `None` when no infinite end oscillates (or the oscillatory search
/// fails), so the caller's rule runs.
#[allow(clippy::too_many_arguments)]
pub(crate) fn integrate<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    fm: &mut dyn NumFn<'h, C64>,
    fb: &mut dyn NumFn<'h, Num>,
    ends: &[BEnd],
    p: u64,
    goals: (f64, f64),
    level: u32,
) -> Res<Option<BallOutcome>> {
    let n = ends.len();
    if n < 2 {
        return Ok(None);
    }
    let zero = Ball::from_i64(0);
    // (start of the half-line, its direction, the sign of its contribution)
    let mut halves: Vec<(Ball, f64, bool)> = Vec::new();
    let mut lo = 0;
    let mut hi = n - 1;
    let mut extra_mid: Option<Ball> = None;
    match (&ends[0], &ends[n - 1]) {
        (BEnd::Inf(s), BEnd::Inf(t)) if n == 2 => {
            if s == t {
                return Ok(None);
            }
            // split at 0
            halves.push((zero.clone(), if *t { 1.0 } else { -1.0 }, true));
            halves.push((zero.clone(), if *s { 1.0 } else { -1.0 }, false));
            extra_mid = Some(zero);
        }
        _ => {
            if let (BEnd::Inf(s), BEnd::At(a)) = (&ends[0], &ends[1]) {
                halves.push((a.clone(), if *s { 1.0 } else { -1.0 }, false));
                lo = 1;
            }
            if let (BEnd::At(b), BEnd::Inf(t)) = (&ends[n - 2], &ends[n - 1]) {
                halves.push((b.clone(), if *t { 1.0 } else { -1.0 }, true));
                hi = n - 2;
            }
        }
    }
    if halves.is_empty() {
        return Ok(None);
    }
    let mut any = false;
    for (a, dir, _) in &halves {
        any |= oscillating(cx, fm, Real::Ball(a.clone(), p).to_f64(), *dir)?;
    }
    if !any {
        return Ok(None);
    }
    let mut total = Num::int(0);
    let mut err = f64::NEG_INFINITY;
    let mut conv = true;
    for (a, dir, plus) in &halves {
        match half_line(cx, fm, fb, a, *dir, p, goals, level)? {
            Some(BallOutcome::Done { value, err_log2, converged }) => {
                total = if *plus { total.add(&value) } else { total.add(&value.neg()) };
                err = log2_add(err, err_log2);
                conv &= converged;
            }
            Some(nn) => return Ok(Some(nn)),
            None => return Ok(None),
        }
    }
    if extra_mid.is_none() && hi > lo {
        match hp::integrate(cx, fb, &ends[lo..=hi], p, goals, level)? {
            BallOutcome::Done { value, err_log2, converged } => {
                total = total.add(&value);
                err = log2_add(err, err_log2);
                conv &= converged;
            }
            nn => return Ok(Some(nn)),
        }
    }
    Ok(Some(BallOutcome::Done { value: total, err_log2: err, converged: conv }))
}

/// The machine value at one point, `None` when not numeric.
fn mval<'h>(cx: &mut NativeCx<'_, '_, 'h>, f: &mut dyn NumFn<'h, C64>, x: f64) -> Res<Option<C64>> {
    let mut y = [C64 { re: 0.0, im: 0.0 }];
    Ok(f.eval(cx, &[C64 { re: x, im: 0.0 }], &mut y)?.then_some(y[0]))
}

/// Does the integrand oscillate without decaying on the half-line from `a` in direction `dir` (the test of
/// [`crate::osc::oscillating_tail`]: 8 sign changes in 400 samples over 100 |a| units, the last quarter above 10^-6
/// of the largest value)?
pub(crate) fn oscillating<'h>(cx: &mut NativeCx<'_, '_, 'h>, f: &mut dyn NumFn<'h, C64>, a: f64, dir: f64) -> Res<bool> {
    let sc = a.abs().max(1.0);
    let mut ys = Vec::with_capacity(400);
    for k in 1..=400 {
        match mval(cx, f, a + dir * 0.25 * sc * k as f64)? {
            Some(v) => ys.push(v),
            None => return Ok(false),
        }
    }
    for imag in [false, true] {
        let g: Vec<f64> = ys.iter().map(|v| if imag { v.im } else { v.re }).collect();
        let changes = g.windows(2).filter(|w| (w[0] < 0.0) != (w[1] < 0.0)).count();
        let big = g.iter().fold(0.0f64, |m, v| m.max(v.abs()));
        let tail = g[300..].iter().fold(0.0f64, |m, v| m.max(v.abs()));
        if changes >= 8 && big > 0.0 && tail > 1e-6 * big {
            return Ok(true);
        }
    }
    Ok(false)
}

/// The integral over the half-line from `a` in direction `dir` (±1) at `p` bits, part by part for a complex
/// integrand; `None` when the zero search fails (not oscillatory at this scale, a point that is not numeric).
#[allow(clippy::too_many_arguments)]
pub(crate) fn half_line<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    fm: &mut dyn NumFn<'h, C64>,
    fb: &mut dyn NumFn<'h, Num>,
    a: &Ball,
    dir: f64,
    p: u64,
    goals: (f64, f64),
    level: u32,
) -> Res<Option<BallOutcome>> {
    let af = Real::Ball(a.clone(), p).to_f64();
    let sc = af.abs().max(1.0);
    let mut mr = 0.0f64;
    let mut mi = 0.0f64;
    for k in 0..64 {
        let Some(v) = mval(cx, fm, af + dir * 0.37 * (k as f64 + 0.5) * sc)? else { return Ok(None) };
        mr = mr.max(v.re.abs());
        mi = mi.max(v.im.abs());
    }
    let parts: &[bool] = if mi > 1e3 * mr {
        &[true]
    } else if mr > 1e3 * mi || mi == 0.0 {
        &[false]
    } else {
        &[false, true]
    };
    let mut total = Num::int(0);
    let mut err = f64::NEG_INFINITY;
    let mut conv = true;
    for &imag in parts {
        let mut pf = Part { f: &mut *fb, imag };
        match one_part(cx, fm, &mut pf, a, af, dir, imag, p, goals, level)? {
            Some(BallOutcome::Done { value, err_log2, converged }) => {
                total = total.add(&value);
                err = log2_add(err, err_log2);
                conv &= converged;
            }
            other => return Ok(other),
        }
    }
    Ok(Some(BallOutcome::Done { value: total, err_log2: err, converged: conv }))
}

/// log2(2^a + 2^b).
fn log2_add(a: f64, b: f64) -> f64 {
    let (hi, lo) = if a > b { (a, b) } else { (b, a) };
    if lo == f64::NEG_INFINITY {
        return hi;
    }
    hi + (1.0 + (lo - hi).exp2()).log2()
}

/// The sign-relevant part of a machine value.
fn mpart(v: C64, imag: bool) -> f64 {
    if imag {
        v.im
    } else {
        v.re
    }
}

/// The midpoint of the part of the ball value at x (as a ball without radius).
fn bpart<'h>(cx: &mut NativeCx<'_, '_, 'h>, f: &mut Part<'_, 'h>, x: &Ball, p: u64) -> Res<Option<Ball>> {
    let mut y = [Num::int(0)];
    if !f.eval(cx, &[Num::Real(Real::Ball(x.clone(), p))], &mut y)? {
        return Ok(None);
    }
    let r = match &y[0] {
        Num::Real(r) => r.clone(),
        Num::Complex(_, b) => b.clone(),
    };
    Ok(match r.to_kind(Kind::Ball, p) {
        Real::Ball(b, _) => Some(Ball::exact(b.mid)),
        _ => None,
    })
}

/// Refine a machine zero `z` of the part by the secant method on balls to about p/2 + 32 bits.
fn refine_zero<'h>(cx: &mut NativeCx<'_, '_, 'h>, f: &mut Part<'_, 'h>, z: f64, p: u64) -> Res<Ball> {
    let exact = |v: f64| hp::ball_f64(v);
    let mut x0 = exact(z * (1.0 + 1e-9) + 1e-12);
    let mut x1 = exact(z);
    let Some(mut g0) = bpart(cx, f, &x0, p)? else { return Ok(x1) };
    let Some(mut g1) = bpart(cx, f, &x1, p)? else { return Ok(x1) };
    let target = -((p / 2 + 32) as f64) + z.abs().max(1.0).log2();
    for _ in 0..12 {
        cx.tick(1)?;
        if g1.mid.is_zero() {
            break;
        }
        let dg = g1.sub(&g0, p);
        let Some(step) = g1.mul(&x1.sub(&x0, p), p).div(&dg, p) else { break };
        let x2 = Ball::exact(x1.sub(&step, p).mid);
        let small = step.mid.is_zero() || (step.mid.top() as f64) < target;
        x0 = x1;
        g0 = g1;
        x1 = x2;
        if small {
            break;
        }
        match bpart(cx, f, &x1, p)? {
            Some(g) => g1 = g,
            None => return Ok(x0),
        }
        // a step that left the bracket's scale: keep the machine zero
        if (Real::Ball(x1.clone(), p).to_f64() - z).abs() > 1e-6 * z.abs().max(1.0) {
            return Ok(exact(z));
        }
    }
    Ok(x1)
}

/// One part: zeros, terms, extrapolation.
#[allow(clippy::too_many_arguments)]
fn one_part<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    fm: &mut dyn NumFn<'h, C64>,
    fb: &mut Part<'_, 'h>,
    a: &Ball,
    af: f64,
    dir: f64,
    imag: bool,
    p: u64,
    goals: (f64, f64),
    level: u32,
) -> Res<Option<BallOutcome>> {
    let x_of = |u: f64| af + dir * u;
    let mut step = 0.05 * af.abs().max(1.0);
    let mut u = 0.0;
    let mut g0 = match mval(cx, fm, x_of(u))? {
        Some(v) => mpart(v, imag),
        None => match mval(cx, fm, x_of(1e-9 * af.abs().max(1.0)))? {
            Some(v) => mpart(v, imag),
            None => return Ok(None),
        },
    };
    // the machine zeros (offsets u from a) and the ball partition points
    let mut zeros: Vec<f64> = vec![0.0];
    let mut last_b: Ball = a.clone();
    let mut sums: Vec<F2> = Vec::new();
    let mut total = Num::int(0);
    let mut term_err = f64::NEG_INFINITY;
    let mut samples = 0usize;
    let mut history: Vec<F2> = Vec::new();
    let mut best: Option<(F2, f64, f64)> = None;
    let piece_goals = (goals.0 - 8.0, goals.1 - 8.0);
    while zeros.len() <= MAX_TERMS && samples < MAX_SAMPLES {
        cx.tick(1)?;
        let mut found = None;
        'search: while samples < MAX_SAMPLES {
            for k in 1..=16 {
                let uk = u + step * k as f64;
                let Some(v) = mval(cx, fm, x_of(uk))? else { return Ok(None) };
                samples += 1;
                let g = mpart(v, imag);
                if g == 0.0 || (g < 0.0) != (g0 < 0.0) {
                    found = Some((uk - step, uk));
                    u = uk;
                    g0 = g;
                    break 'search;
                }
            }
            u += 16.0 * step;
            if zeros.len() == 1 && u > 1e6 * af.abs().max(1.0) {
                return Ok(None);
            }
            if zeros.len() > 1 && u - zeros[zeros.len() - 1] > 1e4 * (zeros[zeros.len() - 1] - zeros[zeros.len() - 2]).max(1e-300) {
                return Ok(None);
            }
            step *= 1.5;
        }
        let Some((mut lo, mut hi)) = found else { break };
        // regula falsi (Illinois) on the machine values
        let (Some(vl), Some(vh)) = (mval(cx, fm, x_of(lo))?, mval(cx, fm, x_of(hi))?) else { return Ok(None) };
        let (mut glo, mut ghi) = (mpart(vl, imag), mpart(vh, imag));
        let mut side = 0i8;
        for _ in 0..60 {
            if glo == ghi || ghi == 0.0 {
                break;
            }
            let m = hi - ghi * (hi - lo) / (ghi - glo);
            if !(m > lo && m < hi) || (hi - lo) <= 4e-16 * hi.abs().max(1.0) {
                break;
            }
            let Some(gm) = mval(cx, fm, x_of(m))? else { return Ok(None) };
            let gm = mpart(gm, imag);
            if gm == 0.0 {
                lo = m;
                hi = m;
                break;
            }
            if (gm < 0.0) == (glo < 0.0) {
                lo = m;
                glo = gm;
                if side == -1 {
                    ghi *= 0.5;
                }
                side = -1;
            } else {
                hi = m;
                ghi = gm;
                if side == 1 {
                    glo *= 0.5;
                }
                side = 1;
            }
        }
        let z = if glo.abs() < ghi.abs() { lo } else { hi };
        let last = *zeros.last().expect("one");
        if z <= last {
            continue;
        }
        let zb = refine_zero(cx, fb, x_of(z), p)?;
        let o = hp::integrate(cx, fb, &[BEnd::At(last_b.clone()), BEnd::At(zb.clone())], p, piece_goals, level)?;
        let (v, e) = match o {
            BallOutcome::Done { value, err_log2, .. } => (value, err_log2),
            nn @ BallOutcome::NotNumeric(_) => return Ok(Some(nn)),
        };
        total = total.add(&v);
        term_err = log2_add(term_err, e);
        let Some(s) = F2::of(&total, p) else { return Ok(None) };
        sums.push(s);
        zeros.push(z);
        last_b = zb;
        step = (z - last) / 8.0;
        u = z;
        g0 = match mval(cx, fm, x_of(u + step * 1e-3))? {
            Some(v) => mpart(v, imag),
            None => return Ok(None),
        };
        if sums.len() >= MIN_TERMS {
            let window = &sums[sums.len().saturating_sub(120)..];
            if let Some(est) = epsilon(window, p) {
                history.push(est);
                let k = history.len();
                if k < 3 {
                    continue;
                }
                let de = history[k - 1].sub(&history[k - 2], p).log2().max(history[k - 1].sub(&history[k - 3], p).log2());
                let mag = history[k - 1].log2();
                let err = log2_add(de, term_err);
                if best.as_ref().is_none_or(|b| de < b.2) {
                    best = Some((history[k - 1].clone(), err, de));
                }
                let floor = mag - p as f64 + 8.0;
                let goal = goals.0.max(goals.1 + mag);
                if de.max(floor) <= goal - 4.0 {
                    let e = log2_add(de.max(floor), term_err);
                    return Ok(Some(BallOutcome::Done { value: history[k - 1].to_num(p), err_log2: e, converged: e <= goal }));
                }
            }
        }
    }
    Ok(best.map(|(v, err, _)| {
        let mag = v.log2();
        BallOutcome::Done { value: v.to_num(p), err_log2: err, converged: err <= goals.0.max(goals.1 + mag) }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn epsilon_accelerates_log2_at_200_bits() {
        let p = 200;
        let mut s = Vec::new();
        let mut t = F2::zero();
        for k in 1..=60i64 {
            let term = Ball::from_ratio(if k % 2 == 1 { 1 } else { -1 }, k, p);
            t = t.add(&F2 { re: term.mid, im: Float::zero() }, p);
            s.push(t.clone());
        }
        let v = epsilon(&s, p).unwrap();
        let ln2 = mc_numeric::elementary::log(&Ball::from_i64(2), p).unwrap();
        let d = F2 { re: v.re, im: Float::zero() }.sub(&F2 { re: ln2.mid, im: Float::zero() }, p);
        assert!(d.log2() < -150.0, "{}", d.log2());
    }
}
