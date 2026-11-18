//! `centered_interval` of one argument: an exact number, a numeric constant or an interval becomes a machine centre
//! and a radius rounded up, and the interval holds the number (checked at 40 digits against the exact value); a
//! symbol stays; with two arguments centre and radius are kept as given. And the Clebsch-Gordan coefficient of six
//! symbols written by its 3j symbol (DLMF 34.1.1), checked against the numeric coefficient at sample values.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn ev(e: &mut CoreEngine, src: &str) -> String {
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

/// The centre and the radius of `centered_interval(x)` enclose x: |x - c| <= r with x at 40 digits.
fn holds_value(e: &mut CoreEngine, x: &str) {
    let ci = ev(e, &format!("centered_interval({x})"));
    assert!(ci.starts_with("centered_interval(") && ci.contains('.'), "{x}: {ci}");
    let t = format!("{{ let ci = centered_interval({x}); abs(n({x}, 40) - ci[1]) <= abs(ci[2]) }}");
    assert_eq!(ev(e, &t), "true", "{x}: {ci}");
}

#[test]
fn exact_numbers_get_a_machine_enclosure() {
    let mut e = CoreEngine::new();
    for x in ["1/3", "2/3", "123/2^12", "-7/11", "10^20 + 1/7", "pi", "e", "sqrt(2)", "0"] {
        holds_value(&mut e, x);
    }
    assert_eq!(ev(&mut e, "centered_interval(123/2^12)"), "centered_interval(0.030029296875, 3.3339216803929292e-18)");
}

#[test]
fn complex_constants_get_a_box() {
    let mut e = CoreEngine::new();
    for x in ["1/2 + 3/4*im", "e + im*pi"] {
        let t = format!(
            "{{ let ci = centered_interval({x}); let d = n({x}, 40) - ci[1]; abs(re(d)) <= re(ci[2]) and abs(im_part(d)) <= im_part(ci[2]) }}"
        );
        assert_eq!(ev(&mut e, &t), "true", "{x}");
    }
}

#[test]
fn intervals_become_midpoint_and_half_width() {
    let mut e = CoreEngine::new();
    let t = "{ let ci = centered_interval(interval([1/3, 2])); ci[1] - ci[2] <= 1/3 and ci[1] + ci[2] >= 2 and ci[2] < 0.834 }";
    assert_eq!(ev(&mut e, t), "true");
}

#[test]
fn symbols_and_two_arguments_stay() {
    let mut e = CoreEngine::new();
    assert_eq!(ev(&mut e, "centered_interval(x)"), "centered_interval(x)");
    assert_eq!(ev(&mut e, "centered_interval(2, 0.1)"), "centered_interval(2, 0.1)");
    assert_eq!(ev(&mut e, "centered_interval(9.87)"), "centered_interval(9.87, 0)");
}

#[test]
fn clebsch_gordan_of_symbols_is_its_three_j_form() {
    let mut e = CoreEngine::new();
    assert_eq!(
        ev(&mut e, "clebsch_gordan([j, m], [j1, m1], [j2, m2])"),
        "(-1)^(j + m2 - j1)*(1 + 2*j2)^(1/2)*three_j_symbol([j, m], [j1, m1], [j2, -m2])"
    );
    // the identity at numeric values: the 3j form substituted equals the coefficient
    for (a, b, c, d, j, m) in [("5", "0", "4", "0", "1", "0"), ("3/2", "1/2", "1", "0", "3/2", "1/2"), ("2", "1", "1", "-1", "2", "0")] {
        let t = format!(
            "simplify(rewrite(clebsch_gordan([aa, bb], [cc, dd], [jj, mm]), [aa -> {a}, bb -> {b}, cc -> {c}, dd -> {d}, jj -> {j}, mm -> {m}]) - clebsch_gordan([{a}, {b}], [{c}, {d}], [{j}, {m}]))"
        );
        assert_eq!(ev(&mut e, &t), "0", "{t}");
    }
    // a numeric argument keeps the call
    assert_eq!(ev(&mut e, "clebsch_gordan([5, 0], [4, m], [1, 0])"), "clebsch_gordan([5, 0], [4, m], [1, 0])");
}
