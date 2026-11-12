#! mathcas 2
#@ title: Dense multivariate multiplication, five variables
#@ stresses: large multiplication
#@ sizes: m = 10, 15, 20
#@ check: property
#@ setup
f = expand((1 + x + y + z + t)^m)
#@ measure
expand(f*(f + 1))
#@ property
length(result) == binomial(2m + 4, 4) and rewrite(result, [x -> 1, y -> 1, z -> 1, t -> 1]) == 5^m*(5^m + 1)
