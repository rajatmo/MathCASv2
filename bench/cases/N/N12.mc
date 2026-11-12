#! mathcas 2
#@ title: Ball comparisons that need refinement
#@ stresses: refinement loop, termination
#@ check: exact
#@ expect: [true, true, true]
#@ measure
[exp(pi*sqrt(163)) < 640320^3 + 744, pi^4 + pi^5 < exp(6), sqrt(1000001) - 1000 < 1/2000]
