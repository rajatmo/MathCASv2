//! MathCAS v2 sets and associations (package 1B): natives of the evaluator for set operations on lists,
//! associations (insertion-ordered maps from any term to any term) and the failure tests.
//!
//! [`install`] registers every function of [`FUNCTIONS`] with a kernel, and the argument-head rules that make
//! `a[k]` a lookup. Wrong arguments follow `mc-lists`: a symbol where an association, list or set is expected leaves
//! the call unevaluated, any other wrong argument gives `failure(argument, f, k)` (argument `k` of `f`).
//!
//! - `sets`: `union`, `intersection`, `complement`, `is_subset`;
//! - `assoc`: `assoc`, `is_assoc`, `keys`, `values`, `to_rules`, `lookup`, `key_exists`, `key_take`, `key_drop`,
//!   `key_sort`, `insert`, `delete_key`, `merge`, `map_values`, `select_keys`;
//! - `group`: `counts`, `group_by`, `is_failure`, `failure_tag`;
//! - `index`: the per-term key indexes that keep lookups from being linear.
//!
//! **Associations** are the terms `assoc(k1 -> v1, ..., kn -> vn)` (that is, `assoc(rule(k1, v1), ...)`), keys
//! distinct and compared by identity, in insertion order; they print as that call and read back to the same term.
//! `length(a)` (from `mc-lists`) is the number of keys. Decisions of this package are numbered 1B.1, 1B.2, ... in
//! the comments where they are applied.
//!
//! ```
//! use mc_core::{Session, Store};
//! let mut s = Session::with_state(mc_core::Config::default(), 1, |h| {
//!     let mut k = mc_eval::Kernel::new(h, mc_eval::EvalConfig::default());
//!     mc_assoc::install(&mut k, h);
//!     k
//! });
//! let found = s.step(|h, k| {
//!     let (a, b) = (h.sym_term(h.sym("a")), h.sym_term(h.sym("b")));
//!     let rule = |x, v| h.call(h.sym("rule"), &[x, h.int(v)]);
//!     let m = h.call(h.sym("assoc"), &[rule(a, 1), rule(b, 2), rule(a, 3)]);
//!     let t = h.call(h.sym("lookup"), &[m, a]);
//!     k.eval(h, t).value == h.int(3)
//! });
//! assert!(found);
//! ```
#![warn(missing_docs)]
mod assoc;
mod ctx;
mod group;
mod index;
mod sets;

use ctx::Lib;
use mc_core::{Heap, Raw, Store, T};
use mc_eval::pat::{self, Pat};
use mc_eval::{Attributes, Kernel, Native, NativeCx, Reply, Res, Rhs};
use std::sync::Arc;

/// The implementation of one function.
type Body = for<'a, 'b, 'c, 'h> fn(&Lib, &mut NativeCx<'a, 'b, 'h>, T<'h>, &'c [T<'h>]) -> Res<Reply<'h>>;

/// A native of the library: its body and the shared state. It keeps no term handles (decision 1B.6).
struct Fun {
    lib: Arc<Lib>,
    body: Body,
}

impl Native for Fun {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        (self.body)(&self.lib, cx, t, args)
    }

    /// A safe point: the cached indexes name terms by handle, which the collection may move or reuse.
    fn trace(&mut self, _visit: &mut dyn FnMut(&mut Raw)) {
        self.lib.indexes.clear();
    }
}

/// How a function receives failure arguments.
#[derive(Clone, Copy)]
enum Mode {
    /// a failure argument is the result (semantics 10)
    Strict,
    /// failures arrive as values
    Failures,
}

macro_rules! functions {
    ($($name:literal => $body:path, $mode:ident, $desc:literal;)*) => {
        /// Every function of the library, with a one-line description.
        pub const FUNCTIONS: &[(&str, &str)] = &[$(($name, $desc),)*];
        const BODIES: &[(&str, Body, Mode)] = &[$(($name, $body, Mode::$mode),)*];
    };
}

functions! {
    "union" => sets::union, Strict,
        "union(xs, ys, ...): the elements of any argument, without repetitions, in canonical order";
    "intersection" => sets::intersection, Strict,
        "intersection(xs, ys, ...): the elements of every argument, without repetitions, in canonical order";
    "complement" => sets::complement, Strict,
        "complement(all, xs, ...): the elements of all in no other argument, without repetitions, in canonical order";
    "is_subset" => sets::is_subset, Strict,
        "is_subset(xs, ys): whether every element of xs is an element of ys";
    "assoc" => assoc::assoc, Strict,
        "assoc(k -> v, ...), assoc([k -> v, ...]): the association of the entries; a repeated key keeps its last value";
    "is_assoc" => assoc::is_assoc, Strict,
        "is_assoc(x): whether x is an association";
    "keys" => assoc::keys, Strict,
        "keys(a): the list of the keys of an association, in order";
    "values" => assoc::values, Strict,
        "values(a): the list of the values of an association, in order";
    "to_rules" => assoc::to_rules, Strict,
        "to_rules(a): the list of the entries k -> v of an association, in order";
    "lookup" => assoc::lookup, Strict,
        "lookup(a, k): the value of key k (failure(missing_key, k) if none); lookup(a, k, d): d if none";
    "key_exists" => assoc::key_exists, Strict,
        "key_exists(a, k): whether the association a has the key k";
    "key_take" => assoc::key_take, Strict,
        "key_take(a, [k, ...]): the entries of the keys that a has, in the order given";
    "key_drop" => assoc::key_drop, Strict,
        "key_drop(a, [k, ...]): the association without the keys given";
    "key_sort" => assoc::key_sort, Strict,
        "key_sort(a): the entries ordered by key in the canonical order";
    "insert" => assoc::insert, Strict,
        "insert(a, k -> v) or insert(a, k, v): the association with key k set to v (in place, or added at the end)";
    "delete_key" => assoc::delete_key, Strict,
        "delete_key(a, k): the association without the key k";
    "merge" => assoc::merge, Strict,
        "merge([a1, a2, ...]): all entries, later values replacing earlier; merge([...], f): f of each key's values";
    "map_values" => assoc::map_values, Strict,
        "map_values(f, a): the association with every value v replaced by f(v)";
    "select_keys" => assoc::select_keys, Strict,
        "select_keys(a, f): the entries whose key k has f(k) true";
    "counts" => group::counts, Strict,
        "counts(xs): the association from each distinct element to its number of occurrences";
    "group_by" => group::group_by, Strict,
        "group_by(xs, f): the association from each value f(x) to the list of those x; group_by(xs, f, g): g of each";
    "is_failure" => group::is_failure, Failures,
        "is_failure(x): whether x is a failure(...)";
    "failure_tag" => group::failure_tag, Failures,
        "failure_tag(x): the tag of a failure(tag, ...)";
}

/// Register every function of [`FUNCTIONS`] with `k` (all strict; `is_failure` and `failure_tag` receive failures),
/// and the argument-head rules on `assoc` that make part access a lookup (decision 1B.4):
/// `part(a @ assoc(___), k_) := lookup(a, k)` and `part(a @ assoc(___), k_, ks__) := part(lookup(a, k), ks...)`.
pub fn install(k: &mut Kernel, h: &Heap) {
    let lib = Arc::new(Lib::new(h));
    for &(name, body, mode) in BODIES {
        let a = Attributes { handles_failures: matches!(mode, Mode::Failures), ..Attributes::default() };
        k.register(h.sym(name), a, Box::new(Fun { lib: lib.clone(), body }));
    }
    let (part, assoc, lookup) = (h.sym("part"), lib.s.assoc, h.sym("lookup"));
    let whole = || pat::named(0, pat::call(h, assoc, vec![Pat::Seq { var: None, min: 0 }]));
    let one: Rhs = Arc::new(move |b, h| h.call(lookup, &[b.one(0), b.one(1)]));
    let more: Rhs = Arc::new(move |b, h| {
        let mut args = vec![h.call(lookup, &[b.one(0), b.one(1)])];
        args.extend_from_slice(b.seq(2));
        h.call(part, &args)
    });
    let rules = [
        (pat::call(h, part, vec![whole(), pat::var(1)]), one),
        (pat::call(h, part, vec![whole(), pat::var(1), pat::seq1(2)]), more),
    ];
    for (lhs, rhs) in rules {
        k.define_attached(h, lhs, rhs, assoc).expect("the part rules of associations are well formed");
    }
}
