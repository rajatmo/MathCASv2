//! Item 9B part 2: optional arguments under associative-commutative heads and against a term that
//! is not a call of the head (the unit of the head), optional parameters with names, conditions and runs, typed
//! sequence variables, on source text.
mod common;
use common::{check, Env};

#[test]
fn optional_arguments_under_ac_heads() {
    check(
        &mut Env::new(),
        &[
            // a term that is not a product is the product with the unit factor 1
            ("match_q(x, optional(n_, 1) * x_)", "true"),
            ("rewrite([x, 2*x, -x], optional(n_, 1) * x :> y^n)", "[y, y^2, y^-1]"),
            ("rewrite(x, optional(c_, 1) * v_ :> [c, v])", "[1, x]"),
            // a default that is not the unit does not stand for a missing factor
            ("rewrite(x, optional(c_, 7) * v_ :> [c, v])", "x"),
            // present first; part of a longer sum
            ("rewrite([a, a + b, a + b + c], a + optional(x_, 0) -> f(x))", "[f(0), f(b), c + f(b)]"),
            ("rewrite(b, b^optional(k_, 1) :> g(k))", "g(1)"),
            ("rewrite(b^3, b^optional(k_, 1) :> g(k))", "g(3)"),
            ("rewrite(2*sin(x) + 3*cos(x), optional(a_, 1)*sin(x_) + optional(b_, 1)*cos(x_) :> [a, b, x])", "[2, 3, x]"),
            ("rewrite(sin(y) + 3*cos(y), optional(a_, 1)*sin(x_) + optional(b_, 1)*cos(x_) :> [a, b, x])", "[1, 3, y]"),
            ("replace(x^2/y^3, (a @ (_^optional(p_, 1))) * (b @ (_^optional(q_, 1))) -> f(a, b))", "f(x^2, y^-3)"),
            // in definitions: the rule index takes such an argument as any term
            ("rule k(optional(n_, 1) * x_) := [n, x]", "null"),
            ("[k(y), k(2*y)]", "[[1, y], [2, y]]"),
            ("rule pw(x_^optional(m_, 1)) := m", "null"),
            ("[pw(b), pw(b^3)]", "[1, 3]"),
        ],
    );
}

#[test]
fn optional_parameters_with_patterns() {
    check(
        &mut Env::new(),
        &[
            ("rule g(optional(x_ when is_number(x), \"abc\")) := x", "null"),
            ("[g(5), g(\"xyz\"), g()]", "[5, g(\"xyz\"), \"abc\"]"),
            ("rule f(x_, optional(type @ (\"I\" | \"II\"), \"none\")) := [x, type]", "null"),
            ("[f(1), f(1, \"I\"), f(1, \"III\")]", "[[1, \"none\"], [1, \"I\"], f(1, \"III\")]"),
            // a missing run is the run of the one default
            ("rule h(optional(x__, 1)) := x", "null"),
            ("[h(), h(2), h(2, 3)]", "[[1], [2], [2, 3]]"),
            ("match_q([x], [x, optional(t @ (_: Int), 2)])", "true"),
        ],
    );
}

#[test]
fn typed_sequence_variables() {
    check(
        &mut Env::new(),
        &[
            ("rule f(x__: Float) := apply(add, x) / length(x)", "null"),
            ("[f(1., 2.), f(1, 2.)]", "[1.5, f(1, 2.0)]"),
            ("rule g(x_: Sym, p__: Int) := apply(add, x^p)", "null"),
            ("[g(x, 1, 2), g(0, 1, 2), g(x, 1, 2.)]", "[x + x^2, g(0, 1, 2), g(x, 1, 2.0)]"),
            ("rewrite([u(1), u(1, u(1, 2)), u(1, u(1, 2, 3))], u(1, x__: Int) :> 1, strategy: fixed_point)", "[u(1), 1, 1]"),
            ("rewrite([u(1), u(1, u(1, 2)), u(1, u(1, 2, 3))], u(1, x___: Int) :> 1, strategy: fixed_point)", "[1, 1, 1]"),
            // under an AC head the run takes the arguments of its type
            ("rewrite(1 + 2*a + b, xs__: Int + r__ -> [xs, r])", "[1, b + 2*a]"),
            ("match_q(a + b, xs__: Int + r__)", "false"),
        ],
    );
}

#[test]
fn rules_for_calls_with_compound_heads() {
    check(
        &mut Env::new(),
        &[
            ("rule f(x_)(y_) := x + y", "null"),
            ("[f(1)(2), f(a)]", "[3, f(a)]"),
            ("rule g(w_)(x_)(y_)(z_) := [w, x, y, z]", "null"),
            ("g(1)(2)(3)(4)", "[1, 2, 3, 4]"),
            ("rule k(x_)(0) := x", "null"),
            ("rule k(x_)(y_ when y > -2) := h(x, y)", "null"),
            ("[k(5)(0), k(5)(1), k(5)(-3)]", "[5, h(5, 1), k(5)(-3)]"),
            ("rule m(_)(0) = 0", "null"),
            ("[m(7)(0), m(7)(1)]", "[0, m(7)(1)]"),
            ("rewrite(u(a)(b)(c), g_(x_)(y__) -> v(g, x, y))", "v(u(a), b, [c])"),
            ("rule p(g_(x_)) := [g, x]", "null"),
            ("rule q_(x_)(y_) := w(q, x, y)", "null"),
            ("[r(1)(2), m(7)(0)]", "[w(r, 1, 2), 0]"),
        ],
    );
}

#[test]
fn replace_list_enumerates_matches() {
    check(
        &mut Env::new(),
        &[
            ("replace_list([a, b, c], [x__, y__] -> [x, y])", "[[[a], [b, c]], [[a, b], [c]]]"),
            ("replace_list([a, b, b, b, c, c, a], [___, x_, x_, ___] -> x)", "[b, b, c]"),
            ("replace_list(x, [x -> a, x -> b, x -> c])", "[a, b, c]"),
            ("replace_list([a, b, c, d], [___, x__, ___] -> x, 3)", "[[a], [a, b], [a, b, c]]"),
        ],
    );
}

#[test]
fn riffle_at_positions() {
    check(
        &mut Env::new(),
        &[
            ("riffle(range(9), [x, y])", "[1, x, 2, y, 3, x, 4, y, 5, x, 6, y, 7, x, 8, y, 9]"),
            ("riffle(range(9), x, 3)", "[1, 2, x, 3, 4, x, 5, 6, x, 7, 8, x, 9]"),
            ("riffle(range(5), x, [1, -1, 2])", "[x, 1, x, 2, x, 3, x, 4, x, 5, x]"),
            ("riffle(range(5), [x, y], [3, -1, 2])", "[1, 2, x, 3, y, 4, x, 5, y]"),
            ("riffle([4, 3, 4, 5, 2, 5, 2, 5, 2, 3, 5, 3, 5], c, [-4, 1, -4])", "[4, c, 3, 4, 5, c, 2, 5, 2, c, 5, 2, 3, c, 5, 3, 5]"),
        ],
    );
}
