#! mathcas 2
#@ title: An ODE solution value to 30 digits
#@ stresses: the high-precision Taylor series method (coefficients by recurrences: (y^2)_k = sum y_j y_(k-j))
#@ note: n_dsolve returns rules [y -> interpolating_function(...)] (package N3), applied with rewrite. y' = 1 + y^2, y(0) = 0 is y = tan(t), so y(3/2) = tan(3/2), near the pole at pi/2 (radius of convergence about 0.07 there). Reference: tan(3/2) in mpmath at 40 digits (ref/L3h-30.txt), checked against mpmath's Taylor-series ODE solver (odefun) to 33 digits.
#@ check: digits 30
#@ memo: off
#@ expect: @ref/L3h-30.txt
#@ measure
{ let s = n_dsolve([y'(t) == 1 + y(t)^2, y(0) == 0], y, [t, 0, 3/2], digits: 30); rewrite(y(3/2), s) }
