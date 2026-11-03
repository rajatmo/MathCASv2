MathCAS v2 - development build
==============================

This is an early build of the v2 core: the language reads, evaluates and prints, but the library is still small
(see "What works
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

Not yet (being built now)
-------------------------
  number theory (mod, gcd, factorial, primes, factor_integer);
  polynomials (expand, factor, series, resultant).

Benchmarks
----------
  mc-bench.exe list
  mc-bench.exe run --cases bench\cases --only R1 --mem-ceiling 4GB
Every case runs in its own child process with a memory cap and a timeout and prints [k/N pct%] progress.
Most cases still need library functions that are not in this build.
