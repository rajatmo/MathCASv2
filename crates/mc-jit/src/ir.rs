//! The typed numeric IR (spike E's decision E.2, reduced to what the front end produces).
//!
//! Two levels:
//! - **HIR** ([`Func`], [`E`], [`S`]): a typed tree. Types are [`Ty::Int`] (an exact integer held in an `i64`),
//!   [`Ty::Real`] (a machine real) and [`Ty::Bool`]; there are no implicit conversions. Expressions have no control
//!   flow (the front end turns `if`, `and`, `or`, blocks and inlined calls into statements), but many of them can
//!   **decline**: an integer overflow, an inexact quotient, a value outside a function's real domain. A declined
//!   call returns nothing and the interpreter evaluates it instead, so results stay exactly the interpreter's
//!   (decision 1b.3).
//! - **LIR** ([`Lir`], [`Op`]): three-address code over typed virtual registers ([`K`]) with labels and branches;
//!   every loop back edge carries a [`Op::Poll`]. Both back ends ([`crate::interp`], [`crate::jit`]) consume it, so
//!   it is the one definition of every operation's order and of every decline.
//!
//! Powers, and sums and products of more than two operands or with exact parts ([`E::Num`]), are lowered to the
//! very computation the arithmetic (`mc_arith::Num`) performs (decision J.2): a power of a machine real is one
//! `powf` call; a sum or product folds its exact operands exactly, converts that part once (correctly rounded),
//! then adds or multiplies the machine reals in canonical order, which a sorting network establishes at run time.
//! What the inline code cannot compute exactly as the arithmetic does (an exact part beyond 64 bits, more than
//! [`MAX_SORTED`] machine reals, a rational in a sum, an exact base) calls the arithmetic itself
//! ([`crate::rt::num`]), so that results agree with the interpreter by construction.
use crate::rt::EXACT_REAL_INT;
use mc_arith::num::{Int, Rat};
use mc_arith::Real;

/// A HIR variable: an index into [`Func::vars`].
pub type V = u32;

/// The machine types.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Ty {
    /// An exact integer that fits in `i64`.
    Int,
    /// A machine real (IEEE binary64).
    Real,
    /// `true` or `false`.
    Bool,
}

impl Ty {
    /// The name of the type in the language.
    pub fn name(self) -> &'static str {
        match self {
            Ty::Int => "Int",
            Ty::Real => "Real",
            Ty::Bool => "Bool",
        }
    }
}

/// Exact integer operations. Each declines when its exact result is not an `i64`: on overflow, a zero divisor, an
/// inexact quotient ([`IOp::DivExact`]) or a negative exponent ([`IOp::Pow`]).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum IOp {
    /// `a + b`
    Add,
    /// `a - b`
    Sub,
    /// `a b`
    Mul,
    /// `a / b` when it is an integer
    DivExact,
    /// `quotient(a, b)`: floor(a / b)
    Quot,
    /// `mod(a, b)`: the remainder with the sign of `b`
    Mod,
    /// `a^b` for `b >= 0` (`0^0 = 1`)
    Pow,
}

/// IEEE operations on machine reals.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FOp {
    /// `a + b`
    Add,
    /// `a - b`
    Sub,
    /// `a b`
    Mul,
    /// `a / b`
    Div,
}

/// Comparison conditions.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Cc {
    /// `<`
    Lt,
    /// `<=`
    Le,
    /// `>`
    Gt,
    /// `>=`
    Ge,
    /// `==`
    Eq,
    /// `!=`
    Ne,
}

/// Boolean connectives (both operands evaluated: the front end makes short-circuit evaluation explicit).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BOp {
    /// conjunction
    And,
    /// disjunction
    Or,
}

/// The elementary functions on machine reals: calls to the one shared set of `mc-numeric` (decision E.4).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Lib {
    /// e^x
    Exp,
    /// natural logarithm (declines for x < 0, where the interpreter gives a complex value)
    Log,
    /// sine
    Sin,
    /// cosine
    Cos,
    /// tangent
    Tan,
    /// inverse tangent
    Atan,
    /// inverse sine (declines outside [-1, 1])
    Asin,
    /// inverse cosine (declines outside [-1, 1])
    Acos,
    /// hyperbolic sine
    Sinh,
    /// hyperbolic cosine
    Cosh,
    /// hyperbolic tangent
    Tanh,
}

impl Lib {
    /// Every function, in the order of the back ends' tables.
    pub const ALL: [Lib; 11] =
        [Lib::Exp, Lib::Log, Lib::Sin, Lib::Cos, Lib::Tan, Lib::Atan, Lib::Asin, Lib::Acos, Lib::Sinh, Lib::Cosh, Lib::Tanh];

    /// The function's name in the language.
    pub fn name(self) -> &'static str {
        match self {
            Lib::Exp => "exp",
            Lib::Log => "log",
            Lib::Sin => "sin",
            Lib::Cos => "cos",
            Lib::Tan => "tan",
            Lib::Atan => "atan",
            Lib::Asin => "asin",
            Lib::Acos => "acos",
            Lib::Sinh => "sinh",
            Lib::Cosh => "cosh",
            Lib::Tanh => "tanh",
        }
    }

    /// The function with this name.
    pub fn by_name(n: &str) -> Option<Lib> {
        Lib::ALL.into_iter().find(|l| l.name() == n)
    }
}

/// An arithmetic operation computed by the arithmetic's own code ([`crate::rt::num`]).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NumOp {
    /// the canonical sum of the operands (exact ones first, inexact ones in canonical order)
    Sum,
    /// the canonical product, in the same order
    Product,
    /// the first operand to the power of the second
    Pow,
}

/// An operand of a [`NumOp`]: an exact integer, a machine real, or the exact rational `num / den`.
#[derive(Clone, Debug)]
pub enum Opd {
    /// an exact integer
    Int(E),
    /// a machine real
    Real(E),
    /// the exact rational `num / den` (a zero denominator declines)
    Ratio(E, E),
}

/// HIR expressions. Every operand has the type the operation names; [`type_of`] checks it.
#[derive(Clone, Debug)]
pub enum E {
    /// an integer constant
    Int(i64),
    /// a machine-real constant
    Real(f64),
    /// a boolean constant
    Bool(bool),
    /// a variable
    Var(V),
    /// an exact integer operation (may decline)
    I(IOp, Box<E>, Box<E>),
    /// an IEEE operation on machine reals
    F(FOp, Box<E>, Box<E>),
    /// a comparison of two integers or two machine reals
    Cmp(Cc, Box<E>, Box<E>),
    /// a connective of two booleans
    Logic(BOp, Box<E>, Box<E>),
    /// boolean negation
    Not(Box<E>),
    /// an integer as a machine real; declines beyond 2^53, where the conversion could round
    ToReal(Box<E>),
    /// the floor of a machine real as an integer; declines for a non-finite value or one beyond `i64`
    Floor(Box<E>),
    /// |x| of a machine real
    AbsF(Box<E>),
    /// the larger (`true`) or smaller of two machine reals, the first at a tie; declines for a NaN
    MinMaxF(bool, Box<E>, Box<E>),
    /// an elementary function (may decline outside the real domain)
    Math(Lib, Box<E>),
    /// an operation of the arithmetic with a machine-real result (declines otherwise)
    Num(NumOp, Vec<Opd>),
    /// `c ? a : b`, both sides evaluated (they are pure)
    Sel(Box<E>, Box<E>, Box<E>),
}

/// HIR statements.
#[derive(Clone, Debug)]
pub enum S {
    /// `x = e`
    Set(V, E),
    /// `e` evaluated for its declines only
    Eval(E),
    /// output slot `k` of a vector result = `e` (decision N0.3: written to the [`crate::rt::Env`]'s output buffer;
    /// a function with outputs returns its scalar result as usual, which callers ignore)
    Out(u32, E),
    /// `if c { a } else { b }`
    If(E, Vec<S>, Vec<S>),
    /// `while c { body }`
    While(E, Vec<S>),
    /// `for x in range(lo, hi, step) { body }` (inclusive bounds; the bounds and step are evaluated once; a zero step
    /// declines; the loop stops when the next value would overflow)
    For {
        /// the loop variable (read-only in the body)
        var: V,
        /// first value
        lo: E,
        /// last value (inclusive)
        hi: E,
        /// step
        step: E,
        /// the body
        body: Vec<S>,
    },
}

/// A function in HIR.
#[derive(Clone, Debug)]
pub struct Func {
    /// for diagnostics and symbol names in the back end
    pub name: String,
    /// the type of every variable
    pub vars: Vec<Ty>,
    /// the parameters, in order (`Int` or `Real`)
    pub params: Vec<V>,
    /// the statements
    pub body: Vec<S>,
    /// the result
    pub ret: E,
}

// ---------------------------------------------------------------------------------------------------------------
// Type checking: the front end's output is checked again before lowering (a cheap consistency check)

fn want(func: &Func, e: &E, t: Ty) -> Result<(), String> {
    let got = type_of(func, e)?;
    if got == t {
        Ok(())
    } else {
        Err(format!("expected {} but found {}", t.name(), got.name()))
    }
}

/// The type of `e`, or why it is ill-typed.
pub fn type_of(func: &Func, e: &E) -> Result<Ty, String> {
    use Ty::*;
    Ok(match e {
        E::Int(_) => Int,
        E::Real(_) => Real,
        E::Bool(_) => Bool,
        E::Var(x) => *func.vars.get(*x as usize).ok_or("unknown variable")?,
        E::I(_, a, b) => {
            want(func, a, Int)?;
            want(func, b, Int)?;
            Int
        }
        E::F(_, a, b) | E::MinMaxF(_, a, b) => {
            want(func, a, Real)?;
            want(func, b, Real)?;
            Real
        }
        E::Cmp(_, a, b) => {
            let t = type_of(func, a)?;
            if t == Bool {
                return Err("comparison of booleans".into());
            }
            want(func, b, t)?;
            Bool
        }
        E::Logic(_, a, b) => {
            want(func, a, Bool)?;
            want(func, b, Bool)?;
            Bool
        }
        E::Not(a) => {
            want(func, a, Bool)?;
            Bool
        }
        E::ToReal(a) => {
            want(func, a, Int)?;
            Real
        }
        E::Floor(a) => {
            want(func, a, Real)?;
            Int
        }
        E::AbsF(a) | E::Math(_, a) => {
            want(func, a, Real)?;
            Real
        }
        E::Num(op, opds) => {
            if *op == NumOp::Pow && opds.len() != 2 {
                return Err("a power takes two operands".into());
            }
            for o in opds {
                match o {
                    Opd::Int(x) => want(func, x, Int)?,
                    Opd::Real(x) => want(func, x, Real)?,
                    Opd::Ratio(n, d) => {
                        want(func, n, Int)?;
                        want(func, d, Int)?;
                    }
                }
            }
            Real
        }
        E::Sel(c, a, b) => {
            want(func, c, Bool)?;
            let t = type_of(func, a)?;
            want(func, b, t)?;
            t
        }
    })
}

fn check_block(func: &Func, b: &[S]) -> Result<(), String> {
    for s in b {
        match s {
            S::Set(x, e) => want(func, e, *func.vars.get(*x as usize).ok_or("unknown variable")?)?,
            S::Eval(e) => {
                type_of(func, e)?;
            }
            S::Out(_, e) => {
                if type_of(func, e)? == Ty::Bool {
                    return Err("a boolean output".into());
                }
            }
            S::If(c, t, e) => {
                want(func, c, Ty::Bool)?;
                check_block(func, t)?;
                check_block(func, e)?;
            }
            S::While(c, b) => {
                want(func, c, Ty::Bool)?;
                check_block(func, b)?;
            }
            S::For { var, lo, hi, step, body } => {
                if func.vars.get(*var as usize) != Some(&Ty::Int) {
                    return Err("a loop variable that is not an integer".into());
                }
                for e in [lo, hi, step] {
                    want(func, e, Ty::Int)?;
                }
                check_block(func, body)?;
            }
        }
    }
    Ok(())
}

/// Accepts exactly the functions the back ends handle.
pub fn check(func: &Func) -> Result<(), String> {
    for &p in &func.params {
        if func.vars.get(p as usize) == Some(&Ty::Bool) {
            return Err("boolean parameters are not part of the ABI".into());
        }
    }
    check_block(func, &func.body)?;
    type_of(func, &func.ret).map(|_| ())
}

/// Number of HIR nodes (a size measure for limits and `compiled_info`).
pub fn size(func: &Func) -> usize {
    fn e(x: &E) -> usize {
        1 + match x {
            E::Int(_) | E::Real(_) | E::Bool(_) | E::Var(_) => 0,
            E::I(_, a, b) | E::F(_, a, b) | E::Cmp(_, a, b) | E::Logic(_, a, b) | E::MinMaxF(_, a, b) => e(a) + e(b),
            E::Not(a) | E::ToReal(a) | E::Floor(a) | E::AbsF(a) | E::Math(_, a) => e(a),
            E::Num(_, os) => os
                .iter()
                .map(|o| match o {
                    Opd::Int(x) | Opd::Real(x) => e(x),
                    Opd::Ratio(n, d) => e(n) + e(d),
                })
                .sum(),
            E::Sel(c, a, b) => e(c) + e(a) + e(b),
        }
    }
    fn b(ss: &[S]) -> usize {
        ss.iter()
            .map(|s| {
                1 + match s {
                    S::Set(_, x) | S::Eval(x) | S::Out(_, x) => e(x),
                    S::If(c, t, f) => e(c) + b(t) + b(f),
                    S::While(c, body) => e(c) + b(body),
                    S::For { lo, hi, step, body, .. } => e(lo) + e(hi) + e(step) + b(body),
                }
            })
            .sum()
    }
    b(&func.body) + e(&func.ret)
}

// ---------------------------------------------------------------------------------------------------------------
// If-conversion (decision E.3)

/// Pure and unable to decline: evaluating it on the side of a branch not taken changes nothing.
fn speculable(e: &E) -> bool {
    match e {
        E::Int(_) | E::Real(_) | E::Bool(_) | E::Var(_) => true,
        E::F(_, a, b) | E::Cmp(_, a, b) | E::Logic(_, a, b) => speculable(a) && speculable(b),
        E::Not(a) | E::AbsF(a) => speculable(a),
        E::Sel(c, a, b) => speculable(c) && speculable(a) && speculable(b),
        E::I(..) | E::ToReal(_) | E::Floor(_) | E::MinMaxF(..) | E::Math(..) | E::Num(..) => false,
    }
}

fn sel(c: E, a: E, b: E) -> E {
    E::Sel(Box::new(c), Box::new(a), Box::new(b))
}

fn convert_block(b: &[S]) -> Vec<S> {
    b.iter()
        .map(|s| match s {
            S::If(c, t, e) => {
                let one = |blk: &[S]| match blk {
                    [S::Set(x, ex)] if speculable(ex) => Some(Some((*x, ex.clone()))),
                    [] => Some(None),
                    _ => None,
                };
                // `if c { v = a } else { v = b }`, or one side empty: `v = select(c, a, b)` with the missing side `v`
                match (one(t), one(e), speculable(c)) {
                    (Some(Some((x, a))), Some(Some((y, bb))), true) if x == y => S::Set(x, sel(c.clone(), a, bb)),
                    (Some(Some((x, a))), Some(None), true) => S::Set(x, sel(c.clone(), a, E::Var(x))),
                    (Some(None), Some(Some((x, bb))), true) => S::Set(x, sel(c.clone(), E::Var(x), bb)),
                    _ => S::If(c.clone(), convert_block(t), convert_block(e)),
                }
            }
            S::While(c, b) => S::While(c.clone(), convert_block(b)),
            S::For { var, lo, hi, step, body } => {
                S::For { var: *var, lo: lo.clone(), hi: hi.clone(), step: step.clone(), body: convert_block(body) }
            }
            s => s.clone(),
        })
        .collect()
}

/// Replace small branches that only assign one variable by a select (a mispredicted branch costs more than
/// evaluating both sides). Semantics are unchanged: both sides are pure and cannot decline.
pub fn if_convert(func: &Func) -> Func {
    Func { body: convert_block(&func.body), ..func.clone() }
}

// ---------------------------------------------------------------------------------------------------------------
// LIR

/// Register kinds.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum K {
    /// a 64-bit integer
    I,
    /// a machine real
    F,
    /// a boolean
    B,
}

/// Most machine reals of one sum or product sorted inline; with more, the arithmetic's own code sorts them.
pub const MAX_SORTED: usize = 6;

/// Sorting networks for 2 to [`MAX_SORTED`] values (comparators as index pairs, the minimum to the first), each
/// with the fewest comparators known for its size.
pub const NETWORKS: [&[(usize, usize)]; MAX_SORTED + 1] = [
    &[],
    &[],
    &[(0, 1)],
    &[(0, 1), (0, 2), (1, 2)],
    &[(0, 1), (2, 3), (0, 2), (1, 3), (1, 2)],
    &[(0, 1), (3, 4), (2, 4), (2, 3), (0, 3), (0, 2), (1, 4), (1, 3), (1, 2)],
    &[(1, 2), (4, 5), (0, 2), (3, 5), (0, 1), (3, 4), (2, 5), (0, 3), (1, 4), (2, 4), (1, 3), (2, 3)],
];

/// What lowering knows of the exponent of a power of machine reals.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Exp {
    /// a run-time value
    Unknown,
    /// integral or not finite: the power is real for every base
    Integral,
    /// finite and not integral: the power is real only for a base >= 0, a NaN or an infinity
    Fractional,
}

/// A LIR register.
pub type R = u32;
/// A LIR label.
pub type L = u32;

/// The kind of a [`NumOp`] operand in the LIR: which registers hold it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum OpdK {
    /// one `I` register
    Int,
    /// one `F` register
    Real,
    /// two `I` registers: numerator and denominator
    Ratio,
}

/// LIR instructions. Destinations come first.
#[derive(Clone, PartialEq, Debug)]
pub enum Op {
    /// `d = n`
    IConst(R, i64),
    /// `d = x`
    FConst(R, f64),
    /// `d = b`
    BConst(R, bool),
    /// `d = a`
    Mov(R, R),
    /// `d = a op b`, exactly; declines when the result is not an `i64` ([`IOp`])
    IChk(IOp, R, R, R),
    /// `d = a + b` wrapping, `of` = whether it overflowed (loop steps)
    IAddOv(R, R, R, R),
    /// `d = a op b` (IEEE)
    FBin(FOp, R, R, R),
    /// `d = |a|`
    FAbs(R, R),
    /// `d = a cc b` on integers
    ICmp(Cc, R, R, R),
    /// `d = a cc b` on machine reals (IEEE: false with a NaN, except `!=`)
    FCmp(Cc, R, R, R),
    /// `d = a op b` on booleans
    BBin(BOp, R, R, R),
    /// `d = !a`
    BNot(R, R),
    /// `d = c ? a : b`
    Sel(R, R, R, R),
    /// `d = a` as a machine real; declines when |a| > 2^53
    ToReal(R, R),
    /// `d = floor(a)` as an integer; declines for a non-finite `a` or one outside `i64`
    Floor(R, R),
    /// `d = f(a)` (the domain was checked before)
    Math(Lib, R, R),
    /// `d = op(operands)` through the arithmetic; declines unless the result is a machine real. Each operand is its
    /// kind and its registers (the second is the denominator of a ratio, else equal to the first).
    Num(NumOp, R, Vec<(OpdK, R, R)>),
    /// `d = a b` wrapping, `of` = whether it overflowed (exact parts of products)
    IMulOv(R, R, R, R),
    /// `d = max(a, b)` (`true`) or `min(a, b)` of integers
    IMinMax(bool, R, R, R),
    /// `d = a` rounded to the nearest machine real (ties to even): exact up to 2^53, never declines
    IToF(R, R),
    /// `d = trunc(a)` (towards zero; infinities and NaN unchanged)
    FTrunc(R, R),
    /// `d` = the integer whose signed order is the canonical order of machine reals ([`crate::rt::order_key`])
    FKey(R, R),
    /// `d` = the machine real whose key is `a` (the inverse of [`Op::FKey`])
    FromKey(R, R),
    /// `d = a^k` for a machine real `a` and an integer `k != 0`, as the arithmetic computes it ([`crate::rt::pow_int`])
    PowI(R, R, R),
    /// `d = a^c` for machine reals with a real result ([`crate::rt::pow_real`]; the domain was checked before)
    PowF(R, R, R),
    /// decline when `c` is true
    Bail(R),
    /// jump
    Jmp(L),
    /// `if c goto a else goto b`
    Br(R, L, L),
    /// a jump target
    Label(L),
    /// loop back edge: interrupts and fuel (decision E.5)
    Poll,
    /// return the value of a register
    Ret(R),
    /// write a register (an `I` or `F` word) to slot `k` of the environment's output buffer (decision N0.3)
    Out(u32, R),
}

/// A function in LIR.
#[derive(Clone, Debug)]
pub struct Lir {
    /// the function's name
    pub name: String,
    /// the kind of every register
    pub regs: Vec<K>,
    /// the parameters in ABI order (the environment pointer comes after them)
    pub params: Vec<R>,
    /// the kind of the result
    pub ret: K,
    /// the instructions
    pub code: Vec<Op>,
    /// number of labels
    pub nlabels: u32,
}

impl Lir {
    /// Number of instructions (labels excluded).
    pub fn len(&self) -> usize {
        self.code.iter().filter(|o| !matches!(o, Op::Label(_))).count()
    }

    /// Whether there are no instructions.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

fn kind(t: Ty) -> K {
    match t {
        Ty::Int => K::I,
        Ty::Real => K::F,
        Ty::Bool => K::B,
    }
}

struct Lower<'a> {
    func: &'a Func,
    regs: Vec<K>,
    /// the register of each variable
    home: Vec<R>,
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
    fn iconst(&mut self, n: i64) -> R {
        let d = self.reg(K::I);
        self.emit(Op::IConst(d, n));
        d
    }
    fn fconst(&mut self, x: f64) -> R {
        let d = self.reg(K::F);
        self.emit(Op::FConst(d, x));
        d
    }
    fn fcmp(&mut self, c: Cc, a: R, b: R) -> R {
        let d = self.reg(K::B);
        self.emit(Op::FCmp(c, d, a, b));
        d
    }
    fn bail_if_nan(&mut self, a: R) {
        let n = self.fcmp(Cc::Ne, a, a);
        self.emit(Op::Bail(n));
    }

    /// The register holding the value of `e` (a variable's own register for a variable).
    fn ex(&mut self, e: &E) -> R {
        match e {
            E::Int(n) => self.iconst(*n),
            E::Real(x) => self.fconst(*x),
            E::Bool(b) => {
                let d = self.reg(K::B);
                self.emit(Op::BConst(d, *b));
                d
            }
            E::Var(x) => self.home[*x as usize],
            E::I(op, a, b) => {
                let (ra, rb) = (self.ex(a), self.ex(b));
                let d = self.reg(K::I);
                self.emit(Op::IChk(*op, d, ra, rb));
                d
            }
            E::F(op, a, b) => {
                let (ra, rb) = (self.ex(a), self.ex(b));
                let d = self.reg(K::F);
                self.emit(Op::FBin(*op, d, ra, rb));
                d
            }
            E::Cmp(c, a, b) => {
                let ta = type_of(self.func, a).expect("checked");
                let (ra, rb) = (self.ex(a), self.ex(b));
                let d = self.reg(K::B);
                self.emit(if ta == Ty::Int { Op::ICmp(*c, d, ra, rb) } else { Op::FCmp(*c, d, ra, rb) });
                d
            }
            E::Logic(op, a, b) => {
                let (ra, rb) = (self.ex(a), self.ex(b));
                let d = self.reg(K::B);
                self.emit(Op::BBin(*op, d, ra, rb));
                d
            }
            E::Not(a) => {
                let ra = self.ex(a);
                let d = self.reg(K::B);
                self.emit(Op::BNot(d, ra));
                d
            }
            E::ToReal(a) => {
                let ra = self.ex(a);
                let d = self.reg(K::F);
                self.emit(Op::ToReal(d, ra));
                d
            }
            E::Floor(a) => {
                let ra = self.ex(a);
                let d = self.reg(K::I);
                self.emit(Op::Floor(d, ra));
                d
            }
            E::AbsF(a) => {
                let ra = self.ex(a);
                let d = self.reg(K::F);
                self.emit(Op::FAbs(d, ra));
                d
            }
            E::MinMaxF(max, a, b) => {
                // the library's max: the first operand unless the second is greater; a NaN is not decided
                let (ra, rb) = (self.ex(a), self.ex(b));
                self.bail_if_nan(ra);
                self.bail_if_nan(rb);
                let c = self.fcmp(if *max { Cc::Gt } else { Cc::Lt }, rb, ra);
                let d = self.reg(K::F);
                self.emit(Op::Sel(d, c, rb, ra));
                d
            }
            E::Math(f, a) => {
                let ra = self.ex(a);
                match f {
                    Lib::Log => {
                        let z = self.fconst(0.0);
                        let bad = self.fcmp(Cc::Lt, ra, z);
                        self.emit(Op::Bail(bad));
                    }
                    Lib::Asin | Lib::Acos => {
                        for (c, lim) in [(Cc::Lt, -1.0), (Cc::Gt, 1.0)] {
                            let l = self.fconst(lim);
                            let bad = self.fcmp(c, ra, l);
                            self.emit(Op::Bail(bad));
                        }
                    }
                    _ => {}
                }
                let d = self.reg(K::F);
                self.emit(Op::Math(*f, d, ra));
                d
            }
            E::Num(op, opds) => self.num(*op, opds),
            E::Sel(c, a, b) => {
                let t = type_of(self.func, a).expect("checked");
                let (rc, ra, rb) = (self.ex(c), self.ex(a), self.ex(b));
                let d = self.reg(kind(t));
                self.emit(Op::Sel(d, rc, ra, rb));
                d
            }
        }
    }

    /// An [`E::Num`]: the operands in order, then the arithmetic's computation inline where it can be.
    fn num(&mut self, op: NumOp, opds: &[Opd]) -> R {
        let mut rs = Vec::with_capacity(opds.len());
        for o in opds {
            rs.push(match o {
                Opd::Int(x) => {
                    let r = self.ex(x);
                    (OpdK::Int, r, r)
                }
                Opd::Real(x) => {
                    let r = self.ex(x);
                    (OpdK::Real, r, r)
                }
                Opd::Ratio(n, dd) => {
                    let (rn, rd) = (self.ex(n), self.ex(dd));
                    (OpdK::Ratio, rn, rd)
                }
            });
        }
        match op {
            NumOp::Pow => self.pow(opds, rs),
            NumOp::Sum | NumOp::Product => self.fold(op, rs),
        }
    }

    /// The arithmetic's own code: [`Op::Num`].
    fn helper(&mut self, op: NumOp, rs: Vec<(OpdK, R, R)>) -> R {
        let d = self.reg(K::F);
        self.emit(Op::Num(op, d, rs));
        d
    }

    fn bnot(&mut self, a: R) -> R {
        let d = self.reg(K::B);
        self.emit(Op::BNot(d, a));
        d
    }

    /// `a or b`, where a missing `a` is false.
    fn bor(&mut self, a: Option<R>, b: R) -> R {
        match a {
            None => b,
            Some(a) => {
                let d = self.reg(K::B);
                self.emit(Op::BBin(BOp::Or, d, a, b));
                d
            }
        }
    }

    fn icmp(&mut self, c: Cc, a: R, b: R) -> R {
        let d = self.reg(K::B);
        self.emit(Op::ICmp(c, d, a, b));
        d
    }

    /// `x^k` for a machine real and an integer: `Num::pow_int` gives the exact 1 for `k = 0` (declined: the result
    /// is not a machine real) and otherwise one `powf` with the signed exponent.
    fn pow_int(&mut self, x: R, k: R) -> R {
        let zero = self.iconst(0);
        let z = self.icmp(Cc::Eq, k, zero);
        self.emit(Op::Bail(z));
        let d = self.reg(K::F);
        self.emit(Op::PowI(d, x, k));
        d
    }

    /// `x^c` for machine reals as `Num::pow_machine` computes it: `powf` when the result is real (x >= 0, x a NaN
    /// or an infinity, or c integral or not finite); otherwise the result is complex and the call declines. What is
    /// known of a constant exponent saves the run-time test.
    fn pow_real(&mut self, x: R, c: R, exp: Exp) -> R {
        if exp != Exp::Integral {
            let zero = self.fconst(0.0);
            let nonneg = self.fcmp(Cc::Ge, x, zero);
            let nan = self.fcmp(Cc::Ne, x, x);
            let mut ok = self.bor(Some(nonneg), nan);
            let ax = self.reg(K::F);
            self.emit(Op::FAbs(ax, x));
            let inf = self.fconst(f64::INFINITY);
            let is_inf = self.fcmp(Cc::Eq, ax, inf);
            ok = self.bor(Some(ok), is_inf);
            if exp == Exp::Unknown {
                // c.fract() == 0 or c not finite: trunc(c) == c holds for the integral values and the infinities,
                // and a NaN is the remaining non-finite exponent
                let t = self.reg(K::F);
                self.emit(Op::FTrunc(t, c));
                let integral = self.fcmp(Cc::Eq, t, c);
                ok = self.bor(Some(ok), integral);
                let cnan = self.fcmp(Cc::Ne, c, c);
                ok = self.bor(Some(ok), cnan);
            }
            let bad = self.bnot(ok);
            self.emit(Op::Bail(bad));
        }
        let d = self.reg(K::F);
        self.emit(Op::PowF(d, x, c));
        d
    }

    /// A power with a machine-real base, inline; an exact base goes to the arithmetic.
    fn pow(&mut self, opds: &[Opd], rs: Vec<(OpdK, R, R)>) -> R {
        let [(OpdK::Real, x, _), (ek, e, _)] = rs[..] else { return self.helper(NumOp::Pow, rs) };
        match ek {
            OpdK::Int => self.pow_int(x, e),
            OpdK::Real => self.pow_real(x, e, Exp::Unknown),
            OpdK::Ratio => {
                // a constant rational exponent is converted here exactly as the arithmetic converts it
                let Opd::Ratio(E::Int(n), E::Int(den)) = opds[1] else { return self.helper(NumOp::Pow, rs) };
                if den == 0 {
                    return self.helper(NumOp::Pow, rs);
                }
                let q = Rat::new(Int::from_i64(n), Int::from_i64(den));
                if q.is_int() {
                    return match q.num.to_i64() {
                        Some(k) => {
                            let k = self.iconst(k);
                            self.pow_int(x, k)
                        }
                        None => self.helper(NumOp::Pow, rs),
                    };
                }
                // the machine real nearest q, which can be integral (q beyond 2^53)
                let c = Real::Exact(q).to_f64();
                let exp = if c.fract() == 0.0 || !c.is_finite() { Exp::Integral } else { Exp::Fractional };
                let rc = self.fconst(c);
                self.pow_real(x, rc, exp)
            }
        }
    }

    /// A sum or product with machine reals as `Num::sum` / `Num::product` computes it: the exact operands combined
    /// exactly and converted once (correctly rounded), then the machine reals in canonical order, folded from the
    /// left starting with the exact part. The exact part is computed inline when it fits 64 bits (and, for a
    /// product with rational parts, when numerator and denominator are exactly machine reals, so that one IEEE
    /// division rounds the quotient correctly); otherwise, and for a rational in a sum or more than
    /// [`MAX_SORTED`] machine reals, the arithmetic computes the whole operation.
    fn fold(&mut self, op: NumOp, rs: Vec<(OpdK, R, R)>) -> R {
        let reals: Vec<R> = rs.iter().filter(|o| o.0 == OpdK::Real).map(|o| o.1).collect();
        let has_ratio = rs.iter().any(|o| o.0 == OpdK::Ratio);
        if reals.is_empty() || reals.len() > MAX_SORTED || (op == NumOp::Sum && has_ratio) {
            return self.helper(op, rs);
        }
        let fop = if op == NumOp::Sum { FOp::Add } else { FOp::Mul };
        // the exact part: numerator (and denominator), and when it cannot be computed here
        let mut bad: Option<R> = None;
        let (mut num, mut den): (Option<R>, Option<R>) = (None, None);
        for &(k, a, b) in rs.iter().filter(|o| o.0 != OpdK::Real) {
            num = Some(self.exact_step(op, &mut bad, num, a));
            if k == OpdK::Ratio {
                den = Some(self.exact_step(op, &mut bad, den, b));
            }
        }
        let exact = num.map(|n| {
            let nf = self.reg(K::F);
            self.emit(Op::IToF(nf, n));
            let Some(dn) = den else { return nf };
            let (lo, hi, zero) = (self.iconst(-EXACT_REAL_INT), self.iconst(EXACT_REAL_INT), self.iconst(0));
            for v in [n, dn] {
                let below = self.icmp(Cc::Lt, v, lo);
                bad = Some(self.bor(bad, below));
                let above = self.icmp(Cc::Gt, v, hi);
                bad = Some(self.bor(bad, above));
            }
            let dz = self.icmp(Cc::Eq, dn, zero);
            bad = Some(self.bor(bad, dz));
            let df = self.reg(K::F);
            self.emit(Op::IToF(df, dn));
            let q = self.reg(K::F);
            self.emit(Op::FBin(FOp::Div, q, nf, df));
            // the exact zero converts to +0, whatever the sign of the denominator
            let nz = self.icmp(Cc::Eq, n, zero);
            let pz = self.fconst(0.0);
            let d = self.reg(K::F);
            self.emit(Op::Sel(d, nz, pz, q));
            d
        });
        let d = self.reg(K::F);
        let done = bad.map(|bad| {
            // the arithmetic itself, on the same operands, when the exact part is out of reach
            let (slow, fast, done) = (self.label(), self.label(), self.label());
            self.emit(Op::Br(bad, slow, fast));
            self.emit(Op::Label(slow));
            self.emit(Op::Num(op, d, rs.clone()));
            self.emit(Op::Jmp(done));
            self.emit(Op::Label(fast));
            done
        });
        let sorted = self.sort(&reals);
        let mut it = sorted.into_iter();
        let mut acc = match exact {
            Some(x) => x,
            None => it.next().expect("a machine real"),
        };
        for x in it {
            let s = self.reg(K::F);
            self.emit(Op::FBin(fop, s, acc, x));
            acc = s;
        }
        self.emit(Op::Mov(d, acc));
        if let Some(done) = done {
            self.emit(Op::Label(done));
        }
        d
    }

    /// One step of an exact sum or product `acc op x` (the first operand alone starts it), wrapping, with its
    /// overflow joined to `bad`.
    fn exact_step(&mut self, op: NumOp, bad: &mut Option<R>, acc: Option<R>, x: R) -> R {
        let Some(y) = acc else { return x };
        let (d, of) = (self.reg(K::I), self.reg(K::B));
        self.emit(if op == NumOp::Sum { Op::IAddOv(d, of, y, x) } else { Op::IMulOv(d, of, y, x) });
        *bad = Some(self.bor(*bad, of));
        d
    }

    /// The machine reals in `xs` in canonical order: their order keys through a sorting network (the minimum and
    /// the maximum of each compared pair), then back to machine reals. Equal keys are equal bit patterns, so the
    /// result does not depend on how ties are broken.
    fn sort(&mut self, xs: &[R]) -> Vec<R> {
        if xs.len() < 2 {
            return xs.to_vec();
        }
        let mut keys: Vec<R> = Vec::with_capacity(xs.len());
        for &x in xs {
            let k = self.reg(K::I);
            self.emit(Op::FKey(k, x));
            keys.push(k);
        }
        for &(i, j) in NETWORKS[xs.len()] {
            let (lo, hi) = (self.reg(K::I), self.reg(K::I));
            self.emit(Op::IMinMax(false, lo, keys[i], keys[j]));
            self.emit(Op::IMinMax(true, hi, keys[i], keys[j]));
            keys[i] = lo;
            keys[j] = hi;
        }
        keys.into_iter()
            .map(|k| {
                let x = self.reg(K::F);
                self.emit(Op::FromKey(x, k));
                x
            })
            .collect()
    }

    fn block(&mut self, b: &[S]) {
        for s in b {
            match s {
                S::Set(x, e) => {
                    let r = self.ex(e);
                    let d = self.home[*x as usize];
                    if d != r {
                        self.emit(Op::Mov(d, r));
                    }
                }
                S::Eval(e) => {
                    self.ex(e);
                }
                S::Out(k, e) => {
                    let r = self.ex(e);
                    self.emit(Op::Out(*k, r));
                }
                S::If(c, t, e) => {
                    let rc = self.ex(c);
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
                    let rc = self.ex(c);
                    self.emit(Op::Br(rc, lb, lx));
                    self.emit(Op::Label(lb));
                    self.block(b);
                    self.emit(Op::Poll);
                    self.emit(Op::Jmp(lh));
                    self.emit(Op::Label(lx));
                }
                S::For { var, lo, hi, step, body } => self.for_loop(*var, lo, hi, step, body),
            }
        }
    }

    /// `for x in range(lo, hi, step)` as the interpreter runs it: while `(step > 0 and i <= hi) or (step < 0 and
    /// i >= hi)`, run the body with `x = i`, then stop if `i + step` overflows, else continue with it.
    fn for_loop(&mut self, var: V, lo: &E, hi: &E, step: &E, body: &[S]) {
        let rlo = self.ex(lo);
        let i = self.reg(K::I);
        self.emit(Op::Mov(i, rlo));
        let rhi = self.ex(hi);
        let h = self.reg(K::I);
        self.emit(Op::Mov(h, rhi));
        let rst = self.ex(step);
        let st = self.reg(K::I);
        self.emit(Op::Mov(st, rst));
        let zero = self.iconst(0);
        let z = self.reg(K::B);
        self.emit(Op::ICmp(Cc::Eq, z, st, zero));
        self.emit(Op::Bail(z));
        let up = self.reg(K::B);
        self.emit(Op::ICmp(Cc::Gt, up, st, zero));
        let x = self.home[var as usize];
        let (lh, lb, lx) = (self.label(), self.label(), self.label());
        self.emit(Op::Jmp(lh));
        self.emit(Op::Label(lh));
        // (up and i <= h) or (not up and i >= h)
        let (le, ge) = (self.reg(K::B), self.reg(K::B));
        self.emit(Op::ICmp(Cc::Le, le, i, h));
        self.emit(Op::ICmp(Cc::Ge, ge, i, h));
        let go = self.reg(K::B);
        self.emit(Op::Sel(go, up, le, ge));
        self.emit(Op::Br(go, lb, lx));
        self.emit(Op::Label(lb));
        self.emit(Op::Mov(x, i));
        self.block(body);
        let of = self.reg(K::B);
        self.emit(Op::IAddOv(i, of, i, st));
        let lc = self.label();
        self.emit(Op::Br(of, lx, lc));
        self.emit(Op::Label(lc));
        self.emit(Op::Poll);
        self.emit(Op::Jmp(lh));
        self.emit(Op::Label(lx));
    }
}

/// Type-check, if-convert and lower a function to LIR.
pub fn lower(func: &Func) -> Result<Lir, String> {
    check(func)?;
    let func = &if_convert(func);
    let mut lw = Lower { func, regs: vec![], home: vec![], code: vec![], nlabels: 0 };
    for k in 0..func.vars.len() {
        let r = lw.reg(kind(func.vars[k]));
        lw.home.push(r);
    }
    let params = func.params.iter().map(|&p| lw.home[p as usize]).collect();
    lw.block(&func.body);
    let r = lw.ex(&func.ret);
    lw.emit(Op::Ret(r));
    let ret = lw.regs[r as usize];
    Ok(Lir { name: func.name.clone(), regs: lw.regs, params, ret, code: lw.code, nlabels: lw.nlabels })
}

/// The number of output slots a LIR function writes (one more than the largest slot of an [`Op::Out`]; 0 for a
/// scalar function).
pub fn outputs(lir: &Lir) -> usize {
    lir.code.iter().filter_map(|o| if let Op::Out(k, _) = o { Some(*k as usize + 1) } else { None }).max().unwrap_or(0)
}
