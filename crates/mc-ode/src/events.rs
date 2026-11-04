//! Events: zero crossings of functions `g(t, y)` along the solution (`events: [g == 0, ...]`), detected by a sign
//! change between the ends of each accepted step and located on the step's dense output by the Illinois variant
//! of regula falsi (decision N3.7). A crossing and a touching zero at a step end count; two crossings within one
//! step cancel and are not seen.
use crate::dense;
use crate::options::EventAction;
use mc_eval::{NativeCx, Res};
use mc_numfn::NumFn;

/// The event functions and their values at the start of the current step.
pub(crate) struct Events<'h> {
    g: Box<dyn NumFn<'h, f64> + 'h>,
    /// the number of event functions
    m: usize,
    /// the state size
    n: usize,
    /// stop at the first event or record them all
    pub action: EventAction,
    prev: Vec<f64>,
    cur: Vec<f64>,
    args: Vec<f64>,
    /// the events found so far: time and the event's index (from 0)
    pub found: Vec<(f64, usize)>,
}

/// A located event: θ within the step, the event's index.
pub(crate) struct Hit {
    pub theta: f64,
    pub index: usize,
}

/// The iteration limit of the root finder (θ is located to about one unit of rounding in fewer).
const MAX_ITERATIONS: usize = 100;

impl<'h> Events<'h> {
    /// `m` event functions of `(t, y1, ..., yn)`.
    pub fn new(g: Box<dyn NumFn<'h, f64> + 'h>, m: usize, n: usize, action: EventAction) -> Events<'h> {
        Events { g, m, n, action, prev: vec![0.0; m], cur: vec![0.0; m], args: vec![0.0; n + 1], found: vec![] }
    }

    fn eval(&mut self, cx: &mut NativeCx<'_, '_, 'h>, t: f64, y: &[f64], out: &mut [f64]) -> Res<bool> {
        self.args[0] = t;
        self.args[1..].copy_from_slice(y);
        self.g.eval(cx, &self.args, out)
    }

    /// The values at the initial point; `false` when not numeric.
    pub fn start(&mut self, cx: &mut NativeCx<'_, '_, 'h>, t: f64, y: &[f64]) -> Res<bool> {
        let mut prev = std::mem::take(&mut self.prev);
        let ok = self.eval(cx, t, y, &mut prev)?;
        self.prev = prev;
        Ok(ok)
    }

    /// The events within the step from `origin` of size `h` (its dense coefficients `coefs`), which ended at `y`:
    /// the crossings ordered by time (only the first under [`EventAction::Stop`]). `Err(t)` when an event
    /// function is not numeric at time t.
    pub fn check(
        &mut self,
        cx: &mut NativeCx<'_, '_, 'h>,
        origin: f64,
        h: f64,
        coefs: &[f64],
        y: &[f64],
    ) -> Res<Result<Vec<Hit>, f64>> {
        let mut cur = std::mem::take(&mut self.cur);
        let ok = self.eval(cx, origin + h, y, &mut cur)?;
        self.cur = cur;
        if !ok {
            return Ok(Err(origin + h));
        }
        let mut hits = vec![];
        let mut ys = vec![0.0; self.n];
        let mut gs = vec![0.0; self.m];
        for e in 0..self.m {
            let (g0, g1) = (self.prev[e], self.cur[e]);
            // a zero at the start was the previous step's event (or the initial point)
            if g0 == 0.0 || (g0 < 0.0) == (g1 < 0.0) && g1 != 0.0 {
                continue;
            }
            // Illinois: regula falsi, halving the value of an end kept twice in a row
            let (mut a, mut fa, mut b, mut fb) = (0.0f64, g0, 1.0f64, g1);
            let mut side = 0i8;
            for _ in 0..MAX_ITERATIONS {
                if fa == 0.0 || fb == 0.0 || (b - a).abs() <= 4.0 * f64::EPSILON {
                    break;
                }
                let c = (a * fb - b * fa) / (fb - fa);
                let c = if c > a.min(b) && c < a.max(b) { c } else { 0.5 * (a + b) };
                for (i, v) in ys.iter_mut().enumerate() {
                    *v = dense::eval(coefs, self.n, i, h, c, 0);
                }
                if !self.eval(cx, origin + c * h, &ys, &mut gs)? {
                    return Ok(Err(origin + c * h));
                }
                let fc = gs[e];
                if fc == 0.0 || (fc < 0.0) == (fb < 0.0) {
                    (b, fb) = (c, fc);
                    if side == -1 {
                        fa *= 0.5;
                    }
                    side = -1;
                } else {
                    (a, fa) = (c, fc);
                    if side == 1 {
                        fb *= 0.5;
                    }
                    side = 1;
                }
                cx.tick(1)?;
            }
            // the end of the bracket where g has crossed (has the sign of g1, or is zero)
            let after = if fa == 0.0 { a } else if fb == 0.0 || (fb < 0.0) == (g1 < 0.0) { b } else { a };
            hits.push(Hit { theta: after.clamp(0.0, 1.0), index: e });
        }
        hits.sort_by(|p, q| p.theta.total_cmp(&q.theta).then(p.index.cmp(&q.index)));
        if self.action == EventAction::Stop {
            hits.truncate(1);
        }
        for hit in &hits {
            self.found.push((origin + hit.theta * h, hit.index));
        }
        std::mem::swap(&mut self.prev, &mut self.cur);
        Ok(Ok(hits))
    }
}
