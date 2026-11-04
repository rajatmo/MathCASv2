#! mathcas 2
#@ title: Stiff ODE: Robertson's chemical kinetics
#@ stresses: an implicit method (Radau IIA or Rosenbrock), Jacobians from d, dense linear solves
#@ note: n_dsolve returns rules [y -> interpolating_function(...)] (package N3), applied with rewrite. References: scipy 1.18 solve_ivp with Radau at rtol 1e-12 (absolute tolerances 1e-14, 1e-20, 1e-14) and the analytic Jacobian; BDF and LSODA at the same tolerances agree to a relative 1.6e-11 (t = 10^2), 5.4e-11 (10^5) and 5.7e-7 (10^11). The property also checks y1 + y2 + y3 = 1, which the problem conserves.
#@ sizes: t1 = 10^2, 10^5, 10^11
#@ check: property
#@ memo: off
#@ setup
robertson = [y1'(t) == -0.04*y1(t) + 10^4*y2(t)*y3(t), y2'(t) == 0.04*y1(t) - 10^4*y2(t)*y3(t) - 3*10^7*y2(t)^2, y3'(t) == 3*10^7*y2(t)^2, y1(0) == 1, y2(0) == 0, y3(0) == 0]
fn reference(s) = if s == 10^2 then [6.172348823960893e-01, 6.153591274639171e-06, 3.827589640126364e-01] else if s == 10^5 then [1.786592114209984e-02, 7.274751468436474e-08, 9.821340061103856e-01] else [2.083340144135731e-08, 8.333360748072538e-14, 9.999999791665144e-01]
fn tolerance(s) = if s > 10^6 then 1e-5 else 1e-8
#@ measure
{ let s = n_dsolve(robertson, [y1, y2, y3], [t, 0, t1], accuracy_goal: 14, precision_goal: 10); rewrite([y1(t1), y2(t1), y3(t1)], s) }
#@ property
abs(result[1] + result[2] + result[3] - 1) < 1e-9 and max(map(k => abs(result[k] - reference(t1)[k])/reference(t1)[k], range(3))) < tolerance(t1)
