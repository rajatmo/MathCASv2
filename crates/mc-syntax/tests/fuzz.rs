//! Fuzz tests with a fixed-seed generator (no external crates):
//! - random token streams and random text never make the reader panic, every error position is inside the text,
//!   and whatever reads without error prints back to the identical term;
//! - random terms, biased towards the shapes of the special syntax and near misses of them, print to text that
//!   reads back to the identical term.
mod common;
use common::raw;
use mc_api::{RefStore, Store, T};
use mc_syntax::{parse, read, Head, LineIndex, Printer};

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        // xorshift64*
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
    fn chance(&mut self, pct: u64) -> bool {
        self.next() % 100 < pct
    }
    fn pick<'a, X>(&mut self, xs: &'a [X]) -> &'a X {
        &xs[self.below(xs.len())]
    }
}

const TOKENS: &[&str] = &[
    "x", "y", "f", "g", "xs", "M.f", "`if`", "`a b`", "α", "π", "∞", "true", "false", "null", "e", "im", "_", "x_",
    "xs__", "___", "&1", "&2", "&", "0", "1", "42", "-", "2x", "3(", "1.5", "1.", "2e-3", "1.25~40", "3im", "0x1F",
    "0b101", "123456789012345678901234567890", "\"s\"", "\"a\\n\"", "\"", "(", ")", "[", "]", "{", "}", ",", ";", ":",
    ".", "...", "=", ":=", "|>", "->", ":>", "=>", "|", "==", "!=", "<", "<=", ">", ">=", "===", "!==", "+", "*", "/",
    "^", "@", "'", "''", "~", "!", "let", "var", "fn", "rule", "sym", "module", "pub", "import", "if", "then", "else",
    "while", "for", "in", "match", "when", "held", "and", "or", "not", "except", "where", "Int", "List[", "\n", "\n",
    "# c\n", "\t", "‽", "\\",
];

/// Reads `src`; if it is valid, checks the print round trip. Panics (fails the test) on any violation.
fn check_text(store: &RefStore, src: &str) {
    let parsed = parse(src);
    let index = LineIndex::new(src);
    for d in &parsed.errors {
        assert!(d.span.start <= d.span.end && d.span.end as usize <= src.len(), "span out of range in {src:?}");
        let _ = index.line_col(d.span.start);
        let _ = d.render(src);
    }
    if !parsed.errors.is_empty() {
        return;
    }
    let Ok(terms) = mc_syntax::lower_program(store, &parsed.program) else { return };
    let printed = Printer::new(store).program(&terms);
    match read(store, &printed) {
        Ok(again) => assert!(again == terms, "{src:?} printed as {printed:?} reads back differently"),
        Err(e) => panic!("{src:?} printed as {printed:?}, which does not read: {e:?}"),
    }
}

#[test]
fn random_token_streams() {
    let store = RefStore::new();
    let mut rng = Rng(seed(0x9E37_79B9_7F4A_7C15));
    let n = 12_000 * scale();
    for i in 0..n {
        let len = 1 + rng.below(24);
        let mut src = String::new();
        for _ in 0..len {
            src.push_str(rng.pick(TOKENS));
            if rng.chance(60) {
                src.push(' ');
            }
        }
        check_text(&store, &src);
        if i % 3000 == 0 {
            println!("[{i}/{n} {}%]", i * 100 / n);
        }
    }
}

#[test]
fn random_text() {
    let store = RefStore::new();
    let mut rng = Rng(seed(42));
    let chars: Vec<char> = "ab_x1 0.~e\"\\`'(){}[],;:=<>!|&@#+-*/^\nθπ∞é\u{1}\u{7f}".chars().collect();
    for _ in 0..6_000 * scale() {
        let len = rng.below(30);
        let src: String = (0..len).map(|_| *rng.pick(&chars)).collect();
        check_text(&store, &src);
    }
}

/// Random terms over one store.
struct Gen<'h> {
    st: &'h RefStore,
    rng: Rng,
}

const NAMES: &[&str] = &[
    "x", "y", "f", "g", "a", "b", "e", "im", "pi", "M.f", "if", "let", "fn", "and", "true", "false", "null", "&1",
    "&2", "&3", "&01", "a b", "x_", "π", "Int", "Real", "except", "where", "", "`",
];

impl<'h> Gen<'h> {
    fn sym(&mut self) -> T<'h> {
        let n = *self.rng.pick(NAMES);
        self.st.sym_term(self.st.sym(n))
    }
    fn plain(&mut self) -> T<'h> {
        let n = *self.rng.pick(&["x", "y", "a", "b", "n", "xs"]);
        self.st.sym_term(self.st.sym(n))
    }
    fn int(&self, v: i64) -> T<'h> {
        self.st.int(v)
    }

    fn number(&mut self) -> T<'h> {
        let st = self.st;
        match self.rng.below(12) {
            0 => st.int(*self.rng.pick(&[0, 1, -1, 2, -7, 42, i64::MAX, i64::MIN])),
            1 => {
                let limbs = [self.rng.next(), self.rng.next() | 1];
                st.big_int(self.rng.chance(50), &limbs)
            }
            2 => {
                let (p, q) = *self.rng.pick(&[(1i64, 2i64), (-3, 4), (5, 7), (-1, 3), (22, 7)]);
                st.rational(st.int(p), st.int(q))
            }
            3 => st.rational(st.big_int(true, &[3, 1]), st.int(7)),
            4 => st.real(*self.rng.pick(&[0.5, -0.0, 0.0, 1e300, 2.5e-8, 3.0, -1.5, 1e16, 123.456])),
            5 => {
                let s = *self.rng.pick(&["1.25", "-2", "1.", "1e-5", "1E5", "--1", "", ".5", "7e+3"]);
                let d = *self.rng.pick(&[40, 1, 0, -3]);
                mk(self.st, Head::BallLiteral, &[st.string(s), st.int(d)])
            }
            6 => {
                let k = self.number();
                let re = if self.rng.chance(80) { st.int(0) } else { st.real(0.0) };
                mk(self.st, Head::Complex, &[re, k])
            }
            _ => st.int(self.rng.below(20) as i64 - 5),
        }
    }

    fn leaf(&mut self) -> T<'h> {
        match self.rng.below(6) {
            0 | 1 => self.sym(),
            2 => self.number(),
            3 => self.st.string(self.rng.pick(&["", "a\"b\\c\n", "θ\t", "\u{1}"])),
            4 => self.pattern(),
            _ => self.plain(),
        }
    }

    fn pattern(&mut self) -> T<'h> {
        let h = *self.rng.pick(&[Head::Any, Head::Seq1, Head::Seq0]);
        match self.rng.below(3) {
            0 => mk(self.st, h, &[]),
            1 => {
                let x = self.plain();
                mk(self.st, h, &[x])
            }
            _ => {
                let x = self.sym();
                mk(self.st, h, &[x])
            }
        }
    }

    fn ty(&mut self, d: u32) -> T<'h> {
        if d == 0 {
            return if self.rng.chance(85) { self.sym() } else { self.int(2) };
        }
        match self.rng.below(7) {
            0 => {
                let a = self.ty(d - 1);
                let list = self.st.sym_term(self.st.sym("List"));
                self.st.app(list, &[a])
            }
            1 => {
                let ps: Vec<_> = (0..self.rng.below(3)).map(|_| self.ty(d - 1)).collect();
                let l = mk(self.st, Head::List, &ps);
                let r = self.ty(d - 1);
                mk(self.st, Head::FunType, &[l, r])
            }
            2 => {
                let ms: Vec<_> = (0..1 + self.rng.below(3)).map(|_| self.ty(d - 1)).collect();
                mk(self.st, Head::TypeUnion, &ms)
            }
            3 => {
                let t = self.ty(d - 1);
                let p = self.term(1);
                mk(self.st, Head::TypeWhere, &[t, p])
            }
            4 => self.term(1),
            _ => self.sym(),
        }
    }

    fn lambda_param(&mut self) -> T<'h> {
        let mut p = match self.rng.below(4) {
            0 => {
                let x = self.plain();
                let t = self.ty(1);
                mk(self.st, Head::Typed, &[x, t])
            }
            1 => {
                let x = self.plain();
                mk(self.st, Head::Rest, &[x])
            }
            2 => self.leaf(),
            _ => self.sym(),
        };
        if self.rng.chance(25) {
            let d = self.term(1);
            p = mk(self.st, Head::Optional, &[p, d]);
        }
        if self.rng.chance(20) {
            p = mk(self.st, Head::Held, &[p]);
        }
        p
    }

    fn fn_param(&mut self) -> T<'h> {
        let mut p = match self.rng.below(6) {
            0 => {
                let x = self.plain();
                mk(self.st, Head::Any, &[x])
            }
            1 => {
                let x = self.plain();
                let a = mk(self.st, Head::Any, &[x]);
                let t = self.ty(1);
                mk(self.st, Head::Typed, &[a, t])
            }
            2 => {
                let x = self.plain();
                mk(self.st, Head::Seq0, &[x])
            }
            3 => self.leaf(),
            _ => self.term(2),
        };
        if self.rng.chance(25) {
            let d = self.term(1);
            p = mk(self.st, Head::Optional, &[p, d]);
        }
        if self.rng.chance(15) {
            p = mk(self.st, Head::Held, &[p]);
        }
        p
    }

    fn list(&mut self, n: usize, d: u32) -> T<'h> {
        let xs: Vec<_> = (0..n).map(|_| self.term(d)).collect();
        mk(self.st, Head::List, &xs)
    }

    fn statement(&mut self, d: u32) -> T<'h> {
        match self.rng.below(8) {
            0 | 1 => {
                let h = if self.rng.chance(50) { Head::Let } else { Head::Var };
                let x = if self.rng.chance(80) { self.plain() } else { self.sym() };
                let x = if self.rng.chance(20) {
                    let t = self.ty(1);
                    mk(self.st, Head::Typed, &[x, t])
                } else {
                    x
                };
                let v = self.term(d);
                mk(self.st, h, &[x, v])
            }
            2 | 3 => {
                let h = if self.rng.chance(50) { Head::Assign } else { Head::Set };
                let x = self.plain();
                let v = self.term(d);
                mk(self.st, h, &[x, v])
            }
            4 => self.definition(d),
            _ => self.term(d),
        }
    }

    fn definition(&mut self, d: u32) -> T<'h> {
        let decs: Vec<_> = (0..self.rng.below(3))
            .map(|_| if self.rng.chance(50) { self.sym() } else { self.term(1) })
            .collect();
        let decs = mk(self.st, Head::List, &decs);
        match self.rng.below(6) {
            0 | 1 => {
                let f = if self.rng.chance(80) { self.plain() } else { self.sym() };
                let ps: Vec<_> = (0..self.rng.below(3)).map(|_| self.fn_param()).collect();
                let mut lhs = self.st.app(f, &ps);
                for _ in 0..self.rng.below(3) {
                    let c = self.term(1);
                    lhs = mk(self.st, Head::Condition, &[lhs, c]);
                }
                let os: Vec<_> = (0..self.rng.below(2)).map(|_| self.fn_param()).collect();
                let opts = mk(self.st, Head::List, &os);
                let r = if self.rng.chance(50) { self.st.sym_term(self.st.sym("null")) } else { self.ty(1) };
                let body = self.term(d);
                mk(self.st, Head::DefineFn, &[lhs, body, opts, r, decs])
            }
            2 | 3 => {
                let mut lhs = self.term(d);
                if self.rng.chance(40) {
                    let c = self.term(1);
                    lhs = mk(self.st, Head::Condition, &[lhs, c]);
                }
                let rhs = self.term(d);
                let r = mk(self.st, if self.rng.chance(50) { Head::Rule } else { Head::RuleDelayed }, &[lhs, rhs]);
                mk(self.st, Head::DefineRule, &[r, decs])
            }
            4 => {
                let s = self.sym();
                mk(self.st, Head::DefineSym, &[s, decs])
            }
            _ => {
                let n = self.plain();
                let stmts: Vec<_> = (0..self.rng.below(3))
                    .map(|_| {
                        let s = self.statement(1);
                        if self.rng.chance(30) {
                            mk(self.st, Head::Public, &[s])
                        } else {
                            s
                        }
                    })
                    .collect();
                let body = mk(self.st, Head::List, &stmts);
                mk(self.st, Head::Module, &[n, body])
            }
        }
    }

    /// A term of depth at most `d`.
    fn term(&mut self, d: u32) -> T<'h> {
        if d == 0 || self.rng.chance(25) {
            return self.leaf();
        }
        let st = self.st;
        let d1 = d - 1;
        match self.rng.below(30) {
            0 => {
                let n = 1 + self.rng.below(3);
                let ps: Vec<_> = (1..=n).map(|k| st.sym_term(st.sym(&format!("&{k}")))).collect();
                let l = mk(self.st, Head::List, &ps);
                let body = self.term(d1);
                mk(self.st, Head::Fn, &[l, body])
            }
            1 => {
                let ps: Vec<_> = (0..self.rng.below(3)).map(|_| self.lambda_param()).collect();
                let l = mk(self.st, Head::List, &ps);
                let body = self.term(d1);
                mk(self.st, Head::Fn, &[l, body])
            }
            2 => {
                let ss: Vec<_> = (0..self.rng.below(4)).map(|_| self.statement(d1)).collect();
                mk(self.st, Head::Block, &ss)
            }
            3 => self.definition(d1),
            4 => {
                let n = 1 + self.rng.below(3);
                let mut args = vec![self.term(d1), self.term(d1)];
                if n == 3 {
                    args.push(self.term(d1));
                }
                if self.rng.chance(30) {
                    let inner = mk(self.st, Head::If, &[self.plain(), self.term(d1)]);
                    args[1] = inner;
                }
                mk(self.st, Head::If, &args)
            }
            5 => {
                let arms: Vec<_> = (0..self.rng.below(3))
                    .map(|_| {
                        let p = self.term(d1);
                        let r = self.term(d1);
                        mk(self.st, Head::RuleDelayed, &[p, r])
                    })
                    .collect();
                let arms = mk(self.st, Head::List, &arms);
                let e = self.term(d1);
                mk(self.st, Head::Match, &[e, arms])
            }
            6 => {
                let p = self.pattern();
                let t = self.ty(2);
                mk(self.st, Head::Typed, &[p, t])
            }
            7 => {
                let b = self.term(d1);
                let e = if self.rng.chance(60) { self.int(-1) } else { self.term(d1) };
                mk(self.st, Head::Pow, &[b, e])
            }
            8 => {
                let b = self.term(d1);
                mk(self.st, Head::Mul, &[self.int(-1), b])
            }
            9 => {
                let p = self.number();
                let q = self.number();
                let r = mk(self.st, Head::Pow, &[q, self.int(-1)]);
                mk(self.st, Head::Mul, &[p, r])
            }
            10 => {
                let n = 2 + self.rng.below(3);
                let xs: Vec<_> = (0..n)
                    .map(|_| if self.rng.chance(40) { self.number() } else { self.term(d1) })
                    .collect();
                mk(self.st, *self.rng.pick(&[Head::Add, Head::Mul]), &xs)
            }
            11 => {
                let f = self.term(d1);
                let mut args: Vec<_> = (0..self.rng.below(3)).map(|_| self.term(d1)).collect();
                if self.rng.chance(50) {
                    let rules: Vec<_> = (0..self.rng.below(3))
                        .map(|_| {
                            let n = self.sym();
                            let v = self.term(d1);
                            mk(self.st, Head::Rule, &[n, v])
                        })
                        .collect();
                    args.push(mk(self.st, Head::Options, &rules));
                }
                if self.rng.chance(30) {
                    let p = if self.rng.chance(50) { self.pattern() } else { self.term(0) };
                    let v = self.term(d1);
                    args.push(mk(self.st, *self.rng.pick(&[Head::Optional, Head::Set]), &[p, v]));
                }
                st.app(f, &args)
            }
            12 => {
                let b = self.term(d1);
                let mut args = vec![b];
                for _ in 0..1 + self.rng.below(2) {
                    args.push(if self.rng.chance(50) {
                        let i = self.term(d1);
                        let j = self.term(d1);
                        mk(self.st, Head::Span, &[i, j])
                    } else {
                        self.term(d1)
                    });
                }
                mk(self.st, Head::Part, &args)
            }
            13 => {
                let n = *self.rng.pick(&[1, 2, 4, 5, 0]);
                let dv = mk(self.st, Head::Derivative, &[self.int(n)]);
                let f = if self.rng.chance(70) { self.sym() } else { self.term(d1) };
                st.app(dv, &[f])
            }
            14 => {
                let x = self.sym();
                let p = self.term(d1);
                mk(self.st, Head::Named, &[x, p])
            }
            15 => {
                let x = self.plain();
                let xs = self.term(d1);
                let body = mk(self.st, Head::Block, &[self.statement(d1)]);
                mk(self.st, Head::For, &[x, xs, body])
            }
            16 => {
                let c = self.term(d1);
                let body = mk(self.st, Head::Block, &[self.statement(d1)]);
                mk(self.st, Head::While, &[c, body])
            }
            17 => {
                let n = self.rng.below(4);
                self.list(n, d1)
            }
            18 => {
                let path = self.sym();
                let ns: Vec<_> = (0..self.rng.below(3)).map(|_| self.sym()).collect();
                let l = mk(self.st, Head::List, &ns);
                match self.rng.below(3) {
                    0 => mk(self.st, Head::Import, &[path]),
                    1 => mk(self.st, Head::Import, &[path, l]),
                    _ => mk(self.st, Head::ImportExcept, &[path, l]),
                }
            }
            _ => {
                // Any known head with a random arity, or an arbitrary call.
                let head = if self.rng.chance(70) {
                    let h = *self.rng.pick(Head::ALL);
                    st.sym_term(st.sym(h.name()))
                } else {
                    self.term(d1)
                };
                let args: Vec<_> = (0..self.rng.below(4)).map(|_| self.term(d1)).collect();
                st.app(head, &args)
            }
        }
    }
}


/// `h(args)` for a known head.
fn mk<'h>(st: &'h RefStore, h: Head, args: &[T<'h>]) -> T<'h> {
    st.call(st.sym(h.name()), args)
}

/// Checks that `text` reads back as exactly `terms`; describes the difference otherwise.
fn faithful(store: &RefStore, how: &str, terms: &[T<'_>], text: &str) -> Option<String> {
    let show = |ts: &[T<'_>]| ts.iter().map(|&t| raw(store, t)).collect::<Vec<_>>().join("; ");
    match read(store, text) {
        Ok(back) if back == terms => None,
        Ok(back) => Some(format!("{how}: {}\n   printed {text:?}\n   reads   {}", show(terms), show(&back))),
        Err(e) => Some(format!("{how}: {}\n   printed {text:?}\n   error {}", show(terms), e[0].render(text))),
    }
}

#[test]
fn random_terms_print_faithfully() {
    let store = RefStore::new();
    let mut g = Gen { st: &store, rng: Rng(seed(7)) };
    let p = Printer::new(&store);
    let (batches, size) = (500 * scale(), 40);
    let mut failures = Vec::new();
    for i in 0..batches {
        let terms: Vec<_> = (0..size).map(|_| g.term(5)).collect();
        // As one program (bindings of `let` and `var` statements carry over), and as separate expressions.
        let exprs: Vec<String> = terms.iter().map(|&t| p.expr(t)).collect();
        let whole = [("program", p.program(&terms)), ("exprs", exprs.join("\n"))];
        if i % 100 == 0 {
            println!("[{i}/{batches} {}%]", i * 100 / batches);
        }
        if whole.iter().all(|(how, text)| faithful(&store, how, &terms, text).is_none()) {
            continue;
        }
        // Narrow down to the failing terms.
        for &t in &terms {
            for (how, text) in [("statement", p.program(&[t])), ("expr", p.expr(t))] {
                failures.extend(faithful(&store, how, &[t], &text));
            }
        }
        if failures.is_empty() {
            failures.push(format!("batch {i} fails only as a whole: {}", whole[0].1));
        }
        if failures.len() >= 6 {
            break;
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

/// The seed, overridable with `MC_FUZZ_SEED` for longer campaigns.
fn seed(default: u64) -> u64 {
    std::env::var("MC_FUZZ_SEED").ok().and_then(|s| s.parse::<u64>().ok()).map_or(default, |s| s.wrapping_mul(default) | 1)
}

/// A multiplier of the case counts, from `MC_FUZZ_SCALE` (default 1).
fn scale() -> usize {
    std::env::var("MC_FUZZ_SCALE").ok().and_then(|s| s.parse().ok()).unwrap_or(1)
}
