//! The polynomial benchmark cases at their small sizes (the large sizes are in `large_cases.rs`).
mod cases;
mod common;
use common::Sys;

#[test]
fn p1_dense_three_variables() {
    let mut s = Sys::new();
    for m in [3, 10] {
        cases::dense_product(&mut s, &["x", "y", "z"], m);
    }
}

#[test]
fn p2_dense_four_variables() {
    cases::dense_product(&mut Sys::new(), &["x", "y", "z", "t"], 6);
}

#[test]
fn r8_plain_expansion() {
    let mut s = Sys::new();
    for m in [1, 10] {
        cases::r8(&mut s, m);
    }
}

#[test]
fn p4_multivariate_gcd() {
    cases::p4(&mut Sys::new(), 5);
}

#[test]
fn p5_univariate_factorisation() {
    let mut s = Sys::new();
    for d in [32, 64] {
        cases::p5(&mut s, d);
    }
}

#[test]
fn p7_resultants() {
    let mut s = Sys::new();
    assert_eq!(cases::p7(&mut s, 20), "79792266297612001");
    assert_eq!(cases::p7(&mut s, 50), "1798465042647412146620280340569649349251249");
    assert_eq!(
        cases::p7(&mut s, 100),
        "3234476509624757991344647769100216810857203198904625400933895331391691459636928060001"
    );
}

#[test]
fn p11_dense_univariate() {
    cases::p11(&mut Sys::new(), 2000);
}

#[test]
fn p12_series() {
    cases::p12(&mut Sys::new(), 100);
}
