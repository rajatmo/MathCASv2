//! import_data / export_data round trips in every format, and their failures.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn eval(e: &mut CoreEngine, src: &str) -> String {
    match e.eval(src, &StopFlag::new()) {
        Ok(v) => v.printed,
        Err(err) => format!("error: {err}"),
    }
}

#[test]
fn round_trips() {
    let dir = std::env::temp_dir().join(format!("mc-io-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let d = dir.to_string_lossy().replace('\\', "/");
    let mut e = CoreEngine::new();
    let rows = r#"[[1, 2.5, "a b", -7], [2, 1.0e-12, "c,d", 0], [3, -0.25, "say \"hi\"", 9]]"#;
    for (ext, data) in [("csv", rows), ("tsv", rows), ("json", r#"[1, 2.5, "s", true, null, [["k" -> 1, "v" -> [1, 2]]], []]"#)] {
        let f = format!("{d}/t.{ext}");
        assert_eq!(eval(&mut e, &format!("export_data(\"{f}\", {data})")), format!("\"{f}\""), "{ext}");
        assert_eq!(eval(&mut e, &format!("import_data(\"{f}\") === {data}")), "true", "{ext}");
    }
    // whitespace tables and lines; a format given explicitly
    let f = format!("{d}/t.dat");
    eval(&mut e, &format!("export_data(\"{f}\", [[1, 2], [3, 4.5]])"));
    assert_eq!(eval(&mut e, &format!("import_data(\"{f}\")")), "[[1, 2], [3, 4.5]]");
    assert_eq!(eval(&mut e, &format!("import_data(\"{f}\", lines)")), r#"["1 2", "3 4.5"]"#);
    // a plot as PNG
    let f = format!("{d}/p.png");
    assert_eq!(eval(&mut e, &format!("export_data(\"{f}\", list_plot([1, 2, 3]))")), format!("\"{f}\""));
    assert!(eval(&mut e, &format!("import_data(\"{f}\")")).starts_with("image(\""));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn failures() {
    let mut e = CoreEngine::new();
    assert!(eval(&mut e, r#"import_data("no/such/file.csv")"#).starts_with("failure(io, "));
    assert_eq!(eval(&mut e, r#"import_data("a.xyz")"#), r#"failure(unknown_format, "a.xyz")"#);
    assert_eq!(eval(&mut e, r#"import_data(5)"#), "failure(argument, import_data, 1)");
    assert_eq!(eval(&mut e, r#"export_data("a.png", 5)"#), "failure(argument, export_data, 2)");
    assert_eq!(eval(&mut e, r#"export_data("a.csv", 5)"#), "failure(argument, export_data, 2)");
}

#[test]
fn interpolation() {
    let mut e = CoreEngine::new();
    eval(&mut e, "f = interpolation(table([x, sin(x)], x, range(0, 6.3, 0.1)))");
    assert_eq!(eval(&mut e, "abs(f(1.0) - sin(1.0)) < 1e-5"), "true");
    assert_eq!(eval(&mut e, "abs(f(pi/2) - 1) < 1e-5"), "true");
    assert!(eval(&mut e, "f(7)").starts_with("failure(out_of_range, 7, [0.0, 6.3"));
    assert_eq!(eval(&mut e, "interpolation([1, 4, 9, 16], method: linear)(2.5)"), "6.5");
    // the spline through a line is the line
    assert_eq!(eval(&mut e, "interpolation([[0, 1], [1, 3], [2.5, 6], [4, 9]])(3)"), "7.0");
    assert_eq!(eval(&mut e, "interpolation([[1, 2], [1, 3]])"), "failure(argument, interpolation, 1)");
    assert_eq!(eval(&mut e, "interpolation([5])"), "failure(argument, interpolation, 1)");
}

/// A compiled function calling an interpolation (bound to a name, or reached through n_integrate's compiled
/// integrand) gives the interpreter's value bit for bit, declines outside the data, and is invalidated when the
/// name is redefined.
#[test]
fn compiled_interpolation() {
    let mut e = CoreEngine::new();
    eval(&mut e, "S = interpolation(table([x, sin(x)], x, range(0, 6.3, 0.05)))");
    eval(&mut e, "@compiled fn sq(t: Real) = S(t)^2 + 1.0");
    assert_eq!(eval(&mut e, "sq(1.3) === S(1.3)^2 + 1.0"), "true");
    assert_eq!(eval(&mut e, "compiled_info(sq)[1]"), "params -> [Real]");
    assert!(eval(&mut e, "sq(9.0)").starts_with("failure(out_of_range, 9.0, "));
    assert_eq!(
        eval(&mut e, "n_integrate(S(x)^2, [x, 0, 6]) === n_integrate(S(x)^2, [x, 0, 6], compile: false)"),
        "true"
    );
    eval(&mut e, "S = interpolation(table([x, cos(x)], x, range(0, 6.3, 0.05)))");
    assert_eq!(eval(&mut e, "sq(1.3) === S(1.3)^2 + 1.0"), "true");
}
