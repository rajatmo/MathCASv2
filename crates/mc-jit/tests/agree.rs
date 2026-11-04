//! The two back ends agree: random HIR programs give the same result and the same exit (done or declined) on the
//! LIR interpreter and on Cranelift's code, for arguments that include the ends of `i64`, signed zeros, infinities
//! and NaN; and every operation that can decline does so on both at the same operands.
use mc_eval::Control;
use mc_jit::ir::{BOp, Cc, Draw, FOp, Func, IOp, Lib, NumOp, Opd, Pair, Ty, E, S};
use mc_jit::ir::NETWORKS;
use mc_jit::rt::{cmath_value, cnum_words, from_order_key, num_words, order_key, Exit, OPD_CEXACT, OPD_COMPLEX, OPD_INT, OPD_RATIO, OPD_REAL};
use mc_random::draw::{normal_ok, real_range_ok};
use mc_random::Stream;
use mc_jit::{build, Backend, Built};

/// A small deterministic generator (xorshift64*).
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
    fn pick<T: Copy>(&mut self, xs: &[T]) -> T {
        xs[self.below(xs.len() as u64) as usize]
    }
    fn int(&mut self) -> i64 {
        if self.below(8) == 0 {
            self.pick(EDGE_INTS)
        } else {
            self.pick(INTS)
        }
    }
}

const INTS: &[i64] = &[0, 1, -1, 2, 3, -7, 10, 5, -2, 4, 12, -3];
/// Values at the ends, used one time in eight.
const EDGE_INTS: &[i64] = &[1 << 31, i64::MAX, i64::MIN, 1 << 53, (1 << 53) + 1, -(1 << 62)];
const REALS: &[f64] = &[0.0, -0.0, 1.0, -1.5, 0.1, 2.5, 1e300, -1e-300, f64::INFINITY, f64::NEG_INFINITY, f64::NAN, 0.75];

// variables: 0, 1 Int parameters; 2, 3 Real parameters; 4 Int local; 5 Real local; 6 the loop variable
const INT_VARS: &[u32] = &[0, 1, 4, 6];
const REAL_VARS: &[u32] = &[2, 3, 5];

fn b(e: E) -> Box<E> {
    Box::new(e)
}

fn int_e(r: &mut Rng, d: u32) -> E {
    if d == 0 || r.below(4) == 0 {
        return if r.below(2) == 0 { E::Var(r.pick(INT_VARS)) } else { E::Int(r.int()) };
    }
    match r.below(5) {
        0 | 1 => {
            let op = r.pick(&[IOp::Add, IOp::Sub, IOp::Mul, IOp::Add, IOp::Sub, IOp::Mul, IOp::DivExact, IOp::Quot, IOp::Mod, IOp::Pow]);
            E::I(op, b(int_e(r, d - 1)), b(int_e(r, d - 1)))
        }
        2 => E::Floor(b(real_e(r, d - 1))),
        3 => E::Sel(b(bool_e(r, d - 1)), b(int_e(r, d - 1)), b(int_e(r, d - 1))),
        _ => E::I(IOp::Add, b(E::Var(r.pick(INT_VARS))), b(E::Int(r.int()))),
    }
}

fn real_e(r: &mut Rng, d: u32) -> E {
    if d == 0 || r.below(4) == 0 {
        return if r.below(2) == 0 { E::Var(r.pick(REAL_VARS)) } else { E::Real(r.pick(REALS)) };
    }
    match r.below(8) {
        0 | 1 => E::F(r.pick(&[FOp::Add, FOp::Sub, FOp::Mul, FOp::Div]), b(real_e(r, d - 1)), b(real_e(r, d - 1))),
        2 => E::ToReal(b(int_e(r, d - 1))),
        3 => {
            if r.below(2) == 0 {
                E::AbsF(b(real_e(r, d - 1)))
            } else {
                E::MinMaxF(r.below(2) == 0, b(real_e(r, d - 1)), b(real_e(r, d - 1)))
            }
        }
        4 => E::Math(r.pick(&Lib::ALL), b(real_e(r, d - 1))),
        5 => {
            // as the front end makes them: at least one machine real
            let n = 1 + r.below(3);
            let mut opds: Vec<Opd> = vec![Opd::Real(real_e(r, d - 1))];
            opds.extend((0..n).map(|_| opd(r, d - 1)));
            E::Num(r.pick(&[NumOp::Sum, NumOp::Product]), opds)
        }
        6 => E::Num(NumOp::Pow, vec![Opd::Real(real_e(r, d - 1)), opd(r, d - 1)]),
        _ => E::Sel(b(bool_e(r, d - 1)), b(real_e(r, d - 1)), b(real_e(r, d - 1))),
    }
}

fn opd(r: &mut Rng, d: u32) -> Opd {
    match r.below(3) {
        0 => Opd::Int(int_e(r, d)),
        1 => Opd::Real(real_e(r, d)),
        _ => Opd::Ratio(int_e(r, d), int_e(r, d)),
    }
}

fn bool_e(r: &mut Rng, d: u32) -> E {
    let c = r.pick(&[Cc::Lt, Cc::Le, Cc::Gt, Cc::Ge, Cc::Eq, Cc::Ne]);
    match r.below(4) {
        0 => E::Cmp(c, b(int_e(r, d.saturating_sub(1))), b(int_e(r, d.saturating_sub(1)))),
        1 => E::Cmp(c, b(real_e(r, d.saturating_sub(1))), b(real_e(r, d.saturating_sub(1)))),
        2 if d > 0 => E::Logic(r.pick(&[BOp::And, BOp::Or]), b(bool_e(r, d - 1)), b(bool_e(r, d - 1))),
        _ if d > 0 => E::Not(b(bool_e(r, d - 1))),
        _ => E::Bool(r.below(2) == 0),
    }
}

fn stmts(r: &mut Rng, d: u32, n: u64) -> Vec<S> {
    (0..n)
        .map(|_| match r.below(if d == 0 { 2 } else { 4 }) {
            0 => S::Set(4, int_e(r, 3)),
            1 => S::Set(5, real_e(r, 3)),
            2 => S::If(bool_e(r, 2), stmts(r, d - 1, 2), stmts(r, d - 1, 1)),
            _ => {
                let lo = r.below(7) as i64 - 3;
                let (hi, step) = (lo + r.below(30) as i64 - 10, r.pick(&[1, -1, 2, -3]));
                S::For { var: 6, lo: E::Int(lo), hi: E::Int(hi), step: E::Int(step), body: stmts(r, d - 1, 2) }
            }
        })
        .collect()
}

fn program(r: &mut Rng, k: usize) -> Func {
    let mut body = vec![S::Set(4, E::Var(0)), S::Set(5, E::Var(2))];
    body.extend(stmts(r, 2, 3));
    let ret = match r.below(3) {
        0 => int_e(r, 3),
        1 => real_e(r, 3),
        _ => bool_e(r, 3),
    };
    Func {
        name: format!("p{k}"),
        vars: vec![Ty::Int, Ty::Int, Ty::Real, Ty::Real, Ty::Int, Ty::Real, Ty::Int],
        params: vec![0, 1, 2, 3],
        body,
        ret,
    }
}

fn run(f: &Built, args: &[u64]) -> (u64, Exit) {
    let ctl = Control::default();
    ctl.fuel.store(i64::MAX / 2, std::sync::atomic::Ordering::Relaxed);
    let mut poll = |_| true;
    f.call(&ctl, args, &mut poll)
}

/// Results compare bitwise, except that every NaN is one value.
fn same(t: Ty, a: (u64, Exit), b: (u64, Exit)) -> bool {
    if a.1 != b.1 {
        return false;
    }
    if a.1 != Exit::Done {
        return true;
    }
    match t {
        Ty::Real => {
            let (x, y) = (f64::from_bits(a.0), f64::from_bits(b.0));
            (x.is_nan() && y.is_nan()) || a.0 == b.0
        }
        _ => a.0 == b.0,
    }
}

#[test]
fn random_programs_agree_on_both_back_ends() {
    let mut r = Rng(0x9e37_79b9_7f4a_7c15);
    let (mut done, mut declined) = (0, 0);
    for k in 0..300 {
        let f = program(&mut r, k);
        let t = mc_jit::ir::type_of(&f, &f.ret).unwrap();
        let interp = build(&f, vec![], Backend::Interpreter).unwrap();
        let jit = build(&f, vec![], Backend::Cranelift).unwrap();
        assert_eq!(jit.info.back_end, Backend::Cranelift, "{:?}", jit.info.fallback);
        for _ in 0..12 {
            let args = [r.int() as u64, r.int() as u64, r.pick(REALS).to_bits(), r.pick(REALS).to_bits()];
            let (a, c) = (run(&interp, &args), run(&jit, &args));
            assert!(same(t, a, c), "program {k} on {args:?}: interpreter {a:?}, cranelift {c:?}\n{f:#?}");
            if a.1 == Exit::Done {
                done += 1;
            } else {
                declined += 1;
            }
        }
    }
    // both outcomes are well represented
    assert!(done > 1000 && declined > 300, "{done} done, {declined} declined");
}

/// Package N0: random programs with a vector result (1-6 output words, written by `S::Out`, also inside loops and
/// branches) give the same words and exit on both back ends; a buffer shorter than the outputs declines.
#[test]
fn random_vector_programs_agree_on_both_back_ends() {
    let mut r = Rng(0x2545_f491_4f6c_dd1d);
    let (mut done, mut declined) = (0, 0);
    for k in 0..200 {
        let mut f = program(&mut r, k);
        let n = 1 + r.below(6) as u32;
        let mut tys = vec![];
        for slot in 0..n {
            let (e, t) = if r.below(2) == 0 { (real_e(&mut r, 2), Ty::Real) } else { (int_e(&mut r, 2), Ty::Int) };
            tys.push(t);
            // some outputs written early and overwritten later, some inside a branch
            match r.below(4) {
                0 => {
                    f.body.insert(2, S::Out(slot, e.clone()));
                    f.body.push(S::Out(slot, e));
                }
                1 => f.body.push(S::If(bool_e(&mut r, 2), vec![S::Out(slot, e.clone())], vec![S::Out(slot, e)])),
                _ => f.body.push(S::Out(slot, e)),
            }
        }
        f.ret = E::Int(0);
        let interp = build(&f, vec![], Backend::Interpreter).unwrap();
        let jit = build(&f, vec![], Backend::Cranelift).unwrap();
        assert_eq!((interp.outputs(), jit.outputs()), (n as usize, n as usize));
        for _ in 0..12 {
            let args = [r.int() as u64, r.int() as u64, r.pick(REALS).to_bits(), r.pick(REALS).to_bits()];
            let call = |b: &Built| {
                let ctl = Control::default();
                ctl.fuel.store(i64::MAX / 2, std::sync::atomic::Ordering::Relaxed);
                let mut out = vec![0u64; n as usize];
                let (_, e) = b.call_out(&ctl, &args, &mut out, &mut |_| true);
                (out, e)
            };
            let ((oa, ea), (oc, ec)) = (call(&interp), call(&jit));
            assert_eq!(ea, ec, "program {k} on {args:?}");
            if ea == Exit::Done {
                done += 1;
                for (i, t) in tys.iter().enumerate() {
                    assert!(same(*t, (oa[i], ea), (oc[i], ec)), "program {k} slot {i} on {args:?}: {oa:?} vs {oc:?}");
                }
            } else {
                declined += 1;
            }
            // too short a buffer: declined before running
            let ctl = Control::default();
            let mut short = vec![0u64; n as usize - 1];
            assert_eq!(jit.call_out(&ctl, &args, &mut short, &mut |_| true).1, Exit::Decline);
        }
    }
    assert!(done > 400 && declined > 100, "{done} done, {declined} declined");
}

/// `f(a, b)` with integer parameters on both back ends: the result, or `None` for a decline.
fn int2(op: impl Fn(E, E) -> E, a: i64, b2: i64) -> [Option<i64>; 2] {
    let f = Func { name: "t".into(), vars: vec![Ty::Int, Ty::Int], params: vec![0, 1], body: vec![], ret: op(E::Var(0), E::Var(1)) };
    [Backend::Interpreter, Backend::Cranelift].map(|be| {
        let built = build(&f, vec![], be).unwrap();
        let (v, exit) = run(&built, &[a as u64, b2 as u64]);
        (exit == Exit::Done).then_some(v as i64)
    })
}

#[test]
fn integer_operations_decline_where_the_result_is_not_an_i64() {
    let i = |op: IOp| move |x, y| E::I(op, b(x), b(y));
    let cases: &[(IOp, i64, i64, Option<i64>)] = &[
        (IOp::Add, i64::MAX, 1, None),
        (IOp::Add, i64::MAX, -1, Some(i64::MAX - 1)),
        (IOp::Sub, i64::MIN, 1, None),
        (IOp::Sub, 0, i64::MIN, None),
        (IOp::Mul, 1 << 32, 1 << 31, None),
        (IOp::Mul, -(1 << 31), 1 << 32, Some(i64::MIN)),
        (IOp::DivExact, 12, -4, Some(-3)),
        (IOp::DivExact, 7, 2, None),
        (IOp::DivExact, 7, 0, None),
        (IOp::DivExact, i64::MIN, -1, None),
        (IOp::Quot, -7, 2, Some(-4)),
        (IOp::Quot, 7, -2, Some(-4)),
        (IOp::Quot, -7, -2, Some(3)),
        (IOp::Quot, 7, 0, None),
        (IOp::Mod, -7, 3, Some(2)),
        (IOp::Mod, 7, -3, Some(-2)),
        (IOp::Mod, -7, -3, Some(-1)),
        (IOp::Mod, 6, -3, Some(0)),
        (IOp::Mod, i64::MIN, -1, None),
        (IOp::Pow, 2, 62, Some(1 << 62)),
        (IOp::Pow, 2, 63, None),
        (IOp::Pow, -2, 63, Some(i64::MIN)),
        (IOp::Pow, 0, 0, Some(1)),
        (IOp::Pow, -1, 1 << 40, Some(1)),
        (IOp::Pow, 3, -1, None),
    ];
    for &(op, x, y, want) in cases {
        assert_eq!(int2(i(op), x, y), [want, want], "{op:?}({x}, {y})");
    }
    let to_real_floor = |x: E, _| E::Floor(b(E::ToReal(b(x))));
    assert_eq!(int2(to_real_floor, 1 << 53, 0), [Some(1 << 53); 2]);
    assert_eq!(int2(to_real_floor, (1 << 53) + 1, 0), [None; 2]);
    assert_eq!(int2(to_real_floor, -(1 << 53) - 1, 0), [None; 2]);
}

#[test]
fn a_stopped_poll_stops_the_loop_on_both_back_ends() {
    // while true { i = i + 1 }: only the poll ends it
    let f = Func {
        name: "spin".into(),
        vars: vec![Ty::Int],
        params: vec![0],
        body: vec![S::While(E::Bool(true), vec![S::Set(0, E::I(IOp::Add, b(E::Var(0)), b(E::Int(1))))])],
        ret: E::Var(0),
    };
    for be in [Backend::Interpreter, Backend::Cranelift] {
        let built = build(&f, vec![], be).unwrap();
        let ctl = Control::default();
        let mut polls = 0;
        let mut poll = |n: u64| {
            assert_eq!(n, 1024);
            polls += 1;
            polls < 10
        };
        let (_, exit) = built.call(&ctl, &[0], &mut poll);
        assert_eq!((exit, polls), (Exit::Stopped, 10), "{be:?}");
        // the control block's stop flag is read at the poll too
        ctl.interrupt();
        let mut never = |_: u64| -> bool { panic!("the stop flag is read first") };
        let (_, exit) = built.call(&ctl, &[0], &mut never);
        assert_eq!(exit, Exit::Stopped, "{be:?}");
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Powers, sums and products of machine reals: the inline code against the arithmetic itself

/// Machine reals that stress powers, sums and products: signed zeros, infinities, NaN (with either sign), the
/// subnormals, the ends of the range, values whose sum cancels, and ones that differ in the last bit.
const SPECIAL: &[f64] = &[
    0.0,
    -0.0,
    1.0,
    -1.0,
    0.5,
    -2.0,
    3.0,
    0.1,
    0.2,
    0.3,
    1e16,
    -1e16,
    1.0000000000000002,
    0.9999999999999999,
    f64::INFINITY,
    f64::NEG_INFINITY,
    f64::NAN,
    -f64::NAN,
    f64::MIN_POSITIVE,
    -f64::MIN_POSITIVE,
    5e-324,
    -5e-324,
    2.225e-308,
    f64::MAX,
    f64::MIN,
    1e300,
    -1e-300,
    1e154,
    1.5e-162,
    9007199254740992.0,
];

/// Integer operands: small values, the edges of exact conversion (2^53) and of `i64`.
const OPD_INTS: &[i64] =
    &[0, 1, -1, 2, 3, -7, 300, 1 << 40, (1 << 53) - 1, 1 << 53, (1 << 53) + 1, -(1 << 53) - 1, 1 << 62, i64::MAX, i64::MIN];

impl Rng {
    fn real(&mut self) -> f64 {
        match self.below(4) {
            // any bit pattern: every binade, subnormals, NaN payloads
            0 => f64::from_bits(self.next()),
            // a moderate value, so that the operands of one sum are comparable and rounding decides
            1 => (self.next() >> 11) as f64 / (1u64 << 53) as f64 * if self.below(2) == 0 { 8.0 } else { -8.0 },
            _ => self.pick(SPECIAL),
        }
    }
    fn opd_int(&mut self) -> i64 {
        if self.below(3) == 0 {
            self.pick(OPD_INTS)
        } else {
            self.below(41) as i64 - 20
        }
    }
}

/// Bitwise equality, except that every NaN is one value (the sign and payload of a NaN are not specified).
fn same_real(x: Option<f64>, y: Option<f64>) -> bool {
    match (x, y) {
        (Some(x), Some(y)) => (x.is_nan() && y.is_nan()) || x.to_bits() == y.to_bits(),
        (None, None) => true,
        _ => false,
    }
}

/// An operand kind of [`check_num`]: a machine real, an integer, a ratio of two integers.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    R,
    I,
    Q,
}

/// `op` of operands of the given kinds, each a parameter (two for a ratio), on both back ends and in the arithmetic's
/// own code (`rt::num_words`, that is `mc_arith::Num`), for `tuples` random arguments. Returns how many results the
/// arithmetic gave and how many it declined.
fn check_num(op: NumOp, kinds: &[Kind], r: &mut Rng, tuples: usize) -> (usize, usize) {
    let mut vars = vec![];
    let mut opds = vec![];
    for &k in kinds {
        let v = vars.len() as u32;
        match k {
            Kind::R => {
                vars.push(Ty::Real);
                opds.push(Opd::Real(E::Var(v)));
            }
            Kind::I => {
                vars.push(Ty::Int);
                opds.push(Opd::Int(E::Var(v)));
            }
            Kind::Q => {
                vars.extend([Ty::Int, Ty::Int]);
                opds.push(Opd::Ratio(E::Var(v), E::Var(v + 1)));
            }
        }
    }
    let params = (0..vars.len() as u32).collect();
    let f = Func { name: "n".into(), vars: vars.clone(), params, body: vec![], ret: E::Num(op, opds) };
    let built = [Backend::Interpreter, Backend::Cranelift].map(|be| build(&f, vec![], be).unwrap());
    let (mut done, mut declined) = (0, 0);
    for _ in 0..tuples {
        let args: Vec<u64> = vars.iter().map(|t| if *t == Ty::Real { r.real().to_bits() } else { r.opd_int() as u64 }).collect();
        // the helper's operand words
        let mut words = vec![];
        let mut at = 0;
        for &k in kinds {
            match k {
                Kind::R => words.extend([OPD_REAL, args[at], 0]),
                Kind::I => words.extend([OPD_INT, args[at], 0]),
                Kind::Q => {
                    words.extend([OPD_RATIO, args[at], args[at + 1]]);
                    at += 1;
                }
            }
            at += 1;
        }
        let want = num_words(op, &words);
        for b in &built {
            let (v, exit) = run(b, &args);
            let got = (exit == Exit::Done).then(|| f64::from_bits(v));
            assert!(same_real(want, got), "{op:?} {kinds:?} on {args:x?}: arithmetic {want:?}, compiled {got:?} ({:?})", b.info.back_end);
        }
        if want.is_some() {
            done += 1;
        } else {
            declined += 1;
        }
    }
    (done, declined)
}

#[test]
fn sums_and_products_are_the_arithmetics() {
    let mut r = Rng(0x2545_f491_4f6c_dd1d);
    // every mix of 1 to 8 machine reals (more than MAX_SORTED go to the helper) with up to two integers and two
    // ratios
    let (mut shapes, mut done, mut declined) = (0, 0, 0);
    for reals in 1..=8 {
        for ints in 0..=2 {
            for ratios in 0..=2 {
                if reals + ints + ratios < 2 {
                    continue;
                }
                let mut kinds = [vec![Kind::R; reals], vec![Kind::I; ints], vec![Kind::Q; ratios]].concat();
                // the operands in a random order: the canonical order must not depend on it
                for k in (1..kinds.len()).rev() {
                    kinds.swap(k, r.below(k as u64 + 1) as usize);
                }
                for op in [NumOp::Sum, NumOp::Product] {
                    let (d, n) = check_num(op, &kinds, &mut r, 400);
                    done += d;
                    declined += n;
                    shapes += 1;
                }
            }
        }
    }
    assert_eq!(shapes, 142);
    assert!(done > 40_000 && declined > 1000, "{done} done, {declined} declined");
}

#[test]
fn powers_are_the_arithmetics() {
    let mut r = Rng(0x0123_4567_89ab_cdef);
    // a machine real to an integer, a machine real, a rational parameter; an integer to a real or a rational
    for kinds in [[Kind::R, Kind::I], [Kind::R, Kind::R], [Kind::R, Kind::Q], [Kind::I, Kind::R], [Kind::I, Kind::Q]] {
        let (done, declined) = check_num(NumOp::Pow, &kinds, &mut r, 20_000);
        // an integer to a rational power is exact (a radical): always declined
        let exact = kinds == [Kind::I, Kind::Q];
        assert!(if exact { done == 0 } else { done > 1000 && declined > 100 }, "{kinds:?}: {done} done, {declined} declined");
    }
    // constant exponents, as the front end makes them: integers, and rationals converted at compile time
    let ints = [1, 2, 3, -1, -2, 5, 17, 0, -1075, 1 << 60, i64::MIN, i64::MAX];
    let ratios = [(1, 2), (1, 3), (-1, 2), (4, 2), (-6, 3), (7, 3), (3, -4), ((1 << 60) + 1, 3), (i64::MIN, -1), (5, 0), (0, 7)];
    let mut exps: Vec<(Opd, [u64; 3])> = ints.iter().map(|&k| (Opd::Int(E::Int(k)), [OPD_INT, k as u64, 0])).collect();
    exps.extend(ratios.iter().map(|&(n, d)| (Opd::Ratio(E::Int(n), E::Int(d)), [OPD_RATIO, n as u64, d as u64])));
    for (e, ew) in exps {
        let ret = E::Num(NumOp::Pow, vec![Opd::Real(E::Var(0)), e.clone()]);
        let f = Func { name: "p".into(), vars: vec![Ty::Real], params: vec![0], body: vec![], ret };
        let built = [Backend::Interpreter, Backend::Cranelift].map(|be| build(&f, vec![], be).unwrap());
        let xs: Vec<f64> = SPECIAL.iter().copied().chain((SPECIAL.len()..2000).map(|_| r.real())).collect();
        for x in xs {
            let want = num_words(NumOp::Pow, &[[OPD_REAL, x.to_bits(), 0], ew].concat());
            for b in &built {
                let (v, exit) = run(b, &[x.to_bits()]);
                let got = (exit == Exit::Done).then(|| f64::from_bits(v));
                assert!(same_real(want, got), "{x:e}^{e:?}: arithmetic {want:?}, compiled {got:?} ({:?})", b.info.back_end);
            }
        }
    }
}

#[test]
fn sorting_networks_sort() {
    // the 0-1 principle: a comparator network sorts every input if it sorts every sequence of zeros and ones
    for (n, net) in NETWORKS.iter().enumerate().skip(2) {
        for bits in 0u32..1 << n {
            let mut v: Vec<u32> = (0..n).map(|i| (bits >> i) & 1).collect();
            for &(i, j) in *net {
                if v[i] > v[j] {
                    v.swap(i, j);
                }
            }
            assert!(v.windows(2).all(|w| w[0] <= w[1]), "network {n} on {bits:b}");
        }
    }
}

#[test]
fn order_keys_follow_the_canonical_order() {
    let mut r = Rng(77);
    let xs: Vec<f64> = SPECIAL.iter().copied().chain((SPECIAL.len()..3000).map(|_| r.real())).collect();
    for &x in &xs {
        assert_eq!(from_order_key(order_key(x)).to_bits(), x.to_bits());
        for &y in xs.iter().take(200) {
            // IEEE total order, which is the canonical order on numbers (NaNs aside, see `order_key`)
            if !x.is_nan() && !y.is_nan() {
                assert_eq!(order_key(x).cmp(&order_key(y)), x.total_cmp(&y), "{x:e} {y:e}");
            }
        }
    }
}

/// An operand kind of [`check_cnum`].
#[derive(Clone, Copy, Debug, PartialEq)]
enum CKind {
    /// a machine complex number (two real parameters)
    C,
    /// a machine real
    R,
    /// an integer
    I,
    /// an exact complex constant with integer parts
    X(i64, i64),
}

/// The pair `(re, im)` a function with two outputs wrote, or `None` when it declined.
fn run_pair(f: &Built, args: &[u64]) -> Option<(f64, f64)> {
    let ctl = Control::default();
    ctl.fuel.store(i64::MAX / 2, std::sync::atomic::Ordering::Relaxed);
    let mut out = [0u64; 2];
    let (_, exit) = f.call_out(&ctl, args, &mut out, &mut |_| true);
    (exit == Exit::Done).then(|| (f64::from_bits(out[0]), f64::from_bits(out[1])))
}

fn same_pair(x: Option<(f64, f64)>, y: Option<(f64, f64)>) -> bool {
    match (x, y) {
        (Some(a), Some(b)) => same_real(Some(a.0), Some(b.0)) && same_real(Some(a.1), Some(b.1)),
        (None, None) => true,
        _ => false,
    }
}

/// `op` of operands of the kinds `kinds` as an [`S::Pair`] on both back ends against the arithmetic
/// (`rt::cnum_words`, that is `mc_arith::Num`), for `tuples` random arguments; returns (done, declined).
fn check_cnum(op: NumOp, kinds: &[CKind], r: &mut Rng, tuples: usize) -> (usize, usize) {
    let mut vars = vec![];
    let mut opds = vec![];
    for &k in kinds {
        let v = vars.len() as u32;
        match k {
            CKind::C => {
                vars.extend([Ty::Real, Ty::Real]);
                opds.push(Opd::Complex(E::Var(v), E::Var(v + 1)));
            }
            CKind::R => {
                vars.push(Ty::Real);
                opds.push(Opd::Real(E::Var(v)));
            }
            CKind::I => {
                vars.push(Ty::Int);
                opds.push(Opd::Int(E::Var(v)));
            }
            CKind::X(a, b) => opds.push(Opd::CExact(E::Int(a), E::Int(b))),
        }
    }
    let np = vars.len() as u32;
    vars.extend([Ty::Real, Ty::Real]);
    let params = (0..np).collect();
    let body = vec![S::Pair(np, np + 1, Pair::Num(op, opds)), S::Out(0, E::Var(np)), S::Out(1, E::Var(np + 1))];
    let f = Func { name: "c".into(), vars: vars.clone(), params, body, ret: E::Int(0) };
    let built = [Backend::Interpreter, Backend::Cranelift].map(|be| build(&f, vec![], be).unwrap());
    let (mut done, mut declined) = (0, 0);
    for _ in 0..tuples {
        let args: Vec<u64> =
            vars[..np as usize].iter().map(|t| if *t == Ty::Real { r.real().to_bits() } else { r.opd_int() as u64 }).collect();
        let mut words = vec![];
        let mut at = 0;
        for &k in kinds {
            match k {
                CKind::C => {
                    words.extend([OPD_COMPLEX, args[at], args[at + 1]]);
                    at += 2;
                }
                CKind::R => {
                    words.extend([OPD_REAL, args[at], 0]);
                    at += 1;
                }
                CKind::I => {
                    words.extend([OPD_INT, args[at], 0]);
                    at += 1;
                }
                CKind::X(a, b) => words.extend([OPD_CEXACT, a as u64, b as u64]),
            }
        }
        let want = cnum_words(op, &words);
        for b in &built {
            let got = run_pair(b, &args);
            assert!(same_pair(want, got), "{op:?} {kinds:?} on {args:x?}: arithmetic {want:?}, compiled {got:?} ({:?})", b.info.back_end);
        }
        if want.is_some() {
            done += 1;
        } else {
            declined += 1;
        }
    }
    (done, declined)
}

#[test]
fn complex_pairs_are_the_arithmetics() {
    let mut r = Rng(0x5eed_c0de_1234_5678);
    use CKind::{C, I, R, X};
    let shapes: &[&[CKind]] = &[
        &[C, C],
        &[C, R],
        &[R, C],
        &[C, I],
        &[C, C, C],
        &[C, R, C, I],
        &[X(0, 1), R, R],
        &[X(2, -3), C, C],
        &[C, C, C, C, C],
    ];
    let (mut done, mut declined) = (0, 0);
    for kinds in shapes {
        for op in [NumOp::Sum, NumOp::Product] {
            let (d, n) = check_cnum(op, kinds, &mut r, 500);
            done += d;
            declined += n;
        }
    }
    // powers: complex bases to integer, real and complex exponents; real and integer bases to complex exponents
    for kinds in [[C, I], [C, R], [C, C], [R, C], [I, C], [C, X(0, 1)]] {
        let (d, n) = check_cnum(NumOp::Pow, &kinds, &mut r, 2000);
        done += d;
        declined += n;
    }
    assert!(done > 10_000 && declined > 100, "{done} done, {declined} declined");
}

#[test]
fn complex_functions_are_mc_numerics() {
    let mut r = Rng(31337);
    for f in Lib::ALL {
        let body = vec![S::Pair(2, 3, Pair::Math(f, E::Var(0), E::Var(1))), S::Out(0, E::Var(2)), S::Out(1, E::Var(3))];
        let func = Func { name: "m".into(), vars: vec![Ty::Real; 4], params: vec![0, 1], body, ret: E::Int(0) };
        let built = [Backend::Interpreter, Backend::Cranelift].map(|be| build(&func, vec![], be).unwrap());
        for _ in 0..2000 {
            let (x, y) = (r.real(), r.real());
            let want = Some(cmath_value(f, x, y));
            for bt in &built {
                let got = run_pair(bt, &[x.to_bits(), y.to_bits()]);
                assert!(same_pair(want, got), "{f:?} at {x:e} {y:e}: {want:?} vs {got:?} ({:?})", bt.info.back_end);
            }
        }
    }
}

#[test]
fn negation_flips_the_sign() {
    let f = Func { name: "n".into(), vars: vec![Ty::Real], params: vec![0], body: vec![], ret: E::NegF(b(E::Var(0))) };
    let built = [Backend::Interpreter, Backend::Cranelift].map(|be| build(&f, vec![], be).unwrap());
    for &x in SPECIAL {
        for bt in &built {
            let (v, exit) = run(bt, &[x.to_bits()]);
            assert_eq!(exit, Exit::Done);
            assert!(same_real(Some(-x), Some(f64::from_bits(v))), "-{x:e}");
            if !x.is_nan() {
                assert_eq!(v, (-x).to_bits());
            }
        }
    }
}

#[test]
fn draws_are_the_streams() {
    // a loop drawing reals, normals and integers with arguments that include ranges the natives refuse: the values
    // are the stream's own conversions, and a refused range declines
    let mut r = Rng(99);
    // params a, b (reals), lo, hi, n (ints); locals: the sums s (reals), t (normals), u (integers), the loop variable
    let vars = vec![Ty::Real, Ty::Real, Ty::Int, Ty::Int, Ty::Int, Ty::Real, Ty::Real, Ty::Int, Ty::Int];
    let draw = |d: Draw, x: u32, y: u32| E::Draw(d, b(E::Var(x)), b(E::Var(y)));
    let body = vec![
        S::For {
            var: 8,
            lo: E::Int(1),
            hi: E::Var(4),
            step: E::Int(1),
            body: vec![
                S::Set(5, E::F(FOp::Add, b(E::Var(5)), b(draw(Draw::Real, 0, 1)))),
                S::Set(6, E::F(FOp::Add, b(E::Var(6)), b(draw(Draw::Normal, 0, 1)))),
                S::Set(7, E::I(IOp::Add, b(E::Var(7)), b(draw(Draw::Integer, 2, 3)))),
            ],
        },
        S::Out(0, E::Var(5)),
        S::Out(1, E::Var(6)),
        S::Out(2, E::Var(7)),
    ];
    let f = Func { name: "d".into(), vars, params: vec![0, 1, 2, 3, 4], body, ret: E::Int(0) };
    let built = [Backend::Interpreter, Backend::Cranelift].map(|be| build(&f, vec![], be).unwrap());
    assert!(built.iter().all(|bt| bt.draws()));
    let reals = [0.0, 1.0, -2.5, 3.0, f64::INFINITY, f64::NAN, -0.0, 1e300, 0.5];
    let ints = [0, 1, 6, -3, i64::MIN, i64::MAX, 100];
    let (mut done, mut declined) = (0, 0);
    for case in 0..600u64 {
        let (a, bb) = (r.pick(&reals), r.pick(&reals));
        let (lo, hi) = (r.pick(&ints), r.pick(&ints));
        let n = r.below(20) as i64;
        // the expected values from the stream itself; `None` where a draw is refused or the integer sum overflows
        let mut s = Stream::from_key(case);
        let want = (|| {
            let (mut x, mut y, mut z) = (0.0f64, 0.0f64, 0i64);
            for _ in 0..n {
                if !(real_range_ok(a, bb) && normal_ok(a, bb) && lo <= hi) {
                    return None;
                }
                x += s.real_in(a, bb - a);
                y += s.normal_scaled(a, bb);
                z = z.checked_add(s.int_in(lo, hi))?;
            }
            Some((x, y, z))
        })();
        let args = [a.to_bits(), bb.to_bits(), lo as u64, hi as u64, n as u64];
        for bt in &built {
            let mut st = Stream::from_key(case);
            let ctl = Control::default();
            ctl.fuel.store(i64::MAX / 2, std::sync::atomic::Ordering::Relaxed);
            let mut out = [0u64; 3];
            let (_, exit) = bt.call_with(&ctl, &args, &mut out, Some(&mut st), &mut |_| true);
            match want {
                Some((x, y, z)) => {
                    assert_eq!(exit, Exit::Done, "{case}");
                    assert!(same_real(Some(x), Some(f64::from_bits(out[0]))), "{case}: reals");
                    assert!(same_real(Some(y), Some(f64::from_bits(out[1]))), "{case}: normals");
                    assert_eq!(z, out[2] as i64, "{case}: integers");
                    assert_eq!(st, s, "{case}: the stream moved as far as the draws");
                    done += 1;
                }
                None => {
                    assert_eq!(exit, Exit::Decline, "{case}");
                    declined += 1;
                }
            }
            // without a stream a draw declines
            if n > 0 {
                let (_, exit) = bt.call_with(&ctl, &args, &mut out, None, &mut |_| true);
                assert_eq!(exit, Exit::Decline);
            }
        }
    }
    assert!(done > 100 && declined > 100, "{done} done, {declined} declined");
}
