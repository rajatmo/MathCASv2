#! mathcas 2
# Sample case for the stub engine: a stored value that is wrong on purpose, so the check reports fail.
#@ title: A failing check (stub)
#@ stresses: failed checks in results and summary
#@ check: exact
#@ expect: 4
#@ limits: time 10s, memory 256MB
#@ reps: 5, 50ms
#@ measure
2 + 3
