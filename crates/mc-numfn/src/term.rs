//! The user's expression as a closure over its variables, applied by the evaluator (the shared convention of phase
//! 4A, moved from `mc-roots`): the expression in named variables becomes `fn([x1, ..., xn], body)`, applied to
//! number terms (a list term for a vector variable). A body that is a (nested) list gives its leaves in row-major
//! order, so one closure computes a whole residual vector or Jacobian.
use crate::{Arg, Var};
use mc_api::names;
use mc_arith::{Arith, Num};
use mc_core::{Heap, Store, View, T};
use mc_eval::{NativeCx, Res};

/// The expression (or its derivative) as a closure over its variables.
pub struct TermFn<'h> {
    heap: &'h Heap,
    /// `fn(list(x1, ..., xn), body)`
    closure: T<'h>,
    body: T<'h>,
    vars: Vec<Var>,
    /// the number of scalar leaves of the body
    outputs: usize,
    list: T<'h>,
    /// evaluations so far (for the tests and measurements)
    pub calls: u64,
}

impl<'h> TermFn<'h> {
    /// The closure of `body` in the scalar variables `vars`.
    pub fn new(h: &'h Heap, vars: &[mc_core::Sym], body: T<'h>) -> TermFn<'h> {
        let vs: Vec<Var> = vars.iter().map(|&s| Var::scalar(s)).collect();
        TermFn::with_vars(h, &vs, body)
    }

    /// The closure of `body` in `vars`, scalars and vectors of known length.
    pub fn with_vars(h: &'h Heap, vars: &[Var], body: T<'h>) -> TermFn<'h> {
        let params: Vec<T<'h>> = vars.iter().map(|v| h.sym_term(v.sym)).collect();
        let list = h.sym_term(h.sym(names::LIST));
        let closure = h.call(h.sym("fn"), &[h.app(list, &params), body]);
        let outputs = leaves(h, list, body).len();
        TermFn { heap: h, closure, body, vars: vars.to_vec(), outputs, list, calls: 0 }
    }

    /// The number of scalar outputs.
    pub fn outputs(&self) -> usize {
        self.outputs
    }

    /// Declare the number of scalar outputs. They are counted from the body's list literals, which a body that
    /// computes its list (a call of a function returning one, a block ending in one) does not show; a value of
    /// another shape is not numeric.
    pub fn set_outputs(&mut self, n: usize) {
        self.outputs = n;
    }

    /// The number of scalar inputs (a vector variable of length n counts n).
    pub fn inputs(&self) -> usize {
        self.vars.iter().map(|v| v.arg.words()).sum()
    }

    /// The variables.
    pub fn vars(&self) -> &[Var] {
        &self.vars
    }

    /// The body.
    pub fn body(&self) -> T<'h> {
        self.body
    }

    /// The closure applied to `args` (one term per variable, a list term for a vector), unevaluated.
    pub fn call(&self, args: &[T<'h>]) -> T<'h> {
        self.heap.app(self.closure, args)
    }

    /// One term per variable from the scalar inputs `flat` (vectors gathered into list terms).
    pub fn terms(&self, numbers: &[T<'h>]) -> Vec<T<'h>> {
        if self.vars.iter().all(|v| v.arg == Arg::Scalar) {
            return numbers.to_vec();
        }
        let mut out = Vec::with_capacity(self.vars.len());
        let mut at = 0;
        for v in &self.vars {
            match v.arg {
                Arg::Scalar => {
                    out.push(numbers[at]);
                    at += 1;
                }
                Arg::Vector(n) => {
                    out.push(self.heap.app(self.list, &numbers[at..at + n]));
                    at += n;
                }
            }
        }
        out
    }

    /// The outputs of an evaluated call as numbers, if every leaf is one and the shape is the body's.
    pub fn read(&self, ar: &Arith<'h, Heap>, value: T<'h>) -> Option<Vec<Num>> {
        let ls = leaves(self.heap, self.list, value);
        if ls.len() != self.outputs {
            return None;
        }
        ls.into_iter().map(|t| ar.num(t)).collect()
    }

    /// Evaluate at the terms `args` (one per variable): the outputs as numbers, or `None`.
    pub fn eval_terms(
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
pub fn leaves<'h>(h: &'h Heap, list: T<'h>, t: T<'h>) -> Vec<T<'h>> {
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
