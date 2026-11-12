//! `regular_expression("...")`: the common regular-expression syntax read into a string pattern ([`crate::strpat`])
//! and matched by the same backtracking matcher (decision S.9). Supported: literals and escapes (`\.`, `\n`, `\t`),
//! `.` (any character but a newline), classes `[abc]`, `[a-z]`, `[^...]` with `\d \w \s` inside, the escapes `\d \D
//! \w \W \s \S`, the anchors `^ $ \b`, groups `(...)` (numbered from 1, bound to `$1`, `$2`, ... for the
//! replacement text) and `(?:...)`, alternation `|`, and the quantifiers `* + ? {m} {m,} {m,n}`, each greedy or,
//! followed by `?`, lazy. Back references, look-around and flags are not supported (the pattern is then not read
//! and the call stays unevaluated).
//!
//! Literature: the syntax is the POSIX extended / Perl-compatible one as described in J. Friedl, *Mastering Regular
//! Expressions* (3rd ed., 2006), chapters 1 to 4; the matcher is the backtracking (NFA) kind described there.
use crate::strpat::{repeat, Anchor, Class, Node, Vars};
use mc_core::{Heap, Store};

/// The pattern of a regular expression, `None` when it uses syntax not supported here.
pub(crate) fn parse<'h>(h: &'h Heap, s: &str, vars: &mut Vars) -> Option<Node<'h>> {
    let mut p = P { cs: s.chars().collect(), i: 0, groups: 0, h, vars };
    let n = p.alt()?;
    (p.i == p.cs.len()).then_some(n)
}

struct P<'v, 'h> {
    cs: Vec<char>,
    i: usize,
    groups: usize,
    h: &'h Heap,
    vars: &'v mut Vars,
}

impl<'h> P<'_, 'h> {
    fn peek(&self) -> Option<char> {
        self.cs.get(self.i).copied()
    }

    fn alt(&mut self) -> Option<Node<'h>> {
        let mut alts = vec![self.cat()?];
        while self.peek() == Some('|') {
            self.i += 1;
            alts.push(self.cat()?);
        }
        Some(if alts.len() == 1 { alts.pop()? } else { Node::Alt(alts) })
    }

    fn cat(&mut self) -> Option<Node<'h>> {
        let mut items = vec![];
        while let Some(c) = self.peek() {
            if c == '|' || c == ')' {
                break;
            }
            let atom = self.atom()?;
            items.push(self.quantified(atom)?);
        }
        Some(Node::Cat(items))
    }

    fn quantified(&mut self, atom: Node<'h>) -> Option<Node<'h>> {
        let (min, max) = match self.peek() {
            Some('*') => (0, usize::MAX),
            Some('+') => (1, usize::MAX),
            Some('?') => (0, 1),
            Some('{') => {
                let save = self.i;
                match self.braces() {
                    Some(mn) => {
                        self.i -= 1; // braces consumed the closing brace; step back so the shared advance below works
                        mn
                    }
                    None => {
                        self.i = save;
                        return Some(atom);
                    }
                }
            }
            _ => return Some(atom),
        };
        self.i += 1;
        let lazy = self.peek() == Some('?');
        if lazy {
            self.i += 1;
        }
        if matches!(atom, Node::At(_)) {
            return None;
        }
        Some(Node::Mode(Box::new(repeat(atom, min, max)), !lazy))
    }

    /// `{m}`, `{m,}`, `{m,n}`: the counts, the closing brace consumed.
    fn braces(&mut self) -> Option<(usize, usize)> {
        self.i += 1;
        let num = |p: &mut Self| {
            let start = p.i;
            while p.peek().is_some_and(|c| c.is_ascii_digit()) {
                p.i += 1;
            }
            let s: String = p.cs[start..p.i].iter().collect();
            s.parse::<usize>().ok()
        };
        let m = num(self)?;
        let n = if self.peek() == Some(',') {
            self.i += 1;
            if self.peek() == Some('}') {
                usize::MAX
            } else {
                num(self)?
            }
        } else {
            m
        };
        (self.peek() == Some('}') && m <= n).then(|| {
            self.i += 1;
            (m, n)
        })
    }

    fn atom(&mut self) -> Option<Node<'h>> {
        let c = self.peek()?;
        self.i += 1;
        Some(match c {
            '.' => Node::One(Class::Not(vec![Class::Set(vec!['\n'])])),
            '^' => Node::At(Anchor::StartLine),
            '$' => Node::At(Anchor::EndLine),
            '(' => {
                let capture = if self.peek() == Some('?') {
                    if self.cs.get(self.i + 1) != Some(&':') {
                        return None;
                    }
                    self.i += 2;
                    false
                } else {
                    true
                };
                let var = if capture {
                    self.groups += 1;
                    Some(self.vars_of(self.groups))
                } else {
                    None
                };
                let inner = self.alt()?;
                if self.peek() != Some(')') {
                    return None;
                }
                self.i += 1;
                match var {
                    Some(v) => Node::Bind { var: v, p: Box::new(inner) },
                    None => inner,
                }
            }
            '[' => self.class()?,
            '\\' => self.escape(false)?,
            '*' | '+' | '?' | ')' => return None,
            c => Node::Lit(vec![c]),
        })
    }

    fn vars_of(&mut self, k: usize) -> usize {
        let s = self.h.sym(&format!("${k}"));
        match self.vars.syms.iter().position(|&x| x == s) {
            Some(i) => i,
            None => {
                self.vars.syms.push(s);
                self.vars.syms.len() - 1
            }
        }
    }

    /// After a backslash: a class, an anchor or a literal character (inside a class only classes and literals).
    fn escape(&mut self, in_class: bool) -> Option<Node<'h>> {
        let c = self.peek()?;
        self.i += 1;
        Some(match c {
            'd' => Node::One(Class::Digit),
            'D' => Node::One(Class::Not(vec![Class::Digit])),
            'w' => Node::One(Class::Set(vec!['_'])).or_word(),
            'W' => Node::One(Class::Not(vec![Class::Word, Class::Set(vec!['_'])])),
            's' => Node::One(Class::Space),
            'S' => Node::One(Class::Not(vec![Class::Space])),
            'b' if !in_class => Node::At(Anchor::WordBoundary),
            'A' if !in_class => Node::At(Anchor::StartString),
            'z' if !in_class => Node::At(Anchor::EndString),
            'n' => Node::Lit(vec!['\n']),
            't' => Node::Lit(vec!['\t']),
            'r' => Node::Lit(vec!['\r']),
            c if c.is_ascii_alphanumeric() => return None,
            c => Node::Lit(vec![c]),
        })
    }

    /// `[...]` after the opening bracket.
    fn class(&mut self) -> Option<Node<'h>> {
        let neg = self.peek() == Some('^');
        if neg {
            self.i += 1;
        }
        let mut parts: Vec<Class> = vec![];
        let mut chars: Vec<char> = vec![];
        let mut first = true;
        loop {
            let c = self.peek()?;
            if c == ']' && !first {
                self.i += 1;
                break;
            }
            first = false;
            let lo = if c == '\\' {
                self.i += 1;
                match self.escape(true)? {
                    Node::Lit(cs) => cs[0],
                    Node::One(k) => {
                        parts.push(k);
                        continue;
                    }
                    _ => return None,
                }
            } else {
                self.i += 1;
                c
            };
            if self.peek() == Some('-') && self.cs.get(self.i + 1).is_some_and(|&d| d != ']') {
                self.i += 1;
                let hi = self.peek()?;
                self.i += 1;
                let hi = if hi == '\\' {
                    match self.escape(true)? {
                        Node::Lit(cs) => cs[0],
                        _ => return None,
                    }
                } else {
                    hi
                };
                if hi < lo || (hi as u32 - lo as u32) > 0x10000 {
                    return None;
                }
                chars.extend((lo as u32..=hi as u32).filter_map(char::from_u32));
            } else {
                chars.push(lo);
            }
        }
        if !chars.is_empty() {
            parts.push(Class::Set(chars));
        }
        Some(Node::One(if neg {
            Class::Not(parts)
        } else if parts.len() == 1 {
            parts.pop()?
        } else {
            Class::Not(vec![Class::Not(parts)])
        }))
    }
}

impl Node<'_> {
    /// `\w`: a letter, a digit or an underscore.
    fn or_word(self) -> Self {
        Node::One(Class::Not(vec![Class::Not(vec![Class::Word, Class::Set(vec!['_'])])]))
    }
}
