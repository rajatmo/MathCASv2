//! Powers c^x with a negative base over the reals: the principal value exp(x log|c|) e^(i pi x) is real only at
//! integer x, so the real domain is the integers and the function is singular and discontinuous everywhere.
mod common;
use common::check;

#[test]
fn domain_is_the_integers() {
    check(&[
        // the values themselves: real at an integer, not real at x = 1/2 or 1/3
        ("[(-2)^3, im_part((-1)^(1/2)), im_part(n((-2)^(1/3))) != 0, im_part(n((-1/2)^(1/2))) != 0]", "[-8, 1, true, true]"),
        ("function_domain((-1)^x, x)", "element(x, integers)"),
        ("function_domain((-2)^x, x)", "element(x, integers)"),
        ("function_domain((-1/2)^x, x)", "element(x, integers)"),
        ("function_domain(a^x, x, assumptions: a < 0)", "element(x, integers)"),
        // a positive base or a base of unknown sign: no restriction; over the complexes the principal value is entire
        ("function_domain(2^x, x)", "true"),
        ("function_domain(a^x, x)", "true"),
        ("function_domain((-1)^x, x, complexes)", "true"),
    ]);
}

#[test]
fn singular_everywhere() {
    check(&[
        ("function_singularities((-2)^x, x)", "true"),
        ("function_singularities((-1/2)^x, x)", "true"),
        ("function_singularities(a^x, x, assumptions: a < 0)", "true"),
        ("function_discontinuities((-1)^x, x)", "true"),
        ("function_analytic((-1)^x, x)", "false"),
        ("function_singularities(2^x, x)", "false"),
        ("function_singularities((-1)^x, x, complexes)", "false"),
    ]);
}
