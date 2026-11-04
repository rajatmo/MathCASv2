//! The run-time side of compiled code: the environment block every compiled call receives, and the helpers both
//! back ends call (the poll, exact integer powers, the arithmetic's own operations, its powers of machine reals,
//! the order key of machine reals, the elementary functions, random draws from the session stream, the complex
//! functions).
//!
//! **ABI** (decision 1b.4, after spike E's E.6): a compiled function takes its parameters (an `Int` as `i64`, a
//! `Real` as `f64`) and then a pointer to an [`Env`]; it returns one word (a boolean as 0 or 1). Its uniform entry
//! `extern "C" fn(args: *const u64, env: *mut Env) -> u64` reads the parameters from memory. A call ends normally
//! with [`Env::exit`] at [`Exit::Done`]; a decline or a stop sets it and returns 0.
use crate::ir::{Draw, Lib, NumOp};
use mc_arith::num::{Int, Rat};
use mc_arith::{Num, Real, DEFAULT_MAX_BITS};
use mc_eval::Control;
use mc_random::draw::{normal_ok, real_range_ok};
use mc_random::Stream;
use std::ffi::c_void;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::Ordering::Relaxed;

/// How a compiled call ended.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum Exit {
    /// The result is valid.
    Done = 0,
    /// The exact result is not what the compiled code can represent (an overflow, an inexact quotient, a value
    /// outside a function's real domain): the interpreter evaluates the call instead.
    Decline = 1,
    /// The poll found the evaluation stopped (interrupt, fuel, or a speculative task's exhausted share).
    Stopped = 2,
}

/// The block a compiled call receives. Compiled code reads `ctl` and writes `exit`; the rest is the host's.
#[repr(C)]
pub struct Env {
    /// The evaluator's control block (decision E.6: the very atomics the interpreter polls).
    pub ctl: *const Control,
    /// How the call ended ([`Exit`] as `u32`).
    pub exit: u32,
    /// The host's poll: called every [`POLL_EVERY`] loop iterations with the fuel they cost; `false` stops.
    poll: *mut c_void,
    /// A panic raised inside the host's poll, resumed once the compiled code has returned.
    panic: Option<Box<dyn std::any::Any + Send>>,
    /// The output buffer of a vector result (decision N0.3): [`crate::ir::Op::Out`] writes word `k` at `out + 8k`;
    /// null with `nout` 0 for a scalar call.
    pub out: *mut u64,
    /// The length of the output buffer in words (the host checks it covers every slot before the call).
    pub nout: usize,
    /// The session's random stream for the draws of this call (package N4b), or null: a draw then declines. The
    /// host holds the generator's lock for the whole call and restores the stream when the call does not complete,
    /// so that the interpreter, evaluating the call instead, draws the same numbers.
    pub stream: *mut Stream,
    /// The vectors the call made ([`vec_new`]), freed with the environment unless one became the result.
    vectors: Vec<Vec<u64>>,
    /// The words of a vector result ([`vec_out`]), taken by the host after the call.
    result: Option<Vec<u64>>,
}

/// The most words of one vector compiled code makes (2^26, the library's `table` limit: a larger `table` is the
/// interpreter's `failure(limit, ...)`, so the compiled call declines).
pub const MAX_VECTOR: i64 = 1 << 26;

/// Byte offset of [`Env::ctl`] (ABI).
pub const ENV_CTL: i32 = std::mem::offset_of!(Env, ctl) as i32;
/// Byte offset of [`Env::exit`] (ABI).
pub const ENV_EXIT: i32 = std::mem::offset_of!(Env, exit) as i32;
/// Byte offset of [`Env::out`] (ABI).
pub const ENV_OUT: i32 = std::mem::offset_of!(Env, out) as i32;
/// Byte offset of the stop flag and status pair in [`Control`], read as one 64-bit word (both zero: running).
pub const CTL_STOP_STATUS: i32 = mc_eval::ctl::CTL_STOP as i32;
const _: () = assert!(mc_eval::ctl::CTL_STATUS == mc_eval::ctl::CTL_STOP + 4);

/// Loop iterations between polls, and the fuel one poll charges (decision E.5; the evaluator's fuel block).
pub const POLL_EVERY: u64 = mc_eval::ctl::FUEL_BLOCK;

/// The host's poll: charge `n` units of fuel; `false` when the evaluation must stop.
pub type PollFn<'a> = dyn FnMut(u64) -> bool + 'a;

impl Env {
    /// Run `f` with an environment for the control block `ctl` whose polls call `poll`.
    pub fn with<R>(ctl: &Control, poll: &mut PollFn<'_>, f: impl FnOnce(&mut Env) -> R) -> (R, Exit) {
        Env::with_out(ctl, poll, &mut [], f)
    }

    /// [`Env::with`] with the output buffer `out` for a vector result.
    pub fn with_out<R>(ctl: &Control, poll: &mut PollFn<'_>, out: &mut [u64], f: impl FnOnce(&mut Env) -> R) -> (R, Exit) {
        Env::with_stream(ctl, poll, out, None, f)
    }

    /// [`Env::with_out`] with the random stream the call's draws come from.
    pub fn with_stream<R>(
        ctl: &Control,
        poll: &mut PollFn<'_>,
        out: &mut [u64],
        stream: Option<&mut Stream>,
        f: impl FnOnce(&mut Env) -> R,
    ) -> (R, Exit) {
        let mut p: &mut PollFn<'_> = poll;
        let mut env = Env {
            ctl,
            exit: Exit::Done as u32,
            poll: (&mut p as *mut &mut PollFn<'_>).cast(),
            panic: None,
            out: out.as_mut_ptr(),
            nout: out.len(),
            stream: stream.map_or(std::ptr::null_mut(), |s| s as *mut Stream),
            vectors: vec![],
            result: None,
        };
        let r = f(&mut env);
        if let Some(payload) = env.panic.take() {
            std::panic::resume_unwind(payload);
        }
        let exit = match env.exit {
            0 => Exit::Done,
            1 => Exit::Decline,
            _ => Exit::Stopped,
        };
        (r, exit)
    }

    /// The control block.
    pub fn control(&self) -> &Control {
        // SAFETY: `ctl` comes from a reference that outlives the environment (`Env::with`).
        unsafe { &*self.ctl }
    }

    /// Write output word `k`; `false` when the buffer is too short (the interpreter back end checks every write).
    pub fn write_out(&mut self, k: usize, w: u64) -> bool {
        if k >= self.nout {
            return false;
        }
        // SAFETY: `out` points to `nout` live words (`Env::with_out`), and `k < nout`.
        unsafe { *self.out.add(k) = w };
        true
    }

    /// Record a decline (the first exit wins).
    pub fn decline(&mut self) {
        if self.exit == 0 {
            self.exit = Exit::Decline as u32;
        }
    }

    /// The words of the vector result the call set ([`crate::ir::Op::VOut`]), once.
    pub fn take_result(&mut self) -> Option<Vec<u64>> {
        self.result.take()
    }

    /// Check the stop flag and charge `n` units of fuel through the host's poll; `false` (with the exit set) when
    /// the call must stop.
    fn charge(&mut self, n: u64) -> bool {
        let ctl = self.control();
        if ctl.stop.load(Relaxed) != 0 || ctl.status.load(Relaxed) != 0 {
            self.exit = Exit::Stopped as u32;
            return false;
        }
        // SAFETY: `poll` points to the `&mut PollFn` that `Env::with` keeps alive for the whole call.
        let f = unsafe { &mut *self.poll.cast::<&mut PollFn<'_>>() };
        match catch_unwind(AssertUnwindSafe(|| f(n))) {
            Ok(true) => true,
            Ok(false) => {
                self.exit = Exit::Stopped as u32;
                false
            }
            Err(payload) => {
                self.panic = Some(payload);
                self.exit = Exit::Stopped as u32;
                false
            }
        }
    }
}

/// The poll of compiled code, every [`POLL_EVERY`] iterations: returns nonzero when the call must stop (and has set
/// the exit).
///
/// # Safety
/// `env` points to a live [`Env`] made by [`Env::with`].
pub unsafe extern "C" fn poll(env: *mut Env) -> u32 {
    // SAFETY: the caller's contract.
    let env = unsafe { &mut *env };
    u32::from(!env.charge(POLL_EVERY))
}

/// A new zeroed vector of `n` words owned by the call's environment (package P); its address, or 0 with a decline
/// for a negative `n`, one beyond [`MAX_VECTOR`], or memory the allocator refuses.
///
/// # Safety
/// `env` points to a live [`Env`].
pub unsafe extern "C" fn vec_new(env: *mut Env, n: i64) -> u64 {
    // SAFETY: the caller's contract.
    let env = unsafe { &mut *env };
    if !(0..=MAX_VECTOR).contains(&n) {
        env.decline();
        return 0;
    }
    let mut v: Vec<u64> = Vec::new();
    if v.try_reserve_exact(n as usize).is_err() {
        env.decline();
        return 0;
    }
    v.resize(n as usize, 0);
    let p = v.as_mut_ptr() as u64;
    env.vectors.push(v);
    p
}

/// Set the call's result to the `n` leaves of the vector at `base`: a vector the call made is moved (its first `n`
/// words), any other (an argument's payload) copied. Declines for a negative `n`.
///
/// # Safety
/// `env` points to a live [`Env`]; `base` is the address of `n` readable words (a vector the call made, or an
/// argument's leaves, which the host keeps alive for the call).
pub unsafe extern "C" fn vec_out(env: *mut Env, base: u64, n: i64) -> u64 {
    // SAFETY: the caller's contract.
    let env = unsafe { &mut *env };
    if n < 0 {
        env.decline();
        return 0;
    }
    let n = n as usize;
    let words = match env.vectors.iter().position(|v| v.as_ptr() as u64 == base && v.len() >= n) {
        Some(k) => {
            let mut v = env.vectors.swap_remove(k);
            v.truncate(n);
            v
        }
        // SAFETY: the caller's contract.
        None => unsafe { vector(base, n as i64) }.to_vec(),
    };
    env.result = Some(words);
    0
}

/// The `n` words at `base` (none for `n <= 0`).
///
/// # Safety
/// `base` is the address of `n` readable words when `n > 0`.
unsafe fn vector<'a>(base: u64, n: i64) -> &'a [u64] {
    if n <= 0 {
        return &[];
    }
    // SAFETY: the caller's contract.
    unsafe { std::slice::from_raw_parts(base as *const u64, n as usize) }
}

/// The fuel of a reduction over `n` leaves (the library's `sum` on a payload: a copy and a sort, about n/4 units).
fn reduction_fuel(n: usize) -> u64 {
    (n as u64 / 4).max(1)
}

/// `sum(v)` of `n` machine reals at `base`, as the library's `sum` computes it (decision N4a.10): ascending (IEEE
/// total order, which puts -0.0 before 0.0 as the canonical order does) and folded from an exact zero, +0.0. A NaN
/// declines (the general path decides), and so does an empty vector (whose sum is the exact 0).
pub fn sum_reals(data: &[f64]) -> Option<f64> {
    if data.is_empty() || data.iter().any(|x| x.is_nan()) {
        return None;
    }
    let mut v = data.to_vec();
    v.sort_unstable_by(f64::total_cmp);
    Some(v.iter().fold(0.0, |acc, &x| acc + x))
}

/// `sum(v)` of machine integers: exact, `None` beyond `i64` (the interpreter's sum is a big integer).
pub fn sum_ints(data: &[i64]) -> Option<i64> {
    i64::try_from(data.iter().map(|&x| x as i128).sum::<i128>()).ok()
}

/// [`sum_reals`] for compiled code: charges the fuel of the work first; declines (or stops) returning 0.0.
///
/// # Safety
/// `env` points to a live [`Env`]; `base` is the address of `n` readable machine reals.
pub unsafe extern "C" fn vsum_real(env: *mut Env, base: u64, n: i64) -> f64 {
    // SAFETY: the caller's contract.
    let (env, w) = unsafe { (&mut *env, vector(base, n)) };
    if !env.charge(reduction_fuel(w.len())) {
        return 0.0;
    }
    sum_reals(mc_api::packed::words_reals(w)).unwrap_or_else(|| {
        env.decline();
        0.0
    })
}

/// [`sum_ints`] for compiled code, as [`vsum_real`].
///
/// # Safety
/// As [`vsum_real`], with `n` machine integers.
pub unsafe extern "C" fn vsum_int(env: *mut Env, base: u64, n: i64) -> i64 {
    // SAFETY: the caller's contract.
    let (env, w) = unsafe { (&mut *env, vector(base, n)) };
    if !env.charge(reduction_fuel(w.len())) {
        return 0;
    }
    sum_ints(mc_api::packed::words_ints(w)).unwrap_or_else(|| {
        env.decline();
        0
    })
}

/// `a^n` exactly for `n >= 0`; declines (returning 0) for a negative exponent or a result beyond `i64`.
///
/// # Safety
/// `env` points to a live [`Env`].
pub unsafe extern "C" fn ipow(env: *mut Env, a: i64, n: i64) -> i64 {
    match ipow_exact(a, n) {
        Some(v) => v,
        None => {
            // SAFETY: the caller's contract.
            unsafe { &mut *env }.decline();
            0
        }
    }
}

/// `a^n` when it is an `i64` and `n >= 0`.
pub fn ipow_exact(a: i64, n: i64) -> Option<i64> {
    if n < 0 {
        return None;
    }
    match a {
        0 => Some(if n == 0 { 1 } else { 0 }),
        1 => Some(1),
        -1 => Some(if n % 2 == 0 { 1 } else { -1 }),
        _ => a.checked_pow(u32::try_from(n).ok()?),
    }
}

/// `x^k` for a machine real `x` and an integer `k != 0`, exactly as the arithmetic computes it
/// (`mc_arith::Num::pow_int`): one `powf` with the signed exponent converted to a machine real, with no reciprocal
/// and no repeated multiplication. (`k = 0` gives the exact 1 there, which compiled code declines before calling.)
pub extern "C" fn pow_int(x: f64, k: i64) -> f64 {
    let e = k.unsigned_abs() as f64;
    x.powf(if k < 0 { -e } else { e })
}

/// `x^c` for machine reals whose power is real, as the arithmetic computes it (`mc_arith::Num::pow_machine`).
pub extern "C" fn pow_real(x: f64, c: f64) -> f64 {
    x.powf(c)
}

/// The integer whose signed order is the canonical order of machine reals (`mc_arith::Num::cmp_canonical`): IEEE
/// total order, which puts -0.0 before 0.0 as the canonical order's tie-break does. The one difference is NaN,
/// which the canonical order puts after every number while total order puts a NaN with the sign bit first; a sum
/// or product with a NaN operand is a NaN either way, so only the payload of that NaN can differ.
pub fn order_key(x: f64) -> i64 {
    let b = x.to_bits() as i64;
    b ^ (((b >> 63) as u64) >> 1) as i64
}

/// The machine real whose [`order_key`] is `k` (the map is its own inverse on the bits).
pub fn from_order_key(k: i64) -> f64 {
    f64::from_bits((k ^ (((k >> 63) as u64) >> 1) as i64) as u64)
}

/// The tag of a [`NumOp`] operand in the helper's operand array.
pub const OPD_INT: u64 = 0;
/// A machine real operand.
pub const OPD_REAL: u64 = 1;
/// An exact rational operand: numerator, denominator.
pub const OPD_RATIO: u64 = 2;
/// A machine complex operand: real part, imaginary part.
pub const OPD_COMPLEX: u64 = 3;
/// An exact complex operand with integer parts: real part, imaginary part.
pub const OPD_CEXACT: u64 = 4;

/// The words of one [`NumOp`] operand: tag, value (or numerator), denominator.
pub const OPD_WORDS: usize = 3;

/// A [`NumOp`] computed by the arithmetic's own code (`mc_arith::Num`), so that it agrees with the interpreter by
/// construction: `ops` holds `n` operands of [`OPD_WORDS`] words. Returns the machine real result's bits; any other
/// result (exact, complex, a division by zero) declines.
///
/// # Safety
/// `env` points to a live [`Env`]; `ops` points to `n * OPD_WORDS` readable words.
pub unsafe extern "C" fn num(env: *mut Env, op: u64, ops: *const u64, n: u64) -> u64 {
    // SAFETY: the caller's contract.
    let words = unsafe { std::slice::from_raw_parts(ops, n as usize * OPD_WORDS) };
    let op = match op {
        0 => NumOp::Sum,
        1 => NumOp::Product,
        _ => NumOp::Pow,
    };
    match num_words(op, words) {
        Some(v) => v.to_bits(),
        None => {
            // SAFETY: the caller's contract.
            unsafe { &mut *env }.decline();
            0
        }
    }
}

/// The helper's code on safe data: the machine real result of `op`, or `None`.
pub fn num_words(op: NumOp, words: &[u64]) -> Option<f64> {
    match num_value(op, words)? {
        Num::Real(Real::Machine(v)) => Some(v),
        _ => None,
    }
}

/// The value of `op` on the operands in `words`, computed by the arithmetic (`None`: a zero denominator, or an
/// error of the arithmetic).
fn num_value(op: NumOp, words: &[u64]) -> Option<Num> {
    let m = |w: u64| Real::Machine(f64::from_bits(w));
    let nums = words
        .as_chunks::<OPD_WORDS>()
        .0
        .iter()
        .map(|w| match w[0] {
            OPD_INT => Some(Num::int(w[1] as i64)),
            OPD_REAL => Some(Num::Real(m(w[1]))),
            OPD_COMPLEX => Some(Num::Complex(m(w[1]), m(w[2]))),
            OPD_CEXACT => Some(Num::complex(Real::int(w[1] as i64), Real::int(w[2] as i64))),
            _ => {
                let den = w[2] as i64;
                (den != 0).then(|| Num::rat(Rat::new(Int::from_i64(w[1] as i64), Int::from_i64(den))))
            }
        })
        .collect::<Option<Vec<Num>>>()?;
    Some(match op {
        NumOp::Sum => Num::sum(nums),
        NumOp::Product => Num::product(nums),
        NumOp::Pow => {
            let [b, e] = nums.as_slice() else { return None };
            b.pow(e, DEFAULT_MAX_BITS).ok()?
        }
    })
}

/// The machine complex result of `op` on the operands in `words` as the arithmetic computes it, or `None` when the
/// result is anything else (a real or exact number, an error).
pub fn cnum_words(op: NumOp, words: &[u64]) -> Option<(f64, f64)> {
    match num_value(op, words)? {
        Num::Complex(Real::Machine(a), Real::Machine(b)) => Some((a, b)),
        _ => None,
    }
}

/// [`num`] with a machine complex result: returns the real part's bits and writes the imaginary part's to `im`;
/// declines (returning 0) for any other result.
///
/// # Safety
/// `env` points to a live [`Env`]; `ops` points to `n * OPD_WORDS` readable words; `im` to one writable word.
pub unsafe extern "C" fn cnum(env: *mut Env, op: u64, ops: *const u64, n: u64, im: *mut u64) -> u64 {
    // SAFETY: the caller's contract.
    let words = unsafe { std::slice::from_raw_parts(ops, n as usize * OPD_WORDS) };
    let op = match op {
        0 => NumOp::Sum,
        1 => NumOp::Product,
        _ => NumOp::Pow,
    };
    match cnum_words(op, words) {
        Some((a, b)) => {
            // SAFETY: the caller's contract.
            unsafe { *im = b.to_bits() };
            a.to_bits()
        }
        None => {
            // SAFETY: the caller's contract.
            unsafe { &mut *env }.decline();
            0
        }
    }
}

/// The principal value of the elementary function `f` at `re + im i`, as the interpreter computes it for a machine
/// complex argument (`mc_numeric::Fun::machine`).
pub fn cmath_value(f: Lib, re: f64, im: f64) -> (f64, f64) {
    use mc_numeric::fun::Mach;
    use mc_numeric::machine::C64;
    let fun = mc_numeric::Fun::from_name(f.name()).expect("every compiled function is an elementary function");
    let z = fun.machine(Mach::Complex(C64::new(re, im))).c64();
    (z.re, z.im)
}

/// [`cmath_value`] for compiled code: `f` is the index of the function in [`Lib::ALL`]; returns the real part and
/// writes the imaginary part to `im`.
///
/// # Safety
/// `im` points to one writable machine real.
pub unsafe extern "C" fn cmath(f: u64, re: f64, im_in: f64, im: *mut f64) -> f64 {
    let (a, b) = cmath_value(Lib::ALL[f as usize], re, im_in);
    // SAFETY: the caller's contract.
    unsafe { *im = b };
    a
}

/// A draw on the call's stream, or `None` (no stream attached, or arguments the native would refuse: the call
/// declines before the stream moves, and the interpreter gives the native's failure).
///
/// # Safety
/// `env` points to a live [`Env`] whose `stream` is null or points to a stream nothing else uses during the call.
pub unsafe fn draw_value(env: *mut Env, d: Draw, a: u64, b: u64) -> Option<u64> {
    // SAFETY: the caller's contract.
    let env = unsafe { &mut *env };
    if env.stream.is_null() {
        return None;
    }
    // SAFETY: as above: the host holds the generator's lock for the whole call.
    let s = unsafe { &mut *env.stream };
    let (x, y) = (f64::from_bits(a), f64::from_bits(b));
    match d {
        Draw::Real => real_range_ok(x, y).then(|| s.real_in(x, y - x).to_bits()),
        Draw::Normal => normal_ok(x, y).then(|| s.normal_scaled(x, y).to_bits()),
        Draw::Integer => ((a as i64) <= (b as i64)).then(|| s.int_in(a as i64, b as i64) as u64),
    }
}

/// `random_real([a, b])` for compiled code (declines like [`draw_value`]).
///
/// # Safety
/// As [`draw_value`].
pub unsafe extern "C" fn draw_real(env: *mut Env, a: f64, b: f64) -> f64 {
    // SAFETY: the caller's contract.
    match unsafe { draw_value(env, Draw::Real, a.to_bits(), b.to_bits()) } {
        Some(w) => f64::from_bits(w),
        None => {
            // SAFETY: the caller's contract.
            unsafe { &mut *env }.decline();
            0.0
        }
    }
}

/// `random_normal(mu, sigma)` for compiled code (declines like [`draw_value`]).
///
/// # Safety
/// As [`draw_value`].
pub unsafe extern "C" fn draw_normal(env: *mut Env, mu: f64, sigma: f64) -> f64 {
    // SAFETY: the caller's contract.
    match unsafe { draw_value(env, Draw::Normal, mu.to_bits(), sigma.to_bits()) } {
        Some(w) => f64::from_bits(w),
        None => {
            // SAFETY: the caller's contract.
            unsafe { &mut *env }.decline();
            0.0
        }
    }
}

/// `random_integer([a, b])` for compiled code (declines like [`draw_value`]).
///
/// # Safety
/// As [`draw_value`].
pub unsafe extern "C" fn draw_int(env: *mut Env, a: i64, b: i64) -> i64 {
    // SAFETY: the caller's contract.
    match unsafe { draw_value(env, Draw::Integer, a as u64, b as u64) } {
        Some(w) => w as i64,
        None => {
            // SAFETY: the caller's contract.
            unsafe { &mut *env }.decline();
            0
        }
    }
}

/// The code of [`NumOp`] for the helper's `op` argument.
pub fn num_op_code(op: NumOp) -> u64 {
    match op {
        NumOp::Sum => 0,
        NumOp::Product => 1,
        NumOp::Pow => 2,
    }
}

/// The elementary functions with the C calling convention, wrapping `mc-numeric`'s shared machine functions.
pub mod math {
    use mc_numeric::machine as m;
    macro_rules! wrap {
        ($($name:ident),*) => {$(
            #[doc = concat!("`mc_numeric::machine::", stringify!($name), "`")]
            pub extern "C" fn $name(x: f64) -> f64 {
                m::$name(x)
            }
        )*};
    }
    wrap!(exp, log, sin, cos, tan, atan, asin, acos, sinh, cosh, tanh);
}

/// The C entry of an elementary function.
pub fn math_fn(f: Lib) -> extern "C" fn(f64) -> f64 {
    match f {
        Lib::Exp => math::exp,
        Lib::Log => math::log,
        Lib::Sin => math::sin,
        Lib::Cos => math::cos,
        Lib::Tan => math::tan,
        Lib::Atan => math::atan,
        Lib::Asin => math::asin,
        Lib::Acos => math::acos,
        Lib::Sinh => math::sinh,
        Lib::Cosh => math::cosh,
        Lib::Tanh => math::tanh,
    }
}

/// `floor(x)` as an integer, when `x` is finite and the floor fits in `i64`.
pub fn floor_exact(x: f64) -> Option<i64> {
    let f = x.floor();
    // -2^63 is representable; 2^63 is not an i64
    (f.is_finite() && (-9_223_372_036_854_775_808.0..9_223_372_036_854_775_808.0).contains(&f)).then_some(f as i64)
}

/// The largest integer magnitude a machine real represents exactly together with all smaller ones.
pub const EXACT_REAL_INT: i64 = 1 << 53;

/// Integer floor division and remainder with the sign of the divisor (`quotient`, `mod`); `None` for a zero divisor
/// or an overflow.
pub fn floor_divmod(a: i64, b: i64) -> Option<(i64, i64)> {
    if b == 0 {
        return None;
    }
    let (q, r) = (a.checked_div(b)?, a.checked_rem(b)?);
    Some(if r != 0 && (r < 0) != (b < 0) { (q - 1, r + b) } else { (q, r) })
}
