//! The Cranelift back end: LIR to machine code (spike E, decision E.1: Cranelift 0.137 at `opt_level=speed`).
//!
//! Every compiled function lives in a module of its own, so that its code memory is freed when the function is
//! dropped (when its specialisation is replaced). The ABI is [`crate::rt`]'s: the native calling convention with
//! the parameters followed by the [`Env`] pointer, and a uniform entry `fn(args: *const u64, env: *mut Env) -> u64`
//! that the host calls.
use crate::ir::{BOp, Cc, FOp, IOp, Lib, Lir, Op, OpdK, K};
use crate::rt::{self, Env, Exit, CTL_STOP_STATUS, ENV_CTL, ENV_EXIT, ENV_OUT, EXACT_REAL_INT, OPD_INT, OPD_RATIO, OPD_REAL, OPD_WORDS, POLL_EVERY};
use cranelift_codegen::ir::condcodes::{FloatCC, IntCC};
use cranelift_codegen::ir::{
    types, AbiParam, Block, FuncRef, InstBuilder, MemFlagsData, StackSlotData, StackSlotKind, Type, UserFuncName, Value,
};
use cranelift_codegen::settings::{self, Configurable};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext, Variable};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{default_libcall_names, FuncId, Linkage, Module};

// Library calls Cranelift may emit when the host lacks an instruction (rounding, fused multiply-add).
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
extern "C" fn c_fma(x: f64, y: f64, z: f64) -> f64 {
    x.mul_add(y, z)
}

/// One compiled function: its machine code, owned.
pub struct Code {
    /// The module holding the code; taken when the code is freed.
    module: Option<JITModule>,
    /// `unsafe extern "C" fn(*const u64, *mut Env) -> u64`
    entry: *const u8,
    /// Size of the function's machine code in bytes (the uniform entry excluded).
    pub code_bytes: usize,
}

// SAFETY: the code is immutable once finalised and touches only its arguments, its `Env` and the control block's
// atomics; the module is only used again to free the memory, with `&mut self` in `drop`.
unsafe impl Send for Code {}
// SAFETY: as above: shared use only calls the finalised code.
unsafe impl Sync for Code {}

impl Code {
    /// Call through the uniform entry (the same contract as [`crate::interp::Interp::call`]).
    pub fn call(&self, args: &[u64], env: &mut Env) -> u64 {
        // SAFETY: `entry` was generated with exactly this signature and reads as many words as the function has
        // parameters; the caller passes that many (checked by the specialisation against its parameter list).
        let f: unsafe extern "C" fn(*const u64, *mut Env) -> u64 = unsafe { std::mem::transmute(self.entry) };
        // SAFETY: as above; `env` is live for the call.
        unsafe { f(args.as_ptr(), env) }
    }
}

impl Drop for Code {
    fn drop(&mut self) {
        if let Some(m) = self.module.take() {
            // SAFETY: nothing calls the code any more: the last owner is going away.
            unsafe { m.free_memory() };
        }
    }
}

fn ty(k: K) -> Type {
    match k {
        K::I => types::I64,
        K::F => types::F64,
        K::B => types::I8,
    }
}

/// The helpers compiled code imports, by symbol name.
struct Imports {
    math: Vec<FuncId>,
    poll: FuncId,
    num: FuncId,
    ipow: FuncId,
    pow_int: FuncId,
    pow_real: FuncId,
}

/// The host's target description, made once per process (decision N0.8: detecting the host and building the
/// flags took a large share of compiling a small expression).
fn host_isa() -> Result<cranelift_codegen::isa::OwnedTargetIsa, String> {
    static ISA: std::sync::OnceLock<Result<cranelift_codegen::isa::OwnedTargetIsa, String>> = std::sync::OnceLock::new();
    ISA.get_or_init(|| {
        let mut fb = settings::builder();
        let set = |fb: &mut settings::Builder, k: &str, v: &str| fb.set(k, v).map_err(|e| e.to_string());
        set(&mut fb, "opt_level", "speed")?;
        set(&mut fb, "use_colocated_libcalls", "false")?;
        set(&mut fb, "is_pic", "true")?;
        let isa = cranelift_native::builder().map_err(|e| format!("host: {e}"))?;
        isa.finish(settings::Flags::new(fb)).map_err(|e| e.to_string())
    })
    .clone()
}

/// Compile one LIR function to machine code.
pub fn compile(lir: &Lir) -> Result<Code, String> {
    let mut jb = JITBuilder::with_isa(host_isa()?, default_libcall_names());
    for f in Lib::ALL {
        jb.symbol(format!("mc_jit_{}", f.name()), rt::math_fn(f) as *const u8);
    }
    let helpers: [(&str, *const u8); 10] = [
        ("mc_jit_poll", rt::poll as *const u8),
        ("mc_jit_num", rt::num as *const u8),
        ("mc_jit_ipow", rt::ipow as *const u8),
        ("mc_jit_pow_int", rt::pow_int as *const u8),
        ("mc_jit_pow_real", rt::pow_real as *const u8),
        ("floor", c_floor as *const u8),
        ("ceil", c_ceil as *const u8),
        ("trunc", c_trunc as *const u8),
        ("nearbyint", c_nearest as *const u8),
        ("fma", c_fma as *const u8),
    ];
    for (n, p) in helpers {
        jb.symbol(n, p);
    }
    let mut module = JITModule::new(jb);
    let ptr = module.target_config().pointer_type();
    let import = |m: &mut JITModule, name: &str, params: &[Type], ret: Type| -> Result<FuncId, String> {
        let mut sig = m.make_signature();
        sig.params.extend(params.iter().map(|&t| AbiParam::new(t)));
        sig.returns.push(AbiParam::new(ret));
        m.declare_function(name, Linkage::Import, &sig).map_err(|e| e.to_string())
    };
    let mut math = vec![];
    for f in Lib::ALL {
        math.push(import(&mut module, &format!("mc_jit_{}", f.name()), &[types::F64], types::F64)?);
    }
    let imports = Imports {
        math,
        poll: import(&mut module, "mc_jit_poll", &[ptr], types::I32)?,
        num: import(&mut module, "mc_jit_num", &[ptr, types::I64, ptr, types::I64], types::I64)?,
        ipow: import(&mut module, "mc_jit_ipow", &[ptr, types::I64, types::I64], types::I64)?,
        pow_int: import(&mut module, "mc_jit_pow_int", &[types::F64, types::I64], types::F64)?,
        pow_real: import(&mut module, "mc_jit_pow_real", &[types::F64, types::F64], types::F64)?,
    };

    let mut ctx = module.make_context();
    let mut fctx = FunctionBuilderContext::new();
    let mut sig = module.make_signature();
    for &p in &lir.params {
        match lir.regs[p as usize] {
            K::B => return Err("a boolean parameter".into()),
            k => sig.params.push(AbiParam::new(ty(k))),
        }
    }
    sig.params.push(AbiParam::new(ptr));
    let rty = if lir.ret == K::F { types::F64 } else { types::I64 };
    sig.returns.push(AbiParam::new(rty));
    let fid = module.declare_function(&lir.name, Linkage::Local, &sig).map_err(|e| e.to_string())?;
    ctx.func.signature = sig.clone();
    ctx.func.name = UserFuncName::user(0, fid.as_u32());
    {
        let mut b = FunctionBuilder::new(&mut ctx.func, &mut fctx);
        let refs = Refs {
            math: imports.math.iter().map(|&id| module.declare_func_in_func(id, b.func)).collect(),
            poll: module.declare_func_in_func(imports.poll, b.func),
            num: module.declare_func_in_func(imports.num, b.func),
            ipow: module.declare_func_in_func(imports.ipow, b.func),
            pow_int: module.declare_func_in_func(imports.pow_int, b.func),
            pow_real: module.declare_func_in_func(imports.pow_real, b.func),
        };
        let mut g = Gen::new(&mut b, lir, rty, refs, ptr);
        g.body();
        b.seal_all_blocks();
        b.finalize(module.target_config());
    }
    module.define_function(fid, &mut ctx).map_err(|e| format!("{e:?}"))?;
    let code_bytes = ctx.compiled_code().map_or(0, |c| c.code_buffer().len());
    module.clear_context(&mut ctx);

    // the uniform entry: load the parameters from memory and call
    let mut esig = module.make_signature();
    esig.params.push(AbiParam::new(ptr));
    esig.params.push(AbiParam::new(ptr));
    esig.returns.push(AbiParam::new(types::I64));
    let eid = module.declare_function(&format!("{}_entry", lir.name), Linkage::Local, &esig).map_err(|e| e.to_string())?;
    ctx.func.signature = esig;
    ctx.func.name = UserFuncName::user(0, eid.as_u32());
    {
        let mut b = FunctionBuilder::new(&mut ctx.func, &mut fctx);
        let blk = b.create_block();
        b.append_block_params_for_function_params(blk);
        b.switch_to_block(blk);
        let (args, env) = (b.block_params(blk)[0], b.block_params(blk)[1]);
        let callee = module.declare_func_in_func(fid, b.func);
        let mf = MemFlagsData::trusted();
        let mut vals: Vec<Value> = vec![];
        for (k, p) in sig.params.iter().take(lir.params.len()).enumerate() {
            vals.push(b.ins().load(p.value_type, mf, args, (8 * k) as i32));
        }
        vals.push(env);
        let call = b.ins().call(callee, &vals);
        let v = b.inst_results(call)[0];
        let r = if rty == types::F64 { b.ins().bitcast(types::I64, MemFlagsData::new(), v) } else { v };
        b.ins().return_(&[r]);
        b.seal_all_blocks();
        b.finalize(module.target_config());
    }
    module.define_function(eid, &mut ctx).map_err(|e| format!("{e:?}"))?;
    module.clear_context(&mut ctx);
    module.finalize_definitions().map_err(|e| e.to_string())?;
    let entry = module.get_finalized_function(eid);
    Ok(Code { module: Some(module), entry, code_bytes })
}

/// The imported helpers, declared in the function being built.
struct Refs {
    math: Vec<FuncRef>,
    poll: FuncRef,
    num: FuncRef,
    ipow: FuncRef,
    pow_int: FuncRef,
    pow_real: FuncRef,
}

struct Gen<'a, 'b> {
    b: &'a mut FunctionBuilder<'b>,
    lir: &'a Lir,
    rty: Type,
    refs: Refs,
    ptr: Type,
    vars: Vec<Variable>,
    blocks: Vec<Block>,
    /// the cold blocks that set an exit and return 0, made on first use
    decline: Option<Block>,
    stopped: Option<Block>,
    /// returns 0 with the exit a helper already set
    propagate: Option<Block>,
    env: Value,
    ctl: Value,
    counter: Variable,
    terminated: bool,
    mf: MemFlagsData,
}

impl<'a, 'b> Gen<'a, 'b> {
    fn new(b: &'a mut FunctionBuilder<'b>, lir: &'a Lir, rty: Type, refs: Refs, ptr: Type) -> Self {
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
        let env = *params.last().expect("the environment parameter");
        let mf = MemFlagsData::trusted();
        let ctl = b.ins().load(ptr, mf, env, ENV_CTL);
        let counter = b.declare_var(types::I64);
        let n = b.ins().iconst(types::I64, POLL_EVERY as i64);
        b.def_var(counter, n);
        let blocks = (0..lir.nlabels).map(|_| b.create_block()).collect();
        Gen {
            b,
            lir,
            rty,
            refs,
            ptr,
            vars,
            blocks,
            decline: None,
            stopped: None,
            propagate: None,
            env,
            ctl,
            counter,
            terminated: false,
            mf,
        }
    }

    fn get(&mut self, r: u32) -> Value {
        self.b.use_var(self.vars[r as usize])
    }
    fn set(&mut self, r: u32, v: Value) {
        self.b.def_var(self.vars[r as usize], v);
    }

    fn cold(&mut self, which: Option<Exit>) -> Block {
        let slot = match which {
            Some(Exit::Decline) => &mut self.decline,
            Some(_) => &mut self.stopped,
            None => &mut self.propagate,
        };
        if let Some(b) = *slot {
            return b;
        }
        let blk = self.b.create_block();
        self.b.set_cold_block(blk);
        *slot = Some(blk);
        blk
    }

    /// Fill the exit blocks that were used: set the exit (unless a helper did) and return 0.
    fn emit_exits(&mut self) {
        for (blk, code) in [(self.decline, Some(Exit::Decline)), (self.stopped, Some(Exit::Stopped)), (self.propagate, None)] {
            let Some(blk) = blk else { continue };
            self.b.switch_to_block(blk);
            if let Some(c) = code {
                let v = self.b.ins().iconst(types::I32, c as i64);
                self.b.ins().store(self.mf, v, self.env, ENV_EXIT);
            }
            let z = if self.rty == types::F64 { self.b.ins().f64const(0.0) } else { self.b.ins().iconst(types::I64, 0) };
            self.b.ins().return_(&[z]);
        }
    }

    /// Branch to the exit block when `bad` is nonzero, else continue in a new block.
    fn guard(&mut self, bad: Value, exit: Option<Exit>) {
        let fb = self.cold(exit);
        let cont = self.b.create_block();
        self.b.ins().brif(bad, fb, &[], cont, &[]);
        self.b.switch_to_block(cont);
    }

    /// After a helper call: leave when it set the exit.
    fn check_exit(&mut self) {
        let e = self.b.ins().load(types::I32, self.mf, self.env, ENV_EXIT);
        self.guard(e, None);
    }

    fn body(&mut self) {
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
        self.emit_exits();
    }

    fn int_op(&mut self, o: IOp, x: Value, y: Value) -> Value {
        let i64t = types::I64;
        match o {
            IOp::Add | IOp::Sub | IOp::Mul => {
                let (v, of) = match o {
                    IOp::Add => self.b.ins().sadd_overflow(x, y),
                    IOp::Sub => self.b.ins().ssub_overflow(x, y),
                    _ => self.b.ins().smul_overflow(x, y),
                };
                self.guard(of, Some(Exit::Decline));
                v
            }
            IOp::Pow => {
                let call = self.b.ins().call(self.refs.ipow, &[self.env, x, y]);
                let v = self.b.inst_results(call)[0];
                self.check_exit();
                v
            }
            IOp::DivExact | IOp::Quot | IOp::Mod => {
                // a zero divisor or MIN / -1 declines; after the guards sdiv and srem cannot trap
                let z = self.b.ins().icmp_imm_s(IntCC::Equal, y, 0);
                self.guard(z, Some(Exit::Decline));
                let m1 = self.b.ins().icmp_imm_s(IntCC::Equal, y, -1);
                let mn = self.b.ins().icmp_imm_s(IntCC::Equal, x, i64::MIN);
                let ov = self.b.ins().band(m1, mn);
                self.guard(ov, Some(Exit::Decline));
                let q = self.b.ins().sdiv(x, y);
                let r = self.b.ins().srem(x, y);
                let rnz = self.b.ins().icmp_imm_s(IntCC::NotEqual, r, 0);
                if o == IOp::DivExact {
                    self.guard(rnz, Some(Exit::Decline));
                    return q;
                }
                // floor convention: a nonzero remainder whose sign differs from the divisor's moves one step
                let xr = self.b.ins().bxor(r, y);
                let neg = self.b.ins().icmp_imm_s(IntCC::SignedLessThan, xr, 0);
                let adj = self.b.ins().band(rnz, neg);
                if o == IOp::Quot {
                    let one = self.b.ins().uextend(i64t, adj);
                    self.b.ins().isub(q, one)
                } else {
                    let zero = self.b.ins().iconst(i64t, 0);
                    let add = self.b.ins().select(adj, y, zero);
                    self.b.ins().iadd(r, add)
                }
            }
        }
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
            Op::IChk(o, d, a, b) => {
                let (x, y) = (self.get(a), self.get(b));
                let v = self.int_op(o, x, y);
                self.set(d, v)
            }
            Op::IAddOv(d, of, a, b) => {
                let (x, y) = (self.get(a), self.get(b));
                let (v, o) = self.b.ins().sadd_overflow(x, y);
                self.set(d, v);
                self.set(of, o)
            }
            Op::IMulOv(d, of, a, b) => {
                let (x, y) = (self.get(a), self.get(b));
                let (v, o) = self.b.ins().smul_overflow(x, y);
                self.set(d, v);
                self.set(of, o)
            }
            Op::IMinMax(max, d, a, b) => {
                let (x, y) = (self.get(a), self.get(b));
                let v = if max { self.b.ins().smax(x, y) } else { self.b.ins().smin(x, y) };
                self.set(d, v)
            }
            Op::IToF(d, a) => {
                let x = self.get(a);
                let v = self.b.ins().fcvt_from_sint(types::F64, x);
                self.set(d, v)
            }
            Op::FTrunc(d, a) => {
                let x = self.get(a);
                let v = self.b.ins().trunc(x);
                self.set(d, v)
            }
            Op::FKey(d, a) => {
                let x = self.get(a);
                let bits = self.b.ins().bitcast(i64t, MemFlagsData::new(), x);
                let v = self.order_key(bits);
                self.set(d, v)
            }
            Op::FromKey(d, a) => {
                let k = self.get(a);
                let bits = self.order_key(k);
                let v = self.b.ins().bitcast(types::F64, MemFlagsData::new(), bits);
                self.set(d, v)
            }
            Op::PowI(d, a, b) | Op::PowF(d, a, b) => {
                let (x, y) = (self.get(a), self.get(b));
                let f = if matches!(op, Op::PowI(..)) { self.refs.pow_int } else { self.refs.pow_real };
                let call = self.b.ins().call(f, &[x, y]);
                let v = self.b.inst_results(call)[0];
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
            Op::FAbs(d, a) => {
                let x = self.get(a);
                let v = self.b.ins().fabs(x);
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
                // ordered comparisons are false with a NaN; `NotEqual` is true with one (as in Rust)
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
            Op::ToReal(d, a) => {
                let x = self.get(a);
                // |x| > 2^53 <=> x + 2^53 > 2^54 as unsigned
                let s = self.b.ins().iadd_imm_s(x, EXACT_REAL_INT);
                let bad = self.b.ins().icmp_imm_s(IntCC::UnsignedGreaterThan, s, 2 * EXACT_REAL_INT);
                self.guard(bad, Some(Exit::Decline));
                let v = self.b.ins().fcvt_from_sint(types::F64, x);
                self.set(d, v)
            }
            Op::Floor(d, a) => {
                let x = self.get(a);
                let f = self.b.ins().floor(x);
                let lo = self.b.ins().f64const(-9_223_372_036_854_775_808.0);
                let hi = self.b.ins().f64const(9_223_372_036_854_775_808.0);
                let ge = self.b.ins().fcmp(FloatCC::GreaterThanOrEqual, f, lo);
                let lt = self.b.ins().fcmp(FloatCC::LessThan, f, hi);
                let ok = self.b.ins().band(ge, lt);
                let bad = self.b.ins().bxor_imm_s(ok, 1);
                self.guard(bad, Some(Exit::Decline));
                let v = self.b.ins().fcvt_to_sint_sat(i64t, f);
                self.set(d, v)
            }
            Op::Math(f, d, a) => {
                let x = self.get(a);
                let i = Lib::ALL.iter().position(|&l| l == f).expect("a library function");
                let call = self.b.ins().call(self.refs.math[i], &[x]);
                let v = self.b.inst_results(call)[0];
                self.set(d, v)
            }
            Op::Num(o, d, ref opds) => {
                let words = (opds.len() * OPD_WORDS * 8) as u32;
                let ss = self.b.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, words, 3));
                for (i, &(k, a, b)) in opds.iter().enumerate() {
                    let base = (i * OPD_WORDS * 8) as i32;
                    let tag = match k {
                        OpdK::Int => OPD_INT,
                        OpdK::Real => OPD_REAL,
                        OpdK::Ratio => OPD_RATIO,
                    };
                    let t = self.b.ins().iconst(i64t, tag as i64);
                    self.b.ins().stack_store(self.ptr, t, ss, base);
                    let va = self.get(a);
                    self.b.ins().stack_store(self.ptr, va, ss, base + 8);
                    let vb = self.get(b);
                    self.b.ins().stack_store(self.ptr, vb, ss, base + 16);
                }
                let addr = self.b.ins().stack_addr(self.ptr, ss, 0);
                let code = self.b.ins().iconst(i64t, rt::num_op_code(o) as i64);
                let n = self.b.ins().iconst(i64t, opds.len() as i64);
                let call = self.b.ins().call(self.refs.num, &[self.env, code, addr, n]);
                let bits = self.b.inst_results(call)[0];
                self.check_exit();
                let v = self.b.ins().bitcast(types::F64, MemFlagsData::new(), bits);
                self.set(d, v)
            }
            Op::Bail(c) => {
                let c = self.get(c);
                self.guard(c, Some(Exit::Decline));
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
            Op::Out(k, r) => {
                // the host checked that the buffer covers every slot (`Built::call_out`)
                let v = self.get(r);
                let out = self.b.ins().load(self.ptr, self.mf, self.env, ENV_OUT);
                self.b.ins().store(self.mf, v, out, (8 * k) as i32);
            }
            Op::Ret(r) => {
                let mut v = self.get(r);
                if self.lir.regs[r as usize] == K::B {
                    v = self.b.ins().uextend(i64t, v);
                }
                self.b.ins().return_(&[v]);
                self.terminated = true;
            }
        }
    }

    /// [`rt::order_key`] on the bits of a machine real, and its inverse on a key: flip the 63 low bits when the sign
    /// bit is set.
    fn order_key(&mut self, bits: Value) -> Value {
        let sign = self.b.ins().sshr_imm_u(bits, 63);
        let low = self.b.ins().ushr_imm_u(sign, 1);
        self.b.ins().bxor(bits, low)
    }

    /// Interrupt and fuel check at a loop back edge (decision E.5): every [`POLL_EVERY`] iterations read the control
    /// block's stop flag and status (one 64-bit word) and let the host charge the fuel.
    fn poll(&mut self) {
        let slow = self.b.create_block();
        let cont = self.b.create_block();
        let x = self.b.use_var(self.counter);
        let x = self.b.ins().iadd_imm_s(x, -1);
        self.b.def_var(self.counter, x);
        self.b.ins().brif(x, cont, &[], slow, &[]);
        self.b.set_cold_block(slow);
        self.b.switch_to_block(slow);
        let n = self.b.ins().iconst(types::I64, POLL_EVERY as i64);
        self.b.def_var(self.counter, n);
        let addr = self.b.ins().iadd_imm_s(self.ctl, CTL_STOP_STATUS as i64);
        let word = self.b.ins().atomic_load(types::I64, self.mf, addr);
        self.guard(word, Some(Exit::Stopped));
        let call = self.b.ins().call(self.refs.poll, &[self.env]);
        let stop = self.b.inst_results(call)[0];
        self.guard(stop, None);
        self.b.ins().jump(cont, &[]);
        self.b.switch_to_block(cont);
    }
}
