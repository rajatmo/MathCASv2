#! mathcas 2
#@ title: Integer factorisation of semiprimes
#@ stresses: ECM, quadratic sieve
#@ sizes: d = 30, 45, 60
#@ check: property
#@ setup
h = floor(d/2)
p = next_prime(2*10^(h - 1))
q = next_prime(5*10^(d - h - 1))
#@ measure
factor_integer(p*q)
#@ property
result == [[p, 1], [q, 1]]
