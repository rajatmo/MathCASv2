//! Cranelift back end: LIR to machine code.
//!
//! ABI (decision E.6): the native calling convention of the host; LIR parameters in order (`I` as i64, `F` as f64,
//! arrays as pointer and extents), then a pointer to the [`Ctl`](crate::Ctl) block; the result, if any, as one
//! i64 or f64 (booleans widened to i64). A failure stores its status in the control block and returns 0.
//! Every compiled function also gets a uniform entry `fn(args: *const u64, ctl: *const Ctl) -> u64` so that a generic
//! caller (and the cross-check tests) can call it like the interpreter.
use crate::ir::{BOp, Cc, Elem, FOp, IOp, Lib, Lir, Op, FU, K};
use crate::{libm, Ctl, Poll, Status, CTL_FUEL, CTL_STATUS, CTL_STOP};
use cranelift_codegen::ir::condcodes::{FloatCC, IntCC};
use cranelift_codegen::ir::{
    types, AbiParam, AtomicRmwOp, Block, FuncRef, InstBuilder, MemFlagsData, Type, UserFuncName, Value,
};
use cranelift_codegen::settings::{self, Configurable};
use cranelift_codegen::Context;
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext, Variable};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{default_libcall_names, FuncId, Linkage, Module};
use std::collections::HashMap;
use std::time::{Duration, Instant};

extern "C" fn c_floor(x: f64) -> f64 {
    x.floor()
}
extern "C" fn c_ceil(x: f64) -> f64 {
    x.ceil()
}
extern "C" fn c_trunc(x: f64) -> f64 {
    x.trunc()
}
extern "C" fn c_nearest(x: f64) -> f64 {
    x.round_ties_even()
}

/// One compiled function.
#[derive(Clone, Copy)]
pub struct Compiled {
    /// The function with its native signature.
    pub code: *const u8,
    /// `extern "C" fn(*const u64, *const Ctl) -> u64`
    pub entry: *const u8,
    pub compile_time: Duration,
    pub code_bytes: usize,
}

// SAFETY: compiled code is immutable once finalised and is pure apart from the control block it is handed.
unsafe impl Send for Compiled {}
unsafe impl Sync for Compiled {}

impl Compiled {
    /// Generic call through the uniform entry (same contract as [`Interp::call`](crate::interp::Interp::call)).
    pub fn call(&self, args: &[u64], ctl: &Ctl) -> u64 {
        // SAFETY: `entry` was generated with exactly this signature and reads `args.len()` words, which the caller
        // matches to the function's parameter list.
        let f: extern "C" fn(*const u64, *const Ctl) -> u64 = unsafe { std::mem::transmute(self.entry) };
        f(args.as_ptr(), ctl)
    }
}

pub struct Jit {
    module: JITModule,
    ctx: Context,
    fctx: FunctionBuilderContext,
    libs: Vec<FuncId>,
    count: u32,
    pub opt: &'static str,
}

fn ty(k: K) -> Type {
    match k {
        K::I => types::I64,
        K::F => types::F64,
        K::B => types::I8,
    }
}

impl Jit {
    /// `opt`: Cranelift `opt_level` ("none", "speed").
    pub fn new(opt: &'static str) -> Jit {
        let mut fb = settings::builder();
        fb.set("opt_level", opt).unwrap();
        fb.set("use_colocated_libcalls", "false").unwrap();
        fb.set("is_pic", "true").unwrap();
        let isa = cranelift_native::builder().expect("host isa").finish(settings::Flags::new(fb)).expect("isa");
        let mut jb = JITBuilder::with_isa(isa, default_libcall_names());
        let syms: [(&str, *const u8); 10] = [
            ("mc_sin", libm::sin as *const u8),
            ("mc_cos", libm::cos as *const u8),
            ("mc_exp", libm::exp as *const u8),
            ("mc_log", libm::log as *const u8),
            ("mc_pow", libm::pow as *const u8),
            // libcalls Cranelift may emit when the host lacks an instruction
            ("fma", libm::fma as *const u8),
            ("floor", c_floor as *const u8),
            ("ceil", c_ceil as *const u8),
            ("trunc", c_trunc as *const u8),
            ("nearbyint", c_nearest as *const u8),
        ];
        for (n, p) in syms {
            jb.symbol(n, p);
        }
        let mut module = JITModule::new(jb);
        let mut libs = vec![];
        for (n, arity) in [("mc_sin", 1), ("mc_cos", 1), ("mc_exp", 1), ("mc_log", 1), ("mc_pow", 2)] {
            let mut sig = module.make_signature();
            for _ in 0..arity {
                sig.params.push(AbiParam::new(types::F64));
            }
            sig.returns.push(AbiParam::new(types::F64));
            libs.push(module.declare_function(n, Linkage::Import, &sig).unwrap());
        }
        let ctx = module.make_context();
        Jit { module, ctx, fctx: FunctionBuilderContext::new(), libs, count: 0, opt }
    }

    pub fn has_fma(&self) -> bool {
        self.module.isa().isa_flags().iter().any(|f| f.name == "has_fma" && f.as_bool() == Some(true))
    }

    /// Compile one LIR function (and its uniform entry) to finalised machine code.
    pub fn compile(&mut self, lir: &Lir, poll: Poll) -> Result<Compiled, String> {
        let t0 = Instant::now();
        self.count += 1;
        let ptr = types::I64;
        let mut sig = self.module.make_signature();
        for &p in &lir.params {
            match lir.regs[p as usize] {
                K::B => return Err("boolean parameter".into()),
                k => sig.params.push(AbiParam::new(ty(k))),
            }
        }
        sig.params.push(AbiParam::new(ptr));
        let rty = lir.ret.map(|k| if k == K::F { types::F64 } else { types::I64 });
        if let Some(t) = rty {
            sig.returns.push(AbiParam::new(t));
        }
        let name = format!("{}_{}", lir.name, self.count);
        let fid = self.module.declare_function(&name, Linkage::Local, &sig).map_err(|e| e.to_string())?;
        self.ctx.func.signature = sig.clone();
        self.ctx.func.name = UserFuncName::user(0, fid.as_u32());
        {
            let mut b = FunctionBuilder::new(&mut self.ctx.func, &mut self.fctx);
            let lib_refs: Vec<FuncRef> = self.libs.iter().map(|&id| self.module.declare_func_in_func(id, b.func)).collect();
            let mut g = Gen::new(&mut b, lir, poll, rty, lib_refs);
            g.body()?;
            b.seal_all_blocks();
            b.finalize(self.module.target_config());
        }
        self.module.define_function(fid, &mut self.ctx).map_err(|e| format!("{e:?}"))?;
        let code_bytes = self.ctx.compiled_code().map_or(0, |c| c.code_buffer().len());
        self.module.clear_context(&mut self.ctx);

        // uniform entry: load the flattened arguments from memory and call
        let mut esig = self.module.make_signature();
        esig.params.push(AbiParam::new(ptr));
        esig.params.push(AbiParam::new(ptr));
        esig.returns.push(AbiParam::new(types::I64));
        let eid = self
            .module
            .declare_function(&format!("{name}_entry"), Linkage::Local, &esig)
            .map_err(|e| e.to_string())?;
        self.ctx.func.signature = esig;
        self.ctx.func.name = UserFuncName::user(0, eid.as_u32());
        {
            let mut b = FunctionBuilder::new(&mut self.ctx.func, &mut self.fctx);
            let blk = b.create_block();
            b.append_block_params_for_function_params(blk);
            b.switch_to_block(blk);
            let (args, ctl) = (b.block_params(blk)[0], b.block_params(blk)[1]);
            let callee = self.module.declare_func_in_func(fid, b.func);
            let mf = MemFlagsData::trusted();
            let plain = MemFlagsData::new();
            let mut vals: Vec<Value> = vec![];
            for (k, p) in sig.params.iter().take(lir.params.len()).enumerate() {
                vals.push(b.ins().load(p.value_type, mf, args, (8 * k) as i32));
            }
            vals.push(ctl);
            let call = b.ins().call(callee, &vals);
            let r = match rty {
                None => b.ins().iconst(types::I64, 0),
                Some(t) => {
                    let v = b.inst_results(call)[0];
                    if t == types::F64 {
                        b.ins().bitcast(types::I64, plain, v)
                    } else {
                        v
                    }
                }
            };
            b.ins().return_(&[r]);
            b.seal_all_blocks();
            b.finalize(self.module.target_config());
        }
        self.module.define_function(eid, &mut self.ctx).map_err(|e| format!("{e:?}"))?;
        self.module.clear_context(&mut self.ctx);
        self.module.finalize_definitions().map_err(|e| e.to_string())?;
        Ok(Compiled {
            code: self.module.get_finalized_function(fid),
            entry: self.module.get_finalized_function(eid),
            compile_time: t0.elapsed(),
            code_bytes,
        })
    }
}

struct Gen<'a, 'b> {
    b: &'a mut FunctionBuilder<'b>,
    lir: &'a Lir,
    poll: Poll,
    rty: Option<Type>,
    libs: Vec<FuncRef>,
    vars: Vec<Variable>,
    blocks: Vec<Block>,
    fail: HashMap<u32, Block>,
    ctl: Value,
    counter: Option<Variable>,
    terminated: bool,
    mf: MemFlagsData,
}

impl<'a, 'b> Gen<'a, 'b> {
    fn new(b: &'a mut FunctionBuilder<'b>, lir: &'a Lir, poll: Poll, rty: Option<Type>, libs: Vec<FuncRef>) -> Self {
        let entry = b.create_block();
        b.append_block_params_for_function_params(entry);
        b.switch_to_block(entry);
        let params: Vec<Value> = b.block_params(entry).to_vec();
        let vars: Vec<Variable> = lir.regs.iter().map(|&k| b.declare_var(ty(k))).collect();
        // every register starts at zero, as in the interpreter
        for (r, &k) in lir.regs.iter().enumerate() {
            let z = match k {
                K::I => b.ins().iconst(types::I64, 0),
                K::F => b.ins().f64const(0.0),
                K::B => b.ins().iconst(types::I8, 0),
            };
            b.def_var(vars[r], z);
        }
        for (k, &p) in lir.params.iter().enumerate() {
            b.def_var(vars[p as usize], params[k]);
        }
        let ctl = *params.last().unwrap();
        let counter = match poll {
            Poll::Every(n) if n > 1 => {
                let c = b.declare_var(types::I64);
                let v = b.ins().iconst(types::I64, n as i64);
                b.def_var(c, v);
                Some(c)
            }
            _ => None,
        };
        let blocks = (0..lir.nlabels).map(|_| b.create_block()).collect();
        let mf = MemFlagsData::trusted();
        Gen { b, lir, poll, rty, libs, vars, blocks, fail: HashMap::new(), ctl, counter, terminated: false, mf }
    }

    fn get(&mut self, r: u32) -> Value {
        self.b.use_var(self.vars[r as usize])
    }
    fn set(&mut self, r: u32, v: Value) {
        self.b.def_var(self.vars[r as usize], v);
    }

    /// The block that records failure `s` and returns 0.
    fn fail_block(&mut self, s: Status) -> Block {
        if let Some(&blk) = self.fail.get(&(s as u32)) {
            return blk;
        }
        let blk = self.b.create_block();
        self.b.set_cold_block(blk);
        self.fail.insert(s as u32, blk);
        blk
    }

    fn emit_fail_blocks(&mut self) {
        let fails: Vec<(u32, Block)> = self.fail.iter().map(|(&k, &v)| (k, v)).collect();
        for (code, blk) in fails {
            self.b.switch_to_block(blk);
            let addr = self.b.ins().iadd_imm_s(self.ctl, CTL_STATUS as i64);
            let zero = self.b.ins().iconst(types::I32, 0);
            let c = self.b.ins().iconst(types::I32, code as i64);
            self.b.ins().atomic_cas(self.mf, addr, zero, c);
            self.ret_zero();
        }
    }

    fn ret_zero(&mut self) {
        match self.rty {
            None => {
                self.b.ins().return_(&[]);
            }
            Some(t) if t == types::F64 => {
                let z = self.b.ins().f64const(0.0);
                self.b.ins().return_(&[z]);
            }
            Some(_) => {
                let z = self.b.ins().iconst(types::I64, 0);
                self.b.ins().return_(&[z]);
            }
        }
    }

    /// Branch to `fail` when `bad` is nonzero, else continue in a new block.
    fn guard(&mut self, bad: Value, s: Status) {
        let fb = self.fail_block(s);
        let cont = self.b.create_block();
        self.b.ins().brif(bad, fb, &[], cont, &[]);
        self.b.switch_to_block(cont);
    }

    fn body(&mut self) -> Result<(), String> {
        let lir = self.lir;
        for op in &lir.code {
            if self.terminated && !matches!(op, Op::Label(_)) {
                // unreachable code after a jump: give it a block of its own
                let blk = self.b.create_block();
                self.b.switch_to_block(blk);
                self.terminated = false;
            }
            self.op(op);
        }
        self.emit_fail_blocks();
        Ok(())
    }

    fn op(&mut self, op: &Op) {
        let i64t = types::I64;
        match *op {
            Op::IConst(d, n) => {
                let v = self.b.ins().iconst(i64t, n);
                self.set(d, v)
            }
            Op::FConst(d, x) => {
                let v = self.b.ins().f64const(x);
                self.set(d, v)
            }
            Op::BConst(d, x) => {
                let v = self.b.ins().iconst(types::I8, x as i64);
                self.set(d, v)
            }
            Op::Mov(d, a) => {
                let v = self.get(a);
                self.set(d, v)
            }
            Op::IBin(o, d, a, b) => {
                let (x, y) = (self.get(a), self.get(b));
                let v = match o {
                    IOp::Add => self.b.ins().iadd(x, y),
                    IOp::Sub => self.b.ins().isub(x, y),
                    IOp::Mul => self.b.ins().imul(x, y),
                    IOp::And => self.b.ins().band(x, y),
                    IOp::Or => self.b.ins().bor(x, y),
                    IOp::Xor => self.b.ins().bxor(x, y),
                    IOp::Shl => self.b.ins().ishl(x, y),
                    IOp::Shr => self.b.ins().sshr(x, y),
                    IOp::UShr => self.b.ins().ushr(x, y),
                    IOp::Div | IOp::Rem => {
                        let z = self.b.ins().icmp_imm_s(IntCC::Equal, y, 0);
                        self.guard(z, Status::DivZero);
                        // wrapping semantics: x / -1 = -x, x % -1 = 0 (avoids the overflow trap of MIN / -1)
                        let m1 = self.b.ins().icmp_imm_s(IntCC::Equal, y, -1);
                        let one = self.b.ins().iconst(i64t, 1);
                        let sy = self.b.ins().select(m1, one, y);
                        if o == IOp::Div {
                            let q = self.b.ins().sdiv(x, sy);
                            let n = self.b.ins().ineg(x);
                            self.b.ins().select(m1, n, q)
                        } else {
                            let r = self.b.ins().srem(x, sy);
                            let zero = self.b.ins().iconst(i64t, 0);
                            self.b.ins().select(m1, zero, r)
                        }
                    }
                };
                self.set(d, v)
            }
            Op::INeg(d, a) => {
                let x = self.get(a);
                let v = self.b.ins().ineg(x);
                self.set(d, v)
            }
            Op::INot(d, a) => {
                let x = self.get(a);
                let v = self.b.ins().bnot(x);
                self.set(d, v)
            }
            Op::FBin(o, d, a, b) => {
                let (x, y) = (self.get(a), self.get(b));
                let v = match o {
                    FOp::Add => self.b.ins().fadd(x, y),
                    FOp::Sub => self.b.ins().fsub(x, y),
                    FOp::Mul => self.b.ins().fmul(x, y),
                    FOp::Div => self.b.ins().fdiv(x, y),
                };
                self.set(d, v)
            }
            Op::FUn(o, d, a) => {
                let x = self.get(a);
                let v = match o {
                    FU::Neg => self.b.ins().fneg(x),
                    FU::Abs => self.b.ins().fabs(x),
                    FU::Sqrt => self.b.ins().sqrt(x),
                    FU::Floor => self.b.ins().floor(x),
                };
                self.set(d, v)
            }
            Op::Fma(d, a, b, c) => {
                let (x, y, z) = (self.get(a), self.get(b), self.get(c));
                let v = self.b.ins().fma(x, y, z);
                self.set(d, v)
            }
            Op::ICmp(c, d, a, b) => {
                let (x, y) = (self.get(a), self.get(b));
                let cc = match c {
                    Cc::Lt => IntCC::SignedLessThan,
                    Cc::Le => IntCC::SignedLessThanOrEqual,
                    Cc::Gt => IntCC::SignedGreaterThan,
                    Cc::Ge => IntCC::SignedGreaterThanOrEqual,
                    Cc::Eq => IntCC::Equal,
                    Cc::Ne => IntCC::NotEqual,
                };
                let v = self.b.ins().icmp(cc, x, y);
                self.set(d, v)
            }
            Op::FCmp(c, d, a, b) => {
                let (x, y) = (self.get(a), self.get(b));
                let cc = match c {
                    Cc::Lt => FloatCC::LessThan,
                    Cc::Le => FloatCC::LessThanOrEqual,
                    Cc::Gt => FloatCC::GreaterThan,
                    Cc::Ge => FloatCC::GreaterThanOrEqual,
                    Cc::Eq => FloatCC::Equal,
                    Cc::Ne => FloatCC::NotEqual,
                };
                let v = self.b.ins().fcmp(cc, x, y);
                self.set(d, v)
            }
            Op::BBin(o, d, a, b) => {
                let (x, y) = (self.get(a), self.get(b));
                let v = match o {
                    BOp::And => self.b.ins().band(x, y),
                    BOp::Or => self.b.ins().bor(x, y),
                    BOp::Xor => self.b.ins().bxor(x, y),
                };
                self.set(d, v)
            }
            Op::BNot(d, a) => {
                let x = self.get(a);
                let v = self.b.ins().bxor_imm_s(x, 1);
                self.set(d, v)
            }
            Op::Sel(d, c, a, b) => {
                let (c, x, y) = (self.get(c), self.get(a), self.get(b));
                let v = self.b.ins().select(c, x, y);
                self.set(d, v)
            }
            Op::IToF(d, a) => {
                let x = self.get(a);
                let v = self.b.ins().fcvt_from_sint(types::F64, x);
                self.set(d, v)
            }
            Op::FToI(d, a) => {
                let x = self.get(a);
                let v = self.b.ins().fcvt_to_sint_sat(i64t, x);
                self.set(d, v)
            }
            Op::Call1(l, d, a) => {
                let x = self.get(a);
                let f = self.libs[match l {
                    Lib::Sin => 0,
                    Lib::Cos => 1,
                    Lib::Exp => 2,
                    _ => 3,
                }];
                let call = self.b.ins().call(f, &[x]);
                let v = self.b.inst_results(call)[0];
                self.set(d, v)
            }
            Op::Call2(_, d, a, b) => {
                let (x, y) = (self.get(a), self.get(b));
                let call = self.b.ins().call(self.libs[4], &[x, y]);
                let v = self.b.inst_results(call)[0];
                self.set(d, v)
            }
            Op::Load(el, d, p, i) => {
                let (p, i) = (self.get(p), self.get(i));
                let off = self.b.ins().ishl_imm_s(i, 3);
                let addr = self.b.ins().iadd(p, off);
                let t = if el == Elem::I64 { i64t } else { types::F64 };
                let v = self.b.ins().load(t, self.mf, addr, 0);
                self.set(d, v)
            }
            Op::Store(_, p, i, x) => {
                let (p, i, x) = (self.get(p), self.get(i), self.get(x));
                let off = self.b.ins().ishl_imm_s(i, 3);
                let addr = self.b.ins().iadd(p, off);
                self.b.ins().store(self.mf, x, addr, 0);
            }
            Op::Check(i, n) => {
                let (i, n) = (self.get(i), self.get(n));
                let bad = self.b.ins().icmp(IntCC::UnsignedGreaterThanOrEqual, i, n);
                self.guard(bad, Status::Index);
            }
            Op::Jmp(l) => {
                self.b.ins().jump(self.blocks[l as usize], &[]);
                self.terminated = true;
            }
            Op::Br(c, x, y) => {
                let c = self.get(c);
                self.b.ins().brif(c, self.blocks[x as usize], &[], self.blocks[y as usize], &[]);
                self.terminated = true;
            }
            Op::Label(l) => {
                let blk = self.blocks[l as usize];
                if !self.terminated {
                    self.b.ins().jump(blk, &[]);
                }
                self.b.switch_to_block(blk);
                self.terminated = false;
            }
            Op::Poll => self.poll(),
            Op::Ret(r) => {
                match (r, self.rty) {
                    (Some(r), Some(_)) => {
                        let mut v = self.get(r);
                        if self.lir.regs[r as usize] == K::B {
                            v = self.b.ins().uextend(i64t, v);
                        }
                        self.b.ins().return_(&[v]);
                    }
                    _ => {
                        self.b.ins().return_(&[]);
                    }
                }
                self.terminated = true;
            }
        }
    }

    /// Interrupt and fuel check at a loop back edge (decision E.5).
    fn poll(&mut self) {
        let n = match self.poll {
            Poll::Off => return,
            Poll::Every(n) => n as i64,
        };
        let slow = self.b.create_block();
        let cont = self.b.create_block();
        if let Some(c) = self.counter {
            let x = self.b.use_var(c);
            let x = self.b.ins().iadd_imm_s(x, -1);
            self.b.def_var(c, x);
            self.b.ins().brif(x, cont, &[], slow, &[]);
            self.b.set_cold_block(slow);
        } else {
            self.b.ins().jump(slow, &[]);
        }
        self.b.switch_to_block(slow);
        if let Some(c) = self.counter {
            let v = self.b.ins().iconst(types::I64, n);
            self.b.def_var(c, v);
        }
        let amount = self.b.ins().iconst(types::I64, n);
        let fuel_addr = self.b.ins().iadd_imm_s(self.ctl, CTL_FUEL as i64);
        let old = self.b.ins().atomic_rmw(types::I64, self.mf, AtomicRmwOp::Sub, fuel_addr, amount);
        let out = self.b.ins().icmp_imm_s(IntCC::SignedLessThan, old, n);
        self.guard(out, Status::Fuel);
        let stop_addr = self.b.ins().iadd_imm_s(self.ctl, CTL_STOP as i64);
        let stop = self.b.ins().atomic_load(types::I32, self.mf, stop_addr);
        self.guard(stop, Status::Interrupted);
        self.b.ins().jump(cont, &[]);
        self.b.switch_to_block(cont);
    }
}
