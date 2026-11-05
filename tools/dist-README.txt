MathCAS v2 - development build
==============================

This is an early build of the v2 core: the language reads, evaluates and prints; the library is growing (see "What
works" below). VERSION.txt names the commit.

mc-notebook.exe the notebook window: .mcn notebooks with cells, In/Out labels, grouping, themes (mc-notebook file.mcn)
mc-repl.exe     the interactive REPL
mc-bench.exe    the benchmark harness (runs the cases under bench\cases)
mc-lsp.exe      the language server for editors (see "Editor support" below)
mc-kernel.exe   the kernel for notebooks and editors: JSON lines over stdio (protocol 1)
quickstart.pdf  a quick start guide (source: docs\guide\quickstart.tex in the repository)

REPL
----
Run mc-repl.exe and type expressions. Commands: :help, :time (toggle timing), :history, :redo [k], :cancel, :quit,
:load file.mc (evaluate a file; a module file again replaces its definitions), :modules (loaded modules).
Input continues on the next line while brackets are open. Ctrl+C stops a running evaluation.

Scripts and modules
-------------------
  mc-repl.exe examples\hello.mc          run a script: it prints its output and the value of its last statement
  mc-repl.exe a.mc b.mc                  several files in order, in one session
  mc-repl.exe -e "factor(x^4 - 1)"       evaluate source given on the command line (repeatable)
  mc-repl.exe -i examples\vectors.mc     enter the REPL afterwards
  mc-repl.exe --no-prelude               start without the prelude; --help lists the options
Exit codes: 0 ok, 1 read error (nothing of that file runs), 2 a top-level statement gave a failure, 130 interrupted.
A file may start with the line  #! mathcas 2 .
  module geo { pub fn area(r) = pi*r^2 }   a module; members marked pub are exported
  import geo                               bring its public names into scope (also import geo.{area}, import geo
                                           except area); geo.area(1) works as well
`import M` loads M.mc from the importing file's folder (the current folder at the REPL), or a standard module:
  import stats                             mean, median, variance, std_dev, geometric_mean
The prelude, loaded at startup, adds: is_even, is_odd, clamp, log_base, count_if, every, some, member, zip,
flat_map, accumulate, dot, norm, fix_point, say (print several values on one line). See examples\ for scripts.

Notebooks
---------
  mc-repl.exe examples\notebook.mcn            run a notebook's code cells, printing their output
  mc-repl.exe --update examples\notebook.mcn   also write each cell's output back into the file
A .mcn file is plain text: "#! mathcas 2 notebook", then cells marked "#% cell" (code), "#% text" (prose as
"# ..." lines) and "#% output" (the last run's output: "#|" print, "#=" value, "#!" failure or error).

Jupyter and VS Code notebooks
-----------------------------
jupyter\ holds a Jupyter kernel: .ipynb notebooks run MathCAS in Jupyter Notebook, JupyterLab and VS Code (Jupyter
extension; pick the kernel "MathCAS v2"). Needs Python with ipykernel; register it with
  python jupyter\install.py
(the installer offers this, as the default kernel). Plots (plot, list_plot) show as images in
notebook cells; in the REPL they print image("path.png"), a PNG file you can open.
Data files: import_data("file.csv") and export_data("file.csv", data) read and write lists as csv, tsv,
txt/dat (whitespace tables), lines and json; export_data("plot.png", plot(...)) saves a plot. jupyter\INSTALL.txt has the details; examples\jupyter.ipynb is a sample.

Editor support
--------------
editors\vscode is a VS Code extension (no dependencies): syntax colouring for .mc and .mcn, error diagnostics while
typing, go to definition (also into imported module files), hover, completion (including module members after
"M."), the outline, and the commands "MathCAS: Run File" and "MathCAS: Run Selection". With this folder on PATH:
  code --install-extension mathcas-0.1.0.vsix
or copy editors\vscode to %USERPROFILE%\.vscode\extensions\mathcas.mathcas-0.1.0 and restart VS Code
(editors\vscode\INSTALL.txt has the details and the settings). Other editors can start mc-lsp.exe as a language
server over stdio.

What works
----------
  2^100 + 1                                   exact big integers
  6/4 + 1/3                                   exact rationals (11/6)
  x + 2x + y*x                                symbolic collection (3*x + x*y)
  fn fact(0) = 1                              functions with literal clauses
  fn fact(k: Int) when k > 0 = k*fact(k - 1)  typed parameters and conditions
  rule f(a_, b__) := [a, b]                   rules with sequence patterns
  map(x => x^2, range(5))                     lambdas and list functions
  select(range(10), k => k > 6)
  sum(range(100)), sort([3, 1, 2]), flatten, transpose, part (xs[i]), nest, fold, ...
  { var s = 0; for k in range(10) { s = s + k }; s }    blocks, var/let, for/while
  match e { 0 => "zero", n_: Int => "int" }   match
  rewrite(e, [sin(a_)^2 + cos(a_)^2 -> 1])    rule application
  n(pi, 50), n(sqrt(2), 100)                  arbitrary precision (balls, only correct digits shown)
  sin(pi/6), exp(1.5), log(-1)                elementary functions, exact and numeric
  1.25~40 * 2, 2 + 3im                        ball literals, complex numbers
  print("hi")                                 output
  1/0                                         failure(division_by_zero)
  mod(17, 5), gcd(12, 18), factorial(30)      number theory
  is_prime(97), factor_integer(360)
  expand((x + 1)^5), factor(x^2 - 1)          polynomials
  resultant(x^2 - 2, x - y, x)

Not yet
-------
  nested modules, re-exports;
  the JIT is built but not wired in (loops are interpreted, about 1 us per iteration);
  series results print in their raw internal form.

Benchmarks
----------
  mc-bench.exe list
  mc-bench.exe run --cases bench\cases --only R1 --mem-ceiling 4GB
Every case runs in its own child process with a memory cap and a timeout and prints [k/N pct%] progress.
Last full run: 61 of 70 measurements pass their check, none wrong; 3 have no stored reference, 6 stop at a guard
(time, memory or evaluation depth). Some short medians (about 0.1 ms) are memo hits on repeated runs.
