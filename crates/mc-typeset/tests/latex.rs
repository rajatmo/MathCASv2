//! LaTeX of typeset output: printed values read into boxes, then written as LaTeX.
use mc_boxes::to_latex;
use mc_typeset::typeset_output;

fn out(t: &str) -> String {
    to_latex(&typeset_output(t).row)
}

#[test]
fn outputs() {
    assert_eq!(out("(2/3)*2^(1/2)"), "\\frac{2 \\sqrt{2}}{3}");
    assert_eq!(out("[[1, 2], [3, 4]]"), "\\begin{bmatrix} 1 & 2 \\\\ 3 & 4 \\end{bmatrix}");
    assert_eq!(out("x^2 - 2*x*y"), "{x}^{2} - 2 x y");
    assert_eq!(out("2*3"), "2\\times 3");
    assert_eq!(out("pi + inf"), "\\pi + \\infty");
}
