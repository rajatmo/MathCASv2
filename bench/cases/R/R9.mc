#! mathcas 2
#@ title: Deep nesting
#@ stresses: stack safety, deep terms
#@ sizes: m = 10^4, 10^5, 10^6
#@ check: exact
#@ expect 10^4: 10001
#@ expect 10^5: 100001
#@ expect 10^6: 1000001
#@ measure
{ let t = nest(f, x, m); depth(t) }
