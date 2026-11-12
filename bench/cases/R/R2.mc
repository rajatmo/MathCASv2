#! mathcas 2
#@ title: Memoising Fibonacci
#@ stresses: definition creation, rule-set growth
#@ note: each repetition needs a fresh session, or the memo answers at once
#@ sizes: m = 10^3, 10^4, 10^5
#@ check: property
#@ session: fresh
#@ setup
rule fib(0) = 0
rule fib(1) = 1
# definitions return null (semantics 6.4): the block defines fib(k) and returns its value
rule fib(k_: Int) := { let v = fib(k - 1) + fib(k - 2); fib(k) = v; v }
#@ measure
fib(m)
#@ property
# Cassini's identity, with values the memo already holds.
result^2 - fib(m - 1)*fib(m + 1) == (-1)^(m - 1)
