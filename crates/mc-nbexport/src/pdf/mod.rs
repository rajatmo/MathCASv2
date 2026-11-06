//! The PDF export: the notebook's parts as blocks of lines, flowed onto A4 pages with margins and page numbers.
//!
//! A block is a list of lines, each with its drawing operations; a page break may come between any two lines of a
//! block (a long code cell or paragraph continues on the next page) but never inside one (a picture is one line).
//! A heading stays with the first line after it. Prose is wrapped at word boundaries in Noto Sans, code at the
//! character in DejaVu Sans Mono, and typeset content (outputs, typeset inputs, display formulas) is laid out by
//! `mc_boxes::layout` with the code font's metrics, then drawn glyph by glyph.
mod fonts;
mod writer;

use crate::model::{code_spans, first_title, parts, Out, Part};
use fonts::{fonts, CodeMetrics, Font, LINE_PITCH_CODE};
use mc_boxes::layout::{Draw, FontRole, LayoutOptions};
use mc_boxes::Row;
use mc_nbfile::images::{picture_size, resolve, ImageFormat};
use mc_nbfile::{Kind, Notebook};
use mc_syntax::TokenClass;
use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use writer::{Op, Picture, Rgb, Writer};

/// A4 portrait, in points: the paper size of most of the world (ISO 216); a Letter printer scales it to fit.
pub(crate) const PAGE_W: f32 = 595.28;
/// The page height.
pub(crate) const PAGE_H: f32 = 841.89;
/// Left and right margins (2 cm).
const MARGIN_X: f32 = 56.69;
/// Top margin.
const MARGIN_TOP: f32 = 56.69;
/// Bottom margin (above it the page number).
const MARGIN_BOTTOM: f32 = 62.0;
/// The width of the text column.
const WIDTH: f32 = PAGE_W - 2.0 * MARGIN_X;
/// Cells other than headings start this far right of the headings (as in the notebook).
const BODY: f32 = 22.0;
/// Pictures: a notebook point (`size=WxH`, a pixel at 100 %) is this many PDF points; the text sizes below are the
/// notebook's scaled by about the same factor.
const PICTURE_SCALE: f32 = 0.75;

const TEXT: Rgb = 0x383a42;
const WEAK: Rgb = 0x696c77;
const FAINT: Rgb = 0xa0a1a7;
const BORDER: Rgb = 0xdbdbdc;
const CODE_BG: Rgb = 0xf6f6f7;
const INIT_BG: Rgb = 0xeef1fb;
const FAILURE: Rgb = 0xca1243;
const NOTE: Rgb = 0x986801;

/// Code and output text size.
const CODE_SIZE: f32 = 9.0;
/// Prose size.
const PROSE_SIZE: f32 = 10.5;
/// Message size.
const MESSAGE_SIZE: f32 = 8.5;
/// A code cell of at most this many lines is kept on the page of its output's first line.
const KEEP_CODE_LINES: usize = 12;
/// Prose line pitch in ems.
const PROSE_PITCH: f32 = 1.38;

/// One line of a block: its height and its operations, relative to the line's top-left corner at the text
/// column's left edge.
struct Line {
    h: f32,
    ops: Vec<Op>,
}

/// A block: lines kept in order, spacing around it, a background behind it.
struct Block {
    lines: Vec<Line>,
    before: f32,
    after: f32,
    /// keep this block on the page of the next block's first line (headings)
    keep_next: bool,
    /// a background colour behind the lines, from `x` for `w` points, with `pad` above and below
    bg: Option<(Rgb, f32, f32, f32)>,
}

impl Block {
    fn new(lines: Vec<Line>, before: f32, after: f32) -> Block {
        Block { lines, before, after, keep_next: false, bg: None }
    }
}

/// The colour of a token class (the light theme).
fn token_colour(c: TokenClass) -> Rgb {
    use TokenClass::*;
    match c {
        Operator | Global | Placeholder => TEXT,
        Undefined => 0x4078f2,
        Local | PatternVar => 0x0184bc,
        Shadowing | UnmatchedBracket => 0xca1243,
        Keyword => 0xa626a4,
        Constant | Number => 0x986801,
        String => 0x50a14f,
        Comment => FAINT,
        SyntaxError => 0xe45649,
    }
}

/// Text in one font and colour, as pieces of a wrapped line.
#[derive(Clone)]
struct Run {
    text: String,
    font: Font,
    color: Rgb,
}

/// Prose wrapped at word boundaries into `width`: lines of positioned runs (x, run). Line breaks in the text are
/// kept; a word longer than a line is broken between characters.
fn wrap_prose(runs: &[Run], size: f32, width: f32) -> Vec<Vec<(f32, Run)>> {
    let f = fonts();
    let mut lines: Vec<Vec<(f32, Run)>> = vec![vec![]];
    let mut x = 0.0f32;
    let push = |lines: &mut Vec<Vec<(f32, Run)>>, x: f32, piece: &str, run: &Run| {
        let line = lines.last_mut().expect("a line");
        match line.last_mut() {
            Some((_, r)) if r.font == run.font && r.color == run.color => r.text.push_str(piece),
            _ => line.push((x, Run { text: piece.to_string(), ..run.clone() })),
        }
    };
    for run in runs {
        // pieces: words, single spaces, line breaks
        let mut pieces: Vec<&str> = Vec::new();
        let mut start = 0;
        for (i, c) in run.text.char_indices() {
            if c == ' ' || c == '\n' {
                if start < i {
                    pieces.push(&run.text[start..i]);
                }
                pieces.push(&run.text[i..i + 1]);
                start = i + 1;
            }
        }
        if start < run.text.len() {
            pieces.push(&run.text[start..]);
        }
        for piece in pieces {
            if piece == "\n" {
                lines.push(vec![]);
                x = 0.0;
                continue;
            }
            let w = f.width(run.font, piece) * size;
            if piece == " " {
                if x > 0.0 {
                    push(&mut lines, x, piece, run);
                    x += w;
                }
                continue;
            }
            if x + w > width && x > 0.0 {
                // drop the spaces the line ends with
                if let Some((_, r)) = lines.last_mut().and_then(|l| l.last_mut()) {
                    let t = r.text.trim_end_matches(' ').len();
                    r.text.truncate(t);
                }
                lines.push(vec![]);
                x = 0.0;
            }
            if w > width {
                for c in piece.chars() {
                    let cw = f.width(run.font, c.encode_utf8(&mut [0; 4])) * size;
                    if x + cw > width && x > 0.0 {
                        lines.push(vec![]);
                        x = 0.0;
                    }
                    push(&mut lines, x, c.encode_utf8(&mut [0; 4]), run);
                    x += cw;
                }
            } else {
                push(&mut lines, x, piece, run);
                x += w;
            }
        }
    }
    lines
}

/// Lines of wrapped runs as block lines at `x0`, `size` points, `pitch` ems apart.
fn run_lines(wrapped: Vec<Vec<(f32, Run)>>, x0: f32, size: f32, pitch: f32, font: Font) -> Vec<Line> {
    let (asc, desc) = fonts().ascent_descent(font);
    let baseline = size * (pitch + asc - desc) / 2.0;
    wrapped
        .into_iter()
        .map(|runs| Line {
            h: size * pitch,
            ops: runs
                .into_iter()
                .map(|(x, r)| Op::Text { x: x0 + x, y: baseline, size, font: r.font, color: r.color, text: r.text })
                .collect(),
        })
        .collect()
}

/// Prose with code spans as runs of `font` (code spans in the code font).
fn prose_runs(text: &str, font: Font, color: Rgb) -> Vec<Run> {
    code_spans(text)
        .into_iter()
        .map(|(t, code)| Run { text: t, font: if code { Font::Mono } else { font }, color })
        .collect()
}

/// Code with per-character colours wrapped at the character into `width` (tabs as four spaces).
fn wrap_code(chars: &[(char, Rgb)], font: Font, size: f32, width: f32) -> Vec<Vec<(f32, Run)>> {
    let f = fonts();
    let mut lines: Vec<Vec<(f32, Run)>> = vec![vec![]];
    let mut x = 0.0f32;
    for &(c, color) in chars {
        if c == '\n' {
            lines.push(vec![]);
            x = 0.0;
            continue;
        }
        let (c, n) = if c == '\t' { (' ', 4) } else { (c, 1) };
        for _ in 0..n {
            let w = f.width(font, c.encode_utf8(&mut [0; 4])) * size;
            if x + w > width + 0.01 && x > 0.0 {
                lines.push(vec![]);
                x = 0.0;
            }
            let line = lines.last_mut().expect("a line");
            match line.last_mut() {
                Some((_, r)) if r.color == color => r.text.push(c),
                _ => line.push((x, Run { text: c.to_string(), font, color })),
            }
            x += w;
        }
    }
    lines
}

/// The characters of `src` with their syntax colours.
fn coloured_chars(src: &str) -> Vec<(char, Rgb)> {
    let tokens = mc_syntax::classify(src);
    let mut k = 0;
    src.char_indices()
        .map(|(i, c)| {
            while k < tokens.len() && tokens[k].0.end <= i {
                k += 1;
            }
            let colour = match tokens.get(k) {
                Some((r, cls)) if r.start <= i => token_colour(*cls),
                _ => TEXT,
            };
            (c, colour)
        })
        .collect()
}

/// A typeset row laid out by `mc_boxes` in the code font, as block lines (right-justified with `right`).
fn typeset_lines(row: &Row, font: Font, color: Rgb, x0: f32, width: f32, output: bool, right: bool) -> Vec<Line> {
    let base = if output { LayoutOptions::output(CODE_SIZE, Some(width)) } else { LayoutOptions::input(CODE_SIZE, Some(width)) };
    let opts = LayoutOptions { operator_spacing: false, small_inline_rationals: false, ..base };
    let mut lay = mc_boxes::layout::layout(row, &CodeMetrics { font }, &opts);
    if right {
        lay.align_right(width);
    }
    // the first line starts at the layout's top (anything reaching above its line box stays on it)
    let mut tops: Vec<f32> = lay.lines.iter().map(|l| l.top).collect();
    match tops.first_mut() {
        Some(t) => *t = t.min(0.0),
        None => tops.push(0.0),
    }
    let mut lines: Vec<Line> = tops
        .iter()
        .enumerate()
        .map(|(i, &top)| {
            let end = tops.get(i + 1).copied().unwrap_or(lay.height.max(top));
            Line { h: (end - top).max(0.0), ops: vec![] }
        })
        .collect();
    let bottoms: Vec<f32> = lay.lines.iter().map(|l| l.bottom).collect();
    let line_of = |y: f32| bottoms.iter().position(|&b| y <= b + 0.01).unwrap_or(tops.len() - 1).min(tops.len() - 1);
    // consecutive characters of one font continue one text operation
    let mut last: Option<(usize, f32)> = None;
    for d in &lay.items {
        let (key, op, advance) = match d {
            Draw::Char { c, role, size, x, y, advance, .. } => {
                let f = if *role == FontRole::Math { Font::Math } else { font };
                (*y, Op::Text { x: *x, y: *y, size: *size, font: f, color, text: c.to_string() }, *advance)
            }
            Draw::Placeholder { x, y, size, .. } => {
                (*y, Op::Text { x: *x, y: *y, size: *size, font: Font::Math, color: FAINT, text: "\u{25a1}".into() }, 0.0)
            }
            Draw::Glyph { glyph, size, x, y } => (*y, Op::Glyph { x: *x, y: *y, size: *size, font: Font::Math, glyph: *glyph, color }, 0.0),
            Draw::Rule { x, y, w, h } => (*y + *h / 2.0, Op::Rect { x: *x, y: *y, w: *w, h: *h, color }, 0.0),
            Draw::Graphic { x, y, w, h, .. } => (*y, Op::Frame { x: *x, y: *y, w: *w, h: *h, color: FAINT }, 0.0),
        };
        let li = line_of(key);
        let top = tops[li];
        let op = op.shifted(x0, -top);
        let ops = &mut lines[li].ops;
        if let (Op::Text { x, y, size, font: f, text, .. }, Some((n, end))) = (&op, last) {
            if n == ops.len() && advance > 0.0 {
                if let Some(Op::Text { y: py, size: ps, font: pf, text: pt, color: pc, .. }) = ops.last_mut() {
                    if (*py - *y).abs() < 0.01 && *ps == *size && *pf == *f && *pc == color && (end - *x).abs() < 0.05 {
                        pt.push_str(text);
                        last = Some((n, x + advance));
                        continue;
                    }
                }
            }
        }
        let next_end = match &op {
            Op::Text { x, .. } if advance > 0.0 => Some(x + advance),
            _ => None,
        };
        ops.push(op);
        last = next_end.map(|e| (ops.len(), e));
    }
    lines
}

/// Decode a PNG file into a picture.
fn decode_png(bytes: &[u8]) -> Option<Picture> {
    let mut d = png::Decoder::new(std::io::Cursor::new(bytes));
    d.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut r = d.read_info().ok()?;
    let mut buf = vec![0; r.output_buffer_size()?];
    let info = r.next_frame(&mut buf).ok()?;
    let (cols, rows) = (info.width as usize, info.height as usize);
    let channels = match r.output_color_type().0 {
        png::ColorType::Grayscale => 1,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        png::ColorType::Indexed => return None,
    };
    let mut rgb = Vec::with_capacity(cols * rows * 3);
    let mut alpha = Vec::with_capacity(cols * rows);
    for row in buf.chunks(info.line_size).take(rows) {
        for px in row.chunks_exact(channels).take(cols) {
            match channels {
                1 => rgb.extend_from_slice(&[px[0]; 3]),
                2 => {
                    rgb.extend_from_slice(&[px[0]; 3]);
                    alpha.push(px[1]);
                }
                3 => rgb.extend_from_slice(px),
                _ => {
                    rgb.extend_from_slice(&px[..3]);
                    alpha.push(px[3]);
                }
            }
        }
    }
    if rgb.len() != cols * rows * 3 {
        return None;
    }
    let alpha = (alpha.len() == cols * rows && alpha.iter().any(|&a| a < 255)).then_some(alpha);
    Some(Picture { cols, rows, rgb, alpha })
}

/// A picture in the writer: its index and its size in pixels.
type Placed = (usize, (usize, usize));

/// Builds the blocks of a document, placing pictures in the writer.
struct Builder<'a> {
    base: &'a Path,
    writer: Writer,
    /// picture file to its index and natural size in pixels (`None`: not readable)
    pictures: HashMap<PathBuf, Option<Placed>>,
}

impl Builder<'_> {
    /// The picture at `path` (an SVG drawing through its PNG sibling): its index and size in pixels.
    fn picture(&mut self, path: &str) -> Option<Placed> {
        let mut file = resolve(path, Some(self.base));
        if ImageFormat::of(&file) == Some(ImageFormat::Svg) {
            file = file.with_extension("png");
        }
        if let Some(p) = self.pictures.get(&file) {
            return *p;
        }
        let found = std::fs::read(&file).ok().and_then(|b| decode_png(&b)).map(|p| {
            let size = (p.cols, p.rows);
            (self.writer.image(p), size)
        });
        self.pictures.insert(file, found);
        found
    }

    fn picture_block(&mut self, path: &str, size: Option<(u32, u32)>) -> Block {
        let width = WIDTH - BODY;
        let max_h = PAGE_H - MARGIN_TOP - MARGIN_BOTTOM - 8.0;
        let line = match self.picture(path) {
            Some((k, (cols, rows))) => {
                let file = resolve(path, Some(self.base));
                let (w, h) = size
                    .or_else(|| picture_size(&file))
                    .map(|(w, h)| (w as f32, h as f32))
                    .unwrap_or((cols as f32, rows as f32));
                let (mut w, mut h) = (w * PICTURE_SCALE, h * PICTURE_SCALE);
                let fit = (width / w).min(max_h / h).min(1.0);
                w *= fit;
                h *= fit;
                Line { h: h + 4.0, ops: vec![Op::Image { x: BODY + width - w, y: 2.0, w, h, image: k }] }
            }
            None => {
                let label = format!("picture not found: {path}");
                let lw = (fonts().width(Font::Prose, &label) * MESSAGE_SIZE + 24.0).min(width);
                let (w, h) = (lw.max(160.0), 48.0);
                let x = BODY + width - w;
                Line {
                    h: h + 4.0,
                    ops: vec![
                        Op::Frame { x, y: 2.0, w, h, color: FAINT },
                        Op::Text { x: x + 12.0, y: 2.0 + h / 2.0 + 3.0, size: MESSAGE_SIZE, font: Font::Prose, color: WEAK, text: label },
                    ],
                }
            }
        };
        Block::new(vec![line], 3.0, 3.0)
    }

    /// The blocks of one part.
    fn block(&mut self, part: Part) -> Block {
        let body = WIDTH - BODY;
        match part {
            Part::Heading { kind, text } => {
                let (size, font, before, after, x) = match kind {
                    Kind::Title => (24.0, Font::ProseBold, 18.0, 6.0, 0.0),
                    Kind::Subtitle => (16.0, Font::Prose, 2.0, 10.0, 0.0),
                    Kind::Subsubtitle => (12.5, Font::Prose, 2.0, 6.0, 0.0),
                    Kind::Chapter => (20.0, Font::ProseBold, 16.0, 6.0, 0.0),
                    Kind::Subchapter => (17.0, Font::ProseBold, 14.0, 5.0, 0.0),
                    Kind::Section => (16.0, Font::ProseBold, 14.0, 4.0, 0.0),
                    Kind::Subsection => (13.5, Font::ProseBold, 11.0, 4.0, 0.0),
                    _ => (12.0, Font::ProseBold, 8.0, 3.0, 0.0),
                };
                let mut lines = Vec::new();
                if kind == Kind::Section {
                    lines.push(Line { h: 7.0, ops: vec![Op::Rect { x, y: 0.0, w: WIDTH, h: 0.6, color: BORDER }] });
                }
                let wrapped = wrap_prose(&prose_runs(&text, font, TEXT), size, WIDTH - x);
                lines.extend(run_lines(wrapped, x, size, 1.25, font));
                Block { keep_next: true, ..Block::new(lines, before, after) }
            }
            Part::Para { text } => {
                let wrapped = wrap_prose(&prose_runs(&text, Font::Prose, TEXT), PROSE_SIZE, body);
                Block::new(run_lines(wrapped, BODY, PROSE_SIZE, PROSE_PITCH, Font::Prose), 5.0, 3.0)
            }
            Part::Item { level, number, bullet, text } => {
                let size = PROSE_SIZE - 0.5 * f32::from(level - 1);
                let x = BODY + 14.0 * f32::from(level - 1);
                let hang = 14.0f32.max(number.as_ref().map_or(0.0, |n| fonts().width(Font::Prose, n) * size + 4.0));
                let wrapped = wrap_prose(&prose_runs(&text, Font::Prose, TEXT), size, WIDTH - x - hang);
                let mut lines = run_lines(wrapped, x + hang, size, PROSE_PITCH, Font::Prose);
                let mark = match (number, bullet) {
                    (Some(n), _) => Some(n),
                    (None, true) => Some("\u{25aa}".to_string()),
                    _ => None,
                };
                if let (Some(m), Some(first)) = (mark, lines.first_mut()) {
                    let y = first.ops.first().map_or(size, |o| match o {
                        Op::Text { y, .. } => *y,
                        _ => size,
                    });
                    first.ops.push(Op::Text { x, y, size, font: Font::Prose, color: WEAK, text: m });
                }
                Block::new(lines, 2.0, 1.0)
            }
            Part::Formula { text } => {
                let row = mc_boxes::math_text(&text).row;
                Block::new(typeset_lines(&row, Font::Mono, TEXT, BODY, body, true, false), 5.0, 5.0)
            }
            Part::Program { text } => {
                let chars: Vec<(char, Rgb)> = text.chars().map(|c| (c, TEXT)).collect();
                let mut lines = vec![Line { h: 4.0, ops: vec![Op::Rect { x: BODY, y: 0.0, w: body, h: 0.6, color: BORDER }] }];
                lines.extend(run_lines(wrap_code(&chars, Font::Mono, CODE_SIZE, body), BODY, CODE_SIZE, LINE_PITCH_CODE, Font::Mono));
                lines.push(Line { h: 4.0, ops: vec![Op::Rect { x: BODY, y: 3.4, w: body, h: 0.6, color: BORDER }] });
                Block::new(lines, 6.0, 4.0)
            }
            Part::Code { source, init, typeset } => {
                let font = Font::MonoBold;
                let inner = body - 12.0;
                let lines = if typeset {
                    let row = mc_boxes::restrict(&mc_boxes::from_text(&source));
                    typeset_lines(&row, font, TEXT, BODY + 6.0, inner, false, false)
                } else {
                    let wrapped = wrap_code(&coloured_chars(&source), font, CODE_SIZE, inner);
                    run_lines(wrapped, BODY + 6.0, CODE_SIZE, LINE_PITCH_CODE, font)
                };
                let bg = if init { INIT_BG } else { CODE_BG };
                Block { bg: Some((bg, BODY, body, 3.0)), ..Block::new(lines, 8.0, 3.0) }
            }
            Part::Out(o) => match o {
                Out::Value(text) => {
                    let t = mc_boxes::math_text(&text);
                    if t.message {
                        return self.message(&text, FAILURE);
                    }
                    Block::new(typeset_lines(&t.row, Font::Mono, TEXT, BODY + 6.0, body - 12.0, true, true), 2.0, 2.0)
                }
                Out::Picture { path, size } => self.picture_block(&path, size),
                Out::Print(text) => {
                    let chars: Vec<(char, Rgb)> = text.chars().map(|c| (c, TEXT)).collect();
                    let wrapped = wrap_code(&chars, Font::Mono, CODE_SIZE, body - 12.0);
                    Block::new(run_lines(wrapped, BODY + 6.0, CODE_SIZE, LINE_PITCH_CODE, Font::Mono), 2.0, 2.0)
                }
                Out::Failure(text) => self.message(&text, FAILURE),
                Out::Note(text) => self.message(&text, NOTE),
            },
        }
    }

    /// A failure or message line in the prose font.
    fn message(&self, text: &str, color: Rgb) -> Block {
        let runs = [Run { text: text.to_string(), font: Font::Prose, color }];
        let wrapped = wrap_prose(&runs, MESSAGE_SIZE, WIDTH - BODY - 12.0);
        Block::new(run_lines(wrapped, BODY + 6.0, MESSAGE_SIZE, PROSE_PITCH, Font::Prose), 2.0, 2.0)
    }
}

/// Flows blocks onto pages.
struct Pager {
    pages: Vec<Vec<Op>>,
    ops: Vec<Op>,
    y: f32,
}

const TOP: f32 = MARGIN_TOP;
const BOTTOM: f32 = PAGE_H - MARGIN_BOTTOM;

impl Pager {
    fn new_page(&mut self) {
        self.pages.push(std::mem::take(&mut self.ops));
        self.y = TOP;
    }

    fn at_top(&self) -> bool {
        self.y <= TOP + 0.01
    }

    /// Place `b`; `next` is the height of the next block's first line (kept with a heading).
    fn place(&mut self, b: Block, next: f32) {
        let pad = b.bg.map_or(0.0, |bg| bg.3);
        let before = if self.at_top() { 0.0 } else { b.before };
        let first = b.lines.first().map_or(0.0, |l| l.h);
        let mut need = before + pad + first;
        if b.keep_next {
            need = before + 2.0 * pad + b.lines.iter().map(|l| l.h).sum::<f32>() + next;
        }
        if self.y + need > BOTTOM && !self.at_top() {
            self.new_page();
        }
        self.y += if self.at_top() { 0.0 } else { before };
        let mut frag = (self.ops.len(), self.y);
        self.y += pad;
        let n = b.lines.len();
        for (i, line) in b.lines.into_iter().enumerate() {
            let last = i + 1 == n;
            let room = if last { line.h + pad } else { line.h };
            if self.y + room > BOTTOM && self.y > frag.1 + pad + 0.01 {
                self.background(&b.bg, frag, self.y);
                self.new_page();
                frag = (0, self.y);
            }
            let top = self.y;
            self.ops.extend(line.ops.into_iter().map(|op| op.shifted(MARGIN_X, top)));
            self.y += line.h;
        }
        self.y += pad;
        self.background(&b.bg, frag, self.y);
        self.y += b.after;
    }

    /// Put the background of a block's fragment from `frag.1` to `end` behind its operations.
    fn background(&mut self, bg: &Option<(Rgb, f32, f32, f32)>, frag: (usize, f32), end: f32) {
        if let Some((color, x, w, _)) = *bg {
            let rect = Op::Rect { x: MARGIN_X + x, y: frag.1, w, h: end - frag.1, color };
            self.ops.insert(frag.0, rect);
        }
    }
}

/// The PDF document (see [`crate::to_pdf`]).
pub(crate) fn document(nb: &Notebook, base: &Path) -> io::Result<Vec<u8>> {
    let mut b = Builder { base, writer: Writer::new(PAGE_W, PAGE_H), pictures: HashMap::new() };
    let parts = parts(nb);
    let mut blocks: Vec<Block> = Vec::with_capacity(parts.len());
    for (i, p) in parts.iter().enumerate() {
        let code = matches!(p, Part::Code { .. });
        let mut block = b.block(p.clone());
        // a short code cell stays with the first line of its output
        if code && matches!(parts.get(i + 1), Some(Part::Out(_))) && block.lines.len() <= KEEP_CODE_LINES {
            block.keep_next = true;
        }
        blocks.push(block);
    }
    let mut pager = Pager { pages: vec![], ops: vec![], y: TOP };
    let mut blocks = blocks.into_iter().peekable();
    while let Some(block) = blocks.next() {
        let next = blocks.peek().and_then(|n| n.lines.first()).map_or(0.0, |l| l.h);
        pager.place(block, next);
    }
    if !pager.ops.is_empty() || pager.pages.is_empty() {
        pager.new_page();
    }
    let total = pager.pages.len();
    for (i, mut ops) in pager.pages.into_iter().enumerate() {
        let label = format!("{} / {total}", i + 1);
        let w = fonts().width(Font::Prose, &label) * 8.5;
        ops.push(Op::Text { x: (PAGE_W - w) / 2.0, y: PAGE_H - 30.0, size: 8.5, font: Font::Prose, color: WEAK, text: label });
        b.writer.page(&ops);
    }
    debug_assert_eq!(b.writer.pages(), total);
    Ok(b.writer.finish(first_title(nb).as_deref()))
}

/// Whether the prose font draws `c` itself (tests).
#[cfg(test)]
pub(crate) fn has_prose_glyph(c: char) -> bool {
    fonts().has(Font::Prose, c).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(text: &str, font: Font) -> Run {
        Run { text: text.into(), font, color: TEXT }
    }

    #[test]
    fn prose_wraps_at_words_and_keeps_line_breaks() {
        let lines = wrap_prose(&[run("aaa bbb ccc\nddd", Font::Prose)], 10.0, fonts().width(Font::Prose, "aaa bbb ") * 10.0);
        let texts: Vec<String> = lines.iter().map(|l| l.iter().map(|(_, r)| r.text.clone()).collect()).collect();
        assert_eq!(texts, ["aaa bbb", "ccc", "ddd"]);
    }

    #[test]
    fn code_wraps_at_characters() {
        let chars: Vec<(char, Rgb)> = "abcdefghij".chars().map(|c| (c, TEXT)).collect();
        let w = fonts().width(Font::Mono, "abcd") * 10.0;
        let lines = wrap_code(&chars, Font::Mono, 10.0, w);
        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn typeset_output_draws_a_rule_for_a_fraction() {
        let row = mc_boxes::math_text("1/3").row;
        let lines = typeset_lines(&row, Font::Mono, TEXT, 0.0, 300.0, true, true);
        assert!(lines.iter().flat_map(|l| &l.ops).any(|o| matches!(o, Op::Rect { .. })));
        let row = mc_boxes::math_text("sqrt(x)").row;
        let lines = typeset_lines(&row, Font::Mono, TEXT, 0.0, 300.0, true, true);
        assert!(lines.iter().flat_map(|l| &l.ops).any(|o| matches!(o, Op::Glyph { .. } | Op::Rect { .. })));
    }
}
