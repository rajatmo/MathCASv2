//! Imported data feeding other functions (round 12): CSV, JSON and whitespace tables into statistics, datasets and
//! linear algebra.
use mc_bench::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn eval(e: &mut CoreEngine, src: &str) -> String {
    match e.eval(src, &StopFlag::new()) {
        Ok(v) => v.printed,
        Err(err) => format!("error: {err}"),
    }
}

/// A file of its own for one test, removed when dropped; its path as the engine reads it.
struct File(std::path::PathBuf);

impl File {
    fn new(name: &str, text: &str) -> File {
        let p = std::env::temp_dir().join(format!("mc-io-interop-{}-{name}", std::process::id()));
        std::fs::write(&p, text).unwrap();
        File(p)
    }
    fn path(&self) -> String {
        self.0.to_string_lossy().replace('\\', "/")
    }
}

impl Drop for File {
    fn drop(&mut self) {
        std::fs::remove_file(&self.0).ok();
    }
}

#[test]
fn tables_into_statistics() {
    let f = File::new("t.csv", "x,y\n1,2.5\n2,3.5\n4,8\n");
    let mut e = CoreEngine::new();
    let p = f.path();
    assert_eq!(eval(&mut e, &format!(r#"d = import_data("{p}"); mean(rest(d))"#)), "[7/3, 4.666666666666667]");
    assert_eq!(eval(&mut e, &format!(r#"ds = import_data("{p}", "dataset"); mean(ds)"#)), r#"assoc("x" -> 7/3, "y" -> 4.666666666666667)"#);
    assert_eq!(eval(&mut e, "median(ds)"), r#"assoc("x" -> 2, "y" -> 3.5)"#);
    assert_eq!(eval(&mut e, r#"variance(ds["x"])"#), "7/3");
}

#[test]
fn matrices_into_linear_algebra() {
    let j = File::new("m.json", "[[1, 2], [3, 4]]");
    let t = File::new("m.txt", "1 2\n3 5\n");
    let mut e = CoreEngine::new();
    assert_eq!(eval(&mut e, &format!(r#"det(import_data("{}"))"#, j.path())), "-2");
    assert_eq!(eval(&mut e, &format!(r#"inverse(import_data("{}"))"#, t.path())), "[[-5, 2], [3, -1]]");
}

/// G68: `interpolation_order: k`, the local polynomial through the k + 1 nearest points (the values of the
/// reference's default), and the derivatives of the spline and of the local scheme.
#[test]
fn local_interpolation_and_derivatives() {
    let mut e = CoreEngine::new();
    eval(&mut e, "w = interpolation(table([x, sin(x)*exp(-x/7)], x, range(0.0, 25.0, 0.5)), interpolation_order: 3)");
    let v: Vec<f64> = eval(&mut e, "[w(1.234), w'(1.234), d(w(x), x) /. x -> 1.234]")
        .trim_matches(['[', ']'])
        .split(", ")
        .map(|s| s.parse().unwrap())
        .collect();
    assert!((v[0] - 0.7900668714466161).abs() < 1e-15, "{v:?}");
    assert!((v[1] - 0.16341801886913404).abs() < 1e-15, "{v:?}");
    assert_eq!(v[1], v[2]);
    // the spline of a cubic has the cubic's derivatives
    eval(&mut e, "f = interpolation(table([x, x^3 - 2*x], x, range(0, 4, 0.5)))");
    assert_eq!(eval(&mut e, "chop([f'(1.5) - 4.75, f''(1.5) - 9, f'''(2.2) - 6], 1e-9)"), "[0, 0, 0]");
    // a local quadratic reproduces a quadratic; order 1 is the linear scheme
    assert_eq!(eval(&mut e, "interpolation([[0, 1], [1, 2], [2, 5], [3, 10]], interpolation_order: 2)(2.5)"), "7.25");
    assert_eq!(
        eval(&mut e, "interpolation([[0, 0], [1, 1], [2, 0]], interpolation_order: 1)"),
        eval(&mut e, "interpolation([[0, 0], [1, 1], [2, 0]], method: linear)")
    );
    assert_eq!(eval(&mut e, "interpolation([1, 2, 3], interpolation_order: 0)"), "failure(argument, interpolation, 2)");
    // a derivative inside a differential equation's right-hand side
    assert_eq!(
        eval(&mut e, "s = n_dsolve([y'(t) == f'(t), y(0) == 0], y, [t, 0, 3]); chop((y(3) /. s) - 21, 1e-6)"),
        "0"
    );
}

/// G75: `put(value, file)` and `get(file)`; a solution of `n_dsolve` comes back as an interpolation.
#[test]
fn values_saved_and_read_back() {
    let f = File::new("v.mx", "");
    let s = File::new("sol.mx", "");
    let mut e = CoreEngine::new();
    let (p, q) = (f.path(), s.path());
    assert_eq!(eval(&mut e, &format!(r#"put([1/3, 0.1 + 0.2, "a\"b", x -> 2, assoc("k" -> [1e-300])], "{p}")"#)), format!("{p:?}"));
    assert!(std::fs::read_to_string(&f.0).unwrap().starts_with("#! mathcas 2 value\n"));
    assert_eq!(eval(&mut e, &format!(r#"get("{p}") == [1/3, 0.1 + 0.2, "a\"b", x -> 2, assoc("k" -> [1e-300])]"#)), "true");
    eval(&mut e, &format!(r#"sol = n_dsolve([y'(t) == -y(t), y(0) == 1], y, [t, 0, 1]); put(sol, "{q}")"#));
    // read back in a fresh session: no solution store to refer to
    let mut e2 = CoreEngine::new();
    assert_eq!(eval(&mut e2, &format!(r#"r = get("{q}"); head(y /. first(r))"#)), "interpolated");
    let d: f64 = eval(&mut e2, "abs((y(0.5) /. r) - exp(-0.5)) + abs((y'(0.5) /. r) + exp(-0.5))").parse().unwrap();
    assert!(d < 1e-7, "{d}");
    assert_eq!(eval(&mut e2, r#"failure_tag(get("no such file here.mx"))"#), "io");
    assert_eq!(eval(&mut e2, "put(1, 2)"), "failure(argument, put, 2)");
}

/// G61: a picture written by `export_data` reads back as its term (SVG files, and PNG files with the element
/// `"graphics"`); a PNG without the element stays an image, a file without a term gives a failure.
#[test]
fn exported_graphics_read_back() {
    let svg = File::new("g.svg", "");
    let png = File::new("g.png", "");
    let txt = File::new("plain.svg", "<svg xmlns=\"http://www.w3.org/2000/svg\"></svg>");
    let mut e = CoreEngine::new();
    let (s, p) = (svg.path(), png.path());
    eval(&mut e, &format!(r#"export_data("{s}", graphics([disk([0, 0], 1)]))"#));
    assert_eq!(eval(&mut e, &format!(r#"g = import_data("{s}"); head(g)"#)), "graphics");
    assert_eq!(eval(&mut e, "g == graphics([disk([0, 0], 1)])"), "true");
    eval(&mut e, &format!(r#"export_data("{p}", graphics([circle([0, 0], 2)]))"#));
    assert_eq!(eval(&mut e, &format!(r#"head(import_data("{p}"))"#)), "image");
    assert_eq!(eval(&mut e, &format!(r#"import_data("{p}", "graphics") == graphics([circle([0, 0], 2)])"#)), "true");
    assert_eq!(eval(&mut e, &format!(r#"failure_tag(import_data("{}"))"#, txt.path())), "io");
}
