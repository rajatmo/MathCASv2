#! mathcas 2
#@ title: Exact harmonic numbers
#@ stresses: rational arithmetic, binary splitting
#@ sizes: m = 10^4, 10^5, 10^6
#@ check: property
#@ measure
range(m) |> map(k => 1/k, _) |> sum
#@ property
# The power of 2 in the denominator of H(m) is floor(log2(m)); the numerator is odd.
integer_exponent(denominator(result), 2) == integer_length(m, 2) - 1 and mod(numerator(result), 2) == 1
