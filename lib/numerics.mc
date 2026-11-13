#! mathcas 2
# The numerics library: numerical and series functions written in MathCAS itself, evaluated
# after the prelude and the language library when the engine starts. Everything is in the bare namespace; helpers
# are prefixed `nl_` and are not part of the interface. Every public function is listed in
# `mc_quad::LIBRARY_FUNCTIONS` and has a page docs/functions/<name>.mcn. Methods and literature are in
# crates/mc-quad/NOTES.md ("Numerics library").

# --- sequences: ratios, linear_recurrence ---

# ratios(xs): the successive ratios xs[i + 1]/xs[i] (of the rows of an array: elementwise); ratios(xs, k): applied k
# times; ratios(xs, [k1, k2, ...]): k1 times at level 1, k2 times at level 2, ...
rule ratios(xs_) := map(i => xs[i + 1]/xs[i], range(length(xs) - 1)) when is_list(xs)
rule ratios(xs_, k_) := nest(ratios, xs, k) when is_list(xs) and is_integer(k) and k >= 0
rule ratios(xs_, ks_) := nl_ratios_levels(xs, ks) when is_list(xs) and is_list(ks) and every(ks, k => is_integer(k) and k >= 0)
fn nl_ratios_levels(xs, ks) =
  if length(ks) == 0 then xs
  else {
    let ys = nest(ratios, xs, ks[1])
    if length(ks) == 1 then ys else map(y => nl_ratios_levels(y, drop(ks, 1)), ys)
  }

# the term of a linear recurrence from the d previous terms: sum of ker[j] times the term j back (dot products for
# matrix coefficients)
fn nl_lr_step(ker, prev) = {
  let d = length(ker)
  let m = length(prev)
  total(map(j => if is_matrix(ker[j]) then dot(ker[j], prev[m - j + 1]) else ker[j]*prev[m - j + 1], range(d)))
}

# terms 1, ..., hi of the recurrence with kernel ker from init (the last length(ker) entries of init seed it)
fn nl_lr_forward(ker, init, hi) = {
  var out = take(init, min(hi, length(init)))
  while length(out) < hi { out = append(out, nl_lr_step(ker, out)) }
  out
}

# linear_recurrence(ker, init, n): the first n terms of a[k] = ker[1] a[k - 1] + ... + ker[d] a[k - d] from init;
# (ker, init, [m]): term m; (ker, init, [m1, m2]): terms m1 to m2 (m1 <= 0 extends the sequence backwards from
# the first initial value, index 1)
rule linear_recurrence(ker_, init_, len_) := nl_lr_forward(ker, init, len) when is_list(ker) and is_list(init) and is_integer(len) and len >= 0 and length(init) >= length(ker)
rule linear_recurrence(ker_, init_, spec_) := {
  let lo = spec[1]
  let hi = if length(spec) == 1 then spec[1] else spec[2]
  if lo >= 1 then drop(nl_lr_forward(ker, init, hi), lo - 1)
  else join(nl_lr_before(ker, init, lo), nl_lr_forward(ker, init, max(hi, 0)))
} when is_list(ker) and is_list(init) and is_list(spec) and length(spec) >= 1 and length(spec) <= 2 and every(spec, is_integer) and length(init) >= length(ker)

# the terms lo, ..., 0 (lo <= 0)
fn nl_lr_before(ker, init, lo) = {
  let d = length(ker)
  var seq = take(init, d)
  var out = []
  var k = 0
  while k >= lo {
    let rest = total(map(j => ker[j]*seq[d - j], range(d - 1)))
    let t =(seq[d] - rest)/ker[d]
    out = prepend(out, t)
    seq = take(prepend(seq, t), d)
    k = k - 1
  }
  out
}

# --- thresholding ---

# threshold(data), (data, delta), (data, spec): the elements of data with small values set to zero, at every level.
# spec: delta or ["Hard", delta] (x when |x| > delta), ["Soft", delta] (shrunk by delta), ["Firm", delta, r, p],
# ["PiecewiseGarrote", delta], ["SmoothGarrote", delta, k], ["Hyperbola", delta], ["LargestValues", k] (the k
# largest in absolute value kept). The default delta is 10^-10.
rule threshold(data_) := threshold(data, 1/10^10)
rule threshold(data_, spec_) := {
  if is_list(spec) and length(spec) == 2 and spec[1] === "LargestValues" then nl_th_largest(data, spec[2])
  else {
    let s = if is_list(spec) then spec else ["Hard", spec]
    nl_th_map(x => nl_th_one(x, s), data)
  }
} when not match_q(spec, _ -> _)
fn nl_th_map(f, d) = if is_list(d) then map(e => nl_th_map(f, e), d) else f(d)
fn nl_th_zero(x) = if not is_inexact(x) then 0 else if is_real(x) === true then 0*abs(x) else complex(0*abs(re(x)), 0*abs(im_part(x)))
fn nl_th_one(x, s) = {
  let kind = s[1]
  let dl = s[2]
  let ax = abs(x)
  if not is_number(x) then x
  else if kind === "Hard" then (if ax > dl then x else nl_th_zero(x))
  else if kind === "Soft" then (if ax > dl then x - dl*x/ax else nl_th_zero(x))
  else if kind === "PiecewiseGarrote" then (if ax > dl then x - dl^2/x else nl_th_zero(x))
  else if kind === "Hyperbola" then (if ax > dl then x/ax*sqrt(x*conjugate(x) - dl^2) else nl_th_zero(x))
  else if kind === "SmoothGarrote" then { let k = if length(s) > 2 then s[3] else 1; x^(2*k + 1)/(dl^(2*k) + x^(2*k)) }
  else if kind === "Firm" then {
    let r = if length(s) > 2 then s[3] else 1
    let p = if length(s) > 3 then s[4] else 1/2
    let lo = dl - p*r*dl
    let hi = dl + (1 - p)*r*dl
    if ax <= lo then nl_th_zero(x)
    else if ax <= hi then x/ax*hi*(ax - lo)/(r*dl)
    else x
  }
  else x
}
fn nl_th_largest(data, k) = {
  let flat = flatten([data])
  let mags = reverse(sort(map(abs, select(flat, is_number))))
  let cut = if k >= length(mags) then -1 else if k <= 0 then inf else mags[k]
  nl_th_map(x => if is_number(x) and abs(x) >= cut then x else if is_number(x) then nl_th_zero(x) else x, data)
}

# --- cantor_staircase ---

# cantor_staircase(x): the Cantor function at x (0 for x <= 0, 1 for x >= 1), exact at rational x (the ternary
# expansion is eventually periodic: the periodic part sums as a geometric series), numeric at inexact x
rule cantor_staircase(x_) := nl_cantor_exact(x) when is_rational(x) or is_integer(x)
rule cantor_staircase(x_) := nl_cantor_inexact(x) when is_real(x) and is_inexact(x)
rule cantor_staircase(xs_) := map(cantor_staircase, xs) when is_list(xs)
fn nl_cantor_exact(x) =
  if x <= 0 then 0 else if x >= 1 then 1
  else {
    # digits of the expansion, with the residues seen so far: a repeat closes the period
    var y = x
    var seen = []
    var bits = []
    var res = null
    while res === null {
      let at = position(seen, y)
      if length(at) > 0 then {
        let i = at[1][1]
        let pre = take(bits, i - 1)
        let per = drop(bits, i - 1)
        let p = length(per)
        let a = total(map(j => pre[j]/2^j, range(length(pre))))
        let b = total(map(j => per[j]/2^(i - 1 + j), range(p)))
        res = a + b/(1 - 1/2^p)
      } else {
        seen = append(seen, y)
        let d = floor(3*y)
        if d == 1 then res = total(map(j => bits[j]/2^j, range(length(bits)))) + 1/2^(length(bits) + 1)
        else { bits = append(bits, d/2); y = 3*y - d }
      }
    }
    res
  }
fn nl_cantor_inexact(x) =
  if x <= 0 then 0*x else if x >= 1 then 1 + 0*x
  else {
    let steps = ceiling(precision(x)*log(10)/log(3)) + 2
    var y = x
    var acc = 0
    var k = 1
    var done = false
    while not done and k <= steps {
      let d = floor(3*y)
      if d == 1 then { acc = acc + 1/2^k; done = true }
      else { acc = acc + (d/2)/2^k; y = 3*y - d; k = k + 1 }
    }
    acc + 0*x
  }

# --- dirac_comb ---

# dirac_comb(x1, ..., xk): the Dirac comb, a sum of delta functions at the integers (the product over the
# arguments): 0 at a number that is not an integer, unevaluated at integers and symbols; even in each argument
rule dirac_comb(xs__) := 0 when some(xs, x => is_number(x) and is_real(x) and not is_integer(x) and (is_inexact(x) or not is_integer(x)))
rule dirac_comb(xs__) := apply(dirac_comb, map(nl_comb_sign, xs)) when some(xs, x => nl_comb_sign(x) !== x)
rule dirac_comb(l_) := map(dirac_comb, l) when is_list(l)
fn nl_comb_sign(x) = if is_number(x) and is_real(x) and x < 0 then -x else if not is_atom(x) and head(x) === mul and is_number(x[1]) and x[1] < 0 then -x else x

# --- mandelbrot_set_member_q ---

# mandelbrot_set_member_q(c): whether the orbit of 0 under z^2 + c stays bounded (|z| <= 2) for max_iterations
# steps; the main cardioid and the period-2 disk are recognised directly
fn mandelbrot_set_member_q(c; max_iterations = 1000, working_precision = machine_precision) =
  if is_list(c) then map(z => mandelbrot_set_member_q(z, max_iterations: max_iterations, working_precision: working_precision), c)
  else if not is_numeric(c) then failure(argument, mandelbrot_set_member_q, 1)
  else {
    let w = if working_precision === machine_precision then n(c) else n(c, working_precision)
    let q = (re(w) - 1/4)^2 + im_part(w)^2
    if q*(q + re(w) - 1/4) < im_part(w)^2/4 or (re(w) + 1)^2 + im_part(w)^2 < 1/16 then true
    else {
      var z = 0*w
      var k = 0
      var out = false
      while not out and k < max_iterations { z = z^2 + w; out = (abs(z) > 2) === true; k = k + 1 }
      not out
    }
  }

# --- interpolating_polynomial ---

# interpolating_polynomial(data, x): the polynomial of least degree through the data, in Newton form. data: values
# [f1, f2, ...] at 1, 2, ...; pairs [[x1, f1], ...]; [x1, f1, d1, ...] entries with derivatives (Hermite
# interpolation; automatic marks an unknown value: then the polynomial comes from a linear solve, expanded)
rule interpolating_polynomial(data_, x_) := nl_ip(nl_ip_points(data), x) when is_list(data) and length(data) > 0 and not is_list(x)
fn nl_ip_points(data) =
  if every(data, is_list) then map(d => if length(d) == 2 and is_list(d[2]) then join([d[1]], d[2]) else d, data)
  else map(i => if is_list(data[i]) then join([i], data[i]) else [i, data[i]], range(length(data)))
fn nl_ip(pts, x) =
  if some(pts, p => some(drop(p, 1), v => v === automatic)) then nl_ip_solve(pts, x)
  else {
    # confluent divided differences: node z_i repeated once per given value
    var zs = []
    var fs = []
    for p in pts {
      let m = length(p) - 1
      for j in range(m) { zs = append(zs, p[1]); fs = append(fs, p) }
    }
    let len = length(zs)
    var col = map(p => p[2], fs)
    var coef = [col[1]]
    for k in range(len - 1) {
      col = map(i => if zs[i + k] === zs[i] then fs[i][k + 2]/factorial(k) else (col[i + 1] - col[i])/(zs[i + k] - zs[i]), range(len - k))
      coef = append(coef, col[1])
    }
    var acc = coef[len]
    var k = len - 1
    while k >= 1 { acc = coef[k] + (x - zs[k])*acc; k = k - 1 }
    acc
  }
fn nl_ip_solve(pts, x) = {
  var eqs = []
  var count = 0
  for p in pts { count = count + length(select(drop(p, 1), v => v !== automatic)) }
  let cs = map(i => nl_ip_c(i), range(count))
  let poly = total(map(i => cs[i]*x^(i - 1), range(count)))
  for p in pts {
    for j in range(length(p) - 1) {
      if p[j + 1] !== automatic then eqs = append(eqs, rewrite(if j == 1 then poly else d(poly, [x, j - 1]), x -> p[1]) == p[j + 1])
    }
  }
  let sol = solve(eqs, cs)
  if length(sol) == 0 then failure(no_solution, interpolating_polynomial) else expand(rewrite(poly, sol[1]))
}

# --- fourier, inverse_fourier ---

# the discrete Fourier transform of a vector of numbers with sign s (+1 forward): sum over r of u[r] w^(r s), w =
# exp(2 pi i s/n), by the mixed-radix Cooley-Tukey recursion on the smallest prime factor of n (Cooley and Tukey
# 1965); the twiddle factors are computed at the working precision
fn nl_dft(u, s, prec) = {
  let len = length(u)
  # long machine vectors: the fast transform (it stays unevaluated for entries that are not machine numbers)
  let fast = if prec === machine_precision and len > 128 and is_integer(s) then nl_fft_machine(u, s) else false
  if len == 1 then u
  else if is_list(fast) then fast
  else {
    let p = factor_integer(len)[1][1]
    let m = len/p
    let wt = if prec === machine_precision then map(k => n(exp(2*pi*im*s*k/len)), range(0, len - 1)) else map(k => n(exp(2*pi*im*s*k/len), prec), range(0, len - 1))
    let w = k => wt[k + 1]
    if p == len then map(k => total(map(r => u[r + 1]*w(mod(r*k, len)), range(0, len - 1))), range(0, len - 1))
    else {
      let subs = map(q => nl_dft(take(drop(u, q), [1, len - q, p]), s, prec), range(0, p - 1))
      map(k => total(map(q => w(mod(q*k, len))*subs[q + 1][mod(k, m) + 1], range(0, p - 1))), range(0, len - 1))
    }
  }
}

# the transform of an array along every level, scaled by 1/n^((1 - a)/2) per level for parameters [a, b]
fn nl_fourier(u, a, b, sgn) = {
  let big = select(flatten([u]), x => is_inexact(x) and not is_machine_number(x) and x != 0)
  let prec = if length(big) == 0 then machine_precision else floor(min(map(precision, big)))
  let v = if prec === machine_precision then n(u) else u
  let w = nl_fourier_levels(v, a, b, sgn, prec)
  # complex results, real ones when the data is conjugate-even (u[k] = conj(u[n + 2 - k]) at every level)
  if nl_conj_even(v) then nl_map_all(x => re(x), w) else nl_map_all(x => complex(re(x), im_part(x)), w)
}
# deeper levels first; the first level then combines whole subarrays (the arithmetic is elementwise)
# a matrix is transformed along its rows as a whole (the entries of the transform are rows), then along the columns
# the same way: the arithmetic of each step is on whole lists, not on single numbers
fn nl_fourier_levels(u, a, b, sgn, prec) =
  if prec === machine_precision and is_integer(b) and is_list(u[1]) and not is_list(u[1][1]) and length(u[1]) > 1 and length(u) > 1 then nl_fourier_matrix(u, a, b, sgn)
  else if is_list(u[1]) and not is_list(u[1][1]) and length(u[1]) > 1 and length(u) > 1 then transpose(nl_fourier_vec(transpose(nl_fourier_vec(u, a, b, sgn, prec)), a, b, sgn, prec))
  else nl_fourier_levels1(u, a, b, sgn, prec)
# a machine matrix: F_rows . U . F_cols with the DFT matrices (native matrix products beat the list arithmetic of the
# mixed-radix recursion, which is O(len^2) on whole lists at prime lengths)
fn nl_dft_matrix(len, s) = {
  let wt = map(k => n(exp(2*pi*im*s*k/len)), range(0, len - 1))
  table(table(wt[mod(j*k, len) + 1], k, range(0, len - 1)), j, range(0, len - 1))
}
fn nl_fourier_matrix(u, a, b, sgn) = {
  let nr = length(u)
  let nc = length(u[1])
  let sr = n(1/nr^((1 - sgn*a)/2))
  let sc = n(1/nc^((1 - sgn*a)/2))
  (sr*sc)*dot(dot(nl_dft_matrix(nr, sgn*b), u), nl_dft_matrix(nc, sgn*b))
}
fn nl_fourier_levels1(u, a, b, sgn, prec) =
  nl_fourier_vec(if is_list(u[1]) then map(r => nl_fourier_levels(r, a, b, sgn, prec), u) else u, a, b, sgn, prec)
fn nl_fourier_vec(u, a, b, sgn, prec) = {
  let len = length(u)
  let scale = if prec === machine_precision then n(1/len^((1 - sgn*a)/2)) else n(1/len^((1 - sgn*a)/2), prec)
  if is_integer(b) then map(x => scale*x, nl_dft(u, sgn*b, prec))
  else map(k => scale*total(map(r => u[r + 1]*n(exp(2*pi*im*sgn*b*r*k/len)), range(0, len - 1))), range(0, len - 1))
}

# fourier(list), fourier(array): the discrete Fourier transform 1/n^((1 - a)/2) sum u[r] exp(2 pi i b (r - 1)(s -
# 1)/n) with fourier_parameters [a, b] (default [0, 1]), along every level of an array; numeric, at the precision
# of the data
fn fourier(u; fourier_parameters = [0, 1]) when is_list(u) and length(u) > 0 and every(flatten([u]), is_numeric) =
  nl_fourier(u, fourier_parameters[1], fourier_parameters[2], 1)

# inverse_fourier(list): the inverse of fourier with the same fourier_parameters
fn inverse_fourier(u; fourier_parameters = [0, 1]) when is_list(u) and length(u) > 0 and every(flatten([u]), is_numeric) =
  nl_fourier(u, fourier_parameters[1], fourier_parameters[2], -1)

# --- find_peaks ---

# the peaks of a list of values: [position, value] for each run of equal values above both neighbours (a run at an
# end of the list needs only its inner neighbour), the position the middle of the run; with sharpness above s (the
# largest negated second difference in the run, the list padded by reflection) and value above t
fn nl_peaks(v, s, t) = {
  let len = length(v)
  let at = i => if i < 1 then v[1] else if i > len then v[len] else v[i]
  var out = []
  var i = 1
  while i <= len {
    var j = i
    while j < len and v[j + 1] == v[i] { j = j + 1 }
    let left_ok = i == 1 or v[i - 1] < v[i]
    let right_ok = j == len or v[j + 1] < v[i]
    let inner = (i > 1 and v[i - 1] < v[i]) or (j < len and v[j + 1] < v[i])
    if left_ok and right_ok and inner then {
      let sharp = max(map(k => 2*v[k] - at(k - 1) - at(k + 1), range(i, j)))
      if sharp > s and v[i] > t then out = append(out, [(i + j)/2, v[i]])
    }
    i = j + 1
  }
  out
}

# the values blurred by a sampled Gaussian of width sigma (radius ceiling(3 sigma)), the ends padded by reflection
fn nl_gauss_blur(v, sigma) = {
  let r = ceiling(3*sigma)
  let len = length(v)
  let ws0 = map(k => n(exp(-k^2/(2*sigma^2))), range(-r, r))
  let ws = ws0/total(ws0)
  let at = i => if i < 1 then v[min(len, 1 - i)] else if i > len then v[max(1, 2*len + 1 - i)] else v[i]
  map(i => total(map(k => ws[k + r + 1]*at(i + k), range(-r, r))), range(len))
}

# the second derivatives at the knots of the cubic spline through (x, y) with zero slope at both ends (a clamped
# spline; the tridiagonal system by the Thomas algorithm, Burden and Faires, Numerical Analysis, 3.5)
fn nl_clamped_spline(x, y) = {
  let m = length(x)
  let h = map(i => x[i + 1] - x[i], range(m - 1))
  var lo = constant_array(0, m)
  var di = constant_array(0, m)
  var up = constant_array(0, m)
  var rh = constant_array(0, m)
  di[1] = 2*h[1]
  up[1] = h[1]
  rh[1] = 6*(y[2] - y[1])/h[1]
  di[m] = 2*h[m - 1]
  lo[m] = h[m - 1]
  rh[m] = -6*(y[m] - y[m - 1])/h[m - 1]
  var i = 2
  while i < m {
    lo[i] = h[i - 1]
    di[i] = 2*(h[i - 1] + h[i])
    up[i] = h[i]
    rh[i] = 6*((y[i + 1] - y[i])/h[i] - (y[i] - y[i - 1])/h[i - 1])
    i = i + 1
  }
  i = 2
  while i <= m {
    let w = lo[i]/di[i - 1]
    di[i] = di[i] - w*up[i - 1]
    rh[i] = rh[i] - w*rh[i - 1]
    i = i + 1
  }
  var sol = constant_array(0, m)
  sol[m] = rh[m]/di[m]
  i = m - 1
  while i >= 1 {
    sol[i] = (rh[i] - up[i]*sol[i + 1])/di[i]
    i = i - 1
  }
  sol
}

# the maximum of that spline (second derivatives mm) over the segments within one knot of the position p: [position, value]
fn nl_spline_top(x, y, mm, p) = {
  let m = length(x)
  var best = [x[1], -inf]
  let first = max(1, floor(p - 1))
  let last = min(m - 1, ceiling(p + 1) - 1)
  var j = first
  while j <= last {
    let h = x[j + 1] - x[j]
    let m0 = mm[j]
    let m1 = mm[j + 1]
    let val = u => m0*(h - u)^3/(6*h) + m1*u^3/(6*h) + (y[j]/h - m0*h/6)*(h - u) + (y[j + 1]/h - m1*h/6)*u
    let qa = (m1 - m0)/(2*h)
    let qb = m0
    let qc = (y[j + 1] - y[j])/h - h*(m1 - m0)/6 - m0*h/2
    let roots = if qa == 0 then (if qb == 0 then [] else [-qc/qb]) else {
      let dd = qb^2 - 4*qa*qc
      if dd < 0 then [] else [(-qb + sqrt(dd))/(2*qa), (-qb - sqrt(dd))/(2*qa)]
    }
    let cands = join([0, h], select(roots, u => u > 0 and u < h))
    for u in cands { if val(u) > best[2] then best = [x[j] + u, val(u)] }
    j = j + 1
  }
  best
}

# find_peaks(data), (data, sigma), (data, sigma, s), (data, sigma, s, t): the peaks [position, value] of a list of
# real values: local maxima (plateaus at their middle) of the data blurred by a Gaussian of width sigma (default
# 0), with sharpness above s and value above t (defaults 0 and -inf); with sigma > 0 the peaks of the blurred data
# are reported at the nearest peak of the data itself
# (no `when` clause: with defaults and options it shifts the arguments, see NOTES.md)
fn find_peaks(data, sigma = automatic, s = automatic, t = automatic; interpolation_order = automatic) = if not is_list(data) or length(data) == 0 then failure(argument, find_peaks, 1)
else if every(data, q => match_q(q, quantity(_, _))) then {
  # quantities (of any compatible units): their magnitudes in the unit of the first, the peaks with that unit
  let u = data[1][2]
  let mags = map(q => quantity_magnitude(q, u), data)
  let tq = if match_q(t, quantity(_, _)) then quantity_magnitude(t, u) else t
  let r = find_peaks(mags, sigma, s, tq, interpolation_order: interpolation_order)
  if is_list(r) then map(p => [p[1], quantity(p[2], u)], r) else r
} else {
  let sg = if sigma === automatic then 0 else sigma
  let sh = if s === automatic then 0 else s
  let tv = if t === automatic then -inf else if is_list(t) then t[1] else t
  let pairs = is_list(data[1])
  let xs = if pairs then map(p => p[1], data) else range(length(data))
  let v = if pairs then map(p => p[2], data) else data
  let s0 = if sg == 0 then sh else -inf
  let raw = nl_peaks(v, s0, tv)
  let found = if sg == 0 then raw else {
    let blurred = nl_peaks(nl_gauss_blur(v, sg), sh, -inf)
    unique(flat_map(b => {
      let near = sort_by(raw, p => abs(p[1] - b[1]))
      if length(near) > 0 and abs(near[1][1] - b[1]) <= 3*sg then [near[1]] else []
    }, blurred))
  }
  let placed = if pairs then map(p => [if is_integer(p[1]) then xs[p[1]] else (xs[floor(p[1])] + xs[ceiling(p[1])])/2, p[2]], found) else found
  # interpolation_order >= 2: each peak moved to the maximum of the clamped cubic spline through the data near it
  if is_integer(interpolation_order) and interpolation_order >= 2 and length(v) >= 3 then {
    let xn = n(if pairs then xs else range(length(v)))
    let yn = n(v)
    let mm = nl_clamped_spline(xn, yn)
    map(p => nl_spline_top(xn, yn, mm, p[1]), found)
  } else placed
}

# --- d_fixed_points, r_fixed_points ---

# the function symbol of a variable given as y or y(t)
fn nl_fp_sym(v) = if is_atom(v) then v else head(v)
fn nl_fp_list(x) = if is_list(x) then x else [x]
# the highest derivative of y in e (0 when only y(t) occurs)
fn nl_fp_dorder(e, y, t) = max(join([0], map(c => c[1], cases(e, derivative(k_)(y)(t) :> [k], inf))))
# the constant solutions of the equations after the substitution sub, without those depending on the independent
# variable; each as the list of the unknowns' values
fn nl_fp_solve(eqs, ys, ind) = {
  let sols = solve(eqs, ys)
  if is_failure(sols) or not is_list(sols) then sols
  else sort(select(map(s => rewrite(ys, s), sols), v => free_of(v, ind) and every(v, w => free_of(w, ys))))
}

# d_fixed_points(eqs, vars, t): the fixed points (equilibria) of the differential equations: the constant
# solutions, as the state [y, y', ..., y^(k-1)] of each unknown of order k (all derivatives 0), one list per point
fn d_fixed_points(eqs, vars, t) = {
  let es = nl_fp_list(eqs)
  let ys = map(nl_fp_sym, nl_fp_list(vars))
  let orders = map(y => max(1, nl_fp_dorder(es, y, t)), ys)
  let flat = rewrite(es, join(map(y => derivative(_)(y)(t) -> 0, ys), map(y => y(t) -> y, ys)))
  let pts = nl_fp_solve(flat, ys, t)
  if not is_list(pts) then pts
  else map(p => flatten(map(i => join([p[i]], constant_array(0, orders[i] - 1)), range(length(ys)))), pts)
}

# the shifts k of the occurrences y(n + k) in e
fn nl_fp_shifts(e, y, ind) = map(c => c[1], cases(e, y(a_) :> [expand(a - ind)], inf))

# r_fixed_points(eqs, vars, n): the fixed points of the difference equations: the constant solutions, as the
# state [y, ..., y] (one entry per step of the order of each unknown), one list per point
fn r_fixed_points(eqs, vars, ind) = {
  let es = nl_fp_list(eqs)
  let ys = map(nl_fp_sym, nl_fp_list(vars))
  let orders = map(y => { let ks = nl_fp_shifts(es, y, ind); if length(ks) == 0 then 1 else max(1, max(ks) - min(ks)) }, ys)
  let flat = rewrite(es, map(y => y(_) -> y, ys))
  let pts = nl_fp_solve(flat, ys, ind)
  if not is_list(pts) then pts
  else map(p => flatten(map(i => constant_array(p[i], orders[i]), range(length(ys)))), pts)
}

# x mapped over the elements of a nested list
fn nl_map_all(f, u) = if is_list(u) then map(x => nl_map_all(f, x), u) else f(u)
# the array with every level reversed cyclically after its first element (index k -> n + 2 - k)
fn nl_flip(u) = if is_list(u) then { let r = map(nl_flip, u); join([r[1]], reverse(drop(r, 1))) } else u
# whether the (numeric) array equals the conjugate of its flip, to rounding
fn nl_conj_even(u) = {
  let f = flatten([u])
  let g = flatten([nl_flip(u)])
  let s = max(join([1], abs(f)))
  # list arithmetic (native, elementwise) rather than a test per entry
  max(abs(f - conjugate(g))) <= 10^-14*s
}

# --- parametric_nd_solve, parametric_nd_solve_value ---

# parametric_nd_solve_value(eqs, expr, range, params, opts...): another name for parametric_n_dsolve_value
rule parametric_nd_solve_value(args__) := apply(parametric_n_dsolve_value, args)

# parametric_nd_solve(eqs, y, range, params, opts...): [y -> f] with f = parametric_n_dsolve_value(eqs, y, ...), a
# parametric function of the parameters; [y -> f, z -> g] for a list of unknowns
rule parametric_nd_solve(eqs_, ys_, rest__) := {
  let f = y => y -> apply(parametric_n_dsolve_value, join([eqs, y], rest))
  if is_list(ys) then map(f, ys) else [f(ys)]
}

# --- Fourier coefficients and series ---

# Kinds: exp (exponential coefficients over the period [-pi/|b|, pi/|b|]), cos and sin (the cosine and sine
# coefficients of the even and odd extension of f from the half period [0, pi/|b|], the convention of
# fourier_cos_coefficient and fourier_sin_coefficient), cosf and sinf (the trigonometric coefficients over the
# whole period, those of fourier_trig_series).

# the integral of e over the box of the variables vs: [-pi/|b|, pi/|b|] in each (whole), or [0, pi/|b|] (half)
fn nl_fc_int(e, vs, b, half) = {
  var r = e
  for v in vs { r = integrate(r, [v, if half then 0 else -pi/abs(b), pi/abs(b)]) }
  r
}
# the integer values of the symbol k where the denominator of c vanishes (positive ones only when pos): the
# indices whose coefficient comes separately
fn nl_fc_poles(c, k, pos, sym_ok) = {
  let den = denominator(together(c))
  if free_of(den, k) then []
  else {
    let s = solve(den == 0, k)
    if not is_list(s) then [] else unique(select(map(r => rewrite(k, r), s), p => (is_integer(p) and (not pos or p > 0)) or (sym_ok and not is_number(p) and free_of(p, k))))
  }
}
# the coefficient for one symbol index k (the other indices at their values): the generic formula simplified for
# integer k, its poles as separate cases; a single pole with a zero generic value is a discrete_delta
fn nl_fc_symbolic(gen, at, k, pos, asm) = {
  let c = simplify(gen, assumptions: asm and element(k, integers) and (if pos then k > 0 else true))
  let poles = nl_fc_poles(gen, k, pos, asm !== true)
  if length(poles) == 0 then c
  else {
    let alts = map(p => [simplify(at(p)), k == p], poles)
    if c === 0 and length(poles) == 1 then alts[1][1]*discrete_delta(k - poles[1])
    else piecewise(alts, c)
  }
}
# the coefficient of f at the indices ns (numbers or symbols) for variables vs, of one of the kinds above
fn nl_fc(f, vs, ns, a, b, kind, asm) = {
  let d = length(vs)
  let half = kind === cos or kind === sin
  let scale = if kind === exp then (abs(b)/(2*pi))^(d*(1 + a)/2) else if half then (2*abs(b)/pi)^d else (abs(b)/pi)^d
  let trig = if kind === cos or kind === cosf then cos else sin
  let body = ms => if kind === exp then f*exp(-im*total(map(i => b*ms[i]*vs[i], range(d)))) else f*product(map(i => trig(b*ms[i]*vs[i]), range(d)))
  let value = ms => simplify(scale*nl_fc_int(body(ms), vs, b, half), assumptions: asm)
  let syms = select(ns, k => not is_number(k))
  if length(syms) == 0 then value(ns)
  else if length(syms) == 1 then {
    let k = syms[1]
    nl_fc_symbolic(scale*nl_fc_int(body(ns), vs, b, half), p => value(rewrite(ns, k -> p)), k, kind !== exp, asm)
  }
  else simplify(scale*nl_fc_int(body(ns), vs, b, half), assumptions: asm and fold((p, q) => p and q, map(k => element(k, integers), syms)))
}
fn nl_fc_vars(v) = if is_list(v) then v else [v]

# fourier_coefficient(f, t, n): the n-th coefficient of the exponential Fourier series of f, periodic with period
# 2 pi/|b|: (|b|/(2 pi))^((1 + a)/2) times the integral of f exp(-i b n t) over [-pi/|b|, pi/|b|], fourier_parameters
# [a, b] (default [1, 1]); a symbolic n gives the generic coefficient with its exceptions; several variables
# with lists of variables and indices
fn fourier_coefficient(f, t, k; fourier_parameters = [1, 1], assumptions = true) =
  nl_fc(f, nl_fc_vars(t), nl_fc_vars(k), fourier_parameters[1], fourier_parameters[2], exp, assumptions)

# fourier_cos_coefficient(f, t, n): the n-th coefficient of the cosine series, 2|b|/pi times the integral of f
# cos(b n t) over [0, pi/|b|] (n >= 0)
fn fourier_cos_coefficient(f, t, k; fourier_parameters = [1, 1], assumptions = true) =
  nl_fc(f, nl_fc_vars(t), nl_fc_vars(k), fourier_parameters[1], fourier_parameters[2], cos, assumptions)

# fourier_sin_coefficient(f, t, n): the n-th coefficient of the sine series, 2|b|/pi times the integral of f
# sin(b n t) over [0, pi/|b|] (n >= 1)
fn fourier_sin_coefficient(f, t, k; fourier_parameters = [1, 1], assumptions = true) =
  nl_fc(f, nl_fc_vars(t), nl_fc_vars(k), fourier_parameters[1], fourier_parameters[2], sin, assumptions)

# the index tuples of d dimensions with entries in xs
fn nl_fc_tuples(xs, d) = tuples(xs, d)
# the orders: one per variable
fn nl_fc_orders(k, d) = if is_list(k) then k else constant_array(k, d)

# fourier_series(f, t, k): the exponential Fourier series of f to order k: (|b|/(2 pi))^((1 - a)/2) times the sum
# of c_n exp(i b n t) for n from -k to k (per variable for lists of variables and orders)
fn fourier_series(f, t, k; fourier_parameters = [1, 1]) = {
  let a = fourier_parameters[1]
  let b = fourier_parameters[2]
  let vs = nl_fc_vars(t)
  let d = length(vs)
  let ks = nl_fc_orders(k, d)
  let idx = tuples(map(m => range(-m, m), ks))
  (abs(b)/(2*pi))^(d*(1 - a)/2)*total(map(ms => nl_fc(f, vs, ms, a, b, exp, true)*exp(im*b*total(map(i => ms[i]*vs[i], range(d)))), idx))
}

# fourier_cos_series(f, t, k): a_0/2 + the sum of a_n cos(b n t) for n from 1 to k, the coefficients of
# fourier_cos_coefficient (a product of cosines per variable for lists, each zero index halving)
fn fourier_cos_series(f, t, k; fourier_parameters = [1, 1]) = {
  let b = fourier_parameters[2]
  let vs = nl_fc_vars(t)
  let d = length(vs)
  let idx = tuples(map(m => range(0, m), nl_fc_orders(k, d)))
  total(map(ms => nl_fc(f, vs, ms, 1, b, cos, true)*product(map(i => if ms[i] == 0 then 1/2 else cos(b*ms[i]*vs[i]), range(d))), idx))
}

# fourier_sin_series(f, t, k): the sum of b_n sin(b n t) for n from 1 to k, the coefficients of
# fourier_sin_coefficient (products of sines per variable for lists)
fn fourier_sin_series(f, t, k; fourier_parameters = [1, 1]) = {
  let b = fourier_parameters[2]
  let vs = nl_fc_vars(t)
  let d = length(vs)
  let idx = tuples(map(m => range(1, m), nl_fc_orders(k, d)))
  total(map(ms => nl_fc(f, vs, ms, 1, b, sin, true)*product(map(i => sin(b*ms[i]*vs[i]), range(d))), idx))
}

# fourier_trig_series(f, t, k): a_0/2 + the sum of a_n cos(b n t) + b_n sin(b n t) for n from 1 to k, the
# coefficients over the whole period [-pi/|b|, pi/|b|]
fn fourier_trig_series(f, t, k: Int; fourier_parameters = [1, 1]) = {
  let b = fourier_parameters[2]
  nl_fc(f, [t], [0], 1, b, cosf, true)/2 + total(map(m => nl_fc(f, [t], [m], 1, b, cosf, true)*cos(b*m*t) + nl_fc(f, [t], [m], 1, b, sinf, true)*sin(b*m*t), range(k)))
}


# --- unilateral_convolve, mellin_convolve ---

# [constant factor, rest] of e with respect to the variables vs
fn nl_cv_split(e, vs) =
  if not is_atom(e) and head(e) === mul then {
    let fs = apply(list, e)
    [product(select(fs, x => every(vs, v => free_of(x, v)))), product(select(fs, x => some(vs, v => not free_of(x, v))))]
  } else if every(vs, v => free_of(e, v)) then [e, 1] else [1, e]

# unilateral_convolve(f, g, tau, t): the convolution of f and g on [0, inf): the integral of f(tau) g(t - tau)
# over 0 <= tau <= t (t > 0); lists of variables for several dimensions. Constant factors come out; a delta at the
# origin picks the value of the other function; it stays unevaluated when the integral cannot be done
fn nl_uc(f, g, tau, t, asm) = {
  let ts = nl_fc_vars(tau)
  let xs = nl_fc_vars(t)
  let sf = nl_cv_split(f, ts)
  let sg = nl_cv_split(g, ts)
  let c = sf[1]*sg[1]
  let back = e => rewrite(e, map(i => ts[i] -> xs[i], range(length(ts))))
  let shift = e => rewrite(e, map(i => ts[i] -> xs[i] - ts[i], range(length(ts))))
  let delta = e => e === apply(dirac_delta, ts)
  if c !== 1 then (if c === 0 then 0 else c*apply(unilateral_convolve, join([sf[2], sg[2], tau, t], if asm === true then [] else [assumptions -> asm])))
  else if delta(f) then back(g)
  else if delta(g) then back(f)
  else {
    var r = f*shift(g)
    var ok = true
    for i in range(length(ts)) {
      if ok then {
        r = integrate(r, [ts[i], 0, xs[i]], assumptions: asm and xs[i] > 0)
        ok = free_of(r, integrate)
      }
    }
    if ok then simplify(r, assumptions: asm and fold((p, q) => p and q, map(x => x > 0, xs))) else null
  }
}

# mellin_convolve(f, g, x, y): the Mellin convolution, the integral of f(x) g(y/x)/x over 0 < x < inf (y > 0);
# lists of variables for several dimensions; constant factors come out; unevaluated when the integral cannot be
# done
fn nl_mc(f, g, x, y, asm, gc) = {
  let ts = nl_fc_vars(x)
  let ys = nl_fc_vars(y)
  let sf = nl_cv_split(f, ts)
  let sg = nl_cv_split(g, ts)
  let c = sf[1]*sg[1]
  if c !== 1 then (if c === 0 then 0 else c*apply(mellin_convolve, join(join([sf[2], sg[2], x, y], if asm === true then [] else [assumptions -> asm]), if gc then [generate_conditions -> true] else [])))
  else {
    let num = every(ys, v => is_number(v) and is_inexact(v))
    var r = f*rewrite(g, map(i => ts[i] -> ys[i]/ts[i], range(length(ts))))/product(ts)
    var ok = true
    for i in range(length(ts)) {
      if ok then {
        r = if num then n_integrate(r, [ts[i], 0, inf]) else integrate(r, [ts[i], 0, inf], assumptions: asm and ys[i] > 0)
        ok = is_number(r) or free_of(r, integrate)
      }
    }
    if ok then (if num then r else simplify(r, assumptions: asm and fold((p, q) => p and q, map(v => v > 0, ys))))
    else if length(ts) == 1 and not num then (if asm === true then calc_mconv_mb(f, g, ts[1], ys[1], gc) else assuming(asm, calc_mconv_mb(f, g, ts[1], ys[1], gc)))
    else null
  }
}

# whether generate_conditions: true is among trailing options
fn nl_gc(opts) = {
  let rs = flatten(map(o => if not is_atom(o) and head(o) === options then apply(list, o) else [o], opts))
  some(rs, r => match_q(r, generate_conditions -> true))
}

# the assumptions among trailing options (true without them)
fn nl_asm(opts) = {
  let rs = flatten(map(o => if not is_atom(o) and head(o) === options then apply(list, o) else [o], opts))
  let hit = select(rs, r => match_q(r, assumptions -> _))
  if length(hit) == 0 then true else hit[1][2]
}
rule unilateral_convolve(f_, g_, tau_, t_, opts___) := nl_uc(f, g, tau, t, nl_asm(opts)) when nl_uc(f, g, tau, t, nl_asm(opts)) !== null
rule mellin_convolve(f_, g_, x_, y_, opts___) := nl_mc(f, g, x, y, nl_asm(opts), nl_gc(opts)) when nl_mc(f, g, x, y, nl_asm(opts), nl_gc(opts)) !== null

# --- list_interpolation, function_interpolation ---

# the value at x (in index units, 1 = first node) of the local Lagrange polynomial of order k through the k + 1
# nodes nearest x, along the first level of vals (whose elements may be arrays: the arithmetic is elementwise)
fn nl_li_1d(vals, coords, k, x) = {
  let len = length(vals)
  let ord = floor(min(k, len - 1))
  # the interval [coords[i], coords[i + 1]] holding x (clamped to the ends)
  var i = 1
  while i < len - 1 and x > coords[i + 1] { i = i + 1 }
  let lo = max(1, min(len - ord, i - floor((ord - 1)/2)))
  let idx = range(lo, lo + ord)
  if ord == 0 then vals[if len == 1 or x < (coords[i] + coords[min(len, i + 1)])/2 then i else min(len, i + 1)]
  else total(map(j => vals[j]*product(map(m => if m == j then 1 else (x - coords[m])/(coords[j] - coords[m]), idx)), idx))
}
# the value at the point xs of the tensor-product interpolation of the array vals on the coordinate lists grids
fn nl_li_eval(vals, grids, ks, xs) when every(xs, is_number) = {
  let v = nl_li_1d(vals, grids[1], ks[1], xs[1])
  if length(xs) == 1 then v else nl_li_eval(v, drop(grids, 1), drop(ks, 1), drop(xs, 1))
}
# the coordinate list of one dimension of length len from a domain spec: [lo, hi] (equally spaced) or the
# coordinates themselves
fn nl_li_grid(spec, len) =
  if spec === automatic then range(len)
  else if length(spec) == len and len != 2 then spec
  else if length(spec) == 2 and len != 2 then map(j => spec[1] + (spec[2] - spec[1])*(j - 1)/(len - 1), range(len))
  else spec
# the function of d arguments that evaluates the interpolation
fn nl_li_fun(vals, grids, ks) = {
  let d = length(grids)
  if d == 1 then (x => nl_li_eval(vals, grids, ks, [x]))
  else if d == 2 then ((x, y) => nl_li_eval(vals, grids, ks, [x, y]))
  else if d == 3 then ((x, y, z) => nl_li_eval(vals, grids, ks, [x, y, z]))
  else ((x, y, z, w) => nl_li_eval(vals, grids, ks, [x, y, z, w]))
}

# list_interpolation(array), (array, domain): a function interpolating the array of values on a grid: the
# coordinates 1, 2, ... in each dimension, or domain [[xmin, xmax], ...] (equally spaced) or [[x1, x2, ...], ...];
# piecewise Lagrange polynomials of interpolation_order (default 3, an integer or one per dimension) through the
# nearest nodes. Pairs [[x], v] in one dimension give the coordinates with the values; a value given as [f, f', ...]
# uses f only.
fn list_interpolation(data, dom = automatic; interpolation_order = 3, method = automatic, periodic_interpolation = false) = {
  let pts = is_list(data) and length(data) > 0 and is_list(data[1]) and length(data[1]) == 2 and is_list(data[1][1])
  let vals0 = if pts then map(p => p[2], data) else data
  let dims = if pts then [length(data)] else if dom !== automatic then take(dimensions(vals0), length(dom)) else nl_li_dims(vals0)
  let vals = if pts then vals0 else nl_li_strip(vals0, length(dims))
  let grids = if pts then [map(p => p[1][1], data)] else map(i => nl_li_grid(if dom === automatic then automatic else dom[i], dims[i]), range(length(dims)))
  let ks = if is_list(interpolation_order) then interpolation_order else constant_array(interpolation_order, length(dims))
  nl_li_fun(vals, grids, ks)
}
# the dimensions of the grid of values: as deep as the array is rectangular with number (or symbol) entries or
# [value, derivatives] entries at the bottom
fn nl_li_dims(v) = {
  let ds = dimensions(v)
  # entries [f, f'] or [f, [fx, fy]] at the bottom are values with derivatives
  if length(ds) > 1 and (not every(flatten([v]), is_number) or (length(ds) > 1 and not is_list(v[1][1]) and length(ds) == 2 and false)) then nl_li_value_dims(v) else ds
}
fn nl_li_value_dims(v) = if is_list(v) and is_list(v[1]) and is_list(v[1][1]) and not is_number(v[1][1][1]) then join([length(v)], nl_li_value_dims(v[1])) else if is_list(v) and is_list(v[1]) and is_list(v[1][2]) then [length(v)] else dimensions(v)
fn nl_li_strip(v, d) = if d == 0 then (if is_list(v) then v[1] else v) else map(x => nl_li_strip(x, d - 1), v)

# function_interpolation(f, [x, a, b], ...): a function interpolating the expression f sampled on a grid (65 points
# per dimension in one dimension, 33 in two, 17 in more) over the ranges
fn function_interpolation(f, ranges...; interpolation_order = 3, max_recursion = automatic, accuracy_goal = automatic, precision_goal = automatic) = {
  let d = length(ranges)
  let m = if d == 1 then 65 else if d == 2 then 33 else 17
  let grids = map(r => map(j => n(r[2] + (r[3] - r[2])*(j - 1)/(m - 1)), range(m)), ranges)
  let vars = map(r => r[1], ranges)
  let sample = (lvl, sub) => if lvl > d then n(rewrite(f, sub)) else map(c => sample(lvl + 1, append(sub, vars[lvl] -> c)), grids[lvl])
  nl_li_fun(nl_fi_sample(f, vars, grids, 1, []), grids, constant_array(interpolation_order, d))
}
fn nl_fi_sample(f, vars, grids, lvl, sub) =
  if lvl > length(vars) then n(rewrite(f, sub)) else map(c => nl_fi_sample(f, vars, grids, lvl + 1, append(sub, vars[lvl] -> c)), grids[lvl])

# --- mean_around ---

# the sample covariance matrix of a list of vectors (divisor n - 1)
fn nl_cov(vs) = {
  let len = length(vs)
  let m = total(vs)/len
  let ds = map(v => v - m, vs)
  total(map(dv => outer(mul, dv, dv), ds))/(len - 1)
}

# mean_around(xs): the mean of the numbers xs with its standard error, around(mean, s/sqrt(n)) (s the sample
# standard deviation); of a list of around(x, d) values the weighted mean by 1/d^2 with error 1/sqrt(sum 1/d^2);
# of a list of vectors vector_around(mean, covariance/n)
fn mean_around(xs) when is_list(xs) and length(xs) > 1 =
  if every(xs, x => not is_atom(x) and head(x) === around) then {
    let ws = map(x => 1/x[2]^2, xs)
    around(total(map(i => ws[i]*xs[i][1], range(length(xs))))/total(ws), 1/sqrt(total(ws)))
  }
  else if every(xs, is_list) then vector_around(total(xs)/length(xs), nl_cov(xs)/length(xs))
  else {
    let len = length(xs)
    let m = total(xs)/len
    around(n(m), n(sqrt(total(map(x => (x - m)^2, xs))/(len - 1)/len)))
  }

# --- random_polygon ---

# the convex hull of points in the plane, counterclockwise, by the monotone chain (Andrew 1979)
fn nl_hull(ps) = {
  let s = sort(ps)
  let cross = (o, a, b) => (a[1] - o[1])*(b[2] - o[2]) - (a[2] - o[2])*(b[1] - o[1])
  var lower = []
  for p in s {
    while length(lower) >= 2 and cross(lower[-2], lower[-1], p) <= 0 { lower = drop(lower, -1) }
    lower = append(lower, p)
  }
  var upper = []
  for p in reverse(s) {
    while length(upper) >= 2 and cross(upper[-2], upper[-1], p) <= 0 { upper = drop(upper, -1) }
    upper = append(upper, p)
  }
  join(drop(lower, -1), drop(upper, -1))
}
# points sorted by angle about their centroid: a star-shaped, hence simple, polygon through them
fn nl_by_angle(ps) = {
  let c = total(ps)/length(ps)
  sort_by(ps, p => atan(p[1] - c[1], p[2] - c[2]))
}
# a random convex polygon with k vertices in the unit square (Valtr 1995): random vectors from the sorted
# coordinates, ordered by angle and chained, then shifted into the square
fn nl_convex(k) = {
  let parts = xs => {
    let s = sort(xs)
    let lo = s[1]
    let hi = s[-1]
    var a = lo
    var b = lo
    var out = []
    for x in drop(drop(s, 1), -1) {
      if random_real() < 1/2 then { out = append(out, x - a); a = x } else { out = append(out, b - x); b = x }
    }
    append(append(out, hi - a), b - hi)
  }
  let dx = parts(map(i => random_real(), range(k)))
  let dy = random_sample(parts(map(i => random_real(), range(k))))
  let vs = sort_by(map(i => [dx[i], dy[i]], range(k)), v => atan(v[1], v[2]))
  let pts = fold_list((p, v) => p + v, [0, 0], drop(vs, -1))
  let lo = [min(map(p => p[1], pts)), min(map(p => p[2], pts))]
  let w = max(max(map(p => p[1], pts)) - lo[1], max(map(p => p[2], pts)) - lo[2])
  map(p => (p - lo)/w, pts)
}
# a random star-shaped polygon with k vertices about the centre of the unit square: sorted random angles, random radii
fn nl_star(k) = {
  let th = sort(map(i => 2*pi*random_real(), range(k)))
  map(t => [1/2, 1/2] + random_real()/2*[cos(t), sin(t)], th)
}
fn nl_rand_points(dist, k) = if dist === automatic then map(i => [random_real(), random_real()], range(k)) else { let v = random_variate(dist, k); if is_list(v) and length(v) > 0 and is_list(v[1]) then v else partition(random_variate(dist, 2*k), 2) }
# the vertices of one random polygon of a type with k vertices
fn nl_rpoly(kind, dist, k) =
  if kind === "Convex" then nl_convex(k)
  else if kind === "StarShaped" then nl_star(k)
  else if kind === "ConvexHull" then nl_hull(nl_rand_points(dist, k))
  else nl_by_angle(nl_rand_points(dist, k))
# the planar vertices placed in a random plane through the unit cube of dimension d
fn nl_embed(ps, d) =
  if d == 2 then ps
  else {
    let u0 = map(i => random_real() - 1/2, range(d))
    let u = u0/sqrt(total(u0^2))
    let w0 = map(i => random_real() - 1/2, range(d))
    let w1 = w0 - total(w0*u)*u
    let w = w1/sqrt(total(w1^2))
    let c = constant_array(1/2, d)
    map(p => c + (p[1] - 1/2)*u/2 + (p[2] - 1/2)*w/2, ps)
  }
# the spec [type, dist, k] of a random_polygon argument
fn nl_rp_spec(s) =
  if is_integer(s) then ["Simple", automatic, s]
  else if is_string(s) then [s, automatic, automatic]
  else if is_list(s) and length(s) == 2 then [s[1], automatic, s[2]]
  else if is_list(s) and length(s) == 3 then s
  else s
fn nl_rp_one(spec, d) = {
  let s = nl_rp_spec(spec)
  let k = if s[3] === automatic then 3 + floor(8*random_real()) else s[3]
  polygon(n(nl_embed(nl_rpoly(s[1], s[2], k), d)))
}
fn nl_rp_many(spec, d, shape) = if length(shape) == 0 then nl_rp_one(spec, d) else map(i => nl_rp_many(spec, d, drop(shape, 1)), range(shape[1]))

# random_polygon(), random_polygon(k), random_polygon(spec), random_polygon(spec, m), random_polygon(spec, [m1, ...]):
# a random polygon in the unit square, or m of them, or an array; spec k (vertices), a type "Simple" (the default),
# "Convex", "StarShaped" or "ConvexHull", [type, k], [type, dist, k] (vertices from a distribution, for
# "ConvexHull" and "Simple"); d -> spec for a planar polygon in a random plane of dimension d
fn random_polygon(spec = automatic, shape = automatic) = {
  let d = if match_q(spec, _ -> _) then spec[1] else 2
  let s0 = if match_q(spec, _ -> _) then spec[2] else spec
  let s = if s0 === automatic then "Simple" else s0
  if shape === automatic then nl_rp_one(s, d) else nl_rp_many(s, d, if is_list(shape) then shape else [shape])
}

# --- piecewise functions of complex and symbolic arguments (round 36) ---

# round, floor, ceiling, integer_part and fractional_part of an exact complex number whose parts are not rational
# (1/2 + i sqrt(15)/3): part by part, as for Gaussian rationals
rule round(z_) := round(re(z)) + im*round(im_part(z)) when is_numeric(z) and not is_number(z) and not is_real(n(z)) and is_number(n(z))
rule floor(z_) := floor(re(z)) + im*floor(im_part(z)) when is_numeric(z) and not is_number(z) and not is_real(n(z)) and is_number(n(z))
rule ceiling(z_) := ceiling(re(z)) + im*ceiling(im_part(z)) when is_numeric(z) and not is_number(z) and not is_real(n(z)) and is_number(n(z))
rule integer_part(z_) := integer_part(re(z)) + im*integer_part(im_part(z)) when is_numeric(z) and not is_number(z) and not is_real(n(z)) and is_number(n(z))
rule fractional_part(z_) := fractional_part(re(z)) + im*fractional_part(im_part(z)) when is_numeric(z) and not is_number(z) and not is_real(n(z)) and is_number(n(z))

# the derivative of unit_triangle away from its kinks (-1, 0, 1), as for unit_box (0 away from its jumps):
# -sign(x) inside (-1, 1), 0 outside; for several arguments (a product) the factor of the differentiated one
rule derivative(1)(unit_triangle)(x_) := -sign(x)*unit_box(x/2)
rule derivative(1, 0)(unit_triangle)(x_, y_) := -sign(x)*unit_box(x/2)*unit_triangle(y)
rule derivative(0, 1)(unit_triangle)(x_, y_) := -sign(y)*unit_box(y/2)*unit_triangle(x)

# (the area of the intersection of two disks, the lens, is computed by the mc-geom natives: lens_area)

# integer_exponent(0, b): every power of b divides 0
rule integer_exponent(0) := inf
rule integer_exponent(0, b_) := inf when is_integer(b) and b > 1
# log(b, x): the logarithm of x to the base b, as log_base(b, x)
rule log(b_, x_) := log_base(b, x)

# --- numbers with uncertainty ---

# cube_root(x) of a real number with uncertainty (around, centered_interval): the real cube root surd(x, 3) carried
# through the uncertainty
fn cube_root(x) when not is_atom(x) and (head(x) === around or head(x) === centered_interval) and length(x) == 2 and is_real(n(x[1])) = surd(x, 3)
