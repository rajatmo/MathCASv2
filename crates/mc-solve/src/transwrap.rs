//! Root objects of analytic functions (`mc_algebra::transroot`) where other functions meet them: `n` gives their
//! values, and the one-variable extrema on a closed bounded interval, the real line or a closed half-line (through
//! `tails`) give them as critical points when the methods installed before decline.
use mc_algebra::transroot::{self, Goal};
use mc_api::{Store, View, T};
use mc_eval::{Native, NativeCx, Reply, Res};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// `n(e)`, `n(e, d)` with root objects of analytic functions inside `e`: each replaced by its value first.
pub(crate) struct NumericRoots {
    pub inner: Box<dyn Native>,
    pub stop: Arc<AtomicBool>,
}

impl Native for NumericRoots {
    fn reads_term(&self) -> bool {
        self.inner.reads_term()
    }

    fn curried(&self) -> bool {
        self.inner.curried()
    }

    fn trace(&mut self, visit: &mut dyn FnMut(&mut mc_core::Raw)) {
        self.inner.trace(visit);
    }

    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        if transroot::any_made() {
            let h = cx.heap();
            let digits = match args {
                [_] => Some(None),
                [_, d] => match h.view(*d) {
                    View::Int(d @ 1..=100_000) => Some(Some(d as u64)),
                    _ => None,
                },
                _ => None,
            };
            if let Some(digits) = digits {
                if let Some(v) = transroot::numeric_objects(cx, &self.stop, args[0], digits)? {
                    let mut new = args.to_vec();
                    new[0] = v;
                    return self.inner.call(cx, t, &new);
                }
            }
        }
        self.inner.call(cx, t, args)
    }
}

/// `minimize`, `maximize`, `min_value`, `max_value`, `arg_min`, `arg_max` of a function of one variable on a
/// closed bounded interval, the real line or a closed half-line, after the methods installed before decline.
pub(crate) struct Extremum {
    pub inner: Box<dyn Native>,
    pub stop: Arc<AtomicBool>,
    pub max: bool,
    pub goal: Goal,
}

impl Native for Extremum {
    fn reads_term(&self) -> bool {
        self.inner.reads_term()
    }

    fn curried(&self) -> bool {
        self.inner.curried()
    }

    fn trace(&mut self, visit: &mut dyn FnMut(&mut mc_core::Raw)) {
        self.inner.trace(visit);
    }

    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let r = self.inner.call(cx, t, args)?;
        if !matches!(r, Reply::Decline) {
            return Ok(r);
        }
        let &[fc, x] = args else { return Ok(r) };
        let (f, cond) = match cx.as_list(fc) {
            Some(&[f, cond]) => (f, Some(cond)),
            Some(_) => return Ok(r),
            None => (fc, None),
        };
        if let Some(cond) = cond {
            if let Some(v) = transroot::minimize_interval(cx, &self.stop, f, cond, x, self.max, self.goal)? {
                return Ok(Reply::Done(v));
            }
        }
        // the real line or a closed half-line
        Ok(match crate::tails::minimize_line(cx, &self.stop, f, cond, x, self.max, self.goal)? {
            Some(v) => Reply::Done(v),
            None => r,
        })
    }
}

/// Wraps `n` and the extrema (after the natives they wrap are installed).
pub(crate) fn install(k: &mut mc_eval::Kernel, h: &mc_core::Heap, stop: &Arc<AtomicBool>) {
    let st = stop.clone();
    k.wrap_native(h.sym("n"), move |inner| Box::new(NumericRoots { inner, stop: st }));
    for (name, max, goal) in [
        ("minimize", false, Goal::Both),
        ("maximize", true, Goal::Both),
        ("min_value", false, Goal::Value),
        ("max_value", true, Goal::Value),
        ("arg_min", false, Goal::Point),
        ("arg_max", true, Goal::Point),
    ] {
        let st = stop.clone();
        k.wrap_native(h.sym(name), move |inner| Box::new(Extremum { inner, stop: st, max, goal }));
    }
}
