//! The workloads as HIR, built the way a front end would produce them from terms (no common-subexpression
//! elimination at this level: `x*x` appears three times in L1). Each workload has a scalar function (what a user
//! writes, called per point by a host routine) and, where it applies, a loop function (the compiled table or solve,
//! whose iterations are independent and may run in parallel).
use crate::ir::{cplx, f, i, len, load, sel, v, Elem, FnB, Func, Ty, E, V};

// ----- L1: a 20-odd-operation machine-real expression

/// `((x^3 - 2x + 1) sin x + exp(-x^2/2) cos 3x - sqrt(1 + x^2)) / (1 + log(1 + x^2))`
pub fn l1_expr(x: E) -> E {
    let x2 = || x.clone() * x.clone();
    let p = x.clone().powi(3) - f(2.0) * x.clone() + f(1.0);
    let s = x.clone().sin() * p;
    let g = (-x2() * f(0.5)).exp();
    let c = (f(3.0) * x.clone()).cos();
    let r = (f(1.0) + x2()).sqrt();
    let num = s + g * c - r;
    let den = f(1.0) + (f(1.0) + x2()).log();
    num / den
}

/// The same with `x^3 - 2x + 1` in Horner form, fused (`fma(fma(x, x, -2), x, 1)`) or not (`(x*x - 2)*x + 1`).
pub fn l1h_expr(x: E, fused: bool) -> E {
    let p = if fused {
        x.clone().fma(x.clone(), f(-2.0)).fma(x.clone(), f(1.0))
    } else {
        (x.clone() * x.clone() - f(2.0)) * x.clone() + f(1.0)
    };
    let x2 = || x.clone() * x.clone();
    let s = x.clone().sin() * p;
    let g = (-x2() * f(0.5)).exp();
    let c = (f(3.0) * x.clone()).cos();
    let r = (f(1.0) + x2()).sqrt();
    let num = s + g * c - r;
    let den = f(1.0) + (f(1.0) + x2()).log();
    num / den
}

pub fn scalar_fn(name: &str, body: impl Fn(E) -> E) -> Func {
    let mut b = FnB::new(name);
    let x = b.param(Ty::F64);
    b.finish(Some(body(v(x))))
}

/// `table(xs, out)`: `out[k] = body(xs[k])` for every `k`.
pub fn table_fn(name: &str, body: impl Fn(E) -> E) -> Func {
    let mut b = FnB::new(name);
    let xs = b.param(Ty::Arr(Elem::F64, 1));
    let out = b.param(Ty::Arr(Elem::F64, 1));
    let k = b.var(Ty::I64);
    b.for_(k, i(0), len(xs, 0), |b| {
        b.store(out, vec![v(k)], body(load(xs, vec![v(k)])));
    });
    b.finish(None)
}

pub fn l1_points(n: usize) -> Vec<f64> {
    (0..n).map(|k| -5.0 + 10.0 * k as f64 / n as f64).collect()
}

// ----- L9: Mandelbrot escape counts

pub const MANDEL_ITERS: i64 = 256;

/// Emit `k = escape count of c = complex(cr, ci)` into `b`, using variables `z` (complex) and `k` (integer).
fn emit_escape(b: &mut FnB, cr: E, ci: E, z: V, c: V, k: V) {
    b.set(z, cplx(f(0.0), f(0.0)));
    b.set(c, cplx(cr, ci));
    b.set(k, i(0));
    b.while_(v(k).lt(i(MANDEL_ITERS)).and(v(z).abs2().le(f(4.0))), |b| {
        b.set(z, v(z) * v(z) + v(c));
        b.set(k, v(k) + i(1));
    });
}

/// `escape(cr, ci) -> integer`
pub fn mandel_scalar() -> Func {
    let mut b = FnB::new("escape");
    let cr = b.param(Ty::F64);
    let ci = b.param(Ty::F64);
    let z = b.var(Ty::C64);
    let c = b.var(Ty::C64);
    let k = b.var(Ty::I64);
    emit_escape(&mut b, v(cr), v(ci), z, c, k);
    b.finish(Some(v(k)))
}

/// `mandel(out, n, p0, p1)`: escape counts of pixels `p0..p1` of an `n`-by-`n` grid over
/// `[-2, 0.5] x [-1.25, 1.25]`, pixel `p` at row `p / n`, column `p % n`, into `out[p - p0]`.
pub fn mandel_loop() -> Func {
    let mut b = FnB::new("mandel");
    let out = b.param(Ty::Arr(Elem::I64, 1));
    let n = b.param(Ty::I64);
    let p0 = b.param(Ty::I64);
    let p1 = b.param(Ty::I64);
    let p = b.var(Ty::I64);
    let z = b.var(Ty::C64);
    let c = b.var(Ty::C64);
    let k = b.var(Ty::I64);
    b.for_(p, v(p0), v(p1), |b| {
        let col = (v(p).rem_(v(n))).itof();
        let row = (v(p) / v(n)).itof();
        let cr = f(-2.0) + f(2.5) * col / v(n).itof();
        let ci = f(-1.25) + f(2.5) * row / v(n).itof();
        emit_escape(b, cr, ci, z, c, k);
        b.store(out, vec![v(p) - v(p0)], v(k));
    });
    b.finish(None)
}

pub fn mandel_c(n: i64, p: i64) -> (f64, f64) {
    let col = (p % n) as f64;
    let row = (p / n) as f64;
    (-2.0 + 2.5 * col / n as f64, -1.25 + 2.5 * row / n as f64)
}

// ----- L3: Lorenz right-hand side and RK4

pub const SIGMA: f64 = 10.0;
pub const RHO: f64 = 28.0;
pub const BETA: f64 = 8.0 / 3.0;

pub fn lorenz_e(x: E, y: E, z: E) -> [E; 3] {
    [
        f(SIGMA) * (y.clone() - x.clone()),
        x.clone() * (f(RHO) - z.clone()) - y.clone(),
        x * y - f(BETA) * z,
    ]
}

/// `rhs(s, d)`: `d = lorenz(s)` for 3-element arrays.
pub fn lorenz_rhs() -> Func {
    let mut b = FnB::new("lorenz");
    let s = b.param(Ty::Arr(Elem::F64, 1));
    let d = b.param(Ty::Arr(Elem::F64, 1));
    let [dx, dy, dz] = lorenz_e(load(s, vec![i(0)]), load(s, vec![i(1)]), load(s, vec![i(2)]));
    b.store(d, vec![i(0)], dx);
    b.store(d, vec![i(1)], dy);
    b.store(d, vec![i(2)], dz);
    b.finish(None)
}

/// The whole fixed-step RK4 integration compiled: `rk4(state, steps, dt)` (same operation order as [`rk4_host`]).
pub fn lorenz_rk4_loop() -> Func {
    let mut b = FnB::new("lorenz_rk4");
    let st = b.param(Ty::Arr(Elem::F64, 1));
    let steps = b.param(Ty::I64);
    let dt = b.param(Ty::F64);
    let y: Vec<V> = (0..3).map(|_| b.var(Ty::F64)).collect();
    let ks: Vec<Vec<V>> = (0..4).map(|_| (0..3).map(|_| b.var(Ty::F64)).collect()).collect();
    let h2 = b.var(Ty::F64);
    let h6 = b.var(Ty::F64);
    let t = b.var(Ty::I64);
    for (c, &yc) in y.iter().enumerate() {
        b.set(yc, load(st, vec![i(c as i64)]));
    }
    b.set(h2, v(dt) * f(0.5));
    b.set(h6, v(dt) / f(6.0));
    b.for_(t, i(0), v(steps), |b| {
        let stage = |b: &mut FnB, kout: &[V], arg: [E; 3]| {
            let [a0, a1, a2] = arg;
            let r = lorenz_e(a0, a1, a2);
            for (c, e) in r.into_iter().enumerate() {
                b.set(kout[c], e);
            }
        };
        stage(b, &ks[0], [v(y[0]), v(y[1]), v(y[2])]);
        let arg = |kp: &[V], h: V| -> [E; 3] { [0, 1, 2].map(|c| v(y[c]) + v(h) * v(kp[c])) };
        stage(b, &ks[1], arg(&ks[0], h2));
        stage(b, &ks[2], arg(&ks[1], h2));
        stage(b, &ks[3], arg(&ks[2], dt));
        for c in 0..3 {
            let sum = v(ks[0][c]) + f(2.0) * v(ks[1][c]) + f(2.0) * v(ks[2][c]) + v(ks[3][c]);
            b.set(y[c], v(y[c]) + v(h6) * sum);
        }
    });
    for (c, &yc) in y.iter().enumerate() {
        b.store(st, vec![i(c as i64)], v(yc));
    }
    b.finish(None)
}

/// Fixed-step RK4 driven by the host, calling a right-hand side `f(s, d)` four times per step.
pub fn rk4_host(y0: [f64; 3], steps: usize, dt: f64, mut rhs: impl FnMut(&[f64; 3], &mut [f64; 3])) -> [f64; 3] {
    let mut y = y0;
    let h2 = dt * 0.5;
    let h6 = dt / 6.0;
    let (mut k1, mut k2, mut k3, mut k4) = ([0.0; 3], [0.0; 3], [0.0; 3], [0.0; 3]);
    for _ in 0..steps {
        rhs(&y, &mut k1);
        let a: [f64; 3] = std::array::from_fn(|c| y[c] + h2 * k1[c]);
        rhs(&a, &mut k2);
        let a: [f64; 3] = std::array::from_fn(|c| y[c] + h2 * k2[c]);
        rhs(&a, &mut k3);
        let a: [f64; 3] = std::array::from_fn(|c| y[c] + dt * k3[c]);
        rhs(&a, &mut k4);
        for c in 0..3 {
            y[c] += h6 * (k1[c] + 2.0 * k2[c] + 2.0 * k3[c] + k4[c]);
        }
    }
    y
}

pub const LORENZ_Y0: [f64; 3] = [1.0, 1.0, 1.0];
pub const LORENZ_DT: f64 = 0.001;

// ----- L5: Newton solves of x e^x = a

pub const NEWTON_MAX: i64 = 50;
pub const NEWTON_TOL: f64 = 1e-15;

pub fn newton_f() -> Func {
    let mut b = FnB::new("newton_f");
    let x = b.param(Ty::F64);
    let a = b.param(Ty::F64);
    b.finish(Some(v(x) * v(x).exp() - v(a)))
}

pub fn newton_df() -> Func {
    let mut b = FnB::new("newton_df");
    let x = b.param(Ty::F64);
    b.finish(Some(v(x).exp() * (f(1.0) + v(x))))
}

/// Newton driven by the host with a compiled function and derivative.
pub fn newton_host(a: f64, mut fx: impl FnMut(f64, f64) -> f64, mut dfx: impl FnMut(f64) -> f64) -> f64 {
    let mut x = (1.0 + a).ln();
    let mut it = 0;
    let mut done = false;
    while !done && it < NEWTON_MAX {
        let dx = fx(x, a) / dfx(x);
        x -= dx;
        done = dx.abs() <= NEWTON_TOL * (1.0 + x.abs());
        it += 1;
    }
    x
}

/// `solve(as, out)`: all Newton solves compiled, including the iteration.
pub fn newton_loop() -> Func {
    let mut b = FnB::new("newton_solve");
    let as_ = b.param(Ty::Arr(Elem::F64, 1));
    let out = b.param(Ty::Arr(Elem::F64, 1));
    let k = b.var(Ty::I64);
    let a = b.var(Ty::F64);
    let x = b.var(Ty::F64);
    let dx = b.var(Ty::F64);
    let it = b.var(Ty::I64);
    let done = b.var(Ty::Bool);
    b.for_(k, i(0), len(as_, 0), |b| {
        b.set(a, load(as_, vec![v(k)]));
        b.set(x, (f(1.0) + v(a)).log());
        b.set(it, i(0));
        b.set(done, E::B(false));
        b.while_(v(done).not_().and(v(it).lt(i(NEWTON_MAX))), |b| {
            b.set(dx, (v(x) * v(x).exp() - v(a)) / (v(x).exp() * (f(1.0) + v(x))));
            b.set(x, v(x) - v(dx));
            b.set(done, v(dx).abs().le(f(NEWTON_TOL) * (f(1.0) + v(x).abs())));
            b.set(it, v(it) + i(1));
        });
        b.store(out, vec![v(k)], v(x));
    });
    b.finish(None)
}

pub fn newton_params(n: usize) -> Vec<f64> {
    (0..n).map(|k| 0.1 + 10.0 * k as f64 / n as f64).collect()
}

// ----- L10: Monte Carlo with a counter-based generator

pub const GOLDEN: u64 = 0x9E37_79B9_7F4A_7C15;
pub const MIX1: u64 = 0xbf58_476d_1ce4_e5b9;
pub const MIX2: u64 = 0x94d0_49bb_1331_11eb;

/// `mc(seed, i0, i1) -> integer`: the number of `i` in `i0..i1` whose point `(u(2i), u(2i+1))` lies in the unit
/// disc, where `u(c) = (mix(seed + c * golden) >>> 11) * 2^-53`.
pub fn mc_loop() -> Func {
    mc_loop_with(true)
}

/// `branch`: count with `if inside { count += 1 }` (what the front end produces from the source) or with
/// `count += select(inside, 1, 0)` (if-converted).
pub fn mc_loop_with(branch: bool) -> Func {
    let mut b = FnB::new(if branch { "monte_carlo" } else { "monte_carlo_sel" });
    let seed = b.param(Ty::I64);
    let i0 = b.param(Ty::I64);
    let i1 = b.param(Ty::I64);
    let k = b.var(Ty::I64);
    let z = b.var(Ty::I64);
    let x = b.var(Ty::F64);
    let y = b.var(Ty::F64);
    let count = b.var(Ty::I64);
    b.set(count, i(0));
    let uniform = |b: &mut FnB, c: E, out: V| {
        b.set(z, v(seed) + c * i(GOLDEN as i64));
        b.set(z, (v(z).xor(v(z).ushr(i(30)))) * i(MIX1 as i64));
        b.set(z, (v(z).xor(v(z).ushr(i(27)))) * i(MIX2 as i64));
        b.set(z, v(z).xor(v(z).ushr(i(31))));
        b.set(out, v(z).ushr(i(11)).itof() * f(1.0 / (1u64 << 53) as f64));
    };
    b.for_(k, v(i0), v(i1), |b| {
        uniform(b, i(2) * v(k), x);
        uniform(b, i(2) * v(k) + i(1), y);
        let inside = (v(x) * v(x) + v(y) * v(y)).lt(f(1.0));
        if branch {
            b.if_(inside, |b| b.set(count, v(count) + i(1)), |_| {});
        } else {
            b.set(count, v(count) + sel(inside, i(1), i(0)));
        }
    });
    b.finish(Some(v(count)))
}
