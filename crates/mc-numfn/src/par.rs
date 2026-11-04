//! Parallel batch evaluation (package N4b, semantics 18.2 (2)): [`Par`] evaluates large [`NumFn::eval_flat`]
//! batches of compiled, effect-free code on several threads, with results identical to the sequential evaluation.
//!
//! - **Eligible code**: the expression compiles (`mc_jit::compile_expr`), so it is pure (expressions never draw)
//!   and gives exactly the interpreter's results (decision 1b.3). Anything else goes to the sequential backend.
//! - **Splitting** (decision N4b.5): `mc_jit::batch` cuts the points into blocks that the calling thread and tasks
//!   of the session's thread pool claim one at a time; each point writes only its own outputs. With one thread,
//!   fewer than [`PAR_MIN_POINTS`] points, or a function it inlined redefined, the batch is the sequential one.
//! - **Declines and stops**: every point the compiled code declined, or did not complete, is evaluated afterwards,
//!   in order, by the sequential backend on the calling thread, as `Auto` does point by point. So the results are
//!   the same for any number of threads (semantics 18.1) and the same as the sequential backend's.
//! - **Fuel and interrupts** (decision N4b.6): one unit per point before the batch, as the compiled backend charges
//!   per call; the loops' fuel from every thread is charged to the calling task as it is spent, and a limit or an
//!   interrupt stops every thread and returns the evaluator's failure.
use crate::{auto_expr, Arg, Auto, Backend, Interpreted, NumFn, Options, Stats, Var};
use mc_core::{Sym, T};
use mc_eval::{Abort, Control, NativeCx, Res};
use mc_jit::ExprFn;

/// Batches smaller than this run sequentially: a compiled point costs tens of nanoseconds, and handing blocks to
/// other threads some microseconds.
pub const PAR_MIN_POINTS: usize = 256;

/// Counters of a [`Par`]: what ran where.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct ParStats {
    /// batches split across threads
    pub batches: u64,
    /// points of split batches the compiled code completed
    pub points: u64,
    /// points of split batches the sequential backend evaluated (declined compiled calls)
    pub fallback: u64,
    /// the most threads one batch used
    pub threads: u64,
}

/// A [`NumFn`] whose large `eval_flat` batches run compiled on several threads, with `I` (typically an [`Auto`])
/// as the sequential backend for single points, small batches, refused expressions and declined points.
pub struct Par<'h, I> {
    seq: I,
    code: Option<ExprFn>,
    refusal: Option<String>,
    /// the control block of the batches' compiled loops (its stop word stops a batch)
    ctl: Control,
    /// the most threads a batch uses (`None`: the session's pool size)
    threads: Option<usize>,
    min_points: usize,
    stats: ParStats,
    _h: std::marker::PhantomData<T<'h>>,
}

/// `expr` in `vars` as a [`Par`] over the automatic backend of [`auto_expr`] (with `opts`).
pub fn par_expr<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    expr: T<'h>,
    vars: &[Var],
    opts: &Options,
) -> Res<Par<'h, Auto<'h, Interpreted<'h>>>> {
    let mut seq = auto_expr(cx, expr, vars, opts)?;
    let (body, numeric) = (seq.interpreted().body(), seq.interpreted().numeric());
    Ok(Par::new(cx, seq, vars, body, numeric))
}

impl<'h, I: NumFn<'h, f64>> Par<'h, I> {
    /// `seq` evaluates `body` in `vars`; the parallel code is compiled from the same body (`consts` as in
    /// [`crate::Compiled::new`]). When the compiler refuses, or the compiled code does not have `seq`'s inputs and
    /// outputs, every batch is `seq`'s.
    pub fn new(cx: &mut NativeCx<'_, '_, 'h>, seq: I, vars: &[Var], body: T<'h>, consts: bool) -> Par<'h, I> {
        let params: Vec<(Sym, mc_jit::Arg)> = vars
            .iter()
            .map(|v| {
                let a = match v.arg {
                    Arg::Scalar => mc_jit::Arg::Real,
                    Arg::Vector(n) => mc_jit::Arg::Vector(n),
                };
                (v.sym, a)
            })
            .collect();
        let (code, refusal) = match mc_jit::compile_expr(cx, &params, body, consts, mc_jit::Backend::Cranelift) {
            Ok(c) if c.draws() => (None, Some("the code draws random numbers (an effect)".to_string())),
            Ok(c) if c.output_types().contains(&mc_jit::ir::Ty::Bool) => (None, Some("a boolean result".to_string())),
            Ok(c) if c.inputs() != seq.inputs() || c.outputs() != seq.outputs() => {
                (None, Some(format!("{} outputs compiled, {} sequential", c.outputs(), seq.outputs())))
            }
            Ok(c) => (Some(c), None),
            Err(e) => (None, Some(e)),
        };
        Par {
            seq,
            code,
            refusal,
            ctl: Control::default(),
            threads: None,
            min_points: PAR_MIN_POINTS,
            stats: ParStats::default(),
            _h: std::marker::PhantomData,
        }
    }

    /// Use at most `n` threads per batch (at least one; by default the session's pool size).
    pub fn with_threads(mut self, n: usize) -> Self {
        self.threads = Some(n.max(1));
        self
    }

    /// Split batches of at least `n` points (default [`PAR_MIN_POINTS`]).
    pub fn with_min_points(mut self, n: usize) -> Self {
        self.min_points = n.max(1);
        self
    }

    /// Why batches are not split (`None` when they can be).
    pub fn refusal(&self) -> Option<&str> {
        self.refusal.as_deref()
    }

    /// The counters of split batches.
    pub fn par_stats(&self) -> ParStats {
        self.stats
    }

    /// The sequential backend.
    pub fn sequential(&mut self) -> &mut I {
        &mut self.seq
    }

    /// One batch on `threads` threads.
    fn split(&mut self, cx: &mut NativeCx<'_, '_, 'h>, args: &[f64], out: &mut [f64], threads: usize) -> Res<bool> {
        let code = self.code.as_ref().expect("compiled");
        let (ni, no) = (code.inputs(), code.outputs());
        let n = out.len() / no;
        cx.tick(n as u64)?;
        let interrupt = cx.stop_flag().clone();
        let mut failed: Option<Abort> = None;
        let batch = {
            let mut charge = |k: u64| {
                if let Err(e) = cx.tick(k) {
                    failed = Some(e);
                    return false;
                }
                !cx.stopped()
            };
            mc_jit::batch::run(code, &self.ctl, args, out, threads, &interrupt, &mut charge)
        };
        if let Some(e) = failed {
            return Err(e);
        }
        self.stats.batches += 1;
        self.stats.threads = self.stats.threads.max(batch.threads as u64);
        let mut ok = true;
        for (k, &done) in batch.done.iter().enumerate() {
            let o = &mut out[k * no..(k + 1) * no];
            if done {
                self.stats.points += 1;
                ok &= o.iter().all(|v| v.is_finite());
            } else {
                // declined or not reached: the sequential backend, in order (an interrupt stops it there)
                self.stats.fallback += 1;
                ok &= self.seq.eval(cx, &args[k * ni..(k + 1) * ni], o)?;
            }
        }
        Ok(ok)
    }
}

impl<'h, I: NumFn<'h, f64>> NumFn<'h, f64> for Par<'h, I> {
    fn eval(&mut self, cx: &mut NativeCx<'_, '_, 'h>, args: &[f64], out: &mut [f64]) -> Res<bool> {
        self.seq.eval(cx, args, out)
    }

    fn eval_batch(&mut self, cx: &mut NativeCx<'_, '_, 'h>, points: &[Vec<f64>], outs: &mut [Vec<f64>]) -> Res<bool> {
        self.seq.eval_batch(cx, points, outs)
    }

    fn eval_flat(&mut self, cx: &mut NativeCx<'_, '_, 'h>, args: &[f64], out: &mut [f64]) -> Res<bool> {
        let (ni, no) = (self.inputs(), self.outputs());
        if no == 0 || !out.len().is_multiple_of(no) || (ni > 0 && args.len() != ni * (out.len() / no)) {
            return Ok(false);
        }
        let n = out.len() / no;
        let threads = self.threads.unwrap_or_else(mc_jit::batch::pool_threads);
        // decision N4b.5: only compiled code, large batches, more than one thread, current definitions
        let split = threads > 1 && n >= self.min_points && self.code.as_ref().is_some_and(|c| c.current(cx));
        if !split {
            return self.seq.eval_flat(cx, args, out);
        }
        self.split(cx, args, out, threads)
    }

    fn inputs(&self) -> usize {
        self.seq.inputs()
    }

    fn outputs(&self) -> usize {
        self.seq.outputs()
    }

    fn backend(&self) -> Backend {
        if self.code.is_some() {
            Backend::Compiled
        } else {
            self.seq.backend()
        }
    }

    /// The sequential backend's counters, with the completed points of split batches counted as compiled.
    fn stats(&self) -> Stats {
        let mut s = self.seq.stats();
        s.compiled += self.stats.points;
        s
    }
}
