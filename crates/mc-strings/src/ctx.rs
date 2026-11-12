//! What every string native shares: the symbols it builds and recognises, argument checks, failures and fuel.
use mc_api::names;
use mc_core::{Heap, Store, Sym, View, T};
use mc_eval::{NativeCx, Reply, Res};

/// The symbols the natives use.
pub(crate) struct Syms {
    pub list: Sym,
    pub failure: Sym,
    /// `failure(argument, f, k)`: argument `k` of `f` has the wrong kind
    pub argument: Sym,
    /// `failure(index, i, n)`: index `i` is outside a string of `n` characters
    pub index: Sym,
    pub rule: Sym,
    pub rule_delayed: Sym,
    pub options: Sym,
}

/// The shared state of the natives: symbols only, so they need no tracing.
pub(crate) struct Lib {
    pub s: Syms,
}

impl Lib {
    pub fn new(h: &Heap) -> Lib {
        Lib {
            s: Syms {
                list: h.sym(names::LIST),
                failure: h.sym(names::FAILURE),
                argument: h.sym("argument"),
                index: h.sym("index"),
                rule: h.sym(names::RULE),
                rule_delayed: h.sym(names::RULE_DELAYED),
                options: h.sym("options"),
            },
        }
    }

    /// `failure(tag, details...)`.
    pub fn fail<'h>(&self, h: &'h Heap, tag: Sym, details: &[T<'h>]) -> T<'h> {
        let mut a = Vec::with_capacity(details.len() + 1);
        a.push(h.sym_term(tag));
        a.extend_from_slice(details);
        h.call(self.s.failure, &a)
    }

    /// `failure(argument, f, k)` for argument `k` (1-based) of the call `t`.
    pub fn bad_arg<'h>(&self, h: &'h Heap, t: T<'h>, k: usize) -> Reply<'h> {
        let f = match h.view(t) {
            View::App { head, .. } => head,
            _ => t,
        };
        Reply::Done(self.fail(h, self.s.argument, &[f, h.int(k as i64)]))
    }

    /// `failure(index, i, n)`.
    pub fn bad_index<'h>(&self, h: &'h Heap, i: T<'h>, n: usize) -> Reply<'h> {
        Reply::Done(self.fail(h, self.s.index, &[i, h.int(n as i64)]))
    }

    /// The elements of a list term.
    pub fn as_list<'h>(&self, h: &'h Heap, t: T<'h>) -> Option<&'h [T<'h>]> {
        match h.view(t) {
            View::App { head, args } if head == h.sym_term(self.s.list) => Some(args),
            _ => None,
        }
    }

    /// Argument `k` of the call `t` as a string: a symbol or compound term declines (the call stays unevaluated,
    /// as for any symbolic argument), another atom is `failure(argument, f, k)`.
    pub fn string<'h>(&self, h: &'h Heap, t: T<'h>, a: T<'h>, k: usize) -> Result<&'h str, Reply<'h>> {
        match h.view(a) {
            View::Str(s) => Ok(s),
            View::Sym(_) | View::App { .. } => Err(Reply::Decline),
            _ => Err(self.bad_arg(h, t, k)),
        }
    }

    /// Argument `k` as a nonempty string (a separator, a search string).
    pub fn needle<'h>(&self, h: &'h Heap, t: T<'h>, a: T<'h>, k: usize) -> Result<&'h str, Reply<'h>> {
        match self.string(h, t, a, k)? {
            "" => Err(self.bad_arg(h, t, k)),
            s => Ok(s),
        }
    }

    /// Argument `k` as one string or a list of strings (each nonempty if `nonempty`).
    pub fn strings<'h>(
        &self,
        h: &'h Heap,
        t: T<'h>,
        a: T<'h>,
        k: usize,
        nonempty: bool,
    ) -> Result<Vec<&'h str>, Reply<'h>> {
        let one = |x: T<'h>| if nonempty { self.needle(h, t, x, k) } else { self.string(h, t, x, k) };
        match self.as_list(h, a) {
            Some(items) => items
                .iter()
                .map(|&x| match h.view(x) {
                    // a symbol inside a list is not a pending value: the list is malformed
                    View::Sym(_) | View::App { .. } => Err(self.bad_arg(h, t, k)),
                    _ => one(x),
                })
                .collect(),
            None => Ok(vec![one(a)?]),
        }
    }

    /// Argument `k` as a machine integer: a symbol or compound term declines, another atom is a failure.
    pub fn int<'h>(&self, h: &'h Heap, t: T<'h>, a: T<'h>, k: usize) -> Result<i64, Reply<'h>> {
        match h.view(a) {
            View::Int(v) => Ok(v),
            View::Sym(_) | View::App { .. } => Err(Reply::Decline),
            _ => Err(self.bad_arg(h, t, k)),
        }
    }

    /// The `(name, value)` pairs of a trailing `options(name -> v, ...)` argument, and the arguments before it.
    pub fn options<'a, 'h>(&self, h: &'h Heap, args: &'a [T<'h>]) -> (&'a [T<'h>], Vec<(Sym, T<'h>)>) {
        let Some((&last, rest)) = args.split_last() else { return (args, vec![]) };
        let View::App { head, args: rules } = h.view(last) else { return (args, vec![]) };
        if head != h.sym_term(self.s.options) {
            return (args, vec![]);
        }
        let mut out = vec![];
        for &r in rules {
            match h.view(r) {
                View::App { head, args: &[k, v] } if head == h.sym_term(self.s.rule) => match h.view(k) {
                    View::Sym(s) => out.push((s, v)),
                    _ => return (args, vec![]),
                },
                _ => return (args, vec![]),
            }
        }
        (rest, out)
    }

    /// `failure(unknown_option, name)`.
    pub fn unknown_option<'h>(&self, h: &'h Heap, name: Sym) -> Reply<'h> {
        Reply::Done(self.fail(h, h.sym("unknown_option"), &[h.sym_term(name)]))
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

/// Bytes per fuel unit of the string natives' work (copying, scanning).
const PER_UNIT: usize = 16;

/// The fuel of a pass over `n` bytes, charged before it (one pass is too fast to need polling inside).
pub(crate) fn charge(cx: &mut NativeCx<'_, '_, '_>, n: usize) -> Res<()> {
    cx.tick((n / PER_UNIT) as u64 + 1)
}

/// The most bytes a native builds in one string: a larger result is `failure(limit, memory, call)`.
pub(crate) const MAX_BYTES: usize = 1 << 30;

/// `failure(limit, memory, t)`.
pub(crate) fn too_large<'h>(lib: &Lib, h: &'h Heap, t: T<'h>) -> Reply<'h> {
    let memory = h.sym_term(h.sym(names::failure::MEMORY));
    Reply::Done(lib.fail(h, h.sym(names::failure::LIMIT), &[memory, t]))
}
