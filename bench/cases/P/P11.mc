#! mathcas 2
#@ title: Univariate dense multiplication with large coefficients
#@ stresses: FFT-based multiplication
#@ sizes: m = 10^4, 10^5, 10^6
#@ check: property
#@ setup
p = range(m) |> map(k => (k + 10^50)*x^k, _) |> sum
s = rewrite(p, [x -> 1])
#@ measure
expand(p*p)
#@ property
length(result) == 2m - 1 and rewrite(result, [x -> 1]) == s^2
