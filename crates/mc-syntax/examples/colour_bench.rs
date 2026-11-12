//! Timing of the editor helpers on cells of growing size: `cargo run --release -p mc-syntax --example colour_bench`.
use std::time::Instant;

fn cell(lines: usize) -> String {
    let mut s = String::new();
    for k in 0..lines {
        s.push_str(&match k % 4 {
            0 => format!("fn f{k}(x, y: Real) = {{ let z = x*y + g(t => t^2, [x, y])  # comment\n"),
            1 => "  z + h(a_ -> a, digits: 20) }\n".to_string(),
            2 => format!("rule r{k}(n_: Int) := r{k}(n - 1) + \"text\" + 1.5im\n"),
            _ => "match q { [h_, t__] => h + t, _ => for i in range(3) { i } }\n".to_string(),
        });
    }
    s
}

fn time<R>(reps: usize, f: impl Fn() -> R) -> f64 {
    let t = Instant::now();
    for _ in 0..reps {
        std::hint::black_box(f());
    }
    t.elapsed().as_secs_f64() * 1e3 / reps as f64
}

fn main() {
    println!("{:>8} {:>9} {:>11} {:>11} {:>11} {:>11}", "lines", "bytes", "colour ms", "lexical ms", "scope ms", "select ms");
    for lines in [10, 100, 1_000, 2_000, 10_000] {
        let src = cell(lines);
        let reps = (20_000 / lines).clamp(3, 200);
        let full = time(reps, || mc_syntax::colour(&src, Some(&|n: &str| n.len() > 2)));
        let mid = src.len() / 2;
        let scope = time(reps, || mc_syntax::names_in_scope(&src, mid));
        let select = time(reps, || mc_syntax::extend_selection(&src, mid, mid));
        let lexical = if src.len() > mc_syntax::LEXICAL_ONLY_ABOVE { format!("{full:.3}") } else { "-".into() };
        println!("{lines:>8} {:>9} {full:>11.3} {lexical:>11} {scope:>11.3} {select:>11.3}", src.len());
    }
}
