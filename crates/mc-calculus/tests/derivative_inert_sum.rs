//! Derivatives of inert sums: term by term inside the sum, the bound index never escapes.
mod common;
use common::check;

#[test]
fn inert_sum_term_by_term() {
    check(&[
        ("d(inactive(sum)(f(k, y), [k, a, b]), y)", "inactive(sum)(derivative(0, 1)(f)(k, y), [k, a, b])"),
        ("d(inactive(sum)(k*y^2, [k, 1, n]), y)", "inactive(sum)(2*k*y, [k, 1, n])"),
        // the index is bound: the sum does not vary with it
        ("d(inactive(sum)(f(k, y), [k, a, b]), k)", "0"),
        // by hand: d/dy (y^2 (1 + 2 + 3 + 4)) = 20 y
        ("activate(d(inactive(sum)(k*y^2, [k, 1, 4]), y))", "20*y"),
        // a bound that varies has no derivative over the integers: the call stays
        ("d(inactive(sum)(f(k, y), [k, a, y]), y)", "d(inactive(sum)(f(k, y), [k, a, y]), y)"),
    ]);
}
