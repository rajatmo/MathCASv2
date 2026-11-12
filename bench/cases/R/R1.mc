#! mathcas 2
#@ title: Naive recursive Fibonacci
#@ stresses: call and rule-dispatch overhead
#@ sizes: m = 20, 25, 30
#@ check: exact
#@ note: memo off, so the cache of normal forms does not make naive recursion linear
#@ memo: off
#@ expect 20: 6765
#@ expect 25: 75025
#@ expect 30: 832040
#@ setup
rule fib(0) = 0
rule fib(1) = 1
rule fib(k_: Int) := fib(k - 1) + fib(k - 2)
#@ measure
fib(m)
