#! mathcas 2
#@ title: Monte Carlo estimate of pi with the explicit generator
#@ stresses: random generation (mc-random), a loop over blocks of draws
#@ note: hits of m uniform points in the unit square inside the quarter disc, drawn in blocks of 10^5 pairs. Checked by a property (4 hits/m within 5 standard deviations of pi), which does not depend on the generator's bits; the seeded count itself is reproducible (decision RNG.2). Compiled code cannot draw yet (numerics roadmap N4), so 10^7 and 10^8 are expected to reach the time limit until then.
#@ sizes: m = 10^6, 10^7, 10^8
#@ check: property
#@ reps: 3, 1s
#@ limits: time 40s, memory 4GB
#@ memo: off
#@ setup
fn inside(p) = if p[1]^2 + p[2]^2 < 1.0 then 1 else 0
#@ measure
{ seed(2026); var hits = 0; for c in range(m/10^5) { hits = hits + sum(map(inside, random_real(1.0, [10^5, 2]))) }; hits }
#@ property
abs(4.0*result/m - n(pi)) < 5.0*sqrt(n(pi)*(4 - n(pi))/m)
