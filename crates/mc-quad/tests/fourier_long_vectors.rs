//! `fourier` and `inverse_fourier` of a long vector of machine numbers are done by one native call; the value must be
//! the language definition's (`nl_fourier`) bit for bit: real, complex and conjugate-even data, lengths around the
//! threshold, powers of two and others, several `fourier_parameters`.
use mc_api::{Engine, StopFlag};
use std::time::{Duration, Instant};

fn eval(e: &mut mc_engine::CoreEngine, src: &str) -> String {
    let stop = StopFlag::new();
    e.eval(src, &stop).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

#[test]
fn native_transforms_are_the_language_transforms() {
    let t0 = Instant::now();
    let mut e = mc_engine::CoreEngine::new();
    let data = [
        // real data
        "u = map(k => sin(0.37*k^2) + 0.25*k, range(LEN))",
        // complex data
        "u = map(k => cos(1.1*k) + sin(0.3*k)*im, range(LEN))",
        // conjugate-even data: real and symmetric under k -> n + 2 - k (a real transform)
        "u = map(k => cos(2*pi*3.*(k - 1)/LEN) + 0.5, range(LEN))",
        // packed zeros
        "u = constant_array(0., LEN)",
    ];
    let mut bad = vec![];
    let mut checked = 0;
    for len in [129, 200, 256, 1000, 1024] {
        for d in data {
            eval(&mut e, &d.replace("LEN", &len.to_string()));
            for (a, b) in [("0", "1"), ("1", "-1"), ("-1", "1"), ("0", "3"), ("1/2", "2")] {
                for (f, sgn) in [("fourier", "1"), ("inverse_fourier", "-1")] {
                    let fast = format!("{f}(u, fourier_parameters: [{a}, {b}])");
                    let lang = format!("nl_fourier(u, {a}, {b}, {sgn})");
                    // the native answered (not the language), and gave the language's value
                    let native = eval(&mut e, &format!("is_list(nl_fourier_machine(u, {sgn}*{b}, n(1/length(u)^((1 - {sgn}*{a})/2))))"));
                    let same = eval(&mut e, &format!("{fast} === {lang}"));
                    if native != "true" || same != "true" {
                        bad.push(format!("{d} (len {len}) {fast}: native {native}, same {same}"));
                    }
                    checked += 1;
                }
            }
        }
    }
    assert!(bad.is_empty(), "{} of {checked}:\n{}", bad.len(), bad.join("\n"));
    // short vectors and exact data stay with the language
    assert_eq!(eval(&mut e, "nl_fourier_machine(n(range(10)), 1, 1.)"), "nl_fourier_machine([1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0], 1, 1.0)");
    assert_eq!(eval(&mut e, "is_list(nl_fourier_machine(range(300), 1, 1.))"), "false");
    assert!(t0.elapsed() < Duration::from_secs(120), "took {:?}", t0.elapsed());
}

#[test]
fn long_transforms_take_a_fraction_of_a_second() {
    let mut e = mc_engine::CoreEngine::new();
    eval(&mut e, "x = constant_array(0., 2^18);");
    let t0 = Instant::now();
    assert_eq!(eval(&mut e, "y = fourier(inverse_fourier(x)); [length(y), max(abs(y))]"), "[262144, 0.0]");
    // the work per entry in the language took about 1.4 s here, the transforms themselves a tenth of that
    assert!(t0.elapsed() < Duration::from_secs(1), "took {:?}", t0.elapsed());
}
