//! Evaluation under a time or memory limit (round 19): `time_constrained(e, t)`,
//! `time_constrained(e, t, fail)`, `memory_constrained(e, b)`, `memory_constrained(e, b, fail)`; and `pause(t)`.
//!
//! | term | value |
//! |---|---|
//! | `time_constrained(e, t)` | the value of `e` if it is reached within `t` seconds, else `failure(limit, time)` |
//! | `time_constrained(e, t, fail)` | as above, `fail` evaluated in place of the failure |
//! | `memory_constrained(e, b)` | the value of `e` if evaluating it allocates at most `b` bytes, else `failure(limit, memory)` |
//! | `memory_constrained(e, b, fail)` | as above, `fail` evaluated in place of the failure |
//! | `pause(t)` | `null` after `t` seconds (interruptible) |
//!
//! **Time.** A watchdog thread waits on a channel for `t` seconds; if the evaluation is not done by then it raises the
//! interrupt of the control block, which every task polls with its fuel and every long native through
//! `NativeCx::stopped`. The evaluation of `e` then aborts as for an interrupt; the native sees its own watchdog
//! fired, withdraws the interrupt and the status, and gives the failure value. An interrupt from the user during
//! the evaluation (the watchdog had not fired) goes on as usual.
//!
//! **Memory.** The heap's young generation only grows inside an evaluation (collections happen at safe points,
//! between evaluations), so the bytes allocated by `e` are the growth of `Heap::young_bytes`. The limit is a cap the
//! fuel poll checks beside the session's memory budget (`Kernel::refill`), and the growth is checked once more after
//! `e` (a single native call may allocate a whole list between two polls). Nested limits keep the smallest cap.
use crate::ctl::Status;
use crate::defs::{Attributes, Hold};
use crate::eval::{Abort, Kernel, Res};
use crate::native::{Native, NativeCx, Reply};
use mc_core::{Heap, Store, View, T};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::time::Duration;

/// A positive real number of seconds or bytes, from an evaluated limit argument.
fn positive<'h>(h: &'h Heap, t: T<'h>) -> Option<f64> {
    let v = match h.view(t) {
        View::Int(n) => n as f64,
        View::Real(x) => x,
        View::BigInt { neg: false, .. } => f64::MAX,
        View::Rational { num, den } => match (h.view(num), h.view(den)) {
            (View::Int(a), View::Int(b)) => a as f64 / b as f64,
            _ => return None,
        },
        _ => return None,
    };
    (v.is_finite() && v >= 0.0).then_some(v)
}

/// The value of the failure branch: `fail` evaluated, or `failure(limit, what)`.
fn failed<'h>(cx: &mut NativeCx<'_, '_, 'h>, fail: Option<T<'h>>, what: &str) -> Res<Reply<'h>> {
    Ok(Reply::Done(match fail {
        Some(f) => cx.eval(f)?,
        None => cx.failure(&["limit", what]),
    }))
}

/// `time_constrained(e, t[, fail])`.
struct TimeConstrained;

impl Native for TimeConstrained {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let (e, lim, fail) = match *args {
            [e, l] => (e, l, None),
            [e, l, f] => (e, l, Some(f)),
            _ => return Ok(Reply::Decline),
        };
        let lim = cx.eval(lim)?;
        let Some(secs) = positive(cx.heap(), lim) else { return Ok(Reply::Decline) };
        if secs == 0.0 {
            return failed(cx, fail, "time");
        }
        let ctl = cx.ev.k.ctl.clone();
        let flags: Vec<Arc<AtomicBool>> = cx.ev.k.watch_flags.lock().unwrap().clone();
        let fired = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::channel::<()>();
        let watch = {
            let (ctl, fired, flags) = (ctl.clone(), fired.clone(), flags.clone());
            let dur = Duration::from_secs_f64(secs.min(1e9));
            std::thread::spawn(move || {
                if let Err(mpsc::RecvTimeoutError::Timeout) = rx.recv_timeout(dur) {
                    fired.store(true, Ordering::Release);
                    ctl.interrupt();
                    for f in &flags {
                        f.store(true, Ordering::Relaxed);
                    }
                }
            })
        };
        let r = cx.eval(e);
        let _ = tx.send(());
        let _ = watch.join();
        if !fired.load(Ordering::Acquire) {
            return Ok(Reply::Done(r?));
        }
        // our watchdog raised the interrupt: withdraw it (and the status it set, if that is what stopped `e`)
        ctl.clear_interrupt();
        for f in &flags {
            f.store(false, Ordering::Relaxed);
        }
        ctl.clear_status(Status::Interrupted);
        match r {
            // the number kernels stopped by the watchdog may have given a junk value: past the limit is a failure
            Ok(_) => failed(cx, fail, "time"),
            Err(Abort::Limit) => failed(cx, fail, "time"),
            Err(a) => Err(a),
        }
    }
}

/// `memory_constrained(e, b[, fail])`.
struct MemoryConstrained;

impl Native for MemoryConstrained {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let (e, lim, fail) = match *args {
            [e, l] => (e, l, None),
            [e, l, f] => (e, l, Some(f)),
            _ => return Ok(Reply::Decline),
        };
        let lim = cx.eval(lim)?;
        let h = cx.heap();
        let Some(bytes) = positive(h, lim) else { return Ok(Reply::Decline) };
        let k: &Kernel = cx.ev.k;
        let start = h.young_bytes();
        let cap = start.saturating_add(bytes.min(usize::MAX as f64 / 2.0) as usize);
        let outer = k.mem_cap.load(Ordering::Relaxed);
        let mine = cap < outer && cap < k.young_cap;
        if mine {
            k.mem_cap.store(cap, Ordering::Relaxed);
        }
        let r = cx.eval(e);
        if mine {
            k.mem_cap.store(outer, Ordering::Relaxed);
        }
        // the arena grows by whole chunks: a small value is measured by its own size
        let over = h.young_bytes() > cap || matches!(&r, Ok(v) if term_bytes(h, *v, bytes as usize) > bytes as usize);
        match r {
            Ok(_) if over => failed(cx, fail, "memory"),
            Ok(v) => Ok(Reply::Done(v)),
            // our cap stopped `e`: withdraw the memory status
            Err(Abort::Limit) if mine && k.ctl.status() == Status::Memory && h.young_bytes() <= k.young_cap => {
                k.ctl.clear_status(Status::Memory);
                failed(cx, fail, "memory")
            }
            Err(a) => Err(a),
        }
    }
}

/// An estimate of the bytes term `t` takes (16 per leaf, 16 plus 8 per argument per call, 8 per element of a packed
/// array; shared subterms counted once), stopping once it is past `stop`.
fn term_bytes<'h>(h: &'h Heap, t: T<'h>, stop: usize) -> usize {
    let mut seen = std::collections::HashSet::new();
    let mut stack = vec![t];
    let mut n = 0usize;
    while let Some(u) = stack.pop() {
        if n > stop {
            break;
        }
        if !seen.insert(u) {
            continue;
        }
        if let Some(p) = h.packed(u) {
            n += 16 + 8 * p.count();
            continue;
        }
        match h.view(u) {
            View::App { head, args } => {
                n += 16 + 8 * args.len();
                stack.push(head);
                stack.extend_from_slice(args);
            }
            View::Str(s) => n += 16 + s.len(),
            View::BigInt { mag, .. } => n += 16 + 8 * mag.len(),
            _ => n += 16,
        }
    }
    n
}

/// `pause(t)`: wait `t` seconds, polling the interrupt every 20 ms.
struct Pause;

impl Native for Pause {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let [lim] = *args else { return Ok(Reply::Decline) };
        let Some(secs) = positive(cx.heap(), lim) else { return Ok(Reply::Decline) };
        let end = std::time::Instant::now() + Duration::from_secs_f64(secs.min(1e9));
        loop {
            if cx.stopped() {
                return Err(Abort::Limit);
            }
            let now = std::time::Instant::now();
            if now >= end {
                return Ok(Reply::Done(cx.null()));
            }
            std::thread::park_timeout((end - now).min(Duration::from_millis(20)));
        }
    }
}

/// Register the functions of this module.
pub(crate) fn register(k: &mut Kernel, h: &Heap) {
    let held = Attributes { hold: Hold::All, effect: true, ..Attributes::default() };
    k.register(h.sym("time_constrained"), held, Box::new(TimeConstrained));
    k.register(h.sym("memory_constrained"), held, Box::new(MemoryConstrained));
    k.register(h.sym("pause"), Attributes { effect: true, ..Attributes::default() }, Box::new(Pause));
}
