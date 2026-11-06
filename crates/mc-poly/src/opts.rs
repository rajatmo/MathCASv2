//! The `assumptions:` option of the symbolic functions (package io-sym, decision IOSYM.4): `f(..., assumptions: a)`
//! is `f(...)` evaluated with the facts `a` added to the assumption context, as inside `assuming(a, f(...))`, for
//! every function wrapped by [`WithAssumptions`] (`integrate`, `limit`, `series`, `solve`, `d_solve`, the function
//! properties, ...), so the option behaves the same everywhere; `reduce` takes the facts as further conditions.
//! `assumptions: true` adds nothing; `assumptions: false` is left to the function (`mc-solve` marks its inner calls
//! with it).
use mc_api::{names, Store, View, T};
use mc_core::Raw;
use mc_eval::{Native, NativeCx, Reply, Res};

/// The call `name(args)` without its `assumptions` option, evaluated under the option's facts; `None` when the call
/// has no such option (or it is `false`).
pub fn assumptions_option<'h>(cx: &mut NativeCx<'_, '_, 'h>, name: &str, args: &[T<'h>]) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    let Some((&last, rest)) = args.split_last() else { return Ok(None) };
    let View::App { head, args: rules } = h.view(last) else { return Ok(None) };
    if head != h.sym_term(h.sym("options")) {
        return Ok(None);
    }
    let key = h.sym_term(h.sym("assumptions"));
    let mut facts = None;
    let mut others = vec![];
    for &r in rules {
        match h.view(r) {
            View::App { args: &[k, v], .. } if k == key => facts = Some(v),
            _ => others.push(r),
        }
    }
    let Some(a) = facts else { return Ok(None) };
    if a == h.sym_term(h.sym(names::FALSE)) {
        return Ok(None);
    }
    let mut new_args = rest.to_vec();
    // reduce takes the facts as conditions: they restrict its answer
    if name == "reduce" && !new_args.is_empty() && a != h.sym_term(h.sym(names::TRUE)) {
        new_args[0] = h.call(h.sym("and"), &[new_args[0], a]);
        if !others.is_empty() {
            new_args.push(h.call(h.sym("options"), &others));
        }
        return Ok(Some(cx.eval(h.call(h.sym(name), &new_args))?));
    }
    if !others.is_empty() {
        new_args.push(h.call(h.sym("options"), &others));
    }
    let call = h.call(h.sym(name), &new_args);
    if a == h.sym_term(h.sym(names::TRUE)) {
        return Ok(Some(cx.eval(call)?));
    }
    Ok(Some(cx.eval_assuming(&[a], call)?))
}

/// A native with the `assumptions:` option handled before it is called ([`assumptions_option`]).
pub struct WithAssumptions<N> {
    /// the function's name
    pub name: &'static str,
    /// the native
    pub inner: N,
}

impl<N: Native> Native for WithAssumptions<N> {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        if let Some(r) = assumptions_option(cx, self.name, args)? {
            return Ok(Reply::Done(r));
        }
        self.inner.call(cx, t, args)
    }

    fn trace(&mut self, visit: &mut dyn FnMut(&mut Raw)) {
        self.inner.trace(visit)
    }

    fn reads_term(&self) -> bool {
        self.inner.reads_term()
    }

    fn curried(&self) -> bool {
        self.inner.curried()
    }
}
