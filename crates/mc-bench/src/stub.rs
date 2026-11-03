//! A tiny stand-in engine: integer arithmetic in a subset of the concrete syntax, enough to run sample cases end to
//! end and to drive the REPL before the real evaluator exists (decision D.3).
//!
//! The subset: statements separated by `;` or new lines, `name = expr` definitions, integers (`1_000` allowed),
//! `true`, `false`, lists `[a, b]`, `+ - * / ^` with the precedences of 04-syntax section 3.1, unary minus,
//! comparisons `== != < <= > >=` (non-associative), `#` comments, and these functions:
//!
//! | Function | Value |
//! |---|---|
//! | `range(n)` | `[1, 2, ..., n]` |
//! | `sum(xs)`, `length(xs)` | sum and length of a list |
//! | `fact(n)`, `fib(n)` | factorial and Fibonacci number |
//! | `busy(n)` | `n`, after `n` steps of busy work that poll the stop flag |
//! | `busy_blind(n)` | `n`, after `n` steps that do not poll it (to test the guard's hard kill) |
//!
//! Integers are 128-bit; overflow and inexact division are errors, not results. Names that are not defined are
//! errors too, unlike in the real language, where they stay symbolic.
use crate::check::fnv1a;
use crate::engine::{Engine, EngineError, Evaluated, StopFlag};
use std::collections::HashMap;
use std::fmt;
use std::rc::Rc;

/// The stub engine and its session (the defined names).
#[derive(Default)]
pub struct StubEngine {
    vars: HashMap<String, Value>,
    last: Option<Value>,
}

impl StubEngine {
    /// A fresh session.
    pub fn new() -> StubEngine {
        StubEngine::default()
    }
}

impl Engine for StubEngine {
    fn name(&self) -> String {
        "stub 0".into()
    }

    fn eval(&mut self, src: &str, stop: &StopFlag) -> Result<Evaluated, EngineError> {
        let program = Parser::new(src)?.program()?;
        let mut value = Value::Bool(true);
        for statement in &program {
            value = match statement {
                Statement::Define(name, e) => {
                    let v = self.eval_expr(e, stop)?;
                    self.vars.insert(name.clone(), v.clone());
                    v
                }
                Statement::Expr(e) => self.eval_expr(e, stop)?,
            };
        }
        let printed = value.to_string();
        self.last = Some(value);
        Ok(Evaluated { fingerprint: Some(fnv1a(printed.as_bytes())), printed })
    }

    fn bind_last(&mut self, name: &str) -> Result<(), EngineError> {
        let v = self.last.clone().ok_or_else(|| EngineError::Other("nothing evaluated yet".into()))?;
        self.vars.insert(name.to_string(), v);
        Ok(())
    }
}

/// A stub value.
#[derive(Clone, Debug, PartialEq)]
enum Value {
    Int(i128),
    Bool(bool),
    List(Rc<Vec<Value>>),
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Int(i) => write!(f, "{i}"),
            Value::Bool(b) => write!(f, "{b}"),
            Value::List(xs) => {
                write!(f, "[")?;
                for (i, x) in xs.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{x}")?;
                }
                write!(f, "]")
            }
        }
    }
}

fn err(message: impl Into<String>) -> EngineError {
    EngineError::Other(format!("stub: {}", message.into()))
}

/// How many loop steps run between polls of the stop flag.
const POLL_EVERY: u64 = 1 << 12;

impl StubEngine {
    fn eval_expr(&self, e: &Expr, stop: &StopFlag) -> Result<Value, EngineError> {
        // Recursion here follows the parse tree, whose depth the parser bounds (MAX_DEPTH).
        Ok(match e {
            Expr::Int(i) => Value::Int(*i),
            Expr::Bool(b) => Value::Bool(*b),
            Expr::Var(name) => self.vars.get(name).cloned().ok_or_else(|| err(format!("unknown name {name}")))?,
            Expr::List(items) => {
                Value::List(Rc::new(items.iter().map(|x| self.eval_expr(x, stop)).collect::<Result<_, _>>()?))
            }
            Expr::Neg(x) => Value::Int(int(&self.eval_expr(x, stop)?)?.checked_neg().ok_or_else(overflow)?),
            Expr::Bin(op, a, b) => binary(*op, self.eval_expr(a, stop)?, self.eval_expr(b, stop)?)?,
            Expr::Call(name, args) => {
                let args: Vec<Value> = args.iter().map(|x| self.eval_expr(x, stop)).collect::<Result<_, _>>()?;
                call(name, &args, stop)?
            }
        })
    }
}

fn overflow() -> EngineError {
    err("integer overflow")
}

fn int(v: &Value) -> Result<i128, EngineError> {
    match v {
        Value::Int(i) => Ok(*i),
        other => Err(err(format!("expected an integer, got {other}"))),
    }
}

fn count(v: &Value) -> Result<u64, EngineError> {
    u64::try_from(int(v)?).map_err(|_| err("expected a nonnegative count"))
}

fn binary(op: Op, a: Value, b: Value) -> Result<Value, EngineError> {
    if op == Op::Eq {
        return Ok(Value::Bool(a == b));
    }
    if op == Op::Ne {
        return Ok(Value::Bool(a != b));
    }
    let (x, y) = (int(&a)?, int(&b)?);
    Ok(match op {
        Op::Add => Value::Int(x.checked_add(y).ok_or_else(overflow)?),
        Op::Sub => Value::Int(x.checked_sub(y).ok_or_else(overflow)?),
        Op::Mul => Value::Int(x.checked_mul(y).ok_or_else(overflow)?),
        Op::Div if y == 0 => return Err(err("division by zero")),
        Op::Div if x % y != 0 => return Err(err(format!("{x}/{y} is not an integer"))),
        Op::Div => Value::Int(x / y),
        Op::Pow => {
            let e = u32::try_from(y).map_err(|_| err("exponent must be a nonnegative machine integer"))?;
            Value::Int(x.checked_pow(e).ok_or_else(overflow)?)
        }
        Op::Lt => Value::Bool(x < y),
        Op::Le => Value::Bool(x <= y),
        Op::Gt => Value::Bool(x > y),
        Op::Ge => Value::Bool(x >= y),
        Op::Eq | Op::Ne => unreachable!("handled above"),
    })
}

fn call(name: &str, args: &[Value], stop: &StopFlag) -> Result<Value, EngineError> {
    let [arg] = args else { return Err(err(format!("{name} takes one argument"))) };
    let poll =
        |i: u64| if i.is_multiple_of(POLL_EVERY) && stop.is_raised() { Err(EngineError::Stopped) } else { Ok(()) };
    Ok(match name {
        "range" => {
            let n = count(arg)?;
            let mut xs = Vec::new();
            for i in 1..=n {
                poll(i)?;
                xs.push(Value::Int(i as i128));
            }
            Value::List(Rc::new(xs))
        }
        "sum" | "length" => {
            let Value::List(xs) = arg else { return Err(err(format!("{name} expects a list"))) };
            if name == "length" {
                Value::Int(xs.len() as i128)
            } else {
                let mut s: i128 = 0;
                for x in xs.iter() {
                    s = s.checked_add(int(x)?).ok_or_else(overflow)?;
                }
                Value::Int(s)
            }
        }
        "fact" => {
            let mut p: i128 = 1;
            for k in 2..=count(arg)? {
                poll(k)?;
                p = p.checked_mul(k as i128).ok_or_else(overflow)?;
            }
            Value::Int(p)
        }
        "fib" => {
            let (mut a, mut b): (i128, i128) = (0, 1);
            for k in 0..count(arg)? {
                poll(k)?;
                (a, b) = (b, a.checked_add(b).ok_or_else(overflow)?);
            }
            Value::Int(a)
        }
        "busy" | "busy_blind" => {
            let n = count(arg)?;
            let polls = name == "busy";
            let mut acc = 0u64;
            for k in 0..n {
                if polls {
                    poll(k)?;
                }
                acc = std::hint::black_box(acc.wrapping_mul(6364136223846793005).wrapping_add(k));
            }
            std::hint::black_box(acc);
            Value::Int(n as i128)
        }
        _ => return Err(err(format!("unknown function {name}"))),
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Op {
    Add,
    Sub,
    Mul,
    Div,
    Pow,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

#[derive(Debug)]
enum Expr {
    Int(i128),
    Bool(bool),
    Var(String),
    List(Vec<Expr>),
    Neg(Box<Expr>),
    Bin(Op, Box<Expr>, Box<Expr>),
    Call(String, Vec<Expr>),
}

enum Statement {
    Define(String, Expr),
    Expr(Expr),
}

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Int(i128),
    Name(String),
    Punct(&'static str),
    /// A statement separator: `;` or a new line outside brackets.
    Sep,
}

/// Parse trees deeper than this are rejected, which bounds the evaluator's recursion.
const MAX_DEPTH: usize = 200;

struct Parser {
    toks: Vec<(Tok, usize)>,
    pos: usize,
    end: usize,
    depth: usize,
}

const PUNCT: [&str; 17] = ["==", "!=", "<=", ">=", "<", ">", "+", "-", "*", "/", "^", "(", ")", "[", "]", ",", "="];

impl Parser {
    fn new(src: &str) -> Result<Parser, EngineError> {
        let bytes = src.as_bytes();
        let mut toks = Vec::new();
        let mut i = 0;
        let mut brackets = 0usize;
        while i < bytes.len() {
            let c = bytes[i];
            match c {
                b'#' => {
                    while i < bytes.len() && bytes[i] != b'\n' {
                        i += 1;
                    }
                }
                b'\n' | b';' => {
                    if brackets == 0 || c == b';' {
                        toks.push((Tok::Sep, i));
                    }
                    i += 1;
                }
                _ if c.is_ascii_whitespace() => i += 1,
                b'0'..=b'9' => {
                    let start = i;
                    let mut v: i128 = 0;
                    while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'_') {
                        if bytes[i] != b'_' {
                            v = v
                                .checked_mul(10)
                                .and_then(|v| v.checked_add((bytes[i] - b'0') as i128))
                                .ok_or(EngineError::Syntax { message: "integer too large".into(), offset: start })?;
                        }
                        i += 1;
                    }
                    toks.push((Tok::Int(v), start));
                }
                _ if c.is_ascii_alphabetic() || c == b'_' => {
                    let start = i;
                    while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                        i += 1;
                    }
                    toks.push((Tok::Name(src[start..i].to_string()), start));
                }
                _ => {
                    let Some(p) = PUNCT.iter().find(|p| bytes[i..].starts_with(p.as_bytes())) else {
                        let ch = src[i..].chars().next().unwrap_or('?');
                        return Err(EngineError::Syntax { message: format!("unexpected character {ch:?}"), offset: i });
                    };
                    match *p {
                        "(" | "[" => brackets += 1,
                        ")" | "]" => brackets = brackets.saturating_sub(1),
                        _ => {}
                    }
                    toks.push((Tok::Punct(p), i));
                    i += p.len();
                }
            }
        }
        Ok(Parser { toks, pos: 0, end: src.len(), depth: 0 })
    }

    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos).map(|(t, _)| t)
    }

    fn offset(&self) -> usize {
        self.toks.get(self.pos).map(|&(_, o)| o).unwrap_or(self.end)
    }

    fn error(&self, message: impl Into<String>) -> EngineError {
        EngineError::Syntax { message: message.into(), offset: self.offset() }
    }

    fn eat(&mut self, p: &str) -> bool {
        if self.at(p) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn at(&self, p: &str) -> bool {
        matches!(self.peek(), Some(Tok::Punct(q)) if *q == p)
    }

    fn expect(&mut self, p: &str) -> Result<(), EngineError> {
        if self.eat(p) {
            Ok(())
        } else {
            Err(self.error(format!("expected {p}")))
        }
    }

    fn program(&mut self) -> Result<Vec<Statement>, EngineError> {
        let mut out = Vec::new();
        loop {
            while self.peek() == Some(&Tok::Sep) {
                self.pos += 1;
            }
            let Some(tok) = self.peek().cloned() else { return Ok(out) };
            let define =
                matches!(tok, Tok::Name(_)) && matches!(self.toks.get(self.pos + 1), Some((Tok::Punct("="), _)));
            out.push(if let (true, Tok::Name(name)) = (define, tok) {
                self.pos += 2;
                Statement::Define(name, self.expr()?)
            } else {
                Statement::Expr(self.expr()?)
            });
            if !matches!(self.peek(), None | Some(Tok::Sep)) {
                return Err(self.error("expected the end of the statement"));
            }
        }
    }

    fn expr(&mut self) -> Result<Expr, EngineError> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return Err(self.error("nesting too deep for the stub"));
        }
        let e = self.comparison();
        self.depth -= 1;
        e
    }

    fn comparison(&mut self) -> Result<Expr, EngineError> {
        let a = self.additive()?;
        let ops = [("==", Op::Eq), ("!=", Op::Ne), ("<=", Op::Le), (">=", Op::Ge), ("<", Op::Lt), (">", Op::Gt)];
        for (p, op) in ops {
            if self.eat(p) {
                let b = self.additive()?;
                if ops.iter().any(|(q, _)| self.at(q)) {
                    return Err(self.error("comparisons do not chain"));
                }
                return Ok(Expr::Bin(op, Box::new(a), Box::new(b)));
            }
        }
        Ok(a)
    }

    fn additive(&mut self) -> Result<Expr, EngineError> {
        let mut a = self.multiplicative()?;
        loop {
            let op = if self.eat("+") {
                Op::Add
            } else if self.eat("-") {
                Op::Sub
            } else {
                return Ok(a);
            };
            a = Expr::Bin(op, Box::new(a), Box::new(self.multiplicative()?));
        }
    }

    fn multiplicative(&mut self) -> Result<Expr, EngineError> {
        let mut a = self.unary()?;
        loop {
            let op = if self.eat("*") {
                Op::Mul
            } else if self.eat("/") {
                Op::Div
            } else {
                return Ok(a);
            };
            a = Expr::Bin(op, Box::new(a), Box::new(self.unary()?));
        }
    }

    fn unary(&mut self) -> Result<Expr, EngineError> {
        if self.eat("-") {
            self.depth += 1;
            if self.depth > MAX_DEPTH {
                return Err(self.error("nesting too deep for the stub"));
            }
            let e = self.unary().map(|x| Expr::Neg(Box::new(x)));
            self.depth -= 1;
            return e;
        }
        let base = self.primary()?;
        if self.eat("^") {
            // Right associative, and the exponent may carry a unary minus: 2^-1, 2^3^2.
            self.depth += 1;
            if self.depth > MAX_DEPTH {
                return Err(self.error("nesting too deep for the stub"));
            }
            let exponent = self.unary();
            self.depth -= 1;
            return Ok(Expr::Bin(Op::Pow, Box::new(base), Box::new(exponent?)));
        }
        Ok(base)
    }

    fn primary(&mut self) -> Result<Expr, EngineError> {
        let Some(tok) = self.peek().cloned() else { return Err(self.error("unexpected end of input")) };
        self.pos += 1;
        match tok {
            Tok::Int(i) => Ok(Expr::Int(i)),
            Tok::Name(n) if n == "true" || n == "false" => Ok(Expr::Bool(n == "true")),
            Tok::Name(n) => {
                if self.eat("(") {
                    let args = self.list_items(")")?;
                    Ok(Expr::Call(n, args))
                } else {
                    Ok(Expr::Var(n))
                }
            }
            Tok::Punct("(") => {
                let e = self.expr()?;
                self.expect(")")?;
                Ok(e)
            }
            Tok::Punct("[") => Ok(Expr::List(self.list_items("]")?)),
            _ => {
                self.pos -= 1;
                Err(self.error("expected a value"))
            }
        }
    }

    fn list_items(&mut self, close: &str) -> Result<Vec<Expr>, EngineError> {
        let mut items = Vec::new();
        if self.eat(close) {
            return Ok(items);
        }
        loop {
            items.push(self.expr()?);
            if self.eat(close) {
                return Ok(items);
            }
            self.expect(",")?;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Result<String, EngineError> {
        StubEngine::new().eval(src, &StopFlag::new()).map(|e| e.printed)
    }

    #[test]
    fn arithmetic_and_precedence() {
        assert_eq!(run("1 + 2*3^2").unwrap(), "19");
        assert_eq!(run("-2^2").unwrap(), "-4");
        assert_eq!(run("2^3^2").unwrap(), "512");
        assert_eq!(run("(1 + 2)*3 - 10/5").unwrap(), "7");
        assert_eq!(run("1_000 * 1_000").unwrap(), "1000000");
        assert_eq!(run("[1, 2 + 3, [4]]").unwrap(), "[1, 5, [4]]");
        assert_eq!(run("3 >= 3").unwrap(), "true");
    }

    #[test]
    fn session_and_functions() {
        assert_eq!(run("n = 10\n# comment\nsum(range(n)); fact(5) + fib(n)").unwrap(), "175");
        let mut e = StubEngine::new();
        let stop = StopFlag::new();
        e.eval("length(range(3))", &stop).unwrap();
        e.bind_last("result").unwrap();
        assert_eq!(e.eval("result == 3", &stop).unwrap().printed, "true");
    }

    #[test]
    fn errors() {
        assert!(matches!(run("1 +"), Err(EngineError::Syntax { offset: 3, .. })));
        assert!(matches!(run("1 < 2 < 3"), Err(EngineError::Syntax { .. })));
        assert!(matches!(run("a b"), Err(EngineError::Syntax { offset: 2, .. })));
        assert!(matches!(run("7/2"), Err(EngineError::Other(_))));
        assert!(matches!(run("2^200"), Err(EngineError::Other(_))));
        assert!(matches!(run("x"), Err(EngineError::Other(_))));
        assert!(matches!(run(&"(".repeat(500)), Err(EngineError::Syntax { .. })));
    }

    #[test]
    fn stops_when_the_flag_is_raised() {
        let stop = StopFlag::new();
        stop.raise();
        assert_eq!(StubEngine::new().eval("busy(10^9)", &stop), Err(EngineError::Stopped));
    }
}
