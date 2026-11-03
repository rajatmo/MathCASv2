#! mathcas 2
# Sample case for the stub engine: the digits checker on an integer result (12 significant digits of 30!).
#@ title: Leading digits of a factorial (stub)
#@ stresses: the runner's digits checker
#@ check: digits 12
#@ expect: 2.65252859812191058636e32
#@ limits: time 10s, memory 256MB
#@ reps: 5, 50ms
#@ measure
fact(30)
