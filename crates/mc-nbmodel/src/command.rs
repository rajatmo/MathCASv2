//! Commands: every key, menu item and mouse action becomes a [`Command`] applied to the model
//! ([`crate::Notebook::apply`]), which answers with [`Effects`] for the window.
use crate::cell::CellId;
use mc_boxes::RowPath;
use mc_edit::Structure;
use crate::style::Style;

/// A caret motion (design 3.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Motion {
    /// previous character (at a cell's start: the insertion point before it)
    Left,
    /// next character (at a cell's end: the insertion point after it)
    Right,
    /// previous natural word
    WordLeft,
    /// next natural word
    WordRight,
    /// previous visual line (at the first line: the insertion point above)
    Up,
    /// next visual line (at the last line: the insertion point below)
    Down,
    /// start of the visual line
    Home,
    /// end of the visual line
    End,
    /// the insertion point before the first cell
    DocStart,
    /// the insertion point after the last cell
    DocEnd,
}

/// A mouse target on a cell bracket: one cell's bracket, or a group's (by its head and end).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bracket {
    /// the bracket of one cell
    Cell(CellId),
    /// the bracket of the group of cells `start..end` (indices)
    Group(usize, usize),
}

/// What a window action outside the document asks for (handled by the application).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppAction {
    /// File ▸ New
    New,
    /// File ▸ Open...
    Open,
    /// File ▸ Save
    Save,
    /// File ▸ Save as...
    SaveAs,
    /// File ▸ Close: close the active tab (Ctrl+W)
    Close,
    /// the next tab (Ctrl+Tab, Ctrl+PageDown)
    NextTab,
    /// the previous tab (Ctrl+Shift+Tab, Ctrl+PageUp)
    PrevTab,
    /// the n-th tab, 1-based; 9 is the last (Ctrl+1 ... Ctrl+9)
    NthTab(u8),
    /// reopen the last closed tab (Ctrl+Shift+T)
    ReopenTab,
    /// the style list with type-ahead (Alt+0)
    OtherStyle,
    /// the interrupt dialog (Alt+,)
    InterruptDialog,
    /// scroll by lines without moving the caret (Ctrl+Up/Down)
    ScrollLines(i32),
    /// scroll by pages (PageUp/PageDown)
    ScrollPages(i32),
    /// larger / smaller magnification by `k` steps of the ladder (Ctrl+= / Ctrl+-, Ctrl+Alt+= / Ctrl+Alt+-, the
    /// wheel with Ctrl); 0 resets to 100 % (Ctrl+0)
    Zoom(i32),
    /// the find bar (Ctrl+F)
    Find,
    /// next / previous match (F3 / Shift+F3)
    FindNext(bool),
    /// full screen (F12)
    FullScreen,
    /// outputs on the left or on the right edge, switched (Ctrl+Shift+J; View > Output alignment)
    ToggleOutputAlignment,
    /// Help ▸ Find selected function (F1)
    Help,
    /// complete the name before the caret (Ctrl+K, F2)
    Complete,
    /// the preferences dialog (Edit > Preferences)
    Preferences,
}

/// The answer to the initialization-cells question (design 5.7).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InitAnswer {
    /// evaluate them first
    Yes,
    /// not this time
    No,
    /// evaluate them first, and do not ask again for this notebook
    Always,
}

/// A change to the document or the selection.
#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    /// typed text (at an insertion point it creates a cell)
    Insert(String),
    /// Return
    Enter,
    /// Tab: indentation (placeholders come with typeset input)
    Tab,
    /// Shift+Tab: remove one level of indentation from the selected lines
    Outdent,
    /// comment or uncomment the selected lines of an input (Alt+/)
    ToggleComment,
    /// start or finish an alias (Esc, design 3.9)
    Alias,
    /// extend the selection to the enclosing expression, then the content, then the cell (Ctrl+., design 3.4)
    ExtendSelection,
    /// select the smallest bracketed expression around the selection (Ctrl+Shift+B)
    CheckBalance,
    /// delete the previous character, or the selection, or the selected cells
    Backspace,
    /// delete the next character, or the selection, or the selected cells
    Delete,
    /// delete the previous word
    DeleteWordBack,
    /// delete the next word
    DeleteWordForward,
    /// move the caret; `true` extends the selection
    Move(Motion, bool),
    /// select all (design 2.9: the cell's content first, then every cell)
    SelectAll,
    /// undo one unit
    Undo,
    /// redo one unit
    Redo,
    /// cut
    Cut,
    /// copy
    Copy,
    /// copy the plain text only
    CopyPlain,
    /// copy the selection (or the cell's content, or the selected cells) as LaTeX (Edit > Copy as > LaTeX, M7)
    CopyLatex,
    /// replace the name being typed before the caret by this one (a completion is accepted), in plain and typeset
    /// cells
    ReplaceWord(String),
    /// paste
    Paste,
    /// paste text read as math (Ctrl+Shift+V, round 19): `x^2`, `a/b`, `sqrt(x)` and `surd(x, n)` in the clipboard's
    /// text become boxes, so a typeset cell copied to another program and back is the same cell
    PasteMath,
    /// evaluate the selected cells (Shift+Enter)
    Evaluate,
    /// evaluate, then put the caret in the next evaluatable cell (Shift+Keypad Enter)
    EvaluateNext,
    /// evaluate every evaluatable cell of the notebook
    EvaluateNotebook,
    /// evaluate the initialization cells
    EvaluateInitialization,
    /// the answer to the initialization question
    InitAnswer(InitAnswer),
    /// stop the running evaluation and clear the queue (Alt+.)
    Abort,
    /// remove the selected cells from the queue (Alt+Shift+.)
    Unqueue,
    /// stop the kernel (labels stay; the next evaluation starts a new one)
    QuitKernel,
    /// set the style of the selected cells, of the caret's cell, or of the cell typing will create
    SetStyle(Style),
    /// divide the cell at the caret (Ctrl+Shift+D)
    Divide,
    /// merge the selected cells (Ctrl+Shift+M)
    Merge,
    /// group the selected cells by hand (Ctrl+Shift+G)
    GroupTogether,
    /// remove the hand-made grouping of the selected cells (Ctrl+Shift+U)
    Ungroup,
    /// switch between automatic and manual grouping
    SetManualGrouping(bool),
    /// open or close the innermost group around the selection (Ctrl+')
    ToggleGroup,
    /// open every group in the selection (Ctrl+{)
    OpenAll,
    /// close every group in the selection (Ctrl+})
    CloseAll,
    /// toggle the initialization attribute of the selected cells (Ctrl+8)
    ToggleInitialization,
    /// toggle editability of the selected cells
    ToggleEditable,
    /// toggle evaluatability of the selected cells
    ToggleEvaluatable,
    /// toggle deletability of the selected cells
    ToggleDeletable,
    /// show the cell's `.mcn` source, or parse it back (Ctrl+Shift+E)
    ShowExpression,
    /// a copy of the nearest input above (Ctrl+L)
    InputFromAbove,
    /// a copy of the nearest output above, as an input (Ctrl+Shift+L)
    OutputFromAbove,
    /// a new empty cell of the current cell's style below it (Alt+Enter)
    SameStyleBelow,
    /// delete every generated cell
    DeleteAllOutput,
    /// a new empty cell of `style` at the insertion point, caret in it (the "+" menu)
    NewCell(Style),
    /// put the caret at `pos` in a cell (a click); `true` extends the selection
    ClickText(CellId, usize, bool),
    /// select bytes `anchor..pos` of a cell's text (Find; in typeset cells the boxes they cover)
    SelectText(CellId, usize, usize),
    /// select the word at `pos` (double-click)
    SelectWord(CellId, usize),
    /// select the line at `pos` (triple-click)
    SelectLine(CellId, usize),
    /// put the insertion point before cell `index` (a click between cells)
    ClickGap(usize),
    /// select a bracket: plain click, with Shift (extend) or with Ctrl (toggle)
    ClickBracket(Bracket, bool, bool),
    /// open or close a group by its bracket (double-click)
    DoubleClickBracket(Bracket),
    /// move (or with `true` copy) the selected cells to before cell `index`
    MoveCells(usize, bool),
    /// set the display size of an Output cell's graphic (points at 100 %), `None` for its natural size (design 6:
    /// resizing by the handles, double-click restores); an undo unit
    ResizeGraphic(CellId, Option<(u32, u32)>),
    /// set (`Some`) or remove (`None`) attributes of an Output cell's block, kept in the file: a 3D picture's view
    /// (`view`, `zoom`, `vertical`), a `manipulate` output's control values (`controls`); an undo unit
    SetOutputAttrs(CellId, Vec<(String, Option<String>)>),
    /// move control `k` of a `manipulate` output to a value; `true` when the slider is let go (round 13). The
    /// position is kept on the output and evaluated; not an undo unit
    SetControl(CellId, usize, f64, bool),
    /// a box key (NBUI.5: `Ctrl+2` square root, `Ctrl+/` fraction, `Ctrl+6` power); a plain code cell takes boxes
    /// from then on
    Structure(Structure),
    /// move the caret just after the innermost box (`Ctrl+Space`)
    ExitStructure,
    /// a click in a typeset cell at a caret position of its boxes; `true` extends the selection
    ClickBox(CellId, RowPath, usize, bool),
    /// a key that is bound to nothing on purpose (reserved)
    Nop,
    /// something the window does
    App(AppAction),
}

/// What the window should do after a command.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Effects {
    /// the document or the selection changed: repaint
    pub changed: bool,
    /// the document's content changed (for "modified")
    pub modified: bool,
    /// the selection moved: scroll it into view
    pub scroll: bool,
    /// the command could not act: beep
    pub beep: bool,
    /// a window action
    pub app: Option<AppAction>,
    /// ask whether to evaluate the initialization cells first
    pub ask_init: bool,
}

impl Command {
    /// A short description for menus and the keyboard help.
    pub fn describe(&self) -> String {
        use Command::*;
        let s = match self {
            Insert(t) => return format!("type {t:?}"),
            Enter => "new line (in items: a new item)",
            Tab => "indent",
            Outdent => "outdent",
            ToggleComment => "comment / uncomment the lines",
            Alias => "start or finish an alias",
            ExtendSelection => "extend the selection",
            CheckBalance => "check balance",
            Backspace => "delete the previous character or the selection",
            Delete => "delete the next character or the selection",
            DeleteWordBack => "delete the previous word",
            DeleteWordForward => "delete the next word",
            Move(m, e) => {
                let what = match m {
                    Motion::Left => "previous character",
                    Motion::Right => "next character",
                    Motion::WordLeft => "previous word",
                    Motion::WordRight => "next word",
                    Motion::Up => "previous line",
                    Motion::Down => "next line",
                    Motion::Home => "line start",
                    Motion::End => "line end",
                    Motion::DocStart => "notebook start",
                    Motion::DocEnd => "notebook end",
                };
                return if *e { format!("extend the selection to the {what}") } else { format!("move to the {what}") };
            }
            SelectAll => "select all",
            Undo => "undo",
            Redo => "redo",
            Cut => "cut",
            Copy => "copy",
            CopyPlain => "copy as plain text",
            CopyLatex => "copy as LaTeX",
            ReplaceWord(_) => "complete the name",
            Paste => "paste",
            PasteMath => "paste as math (powers, fractions and roots as boxes)",
            Evaluate => "evaluate the selected cells",
            EvaluateNext => "evaluate, then go to the next input",
            EvaluateNotebook => "evaluate the notebook",
            EvaluateInitialization => "evaluate the initialization cells",
            InitAnswer(_) => "answer the initialization question",
            Abort => "abort the evaluation",
            Unqueue => "remove the selected cells from the queue",
            QuitKernel => "quit the kernel",
            SetStyle(s) => return format!("style: {}", s.info().name),
            Divide => "divide the cell",
            Merge => "merge the cells",
            GroupTogether => "group the cells together",
            Ungroup => "ungroup the cells",
            SetManualGrouping(_) => "manual grouping",
            ToggleGroup => "open or close the group",
            OpenAll => "open all subgroups",
            CloseAll => "close all subgroups",
            ToggleInitialization => "toggle the initialization attribute",
            ToggleEditable => "toggle editable",
            ToggleEvaluatable => "toggle evaluatable",
            ToggleDeletable => "toggle deletable",
            ShowExpression => "show the cell's source",
            InputFromAbove => "input from above",
            OutputFromAbove => "output from above",
            SameStyleBelow => "a new cell of the same style",
            DeleteAllOutput => "delete all output",
            NewCell(s) => return format!("new {} cell", s.info().name),
            SelectText(..) => "select text",
            ClickText(..)
            | SelectWord(..)
            | SelectLine(..)
            | ClickGap(_)
            | ClickBracket(..)
            | DoubleClickBracket(_) => "mouse",
            MoveCells(..) => "move cells",
            ResizeGraphic(..) => "resize the graphic",
            SetOutputAttrs(..) => "change the output's view",
            SetControl(..) => "move a control",
            Structure(s) => match s {
                mc_edit::Structure::Fraction => "fraction",
                mc_edit::Structure::Superscript => "power",
                mc_edit::Structure::Radical => "square root",
                mc_edit::Structure::Root => "n-th root (in a root: its index)",
            },
            ExitStructure => "move out of the box",
            ClickBox(..) => "mouse",
            Nop => "(reserved, does nothing)",
            App(a) => match a {
                AppAction::New => "new notebook",
                AppAction::Open => "open a notebook",
                AppAction::Save => "save",
                AppAction::SaveAs => "save as",
                AppAction::Close => "close the notebook (tab)",
                AppAction::NextTab => "next tab",
                AppAction::PrevTab => "previous tab",
                AppAction::NthTab(9) => "last tab",
                AppAction::NthTab(_) => "tab by number",
                AppAction::ReopenTab => "reopen the closed tab",
                AppAction::OtherStyle => "other style...",
                AppAction::InterruptDialog => "interrupt...",
                AppAction::ScrollLines(_) => "scroll one line",
                AppAction::ScrollPages(_) => "scroll one page",
                AppAction::Zoom(_) => "magnification",
                AppAction::Find => "find",
                AppAction::FindNext(true) => "find next",
                AppAction::FindNext(false) => "find previous",
                AppAction::FullScreen => "full screen",
                AppAction::ToggleOutputAlignment => "output alignment: left / right",
                AppAction::Help => "help for the selected name",
                AppAction::Complete => "complete the name",
                AppAction::Preferences => "preferences",
            },
        };
        s.to_string()
    }
}
