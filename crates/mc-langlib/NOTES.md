# `mc-langlib` (the language library)

A new crate holding core-language functions that are written in
MathCAS itself (`lib/langlib.mc`, embedded as `SOURCE`) and evaluated by the engine right after the prelude. Outside the
crate there are only registrations: the dependency line and the `FUNCTIONS` list in `mc-engine` (and the call that
evaluates `SOURCE` after the prelude), the catalogue entry in `mc-docs`, the list in `mc-lsp`, the help index table, and
30 pages in `docs/functions/`.

## Why source and not natives

The functions are compositions of existing natives (lists, associations, truth tables, `throw`/`catch`); writing them
in the language keeps them readable, lets the pages show real definitions, and costs no evaluator API. Helpers
(`bool_*`, `join_*`, `resample_*`, `threaded_*`, `option_*`, `logical_*`, `assocs_of`, `exception_chain`, `position_of`)
live in the bare namespace like the prelude's; they are not part of the interface. Two pitfalls found on the way and
worth knowing for any library written this way: a local named `same`, `index` or `tuple` shadows the function the
reader lowers `===` and friends to (n24c.1); and a `fn` is closed (an unmatched call is `failure(no_match, ...)`), so
overloads that must stay inert for other arguments are written as `rule`s.

## Decisions

- **n24c.1 `elementwise(f)(a, b, ...)`** is a rule on the operator form `elementwise(f)`. Lists are paired element by
  element at every level (listable semantics, ragged lists as far as they go), associations by key, other arguments
  broadcast; unequal lengths give `failure(shape_mismatch)`, as for listable functions (semantics 7.2). Tested on
  16 cases (including `elementwise(f)()` = `f()`, `[]` -> `[]`).
- **n24c.2 `threaded(v, n)`**: inert wrapper; the alignment rules (trailing levels by default, level `n` from the top
  for `n > 0`, level `r + n + 1` for `n < 0`, errors for too deep / non-fitting) were worked out on arrays of
  rank 3 and 4. Understood by `elementwise`; a threaded operand of `add`/`mul`/a user listable
  function needs the evaluator (see Requests).
- **n24c.3 `join_across`** on lists of associations: inner/left/right/outer (strings in any case, or symbols),
  keys as `k`, `lk -> rk`, `key(k)`, or lists; hash index on the right-hand key tuples (linear, not quadratic);
  absent side `missing("Unmatched")`; collisions keep the left value, `key_collision_function` is `left`, `right`
  or `f(key) = [kl, kr]`. 13 test cases, including key collisions. Datasets go to
  `join_on` with `how:`.
- **n24c.4 Boolean functions.** Truth tables are indexed by the assignment read as a binary number (false 0, first
  variable highest): `boolean_function(k, n)` has bit `i` of `k` as the value at index `i`; tested on
  30 tables of 3 variables (`[false, true]` is `1` of 1 variable). The function is `boolean_function(k, n)`, the
  integer being the canonical form (any function is one pair), not an opaque decision diagram. Calls restrict
  (`f(true, x, false)` is the function of `x`).
  `boolean_counting_function`: `k` means *at most* k, `[kmin, kmax]` a range,
  `[[k1, ...]]` exact counts.
- **n24c.5 `boolean_convert`** works on the truth table (at most 14 variables): Quine-McCluskey prime implicants
  (hash-based pairing, O(m n) per round), essential primes, then greedy cover (most newly covered, then fewest literals).
  Not guaranteed minimum (Petrick's method is not used). A condition makes the rows where it fails don't-cares.
  CNF is the dual (the same procedure on the zeros). ANF by the Moebius transform over GF(2). NAND/NOR forms from the SOP/POS;
  binary NAND/NOR by the standard gate identities; "and"/"or"/"implies" forms use De Morgan over the same covers.
  If-then-else and decision-tree forms are not provided. Literature: McCluskey 1956, "Minimization of Boolean functions";
  Reed-Muller / Moebius transform for the ANF.
- **n24c.6 `logical_expand`** distributes `and` over `or` on the NNF, drops contradictory terms and absorbed terms,
  treats any other subexpression (equations, comparisons) as an atom.
- **n24c.7 `nest_graph`** is breadth-first with level counting: the last level is not expanded, default one level,
  `inf` to closure; values may be one vertex or a list; edges listed once; options pass to
  `graph` except `directed_edges`. Tested on 8 edge/vertex lists.
- **n24c.8 `array_resample`**: exact rational positions, separable multilinear interpolation (array arithmetic on
  whole sub-arrays, so it works for any rank and for symbolic data), "point" and "bin" sampling, a position range,
  `scaled`, per-axis sizes, nearest / nearest_left / nearest_right (a tie averages), padding fixed/periodic/reflected/number.
  Tested on 20 cases (rationals included, e.g. `[1, 5/3, 7/3, 3]`). `"cubic"` and
  `"quadratic"` are cardinal B-spline interpolation (Unser, Aldroubi, Eden 1993: the recursive filter of the pole
  `-2 + sqrt(3)`, `-3 + 2 sqrt(2)` on the data continued by the padding over 30 extra elements per side, error below
  1e-16): machine reals, for fixed, constant, periodic and reflected padding and in two dimensions; exact data gives
  floats, not exact algebraic numbers. Not done:
  machine-real output type for exact entries mixed with machine reals.
- **n24c.9 `arguments_options` / `check_arguments`** do not know the options a function declares (v2 has no
  table of declared options to consult), so any trailing rule is an option; option names are not validated. A rule needed to reach the minimum count is an argument. `check_arguments` returns
  `true` or the failure (v2 prints no messages).
- **n24c.10 Exceptions.** An exception model with property access, a global hierarchy and printed messages would
  contradict semantics 10 (errors are values `failure(tag, details...)` that propagate through strict arguments, no
  printed messages) and the way `failure` is already used by every library function. Here:
  `exception(type, data)` is a plain value (so it can be passed to functions, which a `failure` cannot),
  `register_exception_type(type, parent)` keeps a parent table, `throw_exception` is `throw(exc, type)` and
  `catch_exception(body, type, f)` is `catch` with a pattern of the type and its registered descendants. No validation
  bookkeeping, property access or message templates.
- **n24c.11 small functions**: `key_union`, `key_intersection`, `key_member_q`, `key_free_q`, `disjoint_q`,
  `intersecting_q`, `compose_list`, `longest_common_subsequence` (consecutive elements) and
  `longest_common_sequence`.

## Properties (evidence: tests/functions.rs, docs pages)

- `join_across` and `key_union` run on hash indexes; `array_resample` and `boolean_convert` work for any rank / any
  number of variables up to the stated limits, exactly (rationals, symbols) for exact data in the linear modes.
- `boolean_convert` and `boolean_minimize` honour a condition with true don't-cares and never fail silently on an
  expression that is not Boolean (`failure(argument, boolean_convert, 1)`).
- Errors are values with a tag the caller can match, not messages.

## Gaps

- `threaded` operands of `add`, `mul`, `pow` and user listable functions (evaluator: `Evaluator::thread` must call a
  threaded-aware path); `elementwise(add)` is the workaround.
- Exact (algebraic) output of the spline resampling; if-then-else and decision-tree forms; `boolean_function` from a function value (arity unknown);
  `boolean_function` rules with sequence patterns (`[false, __]`).
- Option-name validation against declared options; mode and head settings of `arguments_options`.
- Skipped on purpose: `splice`, `nothing`, `speak`, `dynamic_module`, `tabular`, `create_data_structure`,
  `axiomatic_theory`, `function_compile`, assignments of option defaults, message tags, `?f`, and everything needing
  evaluator internals (`names`, `down_values`, `up_values`, `own_values`, `check_abort`, `abort`, `return`, `break`).
  `heads` and `subset_count` are not provided.

## Changes needed in other crates

- **mc-eval** (`Evaluator::thread`, the listable threading of `add`, `mul`, `pow`, user listable heads): when an
  argument is `threaded(v)` or `threaded(v, n)`, spread it over the levels of the array arguments as `elementwise`
  does (`threaded_window`/`threaded_tile` in `lib/langlib.mc` give the exact rules: trailing levels by default,
  level `n` from the top for `n > 0`, `r + n + 1` for `n < 0`, `failure(shape_mismatch)` / `failure(threaded_level, n)`
  on misfit, a threaded result when every argument is threaded or a scalar). Failing tests to write there:
  `[[1, 2], [3, 4], [5, 6]] + threaded([x, y])` is `[[1 + x, 2 + y], [3 + x, 4 + y], [5 + x, 6 + y]]`;
  `2*threaded([-1, 1])` is `threaded([-2, 2])`; with `set_attributes(f, listable)`,
  `f(threaded([a, b]), [[1, 2], [3, 4]])` is `[[f(a, 1), f(b, 2)], [f(a, 3), f(b, 4)]]`.
- **mc-eval** (reader): a local name that equals a function the reader lowers an operator to (`same` for `===`, and
  the like) silently breaks `===` inside the scope; a read-time warning for `let same = ...` would have saved an hour.
- **mc-eval** (`let`): a `let x = <failure>` makes the whole block return the failure at once, so a function cannot
  inspect a failure it computed in a `let` (use `is_failure(f(...))` directly in an expression).

Note on Boolean forms: the covers of `boolean_convert` and `boolean_minimize` are not guaranteed minimal and ties are
broken greedily, so equivalent inputs can give forms that agree as expressions but not as text.

## Round 25

- The requests on `Evaluator::thread` are done (R24-05, see mc-eval NOTES): `add`, `mul` and user listable heads
  spread `threaded` operands; tests in `tests/functions.rs`. The `let x = <failure>` request (R24-07) is deferred
  there with the reason.

## Round 27

- B26-12: `boolean_counting_function([k], n)` is "exactly k" (a one-element list of a number), like `[[k1, ...]]`.
- X26-18: `longest_common_subsequence` keeps, per row, only the positions where the elements are equal (positions of each
  value from `group_by`): 60 ms for two lists of 1500 (was 3.4 s, 1.8 GB). `longest_common_sequence` is Hunt and
  Szymanski, "A fast algorithm for computing longest common subsequences", CACM 20 (1977): thresholds per length with
  a binary search, no n x m table (155 ms for 1500; the table of the old one ran out of memory); it runs on the reversed
  lists and reverses the answer, which keeps the answers the old traceback gave on ties.
- D26-19: `exception(type)` is a rule, not a `fn`: a call of a `fn`-defined function whose arguments are all numbers or
  strings and no definition accepts gives `failure(no_match)` instead of staying inert, so `exception("s")` (which became
  `exception("s", assoc())`) failed. `exception()` now stays as it is.
- D26-20: key lists are compared with `!==`; `!=` of two lists of symbols stays unevaluated, so differing keys passed.

## Round 33

- `arguments_options` and `check_arguments` read a held call `apply(g, [a, b, ...])` as `g(a, b, ...)`: a rule with
  `args___` writes the call that way (v2 has no splicing), and evaluating it would run `g` again inside its own
  condition (`check_arguments` returned null instead of leaving `f(2)` alone).
- By design: `check_arguments` does not know the options a function declares (options are `name:` arguments in v2), so
  an unknown option name passes the check.

## Round 36

- `logical_expand` of an equation whose left side is a series at 0: the conjunction of the coefficient equations.
