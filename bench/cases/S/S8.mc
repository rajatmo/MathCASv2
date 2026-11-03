#! mathcas 2
#@ title: Allocation stress: build distinct small terms
#@ stresses: hash-consing table, arena
#@ sizes: m = 10^5, 10^6, 10^7
#@ check: exact
#@ expect 10^5: 100000
#@ expect 10^6: 1000000
#@ expect 10^7: 10000000
#@ measure
range(m) |> map(k => h(k, k + 1), _) |> length
