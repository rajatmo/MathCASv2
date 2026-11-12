//! Item F2 through the evaluator: subresultants, decompositions and the modulus option, on the documented
//! examples.
mod common;
use common::Sys;

fn check(cases: &[(&str, &str)]) {
    let mut s = Sys::new();
    for (src, want) in cases {
        if *want == "?" {
            println!("{src} => {}", s.run(src));
            continue;
        }
        assert_eq!(s.run(src), *want, "{src}");
    }
}

#[test]
fn subresultant_chains() {
    check(&[
        (
            "subresultant_polynomials((x - 1)^2*(x - 2)*(x - 3), (x - 1)*(x - 4)^2, x)",
            "[0, -36 + 36*x, 38 + -49*x + 11*x^2, -16 + -9*x^2 + 24*x + x^3]",
        ),
        (
            "subresultant_polynomials(a*x^3 + b*x^2 + c*x + d, 3*a*x^2 + b*x + c, x)",
            "[4*a^2*c^3 + 27*a^3*d^2 + 2*a*d*b^3 + -18*b*c*d*a^2, 9*d*a^2 + -2*a*b*c + -2*a*x*b^2 + 6*c*x*a^2, c + b*x + 3*a*x^2]",
        ),
        (
            "subresultant_polynomials(2*x^7 + 3*x^3 + 5*x - 1, 7*x^6 + 8*x - 9, x)",
            "[-183782157189, -761749829 + 3208696817*x, -3143546 + 3838135*x^2 + 11222638*x, -21609 + -49392*x^2 + 64827*x^3 + 163611*x, 0, -49 + -112*x^2 + 147*x^3 + 371*x, -9 + 7*x^6 + 8*x]",
        ),
        ("subresultant_polynomials((x - 1)^2*(x - 2)*(x - 3), (x - 1)*(x - 4)^2, x, modulus: 2)", "[0, 0, x + x^2, x^2 + x^3]"),
        (
            "subresultant_polynomials((x - 1)^2*(x - 2)*(x - 3), (x - 1)*(x - 4)^2, x, modulus: 5)",
            "[0, 4 + x, 3 + x + x^2, 4 + 4*x + x^2 + x^3]",
        ),
        ("subresultant_polynomials(-14 + 4*x + 3*x^2 + 2*x^3 + 5*x^8, g, x)", "[g^8]"),
        ("subresultants(-14 + 4*x + 3*x^2 + 2*x^3 + 5*x^8, g, x)", "[g^8]"),
        ("subresultants(x^3 + x/2 + 1, x^2/3 - 1, x)", "[-143/108, 7/18, 1/3]"),
        // the smaller degree first: the coefficients of the swapped pair with the sign (-1)^((dp - j)(dq - j))
        ("subresultants(x^2/3 - 1, x^3 + x/2 + 1, x)", "[-143/108, 7/18, 1/3]"),
        ("subresultants(x + 1, x^3 + 2, x)", "[1, 1]"),
        // the polynomial chains stay defined for deg p >= deg q
        ("subresultant_polynomials(x + 1, x^3 + 2, x)", "subresultant_polynomials(1 + x, 2 + x^3, x)"),
        ("subresultant_polynomials(x^2 - 1.2, x - 3.4, x)", "subresultant_polynomials(-1.2 + x^2, -3.4 + x, x)"),
        (
            "subresultant_polynomial_remainders((x - 1)^2*(x - 2)*(x - 3), (x - 1)*(x - 4)^2, x)",
            "[6 + -17*x + -7*x^3 + 17*x^2 + x^4, -16 + -9*x^2 + 24*x + x^3, 38 + -49*x + 11*x^2, -36 + 36*x]",
        ),
        (
            "subresultant_polynomial_remainders(-10 + 4*x + 3*x^2 + 2*x^3 + x^8, -19 + 7*x + 6*x^2 + 5*x^3 + x^7, x)",
            "[-10 + 2*x^3 + 3*x^2 + 4*x + x^8, -19 + 5*x^3 + 6*x^2 + 7*x + x^7, -10 + -5*x^4 + -4*x^2 + -4*x^3 + 23*x, -18585 + -3029*x^3 + 1606*x^2 + 20008*x, 452264 + -595320*x^2 + 143056*x, -692132416 + 692132416*x]",
        ),
        (
            "subresultant_polynomial_remainders((x - 1)^2*(x - 2)*(x - 3), (x - 1)*(x - 4)^2, x, modulus: 2)",
            "[x + x^2 + x^3 + x^4, x^2 + x^3, x + x^2]",
        ),
        (
            "subresultant_polynomial_remainders((x - 1)^2*(x - 2)*(x - 3), (x - 1)*(x - 4)^2, x, modulus: 3)",
            "[x + 2*x^2 + 2*x^3 + x^4, 2 + x^3, 2 + 2*x + 2*x^2]",
        ),
        ("subresultants(x, y, x, modulus: 4)", "failure(argument, subresultants, \"the modulus must be a prime (or 0)\")"),
    ]);
}

#[test]
fn decompositions() {
    check(&[
        ("decompose(x^2 + 1, x)", "[1 + x, x^2]"),
        ("decompose(x^4 + x^2, x)", "[x + x^2, x^2]"),
        ("decompose((x + x^2)^2, x)", "[x^2, x + x^2]"),
        ("decompose((x^2 + x)^4 + 1, x)", "[1 + x, x^4, x + x^2]"),
        ("decompose((x^2 + 1)^4 + 3, x)", "[4 + 2*x + x^2, 2*x + x^2, x^2]"),
        ("decompose(2*x + 1, x)", "[1 + 2*x]"),
        ("decompose((a*x^3 + 1)^2 + b, x)", "[1 + b + a^2*x^2 + 2*a*x, x^3]"),
        ("decompose((x^2 + im)^2, x)", "[-1 + 2im*x + x^2, x^2]"),
        ("decompose(x^4 + 4*x^3 + 4*x^2 + 6*x + 24, x, modulus: 3)", "[x^2, 2*x + x^2]"),
        ("decompose(x^3 + 2*x^2 + 4*x + 7, x, modulus: 2)", "[1 + x, x^3]"),
        ("decompose(x^8 + 4*x^7 + 2*x^6 - 8*x^5 - 5*x^4 + 8*x^3 + 2*x^2 - 4*x + 8, x)", "[8 + 2*x + x^2, -2*x + x^2, x + x^2]"),
        ("decompose(x^2 + 3*x + 2, x)", "[2 + 3*x + x^2]"),
        ("decompose(g, x)", "[g]"),
        ("decompose((x + 1)^3 + 1, x)", "[2 + 3*x + 3*x^2 + x^3]"),
        ("decompose((2*x^2 + a*x)^3 - 5*(2*x^2 + a*x)/b, x)", "[8*x^3 + -10*x/b, (1/2)*a*x + x^2]"),
        ("decompose(1/x + x, x)", "decompose(x + x^-1, x)"),
    ]);
}

#[test]
fn modulus_option() {
    check(&[
        ("resultant(x^2 - 2*x + 7, x^3 - x + 5, x, modulus: 3)", "1"),
        (
            "resultant(a*x^3 + b*x^2 + c*x + d, d*x^3 + c*x^2 + b*x + a, x, modulus: 2)",
            "a^2*c^4 + a^2*d^4 + a^4*b^2 + a^4*d^2 + b^4*d^2 + c^2*d^4 + a^2*b^2*c^2 + a^2*b^2*d^2 + a^2*c^2*d^2 + b^2*c^2*d^2 + a^6 + d^6",
        ),
        ("discriminant(x^5 - x*y + y^2 - 1, x, modulus: 3)", "2 + 2*y^5 + 2*y^8 + y^2 + y^6"),
        ("discriminant(a*x^3 + b*x^2 + c*x + d, x, modulus: 2)", "a^2*d^2 + b^2*c^2"),
        ("discriminant(a*x^3 + b*x^2 + c*x + d, x, modulus: 3)", "b^2*c^2 + 2*a*c^3 + 2*d*b^3"),
        ("resultant(x^2 - 1, x - 1, x, modulus: 4)", "failure(argument, resultant, \"the modulus must be a prime (or 0)\")"),
        ("resultant(x^2 - 1, x - 3, x, modulus: 0)", "8"),
        // products stay products (decision F2.9)
        ("resultant((x - a)^2*(x - b), (x - c)^2*(x - d)*(x - e), x)", "(b - d)*(b - e)*(a - c)^4*(a - d)^2*(a - e)^2*(b - c)^2"),
        ("discriminant((x - a)*(x - b)*(x - c)*(x - d), x)", "(a - b)^2*(a - c)^2*(a - d)^2*(b - c)^2*(b - d)^2*(c - d)^2"),
        ("discriminant(3*(x - a)*(x^2 + b), x)", "-324*b*(b + a^2)^2"),
        ("discriminant((x - 1)^2*(x + 2), x)", "0"),
        ("expand(resultant((x - 2)^2*(x^2 + 3*x - 1), (x + 1)*(x^3 - 5), x) - resultant(expand((x - 2)^2*(x^2 + 3*x - 1)), expand((x + 1)*(x^3 - 5)), x))", "0"),
        ("expand(discriminant(2*(x - 2)*(x^2 + 3*x - 1)*(x^3 - y), x) - discriminant(expand(2*(x - 2)*(x^2 + 3*x - 1)*(x^3 - y)), x))", "0"),
    ]);
}
