//! The icons of the windows: drawn in TikZ (`assets/icons/src/<name>.tex`, one family on a 24 x 24 canvas, one colour),
//! generated to SVG by `tools/icons.ps1` and compiled in. Each icon is rasterised with resvg at the size and display
//! scale it is drawn at, as white coverage, and uploaded once as a texture per size (at most [`MAX_TEXTURES`] are
//! kept); painting tints it with the theme's colour for its state (normal, hovered, disabled, accent).
use eframe::egui::{self, pos2, Color32, ColorImage, Rect, TextureHandle, TextureOptions, Vec2};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

macro_rules! icons {
    ($($variant:ident => $file:literal, $what:literal;)*) => {
        /// An icon of the set.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub enum Icon {
            $(#[doc = $what] $variant,)*
        }

        impl Icon {
            /// Every icon.
            pub const ALL: &'static [Icon] = &[$(Icon::$variant,)*];

            /// The icon's name (its source `assets/icons/src/<name>.tex`).
            pub fn name(self) -> &'static str {
                match self {
                    $(Icon::$variant => $file,)*
                }
            }

            /// The icon's SVG.
            pub fn svg(self) -> &'static [u8] {
                match self {
                    $(Icon::$variant => include_bytes!(concat!("../../../assets/icons/", $file, ".svg")),)*
                }
            }
        }
    };
}

icons! {
    Evaluate => "evaluate", "evaluate: a play triangle";
    Abort => "abort", "abort: a stop square";
    ThemeLight => "theme-light", "the light theme: a sun";
    ThemeDark => "theme-dark", "the dark theme: a crescent";
    ThemeSystem => "theme-system", "the system's theme: a half-filled disc";
    Zoom => "zoom", "magnification: a magnifier with a plus";
    Search => "search", "search: a magnifier";
    Plus => "plus", "a new notebook: a plus";
    Close => "close", "close: a cross";
    Dot => "dot", "modified: a dot";
    Kernel => "kernel", "the kernel's state: a ring with a dot";
    ChevronLeft => "chevron-left", "scroll left";
    ChevronRight => "chevron-right", "scroll right, a submenu";
    ChevronDown => "chevron-down", "a list";
    ArrowUp => "arrow-up", "the previous match";
    ArrowDown => "arrow-down", "the next match";
    ArrowLeft => "arrow-left", "back";
    ArrowRight => "arrow-right", "forward";
    Home => "home", "home: a house";
    Spinner => "spinner", "busy: three quarters of a ring (turned while it spins)";
    Insert => "insert", "the insertion point: a plus in a ring";
    Check => "check", "a check mark";
    Info => "info", "information";
    Warning => "warning", "a warning";
    Error => "error", "an error";
    Notebook => "notebook", "open as a notebook";
    Copy => "copy", "copy: a page in front of another";
}

/// Textures kept at most (icons times sizes): beyond that the cache starts again.
pub const MAX_TEXTURES: usize = 256;

/// The window's icon textures by icon and pixel size.
#[derive(Default)]
struct Cache {
    textures: HashMap<(Icon, u32), TextureHandle>,
}

fn cache(ctx: &egui::Context) -> Arc<Mutex<Cache>> {
    let id = egui::Id::unique("mc-ui-icons");
    ctx.data_mut(|d| d.get_temp_mut_or_insert_with(id, || Arc::new(Mutex::new(Cache::default()))).clone())
}

/// The icon as white coverage on a transparent square of `px` pixels (`None` when the SVG does not parse).
pub fn rasterize(icon: Icon, px: u32) -> Option<ColorImage> {
    let px = px.clamp(1, 1024);
    let tree = resvg::usvg::Tree::from_data(icon.svg(), &resvg::usvg::Options::default()).ok()?;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(px, px)?;
    let size = tree.size();
    let scale = px as f32 / size.width().max(size.height());
    resvg::render(&tree, resvg::tiny_skia::Transform::from_scale(scale, scale), &mut pixmap.as_mut());
    // the icons are one colour: keep the coverage, so a tint gives the colour
    let pixels = pixmap.pixels().iter().map(|p| Color32::from_white_alpha(p.alpha())).collect();
    Some(ColorImage::new([px as usize, px as usize], pixels))
}

/// The texture of `icon` for `size` points at the display's scale, uploaded on first use.
pub fn texture(ctx: &egui::Context, icon: Icon, size: f32) -> Option<egui::TextureId> {
    let px = (size * ctx.pixels_per_point()).round().max(1.0) as u32;
    let cache = cache(ctx);
    let mut c = cache.lock().ok()?;
    if let Some(t) = c.textures.get(&(icon, px)) {
        return Some(t.id());
    }
    if c.textures.len() >= MAX_TEXTURES {
        c.textures.clear();
    }
    let img = rasterize(icon, px)?;
    let t = ctx.load_texture(format!("mc-icon-{}-{px}", icon.name()), img, TextureOptions::LINEAR);
    let id = t.id();
    c.textures.insert((icon, px), t);
    Some(id)
}

/// The icon as an egui image of `size` points, tinted `tint` (for buttons and labels).
pub fn image(ctx: &egui::Context, icon: Icon, size: f32, tint: Color32) -> egui::Image<'static> {
    let id = texture(ctx, icon, size).unwrap_or_default();
    egui::Image::new(egui::load::SizedTexture::new(id, Vec2::splat(size))).tint(tint)
}

/// Paint the icon in `rect` (a square, on whole device pixels so it stays crisp), tinted `tint`.
pub fn paint(painter: &egui::Painter, icon: Icon, rect: Rect, tint: Color32) {
    let ppp = painter.pixels_per_point();
    let side = rect.width().min(rect.height());
    let snap = |v: f32| (v * ppp).round() / ppp;
    let r = Rect::from_center_size(rect.center(), Vec2::splat(side));
    let r = Rect::from_min_size(pos2(snap(r.left()), snap(r.top())), Vec2::splat(side));
    if let Some(id) = texture(painter.ctx(), icon, side) {
        painter.image(id, r, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), tint);
    }
}

/// Paint the icon turned by `angle` radians about its centre (the busy spinner).
pub fn paint_turned(painter: &egui::Painter, icon: Icon, rect: Rect, tint: Color32, angle: f32) {
    let side = rect.width().min(rect.height());
    if let Some(id) = texture(painter.ctx(), icon, side) {
        let r = Rect::from_center_size(rect.center(), Vec2::splat(side));
        let mut mesh = egui::Mesh::with_texture(id);
        mesh.add_rect_with_uv(r, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), tint);
        mesh.rotate(egui::emath::Rot2::from_angle(angle), r.center());
        painter.add(egui::Shape::mesh(mesh));
    }
}

/// The open/closed marker of a collapsing header (egui's `CollapsingHeader::icon`): the chevron turning from right
/// (closed) to down (open) with `openness`.
pub fn collapsing_marker(ui: &mut egui::Ui, openness: f32, response: &egui::Response) {
    let visuals = ui.style().interact(response);
    let rect = Rect::from_center_size(response.rect.left_center() + egui::vec2(ui.spacing().icon_width / 2.0, 0.0), Vec2::splat(13.0));
    let angle = openness * std::f32::consts::FRAC_PI_2;
    paint_turned(ui.painter(), Icon::ChevronRight, rect, visuals.fg_stroke.color, angle);
}

/// The drop-down marker of a combo box (egui's `ComboBox::icon`): the chevron, pointing up while open.
pub fn combo_marker(ui: &egui::Ui, rect: Rect, visuals: &egui::style::WidgetVisuals, open: bool) {
    let r = Rect::from_center_size(rect.center(), Vec2::splat(rect.height().clamp(10.0, 14.0)));
    let angle = if open { std::f32::consts::PI } else { 0.0 };
    paint_turned(ui.painter(), Icon::ChevronDown, r, visuals.fg_stroke.color, angle);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn assets() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/icons")
    }

    /// Every TikZ source has its generated SVG and every icon of the set has a source (tools/icons.ps1 was run).
    #[test]
    fn every_source_has_its_svg_and_every_icon_its_source() {
        let mut sources: Vec<String> = std::fs::read_dir(assets().join("src"))
            .expect("assets/icons/src")
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter_map(|n| n.strip_suffix(".tex").map(str::to_string))
            .filter(|n| n != "icon-style")
            .collect();
        sources.sort();
        let mut names: Vec<String> = Icon::ALL.iter().map(|i| i.name().to_string()).collect();
        names.sort();
        assert_eq!(sources, names, "the icon list and assets/icons/src differ");
        for n in &sources {
            assert!(assets().join(format!("{n}.svg")).is_file(), "{n}.svg is missing: run tools/icons.ps1");
        }
    }

    /// Every SVG parses with resvg, is square and has ink inside its canvas.
    #[test]
    fn every_icon_parses_and_has_ink() {
        for &i in Icon::ALL {
            let tree = resvg::usvg::Tree::from_data(i.svg(), &resvg::usvg::Options::default())
                .unwrap_or_else(|e| panic!("{}: {e}", i.name()));
            let s = tree.size();
            assert!((s.width() - s.height()).abs() < 0.1, "{} is not square: {s:?}", i.name());
            let img = rasterize(i, 48).expect("rasterised");
            let ink = img.pixels.iter().filter(|p| p.a() > 128).count();
            assert!(ink > 20 && ink < 48 * 48 * 3 / 4, "{}: {ink} pixels of ink", i.name());
            // nothing touches the canvas edge (the family keeps a margin)
            let edge = (0..48).any(|k| img.pixels[k].a() > 200 || img.pixels[47 * 48 + k].a() > 200);
            assert!(!edge, "{} touches the edge", i.name());
        }
    }
}
