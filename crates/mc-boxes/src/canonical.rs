//! Canonical box text (design 08, section 4.4): ordinary MathCAS source in which each box is a
//! double-parenthesised expression of a fixed shape, and its reading back; plus the linear input text (4.6), the
//! same source with the box markers reduced to the parentheses the meaning needs.
//!
//! | Box | Canonical text |
//! |---|---|
//! | fraction | `((N)/(D))` |
//! | superscript | `((B)^(E))` |
//! | subscript | `((subscript(B, S)))` |
//! | radical | `((sqrt(X)))`, `((root(X, n)))` |
//! | over/under | `((overscript(B, O)))`, `((underscript(B, U)))`, `((underoverscript(B, U, O)))` |
//! | grid | `(([[a, b], [c, d]]))` |
//! | big operators | `((sum(f, [k, a, b])))`, `((sum(f, k)))`, `((product(...)))`, `((integrate(f, x)))`, `((integrate(f, [x, a, b])))`, `((limit(f, x -> a)))`, `((d(f, x)))` |
//! | graphic | `((image("path")))` |
//! | placeholder | `□` |
//!
//! Slots of the call shapes are separated by `, `. A slot whose text would not read back as one slot (it has a
//! top-level `,`, or `->` in a limit's variable, or is a bracket list where a list means limits) is written in one
//! more pair of parentheses, and that pair is removed on reading; the rule is applied recursively so that a slot
//! which itself looks like such a wrapped slot is wrapped again (see [`Wrap`]). With it, `from_text(to_text(b)) == b`
//! for every well-formed tree whose rows have balanced brackets outside strings and comments, and
//! `to_text(from_text(t)) == t` for every text.
use crate::scan::Scan;
use std::cell::RefCell;
use std::collections::HashMap;
use crate::tree::{BigOp, Item, Node, Row, RowPath, Step, MAX_NESTING};

/// The canonical text of a row.
pub fn to_text(row: &Row) -> String {
    let mut s = String::new();
    write_row(row, &mut s);
    s
}

/// Reads canonical text: every double-parenthesised box shape becomes its box (recursively), `□` outside
/// literals becomes a placeholder, and everything else stays characters. Never fails.
pub fn from_text(text: &str) -> Row {
    let scan = Scan::new(text);
    Reader { s: &scan, memo: RefCell::new(HashMap::new()) }.row(0, scan.chars.len(), 0)
}

/// What makes a slot's text need one more pair of parentheses in a call shape.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Wrap {
    /// a top-level `,`
    Comma,
    /// a top-level `,` or `->` (a limit's variable)
    CommaArrow,
    /// a top-level `,`, or a bracket list (the variable of a sum, product or integral without limits)
    CommaList,
}

impl Wrap {
    /// Whether the text `lo..hi` of `s` must be wrapped: its own reason, or it is one parenthesised group whose
    /// inside would have to be wrapped (so that unwrapping on reading is unambiguous).
    pub(crate) fn needs(self, s: &Scan, lo: usize, hi: usize) -> bool {
        let own = match self {
            Wrap::Comma => s.has_top(lo, hi, ","),
            Wrap::CommaArrow => s.has_top(lo, hi, ",") || s.has_top(lo, hi, "->"),
            Wrap::CommaList => s.has_top(lo, hi, ",") || s.is_group(lo, hi, '['),
        };
        own || (s.is_group(lo, hi, '(') && self.needs(s, lo + 1, hi - 1))
    }

    /// The text to write for a slot whose text is `t`.
    pub(crate) fn apply(self, t: String) -> String {
        let s = Scan::new(&t);
        if self.needs(&s, 0, s.chars.len()) {
            format!("({t})")
        } else {
            t
        }
    }
}

fn write_row(row: &Row, out: &mut String) {
    for it in &row.0 {
        match it {
            Item::Char(c) => out.push(*c),
            Item::Placeholder => out.push('□'),
            Item::Box(b) => write_node(b, out),
        }
    }
}

fn slot(r: &Row, w: Wrap) -> String {
    w.apply(to_text(r))
}

fn write_node(n: &Node, out: &mut String) {
    let c = Wrap::Comma;
    let body = match n {
        Node::Fraction { num, den } => format!("({})/({})", to_text(num), to_text(den)),
        Node::Superscript { base, exp } => format!("({})^({})", to_text(base), to_text(exp)),
        Node::Subscript { base, index } => format!("(subscript({}, {}))", slot(base, c), slot(index, c)),
        Node::Radical { radicand, index: None } => format!("(sqrt({}))", slot(radicand, c)),
        Node::Radical { radicand, index: Some(i) } => format!("(root({}, {}))", slot(radicand, c), slot(i, c)),
        Node::Overscript { base, over } => format!("(overscript({}, {}))", slot(base, c), slot(over, c)),
        Node::Underscript { base, under } => format!("(underscript({}, {}))", slot(base, c), slot(under, c)),
        Node::Underoverscript { base, under, over } => {
            format!("(underoverscript({}, {}, {}))", slot(base, c), slot(under, c), slot(over, c))
        }
        Node::Grid { rows } => {
            let rows: Vec<String> =
                rows.iter().map(|r| format!("[{}]", r.iter().map(|x| slot(x, c)).collect::<Vec<_>>().join(", "))).collect();
            format!("([{}])", rows.join(", "))
        }
        Node::BigOp { op, body, var, lower, upper } => {
            let f = op.function();
            let b = slot(body, c);
            match (op, lower, upper) {
                (BigOp::Limit, Some(p), _) => format!("({f}({b}, {} -> {}))", slot(var, Wrap::CommaArrow), slot(p, c)),
                (_, Some(lo), Some(hi)) => format!("({f}({b}, [{}, {}, {}]))", slot(var, c), slot(lo, c), slot(hi, c)),
                _ => format!("({f}({b}, {}))", slot(var, Wrap::CommaList)),
            }
        }
        Node::Graphic { path } => {
            let mut lit = String::new();
            mc_syntax::string_literal(path, &mut lit);
            format!("(image({lit}))")
        }
        Node::Labeled { text, .. } => {
            // no box marks: the printed call reads back as itself
            out.push_str(text);
            return;
        }
    };
    out.push('(');
    out.push_str(&body);
    out.push(')');
}

/// Where each item of a tree is in its canonical text: the byte offset of every item of every row (and of each
/// row's end), by row path. The editor colours typeset input with the classes the syntax colouring gives the
/// canonical text (design 3.11 in typeset cells).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TextMap {
    rows: HashMap<RowPath, Vec<usize>>,
}

impl TextMap {
    /// The byte offset in the canonical text of item `index` of the row at `path` (`index == len`: the row's end).
    pub fn offset(&self, path: &[Step], index: usize) -> Option<usize> {
        self.rows.get(path)?.get(index).copied()
    }

    /// The slots of box item `item` of the row at `path`: their steps and item offsets.
    fn slots(&self, path: &[Step], item: usize) -> Vec<(Step, &Vec<usize>)> {
        let mut out = vec![];
        let mut p = path.to_vec();
        for slot in 0.. {
            p.push(Step { item, slot });
            let Some(o) = self.rows.get(&p) else { break };
            out.push((Step { item, slot }, o));
            p.pop();
        }
        out
    }

    /// The caret position for byte `b` of the canonical text: inside the innermost slot holding it, before the
    /// first item starting at or after it.
    pub fn caret_at(&self, b: usize) -> (RowPath, usize) {
        let mut path: RowPath = vec![];
        'down: loop {
            let Some(offs) = self.rows.get(&path) else { return (path, 0) };
            let n = offs.len().saturating_sub(1);
            for i in 0..n {
                if offs[i] < b && b < offs[i + 1] {
                    for (step, so) in self.slots(&path, i) {
                        if so.first().is_some_and(|&s| s <= b) && so.last().is_some_and(|&e| b <= e) {
                            path.push(step);
                            continue 'down;
                        }
                    }
                }
            }
            let offset = offs[..n].iter().take_while(|&&o| o < b).count();
            return (path, offset);
        }
    }

    /// The smallest selection of items covering bytes `range` of the canonical text (Find in typeset input).
    pub fn find(&self, range: std::ops::Range<usize>) -> Option<(RowPath, std::ops::Range<usize>)> {
        if range.start >= range.end {
            return None;
        }
        let mut path: RowPath = vec![];
        'down: loop {
            let offs = self.rows.get(&path)?;
            let n = offs.len().saturating_sub(1);
            let hit: Vec<usize> = (0..n).filter(|&i| offs[i] < range.end && offs[i + 1] > range.start).collect();
            let (first, last) = (*hit.first()?, *hit.last()?);
            if first == last {
                for (step, so) in self.slots(&path, first) {
                    if so.first().is_some_and(|&s| s <= range.start) && so.last().is_some_and(|&e| range.end <= e) {
                        path.push(step);
                        continue 'down;
                    }
                }
            }
            return Some((path, first..last + 1));
        }
    }
}

/// The canonical text of a row with the place of each item in it ([`to_text`] gives the same text).
pub fn to_text_mapped(row: &Row) -> (String, TextMap) {
    let mut out = String::new();
    let mut map = TextMap::default();
    write_row_mapped(row, &mut Vec::new(), &mut out, &mut map);
    (out, map)
}

fn write_row_mapped(row: &Row, path: &mut RowPath, out: &mut String, map: &mut TextMap) {
    let mut offsets = Vec::with_capacity(row.len() + 1);
    for (i, it) in row.0.iter().enumerate() {
        offsets.push(out.len());
        match it {
            Item::Char(c) => out.push(*c),
            Item::Placeholder => out.push('□'),
            Item::Box(b) => write_node_mapped(b, i, path, out, map),
        }
    }
    offsets.push(out.len());
    map.rows.insert(path.clone(), offsets);
}

/// [`write_node`] with the places of the slots' items recorded: the same text, written piece by piece.
fn write_node_mapped(n: &Node, item: usize, path: &mut RowPath, out: &mut String, map: &mut TextMap) {
    if let Node::Labeled { text, .. } = n {
        out.push_str(text);
        return;
    }
    let slots = n.slots();
    // a slot's text at the end of `out`, in one more pair of parentheses when `wrap` says so
    let mut slot = |r: &Row, wrap: Option<Wrap>, out: &mut String, map: &mut TextMap| {
        let k = slots.iter().position(|s| std::ptr::eq(*s, r)).unwrap_or(0);
        let mut buf = String::new();
        let mut local = TextMap::default();
        path.push(Step { item, slot: k });
        write_row_mapped(r, path, &mut buf, &mut local);
        path.pop();
        let wrapped = wrap.is_some_and(|w| {
            let s = Scan::new(&buf);
            w.needs(&s, 0, s.chars.len())
        });
        if wrapped {
            out.push('(');
        }
        let shift = out.len();
        out.push_str(&buf);
        if wrapped {
            out.push(')');
        }
        for (p, offs) in local.rows {
            map.rows.insert(p, offs.into_iter().map(|o| o + shift).collect());
        }
    };
    let c = Some(Wrap::Comma);
    out.push_str("((");
    match n {
        Node::Fraction { num, den } => {
            slot(num, None, out, map);
            out.push_str(")/(");
            slot(den, None, out, map);
        }
        Node::Superscript { base, exp } => {
            slot(base, None, out, map);
            out.push_str(")^(");
            slot(exp, None, out, map);
        }
        Node::Subscript { base, index: i } => {
            out.push_str("subscript(");
            slot(base, c, out, map);
            out.push_str(", ");
            slot(i, c, out, map);
            out.push(')');
        }
        Node::Radical { radicand, index: None } => {
            out.push_str("sqrt(");
            slot(radicand, c, out, map);
            out.push(')');
        }
        Node::Radical { radicand, index: Some(i) } => {
            out.push_str("root(");
            slot(radicand, c, out, map);
            out.push_str(", ");
            slot(i, c, out, map);
            out.push(')');
        }
        Node::Overscript { base, over } => {
            out.push_str("overscript(");
            slot(base, c, out, map);
            out.push_str(", ");
            slot(over, c, out, map);
            out.push(')');
        }
        Node::Underscript { base, under } => {
            out.push_str("underscript(");
            slot(base, c, out, map);
            out.push_str(", ");
            slot(under, c, out, map);
            out.push(')');
        }
        Node::Underoverscript { base, under, over } => {
            out.push_str("underoverscript(");
            slot(base, c, out, map);
            out.push_str(", ");
            slot(under, c, out, map);
            out.push_str(", ");
            slot(over, c, out, map);
            out.push(')');
        }
        Node::Grid { rows } => {
            out.push('[');
            for (k, r) in rows.iter().enumerate() {
                if k > 0 {
                    out.push_str(", ");
                }
                out.push('[');
                for (j, x) in r.iter().enumerate() {
                    if j > 0 {
                        out.push_str(", ");
                    }
                    slot(x, c, out, map);
                }
                out.push(']');
            }
            out.push(']');
        }
        Node::BigOp { op, body, var, lower, upper } => {
            out.push_str(op.function());
            out.push('(');
            slot(body, c, out, map);
            out.push_str(", ");
            match (op, lower, upper) {
                (BigOp::Limit, Some(p), _) => {
                    slot(var, Some(Wrap::CommaArrow), out, map);
                    out.push_str(" -> ");
                    slot(p, c, out, map);
                }
                (_, Some(lo), Some(hi)) => {
                    out.push('[');
                    slot(var, c, out, map);
                    out.push_str(", ");
                    slot(lo, c, out, map);
                    out.push_str(", ");
                    slot(hi, c, out, map);
                    out.push(']');
                }
                _ => slot(var, Some(Wrap::CommaList), out, map),
            }
            out.push(')');
        }
        Node::Graphic { path: p } => {
            let mut lit = String::new();
            mc_syntax::string_literal(p, &mut lit);
            out.push_str("image(");
            out.push_str(&lit);
            out.push(')');
        }
        // written before the match
        Node::Labeled { .. } => {}
    }
    out.push_str("))");
}

struct Reader<'a> {
    s: &'a Scan,
    /// the box read at each (opening parenthesis, depth): the fraction rule looks at inner groups again
    memo: RefCell<HashMap<(usize, usize), Option<Node>>>,
}

/// The heads of the call shapes.
const CALLS: &[&str] =
    &["subscript", "sqrt", "root", "overscript", "underscript", "underoverscript", "sum", "product", "integrate", "limit", "d", "image"];

impl Reader<'_> {
    fn row(&self, lo: usize, hi: usize, depth: usize) -> Row {
        let s = self.s;
        let mut items = Vec::new();
        let mut i = lo;
        while i < hi {
            if s.is(i, '(') && depth < MAX_NESTING {
                if let Some(j) = s.close[i].filter(|&j| j < hi) {
                    if let Some(n) = self.node(i, j, depth + 1) {
                        items.push(Item::Box(Box::new(n)));
                        i = j + 1;
                        continue;
                    }
                }
            }
            let c = s.chars[i];
            items.push(if c == '□' && !s.literal[i] { Item::Placeholder } else { Item::Char(c) });
            i += 1;
        }
        Row(items)
    }

    /// A slot row: never empty.
    fn slot(&self, lo: usize, hi: usize, depth: usize) -> Option<Row> {
        (hi > lo).then(|| self.row(lo, hi, depth))
    }

    /// A call argument's slot: for all but the first argument one leading space (after `, `) dropped, then the
    /// wrapping pair if [`Wrap`] put one.
    fn arg(&self, (lo, hi): (usize, usize), w: Wrap, depth: usize, after_sep: bool) -> Option<Row> {
        let range = if after_sep { strip_lead(self.s, lo, hi) } else { (lo, hi) };
        let (lo, hi) = self.unwrap(range, w);
        self.slot(lo, hi, depth)
    }

    fn unwrap(&self, (lo, hi): (usize, usize), w: Wrap) -> (usize, usize) {
        if self.s.is_group(lo, hi, '(') && w.needs(self.s, lo + 1, hi - 1) {
            (lo + 1, hi - 1)
        } else {
            (lo, hi)
        }
    }

    /// The box whose text is the group `i..=j`, if it has one of the shapes.
    fn node(&self, i: usize, j: usize, depth: usize) -> Option<Node> {
        if let Some(n) = self.memo.borrow().get(&(i, depth)) {
            return n.clone();
        }
        let n = self.node_uncached(i, j, depth);
        self.memo.borrow_mut().insert((i, depth), n.clone());
        n
    }

    fn node_uncached(&self, i: usize, j: usize, depth: usize) -> Option<Node> {
        let s = self.s;
        if !s.is(i + 1, '(') {
            return None;
        }
        let k = s.close[i + 1]?;
        if k == j - 1 {
            // `((inner))`: a call shape or a grid
            return self.call(i + 2, j - 1, depth);
        }
        // `((N)op(D))`
        let op = s.chars.get(k + 1).copied().filter(|_| !s.literal[k + 1])?;
        if !(op == '/' || op == '^') || !s.is_group(k + 2, j, '(') {
            return None;
        }
        // The slot delimiters are plain parentheses: if either is itself a box's text, this is a parenthesised
        // `box op box` the user wrote (`(x²/√y)`), not a fraction or power box.
        if depth < MAX_NESTING && (self.node(i + 1, k, depth + 1).is_some() || self.node(k + 2, j - 1, depth + 1).is_some()) {
            return None;
        }
        let a = self.slot(i + 2, k, depth)?;
        let b = self.slot(k + 3, j - 1, depth)?;
        Some(if op == '/' { Node::Fraction { num: a, den: b } } else { Node::Superscript { base: a, exp: b } })
    }

    fn call(&self, lo: usize, hi: usize, depth: usize) -> Option<Node> {
        let s = self.s;
        if s.is_group(lo, hi, '[') {
            return self.grid(lo, hi, depth);
        }
        let open = (lo..hi).find(|&p| !(s.chars[p].is_ascii_alphanumeric() || s.chars[p] == '_'))?;
        if !s.is_group(open, hi, '(') {
            return None;
        }
        let name = s.string(lo, open);
        if !CALLS.contains(&name.as_str()) {
            return None;
        }
        let args = s.split(open + 1, hi - 1, ",", false);
        let c = Wrap::Comma;
        let a = |k: usize, w: Wrap| self.arg(args[k], w, depth, k > 0);
        Some(match (name.as_str(), args.len()) {
            ("subscript", 2) => Node::Subscript { base: a(0, c)?, index: a(1, c)? },
            ("sqrt", 1) => Node::Radical { radicand: a(0, c)?, index: None },
            ("root", 2) => Node::Radical { radicand: a(0, c)?, index: Some(a(1, c)?) },
            ("overscript", 2) => Node::Overscript { base: a(0, c)?, over: a(1, c)? },
            ("underscript", 2) => Node::Underscript { base: a(0, c)?, under: a(1, c)? },
            ("underoverscript", 3) => Node::Underoverscript { base: a(0, c)?, under: a(1, c)?, over: a(2, c)? },
            ("image", 1) => {
                Node::Graphic { path: string_value(s, args[0].0, args[0].1)? }
            }
            ("limit", 2) => {
                let (l, h) = strip_lead(s, args[1].0, args[1].1);
                let parts = s.split(l, h, "->", true);
                let [(vl, vh), (pl, ph)] = parts[..] else { return None };
                let vh = if vh > vl && s.chars[vh - 1] == ' ' { vh - 1 } else { vh };
                let (vl, vh) = self.unwrap((vl, vh), Wrap::CommaArrow);
                Node::BigOp {
                    op: BigOp::Limit,
                    body: a(0, c)?,
                    var: self.slot(vl, vh, depth)?,
                    lower: Some(self.arg((pl, ph), c, depth, true)?),
                    upper: None,
                }
            }
            ("sum" | "product" | "integrate" | "d", 2) => {
                let op = match name.as_str() {
                    "sum" => BigOp::Sum,
                    "product" => BigOp::Product,
                    "integrate" => BigOp::Integral,
                    _ => BigOp::Derivative,
                };
                let body = a(0, c)?;
                let (l, h) = strip_lead(s, args[1].0, args[1].1);
                if op != BigOp::Derivative && s.is_group(l, h, '[') {
                    let items = s.split(l + 1, h - 1, ",", false);
                    let [v, lo_, hi_] = items[..] else { return None };
                    Node::BigOp {
                        op,
                        body,
                        var: self.arg(v, c, depth, false)?,
                        lower: Some(self.arg(lo_, c, depth, true)?),
                        upper: Some(self.arg(hi_, c, depth, true)?),
                    }
                } else {
                    Node::BigOp { op, body, var: a(1, Wrap::CommaList)?, lower: None, upper: None }
                }
            }
            _ => return None,
        })
    }

    fn grid(&self, lo: usize, hi: usize, depth: usize) -> Option<Node> {
        let s = self.s;
        let mut rows = Vec::new();
        for (n, r) in s.split(lo + 1, hi - 1, ",", false).into_iter().enumerate() {
            let (l, h) = if n > 0 { strip_lead(s, r.0, r.1) } else { r };
            if !s.is_group(l, h, '[') {
                return None;
            }
            let cells = s.split(l + 1, h - 1, ",", false);
            rows.push(
                cells.into_iter().enumerate().map(|(m, c)| self.arg(c, Wrap::Comma, depth, m > 0)).collect::<Option<Vec<_>>>()?,
            );
        }
        let n = Node::Grid { rows };
        n.is_well_formed().then_some(n)
    }
}

/// Drops one leading space (the one after a separator).
fn strip_lead(s: &Scan, lo: usize, hi: usize) -> (usize, usize) {
    if lo < hi && s.chars[lo] == ' ' {
        (lo + 1, hi)
    } else {
        (lo, hi)
    }
}

/// The value of a string literal spanning exactly `lo..hi` (escapes `\n \t \" \\ \u{..}`).
fn string_value(s: &Scan, lo: usize, hi: usize) -> Option<String> {
    if hi < lo + 2 || s.chars[lo] != '"' || s.chars[hi - 1] != '"' {
        return None;
    }
    let mut out = String::new();
    let mut i = lo + 1;
    while i < hi - 1 {
        let c = s.chars[i];
        if c == '"' {
            return None;
        }
        if c != '\\' {
            out.push(c);
            i += 1;
            continue;
        }
        i += 1;
        match s.chars.get(i)? {
            'n' => out.push('\n'),
            't' => out.push('\t'),
            '"' => out.push('"'),
            '\\' => out.push('\\'),
            'u' => {
                let close = (i..hi).find(|&k| s.chars[k] == '}')?;
                let hex: String = s.chars.get(i + 2..close)?.iter().collect();
                out.push(char::from_u32(u32::from_str_radix(&hex, 16).ok()?)?);
                i = close;
            }
            _ => return None,
        }
        i += 1;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::notation;

    fn rt(t: &str) -> String {
        let r = from_text(t);
        assert_eq!(to_text(&r), t, "text round trip of {t}");
        notation(&r)
    }

    #[test]
    fn reads_every_shape() {
        assert_eq!(rt("x = ((a + b)/(2c))"), "x = (a + b/2c)");
        assert_eq!(rt("((x)^(2)) + 1"), "(x^2) + 1");
        assert_eq!(rt("((subscript(x, i)))"), "(x_i)");
        assert_eq!(rt("((sqrt(x + 1)))"), "√(x + 1)");
        assert_eq!(rt("((root(x, 3)))"), "√[3](x)");
        assert_eq!(rt("((overscript(x, ^)))((underscript(y, _)))"), "(x over ^)(y under _)");
        assert_eq!(rt("((underoverscript(S, a, b)))"), "(S under a over b)");
        assert_eq!(rt("(([[a, b], [c, d]]))"), "grid[[a,b],[c,d]]");
        assert_eq!(rt("((sum(k^2, [k, 1, n])))"), "∑[k=1..n](k^2)");
        assert_eq!(rt("((product(f(k), k)))"), "∏[k](f(k))");
        assert_eq!(rt("((integrate(sin(x), [x, 0, pi])))"), "∫[x=0..pi](sin(x))");
        assert_eq!(rt("((integrate(f, x)))"), "∫[x](f)");
        assert_eq!(rt("((limit(sin(x)/x, x -> 0)))"), "lim[x→0](sin(x)/x)");
        assert_eq!(rt("((d(f, x)))"), "∂[x](f)");
        assert_eq!(rt(r#"((image("a \"b\".png")))"#), "image(a \"b\".png)");
        assert_eq!(rt("((□)/(□))"), "(□/□)");
    }

    #[test]
    fn nested_and_linear_text_is_kept() {
        assert_eq!(rt("((((subscript(x, i))))^(2))"), "((x_i)^2)");
        assert_eq!(rt("(((a)/(b)))"), "((a/b))");
        assert_eq!(rt("(((sqrt(x))))"), "(√(x))");
        assert_eq!(rt("((a+b))"), "((a+b))");
        assert_eq!(rt("((foo(x)))"), "((foo(x)))");
        assert_eq!(rt("((sqrt(x, y)))"), "((sqrt(x, y)))");
        assert_eq!(rt("(([[a], [b, c]]))"), "(([[a], [b, c]]))");
        assert_eq!(rt("\"((a)/(b))\" # ((a)/(b))"), "\"((a)/(b))\" # ((a)/(b))");
        assert_eq!(rt("f(((a)/(b))"), "f((a/b)");
        assert_eq!(rt("□ \"□\""), "□ \"□\"");
        assert!(matches!(from_text("□").0[..], [Item::Placeholder]));
        assert!(matches!(from_text("\"□\"").0[1], Item::Char('□')));
        assert_eq!(rt("(()/(x))"), "(()/(x))");
        // a parenthesised quotient of two boxes, not a fraction box
        assert_eq!(rt("f(((x)^(2))/((sqrt(z))))"), "f((x^2)/√(z))");
        assert_eq!(rt("(((x)^(2))^((sqrt(z))))"), "((x^2)^√(z))");
        // a fraction whose slots are boxes
        assert_eq!(rt("((((x)^(2)))/(((sqrt(z)))))"), "((x^2)/√(z))");
    }

    #[test]
    fn slots_with_separators_are_wrapped() {
        let sub = |i: &str| Row::boxed(Node::Subscript { base: Row::text("x"), index: Row::text(i) });
        for (index, text) in [
            ("i, j", "((subscript(x, (i, j))))"),
            ("(i, j)", "((subscript(x, ((i, j)))))"),
            ("(i)", "((subscript(x, (i))))"),
            (" i", "((subscript(x,  i)))"),
        ] {
            let r = sub(index);
            assert_eq!(to_text(&r), text);
            assert_eq!(from_text(text), r, "{text}");
        }
        let lim = Row::boxed(Node::BigOp {
            op: BigOp::Limit,
            body: Row::text("f"),
            var: Row::text("a -> b"),
            lower: Some(Row::text("c")),
            upper: None,
        });
        assert_eq!(to_text(&lim), "((limit(f, (a -> b) -> c)))");
        assert_eq!(from_text(&to_text(&lim)), lim);
        let int = Row::boxed(Node::BigOp { op: BigOp::Integral, body: Row::text("f"), var: Row::text("[x]"), lower: None, upper: None });
        assert_eq!(to_text(&int), "((integrate(f, ([x]))))");
        assert_eq!(from_text(&to_text(&int)), int);
    }
}
