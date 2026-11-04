#! mathcas 2
#@ title: Non-stiff ODE: the Lorenz system, tolerance 1e-10
#@ stresses: an explicit Runge-Kutta method (Dormand-Prince 5(4)), right-hand-side evaluation (compiled f(t, y))
#@ note: EXPECTED TO FAIL until package N3 (n_dsolve) lands: the call stays unevaluated and the property is false. The API is the one planned in docs/design/07-numerics.md (decision NUM.6: n_dsolve returns an interpolating_function); N3 may adjust the call. Lorenz is chaotic: an error of 1e-10 grows like exp(0.9 t), so only t = 10 is compared with a reference point (ref/L3-10.txt: mpmath's Taylor-series solver at 40 digits, agreeing with a 30-digit run to 20 digits); at t = 100 and 1000 the property checks that the point lies in the attractor's bounding box.
#@ sizes: t1 = 10, 100, 1000
#@ check: property
#@ memo: off
#@ setup
lorenz = [x'(t) == 10*(y(t) - x(t)), y'(t) == x(t)*(28 - z(t)) - y(t), z'(t) == x(t)*y(t) - 8/3*z(t), x(0) == 1, y(0) == 1, z(0) == 1]
reference = [-4.9026875411346457319, -3.7438729218029196163, 24.690858102790555453]
#@ measure
{ let s = n_dsolve(lorenz, [x, y, z], [t, 0, t1], accuracy_goal: 10, precision_goal: 10); [s[1](t1), s[2](t1), s[3](t1)] }
#@ property
if t1 == 10 then max(map(k => abs(result[k] - reference[k]), range(3))) < 1e-5 else abs(result[1]) < 25 and abs(result[2]) < 30 and 0 < result[3] and result[3] < 50
