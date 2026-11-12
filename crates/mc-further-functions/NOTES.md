# mc-further-functions: notes (round 30; `mc-cover30` until round 34)

Functions whose home is another crate, written here so that the other crates change only by one-line
registrations. Library code is `lib/further.mc` (helpers prefixed `c30_`); the machine kernels are natives
(`c30_julia_counts`, `c30_fox_machine`). Decisions are numbered e30.N.

## julia_set_iteration_count

- e30.1 The count is the first k at which the orbit z_k of z0 under the rational map f is *caught*: within
  delta_p = (1/32)/max(1, |f'(p)|)^2 of a point p of an attracting cycle, or beyond |z| = 32 when infinity is in an
  attracting cycle of a rational map, or beyond the escape radius R = max(4, 1 + sum |a_k/a_d|) for a polynomial of
  degree d >= 2 (every orbit beyond the Cauchy-type bound tends to infinity). Points of a cycle at a pole of f are
  not targets (the orbit is caught one step later, at infinity). Not caught within max_iterations: max_iterations +
  1. The constants give the expected counts of the documented examples (z^2 - 1, z^2/(z^2 - 1), z^3 - z + 1, z^2 + c).
- The attracting cycles come from the critical points (zeros of N'D - N D' and infinity; Fatou's theorem, Milnor,
  *Dynamics in one complex variable*, Thm. 8.6): each is iterated 4000 times on the Riemann sphere in homogeneous
  coordinates, a repetition within 64 steps gives the cycle, and the contraction of a small perturbation over one
  period (< 0.999) tells it is attracting. Polynomial roots by the Aberth-Ehrlich iteration.
- Gap: exact or high-precision points (`im + 10^(-309)`, `n(3/5, 1)`) are iterated in machine numbers.

## meijer_g_reduce, fox_h_reduce

- e30.2 A table of Mellin-transform representations (Luke, *The special functions and their approximations* I,
  5.6; Prudnikov, Brychkov, Marichev, *Integrals and series* 3, 8.4; Adamchik and Marichev, ISSAC 1990), applied to
  each term of a sum; constant factors and powers of x stay outside (Meijer G), or are absorbed into one Fox H
  function. The internal form c30_gt(a, b, z, r) = G(z^(1/r)) becomes inactive(meijer_g)(a, b, z[, r]) or, by
  G(z^(1/r)) = |r| H(z^(sign r)) with scales |r| (substitute s = r t in the Mellin-Barnes integral), inactive(fox_h).
  Conventions (duplication forms with r = 1/2 for sin, cos, Bessel; e^w as G^(2,0)_(0,2) of
  w^2/4 unless a power of x multiplies e^(-x^a)); x > 0 throughout (|x| -> x in unit steps).
- Gaps: linear combinations merged into one function (2 sin x + cos x as one G with a phase), products other than
  the listed pairs (cosh x sin x, e^x airy_bi), mittag_leffler_e; `activate` of the r = 1/2 forms gives
  bessel_j(1, (a^2 x^2)^(1/2)) (meijer_g's own simplification).

## fox_h

- e30.3 Exact rules: pairs sorted within each group (the function is symmetric in them); a pair of the first
  a-group equal to one of the second b-group (or second a-group and first b-group) cancels; all scales equal c:
  meijer_g(a, b, z, c)/c (Kilbas and Saigo, *H-Transforms*, 2004, 1.1); H^(1,0)_(0,1)(z | (b, B)) =
  z^(b/B) e^(-z^(1/B))/B; z = 0 from the leading pole.
- e30.4 Numbers: the residue series (Kilbas-Saigo Thms. 1.2, 1.3) at the poles of the gamma(b_j + B_j s), j <= m,
  when mu = sum B - sum A > 0 (|z| < delta for mu = 0), else at those of gamma(1 - a_j - A_j s), j <= n. Machine
  arguments: a native with a complex Lanczos log-gamma (terms in log space). Other inexact arguments: the same sum
  in balls at d + 20 digits (again with the cancellation, log10 of the largest term over the sum, added when it
  exceeds 12), at 2d + 30 digits with the perturbation below (inputs padded with set_precision), rounded to d digits. Poles of higher order: the
  first-group parameters are shifted by +-eps (distinct multiples per factor) and the symmetric mean taken; in
  machine numbers eps = 2^-14 with Richardson extrapolation over eps and 2 eps, in balls eps = 10^(-(d + 12)/2) (the error eps^2, the cancellation about 2 log10(1/eps) digits of the 2d + 30).
- Gaps: series and asymptotic expansions, `around` arguments, derivatives: `d` of fox_h needs a derivative row
  in mc-calculus, d^k/dz^k H(z) = H^(m,n+1)_(p+1,q+1)(z | (-k, 1), (a_j - k A_j, A_j); (b_j - k B_j, B_j), (0, 1))
  with (0, 1) first in the second b-group (Kilbas-Saigo 2.2, with z^(-k) folded into the shifted parameters).

## spheroidal_s1

- e30.5 Flammer's expansion in spherical Bessel functions (Flammer, *Spheroidal wave functions*, 1957, 4.1.17;
  DLMF 30.11.3 with Flammer's weights): S1 = (z^2 - 1)^(m/2) z^(-m) sum a_k w_k j_(n+2k)(gamma z) / sum (-1)^k a_k
  w_k, w_k = (n + m + 2k)!/(n - m + 2k)!, a_k the Legendre coefficients of spheroidal_ps (the same continued
  fraction, at spheroidal_eigenvalue). The prefactor (z^2 - 1)^(m/2) z^(-m) (not ((z^2 - 1)/z^2)^(m/2)) gives the
  expected sign for z < 0 and the imaginary values for |z| < 1. The result is complex with an inexact zero
  imaginary part when real. 20 + |gamma| + p/2 coefficients above k = 0.
- The spherical Bessel functions are computed here (Miller's backward recurrence for |x| <= L, forward beyond):
  `spherical_bessel_j` of mc-special loses everything at high order and small machine argument
  (spherical_bessel_j(20, 0.0703) gives -1.49e30; the true value is about 1e-35).
- Gaps: exact closed forms at gamma = n pi/2, non-integer m, derivatives (spheroidal_s1_prime and a derivative row
  in mc-calculus), function properties (function_singularities etc.), series.

## trig_factor

- e30.6 Tangents and the reciprocal functions (circular and hyperbolic) through sines and cosines, together, then
  `factor` on numerator and denominator with the sine and cosine terms as variables, once as given and once after
  `trig_expand`; the result with the smaller leaf count is kept. Linear factors a cos u + a sin u become
  sqrt(2) a sin(u + pi/4). Parts of lists, relations and logical combinations separately.
- Gaps: sum-to-product (cos x + cos y = 2 cos((x + y)/2) cos((x - y)/2)), half-angle forms
  (sin 2x + 2 cos x = 8 sin(pi/4 + x/2)^3 sin(pi/4 - x/2)), tan/sec forms in the output.

## moment_of_inertia

- e30.7 The tensor integral of |r|^2 I - r r^T over the region (unit density), r = x - p, p the origin by default
  (the documented examples check this: ball() about the origin, about [1, 1, 0] by the parallel-axis shift); a plane
  region gives the 2 x 2 tensor [[int y^2, -int x y], [-int x y, int x^2]]; the axis moment v.I.v/(v.v). Integrals
  by `integrate(f, element(xs, reg))`; implicit regions x^2 + y^2 <= c (and with a z range) are turned into the disk
  and the cylinder, and z-axis cylinders are integrated in cylindrical coordinates, since region integration of
  those does not evaluate.
- Gaps: other implicit regions; stadium_shape and spherical_shell do not exist yet.

## number_field_discriminant

- e30.8 disc(O_K) = disc(g)/[O_K : Z[t]]^2 with g the monic integer polynomial of t = a theta (a the leading
  coefficient of the minimal polynomial). The Z-module M (starting at Z[t]) grows by an algebraic integer w in
  M/p \ M (searched over the p^d residues, integrality by the characteristic polynomial resultant(g(x), y - w(x)))
  while p^2 divides disc(M); the search is complete because a non-maximal M has an element of prime order p in
  O_K/M with p^2 | disc(M) (Cohen, *A course in computational algebraic number theory*, 6.1, used instead of Round
  2). Fine for small degrees (p^d candidates per step).

## algebraic_number

- e30.9 Normal form only (rules that fire while the form is not normal): generator an algebraic integer root(g, k)
  of its monic integer minimal polynomial (a radical for degree 2, as `root` itself gives), exactly deg g rational
  coefficients reduced modulo g; a theta of leading coefficient a becomes a theta, a nested algebraic_number its own
  root; rational generators and constant elements give numbers; inexact arguments give the value. The root index
  is found numerically at 30 digits.
- Gaps: arithmetic (add, mul, pow, re, abs, comparisons are natives: the library cannot extend them; arithmetic on
  algebraic_number terms needs the arithmetic natives to call a hook), root_reduce and
  to_number_field of algebraic numbers (mc-algebra), algebraic_number_polynomial.

## polynomial_sum_of_squares_list

- e30.10 Degree <= 2: L D L^T elimination of the Gram matrix in [x1..xk, 1], pivots from the last variable to the
  first, then the constant (the squares of x^2 + x y + y^2 and of the ternary form come out in the documented
  order); any failing pivot leaves the call unevaluated (non-SOS inputs).
- Gap: higher degrees need a semidefinite program for the free Gram entries (x^4 + x y + y^2 + 1/60 is SOS only for
  the parameter of x^2 = x.x = 1.x^2 in (1/8, 0.129)); a check by `expand(p - q.q)` accepts any certificate.

## inverse_radon_transform

- e30.11 Ansatz and linear algebra instead of the filtered back-projection integral: for g = e^(-p^2) Q(p, cos phi,
  sin phi) the preimage is e^(-r^2) P(x, y), deg P = deg Q (the transform of e^(-r^2) x^i y^j in the rotated frame
  x = p c - t s, y = p s + t c is e^(-p^2) times binomial sums of the moments gamma((k + 1)/2)); the coefficients
  solve the identity at rational points on the unit circle and integer p, and injectivity of the transform makes
  the solution the inverse (Deans, *The Radon transform and some of its applications*, 1983). Point masses and the
  disk's chord lengths are recognised directly.
- Gaps: general integrands (the Hilbert-transform inversion formula); examples that need `radon_transform` itself
  to evaluate first.

## complete_integral

- e30.12 Separable first-order equations only: F(x, p) = 0 free of u, explicit in at most one variable x_k; the
  other derivatives are the constants c(1), c(2), ... in variable order, p_k from `solve`, u = integrate(p_k, x_k) +
  sum c(j) x_j + c(last) (Courant-Hilbert II, II.4). The documented examples of this kind give this numbering and form.
- Gaps: quasi-linear and general equations (Lagrange-Charpit characteristics), Clairaut equations, equations with
  conditions, another parameterisation for u_x - x u_y = 0 (with 1/c(1)).

## Round 34

- The crate is `mc-further-functions` (was `mc-cover30`), named after its catalogue topic, and its library file
  `lib/further.mc` (was `lib/cover30.mc`). Its functions span several areas; moving them into the crates of those
  areas would change their topic in the catalogue and the help index, so they stay together. No behaviour change.

## Round 36

- `complete_integral`: Clairaut's equation u = sum x_i p_i + g(p) gives u = sum c_i x_i + g(c) (Courant and Hilbert II.2);
  the separable method tries the other variables when the equation for the first has not exactly one solution
  (u_t + u_x^2 = 0: u = c1 x - c1^2 t + c2).

## Round 36, follow-up

- julia_set_iteration_count of an exact point under a polynomial map (and of any point with working_precision: d)
  iterates in complex ball arithmetic and decides escape and capture only when the ball is on one side, doubling the
  precision (from 128 bits, or d digits, up to 2^16) until every decision is certain, else it stays unevaluated.
  i + 10^-309 under z^2 escapes at k = 2055 (|z0|^(2^k) > 4); machine arithmetic gave 1055.

## Round 38: inverse Radon transform

- Linearity: a sum term by term with constant factors outside, calls `g(p, phi)` of undefined functions kept as
  `inverse_radon_transform(g(p, phi), [p, phi], [x, y])`; a term with no known preimage keeps the whole sum
  unevaluated. `inverse_radon_transform(radon_transform(f, [x, y], [p, phi]), [p, phi], [x, y])` is f.
