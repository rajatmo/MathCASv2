#! mathcas 2
#@ title: Big-integer multiplication, division and gcd
#@ stresses: FFT multiplication, subquadratic division and gcd
#@ sizes: d = 10^5, 10^6, 10^7
#@ check: property
#@ setup
a = 7^d
b = 11^d + 1
#@ measure
{ let c = a*b; [quotient(c + 5, b), gcd(c, a^2)] }
#@ property
# 7 never divides 11^d + 1 (the powers of 4 modulo 7 are 4, 2, 1), so gcd(a*b, a^2) is a.
result == [a, a]
