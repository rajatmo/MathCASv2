//! MathCAS v2 wavelet analysis.
//!
//! The discrete (periodized, decimated), stationary (undecimated) and packet wavelet transforms of lists and
//! matrices with the orthonormal Haar and Daubechies wavelets, the continuous wavelet transform of a list with the
//! Morlet, Gabor, Paul and derivative-of-Gaussian wavelets, their inverses, the data objects
//! `discrete_wavelet_data` and `continuous_wavelet_data` with their selections, and the plots of the coefficients.
//! Everything is MathCAS source (`lib/wavelets.mc`, [`SOURCE`]) built on the list, Fourier, root-finding and plot
//! libraries, called through the evaluator. Methods and literature: `NOTES.md`.
#![warn(missing_docs)]

/// The library's source text.
pub const SOURCE: &str = include_str!("../lib/wavelets.mc");

/// The name of the source in messages.
pub const ORIGIN: &str = "lib/wavelets.mc";

/// The public functions of this crate with a one-line description each.
pub const FUNCTIONS: &[(&str, &str)] = &[
    ("haar_wavelet", "haar_wavelet(): the Haar wavelet, the orthonormal filter [1, 1]/sqrt(2)"),
    ("daubechies_wavelet", "daubechies_wavelet(n): the orthonormal Daubechies wavelet with n vanishing moments (2 n taps); daubechies_wavelet() is n = 2"),
    ("discrete_wavelet_transform", "discrete_wavelet_transform(data), (data, w), (data, w, r): the r-level discrete wavelet transform of a list or matrix (padding: \"Periodic\", a number, \"Fixed\", \"Reflected\")"),
    ("discrete_wavelet_packet_transform", "discrete_wavelet_packet_transform(data), (data, w), (data, w, r): the wavelet packet transform, every node split"),
    ("stationary_wavelet_transform", "stationary_wavelet_transform(data), (data, w), (data, w, r): the undecimated (stationary) wavelet transform of a list or matrix"),
    ("inverse_wavelet_transform", "inverse_wavelet_transform(d): the data reconstructed from the discrete wavelet data d"),
    ("discrete_wavelet_data", "discrete_wavelet_data(coefficients, info): the coefficients of a discrete wavelet transform; d(all), d(automatic), d(index), d(pattern), d(\"Wavelet\"), ..."),
    ("continuous_wavelet_transform", "continuous_wavelet_transform(data), (data, w), (data, w, [noct, nvoc]): the continuous wavelet transform of a list over noct octaves of nvoc voices"),
    ("inverse_continuous_wavelet_transform", "inverse_continuous_wavelet_transform(c), (c, w), (c, w, sel): the data from continuous wavelet coefficients (least squares), or the part of the selected scales"),
    ("continuous_wavelet_data", "continuous_wavelet_data(coefficients, info): the coefficients of a continuous wavelet transform by [octave, voice]; c(all), c(index), c(\"Scales\"), ..."),
    ("morlet_wavelet", "morlet_wavelet(w0): the analytic Morlet wavelet of central frequency w0 (default 6)"),
    ("gabor_wavelet", "gabor_wavelet(w0): the Gabor wavelet, a Gaussian-windowed complex exponential of frequency w0 (default 6)"),
    ("paul_wavelet", "paul_wavelet(m): the Paul wavelet of order m (default 4)"),
    ("d_gaussian_wavelet", "d_gaussian_wavelet(m): the m-th derivative of a Gaussian as a wavelet (default 2)"),
    ("mexican_hat_wavelet", "mexican_hat_wavelet(sigma): the Mexican hat wavelet (the second derivative of a Gaussian) at the scale sigma (default 1)"),
    ("wavelet_map_indexed", "wavelet_map_indexed(f, d), (f, d, sel): the wavelet data d with f(c, index) applied to the selected coefficient arrays"),
    ("wavelet_list_plot", "wavelet_list_plot(d), (d, sel), (d, sel, f): the coefficients of discrete wavelet data of a list, one plot per index"),
    ("wavelet_matrix_plot", "wavelet_matrix_plot(d), (d, r), (d, sel, f): the coefficients of discrete wavelet data of a matrix, tiled as stored"),
    ("wavelet_scalogram", "wavelet_scalogram(d), (d, sel), (d, sel, f): the magnitude of wavelet coefficients against time and scale"),
];
