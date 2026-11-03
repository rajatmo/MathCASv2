#! mathcas 2
#@ title: Power series of exp(sin(x))
#@ stresses: truncated series arithmetic
#@ sizes: m = 100, 1000, 10000
#@ check: property
#@ measure
normal(series(exp(sin(x)), x, 0, m))
#@ property
# The leading coefficients: 1 + x + x^2/2 - x^4/8 - x^5/15 - x^6/240 + x^7/90 + 31x^8/5760 + ...
[coefficient(result, x, 3), coefficient(result, x, 4), coefficient(result, x, 6), coefficient(result, x, 8)] == [0, -1/8, -1/240, 31/5760]
