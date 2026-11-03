//! Spike E: a numeric compiler for monomorphic machine-typed functions (plan section 4, semantics 16 and 18.2 item 2).
//!
//! Pipeline: a front end (here: Rust builder code in [`kernels`]) produces the typed tree IR of [`ir`]; the type
//! checker accepts only monomorphic machine types; lowering produces the scalarised three-address code [`ir::Lir`]
//! (complex numbers become pairs of reals, arrays become pointer and extents, structured control flow becomes labels
//! and branches, every loop back edge gets a poll). The same LIR feeds every back end:
//! - module [`interp`]: a register-machine interpreter of the LIR (the baseline without machine code);
//! - module [`jit`]: Cranelift;
//! - module [`x64`]: a tiny hand-written x86-64 encoder for one kernel (Mandelbrot), to judge Cranelift's code quality.
//!
//! [`native`] holds the same kernels written in Rust (the upper bound) and [`term`] evaluates the same functions as
//! hash-consed terms (what v2 would do without the compiler).
use std::sync::atomic::{AtomicI64, AtomicU32, Ordering};

pub mod interp;
pub mod ir;
pub mod jit;
pub mod kernels;
pub mod native;
pub mod run;
pub mod term;
pub mod x64;

/// Control block shared by every task of one evaluation (semantics 10, 18.4): one fuel budget, one stop flag and the
/// failure status. Compiled code reads it at polls; its layout is part of the compiled-code ABI.
#[repr(C)]
pub struct Ctl {
    /// Remaining fuel, in loop iterations; drawn in chunks of the poll interval.
    pub fuel: AtomicI64,
    /// Nonzero: an external interrupt was requested.
    pub stop: AtomicU32,
    /// 0 while running normally; otherwise the [`Status`] of the first failure seen.
    pub status: AtomicU32,
}

pub const CTL_FUEL: i32 = 0;
pub const CTL_STOP: i32 = 8;
pub const CTL_STATUS: i32 = 12;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum Status {
    Ok = 0,
    Interrupted = 1,
    Fuel = 2,
    Index = 3,
    DivZero = 4,
}

impl Status {
    pub fn from_u32(v: u32) -> Status {
        match v {
            0 => Status::Ok,
            1 => Status::Interrupted,
            2 => Status::Fuel,
            3 => Status::Index,
            _ => Status::DivZero,
        }
    }
}

impl Ctl {
    pub fn new(fuel: i64) -> Ctl {
        Ctl { fuel: AtomicI64::new(fuel), stop: AtomicU32::new(0), status: AtomicU32::new(0) }
    }
    pub fn unlimited() -> Ctl {
        Ctl::new(i64::MAX / 2)
    }
    pub fn status(&self) -> Status {
        Status::from_u32(self.status.load(Ordering::Relaxed))
    }
    /// Record a failure (the first one wins).
    pub fn fail(&self, s: Status) {
        let _ = self.status.compare_exchange(0, s as u32, Ordering::Relaxed, Ordering::Relaxed);
    }
}

/// How compiled loops poll the control block (decision E.5).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Poll {
    /// No polling (only for measuring its cost; never used by the system).
    Off,
    /// At every back edge, decrement a local counter; every `n` iterations draw `n` fuel and read the stop flag.
    Every(u32),
}

/// FNV-style fingerprint of a sequence of 64-bit words (results are compared by fingerprint across back ends).
pub fn fp_words(it: impl IntoIterator<Item = u64>) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for w in it {
        h = (h ^ w).wrapping_mul(0x100_0000_01b3).rotate_left(17);
    }
    h
}

pub fn fp_f64(xs: &[f64]) -> u64 {
    fp_words(xs.iter().map(|x| x.to_bits()))
}

pub fn fp_i64(xs: &[i64]) -> u64 {
    fp_words(xs.iter().map(|&x| x as u64))
}

/// The elementary functions compiled code calls. Every back end calls these same Rust functions, so results agree to
/// the last bit (decision E.4).
pub mod libm {
    pub extern "C" fn sin(x: f64) -> f64 {
        x.sin()
    }
    pub extern "C" fn cos(x: f64) -> f64 {
        x.cos()
    }
    pub extern "C" fn exp(x: f64) -> f64 {
        x.exp()
    }
    pub extern "C" fn log(x: f64) -> f64 {
        x.ln()
    }
    pub extern "C" fn pow(x: f64, y: f64) -> f64 {
        x.powf(y)
    }
    pub extern "C" fn fma(x: f64, y: f64, z: f64) -> f64 {
        x.mul_add(y, z)
    }
}
