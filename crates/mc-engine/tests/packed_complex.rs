//! Packed arrays of machine complex numbers through the engine (gap G48, decisions sp-core.1 to sp-core.3): every
//! element of an element-wise result computed on the payload is bitwise the value the function gives for that
//! element alone (`same`, structural identity, so signed zeros and every bit count).
use mc_api::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};

fn eval(e: &mut CoreEngine, src: &str) -> String {
    match e.eval(src, &StopFlag::new()) {
        Ok(v) => v.printed,
        Err(err) => format!("error: {err}"),
    }
}

/// An engine with `xs` (640 machine reals of mixed signs and magnitudes, with zeros of both signs), `zs` (640
/// machine complex numbers made one by one) and the comparison functions.
fn engine() -> CoreEngine {
    let mut e = CoreEngine::with_config(EngineConfig { threads: 2, ..EngineConfig::default() });
    for src in [
        "xs = join(table(sin(1.7*k)*10.0^(mod(k, 7) - 3), k, range(1, 636)), [0.0, -0.0, 2.5, -2.5])",
        "zs = map_thread(complex, [xs, reverse(xs)])",
        "fn chunks(n) = map(k => [k, min(k + 49, n)], range(1, n, 50))",
        "fn agree1(f, u) = count(map(c => same(take(f(u), c), map(f, take(u, c))), chunks(length(u))), false) == 0",
        "fn agree2(f, u, v) = count(map(c => same(take(f(u, v), c), map_thread(f, [take(u, c), take(v, c)])), chunks(length(u))), false) == 0",
    ] {
        let r = eval(&mut e, src);
        assert!(!r.starts_with("error") && !r.contains("failure"), "{src}: {r}");
    }
    e
}

#[test]
fn complex_lists_are_lists_of_complex_numbers() {
    let mut e = engine();
    for (src, want) in [
        ("length(zs)", "640"),
        ("zs[3] == complex(xs[3], xs[638])", "true"),
        ("same(zs, map_thread(complex, [xs, reverse(xs)]))", "true"),
        ("take(zs, 2) == [complex(xs[1], xs[640]), complex(xs[2], xs[639])]", "true"),
        ("same(drop(zs, 600), take(zs, -40))", "true"),
        ("same(take(zs, [101, 300]), map(k => zs[k], range(101, 300)))", "true"),
        ("re(take(zs, 1)) == take(xs, 1)", "true"),
    ] {
        assert_eq!(eval(&mut e, src), want, "{src}");
    }
}

#[test]
fn element_wise_functions_agree_with_the_scalar_values() {
    let mut e = engine();
    let mut bad = vec![];
    for f in [
        "re", "im_part", "conjugate", "arg", "abs", "exp", "log", "sin", "cos", "tan", "sinh", "cosh", "tanh", "atan",
        "asin", "acos", "log1p", "expm1", "sqrt",
    ] {
        for u in ["zs", "xs"] {
            let src = format!("agree1(x => {f}(x), {u})");
            if eval(&mut e, &src) != "true" {
                bad.push(src);
            }
        }
    }
    assert!(bad.is_empty(), "{bad:?}");
}

#[test]
fn element_wise_arithmetic_agrees_with_the_scalar_values() {
    let mut e = engine();
    let mut bad = vec![];
    for (f, u, v) in [
        ("(a, b) => a*b", "zs", "zs"),
        ("(a, b) => a + b", "zs", "reverse(zs)"),
        ("(a, b) => a*b", "zs", "xs"),
        ("(a, b) => a + b", "xs", "zs"),
        ("(a, b) => a - b", "zs", "xs"),
        ("(a, b) => im*a + b", "xs", "xs"),
        ("(a, b) => (2 - 3*im)*a + 0.5*b", "zs", "zs"),
        ("(a, b) => im*0.06*a*b", "xs", "zs"),
        ("(a, b) => a + b + 1.5 + im", "zs", "xs"),
        ("(a, b) => 3*a*b*b", "zs", "zs"),
        ("(a, b) => a*b + 2", "zs", "zs"),
        ("(a, b) => exp(im*(0.06*a + b))", "xs", "xs"),
    ] {
        let src = format!("agree2({f}, {u}, {v})");
        if eval(&mut e, &src) != "true" {
            bad.push(src);
        }
    }
    assert!(bad.is_empty(), "{bad:?}");
}

#[test]
fn differences_of_complex_lists() {
    let mut e = engine();
    assert_eq!(eval(&mut e, "same(differences(zs), map(k => zs[k + 1] - zs[k], range(1, 639)))"), "true");
}

#[test]
fn square_roots_of_machine_reals_are_powers() {
    let mut e = engine();
    for src in [
        "max(abs(map(x => sqrt(x), take(abs(xs), 50)) - map(x => x^(1/2), take(abs(xs), 50)))) <= 1.0e-12",
        "max(abs(sqrt(abs(xs)) - map(x => x^(1/2), abs(xs)))) <= 1.0e-12",
        "sqrt(-4.0) == complex(0.0, 2.0)",
        "sqrt(4) == 2",
    ] {
        assert_eq!(eval(&mut e, src), "true", "{src}");
    }
}
