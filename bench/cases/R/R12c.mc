#! mathcas 2
#@ title: Compiled loops: longest Collatz chain
#@ stresses: the numeric compiler (R12 with typed functions and @compiled)
#@ sizes: m = 10^4, 10^5, 10^6
#@ check: exact
#@ note: memo off, so repetitions run the compiled loop instead of finding the result in the cache of normal forms
#@ memo: off
#@ expect 10^4: 261
#@ expect 10^5: 350
#@ expect 10^6: 524
#@ setup
@compiled fn collatz_steps(k: Int) = {
  var c = k
  var s = 0
  while c != 1 {
    c = if mod(c, 2) == 0 then c/2 else 3c + 1
    s = s + 1
  }
  s
}
@compiled fn longest(m: Int) = {
  var best = 0
  for k in range(m) {
    best = max(best, collatz_steps(k))
  }
  best
}
#@ measure
longest(m)
