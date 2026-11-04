#! mathcas 2
#@ title: Newton solves of a parametrised equation (Kepler's equation)
#@ stresses: find_root's machine Newton iteration; the function and its derivative evaluated per step (compiled or interpreted, mc-numfn)
#@ note: E - 0.3 sin(E) = M for m mean anomalies M in (0, 6]; each solve differentiates once and takes 4-6 Newton steps. The environment variable MC_NUMFN (interpreted, compiled, auto:N) selects mc-numfn's policy for measurements; the default compiles only after 32 evaluations, so these short solves stay interpreted.
#@ sizes: m = 10^3, 10^4, 10^5
#@ check: property
#@ reps: 3, 1s
#@ limits: time 60s, memory 4GB
#@ memo: off
#@ measure
map(k => rewrite(x, find_root(x - 0.3*sin(x) == 6.0*k/m, [x, 1.0])), range(m))
#@ property
# every root satisfies its equation to a few ulps
length(result) == m and max(map(k => abs(result[k] - 0.3*sin(result[k]) - 6.0*k/m), range(m))) < 1e-14
