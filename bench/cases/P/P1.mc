#! mathcas 2
#@ title: Dense multivariate multiplication, three variables
#@ stresses: multivariate multiplication
#@ sizes: m = 10, 20, 30
#@ check: property
#@ setup
f = expand((1 + x + y + z)^m)
#@ measure
expand(f*(f + 1))
#@ property
length(result) == binomial(2m + 3, 3) and rewrite(result, [x -> 1, y -> 1, z -> 1]) == 4^m*(4^m + 1)
