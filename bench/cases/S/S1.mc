#! mathcas 2
#@ title: Integer list: build, map, select, sum
#@ stresses: packed arrays, list primitives
#@ sizes: m = 10^5, 10^6, 10^7
#@ check: exact
#@ expect 10^5: 222225555588889
#@ expect 10^6: 222222555555888889
#@ expect 10^7: 222222255555558888889
#@ measure
range(m) |> map(k => k^2, _) |> select(_, k => mod(k, 3) == 1) |> sum
