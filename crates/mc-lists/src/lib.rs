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
mod levels;
mod matching;
mod measure;
mod predicates;
mod reduce;
mod reshape;
mod search;
mod select;
mod sort;
mod strings;
mod structure;
mod sums;

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
    /// whether a dataset as the first argument declines the call (one of [`DATASET_ROWS`])
    rows_of_datasets: bool,
    /// a pattern argument may be a failure pattern (one of [`PATTERNS`]): only a failure as the first argument
    /// (the data) is the result
    patterns: bool,
}

impl Native for Fun {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        if self.rows_of_datasets && args.first().is_some_and(|&x| self.lib.is_dataset(cx.heap(), x)) {
            return Ok(Reply::Decline);
        }
        if self.patterns {
            if let Some(&x) = args.first().filter(|&&x| self.lib.is_failure(cx.heap(), x)) {
                return Ok(Reply::Done(x));
            }
        }
        (self.body)(&self.lib, cx, t, args)
    }
}

/// The functions a dataset (`dataset(assoc(...))`, package data in `mc-assoc`) gives its own meaning on its rows:
/// with a dataset as the first argument they decline, and the dataset's argument-head rules answer (as `part` does
/// for associations, decision 1A.16). A dataset is one term `dataset(columns)`, so the structural meaning would
/// see a single argument.
pub const DATASET_ROWS: &[&str] =
    &["length", "part", "first", "last", "rest", "most", "take", "drop", "reverse", "join", "select", "sort_by"];

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
        "array(f, n): the list [f(1), ..., f(n)]; array(f, [n1, n2, ...]): nested lists of f(i1, i2, ...); array(f, n, r): indices from r";
    "table" => construct::table,
        "table(e, k, r): e for each k in r (a list, or n for 1..n); more pairs k, r nest; table(e, n): n values of e";
    "subdivide" => construct::subdivide,
        "subdivide(n), subdivide(b, n), subdivide(a, b, n): the n + 1 equally spaced values from a (default 0) to b (default 1)";
    "map_indexed" => construct::map_indexed,
        "map_indexed(f, xs): [f(x1, 1), f(x2, 2), ...]; map_indexed(f, x, levels): f(part, position) for the parts at levels";
    "map_thread" => construct::map_thread,
        "map_thread(f, [xs, ys, ...]): [f(xs[1], ys[1], ...), f(xs[2], ys[2], ...), ...]";
    "scan" => construct::scan,
        "scan(f, xs): f(x) for each element in order, for its effects; the value is null";
    "nest" => construct::nest,
        "nest(f, x, n): f applied n times to x";
    "nest_while" => construct::nest_while,
        "nest_while(f, x, test), nest_while(f, x, test, 1, max): f applied from x while test of the latest value is true (at most max times)";
    "nest_while_list" => construct::nest_while_list,
        "nest_while_list(f, x, test): the list of the values nest_while goes through, x first";
    "fixed_point_list" => construct::fixed_point_list,
        "fixed_point_list(f, x), fixed_point_list(f, x, n): x, f(x), f(f(x)), ... until a value repeats the one before it (at most n steps)";
    "nest_list" => construct::nest_list,
        "nest_list(f, x, n): [x, f(x), ..., f^n(x)]";
    "fold" => construct::fold,
        "fold(f, x, xs): f(...f(f(x, xs[1]), xs[2])..., xs[n]); fold(f, xs) starts from xs[1]";
    "differences" => construct::differences,
        "differences(xs): [x2 - x1, x3 - x2, ...]; differences(xs, n): the n-th differences";
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
        "reverse(xs): the elements in reverse order; reverse(x, n), reverse(x, [n1, ...]): the parts at those levels reversed";
    "unique" => reshape::unique,
        "unique(xs): the elements without repetitions (compared by identity), in order of first occurrence";
    "flatten" => reshape::flatten,
        "flatten(xs), flatten(xs, d): nested terms with the head of xs spliced in (to depth d)";
    "transpose" => reshape::transpose,
        "transpose(m): rows and columns of a rectangular list of lists exchanged";
    "apply" => reshape::apply,
        "apply(f, xs): the head of xs replaced by f; apply(f, x, levels): the heads of the parts at levels (n, [n], [m, n])";
    "sum" => reduce::sum,
        "sum(xs): the canonical sum of the elements; sum(f, [k, a, b]), sum(f, [k, n]): the sum of f for k from a to b (exact for integer bounds; closed forms for polynomial and geometric terms, symbolic or infinite bounds; zeta for k^-s)";
    "piecewise" => sums::piecewise,
        "piecewise([[v1, c1], [v2, c2], ...]), piecewise(cases, default): the value of the first case whose condition is true (default, or 0, when none is); undecided conditions keep the call";
    "product" => reduce::product,
        "product(xs): the canonical product of the elements";
    "total" => reduce::total,
        "total(xs): the sum of the elements (lists added element-wise: column sums of a matrix); total(xs, n): levels 1 to n (inf: all); total(xs, [n]): level n only";
    "max" => reduce::max,
        "max(xs), max(a, b, ...): the largest real number (lists flattened)";
    "min" => reduce::min,
        "min(xs), min(a, b, ...): the smallest real number (lists flattened)";
    "select" => select::select,
        "select(xs, f): the elements x with f(x) true";
    "drop_failures" => select::drop_failures,
        "drop_failures(xs): the elements of xs that neither are nor contain a failure (rows [x, failure(...)] are dropped whole)";
    "cases" => select::cases,
        "cases(xs, p): the elements matching the pattern p; cases(xs, p -> r): r for each match";
    "count" => select::count,
        "count(xs, p): the number of elements matching the pattern p; count(x, p, levels): of the parts at levels";
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
    "match_q" => structure::match_q,
        "match_q(x, p): whether x matches the pattern p";
    "head" => structure::head,
        "head(x): the head of a compound term; for an atom the name of its type (Int, Rat, Float, Complex, Str, Sym)";
    "dimensions" => structure::dimensions,
        "dimensions(x): the lengths of the levels of x that form a full array ([2, 3] for a 2 by 3 matrix)";
    "array_depth" => structure::array_depth,
        "array_depth(x): the number of levels of x that form a full array";
    "extract" => structure::extract,
        "extract(x, pos): the part at a position [i, j, ...]; extract(x, [pos1, ...]): the list of parts at several";
    "replace_part" => structure::replace_part,
        "replace_part(x, pos -> v), replace_part(x, [pos1 -> v1, ...]): x with the parts at the positions replaced";
    "delete" => structure::delete,
        "delete(x, pos), delete(x, [pos1, ...]): x without the parts at the positions";
    "map_at" => structure::map_at,
        "map_at(f, x, pos), map_at(f, x, [pos1, ...]): x with each part p at the positions replaced by f(p)";
    "level" => structure::level,
        "level(x, levels): the list of the parts of x at the levels (n, [n], [m, n]; negative from the leaves)";
    "replace" => structure::replace,
        "replace(x, rules): the first rule matching the whole of x applied; replace(x, rules, levels): applied to the parts at the levels";
    "delete_cases" => structure::delete_cases,
        "delete_cases(xs, p): the elements not matching p; delete_cases(x, p, levels): matching parts at the levels removed";
    "first_case" => structure::first_case,
        "first_case(xs, p), first_case(xs, p, default): the first element matching p (p -> r: its r), else default or failure(not_found, p)";
    "reverse_sort" => structure::reverse_sort,
        "reverse_sort(xs): the elements in reverse canonical order";
    "take_largest" => structure::take_largest,
        "take_largest(xs, n): the n largest elements, largest first";
    "take_smallest" => structure::take_smallest,
        "take_smallest(xs, n): the n smallest elements, smallest first";
    "sequence_cases" => structure::sequence_cases,
        "sequence_cases(xs, p): the runs of consecutive elements matching the list pattern p ([a, b], [x_, x_]); p -> r gives each run's r";
    "sequence_position" => structure::sequence_position,
        "sequence_position(xs, p): the [start, end] positions of the runs matching the list pattern p";
    "sequence_count" => structure::sequence_count,
        "sequence_count(xs, p): the number of non-overlapping runs matching the list pattern p";
    "sequence_replace" => structure::sequence_replace,
        "sequence_replace(xs, p -> r): each run matching the list pattern p replaced by one element r";
    "sequence_split" => structure::sequence_split,
        "sequence_split(xs, p): the pieces of xs between the runs matching the list pattern p";
    "full_form" => structure::display_wrapper,
        "full_form(x): x shown with every part written as a plain call, add(x, pow(y, 2))";
    "input_form" => structure::display_wrapper,
        "input_form(x): x shown in the input syntax (a wrapper the printer removes)";
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
        "pad_left(xs, n), pad_left(xs, n, x): xs made n long by adding x (default 0; a list repeats cyclically) at the start or cutting it; n may list lengths per level; pad_left(xss) pads rows to a matrix";
    "pad_right" => arrange::pad_right,
        "pad_right(xs, n), pad_right(xs, n, x): xs made n long by adding x (default 0; a list repeats cyclically) at the end, or cut at the end; n may list lengths per level; pad_right(xss) pads rows to a matrix";
    "constant_array" => arrange::constant_array,
        "constant_array(c, n), constant_array(c, [n1, ...]): the list (nested lists) of copies of c";
    "thread" => arrange::thread,
        "thread(f(a, b, ...)): [f(a1, b1, ...), f(a2, b2, ...), ...] over the list arguments, others repeated; thread(x, g): over the arguments with head g";
    "tally" => group::tally,
        "tally(xs): the pairs [x, n] of each distinct element and its number of occurrences";
    "gather" => group::gather,
        "gather(xs): the lists of identical elements, in order of first occurrence";
    "gather_by" => group::gather_by,
        "gather_by(xs, f): the lists of elements with identical f(x); gather_by(xs, f, r): each list reduced by r";
    "unique_by" => group::unique_by,
        "unique_by(xs, f): the first element of each distinct f(x), in order";
    "count_by" => group::count_by,
        "count_by(xs, f): the association of each key f(x) with its number of elements";
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
        "split(s), split(s, sep): the words of s, or the pieces between occurrences of sep; split(xs), split(xs, test): the runs of identical neighbours (or neighbours passing test(a, b)) of a list";
}

/// The functions whose arguments are all held; every other one is strict.
const HELD: &[&str] = &["table", "piecewise"];

/// The functions taking a pattern after the data: the pattern may be a failure pattern (`failure(___)`), so a
/// failure among the arguments is not their result, except as the data (decision p6core.21).
const PATTERNS: &[&str] = &[
    "cases",
    "count",
    "position",
    "free_of",
    "member_of",
    "delete_cases",
    "first_case",
    "sequence_cases",
    "sequence_position",
    "sequence_count",
    "sequence_replace",
    "sequence_split",
];

/// Register every function of [`FUNCTIONS`] with `k`, all strict (decision I.2) except those of `HELD`. `stop` is
/// the flag `mc-arith`'s long loops poll, the one the engine raises with the evaluator's interrupt (decision I.1).
pub fn install(k: &mut Kernel, h: &Heap, stop: &Arc<AtomicBool>) {
    let lib = Arc::new(Lib::new(h, stop));
    for &(name, body) in BODIES {
        // `table` holds its body and iterators (decision 1A.14)
        let hold = if HELD.contains(&name) { Hold::All } else { Hold::None };
        let rows_of_datasets = DATASET_ROWS.contains(&name);
        let patterns = PATTERNS.contains(&name) || name == "match_q";
        let f = Fun { lib: lib.clone(), body, rows_of_datasets, patterns: patterns && name != "match_q" };
        k.register(h.sym(name), Attributes { hold, handles_failures: patterns, ..Attributes::default() }, Box::new(f));
    }
    // `map` with levels, over associations and other compound terms (the evaluator's `map` keeps lists)
    let more = lib.clone();
    k.wrap_native(h.sym("map"), move |inner| Box::new(levels::MapMore { inner, lib: more }));
}
