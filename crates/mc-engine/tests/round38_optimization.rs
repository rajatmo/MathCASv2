//! Round 38: global extrema of quadratics with parameters, by the complete case split on the leading coefficients.
//! Each case is checked by substituting numbers for the parameters and comparing with the extremum of the numeric
//! polynomial.
use mc_api::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};

fn ev(src: &str) -> String {
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let s = src.to_string();
    std::thread::Builder::new()
        .stack_size(256 << 20)
        .spawn(move || {
            let stop = StopFlag::new();
            let mut e = CoreEngine::with_config(EngineConfig { threads: 1, ..EngineConfig::default() });
            let out = match e.eval(&s, &stop) {
                Ok(v) => v.printed,
                Err(err) => format!("error: {err}"),
            };
            let _ = tx.send(out);
        })
        .unwrap();
    rx.recv_timeout(std::time::Duration::from_secs(120)).unwrap_or_else(|_| "TIMEOUT".to_string())
}

fn same(a: &str, b: &str) {
    assert_eq!(ev(&format!("({a}) === ({b})")), "true", "{a}: {}", ev(a));
}

#[test]
fn extrema_of_a_parametric_quadratic() {
    let q = "a*x^2 + b*x + c";
    same(&format!("max_value({q}, x)"), "piecewise([[c - b^2/(4*a), a < 0], [c, a == 0 and b == 0]], inf)");
    same(&format!("min_value({q}, x)"), "piecewise([[c - b^2/(4*a), a > 0], [c, a == 0 and b == 0]], -inf)");
    same(&format!("arg_max({q}, x)"), "piecewise([[-b/(2*a), a < 0], [0, a == 0 and b == 0]], indeterminate)");
    same(&format!("arg_min({q}, x)"), "piecewise([[-b/(2*a), a > 0], [0, a == 0 and b == 0]], indeterminate)");
    // the parametric answer at numbers agrees with the extremum of the numeric quadratic
    for (a, b, c) in [("-2", "3", "1"), ("-1/3", "-5", "7"), ("5", "-2", "1/2")] {
        let r = format!("rewrite(maximize({q}, x), [a -> {a}, b -> {b}, c -> {c}])");
        let r2 = format!("rewrite(minimize({q}, x), [a -> {a}, b -> {b}, c -> {c}])");
        let direct = |opt: &str| format!("{opt}({a}*x^2 + {b}*x + {c}, x)");
        if a.starts_with('-') {
            same(&r, &direct("maximize"));
            same(&format!("{r2}[1]"), "-inf");
        } else {
            same(&r2, &direct("minimize"));
            same(&format!("{r}[1]"), "inf");
        }
    }
    // a = b = 0: the constant c, every point a maximum
    same(&format!("rewrite(maximize({q}, x), [a -> 0, b -> 0])"), "[c, [x -> 0]]");
    same(&format!("rewrite(max_value({q}, x), [a -> 0, b -> 1])"), "inf");
}

#[test]
fn extrema_of_quadratics_with_parameters_in_lower_coefficients() {
    same("minimize(x^2 + b*x, x)", "[-b^2/4, [x -> -b/2]]");
    same("maximize(-x^2 + b*x + 1, x)", "[1 + b^2/4, [x -> b/2]]");
    same("max_value(x^2 + b*x, x)", "inf");
    same("max_value(b*x + c, x)", "piecewise([[c, b == 0]], inf)");
    // the vertex value checked by substitution of the point
    same("expand(rewrite(-3*x^2 + b*x + c, x -> arg_max(-3*x^2 + b*x + c, x)) - max_value(-3*x^2 + b*x + c, x))", "0");
}
