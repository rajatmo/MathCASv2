#! mathcas 2
#@ title: Mandelbrot escape counts with complex arithmetic, 256 iterations
#@ stresses: compiled loops with machine complex numbers (mc-jit: z^2 + c and abs(z) inline on the parts)
#@ note: the total escape count over an m x m grid of [-2, 1] x [-1.5, 1.5] (point (i, j) is c = -2 + 3i/m + (-1.5 + 3j/m) im). The counts are exact: the compiled code computes the interpreter's complex arithmetic bit for bit (z^2 is z*z, abs(z) is (re^2 + im^2)^(1/2)); the expected totals agree with the interpreter at m = 40 and 200 and with an independent reimplementation of the same IEEE operations at 40, 200 and 1000. memo off, so repetitions run the compiled loop.
#@ sizes: m = 200, 1000, 3000
#@ check: exact
#@ reps: 3, 1s
#@ limits: time 60s, memory 4GB
#@ memo: off
#@ expect 200: 1907889
#@ expect 1000: 47555025
#@ expect 3000: 427703486
#@ setup
@compiled fn escape(cr: Real, ci: Real) = {
  let c = complex(cr, ci)
  var z = complex(0.0, 0.0)
  var k = 0
  while k < 256 and abs(z) <= 2.0 {
    z = z^2 + c
    k = k + 1
  }
  k
}
@compiled fn grid(m: Int) = {
  var total = 0
  for i in range(m) {
    for j in range(m) {
      total = total + escape(-2.0 + 3.0*i/m, -1.5 + 3.0*j/m)
    }
  }
  total
}
#@ measure
grid(m)
