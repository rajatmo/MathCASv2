//! The budget of parallel lifting (roadmap item 16A): [`crate::natives::metered`] makes an
//! [`mc_eval::Budget`] for its work and offers it to the decompositions it runs on this thread; a decomposition
//! that finds one lifts the cells of each stack in parallel ([`crate::cad`], decision n16a.3).
use mc_eval::Budget;
use std::cell::RefCell;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

thread_local! {
    static CURRENT: RefCell<Option<(Arc<Budget>, Arc<AtomicBool>)>> = const { RefCell::new(None) };
}

/// The budget offered on this thread, with the arithmetic's stop flag.
pub(crate) fn current() -> Option<(Arc<Budget>, Arc<AtomicBool>)> {
    CURRENT.with(|c| c.borrow().clone())
}

/// Offers a budget on this thread until dropped (scopes nest).
pub(crate) struct Offer {
    prev: Option<(Arc<Budget>, Arc<AtomicBool>)>,
}

impl Offer {
    pub(crate) fn new(b: Budget, stop: &Arc<AtomicBool>) -> Offer {
        let prev = CURRENT.with(|c| c.replace(Some((Arc::new(b), stop.clone()))));
        Offer { prev }
    }
}

impl Offer {
    /// No budget on this thread until dropped.
    pub(crate) fn none() -> Offer {
        let prev = CURRENT.with(|c| c.replace(None));
        Offer { prev }
    }
}

impl Drop for Offer {
    fn drop(&mut self) {
        let prev = self.prev.take();
        CURRENT.with(|c| *c.borrow_mut() = prev);
    }
}

/// `f(0), f(1), ...` (each on a meter of its own) in order until `stop` holds of a value, under the budget offered
/// on this thread (in parallel when it splits); without one, in order on `m`. The fuel of the values kept is
/// charged to `m` in order, and an element whose task was stopped (an interrupt, a limit, the budget's cap) is
/// computed again on `m`, so the values are those of the sequential loop for any number of threads.
pub(crate) fn until<R: Send>(
    n: usize,
    m: &mut mc_poly::work::Meter,
    stop: &(dyn Fn(&mc_poly::work::WorkResult<R>) -> bool + Sync),
    f: &(dyn Fn(usize, &mut mc_poly::work::Meter) -> mc_poly::work::WorkResult<R> + Sync),
) -> mc_poly::work::WorkResult<Vec<R>> {
    use mc_poly::work::{Halt, Meter, Work, WorkResult};
    let mut out = Vec::with_capacity(n);
    let Some((b, flag)) = current() else {
        for i in 0..n {
            let r = f(i, m);
            let end = stop(&r);
            out.push(r?);
            if end {
                break;
            }
        }
        return Ok(out);
    };
    struct SubWork<'a> {
        sub: &'a mc_eval::Sub<'a>,
        stop: &'a AtomicBool,
    }
    impl Work for SubWork<'_> {
        fn charge(&mut self, units: u64) -> WorkResult<()> {
            if self.stop.load(std::sync::atomic::Ordering::Relaxed) || !self.sub.charge(units) {
                return Err(Halt::Stopped);
            }
            Ok(())
        }
    }
    let root = b.root();
    let vals = root.until(n, stop, &|i, sub| {
        let _scope = mc_arith::num::StopScope::enter(Some(&flag));
        // an element runs its decompositions without the budget, wherever it runs (the same fuel at any thread
        // count)
        let _none = Offer::none();
        let mut w = SubWork { sub, stop: &flag };
        let mut mm = Meter::new(&mut w);
        f(i, &mut mm)
    });
    for (i, (r, fuel)) in vals.into_iter().enumerate() {
        m.work().charge(fuel)?;
        let r = match r {
            Err(_) => {
                // stopped by an interrupt: do not compute the element again, nor the ones after it
                if flag.load(std::sync::atomic::Ordering::Relaxed) {
                    return Err(Halt::Stopped);
                }
                let _none = Offer::none();
                f(i, m)
            }
            ok => ok,
        };
        let end = stop(&r);
        out.push(r?);
        if end {
            break;
        }
    }
    Ok(out)
}
