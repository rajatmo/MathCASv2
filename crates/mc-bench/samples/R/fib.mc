#! mathcas 2
# Sample case for the stub engine: an exact check at three sizes.
#@ title: Fibonacci numbers (stub)
#@ stresses: the runner's exact checker
#@ sizes: n = 20, 50, 90
#@ check: exact
#@ expect 20: 6765
#@ expect 50: 12586269025
#@ expect 90: 2880067194370816120
#@ limits: time 10s, memory 256MB
#@ reps: 5, 50ms
#@ measure
fib(n)
