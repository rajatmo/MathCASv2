#! mathcas 2
#@ title: An integral to 500 digits (Serret's integral)
#@ stresses: high-precision quadrature (tanh-sinh: digits double per level; rigorous Gauss-Legendre)
#@ note: Item N2 (n_integrate, tanh-sinh on balls). The integral of L6h50, pi log(2)/8, to 500 digits. Reference: the closed form in mpmath at 520 digits (ref/L6h-500.txt).
#@ check: digits 500
#@ memo: off
#@ expect: @ref/L6h-500.txt
#@ measure
n_integrate(log(1 + x)/(1 + x^2), [x, 0, 1], digits: 500)
