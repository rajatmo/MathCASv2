//! MathCAS v2 numeric functions (numerics roadmap, item N0, decisions NUM.1 and NUM.2): the one trait
//! [`NumFn`] through which every numeric solver (roots, integrals, differential equations) sees the user's
//! function, generic over the number type, with three backends:
//!
//! - **interpreted** ([`Interpreted`], [`Machine`]): the expression as a closure applied by the evaluator, for any
//!   number type [`Value`] (`f64`, complex [`C64`](mc_numeric::machine::C64), and [`Num`](mc_arith::Num), which
//!   covers balls at any precision);
//! - **compiled** ([`Compiled`]): machine code from `mc-jit`, machine reals only;
//! - **batch**: [`NumFn::eval_batch`] and [`NumFn::eval_flat`] evaluate many points per call (the interpreted
//!   backend as a speculative group, the compiled one in a loop of direct calls).
//!
//! [`compile_expr`] builds an [`Auto`]: compiled when the compiler accepts the expression, interpreted otherwise,
//! and every call the compiled code declines evaluated by the interpreter (decision 1b.3), so results never depend
//! on the compiler.
//!
//! Compiled code comes from a process-wide cache (item N1, [`cache_stats`]): bodies that differ only in their
//! machine-real constants form a *family* compiled once, with the constants lifted to parameters, so a solver
//! called once per parameter value compiles once ([`Policy::Family`], the default).
//!
//! ```text
//! // inside a native: f(t, y) = [y[2], -y[1]] for an ODE right-hand side
//! let vars = [Var::scalar(t), Var::vector(y, 2)];
//! let mut f = mc_numfn::compile_expr(cx, rhs, &vars, &Options::default())?;
//! let mut dy = [0.0; 2];
//! if f.eval(cx, &[0.0, 1.0, 0.5], &mut dy)? { ... }      // inputs flat: t, y[1], y[2]
//! ```
//!
//! Decisions of this crate are numbered N0.1, N0.2, ... (shared with the compiler extensions in `mc-jit`) and
//! N1.1, N1.2, ... (the code cache).
#![warn(missing_docs)]
mod cache;
mod compiled;
mod interp;
pub mod par;
mod prep;
mod term;
mod value;

pub use cache::{cache_stats, clear_cache, CacheStats, CAPACITY};
pub use compiled::{Auto, Compiled, Policy, DEFAULT_AFTER};
pub use interp::{CentralGradient, Interpreted, Machine};
pub use par::{par_expr, Par, ParStats, PAR_MIN_POINTS};
pub use prep::{prepare, replace};
pub use term::{leaves, TermFn};
pub use value::{all_finite, norm, norm2, Scalar, Value};

use mc_core::{Heap, Store, Sym, T};
use mc_eval::{NativeCx, Res};

/// A vector-valued function of numbers of type `S` (decision NUM.1): inputs and outputs are flat slices of
/// scalars (a vector variable of length n takes n consecutive inputs; a list result gives its leaves in row-major
/// order).
pub trait NumFn<'h, S> {
    /// Evaluate at `args`, writing every output into `out`; `Ok(false)` when a value is not a finite number of
    /// the type (a complex value for real scalars, an unevaluated call, a failure, an infinity or NaN).
    fn eval(&mut self, cx: &mut NativeCx<'_, '_, 'h>, args: &[S], out: &mut [S]) -> Res<bool>;

    /// Evaluate at independent points (`outs[i]` for `points[i]`); `Ok(false)` if any value is not numeric. The
    /// default evaluates one after the other.
    fn eval_batch(&mut self, cx: &mut NativeCx<'_, '_, 'h>, points: &[Vec<S>], outs: &mut [Vec<S>]) -> Res<bool> {
        for (p, o) in points.iter().zip(outs.iter_mut()) {
            if !self.eval(cx, p, o)? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// Evaluate at `out.len() / outputs()` points stored one after the other in `args` (each `inputs()`
    /// scalars), writing their outputs one after the other into `out`; `Ok(false)` if any value is not numeric
    /// or the lengths do not fit. The default evaluates one after the other.
    fn eval_flat(&mut self, cx: &mut NativeCx<'_, '_, 'h>, args: &[S], out: &mut [S]) -> Res<bool> {
        let (ni, no) = (self.inputs(), self.outputs());
        if no == 0 || !out.len().is_multiple_of(no) || (ni > 0 && args.len() != ni * (out.len() / no)) {
            return Ok(false);
        }
        for k in 0..out.len() / no {
            if !self.eval(cx, &args[k * ni..(k + 1) * ni], &mut out[k * no..(k + 1) * no])? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// The number of scalar inputs.
    fn inputs(&self) -> usize;

    /// The number of scalar outputs.
    fn outputs(&self) -> usize;

    /// The backend that runs calls (an [`Auto`] reports [`Backend::Compiled`] once it has compiled code).
    fn backend(&self) -> Backend {
        Backend::Interpreted
    }

    /// Counters of compiled, declined and interpreted evaluations (zero for backends that keep none).
    fn stats(&self) -> Stats {
        Stats::default()
    }
}

impl<'h, S, F: NumFn<'h, S> + ?Sized> NumFn<'h, S> for Box<F> {
    fn eval(&mut self, cx: &mut NativeCx<'_, '_, 'h>, args: &[S], out: &mut [S]) -> Res<bool> {
        (**self).eval(cx, args, out)
    }
    fn eval_batch(&mut self, cx: &mut NativeCx<'_, '_, 'h>, points: &[Vec<S>], outs: &mut [Vec<S>]) -> Res<bool> {
        (**self).eval_batch(cx, points, outs)
    }
    fn eval_flat(&mut self, cx: &mut NativeCx<'_, '_, 'h>, args: &[S], out: &mut [S]) -> Res<bool> {
        (**self).eval_flat(cx, args, out)
    }
    fn inputs(&self) -> usize {
        (**self).inputs()
    }
    fn outputs(&self) -> usize {
        (**self).outputs()
    }
    fn backend(&self) -> Backend {
        (**self).backend()
    }
    fn stats(&self) -> Stats {
        (**self).stats()
    }
}

/// Which backend evaluates.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Backend {
    /// the evaluator
    Interpreted,
    /// machine code from `mc-jit` (with the evaluator for declined calls)
    Compiled,
}

/// Evaluation counters of an [`Auto`].
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Stats {
    /// evaluations the compiled code completed
    pub compiled: u64,
    /// evaluations the compiled code declined (each then interpreted)
    pub declined: u64,
    /// evaluations by the interpreted backend (declined ones included)
    pub interpreted: u64,
    /// the compiler's time, in microseconds (0 when nothing was compiled, also when the code came from the cache)
    pub compile_micros: u64,
    /// 1 when the code cache held the expression's family (compiled code, or the compiler's refusal)
    pub cache_hits: u64,
    /// 1 when the code cache had nothing for the expression's family (compiled later as the policy says)
    pub cache_misses: u64,
}

/// The kind of a variable.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Arg {
    /// one number
    Scalar,
    /// a list of this many numbers, read in the body by constant indices `y[k]` (and `length(y)`)
    Vector(usize),
}

impl Arg {
    /// The number of scalar inputs it takes.
    pub fn words(self) -> usize {
        match self {
            Arg::Scalar => 1,
            Arg::Vector(n) => n,
        }
    }
}

/// A variable of the function: a symbol and its kind.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Var {
    /// the symbol the body uses
    pub sym: Sym,
    /// scalar or vector
    pub arg: Arg,
}

impl Var {
    /// A scalar variable.
    pub fn scalar(sym: Sym) -> Var {
        Var { sym, arg: Arg::Scalar }
    }
    /// A vector variable of length `n`.
    pub fn vector(sym: Sym, n: usize) -> Var {
        Var { sym, arg: Arg::Vector(n) }
    }
}

/// How the body is prepared before evaluation (decision N0.5).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Prep {
    /// as given
    None,
    /// through `n` at machine precision (constants become machine reals; `mc-roots`' decision R.9)
    Machine,
    /// through `n(_, d)`: constants as balls of `d` digits
    Digits(u64),
}

/// The options of [`compile_expr`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Options {
    /// when to compile (default: [`Policy::default`])
    pub policy: Policy,
    /// give the body to `n` first (default `true`; then bare `pi` and `e` also compile, decision N0.1)
    pub numeric: bool,
    /// the number of scalar outputs, when the body computes its list instead of writing a list literal (a call of
    /// a function returning `[...]`); default: counted from the body's list literals ([`TermFn::set_outputs`])
    pub outputs: Option<usize>,
}

impl Default for Options {
    fn default() -> Options {
        Options { policy: Policy::default(), numeric: true, outputs: None }
    }
}

/// The function `expr` of `vars` at machine reals (decision NUM.2): an [`Auto`] over the [`Interpreted`]
/// backend, compiled as `opts.policy` says, falling back to the interpreter for every call the compiled code
/// declines and entirely when the compiler refuses. Errors are only the evaluator's aborts (interrupts, limits)
/// while preparing the body.
pub fn compile_expr<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    expr: T<'h>,
    vars: &[Var],
    opts: &Options,
) -> Res<Box<dyn NumFn<'h, f64> + 'h>> {
    Ok(Box::new(auto_expr(cx, expr, vars, opts)?))
}

/// [`compile_expr`] with the concrete type (for [`Auto::refusal`], [`Auto::compiled`]).
pub fn auto_expr<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    expr: T<'h>,
    vars: &[Var],
    opts: &Options,
) -> Res<Auto<'h, Interpreted<'h>>> {
    let mut interp = Interpreted::new(cx, expr, vars, if opts.numeric { Prep::Machine } else { Prep::None })?;
    if let Some(n) = opts.outputs {
        interp.f.set_outputs(n);
    }
    let (body, numeric) = (interp.body(), interp.numeric());
    Ok(Auto::new(cx, interp, vars, body, numeric, opts.policy))
}

/// The variables and body of a function value with `args` arguments of the kinds `kinds`: a closure
/// `(t, y) => ...` gives its own (decision N0.4); any other function (a symbol naming a defined function, say)
/// gives fresh variables and the body `f(v1, ..., vn)`, whose definition the compiler can inline.
pub fn function_parts<'h>(h: &'h Heap, f: T<'h>, kinds: &[Arg]) -> Option<(Vec<Var>, T<'h>)> {
    if let Some((ps, body)) = mc_jit::closure_parts(h, f) {
        if ps.len() != kinds.len() {
            return None;
        }
        let admitted = ps.iter().zip(kinds).all(|((_, ty), &a)| {
            ty.as_deref().is_none_or(|t| mc_jit::expr::admits(t, if a == Arg::Scalar { mc_jit::Arg::Real } else { mc_jit::Arg::Vector(a.words()) }))
        });
        if admitted {
            let vars = ps.iter().zip(kinds).map(|((s, _), &a)| Var { sym: *s, arg: a }).collect();
            return Some((vars, body));
        }
        // a declared type the arguments may not have: the closure is applied as it is (the interpreter checks)
    } else if matches!(h.view(f), mc_core::View::App { head, .. } if head == h.sym_term(h.sym("fn"))) {
        // a closure of a form the compiler does not read (rest or optional parameters)
        return None;
    }
    let vars: Vec<Var> = kinds
        .iter()
        .enumerate()
        .map(|(i, &a)| Var { sym: h.sym(&format!("numfn·arg·{}", i + 1)), arg: a })
        .collect();
    let args: Vec<T<'h>> = vars.iter().map(|v| h.sym_term(v.sym)).collect();
    Some((vars, h.app(f, &args)))
}

/// [`compile_expr`] for a function value `f` (a closure or a function name) of arguments of the kinds `kinds`;
/// `None` when `f` is a closure whose parameters cannot be read (rest or optional ones) or of another arity.
pub fn compile_fn<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    f: T<'h>,
    kinds: &[Arg],
    opts: &Options,
) -> Res<Option<Box<dyn NumFn<'h, f64> + 'h>>> {
    let Some((vars, body)) = function_parts(cx.heap(), f, kinds) else { return Ok(None) };
    Ok(Some(compile_expr(cx, body, &vars, opts)?))
}
