//! Compiling an *expression in given variables* from Rust (item N0, decision N0.4): the API `mc-numfn` builds
//! its compiled backend on.
//!
//! ```text
//! let f = mc_jit::compile_expr(cx, &[(t, Arg::Real), (y, Arg::Vector(2))], body, true, Backend::Cranelift)?;
//! let mut out = [0.0; 2];
//! match f.call_f64(cx, &[0.0, 1.0, 2.0], &mut out) { Call::Done => ..., Call::Declined | Call::Stopped => ... }
//! ```
//!
//! The expression is compiled as the function `(p1, ..., pn) => body` (the front end's [`front_expr`]); a closure
//! value `x => ...` or `(t, y) => ...` compiles the same way ([`compile_closure`]). Calls return
//! [`Call::Declined`] wherever a compiled specialisation would decline (decision 1b.3), and [`Call::Stopped`] when
//! a loop's poll found the evaluation stopped: the caller then evaluates the call with the interpreter, which gives
//! the result, or stops at its first poll (decision 1b.8). A call also declines when a function it inlined has been
//! redefined since it was compiled (decision 1b.6).
//!
//! An expression never draws (random draws are refused: it has no session stream) and its results are machine
//! reals and integers (a complex result is refused; complex numbers inside the expression compile). So every
//! `ExprFn` is pure, and [`ExprFn::call_detached`] may run it on several threads at once (item N4b, decision
//! N4b.5: `mc_numfn::par` splits batches across threads with it).
use crate::front::{front_expr, Arg, Shape};
use crate::ir::Ty;
use crate::rt::{Exit, PollFn};
use crate::spec::{build, Backend, Built, Info, KernelSource};
use mc_core::{Heap, Store, Sym, View, T};
use mc_eval::{Control, NativeCx};
use std::collections::HashMap;

/// How a compiled call ended.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Call {
    /// the outputs hold the result
    Done,
    /// the compiled code cannot give the interpreter's result here: evaluate the call with the interpreter
    Declined,
    /// a loop's poll found the evaluation stopped (interrupt, fuel): the interpreter reproduces the stop
    Stopped,
}

/// A compiled expression: call it with its parameters' words, read its outputs.
pub struct ExprFn {
    built: Built,
    args: Vec<Arg>,
    words: usize,
    shape: Shape,
    leaf_types: Vec<Ty>,
    /// the inlined functions with the versions read
    deps: Vec<(Sym, u32)>,
    /// a private control block: fuel and interrupts reach compiled loops through the host poll (`cx.tick`)
    ctl: Control,
}

impl ExprFn {
    /// The parameters.
    pub fn args(&self) -> &[Arg] {
        &self.args
    }

    /// The number of input words (a vector parameter of length n takes n).
    pub fn inputs(&self) -> usize {
        self.words
    }

    /// The number of outputs: 1 for a scalar result, else the leaves of the list result in row-major order.
    pub fn outputs(&self) -> usize {
        self.leaf_types.len()
    }

    /// The result's shape.
    pub fn shape(&self) -> &Shape {
        &self.shape
    }

    /// Whether the code draws random numbers: never for an expression (draws are refused), so every `ExprFn` may
    /// be called from several threads at once ([`crate::batch`]).
    pub fn draws(&self) -> bool {
        self.built.draws()
    }

    /// The types of the outputs.
    pub fn output_types(&self) -> &[Ty] {
        &self.leaf_types
    }

    /// Sizes, back end and compile time.
    pub fn info(&self) -> &Info {
        &self.built.info
    }

    /// Are the functions it inlined unchanged? (Records the reads, as every call does.)
    pub fn current(&self, cx: &mut NativeCx<'_, '_, '_>) -> bool {
        self.deps.iter().all(|&(s, v)| cx.version(s) == v)
    }

    /// Call with the inputs as words (an `Int` as `i64`, a `Real` by its bits), the outputs as words. `out` must
    /// have [`ExprFn::outputs`] words.
    pub fn call_words(&self, cx: &mut NativeCx<'_, '_, '_>, args: &[u64], out: &mut [u64]) -> Call {
        if !self.current(cx) {
            return Call::Declined;
        }
        let mut poll = |n: u64| cx.tick(n).is_ok();
        self.call_detached(&self.ctl, args, out, &mut poll)
    }

    /// Call without an evaluation context, from any thread (decision N4b.5): `ctl` is the control block whose stop
    /// flag and status the loops read, `poll` charges the loops' fuel ([`crate::rt::POLL_EVERY`] per call; `false`
    /// stops the call). The caller must have checked [`ExprFn::current`] on the evaluation's thread before (that
    /// check records the reads). Inputs and outputs as in [`ExprFn::call_words`].
    pub fn call_detached(&self, ctl: &Control, args: &[u64], out: &mut [u64], poll: &mut PollFn<'_>) -> Call {
        if args.len() != self.words || out.len() != self.outputs() {
            return Call::Declined;
        }
        let list = matches!(self.shape, Shape::List(_));
        let (r, exit) = if list {
            self.built.call_out(ctl, args, out, poll)
        } else {
            self.built.call_out(ctl, args, &mut [], poll)
        };
        match exit {
            Exit::Done => {
                if !list {
                    out[0] = r;
                }
                Call::Done
            }
            Exit::Decline => Call::Declined,
            Exit::Stopped => Call::Stopped,
        }
    }

    /// Call with machine reals without an evaluation context (decision N4b.5): [`ExprFn::call_f64`] through
    /// [`ExprFn::call_detached`], with its contract.
    pub fn call_f64_detached(&self, ctl: &Control, args: &[f64], out: &mut [f64], poll: &mut PollFn<'_>) -> Call {
        self.f64_words(args, out, |w, o| self.call_detached(ctl, w, o, poll))
    }

    /// Call with machine reals: every parameter is `Real` or a vector; the outputs are converted to machine reals
    /// (an integer result rounded to the nearest, as the arithmetic's `to_f64` does). Declines for an `Int`
    /// parameter or a boolean output.
    pub fn call_f64(&self, cx: &mut NativeCx<'_, '_, '_>, args: &[f64], out: &mut [f64]) -> Call {
        self.f64_words(args, out, |w, o| self.call_words(cx, w, o))
    }

    /// The machine reals `args` as words for `call`, and its output words as machine reals in `out`.
    fn f64_words(&self, args: &[f64], out: &mut [f64], call: impl FnOnce(&mut [u64], &mut [u64]) -> Call) -> Call {
        if self.args.contains(&Arg::Int) || out.len() != self.outputs() {
            return Call::Declined;
        }
        let mut words: [u64; 16] = [0; 16];
        let mut big = vec![];
        let w: &mut [u64] = if args.len() <= 16 {
            &mut words[..args.len()]
        } else {
            big.resize(args.len(), 0);
            &mut big
        };
        for (d, a) in w.iter_mut().zip(args) {
            *d = a.to_bits();
        }
        let mut ow: [u64; 16] = [0; 16];
        let mut obig = vec![];
        let o: &mut [u64] = if out.len() <= 16 {
            &mut ow[..out.len()]
        } else {
            obig.resize(out.len(), 0);
            &mut obig
        };
        let r = call(w, o);
        if r != Call::Done {
            return r;
        }
        for ((d, &x), t) in out.iter_mut().zip(o.iter()).zip(&self.leaf_types) {
            *d = match t {
                Ty::Real => f64::from_bits(x),
                Ty::Int => x as i64 as f64,
                Ty::Bool => return Call::Declined,
            };
        }
        Call::Done
    }
}

/// Compile `body` in the parameters `params` (decision N0.4). With `numeric`, bare `pi` and `e` in `body` are
/// their machine reals (decision N0.1: for an expression the caller evaluates as `n(body)`). The reason is given
/// when the expression is outside the compiled subset.
pub fn compile_expr<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    params: &[(Sym, Arg)],
    body: T<'h>,
    numeric: bool,
    backend: Backend,
) -> Result<ExprFn, String> {
    let h = cx.heap();
    let mut src = KernelSource { cx, shared: None, versions: HashMap::new() };
    let fr = front_expr(h, params, body, &mut src, numeric)?;
    let versions = src.versions;
    let built = build(&fr.func, fr.deps.clone(), backend)?;
    if fr.result.has_complex() {
        return Err("a complex result (expressions compile to machine reals; complex values inside them compile)".into());
    }
    let leaf_types = fr.result.leaf_types();
    if leaf_types.contains(&Ty::Bool) && leaf_types.len() > 1 {
        return Err("a boolean in a list result".into());
    }
    let deps = fr.deps.iter().map(|s| (*s, versions[s])).collect();
    Ok(ExprFn {
        built,
        args: params.iter().map(|p| p.1).collect(),
        words: params.iter().map(|p| p.1.words()).sum(),
        shape: fr.result,
        leaf_types,
        deps,
        ctl: Control::default(),
    })
}

/// A closure parameter: its symbol and declared type name, if any.
pub type ClosureParam = (Sym, Option<String>);

/// The parameters and body of a closure `fn([p1, ...], body)` (what `x => ...` and `(t, y) => ...` are): each
/// parameter's symbol and declared type name, if any. `None` for anything else (rest or optional parameters,
/// patterns, attributes).
pub fn closure_parts<'h>(h: &'h Heap, f: T<'h>) -> Option<(Vec<ClosureParam>, T<'h>)> {
    let View::App { head, args: [params, body] } = h.view(f) else { return None };
    if head != h.sym_term(h.sym("fn")) {
        return None;
    }
    let View::App { head: lh, args: ps } = h.view(*params) else { return None };
    if lh != h.sym_term(h.sym(mc_api::names::LIST)) {
        return None;
    }
    let typed = h.sym_term(h.sym("typed"));
    let mut out = vec![];
    for &p in ps {
        match h.view(p) {
            View::Sym(s) => out.push((s, None)),
            View::App { head, args: [x, ty] } if head == typed => {
                let (View::Sym(s), View::Sym(t)) = (h.view(*x), h.view(*ty)) else { return None };
                out.push((s, Some(h.sym_name(t))));
            }
            _ => return None,
        }
    }
    Some((out, *body))
}

/// Whether a declared parameter type admits the argument kind (as the interpreter's check would pass).
pub fn admits(ty: &str, a: Arg) -> bool {
    match a {
        Arg::Int => matches!(ty, "Int" | "integer" | "Integer" | "Real" | "real" | "Number" | "number"),
        Arg::Real => matches!(ty, "Real" | "real" | "Number" | "number" | "MachineReal" | "machine_real"),
        Arg::Vector(_) => matches!(ty, "List" | "list"),
    }
}

/// Compile the closure `f` (`x => ...`, `(t, y) => ...`) for arguments of the kinds `args` (decision N0.4).
pub fn compile_closure<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    f: T<'h>,
    args: &[Arg],
    numeric: bool,
    backend: Backend,
) -> Result<ExprFn, String> {
    let h = cx.heap();
    let (ps, body) = closure_parts(h, f).ok_or("not a closure of named parameters")?;
    if ps.len() != args.len() {
        return Err(format!("a closure of {} parameters called with {} arguments", ps.len(), args.len()));
    }
    let mut params = vec![];
    for ((s, ty), &a) in ps.iter().zip(args) {
        if let Some(t) = ty {
            if !admits(t, a) {
                return Err(format!("parameter `{}` declared {t}", h.sym_name(*s)));
            }
        }
        params.push((*s, a));
    }
    compile_expr(cx, &params, body, numeric, backend)
}
