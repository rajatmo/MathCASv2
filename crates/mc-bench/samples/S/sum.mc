#! mathcas 2
# Sample case for the stub engine: a property check, with a fresh session for every repetition.
#@ title: Sum of a range (stub)
#@ stresses: list building, the property checker, fresh sessions
#@ sizes: n = 10^3, 10^5
#@ check: property
#@ session: fresh
#@ limits: time 20s, memory 512MB
#@ reps: 3, 50ms
#@ setup
offset = 0
#@ measure
sum(range(n)) + offset
#@ property
result == n*(n + 1)/2
