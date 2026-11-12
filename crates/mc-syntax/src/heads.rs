//! The head symbols that lowering produces and the printer recognises (04-syntax.md section 12, `mc_api::names`).
//! Each head's argument shape is documented on its variant; the printer falls back to plain call syntax for any
//! term that does not have exactly that shape, so every term prints faithfully.
use mc_api::{names, Store, Sym, View, T};
use std::collections::HashMap;

macro_rules! heads {
    ($($(#[$doc:meta])* $v:ident = $name:expr,)*) => {
        /// A head with a meaning in the concrete syntax.
        #[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
        pub enum Head { $($(#[$doc])* $v,)* }

        impl Head {
            pub const ALL: &'static [Head] = &[$(Head::$v,)*];
            /// The symbol name of the head.
            pub fn name(self) -> &'static str {
                match self { $(Head::$v => $name,)* }
            }
        }
    };
}

heads! {
    /// `list(a, ...)`: `[a, ...]`
    List = names::LIST,
    /// `add(a, b, ...)`: `a + b`, with `a - b` for `add(a, mul(-1, b))` and for negative literals.
    Add = names::ADD,
    /// `mul(a, b, ...)`: `a*b`, with `a/b` for `mul(a, pow(b, -1))`; `mul(-1, a)` is `-a`.
    Mul = names::MUL,
    /// `pow(a, b)`: `a^b`
    Pow = names::POW,
    /// `complex(0, k)` for the literal `kim`.
    Complex = names::COMPLEX,
    /// `rule(a, b)`: `a -> b`
    Rule = names::RULE,
    /// `rule_delayed(a, b)`: `a :> b`
    RuleDelayed = names::RULE_DELAYED,
    /// `set(lhs, v)`: `lhs = v` (a definition)
    Set = "set",
    /// `set_delayed(lhs, e)`: `lhs := e`
    SetDelayed = "set_delayed",
    /// `assign(x, v)`: `x = v` for a `var` local `x`
    Assign = "assign",
    /// `condition(p, c)`: `p when c`
    Condition = "condition",
    /// `alternatives(p, q, ...)`: `p | q`
    Alternatives = "alternatives",
    Or = "or",
    And = "and",
    Not = "not",
    Equal = "equal",
    Unequal = "unequal",
    Less = "less",
    LessEqual = "less_equal",
    Greater = "greater",
    GreaterEqual = "greater_equal",
    Same = "same",
    NotSame = "not_same",
    /// `part(a, i, ...)`: `a[i, ...]`
    Part = "part",
    /// `span(i, j)`: `i:j` inside an index
    Span = "span",
    /// `options(name -> v, ...)`: trailing named arguments `f(x, name: v)`
    Options = "options",
    /// `derivative(n)(f)`: `f'`
    Derivative = "derivative",
    /// `fn(list(params), body)`: a closure `x => body` or `&(body)`
    Fn = "fn",
    /// `block(s, ...)`: `{ s; ... }`
    Block = "block",
    /// `let(x, v)` or `let(typed(x, T), v)`: a block statement `let x = v`
    Let = "let",
    /// `var(x, v)`: `var x = v`
    Var = "var",
    /// `if(c, a)` or `if(c, a, b)`
    If = "if",
    /// `while(c, block(...))`
    While = "while",
    /// `for(x, xs, block(...))`
    For = "for",
    /// `match(e, list(rule_delayed(p, r), ...))`
    Match = "match",
    /// `any()` or `any(x)`: the patterns `_` and `x_`
    Any = "any",
    /// `seq1()` or `seq1(x)`: `__`, `x__`
    Seq1 = "seq1",
    /// `seq0()` or `seq0(x)`: `___`, `x___`; also a `fn` rest parameter `x...`
    Seq0 = "seq0",
    /// `typed(p, T)`: `p: T` (a typed pattern or parameter)
    Typed = "typed",
    /// `named(x, p)`: `x @ p`
    Named = "named",
    /// `optional(p, d)`: `p = d` in an argument list or parameter list
    Optional = "optional",
    /// `held(p)`: a `held` parameter
    Held = "held",
    /// `rest(x)`: a lambda rest parameter `x...`
    Rest = "rest",
    /// `ball_literal(text, digits)`: the ball literal `text~digits` (decision C.6)
    BallLiteral = "ball_literal",
    /// `define_fn(lhs, body, list(options), result_type | null, list(decorators))`
    DefineFn = "define_fn",
    /// `define_rule(rule(lhs, rhs) | rule_delayed(lhs, rhs), list(decorators))`
    DefineRule = "define_rule",
    /// `define_sym(name, list(decorators))`
    DefineSym = "define_sym",
    /// `module(name, list(statements))`
    Module = "module",
    /// `public(statement)`: `pub` in a module
    Public = "public",
    /// `import(path)` or `import(path, list(names))`
    Import = "import",
    /// `import_except(path, list(names))`
    ImportExcept = "import_except",
    /// `fun_type(list(T, ...), R)`: the type `(T, ...) -> R`
    FunType = "fun_type",
    /// `type_union(T, U, ...)`: the type `T | U`
    TypeUnion = "type_union",
    /// `type_where(T, predicate)`: the type `T where predicate`
    TypeWhere = "type_where",
}

/// The head symbols of one store, and the symbol `null`.
pub struct Heads {
    syms: HashMap<Head, Sym>,
    by_sym: HashMap<Sym, Head>,
    pub null: Sym,
}

impl Heads {
    pub fn new<S: Store + ?Sized>(store: &S) -> Heads {
        let mut syms = HashMap::new();
        let mut by_sym = HashMap::new();
        for &h in Head::ALL {
            let s = store.sym(h.name());
            syms.insert(h, s);
            by_sym.insert(s, h);
        }
        Heads { syms, by_sym, null: store.sym(names::NULL) }
    }

    pub fn sym(&self, h: Head) -> Sym {
        self.syms[&h]
    }

    /// The head of `t` when `t` is a call of a known head symbol, with its arguments.
    pub fn classify<'h, S: Store + ?Sized>(&self, store: &'h S, t: T<'h>) -> Option<(Head, &'h [T<'h>])> {
        match store.view(t) {
            View::App { head, args } => match store.view(head) {
                View::Sym(s) => self.by_sym.get(&s).map(|&h| (h, args)),
                _ => None,
            },
            _ => None,
        }
    }

    /// Whether `t` is `h(...)` with exactly `n` arguments; returns them.
    pub fn is<'h, S: Store + ?Sized>(&self, store: &'h S, t: T<'h>, h: Head, n: usize) -> Option<&'h [T<'h>]> {
        match self.classify(store, t) {
            Some((x, args)) if x == h && args.len() == n => Some(args),
            _ => None,
        }
    }
}

/// The symbol of a term that is a symbol.
pub fn as_sym<S: Store + ?Sized>(store: &S, t: T<'_>) -> Option<Sym> {
    match store.view(t) {
        View::Sym(s) => Some(s),
        _ => None,
    }
}
