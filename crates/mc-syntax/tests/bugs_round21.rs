//! Round 21: tests for the editor-warning bugs found in this round; a test that still fails is kept ignored until
//! its bug is fixed.
use mc_syntax::lint_calls;

/// A session that knows the library's functions the examples use, and nothing else.
fn known(n: &str) -> bool {
    ["d_solve", "n_dsolve", "r_solve", "recurrence_table", "free_of", "distribute", "flatten", "inactivate", "sin", "plot"].contains(&n)
}

fn warned(src: &str) -> Vec<String> {
    lint_calls(src, &known).into_iter().map(|l| l.message).collect()
}

#[test]
fn derivative_notation_is_not_a_call_of_a_variable() {
    for src in [
        "y'(x) + y(x)",
        "d_solve(y'(x) == y(x), y(x), x)",
        "d_solve(y''(x) + y(x) == 0, y(x), x)",
        "d_solve([y'(x) == z(x), z'(x) == -y(x)])",
    ] {
        assert_eq!(warned(src), Vec::<String>::new(), "{src}");
    }
}

#[test]
fn a_name_given_as_an_argument_and_called_is_not_a_variable() {
    for src in [
        "d_solve(y'(x) == 2*y(x), y, x)",
        "n_dsolve([y'(t) == -y(t), y(0) == 1], y, [t, 0, 2])",
        "r_solve([a(n) == 3*a(n - 1), a(0) == 2], a, n)",
        "recurrence_table([a(n + 1) == 3*a(n), a(1) == 7], a, [n, 1, 10])",
        "free_of(h(1), h)",
        "distribute(f(g(a, b), g(c, d, e)), g)",
    ] {
        assert_eq!(warned(src), Vec::<String>::new(), "{src}");
    }
}

#[test]
fn documented_examples_do_not_warn() {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let root = manifest.join("../../docs/functions");
    let index = std::fs::read_to_string(manifest.join("../mc-help/data/index.txt")).unwrap();
    let mut names: std::collections::HashSet<String> =
        index.lines().filter(|l| !l.starts_with('#')).filter_map(|l| l.split('\t').next()).map(str::to_string).collect();
    let mut files: Vec<_> = std::fs::read_dir(&root).unwrap().map(|e| e.unwrap().path()).collect();
    files.retain(|p| p.extension().is_some_and(|e| e == "mcn"));
    files.sort();
    names.extend(files.iter().map(|f| f.file_stem().unwrap().to_string_lossy().to_string()));
    let mut report = vec![];
    for f in &files {
        let stem = f.file_stem().unwrap().to_string_lossy().to_string();
        if stem == "mul" {
            continue; // shows the warning on purpose
        }
        let text = std::fs::read_to_string(f).unwrap();
        let mut cells: Vec<String> = vec![];
        let mut cur: Option<String> = None;
        for line in text.lines() {
            if line.starts_with("#% ") {
                cells.extend(cur.take());
                if line.starts_with("#% cell") {
                    cur = Some(String::new());
                }
            } else if let Some(c) = cur.as_mut() {
                c.push_str(line);
                c.push('\n');
            }
        }
        cells.extend(cur.take());
        // names the page itself defines (`f(x) := ...`, `f = ...`) are known to the session when a later cell runs
        let mut local = names.clone();
        for c in &cells {
            for l in c.lines() {
                let l = l.trim_start();
                let id: String = l.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
                let rest = l[id.len()..].trim_start();
                let defines = (rest.starts_with('=') && !rest.starts_with("==")) || rest.starts_with(":=") || (rest.starts_with('(') && rest.contains(":="));
                if !id.is_empty() && defines {
                    local.insert(id);
                }
            }
        }
        for c in &cells {
            let lints = lint_calls(c, &|n| local.contains(n));
            if !lints.is_empty() {
                let why: Vec<String> = lints.iter().map(|l| l.message.chars().take(120).collect()).collect();
                let cell: String = c.trim().replace('\n', " / ").chars().take(100).collect();
                report.push(format!("{stem}: {cell} ({})", why.join("; ")));
            }
        }
    }
    assert!(report.is_empty(), "{} cells warn:\n{}", report.len(), report.join("\n"));
}
