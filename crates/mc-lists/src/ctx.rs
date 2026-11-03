//! What every native of the library shares: the symbols it builds and recognises, the stop flag `mc-arith` polls,
//! and small helpers for arguments, failures, fuel and calls of function-valued arguments.
use mc_api::names;
use mc_arith::Arith;
use mc_core::{Heap, Store, Sym, View, T};
use mc_eval::{NativeCx, Reply, Res};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// The symbols the natives use.
pub(crate) struct Syms {
    pub list: Sym,
    pub failure: Sym,
    /// `failure(index, i, n)`: index `i` is outside a term with `n` arguments
    pub index: Sym,
    /// `failure(argument, f, k)`: argument `k` of `f` has the wrong kind
    pub argument: Sym,
    /// `failure(shape_mismatch, f)`: the lists of a rectangular operation have unequal lengths
    pub shape_mismatch: Sym,
    pub limit: Sym,
    pub memory: Sym,
    pub span: Sym,
    pub rule: Sym,
    pub rule_delayed: Sym,
    /// the pattern heads of `mc-syntax` (matching, `crate::matching`)
    pub any: Sym,
    pub seq0: Sym,
    pub seq1: Sym,
    pub typed: Sym,
    pub named: Sym,
    pub condition: Sym,
    pub alternatives: Sym,
}

impl Syms {
    fn new(h: &Heap) -> Syms {
        Syms {
            list: h.sym(names::LIST),
            failure: h.sym(names::FAILURE),
            index: h.sym("index"),
            argument: h.sym("argument"),
            shape_mismatch: h.sym("shape_mismatch"),
            limit: h.sym(names::failure::LIMIT),
            memory: h.sym(names::failure::MEMORY),
            span: h.sym("span"),
            rule: h.sym(names::RULE),
            rule_delayed: h.sym(names::RULE_DELAYED),
            any: h.sym("any"),
            seq0: h.sym("seq0"),
            seq1: h.sym("seq1"),
            typed: h.sym("typed"),
            named: h.sym("named"),
            condition: h.sym("condition"),
            alternatives: h.sym("alternatives"),
        }
    }
}

/// The shared state of the library's natives. It holds only symbols and a flag, so natives need no tracing.
pub(crate) struct Lib {
    pub s: Syms,
    /// the flag `mc-arith`'s long loops poll (the engine raises it with the evaluator's interrupt)
    pub stop: Arc<AtomicBool>,
}

/// The most elements a native builds in one list: a larger request is `failure(limit, memory, call)` rather than an
/// allocation that brings the process down (decision I.6).
pub(crate) const MAX_LEN: usize = 1 << 28;

impl Lib {
    pub fn new(h: &Heap, stop: &Arc<AtomicBool>) -> Lib {
        Lib { s: Syms::new(h), stop: stop.clone() }
    }

    /// `mc-arith`'s canonicaliser with the stop flag.
    pub fn arith<'h>(&self, h: &'h Heap) -> Arith<'h, Heap> {
        Arith::new(h).with_stop(self.stop.clone())
    }

    /// `failure(tag, details...)`
    pub fn fail<'h>(&self, h: &'h Heap, tag: Sym, details: &[T<'h>]) -> T<'h> {
        let mut a = Vec::with_capacity(details.len() + 1);
        a.push(h.sym_term(tag));
        a.extend_from_slice(details);
        h.call(self.s.failure, &a)
    }

    /// `failure(argument, f, k)` for argument `k` (1-based) of the call `t` with head `f`.
    pub fn bad_arg<'h>(&self, h: &'h Heap, t: T<'h>, k: usize) -> T<'h> {
        let f = match h.view(t) {
            View::App { head, .. } => head,
            _ => t,
        };
        self.fail(h, self.s.argument, &[f, h.int(k as i64)])
    }

    /// `failure(index, i, n)`
    pub fn bad_index<'h>(&self, h: &'h Heap, i: T<'h>, n: usize) -> T<'h> {
        self.fail(h, self.s.index, &[i, h.int(n as i64)])
    }

    /// `failure(limit, memory, t)`: a result too large to build.
    pub fn too_large<'h>(&self, h: &'h Heap, t: T<'h>) -> T<'h> {
        self.fail(h, self.s.limit, &[h.sym_term(self.s.memory), t])
    }

    /// Is `t` a failure?
    pub fn is_failure(&self, h: &Heap, t: T<'_>) -> bool {
        matches!(h.view(t), View::App { head, .. } if head == h.sym_term(self.s.failure))
    }

    /// The term `head(args)` as a reply: a list is a normal form already (its elements are), any other head is
    /// evaluated again so that its canonicalisation and rules apply (decision I.3).
    pub fn rebuilt<'h>(&self, h: &'h Heap, head: T<'h>, args: &[T<'h>]) -> Reply<'h> {
        let t = h.app(head, args);
        if head == h.sym_term(self.s.list) {
            Reply::Done(t)
        } else {
            Reply::Again(t)
        }
    }

    /// Argument `k` (1-based) of the call `t` as a compound term, the operand of a structural function. A symbol
    /// declines (the call stays unevaluated, as for any symbolic argument); any other atom is a failure.
    pub fn operand<'h>(&self, h: &'h Heap, t: T<'h>, a: T<'h>, k: usize) -> Result<(T<'h>, &'h [T<'h>]), Reply<'h>> {
        match h.view(a) {
            View::App { head, args } => Ok((head, args)),
            View::Sym(_) => Err(Reply::Decline),
            _ => Err(Reply::Done(self.bad_arg(h, t, k))),
        }
    }

    /// Argument `k` of the call `t` as a list.
    pub fn list_operand<'h>(&self, h: &'h Heap, t: T<'h>, a: T<'h>, k: usize) -> Result<&'h [T<'h>], Reply<'h>> {
        match h.view(a) {
            View::App { head, args } if head == h.sym_term(self.s.list) => Ok(args),
            View::Sym(_) => Err(Reply::Decline),
            _ => Err(Reply::Done(self.bad_arg(h, t, k))),
        }
    }

    /// Argument `k` of the call `t` as a machine integer: a symbol or compound term declines, any other atom (a
    /// non-integer number, a string) is a failure.
    pub fn int_operand<'h>(&self, h: &'h Heap, t: T<'h>, a: T<'h>, k: usize) -> Result<i64, Reply<'h>> {
        match h.view(a) {
            View::Int(v) => Ok(v),
            View::Sym(_) | View::App { .. } => Err(Reply::Decline),
            _ => Err(Reply::Done(self.bad_arg(h, t, k))),
        }
    }
}

/// `return Ok(reply)` when an argument check gives a reply instead of a value.
macro_rules! take {
    ($e:expr) => {
        match $e {
            Ok(v) => v,
            Err(r) => return Ok(r),
        }
    };
}
pub(crate) use take;

/// The arguments of a compound term.
pub(crate) fn compound<'h>(h: &'h Heap, t: T<'h>) -> Option<(T<'h>, &'h [T<'h>])> {
    match h.view(t) {
        View::App { head, args } => Some((head, args)),
        _ => None,
    }
}

/// The term of a 128-bit integer.
pub(crate) fn int128<'h>(h: &'h Heap, v: i128) -> T<'h> {
    match i64::try_from(v) {
        Ok(v) => h.int(v),
        Err(_) => {
            let m = v.unsigned_abs();
            h.big_int(v < 0, &[m as u64, (m >> 64) as u64])
        }
    }
}

/// Elements per fuel unit of the library's per-element work: copying, comparing, one arithmetic step (decision
/// I.4). Calls of function-valued arguments are charged by the evaluator instead.
const PER_UNIT: u32 = 16;
/// Elements between two charges.
const CHUNK: u32 = 4096;

/// Charges the fuel of a loop over elements in chunks, polling the interrupt at each charge.
#[derive(Default)]
pub(crate) struct Meter(u32);

impl Meter {
    /// One element done.
    #[inline]
    pub fn step(&mut self, cx: &mut NativeCx<'_, '_, '_>) -> Res<()> {
        self.0 += 1;
        if self.0 == CHUNK {
            self.0 = 0;
            cx.tick((CHUNK / PER_UNIT) as u64)?;
        }
        Ok(())
    }
}

/// The fuel of a copy of `n` elements, charged before it (the copy itself is too fast to need polling).
pub(crate) fn charge_copy(cx: &mut NativeCx<'_, '_, '_>, n: usize) -> Res<()> {
    cx.tick(n as u64 / PER_UNIT as u64)
}

/// Evaluate the call `f(args...)` of a function-valued argument.
pub(crate) fn apply<'h>(cx: &mut NativeCx<'_, '_, 'h>, f: T<'h>, args: &[T<'h>]) -> Res<T<'h>> {
    let h = cx.heap();
    cx.eval(h.app(f, args))
}

/// The cost hint of a group of calls of `f` (as `map`'s): the learned cost of a symbol not declared `effect`, else
/// 0 (unknown: the group stays sequential until measured).
pub(crate) fn group_cost<'h>(cx: &NativeCx<'_, '_, 'h>, f: T<'h>) -> u64 {
    match cx.heap().view(f) {
        View::Sym(s) if !cx.is_effect(f) => cx.head_cost(s).max(1),
        _ => 0,
    }
}
