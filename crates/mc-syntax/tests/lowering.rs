//! Lowering produces the term shapes of 04-syntax.md section 12 (and the decisions recorded in the notes).
mod common;
use common::raw;
use mc_api::RefStore;
use mc_syntax::{read, render_all};

fn lower(src: &str) -> String {
    let store = RefStore::new();
    let ts = read(&store, src).unwrap_or_else(|e| panic!("{src:?}:\n{}", render_all(&e, src)));
    ts.iter().map(|&t| raw(&store, t)).collect::<Vec<_>>().join("; ")
}

fn check(cases: &[(&str, &str)]) {
    let mut bad = Vec::new();
    for (src, want) in cases {
        let got = lower(src);
        if got != *want {
            bad.push(format!("{src:?}\n   got  {got}\n   want {want}"));
        }
    }
    assert!(bad.is_empty(), "\n{}", bad.join("\n"));
}

#[test]
fn arithmetic() {
    check(&[
        ("a - b", "add(a, mul(-1, b))"),
        ("a / b", "mul(a, pow(b, -1))"),
        ("-a", "mul(-1, a)"),
        ("a + b - c", "add(a, b, mul(-1, c))"),
        ("(a + b) + c", "add(add(a, b), c)"),
        ("a*b/c", "mul(a, b, pow(c, -1))"),
        ("2x", "mul(2, x)"),
        ("3(a + b)", "mul(3, add(a, b))"),
        ("2x^2", "mul(2, pow(x, 2))"),
        ("-2x", "mul(-2, x)"),
        ("-2^2", "mul(-1, pow(2, 2))"),
        ("2^-1", "pow(2, -1)"),
        ("a^b^c", "pow(a, pow(b, c))"),
        ("-x^2", "mul(-1, pow(x, 2))"),
        ("a*-b", "mul(a, mul(-1, b))"),
    ]);
}

#[test]
fn numbers_are_exact() {
    check(&[
        ("a - 7", "add(a, -7)"),
        ("3/4", "3/4"),
        ("6/4", "3/2"),
        ("-3/4", "-3/4"),
        ("4/2", "2"),
        ("0/7", "0"),
        ("a - 3/4", "add(a, -3/4)"),
        ("5/0", "mul(5, pow(0, -1))"),
        ("1/2/3", "mul(1, pow(2, -1), pow(3, -1))"),
        ("x/4", "mul(x, pow(4, -1))"),
        ("18446744073709551616", "big[0, 1]"),
        ("-18446744073709551616", "-big[0, 1]"),
        ("9223372036854775808", "big[9223372036854775808]"),
        ("-9223372036854775808", "-9223372036854775808"),
        ("0x10", "16"),
        ("0b1_0000", "16"),
        ("1_000", "1000"),
        ("36893488147419103232/18446744073709551616", "2"),
        ("1/18446744073709551616", "1/big[0, 1]"),
        ("1.5", "1.5"),
        ("-0.0", "-0.0"),
        ("2e-10", "2e-10"),
        ("1.", "1.0"),
        ("1.25~40", "ball_literal(\"1.25\", 40)"),
        ("-2~10", "ball_literal(\"-2\", 10)"),
        ("3im", "complex(0, 3)"),
        ("-1.5im", "complex(0, -1.5)"),
        ("2 + 3im", "add(2, complex(0, 3))"),
        ("2 - 3im", "add(2, complex(0, -3))"),
        ("2imag", "mul(2, imag)"),
        ("2e", "mul(2, e)"),
    ]);
}

#[test]
fn collections_and_calls() {
    check(&[
        ("[a, b]", "list(a, b)"),
        ("[]", "list()"),
        ("xs[i]", "part(xs, i)"),
        ("xs[i, j]", "part(xs, i, j)"),
        ("xs[i:j]", "part(xs, span(i, j))"),
        ("f(a)(b)", "f(a)(b)"),
        ("n(x, digits: 50)", "n(x, options(rule(digits, 50)))"),
        ("f(a: 1, b: 2)", "f(options(rule(a, 1), rule(b, 2)))"),
        ("a |> f(b, _)", "f(b, a)"),
        ("a |> f", "f(a)"),
        ("a |> f(b)", "f(b)(a)"),
        ("f'", "derivative(1)(f)"),
        ("y''(t)", "derivative(2)(y)(t)"),
        ("π + ∞", "add(pi, inf)"),
        ("\"a\\tb\"", "\"a\\tb\""),
        ("`if`(x)", "if(x)"),
        ("M.f(x)", "M.f(x)"),
    ]);
}

#[test]
fn operators_and_patterns() {
    check(&[
        ("a == b", "equal(a, b)"),
        ("a != b", "unequal(a, b)"),
        ("a <= b", "less_equal(a, b)"),
        ("a >= b", "greater_equal(a, b)"),
        ("a === b", "same(a, b)"),
        ("a !== b", "not_same(a, b)"),
        ("not a and b", "and(not(a), b)"),
        ("a or b or c", "or(a, b, c)"),
        ("a -> b -> c", "rule(a, rule(b, c))"),
        ("a :> b", "rule_delayed(a, b)"),
        ("x_: Int", "typed(any(x), Int)"),
        ("_", "any()"),
        ("xs__", "seq1(xs)"),
        ("___", "seq0()"),
        ("x @ f(y_)", "named(x, f(any(y)))"),
        ("a_ | b_ | c_", "alternatives(any(a), any(b), any(c))"),
        ("p_ when c", "condition(any(p), c)"),
        ("f(x_ = 1)", "f(optional(any(x), 1))"),
        ("f(x_: Int = 1)", "f(optional(typed(any(x), Int), 1))"),
        ("x_: List[Int] | (Int) -> Real", "typed(any(x), type_union(List(Int), fun_type(list(Int), Real)))"),
        ("x_: Int where x > 0", "typed(any(x), type_where(Int, greater(x, 0)))"),
        ("x_: Array[Real, 2]", "typed(any(x), Array(Real, 2))"),
    ]);
}

#[test]
fn functions_blocks_control() {
    check(&[
        ("x => e", "fn(list(x), e)"),
        ("() => 1", "fn(list(), 1)"),
        ("(x: Real, y = 2, zs...) => x", "fn(list(typed(x, Real), optional(y, 2), rest(zs)), x)"),
        ("(held e) => e", "fn(list(held(e)), e)"),
        ("&(&1 + 1)", "fn(list(&1), add(&1, 1))"),
        ("&(&2)", "fn(list(&1, &2), &2)"),
        ("&(f(&1, &(&3)))", "fn(list(&1), f(&1, fn(list(&1, &2, &3), &3)))"),
        ("{ let x = a; e }", "block(let(x, a), e)"),
        ("{ var i = 0; i = i + 1 }", "block(var(i, 0), assign(i, add(i, 1)))"),
        ("{ var x = 0; y => { x = y } }", "block(var(x, 0), fn(list(y), block(assign(x, y))))"),
        ("{ let x: Int = 3; x }", "block(let(typed(x, Int), 3), x)"),
        ("{ x = 1 }", "block(set(x, 1))"),
        ("if c then a else b", "if(c, a, b)"),
        ("if c then a", "if(c, a)"),
        ("while c { b }", "while(c, block(b))"),
        ("for x in xs { b }", "for(x, xs, block(b))"),
        ("match e { p_ => r }", "match(e, list(rule_delayed(any(p), r)))"),
        ("match e { p_ when c => r, 0 => 1 }", "match(e, list(rule_delayed(condition(any(p), c), r), rule_delayed(0, 1)))"),
    ]);
}

#[test]
fn definitions() {
    check(&[
        ("x = 1", "set(x, 1)"),
        ("x := e", "set_delayed(x, e)"),
        ("var x = 0\nx = 1", "var(x, 0); assign(x, 1)"),
        ("let x: Int = 3", "let(typed(x, Int), 3)"),
        ("rule f(x_) := b", "define_rule(rule_delayed(f(any(x)), b), list())"),
        ("rule f(x_) = b", "define_rule(rule(f(any(x)), b), list())"),
        ("rule f(x_) := b when c", "define_rule(rule_delayed(condition(f(any(x)), c), b), list())"),
        ("fn f(x: T) = b", "define_fn(f(typed(any(x), T)), b, list(), null, list())"),
        (
            "fn f(x, n: Int = 10; d: Int = 16): Real when x > 0 = b",
            "define_fn(condition(f(any(x), optional(typed(any(n), Int), 10)), greater(x, 0)), b, \
             list(optional(typed(any(d), Int), 16)), Real, list())",
        ),
        ("fn f(held e, rest...) = e", "define_fn(f(held(any(e)), seq0(rest)), e, list(), null, list())"),
        ("fn f(0) = 1", "define_fn(f(0), 1, list(), null, list())"),
        ("@commutative @identity(0) sym op", "define_sym(op, list(commutative, identity(0)))"),
        (
            "@on(square) rule area(square(a_)) := a^2",
            "define_rule(rule_delayed(area(square(any(a))), pow(a, 2)), list(on(square)))",
        ),
        ("module M { pub x = 1 }", "module(M, list(public(set(x, 1))))"),
        ("import M", "import(M)"),
        ("import M.{f, g}", "import(M, list(f, g))"),
        ("import core except e", "import_except(core, list(e))"),
    ]);
}

/// Unicode operator characters read exactly as their ASCII spellings (04 section 2).
#[test]
fn unicode_operators() {
    let same = [
        ("a → b", "a -> b"),
        ("f(x_) ⧴ x", "f(x_) :> x"),
        ("a ≤ b", "a <= b"),
        ("a ≥ b", "a >= b"),
        ("a ≠ b", "a != b"),
        ("a ⩵ b", "a == b"),
        ("a ≡ b", "a === b"),
        ("a ≢ b", "a !== b"),
        ("a × b · c ⋅ d", "a * b * c * d"),
        ("a ÷ b", "a / b"),
        ("a − b", "a - b"),
        ("−3", "-3"),
        ("p ∧ q ∨ ¬r", "p and q or not r"),
        ("for k ∈ xs { k }", "for k in xs { k }"),
        ("ⅇ^(ⅈ*π)", "e^(im*pi)"),
        ("3ⅈ + 2.5ⅈ", "3im + 2.5im"),
        ("2∞", "2inf"),
        ("√x", "sqrt(x)"),
        ("√(x + 1)", "sqrt(x + 1)"),
        ("√x^2", "sqrt(x)^2"),
        ("2√2", "2*sqrt(2)"),
        ("√√x", "sqrt(sqrt(x))"),
        ("√f'(x)", "sqrt(f'(x))"),
    ];
    let mut bad = Vec::new();
    for (a, b) in same {
        if lower(a) != lower(b) {
            bad.push(format!("{a:?} gives {}, {b:?} gives {}", lower(a), lower(b)));
        }
    }
    assert!(bad.is_empty(), "\n{}", bad.join("\n"));
}

/// Round 32 (design 08 section 3.9): `∈` between operands is `element`, `°` after an operand is the degree.
#[test]
fn element_and_degree_characters() {
    check(&[
        ("x ∈ reals", "element(x, reals)"),
        ("x + 1 ∈ integers", "element(add(x, 1), integers)"),
        ("[x, y] ∈ reals and z ∈ reals", "and(element(list(x, y), reals), element(z, reals))"),
        ("sin(30°)", "sin(mul(30, mul(pi, pow(180, -1))))"),
        ("x°", "mul(x, mul(pi, pow(180, -1)))"),
        ("(a + b)°", "mul(add(a, b), mul(pi, pow(180, -1)))"),
    ]);
    // `∈` is also the `in` of a loop; a lone `°` and chained comparisons still read as errors
    assert!(mc_syntax::parse("for k ∈ [1, 2] { k }").errors.is_empty());
    assert!(!mc_syntax::parse("°").errors.is_empty());
    assert!(!mc_syntax::parse("x ∈ a < b").errors.is_empty());
}
