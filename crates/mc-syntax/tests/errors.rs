//! Error messages carry correct `line:column` positions; the parser recovers at the next statement; hostile nesting
//! is an error, not a stack overflow.
use mc_api::RefStore;
use mc_syntax::{parse, read, MAX_DEPTH};

/// All errors of reading `src`, rendered `line:col: message`.
fn errors(src: &str) -> Vec<String> {
    let store = RefStore::new();
    match read(&store, src) {
        Ok(_) => Vec::new(),
        Err(es) => es.iter().map(|e| e.render(src)).collect(),
    }
}

/// `src` gives exactly one error, at `pos`, whose message contains `msg`.
fn one(src: &str, pos: &str, msg: &str) -> Option<String> {
    let es = errors(src);
    match es.as_slice() {
        [e] if e.starts_with(&format!("{pos}: ")) && e.contains(msg) => None,
        _ => Some(format!("{src:?}: want one error `{pos}: ...{msg}...`, got {es:?}")),
    }
}

#[test]
fn positions_and_messages() {
    let cases = [
        ("f(1, 2", "1:7", "expected `)` to close the arguments, found end of input"),
        ("a < b < c", "1:7", "comparisons do not chain"),
        ("a b", "1:3", "names do not multiply by juxtaposition"),
        ("x = (1 + )", "1:10", "expected an expression, found `)`"),
        ("\"abc", "1:1", "unterminated string"),
        ("let = 3", "1:5", "expected a name after `let`, found `=`"),
        ("1 +\n2 *\n", "3:1", "expected an expression, found end of input"),
        ("&1 + 1", "1:1", "only allowed inside a slot function"),
        ("{ fn f(x) = x }", "1:3", "only allowed at the top level or in a module"),
        ("pub x = 1", "1:1", "`pub` is only allowed inside a module"),
        ("0x", "1:1", "expected hexadecimal digits"),
        ("1.5~", "1:4", "expected the precision"),
        ("x !", "1:3", "unexpected `!`"),
        ("\"\\q\"", "1:2", "unknown escape"),
        ("f(a: 1, 2)", "1:9", "positional arguments must come before named arguments"),
        ("xs[]", "1:4", "an index needs at least one argument"),
        ("{ let x = 1; x = 2 }", "1:14", "cannot assign to `x`"),
        ("(x) => { x = 1 }", "1:10", "cannot assign to `x`"),
        ("1e999", "1:1", "out of the range of machine reals"),
        ("a |> f(_, _)", "1:6", "only one `_` hole"),
        ("match e { 1 => 2 3 => 4 }", "1:18", "expected `,` or `}` after the match arm"),
        ("if x 1", "1:6", "expected `then` after the condition of `if`, found a number"),
        ("θ = é ‽", "1:7", "unexpected character `‽`"),
        ("αβγ + )", "1:7", "expected an expression, found `)`"),
        ("module M {\n  fn f(x) = x\n  g(\n}", "4:1", "expected an expression, found `}`"),
        ("fn f(x) x", "1:9", "expected `=` before the function body"),
        ("rule f(x_) -> 1", "1:16", "expected `=` or `:=` in the rule"),
        ("x_____", "1:1", "at most three underscores"),
        ("f(x)'", "1:5", "prime mark"),
        ("@listable x = 1", "1:11", "expected `fn`, `rule` or `sym` after decorators"),
        ("import M except", "1:16", "expected a name after `except`"),
        ("x_: ", "1:5", "expected a type"),
        ("(a, b)", "1:3", "expected `)` to close the parenthesis"),
    ];
    let bad: Vec<String> = cases.iter().filter_map(|(src, pos, msg)| one(src, pos, msg)).collect();
    assert!(bad.is_empty(), "\n{}", bad.join("\n"));
}

#[test]
fn recovery_reports_every_statement() {
    let src = "a = )\nb = 2\nc = (]\nd = 4 4\ne = 5; f = ; g = 7\n";
    let p = parse(src);
    let es: Vec<String> = p.errors.iter().map(|e| e.render(src)).collect();
    let pos: Vec<&str> = es.iter().map(|e| e.split(": ").next().unwrap_or("")).collect();
    assert_eq!(pos, ["1:5", "3:6", "4:7", "5:12"], "{es:?}");
    // The good statements survive: b, e, g.
    assert_eq!(p.program.stmts.len(), 3);
}

#[test]
fn recovery_inside_blocks() {
    let src = "{\n  x = )\n  y = 1\n}\nz = (\n";
    let es = errors(src);
    assert_eq!(es.len(), 2, "{es:?}");
    assert!(es[0].starts_with("2:7: "), "{es:?}");
    assert!(es[1].starts_with("6:1: "), "{es:?}");
}

/// Deep nesting of each kind of construct is rejected cleanly; just under the limit it parses. Run on a 1 MB stack
/// (the Windows main-thread default) to check the bound leaves room.
#[test]
fn nesting_is_bounded() {
    let run = || {
        let n = 400;
        let deep = [
            format!("{}1{}", "(".repeat(n), ")".repeat(n)),
            format!("{}1", "-".repeat(n)),
            format!("{}b", "a^".repeat(n)),
            format!("{}1{}", "[".repeat(n), "]".repeat(n)),
            format!("{}1{}", "{".repeat(n), "}".repeat(n)),
            format!("x_: {}Int{}", "List[".repeat(n), "]".repeat(n)),
            format!("{}z", "x @ ".repeat(n)),
            format!("{}a", "not ".repeat(n)),
            format!("{}x", "x => ".repeat(n)),
            format!("{}x", "if a then ".repeat(n)),
            format!("{}x", "a -> ".repeat(n)),
            format!("{}0{}", "f(".repeat(n), ")".repeat(n)),
        ];
        for src in &deep {
            let p = parse(src);
            assert!(p.errors.iter().any(|e| e.message.contains("nested more than")), "{:?}", &src[..20]);
        }
        let ok = MAX_DEPTH as usize - 2;
        let store = RefStore::new();
        for src in [
            format!("{}1{}", "(".repeat(ok), ")".repeat(ok)),
            format!("{}0{}", "f(".repeat(ok), ")".repeat(ok)),
            format!("{}1{}", "[".repeat(ok / 2), "]".repeat(ok / 2)),
            format!("{}b", "a^".repeat(ok)),
        ] {
            let ts = read(&store, &src).unwrap_or_else(|e| panic!("{e:?}"));
            let printed = mc_syntax::Printer::new(&store).program(&ts);
            assert_eq!(read(&store, &printed).unwrap(), ts);
        }
    };
    std::thread::Builder::new().stack_size(1 << 20).spawn(run).unwrap().join().unwrap();
}

/// The printer and the term store handle terms far deeper than any source text.
#[test]
fn printer_handles_deep_terms() {
    use mc_api::Store;
    let run = || {
        let store = RefStore::new();
        let f = store.sym("f");
        let mut t = store.int(0);
        for _ in 0..100_000 {
            t = store.call(f, &[t]);
        }
        let s = mc_syntax::Printer::new(&store).expr(t);
        assert_eq!(s.len(), 100_000 * 3 + 1);
    };
    std::thread::Builder::new().stack_size(1 << 20).spawn(run).unwrap().join().unwrap();
}

/// Placeholders and characters with no meaning (08-notebook.md sections 3.9 and 4.3).
#[test]
fn placeholders_and_unicode_errors() {
    let cases = [
        ("□/B", "1:1", "a placeholder `□` is not filled in"),
        ("A/□", "1:3", "a placeholder `□` is not filled in"),
        ("f(□, □)", "1:3", "a placeholder `□` is not filled in"),
        ("∀x", "1:1", "unexpected character `∀` (U+2200): it has no meaning in MathCAS syntax"),
        ("x ±", "1:3", "has no meaning"),
        ("30°", "1:3", "angles are in radians"),
    ];
    let bad: Vec<String> = cases.iter().filter_map(|&(s, p, m)| one(s, p, m)).collect();
    assert!(bad.is_empty(), "\n{}", bad.join("\n"));
    // one error per statement: the first placeholder of each
    let es = errors("□ + □\nx + □");
    assert_eq!(es, ["1:1: a placeholder `□` is not filled in: replace it with an expression", "2:5: a placeholder `□` is not filled in: replace it with an expression"]);
}
