#! mathcas 2
# Lists library: vectors, matrices and number functions written in MathCAS itself, evaluated
# after the numerics library, defined in every session. Helper functions are prefixed `b29_` and not part of the
# interface. Methods and literature: crates/mc-lists/NOTES.md, section "Round 29: the lists library".

# b29_matrix(rows, cols, f): the matrix of f(i, j) (lexical index names: entries may contain any symbol)
fn b29_matrix(rows, cols, f) = map(i => map(j => f(i, j), range(cols)), range(rows))

# b29_small(x, scale): whether the inexact number x is zero up to rounding relative to scale
fn b29_small(x, scale) = abs(x) <= 1.0e-10*max(1, scale)

# --- vectors ---

# unit_vector(n, k): the vector of length n with a 1 at position k and zeros elsewhere; unit_vector(k) is the
# 2-vector (k = 1 or 2)
fn unit_vector(n: Int, k: Int) when n >= 1 and k >= 1 and k <= n = map(i => boole(i == k), range(n))
fn unit_vector(k: Int) when k == 1 or k == 2 = unit_vector(2, k)

# angle_vector(phi): [cos(phi), sin(phi)]; angle_vector([r, phi]): the point of polar coordinates r, phi;
# angle_vector([r, theta, phi]): the point of spherical coordinates (theta from the z axis); angle_vector(c, p):
# the same relative to the point c
fn angle_vector(p) when not is_list(p) = [cos(p), sin(p)]
fn angle_vector(p) when is_list(p) and length(p) == 1 = angle_vector(p[1])
fn angle_vector(p) when is_list(p) and length(p) == 2 = [p[1]*cos(p[2]), p[1]*sin(p[2])]
fn angle_vector(p) when is_list(p) and length(p) == 3 = [p[1]*sin(p[2])*cos(p[3]), p[1]*sin(p[2])*sin(p[3]), p[1]*cos(p[2])]
fn angle_vector(c, p) when is_list(c) = map(i => c[i] + angle_vector(p)[i], range(length(c)))

# vector_angle(u, v): the angle arccos(u . conjugate(v)/(|u| |v|)) between two vectors
fn vector_angle(u, v) when is_list(u) and is_list(v) and length(u) == length(v) =
  if is_inexact(total(flatten([u, v]))) and every(flatten([u, v]), t => abs(im_part(t)) < 1.0e-300) then {
    # real machine or high-precision vectors: Kahan's 2 atan2(|a - b|, |a + b|) of the unit vectors, exact to the last
    # digit for parallel vectors where acos(cos) loses half the digits (8e-9 for a parallel pair)
    # vectors whose 2 x 2 minors u_i v_j - u_j v_i all vanish in the arithmetic are parallel: 0 or pi exactly
    # (normalising first would leave 2.5e-16 for [1, 2] and [0.1, 0.2])
    let len = length(u)
    let minors = flatten(table(u[i]*v[j] - u[j]*v[i], [i, 1, len], [j, i + 1, len]))
    let d = dot(u, v)
    if len >= 2 and every(minors, t => t == 0) and d != 0 then (if d > 0 then 0*d else acos(-1 + 0*d)) else {
      let a = u/norm(u)
      let b = v/norm(v)
      2*atan(norm(a + b), norm(a - b))
    }
  } else acos(dot(u, conjugate(v))/(norm(u)*norm(v)))

# manhattan_distance(u, v): the sum of |u_i - v_i|
fn manhattan_distance(u, v) when is_list(u) and is_list(v) and length(u) == length(v) = total(map(i => abs(u[i] - v[i]), range(length(u))))
fn manhattan_distance(u, v) when not is_list(u) and not is_list(v) = abs(u - v)

# from_polar_coordinates([r, theta1, ..., theta_(n-1)]): the Cartesian point of hyperspherical coordinates (the
# angle theta1 first: x1 = r cos(theta1), x2 = r sin(theta1) cos(theta2), ...); a list of points maps over it
fn from_polar_coordinates(p) when is_list(p) and length(p) >= 1 and not is_list(p[1]) = {
  let k = length(p)
  let sines = j => product(map(t => sin(p[t + 1]), range(j - 1)))
  if k == 1 then [p[1]]
  else map(j => if j < k then p[1]*sines(j)*cos(p[j + 1]) else p[1]*sines(j), range(k))
}
fn from_polar_coordinates(ps) when is_list(ps) and length(ps) >= 1 and is_list(ps[1]) = map(from_polar_coordinates, ps)
fn from_polar_coordinates(ps) when is_list(ps) and length(ps) == 0 = []

# permute(xs, p): the list xs with its element i moved to position p[i], for a permutation list p (the positions
# beyond length(p) stay) or a permutation in cycles form (cycles([[1, 3, 2]]) moves element 1 to 3, 3 to 2, 2 to 1)
fn permute(xs, p) when is_list(xs) and not is_atom(p) and head(p) === cycles = permute(xs, permutation_list(p, length(xs)))
fn permute(xs, p) when is_list(xs) and is_list(p) and length(p) <= length(xs) and sort(p) == range(length(p)) = {
  let k = length(xs)
  let q = join(p, range(length(p) + 1, k))
  let inv = ordering(q)
  map(j => xs[inv[j]], range(k))
}

# to_polar_coordinates([x1, ..., xn]): the hyperspherical coordinates [r, theta1, ..., theta_(n-1)] of the point, the
# inverse of from_polar_coordinates: r = |x|, theta_k = acos(x_k/sqrt(x_k^2 + ... + x_n^2)) for k < n - 1 and the
# last angle atan(x_(n-1), x_n) in (-pi, pi]; a list of points maps over it
fn to_polar_coordinates(p) when is_list(p) and length(p) >= 2 and not is_list(p[1]) = {
  let k = length(p)
  let tail = j => sqrt(total(map(i => p[i]^2, range(j, k))))
  join([tail(1)], map(j => acos(p[j]/tail(j)), range(k - 2)), [atan(p[k - 1], p[k])])
}
fn to_polar_coordinates(ps) when is_list(ps) and length(ps) >= 1 and is_list(ps[1]) = map(to_polar_coordinates, ps)

# --- matrix predicates ---

# b29_zero_matrix(d, scale): whether every entry of d is zero (up to rounding when inexact)
fn b29_zero_matrix(d, scale) = every(flatten(d), x => x === 0 or (is_inexact(x) and b29_small(x, scale)) or (is_number(x) and simplify(x) === 0))

# symmetric_matrix_q(m): whether the square matrix m equals its transpose (inexact entries up to rounding)
fn symmetric_matrix_q(m) = is_matrix(m) and length(m) == length(m[1]) and b29_zero_matrix(m - transpose(m), max(map(abs, flatten(m))))

# hermitian_matrix_q(m): whether the square matrix m equals its conjugate transpose (inexact entries up to rounding)
fn hermitian_matrix_q(m) = is_matrix(m) and length(m) == length(m[1]) and b29_zero_matrix(m - conjugate(transpose(m)), max(map(abs, flatten(m))))

# diagonalizable_matrix_q(m): whether the square exact matrix m is diagonalizable over the complex numbers, that is
# whether its minimal polynomial has no repeated factor; that holds when the square-free part s = p/gcd(p, p') of the
# characteristic polynomial p annihilates m (s(m) = 0), as s divides every polynomial with the roots of p (Horn and
# Johnson, Matrix analysis, 3.3.10); unevaluated for inexact entries
fn diagonalizable_matrix_q(m) when is_matrix(m) and length(m) == length(m[1]) and not is_inexact(total(flatten(m))) = {
  let p = characteristic_polynomial(m, r38_t)
  let s = cancel(p/polynomial_gcd(p, d(p, r38_t)))
  let cs = coefficient_list(expand(s), r38_t)
  let k = length(m)
  var acc = 0*m
  var pw = identity_matrix(k)
  for i in range(length(cs)) {
    acc = acc + cs[i]*pw
    pw = dot(pw, m)
  }
  every(flatten(acc), x => possible_zero_q(x))
}
fn diagonalizable_matrix_q(m) when is_matrix(m) and length(m) != length(m[1]) = false

# pauli_matrix(k): the Pauli matrix sigma_k for k = 1, 2, 3 and the identity matrix for k = 0; a list maps
fn pauli_matrix(k) when k === 0 = [[1, 0], [0, 1]]
fn pauli_matrix(k) when k === 1 = [[0, 1], [1, 0]]
fn pauli_matrix(k) when k === 2 = [[0, -im], [im, 0]]
fn pauli_matrix(k) when k === 3 = [[1, 0], [0, -1]]
fn pauli_matrix(ks) when is_list(ks) = map(pauli_matrix, ks)

# fourier_matrix(n): the n x n matrix of the discrete Fourier transform, F[s][r] = n^((a - 1)/2) exp(2 pi i b (r -
# 1)(s - 1)/n) with fourier_parameters: [a, b] (default [0, 1], the convention of fourier), so that dot(F, u) is
# fourier(u)
fn fourier_matrix(n; fourier_parameters = [0, 1]) when is_integer(n) and n >= 1 = {
  let a = fourier_parameters[1]
  let b = fourier_parameters[2]
  # the exponent reduced mod n at an integer b (exp is periodic), which keeps the entries small roots of unity
  table(n^((a - 1)/2)*exp(2*pi*im*b*(if is_integer(b) then mod((r - 1)*(s - 1), n) else (r - 1)*(s - 1))/n), [s, 1, n], [r, 1, n])
}

# b29_numeric_matrix(m): whether every entry of the matrix m is a number
fn b29_numeric_matrix(m) = every(flatten(m), x => is_number(x))

# positive_definite_matrix_q(m): whether the Hermitian part (m + m^H)/2 of the square matrix m is positive definite
# (so a real matrix is tested through its symmetric part), by Sylvester's criterion: every leading principal minor
# positive; unevaluated for entries that are not numbers
fn positive_definite_matrix_q(m) when is_matrix(m) and length(m) == length(m[1]) and b29_numeric_matrix(m) = {
  let h = (m + conjugate(transpose(m)))/2
  every(range(length(m)), k => { let d = det(map(r => take(r, k), take(h, k))); is_numeric(d) and n(re(d)) > 0 })
}
fn positive_definite_matrix_q(m) when is_matrix(m) and (length(m) != length(m[1])) = false

# --- matrix functions ---

# permanent(m): the permanent of the matrix m (sum over the injections of the rows into the columns of the products
# of the chosen entries; for a square matrix the sum over all permutations), by Ryser's formula
fn b29_ryser(m) = {
  let k = length(m)
  let cols = range(length(m[1]))
  let terms = map(s => (-1)^length(s)*product(map(r => total(map(j => r[j], s)), m)), subsets(cols))
  (-1)^k*total(terms)
}
# a matrix with symbolic entries: the dynamic programme over the column subsets (the permanent of the first i rows
# with the columns S, |S| = i, is the sum over j in S of m[i][j] times that of the first i - 1 rows with S without j),
# expanding as it goes, so the work follows the number of terms of the answer (Ryser's product of sums would expand
# to n^n terms first)
fn b29_perm_dp(m) = {
  let c = length(m[1])
  var layer = association_thread([[]], [1])
  for i in range(length(m)) {
    let prev = layer
    let sets = subsets(range(c), [i])
    layer = association_thread(sets, map(s => expand(total(map(j => m[i][j]*lookup(prev, select(s, x => x != j)), s))), sets))
  }
  total(map(k => lookup(layer, k), keys(layer)))
}
fn permanent(m) when is_matrix(m) and length(m) <= length(m[1]) =
  if every(flatten(m), is_numeric) then (
    if length(m) == length(m[1]) then expand(b29_ryser(m))
    else total(map(cs => b29_ryser(map(r => map(j => r[j], cs), m)), subsets(range(length(m[1])), [length(m)])))
  ) else b29_perm_dp(m)
fn permanent(m) when is_matrix(m) and length(m) > length(m[1]) = permanent(transpose(m))

# shearing_matrix(theta, v, w): the matrix I + tan(theta) u w^T of the shear along the direction v (u = v/|v|) with
# the normal w (|w| = 1 after scaling); a direction v not orthogonal to w shears along its part orthogonal to w,
# v - (v . n) n (a shear moves points parallel to the planes w . x = constant)
fn shearing_matrix(t, v, w) when is_list(v) and is_list(w) and length(v) == length(w) = {
  let nn = w/norm(w)
  let vp = simplify(v - dot(v, nn)*nn)
  let u = vp/norm(vp)
  b29_matrix(length(v), length(v), (i, j) => boole(i == j) + tan(t)*u[i]*nn[j])
}

# scaling_matrix([s1, ..., sk]): the diagonal matrix of the factors; scaling_matrix(s, v): the scaling by s along the
# direction v, I + (s - 1) v v^T/(v . v); scaling_matrix([s1, ..., sk], [v1, ..., vk]) scales along the directions
fn scaling_matrix(s) when is_list(s) = diagonal_matrix(s)
fn scaling_matrix(s, v) when is_list(v) and not is_list(s) = {
  let k = length(v)
  b29_matrix(k, k, (i, j) => simplify(boole(i == j) + (s - 1)*v[i]*conjugate(v[j])/dot(v, conjugate(v))))
}
fn scaling_matrix(ss, vs) when is_list(vs) and is_list(ss) and is_list(vs[1]) = {
  let k = length(vs[1])
  b29_matrix(k, k, (i, j) => boole(i == j) + total(map(t => (ss[t] - 1)*vs[t][i]*conjugate(vs[t][j])/dot(vs[t], conjugate(vs[t])), range(length(vs)))))
}

# distance_matrix(points): the matrix of Euclidean distances of the points (vectors or numbers);
# distance_matrix(a, b): the distances of the points of a to those of b
fn distance_matrix(a) when is_list(a) = distance_matrix(a, a)
fn distance_matrix(a, b) when is_list(a) and is_list(b) = map(u => map(v => nl_dm_dist(u, v), b), a)

# the distance the points' data type calls for: strings and sequences of different lengths by the edit distance,
# lists of booleans by the Jaccard dissimilarity, lists of strings by the edit distance of the sequences, numbers
# by the Euclidean distance (a non-number contributing 0 or 1 to it when the list is mixed); an association is its
# values, taken by the keys of the first (associations whose keys come in another order align by key)
fn nl_dm_dist(u, v) =
  if is_assoc(u) and is_assoc(v) then nl_dm_dist(values(u), map(k => v[k], keys(u)))
  else if is_string(u) and is_string(v) then nl_edit_dist(u, v)
  else if is_list(u) and is_list(v) then nl_dm_list(u, v)
  else if is_list(u) or is_list(v) then euclidean_distance(u, v)
  else abs(u - v)
fn nl_dm_list(u, v) =
  if length(u) > 0 and length(v) > 0 and every(u, x => is_string(x)) and every(v, x => is_string(x)) then nl_edit_dist(u, v)
  else if length(u) != length(v) then nl_dtw(u, v)
  else if length(u) > 0 and every(u, nl_is_bool) and every(v, nl_is_bool) then {
    let both = length(select(range(length(u)), i => u[i] and v[i]))
    let diff = length(select(range(length(u)), i => not (u[i] === v[i])))
    if diff == 0 then 0 else diff/(both + diff)
  }
  else if any_true(u, nl_is_nominal) or any_true(v, nl_is_nominal) then
    euclidean_distance(map(i => if is_string(u[i]) or is_string(v[i]) then boole(not (u[i] === v[i])) else abs(u[i] - v[i]), range(length(u))), constant_array(0, length(u)))
  else euclidean_distance(u, v)
fn nl_is_bool(x) = x === true or x === false
fn nl_is_nominal(x) = is_string(x) or nl_is_bool(x)
# nl_dtw(u, v): dynamic time warping distance of two numeric sequences of different lengths (least total |u_i - v_j|
# over monotone alignments), the distance between ragged numeric points
fn nl_dtw(u, v) = {
  let m = length(v)
  var prev = join([0], constant_array(inf, m))
  for i in range(length(u)) {
    var cur = [inf]
    for j in range(m) { cur = append(cur, abs(u[i] - v[j]) + min(prev[j + 1], cur[j], prev[j])) }
    prev = cur
  }
  prev[m + 1]
}
# edit_dist(u, v): the least number of insertions, deletions and substitutions turning the string (or list) u into v
fn nl_edit_dist(u, v) = {
  let a = if is_string(u) then characters(u) else u
  let b = if is_string(v) then characters(v) else v
  var prev = range(0, length(b))
  for i in range(length(a)) {
    var cur = [i]
    for j in range(length(b)) { cur = append(cur, min(prev[j + 1] + 1, cur[j] + 1, prev[j] + boole(not (a[i] === b[j])))) }
    prev = cur
  }
  prev[length(b) + 1]
}

# quantity matrices (round 36): a matrix whose columns each hold quantities of one dimension (or plain numbers) is
# M . diag(u) with the column units u; row_reduce, null_space, rank, least_squares and
# matrix_minimal_polynomial work on the magnitudes M and put the units back. A matrix not of that form stays
# unevaluated.
fn nl_qm_is(x) = not is_atom(x) and head(x) === quantity
fn nl_qm_has(m) = is_list(m) and length(select(flatten([m]), nl_qm_is)) > 0
fn nl_qm_unit(col) = { let qs = select(col, nl_qm_is); if length(qs) == 0 then 1 else quantity_unit(qs[1]) }
fn nl_qm_mag(x, u) = if nl_qm_is(x) then quantity_magnitude(x, u) else if u === 1 or x === 0 then x else failure(incompatible_units)
# [magnitudes, column units] of a matrix (a vector is one column)
fn nl_qm_split(m) = {
  let a = if is_matrix(m) then m else map(x => [x], m)
  let us = map(j => nl_qm_unit(map(r => r[j], a)), range(length(a[1])))
  [map(r => map(j => nl_qm_mag(r[j], us[j]), range(length(us))), a), us]
}
fn nl_qm_ok(m) = is_list(m) and length(m) > 0 and nl_qm_has(m) and free_of(nl_qm_split(m)[1], failure)
fn nl_qm_make(x, u) = if u === 1 or x === 0 then x else quantity(x, u)
fn nl_qm_row_reduce(m) = {
  let s = nl_qm_split(m)
  let us = s[2]
  map(r => { let p = select(range(length(r)), j => not (r[j] === 0)); if length(p) == 0 then r else map(j => nl_qm_make(r[j], us[j]/us[p[1]]), range(length(r))) }, row_reduce(s[1]))
}
fn nl_qm_null_space(m) = {
  let s = nl_qm_split(m)
  let us = s[2]
  map(v => { let f = last(select(range(length(v)), j => not (v[j] === 0))); map(j => nl_qm_make(v[j], us[f]/us[j]), range(length(v))) }, null_space(s[1]))
}
rule row_reduce(m_) := nl_qm_row_reduce(m) when is_matrix(m) and nl_qm_ok(m)
rule null_space(m_) := nl_qm_null_space(m) when is_matrix(m) and nl_qm_ok(m)
rule rank(m_) := rank(nl_qm_split(m)[1]) when is_matrix(m) and nl_qm_ok(m)
fn least_squares(m, b) when is_matrix(m) and is_list(b) and (nl_qm_has(m) or nl_qm_has(b)) and free_of(nl_qm_split(m)[1], failure) and free_of(nl_qm_split(b)[1], failure) = {
  let sm = nl_qm_split(m)
  let sb = nl_qm_split(b)
  let y = least_squares(sm[1], sb[1])
  let x = map(j => map(k => nl_qm_make(y[j][k], sb[2][k]/sm[2][j]), range(length(sb[2]))), range(length(sm[2])))
  if is_matrix(b) then x else map(first, x)
}
fn matrix_minimal_polynomial(m, x) when is_matrix(m) and nl_qm_ok(m) and length(union(nl_qm_split(m)[2])) == 1 = {
  let s = nl_qm_split(m)
  let u = s[2][1]
  let p = matrix_minimal_polynomial(s[1], x)
  let cs = coefficient_list(p, x)
  let d = length(cs) - 1
  total(map(i => x^i * nl_qm_make(cs[i + 1], u^(d - i)), range(0, d)))
}

# least_squares(m, b): the vector x minimizing |m . x - b| (the minimum-norm one when m is rank deficient), the
# pseudo-inverse of m applied to b (a matrix b: column by column)
fn least_squares(m, b) when is_matrix(m) and is_list(b) = dot(pseudo_inverse(m), b)

# drazin_inverse(m): the Drazin inverse of the square matrix m, m^k (m^(2k + 1))^+ m^k with k the index of m (the
# smallest k with rank(m^k) = rank(m^(k + 1)))
fn drazin_inverse(m) when is_matrix(m) and length(m) == length(m[1]) = {
  let k = length(m)
  var p = identity_matrix(k)
  var idx = 0
  var done = false
  var pw = m
  while not done and idx < k {
    let r0 = rank(p)
    p = dot(p, m)
    if rank(p) == r0 then done = true else idx = idx + 1
  }
  let mk = matrix_power(m, idx)
  if idx == 0 then inverse(m) else dot(mk, dot(pseudo_inverse(matrix_power(m, 2*idx + 1)), mk))
}

# --- statistics and colour operations of images ---

# an image value image("file")
fn img_is_image(x) = not is_atom(x) and head(x) === image and length(x) == 1 and is_string(x[1])
# the channels of an image as lists of samples (one list per channel, every pixel once)
fn img_channels_of(img) = transpose(flatten(image_data(img), 1))
# a statistic per channel: one number for a gray image, a list for an image of several channels
fn img_per_channel(f, img) = { let cs = map(f, img_channels_of(img)); if length(cs) == 1 then cs[1] else cs }
rule mean(img_) := img_per_channel(mean, img) when img_is_image(img)
rule median(img_) := img_per_channel(median, img) when img_is_image(img)
rule variance(img_) := img_per_channel(variance, img) when img_is_image(img)
rule std_dev(img_) := img_per_channel(std_dev, img) when img_is_image(img)
# the colour channels (not an alpha channel: the second of two, the fourth of four) of every pixel through f
fn img_colour_map(img, f) = {
  let d = image_data(img)
  let c = length(d[1, 1])
  let k = if c == 2 or c == 4 then c - 1 else c
  image(map(row => map(px => join(map(f, take(px, k)), drop(px, k)), row), d))
}
rule color_negate(img_) := img_colour_map(img, x => 1 - x) when img_is_image(img)
rule darker(img_) := darker(img, 1/3) when img_is_image(img)
rule darker(img_, f_) := img_colour_map(img, x => (1 - f)*x) when img_is_image(img) and is_real(n(f)) and 0 <= f and f <= 1
rule lighter(img_) := lighter(img, 1/3) when img_is_image(img)
rule lighter(img_, f_) := img_colour_map(img, x => x + f*(1 - x)) when img_is_image(img) and is_real(n(f)) and 0 <= f and f <= 1
# a gray image as an RGB image (the gray level in each colour channel), an RGB image as gray by the luma weights
# of ITU-R BT.601 (0.299, 0.587, 0.114)
# (other channel counts and colour spaces: failure(unsupported, ...))
rule color_convert(img_, s_) := img_convert(img, s) when img_is_image(img) and (s === "RGB" or s === "Grayscale")
fn img_convert(img, s) = {
  let d = image_data(img)
  let c = length(d[1, 1])
  if c == 1 and s === "RGB" then image(map(row => map(px => [px[1], px[1], px[1]], row), d))
  else if (c == 3 and s === "RGB") or (c == 1 and s === "Grayscale") then img
  else if c == 3 then image(map(row => map(px => 0.299*px[1] + 0.587*px[2] + 0.114*px[3], row), d))
  else failure(unsupported, color_convert, s)
}

# --- structuring matrices (kernels of image processing) ---

# b29_nd(dims, idx, f): the nested list of f(index vector) over a grid of the given dimensions
fn b29_nd(dims, idx, f) = if length(dims) == 0 then f(idx) else map(i => b29_nd(rest(dims), append(idx, i), f), range(first(dims)))

# b29_shape(r, d, test): the array of the given dimensions d (a number: every dimension) with entries boole(test(x)),
# x the offsets from the centre in units of the radii r (a number: two dimensions)
fn b29_shape(r, d, test) = {
  let rs0 = if is_list(r) then r else (if is_list(d) then constant_array(r, length(d)) else [r, r])
  # a dimension `all` is the natural width 2 r + 1 of its radius; a radius `all` reaches the edges of its dimension
  let ds = if is_list(d) then map(i => if d[i] === all then 2*rs0[i] + 1 else d[i], range(length(d))) else constant_array(d, length(rs0))
  let rs = map(i => if rs0[i] === all then (ds[i] - 1)/2 else rs0[i], range(length(rs0)))
  b29_nd(ds, [], idx => boole(test(map(i => b29_scaled(idx[i] - (ds[i] + 1)/2, rs[i]), range(length(ds))), rs)))
}
# the offset o in units of the radius r: o/r; for r = 0 the shape holds only the offset 0 (|o| <= r), so any other
# offset is infinitely far
fn b29_scaled(o, r) = if r == 0 then (if o == 0 then 0 else inf) else o/r
fn b29_size(r) = if is_list(r) then map(t => 2*t + 1, r) else 2*r + 1

# disk_matrix(r), disk_matrix(r, d): the array of 1 where (x1/r1)^2 + ... <= 1 (a disk, ball or ellipsoid of
# radii r around the centre of an array of 2 r + 1 entries or of the dimensions d), else 0
fn disk_matrix(r) = b29_shape(r, b29_size(r), (x, rs) => total(map(t => t^2, x)) <= 1)
fn disk_matrix(r, d) = b29_shape(r, d, (x, rs) => total(map(t => t^2, x)) <= 1)

# diamond_matrix(r), (r, d): 1 where |x1/r1| + ... <= 1, else 0
fn diamond_matrix(r) = b29_shape(r, b29_size(r), (x, rs) => total(map(abs, x)) <= 1)
fn diamond_matrix(r, d) = b29_shape(r, d, (x, rs) => total(map(abs, x)) <= 1)

# box_matrix(r), (r, d): 1 where every |xi/ri| <= 1, i.e. the whole array when it is 2 r + 1 entries wide
fn box_matrix(r) = b29_shape(r, b29_size(r), (x, rs) => every(x, t => abs(t) <= 1))
fn box_matrix(r, d) = b29_shape(r, d, (x, rs) => every(x, t => abs(t) <= 1))

# cross_matrix(r), (r, d): 1 on the axes through the centre up to the radii (every entry whose offsets are zero in
# all but one dimension), else 0
fn cross_matrix(r) = b29_shape(r, b29_size(r), (x, rs) => length(select(x, t => t != 0)) <= 1 and every(x, t => abs(t) <= 1))
fn cross_matrix(r, d) = b29_shape(r, d, (x, rs) => length(select(x, t => t != 0)) <= 1 and every(x, t => abs(t) <= 1))

# gaussian_matrix(r), ([r, s]): the (2 r + 1) x (2 r + 1) kernel of the Gaussian of standard deviation r/2 (s)
# sampled at the integers, normalized to total 1 (the outer product of the normalized one-dimensional kernel)
fn gaussian_matrix(p) when not is_list(p) = gaussian_matrix([p, p/2])
fn gaussian_matrix(p) when is_list(p) and length(p) == 2 and not is_list(p[1]) = {
  let r = p[1]
  let sg = n(p[2])
  let g = map(t => exp(-t^2/(2*sg^2)), range(-r, r))
  let w = total(g)
  let k = map(t => t/w, g)
  b29_matrix(2*r + 1, 2*r + 1, (i, j) => k[i]*k[j])
}
fn gaussian_matrix(p) when is_list(p) and length(p) == 1 and is_list(p[1]) = {
  let r = p[1][1]
  let g = map(t => exp(-t^2/(2*(n(r)/2)^2)), range(-r, r))
  map(t => t/total(g), g)
}
# gaussian_matrix(p, [n1, ..., nd]): the d-dimensional kernel of the n_i-th derivatives of the Gaussian, the outer
# product of one-dimensional kernels sampled at the integers -r..r; p is r (standard deviation r/2), [r, s], or per
# dimension [[r1, ..., rd]] or [[r1, ..., rd], [s1, ..., sd]]. Each one-dimensional kernel of order k is scaled so
# that its correlation with t^k is k! (total 1 for k = 0), so it takes the k-th derivative of polynomials of degree
# k exactly (the moments below k vanish by symmetry).
fn gm_radii(p, d) =
  if is_list(p) and length(p) >= 1 and is_list(p[1]) and length(p[1]) == d then
    [p[1], if length(p) == 2 and is_list(p[2]) and length(p[2]) == d then map(n, p[2]) else map(r => n(r)/2, p[1])]
  else if is_list(p) and length(p) == 2 and not is_list(p[1]) then [constant_array(p[1], d), constant_array(n(p[2]), d)]
  else if not is_list(p) then [constant_array(p, d), constant_array(n(p)/2, d)]
  else null
fn gm_kernel(r, s, k) = {
  # the k-th derivative of exp(-t^2/(2 s^2)) is (-1/(s sqrt 2))^k H_k(t/(s sqrt 2)) exp(-t^2/(2 s^2))
  let c = n(s*sqrt(2))
  let g1 = map(t => (-1/c)^k*hermite_h(k, t/c)*exp(-t^2/(2*s^2)), range(-r, r))
  # an even order: the sampled, truncated kernel does not sum to 0 exactly; a multiple of the order-0 kernel
  # removes the total (constants have derivative 0), odd orders sum to 0 by symmetry
  let g0 = map(t => exp(-t^2/(2*s^2)), range(-r, r))
  let g = if k > 0 and mod(k, 2) == 0 then g1 - total(g1)/total(g0)*g0 else g1
  let m = total(map(i => g[i]*(r + 1 - i)^k, range(2*r + 1)))
  # (+ 0.0 keeps the centre of an odd order a plain zero)
  map(v => v*factorial(k)/m + 0.0, g)
}
# gaussian_matrix([[r1, ...], [s1, ...]]): radii and standard deviations per dimension
fn gaussian_matrix(p) when is_list(p) and length(p) == 2 and is_list(p[1]) and is_list(p[2]) and length(p[1]) == length(p[2]) = gaussian_matrix(p, constant_array(0, length(p[1])))
fn gaussian_matrix(p, ns) when is_list(ns) and length(ns) >= 1 and every(ns, k => is_integer(k) and k >= 0) and not (gm_radii(p, length(ns)) === null) and every(gm_radii(p, length(ns))[1], r => is_integer(r) and r >= 0) = {
  let rs = gm_radii(p, length(ns))
  let ks = map(i => gm_kernel(rs[1][i], rs[2][i], ns[i]), range(length(ns)))
  fold((a, b) => outer(mul, a, b), ks[1], rest(ks))
}

# --- numbers ---

# mixed_fraction_parts(x): [ip, fp], the integer part and the fractional part of x with the sign of x (the real and
# the imaginary part separately for a complex x); a list maps
fn mixed_fraction_parts(x) when is_list(x) = map(mixed_fraction_parts, x)
fn mixed_fraction_parts(x) when not is_list(x) and is_numeric(x) and is_real(n(x)) === true = [integer_part(x), fractional_part(x)]
fn mixed_fraction_parts(x) when not is_list(x) and is_numeric(x) and is_real(n(x)) === false = {
  let a = mixed_fraction_parts(re(x))
  let b = mixed_fraction_parts(im_part(x))
  [a[1] + im*b[1], a[2] + im*b[2]]
}

# perfect_number(k): the k-th (even) perfect number 2^(p - 1) (2^p - 1), p the k-th Mersenne prime exponent
fn perfect_number(k: Int) when k >= 1 and k <= 51 = { let p = mersenne_prime_exponent(k); 2^(p - 1)*(2^p - 1) }

# rudin_shapiro(n): (-1)^(the number of occurrences of the digits 11 in the binary expansion of n), n >= 0
fn rudin_shapiro(k) when is_list(k) = map(rudin_shapiro, k)
fn rudin_shapiro(k: Int) when k >= 0 = {
  let b = integer_digits(k, 2)
  (-1)^total(map(i => b[i]*b[i + 1], range(length(b) - 1)))
}

# quadratic_irrational_q(x): whether x is an irrational root of a quadratic polynomial with integer coefficients
fn quadratic_irrational_q(x) when is_list(x) = map(quadratic_irrational_q, x)
fn quadratic_irrational_q(x) when not is_list(x) = {
  let c = is_numeric(x) and not is_inexact(x) and not is_integer(x) and not is_rational(x)
  if not c then false else { let p = minimal_polynomial(x, b29_t); free_of(p, minimal_polynomial) and degree(p, b29_t) == 2 }
}

# discrete_indicator(x, x0, domain): 1 if x = x0 and 0 if x is another element of the finite domain; 0 for an x0
# outside the domain; unevaluated for an x that is not an element
rule discrete_indicator(x_, x0_, dom_) := 0 when is_list(dom) and not member_of(dom, x0)
rule discrete_indicator(x_, x0_, dom_) := boole(x === x0) when is_list(dom) and member_of(dom, x0) and member_of(dom, x)

# list_convolve(ker, list): the convolution of the list with the kernel, only where the kernel fits (n - k + 1
# entries); list_convolve(ker, list, q): cyclic, the kernel entry q aligned with the first entry of the list;
# matrices (two-dimensional kernel and array) likewise for the valid part
fn b29_conv1(ker, xs) = {
  let k = length(ker)
  map(i => total(map(j => ker[j]*xs[i + k - j], range(k))), range(length(xs) - k + 1))
}
fn list_convolve(ker, xs) when is_list(ker) and is_list(xs) and not is_list(ker[1]) and not is_list(xs[1]) and length(ker) <= length(xs) = b29_conv1(ker, xs)
fn list_convolve(ker, xs) when is_matrix(ker) and is_matrix(xs) and length(ker) <= length(xs) and length(ker[1]) <= length(xs[1]) = {
  let kr = length(ker)
  let kc = length(ker[1])
  b29_matrix(length(xs) - kr + 1, length(xs[1]) - kc + 1, (i, j) => total(flatten(b29_matrix(kr, kc, (a, b) => ker[a][b]*xs[i + kr - a][j + kc - b]))))
}
fn list_convolve(ker, xs, q) when is_list(ker) and is_list(xs) and is_integer(q) and not is_list(ker[1]) and not is_list(xs[1]) = {
  let m = length(xs)
  map(i => total(map(j => ker[j]*xs[mod(i - j + q - 1, m) + 1], range(length(ker)))), range(m))
}

# cumulant(r, data): the r-th cumulant of the list data computed from its raw moments (kappa_r = m_r -
# sum_k binomial(r - 1, k - 1) kappa_k m_(r - k)); cumulant(dist, r): the r-th derivative of log(moment generating
# function) at 0 for a distribution
fn b29_cumulants(ms, r) = {
  var ks = []
  for q in range(r) {
    ks = append(ks, ms[q] - total(map(k => binomial(q - 1, k - 1)*ks[k]*(if q - k == 0 then 1 else ms[q - k]), range(q - 1))))
  }
  ks
}
fn cumulant(data, r: Int) when is_list(data) and r >= 1 = {
  let m = length(data)
  last(b29_cumulants(map(q => total(map(x => x^q, data))/m, range(r)), r))
}
fn cumulant(r: Int, data) when is_list(data) and r >= 1 = cumulant(data, r)

# --- polynomials and arrays ---

# array_rules(a), array_rules(a, v): the rules [i, j, ...] -> x of the entries x of the array a that differ from v
# (0), followed by the rule of the other positions [_, _, ...] -> v
fn array_rules(a) = array_rules(a, 0)
fn array_rules(a, v) when is_list(a) = {
  let d = dimensions(a)
  let pos = select(b29_flat(b29_nd(d, [], idx => idx), length(d)), idx => not (b29_part_at(a, idx) === v))
  append(map(idx => idx -> b29_part_at(a, idx), pos), constant_array(_, length(d)) -> v)
}
fn b29_part_at(a, idx) = fold((x, i) => x[i], a, idx)
fn b29_flat(xs, k) = if k <= 1 then xs else flatten(xs, k - 1)

# matrix_minimal_polynomial(m, x): the monic polynomial in x of least degree that vanishes at the square matrix m
# (the first linear dependency among the powers I, m, m^2, ...)
fn matrix_minimal_polynomial(m, x) when is_matrix(m) and length(m) == length(m[1]) and not nl_qm_has(m) = {
  let k = length(m)
  var pows = [flatten(identity_matrix(k))]
  var cur = identity_matrix(k)
  var res = 0
  var d = 0
  while res === 0 and d < k + 1 {
    d = d + 1
    cur = dot(cur, m)
    pows = append(pows, flatten(cur))
    let ns = null_space(transpose(pows))
    if length(ns) > 0 then { let c = ns[1]; res = total(map(i => c[i]/c[d + 1]*x^(i - 1), range(d + 1))) }
  }
  res
}

# coefficient_arrays(polys, vars): the list [a0, a1, ..., ad] of arrays of the coefficients of the polynomial (or list
# of polynomials) in the variables vars: a_k holds the coefficient of a monomial x_i1 ... x_ik (i1 <= ... <= ik) at
# the position [i1, ..., ik] (after the polynomial's own index for a list), 0 elsewhere
fn b29_sorted_q(t) = every(range(length(t) - 1), i => t[i] <= t[i + 1])
# b29_eq_poly(p): lhs - rhs of an equation, else p
fn b29_eq_poly(p) = if not is_atom(p) and head(p) === head(b29_p == b29_q) and length(p) == 2 then p[1] - p[2] else p
fn coefficient_arrays(ps, vars) when is_list(ps) and is_list(vars) = {
  let nv = length(vars)
  let rules = map(p => coefficient_rules(b29_eq_poly(p), vars), ps)
  let deg = max(map(r => max(map(q => total(first(q)), r)), rules))
  let entry = (i, t) => {
    if not b29_sorted_q(t) then 0
    else {
      let ex = map(j => length(select(t, s => s == j)), range(nv))
      let hit = select(rules[i], q => first(q) == ex)
      if length(hit) == 0 then 0 else last(hit[1])
    }
  }
  map(k => b29_nd(join([length(ps)], constant_array(nv, k)), [], idx => entry(idx[1], rest(idx))), range(0, deg))
}
fn coefficient_arrays(p, vars) when not is_list(p) and is_list(vars) = map(a => a[1], coefficient_arrays([p], vars))

# pfaffian_det(m): the Pfaffian of the antisymmetric matrix m (expansion along the first row; 0 for odd size)
fn b29_pfaffian(m) = {
  let k = length(m)
  if k == 0 then 1
  else total(map(j => (-1)^j*m[1][j]*b29_pfaffian(b29_minor2(m, j)), range(2, k)))
}
fn b29_minor2(m, j) = {
  let keep = select(range(length(m)), i => i != 1 and i != j)
  map(a => map(b => m[a][b], keep), keep)
}
fn pfaffian_det(m) when is_matrix(m) and length(m) == length(m[1]) and b29_zero_matrix(m + transpose(m), 1) =
  if mod(length(m), 2) == 1 then 0 else expand(b29_pfaffian(m))

# principal_components(data): the rows of the data matrix in the coordinates of its principal axes (the eigenvectors of
# the covariance matrix, largest eigenvalue first), the mean subtracted; karhunen_loeve_decomposition(data): [the
# transformed rows, the basis] basis the eigenvectors of the covariance matrix of the rows of data (the rows are the variables), applied to the uncentred data
fn b29_axes(cov) = {
  let es = eigensystem(cov)
  let ord = reverse(ordering(map(v => re(n(v)), es[1])))
  map(i => { let v = es[2][i]/norm(es[2][i]); if re(n(v[1])) > 0 then -v else v }, ord)
}
fn principal_components(data) when is_matrix(data) and every(flatten(data), x => is_numeric(x)) = {
  let m = length(data)
  let c = length(data[1])
  let mu = map(j => total(map(r => r[j], data))/m, range(c))
  let x = map(r => r - mu, data)
  dot(x, transpose(b29_axes(dot(transpose(x), x)/(m - 1))))
}
fn karhunen_loeve_decomposition(data; standardized = false) when is_matrix(data) and every(flatten(data), x => is_numeric(x)) = {
  let c = length(data[1])
  let xc = map(r => r - total(r)/c, data)
  let basis = map(v => if re(n(v[1])) < 0 then -v else v, b29_axes(dot(xc, transpose(xc))/(c - 1)))
  # standardized: true transforms the data with each row's mean taken off
  [dot(basis, if standardized === true then xc else data), basis]
}
# karhunen_loeve_decomposition([b1, ..., bk], m): [m^T . b, m^T], the rows b (zero rows added up to the size of m) in
# the coordinates of the transposed basis: for the basis m of a decomposition (orthogonal) the data rebuilt from its
# first k transformed rows (all of them: the data again)
fn kld_with(bs, m) = { let z = 0*bs[1]; let full = join(bs, constant_array(z, length(m) - length(bs))); [dot(transpose(m), full), transpose(m)] }
fn karhunen_loeve_decomposition(bs, m) when is_matrix(bs) and is_matrix(m) and length(bs) <= length(m) and length(m[1]) == length(m) and length(m) >= 1 = kld_with(bs, m)
# of images of one size: each image's samples as one row, the transformed rows as images again
fn kld_images_ok(xs) = is_list(xs) and length(xs) >= 1 and every(xs, img_is_image)
fn kld_rows(imgs) = map(i => flatten(image_data(i)), imgs)
fn kld_images(rows, dims) = map(r => image(array_reshape(r, dims)), rows)
fn karhunen_loeve_decomposition(imgs; standardized = false) when kld_images_ok(imgs) = {
  let dims = dimensions(image_data(imgs[1]))
  let r = karhunen_loeve_decomposition(kld_rows(imgs), standardized: standardized)
  [kld_images(r[1], dims), r[2]]
}
fn karhunen_loeve_decomposition(imgs, m) when kld_images_ok(imgs) and is_matrix(m) and length(imgs) <= length(m) and length(m[1]) == length(m) = {
  let r = kld_with(kld_rows(imgs), m)
  [kld_images(r[1], dimensions(image_data(imgs[1]))), r[2]]
}

# --- precision ---

# machine_epsilon: the smallest machine number e with 1 + e != 1 (2^-52); min_machine_number: the smallest positive
# normal machine number (2^-1022)
rule machine_epsilon := 2.220446049250313e-16
rule min_machine_number := 2.2250738585072014e-308

# set_accuracy(x, a): x with the precision of every number in it set so that its accuracy is a digits after the decimal
# point (a number: precision a plus the decimal logarithm of its absolute value; a list or any other expression:
# its parts; symbols stay)
rule set_accuracy(x_, a_) := map(t => set_accuracy(t, a), x) when is_list(x) and is_real(a)
rule set_accuracy(x_, a_) := (if n(x) == 0 then set_precision(x, a) else set_precision(x, ceiling(a + log(abs(n(x)))/log(10)))) when is_numeric(x) and is_real(a) and not is_list(x)
rule set_accuracy(x_, a_) := x when is_symbol(x) and not is_numeric(x) and is_real(a)
rule set_accuracy(x_, a_) := apply(head(x), map(k => set_accuracy(x[k], a), range(length(x)))) when not is_atom(x) and not is_numeric(x) and not is_list(x) and is_real(a)

# --- intervals ---

# b29_boxes(i): the boxes [re lo, re hi, im lo, im hi] of an interval or centered interval, or of a number
fn b29_boxes(i) = {
  let h = head(i)
  if h === interval then map(p => [p[1], p[2], 0, 0], map(k => i[k], range(length(i))))
  else if h === centered_interval then { let c = i[1]; let r = i[2]; [[re(c) - re(r), re(c) + re(r), im_part(c) - im_part(r), im_part(c) + im_part(r)]] }
  else [[re(i), re(i), im_part(i), im_part(i)]]
}
fn b29_box_in(b, c) = c[1] <= b[1] and b[2] <= c[2] and c[3] <= b[3] and b[4] <= c[4]

# interval_member_q(i, x): whether the number x, or every piece of the interval x, lies in the interval i (real
# intervals, or centered intervals with complex centers); interval_member_q(i)(x) likewise
fn interval_member_q(i, x) when head(i) === interval or head(i) === centered_interval =
  every(b29_boxes(x), b => any_true(map(c => b29_box_in(b, c), b29_boxes(i)), t => t))
fn interval_member_q(i) = x => interval_member_q(i, x)

# interval_union(i1, i2, ...): the union of the intervals as one interval (real pieces merged); the smallest
# centered interval containing them when a complex centered interval is involved
fn interval_union(parts...) when length(parts) >= 1 and every(parts, t => head(t) === interval or head(t) === centered_interval) = {
  let bs = flatten(map(b29_boxes, parts), 1)
  if every(bs, b => b[3] == 0 and b[4] == 0) then apply(interval, map(b => [b[1], b[2]], bs))
  else {
    let lo = min(map(b => b[1], bs))
    let hi = max(map(b => b[2], bs))
    let ilo = min(map(b => b[3], bs))
    let ihi = max(map(b => b[4], bs))
    centered_interval((lo + hi)/2 + im*(ilo + ihi)/2, (hi - lo)/2 + im*(ihi - ilo)/2)
  }
}

# --- knapsack ---

# b29_is_rule(r): whether r is a rule a -> b
fn b29_is_rule(r) = head(r) === head(0 -> 0)

# b29_item(it): [payoff, cost, maximal count] of a knapsack item: a number is a cost with payoff 1; a list is
# [payoff, cost] or [payoff, cost, maximal count]; a list of rules or an association has the keys "Payoff", "Cost"
# and "MaxCount"
fn b29_item(it) = {
  if is_assoc(it) then b29_item(map(k => k -> lookup(it, k), keys(it)))
  else if is_list(it) and length(it) > 0 and every(it, r => b29_is_rule(r)) then {
    let get = (k, d) => { let hit = select(it, r => first(r) == k); if length(hit) == 0 then d else last(hit[1]) }
    [get("Payoff", 1), get("Cost", 1), get("MaxCount", inf)]
  }
  else if is_list(it) then [it[1], it[2], if length(it) >= 3 then it[3] else inf]
  else [1, it, inf]
}

# knapsack_solve(items, cap): the counts of each item that maximize the total payoff within the total cost cap
# (an unbounded or bounded integer knapsack by dynamic programming over the scaled costs); items given as an
# association or a list of rules give the counts the same way
fn b29_knap(its, cap) = {
  let rawcost = map(t => if is_inexact(t[2]) then rationalize(t[2]) else t[2], its)
  let sc = fold((a, c) => lcm(a, denominator(c)), 1, rawcost)
  let w = map(c => c*sc, rawcost)
  let cp = floor(cap*sc)
  let m = length(its)
  if not every(w, c => is_integer(c) and c > 0) or cp > 200000 then null
  else {
    var table = [constant_array(0, cp + 1)]
    for i in range(m) {
      let prev = last(table)
      let wi = w[i]
      let pay = its[i][1]
      let mx = min(its[i][3], floor(cp/wi))
      table = append(table, map(c => max(map(k => prev[c - k*wi + 1] + k*pay, range(0, min(mx, floor(c/wi))))), range(0, cp)))
    }
    var c = cp
    var out = constant_array(0, m)
    for t in range(m) {
      let i = m + 1 - t
      let wi = w[i]
      let target = table[i + 1][c + 1]
      let mx = min(its[i][3], floor(c/wi))
      let ks = select(range(0, mx), k => table[i][c - k*wi + 1] + k*its[i][1] == target)
      let k = if i == m then first(ks) else last(ks)
      out[i] = k
      c = c - k*wi
    }
    out
  }
}
fn b29_knap_items(items) = if is_assoc(items) then map(k => b29_item(lookup(items, k)), keys(items)) else map(t => b29_item(if b29_is_rule(t) then last(t) else t), items)
fn b29_knap_ok(its, cap) = {
  let rawcost = map(t => if is_inexact(t[2]) then rationalize(t[2]) else t[2], its)
  let sc = fold((a, c) => lcm(a, denominator(c)), 1, rawcost)
  length(its) > 0 and every(its, t => is_numeric(t[1]) and is_numeric(t[2]) and n(t[2]) > 0) and floor(cap*sc) <= 200000
}
rule knapsack_solve(items_, cap_) := {
  let res = b29_knap(b29_knap_items(items), cap)
  if is_assoc(items) then association_thread(keys(items), res)
  else if every(items, t => b29_is_rule(t)) then map(i => first(items[i]) -> res[i], range(length(items)))
  else res
} when (is_list(items) or is_assoc(items)) and is_numeric(cap) and b29_knap_ok(b29_knap_items(items), cap)

# --- the domain of the primes ---

# element(x, primes): whether the integer x is a prime (false for other numbers, a list element by element); other
# arguments stay unevaluated
rule element(x_, primes) := map(t => element(t, primes), x) when is_list(x)
rule element(x_, primes) := x > 1 and is_prime(x) when is_integer(x)
# an exact numeric expression is false only when it is a non-integer rational or its value is far from an integer;
# one that may be an integer, ((e + 7)^2 - e^2)/7 - 2 e say, is simplified first (and stays unevaluated if that
# does not decide it)
rule element(x_, primes) := false when is_numeric(x) and not is_integer(x) and is_real(n(x)) === true and (is_rational(x) or is_inexact(x) or abs(n(x, 30) - round(n(x, 30))) > 10^-20)
rule element(x_, primes) := element(simplify(x), primes) when is_numeric(x) and not is_integer(x) and not is_rational(x) and not is_inexact(x) and is_integer(simplify(x))

# is_prime(z, gaussian_integers: true): primality in Z[i]: a + b i with a b != 0 is prime when a^2 + b^2 is a
# rational prime; a unit times a rational integer n is prime when |n| is a prime congruent to 3 modulo 4 (Hardy and
# Wright, An Introduction to the Theory of Numbers, 15.1)
fn gauss_is_prime(z) = {
  let a = re(z)
  let b = im_part(z)
  if not (is_integer(a) and is_integer(b)) then false
  else if a != 0 and b != 0 then is_prime(a^2 + b^2)
  else { let m = abs(a + b); is_prime(m) and mod(m, 4) == 3 }
}
fn gauss_prime_opts(z; gaussian_integers = false) = if gaussian_integers === true then gauss_is_prime(z) else is_prime(z)
rule is_prime(z_, o_) := gauss_prime_opts(z, o) when not is_atom(o) and head(o) === options and is_number(z)

# --- convex optimization: quadratic, conic, second-order cone, semidefinite, robust, parametric ---
# All of them read the problem as constraints on the variables, turn norm bounds and matrix cones into polynomial
# inequalities (a bound norm(v) <= t is |v|^2 <= t^2 with t >= 0; a symmetric matrix is positive semidefinite
# when all its principal minors are non-negative) and minimize with the symbolic optimizer.

# b29_cone_conditions(c): the scalar conditions equivalent to the constraint c
fn b29_cone_conditions(c) = {
  if is_atom(c) then [c]
  else if head(c) === less_equal and length(c) == 2 and head(c[1]) === pow and c[1][2] == 1/2 then {
    let s = rewrite(c[1][1], abs(u_)^2 :> u^2)
    [s <= c[2]^2, c[2] >= 0]
  }
  else if head(c) === vector_greater_equal and length(c) == 2 and is_list(c[1]) and length(c[1]) == 2 and is_list(c[2]) and length(c[2]) >= 1 then {
    let kind = c[2][1]
    let v = c[1][1]
    let w = c[1][2]
    if kind == "SemidefiniteCone" and is_matrix(v) then {
      let m = if is_matrix(w) then v - w else v
      let k = length(m)
      map(s => det(map(i => map(j => m[i][j], s), s)) >= 0, select(subsets(range(k)), s => length(s) > 0))
    }
    else if kind == "NormCone" and is_list(v) then {
      let t = last(v) - (if is_list(w) then last(w) else w)
      let xs = most(v)
      [total(map(a => a^2, xs)) <= t^2, t >= 0]
    }
    else if kind == "NonNegativeCone" then [vector_greater_equal([v, w])]
    else [c]
  }
  else [c]
}

# b29_opt_result(f, rules, prop): the answer of an optimization in the form the property names: the rules (default),
# "PrimalMinimumValue", "PrimalMinimizer" (the values), or a list of properties
fn b29_opt_result(f, rules, prop) = {
  let ok = is_list(rules) and length(rules) > 0 and every(rules, r => b29_is_rule(r))
  if not ok then rules
  else if is_list(prop) then map(p => b29_opt_result(f, rules, p), prop)
  else if prop == "PrimalMinimumValue" then simplify(rewrite(f, rules))
  else if prop == "PrimalMinimizer" then map(r => last(r), rules)
  else rules
}

# (a repeated constraint is dropped: the robust forms repeat the constraints that do not depend on the parameters)
fn b29_opt_run(f, cons0, vars) = {
  let cons = if is_list(cons0) then unique(cons0) else cons0
  if c30_special(f, cons, vars) then c30_run(f, cons, vars) else optimization_point(f, flatten(map(b29_cone_conditions, if is_list(cons) then cons else [cons]), 1), vars)
}

rule quadratic_optimization(f_, cons_, vars_) := b29_opt_run(f, cons, vars) when not is_list(f) or not is_matrix(f[1])
rule quadratic_optimization(f_, cons_, vars_, prop_) := b29_opt_result(f, b29_opt_run(f, cons, vars), prop) when not is_list(f) or not is_matrix(f[1])
rule conic_optimization(f_, cons_, vars_) := b29_opt_run(f, cons, vars)
rule conic_optimization(f_, cons_, vars_, prop_) := b29_opt_result(f, b29_opt_run(f, cons, vars), prop)
rule second_order_cone_optimization(f_, cons_, vars_) := b29_opt_run(f, cons, vars) when not is_vector(f) or not is_symbol(vars)
rule second_order_cone_optimization(f_, cons_, vars_, prop_) := b29_opt_result(f, b29_opt_run(f, cons, vars), prop) when not is_vector(f) or not is_symbol(vars)
rule semidefinite_optimization(f_, cons_, vars_) := b29_opt_run(f, cons, vars) when not is_vector(f)
rule semidefinite_optimization(f_, cons_, vars_, prop_) := b29_opt_result(f, b29_opt_run(f, cons, vars), prop) when not is_vector(f)

# semidefinite_optimization(c, [a0, a1, ..., an]): the vector x minimizing c . x such that a0 + x1 a1 + ... + xn an is
# positive semidefinite
rule semidefinite_optimization(c_, mats_) := {
  let k = length(c)
  let xs = map(i => to_expression(string_join(["sdp_x", to_string(i)])), range(k))
  let m = map(i => map(j => mats[1][i][j] + total(map(t => xs[t]*mats[t + 1][i][j], range(k))), range(length(mats[1]))), range(length(mats[1])))
  let cons = b29_cone_conditions(vector_greater_equal([m, 0], ["SemidefiniteCone", length(m)]))
  let r = optimization_point(dot(c, xs), cons, xs)
  if is_list(r) and length(r) > 0 and every(r, t => b29_is_rule(t)) then map(t => last(t), r) else r
} when is_vector(c) and is_list(mats) and length(mats) == length(c) + 1 and every(mats, is_matrix)

# quadratic_optimization([q, c], [a, b]), ([q, c], [a, b], [aeq, beq]): the vector x minimizing x . q . x/2 + c . x
# under a . x + b >= 0 (and aeq . x + beq == 0)
rule quadratic_optimization(qc_, ab_) := b29_qmat(qc[1], qc[2], ab, [[], []]) when is_list(qc) and length(qc) == 2 and is_matrix(qc[1]) and lo_pair(ab)
rule quadratic_optimization(qc_, ab_, eq_) := b29_qmat(qc[1], qc[2], ab, eq) when is_list(qc) and length(qc) == 2 and is_matrix(qc[1]) and lo_pair(ab) and lo_pair(eq)
fn b29_qmat(q, c, ab, eq) = {
  let k = length(c)
  let xs = map(i => to_expression(string_join(["qo_x", to_string(i)])), range(k))
  let ineq = map(i => dot(ab[1][i], xs) + ab[2][i] >= 0, range(length(ab[1])))
  let equ = map(i => dot(eq[1][i], xs) + eq[2][i] == 0, range(length(eq[1])))
  let r = optimization_point(dot(xs, dot(q, xs))/2 + dot(c, xs), join(ineq, equ), xs)
  if is_list(r) and length(r) > 0 and every(r, t => b29_is_rule(t)) then map(t => last(t), r) else r
}

# parametric_convex_optimization(f, cons, vars, params), (..., prop): the function of the parameters that gives the
# minimizer (or the property) of the problem for given values of the parameters; up to three parameters
fn b29_param_solve(f, cons, vars, params, vals, prop) = {
  let sub = map(i => params[i] -> vals[i], range(length(params)))
  b29_opt_result(rewrite(f, sub), b29_opt_run(rewrite(f, sub), rewrite(cons, sub), vars), prop)
}
rule parametric_convex_optimization(f_, cons_, vars_, params_) := parametric_convex_optimization(f, cons, vars, params, "PrimalMinimizerRules")
rule parametric_convex_optimization(f_, cons_, vars_, params_, prop_) := {
  let ps = if is_list(params) then params else [params]
  let pk = length(ps)
  if pk == 1 then (a => b29_param_solve(f, cons, vars, ps, [a], prop))
  else if pk == 2 then ((a, b) => b29_param_solve(f, cons, vars, ps, [a, b], prop))
  else ((a, b, c) => b29_param_solve(f, cons, vars, ps, [a, b, c], prop))
} when (not is_list(params) or length(params) <= 3) and not is_string(params)

# robust_convex_optimization(f, for_all(params, region, cons), vars): the minimizer of f such that cons holds for every
# value of the parameters in the region. For a region that is a box (bounds on each parameter, written as inequalities
# or as element(p, interval([lo, hi]))) and constraints affine in the parameters the worst cases are at the corners of
# the box, so the problem is solved with the constraints at every corner; other regions stay unevaluated.
fn b29_box_region(r) = {
  if is_atom(r) then r
  else if head(r) === element and length(r) == 2 and head(r[2]) === interval and length(r[2]) == 1 then r[2][1][1] <= r[1] and r[1] <= r[2][1][2]
  else if is_list(r) then fold((a, b) => a and b, b29_box_region(r[1]), map(b29_box_region, rest(r)))
  else if head(r) === head(b29_p and b29_q) then fold((u, v) => u and v, true, map(b29_box_region, map(k => r[k], range(length(r)))))
  else r
}
fn b29_corners(ps, region) = {
  let reg = b29_box_region(region)
  let los = map(p => minimize([p, reg], ps), ps)
  let his = map(p => minimize([-p, reg], ps), ps)
  if not every(join(los, his), t => is_list(t) and length(t) == 2 and is_numeric(t[1])) then null
  else {
    let lo = map(t => t[1], los)
    let hi = map(t => -t[1], his)
    let cs = tuples([0, 1], length(ps))
    let corners = map(c => map(i => ps[i] -> (if c[i] == 0 then lo[i] else hi[i]), range(length(ps))), cs)
    if every(corners, c => rewrite(reg, c) === true) then corners else null
  }
}
rule robust_convex_optimization(f_, fa_, vars_) := {
  let ps = if is_list(fa[1]) then fa[1] else [fa[1]]
  let cons = if is_list(fa[3]) then fa[3] else [fa[3]]
  let corners = b29_corners(ps, fa[2])
  b29_opt_run(f, flatten(map(c => rewrite(cons, c), corners), 1), vars)
} when head(fa) === for_all and length(fa) == 3 and b29_corners(if is_list(fa[1]) then fa[1] else [fa[1]], fa[2]) !== null
rule robust_convex_optimization(f_, fa_, vars_, prop_) := b29_opt_result(f, robust_convex_optimization(f, fa, vars), prop) when head(fa) === for_all

# --- distributions: empirical, multivariate Poisson, Wishart matrix ---
# These are inert terms like the families of mc-dist; the statistics below are rules on them.

# b29_emp(d): [values, weights] of empirical_distribution(data) or empirical_distribution(weights -> data), the
# values sorted and equal ones merged, the weights summing to 1
fn b29_emp(d) = {
  let a = d[1]
  let weighted = b29_is_rule(a)
  let xs = if weighted then last(a) else a
  let ws = if weighted then first(a) else constant_array(1, length(xs))
  let total_w = total(ws)
  let ord = sort(unique_by(xs, x => x))
  let wsum = v => total(map(k => ws[k], select(range(length(xs)), k => xs[k] === v)))/total_w
  [ord, map(wsum, ord)]
}
rule pdf(d_, x_) := { let e = b29_emp(d); let h = select(range(length(e[1])), k => e[1][k] == x); if length(h) == 0 then 0 else e[2][h[1]] } when head(d) === empirical_distribution and not is_list(x)
rule cdf(d_, x_) := { let e = b29_emp(d); total(map(k => e[2][k], select(range(length(e[1])), k => e[1][k] <= x))) } when head(d) === empirical_distribution and not is_list(x)
rule mean(d_) := { let e = b29_emp(d); total(map(k => e[1][k]*e[2][k], range(length(e[1])))) } when head(d) === empirical_distribution
rule variance(d_) := { let e = b29_emp(d); let m = mean(d); total(map(k => (e[1][k] - m)^2*e[2][k], range(length(e[1])))) } when head(d) === empirical_distribution
rule standard_deviation(d_) := sqrt(variance(d)) when head(d) === empirical_distribution
rule quantile(d_, q_) := { let e = b29_emp(d); let cum = accumulate(e[2]); e[1][first(select(range(length(cum)), k => cum[k] >= q))] } when head(d) === empirical_distribution and is_numeric(q) and q > 0 and q <= 1
rule median(d_) := quantile(d, 1/2) when head(d) === empirical_distribution
rule random_variate(d_) := { let e = b29_emp(d); let u = random_real(); let cum = accumulate(e[2]); e[1][first(select(range(length(cum)), k => cum[k] >= u))] } when head(d) === empirical_distribution
rule random_variate(d_, k_) := map(t => random_variate(d), range(k)) when head(d) === empirical_distribution and is_integer(k)

# multivariate_poisson_distribution(m0, [m1, ..., mk]): the vector (Y0 + Y1, ..., Y0 + Yk) of sums of independent
# Poisson variables of means m0 and mi
rule mean(d_) := map(m => d[1] + m, d[2]) when head(d) === multivariate_poisson_distribution
rule covariance(d_) := map(i => map(j => if i == j then d[1] + d[2][i] else d[1], range(length(d[2]))), range(length(d[2]))) when head(d) === multivariate_poisson_distribution
rule variance(d_) := map(m => d[1] + m, d[2]) when head(d) === multivariate_poisson_distribution
rule random_variate(d_) := { let y0 = random_variate(poisson_distribution(d[1])); map(m => y0 + random_variate(poisson_distribution(m)), d[2]) } when head(d) === multivariate_poisson_distribution
rule random_variate(d_, k_) := map(t => random_variate(d), range(k)) when head(d) === multivariate_poisson_distribution and is_integer(k)
rule pdf(d_, xs_) := {
  let m0 = d[1]
  let ms = d[2]
  let top = min(xs)
  sum(map(j => exp(-m0)*m0^j/factorial(j)*product(map(i => exp(-ms[i])*ms[i]^(xs[i] - j)/factorial(xs[i] - j), range(length(ms)))), range(0, top)))
} when head(d) === multivariate_poisson_distribution and is_list(xs) and length(xs) == length(d[2]) and every(xs, x => is_integer(x) and x >= 0)

# wishart_matrix_distribution(nu, sigma): the sum of nu outer products x x^T of vectors of the multinormal
# distribution of covariance sigma (for integer nu >= the dimension): mean nu sigma, variance nu (sigma_ij^2 +
# sigma_ii sigma_jj) elementwise, and the density det(x)^((nu - p - 1)/2) exp(-tr(sigma^-1 x)/2)/(2^(nu p/2) det(sigma)^(nu/2) Gamma_p(nu/2))
rule mean(d_) := d[1]*d[2] when head(d) === wishart_matrix_distribution
rule variance(d_) := map(i => map(j => d[1]*(d[2][i][j]^2 + d[2][i][i]*d[2][j][j]), range(length(d[2]))), range(length(d[2]))) when head(d) === wishart_matrix_distribution
rule random_variate(d_) := {
  let p = length(d[2])
  let l = cholesky(n(d[2]))
  let zs = map(t => dot(l, map(i => random_variate(normal_distribution(0, 1)), range(p))), range(d[1]))
  total(map(z => map(i => map(j => z[i]*z[j], range(p)), range(p)), zs))
} when head(d) === wishart_matrix_distribution and is_integer(d[1]) and d[1] >= length(d[2]) and is_matrix(d[2])
rule random_variate(d_, k_) := map(t => random_variate(d), range(k)) when head(d) === wishart_matrix_distribution and is_integer(k)
rule pdf(d_, x_) := {
  let nu = d[1]
  let sg = d[2]
  let p = length(sg)
  let mgamma = pi^(p*(p - 1)/4)*product(map(j => gamma(nu/2 + (1 - j)/2), range(p)))
  det(x)^((nu - p - 1)/2)*exp(-trace(dot(inverse(sg), x))/2)/(2^(nu*p/2)*det(sg)^(nu/2)*mgamma)
} when head(d) === wishart_matrix_distribution and is_matrix(x) and is_matrix(d[2])

# --- constants given by series ---
# `n` hands khinchin and champernowne_number(b) to b29_n_constant(x, digits) (digits 0: machine precision).

# khinchin: Khinchin's constant 2.68545..., the geometric mean of the continued fraction terms of almost every real
# number; ln K = (1/ln 2) sum_(k >= 1) (zeta(2k) - 1)/k (1 - 1/2 + ... - 1/(2k - 1)) (Khinchin; Bailey, Borwein and
# Crandall 1997), whose terms fall like 4^-k
fn b29_khinchin(p) = {
  let w = p + 12
  var s = 0
  var alt = 0
  for k in range(ceiling(1.7*w) + 2) {
    alt = alt + (-1)^(2*k)/(2*k - 1) - (if k > 1 then 1/(2*k - 2) else 0)
    s = s + (n(zeta(2*k), w + ceiling(1.25*k) + 6) - 1)/k*n(alt, w)
  }
  n(exp(s/log(2)), p)
}

# champernowne_number(b): the base-b number 0.123456789(10)(11)... whose digits are the integers written one after
# another (10 by default)
fn b29_champernowne(b, p) = {
  let digits = ceiling(p*log(10)/log(b)) + 8
  var acc = 0
  var count = 0
  var k = 1
  while count < digits {
    let ds = integer_digits(k, b)
    acc = acc*b^length(ds) + from_digits(ds, b)
    count = count + length(ds)
    k = k + 1
  }
  n(acc/b^count, p)
}
rule champernowne_number() := champernowne_number(10)
fn b29_n_constant(x, d) = {
  let p = if d == 0 then 25 else d
  let v = if x === khinchin then b29_khinchin(p) else if is_integer(x[1]) and x[1] >= 2 then b29_champernowne(x[1], p) else null
  if v === null then x else if d == 0 then n(v) else v
}

# --- c30: integer and complex variables, inactive forms, in the convex optimizers ---
# Variables declared element(x, integers) (or constrained so, also nonnegative/nonpositive integers) are found by branch
# and bound over the symbolic relaxation: a fractional relaxed value v of an integer variable splits the problem into
# x <= floor(v) and x >= floor(v) + 1; a node is pruned when its relaxed value cannot beat the best integer point
# (Land and Doig 1960). Exact relaxations stay exact. A complex variable z (element(z, complexes)) is split into the
# real variables of its real and imaginary parts; a complex vector inequality means both parts compare (the cone of
# the reals and the imaginary axis). inactive(add) and inactive(norm) are taken as their active forms.

fn c30_prep(e) = rewrite(e, [inactive(add)(u_, w_) :> u + w, inactive(norm)(u_) :> sqrt(total(map(a => abs(a)^2, u)))])
fn c30_domain_of(d) = if d === integers or d === nonnegative_integers or d === nonpositive_integers then 1 else if d === complexes then 2 else if d === reals then 0 else 3
fn c30_is_el(c) = not is_atom(c) and head(c) === element and length(c) == 2
fn c30_conj(c) = {
  if is_list(c) then flatten(map(c30_conj, c), 1)
  else if not is_atom(c) and head(c) === head(c30_p and c30_q) then flatten(map(k => c30_conj(c[k]), range(length(c))), 1)
  else [c]
}
fn c30_entry(v) = if c30_is_el(v) then [v[1], c30_domain_of(v[2]), v[2]] else [v, 0, reals]
fn c30_special(f, cons, vars) = {
  let vl = if is_list(vars) then vars else [vars]
  length(select(vl, c30_is_el)) > 0 or not free_of([f, cons], element) or not free_of([f, cons], inactive)
}
fn c30_solve(f, cons, vs) = {
  let r = optimization_point(f, cons, vs)
  if is_list(r) and length(r) > 0 and every(r, t => b29_is_rule(t)) and free_of(r, indeterminate) and free_of(r, minimize) then r else null
}
fn c30_frac(v, r) = {
  let a = simplify(rewrite(v, r))
  if is_integer(a) then false else abs(n(a) - round(n(a))) > 1/10000000
}
# the tie-break between integer points of equal value: the lexicographically smaller point
fn c30_lex_less(a, b) = {
  var out = false
  var done = false
  for k in range(length(a)) {
    if not done and abs(a[k] - b[k]) > 1/1000000000 then { out = a[k] < b[k]; done = true }
  }
  out
}
fn c30_bb(f, cons, vs, ints) = {
  var best = null
  var best_val = 0
  var stack = [cons]
  var nodes = 0
  while length(stack) > 0 and nodes < 300 {
    let node = first(stack)
    stack = rest(stack)
    nodes = nodes + 1
    let r = c30_solve(f, node, vs)
    if not (r === null) then {
      let val = n(simplify(rewrite(f, r)))
      if best === null or val <= best_val + 1/1000000000 then {
        let bad = select(ints, v => c30_frac(v, r))
        if length(bad) == 0 then {
          let cand = map(t => if length(select(ints, u => u === t[1])) > 0 then t[1] -> round(t[2]) else t, r)
          if best === null or val < best_val - 1/1000000000 or c30_lex_less(map(t => n(t[2]), cand), map(t => n(t[2]), best)) then {
            best = cand
            best_val = val
          }
        } else {
          let v = bad[1]
          let x0 = n(rewrite(v, r))
          stack = join([join(node, [v <= floor(x0)]), join(node, [v >= floor(x0) + 1])], stack)
        }
      }
    }
  }
  best
}
fn c30_cparts(d) = if is_list(d) then flatten(map(c30_cparts, d), 1) else [complex_expand(re(d)) >= 0, complex_expand(im_part(d)) >= 0]
fn c30_ccond(c) = {
  if is_atom(c) then [c]
  else if (head(c) === vector_greater_equal or head(c) === vector_less_equal) and length(c) == 1 and is_list(c[1]) and length(c[1]) == 2 then {
    let d = complex_expand(if head(c) === vector_greater_equal then c[1][1] - c[1][2] else c[1][2] - c[1][1])
    select(c30_cparts(d), t => not (t === true))
  }
  else if head(c) === equal and length(c) == 2 and not is_list(c[1]) then {
    let d = complex_expand(c[1] - c[2])
    select([complex_expand(re(d)) == 0, complex_expand(im_part(d)) == 0], t => not (t === true))
  }
  else if (head(c) === less_equal or head(c) === greater_equal) and length(c) == 2 then [apply(head(c), [complex_expand(c[1]), complex_expand(c[2])])]
  else [c]
}
fn c30_opt(f, cons0, vars) = {
  let cons = if is_list(cons0) then unique(cons0) else cons0
  if c30_special(f, cons, vars) then c30_run(f, cons, vars) else optimization_point(f, cons, vars)
}
# a vector variable x (used through dot, total or indexed) is split into its entries first, as optimization_point does
fn c30_run(f, cons, vars) = {
  let vs = if is_list(vars) then vars else [vars]
  let isvec = length(vs) == 1 and is_symbol(vs[1]) and not free_of([f, cons], vs[1]) and lo_is_vector_use(f, vs[1])
  if isvec then {
    let dim = lo_dimension([f, cons], vs[1])
    let xs = map(k => to_expression(string_join(["lo_v", to_string(k)])), range(dim))
    let r = c30_run1(rewrite(f, vs[1] -> xs), rewrite(cons, vs[1] -> xs), xs)
    if is_list(r) and length(r) > 0 and every(r, t => b29_is_rule(t)) then [vs[1] -> map(t => t[2], r)] else r
  } else c30_run1(f, cons, vars)
}
fn c30_run1(f0, cons0, vars) = {
  let f1 = c30_prep(f0)
  let cl = flatten(map(c30_conj, c30_prep(if is_list(cons0) then cons0 else [cons0])), 1)
  let elem = select(cl, c30_is_el)
  let ents = map(e => [e[1], max(join([e[2]], map(c => c30_domain_of(c[2]), select(elem, c => c[1] === e[1] and c30_domain_of(c[2]) < 3)))), e[3]], map(c30_entry, if is_list(vars) then vars else [vars]))
  let others = select(cl, c => not (c30_is_el(c) and c30_domain_of(c[2]) < 3))
  let sign = flatten(map(e => if e[3] === nonnegative_integers then [e[1] >= 0] else if e[3] === nonpositive_integers then [e[1] <= 0] else [], ents), 1)
  let cplx = select(ents, e => e[2] == 2)
  let sub = map(e => e[1] -> to_expression(string_join(["c30re", to_string(e[1])])) + im*to_expression(string_join(["c30im", to_string(e[1])])), cplx)
  let f2 = if length(cplx) == 0 then f1 else complex_expand(rewrite(f1, sub))
  let c2 = join(others, sign)
  let c3 = if length(cplx) == 0 then c2 else flatten(map(c30_ccond, rewrite(c2, sub)), 1)
  let c4 = flatten(map(b29_cone_conditions, c3), 1)
  let vs = flatten(map(e => if e[2] == 2 then [to_expression(string_join(["c30re", to_string(e[1])])), to_expression(string_join(["c30im", to_string(e[1])]))] else [e[1]], ents), 1)
  let ints = map(e => e[1], select(ents, e => e[2] == 1))
  let r = if length(ints) == 0 then c30_solve(f2, c4, vs) else c30_bb(f2, c4, vs, ints)
  if r === null then optimization_point(f0, cons0, vars)
  else flatten(map(e => if e[2] == 2 then [e[1] -> simplify(rewrite(to_expression(string_join(["c30re", to_string(e[1])])) + im*to_expression(string_join(["c30im", to_string(e[1])])), r))] else select(r, t => t[1] === e[1]), ents), 1)
}

# prime_omega(z), prime_omega(n, gaussian_integers: true): the number of prime factors, with multiplicity, in the
# Gaussian integers. A Gaussian integer of norm N = a^2 + b^2 has, for every rational prime p with multiplicity e in N,
# e prime factors above 2 and above p = 1 (mod 4) (each of norm p), and e/2 above p = 3 (mod 4) (each p itself, of
# norm p^2); units have none (Hardy and Wright, ch. 12).
fn c30_gauss_omega(a, b) = {
  let fs = factor_integer(a^2 + b^2)
  total(map(f => if mod(f[1], 4) == 3 then f[2]/2 else f[2], fs))
}
rule prime_omega(z_) := c30_gauss_omega(re(z), im_part(z)) when is_integer(re(z)) and is_integer(im_part(z)) and not (im_part(z) == 0) and abs(re(z)) + abs(im_part(z)) > 1
rule prime_omega(z_, gaussian_integers: g_) := if g == true then (if is_integer(z) and z != 0 then (if abs(z) == 1 then 0 else c30_gauss_omega(z, 0)) else prime_omega(z)) else prime_omega(z) when is_integer(z) and z != 0 or (is_integer(re(z)) and is_integer(im_part(z)))

# from_digits(ds), from_digits(ds, b) with symbolic digits or base: the polynomial sum of d_k b^(n - k) (Horner's rule)
rule from_digits(ds_) := fold((acc, d) => acc*10 + d, 0, ds) when is_list(ds) and length(ds) > 0 and not every(ds, is_integer) and (every(ds, is_list) or every(ds, d => not is_list(d) and not is_string(d)))
rule from_digits(ds_, b_) := fold((acc, d) => acc*b + d, 0, ds) when is_list(ds) and length(ds) > 0 and not (every(ds, is_integer) and is_integer(b)) and (every(ds, is_list) or every(ds, d => not is_list(d) and not is_string(d)))

# --- c30: options and forms of linear algebra functions ---
# matrix_log and permanent take a method (Jordan, Schur, Ryser, ...): every method gives the same value
rule matrix_log(m_, method: mth_) := matrix_log(m) when is_matrix(m)
rule permanent(m_, method: mth_) := permanent(m) when is_matrix(m)
# distance_matrix(points, distance_function: f): f applied to every pair of points
rule distance_matrix(pts_, distance_function: df_) := map(a => map(b => df(a, b), pts), pts) when is_list(pts) and length(pts) > 0
# identity_matrix(n, sparse_array, working_precision: p): the identity matrix at precision p (machine numbers at machine precision)
rule identity_matrix(k_, sparse_array, working_precision: p_) := if p > 15.9 and p < 16 then n(identity_matrix(k)) else n(identity_matrix(k), p) when is_integer(k) and k > 0
# cross(v1, ..., v(k)) of k vectors in k + 1 dimensions: the vector whose i-th entry is (-1)^(i + k + 1) times the
# determinant of the vectors without their i-th entry (so that its dot product with each vector is 0)
fn c30_gcross(vs) = {
  let dim = length(vs[1])
  map(i => (-1)^(i + dim)*det(map(v => map(j => v[j], select(range(dim), j => not (j == i))), vs)), range(dim))
}
rule cross(a_, b_, c_) := c30_gcross([a, b, c]) when is_list(a) and length(a) == 4 and length(b) == 4 and length(c) == 4
rule cross(a_, b_, c_, d_) := c30_gcross([a, b, c, d]) when is_list(a) and length(a) == 5 and length(b) == 5 and length(c) == 5 and length(d) == 5
# a scalar against a vector: the scalar is taken at every entry
rule euclidean_distance(u_, w_) := rewrite(sqrt(total(abs(u - w)^2)), (abs(z_)^2)^(1/2) :> abs(z)) when is_list(w) and not is_list(u)
rule manhattan_distance(u_, w_) := total(abs(u - w)) when is_list(w) and not is_list(u)
rule manhattan_distance(u_, w_) := total(abs(u - w)) when is_list(u) and not is_list(w)
rule euclidean_distance(u_, w_) := rewrite(sqrt(total(abs(u - w)^2)), (abs(z_)^2)^(1/2) :> abs(z)) when is_list(u) and not is_list(w)
# total(xs, method: "CompensatedSummation"): Neumaier's compensated sum (a running compensation for the low bits
# lost by each addition); the default total is the plain sum in the canonical order (the same packed, unpacked and
# compiled), which "Pairwise" and "Ordinary" name
rule total(xs_, method: mth_) := nl_total_compensated(xs) when mth == "CompensatedSummation" and is_list(xs)
# what the native passes on: reals that are not all machine numbers (the same sum, folded), anything else (the plain total)
rule b29_total_fold(xs_) := {
  let r = fold((acc, x) => {
    let t = acc[1] + x
    [t, acc[2] + (if abs(acc[1]) >= abs(x) then (acc[1] - t) + x else (x - t) + acc[1])]
  }, [0.0, 0.0], xs)
  r[1] + r[2]
} when every(xs, is_real)
rule b29_total_fold(xs_) := total(xs)
rule total(xs_, method: mth_) := total(xs) when (mth == "Pairwise" or mth == "Ordinary") and is_list(xs) and every(xs, is_real)

# lo_cmp(a, b, op): op applied entry by entry to lists of any depth (a scalar against a list is taken at every entry)
fn lo_cmp(a, b, op) = if is_list(a) or is_list(b) then flatten(map(k => lo_cmp(if is_list(a) then a[k] else a, if is_list(b) then b[k] else b, op), range(if is_list(a) then length(a) else length(b))), 1) else [op(a, b)]

# flatten(a, [[l1, l2, ...], [l3, ...], ...]): the levels of a gathered into new levels, the levels of each group
# merged (in the order of the group) into one; the groups give the new levels in order. The array is permuted so that
# the levels in the groups follow each other, then every group is merged.
fn c30_permute_at(a, perm, dn, js, lev) = {
  let r = length(perm)
  if lev > r then fold((x, i) => x[i], a, map(k => js[perm[k]], range(r)))
  else map(j => c30_permute_at(a, perm, dn, join(js, [j]), lev + 1), range(dn[lev]))
}
fn c30_permute(a, perm) = {
  let d = dimensions(a)
  let r = length(perm)
  let dn = map(j => d[select(range(r), k => perm[k] == j)[1]], range(r))
  c30_permute_at(a, perm, dn, [], 1)
}
# merge c consecutive levels starting at level d
fn c30_merge(a, d, c) = if c == 1 then a else if d == 1 then flatten(map(row => c30_merge(row, 1, c - 1), a), 1) else map(row => c30_merge(row, d - 1, c), a)
rule flatten(a_, spec_) := {
  let flat = flatten(spec, 1)
  let r = length(flat)
  let perm = map(k => first(select(range(r), j => flat[j] == k)), range(r))
  let t = c30_permute(a, perm)
  var out = t
  var starts = []
  var s = 1
  for g in spec {
    starts = join(starts, [s])
    s = s + length(g)
  }
  for gi in range(length(spec), 1, -1) {
    out = c30_merge(out, starts[gi], length(spec[gi]))
  }
  out
} when is_list(a) and is_list(spec) and length(spec) > 0 and every(spec, g => is_list(g) and length(g) > 0 and every(g, is_integer)) and sort(flatten(spec, 1)) == range(length(dimensions(a)))

# real_valued_numeric_q(x): x is a numeric expression with a real value
fn real_valued_numeric_q(x) = is_numeric(x) and is_real(n(x))

# operator forms whose data comes last: nest_list(f, n)(x) is nest_list(f, x, n), nest(f, n)(x) is nest(f, x, n)
rule nest_list(f_, n_)(x_) := nest_list(f, x, n) when is_integer(n)
rule nest(f_, n_)(x_) := nest(f, x, n) when is_integer(n)

# qr(m, full): the factorization with a square q: the columns of q completed to an orthonormal basis of the space
# and r padded with zero rows (m == dot(q, r) still)
rule qr(m_, full) := nl_qr_full(m) when is_matrix(m)
rule qr(m_, "Full") := nl_qr_full(m) when is_matrix(m)
fn nl_qr_full(m) = {
  let t = qr(m)
  let q = t[1]
  let r = t[2]
  let rows = length(m)
  let k = length(r)
  if k >= rows then t else {
    let basis = orthogonalize(join(transpose(q), identity_matrix(rows)))
    [transpose(basis), join(r, constant_array(constant_array(0, length(r[1])), rows - k))]
  }
}

# array_reshape(xs, dims, "Extrapolated"): the missing entries continue the last step of the data linearly
rule array_reshape(xs_, dims_, "Extrapolated") := {
  let fl = flatten(xs)
  let total = apply(mul, dims)
  let step = fl[-1] - fl[-2]
  array_reshape(join(fl, table(fl[-1] + k*step, k, total - length(fl))), dims)
} when is_list(xs) and is_list(dims) and length(flatten(xs)) >= 2 and apply(mul, dims) > length(flatten(xs))

# rotate_left(x, [n1, n2, ...]), rotate_right(x, [n1, ...]): level k rotated by nk (rows, then columns, ...)
fn nl_rot(x, ns, left) = if length(ns) == 0 or not (is_list(x) or is_assoc(x)) then x else map(r => nl_rot(r, rest(ns), left), if left then rotate_left(x, ns[1]) else rotate_right(x, ns[1]))
rule rotate_left(x_, ns_) := nl_rot(x, ns, true) when (is_list(x) or is_assoc(x)) and is_list(ns) and length(ns) > 0
rule rotate_right(x_, ns_) := nl_rot(x, ns, false) when (is_list(x) or is_assoc(x)) and is_list(ns) and length(ns) > 0

# take_list(m, rows, cols): the matrix cut into blocks, [row block][column block]
rule take_list(m_, rs_, cs_) := map(b => map(c => transpose(c), take_list(transpose(b), cs)), take_list(m, rs)) when is_matrix(m) and is_list(rs) and is_list(cs)

# orthogonalize(vs, tolerance: t): Gram-Schmidt dropping the vectors whose remainder is shorter than t
rule orthogonalize(vs_, tolerance: t_) := {
  var basis = []
  for v in vs {
    var w = v
    for e in basis { w = w - dot(w, conjugate(e))*e }
    let nw = norm(w)
    if nw >= t then basis = append(basis, w/nw)
  }
  basis
} when is_list(vs)

# flatten_at(n) is the operator form: flatten_at(n)(x) is flatten_at(x, n)
rule flatten_at(n_) := nl_flatten_at_op(n)
rule nl_flatten_at_op(n_)(x_) := flatten_at(x, n)

# rotation_matrix(theta, [u, v]): the rotation by theta in the plane of the vectors u and v (from u towards v), the
# identity on the orthogonal complement: I + (cos t - 1) (a a^H + b b^H) + sin t (b a^H - a b^H) with (a, b) an
# orthonormal basis of the plane (Gram-Schmidt of u, v with the Hermitian product <x, y> = conjugate(x) . y, so that
# complex vectors give a unitary matrix; for real vectors a^H = a^T)
rule rotation_matrix(t_, uv_) := {
  let a = uv[1]/norm(uv[1])
  let w = uv[2] - dot(conjugate(a), uv[2])*a
  let b = w/norm(w)
  let n = length(a)
  map(i => map(j => simplify(boole(i == j) + (cos(t) - 1)*(a[i]*conjugate(a[j]) + b[i]*conjugate(b[j])) + sin(t)*(b[i]*conjugate(a[j]) - a[i]*conjugate(b[j]))), range(n)), range(n))
} when is_list(uv) and length(uv) == 2 and is_list(uv[1]) and is_list(uv[2]) and length(uv[1]) == length(uv[2]) and not is_list(t)
# rotation_matrix([u, v]) for complex vectors (round 36): the unitary map I + E (U - I) E^H, E = [a, b] the
# orthonormal basis of the plane as above, U = [[al, -conjugate(be)], [be, conjugate(al)]] with v/|v| = al a + be b,
# so u/|u| goes to v/|v| and the orthogonal complement is fixed (real vectors keep the native rotation)
rule rotation_matrix(uv_) := {
  let a = uv[1]/norm(uv[1])
  let w = uv[2] - dot(conjugate(a), uv[2])*a
  let b = w/norm(w)
  let vn = uv[2]/norm(uv[2])
  let al = dot(conjugate(a), vn)
  let be = dot(conjugate(b), vn)
  let n = length(a)
  map(i => map(j => simplify(boole(i == j) + (al - 1)*a[i]*conjugate(a[j]) - conjugate(be)*a[i]*conjugate(b[j]) + be*b[i]*conjugate(a[j]) + (conjugate(al) - 1)*b[i]*conjugate(b[j])), range(n)), range(n))
} when is_list(uv) and length(uv) == 2 and is_list(uv[1]) and is_list(uv[2]) and length(uv[1]) == length(uv[2]) and not every(join(uv[1], uv[2]), x => is_real(n(x)))




# variance(weighted_data(d, w)): sum w (d - m)^2 / sum w with m the weighted mean (frequency weights: the variance
# of the distribution the weights give the values, round 37); numbers by two passes (no cancellation), other
# weights as one fraction, (W sum w d^2 - (sum w d)^2)/W^2 with its numerator expanded
fn r37_weighted_ok(d, w) = is_list(d) and is_list(w) and length(d) == length(w) and length(d) > 0 and every(d, x => not is_list(x))
rule variance(weighted_data(d_, w_)) := {
  let tw = total(w)
  if every(join(d, w), x => is_number(x)) then dot(w, (d - dot(d, w)/tw)^2)/tw else expand(tw*dot(w, d^2) - dot(w, d)^2)/tw^2
} when r37_weighted_ok(d, w)
rule std_dev(weighted_data(d_, w_)) := sqrt(variance(weighted_data(d, w))) when r37_weighted_ok(d, w)

# conjugate_transpose(a, perm): the conjugate of transpose(a, perm)
rule conjugate_transpose(a_, p_) := conjugate(transpose(a, p)) when is_list(a) and is_list(p)

# --- ordering, frequency, missing values, distances and palindromes ---

# reverse_sort_by(xs, f): the elements of xs in decreasing order of f(x); reverse_sort_by(xs, f, p): the order of
# sort_by(xs, f, p) reversed
fn reverse_sort_by(xs, f) = reverse(sort_by(xs, f))
fn reverse_sort_by(xs, f, p) = reverse(sort_by(xs, f, p))

# ordering_by(xs, f), ordering_by(xs, f, n): the positions of the elements of xs in the canonical order of f(x)
# (the first n, the last -n; all: every one); for an association, of its values; ordering_by(f)(xs);
# ordering_by(xs, f, k, p): the keys f(x) ordered by the ordering function p
fn ordering_by(xs, f) = ordering(map(f, if is_assoc(xs) then values(xs) else xs))
fn ordering_by(xs, f, k) = if k === all then ordering_by(xs, f) else ordering(map(f, if is_assoc(xs) then values(xs) else xs), k)
fn ordering_by(f) = xs => ordering_by(xs, f)
fn ordering_by(xs, f, k, p) = ordering(map(f, if is_assoc(xs) then values(xs) else xs), k, p)

# commonest(xs): the elements that occur most often, in order of first occurrence; commonest(xs, n): the n most
# common (ties by first occurrence), in order of first occurrence; commonest(xs, up_to(n)): at most n of them
fn commonest(xs) = { let t = tally(xs); let c = max(map(p => p[2], t)); map(p => p[1], select(t, p => p[2] == c)) }
fn commonest(xs, n: Int) when n >= 0 = {
  let t = tally(xs)
  let top = take(sort_by(range(length(t)), i => [-t[i][2], i]), min(n, length(t)))
  map(i => t[i][1], sort(top))
}
fn commonest(xs, u) when match_q(u, up_to(_)) and is_integer(u[1]) and u[1] >= 0 = commonest(xs, u[1])

# delete_missing(xs): the elements of the list xs (values of the association xs) that are not missing(...)
fn delete_missing(xs) when is_list(xs) = select(xs, x => not is_missing(x) and not (match_q(x, _ -> _) and is_missing(x[2])))
fn delete_missing(a) when is_assoc(a) = select_keys(a, k => not is_missing(a[k]))
# delete_missing(xs, n): down to level n of nested lists and associations; delete_missing(xs, n, d): the elements
# at levels 1 to n that hold a missing(...) at most d levels inside them (lists and associations)
fn l38_has_missing(e, k) = if is_missing(e) then true else if k === inf or (is_integer(k) and k >= 1) then (if is_list(e) then any_true(e, u => l38_has_missing(u, k - 1)) else if is_assoc(e) then any_true(values(e), u => l38_has_missing(u, k - 1)) else false) else false
fn l38_level_ok(n) = n === inf or (is_integer(n) and n >= 0)
fn delete_missing(xs, n) when (is_list(xs) or is_assoc(xs)) and l38_level_ok(n) =
  if n === 0 then xs else map(u => if is_list(u) or is_assoc(u) then delete_missing(u, n - 1) else u, delete_missing(xs))
fn delete_missing(xs, n, d) when (is_list(xs) or is_assoc(xs)) and l38_level_ok(n) and l38_level_ok(d) = {
  if n === 0 then xs else {
    let kept = if is_list(xs) then select(xs, u => not l38_has_missing(u, d)) else select_keys(xs, k => not l38_has_missing(xs[k], d))
    map(u => if is_list(u) or is_assoc(u) then delete_missing(u, n - 1, d) else u, kept)
  }
}

# hamming_distance(a, b): the number of places where the strings or lists a and b (of one length) differ
# hamming_distance(a, b, ignore_case: true): the distance of the strings in lower case
fn hamming_distance(a, b; ignore_case = false) when is_string(a) and is_string(b) =
  if ignore_case === true then hamming_distance(characters(to_lower(a)), characters(to_lower(b))) else hamming_distance(characters(a), characters(b))
fn hamming_distance(a: Str, b: Str) = hamming_distance(characters(a), characters(b))
fn hamming_distance(a, b) when is_list(a) and is_list(b) and length(a) == length(b) = count_if(range(length(a)), i => a[i] !== b[i])

# edit_distance(a, b): the Levenshtein distance of the strings or lists a and b, the least number of insertions,
# deletions and substitutions of one element that turn a into b (the dynamic programme of Wagner and Fischer, J. ACM
# 21 (1974), one row at a time)
fn edit_distance(a: Str, b: Str) = edit_distance(characters(a), characters(b))
fn edit_distance(a, b) when is_list(a) and is_list(b) = {
  let m = length(a)
  let k = length(b)
  var row = range(0, k)
  for i in range(m) {
    var next = [i]
    for j in range(k) {
      next = append(next, min(row[j + 1] + 1, next[j] + 1, row[j] + (if a[i] === b[j] then 0 else 1)))
    }
    row = next
  }
  row[k + 1]
}

# chessboard_distance(u, v): the Chebyshev distance max |u_i - v_i| of two vectors (or numbers)
fn chessboard_distance(u, v) when is_list(u) and is_list(v) and length(u) == length(v) and length(u) >= 1 = max(map(i => abs(u[i] - v[i]), range(length(u))))
fn chessboard_distance(u, v) when not is_list(u) and not is_list(v) = abs(u - v)

# squared_euclidean_distance(u, v): the square sum |u_1 - v_1|^2 + ... of the differences of two vectors (or numbers)
fn squared_euclidean_distance(u, v) when is_list(u) and is_list(v) and length(u) == length(v) = total(map(i => abs(u[i] - v[i])^2, range(length(u))))
fn squared_euclidean_distance(u, v) when not is_list(u) and not is_list(v) = abs(u - v)^2

# palindrome_q(x): whether the list, the string or the digits of the integer x read the same backwards
fn palindrome_q(x: Str) = x === string_reverse(x)
fn palindrome_q(x) when is_list(x) = x === reverse(x)
fn palindrome_q(x) when is_integer(x) = palindrome_q(integer_digits(x))
fn palindrome_q(x) when not is_list(x) and not is_string(x) and not is_integer(x) = false

# --- more argument forms of list and structure functions ---

# cross([x, y]): the vector [-y, x], the plane vector turned a quarter anticlockwise (orthogonal to [x, y], of the
# same length)
rule cross(v_) := [-v[2], v[1]] when is_list(v) and length(v) == 2 and not is_list(v[1]) and not is_list(v[2])

# head(e, f): f applied to the head of e
rule head(e_, f_) := f(head(e))

# flatten(xs, [[2], [1]]): the transpose of a list of lists of any lengths: the j-th list holds the j-th elements of
# the lists that have one
rule flatten(xs_, spec_) := {
  let m = if length(xs) == 0 then 0 else max(map(length, xs))
  map(j => map(x => x[j], select(xs, x => length(x) >= j)), range(m))
} when is_list(xs) and spec === [[2], [1]] and every(xs, x => is_list(x))

# subsets(xs, spec, [i, j, s]): the subsets numbered i, i + s, ..., up to j in the order of subsets(xs, spec) (the
# step may be negative)
rule subsets(xs_, spec_, ns_) := {
  let idx = range(ns[1], ns[2], ns[3])
  if length(idx) == 0 then [] else {
    let all = subsets(xs, spec, [min(idx), max(idx)])
    map(k => all[k - min(idx) + 1], idx)
  }
} when is_list(xs) and is_list(ns) and length(ns) == 3 and every(ns, k => is_integer(k)) and ns[3] != 0

# subset_map(f, xs, all): f applied to the whole list, element by element in place
fn subset_map(f, xs, a) when is_list(xs) and a === all = subset_map(f, xs, range(length(xs)))

# numerical_sort(a) of an association: its entries in the order of numerical_sort of their values
fn numerical_sort(a) when is_assoc(a) = {
  let ks = keys(a)
  let real = x => (x === inf or x === -inf or is_real(n(x))) === true
  let num = select(ks, k => real(a[k]))
  let other = select(ks, k => not real(a[k]))
  let ord = join(sort(num, (p, q) => n(a[p]) <= n(a[q])), sort_by(other, k => a[k]))
  assoc(map(k => k -> a[k], ord))
}

# replace(e, rules, all): the rules applied at every level from the leaves up (each part replaced at most once;
# heads are not replaced)
rule replace(e_, r_, a_) := replace(e, r, [0, inf]) when a === all


# key_complement([a1, a2, ...], same_test: f): the entries of a1 whose keys k have f(k, k2) false for every key k2 of
# a2, ...
fn key_complement(xs; same_test = automatic) when is_list(xs) and length(xs) > 0 = {
  let ms = map(x => if is_assoc(x) then x else if is_list(x) then assoc(x) else assoc([x]), xs)
  let others = flatten(map(keys, rest(ms)), 1)
  let keyeq = if same_test === automatic then ((p, q) => p === q) else same_test
  select_keys(ms[1], k => not any_true(others, k2 => keyeq(k, k2) === true))
}

# --- alphabetic order: more languages and the options of alphabetic_sort ---

# more tailorings of the Latin script (CLDR): Danish and Norwegian æ, ø, å after z; Turkish ç, ğ, ı, ö, ş, ü after
# c, g, h, o, s, u (the dotless ı is the lower case of I, the dotted i that of İ); Dutch ĳ as y
fn alpha_tailoring(lang) =
  if lang == "Danish" or lang == "Norwegian" then assoc("æ" -> ["z", 1], "ø" -> ["z", 2], "å" -> ["z", 3])
  else if lang == "Turkish" then assoc("ç" -> ["c", 1], "ğ" -> ["g", 1], "ı" -> ["h", 1], "ö" -> ["o", 1], "ş" -> ["s", 1], "ü" -> ["u", 1])
  else if lang == "Dutch" then assoc("ĳ" -> ["y", 0])
  else if lang == "English" then assoc()
  else r37_tailoring(lang)
# the language's lower case: Turkish I is ı and İ is i
fn alpha_lower(s, lang) = if lang == "Turkish" then to_lower(string_replace(s, ["I" -> "ı", "İ" -> "i"])) else to_lower(s)
# the levels of a string: [letters (first-level weights), accents (the code points of its lower case), case (1 for
# an upper-case letter)]; punctuation and spaces left out when they are ignored
fn alpha_levels(s0, lang, tab, nopunct) = {
  let s = if nopunct then from_characters(select(characters(s0), c => string_match_q(c, letter_character | digit_character))) else s0
  let low = alpha_lower(s, lang)
  [r37_weights(low, tab), to_character_code(low), map(c => boole(alpha_lower(c, lang) != c), characters(s))]
}
# the strings of xs in the order of the language: letters first, then accents unless ignored, then case unless
# ignored (lower case first, upper case first for "UpperFirst" and in Danish), then the order of xs
fn alpha_sort(xs, lang, o) = {
  let get = (k, d) => { let hit = select(apply(list, o), r => r[1] === k); if length(hit) == 0 then d else last(hit)[2] }
  let tab = alpha_tailoring(lang)
  let co = get(case_ordering, automatic)
  let upper = co === "UpperFirst" or (co === automatic and lang == "Danish")
  let icase = get(ignore_case, false) === true
  let iacc = get(ignore_diacritics, false) === true
  let lv = map(s => alpha_levels(s, lang, tab, get(ignore_punctuation, false) === true), xs)
  let cmp = (i, j) => {
    var o1 = lexicographic_order(lv[i][1], lv[j][1])
    if o1 == 0 and not iacc then o1 = lexicographic_order(lv[i][2], lv[j][2])
    if o1 == 0 and not icase then o1 = if upper then lexicographic_order(lv[j][3], lv[i][3]) else lexicographic_order(lv[i][3], lv[j][3])
    if o1 == 0 then o1 = order(i, j)
    o1 == 1
  }
  map(i => xs[i], sort(range(length(xs)), cmp))
}
fn alpha_sort_ok(xs, lang) = is_list(xs) and every(xs, x => is_string(x)) and is_string(lang) and alpha_tailoring(lang) !== null
rule alphabetic_sort(xs_, lang_, o_) := alpha_sort(xs, lang, o) when not is_atom(o) and head(o) === options and alpha_sort_ok(xs, lang)
rule alphabetic_sort(xs_, o_) := alpha_sort(xs, "English", o) when not is_atom(o) and head(o) === options and alpha_sort_ok(xs, "English")
rule alphabetic_sort(xs_, lang_) := alpha_sort(xs, lang, options()) when alpha_sort_ok(xs, lang) and (lang == "Danish" or lang == "Norwegian" or lang == "Turkish" or lang == "Dutch")

# --- statistics of distributions of a symbolic order ---

# the cumulants and moments of order r (a symbol) of the Poisson, normal and chi-square distributions (Johnson, Kemp
# and Kotz, Univariate discrete distributions, 4.3; Johnson, Kotz and Balakrishnan, Continuous univariate
# distributions 1, 13.3 and 18.3): Poisson moments are the Touchard polynomials bell_b(r, mu), its cumulants all mu;
# normal cumulants mu, sigma^2 and then 0, its central moments (r - 1)!! sigma^r for even r; chi-square cumulants
# 2^(r - 1) (r - 1)! nu
fn dist_order_symbolic(r) = not is_numeric(r) and not is_list(r)
# whether d is a call of the head name
fn call_head_is(d, name) = not is_atom(d) and head(d) === name
fn cumulant(d, r) when call_head_is(d, poisson_distribution) and length(d) == 1 and dist_order_symbolic(r) = piecewise([[d[1], r >= 1]], 0)
rule moment(d_, r_) := piecewise([[bell_b(r, d[1]), r >= 0]], 0) when call_head_is(d, poisson_distribution) and length(d) == 1 and dist_order_symbolic(r)
fn cumulant(d, r) when call_head_is(d, normal_distribution) and length(d) == 2 and dist_order_symbolic(r) = piecewise([[d[1], r == 1], [d[2]^2, r == 2]], 0)
rule central_moment(d_, r_) := piecewise([[factorial2(r - 1)*d[2]^r, mod(r, 2) == 0 and r >= 0]], 0) when call_head_is(d, normal_distribution) and length(d) == 2 and dist_order_symbolic(r)
fn cumulant(d, r) when call_head_is(d, chi_square_distribution) and length(d) == 1 and dist_order_symbolic(r) = piecewise([[d[1]*factorial(r - 1)*2^(r - 1), r >= 1]], 0)

# covariance(d, i, j): the entry i, j of the covariance matrix of a multivariate distribution
rule covariance(d_, i_: Int, j_: Int) := covariance(d)[i, j] when not is_list(d) and is_matrix(covariance(d))

# --- dimensions of regions given by symbols and curves ---

# a regular polygon of n sides (n a symbol) is a plane region; a filled curve is a plane region, a joined curve a
# curve, each in the space of its points
rule region_dimension(r_) := 2 when call_head_is(r, regular_polygon) and length(r) >= 1 and not is_numeric(last(r))
rule region_embedding_dimension(r_) := 2 when call_head_is(r, regular_polygon) and length(r) >= 1 and not is_numeric(last(r))
fn region_curve_points(c) = if is_list(c) then flatten(map(region_curve_points, c), 1) else if not is_atom(c) and length(c) >= 1 and is_list(c[1]) then (if is_list(c[1][1]) then c[1] else [c[1]]) else []
fn region_curve_dim(r) = { let ps = region_curve_points(r[1]); if length(ps) > 0 and every(ps, p => is_list(p) and length(p) == length(ps[1])) then length(ps[1]) else null }
rule region_dimension(r_) := 2 when call_head_is(r, filled_curve) and length(r) == 1 and region_curve_dim(r) === 2
rule region_embedding_dimension(r_) := 2 when call_head_is(r, filled_curve) and length(r) == 1 and region_curve_dim(r) === 2
rule region_dimension(r_) := 1 when call_head_is(r, joined_curve) and length(r) == 1 and region_curve_dim(r) !== null
rule region_embedding_dimension(r_) := region_curve_dim(r) when call_head_is(r, joined_curve) and length(r) == 1 and region_curve_dim(r) !== null

# --- Bezier curves: length and bounds ---

# the point at t of the Bezier curve of the control points ps (Bernstein form); a B-spline of n <= 4 control points
# with the default degree (n - 1) is that Bezier curve
fn bezier_point(ps, t) = { let k = length(ps) - 1; total(map(i => binomial(k, i)*t^i*(1 - t)^(k - i)*ps[i + 1], range(0, k))) }
fn bezier_ok(c) = not is_atom(c) and (head(c) === bezier_curve or (head(c) === b_spline_curve and length(c[1]) <= 4)) and length(c) == 1 and is_list(c[1]) and length(c[1]) >= 2 and every(c[1], p => is_list(p) and every(p, x => is_numeric(x) and not is_inexact(x)))
# the length: the integral of the speed over [0, 1]
rule arc_length(c_) := {
  integrate(sqrt(total(map(x => x^2, d(bezier_point(c[1], bezier_param), bezier_param)))), [bezier_param, 0, 1])
} when bezier_ok(c)
# the bounds: each coordinate at the ends and at the real zeros of its derivative inside (0, 1)
fn bezier_range(p, t) = {
  let sol = solve(d(p, t) == 0, t)
  let crit = if is_list(sol) then select(map(s => s[1][2], sol), s => is_real(n(s)) and 0 < s and s < 1) else []
  let vs = map(s => simplify(rewrite(p, t -> s)), join([0, 1], crit))
  [min(vs), max(vs)]
}
rule region_bounds(c_) := map(p => bezier_range(p, bezier_param), bezier_point(c[1], bezier_param)) when bezier_ok(c)

# --- matrices modulo a prime, options of the matrix tests, polynomial extrapolation ---

# the value of the option k in an options(...) term (d when absent)
fn opt_value(o, k, d) = { let hit = select(apply(list, o), r => r[1] === k); if length(hit) == 0 then d else last(hit)[2] }
fn opt_only(o, ks) = every(apply(list, o), r => member(ks, r[1]))
fn is_options(o) = not is_atom(o) and head(o) === options

# the reduced row echelon form of an integer matrix over the integers mod the prime p (Gauss-Jordan elimination, the
# pivots made 1 with their inverses mod p)
fn modp_rref(m, p) = {
  var a = mod(m, p)
  let rows = length(a)
  let cols = length(a[1])
  var r = 1
  for c in range(cols) {
    if r <= rows then {
      let piv = select(range(r, rows), i => a[i][c] != 0)
      if length(piv) > 0 then {
        let k = piv[1]
        let tmp = a[r]
        a[r] = a[k]
        a[k] = tmp
        a[r] = mod(power_mod(a[r][c], -1, p)*a[r], p)
        for i in range(rows) {
          if i != r and a[i][c] != 0 then a[i] = mod(a[i] - a[i][c]*a[r], p) else null
        }
        r = r + 1
      } else null
    } else null
  }
  a
}
fn modp_ok(m, o) = is_options(o) and opt_only(o, [modulus]) and is_matrix(m) and every(flatten(m), x => is_integer(x)) and is_integer(opt_value(o, modulus, 0)) and opt_value(o, modulus, 0) > 1 and is_prime(opt_value(o, modulus, 0))
# the null space mod p: for each column without a pivot, the vector with 1 there and minus the column's entries of
# the pivot rows at the pivot columns
fn modp_null_space(m, p) = {
  let a = modp_rref(m, p)
  let cols = length(a[1])
  let piv = map(row => first(select(range(cols), j => row[j] != 0)), select(a, row => not every(row, x => x == 0)))
  let free = select(range(cols), j => not member(piv, j))
  map(f => map(j => if j == f then 1 else if member(piv, j) then mod(-a[first(first(position(piv, j)))][f], p) else 0, range(cols)), free)
}
rule row_reduce(m_, o_) := modp_rref(m, opt_value(o, modulus, 0)) when modp_ok(m, o)
rule null_space(m_, o_) := modp_null_space(m, opt_value(o, modulus, 0)) when modp_ok(m, o)
rule range_space(m_, o_) := select(modp_rref(transpose(m), opt_value(o, modulus, 0)), row => not every(row, x => x == 0)) when modp_ok(m, o)
# the tolerance of an exact matrix changes nothing
rule range_space(m_, o_) := range_space(m) when is_options(o) and opt_only(o, [tolerance]) and is_matrix(m) and not is_inexact(total(flatten(m)))

# a numeric matrix (exact constants such as pi included) is tested through its leading minors
rule positive_definite_matrix_q(m_) := {
  let h = (m + conjugate(transpose(m)))/2
  every(range(length(m)), k => { let d = det(map(r => take(r, k), take(h, k))); n(re(d)) > 0 })
} when is_matrix(m) and length(m) == length(m[1]) and every(flatten(m), x => is_numeric(x)) and not b29_numeric_matrix(m)

# the tests of matrix shapes with tolerance: t (an entry counts as 0 when its size is at most t; a symbolic entry
# when possible_zero_q says it is 0) and symmetric_matrix_q with same_test: f (f(m_ij, m_ji) for every pair)
fn tol_zero(x, t) = if is_numeric(x) then n(abs(x)) <= t else possible_zero_q(x)
rule symmetric_matrix_q(m_, o_) := (is_matrix(m) and length(m) == length(m[1]) and every(range(length(m)), i => every(range(length(m)), j => tol_zero(m[i][j] - m[j][i], opt_value(o, tolerance, 0))))) when is_options(o) and opt_only(o, [tolerance]) and is_real(n(opt_value(o, tolerance, 0)))
rule symmetric_matrix_q(m_, o_) := (is_matrix(m) and length(m) == length(m[1]) and every(range(length(m)), i => every(range(i), j => opt_value(o, same_test, null)(m[i][j], m[j][i]) === true))) when is_options(o) and opt_only(o, [same_test])
rule diagonal_matrix_q(m_, o_) := (is_matrix(m) and every(range(length(m)), i => every(range(length(m[1])), j => i == j or tol_zero(m[i][j], opt_value(o, tolerance, 0))))) when is_options(o) and opt_only(o, [tolerance]) and is_real(n(opt_value(o, tolerance, 0)))
rule upper_triangular_matrix_q(m_, o_) := (is_matrix(m) and every(range(length(m)), i => every(range(length(m[1])), j => j >= i or tol_zero(m[i][j], opt_value(o, tolerance, 0))))) when is_options(o) and opt_only(o, [tolerance]) and is_real(n(opt_value(o, tolerance, 0)))
rule lower_triangular_matrix_q(m_, o_) := (is_matrix(m) and every(range(length(m)), i => every(range(length(m[1])), j => j <= i or tol_zero(m[i][j], opt_value(o, tolerance, 0))))) when is_options(o) and opt_only(o, [tolerance]) and is_real(n(opt_value(o, tolerance, 0)))

# array_pad(v, m, "Extrapolated", interpolation_order: k) of a list of numbers: the polynomial of degree k (all the
# points for inf, at most length(v) - 1) through the last k + 1 entries continued (Newton's forward differences
# with the k-th difference held), and likewise before the first entries
fn extrap_right(v, k, cnt) = {
  let q = min(k, length(v) - 1)
  var d = drop(v, length(v) - (q + 1))
  var out = []
  for s in range(cnt) {
    # the differences of the window, the highest held: the next value
    var diffs = [d]
    for j in range(q) {
      let prev = last(diffs)
      diffs = append(diffs, map(i => prev[i + 1] - prev[i], range(length(prev) - 1)))
    }
    let nxt = total(map(j => last(diffs[j]), range(q + 1)))
    out = append(out, nxt)
    d = append(rest(d), nxt)
  }
  out
}
rule array_pad(v_, m_, mode_, o_) := {
  let k = opt_value(o, interpolation_order, 1)
  let kk = if k === inf then length(v) - 1 else k
  let lr = if is_integer(m) then [m, m] else if length(m) == 1 then [m[1], m[1]] else m
  join(reverse(extrap_right(reverse(v), kk, lr[1])), v, extrap_right(v, kk, lr[2]))
} when mode === "Extrapolated" and is_options(o) and opt_only(o, [interpolation_order]) and is_list(v) and length(v) >= 1 and every(v, x => is_numeric(x)) and (is_integer(m) or (is_list(m) and every(m, is_integer) and length(m) <= 2)) and every(flatten([m]), c => c >= 0) and (opt_value(o, interpolation_order, 1) === inf or (is_integer(opt_value(o, interpolation_order, 1)) and opt_value(o, interpolation_order, 1) >= 0))

# total(e, allowed_heads: inherited): the sum of the arguments of e whatever its head
rule total(e_, o_) := apply(add, apply(list, e)) when is_options(o) and opt_only(o, [allowed_heads]) and opt_value(o, allowed_heads, null) === inherited and not is_atom(e) and not is_options(e)

# nearest(ms, m) of matrices (or arrays) of one shape: the one nearest to m in the Frobenius norm of the difference
# (unevaluated when two are equally near)
rule nearest(ms_, m_) := {
  let ds = map(x => n(norm(flatten(x - m))), ms)
  let lo = min(ds)
  map(i => ms[i], select(range(length(ms)), i => ds[i] == lo))
} when is_list(ms) and length(ms) >= 1 and is_matrix(m) and every(ms, x => is_matrix(x) and dimensions(x) == dimensions(m)) and every(flatten(ms), x => is_numeric(x)) and every(flatten(m), x => is_numeric(x)) and length(select(map(x => n(norm(flatten(x - m))), ms), d => d == min(map(x => n(norm(flatten(x - m))), ms)))) == 1
# identity_matrix(n, working_precision: p): at precision p (machine numbers at machine precision)
rule identity_matrix(k_, working_precision: p_) := (if n(p) > 15.9 and n(p) < 16 then n(identity_matrix(k)) else n(identity_matrix(k), p)) when is_integer(k) and k > 0 and is_real(n(p))

# array_flatten(a, r) for r >= 3: the array of rank r made from the block array a of rank r whose entries are arrays
# of rank r (a number stands for a constant block of the shape of the other blocks): entry (i1, ..., ir) of block
# (b1, ..., br) goes to (b1 - 1) s1 + i1, ... (s the block shape), by a transpose that interleaves the levels and a
# reshape
fn af_blocks(a, r) = if r == 0 then [a] else flatten(map(x => af_blocks(x, r - 1), a), 1)
fn af_fill(a, r, s) = if r == 0 then (if is_list(a) then a else constant_array(a, s)) else map(x => af_fill(x, r - 1, s), a)
rule array_flatten(a_, r_) := {
  let outer = take(dimensions(a), r)
  let blocks = select(af_blocks(a, r), b => is_list(b))
  let s = dimensions(blocks[1])
  let full = af_fill(a, r, s)
  let t = transpose(full, join(map(k => 2*k - 1, range(r)), map(k => 2*k, range(r))))
  array_reshape(flatten(t), map(k => outer[k]*s[k], range(r)))
} when is_integer(r) and r >= 3 and is_list(a) and length(dimensions(a)) >= r and length(select(af_blocks(a, r), b => is_list(b))) >= 1 and every(select(af_blocks(a, r), b => is_list(b)), b => length(dimensions(b)) == r and dimensions(b) == dimensions(first(select(af_blocks(a, r), c => is_list(c)))))

# a Boolean function given by its number k (of n variables) is satisfiable when k is not 0, a tautology when k is
# 2^(2^n) - 1 (the truth table of all ones)
rule is_satisfiable(f_, v_) := f[1] != 0 when call_head_is(f, boolean_function) and length(f) == 2 and is_integer(f[1]) and is_integer(f[2]) and f[1] >= 0 and v === []
rule is_tautology(f_, v_) := f[1] == 2^(2^f[2]) - 1 when call_head_is(f, boolean_function) and length(f) == 2 and is_integer(f[1]) and is_integer(f[2]) and f[1] >= 0 and f[2] >= 0 and f[2] <= 20 and v === []

# the arc of a circle of radius r between the angles t0 and t1: r (t1 - t0) long (a full turn at most)
rule arc_length(c_) := c[2]*(c[3][2] - c[3][1]) when call_head_is(c, circle) and length(c) == 3 and is_list(c[3]) and length(c[3]) == 2 and not is_list(c[2]) and is_real(n(c[2])) and n(c[2]) >= 0 and is_real(n(c[3][2] - c[3][1])) and n(c[3][2] - c[3][1]) >= 0 and n(c[3][2] - c[3][1]) <= n(2*pi)
