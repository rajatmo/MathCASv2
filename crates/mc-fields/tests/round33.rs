//! Round 33: failing tests of number-field examples whose fix lies in other crates (mc-further-functions owns
//! `number_field_discriminant`, mc-algebra owns `to_number_field`); each is ignored until it is fixed there.
use mc_api::{Engine, StopFlag};

fn eval(src: &str) -> String {
    let mut e = mc_engine::CoreEngine::new();
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

#[test]
#[ignore = "mc-further-functions: the discriminant of an algebraic_number term is that of its generator's field"]
fn discriminant_of_an_algebraic_number_term() {
    // Q(1 + 2 t + t^2) with t a root of t^3 + t + 1 is the cubic field of discriminant -31 (printed 0 in round 32)
    assert_eq!(eval("number_field_discriminant(algebraic_number(root(&(&1^3 + &1 + 1), 3), [1, 2, 1]))"), "-31");
}

#[test]
#[ignore = "mc-algebra: to_number_field([sqrt(2), im], all) returns the list of numbers in a primitive field"]
fn to_number_field_with_all_returns_the_field_numbers() {
    // the first number is the generator root(x^4 - 2 x^2 + 9, 4) of Q(sqrt(2), i), whose discriminant is 256
    assert_eq!(eval("number_field_discriminant(to_number_field([sqrt(2), im], all)[1, 1])"), "256");
}
