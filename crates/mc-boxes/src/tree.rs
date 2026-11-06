//! The box tree (design 08, section 4.1): rows of items, where an item is a character, a placeholder or a box
//! with slots that are rows again.
use std::fmt::Write;

/// The deepest nesting of boxes that is read, built or laid out. Deeper input stays linear text (reading) or is
/// refused (editing), so every recursion over a tree is bounded.
pub const MAX_NESTING: usize = 100;

/// A sequence of items: the content of a cell or of one slot of a box.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Row(pub Vec<Item>);

/// One element of a row.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Item {
    /// a character, drawn as itself
    Char(char),
    /// an empty slot, drawn `□`; its canonical text is `□`
    Placeholder,
    /// a two-dimensional structure
    Box(Box<Node>),
}

/// The big operators (design 08, sections 4.1 and 4.4).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BigOp {
    /// `∑`: `sum(f, [k, a, b])` or `sum(f, k)`
    Sum,
    /// `∏`: `product(f, [k, a, b])` or `product(f, k)`
    Product,
    /// `∫`: `integrate(f, [x, a, b])` or `integrate(f, x)`
    Integral,
    /// `lim`: `limit(f, x -> a)` (the point is the lower slot)
    Limit,
    /// `∂`: `d(f, x)`
    Derivative,
}

impl BigOp {
    /// The library function the canonical text calls.
    pub fn function(self) -> &'static str {
        match self {
            BigOp::Sum => "sum",
            BigOp::Product => "product",
            BigOp::Integral => "integrate",
            BigOp::Limit => "limit",
            BigOp::Derivative => "d",
        }
    }

    /// The operator symbol drawn.
    pub fn symbol(self) -> &'static str {
        match self {
            BigOp::Sum => "∑",
            BigOp::Product => "∏",
            BigOp::Integral => "∫",
            BigOp::Limit => "lim",
            BigOp::Derivative => "∂",
        }
    }
}

/// A box: a two-dimensional structure with slots.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Node {
    /// numerator over denominator, with a rule
    Fraction {
        /// above the rule
        num: Row,
        /// below the rule
        den: Row,
    },
    /// base with a raised exponent; a superscript whose base is a single [`Node::Subscript`] draws both scripts
    /// stacked on the right
    Superscript {
        /// the base (a row slot of its own, not "the preceding character")
        base: Row,
        /// the exponent
        exp: Row,
    },
    /// base with a lowered index
    Subscript {
        /// the base
        base: Row,
        /// the index
        index: Row,
    },
    /// square root, or n-th root with an index in the notch
    Radical {
        /// under the sign
        radicand: Row,
        /// the root's index (`None` for a square root)
        index: Option<Row>,
    },
    /// base with something centred above
    Overscript {
        /// the base
        base: Row,
        /// above
        over: Row,
    },
    /// base with something centred below
    Underscript {
        /// the base
        base: Row,
        /// below
        under: Row,
    },
    /// base with something above and below
    Underoverscript {
        /// the base
        base: Row,
        /// below
        under: Row,
        /// above
        over: Row,
    },
    /// rows of cells in aligned columns, without delimiters (rectangular, at least one cell)
    Grid {
        /// the rows, each with the same number of cells
        rows: Vec<Vec<Row>>,
    },
    /// a big operator with its variable, limits and body
    BigOp {
        /// which operator
        op: BigOp,
        /// the summand, integrand, ...
        body: Row,
        /// the variable (`k`, `x`)
        var: Row,
        /// lower limit; for [`BigOp::Limit`] the point approached
        lower: Option<Row>,
        /// upper limit
        upper: Option<Row>,
    },
    /// an image reference (design 08, section 6); canonical text `((image("path")))`
    Graphic {
        /// the path as written in the notebook (relative to it)
        path: String,
    },
    /// a value shown with a label (typeset output of `labeled(x, l)` and `legended(x, l)`, round 13): the value as
    /// typeset, the label as plain text at `side` (a legend's entries one per line, each after its swatch); made by
    /// [`crate::math_text`] only, never by editing. Its linear and canonical text is the printed `text` (no box
    /// marks: it reads back as the call).
    Labeled {
        /// the value
        value: Row,
        /// the label's characters (strings without quotes); a legend's entries separated by new lines
        label: Row,
        /// where the label is drawn
        side: Side,
        /// the swatch characters of a legend in the label row: (item index, sRGB colour)
        swatches: Vec<(usize, [u8; 3])>,
        /// the printed text of the whole call
        text: String,
    },
}

/// Where a label goes around its value ([`Node::Labeled`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Side {
    /// under the value, centred (the default of `labeled`)
    Below,
    /// over the value, centred
    Above,
    /// before the value, on its baseline
    Left,
    /// after the value (legends): on its baseline, or a legend of several entries centred on the value
    Right,
}

/// One step of a path from a root row to a nested row: the item index in the current row and the slot index (in
/// [`Node::slots`] order) of that item's box.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Step {
    /// index of the box in its row
    pub item: usize,
    /// index of the slot in the box
    pub slot: usize,
}

/// The path from the root row to a nested row (empty for the root itself).
pub type RowPath = Vec<Step>;

impl Row {
    /// An empty row.
    pub fn new() -> Row {
        Row(Vec::new())
    }

    /// A row of plain characters; `□` becomes a placeholder.
    pub fn text(s: &str) -> Row {
        Row(s.chars().map(|c| if c == '□' { Item::Placeholder } else { Item::Char(c) }).collect())
    }

    /// A slot row holding only a placeholder.
    pub fn placeholder() -> Row {
        Row(vec![Item::Placeholder])
    }

    /// A row holding one box.
    pub fn boxed(n: Node) -> Row {
        Row(vec![Item::Box(Box::new(n))])
    }

    /// Number of items.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether the row has no items.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Whether the row is exactly one placeholder (an empty slot).
    pub fn is_placeholder(&self) -> bool {
        matches!(self.0.as_slice(), [Item::Placeholder])
    }

    /// The row's characters if it holds no box or placeholder.
    pub fn plain(&self) -> Option<String> {
        self.0.iter().map(|i| if let Item::Char(c) = i { Some(*c) } else { None }).collect()
    }

    /// The nested row at `path`, if the path is valid.
    pub fn at(&self, path: &[Step]) -> Option<&Row> {
        let mut r = self;
        for s in path {
            match r.0.get(s.item)? {
                Item::Box(b) => r = b.slot(s.slot)?,
                _ => return None,
            }
        }
        Some(r)
    }

    /// The nested row at `path`, mutably.
    pub fn at_mut(&mut self, path: &[Step]) -> Option<&mut Row> {
        let mut r = self;
        for s in path {
            match r.0.get_mut(s.item)? {
                Item::Box(b) => r = b.slot_mut(s.slot)?,
                _ => return None,
            }
        }
        Some(r)
    }

    /// The box holding the row at `path` (the last step's item), if `path` is not empty.
    pub fn box_of(&self, path: &[Step]) -> Option<&Node> {
        let (last, parent) = path.split_last()?;
        match self.at(parent)?.0.get(last.item)? {
            Item::Box(b) => Some(b),
            _ => None,
        }
    }

    /// Puts a placeholder into every empty slot below this row (slots are never empty; the root may be).
    pub fn fill_empty_slots(&mut self) {
        for it in &mut self.0 {
            if let Item::Box(b) = it {
                b.fill_empty_slots();
            }
        }
    }

    /// Depth of box nesting (0 for a row without boxes).
    pub fn nesting(&self) -> usize {
        self.0
            .iter()
            .map(|i| match i {
                Item::Box(b) => 1 + b.slots().iter().map(|s| s.nesting()).max().unwrap_or(0),
                _ => 0,
            })
            .max()
            .unwrap_or(0)
    }

    /// Whether the row contains a placeholder anywhere.
    pub fn has_placeholder(&self) -> bool {
        self.0.iter().any(|i| match i {
            Item::Placeholder => true,
            Item::Box(b) => b.slots().iter().any(|s| s.has_placeholder()),
            Item::Char(_) => false,
        })
    }
}

impl Node {
    /// The slots in reading order (left to right, top to bottom as drawn); [`Step::slot`] indexes this list.
    pub fn slots(&self) -> Vec<&Row> {
        match self {
            Node::Fraction { num, den } => vec![num, den],
            Node::Superscript { base, exp } => vec![base, exp],
            Node::Subscript { base, index } => vec![base, index],
            Node::Radical { radicand, index } => index.iter().chain(std::iter::once(radicand)).collect(),
            Node::Overscript { base, over } => vec![base, over],
            Node::Underscript { base, under } => vec![base, under],
            Node::Underoverscript { base, under, over } => vec![base, under, over],
            Node::Grid { rows } => rows.iter().flatten().collect(),
            Node::BigOp { op, body, var, lower, upper } => match op {
                BigOp::Integral => lower.iter().chain(upper.iter()).chain([body, var]).collect(),
                _ => std::iter::once(var).chain(lower.iter()).chain(upper.iter()).chain(std::iter::once(body)).collect(),
            },
            Node::Graphic { .. } => vec![],
            Node::Labeled { value, label, .. } => vec![value, label],
        }
    }

    /// The slots in reading order, mutably.
    pub fn slots_mut(&mut self) -> Vec<&mut Row> {
        match self {
            Node::Fraction { num, den } => vec![num, den],
            Node::Superscript { base, exp } => vec![base, exp],
            Node::Subscript { base, index } => vec![base, index],
            Node::Radical { radicand, index } => index.iter_mut().chain(std::iter::once(radicand)).collect(),
            Node::Overscript { base, over } => vec![base, over],
            Node::Underscript { base, under } => vec![base, under],
            Node::Underoverscript { base, under, over } => vec![base, under, over],
            Node::Grid { rows } => rows.iter_mut().flatten().collect(),
            Node::BigOp { op, body, var, lower, upper } => match op {
                BigOp::Integral => lower.iter_mut().chain(upper.iter_mut()).chain([body, var]).collect(),
                _ => std::iter::once(var)
                    .chain(lower.iter_mut())
                    .chain(upper.iter_mut())
                    .chain(std::iter::once(body))
                    .collect(),
            },
            Node::Graphic { .. } => vec![],
            Node::Labeled { value, label, .. } => vec![value, label],
        }
    }

    /// Slot `i` in reading order.
    pub fn slot(&self, i: usize) -> Option<&Row> {
        self.slots().into_iter().nth(i)
    }

    /// Slot `i` in reading order, mutably.
    pub fn slot_mut(&mut self, i: usize) -> Option<&mut Row> {
        self.slots_mut().into_iter().nth(i)
    }

    /// Number of slots.
    pub fn slot_count(&self) -> usize {
        self.slots().len()
    }

    /// Whether every slot is an empty placeholder (such a box is deleted as a whole by Backspace).
    pub fn is_blank(&self) -> bool {
        self.slots().iter().all(|s| s.is_placeholder())
    }

    fn fill_empty_slots(&mut self) {
        for s in self.slots_mut() {
            if s.is_empty() {
                *s = Row::placeholder();
            } else {
                s.fill_empty_slots();
            }
        }
    }

    /// Whether the box is well formed: grids rectangular and not empty; sums and products with both limits or
    /// none; integrals likewise; a limit with a point and no upper slot; a derivative without limits.
    pub fn is_well_formed(&self) -> bool {
        match self {
            Node::Grid { rows } => !rows.is_empty() && !rows[0].is_empty() && rows.iter().all(|r| r.len() == rows[0].len()),
            Node::BigOp { op, lower, upper, .. } => match op {
                BigOp::Sum | BigOp::Product | BigOp::Integral => lower.is_some() == upper.is_some(),
                BigOp::Limit => lower.is_some() && upper.is_none(),
                BigOp::Derivative => lower.is_none() && upper.is_none(),
            },
            _ => true,
        }
    }
}

/// The compact notation used by tests and debugging (design 08, section 4.2's tables): `(N/D)`, `(B^E)`,
/// `(B_S)`, `√(X)` or `√[n](X)`, `(B over O)`, `(B under U)`, `(B under U over O)`, `grid[[a,b],[c,d]]`,
/// `∑[k=a..b](f)` (big operators with their slots), `image(path)`, `(V label below L)` (also `above`, `left`,
/// `right`); placeholders print `□`.
pub fn notation(row: &Row) -> String {
    let mut s = String::new();
    write_notation(row, &mut s, &mut |_, _| {});
    s
}

/// [`notation`], calling `mark(path_len_and_offset)` hooks so the editor can insert caret and selection marks:
/// `mark(depth_path, offset, out)` is called before the item at each offset of each row and at the row's end.
pub(crate) fn write_notation(row: &Row, out: &mut String, mark: &mut dyn FnMut(&mut String, Pos<'_>)) {
    let mut path = Vec::new();
    notation_row(row, out, mark, &mut path);
}

/// A position handed to the marking hook of [`write_notation`].
pub(crate) struct Pos<'a> {
    pub path: &'a [Step],
    pub offset: usize,
}

fn notation_row(row: &Row, out: &mut String, mark: &mut dyn FnMut(&mut String, Pos<'_>), path: &mut RowPath) {
    for (i, it) in row.0.iter().enumerate() {
        mark(out, Pos { path, offset: i });
        match it {
            Item::Char(c) => out.push(*c),
            Item::Placeholder => out.push('□'),
            Item::Box(b) => {
                let mut slot = |k: usize, out: &mut String, mark: &mut dyn FnMut(&mut String, Pos<'_>)| {
                    path.push(Step { item: i, slot: k });
                    if let Some(r) = b.slot(k) {
                        notation_row(r, out, mark, path);
                    }
                    path.pop();
                };
                match &**b {
                    Node::Fraction { .. } => {
                        out.push('(');
                        slot(0, out, mark);
                        out.push('/');
                        slot(1, out, mark);
                        out.push(')');
                    }
                    Node::Superscript { .. } | Node::Subscript { .. } => {
                        out.push('(');
                        slot(0, out, mark);
                        out.push(if matches!(**b, Node::Superscript { .. }) { '^' } else { '_' });
                        slot(1, out, mark);
                        out.push(')');
                    }
                    Node::Radical { index, .. } => {
                        out.push('√');
                        if index.is_some() {
                            out.push('[');
                            slot(0, out, mark);
                            out.push(']');
                        }
                        out.push('(');
                        slot(usize::from(index.is_some()), out, mark);
                        out.push(')');
                    }
                    Node::Overscript { .. } | Node::Underscript { .. } => {
                        out.push('(');
                        slot(0, out, mark);
                        out.push_str(if matches!(**b, Node::Overscript { .. }) { " over " } else { " under " });
                        slot(1, out, mark);
                        out.push(')');
                    }
                    Node::Underoverscript { .. } => {
                        out.push('(');
                        slot(0, out, mark);
                        out.push_str(" under ");
                        slot(1, out, mark);
                        out.push_str(" over ");
                        slot(2, out, mark);
                        out.push(')');
                    }
                    Node::Grid { rows } => {
                        out.push_str("grid[");
                        let w = rows.first().map_or(0, |r| r.len());
                        for (r, cells) in rows.iter().enumerate() {
                            if r > 0 {
                                out.push(',');
                            }
                            out.push('[');
                            for c in 0..cells.len() {
                                if c > 0 {
                                    out.push(',');
                                }
                                slot(r * w + c, out, mark);
                            }
                            out.push(']');
                        }
                        out.push(']');
                    }
                    Node::BigOp { op, lower, upper, .. } => {
                        out.push_str(op.symbol());
                        out.push('[');
                        let n = b.slot_count();
                        let limits = usize::from(lower.is_some()) + usize::from(upper.is_some());
                        // slot order: integral lower, upper, body, var; others var, lower, upper, body
                        let (var, lims, body) = if *op == BigOp::Integral {
                            (n - 1, (0..limits).collect::<Vec<_>>(), n - 2)
                        } else {
                            (0, (1..=limits).collect(), n - 1)
                        };
                        slot(var, out, mark);
                        for (k, l) in lims.into_iter().enumerate() {
                            out.push_str(if k == 0 { if *op == BigOp::Limit { "→" } else { "=" } } else { ".." });
                            slot(l, out, mark);
                        }
                        out.push_str("](");
                        slot(body, out, mark);
                        out.push(')');
                    }
                    Node::Graphic { path: p } => {
                        let _ = write!(out, "image({p})");
                    }
                    Node::Labeled { side, .. } => {
                        out.push('(');
                        slot(0, out, mark);
                        out.push_str(match side {
                            Side::Below => " label below ",
                            Side::Above => " label above ",
                            Side::Left => " label left ",
                            Side::Right => " label right ",
                        });
                        slot(1, out, mark);
                        out.push(')');
                    }
                }
            }
        }
    }
    mark(out, Pos { path, offset: row.0.len() });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frac(n: &str, d: &str) -> Item {
        Item::Box(Box::new(Node::Fraction { num: Row::text(n), den: Row::text(d) }))
    }

    #[test]
    fn paths_and_slots() {
        let mut r = Row::text("1+");
        r.0.push(frac("x", "□"));
        assert_eq!(notation(&r), "1+(x/□)");
        let p = vec![Step { item: 2, slot: 1 }];
        assert!(r.at(&p).unwrap().is_placeholder());
        *r.at_mut(&p).unwrap() = Row::text("y");
        assert_eq!(notation(&r), "1+(x/y)");
        assert!(r.at(&[Step { item: 0, slot: 0 }]).is_none());
        assert!(matches!(r.box_of(&p), Some(Node::Fraction { .. })));
        assert_eq!(r.nesting(), 1);
    }

    #[test]
    fn notation_of_every_box() {
        let g = Node::Grid { rows: vec![vec![Row::text("a"), Row::text("b")], vec![Row::text("c"), Row::text("d")]] };
        let s = Node::BigOp {
            op: BigOp::Sum,
            body: Row::text("f"),
            var: Row::text("k"),
            lower: Some(Row::text("1")),
            upper: Some(Row::text("n")),
        };
        let i = Node::BigOp { op: BigOp::Integral, body: Row::text("f"), var: Row::text("x"), lower: None, upper: None };
        let rad = Node::Radical { radicand: Row::text("x"), index: Some(Row::text("3")) };
        let mut r = Row::new();
        for n in [g, s, i, rad] {
            r.0.push(Item::Box(Box::new(n)));
        }
        assert_eq!(notation(&r), "grid[[a,b],[c,d]]∑[k=1..n](f)∫[x](f)√[3](x)");
        let mut e = Row::boxed(Node::Fraction { num: Row::new(), den: Row::text("2") });
        e.fill_empty_slots();
        assert_eq!(notation(&e), "(□/2)");
        assert!(e.has_placeholder());
    }
}
