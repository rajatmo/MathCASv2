#! mathcas 2
# Sample case for the stub engine: the loop polls the stop flag, so the watchdog stops it cleanly at the time limit.
#@ title: Busy loop past its time limit (stub)
#@ stresses: cooperative stop at the time limit, reported as limit
#@ check: none
#@ limits: time 300ms, memory 256MB
#@ measure
busy(10^15)
