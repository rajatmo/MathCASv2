//! Assumption contexts (semantics 13): `assuming(facts, body)` evaluates `body` with the context extended by
//! `facts`.
//!
//! - **The context** is a set of facts, held as one term: `list(f1, ..., fn)`, flattened (`and` and lists of facts
//!   are split), sorted by the canonical order and without duplicates, so equal sets are the same handle. It lives
//!   in the evaluation task ([`Cx::assume`]): lexically scoped to the evaluation of the body, restored when it
//!   returns (also on an abort), and copied into every speculative task split off it.
//! - **Who reads it**: only natives of heads that declare [`Attributes::uses_assumptions`], through
//!   [`NativeCx::assumptions`]. The canonicaliser and ordinary rules never see it (13.4); a rule that needs a fact
//!   calls a decision function, which declares the flag itself.
//! - **Dependencies** (4.3): calling a head with the flag, or reading the context, puts the read-set bit of the
//!   symbol `assuming` into the evaluation's read set. That bit travels with read sets as every other read does:
//!   up through enclosing evaluations, through memo hits, and back from speculative tasks.
//! - **The memo**: an evaluation inside a nonempty context whose read set has the bit is stored under the key
//!   `assume$memo(t, context)` instead of `t`; every other evaluation is stored under `t`. Inside a nonempty context a
//!   lookup accepts an entry under `t` only if its read set lacks the bit (the result did not depend on any context)
//!   and otherwise looks under the context's key. Outside every context, entries under `t` with the bit were made
//!   with the empty context too, so they are valid. A result is therefore never reused under another context; a
//!   collision of the bit with another symbol's only sends more entries to the keyed form (conservative).
use crate::defs::{Attributes, Hold};
use crate::eval::{bit, Cx, Eval, Kernel, Res};
use crate::native::{Native, NativeCx, Reply};
use mc_core::{Heap, Store, Sym, View, T};

/// The symbol of the held form.
pub const ASSUMING: &str = "assuming";
/// The head of the memo key of an evaluation that depends on its context (not writable as an identifier).
const MEMO_KEY: &str = "assume$memo";

/// The symbols of the assumption mechanism, set by [`install`].
#[derive(Clone, Copy)]
pub(crate) struct Syms {
    /// `assuming`: its read-set bit marks a dependency on the context
    assuming: Sym,
    /// the head of the keyed memo form
    key: Sym,
    /// `and`, split when facts are added
    and: Sym,
}

/// Register the held form `assuming(facts, body)` with `k` (the facts evaluated, the body held). Until this is
/// called no context can exist, so heads declaring [`Attributes::uses_assumptions`] add no dependency.
pub fn install(k: &mut Kernel, h: &Heap) {
    let syms = Syms { assuming: h.sym(ASSUMING), key: h.sym(MEMO_KEY), and: h.sym("and") };
    k.assume = Some(syms);
    let a = Attributes { hold: Hold::Positions(0b10), uses_assumptions: true, ..Attributes::default() };
    k.register(syms.assuming, a, Box::new(Assuming));
}

/// `assuming(facts, body)`: `body` evaluated with the context extended by `facts` (a fact, an `and` of facts or a
/// list of facts).
struct Assuming;

impl Native for Assuming {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let [facts, body] = args else { return Ok(Reply::Decline) };
        Ok(Reply::Done(cx.eval_assuming(&[*facts], *body)?))
    }

    fn reads_term(&self) -> bool {
        false
    }
}

impl<'h> Eval<'_, 'h> {
    /// The read-set bit that marks a dependency on the assumption context (0 before [`install`]).
    #[inline]
    pub(crate) fn assume_bit(&self) -> u64 {
        self.k.assume.map_or(0, |s| bit(s.assuming))
    }

    /// May a memo entry stored under the plain term, with read set `deps`, be used in `cx`'s context?
    #[inline]
    pub(crate) fn plain_memo_ok(&self, cx: &Cx<'h>, deps: u64) -> bool {
        cx.assume.is_none() || deps & self.assume_bit() == 0
    }

    /// The memo key of `t` in `cx`'s nonempty context, if there is one.
    #[inline]
    pub(crate) fn context_key(&self, cx: &Cx<'h>, t: T<'h>) -> Option<T<'h>> {
        let ctx = cx.assume?;
        let s = self.k.assume?;
        Some(self.heap.app(self.sym(s.key), &[t, ctx]))
    }

    /// The memo key under which the result of evaluating `t`, with read set `deps`, is stored in `cx`'s context.
    #[inline]
    pub(crate) fn memo_key(&self, cx: &Cx<'h>, t: T<'h>, deps: u64) -> T<'h> {
        if deps & self.assume_bit() != 0 {
            if let Some(k) = self.context_key(cx, t) {
                return k;
            }
        }
        t
    }

    /// The context of `cx` extended by `facts`: flattened, sorted, without duplicates; `None` when empty.
    fn extended(&self, cx: &mut Cx<'h>, facts: &[T<'h>]) -> Res<Option<T<'h>>> {
        let h = self.heap;
        let list = self.sym(self.k.names.list);
        let and = self.k.assume.map(|s| self.sym(s.and));
        let true_ = self.sym(self.k.names.true_);
        let mut out: Vec<T<'h>> = match cx.assume {
            Some(c) => match h.view(c) {
                View::App { args, .. } => args.to_vec(),
                _ => vec![],
            },
            None => vec![],
        };
        let mut stack: Vec<T<'h>> = facts.iter().rev().copied().collect();
        while let Some(f) = stack.pop() {
            self.tick(cx)?;
            match h.view(f) {
                View::App { head, args } if head == list || Some(head) == and => stack.extend(args.iter().rev()),
                _ if f == true_ => {}
                _ => out.push(f),
            }
        }
        if out.is_empty() {
            return Ok(None);
        }
        self.spend(cx, out.len() as u64)?;
        self.k.sort(h, &mut out);
        out.dedup();
        Ok(Some(h.app(list, &out)))
    }
}

impl<'h> NativeCx<'_, '_, 'h> {
    /// The facts of the assumption context the call runs in (semantics 13), in canonical order, without duplicates;
    /// empty outside every `assuming`. Reading them makes the caller's result depend on the context (it is never
    /// reused under another one). Only natives of heads declaring [`Attributes::uses_assumptions`] should call this.
    pub fn assumptions(&mut self) -> &'h [T<'h>] {
        self.cx.reads |= self.ev.assume_bit();
        match self.cx.assume.map(|c| self.ev.heap.view(c)) {
            Some(View::App { args, .. }) => args,
            _ => &[],
        }
    }

    /// Evaluate `t` with the assumption context extended by `facts` (each a fact, an `and` of facts or a list of
    /// them; `true` adds nothing); the context is restored afterwards. The result depends on the outer context.
    /// Without [`install`] there is no keyed memo for contexts: the call gives `failure(not_installed, assuming)`.
    pub fn eval_assuming(&mut self, facts: &[T<'h>], t: T<'h>) -> Res<T<'h>> {
        if self.ev.k.assume.is_none() {
            return Ok(self.ev.failure(&["not_installed", ASSUMING]));
        }
        self.cx.reads |= self.ev.assume_bit();
        let outer = self.cx.assume;
        self.cx.assume = self.ev.extended(self.cx, facts)?;
        let r = self.ev.eval(self.cx, t);
        self.cx.assume = outer;
        r
    }
}
