//! Where evaluation time goes (decision sp-core.5): `profile(e)` and `timing(e, breakdown: true)`.
//!
//! While a profiled evaluation runs (its task's [`Cx::prof`] is set; speculative tasks inherit it), the evaluator
//! measures
//!
//! - every **native** call: the time from the call to its reply, *inclusive* of the evaluations it asks for and
//!   *own* (exclusive) of the profiled calls inside it;
//! - every **compiled specialisation** called for a head (`compile(f)`), as compiled code;
//! - every call of a function **defined by rules** (`fn f(x) = ...`): from the rule's rewrite until the evaluation
//!   level of the call finishes (the body's evaluation included), with its own time the part not spent in other
//!   profiled calls.
//!
//! The counts and times are kept per head symbol in a table of atomics, so the worker threads of a parallel
//! evaluation record into it directly. When profiling is off the evaluator pays one flag test per native call,
//! rule rewrite and finished level.
//!
//! The per-task bookkeeping: `prof_child` sums the inclusive times of the profiled calls finished inside the
//! current one (its own time is its inclusive time minus that), and `prof_stack` holds the rule calls in progress
//! with the depth of their evaluation level, closed when a level at that depth (or above) finishes.
//!
//! Speculative tasks record into the same table, so in a parallel evaluation the times add up over the threads
//! (they may exceed the wall-clock time), and the native that split the work (`map`, `table`) counts the time it
//! waited for the other threads as its own.
use crate::conc::SymTable;
use mc_core::Sym;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::Relaxed};
use std::time::Instant;

/// What a measured call was.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum CallKind {
    /// a native (library code in Rust)
    Native,
    /// a compiled specialisation (machine code)
    Compiled,
    /// a function defined by rules (evaluated by the interpreter)
    Rules,
}

impl CallKind {
    const ALL: [CallKind; 3] = [CallKind::Native, CallKind::Compiled, CallKind::Rules];

    fn index(self) -> usize {
        match self {
            CallKind::Native => 0,
            CallKind::Compiled => 1,
            CallKind::Rules => 2,
        }
    }

    /// The name of the kind in reports.
    pub fn name(self) -> &'static str {
        match self {
            CallKind::Native => "library",
            CallKind::Compiled => "compiled",
            CallKind::Rules => "definitions",
        }
    }
}

/// Counts and times (nanoseconds) of one symbol's calls of one kind.
#[derive(Default)]
struct Stat {
    calls: AtomicU64,
    total: AtomicU64,
    own: AtomicU64,
}

/// One symbol's statistics, by kind.
#[derive(Default)]
struct SymStats {
    by_kind: [Stat; 3],
    /// listed in `Profiler::touched`
    listed: AtomicBool,
}

/// One line of a profile: a symbol's calls of one kind.
#[derive(Clone, Copy, Debug)]
pub struct ProfileLine {
    /// the head symbol of the calls
    pub sym: Sym,
    /// what the calls were
    pub kind: CallKind,
    /// how many calls finished
    pub calls: u64,
    /// seconds, the calls inside included
    pub total: f64,
    /// seconds, without the profiled calls inside
    pub own: f64,
}

/// The kernel's profile table.
#[derive(Default)]
pub(crate) struct Profiler {
    table: SymTable<SymStats>,
    touched: Mutex<Vec<Sym>>,
    /// a profiled evaluation is running (profiles do not nest)
    pub(crate) active: AtomicBool,
}

impl Profiler {
    /// Record one call of `s` of kind `kind`.
    pub(crate) fn record(&self, s: Sym, kind: CallKind, total_ns: u64, own_ns: u64) {
        let st = self.table.get_or_init(s.0);
        if !st.listed.swap(true, Relaxed) {
            self.touched.lock().push(s);
        }
        let k = &st.by_kind[kind.index()];
        k.calls.fetch_add(1, Relaxed);
        k.total.fetch_add(total_ns, Relaxed);
        k.own.fetch_add(own_ns, Relaxed);
    }

    /// Forget every measurement.
    pub(crate) fn reset(&self) {
        let mut t = self.touched.lock();
        for s in t.drain(..) {
            let st = self.table.get_or_init(s.0);
            st.listed.store(false, Relaxed);
            for k in &st.by_kind {
                k.calls.store(0, Relaxed);
                k.total.store(0, Relaxed);
                k.own.store(0, Relaxed);
            }
        }
    }

    /// The measurements so far, one line per symbol and kind with calls.
    pub(crate) fn lines(&self) -> Vec<ProfileLine> {
        let t = self.touched.lock();
        let mut out = vec![];
        for &s in t.iter() {
            let st = self.table.get_or_init(s.0);
            for kind in CallKind::ALL {
                let k = &st.by_kind[kind.index()];
                let calls = k.calls.load(Relaxed);
                if calls > 0 {
                    let secs = |a: &AtomicU64| a.load(Relaxed) as f64 * 1e-9;
                    out.push(ProfileLine { sym: s, kind, calls, total: secs(&k.total), own: secs(&k.own) });
                }
            }
        }
        out
    }
}

/// A rule call in progress on a task.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ProfFrame {
    /// the depth of the call's evaluation level (`Cx::depth` while it is processed: unique along a chain of nested
    /// levels, natives' own evaluations included)
    pub(crate) depth: usize,
    pub(crate) sym: Sym,
    pub(crate) start: Instant,
    /// the enclosing call's `prof_child` when this one started
    pub(crate) saved: u64,
}

/// Nanoseconds since `start`.
#[inline]
pub(crate) fn nanos(start: Instant) -> u64 {
    start.elapsed().as_nanos().min(u64::MAX as u128) as u64
}
