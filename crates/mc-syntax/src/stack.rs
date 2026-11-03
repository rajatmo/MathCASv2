//! Running the recursive phases (parsing and lowering) on a stack of known size.
//!
//! Their recursion follows the nesting of the source and is bounded by [`crate::MAX_DEPTH`], but the stack a
//! nesting level takes depends on the build (unoptimised frames are many times larger) and the caller's stack is
//! unknown (1 MB on a Windows main thread). So the work runs on a scoped thread whose stack is reserved, not
//! committed: memory is only used as deep as the recursion actually goes, and the bound holds for every caller.
use std::sync::Mutex;

/// Stack reserved for parsing and lowering: room for [`crate::MAX_DEPTH`] levels with a wide margin even in
/// unoptimised builds.
pub const STACK_BYTES: usize = 64 << 20;

/// Runs `f` on a thread with [`STACK_BYTES`] of stack and returns its result; a panic in `f` is resumed here. If no
/// thread can be started, `f` runs on the current thread.
pub fn on_big_stack<R: Send, F: FnOnce() -> R + Send>(f: F) -> R {
    let job = Mutex::new(Some(f));
    let take = || job.lock().ok().and_then(|mut g| g.take());
    let spawned = std::thread::scope(|s| {
        let handle = std::thread::Builder::new()
            .name("mc-syntax".into())
            .stack_size(STACK_BYTES)
            .spawn_scoped(s, || take().map(|f| f()));
        match handle {
            Ok(h) => match h.join() {
                Ok(r) => r,
                Err(panic) => std::panic::resume_unwind(panic),
            },
            Err(_) => None,
        }
    });
    match spawned {
        Some(r) => r,
        // The thread could not be created, so the job was never taken.
        None => match take() {
            Some(f) => f(),
            None => unreachable!("the job runs exactly once"),
        },
    }
}
