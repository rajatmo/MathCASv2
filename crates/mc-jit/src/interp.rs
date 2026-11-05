//! The LIR interpreter: the reference back end for differential tests, and the fallback when Cranelift cannot
//! compile a function or does not support the host (decision E.7).
//!
//! Labels are resolved to instruction indices once; registers are untyped 64-bit words (reals by their bits,
//! booleans as 0 or 1). Register indices are checked when the program is built, so the run loop indexes freely.
use crate::ir::{BOp, Cc, FOp, IOp, Lir, Op, OpdK, K};
use crate::rt::{self, Env, Exit, EXACT_REAL_INT, OPD_CEXACT, OPD_COMPLEX, OPD_INT, OPD_RATIO, OPD_REAL, OPD_WORDS, POLL_EVERY};

/// A LIR function prepared for interpretation. Immutable: every call has its own registers.
pub struct Interp {
    code: Vec<Op>,
    nregs: usize,
    params: Vec<u32>,
    /// the kind of every register (for helpers with an integer or a real result)
    kinds: Vec<K>,
}

/// Every register operand of an instruction (destinations included).
fn operands(op: &Op) -> Vec<u32> {
    match op {
        Op::IConst(d, _) | Op::FConst(d, _) | Op::BConst(d, _) | Op::Ret(d) | Op::Bail(d) | Op::Br(d, _, _) | Op::Out(_, d) => {
            vec![*d]
        }
        Op::Mov(d, a)
        | Op::FAbs(d, a)
        | Op::BNot(d, a)
        | Op::ToReal(d, a)
        | Op::Floor(d, a)
        | Op::Math(_, d, a)
        | Op::IToF(d, a)
        | Op::FTrunc(d, a)
        | Op::FKey(d, a)
        | Op::FNeg(d, a)
        | Op::FromKey(d, a) => vec![*d, *a],
        Op::Draw(_, d, a, b) | Op::Load(d, a, b) | Op::Store(d, a, b) | Op::VSum(d, a, b) => vec![*d, *a, *b],
        Op::VNew(d, a) | Op::VOut(d, a) | Op::Spline(_, d, a) => vec![*d, *a],
        Op::CMath(_, d, e, a, b) => vec![*d, *e, *a, *b],
        Op::CNum(_, d, e, opds) => [*d, *e].into_iter().chain(opds.iter().flat_map(|&(_, a, b)| [a, b])).collect(),
        Op::IChk(_, d, a, b)
        | Op::FBin(_, d, a, b)
        | Op::ICmp(_, d, a, b)
        | Op::FCmp(_, d, a, b)
        | Op::BBin(_, d, a, b)
        | Op::IMinMax(_, d, a, b)
        | Op::PowI(d, a, b)
        | Op::PowF(d, a, b) => vec![*d, *a, *b],
        Op::IAddOv(d, o, a, b) | Op::IMulOv(d, o, a, b) | Op::Sel(d, o, a, b) => vec![*d, *o, *a, *b],
        Op::Num(_, d, opds) => std::iter::once(*d).chain(opds.iter().flat_map(|&(_, a, b)| [a, b])).collect(),
        Op::Jmp(_) | Op::Label(_) | Op::Poll => vec![],
    }
}

impl Interp {
    /// Prepare `lir`. Panics on a register out of range (a lowering defect).
    pub fn new(lir: &Lir) -> Interp {
        let mut at = vec![0u32; lir.nlabels as usize];
        let mut pc = 0u32;
        for op in &lir.code {
            if let Op::Label(l) = op {
                at[*l as usize] = pc;
            } else {
                pc += 1;
            }
        }
        let nregs = lir.regs.len();
        let mut code = Vec::with_capacity(pc as usize);
        for op in &lir.code {
            assert!(operands(op).iter().all(|&r| (r as usize) < nregs), "register out of range");
            match op {
                Op::Label(_) => {}
                Op::Jmp(l) => code.push(Op::Jmp(at[*l as usize])),
                Op::Br(c, a, b) => code.push(Op::Br(*c, at[*a as usize], at[*b as usize])),
                o => code.push(o.clone()),
            }
        }
        Interp { code, nregs, params: lir.params.clone(), kinds: lir.regs.clone() }
    }

    /// Number of instructions.
    pub fn instructions(&self) -> usize {
        self.code.len()
    }

    /// Call with the parameters as words (reals by their bits). Returns the result word, or 0 with the exit set.
    pub fn call(&self, args: &[u64], env: &mut Env) -> u64 {
        assert_eq!(args.len(), self.params.len(), "argument count");
        let mut regs = vec![0u64; self.nregs];
        for (&p, &a) in self.params.iter().zip(args) {
            regs[p as usize] = a;
        }
        match self.run(&mut regs, env) {
            Ok(v) => v,
            Err(e) => {
                if env.exit == Exit::Done as u32 {
                    env.exit = e as u32;
                }
                0
            }
        }
    }

    fn run(&self, r: &mut [u64], env: &mut Env) -> Result<u64, Exit> {
        let code = &self.code[..];
        let mut count = POLL_EVERY;
        let gi = |r: &[u64], k: u32| r[k as usize] as i64;
        let gf = |r: &[u64], k: u32| f64::from_bits(r[k as usize]);
        let mut pc = 0usize;
        loop {
            let op = &code[pc];
            pc += 1;
            match *op {
                Op::IConst(d, n) => r[d as usize] = n as u64,
                Op::FConst(d, x) => r[d as usize] = x.to_bits(),
                Op::BConst(d, b) => r[d as usize] = b as u64,
                Op::Mov(d, a) => r[d as usize] = r[a as usize],
                Op::IChk(o, d, a, b) => {
                    let (x, y) = (gi(r, a), gi(r, b));
                    let v = match o {
                        IOp::Add => x.checked_add(y),
                        IOp::Sub => x.checked_sub(y),
                        IOp::Mul => x.checked_mul(y),
                        IOp::DivExact => rt::floor_divmod(x, y).and_then(|(q, m)| (m == 0).then_some(q)),
                        IOp::Quot => rt::floor_divmod(x, y).map(|(q, _)| q),
                        IOp::Mod => rt::floor_divmod(x, y).map(|(_, m)| m),
                        IOp::Pow => rt::ipow_exact(x, y),
                    };
                    r[d as usize] = v.ok_or(Exit::Decline)? as u64;
                }
                Op::IAddOv(d, of, a, b) => {
                    let (v, o) = gi(r, a).overflowing_add(gi(r, b));
                    r[d as usize] = v as u64;
                    r[of as usize] = o as u64;
                }
                Op::IMulOv(d, of, a, b) => {
                    let (v, o) = gi(r, a).overflowing_mul(gi(r, b));
                    r[d as usize] = v as u64;
                    r[of as usize] = o as u64;
                }
                Op::IMinMax(max, d, a, b) => {
                    let (x, y) = (gi(r, a), gi(r, b));
                    r[d as usize] = if max { x.max(y) } else { x.min(y) } as u64;
                }
                Op::IToF(d, a) => r[d as usize] = (gi(r, a) as f64).to_bits(),
                Op::FTrunc(d, a) => r[d as usize] = gf(r, a).trunc().to_bits(),
                Op::FKey(d, a) => r[d as usize] = rt::order_key(gf(r, a)) as u64,
                Op::FromKey(d, a) => r[d as usize] = rt::from_order_key(gi(r, a)).to_bits(),
                Op::PowI(d, a, b) => r[d as usize] = rt::pow_int(gf(r, a), gi(r, b)).to_bits(),
                Op::PowF(d, a, b) => r[d as usize] = rt::pow_real(gf(r, a), gf(r, b)).to_bits(),
                Op::FBin(o, d, a, b) => {
                    let (x, y) = (gf(r, a), gf(r, b));
                    r[d as usize] = match o {
                        FOp::Add => x + y,
                        FOp::Sub => x - y,
                        FOp::Mul => x * y,
                        FOp::Div => x / y,
                    }
                    .to_bits();
                }
                Op::FAbs(d, a) => r[d as usize] = gf(r, a).abs().to_bits(),
                Op::ICmp(c, d, a, b) => {
                    let (x, y) = (gi(r, a), gi(r, b));
                    r[d as usize] = cmp(c, x.partial_cmp(&y)) as u64;
                }
                Op::FCmp(c, d, a, b) => {
                    let (x, y) = (gf(r, a), gf(r, b));
                    r[d as usize] = cmp(c, x.partial_cmp(&y)) as u64;
                }
                Op::BBin(o, d, a, b) => {
                    let (x, y) = (r[a as usize], r[b as usize]);
                    r[d as usize] = match o {
                        BOp::And => x & y,
                        BOp::Or => x | y,
                    };
                }
                Op::BNot(d, a) => r[d as usize] = r[a as usize] ^ 1,
                Op::Sel(d, c, a, b) => r[d as usize] = if r[c as usize] != 0 { r[a as usize] } else { r[b as usize] },
                Op::ToReal(d, a) => {
                    let x = gi(r, a);
                    if !(-EXACT_REAL_INT..=EXACT_REAL_INT).contains(&x) {
                        return Err(Exit::Decline);
                    }
                    r[d as usize] = (x as f64).to_bits();
                }
                Op::Floor(d, a) => r[d as usize] = rt::floor_exact(gf(r, a)).ok_or(Exit::Decline)? as u64,
                Op::Math(f, d, a) => r[d as usize] = rt::math_fn(f)(gf(r, a)).to_bits(),
                Op::Num(o, d, ref opds) => {
                    r[d as usize] = rt::num_words(o, &opd_words(r, opds)).ok_or(Exit::Decline)?.to_bits();
                }
                Op::CNum(o, d, e, ref opds) => {
                    let (a, b) = rt::cnum_words(o, &opd_words(r, opds)).ok_or(Exit::Decline)?;
                    r[d as usize] = a.to_bits();
                    r[e as usize] = b.to_bits();
                }
                Op::CMath(f, d, e, a, b) => {
                    let (x, y) = rt::cmath_value(f, gf(r, a), gf(r, b));
                    r[d as usize] = x.to_bits();
                    r[e as usize] = y.to_bits();
                }
                Op::FNeg(d, a) => r[d as usize] = (-gf(r, a)).to_bits(),
                Op::Spline(id, d, a) => {
                    r[d as usize] = rt::spline_value(id, gf(r, a)).ok_or(Exit::Decline)?.to_bits();
                }
                Op::Draw(k, d, a, b) => {
                    // SAFETY: `env` is a live environment whose stream, if any, is this call's (`Env::with_stream`).
                    r[d as usize] = unsafe { rt::draw_value(env, k, r[a as usize], r[b as usize]) }.ok_or(Exit::Decline)?;
                }
                Op::Bail(c) => {
                    if r[c as usize] != 0 {
                        return Err(Exit::Decline);
                    }
                }
                Op::Jmp(t) => pc = t as usize,
                Op::Br(c, a, b) => pc = if r[c as usize] != 0 { a as usize } else { b as usize },
                Op::Label(_) => {}
                Op::Poll => {
                    count -= 1;
                    if count == 0 {
                        count = POLL_EVERY;
                        // SAFETY: `env` is a live environment made by `Env::with`.
                        if unsafe { rt::poll(env) } != 0 {
                            return Err(Exit::Stopped);
                        }
                    }
                }
                Op::Ret(v) => return Ok(r[v as usize]),
                Op::Out(k, v) => {
                    if !env.write_out(k as usize, r[v as usize]) {
                        return Err(Exit::Decline);
                    }
                }
                Op::Load(d, base, i) => {
                    // SAFETY: lowering checked `0 <= i < len` against the vector's length, and the vector is an
                    // argument's leaves or one the call made, alive for the call (`rt::Env`)
                    r[d as usize] = unsafe { *(r[base as usize] as *const u64).add(r[i as usize] as usize) };
                }
                Op::Store(base, i, v) => {
                    // SAFETY: as for `Load`; stores go only to vectors the call made
                    unsafe { *(r[base as usize] as *mut u64).add(r[i as usize] as usize) = r[v as usize] };
                }
                Op::VNew(d, n) => {
                    // SAFETY: `env` is a live environment.
                    r[d as usize] = unsafe { rt::vec_new(env, gi(r, n)) };
                    exited(env)?;
                }
                Op::VOut(base, n) => {
                    // SAFETY: `env` is live; `base` holds `n` words (lowering and the host's arguments).
                    unsafe { rt::vec_out(env, r[base as usize], gi(r, n)) };
                    exited(env)?;
                }
                Op::VSum(d, base, n) => {
                    // SAFETY: as for `VOut`.
                    r[d as usize] = unsafe {
                        if self.kinds[d as usize] == K::I {
                            rt::vsum_int(env, r[base as usize], gi(r, n)) as u64
                        } else {
                            rt::vsum_real(env, r[base as usize], gi(r, n)).to_bits()
                        }
                    };
                    exited(env)?;
                }
            }
        }
    }
}

/// After a helper that may set the exit: leave with it.
fn exited(env: &Env) -> Result<(), Exit> {
    match env.exit {
        0 => Ok(()),
        1 => Err(Exit::Decline),
        _ => Err(Exit::Stopped),
    }
}

/// The helper's operand words ([`rt::OPD_WORDS`] per operand) from the registers.
fn opd_words(r: &[u64], opds: &[(OpdK, u32, u32)]) -> Vec<u64> {
    let mut words = Vec::with_capacity(opds.len() * OPD_WORDS);
    for &(k, a, b) in opds {
        words.extend([opd_tag(k), r[a as usize], r[b as usize]]);
    }
    words
}

/// The helper's tag of an operand kind.
pub(crate) fn opd_tag(k: OpdK) -> u64 {
    match k {
        OpdK::Int => OPD_INT,
        OpdK::Real => OPD_REAL,
        OpdK::Ratio => OPD_RATIO,
        OpdK::Complex => OPD_COMPLEX,
        OpdK::CExact => OPD_CEXACT,
    }
}

/// A comparison's truth from the operands' order (`None`: unordered, a NaN).
fn cmp(c: Cc, o: Option<std::cmp::Ordering>) -> bool {
    use std::cmp::Ordering::*;
    match (c, o) {
        (Cc::Ne, None) => true,
        (_, None) => false,
        (Cc::Lt, Some(o)) => o == Less,
        (Cc::Le, Some(o)) => o != Greater,
        (Cc::Gt, Some(o)) => o == Greater,
        (Cc::Ge, Some(o)) => o != Less,
        (Cc::Eq, Some(o)) => o == Equal,
        (Cc::Ne, Some(o)) => o != Equal,
    }
}
