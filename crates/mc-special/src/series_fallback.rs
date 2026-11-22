//! Series of special functions: a wrapper around the `series` native. When the series kernels
//! decline (a special function at a logarithmic branch point, a pole whose derivatives the Taylor table cannot
//! evaluate, an expansion at infinity), the call is handed to the library helper `ds_series` (`lib/special.mc`),
//! which writes each special function of the variable as its truncated expansion at the point (Taylor, logarithmic
//! or Laurent terms near a finite point; the asymptotic power series at infinity, with an exponential or oscillating
//! factor kept outside when there is one) and calls `series` again. The helper answers `ds_none` when it has no
//! expansion, and the call then stays unevaluated as before.
use mc_api::{Store, View};
use mc_core::T;
use mc_eval::{Kernel, Native, NativeCx, Reply, Res};

/// The library helper the wrapper delegates to.
pub const HELPER: &str = "ds_series";
/// The helper's answer when it has no expansion.
pub const NONE: &str = "ds_none";

struct SeriesSpecial {
    inner: Box<dyn Native>,
}

impl Native for SeriesSpecial {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        // a Kelvin ber or bei at 0: the derivative table of the native grows exponentially with the order (each
        // derivative is two terms of the neighbouring orders), so the closed series of the helper answers first; the
        // same for the spherical Hankel functions, whose derivatives at a point are functions of other orders
        // without closed values there
        if let [e, x, x0, n, rest @ ..] = args {
            let h = cx.heap();
            if let (View::Sym(_), View::Int(_), View::App { head, args: ea }) = (h.view(*x), h.view(*n), h.view(*e)) {
                let kelvin = matches!(h.view(*x0), View::Int(0))
                    && ea.len() == 1
                    && ea[0] == *x
                    && ["kelvin_ber", "kelvin_bei"].iter().any(|s| head == h.sym_term(h.sym(s)));
                let first = kelvin || mentions_head(h, *e, &FIRST);
                if first {
                    let mut call = vec![*e, *x, *x0, *n];
                    call.extend_from_slice(rest);
                    let v = cx.eval(h.call(h.sym(HELPER), &call))?;
                    let stuck = match h.view(v) {
                        View::Sym(s) => &*h.sym_name(s) == NONE,
                        View::App { head, .. } => head == h.sym_term(h.sym(HELPER)),
                        _ => false,
                    };
                    if !stuck {
                        return Ok(Reply::Done(v));
                    }
                }
            }
        }
        // a bilateral series at z = 0 has terms in z^k and z^-k without end: no power series there (the Taylor table
        // gave z H'(0) + ..., with no constant term)
        if let [e, x, x0, ..] = args {
            let h = cx.heap();
            if let View::App { head, args: ea } = h.view(*e) {
                if head == h.sym_term(h.sym("bilateral_hypergeometric_pfq")) && ea.len() == 3 && ea[2] == *x && matches!(h.view(*x0), View::Int(0)) {
                    return Ok(Reply::Decline);
                }
            }
        }
        let r = self.inner.call(cx, t, args)?;
        let h = cx.heap();
        // a value with unevaluated derivatives (the Taylor table of a function whose derivatives do not evaluate at
        // the point) is replaced by the library's expansion when it has one; a table with infinite values is no answer
        let infinite = match &r {
            Reply::Done(v) | Reply::Again(v) => mentions_infinite_bessel(h, *v),
            _ => false,
        };
        let unresolved = !infinite
            && match &r {
                Reply::Done(v) | Reply::Again(v) => mentions_derivative(h, *v),
                _ => false,
            };
        if !matches!(r, Reply::Decline) && !unresolved && !infinite {
            return Ok(r);
        }
        // series(e, x, x0, n) with a symbol x and an integer order only
        let [e, x, x0, n, rest @ ..] = args else { return Ok(r) };
        if !matches!(h.view(*x), View::Sym(_)) || !matches!(h.view(*n), View::Int(_)) || !matches!(h.view(*e), View::App { .. }) {
            return Ok(r);
        }
        let mut call = vec![*e, *x, *x0, *n];
        call.extend_from_slice(rest);
        let v = cx.eval(h.call(h.sym(HELPER), &call))?;
        let stuck = match h.view(v) {
            View::Sym(s) => &*h.sym_name(s) == NONE,
            View::App { head, .. } => head == h.sym_term(h.sym(HELPER)),
            _ => false,
        };
        Ok(if !stuck {
            Reply::Done(v)
        } else if unresolved {
            r
        } else {
            Reply::Decline
        })
    }
}

/// Heads whose expansions the helper gives before the native's Taylor table is tried.
const FIRST: [&str; 2] = ["spherical_hankel_h1", "spherical_hankel_h2"];

/// Does `t` contain a call whose head is one of `names`?
fn mentions_head<'h>(h: &'h mc_core::Heap, t: T<'h>, names: &[&str]) -> bool {
    let heads: Vec<T<'h>> = names.iter().map(|s| h.sym_term(h.sym(s))).collect();
    let mut stack = mc_api::walk::Walk::new(t);
    while let Some(u) = stack.pop() {
        if let View::App { head, args } = h.view(u) {
            if heads.contains(&head) {
                return true;
            }
            stack.push(head);
            stack.extend_from_slice(args);
        }
    }
    false
}

/// Does `t` mention the symbol `derivative`?
fn mentions_derivative<'h>(h: &'h mc_core::Heap, t: T<'h>) -> bool {
    let d = h.sym_term(h.sym("derivative"));
    let mut stack = mc_api::walk::Walk::new(t);
    while let Some(u) = stack.pop() {
        if u == d {
            return true;
        }
        if let View::App { head, args } = h.view(u) {
            stack.push(head);
            stack.extend_from_slice(args);
        }
    }
    false
}

/// Does `t` mention Y, K or a Hankel function at the argument 0, where each is infinite for every order (DLMF
/// 10.7.1, 10.30.2)? A Taylor table with such values is no expansion.
fn mentions_infinite_bessel<'h>(h: &'h mc_core::Heap, t: T<'h>) -> bool {
    let heads = ["bessel_y", "bessel_k", "hankel_h1", "hankel_h2"].map(|s| h.sym_term(h.sym(s)));
    let mut stack = mc_api::walk::Walk::new(t);
    while let Some(u) = stack.pop() {
        if let View::App { head, args } = h.view(u) {
            if heads.contains(&head) && args.len() == 2 && matches!(h.view(args[1]), View::Int(0)) {
                return true;
            }
            stack.push(head);
            stack.extend_from_slice(args);
        }
    }
    false
}

/// Wrap the `series` native (installed by `mc-poly` before this crate).
pub fn install(k: &mut Kernel, h: &mc_core::Heap) {
    k.wrap_native(h.sym("series"), |inner| Box::new(SeriesSpecial { inner }));
}
