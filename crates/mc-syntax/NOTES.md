# Package C: `mc-syntax`

Status: built and committed (branch `core/c-syntax`, commit 35b5015). A 256-statement corpus plus a multi-line program
file round-trip; error tests check positions and recovery; fuzzing never panics. No build or clippy warnings. About
4,800 lines of source plus 960 of tests; depends only on `mc-api` (`num-bigint` as a test oracle).

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
- **C.3 Juxtaposition** (`2x`, `3(a+b)`) needs no space; `2 x` is an error; numbers take no postfix.
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

## Suggestions for the coordinator

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

## What integration needs from this package

- `mc-eval` interprets the shapes of C.7 to C.12 (documented on `Head`): `define_fn`, `define_rule`, `define_sym`,
  `set`/`set_delayed`/`assign`, `block`/`let`/`var`, `fn` closures with the parameter wrappers, the pattern heads; and
  turns `ball_literal(text, digits)` into balls.
- `mc-arith` implements `NumberFormat` (fast decimal integers; balls with only correct digits).
- The REPL and bench harness call `read(store, src)`, show `Diagnostic::render(src)`, and print with
  `Printer::program`.
