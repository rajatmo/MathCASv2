//! The functions on the assembled engine: each case is source text and the printed value (the field terms
//! abbreviated by `F17`, ... in the expectations).
use mc_api::{Engine, StopFlag};

const F17: &str = r#"finite_field(17, &(14 + &1 + &1^3), "Polynomial")"#;
const F17_6: &str = r#"finite_field(17, &(3 + 3*&1 + 10*&1^2 + 2*&1^4 + &1^6), "Polynomial")"#;
const F7E: &str = r#"finite_field(7, &(4 + 6*&1^2 + &1^3), "Exponential")"#;

fn run(cases: &[(&str, &str)]) {
    let mut e = mc_engine::CoreEngine::new();
    let stop = StopFlag::new();
    let mut bad = vec![];
    for (src, want) in cases {
        let want = want.replace("F17_6", F17_6).replace("F17", F17).replace("F7E", F7E);
        let got = e.eval(src, &stop).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"));
        if got != want {
            bad.push(format!("{src}\n   want {want}\n   got  {got}"));
        }
    }
    assert!(bad.is_empty(), "{} of {} cases differ:\n{}", bad.len(), cases.len(), bad.join("\n"));
}

#[test]
fn fields_and_elements() {
    run(&[
        ("F = finite_field(17, 3)", "null"),
        // any spelling of the same field is the same term, and the printed form reads back
        (r#"finite_field(17, &(&1^3 + &1 + 31), "Polynomial") === F"#, "true"),
        ("finite_field(4913) === F", "true"),
        ("a = F([1, 2, 3]); a", "finite_field_element(F17, [1, 2, 3])"),
        ("b = finite_field_element(F, 123); b", "finite_field_element(F17, [4, 7])"),
        ("(a^2 + 2*b)/(5*a*b - 7)", "finite_field_element(F17, [13, 10, 2])"),
        ("[a - a, a/a]", "[finite_field_element(F17, []), finite_field_element(F17, [1])]"),
        ("a + 7/8", "finite_field_element(F17, [4, 2, 3])"),
        ("[F(0) == 0, F(1) == 1, a == b, a != b]", "[true, true, false, true]"),
        ("[element(7/8, F), element(2/17, F), element(a, F)]", "[true, false, true]"),
        ("b([\"Index\", \"Coefficients\", \"Characteristic\", \"FieldSize\"])", "[123, [4, 7], 17, 4913]"),
        ("finite_field_index(b)", "123"),
        ("F(4913 + 5) == F(5) + F([0, 0, 0, 1])", "true"),
        ("sqrt(a) === a^(1/2)", "true"),
        ("length(finite_field_root(a^2, 2))", "2"),
        ("finite_field(7, 3, \"Exponential\")(2)^77", "finite_field_element(F7E, 78)"),
        ("(finite_field(7, 3, \"Exponential\")(2)^77)(\"Coefficients\")", "[4, 2, 1]"),
        ("dot([4, 2, 1], [1, finite_field(7, 3, \"Exponential\")(2), finite_field(7, 3, \"Exponential\")(3)])", "finite_field_element(F7E, 78)"),
        ("finite_field(10, 2)", "finite_field(10, 2)"),
        ("finite_field_element_trace(F([0, 1]))", "0"),
        ("finite_field_element_norm(F([0, 1]))", "3"),
        ("finite_field_element_minimal_polynomial(F([0, 1]), x)", "14 + x + x^3"),
        ("is_finite_field_element_primitive(F([0, 1]))", "true"),
        ("finite_field_log(F([0, 1])^1234)", "1234"),
        ("frobenius_automorphism(a, 3) == a", "true"),
        ("conway_polynomial(2, 8, x)", "1 + x^2 + x^3 + x^4 + x^8"),
        ("[is_irreducible_polynomial(x^4 + 1), is_irreducible_polynomial(x^4 + 1, modulus: 3)]", "[true, false]"),
        ("[is_primitive_polynomial(x^4 + x + 1, 2), is_primitive_polynomial(x^4 + x^3 + x^2 + x + 1, 2)]", "[true, false]"),
    ]);
}

#[test]
fn polynomials_and_embeddings() {
    run(&[
        ("F = finite_field(17, 3)", "null"),
        ("polynomial_gcd(F(1)*x^2 + F(246)*x + F(4436), F(3)*x^2 + F(1771))", "finite_field_element(F17, [4, 7]) + x*finite_field_element(F17, [1])"),
        ("expand(factor(x^3 + 5*x + 19, extension: F)) == expand(F(1)*x^3 + 5*x + 19)", "true"),
        ("is_irreducible_polynomial(x^3 + 5*x + 19, extension: F)", "false"),
        ("K = finite_field(17, 6)", "null"),
        ("e = finite_field_embedding(F, K); e", "finite_field_embedding(finite_field_element(F17, [0, 1]) -> finite_field_element(F17_6, [14, 6, 1, 3, 16, 12]))"),
        ("e(F(123))", "finite_field_element(F17_6, [0, 8, 7, 4, 10, 16])"),
        ("e(\"Projection\")(e(F(123))) == F(123)", "true"),
        ("[e(F(5) + F(77)) == e(F(5)) + e(F(77)), e(F(5) * F(77)) == e(F(5)) * e(F(77))]", "[true, true]"),
        ("finite_field_embedding(finite_field(2, 2), finite_field(3, 2)) === finite_field_embedding(finite_field(2, 2), finite_field(3, 2))", "true"),
        ("minimal_polynomial(finite_field(29, 4)(123), x)", "23 + 7*x^2 + 10*x + x^3 + x^4"),
    ]);
}
