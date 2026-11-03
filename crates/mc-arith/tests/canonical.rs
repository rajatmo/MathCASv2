//! The canonicaliser on the cases semantics 2.4 and 3 name, written out.
mod common;

use common::show;
use common::TestStore;
use mc_api::{Store, T};
use mc_arith::{Arith, Cmp3};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

struct Env<'h> {
    a: Arith<'h, TestStore>,
}

impl<'h> Env<'h> {
    fn s(&self, name: &str) -> T<'h> {
        let st = self.a.store();
        st.sym_term(st.sym(name))
    }
    fn i(&self, v: i64) -> T<'h> {
        self.a.int(v)
    }
    fn q(&self, n: i64, d: i64) -> T<'h> {
        self.a.rational(self.i(n), self.i(d))
    }
    fn r(&self, v: f64) -> T<'h> {
        self.a.store().real(v)
    }
    fn show(&self, t: T<'h>) -> String {
        show(self.a.store(), t)
    }
    fn check(&self, t: T<'h>, want: &str) {
        assert_eq!(self.show(t), want);
    }
}

fn with_env(f: impl Fn(&Env)) {
    for st in TestStore::all() {
        let env = Env { a: Arith::new(&st) };
        f(&env);
    }
}

#[test]
fn sums_and_products() {
    with_env(|e| {
        let a = &e.a;
        let (x, y, z) = (e.s("x"), e.s("y"), e.s("z"));
        let two_x = a.mul(&[e.i(2), x]);
        e.check(a.add(&[x, two_x]), "mul(3, x)");
        let x2 = a.pow(x, e.i(2));
        e.check(a.mul(&[x, x2]), "pow(x, 3)");
        e.check(a.add(&[x]), "x");
        e.check(a.add(&[]), "0");
        e.check(a.mul(&[]), "1");
        e.check(a.mul(&[x]), "x");
        let yz = a.add(&[z, y]);
        e.check(a.add(&[yz, x]), "add(x, y, z)");
        e.check(a.add(&[x, e.i(0)]), "x");
        e.check(a.mul(&[x, e.i(1)]), "x");
        e.check(a.mul(&[x, e.i(0)]), "0");
        e.check(a.sub(x, x), "0");
        e.check(a.div(x, x), "1");
        e.check(a.add(&[e.i(1), x, e.i(2)]), "add(3, x)");
        e.check(a.mul(&[y, e.i(2), x, e.i(3)]), "mul(6, x, y)");
        // like terms with numeric coefficients, and terms that cancel
        let t = a.add(&[a.mul(&[e.i(3), x, y]), a.mul(&[e.i(-3), y, x]), z]);
        e.check(t, "z");
        let half_x = a.mul(&[e.q(1, 2), x]);
        e.check(a.add(&[half_x, half_x]), "x");
        // subtraction and division lower to add, mul and pow
        e.check(a.sub(x, y), "add(x, mul(-1, y))");
        e.check(a.div(x, y), "mul(x, pow(y, -1))");
        e.check(a.neg(a.neg(x)), "x");
        // products of sums are not expanded; equal sums collect
        let s = a.add(&[x, y]);
        e.check(a.mul(&[s, s]), "pow(add(x, y), 2)");
        e.check(a.add(&[s, s]), "add(mul(2, x), mul(2, y))");
        e.check(a.mul(&[e.i(2), s]), "mul(2, add(x, y))");
    });
}

#[test]
fn exact_numbers() {
    with_env(|e| {
        let a = &e.a;
        e.check(a.add(&[e.q(1, 2), e.q(1, 3)]), "5/6");
        e.check(a.add(&[e.q(1, 2), e.q(1, 2)]), "1");
        e.check(e.q(4, -6), "-2/3");
        e.check(a.div(e.i(1), e.i(0)), "failure(division_by_zero)");
        e.check(a.mul(&[e.s("x"), a.pow(e.i(0), e.i(-1))]), "failure(division_by_zero)");
        e.check(a.pow(e.i(2), e.i(-2)), "1/4");
        e.check(a.pow(e.q(-2, 3), e.i(3)), "-8/27");
        e.check(a.pow(e.i(2), e.i(64)), "18446744073709551616");
        e.check(a.pow(e.i(0), e.i(0)), "1");
        e.check(a.pow(e.i(-1), a.int_from_decimal("123456789012345678901234567890").unwrap()), "1");
        let big = a.int_from_decimal("10000000000").unwrap();
        e.check(a.pow(e.i(2), big), "failure(limit, memory, pow(2, 10000000000))");
        // exact powers stay exact when the result is rational, otherwise canonical radicals
        e.check(a.pow(e.i(4), e.q(1, 2)), "2");
        e.check(a.pow(e.i(8), e.q(1, 2)), "mul(2, pow(2, 1/2))");
        e.check(a.pow(e.i(8), e.q(2, 3)), "4");
        e.check(a.pow(e.i(12), e.q(1, 3)), "mul(pow(2, 2/3), pow(3, 1/3))");
        e.check(a.pow(e.q(1, 2), e.q(1, 2)), "mul(1/2, pow(2, 1/2))");
        e.check(a.pow(e.i(2), e.q(-1, 2)), "mul(1/2, pow(2, 1/2))");
        let r2 = a.pow(e.i(2), e.q(1, 2));
        e.check(a.mul(&[r2, r2]), "2");
        let r3 = a.pow(e.i(3), e.q(1, 2));
        e.check(a.mul(&[r2, r3]), "pow(6, 1/2)");
        let c2 = a.pow(e.i(2), e.q(1, 3));
        e.check(a.mul(&[r2, c2]), "pow(2, 5/6)");
        e.check(a.add(&[r2, r2]), "mul(2, pow(2, 1/2))");
        e.check(a.pow(e.i(0), e.q(1, 2)), "0");
        e.check(a.pow(e.i(0), e.q(-1, 2)), "failure(division_by_zero)");
    });
}

#[test]
fn complex_numbers() {
    with_env(|e| {
        let a = &e.a;
        let i = a.complex(e.i(0), e.i(1));
        e.check(i, "complex(0, 1)");
        e.check(a.mul(&[i, i]), "-1");
        e.check(a.complex(e.i(2), e.i(0)), "2");
        e.check(a.complex(e.i(1), e.r(0.0)), "complex(1.0, 0.0)");
        e.check(a.complex(e.q(1, 2), e.r(2.0)), "complex(0.5, 2.0)");
        e.check(a.pow(e.i(-1), e.q(1, 2)), "complex(0, 1)");
        e.check(a.pow(e.i(-4), e.q(1, 2)), "complex(0, 2)");
        e.check(a.pow(e.i(-8), e.q(1, 3)), "mul(2, pow(-1, 1/3))");
        let one_i = a.add(&[e.i(1), i]);
        e.check(one_i, "complex(1, 1)");
        e.check(a.pow(one_i, e.i(2)), "complex(0, 2)");
        e.check(a.pow(one_i, e.i(-1)), "complex(1/2, -1/2)");
        e.check(a.add(&[e.i(1), a.mul(&[e.i(2), i])]), "complex(1, 2)");
        // a complex coefficient collects like any other
        let x = e.s("x");
        e.check(a.add(&[x, a.mul(&[i, x])]), "mul(complex(1, 1), x)");
        // exact complex powers with non-integer exponents stay unevaluated
        e.check(a.pow(one_i, e.q(1, 2)), "pow(complex(1, 1), 1/2)");
    });
}

#[test]
fn inexact_numbers() {
    with_env(|e| {
        let a = &e.a;
        let x = e.s("x");
        e.check(a.add(&[e.i(1), e.r(2.5)]), "3.5");
        e.check(a.mul(&[e.q(1, 2), e.r(3.0)]), "1.5");
        // identities are exact: 1.0 and 0.0 stay (B.6), but an inexact zero absorbs a product
        e.check(a.mul(&[e.r(1.0), x]), "mul(1.0, x)");
        e.check(a.add(&[e.r(0.0), x]), "add(0.0, x)");
        e.check(a.mul(&[e.r(0.0), x]), "0.0");
        e.check(a.add(&[x, a.mul(&[e.r(-1.0), x])]), "0.0");
        e.check(a.add(&[x, a.mul(&[e.r(1.5), x])]), "mul(2.5, x)");
        // the fold does not depend on the order of the operands
        let p = a.add(&[e.r(0.1), x, e.r(0.2), e.r(0.3)]);
        let q = a.add(&[e.r(0.3), e.r(0.2), x, e.r(0.1)]);
        assert_eq!(p, q);
        e.check(a.pow(e.r(2.0), e.q(1, 2)), &format!("{:?}", 2f64.sqrt()));
        e.check(a.pow(e.i(2), e.r(0.5)), &format!("{:?}", 2f64.sqrt()));
        e.check(a.pow(e.r(-8.0), e.q(1, 3)), "complex(1.0, 1.732050807568877)");
        e.check(a.pow(e.r(-2.0), e.r(2.0)), "4.0");
        // balls: contagion exact < ball < machine
        let b = a.ball(e.q(1, 3), 100).unwrap();
        let s = a.add(&[b, e.q(2, 3)]);
        assert_eq!(a.compare(s, e.i(1)), Some(Cmp3::Undecided));
        assert_eq!(a.compare(s, e.i(2)), Some(Cmp3::Less));
        let m = a.add(&[b, e.r(0.5)]);
        assert!(e.show(m).starts_with("0.83333"));
        assert_eq!(a.text(a.ball(e.q(1, 4), 64).unwrap()).unwrap(), "0.2500000000000000000");
        assert_eq!(a.compare(e.q(1, 2), e.r(0.5)), Some(Cmp3::Equal));
        assert_eq!(a.equal(e.r(f64::NAN), e.r(f64::NAN)), Some(Some(false)));
        // an explicit radius
        let wide = a.ball_with_radius(e.i(1), e.q(1, 10), 64).unwrap();
        assert_eq!(a.compare(wide, e.q(21, 20)), Some(Cmp3::Undecided));
        assert_eq!(a.compare(wide, e.q(6, 5)), Some(Cmp3::Less));
        assert_eq!(a.text(wide).unwrap(), "ball(1., 0.101)");
    });
}

#[test]
fn powers_of_symbols() {
    with_env(|e| {
        let a = &e.a;
        let (x, y) = (e.s("x"), e.s("y"));
        e.check(a.pow(x, e.i(0)), "1");
        e.check(a.pow(x, e.i(1)), "x");
        e.check(a.pow(e.i(1), x), "1");
        let x2 = a.pow(x, e.i(2));
        // (x^2)^(1/2) is not x for all complex x; (x^(1/2))^2 is
        e.check(a.pow(x2, e.q(1, 2)), "pow(pow(x, 2), 1/2)");
        let rx = a.pow(x, e.q(1, 2));
        e.check(a.pow(rx, e.i(2)), "x");
        e.check(a.pow(rx, e.q(1, 3)), "pow(x, 1/6)");
        e.check(a.mul(&[rx, rx]), "x");
        e.check(a.mul(&[x, rx]), "pow(x, 3/2)");
        let xy = a.mul(&[x, y]);
        e.check(a.pow(xy, e.i(2)), "mul(pow(x, 2), pow(y, 2))");
        e.check(a.pow(xy, e.q(1, 2)), "pow(mul(x, y), 1/2)");
        let sq = a.pow(xy, e.q(1, 2));
        e.check(a.mul(&[sq, sq]), "mul(x, y)");
        e.check(a.mul(&[sq, sq, x]), "mul(y, pow(x, 2))");
        let two_x = a.mul(&[e.i(2), x]);
        e.check(a.pow(two_x, e.q(1, 2)), "mul(pow(2, 1/2), pow(x, 1/2))");
        e.check(a.pow(a.mul(&[e.i(-2), x]), e.q(1, 2)), "pow(mul(-2, x), 1/2)");
        e.check(a.pow(two_x, e.i(3)), "mul(8, pow(x, 3))");
        let xn = a.pow(x, y);
        e.check(a.pow(xn, e.i(2)), "pow(x, mul(2, y))");
        e.check(a.mul(&[xn, x]), "pow(x, add(1, y))");
        e.check(a.div(xn, xn), "1");
        let ex = a.pow(e.i(2), x);
        e.check(a.mul(&[ex, a.pow(e.i(2), y)]), "pow(2, add(x, y))");
        e.check(a.mul(&[a.pow(x, e.i(-1)), x2]), "x");
    });
}

#[test]
fn failures_propagate_and_stop_interrupts() {
    for st in TestStore::all() {
        let a = Arith::new(&st);
        let x = st.sym_term(st.sym("x"));
        let f = a.div(a.int(1), a.int(0));
        assert_eq!(a.add(&[x, f]), f);
        assert_eq!(a.mul(&[f, x]), f);
        assert_eq!(a.pow(x, f), f);
        let flag = Arc::new(AtomicBool::new(true));
        let a = Arith::new(&st).with_stop(flag);
        let t = a.add(&[x, x]);
        assert_eq!(show(&st, t), "failure(interrupted, add(x, x))");
    }
}
