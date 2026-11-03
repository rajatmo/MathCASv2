//! The resolver and loader on `RefStore` with in-memory files: shadowing order, forward references, exports and
//! their errors with positions, import forms, ambiguity, module files and cycles, the file header, and the reader
//! state left unchanged by an error.
use mc_api::{RefStore, Store, Sym};
use mc_load::{InMemory, LoadError, Loader, Origin};
use mc_syntax::Printer;
use std::path::PathBuf;

/// Module files used by the tests: `N` exports `h`, `k` and `cos`; `A` and `B` both export `f`.
fn files() -> InMemory {
    InMemory::new()
        .with("N.mc", "module N {\n  pub fn h(x) = x\n  pub fn k(x) = 2x\n  pub fn cos(x) = 3x\n  fn hidden(x) = x\n}")
        .with("A.mc", "module A { pub fn f(x) = 1\n pub fn g(x) = 1 }")
        .with("B.mc", "module B { pub fn f(x) = 2 }")
        .with("C1.mc", "import C2\nmodule C1 { pub fn f(x) = 1 }")
        .with("C2.mc", "import C1\nmodule C2 { pub fn f(x) = 1 }")
        .with("Odd.mc", "1 + 1")
        .with("sub/P.mc", "module P { import Q\n pub fn p(x) = q(x) }")
        .with("sub/Q.mc", "module Q { pub fn q(x) = x }")
        .with("main.mc", "#! mathcas 2\nimport N\nh(1)")
}

struct Fixture {
    store: RefStore,
    loader: Loader,
}

impl Fixture {
    fn new() -> Fixture {
        Fixture { store: RefStore::new(), loader: Loader::new(Box::new(files())) }
    }

    /// Read `src` as REPL input; the kernel knows `sin`, `cos`, `rewrite` and `range`.
    fn read(&mut self, src: &str) -> Result<Vec<String>, LoadError> {
        self.read_from(src, &Origin::repl())
    }

    fn read_from(&mut self, src: &str, origin: &Origin) -> Result<Vec<String>, LoadError> {
        let store = &self.store;
        let known = |s: Sym| matches!(store.sym_name(s).as_str(), "sin" | "cos" | "rewrite" | "range");
        let read = self.loader.read(store, src, origin, &known)?;
        let p = Printer::new(store);
        Ok(read.statements.iter().map(|&t| p.statement(t)).collect())
    }

    fn ok(&mut self, src: &str) -> Vec<String> {
        self.read(src).unwrap_or_else(|e| panic!("{src}: {}", e.render()))
    }

    /// The rendered error of `src`.
    fn err(&mut self, src: &str) -> String {
        match self.read(src) {
            Ok(v) => panic!("{src}: expected an error, got {v:?}"),
            Err(e) => e.render(),
        }
    }
}

#[test]
fn shadowing_order_inside_a_module() {
    let mut f = Fixture::new();
    let out = f.ok(concat!(
        "module M {\n",
        "  import N\n",
        "  fn h(x) = x\n",
        "  fn f(k) = k + h(1) + cos(2) + sin(z) + w + N.k(3)\n",
        "  fn g(x) = k(x) + { let w = 1; w } + ((cos) => cos)(1)\n",
        "}",
    ));
    assert_eq!(
        out,
        [
            // N's file runs first
            "fn N.h(x) = x",
            "fn N.k(x) = 2*x",
            "fn N.cos(x) = 3*x",
            "fn N.hidden(x) = x",
            "null",
            // a member shadows an import; an import shadows a known name; a known name stays bare; others are
            // fresh symbols of the module; a parameter is never renamed
            "fn M.h(x) = x",
            "fn M.f(k) = k + M.h(1) + N.cos(2) + sin(M.z) + M.w + N.k(3)",
            // a block local and a lambda parameter shadow the members and imports
            "fn M.g(x) = N.k(x) + { let w = 1; w } + (cos => cos)(1)",
            "null",
        ]
    );
}

#[test]
fn shadowing_order_at_the_top_level() {
    let mut f = Fixture::new();
    let out = f.ok("import N\ncos(1) + sin(1) + h(x)\n(h => h(1))(cos)\nfn t(k) = k + cos(k)");
    assert_eq!(out[5..], ["null", "N.cos(1) + sin(1) + N.h(x)", "(h => h(1))(N.cos)", "fn t(k) = k + N.cos(k)"]);
}

#[test]
fn forward_references_and_definition_forms() {
    let mut f = Fixture::new();
    let out = f.ok(concat!(
        "module M {\n",
        "  pub fn a(x) = b(x) + c + d\n",
        "  rule b(y_) := y + e\n",
        "  c = 1\n",
        "  d := 2\n",
        "  let e = 3\n",
        "  @commutative sym op\n",
        "  pub rule op(u_, v_) := u\n",
        "}\nM.a(1)",
    ));
    assert_eq!(
        out,
        [
            "fn M.a(x) = M.b(x) + M.c + M.d",
            "rule M.b(y_) := y + M.e",
            "M.c = 1",
            "M.d := 2",
            "let M.e = 3",
            "@commutative sym M.op",
            "rule M.op(u_, v_) := u",
            "null",
            "M.a(1)",
        ]
    );
    let m = f.loader.module("M").unwrap();
    assert_eq!(m.members.iter().map(String::as_str).collect::<Vec<_>>(), ["a", "b", "c", "d", "e", "op"]);
    assert_eq!(m.public.iter().map(String::as_str).collect::<Vec<_>>(), ["a", "op"]);
}

#[test]
fn locals_patterns_types_options_and_failure_tags_are_not_renamed() {
    let mut f = Fixture::new();
    let out = f.ok(concat!(
        "module M {\n",
        "  fn f(x: Int, n: Int = 2; scale: Int = 3) = failure(bad_input, x*n*scale*y)\n",
        "  fn g(xs) = match xs { [h_, t___] => [h, t], _ => rewrite(xs, z_ :> z + 1) }\n",
        "  fn s(n) = { var a = 0; for i in range(n) { a = a + i }; a + plot(n, colour: red) }\n",
        "  fn l() = &(&1 + &2)\n",
        "}",
    ));
    assert_eq!(out[0], "fn M.f(x: Int, n: Int = 2; scale: Int = 3) = failure(bad_input, x*n*scale*M.y)");
    assert_eq!(out[1], "fn M.g(xs) = match xs { [h_, t___] => [h, t], _ => rewrite(xs, z_ :> z + 1) }");
    assert!(out[2].contains("for i in range(n)") && out[2].contains("M.plot(n, colour: M.red)"), "{}", out[2]);
    assert_eq!(out[3], "fn M.l() = &(&1 + &2)");
}

#[test]
fn exports_and_their_errors_with_positions() {
    let mut f = Fixture::new();
    f.ok("module M {\n  pub fn a(x) = M.b(x)\n  fn b(x) = x\n}");
    assert_eq!(f.ok("M.a(2)"), ["M.a(2)"]);
    let e = f.err("1 + 1\nf(x) + M.b(2)");
    assert_eq!(e, "<repl>:2:8: `M.b` is private to module M (mark it `pub` to export it)\n  f(x) + M.b(2)\n         ^");
    let e = f.err("M.nope");
    assert!(e.starts_with("<repl>:1:1: module M has no member `nope`"), "{e}");
    let e = f.err("x + Zed.f(1)");
    assert!(e.starts_with("<repl>:1:5: unknown module `Zed` in `Zed.f`; import it first"), "{e}");
    let e = f.err("module K {\n  fn a() = K.zz\n}");
    assert!(e.starts_with("<repl>:2:12: module K has no member `zz`"), "{e}");
    // lowering already rejects `pub` outside a module
    let e = f.err("pub fn top(x) = x");
    assert!(e.starts_with("<repl>:1:1: `pub` is only allowed inside a module"), "{e}");
    let e = f.err("module K {\n  fn a() = 1\n  pub 1 + 2\n}");
    assert!(e.starts_with("<repl>:3:3: `pub` marks a definition"), "{e}");
}

#[test]
fn import_forms() {
    let mut f = Fixture::new();
    assert_eq!(f.ok("import N.{h}\nh(1) + k(1)")[5..], ["null", "N.h(1) + k(1)"]);
    let mut f = Fixture::new();
    assert_eq!(f.ok("import N except h\nh(1) + k(1)")[5..], ["null", "h(1) + N.k(1)"]);
    let mut f = Fixture::new();
    let e = f.err("import N.{h, hidden}");
    assert!(e.starts_with("<repl>:1:14: `hidden` is private to module N"), "{e}");
    let e = f.err("import N except nothere");
    assert!(e.starts_with("<repl>:1:17: module N has no member `nothere`"), "{e}");
    // a module file is read once: the second import finds the module in the table
    let out = f.ok("import N\nimport N.{k}");
    assert_eq!((out.len(), &out[5..]), (7, &["null".to_string(), "null".to_string()][..]));
    assert_eq!(f.ok("import N"), ["null"]);
}

#[test]
fn session_imports_persist_and_errors_change_nothing() {
    let mut f = Fixture::new();
    f.ok("import N.{h}");
    assert_eq!(f.ok("h(1)"), ["N.h(1)"]);
    // an error after a module definition and an import: neither takes effect
    let e = f.err("module Z { pub fn z(x) = x }\nimport A\nM.q");
    assert!(e.starts_with("<repl>:3:1: unknown module `M`"), "{e}");
    assert!(f.loader.module("Z").is_none() && f.loader.module("A").is_none());
    assert_eq!(f.ok("z(1) + f(1) + h(2)"), ["z(1) + f(1) + N.h(2)"]);
}

#[test]
fn ambiguity_is_an_error_only_where_the_name_is_used() {
    let mut f = Fixture::new();
    let out = f.ok("import A\nimport B\ng(1)");
    assert_eq!(out.last().unwrap(), "A.g(1)");
    let e = f.err("x\ny + f(1)");
    assert_eq!(e, "<repl>:2:5: `f` is ambiguous: it is imported from modules A and B\n  y + f(1)\n      ^");
    // a local binding of the name removes the ambiguity
    assert_eq!(f.ok("(f => f)(1)"), ["(f => f)(1)"]);
}

#[test]
fn module_files_cycles_and_lookup() {
    let mut f = Fixture::new();
    let e = f.err("import C1");
    // the cycle is found in C2's file, at its import of C1
    assert_eq!(e, "C2.mc:1:8: import cycle: C1 -> C2 -> C1\n  import C1\n         ^");
    assert!(f.loader.module("C1").is_none() && f.loader.module("C2").is_none());
    let e = f.err("import Odd");
    assert!(e.starts_with("<repl>:1:8: Odd.mc does not define module `Odd`"), "{e}");
    let e = f.err("\nimport Missing");
    assert!(e.starts_with("<repl>:2:8: no module `Missing`: no file Missing.mc in the current directory"), "{e}");
    let e = f.err("import a.b");
    assert!(e.starts_with("<repl>:1:8: nested modules are not supported yet: `a.b`"), "{e}");
    let e = f.err("module X { module Y { } }");
    assert!(e.starts_with("<repl>:1:12: nested modules are not supported yet"), "{e}");
    // module files are found next to the importing file: sub/P.mc imports sub/Q.mc
    let origin = Origin::new("sub/main.mc", Some(PathBuf::from("sub")));
    let out = f.read_from("import P\np(1)", &origin).unwrap();
    assert_eq!(out, ["fn Q.q(x) = x", "null", "fn P.p(x) = Q.q(x)", "null", "null", "P.p(1)"]);
    // and only once per session
    assert_eq!(f.read_from("import Q\nq(2)", &origin).unwrap(), ["null", "Q.q(2)"]);
}

#[test]
fn the_standard_library_is_embedded() {
    let mut f = Fixture::new();
    let out = f.ok("import stats.{mean}\nmean([1, 2])");
    assert_eq!(out.last().unwrap(), "stats.mean([1, 2])");
    assert!(out.iter().any(|s| s.starts_with("fn stats.median(xs)")), "{out:?}");
    assert_eq!(f.loader.module("stats").unwrap().origin, "lib/stats.mc");
    assert!(!f.loader.module("stats").unwrap().public.contains("squared_deviations"));
}

#[test]
fn headers() {
    let mut f = Fixture::new();
    assert_eq!(f.ok("#! mathcas 2\n1"), ["1"]);
    assert_eq!(f.ok("#!mathcas   2\r\n1"), ["1"]);
    let e = f.err("#! python3\n1");
    assert_eq!(e, "<repl>:1:1: unknown file header `#! python3`: a MathCAS v2 file starts with `#! mathcas 2`\n  #! python3\n  ^");
    // a `#!` line further down is an ordinary comment
    assert_eq!(f.ok("1\n#! anything"), ["1"]);
}

#[test]
fn files_and_syntax_errors() {
    let mut f = Fixture::new();
    let store = &f.store;
    let known = |_: Sym| false;
    let read = f.loader.read_file(store, std::path::Path::new("main.mc"), &known).unwrap();
    assert_eq!(read.statements.len(), 7);
    let e = f.loader.read_file(store, std::path::Path::new("none.mc"), &known).unwrap_err();
    assert!(e.render().starts_with("none.mc: cannot read the file"), "{}", e.render());
    let e = f.err("1 +\n(2");
    assert!(e.starts_with("<repl>:2:3:"), "{e}");
}

#[test]
fn redefining_a_module_lists_its_old_members_to_clear() {
    let mut f = Fixture::new();
    let store = &f.store;
    let known = |_: Sym| false;
    let src = "module M { pub fn a(x) = x\n fn b(x) = x }";
    assert!(f.loader.read(store, src, &Origin::repl(), &known).unwrap().reloaded.is_empty());
    let again = f.loader.read(store, "module M { pub fn a(x) = 2x }", &Origin::repl(), &known).unwrap();
    assert_eq!(again.reloaded, ["M.a", "M.b"]);
    assert!(!f.loader.module("M").unwrap().members.contains("b"));
}

#[test]
fn errors_inside_module_files_name_the_file() {
    let mut f = Fixture { store: RefStore::new(), loader: Loader::new(Box::new(files().with("Bad.mc", "module Bad {\n  pub fn f(x) = N.hidden(x)\n}"))) };
    let e = f.read("import N\nimport Bad").unwrap_err();
    assert!(!e.in_input);
    assert_eq!(e.render(), "Bad.mc:2:17: `N.hidden` is private to module N (mark it `pub` to export it)\n    pub fn f(x) = N.hidden(x)\n                  ^");
}
