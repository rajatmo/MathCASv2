#! mathcas 2
#@ title: Newton solves of a parametrised equation (Kepler's equation)
#@ stresses: find_root's machine Newton iteration; the function and its derivative evaluated per step (compiled or interpreted, mc-numfn)
#@ note: E - 0.3 sin(E) = M for m mean anomalies M in (0, 6]; each solve differentiates once and takes 4-6 Newton steps. The bodies differ only in their constants, so mc-numfn's code cache compiles the family once (constants lifted to parameters) after 32 evaluations of all solves together, and every later solve runs compiled from its first step. The option compile: false (or the environment variable MC_NUMFN = interpreted, compiled, auto:N) selects the policy for measurements.
#@ sizes: m = 10^3, 10^4, 10^5
#@ check: property
#@ reps: 3, 1s
#@ limits: time 60s, memory 4GB
#@ memo: off
#@ measure
map(k => rewrite(x, find_root(x - 0.3*sin(x) == 6.0*k/m, [x, 1.0])), range(m))
#@ property
# every root satisfies its equation to a few ulps (whole-list arithmetic: indexing result[k] in a loop is quadratic)
length(result) == m and max(abs(result - 0.3*sin(result) - 6.0*range(m)/m)) < 1e-14
