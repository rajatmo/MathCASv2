# Item C: `mc-syntax`

Status: built. A 256-statement corpus plus a multi-line program
file round-trip; error tests check positions and recovery; fuzzing never panics. No build or clippy warnings. About
4,800 lines of source plus 960 of tests; depends only on `mc-api` (`num-bigint` as an independent check in tests).

## What was built

- **Lexer** (`lexer.rs`, `ident.rs`): comments; identifiers with Unicode letters; qualified names `M.f`; the aliases
  `π` and `∞`; keywords; pattern tokens `x_`, `xs__`, `___`; numbers with `_` separators, `0x`/`0b`, reals, `~` balls,
  the `im` suffix; strings with `\n \t \" \\ \u{..}`; primes; slots `&k`. Every token carries a byte span; new lines
  are significant except inside `()` and `[]`.
- **Parser** (`parser.rs`): precedence climbing over 04 §3.1 (`prec.rs`). Non-chaining comparisons; numeric-literal
  juxtaposition only; calls, named arguments, lists, indexing and spans; pipes with `_` holes (a pipe at the start of a
  line continues the expression); lambdas and slot functions; blocks with `let`/`var`; `if`/`while`/`for`/`match`;
  patterns, types, `fn`/`rule`/`sym` with decorators, `held` parameters and options after `;`; modules, `pub`, imports
  (`M.{..}`, `except`). Errors are `line:col: message`; recovery skips to the next statement (new line, `;`, or `}`).
- **Lowering** (`lower.rs`, `scope.rs`, `nat.rs`): AST to terms through any `Store`; big integers and rationals built
  exactly with simple limb arithmetic.
- **Printer** (`print.rs`): an explicit task stack (any depth; tested at 100,000 levels on a 1 MB stack); minimal
  parentheses; recovers `a - b`, `a/b`, `-a`; named arguments, primes up to order 4, string escapes, backquoted names
  where needed; numbers through a pluggable `NumberFormat`. Faithful: except balls and non-finite reals, printed text
  reads back to the identical term.

## Public API

- `parse(&str) -> Parsed { program, errors }`
- `lower_program(&S, &Program) -> Result<Vec<T>, Vec<Diagnostic>>`
- `read(&S, &str) -> Result<Vec<T>, Vec<Diagnostic>>` (parse and lower)
- `Printer::new(&S)` / `Printer::with_numbers(&S, Box<dyn NumberFormat>)` with `.expr(t)`, `.statement(t)`,
  `.program(&[t])`
- `NumberFormat` (`big_int`, `ball`, `non_finite`) and `DefaultNumberFormat`
- `Head` (every head the syntax uses, with its documented shape and `name()`)
- `Diagnostic` (`render(src)`, `position(src)`), `Span`, `LineIndex`, `LineCol`, `render_all`, `MAX_DEPTH`, `ast`

## Tests

| File | Covers |
|---|---|
| `tests/corpus.rs` | 256 statements in `tests/corpus/statements.mc` (each, and the file as one program) and `tests/corpus/programs.mc` (04's tour, multi-line forms): read, print, read back to identical terms, print identically again. |
| `tests/lowering.rs` | About 120 exact term shapes. |
| `tests/errors.rs` | 33 error cases with exact `line:col` (including Unicode columns); recovery across statements and inside blocks; 12 kinds of hostile nesting rejected cleanly on a 1 MB stack; depth just under the limit round-trips; a 100,000-deep term printed on a 1 MB stack. |
| `tests/fuzz.rs` | Fixed seed (override with `MC_FUZZ_SEED`, `MC_FUZZ_SCALE`): random token streams and raw text never panic, spans stay in range, anything that reads must round-trip; random terms biased to every special form and near misses read back identically. |
| unit tests | lexer, line/column, quoting, limb arithmetic against `num-bigint` |

The default suite runs in about 4 s; six extra campaigns at 8× scale (about 300k cases each, under the guard) are
clean. The term fuzzer found four printer bugs, all fixed: nested union types flattened; a pattern token before the `:`
of an index span read back as a typed pattern; a typed pattern followed by `|` lost the `|` to its type; a type ending
in a parenthesised group followed by `->` read back as a function type.

## Measurements (busy machine, release, `RefStore`)

- `read` about 7.8 MB/s (780 KB, 51,200 statements, 103 ms); printing that output 42 ms.
- A tiny `read` costs about 74 µs per call, mostly starting the reserved-stack thread (C.13).

## Decisions

- **C.1 Chains are n-ary:** `a + b - c` → `add(a, b, mul(-1, c))`; `a*b/c` → `mul(a, b, pow(c, -1))`; parentheses keep
  grouping (`(a+b)+c` → `add(add(a, b), c)`).
- **C.2 Literal folding:** a minus directly on a number token gives the negative literal (`a - 7` → `add(a, -7)`);
  `p/q` with integer literals (q ≠ 0) is the exact reduced rational or integer (`6/4` → `3/2`, `4/2` → `2`); otherwise
  04's general forms (`5/0` stays `mul(5, pow(0, -1))`); the printer avoids ambiguity (`mul(3, pow(4, -1))` prints
  `3*4^-1`).
- **C.3 Juxtaposition** (`2x`, `3(a+b)`) needs no space; numbers take no postfix. Since round 35 a space between
  operands multiplies too (`2 x`, `a b`; see that section).
- **C.4 Backquoted names** (`` `if` ``, `` `a b` ``), an addition to 04, make the printer total: keyword heads print as
  `` `if`(c) `` when the term is not in the `if` shape.
- **C.5 Slot functions:** parameters are the symbols `&1..&n` (n the highest slot used); nested slot functions bind
  their own; no capture of user names.
- **C.6 Ball literals** lower to `ball_literal("1.25", 40)` (sign inside the string), because exact decimal-to-ball
  conversion needs `mc-arith`; `3im` lowers to `complex(0, 3)`.
- **C.7 Blocks** lower to `block(s1, …)` with statements `let(x, v)`, `let(typed(x, T), v)`, `var(x, v)` and
  expressions (one flat shape rather than 04's nested `let(x = a, e)`).
- **C.8 Assignment** is resolved lexically: `x = v` is `assign(x, v)` when `x` is a visible `var`, an error when bound
  by `let`, a parameter or a pattern, otherwise `set(x, v)`; `:=` is always `set_delayed`. Lowering and printer share
  `scope.rs`.
- **C.9 Definition terms:** `fn f(ps; opts): R when c = b` → `define_fn(lhs, b, list(opts), R | null, list(decs))`,
  with `lhs` = `f(patterns)` wrapped in `condition(…, c)`; a parameter `x` is `any(x)`, `x: T` is `typed(any(x), T)`,
  `x...` is `seq0(x)`, a default is `optional(p, d)`, `held` is `held(p)`. `rule L := R` → `define_rule(rule_delayed(L,
  R), list(decs))` (`=` gives `rule(L, R)`). A trailing `when c` moves to `condition(L, c)`. `sym s` → `define_sym(s,
  list(decs))`; decorators are symbols or calls such as `identity(0)`.
- **C.10 Other heads** (all on `Head`): patterns `any`, `seq1`, `seq0`, `typed`, `named`, `optional`, `condition`,
  `alternatives`; lambda parameters `rest(x)` (and `held`, `typed`, `optional`), a lambda is `fn(list(params), body)`;
  types `fun_type`, `type_union`, `type_where` (type names as written: `Int`, `List(T)`); modules `module(M, list)`,
  `public(s)`, `import(M[, list])`, `import_except(M, list)`; `M.f` is the symbol named `"M.f"`; comparisons `equal`,
  `unequal`, `less`, `less_equal`, `greater`, `greater_equal`, `same`, `not_same`; `part`, `span`, `options`,
  `derivative`, `if`, `while`, `for`, `match`.
- **C.11 Pipes:** `a |> f(b)` with no hole is `f(b)(a)`; more than one `_` hole is an error.
- **C.12** `p = d` directly as a call argument with a pattern `p` lowers to `optional(p, d)`.
- **C.13 Bounded recursion:** parsing and lowering recurse per nesting level, capped at `MAX_DEPTH` = 256, on a scoped
  thread with a 64 MB reserved stack (`stack.rs`); the printer is iterative.
- **C.14 Smaller choices:** match arms may be separated by new lines; `x @ p` takes a postfix-level right side; a typed
  pattern's type takes a following `|` (so `(x_: Int) | (y_: Real)` needs parentheses); `where` in types is contextual.

## Suggestions for integration

- `mc_api::names` could adopt the head names of C.9 and C.10, so `mc-eval` depends on constants rather than strings.
- A ball-from-decimal helper in `mc-arith` would let `ball_literal` be folded.

## Known gaps

- Balls and non-finite machine reals do not round-trip: the default formatter prints `ball_payload(neg, [words])` and
  `machine_real("inf")`; `mc-arith` must supply decimal ball text, and integration decides whether reading `1.25~40`
  back builds the ball.
- Name resolution through modules and imports (semantics 1.2) is not done: bare names lower to bare symbols, `M.f` to
  a qualified symbol.
- No static checks: type mismatches (04 §9), unknown options, patterns in lambda bodies, variables of different
  alternatives.
- Text nested deeper than `MAX_DEPTH` (which the printer can produce from deep terms) cannot be read back.
- Each `read` or `parse` call starts a thread (C.13).

## What integration needs from this crate

- `mc-eval` interprets the shapes of C.7 to C.12 (documented on `Head`): `define_fn`, `define_rule`, `define_sym`,
  `set`/`set_delayed`/`assign`, `block`/`let`/`var`, `fn` closures with the parameter wrappers, the pattern heads; and
  turns `ball_literal(text, digits)` into balls.
- `mc-arith` implements `NumberFormat` (fast decimal integers; balls with only correct digits).
- The REPL and the benchmark runner call `read(store, src)`, show `Diagnostic::render(src)`, and print with
  `Printer::program`.

## Item 9 (library and kernel gaps)

- **Series printing (9.3):** `series_data(x, x0, [c...], v, p)` (integer v, p; at most p - v coefficients) prints as a
  sum plus an order term: `1 + x + x^2/2 + O(x^3)`, `x^-1 - x/6 + O(x^4)`, `exp(1) + exp(1)*(x - 1) + O((x - 1)^2)`,
  `O(x^3)`; rational coefficients `n*x^k/d` (or `n/(d*x^m)`), negatives with ` - `; other shapes print as calls. The text
  reads back as that sum (`series(text, x, x0, n)` gives the same `series_data`). Private `Run::series`,
  `series_term`, `series_power`; re-export `string_literal`; truncated series named as a faithfulness exception.
  Tests `tests/series.rs` (18 printing, 3 read-back).

## Editor support (notebook round, item P7)

- **Unicode operators** (one token each, same as the ASCII spelling; the printer still writes ASCII): `→` `->`,
  `⧴` `:>`, `≤ ≥ ≠ ⩵` `<= >= != ==`, `≡ ≢` `=== !==`, `× · ⋅` `*`, `÷` `/`, `−` `-`, `∧ ∨ ¬` `and or not`, `∈` `in`;
  `√` prefix square root binding like a call (`√x^2` is `(√x)^2`, `2√2` is `2*sqrt(2)`); `ⅇ` = `e`, `ⅈ` = `im`
  (`3ⅈ` an imaginary literal). Meaningless symbols (`∀ ± ∫ ∑ ∂ ↔`) give a clear lexical error (`°`: angles are in
  radians). Public table `UNICODE_OPERATORS`, `unicode_operator(c)`.
- **Placeholder** `□` (`PLACEHOLDER`) is a token; the parser reports the first unfilled placeholder per statement
  without cascading errors.
- **Colouring** (`src/colour.rs`): `classify(src)`, `colour(src, defined) -> Colouring { tokens, errors, incomplete,
  lexical_only, pairs }` with `class_at`, `matching_bracket`, `enclosing_brackets`, `pair_at`; `TokenClass::{Operator,
  Global, Undefined, Local, PatternVar, Shadowing, Keyword, Constant, Number, String, Comment, Placeholder, SyntaxError,
  UnmatchedBracket}`. Scopes computed from tokens and the bracket tree (stable while typing): `let`/`var` to the end
  of the block, `fn` parameters, lambdas, `for`, pattern names on rule left sides; names defined in the cell count as
  defined. Errors at the end of input set `incomplete` only (`SHOW_ERRORS_AT_END = false`); above
  `LEXICAL_ONLY_ABOVE = 100_000` bytes lexical colouring only. Every call is total, run on the reserved-stack thread.
- `names_in_scope(src, offset)` (locals for completion), `extend_selection(src, start, end)` (all the section 3.4
  vectors; `None` when the whole content is selected).
- Docs: `docs/design/04-syntax.md` Unicode operator table, placeholder, aliases, section 14 "Editor support".
- Tests: `tests/colour.rs` (11, incl. every-prefix stability, 35 hostile inputs, linear-time checks, snapshot
  `tests/corpus/colour.snap`, re-bless with `MC_BLESS=1`), `tests/select.rs` (2), +1 lowering (23 Unicode spellings),
  +1 errors, fuzz generators with Unicode tokens.
- Measurements (release): `colour` 0.14 ms for 10 lines, 0.77 ms for 100, 6.5 ms for 1,000 (53 KB), 29 ms for 10,000
  lines lexical-only; linear but not incremental (debounce large cells).
- Gaps: not incremental; `names_in_scope`/`extend_selection` have no lexical-only cutoff; signature-based classes
  (M7) not done; rare scope heuristics (a `for` iterable containing a brace block; `f(x_) when x > 0` without a rule
  operator leaves `x` global; bare-name `match` arm patterns coloured as lambda binders); the VS Code grammar does
  not know the Unicode operators.

## Round 10: raw strings

`r"..."` (no escapes) and `r#"..."#` with any number of `#` (ends at the first `"` followed by as many `#`); one
`Str` token, coloured `String`; ends on its line ("unterminated raw string", incomplete at the end of a cell). `r`
followed by anything else is still the name `r`. The printer writes a TeX string (a `$` and a backslash, no
control character) raw, with the fewest `#` (`string_token`); paths keep their escapes because front ends read
printed `image("...")` values; `string_literal` (escaped) is unchanged for other crates.

## Round 20: display printing, warnings

- **Display of negative balls** (P19-06): a ball whose text is a number is a literal for the printer
  (`literal`), so a negative one joins a sum as a difference, `0.3465... - 1.5707...im`, `y - 3.14159*x` (before:
  `+ -1.5707...im`); `negated_number` knows negative balls and imaginary coefficients, so `a - im*pi` and `a -
  2im*x` print as differences too. The default number format (`ball_payload(...)`) is not a number token and
  prints as before.
- **The unit** (display only): `complex(0, 1)` and `complex(0, -1)` print `im` and `-im` (`im*pi`, `x == -im`),
  not `1im`. The exact form keeps the literal `1im`: it is the atom itself, where `im` reads as the unit's name.
  A machine zero real part of a complex term in a sum is left out (`x - 2.5*im` holds `complex(-0.0, -2.5)`:
  `x - 2.5im`, not `-0.0 - 2.5im + x`).
- **Calls that read like products**: `x(x - a)` is a call (the language does not change). As a power's base such
  a call (a plain name that is also an operand of its own arguments) keeps its brackets: `(x(x - a))^(1/2)`, not
  `x(x - a)^(1/2)`, which a reader takes for x times a root; `f(x)^2` and `y(t)^2` are unchanged (the head is not
  among the arguments). Both forms read back as the same term.
- **`lint_calls`** (`lint.rs`, shared by the notebook, the language server and the kernel): a call whose head is a
  plain name with no definition (the caller's `known`, plus the text's own `fn`, `rule`, `f(x) := ...`, `sym`,
  `let`, `x = ...`), not bound in the statement (lambda and `fn` parameters, `let`, `for`, patterns), and used as
  a bare operand in the same statement, gets "x is called as a function; did you mean x*(x - a)?" with the fix
  "insert `*` after the name". `colour_lint` gives the colouring with its `warnings` from the same parse.
- **`Colouring::shifted`**: a colouring moved over an edit (tokens, pairs, errors and warnings before it kept,
  after it moved, touching it dropped; a token the edit lies inside resized). The notebook shows it while typing
  in a long cell and colours the cell again when typing pauses.
- Timings (`tests/long_text.rs`, ignored, 92 KB, release): colour 7.1 ms, lint 6.9 ms of which parse 5.4 ms; with
  `colour_lint` the lint adds about 1.5 ms to the colouring.
- Tests: `lint.rs` unit tests (flagged: integration, iterator, solve variables, own operand, a space before the
  bracket; not flagged: library functions, definitions in the text, lambda and `fn` parameters, other statements,
  rule heads, products), `tests/colour.rs` (shifted colouring agrees with a fresh one), `mc-kernel/tests/display.rs`.

## Round 21

- `tests/bugs_round21.rs`: three ignored failing tests for B21-7, the call-of-a-variable warning on the documented
  ODE and recurrence notation (`y'(x)` marks `y` as a bare operand; a name passed as an argument and also called).
  Several documentation examples warn.

## Round 25

- **R24-06 read-time warning:** `lint_program` also reports a `let`/`var` or parameter named like a function the reader
  lowers syntax to (`same`, `not_same`, `index`, `tuple`, `part`, `list`, comparisons, `rule`, `options`, ...: `LOWERED`
  in `lint.rs`); the editors show it with the other lints (the fix inserts nothing). The engine's own `eval` does not
  lint (as before).

## Round 32: `∈` and `°` (design 08 section 3.9)

- `∈` lexes as its own token (`lexer::ELEMENT`): between operands it reads as `element(x, s)` at comparison level
  (no chaining); after a `for` loop variable it is still the loop's `in`.
- `°` directly after a name, number, closing bracket or prime is the degree: the tokens of `*(pi/180)`, so
  `sin(30°)` is `sin(30*(pi/180))` and the alias `deg` gives working input. Elsewhere it is an error saying where it
  may stand. Tests: `tests/lowering.rs` `element_and_degree_characters`, `tests/errors.rs`.

## Round 33

- Display mode (`Printer::with_display`) writes a sum of monomials in one symbol by ascending power (`1 + x + x^2/2 +
  x^3/6 + x^4/24`): decision 13.3 makes the printing order independent of the canonical order. Plain text keeps the
  term's order: printed text must read back as the same term, and the reader keeps the order it is given
  (`tests/series.rs` `r33_display_orders_a_polynomial_sum_by_power_and_plain_text_keeps_the_term_order`; the round-trip
  corpus tests fail when the plain printer sorts).

## Round 35: a space multiplies

User decision 2026-11-11 (design 04 section 3.1, which records it): two operands next to each other are a product
at the level of `*`, left associative.

- **Parser** (`parser.rs`): when no infix operator follows an operand and the next token can start one
  (`at_juxtaposed_operand`: a name other than `where`/`except`, a number, a pattern, a slot, `(`, `[`, `√`, `□`),
  `expr_inner` takes it as a factor of a `Mul` chain (`MulOp::Times`), at `prec::MUL` with operands at `MUL + 1`,
  so `m c^2` is `m*(c^2)`, `a/b c` is `(a/b)*c`, `-x y` is `-x*y`, `a^b c` is `(a^b)*c`. A string never multiplies
  (on either side): `"a" "b"` and `x "b"` keep the end-of-statement error with the hint "strings do not multiply".
  In `postfix_expr` a `(` or `[` after white space is not a call or an index (`f (x)`, `a [1]`, `(a+b) (c+d)` are
  products); "white space" is a gap between the previous token's span and this one's (spaces, comments, a line end
  inside brackets, where the lexer drops it). The lexer is unchanged: `Tok::Juxt` still joins `2x` and `2(x)`.
- **Gaps:** `Parsed::products` (and `Colouring::products`, moved by `Colouring::shifted`, which drops gaps touching
  the edit) holds the span of each gap a spaced product joins; the notebook draws a faint `×` there. A statement
  that fails later keeps the gaps the parser had already passed (no backtracking), so the mark does not flicker
  while a product is typed (`m c^` keeps the gap of `m c`).
- **Departures from the plan's table:** none in meaning. `a/b c` reads as the n-ary `mul(a, pow(b, -1), c)` (as
  `a/b*c` does; C.1), not as the nested `mul(mul(a, pow(b, -1)), c)` that the written `(a/b)*c` gives; likewise
  `-x y` is `-x*y` (`mul(mul(-1, x), y)`), equal in value to `-(x*y)`.
- **Lints** (`lint.rs`): `Lint::fix` is now a list of replacements (`Vec<(Range, String)>`, insertion = empty
  range) with `fix_label`, `fixed(src)` and `replacement(src)` (one range and text, for an editor's single undo
  step). New: `lint_calls_with(src, known, function)` and an extra `function` argument of `lint_program` and
  `colour_lint`; `lint_calls` keeps its signature (no function names: the kernel's notes are unchanged).
  - a name that `function` says is a function (or that the text defines as one: `fn`, `rule`, `f(...) :=`), not
    bound in the statement, followed by a factor across white space: "sin x is the product sin*x; for the
    function write sin(x)", fix `sin(x)` (the gap becomes `(`, a `)` after the factor; for a parenthesised factor
    `f (x)` the gap is removed);
  - a call whose callee is a parenthesised sum, product, power, negation or number with no named or string
    argument: "this calls (a + b); for the product write a space or `*`", fix: `*` after the callee
    (`(E(4) * E(7))("Coefficients")` from the documentation is a property access and is not flagged).
- Tests: `tests/juxtaposition.rs` (every row of the table both ways, precedence against `^ / -`, calls, indexing,
  primes, `√`, `×`, box forms, lambdas, options, patterns, conditions, slots, blocks and control; line ends and
  comments; the gaps; the round trip of the new forms), lint unit tests; `tests/errors.rs` and `tests/colour.rs`
  cases that relied on `a b` being an error now use strings.

## Round 36

A slot function without slots (`&(x)`) lowers to `fn([&1], x)`: it takes one argument and ignores it (semantics 6.7: a slot function ignores arguments past its highest slot). `() => x` stays the zero-argument closure.
