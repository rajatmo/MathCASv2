//! Editor support (08-notebook.md sections 3.10 to 3.12): colouring classes for every token of a cell, bracket
//! pairs, and the local names in scope at a position.
//!
//! The analysis works on the token stream, not on the syntax tree, so that it stays stable while a statement is
//! being typed: the parser drops a statement with an error, but the tokens before the caret keep their classes. It
//! has three passes, each linear in the length of the text:
//!
//! 1. **Lexical classes** from the lexer (keywords, constants, numbers, strings, comments, operators, names).
//! 2. **Brackets**: a stack pass pairs `()`, `[]`, `{}`; a closer pairs with the nearest open bracket of its kind
//!    (open brackets left inside are unmatched), a closer with no open bracket of its kind is unmatched.
//! 3. **Scopes**: a walk over the bracket tree that mirrors the grammar of 04-syntax.md where names are bound: `let`
//!    and `var` (to the end of the block), `fn` parameters (the definition), lambdas `x => e` and `(x, y) => e` (the
//!    body), `for x in xs { ... }` (the body), pattern variables `x_` and `x @ p` on the left of `->`, `:>`, `=`,
//!    `:=` or of a `match` arm's `=>` (the whole rule), and type positions and option names (never variables).
//!
//! Syntax errors come from the parser; an error at the end of the input (the cell is unfinished, as while typing)
//! does not colour a token ([`SHOW_ERRORS_AT_END`]).
use crate::lexer::{lex, Tok, Token, P};
use crate::parser::parse_tokens;
use crate::span::{Diagnostic, Span};
use std::collections::{HashMap, HashSet};
use std::ops::Range;

/// Texts longer than this (in bytes) are coloured lexically only: no syntax errors and no scopes (section 3.11).
pub const LEXICAL_ONLY_ABOVE: usize = 100_000;

/// Whether an error at the end of the input (an unfinished statement, an unterminated string running to the end)
/// colours its token. Off: an input being typed is not shown as an error until it is wrong before its end.
pub const SHOW_ERRORS_AT_END: bool = false;

/// Bracket groups nested deeper than this are coloured without scopes (their names are plain globals).
const MAX_NESTING: u32 = crate::MAX_DEPTH;

/// The colouring class of a token (08-notebook.md section 3.11).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum TokenClass {
    /// Operators, punctuation and matched brackets (default text colour).
    Operator,
    /// A global name with a definition, or any global name when no name set is given (default text colour).
    Global,
    /// A global name with no definition (only with a name set, see [`colour`]).
    Undefined,
    /// A name bound by `let`, `var`, a lambda or `fn` parameter or a `for` loop, where it is bound and used; also
    /// the slots `&1`, `&2` of a slot function.
    Local,
    /// A pattern (`x_`, `xs__`, `_`, the `x` of `x @ p`) and the uses of its name in the rule it belongs to.
    PatternVar,
    /// The binding occurrence of a local or pattern name that hides a global definition.
    Shadowing,
    /// A reserved word other than the constants.
    Keyword,
    /// `true`, `false`, `null`.
    Constant,
    /// A numeric literal, with its `im` suffix.
    Number,
    /// A string literal with its quotes.
    String,
    /// `#` to the end of the line.
    Comment,
    /// An unfilled template slot `□` (also reported as an error).
    Placeholder,
    /// The token at a syntax error, or text the lexer could not read.
    SyntaxError,
    /// A bracket with no partner (drawn bold).
    UnmatchedBracket,
}

impl TokenClass {
    /// A short stable name, for snapshot tests and debugging.
    pub fn name(self) -> &'static str {
        match self {
            TokenClass::Operator => "op",
            TokenClass::Global => "global",
            TokenClass::Undefined => "undefined",
            TokenClass::Local => "local",
            TokenClass::PatternVar => "pattern",
            TokenClass::Shadowing => "shadowing",
            TokenClass::Keyword => "keyword",
            TokenClass::Constant => "constant",
            TokenClass::Number => "number",
            TokenClass::String => "string",
            TokenClass::Comment => "comment",
            TokenClass::Placeholder => "placeholder",
            TokenClass::SyntaxError => "error",
            TokenClass::UnmatchedBracket => "unmatched",
        }
    }
}

/// The colouring of one text: see [`colour`].
#[derive(Clone, Debug, Default)]
pub struct Colouring {
    /// Byte ranges and their classes, sorted, not overlapping, never empty. Whitespace and line ends between
    /// tokens are not listed (default text colour).
    pub tokens: Vec<(Range<usize>, TokenClass)>,
    /// Every lexical and syntax error (also those at the end of the input, which colour nothing), in text order.
    pub errors: Vec<Diagnostic>,
    /// Some error lies at the end of the input: the text looks unfinished.
    pub incomplete: bool,
    /// The text was longer than [`LEXICAL_ONLY_ABOVE`]: no scopes and no syntax errors.
    pub lexical_only: bool,
    /// Matched bracket pairs `(open, close)` by byte offset of each bracket, sorted by the open bracket.
    pub pairs: Vec<(usize, usize)>,
    /// The same pairs keyed both ways (`(a, b)` and `(b, a)`), sorted by the first offset.
    partners: Vec<(usize, usize)>,
}

impl Colouring {
    /// The class of the token containing byte `offset`, if any.
    pub fn class_at(&self, offset: usize) -> Option<TokenClass> {
        let i = self.tokens.partition_point(|t| t.0.end <= offset);
        self.tokens.get(i).filter(|t| t.0.start <= offset).map(|t| t.1)
    }

    /// The bracket next to a caret at byte `caret` and its partner, as byte offsets `(bracket, partner)`
    /// (section 3.12). The bracket just before the caret is preferred to the one just after it; a bracket with no
    /// partner gives `None`.
    pub fn matching_bracket(&self, caret: usize) -> Option<(usize, usize)> {
        let find = |at: usize| {
            self.partners.binary_search_by_key(&at, |p| p.0).ok().map(|i| self.partners[i])
        };
        caret.checked_sub(1).and_then(find).or_else(|| find(caret))
    }

    /// The smallest bracketed expression, brackets included, that strictly contains the byte range `start..end`
    /// ("check balance"; double-click on a bracket selects `enclosing_brackets(b, b + 1)` or the pair itself).
    pub fn enclosing_brackets(&self, start: usize, end: usize) -> Option<Range<usize>> {
        let mut best: Option<Range<usize>> = None;
        for &(o, c) in &self.pairs {
            if o > start {
                break;
            }
            let r = o..c + 1;
            if r.end >= end && (r.start < start || r.end > end) && best.as_ref().is_none_or(|b| r.len() < b.len()) {
                best = Some(r);
            }
        }
        best
    }

    /// The bracket pair, brackets included, whose open or close bracket is at byte `offset`.
    pub fn pair_at(&self, offset: usize) -> Option<Range<usize>> {
        let i = self.partners.binary_search_by_key(&offset, |p| p.0).ok()?;
        let (a, b) = self.partners[i];
        Some(a.min(b)..a.max(b) + 1)
    }
}

/// How a local name is bound.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LocalKind {
    /// `let`, `var`, a parameter, a lambda parameter or a loop variable.
    Local,
    /// A pattern variable.
    Pattern,
}

/// A local name visible at a position ([`names_in_scope`]).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct LocalName {
    pub name: String,
    pub kind: LocalKind,
    /// The byte range of the binding occurrence.
    pub binder: Range<usize>,
}

/// The classes of the tokens of `src`, with no kernel: every global name is [`TokenClass::Global`]. The same as
/// `colour(src, None).tokens`.
pub fn classify(src: &str) -> Vec<(Range<usize>, TokenClass)> {
    colour(src, None).tokens
}

/// Colours a text (a cell). `defined` tells whether a global name has a definition (the kernel's name set,
/// 08-notebook.md section 3.10); with it, global names without one are [`TokenClass::Undefined`] and local names
/// that hide a defined one are [`TokenClass::Shadowing`] where they are bound. Names defined in the text itself (by
/// `fn`, `rule`, `sym`, `module` or a top-level `x = ...`, `f(x_) := ...`) count as defined.
///
/// Linear in the length of `src`; never fails, whatever the input.
pub fn colour(src: &str, defined: Option<&(dyn Fn(&str) -> bool + Sync)>) -> Colouring {
    // The parser and the scope walk recurse per nesting level: run them on the reserved stack (`stack.rs`).
    crate::stack::on_big_stack(|| colour_here(src, defined))
}

fn colour_here(src: &str, defined: Option<&(dyn Fn(&str) -> bool + Sync)>) -> Colouring {
    let lexed = lex(src);
    let toks = lexed.tokens;
    let lexical_only = src.len() > LEXICAL_ONLY_ABOVE;
    let errors = if lexical_only { lexed.errors } else { parse_tokens(&toks, lexed.errors).errors };
    let mut w = Walker::new(&toks, defined, None);
    if !lexical_only {
        w.run();
    }
    let Walker { mut class, partner, .. } = w;

    // Errors: the token at the error, or the unread text, unless the error is at the end of the input.
    let content_end = toks.iter().rev().find(|t| !matches!(t.tok, Tok::Newline | Tok::Eof)).map_or(0, |t| t.span.end);
    let unmatched = class.contains(&Some(TokenClass::UnmatchedBracket));
    let mut extra: Vec<(Range<usize>, TokenClass)> = Vec::new();
    let mut incomplete = false;
    let text_end = src.trim_end().len();
    for e in &errors {
        let at = e.span.start as usize;
        let at_end = e.span.start >= content_end
            || e.message.starts_with("unterminated") && e.span.end as usize >= text_end;
        if at_end {
            incomplete = true;
            if !SHOW_ERRORS_AT_END {
                continue;
            }
        }
        // the token containing the error position; at a line end, the token before it
        let mut i = toks.partition_point(|t| (t.span.end as usize) <= at);
        if toks.get(i).is_some_and(|t| t.tok == Tok::Newline) {
            if let Some(j) = class[..i].iter().rposition(Option::is_some) {
                i = j;
            }
        }
        match toks.get(i) {
            Some(t) if (t.span.start as usize) <= at && t.span.end > t.span.start && !matches!(t.tok, Tok::Eof) => {
                // an error at a closing bracket follows from an unmatched bracket, which is already shown
                let consequence = unmatched && matches!(t.tok, Tok::Punct(P::RParen | P::RBracket | P::RBrace));
                if !consequence && !matches!(class[i], Some(TokenClass::UnmatchedBracket | TokenClass::Placeholder)) {
                    class[i] = Some(TokenClass::SyntaxError);
                }
            }
            next => {
                // unread text (skipped by the lexer): up to the next token
                let limit = next.map_or(src.len(), |t| t.span.start as usize).max(at);
                let mut end = (e.span.end as usize).min(limit).max(at);
                if end == at {
                    end = src[at..].chars().next().map_or(at, |c| at + c.len_utf8()).min(limit);
                }
                if end > at {
                    extra.push((at..end, TokenClass::SyntaxError));
                }
            }
        }
    }

    let mut tokens: Vec<(Range<usize>, TokenClass)> = toks
        .iter()
        .zip(&class)
        .filter_map(|(t, c)| c.map(|c| (t.span.start as usize..t.span.end as usize, c)))
        .filter(|(r, _)| !r.is_empty())
        .collect();
    tokens.extend(lexed.comments.iter().map(|s| (s.start as usize..s.end as usize, TokenClass::Comment)));
    tokens.extend(extra);
    tokens.sort_by_key(|t| t.0.start);
    // Lexer errors inside a token were recorded on the token; drop any overlap defensively.
    tokens.dedup_by(|b, a| b.0.start < a.0.end);

    let mut pairs = Vec::new();
    for (i, p) in partner.iter().enumerate() {
        if let Some(j) = *p {
            if i < j {
                pairs.push((toks[i].span.start as usize, toks[j].span.start as usize));
            }
        }
    }
    let mut partners: Vec<(usize, usize)> = pairs.iter().flat_map(|&(a, b)| [(a, b), (b, a)]).collect();
    partners.sort_unstable();
    Colouring { tokens, errors, incomplete, lexical_only, pairs, partners }
}

/// The local names visible at byte `offset` of `src`, innermost first, each name once (for completion, section
/// 3.10). An identifier touching `offset` (as while typing it) is not itself counted unless it is a use.
pub fn names_in_scope(src: &str, offset: usize) -> Vec<LocalName> {
    let mut toks = lex(src).tokens;
    // The probe: the identifier touching the offset, or a synthetic one inserted there.
    let touching = toks.iter().position(|t| {
        matches!(t.tok, Tok::Ident(_)) && t.span.start as usize <= offset && offset <= t.span.end as usize
    });
    let probe = match touching {
        Some(i) => i,
        None => {
            let i = toks.partition_point(|t| (t.span.start as usize) < offset && !matches!(t.tok, Tok::Eof));
            toks.insert(i, Token { tok: Tok::Ident("\u{0}".into()), span: Span::new(offset, offset) });
            i
        }
    };
    let snapshot = crate::stack::on_big_stack(|| {
        let mut w = Walker::new(&toks, None, Some(probe));
        w.run();
        w.snapshot.take()
    });
    snapshot.unwrap_or_default()
}

/// Where a group of tokens is.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Group {
    /// The whole text: statements.
    Root,
    /// A module body: statements.
    Module,
    /// A block `{...}`: statements.
    Brace,
    /// The arms of a `match`.
    Arms,
    /// `(...)`: call arguments or a parenthesised expression.
    Paren,
    /// `[...]`: a list or an index.
    Bracket,
    /// The parameters of a `fn` definition or a lambda: no frame of its own.
    Params,
}

#[derive(Clone, Copy)]
struct Binding {
    kind: LocalKind,
    tok: usize,
}

struct Walker<'a> {
    toks: &'a [Token],
    /// The number of tokens before `Eof`.
    n: usize,
    class: Vec<Option<TokenClass>>,
    partner: Vec<Option<usize>>,
    /// Names that are never variables: option names, type names, decorators, module paths, defined names.
    exempt: Vec<bool>,
    env: HashMap<&'a str, Vec<Binding>>,
    /// Bound names in binding order; frames are lengths of this stack.
    bound: Vec<&'a str>,
    frames: Vec<usize>,
    in_text: HashSet<&'a str>,
    defined: Option<&'a (dyn Fn(&str) -> bool + Sync)>,
    depth: u32,
    probe: Option<usize>,
    snapshot: Option<Vec<LocalName>>,
}

fn is_open(t: &Tok) -> bool {
    matches!(t, Tok::Punct(P::LParen | P::LBracket | P::LBrace))
}

fn closer_of(t: &Tok) -> Option<P> {
    match t {
        Tok::Punct(P::LParen) => Some(P::RParen),
        Tok::Punct(P::LBracket) => Some(P::RBracket),
        Tok::Punct(P::LBrace) => Some(P::RBrace),
        _ => None,
    }
}

fn lexical_class(t: &Tok) -> Option<TokenClass> {
    Some(match t {
        Tok::Ident(_) => TokenClass::Global,
        Tok::Kw("true" | "false" | "null") => TokenClass::Constant,
        Tok::Kw(_) => TokenClass::Keyword,
        Tok::Pattern { .. } => TokenClass::PatternVar,
        Tok::Num(_) => TokenClass::Number,
        Tok::Str(_) => TokenClass::String,
        Tok::Slot(_) => TokenClass::Local,
        Tok::Prime(_) | Tok::Punct(_) | Tok::Sqrt => TokenClass::Operator,
        Tok::Placeholder => TokenClass::Placeholder,
        Tok::Juxt | Tok::Newline | Tok::Eof => return None,
    })
}

impl<'a> Walker<'a> {
    fn new(toks: &'a [Token], defined: Option<&'a (dyn Fn(&str) -> bool + Sync)>, probe: Option<usize>) -> Self {
        let n = toks.iter().position(|t| t.tok == Tok::Eof).unwrap_or(toks.len());
        let class = toks.iter().map(|t| lexical_class(&t.tok)).collect();
        let mut w = Walker {
            toks,
            n,
            class,
            partner: vec![None; toks.len()],
            exempt: vec![false; toks.len()],
            env: HashMap::new(),
            bound: Vec::new(),
            frames: Vec::new(),
            in_text: HashSet::new(),
            defined,
            depth: 0,
            probe,
            snapshot: None,
        };
        w.brackets();
        w
    }

    fn tok(&self, i: usize) -> &'a Tok {
        let toks: &'a [Token] = self.toks;
        &toks[i.min(self.n)].tok
    }

    fn name(&self, i: usize) -> &'a str {
        match self.tok(i) {
            Tok::Ident(n) => n,
            Tok::Pattern { name: Some(n), .. } => n,
            _ => "",
        }
    }

    fn is_ident(&self, i: usize) -> bool {
        matches!(self.tok(i), Tok::Ident(_))
    }

    fn at(&self, i: usize, p: P) -> bool {
        *self.tok(i) == Tok::Punct(p)
    }

    /// Pass 2: bracket pairs; unmatched brackets get their class.
    fn brackets(&mut self) {
        let mut stack: Vec<usize> = Vec::new();
        // open brackets of each kind on the stack, so that a closer with no opener of its kind costs nothing
        let mut open = [0usize; 3];
        let kind = |p: P| match p {
            P::LParen | P::RParen => 0,
            P::LBracket | P::RBracket => 1,
            _ => 2,
        };
        for i in 0..self.n {
            match self.toks[i].tok {
                Tok::Punct(p @ (P::LParen | P::LBracket | P::LBrace)) => {
                    stack.push(i);
                    open[kind(p)] += 1;
                }
                Tok::Punct(p @ (P::RParen | P::RBracket | P::RBrace)) => {
                    if open[kind(p)] == 0 {
                        self.class[i] = Some(TokenClass::UnmatchedBracket);
                        continue;
                    }
                    while let Some(j) = stack.pop() {
                        let Tok::Punct(q) = self.toks[j].tok else { continue };
                        open[kind(q)] -= 1;
                        if kind(q) == kind(p) {
                            self.partner[i] = Some(j);
                            self.partner[j] = Some(i);
                            break;
                        }
                        self.class[j] = Some(TokenClass::UnmatchedBracket);
                    }
                }
                _ => {}
            }
        }
        for j in stack {
            self.class[j] = Some(TokenClass::UnmatchedBracket);
        }
    }

    /// The end (exclusive) of the group opened at `i`: its closer, or `hi` for an unmatched opener.
    fn group_end(&self, i: usize, hi: usize) -> usize {
        self.partner[i].filter(|&c| c < hi).unwrap_or(hi)
    }

    /// The token after the item starting at `i`: a whole bracket group is one item.
    fn next_item(&self, i: usize, hi: usize) -> usize {
        if is_open(self.tok(i)) {
            match self.partner[i] {
                Some(c) if c < hi => c + 1,
                _ => hi,
            }
        } else {
            i + 1
        }
    }

    fn run(&mut self) {
        self.definitions();
        self.walk_group(0, self.n, Group::Root);
    }

    /// Names defined at the top level or in a module body: `fn f`, `rule f(...)`, `sym s`, `module M`, `x = ...`,
    /// `f(...) := ...`.
    fn definitions(&mut self) {
        // for each open bracket: whether it is a module body
        let mut ctx: Vec<bool> = Vec::new();
        let mut start = true;
        let mut module_next = false;
        // top-level `let` and `var` names: assigning one is not a definition
        let mut locals: HashSet<&'a str> = HashSet::new();
        for i in 0..self.n {
            let top = ctx.last().is_none_or(|&m| m);
            if top && matches!(self.tok(i), Tok::Kw("let" | "var")) && self.is_ident(i + 1) {
                locals.insert(self.name(i + 1));
            }
            match self.tok(i) {
                Tok::Newline | Tok::Punct(P::Semi) => {
                    start = true;
                    continue;
                }
                Tok::Punct(P::LBrace) => {
                    ctx.push(module_next);
                    module_next = false;
                    start = true;
                    continue;
                }
                Tok::Punct(P::LParen | P::LBracket) => ctx.push(false),
                Tok::Punct(P::RParen | P::RBracket | P::RBrace) => {
                    ctx.pop();
                }
                Tok::Kw("fn" | "rule" | "sym" | "module") if self.is_ident(i + 1) => {
                    module_next = *self.tok(i) == Tok::Kw("module");
                    self.in_text.insert(self.name(i + 1));
                }
                Tok::Kw("pub") => continue,
                Tok::Ident(n) if start && top => {
                    let mut j = i + 1;
                    if self.at(j, P::LParen) {
                        j = self.next_item(j, self.n);
                    }
                    if self.at(j, P::Assign) || self.at(j, P::ColonAssign) {
                        self.in_text.insert(n);
                    }
                }
                _ => {}
            }
            start = false;
        }
        self.in_text.retain(|n| !locals.contains(n));
    }

    // ---- frames and bindings ----

    fn push_frame(&mut self) {
        self.frames.push(self.bound.len());
    }

    fn pop_frame(&mut self) {
        let keep = self.frames.pop().unwrap_or(0);
        while self.bound.len() > keep {
            if let Some(name) = self.bound.pop() {
                if let Some(v) = self.env.get_mut(name) {
                    v.pop();
                }
            }
        }
    }

    fn is_defined_global(&self, name: &str) -> bool {
        self.in_text.contains(name) || self.defined.is_some_and(|f| f(name))
    }

    /// Binds the name at token `i` in the innermost frame and colours its binding occurrence.
    fn bind(&mut self, i: usize, kind: LocalKind) {
        self.probe_here(i);
        let name = self.name(i);
        if name.is_empty() {
            return;
        }
        self.class[i] = Some(if self.is_defined_global(name) {
            TokenClass::Shadowing
        } else if kind == LocalKind::Local {
            TokenClass::Local
        } else {
            TokenClass::PatternVar
        });
        self.env.entry(name).or_default().push(Binding { kind, tok: i });
        self.bound.push(name);
    }

    /// Colours a use of the name at token `i`.
    fn resolve(&mut self, i: usize) {
        self.probe_here(i);
        if self.exempt[i] {
            return;
        }
        let name = self.name(i);
        let local = self.env.get(name).and_then(|v| v.last()).map(|b| b.kind);
        self.class[i] = Some(match local {
            Some(LocalKind::Local) => TokenClass::Local,
            Some(LocalKind::Pattern) => TokenClass::PatternVar,
            None if self.in_text.contains(name) => TokenClass::Global,
            None => match self.defined {
                Some(f) if !f(name) => TokenClass::Undefined,
                _ => TokenClass::Global,
            },
        });
    }

    fn probe_here(&mut self, i: usize) {
        if self.probe != Some(i) || self.snapshot.is_some() {
            return;
        }
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        for &name in self.bound.iter().rev() {
            if !seen.insert(name) {
                continue;
            }
            if let Some(b) = self.env.get(name).and_then(|v| v.last()) {
                let s = self.toks[b.tok].span;
                out.push(LocalName { name: name.to_string(), kind: b.kind, binder: s.start as usize..s.end as usize });
            }
        }
        self.snapshot = Some(out);
    }

    /// Marks the name at `i` as never a variable.
    fn exempt(&mut self, i: usize) {
        if self.is_ident(i) {
            self.probe_here(i);
            self.exempt[i] = true;
        }
    }

    // ---- groups and statements ----

    /// The tokens `lo..hi` inside a bracket group (or the whole text), split into statements, arguments, elements
    /// or parameters.
    fn walk_group(&mut self, lo: usize, hi: usize, kind: Group) {
        if self.depth >= MAX_NESTING {
            return;
        }
        self.depth += 1;
        if kind != Group::Params {
            self.push_frame();
        }
        let lines = matches!(kind, Group::Root | Group::Module | Group::Brace | Group::Arms);
        let mut s = lo;
        let mut i = lo;
        let mut block_kw = false;
        while i < hi {
            let sep = match self.tok(i) {
                Tok::Punct(P::Comma | P::Semi) => true,
                Tok::Newline => lines && !self.continues(s, i, hi, block_kw),
                _ => false,
            };
            if sep {
                self.segment(s, i, kind);
                s = i + 1;
                i += 1;
                block_kw = false;
                continue;
            }
            if matches!(self.tok(i), Tok::Kw("while" | "for" | "match" | "module")) {
                block_kw = true;
            }
            i = self.next_item(i, hi);
        }
        self.segment(s, hi, kind);
        if kind != Group::Params {
            self.pop_frame();
        }
        self.depth -= 1;
    }

    /// Whether the statement `s..` goes on past the line end at `nl` (the parser's continuation rules: after an
    /// operator or keyword, before `|>`, `=>`, `then`, `else`, a block after `while`/`for`/`match`/`module`, and
    /// decorators before their definition).
    fn continues(&self, s: usize, nl: usize, hi: usize, block_kw: bool) -> bool {
        let prev = (s..nl).rev().find(|&j| *self.tok(j) != Tok::Newline);
        let next = (nl + 1..hi).find(|&j| *self.tok(j) != Tok::Newline);
        let (Some(prev), Some(next)) = (prev, next) else { return false };
        let after_op = match self.tok(prev) {
            Tok::Punct(P::RParen | P::RBracket | P::RBrace) => false,
            Tok::Punct(_) => true,
            Tok::Kw(k) => !matches!(*k, "true" | "false" | "null"),
            _ => false,
        };
        let before = match self.tok(next) {
            Tok::Punct(P::Pipe | P::FatArrow) | Tok::Kw("then" | "else") => true,
            Tok::Punct(P::LBrace) => block_kw,
            Tok::Punct(P::At) | Tok::Kw("fn" | "rule" | "sym") => self.at(s, P::At),
            _ => false,
        };
        after_op || before
    }

    fn segment(&mut self, mut s: usize, e: usize, kind: Group) {
        while s < e && *self.tok(s) == Tok::Newline {
            s += 1;
        }
        if s >= e {
            return;
        }
        match kind {
            Group::Params => self.param(s, e),
            Group::Paren if self.is_ident(s) && s + 1 < e && self.at(s + 1, P::Colon) => {
                // a named argument `name: value`
                self.exempt(s);
                self.expr(s + 2, e, false);
            }
            Group::Paren | Group::Bracket => self.expr(s, e, false),
            Group::Arms => self.expr(s, e, true),
            Group::Root | Group::Module | Group::Brace => self.statement(s, e),
        }
    }

    fn skip_newlines(&self, mut i: usize, e: usize) -> usize {
        while i < e && *self.tok(i) == Tok::Newline {
            i += 1;
        }
        i
    }

    fn statement(&mut self, s: usize, e: usize) {
        let mut i = s;
        // decorators
        while i + 1 < e && self.at(i, P::At) && self.is_ident(i + 1) {
            self.exempt(i + 1);
            i += 2;
            if i < e && self.at(i, P::LParen) {
                let end = self.group_end(i, e);
                self.walk_group(i + 1, end, Group::Paren);
                i = self.next_item(i, e);
            }
            i = self.skip_newlines(i, e);
        }
        if *self.tok(i) == Tok::Kw("pub") {
            i += 1;
        }
        if i >= e {
            return;
        }
        match self.tok(i) {
            Tok::Kw("let" | "var") if i + 1 < e && self.is_ident(i + 1) => {
                let mut j = i + 2;
                if j < e && self.at(j, P::Colon) {
                    j = self.type_run(j + 1, e);
                }
                self.expr(j, e, false);
                // bound for the rest of the enclosing block
                self.bind(i + 1, LocalKind::Local);
            }
            Tok::Kw("fn") if i + 1 < e && self.is_ident(i + 1) => {
                self.exempt(i + 1);
                let mut j = i + 2;
                self.push_frame();
                if j < e && self.at(j, P::LParen) {
                    let end = self.group_end(j, e);
                    self.walk_group(j + 1, end, Group::Params);
                    j = self.next_item(j, e);
                }
                if j < e && self.at(j, P::Colon) {
                    j = self.type_run(j + 1, e);
                }
                self.expr(j, e, false);
                self.pop_frame();
            }
            Tok::Kw("sym" | "module") if i + 1 < e && self.is_ident(i + 1) => {
                self.exempt(i + 1);
                let mut j = i + 2;
                while j < e {
                    if self.at(j, P::LBrace) {
                        let end = self.group_end(j, e);
                        self.walk_group(j + 1, end, Group::Module);
                    }
                    j = self.next_item(j, e);
                }
            }
            Tok::Kw("import") => {
                for j in i + 1..e {
                    self.exempt(j);
                }
            }
            Tok::Kw("rule") => self.expr(i + 1, e, false),
            _ => self.expr(i, e, false),
        }
    }

    /// One parameter of a `fn` definition or a lambda, in the frame of the definition.
    fn param(&mut self, s: usize, e: usize) {
        let mut i = s;
        if *self.tok(i) == Tok::Kw("held") {
            i += 1;
        }
        if i >= e {
            return;
        }
        let simple = self.is_ident(i)
            && (i + 1 >= e || self.at(i + 1, P::Colon) || self.at(i + 1, P::Assign) || self.at(i + 1, P::Ellipsis));
        if simple {
            let mut j = i + 1;
            if j < e && self.at(j, P::Ellipsis) {
                j += 1;
            }
            if j < e && self.at(j, P::Colon) {
                j = self.type_run(j + 1, e);
            }
            // the default (`= d`), seen before the parameter is bound
            self.expr(j, e, false);
            self.bind(i, LocalKind::Local);
        } else {
            for p in self.patterns(i, e) {
                self.bind(p, LocalKind::Pattern);
            }
            self.expr(i, e, false);
        }
    }

    /// The binding tokens of the patterns in `s..e`: `x_`, `xs__`, and the `x` of `x @ p`.
    fn patterns(&self, s: usize, e: usize) -> Vec<usize> {
        (s..e)
            .filter(|&j| match self.tok(j) {
                Tok::Pattern { name: Some(_), .. } => true,
                Tok::Ident(_) => j + 1 < e && self.at(j + 1, P::At),
                _ => false,
            })
            .collect()
    }

    /// An expression `s..e` at statement, argument or element level: its rule splits (`->`, `:>`, `=`, `:=`, and
    /// in a `match` arm the first `=>`) bind the pattern variables of their left side for the rest of it; a lambda
    /// `x => body` binds its parameters for the rest of it.
    fn expr(&mut self, s: usize, e: usize, arm: bool) {
        let mut frames = 0;
        let mut cur = s;
        let mut arm_split = arm;
        let mut prev = s;
        let mut i = s;
        while i < e {
            match self.tok(i) {
                Tok::Punct(P::FatArrow) if !arm_split => {
                    // the parameters: the name or `(...)` group just before `=>`
                    let p = prev;
                    let paren = self.at(p, P::LParen) && self.partner[p] == Some(i - 1);
                    if p >= cur && p < i && (self.is_ident(p) || paren) {
                        self.linear(cur, p);
                        self.push_frame();
                        frames += 1;
                        if paren {
                            self.walk_group(p + 1, i - 1, Group::Params);
                        } else {
                            self.bind(p, LocalKind::Local);
                        }
                    } else {
                        self.rule_split(cur, i);
                        frames += 1;
                    }
                    cur = i + 1;
                }
                Tok::Punct(P::Arrow | P::ColonArrow | P::Assign | P::ColonAssign | P::FatArrow) => {
                    arm_split = false;
                    self.rule_split(cur, i);
                    frames += 1;
                    cur = i + 1;
                }
                _ => {}
            }
            prev = i;
            i = self.next_item(i, e);
        }
        // the last part: its own pattern names at this level are visible in it (`x_ when x > 0`,
        // `x_: Int where x > 0`); patterns inside its brackets belong to the rules there
        self.push_frame();
        frames += 1;
        let mut j = cur;
        while j < e {
            let named = matches!(self.tok(j), Tok::Pattern { name: Some(_), .. })
                || self.is_ident(j) && j + 1 < e && self.at(j + 1, P::At);
            if named {
                self.bind(j, LocalKind::Pattern);
            }
            j = self.next_item(j, e);
        }
        self.linear(cur, e);
        for _ in 0..frames {
            self.pop_frame();
        }
    }

    /// The left side `s..k` of a rule split: binds its pattern variables in a new frame, then colours it.
    fn rule_split(&mut self, s: usize, k: usize) {
        self.push_frame();
        for p in self.patterns(s, k) {
            self.bind(p, LocalKind::Pattern);
        }
        self.linear(s, k);
    }

    /// Items `s..e` with no rule split at this level: names, nested groups, `for` loops.
    fn linear(&mut self, s: usize, e: usize) {
        let mut arms_next = false;
        let mut i = s;
        while i < e {
            match self.tok(i) {
                Tok::Ident(_) => {
                    if i + 1 < e && self.at(i + 1, P::At) {
                        // `x @ p`: a pattern name, bound by its rule (if any) where the rule was split
                        if !self.bound_at(i) {
                            self.class[i] = Some(TokenClass::PatternVar);
                            self.probe_here(i);
                        }
                    } else {
                        self.resolve(i);
                    }
                }
                Tok::Pattern { .. } if i + 1 < e && self.at(i + 1, P::Colon) => {
                    self.probe_here(i);
                    i = self.type_run(i + 2, e);
                    continue;
                }
                Tok::Kw("for") if i + 2 < e && self.is_ident(i + 1) && *self.tok(i + 2) == Tok::Kw("in") => {
                    let mut b = i + 3;
                    while b < e && !self.at(b, P::LBrace) {
                        b = self.next_item(b, e);
                    }
                    self.linear(i + 3, b);
                    self.push_frame();
                    self.bind(i + 1, LocalKind::Local);
                    if b < e {
                        let end = self.group_end(b, e);
                        self.walk_group(b + 1, end, Group::Brace);
                    }
                    self.pop_frame();
                    i = if b < e { self.next_item(b, e) } else { e };
                    continue;
                }
                Tok::Kw("match") => arms_next = true,
                t if is_open(t) => {
                    let kind = match closer_of(t) {
                        Some(P::RParen) => Group::Paren,
                        Some(P::RBracket) => Group::Bracket,
                        _ if arms_next => Group::Arms,
                        _ => Group::Brace,
                    };
                    if kind == Group::Arms {
                        arms_next = false;
                    }
                    let end = self.group_end(i, e);
                    self.walk_group(i + 1, end, kind);
                    i = self.next_item(i, e);
                    continue;
                }
                _ => {}
            }
            i += 1;
        }
    }

    /// Whether the name at `i` is a binding occurrence in an active frame.
    fn bound_at(&self, i: usize) -> bool {
        self.env.get(self.name(i)).is_some_and(|v| v.iter().any(|b| b.tok == i))
    }

    /// A type after `:`: names (with `[...]` arguments), numbers, `|`, `(...) -> T`, up to a `where` (whose
    /// predicate is an expression). Its names are never variables; returns the index after it.
    fn type_run(&mut self, mut i: usize, e: usize) -> usize {
        loop {
            if i >= e {
                return i;
            }
            match self.tok(i) {
                Tok::Ident(n) if n == "where" => {
                    self.class[i] = Some(TokenClass::Keyword);
                    return i + 1;
                }
                Tok::Ident(_) => {
                    self.exempt(i);
                    i += 1;
                    if i < e && self.at(i, P::LBracket) {
                        i = self.type_args(i, e);
                    }
                }
                Tok::Num(_) => i += 1,
                Tok::Punct(P::LParen) => {
                    i = self.type_args(i, e);
                    if i < e && self.at(i, P::Arrow) {
                        i += 1;
                        continue;
                    }
                }
                _ => return i,
            }
            if i < e && self.at(i, P::Bar) {
                i += 1;
                continue;
            }
            if i < e && self.name(i) == "where" && self.is_ident(i) {
                continue;
            }
            return i;
        }
    }

    /// The bracket group of type arguments or parameter types opened at `i`: all names in it are types.
    fn type_args(&mut self, i: usize, e: usize) -> usize {
        let end = self.group_end(i, e);
        for j in i + 1..end {
            if self.name(j) == "where" && self.is_ident(j) {
                self.class[j] = Some(TokenClass::Keyword);
            } else {
                self.exempt(j);
            }
        }
        self.next_item(i, e)
    }
}
