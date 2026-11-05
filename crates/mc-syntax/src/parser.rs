//! The parser (04-syntax.md sections 3 and 5 to 10, grammar in 13): tokens to a [`Program`].
//!
//! Recursive descent with precedence climbing over the table of 3.1 (`prec.rs`). Recursion follows the nesting of
//! the source and is capped at [`MAX_DEPTH`], so hostile input gives an error instead of a stack overflow.
//!
//! Errors carry spans; after an error the parser skips to the end of the statement (a new line or `;` outside
//! brackets, or the `}` that closes the enclosing block) and goes on, so one run reports the errors of every
//! statement.
use crate::ast::*;
use crate::lexer::{lex, NumKind, Tok, Token, P};
use crate::prec;
use crate::span::{Diagnostic, Span};

/// Maximum nesting of expressions. Deeper input is an error. Parsing and lowering recurse once per level and run on
/// a reserved stack (`stack.rs`), so the bound is what keeps them finite, whatever the caller's stack.
pub const MAX_DEPTH: u32 = 256;

/// The result of parsing: the program (statements with errors left out) and all lexical and syntax errors, in source
/// order. A program with errors must not be evaluated (04 section 11).
pub struct Parsed {
    pub program: Program,
    pub errors: Vec<Diagnostic>,
}

/// Parses a source text.
pub fn parse(src: &str) -> Parsed {
    crate::stack::on_big_stack(|| parse_here(src))
}

/// [`parse`] on the current stack.
pub(crate) fn parse_here(src: &str) -> Parsed {
    let lexed = lex(src);
    parse_tokens(&lexed.tokens, lexed.errors)
}

/// Parses lexed tokens (ending with [`Tok::Eof`]); `errors` are the lexical errors, kept in the result.
pub(crate) fn parse_tokens(toks: &[Token], errors: Vec<Diagnostic>) -> Parsed {
    let mut p = Parser { toks, pos: 0, errors, depth: 0, slot_depth: 0, no_lambda: false };
    let stmts = p.statements(Mode::Top);
    let mut errors = p.errors;
    errors.sort_by_key(|d| d.span.start);
    Parsed { program: Program { stmts }, errors }
}

type PResult<T> = Result<T, Diagnostic>;

/// Where a statement list is.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// A file or cell: definitions, modules and imports allowed.
    Top,
    /// A module body: as `Top`, plus `pub`.
    Module,
    /// A block: `let`, `var` and expressions only.
    Block,
}

struct Parser<'t> {
    toks: &'t [Token],
    pos: usize,
    errors: Vec<Diagnostic>,
    depth: u32,
    /// Nesting of slot functions `&(...)`; `&k` is only valid inside one.
    slot_depth: u32,
    /// In the pattern of a `match` arm, `x => ...` is the arm, not a lambda.
    no_lambda: bool,
}

/// An infix operator: its level and what it builds.
#[derive(Clone, Copy)]
enum Infix {
    Bin(BinOp),
    Nary(NaryOp),
    Add,
    Mul,
    Pow,
}

fn infix(tok: &Tok) -> Option<(u8, Infix)> {
    use BinOp::*;
    Some(match tok {
        Tok::Punct(p) => match p {
            P::Assign => (prec::ASSIGN, Infix::Bin(Set)),
            P::ColonAssign => (prec::ASSIGN, Infix::Bin(SetDelayed)),
            P::Pipe => (prec::PIPE, Infix::Bin(Pipe)),
            P::Replace => (prec::PIPE, Infix::Bin(Replace)),
            P::Arrow => (prec::RULE, Infix::Bin(Rule)),
            P::ColonArrow => (prec::RULE, Infix::Bin(RuleDelayed)),
            P::Bar => (prec::ALT, Infix::Nary(NaryOp::Alternatives)),
            P::Eq => (prec::CMP, Infix::Bin(Equal)),
            P::Ne => (prec::CMP, Infix::Bin(Unequal)),
            P::Lt => (prec::CMP, Infix::Bin(Less)),
            P::Le => (prec::CMP, Infix::Bin(LessEqual)),
            P::Gt => (prec::CMP, Infix::Bin(Greater)),
            P::Ge => (prec::CMP, Infix::Bin(GreaterEqual)),
            P::Same => (prec::CMP, Infix::Bin(Same)),
            P::NotSame => (prec::CMP, Infix::Bin(NotSame)),
            P::Plus | P::Minus => (prec::ADD, Infix::Add),
            P::Star | P::Slash => (prec::MUL, Infix::Mul),
            P::Caret => (prec::POW, Infix::Pow),
            _ => return None,
        },
        Tok::Juxt => (prec::MUL, Infix::Mul),
        Tok::Kw("when") => (prec::WHEN, Infix::Bin(When)),
        Tok::Kw("or") => (prec::OR, Infix::Nary(NaryOp::Or)),
        Tok::Kw("and") => (prec::AND, Infix::Nary(NaryOp::And)),
        _ => return None,
    })
}

fn is_comparison(tok: &Tok) -> bool {
    matches!(infix(tok), Some((prec::CMP, _)))
}

impl Parser<'_> {
    // ---- token access ----

    fn peek(&self) -> &Tok {
        &self.toks[self.pos].tok
    }
    fn nth(&self, n: usize) -> &Tok {
        &self.toks[(self.pos + n).min(self.toks.len() - 1)].tok
    }
    fn span(&self) -> Span {
        self.toks[self.pos].span
    }
    fn prev_span(&self) -> Span {
        self.toks[self.pos.saturating_sub(1)].span
    }
    fn bump(&mut self) -> Token {
        let t = self.toks[self.pos].clone();
        if self.pos + 1 < self.toks.len() {
            self.pos += 1;
        }
        t
    }
    fn at(&self, p: P) -> bool {
        *self.peek() == Tok::Punct(p)
    }
    fn at_kw(&self, k: &str) -> bool {
        matches!(self.peek(), Tok::Kw(x) if *x == k)
    }
    fn eat(&mut self, p: P) -> bool {
        if self.at(p) {
            self.bump();
            true
        } else {
            false
        }
    }
    fn eat_kw(&mut self, k: &str) -> bool {
        if self.at_kw(k) {
            self.bump();
            true
        } else {
            false
        }
    }
    fn skip_newlines(&mut self) {
        while *self.peek() == Tok::Newline {
            self.bump();
        }
    }
    /// The first token that is not a new line.
    fn peek_past_newlines(&self) -> &Tok {
        let mut i = self.pos;
        while self.toks[i].tok == Tok::Newline {
            i += 1;
        }
        &self.toks[i].tok
    }
    fn error_here(&self, msg: impl Into<String>) -> Diagnostic {
        Diagnostic::new(self.span(), msg)
    }
    fn found(&self) -> String {
        self.peek().describe()
    }
    fn expect(&mut self, p: P, context: &str) -> PResult<Span> {
        if self.at(p) {
            Ok(self.bump().span)
        } else {
            Err(self.error_here(format!("expected `{}` {context}, found {}", p.text(), self.found())))
        }
    }
    fn expect_kw(&mut self, k: &str, context: &str) -> PResult<()> {
        if self.eat_kw(k) {
            Ok(())
        } else {
            Err(self.error_here(format!("expected `{k}` {context}, found {}", self.found())))
        }
    }
    fn expect_ident(&mut self, what: &str) -> PResult<(String, Span)> {
        match self.peek().clone() {
            Tok::Ident(n) => {
                let s = self.bump().span;
                Ok((n, s))
            }
            t => Err(self.error_here(format!("expected {what}, found {}", t.describe()))),
        }
    }

    /// Runs `f` one nesting level deeper, failing past [`MAX_DEPTH`].
    fn nested<T>(&mut self, f: impl FnOnce(&mut Self) -> PResult<T>) -> PResult<T> {
        if self.depth >= MAX_DEPTH {
            return Err(self.error_here(format!("expression nested more than {MAX_DEPTH} levels deep")));
        }
        self.depth += 1;
        let r = f(self);
        self.depth -= 1;
        r
    }

    /// Runs `f` with `no_lambda` set to `v`, restoring it afterwards.
    fn with_no_lambda<T>(&mut self, v: bool, f: impl FnOnce(&mut Self) -> PResult<T>) -> PResult<T> {
        let saved = std::mem::replace(&mut self.no_lambda, v);
        let r = f(self);
        self.no_lambda = saved;
        r
    }

    // ---- statements ----

    /// A statement list, up to the end of input (`Top`) or up to a `}` (left unconsumed).
    fn statements(&mut self, mode: Mode) -> Vec<Stmt> {
        let closing = mode != Mode::Top;
        let mut out = Vec::new();
        loop {
            while matches!(self.peek(), Tok::Newline | Tok::Punct(P::Semi)) {
                self.bump();
            }
            match self.peek() {
                Tok::Eof => break,
                Tok::Punct(P::RBrace) if closing => break,
                Tok::Punct(P::RBrace) => {
                    let e = self.error_here("unexpected `}` with no open `{`");
                    self.errors.push(e);
                    self.bump();
                    continue;
                }
                _ => {}
            }
            match self.statement(mode) {
                Ok(s) => {
                    let ends = match self.peek() {
                        Tok::Newline | Tok::Punct(P::Semi) | Tok::Eof => true,
                        Tok::Punct(P::RBrace) => closing,
                        _ => false,
                    };
                    out.push(s);
                    if !ends {
                        let hint = if matches!(self.peek(), Tok::Ident(_) | Tok::Punct(P::LParen)) {
                            " (names do not multiply by juxtaposition: write `a*b`)"
                        } else {
                            ""
                        };
                        let e = self.error_here(format!("expected the end of the statement, found {}{hint}", self.found()));
                        self.errors.push(e);
                        out.pop();
                        self.sync(closing);
                    }
                }
                Err(e) => {
                    self.errors.push(e);
                    self.sync(closing);
                }
            }
        }
        out
    }

    /// Skips to the end of the current statement: a new line or `;` outside brackets (consumed), the `}` closing the
    /// enclosing list (not consumed), or the end of input.
    fn sync(&mut self, closing: bool) {
        let mut depth = 0usize;
        loop {
            match self.peek() {
                Tok::Eof => return,
                Tok::Newline | Tok::Punct(P::Semi) if depth == 0 => {
                    self.bump();
                    return;
                }
                Tok::Punct(P::RBrace) if depth == 0 => {
                    if !closing {
                        self.bump();
                    }
                    return;
                }
                Tok::Punct(P::LParen | P::LBracket | P::LBrace) => depth += 1,
                Tok::Punct(P::RParen | P::RBracket | P::RBrace) => depth = depth.saturating_sub(1),
                _ => {}
            }
            self.bump();
        }
    }

    fn statement(&mut self, mode: Mode) -> PResult<Stmt> {
        let start = self.span();
        let defn = matches!(self.peek(), Tok::Punct(P::At) | Tok::Kw("fn" | "rule" | "sym" | "module" | "import"));
        if defn && mode == Mode::Block {
            return Err(self.error_here("definitions, modules and imports are only allowed at the top level or in a module"));
        }
        let kind = match self.peek() {
            Tok::Punct(P::At) | Tok::Kw("fn" | "rule" | "sym") => self.definition()?,
            Tok::Kw("module") => self.module()?,
            Tok::Kw("import") => self.import()?,
            Tok::Kw("pub") => {
                if mode != Mode::Module {
                    return Err(self.error_here("`pub` is only allowed inside a module"));
                }
                self.bump();
                let inner = self.statement(Mode::Module)?;
                if matches!(inner.kind, StmtKind::Pub(_) | StmtKind::Module { .. } | StmtKind::Import { .. }) {
                    return Err(Diagnostic::new(inner.span, "`pub` applies to a definition or value"));
                }
                StmtKind::Pub(Box::new(inner))
            }
            Tok::Kw("let" | "var") => self.let_stmt()?,
            _ => StmtKind::Expr(self.expr(prec::ASSIGN)?),
        };
        Ok(Stmt { kind, span: start.to(self.prev_span()) })
    }

    fn definition(&mut self) -> PResult<StmtKind> {
        let mut decorators = Vec::new();
        while self.at(P::At) {
            let start = self.bump().span;
            let (name, _) = self.expect_ident("a decorator name after `@`")?;
            let args = if self.at(P::LParen) { Some(self.call_args()?) } else { None };
            decorators.push(Decorator { name, args, span: start.to(self.prev_span()) });
            self.skip_newlines();
        }
        match self.peek() {
            Tok::Kw("fn") => self.fn_def(decorators),
            Tok::Kw("rule") => {
                self.bump();
                let lhs = self.expr(prec::PIPE)?;
                let delayed = if self.eat(P::ColonAssign) {
                    true
                } else if self.eat(P::Assign) {
                    false
                } else {
                    return Err(self.error_here(format!("expected `=` or `:=` in the rule, found {}", self.found())));
                };
                self.skip_newlines();
                let rhs = self.expr(prec::ASSIGN)?;
                Ok(StmtKind::Rule { decorators, lhs, delayed, rhs })
            }
            Tok::Kw("sym") => {
                self.bump();
                let (name, _) = self.expect_ident("a symbol name after `sym`")?;
                Ok(StmtKind::Sym { decorators, name })
            }
            _ => Err(self.error_here(format!("expected `fn`, `rule` or `sym` after decorators, found {}", self.found()))),
        }
    }

    fn fn_def(&mut self, decorators: Vec<Decorator>) -> PResult<StmtKind> {
        self.bump();
        let (name, _) = self.expect_ident("a function name after `fn`")?;
        self.expect(P::LParen, "after the function name")?;
        let params = self.param_list(false)?;
        let options = if self.eat(P::Semi) { self.param_list(false)? } else { Vec::new() };
        self.expect(P::RParen, "to close the parameter list")?;
        let result = if self.eat(P::Colon) { Some(self.ty()?) } else { None };
        let condition = if self.eat_kw("when") { Some(self.expr(prec::PIPE)?) } else { None };
        self.expect(P::Assign, "before the function body")?;
        self.skip_newlines();
        let body = self.expr(prec::ASSIGN)?;
        Ok(StmtKind::Fn { decorators, name, params, options, result, condition, body })
    }

    /// Parameters up to `)` or `;`.
    fn param_list(&mut self, lambda: bool) -> PResult<Vec<Param>> {
        let mut out = Vec::new();
        while !self.at(P::RParen) && !self.at(P::Semi) {
            out.push(self.param(lambda)?);
            if !self.eat(P::Comma) {
                break;
            }
        }
        Ok(out)
    }

    fn param(&mut self, lambda: bool) -> PResult<Param> {
        let start = self.span();
        let held = self.eat_kw("held");
        let kind = match (self.peek().clone(), self.nth(1).clone()) {
            (Tok::Ident(n), Tok::Punct(P::Ellipsis)) => {
                self.bump();
                self.bump();
                ParamKind::Rest(n)
            }
            (Tok::Ident(n), Tok::Punct(P::Colon)) => {
                self.bump();
                self.bump();
                ParamKind::Name(n, Some(self.ty()?))
            }
            (Tok::Ident(n), Tok::Punct(P::Comma | P::RParen | P::Semi | P::Assign)) => {
                self.bump();
                ParamKind::Name(n, None)
            }
            _ if lambda => {
                return Err(self.error_here(format!(
                    "expected a lambda parameter (`x`, `x: T` or `xs...`), found {}",
                    self.found()
                )))
            }
            _ => ParamKind::Pattern(self.expr(prec::PIPE)?),
        };
        let default = if self.eat(P::Assign) {
            if matches!(kind, ParamKind::Rest(_)) {
                return Err(Diagnostic::new(self.prev_span(), "a rest parameter cannot have a default"));
            }
            Some(self.expr(prec::PIPE)?)
        } else {
            None
        };
        Ok(Param { held, kind, default, span: start.to(self.prev_span()) })
    }

    fn module(&mut self) -> PResult<StmtKind> {
        self.bump();
        let (name, _) = self.expect_ident("a module name")?;
        self.skip_newlines();
        self.expect(P::LBrace, "to open the module body")?;
        let body = self.statements(Mode::Module);
        self.expect(P::RBrace, "to close the module body")?;
        Ok(StmtKind::Module { name, body })
    }

    fn import(&mut self) -> PResult<StmtKind> {
        self.bump();
        let (path, _) = self.expect_ident("a module name after `import`")?;
        let kind = if self.at(P::Dot) && *self.nth(1) == Tok::Punct(P::LBrace) {
            self.bump();
            self.bump();
            let mut names = Vec::new();
            while !self.at(P::RBrace) {
                names.push(self.expect_ident("a name to import")?.0);
                if !self.eat(P::Comma) {
                    break;
                }
            }
            self.expect(P::RBrace, "to close the import list")?;
            ImportKind::Only(names)
        } else if matches!(self.peek(), Tok::Ident(n) if n == "except") {
            self.bump();
            let mut names = vec![self.expect_ident("a name after `except`")?.0];
            while self.eat(P::Comma) {
                names.push(self.expect_ident("a name after `,`")?.0);
            }
            ImportKind::Except(names)
        } else {
            ImportKind::All
        };
        Ok(StmtKind::Import { path, kind })
    }

    fn let_stmt(&mut self) -> PResult<StmtKind> {
        let mutable = self.bump().tok == Tok::Kw("var");
        let (name, _) = self.expect_ident(if mutable { "a name after `var`" } else { "a name after `let`" })?;
        let ty = if self.eat(P::Colon) { Some(self.ty()?) } else { None };
        self.expect(P::Assign, "in the binding")?;
        self.skip_newlines();
        let value = self.expr(prec::ASSIGN)?;
        Ok(StmtKind::Let { mutable, name, ty, value })
    }

    // ---- expressions ----

    /// An expression whose operators all bind at least as tightly as `min`.
    fn expr(&mut self, min: u8) -> PResult<Expr> {
        self.nested(|p| p.expr_inner(min))
    }

    fn expr_inner(&mut self, min: u8) -> PResult<Expr> {
        let mut lhs = self.prefix()?;
        loop {
            // A pipe at the start of the next line continues the expression.
            if *self.peek() == Tok::Newline && min <= prec::PIPE && *self.peek_past_newlines() == Tok::Punct(P::Pipe) {
                self.skip_newlines();
            }
            let Some((level, op)) = infix(self.peek()) else { break };
            if level < min {
                break;
            }
            lhs = match op {
                Infix::Bin(b) => {
                    self.bump();
                    self.skip_newlines();
                    let rhs = match b {
                        BinOp::Set | BinOp::SetDelayed => self.expr(prec::ASSIGN)?,
                        BinOp::Rule | BinOp::RuleDelayed => self.expr(prec::RULE)?,
                        _ => self.expr(level + 1)?,
                    };
                    if level == prec::CMP && is_comparison(self.peek()) {
                        return Err(self.error_here("comparisons do not chain: write `a < b and b < c`"));
                    }
                    let span = lhs.span.to(rhs.span);
                    Expr { kind: ExprKind::Binary(b, Box::new(lhs), Box::new(rhs)), span }
                }
                Infix::Nary(n) => {
                    let mut items = vec![lhs];
                    while infix(self.peek()).is_some_and(|(l, _)| l == level) {
                        self.bump();
                        self.skip_newlines();
                        items.push(self.expr(level + 1)?);
                    }
                    let span = items[0].span.to(items[items.len() - 1].span);
                    Expr { kind: ExprKind::Nary(n, items), span }
                }
                Infix::Add => {
                    let mut items = vec![(false, lhs)];
                    while self.at(P::Plus) || self.at(P::Minus) {
                        let neg = self.bump().tok == Tok::Punct(P::Minus);
                        self.skip_newlines();
                        items.push((neg, self.expr(prec::ADD + 1)?));
                    }
                    let span = items[0].1.span.to(items[items.len() - 1].1.span);
                    Expr { kind: ExprKind::Add(items), span }
                }
                Infix::Mul => {
                    let mut items = vec![(MulOp::Times, lhs)];
                    while matches!(self.peek(), Tok::Punct(P::Star | P::Slash) | Tok::Juxt) {
                        let op = if self.bump().tok == Tok::Punct(P::Slash) { MulOp::Div } else { MulOp::Times };
                        self.skip_newlines();
                        items.push((op, self.expr(prec::MUL + 1)?));
                    }
                    let span = items[0].1.span.to(items[items.len() - 1].1.span);
                    Expr { kind: ExprKind::Mul(items), span }
                }
                Infix::Pow => {
                    self.bump();
                    self.skip_newlines();
                    let rhs = self.expr(prec::POW)?;
                    let span = lhs.span.to(rhs.span);
                    Expr { kind: ExprKind::Pow(Box::new(lhs), Box::new(rhs)), span }
                }
            };
        }
        Ok(lhs)
    }

    /// Prefix `-` and `not`, accepted in any operand position (so `a^-1` and `a*-b` need no parentheses).
    fn prefix(&mut self) -> PResult<Expr> {
        match self.peek() {
            Tok::Punct(P::Minus) => {
                let start = self.bump().span;
                self.skip_newlines();
                let e = self.expr(prec::POW)?;
                let span = start.to(e.span);
                Ok(Expr { kind: ExprKind::Neg(Box::new(e)), span })
            }
            Tok::Kw("not") => {
                let start = self.bump().span;
                self.skip_newlines();
                let e = self.expr(prec::CMP)?;
                let span = start.to(e.span);
                Ok(Expr { kind: ExprKind::Not(Box::new(e)), span })
            }
            Tok::Sqrt => {
                // `√x` is `sqrt(x)`; the operand is a primary with its postfixes (`√x^2` is `(√x)^2`).
                let start = self.bump().span;
                let e = self.nested(|p| if *p.peek() == Tok::Sqrt { p.prefix() } else { p.postfix_expr() })?;
                let span = start.to(e.span);
                let f = Expr { kind: ExprKind::Name("sqrt".into()), span: start };
                Ok(Expr { kind: ExprKind::Call(Box::new(f), vec![Arg { name: None, value: e }]), span })
            }
            _ => self.postfix_expr(),
        }
    }

    /// A primary with its calls, indices, prime marks and `@`.
    fn postfix_expr(&mut self) -> PResult<Expr> {
        let (mut e, is_number) = self.primary()?;
        if is_number {
            // A number takes no postfix: `2(x)` is juxtaposition, handled as a product.
            return Ok(e);
        }
        loop {
            match self.peek().clone() {
                Tok::Punct(P::LParen) => {
                    let args = self.call_args()?;
                    let span = e.span.to(self.prev_span());
                    e = Expr { kind: ExprKind::Call(Box::new(e), args), span };
                }
                Tok::Punct(P::LBracket) => {
                    let idx = self.index_args()?;
                    let span = e.span.to(self.prev_span());
                    e = Expr { kind: ExprKind::Index(Box::new(e), idx), span };
                }
                Tok::Prime(n) => {
                    self.bump();
                    let span = e.span.to(self.prev_span());
                    e = Expr { kind: ExprKind::Prime(Box::new(e), n), span };
                }
                Tok::Punct(P::At) => {
                    let ExprKind::Name(name) = &e.kind else {
                        return Err(self.error_here("`@` names a pattern: the left side must be a name, as in `x @ f(y_)`"));
                    };
                    let name = name.clone();
                    self.bump();
                    let p = self.nested(|p| p.postfix_expr())?;
                    let span = e.span.to(p.span);
                    return Ok(Expr { kind: ExprKind::Named(name, Box::new(p)), span });
                }
                _ => return Ok(e),
            }
        }
    }

    /// A primary expression and whether it is a numeric literal token.
    fn primary(&mut self) -> PResult<(Expr, bool)> {
        let start = self.span();
        let tok = self.peek().clone();
        let kind = match tok {
            Tok::Num(n) => {
                self.bump();
                return Ok((Expr { kind: ExprKind::Num(n), span: start }, true));
            }
            Tok::Str(s) => {
                self.bump();
                ExprKind::Str(s)
            }
            Tok::Ident(name) => {
                if !self.no_lambda && *self.nth(1) == Tok::Punct(P::FatArrow) {
                    self.bump();
                    self.bump();
                    self.skip_newlines();
                    let body = self.with_no_lambda(false, |p| p.expr(prec::PIPE))?;
                    let param = Param { held: false, kind: ParamKind::Name(name, None), default: None, span: start };
                    let span = start.to(body.span);
                    return Ok((Expr { kind: ExprKind::Lambda(vec![param], Box::new(body)), span }, false));
                }
                self.bump();
                ExprKind::Name(name)
            }
            Tok::Kw(k @ ("true" | "false" | "null")) => {
                self.bump();
                ExprKind::Name(k.to_string())
            }
            Tok::Pattern { name, kind } => {
                self.bump();
                let pat = Expr { kind: ExprKind::Pattern { name, kind }, span: start };
                if self.eat(P::Colon) {
                    let ty = self.ty()?;
                    let span = start.to(ty.span);
                    return Ok((Expr { kind: ExprKind::Typed(Box::new(pat), Box::new(ty)), span }, false));
                }
                return Ok((pat, false));
            }
            Tok::Slot(k) => {
                if self.slot_depth == 0 {
                    return Err(self.error_here(format!("`&{k}` is only allowed inside a slot function `&(...)`")));
                }
                self.bump();
                ExprKind::Slot(k)
            }
            Tok::Punct(P::Amp) => {
                self.bump();
                if !self.at(P::LParen) {
                    return Err(self.error_here("expected `(` after `&` to start a slot function `&(...)`"));
                }
                self.bump();
                self.slot_depth += 1;
                let body = self.with_no_lambda(false, |p| p.expr(prec::ASSIGN));
                self.slot_depth -= 1;
                let body = body?;
                self.expect(P::RParen, "to close the slot function")?;
                ExprKind::SlotFn(Box::new(body))
            }
            Tok::Punct(P::LParen) => {
                if !self.no_lambda && self.paren_lambda_ahead() {
                    return self.paren_lambda().map(|e| (e, false));
                }
                self.bump();
                if self.at(P::RParen) {
                    return Err(self.error_here("empty parentheses: `()` only starts a lambda `() => e`"));
                }
                let e = self.with_no_lambda(false, |p| p.expr(prec::ASSIGN))?;
                self.expect(P::RParen, "to close the parenthesis")?;
                return Ok((Expr { kind: e.kind, span: start.to(self.prev_span()) }, false));
            }
            Tok::Punct(P::LBracket) => {
                self.bump();
                let mut items = Vec::new();
                self.with_no_lambda(false, |p| {
                    while !p.at(P::RBracket) {
                        items.push(p.expr(prec::ASSIGN)?);
                        if !p.eat(P::Comma) {
                            break;
                        }
                    }
                    Ok(())
                })?;
                self.expect(P::RBracket, "to close the list")?;
                ExprKind::List(items)
            }
            Tok::Punct(P::LBrace) => ExprKind::Block(self.block()?),
            Tok::Kw("if") => self.if_expr()?,
            Tok::Kw("while") => {
                self.bump();
                let c = self.with_no_lambda(false, |p| p.expr(prec::ASSIGN))?;
                self.skip_newlines();
                let body = self.block_expr()?;
                ExprKind::While(Box::new(c), Box::new(body))
            }
            Tok::Kw("for") => {
                self.bump();
                let (name, _) = self.expect_ident("a loop variable after `for`")?;
                self.expect_kw("in", "after the loop variable")?;
                let it = self.with_no_lambda(false, |p| p.expr(prec::ASSIGN))?;
                self.skip_newlines();
                let body = self.block_expr()?;
                ExprKind::For(name, Box::new(it), Box::new(body))
            }
            Tok::Kw("match") => self.match_expr()?,
            Tok::Placeholder => {
                return Err(self.error_here("a placeholder `□` is not filled in: replace it with an expression"))
            }
            t => return Err(self.error_here(format!("expected an expression, found {}", t.describe()))),
        };
        Ok((Expr { kind, span: start.to(self.prev_span()) }, false))
    }

    /// Whether the `(` at the cursor opens a lambda parameter list: its matching `)` is followed by `=>`.
    fn paren_lambda_ahead(&self) -> bool {
        let mut depth = 0usize;
        for (i, t) in self.toks[self.pos..].iter().enumerate() {
            match t.tok {
                Tok::Punct(P::LParen | P::LBracket | P::LBrace) => depth += 1,
                Tok::Punct(P::RParen | P::RBracket | P::RBrace) => {
                    depth -= 1;
                    if depth == 0 {
                        return self.toks.get(self.pos + i + 1).is_some_and(|t| t.tok == Tok::Punct(P::FatArrow));
                    }
                }
                Tok::Eof => return false,
                _ => {}
            }
        }
        false
    }

    fn paren_lambda(&mut self) -> PResult<Expr> {
        let start = self.bump().span;
        let params = self.with_no_lambda(false, |p| p.param_list(true))?;
        self.expect(P::RParen, "to close the lambda parameters")?;
        self.expect(P::FatArrow, "after the lambda parameters")?;
        self.skip_newlines();
        let body = self.with_no_lambda(false, |p| p.expr(prec::PIPE))?;
        let span = start.to(body.span);
        Ok(Expr { kind: ExprKind::Lambda(params, Box::new(body)), span })
    }

    fn block(&mut self) -> PResult<Vec<Stmt>> {
        self.expect(P::LBrace, "to open a block")?;
        let saved = std::mem::replace(&mut self.no_lambda, false);
        let body = self.statements(Mode::Block);
        self.no_lambda = saved;
        self.expect(P::RBrace, "to close the block")?;
        Ok(body)
    }

    fn block_expr(&mut self) -> PResult<Expr> {
        let start = self.span();
        let b = self.block()?;
        Ok(Expr { kind: ExprKind::Block(b), span: start.to(self.prev_span()) })
    }

    fn if_expr(&mut self) -> PResult<ExprKind> {
        self.bump();
        self.with_no_lambda(false, |p| {
            p.skip_newlines();
            let c = p.expr(prec::ASSIGN)?;
            p.skip_newlines();
            p.expect_kw("then", "after the condition of `if`")?;
            p.skip_newlines();
            let a = p.expr(prec::ASSIGN)?;
            let b = if *p.peek_past_newlines() == Tok::Kw("else") {
                p.skip_newlines();
                p.bump();
                p.skip_newlines();
                Some(Box::new(p.expr(prec::ASSIGN)?))
            } else {
                None
            };
            Ok(ExprKind::If(Box::new(c), Box::new(a), b))
        })
    }

    fn match_expr(&mut self) -> PResult<ExprKind> {
        self.bump();
        let e = self.with_no_lambda(false, |p| p.expr(prec::ASSIGN))?;
        self.skip_newlines();
        self.expect(P::LBrace, "to open the match arms")?;
        let mut arms = Vec::new();
        loop {
            while matches!(self.peek(), Tok::Newline) {
                self.bump();
            }
            if self.at(P::RBrace) {
                break;
            }
            let pattern = self.with_no_lambda(true, |p| p.expr(prec::WHEN))?;
            self.skip_newlines();
            self.expect(P::FatArrow, "after the pattern of the match arm")?;
            self.skip_newlines();
            let body = self.with_no_lambda(false, |p| p.expr(prec::PIPE))?;
            arms.push(Arm { pattern, body });
            // Arms are separated by `,`, by new lines, or both.
            let newline = *self.peek() == Tok::Newline;
            self.skip_newlines();
            if !self.eat(P::Comma) && !newline && !self.at(P::RBrace) {
                return Err(self.error_here(format!("expected `,` or `}}` after the match arm, found {}", self.found())));
            }
        }
        self.expect(P::RBrace, "to close the match arms")?;
        Ok(ExprKind::Match(Box::new(e), arms))
    }

    fn call_args(&mut self) -> PResult<Vec<Arg>> {
        self.expect(P::LParen, "to open the arguments")?;
        let mut args = Vec::new();
        self.with_no_lambda(false, |p| {
            let mut named = false;
            while !p.at(P::RParen) {
                if let (Tok::Ident(n), Tok::Punct(P::Colon)) = (p.peek().clone(), p.nth(1).clone()) {
                    let s = p.bump().span;
                    p.bump();
                    let value = p.expr(prec::PIPE)?;
                    args.push(Arg { name: Some((n, s)), value });
                    named = true;
                } else {
                    if named {
                        return Err(p.error_here("positional arguments must come before named arguments"));
                    }
                    let value = p.expr(prec::ASSIGN)?;
                    args.push(Arg { name: None, value });
                }
                if !p.eat(P::Comma) {
                    break;
                }
            }
            Ok(())
        })?;
        self.expect(P::RParen, "to close the arguments")?;
        Ok(args)
    }

    fn index_args(&mut self) -> PResult<Vec<Expr>> {
        self.expect(P::LBracket, "to open the index")?;
        if self.at(P::RBracket) {
            return Err(self.error_here("an index needs at least one argument"));
        }
        let mut idx = Vec::new();
        self.with_no_lambda(false, |p| {
            loop {
                let a = p.expr(prec::PIPE)?;
                let item = if p.eat(P::Colon) {
                    let b = p.expr(prec::PIPE)?;
                    let span = a.span.to(b.span);
                    Expr { kind: ExprKind::Span(Box::new(a), Box::new(b)), span }
                } else {
                    a
                };
                idx.push(item);
                if !p.eat(P::Comma) || p.at(P::RBracket) {
                    break;
                }
            }
            Ok(())
        })?;
        self.expect(P::RBracket, "to close the index")?;
        Ok(idx)
    }

    // ---- types (04 section 9) ----

    fn ty(&mut self) -> PResult<Type> {
        self.nested(|p| {
            let start = p.span();
            let u = p.union_ty()?;
            if matches!(p.peek(), Tok::Ident(n) if n == "where") {
                p.bump();
                let pred = p.expr(prec::PIPE)?;
                let span = start.to(pred.span);
                return Ok(Type { kind: TypeKind::Where(Box::new(u), Box::new(pred)), span });
            }
            Ok(u)
        })
    }

    fn union_ty(&mut self) -> PResult<Type> {
        let first = self.atom_ty()?;
        if !self.at(P::Bar) {
            return Ok(first);
        }
        let mut items = vec![first];
        while self.eat(P::Bar) {
            items.push(self.atom_ty()?);
        }
        let span = items[0].span.to(items[items.len() - 1].span);
        Ok(Type { kind: TypeKind::Union(items), span })
    }

    fn atom_ty(&mut self) -> PResult<Type> {
        self.nested(|p| {
            let start = p.span();
            let kind = match p.peek().clone() {
                Tok::Ident(n) => {
                    p.bump();
                    if p.at(P::LBracket) {
                        p.bump();
                        let mut args = Vec::new();
                        while !p.at(P::RBracket) {
                            args.push(p.ty()?);
                            if !p.eat(P::Comma) {
                                break;
                            }
                        }
                        p.expect(P::RBracket, "to close the type arguments")?;
                        if args.is_empty() {
                            return Err(Diagnostic::new(p.prev_span(), "a type application needs at least one argument"));
                        }
                        TypeKind::App(n, args)
                    } else {
                        TypeKind::Name(n)
                    }
                }
                Tok::Num(n) if matches!(n.kind, NumKind::Int { .. }) && !n.imag => {
                    p.bump();
                    TypeKind::Num(n)
                }
                Tok::Punct(P::LParen) => {
                    p.bump();
                    let mut items = Vec::new();
                    while !p.at(P::RParen) {
                        items.push(p.ty()?);
                        if !p.eat(P::Comma) {
                            break;
                        }
                    }
                    p.expect(P::RParen, "to close the type")?;
                    if p.eat(P::Arrow) {
                        let r = p.atom_ty()?;
                        TypeKind::Fun(items, Box::new(r))
                    } else if items.len() == 1 {
                        let t = items.pop().map(|t| t.kind);
                        t.unwrap_or(TypeKind::Name(String::new()))
                    } else {
                        return Err(p.error_here("expected `->` after the parameter types of a function type"));
                    }
                }
                t => return Err(p.error_here(format!("expected a type, found {}", t.describe()))),
            };
            Ok(Type { kind, span: start.to(p.prev_span()) })
        })
    }
}
