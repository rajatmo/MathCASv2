# MathCAS v2

An independent computer algebra system built around one idea: **a small, fully specified core that combines fast
term rewriting with first-class arbitrary-precision and exact algebra.**

v2 is a new design, not a port. It has its own syntax, its own evaluation control and its own number model. A
compatibility layer for other languages may come later, as a front end that translates into the core, never as the
core's semantics.

![A notebook session: exact integration, typeset input with the typesetting keys, a plot with coordinate read-outs,
and solving an equation](docs/media/workflow.gif)

*A session in the MathCAS notebook: exact integration, typeset input written with `Ctrl+6` (power) and `Ctrl+/`
(fraction), a plot whose curve snaps to the pointer with its coordinates, and solving a cubic. Every frame is the real
program ([MP4 version](docs/media/workflow.mp4); recorded with [tools/demo/workflow.py](tools/demo/workflow.py)).*

```text
2^100 + 1/3                               # 3802951800684688204490109616129/3
factor(x^4 - 1)                           # (-1 + x)*(1 + x)*(1 + x^2)
d(sin(x)^2, x)                            # 2*cos(x)*sin(x)
integrate(1/(x^2 - 1), x)                 # (-1/2)*log(1 + x) + (1/2)*log(-1 + x)
series(exp(x), x, 0, 4)                   # 1 + x + x^2/2 + x^3/6 + x^4/24 + O(x^5)
sum(1/k^2, [k, 1, inf])                   # (1/6)*pi^2
n(sqrt(2), 50)                            # 1.4142135623730950488016887242096980785696718753769...
find_root(cos(x) == x, [x, 1])            # [x -> 0.7390851332151607]
n_integrate(exp(-x^2), [x, -inf, inf])    # 1.7724538509055163
range(5) |> map(k => k^2, _)              # [1, 4, 9, 16, 25]
```

## Goals

- **Simplicity:** few core primitives, every one of them specified; no behaviour that a reader of the spec could not
  predict.
- **Speed:** measured, not claimed, on a fixed benchmark suite (rewriting, polynomial algebra, big numbers, numeric
  loops, structural operations).
- **Predictability:** evaluation that terminates or reports; resource limits (time, memory, fuel) built in from the
  first line of code.

## Status

Early but usable for real numeric and symbolic work: a research notebook (a first-order phase transition: thermal
integrals, a bounce solution by shooting, its action and the wall profile) runs as
[examples/wall_profile.mc](examples/wall_profile.mc) in about 20 seconds. What exists today:

- **Language:** functions as ordered rule clauses with typed parameters and `when` guards, rules with sequence
  patterns, lambdas, pipes, `match`, blocks with `var`/`let`, `for`/`while`, modules with `pub` exports and
  `import`, a prelude written in the language itself ([lib/prelude.mc](lib/prelude.mc)).
- **Numbers:** exact integers and rationals, machine reals, arbitrary-precision balls (only correct digits are
  shown), complex numbers.
- **Symbolic:** polynomials (expand, factor, gcd, resultant), `simplify` with assumptions, `d`, `integrate`, `apart`,
  `limit`, `series`, symbolic `sum`, number theory, special functions (gamma family, zeta, beta, Bessel I and K).
- **Numeric:** `find_root`, `find_minimum`, `n_integrate`, `n_dsolve` (with events), `interpolation`, random numbers;
  numeric functions compile to machine code (Cranelift JIT) when they can, and packed real arrays avoid boxing.
- **Data and plots:** lists, associations and sets, strings; `import_data`/`export_data` (CSV, TSV, whitespace
  tables, lines, JSON, PNG); `plot`, `list_plot` and log variants rendered to PNG.
- **Tools:** a REPL and script runner, `.mcn` text notebooks, a kernel protocol (JSON lines), a Jupyter kernel, a
  language server with a VS Code extension, and a Windows installer.

Not there yet: linear algebra, `solve`, probability distributions, a graphics model (SVG, graphics primitives),
PDF export and a notebook window of its own. The plan is in [docs/design/06-roadmap.md](docs/design/06-roadmap.md).

## Getting started

Build with a recent stable Rust toolchain:

```bash
cargo build --release
```

Run the REPL, a script or a notebook:

```bash
cargo run --release -p mc-repl
cargo run --release -p mc-repl -- examples/hello.mc
cargo run --release -p mc-repl -- -e "factor(x^4 - 1)"
cargo run --release -p mc-repl -- --update examples/notebook.mcn
```

`mc-repl --help` lists the options. At the REPL, `:help` lists the commands and Ctrl+C stops a running evaluation.
The guide [docs/guide/quickstart.tex](docs/guide/quickstart.tex) walks through numeric work (roots, integrals,
differential equations, compiled functions).

A file may start with `#! mathcas 2`. Scripts are `.mc`; notebooks are `.mcn` (plain text: `#% cell`, `#% text` and
`#% output` sections). See [examples/](examples) for scripts, a module, a notebook and Jupyter notebooks.

### Editors and notebooks

- **Jupyter** (Notebook, JupyterLab, VS Code): [editors/jupyter](editors/jupyter) wraps `mc-kernel`; register it with
  `python editors/jupyter/install.py` (details in its `INSTALL.txt`). Plots show inline.
- **VS Code:** [editors/vscode](editors/vscode) is a dependency-free extension over `mc-lsp`: diagnostics while
  typing, go to definition, hover, completion, outline, and Run File / Run Selection.
- **Other editors** can start `mc-lsp` as a language server over stdio.

### Distribution

`powershell -File tools/dist.ps1` builds a release folder (programs, examples, guide, editor support) and
`tools/package.ps1` the Windows installer and zip. CI builds the installer
([.github/workflows/windows-installer.yml](.github/workflows/windows-installer.yml)).

## Repository layout

| Path | Contents |
|---|---|
| `crates/mc-api`, `mc-core` | the term-store contract; hash-consed arena terms, immediates, generational collector |
| `crates/mc-arith` | integers, rationals, balls (own NTT big-number layer), complex numbers |
| `crates/mc-syntax`, `mc-eval`, `mc-load` | parser and printer; explicit-stack evaluator with indexed rule dispatch; files, modules and prelude |
| `crates/mc-engine` | the assembled engine: registers every library crate into one session |
| `crates/mc-lists`, `mc-assoc`, `mc-strings` | lists, associations and sets, strings |
| `crates/mc-numeric`, `mc-numtheory`, `mc-poly` | numeric evaluation and precision, number theory, polynomials |
| `crates/mc-calculus`, `mc-simplify`, `mc-special` | derivatives, integrals, limits, series; simplification; special functions |
| `crates/mc-jit`, `mc-numfn` | the Cranelift compiler; compiled numeric functions and vectors |
| `crates/mc-roots`, `mc-quad`, `mc-ode`, `mc-random` | root finding and minimisation, numeric integration, differential equations, random numbers |
| `crates/mc-io`, `mc-plot` | data import and export, interpolation; plots |
| `crates/mc-repl`, `mc-kernel`, `mc-lsp`, `mc-bench` | REPL and script runner, kernel protocol, language server, benchmark harness |
| `lib/` | the prelude and standard modules, in MathCAS itself |
| `docs/design/` | design documents (below) |
| `bench/` | benchmark cases and results |
| `spikes/` | the measured prototypes the design was decided on (outside the workspace; build one in its own folder) |
| `editors/`, `examples/`, `tools/`, `assets/` | editor support, examples, build and packaging scripts, icons |

Most crates have a `NOTES.md` with their design decisions and known gaps.

## Design

The architecture was decided by measurement, top down: goals, a benchmark suite, a semantics specification, spikes
for the risky parts, then the build.

| Document | Subject |
|---|---|
| [00-plan](docs/design/00-plan.md) | method and starting architecture |
| [01-semantics](docs/design/01-semantics.md) | terms, numbers, evaluation order, holding, rules, strategies |
| [02-benchmarks](docs/design/02-benchmarks.md) | the benchmark suite, by workload class |
| [03-spikes](docs/design/03-spikes.md) | spike results and the decisions they settled |
| [04-syntax](docs/design/04-syntax.md) | the surface syntax |
| [05-assembly](docs/design/05-assembly.md) | crate layout and the interface contract |
| [06-roadmap](docs/design/06-roadmap.md) | implementation phases after the first milestone |
| [07-numerics](docs/design/07-numerics.md) | root finding, integration and differential equations |
| [08-notebook](docs/design/08-notebook.md) | the notebook window: document model, editing, typesetting, evaluation, themes |
| [09-docs](docs/design/09-docs.md) | function documentation pages and the documentation window |

## Testing and benchmarks

```bash
cargo test --release --workspace
cargo run --release -p mc-bench -- list
cargo run --release -p mc-bench -- run --cases bench/cases --only R1 --mem-ceiling 4GB
```

Every benchmark case runs in its own child process with a memory cap and a timeout.

## License

MathCAS v2 is free software: you can redistribute it and/or modify it under the terms of the GNU General Public
License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later
version (`GPL-3.0-or-later`; the text is in [LICENSE](LICENSE)). It is distributed in the hope that it will be useful,
but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.

The libraries it is built with keep their own licences (MIT, Apache-2.0, BSD, Zlib, ISC and similar, all compatible
with GPL-3.0); the bundled fonts are under the SIL Open Font License 1.1 (STIX Two Math), the Bitstream Vera licence
(DejaVu) and Apache-2.0 (Noto Sans), with their licence files next to them.
