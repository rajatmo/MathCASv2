//! MathCAS v2 language library: core-language functions that are written in MathCAS itself
//! (`lib/langlib.mc`) and evaluated by the engine after the prelude, so that they are defined in every session.
//!
//! [`SOURCE`] is that text, [`ORIGIN`] its name in messages, and [`FUNCTIONS`] lists the public functions with a
//! one-line description (the documentation catalogue and the name tables read it). The helper functions the source
//! defines for its own use (`bool_*`, `join_*`, `resample_*`, `threaded_*`, `option_*`, `logical_*`) are ordinary
//! definitions in the bare namespace and not part of the interface.
//!
//! - threading: `threaded`, `elementwise`;
//! - associations and lists: `join_across`, `key_union`, `key_intersection`, `key_member_q`, `key_free_q`,
//!   `disjoint_q`, `intersecting_q`, `compose_list`, `longest_common_subsequence`, `longest_common_sequence`;
//! - Boolean functions: `boolean_function`, `boolean_counting_function`, `boolean_convert`, `boolean_minimize`,
//!   `logical_expand`;
//! - graphs and arrays: `nest_graph`, `array_resample`;
//! - arguments: `arguments_options`, `check_arguments`;
//! - exceptions over `throw` and `catch`: `exception`, `exception_type`, `exception_data`, `exception_tags`,
//!   `is_exception`, `register_exception_type`, `exception_types`, `throw_exception`, `catch_exception`.
//!
//! ```
//! use mc_api::{Engine, StopFlag};
//! let mut e = mc_engine::CoreEngine::new();
//! let stop = StopFlag::new();
//! assert_eq!(e.eval("elementwise(max)([1, -1, 3], 0)", &stop).unwrap().printed, "[1, 0, 3]");
//! ```
//!
//! Decisions are numbered n24c.1, n24c.2, ... in `NOTES.md` and in the comments of the source.
#![warn(missing_docs)]

/// The library's source text.
pub const SOURCE: &str = include_str!("../lib/langlib.mc");

/// The name of the source in messages.
pub const ORIGIN: &str = "lib/langlib.mc";

/// Every public function of the library with a one-line description.
pub const FUNCTIONS: &[(&str, &str)] = &[
    ("threaded", "threaded(v), threaded(v, n): v as an operand that elementwise spreads over the trailing levels of the other arguments (or from level n); inert by itself"),
    ("elementwise", "elementwise(f)(a, b, ...): f applied to the corresponding elements of lists and associations, other arguments used with each element, threaded(v) spread over the levels first"),
    ("join_across", "join_across(left, right, key), (..., type), (..., key_collision_function: f): join two lists of associations (or datasets) on a key; type inner, left, right or outer"),
    ("key_union", "key_union([a1, a2, ...]), key_union(list, f): the associations with every key of any of them, absent keys missing or f(key)"),
    ("key_intersection", "key_intersection([a1, a2, ...]): the associations restricted to the keys all of them have"),
    ("key_member_q", "key_member_q(a, k): whether the association a has the key k"),
    ("key_free_q", "key_free_q(a, k): whether the association a lacks the key k"),
    ("disjoint_q", "disjoint_q(xs, ys): whether two lists have no element in common"),
    ("intersecting_q", "intersecting_q(xs, ys): whether two lists have an element in common"),
    ("compose_list", "compose_list([f1, f2, ...], x): the list x, f1(x), f2(f1(x)), ..."),
    ("longest_common_subsequence", "longest_common_subsequence(a, b): the longest run of consecutive elements (characters) that two lists (strings) share"),
    ("longest_common_sequence", "longest_common_sequence(a, b): a longest list of elements (characters) occurring in both lists (strings) in the same order"),
    ("boolean_function", "boolean_function(k, n), (table), (rules), (spec, [x, ...]): the Boolean function with a given truth table (index k of n variables); f(a, ...) evaluates or restricts it"),
    ("boolean_counting_function", "boolean_counting_function(k, n), ([kmin, kmax], n), ([[k1, ...]], n): the function true when at most k, between kmin and kmax, or exactly k1, ... of n arguments are true"),
    ("boolean_convert", "boolean_convert(e, form), (e, form, cond): e as dnf, cnf, anf, esop, nand, nor, binary_nand, binary_nor, and, or, implies or bff, agreeing with e where cond holds"),
    ("boolean_minimize", "boolean_minimize(e), (e, form), (e, form, cond): the shortest sum of products or product of sums of e, or of the named form"),
    ("logical_expand", "logical_expand(e): the Boolean expression e as a sum of products by distributing and over or"),
    ("nest_graph", "nest_graph(f, start, n): the graph of the values f(v) from start (a vertex or list of them) for n levels, edges from v to each f(v)"),
    ("array_resample", "array_resample(a, n), (a, n, mode), (a, n, mode, [lo, hi]): the array a resampled to n samples by interpolation (linear, nearest or spline), point or bin sampling"),
    ("arguments_options", "arguments_options(f(args, opts...), n), (call, [min, max]): [arguments, options] with the trailing rules split off, or failure(argument_count, ...)"),
    ("check_arguments", "check_arguments(f(args, opts...), n), (call, [min, max]): true if the call has an acceptable number of arguments, else the failure that says why"),
    ("exception", "exception(type), exception(type, data): an exception of a type with an association of data, a plain value that throw_exception can throw"),
    ("exception_type", "exception_type(exc): the type of an exception"),
    ("exception_data", "exception_data(exc): the data association of an exception"),
    ("exception_tags", "exception_tags(exc): the chain of types of an exception: its type, then its registered parent types"),
    ("is_exception", "is_exception(x): whether x is an exception(type, data)"),
    ("register_exception_type", "register_exception_type(type), (type, parent): declare an exception type, a kind of the parent type"),
    ("exception_types", "exception_types(): the registered exception types"),
    ("throw_exception", "throw_exception(exc): throw the exception, tagged by its type, to the nearest catch_exception of that type or of a type it is a kind of"),
    ("catch_exception", "catch_exception(body, type), (body, type, f): the value of body, or the exception of that type (or a kind of it) thrown in it, or f of it"),
];
