//! Display printing as the kernel prints (`display: true`): negative arbitrary-precision numbers join sums as
//! differences, as machine numbers do (P19-06: `0.3465... + -1.5707...im` before round 20).
use mc_api::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};

fn eval(e: &mut CoreEngine, src: &str) -> String {
    match e.eval(src, &StopFlag::new()) {
        Ok(v) => v.printed,
        Err(err) => format!("error: {err}"),
    }
}

#[test]
fn negative_balls_print_as_differences() {
    let mut e = CoreEngine::with_config(EngineConfig { threads: 1, display: true, ..EngineConfig::default() });
    for (src, want) in [
        ("n(1 - pi*im, 20)", "1.00000000000000000000000 - 3.14159265358979323846264im"),
        ("n(log(-2), 10)", "0.6931471805599 + 3.14159265358979im"),
        ("n(-log(-2), 10)", "-0.6931471805599 - 3.14159265358979im"),
        ("n(y - pi*x, 10)", "y - 3.14159265358979*x"),
        ("n(x - 2*pi, 10)", "-6.2831853071796 + x"),
        ("n(-pi*x - e*y, 10)", "-3.14159265358979*x - 2.7182818284590*y"),
        ("[n(1 - 2*im, 10), n(-3 - im, 10)]", "[1.0000000000000 - 2.00000000000000im, -3.00000000000000 - 1.0000000000000im]"),
        // machine numbers, as before
        ("1.5 - 2.5*im", "1.5 - 2.5im"),
        ("x - 2.5*im", "-2.5im + x"),
    ] {
        let got = eval(&mut e, src);
        assert_eq!(got, want, "{src}");
        assert!(!got.contains("+ -"), "{src}: {got}");
    }
}

#[test]
fn a_call_that_reads_like_a_product_prints_with_its_brackets() {
    let mut e = CoreEngine::with_config(EngineConfig { threads: 1, display: true, ..EngineConfig::default() });
    for (src, want) in [
        ("x(x - a)^(1/2)", "(x(x - a))^(1/2)"),
        ("sqrt(x(x - a))", "(x(x - a))^(1/2)"),
        ("x(x - a)^2", "(x(x - a))^2"),
        ("x*(x - a)^2", "x*(x - a)^2"),
        ("f(x)^2", "f(x)^2"),
        ("y(t)^2", "y(t)^2"),
    ] {
        assert_eq!(eval(&mut e, src), want, "{src}");
    }
    // the text reads back as the same term
    assert_eq!(eval(&mut e, "head((x(x - a))^(1/2)) === head(x(x - a)^(1/2))"), "true");
    let mut plain = CoreEngine::with_config(EngineConfig { threads: 1, ..EngineConfig::default() });
    assert_eq!(eval(&mut plain, "x(x - a)^(1/2)"), "(x(x - a))^(1/2)");
}

#[test]
fn a_unit_imaginary_coefficient_prints_as_im() {
    let mut e = CoreEngine::with_config(EngineConfig { threads: 1, display: true, ..EngineConfig::default() });
    let mut plain = CoreEngine::with_config(EngineConfig { threads: 1, ..EngineConfig::default() });
    for (src, want) in [
        ("im*pi", "im*pi"),
        ("-im*pi", "-im*pi"),
        ("im*x*y", "im*x*y"),
        ("a - im*pi", "a - im*pi"),
        ("a + im*pi", "a + im*pi"),
        ("2*im*pi", "2im*pi"),
        ("refine(log(-a/2) - log(a/2), a > 0)", "im*pi"),
    ] {
        assert_eq!(eval(&mut e, src), want, "display: {src}");
    }
    // the exact form (not display) keeps the literal `1im`, the atom itself, so that it reads back unchanged
    assert_eq!(eval(&mut plain, "im*pi"), "1im*pi");
}

#[test]
fn an_unevaluated_call_of_a_variable_gets_a_note() {
    use mc_kernel::{run_each, EachEvent};
    let mut e = CoreEngine::with_config(EngineConfig { threads: 1, display: true, ..EngineConfig::default() });
    let mut events = vec![];
    let origin = mc_engine::Origin::input();
    run_each(&mut e, "integrate(sqrt(x(x - a)), x)", &origin, 1, &StopFlag::new(), Default::default(), &mut |ev| events.push(ev));
    let notes: Vec<String> = events
        .iter()
        .filter_map(|ev| match ev {
            EachEvent::Report(mc_engine::Report::Message { tag, text, .. }) if tag == "note" => Some(text.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(notes, ["x is called as a function; did you mean x*(x - a)?"], "{events:?}");
    // with the product, or for a real call: no note
    for src in ["integrate(sqrt(x*(x - a)), x)", "f(x) := x^2\nintegrate(f(x), x)", "integrate(g(x), x)"] {
        let mut events = vec![];
        run_each(&mut e, src, &origin, 1, &StopFlag::new(), Default::default(), &mut |ev| events.push(ev));
        assert!(!events.iter().any(|ev| matches!(ev, EachEvent::Report(mc_engine::Report::Message { tag, .. }) if tag == "note")), "{src}");
    }
}
