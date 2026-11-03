//! The run-time side of compiled code: the environment block every compiled call receives, and the helpers both
//! back ends call (the poll, exact integer powers, the arithmetic's own operations, the elementary functions).
//!
//! **ABI** (decision 1b.4, after spike E's E.6): a compiled function takes its parameters (an `Int` as `i64`, a
//! `Real` as `f64`) and then a pointer to an [`Env`]; it returns one word (a boolean as 0 or 1). Its uniform entry
//! `extern "C" fn(args: *const u64, env: *mut Env) -> u64` reads the parameters from memory. A call ends normally
//! with [`Env::exit`] at [`Exit::Done`]; a decline or a stop sets it and returns 0.
use crate::ir::{Lib, NumOp};
use mc_arith::num::{Int, Rat};
use mc_arith::{Num, Real, DEFAULT_MAX_BITS};
use mc_eval::Control;
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
}

/// Byte offset of [`Env::ctl`] (ABI).
pub const ENV_CTL: i32 = std::mem::offset_of!(Env, ctl) as i32;
/// Byte offset of [`Env::exit`] (ABI).
pub const ENV_EXIT: i32 = std::mem::offset_of!(Env, exit) as i32;
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
        let mut p: &mut PollFn<'_> = poll;
        let mut env = Env {
            ctl,
            exit: Exit::Done as u32,
            poll: (&mut p as *mut &mut PollFn<'_>).cast(),
            panic: None,
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

    /// Record a decline (the first exit wins).
    pub fn decline(&mut self) {
        if self.exit == 0 {
            self.exit = Exit::Decline as u32;
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
    let ctl = env.control();
    if ctl.stop.load(Relaxed) != 0 || ctl.status.load(Relaxed) != 0 {
        env.exit = Exit::Stopped as u32;
        return 1;
    }
    // SAFETY: `poll` points to the `&mut PollFn` that `Env::with` keeps alive for the whole call.
    let f = unsafe { &mut *env.poll.cast::<&mut PollFn<'_>>() };
    match catch_unwind(AssertUnwindSafe(|| f(POLL_EVERY))) {
        Ok(true) => 0,
        Ok(false) => {
            env.exit = Exit::Stopped as u32;
            1
        }
        Err(payload) => {
            env.panic = Some(payload);
            env.exit = Exit::Stopped as u32;
            1
        }
    }
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

/// The tag of a [`NumOp`] operand in the helper's operand array.
pub const OPD_INT: u64 = 0;
/// A machine real operand.
pub const OPD_REAL: u64 = 1;
/// An exact rational operand: numerator, denominator.
pub const OPD_RATIO: u64 = 2;

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
    let nums = words
        .as_chunks::<OPD_WORDS>()
        .0
        .iter()
        .map(|w| match w[0] {
            OPD_INT => Some(Num::int(w[1] as i64)),
            OPD_REAL => Some(Num::Real(Real::Machine(f64::from_bits(w[1])))),
            _ => {
                let den = w[2] as i64;
                (den != 0).then(|| Num::rat(Rat::new(Int::from_i64(w[1] as i64), Int::from_i64(den))))
            }
        })
        .collect::<Option<Vec<Num>>>()?;
    let r = match op {
        NumOp::Sum => Num::sum(nums),
        NumOp::Product => Num::product(nums),
        NumOp::Pow => {
            let [b, e] = nums.as_slice() else { return None };
            b.pow(e, DEFAULT_MAX_BITS).ok()?
        }
    };
    match r {
        Num::Real(Real::Machine(v)) => Some(v),
        _ => None,
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
