//! Functions of truncated power series (gap G59): `sin(s)`, `sqrt(1 + s)`, `bessel_j(0, s)` with `s` a
//! `series_data` term give the series of the composition, to the precision the input determines.
//!
//! The series algebra lives in a crate above this one, so it is registered here as a hook ([`register`], once at
//! start-up by the engine); the elementary and special function natives ask [`apply`] first when an argument is a
//! series. Without a registered hook the calls stay as they are.

use mc_core::{Heap, Store, View, T};
use mc_eval::{NativeCx, Res};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, RwLock};

/// The series of `op(args...)` at the point of the series arguments, or `None` when it does not expand there.
pub type Hook = for<'h> fn(cx: &mut NativeCx<'_, '_, 'h>, stop: &Arc<AtomicBool>, op: &str, args: &[T<'h>]) -> Res<Option<T<'h>>>;

static HOOK: RwLock<Option<Hook>> = RwLock::new(None);

/// Register the series hook (a later registration replaces the earlier one).
pub fn register(hook: Hook) {
    *HOOK.write().unwrap_or_else(|e| e.into_inner()) = Some(hook);
}

/// Is some argument a `series_data` term (five arguments)?
pub fn any(h: &Heap, args: &[T<'_>]) -> bool {
    let mut head_sym = None;
    args.iter().any(|&a| match h.view(a) {
        View::App { head, args } if args.len() == 5 => head == *head_sym.get_or_insert_with(|| h.sym_term(h.sym("series_data"))),
        _ => false,
    })
}

/// The series of `op(args...)` when some argument is a series and a hook is registered.
pub fn apply<'h>(cx: &mut NativeCx<'_, '_, 'h>, stop: &Arc<AtomicBool>, op: &str, args: &[T<'h>]) -> Res<Option<T<'h>>> {
    if !any(cx.heap(), args) {
        return Ok(None);
    }
    let hook = *HOOK.read().unwrap_or_else(|e| e.into_inner());
    let Some(f) = hook else { return Ok(None) };
    if let Some(v) = f(cx, stop, op, args)? {
        return Ok(Some(v));
    }
    composed(cx, op, args)
}

/// `series(op(normal(s), ...), x, x0, p - 1)` for series arguments `s` at one point, truncated where the least
/// precise of them is (`O((x - x0)^p)`): the expansion of the function itself, for functions the series algebra
/// does not compose directly (`erf(s)`, `bessel_j(1, s)`). `None` when the series are at different points or the
/// expansion does not give a series.
fn composed<'h>(cx: &mut NativeCx<'_, '_, 'h>, op: &str, args: &[T<'h>]) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    let sd = h.sym_term(h.sym("series_data"));
    let mut point = None;
    let mut order = i64::MAX;
    let mut plain = Vec::with_capacity(args.len());
    for &a in args {
        match h.view(a) {
            View::App { head, args: &[x, x0, _, _, p] } if head == sd => {
                let View::Int(p) = h.view(p) else { return Ok(None) };
                match point {
                    None => point = Some((x, x0)),
                    Some(pt) if pt == (x, x0) => {}
                    Some(_) => return Ok(None),
                }
                order = order.min(p);
                plain.push(cx.eval(h.call(h.sym("normal"), &[a]))?);
            }
            _ => plain.push(a),
        }
    }
    let Some((x, x0)) = point else { return Ok(None) };
    if !(1..=4096).contains(&order) {
        return Ok(None);
    }
    let e = h.call(h.sym(op), &plain);
    let v = cx.eval(h.call(h.sym("series"), &[e, x, x0, h.int(order - 1)]))?;
    Ok(match h.view(v) {
        View::App { head, .. } if head == sd => Some(v),
        _ => None,
    })
}
