#! mathcas 2
# The wavelet library: discrete, stationary, packet and continuous wavelet transforms, their inverses, the data
# objects that hold the coefficients, and their plots. Helpers are prefixed `wv_`. Methods and literature:
# crates/mc-wavelets/NOTES.md.

# ---------------------------------------------------------------- wavelet families

# daubechies_wavelet(n): the orthonormal wavelet of Daubechies with n vanishing moments and 2 n filter taps;
# daubechies_wavelet() is daubechies_wavelet(2), daubechies_wavelet(1) the Haar wavelet. haar_wavelet() takes the
# filter [1, 1]/sqrt(2). The low-pass filter h of daubechies_wavelet(n) comes from the spectral factorisation
# |m0|^2 = cos(w/2)^(2 n) P(sin(w/2)^2), P(y) = sum_(k < n) binomial(n - 1 + k, k) y^k (Daubechies, Ten lectures on
# wavelets, 6.1): every root y of P gives the root z of z^2 - (2 - 4 y) z + 1 outside the unit circle, and
# h(z) = c (1 + z)^n prod (z - z_k) with sum h = sqrt(2) (the extremal-phase choice, the largest taps first).
rule daubechies_wavelet() := daubechies_wavelet(2)

fn wv_daub(k) = {
  let p = total(map(j => binomial(k - 1 + j, j)*wv_y^j, range(0, k - 1)))
  let ys = if k == 1 then [] else map(r => r[1][2], n_solve(p == 0, wv_y, working_precision: 40))
  let zs = map(y => {
    let b = (2 - 4*y)/2
    let s = sqrt(b^2 - 1)
    if abs(b + s) > 1 then b + s else b - s
  }, ys)
  let q = expand((1 + wv_z)^k*product(map(z => wv_z - z, zs)))
  let cs = coefficient_list(q, wv_z)
  n(re(cs*sqrt(2)/total(cs)))
}

# wv_lowpass(w): the orthonormal low-pass filter of the wavelet w (machine numbers), or null
fn wv_lowpass(w) =
  if w === automatic or w === haar_wavelet() then n([1, 1]/sqrt(2))
  else if not is_atom(w) and head(w) === daubechies_wavelet and length(w) == 1 and is_integer(w[1]) and w[1] >= 1 and w[1] <= 20 then wv_daub(w[1])
  else null
fn wv_name(w) = if w === automatic then haar_wavelet() else w
# the high-pass filter g_k = (-1)^k h_(L-1-k) (the quadrature mirror filter)
fn wv_highpass(h) = { let l = length(h); map(k => (-1)^k*h[l - k], range(0, l - 1)) }

# ---------------------------------------------------------------- one level of the periodized transform

# wv_extend(x, m, pad): the list x extended on the right to length m: "Periodic" repeats it, a number pads with
# it, "Fixed" repeats the last entry, "Reflected" mirrors without repeating the end
fn wv_extend(x, m, pad) = {
  let k = length(x)
  if m <= k then x
  else if pad === "Periodic" then map(i => x[mod(i - 1, k) + 1], range(m))
  else if pad === "Fixed" then join(x, constant_array(x[k], m - k))
  else if pad === "Reflected" then map(p => { let q = mod(p - 1, 2*k - 2); x[if q < k then q + 1 else 2*k - 1 - q] }, range(m))
  else join(x, constant_array(pad, m - k))
}
# the extended length of a level: L - 2 more entries of the padding (none when periodic), then even
fn wv_len(k, l, pad) = { let m = if pad === "Periodic" then k else k + l - 2; m + mod(m, 2) }

# wv_down(x, f): the periodized filtering of the even-length list x by f followed by downsampling,
# y_i = sum_j f_j x_((2 i + j) mod m)
fn wv_down(x, f) = { let m = length(x); total(map(j => f[j + 1]*rotate_left(x, j), range(0, length(f) - 1)))[range(1, m, 2)] }
# wv_up(y, f): the adjoint, x_n = sum_i y_i f_(n - 2 i)
fn wv_up(y, f) = {
  let m = 2*length(y)
  let u = flatten(map(v => [v, 0*v], y))
  total(map(j => f[j + 1]*rotate_right(u, j), range(0, length(f) - 1)))
}
# wv_split(x, h, pad): [approximation, detail] of one level of the list x
fn wv_split(x, h, pad) = {
  let xe = wv_extend(x, wv_len(length(x), length(h), pad), pad)
  [wv_down(xe, h), wv_down(xe, wv_highpass(h))]
}
fn wv_merge(a, d, h, k) = take(wv_up(a, h) + wv_up(d, wv_highpass(h)), k)

# the undecimated (stationary) level j: circular filtering with the filter upsampled by 2^(j-1), halved so that
# the inverse is the mean of the two reconstructions (Nason and Silverman 1995)
fn wv_sfilt(x, f, s) = total(map(j => f[j + 1]*rotate_left(x, mod(j*s, length(x))), range(0, length(f) - 1)))
fn wv_sadj(y, f, s) = total(map(j => f[j + 1]*rotate_right(y, mod(j*s, length(y))), range(0, length(f) - 1)))

# matrices: rows first, then columns
fn wv_split2(x, h, pad) = {
  let rs = map(r => wv_split(r, h, pad), x)
  let lo = transpose(map(r => r[1], rs))
  let hi = transpose(map(r => r[2], rs))
  let a = map(c => wv_split(c, h, pad), lo)
  let b = map(c => wv_split(c, h, pad), hi)
  [transpose(map(c => c[1], a)), transpose(map(c => c[2], a)), transpose(map(c => c[1], b)), transpose(map(c => c[2], b))]
}
fn wv_merge2(cs, h, dims) = {
  let lo = map(i => wv_merge(transpose(cs[1])[i], transpose(cs[2])[i], h, dims[1]), range(length(cs[1][1])))
  let hi = map(i => wv_merge(transpose(cs[3])[i], transpose(cs[4])[i], h, dims[1]), range(length(cs[3][1])))
  let l = transpose(lo)
  let g = transpose(hi)
  map(i => wv_merge(l[i], g[i], h, dims[2]), range(dims[1]))
}
fn wv_ssplit2(x, h, s) = {
  let g = wv_highpass(h)
  let lo = map(r => wv_sfilt(r, h, s), x)
  let hi = map(r => wv_sfilt(r, g, s), x)
  let col = (m, f) => transpose(map(c => wv_sfilt(c, f, s), transpose(m)))
  [col(lo, h), col(lo, g), col(hi, h), col(hi, g)]
}
fn wv_smerge2(cs, h, s) = {
  let g = wv_highpass(h)
  let col = (m, f) => transpose(map(c => wv_sadj(c, f, s), transpose(m)))
  let lo = (col(cs[1], h) + col(cs[2], g))/2
  let hi = (col(cs[3], h) + col(cs[4], g))/2
  (map(r => wv_sadj(r, h, s), lo) + map(r => wv_sadj(r, g, s), hi))/2
}

# ---------------------------------------------------------------- the transforms

fn wv_shape(x) = if is_matrix(x) then dimensions(x) else [length(x)]
# the default number of levels: floor(log2(n/(L - 1))) for the shortest side n and L taps, at least 1
fn wv_levels(x, h, pad) = max(1, integer_length(floor(min(wv_shape(x))/(length(h) - 1)), 2) - 1)

# wv_tree(x, h, r, pad, kind): the coefficients as an association index -> array, and the shapes of the split nodes
fn wv_tree(x, h, r, pad, kind) = {
  let two = is_matrix(x)
  var cs = assoc()
  var shapes = assoc([] -> wv_shape(x))
  var todo = [[[], x]]
  for lev in range(r) {
    var next = []
    for nd in todo {
      let parts = if kind === "SWT" then (if two then wv_ssplit2(nd[2], h, 2^(lev - 1)) else [wv_sfilt(nd[2], h, 2^(lev - 1)), wv_sfilt(nd[2], wv_highpass(h), 2^(lev - 1))]) else (if two then wv_split2(nd[2], h, pad) else wv_split(nd[2], h, pad))
      shapes = join(shapes, assoc(nd[1] -> wv_shape(nd[2])))
      for i in range(length(parts)) {
        let key = append(nd[1], i - 1)
        cs = join(cs, assoc(key -> parts[i]))
        if i == 1 or kind === "Packet" then next = append(next, [key, parts[i]])
      }
    }
    todo = next
  }
  [cs, shapes]
}

fn wv_data(x) = n(x)
fn wv_ok(x) = (is_list(x) and length(x) >= 2 and not is_list(x[1]) and every(n(x), v => is_number(v))) or (is_matrix(x) and min(dimensions(x)) >= 2 and every(flatten(n(x)), v => is_number(v)))

fn wv_transform(x, w, r, pad, kind) = {
  let h = wv_lowpass(w)
  let xd = wv_data(x)
  let rr = if r === automatic then wv_levels(xd, h, if kind === "SWT" then "Periodic" else pad) else r
  let t = wv_tree(xd, h, rr, pad, kind)
  discrete_wavelet_data(t[1], assoc("Wavelet" -> wv_name(w), "Levels" -> rr, "Transform" -> kind, "Padding" -> pad, "Shapes" -> t[2], "DataDimensions" -> wv_shape(x)))
}

# discrete_wavelet_transform(data), (data, w), (data, w, r): the r-level discrete wavelet transform of a list or a
# matrix with the orthonormal wavelet w (haar_wavelet() by default; daubechies_wavelet(n)); padding: "Periodic"
# (default), a number, "Fixed" or "Reflected"
fn discrete_wavelet_transform(x; padding = "Periodic") when wv_ok(x) = wv_transform(x, automatic, automatic, padding, "DWT")
fn discrete_wavelet_transform(x, w; padding = "Periodic") when wv_ok(x) and wv_lowpass(w) !== null = wv_transform(x, w, automatic, padding, "DWT")
fn discrete_wavelet_transform(x, w, r; padding = "Periodic") when wv_ok(x) and wv_lowpass(w) !== null and (r === automatic or (is_integer(r) and r >= 1)) = wv_transform(x, w, r, padding, "DWT")

# discrete_wavelet_packet_transform: every node of the tree is split, approximations and details alike
fn discrete_wavelet_packet_transform(x; padding = "Periodic") when wv_ok(x) = wv_transform(x, automatic, automatic, padding, "Packet")
fn discrete_wavelet_packet_transform(x, w; padding = "Periodic") when wv_ok(x) and wv_lowpass(w) !== null = wv_transform(x, w, automatic, padding, "Packet")
fn discrete_wavelet_packet_transform(x, w, r; padding = "Periodic") when wv_ok(x) and wv_lowpass(w) !== null and (r === automatic or (is_integer(r) and r >= 1)) = wv_transform(x, w, r, padding, "Packet")

# stationary_wavelet_transform: the undecimated transform, every level of the length of the data (circular)
fn stationary_wavelet_transform(x) when wv_ok(x) = wv_transform(x, automatic, automatic, "Periodic", "SWT")
fn stationary_wavelet_transform(x, w) when wv_ok(x) and wv_lowpass(w) !== null = wv_transform(x, w, automatic, "Periodic", "SWT")
fn stationary_wavelet_transform(x, w, r) when wv_ok(x) and wv_lowpass(w) !== null and (r === automatic or (is_integer(r) and r >= 1)) = wv_transform(x, w, r, "Periodic", "SWT")

# the leaves of the tree: the keys that are not split, which hold the data between them
fn wv_leaves(d) = {
  let ks = keys(d[1])
  select(ks, k => not key_exists(d[1], append(k, 0)))
}
fn wv_info(d, p) = lookup(d[2], p)

# inverse_wavelet_transform(d): the data back from the leaves of the discrete wavelet data d
fn wv_rebuild(d, key, cs) = {
  let kind = wv_info(d, "Transform")
  let h = wv_lowpass(wv_info(d, "Wavelet"))
  let shape = lookup(wv_info(d, "Shapes"), key)
  let nkids = if length(shape) == 2 then 4 else 2
  let kids = map(i => {
    let k = append(key, i - 1)
    if key_exists(cs, append(k, 0)) then wv_rebuild(d, k, cs) else lookup(cs, k)
  }, range(nkids))
  let s = 2^length(key)
  if kind === "SWT" then (if nkids == 4 then wv_smerge2(kids, h, s) else (wv_sadj(kids[1], h, s) + wv_sadj(kids[2], wv_highpass(h), s))/2)
  else if nkids == 4 then wv_merge2(kids, h, shape)
  else wv_merge(kids[1], kids[2], h, shape[1])
}
fn inverse_wavelet_transform(d) when not is_atom(d) and head(d) === discrete_wavelet_data = wv_rebuild(d, [], d[1])
fn inverse_wavelet_transform(d, w) when not is_atom(d) and head(d) === discrete_wavelet_data and (w === automatic or w === wv_info(d, "Wavelet")) = wv_rebuild(d, [], d[1])

# the coefficient rules of a data object for a selection: all, automatic (the leaves), a key, a list of keys or a
# pattern of keys
fn wv_select(cs, leaves, sel) = {
  let ks = keys(cs)
  if sel === all then ks
  else if sel === automatic then leaves
  else if is_list(sel) and length(sel) >= 1 and every(sel, is_list) then select(ks, k => some(sel, s => match_q(k, s)))
  else select(ks, k => match_q(k, sel))
}
rule discrete_wavelet_data(cs_, info_)(sel_) := {
  let d = discrete_wavelet_data(cs, info)
  if sel === "Properties" then ["BasisIndex", "DataDimensions", "Levels", "Padding", "Transform", "Wavelet"]
  else if sel === "BasisIndex" then wv_leaves(d)
  else if is_string(sel) and key_exists(info, sel) then lookup(info, sel)
  else map(k => k -> lookup(cs, k), wv_select(cs, wv_leaves(d), sel))
} when is_assoc(cs) and is_assoc(info) and (not is_string(sel) or key_exists(info, sel) or member(["Properties", "BasisIndex"], sel))

# wavelet_map_indexed(f, d), (f, d, sel): the data object with f(c, index) (for a function of two parameters) or
# f(c) in place of every selected coefficient array c
fn wv_call(f, c, k) = if not is_atom(f) and is_list(f[1]) and length(f[1]) == 2 then f(c, k) else f(c)
fn wavelet_map_indexed(f, d) when not is_atom(d) and (head(d) === discrete_wavelet_data or head(d) === continuous_wavelet_data) = wavelet_map_indexed(f, d, all)
fn wavelet_map_indexed(f, d, sel) when not is_atom(d) and (head(d) === discrete_wavelet_data or head(d) === continuous_wavelet_data) = {
  let cs = d[1]
  let chosen = wv_select(cs, if head(d) === discrete_wavelet_data then wv_leaves(d) else keys(cs), sel)
  let new = association_thread(keys(cs), map(k => if member(chosen, k) then wv_call(f, lookup(cs, k), k) else lookup(cs, k), keys(cs)))
  if head(d) === discrete_wavelet_data then discrete_wavelet_data(new, d[2]) else continuous_wavelet_data(new, d[2])
}

# ---------------------------------------------------------------- continuous wavelets

# The Fourier transforms of the continuous wavelets at s w, normalised to unit energy (Torrence and Compo, A
# practical guide to wavelet analysis, Bull. Amer. Meteor. Soc. 79 (1998), table 1): morlet_wavelet(w0) (default 6)
# the analytic Morlet wavelet, gabor_wavelet(w0) (default 6) its Gaussian on both signs of the frequency,
# paul_wavelet(m) (default 4), d_gaussian_wavelet(m) (default 2) the m-th derivative of a Gaussian and
# mexican_hat_wavelet(sigma) (default 1) the second one at the scale sigma
rule morlet_wavelet() := morlet_wavelet(6)
rule gabor_wavelet() := gabor_wavelet(6)
rule paul_wavelet() := paul_wavelet(4)
rule d_gaussian_wavelet() := d_gaussian_wavelet(2)
rule mexican_hat_wavelet() := mexican_hat_wavelet(1)
fn wv_cwave(w) = if w === automatic then morlet_wavelet() else w
fn wv_cok(w) = {
  let v = wv_cwave(w)
  not is_atom(v) and length(v) == 1 and is_numeric(v[1]) and (head(v) === morlet_wavelet or head(v) === gabor_wavelet or (head(v) === mexican_hat_wavelet and n(v[1]) > 0) or ((head(v) === paul_wavelet or head(v) === d_gaussian_wavelet) and is_integer(v[1]) and v[1] >= 1))
}
fn wv_analytic(w) = { let v = wv_cwave(w); head(v) === morlet_wavelet or head(v) === paul_wavelet }
# psi-hat(u) at the list u = s w of frequencies
fn wv_psihat(w, u) = {
  let v = wv_cwave(w)
  let p = n(v[1])
  if head(v) === morlet_wavelet then n(pi^(-1/4))*unit_step(u)*exp(-(u - p)^2/2)
  else if head(v) === gabor_wavelet then n(pi^(-1/4))*exp(-(u - p)^2/2)
  else if head(v) === paul_wavelet then n(2^p/sqrt(p*factorial(2*p - 1)))*unit_step(u)*abs(u)^p*exp(-abs(u))
  else if head(v) === d_gaussian_wavelet then n(-im^p/sqrt(gamma(p + 1/2)))*u^p*exp(-u^2/2)
  else n(sqrt(p)*(-im^2)/sqrt(gamma(5/2)))*(p*u)^2*exp(-(p*u)^2/2)
}
# the angular frequencies of the discrete Fourier coefficients (unit sampling step)
fn wv_omega(k) = map(j => n(2*pi*(if j <= floor(k/2) then j else j - k)/k), range(0, k - 1))
fn wv_scales(noct, nvoc) = flatten(table([j, v] -> n(2*2^(j - 1 + (v - 1)/nvoc)), [j, noct], [v, nvoc]))
# the frequencies where the sum of |Psi_s|^2 over the scales is below wv_tau() of its largest value are kept as
# Fourier coefficients (the division by that sum would amplify rounding by more than 1/wv_tau())
fn wv_tau() = 1.0e-4
fn wv_dft(x) = fourier(x, fourier_parameters: [1, -1])
fn wv_idft(x) = inverse_fourier(x, fourier_parameters: [1, -1])

fn wv_cwt(x, w, spec) = {
  let k = length(x)
  let xd = n(x)
  let noct = if spec[1] === automatic then max(1, integer_length(k, 2) - 2) else spec[1]
  let nvoc = if spec[2] === automatic then 4 else spec[2]
  let om = wv_omega(k)
  let xf = wv_dft(xd)
  let sc = wv_scales(noct, nvoc)
  let real = every(xd, v => im_part(v) == 0)
  let psis = map(r => sqrt(n(2*pi)*r[2])*wv_psihat(w, r[2]*om), sc)
  let cs = association_thread(map(r => r[1], sc), map(ps => wv_idft(xf*conjugate(ps)), psis))
  # the frequencies the scales do not reach: their Fourier coefficients, kept so that the inverse is exact
  let den = total(map(ps => abs(ps)^2, psis))
  let top = max(den)
  let covered = map(j => den[j] > wv_tau()*top or (real and wv_analytic(w) and om[j] < 0 and den[mod(k - j + 1, k) + 1] > wv_tau()*top), range(k))
  let res = map(j => if covered[j] then 0 else xf[j], range(k))
  continuous_wavelet_data(cs, assoc("Wavelet" -> wv_cwave(w), "Octaves" -> noct, "Voices" -> nvoc, "Scales" -> apply(assoc, sc), "Residual" -> res, "Real" -> real, "Length" -> k))
}
fn wv_cspec(sp) = is_list(sp) and length(sp) == 2 and (sp[1] === automatic or (is_integer(sp[1]) and sp[1] >= 1)) and (sp[2] === automatic or (is_integer(sp[2]) and sp[2] >= 1))
fn wv_vok(x) = is_list(x) and length(x) >= 2 and not is_list(x[1]) and every(n(x), v => is_number(v))

# continuous_wavelet_transform(data), (data, w), (data, w, [noct, nvoc]): the continuous wavelet transform of a
# list at the scales s = 2 2^(j - 1 + (v - 1)/nvoc), octaves j and voices v, computed by the fast Fourier transform
# (Torrence and Compo 1998, eq. 4); w is morlet_wavelet() by default
fn continuous_wavelet_transform(x) when wv_vok(x) = wv_cwt(x, automatic, [automatic, automatic])
fn continuous_wavelet_transform(x, w) when wv_vok(x) and wv_cok(w) = wv_cwt(x, w, [automatic, automatic])
fn continuous_wavelet_transform(x, w, sp) when wv_vok(x) and wv_cok(w) and wv_cspec(sp) = wv_cwt(x, w, sp)
fn continuous_wavelet_transform(x, w, k) when wv_vok(x) and wv_cok(w) and is_integer(k) and k >= 1 = wv_cwt(x, w, [k, automatic])

# inverse_continuous_wavelet_transform(c), (c, w), (c, w, sel): the data from the coefficients, by least squares
# in every Fourier coefficient: x_k = sum_s Psi_s(w_k) W_s(w_k) / sum_s |Psi_s(w_k)|^2 over the scales s (each scale
# is a convolution, so the normal equations are diagonal in frequency), plus the stored remainder; with sel, only
# the selected scales enter the numerator and the remainder is left out (a band-pass reconstruction)
fn wv_icwt(c, sel) = {
  let info = c[2]
  let w = lookup(info, "Wavelet")
  let k = lookup(info, "Length")
  let om = wv_omega(k)
  let sc = lookup(info, "Scales")
  let ks = keys(c[1])
  let chosen = wv_select(c[1], ks, sel)
  let psis = association_thread(ks, map(key => sqrt(n(2*pi)*lookup(sc, key))*wv_psihat(w, lookup(sc, key)*om), ks))
  let den = total(map(key => abs(lookup(psis, key))^2, ks))
  let top = max(den)
  let num = total(map(key => lookup(psis, key)*wv_dft(lookup(c[1], key)), chosen))
  let xf0 = map(j => if den[j] > wv_tau()*top then num[j]/den[j] else 0, range(k))
  let real = lookup(info, "Real")
  # real data and an analytic wavelet: the negative frequencies are the conjugates of the positive ones
  let xf1 = if real and wv_analytic(w) then map(j => if om[j] < 0 and not (den[j] > wv_tau()*top) then conjugate(xf0[mod(k - j + 1, k) + 1]) else xf0[j], range(k)) else xf0
  let xf = if sel === all then xf1 + lookup(info, "Residual") else xf1
  let x = wv_idft(xf)
  if real then re(x) else x
}
fn wv_isc(c) = not is_atom(c) and head(c) === continuous_wavelet_data
fn inverse_continuous_wavelet_transform(c) when wv_isc(c) = wv_icwt(c, all)
# method: "LeastSquares" is the method above (the only one)
rule inverse_continuous_wavelet_transform(c_, options(method -> m_)) := wv_icwt(c, all) when wv_isc(c) and (m === automatic or m === "LeastSquares")
fn inverse_continuous_wavelet_transform(c, w) when wv_isc(c) and (w === automatic or w === lookup(c[2], "Wavelet")) = wv_icwt(c, all)
fn inverse_continuous_wavelet_transform(c, w, sel) when wv_isc(c) and (w === automatic or w === lookup(c[2], "Wavelet")) = wv_icwt(c, sel)

# continuous_wavelet_data([[j, v] -> list, ...]), (rules, w): continuous wavelet data from coefficient lists of one
# length (the wavelet morlet_wavelet() by default; octaves and voices from the largest indices, the missing lists
# zero; the data count as real when every coefficient is real)
fn wv_cdata(rs, w) = {
  let ks = map(r => r[1], rs)
  let k = length(rs[1][2])
  let noct = max(map(q => q[1], ks))
  let nvoc = max(map(q => q[2], ks))
  let sc = wv_scales(noct, nvoc)
  let lk = association_thread(ks, map(r => n(r[2]), rs))
  let cs = association_thread(map(r => r[1], sc), map(r => if key_exists(lk, r[1]) then lookup(lk, r[1]) else constant_array(0.0, k), sc))
  continuous_wavelet_data(cs, assoc("Wavelet" -> wv_cwave(w), "Octaves" -> noct, "Voices" -> nvoc, "Scales" -> apply(assoc, sc), "Residual" -> constant_array(0.0, k), "Real" -> every(flatten(map(r => n(r[2]), rs)), x => im_part(x) == 0), "Length" -> k))
}
fn wv_rules_ok(rs) = is_list(rs) and length(rs) >= 1 and every(rs, r => match_q(r, _ -> _) and is_list(r[1]) and length(r[1]) == 2 and is_integer(r[1][1]) and is_integer(r[1][2]) and r[1][1] >= 1 and r[1][2] >= 1 and is_list(r[2]) and length(r[2]) == length(rs[1][2]) and length(r[2]) >= 2)
rule continuous_wavelet_data(rs_) := wv_cdata(rs, automatic) when wv_rules_ok(rs)
rule continuous_wavelet_data(rs_, w_) := wv_cdata(rs, w) when wv_rules_ok(rs) and wv_cok(w)

rule continuous_wavelet_data(cs_, info_)(sel_) := {
  if sel === "Properties" then ["IndexMap", "Length", "Octaves", "Scales", "Voices", "Wavelet"]
  else if sel === "IndexMap" then keys(cs)
  else if sel === "Scales" then normal(lookup(info, "Scales"))
  else if is_string(sel) then lookup(info, sel)
  else map(k => k -> lookup(cs, k), wv_select(cs, keys(cs), sel))
} when is_assoc(cs) and is_assoc(info) and (not is_string(sel) or member(["Properties", "IndexMap", "Scales", "Length", "Octaves", "Voices", "Wavelet"], sel))

# ---------------------------------------------------------------- plots

# options for the plots: the layout option is read here, the rest goes to the plotting functions
fn wv_opts(o) = apply(options, select(apply(list, o), r => not (r[1] === plot_layout)))
fn wv_layout(o) = { let rs = select(apply(list, o), r => r[1] === plot_layout); if length(rs) == 0 then "CommonXAxis" else rs[1][2] }
fn wv_label(k) = to_string(k)
# real values as they are, complex ones by their modulus
fn wv_real(v) = if every(flatten([v]), x => im_part(x) == 0) then re(v) else abs(v)
fn wv_rows(xs, k) = map(i => take(drop(xs, k*(i - 1)), min(k, length(xs) - k*(i - 1))), range(ceiling(length(xs)/k)))
# blocks of matrices side by side and one above the other, padded with zeros to a common size
fn wv_pad(m, r, c) = join(map(row => join(row, constant_array(0, c - length(row))), m), constant_array(0, [r - length(m), c]))
fn wv_hjoin(a, b) = { let r = max(length(a), length(b)); let pa = wv_pad(a, r, length(a[1])); let pb = wv_pad(b, r, length(b[1])); map(i => join(pa[i], pb[i]), range(r)) }
fn wv_vjoin(a, b) = { let c = max(length(a[1]), length(b[1])); join(wv_pad(a, length(a), c), wv_pad(b, length(b), c)) }

# wavelet_list_plot(d), (d, sel), (d, sel, f): the coefficients of the discrete wavelet data d (or a list of them,
# drawn together), one plot of f(c) (default the coefficients themselves, the modulus of complex ones) per index;
# plot_layout: "CommonXAxis" (a column, default) or "CommonYAxis" (a row)
fn wv_isd(d) = not is_atom(d) and head(d) === discrete_wavelet_data and length(wv_info(d, "DataDimensions")) == 1
fn wv_list_plot(ds, sel, f, o) = {
  let d = ds[1]
  let ks = wv_select(d[1], wv_leaves(d), sel)
  let plots = map(k => list_plot(map(e => wv_real(f(lookup(e[1], k))), ds), filling: axis, plot_label: wv_label(k), wv_opts(o)), ks)
  if wv_layout(o) === "CommonYAxis" then graphics_row(plots) else graphics_column(plots)
}
rule wavelet_list_plot(d_) := wv_list_plot([d], automatic, x => x, options()) when wv_isd(d)
rule wavelet_list_plot(d_, o_) := wv_list_plot([d], automatic, x => x, o) when wv_isd(d) and not is_atom(o) and head(o) === options
rule wavelet_list_plot(d_, sel_) := wv_list_plot([d], sel, x => x, options()) when wv_isd(d) and (is_atom(sel) or not (head(sel) === options))
rule wavelet_list_plot(d_, sel_, o_) := wv_list_plot([d], sel, x => x, o) when wv_isd(d) and not is_atom(o) and head(o) === options
rule wavelet_list_plot(d_, sel_, f_) := wv_list_plot([d], sel, f, options()) when wv_isd(d) and (is_atom(f) or not (head(f) === options))
rule wavelet_list_plot(d_, sel_, f_, o_) := wv_list_plot([d], sel, f, o) when wv_isd(d) and not is_atom(o) and head(o) === options
rule wavelet_list_plot(ds_) := wv_list_plot(ds, automatic, x => x, options()) when is_list(ds) and length(ds) >= 1 and every(ds, wv_isd)
rule wavelet_list_plot(ds_, o_) := wv_list_plot(ds, automatic, x => x, o) when is_list(ds) and length(ds) >= 1 and every(ds, wv_isd) and not is_atom(o) and head(o) === options

# wavelet_matrix_plot(d), (d, r), (d, sel), (d, sel, f): the coefficients of matrix data: for the discrete
# transform the arrays of the levels up to r tiled as the transform stores them (the approximation in the corner,
# the details of each level around it); for the other transforms one plot per index in a grid
fn wv_ism(d) = not is_atom(d) and head(d) === discrete_wavelet_data and length(wv_info(d, "DataDimensions")) == 2
fn wv_mval(f, m) = wv_real(f(m))
fn wv_tile(d, key, depth, f) = {
  let a = lookup(d[1], append(key, 0))
  let sub = if depth > 1 and key_exists(d[1], append(append(key, 0), 0)) then wv_tile(d, append(key, 0), depth - 1, f) else wv_mval(f, a)
  let m2 = wv_mval(f, lookup(d[1], append(key, 2)))
  let m1 = wv_mval(f, lookup(d[1], append(key, 1)))
  let m3 = wv_mval(f, lookup(d[1], append(key, 3)))
  wv_vjoin(wv_hjoin(sub, m2), wv_hjoin(m1, m3))
}
fn wv_matrix_plot(d, r, sel, f, o) = {
  let kind = wv_info(d, "Transform")
  let ks = wv_select(d[1], wv_leaves(d), sel)
  if kind === "DWT" and sel === automatic and wv_layout(o) !== "Grid" then matrix_plot(wv_tile(d, [], if r === automatic then wv_info(d, "Levels") else r, f), wv_opts(o))
  else graphics_grid(wv_rows(map(k => matrix_plot(wv_mval(f, lookup(d[1], k)), plot_label: wv_label(k), wv_opts(o)), ks), 4))
}
rule wavelet_matrix_plot(d_) := wv_matrix_plot(d, automatic, automatic, x => x, options()) when wv_ism(d)
rule wavelet_matrix_plot(d_, o_) := wv_matrix_plot(d, automatic, automatic, x => x, o) when wv_ism(d) and not is_atom(o) and head(o) === options
rule wavelet_matrix_plot(d_, r_) := wv_matrix_plot(d, r, automatic, x => x, options()) when wv_ism(d) and is_integer(r) and r >= 1
rule wavelet_matrix_plot(d_, r_, o_) := wv_matrix_plot(d, r, automatic, x => x, o) when wv_ism(d) and is_integer(r) and r >= 1 and not is_atom(o) and head(o) === options
rule wavelet_matrix_plot(d_, sel_) := wv_matrix_plot(d, automatic, sel, x => x, options()) when wv_ism(d) and not is_integer(sel) and (is_atom(sel) or not (head(sel) === options))
rule wavelet_matrix_plot(d_, sel_, f_) := wv_matrix_plot(d, automatic, sel, f, options()) when wv_ism(d) and not is_integer(sel) and (is_atom(f) or not (head(f) === options))
rule wavelet_matrix_plot(d_, sel_, f_, o_) := wv_matrix_plot(d, automatic, sel, f, o) when wv_ism(d) and not is_integer(sel) and not is_atom(o) and head(o) === options

# wavelet_scalogram(d), (d, sel), (d, sel, f): the coefficients against time and scale, |c| (or f(c)) as colour:
# for discrete wavelet data of a list every coefficient covers its 2^level samples of the data, for continuous
# wavelet data every scale is a row (the smallest at the top)
fn wv_spread(c, k) = { let m = length(c); map(i => c[min(m, floor((i - 1)*m/k) + 1)], range(k)) }
fn wv_scal(d, sel, f, o) = {
  if head(d) === continuous_wavelet_data then {
    let ks = wv_select(d[1], keys(d[1]), sel)
    array_plot(map(k => map(x => re(f(x)), lookup(d[1], k)), ks), wv_opts(o))
  } else {
    let ks = wv_select(d[1], wv_leaves(d), sel)
    let k = wv_info(d, "DataDimensions")[1]
    array_plot(map(key => wv_spread(map(x => re(f(x)), lookup(d[1], key)), k), ks), wv_opts(o))
  }
}
fn wv_isscal(d) = wv_isc(d) or wv_isd(d)
rule wavelet_scalogram(d_) := wv_scal(d, automatic, abs, options()) when wv_isscal(d)
rule wavelet_scalogram(d_, o_) := wv_scal(d, automatic, abs, o) when wv_isscal(d) and not is_atom(o) and head(o) === options
rule wavelet_scalogram(d_, sel_) := wv_scal(d, sel, abs, options()) when wv_isscal(d) and (is_atom(sel) or not (head(sel) === options))
rule wavelet_scalogram(d_, sel_, o_) := wv_scal(d, sel, abs, o) when wv_isscal(d) and not is_atom(o) and head(o) === options
rule wavelet_scalogram(d_, sel_, f_) := wv_scal(d, sel, f, options()) when wv_isscal(d) and (is_atom(f) or not (head(f) === options))
rule wavelet_scalogram(d_, sel_, f_, o_) := wv_scal(d, sel, f, o) when wv_isscal(d) and not is_atom(o) and head(o) === options
