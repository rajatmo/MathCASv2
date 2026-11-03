//! The two back ends agree: random HIR programs give the same result and the same exit (done or declined) on the
//! LIR interpreter and on Cranelift's code, for arguments that include the ends of `i64`, signed zeros, infinities
//! and NaN; and every operation that can decline does so on both at the same operands.
use mc_eval::Control;
use mc_jit::ir::{BOp, Cc, FOp, Func, IOp, Lib, NumOp, Opd, Ty, E, S};
use mc_jit::rt::Exit;
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
