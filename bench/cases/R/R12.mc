#! mathcas 2
#@ title: Interpreted loops: longest Collatz chain
#@ stresses: interpreter overhead
#@ sizes: m = 10^4, 10^5, 10^6
#@ check: exact
#@ expect 10^4: 261
#@ expect 10^5: 350
#@ expect 10^6: 524
#@ setup
fn collatz_steps(k: Int) = {
  var c = k
  var s = 0
  while c != 1 {
    c = if mod(c, 2) == 0 then c/2 else 3c + 1
    s = s + 1
  }
  s
}
#@ measure
range(m) |> map(collatz_steps, _) |> max
