//! Compiled specialisations and the natives `compile` and `compiled_info`.
//!
//! `compile(f)` reads `f`'s definition through the native interface, runs the front end, lowering and a back end,
//! and queues the result; [`Jit::flush`] registers queued specialisations between evaluations (decision 1b.5:
//! registering a native needs the kernel exclusively, which a native does not have). A specialisation applies when
//! every argument has its parameter's machine type and no definition it read has changed since (version counters,
//! decision 1b.6); otherwise it declines and the interpreter evaluates the call.
//!
//! **Draws** (package N4b, decision N4b.2): a specialisation that draws is an effect. Its call announces the effect
//! first (declining on a speculative task or inside a condition, where the interpreter then reports the same), takes
//! the session generator's lock for the whole call and gives the code the session stream; a call that declines or
//! stops puts the stream back where it was, so the interpreter, evaluating the call instead, draws the very
//! numbers the interpreted call would have drawn.
use mc_eval::Compiled;
use crate::front::{front_vectors, FnDef, Input, Param, Shape, Source};
use crate::interp::Interp;
use crate::ir::{self, Ty};
use crate::jit::{self, Code};
use crate::rt::{Env, Exit, PollFn};
use mc_api::packed::{int_words, real_words, words_ints, words_reals, Packed};
use mc_arith::{Arith, Num, Real};
use mc_core::{Heap, Store, Sym, View, T};
use mc_eval::pat::{Pat, Ty as PatTy};
use mc_eval::{Attributes, Control, Hold, Kernel, Native, NativeCx, Reply, Res};
use mc_random::{Rng, Stream};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering::Relaxed};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

/// Which back end runs a specialisation.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Backend {
    /// machine code from Cranelift
    Cranelift,
    /// the LIR interpreter (requested, or the fallback when Cranelift fails)
    Interpreter,
}

impl Backend {
    /// The name `compiled_info` reports and `compile(f, back_end: ...)` accepts.
    pub fn name(self) -> &'static str {
        match self {
            Backend::Cranelift => "cranelift",
            Backend::Interpreter => "interpreter",
        }
    }
}

/// What a compilation produced, for `compiled_info`.
#[derive(Clone, Debug)]
pub struct Info {
    /// the types of the parameter words (a vector takes two `Int` words: address and length)
    pub params: Vec<Ty>,
    /// how the function takes its arguments (a scalar, or a vector of `Int` or `Real` leaves: package P)
    pub inputs: Vec<Input>,
    /// the result type (`Int` for a list result, whose words go to the output buffer)
    pub result: Ty,
    /// the result's shape (a list result: its leaves are the output words, decision N0.3)
    pub shape: Shape,
    /// HIR nodes
    pub hir_nodes: usize,
    /// LIR instructions
    pub lir_instructions: usize,
    /// machine code bytes (0 for the interpreter)
    pub code_bytes: usize,
    /// the back end running it
    pub back_end: Backend,
    /// why Cranelift was not used when it was asked for
    pub fallback: Option<String>,
    /// front end, lowering and back end, in microseconds
    pub compile_micros: u64,
    /// the functions inlined into it
    pub inlined: Vec<Sym>,
}

enum Back {
    Native(Box<Code>),
    Interp(Interp),
}

/// A function compiled from its HIR, ready to call with words (no kernel needed: tests and differential checks use
/// it directly).
pub struct Built {
    back: Back,
    outputs: usize,
    draws: bool,
    /// the LIR (kept for inspection and differential tests)
    pub lir: ir::Lir,
    /// what was compiled
    pub info: Info,
}

impl Built {
    /// Call with the parameters as words, polling with `poll`.
    pub fn call(&self, ctl: &Control, args: &[u64], poll: &mut crate::rt::PollFn<'_>) -> (u64, Exit) {
        Env::with(ctl, poll, |env| match &self.back {
            Back::Native(c) => c.call(args, env),
            Back::Interp(i) => i.call(args, env),
        })
    }

    /// Call with the parameters as words, writing a vector result's words into `out` (decision N0.3); declines when
    /// `out` is shorter than the function's outputs ([`Built::outputs`]).
    pub fn call_out(&self, ctl: &Control, args: &[u64], out: &mut [u64], poll: &mut crate::rt::PollFn<'_>) -> (u64, Exit) {
        self.call_with(ctl, args, out, None, poll)
    }

    /// [`Built::call_out`] with the random stream the code's draws come from (package N4b; without one, a draw
    /// declines). The caller makes sure nothing else uses the stream during the call and restores it when the call
    /// does not complete.
    pub fn call_with(
        &self,
        ctl: &Control,
        args: &[u64],
        out: &mut [u64],
        stream: Option<&mut Stream>,
        poll: &mut PollFn<'_>,
    ) -> (u64, Exit) {
        let ((r, _), exit) = self.call_vector(ctl, args, out, stream, poll);
        (r, exit)
    }

    /// [`Built::call_with`] that also returns the words of a vector result ([`Shape::Vector`], package P), when the
    /// call set one.
    pub fn call_vector(
        &self,
        ctl: &Control,
        args: &[u64],
        out: &mut [u64],
        stream: Option<&mut Stream>,
        poll: &mut PollFn<'_>,
    ) -> ((u64, Option<Vec<u64>>), Exit) {
        if out.len() < self.outputs || args.len() != self.info.params.len() {
            return ((0, None), Exit::Decline);
        }
        Env::with_stream(ctl, poll, out, stream, |env| {
            let r = match &self.back {
                Back::Native(c) => c.call(args, env),
                Back::Interp(i) => i.call(args, env),
            };
            (r, env.take_result())
        })
    }

    /// The number of output words of a vector result (0 for a scalar function).
    pub fn outputs(&self) -> usize {
        self.outputs
    }

    /// Whether the code draws from the session generator (an effect, decision N4b.2); code that does not is pure
    /// and may run on several threads at once (decision N4b.5).
    pub fn draws(&self) -> bool {
        self.draws
    }
}

/// Lower `func` and compile it with `backend` (falling back to the interpreter when Cranelift fails).
pub fn build(func: &ir::Func, inlined: Vec<Sym>, backend: Backend) -> Result<Built, String> {
    let t0 = Instant::now();
    let lir = ir::lower(func)?;
    let (back, back_end, code_bytes, fallback) = match backend {
        Backend::Cranelift => match jit::compile(&lir) {
            Ok(c) => {
                let n = c.code_bytes;
                (Back::Native(Box::new(c)), Backend::Cranelift, n, None)
            }
            Err(e) => (Back::Interp(Interp::new(&lir)), Backend::Interpreter, 0, Some(e)),
        },
        Backend::Interpreter => (Back::Interp(Interp::new(&lir)), Backend::Interpreter, 0, None),
    };
    let params: Vec<Ty> = func.params.iter().map(|&p| func.vars[p as usize]).collect();
    let result = ir::type_of(func, &func.ret)?;
    let info = Info {
        inputs: params.iter().map(|&t| Input::Scalar(t)).collect(),
        params,
        result,
        shape: Shape::Scalar(result),
        hir_nodes: ir::size(func),
        lir_instructions: lir.len(),
        code_bytes,
        back_end,
        fallback,
        compile_micros: t0.elapsed().as_micros() as u64,
        inlined,
    };
    let outputs = ir::outputs(&lir);
    let draws = ir::draws(&lir);
    Ok(Built { back, outputs, draws, lir, info })
}

/// A compiled specialisation of one head.
pub struct Specialisation {
    head: Sym,
    /// the compiled code; a function with vector parameters has one variant per leaf type that compiled, `Real`
    /// first (decision P.3), and a call runs the one its vectors' leaves select
    variants: Vec<Built>,
    /// every function it read with the version it read (the head first)
    deps: Vec<(Sym, AtomicU32)>,
    ctl: Arc<Control>,
    polls: Arc<AtomicU64>,
    /// calls that ran compiled to the end, and calls that declined (diagnostics for `compiled_info`)
    runs: AtomicU64,
    declines: AtomicU64,
    /// the session generator, for code that draws
    rng: Option<Rng>,
}

impl Specialisation {
    /// What was compiled (the first variant).
    pub fn info(&self) -> &Info {
        &self.variants[0].info
    }

    /// The leaf types of the vector parameters of each variant (none without vector parameters).
    fn vector_leaves(&self) -> Vec<Ty> {
        self.variants
            .iter()
            .filter_map(|b| b.info.inputs.iter().find_map(|i| if let Input::Vector(t) = i { Some(*t) } else { None }))
            .collect()
    }

    /// Are the definitions it read unchanged?
    fn current(&self, version: &mut dyn FnMut(Sym) -> u32) -> bool {
        self.deps.iter().all(|(s, v)| version(*s) == v.load(Relaxed))
    }
}

impl Compiled for Specialisation {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, args: &[T<'h>]) -> Option<T<'h>> {
        let inputs = &self.info().inputs;
        if args.len() != inputs.len() {
            return None;
        }
        let h = cx.heap();
        // the vectors' leaves (decision P.5): a packed payload as it is, a short list's machine numbers copied; every
        // vector of one call has leaves of one type, which selects the variant
        let mut vectors: Vec<(Ty, Leaves<'h>)> = vec![];
        for (&a, i) in args.iter().zip(inputs) {
            if let Input::Vector(_) = i {
                let v = vector_arg(cx, a)?;
                if vectors.first().is_some_and(|w| w.0 != v.0) {
                    return None;
                }
                vectors.push(v);
            }
        }
        let built = match vectors.first() {
            None => &self.variants[0],
            Some((t, _)) => self.variants.iter().find(|b| b.info.inputs.contains(&Input::Vector(*t)))?,
        };
        let mut words = Vec::with_capacity(built.info.params.len());
        let mut vs = vectors.iter();
        for (&a, i) in args.iter().zip(&built.info.inputs) {
            match (*i, h.view(a)) {
                (Input::Scalar(Ty::Int), View::Int(v)) => words.push(v as u64),
                (Input::Scalar(Ty::Real), View::Real(x)) => words.push(x.to_bits()),
                (Input::Vector(_), _) => {
                    let w = vs.next().expect("a vector per vector parameter").1.words();
                    words.extend([w.as_ptr() as u64, w.len() as u64]);
                }
                // a big integer, an exact or inexact non-machine number, anything else: the interpreter's
                _ => return None,
            }
        }
        if !self.current(&mut |s| cx.version(s)) {
            self.declines.fetch_add(1, Relaxed);
            return None;
        }
        // decision N4b.2: drawing code runs only where the interpreter's draw would (the frontier, outside
        // conditions), with the session stream
        let rng = match (built.draws(), &self.rng) {
            (false, _) => None,
            (true, Some(rng)) if cx.effect().is_ok() => Some(rng),
            (true, _) => {
                self.declines.fetch_add(1, Relaxed);
                return None;
            }
        };
        let mut guard = rng.map(|r| r.lock());
        let saved = guard.as_mut().map(|g| g.stream().clone());
        let shape = &built.info.shape;
        let words_out = matches!(shape, Shape::List(_) | Shape::Complex);
        let mut out = vec![0u64; if words_out { built.outputs() } else { 0 }];
        let ((r, vector), exit) = {
            // the fuel of compiled loops is the task's (decision F.3: a speculative task spends its own share)
            let mut poll = |n: u64| {
                self.polls.fetch_add(1, Relaxed);
                cx.tick(n).is_ok()
            };
            let stream = guard.as_mut().map(|g| g.stream());
            built.call_vector(&self.ctl, &words, &mut out, stream, &mut poll)
        };
        // the payloads the code read stay borrowed until here
        drop(vectors);
        // a decline or a stop: the interpreter evaluates the call (and stops at its first poll after a stop), from
        // the stream position the call started at
        if exit != Exit::Done {
            if let (Some(g), Some(s)) = (guard.as_mut(), saved) {
                *g.stream() = s;
            }
            self.declines.fetch_add(1, Relaxed);
            return None;
        }
        drop(guard);
        if let Shape::Vector(t) = *shape {
            // decision P.6: a vector result is built as one payload (packed when long enough)
            let w = vector?;
            self.runs.fetch_add(1, Relaxed);
            return Some(match t {
                Ty::Int => h.int_array(&[w.len()], words_ints(&w)),
                _ => h.real_array(&[w.len()], words_reals(&w)),
            });
        }
        self.runs.fetch_add(1, Relaxed);
        if words_out {
            return Some(list_term(cx, shape, &mut out.into_iter()));
        }
        Some(word_term(cx, built.info.result, r))
    }
}

/// The leaves of a vector argument: borrowed from a packed payload, or copied from a short list.
enum Leaves<'h> {
    Payload(&'h [u64]),
    Copied(Vec<u64>),
}

impl Leaves<'_> {
    fn words(&self) -> &[u64] {
        match self {
            Leaves::Payload(w) => w,
            Leaves::Copied(w) => w,
        }
    }
}

/// A vector argument's leaf type and leaves (decision P.5): a packed vector's payload, read without unpacking; a
/// compound list of at least one element, all machine reals or all machine integers, element by element (such a
/// list is shorter than the packing threshold: longer ones are packed). Anything else (an empty list, mixed
/// leaves, big integers, matrices, other terms) is `None`: the call declines.
fn vector_arg<'h>(cx: &NativeCx<'_, '_, 'h>, a: T<'h>) -> Option<(Ty, Leaves<'h>)> {
    let h = cx.heap();
    if let Some(p) = h.packed(a) {
        return match p {
            _ if p.rank() != 1 => None,
            Packed::Reals { data, .. } => Some((Ty::Real, Leaves::Payload(real_words(data)))),
            Packed::Ints { data, .. } => Some((Ty::Int, Leaves::Payload(int_words(data)))),
        };
    }
    let items = cx.as_list(a)?;
    let t = match h.view(*items.first()?) {
        View::Real(_) => Ty::Real,
        View::Int(_) => Ty::Int,
        _ => return None,
    };
    let mut w = Vec::with_capacity(items.len());
    for &x in items {
        w.push(match (h.view(x), t) {
            (View::Real(v), Ty::Real) => v.to_bits(),
            (View::Int(v), Ty::Int) => v as u64,
            _ => return None,
        });
    }
    Some((t, Leaves::Copied(w)))
}

/// The term of a result word of type `t`.
fn word_term<'h>(cx: &NativeCx<'_, '_, 'h>, t: Ty, w: u64) -> T<'h> {
    let h = cx.heap();
    match t {
        Ty::Int => h.int(w as i64),
        Ty::Real => h.real(f64::from_bits(w)),
        Ty::Bool => cx.bool(w != 0),
    }
}

/// The term of a value of shape `s` from its leaves' words in row-major order (a list, a complex number).
fn list_term<'h>(cx: &NativeCx<'_, '_, 'h>, s: &Shape, words: &mut impl Iterator<Item = u64>) -> T<'h> {
    match s {
        Shape::Scalar(t) => word_term(cx, *t, words.next().unwrap_or(0)),
        Shape::List(items) => {
            let v: Vec<T<'h>> = items.iter().map(|i| list_term(cx, i, words)).collect();
            cx.list(&v)
        }
        Shape::Complex => {
            let mut part = || Real::Machine(f64::from_bits(words.next().unwrap_or(0)));
            let (re, im) = (part(), part());
            Arith::new(cx.heap()).number(&Num::Complex(re, im))
        }
        Shape::Vector(_) => unreachable!("the front end refuses vectors inside lists"),
    }
}

/// The compiler's state in one kernel: queued and registered specialisations.
pub(crate) struct Shared {
    pending: Mutex<Vec<Arc<Specialisation>>>,
    active: Mutex<HashMap<Sym, Arc<Specialisation>>>,
    polls: Arc<AtomicU64>,
    ctl: Arc<Control>,
    /// the session generator compiled draws use ([`Jit::set_rng`]); without it draws are not compiled
    rng: OnceLock<Rng>,
}

impl Shared {
    /// Whether `s`'s native implementation is a specialisation of ours.
    fn owns(&self, s: Sym) -> bool {
        self.active.lock().expect("compiler state").contains_key(&s)
    }
}

/// The compiler installed in a kernel. Cheap to clone; [`Jit::flush`] must run between evaluations (the engine
/// calls it after every statement).
#[derive(Clone)]
pub struct Jit(Arc<Shared>);

impl Jit {
    /// Register the specialisations `compile` queued since the last flush; returns how many. A queued
    /// specialisation whose definitions changed since it was compiled is dropped.
    pub fn flush(&self, k: &mut Kernel) -> usize {
        let pending = std::mem::take(&mut *self.0.pending.lock().expect("compiler state"));
        let mut active = self.0.active.lock().expect("compiler state");
        let mut n = 0;
        for spec in pending {
            if !spec.current(&mut |s| k.version(s)) {
                continue;
            }
            let head = spec.head;
            // registering a specialisation is not a definition (decision 1a.7): versions and memo entries stay valid
            k.register_compiled(head, spec.clone());
            active.insert(head, spec);
            n += 1;
        }
        n
    }

    /// Polls made by compiled loops so far (each charges [`crate::rt::POLL_EVERY`] fuel).
    pub fn polls(&self) -> u64 {
        self.0.polls.load(Relaxed)
    }

    /// The registered specialisation of `f`.
    pub fn info(&self, f: Sym) -> Option<Info> {
        self.0.active.lock().expect("compiler state").get(&f).map(|s| s.info().clone())
    }

    /// Attach the session's random generator (the one `mc_random::install` returned), so that compiled functions
    /// may draw from it (package N4b). Only the first call has an effect; functions compiled before it do not draw.
    pub fn set_rng(&self, rng: Rng) {
        let _ = self.0.rng.set(rng);
    }
}

/// Register `compile` and `compiled_info` with `k`; the returned handle flushes queued specialisations.
pub fn install(k: &mut Kernel, h: &Heap) -> Jit {
    let shared = Arc::new(Shared {
        pending: Mutex::new(vec![]),
        active: Mutex::new(HashMap::new()),
        polls: Arc::new(AtomicU64::new(0)),
        ctl: k.control(),
        rng: OnceLock::new(),
    });
    let held = Attributes { hold: Hold::All, ..Attributes::default() };
    k.register(h.sym("compile"), Attributes { effect: true, ..held }, Box::new(CompileFn(shared.clone())));
    k.register(h.sym("compiled_info"), held, Box::new(InfoFn(shared.clone())));
    Jit(shared)
}

/// The kernel's definitions as the front end's [`Source`], recording the version of every function read.
pub(crate) struct KernelSource<'c, 'a, 'k, 'h> {
    pub(crate) cx: &'c mut NativeCx<'a, 'k, 'h>,
    /// the compiler's state, whose specialisations do not count as natives (none for `compile_expr`)
    pub(crate) shared: Option<&'c Shared>,
    pub(crate) versions: HashMap<Sym, u32>,
}

impl<'h> Source<'h> for KernelSource<'_, '_, '_, 'h> {
    fn draws(&self) -> bool {
        self.shared.is_some_and(|sh| sh.rng.get().is_some())
    }

    fn constant(&mut self, s: Sym) -> Option<T<'h>> {
        let v = self.cx.own_value(s)?;
        self.versions.insert(s, self.cx.version(s));
        Some(v)
    }

    fn function(&mut self, f: Sym) -> Result<FnDef<'h>, String> {
        let h = self.cx.heap();
        let name = h.sym_name(f);
        if self.cx.has_native(f) && !self.shared.is_some_and(|sh| sh.owns(f)) {
            return Err(format!("`{name}` has a native implementation"));
        }
        if self.cx.attributes(f) != Attributes::default() {
            return Err(format!("`{name}` has attributes (held arguments, listable, ...)"));
        }
        self.versions.insert(f, self.cx.version(f));
        let rules = self.cx.rules(f);
        let clause = match rules.as_slice() {
            [] => return Err(format!("`{name}` has no definition")),
            [c] => c,
            [c, last] if catch_all(h, f, &last.lhs) => c,
            _ => return Err(format!("`{name}` has more than one clause")),
        };
        let Pat::App { head, args, .. } = &clause.lhs else {
            return Err(format!("`{name}` is defined by a pattern with a condition or alternatives"));
        };
        if head.get(h) != h.sym_term(f) {
            return Err(format!("`{name}` is defined by a pattern with another head"));
        }
        let body = clause.body.ok_or_else(|| format!("`{name}` has a native right side"))?;
        let mut params = vec![];
        for (i, a) in args.iter().enumerate() {
            let Pat::Blank { var: Some(v), ty } = a else {
                return Err(format!("parameter {} of `{name}` is not a named parameter", i + 1));
            };
            let pos = clause.vars.iter().position(|x| x == v).ok_or("a parameter without a name")?;
            let sym = clause.names[pos];
            let p = match ty {
                Some(PatTy::Integer) => Param::Scalar(Ty::Int),
                Some(PatTy::Real | PatTy::MachineReal | PatTy::Number) => Param::Scalar(Ty::Real),
                Some(_) => {
                    return Err(format!(
                        "parameter `{}` of `{name}` has a type other than Int or Real (an untyped parameter is a vector)",
                        h.sym_name(sym)
                    ))
                }
                // decision P.2: the pattern types have no vector type, so an untyped parameter is a vector
                None => Param::Vector,
            };
            params.push((sym, p));
        }
        Ok(FnDef { params, body })
    }
}

/// `f(args___)`: the catch-all a `fn` definition adds (under its `is_ground(args)` condition; compiled code only
/// sees machine numbers, which are ground).
fn catch_all(h: &Heap, f: Sym, p: &Pat) -> bool {
    let p = match p {
        Pat::Cond(inner, _) => inner,
        p => p,
    };
    matches!(p, Pat::App { head, args, .. } if head.get(h) == h.sym_term(f) && matches!(args.as_slice(), [Pat::Seq { min: 0, .. }]))
}

/// `failure(tag, details...)`.
fn failure<'h>(h: &'h Heap, tag: &str, details: &[T<'h>]) -> T<'h> {
    let mut a = vec![h.sym_term(h.sym(tag))];
    a.extend_from_slice(details);
    h.call(h.sym(mc_api::names::FAILURE), &a)
}

/// The back end asked for by `compile(f, back_end: "...")`.
fn backend_option<'h>(h: &'h Heap, opts: T<'h>) -> Result<Backend, String> {
    let bad = || "the only option is back_end: \"cranelift\" or \"interpreter\"".to_string();
    let View::App { head, args } = h.view(opts) else { return Err(bad()) };
    if head != h.sym_term(h.sym("options")) {
        return Err(bad());
    }
    let mut b = Backend::Cranelift;
    for &o in args {
        let View::App { args: [name, value], .. } = h.view(o) else { return Err(bad()) };
        match (h.view(*name), h.view(*value)) {
            (View::Sym(n), View::Str(v)) if h.sym_name(n) == "back_end" => {
                b = match v {
                    "cranelift" => Backend::Cranelift,
                    "interpreter" => Backend::Interpreter,
                    _ => return Err(bad()),
                }
            }
            _ => return Err(bad()),
        }
    }
    Ok(b)
}

/// `compile(f)`, `compile(f, back_end: "interpreter")`.
struct CompileFn(Arc<Shared>);

impl Native for CompileFn {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let (fterm, backend) = match args {
            [f] => (*f, Ok(Backend::Cranelift)),
            [f, opts] => (*f, backend_option(h, *opts)),
            _ => return Ok(Reply::Decline),
        };
        // compiling changes how calls dispatch: ordered like a definition
        cx.effect()?;
        let not = |reason: &str| failure(h, "not_compilable", &[fterm, h.string(reason)]);
        let View::Sym(f) = h.view(fterm) else { return Ok(Reply::Done(not("not a function name"))) };
        let backend = match backend {
            Ok(b) => b,
            Err(e) => return Ok(Reply::Done(not(&e))),
        };
        let mut src = KernelSource { cx, shared: Some(&self.0), versions: HashMap::new() };
        let mut compile = |elem: Ty| {
            front_vectors(h, f, &mut src, elem).and_then(|fr| {
                let inlined = fr.deps[1..].to_vec();
                build(&fr.func, inlined, backend).map(|mut b| {
                    b.info.shape = fr.result;
                    b.info.inputs = fr.inputs;
                    (b, fr.deps)
                })
            })
        };
        // decision P.3: the variant for vectors of machine reals, and, when the function has vector parameters (or
        // the first variant failed, possibly on a real leaf), the one for vectors of machine integers
        let first = compile(Ty::Real);
        let vectors = first.as_ref().map_or(true, |(b, _)| b.info.inputs.iter().any(|i| matches!(i, Input::Vector(_))));
        let second = if vectors { compile(Ty::Int).ok() } else { None };
        let versions = src.versions;
        let mut variants = vec![];
        let mut deps: Vec<Sym> = vec![];
        let reason = match first {
            Ok(x) => {
                variants.push(x);
                None
            }
            Err(e) => Some(e),
        };
        variants.extend(second.filter(|(b, _)| b.info.inputs.iter().any(|i| matches!(i, Input::Vector(_)))));
        if variants.is_empty() {
            return Ok(Reply::Done(not(&reason.expect("a failed variant"))));
        }
        let variants: Vec<Built> = variants
            .into_iter()
            .map(|(b, ds)| {
                for s in ds {
                    if !deps.contains(&s) {
                        deps.push(s);
                    }
                }
                b
            })
            .collect();
        let deps = deps.into_iter().map(|s| (s, AtomicU32::new(versions[&s]))).collect();
        let spec = Specialisation {
            head: f,
            variants,
            deps,
            ctl: self.0.ctl.clone(),
            polls: self.0.polls.clone(),
            runs: AtomicU64::new(0),
            declines: AtomicU64::new(0),
            rng: self.0.rng.get().cloned(),
        };
        self.0.pending.lock().expect("compiler state").push(Arc::new(spec));
        Ok(Reply::Done(fterm))
    }
}

/// `compiled_info(f)`.
struct InfoFn(Arc<Shared>);

impl Native for InfoFn {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let [fterm] = args else { return Ok(Reply::Decline) };
        // the answer is the compiler's state (registrations, run counters), which is not a definition: never memoised
        cx.volatile();
        let View::Sym(f) = h.view(*fterm) else { return Ok(Reply::Done(failure(h, "not_compiled", &[*fterm]))) };
        let (spec, state) = {
            let active = self.0.active.lock().expect("compiler state").get(&f).cloned();
            let pending = self.0.pending.lock().expect("compiler state").iter().rev().find(|s| s.head == f).cloned();
            match (pending, active) {
                (Some(p), _) => (p, "pending"),
                (None, Some(a)) => (a, "active"),
                (None, None) => return Ok(Reply::Done(failure(h, "not_compiled", &[*fterm]))),
            }
        };
        let state = if state == "active" && !spec.current(&mut |s| cx.version(s)) { "stale" } else { state };
        let i = spec.info();
        let sym = |n: &str| h.sym_term(h.sym(n));
        let int = |n: usize| h.int(n as i64);
        let rule = |n: &str, v: T<'h>| h.call(h.sym(mc_api::names::RULE), &[sym(n), v]);
        let types: Vec<T<'h>> = i
            .inputs
            .iter()
            .map(|t| match t {
                Input::Scalar(t) => sym(t.name()),
                // a vector parameter: its leaves are the first variant's
                other => h.string(&other.name()),
            })
            .collect();
        let inlined: Vec<T<'h>> = i.inlined.iter().map(|&s| h.sym_term(s)).collect();
        let mut items = vec![
            rule("params", cx.list(&types)),
            rule("result", match &i.shape {
                Shape::Scalar(t) => sym(t.name()),
                list => h.string(&list.name()),
            }),
            rule("hir_nodes", int(i.hir_nodes)),
            rule("lir_instructions", int(i.lir_instructions)),
            rule("code_bytes", int(i.code_bytes)),
            rule("back_end", h.string(i.back_end.name())),
            rule("compile_microseconds", int(i.compile_micros as usize)),
            rule("inlined", cx.list(&inlined)),
            rule("state", h.string(state)),
            rule("runs", int(spec.runs.load(Relaxed) as usize)),
            rule("declines", int(spec.declines.load(Relaxed) as usize)),
        ];
        let leaves = spec.vector_leaves();
        if !leaves.is_empty() {
            // the leaf types the vector parameters compiled for (decision P.3)
            let ts: Vec<T<'h>> = leaves.iter().map(|t| sym(t.name())).collect();
            items.push(rule("vector_leaves", cx.list(&ts)));
        }
        if let Some(why) = &i.fallback {
            items.push(rule("fallback", h.string(why)));
        }
        Ok(Reply::Done(cx.list(&items)))
    }
}
