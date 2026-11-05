//! The notebook canvas (design 9.6): per-cell layout cached by revision, cell positions as prefix sums, painting
//! of the visible cells only (labels, brackets, the insertion point, the caret and selections), and hit-testing.
use crate::fonts::Face;
use crate::highlight;
use crate::theme::Palette;
use eframe::egui::{self, pos2, vec2, Align2, Color32, Galley, Painter, Pos2, Rect, Stroke};
use eframe::epaint::text::{cursor::CCursor, LayoutJob, TextFormat};
use mc_nbmodel::{Bracket, Cell, CellId, EvalMark, FontRole, Notebook, Selection, Style};
use mc_syntax::Colouring;
use std::collections::HashMap;
use std::collections::HashSet;
use std::ops::Range;
use std::sync::Arc;

/// Space above the first cell and the column pitch of brackets, in points at 100 %.
const TOP_PAD: f32 = 8.0;
/// The distance between bracket columns.
pub const BRACKET_PITCH: f32 = 7.0;
/// The length of a bracket's hooks.
const HOOK: f32 = 3.0;
/// The gap between the content's right edge and the first bracket column.
const BRACKET_GAP: f32 = 10.0;
/// Off-screen cells laid out per frame to refine their estimated heights.
const REFINE_PER_FRAME: usize = 64;

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
    key: (u64, u32, u32, bool),
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
}

/// The canvas state that lives across frames.
pub struct View {
    laid: HashMap<CellId, Laid>,
    /// cell tops relative to the canvas top, and the total height
    tops: Vec<f32>,
    heights: Vec<f32>,
    /// the magnification (1.0 = 100 %)
    pub zoom: f32,
    /// the canvas width of the last frame
    pub width: f32,
    /// the canvas's top-left of the last frame (screen coordinates)
    pub origin: Pos2,
    /// the palette
    pub palette: Palette,
    /// the time the caret last moved (blink phase)
    pub blink_from: f64,
    /// the bracket under the pointer
    pub hover: Option<Bracket>,
    /// an insertion line to draw while cells are dragged
    pub drop_gap: Option<usize>,
    /// the largest bracket column of the groups
    max_col: usize,
    /// the caret, for bracket matching
    pub caret: Option<(CellId, usize)>,
    /// the names the kernel defines (`None`: no kernel, every global name is plain)
    defined: Option<Arc<HashSet<String>>>,
    defined_rev: u64,
    colours: HashMap<CellId, ((u64, u64), Arc<Colouring>)>,
}

impl View {
    /// An empty view.
    pub fn new(palette: Palette) -> View {
        View {
            laid: HashMap::new(),
            tops: vec![],
            heights: vec![],
            zoom: 1.0,
            width: 800.0,
            origin: Pos2::ZERO,
            palette,
            blink_from: 0.0,
            hover: None,
            drop_gap: None,
            max_col: 0,
            caret: None,
            defined: None,
            defined_rev: 0,
            colours: HashMap::new(),
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
    }

    /// The x of bracket column 0 (relative to the canvas), for `max_col` group columns.
    fn bracket_base(&self, max_col: usize) -> f32 {
        self.width - 8.0 - BRACKET_PITCH * max_col as f32
    }

    /// The left and right edges of a cell's content (relative to the canvas).
    pub fn content_x(&self, style: Style) -> (f32, f32) {
        let m = style.info().margins;
        let max_col = self.max_col;
        let left = m[0] * self.zoom;
        let right = (self.bracket_base(max_col) - BRACKET_GAP - m[1].min(10.0)).max(left + 40.0);
        (left, right)
    }

    /// The layout of a cell's content at the current width (egui caches it).
    pub fn galley(&mut self, ctx: &egui::Context, cell: &Cell) -> Arc<Galley> {
        let (l, r) = self.content_x(cell.style);
        let wrap = r - l;
        let key = (cell.rev, wrap.to_bits(), self.zoom.to_bits(), self.palette.dark);
        let lk = look(cell, &self.palette);
        let mut job = LayoutJob::default();
        let format =
            TextFormat { font_id: lk.face.font(lk.size * self.zoom), color: lk.color, ..TextFormat::default() };
        if cell.style.is_code() && !cell.expr_view {
            self.append_code(&mut job, cell, &format);
        } else {
            job.append(&cell.text, 0.0, format);
        }
        job.wrap.max_width = wrap;
        job.wrap.break_anywhere = false;
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
        let (l, r) = self.content_x(cell.style);
        let key = (cell.rev, (r - l).to_bits(), self.zoom.to_bits(), self.palette.dark);
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
        self.max_col = nb.groups().list.iter().map(|g| g.column).max().unwrap_or(0);
        self.tops.resize(n, 0.0);
        self.heights.resize(n, 0.0);
        let mut y = TOP_PAD * self.zoom;
        let mut refined = 0;
        for i in 0..n {
            let c = &nb.cells()[i];
            let content = if !nb.is_visible(i) || c.props.hidden {
                0.0
            } else if let Some(h) = self.cached_height(c) {
                h
            } else if y < visible.end + 200.0 && y + 400.0 > visible.start || refined < REFINE_PER_FRAME {
                refined += 1;
                self.galley(ctx, c).size().y
            } else {
                let lines = c.text.split('\n').count() as f32;
                lines * look(c, &self.palette).size * self.zoom * 1.25
            };
            self.tops[i] = y;
            self.heights[i] = self.cell_height(nb, i, content);
            y += self.heights[i];
        }
        if refined >= REFINE_PER_FRAME {
            ctx.request_repaint();
        }
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
        let g = self.galley(ctx, c);
        let chars = c.text[..pos.min(c.text.len())].chars().count();
        let r = g.pos_from_cursor(CCursor::new(chars));
        Some(r.translate(self.content_origin(nb, i).to_vec2()))
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
        let max_col = self.max_col;
        let base = self.bracket_base(max_col);
        if p.x >= base - HOOK - 3.0 {
            let col = ((p.x - base) / BRACKET_PITCH).round().max(0.0) as usize;
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
        // the visible cell under the point
        let i = (0..n).find(|&i| nb.is_visible(i) && p.y >= self.tops[i] && p.y < self.tops[i] + self.heights[i]);
        let Some(i) = i else { return Hit::Gap(if p.y < TOP_PAD * self.zoom { 0 } else { n }) };
        let c = &nb.cells()[i];
        let m = c.style.info().margins;
        let content_top = self.tops[i] + m[2] * self.zoom;
        let g = self.galley(ctx, c);
        let content_bottom = content_top + g.size().y;
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
        let cur = g.cursor_from_pos(p - origin);
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
        let label_font = Face::ProseItalic.font(10.5 * z);
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
            let g = self.galley(ctx, c);
            // backgrounds and rules
            if c.style == Style::Code && !c.expr_view {
                let bg = Rect::from_min_max(
                    pos2(content.x - 8.0 * z, r.top() + 2.0),
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
                // the Section rule runs across the window, the Program rules across the content
                let from = if c.style == Style::Program { content.x } else { origin.x + 6.0 };
                painter.hline(from..=origin.x + rr, y, Stroke::new(1.0, p.border));
                if c.style == Style::Program {
                    painter.hline(content.x..=origin.x + rr, r.bottom() - 3.0, Stroke::new(1.0, p.border));
                }
            }
            // the text selection
            if let Selection::Caret { cell, anchor, pos } = sel {
                if cell == c.id && anchor != pos {
                    paint_selection(
                        painter,
                        &g,
                        &c.text,
                        content,
                        anchor.min(pos),
                        anchor.max(pos),
                        p.text_selection(),
                    );
                }
            }
            painter.galley(content, g.clone(), p.text);
            // bullets and numbers of items
            if info.bullet || c.style.numbering_level().is_some() {
                let row_h = g.rows.first().map_or(info.size * z, |r| r.rect().height());
                let mark = if info.bullet { "■".to_string() } else { nb.item_number(i).unwrap_or_default() };
                let font = Face::Prose.font(info.size * z * if info.bullet { 1.1 } else { 1.0 });
                painter.text(pos2(content.x - 7.0 * z, top + row_h * 0.52), Align2::RIGHT_CENTER, mark, font, p.red);
            }
            // labels
            let label = match (c.style, c.eval, c.label) {
                (_, EvalMark::Running, _) if c.evaluatable() => Some("In[*]:=".to_string()),
                (Style::Output, _, Some(n)) => Some(format!("Out[{n}]=")),
                (_, _, Some(n)) if c.style.info().label => Some(format!("In[{n}]:=")),
                _ => None,
            };
            if let Some(label) = label {
                let lg = painter.layout_no_wrap(label, label_font.clone(), p.gutter);
                let row = g.rows.first().map_or(lg.size().y, |r| r.rect().height());
                let y = top + row * 0.78 - lg.size().y * 0.78;
                painter.galley(pos2(content.x - 6.0 * z - lg.size().x, y), lg, p.gutter);
            }
            // the caret
            if let Selection::Caret { cell, pos, .. } = sel {
                if cell == c.id && ((time - self.blink_from) % 1.0) < 0.6 {
                    let chars = c.text[..pos.min(c.text.len())].chars().count();
                    let cr = g.pos_from_cursor(CCursor::new(chars)).translate(content.to_vec2());
                    let x = cr.left().round();
                    painter.vline(x, cr.top()..=cr.bottom(), Stroke::new(2.0, p.cursor));
                }
            }
        }
        self.paint_brackets(painter, nb, origin, clip, &selected);
        // the insertion point
        let gap = self.drop_gap.or(match sel {
            Selection::Gap(g) => Some(g),
            _ => None,
        });
        if let Some(g) = gap {
            let y = origin.y + self.gap_y(g);
            let max_col = self.max_col;
            let right = origin.x + self.bracket_base(max_col) - 6.0;
            painter.hline(
                origin.x + 22.0..=right,
                y,
                Stroke::new(1.5, if self.drop_gap.is_some() { p.accent } else { p.weak }),
            );
            if self.drop_gap.is_none() {
                let center = pos2(origin.x + 12.0, y);
                painter.circle_stroke(center, 6.0, Stroke::new(1.0, p.gutter));
                painter.hline(center.x - 3.0..=center.x + 3.0, y, Stroke::new(1.0, p.gutter));
                painter.vline(center.x, y - 3.0..=y + 3.0, Stroke::new(1.0, p.gutter));
                if nb.gap_style() != Style::Input {
                    let font = Face::Ui.font(10.0);
                    painter.text(
                        pos2(origin.x + 24.0, y - 2.0),
                        Align2::LEFT_BOTTOM,
                        nb.gap_style().info().name,
                        font,
                        p.weak,
                    );
                }
            }
        }
    }

    fn paint_brackets(&self, painter: &Painter, nb: &Notebook, origin: Pos2, clip: Rect, selected: &[CellId]) {
        let p = &self.palette;
        let max_col = self.max_col;
        let base = origin.x + self.bracket_base(max_col);
        for (col, top, bottom, b) in self.brackets(nb) {
            let (top, bottom) = (origin.y + top + 1.5, origin.y + bottom - 1.5);
            if bottom < clip.top() || top > clip.bottom() {
                continue;
            }
            let x = base + BRACKET_PITCH * col as f32;
            let (cells, head): (Vec<usize>, Option<usize>) = match b {
                Bracket::Cell(id) => (nb.index_of(id).into_iter().collect(), None),
                Bracket::Group(s, e) => ((s..e).collect(), Some(s)),
            };
            let is_selected = !selected.is_empty() && cells.iter().all(|&i| selected.contains(&nb.cells()[i].id));
            let mark = match b {
                Bracket::Cell(id) => nb.cell(id).map_or(EvalMark::None, |c| c.eval),
                Bracket::Group(..) => EvalMark::None,
            };
            let (color, width) = match mark {
                EvalMark::Running => (p.accent, 2.0),
                EvalMark::Queued => (p.yellow, 1.5),
                EvalMark::Error => (p.red, 1.5),
                EvalMark::None if self.hover == Some(b) => (p.accent, 1.0),
                EvalMark::None => (p.gutter, 1.0),
            };
            if is_selected {
                let fill = Rect::from_min_max(pos2(x - HOOK - 1.0, top), pos2(x + 1.5, bottom));
                painter.rect_filled(fill, 0.0, p.over_bg(p.accent, 0.35));
            }
            let stroke = Stroke::new(width, if is_selected { p.accent } else { color });
            painter.vline(x, top..=bottom, stroke);
            painter.hline(x - HOOK..=x, top, stroke);
            painter.hline(x - HOOK..=x, bottom, stroke);
            let closed = head.is_some_and(|h| nb.cells()[h].closed.is_some());
            if closed {
                let tri = vec![
                    pos2(x - HOOK - 1.0, bottom - 1.0),
                    pos2(x + 1.0, bottom - 1.0),
                    pos2(x + 1.0, bottom - HOOK - 3.0),
                ];
                painter.add(egui::Shape::convex_polygon(tri, stroke.color, Stroke::NONE));
            }
        }
    }

    /// The visual lines of a cell's content as byte ranges.
    pub fn lines(&mut self, ctx: &egui::Context, cell: &Cell) -> Vec<Range<usize>> {
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
        let g = self.galley(ctx, cell);
        let chars = cell.text[..pos.min(cell.text.len())].chars().count();
        g.pos_from_cursor(CCursor::new(chars)).left()
    }

    /// The position in `line` nearest to `x`.
    pub fn pos_at(&mut self, ctx: &egui::Context, cell: &Cell, line: Range<usize>, x: f32) -> usize {
        let g = self.galley(ctx, cell);
        let chars = cell.text[..line.start].chars().count();
        let y = g.pos_from_cursor(CCursor::new(chars)).center().y;
        let cur = g.cursor_from_pos(vec2(x, y));
        let byte = cell.text.char_indices().nth(cur.index.0).map_or(cell.text.len(), |(b, _)| b);
        byte.clamp(line.start, line.end)
    }
}

/// Paint a selection highlight behind characters `a..b` (bytes) of a galley.
fn paint_selection(painter: &Painter, g: &Galley, text: &str, origin: Pos2, a: usize, b: usize, color: Color32) {
    let ca = text[..a].chars().count();
    let cb = text[..b].chars().count();
    let mut idx = 0;
    for row in &g.rows {
        let n = row.row.glyphs.len();
        let (rs, re) = (idx, idx + n);
        let (s, e) = (ca.max(rs), cb.min(re));
        if s < e || (s == e && row.ends_with_newline && ca <= re && cb > re) {
            let x0 = if s > rs { row.row.glyphs[s - rs].pos.x } else { 0.0 };
            let x1 = if e < re {
                row.row.glyphs[e - rs].pos.x
            } else {
                row.rect().width() + if cb > re { 4.0 } else { 0.0 }
            };
            let rr = Rect::from_min_max(pos2(x0, row.rect().top()), pos2(x1.max(x0 + 2.0), row.rect().bottom()));
            painter.rect_filled(rr.translate(origin.to_vec2() + vec2(row.pos.x, 0.0)), 0.0, color);
        }
        idx = re + usize::from(row.ends_with_newline);
    }
}
