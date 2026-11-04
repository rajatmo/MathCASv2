#! mathcas 2
#@ title: A guaranteed enclosure to 500 digits (Serret's integral)
#@ stresses: rigorous quadrature: Clenshaw-Curtis on balls with Bernstein-ellipse error bounds from complex-ball evaluation
#@ note: Package N2 (n_integrate, rigorous: true). The ball printed contains pi log(2)/8: the printer shows only correct digits of a guaranteed ball. Reference: the closed form in mpmath at 520 digits (ref/L6h-500.txt).
#@ check: digits 500
#@ memo: off
#@ expect: @ref/L6h-500.txt
#@ measure
n_integrate(log(1 + x)/(1 + x^2), [x, 0, 1], digits: 500, rigorous: true)
