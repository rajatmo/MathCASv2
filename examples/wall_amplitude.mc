#! mathcas 2
# Spinor bilinears for a fermion crossing a rippled bubble wall (a port of a short research notebook).
#
# Incoming and outgoing spinors g(s), h(s) (positive and negative frequency, helicity s = 1, 2) and ga(r), ha(r) of
# the second particle, with real normalisations c1, c2, velocity factors t1, t2 and mode coefficients u1c, v1c,
# u2c, v2c; the amplitude B(s, r) = sum of the four bilinears psi1^dagger gamma^0 psi2 with all symbols real.
# Run: mc-repl examples/wall_amplitude.mc

fn g(s) = if s == 1 then c1*[0, t1, 1, 0] else c1*[t1, 0, 0, 1]
fn h(s) = if s == 1 then im*c1*[1, 0, 0, t1] else -im*c1*[0, 1, t1, 0]
fn ga(r) = if r == 1 then c2*[1, 0, 0, -t2] else c2*[0, 1, -t2, 0]
fn ha(r) = if r == 1 then im*c2*[0, t2, -1, 0] else im*c2*[-t2, 0, 0, 1]
gamma0 = diagonal_matrix([1, 1, -1, -1])
gamma3 = [[0, 0, 1, 0], [0, 0, 0, -1], [-1, 0, 0, 0], [0, 1, 0, 0]]

# psi1^dagger gamma^0 psi2 for column vectors (the conjugate transpose of a vector is its conjugate)
fn bil(a, b) = dot(conjugate(a), gamma0, b)
fn amp(s, r) = simplify(complex_expand(u1c*u2c*bil(g(s), ga(r)) + u1c*v2c*bil(g(s), ha(r)) +
                                       v1c*u2c*bil(h(s), ga(r)) + v1c*v2c*bil(h(s), ha(r))))
stage = timing(map(sr => [sr, factor(amp(sr[1], sr[2]))], [[1, 1], [2, 2], [1, 2], [2, 1]]))
for row in stage[2] { say("B", row[1], " = ", row[2]) }
say("time (s): ", stage[1])
