//! Invariants: every back end gives the same bits; failures (index, division, fuel, interrupt) are reported the same
//! way; parallel loops give the same result for any number of threads.
use jit::interp::Interp;
use jit::ir::{cplx, f, i, lower, sel, v, E, FnB, Func, Ty};
use jit::jit::Jit;
use jit::kernels as k;
use jit::run::*;
use jit::term::{TVal, TermFn};
use jit::{fp_f64, fp_i64, native, Ctl, Poll, Status};
use std::sync::atomic::Ordering;

const POLL: Poll = Poll::Every(1024);

fn pools() -> Vec<rayon::ThreadPool> {
    [1, 2, 4].iter().map(|&n| rayon::ThreadPoolBuilder::new().num_threads(n).build().unwrap()).collect()
}

#[test]
fn l1_scalar_and_table_agree_bitwise() {
    let mut jit = Jit::new("speed");
    let ps = prepare(&mut jit, &k::scalar_fn("l1", k::l1_expr), POLL);
    let pt = prepare(&mut jit, &k::table_fn("l1_table", k::l1_expr), POLL);
    let mut tf = TermFn::new(&k::scalar_fn("l1", k::l1_expr)).unwrap();
    let ctl = Ctl::unlimited();
    let xs = k::l1_points(5000);
    let want: Vec<f64> = xs.iter().map(|&x| native::l1(x)).collect();
    let mut it = ps.interp.clone();
    let fj = scalar_jit(&ps.jit);
    for &x in xs.iter().step_by(7) {
        let n = native::l1(x).to_bits();
        assert_eq!(it.call(&[x.to_bits()], &ctl), n, "interp at {x}");
        assert_eq!(fj(x, &ctl).to_bits(), n, "jit at {x}");
        assert_eq!(ps.jit.call(&[x.to_bits()], &ctl), n, "jit entry at {x}");
        assert_eq!(term_scalar(&mut tf, x, &ctl).to_bits(), n, "term at {x}");
    }
    let mut out = vec![0.0; xs.len()];
    table_interp(&mut pt.interp.clone(), &xs, &mut out, &ctl);
    assert_eq!(fp_f64(&out), fp_f64(&want));
    out.fill(0.0);
    table_jit(&pt.jit, &xs, &mut out, &ctl);
    assert_eq!(fp_f64(&out), fp_f64(&want));
    for p in pools() {
        out.fill(0.0);
        p.install(|| table_par(&pt.jit, &xs, &mut out, 333, &ctl));
        assert_eq!(fp_f64(&out), fp_f64(&want));
        out.fill(0.0);
        p.install(|| table_interp_par(&pt.interp, &xs, &mut out, 333, &ctl));
        assert_eq!(fp_f64(&out), fp_f64(&want));
    }
    assert_eq!(ctl.status(), Status::Ok);
}

#[test]
fn mandelbrot_agrees_on_every_back_end() {
    let n = 48i64;
    let mut jit = Jit::new("speed");
    let pl = prepare(&mut jit, &k::mandel_loop(), POLL);
    let pe = prepare(&mut jit, &k::mandel_scalar(), POLL);
    let ctl = Ctl::unlimited();
    let mut want = vec![0i64; (n * n) as usize];
    native::mandel(&mut want, n, 0);
    let mut out = vec![0i64; want.len()];
    mandel_interp(&mut pl.interp.clone(), &mut out, n, 0, &ctl);
    assert_eq!(out, want);
    out.fill(0);
    mandel_jit(&pl.jit, &mut out, n, 0, &ctl);
    assert_eq!(out, want);
    for p in pools() {
        out.fill(0);
        p.install(|| mandel_par(&pl.jit, &mut out, n, &ctl));
        assert_eq!(fp_i64(&out), fp_i64(&want));
    }
    // per-pixel scalar escape: jit, interp, term, hand-written x86-64
    let fe: EscapeFn = unsafe { typed(pe.jit.code) };
    escape_per_pixel(&mut out, n, |a, b| fe(a, b, &ctl));
    assert_eq!(out, want);
    let mut it = pe.interp.clone();
    escape_per_pixel(&mut out, n, |a, b| it.call(&[a.to_bits(), b.to_bits()], &ctl) as i64);
    assert_eq!(out, want);
    let mut tf = TermFn::new(&k::mandel_scalar()).unwrap();
    escape_per_pixel(&mut out, n, |a, b| term_escape(&mut tf, a, b, &ctl));
    assert_eq!(out, want);
    if let Some(h) = jit::x64::HandEscape::new(1024) {
        let hf = h.func();
        out.fill(0);
        escape_per_pixel(&mut out, n, |a, b| hf(a, b, &ctl));
        assert_eq!(out, want);
    }
    assert_eq!(ctl.status(), Status::Ok);
}

#[test]
fn lorenz_agrees_host_driven_and_compiled() {
    let steps = 300;
    let mut jit = Jit::new("speed");
    let pr = prepare(&mut jit, &k::lorenz_rhs(), POLL);
    let pk = prepare(&mut jit, &k::lorenz_rk4_loop(), POLL);
    let ctl = Ctl::unlimited();
    let want = rk4_native(steps);
    let mut it = pr.interp.clone();
    assert_eq!(k::rk4_host(k::LORENZ_Y0, steps, k::LORENZ_DT, lorenz_interp_rhs(&mut it, &ctl)), want);
    assert_eq!(k::rk4_host(k::LORENZ_Y0, steps, k::LORENZ_DT, lorenz_jit_rhs(&pr.jit, &ctl)), want);
    let mut tf = TermFn::new(&k::lorenz_rhs()).unwrap();
    assert_eq!(k::rk4_host(k::LORENZ_Y0, steps, k::LORENZ_DT, lorenz_term_rhs(&mut tf, &ctl)), want);
    assert_eq!(rk4_jit(&pk.jit, steps, &ctl), want);
    assert_eq!(rk4_interp(&mut pk.interp.clone(), steps, &ctl), want);
    assert_eq!(ctl.status(), Status::Ok);
}

#[test]
fn newton_agrees_and_converges() {
    let mut jit = Jit::new("speed");
    let pf = prepare(&mut jit, &k::newton_f(), POLL);
    let pd = prepare(&mut jit, &k::newton_df(), POLL);
    let pl = prepare(&mut jit, &k::newton_loop(), POLL);
    let ctl = Ctl::unlimited();
    let as_ = k::newton_params(500);
    let mut want = vec![0.0; as_.len()];
    native::newton_solve(&as_, &mut want);
    for (&a, &x) in as_.iter().zip(&want) {
        assert!((x * x.exp() - a).abs() <= 1e-13 * a.max(1.0), "residual at a = {a}");
    }
    let mut out = vec![0.0; as_.len()];
    newton_native_host(&as_, &mut out);
    assert_eq!(fp_f64(&out), fp_f64(&want));
    newton_jit_host(&pf.jit, &pd.jit, &as_, &mut out, &ctl);
    assert_eq!(fp_f64(&out), fp_f64(&want));
    newton_interp_host(&mut pf.interp.clone(), &mut pd.interp.clone(), &as_, &mut out, &ctl);
    assert_eq!(fp_f64(&out), fp_f64(&want));
    let (mut tf, mut td) = (TermFn::new(&k::newton_f()).unwrap(), TermFn::new(&k::newton_df()).unwrap());
    newton_term_host(&mut tf, &mut td, &as_[..50], &mut out[..50], &ctl);
    assert_eq!(fp_f64(&out[..50]), fp_f64(&want[..50]));
    out.fill(0.0);
    table_jit(&pl.jit, &as_, &mut out, &ctl);
    assert_eq!(fp_f64(&out), fp_f64(&want));
    out.fill(0.0);
    table_interp(&mut pl.interp.clone(), &as_, &mut out, &ctl);
    assert_eq!(fp_f64(&out), fp_f64(&want));
    for p in pools() {
        out.fill(0.0);
        p.install(|| table_par(&pl.jit, &as_, &mut out, 37, &ctl));
        assert_eq!(fp_f64(&out), fp_f64(&want));
    }
}

#[test]
fn monte_carlo_counts_agree_for_any_threads_and_chunks() {
    let mut jit = Jit::new("speed");
    let pm = prepare(&mut jit, &k::mc_loop(), POLL);
    let ctl = Ctl::unlimited();
    let n = 200_000u64;
    let want = native::monte_carlo(MC_SEED, 0, n);
    assert_eq!(mc_jit(&pm.jit, MC_SEED, 0, n, &ctl), want);
    assert_eq!(mc_interp(&mut pm.interp.clone(), MC_SEED, 0, n, &ctl), want);
    let mut tf = TermFn::new(&k::mc_loop()).unwrap();
    assert_eq!(mc_term(&mut tf, MC_SEED, 0, 3000, &ctl), native::monte_carlo(MC_SEED, 0, 3000));
    for p in pools() {
        for chunk in [1000, 4096, 77_777] {
            assert_eq!(p.install(|| mc_par(&pm.jit, MC_SEED, n, chunk, &ctl)), want);
        }
    }
    let pi = 4.0 * want as f64 / n as f64;
    assert!((pi - std::f64::consts::PI).abs() < 0.02);
}

/// `while true { k = k + 1 }; k`
fn forever() -> Func {
    let mut b = FnB::new("forever");
    let kk = b.var(Ty::I64);
    b.while_(E::B(true), |b| b.set(kk, v(kk) + i(1)));
    b.finish(Some(v(kk)))
}

#[test]
fn loops_are_interruptible_and_fuel_limited() {
    let mut jit = Jit::new("speed");
    for poll in [Poll::Every(1), Poll::Every(1024)] {
        let p = prepare(&mut jit, &forever(), poll);
        // fuel
        let ctl = Ctl::new(1_000_000);
        assert_eq!(p.jit.call(&[], &ctl), 0);
        assert_eq!(ctl.status(), Status::Fuel);
        let ctl = Ctl::new(1_000_000);
        p.interp.clone().call(&[], &ctl);
        assert_eq!(ctl.status(), Status::Fuel);
        // interrupt set before the call
        let ctl = Ctl::unlimited();
        ctl.stop.store(1, Ordering::Relaxed);
        p.jit.call(&[], &ctl);
        assert_eq!(ctl.status(), Status::Interrupted);
        // interrupt from another thread while running
        for back in 0..2 {
            let ctl = Ctl::unlimited();
            std::thread::scope(|s| {
                let h = s.spawn(|| {
                    if back == 0 {
                        p.jit.call(&[], &ctl);
                    } else {
                        p.interp.clone().call(&[], &ctl);
                    }
                });
                let t0 = std::time::Instant::now();
                while t0.elapsed().as_millis() < 20 {
                    std::hint::spin_loop();
                }
                ctl.stop.store(1, Ordering::Relaxed);
                h.join().unwrap();
            });
            assert_eq!(ctl.status(), Status::Interrupted);
        }
    }
    // the term-level evaluator obeys the same control block
    let mut tf = TermFn::new(&forever()).unwrap();
    let ctl = Ctl::new(100_000);
    assert_eq!(tf.call(&[], &ctl).unwrap_err(), Status::Fuel);
    // one fuel budget shared by parallel tasks
    let pm = prepare(&mut jit, &k::mc_loop(), POLL);
    let ctl = Ctl::new(1_000_000);
    pools()[2].install(|| mc_par(&pm.jit, MC_SEED, 100_000_000, 1 << 16, &ctl));
    assert_eq!(ctl.status(), Status::Fuel);
}

#[test]
fn bounds_are_checked() {
    let mut jit = Jit::new("speed");
    let p = prepare(&mut jit, &k::table_fn("t", k::l1_expr), POLL);
    let xs = k::l1_points(100);
    let mut out = vec![0.0; 60];
    let ctl = Ctl::unlimited();
    table_jit(&p.jit, &xs, &mut out, &ctl);
    assert_eq!(ctl.status(), Status::Index);
    let ctl = Ctl::unlimited();
    table_interp(&mut p.interp.clone(), &xs, &mut out, &ctl);
    assert_eq!(ctl.status(), Status::Index);
    assert_eq!(out[59], native::l1(xs[59]));
}

fn int2(name: &str, body: impl Fn(E, E) -> E) -> Func {
    let mut b = FnB::new(name);
    let x = b.param(Ty::I64);
    let y = b.param(Ty::I64);
    b.finish(Some(body(v(x), v(y))))
}

#[test]
fn integer_division_wraps_and_fails_on_zero() {
    let mut jit = Jit::new("speed");
    let pd = prepare(&mut jit, &int2("div", |a, b| a / b), POLL);
    let pr = prepare(&mut jit, &int2("rem", |a, b| a.rem_(b)), POLL);
    let cases = [(7, 2), (-7, 2), (i64::MIN, -1), (i64::MIN, 1), (5, -1), (0, 3), (i64::MAX, -7)];
    for (a, b) in cases {
        let ctl = Ctl::unlimited();
        let args = [a as u64, b as u64];
        assert_eq!(pd.jit.call(&args, &ctl) as i64, a.wrapping_div(b));
        assert_eq!(pd.interp.clone().call(&args, &ctl) as i64, a.wrapping_div(b));
        assert_eq!(pr.jit.call(&args, &ctl) as i64, a.wrapping_rem(b));
        assert_eq!(pr.interp.clone().call(&args, &ctl) as i64, a.wrapping_rem(b));
        assert_eq!(ctl.status(), Status::Ok);
    }
    let ctl = Ctl::unlimited();
    pd.jit.call(&[3, 0], &ctl);
    assert_eq!(ctl.status(), Status::DivZero);
    let ctl = Ctl::unlimited();
    pr.interp.clone().call(&[3, 0], &ctl);
    assert_eq!(ctl.status(), Status::DivZero);
}

#[test]
fn checker_accepts_only_monomorphic_machine_types() {
    let mut b = FnB::new("mixed");
    let x = b.param(Ty::F64);
    let bad = b.finish(Some(v(x) + i(1)));
    assert!(lower(&bad).is_err(), "no implicit integer-to-real conversion");
    let mut b = FnB::new("cret");
    let x = b.param(Ty::F64);
    let bad = b.finish(Some(cplx(v(x), v(x))));
    assert!(lower(&bad).is_err(), "complex results go through arrays");
    let mut b = FnB::new("bparam");
    b.param(Ty::Bool);
    assert!(lower(&b.finish(None)).is_err());
}

/// Small deterministic generator for random programs.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

fn rand_f(r: &mut Rng, d: u32, x: &E, y: &E, n: &E) -> E {
    if d == 0 || r.below(5) == 0 {
        return match r.below(4) {
            0 => x.clone(),
            1 => y.clone(),
            2 => f([0.5, -1.25, 3.0, 0.0, 1e300, -0.0][r.below(6) as usize]),
            _ => n.clone().itof(),
        };
    }
    let g = |r: &mut Rng| rand_f(r, d - 1, x, y, n);
    match r.below(16) {
        0 => g(r) + g(r),
        1 => g(r) - g(r),
        2 => g(r) * g(r),
        3 => g(r) / g(r),
        4 => -g(r),
        5 => g(r).abs().sqrt(),
        6 => g(r).floor(),
        7 => g(r).sin(),
        8 => g(r).cos(),
        9 => g(r).exp(),
        10 => g(r).abs().log(),
        11 => g(r).pow(g(r)),
        12 => g(r).fma(g(r), g(r)),
        13 => sel(g(r).lt(g(r)).or(g(r).ne_(g(r))), g(r), g(r)),
        14 => cplx(g(r), g(r)).powi(r.below(5) as u32).abs2(),
        _ => rand_i(r, d - 1, x, y, n).itof(),
    }
}

fn rand_i(r: &mut Rng, d: u32, x: &E, y: &E, n: &E) -> E {
    if d == 0 || r.below(5) == 0 {
        return match r.below(4) {
            0 => n.clone(),
            1 => i([3, -1, i64::MIN, 0x5DEECE66D][r.below(4) as usize]),
            2 => x.clone().ftoi(),
            _ => i(r.below(100) as i64 - 50),
        };
    }
    let g = |r: &mut Rng| rand_i(r, d - 1, x, y, n);
    match r.below(10) {
        0 => g(r) + g(r),
        1 => g(r) * g(r),
        2 => g(r).xor(g(r)),
        3 => g(r).shl_(g(r)),
        4 => g(r).ushr(g(r)),
        5 => g(r).shr_(g(r)),
        6 => g(r) / g(r),
        7 => g(r).rem_(g(r)),
        8 => rand_f(r, d - 1, x, y, n).ftoi(),
        _ => -g(r),
    }
}

fn canon(bits: u64) -> u64 {
    if f64::from_bits(bits).is_nan() {
        u64::MAX
    } else {
        bits
    }
}

#[test]
fn random_programs_agree_across_back_ends() {
    let mut jit = Jit::new("speed");
    let mut r = Rng(0x1234_5678_9abc_def1);
    let mut failures = 0;
    for case in 0..300 {
        let mut b = FnB::new(&format!("rand{case}"));
        let x = b.param(Ty::F64);
        let y = b.param(Ty::F64);
        let n = b.param(Ty::I64);
        let real = case % 2 == 0;
        let body = if real {
            rand_f(&mut r, 5, &v(x), &v(y), &v(n))
        } else {
            rand_i(&mut r, 5, &v(x), &v(y), &v(n))
        };
        let func = b.finish(Some(body));
        let p = prepare(&mut jit, &func, POLL);
        let mut it = p.interp.clone();
        let mut tf = TermFn::new(&func).unwrap();
        for _ in 0..8 {
            let xv = (r.below(2001) as f64 - 1000.0) / 64.0;
            let yv = f64::from_bits(r.next() >> 2);
            let nv = r.below(200) as i64 - 100;
            let args = [xv.to_bits(), yv.to_bits(), nv as u64];
            let (c1, c2, c3) = (Ctl::unlimited(), Ctl::unlimited(), Ctl::unlimited());
            let a = p.jit.call(&args, &c1);
            let bb = it.call(&args, &c2);
            let t = tf.call(&[TVal::F(xv), TVal::F(yv), TVal::I(nv)], &c3);
            assert_eq!(c1.status(), c2.status(), "status, case {case}");
            if c1.status() != Status::Ok {
                failures += 1;
                assert_eq!(t.unwrap_err(), c1.status());
                continue;
            }
            let tb = match t.unwrap().0.unwrap() {
                TVal::F(z) => z.to_bits(),
                TVal::I(z) => z as u64,
                TVal::List(_) => unreachable!(),
            };
            if real {
                assert_eq!(canon(a), canon(bb), "jit vs interp, case {case}: {func:?}");
                assert_eq!(canon(a), canon(tb), "jit vs term, case {case}");
            } else {
                assert_eq!(a, bb, "jit vs interp, case {case}: {func:?}");
                assert_eq!(a, tb, "jit vs term, case {case}");
            }
        }
    }
    assert!(failures > 0, "division by zero should occur in some random integer programs");
}

#[test]
fn explicit_fma_is_one_rounding_everywhere() {
    let mut jit = Jit::new("speed");
    let mut b = FnB::new("fma");
    let x = b.param(Ty::F64);
    let y = b.param(Ty::F64);
    let func = b.finish(Some(v(x).fma(v(y), -(v(x) * v(y)))));
    let p = prepare(&mut jit, &func, POLL);
    let ctl = Ctl::unlimited();
    let (xv, yv) = (0.1f64, 10.0f64 / 3.0);
    let want = xv.mul_add(yv, -(xv * yv));
    assert_ne!(want, 0.0, "the fused residual of a product is its rounding error");
    assert_eq!(p.jit.call(&[xv.to_bits(), yv.to_bits()], &ctl), want.to_bits());
    assert_eq!(p.interp.clone().call(&[xv.to_bits(), yv.to_bits()], &ctl), want.to_bits());
}

#[test]
fn interpreter_and_jit_agree_for_complex_division_and_conjugate() {
    let mut jit = Jit::new("speed");
    let mut b = FnB::new("cdiv");
    let x = b.param(Ty::F64);
    let y = b.param(Ty::F64);
    let z = b.var(Ty::C64);
    b.set(z, cplx(v(x), v(y)) / cplx(v(y), f(1.5)));
    b.set(z, cplx(v(z).im(), v(z).re())); // swap through temporaries
    b.set(z, v(z).conj() * v(z));
    let func = b.finish(Some(v(z).re() + f(3.0) * v(z).im()));
    let p = prepare(&mut jit, &func, POLL);
    let mut tf = TermFn::new(&func).unwrap();
    let ctl = Ctl::unlimited();
    for (xv, yv) in [(1.0, 2.0), (-3.5, 0.25), (1e-300, 7.0)] {
        let a = p.jit.call(&[f64::to_bits(xv), f64::to_bits(yv)], &ctl);
        assert_eq!(a, p.interp.clone().call(&[f64::to_bits(xv), f64::to_bits(yv)], &ctl));
        let Ok((Some(TVal::F(t)), _)) = tf.call(&[TVal::F(xv), TVal::F(yv)], &ctl) else { panic!() };
        assert_eq!(a, t.to_bits());
        // reference in plain Rust with the lowering's operation order
        let (ar, ai, br, bi) = (xv, yv, yv, 1.5);
        let d = br * br + bi * bi;
        let (qr, qi) = ((ar * br + ai * bi) / d, (ai * br - ar * bi) / d);
        let (zr, zi) = (qi, qr);
        let (cr, ci) = (zr, -zi);
        let (pr, pi) = (cr * zr - ci * zi, cr * zi + ci * zr);
        assert_eq!(a, (pr + 3.0 * pi).to_bits());
    }
}

#[test]
fn interp_reuse_is_clean() {
    // registers are reset between calls
    let mut b = FnB::new("acc");
    let x = b.param(Ty::F64);
    let s = b.var(Ty::F64);
    b.set(s, v(s) + v(x));
    let func = b.finish(Some(v(s)));
    let lir = lower(&func).unwrap();
    let mut it = Interp::new(&lir, POLL);
    let ctl = Ctl::unlimited();
    assert_eq!(it.call_f(&[2.0f64.to_bits()], &ctl), 2.0);
    assert_eq!(it.call_f(&[2.0f64.to_bits()], &ctl), 2.0);
}

#[test]
fn if_conversion_keeps_results_and_never_speculates_a_failing_operation() {
    use jit::ir::{len, load, lower_with, Elem, Op, Opts};
    let off = Opts { if_convert: false };
    let branches = |l: &jit::ir::Lir| l.code.iter().filter(|o| matches!(o, Op::Br(..))).count();
    let (a, b) = (lower_with(&k::mc_loop(), off).unwrap(), lower(&k::mc_loop()).unwrap());
    assert_eq!(branches(&b), branches(&a) - 1, "the counting branch became a select");
    // a guarded load must stay a branch: speculating it would fail with an index error
    let mut fb = FnB::new("guarded");
    let arr = fb.param(Ty::Arr(Elem::F64, 1));
    let ix = fb.param(Ty::I64);
    let s = fb.var(Ty::F64);
    fb.if_(v(ix).lt(len(arr, 0)), |b| b.set(s, load(arr, vec![v(ix)])), |_| {});
    let func = fb.finish(Some(v(s)));
    let mut jit = Jit::new("speed");
    let p = prepare(&mut jit, &func, POLL);
    let xs = [1.5f64, 2.5];
    let ctl = Ctl::unlimited();
    assert_eq!(p.jit.call(&[xs.as_ptr() as u64, 2, 7], &ctl), 0f64.to_bits());
    assert_eq!(p.jit.call(&[xs.as_ptr() as u64, 2, 1], &ctl), 2.5f64.to_bits());
    assert_eq!(ctl.status(), Status::Ok);
    // integer division likewise
    let mut fb = FnB::new("guarded_div");
    let x = fb.param(Ty::I64);
    let r = fb.var(Ty::I64);
    fb.if_(v(x).ne_(i(0)), |b| b.set(r, i(100) / v(x)), |_| {});
    let func = fb.finish(Some(v(r)));
    let p = prepare(&mut jit, &func, POLL);
    assert_eq!(p.jit.call(&[0], &ctl), 0);
    assert_eq!(p.interp.clone().call(&[0], &ctl), 0);
    assert_eq!(ctl.status(), Status::Ok);
}

#[test]
fn two_dimensional_arrays_index_row_major_and_check_both_extents() {
    use jit::ir::{len, load, Elem};
    // out[r][c] = 10 r + c + m[r][c] over an r x c matrix
    let mut fb = FnB::new("mat");
    let m = fb.param(Ty::Arr(Elem::F64, 2));
    let o = fb.param(Ty::Arr(Elem::F64, 2));
    let (r, c) = (fb.var(Ty::I64), fb.var(Ty::I64));
    fb.for_(r, i(0), len(m, 0), |b| {
        b.for_(c, i(0), len(m, 1), |b| {
            let e = f(10.0) * v(r).itof() + v(c).itof() + load(m, vec![v(r), v(c)]);
            b.store(o, vec![v(r), v(c)], e);
        });
    });
    let func = fb.finish(None);
    let mut jit = Jit::new("speed");
    let p = prepare(&mut jit, &func, POLL);
    let (rows, cols) = (3usize, 4usize);
    let mat: Vec<f64> = (0..rows * cols).map(|k| k as f64 * 0.5).collect();
    let want: Vec<f64> = (0..rows * cols).map(|k| 10.0 * (k / cols) as f64 + (k % cols) as f64 + mat[k]).collect();
    for back in 0..2 {
        let mut out = vec![0.0; rows * cols];
        let args = [mat.as_ptr() as u64, rows as u64, cols as u64, out.as_mut_ptr() as u64, rows as u64, cols as u64];
        let ctl = Ctl::unlimited();
        if back == 0 {
            p.jit.call(&args, &ctl);
        } else {
            p.interp.clone().call(&args, &ctl);
        }
        assert_eq!(ctl.status(), Status::Ok);
        assert_eq!(out, want);
        // an output with fewer columns fails on the column extent, not by writing past a row
        let mut small = vec![0.0; rows * 3];
        let args = [mat.as_ptr() as u64, rows as u64, cols as u64, small.as_mut_ptr() as u64, rows as u64, 3];
        let ctl = Ctl::unlimited();
        if back == 0 {
            p.jit.call(&args, &ctl);
        } else {
            p.interp.clone().call(&args, &ctl);
        }
        assert_eq!(ctl.status(), Status::Index);
        assert_eq!(&small[..3], &want[..3]);
        assert_eq!(small[3], 0.0, "row 1 untouched");
    }
}
