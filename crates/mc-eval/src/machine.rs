//! The evaluator's machine: evaluation (semantics 4.1) on an explicit, heap-allocated stack of frames, so the depth
//! of an evaluation is bounded by memory ([`EvalConfig::max_depth`](crate::EvalConfig::max_depth) frames) and not by
//! the Rust stack (decision 1a.1).
//!
//! A [`Frame::Eval`] is one evaluation level: the term it started from (the memo key), the term being reduced (the
//! loop of 4.1, in which a replacement is evaluated again), the read set, effect, fuel and cell counters saved at its
//! start, and the [`Stage`] it waits in: for the value of its head, of its next strict argument (the values so far
//! are on the task's value stack, [`Cx::vals`]), of a symbol's own value that is its result, or for a language form
//! above it. The language forms whose bodies may recurse (`block`, `while`, `for`; see [`crate::lang`]) are frames
//! of their own, so recursion through them adds no Rust frames either.
//!
//! The driver ([`Eval::eval`]) moves values up and aborts down the stack until the frame it started is done; each
//! step function returns what the driver does next ([`Next`]). Natives, conditions and the speculative split of a
//! group call back into [`Eval::eval`], which nests a driver on the same stack: that nesting is the only Rust
//! recursion left, bounded by [`EvalConfig::max_nest`](crate::EvalConfig::max_nest) (decision 1a.2); a group is
//! not split once the nesting reaches [`SPLIT_NEST`], so speculation never makes an evaluation deeper in Rust than a
//! sequential one would be by more than that.
use crate::defs::{Attributes, Hold, SymDefs};
use crate::eval::{bit, sym_of, Abort, Cx, Eval, Impl, Res, Step};
use crate::lang::{self, BlockState, ForState, WhileState};
use crate::native::{NativeCx, Reply};
use crate::ctl::Status;
use mc_core::{Store, View, T};
use std::sync::atomic::Ordering::*;

/// Native-callback nesting beyond which groups are no longer split (the split path evaluates its elements through
/// nested drivers): deep recursions split near their root and run sequentially below.
pub(crate) const SPLIT_NEST: u32 = 48;

/// A frame of the evaluation stack.
pub(crate) enum Frame<'h> {
    /// an evaluation level
    Eval(EvalFrame<'h>),
    /// `block(...)`: the statements so far, the bindings, the cells
    Block(Box<BlockState<'h>>),
    /// `while(c, body)`
    While(WhileState<'h>),
    /// `for(x, xs, body)`
    For(ForState<'h>),
}

/// One evaluation level (the body of the evaluation loop of 4.1 as data).
pub(crate) struct EvalFrame<'h> {
    /// the term the evaluation started from: the memo key, the head whose cost is learned
    t0: T<'h>,
    /// the term being reduced
    t: T<'h>,
    reads0: u64,
    fuel0: u64,
    effects0: u32,
    epoch0: u32,
    volatile0: u32,
    /// whether the result is memoised (the term refers to no cell)
    memo: bool,
    stage: Stage,
}

/// What a suspended evaluation level waits for.
#[derive(Clone, Copy)]
enum Stage {
    /// the value of the head of `t` (4.1 step 3.1)
    Head,
    /// the value of argument `next` of `t`; the earlier values are on the value stack from `base` (step 3.2)
    Args { next: u32, base: u32, attrs: Attributes, closure: bool },
    /// the value of a symbol's own value, which is this level's result
    Value,
    /// a language form frame above this one
    Form,
}

/// What the driver does next.
pub(crate) enum Next<'h> {
    /// evaluate a term and deliver its value to the frame on top
    Eval(T<'h>),
    /// deliver a value to the frame on top (or return it, at the driver's base)
    Ret(T<'h>),
    /// unwind an abort through the frame on top (or return it, at the driver's base)
    Err(Abort),
}

/// The outcome of a step of an evaluation level.
enum Go<'h> {
    /// suspended or finished: the driver goes on
    Next(Next<'h>),
    /// a replacement: the level reduces its (new) term again
    Again(EvalFrame<'h>),
}

/// What a language form does next.
pub(crate) enum FormOut<'h> {
    /// push this frame and evaluate the term; the value resumes the frame
    Eval(Frame<'h>, T<'h>),
    /// the form is done
    Reply(Reply<'h>),
}

/// Where the values of a call's arguments are.
enum Vals<'h> {
    /// unchanged: the call's own arguments
    Same,
    /// on the value stack, from this index
    Stack(usize),
    /// in a vector (the split path)
    Owned(Vec<T<'h>>),
}

/// Argument values kept on fixed buffers (larger calls use a vector).
const ARGS: usize = 16;

/// Try an expression returning `Res`; on an abort, the level `$e` fails with it.
macro_rules! attempt {
    ($self:ident, $cx:ident, $e:ident, $x:expr) => {
        match $x {
            Ok(v) => v,
            Err(a) => return Go::Next($self.fail($cx, $e, a)),
        }
    };
}

impl<'k, 'h> Eval<'k, 'h> {
    /// Evaluate `t` to normal form on this task: a driver over the task's stack, nested in the caller's.
    pub(crate) fn eval(&self, cx: &mut Cx<'h>, t: T<'h>) -> Res<T<'h>> {
        let h = self.heap;
        match h.view(t) {
            View::App { .. } => {}
            View::Sym(s) => {
                cx.reads |= bit(s);
                match self.k.defs.own(h, s) {
                    Some(v) if v != t => {}
                    _ => return Ok(t),
                }
            }
            _ => return Ok(t),
        }
        if cx.nest >= self.k.cfg.max_nest {
            return Err(self.too_deep(cx));
        }
        cx.nest += 1;
        let r = self.run(cx, t);
        cx.nest -= 1;
        r
    }

    /// The depth limit: `failure(limit, depth)` on the frontier, a hole on a speculative task.
    fn too_deep(&self, cx: &Cx<'h>) -> Abort {
        if cx.front {
            self.k.ctl.fail(Status::Depth);
            Abort::Limit
        } else {
            Abort::Hole
        }
    }

    fn run(&self, cx: &mut Cx<'h>, t: T<'h>) -> Res<T<'h>> {
        let base = cx.stack.len();
        let mut next = Next::Eval(t);
        loop {
            next = match next {
                Next::Eval(t) => self.enter(cx, t),
                Next::Ret(v) if cx.stack.len() == base => return Ok(v),
                Next::Ret(v) => self.resume(cx, v),
                Next::Err(a) if cx.stack.len() == base => return Err(a),
                Next::Err(a) => self.unwind(cx, a),
            };
        }
    }

    /// Start evaluating `t`: atoms and symbols at once, compound terms from the memo or as a new level.
    fn enter(&self, cx: &mut Cx<'h>, t: T<'h>) -> Next<'h> {
        let h = self.heap;
        match h.view(t) {
            View::App { .. } => {}
            View::Sym(s) => {
                cx.reads |= bit(s);
                return match self.k.defs.own(h, s) {
                    Some(v) if v != t => match self.tick(cx) {
                        Ok(()) => Next::Eval(v),
                        Err(a) => Next::Err(a),
                    },
                    _ => Next::Ret(t),
                };
            }
            _ => return Next::Ret(t),
        }
        if let Err(a) = self.tick(cx) {
            return Next::Err(a);
        }
        // a term that refers to a cell is volatile (its value depends on the cell): not looked up
        let memo = self.k.cfg.memo && h.mask(t) & bit(self.k.names.local_cell) == 0;
        if memo {
            if let Some(hit) = h.memo_get(t) {
                // the epoch fast path, then the read-set check (semantics 4.3); a hit passes its read set on (2.7)
                if hit.epoch == cx.epoch || (hit.epoch < cx.epoch && !self.k.changed(hit.epoch, cx.epoch, hit.deps)) {
                    cx.reads |= hit.deps;
                    return Next::Ret(hit.value);
                }
            }
        }
        if cx.depth >= self.k.cfg.max_depth {
            return Next::Err(self.too_deep(cx));
        }
        let e = EvalFrame {
            t0: t,
            t,
            reads0: cx.reads,
            fuel0: cx.fuel,
            effects0: cx.effects,
            epoch0: cx.epoch,
            volatile0: cx.volatile,
            memo,
            stage: Stage::Head,
        };
        cx.reads = 0;
        cx.depth += 1;
        self.reduce(cx, e)
    }

    /// Reduce the level's term until it suspends or finishes.
    fn reduce(&self, cx: &mut Cx<'h>, mut e: EvalFrame<'h>) -> Next<'h> {
        loop {
            match self.start(cx, e) {
                Go::Next(n) => return n,
                Go::Again(e2) => e = e2,
            }
        }
    }

    #[inline]
    fn drive(&self, cx: &mut Cx<'h>, go: Go<'h>) -> Next<'h> {
        match go {
            Go::Next(n) => n,
            Go::Again(e) => self.reduce(cx, e),
        }
    }

    /// Suspend level `e` in `stage` and evaluate `t` above it.
    #[inline]
    fn suspend(&self, cx: &mut Cx<'h>, mut e: EvalFrame<'h>, stage: Stage, t: T<'h>) -> Go<'h> {
        e.stage = stage;
        cx.stack.push(Frame::Eval(e));
        Go::Next(Next::Eval(t))
    }

    /// One pass of the loop of 4.1 from the top: the head (step 3.1).
    fn start(&self, cx: &mut Cx<'h>, e: EvalFrame<'h>) -> Go<'h> {
        let h = self.heap;
        let head = match h.view(e.t) {
            View::App { head, .. } => head,
            View::Sym(s) => {
                cx.reads |= bit(s);
                return match self.k.defs.own(h, s) {
                    Some(v) if v != e.t => {
                        attempt!(self, cx, e, self.tick(cx));
                        self.suspend(cx, e, Stage::Value, v)
                    }
                    _ => {
                        let v = e.t;
                        Go::Next(self.finish(cx, e, v))
                    }
                };
            }
            _ => {
                let v = e.t;
                return Go::Next(self.finish(cx, e, v));
            }
        };
        match sym_of(h, head) {
            Some(s) => {
                cx.reads |= bit(s);
                match self.k.defs.get(s).and_then(|d| d.own(h)).filter(|&v| v != head) {
                    Some(v) => self.suspend(cx, e, Stage::Head, v),
                    None => self.with_head(cx, e, head),
                }
            }
            None => self.suspend(cx, e, Stage::Head, head),
        }
    }

    /// The head's value is `hv`: a different head gives a new term; a closure is applied; otherwise the arguments
    /// are evaluated by the head's argument modes (step 3.2).
    fn with_head(&self, cx: &mut Cx<'h>, mut e: EvalFrame<'h>, hv: T<'h>) -> Go<'h> {
        let h = self.heap;
        let View::App { head, args } = h.view(e.t) else { unreachable!("only calls wait for their head") };
        if hv != head {
            attempt!(self, cx, e, self.tick(cx));
            e.t = h.app(hv, args);
            return Go::Again(e);
        }
        let hs = sym_of(h, head);
        // a closure applied to arguments (semantics 6.7): its body with the parameters bound, evaluated in this
        // loop (a tail call)
        if hs.is_none() {
            if let Some((params, _)) = self.closure(head) {
                let Some(hold) = self.closure_hold(params) else {
                    let v = self.no_match(head, args);
                    return Go::Next(self.finish(cx, e, v));
                };
                let attrs = Attributes { hold, ..Attributes::default() };
                return self.eval_args(cx, e, args, attrs, true);
            }
        }
        let attrs = hs.and_then(|s| self.k.defs.get(s)).map_or_else(Attributes::default, SymDefs::attrs);
        self.eval_args(cx, e, args, attrs, false)
    }

    /// Evaluate the strict arguments: none to evaluate, a group split by the granularity rule, or one at a time on
    /// the stack.
    fn eval_args(&self, cx: &mut Cx<'h>, e: EvalFrame<'h>, args: &'h [T<'h>], attrs: Attributes, closure: bool) -> Go<'h> {
        let hold = attrs.hold;
        if hold == Hold::All || !(0..args.len()).any(|i| !hold.holds(i) && self.needs_eval(cx, args[i])) {
            return self.after_args(cx, e, Vals::Same, attrs, closure);
        }
        if args.len() >= 2 && !attrs.effect && self.may_split(cx) && self.may_split_args(args, hold) {
            let vals = attempt!(self, cx, e, self.split_args(cx, args, hold));
            return self.after_args(cx, e, Vals::Owned(vals), attrs, closure);
        }
        let base = cx.vals.len();
        self.args_from(cx, e, args, 0, base, attrs, closure)
    }

    /// Evaluate arguments `i..` of the level's call one at a time, their values pushed from `base`.
    #[allow(clippy::too_many_arguments)]
    fn args_from(
        &self,
        cx: &mut Cx<'h>,
        mut e: EvalFrame<'h>,
        args: &'h [T<'h>],
        mut i: usize,
        base: usize,
        attrs: Attributes,
        closure: bool,
    ) -> Go<'h> {
        let h = self.heap;
        let stage = |i: usize| Stage::Args { next: i as u32, base: base as u32, attrs, closure };
        // the values pushed so far are dropped if the level fails
        e.stage = stage(i);
        while i < args.len() {
            let a = args[i];
            if !attrs.hold.holds(i) {
                match h.view(a) {
                    View::App { .. } => return self.suspend(cx, e, stage(i), a),
                    View::Sym(s) => {
                        cx.reads |= bit(s);
                        if let Some(v) = self.k.defs.own(h, s).filter(|&v| v != a) {
                            attempt!(self, cx, e, self.tick(cx));
                            return self.suspend(cx, e, stage(i), v);
                        }
                    }
                    _ => {}
                }
            }
            cx.vals.push(a);
            i += 1;
        }
        self.after_args(cx, e, Vals::Stack(base), attrs, closure)
    }

    /// The arguments are evaluated: failures, closures, listable threading and AC canonicalisation (step 3.3), then
    /// the native, the compiled specialisation and the rules.
    fn after_args(&self, cx: &mut Cx<'h>, mut e: EvalFrame<'h>, vals: Vals<'h>, attrs: Attributes, closure: bool) -> Go<'h> {
        let h = self.heap;
        let View::App { head, args } = h.view(e.t) else { unreachable!("only calls have arguments") };
        // the argument values are consumed here: a later failure of the level has none to drop
        e.stage = Stage::Head;
        if closure {
            let vals: Vec<T<'h>> = match vals {
                Vals::Same => args.to_vec(),
                Vals::Stack(base) => cx.vals.drain(base..).collect(),
                Vals::Owned(v) => v,
            };
            let (params, body) = self.closure(head).expect("a closure head");
            return match attempt!(self, cx, e, self.bind_closure(cx, head, params, body, &vals)) {
                Step::Again(u) => {
                    attempt!(self, cx, e, self.tick(cx));
                    e.t = u;
                    Go::Again(e)
                }
                Step::Done(v) => Go::Next(self.finish(cx, e, v)),
            };
        }
        // the values, off the value stack: on a fixed buffer when few
        let mut small = [head; ARGS];
        let mut big: Vec<T<'h>> = vec![];
        let vs: &[T<'h>] = match vals {
            Vals::Same => args,
            Vals::Stack(base) => {
                let n = cx.vals.len() - base;
                let dst = if n <= ARGS {
                    &mut small[..n]
                } else {
                    big.resize(n, head);
                    &mut big[..]
                };
                dst.copy_from_slice(&cx.vals[base..]);
                cx.vals.truncate(base);
                dst
            }
            Vals::Owned(v) => {
                big = v;
                &big
            }
        };
        if !attrs.handles_failures {
            if let Some(f) = self.first_failure(vs) {
                return Go::Next(self.finish(cx, e, f));
            }
        }
        // listable functions map over list arguments (semantics 7.2)
        if attrs.listable {
            if let Some(u) = self.thread(head, vs) {
                attempt!(self, cx, e, self.tick(cx));
                e.t = u;
                return Go::Again(e);
            }
        }
        // canonicalise: flattened and sorted; with an identity element the result may be no call of the head
        let mut canon = [head; ARGS];
        let mut canon_big: Vec<T<'h>> = vec![];
        let cv: &[T<'h>] = if attrs.ac {
            match self.canonical(head, vs, &mut canon, &mut canon_big) {
                Ok(cv) => cv,
                Err(u) => {
                    attempt!(self, cx, e, self.tick(cx));
                    e.t = u;
                    return Go::Again(e);
                }
            }
        } else {
            vs
        };
        // the call with the evaluated arguments (an AC call is rebuilt: the store shares it if it was canonical)
        let changed = attrs.ac || !std::ptr::eq(vs, args) && vs != args;
        self.call(cx, e, head, cv, changed)
    }

    /// The call of `head` on `vals`, the evaluated arguments of `e.t` (`changed`: the call is not `e.t` itself): its
    /// native implementation (step 3.4) or language form, then its compiled specialisation and its rules. A native
    /// that does not read its term is called before the call is built (decision 1a.6).
    fn call(&self, cx: &mut Cx<'h>, mut e: EvalFrame<'h>, head: T<'h>, vals: &[T<'h>], changed: bool) -> Go<'h> {
        let h = self.heap;
        let native = sym_of(h, head)
            .and_then(|s| self.k.defs.get(s))
            .map(|sd| sd.native.load(Acquire))
            .filter(|&n| n != 0)
            .map(|n| &self.k.natives[n as usize - 1]);
        match native {
            Some(Impl::Form(form)) => {
                if changed {
                    e.t = h.app(head, vals);
                }
                let View::App { args, .. } = h.view(e.t) else { unreachable!("a call") };
                e.stage = Stage::Form;
                cx.stack.push(Frame::Eval(e));
                let r = lang::start(&mut NativeCx { ev: self, cx }, *form, args);
                Go::Next(self.form_out(cx, r))
            }
            Some(Impl::Native { native, reads_term }) => {
                let early = changed && !reads_term;
                if changed && *reads_term {
                    e.t = h.app(head, vals);
                }
                let t = e.t;
                let r = native.call(&mut NativeCx { ev: self, cx }, t, vals);
                if early && matches!(r, Ok(Reply::Decline)) {
                    e.t = h.app(head, vals);
                }
                self.native_result(cx, e, r)
            }
            None => {
                if changed {
                    e.t = h.app(head, vals);
                }
                self.rules(cx, e)
            }
        }
    }

    /// The canonical arguments of AC head `head` applied to `vals`: calls of the head spliced in (one level: they
    /// are canonical already), sorted by the canonical order, identity elements dropped (semantics 7.3); on `small`
    /// when they fit, else on `big`. `Err` with the replacement when an identity element leaves fewer than two.
    fn canonical<'b>(
        &self,
        head: T<'h>,
        vals: &[T<'h>],
        small: &'b mut [T<'h>; ARGS],
        big: &'b mut Vec<T<'h>>,
    ) -> Result<&'b [T<'h>], T<'h>> {
        let h = self.heap;
        let inner = |a: T<'h>| match h.view(a) {
            View::App { head: g, args } if g == head => Some(args),
            _ => None,
        };
        let len: usize = vals.iter().map(|&a| inner(a).map_or(1, <[T]>::len)).sum();
        let v: &mut [T<'h>] = if len <= ARGS {
            &mut small[..len]
        } else {
            big.resize(len, head);
            &mut big[..]
        };
        let mut n = 0;
        for &a in vals {
            match inner(a) {
                Some(xs) => {
                    v[n..n + xs.len()].copy_from_slice(xs);
                    n += xs.len();
                }
                None => {
                    v[n] = a;
                    n += 1;
                }
            }
        }
        self.k.sort(h, v);
        let Some(id) = sym_of(h, head).and_then(|s| self.k.defs.get(s)).and_then(|d| d.identity(h)) else { return Ok(v) };
        let mut kept = 0;
        for i in 0..v.len() {
            if v[i] != id {
                v[kept] = v[i];
                kept += 1;
            }
        }
        match v[..kept] {
            [] => Err(id),
            [a] => Err(a),
            _ => Ok(&v[..kept]),
        }
    }

    /// What a native (or a language form) replied.
    fn native_result(&self, cx: &mut Cx<'h>, mut e: EvalFrame<'h>, r: Res<Reply<'h>>) -> Go<'h> {
        match r {
            Ok(Reply::Done(v)) => Go::Next(self.finish(cx, e, v)),
            Ok(Reply::Again(u)) => {
                attempt!(self, cx, e, self.tick(cx));
                e.t = u;
                Go::Again(e)
            }
            Ok(Reply::Decline) => self.rules(cx, e),
            Err(Abort::EffectInCondition) if cx.in_cond == 0 => {
                let v = self.effect_failure();
                Go::Next(self.finish(cx, e, v))
            }
            Err(a) => Go::Next(self.fail(cx, e, a)),
        }
    }

    /// The compiled specialisation of the head, then the rules (step 3.5).
    fn rules(&self, cx: &mut Cx<'h>, mut e: EvalFrame<'h>) -> Go<'h> {
        if let Some(v) = self.compiled(cx, e.t) {
            return Go::Next(self.finish(cx, e, v));
        }
        match self.dispatch(cx, e.t) {
            Ok(Some(u)) => {
                attempt!(self, cx, e, self.tick(cx));
                e.t = u;
                Go::Again(e)
            }
            Ok(None) => {
                let v = e.t;
                Go::Next(self.finish(cx, e, v))
            }
            Err(Abort::EffectInCondition) if cx.in_cond == 0 => {
                let v = self.effect_failure();
                Go::Next(self.finish(cx, e, v))
            }
            Err(a) => Go::Next(self.fail(cx, e, a)),
        }
    }

    /// A language form's step: push its frame and evaluate, or hand its reply to the level below it.
    fn form_out(&self, cx: &mut Cx<'h>, r: Res<FormOut<'h>>) -> Next<'h> {
        let r = match r {
            Ok(FormOut::Eval(frame, t)) => {
                cx.stack.push(frame);
                return Next::Eval(t);
            }
            Ok(FormOut::Reply(reply)) => Ok(reply),
            Err(a) => Err(a),
        };
        let Some(Frame::Eval(e)) = cx.stack.pop() else { unreachable!("a form runs above its evaluation level") };
        let go = self.native_result(cx, e, r);
        self.drive(cx, go)
    }

    /// Deliver value `v` to the frame on top.
    fn resume(&self, cx: &mut Cx<'h>, v: T<'h>) -> Next<'h> {
        let frame = cx.stack.pop().expect("a frame above the driver's base");
        let r = match frame {
            Frame::Eval(e) => {
                let go = match e.stage {
                    Stage::Head => self.with_head(cx, e, v),
                    Stage::Args { next, base, attrs, closure } => {
                        cx.vals.push(v);
                        let View::App { args, .. } = self.heap.view(e.t) else { unreachable!("a call") };
                        self.args_from(cx, e, args, next as usize + 1, base as usize, attrs, closure)
                    }
                    Stage::Value => return self.finish(cx, e, v),
                    Stage::Form => unreachable!("a form level resumes through its form"),
                };
                return self.drive(cx, go);
            }
            frame => lang::resume(&mut NativeCx { ev: self, cx }, frame, v),
        };
        self.form_out(cx, r)
    }

    /// Unwind abort `a` through the frame on top.
    fn unwind(&self, cx: &mut Cx<'h>, a: Abort) -> Next<'h> {
        match cx.stack.pop().expect("a frame above the driver's base") {
            Frame::Eval(e) => self.fail(cx, e, a),
            frame => {
                lang::abort(&mut NativeCx { ev: self, cx }, frame);
                self.form_out(cx, Err(a))
            }
        }
    }

    /// Level `e` has normal form `v`: its read set (with the normal-form stamp), the learned cost of its head, the
    /// memo entry (not for results of effects or cells, semantics 9 and 6.7; stamped with the start epoch).
    fn finish(&self, cx: &mut Cx<'h>, e: EvalFrame<'h>, v: T<'h>) -> Next<'h> {
        let h = self.heap;
        cx.depth -= 1;
        // a normal form stays normal while none of its symbols (or the term's) gains a definition
        let deps = cx.reads | h.mask(e.t0) | h.mask(v);
        cx.reads = e.reads0 | deps;
        if let View::App { head, .. } = h.view(e.t0) {
            if let Some(s) = sym_of(h, head) {
                self.k.learn(s, cx.fuel - e.fuel0);
            }
        }
        if e.memo && cx.effects == e.effects0 && cx.volatile == e.volatile0 {
            h.memo_put(e.t0, v, e.epoch0, deps);
            if v != e.t0 {
                h.memo_put(v, v, e.epoch0, deps);
            }
        }
        Next::Ret(v)
    }

    /// Level `e` stops with abort `a`: its reads stay recorded, the argument values it pushed are dropped.
    fn fail(&self, cx: &mut Cx<'h>, e: EvalFrame<'h>, a: Abort) -> Next<'h> {
        cx.depth -= 1;
        cx.reads |= e.reads0;
        if let Stage::Args { base, .. } = e.stage {
            cx.vals.truncate(base as usize);
        }
        Next::Err(a)
    }
}
