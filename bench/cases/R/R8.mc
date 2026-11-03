#! mathcas 2
#@ title: Expansion as plain terms
#@ stresses: canonicaliser, like-term collection
#@ note: run with polynomial packing disabled once the option exists
#@ sizes: m = 10, 20, 30
#@ check: property
#@ measure
expand((a + b + c + d)^m)
#@ property
# One term per monomial of degree n in four variables; the coefficients sum to 4^n.
length(result) == binomial(m + 3, 3) and rewrite(result, [a -> 1, b -> 1, c -> 1, d -> 1]) == 4^m
