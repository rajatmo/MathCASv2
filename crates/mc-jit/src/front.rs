//! The front end: a typed MathCAS function, as the terms `mc-syntax` lowers it to, turned into HIR.
//!
//! **The subset** (decision 1b.2): parameters typed `Int` or `Real` (`Real`, `MachineReal` and `Number` parameters
//! take machine reals); a body built from integer, machine-real and boolean values with `+`, `-`, `*`, `/`, `^`,
//! the comparisons, `and`, `or`, `not`, `if`, blocks with `let` and `var`, assignment, `while`, `for` over
//! `range(...)`, `exp` `log` `sin` `cos` `tan` `atan` `asin` `acos` `sinh` `cosh` `tanh` `sqrt` on machine reals,
//! `mod`, `quotient`, `floor`, `abs`, `min`, `max`, and calls of other functions of the subset, which are inlined.
//! Exact rationals appear only as operands of a sum or product with a machine real, or as the exponent of a machine
//! real (`x^(1/3)`), where the arithmetic folds them exactly as the interpreter does. Anything else is refused with
//! a reason.
//!
//! **Types** are inferred: each local has the type of its first value and keeps it (assigning `0` to a `Real`
//! local is refused, since the interpreter would hold the exact `0`); the branches of an `if` used as a value have
//! one type; a value whose type would depend on run-time values (`max(0, x)` with `x` real) is refused.
//!
//! **Control flow inside expressions** (`if`, `and`, `or`, blocks, inlined calls) becomes statements assigning a
//! temporary, so HIR expressions stay straight-line and both sides of `and`/`or` keep the interpreter's
//! short-circuit order.
use crate::ir::{Cc, Func, IOp, Lib, NumOp, Opd, Ty, E, FOp, S, V};
use mc_core::{Heap, Store, Sym, View, T};
use std::collections::HashMap;

/// A function the front end may compile or inline: typed parameters and the body, in which each parameter is
/// written as its symbol.
pub struct FnDef<'h> {
    /// parameter symbols and types, in order
    pub params: Vec<(Sym, Ty)>,
    /// the body
    pub body: T<'h>,
}

/// Where the front end finds functions (the evaluator's definitions, through a native's context).
pub trait Source<'h> {
    /// The definition of `f`, or why it is not a function of the subset.
    fn function(&mut self, f: Sym) -> Result<FnDef<'h>, String>;
}

/// The front end's result.
pub struct Front {
    /// the HIR
    pub func: Func,
    /// the functions read: the compiled one first, then every inlined one, each once
    pub deps: Vec<Sym>,
}

/// Most HIR-building steps for one function (inlining included).
const MAX_NODES: usize = 100_000;
/// Deepest nesting of inlined calls.
const MAX_INLINE: usize = 32;

/// Compile `f` (looked up through `src`) to HIR.
pub fn front<'h>(h: &'h Heap, f: Sym, src: &mut dyn Source<'h>) -> Result<Front, String> {
    let def = src.function(f)?;
    let mut fe = Fe {
        h,
        src,
        prims: prims(h),
        vars: vec![],
        out: vec![vec![]],
        scopes: vec![vec![]],
        base: 0,
        stack: vec![f],
        deps: vec![f],
        nodes: 0,
    };
    let mut params = vec![];
    for &(s, t) in &def.params {
        let v = fe.var(t);
        fe.bind(s, v, t, false);
        params.push(v);
    }
    let (ret, _) = fe.expr(def.body)?;
    let body = fe.out.pop().expect("the function's block");
    let func = Func { name: format!("mc_{}", f.0), vars: fe.vars, params, body, ret };
    Ok(Front { func, deps: fe.deps })
}

/// The heads with a meaning here.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Prim {
    Add,
    Mul,
    Pow,
    Cmp(Cc),
    And,
    Or,
    Not,
    If,
    Block,
    Let,
    Var,
    Assign,
    While,
    For,
    Range,
    Typed,
    Mod,
    Quotient,
    Floor,
    Abs,
    Min,
    Max,
    Sqrt,
    Math(Lib),
    True,
    False,
}

fn prims(h: &Heap) -> HashMap<Sym, Prim> {
    let mut m = HashMap::new();
    let fixed = [
        ("add", Prim::Add),
        ("mul", Prim::Mul),
        ("pow", Prim::Pow),
        ("equal", Prim::Cmp(Cc::Eq)),
        ("unequal", Prim::Cmp(Cc::Ne)),
        ("less", Prim::Cmp(Cc::Lt)),
        ("less_equal", Prim::Cmp(Cc::Le)),
        ("greater", Prim::Cmp(Cc::Gt)),
        ("greater_equal", Prim::Cmp(Cc::Ge)),
        ("and", Prim::And),
        ("or", Prim::Or),
        ("not", Prim::Not),
        ("if", Prim::If),
        ("block", Prim::Block),
        ("let", Prim::Let),
        ("var", Prim::Var),
        ("assign", Prim::Assign),
        ("while", Prim::While),
        ("for", Prim::For),
        ("range", Prim::Range),
        ("typed", Prim::Typed),
        ("mod", Prim::Mod),
        ("quotient", Prim::Quotient),
        ("floor", Prim::Floor),
        ("abs", Prim::Abs),
        ("min", Prim::Min),
        ("max", Prim::Max),
        ("sqrt", Prim::Sqrt),
        ("true", Prim::True),
        ("false", Prim::False),
    ];
    for (n, p) in fixed {
        m.insert(h.sym(n), p);
    }
    for f in Lib::ALL {
        m.insert(h.sym(f.name()), Prim::Math(f));
    }
    m
}

/// A local binding.
#[derive(Clone, Copy)]
struct Bind {
    var: V,
    ty: Ty,
    mutable: bool,
}

/// A local's declared type: `Int`; `Real` or `Number` (an integer or a machine real here); `MachineReal`.
#[derive(Clone, Copy)]
enum Decl {
    Int,
    Real,
    Machine,
}

/// An operand of a sum or product, classified.
enum Operand {
    Int(E),
    Real(E),
    /// an exact rational `num / den` (a rational literal, or `b^-k` for an integer `b`)
    Ratio(E, E),
}

type Res<X> = Result<X, String>;

struct Fe<'h, 's> {
    h: &'h Heap,
    src: &'s mut dyn Source<'h>,
    prims: HashMap<Sym, Prim>,
    vars: Vec<Ty>,
    /// the statement lists being built, innermost last
    out: Vec<Vec<S>>,
    /// lexical scopes, innermost last
    scopes: Vec<Vec<(Sym, Bind)>>,
    /// the first scope of the function being compiled (an inlined body does not see its caller's locals)
    base: usize,
    /// the functions being compiled or inlined, outermost first
    stack: Vec<Sym>,
    deps: Vec<Sym>,
    nodes: usize,
}

fn int(e: E, t: Ty) -> Res<E> {
    if t == Ty::Int {
        Ok(e)
    } else {
        Err(format!("an integer was expected, found a {} value", t.name()))
    }
}

fn b(e: E) -> Box<E> {
    Box::new(e)
}

impl<'h> Fe<'h, '_> {
    fn name(&self, s: Sym) -> String {
        self.h.sym_name(s)
    }

    fn var(&mut self, t: Ty) -> V {
        self.vars.push(t);
        (self.vars.len() - 1) as V
    }

    fn bind(&mut self, s: Sym, var: V, ty: Ty, mutable: bool) {
        self.scopes.last_mut().expect("a scope").push((s, Bind { var, ty, mutable }));
    }

    fn lookup(&self, s: Sym) -> Option<Bind> {
        self.scopes[self.base..].iter().rev().flat_map(|sc| sc.iter().rev()).find(|(x, _)| *x == s).map(|&(_, b)| b)
    }

    fn emit(&mut self, s: S) {
        self.out.last_mut().expect("a statement list").push(s);
    }

    /// Build a statement list with `f`.
    fn stmts<R>(&mut self, f: impl FnOnce(&mut Self) -> Res<R>) -> Res<(Vec<S>, R)> {
        self.out.push(vec![]);
        let r = f(self);
        let ss = self.out.pop().expect("the statement list");
        r.map(|r| (ss, r))
    }

    /// Run `f` in a new scope.
    fn scoped<R>(&mut self, f: impl FnOnce(&mut Self) -> Res<R>) -> Res<R> {
        self.scopes.push(vec![]);
        let r = f(self);
        self.scopes.pop();
        r
    }

    fn tick(&mut self) -> Res<()> {
        self.nodes += 1;
        if self.nodes > MAX_NODES {
            return Err("the function is too large (with what it inlines)".into());
        }
        Ok(())
    }

    /// A temporary holding the value of `e`.
    fn temp(&mut self, e: E, t: Ty) -> E {
        let v = self.var(t);
        self.emit(S::Set(v, e));
        E::Var(v)
    }

    /// The head and arguments of a call with a symbol head.
    fn call_of(&self, t: T<'h>) -> Option<(Sym, &'h [T<'h>])> {
        match self.h.view(t) {
            View::App { head, args } => match self.h.view(head) {
                View::Sym(s) => Some((s, args)),
                _ => None,
            },
            _ => None,
        }
    }

    fn prim_of(&self, t: T<'h>) -> Option<(Prim, &'h [T<'h>])> {
        let (s, args) = self.call_of(t)?;
        self.prims.get(&s).map(|&p| (p, args))
    }

    // --- expressions ---------------------------------------------------------------------------------------------

    fn expr(&mut self, t: T<'h>) -> Res<(E, Ty)> {
        self.tick()?;
        let h = self.h;
        match h.view(t) {
            View::Int(v) => Ok((E::Int(v), Ty::Int)),
            View::BigInt { .. } => Err("an integer literal beyond 64 bits".into()),
            View::Real(x) => Ok((E::Real(x), Ty::Real)),
            View::Rational { .. } => {
                Err("an exact rational outside a sum or product with a machine real (the result would be exact)".into())
            }
            View::Sym(s) => match self.prims.get(&s) {
                Some(Prim::True) => Ok((E::Bool(true), Ty::Bool)),
                Some(Prim::False) => Ok((E::Bool(false), Ty::Bool)),
                _ => match self.lookup(s) {
                    Some(bd) => Ok((E::Var(bd.var), bd.ty)),
                    None => Err(format!("the free name `{}`", self.name(s))),
                },
            },
            View::App { head, args } => {
                let View::Sym(s) = h.view(head) else { return Err("a call whose head is not a symbol".into()) };
                match self.prims.get(&s).copied() {
                    Some(p) => self.prim(p, s, args),
                    None => self.inline(s, args),
                }
            }
            View::Ball { .. } => Err("a ball".into()),
            View::Str(_) => Err("a string".into()),
        }
    }

    fn arity(&self, s: Sym, args: &[T<'h>], n: usize) -> Res<()> {
        if args.len() == n {
            Ok(())
        } else {
            Err(format!("`{}` with {} arguments", self.name(s), args.len()))
        }
    }

    fn prim(&mut self, p: Prim, s: Sym, args: &'h [T<'h>]) -> Res<(E, Ty)> {
        match p {
            Prim::Add => self.sum(args),
            Prim::Mul => self.product(args),
            Prim::Pow => {
                self.arity(s, args, 2)?;
                self.power(args[0], args[1])
            }
            Prim::Cmp(c) => {
                self.arity(s, args, 2)?;
                let (x, tx) = self.expr(args[0])?;
                let (y, ty) = self.expr(args[1])?;
                let (x, y) = match (tx, ty) {
                    (Ty::Bool, _) | (_, Ty::Bool) => return Err("a comparison of booleans".into()),
                    (Ty::Int, Ty::Real) => (E::ToReal(b(x)), y),
                    (Ty::Real, Ty::Int) => (x, E::ToReal(b(y))),
                    _ => (x, y),
                };
                Ok((E::Cmp(c, b(x), b(y)), Ty::Bool))
            }
            Prim::And | Prim::Or => self.junction(p == Prim::And, args),
            Prim::Not => {
                self.arity(s, args, 1)?;
                let x = self.boolean(args[0])?;
                Ok((E::Not(b(x)), Ty::Bool))
            }
            Prim::If => match args {
                [c, x, y] => self.if_value(*c, *x, *y),
                [_, _] => Err("an `if` without `else` used as a value (it can be null)".into()),
                _ => Err("a malformed `if`".into()),
            },
            Prim::Block => self.scoped(|fe| fe.block_value(args)),
            Prim::Assign => {
                self.assign(args)?;
                let View::Sym(x) = self.h.view(args[0]) else { unreachable!("checked by assign") };
                let bd = self.lookup(x).expect("checked by assign");
                Ok((E::Var(bd.var), bd.ty))
            }
            Prim::Let | Prim::Var | Prim::While | Prim::For => {
                Err(format!("`{}` used as a value (its value is null)", self.name(s)))
            }
            Prim::Mod | Prim::Quotient => {
                self.arity(s, args, 2)?;
                let (x, tx) = self.expr(args[0])?;
                let (y, ty) = self.expr(args[1])?;
                if (tx, ty) != (Ty::Int, Ty::Int) {
                    return Err(format!("`{}` of a machine real", self.name(s)));
                }
                let op = if p == Prim::Mod { IOp::Mod } else { IOp::Quot };
                Ok((E::I(op, b(x), b(y)), Ty::Int))
            }
            Prim::Floor => {
                self.arity(s, args, 1)?;
                match self.expr(args[0])? {
                    (x, Ty::Int) => Ok((x, Ty::Int)),
                    (x, Ty::Real) => Ok((E::Floor(b(x)), Ty::Int)),
                    _ => Err("`floor` of a boolean".into()),
                }
            }
            Prim::Abs => {
                self.arity(s, args, 1)?;
                match self.expr(args[0])? {
                    (x, Ty::Int) => {
                        // |x| = x < 0 ? 0 - x : x (0 - x declines only for MIN, whose absolute value is not an i64)
                        let x = self.temp(x, Ty::Int);
                        let neg = E::I(IOp::Sub, b(E::Int(0)), b(x.clone()));
                        let lt = E::Cmp(Cc::Lt, b(x.clone()), b(E::Int(0)));
                        Ok((E::Sel(b(lt), b(neg), b(x)), Ty::Int))
                    }
                    (x, Ty::Real) => Ok((E::AbsF(b(x)), Ty::Real)),
                    _ => Err("`abs` of a boolean".into()),
                }
            }
            Prim::Min | Prim::Max => self.extreme(p == Prim::Max, args),
            Prim::Sqrt => {
                self.arity(s, args, 1)?;
                // sqrt(x) is the canonical power x^(1/2)
                match self.expr(args[0])? {
                    (x, Ty::Real) => Ok((E::Num(NumOp::Pow, vec![Opd::Real(x), Opd::Ratio(E::Int(1), E::Int(2))]), Ty::Real)),
                    _ => Err("`sqrt` of an exact or boolean value (the result stays exact)".into()),
                }
            }
            Prim::Math(f) => {
                self.arity(s, args, 1)?;
                match self.expr(args[0])? {
                    (x, Ty::Real) => Ok((E::Math(f, b(x)), Ty::Real)),
                    _ => Err(format!("`{}` of an exact or boolean value (the result stays exact)", f.name())),
                }
            }
            Prim::Range | Prim::Typed => Err(format!("`{}` outside its place", self.name(s))),
            Prim::True | Prim::False => Err("a call of `true` or `false`".into()),
        }
    }

    fn boolean(&mut self, t: T<'h>) -> Res<E> {
        match self.expr(t)? {
            (x, Ty::Bool) => Ok(x),
            (_, ty) => Err(format!("a condition of type {}", ty.name())),
        }
    }

    /// An operand of a sum or product: an exact rational literal, `b^-k` (exact), or a value.
    fn operand(&mut self, t: T<'h>) -> Res<Operand> {
        let h = self.h;
        if let View::Rational { num, den } = h.view(t) {
            return match (h.view(num), h.view(den)) {
                (View::Int(n), View::Int(d)) => Ok(Operand::Ratio(E::Int(n), E::Int(d))),
                _ => Err("a rational literal beyond 64 bits".into()),
            };
        }
        if let Some((Prim::Pow, [base, e])) = self.prim_of(t) {
            if let View::Int(k @ i64::MIN..=-1) = h.view(*e) {
                let (x, tx) = self.expr(*base)?;
                if tx == Ty::Int {
                    let k = k.checked_neg().ok_or("an exponent beyond 64 bits")?;
                    // b^-1 (a division) needs no power
                    let den = if k == 1 { x } else { E::I(IOp::Pow, b(x), b(E::Int(k))) };
                    return Ok(Operand::Ratio(E::Int(1), den));
                }
                // a machine real to a negative power: a value like any other
                let (v, _) = self.power_of(x, tx, E::Int(k), Ty::Int)?;
                return Ok(Operand::Real(v));
            }
        }
        match self.expr(t)? {
            (x, Ty::Int) => Ok(Operand::Int(x)),
            (x, Ty::Real) => Ok(Operand::Real(x)),
            (_, Ty::Bool) => Err("arithmetic on a boolean".into()),
        }
    }

    /// `add(...)`: exact integers add exactly; with machine reals, the arithmetic's canonical sum (exact operands
    /// first, then the inexact ones in canonical order): one IEEE addition for two operands, otherwise an
    /// [`E::Num`] that lowering expands inline.
    fn sum(&mut self, args: &'h [T<'h>]) -> Res<(E, Ty)> {
        let ops = args.iter().map(|&a| self.operand(a)).collect::<Res<Vec<_>>>()?;
        self.fold(NumOp::Sum, ops)
    }

    /// `mul(...)`: exact integers multiply exactly (with `b^-k` factors an exact division); with machine reals, the
    /// canonical product.
    fn product(&mut self, args: &'h [T<'h>]) -> Res<(E, Ty)> {
        let ops = args.iter().map(|&a| self.operand(a)).collect::<Res<Vec<_>>>()?;
        self.fold(NumOp::Product, ops)
    }

    fn fold(&mut self, op: NumOp, ops: Vec<Operand>) -> Res<(E, Ty)> {
        let reals = ops.iter().filter(|o| matches!(o, Operand::Real(_))).count();
        let ratios = ops.iter().filter(|o| matches!(o, Operand::Ratio(..))).count();
        let (iop, fop, unit) = if op == NumOp::Sum { (IOp::Add, FOp::Add, 0) } else { (IOp::Mul, FOp::Mul, 1) };
        if reals == 0 {
            if ratios > 0 && op == NumOp::Sum {
                return Err("exact rational arithmetic (the result can be a rational)".into());
            }
            // exact: the numerators' product over the denominators' product, which must divide it
            let (mut num, mut den) = (None::<E>, None::<E>);
            let join = |acc: Option<E>, x: E| Some(acc.map_or(x.clone(), |a| E::I(iop, b(a), b(x))));
            for o in ops {
                match o {
                    Operand::Int(x) => num = join(num, x),
                    Operand::Ratio(n, d) => {
                        num = join(num, n);
                        den = join(den, d);
                    }
                    Operand::Real(_) => unreachable!("no reals"),
                }
            }
            let num = num.unwrap_or(E::Int(unit));
            return Ok((
                match den {
                    Some(d) => E::I(IOp::DivExact, b(num), b(d)),
                    None => num,
                },
                Ty::Int,
            ));
        }
        // one IEEE operation when there are two operands and no rational: exact integers convert exactly up to
        // 2^53 (beyond, the conversion declines) and the operation commutes
        if ops.len() == 2 && ratios == 0 {
            let mut it = ops.into_iter().map(|o| match o {
                Operand::Int(x) => E::ToReal(b(x)),
                Operand::Real(x) => x,
                Operand::Ratio(..) => unreachable!("no ratios"),
            });
            let (x, y) = (it.next().expect("two"), it.next().expect("two"));
            return Ok((E::F(fop, b(x), b(y)), Ty::Real));
        }
        if ops.len() == 1 {
            if let Some(Operand::Real(x)) = ops.into_iter().next() {
                return Ok((x, Ty::Real));
            }
            unreachable!("one real operand");
        }
        let opds = ops
            .into_iter()
            .map(|o| match o {
                Operand::Int(x) => Opd::Int(x),
                Operand::Real(x) => Opd::Real(x),
                Operand::Ratio(n, d) => Opd::Ratio(n, d),
            })
            .collect();
        Ok((E::Num(op, opds), Ty::Real))
    }

    /// `pow(a, e)`.
    fn power(&mut self, base: T<'h>, e: T<'h>) -> Res<(E, Ty)> {
        let h = self.h;
        let (x, tx) = self.expr(base)?;
        if let View::Rational { num, den } = h.view(e) {
            let (View::Int(n), View::Int(d)) = (h.view(num), h.view(den)) else {
                return Err("a rational exponent beyond 64 bits".into());
            };
            return match tx {
                Ty::Real => Ok((E::Num(NumOp::Pow, vec![Opd::Real(x), Opd::Ratio(E::Int(n), E::Int(d))]), Ty::Real)),
                _ => Err("an exact power with a rational exponent (the result stays exact)".into()),
            };
        }
        let (y, ty) = self.expr(e)?;
        self.power_of(x, tx, y, ty)
    }

    fn power_of(&mut self, x: E, tx: Ty, y: E, ty: Ty) -> Res<(E, Ty)> {
        match (tx, ty) {
            (Ty::Int, Ty::Int) => match y {
                // an exact negative power: 1 / x^k, when that is an integer
                E::Int(k) if k < 0 => {
                    let k = k.checked_neg().ok_or("an exponent beyond 64 bits")?;
                    Ok((E::I(IOp::DivExact, b(E::Int(1)), b(E::I(IOp::Pow, b(x), b(E::Int(k))))), Ty::Int))
                }
                y => Ok((E::I(IOp::Pow, b(x), b(y)), Ty::Int)),
            },
            (Ty::Bool, _) | (_, Ty::Bool) => Err("a power of booleans".into()),
            _ => {
                let o = |v: E, t: Ty| if t == Ty::Int { Opd::Int(v) } else { Opd::Real(v) };
                Ok((E::Num(NumOp::Pow, vec![o(x, tx), o(y, ty)]), Ty::Real))
            }
        }
    }

    /// `and(...)` / `or(...)`: left to right, stopping at the first deciding value.
    fn junction(&mut self, is_and: bool, args: &'h [T<'h>]) -> Res<(E, Ty)> {
        let Some((first, rest)) = args.split_first() else { return Ok((E::Bool(is_and), Ty::Bool)) };
        let x = self.boolean(*first)?;
        let t = self.var(Ty::Bool);
        self.emit(S::Set(t, x));
        let mut target = vec![];
        // nested: and(a, b, c) is t = a; if t { t = b; if t { t = c } }
        let mut chain: Vec<Vec<S>> = vec![];
        for &a in rest {
            let (ss, x) = self.stmts(|fe| fe.boolean(a))?;
            let mut ss = ss;
            ss.push(S::Set(t, x));
            chain.push(ss);
        }
        while let Some(mut inner) = chain.pop() {
            if !target.is_empty() {
                let c = if is_and { E::Var(t) } else { E::Not(b(E::Var(t))) };
                inner.push(S::If(c, std::mem::take(&mut target), vec![]));
            }
            target = inner;
        }
        if !target.is_empty() {
            let c = if is_and { E::Var(t) } else { E::Not(b(E::Var(t))) };
            self.emit(S::If(c, target, vec![]));
        }
        Ok((E::Var(t), Ty::Bool))
    }

    /// `if c then a else b` as a value.
    fn if_value(&mut self, c: T<'h>, x: T<'h>, y: T<'h>) -> Res<(E, Ty)> {
        let c = self.boolean(c)?;
        let (sa, (ea, ta)) = self.stmts(|fe| fe.expr(x))?;
        let (sb, (eb, tb)) = self.stmts(|fe| fe.expr(y))?;
        if ta != tb {
            return Err(format!("`if` branches of types {} and {}", ta.name(), tb.name()));
        }
        let t = self.var(ta);
        let (mut sa, mut sb) = (sa, sb);
        sa.push(S::Set(t, ea));
        sb.push(S::Set(t, eb));
        self.emit(S::If(c, sa, sb));
        Ok((E::Var(t), ta))
    }

    /// `min(...)` / `max(...)`: the first extreme value; all operands of one type.
    fn extreme(&mut self, max: bool, args: &'h [T<'h>]) -> Res<(E, Ty)> {
        let Some((first, rest)) = args.split_first() else { return Err("`min` or `max` without arguments".into()) };
        let (mut acc, t) = self.expr(*first)?;
        if t == Ty::Bool {
            return Err("`min` or `max` of booleans".into());
        }
        for &a in rest {
            let (x, tx) = self.expr(a)?;
            if tx != t {
                return Err("`min` or `max` of an integer and a machine real (the result's type would depend on the values)".into());
            }
            acc = if t == Ty::Int {
                let (best, x) = (self.temp(acc, t), self.temp(x, t));
                let better = E::Cmp(if max { Cc::Gt } else { Cc::Lt }, b(x.clone()), b(best.clone()));
                E::Sel(b(better), b(x), b(best))
            } else {
                E::MinMaxF(max, b(acc), b(x))
            };
        }
        Ok((acc, t))
    }

    /// The statements of a block; the last one's value is the block's.
    fn block_value(&mut self, items: &'h [T<'h>]) -> Res<(E, Ty)> {
        let Some((last, init)) = items.split_last() else { return Err("an empty block used as a value (null)".into()) };
        for &s in init {
            self.stmt(s)?;
        }
        if let Some((Prim::Let | Prim::Var, _)) = self.prim_of(*last) {
            return Err("a block ending in a binding used as a value (null)".into());
        }
        self.expr(*last)
    }

    /// An inlined call of another function of the subset.
    fn inline(&mut self, f: Sym, args: &'h [T<'h>]) -> Res<(E, Ty)> {
        let name = self.name(f);
        if self.stack.contains(&f) {
            return Err(format!("the recursive call of `{name}`"));
        }
        if self.stack.len() >= MAX_INLINE {
            return Err("calls nested too deeply to inline".into());
        }
        let def = self.src.function(f).map_err(|e| format!("the call of `{name}`: {e}"))?;
        if def.params.len() != args.len() {
            return Err(format!("`{name}` called with {} arguments", args.len()));
        }
        let mut vals = vec![];
        for (&a, &(_, pt)) in args.iter().zip(&def.params) {
            let (x, tx) = self.expr(a)?;
            if tx != pt {
                return Err(format!("`{name}` called with a {} value for a {} parameter", tx.name(), pt.name()));
            }
            let v = self.var(pt);
            self.emit(S::Set(v, x));
            vals.push(v);
        }
        if !self.deps.contains(&f) {
            self.deps.push(f);
        }
        // the callee's body sees only its parameters
        let saved = self.base;
        self.scopes.push(vec![]);
        self.base = self.scopes.len() - 1;
        for (&(s, t), v) in def.params.iter().zip(vals) {
            self.bind(s, v, t, false);
        }
        self.stack.push(f);
        let r = self.expr(def.body);
        self.stack.pop();
        self.scopes.pop();
        self.base = saved;
        r
    }

    // --- statements ----------------------------------------------------------------------------------------------

    fn stmt(&mut self, t: T<'h>) -> Res<()> {
        self.tick()?;
        let Some((p, args)) = self.prim_of(t) else {
            let (x, _) = self.expr(t)?;
            self.emit(S::Eval(x));
            return Ok(());
        };
        match (p, args) {
            (Prim::Let | Prim::Var, [lhs, v]) => {
                let (x, declared) = self.binder(*lhs)?;
                let (e, ty) = self.expr(*v)?;
                // the interpreter checks the declared type: a mismatch is a failure, so it is not compiled
                let fits = match declared {
                    None => true,
                    Some(Decl::Int) => ty == Ty::Int,
                    Some(Decl::Real) => ty != Ty::Bool,
                    Some(Decl::Machine) => ty == Ty::Real,
                };
                if !fits {
                    return Err(format!("`{}` declared with a type its {} value does not have", self.name(x), ty.name()));
                }
                let var = self.var(ty);
                self.emit(S::Set(var, e));
                self.bind(x, var, ty, p == Prim::Var);
                Ok(())
            }
            (Prim::Assign, _) => self.assign(args),
            (Prim::While, [c, body]) => self.while_loop(*c, *body),
            (Prim::For, [x, range, body]) => self.for_loop(*x, *range, *body),
            (Prim::If, [c, x, rest @ ..]) if rest.len() <= 1 => {
                let c = self.boolean(*c)?;
                let (sa, ()) = self.stmts(|fe| fe.scoped(|fe| fe.stmt(*x)))?;
                let (sb, ()) = match rest {
                    [y] => self.stmts(|fe| fe.scoped(|fe| fe.stmt(*y)))?,
                    _ => (vec![], ()),
                };
                self.emit(S::If(c, sa, sb));
                Ok(())
            }
            (Prim::Block, items) => self.scoped(|fe| items.iter().try_for_each(|&s| fe.stmt(s))),
            _ => {
                let (x, _) = self.expr(t)?;
                self.emit(S::Eval(x));
                Ok(())
            }
        }
    }

    /// The name a `let` or `var` binds, and its declared type.
    fn binder(&self, lhs: T<'h>) -> Res<(Sym, Option<Decl>)> {
        match self.h.view(lhs) {
            View::Sym(x) => Ok((x, None)),
            _ => match self.prim_of(lhs) {
                Some((Prim::Typed, [x, ty])) => {
                    let View::Sym(x) = self.h.view(*x) else { return Err("a malformed binding".into()) };
                    let View::Sym(ty) = self.h.view(*ty) else { return Err("a local of a compound type".into()) };
                    let d = match self.name(ty).as_str() {
                        "Int" | "integer" | "Integer" => Decl::Int,
                        "real" | "Real" | "number" | "Number" => Decl::Real,
                        "machine_real" | "MachineReal" => Decl::Machine,
                        other => return Err(format!("a local of type {other}")),
                    };
                    Ok((x, Some(d)))
                }
                _ => Err("a malformed binding".into()),
            },
        }
    }

    fn assign(&mut self, args: &'h [T<'h>]) -> Res<()> {
        let [x, v] = args else { return Err("a malformed assignment".into()) };
        let View::Sym(x) = self.h.view(*x) else { return Err("an assignment to a compound target".into()) };
        let Some(bd) = self.lookup(x) else { return Err(format!("an assignment to the free name `{}`", self.name(x))) };
        if !bd.mutable {
            return Err(format!("an assignment to `{}`, which is not a `var`", self.name(x)));
        }
        let (e, ty) = self.expr(*v)?;
        if ty != bd.ty {
            return Err(format!("`{}` holds a {} value and is assigned a {} value", self.name(x), bd.ty.name(), ty.name()));
        }
        self.emit(S::Set(bd.var, e));
        Ok(())
    }

    fn while_loop(&mut self, c: T<'h>, body: T<'h>) -> Res<()> {
        let (cs, ce) = self.stmts(|fe| fe.boolean(c))?;
        let (mut bs, ()) = self.stmts(|fe| fe.scoped(|fe| fe.stmt(body)))?;
        if cs.is_empty() {
            self.emit(S::While(ce, bs));
            return Ok(());
        }
        // a condition with statements of its own: evaluated before the loop and again after each iteration
        let t = self.var(Ty::Bool);
        for s in cs {
            self.emit(s);
        }
        self.emit(S::Set(t, ce));
        let (cs2, ce2) = self.stmts(|fe| fe.boolean(c))?;
        bs.extend(cs2);
        bs.push(S::Set(t, ce2));
        self.emit(S::While(E::Var(t), bs));
        Ok(())
    }

    fn for_loop(&mut self, x: T<'h>, range: T<'h>, body: T<'h>) -> Res<()> {
        let View::Sym(x) = self.h.view(x) else { return Err("a `for` over a pattern".into()) };
        let Some((Prim::Range, bounds)) = self.prim_of(range) else {
            return Err("a `for` loop over something other than `range(...)`".into());
        };
        let mut v = vec![];
        for &a in bounds {
            let (e, t) = self.expr(a)?;
            v.push(int(e, t).map_err(|_| "a `range` with a bound that is not an integer".to_string())?);
        }
        let (lo, hi, step) = match v.len() {
            1 => (E::Int(1), v.pop().expect("one"), E::Int(1)),
            2 => {
                let hi = v.pop().expect("two");
                (v.pop().expect("two"), hi, E::Int(1))
            }
            3 => {
                let step = v.pop().expect("three");
                let hi = v.pop().expect("three");
                (v.pop().expect("three"), hi, step)
            }
            _ => return Err("a `range` with more than three arguments".into()),
        };
        // the bounds are evaluated once, in order, before the loop
        let (lo, hi, step) = (self.temp(lo, Ty::Int), self.temp(hi, Ty::Int), self.temp(step, Ty::Int));
        let var = self.var(Ty::Int);
        let (body, ()) = self.stmts(|fe| {
            fe.scoped(|fe| {
                fe.bind(x, var, Ty::Int, false);
                fe.stmt(body)
            })
        })?;
        self.emit(S::For { var, lo, hi, step, body });
        Ok(())
    }
}
