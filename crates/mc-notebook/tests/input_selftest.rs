//! The math input experience in the window (round 32), driven through the self-test scripts
//! `tests/selftest/input-*.txt`: a corpus of everyday formulas typed through the keyboard (each checked for its box
//! tree, the linear text it lowers to, its value, and its output copied back and pasted as math), and scripts for
//! the caret, selection, deletion, undo and redo inside the boxes. Each script opens a window, so the tests are
//! ignored by default; run one at a time:
//! `cargo test -p mc-notebook --test input_selftest -- --ignored --test-threads 1 corpus1`.
//!
//! `MCNB_SELFTEST_OUT=<dir>` keeps the log, the screenshots and the dumped notebook in `<dir>/<script>/`.
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::Command;

/// Formulas whose failure is a known gap (label, why); everything else must pass.
const KNOWN: &[(&str, &str)] = &[
];

/// Run a script; returns the log.
fn run(name: &str) -> String {
    let keep = std::env::var_os("MCNB_SELFTEST_OUT").map(PathBuf::from);
    let dir = match &keep {
        Some(d) => d.join(name),
        None => std::env::temp_dir().join(format!("mcnb-input-{name}-{}", std::process::id())),
    };
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("config")).unwrap();
    let script = dir.join(format!("{name}.txt"));
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("selftest").join(format!("{name}.txt"));
    std::fs::copy(&src, &script).unwrap();
    let status = Command::new(env!("CARGO_BIN_EXE_mc-notebook"))
        .env("MCNB_SELFTEST", &script)
        .env("MCNB_CONFIG_DIR", dir.join("config"))
        .env("MCUI_CLIPBOARD", "local")
        .env("MATHCAS_DOCS", "-")
        .status()
        .expect("start the notebook");
    assert!(status.success(), "{status}");
    let log = std::fs::read_to_string(dir.join("selftest.log")).expect("the self-test log");
    if keep.is_none() {
        let _ = std::fs::remove_dir_all(&dir);
    }
    log
}

/// The pass table of a log: per category (the label up to `/`), passed and total; and the failing labels.
fn table(log: &str) -> (String, Vec<String>) {
    let mut cats: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut fails = vec![];
    for l in log.lines() {
        let Some(rest) = l.strip_prefix("result ") else { continue };
        let Some((label, verdict)) = rest.split_once(": ") else { continue };
        let cat = label.split('/').next().unwrap_or(label).to_string();
        let e = cats.entry(cat).or_default();
        e.1 += 1;
        if verdict == "pass" {
            e.0 += 1;
        } else {
            fails.push(format!("{label}: {verdict}"));
        }
    }
    let (p, t) = cats.values().fold((0, 0), |(a, b), (x, y)| (a + x, b + y));
    let mut s = String::new();
    for (c, (pass, total)) in &cats {
        s.push_str(&format!("{c:<12} {pass:>3}/{total:<3}\n"));
    }
    s.push_str(&format!("{:<12} {p:>3}/{t:<3}\n", "all"));
    (s, fails)
}

fn check(name: &str) {
    check_log(name, &run(name));
}

/// The checks of [`check`] on a log.
fn check_log(name: &str, log: &str) {
    let (t, fails) = table(log);
    println!("{name}\n{t}");
    let unexpected: Vec<&String> =
        fails.iter().filter(|f| !KNOWN.iter().any(|(k, _)| f.starts_with(&format!("{k}:")))).collect();
    assert!(unexpected.is_empty(), "{name}: unexpected failures {unexpected:#?}\n{t}\nlog:\n{log}");
    assert!(!log.contains("unknown step"), "{log}");
    assert!(!log.contains("keys: "), "a key script did not read:\n{log}");
}

#[test]
#[ignore = "opens a window"]
fn corpus1() {
    check("input-corpus1");
}

#[test]
#[ignore = "opens a window"]
fn corpus2() {
    check("input-corpus2");
}

#[test]
#[ignore = "opens a window"]
fn caret() {
    check("input-caret");
}

/// Round 33: powers of compound bases, brackets that do not match yet, bracket pairs.
#[test]
#[ignore = "opens a window"]
fn brackets() {
    check("input-brackets");
}

/// Round 35: a space between operands multiplies; the faint multiplication signs, the `sin x` warning and its fix,
/// `Esc * Esc`, and screenshots of the signs (looked at by eye).
#[test]
#[ignore = "opens a window"]
fn space() {
    let log = run("input-space");
    check_log("input-space", &log);
    let warned = log.lines().filter(|l| l.starts_with("warnings: ")).collect::<Vec<_>>();
    assert_eq!(warned.len(), 2, "{log}");
    assert!(warned[0].contains("sin x is the product sin*x; for the function write sin(x)"), "{log}");
    assert!(warned[1].starts_with("warnings: 0 "), "{log}");
    assert!(log.contains("fixwarning: true"), "{log}");
    // every gap has its mark
    for l in log.lines().filter(|l| l.starts_with("products: ")) {
        let n: Vec<&str> = l["products: ".len()..].split_whitespace().collect();
        assert_eq!(n[0], n[2], "a gap without its mark: {l}");
    }
    for s in ["space-light-100", "space-dark-100", "space-dark-150", "space-light-150", "space-light-200", "space-dark-200"] {
        assert!(log.contains(&format!("shot {s}")), "{s}\n{log}");
    }
}

#[test]
#[ignore = "opens a window"]
fn shots() {
    let log = run("input-shots");
    for s in ["s1-light-100", "s2-light-100-selected", "s3-dark-100", "s4-dark-150", "s5-light-150", "s6-light-200", "s7-dark-200", "s8-insert-menu", "s9-greek-menu"] {
        assert!(log.contains(&format!("shot {s}")), "{s}\n{log}");
    }
}
