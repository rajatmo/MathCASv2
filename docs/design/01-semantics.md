# MathCAS v2: core semantics (draft 1)

This document specifies what the core computes. It does not fix the concrete syntax (that is `04-syntax.md`); the
notation here is abstract: `f(a, b)` is a compound term, `[a, b]` a list, `p -> r` a rule, `p := r` a delayed
definition. Every rule of evaluation the implementation follows must be written here. Where this draft makes a choice,
the reason is given. Open questions are collected in section 13.

Words: **must** is a requirement on any implementation; **term** is a value of the core; **normal form** is a term
that evaluation leaves unchanged under the current definitions.

## 1. Terms

A term is one of:

| Kind | Examples | Notes |
|---|---|---|
| Symbol | `x`, `sin`, `alg.gcd` | Qualified by a module path; see 1.2. |
| Integer | `0`, `-12`, `2^100000` | Arbitrary size. |
| Rational | `3/4` | Always reduced, positive denominator, denominator > 1. |
| Machine real | `1.5`, `1e-300` | IEEE 754 binary64. |
| Ball | `1.2345[+-1e-40]` | Arbitrary-precision midpoint and radius; see 2.3. |
| String | `"abc"` | Unicode, immutable. |
| Compound | `f(a, b)`, `g(x)(y)` | A head term and zero or more argument terms. Any term may be a head. |
| Packed | polynomial, series, numeric array | An opaque representation of a compound term; see section 11. |

Complex numbers are the compound `complex(re, im)` with numeric parts, normalised by arithmetic (2.4). Lists are the
compound `list(...)`, written `[...]`.

### 1.1 Identity and equality

- Terms are immutable and **hash-consed**: two structurally equal terms are the same object. Structural identity
  `a === b` is therefore O(1) and is the only equality the core uses for matching and caching.
- Structural equality distinguishes number kinds: `1 === 1.0` is false, `1/2 === 0.5` is false.
- Mathematical equality `a == b` is a function, not a primitive: it returns `true`, `false`, or stays unevaluated when
  undecided (see 2.5 for numbers).

### 1.2 Symbols and modules

- Every symbol lives in a module (`core`, `alg`, `user.session`, ...). A bare name resolves through the importing
  module's import list; resolution happens once, when source is read, never during evaluation.
- There is no mutable global search path. *Reason:* name resolution that changes at run time was a source of
  unpredictability in v1's reference language.
- A symbol carries a **signature** (argument modes, section 5), **operator properties** (section 7.3), a **rule set**
  (section 6) and a **version counter** incremented on every change to any of these.

### 1.3 No sequence splicing in the core

There is no term that splices itself into its parent's argument list. Variadic behaviour comes from sequence patterns
(6.1) and the explicit `apply(f, list)`. *Reason:* splicing makes the arity of a term depend on evaluation and
complicates every structural operation; the use cases are covered by sequence patterns.

## 2. Numbers

### 2.1 The kinds

Exact: integers, rationals, and exact complex numbers built from them. Inexact: machine reals and balls, and inexact
complex numbers built from them. A complex number is exact only if both parts are exact.

### 2.2 Contagion

The result of arithmetic on mixed kinds is the *least exact* kind present:

`exact < ball < machine`

An operation with a machine real produces a machine real; one with a ball (and no machine real) produces a ball;
only exact operands give an exact result. *Reason:* a machine real carries no error bound, so mixing it with a ball
cannot honestly produce a ball.

### 2.3 Balls

- A ball `m[+-r]` is the set of reals within `r` of `m`, where `m` is a binary floating-point number of `p` bits and
  `r` a small-precision upper bound. Operations return a ball containing every possible exact result (enclosure).
- `p` is the working precision of the midpoint. The radius records the accumulated error, so a ball's **accuracy**
  is derived from `r` and is never stored separately.
- A ball with `r = 0` is an exact binary number.
- *Reason:* ball arithmetic gives provable error bounds, has one rule (enclosure) instead of the per-operation
  precision-propagation rules of significance arithmetic, and costs about the same as plain floats at high precision.

### 2.4 Exact arithmetic and canonical numbers

- Integer and rational arithmetic is exact. `n/0` is `failure(division_by_zero)`.
- Exact powers stay exact when the result is rational (`4^(1/2)` is `2`); otherwise they stay as the term `pow(4, 1/3)`
  in canonical radical form (perfect powers extracted, `pow(8, 1/2)` is `2*pow(2, 1/2)`).
- Exact complex results with a zero imaginary part are real.

### 2.5 Comparisons and undecidability

Comparisons on numbers are three-valued:

- exact against exact: decided;
- involving balls: `true` or `false` when the balls do not overlap, otherwise the comparison **stays unevaluated**;
- involving machine reals: decided by IEEE comparison (NaN compares unordered: `==` is false, `!=` is true).

Code that needs a decision on overlapping balls must refine precision explicitly (2.6). *Reason:* silently picking an
answer for overlapping balls is the classic source of wrong results.

### 2.6 Numerical evaluation

`n(e, d)` evaluates an exact or symbolic `e` to a ball with at least `d` correct decimal digits, or fails:

1. evaluate `e` with balls at working precision `p = d * log2(10) + guard`;
2. if the result's radius gives at least `d` correct digits, return it;
3. otherwise double `p` and repeat, until a precision cap or the fuel limit, then return
   `failure(precision_not_reached, best_so_far)`.

`n(e)` with no target gives a machine real. The precision cap and the guard bits are parameters with defaults.

### 2.7 Machine arithmetic

Machine reals follow IEEE 754 binary64 with round-to-nearest. Overflow gives infinity, invalid operations give NaN;
neither raises. There is no automatic switch from machine reals to balls. *Reason:* v1's "redo in high precision when
the machine result underflows" made results depend on hidden thresholds.

## 3. Canonical arithmetic

`add`, `mul` and `pow` (surface `+`, `*`, `^`) are built-in constructors with a fixed **canonicaliser** that every
construction passes through. It is not done by rules and cannot be overridden by user rules on `add` or `mul`.

The canonical form of a sum or product:

1. flattened (`add(a, add(b, c))` is `add(a, b, c)`);
2. numbers folded into one numeric coefficient (respecting contagion, 2.2);
3. like terms collected: `x + 2x` is `3x`, `x * x^2` is `x^3`;
4. identity elements removed and single-argument forms unwrapped (`add(x)` is `x`, `mul()` is `1`);
5. arguments ordered by the **canonical order** (section 8).

`pow` is canonicalised for numeric bases and exponents (2.4) and for `pow(pow(x, a), b)` only where the combination is
valid for all values (integer `b`, or `x` known positive); otherwise left as is.

Subtraction and division are not separate operators: `a - b` constructs `add(a, mul(-1, b))` and `a / b` constructs
`mul(a, pow(b, -1))`.

*Reason:* a fixed, specified canonicaliser makes the cheap simplifications free and predictable, and keeps them out of
the rule engine, where they would be retried constantly.

## 4. Evaluation

### 4.1 The evaluator

Evaluation `eval(t)` under the current definitions:

1. **Already normal:** if `t` carries a normal-form stamp that is still valid (4.3), return `t`.
2. **Atoms:** numbers and strings are normal. A symbol evaluates to the right side of its own-value rule, if it has
   one (6.4), otherwise it is normal.
3. **Compound `h(a1, ..., an)`:**
   1. evaluate the head `h`;
   2. evaluate each argument according to the head's argument modes (section 5), left to right;
   3. if the head is a canonicalised constructor (section 3), canonicalise;
   4. if the head is a built-in with a native implementation, call it; it either returns a result or declines;
   5. otherwise try the head's rule set (section 6) on the term; the first rule that matches gives the replacement;
   6. if a replacement was produced, evaluate it (this is a loop, not recursion, and each iteration costs fuel);
      otherwise the term is in normal form: stamp it (4.3) and return it.

The strategy is **innermost** (arguments before the call). There is no re-evaluation of a result "until nothing
changes": a term is evaluated once to normal form, and only definition changes (4.3) can make it evaluate again.

### 4.2 Termination

Every evaluation runs under a **fuel** budget, plus time and memory limits (section 10). Each rule application and each
native step costs fuel. Exhausting any limit stops evaluation with `failure(limit, kind, partial)`, where `partial` is
the term reached. There is no recursion limit separate from these.

### 4.3 Normal-form stamps

Each symbol has a version counter (1.2). The **dependency version** of a term is the maximum version over the symbols
it contains, cached per term (terms are immutable). A normal-form stamp records the dependency version at the time the
term was found normal; the stamp is valid while the dependency version is unchanged.

Consequence: changing the definition of `f` re-opens only terms that contain `f`. *Reason:* this is the precise form of
the timestamp trick v1's reference language uses informally, and it lets evaluation skip normal subterms in O(1).

Rules whose right sides depend on symbols not in the term (for example through a condition that reads a global value)
must declare those symbols, or the stamp may be stale. **Open:** whether to track this automatically (13.1).

### 4.4 Memoisation

Because terms are hash-consed and normal forms are stamped, `eval` may cache `t -> normal form of t` per term. This is
an implementation freedom, not an observable behaviour, except through side effects, which section 9 restricts.

## 5. Argument modes (holding)

A function's signature declares, per argument position, one of:

- `strict` (default): the argument is evaluated before the call;
- `held`: the argument is passed as the unevaluated term.

Signatures may give a mode per position, a mode for all positions, or a mode for "the rest".

The only escape is `eval(e)` written directly in a held position: it forces evaluation of that argument. There is no
other mechanism that changes holding: no attributes that differ from behaviour, no per-call override heads, no hidden
holds. `signature(f)` shows exactly what applies. *Reason:* v1's reference language mixes attributes, inline override
heads and built-ins that hold without saying so; every one of those was a source of bugs.

`quote(e)` is a built-in with a held argument that returns `e` unevaluated, wrapped so that it stays unevaluated when
passed on; `unquote(q)` evaluates it.

## 6. Rules and patterns

### 6.1 Patterns

| Pattern | Matches |
|---|---|
| `_` | any single term |
| `x_` | any single term, bound to `x` |
| `x_ : T` | a term whose type (head or number kind) is `T` |
| `xs__` / `xs___` | one or more / zero or more consecutive arguments, bound to a list |
| `p | q` | `p` or `q` (both must bind the same variables) |
| `p /; c` | `p`, and then `c` evaluates to `true` with the bindings in scope |
| `x_ = d` | an optional argument with default `d` |
| literal term | exactly that term |

A variable used twice in a pattern must bind identical terms both times (non-linear patterns).

### 6.2 Matching modulo operator properties

Operators can declare `associative`, `commutative` and an `identity` element (7.3). Matching against such a head is
modulo those properties: `f(a, x_)` matches `f(b, a)` when `f` is commutative. Matching returns the bindings of the
**first** match in a defined enumeration order (matches are enumerated in canonical order of the bound arguments), so
results are deterministic.

### 6.3 Rule order

Within one symbol's rule set, rules are tried **most specific first**:

- rule A is more specific than rule B if A's left side is an instance of B's (B matches every term A matches); this is
  a syntactic check on patterns, conditions ignored;
- rules not ordered by this relation are tried in definition order.

Defining a rule whose left side is identical (up to renaming of pattern variables) to an existing rule replaces it.
*Reason:* the "instance of" relation is decidable and predictable; heuristic specificity orderings are not.

### 6.4 Definitions

- `p := r` (delayed): adds the rule `p -> r` to the rule set of `p`'s head symbol; `r` is evaluated each time the rule
  fires, with the bindings substituted.
- `p = r` (immediate): evaluates `r` now, then adds `p -> value`.
- For a symbol `x`, `x = r` defines its own value.
- Definitions return `null` and increment the symbol's version (4.3).

### 6.5 Substitution is hygienic

Pattern variables are substituted into the right side as terms. Binding forms inside the right side (functions, 6.6;
local variables) are renamed when needed so that a substituted term never captures a bound variable.

### 6.6 Functions and scoping

- `fn(x, y) -> body` is a closure with **lexical** scope. Closures capture the bindings visible where they are
  created.
- `let(x = a, body)` introduces lexical local names.
- There is no dynamic scoping in the core. `with_values(x = a, body)` (temporarily changing a symbol's own value) is a
  library function defined in terms of definitions and their removal, and is marked as having an effect.

## 7. Built-in structure

### 7.1 Native built-ins

A built-in may have a native implementation. It receives the evaluated arguments (per its signature) and either returns
a term or **declines**, in which case the rule set is tried (4.1 step 3.5). Native code must poll the interrupt and
charge fuel in every loop whose trip count depends on input (section 10).

### 7.2 Listability

A function may declare `listable`: called with list arguments of equal length, it maps over them element-wise (and
broadcasts non-list arguments). Lists of unequal length give `failure(shape_mismatch)`.

### 7.3 Operator properties

`associative`, `commutative`, `identity(e)` and `idempotent` may be declared on a symbol. They affect matching (6.2)
and the symbol's own canonicalisation (flattening, sorting arguments, removing identity elements), using the same
procedure as section 3 but without number folding. They are visible through `properties(f)`.

## 8. Canonical order

The canonical order is a total order on terms, used for sorting arguments of commutative operators, for `sort`, and
for enumerating matches (6.2). Draft order:

1. numbers before everything else, ordered by value (exact before inexact at equal value), complex numbers by real
   then imaginary part;
2. then strings, ordered by code point;
3. then symbols, by name, then module path;
4. then compound terms: by head, then by number of arguments, then argument by argument.

Printing may use a different, presentation-oriented order (for example polynomials by degree); that is a property of
the printer, not of the term. **Open:** whether to order monomials by degree in the canonical order itself (13.3).

## 9. Effects and state

- Evaluation is deterministic given the definitions, the inputs and the random-number state.
- Effects are: changing definitions; input and output; reading the clock; drawing random numbers. Functions that
  perform effects are marked `effect` in their signature.
- Random numbers come from an explicit generator state owned by the session; `seed(n)` resets it. Results that involve
  random draws are never cached (4.4).
- Output functions do not print as a side effect of evaluation; they return values or append to the session's output
  stream, which the front end displays.

## 10. Failures, diagnostics and limits

- Errors are values: `failure(tag, details...)`. A failure propagates through strict arguments (a call with a failure
  argument returns that failure) unless the function declares that it handles failures.
- Diagnostics (warnings) are attached to the evaluation result as a list, not printed during evaluation. A computation
  that produces a value with a warning still returns the value.
- Limits are fuel, wall-clock time and memory, set per evaluation with defaults per session. Exceeding one gives
  `failure(limit, kind, partial)` (4.2).
- An external interrupt (the user pressing stop) is checked at the same points as fuel and gives
  `failure(interrupted, partial)`.

## 11. Packed terms

Polynomials, rational functions, truncated series and numeric arrays may be held in packed internal forms.

- A packed term is **observably the same** as its unpacked compound form: `===`, matching, printing and every function
  behave as if it were unpacked. `normal(t)` returns the unpacked form explicitly.
- Packing is an implementation choice made by the functions that produce such values (expansion, series, array
  construction) and by the canonicaliser when it pays off.
- An operation that leaves the domain (for example applying an unknown function to a polynomial's coefficient)
  unpacks.

*Reason:* the fast algorithms (heap-based sparse multiplication, packed numeric arrays) need their own
representations, but users should never need to know which one is in use.

## 12. What is deliberately absent

- Re-evaluation until no change ("infinite evaluation").
- Attributes separate from behaviour; hidden holds; inline override heads other than `eval` in held positions.
- Self-splicing sequences.
- Run-time name resolution and dynamic scoping in the core.
- Automatic machine-to-arbitrary-precision switching.
- Messages printed as a side effect of evaluation.

## 13. Open questions

1. **Hidden dependencies of rules.** Should the evaluator track the symbols a condition or right side reads (so stamps
   stay valid automatically), or must rules declare them? Tracking is safer; declaring is cheaper.
2. **Upvalues.** Rules attached to an argument's symbol rather than the head (for example defining `area(square(_))`
   on `square`) are useful for extensibility. Include them in the core, or provide them through operator dispatch on
   types?
3. **Canonical order of polynomials.** Order monomials by degree in the canonical order (helps printing and some
   algorithms), or keep the simple structural order and let the printer reorder?
4. **Assumptions.** How facts like "x is positive" reach the canonicaliser (3) and simplification: a scoped assumption
   context, or explicit arguments to the functions that need them?
5. **Simplification.** Specify the cost function for equality-saturation simplification here, or leave it to the
   library?
6. **Exact constants.** Are `pi`, `e` and friends symbols with numerical-evaluation rules, or a small exact-constant
   kind of term with canonical arithmetic of their own?
7. **Types.** How the gradual type system of the surface language maps onto the pattern types in 6.1 and onto the
   numeric compiler.

Each open question gets a short decision note in this document once a spike or a benchmark settles it.
