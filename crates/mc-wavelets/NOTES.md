# mc-wavelets: notes

## Round 38: the wavelet library

Everything is MathCAS source (`lib/wavelets.mc`), in machine numbers (exact data are converted with `n`).

### Filters

- `haar_wavelet()`: h = [1, 1]/sqrt(2).
- `daubechies_wavelet(n)`, 1 <= n <= 20: the spectral factorisation of Daubechies (Ten lectures on wavelets, SIAM
  1992, 6.1): |m0(w)|^2 = cos(w/2)^(2n) P(sin(w/2)^2) with P(y) = sum_(k<n) binomial(n - 1 + k, k) y^k. The n - 1
  roots of P are found by `n_solve` at 40 digits; each gives the root of z^2 - (2 - 4y) z + 1 = 0 outside the unit
  circle, and h(z) = c (1 + z)^n prod (z - z_k) normalised to sum h = sqrt(2): the extremal-phase filter with the
  largest taps first (daubechies_wavelet(2) = [0.4830, 0.8365, 0.2241, -0.1294]). The high-pass filter is the
  quadrature mirror g_k = (-1)^k h_(L-1-k).

### Discrete, packet and stationary transforms

- One level of the discrete transform of a list x (Mallat, A wavelet tour of signal processing, 7.5): x is extended
  on the right by L - 2 entries of the padding (none for "Periodic") and to an even length m, and filtered with
  periodization, a_i = sum_j h_j x_((2i + j) mod m), d_i likewise with g. Orthonormal filters make the periodized
  level orthogonal, so the inverse is the adjoint followed by truncation to the stored length: the reconstruction is
  exact for every padding. Decision: padding "Periodic" (default), a number, "Fixed" (the last value) or "Reflected"
  (mirrored without repeating the end).
- Matrices: one level on every row, then on every column of the row approximations and details. Indices 0 (low in
  both directions), 1 (high along the columns), 2 (high along the rows), 3 (high in both).
- Keys: [0] and [1] for the first level, [0, 0] and [0, 1] for the split of [0], and so on; the packet transform
  splits every node. The data object keeps every node (`d(all)`); `d(automatic)` gives the leaves, which hold the
  data between them.
- Decision: levels by default floor(log2(n/(L - 1))) for the shortest side n and L taps, at least 1.
- The stationary transform (Nason and Silverman, The stationary wavelet transform and some statistical
  applications, 1995): circular filtering with the filters upsampled by 2^(j-1) at level j, without decimation;
  since |H|^2 + |G|^2 = 2 at every frequency, the inverse is the mean of the two adjoint filterings, exact for every
  length.

### Continuous transform

- Torrence and Compo, A practical guide to wavelet analysis, Bull. Amer. Meteor. Soc. 79 (1998): W_n(s) = sum_k
  x_k Psi(s w_k)^* e^(i w_k n) with Psi(s w) = sqrt(2 pi s) psi(s w) in frequency, by the fast Fourier transform;
  the transforms of the wavelets of their table 1 (Morlet with the Heaviside factor, Paul, derivative of a
  Gaussian). `gabor_wavelet` is the Gaussian of the Morlet wavelet on both signs of the frequency,
  `mexican_hat_wavelet(sigma)` the second derivative of a Gaussian at the scale sigma.
- Decision: scales s = 2 * 2^(j - 1 + (v - 1)/nvoc) samples for octaves j = 1..noct and voices v = 1..nvoc, indexed
  [j, v]; by default noct = max(1, integer_length(n, 2) - 2) and nvoc = 4.
- Inverse: every scale is a convolution, so the least-squares inverse is diagonal in frequency: X_k = sum_s
  Psi_s(w_k) W_s,k / sum_s |Psi_s(w_k)|^2 (W_s,k the Fourier coefficients of the scale s). The frequencies where that
  sum is below 1e-4 of its largest value (the constant term and the edges of the band) are kept in the data object
  as Fourier coefficients, so that the inverse of the whole transform reproduces the data to rounding (the division
  elsewhere amplifies rounding by at most 1e4). For real data and an analytic wavelet the negative frequencies are
  the conjugates of the positive ones. With a selection only the selected scales enter, without the kept remainder
  (the band-pass part of the data).
- `continuous_wavelet_data([[j, v] -> list, ...])` builds data from coefficient lists (missing indices zero, the
  wavelet `morlet_wavelet()` unless given). The option `method: "LeastSquares"` names the one inversion method.
- Gaps: inversion with another wavelet than the one of the transform and other inversion methods stay
  unevaluated; Symlets, Coiflets, biorthogonal and Meyer wavelets are not provided (their filters need a nonlinear
  design or tables).

### Plots

`wavelet_list_plot` (stem plots of the leaves; plot_layout "CommonXAxis" a column, "CommonYAxis" a row; a list of
data objects drawn together), `wavelet_matrix_plot` (the tiling of a matrix transform, levels up to r; a grid of the
leaves for the other transforms), `wavelet_scalogram` (array plots of |c| against time and scale). Other options go
to the plotting functions.
