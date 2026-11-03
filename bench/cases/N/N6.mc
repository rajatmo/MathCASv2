#! mathcas 2
#@ title: Prime counting and the k-th prime
#@ stresses: combinatorial prime counting, segmented sieve
#@ sizes: k = 8, 10, 12
#@ check: exact
#@ expect 8: [5761455, 2038074743]
#@ expect 10: [455052511, 252097800623]
#@ expect 12: [37607912018, 29996224275833]
#@ measure
[prime_pi(10^k), prime(10^k)]
