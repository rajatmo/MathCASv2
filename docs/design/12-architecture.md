# MathCAS v2: architecture of the workspace (round 34)

The crates, measured: how they depend on one another, how large they are, and the rules the structure keeps.
`tools/depcheck.ps1` prints the table from `cargo metadata` and checks the rules in `tools/depcheck-rules.txt`
(exit code 1 on a violation). Columns: **layer** is the longest path of normal dependencies down to a crate with
none (0 = a leaf); **lines** are lines of Rust in the crate's folder; **direct** and **trans** count the workspace
crates it depends on (normal and build dependencies, directly and transitively; dev-dependencies do not count);
**fan-in** counts the workspace crates that depend on it directly.

Round 34 restructured the workspace without changing behaviour: crate splits, moves and renames
only. Section 1 is the state before, section 2 the state after, section 3 what depends on what and why.

## 1. Start state

| crate | layer | lines | direct | trans | fan-in |
|---|---:|---:|---:|---:|---:|
| `mc-docs` | 11 | 5589 | 40 | 53 | 0 |
| `mc-repl` | 11 | 1409 | 6 | 48 | 0 |
| `mc-kernel` | 10 | 2174 | 6 | 46 | 2 |
| `mc-engine` | 9 | 7925 | 39 | 44 | 3 |
| `mc-lsp` | 9 | 2119 | 34 | 44 | 0 |
| `mc-calculus` | 8 | 38179 | 11 | 13 | 3 |
| `mc-plot` | 8 | 15006 | 8 | 16 | 3 |
| `mc-solve` | 8 | 9535 | 9 | 10 | 3 |
| `mc-sums` | 8 | 7801 | 7 | 13 | 3 |
| `mc-cad` | 7 | 6182 | 7 | 9 | 4 |
| `mc-geom` | 7 | 6520 | 5 | 10 | 3 |
| `mc-linalg` | 7 | 6482 | 7 | 8 | 3 |
| `mc-lists` | 7 | 13963 | 6 | 10 | 3 |
| `mc-ode` | 7 | 4990 | 8 | 9 | 3 |
| `mc-quad` | 7 | 8333 | 7 | 9 | 3 |
| `mc-roots` | 7 | 6704 | 6 | 9 | 3 |
| `mc-rules` | 7 | 2459 | 5 | 8 | 1 |
| `mc-simplify` | 7 | 8560 | 6 | 8 | 5 |
| `mc-algebra` | 6 | 5543 | 6 | 7 | 7 |
| `mc-decide` | 6 | 3776 | 6 | 7 | 7 |
| `mc-fields` | 6 | 4448 | 6 | 7 | 3 |
| `mc-integrate` | 6 | 3899 | 5 | 7 | 2 |
| `mc-numfn` | 6 | 3303 | 6 | 8 | 7 |
| `mc-dist` | 5 | 5432 | 7 | 7 | 3 |
| `mc-jit` | 5 | 9329 | 7 | 7 | 4 |
| `mc-notebook` | 5 | 12418 | 9 | 13 | 1 |
| `mc-poly` | 5 | 12807 | 5 | 6 | 12 |
| `mc-io` | 4 | 4519 | 5 | 6 | 4 |
| `mc-nbmodel` | 4 | 8777 | 4 | 6 | 1 |
| `mc-numtheory` | 4 | 7903 | 5 | 5 | 5 |
| `mc-special` | 4 | 36178 | 5 | 5 | 6 |
| `mc-assoc` | 3 | 4705 | 3 | 3 | 4 |
| `mc-combinat` | 3 | 497 | 3 | 4 | 3 |
| `mc-cover30` | 3 | 840 | 3 | 4 | 3 |
| `mc-edit` | 3 | 3879 | 2 | 4 | 1 |
| `mc-graphics` | 3 | 12648 | 6 | 6 | 5 |
| `mc-groups` | 3 | 4538 | 2 | 3 | 3 |
| `mc-nbexport` | 3 | 3010 | 4 | 5 | 2 |
| `mc-numeric` | 3 | 11068 | 4 | 4 | 17 |
| `mc-random` | 3 | 1404 | 4 | 4 | 6 |
| `mc-strings` | 3 | 2669 | 4 | 4 | 3 |
| `mc-boxes` | 2 | 8208 | 2 | 3 | 5 |
| `mc-eval` | 2 | 15675 | 2 | 2 | 34 |
| `mc-load` | 2 | 1721 | 2 | 2 | 2 |
| `mc-arith` | 1 | 7597 | 1 | 1 | 26 |
| `mc-core` | 1 | 5955 | 1 | 1 | 34 |
| `mc-syntax` | 1 | 9421 | 1 | 1 | 17 |
| `mc-ui` | 1 | 1551 | 1 | 1 | 2 |
| `mc-api` | 0 | 965 | 0 | 0 | 37 |
| `mc-bench` | 0 | 5009 | 0 | 0 | 4 |
| `mc-diffalg` | 0 | 197 | 0 | 0 | 3 |
| `mc-help` | 0 | 713 | 0 | 0 | 3 |
| `mc-langlib` | 0 | 413 | 0 | 0 | 3 |
| `mc-mathfont` | 0 | 736 | 0 | 0 | 5 |
| `mc-mechanics` | 0 | 241 | 0 | 0 | 3 |
| `mc-nbfile` | 0 | 2218 | 0 | 0 | 6 |

56 crates, 378140 lines, 12 layers (0 to 11)

Two rules the round sets out to make true did not hold: `mc-notebook` reached `mc-eval` and `mc-core` through
`mc-graphics` (colours, themes and the viewer live beside the graphics term), and `mc-engine`, `mc-kernel`, `mc-repl`
and `mc-docs` depended on the benchmark runner `mc-bench` for the `Engine` trait.

## 2. End state (round 34)

| crate | layer | lines | direct | trans | fan-in |
|---|---:|---:|---:|---:|---:|
| `mc-docs` | 14 | 5590 | 10 | 58 | 0 |
| `mc-repl` | 14 | 1420 | 6 | 53 | 0 |
| `mc-bench` | 13 | 4843 | 2 | 50 | 0 |
| `mc-kernel` | 13 | 2174 | 5 | 51 | 2 |
| `mc-engine` | 12 | 7805 | 12 | 49 | 4 |
| `mc-lsp` | 12 | 2120 | 5 | 50 | 0 |
| `mc-libraries` | 11 | 194 | 38 | 47 | 3 |
| `mc-transforms` | 10 | 4996 | 8 | 16 | 1 |
| `mc-integrals` | 9 | 11106 | 12 | 14 | 2 |
| `mc-dsolve` | 8 | 4762 | 5 | 9 | 1 |
| `mc-limit` | 8 | 4904 | 7 | 9 | 3 |
| `mc-plot` | 8 | 15006 | 9 | 17 | 3 |
| `mc-solve` | 8 | 9535 | 9 | 10 | 1 |
| `mc-sums` | 8 | 7801 | 7 | 13 | 1 |
| `mc-cad` | 7 | 6182 | 7 | 9 | 2 |
| `mc-calculus` | 7 | 12938 | 6 | 8 | 5 |
| `mc-geom` | 7 | 6520 | 5 | 10 | 2 |
| `mc-integrate-rules` | 7 | 2459 | 5 | 8 | 1 |
| `mc-linalg` | 7 | 6482 | 7 | 8 | 1 |
| `mc-lists` | 7 | 13963 | 6 | 10 | 1 |
| `mc-ode` | 7 | 4990 | 8 | 9 | 1 |
| `mc-quad` | 7 | 8333 | 7 | 9 | 1 |
| `mc-roots` | 7 | 6704 | 6 | 9 | 1 |
| `mc-simplify` | 7 | 8560 | 6 | 8 | 3 |
| `mc-algebra` | 6 | 5543 | 6 | 7 | 5 |
| `mc-decide` | 6 | 3776 | 6 | 7 | 7 |
| `mc-fields` | 6 | 4448 | 6 | 7 | 1 |
| `mc-integrate` | 6 | 3899 | 5 | 7 | 2 |
| `mc-numfn` | 6 | 3303 | 6 | 8 | 7 |
| `mc-dist` | 5 | 5432 | 7 | 7 | 1 |
| `mc-jit` | 5 | 9329 | 7 | 7 | 4 |
| `mc-notebook` | 5 | 12418 | 9 | 11 | 1 |
| `mc-poly` | 5 | 12807 | 5 | 6 | 13 |
| `mc-graphics-kernel` | 4 | 2498 | 5 | 7 | 2 |
| `mc-io` | 4 | 4519 | 5 | 6 | 2 |
| `mc-nbmodel` | 4 | 8777 | 4 | 6 | 1 |
| `mc-numtheory` | 4 | 7903 | 5 | 5 | 3 |
| `mc-special` | 4 | 36178 | 5 | 5 | 4 |
| `mc-assoc` | 3 | 4705 | 3 | 3 | 3 |
| `mc-combinat` | 3 | 497 | 3 | 4 | 1 |
| `mc-edit` | 3 | 3879 | 2 | 4 | 1 |
| `mc-further-functions` | 3 | 840 | 3 | 4 | 1 |
| `mc-graphics` | 3 | 10175 | 4 | 4 | 3 |
| `mc-groups` | 3 | 4538 | 2 | 3 | 1 |
| `mc-nbexport` | 3 | 3010 | 4 | 5 | 2 |
| `mc-numeric` | 3 | 11068 | 4 | 4 | 16 |
| `mc-random` | 3 | 1404 | 4 | 4 | 4 |
| `mc-strings` | 3 | 2669 | 4 | 4 | 1 |
| `mc-boxes` | 2 | 8208 | 2 | 3 | 5 |
| `mc-eval` | 2 | 15675 | 2 | 2 | 39 |
| `mc-load` | 2 | 1721 | 2 | 2 | 2 |
| `mc-arith` | 1 | 7597 | 1 | 1 | 30 |
| `mc-core` | 1 | 5955 | 1 | 1 | 39 |
| `mc-langlib` | 1 | 413 | 1 | 1 | 1 |
| `mc-syntax` | 1 | 9421 | 1 | 1 | 18 |
| `mc-ui` | 1 | 1551 | 1 | 1 | 2 |
| `mc-api` | 0 | 1149 | 0 | 0 | 46 |
| `mc-diffalg` | 0 | 197 | 0 | 0 | 1 |
| `mc-help` | 0 | 713 | 0 | 0 | 3 |
| `mc-mathfont` | 0 | 736 | 0 | 0 | 5 |
| `mc-mechanics` | 0 | 241 | 0 | 0 | 1 |
| `mc-nbfile` | 0 | 2218 | 0 | 0 | 6 |
62 crates, 378797 lines, 15 layers (0 to 14)

New crates: `mc-graphics-kernel` (step 3), `mc-libraries` (step 4), `mc-limit`, `mc-integrals`, `mc-transforms`,
`mc-dsolve` (step 5). Renamed: `mc-rules` is `mc-integrate-rules` (step 6), `mc-cover30` is
`mc-further-functions` (step 7). The layers grew from 12 to 15 because the registry and the calculus crates add
levels between the libraries and the engine; the largest crate is now `mc-special` (36k lines), then `mc-eval`,
`mc-plot`, `mc-lists` and `mc-calculus` (13k, from 38k).

The rules of `tools/depcheck-rules.txt` that hold: nothing but dev-dependencies depends on `mc-bench`; the UI crates
(`mc-notebook`, `mc-nbmodel`, `mc-nbexport`, `mc-ui`, `mc-boxes`, `mc-edit`) and `mc-graphics` depend on neither
`mc-eval` nor `mc-core`; `mc-calculus` depends on none of the calculus crates above it, and neither `mc-limit` nor `mc-dsolve` on
`mc-integrals`.

Step 8 of the round (output typesetting out of `mc-boxes` into a crate of its own) was not done in this round:
`mc-boxes` was being changed by other work of the round at the same time; round 38 did it (section 4).

### The calculus crates (step 5)

| crate | lines | holds | depends on (calculus) |
|---|---:|---|---|
| `mc-calculus` | 12938 | `d`, the derivative operator, the rule table, `apart`, properties of functions, optimisation; the helpers of every calculus crate (`common.rs`), the transforms by name; the `FUNCTIONS` table of the whole area and the calculus library `lib/calculus.mc` | (none) |
| `mc-limit` | 4904 | `limit`, `max_limit`, `min_limit`, `discrete_limit`, `asymptotic`; real bounds (`bounds.rs`) and functions defined by cases (`switches.rs`), shared with definite integrals | `mc-calculus` |
| `mc-dsolve` | 4762 | `d_solve`, `d_solve_value`, `asymptotic_d_solve_value`, `r_solve`, `r_solve_value` | `mc-calculus` |
| `mc-integrals` | 11106 | `integrate` (antiderivatives, definite integrals), `risch_integrate` | `mc-calculus`, `mc-limit` (and `mc-integrate`, `mc-integrate-rules`) |
| `mc-transforms` | 4996 | the Laplace, Mellin, Fourier and Z transforms and their inverses, `discrete_delta`, `kronecker_delta` | `mc-calculus`, `mc-limit`, `mc-integrals` |

The cut follows the imports: definite integrals take limits at their bounds and critical points, and limits used
four helpers of the definite integrals (the real bound `Pt`, `sign`, `subst`, `resolve_near`); those and what they
need moved down into `mc-limit`, so integration sits above limits and nothing is cyclic. The derivative rules used
two transform helpers (the derivatives of `meijer_g` and `fox_h` in z) and the derivative operator the transforms'
names: both moved down into `mc-calculus`. Differential equations and the properties of functions use other areas
only through the evaluator (by name), so they need no crate dependency.

## 3. What depends on what, and why

- **Contract and core.** `mc-api` (handles, views, the `Store` trait, the `Engine` trait and `StopFlag`) is the leaf
  everything builds on. `mc-core` (the store), `mc-arith` (canonical arithmetic) and `mc-syntax` (reader, printer)
  sit on it; `mc-eval` (matcher, evaluator) on `mc-core` and `mc-arith`.
- **Libraries.** Every library crate depends on `mc-eval` and `mc-core` to register natives, on `mc-arith` for
  numbers, and on the few libraries whose Rust functions it calls directly (`mc-poly` is used by 13, `mc-numeric`
  by 16); everything else a library needs from another it calls through the evaluator by name, which keeps the
  graph shallow. `mc-libraries` is the one crate that knows them all: their installation in a kernel, the
  libraries written in MathCAS in load order and the `FUNCTIONS` tables.
- **Assembly.** `mc-engine` (the session behind the `Engine` trait) depends on `mc-libraries` and on the few
  libraries it calls itself (plots, series and uncertain arithmetic, the compiler's queue, dataset tables);
  `mc-kernel` (the notebook protocol), `mc-repl` and `mc-bench` (the benchmark runner) on `mc-engine`;
  `mc-lsp` and `mc-docs` on `mc-libraries` for the names and descriptions.
- **Front ends.** The UI crates (`mc-ui`, `mc-boxes`, `mc-typeset`, `mc-edit`, `mc-nbmodel`, `mc-nbexport`, `mc-notebook`) talk to the
  kernel over the protocol and never link the evaluator: they use `mc-graphics` (the graphics model read through
  the store contract, colours, themes, renderers, the 3D viewer), `mc-syntax` and `mc-boxes`. The natives of the
  graphics term are `mc-graphics-kernel`'s, which the libraries install.

## 4. Step 8: output typesetting in a crate of its own (round 38)

`mc-boxes` read texts through the parser into box trees (printed output, linear input) beside the box tree and its
layout. Those readers are now the crate `mc-typeset`, so `mc-boxes` is the box tree, its canonical and linear text,
the alias table, LaTeX of a tree and the layout. No behaviour changed: the same functions under a new crate name, the
same tests (moved with the code they test).

| crate | layer | lines | direct | trans | fan-in | holds |
|---|---:|---:|---:|---:|---:|---|
| `mc-boxes` | 2 | 5878 | 2 | 3 | 6 | the box tree (`tree`), canonical text (`canonical`, `scan`), linear text with `restrict` and `linear_segments` (`linear`), aliases, LaTeX of a tree (`latex`), the layout and line breaking (`layout`) |
| `mc-typeset` | 3 | 2637 | 2 | 4 | 4 | `math_text` and `label_rows` (`math`: the notebook's one look of an output), `typeset_output`, `typeset_input`, `output_input_text`, `output_input_boxes` (`printed`) |

(before: `mc-boxes` 8486 lines, fan-in 5.) Its users: `mc-graphics` (plot labels), `mc-nbmodel` (outputs, pasted
text), `mc-nbexport` and `mc-notebook` (painting outputs); `mc-edit` uses it in its tests only. Three helpers the
readers shared with the tree stayed below: `has_boxes` (in `tree`), `is_swatch` (in `ops`) and `is_one_group` (the
canonical text's bracket scan, for the parentheses of a slot). `mc-boxes` keeps `mc-syntax` for the string literals
of the canonical text and the parse that decides the parentheses of the linear text.

New rules in `tools/depcheck-rules.txt`: neither `mc-boxes` nor `mc-edit` depends on `mc-typeset` (the layout and the
editor know no reader of outputs), and `mc-typeset` depends on neither `mc-eval` nor `mc-core`. The layout tests of
`mc-boxes` lay out printed output and take `mc-typeset` as a dev-dependency (dev-dependencies do not count). 63
crates, 15 layers; 25 rules, 0 violations.
