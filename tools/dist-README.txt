MathCAS v2 - development build
==============================

This is an early build of the v2 core: the language reads, evaluates and prints; the library is growing (see "What
works" below). VERSION.txt names the commit.

mc-repl.exe     the interactive REPL
mc-bench.exe    the benchmark harness (runs the cases under bench\cases)

REPL
----
Run mc-repl.exe and type expressions. Commands: :help, :time (toggle timing), :history, :redo [k], :cancel, :quit.
Input continues on the next line while brackets are open. Ctrl+C stops a running evaluation.

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
  loading .mc files and a startup prelude; modules and imports;
  the JIT is built but not wired in (loops are interpreted, about 1 us per iteration);
  series results print in their raw internal form.

Benchmarks
----------
  mc-bench.exe list
  mc-bench.exe run --cases bench\cases --only R1 --mem-ceiling 4GB
Every case runs in its own child process with a memory cap and a timeout and prints [k/N pct%] progress.
Last full run: 61 of 70 measurements pass their check, none wrong; 3 have no stored reference, 6 stop at a guard
(time, memory or evaluation depth). Some short medians (about 0.1 ms) are memo hits on repeated runs.
