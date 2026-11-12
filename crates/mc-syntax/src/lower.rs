//! Lowering (04-syntax.md section 12): the syntax tree to core terms, through any [`Store`].
//!
//! The term shapes are documented on [`Head`]. Choices that go beyond 04's table are numbered decisions in the
//! crate notes (C.1 to C.14); the ones visible here:
//! - operator chains lower to one n-ary term: `a + b - c` is `add(a, b, mul(-1, c))`, `a*b/c` is
//!   `mul(a, b, pow(c, -1))`; parentheses keep their grouping: `(a + b) + c` is `add(add(a, b), c)`;
//! - a minus sign applied directly to a numeric literal gives the negative literal (`-7`, `a - 7` is
//!   `add(a, -7)`); `p/q` with integer literals `p` (optionally negated) and `q > 0` is the exact rational (or
//!   integer) it denotes, reduced; anything else is the general form of the table;
//! - ball literals lower to `ball_literal("1.25", 40)` (the exact decimal-to-ball conversion needs `mc-arith`);
//! - a block is `block(s1, ..., sn)` with statements `let(x, v)`, `var(x, v)`, `assign(x, v)` and expressions.
use crate::ast::*;
use crate::heads::{Head, Heads};
use crate::lexer::{NumKind, NumLit, PatKind};
use crate::nat;
use crate::scope::{lambda_param_names, pattern_vars, statement_binding, Bind, Scopes};
use crate::span::{Diagnostic, Span};
use mc_api::{Store, T};

type LResult<T> = Result<T, Diagnostic>;

/// The digits and radix of an integer literal.
type IntDigits<'a> = (&'a str, u32);

/// The parts of a `fn` definition lowered in its parameter scope: result type, condition, body, body condition.
type FnParts<'h> = (T<'h>, Option<T<'h>>, T<'h>, Option<T<'h>>);

/// Lowers a parsed program to one term per statement. Fails with every error found (all statements are tried).
pub fn lower_program<'h, S: Store + ?Sized>(store: &'h S, program: &Program) -> Result<Vec<T<'h>>, Vec<Diagnostic>> {
    // Lowering recurses along the syntax tree, whose depth the parser bounds; see `stack.rs`.
    crate::stack::on_big_stack(|| lower_here(store, program))
}

/// [`lower_program`] on the current stack.
pub(crate) fn lower_here<'h, S: Store + ?Sized>(store: &'h S, program: &Program) -> Result<Vec<T<'h>>, Vec<Diagnostic>> {
    let mut lw = Lowerer::new(store);
    lw.scopes.push();
    let out = lw.stmts(&program.stmts);
    if lw.errors.is_empty() {
        Ok(out)
    } else {
        Err(lw.errors)
    }
}

struct Lowerer<'h, S: Store + ?Sized> {
    store: &'h S,
    heads: Heads,
    scopes: Scopes,
    errors: Vec<Diagnostic>,
}

/// Whether an expression is an integer literal token (not imaginary), and its digits.
fn int_lit(e: &Expr) -> Option<IntDigits<'_>> {
    match &e.kind {
        ExprKind::Num(NumLit { kind: NumKind::Int { digits, radix }, imag: false }) => Some((digits, *radix)),
        _ => None,
    }
}

/// `p/q` written with integer literals: `p` possibly negated, `q` not.
fn rational_chain(items: &[(MulOp, Expr)]) -> Option<(bool, IntDigits<'_>, IntDigits<'_>)> {
    let [(MulOp::Times, p), (MulOp::Div, q)] = items else { return None };
    let (neg, p) = match &p.kind {
        ExprKind::Neg(inner) => (true, int_lit(inner)?),
        _ => (false, int_lit(p)?),
    };
    let q = int_lit(q)?;
    if nat::from_radix(q.0, q.1).is_empty() {
        return None;
    }
    Some((neg, p, q))
}

/// The canonical name of the slot symbol for `&k`.
pub fn slot_name(k: u32) -> String {
    format!("&{k}")
}

/// `k` when `name` is the canonical slot name `&k`.
pub fn slot_index(name: &str) -> Option<u32> {
    let digits = name.strip_prefix('&')?;
    let k: u32 = digits.parse().ok()?;
    (k >= 1 && slot_name(k) == name).then_some(k)
}

impl<'h, S: Store + ?Sized> Lowerer<'h, S> {
    fn new(store: &'h S) -> Self {
        Lowerer { store, heads: Heads::new(store), scopes: Scopes::default(), errors: Vec::new() }
    }

    fn sym(&self, name: &str) -> T<'h> {
        self.store.sym_term(self.store.sym(name))
    }
    fn call(&self, h: Head, args: &[T<'h>]) -> T<'h> {
        self.store.call(self.heads.sym(h), args)
    }
    fn int(&self, v: i64) -> T<'h> {
        self.store.int(v)
    }
    fn null(&self) -> T<'h> {
        self.store.sym_term(self.heads.null)
    }

    /// A statement list in the current frame; failed statements are recorded and skipped.
    fn stmts(&mut self, stmts: &[Stmt]) -> Vec<T<'h>> {
        let mut out = Vec::new();
        for s in stmts {
            match self.stmt(s) {
                Ok(t) => out.push(t),
                Err(e) => self.errors.push(e),
            }
        }
        out
    }

    fn stmt(&mut self, s: &Stmt) -> LResult<T<'h>> {
        match &s.kind {
            StmtKind::Expr(e) => self.expr(e),
            StmtKind::Let { mutable, name, ty, value } => {
                let v = self.expr(value)?;
                let x = self.sym(name);
                let lhs = match ty {
                    Some(t) => {
                        let t = self.ty(t)?;
                        self.call(Head::Typed, &[x, t])
                    }
                    None => x,
                };
                let t = self.call(if *mutable { Head::Var } else { Head::Let }, &[lhs, v]);
                if let Some((x, b)) = statement_binding(self.store, &self.heads, t) {
                    self.scopes.bind(x, b);
                }
                Ok(t)
            }
            StmtKind::Fn { decorators, name, params, options, result, condition, body } => {
                let ps = params.iter().map(|p| self.fn_param(p)).collect::<LResult<Vec<_>>>()?;
                let os = options.iter().map(|p| self.fn_param(p)).collect::<LResult<Vec<_>>>()?;
                let f = self.sym(name);
                let call = self.store.app(f, &ps);
                let opts = self.call(Head::List, &os);
                let mut vars = pattern_vars(self.store, &self.heads, call);
                vars.extend(pattern_vars(self.store, &self.heads, opts));
                self.scopes.push_lets(vars);
                let r = self.fn_rest(result, condition, body);
                self.scopes.pop();
                let (result, cond, body, body_cond) = r?;
                let mut lhs = call;
                for c in [cond, body_cond].into_iter().flatten() {
                    lhs = self.call(Head::Condition, &[lhs, c]);
                }
                let decs = self.decorators(decorators)?;
                Ok(self.call(Head::DefineFn, &[lhs, body, opts, result, decs]))
            }
            StmtKind::Rule { decorators, lhs, delayed, rhs } => {
                let l = self.expr(lhs)?;
                self.scopes.push_lets(pattern_vars(self.store, &self.heads, l));
                let r = self.split_when(rhs);
                self.scopes.pop();
                let (b, c) = r?;
                let l = match c {
                    Some(c) => self.call(Head::Condition, &[l, c]),
                    None => l,
                };
                let rule = self.call(if *delayed { Head::RuleDelayed } else { Head::Rule }, &[l, b]);
                let decs = self.decorators(decorators)?;
                Ok(self.call(Head::DefineRule, &[rule, decs]))
            }
            StmtKind::Sym { decorators, name } => {
                let decs = self.decorators(decorators)?;
                Ok(self.call(Head::DefineSym, &[self.sym(name), decs]))
            }
            StmtKind::Module { name, body } => {
                self.scopes.push();
                let items = self.stmts(body);
                self.scopes.pop();
                let list = self.call(Head::List, &items);
                Ok(self.call(Head::Module, &[self.sym(name), list]))
            }
            StmtKind::Import { path, kind } => {
                let p = self.sym(path);
                Ok(match kind {
                    ImportKind::All => self.call(Head::Import, &[p]),
                    ImportKind::Only(names) | ImportKind::Except(names) => {
                        let ns: Vec<_> = names.iter().map(|n| self.sym(n)).collect();
                        let list = self.call(Head::List, &ns);
                        let h = if matches!(kind, ImportKind::Only(_)) { Head::Import } else { Head::ImportExcept };
                        self.call(h, &[p, list])
                    }
                })
            }
            StmtKind::Pub(inner) => {
                let t = self.stmt(inner)?;
                Ok(self.call(Head::Public, &[t]))
            }
        }
    }

    /// The parts of a `fn` definition lowered with the parameters in scope: result type (or `null`), condition,
    /// body, and a trailing `when` moved off the body.
    fn fn_rest(
        &mut self,
        result: &Option<Type>,
        condition: &Option<Expr>,
        body: &Expr,
    ) -> LResult<FnParts<'h>> {
        let result = match result {
            Some(t) => self.ty(t)?,
            None => self.null(),
        };
        let cond = condition.as_ref().map(|c| self.expr(c)).transpose()?;
        let (body, body_cond) = self.split_when(body)?;
        Ok((result, cond, body, body_cond))
    }

    /// `body when c` on the right of a definition puts the condition on the rule (04 section 6.1).
    fn split_when(&mut self, e: &Expr) -> LResult<(T<'h>, Option<T<'h>>)> {
        match &e.kind {
            ExprKind::Binary(BinOp::When, b, c) => Ok((self.expr(b)?, Some(self.expr(c)?))),
            _ => Ok((self.expr(e)?, None)),
        }
    }

    fn decorators(&mut self, ds: &[Decorator]) -> LResult<T<'h>> {
        let mut out = Vec::new();
        for d in ds {
            let s = self.sym(&d.name);
            out.push(match &d.args {
                Some(args) => {
                    let a = self.args(args, None)?;
                    self.store.app(s, &a)
                }
                None => s,
            });
        }
        Ok(self.call(Head::List, &out))
    }

    /// A `fn` parameter as a pattern: `x` is `any(x)`, `x: T` is `typed(any(x), T)`, `xs...` is `seq0(xs)`.
    fn fn_param(&mut self, p: &Param) -> LResult<T<'h>> {
        let mut t = match &p.kind {
            ParamKind::Name(x, None) => self.call(Head::Any, &[self.sym(x)]),
            ParamKind::Name(x, Some(ty)) => {
                let any = self.call(Head::Any, &[self.sym(x)]);
                let ty = self.ty(ty)?;
                self.call(Head::Typed, &[any, ty])
            }
            ParamKind::Rest(x) => self.call(Head::Seq0, &[self.sym(x)]),
            ParamKind::Pattern(e) => self.expr(e)?,
        };
        if let Some(d) = &p.default {
            let d = self.expr(d)?;
            t = self.call(Head::Optional, &[t, d]);
        }
        if p.held {
            t = self.call(Head::Held, &[t]);
        }
        Ok(t)
    }

    /// A lambda parameter: `x`, `typed(x, T)`, `rest(x)`, wrapped in `optional` and `held` as written.
    fn lambda_param(&mut self, p: &Param) -> LResult<T<'h>> {
        let mut t = match &p.kind {
            ParamKind::Name(x, None) => self.sym(x),
            ParamKind::Name(x, Some(ty)) => {
                let ty = self.ty(ty)?;
                self.call(Head::Typed, &[self.sym(x), ty])
            }
            ParamKind::Rest(x) => self.call(Head::Rest, &[self.sym(x)]),
            ParamKind::Pattern(_) => return Err(Diagnostic::new(p.span, "a lambda parameter must be a name")),
        };
        if let Some(d) = &p.default {
            let d = self.expr(d)?;
            t = self.call(Head::Optional, &[t, d]);
        }
        if p.held {
            t = self.call(Head::Held, &[t]);
        }
        Ok(t)
    }

    fn ty(&mut self, t: &Type) -> LResult<T<'h>> {
        Ok(match &t.kind {
            TypeKind::Name(n) => self.sym(n),
            TypeKind::Num(n) => self.num(n, false, t.span)?,
            TypeKind::App(n, args) => {
                let a = args.iter().map(|x| self.ty(x)).collect::<LResult<Vec<_>>>()?;
                self.store.app(self.sym(n), &a)
            }
            TypeKind::Fun(args, r) => {
                let a = args.iter().map(|x| self.ty(x)).collect::<LResult<Vec<_>>>()?;
                let list = self.call(Head::List, &a);
                let r = self.ty(r)?;
                self.call(Head::FunType, &[list, r])
            }
            TypeKind::Union(items) => {
                let a = items.iter().map(|x| self.ty(x)).collect::<LResult<Vec<_>>>()?;
                self.call(Head::TypeUnion, &a)
            }
            TypeKind::Where(t, p) => {
                let t = self.ty(t)?;
                let p = self.expr(p)?;
                self.call(Head::TypeWhere, &[t, p])
            }
        })
    }

    /// A numeric literal, negated when `neg` (the sign goes to the imaginary part of `kim`).
    fn num(&self, n: &NumLit, neg: bool, span: Span) -> LResult<T<'h>> {
        let v = match &n.kind {
            NumKind::Int { digits, radix } => self.store.big_int(neg, &nat::from_radix(digits, *radix)),
            NumKind::Real(text) => {
                let v: f64 = text.parse().map_err(|_| Diagnostic::new(span, "malformed real literal"))?;
                if !v.is_finite() {
                    return Err(Diagnostic::new(span, "real literal out of the range of machine reals; write a ball literal `...~digits`"));
                }
                self.store.real(if neg { -v } else { v })
            }
            NumKind::Ball { mantissa, digits } => {
                let d: i64 = match digits.parse() {
                    Ok(d) if d >= 1 => d,
                    _ => return Err(Diagnostic::new(span, "the precision of a ball literal is a positive number of digits")),
                };
                let text = if neg { format!("-{mantissa}") } else { mantissa.clone() };
                let s = self.store.string(&text);
                self.call(Head::BallLiteral, &[s, self.int(d)])
            }
        };
        Ok(if n.imag { self.call(Head::Complex, &[self.int(0), v]) } else { v })
    }

    /// The exact value of `p/q` (decision C.2).
    fn rational(&self, neg: bool, p: IntDigits<'_>, q: IntDigits<'_>) -> T<'h> {
        let p = nat::from_radix(p.0, p.1);
        let q = nat::from_radix(q.0, q.1);
        if p.is_empty() {
            return self.int(0);
        }
        let g = nat::gcd(&p, &q);
        let (p, q) = (nat::div(&p, &g), nat::div(&q, &g));
        let num = self.store.big_int(neg, &p);
        if nat::is_one(&q) {
            num
        } else {
            self.store.rational(num, self.store.big_int(false, &q))
        }
    }

    /// `-e` as the operand of a subtraction: literals and literal rationals are negated, anything else is
    /// `mul(-1, e)`.
    fn negated(&mut self, e: &Expr) -> LResult<T<'h>> {
        match &e.kind {
            ExprKind::Num(n) => self.num(n, true, e.span),
            ExprKind::Mul(items) => match rational_chain(items) {
                Some((neg, p, q)) => Ok(self.rational(!neg, p, q)),
                None => {
                    let v = self.expr(e)?;
                    Ok(self.call(Head::Mul, &[self.int(-1), v]))
                }
            },
            _ => {
                let v = self.expr(e)?;
                Ok(self.call(Head::Mul, &[self.int(-1), v]))
            }
        }
    }

    /// Call arguments: positional ones (with `p = d` for a pattern `p` as `optional(p, d)`), then the named ones as
    /// one trailing `options(name -> v, ...)`. A positional argument after named ones (a stored option list among
    /// the options, gap G82) ends their group: `f(a, k: 1, opts, j: 2)` is `f(a, options(k -> 1), opts,
    /// options(j -> 2))`, which the evaluator merges into one option list when `opts` is one (gap G65). `hole`
    /// replaces the one `_` argument of a pipe's right side.
    fn args(&mut self, args: &[Arg], mut hole: Option<T<'h>>) -> LResult<Vec<T<'h>>> {
        let mut out = Vec::new();
        let mut named = Vec::new();
        for a in args {
            match &a.name {
                Some((n, _)) => {
                    let v = self.expr(&a.value)?;
                    let r = self.call(Head::Rule, &[self.sym(n), v]);
                    named.push(r);
                }
                None => {
                    if !named.is_empty() {
                        out.push(self.call(Head::Options, &named));
                        named.clear();
                    }
                    if hole.is_some() && is_hole(&a.value) {
                        out.push(hole.take().unwrap_or_else(|| self.null()));
                        continue;
                    }
                    out.push(self.arg_value(&a.value)?);
                }
            }
        }
        if !named.is_empty() {
            out.push(self.call(Head::Options, &named));
        }
        Ok(out)
    }

    fn arg_value(&mut self, e: &Expr) -> LResult<T<'h>> {
        if let ExprKind::Binary(BinOp::Set, lhs, rhs) = &e.kind {
            let is_pattern = match &lhs.kind {
                ExprKind::Pattern { .. } => true,
                ExprKind::Typed(p, _) => matches!(p.kind, ExprKind::Pattern { .. }),
                _ => false,
            };
            if is_pattern {
                let p = self.expr(lhs)?;
                let d = self.expr(rhs)?;
                return Ok(self.call(Head::Optional, &[p, d]));
            }
        }
        self.expr(e)
    }

    fn expr(&mut self, e: &Expr) -> LResult<T<'h>> {
        let st = self.store;
        Ok(match &e.kind {
            ExprKind::Num(n) => self.num(n, false, e.span)?,
            ExprKind::Str(s) => st.string(s),
            ExprKind::Name(n) => self.sym(n),
            ExprKind::Pattern { name, kind } => {
                let h = match kind {
                    PatKind::Single => Head::Any,
                    PatKind::Seq1 => Head::Seq1,
                    PatKind::Seq0 => Head::Seq0,
                };
                match name {
                    Some(n) => self.call(h, &[self.sym(n)]),
                    None => self.call(h, &[]),
                }
            }
            ExprKind::Slot(k) => self.sym(&slot_name(*k)),
            ExprKind::Typed(p, t) => {
                let p = self.expr(p)?;
                let t = self.ty(t)?;
                self.call(Head::Typed, &[p, t])
            }
            ExprKind::Named(x, p) => {
                let p = self.expr(p)?;
                self.call(Head::Named, &[self.sym(x), p])
            }
            ExprKind::Neg(inner) => match &inner.kind {
                ExprKind::Num(n) => self.num(n, true, inner.span)?,
                _ => {
                    let v = self.expr(inner)?;
                    self.call(Head::Mul, &[self.int(-1), v])
                }
            },
            ExprKind::Not(a) => {
                let a = self.expr(a)?;
                self.call(Head::Not, &[a])
            }
            ExprKind::Add(items) => {
                let mut out = Vec::with_capacity(items.len());
                for (neg, x) in items {
                    out.push(if *neg { self.negated(x)? } else { self.expr(x)? });
                }
                self.call(Head::Add, &out)
            }
            ExprKind::Mul(items) => {
                if let Some((neg, p, q)) = rational_chain(items) {
                    return Ok(self.rational(neg, p, q));
                }
                let mut out = Vec::with_capacity(items.len());
                for (op, x) in items {
                    let v = self.expr(x)?;
                    out.push(match op {
                        MulOp::Times => v,
                        MulOp::Div => self.call(Head::Pow, &[v, self.int(-1)]),
                    });
                }
                self.call(Head::Mul, &out)
            }
            ExprKind::Pow(a, b) => {
                let a = self.expr(a)?;
                let b = self.expr(b)?;
                self.call(Head::Pow, &[a, b])
            }
            ExprKind::Binary(op, a, b) => self.binary(*op, a, b, e.span)?,
            ExprKind::Nary(op, items) => {
                let h = match op {
                    NaryOp::Or => Head::Or,
                    NaryOp::And => Head::And,
                    NaryOp::Alternatives => Head::Alternatives,
                };
                let a = items.iter().map(|x| self.expr(x)).collect::<LResult<Vec<_>>>()?;
                self.call(h, &a)
            }
            ExprKind::Call(f, args) => {
                let f = self.expr(f)?;
                let a = self.args(args, None)?;
                st.app(f, &a)
            }
            ExprKind::Index(base, idx) => {
                let mut a = vec![self.expr(base)?];
                for i in idx {
                    a.push(match &i.kind {
                        ExprKind::Span(x, y) => {
                            let x = self.expr(x)?;
                            let y = self.expr(y)?;
                            self.call(Head::Span, &[x, y])
                        }
                        _ => self.expr(i)?,
                    });
                }
                self.call(Head::Part, &a)
            }
            ExprKind::Span(..) => return Err(Diagnostic::new(e.span, "a span `i:j` is only allowed inside an index")),
            ExprKind::Prime(f, n) => {
                let f = self.expr(f)?;
                let d = self.call(Head::Derivative, &[self.int(*n as i64)]);
                st.app(d, &[f])
            }
            ExprKind::List(items) => {
                let a = items.iter().map(|x| self.expr(x)).collect::<LResult<Vec<_>>>()?;
                self.call(Head::List, &a)
            }
            ExprKind::Block(stmts) => {
                self.scopes.push();
                let r = stmts.iter().map(|s| self.stmt(s)).collect::<LResult<Vec<_>>>();
                self.scopes.pop();
                self.call(Head::Block, &r?)
            }
            ExprKind::Lambda(params, body) => {
                let ps = params.iter().map(|p| self.lambda_param(p)).collect::<LResult<Vec<_>>>()?;
                self.scopes.push_lets(lambda_param_names(st, &self.heads, &ps));
                let body = self.expr(body);
                self.scopes.pop();
                let list = self.call(Head::List, &ps);
                self.call(Head::Fn, &[list, body?])
            }
            ExprKind::SlotFn(body) => {
                // a slot function without slots (`&(x)`) still takes an argument and ignores it (6.7)
                let n = max_slot(body).max(1);
                let ps: Vec<_> = (1..=n).map(|k| self.sym(&slot_name(k))).collect();
                self.scopes.push_lets(ps.iter().filter_map(|&p| crate::heads::as_sym(st, p)));
                let body = self.expr(body);
                self.scopes.pop();
                let list = self.call(Head::List, &ps);
                self.call(Head::Fn, &[list, body?])
            }
            ExprKind::If(c, a, b) => {
                let mut v = vec![self.expr(c)?, self.expr(a)?];
                if let Some(b) = b {
                    v.push(self.expr(b)?);
                }
                self.call(Head::If, &v)
            }
            ExprKind::While(c, body) => {
                let c = self.expr(c)?;
                let b = self.expr(body)?;
                self.call(Head::While, &[c, b])
            }
            ExprKind::For(x, it, body) => {
                let it = self.expr(it)?;
                let x = self.sym(x);
                self.scopes.push_lets(crate::heads::as_sym(st, x));
                let b = self.expr(body);
                self.scopes.pop();
                self.call(Head::For, &[x, it, b?])
            }
            ExprKind::Match(scrutinee, arms) => {
                let s = self.expr(scrutinee)?;
                let mut out = Vec::new();
                for arm in arms {
                    let p = self.expr(&arm.pattern)?;
                    self.scopes.push_lets(pattern_vars(st, &self.heads, p));
                    let r = self.expr(&arm.body);
                    self.scopes.pop();
                    out.push(self.call(Head::RuleDelayed, &[p, r?]));
                }
                let list = self.call(Head::List, &out);
                self.call(Head::Match, &[s, list])
            }
        })
    }

    fn binary(&mut self, op: BinOp, a: &Expr, b: &Expr, span: Span) -> LResult<T<'h>> {
        let h = match op {
            BinOp::Set => return self.set(a, b, span),
            BinOp::Pipe => return self.pipe(a, b),
            BinOp::Replace => {
                let (e, r) = (self.expr(a)?, self.expr(b)?);
                let rewrite = self.store.sym_term(self.store.sym("rewrite"));
                return Ok(self.store.app(rewrite, &[e, r]));
            }
            BinOp::SetDelayed => Head::SetDelayed,
            BinOp::Rule => Head::Rule,
            BinOp::RuleDelayed => Head::RuleDelayed,
            BinOp::When => Head::Condition,
            BinOp::Equal => Head::Equal,
            BinOp::Unequal => Head::Unequal,
            BinOp::Less => Head::Less,
            BinOp::LessEqual => Head::LessEqual,
            BinOp::Greater => Head::Greater,
            BinOp::GreaterEqual => Head::GreaterEqual,
            BinOp::Same => Head::Same,
            BinOp::NotSame => Head::NotSame,
        };
        let a = self.expr(a)?;
        let b = self.expr(b)?;
        Ok(self.call(h, &[a, b]))
    }

    /// `x = v`: assignment to a `var` local, an error for an immutable local, otherwise a definition.
    fn set(&mut self, lhs: &Expr, rhs: &Expr, span: Span) -> LResult<T<'h>> {
        if let ExprKind::Name(n) = &lhs.kind {
            let s = self.store.sym(n);
            let bind = self.scopes.lookup(s);
            if bind == Some(Bind::Let) {
                return Err(Diagnostic::new(
                    lhs.span.to(span),
                    format!("cannot assign to `{n}`: it is bound by `let`, a parameter or a pattern (use `var` for a mutable local)"),
                ));
            }
            let v = self.expr(rhs)?;
            let x = self.store.sym_term(s);
            return Ok(self.call(if bind == Some(Bind::Var) { Head::Assign } else { Head::Set }, &[x, v]));
        }
        // `x[k...] = v` on a local: `let` locals are immutable; a `var` local is rebound to its value with the part
        // replaced (top-level symbols are handled by `set` at run time)
        if let ExprKind::Index(base, idx) = &lhs.kind {
            if let ExprKind::Name(n) = &base.kind {
                let s = self.store.sym(n);
                match self.scopes.lookup(s) {
                    Some(Bind::Let) => {
                        return Err(Diagnostic::new(
                            lhs.span.to(span),
                            format!("cannot assign to a part of `{n}`: it is bound by `let`, a parameter or a pattern (use `var` for a mutable local)"),
                        ));
                    }
                    Some(Bind::Var) if idx.iter().all(|i| !matches!(i.kind, ExprKind::Span(..))) => {
                        let x = self.store.sym_term(s);
                        let mut ks = Vec::with_capacity(idx.len());
                        for i in idx {
                            ks.push(self.expr(i)?);
                        }
                        let pos = if ks.len() == 1 { ks[0] } else { self.call(Head::List, &ks) };
                        let v = self.expr(rhs)?;
                        let rule = self.call(Head::Rule, &[pos, v]);
                        let rp = self.store.sym_term(self.store.sym("replace_part"));
                        let new = self.store.app(rp, &[x, rule]);
                        return Ok(self.call(Head::Assign, &[x, new]));
                    }
                    _ => {}
                }
            }
        }
        let l = self.expr(lhs)?;
        let v = self.expr(rhs)?;
        Ok(self.call(Head::Set, &[l, v]))
    }

    /// `a |> f(b, _)` is `f(b, a)`; `a |> g` is `g(a)` (04 section 3.3).
    fn pipe(&mut self, a: &Expr, rhs: &Expr) -> LResult<T<'h>> {
        let v = self.expr(a)?;
        if let ExprKind::Call(f, args) = &rhs.kind {
            let holes = args.iter().filter(|x| x.name.is_none() && is_hole(&x.value)).count();
            if holes > 1 {
                return Err(Diagnostic::new(rhs.span, "the right side of `|>` may contain only one `_` hole"));
            }
            if holes == 1 {
                let f = self.expr(f)?;
                let a = self.args(args, Some(v))?;
                return Ok(self.store.app(f, &a));
            }
        }
        let f = self.expr(rhs)?;
        Ok(self.store.app(f, &[v]))
    }
}

/// A bare `_` argument.
fn is_hole(e: &Expr) -> bool {
    matches!(e.kind, ExprKind::Pattern { name: None, kind: PatKind::Single })
}

/// The largest slot `&k` in a slot function body, not counting nested slot functions (they bind their own). Slot
/// names written in backquotes count as well, so that a printed body reads back with the same parameters.
fn max_slot(e: &Expr) -> u32 {
    let mut m = 0;
    let mut stack: Vec<&Expr> = vec![e];
    let name = |n: &str| slot_index(n).unwrap_or(0);
    while let Some(e) = stack.pop() {
        match &e.kind {
            ExprKind::Slot(k) => m = m.max(*k),
            ExprKind::Name(n) => m = m.max(name(n)),
            ExprKind::SlotFn(_) | ExprKind::Num(_) | ExprKind::Str(_) | ExprKind::Pattern { .. } => {}
            ExprKind::Typed(p, t) => {
                stack.push(p);
                m = m.max(type_max_slot(t, &mut stack));
            }
            ExprKind::Named(x, p) => {
                m = m.max(name(x));
                stack.push(p);
            }
            ExprKind::Neg(a) | ExprKind::Not(a) | ExprKind::Prime(a, _) => stack.push(a),
            ExprKind::Add(items) => stack.extend(items.iter().map(|x| &x.1)),
            ExprKind::Mul(items) => stack.extend(items.iter().map(|x| &x.1)),
            ExprKind::Pow(a, b) | ExprKind::Binary(_, a, b) | ExprKind::Span(a, b) | ExprKind::While(a, b) => {
                stack.push(a);
                stack.push(b);
            }
            ExprKind::Nary(_, items) | ExprKind::List(items) => stack.extend(items),
            ExprKind::Call(f, args) => {
                stack.push(f);
                for a in args {
                    if let Some((n, _)) = &a.name {
                        m = m.max(name(n));
                    }
                    stack.push(&a.value);
                }
            }
            ExprKind::Index(b, idx) => {
                stack.push(b);
                stack.extend(idx);
            }
            ExprKind::Block(stmts) => {
                for s in stmts {
                    if let StmtKind::Expr(x) = &s.kind {
                        stack.push(x);
                    } else if let StmtKind::Let { name: n, ty, value, .. } = &s.kind {
                        m = m.max(name(n));
                        if let Some(t) = ty {
                            m = m.max(type_max_slot(t, &mut stack));
                        }
                        stack.push(value);
                    }
                }
            }
            ExprKind::Lambda(params, body) => {
                for p in params {
                    match &p.kind {
                        ParamKind::Name(n, t) => {
                            m = m.max(name(n));
                            if let Some(t) = t {
                                m = m.max(type_max_slot(t, &mut stack));
                            }
                        }
                        ParamKind::Rest(n) => m = m.max(name(n)),
                        ParamKind::Pattern(x) => stack.push(x),
                    }
                    if let Some(d) = &p.default {
                        stack.push(d);
                    }
                }
                stack.push(body);
            }
            ExprKind::If(c, a, b) => {
                stack.push(c);
                stack.push(a);
                if let Some(b) = b {
                    stack.push(b);
                }
            }
            ExprKind::For(x, it, body) => {
                m = m.max(name(x));
                stack.push(it);
                stack.push(body);
            }
            ExprKind::Match(s, arms) => {
                stack.push(s);
                for a in arms {
                    stack.push(&a.pattern);
                    stack.push(&a.body);
                }
            }
        }
    }
    m
}

/// Slot names in a type; `where` predicates go on the expression stack.
fn type_max_slot<'a>(t: &'a Type, stack: &mut Vec<&'a Expr>) -> u32 {
    let mut m = 0;
    let mut ts = vec![t];
    while let Some(t) = ts.pop() {
        match &t.kind {
            TypeKind::Name(n) => m = m.max(slot_index(n).unwrap_or(0)),
            TypeKind::Num(_) => {}
            TypeKind::App(n, args) => {
                m = m.max(slot_index(n).unwrap_or(0));
                ts.extend(args);
            }
            TypeKind::Fun(args, r) => {
                ts.extend(args);
                ts.push(r);
            }
            TypeKind::Union(items) => ts.extend(items),
            TypeKind::Where(t, p) => {
                ts.push(t);
                stack.push(p);
            }
        }
    }
    m
}
