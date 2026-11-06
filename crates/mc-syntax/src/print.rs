//! The printer: core terms back to the concrete syntax.
//!
//! **Faithfulness.** For every term the store can hold except balls and non-finite machine reals (whose text comes
//! from the pluggable [`NumberFormat`]) and truncated power series (`series_data(...)`, written as their sum with an
//! order term `O(x^p)`, which `series` reads back), the printed text reads back, through [`crate::read`], to the
//! identical term.
//! Special syntax is used only when the term has exactly the shape lowering produces for it (shapes on
//! [`crate::heads::Head`]); any other term prints as a plain call, `` `if`(c) `` with backquotes for keyword heads.
//! Context-dependent readings are tracked while printing, with the same functions lowering uses (`scope.rs`): which
//! names are `var` locals (so `x = v` reads back as `assign` or `set` correctly), and which slot names `&k` a slot
//! function binds.
//!
//! **Minimal parentheses.** Each form has a binding level (`prec.rs`); a subterm is parenthesised when its level is
//! below what its position requires. Open forms (`if`, lambdas) and forms whose right edge would absorb what follows
//! are parenthesised only where something follows them.
//!
//! Terms can be arbitrarily deep, so printing runs on an explicit task stack (decision 1.6), not on recursion.
use crate::heads::{as_sym, Head, Heads};
use crate::ident::{ident_token, is_pattern_name, string_token};
use crate::lower::slot_index;
use crate::nat;
use crate::prec::*;
use crate::scope::{lambda_param_names, pattern_vars, statement_binding, Bind, Scopes};
use mc_api::{Store, Sym, View, T};
use std::collections::HashMap;

/// The head of a truncated power series `series_data(x, x0, [c_v, ..., c_(p-1)], v, p)` (the result of `mc-poly`'s
/// `series`): the sum of c_k (x - x0)^k plus an unknown remainder of order p.
const SERIES_DATA: &str = "series_data";

/// Text of the number kinds whose conversion belongs to `mc-arith`. The default converts big integers with a simple
/// quadratic algorithm and writes balls as their payload.
pub trait NumberFormat {
    /// Decimal digits of a big integer's magnitude (the printer adds the sign).
    fn big_int(&self, mag: &[u64]) -> String {
        nat::to_decimal(mag)
    }
    /// The text of a ball. It must be an expression; it reads back as a ball only once `mc-arith` provides a
    /// faithful conversion.
    fn ball(&self, neg: bool, words: &[u64]) -> String {
        let ws: Vec<String> = words.iter().map(|w| w.to_string()).collect();
        format!("ball_payload({neg}, [{}])", ws.join(", "))
    }
    /// The text of an infinite or NaN machine real (no literal denotes them).
    fn non_finite(&self, v: f64) -> String {
        let s = if v.is_nan() {
            "nan"
        } else if v > 0.0 {
            "inf"
        } else {
            "-inf"
        };
        format!("machine_real(\"{s}\")")
    }
}

/// The default [`NumberFormat`].
pub struct DefaultNumberFormat;

impl NumberFormat for DefaultNumberFormat {}

/// Prints terms of one store.
pub struct Printer<'h, S: Store + ?Sized> {
    store: &'h S,
    heads: Heads,
    numbers: Box<dyn NumberFormat + 'h>,
    /// the head of truncated power series, printed as sums with an order term (see [`Run::series`])
    series_data: Sym,
    /// polled while printing: once it is true the text printed so far is returned, ending in `…`
    stop: Option<Box<dyn Fn() -> bool + 'h>>,
}

impl<'h, S: Store + ?Sized> Printer<'h, S> {
    pub fn new(store: &'h S) -> Self {
        Self::with_numbers(store, Box::new(DefaultNumberFormat))
    }

    pub fn with_numbers(store: &'h S, numbers: Box<dyn NumberFormat + 'h>) -> Self {
        Printer { store, heads: Heads::new(store), numbers, series_data: store.sym(SERIES_DATA), stop: None }
    }

    /// This printer, polling `stop` as it prints: a huge term (a million copies of a deep term) takes long and
    /// its text is huge, so an interrupt ends the printing with the text so far and `…`.
    pub fn with_stop(mut self, stop: impl Fn() -> bool + 'h) -> Self {
        self.stop = Some(Box::new(stop));
        self
    }

    /// A term as an expression.
    pub fn expr(&self, t: T<'h>) -> String {
        self.run(vec![Task::Expr(t, Ctx::at(ASSIGN))])
    }

    /// A term as one top-level statement (definitions, modules and imports in their statement syntax).
    pub fn statement(&self, t: T<'h>) -> String {
        self.run(vec![Task::Stmt(t, Mode::Top)])
    }

    /// Terms as the statements of one program, one per line; `let` and `var` bindings carry over to later lines.
    pub fn program(&self, ts: &[T<'h>]) -> String {
        let mut tasks = Vec::new();
        for (i, &t) in ts.iter().enumerate() {
            if i > 0 {
                tasks.push(Task::Text("\n"));
            }
            tasks.push(Task::Stmt(t, Mode::Top));
        }
        self.run(tasks)
    }

    fn run(&self, tasks: Vec<Task<'h>>) -> String {
        let mut r = Run {
            p: self,
            out: String::new(),
            stack: tasks.into_iter().rev().collect(),
            scopes: Scopes::default(),
            slot_depth: 0,
            slot_memo: HashMap::new(),
        };
        r.scopes.push();
        r.go();
        r.out
    }
}

/// Where a statement is printed (what statement syntax reads back there).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Top,
    Module,
    Block,
}

/// The requirements of an expression position.
#[derive(Clone, Copy, Default)]
struct Ctx {
    /// Minimum binding level that needs no parentheses.
    min: u8,
    /// Directly a call argument: `p = d` reads as `optional(p, d)` here.
    arg: bool,
    /// A callee or indexed base: a number token here would read as juxtaposition or take no postfix.
    no_num: bool,
    /// The parser accepts a prefix `-` here even though `min` is above [`NEG`] (the exponent of `^`).
    prefix_ok: bool,
    /// Parenthesise whatever the form.
    force: bool,
}

impl Ctx {
    fn at(min: u8) -> Ctx {
        Ctx { min, ..Ctx::default() }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ParamMode {
    Lambda,
    Fn,
}

enum Task<'h> {
    Expr(T<'h>, Ctx),
    Stmt(T<'h>, Mode),
    Param(T<'h>, ParamMode),
    /// A type; the `u8` is the minimum type level: 0 any, 1 no `where`, 2 an atom.
    Type(T<'h>, u8),
    Text(&'static str),
    Owned(String),
    PushLets(Vec<Sym>),
    PushFrame,
    PopFrame,
    Bind(Sym, Bind),
    SlotEnter,
    SlotExit,
}

struct Run<'p, 'h, S: Store + ?Sized> {
    p: &'p Printer<'h, S>,
    out: String,
    stack: Vec<Task<'h>>,
    scopes: Scopes,
    slot_depth: u32,
    /// The largest free slot `&k` of each term visited (see [`Run::slots`]).
    slot_memo: HashMap<T<'h>, u32>,
}

/// Validity of a ball literal's text: what the lexer produces for a mantissa (`-`, digits, optional `.` and
/// digits, optional `e`, sign and digits).
fn valid_mantissa(s: &str) -> bool {
    let b = s.strip_prefix('-').unwrap_or(s).as_bytes();
    let mut i = 0;
    let digits = |i: &mut usize| {
        let st = *i;
        while *i < b.len() && b[*i].is_ascii_digit() {
            *i += 1;
        }
        *i > st
    };
    if !digits(&mut i) {
        return false;
    }
    if i < b.len() && b[i] == b'.' {
        i += 1;
        digits(&mut i);
    }
    if i < b.len() && b[i] == b'e' {
        i += 1;
        if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
            i += 1;
        }
        if !digits(&mut i) {
            return false;
        }
    }
    i == b.len()
}

impl<'p, 'h, S: Store + ?Sized> Run<'p, 'h, S> {
    fn store(&self) -> &'h S {
        self.p.store
    }
    fn heads(&self) -> &'p Heads {
        &self.p.heads
    }
    fn is(&self, t: T<'h>, h: Head, n: usize) -> Option<&'h [T<'h>]> {
        self.p.heads.is(self.p.store, t, h, n)
    }
    fn classify(&self, t: T<'h>) -> Option<(Head, &'h [T<'h>])> {
        self.p.heads.classify(self.p.store, t)
    }
    fn sym_of(&self, t: T<'h>) -> Option<Sym> {
        as_sym(self.p.store, t)
    }
    fn name(&self, s: Sym) -> String {
        self.p.store.sym_name(s)
    }
    fn ident(&self, s: Sym) -> String {
        let mut o = String::new();
        ident_token(&self.name(s), &mut o);
        o
    }

    fn go(&mut self) {
        let mut polled = 0u32;
        while let Some(task) = self.stack.pop() {
            polled += 1;
            if polled.is_multiple_of(4096) && self.p.stop.as_ref().is_some_and(|stop| stop()) {
                self.out.push('…');
                return;
            }
            match task {
                Task::Text(s) => self.out.push_str(s),
                Task::Owned(s) => self.out.push_str(&s),
                Task::PushLets(v) => self.scopes.push_lets(v),
                Task::PushFrame => self.scopes.push(),
                Task::PopFrame => self.scopes.pop(),
                Task::Bind(s, b) => self.scopes.bind(s, b),
                Task::SlotEnter => self.slot_depth += 1,
                Task::SlotExit => self.slot_depth -= 1,
                Task::Expr(t, ctx) => {
                    let mut v = Vec::new();
                    let (level, prefix) = self.expr_form(t, ctx, &mut v);
                    let paren = ctx.force
                        || (level < ctx.min && !(ctx.prefix_ok && prefix))
                        || (ctx.no_num && self.is_num_token(t));
                    self.emit(v, paren);
                }
                Task::Stmt(t, mode) => {
                    let mut v = Vec::new();
                    if !self.stmt_form(t, mode, &mut v) {
                        v.clear();
                        v.push(Task::Expr(t, Ctx::at(ASSIGN)));
                    }
                    self.emit(v, false);
                }
                Task::Param(t, mode) => {
                    let mut v = Vec::new();
                    self.param_form(t, mode, &mut v);
                    self.emit(v, false);
                }
                Task::Type(t, min) => {
                    let mut v = Vec::new();
                    let level = self.type_form(t, &mut v);
                    self.emit(v, level < min);
                }
            }
        }
    }

    fn emit(&mut self, v: Vec<Task<'h>>, paren: bool) {
        if paren {
            self.stack.push(Task::Text(")"));
        }
        self.stack.extend(v.into_iter().rev());
        if paren {
            self.stack.push(Task::Text("("));
        }
    }

    // ---- numbers ----

    fn int_text(&self, t: T<'h>) -> (String, bool) {
        match self.store().view(t) {
            View::Int(x) => (x.unsigned_abs().to_string(), x < 0),
            View::BigInt { neg, mag } => (self.p.numbers.big_int(mag), neg),
            _ => (String::new(), false),
        }
    }

    /// A literal written as one number token, perhaps with a leading `-`: its unsigned text and sign. Integers,
    /// finite machine reals, `ball_literal` terms and `complex(0, k)` for such a `k`.
    fn literal(&self, t: T<'h>) -> Option<(String, bool)> {
        let st = self.store();
        match st.view(t) {
            View::Int(_) | View::BigInt { .. } => Some(self.int_text(t)),
            View::Real(x) if x.is_finite() => Some((format!("{:?}", x.abs()), x.is_sign_negative())),
            View::App { .. } => {
                if let Some([s, d]) = self.is(t, Head::BallLiteral, 2) {
                    if let (View::Str(s), View::Int(d)) = (st.view(*s), st.view(*d)) {
                        if d >= 1 && valid_mantissa(s) {
                            let neg = s.starts_with('-');
                            return Some((format!("{}~{d}", s.trim_start_matches('-')), neg));
                        }
                    }
                }
                if let Some([re, k]) = self.is(t, Head::Complex, 2) {
                    if matches!(st.view(*re), View::Int(0)) && self.is(*k, Head::Complex, 2).is_none() {
                        let (txt, neg) = self.literal(*k)?;
                        return Some((txt + "im", neg));
                    }
                }
                None
            }
            _ => None,
        }
    }

    /// Whether `t` prints as a bare (unsigned) number token.
    fn is_num_token(&self, t: T<'h>) -> bool {
        match self.store().view(t) {
            View::Ball { neg, words } => self.p.numbers.ball(neg, words).starts_with(|c: char| c.is_ascii_digit()),
            _ => matches!(self.literal(t), Some((_, false))),
        }
    }

    fn is_rational(&self, t: T<'h>) -> bool {
        matches!(self.store().view(t), View::Rational { .. })
    }

    // ---- expressions ----

    /// Fills `v` with the tasks printing `t` and returns its level and whether it starts with a prefix `-`.
    fn expr_form(&mut self, t: T<'h>, ctx: Ctx, v: &mut Vec<Task<'h>>) -> (u8, bool) {
        let st = self.store();
        if let Some((txt, neg)) = self.literal(t) {
            v.push(Task::Owned(if neg { format!("-{txt}") } else { txt }));
            return if neg { (NEG, true) } else { (ATOM, false) };
        }
        match st.view(t) {
            View::Rational { num, den } => {
                let (n, neg) = self.int_text(num);
                let (d, _) = self.int_text(den);
                v.push(Task::Owned(format!("{}{n}/{d}", if neg { "-" } else { "" })));
                (MUL, false)
            }
            View::Real(x) => text_form(self.p.numbers.non_finite(x), v),
            View::Ball { neg, words } => text_form(self.p.numbers.ball(neg, words), v),
            View::Str(s) => {
                let mut o = String::new();
                string_token(s, &mut o);
                v.push(Task::Owned(o));
                (ATOM, false)
            }
            View::Sym(s) => {
                let name = self.name(s);
                // Slot names inside a slot function and the constant keywords are written bare.
                let bare = (self.slot_depth > 0 && slot_index(&name).is_some()) || matches!(name.as_str(), "true" | "false" | "null");
                let text = if bare {
                    name
                } else {
                    let mut o = String::new();
                    ident_token(&name, &mut o);
                    o
                };
                v.push(Task::Owned(text));
                (ATOM, false)
            }
            View::App { head, args } => {
                if let Some(r) = self.derivative(head, args, v) {
                    return r;
                }
                if let Some(r) = self.series(head, args, v) {
                    return r;
                }
                if let Some((h, args)) = self.classify(t) {
                    if let Some(r) = self.special(t, h, args, ctx, v) {
                        return r;
                    }
                    v.clear();
                }
                self.call_items(head, args, v);
                (POSTFIX, false)
            }
            View::Int(_) | View::BigInt { .. } => (ATOM, false),
        }
    }

    /// `derivative(n)(f)` for a symbol `f` and `1 <= n <= 4`: `f'`.
    fn derivative(&self, head: T<'h>, args: &[T<'h>], v: &mut Vec<Task<'h>>) -> Option<(u8, bool)> {
        let [n] = self.is(head, Head::Derivative, 1)? else { return None };
        let View::Int(n @ 1..=4) = self.store().view(*n) else { return None };
        let [f] = args else { return None };
        let f = self.sym_of(*f)?;
        v.push(Task::Owned(format!("{}{}", self.ident(f), "'".repeat(n as usize))));
        Some((POSTFIX, false))
    }

    /// A truncated power series `series_data(x, x0, [c_v, ...], v, p)` with at most p - v coefficients, as
    /// the sum of its nonzero terms followed by the order term: `1 + x + x^2/2 + O(x^3)`, `(x - 1) + O((x - 1)^2)`.
    /// The text reads back as that sum, an `add` with an `O(...)` call, which `series` reads as the same series (the
    /// one form this printer does not keep: decision 9.3 of the library-gaps package).
    fn series(&self, head: T<'h>, args: &'h [T<'h>], v: &mut Vec<Task<'h>>) -> Option<(u8, bool)> {
        let st = self.store();
        if self.sym_of(head) != Some(self.p.series_data) {
            return None;
        }
        let [x, x0, cs, val, prec] = args else { return None };
        let Some((Head::List, cs)) = self.classify(*cs) else { return None };
        let (View::Int(val), View::Int(prec)) = (st.view(*val), st.view(*prec)) else { return None };
        // the coefficients of t^val, t^(val+1), ...; missing ones up to the order are zero
        if prec.checked_sub(val).is_none_or(|n| n < cs.len() as i64) {
            return None;
        }
        let mut first = true;
        for (i, &c) in cs.iter().enumerate() {
            if matches!(st.view(c), View::Int(0)) {
                continue;
            }
            self.series_term(c, *x, *x0, val + i as i64, first, v);
            first = false;
        }
        v.push(Task::Text(if first { "O(" } else { " + O(" }));
        self.series_power(*x, *x0, prec, v);
        v.push(Task::Text(")"));
        Some(if first { (POSTFIX, false) } else { (ADD, false) })
    }

    /// One term c (x - x0)^k of a series, with its sign as ` - ` (or a leading `-`) when c is a negative number or
    /// `mul(-1, b)`; a rational coefficient n/d is written `n*x^k/d` (`n/(d*x^m)` for k = -m < 0).
    fn series_term(&self, c: T<'h>, x: T<'h>, x0: T<'h>, k: i64, first: bool, v: &mut Vec<Task<'h>>) {
        let st = self.store();
        let sign = |neg: bool, v: &mut Vec<Task<'h>>| match (first, neg) {
            (true, false) => {}
            (true, true) => v.push(Task::Text("-")),
            (false, false) => v.push(Task::Text(" + ")),
            (false, true) => v.push(Task::Text(" - ")),
        };
        let number = match st.view(c) {
            View::Int(_) | View::BigInt { .. } => Some((self.int_text(c), None)),
            View::Rational { num, den } => Some((self.int_text(num), Some(self.int_text(den).0))),
            _ => None,
        };
        if let Some(((n, neg), d)) = number {
            sign(neg, v);
            let one = n == "1";
            match (k, d) {
                (0, None) => v.push(Task::Owned(n)),
                (0, Some(d)) => v.push(Task::Owned(format!("{n}/{d}"))),
                (k, Some(d)) if k < 0 => {
                    v.push(Task::Owned(format!("{n}/({d}*")));
                    self.series_power(x, x0, -k, v);
                    v.push(Task::Text(")"));
                }
                (k, d) => {
                    if !one {
                        v.push(Task::Owned(n + "*"));
                    }
                    self.series_power(x, x0, k, v);
                    if let Some(d) = d {
                        v.push(Task::Owned(format!("/{d}")));
                    }
                }
            }
            return;
        }
        let (neg, c) = match self.is(c, Head::Mul, 2) {
            Some([m, b]) if matches!(st.view(*m), View::Int(-1)) => (true, *b),
            _ => (false, c),
        };
        sign(neg, v);
        v.push(Task::Expr(c, Ctx::at(if k == 0 { ADD + 1 } else { MUL })));
        if k != 0 {
            v.push(Task::Text("*"));
            self.series_power(x, x0, k, v);
        }
    }

    /// (x - x0)^k as a factor (`x`, `x^k`, `(x - x0)^k`; k = 0 only inside `O`, as `1`).
    fn series_power(&self, x: T<'h>, x0: T<'h>, k: i64, v: &mut Vec<Task<'h>>) {
        if k == 0 {
            v.push(Task::Text("1"));
            return;
        }
        let st = self.store();
        if matches!(st.view(x0), View::Int(0)) {
            v.push(Task::Expr(x, Ctx::at(POSTFIX)));
        } else {
            v.push(Task::Text("("));
            v.push(Task::Expr(x, Ctx::at(ADD + 1)));
            match self.literal(x0) {
                Some((txt, true)) => v.push(Task::Owned(format!(" + {txt}"))),
                _ => match st.view(x0) {
                    View::Rational { num, den } if self.int_text(num).1 => {
                        v.push(Task::Owned(format!(" + {}/{}", self.int_text(num).0, self.int_text(den).0)))
                    }
                    _ => {
                        v.push(Task::Text(" - "));
                        v.push(Task::Expr(x0, Ctx::at(MUL)));
                    }
                },
            }
            v.push(Task::Text(")"));
        }
        // `x^-2` reads back with the exponent's prefix minus
        if k != 1 {
            v.push(Task::Owned(format!("^{k}")));
        }
    }

    /// A call `head(args)`, with a trailing `options(name -> v, ...)` as named arguments.
    fn call_items(&self, head: T<'h>, args: &[T<'h>], v: &mut Vec<Task<'h>>) {
        v.push(Task::Expr(head, Ctx { min: POSTFIX, no_num: true, ..Ctx::default() }));
        self.args_items(args, v);
    }

    fn args_items(&self, args: &[T<'h>], v: &mut Vec<Task<'h>>) {
        v.push(Task::Text("("));
        let (pos, named) = match args.split_last() {
            Some((last, rest)) => match self.named_args(*last) {
                Some(n) => (rest, n),
                None => (args, Vec::new()),
            },
            None => (args, Vec::new()),
        };
        let mut first = true;
        for &a in pos {
            if !first {
                v.push(Task::Text(", "));
            }
            first = false;
            v.push(Task::Expr(a, Ctx { min: ASSIGN, arg: true, ..Ctx::default() }));
        }
        for (name, val) in named {
            if !first {
                v.push(Task::Text(", "));
            }
            first = false;
            v.push(Task::Owned(format!("{}: ", self.ident(name))));
            v.push(Task::Expr(val, Ctx::at(PIPE)));
        }
        v.push(Task::Text(")"));
    }

    /// `options(n1 -> v1, ...)` with symbol names, at least one.
    fn named_args(&self, t: T<'h>) -> Option<Vec<(Sym, T<'h>)>> {
        let (Head::Options, rules) = self.classify(t)? else { return None };
        if rules.is_empty() {
            return None;
        }
        rules
            .iter()
            .map(|&r| {
                let [n, val] = self.is(r, Head::Rule, 2)? else { return None };
                Some((self.sym_of(*n)?, *val))
            })
            .collect()
    }

    /// A pattern written as one token: `_`, `x_`, `__`, `x__`, `___`, `x___`.
    fn pattern_token(&self, t: T<'h>) -> Option<String> {
        let (h, args) = self.classify(t)?;
        let marks = match h {
            Head::Any => "_",
            Head::Seq1 => "__",
            Head::Seq0 => "___",
            _ => return None,
        };
        match args {
            [] => Some(marks.to_string()),
            [x] => {
                let n = self.name(self.sym_of(*x)?);
                is_pattern_name(&n).then(|| n + marks)
            }
            _ => None,
        }
    }

    /// Prints as a pattern token or as `p: T` with a pattern token `p`: the left sides that `=` turns into
    /// `optional` in an argument.
    fn pattern_like(&self, t: T<'h>) -> bool {
        self.pattern_token(t).is_some()
            || self.is(t, Head::Typed, 2).is_some_and(|a| self.pattern_token(a[0]).is_some() && self.type_printable(a[1]))
    }

    /// The special syntax of a known head, or `None` for the plain call.
    fn special(&mut self, t: T<'h>, h: Head, args: &'h [T<'h>], ctx: Ctx, v: &mut Vec<Task<'h>>) -> Option<(u8, bool)> {
        let st = self.store();
        let bin = |v: &mut Vec<Task<'h>>, a: T<'h>, op: &'static str, b: T<'h>, la: u8, lb: u8| {
            v.push(Task::Expr(a, Ctx::at(la)));
            v.push(Task::Text(op));
            v.push(Task::Expr(b, Ctx::at(lb)));
        };
        let cmp_op = match h {
            Head::Equal => Some(" == "),
            Head::Unequal => Some(" != "),
            Head::Less => Some(" < "),
            Head::LessEqual => Some(" <= "),
            Head::Greater => Some(" > "),
            Head::GreaterEqual => Some(" >= "),
            Head::Same => Some(" === "),
            Head::NotSame => Some(" !== "),
            _ => None,
        };
        if let Some(op) = cmp_op {
            let [a, b] = args else { return None };
            bin(v, *a, op, *b, CMP + 1, CMP + 1);
            return Some((CMP, false));
        }
        match h {
            Head::List => {
                v.push(Task::Text("["));
                self.join(v, args, ", ", Ctx::at(ASSIGN));
                v.push(Task::Text("]"));
                Some((ATOM, false))
            }
            Head::Add if args.len() >= 2 => {
                v.push(Task::Expr(args[0], Ctx::at(ADD + 1)));
                for &a in &args[1..] {
                    self.add_term(a, v);
                }
                Some((ADD, false))
            }
            Head::Mul if args.len() >= 2 => self.mul_form(args, v),
            Head::Pow => {
                let [a, b] = args else { return None };
                v.push(Task::Expr(*a, Ctx::at(POSTFIX)));
                v.push(Task::Text("^"));
                v.push(Task::Expr(*b, Ctx { min: POW, prefix_ok: true, ..Ctx::default() }));
                Some((POW, false))
            }
            Head::Rule | Head::RuleDelayed => {
                let [a, b] = args else { return None };
                bin(v, *a, if h == Head::Rule { " -> " } else { " :> " }, *b, RULE + 1, RULE);
                Some((RULE, false))
            }
            Head::Set => {
                let [a, b] = args else { return None };
                if let Some(s) = self.sym_of(*a) {
                    // A local name would read back as an assignment or an error.
                    if self.scopes.lookup(s).is_some() {
                        return None;
                    }
                } else if ctx.arg && self.pattern_like(*a) {
                    return None;
                }
                bin(v, *a, " = ", *b, RULE, ASSIGN);
                Some((ASSIGN, false))
            }
            Head::SetDelayed => {
                let [a, b] = args else { return None };
                bin(v, *a, " := ", *b, RULE, ASSIGN);
                Some((ASSIGN, false))
            }
            Head::Assign => {
                let [a, b] = args else { return None };
                let s = self.sym_of(*a)?;
                if self.scopes.lookup(s) != Some(Bind::Var) {
                    return None;
                }
                bin(v, *a, " = ", *b, RULE, ASSIGN);
                Some((ASSIGN, false))
            }
            Head::Condition => {
                let [a, b] = args else { return None };
                bin(v, *a, " when ", *b, WHEN, WHEN + 1);
                Some((WHEN, false))
            }
            Head::Alternatives | Head::Or | Head::And if args.len() >= 2 => {
                let (level, op) = match h {
                    Head::Alternatives => (ALT, " | "),
                    Head::Or => (OR, " or "),
                    _ => (AND, " and "),
                };
                self.join(v, args, op, Ctx::at(level + 1));
                Some((level, false))
            }
            Head::Not => {
                let [a] = args else { return None };
                v.push(Task::Text("not "));
                v.push(Task::Expr(*a, Ctx::at(CMP)));
                Some((NOT, false))
            }
            Head::Part if args.len() >= 2 => {
                v.push(Task::Expr(args[0], Ctx { min: POSTFIX, no_num: true, ..Ctx::default() }));
                v.push(Task::Text("["));
                for (i, &ix) in args[1..].iter().enumerate() {
                    if i > 0 {
                        v.push(Task::Text(", "));
                    }
                    match self.is(ix, Head::Span, 2) {
                        Some([a, b]) => {
                            // A pattern token directly before `:` would read as a typed pattern `x_: T`, so the
                            // left side is a postfix form or parenthesised.
                            let force = self.pattern_token(*a).is_some();
                            v.push(Task::Expr(*a, Ctx { min: POSTFIX, force, ..Ctx::default() }));
                            v.push(Task::Text(":"));
                            v.push(Task::Expr(*b, Ctx::at(RULE)));
                        }
                        _ => v.push(Task::Expr(ix, Ctx::at(PIPE))),
                    }
                }
                v.push(Task::Text("]"));
                Some((POSTFIX, false))
            }
            Head::Fn => self.fn_form(args, v),
            Head::Block => {
                self.block_items(args, v);
                Some((ATOM, false))
            }
            Head::If if args.len() == 2 || args.len() == 3 => {
                v.push(Task::Text("if "));
                v.push(Task::Expr(args[0], Ctx::at(ASSIGN)));
                v.push(Task::Text(" then "));
                let force = args.len() == 3 && self.ends_open_if(args[1]);
                v.push(Task::Expr(args[1], Ctx { min: ASSIGN, force, ..Ctx::default() }));
                if args.len() == 3 {
                    v.push(Task::Text(" else "));
                    v.push(Task::Expr(args[2], Ctx::at(ASSIGN)));
                }
                Some((PIPE, false))
            }
            Head::While => {
                let [c, body] = args else { return None };
                self.is_block(*body)?;
                v.push(Task::Text("while "));
                v.push(Task::Expr(*c, Ctx::at(ASSIGN)));
                v.push(Task::Text(" "));
                v.push(Task::Expr(*body, Ctx::at(ATOM)));
                Some((ATOM, false))
            }
            Head::For => {
                let [x, xs, body] = args else { return None };
                let x = self.sym_of(*x)?;
                self.is_block(*body)?;
                v.push(Task::Owned(format!("for {} in ", self.ident(x))));
                v.push(Task::Expr(*xs, Ctx::at(ASSIGN)));
                v.push(Task::Text(" "));
                v.push(Task::PushLets(vec![x]));
                v.push(Task::Expr(*body, Ctx::at(ATOM)));
                v.push(Task::PopFrame);
                Some((ATOM, false))
            }
            Head::Match => {
                let [e, arms] = args else { return None };
                let (Head::List, arms) = self.classify(*arms)? else { return None };
                let arms: Vec<_> = arms.iter().map(|&a| self.is(a, Head::RuleDelayed, 2)).collect::<Option<_>>()?;
                v.push(Task::Text("match "));
                v.push(Task::Expr(*e, Ctx::at(ASSIGN)));
                v.push(Task::Text(" {"));
                for (i, arm) in arms.iter().enumerate() {
                    v.push(Task::Text(if i > 0 { ", " } else { " " }));
                    v.push(Task::Expr(arm[0], Ctx::at(WHEN)));
                    v.push(Task::Text(" => "));
                    v.push(Task::PushLets(pattern_vars(st, self.heads(), arm[0])));
                    v.push(Task::Expr(arm[1], Ctx::at(PIPE)));
                    v.push(Task::PopFrame);
                }
                v.push(Task::Text(" }"));
                Some((ATOM, false))
            }
            Head::Any | Head::Seq1 | Head::Seq0 => {
                v.push(Task::Owned(self.pattern_token(t)?));
                Some((ATOM, false))
            }
            Head::Typed => {
                let [p, ty] = args else { return None };
                let tok = self.pattern_token(*p)?;
                if !self.type_printable(*ty) {
                    return None;
                }
                v.push(Task::Owned(tok + ": "));
                v.push(Task::Type(*ty, 0));
                // The type's right edge takes a following `|` (as a union) and, for a `where` type, everything up to
                // `=`; so a typed pattern is parenthesised wherever such an operator may follow it.
                // A type ending in a parenthesised group also takes a following `->` (as a function type).
                let level = match self.type_shape(*ty) {
                    TypeShape::Where(..) => PIPE,
                    _ if self.type_ends_grouped(*ty) => RULE,
                    _ => ALT,
                };
                Some((level, false))
            }
            Head::Named => {
                let [x, p] = args else { return None };
                let x = self.sym_of(*x)?;
                v.push(Task::Owned(format!("{} @ ", self.ident(x))));
                v.push(Task::Expr(*p, Ctx::at(POSTFIX)));
                // `x @ p` takes any postfix that follows it, so it is not itself a postfix form.
                Some((POW, false))
            }
            Head::Optional if ctx.arg => {
                let [p, d] = args else { return None };
                if !self.pattern_like(*p) {
                    return None;
                }
                bin(v, *p, " = ", *d, RULE, ASSIGN);
                Some((ASSIGN, false))
            }
            _ => None,
        }
    }

    fn join(&self, v: &mut Vec<Task<'h>>, items: &[T<'h>], sep: &'static str, ctx: Ctx) {
        for (i, &a) in items.iter().enumerate() {
            if i > 0 {
                v.push(Task::Text(sep));
            }
            v.push(Task::Expr(a, ctx));
        }
    }

    /// A non-first operand of a sum: ` - b` for `mul(-1, b)` and for negative literals, ` + a` otherwise.
    fn add_term(&self, a: T<'h>, v: &mut Vec<Task<'h>>) {
        let st = self.store();
        if let Some((txt, true)) = self.literal(a) {
            v.push(Task::Owned(format!(" - {txt}")));
            return;
        }
        if let View::Rational { num, den } = st.view(a) {
            let (n, neg) = self.int_text(num);
            if neg {
                let (d, _) = self.int_text(den);
                v.push(Task::Owned(format!(" - {n}/{d}")));
                return;
            }
        }
        if let Some([m, b]) = self.is(a, Head::Mul, 2) {
            // `a - 7` and `a - 3/4` read back as negative literals, so `mul(-1, 7)` keeps its explicit form.
            if matches!(st.view(*m), View::Int(-1)) && !self.is_num_token(*b) && !self.is_rational(*b) {
                v.push(Task::Text(" - "));
                v.push(Task::Expr(*b, Ctx::at(MUL)));
                return;
            }
        }
        v.push(Task::Text(" + "));
        v.push(Task::Expr(a, Ctx::at(ADD + 1)));
    }

    fn mul_form(&self, args: &'h [T<'h>], v: &mut Vec<Task<'h>>) -> Option<(u8, bool)> {
        let st = self.store();
        let is_int = |t: T<'h>| matches!(st.view(t), View::Int(_) | View::BigInt { .. });
        let recip = |t: T<'h>| -> Option<T<'h>> {
            let [b, e] = self.is(t, Head::Pow, 2)? else { return None };
            matches!(st.view(*e), View::Int(-1)).then_some(*b)
        };
        if let [m, b] = args {
            // A prefix `-` on a number token reads back as a negative literal; on anything else (a parenthesised
            // rational included) as `mul(-1, b)`.
            if matches!(st.view(*m), View::Int(-1)) && !self.is_num_token(*b) {
                v.push(Task::Text("-"));
                v.push(Task::Expr(*b, Ctx::at(POW)));
                return Some((NEG, true));
            }
        }
        // `p/q` with integer literals reads back as the exact rational, so `mul(p, pow(q, -1))` is written `p*q^-1`.
        let rational_like = match args {
            [p, r] => {
                is_int(*p)
                    && recip(*r).is_some_and(|q| match st.view(q) {
                        View::Int(q) => q > 0,
                        View::BigInt { neg, .. } => !neg,
                        _ => false,
                    })
            }
            _ => false,
        };
        v.push(Task::Expr(args[0], Ctx::at(NEG)));
        for &a in &args[1..] {
            match recip(a) {
                Some(b) if !rational_like => {
                    v.push(Task::Text("/"));
                    v.push(Task::Expr(b, Ctx::at(NEG)));
                }
                _ => {
                    v.push(Task::Text("*"));
                    v.push(Task::Expr(a, Ctx::at(NEG)));
                }
            }
        }
        Some((MUL, false))
    }

    fn is_block(&self, t: T<'h>) -> Option<()> {
        matches!(self.classify(t), Some((Head::Block, _))).then_some(())
    }

    fn block_items(&self, stmts: &[T<'h>], v: &mut Vec<Task<'h>>) {
        if stmts.is_empty() {
            v.push(Task::Text("{ }"));
            return;
        }
        v.push(Task::Text("{ "));
        v.push(Task::PushFrame);
        for (i, &s) in stmts.iter().enumerate() {
            if i > 0 {
                v.push(Task::Text("; "));
            }
            v.push(Task::Stmt(s, Mode::Block));
        }
        v.push(Task::PopFrame);
        v.push(Task::Text(" }"));
    }

    /// Whether the right edge of `t`, printed without parentheses, is an `if` without `else` (which would take the
    /// `else` of an enclosing `if`).
    fn ends_open_if(&self, mut t: T<'h>) -> bool {
        loop {
            match self.classify(t) {
                Some((Head::If, [_, _])) => return true,
                Some((Head::If, [_, _, b])) => t = *b,
                Some((Head::Fn | Head::Set | Head::SetDelayed | Head::Assign, [_, b])) => t = *b,
                _ => return false,
            }
        }
    }

    // ---- functions ----

    /// The largest slot `&k` occurring in `t` outside the slot functions nested in it. A term `fn(list(&1, ..., &n),
    /// body)` with `n >= 1` and `slots(body) == n` prints as `&(body)` and contributes nothing to its parent; this
    /// mirrors the count lowering makes over the syntax tree.
    fn slots(&mut self, t: T<'h>) -> u32 {
        let st = self.store();
        let mut stack = vec![(t, false)];
        while let Some((t, expanded)) = stack.pop() {
            if self.slot_memo.contains_key(&t) {
                continue;
            }
            match st.view(t) {
                View::Sym(s) => {
                    let k = slot_index(&self.name(s)).unwrap_or(0);
                    self.slot_memo.insert(t, k);
                }
                View::App { head, args } => {
                    if !expanded {
                        stack.push((t, true));
                        stack.push((head, false));
                        stack.extend(args.iter().map(|&a| (a, false)));
                        continue;
                    }
                    let m = |x: &T<'h>| self.slot_memo.get(x).copied().unwrap_or(0);
                    let k = if self.slot_fn_arity(t).is_some_and(|n| m(&args[1]) == n) {
                        0
                    } else {
                        args.iter().map(m).max().unwrap_or(0).max(m(&head))
                    };
                    self.slot_memo.insert(t, k);
                }
                _ => {
                    self.slot_memo.insert(t, 0);
                }
            }
        }
        self.slot_memo.get(&t).copied().unwrap_or(0)
    }

    /// `n` when `t` is `fn(list(&1, ..., &n), body)` with `n >= 1`.
    fn slot_fn_arity(&self, t: T<'h>) -> Option<u32> {
        let [ps, _] = self.is(t, Head::Fn, 2)? else { return None };
        let (Head::List, ps) = self.classify(*ps)? else { return None };
        if ps.is_empty() {
            return None;
        }
        for (i, &p) in ps.iter().enumerate() {
            if slot_index(&self.name(self.sym_of(p)?)) != Some(i as u32 + 1) {
                return None;
            }
        }
        Some(ps.len() as u32)
    }

    fn fn_form(&mut self, args: &'h [T<'h>], v: &mut Vec<Task<'h>>) -> Option<(u8, bool)> {
        let st = self.store();
        let [ps, body] = args else { return None };
        let (Head::List, params) = self.classify(*ps)? else { return None };
        let fn_term = st.call(self.heads().sym(Head::Fn), args);
        if let Some(n) = self.slot_fn_arity(fn_term) {
            if self.slots(*body) == n {
                v.push(Task::Text("&("));
                v.push(Task::SlotEnter);
                v.push(Task::PushLets(params.iter().filter_map(|&p| self.sym_of(p)).collect()));
                v.push(Task::Expr(*body, Ctx::at(ASSIGN)));
                v.push(Task::PopFrame);
                v.push(Task::SlotExit);
                v.push(Task::Text(")"));
                return Some((ATOM, false));
            }
        }
        if !params.iter().all(|&p| self.lambda_param_ok(p)) {
            return None;
        }
        match params {
            [p] if self.sym_of(*p).is_some() => v.push(Task::Owned(self.ident(self.sym_of(*p)?))),
            _ => {
                v.push(Task::Text("("));
                for (i, &p) in params.iter().enumerate() {
                    if i > 0 {
                        v.push(Task::Text(", "));
                    }
                    v.push(Task::Param(p, ParamMode::Lambda));
                }
                v.push(Task::Text(")"));
            }
        }
        v.push(Task::Text(" => "));
        v.push(Task::PushLets(lambda_param_names(st, self.heads(), params)));
        v.push(Task::Expr(*body, Ctx::at(PIPE)));
        v.push(Task::PopFrame);
        Some((PIPE, false))
    }

    /// Removes `held` and `optional` wrappers: the core parameter and whether a default was present.
    fn strip_param(&self, mut t: T<'h>) -> (T<'h>, bool) {
        if let Some([q]) = self.is(t, Head::Held, 1) {
            t = *q;
        }
        if let Some([q, _]) = self.is(t, Head::Optional, 2) {
            return (*q, true);
        }
        (t, false)
    }

    fn lambda_param_ok(&self, t: T<'h>) -> bool {
        let (q, default) = self.strip_param(t);
        if self.sym_of(q).is_some() {
            return true;
        }
        if let Some([x, ty]) = self.is(q, Head::Typed, 2) {
            return self.sym_of(*x).is_some() && self.type_printable(*ty);
        }
        if let Some([x]) = self.is(q, Head::Rest, 1) {
            return !default && self.sym_of(*x).is_some();
        }
        false
    }

    fn fn_param_ok(&self, t: T<'h>) -> bool {
        let (q, default) = self.strip_param(t);
        if self.sym_of(q).is_some() {
            // A bare name parameter reads back as `any(x)`.
            return false;
        }
        if let Some([x, ty]) = self.is(q, Head::Typed, 2) {
            if self.sym_of(*x).is_some() || (self.any_name(*x).is_some() && !self.type_printable(*ty)) {
                return false;
            }
        }
        if let Some([x]) = self.is(q, Head::Seq0, 1) {
            return !default || self.sym_of(*x).is_none();
        }
        true
    }

    /// The name `x` of `any(x)`.
    fn any_name(&self, t: T<'h>) -> Option<Sym> {
        self.is(t, Head::Any, 1).and_then(|a| self.sym_of(a[0]))
    }

    /// A parameter validated by [`Run::lambda_param_ok`] or [`Run::fn_param_ok`]: one `held`, one default, and the
    /// core form, exactly as those strip them.
    fn param_form(&self, t: T<'h>, mode: ParamMode, v: &mut Vec<Task<'h>>) {
        if self.is(t, Head::Held, 1).is_some() {
            v.push(Task::Text("held "));
        }
        let (q, default) = self.strip_param(t);
        let named = |x: Sym, suffix: &str| Task::Owned(format!("{}{suffix}", self.ident(x)));
        match mode {
            ParamMode::Lambda => {
                if let Some(s) = self.sym_of(q) {
                    v.push(named(s, ""));
                } else if let Some([x, ty]) = self.is(q, Head::Typed, 2) {
                    v.push(named(self.sym_of(*x).unwrap_or(self.p.heads.null), ": "));
                    v.push(Task::Type(*ty, 0));
                } else if let Some([x]) = self.is(q, Head::Rest, 1) {
                    v.push(named(self.sym_of(*x).unwrap_or(self.p.heads.null), "..."));
                }
            }
            ParamMode::Fn => {
                if let Some(s) = self.any_name(q) {
                    v.push(named(s, ""));
                } else if let Some(a) = self.is(q, Head::Typed, 2).filter(|a| self.any_name(a[0]).is_some()) {
                    v.push(named(self.any_name(a[0]).unwrap_or(self.p.heads.null), ": "));
                    v.push(Task::Type(a[1], 0));
                } else if let Some(s) = self.is(q, Head::Seq0, 1).and_then(|a| self.sym_of(a[0])) {
                    v.push(named(s, "..."));
                } else {
                    v.push(Task::Expr(q, Ctx::at(RULE)));
                }
            }
        }
        if default {
            if let Some([_, d]) = self.is(self.is(t, Head::Held, 1).map_or(t, |a| a[0]), Head::Optional, 2) {
                v.push(Task::Text(" = "));
                v.push(Task::Expr(*d, Ctx::at(PIPE)));
            }
        }
    }

    // ---- statements ----

    /// The statement syntax of `t` in `mode`; `false` when `t` prints as an expression statement.
    fn stmt_form(&mut self, t: T<'h>, mode: Mode, v: &mut Vec<Task<'h>>) -> bool {
        let st = self.store();
        let Some((h, args)) = self.classify(t) else { return false };
        let defs = mode != Mode::Block;
        match h {
            Head::Let | Head::Var => {
                let Some((x, bind)) = statement_binding(st, self.heads(), t) else { return false };
                let ty = self.is(args[0], Head::Typed, 2).map(|a| a[1]);
                if ty.is_some_and(|ty| !self.type_printable(ty)) {
                    return false;
                }
                v.push(Task::Owned(format!("{} {}", if h == Head::Let { "let" } else { "var" }, self.ident(x))));
                if let Some(ty) = ty {
                    v.push(Task::Text(": "));
                    v.push(Task::Type(ty, 0));
                }
                v.push(Task::Text(" = "));
                v.push(Task::Expr(args[1], Ctx::at(ASSIGN)));
                v.push(Task::Bind(x, bind));
                true
            }
            Head::DefineFn if defs => self.define_fn(args, v),
            Head::DefineRule if defs => self.define_rule(args, v),
            Head::DefineSym if defs => {
                let [name, decs] = args else { return false };
                let (Some(name), true) = (self.sym_of(*name), self.decorators(*decs, v)) else { return false };
                v.push(Task::Owned(format!("sym {}", self.ident(name))));
                true
            }
            Head::Module if defs => {
                let [name, body] = args else { return false };
                let (Some(name), Some((Head::List, stmts))) = (self.sym_of(*name), self.classify(*body)) else { return false };
                v.push(Task::Owned(format!("module {} {{", self.ident(name))));
                v.push(Task::PushFrame);
                for (i, &s) in stmts.iter().enumerate() {
                    v.push(Task::Text(if i > 0 { "; " } else { " " }));
                    v.push(Task::Stmt(s, Mode::Module));
                }
                v.push(Task::PopFrame);
                v.push(Task::Text(" }"));
                true
            }
            Head::Public if mode == Mode::Module => {
                let [inner] = args else { return false };
                if matches!(
                    self.classify(*inner),
                    Some((Head::Public | Head::Module | Head::Import | Head::ImportExcept, _))
                ) {
                    return false;
                }
                v.push(Task::Text("pub "));
                v.push(Task::Stmt(*inner, Mode::Module));
                true
            }
            Head::Import | Head::ImportExcept if defs => {
                let Some(path) = args.first().and_then(|&p| self.sym_of(p)) else { return false };
                let names: Option<Vec<Sym>> = match args {
                    [_] if h == Head::Import => Some(Vec::new()),
                    [_, list] => match self.classify(*list) {
                        Some((Head::List, ns)) => ns.iter().map(|&n| self.sym_of(n)).collect(),
                        _ => None,
                    },
                    _ => None,
                };
                let Some(names) = names else { return false };
                let list = names.iter().map(|&n| self.ident(n)).collect::<Vec<_>>().join(", ");
                let text = match (h, args.len()) {
                    (Head::Import, 1) => format!("import {}", self.ident(path)),
                    (Head::Import, _) => format!("import {}.{{{list}}}", self.ident(path)),
                    _ if names.is_empty() => return false,
                    _ => format!("import {} except {list}", self.ident(path)),
                };
                v.push(Task::Owned(text));
                true
            }
            _ => false,
        }
    }

    /// Decorators `@d @e(x) ` in front of a definition; `false` if `decs` is not a list of them.
    fn decorators(&self, decs: T<'h>, v: &mut Vec<Task<'h>>) -> bool {
        let st = self.store();
        let Some((Head::List, ds)) = self.classify(decs) else { return false };
        for &d in ds {
            if let Some(s) = self.sym_of(d) {
                v.push(Task::Owned(format!("@{} ", self.ident(s))));
                continue;
            }
            let View::App { head, args } = st.view(d) else { return false };
            let Some(s) = self.sym_of(head) else { return false };
            v.push(Task::Owned(format!("@{}", self.ident(s))));
            self.args_items(args, v);
            v.push(Task::Text(" "));
        }
        true
    }

    fn define_fn(&mut self, args: &'h [T<'h>], v: &mut Vec<Task<'h>>) -> bool {
        let st = self.store();
        let [lhs, body, opts, result, decs] = args else { return false };
        // lhs is f(params), condition(f(params), c0) or condition(condition(f(params), c0), c1).
        let mut call = *lhs;
        let mut conds = Vec::new();
        while let Some([inner, c]) = self.is(call, Head::Condition, 2) {
            conds.push(*c);
            call = *inner;
            if conds.len() > 2 {
                return false;
            }
        }
        conds.reverse();
        let View::App { head: f, args: params } = st.view(call) else { return false };
        let Some(f) = self.sym_of(f) else { return false };
        let Some((Head::List, options)) = self.classify(*opts) else { return false };
        let no_result = self.sym_of(*result) == Some(self.p.heads.null);
        let ok = self.is(*body, Head::Condition, 2).is_none()
            && params.iter().chain(options).all(|&p| self.fn_param_ok(p))
            && (no_result || self.type_printable(*result));
        if !ok {
            return false;
        }
        let mut d = Vec::new();
        if !self.decorators(*decs, &mut d) {
            return false;
        }
        v.extend(d);
        v.push(Task::Owned(format!("fn {}(", self.ident(f))));
        for (i, &p) in params.iter().enumerate() {
            if i > 0 {
                v.push(Task::Text(", "));
            }
            v.push(Task::Param(p, ParamMode::Fn));
        }
        if !options.is_empty() {
            v.push(Task::Text("; "));
            for (i, &p) in options.iter().enumerate() {
                if i > 0 {
                    v.push(Task::Text(", "));
                }
                v.push(Task::Param(p, ParamMode::Fn));
            }
        }
        v.push(Task::Text(")"));
        let mut vars = pattern_vars(st, self.heads(), call);
        vars.extend(pattern_vars(st, self.heads(), *opts));
        v.push(Task::PushLets(vars));
        if !no_result {
            v.push(Task::Text(": "));
            // A `where` type would take the `when` or `=` that follows.
            v.push(Task::Type(*result, 1));
        }
        if let Some(&c0) = conds.first() {
            v.push(Task::Text(" when "));
            v.push(Task::Expr(c0, Ctx::at(RULE)));
        }
        v.push(Task::Text(" = "));
        match conds.get(1) {
            Some(&c1) => {
                v.push(Task::Expr(*body, Ctx::at(WHEN + 1)));
                v.push(Task::Text(" when "));
                v.push(Task::Expr(c1, Ctx::at(WHEN + 1)));
            }
            None => v.push(Task::Expr(*body, Ctx::at(ASSIGN))),
        }
        v.push(Task::PopFrame);
        true
    }

    fn define_rule(&mut self, args: &'h [T<'h>], v: &mut Vec<Task<'h>>) -> bool {
        let st = self.store();
        let [rule, decs] = args else { return false };
        let (delayed, [lhs, rhs]) = match self.classify(*rule) {
            Some((Head::RuleDelayed, [l, r])) => (true, [*l, *r]),
            Some((Head::Rule, [l, r])) => (false, [*l, *r]),
            _ => return false,
        };
        if self.is(rhs, Head::Condition, 2).is_some() {
            return false;
        }
        let mut d = Vec::new();
        if !self.decorators(*decs, &mut d) {
            return false;
        }
        v.extend(d);
        let (l, cond) = match self.is(lhs, Head::Condition, 2) {
            Some([l, c]) => (*l, Some(*c)),
            _ => (lhs, None),
        };
        v.push(Task::Text("rule "));
        v.push(Task::Expr(l, Ctx::at(RULE)));
        v.push(Task::Text(if delayed { " := " } else { " = " }));
        v.push(Task::PushLets(pattern_vars(st, self.heads(), l)));
        match cond {
            Some(c) => {
                v.push(Task::Expr(rhs, Ctx::at(WHEN + 1)));
                v.push(Task::Text(" when "));
                v.push(Task::Expr(c, Ctx::at(WHEN + 1)));
            }
            None => v.push(Task::Expr(rhs, Ctx::at(ASSIGN))),
        }
        v.push(Task::PopFrame);
        true
    }

    // ---- types ----

    /// Whether `t` prints in the type syntax: symbols, non-negative integers, `Name[T, ...]`, function types,
    /// unions and `where` types.
    fn type_printable(&self, t: T<'h>) -> bool {
        let st = self.store();
        let mut stack = vec![t];
        while let Some(t) = stack.pop() {
            match st.view(t) {
                View::Sym(_) => {}
                View::Int(x) if x >= 0 => {}
                View::BigInt { neg: false, .. } => {}
                View::App { head, args } => {
                    if self.sym_of(head).is_none() {
                        return false;
                    }
                    match self.type_shape(t) {
                        TypeShape::Fun(ps, r) => {
                            stack.extend(ps);
                            stack.push(r);
                        }
                        TypeShape::Union(items) => stack.extend(items),
                        TypeShape::Where(t, _) => stack.push(t),
                        TypeShape::App if !args.is_empty() => stack.extend(args),
                        TypeShape::App => return false,
                    }
                }
                _ => return false,
            }
        }
        true
    }

    fn type_shape(&self, t: T<'h>) -> TypeShape<'h> {
        if let Some([ps, r]) = self.is(t, Head::FunType, 2) {
            if let Some((Head::List, ps)) = self.classify(*ps) {
                return TypeShape::Fun(ps, *r);
            }
        }
        if let Some((Head::TypeUnion, items)) = self.classify(t) {
            if items.len() >= 2 {
                return TypeShape::Union(items);
            }
        }
        if let Some([t, p]) = self.is(t, Head::TypeWhere, 2) {
            return TypeShape::Where(*t, *p);
        }
        TypeShape::App
    }

    /// The level [`Run::type_form`] gives a printable type.
    fn type_level(&self, t: T<'h>) -> u8 {
        match self.type_shape(t) {
            TypeShape::Where(..) => 0,
            TypeShape::Union(_) => 1,
            _ => 2,
        }
    }

    /// Whether the printed type ends with a parenthesised group: the last member of a union or the result of a
    /// function type is a union or a `where` type.
    fn type_ends_grouped(&self, mut t: T<'h>) -> bool {
        loop {
            let last = match self.type_shape(t) {
                TypeShape::Union(items) => match items.last() {
                    Some(&m) => m,
                    None => return false,
                },
                TypeShape::Fun(_, r) => r,
                _ => return false,
            };
            if self.type_level(last) < 2 {
                return true;
            }
            t = last;
        }
    }

    /// Fills `v` for a printable type and returns its level: 0 `where`, 1 union, 2 atom.
    fn type_form(&self, t: T<'h>, v: &mut Vec<Task<'h>>) -> u8 {
        let st = self.store();
        match st.view(t) {
            View::Sym(s) => {
                v.push(Task::Owned(self.ident(s)));
                2
            }
            View::Int(_) | View::BigInt { .. } => {
                v.push(Task::Owned(self.int_text(t).0));
                2
            }
            View::App { head, args } => match self.type_shape(t) {
                TypeShape::Fun(ps, r) => {
                    v.push(Task::Text("("));
                    for (i, &p) in ps.iter().enumerate() {
                        if i > 0 {
                            v.push(Task::Text(", "));
                        }
                        v.push(Task::Type(p, 0));
                    }
                    v.push(Task::Text(") -> "));
                    v.push(Task::Type(r, 2));
                    2
                }
                TypeShape::Union(items) => {
                    for (i, &m) in items.iter().enumerate() {
                        if i > 0 {
                            v.push(Task::Text(" | "));
                        }
                        // Members are atoms: a nested union keeps its parentheses.
                        v.push(Task::Type(m, 2));
                    }
                    1
                }
                TypeShape::Where(t, p) => {
                    v.push(Task::Type(t, 1));
                    v.push(Task::Text(" where "));
                    v.push(Task::Expr(p, Ctx::at(RULE)));
                    0
                }
                TypeShape::App => {
                    let name = self.sym_of(head).map(|s| self.ident(s)).unwrap_or_default();
                    v.push(Task::Owned(name + "["));
                    for (i, &a) in args.iter().enumerate() {
                        if i > 0 {
                            v.push(Task::Text(", "));
                        }
                        v.push(Task::Type(a, 0));
                    }
                    v.push(Task::Text("]"));
                    2
                }
            },
            _ => 2,
        }
    }
}

enum TypeShape<'h> {
    Fun(&'h [T<'h>], T<'h>),
    Union(&'h [T<'h>]),
    Where(T<'h>, T<'h>),
    App,
}

/// Text from a [`NumberFormat`]: its level from its first character.
fn text_form<'h>(s: String, v: &mut Vec<Task<'h>>) -> (u8, bool) {
    let r = if s.starts_with('-') {
        (NEG, true)
    } else if s.starts_with(|c: char| c.is_ascii_digit()) {
        (ATOM, false)
    } else {
        (POSTFIX, false)
    };
    v.push(Task::Owned(s));
    r
}
