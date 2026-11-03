//! Exact arithmetic through the canonicaliser against num-bigint as the oracle: integers of 1 to 60 limbs (so
//! schoolbook and Karatsuba products, Knuth division in gcds), products beyond the NTT threshold, rationals, powers,
//! decimal conversion; and canonical uniqueness of numbers computed different ways.
mod common;

use common::{big_of, Rng};
use mc_api::{RefStore, Store, View, T};
use mc_arith::Arith;
use num_bigint::{BigInt, Sign};
use num_traits::{Signed, Zero};

fn rand_big(rng: &mut Rng, max_limbs: u64) -> BigInt {
    let limbs = 1 + rng.below(max_limbs) as usize;
    let mut v = vec![];
    for _ in 0..limbs * 2 {
        v.push(rng.next_u64() as u32);
    }
    // vary the top so that small and boundary values occur
    let shift = rng.below(64 * limbs as u64);
    let mag = num_bigint::BigUint::new(v) >> shift;
    BigInt::from_biguint(if rng.below(2) == 0 { Sign::Minus } else { Sign::Plus }, mag)
}

fn term_of<'h>(a: &Arith<'h, RefStore>, x: &BigInt) -> T<'h> {
    a.int_from_decimal(&x.to_string()).expect("decimal")
}

/// The value of an exact real term as (num, den).
fn frac_of(st: &RefStore, t: T<'_>) -> (BigInt, BigInt) {
    match st.view(t) {
        View::Rational { num, den } => (big_of(st, num).unwrap(), big_of(st, den).unwrap()),
        _ => (big_of(st, t).expect("an exact real"), BigInt::from(1)),
    }
}

fn gcd(a: &BigInt, b: &BigInt) -> BigInt {
    let (mut a, mut b) = (a.abs(), b.abs());
    while !b.is_zero() {
        let r = &a % &b;
        a = b;
        b = r;
    }
    a
}

#[test]
fn integers_agree_with_num_bigint() {
    let st = RefStore::new();
    let a = Arith::new(&st);
    let mut rng = Rng(303);
    let cases = 3000;
    for case in 0..cases {
        let (x, y) = (rand_big(&mut rng, 60), rand_big(&mut rng, 60));
        let (tx, ty) = (term_of(&a, &x), term_of(&a, &y));
        assert_eq!(big_of(&st, tx).as_ref(), Some(&x), "round trip");
        assert_eq!(big_of(&st, a.add(&[tx, ty])), Some(&x + &y), "case {case} add");
        assert_eq!(big_of(&st, a.sub(tx, ty)), Some(&x - &y), "case {case} sub");
        assert_eq!(big_of(&st, a.mul(&[tx, ty])), Some(&x * &y), "case {case} mul");
        assert_eq!(a.text(tx).unwrap(), x.to_string(), "case {case} decimal");
        let k = rng.below(6) as u32;
        assert_eq!(big_of(&st, a.pow(tx, a.int(k as i64))), Some(x.pow(k)), "case {case} pow");
        if case % 1000 == 999 {
            println!("[{}/{cases} {}%]", case + 1, (case + 1) * 100 / cases);
        }
    }
    // products at NTT sizes, and a long decimal conversion
    for limbs in [1600u64, 5000] {
        let (x, y) = (rand_big(&mut rng, limbs) + 1, rand_big(&mut rng, limbs) + 1);
        let p = a.mul(&[term_of(&a, &x), term_of(&a, &y)]);
        assert_eq!(big_of(&st, p), Some(&x * &y), "{limbs} limbs");
        assert_eq!(a.text(p).unwrap(), (&x * &y).to_string());
    }
    let p = a.pow(a.int(3), a.int(20000));
    assert_eq!(big_of(&st, p), Some(BigInt::from(3).pow(20000)));
}

#[test]
fn rationals_agree_with_num_bigint() {
    let st = RefStore::new();
    let a = Arith::new(&st);
    let mut rng = Rng(404);
    let cases = 2000;
    for case in 0..cases {
        let big = |rng: &mut Rng| rand_big(rng, 12);
        let nonzero = |rng: &mut Rng| loop {
            let v = rand_big(rng, 12);
            if !v.is_zero() {
                return v;
            }
        };
        // a common factor, so that reduction has work to do
        let g = nonzero(&mut rng);
        let (n1, d1, n2, d2) = (big(&mut rng) * &g, nonzero(&mut rng) * &g, big(&mut rng), nonzero(&mut rng) * &g);
        let q1 = a.rational(term_of(&a, &n1), term_of(&a, &d1));
        let q2 = a.rational(term_of(&a, &n2), term_of(&a, &d2));
        let check = |t: T<'_>, n: BigInt, d: BigInt, what: &str| {
            let (gn, gd) = frac_of(&st, t);
            assert!(gd > BigInt::from(0) && gcd(&gn, &gd) == BigInt::from(1), "case {case} {what}: not canonical");
            assert_eq!(&gn * &d, &n * &gd, "case {case} {what}");
        };
        check(q1, n1.clone(), d1.clone(), "build");
        check(a.add(&[q1, q2]), &n1 * &d2 + &n2 * &d1, &d1 * &d2, "add");
        check(a.mul(&[q1, q2]), &n1 * &n2, &d1 * &d2, "mul");
        check(a.sub(q1, q2), &n1 * &d2 - &n2 * &d1, &d1 * &d2, "sub");
        if !n2.is_zero() {
            check(a.div(q1, q2), &n1 * &d2, &d1 * &n2, "div");
        }
        let k = rng.below(5) as u32;
        check(a.pow(q1, a.int(k as i64)), n1.pow(k), d1.pow(k), "pow");
        if !n1.is_zero() {
            check(a.pow(q1, a.int(-(k as i64))), d1.pow(k), n1.pow(k), "pow negative");
        }
        if case % 500 == 499 {
            println!("[{}/{cases} {}%]", case + 1, (case + 1) * 100 / cases);
        }
    }
}

/// Equal values built different ways are one handle: sums of fractions in shuffled orders and groupings, and
/// powers computed by products or by one pow.
#[test]
fn equal_numbers_are_one_term() {
    let st = RefStore::new();
    let a = Arith::new(&st);
    let mut rng = Rng(505);
    for case in 0..500 {
        let parts: Vec<(i64, i64)> = (0..rng.range(1, 8)).map(|_| (rng.range(-50, 50), rng.range(1, 30))).collect();
        let mut terms: Vec<T<'_>> = parts.iter().map(|&(n, d)| a.rational(a.int(n), a.int(d))).collect();
        let one = a.add(&terms);
        rng.shuffle(&mut terms);
        let two = terms.iter().fold(a.int(0), |acc, &t| a.add(&[t, acc]));
        assert_eq!(one, two, "case {case}");
        let base = terms[0];
        let k = rng.range(0, 6);
        let by_mul = a.mul(&vec![base; k as usize]);
        assert_eq!(a.pow(base, a.int(k)), by_mul, "case {case} power");
    }
    // integers that fit a machine word are always the small tier
    let big = a.pow(a.int(2), a.int(200));
    let back = a.sub(a.add(&[big, a.int(7)]), big);
    assert_eq!(back, a.int(7));
    assert!(matches!(st.view(back), View::Int(7)));
}

/// Ball results through the canonicaliser enclose the exact results (comparisons with the exact value are never
/// decided the wrong way), and their printed digits are the exact value rounded to that many digits.
#[test]
fn balls_enclose_and_print_correct_digits() {
    use mc_arith::text::ball_digits;
    use mc_arith::{Cmp3, Num, Real};
    let st = RefStore::new();
    let a = Arith::new(&st);
    let mut rng = Rng(808);
    let cases = 1500;
    for case in 0..cases {
        let p = [53u64, 64, 100, 300][rng.below(4) as usize];
        let q = |rng: &mut Rng| loop {
            let (n, d) = (rng.range(-1000, 1000), rng.range(1, 1000));
            if n != 0 {
                return (n, d);
            }
        };
        let ((n1, d1), (n2, d2)) = (q(&mut rng), q(&mut rng));
        let (e1, e2) = (a.rational(a.int(n1), a.int(d1)), a.rational(a.int(n2), a.int(d2)));
        let (b1, b2) = (a.ball(e1, p).unwrap(), a.ball(e2, p).unwrap());
        for (ball, exact) in [
            (a.add(&[b1, b2]), a.add(&[e1, e2])),
            (a.mul(&[b1, b2]), a.mul(&[e1, e2])),
            (a.div(b1, b2), a.div(e1, e2)),
            (a.add(&[b1, e2]), a.add(&[e1, e2])),
            (a.pow(b1, a.int(5)), a.pow(e1, a.int(5))),
        ] {
            let c = a.compare(ball, exact).expect("numbers");
            assert!(matches!(c, Cmp3::Equal | Cmp3::Undecided), "case {case}: {c:?}");
            let Some(Num::Real(Real::Ball(b, bp))) = a.num(ball) else { panic!("a ball") };
            let (en, ed) = frac_of(&st, exact);
            if en.is_zero() {
                continue;
            }
            let Some(d) = ball_digits(&b, bp) else { panic!("case {case}: no digits") };
            assert!(d.digits.len() >= 10, "case {case}: only {} digits at {bp} bits", d.digits.len());
            // |exact| 10^(len - 1 - exp10) is within one half of the digits read as an integer
            let k = d.digits.len() as i64 - 1 - d.exp10;
            let (mut num, mut den) = (en.abs(), ed.clone());
            if k >= 0 {
                num *= BigInt::from(10).pow(k as u32);
            } else {
                den *= BigInt::from(10).pow((-k) as u32);
            }
            let digits: BigInt = d.digits.parse().unwrap();
            let twice_err: BigInt = ((&digits * &den - &num) * BigInt::from(2)).abs();
            assert!(twice_err <= den, "case {case}: digits {} e{} are not the rounding of {en}/{ed}", d.digits, d.exp10);
            assert_eq!(d.neg, en < BigInt::from(0));
        }
        if case % 500 == 499 {
            println!("[{}/{cases} {}%]", case + 1, (case + 1) * 100 / cases);
        }
    }
}
