//! The kernels written in Rust with the same operation order as the IR (the upper bound). rustc never contracts
//! `a * b + c` into a fused multiply-add, so these agree with the IR back ends to the last bit.
use crate::kernels::{mandel_c, BETA, GOLDEN, MANDEL_ITERS, MIX1, MIX2, NEWTON_MAX, NEWTON_TOL, RHO, SIGMA};

#[inline]
pub fn l1(x: f64) -> f64 {
    let p = x * x * x - 2.0 * x + 1.0;
    let s = x.sin() * p;
    let g = (-(x * x) * 0.5).exp();
    let c = (3.0 * x).cos();
    let r = (1.0 + x * x).sqrt();
    let num = s + g * c - r;
    let den = 1.0 + (1.0 + x * x).ln();
    num / den
}

#[inline]
pub fn l1h(x: f64, fused: bool) -> f64 {
    let p = if fused { x.mul_add(x, -2.0).mul_add(x, 1.0) } else { (x * x - 2.0) * x + 1.0 };
    let s = x.sin() * p;
    let g = (-(x * x) * 0.5).exp();
    let c = (3.0 * x).cos();
    let r = (1.0 + x * x).sqrt();
    let num = s + g * c - r;
    let den = 1.0 + (1.0 + x * x).ln();
    num / den
}

pub fn table(xs: &[f64], out: &mut [f64], f: impl Fn(f64) -> f64) {
    for (o, &x) in out.iter_mut().zip(xs) {
        *o = f(x);
    }
}

#[inline]
pub fn escape(cr: f64, ci: f64) -> i64 {
    let (mut zr, mut zi) = (0.0f64, 0.0f64);
    let mut k = 0;
    while k < MANDEL_ITERS && zr * zr + zi * zi <= 4.0 {
        let re = zr * zr - zi * zi + cr;
        let im = zr * zi + zi * zr + ci;
        zr = re;
        zi = im;
        k += 1;
    }
    k
}

pub fn mandel(out: &mut [i64], n: i64, p0: i64) {
    for (j, o) in out.iter_mut().enumerate() {
        let (cr, ci) = mandel_c(n, p0 + j as i64);
        *o = escape(cr, ci);
    }
}

#[inline]
pub fn lorenz(s: &[f64; 3], d: &mut [f64; 3]) {
    let (x, y, z) = (s[0], s[1], s[2]);
    d[0] = SIGMA * (y - x);
    d[1] = x * (RHO - z) - y;
    d[2] = x * y - BETA * z;
}

#[inline]
pub fn newton_f(x: f64, a: f64) -> f64 {
    x * x.exp() - a
}

#[inline]
pub fn newton_df(x: f64) -> f64 {
    x.exp() * (1.0 + x)
}

pub fn newton_solve(as_: &[f64], out: &mut [f64]) {
    for (o, &a) in out.iter_mut().zip(as_) {
        let mut x = (1.0 + a).ln();
        let mut it = 0;
        let mut done = false;
        while !done && it < NEWTON_MAX {
            let dx = (x * x.exp() - a) / (x.exp() * (1.0 + x));
            x -= dx;
            done = dx.abs() <= NEWTON_TOL * (1.0 + x.abs());
            it += 1;
        }
        *o = x;
    }
}

#[inline]
fn uniform(seed: u64, c: u64) -> f64 {
    let mut z = seed.wrapping_add(c.wrapping_mul(GOLDEN));
    z = (z ^ (z >> 30)).wrapping_mul(MIX1);
    z = (z ^ (z >> 27)).wrapping_mul(MIX2);
    z ^= z >> 31;
    (z >> 11) as i64 as f64 * (1.0 / (1u64 << 53) as f64)
}

pub fn monte_carlo(seed: u64, i0: u64, i1: u64) -> i64 {
    let mut count = 0;
    for k in i0..i1 {
        let x = uniform(seed, 2 * k);
        let y = uniform(seed, 2 * k + 1);
        if x * x + y * y < 1.0 {
            count += 1;
        }
    }
    count
}
