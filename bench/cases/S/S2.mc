#! mathcas 2
#@ title: Sort mixed terms by the canonical order
#@ stresses: canonical comparison
#@ sizes: m = 10^4, 10^5, 10^6
#@ check: property
#@ setup
xs = range(m) |> map(k => [k, x^mod(k, 7), f(mod(7919k, m)), string_join("s", to_string(mod(k, 101)))][mod(k, 4) + 1], _)
#@ measure
sort(xs)
#@ property
length(result) == m and ordered(result) and sort(reverse(xs)) == result
