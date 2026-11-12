#! mathcas 2
# Sample case for the stub engine: exact checks by fingerprint.
#@ title: Factorials (stub)
#@ stresses: the runner's hash checker
#@ sizes: n = 10, 25, 30
#@ check: exact hash
#@ expect 10: 356289db1850b01a
#@ expect 25: 0x098f0a369bb15eed
#@ expect 30: 1a19dd8e4cec87b6
#@ limits: time 10s, memory 256MB
#@ reps: 5, 50ms
#@ measure
fact(n)
