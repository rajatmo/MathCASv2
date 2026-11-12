//! Outputs drawn as the notebook draws them, for other windows (round 19: the documentation window shows its pages'
//! outputs through this, so a page and a cell look alike). The printed text goes through `mc_boxes::math_text` (square
//! roots, fractions and powers as boxes, every other character as printed), is laid out with the notebook's output
//! options in the cell font, and painted by the notebook's painter.
use crate::theme::Palette;
use crate::typeset::{self, Glyphs, Ink, TextFont};
use eframe::egui::{self, Color32, Rect, Sense};
use mc_boxes::layout::{Layout, LayoutOptions};
use mc_ui::fonts::{Face, FontSet};
use std::collections::HashMap;
use std::sync::Arc;

/// Text with more bytes than this stays linear (as the notebook's synchronous limit).
const TYPESET_MAX: usize = 3_000;

/// The typeset outputs of a window: the glyph atlas and the layouts of the texts shown.
pub struct OutputPainter {
    glyphs: Glyphs,
    layouts: HashMap<(String, u32, u32, u32), Arc<Layout>>,
    frame_used: u64,
    /// whether a text is drawn typeset
    boxed: HashMap<String, bool>,
}

impl OutputPainter {
    /// A painter for the window of `ctx`, with the fonts `defs` installed there.
    pub fn new(ctx: &egui::Context, defs: &egui::FontDefinitions) -> OutputPainter {
        let mut glyphs = Glyphs::new(ctx);
        glyphs.set_fonts(FontSet::new(defs));
        OutputPainter { glyphs, layouts: HashMap::new(), frame_used: 0, boxed: HashMap::new() }
    }

    /// Call once a frame before drawing.
    pub fn begin_frame(&mut self) {
        self.glyphs.begin_frame();
        self.frame_used += 1;
        if self.layouts.len() > 2_000 {
            self.layouts.clear();
        }
        if self.boxed.len() > 20_000 {
            self.boxed.clear();
        }
    }

    /// Whether `printed` is drawn typeset (it holds a root, fraction or power and is not too long); otherwise the
    /// caller shows it as text.
    pub fn typesets(&mut self, printed: &str) -> bool {
        if let Some(&t) = self.boxed.get(printed) {
            return t;
        }
        let t = printed.len() <= TYPESET_MAX
            && !printed.starts_with("failure(")
            && !printed.starts_with("image(")
            && mc_boxes::has_boxes(&mc_boxes::math_text(printed).row);
        self.boxed.insert(printed.to_string(), t);
        t
    }

    /// Show `printed` typeset at `size` points in the code face, at most `width` wide; the response covers it.
    pub fn show(&mut self, ui: &mut egui::Ui, printed: &str, size: f32, color: Color32, p: &Palette) -> egui::Response {
        let width = ui.available_width().max(40.0);
        let (fonts, generation) = self.glyphs.face(Face::Code);
        let key = (printed.to_string(), size.to_bits(), width.to_bits(), generation);
        let lay = match self.layouts.get(&key) {
            Some(l) => l.clone(),
            None => {
                let font = TextFont::new(ui.ctx(), Face::Code, fonts, size);
                let opts = LayoutOptions {
                    operator_spacing: false,
                    small_inline_rationals: false,
                    ..LayoutOptions::output(size, Some(width))
                };
                let row = mc_boxes::math_text(printed).row;
                let l = Arc::new(typeset::lay_out(&row, &opts, &font));
                self.layouts.insert(key, l.clone());
                l
            }
        };
        let (rect, resp) = ui.allocate_exact_size(egui::vec2(lay.width.max(1.0), lay.height), Sense::hover());
        if ui.is_rect_visible(rect) {
            let ink = Ink::new(p, color, Face::Code);
            let clip = ui.clip_rect().intersect(Rect::EVERYTHING);
            typeset::paint(ui.painter(), &mut self.glyphs, &lay, rect.min, clip, &ink);
        }
        resp
    }
}
