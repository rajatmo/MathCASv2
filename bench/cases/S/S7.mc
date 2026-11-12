#! mathcas 2
#@ title: Pattern-based selection over a list
#@ stresses: matching without rule sets
#@ sizes: m = 10^4, 10^5, 10^6
#@ check: exact
#@ expect 10^4: 3333
#@ expect 10^5: 33333
#@ expect 10^6: 333333
#@ setup
xs = range(m) |> map(k => if mod(k, 3) == 0 then f(k) else g(k), _)
#@ measure
length(cases(xs, f(k_: Int)))
