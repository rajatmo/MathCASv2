//! The lexer (04-syntax.md section 2): source text to tokens with spans.
//!
//! Two pieces of layout are decided here rather than in the parser:
//! - **New lines** end statements, except inside `(...)` and `[...]`, where they are dropped. The lexer keeps a stack
//!   of open brackets for this; inside `{...}` new lines are significant again (blocks, modules, match arms).
//! - **Juxtaposition** (`2x`, `3(a+b)`): a [`Tok::Juxt`] token is inserted between a number and a name or `(` that
//!   follows it with no space in between (decision C.3). Two names never multiply.
use crate::ident::{alias, is_ident_continue, is_ident_start, is_keyword, symbol_alias};
use crate::span::{Diagnostic, Span};

/// The kind of a pattern token: `x_`, `xs__`, `xs___`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PatKind {
    /// `_`: exactly one term.
    Single,
    /// `__`: one or more arguments.
    Seq1,
    /// `___`: zero or more arguments.
    Seq0,
}

/// The body of a numeric literal, as written (underscores removed).
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum NumKind {
    /// Digits in base `radix` (10, 16 or 2), without prefix.
    Int { digits: String, radix: u32 },
    /// A machine real: decimal text accepted by `f64::from_str`.
    Real(String),
    /// A ball literal `mantissa~digits`.
    Ball { mantissa: String, digits: String },
}

/// A numeric literal; `imag` for the `im` suffix.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct NumLit {
    pub kind: NumKind,
    pub imag: bool,
}

/// Punctuation and operators.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum P {
    LParen,
    RParen,
    LBracket,
    RBracket,
    LBrace,
    RBrace,
    Comma,
    Semi,
    Colon,
    Dot,
    Ellipsis,
    Assign,
    ColonAssign,
    Pipe,
    Arrow,
    ColonArrow,
    FatArrow,
    Bar,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    Same,
    NotSame,
    Plus,
    Minus,
    Star,
    Slash,
    Caret,
    At,
    Amp,
}

impl P {
    pub fn text(self) -> &'static str {
        match self {
            P::LParen => "(",
            P::RParen => ")",
            P::LBracket => "[",
            P::RBracket => "]",
            P::LBrace => "{",
            P::RBrace => "}",
            P::Comma => ",",
            P::Semi => ";",
            P::Colon => ":",
            P::Dot => ".",
            P::Ellipsis => "...",
            P::Assign => "=",
            P::ColonAssign => ":=",
            P::Pipe => "|>",
            P::Arrow => "->",
            P::ColonArrow => ":>",
            P::FatArrow => "=>",
            P::Bar => "|",
            P::Eq => "==",
            P::Ne => "!=",
            P::Lt => "<",
            P::Le => "<=",
            P::Gt => ">",
            P::Ge => ">=",
            P::Same => "===",
            P::NotSame => "!==",
            P::Plus => "+",
            P::Minus => "-",
            P::Star => "*",
            P::Slash => "/",
            P::Caret => "^",
            P::At => "@",
            P::Amp => "&",
        }
    }
}

/// Operators, longest first so that the first match is the longest.
const PUNCT: &[(&str, P)] = &[
    ("===", P::Same),
    ("!==", P::NotSame),
    ("...", P::Ellipsis),
    ("==", P::Eq),
    ("!=", P::Ne),
    ("<=", P::Le),
    (">=", P::Ge),
    (":=", P::ColonAssign),
    (":>", P::ColonArrow),
    ("|>", P::Pipe),
    ("->", P::Arrow),
    ("=>", P::FatArrow),
    ("(", P::LParen),
    (")", P::RParen),
    ("[", P::LBracket),
    ("]", P::RBracket),
    ("{", P::LBrace),
    ("}", P::RBrace),
    (",", P::Comma),
    (";", P::Semi),
    (":", P::Colon),
    (".", P::Dot),
    ("=", P::Assign),
    ("|", P::Bar),
    ("<", P::Lt),
    (">", P::Gt),
    ("+", P::Plus),
    ("-", P::Minus),
    ("*", P::Star),
    ("/", P::Slash),
    ("^", P::Caret),
    ("@", P::At),
    ("&", P::Amp),
];

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Tok {
    /// A name: plain, qualified (`M.f`) or backquoted.
    Ident(String),
    /// A reserved word.
    Kw(&'static str),
    Pattern { name: Option<String>, kind: PatKind },
    Num(NumLit),
    Str(String),
    /// `&k`
    Slot(u32),
    /// `'` marks directly after a name; the count.
    Prime(u32),
    Punct(P),
    /// Implicit multiplication of a number and an adjacent name or parenthesis.
    Juxt,
    Newline,
    Eof,
}

impl Tok {
    /// A short description for error messages.
    pub fn describe(&self) -> String {
        match self {
            Tok::Ident(n) => format!("name `{n}`"),
            Tok::Kw(k) => format!("keyword `{k}`"),
            Tok::Pattern { .. } => "a pattern".into(),
            Tok::Num(_) => "a number".into(),
            Tok::Str(_) => "a string".into(),
            Tok::Slot(k) => format!("`&{k}`"),
            Tok::Prime(_) => "`'`".into(),
            Tok::Punct(p) => format!("`{}`", p.text()),
            Tok::Juxt => "a juxtaposed factor".into(),
            Tok::Newline => "end of line".into(),
            Tok::Eof => "end of input".into(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Token {
    pub tok: Tok,
    pub span: Span,
}

/// The result of lexing: tokens (always ending with [`Tok::Eof`]) and the errors found. Lexing never stops at an
/// error; the offending text is skipped or kept in a best-effort token so that parsing can report more.
pub struct Lexed {
    pub tokens: Vec<Token>,
    pub errors: Vec<Diagnostic>,
}

pub fn lex(src: &str) -> Lexed {
    let mut lx = Lexer { src, pos: 0, tokens: Vec::new(), errors: Vec::new(), brackets: Vec::new(), space: true };
    lx.run();
    Lexed { tokens: lx.tokens, errors: lx.errors }
}

struct Lexer<'s> {
    src: &'s str,
    pos: usize,
    tokens: Vec<Token>,
    errors: Vec<Diagnostic>,
    /// Open brackets: `(`, `[` or `{`.
    brackets: Vec<char>,
    /// Whitespace was seen since the last token.
    space: bool,
}

impl<'s> Lexer<'s> {
    fn peek(&self) -> Option<char> {
        self.src[self.pos..].chars().next()
    }
    fn peek2(&self) -> Option<char> {
        let mut it = self.src[self.pos..].chars();
        it.next();
        it.next()
    }
    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.pos += c.len_utf8();
        Some(c)
    }
    fn error(&mut self, start: usize, msg: impl Into<String>) {
        let end = self.pos.max(start);
        self.errors.push(Diagnostic::new(Span::new(start, end), msg));
    }

    fn push(&mut self, tok: Tok, start: usize) {
        // Juxtaposition: a number directly followed by a name or `(`.
        if matches!(tok, Tok::Ident(_) | Tok::Punct(P::LParen)) && !self.space {
            if let Some(Token { tok: Tok::Num(_), span, .. }) = self.tokens.last() {
                let at = span.end as usize;
                self.tokens.push(Token { tok: Tok::Juxt, span: Span::new(at, at) });
            }
        }

        self.tokens.push(Token { tok, span: Span::new(start, self.pos) });
        self.space = false;
    }

    fn newlines_significant(&self) -> bool {
        !matches!(self.brackets.last(), Some('(') | Some('['))
    }

    fn run(&mut self) {
        while let Some(c) = self.peek() {
            let start = self.pos;
            match c {
                '\n' => {
                    self.bump();
                    self.space = true;
                    if self.newlines_significant()
                        && !matches!(self.tokens.last(), None | Some(Token { tok: Tok::Newline, .. }))
                    {
                        self.tokens.push(Token { tok: Tok::Newline, span: Span::new(start, start + 1) });
                    }
                }
                c if c.is_whitespace() => {
                    self.bump();
                    self.space = true;
                }
                '#' => {
                    while let Some(c) = self.peek() {
                        if c == '\n' {
                            break;
                        }
                        self.bump();
                    }
                    self.space = true;
                }
                '"' => self.string(),
                '`' => self.backquoted(),
                '0'..='9' => self.number(),
                '&' if matches!(self.peek2(), Some('0'..='9')) => self.slot(),
                c if is_ident_start(c) => self.ident(),
                c if symbol_alias(c).is_some() => {
                    self.bump();
                    let name = symbol_alias(c).unwrap_or_default().to_string();
                    self.push(Tok::Ident(name), start);
                    self.primes();
                }
                _ => self.punct(),
            }
        }
        let end = self.src.len();
        self.tokens.push(Token { tok: Tok::Eof, span: Span::new(end, end) });
    }

    fn punct(&mut self) {
        let start = self.pos;
        let rest = &self.src[self.pos..];
        for &(text, p) in PUNCT {
            if rest.starts_with(text) {
                self.pos += text.len();
                match p {
                    P::LParen => self.brackets.push('('),
                    P::LBracket => self.brackets.push('['),
                    P::LBrace => self.brackets.push('{'),
                    P::RParen | P::RBracket | P::RBrace => {
                        let open = match p {
                            P::RParen => '(',
                            P::RBracket => '[',
                            _ => '{',
                        };
                        // A mismatched closer is reported by the parser. For layout it closes the nearest matching
                        // opener, or the innermost one if none matches, so that new lines become significant
                        // again after an error such as `(]`.
                        match self.brackets.iter().rposition(|&b| b == open) {
                            Some(i) => self.brackets.truncate(i),
                            None => {
                                self.brackets.pop();
                            }
                        }
                    }
                    _ => {}
                }
                self.push(Tok::Punct(p), start);
                return;
            }
        }
        let c = self.bump().unwrap_or(' ');
        let msg = match c {
            '\'' => "a prime mark `'` must directly follow a name".to_string(),
            '~' => "`~` must directly follow the digits of a number (a ball literal such as `1.25~40`)".to_string(),
            '!' => "unexpected `!`; inequality is `!=` and negation is `not`".to_string(),
            c => format!("unexpected character `{c}`"),
        };
        self.error(start, msg);
        self.space = true;
    }

    /// An identifier run: a name, a keyword, a pattern (`x_`, `__`), or a qualified name `M.f`.
    fn ident(&mut self) {
        let start = self.pos;
        let run = self.run_ident();
        let trailing = run.len() - run.trim_end_matches('_').len();
        if trailing > 0 {
            let name = &run[..run.len() - trailing];
            let kind = match trailing {
                1 => PatKind::Single,
                2 => PatKind::Seq1,
                3 => PatKind::Seq0,
                _ => {
                    self.error(start, "a pattern ends in at most three underscores");
                    PatKind::Seq0
                }
            };
            if !name.is_empty() && (is_keyword(name) || name.starts_with(|c: char| c.is_ascii_digit())) {
                self.error(start, format!("`{name}` cannot name a pattern"));
            }
            let name = if name.is_empty() { None } else { Some(name.to_string()) };
            self.push(Tok::Pattern { name, kind }, start);
            return;
        }
        if is_keyword(&run) {
            let kw = crate::ident::KEYWORDS.iter().copied().find(|k| *k == run).unwrap_or("null");
            self.push(Tok::Kw(kw), start);
            return;
        }
        let mut name = alias(&run).map(str::to_string).unwrap_or(run);
        // Qualified name: `.` directly followed by another identifier.
        while self.peek() == Some('.') && self.peek2().is_some_and(is_ident_start) {
            let save = self.pos;
            self.bump();
            let seg = self.run_ident();
            if seg.ends_with('_') || is_keyword(&seg) {
                self.error(save, format!("`{seg}` cannot be part of a qualified name"));
            }
            name.push('.');
            name.push_str(&seg);
        }
        self.push(Tok::Ident(name), start);
        self.primes();
    }

    fn run_ident(&mut self) -> String {
        let start = self.pos;
        while self.peek().is_some_and(is_ident_continue) {
            self.bump();
        }
        self.src[start..self.pos].to_string()
    }

    /// Prime marks directly after a name.
    fn primes(&mut self) {
        let start = self.pos;
        let mut n = 0u32;
        while self.peek() == Some('\'') {
            self.bump();
            n = n.saturating_add(1);
        }
        if n > 0 {
            self.push(Tok::Prime(n), start);
        }
    }

    /// A backquoted name: any characters, with the escapes of strings plus `` \` ``.
    fn backquoted(&mut self) {
        let start = self.pos;
        self.bump();
        let mut name = String::new();
        loop {
            match self.peek() {
                None | Some('\n') => {
                    self.error(start, "unterminated backquoted name");
                    break;
                }
                Some('`') => {
                    self.bump();
                    break;
                }
                Some('\\') => {
                    if let Some(c) = self.escape(true) {
                        name.push(c);
                    }
                }
                Some(c) => {
                    self.bump();
                    name.push(c);
                }
            }
        }
        self.push(Tok::Ident(name), start);
        self.primes();
    }

    fn string(&mut self) {
        let start = self.pos;
        self.bump();
        let mut s = String::new();
        loop {
            match self.peek() {
                None | Some('\n') => {
                    self.error(start, "unterminated string");
                    break;
                }
                Some('"') => {
                    self.bump();
                    break;
                }
                Some('\\') => {
                    if let Some(c) = self.escape(false) {
                        s.push(c);
                    }
                }
                Some(c) => {
                    self.bump();
                    s.push(c);
                }
            }
        }
        self.push(Tok::Str(s), start);
    }

    /// An escape after `\`: `\n \t \" \\ \u{...}`, and `` \` `` in backquoted names.
    fn escape(&mut self, backquoted: bool) -> Option<char> {
        let start = self.pos;
        self.bump();
        let c = self.peek();
        match c {
            Some('n') => {
                self.bump();
                Some('\n')
            }
            Some('t') => {
                self.bump();
                Some('\t')
            }
            Some('"') => {
                self.bump();
                Some('"')
            }
            Some('\\') => {
                self.bump();
                Some('\\')
            }
            Some('`') if backquoted => {
                self.bump();
                Some('`')
            }
            Some('u') => {
                self.bump();
                if self.peek() != Some('{') {
                    self.error(start, "expected `{` after `\\u`");
                    return None;
                }
                self.bump();
                let hs = self.pos;
                while self.peek().is_some_and(|c| c.is_ascii_hexdigit()) {
                    self.bump();
                }
                let hex = &self.src[hs..self.pos];
                if self.peek() != Some('}') {
                    self.error(start, "expected `}` to close `\\u{...}`");
                    return None;
                }
                self.bump();
                match u32::from_str_radix(hex, 16).ok().and_then(char::from_u32) {
                    Some(c) if hex.len() <= 6 => Some(c),
                    _ => {
                        self.error(start, format!("`\\u{{{hex}}}` is not a Unicode scalar value"));
                        None
                    }
                }
            }
            _ => {
                if c.is_some() {
                    self.bump();
                }
                self.error(start, "unknown escape; the escapes are \\n \\t \\\" \\\\ \\u{...}");
                None
            }
        }
    }

    fn slot(&mut self) {
        let start = self.pos;
        self.bump();
        let ds = self.pos;
        while self.peek().is_some_and(|c| c.is_ascii_digit()) {
            self.bump();
        }
        let k = match self.src[ds..self.pos].parse::<u32>() {
            Ok(k) if k >= 1 => k,
            _ => {
                self.error(start, "a slot is `&1`, `&2`, ...");
                1
            }
        };
        self.push(Tok::Slot(k), start);
    }

    /// Digits of `radix` with single `_` separators between digits.
    fn digits(&mut self, radix: u32, out: &mut String) {
        loop {
            match self.peek() {
                Some(c) if c.is_digit(radix) => {
                    self.bump();
                    out.push(c);
                }
                Some('_') if self.peek2().is_some_and(|c| c.is_digit(radix)) && !out.is_empty() => {
                    self.bump();
                }
                _ => return,
            }
        }
    }

    fn number(&mut self) {
        let start = self.pos;
        let rest = &self.src[self.pos..];
        let radix = if rest.starts_with("0x") || rest.starts_with("0X") {
            16
        } else if rest.starts_with("0b") || rest.starts_with("0B") {
            2
        } else {
            10
        };
        let kind = if radix != 10 {
            self.pos += 2;
            let mut d = String::new();
            self.digits(radix, &mut d);
            if d.is_empty() {
                self.error(start, if radix == 16 { "expected hexadecimal digits after `0x`" } else { "expected binary digits after `0b`" });
                d.push('0');
            }
            NumKind::Int { digits: d, radix }
        } else {
            let mut text = String::new();
            self.digits(10, &mut text);
            let mut real = false;
            // `1.` and `1.5` are reals; `.` followed by a letter or `.` is not part of the number.
            if self.peek() == Some('.') && !self.peek2().is_some_and(|c| c == '.' || is_ident_continue(c) && !c.is_ascii_digit()) {
                self.bump();
                text.push('.');
                self.digits(10, &mut text);
                real = true;
            }
            if matches!(self.peek(), Some('e') | Some('E')) {
                let save = self.pos;
                self.bump();
                let mut exp = String::from("e");
                if let Some(s @ ('+' | '-')) = self.peek() {
                    self.bump();
                    exp.push(s);
                }
                if self.peek().is_some_and(|c| c.is_ascii_digit()) {
                    self.digits(10, &mut exp);
                    text.push_str(&exp);
                    real = true;
                } else {
                    // Not an exponent: `2e` is `2` times `e`.
                    self.pos = save;
                }
            }
            if self.peek() == Some('~') {
                let ts = self.pos;
                self.bump();
                let mut digits = String::new();
                self.digits(10, &mut digits);
                if digits.is_empty() {
                    self.error(ts, "expected the precision in decimal digits after `~`");
                    digits.push('1');
                }
                NumKind::Ball { mantissa: text, digits }
            } else if real {
                NumKind::Real(text)
            } else {
                NumKind::Int { digits: text, radix: 10 }
            }
        };
        let imag = self.src[self.pos..].starts_with("im")
            && !self.src[self.pos + 2..].chars().next().is_some_and(is_ident_continue);
        if imag {
            self.pos += 2;
        }
        self.push(Tok::Num(NumLit { kind, imag }), start);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toks(src: &str) -> Vec<Tok> {
        let l = lex(src);
        assert!(l.errors.is_empty(), "{:?}", l.errors);
        l.tokens.into_iter().map(|t| t.tok).collect()
    }

    #[test]
    fn numbers() {
        let t = toks("1_000 0x1F 0b101 1.5 1. 2e-10 1.25~40 3im 2~100");
        let kinds: Vec<_> = t.iter().filter_map(|t| if let Tok::Num(n) = t { Some(n.clone()) } else { None }).collect();
        assert_eq!(kinds[0].kind, NumKind::Int { digits: "1000".into(), radix: 10 });
        assert_eq!(kinds[1].kind, NumKind::Int { digits: "1F".into(), radix: 16 });
        assert_eq!(kinds[2].kind, NumKind::Int { digits: "101".into(), radix: 2 });
        assert_eq!(kinds[3].kind, NumKind::Real("1.5".into()));
        assert_eq!(kinds[4].kind, NumKind::Real("1.".into()));
        assert_eq!(kinds[5].kind, NumKind::Real("2e-10".into()));
        assert_eq!(kinds[6].kind, NumKind::Ball { mantissa: "1.25".into(), digits: "40".into() });
        assert!(kinds[7].imag);
        assert_eq!(kinds[8].kind, NumKind::Ball { mantissa: "2".into(), digits: "100".into() });
    }

    #[test]
    fn juxtaposition_and_names() {
        assert_eq!(
            toks("2x 2 x 3(a) π θ1 M.f x_ xs__ ___ f''"),
            vec![
                Tok::Num(NumLit { kind: NumKind::Int { digits: "2".into(), radix: 10 }, imag: false }),
                Tok::Juxt,
                Tok::Ident("x".into()),
                Tok::Num(NumLit { kind: NumKind::Int { digits: "2".into(), radix: 10 }, imag: false }),
                Tok::Ident("x".into()),
                Tok::Num(NumLit { kind: NumKind::Int { digits: "3".into(), radix: 10 }, imag: false }),
                Tok::Juxt,
                Tok::Punct(P::LParen),
                Tok::Ident("a".into()),
                Tok::Punct(P::RParen),
                Tok::Ident("pi".into()),
                Tok::Ident("θ1".into()),
                Tok::Ident("M.f".into()),
                Tok::Pattern { name: Some("x".into()), kind: PatKind::Single },
                Tok::Pattern { name: Some("xs".into()), kind: PatKind::Seq1 },
                Tok::Pattern { name: None, kind: PatKind::Seq0 },
                Tok::Ident("f".into()),
                Tok::Prime(2),
                Tok::Eof,
            ]
        );
    }

    #[test]
    fn newlines_inside_brackets_are_dropped() {
        let t = toks("f(a,\n b)\n{ x\n y }\n");
        let n = t.iter().filter(|t| **t == Tok::Newline).count();
        assert_eq!(n, 3);
    }

    #[test]
    fn strings_and_errors() {
        assert_eq!(toks(r#""a\n\u{3b8}\"""#)[0], Tok::Str("a\nθ\"".into()));
        let l = lex("\"abc\n x ! y");
        assert_eq!(l.errors.len(), 2);
    }
}
