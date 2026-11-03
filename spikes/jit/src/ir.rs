//! The typed numeric IR (decision E.2).
//!
//! Two levels:
//! - **HIR** ([`Func`], [`E`], [`S`]): a typed tree, what a front end produces from a term like
//!   `fn(x: machine_real) -> x^3 - 2 x`. Types are monomorphic machine types only (semantics 16): `I64`, `F64`, `C64`,
//!   `Bool` and packed 1-D/2-D arrays of `I64` or `F64`. No implicit conversions: the front end inserts them.
//! - **LIR** ([`Lir`], [`Op`]): scalarised three-address code over typed virtual registers of three kinds (`I`, `F`,
//!   `B`) with labels and branches. Complex values are pairs of `F` registers; an array is a pointer register plus one
//!   extent register per dimension; every array access is preceded by a bounds check; every loop back edge carries a
//!   `Poll`. All back ends consume the LIR, so they share one definition of every operation's order.
use std::ops;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Elem {
    I64,
    F64,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Ty {
    I64,
    F64,
    C64,
    Bool,
    /// Packed array: element type and rank (1 or 2).
    Arr(Elem, u8),
}

pub type V = u32;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Un {
    Neg,
    /// Logical not on `Bool`, bitwise not on `I64`.
    Not,
    Re,
    Im,
    /// `|z|^2 = re*re + im*im`
    Abs2,
    Conj,
    IToF,
    /// Truncation toward zero, saturating, NaN to 0 (Rust `as`).
    FToI,
    Abs,
    Sqrt,
    Floor,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Bin {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    /// Logical (non-short-circuit: operands are pure) on `Bool`, bitwise on `I64`.
    And,
    Or,
    Xor,
    Shl,
    Shr,
    UShr,
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
    /// `complex(re, im)`
    Cplx,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Math {
    Sin,
    Cos,
    Exp,
    Log,
    Pow,
    /// Fused multiply-add: only when the source asks for it (decision E.4).
    Fma,
}

#[derive(Clone, Debug)]
pub enum E {
    I(i64),
    F(f64),
    B(bool),
    Var(V),
    Un(Un, Box<E>),
    Bin(Bin, Box<E>, Box<E>),
    /// Integer power with a fixed exponent, by square-and-multiply from the top bit (`x^3 = (x*x)*x`).
    PowI(Box<E>, u32),
    Call(Math, Vec<E>),
    /// `if c then a else b` on values (both sides evaluated; they are pure).
    Sel(Box<E>, Box<E>, Box<E>),
    Load(V, Vec<E>),
    Len(V, u8),
}

#[derive(Clone, Debug)]
pub enum S {
    Set(V, E),
    Store(V, Vec<E>, E),
    If(E, Vec<S>, Vec<S>),
    While(E, Vec<S>),
    /// `for v in lo..hi` (hi evaluated once).
    For(V, E, E, Vec<S>),
}

#[derive(Clone, Debug)]
pub struct Func {
    pub name: String,
    pub vars: Vec<Ty>,
    pub params: Vec<V>,
    pub body: Vec<S>,
    pub ret: Option<E>,
}

// ---------------------------------------------------------------------------------------------------------------
// Builder

pub fn v(x: V) -> E {
    E::Var(x)
}
pub fn i(n: i64) -> E {
    E::I(n)
}
pub fn f(x: f64) -> E {
    E::F(x)
}
fn un(op: Un, a: E) -> E {
    E::Un(op, Box::new(a))
}
fn bin(op: Bin, a: E, b: E) -> E {
    E::Bin(op, Box::new(a), Box::new(b))
}

impl E {
    pub fn lt(self, b: E) -> E {
        bin(Bin::Lt, self, b)
    }
    pub fn le(self, b: E) -> E {
        bin(Bin::Le, self, b)
    }
    pub fn gt(self, b: E) -> E {
        bin(Bin::Gt, self, b)
    }
    pub fn ge(self, b: E) -> E {
        bin(Bin::Ge, self, b)
    }
    pub fn eq_(self, b: E) -> E {
        bin(Bin::Eq, self, b)
    }
    pub fn ne_(self, b: E) -> E {
        bin(Bin::Ne, self, b)
    }
    pub fn and(self, b: E) -> E {
        bin(Bin::And, self, b)
    }
    pub fn or(self, b: E) -> E {
        bin(Bin::Or, self, b)
    }
    pub fn xor(self, b: E) -> E {
        bin(Bin::Xor, self, b)
    }
    pub fn shl_(self, b: E) -> E {
        bin(Bin::Shl, self, b)
    }
    pub fn shr_(self, b: E) -> E {
        bin(Bin::Shr, self, b)
    }
    pub fn ushr(self, b: E) -> E {
        bin(Bin::UShr, self, b)
    }
    pub fn rem_(self, b: E) -> E {
        bin(Bin::Rem, self, b)
    }
    pub fn powi(self, n: u32) -> E {
        E::PowI(Box::new(self), n)
    }
    pub fn re(self) -> E {
        un(Un::Re, self)
    }
    pub fn im(self) -> E {
        un(Un::Im, self)
    }
    pub fn abs2(self) -> E {
        un(Un::Abs2, self)
    }
    pub fn conj(self) -> E {
        un(Un::Conj, self)
    }
    pub fn itof(self) -> E {
        un(Un::IToF, self)
    }
    pub fn ftoi(self) -> E {
        un(Un::FToI, self)
    }
    pub fn abs(self) -> E {
        un(Un::Abs, self)
    }
    pub fn sqrt(self) -> E {
        un(Un::Sqrt, self)
    }
    pub fn floor(self) -> E {
        un(Un::Floor, self)
    }
    pub fn not_(self) -> E {
        un(Un::Not, self)
    }
    pub fn sin(self) -> E {
        E::Call(Math::Sin, vec![self])
    }
    pub fn cos(self) -> E {
        E::Call(Math::Cos, vec![self])
    }
    pub fn exp(self) -> E {
        E::Call(Math::Exp, vec![self])
    }
    pub fn log(self) -> E {
        E::Call(Math::Log, vec![self])
    }
    pub fn pow(self, y: E) -> E {
        E::Call(Math::Pow, vec![self, y])
    }
    pub fn fma(self, y: E, z: E) -> E {
        E::Call(Math::Fma, vec![self, y, z])
    }
}

pub fn cplx(re: E, im: E) -> E {
    bin(Bin::Cplx, re, im)
}
pub fn sel(c: E, a: E, b: E) -> E {
    E::Sel(Box::new(c), Box::new(a), Box::new(b))
}
pub fn load(a: V, idx: Vec<E>) -> E {
    E::Load(a, idx)
}
pub fn len(a: V, d: u8) -> E {
    E::Len(a, d)
}

impl ops::Add for E {
    type Output = E;
    fn add(self, b: E) -> E {
        bin(Bin::Add, self, b)
    }
}
impl ops::Sub for E {
    type Output = E;
    fn sub(self, b: E) -> E {
        bin(Bin::Sub, self, b)
    }
}
impl ops::Mul for E {
    type Output = E;
    fn mul(self, b: E) -> E {
        bin(Bin::Mul, self, b)
    }
}
impl ops::Div for E {
    type Output = E;
    fn div(self, b: E) -> E {
        bin(Bin::Div, self, b)
    }
}
impl ops::Neg for E {
    type Output = E;
    fn neg(self) -> E {
        un(Un::Neg, self)
    }
}

/// Builds a [`Func`] with nested blocks.
pub struct FnB {
    name: String,
    vars: Vec<Ty>,
    params: Vec<V>,
    blocks: Vec<Vec<S>>,
}

impl FnB {
    pub fn new(name: &str) -> FnB {
        FnB { name: name.into(), vars: vec![], params: vec![], blocks: vec![vec![]] }
    }
    pub fn param(&mut self, ty: Ty) -> V {
        let x = self.var(ty);
        self.params.push(x);
        x
    }
    pub fn var(&mut self, ty: Ty) -> V {
        self.vars.push(ty);
        (self.vars.len() - 1) as V
    }
    fn push(&mut self, s: S) {
        self.blocks.last_mut().unwrap().push(s);
    }
    fn block(&mut self, f: impl FnOnce(&mut FnB)) -> Vec<S> {
        self.blocks.push(vec![]);
        f(self);
        self.blocks.pop().unwrap()
    }
    pub fn set(&mut self, x: V, e: E) {
        self.push(S::Set(x, e));
    }
    pub fn store(&mut self, a: V, idx: Vec<E>, e: E) {
        self.push(S::Store(a, idx, e));
    }
    pub fn while_(&mut self, c: E, f: impl FnOnce(&mut FnB)) {
        let b = self.block(f);
        self.push(S::While(c, b));
    }
    pub fn if_(&mut self, c: E, t: impl FnOnce(&mut FnB), e: impl FnOnce(&mut FnB)) {
        let tb = self.block(t);
        let eb = self.block(e);
        self.push(S::If(c, tb, eb));
    }
    pub fn for_(&mut self, x: V, lo: E, hi: E, f: impl FnOnce(&mut FnB)) {
        let b = self.block(f);
        self.push(S::For(x, lo, hi, b));
    }
    pub fn finish(mut self, ret: Option<E>) -> Func {
        assert_eq!(self.blocks.len(), 1);
        Func { name: self.name, vars: self.vars, params: self.params, body: self.blocks.pop().unwrap(), ret }
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Type checking

pub fn type_of(func: &Func, e: &E) -> Result<Ty, String> {
    use Ty::*;
    let t = |e: &E| type_of(func, e);
    Ok(match e {
        E::I(_) => I64,
        E::F(_) => F64,
        E::B(_) => Bool,
        E::Var(x) => *func.vars.get(*x as usize).ok_or("unknown variable")?,
        E::Un(op, a) => {
            let ta = t(a)?;
            match (op, ta) {
                (Un::Neg, I64 | F64 | C64) => ta,
                (Un::Not, Bool | I64) => ta,
                (Un::Re | Un::Im | Un::Abs2, C64) => F64,
                (Un::Conj, C64) => C64,
                (Un::IToF, I64) => F64,
                (Un::FToI, F64) => I64,
                (Un::Abs | Un::Sqrt | Un::Floor, F64) => F64,
                _ => return Err(format!("{op:?} on {ta:?}")),
            }
        }
        E::Bin(op, a, b) => {
            let (ta, tb) = (t(a)?, t(b)?);
            if ta != tb {
                return Err(format!("{op:?} on {ta:?} and {tb:?}: no implicit conversions"));
            }
            match (op, ta) {
                (Bin::Add | Bin::Sub | Bin::Mul | Bin::Div, I64 | F64 | C64) => ta,
                (Bin::Rem | Bin::Shl | Bin::Shr | Bin::UShr, I64) => I64,
                (Bin::And | Bin::Or | Bin::Xor, I64 | Bool) => ta,
                (Bin::Lt | Bin::Le | Bin::Gt | Bin::Ge, I64 | F64) => Bool,
                (Bin::Eq | Bin::Ne, I64 | F64 | Bool) => Bool,
                (Bin::Cplx, F64) => C64,
                _ => return Err(format!("{op:?} on {ta:?}")),
            }
        }
        E::PowI(a, _) => match t(a)? {
            ta @ (I64 | F64 | C64) => ta,
            ta => return Err(format!("powi on {ta:?}")),
        },
        E::Call(m, args) => {
            let n = match m {
                Math::Pow => 2,
                Math::Fma => 3,
                _ => 1,
            };
            if args.len() != n {
                return Err(format!("{m:?} takes {n} arguments"));
            }
            for a in args {
                if t(a)? != F64 {
                    return Err(format!("{m:?} takes machine reals"));
                }
            }
            F64
        }
        E::Sel(c, a, b) => {
            if t(c)? != Bool {
                return Err("select condition is not boolean".into());
            }
            let (ta, tb) = (t(a)?, t(b)?);
            if ta != tb || matches!(ta, Arr(..)) {
                return Err("select branches differ".into());
            }
            ta
        }
        E::Load(a, idx) => match func.vars.get(*a as usize) {
            Some(Arr(el, r)) if *r as usize == idx.len() => {
                for x in idx {
                    if t(x)? != I64 {
                        return Err("array index is not an integer".into());
                    }
                }
                match el {
                    Elem::I64 => I64,
                    Elem::F64 => F64,
                }
            }
            _ => return Err("load from a non-array or with the wrong rank".into()),
        },
        E::Len(a, d) => match func.vars.get(*a as usize) {
            Some(Arr(_, r)) if *d < *r => I64,
            _ => return Err("length of a non-array".into()),
        },
    })
}

fn check_block(func: &Func, b: &[S]) -> Result<(), String> {
    for s in b {
        match s {
            S::Set(x, e) => {
                let tx = func.vars[*x as usize];
                if matches!(tx, Ty::Arr(..)) {
                    return Err("arrays are not assignable".into());
                }
                let te = type_of(func, e)?;
                if te != tx {
                    return Err(format!("assigning {te:?} to a {tx:?} variable"));
                }
            }
            S::Store(a, idx, e) => {
                let te = type_of(func, &E::Load(*a, idx.clone()))?;
                if type_of(func, e)? != te {
                    return Err("stored value has the wrong type".into());
                }
            }
            S::If(c, t, e) => {
                if type_of(func, c)? != Ty::Bool {
                    return Err("if condition is not boolean".into());
                }
                check_block(func, t)?;
                check_block(func, e)?;
            }
            S::While(c, b) => {
                if type_of(func, c)? != Ty::Bool {
                    return Err("while condition is not boolean".into());
                }
                check_block(func, b)?;
            }
            S::For(x, lo, hi, b) => {
                if func.vars[*x as usize] != Ty::I64 || type_of(func, lo)? != Ty::I64 || type_of(func, hi)? != Ty::I64 {
                    return Err("for needs integer variable and bounds".into());
                }
                check_block(func, b)?;
            }
        }
    }
    Ok(())
}

/// Accepts exactly the functions the compiler handles (semantics 16); anything else stays interpreted as terms.
pub fn check(func: &Func) -> Result<(), String> {
    for &p in &func.params {
        if func.vars[p as usize] == Ty::Bool {
            return Err("boolean parameters are not part of the ABI".into());
        }
    }
    check_block(func, &func.body)?;
    if let Some(r) = &func.ret {
        match type_of(func, r)? {
            Ty::I64 | Ty::F64 | Ty::Bool => {}
            t => return Err(format!("return type {t:?}: complex and array results go through array parameters")),
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------------------------
// LIR

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum K {
    I,
    F,
    B,
}

pub type R = u32;
pub type L = u32;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum IOp {
    Add,
    Sub,
    Mul,
    /// `wrapping_div`; division by zero fails with `Status::DivZero`.
    Div,
    Rem,
    And,
    Or,
    Xor,
    Shl,
    Shr,
    UShr,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FOp {
    Add,
    Sub,
    Mul,
    Div,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FU {
    Neg,
    Abs,
    Sqrt,
    Floor,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Cc {
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BOp {
    And,
    Or,
    Xor,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Lib {
    Sin,
    Cos,
    Exp,
    Log,
    Pow,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Op {
    IConst(R, i64),
    FConst(R, f64),
    BConst(R, bool),
    Mov(R, R),
    IBin(IOp, R, R, R),
    INeg(R, R),
    INot(R, R),
    FBin(FOp, R, R, R),
    FUn(FU, R, R),
    Fma(R, R, R, R),
    ICmp(Cc, R, R, R),
    FCmp(Cc, R, R, R),
    BBin(BOp, R, R, R),
    BNot(R, R),
    Sel(R, R, R, R),
    IToF(R, R),
    FToI(R, R),
    Call1(Lib, R, R),
    Call2(Lib, R, R, R),
    /// `dst = ptr[idx]` (8-byte elements)
    Load(Elem, R, R, R),
    /// `ptr[idx] = val`
    Store(Elem, R, R, R),
    /// fail with `Status::Index` unless `0 <= idx < len`
    Check(R, R),
    Jmp(L),
    Br(R, L, L),
    Label(L),
    /// loop back edge: interrupt and fuel
    Poll,
    Ret(Option<R>),
}

#[derive(Clone, Debug)]
pub struct Lir {
    pub name: String,
    pub regs: Vec<K>,
    /// Flattened parameters in ABI order (the control block pointer comes after them).
    pub params: Vec<R>,
    pub ret: Option<K>,
    pub code: Vec<Op>,
    pub nlabels: u32,
}

struct Lower<'a> {
    func: &'a Func,
    regs: Vec<K>,
    home: Vec<Vec<R>>,
    code: Vec<Op>,
    nlabels: u32,
}

impl Lower<'_> {
    fn reg(&mut self, k: K) -> R {
        self.regs.push(k);
        (self.regs.len() - 1) as R
    }
    fn label(&mut self) -> L {
        self.nlabels += 1;
        self.nlabels - 1
    }
    fn emit(&mut self, op: Op) {
        self.code.push(op);
    }
    fn fbin(&mut self, op: FOp, a: R, b: R) -> R {
        let d = self.reg(K::F);
        self.emit(Op::FBin(op, d, a, b));
        d
    }
    fn ibin(&mut self, op: IOp, a: R, b: R) -> R {
        let d = self.reg(K::I);
        self.emit(Op::IBin(op, d, a, b));
        d
    }
    fn fneg(&mut self, a: R) -> R {
        let d = self.reg(K::F);
        self.emit(Op::FUn(FU::Neg, d, a));
        d
    }

    /// Complex multiplication: `(ar*br - ai*bi, ar*bi + ai*br)`.
    fn cmul(&mut self, a: &[R], b: &[R]) -> Vec<R> {
        let t1 = self.fbin(FOp::Mul, a[0], b[0]);
        let t2 = self.fbin(FOp::Mul, a[1], b[1]);
        let re = self.fbin(FOp::Sub, t1, t2);
        let t3 = self.fbin(FOp::Mul, a[0], b[1]);
        let t4 = self.fbin(FOp::Mul, a[1], b[0]);
        let im = self.fbin(FOp::Add, t3, t4);
        vec![re, im]
    }

    /// The value of `e` in registers (1 for scalars, 2 for complex). May return a variable's home registers.
    fn ex(&mut self, e: &E) -> Vec<R> {
        let ty = type_of(self.func, e).expect("checked");
        match e {
            E::I(n) => {
                let d = self.reg(K::I);
                self.emit(Op::IConst(d, *n));
                vec![d]
            }
            E::F(x) => {
                let d = self.reg(K::F);
                self.emit(Op::FConst(d, *x));
                vec![d]
            }
            E::B(b) => {
                let d = self.reg(K::B);
                self.emit(Op::BConst(d, *b));
                vec![d]
            }
            E::Var(x) => self.home[*x as usize].clone(),
            E::Un(op, a) => {
                let ta = type_of(self.func, a).unwrap();
                let ra = self.ex(a);
                match (op, ta) {
                    (Un::Neg, Ty::I64) => {
                        let d = self.reg(K::I);
                        self.emit(Op::INeg(d, ra[0]));
                        vec![d]
                    }
                    (Un::Neg, Ty::F64) => vec![self.fneg(ra[0])],
                    (Un::Neg, _) => vec![self.fneg(ra[0]), self.fneg(ra[1])],
                    (Un::Not, Ty::Bool) => {
                        let d = self.reg(K::B);
                        self.emit(Op::BNot(d, ra[0]));
                        vec![d]
                    }
                    (Un::Not, _) => {
                        let d = self.reg(K::I);
                        self.emit(Op::INot(d, ra[0]));
                        vec![d]
                    }
                    (Un::Re, _) => vec![ra[0]],
                    (Un::Im, _) => vec![ra[1]],
                    (Un::Abs2, _) => {
                        let a = self.fbin(FOp::Mul, ra[0], ra[0]);
                        let b = self.fbin(FOp::Mul, ra[1], ra[1]);
                        vec![self.fbin(FOp::Add, a, b)]
                    }
                    (Un::Conj, _) => vec![ra[0], self.fneg(ra[1])],
                    (Un::IToF, _) => {
                        let d = self.reg(K::F);
                        self.emit(Op::IToF(d, ra[0]));
                        vec![d]
                    }
                    (Un::FToI, _) => {
                        let d = self.reg(K::I);
                        self.emit(Op::FToI(d, ra[0]));
                        vec![d]
                    }
                    (Un::Abs | Un::Sqrt | Un::Floor, _) => {
                        let u = match op {
                            Un::Abs => FU::Abs,
                            Un::Sqrt => FU::Sqrt,
                            _ => FU::Floor,
                        };
                        let d = self.reg(K::F);
                        self.emit(Op::FUn(u, d, ra[0]));
                        vec![d]
                    }
                }
            }
            E::Bin(op, a, b) => {
                let ta = type_of(self.func, a).unwrap();
                let ra = self.ex(a);
                let rb = self.ex(b);
                let cc = |op: &Bin| match op {
                    Bin::Lt => Cc::Lt,
                    Bin::Le => Cc::Le,
                    Bin::Gt => Cc::Gt,
                    Bin::Ge => Cc::Ge,
                    Bin::Eq => Cc::Eq,
                    _ => Cc::Ne,
                };
                match (op, ta) {
                    (Bin::Cplx, _) => vec![ra[0], rb[0]],
                    (Bin::Lt | Bin::Le | Bin::Gt | Bin::Ge | Bin::Eq | Bin::Ne, _) => {
                        let d = self.reg(K::B);
                        let c = cc(op);
                        let op = match ta {
                            Ty::I64 => Op::ICmp(c, d, ra[0], rb[0]),
                            Ty::F64 => Op::FCmp(c, d, ra[0], rb[0]),
                            _ => {
                                // booleans: compare as xor
                                let x = self.reg(K::B);
                                self.emit(Op::BBin(BOp::Xor, x, ra[0], rb[0]));
                                if c == Cc::Ne {
                                    Op::Mov(d, x)
                                } else {
                                    Op::BNot(d, x)
                                }
                            }
                        };
                        self.emit(op);
                        vec![d]
                    }
                    (_, Ty::Bool) => {
                        let o = match op {
                            Bin::And => BOp::And,
                            Bin::Or => BOp::Or,
                            _ => BOp::Xor,
                        };
                        let d = self.reg(K::B);
                        self.emit(Op::BBin(o, d, ra[0], rb[0]));
                        vec![d]
                    }
                    (_, Ty::I64) => {
                        let o = match op {
                            Bin::Add => IOp::Add,
                            Bin::Sub => IOp::Sub,
                            Bin::Mul => IOp::Mul,
                            Bin::Div => IOp::Div,
                            Bin::Rem => IOp::Rem,
                            Bin::And => IOp::And,
                            Bin::Or => IOp::Or,
                            Bin::Xor => IOp::Xor,
                            Bin::Shl => IOp::Shl,
                            Bin::Shr => IOp::Shr,
                            _ => IOp::UShr,
                        };
                        vec![self.ibin(o, ra[0], rb[0])]
                    }
                    (_, Ty::F64) => {
                        let o = match op {
                            Bin::Add => FOp::Add,
                            Bin::Sub => FOp::Sub,
                            Bin::Mul => FOp::Mul,
                            _ => FOp::Div,
                        };
                        vec![self.fbin(o, ra[0], rb[0])]
                    }
                    (Bin::Add | Bin::Sub, _) => {
                        let o = if *op == Bin::Add { FOp::Add } else { FOp::Sub };
                        vec![self.fbin(o, ra[0], rb[0]), self.fbin(o, ra[1], rb[1])]
                    }
                    (Bin::Mul, _) => self.cmul(&ra, &rb),
                    _ => {
                        // complex division: d = br^2 + bi^2; ((ar*br + ai*bi)/d, (ai*br - ar*bi)/d)
                        let t1 = self.fbin(FOp::Mul, rb[0], rb[0]);
                        let t2 = self.fbin(FOp::Mul, rb[1], rb[1]);
                        let den = self.fbin(FOp::Add, t1, t2);
                        let t3 = self.fbin(FOp::Mul, ra[0], rb[0]);
                        let t4 = self.fbin(FOp::Mul, ra[1], rb[1]);
                        let nr = self.fbin(FOp::Add, t3, t4);
                        let t5 = self.fbin(FOp::Mul, ra[1], rb[0]);
                        let t6 = self.fbin(FOp::Mul, ra[0], rb[1]);
                        let ni = self.fbin(FOp::Sub, t5, t6);
                        vec![self.fbin(FOp::Div, nr, den), self.fbin(FOp::Div, ni, den)]
                    }
                }
            }
            E::PowI(a, n) => {
                let ra = self.ex(a);
                if *n == 0 {
                    return match ty {
                        Ty::I64 => self.ex(&E::I(1)),
                        Ty::F64 => self.ex(&E::F(1.0)),
                        _ => {
                            let mut r = self.ex(&E::F(1.0));
                            r.extend(self.ex(&E::F(0.0)));
                            r
                        }
                    };
                }
                let mut r = ra.clone();
                let top = 31 - n.leading_zeros();
                for bit in (0..top).rev() {
                    r = self.mul_ty(ty, &r, &r.clone());
                    if n >> bit & 1 == 1 {
                        r = self.mul_ty(ty, &r, &ra);
                    }
                }
                r
            }
            E::Call(m, args) => {
                let rs: Vec<R> = args.iter().map(|a| self.ex(a)[0]).collect();
                let d = self.reg(K::F);
                self.emit(match m {
                    Math::Sin => Op::Call1(Lib::Sin, d, rs[0]),
                    Math::Cos => Op::Call1(Lib::Cos, d, rs[0]),
                    Math::Exp => Op::Call1(Lib::Exp, d, rs[0]),
                    Math::Log => Op::Call1(Lib::Log, d, rs[0]),
                    Math::Pow => Op::Call2(Lib::Pow, d, rs[0], rs[1]),
                    Math::Fma => Op::Fma(d, rs[0], rs[1], rs[2]),
                });
                vec![d]
            }
            E::Sel(c, a, b) => {
                let rc = self.ex(c)[0];
                let ra = self.ex(a);
                let rb = self.ex(b);
                ra.iter()
                    .zip(&rb)
                    .map(|(&x, &y)| {
                        let d = self.reg(self.regs[x as usize]);
                        self.emit(Op::Sel(d, rc, x, y));
                        d
                    })
                    .collect()
            }
            E::Load(a, idx) => {
                let (el, ptr, lin) = self.index(*a, idx);
                let d = self.reg(if el == Elem::I64 { K::I } else { K::F });
                self.emit(Op::Load(el, d, ptr, lin));
                vec![d]
            }
            E::Len(a, d) => vec![self.home[*a as usize][1 + *d as usize]],
        }
    }

    fn mul_ty(&mut self, ty: Ty, a: &[R], b: &[R]) -> Vec<R> {
        match ty {
            Ty::I64 => vec![self.ibin(IOp::Mul, a[0], b[0])],
            Ty::F64 => vec![self.fbin(FOp::Mul, a[0], b[0])],
            _ => self.cmul(a, b),
        }
    }

    /// Bounds-checked linear index into array `a`.
    fn index(&mut self, a: V, idx: &[E]) -> (Elem, R, R) {
        let Ty::Arr(el, _) = self.func.vars[a as usize] else { unreachable!() };
        let h = self.home[a as usize].clone();
        let ri: Vec<R> = idx.iter().map(|x| self.ex(x)[0]).collect();
        for (k, &r) in ri.iter().enumerate() {
            self.emit(Op::Check(r, h[1 + k]));
        }
        let lin = if ri.len() == 1 {
            ri[0]
        } else {
            let t = self.ibin(IOp::Mul, ri[0], h[2]);
            self.ibin(IOp::Add, t, ri[1])
        };
        (el, h[0], lin)
    }

    fn assign(&mut self, x: V, src: Vec<R>) {
        let dst = self.home[x as usize].clone();
        // copy through temporaries when a source is one of the destinations (`z = complex(im(z), re(z))`)
        let src = if src.len() > 1 && src.iter().any(|s| dst.contains(s)) {
            src.iter()
                .map(|&s| {
                    let t = self.reg(self.regs[s as usize]);
                    self.emit(Op::Mov(t, s));
                    t
                })
                .collect()
        } else {
            src
        };
        for (d, s) in dst.into_iter().zip(src) {
            if d != s {
                self.emit(Op::Mov(d, s));
            }
        }
    }

    fn block(&mut self, b: &[S]) {
        for s in b {
            match s {
                S::Set(x, e) => {
                    let r = self.ex(e);
                    self.assign(*x, r);
                }
                S::Store(a, idx, e) => {
                    let val = self.ex(e)[0];
                    let (el, ptr, lin) = self.index(*a, idx);
                    self.emit(Op::Store(el, ptr, lin, val));
                }
                S::If(c, t, e) => {
                    let rc = self.ex(c)[0];
                    let (lt, le, lend) = (self.label(), self.label(), self.label());
                    self.emit(Op::Br(rc, lt, le));
                    self.emit(Op::Label(lt));
                    self.block(t);
                    self.emit(Op::Jmp(lend));
                    self.emit(Op::Label(le));
                    self.block(e);
                    self.emit(Op::Jmp(lend));
                    self.emit(Op::Label(lend));
                }
                S::While(c, b) => {
                    let (lh, lb, lx) = (self.label(), self.label(), self.label());
                    self.emit(Op::Jmp(lh));
                    self.emit(Op::Label(lh));
                    let rc = self.ex(c)[0];
                    self.emit(Op::Br(rc, lb, lx));
                    self.emit(Op::Label(lb));
                    self.block(b);
                    self.emit(Op::Poll);
                    self.emit(Op::Jmp(lh));
                    self.emit(Op::Label(lx));
                }
                S::For(x, lo, hi, b) => {
                    let rlo = self.ex(lo);
                    self.assign(*x, rlo);
                    let rhi = self.ex(hi)[0];
                    let h = self.reg(K::I);
                    self.emit(Op::Mov(h, rhi));
                    let xv = self.home[*x as usize][0];
                    let (lh, lb, lx) = (self.label(), self.label(), self.label());
                    self.emit(Op::Jmp(lh));
                    self.emit(Op::Label(lh));
                    let c = self.reg(K::B);
                    self.emit(Op::ICmp(Cc::Lt, c, xv, h));
                    self.emit(Op::Br(c, lb, lx));
                    self.emit(Op::Label(lb));
                    self.block(b);
                    let one = self.reg(K::I);
                    self.emit(Op::IConst(one, 1));
                    self.emit(Op::IBin(IOp::Add, xv, xv, one));
                    self.emit(Op::Poll);
                    self.emit(Op::Jmp(lh));
                    self.emit(Op::Label(lx));
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------
// If-conversion (decision E.3)

/// Cheap, pure and unable to fail: no loads (an out-of-range index must not fail on the untaken side), no integer
/// division, no calls.
fn speculable(func: &Func, e: &E) -> bool {
    let sp = |e: &E| speculable(func, e);
    match e {
        E::I(_) | E::F(_) | E::B(_) | E::Var(_) | E::Len(..) => true,
        E::Un(_, a) | E::PowI(a, _) => sp(a),
        E::Bin(Bin::Div | Bin::Rem, a, _) if type_of(func, a) == Ok(Ty::I64) => false,
        E::Bin(_, a, b) => sp(a) && sp(b),
        E::Sel(c, a, b) => sp(c) && sp(a) && sp(b),
        E::Call(..) | E::Load(..) => false,
    }
}

fn convert_block(func: &Func, b: &[S]) -> Vec<S> {
    b.iter()
        .map(|s| match s {
            S::If(c, t, e) => {
                let one = |blk: &[S]| match blk {
                    [S::Set(x, ex)] if speculable(func, ex) => Some(Some((*x, ex.clone()))),
                    [] => Some(None),
                    _ => None,
                };
                // `if c { v = a } else { v = b }`, or one side empty: `v = select(c, a, b)` with the missing side `v`
                match (one(t), one(e), speculable(func, c)) {
                    (Some(Some((x, a))), Some(Some((y, bb))), true) if x == y => S::Set(x, sel(c.clone(), a, bb)),
                    (Some(Some((x, a))), Some(None), true) => S::Set(x, sel(c.clone(), a, E::Var(x))),
                    (Some(None), Some(Some((x, bb))), true) => S::Set(x, sel(c.clone(), E::Var(x), bb)),
                    _ => S::If(c.clone(), convert_block(func, t), convert_block(func, e)),
                }
            }
            S::While(c, b) => S::While(c.clone(), convert_block(func, b)),
            S::For(x, lo, hi, b) => S::For(*x, lo.clone(), hi.clone(), convert_block(func, b)),
            s => s.clone(),
        })
        .collect()
}

/// Replace small branches that only assign one variable by a `select` (a mispredicted branch costs more than
/// evaluating both sides). Semantics are unchanged: both sides are pure and cannot fail.
pub fn if_convert(func: &Func) -> Func {
    Func { body: convert_block(func, &func.body), ..func.clone() }
}

#[derive(Clone, Copy, Debug)]
pub struct Opts {
    pub if_convert: bool,
}

impl Default for Opts {
    fn default() -> Opts {
        Opts { if_convert: true }
    }
}

/// Type-check and lower a function to LIR with the default passes.
pub fn lower(func: &Func) -> Result<Lir, String> {
    lower_with(func, Opts::default())
}

/// Type-check, optionally if-convert, and lower a function to LIR.
pub fn lower_with(func: &Func, opts: Opts) -> Result<Lir, String> {
    check(func)?;
    let converted;
    let func = if opts.if_convert {
        converted = if_convert(func);
        &converted
    } else {
        func
    };
    let mut lw = Lower { func, regs: vec![], home: vec![], code: vec![], nlabels: 0 };
    for k in 0..func.vars.len() {
        let kinds: &[K] = match func.vars[k] {
            Ty::I64 => &[K::I],
            Ty::F64 => &[K::F],
            Ty::Bool => &[K::B],
            Ty::C64 => &[K::F, K::F],
            Ty::Arr(_, 1) => &[K::I, K::I],
            Ty::Arr(_, _) => &[K::I, K::I, K::I],
        };
        let h = kinds.iter().map(|&kd| lw.reg(kd)).collect();
        lw.home.push(h);
    }
    let params = func.params.iter().flat_map(|&p| lw.home[p as usize].clone()).collect();
    lw.block(&func.body);
    let (ret, r) = match &func.ret {
        Some(e) => {
            let r = lw.ex(e)[0];
            (Some(lw.regs[r as usize]), Some(r))
        }
        None => (None, None),
    };
    lw.emit(Op::Ret(r));
    Ok(Lir { name: func.name.clone(), regs: lw.regs, params, ret, code: lw.code, nlabels: lw.nlabels })
}
