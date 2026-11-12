//! The native functions: one [`Native`] per name, a bridge from the kernels' work budget to the evaluator's fuel
//! and interrupt, and the dispatch to the function modules.
use crate::tensor::Env;
use crate::{decomp, eigen, ops, systems};
use mc_api::names;
use mc_arith::num::StopScope;
use mc_arith::Arith;
use mc_api::{Store, View};
use mc_core::{Heap, T};
use mc_eval::{Abort, Native, NativeCx, Reply, Res};
use mc_poly::work::{Halt, Meter, Work, WorkResult};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// What a function gives back.
pub enum Out<'h> {
    /// a normal form
    Done(T<'h>),
    /// a term for the evaluator to evaluate (calls of other functions: `d`, `abs`, `max`)
    Again(T<'h>),
    /// not this function's business: the call stays unevaluated
    Decline,
}

/// The functions of this crate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fun {
    Det,
    Inverse,
    PseudoInverse,
    Rank,
    NullSpace,
    RowReduce,
    LinearSolve,
    Eigenvalues,
    Eigenvectors,
    Eigensystem,
    CharacteristicPolynomial,
    IdentityMatrix,
    DiagonalMatrix,
    Dot,
    Cross,
    KroneckerProduct,
    Trace,
    Norm,
    Lu,
    Qr,
    Cholesky,
    Svd,
    SingularValues,
    Grad,
    Div,
    Curl,
    Laplacian,
}

/// One native function.
pub struct LinalgNative {
    /// which function
    pub f: Fun,
    /// its name
    pub name: &'static str,
    /// the flag `mc-arith` polls in long numeric loops
    pub stop: Arc<AtomicBool>,
}

/// Charges the kernels' work to the evaluator (fuel and interrupt) and polls the arithmetic stop flag.
struct Tick<'c, 'a, 'k, 'h> {
    cx: &'c mut NativeCx<'a, 'k, 'h>,
    stop: &'c AtomicBool,
    abort: Option<Abort>,
}

impl Work for Tick<'_, '_, '_, '_> {
    fn charge(&mut self, units: u64) -> WorkResult<()> {
        if self.stop.load(Ordering::Relaxed) {
            // raised with the evaluator's interrupt, which the fuel poll has not seen yet: the stop is the abort (a
            // caller that sees no abort tries its next method with the number kernels returning arbitrary values)
            if self.cx.stopped() {
                self.abort = Some(mc_eval::Abort::Limit);
            }
            return Err(Halt::Stopped);
        }
        match self.cx.tick(units) {
            Ok(()) => Ok(()),
            Err(a) => {
                self.abort = Some(a);
                Err(Halt::Stopped)
            }
        }
    }
}

/// Does an argument (a scalar, vector or matrix, nested lists of them) hold an `around`, an interval or a quantity?
fn uncertain_entries<'h>(h: &'h Heap, args: &[T<'h>]) -> bool {
    let list = h.sym_term(h.sym(names::LIST));
    let mut stack: Vec<T<'h>> = args.to_vec();
    let mut budget = 100_000usize;
    while let Some(t) = stack.pop() {
        if h.packed(t).is_some() {
            continue;
        }
        match h.view(t) {
            View::App { head, args } if head == list => {
                if budget < args.len() {
                    return false;
                }
                budget -= args.len();
                stack.extend_from_slice(args);
            }
            View::App { .. } if mc_numeric::uncertain::any(h, &[t]) => return true,
            _ => {}
        }
    }
    false
}

/// Does a matrix argument hold a `quantity` entry?
fn quantity_entries<'h>(h: &'h Heap, args: &[T<'h>]) -> bool {
    let list = h.sym_term(h.sym(names::LIST));
    let quantity = h.sym_term(h.sym("quantity"));
    let Some(&m) = args.first() else { return false };
    let View::App { head, args: rows } = h.view(m) else { return false };
    head == list
        && rows.iter().take(10_000).any(|&r| match h.view(r) {
            View::App { head, args } if head == list => args.iter().any(|&x| matches!(h.view(x), View::App { head, .. } if head == quantity)),
            _ => false,
        })
}

impl Native for LinalgNative {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        // eigenvalues(m, k), eigenvectors(m, k), eigensystem(m, k): the first k (k < 0: the last -k), in the order
        // of decreasing modulus
        if let (Fun::Eigenvalues | Fun::Eigenvectors | Fun::Eigensystem, [m, k]) = (self.f, args) {
            let up_to = |k: T<'h>| matches!(h.view(k), View::App { head, args: &[q] } if head == h.sym_term(h.sym("up_to")) && matches!(h.view(q), View::Int(v) if v > 0));
            if matches!(h.view(*k), View::Int(v) if v != 0) || up_to(*k) {
                let call = |name: &str, a: &[T<'h>]| h.call(h.sym(name), a);
                let all = call(self.name, &[*m]);
                return Ok(Reply::Again(if self.f == Fun::Eigensystem {
                    call("transpose", &[call("take", &[call("transpose", &[all]), *k])])
                } else {
                    call("take", &[all, *k])
                }));
            }
        }
        // eliminations that decide pivots by zero tests: quantity entries (whose zero `quantity(0, u)` is not the
        // number 0) go to the library rules on the magnitudes (lib/lists.mc), or stay unevaluated
        if matches!(self.f, Fun::RowReduce | Fun::NullSpace | Fun::PseudoInverse | Fun::Rank) && quantity_entries(h, args) {
            return Ok(Reply::Decline);
        }
        let env = Env::new(Arith::new(h).with_stop(self.stop.clone()), self.name);
        let _scope = StopScope::enter(Some(&self.stop));
        let mut tick = Tick { cx, stop: &self.stop, abort: None };
        let r = {
            let mut m = Meter::new(&mut tick);
            dispatch(self.f, &env, args, &mut m)
        };
        match r {
            // numbers with uncertainty and quantities among the entries: the result is evaluated again, so that
            // their arithmetic (left as canonical products and sums here) is carried out
            Ok(Out::Done(t)) if uncertain_entries(h, args) => Ok(Reply::Again(t)),
            Ok(Out::Done(t)) => Ok(Reply::Done(t)),
            Ok(Out::Again(t)) => Ok(Reply::Again(t)),
            Ok(Out::Decline) => Ok(Reply::Decline),
            Err(Halt::Stopped) => match tick.abort {
                Some(a) => Err(a),
                None => Ok(Reply::Done(env.call(names::FAILURE, &[env.sym(names::failure::INTERRUPTED)]))),
            },
            Err(Halt::Memory) => {
                let tags = [env.sym(names::failure::LIMIT), env.sym(names::failure::MEMORY)];
                Ok(Reply::Done(env.call(names::FAILURE, &tags)))
            }
        }
    }

    fn reads_term(&self) -> bool {
        false
    }
}

fn dispatch<'h>(f: Fun, env: &Env<'h>, args: &[T<'h>], m: &mut Meter) -> WorkResult<Out<'h>> {
    match f {
        Fun::Det => systems::det(env, args, m),
        Fun::Inverse => systems::inverse(env, args, m),
        Fun::PseudoInverse => systems::pseudo_inverse(env, args, m),
        Fun::Rank => systems::rank(env, args, m),
        Fun::NullSpace => systems::null_space(env, args, m),
        Fun::RowReduce => systems::row_reduce(env, args, m),
        Fun::LinearSolve => systems::linear_solve(env, args, m),
        Fun::Eigenvalues => eigen::eigenvalues(env, args, m),
        Fun::Eigenvectors => eigen::eigenvectors(env, args, m),
        Fun::Eigensystem => eigen::eigensystem(env, args, m),
        Fun::CharacteristicPolynomial => eigen::characteristic_polynomial(env, args, m),
        Fun::IdentityMatrix => ops::identity_matrix(env, args),
        Fun::DiagonalMatrix => ops::diagonal_matrix(env, args),
        Fun::Dot => ops::dot(env, args, m),
        Fun::Cross => ops::cross(env, args),
        Fun::KroneckerProduct => ops::kronecker_product(env, args, m),
        Fun::Trace => ops::trace(env, args),
        Fun::Norm => ops::norm(env, args, m),
        Fun::Lu => decomp::lu(env, args, m),
        Fun::Qr => decomp::qr(env, args, m),
        Fun::Cholesky => decomp::cholesky(env, args, m),
        Fun::Svd => decomp::svd(env, args, m),
        Fun::SingularValues => decomp::singular_values(env, args, m),
        Fun::Grad => ops::grad(env, args),
        Fun::Div => ops::div(env, args),
        Fun::Curl => ops::curl(env, args),
        Fun::Laplacian => ops::laplacian(env, args),
    }
}

/// `normal(numeric_array(xs, type[, mode]))`: the list of the entries as the type stores them; other calls go to the
/// native `normal` installed before.
pub struct NormalNumericArray {
    /// the native `normal` installed earlier
    pub inner: Box<dyn Native>,
}

impl Native for NormalNumericArray {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        if let [x] = args {
            if let View::App { head, args: parts } = h.view(*x) {
                if matches!(h.view(head), View::Sym(s) if h.sym_name(s) == "numeric_array") && (2..=3).contains(&parts.len()) {
                    // the prelude's `numeric_array_values(xs, type[, mode])` applies the type's range and rounding
                    return Ok(Reply::Again(h.call(h.sym("numeric_array_values"), parts)));
                }
            }
        }
        self.inner.call(cx, t, args)
    }
    fn trace(&mut self, visit: &mut dyn FnMut(&mut mc_core::Raw)) {
        self.inner.trace(visit);
    }
    fn reads_term(&self) -> bool {
        self.inner.reads_term()
    }
}
