#! mathcas 2
#@ title: Numeric integration of smooth and oscillatory integrands
#@ stresses: adaptive Gauss-Kronrod quadrature, compiled integrands (15-node batches)
#@ note: Item N2 (n_integrate, crates/mc-quad). The integrands exp(-x) cos(k x) on [0, pi] go from smooth (k = 1) to about 3000 oscillations (k = 10^4); adaptive Gauss-Kronrod with the 61-point rule on unresolved intervals, the integrand compiled. Reference: the closed form integral = (1 - (-1)^k e^-pi)/(1 + k^2) (two integrations by parts), summed in mpmath at 30 digits; the closed form was checked against mpmath's quadrature for k = 1, 7, 40 to 25 digits.
#@ sizes: m = 10^2, 10^3, 10^4
#@ check: digits 10
#@ memo: off
#@ expect 10^2: 1.0824513087312247700
#@ expect 10^3: 1.0914037645076189473
#@ expect 10^4: 1.0923032907109013786
#@ measure
sum(map(k => n_integrate(exp(-x)*cos(k*x), [x, 0, pi]), range(m)))
