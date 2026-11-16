# mc-wavelets

Wavelet analysis: the discrete (periodized, decimated), stationary (undecimated) and packet wavelet transforms of lists
and matrices with the orthonormal Haar and Daubechies wavelets, the continuous wavelet transform with the Morlet,
Gabor, Paul and derivative-of-Gaussian wavelets, their inverses, the data objects `discrete_wavelet_data` and
`continuous_wavelet_data` with their selections, and plots of the coefficients.

## Modules
- `lib.rs`: exposes the library source as `SOURCE`.
- `lib/wavelets.mc`: the library itself, written in MathCAS (filters, decimation and reconstruction, stationary and
  packet transforms, continuous transforms, data objects and plots).

## Design
- Everything is MathCAS source built on the list, Fourier, root-finding and plot libraries and called through the
  evaluator; the Rust side only embeds the text.
- The crate has no Rust dependencies.

## Tests
- The crate has no tests of its own; the functions are tested through the assembled engine in the integration tests of
  `mc-engine`.
- Run them with `cargo test --release -p mc-engine`.
