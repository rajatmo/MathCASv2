//! The prime notation round-trips: derivatives print with prime marks (orders 1 to 4 of a named function) and the
//! printed text reads back as the same term.
mod common;
use common::Sys;

#[test]
fn printed_derivatives_read_back() {
    let mut s = Sys::new();
    let cases = [
        ("d(f(x), x)", "f'(x)"),
        ("d(f(x), [x, 2])", "f''(x)"),
        ("d(f(x), [x, 3])", "f'''(x)"),
        ("d(f(x), [x, 4])", "f''''(x)"),
        ("d(f(x), [x, 5])", "derivative(5)(f)(x)"),
        ("d(y(t), t, t)", "y''(t)"),
        ("d(f(g(x)), x)", "f'(g(x))*g'(x)"),
        ("d(f(g(x)), [x, 2])", "g'(x)^2*f''(g(x)) + f'(g(x))*g''(x)"),
        ("d(f(x, y), x)", "derivative(1, 0)(f)(x, y)"),
        ("d(f(x)^2, x)", "2*f(x)*f'(x)"),
        ("d(exp(f(x)), x)", "exp(f(x))*f'(x)"),
    ];
    for (src, printed) in cases {
        assert_eq!(s.run(src), printed, "{src}");
        // the printed text is the same term
        assert_eq!(s.run(&format!("({printed}) === {src}")), "true", "{printed}");
        assert_eq!(s.run(printed), printed, "{printed} read back");
    }
}

#[test]
fn the_operator_alone() {
    let mut s = Sys::new();
    for n in 1..=6 {
        let marks = "'".repeat(n);
        let op = format!("derivative({n})(f)");
        let shown = if n <= 4 { format!("f{marks}") } else { op.clone() };
        assert_eq!(s.run(&op), shown);
        assert_eq!(s.run(&format!("{shown} === {op}")), "true");
        assert_eq!(s.run(&format!("f{marks}(t) === {op}(t)")), "true");
    }
}

#[test]
fn known_functions_resolve_outside_d() {
    let mut s = Sys::new();
    for (src, want) in [
        ("sin'(x)", "cos(x)"),
        ("sin''(x)", "-sin(x)"),
        ("exp'(2*y)", "exp(2*y)"),
        ("atan'(x)", "(1 + x^2)^-1"),
        ("derivative(1)(log)(3)", "1/3"),
        ("d(sin'(x), x)", "-sin(x)"),
        // unknown functions, the operator alone and a wrong number of arguments stay as they are
        ("f'(x)", "f'(x)"),
        ("sin'", "sin'"),
        ("derivative(1)", "derivative(1)"),
        ("derivative(1, 0)(sin)(x)", "derivative(1, 0)(sin)(x)"),
    ] {
        assert_eq!(s.run(src), want, "{src}");
    }
}
