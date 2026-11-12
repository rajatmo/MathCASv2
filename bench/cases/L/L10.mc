#! mathcas 2
#@ title: Monte Carlo estimate of pi with the explicit generator
#@ stresses: random generation in a compiled loop (mc-jit draws from mc-random's session stream)
#@ note: hits of m uniform points in the unit square inside the quarter disc, after seed(2026). The compiled loop draws exactly the interpreter's numbers (decision N4b.2), so the seeded count is exact: 786153 at 10^6 is also what the interpreted loop gives, and what the earlier list form of this case (sum(map(inside, random_real(1.0, [10^5, 2]))) in blocks) counted. The property 4 hits/m within 5 standard deviations of pi holds at every size.
#@ sizes: m = 10^6, 10^7, 10^8
#@ check: exact
#@ reps: 3, 1s
#@ limits: time 40s, memory 4GB
#@ memo: off
#@ expect 10^6: 786153
#@ expect 10^7: 7854611
#@ expect 10^8: 78542706
#@ setup
@compiled fn hits(m: Int) = {
  var h = 0
  for k in range(m) {
    let x = random_real()
    let y = random_real()
    if x^2 + y^2 < 1.0 then h = h + 1
  }
  h
}
#@ measure
{ seed(2026); hits(m) }
