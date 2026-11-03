MathCAS v2 - development build
==============================

This is an early build of the v2 core: the language reads, evaluates and prints, but the library is still small
(see "What works" below). VERSION.txt names the commit.

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
  fact(30)
  rule f(a_, b__) := [a, b]                   rules with sequence patterns
  f(1, 2, 3)                                  [1, [2, 3]]
  1.25~40 * 2                                 ball (arbitrary precision) literals
  2 + 3im                                     complex numbers
  1/0                                         failure(division_by_zero)
  1 < 2 and not (3 < 2)                       comparisons and logic

Not yet (being built now)
-------------------------
  lambdas (x => ...), blocks with let/var, while/for, match;
  list functions (range, length, sum, select, sort, ...); number theory (mod, gcd, factorial, primes);
  polynomials (expand, factor, series); numerics (n(...), pi, sin, exp, ...).

Benchmarks
----------
  mc-bench.exe list
  mc-bench.exe run --cases bench\cases --only R1 --mem-ceiling 4GB
Every case runs in its own child process with a memory cap and a timeout and prints [k/N pct%] progress.
Most cases still need library functions that are not in this build.
