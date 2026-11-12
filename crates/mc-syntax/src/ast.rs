//! The syntax tree produced by the parser. Every node carries its source span. The tree stays close to the text:
//! operator chains are kept as written (`a + b - c` is one [`ExprKind::Add`] with signs), and the decisions about
//! what core terms they mean are taken in lowering (`lower.rs`).
use crate::lexer::{NumLit, PatKind};
use crate::span::Span;

#[derive(Clone, Debug)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

/// A factor's operator in a product chain.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MulOp {
    /// The first factor, `*`, or juxtaposition.
    Times,
    /// `/`
    Div,
}

/// Binary operators other than the chains.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BinOp {
    Set,
    SetDelayed,
    Pipe,
    /// `e /. rules`: `rewrite(e, rules)`
    Replace,
    Rule,
    RuleDelayed,
    When,
    Equal,
    Unequal,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    Same,
    NotSame,
}

/// Operators that take any number of operands (`a or b or c`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NaryOp {
    Or,
    And,
    Alternatives,
}

#[derive(Clone, Debug)]
pub enum ExprKind {
    Num(NumLit),
    Str(String),
    /// A name, including `true`, `false`, `null` and backquoted names.
    Name(String),
    Pattern { name: Option<String>, kind: PatKind },
    /// `&k` inside a slot function.
    Slot(u32),
    /// `p: T` for a pattern `p`.
    Typed(Box<Expr>, Box<Type>),
    /// `x @ p`
    Named(String, Box<Expr>),
    /// Prefix `-`.
    Neg(Box<Expr>),
    Not(Box<Expr>),
    /// `a + b - c`: operands with `true` for subtraction; the first is never subtracted.
    Add(Vec<(bool, Expr)>),
    /// `a * b / c`, `2x`: the first factor has [`MulOp::Times`].
    Mul(Vec<(MulOp, Expr)>),
    Pow(Box<Expr>, Box<Expr>),
    Binary(BinOp, Box<Expr>, Box<Expr>),
    Nary(NaryOp, Vec<Expr>),
    Call(Box<Expr>, Vec<Arg>),
    /// `a[i, j]`; an index may be [`ExprKind::Span`].
    Index(Box<Expr>, Vec<Expr>),
    /// `i:j`, only as an index.
    Span(Box<Expr>, Box<Expr>),
    /// `f'`: the name and the number of marks.
    Prime(Box<Expr>, u32),
    List(Vec<Expr>),
    Block(Vec<Stmt>),
    Lambda(Vec<Param>, Box<Expr>),
    /// `&(body)`
    SlotFn(Box<Expr>),
    If(Box<Expr>, Box<Expr>, Option<Box<Expr>>),
    While(Box<Expr>, Box<Expr>),
    For(String, Box<Expr>, Box<Expr>),
    Match(Box<Expr>, Vec<Arm>),
}

/// A call argument; `name` for a named argument `name: value`.
#[derive(Clone, Debug)]
pub struct Arg {
    pub name: Option<(String, Span)>,
    pub value: Expr,
}

#[derive(Clone, Debug)]
pub struct Arm {
    pub pattern: Expr,
    pub body: Expr,
}

/// A parameter of a `fn` definition or a lambda (04 sections 6.2, 6.5).
#[derive(Clone, Debug)]
pub struct Param {
    pub held: bool,
    pub kind: ParamKind,
    pub default: Option<Expr>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum ParamKind {
    /// `x` or `x: T`
    Name(String, Option<Type>),
    /// `rest...`
    Rest(String),
    /// A pattern or literal (only in `fn` definitions).
    Pattern(Expr),
}

/// A type expression (04 section 9).
#[derive(Clone, Debug)]
pub struct Type {
    pub kind: TypeKind,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum TypeKind {
    Name(String),
    /// A non-negative integer argument, as in `Array[Real, 2]`.
    Num(NumLit),
    /// `Name[T, ...]`
    App(String, Vec<Type>),
    /// `(T, ...) -> T`
    Fun(Vec<Type>, Box<Type>),
    Union(Vec<Type>),
    Where(Box<Type>, Box<Expr>),
}

#[derive(Clone, Debug)]
pub struct Decorator {
    pub name: String,
    pub args: Option<Vec<Arg>>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct Stmt {
    pub kind: StmtKind,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum StmtKind {
    Expr(Expr),
    /// `let x: T = e` (`mutable` for `var`).
    Let { mutable: bool, name: String, ty: Option<Type>, value: Expr },
    Fn {
        decorators: Vec<Decorator>,
        name: String,
        params: Vec<Param>,
        options: Vec<Param>,
        result: Option<Type>,
        condition: Option<Expr>,
        body: Expr,
    },
    Rule { decorators: Vec<Decorator>, lhs: Expr, delayed: bool, rhs: Expr },
    Sym { decorators: Vec<Decorator>, name: String },
    Module { name: String, body: Vec<Stmt> },
    Import { path: String, kind: ImportKind },
    Pub(Box<Stmt>),
}

#[derive(Clone, Debug)]
pub enum ImportKind {
    All,
    Only(Vec<String>),
    Except(Vec<String>),
}

/// A parsed source text: its statements.
#[derive(Clone, Debug, Default)]
pub struct Program {
    pub stmts: Vec<Stmt>,
}
