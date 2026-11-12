//! Register-machine interpreter of the LIR (the baseline back end).
//!
//! Labels are resolved to instruction indices once; registers are untyped 64-bit words (reals by their bits,
//! booleans as 0/1). Register indices are validated when the program is built, so the loop reads them unchecked.
use crate::ir::{BOp, Cc, FOp, IOp, Lib, Lir, Op, FU, K};
use crate::{libm, Ctl, Poll, Status};
use std::sync::atomic::Ordering;

#[derive(Clone)]
pub struct Interp {
    code: Vec<Op>,
    nregs: usize,
    params: Vec<u32>,
    ret: Option<K>,
    poll: Poll,
    regs: Vec<u64>,
}

impl Interp {
    pub fn new(lir: &Lir, poll: Poll) -> Interp {
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
        let ok = |r: u32| {
            assert!((r as usize) < nregs, "register out of range");
            r
        };
        let mut code = Vec::with_capacity(pc as usize);
        for op in &lir.code {
            // validate every register operand so the run loop can index unchecked
            let regs: Vec<u32> = match *op {
                Op::IConst(d, _) | Op::FConst(d, _) | Op::BConst(d, _) => vec![d],
                Op::Mov(d, a) | Op::INeg(d, a) | Op::INot(d, a) | Op::FUn(_, d, a) | Op::BNot(d, a) => vec![d, a],
                Op::IToF(d, a) | Op::FToI(d, a) | Op::Call1(_, d, a) | Op::Check(d, a) => vec![d, a],
                Op::IBin(_, d, a, b) | Op::FBin(_, d, a, b) | Op::ICmp(_, d, a, b) | Op::FCmp(_, d, a, b) => {
                    vec![d, a, b]
                }
                Op::BBin(_, d, a, b) | Op::Call2(_, d, a, b) | Op::Load(_, d, a, b) | Op::Store(_, d, a, b) => {
                    vec![d, a, b]
                }
                Op::Fma(d, a, b, c) | Op::Sel(d, a, b, c) => vec![d, a, b, c],
                Op::Br(c, _, _) => vec![c],
                Op::Ret(Some(r)) => vec![r],
                _ => vec![],
            };
            for r in regs {
                ok(r);
            }
            match *op {
                Op::Label(_) => {}
                Op::Jmp(l) => code.push(Op::Jmp(at[l as usize])),
                Op::Br(c, a, b) => code.push(Op::Br(c, at[a as usize], at[b as usize])),
                o => code.push(o),
            }
        }
        Interp { code, nregs, params: lir.params.clone(), ret: lir.ret, poll, regs: vec![0; nregs] }
    }

    pub fn instructions(&self) -> usize {
        self.code.len()
    }

    /// Call with the flattened arguments (reals by their bits, arrays as pointer and extents). Returns the result
    /// word (0 without a result); failures are recorded in `ctl` and return 0.
    pub fn call(&mut self, args: &[u64], ctl: &Ctl) -> u64 {
        assert_eq!(args.len(), self.params.len(), "argument count");
        self.regs.clear();
        self.regs.resize(self.nregs, 0);
        for (&p, &a) in self.params.iter().zip(args) {
            self.regs[p as usize] = a;
        }
        match self.run(ctl) {
            Ok(v) => v,
            Err(s) => {
                ctl.fail(s);
                0
            }
        }
    }

    pub fn call_f(&mut self, args: &[u64], ctl: &Ctl) -> f64 {
        debug_assert_eq!(self.ret, Some(K::F));
        f64::from_bits(self.call(args, ctl))
    }

    fn run(&mut self, ctl: &Ctl) -> Result<u64, Status> {
        let code = &self.code[..];
        let r = self.regs.as_mut_ptr();
        let (every, mut count) = match self.poll {
            Poll::Off => (0i64, i64::MAX),
            Poll::Every(n) => (n as i64, n as i64),
        };
        // SAFETY: register indices were validated in `new`; `regs` has `nregs` entries.
        let g = |k: u32| unsafe { *r.add(k as usize) };
        let s = |k: u32, x: u64| unsafe { *r.add(k as usize) = x };
        let gf = |k: u32| f64::from_bits(g(k));
        let sf = |k: u32, x: f64| s(k, x.to_bits());
        let gi = |k: u32| g(k) as i64;
        let mut pc = 0usize;
        loop {
            let op = code[pc];
            pc += 1;
            match op {
                Op::IConst(d, n) => s(d, n as u64),
                Op::FConst(d, x) => sf(d, x),
                Op::BConst(d, b) => s(d, b as u64),
                Op::Mov(d, a) => s(d, g(a)),
                Op::IBin(o, d, a, b) => {
                    let (x, y) = (gi(a), gi(b));
                    let v = match o {
                        IOp::Add => x.wrapping_add(y),
                        IOp::Sub => x.wrapping_sub(y),
                        IOp::Mul => x.wrapping_mul(y),
                        IOp::Div | IOp::Rem if y == 0 => return Err(Status::DivZero),
                        IOp::Div => x.wrapping_div(y),
                        IOp::Rem => x.wrapping_rem(y),
                        IOp::And => x & y,
                        IOp::Or => x | y,
                        IOp::Xor => x ^ y,
                        IOp::Shl => x.wrapping_shl(y as u32),
                        IOp::Shr => x.wrapping_shr(y as u32),
                        IOp::UShr => (x as u64).wrapping_shr(y as u32) as i64,
                    };
                    s(d, v as u64)
                }
                Op::INeg(d, a) => s(d, gi(a).wrapping_neg() as u64),
                Op::INot(d, a) => s(d, !g(a)),
                Op::FBin(o, d, a, b) => {
                    let (x, y) = (gf(a), gf(b));
                    sf(
                        d,
                        match o {
                            FOp::Add => x + y,
                            FOp::Sub => x - y,
                            FOp::Mul => x * y,
                            FOp::Div => x / y,
                        },
                    )
                }
                Op::FUn(o, d, a) => {
                    let x = gf(a);
                    sf(
                        d,
                        match o {
                            FU::Neg => -x,
                            FU::Abs => x.abs(),
                            FU::Sqrt => x.sqrt(),
                            FU::Floor => x.floor(),
                        },
                    )
                }
                Op::Fma(d, a, b, c) => sf(d, libm::fma(gf(a), gf(b), gf(c))),
                Op::ICmp(c, d, a, b) => {
                    let (x, y) = (gi(a), gi(b));
                    s(d, cmp(c, x.cmp(&y).into()) as u64)
                }
                Op::FCmp(c, d, a, b) => {
                    let (x, y) = (gf(a), gf(b));
                    let v = match c {
                        Cc::Lt => x < y,
                        Cc::Le => x <= y,
                        Cc::Gt => x > y,
                        Cc::Ge => x >= y,
                        Cc::Eq => x == y,
                        Cc::Ne => x != y,
                    };
                    s(d, v as u64)
                }
                Op::BBin(o, d, a, b) => s(
                    d,
                    match o {
                        BOp::And => g(a) & g(b),
                        BOp::Or => g(a) | g(b),
                        BOp::Xor => g(a) ^ g(b),
                    },
                ),
                Op::BNot(d, a) => s(d, g(a) ^ 1),
                Op::Sel(d, c, a, b) => s(d, if g(c) != 0 { g(a) } else { g(b) }),
                Op::IToF(d, a) => sf(d, gi(a) as f64),
                Op::FToI(d, a) => s(d, gf(a) as i64 as u64),
                Op::Call1(l, d, a) => {
                    let x = gf(a);
                    sf(
                        d,
                        match l {
                            Lib::Sin => libm::sin(x),
                            Lib::Cos => libm::cos(x),
                            Lib::Exp => libm::exp(x),
                            _ => libm::log(x),
                        },
                    )
                }
                Op::Call2(_, d, a, b) => sf(d, libm::pow(gf(a), gf(b))),
                Op::Load(_, d, p, i) => {
                    // SAFETY: a `Check` against the array's extent precedes every access (lowering invariant), and
                    // the caller passes a pointer and extent describing live memory.
                    s(d, unsafe { *(g(p) as *const u64).add(g(i) as usize) })
                }
                Op::Store(_, p, i, x) => unsafe { *(g(p) as *mut u64).add(g(i) as usize) = g(x) },
                Op::Check(i, n) => {
                    if g(i) >= g(n) {
                        return Err(Status::Index);
                    }
                }
                Op::Jmp(t) => pc = t as usize,
                Op::Br(c, a, b) => pc = if g(c) != 0 { a as usize } else { b as usize },
                Op::Label(_) => {}
                Op::Poll => {
                    count -= 1;
                    if count == 0 {
                        count = every;
                        let old = ctl.fuel.fetch_sub(every, Ordering::Relaxed);
                        if old < every {
                            return Err(Status::Fuel);
                        }
                        if ctl.stop.load(Ordering::Relaxed) != 0 {
                            return Err(Status::Interrupted);
                        }
                    }
                }
                Op::Ret(r) => return Ok(r.map_or(0, g)),
            }
        }
    }
}

fn cmp(c: Cc, o: Ord3) -> bool {
    match c {
        Cc::Lt => o == Ord3::Lt,
        Cc::Le => o != Ord3::Gt,
        Cc::Gt => o == Ord3::Gt,
        Cc::Ge => o != Ord3::Lt,
        Cc::Eq => o == Ord3::Eq,
        Cc::Ne => o != Ord3::Eq,
    }
}

#[derive(PartialEq, Eq)]
enum Ord3 {
    Lt,
    Eq,
    Gt,
}

impl From<std::cmp::Ordering> for Ord3 {
    fn from(o: std::cmp::Ordering) -> Ord3 {
        match o {
            std::cmp::Ordering::Less => Ord3::Lt,
            std::cmp::Ordering::Equal => Ord3::Eq,
            std::cmp::Ordering::Greater => Ord3::Gt,
        }
    }
}
