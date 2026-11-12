//! Text to boxes through the parser (design 08, sections 4.5 and 4.6):
//!
//! - [`typeset_output`]: printed output (`value.printed`) to boxes, with the output substitutions of 4.5
//!   (juxtaposition for `*`, `π ∞ ⅇ ⅈ`, `→ ≤ ≥ ≠ ⩵`, strings without quotes, primes).
//! - [`typeset_input`]: linear input text to boxes (`Ctrl+Shift+N`): every construct with a box form becomes the
//!   box; all other text is kept exactly as typed.
//!
//! Both walk the syntax tree and copy the source text between the children they convert, so text without a box
//! form passes through unchanged (spacing, comments, operators as written).
use mc_boxes::{BigOp, Item, Node, Row, RowPath, Step, MAX_NESTING};
use std::borrow::Cow;
use std::collections::HashMap;
use std::ops::Range;
use mc_syntax::ast::{BinOp, Expr, ExprKind, MulOp, Stmt, StmtKind};
use mc_syntax::Diagnostic;

/// Typeset output: the boxes, where their items come from in the printed text, and whether the value is a failure
/// (drawn as linear text in the message colour).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Typeset {
    /// the content
    pub row: Row,
    /// the printed value is `failure(...)`: `row` is its plain text
    pub message: bool,
    /// the printed text each item stands for (a caret, a selection and Find inside outputs go through it)
    pub map: SourceMap,
}

/// Options of typeset output.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OutputOptions {
    /// a list of two or more lists of equal length (none of them nested further, at most [`MATRIX_MAX`] rows and
    /// columns) is drawn as a grid in brackets (decision NB6.3: on by default)
    pub matrices: bool,
}

impl Default for OutputOptions {
    fn default() -> OutputOptions {
        OutputOptions { matrices: true }
    }
}

/// The most rows and columns a list of lists may have to be drawn as a matrix (larger ones are data, not matrices,
/// and stay linear: a grid of thousands of rows would be harder to read than the wrapped list).
pub const MATRIX_MAX: usize = 64;

/// Where the items of typeset output come from in the printed text: for every item of a row, the byte range of the
/// printed text it stands for (a symbol added by the conversion stands for the text it replaces; a box for its whole
/// expression), and the maps of the slots of each box, in slot order ([`Node::slots`]).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SourceMap {
    /// one byte range per item
    pub spans: Vec<Range<usize>>,
    /// the slot maps of the box items, by item index
    pub boxes: HashMap<usize, Vec<SourceMap>>,
}

impl SourceMap {
    /// A map where every one of `n` items stands for `span` (items the conversion makes up).
    fn synthetic(n: usize, span: Range<usize>) -> SourceMap {
        SourceMap { spans: vec![span; n], boxes: HashMap::new() }
    }

    /// The map of the row at `path`.
    pub fn row(&self, path: &[Step]) -> Option<&SourceMap> {
        let mut m = self;
        for s in path {
            m = m.boxes.get(&s.item)?.get(s.slot)?;
        }
        Some(m)
    }

    /// The byte range of the whole row (`None` for an empty row).
    fn total(&self) -> Option<Range<usize>> {
        let lo = self.spans.iter().map(|s| s.start).min()?;
        let hi = self.spans.iter().map(|s| s.end).max()?;
        Some(lo..hi)
    }

    /// The printed text items `range` of the row at `path` stand for: from the first's start to the last's end.
    pub fn span(&self, path: &[Step], range: Range<usize>) -> Option<Range<usize>> {
        let m = self.row(path)?;
        let spans = m.spans.get(range)?;
        let lo = spans.iter().map(|s| s.start).min()?;
        let hi = spans.iter().map(|s| s.end).max()?;
        Some(lo..hi)
    }

    /// Whether items `range` of the row at `path` stand for one stretch of the printed text in order (so that the
    /// text of [`SourceMap::span`] is exactly what they show); a fraction made of `a*b/c*d` shows `a b` over `c d`,
    /// and its numerator alone is not one stretch.
    pub fn contiguous(&self, path: &[Step], range: Range<usize>, printed: &str) -> bool {
        let Some(m) = self.row(path) else { return false };
        let Some(spans) = m.spans.get(range) else { return false };
        spans.windows(2).all(|w| {
            let (a, b) = (&w[0], &w[1]);
            // the same text (characters of one escaped string), or in order with only spaces and `*` between
            a == b || a.end <= b.start && printed.get(a.end..b.start).is_some_and(|g| g.chars().all(|c| c == ' ' || c == '*'))
        })
    }

    /// The caret position (row path, offset) for byte `b` of the printed text: inside the innermost box slot that
    /// holds it, before the first item that starts at or after it.
    pub fn caret_at(&self, b: usize) -> (RowPath, usize) {
        let mut path = Vec::new();
        let mut m = self;
        loop {
            // a box item strictly around b: into the slot holding it
            let inner = m.spans.iter().enumerate().find_map(|(i, s)| {
                let slots = m.boxes.get(&i)?;
                if !(s.start < b && b < s.end) {
                    return None;
                }
                slots.iter().enumerate().find(|(_, sm)| sm.total().is_some_and(|t| t.start <= b && b <= t.end)).map(|(k, sm)| (i, k, sm))
            });
            match inner {
                Some((i, k, sm)) => {
                    path.push(Step { item: i, slot: k });
                    m = sm;
                }
                None => {
                    // before the first item that starts at or after b
                    let offset = m.spans.iter().take_while(|s| s.start < b).count();
                    return (path, offset);
                }
            }
        }
    }

    /// The byte of the printed text at caret position (`path`, `offset`): the start of the item after it, or the end
    /// of the row's last item.
    pub fn byte_at(&self, path: &[Step], offset: usize) -> Option<usize> {
        let m = self.row(path)?;
        match m.spans.get(offset) {
            Some(s) => Some(s.start),
            // the end of the row: the end of the furthest text it stands for (terms may be reordered)
            None => m.spans.iter().map(|s| s.end).max(),
        }
    }

    /// The smallest selection of items that covers bytes `range` of the printed text (Find in a typeset output):
    /// the row and the item range, inside a box slot when the bytes are all within one.
    pub fn find(&self, range: Range<usize>) -> Option<(RowPath, Range<usize>)> {
        if range.start >= range.end {
            return None;
        }
        let mut path = Vec::new();
        let mut m = self;
        loop {
            let hit: Vec<usize> =
                (0..m.spans.len()).filter(|&i| m.spans[i].start < range.end && m.spans[i].end > range.start).collect();
            let (first, last) = (*hit.first()?, *hit.last()?);
            if first == last {
                if let Some(slots) = m.boxes.get(&first) {
                    let inside = slots.iter().enumerate().find(|(_, sm)| {
                        sm.total().is_some_and(|t| t.start <= range.start && range.end <= t.end)
                    });
                    if let Some((k, sm)) = inside {
                        path.push(Step { item: first, slot: k });
                        m = sm;
                        continue;
                    }
                }
            }
            return Some((path, first..last + 1));
        }
    }
}

/// Items being converted, with their spans.
#[derive(Default)]
struct Out {
    items: Vec<Item>,
    map: SourceMap,
}

impl Out {
    fn push(&mut self, it: Item, span: Range<usize>) {
        self.items.push(it);
        self.map.spans.push(span);
    }

    fn push_box(&mut self, n: Node, slots: Vec<SourceMap>, span: Range<usize>) {
        self.map.boxes.insert(self.items.len(), slots);
        self.push(Item::Box(Box::new(n)), span);
    }

    /// Appends a converted row (its box maps move with it).
    fn append(&mut self, (row, map): (Row, SourceMap)) {
        let base = self.items.len();
        self.items.extend(row.0);
        self.map.spans.extend(map.spans);
        for (i, b) in map.boxes {
            self.map.boxes.insert(base + i, b);
        }
    }

    fn done(self) -> (Row, SourceMap) {
        (Row(self.items), self.map)
    }
}

/// Printed output to boxes with the default options. Text that does not parse (elisions, opaque objects) stays
/// linear text.
pub fn typeset_output(printed: &str) -> Typeset {
    typeset_output_with(printed, OutputOptions::default())
}

/// Printed output to boxes.
pub fn typeset_output_with(printed: &str, opts: OutputOptions) -> Typeset {
    on_big_stack(|| {
        let parsed = mc_syntax::parse(printed);
        if !parsed.errors.is_empty() {
            let (row, map) = plain(printed);
            return Typeset { row, message: false, map };
        }
        if let [Stmt { kind: StmtKind::Expr(e), .. }] = parsed.program.stmts.as_slice() {
            if let ExprKind::Call(f, _) = &e.kind {
                if matches!(&f.kind, ExprKind::Name(n) if n == "failure") {
                    let (row, map) = plain(printed);
                    return Typeset { row, message: true, map };
                }
            }
        }
        let c = Conv { src: printed, output: true, opts };
        let mut out = Out::default();
        c.program(&parsed.program.stmts, &mut out, 0);
        let (row, map) = out.done();
        Typeset { row, message: false, map }
    })
}

/// Linear input text to boxes; on a syntax error nothing is converted and the errors are returned.
pub fn typeset_input(text: &str) -> Result<Row, Vec<Diagnostic>> {
    on_big_stack(|| {
        // canonical box text already in the input is read first, so converting twice changes nothing
        let parsed = mc_syntax::parse(text);
        if !parsed.errors.is_empty() {
            return Err(parsed.errors);
        }
        let c = Conv { src: text, output: false, opts: OutputOptions::default() };
        let mut out = Out::default();
        c.program(&parsed.program.stmts, &mut out, 0);
        Ok(out.done().0)
    })
}

/// A row of the characters of `s`, with no placeholders (output text is never a template), each standing for
/// itself.
fn plain(s: &str) -> (Row, SourceMap) {
    let mut out = Out::default();
    for (k, c) in s.char_indices() {
        out.push(Item::Char(c), k..k + c.len_utf8());
    }
    out.done()
}

/// Stack for the conversion: the syntax tree is up to [`mc_syntax::MAX_DEPTH`] deep and its frames are large in
/// unoptimised builds, so the walk runs on a thread whose stack is reserved, not committed.
const STACK_BYTES: usize = 32 << 20;

fn on_big_stack<R: Send, F: FnOnce() -> R + Send>(f: F) -> R {
    std::thread::scope(|s| {
        match std::thread::Builder::new().name("mc-boxes".into()).stack_size(STACK_BYTES).spawn_scoped(s, f) {
            Ok(h) => h.join().unwrap_or_else(|p| std::panic::resume_unwind(p)),
            Err(e) => panic!("cannot start the typesetting thread: {e}"),
        }
    })
}

/// A child of a node, for copying the source text around it.
enum Child<'a> {
    E(&'a Expr),
    S(&'a Stmt),
}

impl Child<'_> {
    fn span(&self) -> (usize, usize) {
        let s = match self {
            Child::E(e) => e.span,
            Child::S(s) => s.span,
        };
        (s.start as usize, s.end as usize)
    }
}

struct Conv<'a> {
    src: &'a str,
    output: bool,
    opts: OutputOptions,
}

/// Output spellings of operators (design 08, section 4.5), longest first; `===` and `=!=` stay as they are; a
/// minus is the minus sign `−`, not the hyphen.
const OUTPUT_OPS: &[(&str, &str)] =
    &[("===", "==="), ("=!=", "=!="), ("<->", "↔"), ("->", "→"), (":>", "⧴"), ("<=", "≤"), (">=", "≥"), ("!=", "≠"), ("==", "⩵"), ("=>", "⇒"), ("-", "−")];

/// Names drawn as symbols in output.
fn output_name(n: &str) -> Option<char> {
    match n {
        "pi" => Some('π'),
        "inf" => Some('∞'),
        "e" => Some('ⅇ'),
        "im" => Some('ⅈ'),
        _ => None,
    }
}

fn primes(n: u64) -> Option<&'static str> {
    match n {
        1 => Some("′"),
        2 => Some("″"),
        3 => Some("‴"),
        4 => Some("⁗"),
        _ => None,
    }
}

fn span_of(e: &Expr) -> Range<usize> {
    e.span.start as usize..e.span.end as usize
}

impl Conv<'_> {
    fn text(&self, lo: usize, hi: usize, out: &mut Out) {
        let hi = hi.max(lo);
        let t = &self.src[lo..hi];
        if !self.output {
            for (k, c) in t.char_indices() {
                out.push(if c == '□' { Item::Placeholder } else { Item::Char(c) }, lo + k..lo + k + c.len_utf8());
            }
            return;
        }
        let mut pos = lo;
        'outer: while let Some(c) = self.src[pos..hi].chars().next() {
            let rest = &self.src[pos..hi];
            for (from, to) in OUTPUT_OPS {
                if rest.starts_with(from) {
                    for ch in to.chars() {
                        out.push(Item::Char(ch), pos..pos + from.len());
                    }
                    pos += from.len();
                    continue 'outer;
                }
            }
            out.push(Item::Char(c), pos..pos + c.len_utf8());
            pos += c.len_utf8();
        }
    }

    /// Copies `lo..hi`, converting the children (sorted, non-overlapping) and copying the text between them.
    fn splice(&self, lo: usize, hi: usize, children: Vec<Child<'_>>, out: &mut Out, depth: usize) {
        let mut pos = lo;
        for ch in children {
            let (a, b) = ch.span();
            if a < pos || b > hi {
                continue;
            }
            self.text(pos, a, out);
            match ch {
                Child::E(e) => self.expr(e, out, depth),
                Child::S(s) => self.stmt(s, out, depth),
            }
            pos = b;
        }
        self.text(pos, hi, out);
    }

    fn program(&self, stmts: &[Stmt], out: &mut Out, depth: usize) {
        self.splice(0, self.src.len(), stmts.iter().map(Child::S).collect(), out, depth);
    }

    fn stmt(&self, s: &Stmt, out: &mut Out, depth: usize) {
        let (lo, hi) = (s.span.start as usize, s.span.end as usize);
        let children: Vec<Child> = match &s.kind {
            StmtKind::Expr(e) => vec![Child::E(e)],
            StmtKind::Let { value, .. } => vec![Child::E(value)],
            StmtKind::Fn { body, .. } => vec![Child::E(body)],
            StmtKind::Rule { lhs, rhs, .. } => vec![Child::E(lhs), Child::E(rhs)],
            StmtKind::Module { body, .. } => body.iter().map(Child::S).collect(),
            StmtKind::Pub(inner) => vec![Child::S(inner)],
            _ => vec![],
        };
        self.splice(lo, hi, children, out, depth);
    }

    /// A slot's row from an expression, without the parentheses around it if it is parenthesised.
    fn slot(&self, e: &Expr, depth: usize) -> (Row, SourceMap) {
        let mut out = Out::default();
        let (lo, hi) = (e.span.start as usize, e.span.end as usize);
        if self.parenthesised(e) {
            // the same expression with its span shrunk inside the parentheses
            let inner = Expr { kind: e.kind.clone(), span: mc_syntax::Span::new(lo + 1, hi - 1) };
            self.expr(&inner, &mut out, depth);
        } else {
            self.expr(e, &mut out, depth);
        }
        if out.items.is_empty() {
            out.push(Item::Placeholder, lo..hi);
        }
        out.done()
    }

    fn parenthesised(&self, e: &Expr) -> bool {
        let t = &self.src[e.span.start as usize..e.span.end as usize];
        mc_boxes::is_one_group(t, '(')
    }

    fn expr(&self, e: &Expr, out: &mut Out, depth: usize) {
        let (lo, hi) = (e.span.start as usize, e.span.end as usize);
        if depth >= MAX_NESTING {
            self.text(lo, hi, out);
            return;
        }
        let d = depth + 1;
        // output: a product with a negative coefficient is a minus before the product without it (`-1*cos(x)*x^2`
        // is drawn −x² cos(x)); a sum is drawn with subtractions, ordered by degree (NB6.12)
        if self.output && matches!(e.kind, ExprKind::Mul(_)) && !self.parenthesised(e) {
            if let (true, Some(body)) = self.unsigned_term(e) {
                out.push(Item::Char('−'), lo..lo);
                self.expr(&body, out, depth);
                return;
            }
        }
        if let ExprKind::Add(xs) = &e.kind {
            if self.output && !self.parenthesised(e) {
                self.sum(xs, out, d);
                return;
            }
        }
        if let Some((n, maps)) = self.matrix(e, d) {
            // the grid between the list's own brackets (they stretch to its height)
            out.push(Item::Char('['), lo..lo + 1);
            out.push_box(n, maps, lo..hi);
            out.push(Item::Char(']'), hi - 1..hi);
            return;
        }
        if let Some((n, maps)) = self.as_box(e, d) {
            if let Some((minus, _)) = self.negative_coefficient(e).filter(|_| matches!(n, Node::Fraction { .. })) {
                out.push(Item::Char('−'), minus..minus + 1);
            }
            // a parenthesised box keeps its parentheses as characters around it
            if self.parenthesised(e) {
                out.push(Item::Char('('), lo..lo + 1);
                out.push_box(n, maps, lo + 1..hi - 1);
                out.push(Item::Char(')'), hi - 1..hi);
            } else {
                out.push_box(n, maps, lo..hi);
            }
            return;
        }
        match &e.kind {
            ExprKind::Name(n) if self.output => match output_name(n) {
                Some(c) if !self.parenthesised(e) => out.push(Item::Char(c), lo..hi),
                _ => self.text(lo, hi, out),
            },
            ExprKind::Str(s) if self.output && !self.parenthesised(e) => {
                // characters map to themselves when the literal has no escapes
                if hi - lo == s.len() + 2 {
                    for (k, c) in s.char_indices() {
                        out.push(Item::Char(c), lo + 1 + k..lo + 1 + k + c.len_utf8());
                    }
                } else {
                    for c in s.chars() {
                        out.push(Item::Char(c), lo..hi);
                    }
                }
            }
            ExprKind::Num(n) if self.output && n.imag => {
                let t = &self.src[lo..hi];
                match t.strip_suffix("im") {
                    Some(digits) => {
                        for (k, c) in digits.char_indices() {
                            out.push(Item::Char(c), lo + k..lo + k + c.len_utf8());
                        }
                        out.push(Item::Char('ⅈ'), hi - 2..hi);
                    }
                    None => self.text(lo, hi, out),
                }
            }
            ExprKind::Prime(f, n) if self.output => match primes(u64::from(*n)) {
                Some(p) => {
                    let fe = f.span.end as usize;
                    self.splice(lo, fe, vec![Child::E(f)], out, depth);
                    for c in p.chars() {
                        out.push(Item::Char(c), fe..hi);
                    }
                }
                None => self.splice(lo, hi, vec![Child::E(f)], out, depth),
            },
            ExprKind::Mul(fs) if self.output => {
                let paren = self.parenthesised(e);
                if paren {
                    out.push(Item::Char('('), lo..lo + 1);
                }
                self.juxtapose(fs.iter().map(|(_, f)| f), out, depth);
                if paren {
                    out.push(Item::Char(')'), hi - 1..hi);
                }
            }
            ExprKind::Call(f, args) => {
                // derivative(n)(f)(x) and derivative(n)(f): f with primes
                if let Some((fun, n)) = self.derivative_of(e) {
                    if let Some(p) = primes(n) {
                        self.expr(fun, out, d);
                        for c in p.chars() {
                            out.push(Item::Char(c), lo..hi);
                        }
                    } else {
                        let (base, bm) = self.slot(fun, d);
                        let exp = Row::text(&format!("({n})"));
                        let em = SourceMap::synthetic(exp.len(), lo..hi);
                        out.push_box(Node::Superscript { base, exp }, vec![bm, em], lo..hi);
                    }
                    if !matches!(&f.kind, ExprKind::Call(g, _) if matches!(&g.kind, ExprKind::Name(_))) {
                        // applied: copy the argument list
                        let start = args.first().map_or(hi, |a| a.value.span.start as usize);
                        let open = self.src[..start].rfind('(').unwrap_or(start);
                        self.splice(open, hi, args.iter().map(|a| Child::E(&a.value)).collect(), out, depth);
                    }
                    return;
                }
                let mut ch = vec![Child::E(f)];
                ch.extend(args.iter().map(|a| Child::E(&a.value)));
                self.splice(lo, hi, ch, out, depth);
            }
            _ => self.splice(lo, hi, children(e), out, depth),
        }
    }

    /// Output products: factors side by side with a thin space, `×` between two numbers. The space stands for the
    /// text between the two factors when that is their operator, else for nothing.
    fn juxtapose<'e>(&self, factors: impl Iterator<Item = &'e Expr>, out: &mut Out, depth: usize) {
        // numbers first, then names and their powers, then everything else (`cos(x)*x^2` is x² cos(x)); the
        // order within each class is the engine's
        let mut factors: Vec<&Expr> = factors.collect();
        factors.sort_by_key(|f| factor_class(f));
        let mut prev: Option<&Expr> = None;
        for f in factors {
            let num = matches!(f.kind, ExprKind::Num(_));
            if let Some(p) = prev {
                let prev_num = matches!(p.kind, ExprKind::Num(_));
                let (pe, s) = (p.span.end as usize, f.span.start as usize);
                let gap = if pe <= s && self.src[pe..s].chars().all(|c| c == ' ' || c == '*') { pe..s } else { pe..pe };
                // a name before a bracketed factor: a centred dot, so that the product `x·(x − a)` is told from the
                // call `x(x − a)` (round 20)
                let dot = matches!(p.kind, ExprKind::Name(_)) && self.parenthesised(f);
                out.push(Item::Char(if prev_num && num { '×' } else if dot { '\u{22C5}' } else { '\u{2009}' }), gap);
            }
            self.expr(f, out, depth);
            prev = Some(f);
        }
    }

    /// The factors of a product as a row: output juxtaposes them, input joins them with `*`.
    fn factors_row(&self, fs: &[&Expr], depth: usize) -> (Row, SourceMap) {
        if let [one] = fs {
            return self.slot(one, depth);
        }
        let mut out = Out::default();
        if self.output {
            self.juxtapose(fs.iter().copied(), &mut out, depth);
        } else {
            for (k, f) in fs.iter().enumerate() {
                if k > 0 {
                    // keep the source's own operator between neighbours (`2x`, `a * b`), `*` otherwise
                    let (pe, s) = (fs[k - 1].span.end as usize, f.span.start as usize);
                    let gap = self.src.get(pe..s).filter(|g| g.trim() == "*" || g.is_empty());
                    match gap {
                        Some(g) => self.text(pe, pe + g.len(), &mut out),
                        None => out.push(Item::Char('*'), pe..pe),
                    }
                }
                self.expr(f, &mut out, depth);
            }
        }
        out.done()
    }

    /// A rational literal `p/q` (possibly parenthesised, `p` possibly negated): its numerator and denominator.
    fn rational<'e>(&self, e: &'e Expr) -> Option<(&'e Expr, &'e Expr)> {
        let ExprKind::Mul(fs) = &e.kind else { return None };
        match fs.as_slice() {
            [(MulOp::Times, p), (MulOp::Div, q)] => {
                let int = |x: &Expr| {
                    let digits = self.src_of(x).strip_prefix('-').unwrap_or(self.src_of(x));
                    matches!(&x.kind, ExprKind::Num(n) if !n.imag) && !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())
                };
                let p_ok = int(p) || matches!(&p.kind, ExprKind::Neg(x) if int(x));
                (p_ok && int(q)).then_some((p, q))
            }
            _ => None,
        }
    }

    /// The rational coefficient of an output product when it is negative: the byte of its minus sign and the
    /// numerator without it (`(-2/3)*x` is drawn `−` before the fraction 2x/3).
    fn negative_coefficient(&self, e: &Expr) -> Option<(usize, Expr)> {
        if !self.output {
            return None;
        }
        let ExprKind::Mul(fs) = &e.kind else { return None };
        let (p, _) = fs.iter().find_map(|(op, f)| if *op == MulOp::Times { self.rational(f) } else { None })?;
        match &p.kind {
            ExprKind::Neg(x) => Some((p.span.start as usize, (**x).clone())),
            _ if self.src_of(p).starts_with('-') => {
                let s = p.span.start as usize;
                Some((s, Expr { kind: p.kind.clone(), span: mc_syntax::Span::new(s + 1, p.span.end as usize) }))
            }
            _ => None,
        }
    }

    /// Output: `b^-n` for an integer n as the denominator it makes, `b` or `b^n` (NB6.12).
    fn negative_power(&self, f: &Expr) -> Option<Expr> {
        if !self.output {
            return None;
        }
        let ExprKind::Pow(b, x) = &f.kind else { return None };
        let ExprKind::Neg(n) = &x.kind else { return None };
        let digits = self.src_of(n);
        if !matches!(n.kind, ExprKind::Num(_)) || digits.is_empty() || !digits.bytes().all(|c| c.is_ascii_digit()) {
            return None;
        }
        Some(if digits == "1" { (**b).clone() } else { Expr { kind: ExprKind::Pow(b.clone(), n.clone()), span: f.span } })
    }

    /// An output term without its sign: whether it is negative, and the term to draw instead when the sign was
    /// taken off (`-x` is x; `-1*cos(x)` is cos(x); `-3*x` is 3*x; `(-2/3)*x` is (2/3)*x).
    fn unsigned_term(&self, t: &Expr) -> (bool, Option<Expr>) {
        match &t.kind {
            ExprKind::Neg(x) if !self.parenthesised(t) => (true, Some((**x).clone())),
            ExprKind::Mul(fs) if !self.parenthesised(t) => {
                let Some((MulOp::Times, first)) = fs.first() else { return (false, None) };
                let mut rest = fs.clone();
                match &first.kind {
                    ExprKind::Neg(n) if matches!(n.kind, ExprKind::Num(_)) => {
                        if self.src_of(n) == "1" && fs.len() > 1 && fs[1].0 == MulOp::Times {
                            rest.remove(0);
                        } else {
                            rest[0].1 = (**n).clone();
                        }
                    }
                    // a parenthesised negative rational coefficient: the rational without its minus
                    ExprKind::Mul(r) if self.rational(first).is_some() => match r.first() {
                        Some((MulOp::Times, Expr { kind: ExprKind::Neg(n), .. })) => {
                            let mut abs = r.clone();
                            abs[0].1 = (**n).clone();
                            rest[0].1 = Expr { kind: ExprKind::Mul(abs), span: first.span };
                        }
                        _ => return (false, None),
                    },
                    _ => return (false, None),
                }
                let body = if rest.len() == 1 { rest.remove(0).1 } else { Expr { kind: ExprKind::Mul(rest), span: t.span } };
                (true, Some(body))
            }
            _ => (false, None),
        }
    }

    /// A sum in output: subtractions instead of added negative terms (`a + -2*b` is a − 2b), and for a polynomial
    /// in its main variable (a power of degree 2 or more, or a negative power, in some term) the terms by descending
    /// degree; a linear sum keeps the engine's order (`3 - x`).
    fn sum(&self, xs: &[(bool, Expr)], out: &mut Out, depth: usize) {
        let mut terms: Vec<(bool, Cow<'_, Expr>)> = xs
            .iter()
            .map(|(minus, t)| {
                let (neg, body) = self.unsigned_term(t);
                (*minus ^ neg, body.map_or(Cow::Borrowed(t), Cow::Owned))
            })
            .collect();
        if let Some(v) = main_variable(self.src, terms.iter().map(|(_, t)| &**t)).map(str::to_string) {
            let degree = |t: &Expr| factors_of(t).into_iter().filter_map(|f| power_of(self.src, f)).filter(|(n, _)| *n == v).map(|(_, k)| k).sum::<i64>();
            if terms.iter().any(|(_, t)| degree(t) >= 2 || degree(t) < 0) {
                terms.sort_by_key(|(_, t)| std::cmp::Reverse(degree(t)));
            }
        }
        let mut prev_end: Option<usize> = None;
        for (k, (neg, t)) in terms.iter().enumerate() {
            let at = t.span.start as usize;
            // the operator stands for the text between the terms when they are neighbours in it
            let gap = match prev_end {
                Some(p) if p <= at && self.src[p..at].chars().all(|c| matches!(c, ' ' | '+' | '-')) => p..at,
                _ => at..at,
            };
            if k == 0 {
                if *neg {
                    out.push(Item::Char('−'), at..at);
                }
            } else {
                out.push(Item::Char(' '), gap.clone());
                out.push(Item::Char(if *neg { '−' } else { '+' }), gap.clone());
                out.push(Item::Char(' '), gap);
            }
            self.expr(t, out, depth);
            prev_end = Some(t.span.end as usize);
        }
    }

    /// A list of equal-length lists drawn as a matrix (output only, [`OutputOptions::matrices`]).
    fn matrix(&self, e: &Expr, depth: usize) -> Option<(Node, Vec<SourceMap>)> {
        if !self.output || !self.opts.matrices || self.parenthesised(e) {
            return None;
        }
        let ExprKind::List(rows) = &e.kind else { return None };
        if rows.len() < 2 || rows.len() > MATRIX_MAX {
            return None;
        }
        let mut width = None;
        for r in rows {
            let ExprKind::List(cells) = &r.kind else { return None };
            if cells.is_empty() || cells.len() > MATRIX_MAX || width.is_some_and(|w| w != cells.len()) {
                return None;
            }
            if cells.iter().any(|c| matches!(c.kind, ExprKind::List(_))) {
                return None;
            }
            width = Some(cells.len());
        }
        let mut grid = Vec::with_capacity(rows.len());
        let mut maps = Vec::new();
        for r in rows {
            let ExprKind::List(cells) = &r.kind else { return None };
            let mut row = Vec::with_capacity(cells.len());
            for c in cells {
                let (cell, m) = self.slot(c, depth);
                row.push(cell);
                maps.push(m);
            }
            grid.push(row);
        }
        Some((Node::Grid { rows: grid }, maps))
    }

    /// The box an expression is drawn as, if it has one, with the maps of its slots in slot order.
    fn as_box(&self, e: &Expr, depth: usize) -> Option<(Node, Vec<SourceMap>)> {
        let whole = span_of(e);
        match &e.kind {
            ExprKind::Mul(fs) => {
                // a division, or (output) a product with a rational coefficient: `(2/3)*sqrt(2)` is drawn
                // as 2√2 over 3 (decision NB6.2), numerator factors over denominator factors
                // and (output) negative integer powers in the denominator: `a*x^-2` is a over x² (NB6.12)
                let coefficient = self.output && fs.len() >= 2 && fs.iter().any(|(op, f)| *op == MulOp::Times && self.rational(f).is_some());
                let reciprocal = fs.iter().any(|(op, f)| *op == MulOp::Times && self.negative_power(f).is_some());
                if !coefficient && !reciprocal && !fs.iter().any(|(op, _)| *op == MulOp::Div) {
                    return None;
                }
                let mut num: Vec<Expr> = vec![];
                let mut den: Vec<Expr> = vec![];
                // a negative coefficient: its sign goes before the fraction (see `expr`)
                let negative = self.negative_coefficient(e);
                for (op, f) in fs {
                    match (op, self.rational(f).filter(|_| self.output), self.negative_power(f)) {
                        (MulOp::Times, Some((p, q)), _) => {
                            let p = negative.as_ref().map_or(p, |(_, abs)| abs);
                            if self.src_of(p) != "1" {
                                num.push(p.clone());
                            }
                            den.push(q.clone());
                        }
                        (MulOp::Times, None, Some(d)) => den.push(d),
                        (MulOp::Times, None, None) => num.push(f.clone()),
                        (MulOp::Div, ..) => den.push(f.clone()),
                    }
                }
                let num: Vec<&Expr> = num.iter().collect();
                let den: Vec<&Expr> = den.iter().collect();
                let (num, nm) = if num.is_empty() {
                    (Row::text("1"), SourceMap::synthetic(1, whole.clone()))
                } else {
                    self.factors_row(&num, depth)
                };
                let (den, dm) = self.factors_row(&den, depth);
                Some((Node::Fraction { num, den }, vec![nm, dm]))
            }
            ExprKind::Pow(..) if self.negative_power(e).is_some() => {
                let d = self.negative_power(e)?;
                let (den, dm) = self.factors_row(&[&d], depth);
                Some((Node::Fraction { num: Row::text("1"), den }, vec![SourceMap::synthetic(1, whole.clone()), dm]))
            }
            ExprKind::Pow(b, x) => {
                // inputs keep their powers (a radical would read as a different term); outputs show radicals
                let root = if self.output { self.reciprocal(x) } else { None };
                Some(match root {
                    Some(n) if self.src_of(n) == "2" => {
                        let (radicand, rm) = self.slot(b, depth);
                        (Node::Radical { radicand, index: None }, vec![rm])
                    }
                    Some(n) => {
                        let (radicand, rm) = self.slot(b, depth);
                        let (index, im) = self.slot(n, depth);
                        (Node::Radical { radicand, index: Some(index) }, vec![im, rm])
                    }
                    None => {
                        let (base, bm) = self.script_base(b, depth);
                        let (exp, em) = self.slot(x, depth);
                        (Node::Superscript { base, exp }, vec![bm, em])
                    }
                })
            }
            ExprKind::Call(f, args) if args.iter().all(|a| a.name.is_none()) => {
                let ExprKind::Name(name) = &f.kind else { return None };
                let a: Vec<&Expr> = args.iter().map(|a| &a.value).collect();
                let s = |k: usize| self.slot(a[k], depth);
                Some(match (name.as_str(), a.len()) {
                    ("sqrt", 1) => {
                        let (r, m) = s(0);
                        (Node::Radical { radicand: r, index: None }, vec![m])
                    }
                    // the real n-th root (round 19, U19.3); `root(p, k)` is a root of a polynomial, not a radical
                    ("surd", 2) => {
                        let ((r, rm), (i, im)) = (s(0), s(1));
                        (Node::Radical { radicand: r, index: Some(i) }, vec![im, rm])
                    }
                    ("subscript", 2) => {
                        let ((b, bm), (i, im)) = (self.script_base(a[0], depth), s(1));
                        (Node::Subscript { base: b, index: i }, vec![bm, im])
                    }
                    ("overscript", 2) => {
                        let ((b, bm), (o, om)) = (s(0), s(1));
                        (Node::Overscript { base: b, over: o }, vec![bm, om])
                    }
                    ("underscript", 2) => {
                        let ((b, bm), (u, um)) = (s(0), s(1));
                        (Node::Underscript { base: b, under: u }, vec![bm, um])
                    }
                    ("underoverscript", 3) => {
                        let ((b, bm), (u, um), (o, om)) = (s(0), s(1), s(2));
                        (Node::Underoverscript { base: b, under: u, over: o }, vec![bm, um, om])
                    }
                    ("image", 1) => match &a[0].kind {
                        ExprKind::Str(p) => (Node::Graphic { path: p.clone() }, vec![]),
                        _ => return None,
                    },
                    ("limit", 2) => match &a[1].kind {
                        ExprKind::Binary(BinOp::Rule, v, p) => {
                            let ((body, bm), (var, vm), (lower, lm)) = (s(0), self.slot(v, depth), self.slot(p, depth));
                            let n = Node::BigOp { op: BigOp::Limit, body, var, lower: Some(lower), upper: None };
                            (n, vec![vm, lm, bm])
                        }
                        _ => return None,
                    },
                    ("d", 2) => match &a[1].kind {
                        ExprKind::Name(_) => {
                            let ((body, bm), (var, vm)) = (s(0), s(1));
                            (Node::BigOp { op: BigOp::Derivative, body, var, lower: None, upper: None }, vec![vm, bm])
                        }
                        _ => return None,
                    },
                    ("sum" | "product" | "integrate", 2) => {
                        let op = match name.as_str() {
                            "sum" => BigOp::Sum,
                            "product" => BigOp::Product,
                            _ => BigOp::Integral,
                        };
                        let integral = op == BigOp::Integral;
                        match &a[1].kind {
                            ExprKind::List(l) if l.len() == 3 => {
                                let (body, bm) = s(0);
                                let (var, vm) = self.slot(&l[0], depth);
                                let (lo, lm) = self.slot(&l[1], depth);
                                let (hi, hm) = self.slot(&l[2], depth);
                                let maps = if integral { vec![lm, hm, bm, vm] } else { vec![vm, lm, hm, bm] };
                                (Node::BigOp { op, body, var, lower: Some(lo), upper: Some(hi) }, maps)
                            }
                            ExprKind::Name(_) => {
                                let ((body, bm), (var, vm)) = (s(0), s(1));
                                let maps = if integral { vec![bm, vm] } else { vec![vm, bm] };
                                (Node::BigOp { op, body, var, lower: None, upper: None }, maps)
                            }
                            _ => return None,
                        }
                    }
                    _ => return None,
                })
            }
            _ => None,
        }
    }

    /// A script's base keeps its parentheses when it needs them to read right (`(a + b)²`), drops them around a
    /// name or call.
    fn script_base(&self, b: &Expr, depth: usize) -> (Row, SourceMap) {
        let keep = self.parenthesised(b)
            && (!matches!(b.kind, ExprKind::Name(_) | ExprKind::Num(_) | ExprKind::Call(..) | ExprKind::Index(..) | ExprKind::List(_))
                || confusable_call(b));
        if keep {
            let (lo, hi) = (b.span.start as usize, b.span.end as usize);
            let mut out = Out::default();
            out.push(Item::Char('('), lo..lo + 1);
            out.append(self.slot(b, depth));
            out.push(Item::Char(')'), hi - 1..hi);
            out.done()
        } else {
            self.slot(b, depth)
        }
    }

    fn src_of(&self, e: &Expr) -> &str {
        &self.src[e.span.start as usize..e.span.end as usize]
    }

    /// For an exponent `1/n` (possibly parenthesised): `n`.
    fn reciprocal<'e>(&self, x: &'e Expr) -> Option<&'e Expr> {
        match &x.kind {
            ExprKind::Mul(fs) => match fs.as_slice() {
                [(MulOp::Times, one), (MulOp::Div, n)] if self.src_of(one) == "1" => Some(n),
                _ => None,
            },
            _ => None,
        }
    }

    /// `derivative(n)(f)` or `derivative(n)(f)(x)`: the function and the order.
    fn derivative_of<'e>(&self, e: &'e Expr) -> Option<(&'e Expr, u64)> {
        let ExprKind::Call(f, args) = &e.kind else { return None };
        let (inner, fargs) = match &f.kind {
            // derivative(n)(f)(x): f is derivative(n)(f)
            ExprKind::Call(g, gargs) if matches!(&g.kind, ExprKind::Call(..)) => (&**g, gargs),
            // derivative(n)(f): f is derivative(n)
            ExprKind::Call(g, _) if matches!(&g.kind, ExprKind::Name(_)) => (&**f, args),
            _ => return None,
        };
        let ExprKind::Call(d, n) = &inner.kind else { return None };
        let (ExprKind::Name(dn), [n], [fun]) = (&d.kind, n.as_slice(), fargs.as_slice()) else { return None };
        if dn != "derivative" || n.name.is_some() || fun.name.is_some() || !matches!(n.value.kind, ExprKind::Num(_)) {
            return None;
        }
        Some((&fun.value, self.src_of(&n.value).parse().ok()?))
    }
}

/// Input text for a part of typeset output whose printed text is not one stretch (the numerator of `a*b/c*d`):
/// its linear text with the output spellings turned back (`π` to `pi`, `→` to `->`, the thin space of a product to
/// `*`, ...). Strings lost their quotes in output and stay without them.
pub fn output_input_text(row: &Row) -> String {
    let lin = mc_boxes::linear_text(row);
    let mut out = String::with_capacity(lin.len());
    for c in lin.chars() {
        match c {
            '\u{2009}' | '×' => out.push('*'),
            'π' => out.push_str("pi"),
            '∞' => out.push_str("inf"),
            'ⅇ' => out.push('e'),
            'ⅈ' => out.push_str("im"),
            '′' => out.push('\''),
            '″' => out.push_str("''"),
            '‴' => out.push_str("'''"),
            _ => match OUTPUT_OPS.iter().find(|(_, to)| to.chars().eq(std::iter::once(c))) {
                Some((from, _)) => out.push_str(from),
                None => out.push(c),
            },
        }
    }
    out
}

/// Input boxes for a part of typeset output (copying it with its boxes, round 13): the items with the output
/// spellings turned back character by character as [`output_input_text`] does (the thin space of a product to `*`),
/// inside the boxes too, and only the notebook's three boxes kept ([`mc_boxes::restrict`]). Its canonical text
/// ([`mc_boxes::to_text`]) evaluates as the selected part reads.
pub fn output_input_boxes(row: &Row) -> Row {
    fn back(row: &Row) -> Row {
        let mut out = Vec::with_capacity(row.0.len());
        for it in &row.0 {
            match it {
                Item::Char(c) => out.extend(Row::text(&output_input_text(&Row(vec![Item::Char(*c)]))).0),
                Item::Box(b) => {
                    let mut n = (**b).clone();
                    for s in n.slots_mut() {
                        *s = back(s);
                    }
                    out.push(Item::Box(Box::new(n)));
                }
                Item::Placeholder => out.push(Item::Placeholder),
            }
        }
        Row(out)
    }
    mc_boxes::restrict(&back(row))
}

/// The order class of a factor in an output product: numbers, then names and their powers, then the rest.
fn factor_class(f: &Expr) -> u8 {
    match &f.kind {
        ExprKind::Num(_) => 0,
        ExprKind::Neg(x) if matches!(x.kind, ExprKind::Num(_)) => 0,
        ExprKind::Name(_) => 1,
        ExprKind::Pow(b, _) if matches!(b.kind, ExprKind::Name(_)) => 1,
        _ => 2,
    }
}

/// The factors of a term (itself when it is not a product).
fn factors_of(t: &Expr) -> Vec<&Expr> {
    match &t.kind {
        ExprKind::Mul(fs) => fs.iter().filter(|(op, _)| *op == MulOp::Times).map(|(_, f)| f).collect(),
        _ => vec![t],
    }
}

/// A factor that is a name or an integer power of one: the name and the exponent.
fn power_of<'a>(src: &str, f: &'a Expr) -> Option<(&'a str, i64)> {
    match &f.kind {
        ExprKind::Name(n) => Some((n.as_str(), 1)),
        ExprKind::Pow(b, x) => {
            let ExprKind::Name(n) = &b.kind else { return None };
            let k = match &x.kind {
                ExprKind::Num(_) => num_value(src, x)?,
                ExprKind::Neg(y) => -num_value(src, y)?,
                _ => return None,
            };
            Some((n.as_str(), k))
        }
        _ => None,
    }
}

/// The value of an integer literal (from its digits in the source text).
fn num_value(src: &str, e: &Expr) -> Option<i64> {
    match &e.kind {
        ExprKind::Num(n) if !n.imag => src.get(e.span.start as usize..e.span.end as usize)?.parse().ok(),
        _ => None,
    }
}

/// The main variable of a sum: the name that is a factor (or the base of a power) in the most terms; constants do
/// not count.
fn main_variable<'a>(src: &str, terms: impl Iterator<Item = &'a Expr>) -> Option<&'a str> {
    let mut count: Vec<(&str, usize)> = vec![];
    for t in terms {
        let mut seen: Vec<&str> = vec![];
        for (n, _) in factors_of(t).into_iter().filter_map(|f| power_of(src, f)) {
            if matches!(n, "pi" | "e" | "im" | "inf") || seen.contains(&n) {
                continue;
            }
            seen.push(n);
            match count.iter_mut().find(|(m, _)| *m == n) {
                Some(c) => c.1 += 1,
                None => count.push((n, 1)),
            }
        }
    }
    let best = count.iter().map(|c| c.1).max()?;
    count.into_iter().find(|c| c.1 == best).map(|c| c.0)
}

/// A call that reads like a product: a plain name called on arguments in which the name is itself an operand
/// (`x(x - a)`). As a power's base it keeps its brackets, `(x(x − a))²` (round 20; the printer writes them too).
fn confusable_call(e: &Expr) -> bool {
    let ExprKind::Call(f, args) = &e.kind else { return false };
    let ExprKind::Name(n) = &f.kind else { return false };
    let mut stack: Vec<&Expr> = args.iter().map(|a| &a.value).collect();
    let mut seen = 0;
    while let Some(x) = stack.pop() {
        seen += 1;
        if seen > 256 {
            return false;
        }
        if matches!(&x.kind, ExprKind::Name(m) if m == n) {
            return true;
        }
        stack.extend(children(x).into_iter().filter_map(|c| match c {
            Child::E(y) => Some(y),
            Child::S(_) => None,
        }));
    }
    false
}

/// The expression children of a node, in source order.
fn children(e: &Expr) -> Vec<Child<'_>> {
    use Child::{E, S};
    let mut v: Vec<Child> = match &e.kind {
        ExprKind::Typed(x, _) | ExprKind::Named(_, x) | ExprKind::Neg(x) | ExprKind::Not(x) | ExprKind::Prime(x, _) => vec![E(x)],
        ExprKind::SlotFn(x) | ExprKind::Lambda(_, x) => vec![E(x)],
        ExprKind::Add(xs) => xs.iter().map(|(_, x)| E(x)).collect(),
        ExprKind::Mul(xs) => xs.iter().map(|(_, x)| E(x)).collect(),
        ExprKind::Pow(a, b) | ExprKind::Binary(_, a, b) | ExprKind::Span(a, b) | ExprKind::While(a, b) => vec![E(a), E(b)],
        ExprKind::Nary(_, xs) | ExprKind::List(xs) => xs.iter().map(E).collect(),
        ExprKind::Call(f, args) => std::iter::once(E(f)).chain(args.iter().map(|a| E(&a.value))).collect(),
        ExprKind::Index(x, idx) => std::iter::once(E(x)).chain(idx.iter().map(E)).collect(),
        ExprKind::Block(stmts) => stmts.iter().map(S).collect(),
        ExprKind::If(c, t, f) => [Some(&**c), Some(&**t), f.as_deref()].into_iter().flatten().map(E).collect(),
        ExprKind::For(_, it, body) => vec![E(it), E(body)],
        ExprKind::Match(x, arms) => std::iter::once(E(x)).chain(arms.iter().flat_map(|a| [E(&a.pattern), E(&a.body)])).collect(),
        _ => vec![],
    };
    v.sort_by_key(|c| c.span().0);
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use mc_boxes::notation;

    fn out(t: &str) -> String {
        notation(&typeset_output(t).row)
    }

    fn inp(t: &str) -> String {
        notation(&typeset_input(t).unwrap())
    }

    #[test]
    fn output_rows_of_the_table() {
        // a/b, a*b/c: numerator factors over denominator factors
        assert_eq!(out("a/b"), "(a/b)");
        assert_eq!(out("a*b/c"), "(a\u{2009}b/c)");
        assert_eq!(out("a/b*c"), "(a\u{2009}c/b)");
        assert_eq!(out("(a + b)/(2*c)"), "(a + b/2\u{2009}c)");
        // rational literals
        assert_eq!(out("1/2"), "(1/2)");
        assert_eq!(out("x/2 + 1"), "(x/2) + 1");
        // powers and radicals
        assert_eq!(out("x^2"), "(x^2)");
        assert_eq!(out("(a + b)^n"), "((a + b)^n)");
        assert_eq!(out("x^(n + 1)"), "(x^n + 1)");
        assert_eq!(out("x^(1/2)"), "√(x)");
        assert_eq!(out("sqrt(x + 1)"), "√(x + 1)");
        assert_eq!(out("x^(1/n)"), "√[n](x)");
        assert_eq!(out("surd(x, 3)"), "√[3](x)");
        assert_eq!(out("root(x, 3)"), "root(x, 3)", "a root of a polynomial is not a radical");
        assert_eq!(out("x^-1"), "(1/x)");
        // a call that reads like a product keeps the brackets of a power's base; a product with a bracketed
        // factor after a name shows its dot (round 20)
        assert_eq!(out("(x(x - a))^2"), "((x(x − a))^2)");
        assert_eq!(out("(x(x - a))^(1/2)"), "√(x(x − a))");
        assert_eq!(out("x*(x - a)"), "x\u{22C5}(x − a)");
        assert_eq!(out("(x*(x - a))^(1/2)"), "√(x\u{22C5}(x − a))");
        assert_eq!(out("f(x)^2"), "(f(x)^2)");
        assert_eq!(out("a - b"), "a − b");
        // products: thin space, × between numbers
        assert_eq!(out("2*x"), "2\u{2009}x");
        assert_eq!(out("a*b"), "a\u{2009}b");
        assert_eq!(out("2*3"), "2×3");
        assert_eq!(out("2*x*y^2"), "2\u{2009}x\u{2009}(y^2)");
        // scripts
        assert_eq!(out("subscript(x, i)"), "(x_i)");
        assert_eq!(out("subscript(x, i)^2"), "((x_i)^2)");
        assert_eq!(out("overscript(x, \"^\")"), "(x over ^)");
        assert_eq!(out("underscript(lim, n)"), "(lim under n)");
        assert_eq!(out("underoverscript(x, a, b)"), "(x under a over b)");
        // big operators
        assert_eq!(out("sum(k^2, [k, 1, n])"), "∑[k=1..n]((k^2))");
        assert_eq!(out("product(f(k), [k, 1, n])"), "∏[k=1..n](f(k))");
        assert_eq!(out("integrate(f(x), x)"), "∫[x](f(x))");
        assert_eq!(out("integrate(sin(x), [x, 0, pi])"), "∫[x=0..π](sin(x))");
        assert_eq!(out("limit(sin(x)/x, x -> 0)"), "lim[x→0]((sin(x)/x))");
        assert_eq!(out("d(f(x), x)"), "∂[x](f(x))");
        // derivatives
        assert_eq!(out("derivative(1)(f)(x)"), "f′(x)");
        assert_eq!(out("derivative(2)(f)(x)"), "f″(x)");
        assert_eq!(out("derivative(7)(f)(x)"), "(f^(7))(x)");
        assert_eq!(out("derivative(1)(f)"), "f′");
        assert_eq!(out("f''(x)"), "f″(x)");
        // constants and operators
        assert_eq!(out("pi + inf + e + im"), "π + ∞ + ⅇ + ⅈ");
        assert_eq!(out("2 + 3im"), "2 + 3ⅈ");
        assert_eq!(out("x -> 1"), "x → 1");
        assert_eq!(out("[a <= b, a >= b, a != b, a == b]"), "[a ≤ b, a ≥ b, a ≠ b, a ⩵ b]");
        // strings without quotes; a list of equal-length lists is a matrix (NB6.3)
        assert_eq!(out("\"hello\""), "hello");
        assert_eq!(out("[[1, 2], [3, 4]]"), "[grid[[1,2],[3,4]]]");
        assert_eq!(out("image(\"plot-1.png\")"), "image(plot-1.png)");
    }

    #[test]
    fn rational_coefficients_are_one_fraction() {
        // the kernel prints a rational coefficient in parentheses: shown as one fraction (NB6.2)
        assert_eq!(out("(2/3)*2^(1/2)"), "(2\u{2009}√(2)/3)");
        assert_eq!(out("(1/2)*x + y^(1/2)"), "(x/2) + √(y)");
        assert_eq!(out("(-2/3)*x"), "−(2\u{2009}x/3)");
        assert_eq!(out("(-1/3)*x"), "−(x/3)");
        assert_eq!(out("(3/4)*x^2/y"), "(3\u{2009}(x^2)/4\u{2009}y)");
        assert_eq!(out("(1/2)*x/y"), "(x/2\u{2009}y)");
        // a rational alone, and products without one, are as before
        assert_eq!(out("1/2"), "(1/2)");
        assert_eq!(out("2*x"), "2\u{2009}x");
        // input conversion keeps the user's text
        assert_eq!(inp("(2/3)*x"), "((2/3))*x");
    }

    #[test]
    fn sums_and_products_read_as_written_mathematics() {
        // negative terms are subtractions, no visible coefficient 1, numbers then powers then functions, and a
        // polynomial in its main variable by descending degree (NB6.12)
        assert_eq!(
            out("2*cos(x) + -1*cos(x)*x^2 + 2*x*sin(x)"),
            "−(x^2)\u{2009}cos(x) + 2\u{2009}x\u{2009}sin(x) + 2\u{2009}cos(x)"
        );
        assert_eq!(out("-x^-2 + 2*x"), "2\u{2009}x − (1/(x^2))");
        assert_eq!(out("a + -2*b"), "a − 2\u{2009}b");
        assert_eq!(out("-1 + -3*x^2 + 3*x + x^3"), "(x^3) − 3\u{2009}(x^2) + 3\u{2009}x − 1");
        assert_eq!(out("-1*cos(x)*x^2"), "−(x^2)\u{2009}cos(x)");
        assert_eq!(out("-x"), "−x");
        // linear sums keep the engine's order
        assert_eq!(out("3 - x"), "3 − x");
        assert_eq!(out("1 + x"), "1 + x");
        // negative integer powers are fractions
        assert_eq!(out("x^-2"), "(1/(x^2))");
        assert_eq!(out("-x^-1"), "−(1/x)");
        assert_eq!(out("a*x^-1"), "(a/x)");
        assert_eq!(out("3*y*x^-2"), "(3\u{2009}y/(x^2))");
        // fractional powers are not touched
        assert_eq!(out("x^(-3/2)"), "(x^−(3/2))");
        // a negative rational coefficient in a sum is a subtraction too
        assert_eq!(out("x^2 + (-2/3)*x"), "(x^2) − (2\u{2009}x/3)");
        // the part maps still lead back to the printed text
        let t = typeset_output("2*cos(x) + -1*cos(x)*x^2");
        assert_eq!(t.row.len(), t.map.spans.len());
    }

    #[test]
    fn matrices_and_their_limits() {
        let grid = |t: &str| notation(&typeset_output(t).row);
        assert_eq!(grid("[[a, b, c], [1, 2, 3]]"), "[grid[[a,b,c],[1,2,3]]]");
        // not matrices: ragged rows, one row, lists of scalars; an array of three levels is a list of matrices
        assert_eq!(grid("[[1, 2], [3]]"), "[[1, 2], [3]]");
        assert_eq!(grid("[[1, 2]]"), "[[1, 2]]");
        assert_eq!(grid("[[[1], [2]], [[3], [4]]]"), "[[grid[[1],[2]]], [grid[[3],[4]]]]");
        assert_eq!(grid("[1, 2]"), "[1, 2]");
        // too many rows: data, linear
        let tall = format!("[{}]", vec!["[1, 2]"; MATRIX_MAX + 1].join(", "));
        assert!(!grid(&tall).contains("grid"));
        // the setting turns them off
        let off = typeset_output_with("[[1, 2], [3, 4]]", OutputOptions { matrices: false });
        assert_eq!(notation(&off.row), "[[1, 2], [3, 4]]");
    }

    #[test]
    fn source_maps_lead_back_to_the_printed_text() {
        let text = "(2/3)*2^(1/2) + x^2/(1 + y)";
        let t = typeset_output(text);
        // every item has a span, every box its slot maps
        let mut stack = vec![(&t.row, &t.map)];
        while let Some((row, map)) = stack.pop() {
            assert_eq!(row.len(), map.spans.len());
            for (i, it) in row.0.iter().enumerate() {
                if let Item::Box(b) = it {
                    let slots = &map.boxes[&i];
                    assert_eq!(slots.len(), b.slots().len());
                    stack.extend(b.slots().into_iter().zip(slots));
                }
            }
        }
        // the second fraction: its denominator `1 + y` is found from its text, and gives it back
        let at = text.find("1 + y").unwrap();
        let (path, range) = t.map.find(at..at + 5).unwrap();
        assert_eq!(path.len(), 1);
        let span = t.map.span(&path, range.clone()).unwrap();
        assert_eq!(&text[span], "1 + y");
        assert!(t.map.contiguous(&path, range, text));
        // a part of a token selects the item holding it
        let (path, range) = t.map.find(at + 4..at + 5).unwrap();
        assert_eq!(&text[t.map.span(&path, range).unwrap()], "y");
        // the numerator of the first fraction is 2 and sqrt(2): not one stretch of the text
        let c = typeset_output("(2/3)*2^(1/2)");
        let num = vec![Step { item: 0, slot: 0 }];
        let n = c.map.row(&num).unwrap().spans.len();
        assert!(!c.map.contiguous(&num, 0..n, "(2/3)*2^(1/2)"));
        // carets: the byte before `y` is inside the denominator, before `y`
        let (p, k) = t.map.caret_at(at + 4);
        assert_eq!(p.len(), 1);
        assert_eq!(t.map.byte_at(&p, k), Some(at + 4));
        // the whole text at the ends
        assert_eq!(t.map.caret_at(0), (vec![], 0));
        assert_eq!(t.map.byte_at(&[], t.row.len()), Some(text.len()));
    }

    #[test]
    fn failures_and_unparsable_output_stay_linear() {
        let f = typeset_output("failure(\"div\", 1/0)");
        assert!(f.message);
        assert_eq!(notation(&f.row), "failure(\"div\", 1/0)");
        let u = typeset_output("[1, 2, «998», 1000]");
        assert!(!u.message);
        assert_eq!(notation(&u.row), "[1, 2, «998», 1000]");
    }

    #[test]
    fn input_keeps_text_and_makes_boxes() {
        assert_eq!(inp("x = (a + b)/(2c)"), "x = (a + b/2c)");
        assert_eq!(inp("f(x) := x^2 + sqrt(x) # square"), "f(x) := (x^2) + √(x) # square");
        assert_eq!(inp("2*x*y/z"), "(2*x*y/z)");
        assert_eq!(inp("pi -> \"s\""), "pi -> \"s\"");
        assert_eq!(inp("sum(1/k^2, [k, 1, inf])"), "∑[k=1..inf]((1/(k^2)))");
        assert_eq!(inp("x^(1/2) + sqrt(y)"), "(x^(1/2)) + √(y)");
        assert!(typeset_input("f(").is_err());
    }

    #[test]
    fn output_parts_copy_as_input_boxes() {
        let canon = |t: &str| mc_boxes::to_text(&output_input_boxes(&crate::math_text(t).row));
        // a thin space of a product goes back to `*`; a power `^(1/2)` is drawn and copied as a root
        assert_eq!(canon("3*x^2"), "3*((x)^(2))");
        assert_eq!(canon("2*2^(1/2)"), "2*((sqrt(2)))");
        assert_eq!(canon("(a + b)/c"), "((a + b)/(c))");
        assert_eq!(canon("y^-1"), "((1)/(y))");
        assert_eq!(canon("[1, 2]"), "[1, 2]");
    }
}
