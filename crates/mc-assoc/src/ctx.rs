//! What every native of the library shares: the symbols it builds and recognises, the cache of association
//! indexes, and small helpers for arguments, failures, fuel and calls of function-valued arguments.
use crate::index::Indexes;
use crate::table::DataSyms;
use mc_api::names;
use mc_core::{Heap, Store, Sym, View, T};
use mc_eval::{NativeCx, Reply, Res};

/// The symbols the natives use.
pub(crate) struct Syms {
    pub list: Sym,
    pub failure: Sym,
    pub rule: Sym,
    /// the head of an association (decision 1B.1)
    pub assoc: Sym,
    /// `failure(argument, f, k)`: argument `k` of `f` has the wrong kind
    pub argument: Sym,
    /// `failure(missing_key, k)`: `lookup` without a default found no key `k`
    pub missing_key: Sym,
}

impl Syms {
    fn new(h: &Heap) -> Syms {
        Syms {
            list: h.sym(names::LIST),
            failure: h.sym(names::FAILURE),
            rule: h.sym(names::RULE),
            assoc: h.sym("assoc"),
            argument: h.sym("argument"),
            missing_key: h.sym("missing_key"),
        }
    }
}

/// The shared state of the library's natives: symbols and the index cache. It holds no term handles across safe
/// points (the cache is emptied at each one, decision 1B.6), so the natives trace nothing.
pub(crate) struct Lib {
    pub s: Syms,
    /// the symbols of datasets
    pub d: DataSyms,
    pub indexes: Indexes,
}

impl Lib {
    pub fn new(h: &Heap) -> Lib {
        Lib { s: Syms::new(h), d: DataSyms::new(h), indexes: Indexes::default() }
    }

    /// `failure(tag, details...)` with the tag named.
    pub fn fail_named<'h>(&self, h: &'h Heap, tag: &str, details: &[T<'h>]) -> T<'h> {
        self.fail(h, h.sym(tag), details)
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

    /// Is `t` a failure, `failure(...)`?
    pub fn is_failure(&self, h: &Heap, t: T<'_>) -> bool {
        matches!(h.view(t), View::App { head, .. } if head == h.sym_term(self.s.failure))
    }

    /// The key and value of an entry `rule(k, v)`.
    pub fn entry<'h>(&self, h: &'h Heap, e: T<'h>) -> Option<(T<'h>, T<'h>)> {
        match h.view(e) {
            View::App { head, args: &[k, v] } if head == h.sym_term(self.s.rule) => Some((k, v)),
            _ => None,
        }
    }

    /// The key of an entry of an association in normal form (every argument is a rule).
    pub fn key<'h>(&self, h: &'h Heap, e: T<'h>) -> T<'h> {
        match h.view(e) {
            View::App { args: &[k, _], .. } => k,
            _ => e,
        }
    }

    /// Is `x` a rule `k -> v` or `k :> v`?
    pub fn is_rule(&self, h: &Heap, x: T<'_>) -> bool {
        match h.view(x) {
            View::App { head, args: [_, _] } => {
                matches!(h.view(head), View::Sym(s) if matches!(h.sym_name(s).as_str(), "rule" | "rule_delayed"))
            }
            _ => false,
        }
    }

    /// The value of an entry of an association in normal form.
    pub fn value<'h>(&self, h: &'h Heap, e: T<'h>) -> T<'h> {
        match h.view(e) {
            View::App { args: &[_, v], .. } => v,
            _ => e,
        }
    }

    /// The entry `rule(k, v)`. Its parts are normal forms, and `rule` has no definitions, so it is one too.
    pub fn rule<'h>(&self, h: &'h Heap, k: T<'h>, v: T<'h>) -> T<'h> {
        h.call(self.s.rule, &[k, v])
    }

    /// The association with these entries (distinct keys, in order).
    pub fn assoc<'h>(&self, h: &'h Heap, entries: &[T<'h>]) -> T<'h> {
        h.call(self.s.assoc, entries)
    }

    /// The entries of `a` if it is an association.
    pub fn as_assoc<'h>(&self, h: &'h Heap, a: T<'h>) -> Option<&'h [T<'h>]> {
        match h.view(a) {
            View::App { head, args } if head == h.sym_term(self.s.assoc) => Some(args),
            _ => None,
        }
    }

    /// Argument `k` (1-based) of the call `t` as an association: a symbol declines (the call stays unevaluated, as
    /// for any symbolic argument), anything else is `failure(argument, f, k)`.
    pub fn assoc_operand<'h>(&self, h: &'h Heap, t: T<'h>, a: T<'h>, k: usize) -> Result<&'h [T<'h>], Reply<'h>> {
        match h.view(a) {
            View::App { head, args } if head == h.sym_term(self.s.assoc) => Ok(args),
            View::Sym(_) => Err(Reply::Decline),
            _ => Err(Reply::Done(self.bad_arg(h, t, k))),
        }
    }

    /// Argument `k` of the call `t` as a compound term (any head): its head and arguments. A symbol declines, any
    /// other atom is a failure.
    pub fn operand<'h>(&self, h: &'h Heap, t: T<'h>, a: T<'h>, k: usize) -> Result<(T<'h>, &'h [T<'h>]), Reply<'h>> {
        match h.view(a) {
            View::App { head, args } => Ok((head, args)),
            View::Sym(_) => Err(Reply::Decline),
            _ => Err(Reply::Done(self.bad_arg(h, t, k))),
        }
    }

    /// The keys a function such as `key_take` names: the elements of a list, or a single key otherwise (decision
    /// 1B.9: a list key is named by wrapping it in a list).
    pub fn key_list<'h>(&self, h: &'h Heap, ks: T<'h>) -> Vec<T<'h>> {
        match h.view(ks) {
            View::App { head, args } if head == h.sym_term(self.s.list) => args.to_vec(),
            _ => vec![ks],
        }
    }

    /// The term `head(args)` as a reply: a list is a normal form already (its elements are), any other head is
    /// evaluated again so that its canonicalisation and rules apply.
    pub fn rebuilt<'h>(&self, h: &'h Heap, head: T<'h>, args: &[T<'h>]) -> Reply<'h> {
        let t = h.app(head, args);
        if head == h.sym_term(self.s.list) {
            Reply::Done(t)
        } else {
            Reply::Again(t)
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

/// Elements per fuel unit of the library's per-element work (hashing, copying, comparing), as in `mc-lists`. Calls
/// of function-valued arguments are charged by the evaluator instead.
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

/// Sort `xs` in the kernel's canonical order, charging about `n log n` comparisons before it (the sort itself
/// does not poll; its time is bounded by the input).
pub(crate) fn sort_canonical<'h>(cx: &mut NativeCx<'_, '_, 'h>, xs: &mut [T<'h>]) -> Res<()> {
    let n = xs.len() as u64;
    let log = 64 - n.leading_zeros() as u64;
    cx.tick(n * log / PER_UNIT as u64)?;
    cx.sort(xs);
    Ok(())
}

/// Evaluate the calls `f(x)` for every `x` of `xs` as a speculative group (like `map`): the learned cost of a
/// symbol not declared `effect` is the hint, anything else keeps the group sequential until measured.
pub(crate) fn map_calls<'h>(cx: &mut NativeCx<'_, '_, 'h>, f: T<'h>, xs: &[T<'h>]) -> Res<Vec<T<'h>>> {
    let h = cx.heap();
    let cost = match h.view(f) {
        View::Sym(s) if !cx.is_effect(f) => cx.head_cost(s).max(1),
        _ => 0,
    };
    cx.group(xs.len(), cost, &|cx: &mut NativeCx<'_, '_, 'h>, i| {
        cx.tick(1)?;
        let h = cx.heap();
        cx.eval(h.app(f, &[xs[i]]))
    })
}
