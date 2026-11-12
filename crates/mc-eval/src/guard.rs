//! The panic guard: a panic inside an evaluation becomes `failure(internal, "message (location)", f)` and the
//! session goes on (round 11).
//!
//! A native function that panics (an index out of bounds, an `expect` on a case its author thought impossible) used
//! to unwind through the evaluator, the session's pool and the engine, and end the kernel. Now:
//!
//! - [`Kernel::eval_with`](crate::Kernel::eval_with) runs the evaluation under [`std::panic::catch_unwind`]. A
//!   panic in a speculative task reaches it too: `rayon::join` and the parallel iterators resume a task's panic on
//!   the thread that waits for it, after the other half has finished.
//! - Every native call holds a [`NativeScope`]: if a panic unwinds through it, it records which native was running
//!   (the innermost one, since the innermost scope is dropped first) in the kernel's [`PanicSlot`] and sets the
//!   control block's status, so the speculative tasks still running stop at their next poll instead of finishing
//!   work that will be thrown away.
//! - A process-wide panic hook ([`install_hook`]) keeps each panic's message and source location for the failure
//!   term. While a guarded evaluation is running, a panic on a session thread is not printed to standard error (it
//!   is reported as the statement's value instead); every other panic goes to the previous hook as before. The
//!   environment variable `MC_PANIC_TRACE` set to anything prints every panic as well.
//!
//! **What is reset after a panic** (decision ROB.2). The heap needs nothing: no native or evaluator code holds a
//! heap structure half-updated while it can panic (interning and the memo finish their writes before they return,
//! a scratch scope is closed by its guard's `Drop` during unwinding, and `parking_lot` locks are released without
//! poisoning), and the safe point after the statement collects the unwound evaluation's terms like any other
//! garbage. The kernel's evaluation state is per evaluation: the control block and the cells of mutable locals are
//! reset when the next evaluation begins; the stack of frames lived in the unwound call. Definitions made before
//! the panic stay (as they would after a failure), and memo entries hold only completed sub-evaluations, which
//! are correct. The interrupt request is left alone (it belongs to the caller).
use mc_api::Sym;
use parking_lot::Mutex;
use std::any::Any;
use std::cell::RefCell;
use std::sync::atomic::{AtomicUsize, Ordering::*};
use std::sync::Once;

/// The tag of the failure a panic gives: `failure(internal, "message (location)", f)`.
pub const INTERNAL: &str = "internal";

/// The status code a native's panic sets in the control block, so that other tasks stop. It is not a [`Status`]
/// of its own ([`Status::from_u32`] reads it as an interrupt); the guard reports the panic, not the status.
///
/// [`Status`]: crate::Status
/// [`Status::from_u32`]: crate::Status::from_u32
pub(crate) const PANIC_STATUS: u32 = 6;

/// A panic as the hook saw it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PanicNote {
    /// the panic's message
    pub message: String,
    /// `file:line:column` of the `panic!` (or of the failed `unwrap`, index, ...), when known
    pub location: Option<String>,
}

impl PanicNote {
    /// The text of the failure term: the message, then the location in parentheses.
    pub fn text(&self) -> String {
        match &self.location {
            Some(l) => format!("{} ({l})", self.message),
            None => self.message.clone(),
        }
    }
}

thread_local! {
    /// The last panic on this thread (written by the hook before the unwinding starts).
    static LAST: RefCell<Option<PanicNote>> = const { RefCell::new(None) };
}

/// The last panic of any thread (a panic in a stolen task is caught on another thread).
static LAST_ANY: Mutex<Option<PanicNote>> = Mutex::new(None);

/// Guarded evaluations running in this process: while there are any, panics on session threads are not printed.
static GUARDED: AtomicUsize = AtomicUsize::new(0);

static HOOK: Once = Once::new();

/// Install the panic hook (once per process; later calls do nothing). The kernel does it when it is made.
pub fn install_hook() {
    HOOK.call_once(|| {
        let previous = std::panic::take_hook();
        let trace = std::env::var_os("MC_PANIC_TRACE").is_some();
        std::panic::set_hook(Box::new(move |info| {
            let location = info.location().map(|l| l.to_string());
            let note = PanicNote { message: payload_text(info.payload()), location };
            // `try_with`: a panic while the thread's locals are being destroyed must not panic again
            let _ = LAST.try_with(|l| *l.borrow_mut() = Some(note.clone()));
            *LAST_ANY.lock() = Some(note);
            let quiet = GUARDED.load(Relaxed) > 0 && mc_core::on_session_thread();
            if trace || !quiet {
                previous(info);
            }
        }));
    });
}

/// The message of a panic payload (`panic!` with a literal or a formatted message; anything else is unnamed).
pub fn payload_text(payload: &(dyn Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "a panic without a message".to_string()
    }
}

/// What the hook kept for the panic whose payload is `payload`: this thread's last panic, else the last panic of
/// any thread if its message matches (the panic happened in a task another thread ran), else the message alone.
pub(crate) fn note_for(payload: &(dyn Any + Send)) -> PanicNote {
    let message = payload_text(payload);
    let mine = LAST.try_with(|l| l.borrow_mut().take()).ok().flatten();
    if let Some(n) = mine.filter(|n| n.message == message) {
        return n;
    }
    if let Some(n) = LAST_ANY.lock().take().filter(|n| n.message == message) {
        return n;
    }
    PanicNote { message, location: None }
}

/// Run `f`, turning a panic into the hook's [`PanicNote`] for it (for callers outside the evaluator: the engine
/// guards reading and printing this way). Installs the hook if it is not yet installed.
pub fn catch<R>(f: impl FnOnce() -> R) -> Result<R, PanicNote> {
    install_hook();
    let _guarded = Guarded::enter();
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).map_err(|p| note_for(&*p))
}

/// Marks a guarded evaluation as running (see [`install_hook`]); dropped when it ends.
pub(crate) struct Guarded(());

impl Guarded {
    pub(crate) fn enter() -> Guarded {
        GUARDED.fetch_add(1, Relaxed);
        Guarded(())
    }
}

impl Drop for Guarded {
    fn drop(&mut self) {
        GUARDED.fetch_sub(1, Relaxed);
    }
}

/// Where the kernel keeps the native a panic unwound through, until the guard reads it.
#[derive(Default)]
pub(crate) struct PanicSlot(Mutex<Option<(Sym, PanicNote)>>);

impl PanicSlot {
    /// The first native recorded since the last take (the innermost of the first panic).
    pub(crate) fn take(&self) -> Option<(Sym, PanicNote)> {
        self.0.lock().take()
    }
}

/// Held across a native call: on unwinding, records the native's symbol and stops the other tasks.
pub(crate) struct NativeScope<'k> {
    pub(crate) sym: Sym,
    pub(crate) slot: &'k PanicSlot,
    pub(crate) ctl: &'k crate::Control,
}

impl Drop for NativeScope<'_> {
    #[inline]
    fn drop(&mut self) {
        if std::thread::panicking() {
            self.record();
        }
    }
}

impl NativeScope<'_> {
    #[cold]
    fn record(&self) {
        let note = LAST.try_with(|l| l.borrow().clone()).ok().flatten().unwrap_or_default();
        let mut slot = self.slot.0.lock();
        if slot.is_none() {
            *slot = Some((self.sym, note));
        }
        let _ = self.ctl.status.compare_exchange(0, PANIC_STATUS, AcqRel, Relaxed);
    }
}
