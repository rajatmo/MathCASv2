#! mathcas 2
#@ title: Resultant of dense polynomials
#@ stresses: subresultants or modular methods
#@ note: for monic f of degree n, resultant(f, f + c) is c^n
#@ sizes: m = 20, 50, 100
#@ check: exact
#@ expect 20: 79792266297612001
#@ expect 50: 1798465042647412146620280340569649349251249
#@ expect 100: 3234476509624757991344647769100216810857203198904625400933895331391691459636928060001
#@ setup
f = expand((x + 2)^m - x)
#@ measure
resultant(f, f + 7, x)
