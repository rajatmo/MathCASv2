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
//! **Lists** (package N0, decision N0.3): list literals `[a, b, ...]` (nested too), vector parameters of
//! [`front_expr`], `let` bindings of lists, constant indices `y[k]` (negative ones from the end), `length(y)` and
//! `if` with lists of one shape on both sides are values of known shape; a list result is written to the output
//! buffer, its leaves in row-major order. Arithmetic on lists is refused (the interpreter leaves `y + 1.0`
//! unevaluated), as is an index that is not a constant integer. **Constants** (decision N0.1): `n(pi)`, `n(e)`,
//! `n(x)` of a machine real or an integer compile everywhere; bare `pi` and `e` only in the body of a numeric
//! [`front_expr`] (whose callers gave it to `n`), since the interpreter keeps them exact.
//!
//! **Vectors** (package P): an untyped parameter of a compiled function is a vector of run-time length whose leaves
//! are all `Int` or all `Real` ([`front_vectors`] compiles one variant per leaf type, decision P.3); it is two `Int`
//! words, the address of its leaves and their number, which only the operations below see: `v[i]` with any integer
//! index ([`E::Elem`], declining outside the vector as `part` fails), `length(v)`, `sum(v)` and `total(v)`
//! ([`E::VSum`]), `for x in v`, passing `v` to an inlined function's untyped parameter, `let`, and `if` with vectors
//! on both sides. `table(e, n)`, `table(e, k, n)` and `table(e, k, v)` of machine numbers make a new vector
//! ([`S::VNew`], [`S::Store`]); a vector result is the call's vector result ([`S::VOut`]). Arithmetic on vectors and
//! vectors inside lists are refused.
//!
//! **Types** are inferred: each local has the type of its first value and keeps it (assigning `0` to a `Real`
//! local is refused, since the interpreter would hold the exact `0`); the branches of an `if` used as a value have
//! one type; a value whose type would depend on run-time values (`max(0, x)` with `x` real) is refused.
//!
//! **Control flow inside expressions** (`if`, `and`, `or`, blocks, inlined calls) becomes statements assigning a
//! temporary, so HIR expressions stay straight-line and both sides of `and`/`or` keep the interpreter's
//! short-circuit order.
//!
//! **Random draws** (package N4b, decision N4b.1): `random_real()`, `random_real(b)`, `random_real([a, b])`,
//! `random_normal()`, `random_normal(mu, sigma)`, `random_integer(n)` and `random_integer([a, b])` compile, when
//! the source allows draws ([`Source::draws`]), to [`E::Draw`]: each draw is a statement of its own, emitted where the
//! front end meets it, which is the interpreter's evaluation order (arguments first, left to right, only the branch
//! taken), so compiled code consumes the stream exactly as the interpreter does.
//!
//! **Complex numbers** (package N4b, decision N4b.3): a machine complex value is a pair of machine-real HIR
//! expressions (its parts). `complex(x, y)`, sums and products with complex operands, integer powers, `abs`, `re`,
//! `im_part` and `conjugate` compute inline exactly the steps of `mc_arith::Num` (sums and products of two inexact
//! operands, or of an exact part and one inexact operand; integer powers by its binary powering; reciprocals); the
//! rest (three or more inexact operands, other powers, `sqrt`, the elementary functions) is a statement
//! [`S::Pair`] that calls the arithmetic or `mc-numeric` themselves. Exact complex numbers are compiled only as
//! constant operands of a sum or product (`y*im`).
//!
//! **Library calls and more** (round 13, decisions LC.1 to LC.9): special functions of machine reals (`bessel_k`,
//! `gamma`, `erf`, ...), local interpolations and derivatives of interpolations, and solutions of `n_dsolve` are
//! [`E::Call`]s of [`crate::callee`]; `piecewise`, `unit_step`, `heaviside_theta` and `sign` are branches and
//! selections giving the interpreter's exact values; pure subterms are computed once per binding environment (common
//! subexpressions); an inlined function's untyped parameter takes a number, or data (an association, an
//! interpolation) put in its body and folded (`r["f"](x)`); global numeric constants are read (the read recorded);
//! closed numeric operands (`pi^-3`) beside a machine real in a sum or product are their machine values, as the
//! evaluator's contagion for constants makes them.
use crate::ir::{BOp, Cc, Draw, Func, IOp, Lib, NumOp, Opd, Pair, Ty, E, FOp, S, V};
use mc_arith::num::{Int, Rat};
use mc_arith::{Kind, Num, Real};
use mc_core::{Heap, Store, Sym, View, T};
use std::collections::HashMap;

/// A function the front end may compile or inline: typed parameters and the body, in which each parameter is
/// written as its symbol.
pub struct FnDef<'h> {
    /// parameter symbols and kinds, in order
    pub params: Vec<(Sym, Param)>,
    /// the body
    pub body: T<'h>,
}

/// A parameter of a compiled or inlined function.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Param {
    /// a machine integer or real (a parameter typed `Int`, or `Real`, `MachineReal`, `Number`)
    Scalar(Ty),
    /// a vector of machine numbers of any length (package P, decision P.2: an untyped parameter), read through its
    /// payload; the leaves' type is the compiled variant's ([`front_vectors`])
    Vector,
}

/// How a compiled function takes one argument (for the host and `compiled_info`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Input {
    /// one word of this type
    Scalar(Ty),
    /// this many machine reals, one word each ([`Arg::Vector`])
    Words(usize),
    /// a vector of leaves of this type (`Int` or `Real`) as two words: the address of its leaves and their number
    /// (package P)
    Vector(Ty),
}

impl Input {
    /// The type as the language writes it (`Int`, `Real`, `List[Real]`).
    pub fn name(self) -> String {
        match self {
            Input::Scalar(t) => t.name().to_string(),
            Input::Words(n) => format!("[Real; {n}]"),
            Input::Vector(t) => format!("List[{}]", t.name()),
        }
    }
}

/// Where the front end finds functions (the evaluator's definitions, through a native's context).
pub trait Source<'h> {
    /// The definition of `f`, or why it is not a function of the subset.
    fn function(&mut self, f: Sym) -> Result<FnDef<'h>, String>;

    /// Whether the code may draw from the session generator (a compiled function whose host attaches the stream);
    /// otherwise random draws are refused.
    fn draws(&self) -> bool {
        false
    }

    /// The value of the global `s` when the code may use it as a constant (an interpolation bound to a name), the
    /// read recorded so that redefining `s` invalidates the code; `None` otherwise.
    fn constant(&mut self, _s: Sym) -> Option<T<'h>> {
        None
    }
}

/// The front end's result.
pub struct Front {
    /// the HIR
    pub func: Func,
    /// the functions read: for [`front`] the compiled one first, then every inlined one, each once; for
    /// [`front_expr`] the inlined ones
    pub deps: Vec<Sym>,
    /// the shape of the result: a scalar is the function's return value; a list is written to the output buffer,
    /// its leaves in row-major order (decision N0.3); a vector is the environment's vector result (package P)
    pub result: Shape,
    /// how the function takes its arguments, in order
    pub inputs: Vec<Input>,
}

/// The shape of a value: a scalar of a machine type, or a list of values (nested lists allowed).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Shape {
    /// one machine value
    Scalar(Ty),
    /// a list literal of these shapes
    List(Vec<Shape>),
    /// a machine complex number: two `Real` leaves, the real and the imaginary part (package N4b)
    Complex,
    /// a vector of machine numbers whose length is known only at run time (package P), held as two `Int` words
    /// (the address of its leaves and their number); only a function's result or a local, never inside a list
    Vector(Ty),
}

impl Shape {
    /// The number of scalar leaves (the two words of a vector).
    pub fn leaves(&self) -> usize {
        match self {
            Shape::Scalar(_) => 1,
            Shape::List(items) => items.iter().map(Shape::leaves).sum(),
            Shape::Complex | Shape::Vector(_) => 2,
        }
    }

    /// The types of the leaves in row-major order.
    pub fn leaf_types(&self) -> Vec<Ty> {
        fn go(s: &Shape, out: &mut Vec<Ty>) {
            match s {
                Shape::Scalar(t) => out.push(*t),
                Shape::List(items) => items.iter().for_each(|i| go(i, out)),
                Shape::Complex => out.extend([Ty::Real, Ty::Real]),
                Shape::Vector(_) => out.extend([Ty::Int, Ty::Int]),
            }
        }
        let mut out = vec![];
        go(self, &mut out);
        out
    }

    /// Whether a complex number occurs in the shape.
    pub fn has_complex(&self) -> bool {
        match self {
            Shape::Scalar(_) | Shape::Vector(_) => false,
            Shape::List(items) => items.iter().any(Shape::has_complex),
            Shape::Complex => true,
        }
    }

    /// The shape as the language writes types (`Real`, `[Real, Int]`, `Complex`, `List[Real]`).
    pub fn name(&self) -> String {
        match self {
            Shape::Scalar(t) => t.name().to_string(),
            Shape::List(items) => format!("[{}]", items.iter().map(Shape::name).collect::<Vec<_>>().join(", ")),
            Shape::Complex => "Complex".to_string(),
            Shape::Vector(t) => format!("List[{}]", t.name()),
        }
    }
}

/// A parameter of an expression compiled by [`front_expr`] (decision N0.4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Arg {
    /// an exact integer (one `Int` word)
    Int,
    /// a machine real (one `Real` word)
    Real,
    /// a list of this many machine reals (as many consecutive `Real` words); the body reads it by constant
    /// indices `y[k]` and `length(y)`
    Vector(usize),
}

impl Arg {
    /// The number of ABI words of the parameter.
    pub fn words(self) -> usize {
        match self {
            Arg::Int | Arg::Real => 1,
            Arg::Vector(n) => n,
        }
    }
}

/// Most HIR-building steps for one function (inlining included).
const MAX_NODES: usize = 100_000;
/// Deepest nesting of inlined calls.
const MAX_INLINE: usize = 32;

/// Compile `f` (looked up through `src`) to HIR. The constants `pi` and `e` are compiled only inside `n(...)`,
/// where the interpreter gives a machine real too (decision N0.1).
pub fn front<'h>(h: &'h Heap, f: Sym, src: &mut dyn Source<'h>) -> Result<Front, String> {
    front_vectors(h, f, src, Ty::Real)
}

/// [`front`] with `elem` (`Int` or `Real`) as the leaves' type of every vector parameter (decision P.3: a function
/// with vector parameters compiles one variant per leaf type).
pub fn front_vectors<'h>(h: &'h Heap, f: Sym, src: &mut dyn Source<'h>, elem: Ty) -> Result<Front, String> {
    let def = src.function(f)?;
    let params: Vec<(Sym, In)> = def
        .params
        .iter()
        .map(|&(s, p)| {
            (s, match p {
                Param::Scalar(Ty::Int) => In::Arg(Arg::Int),
                Param::Scalar(_) => In::Arg(Arg::Real),
                Param::Vector => In::Vector(elem),
            })
        })
        .collect();
    run(h, format!("mc_{}", f.0), Some(f), &params, def.body, src, false)
}

/// A parameter as [`run`] takes it.
#[derive(Clone, Copy)]
enum In {
    Arg(Arg),
    /// a vector of leaves of this type: two `Int` words, address and length
    Vector(Ty),
}

/// Compile the expression `body` in the parameters `params` to HIR (decision N0.4): the function
/// `(p1, ..., pn) => body`. With `numeric`, the constants `pi` and `e` in `body` itself (not in the functions it
/// inlines) are their machine reals, as in `n(body)` (decision N0.1): for callers that evaluate the expression
/// numerically and passed it through `n` first.
pub fn front_expr<'h>(
    h: &'h Heap,
    params: &[(Sym, Arg)],
    body: T<'h>,
    src: &mut dyn Source<'h>,
    numeric: bool,
) -> Result<Front, String> {
    let distinct: std::collections::HashSet<Sym> = params.iter().map(|p| p.0).collect();
    if distinct.len() != params.len() {
        return Err("a parameter named twice".into());
    }
    let params: Vec<(Sym, In)> = params.iter().map(|&(s, a)| (s, In::Arg(a))).collect();
    let fr = run(h, "mc_expr".into(), None, &params, body, src, numeric)?;
    if matches!(fr.result, Shape::Vector(_)) {
        return Err("a vector result of a length known only at run time".into());
    }
    Ok(fr)
}

fn run<'h>(
    h: &'h Heap,
    name: String,
    f: Option<Sym>,
    params: &[(Sym, In)],
    body: T<'h>,
    src: &mut dyn Source<'h>,
    numeric: bool,
) -> Result<Front, String> {
    let stack: Vec<Sym> = f.into_iter().collect();
    let mut fe = Fe {
        h,
        src,
        prims: prims(h),
        vars: vec![],
        out: vec![vec![]],
        scopes: vec![vec![]],
        base: 0,
        top: stack.len(),
        deps: stack.clone(),
        stack,
        nodes: 0,
        numeric,
        consts: [(h.sym("pi"), std::f64::consts::PI), (h.sym("e"), std::f64::consts::E)],
        im: h.sym("im"),
        carried: 0,
        memo: vec![HashMap::new()],
        epoch: 0,
        epochs: 0,
    };
    let mut words = vec![];
    let mut inputs = vec![];
    for &(s, a) in params {
        match a {
            In::Arg(Arg::Int | Arg::Real) => {
                let t = if matches!(a, In::Arg(Arg::Int)) { Ty::Int } else { Ty::Real };
                let v = fe.var(t);
                fe.bind(s, v, t, false);
                words.push(v);
                inputs.push(Input::Scalar(t));
            }
            In::Arg(Arg::Vector(n)) => {
                let mut items = vec![];
                for _ in 0..n {
                    let v = fe.var(Ty::Real);
                    words.push(v);
                    items.push(Val::S(E::Var(v), Ty::Real));
                }
                fe.bind_list(s, items);
                inputs.push(Input::Words(n));
            }
            In::Vector(t) => {
                let (base, len) = (fe.var(Ty::Int), fe.var(Ty::Int));
                words.extend([base, len]);
                fe.bind_vector(s, E::Var(base), E::Var(len), t);
                inputs.push(Input::Vector(t));
            }
        }
    }
    let value = fe.value(body)?;
    let result = value.shape();
    let ret = match value {
        Val::S(e, _) => e,
        Val::V(base, len, _) => {
            fe.emit(S::VOut(base, len));
            E::Int(0)
        }
        list => {
            for (k, (e, t)) in list.leaves().into_iter().enumerate() {
                if t == Ty::Bool {
                    return Err("a boolean in a list result".into());
                }
                fe.emit(S::Out(k as u32, e));
            }
            E::Int(0)
        }
    };
    // invariant: `stmts` pushed the function's block and is the only popper
    let body = fe.out.pop().expect("the function's block");
    let func = Func { name, vars: fe.vars, params: words, body, ret };
    Ok(Front { func, deps: fe.deps, result, inputs })
}

/// A value during compilation: a scalar expression, a list of values whose length is known (decision N0.3), a
/// machine complex number as its two parts (decision N4b.3), or a vector whose length is known at run time only
/// (package P): the address of its leaves, their number, their type.
#[derive(Clone, Debug)]
enum Val {
    S(E, Ty),
    L(Vec<Val>),
    C(E, E),
    V(E, E, Ty),
}

/// Why a vector cannot stand where a number is expected.
const VECTOR_AS_NUMBER: &str =
    "a vector where a number is expected (an untyped parameter is a vector of machine numbers; arithmetic on vectors is not compiled)";

impl Val {
    fn shape(&self) -> Shape {
        match self {
            Val::S(_, t) => Shape::Scalar(*t),
            Val::L(items) => Shape::List(items.iter().map(Val::shape).collect()),
            Val::C(..) => Shape::Complex,
            Val::V(_, _, t) => Shape::Vector(*t),
        }
    }

    /// The scalar leaves in row-major order (a complex number gives its real, then its imaginary part; a vector its
    /// address, then its length).
    fn leaves(self) -> Vec<(E, Ty)> {
        fn go(v: Val, out: &mut Vec<(E, Ty)>) {
            match v {
                Val::S(e, t) => out.push((e, t)),
                Val::L(items) => items.into_iter().for_each(|i| go(i, out)),
                Val::C(a, b) => out.extend([(a, Ty::Real), (b, Ty::Real)]),
                Val::V(a, n, _) => out.extend([(a, Ty::Int), (n, Ty::Int)]),
            }
        }
        let mut out = vec![];
        go(self, &mut out);
        out
    }

    /// The value of shape `s` whose leaves are taken from `leaves` in row-major order.
    fn rebuild(s: &Shape, leaves: &mut impl Iterator<Item = E>) -> Val {
        match s {
            // invariant: `rebuild` gets exactly one leaf per scalar of the shape it was flattened from
            Shape::Scalar(t) => Val::S(leaves.next().expect("a leaf per scalar"), *t),
            Shape::List(items) => Val::L(items.iter().map(|i| Val::rebuild(i, leaves)).collect()),
            Shape::Complex => {
                // invariant: `rebuild` gets two leaves per complex scalar
                let re = leaves.next().expect("a real part");
                // invariant: `rebuild` gets two leaves per complex scalar
                Val::C(re, leaves.next().expect("an imaginary part"))
            }
            Shape::Vector(t) => {
                // invariant: `rebuild` gets two leaves per vector
                let base = leaves.next().expect("an address");
                // invariant: `rebuild` gets two leaves per vector
                Val::V(base, leaves.next().expect("a length"), *t)
            }
        }
    }
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
    List,
    Part,
    Length,
    N,
    Complex,
    Re,
    ImPart,
    Conjugate,
    Draw(Draw),
    Sum,
    Table,
    /// `piecewise(cases)`, `piecewise(cases, default)` (round 13): nested `if`s
    Piecewise,
    /// `unit_step(x, ...)`: the exact 1 when every x >= 0, else the exact 0
    UnitStep,
    /// `heaviside_theta(x, ...)`: the exact 0 when some x < 0, the exact 1 when every x > 0 (else the call stays)
    Heaviside,
    /// `sign(x)`: the exact -1, 0 or 1
    Sign,
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
        ("list", Prim::List),
        ("part", Prim::Part),
        ("length", Prim::Length),
        ("n", Prim::N),
        ("complex", Prim::Complex),
        ("re", Prim::Re),
        ("im_part", Prim::ImPart),
        ("conjugate", Prim::Conjugate),
        ("random_real", Prim::Draw(Draw::Real)),
        ("random_normal", Prim::Draw(Draw::Normal)),
        ("random_integer", Prim::Draw(Draw::Integer)),
        ("sum", Prim::Sum),
        ("total", Prim::Sum),
        ("table", Prim::Table),
        ("piecewise", Prim::Piecewise),
        ("unit_step", Prim::UnitStep),
        ("heaviside_theta", Prim::Heaviside),
        ("sign", Prim::Sign),
    ];
    for (n, p) in fixed {
        m.insert(h.sym(n), p);
    }
    for f in Lib::ALL {
        m.insert(h.sym(f.name()), Prim::Math(f));
    }
    m
}

/// A local binding: a scalar variable, (with `list`) a list of values bound once, or (with `cplx`) the two `Real`
/// variables of a complex number.
#[derive(Clone)]
struct Bind {
    var: V,
    ty: Ty,
    mutable: bool,
    list: Option<Vec<Val>>,
    cplx: Option<(V, V)>,
    /// a vector: address, length (computed once: variables or constants), leaf type
    vector: Option<(E, E, Ty)>,
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
    /// a machine complex number
    Complex(E, E),
    /// an exact complex constant with integer parts (`im`, `2 + 3im`)
    CExact(i64, i64),
}

impl Operand {
    fn exact(&self) -> bool {
        matches!(self, Operand::Int(_) | Operand::Ratio(..) | Operand::CExact(..))
    }

    fn opd(self) -> Opd {
        match self {
            Operand::Int(x) => Opd::Int(x),
            Operand::Real(x) => Opd::Real(x),
            Operand::Ratio(n, d) => Opd::Ratio(n, d),
            Operand::Complex(a, b) => Opd::Complex(a, b),
            Operand::CExact(a, b) => Opd::CExact(E::Int(a), E::Int(b)),
        }
    }
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
    /// the depth of `stack` of the body being compiled (not inlined)
    top: usize,
    /// `pi` and `e` in that body are their machine reals (decision N0.1)
    numeric: bool,
    consts: [(Sym, f64); 2],
    /// the imaginary unit `im` (the exact `complex(0, 1)`)
    im: Sym,
    /// common subexpressions (round 13, decision LC.6): the values of pure subterms already computed, one table
    /// per open statement list (a value computed in a branch is not available after it), keyed by the term and the
    /// binding environment it was computed in
    memo: Vec<HashMap<(T<'h>, u64), Val>>,
    /// the caller's locals named by data given to inlined functions, renamed (decision LC.7)
    carried: u64,
    /// the current binding environment: a new number for every binding, scope, inlined body and assignment
    epoch: u64,
    epochs: u64,
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

    /// A new binding environment (the memo's entries of the old one stay valid in it only when restored).
    fn new_epoch(&mut self) -> u64 {
        let old = self.epoch;
        self.epochs += 1;
        self.epoch = self.epochs;
        old
    }

    /// Forget every common subexpression (an assignment, or a loop whose body may assign).
    fn forget(&mut self) {
        self.memo.iter_mut().for_each(HashMap::clear);
        self.new_epoch();
    }

    /// Whether the value of `t` may be computed once and reused (decision LC.6): a pure call whose value depends only
    /// on the bindings (arithmetic, comparisons, elementary, special and inlined functions), not one with an
    /// effect, a branch or a binding inside.
    fn memoizable(&self, t: T<'h>) -> bool {
        if self.src.draws() {
            return false;
        }
        match self.call_of(t) {
            Some((s, _)) => !matches!(
                self.prims.get(&s),
                Some(
                    Prim::If
                        | Prim::Block
                        | Prim::Let
                        | Prim::Var
                        | Prim::Assign
                        | Prim::While
                        | Prim::For
                        | Prim::Draw(_)
                        | Prim::Table
                        | Prim::Piecewise
                        | Prim::And
                        | Prim::Or
                )
            ),
            None => false,
        }
    }

    fn bind(&mut self, s: Sym, var: V, ty: Ty, mutable: bool) {
        self.new_epoch();
        let b = Bind { var, ty, mutable, list: None, cplx: None, vector: None };
        // invariant: the function's scope is pushed before any binding and popped last
        self.scopes.last_mut().expect("a scope").push((s, b));
    }

    fn bind_list(&mut self, s: Sym, items: Vec<Val>) {
        self.new_epoch();
        let b = Bind { var: 0, ty: Ty::Int, mutable: false, list: Some(items), cplx: None, vector: None };
        // invariant: the function's scope is pushed before any binding and popped last
        self.scopes.last_mut().expect("a scope").push((s, b));
    }

    fn bind_complex(&mut self, s: Sym, re: V, im: V, mutable: bool) {
        self.new_epoch();
        let b = Bind { var: re, ty: Ty::Real, mutable, list: None, cplx: Some((re, im)), vector: None };
        // invariant: the function's scope is pushed before any binding and popped last
        self.scopes.last_mut().expect("a scope").push((s, b));
    }

    /// Bind `s` to a vector, whose address and length must be variables or constants (computed once).
    fn bind_vector(&mut self, s: Sym, base: E, len: E, t: Ty) {
        self.new_epoch();
        let b = Bind { var: 0, ty: Ty::Int, mutable: false, list: None, cplx: None, vector: Some((base, len, t)) };
        // invariant: the function's scope is pushed before any binding and popped last
        self.scopes.last_mut().expect("a scope").push((s, b));
    }

    fn lookup(&self, s: Sym) -> Option<Bind> {
        self.scopes[self.base..].iter().rev().flat_map(|sc| sc.iter().rev()).find(|(x, _)| *x == s).map(|(_, b)| b.clone())
    }

    /// The value held in fresh temporaries (a list's leaves each in one), so that it is computed once.
    fn hold(&mut self, v: Val) -> Val {
        let shape = v.shape();
        let leaves: Vec<E> = v
            .leaves()
            .into_iter()
            .map(|(e, t)| match e {
                E::Var(_) | E::Int(_) | E::Real(_) | E::Bool(_) => e,
                e => self.temp(e, t),
            })
            .collect();
        Val::rebuild(&shape, &mut leaves.into_iter())
    }

    fn emit(&mut self, s: S) {
        // invariant: the function's block is open while statements are emitted
        self.out.last_mut().expect("a statement list").push(s);
    }

    /// Build a statement list with `f`.
    fn stmts<R>(&mut self, f: impl FnOnce(&mut Self) -> Res<R>) -> Res<(Vec<S>, R)> {
        self.out.push(vec![]);
        self.memo.push(HashMap::new());
        let r = f(self);
        self.memo.pop();
        // invariant: pushed two lines above; `f` pushes and pops in pairs
        let ss = self.out.pop().expect("the statement list");
        r.map(|r| (ss, r))
    }

    /// Run `f` in a new scope (the binding environment before it is the one after it).
    fn scoped<R>(&mut self, f: impl FnOnce(&mut Self) -> Res<R>) -> Res<R> {
        self.scopes.push(vec![]);
        let saved = self.new_epoch();
        let r = f(self);
        self.epoch = saved;
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

    /// `e` computed once: a variable or constant as it is, anything else in a temporary (for operands used twice).
    fn held(&mut self, e: E, t: Ty) -> E {
        match e {
            E::Var(_) | E::Int(_) | E::Real(_) | E::Bool(_) => e,
            e => self.temp(e, t),
        }
    }

    /// The machine complex number `p` computes, in two fresh variables.
    fn pair(&mut self, p: Pair) -> Val {
        let (re, im) = (self.var(Ty::Real), self.var(Ty::Real));
        self.emit(S::Pair(re, im, p));
        Val::C(E::Var(re), E::Var(im))
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

    /// A scalar value.
    fn expr(&mut self, t: T<'h>) -> Res<(E, Ty)> {
        match self.value(t)? {
            Val::S(e, ty) => Ok((e, ty)),
            Val::L(_) => Err("a list where a number is expected".into()),
            Val::C(..) => Err("a complex number where a real number or an integer is expected".into()),
            Val::V(..) => Err(VECTOR_AS_NUMBER.into()),
        }
    }

    /// A value: a scalar, or a list of known length (decision N0.3).
    fn value(&mut self, t: T<'h>) -> Res<Val> {
        if !self.memoizable(t) {
            return self.compute(t);
        }
        let key = (t, self.epoch);
        if let Some(v) = self.memo.iter().rev().find_map(|m| m.get(&key)) {
            return Ok(v.clone());
        }
        let v = self.compute(t)?;
        // an assignment inside (an inlined call's own locals) moved to another environment: not kept
        if self.epoch != key.1 || matches!(v, Val::V(..)) {
            return Ok(v);
        }
        let v = self.hold(v);
        // invariant: the function's statement list and its memo table are open while values are computed
        self.memo.last_mut().expect("a memo table").insert(key, v.clone());
        Ok(v)
    }

    /// [`Fe::value`] without the common subexpressions.
    fn compute(&mut self, t: T<'h>) -> Res<Val> {
        self.tick()?;
        let h = self.h;
        let sc = |(e, ty): (E, Ty)| Val::S(e, ty);
        match h.view(t) {
            View::Int(v) => Ok(Val::S(E::Int(v), Ty::Int)),
            View::BigInt { .. } => Err("an integer literal beyond 64 bits".into()),
            View::Real(x) => Ok(Val::S(E::Real(x), Ty::Real)),
            View::Rational { .. } => {
                Err("an exact rational outside a sum or product with a machine real (the result would be exact)".into())
            }
            View::Sym(s) => match self.prims.get(&s) {
                Some(Prim::True) => Ok(Val::S(E::Bool(true), Ty::Bool)),
                Some(Prim::False) => Ok(Val::S(E::Bool(false), Ty::Bool)),
                _ => match self.lookup(s) {
                    Some(Bind { list: Some(items), .. }) => Ok(Val::L(items)),
                    Some(Bind { vector: Some((base, len, t)), .. }) => Ok(Val::V(base, len, t)),
                    Some(Bind { cplx: Some((re, im)), .. }) => Ok(Val::C(E::Var(re), E::Var(im))),
                    Some(bd) => Ok(Val::S(E::Var(bd.var), bd.ty)),
                    None => match self.constant(s) {
                        Some(x) if self.numeric && self.stack.len() == self.top => Ok(Val::S(E::Real(x), Ty::Real)),
                        Some(_) => Err(format!(
                            "the constant `{}` (the interpreter keeps it exact; n({0}) is its machine real)",
                            self.name(s)
                        )),
                        None => match self.global_number(s) {
                            Some(v) => self.compute(v),
                            None => Err(format!("the free name `{}`", self.name(s))),
                        },
                    },
                },
            },
            View::App { head, args } => {
                // `derivative(n)(f)` of an interpolation or a solution of `n_dsolve` (or of a global bound to one):
                // the term mc-calculus rewrites it to
                if let Some(target) = self.derivative_head(head) {
                    let v = match self.spline_call(target, args)? {
                        Some(v) => Some(v),
                        None => match crate::callee::resolve(h, target, args.len()) {
                            Some(id) => Some(self.library_call(id, h.sym("derivative"), args, false)?),
                            None => None,
                        },
                    };
                    if let Some(v) = v {
                        return Ok(v);
                    }
                }
                if let Some(v) = self.spline_call(head, args)? {
                    return Ok(v);
                }
                // a curried head another crate compiles (a solution of `n_dsolve`)
                if let Some(id) = crate::callee::resolve(h, head, args.len()) {
                    return self.library_call(id, h.sym("call"), args, false);
                }
                // a global bound to an interpolation, called: the interpolation itself
                if let View::Sym(g) = h.view(head) {
                    if self.lookup(g).is_none() {
                        if let Some(value) = self.src.constant(g) {
                            let v = match self.spline_call(value, args)? {
                                Some(v) => Some(v),
                                None => match crate::callee::resolve(h, value, args.len()) {
                                    Some(id) => Some(self.library_call(id, g, args, false)?),
                                    None => None,
                                },
                            };
                            if let Some(v) = v {
                                // a read like an inlined function's: redefining g invalidates the code (1b.6)
                                if !self.deps.contains(&g) {
                                    self.deps.push(g);
                                }
                                return Ok(v);
                            }
                        }
                    }
                }
                // a head that is data known at compile time (`r["f"](x)` with `r` an association): the data called
                if !matches!(h.view(head), View::Sym(_)) {
                    if let Some(d) = self.data_term(head).filter(|&d| d != head) {
                        return self.compute(h.app(d, args));
                    }
                }
                let View::Sym(s) = h.view(head) else { return Err("a call whose head is not a symbol".into()) };
                match self.prims.get(&s).copied() {
                    Some(Prim::List) => {
                        let items = args.iter().map(|&a| self.value(a)).collect::<Res<Vec<_>>>()?;
                        if items.iter().any(|v| matches!(v.shape(), Shape::Vector(_))) {
                            return Err("a list of vectors (a matrix does not compile)".into());
                        }
                        Ok(Val::L(items))
                    }
                    Some(Prim::Part) => match self.fold_part(t) {
                        Some(e) => self.value(e),
                        None => self.part(args),
                    },
                    Some(Prim::Length) => {
                        self.arity(s, args, 1)?;
                        match self.value(args[0])? {
                            Val::L(items) => Ok(Val::S(E::Int(items.len() as i64), Ty::Int)),
                            Val::V(_, len, _) => Ok(Val::S(len, Ty::Int)),
                            Val::S(..) | Val::C(..) => Err("`length` of a number".into()),
                        }
                    }
                    Some(Prim::Sum) => {
                        self.arity(s, args, 1)?;
                        match self.value(args[0])? {
                            Val::V(base, len, t) => Ok(Val::S(E::VSum(t, b(base), b(len)), t)),
                            _ => Err(format!("`{}` of something other than a vector", self.name(s))),
                        }
                    }
                    Some(Prim::Table) => self.table(args),
                    Some(Prim::Piecewise) => self.piecewise(args),
                    Some(Prim::N) => self.numeric_value(s, args).map(sc),
                    Some(Prim::If) => match args {
                        [c, x, y] => self.if_value(*c, *x, *y),
                        [_, _] => Err("an `if` without `else` used as a value (it can be null)".into()),
                        _ => Err("a malformed `if`".into()),
                    },
                    Some(Prim::Block) => self.scoped(|fe| fe.block_value(args)),
                    Some(Prim::Add) => self.sum(args),
                    Some(Prim::Mul) => self.product(args),
                    Some(Prim::Pow) => {
                        self.arity(s, args, 2)?;
                        self.power(args[0], args[1])
                    }
                    Some(Prim::Abs) => {
                        self.arity(s, args, 1)?;
                        let v = self.value(args[0])?;
                        self.abs(v)
                    }
                    Some(Prim::Sqrt) => {
                        self.arity(s, args, 1)?;
                        // sqrt(x) is the canonical power x^(1/2)
                        match self.value(args[0])? {
                            Val::S(x, Ty::Real) => {
                                Ok(Val::S(E::Num(NumOp::Pow, vec![Opd::Real(x), Opd::Ratio(E::Int(1), E::Int(2))]), Ty::Real))
                            }
                            Val::C(a, b) => Ok(self.pair(Pair::Num(NumOp::Pow, vec![Opd::Complex(a, b), Opd::Ratio(E::Int(1), E::Int(2))]))),
                            _ => Err("`sqrt` of an exact or boolean value (the result stays exact)".into()),
                        }
                    }
                    Some(Prim::Math(f)) => {
                        self.arity(s, args, 1)?;
                        match self.value(args[0])? {
                            Val::S(x, Ty::Real) => Ok(Val::S(E::Math(f, b(x)), Ty::Real)),
                            Val::C(a, b) => Ok(self.pair(Pair::Math(f, a, b))),
                            _ => Err(format!("`{}` of an exact or boolean value (the result stays exact)", f.name())),
                        }
                    }
                    Some(p @ (Prim::Re | Prim::ImPart | Prim::Conjugate)) => {
                        self.arity(s, args, 1)?;
                        let v = self.value(args[0])?;
                        self.complex_part(p, v)
                    }
                    Some(Prim::Complex) => self.complex(args),
                    Some(Prim::Draw(d)) => self.draw(d, args),
                    Some(Prim::Assign) => {
                        self.assign(args)?;
                        // invariant: `assign` returned Ok only for a symbol target
                        let View::Sym(x) = self.h.view(args[0]) else { unreachable!("checked by assign") };
                        // invariant: `assign` returned Ok only for a bound symbol
                        match self.lookup(x).expect("checked by assign") {
                            Bind { cplx: Some((re, im)), .. } => Ok(Val::C(E::Var(re), E::Var(im))),
                            bd => Ok(Val::S(E::Var(bd.var), bd.ty)),
                        }
                    }
                    Some(p) => self.prim(p, s, args).map(sc),
                    None if self.lookup(s).is_none() => match crate::callee::special(&self.name(s), args.len()) {
                        Some(id) => self.library_call(id, s, args, true),
                        None => self.inline(s, args),
                    },
                    None => self.inline(s, args),
                }
            }
            View::Ball { .. } => Err("a ball".into()),
            View::Str(_) => Err("a string".into()),
        }
    }

    /// The machine real of the constant `s` (`pi`, `e`), if it is one.
    fn constant(&self, s: Sym) -> Option<f64> {
        self.consts.iter().find(|c| c.0 == s).map(|c| c.1)
    }

    /// `n(x)` at machine precision: a constant's machine real, a machine real itself, an integer converted (exactly
    /// up to 2^53, else declining, as `ToReal` does).
    fn numeric_value(&mut self, s: Sym, args: &'h [T<'h>]) -> Res<(E, Ty)> {
        self.arity(s, args, 1)?;
        if let View::Sym(c) = self.h.view(args[0]) {
            if let (None, Some(x)) = (self.lookup(c), self.constant(c)) {
                return Ok((E::Real(x), Ty::Real));
            }
        }
        match self.expr(args[0])? {
            (x, Ty::Real) => Ok((x, Ty::Real)),
            (x, Ty::Int) => Ok((E::ToReal(b(x)), Ty::Real)),
            (_, Ty::Bool) => Err("`n` of a boolean".into()),
        }
    }

    /// `part(y, k, ...)`: an element of a list value by constant indices (negative ones count from the end).
    fn part(&mut self, args: &'h [T<'h>]) -> Res<Val> {
        let Some((&base, idx)) = args.split_first() else { return Err("`part` without arguments".into()) };
        let mut v = self.value(base)?;
        if let Val::V(base, len, t) = v {
            // a vector by a run-time index (package P): one index, an integer
            let [i] = idx else { return Err("a vector indexed by other than one integer".into()) };
            let (i, ti) = self.expr(*i)?;
            let i = int(i, ti).map_err(|_| "a vector indexed by other than an integer".to_string())?;
            return Ok(Val::S(E::Elem(t, b(base), b(len), b(i)), t));
        }
        for &i in idx {
            let View::Int(k) = self.h.view(i) else { return Err("an index that is not a constant integer".into()) };
            let Val::L(items) = v else { return Err("an index into a number".into()) };
            let n = items.len() as i64;
            let at = match k {
                1.. if k <= n => k - 1,
                ..=-1 if -k <= n => n + k,
                _ => return Err(format!("the index {k} of a list of length {n}")),
            };
            // invariant: `at` was checked against the length just above
            v = items.into_iter().nth(at as usize).expect("checked index");
        }
        Ok(v)
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
            // handled by `value`
            Prim::If
            | Prim::Block
            | Prim::List
            | Prim::Part
            | Prim::Length
            | Prim::N
            | Prim::Add
            | Prim::Mul
            | Prim::Pow
            | Prim::Abs
            | Prim::Sqrt
            | Prim::Math(_)
            | Prim::Re
            | Prim::ImPart
            | Prim::Conjugate
            | Prim::Complex
            | Prim::Draw(_)
            | Prim::Sum
            | Prim::Table
            | Prim::Piecewise
            | Prim::Assign => Err(format!("`{}` outside its place", self.name(s))),
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
            Prim::Min | Prim::Max => self.extreme(p == Prim::Max, args),
            Prim::UnitStep | Prim::Heaviside => self.step(p == Prim::Heaviside, s, args),
            Prim::Sign => self.sign(s, args),
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

    /// An operand of a sum or product: an exact rational literal, an exact complex constant, `b^-k` (exact), or a
    /// value.
    fn operand(&mut self, t: T<'h>) -> Res<Operand> {
        let h = self.h;
        if let View::Rational { num, den } = h.view(t) {
            return match (h.view(num), h.view(den)) {
                (View::Int(n), View::Int(d)) => Ok(Operand::Ratio(E::Int(n), E::Int(d))),
                _ => Err("a rational literal beyond 64 bits".into()),
            };
        }
        if let View::Sym(g) = h.view(t) {
            // a global holding a number: its value (an exact rational is an operand like a literal)
            if self.lookup(g).is_none() && !self.prims.contains_key(&g) && self.constant(g).is_none() {
                if let Some(v) = self.global_number(g) {
                    return self.operand(v);
                }
            }
        }
        if let Some(c) = self.exact_complex(t)? {
            return Ok(c);
        }
        if let Some(c) = self.exact_constant_term(t) {
            return Self::constant_operand(&c);
        }
        if let Some((Prim::Pow, [base, e])) = self.prim_of(t) {
            if let View::Int(k @ i64::MIN..=-1) = h.view(*e) {
                let bv = self.value(*base)?;
                if let Val::S(x, Ty::Int) = bv {
                    let k = k.checked_neg().ok_or("an exponent beyond 64 bits")?;
                    // b^-1 (a division) needs no power
                    let den = if k == 1 { x } else { E::I(IOp::Pow, b(x), b(E::Int(k))) };
                    return Ok(Operand::Ratio(E::Int(1), den));
                }
                // a machine real or complex number to a negative power: a value like any other
                let v = self.power_val(bv, Val::S(E::Int(k), Ty::Int))?;
                return Self::operand_of(v);
            }
        }
        let v = self.value(t)?;
        Self::operand_of(v)
    }

    /// The value of a sum or product of exact constants with an exact complex one among them (`2 + 3im` written in
    /// parentheses stays a sum in the body), computed by the arithmetic; `None` for anything else.
    fn exact_constant_term(&self, t: T<'h>) -> Option<Num> {
        fn go<'h>(fe: &Fe<'h, '_>, t: T<'h>, complex: &mut bool) -> Option<Num> {
            let h = fe.h;
            match h.view(t) {
                View::Int(n) => return Some(Num::int(n)),
                View::Rational { num, den } => {
                    let (View::Int(n), View::Int(d)) = (h.view(num), h.view(den)) else { return None };
                    return Some(Num::rat(Rat::new(Int::from_i64(n), Int::from_i64(d))));
                }
                _ => {}
            }
            if let Ok(Some(Operand::CExact(a, b))) = fe.exact_complex(t) {
                *complex = true;
                return Some(Num::complex(Real::int(a), Real::int(b)));
            }
            let (p, args) = fe.prim_of(t)?;
            let nums = args.iter().map(|&a| go(fe, a, complex)).collect::<Option<Vec<Num>>>()?;
            match p {
                Prim::Add => Some(Num::sum(nums)),
                Prim::Mul => Some(Num::product(nums)),
                _ => None,
            }
        }
        let mut complex = false;
        let v = go(self, t, &mut complex)?;
        complex.then_some(v)
    }

    /// An exact constant as an operand (an integer, a ratio of machine integers, a complex number with integer
    /// parts).
    fn constant_operand(c: &Num) -> Res<Operand> {
        let small = |r: &Real| match r {
            Real::Exact(q) => Some((q.num.to_i64()?, Int::from_nat(q.den.clone()).to_i64()?)),
            _ => None,
        };
        let bad = || "an exact constant beyond machine integers or with rational complex parts".to_string();
        match c {
            Num::Real(r) => match small(r).ok_or_else(bad)? {
                (n, 1) => Ok(Operand::Int(E::Int(n))),
                (n, d) => Ok(Operand::Ratio(E::Int(n), E::Int(d))),
            },
            Num::Complex(a, b) => match (small(a), small(b)) {
                (Some((x, 1)), Some((y, 1))) => Ok(Operand::CExact(x, y)),
                _ => Err(bad()),
            },
        }
    }

    /// A computed value as an operand of a sum or product.
    fn operand_of(v: Val) -> Res<Operand> {
        match v {
            Val::S(x, Ty::Int) => Ok(Operand::Int(x)),
            Val::S(x, Ty::Real) => Ok(Operand::Real(x)),
            Val::S(_, Ty::Bool) => Err("arithmetic on a boolean".into()),
            Val::C(a, b) => Ok(Operand::Complex(a, b)),
            Val::L(_) => Err("a list where a number is expected".into()),
            Val::V(..) => Err(VECTOR_AS_NUMBER.into()),
        }
    }

    /// An exact complex literal `complex(a, b)` with integer parts (`im` is `complex(0, 1)`), if `t` is one.
    fn exact_complex(&self, t: T<'h>) -> Res<Option<Operand>> {
        if let View::Sym(s) = self.h.view(t) {
            return Ok((s == self.im && self.lookup(s).is_none()).then_some(Operand::CExact(0, 1)));
        }
        let Some((Prim::Complex, [re, im])) = self.prim_of(t) else { return Ok(None) };
        Ok(match (self.h.view(*re), self.h.view(*im)) {
            (View::Int(a), View::Int(b)) => Some(Operand::CExact(a, b)),
            (View::Int(_) | View::Rational { .. } | View::BigInt { .. }, View::Rational { .. } | View::BigInt { .. })
            | (View::Rational { .. } | View::BigInt { .. }, View::Int(_)) => {
                return Err("an exact complex number with a rational or big part".into())
            }
            _ => None,
        })
    }

    /// `add(...)`: exact integers add exactly; with machine reals, the arithmetic's canonical sum (exact operands
    /// first, then the inexact ones in canonical order): one IEEE addition for two operands, otherwise an
    /// [`E::Num`] that lowering expands inline; with complex numbers [`Fe::complex_fold`].
    fn sum(&mut self, args: &'h [T<'h>]) -> Res<Val> {
        let ops = self.operands(args)?;
        self.fold(NumOp::Sum, ops)
    }

    /// The operands of a sum or product. A closed numeric operand (`pi`, `pi^-3`, `sqrt(2)`) with a machine real
    /// among the others is its machine value, as the evaluator's contagion for constants makes it (`n` of the
    /// operand, computed here once; decision LC.9); without one it stays exact and is refused.
    fn operands(&mut self, args: &'h [T<'h>]) -> Res<Vec<Operand>> {
        let mut ops: Vec<Option<Operand>> = Vec::with_capacity(args.len());
        let mut first_err = None;
        for &a in args {
            match self.operand(a) {
                Ok(o) => ops.push(Some(o)),
                Err(e) if self.closed_numeric(a) => {
                    first_err.get_or_insert(e);
                    ops.push(None);
                }
                Err(e) => return Err(e),
            }
        }
        if let Some(e) = first_err {
            let machine = ops.iter().flatten().any(|o| matches!(o, Operand::Real(_) | Operand::Complex(..)));
            if !machine {
                return Err(e);
            }
            for (o, &a) in ops.iter_mut().zip(args) {
                if o.is_none() {
                    *o = Some(Operand::Real(E::Real(self.machine_value(a).ok_or(e.clone())?)));
                }
            }
        }
        Ok(ops.into_iter().flatten().collect())
    }

    /// Whether `t` is a closed numeric term that is not a number (the evaluator's test for its contagion into
    /// constants: numbers and the numeric constants at the leaves).
    fn closed_numeric(&self, t: T<'h>) -> bool {
        fn leaves(fe: &Fe<'_, '_>, t: T<'_>) -> bool {
            match fe.h.view(t) {
                View::Sym(s) => {
                    ["pi", "e", "euler_gamma", "catalan", "golden_ratio"].contains(&fe.h.sym_name(s).as_str())
                        && fe.lookup(s).is_none()
                }
                View::App { args, .. } => !args.is_empty() && args.iter().all(|&a| leaves(fe, a)),
                View::Str(_) => false,
                _ => true,
            }
        }
        let list = self.h.sym("list");
        match self.h.view(t) {
            View::Sym(_) => leaves(self, t),
            View::App { head, args } => {
                head != self.h.sym_term(list) && self.h.packed(t).is_none() && !args.is_empty() && leaves(self, t)
            }
            _ => false,
        }
    }

    /// `n(t)`, the machine real of a closed numeric term (the numerical evaluation `n` performs), if it is one.
    fn machine_value(&self, t: T<'h>) -> Option<f64> {
        use mc_numeric::numeval::{numericize, Names, Target};
        let ar = mc_arith::Arith::new(self.h);
        let r = numericize(self.h, &ar, &Names::new(self.h), t, Target::Machine, &mut |_| Ok(())).ok()?;
        match self.h.view(r.term) {
            View::Real(v) if r.reached => Some(v),
            _ => None,
        }
    }

    /// `mul(...)`: exact integers multiply exactly (with `b^-k` factors an exact division); with machine reals, the
    /// canonical product; with complex numbers [`Fe::complex_fold`].
    fn product(&mut self, args: &'h [T<'h>]) -> Res<Val> {
        let ops = self.operands(args)?;
        self.fold(NumOp::Product, ops)
    }

    fn fold(&mut self, op: NumOp, ops: Vec<Operand>) -> Res<Val> {
        if ops.iter().any(|o| matches!(o, Operand::Complex(..) | Operand::CExact(..))) {
            return self.complex_fold(op, ops);
        }
        let (e, t) = self.real_fold(op, ops)?;
        Ok(Val::S(e, t))
    }

    /// A sum or product with a complex operand, as `Num::sum` / `Num::product` computes it (decision N4b.3): the
    /// exact operands combined exactly and converted to machine numbers, then the inexact ones folded in canonical
    /// order starting with the exact part. Two inexact operands without an exact part, or one with it, need no
    /// order (complex addition and multiplication of two operands commute bit for bit), so they are computed
    /// inline with the arithmetic's formulas; anything else calls the arithmetic.
    fn complex_fold(&mut self, op: NumOp, ops: Vec<Operand>) -> Res<Val> {
        let (exact, inexact): (Vec<Operand>, Vec<Operand>) = ops.into_iter().partition(Operand::exact);
        if inexact.is_empty() {
            return Err("exact complex arithmetic (the result stays exact)".into());
        }
        let inline = if exact.is_empty() { inexact.len() <= 2 } else { inexact.len() == 1 };
        if !inline {
            let opds = exact.into_iter().chain(inexact).map(Operand::opd).collect();
            return Ok(self.pair(Pair::Num(op, opds)));
        }
        let mut acc = if exact.is_empty() {
            None
        } else if exact.iter().any(|o| matches!(o, Operand::CExact(..))) {
            Some(Self::exact_constant(op, &exact)?)
        } else {
            // computed integers and rationals: exact + 0.0 (exact * 1.0) is the exact part converted as the
            // arithmetic converts it (correctly rounded; the exact zero gives +0.0)
            let unit = if op == NumOp::Sum { 0.0 } else { 1.0 };
            let mut opds: Vec<Opd> = exact.into_iter().map(Operand::opd).collect();
            opds.push(Opd::Real(E::Real(unit)));
            Some(Val::S(E::Num(op, opds), Ty::Real))
        };
        for o in inexact {
            let x = match o {
                Operand::Real(x) => Val::S(x, Ty::Real),
                Operand::Complex(a, b) => Val::C(a, b),
                // invariant: `inexact` holds only real and complex operands (partitioned above)
                _ => unreachable!("inexact operands"),
            };
            acc = Some(match acc {
                None => x,
                Some(a) => self.op2(op, a, x),
            });
        }
        // invariant: called only with at least one inexact operand
        Ok(acc.expect("an inexact operand"))
    }

    /// The exact constant operands of a sum or product (one an exact complex number) combined by the arithmetic
    /// and converted to a machine number (`to_kind`), as the arithmetic's fold starts.
    fn exact_constant(op: NumOp, exact: &[Operand]) -> Res<Val> {
        let mut nums = vec![];
        for o in exact {
            nums.push(match o {
                Operand::Int(E::Int(n)) => Num::int(*n),
                Operand::Ratio(E::Int(n), E::Int(d)) if *d != 0 => Num::rat(Rat::new(Int::from_i64(*n), Int::from_i64(*d))),
                Operand::CExact(a, b) => Num::complex(Real::int(*a), Real::int(*b)),
                _ => return Err("an exact complex number with a computed exact operand".into()),
            });
        }
        let v = if op == NumOp::Sum { Num::sum(nums) } else { Num::product(nums) };
        match v.to_kind(Kind::Machine, 0) {
            Num::Real(Real::Machine(x)) => Ok(Val::S(E::Real(x), Ty::Real)),
            Num::Complex(Real::Machine(a), Real::Machine(b)) => Ok(Val::C(E::Real(a), E::Real(b))),
            _ => Err("an exact part that is not a machine number".into()),
        }
    }

    /// `x + y` or `x y` of two machine numbers (reals or complex), as `Num::add` / `Num::mul` compute it with `x`
    /// as the left operand.
    fn op2(&mut self, op: NumOp, x: Val, y: Val) -> Val {
        let f = |o: FOp, a: E, c: E| E::F(o, b(a), b(c));
        match (x, y) {
            (Val::S(p, _), Val::S(q, _)) => Val::S(f(if op == NumOp::Sum { FOp::Add } else { FOp::Mul }, p, q), Ty::Real),
            // real + complex: (a + c) + d i; real * complex: a c + a d i (`Num::complex(a.mul(c), a.mul(d))`)
            (Val::S(a, _), Val::C(c, d)) | (Val::C(c, d), Val::S(a, _)) => {
                if op == NumOp::Sum {
                    Val::C(f(FOp::Add, a, c), d)
                } else {
                    let a = self.held(a, Ty::Real);
                    Val::C(f(FOp::Mul, a.clone(), c), f(FOp::Mul, a, d))
                }
            }
            (Val::C(a, bb), Val::C(c, d)) => {
                if op == NumOp::Sum {
                    Val::C(f(FOp::Add, a, c), f(FOp::Add, bb, d))
                } else {
                    self.cmul((a, bb), (c, d))
                }
            }
            // invariant: both operands are numbers (the caller's match)
            _ => unreachable!("numbers"),
        }
    }

    /// `(a + b i)(c + d i)` as `Num::mul` computes it: `(a c - b d) + (a d + b c) i` (the subtraction is the
    /// addition of `-(b d)`, the same IEEE operation), the parts held in temporaries.
    fn cmul(&mut self, x: (E, E), y: (E, E)) -> Val {
        let (a, bb) = (self.held(x.0, Ty::Real), self.held(x.1, Ty::Real));
        let (c, d) = (self.held(y.0, Ty::Real), self.held(y.1, Ty::Real));
        let m = |p: &E, q: &E| E::F(FOp::Mul, b(p.clone()), b(q.clone()));
        let re = E::F(FOp::Sub, b(m(&a, &c)), b(m(&bb, &d)));
        let im = E::F(FOp::Add, b(m(&a, &d)), b(m(&bb, &c)));
        let (re, im) = (self.temp(re, Ty::Real), self.temp(im, Ty::Real));
        Val::C(re, im)
    }

    /// `z^k` for a machine complex `z` and an integer constant `k != 0`, as `Num::pow_int` computes it: the
    /// reciprocal `(a - b i) / (a^2 + b^2)` first for `k < 0`, then binary powering (square, and multiply by the
    /// base for each one bit after the leading one).
    fn cpow_int(&mut self, a: E, bb: E, k: i64) -> Res<Val> {
        if k == 0 {
            return Err("a complex number to the power 0 (the result is the exact 1)".into());
        }
        let (mut a, mut bb) = (self.held(a, Ty::Real), self.held(bb, Ty::Real));
        if k < 0 {
            let m = |p: &E| E::F(FOp::Mul, b(p.clone()), b(p.clone()));
            let d = E::F(FOp::Add, b(m(&a)), b(m(&bb)));
            let d = self.temp(d, Ty::Real);
            let re = E::F(FOp::Div, b(a.clone()), b(d.clone()));
            let im = E::F(FOp::Div, b(E::NegF(b(bb.clone()))), b(d));
            (a, bb) = (self.temp(re, Ty::Real), self.temp(im, Ty::Real));
        }
        let e = k.unsigned_abs();
        let base = (a, bb);
        let mut r = Val::C(base.0.clone(), base.1.clone());
        let mut i = 63 - e.leading_zeros();
        while i > 0 {
            i -= 1;
            // invariant: `r` starts complex and every step keeps it complex
            let Val::C(p, q) = r else { unreachable!("complex") };
            r = self.cmul((p.clone(), q.clone()), (p, q));
            if (e >> i) & 1 == 1 {
                // invariant: `r` starts complex and every step keeps it complex
                let Val::C(p, q) = r else { unreachable!("complex") };
                r = self.cmul((p, q), base.clone());
            }
        }
        Ok(r)
    }

    fn real_fold(&mut self, op: NumOp, ops: Vec<Operand>) -> Res<(E, Ty)> {
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
                    // invariant: the exact branch is taken only when no operand is inexact
                    Operand::Real(_) | Operand::Complex(..) | Operand::CExact(..) => unreachable!("integers and ratios"),
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
                // invariant: this branch is taken only without ratios and complex operands
                Operand::Ratio(..) | Operand::Complex(..) | Operand::CExact(..) => unreachable!("integers and reals"),
            });
            // invariant: `ops.len() == 2` (checked just above)
            let (x, y) = (it.next().expect("two"), it.next().expect("two"));
            return Ok((E::F(fop, b(x), b(y)), Ty::Real));
        }
        if ops.len() == 1 {
            if let Some(Operand::Real(x)) = ops.into_iter().next() {
                return Ok((x, Ty::Real));
            }
            // invariant: one operand, real (the exact cases returned earlier)
            unreachable!("one real operand");
        }
        let opds = ops.into_iter().map(Operand::opd).collect();
        Ok((E::Num(op, opds), Ty::Real))
    }

    /// `pow(a, e)`.
    fn power(&mut self, base: T<'h>, e: T<'h>) -> Res<Val> {
        let h = self.h;
        let bv = self.value(base)?;
        if let View::Rational { num, den } = h.view(e) {
            let (View::Int(n), View::Int(d)) = (h.view(num), h.view(den)) else {
                return Err("a rational exponent beyond 64 bits".into());
            };
            let q = Opd::Ratio(E::Int(n), E::Int(d));
            return match bv {
                Val::S(x, Ty::Real) => Ok(Val::S(E::Num(NumOp::Pow, vec![Opd::Real(x), q]), Ty::Real)),
                Val::C(a, bb) => Ok(self.pair(Pair::Num(NumOp::Pow, vec![Opd::Complex(a, bb), q]))),
                _ => Err("an exact power with a rational exponent (the result stays exact)".into()),
            };
        }
        if let Some(c) = self.exact_complex(e)? {
            // a power with an exact complex exponent: the arithmetic computes it (a machine base, or declines)
            let base = Self::operand_of(bv)?;
            return Ok(self.pair(Pair::Num(NumOp::Pow, vec![base.opd(), c.opd()])));
        }
        let ev = self.value(e)?;
        self.power_val(bv, ev)
    }

    /// The power of two computed values: complex bases to integer constants inline ([`Fe::cpow_int`]), other powers
    /// with a complex base or exponent by the arithmetic, real ones as before.
    fn power_val(&mut self, bv: Val, ev: Val) -> Res<Val> {
        match (bv, ev) {
            (Val::C(a, bb), Val::S(E::Int(k), Ty::Int)) => self.cpow_int(a, bb, k),
            (bv @ Val::C(..), ev) | (bv, ev @ Val::C(..)) => {
                let (x, y) = (Self::operand_of(bv)?, Self::operand_of(ev)?);
                Ok(self.pair(Pair::Num(NumOp::Pow, vec![x.opd(), y.opd()])))
            }
            (Val::S(x, tx), Val::S(y, ty)) => self.power_of(x, tx, y, ty).map(|(e, t)| Val::S(e, t)),
            _ => Err("a power of a list".into()),
        }
    }

    /// `abs(x)`: integers and machine reals as before; a machine complex number as the native computes it,
    /// `(a a + b b)^(1/2)` (the canonical sum and power of the parts' squares).
    fn abs(&mut self, v: Val) -> Res<Val> {
        match v {
            Val::S(x, Ty::Int) => {
                // |x| = x < 0 ? 0 - x : x (0 - x declines only for MIN, whose absolute value is not an i64)
                let x = self.temp(x, Ty::Int);
                let neg = E::I(IOp::Sub, b(E::Int(0)), b(x.clone()));
                let lt = E::Cmp(Cc::Lt, b(x.clone()), b(E::Int(0)));
                Ok(Val::S(E::Sel(b(lt), b(neg), b(x)), Ty::Int))
            }
            Val::S(x, Ty::Real) => Ok(Val::S(E::AbsF(b(x)), Ty::Real)),
            Val::C(a, bb) => {
                let (a, bb) = (self.held(a, Ty::Real), self.held(bb, Ty::Real));
                let sq = |p: E| E::F(FOp::Mul, b(p.clone()), b(p));
                let s = E::F(FOp::Add, b(sq(a)), b(sq(bb)));
                Ok(Val::S(E::Num(NumOp::Pow, vec![Opd::Real(s), Opd::Ratio(E::Int(1), E::Int(2))]), Ty::Real))
            }
            _ => Err("`abs` of a boolean or a list".into()),
        }
    }

    /// `re(z)`, `im_part(z)`, `conjugate(z)` of a number: the parts of a complex number; a real number is its own
    /// real part and conjugate, and its imaginary part is the exact 0.
    fn complex_part(&mut self, p: Prim, v: Val) -> Res<Val> {
        match (p, v) {
            (_, Val::S(_, Ty::Bool) | Val::L(_) | Val::V(..)) => Err("the complex part of a boolean or a list".into()),
            (Prim::ImPart, Val::S(..)) => Ok(Val::S(E::Int(0), Ty::Int)),
            (_, v @ Val::S(..)) => Ok(v),
            (Prim::Re, Val::C(a, _)) => Ok(Val::S(a, Ty::Real)),
            (Prim::ImPart, Val::C(_, bb)) => Ok(Val::S(bb, Ty::Real)),
            (_, Val::C(a, bb)) => Ok(Val::C(a, E::NegF(b(bb)))),
        }
    }

    /// `complex(x, y)` of machine numbers: a machine complex number (an integer part is converted, as the
    /// arithmetic's contagion does; an imaginary part 0.0 stays). Both parts exact give an exact number: refused.
    fn complex(&mut self, args: &'h [T<'h>]) -> Res<Val> {
        let [re, im] = args else { return Err("`complex` with other than two arguments".into()) };
        let (x, tx) = self.expr(*re)?;
        let (y, ty) = self.expr(*im)?;
        let real = |e: E, t: Ty| -> Res<E> {
            match t {
                Ty::Real => Ok(e),
                Ty::Int => Ok(E::ToReal(b(e))),
                Ty::Bool => Err("a complex number with a boolean part".to_string()),
            }
        };
        if tx == Ty::Int && ty == Ty::Int {
            return Err("an exact complex number (the result stays exact)".into());
        }
        Ok(Val::C(real(x, tx)?, real(y, ty)?))
    }

    /// A machine real argument of a draw: a machine real, an integer (converted), or a rational literal.
    /// `interpolated(xs, ys, ms)(x)` (the value of `interpolation`, mc-io) whose nodes are packed real arrays: a call
    /// of the registered spline (`mc_numeric::spline`), the interpreter's own evaluation; `None` for other calls.
    fn spline_call(&mut self, head: T<'h>, args: &'h [T<'h>]) -> Res<Option<Val>> {
        let h = self.h;
        let View::App { head: inner, args: data } = h.view(head) else { return Ok(None) };
        let View::Sym(s) = h.view(inner) else { return Ok(None) };
        if self.name(s) != "interpolated" {
            return Ok(None);
        }
        let reals = |t: T<'h>| h.packed(t).and_then(|p| p.reals());
        // `interpolated(xs, ys, ms_or_degree, d)`: the d-th derivative (mc-io's `Interpolated`)
        let (data, order) = match data {
            [_, _, _, d] => match h.view(*d) {
                View::Int(d @ 0..=64) => (&data[..3], d as u32),
                _ => return Err("a derivative of an interpolation of an order that is not an integer from 0 to 64".into()),
            },
            _ => (data, 0),
        };
        let (&[xt, yt, mt], &[x]) = (data, args) else { return Err("a call of an interpolation with other than one argument".into()) };
        // the local scheme (`interpolation_order: k`): the degree in place of the second derivatives
        let degree = match h.view(mt) {
            View::Int(k @ 1..=20) => Some(k as usize),
            View::Int(_) => return Err("a local interpolation of a degree outside 1 to 20".into()),
            _ => None,
        };
        if degree.is_some() || order > 0 {
            let packed = "an interpolation of fewer than 64 points (its nodes are not a packed array)";
            let (Some(xs), Some(ys)) = (reals(xt), reals(yt)) else { return Err(packed.into()) };
            let (ms, scheme): (&[f64], _) = match degree {
                Some(k) => (&[], crate::callee::Scheme::Local(k)),
                None => (reals(mt).ok_or(packed)?, crate::callee::Scheme::Spline),
            };
            if xs.len() < 2 || ys.len() != xs.len() || (degree.is_none() && ms.len() != xs.len()) {
                return Err("a malformed interpolation".into());
            }
            let id = crate::callee::interpolation(xs, ys, ms, scheme, order);
            let xe = self.real_arg(x)?;
            let v = self.temp(E::Call(id, vec![xe]), Ty::Real);
            return Ok(Some(Val::S(v, Ty::Real)));
        }
        let (Some(xs), Some(ys), Some(ms)) = (reals(xt), reals(yt), reals(mt)) else {
            return Err("an interpolation of fewer than 64 points (its nodes are not a packed array)".into());
        };
        if xs.len() < 2 || ys.len() != xs.len() || ms.len() != xs.len() {
            return Err("a malformed interpolation".into());
        }
        let id = mc_numeric::spline::register(xs, ys, ms);
        let xv = self.value(x)?;
        let xe = match xv {
            Val::S(e, Ty::Real) => e,
            Val::S(e, Ty::Int) => E::ToReal(b(e)),
            _ => return Err("an interpolation at a value that is not a number".into()),
        };
        Ok(Some(Val::S(E::Spline(id, b(xe)), Ty::Real)))
    }

    /// Data known at compile time (decision LC.7): a string, an association, an interpolation or a solution of
    /// `n_dsolve` written in the body, the value of a global holding one (the read recorded, so that redefining
    /// the global invalidates the code), or `part` of such data by constant keys or indices (folded); `None`
    /// otherwise.
    fn data_term(&mut self, t: T<'h>) -> Option<T<'h>> {
        let h = self.h;
        match h.view(t) {
            View::Str(_) => Some(t),
            View::Sym(g) if self.lookup(g).is_none() && !self.prims.contains_key(&g) && self.constant(g).is_none() => {
                let v = self.src.constant(g)?;
                let d = self.data_term(v).filter(|&d| d == v)?;
                if !self.deps.contains(&g) {
                    self.deps.push(g);
                }
                Some(d)
            }
            View::App { head, args } => match h.view(head) {
                View::Sym(s) => match self.name(s).as_str() {
                    "assoc" | "interpolating_function" | "interpolated" => Some(t),
                    "list" if args.iter().any(|&a| matches!(h.view(a), View::App { .. } | View::Str(_))) => {
                        // a list holding data: each element data or a number
                        args.iter().all(|&a| self.closed_value(a)).then_some(t)
                    }
                    "part" => self.fold_part(t),
                    _ => None,
                },
                _ => None,
            },
            _ => None,
        }
    }

    /// Whether `t` is a number or data (a list element that may be put in the body as it is).
    fn closed_value(&mut self, t: T<'h>) -> bool {
        match self.h.view(t) {
            View::Int(_) | View::BigInt { .. } | View::Real(_) | View::Rational { .. } | View::Str(_) => true,
            _ => self.data_term(t) == Some(t),
        }
    }

    /// `part(d, k1, k2, ...)` of data `d` by constant keys (an association's) or indices (a list's): the element,
    /// folded at compile time; `None` when `t` is no such call or a key is missing (the interpreter's failure).
    fn fold_part(&mut self, t: T<'h>) -> Option<T<'h>> {
        let h = self.h;
        let (Prim::Part, [base, idx @ ..]) = self.prim_of(t)? else { return None };
        let mut d = self.data_term(*base)?;
        for &k in idx {
            let View::App { head, args } = h.view(d) else { return None };
            let View::Sym(s) = h.view(head) else { return None };
            d = match (self.name(s).as_str(), h.view(k)) {
                ("assoc", View::Str(_)) => args.iter().find_map(|&r| match h.view(r) {
                    View::App { args: &[key, v], .. } if key == k => Some(v),
                    _ => None,
                })?,
                ("list", View::Int(i)) => {
                    let n = args.len() as i64;
                    let at = match i {
                        1.. if i <= n => i - 1,
                        ..=-1 if -i <= n => n + i,
                        _ => return None,
                    };
                    args[at as usize]
                }
                _ => return None,
            };
        }
        Some(d)
    }

    /// The value of the global `g` when it is a number (a machine real, an integer, an exact rational), the read
    /// recorded so that redefining `g` invalidates the code (decision LC.8: inlined functions read global
    /// constants as the interpreter does).
    fn global_number(&mut self, g: Sym) -> Option<T<'h>> {
        let v = self.src.constant(g)?;
        if !matches!(self.h.view(v), View::Int(_) | View::Real(_) | View::Rational { .. }) {
            return None;
        }
        if !self.deps.contains(&g) {
            self.deps.push(g);
        }
        Some(v)
    }

    /// The symbols occurring in `t`, each once.
    fn symbols(&self, t: T<'h>) -> Vec<Sym> {
        let mut out = vec![];
        let mut seen = std::collections::HashSet::new();
        let mut stack = vec![t];
        while let Some(t) = stack.pop() {
            if !seen.insert(t) {
                continue;
            }
            match self.h.view(t) {
                View::Sym(s) => out.push(s),
                View::App { head, args } => {
                    stack.push(head);
                    stack.extend_from_slice(args);
                }
                _ => {}
            }
        }
        out
    }

    /// `body` with each symbol of `subst` replaced by its term (the data an inlined function was given).
    fn substitute(&self, body: T<'h>, subst: &[(Sym, T<'h>)]) -> T<'h> {
        fn go<'h>(h: &'h Heap, t: T<'h>, subst: &[(Sym, T<'h>)], seen: &mut HashMap<T<'h>, T<'h>>) -> T<'h> {
            if let Some(&r) = seen.get(&t) {
                return r;
            }
            let r = match h.view(t) {
                View::Sym(s) => subst.iter().find(|p| p.0 == s).map_or(t, |p| p.1),
                View::App { head, args } => {
                    let nh = go(h, head, subst, seen);
                    let na: Vec<T<'h>> = args.iter().map(|&a| go(h, a, subst, seen)).collect();
                    if nh == head && na.iter().zip(args).all(|(x, y)| x == y) {
                        t
                    } else {
                        h.app(nh, &na)
                    }
                }
                _ => t,
            };
            seen.insert(t, r);
            r
        }
        go(self.h, body, subst, &mut HashMap::new())
    }

    /// `derivative(n)(f)` where `f` (data: the value of a global, an element of an association, ...) is an interpolation or a
    /// solution of `n_dsolve`: the term mc-calculus' `DerivativeAt` makes of it (decision C.6: the order added as
    /// the fourth argument of `interpolated(...)`, or as primes on the name of `interpolating_function(...)`).
    fn derivative_head(&mut self, head: T<'h>) -> Option<T<'h>> {
        let h = self.h;
        let View::App { head: d, args: &[f] } = h.view(head) else { return None };
        let View::App { head: dsym, args: &[n] } = h.view(d) else { return None };
        if !matches!(h.view(dsym), View::Sym(s) if self.name(s) == "derivative") {
            return None;
        }
        let View::Int(n @ 0..=1000) = h.view(n) else { return None };
        // a global's read is recorded by `data_term`
        let f = self.data_term(f)?;
        let View::App { head: fh, args: data } = h.view(f) else { return None };
        let View::Sym(fs) = h.view(fh) else { return None };
        let target = match (self.name(fs).as_str(), data) {
            ("interpolated", &[xs, ys, ms]) => h.app(fh, &[xs, ys, ms, h.int(n)]),
            ("interpolated", &[xs, ys, ms, k]) => match h.view(k) {
                View::Int(k) => h.app(fh, &[xs, ys, ms, h.int(k.checked_add(n)?)]),
                _ => return None,
            },
            ("interpolating_function", [name, rest @ ..]) => {
                let View::Str(name) = h.view(*name) else { return None };
                let mut items = vec![h.string(&format!("{name}{}", "'".repeat(n as usize)))];
                items.extend_from_slice(rest);
                h.app(fh, &items)
            }
            _ => return None,
        };
        Some(target)
    }

    /// A machine-real argument of a library call, and whether it is inexact: a machine real, an integer
    /// (converted, declining beyond 2^53 where the conversion would round), or a rational literal (rounded once, as
    /// the interpreter rounds exact arguments of a machine computation).
    fn real_arg_kind(&mut self, t: T<'h>) -> Res<(E, bool)> {
        if let View::Rational { .. } = self.h.view(t) {
            return match mc_arith::Arith::new(self.h).num(t) {
                Some(Num::Real(r)) => Ok((E::Real(r.to_f64()), false)),
                _ => Err("a malformed rational".into()),
            };
        }
        match self.value(t)? {
            Val::S(x, Ty::Real) => Ok((x, true)),
            Val::S(x, Ty::Int) => Ok((E::ToReal(b(x)), false)),
            Val::S(_, Ty::Bool) => Err("a boolean argument of a numeric function".into()),
            Val::C(..) => Err("a complex argument of a library function".into()),
            Val::L(_) | Val::V(..) => Err("a list argument of a library function (it would thread)".into()),
        }
    }

    fn real_arg(&mut self, t: T<'h>) -> Res<E> {
        self.real_arg_kind(t).map(|(e, _)| e)
    }

    /// The library function `id` ([`crate::callee`]) at `args` (round 13, decision LC.1). With `special`, one
    /// argument at least must be a machine real: with exact arguments only, the interpreter keeps the call exact.
    fn library_call(&mut self, id: u32, s: Sym, args: &'h [T<'h>], special: bool) -> Res<Val> {
        if args.is_empty() || args.len() > crate::callee::MAX_ARGS {
            return Err(format!("`{}` with {} arguments", self.name(s), args.len()));
        }
        let mut xs = Vec::with_capacity(args.len());
        let mut inexact = false;
        for &a in args {
            let (x, inx) = self.real_arg_kind(a)?;
            inexact |= inx;
            // each argument computed once, in order
            xs.push(self.held(x, Ty::Real));
        }
        if special && !inexact {
            return Err(format!("`{}` of exact values (the result stays exact)", self.name(s)));
        }
        let v = self.temp(E::Call(id, xs), Ty::Real);
        Ok(Val::S(v, Ty::Real))
    }

    /// `if c { decline }`: the interpreter evaluates the call instead (an exact `0 quotient 0`, whose zero divisor
    /// declines).
    fn decline_if(&mut self, c: E) {
        let never = E::I(IOp::Quot, b(E::Int(0)), b(E::Int(0)));
        self.emit(S::If(c, vec![S::Eval(never)], vec![]));
    }

    /// `sign(x)`: the interpreter's exact -1, 0 or 1 (mc-numeric); a NaN leaves the call.
    fn sign(&mut self, s: Sym, args: &'h [T<'h>]) -> Res<(E, Ty)> {
        self.arity(s, args, 1)?;
        let (x, t) = self.expr(args[0])?;
        if t == Ty::Bool {
            return Err("`sign` of a boolean".into());
        }
        let x = self.held(x, t);
        if t == Ty::Real {
            self.decline_if(E::Cmp(Cc::Ne, b(x.clone()), b(x.clone())));
        }
        let zero = if t == Ty::Int { E::Int(0) } else { E::Real(0.0) };
        let pos = E::Cmp(Cc::Gt, b(x.clone()), b(zero.clone()));
        let neg = E::Cmp(Cc::Lt, b(x), b(zero));
        Ok((E::Sel(b(pos), b(E::Int(1)), b(E::Sel(b(neg), b(E::Int(-1)), b(E::Int(0))))), Ty::Int))
    }

    /// `unit_step(x, ...)` (the exact 1 when every x >= 0, else 0) or `heaviside_theta(x, ...)` (0 when some x < 0,
    /// 1 when every x > 0, otherwise the call stays): the interpreter's exact integers (mc-numeric), declining
    /// where it leaves the call (a NaN, or a zero for `heaviside_theta`).
    fn step(&mut self, heaviside: bool, s: Sym, args: &'h [T<'h>]) -> Res<(E, Ty)> {
        if args.is_empty() {
            return Err(format!("`{}` without arguments", self.name(s)));
        }
        let mut xs = vec![];
        for &a in args {
            let (x, t) = self.expr(a)?;
            if t == Ty::Bool {
                return Err(format!("`{}` of a boolean", self.name(s)));
            }
            xs.push((self.held(x, t), t));
        }
        // a NaN argument is not decided by the interpreter's comparisons: the call stays
        for (x, t) in &xs {
            if *t == Ty::Real {
                self.decline_if(E::Cmp(Cc::Ne, b(x.clone()), b(x.clone())));
            }
        }
        let all = |fe: &mut Self, c: Cc| -> E {
            let zero = |t: Ty| if t == Ty::Int { E::Int(0) } else { E::Real(0.0) };
            let mut acc: Option<E> = None;
            for (x, t) in &xs {
                let one = E::Cmp(c, b(x.clone()), b(zero(*t)));
                acc = Some(match acc {
                    None => one,
                    Some(a) => E::Logic(BOp::And, b(a), b(one)),
                });
            }
            // invariant: at least one argument (checked above)
            let e = acc.expect("an argument");
            fe.held(e, Ty::Bool)
        };
        let none_negative = all(self, Cc::Ge);
        if !heaviside {
            return Ok((E::Sel(b(none_negative), b(E::Int(1)), b(E::Int(0))), Ty::Int));
        }
        // heaviside: no argument negative but not all positive (a zero among them) leaves the call
        let all_positive = all(self, Cc::Gt);
        self.decline_if(E::Logic(BOp::And, b(none_negative), b(E::Not(b(all_positive.clone())))));
        Ok((E::Sel(b(all_positive), b(E::Int(1)), b(E::Int(0))), Ty::Int))
    }

    /// `piecewise([[v1, c1], [v2, c2], ...], default)` as the nested `if c1 then v1 else if c2 ...` the native
    /// evaluates (mc-lists: the first true condition's value; the default, or the exact 0 without one).
    fn piecewise(&mut self, args: &'h [T<'h>]) -> Res<Val> {
        let h = self.h;
        let (cases, default) = match args {
            [c] => (*c, h.int(0)),
            [c, d] => (*c, *d),
            _ => return Err("a malformed `piecewise`".into()),
        };
        let list = h.sym("list");
        let is_list = |t: T<'h>| match h.view(t) {
            View::App { head, args } if matches!(h.view(head), View::Sym(s) if s == list) => Some(args),
            _ => None,
        };
        let items = is_list(cases).ok_or("`piecewise` with cases that are not a list")?;
        let mut term = default;
        for &case in items.iter().rev() {
            let Some(&[v, c]) = is_list(case) else { return Err("a `piecewise` case that is not a pair [value, condition]".into()) };
            term = h.call(h.sym("if"), &[c, v, term]);
        }
        self.value(term)
    }

    fn draw_real_arg(&mut self, v: Val) -> Res<E> {
        match v {
            Val::S(x, Ty::Real) => Ok(x),
            Val::S(x, Ty::Int) => Ok(E::ToReal(b(x))),
            _ => Err("a random draw with a range that is not of numbers".into()),
        }
    }

    /// `random_real(...)`, `random_normal(...)`, `random_integer(...)` (decision N4b.1): the arguments, then the draw
    /// as a statement of its own.
    fn draw(&mut self, d: Draw, args: &'h [T<'h>]) -> Res<Val> {
        if !self.src.draws() {
            return Err(format!("`{}`: a random draw (only compiled functions of a session draw)", d.name()));
        }
        let pair = |fe: &mut Self, t: T<'h>| -> Res<(Val, Val)> {
            Ok(match fe.value(t)? {
                Val::L(items) => match <[Val; 2]>::try_from(items) {
                    Ok([x, y]) => (x, y),
                    Err(_) => return Err("a random draw with a range of other than two numbers".into()),
                },
                v => {
                    // `b` alone: [0, b]
                    let zero = if d == Draw::Integer { Val::S(E::Int(0), Ty::Int) } else { Val::S(E::Real(0.0), Ty::Real) };
                    (zero, v)
                }
            })
        };
        let (a, c) = match (d, args) {
            (Draw::Real | Draw::Normal, []) => (E::Real(0.0), E::Real(1.0)),
            (Draw::Real, [r]) => {
                let (x, y) = pair(self, *r)?;
                (self.draw_real_arg(x)?, self.draw_real_arg(y)?)
            }
            (Draw::Integer, [r]) => match pair(self, *r)? {
                (Val::S(x, Ty::Int), Val::S(y, Ty::Int)) => (x, y),
                _ => return Err("`random_integer` with a range that is not of integers".into()),
            },
            (Draw::Normal, [mu, sigma]) => {
                let mu = self.value(*mu)?;
                let mu = self.draw_real_arg(mu)?;
                let sigma = self.value(*sigma)?;
                (mu, self.draw_real_arg(sigma)?)
            }
            _ => return Err(format!("`{}` with a count or other arguments (a list of draws)", d.name())),
        };
        let t = if d == Draw::Integer { Ty::Int } else { Ty::Real };
        let v = self.temp(E::Draw(d, b(a), b(c)), t);
        Ok(Val::S(v, t))
    }

    /// `table(e, n)`, `table(e, k, n)` and `table(e, k, v)` (package P, decision P.4): a new vector of the values of
    /// `e`, a machine number of one type, computed in order (`k` from 1 to `n`, or each element of the vector `v`):
    /// the library's order. `table(e, n)` with `n < 0` and counts beyond [`crate::rt::MAX_VECTOR`] are the
    /// library's failures, so the compiled call declines there; `table(e, k, n)` with `n <= 0` is the empty list.
    fn table(&mut self, args: &'h [T<'h>]) -> Res<Val> {
        // the body, the name bound in it, the vector whose elements it takes (if any), and the count
        let (e, binder, over, count) = match args {
            [e, n] => {
                let (n, t) = self.expr(*n)?;
                let n = int(n, t).map_err(|_| "a `table` count that is not an integer".to_string())?;
                (*e, None, None, n)
            }
            [e, k, r] => {
                let View::Sym(k) = self.h.view(*k) else { return Err("a `table` iterator that is not a name".into()) };
                match self.value(*r)? {
                    Val::S(n, Ty::Int) => {
                        // 1, ..., n: no values for n <= 0
                        let n = self.held(n, Ty::Int);
                        let neg = E::Cmp(Cc::Lt, b(n.clone()), b(E::Int(0)));
                        (*e, Some(k), None, E::Sel(b(neg), b(E::Int(0)), b(n)))
                    }
                    Val::V(base, len, t) => (*e, Some(k), Some((base, len.clone(), t)), len),
                    _ => return Err("a `table` over something other than a count or a vector".into()),
                }
            }
            _ => return Err("`table` with nested iterators or other arguments (a matrix does not compile)".into()),
        };
        let len = self.temp(count, Ty::Int);
        let base = self.var(Ty::Int);
        let i = self.var(Ty::Int);
        let (body, t) = self.stmts(|fe| {
            fe.scoped(|fe| {
                match (binder, over) {
                    (Some(k), None) => fe.bind(k, i, Ty::Int, false),
                    (Some(k), Some((vb, vl, vt))) => {
                        let x = fe.var(vt);
                        fe.emit(S::Set(x, E::Elem(vt, b(vb), b(vl), b(E::Var(i)))));
                        fe.bind(k, x, vt, false);
                    }
                    (None, _) => {}
                }
                let (x, t) = match fe.value(e)? {
                    Val::S(_, Ty::Bool) => return Err("a `table` of booleans".into()),
                    Val::S(x, t) => (x, t),
                    _ => return Err("a `table` whose values are not machine numbers (lists and complex values do not compile)".into()),
                };
                let at = E::I(IOp::Sub, b(E::Var(i)), b(E::Int(1)));
                fe.emit(S::Store(E::Var(base), len.clone(), at, x));
                Ok(t)
            })
        })?;
        self.emit(S::VNew(base, len.clone()));
        self.emit(S::For { var: i, lo: E::Int(1), hi: len.clone(), step: E::Int(1), body });
        Ok(Val::V(E::Var(base), len, t))
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

    /// `if c then a else b` as a value (lists of one shape on both sides: one temporary per leaf).
    fn if_value(&mut self, c: T<'h>, x: T<'h>, y: T<'h>) -> Res<Val> {
        let c = self.boolean(c)?;
        let (mut sa, va) = self.stmts(|fe| fe.value(x))?;
        let (mut sb, vb) = self.stmts(|fe| fe.value(y))?;
        let shape = va.shape();
        if shape != vb.shape() {
            return Err(format!("`if` branches of types {} and {}", shape.name(), vb.shape().name()));
        }
        let mut leaves = vec![];
        for ((ea, ta), (eb, _)) in va.leaves().into_iter().zip(vb.leaves()) {
            let t = self.var(ta);
            sa.push(S::Set(t, ea));
            sb.push(S::Set(t, eb));
            leaves.push(E::Var(t));
        }
        self.emit(S::If(c, sa, sb));
        Ok(Val::rebuild(&shape, &mut leaves.into_iter()))
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
    fn block_value(&mut self, items: &'h [T<'h>]) -> Res<Val> {
        let Some((last, init)) = items.split_last() else { return Err("an empty block used as a value (null)".into()) };
        for &s in init {
            self.stmt(s)?;
        }
        if let Some((Prim::Let | Prim::Var, _)) = self.prim_of(*last) {
            return Err("a block ending in a binding used as a value (null)".into());
        }
        self.value(*last)
    }

    /// An inlined call of another function of the subset.
    fn inline(&mut self, f: Sym, args: &'h [T<'h>]) -> Res<Val> {
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
        // the arguments, in order: scalars in fresh variables, vectors as they are (their address and length are
        // variables or constants already)
        let mut vals = vec![];
        // untyped parameters given data (an association, a string, an interpolation, a solution): the data itself is
        // put in the body, where `part` of it and calls of it are folded at compile time (decision LC.7)
        let mut data: Vec<(Sym, T<'h>)> = vec![];
        for (&a, &(s, p)) in args.iter().zip(&def.params) {
            if p == Param::Vector {
                if let Some(d) = self.data_term(a) {
                    data.push((s, d));
                    vals.push(None);
                    continue;
                }
            }
            vals.push(Some(match p {
                Param::Scalar(pt) => {
                    let (x, tx) = self.expr(a)?;
                    if tx != pt {
                        return Err(format!("`{name}` called with a {} value for a {} parameter", tx.name(), pt.name()));
                    }
                    let v = self.var(pt);
                    self.emit(S::Set(v, x));
                    Val::S(E::Var(v), pt)
                }
                // an untyped parameter of an inlined function takes the argument's own value (the interpreter binds
                // whatever it is given): a number in a fresh variable, a complex number or a list held once, a
                // vector as it is (round 13)
                Param::Vector => match self.value(a)? {
                    v @ Val::V(..) => v,
                    Val::S(_, Ty::Bool) => return Err(format!("`{name}` called with a boolean")),
                    Val::S(x, t) => {
                        let v = self.var(t);
                        self.emit(S::Set(v, x));
                        Val::S(E::Var(v), t)
                    }
                    other => self.hold(other),
                },
            }));
        }
        // the data may name the caller's locals (a parameter, a lifted constant): they get fresh names bound in the
        // callee to the same variables, since the callee's body sees only its own parameters
        let mut carried: Vec<(Sym, Bind)> = vec![];
        for d in data.iter_mut() {
            let mut renames: Vec<(Sym, T<'h>)> = vec![];
            for x in self.symbols(d.1) {
                if let Some(bd) = self.lookup(x) {
                    self.carried += 1;
                    let fresh = self.h.sym(&format!("jit\u{b7}carried\u{b7}{}", self.carried));
                    renames.push((x, self.h.sym_term(fresh)));
                    carried.push((fresh, bd));
                }
            }
            if !renames.is_empty() {
                d.1 = self.substitute(d.1, &renames);
            }
        }
        let body = if data.is_empty() { def.body } else { self.substitute(def.body, &data) };
        if !self.deps.contains(&f) {
            self.deps.push(f);
        }
        // the callee's body sees only its parameters
        let saved = self.base;
        let saved_epoch = self.new_epoch();
        self.scopes.push(vec![]);
        self.base = self.scopes.len() - 1;
        for (fresh, bd) in carried {
            // invariant: the callee's scope was pushed just above
            self.scopes.last_mut().expect("a scope").push((fresh, bd));
        }
        for (&(s, _), v) in def.params.iter().zip(vals) {
            let Some(v) = v else { continue };
            match v {
                Val::S(E::Var(v), t) => self.bind(s, v, t, false),
                Val::V(base, len, t) => self.bind_vector(s, base, len, t),
                Val::C(E::Var(re), E::Var(im)) => self.bind_complex(s, re, im, false),
                Val::L(items) => self.bind_list(s, items),
                // a complex number held in constants: in variables
                Val::C(a, b) => {
                    let (re, im) = (self.var(Ty::Real), self.var(Ty::Real));
                    self.emit(S::Set(re, a));
                    self.emit(S::Set(im, b));
                    self.bind_complex(s, re, im, false)
                }
                // invariant: scalars are bound in fresh variables above
                Val::S(..) => unreachable!("a scalar argument in a variable"),
            }
        }
        self.stack.push(f);
        let r = self.value(body);
        self.stack.pop();
        self.scopes.pop();
        self.base = saved;
        self.epoch = saved_epoch;
        r
    }

    // --- statements ----------------------------------------------------------------------------------------------

    fn stmt(&mut self, t: T<'h>) -> Res<()> {
        self.tick()?;
        let Some((p, args)) = self.prim_of(t) else { return self.eval_stmt(t) };
        match (p, args) {
            (Prim::Let | Prim::Var, [lhs, v]) => {
                let (x, declared) = self.binder(*lhs)?;
                let (e, ty) = match self.value(*v)? {
                    Val::S(e, ty) => (e, ty),
                    Val::C(a, bb) => {
                        // a complex local: two machine-real variables (decision N4b.3)
                        if declared.is_some() {
                            return Err(format!("`{}` holds a complex number and is declared with a type", self.name(x)));
                        }
                        let (re, im) = (self.var(Ty::Real), self.var(Ty::Real));
                        self.emit(S::Set(re, a));
                        self.emit(S::Set(im, bb));
                        self.bind_complex(x, re, im, p == Prim::Var);
                        return Ok(());
                    }
                    vector @ Val::V(..) => {
                        // a vector local is bound once (package P)
                        if declared.is_some() || p == Prim::Var {
                            return Err(format!("`{}` holds a vector and is declared with a type or as a `var`", self.name(x)));
                        }
                        // invariant: `hold` keeps a vector a vector
                        let Val::V(base, len, t) = self.hold(vector) else { unreachable!("a vector") };
                        self.bind_vector(x, base, len, t);
                        return Ok(());
                    }
                    list => {
                        // a list is bound once, its leaves in temporaries (decision N0.3)
                        if declared.is_some() || p == Prim::Var {
                            return Err(format!("`{}` holds a list and is declared with a type or as a `var`", self.name(x)));
                        }
                        let list = self.hold(list);
                        // invariant: `hold` keeps a list a list
                        let Val::L(items) = list else { unreachable!("a list") };
                        self.bind_list(x, items);
                        return Ok(());
                    }
                };
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
            _ => self.eval_stmt(t),
        }
    }

    /// A value computed for its declines only.
    fn eval_stmt(&mut self, t: T<'h>) -> Res<()> {
        for (x, _) in self.value(t)?.leaves() {
            self.emit(S::Eval(x));
        }
        Ok(())
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
        let r = self.assign_inner(args);
        // values computed before may read the variable assigned
        self.forget();
        r
    }

    fn assign_inner(&mut self, args: &'h [T<'h>]) -> Res<()> {
        let [x, v] = args else { return Err("a malformed assignment".into()) };
        let View::Sym(x) = self.h.view(*x) else { return Err("an assignment to a compound target".into()) };
        let Some(bd) = self.lookup(x) else { return Err(format!("an assignment to the free name `{}`", self.name(x))) };
        if !bd.mutable {
            return Err(format!("an assignment to `{}`, which is not a `var`", self.name(x)));
        }
        if let Some((re, im)) = bd.cplx {
            let Val::C(a, bb) = self.value(*v)? else {
                return Err(format!("`{}` holds a complex number and is assigned another value", self.name(x)));
            };
            // both parts computed before either is assigned (each may read the other)
            let (a, bb) = (self.temp(a, Ty::Real), self.temp(bb, Ty::Real));
            self.emit(S::Set(re, a));
            self.emit(S::Set(im, bb));
            return Ok(());
        }
        let (e, ty) = self.expr(*v)?;
        if ty != bd.ty {
            return Err(format!("`{}` holds a {} value and is assigned a {} value", self.name(x), bd.ty.name(), ty.name()));
        }
        self.emit(S::Set(bd.var, e));
        Ok(())
    }

    fn while_loop(&mut self, c: T<'h>, body: T<'h>) -> Res<()> {
        // the body may assign what values computed before the loop read
        self.forget();
        let r = self.while_inner(c, body);
        self.forget();
        r
    }

    fn while_inner(&mut self, c: T<'h>, body: T<'h>) -> Res<()> {
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
        // the body may assign what values computed before the loop read
        self.forget();
        let r = self.for_inner(x, range, body);
        self.forget();
        r
    }

    fn for_inner(&mut self, x: T<'h>, range: T<'h>, body: T<'h>) -> Res<()> {
        let View::Sym(x) = self.h.view(x) else { return Err("a `for` over a pattern".into()) };
        let Some((Prim::Range, bounds)) = self.prim_of(range) else {
            // the elements of a vector, in order (package P)
            let Val::V(base, len, t) = self.value(range)? else {
                return Err("a `for` loop over something other than `range(...)` or a vector".into());
            };
            let i = self.var(Ty::Int);
            let (body, ()) = self.stmts(|fe| {
                fe.scoped(|fe| {
                    let e = fe.var(t);
                    fe.emit(S::Set(e, E::Elem(t, b(base), b(len.clone()), b(E::Var(i)))));
                    fe.bind(x, e, t, false);
                    fe.stmt(body)
                })
            })?;
            self.emit(S::For { var: i, lo: E::Int(1), hi: len, step: E::Int(1), body });
            return Ok(());
        };
        let mut v = vec![];
        for &a in bounds {
            let (e, t) = self.expr(a)?;
            v.push(int(e, t).map_err(|_| "a `range` with a bound that is not an integer".to_string())?);
        }
        let (lo, hi, step) = match v.as_slice() {
            [hi] => (E::Int(1), hi.clone(), E::Int(1)),
            [lo, hi] => (lo.clone(), hi.clone(), E::Int(1)),
            [lo, hi, step] => (lo.clone(), hi.clone(), step.clone()),
            _ => return Err("a `range` with other than one to three arguments".into()),
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
