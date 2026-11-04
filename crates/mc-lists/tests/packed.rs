//! Packed arrays through the library and the evaluator (semantics 11, decisions N4a.6 to N4a.10): producers build
//! packed payloads, the fast paths of `length`, `part`, `sum`, `total` and of element-wise `add`/`mul` give exactly
//! the general path's results (bitwise, on awkward values), and the results are the same at 1 and 4 threads.
mod common;
use common::{check, Env};
use mc_core::{Heap, Store, View, PACK_MIN, T};
use mc_eval::Kernel;

/// A small deterministic generator (xorshift).
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn real(&mut self) -> f64 {
        const SPECIAL: [f64; 9] =
            [0.0, -0.0, f64::INFINITY, f64::NEG_INFINITY, f64::MAX, f64::MIN_POSITIVE / 4.0, 1e300, -1e-300, 0.1];
        match self.next() % 8 {
            0 => SPECIAL[(self.next() % 9) as usize],
            1 => f64::from_bits(self.next()).abs().min(1e308) * if self.next().is_multiple_of(2) { 1.0 } else { -1.0 },
            _ => (self.next() % 2001) as f64 / 8.0 - 125.0,
        }
    }
    fn int(&mut self) -> i64 {
        match self.next() % 6 {
            0 => (self.next() as i64) >> (self.next() % 64),
            1 => (1 << 53) - (self.next() % 3) as i64,
            2 => i64::MAX - (self.next() % 4) as i64,
            _ => (self.next() % 2001) as i64 - 1000,
        }
    }
}

fn eval<'h>(h: &'h Heap, k: &mut Kernel, t: T<'h>) -> T<'h> {
    let e = k.eval(h, t);
    assert_eq!(e.status, mc_eval::Status::Running, "evaluation stopped");
    e.value
}

/// The view's arguments.
fn elems<'h>(h: &'h Heap, t: T<'h>) -> Vec<T<'h>> {
    match h.view(t) {
        View::App { args, .. } => args.to_vec(),
        _ => panic!("a list"),
    }
}

/// Element `i` of a packed operand, or the scalar operand itself.
fn at<'h>(h: &'h Heap, t: T<'h>, i: usize) -> T<'h> {
    if h.packed(t).is_some() {
        elems(h, t)[i]
    } else {
        t
    }
}

#[test]
fn element_wise_arithmetic_is_the_threaded_result() {
    let mut env = Env::new();
    env.step(|h, k| {
        let (add, mul) = (h.sym("add"), h.sym("mul"));
        let mut rng = Rng(0x2545_F491_4F6C_DD1D);
        let n = PACK_MIN * 3;
        let mut fast = 0;
        for round in 0..60 {
            // operands: packed reals, packed integers, scalars of both kinds; sometimes a NaN
            let mut make = |kind: u64| -> T<'_> {
                match kind {
                    0 => {
                        let mut v: Vec<f64> = (0..n).map(|_| rng.real()).collect();
                        if round % 10 == 9 {
                            v[5] = f64::NAN;
                        }
                        h.reals(&v)
                    }
                    1 => h.ints(&(0..n).map(|_| rng.int()).collect::<Vec<i64>>()),
                    2 => h.real(rng.real()),
                    _ => h.int(rng.int()),
                }
            };
            let (ka, kb) = (round % 4, (round / 4) % 4);
            let (ka, kb) = if ka >= 2 && kb >= 2 { (0, kb) } else { (ka, kb) };
            let (a, b) = (make(ka), make(kb));
            for op in [add, mul] {
                let got = eval(h, k, h.call(op, &[a, b]));
                // the reference: each element's scalar call, as threading evaluates it
                let want: Vec<T<'_>> = (0..n).map(|i| eval(h, k, h.call(op, &[at(h, a, i), at(h, b, i)]))).collect();
                assert_eq!(elems(h, got), want, "round {round}: {} of kinds {ka}, {kb}", h.sym_name(op));
                fast += h.packed(got).is_some() as usize;
            }
        }
        assert!(fast > 60, "most results stay packed ({fast})");
    });
}

#[test]
fn sums_and_totals_on_payloads_are_the_canonical_sums() {
    let mut env = Env::new();
    env.step(|h, k| {
        let (add, sum, total) = (h.sym("add"), h.sym("sum"), h.sym("total"));
        let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
        for round in 0..40 {
            let n = PACK_MIN + round * 37;
            let v: Vec<f64> = (0..n).map(|_| rng.real()).collect();
            let xs: Vec<T<'_>> = v.iter().map(|&x| h.real(x)).collect();
            let packed = h.reals(&v);
            assert!(h.packed(packed).is_some());
            // the general path: one canonical add of the elements and the exact zero (`sum`'s own reduction)
            let mut with_zero = xs.clone();
            with_zero.push(h.int(0));
            let want = eval(h, k, h.call(add, &with_zero));
            assert_eq!(eval(h, k, h.call(sum, &[packed])), want, "sum of {n} reals");
            let m = h.real_array(&[n, 1], &v);
            assert_eq!(eval(h, k, h.call(total, &[m])), want, "total of a {n} x 1 matrix");
            let is: Vec<i64> = (0..n).map(|_| rng.int()).collect();
            let ts: Vec<T<'_>> = is.iter().map(|&x| h.int(x)).collect();
            let want = eval(h, k, h.call(add, &ts));
            assert_eq!(eval(h, k, h.call(sum, &[h.ints(&is)])), want, "sum of {n} integers");
        }
        // all negative zeros: the canonical sum starts from the exact zero
        let z = h.reals(&[-0.0; PACK_MIN]);
        assert_eq!(eval(h, k, h.call(sum, &[z])), h.real(0.0));
    });
}

#[test]
fn producers_and_access_read_the_payload() {
    let mut env = Env::new();
    check(
        &mut env,
        &[
            ("v = range(1.0, 1000.0)", "null"),
            ("length(v)", "1000"),
            ("v[1]", "1.0"),
            ("v[-1]", "1000.0"),
            ("v[0]", "list"),
            ("v[1001]", "failure(index, 1001, 1000)"),
            ("v[998:-1]", "[998.0, 999.0, 1000.0]"),
            ("length(v[3:900])", "898"),
            ("v[5:4]", "[]"),
            ("m = [range(100), range(101, 200)]", "null"),
            ("m[2, 1]", "101"),
            ("length(m[2])", "100"),
            ("m[2][100]", "200"),
            ("sum(range(10^5))", "5000050000"),
            ("total(m)", "20100"),
            ("sum(range(0.0, 1.0, 0.25))", "2.5"),
            ("take(range(10) + range(10, 1, -1), 3)", "[11, 11, 11]"),
            ("sum(2.0*v - v) == sum(v)", "true"),
            ("range(1000) === map(k => k, range(1000))", "true"),
            ("range(100) + [1, 2]", "failure(shape_mismatch)"),
        ],
    );
    env.step(|h, k| {
        let range = h.sym("range");
        let t = eval(h, k, h.call(range, &[h.int(1), h.real(100.0)]));
        assert!(h.packed(t).and_then(|p| p.real_vector()).is_some(), "range of reals is packed");
        let t = eval(h, k, h.call(range, &[h.int(1000)]));
        assert_eq!(h.packed(t).and_then(|p| p.int_vector()).map(|v| v.len()), Some(1000));
    });
}

#[test]
fn packed_work_charges_fuel_and_agrees_across_threads() {
    let mut env = Env::new();
    assert_eq!(env.eval_with("sum(range(10^6))", 1000), "status Fuel");
    assert_eq!(env.eval_with("v = range(1.0, 10.0^6); 2.0*v", 1000), "status Fuel");
    let script = "v = range(0.5, 3000.0); w = v*v - 3*v; [sum(w), total([w, v]), length(w), w[17], sum(w[10:2000])]";
    let mut one = Env::with(1, mc_core::Config::stress());
    let mut four = Env::with(4, mc_core::Config::stress());
    let a = one.eval(script);
    assert_eq!(a, four.eval(script));
    assert!(a.starts_with('['), "{a}");
}
