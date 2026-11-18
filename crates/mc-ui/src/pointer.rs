//! The text pointer (the I-beam over text), drawn by the window itself.
//!
//! The system's I-beam takes its colour from what is under it by inverting, and on some systems (a display in HDR
//! mode, some graphics drivers) that inversion is lost over a window drawn by the GPU: the pointer stays white and
//! cannot be seen on a light page. So wherever a frame asks for the text pointer, the system pointer is hidden
//! and an I-beam is painted at its place in the theme's text colour with a halo of the page colour, which is
//! visible on any page in both themes.
use eframe::egui::{self, pos2, Color32, CursorIcon, Id, LayerId, Order, Stroke};

/// Half the height of the I-beam and half the width of its serifs, in points.
const HALF_HEIGHT: f32 = 8.0;
const SERIF: f32 = 2.5;

/// Call at the end of a frame: when the frame asked for the text pointer, hide the system's and paint an I-beam
/// in `ink` with a halo in `page` where the pointer is. Nothing happens for touch input (no pointer to show) or
/// while the pointer is outside the window.
pub fn paint_text_pointer(ctx: &egui::Context, ink: Color32, page: Color32) {
    if ctx.output(|o| o.cursor_icon) != CursorIcon::Text {
        return;
    }
    let Some(p) = ctx.input(|i| if i.any_touches() { None } else { i.pointer.latest_pos() }) else { return };
    ctx.set_cursor_icon(CursorIcon::None);
    let painter = ctx.layer_painter(LayerId::new(Order::Tooltip, Id::unique("text-pointer")));
    let ppp = ctx.pixels_per_point();
    // whole pixels, the stem through a pixel's centre: a crisp one-pixel line at every scale
    let snap = |v: f32| ((v * ppp).floor() + 0.5) / ppp;
    let (x, y) = (snap(p.x), snap(p.y));
    let (top, bottom) = (snap(y - HALF_HEIGHT), snap(y + HALF_HEIGHT));
    let lines = [
        [pos2(x, top), pos2(x, bottom)],
        [pos2(x - SERIF, top), pos2(x + SERIF, top)],
        [pos2(x - SERIF, bottom), pos2(x + SERIF, bottom)],
    ];
    let thin = (1.0 / ppp).max(1.0);
    for l in lines {
        painter.line_segment(l, Stroke::new(thin + 2.0, page));
    }
    for l in lines {
        painter.line_segment(l, Stroke::new(thin, ink));
    }
}
