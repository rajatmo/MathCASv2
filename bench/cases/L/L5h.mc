#! mathcas 2
#@ title: A root to 1000 digits (Kepler's equation)
#@ stresses: ball Newton refinement with precision doubling and interval Newton verification (mc-roots under digits:)
#@ note: the root of x - (3/10) sin(x) = 2 near 2. Reference: mpmath 1.4.1 findroot at 1100 digits, residual below 1e-1090, printed to 1000 digits (ref/L5h-1000.txt). The result keeps guard bits beyond 1000 digits (as n(x, d) does), so all 1000 asked digits print.
#@ check: digits 1000
#@ memo: off
#@ expect: @ref/L5h-1000.txt
#@ measure
rewrite(x, find_root(x - 3/10*sin(x) == 2, [x, 2], digits: 1000))
