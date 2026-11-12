//! A cell: its style, content, label and attributes (design 2.2).
use crate::style::Style;
use mc_boxes::Row;
use mc_nbfile::Attrs;
use std::sync::Arc;

/// A cell's identity, stable for the cell's lifetime in one notebook (also across undo and redo).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CellId(pub u64);

impl std::fmt::Display for CellId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "c{}", self.0)
    }
}

/// Where a cell is in the evaluation queue (drawn on its bracket).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EvalMark {
    /// not queued
    #[default]
    None,
    /// waiting in the queue
    Queued,
    /// being evaluated
    Running,
    /// its last evaluation was a read error
    Error,
}

/// One cell.
#[derive(Clone, Debug, PartialEq)]
pub struct Cell {
    /// the identity
    pub id: CellId,
    /// the style
    pub style: Style,
    /// the content (source for code styles, prose for the others, printed text for generated cells)
    pub text: String,
    /// the number n of an evaluated input or of an output (`out(n)`)
    pub label: Option<u32>,
    /// for a generated cell: the input it came from (the hidden label of Print and Message cells)
    pub input_of: Option<CellId>,
    /// Output: the value is a `failure(...)`; Message: an error or stop reason (`#!`) rather than a warning (`#?`)
    pub failure: bool,
    /// the attributes that differ from the style's defaults
    pub props: Props,
    /// the group headed by this cell is closed, showing its `k`-th cell (0: the head) (design 2.4)
    pub closed: Option<usize>,
    /// queue state
    pub eval: EvalMark,
    /// attributes the model does not interpret, kept for the file
    pub extra: Attrs,
    /// output-block attributes the model does not interpret (`size=WxH`), kept for the file
    pub out_extra: Attrs,
    /// showing its `.mcn` source instead of its content (Show expression, design 3.8): the original style
    pub expr_view: bool,
    /// a notebook-wide revision, new whenever the content or the look changes (layout caches key on it)
    pub rev: u64,
    /// a code cell's content as a box tree (design 11, 3.3): what the editor edits and the view lays out; `text` is
    /// its canonical text. `None` for other cells. Kept in step by the notebook's primitives.
    pub content: Option<Arc<Row>>,
}

/// Cell attributes that override the style's defaults; `None` means the default.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Props {
    /// `readonly`: the content cannot be changed
    pub editable: Option<bool>,
    /// `noeval`: Shift+Enter skips the cell
    pub evaluatable: Option<bool>,
    /// `nodelete`
    pub deletable: Option<bool>,
    /// `init` / `noinit`
    pub initialization: Option<bool>,
    /// `together`: grouped by hand
    pub together: bool,
    /// `hidden`: the cell itself is closed (drawn as a thin bar)
    pub hidden: bool,
    /// `typeset`: the content is canonical box text
    pub typeset: bool,
    /// `traditional`
    pub traditional: bool,
}

impl Cell {
    /// A cell with this identity, style and content.
    pub fn new(id: CellId, style: Style, text: &str) -> Cell {
        Cell {
            id,
            style,
            text: text.to_string(),
            label: None,
            input_of: None,
            failure: false,
            props: Props::default(),
            closed: None,
            eval: EvalMark::None,
            extra: Attrs::new(),
            out_extra: Attrs::new(),
            expr_view: false,
            rev: 0,
            content: None,
        }
    }

    /// Whether the cell's content is a box tree (an Input or Code cell not showing its source).
    pub fn holds_tree(&self) -> bool {
        self.style.is_code() && !self.expr_view
    }

    /// The tree the cell's text reads as: canonical box text when the cell is marked `typeset`, else its characters.
    pub fn read_tree(&self) -> Row {
        if self.props.typeset {
            mc_boxes::from_text(&self.text)
        } else {
            Row::text(&self.text)
        }
    }

    /// The tree of a code cell (its content), or of anything else its text read as canonical box text.
    pub fn tree(&self) -> std::borrow::Cow<'_, Row> {
        match &self.content {
            Some(r) => std::borrow::Cow::Borrowed(&**r),
            None => std::borrow::Cow::Owned(mc_boxes::from_text(&self.text)),
        }
    }

    /// Makes `content` agree with the text (after a change made outside the editor).
    pub(crate) fn fix_content(&mut self) {
        self.content = self.holds_tree().then(|| Arc::new(self.read_tree()));
    }

    /// Whether the content can be changed in place.
    pub fn editable(&self) -> bool {
        self.props.editable.unwrap_or(self.style != Style::Message)
    }

    /// Whether Shift+Enter evaluates the cell.
    pub fn evaluatable(&self) -> bool {
        !self.expr_view && self.props.evaluatable.unwrap_or(self.style.evaluatable())
    }

    /// Whether the cell may be deleted.
    pub fn deletable(&self) -> bool {
        self.props.deletable.unwrap_or(true)
    }

    /// Whether the cell is evaluated before the first evaluation in a fresh kernel (design 5.7).
    pub fn initialization(&self) -> bool {
        self.props.initialization.unwrap_or(self.style == Style::Code)
    }

    /// The picture an Output cell shows: the path its value `image("path")` names (relative paths are relative to
    /// the notebook's folder). `None` for anything else, which shows as text.
    pub fn graphic(&self) -> Option<String> {
        if self.style != Style::Output || self.failure || self.expr_view || !self.text.starts_with("image(") {
            return None;
        }
        mc_nbfile::images::image_path(&self.text)
    }

    /// The display size of a graphic set by resizing it (points at 100 %), `None` for its natural size.
    pub fn graphic_size(&self) -> Option<(u32, u32)> {
        mc_nbfile::images::size_of(&self.out_extra)
    }

    /// Whether the cell was made by evaluation.
    pub fn generated(&self) -> bool {
        self.style.generated()
    }
}
