//! The notebook canvas (design 9.6): per-cell layout cached by revision, cell positions as prefix sums, painting
//! of the visible cells only (labels, brackets, the insertion point, the caret and selections), and hit-testing.
//! Output cells show pictures (design 6), outputs broken into lines at the cell's width (4.8), and large outputs as
//! a sample with buttons to see more (4.9).
use mc_ui::fonts::Face;
use crate::highlight;
use crate::images::{Images, Picture};
use crate::theme::Palette;
use crate::typeset::{self, Background, Glyphs, LayoutKey};
use mc_boxes::layout::{Layout, LayoutOptions};
use mc_boxes::RowPath;
use eframe::egui::{self, pos2, vec2, Align2, Color32, Galley, Painter, Pos2, Rect, Stroke, Vec2};
use eframe::epaint::text::{cursor::CCursor, LayoutJob, TextFormat};
use mc_nbmodel::{printed, Bracket, Cell, CellId, EvalMark, FontRole, Notebook, Selection, Style};
use mc_syntax::Colouring;
use std::cell::RefCell;
use std::collections::HashMap;
use std::collections::HashSet;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

/// Space above the first cell and the column pitch of brackets, in points at 100 %.
const TOP_PAD: f32 = 8.0;
/// The distance between bracket columns.
pub const BRACKET_PITCH: f32 = 7.0;
/// The length of a bracket's hooks.
const HOOK: f32 = 3.0;
/// The x of the outermost bracket column (points from the canvas's left edge).
const BRACKET_LEFT: f32 = 8.0;
/// Bracket columns always reserved (cells, sections and one more level), so the content does not move sideways
/// whenever the first group appears.
const MIN_COLUMNS: usize = 2;
/// The gap between the innermost bracket and the content (points at 100 %, scaled with the magnification).
const BRACKET_GAP: f32 = 8.0;
/// The space right of the content.
const RIGHT_PAD: f32 = 12.0;
/// The left margin of the styles that start at the content edge: the styles' left margins (design 2.1) are taken
/// relative to it, so Title, Section, Text and Input are left-justified and items keep their indents.
const BASE_MARGIN: f32 = 66.0;
/// Off-screen cells laid out per frame to refine their estimated heights, and the time that may take (a typeset
/// output can take milliseconds; the frame must not).
const REFINE_PER_FRAME: usize = 64;
const REFINE_BUDGET: f64 = 0.004;
/// Outputs longer than this (characters of printed text) are shown as linear text even when typeset output is on
/// (NB5.6: about 1 ms per 1,000 characters, on a worker thread).
pub const TYPESET_LIMIT: usize = 100_000;
/// Outputs up to this many characters are typeset in the frame (a few milliseconds); longer ones in the background.
const TYPESET_SYNC: usize = 3_000;

/// What the colours of a typeset input cell depend on: its revision, the kernel's names, the theme, the width.
type ColourKey = (u64, u64, bool, u32);
/// The colours of a typeset input cell's characters, per layout row and item.
type Colours = Arc<Vec<Vec<Color32>>>;

/// The text look of a cell.
#[derive(Clone, Copy, Debug)]
pub struct Look {
    /// the face
    pub face: Face,
    /// the size in points (at 100 %)
    pub size: f32,
    /// the colour
    pub color: Color32,
}

/// The text look of a cell in a palette.
pub fn look(cell: &Cell, p: &Palette) -> Look {
    let info = cell.style.info();
    let mut face = match (info.font, info.bold) {
        (FontRole::Code, true) => Face::CodeBold,
        (FontRole::Code, false) => Face::Code,
        (FontRole::Prose, true) => Face::ProseBold,
        (FontRole::Prose, false) => Face::Prose,
        (FontRole::Ui, _) => Face::Ui,
    };
    let mut size = info.size;
    if cell.expr_view {
        face = Face::Code;
        size = 13.0;
    }
    let color = match cell.style {
        Style::Section | Style::Subsection if !cell.expr_view => p.blue,
        Style::Message if cell.failure => p.dark_red,
        Style::Message => p.orange,
        Style::Output if cell.failure => p.dark_red,
        Style::Subtitle | Style::Subsubtitle => p.weak,
        _ => p.text,
    };
    Look { face, size, color }
}

/// The height of a cell's laid-out content, with what it depends on. Galleys themselves are not kept across
/// frames: egui caches them by content and drops them when its glyph atlas is rebuilt, which a kept galley would
/// not survive.
struct Laid {
    key: (u64, u32, u32, bool, u32),
    height: f32,
}

/// Where a pointer position is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Hit {
    /// in a cell's content, at a byte position
    Text(CellId, usize),
    /// on a bracket
    Bracket(Bracket),
    /// between cells: the insertion point before cell `i`
    Gap(usize),
    /// the "+" button of the insertion point before cell `i`
    Plus(usize),
    /// on the picture of an Output cell
    Graphic(CellId),
    /// on a resize handle of the selected picture
    Handle(CellId, Handle),
    /// on a button of a large output's summary
    Large(CellId, LargeAction),
    /// in the boxes of a typeset input cell ([`View::box_hit`] gives the caret position)
    Boxes(CellId),
    /// on the slider of control `k` of a `manipulate` output
    Control(CellId, usize),
}

/// A resize handle of a selected picture: its place on the frame, -1 (left, top), 0 (middle) or 1 (right, bottom).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Handle {
    /// -1 left, 0 middle, 1 right
    pub x: i8,
    /// -1 top, 0 middle, 1 bottom
    pub y: i8,
}

impl Handle {
    /// The eight handles: corners and the middles of the sides.
    pub const ALL: [Handle; 8] = [
        Handle { x: -1, y: -1 },
        Handle { x: 0, y: -1 },
        Handle { x: 1, y: -1 },
        Handle { x: -1, y: 0 },
        Handle { x: 1, y: 0 },
        Handle { x: -1, y: 1 },
        Handle { x: 0, y: 1 },
        Handle { x: 1, y: 1 },
    ];

    /// Where the handle is on a picture's frame.
    pub fn at(self, r: Rect) -> Pos2 {
        let f = |v: i8, a: f32, b: f32| match v {
            -1 => a,
            0 => (a + b) / 2.0,
            _ => b,
        };
        pos2(f(self.x, r.left(), r.right()), f(self.y, r.top(), r.bottom()))
    }

    /// The pointer shape over the handle.
    pub fn cursor(self) -> egui::CursorIcon {
        match (self.x, self.y) {
            (0, _) => egui::CursorIcon::ResizeVertical,
            (_, 0) => egui::CursorIcon::ResizeHorizontal,
            (x, y) if x == y => egui::CursorIcon::ResizeNwSe,
            _ => egui::CursorIcon::ResizeNeSw,
        }
    }
}

/// The buttons under a large output's sample (design 4.9).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LargeAction {
    /// halve the sample
    Less,
    /// double the sample
    More,
    /// show the whole output
    All,
    /// back to the first sample
    Sample,
}

/// What a cell's content area shows.
enum Shown {
    /// its text
    Text,
    /// the picture at this path
    Graphic(PathBuf),
    /// the sample of a large output
    Large,
    /// typeset input, edited as boxes (design 4, NB5.3)
    Boxes,
    /// a typeset output (design 4.5, M5): one unit for the caret
    Typeset,
    /// the sliders of a `manipulate` output (round 13)
    Controls,
}

/// A large output as shown: the sample (or the whole text) broken into lines at the cell's width.
struct Long {
    key: (u64, usize, usize),
    text: Arc<str>,
    rows: Vec<Range<usize>>,
    /// the characters of the whole output
    chars: usize,
    /// the characters of its longest line
    widest: usize,
}

/// The size of the resize handles and how far from them the pointer still takes them (points).
const HANDLE: f32 = 6.0;
const HANDLE_REACH: f32 = 6.0;

/// The caret's blink period (half on, half off), as the platform's default.
pub const BLINK: f64 = 0.53;

/// Egui points per typographic point: the styles' sizes are points (Input 13 pt), drawn as a word processor does at
/// 100 % on a 96 dpi screen (13 pt = 17.3 px); before this the sizes were taken as pixels and text was 3/4 size.
pub const PX_PER_PT: f32 = 96.0 / 72.0;

/// The canvas state that lives across frames.
pub struct View {
    laid: HashMap<CellId, Laid>,
    /// cell tops relative to the canvas top, and the total height
    /// measure only the cells that show (the canvas holds a magnification anchor this frame)
    pub hold: bool,
    /// outputs end at the content's right edge (View > Output alignment)
    outputs_right: bool,
    tops: Vec<f32>,
    heights: Vec<f32>,
    /// the scale from the styles' sizes (typographic points) to egui points: the magnification (1.0 = 100 %) times
    /// `PX_PER_PT`; set through `set_magnification`
    pub zoom: f32,
    /// the canvas width of the last frame
    pub width: f32,
    /// the canvas's top-left of the last frame (screen coordinates)
    pub origin: Pos2,
    /// the palette
    pub palette: Palette,
    /// the time the caret last moved (blink phase)
    pub blink_from: f64,
    /// the caret blinks (off: steady)
    pub blink: bool,
    /// the bracket under the pointer
    pub hover: Option<Bracket>,
    /// an insertion line to draw while cells are dragged
    pub drop_gap: Option<usize>,
    /// the pointer is on the insertion point's "+" button
    pub plus_hover: bool,
    /// the largest bracket column of the groups
    max_col: usize,
    /// the caret, for bracket matching
    pub caret: Option<(CellId, usize)>,
    /// the names the kernel defines (`None`: no kernel, every global name is plain)
    defined: Option<Arc<HashSet<String>>>,
    defined_rev: u64,
    colours: HashMap<CellId, ((u64, u64), Arc<Colouring>)>,
    /// the window's picture cache (shared by its tabs)
    images: Rc<RefCell<Images>>,
    /// the notebook's folder: relative picture paths are in it
    pub base: Option<PathBuf>,
    /// the picture selected by a click (drawn with its frame and handles)
    pub graphic: Option<CellId>,
    /// a picture being resized and its size on screen (points)
    pub live_size: Option<(CellId, Vec2)>,
    /// a 3D picture being turned or zoomed: its view until the change is kept on the output
    pub live_view: Option<(CellId, mc_graphics::view3d::View3d)>,
    /// the window's 3D pictures (turning them, `rotate3d`)
    pub rot: Option<Rc<RefCell<crate::rotate3d::Rotations>>>,
    /// while a right-justified picture is resized: the canvas x of the edge that stays put (the one opposite the
    /// dragged handle), so the dragged edge follows the pointer; the picture is right-justified again on release
    pub live_anchor: Option<(f32, i8)>,
    /// the sample size of large outputs whose buttons were used (`usize::MAX`: all of it)
    budgets: HashMap<CellId, usize>,
    /// whether an output is large, by its revision
    large: HashMap<CellId, (u64, bool)>,
    /// the controls of `manipulate` outputs, by revision
    manip: HashMap<CellId, (u64, Vec<mc_nbmodel::Control>)>,
    /// large outputs as shown
    long: HashMap<CellId, Arc<Long>>,
    /// output texts with line breaks at the cell's width: (revision, columns, text)
    wrapped: HashMap<CellId, (u64, usize, Arc<str>)>,
    /// the time of this frame (seconds)
    now: f64,
    /// the window's atlas of math-font glyphs
    glyphs: Rc<RefCell<Glyphs>>,
    /// box layouts of typeset cells: (revision, width, zoom) and the layout
    boxes: HashMap<CellId, (LayoutKey, Arc<Layout>)>,
    /// the colours of the characters of typeset input cells, per layout row and item (design 3.11 in boxes)
    box_colours: HashMap<CellId, (ColourKey, Colours)>,
    /// the worker typesetting large outputs, and the layouts asked of it
    background: Option<Background>,
    pending: HashMap<CellId, LayoutKey>,
    /// the context of the last frame (background jobs measure fallback characters with it)
    ctx: Option<egui::Context>,
}

impl View {
    /// An empty view drawing pictures from `images`.
    pub fn new(palette: Palette, images: Rc<RefCell<Images>>, glyphs: Rc<RefCell<Glyphs>>) -> View {
        View {
            laid: HashMap::new(),
            hold: false,
            outputs_right: false,
            tops: vec![],
            heights: vec![],
            zoom: PX_PER_PT,
            width: 800.0,
            origin: Pos2::ZERO,
            palette,
            blink_from: 0.0,
            blink: true,
            hover: None,
            drop_gap: None,
            plus_hover: false,
            max_col: 0,
            caret: None,
            defined: None,
            defined_rev: 0,
            colours: HashMap::new(),
            images,
            base: None,
            graphic: None,
            live_size: None,
            live_view: None,
            rot: None,
            live_anchor: None,
            budgets: HashMap::new(),
            large: HashMap::new(),
            manip: HashMap::new(),
            long: HashMap::new(),
            wrapped: HashMap::new(),
            now: 0.0,
            glyphs,
            boxes: HashMap::new(),
            box_colours: HashMap::new(),
            background: None,
            pending: HashMap::new(),
            ctx: None,
        }
    }

    /// The kernel's defined names changed.
    pub fn set_defined(&mut self, names: Option<HashSet<String>>) {
        self.defined = names.map(Arc::new);
        self.defined_rev += 1;
        self.laid.clear();
    }

    /// Forget every layout (a theme or font change).
    pub fn clear(&mut self) {
        self.laid.clear();
        self.long.clear();
        self.wrapped.clear();
        // box layouts depend on neither the theme nor egui's fonts (they are keyed by revision, width and zoom):
        // kept, so large outputs do not fall back to linear text while the worker typesets them again
    }

    /// What cell `c` shows.
    fn shown(&mut self, c: &Cell) -> Shown {
        if let Some(p) = c.graphic() {
            return Shown::Graphic(mc_nbfile::images::resolve(&p, self.base.as_deref()));
        }
        if mc_nbmodel::is_boxed(c) {
            return Shown::Boxes;
        }
        if c.style == Style::Output && !c.failure && !c.expr_view && c.text.starts_with("manipulate(") {
            let n = self.controls_of(c).len();
            if n > 0 {
                return Shown::Controls;
            }
        }
        if self.typesets(c) && (c.text.len() <= TYPESET_SYNC || self.output_ready(c)) {
            return Shown::Typeset;
        }
        if matches!(c.style, Style::Output | Style::Print) && !c.expr_view && c.text.len() > printed::LARGE_OUTPUT {
            let large = match self.large.get(&c.id) {
                Some(&(rev, l)) if rev == c.rev => l,
                _ => {
                    let l = printed::is_large(&c.text);
                    self.large.insert(c.id, (c.rev, l));
                    l
                }
            };
            if large {
                return Shown::Large;
            }
        }
        Shown::Text
    }

    /// Whether an Output cell is drawn with the cell renderer (its printed text with its square roots, fractions and
    /// powers as boxes, NBUI.5): every output except a failure, a picture, one longer than [`TYPESET_LIMIT`], one
    /// showing its source, or a printed form of several lines (a dataset's table, laid out by its own columns).
    fn typesets(&self, c: &Cell) -> bool {
        c.style == Style::Output
            && !c.failure
            && !c.expr_view
            && c.text.len() <= TYPESET_LIMIT
            && !c.text.trim_end().contains('\n')
    }

    /// The key of cell `c`'s box layout at the current width and zoom.
    fn box_key(&self, c: &Cell) -> LayoutKey {
        let (l, r) = self.content_x(c.style);
        (c.rev, (r - l).to_bits(), self.zoom.to_bits(), self.font_gen())
    }

    /// The generation of the window's fonts (typeset layouts and cell heights depend on them).
    fn font_gen(&self) -> u32 {
        self.glyphs.borrow().face(mc_ui::fonts::Face::Code).1
    }

    /// The text font of cell `c` as its typeset content uses it (NBUI.2).
    fn text_font(&self, ctx: &egui::Context, c: &Cell) -> typeset::TextFont {
        let lk = look(c, &self.palette);
        let (fonts, _) = self.glyphs.borrow().face(lk.face);
        typeset::TextFont::new(ctx, lk.face, fonts, lk.size * self.zoom)
    }

    /// How far a typeset cell's scripts may reach into its top and bottom margins (points on the screen): most of
    /// each margin, at most half an em above and a third below.
    fn script_reach(&self, c: &Cell) -> (f32, f32) {
        let m = c.style.info().margins;
        let size = look(c, &self.palette).size * self.zoom;
        ((m[2] * self.zoom * 0.8).min(0.5 * size), (m[3] * self.zoom * 0.8).min(size / 3.0))
    }

    /// Output cells are right-justified when the alignment is Right (NBUI.1; View > Output alignment): each line
    /// ends at the content's right edge. Print and Message cells (program output, read as a log) and an output
    /// showing its source stay left-justified. With Left (the default since round 13, user decision) every cell
    /// starts at the content's left edge.
    fn right_justified(&self, c: &Cell) -> bool {
        self.outputs_right && c.style == Style::Output && !c.expr_view
    }

    /// Put outputs on the right edge (`true`) or the left: every layout is made again.
    pub fn set_outputs_right(&mut self, on: bool) {
        if self.outputs_right == on {
            return;
        }
        self.outputs_right = on;
        self.laid.clear();
        self.long.clear();
        self.wrapped.clear();
        self.boxes.clear();
        self.pending.clear();
    }

    /// Where the galley of a cell shown as text is anchored, from the content's left edge: right-justified text is
    /// laid out right-aligned at the content's right edge (egui's rows then end at x = 0 of the galley).
    fn text_dx(&self, c: &Cell) -> f32 {
        if self.right_justified(c) {
            let (l, r) = self.content_x(c.style);
            r - l
        } else {
            0.0
        }
    }

    /// The options of cell `c`'s box layout.
    fn box_options(&self, c: &Cell) -> LayoutOptions {
        let (l, r) = self.content_x(c.style);
        let lk = look(c, &self.palette);
        let width = Some((r - l).max(40.0));
        if c.style == Style::Output {
            // the printed text as it is (no spacing added around operators), lines not indented
            LayoutOptions { operator_spacing: false, small_inline_rationals: false, ..LayoutOptions::output(lk.size * self.zoom, width) }
        } else {
            // no thin spaces around operators and no `->` drawn as one arrow: the characters where plain text has
            // them (NBUI.3)
            LayoutOptions { operator_spacing: false, small_inline_rationals: false, ..LayoutOptions::input(lk.size * self.zoom, width) }
        }
    }

    /// A large output's layout is there (or one at another width, shown until the new one arrives); otherwise it is
    /// asked of the worker and the output shows as linear text meanwhile.
    fn output_ready(&mut self, c: &Cell) -> bool {
        let key = self.box_key(c);
        let usable = |k: &LayoutKey| k.0 == key.0 && k.2 == key.2 && k.3 == key.3;
        let have = self.boxes.get(&c.id).map(|(k, _)| *k);
        if have == Some(key) {
            return true;
        }
        if self.pending.get(&c.id) != Some(&key) {
            if let Some(ctx) = self.ctx.clone() {
                let (l, r) = self.content_x(c.style);
                let job = typeset::Job {
                    id: c.id,
                    key,
                    text: c.text.clone(),
                    opts: self.box_options(c),
                    font: self.text_font(&ctx, c),
                    right: self.right_justified(c).then_some(r - l),
                    reach: self.script_reach(c),
                };
                self.background.get_or_insert_with(|| Background::new(&ctx)).submit(job);
                self.pending.insert(c.id, key);
            }
        }
        have.is_some_and(|k| usable(&k))
    }

    /// Take the layouts the worker finished.
    fn take_background(&mut self, nb: &Notebook) {
        let Some(bg) = &self.background else { return };
        for d in bg.results() {
            if self.pending.get(&d.id) == Some(&d.key) {
                self.pending.remove(&d.id);
            }
            if nb.cell(d.id).is_some_and(|c| c.rev == d.key.0) {
                self.boxes.insert(d.id, (d.key, d.layout));
                // its height changes from the linear text's
                self.laid.remove(&d.id);
            }
        }
    }

    /// Whether cell `c` is a typeset output (one unit for the caret: before it or after it).
    pub fn shows_typeset_output(&mut self, c: &Cell) -> bool {
        matches!(self.shown(c), Shown::Typeset)
    }

    /// Whether cell `c` shows its text (rather than a picture or a large output's sample), so a caret and text
    /// selections can be drawn in it.
    pub fn shows_text(&mut self, c: &Cell) -> bool {
        matches!(self.shown(c), Shown::Text)
    }

    /// The font of a cell's text.
    fn font(&self, c: &Cell) -> egui::FontId {
        let lk = look(c, &self.palette);
        lk.face.font(lk.size * self.zoom)
    }

    /// The characters of the code font that fit across cell `c`'s content.
    fn columns(&self, ctx: &egui::Context, c: &Cell) -> usize {
        let (l, r) = self.content_x(c.style);
        let font = self.font(c);
        let w = ctx.fonts_mut(|f| f.glyph_width(&font, '0')).max(1.0);
        ((r - l) / w).floor().max(8.0) as usize
    }

    /// The text an Output cell is laid out from: its text with line breaks at the cell's width on the spaces the
    /// penalties choose (design 4.8), the same length as the text, so positions carry over.
    fn output_text(&mut self, ctx: &egui::Context, c: &Cell) -> Arc<str> {
        let cols = self.columns(ctx, c);
        if let Some((rev, k, t)) = self.wrapped.get(&c.id) {
            if *rev == c.rev && *k == cols {
                return t.clone();
            }
        }
        let t: Arc<str> = Arc::from(printed::wrapped(&c.text, cols));
        self.wrapped.insert(c.id, (c.rev, cols, t.clone()));
        t
    }

    /// The size (points on screen) a picture is drawn at: the size being dragged, else its `size=`, else its own
    /// size, times the magnification; never wider than the cell. `live` takes the size being dragged into account.
    fn graphic_size(&mut self, c: &Cell, path: &Path, live: bool) -> Vec2 {
        let m = self.magnification();
        let (l, r) = self.content_x(c.style);
        let max_w = (r - l).max(40.0);
        let want = match (self.live_size.filter(|(id, _)| live && *id == c.id), c.graphic_size()) {
            (Some((_, s)), _) => s,
            (None, Some((w, h))) => vec2(w as f32, h as f32) * m,
            (None, None) => match self.images.borrow_mut().natural(path, self.now) {
                Some((w, h)) => vec2(w as f32, h as f32) * m,
                // a frame for the path of a missing picture
                None => return vec2(max_w.min(360.0 * m), 40.0 * m),
            },
        };
        if want.x > max_w {
            vec2(max_w, want.y * max_w / want.x)
        } else {
            want
        }
    }

    /// The box layout of a typeset cell at the cell's width (cached by revision, width and magnification).
    pub fn box_layout(&mut self, ctx: &egui::Context, c: &Cell) -> Arc<Layout> {
        let key = self.box_key(c);
        let large = c.style == Style::Output && c.text.len() > TYPESET_SYNC;
        if let Some((k, lay)) = self.boxes.get(&c.id) {
            // a large output keeps its layout at another width until the worker's arrives
            if *k == key || large && k.0 == key.0 && k.2 == key.2 && k.3 == key.3 {
                return lay.clone();
            }
        }
        let opts = self.box_options(c);
        let row = if c.style == Style::Output {
            mc_boxes::math_text(&c.text).row
        } else {
            mc_boxes::from_text(&c.text)
        };
        let font = self.text_font(ctx, c);
        let mut lay = typeset::lay_out(&row, &opts, &font);
        let (above, below) = self.script_reach(c);
        typeset::keep_baseline(&mut lay, &font, opts.size, above, below);
        if self.right_justified(c) {
            let (l, r) = self.content_x(c.style);
            lay.align_right(r - l);
        }
        let lay = Arc::new(lay);
        self.boxes.insert(c.id, (key, lay.clone()));
        lay
    }

    /// The caret position in the boxes of cell `id` nearest to `p` (canvas coordinates); above the boxes the start,
    /// below them the end of the cell's boxes (a drag past a cell's edge selects to its start or end).
    pub fn box_hit(&mut self, ctx: &egui::Context, nb: &Notebook, id: CellId, p: Pos2) -> Option<(RowPath, usize)> {
        let i = nb.index_of(id)?;
        let lay = self.box_layout(ctx, &nb.cells()[i]);
        let o = self.content_origin(nb, i);
        let (x, y) = (p.x - o.x, p.y - o.y);
        let first = lay.lines.first()?;
        let last = lay.lines.last()?;
        if y < first.top.min(0.0) {
            return Some((vec![], first.start));
        }
        if y > last.bottom.max(lay.height) {
            return Some((vec![], last.end));
        }
        Some(lay.hit(x, y))
    }

    /// The items of the visual line of cell `id`'s boxes at `p` (canvas coordinates), its line break excluded (a
    /// triple-click selects them).
    pub fn box_line_items(&mut self, ctx: &egui::Context, nb: &Notebook, id: CellId, p: Pos2) -> Option<Range<usize>> {
        let i = nb.index_of(id)?;
        let c = &nb.cells()[i];
        let lay = self.box_layout(ctx, c);
        let o = self.content_origin(nb, i);
        let l = *lay.lines.get(lay.line_at(p.y - o.y))?;
        Some(l.start..self.line_end(c, l.start, l.end))
    }

    /// The end of a root line `start..end` without its line break.
    fn line_end(&self, c: &Cell, start: usize, end: usize) -> usize {
        let row = if c.style == Style::Output {
            mc_boxes::math_text(&c.text).row
        } else {
            mc_boxes::from_text(&c.text)
        };
        if end > start && matches!(row.0.get(end - 1), Some(mc_boxes::Item::Char('\n'))) {
            end - 1
        } else {
            end
        }
    }

    /// The start or the end (before its line break) of the visual line holding offset `offset` of the root row of
    /// a typeset cell (Home and End).
    pub fn box_line_edge(&mut self, ctx: &egui::Context, c: &Cell, offset: usize, end: bool) -> Option<usize> {
        let lay = self.box_layout(ctx, c);
        // a line's end offset is the next line's start, except on the last line
        let l = *lay.lines.iter().find(|l| offset >= l.start && offset < l.end).or(lay.lines.last())?;
        Some(if end { self.line_end(c, l.start, l.end) } else { l.start })
    }

    /// The caret position on the visual line above or below a caret in a typeset cell's boxes.
    pub fn box_line(&mut self, ctx: &egui::Context, c: &Cell, path: &[mc_boxes::Step], offset: usize, up: bool) -> Option<(RowPath, usize)> {
        let lay = self.box_layout(ctx, c);
        let s = lay.caret(path, offset)?;
        let k = lay.lines.iter().position(|ln| s.baseline <= ln.bottom + 0.01)?;
        let target = if up { lay.lines.get(k.checked_sub(1)?)? } else { lay.lines.get(k + 1)? };
        Some(lay.hit(s.x, target.baseline))
    }

    /// The file of cell `i`'s picture.
    pub fn graphic_path(&mut self, nb: &Notebook, i: usize) -> Option<PathBuf> {
        match self.shown(&nb.cells()[i]) {
            Shown::Graphic(p) => Some(p),
            _ => None,
        }
    }

    /// The rectangle (canvas coordinates) of cell `i`'s picture.
    pub fn graphic_rect(&mut self, nb: &Notebook, i: usize) -> Option<Rect> {
        let c = &nb.cells()[i];
        let Shown::Graphic(path) = self.shown(c) else { return None };
        let size = self.graphic_size(c, &path, true);
        let mut o = self.content_origin(nb, i);
        if self.right_justified(c) {
            let (l, r) = self.content_x(c.style);
            o.x += (r - l - size.x).max(0.0);
            // during a resize the edge opposite the dragged handle stays where it was (within the content)
            if let (Some((id, _)), Some((x, side))) = (self.live_size, self.live_anchor) {
                if id == c.id {
                    let left = if side < 0 { x - size.x } else { x };
                    o.x = left.clamp(l, (r - size.x).max(l));
                }
            }
        }
        Some(Rect::from_min_size(o, size))
    }


    /// The controls of a `manipulate` output (parsed once per revision).
    fn controls_of(&mut self, c: &Cell) -> Vec<mc_nbmodel::Control> {
        if let Some((rev, m)) = self.manip.get(&c.id) {
            if *rev == c.rev {
                return m.clone();
            }
        }
        let m = mc_nbmodel::Manipulate::parse(&c.text).map(|m| m.controls).unwrap_or_default();
        self.manip.insert(c.id, (c.rev, m.clone()));
        m
    }

    /// The height of one control's row.
    pub fn control_row(&self) -> f32 {
        26.0 * self.magnification().max(0.5)
    }

    /// The slider track of control `k` of the `manipulate` output in cell `i`: its left and right ends (canvas x)
    /// and its middle (canvas y).
    pub fn control_track(&mut self, nb: &Notebook, i: usize, k: usize) -> Option<(f32, f32, f32)> {
        let c = &nb.cells()[i];
        let controls = self.controls_of(c);
        let ctl = controls.get(k)?;
        let o = self.content_origin(nb, i);
        let (l, r) = self.content_x(c.style);
        let m = self.magnification().max(0.5);
        let name_w = controls.iter().map(|c| c.name.chars().count()).max().unwrap_or(1) as f32 * 8.0 * m;
        let x0 = o.x + name_w + 14.0 * m;
        let x1 = (x0 + 300.0 * m).min(o.x + (r - l) - 90.0 * m).max(x0 + 40.0);
        let _ = ctl;
        Some((x0, x1, o.y + (k as f32 + 0.5) * self.control_row()))
    }

    /// The value at canvas x `x` on the slider of control `k` (snapped to its steps).
    pub fn control_value_at(&mut self, nb: &Notebook, i: usize, k: usize, x: f32) -> Option<f64> {
        let (x0, x1, _) = self.control_track(nb, i, k)?;
        let ctl = self.controls_of(&nb.cells()[i]).get(k)?.clone();
        let t = f64::from(((x - x0) / (x1 - x0)).clamp(0.0, 1.0));
        Some(ctl.snap(ctl.lo + t * (ctl.hi - ctl.lo)))
    }

    fn paint_controls(&mut self, ctx: &egui::Context, painter: &Painter, nb: &Notebook, i: usize, content: Pos2) {
        let p = self.palette.clone();
        let c = &nb.cells()[i];
        let Some((_, values)) = nb.manipulate_of(c.id) else { return };
        let controls = self.controls_of(c);
        let shift = content - self.content_origin(nb, i);
        let m = self.magnification().max(0.5);
        let font = self.font(c);
        let ui_font = Face::Ui.font(12.0 * m);
        for (k, ctl) in controls.iter().enumerate() {
            let Some((x0, x1, y)) = self.control_track(nb, i, k) else { continue };
            let (x0, x1, y) = (x0 + shift.x, x1 + shift.x, y + shift.y);
            let v = values.get(k).copied().unwrap_or(ctl.init);
            let t = ((v - ctl.lo) / (ctl.hi - ctl.lo)).clamp(0.0, 1.0) as f32;
            let name = painter.layout_no_wrap(ctl.name.clone(), font.clone(), p.text);
            painter.galley(pos2(content.x, y - name.size().y / 2.0), name, p.text);
            // the track, the part up to the knob in the accent colour, ticks at the steps when there are few
            let w = (3.0 * m).round().max(2.0);
            let track = Rect::from_min_max(pos2(x0, y - w / 2.0), pos2(x1, y + w / 2.0));
            painter.rect_filled(track, w / 2.0, p.border);
            let kx = x0 + t * (x1 - x0);
            painter.rect_filled(Rect::from_min_max(track.min, pos2(kx, track.max.y)), w / 2.0, p.accent);
            if let Some(s) = ctl.step {
                let n = ((ctl.hi - ctl.lo) / s).round();
                if (1.0..=40.0).contains(&n) {
                    for j in 0..=(n as usize) {
                        let tx = x0 + (j as f32 / n as f32) * (x1 - x0);
                        painter.vline(tx, (y + 4.0 * m)..=(y + 7.0 * m), Stroke::new(1.0, p.border));
                    }
                }
            }
            painter.circle(pos2(kx, y), 6.5 * m, p.bg, Stroke::new(1.5 * m, p.accent));
            let value = painter.layout_no_wrap(ctl.text(v), ui_font.clone(), p.weak);
            painter.galley(pos2(x1 + 10.0 * m, y - value.size().y / 2.0), value, p.weak);
        }
        let _ = ctx;
    }

    /// Row height, the summary's title height and the buttons' height of a large output.
    fn long_metrics(&self, ctx: &egui::Context, c: &Cell) -> (f32, f32, f32) {
        let font = self.font(c);
        let row = ctx.fonts_mut(|f| f.row_height(&font));
        let ui = ctx.fonts_mut(|f| f.row_height(&Face::Ui.font(12.0 * self.magnification().max(0.75))));
        (row, ui + 6.0, ui + 8.0)
    }

    /// A large output as shown: its sample at the size the buttons chose (or all of it), broken into lines.
    fn long(&mut self, ctx: &egui::Context, c: &Cell) -> Arc<Long> {
        let budget = self.budgets.get(&c.id).copied().unwrap_or(printed::SAMPLE);
        let cols = self.columns(ctx, c);
        let key = (c.rev, cols, budget);
        if let Some(l) = self.long.get(&c.id).filter(|l| l.key == key) {
            return l.clone();
        }
        let chars = c.text.chars().count();
        let text: Arc<str> =
            if budget >= chars { Arc::from(c.text.as_str()) } else { Arc::from(printed::sample(&c.text, budget)) };
        let rows = printed::wrap(&text, cols);
        let widest = rows.iter().map(|r| text[r.clone()].trim_end().chars().count()).max().unwrap_or(0);
        let l = Arc::new(Long { key, text, rows, chars, widest });
        self.long.insert(c.id, l.clone());
        l
    }

    /// The buttons under a large output (rectangles relative to the content's top-left).
    fn large_buttons(&mut self, ctx: &egui::Context, c: &Cell) -> Vec<(Rect, LargeAction, &'static str)> {
        let l = self.long(ctx, c);
        let (row, title, buttons) = self.long_metrics(ctx, c);
        let all = l.text.len() == c.text.len();
        let budget = self.budgets.get(&c.id).copied().unwrap_or(printed::SAMPLE);
        let list: Vec<(LargeAction, &'static str)> = if all {
            vec![(LargeAction::Sample, "Show less")]
        } else {
            let mut v = vec![];
            if budget > printed::SAMPLE / 4 {
                v.push((LargeAction::Less, "Show less"));
            }
            v.extend([(LargeAction::More, "Show more"), (LargeAction::All, "Show all")]);
            v
        };
        let font = Face::Ui.font(12.0 * self.magnification().max(0.75));
        let y = title + l.rows.len() as f32 * row + 4.0;
        let mut x = 0.0;
        let mut out: Vec<(Rect, LargeAction, &'static str)> = list
            .into_iter()
            .map(|(a, label)| {
                let w = ctx.fonts_mut(|f| f.layout_no_wrap(label.to_string(), font.clone(), Color32::WHITE)).size().x;
                let r = Rect::from_min_size(pos2(x, y), vec2(w + 12.0, buttons - 4.0));
                x += w + 20.0;
                (r, a, label)
            })
            .collect();
        // an output's buttons end at the right edge, under its right-justified sample
        if self.right_justified(c) {
            let end = out.last().map_or(0.0, |(r, _, _)| r.right());
            let dx = (self.text_dx(c) - end).max(0.0);
            for (r, _, _) in &mut out {
                *r = r.translate(vec2(dx, 0.0));
            }
        }
        out
    }

    /// Act on a large output's button.
    pub fn large_action(&mut self, id: CellId, a: LargeAction) {
        let b = self.budgets.get(&id).copied().unwrap_or(printed::SAMPLE);
        let b = match a {
            LargeAction::Less => (b / 2).max(printed::SAMPLE / 4),
            LargeAction::More => b.saturating_mul(2),
            LargeAction::All => usize::MAX,
            LargeAction::Sample => printed::SAMPLE,
        };
        self.budgets.insert(id, b);
        self.laid.remove(&id);
    }

    /// The height of cell `c`'s content, and it is remembered for the cell positions.
    fn measure(&mut self, ctx: &egui::Context, c: &Cell) -> f32 {
        let height = match self.shown(c) {
            Shown::Text => return self.galley(ctx, c).size().y,
            Shown::Graphic(path) => self.graphic_size(c, &path, true).y,
            Shown::Large => {
                let l = self.long(ctx, c);
                let (row, title, buttons) = self.long_metrics(ctx, c);
                title + l.rows.len() as f32 * row + buttons
            }
            Shown::Boxes | Shown::Typeset => self.box_layout(ctx, c).height,
            Shown::Controls => self.controls_of(c).len() as f32 * self.control_row(),
        };
        let key = self.layout_key(c);
        self.laid.insert(c.id, Laid { key, height });
        height
    }

    fn layout_key(&self, c: &Cell) -> (u64, u32, u32, bool, u32) {
        let (l, r) = self.content_x(c.style);
        (c.rev, (r - l).to_bits(), self.zoom.to_bits(), self.palette.dark, self.font_gen())
    }

    /// The bracket columns shown: the deepest group's column, at least [`MIN_COLUMNS`].
    fn columns_shown(&self) -> usize {
        self.max_col.max(MIN_COLUMNS)
    }

    /// The x (relative to the canvas) of bracket column `col`: the brackets are on the left, cell brackets (column
    /// 0) nearest the content, enclosing groups further out.
    fn bracket_x(&self, col: usize) -> f32 {
        BRACKET_LEFT + BRACKET_PITCH * self.columns_shown().saturating_sub(col) as f32
    }

    /// The left edge of the content of a style at margin 0: right of the innermost bracket and its hooks.
    fn content_edge(&self) -> f32 {
        self.bracket_x(0) + HOOK + BRACKET_GAP * self.zoom / PX_PER_PT
    }

    /// The left and right edges of a cell's content (relative to the canvas).
    pub fn content_x(&self, style: Style) -> (f32, f32) {
        let m = style.info().margins;
        let left = self.content_edge() + (m[0] - BASE_MARGIN).max(0.0) * self.zoom;
        let right = (self.width - RIGHT_PAD - m[1].min(10.0)).max(left + 40.0);
        (left, right)
    }

    /// The layout of a cell's content at the current width (egui caches it).
    pub fn galley(&mut self, ctx: &egui::Context, cell: &Cell) -> Arc<Galley> {
        let (l, r) = self.content_x(cell.style);
        let wrap = r - l;
        let key = self.layout_key(cell);
        let lk = look(cell, &self.palette);
        let mut job = LayoutJob::default();
        let format =
            TextFormat { font_id: lk.face.font(lk.size * self.zoom), color: lk.color, ..TextFormat::default() };
        if cell.style.is_code() && !cell.expr_view {
            self.append_code(&mut job, cell, &format);
        } else if cell.style == Style::Output && !cell.expr_view {
            let text = self.output_text(ctx, cell);
            job.append(&text, 0.0, format);
        } else {
            job.append(&cell.text, 0.0, format);
        }
        job.wrap.max_width = wrap;
        job.wrap.break_anywhere = false;
        if self.right_justified(cell) {
            // each row ends at the galley's x = 0, drawn at the content's right edge (`text_dx`)
            job.halign = egui::Align::RIGHT;
        }
        let galley = ctx.fonts_mut(|f| f.layout_job(job));
        self.laid.insert(cell.id, Laid { key, height: galley.size().y });
        galley
    }

    /// The colouring of a cell, cached by revision (colouring is linear but not incremental).
    pub fn colouring(&mut self, cell: &Cell) -> Arc<Colouring> {
        let key = (cell.rev, self.defined_rev);
        if let Some((k, c)) = self.colours.get(&cell.id) {
            if *k == key {
                return c.clone();
            }
        }
        let defined = self.defined.clone();
        let check = move |n: &str| defined.as_ref().is_none_or(|d| d.contains(n));
        let c = Arc::new(mc_syntax::colour(&cell.text, Some(&check)));
        self.colours.insert(cell.id, (key, c.clone()));
        c
    }

    /// Append coloured source: the classes of design 3.11 in the roles of 8.2, the brackets matching at the caret.
    fn append_code(&mut self, job: &mut LayoutJob, cell: &Cell, base: &TextFormat) {
        let p = self.palette.clone();
        let text = &cell.text;
        let colouring = self.colouring(cell);
        let matched = self.caret.filter(|c| c.0 == cell.id).and_then(|c| colouring.matching_bracket(c.1));
        // per byte boundary: the class run, the alias runs, the matched brackets
        let mut cuts: Vec<usize> = vec![0, text.len()];
        for (r, _) in &colouring.tokens {
            cuts.extend([r.start, r.end]);
        }
        let aliases = highlight::alias_runs(text);
        for r in &aliases {
            cuts.extend([r.start, r.end]);
        }
        let marks: Vec<usize> = matched.map(|(a, b)| vec![a, b]).unwrap_or_default();
        for &m in &marks {
            cuts.extend([m, m + 1]);
        }
        cuts.retain(|&c| c <= text.len() && text.is_char_boundary(c));
        cuts.sort_unstable();
        cuts.dedup();
        if text.is_empty() {
            // an empty cell still has one line of its font's height (and a caret of that height)
            job.append("", 0.0, base.clone());
            return;
        }
        let bold_font = Face::CodeBold.font(base.font_id.size);
        for w in cuts.windows(2) {
            let (a, b) = (w[0], w[1]);
            let mut f = base.clone();
            if let Some(class) = colouring.class_at(a) {
                let (color, italic, bold, bg) = highlight::style(class, &p);
                f.color = color;
                f.italics = italic;
                if bold {
                    f.font_id = bold_font.clone();
                }
                if let Some(bg) = bg {
                    f.background = bg;
                }
            }
            if aliases.iter().any(|r| r.contains(&a)) {
                f.color = p.purple;
            }
            if marks.contains(&a) && b == a + 1 {
                f.background = p.over_bg(p.accent, 0.25);
            }
            job.append(&text[a..b], 0.0, f);
        }
    }

    fn cached_height(&self, cell: &Cell) -> Option<f32> {
        if self.live_size.is_some_and(|(id, _)| id == cell.id) {
            return None;
        }
        let key = self.layout_key(cell);
        self.laid.get(&cell.id).filter(|l| l.key == key).map(|l| l.height)
    }

    fn cell_height(&self, nb: &Notebook, i: usize, content: f32) -> f32 {
        let c = &nb.cells()[i];
        if !nb.is_visible(i) {
            return 0.0;
        }
        if c.props.hidden {
            return 8.0 * self.zoom;
        }
        let m = c.style.info().margins;
        (m[2] + m[3]) * self.zoom + content
    }

    /// Recompute cell positions: exact heights for laid-out cells, estimates for the rest; lay out the cells in
    /// `visible` (canvas coordinates) and a few more.
    pub fn update_positions(&mut self, ctx: &egui::Context, nb: &Notebook, visible: Range<f32>) {
        let n = nb.cells().len();
        self.now = ctx.input(|i| i.time);
        self.ctx = Some(ctx.clone());
        self.take_background(nb);
        self.forget_removed(nb);
        self.max_col = nb.groups().list.iter().map(|g| g.column).max().unwrap_or(0);
        self.tops.resize(n, 0.0);
        self.heights.resize(n, 0.0);
        let mut y = TOP_PAD * self.zoom;
        let mut refined = 0;
        let start = std::time::Instant::now();
        for i in 0..n {
            let c = &nb.cells()[i];
            let content = if !nb.is_visible(i) || c.props.hidden {
                0.0
            } else if let Some(h) = self.cached_height(c) {
                h
            } else if y < visible.end + 200.0 && y + 400.0 > visible.start
                || !self.hold && refined < REFINE_PER_FRAME && start.elapsed().as_secs_f64() < REFINE_BUDGET
            {
                refined += 1;
                self.measure(ctx, c)
            } else {
                let lines = c.text.split('\n').count() as f32;
                lines * look(c, &self.palette).size * self.zoom * 1.25
            };
            self.tops[i] = y;
            self.heights[i] = self.cell_height(nb, i, content);
            y += self.heights[i];
        }
        if refined >= REFINE_PER_FRAME || start.elapsed().as_secs_f64() >= REFINE_BUDGET {
            ctx.request_repaint();
        }
    }

    /// Drop what the caches hold for cells that are gone (re-evaluation replaces output cells, and their texts can
    /// be large), once the caches hold clearly more entries than the notebook has cells.
    fn forget_removed(&mut self, nb: &Notebook) {
        let n = nb.cells().len();
        let held = self.laid.len() + self.colours.len() + self.wrapped.len() + self.long.len() + self.large.len();
        if held <= 4 * n + 64 {
            return;
        }
        let ids: HashSet<CellId> = nb.cells().iter().map(|c| c.id).collect();
        self.laid.retain(|id, _| ids.contains(id));
        self.colours.retain(|id, _| ids.contains(id));
        self.wrapped.retain(|id, _| ids.contains(id));
        self.long.retain(|id, _| ids.contains(id));
        self.boxes.retain(|id, _| ids.contains(id));
        self.pending.retain(|id, _| ids.contains(id));
        self.large.retain(|id, _| ids.contains(id));
        self.budgets.retain(|id, _| ids.contains(id));
    }

    /// The total height of the cells.
    pub fn total_height(&self) -> f32 {
        self.tops.last().zip(self.heights.last()).map_or(TOP_PAD, |(t, h)| t + h)
    }

    /// The y (canvas coordinates) of the insertion point before cell `g`.
    pub fn gap_y(&self, g: usize) -> f32 {
        if g < self.tops.len() {
            self.tops[g]
        } else {
            self.total_height()
        }
    }

    /// The rectangle (canvas coordinates) of cell `i`.
    pub fn cell_rect(&self, i: usize) -> Rect {
        Rect::from_min_size(pos2(0.0, self.tops[i]), vec2(self.width, self.heights[i]))
    }

    /// The top-left of cell `i`'s content (canvas coordinates).
    pub fn content_origin(&self, nb: &Notebook, i: usize) -> Pos2 {
        let c = &nb.cells()[i];
        let (l, _) = self.content_x(c.style);
        pos2(l, self.tops[i] + c.style.info().margins[2] * self.zoom)
    }

    /// The rectangle (canvas coordinates) of the caret or of a position in a cell.
    pub fn caret_rect(&mut self, ctx: &egui::Context, nb: &Notebook, id: CellId, pos: usize) -> Option<Rect> {
        let i = nb.index_of(id)?;
        let c = &nb.cells()[i];
        match self.shown(c) {
            Shown::Text => {}
            Shown::Graphic(_) => {
                // beside the picture, its height
                let r = self.graphic_rect(nb, i)?;
                let x = if pos == 0 { r.left() - 3.0 } else { r.right() + 3.0 };
                return Some(Rect::from_min_max(pos2(x, r.top()), pos2(x, r.bottom())));
            }
            Shown::Large => {
                let o = self.content_origin(nb, i);
                let (row, _, _) = self.long_metrics(ctx, c);
                return Some(Rect::from_min_size(o, vec2(0.0, row)));
            }
            Shown::Controls => {
                let o = self.content_origin(nb, i);
                return Some(Rect::from_min_size(o, vec2(0.0, self.control_row())));
            }
            Shown::Boxes | Shown::Typeset => {
                let o = self.content_origin(nb, i);
                let lay = self.box_layout(ctx, c);
                let editor = nb.box_editor().or(nb.output_editor()).filter(|(b, _)| *b == id);
                let stop = match editor {
                    Some((_, ed)) => lay.caret(&ed.caret().path, ed.caret().offset),
                    None if c.style == Style::Output => {
                        return Some(self.output_caret(ctx, c, pos).translate(o.to_vec2()));
                    }
                    None => None,
                };
                let r = stop.map_or(Rect::from_min_size(pos2(0.0, 0.0), vec2(0.0, lay.height)), |s| {
                    Rect::from_min_max(pos2(s.x, s.top), pos2(s.x, s.bottom))
                });
                return Some(r.translate(o.to_vec2()));
            }
        }
        let g = self.galley(ctx, c);
        let chars = c.text[..pos.min(c.text.len())].chars().count();
        let r = g.pos_from_cursor(CCursor::new(chars));
        Some(r.translate(self.content_origin(nb, i).to_vec2() + vec2(self.text_dx(c), 0.0)))
    }

    /// The bracket columns of the notebook: (column, top, bottom, bracket) for every cell and group that shows.
    fn brackets(&self, nb: &Notebook) -> Vec<(usize, f32, f32, Bracket)> {
        let mut out = vec![];
        for (i, c) in nb.cells().iter().enumerate() {
            if nb.is_visible(i) {
                out.push((0, self.tops[i], self.tops[i] + self.heights[i], Bracket::Cell(c.id)));
            }
        }
        for g in &nb.groups().list {
            let first = (g.start..g.end).find(|&i| nb.is_visible(i));
            let last = (g.start..g.end).rev().find(|&i| nb.is_visible(i));
            if let (Some(a), Some(b)) = (first, last) {
                out.push((g.column, self.tops[a], self.tops[b] + self.heights[b], Bracket::Group(g.start, g.end)));
            }
        }
        out
    }

    /// What is at `p` (canvas coordinates).
    pub fn hit(&mut self, ctx: &egui::Context, nb: &Notebook, p: Pos2) -> Hit {
        let n = nb.cells().len();
        if let Selection::Gap(g) = nb.selection() {
            if (p - self.plus_center(*g)).length() <= 8.0 * self.zoom.max(1.0) {
                return Hit::Plus(*g);
            }
        }
        let inner = self.bracket_x(0);
        if p.x <= inner + HOOK + 3.0 {
            let col = ((inner - p.x) / BRACKET_PITCH).round().max(0.0) as usize;
            let mut best: Option<(usize, Bracket)> = None;
            for (c, top, bottom, b) in self.brackets(nb) {
                if p.y >= top && p.y < bottom {
                    let d = c.abs_diff(col);
                    if best.is_none_or(|(bd, _)| d < bd) {
                        best = Some((d, b));
                    }
                }
            }
            if let Some((_, b)) = best {
                return Hit::Bracket(b);
            }
        }
        // the handles of the selected picture reach a little outside it
        if let Some((id, r)) = self.graphic.and_then(|id| Some((id, self.graphic_rect(nb, nb.index_of(id)?)?))) {
            if let Some(h) = Handle::ALL.into_iter().find(|h| (h.at(r) - p).abs().max_elem() <= HANDLE_REACH) {
                return Hit::Handle(id, h);
            }
        }
        // the visible cell under the point
        let i = (0..n).find(|&i| nb.is_visible(i) && p.y >= self.tops[i] && p.y < self.tops[i] + self.heights[i]);
        let Some(i) = i else {
            return Hit::Gap(if p.y < TOP_PAD * self.zoom { 0 } else { n });
        };
        let c = &nb.cells()[i];
        let m = c.style.info().margins;
        let content_top = self.tops[i] + m[2] * self.zoom;
        let content_bottom = content_top + self.measure(ctx, c);
        // the margins above and below the content are insertion points (design 2.7)
        let edge = 4.0 * self.zoom;
        if p.y < content_top - edge.min(m[2] * self.zoom * 0.5) || c.props.hidden {
            return Hit::Gap(i);
        }
        if p.y > content_bottom + edge.min(m[3] * self.zoom * 0.5) {
            let mut j = i + 1;
            while j < n && !nb.is_visible(j) {
                j += 1;
            }
            return Hit::Gap(j);
        }
        let origin = self.content_origin(nb, i);
        match self.shown(c) {
            Shown::Text => {}
            Shown::Graphic(_) => {
                let r = self.graphic_rect(nb, i).unwrap_or(Rect::NOTHING);
                if r.contains(p) {
                    return Hit::Graphic(c.id);
                }
                // beside the picture: a caret before or after it
                return Hit::Text(c.id, if p.x < r.center().x { 0 } else { c.text.len() });
            }
            Shown::Large => {
                let rel = (p - origin).to_pos2();
                let buttons = self.large_buttons(ctx, c);
                return match buttons.into_iter().find(|(r, _, _)| r.contains(rel)) {
                    Some((_, a, _)) => Hit::Large(c.id, a),
                    // the sample is not edited in place: a click selects the cell
                    None => Hit::Bracket(Bracket::Cell(c.id)),
                };
            }
            // typeset input and output: the caret goes between their items (NB6.4)
            Shown::Boxes | Shown::Typeset => return Hit::Boxes(c.id),
            Shown::Controls => {
                let k = ((p.y - origin.y) / self.control_row()).floor().max(0.0) as usize;
                let n = self.controls_of(c).len();
                return Hit::Control(c.id, k.min(n.saturating_sub(1)));
            }
        }
        let g = self.galley(ctx, c);
        let cur = g.cursor_from_pos(p - origin - vec2(self.text_dx(c), 0.0));
        let byte = c.text.char_indices().nth(cur.index.0).map_or(c.text.len(), |(b, _)| b);
        Hit::Text(c.id, byte)
    }

    /// Paint the cells that intersect `clip` (screen coordinates); `origin` is the canvas's top-left on screen.
    pub fn paint(
        &mut self,
        ctx: &egui::Context,
        painter: &Painter,
        nb: &Notebook,
        origin: Pos2,
        clip: Rect,
        time: f64,
    ) {
        self.origin = origin;
        let p = self.palette.clone();
        let z = self.zoom;
        let n = nb.cells().len();
        let sel = nb.selection().clone();
        let selected: Vec<CellId> = match &sel {
            Selection::Cells { ids, .. } => ids.clone(),
            _ => vec![],
        };
        let off = origin.to_vec2();
        for i in 0..n {
            if !nb.is_visible(i) {
                continue;
            }
            let r = self.cell_rect(i).translate(off);
            if r.bottom() < clip.top() || r.top() > clip.bottom() {
                continue;
            }
            let c = &nb.cells()[i];
            let info = c.style.info();
            let (l, rr) = self.content_x(c.style);
            if c.props.hidden {
                let bar = Rect::from_min_max(pos2(origin.x + l, r.top() + 2.0), pos2(origin.x + rr, r.bottom() - 2.0));
                painter.rect_filled(bar, 1.0, p.border);
                continue;
            }
            let top = r.top() + info.margins[2] * z;
            let content = pos2(origin.x + l, top);
            let shown = self.shown(c);
            if matches!(shown, Shown::Boxes | Shown::Typeset) {
                self.paint_boxes(ctx, painter, nb, i, content, clip, time);
                continue;
            }
            if !matches!(shown, Shown::Text) {
                match shown {
                    Shown::Graphic(path) => self.paint_graphic(ctx, painter, nb, i, &path, origin, &sel, time),
                    Shown::Controls => self.paint_controls(ctx, painter, nb, i, content),
                    _ => {
                        self.paint_large(ctx, painter, c, content, clip);
                    }
                }
                continue;
            }
            let g = self.galley(ctx, c);
            // backgrounds and rules
            if c.style == Style::Code && !c.expr_view {
                let bg = Rect::from_min_max(
                    pos2(content.x - 6.0 * z, r.top() + 2.0),
                    pos2(origin.x + rr + 4.0, r.bottom() - 2.0),
                );
                painter.rect_filled(bg, 2.0, p.code_bg());
            }
            if c.expr_view {
                let bg = Rect::from_min_max(
                    pos2(content.x - 6.0 * z, top - 3.0),
                    pos2(origin.x + rr + 4.0, top + g.size().y + 3.0),
                );
                painter.rect_stroke(bg, 2.0, Stroke::new(1.0, p.border), egui::StrokeKind::Inside);
            }
            if info.rule {
                let y = r.top() + (info.margins[2] * z * 0.35).max(2.0);
                // the Section rule runs from the content edge across the window, the Program rules across the content
                let from = if c.style == Style::Program { content.x } else { origin.x + self.content_edge() };
                let y = painter.round_to_pixel_center(y);
                let rule = Stroke::new(hairline(painter), p.rule());
                painter.hline(from..=origin.x + rr, y, rule);
                if c.style == Style::Program {
                    painter.hline(content.x..=origin.x + rr, painter.round_to_pixel_center(r.bottom() - 3.0), rule);
                }
            }
            // right-justified text is anchored at the content's right edge
            let at = content + vec2(self.text_dx(c), 0.0);
            // the text selection
            if let Selection::Caret { cell, anchor, pos } = sel {
                if cell == c.id && anchor != pos {
                    paint_selection(painter, &g, &c.text, at, anchor.min(pos), anchor.max(pos), p.text_selection());
                }
            }
            painter.galley(at, g.clone(), p.text);
            // bullets and numbers of items
            if info.bullet || c.style.numbering_level().is_some() {
                let row_h = g.rows.first().map_or(info.size * z, |r| r.rect().height());
                let mark = if info.bullet { "■".to_string() } else { nb.item_number(i).unwrap_or_default() };
                let font = Face::Prose.font(info.size * z * if info.bullet { 1.1 } else { 1.0 });
                painter.text(pos2(content.x - 7.0 * z, top + row_h * 0.52), Align2::RIGHT_CENTER, mark, font, p.red);
            }
            // the caret
            if let Selection::Caret { cell, pos, anchor } = sel {
                // no caret beside a text selection, as in the reference
                if cell == c.id && anchor == pos && self.caret_on(time) {
                    let chars = c.text[..pos.min(c.text.len())].chars().count();
                    let cr = g.pos_from_cursor(CCursor::new(chars)).translate(at.to_vec2());
                    paint_caret(painter, cr, &g, chars, p.cursor);
                }
            }
        }
        if n == 0 {
            // the empty notebook says how to begin
            let c = pos2(clip.center().x, clip.top() + clip.height() * 0.38);
            painter.text(c, Align2::CENTER_BOTTOM, "An empty notebook", Face::Prose.font(18.0), p.weak);
            let hints = [
                "Type to start an Input cell  ·  Alt+7 for a Text cell  ·  Alt+4 for a Section",
                "Shift+Enter evaluates  ·  Ctrl+O opens a notebook  ·  Ctrl+N a new one",
            ];
            for (k, h) in hints.into_iter().enumerate() {
                painter.text(
                    c + vec2(0.0, 12.0 + 20.0 * k as f32),
                    Align2::CENTER_TOP,
                    h,
                    Face::Ui.font(13.0),
                    p.faint,
                );
            }
        }
        self.paint_brackets(painter, nb, origin, clip, &selected);
        // the insertion point
        let gap = self.drop_gap.or(match sel {
            Selection::Gap(g) => Some(g),
            _ => None,
        });
        if let Some(g) = gap {
            let ppp = ctx.pixels_per_point();
            let y = painter.round_to_pixel_center(origin.y + self.gap_line_y(g));
            let left = origin.x + self.content_edge() - 4.0;
            let right = origin.x + self.width - RIGHT_PAD - 22.0;
            let dragging = self.drop_gap.is_some();
            // a steady hairline (a blinking line was distracting, user decision); a drop target is 2 px in the accent
            let (width, color) = if dragging {
                ((2.0 * ppp).round().max(1.0) / ppp, p.accent)
            } else {
                (hairline(painter), p.guide_line())
            };
            painter.hline(left..=right.max(left + 20.0), y, Stroke::new(width, color));
            if !dragging {
                let center = origin + self.plus_center(g).to_vec2();
                let hot = self.plus_hover;
                let color = if hot { p.accent } else { p.gutter };
                if hot {
                    painter.circle_filled(center, 7.0, p.over_bg(p.accent, 0.12));
                }
                // the icon set's plus in a ring
                mc_ui::icons::paint(painter, mc_ui::icons::Icon::Insert, Rect::from_center_size(center, vec2(18.0, 18.0)), color);
                if nb.gap_style() != Style::Input {
                    let font = Face::Ui.font(10.5);
                    painter.text(
                        pos2(left + 2.0, y - 3.0),
                        Align2::LEFT_BOTTOM,
                        nb.gap_style().info().name,
                        font,
                        p.weak,
                    );
                }
            }
        }
    }

    /// The accessibility nodes of the cells on the screen (design 9.7): headings with their level for the title
    /// and section styles, paragraphs for prose, multi-line text inputs for Input and Code cells (typeset ones give
    /// their linear text as value), status text for outputs (named `out(n)`, the printed text as value), images for
    /// pictures. Nodes hang under the window's root; the selection marks the selected cells.
    pub fn accessibility(&mut self, ctx: &egui::Context, nb: &Notebook, origin: Pos2, clip: Rect) {
        use egui::accesskit::{Rect as AkRect, Role};
        let selected: Vec<CellId> = match nb.selection() {
            Selection::Cells { ids, .. } => ids.clone(),
            Selection::Caret { cell, .. } => vec![*cell],
            Selection::Gap(_) => vec![],
        };
        for (i, c) in nb.cells().iter().enumerate() {
            if !nb.is_visible(i) {
                continue;
            }
            let r = self.cell_rect(i).translate(origin.to_vec2());
            if r.bottom() < clip.top() || r.top() > clip.bottom() {
                continue;
            }
            let level = match c.style {
                Style::Title | Style::Chapter => Some(1),
                Style::Subtitle | Style::Subchapter | Style::Section => Some(2),
                Style::Subsubtitle | Style::Subsection => Some(3),
                Style::Subsubsection => Some(4),
                _ => None,
            };
            let (role, name, value) = if let Some(path) = c.graphic() {
                (Role::Image, format!("Picture {path}"), None)
            } else if let Some(l) = level {
                let _ = l;
                (Role::Heading, c.text.clone(), None)
            } else if c.style.is_code() {
                let value = if c.props.typeset { mc_boxes::linear_text(&mc_boxes::from_text(&c.text)) } else { c.text.clone() };
                (Role::MultilineTextInput, format!("{} cell", c.style.info().name), Some(value))
            } else if c.style == Style::Output {
                let name = match c.label {
                    Some(n) => format!("Output out({n})"),
                    None => "Output".to_string(),
                };
                (Role::Status, name, Some(c.text.clone()))
            } else if c.generated() {
                (Role::Log, format!("{} cell", c.style.info().name), Some(c.text.clone()))
            } else {
                (Role::Paragraph, c.text.clone(), None)
            };
            let id = egui::Id::unique("mc-cell").with(c.id);
            ctx.accesskit_node_builder(id, |node| {
                node.set_role(role);
                node.set_label(name);
                if let Some(v) = value {
                    node.set_value(v);
                }
                if let Some(l) = level {
                    node.set_level(l);
                }
                if !c.editable() || c.generated() {
                    node.set_read_only();
                }
                node.set_selected(selected.contains(&c.id));
                node.set_bounds(AkRect { x0: r.min.x.into(), y0: r.min.y.into(), x1: r.max.x.into(), y1: r.max.y.into() });
            });
        }
    }

    fn paint_brackets(&self, painter: &Painter, nb: &Notebook, origin: Pos2, clip: Rect, selected: &[CellId]) {
        let p = &self.palette;
        for (col, top, bottom, b) in self.brackets(nb) {
            let (top, bottom) = (origin.y + top + 1.5, origin.y + bottom - 1.5);
            if bottom < clip.top() || top > clip.bottom() {
                continue;
            }
            let x = origin.x + self.bracket_x(col);
            let (cells, head): (Vec<usize>, Option<usize>) = match b {
                Bracket::Cell(id) => (nb.index_of(id).into_iter().collect(), None),
                Bracket::Group(s, e) => ((s..e).collect(), Some(s)),
            };
            let is_selected = !selected.is_empty() && cells.iter().all(|&i| selected.contains(&nb.cells()[i].id));
            let mark = match b {
                Bracket::Cell(id) => nb.cell(id).map_or(EvalMark::None, |c| c.eval),
                Bracket::Group(..) => EvalMark::None,
            };
            let (color, width): (Color32, f32) = match mark {
                EvalMark::Running => (p.accent, 2.0),
                EvalMark::Queued => (p.yellow, 1.5),
                EvalMark::Error => (p.red, 1.5),
                EvalMark::None if self.hover == Some(b) => (p.accent, 1.0),
                EvalMark::None => (p.gutter, 1.0),
            };
            if is_selected {
                let fill = Rect::from_min_max(pos2(x - 1.5, top), pos2(x + HOOK + 1.0, bottom));
                painter.rect_filled(fill, 0.0, p.over_bg(p.accent, 0.35));
            }
            // one path on device-pixel centres, so the hooks join the stem cleanly and nothing blurs
            let stroke =
                Stroke::new(hairline(painter) * width.round().max(1.0), if is_selected { p.accent } else { color });
            let (x, top, bottom) = (
                painter.round_to_pixel_center(x),
                painter.round_to_pixel_center(top),
                painter.round_to_pixel_center(bottom),
            );
            let hook = snap(painter, HOOK * self.zoom.max(1.0));
            // hooks point right, towards the content
            painter.add(egui::Shape::line(
                vec![pos2(x + hook, top), pos2(x, top), pos2(x, bottom), pos2(x + hook, bottom)],
                stroke,
            ));
            let closed = head.is_some_and(|h| nb.cells()[h].closed.is_some());
            if closed {
                let tri = vec![
                    pos2(x - 1.0, bottom - HOOK - 3.0),
                    pos2(x - 1.0, bottom - 1.0),
                    pos2(x + HOOK + 1.0, bottom - 1.0),
                ];
                painter.add(egui::Shape::convex_polygon(tri, stroke.color, Stroke::NONE));
            }
        }
    }

    /// Paint cell `i`'s picture (or a frame while it loads, or the path of a missing one), the frame and handles when
    /// it is selected, the caret beside it.
    #[allow(clippy::too_many_arguments)]
    fn paint_graphic(
        &mut self,
        ctx: &egui::Context,
        painter: &Painter,
        nb: &Notebook,
        i: usize,
        path: &Path,
        origin: Pos2,
        sel: &Selection,
        time: f64,
    ) {
        let p = self.palette.clone();
        let c = &nb.cells()[i];
        let Some(r) = self.graphic_rect(nb, i) else { return };
        let r = r.translate(origin.to_vec2());
        // the raster size: SVG drawings are rasterised for the display, at the size they keep (not every size a
        // resize passes through)
        let width = (self.graphic_size(c, path, false).x * ctx.pixels_per_point()).round() as u32;
        // a 3D picture turned away from its own view: the scene drawn from that view (the picture shows until the
        // first render is there)
        let turned = self.rot.clone().and_then(|rot| {
            let mut rot = rot.borrow_mut();
            let scene = rot.scene(path)?;
            let view = match self.live_view {
                Some((id, v)) if id == c.id => v,
                _ => crate::rotate3d::view_of(&c.out_extra, scene.view())?,
            };
            let page = scene.page_size().0.max(1.0) as f32;
            let scale = (r.width() * ctx.pixels_per_point() / page).clamp(0.25, 4.0);
            // scales in steps of 1/8, so a resize does not ask for a render per pixel
            let scale = (scale * 8.0).round() / 8.0;
            rot.texture(c.id, path, view, scale)
        });
        let picture = match turned {
            Some(tex) => Picture::Ready(tex.id(), tex.size()),
            None => self.images.borrow_mut().picture(path, width, self.now),
        };
        match picture {
            Picture::Ready(tex, _) => {
                let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
                painter.image(tex, r, uv, Color32::WHITE);
            }
            Picture::Loading => {
                painter.rect_stroke(r, 0.0, Stroke::new(hairline(painter), p.border), egui::StrokeKind::Inside);
            }
            Picture::Missing(why) => {
                painter.rect_stroke(r, 2.0, Stroke::new(1.0, p.border_strong()), egui::StrokeKind::Inside);
                let shown = mc_nbfile::images::image_path(&c.text).unwrap_or_default();
                let text = format!("{shown}: {why}");
                let font = Face::Ui.font(11.0 * self.magnification().max(0.75));
                let g = painter.layout(text, font, p.weak, (r.width() - 16.0).max(20.0));
                painter.galley(pos2(r.left() + 8.0, r.center().y - g.size().y / 2.0), g, p.weak);
            }
        }
        if self.graphic == Some(c.id) {
            // a 1 px frame in the accent colour with eight square handles (design 6)
            let frame = Rect::from_min_max(
                pos2(painter.round_to_pixel_center(r.left()), painter.round_to_pixel_center(r.top())),
                pos2(painter.round_to_pixel_center(r.right()), painter.round_to_pixel_center(r.bottom())),
            );
            painter.rect_stroke(frame, 0.0, Stroke::new(hairline(painter), p.accent), egui::StrokeKind::Middle);
            for h in Handle::ALL {
                let at = h.at(frame);
                let sq = Rect::from_center_size(pos2(snap(painter, at.x), snap(painter, at.y)), vec2(HANDLE, HANDLE));
                painter.rect_filled(sq, 0.0, p.bg);
                painter.rect_stroke(sq, 0.0, Stroke::new(hairline(painter), p.accent), egui::StrokeKind::Inside);
            }
        } else if let Selection::Caret { cell, anchor, pos } = *sel {
            if cell == c.id && anchor == pos && self.caret_on(time) {
                let x = if pos == 0 { r.left() - 3.0 } else { r.right() + 3.0 };
                let ppp = painter.pixels_per_point();
                let w = (2.0 * ppp).round().max(1.0) / ppp;
                let x = (x * ppp).round() / ppp;
                painter.rect_filled(Rect::from_min_max(pos2(x - w / 2.0, r.top()), pos2(x + w / 2.0, r.bottom())), 0.0, p.cursor);
            }
        }
    }

    /// Paint a typeset input cell: its boxes, the label, and the caret, selection and alias mark of the editor.
    #[allow(clippy::too_many_arguments)]
    fn paint_boxes(
        &mut self,
        ctx: &egui::Context,
        painter: &Painter,
        nb: &Notebook,
        i: usize,
        content: Pos2,
        clip: Rect,
        time: f64,
    ) {
        let p = self.palette.clone();
        let c = &nb.cells()[i];
        let lk = look(c, &p);
        let lay = self.box_layout(ctx, c);
        let output = c.style == Style::Output;
        let editor = if output { nb.output_editor() } else { nb.box_editor() };
        let editor = editor.filter(|(id, _)| *id == c.id).map(|(_, e)| e);
        // the selection behind the boxes, line by line when it wraps
        if let Some((path, range)) = editor.and_then(|e| e.selection()) {
            let font = self.text_font(ctx, c);
            for r in selection_rects(&lay, &path, range, (font.ascent, font.row - font.ascent)) {
                painter.rect_filled(r.translate(content.to_vec2()), 0.0, p.text_selection());
            }
        }
        let colours = match (output, c.failure) {
            (_, true) => None,
            (true, false) => self.label_colours(c, &lay),
            (false, false) => Some(self.box_colours(c, &lay)),
        };
        let mut ink = typeset::Ink::new(&p, lk.color, lk.face);
        ink.colours = colours.as_deref().map(|v| v.as_slice());
        typeset::paint(painter, &mut self.glyphs.borrow_mut(), &lay, content, clip, &ink);
        if output && editor.is_none() {
            if let Selection::Caret { cell, anchor, pos } = *nb.selection() {
                if cell == c.id && anchor == pos && self.caret_on(time) {
                    let r = self.output_caret(ctx, c, pos).translate(content.to_vec2());
                    let ppp = painter.pixels_per_point();
                    let w = (2.0 * ppp).round().max(1.0) / ppp;
                    let x = (r.left() * ppp).round() / ppp;
                    let r = Rect::from_min_max(pos2(x - w / 2.0, snap(painter, r.top())), pos2(x + w / 2.0, snap(painter, r.bottom())));
                    painter.rect_filled(r, 0.0, p.cursor);
                }
            }
            return;
        }
        let Some(ed) = editor else { return };
        if output {
            self.paint_box_caret(painter, &lay, ed, content, time);
            return;
        }
        // the alias being typed: a marker before its name, red when the name is unknown
        if let Some(m) = ed.alias_mark() {
            if let Some(s) = lay.caret(&m.path, m.start) {
                let color = if m.unknown { p.red } else { p.accent };
                let x = snap(painter, content.x + s.x - 1.0);
                painter.vline(x, (content.y + s.top)..=(content.y + s.bottom), egui::Stroke::new(1.0, color));
                painter.hline((x - 2.0)..=(x + 2.0), content.y + s.bottom, egui::Stroke::new(1.0, color));
            }
        }
        self.paint_box_caret(painter, &lay, ed, content, time);
    }

    /// The caret of a box editor (none beside a selection, as for text).
    fn paint_box_caret(&self, painter: &Painter, lay: &Layout, ed: &mc_boxes::Editor, content: Pos2, time: f64) {
        if ed.selection().is_some() || !self.caret_on(time) {
            return;
        }
        if let Some(s) = lay.caret(&ed.caret().path, ed.caret().offset) {
            let ppp = painter.pixels_per_point();
            let w = (2.0 * ppp).round().max(1.0) / ppp;
            let x = ((content.x + s.x) * ppp).round() / ppp;
            let r = Rect::from_min_max(pos2(x - w / 2.0, snap(painter, content.y + s.top)), pos2(x + w / 2.0, snap(painter, content.y + s.bottom)));
            painter.rect_filled(r, 0.0, self.palette.cursor);
        }
    }

    /// The colour of every character of a typeset input cell, per layout row and item: the syntax colouring of its
    /// canonical text (design 3.11), placed through the text map of the box tree.
    fn box_colours(&mut self, c: &Cell, lay: &Layout) -> Arc<Vec<Vec<Color32>>> {
        let key = (c.rev, self.defined_rev, self.palette.dark, self.box_key(c).1);
        if let Some((k, v)) = self.box_colours.get(&c.id) {
            if *k == key {
                return v.clone();
            }
        }
        let p = self.palette.clone();
        let base = look(c, &p).color;
        let colouring = self.colouring(c);
        let (_, map) = mc_boxes::to_text_mapped(&mc_boxes::from_text(&c.text));
        let table: Vec<Vec<Color32>> = lay
            .rows
            .iter()
            .map(|r| {
                (0..r.stops.len().saturating_sub(1))
                    .map(|i| {
                        map.offset(&r.path, i)
                            .and_then(|o| colouring.class_at(o))
                            .map_or(base, |class| highlight::style(class, &p).0)
                    })
                    .collect()
            })
            .collect();
        let v = Arc::new(table);
        self.box_colours.insert(c.id, (key, v.clone()));
        v
    }

    /// The colours of a typeset output's labels and legends (round 13), per layout row and item: label rows in the
    /// theme's label colour, swatches in their own; `None` for an output without labels (all in its text colour).
    fn label_colours(&mut self, c: &Cell, lay: &Layout) -> Option<Arc<Vec<Vec<Color32>>>> {
        if !(c.text.contains("labeled(") || c.text.contains("legended(")) {
            return None;
        }
        let key = (c.rev, 0, self.palette.dark, self.box_key(c).1);
        if let Some((k, v)) = self.box_colours.get(&c.id) {
            if *k == key {
                return (!v.is_empty()).then(|| v.clone());
            }
        }
        let labels = mc_boxes::label_rows(&mc_boxes::math_text(&c.text).row);
        let muted = self.palette.label();
        let table: Vec<Vec<Color32>> = if labels.is_empty() {
            vec![]
        } else {
            lay.rows
                .iter()
                .map(|r| match labels.iter().find(|l| l.path == r.path) {
                    Some(l) => {
                        let mut v = vec![muted; r.stops.len().saturating_sub(1)];
                        for &(k, [red, green, blue]) in &l.swatches {
                            if let Some(slot) = v.get_mut(k) {
                                *slot = Color32::from_rgb(red, green, blue);
                            }
                        }
                        v
                    }
                    None => vec![],
                })
                .collect()
        };
        let v = Arc::new(table);
        self.box_colours.insert(c.id, (key, v.clone()));
        (!v.is_empty()).then_some(v)
    }

    /// The caret of a typeset output (content coordinates): before its first line, or after its last.
    fn output_caret(&mut self, ctx: &egui::Context, c: &Cell, pos: usize) -> Rect {
        let lay = self.box_layout(ctx, c);
        let rights = typeset::line_rights(&lay);
        let (ln, x) = if pos == 0 {
            (lay.lines.first(), lay.lines.first().map_or(0.0, |l| l.indent))
        } else {
            (lay.lines.last(), rights.last().copied().unwrap_or(0.0) + 1.0)
        };
        match ln {
            // the caret spans the line's text height around its baseline
            Some(l) => {
                let h = look(c, &self.palette).size * self.zoom;
                Rect::from_min_max(pos2(x, (l.baseline - 0.8 * h).max(l.top)), pos2(x, (l.baseline + 0.25 * h).min(l.bottom)))
            }
            None => Rect::from_min_max(pos2(x, 0.0), pos2(x, lay.height)),
        }
    }

    /// Paint a large output's summary (design 4.9): a title, the sample (only its visible lines), the buttons; gives
    /// the y (screen) of its first line's middle.
    fn paint_large(&mut self, ctx: &egui::Context, painter: &Painter, c: &Cell, content: Pos2, clip: Rect) -> f32 {
        let p = self.palette.clone();
        let l = self.long(ctx, c);
        let (row, title, _) = self.long_metrics(ctx, c);
        let ui = Face::Ui.font(12.0 * self.magnification().max(0.75));
        let all = l.text.len() == c.text.len();
        let head = if all {
            format!("A very large output ({} characters), shown in full:", group_digits(l.chars))
        } else {
            format!("A very large output was generated ({} characters). Here is a sample of it:", group_digits(l.chars))
        };
        // an output's title and lines end at the right edge (NBUI.1), a print's start at the left
        let right = self.right_justified(c);
        let (x0, align) = if right {
            (content.x + self.text_dx(c), (Align2::RIGHT_TOP, egui::Align::RIGHT))
        } else {
            (content.x, (Align2::LEFT_TOP, egui::Align::LEFT))
        };
        painter.text(pos2(x0, content.y), align.0, head, ui.clone(), p.weak);
        let font = self.font(c);
        let color = look(c, &p).color;
        let first = content.y + title;
        // only the lines on the screen are laid out: a whole output can have hundreds of thousands
        let from = ((clip.top() - first) / row).floor().max(0.0) as usize;
        let to = (((clip.bottom() - first) / row).ceil().max(0.0) as usize + 1).min(l.rows.len());
        for k in from..to {
            let line = &l.text[l.rows[k].clone()];
            let line = if align.1 == egui::Align::RIGHT { line.trim_end() } else { line };
            painter.text(pos2(x0, first + k as f32 * row), align.0, line, font.clone(), color);
        }
        // a thin rule left of the sample marks it
        let bottom = first + l.rows.len() as f32 * row;
        let left = if right {
            let cw = ctx.fonts_mut(|f| f.glyph_width(&font, '0'));
            (x0 - l.widest as f32 * cw).max(content.x)
        } else {
            content.x
        };
        let x = painter.round_to_pixel_center(left - 6.0 * self.zoom);
        painter.vline(x, first..=bottom, Stroke::new(hairline(painter), p.border_strong()));
        for (r, _, label) in self.large_buttons(ctx, c) {
            let r = r.translate(content.to_vec2());
            painter.rect_stroke(r, 3.0, Stroke::new(hairline(painter), p.border_strong()), egui::StrokeKind::Inside);
            painter.text(r.center(), Align2::CENTER_CENTER, label, ui.clone(), p.accent);
        }
        first + row / 2.0
    }

    /// The centre of the "+" button of the insertion point before cell `g` (canvas coordinates).
    pub fn plus_center(&self, g: usize) -> Pos2 {
        // at the right end of the insertion line, clear of the group brackets on the left
        pos2(self.width - RIGHT_PAD - 10.0, self.gap_line_y(g))
    }

    /// Where the insertion line before cell `g` is drawn: on the boundary between cells, a little below the last
    /// cell so it does not touch it.
    pub fn gap_line_y(&self, g: usize) -> f32 {
        if g >= self.tops.len() && !self.tops.is_empty() {
            self.total_height() + 5.0 * self.zoom
        } else {
            self.gap_y(g)
        }
    }

    /// The magnification (1.0 = 100 %).
    pub fn magnification(&self) -> f32 {
        self.zoom / PX_PER_PT
    }

    /// Set the magnification (1.0 = 100 %).
    pub fn set_magnification(&mut self, m: f32) {
        self.zoom = m * PX_PER_PT;
    }

    /// The caret is drawn now (blink phase; always right after it moved).
    pub fn caret_on(&self, time: f64) -> bool {
        !self.blink || (((time - self.blink_from) / BLINK) as u64).is_multiple_of(2)
    }

    /// The seconds until the caret's blink phase changes.
    pub fn next_blink(&self, time: f64) -> f64 {
        BLINK - (time - self.blink_from).rem_euclid(BLINK)
    }

    /// The visual lines of a cell's content as byte ranges.
    pub fn lines(&mut self, ctx: &egui::Context, cell: &Cell) -> Vec<Range<usize>> {
        if !self.shows_text(cell) {
            // a picture or a sample is one line for the caret: before it or after it
            return std::iter::once(0..cell.text.len()).collect();
        }
        let g = self.galley(ctx, cell);
        let offsets: Vec<usize> =
            cell.text.char_indices().map(|(b, _)| b).chain(std::iter::once(cell.text.len())).collect();
        let at = |ci: usize| offsets[ci.min(offsets.len() - 1)];
        let mut out = vec![];
        let mut ci = 0;
        for row in &g.rows {
            let n = row.row.glyphs.len();
            out.push(at(ci)..at(ci + n));
            ci += n + usize::from(row.ends_with_newline);
        }
        if out.is_empty() {
            out.push(0..0);
        }
        out
    }

    /// The x of a position in its visual line.
    pub fn x_of(&mut self, ctx: &egui::Context, cell: &Cell, pos: usize) -> f32 {
        if !self.shows_text(cell) {
            return if pos == 0 { 0.0 } else { 1.0 };
        }
        let g = self.galley(ctx, cell);
        let chars = cell.text[..pos.min(cell.text.len())].chars().count();
        g.pos_from_cursor(CCursor::new(chars)).left() + self.text_dx(cell)
    }

    /// The position in `line` nearest to `x`.
    pub fn pos_at(&mut self, ctx: &egui::Context, cell: &Cell, line: Range<usize>, x: f32) -> usize {
        if !self.shows_text(cell) {
            return if x <= 0.5 { line.start } else { line.end };
        }
        let g = self.galley(ctx, cell);
        let chars = cell.text[..line.start].chars().count();
        let y = g.pos_from_cursor(CCursor::new(chars)).center().y;
        let cur = g.cursor_from_pos(vec2(x - self.text_dx(cell), y));
        let byte = cell.text.char_indices().nth(cur.index.0).map_or(cell.text.len(), |(b, _)| b);
        byte.clamp(line.start, line.end)
    }
}

/// The rectangles of a selection of items `range` of the row at `path`, one per visual line it spans, each drawn
/// tightly around what is selected (the ink extent of its characters, glyphs and rules, a text character counting
/// its font's row height `text` (ascent, descent in ems) as in plain text), not the whole height of its line.
fn selection_rects(lay: &Layout, path: &[mc_boxes::Step], range: Range<usize>, text: (f32, f32)) -> Vec<Rect> {
    let stops: Vec<_> = (range.start..=range.end).filter_map(|k| lay.caret(path, k)).collect();
    let mut out: Vec<Rect> = vec![];
    for w in stops.windows(2) {
        let (a, b) = (&w[0], &w[1]);
        // a stop on a new line (a wrap) starts a new rectangle
        let r = Rect::from_min_max(pos2(a.x.min(b.x), a.top.min(b.top)), pos2(a.x.max(b.x), a.bottom.max(b.bottom)));
        match out.last_mut() {
            Some(last) if (last.center().y - r.center().y).abs() < last.height() * 0.5 && b.x >= a.x => *last = last.union(r),
            _ if b.x < a.x => out.push(Rect::from_min_max(pos2(a.x, a.top), pos2(a.x + 2.0, a.bottom))),
            _ => out.push(r),
        }
    }
    out.into_iter().map(|r| tighten(lay, r, text)).collect()
}

/// A selection rectangle (the band of its row's caret stops) shrunk vertically to the extent of the items in it.
fn tighten(lay: &Layout, r: Rect, (asc, desc): (f32, f32)) -> Rect {
    use mc_boxes::layout::{metrics::math_char, Draw, FontRole};
    let f = mc_mathfont::font();
    let (mut top, mut bottom) = (f32::INFINITY, f32::NEG_INFINITY);
    for d in &lay.items {
        // left, right, top, bottom and the y that places it on a line
        let (x0, x1, y0, y1, at) = match d {
            Draw::Char { c, role, x, y, size, advance, .. } => {
                if c.is_whitespace() {
                    continue;
                }
                let (a, b) = if *role == FontRole::Math {
                    let m = math_char(*c);
                    (m.ascent, m.descent)
                } else {
                    (asc, desc)
                };
                (*x, x + advance, y - a * size, y + b * size, *y)
            }
            Draw::Placeholder { x, y, size, .. } => {
                let m = math_char('□');
                (*x, x + m.advance * size, y - m.ascent * size, y + m.descent * size, *y)
            }
            Draw::Glyph { glyph, size, x, y } => {
                let m = f.metrics(*glyph);
                (*x, x + m.advance * size, y - m.height * size, y + m.depth * size, *y)
            }
            Draw::Rule { x, y, w, h } | Draw::Graphic { x, y, w, h, .. } => (*x, x + w, *y, y + h, y + h / 2.0),
        };
        if x1 > r.left() + 0.5 && x0 < r.right() - 0.5 && at >= r.top() - 0.5 && at <= r.bottom() + 0.5 {
            top = top.min(y0);
            bottom = bottom.max(y1);
        }
    }
    if top < bottom {
        Rect::from_min_max(pos2(r.left(), top), pos2(r.right(), bottom))
    } else {
        r
    }
}

fn paint_selection(painter: &Painter, g: &Galley, text: &str, origin: Pos2, a: usize, b: usize, color: Color32) {
    let ca = text[..a].chars().count();
    let cb = text[..b].chars().count();
    let mut idx = 0;
    for row in &g.rows {
        let n = row.row.glyphs.len();
        let (rs, re) = (idx, idx + n);
        let (s, e) = (ca.max(rs), cb.min(re));
        if s < e || (s == e && row.ends_with_newline && ca <= re && cb > re) {
            let x_at = |k: usize| row.row.glyphs.get(k).map_or(row.rect().width(), |g| g.pos.x);
            let x0 = if s > rs { x_at(s - rs) } else { 0.0 };
            let x1 = if e < re { x_at(e - rs) } else { row.rect().width() + if cb > re { 4.0 } else { 0.0 } };
            let rr = Rect::from_min_max(pos2(x0, row.rect().top()), pos2(x1.max(x0 + 2.0), row.rect().bottom()));
            painter.rect_filled(rr.translate(origin.to_vec2() + vec2(row.pos.x, 0.0)), 0.0, color);
        }
        idx = re + usize::from(row.ends_with_newline);
    }
}

/// The caret: a bar of the line's font height (the glyph's ascent down to its line height, so it stands on the
/// baseline whatever the row's other fonts), on whole device pixels, 2 px at 100 % (design 2.9).
fn paint_caret(painter: &Painter, row_rect: Rect, g: &Galley, chars: usize, color: Color32) {
    let ppp = painter.pixels_per_point();
    let (top, bottom) = match glyph_near(g, chars) {
        Some((baseline, ascent, height)) => {
            let top = row_rect.top() + baseline - ascent;
            (top, (top + height).min(row_rect.bottom()))
        }
        None => (row_rect.top(), row_rect.bottom()),
    };
    let w = (2.0 * ppp).round().max(1.0) / ppp;
    let x = (row_rect.left() * ppp).round() / ppp;
    let snap = |y: f32| (y * ppp).round() / ppp;
    painter.rect_filled(Rect::from_min_max(pos2(x - w * 0.5, snap(top)), pos2(x + w * 0.5, snap(bottom))), 0.0, color);
}

/// The glyph next to character `chars` of a galley (before it, or after it at a line start): its baseline in the
/// row, its font's ascent and line height.
fn glyph_near(g: &Galley, chars: usize) -> Option<(f32, f32, f32)> {
    let mut idx = 0;
    for row in &g.rows {
        let n = row.row.glyphs.len();
        if chars <= idx + n {
            let k = chars - idx;
            let gl = if k > 0 { row.row.glyphs.get(k - 1) } else { row.row.glyphs.first() }?;
            return Some((gl.pos.y, gl.font_ascent, gl.font_height));
        }
        idx += n + usize::from(row.ends_with_newline);
    }
    None
}

/// A count with thin spaces between groups of three digits (`5 000 000`).
fn group_digits(n: usize) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (k, ch) in s.chars().enumerate() {
        if k > 0 && (s.len() - k).is_multiple_of(3) {
            out.push('\u{2009}');
        }
        out.push(ch);
    }
    out
}

/// One device pixel at 100 % - 150 %, two at 200 % (in points): lines stay crisp at every scale.
fn hairline(painter: &Painter) -> f32 {
    let ppp = painter.pixels_per_point();
    ppp.floor().max(1.0) / ppp
}

/// `v` on a device-pixel edge.
fn snap(painter: &Painter, v: f32) -> f32 {
    let ppp = painter.pixels_per_point();
    (v * ppp).round() / ppp
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::images::Images;

    /// The accessibility tree of a small notebook (milestone M7's snapshot): every cell on the screen is a node with
    /// its role, name and value.
    #[test]
    fn accessibility_tree_of_a_notebook() {
        let text = "#! mathcas 2 notebook\n\n#% title\n# Heat\n\n#% section\n# Model\n\n#% text\n# A sentence.\n\n\
                    #% cell label=1 typeset\n((a)/(b))+((sqrt(c)))\n#% output n=1\n#= a/b + c^(1/2)\n";
        let nb = Notebook::parse(text).unwrap();
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let images = Rc::new(RefCell::new(Images::new(&ctx)));
        let glyphs = Rc::new(RefCell::new(Glyphs::new(&ctx)));
        let mut view = View::new(Palette::light(), images, glyphs);
        let out = ctx.run_ui(egui::RawInput::default(), |ui| {
            let ctx = ui.ctx().clone();
            view.update_positions(&ctx, &nb, 0.0..4000.0);
            view.accessibility(&ctx, &nb, Pos2::ZERO, Rect::EVERYTHING);
        });
        let update = out.platform_output.accesskit_update.expect("an accessibility update");
        let mut lines: Vec<String> = update
            .nodes
            .iter()
            .filter(|(_, n)| {
                matches!(
                    n.role(),
                    egui::accesskit::Role::Heading
                        | egui::accesskit::Role::Paragraph
                        | egui::accesskit::Role::MultilineTextInput
                        | egui::accesskit::Role::Status
                )
            })
            .map(|(_, n)| {
                format!(
                    "{:?} {:?} {:?} level {:?}",
                    n.role(),
                    n.label().unwrap_or_default(),
                    n.value().unwrap_or_default(),
                    n.level()
                )
            })
            .collect();
        lines.sort();
        assert_eq!(
            lines,
            [
                "Heading \"Heat\" \"\" level Some(1)",
                "Heading \"Model\" \"\" level Some(2)",
                "MultilineTextInput \"Input cell\" \"a/b+sqrt(c)\" level None",
                "Paragraph \"A sentence.\" \"\" level None",
                "Status \"Output out(1)\" \"a/b + c^(1/2)\" level None",
            ]
        );
    }

    /// Outputs start at the content's left edge by default (round 13, user decision; View > Output alignment), and
    /// the caret, hits and clicks in boxes follow; switching to Right and back lays them out again.
    #[test]
    fn outputs_start_at_the_left_edge_by_default_and_switch() {
        let text = "#! mathcas 2 notebook\n\n#% cell label=1\nx/2\n#% output n=1\n#= x/2 + 1\n\n\
                    #% cell label=2\nsqrt(y)\n#% output n=2\n#| a print\n#! failure(division_by_zero)\n";
        let nb = Notebook::parse(text).unwrap();
        let ctx = egui::Context::default();
        ctx.set_fonts(mc_ui::fonts::definitions(mc_ui::fonts::Source::Bundled));
        let images = Rc::new(RefCell::new(Images::new(&ctx)));
        let glyphs = Rc::new(RefCell::new(Glyphs::new(&ctx)));
        let defs = mc_ui::fonts::definitions(mc_ui::fonts::Source::Bundled);
        glyphs.borrow_mut().set_fonts(mc_ui::fonts::FontSet::new(&defs));
        let mut view = View::new(Palette::light(), images, glyphs);
        view.width = 700.0;
        let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
            let ctx = ui.ctx().clone();
            view.update_positions(&ctx, &nb, 0.0..4000.0);
            let (l, r) = view.content_x(Style::Output);
            let typeset = &nb.cells()[1];
            let lay = view.box_layout(&ctx, typeset);
            assert!(lay.rows[0].stops[0].x < 1.0, "the first stop at the left edge: {:?}", lay.rows[0].stops[0]);
            let o = view.content_origin(&nb, 1);
            let (path, k) = view.box_hit(&ctx, &nb, typeset.id, pos2(o.x + 0.5, o.y + lay.baseline)).unwrap();
            assert_eq!((path.len(), k), (0, 0));
            let linear = &nb.cells()[4];
            let start = view.caret_rect(&ctx, &nb, linear.id, 0).unwrap();
            assert!((start.left() - l).abs() < 1.0, "{start:?} {l}");
            assert_eq!(view.hit(&ctx, &nb, pos2(l + 0.5, start.center().y)), Hit::Text(linear.id, 0));
            // Right, then Left again
            view.set_outputs_right(true);
            view.update_positions(&ctx, &nb, 0.0..4000.0);
            let lay = view.box_layout(&ctx, typeset);
            assert!((lay.lines[0].right - (r - l)).abs() < 0.5);
            view.set_outputs_right(false);
            view.update_positions(&ctx, &nb, 0.0..4000.0);
            let lay = view.box_layout(&ctx, typeset);
            assert!(lay.rows[0].stops[0].x < 1.0);
        });
    }

    /// Outputs are right-justified (NBUI.1): a typeset output's lines and a linear output's rows end at the
    /// content's right edge, and the caret, hit-testing and clicks in boxes follow; inputs stay on the left edge.
    #[test]
    fn outputs_end_at_the_right_edge_and_geometry_follows() {
        let text = "#! mathcas 2 notebook\n\n#% cell label=1\nx/2\n#% output n=1\n#= x/2 + 1\n\n\
                    #% cell label=2\nsqrt(y)\n#% output n=2\n#| a print\n#! failure(division_by_zero)\n";
        let nb = Notebook::parse(text).unwrap();
        let ctx = egui::Context::default();
        ctx.set_fonts(mc_ui::fonts::definitions(mc_ui::fonts::Source::Bundled));
        let images = Rc::new(RefCell::new(Images::new(&ctx)));
        let glyphs = Rc::new(RefCell::new(Glyphs::new(&ctx)));
        let defs = mc_ui::fonts::definitions(mc_ui::fonts::Source::Bundled);
        glyphs.borrow_mut().set_fonts(mc_ui::fonts::FontSet::new(&defs));
        let mut view = View::new(Palette::light(), images, glyphs);
        view.width = 700.0;
        view.set_outputs_right(true);
        let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
            let ctx = ui.ctx().clone();
            view.update_positions(&ctx, &nb, 0.0..4000.0);
            let (l, r) = view.content_x(Style::Output);
            // the typeset output: its line ends at the right edge, its first stop is well right of the left edge
            let typeset = &nb.cells()[1];
            let lay = view.box_layout(&ctx, typeset);
            assert!((lay.lines[0].right - (r - l)).abs() < 0.5, "{:?} {}", lay.lines, r - l);
            assert!(lay.rows[0].stops[0].x > (r - l) / 2.0);
            // a click at its right end puts the box caret at the end of the output
            let o = view.content_origin(&nb, 1);
            let (path, k) = view.box_hit(&ctx, &nb, typeset.id, pos2(o.x + r - l - 1.0, o.y + lay.baseline)).unwrap();
            assert_eq!((path.len(), k), (0, lay.rows[0].stops.len() - 1));
            // an output drawn as text (a failure): the caret after its last character is at the right edge, a click
            // there gives it
            let linear = &nb.cells()[4];
            let end = view.caret_rect(&ctx, &nb, linear.id, linear.text.len()).unwrap();
            assert!((end.left() - r).abs() < 1.0, "{end:?} {r}");
            let start = view.caret_rect(&ctx, &nb, linear.id, 0).unwrap();
            assert!(start.left() > l + 100.0);
            assert_eq!(view.hit(&ctx, &nb, pos2(r - 0.5, end.center().y)), Hit::Text(linear.id, linear.text.len()));
            // inputs and prints stay on the left edge
            assert!((view.caret_rect(&ctx, &nb, nb.cells()[0].id, 0).unwrap().left() - l).abs() < 1.0);
            assert_eq!(nb.cells()[3].style, Style::Print);
            assert!((view.caret_rect(&ctx, &nb, nb.cells()[3].id, 0).unwrap().left() - l).abs() < 1.0);
            // x positions of Up and Down are content-relative for both
            assert!((view.x_of(&ctx, linear, linear.text.len()) - (r - l)).abs() < 1.0);
        });
    }

    /// Round 13 (G80): a labeled output is right-justified as a whole with its label under the value, the label row
    /// drawn in the label colour and a legend's swatches in theirs; a click on the value lands in the value.
    #[test]
    fn labeled_outputs_draw_their_labels_at_the_right_edge() {
        let text = "#! mathcas 2 notebook\n\n#% cell label=1\nl\n#% output n=1\n#= labeled(x^2 + 1, \"energy\")\n\n\
                    #% cell label=2\nm\n#% output n=2\n#= legended(sin(x), line_legend([red, blue], [\"a\", \"b\"]))\n";
        let nb = Notebook::parse(text).unwrap();
        let ctx = egui::Context::default();
        ctx.set_fonts(mc_ui::fonts::definitions(mc_ui::fonts::Source::Bundled));
        let images = Rc::new(RefCell::new(Images::new(&ctx)));
        let glyphs = Rc::new(RefCell::new(Glyphs::new(&ctx)));
        let defs = mc_ui::fonts::definitions(mc_ui::fonts::Source::Bundled);
        glyphs.borrow_mut().set_fonts(mc_ui::fonts::FontSet::new(&defs));
        let mut view = View::new(Palette::light(), images, glyphs);
        view.width = 700.0;
        view.set_outputs_right(true);
        let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
            let ctx = ui.ctx().clone();
            view.update_positions(&ctx, &nb, 0.0..4000.0);
            let (l, r) = view.content_x(Style::Output);
            let out = &nb.cells()[1];
            let lay = view.box_layout(&ctx, out);
            // one line holding the labeled box, ending at the right edge; the label below the value's baseline
            assert_eq!(lay.lines.len(), 1);
            assert!((lay.lines[0].right - (r - l)).abs() < 0.5, "{:?} {}", lay.lines, r - l);
            let char_at = |ch: char| {
                lay.items
                    .iter()
                    .find_map(|d| match d {
                        mc_boxes::layout::Draw::Char { c, x, y, .. } if *c == ch => Some((*x, *y)),
                        _ => None,
                    })
                    .unwrap()
            };
            let (_, vy) = char_at('x');
            let (ex, ey) = char_at('e');
            assert!(ey > vy + 10.0 && ex > (r - l) / 2.0, "{vy} {ex},{ey}");
            assert!((lay.baseline - vy).abs() < 0.5, "the value's baseline is the output's");
            // the label row in the label colour; the value's rows in the output's colour
            let colours = view.label_colours(out, &lay).expect("label colours");
            let label = lay.rows.iter().position(|g| g.path == [mc_boxes::Step { item: 0, slot: 1 }]).unwrap();
            assert_eq!(colours[label].len(), "energy".len());
            assert!(colours[label].iter().all(|&c| c == Palette::light().label()));
            assert!(colours[0].is_empty());
            // a click on the value's `x` puts the box caret into the value
            let o = view.content_origin(&nb, 1);
            let (vx, _) = char_at('x');
            let (path, _) = view.box_hit(&ctx, &nb, out.id, pos2(o.x + vx + 1.0, o.y + vy - 3.0)).unwrap();
            assert_eq!(path.first(), Some(&mc_boxes::Step { item: 0, slot: 0 }));
            // the legend: swatches in their colours, entries in the label colour
            let leg = &nb.cells()[3];
            let lay = view.box_layout(&ctx, leg);
            let colours = view.label_colours(leg, &lay).expect("legend colours");
            let label = lay.rows.iter().position(|g| g.path == [mc_boxes::Step { item: 0, slot: 1 }]).unwrap();
            let row = &colours[label];
            assert_eq!(row[0], Color32::from_rgb(255, 0, 0));
            assert_eq!(row[6], Color32::from_rgb(0, 0, 255));
            assert_eq!(row[3], Palette::light().label());
            // an output without labels keeps its one colour
            assert!(view.label_colours(&nb.cells()[0], &lay).is_none());
        });
    }
}
