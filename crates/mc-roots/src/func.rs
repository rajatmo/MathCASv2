//! Evaluating the user's function at numeric points (the shared convention of phase 4A): the expression in named
//! variables becomes the closure `fn([x1, ..., xn], body)`, applied by the evaluator to number terms. A body that is
//! a (nested) list gives its leaves in row-major order, so one closure computes a whole residual vector or Jacobian.
//!
//! The iterations see the function through [`NumFn`], so a compiled backend (`mc-jit`) can be added behind the same
//! trait later. A value that is not a number of the expected kind is reported as `false`, never as an error.
use crate::scalar::Scalar;
use mc_api::names;
use mc_arith::{Arith, Num};
use mc_core::{Heap, Store, Sym, View, T};
use mc_eval::{NativeCx, Res};

/// A vector-valued function of machine scalars.
pub(crate) trait NumFn<'h, S: Scalar> {
    /// Evaluate at `args`, writing every output into `out`; `Ok(false)` when a value is not a number of the scalar
    /// kind (a complex value for real scalars, an unevaluated call, a failure).
    fn eval(&mut self, cx: &mut NativeCx<'_, '_, 'h>, args: &[S], out: &mut [S]) -> Res<bool>;

    /// Evaluate at independent points (`outs[i]` for `points[i]`); `Ok(false)` if any value is not numeric. The
    /// default evaluates one after the other; [`Machine`] evaluates the batch as a speculative group.
    fn eval_batch(&mut self, cx: &mut NativeCx<'_, '_, 'h>, points: &[Vec<S>], outs: &mut [Vec<S>]) -> Res<bool> {
        for (p, o) in points.iter().zip(outs.iter_mut()) {
            if !self.eval(cx, p, o)? {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

/// The user's expression (or its derivative) as a closure over its variables.
pub(crate) struct TermFn<'h> {
    heap: &'h Heap,
    /// `fn(list(x1, ..., xn), body)`
    closure: T<'h>,
    /// the number of scalar leaves of the body
    outputs: usize,
    list: T<'h>,
    /// evaluations so far (for the tests and measurements)
    pub(crate) calls: u64,
}

impl<'h> TermFn<'h> {
    /// The closure of `body` in the variables `vars`.
    pub(crate) fn new(h: &'h Heap, vars: &[Sym], body: T<'h>) -> TermFn<'h> {
        let params: Vec<T<'h>> = vars.iter().map(|&v| h.sym_term(v)).collect();
        let list = h.sym_term(h.sym(names::LIST));
        let closure = h.call(h.sym("fn"), &[h.app(list, &params), body]);
        let outputs = leaves(h, list, body).len();
        TermFn { heap: h, closure, outputs, list, calls: 0 }
    }

    /// The number of scalar outputs.
    pub(crate) fn outputs(&self) -> usize {
        self.outputs
    }

    /// The closure applied to the number terms `args`, unevaluated.
    pub(crate) fn call(&self, args: &[T<'h>]) -> T<'h> {
        self.heap.app(self.closure, args)
    }

    /// The outputs of an evaluated call as numbers, if every leaf is one and the shape is the body's.
    pub(crate) fn read(&self, ar: &Arith<'h, Heap>, value: T<'h>) -> Option<Vec<Num>> {
        let ls = leaves(self.heap, self.list, value);
        if ls.len() != self.outputs {
            return None;
        }
        ls.into_iter().map(|t| ar.num(t)).collect()
    }

    /// Evaluate at the number terms `args`: the outputs as numbers, or `None`.
    pub(crate) fn eval_terms(
        &mut self,
        cx: &mut NativeCx<'_, '_, 'h>,
        ar: &Arith<'h, Heap>,
        args: &[T<'h>],
    ) -> Res<Option<Vec<Num>>> {
        self.calls += 1;
        let v = cx.eval(self.call(args))?;
        Ok(self.read(ar, v))
    }
}

/// The leaves of the list structure of `t` in row-major order (`t` itself when it is not a list).
fn leaves<'h>(h: &'h Heap, list: T<'h>, t: T<'h>) -> Vec<T<'h>> {
    // an explicit stack: the structure is user data
    let mut out = Vec::new();
    let mut stack = vec![t];
    while let Some(u) = stack.pop() {
        match h.view(u) {
            View::App { head, args } if head == list => stack.extend(args.iter().rev()),
            _ => out.push(u),
        }
    }
    out
}

/// A [`TermFn`] with the arithmetic it builds number terms with: the machine-scalar view of the closure.
pub(crate) struct Machine<'r, 'h> {
    pub(crate) f: &'r mut TermFn<'h>,
    pub(crate) ar: &'r Arith<'h, Heap>,
}

impl<'h> Machine<'_, 'h> {
    fn terms<S: Scalar>(&self, args: &[S]) -> Vec<T<'h>> {
        args.iter().map(|a| self.ar.number(&a.to_num())).collect()
    }
}

/// Machine scalars of numbers: `false` unless every value is a finite scalar of the kind.
fn scalars<S: Scalar>(nums: &[Num], out: &mut [S]) -> bool {
    for (o, n) in out.iter_mut().zip(nums) {
        match S::of_num(n) {
            Some(v) if v.is_finite() => *o = v,
            _ => return false,
        }
    }
    true
}

impl<'h, S: Scalar> NumFn<'h, S> for Machine<'_, 'h> {
    fn eval(&mut self, cx: &mut NativeCx<'_, '_, 'h>, args: &[S], out: &mut [S]) -> Res<bool> {
        let terms = self.terms(args);
        let Some(nums) = self.f.eval_terms(cx, self.ar, &terms)? else { return Ok(false) };
        Ok(scalars(&nums, out))
    }

    fn eval_batch(&mut self, cx: &mut NativeCx<'_, '_, 'h>, points: &[Vec<S>], outs: &mut [Vec<S>]) -> Res<bool> {
        let calls: Vec<T<'h>> = points.iter().map(|p| self.f.call(&self.terms(p))).collect();
        self.f.calls += calls.len() as u64;
        // the cost estimate is unknown before the first evaluations (0 lets the evaluator measure it)
        let values = cx.group(calls.len(), 0, &|cx, i| cx.eval(calls[i]))?;
        for (v, o) in values.into_iter().zip(outs.iter_mut()) {
            let Some(nums) = self.f.read(self.ar, v) else { return Ok(false) };
            if !scalars(&nums, o) {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

/// The gradient of a scalar function by central differences (steps of about eps^(1/3) relative), the 2n values
/// evaluated as one batch: BFGS without a symbolic gradient.
pub(crate) struct CentralGradient<'r, 'h> {
    /// the function
    pub(crate) f: Machine<'r, 'h>,
    /// the number of variables
    pub(crate) n: usize,
}

impl<'h> NumFn<'h, f64> for CentralGradient<'_, 'h> {
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
        if !<Machine<'_, 'h> as NumFn<'h, f64>>::eval_batch(&mut self.f, cx, &points, &mut outs)? {
            return Ok(false);
        }
        for (i, o) in out.iter_mut().enumerate().take(self.n) {
            *o = (outs[2 * i][0] - outs[2 * i + 1][0]) / (2.0 * steps[i]);
        }
        Ok(out.iter().all(|v| v.is_finite()))
    }
}
