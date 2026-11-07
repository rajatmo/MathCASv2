//! Colouring classes (08-notebook.md section 3.11), bracket matching (3.12) and names in scope (3.10): snapshot
//! tests of the class of every token for a corpus of cells, incomplete input, and a linear-time check.
use mc_syntax::{classify, colour, names_in_scope, LocalKind, TokenClass};

/// Every coloured token as `text/class`, separated by spaces.
fn snap(src: &str) -> String {
    render(src, &classify(src))
}

fn render(src: &str, toks: &[(std::ops::Range<usize>, TokenClass)]) -> String {
    toks.iter().map(|(r, c)| format!("{}/{}", &src[r.clone()], c.name())).collect::<Vec<_>>().join(" ")
}

/// The classes of the names only (`name/class`), for the scope tests.
fn names(src: &str) -> String {
    names_with(src, None)
}

fn names_with(src: &str, defined: Option<&(dyn Fn(&str) -> bool + Sync)>) -> String {
    let c = colour(src, defined);
    c.tokens
        .iter()
        .filter(|(r, c)| {
            matches!(
                c,
                TokenClass::Global
                    | TokenClass::Undefined
                    | TokenClass::Local
                    | TokenClass::PatternVar
                    | TokenClass::Shadowing
            ) && !src[r.clone()].starts_with('&')
        })
        .map(|(r, c)| format!("{}/{}", &src[r.clone()], c.name()))
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
fn lexical_classes() {
    assert_eq!(
        snap("x = 2.5im + \"s\" # note\nif true then null else 1~40"),
        "x/global =/op 2.5im/number +/op \"s\"/string # note/comment if/keyword true/constant then/keyword \
         null/constant else/keyword 1~40/number"
    );
    assert_eq!(snap("f'(x) |> g"), "f/global '/op (/op x/global )/op |>/op g/global");
    assert_eq!(snap("a ≤ b ∧ ¬c"), "a/global ≤/op b/global ∧/keyword ¬/keyword c/global");
    // raw strings are one string token, brackets inside do not count
    assert_eq!(
        snap(r##"f(r"$\alpha($", r#"("x")"#) + r"##),
        r##"f/global (/op r"$\alpha($"/string ,/op r#"("x")"#/string )/op +/op r/global"##
    );
}

#[test]
fn an_open_raw_string_at_the_end_is_incomplete() {
    let c = colour(r#"plot(x, x_label: r"$\alpha"#, None);
    assert!(c.incomplete);
}

#[test]
fn scopes() {
    let cases = [
        // let and var: to the end of the block, not in their own value
        ("{ let x = x + 1; x }", "x/local x/global x/local"),
        ("{ var i = 0\n  while i < 3 { i = i + 1 }\n  i }", "i/local i/local i/local i/local i/local"),
        ("{ let y = 1 }; y", "y/local y/global"),
        // lambdas
        ("map(x => x^2, xs)", "map/global x/local x/local xs/global"),
        ("(a, b: Real) => a*b + c", "a/local b/local Real/global a/local b/local c/global"),
        ("f = x => y => x + y", "f/global x/local y/local x/local y/local"),
        ("g(k => k, k)", "g/global k/local k/local k/global"),
        // fn parameters, defaults, conditions, types and options
        (
            "fn f(x: Int, n = 10; tol: Real = 1e-9) when x > 0 = x*n*tol*y",
            "f/global x/local Int/global n/local tol/local Real/global x/local x/local n/local tol/local y/global",
        ),
        ("fn fact(0) = 1", "fact/global"),
        ("fn g(rest...) = len(rest)", "g/global rest/local len/global rest/local"),
        // patterns
        ("f(x_) := x^2 + y", "f/global x_/pattern x/pattern y/global"),
        ("rule fib(n_: Int) := fib(n - 1)", "fib/global n_/pattern Int/global fib/global n/pattern"),
        ("rewrite(e, [a_ -> a + b, c -> a])", "rewrite/global e/global a_/pattern a/pattern b/global c/global a/global"),
        ("match e { [h_, t__] => h + t, x @ g(_) => x, _ => t }", "e/global h_/pattern t__/pattern h/pattern t/pattern x/pattern g/global _/pattern x/pattern _/pattern t/global"),
        // for loops
        ("for k in range(n) { s = s + k }; k", "k/local range/global n/global s/global s/global k/local k/global"),
        // named arguments are not variables
        ("n(pi, digits: 50)", "n/global pi/global digits/global"),
        // nested shadowing of locals by locals
        ("x => (x => x)", "x/local x/local x/local"),
        // modules
        ("module Geo {\n  fn sq(x) = x*x\n  pub fn area(r: Real): Real = pi*sq(r)\n}\nimport Geo\narea(1)",
         "Geo/global sq/global x/local x/local x/local area/global r/local Real/global Real/global pi/global sq/global r/local Geo/global area/global"),
    ];
    let mut bad = Vec::new();
    for (src, want) in cases {
        let got = names(src);
        if got != want {
            bad.push(format!("{src:?}\n  want {want}\n  got  {got}"));
        }
    }
    assert!(bad.is_empty(), "\n{}", bad.join("\n"));
}

#[test]
fn undefined_and_shadowing() {
    let defined = |n: &str| matches!(n, "map" | "sin" | "n" | "e" | "pi");
    assert_eq!(
        names_with("map(x => sin(x) + q, e)", Some(&defined)),
        "map/global x/local sin/global x/local q/undefined e/global"
    );
    // a local hiding a defined global, and a name defined in the cell
    assert_eq!(
        names_with("h(n_) := n + e\nfn k(e) = e + h(1)", Some(&defined)),
        "h/global n_/shadowing n/pattern e/global k/global e/shadowing e/local h/global"
    );
    // with no name set, nothing is undefined
    assert_eq!(names("q + r"), "q/global r/global");
}

#[test]
fn brackets() {
    let src = "f(a[1], {b})";
    let c = colour(src, None);
    assert_eq!(c.pairs, vec![(1, 11), (3, 5), (8, 10)]);
    // caret just after `)` and just before `(`
    assert_eq!(c.matching_bracket(12), Some((11, 1)));
    assert_eq!(c.matching_bracket(1), Some((1, 11)));
    assert_eq!(c.matching_bracket(2), Some((1, 11)));
    assert_eq!(c.matching_bracket(6), Some((5, 3)));
    assert_eq!(c.matching_bracket(7), None);
    assert_eq!(c.enclosing_brackets(4, 5), Some(3..6));
    assert_eq!(c.enclosing_brackets(3, 6), Some(1..12));
    assert_eq!(c.enclosing_brackets(0, 12), None);
    assert_eq!(c.pair_at(8), Some(8..11));

    // unmatched: a closer with no opener, an opener never closed, an opener left inside a pair
    assert_eq!(snap("a)"), "a/global )/unmatched");
    assert_eq!(snap("f(x"), "f/global (/unmatched x/global");
    assert_eq!(snap("{ g(x }"), "{/op g/global (/unmatched x/global }/op");
    assert_eq!(snap("(]"), "(/unmatched ]/unmatched");
    // brackets in strings and comments do not count
    assert_eq!(snap("f(\")\") # (\n"), "f/global (/op \")\"/string )/op # (/comment");
}

#[test]
fn errors_and_incomplete_input() {
    // an error before the end colours its token
    assert_eq!(snap("a b"), "a/global b/error");
    assert_eq!(snap("x = (1 + )"), "x/global =/op (/op 1/number +/op )/error");
    // unread characters
    assert_eq!(snap("x ! y"), "x/global !/error y/error");
    assert_eq!(snap("∀x"), "∀/error x/global");
    // unfinished input: nothing is an error, the text is incomplete
    for src in ["f(x) := x +", "\"abc", "let x =", "if a then", "f(x_) := ", "{ let y = 2;", "x =\n"] {
        let c = colour(src, None);
        assert!(c.incomplete, "{src:?}");
        assert!(c.tokens.iter().all(|t| t.1 != TokenClass::SyntaxError), "{src:?}: {}", render(src, &c.tokens));
    }
    // while a statement is typed, every name before the last one already has its final class (a name just typed
    // may still become a lambda parameter when `=>` follows)
    let full = "f(x_) := x^2 + g(y => y + z, x) + { let w = 1; w }";
    let want: Vec<String> = names(full).split(' ').map(str::to_string).collect();
    for cut in 1..full.len() {
        let part = &full[..cut];
        let got = names(part);
        let got: Vec<&str> = got.split(' ').filter(|w| !w.is_empty()).collect();
        let settled = got.len().saturating_sub(1);
        assert_eq!(got[..settled], want[..settled], "prefix {part:?}");
    }
}

#[test]
fn placeholders() {
    assert_eq!(snap("□/B"), "□/placeholder //op B/global");
    let c = colour("1 + □ + □", None);
    assert_eq!(c.errors.len(), 1);
    assert_eq!(c.errors[0].span.start, 4);
    assert!(c.errors[0].message.contains("placeholder"));
}

#[test]
fn unicode_operators() {
    assert_eq!(snap("x → 2 ⧴ 3"), "x/global →/op 2/number ⧴/op 3/number");
    assert_eq!(snap("2×3·4÷5−1"), "2/number ×/op 3/number ·/op 4/number ÷/op 5/number −/op 1/number");
    assert_eq!(snap("for k ∈ xs { k }"), "for/keyword k/local ∈/keyword xs/global {/op k/local }/op");
    assert_eq!(snap("√x + ∞ + π + ⅇ + 3ⅈ"), "√/op x/global +/op ∞/global +/op π/global +/op ⅇ/global +/op 3ⅈ/number");
}

#[test]
fn names_in_scope_for_completion() {
    let src = "fn f(alpha, beta) = { let gamma = 1\n  alp }";
    let caret = src.find("alp }").map(|i| i + 3).unwrap_or(0);
    let got: Vec<String> = names_in_scope(src, caret).into_iter().map(|n| n.name).collect();
    assert_eq!(got, ["gamma", "beta", "alpha"]);
    // at a space after a lambda arrow, while typing
    let src = "map(x => ";
    let n = names_in_scope(src, src.len());
    assert_eq!(n.len(), 1);
    assert_eq!((n[0].name.as_str(), n[0].kind, n[0].binder.clone()), ("x", LocalKind::Local, 4..5));
    // pattern variables on the right of a rule
    let src = "g(u_, v_) := u + ";
    let got: Vec<(String, LocalKind)> = names_in_scope(src, src.len()).into_iter().map(|n| (n.name, n.kind)).collect();
    assert_eq!(got, [("v".to_string(), LocalKind::Pattern), ("u".to_string(), LocalKind::Pattern)]);
    // outside every scope
    assert!(names_in_scope("{ let a = 1 }\n", 15).is_empty());
}

#[test]
fn never_panics_and_covers_text_in_order() {
    let samples = [
        "", "(", ")", "]]]]", "{{{{", "f(x_) := ", "x => => =>", "fn", "fn f(", "fn f(x", "let", "for", "for x in",
        "match", "match e {", "@", "@x\n", "a @", "x_:", "x_: (", "□□□", "\"", "`a", "# only a comment", "1~", "&(",
        "module M {", "import", "(a, b) =>", "→→", "√", "√√x", "x |>\n", "if\nthen\nelse",
    ];
    for s in samples {
        let c = colour(s, Some(&|_: &str| false));
        let mut end = 0;
        for (r, _) in &c.tokens {
            assert!(r.start >= end && r.end > r.start && r.end <= s.len(), "{s:?}: {:?}", c.tokens);
            end = r.end;
        }
        let _ = names_in_scope(s, s.len());
        let _ = names_in_scope(s, 0);
    }
}

#[test]
fn linear_time_on_large_cells() {
    // 4,000 lines of definitions and uses: colouring must stay well under a frame budget per thousand lines
    let mut src = String::new();
    for k in 0..4000 {
        src.push_str(&format!("fn f{k}(x, y: Real) = {{ let z = x*y + g(t => t^2, [x, y])\n  z + h(a_ -> a) }}\n"));
    }
    let t = std::time::Instant::now();
    let c = colour(&src, None);
    let took = t.elapsed();
    assert!(c.errors.is_empty(), "{:?}", &c.errors[..1]);
    assert!(c.tokens.len() > 100_000);
    // generous: debug builds on a busy machine
    assert!(took.as_secs_f64() < 10.0, "{took:?}");

    // deep nesting does not overflow and is coloured
    let deep = format!("{}x{}", "(".repeat(5000), ")".repeat(5000));
    let c = colour(&deep, None);
    assert_eq!(c.pairs.len(), 5000);

    // hostile shapes stay linear: many errors, many unmatched brackets, a long rule chain
    for hostile in ["\"\n".repeat(30_000), ")]}".repeat(20_000), "[(".repeat(20_000), "a -> ".repeat(15_000)] {
        let t = std::time::Instant::now();
        let c = colour(&hostile, None);
        let _ = mc_syntax::extend_selection(&hostile, 7, 7);
        let _ = names_in_scope(&hostile, hostile.len() / 2);
        assert!(!c.tokens.is_empty());
        assert!(t.elapsed().as_secs_f64() < 10.0, "{:?} on {:?}...", t.elapsed(), &hostile[..10]);
    }

    // above the limit: lexical only
    let big = "x + 1\n".repeat(20_000);
    let c = colour(&big, None);
    assert!(c.lexical_only);
}

/// The golden snapshot of the classes of every token of the round-trip corpus (one line per statement of
/// `corpus/statements.mc`, then one line per line of `corpus/programs.mc` coloured as one cell). Regenerate with
/// `MC_BLESS=1` and review the difference.
#[test]
fn corpus_snapshot() {
    let statements = include_str!("corpus/statements.mc");
    let programs = include_str!("corpus/programs.mc");
    let mut out = String::new();
    for line in statements.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')) {
        let c = colour(line, None);
        assert!(
            c.errors.is_empty() && c.tokens.iter().all(|t| !matches!(t.1, TokenClass::SyntaxError | TokenClass::UnmatchedBracket)),
            "{line:?}"
        );
        out.push_str(&render(line, &c.tokens));
        out.push('\n');
    }
    let c = colour(programs, None);
    assert!(c.errors.is_empty(), "{:?}", c.errors);
    let mut start = 0;
    for line in programs.split_inclusive('\n') {
        let end = start + line.len();
        let toks: Vec<_> = c.tokens.iter().filter(|t| t.0.start >= start && t.0.end <= end).cloned().collect();
        if !toks.is_empty() {
            out.push_str(&render(programs, &toks));
            out.push('\n');
        }
        start = end;
    }
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/corpus/colour.snap");
    if std::env::var_os("MC_BLESS").is_some() {
        std::fs::write(path, &out).expect("write the snapshot");
        return;
    }
    // compared without carriage returns: the corpus and the snapshot may be checked out with either line ending
    let want = std::fs::read_to_string(path).unwrap_or_default().replace('\r', "");
    let out = out.replace('\r', "");
    if want != out {
        let diff: Vec<String> = want
            .lines()
            .zip(out.lines())
            .filter(|(a, b)| a != b)
            .take(5)
            .map(|(a, b)| format!("  want {a}\n  got  {b}"))
            .collect();
        panic!("colour snapshot differs (MC_BLESS=1 regenerates it):\n{}", diff.join("\n"));
    }
}
