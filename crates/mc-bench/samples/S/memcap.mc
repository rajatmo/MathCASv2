#! mathcas 2
# Sample case for the stub engine: the list does not fit in the memory cap, so allocation fails in the guard.
#@ title: List larger than the memory cap (stub)
#@ stresses: the guard's memory cap, reported as limit
#@ check: none
#@ limits: time 20s, memory 64MB
#@ measure
sum(range(10^9))
