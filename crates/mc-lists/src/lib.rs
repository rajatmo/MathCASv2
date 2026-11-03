//! MathCAS v2 list and structure library (package I): natives of the evaluator for building, taking apart,
//! reshaping, reducing, selecting and sorting lists and other compound terms, and a few string functions.
//!
//! [`install`] registers every function of [`FUNCTIONS`] with a kernel. The natives work on any compound term where
//! that makes sense (`length(a + b)` is 2, `rest(f(a, b, c))` is `f(b, c)`); a symbol where a compound term is
//! expected leaves the call unevaluated, any other wrong argument gives `failure(argument, f, k)` (argument `k` of
//! `f`), and an index outside a term gives `failure(index, i, n)`.
//!
//! - `construct`: `range`, `array`, `nest`, `nest_list`, `fold`, `fold_list`;
//! - `access`: `length`, `part`, `first`, `last`, `rest`, `most`, `take`, `drop`;
//! - `reshape`: `append`, `prepend`, `join`, `reverse`, `flatten`, `transpose`, `apply`;
//! - `reduce`: `sum`, `product`, `total`, `max`, `min`;
//! - `select`: `select`, `cases`, `count` (patterns through `matching`);
//! - `sort`: `sort`, `ordered`;
//! - `measure`: `depth`, `leaf_count`;
//! - `strings`: `string_join`, `to_string`, `string_length`, `split`.
//!
//! Every loop over elements charges fuel and polls the interrupt (`ctx::Meter`), results are built once (no
//! repeated appends), and nested structure is walked with explicit stacks. Decisions of this package are numbered
//! I.1, I.2, ... in the comments where they are applied.
//!
//! ```
//! use mc_core::{Session, Store};
//! use std::sync::{atomic::AtomicBool, Arc};
//! let stop = Arc::new(AtomicBool::new(false));
//! let mut s = Session::with_state(mc_core::Config::default(), 1, |h| {
//!     let mut k = mc_eval::Kernel::new(h, mc_eval::EvalConfig::default());
//!     mc_lists::install(&mut k, h, &stop);
//!     k
//! });
//! let n = s.step(|h, k| {
//!     let t = h.call(h.sym("length"), &[h.call(h.sym("range"), &[h.int(10)])]);
//!     match h.view(k.eval(h, t).value) {
//!         mc_api::View::Int(n) => n,
//!         _ => -1,
//!     }
//! });
//! assert_eq!(n, 10);
//! ```
#![warn(missing_docs)]
mod access;
mod construct;
mod ctx;
mod matching;
mod measure;
mod reduce;
mod reshape;
mod select;
mod sort;
mod strings;

use ctx::Lib;
use mc_core::{Heap, Store, T};
use mc_eval::{Attributes, Kernel, Native, NativeCx, Reply, Res};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// The implementation of one function.
type Body = for<'a, 'b, 'c, 'h> fn(&Lib, &mut NativeCx<'a, 'b, 'h>, T<'h>, &'c [T<'h>]) -> Res<Reply<'h>>;

/// A native of the library: its body and the shared state. It keeps only symbols, so it needs no tracing.
struct Fun {
    lib: Arc<Lib>,
    body: Body,
}

impl Native for Fun {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        (self.body)(&self.lib, cx, t, args)
    }
}

macro_rules! functions {
    ($($name:literal => $body:path, $desc:literal;)*) => {
        /// Every function of the library, with a one-line description.
        pub const FUNCTIONS: &[(&str, &str)] = &[$(($name, $desc),)*];
        const BODIES: &[(&str, Body)] = &[$(($name, $body),)*];
    };
}

functions! {
    "range" => construct::range,
        "range(n), range(a, b), range(a, b, step): the list a, a + step, ... up to b (a and step default to 1)";
    "array" => construct::array,
        "array(f, n): the list [f(1), ..., f(n)]";
    "nest" => construct::nest,
        "nest(f, x, n): f applied n times to x";
    "nest_list" => construct::nest_list,
        "nest_list(f, x, n): [x, f(x), ..., f^n(x)]";
    "fold" => construct::fold,
        "fold(f, x, xs): f(...f(f(x, xs[1]), xs[2])..., xs[n]); fold(f, xs) starts from xs[1]";
    "fold_list" => construct::fold_list,
        "fold_list(f, x, xs): the list of every intermediate value of fold";
    "length" => access::length,
        "length(t): the number of arguments of t (0 for an atom)";
    "part" => access::part,
        "part(t, i, ...): t[i, ...], 1-based, negative from the end, 0 the head; a span i:j or a list selects several";
    "first" => access::first,
        "first(xs): the first element";
    "last" => access::last,
        "last(xs): the last element";
    "rest" => access::rest,
        "rest(xs): all elements but the first";
    "most" => access::most,
        "most(xs): all elements but the last";
    "take" => access::take_,
        "take(xs, n): the first n elements (the last -n for negative n)";
    "drop" => access::drop,
        "drop(xs, n): all but the first n elements (the last -n for negative n)";
    "append" => reshape::append,
        "append(xs, x): x added at the end";
    "prepend" => reshape::prepend,
        "prepend(xs, x): x added at the start";
    "join" => reshape::join,
        "join(xs, ys, ...): the elements of terms with one head, in order";
    "reverse" => reshape::reverse,
        "reverse(xs): the elements in reverse order";
    "unique" => reshape::unique,
        "unique(xs): the elements without repetitions (compared by identity), in order of first occurrence";
    "flatten" => reshape::flatten,
        "flatten(xs), flatten(xs, d): nested terms with the head of xs spliced in (to depth d)";
    "transpose" => reshape::transpose,
        "transpose(m): rows and columns of a rectangular list of lists exchanged";
    "apply" => reshape::apply,
        "apply(f, xs): the head of xs replaced by f";
    "sum" => reduce::sum,
        "sum(xs): the canonical sum of the elements";
    "product" => reduce::product,
        "product(xs): the canonical product of the elements";
    "total" => reduce::total,
        "total(xs): the sum of every element of nested lists";
    "max" => reduce::max,
        "max(xs), max(a, b, ...): the largest real number (lists flattened)";
    "min" => reduce::min,
        "min(xs), min(a, b, ...): the smallest real number (lists flattened)";
    "select" => select::select,
        "select(xs, f): the elements x with f(x) true";
    "cases" => select::cases,
        "cases(xs, p): the elements matching the pattern p; cases(xs, p -> r): r for each match";
    "count" => select::count,
        "count(xs, p): the number of elements matching the pattern p";
    "sort" => sort::sort,
        "sort(xs): the elements in canonical order; sort(xs, by): ordered by the predicate by(a, b)";
    "ordered" => sort::ordered,
        "ordered(xs), ordered(xs, by): whether the elements are in canonical order (or by holds for neighbours)";
    "depth" => measure::depth,
        "depth(t): 1 for an atom, one more than the deepest argument otherwise";
    "leaf_count" => measure::leaf_count,
        "leaf_count(t): the number of atoms in t, heads included";
    "string_join" => strings::string_join,
        "string_join(s1, s2, ...) or string_join([s1, ...]): the strings concatenated";
    "to_string" => strings::to_string,
        "to_string(t): the printed form of t (a string stays itself)";
    "string_length" => strings::string_length,
        "string_length(s): the number of characters of s";
    "split" => strings::split,
        "split(s), split(s, sep): the words of s, or the pieces between occurrences of sep";
}

/// Register every function of [`FUNCTIONS`] with `k`, all strict (decision I.2). `stop` is the flag `mc-arith`'s
/// long loops poll, the one the engine raises with the evaluator's interrupt (decision I.1).
pub fn install(k: &mut Kernel, h: &Heap, stop: &Arc<AtomicBool>) {
    let lib = Arc::new(Lib::new(h, stop));
    for &(name, body) in BODIES {
        k.register(h.sym(name), Attributes::default(), Box::new(Fun { lib: lib.clone(), body }));
    }
}
