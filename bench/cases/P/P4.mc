#! mathcas 2
#@ title: Multivariate gcd of two products sharing a large factor
#@ stresses: gcd algorithms
#@ sizes: m = 5, 10, 15
#@ check: property
#@ setup
g = expand((1 + x + y + z)^m + 3)
p = expand(g*(x^2 + y*z + 1)^2)
q = expand(g*(x*y - z + 5)^3)
#@ measure
gcd(p, q)
#@ property
# The cofactors are coprime, so the gcd is g up to sign.
expand(result - g) == 0 or expand(result + g) == 0
