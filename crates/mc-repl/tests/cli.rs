//! The `mc-repl` command line on the built binary: the example scripts of `examples/` (output and exit codes),
//! `-e`, `-i`, `--no-prelude`, `--help`, and the exit codes of read errors and failures.
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

/// The repository root (the examples' paths are relative to it).
fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Run `mc-repl` in the repository root with these arguments and this standard input.
fn mc(args: &[&str], stdin: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_mc-repl"))
        .args(args)
        .current_dir(root())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("mc-repl starts");
    child.stdin.take().expect("stdin").write_all(stdin.as_bytes()).expect("write stdin");
    child.wait_with_output().expect("mc-repl ends")
}

fn text(b: &[u8]) -> String {
    String::from_utf8_lossy(b).replace("\r\n", "\n")
}

/// Exit code, standard output, standard error.
fn run(args: &[&str]) -> (i32, String, String) {
    let o = mc(args, "");
    (o.status.code().unwrap_or(-1), text(&o.stdout), text(&o.stderr))
}

#[test]
fn the_hello_example() {
    let (code, out, err) = run(&["examples/hello.mc"]);
    assert_eq!((code, err.as_str()), (0, ""));
    assert_eq!(
        out,
        "Exact arithmetic: 1/2\nA big number: 2^100 = 1267650600228229401496703205376\n\
         The Collatz path of 6: [6, 3, 10, 5, 16, 8, 4, 2, 1]\nRunning sums: [1, 3, 6, 10, 15]\n\
         Odd numbers below 20: 10\n[3, 1, 4, 5, 9, 2, 6]\n"
    );
}

#[test]
fn the_modules_example_imports_the_file_next_to_it() {
    let (code, out, err) = run(&["examples/modules.mc"]);
    assert_eq!((code, err.as_str()), (0, ""));
    assert_eq!(
        out,
        "e1 + 2 e2 = [1, 2]\nThe angle between e1 and [1, 1]: (1/4)*pi\nAlso qualified: [3, 6]\n\
         Mean and median: [109/5, 22]\n[5883/20, 5843/20, 5943/20, 5903/20, 5923/20]\n"
    );
}

#[test]
fn files_and_sources_run_in_order_in_one_session() {
    let (code, out, _) = run(&["-e", "k = 20", "examples/vectors.mc", "-e", "vectors.times(k, vectors.e1)"]);
    assert_eq!((code, out.as_str()), (0, "[20, 0]\n"));
    // a module file run as a script prints nothing: its value is null
    let (code, out, _) = run(&["examples/vectors.mc"]);
    assert_eq!((code, out.as_str()), (0, ""));
}

#[test]
fn failures_are_reported_and_the_script_goes_on() {
    let (code, out, err) = run(&["-e", "print(1)\n1/0\nprint(2)\nfailure(oops)\n3", "-e", "4"]);
    assert_eq!(code, 2);
    assert_eq!(out, "1\n2\n3\n4\n");
    assert_eq!(err, "<-e>: failure(division_by_zero)\n<-e>: failure(oops)\n");
    // a failing last statement is shown once, as a failure
    let (code, out, err) = run(&["-e", "1/0"]);
    assert_eq!((code, out.as_str(), err.as_str()), (2, "", "<-e>: failure(division_by_zero)\n"));
}

#[test]
fn read_errors_stop_everything() {
    let (code, out, err) = run(&["-e", "print(1); null", "-e", "print(2)\n1 +", "-e", "print(3)"]);
    assert_eq!(code, 1);
    assert_eq!(out, "1\n");
    assert!(err.starts_with("<-e>:2:4: "), "{err}");
    assert!(err.ends_with("\n  1 +\n     ^\n"), "{err}");
    let (code, _, err) = run(&["-e", "import vectors"]);
    assert_eq!(code, 1);
    assert!(err.starts_with("<-e>:1:8: no module `vectors`: no file vectors.mc in the current directory"), "{err}");
    let (code, _, err) = run(&["examples/none.mc"]);
    assert_eq!(code, 1);
    assert!(err.starts_with("examples/none.mc: cannot read the file"), "{err}");
    let (code, _, err) = run(&["-e", "#! python\n1"]);
    assert_eq!(code, 1);
    assert!(err.starts_with("<-e>:1:1: unknown file header `#! python`"), "{err}");
    let (code, _, err) = run(&["--bogus"]);
    assert_eq!(code, 1);
    assert!(err.starts_with("mc-repl: unknown option --bogus\nusage:"), "{err}");
}

#[test]
fn options() {
    let (code, out, _) = run(&["--help"]);
    assert_eq!(code, 0);
    assert!(out.starts_with("usage: mc-repl [options] [file.mc ...]"), "{out}");
    let (code, out, _) = run(&["--no-prelude", "-e", "unique([1, 1])"]);
    assert_eq!((code, out.as_str()), (0, "unique([1, 1])\n"));
    // -i: the REPL starts afterwards, in the same session
    let o = mc(&["-i", "examples/vectors.mc"], ":modules\nvectors.plus(vectors.e1, vectors.e2)\n");
    assert_eq!(o.status.code(), Some(0));
    let out = text(&o.stdout);
    assert!(out.contains("mc> vectors: angle, e1, e2, plus, times\nmc> [1, 1]\nmc> \n"), "{out}");
    // with nothing to run, the REPL starts
    let o = mc(&[], "1 + 1\n");
    assert!(text(&o.stdout).contains("mc> 2\n"));
}
