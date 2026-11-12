//! Wavelet transforms: the filters against their defining conditions, perfect reconstruction of the discrete,
//! packet, stationary and continuous transforms for several wavelets, paddings, lengths and shapes, energy
//! preservation of the orthogonal transforms, and the plots as pictures.
use mc_api::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};

fn ev(src: &str) -> String {
    let stop = StopFlag::new();
    let mut e = CoreEngine::with_config(EngineConfig { threads: 1, ..EngineConfig::default() });
    match e.eval(src, &stop) {
        Ok(v) => v.printed,
        Err(err) => format!("error: {err}"),
    }
}

#[test]
fn daubechies_filters_are_orthonormal_with_vanishing_moments() {
    // sum h = sqrt 2, sum h_k h_(k+2m) = delta_m, and sum (-1)^k k^p h_k = 0 for p < n (the moments of the high-pass
    // filter), for n = 2, 3, 6
    for k in [2, 3, 6] {
        let src = format!(
            "{{ let h = wv_daub({k}); let l = length(h); let ip = m => total(map(j => h[j]*h[j + 2*m], range(l - 2*m))); \
             [abs(total(h) - sqrt(2.)) < 1e-14, abs(ip(0) - 1) < 1e-14, every(range({k} - 1), m => abs(ip(m)) < 1e-14), \
             every(range(0, {k} - 1), p => abs(total(map(j => (-1)^j*j^p*h[j + 1], range(0, l - 1)))) < 1e-9)] }}"
        );
        assert_eq!(ev(&src), "[true, true, true, true]", "n = {k}");
    }
    // the four taps of daubechies_wavelet(2): (1 + sqrt 3, 3 + sqrt 3, 3 - sqrt 3, 1 - sqrt 3)/(4 sqrt 2)
    assert_eq!(ev("max(abs(wv_daub(2) - n([1 + sqrt(3), 3 + sqrt(3), 3 - sqrt(3), 1 - sqrt(3)]/(4*sqrt(2))))) < 1e-15"), "true");
}

#[test]
fn discrete_transforms_reconstruct_the_data() {
    // documented form: four levels of a sampled signal of odd length 129
    assert_eq!(ev("{ let x = table(2 - 5*t + 5*exp(-500*(t - 0.5)^2), [t, 0., 1., 1/128]); max(abs(inverse_wavelet_transform(discrete_wavelet_transform(x, automatic, 4)) - x)) < 1e-13 }"), "true");
    // other wavelets, paddings and lengths
    for (w, pad, len) in [("daubechies_wavelet(3)", "0", 37), ("daubechies_wavelet(5)", "\"Reflected\"", 50), ("daubechies_wavelet(2)", "\"Fixed\"", 21), ("haar_wavelet()", "\"Periodic\"", 2)] {
        let src = format!("{{ let x = n(table(sin(k^2/11) + k/7, [k, {len}])); max(abs(inverse_wavelet_transform(discrete_wavelet_transform(x, {w}, automatic, padding: {pad})) - x)) < 1e-12 }}");
        assert_eq!(ev(&src), "true", "{w} {pad} {len}");
    }
    // periodic orthogonal transforms preserve the energy
    assert_eq!(ev("{ let x = n(table(cos(k^3/50), [k, 64])); let d = discrete_wavelet_transform(x, daubechies_wavelet(4), 3); abs(total(x^2) - total(map(r => total(r[2]^2), d(automatic)))) < 1e-12 }"), "true");
    // matrices, complex entries, the packet transform
    assert_eq!(ev("{ let m = n(table(exp(im*(x/y)), [x, 10], [y, 10])); max(abs(flatten(inverse_wavelet_transform(discrete_wavelet_transform(m)) - m))) < 1e-13 }"), "true");
    assert_eq!(ev("{ let m = n(diamond_matrix(9)); max(abs(flatten(inverse_wavelet_transform(discrete_wavelet_packet_transform(m, daubechies_wavelet(2), 2)) - m))) < 1e-13 }"), "true");
}

#[test]
fn haar_coefficients_are_scaled_sums_and_differences() {
    assert_eq!(ev("{ let d = discrete_wavelet_transform([1, 3, 5, 7], haar_wavelet(), 1); max(abs(join(d([0])[1][2] - [4, 12]/sqrt(2.), d([1])[1][2] - [-2, -2]/sqrt(2.)))) < 1e-14 }"), "true");
    // two levels: the second approximation is the sum of four values over 2
    assert_eq!(ev("{ let d = discrete_wavelet_transform(range(8), haar_wavelet(), 2); max(abs(d([0, 0])[1][2] - [10, 26]/2)) < 1e-14 }"), "true");
}

#[test]
fn stationary_transform_is_shift_invariant_and_invertible() {
    assert_eq!(ev("{ let x = sin(range(30)^2/50); max(abs(inverse_wavelet_transform(stationary_wavelet_transform(x, automatic, 3)) - n(x))) < 1e-13 }"), "true");
    assert_eq!(ev("{ let x = n(range(12)^2); let a = stationary_wavelet_transform(x, daubechies_wavelet(2), 2); let b = stationary_wavelet_transform(rotate_left(x, 3), daubechies_wavelet(2), 2); max(abs(rotate_left(a([0, 1])[1][2], 3) - b([0, 1])[1][2])) < 1e-12 }"), "true");
    assert_eq!(ev("{ let m = n(table(sin(x*y/30), [x, 11], [y, 7])); max(abs(flatten(inverse_wavelet_transform(stationary_wavelet_transform(m, daubechies_wavelet(2), 2)) - m))) < 1e-13 }"), "true");
}

#[test]
fn continuous_transform_inverts_by_least_squares() {
    assert_eq!(ev("max(abs(inverse_continuous_wavelet_transform(continuous_wavelet_transform([56, 40, 8, 24, 48, 48, 40, 16])) - [56, 40, 8, 24, 48, 48, 40, 16])) < 1e-12"), "true");
    for w in ["morlet_wavelet()", "gabor_wavelet(4)", "paul_wavelet()", "d_gaussian_wavelet(3)", "mexican_hat_wavelet(2)"] {
        let src = format!("{{ let x = n(table(sin(k^2/300) + k/50, [k, 211])); max(abs(inverse_continuous_wavelet_transform(continuous_wavelet_transform(x, {w}, [5, 3])) - x)) < 1e-9 }}");
        assert_eq!(ev(&src), "true", "{w}");
    }
    // complex data
    assert_eq!(ev("{ let x = n(exp(im*range(40)^2/30)); max(abs(inverse_continuous_wavelet_transform(continuous_wavelet_transform(x, gabor_wavelet(), [4, 4])) - x)) < 1e-9 }"), "true");
    // the strongest scale of a sinusoid of period 16 under the Morlet wavelet: Fourier period 4 pi s/(6 + sqrt(38))
    // (Torrence and Compo, table 1) is 16 at s = 15.6, the scale 2 2^(3 + 0/4) = 16 of index [4, 1]
    assert_eq!(ev("{ let c = continuous_wavelet_transform(n(sin(2*pi*range(256)/16)), morlet_wavelet(), [6, 4]); let ks = c(\"IndexMap\"); ks[ordering(map(k => total(abs(c(k)[1][2])^2), ks))[-1]] }"), "[4, 1]");
}

#[test]
fn plots_are_pictures() {
    assert_eq!(ev("head(wavelet_list_plot(discrete_wavelet_transform(sin(range(30)^2/100))))"), "graphics");
    assert_eq!(ev("head(wavelet_matrix_plot(discrete_wavelet_transform(diamond_matrix(16), automatic, 2)))"), "image");
    assert_eq!(ev("head(wavelet_scalogram(continuous_wavelet_transform(n(sin(range(100)/5)))))"), "image");
}
