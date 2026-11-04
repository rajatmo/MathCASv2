//! The compiled backend (`mc-jit`, machine reals) and [`Auto`], which runs it with the interpreted backend as the
//! fallback for every call it declines (decision 1b.3: correctness never depends on the compiler).
use crate::{Arg, Backend, NumFn, Stats, Var};
use mc_core::{Sym, T};
use mc_eval::{NativeCx, Res};
use mc_jit::{Call, ExprFn};
use std::sync::OnceLock;

/// An expression compiled to machine code for machine-real arguments. A call either gives the interpreter's
/// result or declines (decision 1b.3); it cannot fall back on its own (see [`Auto`]).
pub struct Compiled {
    code: ExprFn,
}

impl Compiled {
    /// Compile `body` in `vars` (scalars and vectors of known length). `consts`: the body is meant numerically
    /// (it went through `n`), so bare `pi` and `e` are machine reals (decision N0.1). The reason is given when
    /// the compiler refuses.
    pub fn new<'h>(cx: &mut NativeCx<'_, '_, 'h>, vars: &[Var], body: T<'h>, consts: bool) -> Result<Compiled, String> {
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
        Ok(Compiled { code })
    }

    /// Evaluate at `args`: `Some(finite)` with the outputs written (`Some(false)`: a value is not finite, as the
    /// interpreted backend reports it), or `None` when the call declined or a loop found the evaluation stopped,
    /// so that the interpreter must evaluate it. One unit of fuel per call.
    pub fn try_eval(&self, cx: &mut NativeCx<'_, '_, '_>, args: &[f64], out: &mut [f64]) -> Res<Option<bool>> {
        cx.tick(1)?;
        Ok(match self.code.call_f64(cx, args, out) {
            Call::Done => Some(out.iter().all(|v| v.is_finite())),
            Call::Declined | Call::Stopped => None,
        })
    }

    /// The number of scalar inputs.
    pub fn inputs(&self) -> usize {
        self.code.inputs()
    }

    /// The number of scalar outputs.
    pub fn outputs(&self) -> usize {
        self.code.outputs()
    }

    /// Sizes, back end and compile time of the code.
    pub fn info(&self) -> &mc_jit::Info {
        self.code.info()
    }
}

/// When [`Auto`] compiles (decision N0.7).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Policy {
    /// never: always the interpreted backend
    Interpreted,
    /// at construction
    Compiled,
    /// after this many evaluations (points) with the interpreted backend: compiling a small expression costs
    /// 0.2-0.5 ms, about 10-50 interpreted evaluations of 10-30 us
    Auto {
        /// evaluations before compiling
        after: u64,
    },
}

/// Evaluations before [`Policy::Auto`] compiles by default.
pub const DEFAULT_AFTER: u64 = 32;

impl Default for Policy {
    /// [`Policy::Auto`] after [`DEFAULT_AFTER`] evaluations, unless the environment variable `MC_NUMFN` says
    /// `interpreted`, `compiled`, `auto` or `auto:N` (a measurement aid, read once per process).
    fn default() -> Policy {
        static P: OnceLock<Policy> = OnceLock::new();
        *P.get_or_init(|| match std::env::var("MC_NUMFN").as_deref() {
            Ok("interpreted") => Policy::Interpreted,
            Ok("compiled") => Policy::Compiled,
            Ok(s) if s.starts_with("auto:") => match s[5..].parse() {
                Ok(n) => Policy::Auto { after: n },
                Err(_) => Policy::Auto { after: DEFAULT_AFTER },
            },
            _ => Policy::Auto { after: DEFAULT_AFTER },
        })
    }
}

/// The compiled backend with the interpreted one `I` as fallback: calls run compiled when the expression compiles
/// (refused: always interpreted) and every declined call is evaluated by `I`. Both evaluate the same body, so the
/// results are the interpreter's (decision N0.2).
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
    stats: Stats,
}

impl<'h, I: NumFn<'h, f64>> Auto<'h, I> {
    /// `interp` evaluates `body` in `vars`; the compiled backend is made from the same body when `policy` says
    /// (`consts` as in [`Compiled::new`]).
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
            stats: Stats::default(),
        };
        if policy == Policy::Compiled {
            a.compile(cx);
        }
        a
    }

    fn compile(&mut self, cx: &mut NativeCx<'_, '_, 'h>) {
        self.tried = true;
        match Compiled::new(cx, &self.vars, self.body, self.consts) {
            Ok(c) if c.inputs() == self.interp.inputs() && c.outputs() == self.interp.outputs() => {
                self.stats.compile_micros = c.info().compile_micros;
                self.compiled = Some(c);
            }
            Ok(c) => {
                self.refusal = Some(format!(
                    "{} inputs and {} outputs compiled, {} and {} interpreted",
                    c.inputs(),
                    c.outputs(),
                    self.interp.inputs(),
                    self.interp.outputs()
                ))
            }
            Err(e) => self.refusal = Some(e),
        }
    }

    /// Compile now if the policy says so after `n` more evaluations.
    fn ready(&mut self, cx: &mut NativeCx<'_, '_, 'h>, n: u64) -> bool {
        self.seen += n;
        if !self.tried {
            if let Policy::Auto { after } = self.policy {
                if self.seen > after {
                    self.compile(cx);
                }
            }
        }
        self.compiled.is_some()
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
            self.stats.declined += 1;
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
            let c = self.compiled.as_ref().expect("compiled");
            match c.try_eval(cx, p, o)? {
                Some(fine) => {
                    self.stats.compiled += 1;
                    ok &= fine;
                }
                None => rest.push(i),
            }
        }
        if rest.is_empty() {
            return Ok(ok);
        }
        self.stats.declined += rest.len() as u64;
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
            let c = self.compiled.as_ref().expect("compiled");
            match c.try_eval(cx, a, o)? {
                Some(fine) => {
                    self.stats.compiled += 1;
                    ok &= fine;
                }
                None => {
                    self.stats.declined += 1;
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
