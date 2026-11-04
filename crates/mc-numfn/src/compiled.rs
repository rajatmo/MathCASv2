//! The compiled backend (`mc-jit`, machine reals) and [`Auto`], which runs it with the interpreted backend as the
//! fallback for every call it declines (decision 1b.3: correctness never depends on the compiler).
use crate::cache::{self, Found, Lifted};
use crate::{Arg, Backend, NumFn, Stats, Var};
use mc_core::{Sym, T};
use mc_eval::{NativeCx, Res};
use mc_jit::{Call, ExprFn};
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::sync::{Arc, OnceLock};

/// An expression compiled to machine code for machine-real arguments. A call either gives the interpreter's
/// result or declines (decision 1b.3); it cannot fall back on its own (see [`Auto`]). Code from the cache
/// ([`Compiled::cached`]) is shared by a family of expressions and called with this instance's constants after
/// the inputs (decision N1.1).
pub struct Compiled {
    code: Arc<ExprFn>,
    /// the lifted constants, passed after the inputs
    lifted: Vec<f64>,
}

/// Compile `body` in `vars`; the reason when the compiler refuses.
fn compile_code<'h>(cx: &mut NativeCx<'_, '_, 'h>, vars: &[Var], body: T<'h>, consts: bool) -> Result<ExprFn, String> {
    let params: Vec<(Sym, mc_jit::Arg)> = vars
        .iter()
        .map(|v| {
            (
                v.sym,
                match v.arg {
                    Arg::Scalar => mc_jit::Arg::Real,
                    Arg::Vector(n) => mc_jit::Arg::Vector(n),
                },
            )
        })
        .collect();
    let code = mc_jit::compile_expr(cx, &params, body, consts, mc_jit::Backend::Cranelift)?;
    if code.output_types().contains(&mc_jit::ir::Ty::Bool) {
        return Err("a boolean result".into());
    }
    Ok(code)
}

/// How a compilation through the cache went.
enum Made {
    /// the family's code (from the cache or compiled now)
    Code(Compiled),
    /// the lifted body is refused; `unlifted`: this instance's own body compiles (`Some`)
    Refused(String, Option<Compiled>),
}

/// Compile the lifted body of a family that missed the cache and record the outcome; on a refusal, try the
/// instance's own body (`vars`, `body`) once and record whether it compiles.
fn compile_family<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    lifted: &Lifted<'h>,
    vars: &[Var],
    body: T<'h>,
    consts: bool,
) -> Made {
    match compile_code(cx, &lifted.params, lifted.body, consts) {
        Ok(code) => {
            let code = Arc::new(code);
            cache::insert_code(cx, &lifted.key, code.clone());
            Made::Code(Compiled { code, lifted: lifted.values.clone() })
        }
        Err(why) => {
            let own = Compiled::new(cx, vars, body, consts).ok();
            let checks = cache::checks(cx, body);
            cache::insert_refusal(cx, &lifted.key, &why, checks, own.is_some());
            Made::Refused(why, own)
        }
    }
}

impl Compiled {
    /// Compile `body` in `vars` (scalars and vectors of known length), for this body alone. `consts`: the body is
    /// meant numerically (it went through `n`), so bare `pi` and `e` are machine reals (decision N0.1). The reason
    /// is given when the compiler refuses.
    pub fn new<'h>(cx: &mut NativeCx<'_, '_, 'h>, vars: &[Var], body: T<'h>, consts: bool) -> Result<Compiled, String> {
        Ok(Compiled { code: Arc::new(compile_code(cx, vars, body, consts)?), lifted: vec![] })
    }

    /// [`Compiled::new`] through the process-wide cache (decisions N1.1 to N1.3): the code of `body`'s family with
    /// its machine reals lifted to parameters, compiled now if the cache has none. Calls give bitwise the results
    /// of [`Compiled::new`]'s code. A refused family gives the reason (a body too large for a key compiles alone).
    pub fn cached<'h>(cx: &mut NativeCx<'_, '_, 'h>, vars: &[Var], body: T<'h>, consts: bool) -> Result<Compiled, String> {
        let Some(lifted) = cache::lift(cx.heap(), vars, body, consts) else { return Compiled::new(cx, vars, body, consts) };
        match cache::lookup(cx, &lifted.key).0 {
            Found::Code(code) => Ok(Compiled { code, lifted: lifted.values }),
            Found::Refused { why, .. } => Err(why.to_string()),
            Found::Miss => match compile_family(cx, &lifted, vars, body, consts) {
                Made::Code(c) => Ok(c),
                Made::Refused(why, _) => Err(why),
            },
        }
    }

    /// Evaluate at `args`: `Some(finite)` with the outputs written (`Some(false)`: a value is not finite, as the
    /// interpreted backend reports it), or `None` when the call declined or a loop found the evaluation stopped,
    /// so that the interpreter must evaluate it. One unit of fuel per call.
    pub fn try_eval(&self, cx: &mut NativeCx<'_, '_, '_>, args: &[f64], out: &mut [f64]) -> Res<Option<bool>> {
        cx.tick(1)?;
        let call = if self.lifted.is_empty() {
            self.code.call_f64(cx, args, out)
        } else {
            let n = args.len() + self.lifted.len();
            let mut small = [0.0; 16];
            let mut big = vec![];
            let all: &mut [f64] = if n <= small.len() {
                &mut small[..n]
            } else {
                big.resize(n, 0.0);
                &mut big
            };
            all[..args.len()].copy_from_slice(args);
            all[args.len()..].copy_from_slice(&self.lifted);
            self.code.call_f64(cx, all, out)
        };
        Ok(match call {
            Call::Done => Some(out.iter().all(|v| v.is_finite())),
            Call::Declined | Call::Stopped => None,
        })
    }

    /// The number of scalar inputs (the lifted constants not counted).
    pub fn inputs(&self) -> usize {
        self.code.inputs() - self.lifted.len()
    }

    /// The number of scalar outputs.
    pub fn outputs(&self) -> usize {
        self.code.outputs()
    }

    /// Sizes, back end and compile time of the code (of the family's code when it came from the cache).
    pub fn info(&self) -> &mc_jit::Info {
        self.code.info()
    }

    /// The constants lifted to parameters (empty for code made by [`Compiled::new`]).
    pub fn lifted(&self) -> &[f64] {
        &self.lifted
    }

    /// Are the functions the code inlined unchanged (decision 1b.6)?
    fn current(&self, cx: &mut NativeCx<'_, '_, '_>) -> bool {
        self.code.current(cx)
    }
}

/// When [`Auto`] compiles (decision N0.7).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Policy {
    /// never: always the interpreted backend
    Interpreted,
    /// at construction
    Compiled,
    /// after this many evaluations (points) of this instance with the interpreted backend: compiling a small
    /// expression costs 0.2-0.5 ms, about 10-50 interpreted evaluations of 10-30 us (code the cache already holds
    /// is taken then, without compiling)
    Auto {
        /// evaluations before compiling
        after: u64,
    },
    /// after this many evaluations of the expression's whole family in the code cache (decision N1.4): an
    /// instance whose family the cache holds runs compiled from its first call, so solvers called once per
    /// parameter value compile once (the default)
    Family {
        /// evaluations of the family before compiling
        after: u64,
    },
}

/// Evaluations before [`Policy::Family`] (the default) and [`Policy::Auto`] compile by default.
pub const DEFAULT_AFTER: u64 = 32;

impl Default for Policy {
    /// [`Policy::Family`] after [`DEFAULT_AFTER`] evaluations, unless the environment variable `MC_NUMFN` says
    /// `interpreted`, `compiled`, `auto` (the default), `auto:N` ([`Policy::Auto`], per instance) or `family:N`
    /// (a measurement aid, read once per process).
    fn default() -> Policy {
        static P: OnceLock<Policy> = OnceLock::new();
        *P.get_or_init(|| {
            let var = std::env::var("MC_NUMFN").unwrap_or_default();
            let count = |s: &str| s.parse().unwrap_or(DEFAULT_AFTER);
            match var.as_str() {
                "interpreted" => Policy::Interpreted,
                "compiled" => Policy::Compiled,
                s if s.starts_with("auto:") => Policy::Auto { after: count(&s[5..]) },
                s if s.starts_with("family:") => Policy::Family { after: count(&s[7..]) },
                _ => Policy::Family { after: DEFAULT_AFTER },
            }
        })
    }
}

/// The compiled backend with the interpreted one `I` as fallback: calls run compiled when the expression compiles
/// (refused: always interpreted) and every declined call is evaluated by `I`. Both evaluate the same body, so the
/// results are the interpreter's (decision N0.2).
///
/// Unless the policy is [`Policy::Interpreted`], the code comes from the process-wide cache (decision N1.4). Under
/// [`Policy::Family`] (the default) an instance whose family the cache holds runs compiled from its first call, and
/// on a miss the evaluations of the whole family are counted; [`Policy::Auto`] counts this instance's evaluations
/// and then takes the family's code from the cache, or compiles it.
pub struct Auto<'h, I> {
    interp: I,
    vars: Vec<Var>,
    body: T<'h>,
    consts: bool,
    policy: Policy,
    compiled: Option<Compiled>,
    /// why compilation was refused (also when the shapes of the two backends differ)
    refusal: Option<String>,
    tried: bool,
    seen: u64,
    /// the lifted body and the family's evaluation counter (`None`: compiled per instance, as without the cache)
    family: Option<(Lifted<'h>, Arc<AtomicU64>)>,
    stats: Stats,
}

impl<'h, I: NumFn<'h, f64>> Auto<'h, I> {
    /// `interp` evaluates `body` in `vars`; the compiled backend is made from the same body when `policy` says
    /// (`consts` as in [`Compiled::new`]), through the cache.
    pub fn new(
        cx: &mut NativeCx<'_, '_, 'h>,
        interp: I,
        vars: &[Var],
        body: T<'h>,
        consts: bool,
        policy: Policy,
    ) -> Auto<'h, I> {
        let mut a = Auto {
            interp,
            vars: vars.to_vec(),
            body,
            consts,
            policy,
            compiled: None,
            refusal: None,
            tried: false,
            seen: 0,
            family: None,
            stats: Stats::default(),
        };
        if policy == Policy::Interpreted {
            return a;
        }
        let Some(lifted) = cache::lift(cx.heap(), vars, body, consts) else {
            // too large for a key: compiled per instance, as without the cache
            if policy == Policy::Compiled {
                a.compile(cx);
            }
            return a;
        };
        let (found, seen) = cache::lookup(cx, &lifted.key);
        let now = matches!(policy, Policy::Compiled | Policy::Family { .. });
        match found {
            Found::Code(code) => {
                a.stats.cache_hits = 1;
                if now {
                    a.tried = true;
                    a.adopt(Compiled { code, lifted: lifted.values.clone() });
                }
            }
            Found::Refused { why, unlifted } => {
                a.stats.cache_hits = 1;
                if !unlifted {
                    // neither the family nor this body compiles
                    a.tried = true;
                    a.refusal = Some(why.to_string());
                }
                // else this body compiles on its own, per instance (no family)
                return a;
            }
            Found::Miss => a.stats.cache_misses = 1,
        }
        a.family = Some((lifted, seen));
        if policy == Policy::Compiled && !a.tried {
            a.compile(cx);
        }
        a
    }

    /// Use `c` if its shape is the interpreted backend's (decision N0.9), else record the refusal.
    fn adopt(&mut self, c: Compiled) {
        if c.inputs() == self.interp.inputs() && c.outputs() == self.interp.outputs() {
            self.compiled = Some(c);
        } else {
            self.refusal = Some(format!(
                "{} inputs and {} outputs compiled, {} and {} interpreted",
                c.inputs(),
                c.outputs(),
                self.interp.inputs(),
                self.interp.outputs()
            ));
        }
    }

    fn compile(&mut self, cx: &mut NativeCx<'_, '_, 'h>) {
        self.tried = true;
        let t0 = std::time::Instant::now();
        let made = match &self.family {
            // the family may have been compiled since this instance looked (by another instance, or before a
            // `Policy::Auto` instance reached its count)
            Some((lifted, _)) => match cache::lookup(cx, &lifted.key).0 {
                Found::Code(code) => Ok(Compiled { code, lifted: lifted.values.clone() }),
                Found::Refused { why, unlifted: false } => Err(why.to_string()),
                Found::Refused { why, unlifted: true } => {
                    Compiled::new(cx, &self.vars, self.body, self.consts).map_err(|_| why.to_string())
                }
                Found::Miss => match compile_family(cx, lifted, &self.vars, self.body, self.consts) {
                    Made::Code(c) => Ok(c),
                    Made::Refused(why, own) => own.ok_or(why),
                },
            },
            None => Compiled::new(cx, &self.vars, self.body, self.consts),
        };
        match made {
            Ok(c) => {
                self.stats.compile_micros = t0.elapsed().as_micros() as u64;
                self.adopt(c);
            }
            Err(e) => self.refusal = Some(e),
        }
    }

    /// Compile now if the policy says so after `n` more evaluations (of the whole family, when it is cached).
    fn ready(&mut self, cx: &mut NativeCx<'_, '_, 'h>, n: u64) -> bool {
        self.seen += n;
        if !self.tried {
            let seen = match (&self.family, self.policy) {
                (Some((_, family)), Policy::Family { .. }) => family.fetch_add(n, Relaxed) + n,
                _ => self.seen,
            };
            match self.policy {
                Policy::Auto { after } | Policy::Family { after } if seen > after => self.compile(cx),
                Policy::Compiled => self.compile(cx),
                _ => {}
            }
        }
        self.compiled.is_some()
    }

    /// A call of the compiled code declined. If a function it inlined was redefined (decision 1b.6), the code and
    /// its cache entry are dropped, so that the next call compiles the current definition.
    fn declined(&mut self, cx: &mut NativeCx<'_, '_, 'h>) {
        self.stats.declined += 1;
        if self.compiled.as_ref().is_some_and(|c| !c.current(cx)) {
            if let Some((lifted, _)) = &self.family {
                cache::invalidate(cx, &lifted.key);
            }
            self.compiled = None;
            self.tried = false;
        }
    }

    /// Why the expression is not compiled (`None` when it is, or compilation was not tried yet).
    pub fn refusal(&self) -> Option<&str> {
        self.refusal.as_deref()
    }

    /// The compiled backend, once made.
    pub fn compiled(&self) -> Option<&Compiled> {
        self.compiled.as_ref()
    }

    /// The interpreted backend.
    pub fn interpreted(&mut self) -> &mut I {
        &mut self.interp
    }
}

impl<'h, I: NumFn<'h, f64>> NumFn<'h, f64> for Auto<'h, I> {
    fn eval(&mut self, cx: &mut NativeCx<'_, '_, 'h>, args: &[f64], out: &mut [f64]) -> Res<bool> {
        if self.ready(cx, 1) {
            let c = self.compiled.as_ref().expect("compiled");
            if let Some(ok) = c.try_eval(cx, args, out)? {
                self.stats.compiled += 1;
                return Ok(ok);
            }
            self.declined(cx);
        }
        self.stats.interpreted += 1;
        self.interp.eval(cx, args, out)
    }

    fn eval_batch(&mut self, cx: &mut NativeCx<'_, '_, 'h>, points: &[Vec<f64>], outs: &mut [Vec<f64>]) -> Res<bool> {
        if !self.ready(cx, points.len() as u64) {
            self.stats.interpreted += points.len() as u64;
            return self.interp.eval_batch(cx, points, outs);
        }
        // compiled point by point; the declined points as one interpreted batch
        let mut rest = vec![];
        let mut ok = true;
        for (i, (p, o)) in points.iter().zip(outs.iter_mut()).enumerate() {
            let done = match self.compiled.as_ref() {
                Some(c) => c.try_eval(cx, p, o)?,
                None => None,
            };
            match done {
                Some(fine) => {
                    self.stats.compiled += 1;
                    ok &= fine;
                }
                None => {
                    if self.compiled.is_some() {
                        self.declined(cx);
                    }
                    rest.push(i);
                }
            }
        }
        if rest.is_empty() {
            return Ok(ok);
        }
        self.stats.interpreted += rest.len() as u64;
        let pts: Vec<Vec<f64>> = rest.iter().map(|&i| points[i].clone()).collect();
        let mut os: Vec<Vec<f64>> = rest.iter().map(|&i| outs[i].clone()).collect();
        let fine = self.interp.eval_batch(cx, &pts, &mut os)?;
        for (&i, o) in rest.iter().zip(os) {
            outs[i] = o;
        }
        Ok(ok && fine)
    }

    fn eval_flat(&mut self, cx: &mut NativeCx<'_, '_, 'h>, args: &[f64], out: &mut [f64]) -> Res<bool> {
        let (ni, no) = (self.inputs(), self.outputs());
        if no == 0 || !out.len().is_multiple_of(no) || (ni > 0 && args.len() != ni * (out.len() / no)) {
            return Ok(false);
        }
        let n = out.len() / no;
        if !self.ready(cx, n as u64) {
            self.stats.interpreted += n as u64;
            return self.interp.eval_flat(cx, args, out);
        }
        let mut ok = true;
        for k in 0..n {
            let (a, o) = (&args[k * ni..(k + 1) * ni], &mut out[k * no..(k + 1) * no]);
            let done = match self.compiled.as_ref() {
                Some(c) => c.try_eval(cx, a, o)?,
                None => None,
            };
            match done {
                Some(fine) => {
                    self.stats.compiled += 1;
                    ok &= fine;
                }
                None => {
                    if self.compiled.is_some() {
                        self.declined(cx);
                    }
                    self.stats.interpreted += 1;
                    ok &= self.interp.eval(cx, a, o)?;
                }
            }
        }
        Ok(ok)
    }

    fn inputs(&self) -> usize {
        self.interp.inputs()
    }

    fn outputs(&self) -> usize {
        self.interp.outputs()
    }

    fn backend(&self) -> Backend {
        if self.compiled.is_some() {
            Backend::Compiled
        } else {
            Backend::Interpreted
        }
    }

    fn stats(&self) -> Stats {
        self.stats
    }
}
