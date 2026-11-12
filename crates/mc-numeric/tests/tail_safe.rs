//! R20-17: exp of a ball far below zero is None generically and a vanishing ball inside a tail-safe scope.
use mc_arith::num::Ball;
use mc_numeric::elementary::{exp, tail_safe, TailSafe};

#[test]
fn far_tail_exp_is_scoped() {
    let x = Ball::from_i64(-(1i64 << 50));
    assert!(exp(&x, 64).is_none());
    let v = tail_safe(|| exp(&x, 64)).expect("a ball inside the scope");
    assert!(v.mid.is_zero() && !v.rad.is_zero());
    // the scope ends with the guard and nests
    {
        let _a = TailSafe::enter();
        let _b = TailSafe::enter();
    }
    assert!(exp(&x, 64).is_none());
    // ordinary values are untouched by the scope
    let one = tail_safe(|| exp(&Ball::from_i64(0), 64)).unwrap();
    assert!(!one.contains_zero());
}
