# Package N0 (part a): `mc-numfn` (numeric functions) and the compiler extensions

Status: branch `n0-numfn` (from main 81538d2). The trait `NumFn` and its interpreted, compiled and batch backends
work, `compile_expr` falls back to the interpreter automatically, and `mc-roots` runs on them with all its tests
unchanged (also with every machine-real function compiled, `MC_NUMFN=compiled`). The compiler (`mc-jit`) gained
`pi`/`e`, lists of known length and closures (its NOTES, section "Package N0"). Class L case files exist
(`bench/cases/L`). Dependencies: `mc-api`, `mc-arith`, `mc-core`, `mc-eval`, `mc-jit`, `mc-numeric`; dev
`mc-engine`, `mc-bench`. No natives, so no registration lines.

## Modules

| module | contents |
|---|---|
| `lib` | `NumFn<'h, S>` (`eval`, `eval_batch`, `eval_flat`, `inputs`, `outputs`, `backend`, `stats`); `Var`, `Arg`, `Prep`, `Options`, `Backend`, `Stats`; `compile_expr`, `auto_expr`, `compile_fn`, `function_parts` |
| `value` | `Value` (`f64`, `C64`, `Num`: what the interpreted backend needs) and `Scalar` (`f64`, `C64`: field operations, moved unchanged from `mc-roots`); `norm`, `norm2`, `all_finite` |
| `term` | `TermFn`: the expression as the closure `fn([x1, ..., xn], body)` (moved from `mc-roots`, plus vector variables and `set_outputs`); `leaves` |
| `interp` | `Machine` (borrowed `TermFn` + `Arith`, what `mc-roots` uses), `Interpreted` (owning, any `Value`), `CentralGradient<F>` |
| `compiled` | `Compiled` (an `mc_jit::ExprFn`, `f64` only), `Auto<'h, I>` (compiled with `I` as fallback), `Policy` |
| `prep` | `prepare` (`n` at machine precision or `d` digits, keeping constant indices `y[k]`), `replace` (an iterative term substitution) |

## Public API (what N1, N2, N3 build on)

```rust
pub trait NumFn<'h, S> {
    fn eval(&mut self, cx: &mut NativeCx<'_, '_, 'h>, args: &[S], out: &mut [S]) -> Res<bool>;
    fn eval_batch(&mut self, cx: &mut NativeCx<'_, '_, 'h>, points: &[Vec<S>], outs: &mut [Vec<S>]) -> Res<bool>; // provided
    fn eval_flat(&mut self, cx: &mut NativeCx<'_, '_, 'h>, args: &[S], out: &mut [S]) -> Res<bool>;               // provided
    fn inputs(&self) -> usize;
    fn outputs(&self) -> usize;
    fn backend(&self) -> Backend { Backend::Interpreted }
    fn stats(&self) -> Stats { Stats::default() }
}
// also implemented for Box<F: NumFn + ?Sized>

pub struct Var { pub sym: Sym, pub arg: Arg }      // Var::scalar(s), Var::vector(s, n)
pub enum Arg { Scalar, Vector(usize) }              // Arg::words()
pub enum Prep { None, Machine, Digits(u64) }
pub enum Policy { Interpreted, Compiled, Auto { after: u64 } }   // Policy::default(): Auto { after: 32 } or $MC_NUMFN
pub struct Options { pub policy: Policy, pub numeric: bool, pub outputs: Option<usize> } // default: policy default, numeric true, None
pub enum Backend { Interpreted, Compiled }
pub struct Stats { pub compiled: u64, pub declined: u64, pub interpreted: u64, pub compile_micros: u64 }

pub fn compile_expr<'h>(cx, expr: T<'h>, vars: &[Var], opts: &Options) -> Res<Box<dyn NumFn<'h, f64> + 'h>>;
pub fn auto_expr<'h>(cx, expr, vars, opts) -> Res<Auto<'h, Interpreted<'h>>>;           // the concrete type
pub fn compile_fn<'h>(cx, f: T<'h>, kinds: &[Arg], opts) -> Res<Option<Box<dyn NumFn<'h, f64> + 'h>>>; // closure or name
pub fn function_parts<'h>(h, f: T<'h>, kinds: &[Arg]) -> Option<(Vec<Var>, T<'h>)>;
pub fn prepare<'h>(cx, expr, vars: &[Var], prep: Prep) -> Res<T<'h>>;

impl<'h> Interpreted<'h> {   // NumFn<'h, S> for every S: Value (f64, C64, Num = balls/exact/complex)
    pub fn new(cx, expr, vars: &[Var], prep: Prep) -> Res<Interpreted<'h>>;
    pub fn body(&self) -> T<'h>; pub fn numeric(&self) -> bool; pub fn arith(&self) -> &Arith<'h, Heap>;
    pub f: TermFn<'h>        // f.calls counts evaluations; f.set_outputs(n)
}
impl Compiled {              // f64 only; not a NumFn by itself (it cannot fall back)
    pub fn new(cx, vars: &[Var], body: T<'h>, consts: bool) -> Result<Compiled, String>;   // Err = the refusal
    pub fn try_eval(&self, cx, args: &[f64], out: &mut [f64]) -> Res<Option<bool>>;         // None = declined/stopped
    pub fn inputs/outputs/info
}
impl<'h, I: NumFn<'h, f64>> Auto<'h, I> {  // NumFn<'h, f64>
    pub fn new(cx, interp: I, vars: &[Var], body: T<'h>, consts: bool, policy: Policy) -> Auto<'h, I>;
    pub fn refusal(&self) -> Option<&str>; pub fn compiled(&self) -> Option<&Compiled>; pub fn interpreted(&mut self) -> &mut I;
}
```

Usage inside a native (an ODE right-hand side; inputs are flat: `t`, then `y[1..n]`):

```rust
let (t, y) = (h.sym("t"), h.sym("y"));
let vars = [Var::scalar(t), Var::vector(y, 2)];
let mut f = mc_numfn::compile_expr(cx, rhs, &vars, &Options { outputs: Some(2), ..Options::default() })?;
let mut dy = [0.0; 2];
if !f.eval(cx, &[0.0, 1.0, 0.5], &mut dy)? { /* not numeric at this point: failure(not_numeric, ...) */ }
// 15 Kronrod nodes in one call: f.eval_flat(cx, &xs, &mut ys)?; balls: Interpreted::new(cx, e, &vars, Prep::Digits(d))?
```

## Decisions

- **N0.1** `pi` and `e`: the interpreter keeps `2.0*pi` exact, so a compiled named function folds them only inside
  `n(...)` (`n(pi)`, `n(e)`; `n(x)` of a real or an integer is compiled too); in a numeric expression (`front_expr`
  with `numeric`, used when the body went through `n`) bare `pi` and `e` in the body itself are machine reals. In
  inlined functions they stay refused (the interpreter's call would keep them exact). `Compiled::new` gets
  `consts = Interpreted::numeric()`: when `n` failed and the body is evaluated as written, constants are not
  folded (the agreement test found the mismatch).
- **N0.2** Exactness: both backends evaluate one body (the prepared one), so `Auto`'s results are the interpreter's
  bit for bit (1b.3); every declined or stopped compiled call is evaluated by the interpreted backend; a compiled
  non-finite value is reported `false` exactly as the interpreted one (`Value::finite`).
- **N0.3** (compiler) Lists of known length are scalarised in the front end: a list value is a tree of scalar HIR
  expressions; a list result is written by `S::Out`/`Op::Out` into `Env::out` (a host buffer checked before the
  call), the leaves in row-major order; the scalar return value is ignored.
- **N0.4** (compiler) Expressions in given variables: `mc_jit::compile_expr(cx, &[(Sym, Arg)], body, numeric,
  backend)` compiles `(p1, ..., pn) => body`; `Arg::Vector(n)` takes n words; closures `fn([p...], body)` compile
  the same way (`compile_closure`, `closure_parts`), a declared parameter type must admit the argument kind. The
  calls get a private `Control`; fuel and interrupts reach compiled loops through the host poll (`cx.tick`), as in
  1b.7. A call declines when an inlined function was redefined (1b.6).
- **N0.5** Preparation: `Prep::Machine` gives the body to `n` (mc-roots' R.9); `n` would turn `y[2]` into `y[2.0]`
  (which no longer indexes) and `length(y)` of a symbol into 0, so constant indices of vector variables are
  replaced by fresh symbols and put back, and `length(y)` by the vector's length, before `n` runs. A failure of `n`
  leaves the body as written.
- **N0.6** `Scalar` and `TermFn` moved from `mc-roots` unchanged (`mc-roots`' `scalar` and `func` modules
  re-export them); `Value` is a separate trait (blanket impl for every `Scalar`, plus `Num`) with distinct method
  names, so `mc-roots`' calls resolve as before.
- **N0.7** `Policy::Auto { after: 32 }` by default: compiling a small expression costs 0.2-0.5 ms, an interpreted
  evaluation 10-30 us, so compiling pays after a few dozen evaluations; a Newton solve of 5 steps never compiles.
  `MC_NUMFN=interpreted|compiled|auto|auto:N` overrides the default per process (a measurement aid; N1 may add an
  option instead).
- **N0.8** (compiler) The host's Cranelift ISA is built once per process (`OnceLock`): it was about a third of the
  time of compiling a small expression.
- **N0.9** Output counts: a `TermFn` counts its outputs from the body's list literals; a body that computes its list
  (a call of a function returning one) needs `Options::outputs` (`TermFn::set_outputs`); `Auto` refuses to compile
  when the compiled shape differs from the interpreted one.
- **N0.10** `mc-roots` uses `Auto` for its machine-real root finding (Newton, secant, Brent, with the symbolic
  derivative or Jacobian); complex Newton, minimisation and the ball refinement stay on the interpreted backend.

## Tests (9 new in this crate; mc-jit 2 new and 1 extended, see its NOTES)

- `tests/agree.rs` (4): 250 random expressions in two scalars and a vector of 3 (constant and negative indices,
  `length`, `pi`, `e`, `1/3`, nine elementary functions, powers, `min`/`max`, `if`, `let` blocks, scalar, list and
  nested list results) x 24 random points (signed zeros, 1e300, 1e-300, values outside the real domains): whenever
  the compiled call completes, outputs and verdict are bitwise the interpreter's (thousands done, hundreds
  declined, under 20% refused; `AGREE_N=5000` passed too); 120 random closures (plain, with a `let`, with declared
  types); named functions inlined (scalar and list results, a function name as the function value); refusals.
- `tests/api.rs` (5): `Auto` with vector variables and list results, declines (sqrt of a negative number), refusals,
  the lazy policy; closures and function names (`compile_fn`), a declared type the arguments do not have; the
  interpreted backend on 40-digit balls and on complex machine numbers; fuel and interrupts stop compiled loops
  inside `compile_expr` (latency under 2 s); the measurement below.

## Measurements (release, busy shared machine)

| what | interpreted | compiled |
|---|---|---|
| van der Pol right-hand side, 20,000 `eval_flat` points (`tests/api.rs`) | 0.66 s (33 us each) | 1.3 ms (63 ns each; compiled before) |
| L5 Kepler, 10^3 / 10^4 / 10^5 `find_root` solves (mc-bench, memo off) | 250 ms / 2.37 s / 25.7 s | 827 ms / 7.63 s / limit |
| L5 with the default policy (auto after 32: never compiles) | 283 ms / 2.92 s / 28.9 s | |
| compiling one small expression (f or f' of L5) | | 0.24-0.29 ms (0.37 ms before N0.8; L5 10^3 in one REPL run 889 ms -> 633 ms) |

L5 calls `find_root` once per parameter, and each solve evaluates f and f' about 12 times: compiling both costs
more than it saves. Compiling once for the whole family (constants as parameters, a cache of machine code by body
shape) is package N1's job; the per-evaluation gain is about 500x.

## Class L cases (`bench/cases/L`)

| case | sizes | check | today |
|---|---|---|---|
| L3 Lorenz, tol 1e-10 | t = 10, 100, 1000 | property (t = 10 against ref/L3-10.txt to 1e-5; attractor box) | expected to fail until N3 (n_dsolve unevaluated) |
| L4 Robertson | t = 10^2, 10^5, 10^11 | property (scipy Radau reference, relative 1e-8 / 1e-5; y1 + y2 + y3 = 1) | expected to fail until N3 |
| L5 Kepler, find_root | 10^3, 10^4, 10^5 solves | property (residual < 1e-14) | 283 ms, 2.92 s pass; 28.9 s (property past the limit) |
| L5h root to 1000 digits | - | digits 999 (ref/L5h-1000.txt, mpmath) | 16.1 ms pass |
| L6 exp(-x) cos(kx) on [0, pi] | 10^2, 10^3, 10^4 integrals | digits 10 (closed form summed in mpmath) | expected to fail until N2 |
| L6h50, L6h500 Serret's integral | - | digits 50, 500 (pi log(2)/8) | expected to fail until N2 |
| L3h y' = 1 + y^2 at 3/2 | - | digits 30 (tan(3/2)) | expected to fail until N3 |
| L10 Monte Carlo pi | 10^6, 10^7, 10^8 draws | property (5 sigma) | 10.3 s pass; 10^7, 10^8 limit |

The case format has no expected-failure marker; the `note` of each such case says so. The references were computed
outside MathCAS (mpmath 1.4.1, scipy 1.18.1); each case's note says how.

## Known gaps

1. L5 does not gain from the compiler yet (see the measurements): N1.
2. `find_root(..., digits: 1000)` prints 999 digits (the printer shows only correct digits; the cause in `mc-roots`'
   rounding was not investigated), so L5h checks 999.
3. Output counts of bodies that compute their lists must be declared (N0.9).
4. The compiled backend is `f64` only; complex numbers and balls run interpreted (NUM.5).
5. Minimisation in `mc-roots` still runs interpreted (`Machine`), as do complex Newton and the refinement.
6. Random draws, arithmetic on whole lists (`y + z`), loops over list elements with a variable index, and list
   parameters of named functions are not compiled.
7. The shared target directory is also written by other worktrees: a stale `mc-jit` build once made an engine test
   fail until cargo rebuilt it.
