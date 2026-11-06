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
//! - `index`: the per-term key indexes that keep lookups from being linear;
//! - `dataset`, `aggregate`, `table`, `text`: datasets (below).
//!
//! **Associations** are the terms `assoc(k1 -> v1, ..., kn -> vn)` (that is, `assoc(rule(k1, v1), ...)`), keys
//! distinct and compared by identity, in insertion order; they print as that call and read back to the same term.
//! `length(a)` (from `mc-lists`) is the number of keys. Decisions of this package are numbered 1B.1, 1B.2, ... in
//! the comments where they are applied.
//!
//! **Datasets** (package data, decisions DATA.n) are tables: `dataset(assoc(c1 -> col1, ...))`, an association from
//! column names to columns of one length, numeric columns packed. `dataset` makes one from rows (associations or
//! lists under a header) or columns; `column`, `columns`, `rows`, `add_column`, `group_by`, `aggregate`, `pivot`
//! and `join_on` query and reshape it, and the list functions `length`, `part`, `first`, `last`, `rest`, `most`,
//! `take`, `drop`, `reverse`, `join`, `select` and `sort_by` act on its rows (they decline in `mc-lists`, and the
//! dataset's argument-head rules answer). `missing()` marks an absent value; aggregates skip it. [`table_text`] is
//! the printed form for text front ends.
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
mod aggregate;
mod assoc;
mod ctx;
mod dataset;
mod group;
mod index;
mod sets;
mod table;
mod text;

pub use table::Col as Column;
pub use text::{table_text, MAX_CELL, MAX_ROWS, MAX_WIDTH};

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
    /// arguments after the first held: the native evaluates them when it needs them (`lookup`'s default)
    HoldRest,
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
    "lookup" => assoc::lookup, HoldRest,
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
    "dataset" => dataset::dataset, Strict,
        "dataset(rows), dataset(assoc(c -> column, ...)), dataset(table), dataset(rows, names): a table of rows and named columns";
    "is_dataset" => dataset::is_dataset, Strict,
        "is_dataset(x): whether x is a dataset";
    "missing" => dataset::missing, Strict,
        "missing(), missing(reason): an absent value in a dataset; aggregates skip it";
    "is_missing" => dataset::is_missing, Strict,
        "is_missing(x): whether x is missing(...)";
    "column" => dataset::column, Strict,
        "column(ds, c): the list of the values of column c of a dataset";
    "columns" => dataset::columns, Strict,
        "columns(ds): the column names of a dataset; columns(ds, [c, ...]): the dataset of those columns";
    "rows" => dataset::rows, Strict,
        "rows(ds): the rows of a dataset, each an association from column names to values";
    "add_column" => dataset::add_column, Strict,
        "add_column(ds, c, f): the dataset with column c set to f(row) for each row (or to a list of values)";
    "aggregate" => aggregate::aggregate, Strict,
        "aggregate(ds, [c -> f, ...]), aggregate(ds, by, specs): total, mean, count, min, max or any f of columns, per group of rows";
    "pivot" => aggregate::pivot, Strict,
        "pivot(ds, r, c, v), pivot(ds, r, c, v, f): a table with a row per value of column r and a column per value of column c";
    "join_on" => dataset::join_on, Strict,
        "join_on(a, b, key): the rows of datasets a and b whose key columns agree, joined (how: inner, left, right, outer)";
}

/// Register every function of [`FUNCTIONS`] with `k` (all strict; `is_failure` and `failure_tag` receive failures),
/// and the argument-head rules on `assoc` that make part access a lookup (decision 1B.4):
/// `part(a @ assoc(___), k_) := lookup(a, k)` and `part(a @ assoc(___), k_, ks__) := part(lookup(a, k), ks...)`.
pub fn install(k: &mut Kernel, h: &Heap) {
    let lib = Arc::new(Lib::new(h));
    for &(name, body, mode) in BODIES {
        let hold = if matches!(mode, Mode::HoldRest) { mc_eval::Hold::Rest } else { mc_eval::Hold::None };
        let a = Attributes { handles_failures: matches!(mode, Mode::Failures), hold, ..Attributes::default() };
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
    install_rows(k, h, &lib);
}

/// The list functions on a dataset's rows (decision DATA.2): `mc-lists` declines them on a dataset, and the rule
/// `f(d @ dataset(___), args___) := dataset_rows(f, d, args...)` attached to `dataset` hands them to the internal
/// native `dataset_rows` (not in [`FUNCTIONS`]: it gives back `f(d, args...)` when it cannot evaluate the call).
fn install_rows(k: &mut Kernel, h: &Heap, lib: &Arc<Lib>) {
    let rows_op = h.sym(dataset::ROWS_OP);
    k.register(rows_op, Attributes::default(), Box::new(Fun { lib: lib.clone(), body: dataset::rows_op }));
    let data = lib.d.dataset;
    for &name in dataset::ROW_FUNCTIONS {
        let f = h.sym(name);
        let lhs = pat::call(
            h,
            f,
            vec![pat::named(0, pat::call(h, data, vec![Pat::Seq { var: None, min: 0 }])), pat::seq0(1)],
        );
        let rhs: Rhs = Arc::new(move |b, h| {
            let mut args = vec![h.sym_term(f), b.one(0)];
            args.extend_from_slice(b.seq(1));
            h.call(rows_op, &args)
        });
        k.define_attached(h, lhs, rhs, data).expect("the row rules of datasets are well formed");
    }
}

/// The dataset with columns `cols` (lists of one length) named `names` (distinct): `dataset(assoc(name -> col,
/// ...))`. Neither condition is checked; `mc-io` builds imported tables with it.
pub fn dataset_of_columns<'h>(h: &'h Heap, names: &[T<'h>], cols: &[T<'h>]) -> T<'h> {
    table::DataSyms::new(h).dataset(h, names, cols)
}

/// The column names and columns of `t` when it is a dataset in normal form.
pub fn dataset_columns<'h>(h: &'h Heap, t: T<'h>) -> Option<(Vec<T<'h>>, Vec<Column<'h>>)> {
    let tab = table::Table::of(&table::DataSyms::new(h), h, t)?;
    Some((tab.names, tab.cols))
}

/// The absent value `missing()`.
pub fn missing_value<'h>(h: &'h Heap) -> T<'h> {
    table::DataSyms::new(h).missing(h)
}

/// Is `t` an absent value `missing(...)`?
pub fn is_missing_value(h: &Heap, t: T<'_>) -> bool {
    table::DataSyms::new(h).is_missing(h, t)
}
