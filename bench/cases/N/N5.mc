#! mathcas 2
#@ title: Exact factorial
#@ stresses: product trees
#@ sizes: m = 10^4, 10^5, 10^6
#@ check: property
#@ measure
factorial(m)
#@ property
# Legendre: the exponent of a prime p in m! is (m - digit sum of m in base p)/(p - 1); and the digit count.
integer_exponent(result, 2) == m - sum(integer_digits(m, 2)) and integer_exponent(result, 5) == (m - sum(integer_digits(m, 5)))/4 and integer_length(result) == [35660, 456574, 5565709][integer_length(m) - 4]
