//! Arguments at the edges of the machine integers and deep in a tail: each must give an answer, stay unevaluated or
//! fail honestly, never abort the process (a size guard on `i64::MIN.abs()` used to wrap and pass).
mod common;
use common::check;

#[test]
fn index_at_the_machine_integer_edge_stays_unevaluated() {
    check(&[
        ("lucas_l(-2^63)", "lucas_l(-9223372036854775808)"),
        ("fibonacci(-2^63)", "fibonacci(-9223372036854775808)"),
        ("lucas_l(-2^63 + 1)", "lucas_l(-9223372036854775807)"),
        ("pochhammer(x, -2^63)", "pochhammer(x, -9223372036854775808)"),
        ("lucas_l(-10)", "123"),
        ("fibonacci(-10)", "-55"),
    ]);
}

#[test]
fn erfc_deep_in_the_tail_does_not_allocate_without_bound() {
    // erfc(10^8) ~ 10^(-4.3 10^15) has no representable value: a failure, found at once (the round 20 zero-ball
    // enclosure was reverted, bugs R20-17)
    check(&[("n(erfc(10^8), 50)", "failure(precision_not_reached, erfc(100000000))")]);
}

#[test]
fn product_log_at_thousands_of_digits_does_not_abort() {
    check(&[("abs(n(product_log(2), 1500)*exp(n(product_log(2), 1500)) - 2) < 10^-1400", "true")]);
}
