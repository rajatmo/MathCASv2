//! A temporary copy of package 1a's hook for compiled specialisations, until its branch merges (the coordinator
//! reconciles: this module goes, and [`Compiled`] and `register_compiled` become `mc-eval`'s).
//!
//! The shape is the one the round-3 brief fixes for 1a: `Kernel::register_compiled(head, c: Arc<dyn Compiled>)`,
//! with `trait Compiled: Send + Sync { fn call(&self, cx, args) -> Option<T> }`, where `None` declines. The only
//! adaptation is `NativeCx`'s three lifetime parameters (`'a, 'k, 'h`; the brief writes two).
//!
//! Here the hook is a native implementation of the head: dispatch calls natives before rules, so a specialisation
//! that declines lets the head's rules run, exactly as the brief specifies. Registering replaces any native the
//! head had; the specialisation's owner (the compiler) refuses heads with a native of their own.
use mc_core::{Sym, T};
use mc_eval::{Kernel, Native, NativeCx, Reply, Res};
use std::sync::Arc;

/// A compiled specialisation of a head: it decides from the evaluated arguments whether it applies, and returns the
/// value, or declines with `None` (the head's rules are tried next).
pub trait Compiled: Send + Sync {
    /// The value of `head(args...)`, or `None` to decline.
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, args: &[T<'h>]) -> Option<T<'h>>;
}

/// Registration of compiled specialisations (package 1a's `Kernel::register_compiled`, as an extension trait so
/// that the call reads the same).
pub trait RegisterCompiled {
    /// Give `head` the compiled specialisation `c`, replacing an earlier one.
    fn register_compiled(&mut self, head: Sym, c: Arc<dyn Compiled>);
}

impl RegisterCompiled for Kernel {
    fn register_compiled(&mut self, head: Sym, c: Arc<dyn Compiled>) {
        let a = self.attributes(head);
        self.register(head, a, Box::new(Dispatch(c)));
    }
}

/// The native that consults a specialisation.
struct Dispatch(Arc<dyn Compiled>);

impl Native for Dispatch {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        Ok(match self.0.call(cx, args) {
            Some(v) => Reply::Done(v),
            None => Reply::Decline,
        })
    }
}
