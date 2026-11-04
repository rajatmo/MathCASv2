#! mathcas 2
#@ title: An integral to 50 digits (Serret's integral)
#@ stresses: high-precision quadrature (tanh-sinh or rigorous Gauss-Legendre with ball error bounds)
#@ note: Package N2 (n_integrate, tanh-sinh on balls). integral_0^1 log(1 + x)/(1 + x^2) dx = pi log(2)/8 (Serret; substitute x = tan t, then t -> pi/4 - t). Reference: the closed form in mpmath at 520 digits (ref/L6h-50.txt); the closed form was checked against mpmath's quadrature at 60 digits to 55. The 500-digit case is L6h500; the rigorous enclosure is L6h500r.
#@ check: digits 50
#@ memo: off
#@ expect: @ref/L6h-50.txt
#@ measure
n_integrate(log(1 + x)/(1 + x^2), [x, 0, 1], digits: 50)
