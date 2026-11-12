//! Round 35: a space between two operands multiplies, with the precedence and associativity of `*` (design 04,
//! section 3.1). Every row of the round's table in both spellings, precedence against the other operators, the
//! forms that keep their meaning, the gaps the editor marks, and the round trip of the new forms.
mod common;
use common::raw;
use mc_api::RefStore;
use mc_syntax::{parse, read, render_all, Printer};

fn lower(src: &str) -> Result<String, String> {
    let store = RefStore::new();
    let ts = read(&store, src).map_err(|e| render_all(&e, src))?;
    Ok(ts.iter().map(|&t| raw(&store, t)).collect::<Vec<_>>().join("; "))
}

/// Each `(spaced, explicit)` pair reads as the same terms.
fn same(cases: &[(&str, &str)]) {
    let mut bad = Vec::new();
    for (a, b) in cases {
        match (lower(a), lower(b)) {
            (Ok(x), Ok(y)) if x == y => {}
            (x, y) => bad.push(format!("{a:?} -> {x:?}\n{b:?} -> {y:?}")),
        }
    }
    assert!(bad.is_empty(), "\n{}", bad.join("\n"));
}

#[test]
fn the_table_of_the_rule() {
    same(&[
        ("a b", "a*b"),
        ("2 x", "2*x"),
        ("m c^2", "m*(c^2)"),
        ("2 pi r", "2*pi*r"),
        ("a/b c", "a/b*c"),
        ("-x y", "-x*y"),
        ("a -b", "a - b"),
        ("1 2", "1*2"),
        ("2 3.5", "2*3.5"),
        ("f (x)", "f*(x)"),
        ("f (x)", "f*x"),
        ("a [1]", "a*[1]"),
        ("(a+b) (c+d)", "(a + b)*(c + d)"),
        ("sin x", "sin*x"),
        ("2x", "2*x"),
        ("2(x)", "2*x"),
        ("2pi", "2*pi"),
    ]);
    // the forms with no space keep their meaning: calls and indexing
    assert_eq!(lower("f(x)").unwrap(), "f(x)");
    assert_eq!(lower("a[1]").unwrap(), "part(a, 1)");
    assert_eq!(lower("(a+b)(c+d)").unwrap(), "add(a, b)(add(c, d))");
    assert_eq!(lower("a - b").unwrap(), "add(a, mul(-1, b))");
    // strings do not multiply
    for src in ["\"a\" \"b\"", "\"a\" b", "x \"b\""] {
        let e = lower(src).expect_err(src);
        assert!(e.contains("strings do not multiply"), "{src:?}: {e}");
    }
}

#[test]
fn precedence() {
    same(&[
        // powers bind tighter on both sides
        ("a b^c", "a*b^c"),
        ("a^b c", "a^b*c"),
        ("a^-b c", "a^(-b)*c"),
        ("e^x y", "e^x*y"),
        // division and products, left to right
        ("a b/c", "a*b/c"),
        ("a/b c/d", "a/b*c/d"),
        ("1/2 x", "1/2*x"),
        // unary minus applies to the whole product, as with `*`
        ("-a b c", "-a*b*c"),
        ("a*-b c", "a*(-b)*c"),
        // sums and comparisons are looser
        ("a b + c d", "a*b + c*d"),
        ("a b - c d", "a*b - c*d"),
        ("a b == c d", "a*b == c*d"),
        ("a b < c", "a*b < c"),
        ("not a b", "not (a*b)"),
        ("a b and c d", "a*b and c*d"),
        ("a b -> c d", "a*b -> c*d"),
        ("a b |> f", "(a*b) |> f"),
        // calls, indexing and primes bind tighter
        ("f(x) g(y)", "f(x)*g(y)"),
        ("f (x) g", "f*x*g"),
        ("f(a) (b)", "f(a)*b"),
        ("f(a)(b) c", "f(a)(b)*c"),
        ("a[1] b", "a[1]*b"),
        ("x y'", "x*y'"),
        ("f'(x) y", "f'(x)*y"),
        ("f' (x)", "f'*x"),
        ("√x y", "sqrt(x)*y"),
        ("a √b", "a*sqrt(b)"),
        ("a (b + c)", "a*(b + c)"),
        ("3 (x)", "3*x"),
        ("a × b", "a*b"),
        // a box form after a name
        ("a ((1)/(x))", "a*(1/x)"),
        // lambdas, named options, patterns, conditions
        ("x => 2 x", "x => 2*x"),
        ("(x, y) => x y", "(x, y) => x*y"),
        ("map(x => 2 x, xs)", "map(x => 2*x, xs)"),
        ("plot(a b, [x, 0, 1], plot_range: 2 c)", "plot(a*b, [x, 0, 1], plot_range: 2*c)"),
        ("plot(f, [x, 0, 1], plot_range: all)", "plot(f, [x, 0, 1], plot_range: all)"),
        ("f(x_ y_) := x y", "f(x_*y_) := x*y"),
        ("rule g(x_) := 2 x when x > 0", "rule g(x_) := 2*x when x > 0"),
        ("a b when c", "(a*b) when c"),
        ("&(2 &1)", "&(2*&1)"),
        // lists, indices, blocks and control
        ("[a b, c]", "[a*b, c]"),
        ("xs[i j]", "xs[i*j]"),
        ("xs[2 i:3 j]", "xs[2*i:3*j]"),
        ("{ let y = 2 x; y z }", "{ let y = 2*x; y*z }"),
        ("if a b then c d else e f", "if a*b then c*d else e*f"),
        ("for k in a b { k }", "for k in a*b { k }"),
        ("match e { a b => c d }", "match e { a*b => c*d }"),
        ("fn h(x) = 2 x", "fn h(x) = 2*x"),
    ]);
}

#[test]
fn line_ends_and_comments() {
    same(&[
        // a line end ends a statement at the top level and in a block
        ("a b\nc d", "a*b\nc*d"),
        ("{ a b\n c }", "{ a*b; c }"),
        // inside ( ) and [ ] a line end is white space: the product goes on
        ("(a\n b)", "a*b"),
        ("[a\n b]", "[a*b]"),
        ("[a,\n b c]", "[a, b*c]"),
        ("f(x,\n  2 y)", "f(x, 2*y)"),
        // comments between operands
        ("(a # a note\n b)", "a*b"),
        ("[2 # two\n pi]", "[2*pi]"),
    ]);
    assert_eq!(lower("a\nb").unwrap(), "a; b");
}

#[test]
fn the_parser_reports_the_gaps() {
    fn gaps(src: &str) -> Vec<&str> {
        parse(src).products.iter().map(|s| &src[s.start as usize..s.end as usize]).collect()
    }
    let at = |src: &str| -> Vec<(u32, u32)> { parse(src).products.iter().map(|s| (s.start, s.end)).collect() };
    assert_eq!(at("m c^2"), [(1, 2)]);
    assert_eq!(at("2 pi r"), [(1, 2), (4, 5)]);
    assert_eq!(gaps("a b + c  d"), [" ", "  "]);
    assert_eq!(gaps("(x + 1) (x - 1)"), [" "]);
    assert_eq!(gaps("[a\n  b, c]"), ["\n  "]);
    assert_eq!(gaps("(a # n\n b)"), [" # n\n "]);
    // adjacent factors, explicit operators, calls and sums get none
    for src in ["2x", "2(x + 1)", "2pi", "a*b", "f(x)", "a - b", "a -b", "(a+b)(c+d)", "x\ny", "\"a\" \"b\""] {
        assert!(gaps(src).is_empty(), "{src:?}: {:?}", gaps(src));
    }
    // a statement that is not finished keeps the products before the error
    assert_eq!(gaps("m c^"), [" "]);
}

#[test]
fn the_new_forms_round_trip() {
    let forms = [
        "a b", "2 x", "m c^2", "2 pi r", "a/b c", "-x y", "1 2", "2 3.5", "f (x)", "a [1]", "(a+b) (c+d)", "sin x",
        "a b^c", "a^b c", "x => 2 x", "f(x_ y_) := x y", "[a\n b]", "(a # c\n b)", "a b == c d", "f' (x)",
        "plot(a b, plot_range: 2 c)", "{ let y = 2 x; y z }", "-a b c", "a × b", "√x y",
    ];
    let store = RefStore::new();
    let p = Printer::new(&store);
    let mut bad = vec![];
    for src in forms {
        let Ok(ts) = read(&store, src) else {
            bad.push(format!("{src:?} does not read"));
            continue;
        };
        let printed = p.program(&ts);
        match read(&store, &printed) {
            Ok(again) if again == ts => {}
            _ => bad.push(format!("{src:?} printed as {printed:?}, which does not read back the same")),
        }
        // the printer keeps writing `*`
        if printed.contains(' ') && !printed.contains(['+', '=', '>', '-', '{', ',', '<']) {
            bad.push(format!("{src:?} printed with a space: {printed:?}"));
        }
    }
    assert!(bad.is_empty(), "\n{}", bad.join("\n"));
}
