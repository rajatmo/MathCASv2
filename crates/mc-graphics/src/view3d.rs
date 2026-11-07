//! Turning a 3D picture without evaluating anything again: the fast path a front end uses to rotate and zoom a
//! `graphics3d` or `plot3d` value interactively.
//!
//! When a 3D graphic is shown as a picture (`plot-N.png`), its scene is written next to it as `plot-N.scene3d`
//! ([`Scene3d::sidecar_of`]): a first line `mathcas-scene3d 1` (the format and its version), then the printed
//! `graphics3d` term (UTF-8). A front end loads it once with [`Scene3d::load`] (or [`Scene3d::cached`]), which
//! reads and prepares the scene (clipping, scaling, face and vertex normals), and then draws it from any
//! [`View3d`] with [`Scene3d::render`]: a projection, the lighting, the depth sort and the painting, into a page of
//! the picture's own size, so the frame stays where it is while the view turns.
//!
//! ```text
//! let scene = Scene3d::load(&Scene3d::sidecar_of(png))?;         // once per picture
//! let mut view = scene.view();                                    // what the PNG shows
//! view = orbit_drag(&view, dx, dy, width, height);                // on each mouse move (pixels)
//! view.zoom = zoom_wheel(view.zoom, notches);                     // on the wheel
//! let (w, h, rgba) = scene.render(&view, 1.5);                    // straight RGBA, the top row first
//! ```
//!
//! Everything here works without a term store or an evaluator once loaded, and is `Send + Sync`: render on a
//! worker thread and drop stale frames.
use crate::three::{orbit, Prepared, P3};
use crate::{page, raster, Failure};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub use crate::three::View3d;

/// The file extension of the scene sidecar written next to a 3D picture.
pub const SIDECAR: &str = "scene3d";

/// The first line of a scene sidecar: the format's name and version.
pub const HEADER: &str = "mathcas-scene3d 1";

/// The text of a scene sidecar for the printed `graphics3d` term `term`.
pub fn sidecar_text(term: &str) -> String {
    format!("{HEADER}\n{term}")
}

/// A 3D graphic prepared for drawing from any view point, with the page size of its picture.
#[derive(Clone, Debug)]
pub struct Scene3d {
    prepared: Prepared,
    page: (f64, f64),
}

impl Scene3d {
    /// The scene sidecar of the picture at `png` (`plot-N.png` -> `plot-N.scene3d`).
    pub fn sidecar_of(png: &Path) -> PathBuf {
        png.with_extension(SIDECAR)
    }

    /// Whether the picture at `png` can be turned: its scene sidecar exists.
    pub fn exists_for(png: &Path) -> bool {
        Scene3d::sidecar_of(png).is_file()
    }

    /// Prepare the `graphics3d` term `t`.
    pub fn from_term<S: mc_core::Store + ?Sized>(h: &S, t: mc_core::T<'_>, stop: &dyn Fn() -> bool) -> Result<Scene3d, Failure> {
        let prepared = Prepared::read(h, t, stop)?;
        let g = prepared.graphic(&prepared.default_view(), None, stop)?;
        let p = page::plan(&g, stop)?;
        Ok(Scene3d { prepared, page: (p.width as f64, p.height as f64) })
    }

    /// Prepare the scene in the text of a sidecar ([`HEADER`] line, then the printed `graphics3d` term; a text
    /// without the header line is read as the term alone).
    pub fn from_text(text: &str, stop: &dyn Fn() -> bool) -> Result<Scene3d, Failure> {
        let body = match text.split_once('\n') {
            Some((first, rest)) if first.trim_end().starts_with("mathcas-scene3d") => {
                if first.trim_end() != HEADER {
                    return Err(Failure::new("not_graphics", format!("unknown scene format {}", first.trim_end())));
                }
                rest
            }
            _ => text,
        };
        let h = mc_core::Heap::new(mc_core::Config::default());
        match mc_syntax::read(&h, body) {
            Ok(v) if v.len() == 1 => Scene3d::from_term(&h, v[0], stop),
            _ => Err(Failure::new("not_graphics", "the scene file does not hold one graphics3d term")),
        }
    }

    /// Load and prepare the scene sidecar at `path` (see [`Scene3d::sidecar_of`]).
    pub fn load(path: &Path) -> std::io::Result<Scene3d> {
        let text = std::fs::read_to_string(path)?;
        Scene3d::from_text(&text, &|| false).map_err(|f| std::io::Error::new(std::io::ErrorKind::InvalidData, f.to_string()))
    }

    /// [`Scene3d::load`] through a small cache of recently loaded scenes, keyed by the sidecar's path (plot
    /// files are never rewritten: a new plot gets a new name).
    pub fn cached(path: &Path) -> std::io::Result<Arc<Scene3d>> {
        static CACHE: Mutex<Vec<(PathBuf, Arc<Scene3d>)>> = Mutex::new(Vec::new());
        const ENTRIES: usize = 8;
        if let Ok(c) = CACHE.lock() {
            if let Some((_, s)) = c.iter().find(|(p, _)| p == path) {
                return Ok(s.clone());
            }
        }
        let s = Arc::new(Scene3d::load(path)?);
        if let Ok(mut c) = CACHE.lock() {
            c.retain(|(p, _)| p != path);
            c.push((path.to_path_buf(), s.clone()));
            if c.len() > ENTRIES {
                c.remove(0);
            }
        }
        Ok(s)
    }

    /// The view the graphic's own options give: what the picture file shows (zoom 1).
    pub fn view(&self) -> View3d {
        self.prepared.default_view()
    }

    /// The page size in points (the picture's pixels at scale 1); every render has this size times its scale.
    pub fn page_size(&self) -> (f64, f64) {
        self.page
    }

    /// The number of primitives in the scene.
    pub fn len(&self) -> usize {
        self.prepared.len()
    }

    /// Whether the scene has no primitives.
    pub fn is_empty(&self) -> bool {
        self.prepared.is_empty()
    }

    /// The two-dimensional graphic seen from `view`, on the picture's page.
    pub fn graphic(&self, view: &View3d, stop: &dyn Fn() -> bool) -> Result<crate::model::Graphic, Failure> {
        self.prepared.graphic(view, Some(self.page), stop)
    }

    /// Draw the scene from `view` at `scale` pixels per point: width, height and straight (not premultiplied)
    /// RGBA bytes, the top row first. A view that cannot be drawn (a zero view point, a zero zoom) gives the page
    /// blank; the scale is kept within 0.05 .. 16. See [`Scene3d::try_render`] for the failures.
    pub fn render(&self, view: &View3d, scale: f32) -> (u32, u32, Vec<u8>) {
        let scale = if scale.is_finite() { scale.clamp(0.05, 16.0) } else { 1.0 };
        match self.try_render(view, scale, &|| false) {
            Ok(r) => r,
            Err(_) => {
                let (w, h) = ((self.page.0 * scale as f64).ceil().max(1.0) as u32, (self.page.1 * scale as f64).ceil().max(1.0) as u32);
                (w, h, vec![255; (w as usize) * (h as usize) * 4])
            }
        }
    }

    /// Whether the scene is large enough that turning it should draw drafts while the mouse moves
    /// ([`Scene3d::render_draft`]).
    pub fn is_large(&self) -> bool {
        self.len() > DRAFT_PRIMITIVES
    }

    /// A cheap render for dragging: a large scene ([`Scene3d::is_large`]) drawn at a lower resolution (a quarter
    /// of the pixels or fewer, by its size), a small one as [`Scene3d::render`]. The image is smaller than the full
    /// render; front ends stretch it over the picture and ask for the full render when the drag ends.
    pub fn render_draft(&self, view: &View3d, scale: f32) -> (u32, u32, Vec<u8>) {
        if !self.is_large() {
            return self.render(view, scale);
        }
        let k = ((DRAFT_PRIMITIVES as f64 / self.len() as f64).sqrt() as f32).clamp(0.25, 0.5);
        self.render(view, scale * k)
    }

    /// [`Scene3d::render`] with its failures (`argument` for a bad view or scale, `too_large`, `interrupted`
    /// when `stop` says so).
    pub fn try_render(&self, view: &View3d, scale: f32, stop: &dyn Fn() -> bool) -> Result<(u32, u32, Vec<u8>), Failure> {
        Ok(self.raster(view, scale, stop)?.rgba())
    }

    /// Draw the scene from `view` at `scale` pixels per point as a PNG file's bytes.
    pub fn render_png(&self, view: &View3d, scale: f32, stop: &dyn Fn() -> bool) -> Result<Vec<u8>, Failure> {
        self.raster(view, scale, stop)?.png().ok_or_else(|| Failure::new("io", "PNG encoding failed"))
    }

    fn raster(&self, view: &View3d, scale: f32, stop: &dyn Fn() -> bool) -> Result<raster::Raster, Failure> {
        if !(scale > 0.0 && scale <= 16.0) {
            return Err(Failure::new("argument", format!("scale {scale}")));
        }
        let g = self.graphic(view, stop)?;
        let p = page::plan(&g, stop)?;
        let mut r = raster::Raster::new(p.width, p.height, scale)
            .ok_or_else(|| Failure::new("too_large", format!("{} x {} points at scale {scale}", p.width, p.height)))?;
        page::paint(&g, &p, &mut r, stop)?;
        Ok(r)
    }
}

/// Scenes with more primitives than this are drawn as drafts while they are turned.
pub const DRAFT_PRIMITIVES: usize = 6000;

/// The view after dragging the mouse by `(dx, dy)` pixels over a picture `width` x `height` pixels: a drag across
/// the whole width turns the scene half a turn about the vertical (right: it turns right), a drag over the whole
/// height tilts it half a turn (down: the eye rises, the top comes into view). Zoom and vertical stay.
pub fn orbit_drag(view: &View3d, dx: f64, dy: f64, width: f64, height: f64) -> View3d {
    let pi = std::f64::consts::PI;
    let turn = if width > 0.0 && dx.is_finite() { pi * dx / width } else { 0.0 };
    let tilt = if height > 0.0 && dy.is_finite() { pi * dy / height } else { 0.0 };
    View3d { view_point: orbit(view.view_point, view.view_vertical, turn, tilt), ..*view }
}

/// The zoom after `notches` of the mouse wheel (positive: closer), kept within 0.25 .. 8.
pub fn zoom_wheel(zoom: f64, notches: f64) -> f64 {
    if !(zoom.is_finite() && zoom > 0.0 && notches.is_finite()) {
        return 1.0;
    }
    (zoom * 1.1f64.powf(notches)).clamp(0.25, 8.0)
}

/// A point in space (re-exported for front ends).
pub type Point3 = P3;
