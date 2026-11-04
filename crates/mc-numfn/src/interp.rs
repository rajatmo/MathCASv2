//! The interpreted backend: the closure applied by the evaluator, for any number type ([`Value`]): machine reals,
//! complex machine numbers, and numbers of every kind (balls at any precision).
use crate::term::TermFn;
use crate::value::Value;
use crate::prep::prepared;
use crate::{Backend, NumFn, Prep, Var};
use mc_arith::{Arith, Num};
use mc_core::{Heap, T};
use mc_eval::{NativeCx, Res};

/// A [`TermFn`] with the arithmetic it builds number terms with: the interpreted view of the closure, borrowing
/// both (what `mc-roots` uses: one closure serves several views and the ball refinement).
pub struct Machine<'r, 'h> {
    /// the closure
    pub f: &'r mut TermFn<'h>,
    /// the arithmetic number terms are built with
    pub ar: &'r Arith<'h, Heap>,
}

impl<'h> Machine<'_, 'h> {
    fn terms<S: Value>(&self, args: &[S]) -> Vec<T<'h>> {
        let numbers: Vec<T<'h>> = args.iter().map(|a| self.ar.number(&a.to_number())).collect();
        self.f.terms(&numbers)
    }
}

/// Values of numbers: `false` unless every value is a finite value of the type.
fn values<S: Value>(nums: &[Num], out: &mut [S]) -> bool {
    for (o, n) in out.iter_mut().zip(nums) {
        match S::from_num(n) {
            Some(v) if v.finite() => *o = v,
            _ => return false,
        }
    }
    true
}

impl<'h, S: Value> NumFn<'h, S> for Machine<'_, 'h> {
    fn eval(&mut self, cx: &mut NativeCx<'_, '_, 'h>, args: &[S], out: &mut [S]) -> Res<bool> {
        let terms = self.terms(args);
        let Some(nums) = self.f.eval_terms(cx, self.ar, &terms)? else { return Ok(false) };
        Ok(values(&nums, out))
    }

    fn eval_batch(&mut self, cx: &mut NativeCx<'_, '_, 'h>, points: &[Vec<S>], outs: &mut [Vec<S>]) -> Res<bool> {
        let calls: Vec<T<'h>> = points.iter().map(|p| self.f.call(&self.terms(p))).collect();
        self.f.calls += calls.len() as u64;
        // the cost estimate is unknown before the first evaluations (0 lets the evaluator measure it)
        let values_ = cx.group(calls.len(), 0, &|cx, i| cx.eval(calls[i]))?;
        for (v, o) in values_.into_iter().zip(outs.iter_mut()) {
            let Some(nums) = self.f.read(self.ar, v) else { return Ok(false) };
            if !values(&nums, o) {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn eval_flat(&mut self, cx: &mut NativeCx<'_, '_, 'h>, args: &[S], out: &mut [S]) -> Res<bool> {
        let (ni, no) = (self.f.inputs(), self.f.outputs());
        let points: Vec<Vec<S>> = if ni == 0 { vec![vec![]; out.len() / no.max(1)] } else { args.chunks(ni).map(|c| c.to_vec()).collect() };
        if points.len() * no != out.len() || points.is_empty() {
            return Ok(points.is_empty() && out.is_empty());
        }
        let mut outs: Vec<Vec<S>> = vec![out[..no].to_vec(); points.len()];
        if !self.eval_batch(cx, &points, &mut outs)? {
            return Ok(false);
        }
        for (dst, o) in out.chunks_mut(no).zip(outs) {
            dst.clone_from_slice(&o);
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

/// The interpreted backend owning its closure and arithmetic: an expression in given variables, evaluated by the
/// evaluator on any number type.
pub struct Interpreted<'h> {
    /// the closure (its `calls` count the evaluations)
    pub f: TermFn<'h>,
    ar: Arith<'h, Heap>,
    numeric: bool,
}

impl<'h> Interpreted<'h> {
    /// `expr` in the variables `vars`, prepared by `prep` (decision N0.5: `Prep::Machine` gives it to `n` at
    /// machine precision, `Prep::Digits(d)` to `n(_, d)`; constant indices `y[k]` of vector variables survive).
    pub fn new(cx: &mut NativeCx<'_, '_, 'h>, expr: T<'h>, vars: &[Var], prep: Prep) -> Res<Interpreted<'h>> {
        let h = cx.heap();
        let body = prepared(cx, expr, vars, prep)?;
        let numeric = body.is_some();
        let ar = Arith::new(h).with_stop(cx.stop_flag().clone());
        Ok(Interpreted { f: TermFn::with_vars(h, vars, body.unwrap_or(expr)), ar, numeric })
    }

    /// Whether the body went through `n` (so its constants are numbers; a failure of `n` leaves it as given).
    pub fn numeric(&self) -> bool {
        self.numeric
    }

    /// The closure as it is evaluated (after the preparation).
    pub fn body(&self) -> T<'h> {
        self.f.body()
    }

    /// The arithmetic it builds number terms with.
    pub fn arith(&self) -> &Arith<'h, Heap> {
        &self.ar
    }
}

impl<'h, S: Value> NumFn<'h, S> for Interpreted<'h> {
    fn eval(&mut self, cx: &mut NativeCx<'_, '_, 'h>, args: &[S], out: &mut [S]) -> Res<bool> {
        Machine { f: &mut self.f, ar: &self.ar }.eval(cx, args, out)
    }
    fn eval_batch(&mut self, cx: &mut NativeCx<'_, '_, 'h>, points: &[Vec<S>], outs: &mut [Vec<S>]) -> Res<bool> {
        Machine { f: &mut self.f, ar: &self.ar }.eval_batch(cx, points, outs)
    }
    fn eval_flat(&mut self, cx: &mut NativeCx<'_, '_, 'h>, args: &[S], out: &mut [S]) -> Res<bool> {
        Machine { f: &mut self.f, ar: &self.ar }.eval_flat(cx, args, out)
    }
    fn inputs(&self) -> usize {
        self.f.inputs()
    }
    fn outputs(&self) -> usize {
        self.f.outputs()
    }
    fn backend(&self) -> Backend {
        Backend::Interpreted
    }
}

/// The gradient of a scalar function by central differences (steps of about eps^(1/3) relative), the 2n values
/// evaluated as one batch: BFGS without a symbolic gradient.
pub struct CentralGradient<F> {
    /// the function
    pub f: F,
    /// the number of variables
    pub n: usize,
}

impl<'h, F: NumFn<'h, f64>> NumFn<'h, f64> for CentralGradient<F> {
    fn eval(&mut self, cx: &mut NativeCx<'_, '_, 'h>, args: &[f64], out: &mut [f64]) -> Res<bool> {
        let steps: Vec<f64> = args.iter().map(|v| f64::EPSILON.cbrt() * v.abs().max(1.0)).collect();
        let mut points = Vec::with_capacity(2 * self.n);
        for (i, &hstep) in steps.iter().enumerate() {
            for sign in [1.0, -1.0] {
                let mut p = args.to_vec();
                p[i] += sign * hstep;
                points.push(p);
            }
        }
        let mut outs = vec![vec![0.0]; 2 * self.n];
        if !self.f.eval_batch(cx, &points, &mut outs)? {
            return Ok(false);
        }
        for (i, o) in out.iter_mut().enumerate().take(self.n) {
            *o = (outs[2 * i][0] - outs[2 * i + 1][0]) / (2.0 * steps[i]);
        }
        Ok(out.iter().all(|v| v.is_finite()))
    }

    fn inputs(&self) -> usize {
        self.n
    }

    fn outputs(&self) -> usize {
        self.n
    }

    fn backend(&self) -> Backend {
        self.f.backend()
    }
}
