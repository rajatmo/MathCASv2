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
//! Operations whose machine-real semantics are not a single IEEE operation (powers, sums and products of more than
//! two operands or with exact rational parts) are calls into the arithmetic's own code ([`crate::rt::num`]), so
//! that they agree with the interpreter by construction.

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
                    S::Set(_, x) | S::Eval(x) => e(x),
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
            E::Num(op, opds) => {
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
                let d = self.reg(K::F);
                self.emit(Op::Num(*op, d, rs));
                d
            }
            E::Sel(c, a, b) => {
                let t = type_of(self.func, a).expect("checked");
                let (rc, ra, rb) = (self.ex(c), self.ex(a), self.ex(b));
                let d = self.reg(kind(t));
                self.emit(Op::Sel(d, rc, ra, rb));
                d
            }
        }
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
