#! mathcas 2
#@ title: Univariate factorisation over the integers
#@ stresses: factorisation over Z
#@ note: add Swinnerton-Dyer polynomials of degree 32, 64 and 128 once products over sign choices are written
#@ sizes: d = 32, 64, 128
#@ check: property
#@ setup
p = expand((x^d + 2x + 1)*(x^(d/2) - 3x^2 + 7)*(x^3 + x + 1))
#@ measure
factor(p)
#@ property
expand(result) == p
