//! Timings of the editor passes on a 2,000-line cell (the notebook's long-cell probe text). Ignored by default:
//! `cargo test --release -p mc-syntax --test long_text -- --ignored --nocapture`.

fn long_text(n: usize) -> String {
    let mut src = String::new();
    for k in 0..n {
        if k > 0 {
            src.push('\n');
        }
        src.push_str(&match k % 4 {
            0 => format!("f{k}(x_) := sum(x^j / factorial(j), [j, 0, {k}])  # a comment"),
            1 => format!("    g{k} = table(sin(i * pi / {k}), [i, 1, 10])"),
            2 => format!("    s{k} = \"a string with (brackets) {k}\""),
            _ => format!("h{k} = [f{}(2), g{}, s{}]", k - 3, k - 2, k - 1),
        });
    }
    src
}

#[test]
#[ignore = "a timing"]
fn editor_passes_on_a_long_cell() {
    let src = long_text(2000);
    let known = |n: &str| n.len() < 4;
    let time = |what: &str, f: &dyn Fn()| {
        let t = std::time::Instant::now();
        for _ in 0..10 {
            f();
        }
        println!("{what}: {:.2} ms", t.elapsed().as_secs_f64() * 100.0);
    };
    println!("{} bytes", src.len());
    time("colour", &|| drop(mc_syntax::colour(&src, Some(&known))));
    time("lint", &|| drop(mc_syntax::lint_calls(&src, &known)));
    time("parse", &|| drop(mc_syntax::parse(&src)));
}
