//! `complex_expand`: real and imaginary parts with every symbol real except the listed ones.
mod common;
use common::check;

#[test]
fn parts_of_functions_and_arithmetic() {
    check(&[
        ("complex_expand(re(sin(a + b*im)))", "cosh(b)*sin(a)"),
        ("complex_expand(sin(a + b*im))", "cosh(b)*sin(a) + 1im*cos(a)*sinh(b)"),
        ("complex_expand(exp(x + im*y))", "cos(y)*exp(x) + 1im*exp(x)*sin(y)"),
        ("complex_expand(abs(x + im*y))", "(x^2 + y^2)^(1/2)"),
        ("complex_expand((x + im*y)^2)", "-y^2 + 2im*x*y + x^2"),
        ("complex_expand(1/(x + im*y))", "x/(x^2 + y^2) + -1im*y/(x^2 + y^2)"),
        ("complex_expand(log(x + im*y))", "1im*arg(x + 1im*y) + (1/2)*log(x^2 + y^2)"),
        ("complex_expand(conjugate(z), z)", "-1im*im_part(z) + re(z)"),
        ("complex_expand(re(z^2), [z])", "-im_part(z)^2 + re(z)^2"),
        ("complex_expand([cos(x + im*y), cosh(im*x)])", "[cos(x)*cosh(y) + -1im*sin(x)*sinh(y), cos(x)]"),
        ("complex_expand(conjugate(x*y + im))", "-1im + x*y"),
        ("complex_expand(f(x))", "f(x)"),
    ]);
}
