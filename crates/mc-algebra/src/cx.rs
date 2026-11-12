//! The shared context of one native call: the canonicaliser, the evaluator's fuel as a work budget and term
//! helpers.
use mc_api::{names, Store, View, T};
use mc_arith::Arith;
use mc_core::Heap;
use mc_eval::{Abort, NativeCx};
use mc_poly::convert::{Conv, Heads};
use mc_poly::work::{Halt, Work, WorkResult};
use rustc_hash::FxHashMap;
use std::sync::atomic::{AtomicBool, Ordering};

/// The evaluator's fuel and interrupt as a [`Work`] budget, also polling `mc-arith`'s stop flag.
pub(crate) struct Tick<'c, 'a, 'k, 'h> {
    pub cx: &'c mut NativeCx<'a, 'k, 'h>,
    pub stop: &'c AtomicBool,
    pub abort: Option<Abort>,
}

impl Work for Tick<'_, '_, '_, '_> {
    fn charge(&mut self, units: u64) -> WorkResult<()> {
        if self.stop.load(Ordering::Relaxed) {
            // raised with the evaluator's interrupt, which the fuel poll has not seen yet: the stop is the abort (a
            // caller that sees no abort tries its next method with the number kernels returning arbitrary values)
            if self.cx.stopped() {
                self.abort = Some(mc_eval::Abort::Limit);
            }
            return Err(Halt::Stopped);
        }
        match self.cx.tick(units) {
            Ok(()) => Ok(()),
            Err(a) => {
                self.abort = Some(a);
                Err(Halt::Stopped)
            }
        }
    }
}

/// What a function gives back to the native wrapper.
pub(crate) enum Out<'h> {
    /// a normal form
    Done(T<'h>),
    /// a term to evaluate
    Again(T<'h>),
    /// the call stays unevaluated
    Decline,
    /// a numeric sum to evaluate, and whether its imaginary part vanishes when its enclosure contains zero (a sum
    /// of a real function over the roots of a real polynomial)
    Numeric(T<'h>, bool),
}

/// Shared state of one native call.
pub(crate) struct Env<'h> {
    pub ar: Arith<'h, Heap>,
    pub hd: Heads<'h>,
    pub name: &'static str,
}

impl<'h> Env<'h> {
    pub fn new(h: &'h Heap, stop: std::sync::Arc<AtomicBool>, name: &'static str) -> Env<'h> {
        Env { ar: Arith::new(h).with_stop(stop), hd: Heads::new(h), name }
    }
    pub fn h(&self) -> &'h Heap {
        self.ar.store()
    }
    /// The term of the symbol `name`.
    pub fn sym(&self, name: &str) -> T<'h> {
        let h = self.h();
        h.sym_term(h.sym(name))
    }
    /// `name(args...)` (not canonicalised).
    pub fn call(&self, name: &str, args: &[T<'h>]) -> T<'h> {
        let h = self.h();
        h.call(h.sym(name), args)
    }
    pub fn int(&self, v: i64) -> T<'h> {
        self.ar.int(v)
    }
    pub fn list(&self, items: &[T<'h>]) -> T<'h> {
        self.h().app(self.hd.list, items)
    }
    /// `failure(argument, name, message)`.
    pub fn arg_failure(&self, message: &str) -> Out<'h> {
        self.failure("argument", message)
    }
    /// `failure(tag, name, message)`.
    pub fn failure(&self, tag: &str, message: &str) -> Out<'h> {
        let h = self.h();
        Out::Done(self.call(names::FAILURE, &[self.sym(tag), self.sym(self.name), h.string(message)]))
    }
    /// The arguments of `t` when its head is the symbol `name`.
    pub fn args_of(&self, t: T<'h>, name: &str) -> Option<&'h [T<'h>]> {
        match self.h().view(t) {
            View::App { head, args } if head == self.sym(name) => Some(args),
            _ => None,
        }
    }
    /// The elements of a list term.
    pub fn as_list(&self, t: T<'h>) -> Option<&'h [T<'h>]> {
        match self.h().view(t) {
            View::App { head, args } if head == self.hd.list => Some(args),
            _ => None,
        }
    }
    /// A rational-function converter.
    pub fn rconv(&self) -> Conv<'_, 'h, Heap> {
        Conv::rational(&self.ar, &self.hd)
    }
    /// A polynomial converter (negative powers are kernels).
    pub fn conv(&self) -> Conv<'_, 'h, Heap> {
        Conv::new(&self.ar, &self.hd)
    }
    /// The function `x => body` as `(x, body)`.
    pub fn function_parts(&self, f: T<'h>) -> Option<(T<'h>, T<'h>)> {
        let &[params, body] = self.args_of(f, "fn")? else { return None };
        match self.as_list(params) {
            Some(&[x]) => Some((x, body)),
            _ => None,
        }
    }
    /// The function `x => body`.
    pub fn function(&self, x: T<'h>, body: T<'h>) -> T<'h> {
        self.call("fn", &[self.list(&[x]), body])
    }
    /// Does `t` contain `x`?
    pub fn contains(&self, t: T<'h>, x: T<'h>) -> bool {
        let h = self.h();
        let mut seen: FxHashMap<T<'h>, ()> = FxHashMap::default();
        let mut stack = vec![t];
        while let Some(u) = stack.pop() {
            if u == x {
                return true;
            }
            if seen.insert(u, ()).is_some() {
                continue;
            }
            if let View::App { head, args } = h.view(u) {
                stack.push(head);
                stack.extend_from_slice(args);
            }
        }
        false
    }
    /// `t` with `x` replaced by `v`, rebuilt canonically (other calls are left for the evaluator).
    pub fn subst(&self, t: T<'h>, x: T<'h>, v: T<'h>, m: &mut mc_poly::work::Meter) -> WorkResult<T<'h>> {
        let h = self.h();
        let mut done: FxHashMap<T<'h>, T<'h>> = FxHashMap::default();
        done.insert(x, v);
        let mut stack = vec![(t, false)];
        while let Some((u, ready)) = stack.pop() {
            m.step(1)?;
            if done.contains_key(&u) {
                continue;
            }
            let View::App { head, args } = h.view(u) else {
                done.insert(u, u);
                continue;
            };
            if !ready {
                stack.push((u, true));
                stack.push((head, false));
                stack.extend(args.iter().map(|&a| (a, false)));
                continue;
            }
            let nh = done[&head];
            let na: Vec<T<'h>> = args.iter().map(|a| done[a]).collect();
            let r = if nh == head && na.as_slice() == args { u } else { self.rebuild(nh, &na) };
            done.insert(u, r);
        }
        Ok(done[&t])
    }
    /// `head(args)` canonicalised when the head is add, mul or pow.
    pub fn rebuild(&self, head: T<'h>, args: &[T<'h>]) -> T<'h> {
        if head == self.hd.add {
            self.ar.add(args)
        } else if head == self.hd.mul {
            self.ar.mul(args)
        } else if head == self.hd.pow && args.len() == 2 {
            self.ar.pow(args[0], args[1])
        } else {
            self.h().app(head, args)
        }
    }
}
