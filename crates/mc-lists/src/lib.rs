//! MathCAS v2 list and structure library (package I): natives of the evaluator for building, taking apart,
//! reshaping, reducing, selecting and sorting lists and other compound terms, and a few string functions.
//!
//! [`install`] registers every function of [`FUNCTIONS`] with a kernel. The natives work on any compound term where
//! that makes sense (`length(a + b)` is 2, `rest(f(a, b, c))` is `f(b, c)`); a symbol where a compound term is
//! expected leaves the call unevaluated, any other wrong argument gives `failure(argument, f, k)` (argument `k` of
//! `f`), and an index outside a term gives `failure(index, i, n)`.
//!
//! - `construct`: `range`, `array`, `table`, `nest`, `nest_list`, `fold`, `fold_list`, `map_indexed`, `map_thread`,
//!   `scan`;
//! - `predicates`: `is_integer`, `is_rational`, `is_number`, `is_exact`, `is_inexact`, `is_machine_number`,
//!   `is_list`, `is_vector`, `is_matrix`, `is_string`, `is_symbol`, `is_atom`, `all_true`, `any_true`, `none_true`;
//! - `search`: `free_of`, `member_of`, `position` (patterns compiled by the evaluator), `select_first`;
//! - `arrange`: `riffle`, `catenate`, `partition`, `rotate_left`, `rotate_right`, `pad_left`, `pad_right`,
//!   `constant_array`, `thread`;
//! - `group`: `tally`, `gather`, `gather_by`, `unique_by`, `count_by`;
//! - `combinat`: `subsets`, `tuples`, `permutations`, `outer`, `inner`;
//! - `access`: `length`, `part`, `first`, `last`, `rest`, `most`, `take`, `drop`;
//! - `reshape`: `append`, `prepend`, `join`, `reverse`, `flatten`, `transpose`, `apply`;
//! - `reduce`: `sum`, `product`, `total`, `max`, `min`;
//! - `select`: `select`, `cases`, `count` (patterns through `matching`);
//! - `sort`: `sort`, `ordered`, `sort_by`, `ordering`, `max_by`, `min_by`;
//! - `measure`: `depth`, `leaf_count`;
//! - `strings`: `string_join`, `to_string`, `string_length`, `split`.
//!
//! Every loop over elements charges fuel and polls the interrupt (`ctx::Meter`), results are built once (no
//! repeated appends), and nested structure is walked with explicit stacks. Decisions of this package are numbered
//! I.1, I.2, ... (package I) and 1A.1, 1A.2, ... (roadmap package 1A) in the comments where they are applied.
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
mod arrange;
mod combinat;
mod construct;
mod ctx;
mod group;
mod matching;
mod measure;
mod predicates;
mod reduce;
mod reshape;
mod search;
mod select;
mod sort;
mod strings;

use ctx::Lib;
use mc_core::{Heap, Store, T};
use mc_eval::{Attributes, Hold, Kernel, Native, NativeCx, Reply, Res};
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
        "array(f, n): the list [f(1), ..., f(n)]; array(f, [n1, n2, ...]): nested lists of f(i1, i2, ...)";
    "table" => construct::table,
        "table(e, k, r): e for each k in r (a list, or n for 1..n); more pairs k, r nest; table(e, n): n values of e";
    "map_indexed" => construct::map_indexed,
        "map_indexed(f, xs): [f(x1, 1), f(x2, 2), ...]";
    "map_thread" => construct::map_thread,
        "map_thread(f, [xs, ys, ...]): [f(xs[1], ys[1], ...), f(xs[2], ys[2], ...), ...]";
    "scan" => construct::scan,
        "scan(f, xs): f(x) for each element in order, for its effects; the value is null";
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
    "sort_by" => sort::sort_by,
        "sort_by(xs, f): the elements ordered by the canonical order of f(x), stably";
    "ordering" => sort::ordering,
        "ordering(xs): the positions of the elements in sorted order; ordering(xs, n): the first n (last -n)";
    "max_by" => sort::max_by,
        "max_by(xs, f): the first element with the largest f(x) in the canonical order";
    "min_by" => sort::min_by,
        "min_by(xs, f): the first element with the smallest f(x) in the canonical order";
    "is_integer" => predicates::is_integer,
        "is_integer(x): whether x is an integer";
    "is_rational" => predicates::is_rational,
        "is_rational(x): whether x is an exact real number (an integer or a fraction)";
    "is_number" => predicates::is_number,
        "is_number(x): whether x is a number of any kind, complex included";
    "is_exact" => predicates::is_exact,
        "is_exact(x): whether x is an exact number (integer, rational, or complex with exact parts)";
    "is_inexact" => predicates::is_inexact,
        "is_inexact(x): whether x is an inexact number (machine real, ball, or complex with an inexact part)";
    "is_machine_number" => predicates::is_machine_number,
        "is_machine_number(x): whether x is a machine real or a complex number with machine-real parts";
    "is_list" => predicates::is_list,
        "is_list(x): whether x is a list";
    "is_vector" => predicates::is_vector,
        "is_vector(v), is_vector(v, test): whether v is a list without list elements (each passing test)";
    "is_matrix" => predicates::is_matrix,
        "is_matrix(m), is_matrix(m, test): whether m is a nonempty list of equal-length vectors (entries passing test)";
    "is_string" => predicates::is_string,
        "is_string(x): whether x is a string";
    "is_symbol" => predicates::is_symbol,
        "is_symbol(x): whether x is a symbol";
    "is_atom" => predicates::is_atom,
        "is_atom(x): whether x is not a compound term (complex numbers are compound)";
    "all_true" => predicates::all_true,
        "all_true(xs, f): whether f(x) is true for every element";
    "any_true" => predicates::any_true,
        "any_true(xs, f): whether f(x) is true for some element";
    "none_true" => predicates::none_true,
        "none_true(xs, f): whether f(x) is true for no element";
    "free_of" => search::free_of,
        "free_of(t, p): whether no part of t (at any depth, heads included) matches the pattern p";
    "member_of" => search::member_of,
        "member_of(xs, p): whether some element of xs matches the pattern p";
    "position" => search::position,
        "position(t, p): the positions (index lists for part) of the parts of t matching the pattern p";
    "select_first" => search::select_first,
        "select_first(xs, f), select_first(xs, f, d): the first x with f(x) true (else d or failure(not_found, ...))";
    "riffle" => arrange::riffle,
        "riffle(xs, s): s between consecutive elements; riffle(xs, ys): the elements of xs and ys alternating";
    "catenate" => arrange::catenate,
        "catenate(xss): the lists of xss joined into one list";
    "partition" => arrange::partition,
        "partition(xs, n), partition(xs, n, d): the runs of n consecutive elements, starting every d (default n)";
    "rotate_left" => arrange::rotate_left,
        "rotate_left(xs), rotate_left(xs, k): the elements cycled k (default 1) places to the left";
    "rotate_right" => arrange::rotate_right,
        "rotate_right(xs), rotate_right(xs, k): the elements cycled k (default 1) places to the right";
    "pad_left" => arrange::pad_left,
        "pad_left(xs, n), pad_left(xs, n, x): xs made n long by adding x (default 0) at the start or cutting it";
    "pad_right" => arrange::pad_right,
        "pad_right(xs, n), pad_right(xs, n, x): xs made n long by adding x (default 0) at the end, or cut at the end";
    "constant_array" => arrange::constant_array,
        "constant_array(c, n), constant_array(c, [n1, ...]): the list (nested lists) of copies of c";
    "thread" => arrange::thread,
        "thread(f(a, b, ...)): [f(a1, b1, ...), f(a2, b2, ...), ...] over the list arguments, others repeated";
    "tally" => group::tally,
        "tally(xs): the pairs [x, n] of each distinct element and its number of occurrences";
    "gather" => group::gather,
        "gather(xs): the lists of identical elements, in order of first occurrence";
    "gather_by" => group::gather_by,
        "gather_by(xs, f): the lists of elements with identical f(x); gather_by(xs, f, r): each list reduced by r";
    "unique_by" => group::unique_by,
        "unique_by(xs, f): the first element of each distinct f(x), in order";
    "count_by" => group::count_by,
        "count_by(xs, f): the rules k -> n of each key f(x) and its number of elements";
    "subsets" => combinat::subsets,
        "subsets(xs), subsets(xs, n | [n] | [a, b]): the sublists of xs (of at most n, exactly n, a to b elements)";
    "tuples" => combinat::tuples,
        "tuples(xs, n): every list of n elements of xs; tuples([xs1, ...]): every list with its i-th element from xsi";
    "permutations" => combinat::permutations,
        "permutations(xs), permutations(xs, n | [n] | [a, b]): the distinct arrangements of the elements";
    "outer" => combinat::outer,
        "outer(f, xs, ys, ...): the nested lists of f(x, y, ...) for every choice of elements";
    "inner" => combinat::inner,
        "inner(f, a, b), inner(f, a, b, g): the generalised product of vectors or matrices, g (default add) of f's";
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

/// The functions whose arguments are all held; every other one is strict.
const HELD: &[&str] = &["table"];

/// Register every function of [`FUNCTIONS`] with `k`, all strict (decision I.2) except those of `HELD`. `stop` is
/// the flag `mc-arith`'s long loops poll, the one the engine raises with the evaluator's interrupt (decision I.1).
pub fn install(k: &mut Kernel, h: &Heap, stop: &Arc<AtomicBool>) {
    let lib = Arc::new(Lib::new(h, stop));
    for &(name, body) in BODIES {
        // `table` holds its body and iterators (decision 1A.14)
        let hold = if HELD.contains(&name) { Hold::All } else { Hold::None };
        k.register(h.sym(name), Attributes { hold, ..Attributes::default() }, Box::new(Fun { lib: lib.clone(), body }));
    }
}
