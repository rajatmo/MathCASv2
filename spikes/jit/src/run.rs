//! Drivers that run each workload on each back end (shared by the main program and the tests).
use crate::interp::Interp;
use crate::ir::{lower_with, Func, Lir, Opts};
use crate::jit::{Compiled, Jit};
use crate::kernels as k;
use crate::term::{TVal, TermFn};
use crate::{native, Ctl, Poll};
use rayon::prelude::*;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

/// One function prepared for every back end.
pub struct Prepared {
    pub lir: Lir,
    pub interp: Interp,
    pub jit: Compiled,
    pub lower_time: Duration,
}

pub fn prepare(jit: &mut Jit, func: &Func, poll: Poll) -> Prepared {
    prepare_with(jit, func, poll, Opts::default())
}

pub fn prepare_with(jit: &mut Jit, func: &Func, poll: Poll, opts: Opts) -> Prepared {
    let t0 = Instant::now();
    let lir = lower_with(func, opts).unwrap_or_else(|e| panic!("{}: {e}", func.name));
    let lower_time = t0.elapsed();
    let interp = Interp::new(&lir, poll);
    let jc = jit.compile(&lir, poll).unwrap_or_else(|e| panic!("{}: {e}", func.name));
    Prepared { lir, interp, jit: jc, lower_time }
}

/// Reinterpret a compiled function's code pointer as a typed function pointer.
///
/// # Safety
/// `F` must be the `extern "C"` function type matching the function's LIR signature plus the trailing control
/// block pointer (see the ABI note in [`crate::jit`]).
pub unsafe fn typed<F: Copy>(p: *const u8) -> F {
    assert_eq!(std::mem::size_of::<F>(), std::mem::size_of::<*const u8>());
    std::mem::transmute_copy(&p)
}

pub type TableFn = extern "C" fn(*const f64, i64, *mut f64, i64, *const Ctl);
pub type ScalarFn = extern "C" fn(f64, *const Ctl) -> f64;
pub type Scalar2Fn = extern "C" fn(f64, f64, *const Ctl) -> f64;
pub type EscapeFn = extern "C" fn(f64, f64, *const Ctl) -> i64;
pub type MandelFn = extern "C" fn(*mut i64, i64, i64, i64, i64, *const Ctl);
pub type Rk4Fn = extern "C" fn(*mut f64, i64, i64, f64, *const Ctl);
pub type McFn = extern "C" fn(i64, i64, i64, *const Ctl) -> i64;

fn w(p: *const f64) -> u64 {
    p as usize as u64
}

fn wm(p: *mut f64) -> u64 {
    p as usize as u64
}

// ----- tables (L1, Newton solve): `fn(xs, out)`

pub fn table_interp(it: &mut Interp, xs: &[f64], out: &mut [f64], ctl: &Ctl) {
    it.call(&[w(xs.as_ptr()), xs.len() as u64, wm(out.as_mut_ptr()), out.len() as u64], ctl);
}

pub fn table_jit(c: &Compiled, xs: &[f64], out: &mut [f64], ctl: &Ctl) {
    // SAFETY: table functions have the signature `TableFn`.
    let f: TableFn = unsafe { typed(c.code) };
    f(xs.as_ptr(), xs.len() as i64, out.as_mut_ptr(), out.len() as i64, ctl);
}

/// Should a parallel driver still start new tasks? Once any task records a failure or the stop flag is set, the
/// remaining chunks are skipped instead of each running up to its first poll (decision E.5).
pub fn live(ctl: &Ctl) -> bool {
    ctl.status.load(Ordering::Relaxed) == 0 && ctl.stop.load(Ordering::Relaxed) == 0
}

/// The same table over independent chunks on the current rayon pool (semantics 18.2 item 2).
pub fn table_par(c: &Compiled, xs: &[f64], out: &mut [f64], chunk: usize, ctl: &Ctl) {
    out.par_chunks_mut(chunk).zip(xs.par_chunks(chunk)).for_each(|(o, x)| {
        if live(ctl) {
            table_jit(c, x, o, ctl)
        }
    });
}

pub fn table_interp_par(it: &Interp, xs: &[f64], out: &mut [f64], chunk: usize, ctl: &Ctl) {
    out.par_chunks_mut(chunk)
        .zip(xs.par_chunks(chunk))
        .for_each_with(it.clone(), |it, (o, x)| {
            if live(ctl) {
                table_interp(it, x, o, ctl)
            }
        });
}

/// Per-point calls of a scalar function from the host (how a plotting or table routine calls a user function).
pub fn map_scalar(xs: &[f64], out: &mut [f64], mut f: impl FnMut(f64) -> f64) {
    for (o, &x) in out.iter_mut().zip(xs) {
        *o = f(x);
    }
}

pub fn scalar_jit(c: &Compiled) -> ScalarFn {
    // SAFETY: scalar functions of one real have the signature `ScalarFn`.
    unsafe { typed(c.code) }
}

pub fn term_scalar(tf: &mut TermFn, x: f64, ctl: &Ctl) -> f64 {
    match tf.call(&[TVal::F(x)], ctl) {
        Ok((Some(TVal::F(y)), _)) => y,
        _ => f64::NAN,
    }
}

// ----- Mandelbrot

pub fn mandel_interp(it: &mut Interp, out: &mut [i64], n: i64, p0: i64, ctl: &Ctl) {
    let p1 = p0 + out.len() as i64;
    it.call(&[out.as_mut_ptr() as usize as u64, out.len() as u64, n as u64, p0 as u64, p1 as u64], ctl);
}

pub fn mandel_jit(c: &Compiled, out: &mut [i64], n: i64, p0: i64, ctl: &Ctl) {
    // SAFETY: `mandel_loop` has the signature `MandelFn`.
    let f: MandelFn = unsafe { typed(c.code) };
    f(out.as_mut_ptr(), out.len() as i64, n, p0, p0 + out.len() as i64, ctl);
}

/// One task per image row.
pub fn mandel_par(c: &Compiled, out: &mut [i64], n: i64, ctl: &Ctl) {
    out.par_chunks_mut(n as usize).enumerate().for_each(|(r, o)| {
        if live(ctl) {
            mandel_jit(c, o, n, r as i64 * n, ctl)
        }
    });
}

pub fn escape_per_pixel(out: &mut [i64], n: i64, mut f: impl FnMut(f64, f64) -> i64) {
    for (p, o) in out.iter_mut().enumerate() {
        let (cr, ci) = k::mandel_c(n, p as i64);
        *o = f(cr, ci);
    }
}

pub fn term_escape(tf: &mut TermFn, cr: f64, ci: f64, ctl: &Ctl) -> i64 {
    match tf.call(&[TVal::F(cr), TVal::F(ci)], ctl) {
        Ok((Some(TVal::I(v)), _)) => v,
        _ => -1,
    }
}

// ----- Lorenz

pub fn lorenz_interp_rhs<'a>(it: &'a mut Interp, ctl: &'a Ctl) -> impl FnMut(&[f64; 3], &mut [f64; 3]) + 'a {
    move |s, d| {
        it.call(&[w(s.as_ptr()), 3, wm(d.as_mut_ptr()), 3], ctl);
    }
}

pub fn lorenz_jit_rhs<'a>(c: &Compiled, ctl: &'a Ctl) -> impl FnMut(&[f64; 3], &mut [f64; 3]) + 'a {
    // SAFETY: `lorenz_rhs` has the signature `TableFn` (two arrays).
    let f: TableFn = unsafe { typed(c.code) };
    move |s, d| f(s.as_ptr(), 3, d.as_mut_ptr(), 3, ctl)
}

pub fn lorenz_term_rhs<'a>(tf: &'a mut TermFn, ctl: &'a Ctl) -> impl FnMut(&[f64; 3], &mut [f64; 3]) + 'a {
    move |s, d| {
        if let Ok((_, lists)) = tf.call(&[TVal::List(s.to_vec()), TVal::List(vec![0.0; 3])], ctl) {
            d.copy_from_slice(&lists[1]);
        }
    }
}

pub fn rk4_interp(it: &mut Interp, steps: usize, ctl: &Ctl) -> [f64; 3] {
    let mut st = k::LORENZ_Y0;
    it.call(&[wm(st.as_mut_ptr()), 3, steps as u64, k::LORENZ_DT.to_bits()], ctl);
    st
}

pub fn rk4_jit(c: &Compiled, steps: usize, ctl: &Ctl) -> [f64; 3] {
    // SAFETY: `lorenz_rk4_loop` has the signature `Rk4Fn`.
    let f: Rk4Fn = unsafe { typed(c.code) };
    let mut st = k::LORENZ_Y0;
    f(st.as_mut_ptr(), 3, steps as i64, k::LORENZ_DT, ctl);
    st
}

pub fn rk4_native(steps: usize) -> [f64; 3] {
    k::rk4_host(k::LORENZ_Y0, steps, k::LORENZ_DT, native::lorenz)
}

// ----- Newton

pub fn newton_jit_host(f: &Compiled, df: &Compiled, as_: &[f64], out: &mut [f64], ctl: &Ctl) {
    // SAFETY: `newton_f` is `Scalar2Fn` and `newton_df` is `ScalarFn`.
    let (ff, dff): (Scalar2Fn, ScalarFn) = unsafe { (typed(f.code), typed(df.code)) };
    for (o, &a) in out.iter_mut().zip(as_) {
        *o = k::newton_host(a, |x, a| ff(x, a, ctl), |x| dff(x, ctl));
    }
}

pub fn newton_interp_host(f: &mut Interp, df: &mut Interp, as_: &[f64], out: &mut [f64], ctl: &Ctl) {
    for (o, &a) in out.iter_mut().zip(as_) {
        *o = k::newton_host(
            a,
            |x, a| f.call_f(&[x.to_bits(), a.to_bits()], ctl),
            |x| df.call_f(&[x.to_bits()], ctl),
        );
    }
}

pub fn newton_term_host(f: &mut TermFn, df: &mut TermFn, as_: &[f64], out: &mut [f64], ctl: &Ctl) {
    let num = |r: Result<(Option<TVal>, Vec<Vec<f64>>), crate::Status>| match r {
        Ok((Some(TVal::F(y)), _)) => y,
        _ => f64::NAN,
    };
    for (o, &a) in out.iter_mut().zip(as_) {
        *o = k::newton_host(
            a,
            |x, a| num(f.call(&[TVal::F(x), TVal::F(a)], ctl)),
            |x| num(df.call(&[TVal::F(x)], ctl)),
        );
    }
}

pub fn newton_native_host(as_: &[f64], out: &mut [f64]) {
    for (o, &a) in out.iter_mut().zip(as_) {
        *o = k::newton_host(a, native::newton_f, native::newton_df);
    }
}

// ----- Monte Carlo

pub fn mc_interp(it: &mut Interp, seed: u64, i0: u64, i1: u64, ctl: &Ctl) -> i64 {
    it.call(&[seed, i0, i1], ctl) as i64
}

pub fn mc_jit(c: &Compiled, seed: u64, i0: u64, i1: u64, ctl: &Ctl) -> i64 {
    // SAFETY: `mc_loop` has the signature `McFn`.
    let f: McFn = unsafe { typed(c.code) };
    f(seed as i64, i0 as i64, i1 as i64, ctl)
}

/// Independent chunks of the counter range: the count is the same for any number of threads.
pub fn mc_par(c: &Compiled, seed: u64, n: u64, chunk: u64, ctl: &Ctl) -> i64 {
    let chunks = n.div_ceil(chunk);
    (0..chunks)
        .into_par_iter()
        .map(|j| if live(ctl) { mc_jit(c, seed, j * chunk, ((j + 1) * chunk).min(n), ctl) } else { 0 })
        .sum()
}

pub fn mc_term(tf: &mut TermFn, seed: u64, i0: u64, i1: u64, ctl: &Ctl) -> i64 {
    match tf.call(&[TVal::I(seed as i64), TVal::I(i0 as i64), TVal::I(i1 as i64)], ctl) {
        Ok((Some(TVal::I(v)), _)) => v,
        _ => -1,
    }
}

pub const MC_SEED: u64 = 20_261_103;
