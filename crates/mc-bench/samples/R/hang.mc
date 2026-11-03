#! mathcas 2
# Sample case for the stub engine: the loop ignores the stop flag, so the guard has to kill the process.
#@ title: Loop that ignores the stop flag (stub)
#@ stresses: the guard's hard timeout, reported as limit and counted as a hang
#@ check: none
#@ limits: time 300ms, memory 256MB
#@ measure
busy_blind(10^15)
