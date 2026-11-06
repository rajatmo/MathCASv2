//! The number functions of roadmap package 1C, as natives: complex parts ([`crate::parts`]: `re`, `im_part`,
//! `conjugate`, `arg`, and rules extending `abs` and `sign` to closed numeric terms), precision and approximation
//! ([`crate::approx`]: `precision`, `accuracy`, `set_precision`, `rationalize`, `chop`) and piecewise functions
//! ([`crate::piecewise`]: `clip`, `rescale`, `unit_step`).
//!
//! This module holds what they share: the symbols they build, the stop flag, the walk that rebuilds a term with
//! its number leaves replaced, and their registration.
use crate::numeval::Names;
use mc_api::{names, Store, Sym, View, T};
use mc_arith::{Arith, Cmp3, Num};
use mc_core::Heap;
use mc_eval::{Attributes, Kernel, Native, NativeCx, Reply, Res};
use rustc_hash::FxHashMap;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// The symbols the number functions build and recognise.
pub(crate) struct Syms {
    pub add: Sym,
    pub mul: Sym,
    pub pow: Sym,
    pub list: Sym,
    pub failure: Sym,
    pub pi: Sym,
    pub inf: Sym,
    pub sin: Sym,
    pub cos: Sym,
    pub atan: Sym,
    pub re: Sym,
    pub im_part: Sym,
    pub conjugate: Sym,
    pub arg: Sym,
    pub abs: Sym,
    pub sign: Sym,
    pub clip: Sym,
    pub rescale: Sym,
}

impl Syms {
    fn new(h: &Heap) -> Syms {
        Syms {
            add: h.sym(names::ADD),
            mul: h.sym(names::MUL),
            pow: h.sym(names::POW),
            list: h.sym(names::LIST),
            failure: h.sym(names::FAILURE),
            pi: h.sym("pi"),
            inf: h.sym("inf"),
            sin: h.sym("sin"),
            cos: h.sym("cos"),
            atan: h.sym("atan"),
            re: h.sym("re"),
            im_part: h.sym("im_part"),
            conjugate: h.sym("conjugate"),
            arg: h.sym("arg"),
            abs: h.sym("abs"),
            sign: h.sym("sign"),
            clip: h.sym("clip"),
            rescale: h.sym("rescale"),
        }
    }
}

/// The shared state of the number functions: symbols, the names numerical evaluation recognises, the stop flag.
pub(crate) struct Env {
    pub s: Syms,
    pub nm: Arc<Names>,
    pub stop: Arc<AtomicBool>,
}

impl Env {
    /// `mc-arith`'s canonicaliser with the stop flag.
    pub fn arith<'h>(&self, h: &'h Heap) -> Arith<'h, Heap> {
        Arith::new(h).with_stop(self.stop.clone())
    }

    /// `failure(tag, details...)`.
    pub fn fail<'h>(&self, h: &'h Heap, tag: &str, details: &[T<'h>]) -> T<'h> {
        let mut a = Vec::with_capacity(details.len() + 1);
        a.push(h.sym_term(h.sym(tag)));
        a.extend_from_slice(details);
        h.call(self.s.failure, &a)
    }

    /// `failure(invalid_argument, call)`.
    pub fn invalid<'h>(&self, h: &'h Heap, t: T<'h>) -> Reply<'h> {
        Reply::Done(self.fail(h, crate::failure_tags::INVALID_ARGUMENT, &[t]))
    }

    /// The elements of a list term.
    pub fn as_list<'h>(&self, h: &'h Heap, t: T<'h>) -> Option<&'h [T<'h>]> {
        match h.view(t) {
            View::App { head, args } if head == h.sym_term(self.s.list) => Some(args),
            _ => None,
        }
    }

    /// The head symbol and arguments of a call.
    pub fn call_of<'h>(&self, h: &'h Heap, t: T<'h>) -> Option<(Sym, &'h [T<'h>])> {
        match h.view(t) {
            View::App { head, args } => match h.view(head) {
                View::Sym(s) => Some((s, args)),
                _ => None,
            },
            _ => None,
        }
    }

    /// `f(x, rest...)` for every element x of a list first argument, as a list to evaluate.
    pub fn map_first<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, f: Sym, xs: &[T<'h>], rest: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        cx.tick(xs.len() as u64)?;
        let mut parts = Vec::with_capacity(rest.len() + 1);
        let items: Vec<T<'h>> = xs
            .iter()
            .map(|&x| {
                parts.clear();
                parts.push(x);
                parts.extend_from_slice(rest);
                h.call(f, &parts)
            })
            .collect();
        Ok(Reply::Again(h.call(self.s.list, &items)))
    }

    /// The three-valued comparison of two real numbers or closed numeric terms (by refinement, decision L.8);
    /// `None` when either is not numeric.
    pub fn compare<'h>(&self, h: &'h Heap, ar: &Arith<'h, Heap>, a: T<'h>, b: T<'h>) -> Option<Cmp3> {
        match (ar.num(a), ar.num(b)) {
            (Some(Num::Real(_)), Some(Num::Real(_))) => ar.compare(a, b),
            (Some(Num::Complex(..)), _) | (_, Some(Num::Complex(..))) => None,
            // signed infinities against real numbers (mc-arith decides those)
            _ if ar.inf_sign(a).is_some() || ar.inf_sign(b).is_some() => ar.compare(a, b),
            _ => crate::numeval::compare(h, ar, &self.nm, a, b),
        }
    }
}

/// How many subterms a walk visits between two fuel charges.
const WALK_CHUNK: u64 = 256;

/// What a leaf function decides for one number.
pub(crate) enum Leaf<'h> {
    /// keep the number
    Keep,
    /// replace it by this term
    With(T<'h>),
}

/// `t` with every number replaced as `leaf` decides, and every other subterm for which `other` gives a value
/// replaced by it; compound terms are rebuilt (not evaluated). Iterative, each shared subterm once; heads are kept.
pub(crate) fn map_leaves<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    ar: &Arith<'h, Heap>,
    t: T<'h>,
    leaf: &mut dyn FnMut(&mut NativeCx<'_, '_, 'h>, &Num, T<'h>) -> Res<Leaf<'h>>,
    other: &mut dyn FnMut(&mut NativeCx<'_, '_, 'h>, T<'h>) -> Res<Option<T<'h>>>,
) -> Res<T<'h>> {
    let h = cx.heap();
    let mut done: FxHashMap<T<'h>, T<'h>> = FxHashMap::default();
    let mut stack = vec![(t, false)];
    let mut visited = 0u64;
    while let Some((u, expanded)) = stack.pop() {
        if !expanded && done.contains_key(&u) {
            continue;
        }
        visited += 1;
        if visited.is_multiple_of(WALK_CHUNK) {
            cx.tick(1)?;
        }
        if !expanded {
            if let Some(x) = ar.num(u) {
                let v = match leaf(cx, &x, u)? {
                    Leaf::Keep => u,
                    Leaf::With(v) => v,
                };
                done.insert(u, v);
                continue;
            }
            if let Some(v) = other(cx, u)? {
                done.insert(u, v);
                continue;
            }
        }
        match h.view(u) {
            View::App { args, .. } if !expanded => {
                stack.push((u, true));
                stack.extend(args.iter().filter(|a| !done.contains_key(a)).map(|&a| (a, false)));
            }
            View::App { head, args } => {
                let new: Vec<T<'h>> = args.iter().map(|a| done[a]).collect();
                let v = if new.as_slice() == args { u } else { h.app(head, &new) };
                done.insert(u, v);
            }
            _ => {
                done.insert(u, u);
            }
        }
    }
    Ok(done[&t])
}

/// Every number in `t` (each shared subterm once), heads excluded. Iterative.
pub(crate) fn for_numbers<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    ar: &Arith<'h, Heap>,
    t: T<'h>,
    visit: &mut dyn FnMut(&Num),
) -> Res<()> {
    let h = cx.heap();
    let mut seen: rustc_hash::FxHashSet<T<'h>> = rustc_hash::FxHashSet::default();
    let mut stack = vec![t];
    while let Some(u) = stack.pop() {
        if !seen.insert(u) {
            continue;
        }
        if (seen.len() as u64).is_multiple_of(WALK_CHUNK) {
            cx.tick(1)?;
        }
        if let Some(x) = ar.num(u) {
            visit(&x);
            continue;
        }
        if let View::App { args, .. } = h.view(u) {
            stack.extend(args.iter().filter(|a| !seen.contains(a)));
        }
    }
    Ok(())
}

/// The implementation of one number function.
pub(crate) type Body = for<'a, 'b, 'c, 'h> fn(&Env, &mut NativeCx<'a, 'b, 'h>, T<'h>, &'c [T<'h>]) -> Res<Reply<'h>>;

/// A native number function: its body and the shared state (symbols and a flag only, so no tracing).
struct NumFn {
    env: Arc<Env>,
    body: Body,
}

impl Native for NumFn {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let _scope = mc_arith::num::StopScope::enter(Some(&self.env.stop));
        (self.body)(&self.env, cx, t, args)
    }
}

/// The number functions: name, body, whether the kernel threads them over list arguments, description.
const TABLE: &[(&str, Body, bool, &str)] = &[
    ("re", crate::parts::re, true, "re(z): the real part; exact, ball and machine numbers, closed numeric terms"),
    ("im_part", crate::parts::im_part, true, "im_part(z): the imaginary part (an exact 0 for every real number)"),
    ("conjugate", crate::parts::conjugate, true, "conjugate(z): the complex conjugate"),
    (
        "arg",
        crate::parts::arg,
        true,
        "arg(z): the principal argument in (-pi, pi]; exact (0, pi, atan forms) for exact and real input",
    ),
    (
        "precision",
        crate::approx::precision,
        false,
        "precision(x): correct significant decimal digits (inf if exact, 15.95 for machine reals); least over a term",
    ),
    (
        "accuracy",
        crate::approx::accuracy,
        false,
        "accuracy(x): correct decimal digits after the point, -log10 of the error bound; the least over a term",
    ),
    (
        "set_precision",
        crate::approx::set_precision,
        false,
        "set_precision(x, d): every number as a ball of d digits (machine reals by their binary value); d = inf: exact",
    ),
    (
        "rationalize",
        crate::approx::rationalize,
        false,
        "rationalize(x): the simplest rational within an inexact number's error if close; (x, dx): within dx",
    ),
    (
        "chop",
        crate::approx::chop,
        false,
        "chop(x), chop(x, delta): inexact numbers (and parts) smaller than delta (default 10^-10) replaced by exact 0",
    ),
    (
        "clip",
        crate::piecewise::clip,
        false,
        "clip(x), clip(x, [lo, hi]), clip(x, [lo, hi], [vlo, vhi]): x limited to [lo, hi] (default [-1, 1])",
    ),
    (
        "rescale",
        crate::piecewise::rescale,
        false,
        "rescale(x, [a, b]): (x - a)/(b - a); rescale(x, [a, b], [c, d]): onto [c, d]; rescale(xs): by its range",
    ),
    (
        "unit_step",
        crate::piecewise::unit_step,
        true,
        "unit_step(x, ...): 1 if every x >= 0, else 0 (exact for every kind of real); decided by refinement",
    ),
];

/// The descriptions of [`TABLE`], for `crate::FUNCTIONS`.
pub(crate) const fn description(name: &str) -> &'static str {
    let mut i = 0;
    while i < TABLE.len() {
        if const_eq(TABLE[i].0, name) {
            return TABLE[i].3;
        }
        i += 1;
    }
    panic!("no such number function")
}

const fn const_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    let mut i = 0;
    while i < a.len() {
        if a[i] != b[i] {
            return false;
        }
        i += 1;
    }
    true
}

/// Register the number functions with `k`, and the rules extending `abs` and `sign` (decision 1C.7).
pub(crate) fn install(k: &mut Kernel, h: &Heap, nm: &Arc<Names>, stop: &Arc<AtomicBool>) {
    let env = Arc::new(Env { s: Syms::new(h), nm: nm.clone(), stop: stop.clone() });
    for &(name, body, listable, _) in TABLE {
        let attrs = Attributes { listable, ..Attributes::default() };
        k.register(h.sym(name), attrs, Box::new(NumFn { env: env.clone(), body }));
    }
    crate::parts::define_abs_sign(k, h, &env);
}
