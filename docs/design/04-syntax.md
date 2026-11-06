# MathCAS v2: concrete syntax (draft 1)

This is the surface language: how source text is written and how it lowers to the core terms of
[`01-semantics.md`](01-semantics.md). It descends from v1's native syntax, keeping what proved itself (calls `f(x)`,
lists `[a, b]`, `=>` lambdas, `let`, pipes, `fn` versus `rule`, `match`, `when`, modules, prime marks). It changes
what the v2 semantics require: declared holding, operator properties, balls, failures, no sequence splicing, and no
legacy escape (compatibility with other languages is a translator outside the core).

## 1. Tour

```text
let f = x => x^3 - 2x                        # lambda; a numeric literal before a name multiplies: 2x is 2*x
diff(f(x), x)                                # 3x^2 - 2
range(5) |> map(k => k^2, _) |> sum          # pipes; `_` marks where the value goes, a bare name takes it as the only argument

{ let y = 2; let z = y + 1; y*z }            # block: lexical, immutable bindings; value of the last expression

fn fact(0) = 1                               # fn: a checked function, clauses tried most specific first
fn fact(n: Int) when n > 0 = n*fact(n - 1)
fact(-1)                                     # failure(no_match, fact, [-1])

rule fib(0) = 0                              # rule: an open rule set; a call nothing matches stays as it is
rule fib(1) = 1
rule fib(n_: Int) := fib(n) = fib(n - 1) + fib(n - 2)    # memoising definition

@associative @commutative @identity(0)
sym oplus                                    # a symbol with operator properties and no rules yet

fn plot_points(held e, held v: Sym, a: Real, b: Real) = ...   # held parameters receive the unevaluated term

n(pi, 50)                                    # 3.1415926535897932384626433832795028841971693993751
1.25~40                                      # a ball literal: 1.25 at 40 digits
2 + 3im                                      # complex number

assuming(x > 0, simplify(sqrt(x^2)))         # x
rewrite(e, [sin(a_)^2 + cos(a_)^2 -> 1])     # rule application is a function, not an operator

module Geo {
  fn sq(x) = x*x                             # private
  pub fn area(r: Real): Real = pi*sq(r)
}
import Geo
area(1)                                      # pi
```

## 2. Lexical structure

- **Comments:** `#` to the end of the line.
- **Identifiers:** letters, digits and `_`, not starting with a digit; Unicode letters allowed (`α`, `θ1`). A trailing
  `_` (and `__`, `___`) makes a pattern (section 5), so identifiers cannot end in `_`.
- **Keywords:** `let var fn rule sym module pub import if then else while for in match when held and or not true
  false null`.
- **Numbers:**

  | Literal | Term |
  |---|---|
  | `42`, `-7`, `1_000_000` | integer (`_` separators allowed) |
  | `0x1F`, `0b1011` | integer in base 16 or 2 |
  | `3/4` | not a literal: the exact division `3 / 4`, which is the rational `3/4` |
  | `1.5`, `2e-10`, `1.` | machine real |
  | `1.25~40`, `2~100` | ball at 40 (100) decimal digits of working precision, radius from the literal's rounding |
  | `3im`, `1.5im` | imaginary: `complex(0, 3)`, `complex(0, 1.5)` |

  The `~` form is the only way to write an arbitrary-precision number directly; it is exact to the digits written and
  carries the given working precision. `ball(m, r)` builds a ball with an explicit radius.
- **Strings:** `"..."` with escapes `\n \t \" \\ \u{...}`. No interpolation in the core syntax.
- **Raw strings:** `r"..."` holds the characters between the quotes as they are, with no escapes, so TeX labels read
  as written: `r"$\alpha_1 \le \beta$"` is the string `"$\\alpha_1 \\le \\beta$"`. To hold a `"`, put the same
  number of `#` (one or more) after the `r` and after the closing quote: `r#"say "hi""#`; the string ends at the
  first `"` followed by that many `#` (`r##"a "# b"##`). A raw string is one token of class `String`; like a plain
  string it ends on its line (a raw string still open at the line end is an error, "unterminated raw string").
  `r` followed by anything else is the name `r` (`r#c` is `r` and a comment). The printer writes a TeX string (one
  that holds a `$` and a backslash, and no control character) in raw form, with the fewest `#` that work; other
  strings, file paths among them, with escapes.
- **Prime marks:** `f'`, `f''`, ... written directly after a name (no space) are derivatives (section 8).
- **Unicode operators:** each of these characters is one token, the same token as its ASCII spelling, so text typed
  with the notebook's aliases (08-notebook.md section 3.9) reads as written. The printer always writes ASCII.

  | Character | Reads as | Character | Reads as | Character | Reads as |
  |---|---|---|---|---|---|
  | `→` | `->` | `≡` | `===` | `−` (minus sign) | `-` |
  | `⧴` | `:>` | `≢` | `!==` | `∧` | `and` |
  | `≤` | `<=` | `×` | `*` | `∨` | `or` |
  | `≥` | `>=` | `·`, `⋅` | `*` | `¬` | `not` |
  | `≠` | `!=` | `÷` | `/` | `∈` | `in` (`for k ∈ xs`) |
  | `⩵` | `==` | `√` | prefix square root | | |

  `√x` is `sqrt(x)`; its operand is a primary with its calls, indices and primes, so `√x^2` is `(√x)^2`,
  `√(x + 1)` is `sqrt(x + 1)`, `2√2` is `2*sqrt(2)`. Other mathematical symbols with no meaning in the language
  (`∀`, `±`, `∫`, `∑`, `∂`, ...) are errors that name the character and its code point; `°` explains that angles are
  in radians. Letter-like characters (`ℏ`, `ℓ`, `ⅆ`, Greek) are identifier characters.
- **Placeholder:** `□` (U+25A1) marks an unfilled slot of a notebook template (08-notebook.md section 4.3). It is a
  token of its own and never valid: reading text that contains one fails with "a placeholder `□` is not filled in"
  at the first placeholder of each statement.

## 3. Expressions

### 3.1 Operators

Binding power, low to high:

| Level | Operators | Associativity | Lowers to |
|---|---|---|---|
| 1 | `=` `:=` | right | definitions (section 6) |
| 2 | `\|>` | left | call with the value inserted (3.3) |
| 3 | `->` `:>` | right | `rule(a, b)`, `rule_delayed(a, b)` |
| 4 | `when` | left | `condition(p, c)` (in patterns) |
| 5 | `\|` | left | `alternatives(a, b)` (in patterns) |
| 6 | `or` | left | `or(a, b)` |
| 7 | `and` | left | `and(a, b)` |
| 8 | `not` | prefix | `not(a)` |
| 9 | `== != < <= > >= === !==` | non-associative | `equal`, `unequal`, `less`, ..., `same`, `not_same` |
| 10 | `+ -` | left | `add(a, b)`, `add(a, mul(-1, b))` |
| 11 | `* /`, juxtaposed numeric coefficient | left | `mul(a, b)`, `mul(a, pow(b, -1))` |
| 12 | unary `-` | prefix | `mul(-1, a)` |
| 13 | `^` | right | `pow(a, b)` |
| 14 | call `f(...)`, index `a[...]`, prime `'` | left | `f(...)`, `part(a, ...)`, `derivative(n)(f)` |

- Comparisons do not chain: `a < b < c` is a syntax error; write `a < b and b < c`.
- Juxtaposition multiplies only a numeric literal followed by a name or a parenthesis (`2x`, `3(a+b)`, `2pi`). Two
  names never multiply by juxtaposition: `a b` is a syntax error. *Reason:* implicit multiplication between names
  makes `f (x)` ambiguous and was rejected in v1.

### 3.2 Calls, lists, indexing

- `f(a, b)` is a call; `f(a)(b)` calls the result. Any expression can be called.
- Named arguments: `n(x, digits: 50)`. They lower to a trailing `options(digits -> 50)` argument, which signatures
  declare (section 7).
- `[a, b, c]` is `list(a, b, c)`; `[]` is the empty list.
- `xs[i]` is `part(xs, i)`; `xs[i, j]` is `part(xs, i, j)`. Indices start at **1**; negative indices count from the
  end; `xs[2:5]` is the span `part(xs, span(2, 5))`.

### 3.3 Pipes

`a |> f(b, _)` is `f(b, a)`: the value goes where the bare `_` stands. `a |> f` (a name or other callable, no
parentheses) is `f(a)`. A `_` is a pipe hole only directly as an argument in the call on the right of `|>`; elsewhere
it is a pattern.

### 3.4 Functions

- `x => body`, `(x, y) => body`, `() => body` are closures (`fn(...)` terms in the core, semantics 6.7).
- Parameters may be typed: `(x: Real) => x^2`.
- `&(&1 + &2)` is a slot function: `&1`, `&2`, ... are its arguments. There is no "all arguments" slot (the core has
  no sequence splicing, semantics 1.3); write a named lambda with a rest parameter instead.

### 3.5 Blocks and local bindings

- `{ s1; s2; ...; e }` is a block; its value is the last expression; statements are separated by `;` or new lines.
- `let x = e` binds an immutable lexical name for the rest of the block.
- `var x = e` binds a mutable lexical local; `x = v` inside the block assigns to it. Assigning to a name bound by
  `let` is a compile error. A `var` never becomes a symbol definition (semantics 6.7).

### 3.6 Control

- `if c then a else b`; without `else`, the value is `null` when `c` is false. If `c` evaluates to neither `true`
  nor `false`, the whole `if` stays unevaluated (semantics 2.5 applies to undecided comparisons).
- `while c { body }` and `for x in xs { body }` evaluate to `null`. `for` iterates over lists, ranges and packed
  arrays.
- `match e { p1 => r1, p2 when c => r2, ... }`: the first arm whose pattern (and condition) matches wins; `e` is
  evaluated once; no arm matching gives `failure(no_match, e)`.

## 4. Numbers and constants in the syntax

- `pi`, `e`, `im`, `inf` and `true`, `false`, `null` are imported from `core` into every module by default. `e` and
  `im` can be shadowed by a local binding or excluded: `import core except e`. *Reason:* `e` and `im` are the
  standard names in mathematics and numerical computing; making them shadowable avoids stealing the names from users
  who need them as variables.
- Unicode aliases: `π` is `pi`, `∞` is `inf`, `ⅇ` (double-struck e) is `e`, `ⅈ` (double-struck i) is `im`; a
  number with the suffix `ⅈ` is imaginary like one with `im` (`3ⅈ` is `3im`). An alias is a whole identifier: `πr`
  is the name `πr`.
- Printing shows only the correct digits of a ball (`n(pi, 20)` prints `3.1415926535897932385`); the input form
  `ball(m, r)` shows midpoint and radius.

## 5. Patterns

| Syntax | Meaning (semantics 6.1) |
|---|---|
| `_`, `x_` | any term; named |
| `x_: T` | a term of type `T` (section 9) |
| `xs__`, `xs___` | one or more, zero or more arguments; bound to a list |
| `x @ p` | `p`, with the whole match bound to `x` |
| `p when c` | `p` with condition `c` |
| `p \| q` | alternatives |
| `x_ = d` | optional argument with default `d` (in argument lists) |
| literal | that term |

Patterns are ordinary expressions; they are only interpreted as patterns on the left of `->`, `:>`, `=`, `:=`, in
`match` arms and in the arguments of matching functions (`cases`, `rewrite`, ...).

## 6. Definitions

### 6.1 `rule`: open rule sets

- `rule f(x_) := body` adds a delayed rule (semantics 6.4); `rule f(0) = 1` an immediate one.
- `rule f(x_) := body when c` puts the condition on the rule.
- A call that matches no rule stays unevaluated, as the semantics specify for terms without applicable rules.
- With the decorator `@on(square)`, `rule area(square(a_)) := a^2` is stored with `square` as an argument-head rule
  (semantics 6.5); without it, it is a rule of `area`:

  ```text
  @on(square)
  rule area(square(a_)) := a^2
  ```

### 6.2 `fn`: checked functions

- `fn name(params) = body` defines one clause. Parameters are names, typed names `x: T`, literals, patterns, defaults
  `n: Int = 10` (trailing) and a rest parameter `rest...` (last, bound to a list).
- `fn name(params) when c = body` adds a condition.
- `fn name(params): T = body` declares the result type.
- Clauses accumulate. A call with ground arguments (numbers, strings, numeric constants and terms built from them,
  no free symbols) that matches no clause gives `failure(no_match, name, args)`. A call with a symbolic argument
  that matches no clause stays unevaluated, like a `rule` call: a type test or condition may be undecided for a
  symbol (`h(y)` for `fn h(x) when x > 0`), and numeric code (`find_root`, `plot`, `n_integrate`) substitutes
  numbers later. So `fact("a")` fails at once, while `fact(x)` stays `fact(x)`.
- `fn` lowers to rules plus that final catch-all (guarded by `is_ground(args)`), so `fn` and `rule` are the same
  machinery with a different default.

### 6.3 Values

- At the top level of a session or module, `x = v` defines the own value of the symbol `x` (semantics 6.4) and
  `x := e` a delayed own value.
- `unset(x)` removes the own value; `unset(f)` removes all rules of `f`.

### 6.4 Decorators

Decorators precede `fn`, `rule` or `sym` and set the symbol's signature and properties (semantics 5, 7.2, 7.3, 9,
13):

| Decorator | Effect |
|---|---|
| `@listable` | maps over list arguments |
| `@associative`, `@commutative`, `@identity(e)`, `@idempotent` | operator properties |
| `@effect` | marks the function as having effects |
| `@uses_assumptions` | the function reads the assumption context |
| `@handles_failures` | failure arguments are passed in instead of propagating |
| `@on(s)` | stores the rule with symbol `s` (argument-head rule) |

`sym name` declares a symbol without rules (useful for properties: `@commutative sym oplus`). An undeclared bare name
that is not bound anywhere is a symbol of the current module, created when the source is read.

### 6.5 Holding

A parameter written `held x` receives the unevaluated argument (semantics 5). `held` may prefix any parameter form
(`held e`, `held v: Sym`, `held body...`). `eval(e)` in a held argument position forces evaluation; `quote(e)` and
`unquote(q)` hold and release explicitly.

## 7. Options

A function that takes named arguments declares them as parameters after a `;`:

```text
fn n(x, d: Int = 16; guard: Int = 32, max_precision: Int = 100_000) = ...
n(x, 50, max_precision: 10^6)
```

Named arguments may appear in any order after the positional ones; unknown names are a compile error when the
signature is known statically, and `failure(unknown_option, name)` otherwise.

## 8. Calculus notation

`f'`, `f''`, ... are `derivative(1)(f)`, `derivative(2)(f)`, ...; `y''(t)` is `derivative(2)(y)(t)`. The printer
writes derivatives of plain names back with marks up to order 4.

## 9. Types

The spelling of the type language of semantics section 16:

```text
T ::= Any
    | Int | Rat | Float | Ball | Real | Complex[T] | Num
    | Str | Sym | Bool | Expr
    | List[T] | Array[T, rank]
    | Head[f]
    | (T, ..., T) -> T
    | T | T
    | T where predicate
```

- `Int` integer, `Rat` integer or rational (exact real), `Float` machine real, `Ball` arbitrary-precision ball,
  `Real` any of these, `Complex[T]` complex with parts of type `T`, `Num` any number.
- `Expr` is any term other than a string or list; `Head[f]` a compound term with head `f`.
- Gradual typing and checking follow v1's native layer: definite mismatches (types that share no value) are compile
  errors, narrowings are allowed and checked at run time at `fn` parameters.

## 10. Modules and names

- `module M { ... }` groups definitions; `pub` exports. `M.f` names a member; `import M` brings its public names into
  scope; `import M.{f, g}` selected ones; `import M except h`.
- Names resolve when source is read (semantics 1.2). A local binding shadows an import; an import shadows `core`.
- There is no run-time context path.

## 11. Statements and files

- A file or notebook cell is a sequence of statements: expressions, definitions, declarations, `module` and `import`.
- The whole input is parsed and checked before any of it is evaluated; on an error nothing is evaluated.
- File types: `.mc` (script) and `.mcn` (notebook), with a first line `#! mathcas 2` that distinguishes v2 files from
  v1 native files. *Reason:* v2's syntax is the successor of v1's native syntax; the header avoids a new set of
  extensions while keeping the two apart.

## 12. Lowering summary

| Surface | Core term |
|---|---|
| `a - b`, `a / b`, `-a` | `add(a, mul(-1, b))`, `mul(a, pow(b, -1))`, `mul(-1, a)` |
| `[a, b]`, `xs[i]`, `xs[i:j]` | `list(a, b)`, `part(xs, i)`, `part(xs, span(i, j))` |
| `x => e`, `&(&1 + 1)` | `fn([x], e)`, `fn([s1], add(s1, 1))` with a fresh `s1` |
| `{ let x = a; e }` | `let(x = a, e)` |
| `{ var i = 0; ... }` | a block with a mutable lexical cell (semantics 6.7) |
| `if c then a else b` | `if(c, a, b)` |
| `while c { b }`, `for x in xs { b }` | `while(c, b)`, `for(x, xs, b)` |
| `match e { p => r, ... }` | `match(e, [p :> r, ...])` |
| `a \|> f(b, _)` | `f(b, a)` |
| `rule f(x_) := b` | rule `f(x_) -> b` (delayed) added to `f` |
| `fn f(x: T) = b` | rule `f(x_: T) -> b` plus the `no_match` catch-all |
| `@commutative sym op` | `op` with property `commutative` |
| `held x` parameter | argument mode `held` in the signature |
| `1.25~40`, `3im` | ball, `complex(0, 3)` |
| `f'`, `y''(t)` | `derivative(1)(f)`, `derivative(2)(y)(t)` |
| `n(x, digits: 50)` | `n(x, options(digits -> 50))` |

## 13. Grammar (sketch)

```text
program    ::= { statement sep }
statement  ::= decorators? definition | 'module' name '{' program '}' | import | expr
decorators ::= { '@' name [ '(' args ')' ] }
definition ::= 'fn' name '(' params [';' params] ')' [':' type] ['when' expr] '=' expr
             | 'rule' expr ( '=' | ':=' ) expr
             | 'sym' name
             | name ( '=' | ':=' ) expr
import     ::= 'import' path [ '.{' names '}' | 'except' names ]
params     ::= [ param { ',' param } ]
param      ::= ['held'] ( name [':' type] ['=' expr] | name '...' | pattern | literal )
expr       ::= pipe-level expression, by the table in 3.1
primary    ::= literal | name | '(' expr ')' | '[' [args] ']' | block | lambda | slotfn
             | 'if' expr 'then' expr ['else' expr]
             | 'while' expr block | 'for' name 'in' expr block
             | 'match' expr '{' arms '}'
block      ::= '{' { ('let' | 'var') name [':' type] '=' expr sep | expr sep } [expr] '}'
lambda     ::= ( name | '(' params ')' ) '=>' expr
slotfn     ::= '&(' expr ')'
args       ::= arg { ',' arg }
arg        ::= expr | name ':' expr
```

## 14. Editor support

`mc-syntax` gives the notebook editor (08-notebook.md sections 3.4 and 3.10 to 3.12) these functions, all linear in
the length of a cell and total (any text, finished or not, gives a result; nothing panics):

- `colour(text, defined) -> Colouring` and `classify(text)` (the same with no name set): the class of every token,
  as sorted byte ranges, plus the bracket pairs and the errors.
- `names_in_scope(text, offset)`: the local names visible at a position, innermost first (completion).
- `extend_selection(text, start, end)`: the next syntactic unit around a selection.
- `Colouring::matching_bracket(caret)`, `enclosing_brackets(start, end)`, `pair_at(offset)`: bracket matching and
  "check balance".

### 14.1 Colouring classes

| Class | Tokens |
|---|---|
| `Operator` | operators, punctuation, prime marks, `√`, matched brackets |
| `Global` | a global name: defined, or any global name when no name set is given |
| `Undefined` | a global name the name set does not define (and not defined in the cell) |
| `Local` | a name bound by `let`, `var`, a `fn` or lambda parameter or a `for` loop, where bound and used; slots `&k` |
| `PatternVar` | `x_`, `xs__`, `_`, the `x` of `x @ p`, and uses of a pattern name in its rule |
| `Shadowing` | the binding occurrence of a local or pattern name that hides a defined global name |
| `Keyword` | reserved words except the constants; `where` in a type; `∧ ∨ ¬ ∈` |
| `Constant` | `true`, `false`, `null` |
| `Number`, `String`, `Comment` | literals with their suffixes and quotes; `#` to the end of the line |
| `Placeholder` | `□` |
| `SyntaxError` | the token at a syntax error, or characters the lexer cannot read |
| `UnmatchedBracket` | a bracket with no partner |

The scopes are computed on the token stream, not the syntax tree, so that they stay stable while a statement is
being typed (the parser drops a statement with an error). The walk follows the bracket tree and the grammar's binders:

- `let x` and `var x` bind `x` from the end of their statement to the end of the enclosing block (or cell); the
  value does not see the new binding (`let x = x + 1`).
- `fn f(params; options)` binds its parameters in its condition, result type and body; a pattern parameter binds its
  pattern names; defaults see earlier parameters.
- `x => e` and `(x, y: T) => e` bind in the body, which runs to the end of the argument, element or statement.
- `for x in xs { ... }` binds `x` in the block.
- In a statement, argument or element, each `->`, `:>`, `=`, `:=` (and the first `=>` of a `match` arm) binds the
  pattern names of its left side for the rest of it: `f(x_) := x^2`, `[a_ -> a + 1]`, `match e { [h_, t__] => h }`.
- Type names after `:`, option names in calls (`digits:` in `n(x, digits: 50)`), decorators, module names and
  imported names are never variables.
- Names defined in the cell (`fn f`, `rule f(...)`, `sym s`, `module M`, top-level `x = ...` and `f(...) := ...`)
  count as defined, so a cell's own definitions are not `Undefined` before it is evaluated.

Rules for unfinished text: an error at the end of the input (an unfinished statement, a string running to the end)
colours nothing and sets `Colouring::incomplete`; an error at a closing bracket while some bracket is unmatched colours
nothing (the unmatched bracket is shown). A name just typed may change class when the next token arrives (`y` becomes
`Local` when `=>` follows); every earlier name keeps its class. Texts over 100,000 bytes are coloured lexically
(brackets included, no scopes, no syntax errors).

### 14.2 Brackets

A closing bracket pairs with the nearest open bracket of its kind; open brackets left between them are unmatched
(`{ g(x }` marks only the `(`). Brackets in strings and comments do not count. `matching_bracket(caret)` prefers the
bracket just before the caret to the one just after it.

### 14.3 Extend selection

Units are tokens, every expression of the syntax tree, the contents of every bracket pair (argument and element
sequences), parenthesised groups with their parentheses (not the parentheses of a call), statements, and the whole
content. From a caret: inside or at the edge of a name or literal, that token; between a name and a following
operator, bracket or comma, the smallest unit around the name (`f|(x)` gives `f(x)`, `b|*c` gives `b*c`); touching
only an operator, bracket or comma, the smallest unit around it. From a selection, the smallest unit strictly
containing it; `None` after the whole content (the editor selects the cell).

## 15. Decisions in this draft, with reasons

| Choice | Reason |
|---|---|
| Keep v1's native look | Proven in v1; users already write it. |
| Rule application by functions (`rewrite`), no `/.` operator | One way to apply rules, with strategies as named arguments (`rewrite(e, r, strategy: innermost)`); easier to read than operator spellings. |
| `~` for ball literals | A literal precision marker is needed for high-precision input; `~` reads as "about". |
| `im` for the imaginary unit | Common in numerical computing; frees `i` for indices. |
| 1-based indexing, negative from the end | Matches matrix notation and v1. |
| `held` parameters and decorators | Holding and properties are visible in the definition and in `signature(f)` (semantics 5, 7.3). |
| No slot sequences, no splicing | The core has no sequence splicing (semantics 1.3). |
| `var` for mutable locals | Loops need local state; `var` keeps it lexical and separate from symbol definitions. |
| No legacy escape | v2 has no legacy core; compatibility is a translator into this syntax. |
| Unicode operators as tokens, ASCII when printing | Notebook aliases insert `→`, `≤`, `×` ...; reading them as the ASCII tokens keeps one grammar, and printed text stays plain ASCII. |
| `∈` reads as `in` | The only membership syntax is the loop `for x in xs`; a set-membership operator would need a library head first. |
| `√` binds like a call | `√x^2` as `(√x)^2` follows the usual precedence of prefix radicals in linear input; `√(x^2)` needs parentheses. |
| `□` is a token that is always an error | One clear message at the first placeholder instead of an unexpected-character error followed by cascades. |
| Scopes from tokens, not the tree | Colouring must not flicker while a statement is unfinished; the token walk is linear and never fails. |
