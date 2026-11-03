#! mathcas 2
#@ title: Digits of pi
#@ stresses: binary splitting, multiplication
#@ note: references for 10^5 and 10^6 digits are generated outside the repository (too large to store)
#@ sizes: d = 10^4, 10^5, 10^6
#@ check: digits 10000
#@ expect 10^4: @ref/N1-pi-1e4.txt
#@ measure
n(pi, d)
