//! Pictures in Output cells (design 6): PNG files and SVG drawings, decoded on a worker thread and kept as GPU
//! textures keyed by path, modification time and, for SVG, the raster width in device pixels and the theme. A
//! texture is uploaded once, when its decode finishes, never per frame; at most [`BUDGET`] bytes of pixels are
//! kept, the least recently drawn going first; no texture side exceeds [`MAX_SIDE`] (larger pictures are scaled
//! down while decoding).
//!
//! A `.svgz` file is an SVG drawing compressed with gzip (the documentation's plots): it is decompressed while
//! parsing and drawn as an `.svg` is.
//!
//! Vector display (round nb-svg): a plot's PNG (`plot-N.png`) is drawn from its SVG sibling, and an SVG is
//! rasterised at exactly the device pixels it covers (picture size x magnification x display scale), so it is
//! sharp at every scale; its chrome takes the window's theme through a style sheet ([`mc_graphics::theme`]). The
//! worker keeps the last few parsed drawings, so a zoom step or a theme switch renders again without parsing, and
//! skips requests that a newer one for the same drawing overtook. While a new raster is made the newest older one
//! shows, stretched.
use eframe::egui::{self, ColorImage, TextureHandle, TextureId, TextureOptions};
use mc_graphics::theme::Theme;
use mc_nbfile::images::ImageFormat;
use resvg::{tiny_skia, usvg};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::time::SystemTime;

/// The pixel memory the cache keeps (design 6: 256 MB).
pub const BUDGET: usize = 256 << 20;
/// The largest texture side: pictures beyond it are scaled down (the GPU's limit can be as low as this).
pub const MAX_SIDE: u32 = 4096;
/// Pictures with more pixels than this are not decoded (the decode alone would need 400 MB).
const MAX_PIXELS: u64 = 100_000_000;
/// Seconds between checks of a file's modification time.
const RECHECK: f64 = 0.5;
/// Parsed drawings the worker keeps.
const TREES: usize = 6;

/// What a cell shows for a picture this frame.
#[derive(Clone, Debug, PartialEq)]
pub enum Picture {
    /// the texture and its size in pixels
    Ready(TextureId, [usize; 2]),
    /// being decoded (drawn as an empty frame)
    Loading,
    /// it cannot be shown: why
    Missing(String),
}

/// What a texture shows: a file, its version, and for an SVG drawing the raster width (device pixels) and the
/// theme ([`Theme::key`]); both 0 for PNG.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Key {
    path: PathBuf,
    modified: Option<SystemTime>,
    width: u32,
    theme: u64,
}

impl Key {
    /// The key of a picture of `format` drawn `width` device pixels wide in `theme`.
    pub fn of(path: &Path, modified: Option<SystemTime>, format: ImageFormat, width: u32, theme: Option<&Theme>) -> Key {
        match format {
            ImageFormat::Png => Key { path: path.to_path_buf(), modified, width: 0, theme: 0 },
            ImageFormat::Svg => Key {
                path: path.to_path_buf(),
                modified,
                width: width.clamp(1, MAX_SIDE),
                theme: theme.map_or(0, Theme::key),
            },
        }
    }
}

enum State {
    Loading,
    Ready(TextureHandle),
    Failed(String),
}

struct Entry {
    state: State,
    bytes: usize,
    /// the frame it was last drawn in
    drawn: u64,
}

/// What is known about a file without decoding it.
#[derive(Clone)]
struct FileInfo {
    checked: f64,
    modified: Option<SystemTime>,
    format: Option<ImageFormat>,
    natural: Option<(u32, u32)>,
    exists: bool,
}

struct Job {
    key: Key,
    format: ImageFormat,
    /// the theme's style sheet (SVG)
    css: Option<String>,
}

/// What the worker gives back for a job.
enum Done {
    Image(ColorImage),
    Failed(String),
    /// a newer request for the same drawing overtook this one
    Skipped,
}

/// The picture cache of a window.
pub struct Images {
    ctx: egui::Context,
    entries: HashMap<Key, Entry>,
    files: HashMap<PathBuf, FileInfo>,
    jobs: Sender<Job>,
    done: Receiver<(Key, Done)>,
    bytes: usize,
    frame: u64,
    uploads: u64,
    theme: Option<Theme>,
    times: Receiver<(f64, u32)>,
    /// the last SVG rasterisation times (milliseconds, parsing included when it was needed), the newest last
    pub svg_times: Vec<(f64, u32)>,
}

impl Images {
    /// An empty cache with its decoding thread.
    pub fn new(ctx: &egui::Context) -> Images {
        let (jobs, rx) = mpsc::channel::<Job>();
        let (tx, done) = mpsc::channel();
        let (times_tx, times) = mpsc::channel();
        let repaint = ctx.clone();
        let spawned = std::thread::Builder::new().name("mc-notebook-images".into()).spawn(move || {
            let mut fonts = None;
            let mut trees = Trees::default();
            while let Ok(first) = rx.recv() {
                // the requests waiting now; of several for one drawing (zoom steps, resizes) only the newest is done
                let mut batch = vec![first];
                while let Ok(j) = rx.try_recv() {
                    batch.push(j);
                }
                for (i, job) in batch.iter().enumerate() {
                    let overtaken = job.format == ImageFormat::Svg
                        && batch[i + 1..].iter().any(|o| o.key.path == job.key.path && o.key.theme == job.key.theme);
                    let r = if overtaken {
                        Done::Skipped
                    } else {
                        let start = std::time::Instant::now();
                        let r = match job.format {
                            ImageFormat::Png => decode_png(&job.key.path),
                            ImageFormat::Svg => trees.raster(&job.key, job.css.as_deref(), &mut fonts),
                        };
                        if job.format == ImageFormat::Svg {
                            let _ = times_tx.send((start.elapsed().as_secs_f64() * 1000.0, job.key.width));
                        }
                        match r {
                            Ok((rgba, w, h, premultiplied)) => {
                                let size = [w as usize, h as usize];
                                Done::Image(if premultiplied {
                                    ColorImage::from_rgba_premultiplied(size, &rgba)
                                } else {
                                    ColorImage::from_rgba_unmultiplied(size, &rgba)
                                })
                            }
                            Err(why) => Done::Failed(why),
                        }
                    };
                    if tx.send((job.key.clone(), r)).is_err() {
                        return;
                    }
                    repaint.request_repaint();
                }
            }
        });
        if let Err(e) = spawned {
            eprintln!("mc-notebook: no picture thread: {e}");
        }
        Images {
            ctx: ctx.clone(),
            entries: HashMap::new(),
            files: HashMap::new(),
            jobs,
            done,
            bytes: 0,
            frame: 0,
            uploads: 0,
            theme: None,
            times,
            svg_times: vec![],
        }
    }

    /// The colours SVG drawings' chrome is drawn in from now on (`None`: as the files have them). Pictures already
    /// shown keep their raster until the new one is there.
    pub fn set_theme(&mut self, theme: Option<Theme>) {
        self.theme = theme;
    }

    /// At the start of a frame: upload the pictures decoded since the last one, then keep within the budget.
    pub fn begin_frame(&mut self) {
        self.frame += 1;
        while let Ok(t) = self.times.try_recv() {
            self.svg_times.push(t);
            if self.svg_times.len() > 200 {
                self.svg_times.remove(0);
            }
        }
        while let Ok((key, result)) = self.done.try_recv() {
            let Some(e) = self.entries.get_mut(&key) else { continue };
            match result {
                Done::Image(img) => {
                    let bytes = img.width() * img.height() * 4;
                    let name = format!("picture:{}", key.path.display());
                    e.state = State::Ready(self.ctx.load_texture(name, img, TextureOptions::LINEAR));
                    e.bytes = bytes;
                    self.bytes += bytes;
                    self.uploads += 1;
                }
                Done::Failed(why) => e.state = State::Failed(why),
                // asked for again if it is still wanted
                Done::Skipped => {
                    self.entries.remove(&key);
                }
            }
        }
        self.prune();
        self.evict();
    }

    /// Drop the textures a newer one of the same file replaced: those last drawn more than two frames before the
    /// newest drawn of their file (a picture shown twice at two sizes keeps both; one scrolled away keeps its last).
    fn prune(&mut self) {
        let mut newest: HashMap<PathBuf, u64> = HashMap::new();
        for (k, e) in &self.entries {
            if matches!(e.state, State::Ready(_)) {
                let n = newest.entry(k.path.clone()).or_insert(0);
                *n = (*n).max(e.drawn);
            }
        }
        let stale: Vec<Key> = self
            .entries
            .iter()
            .filter(|(k, e)| matches!(e.state, State::Ready(_)) && e.drawn + 2 < newest.get(&k.path).copied().unwrap_or(0))
            .map(|(k, _)| k.clone())
            .collect();
        for k in stale {
            self.drop_entry(&k);
        }
    }

    fn drop_entry(&mut self, k: &Key) {
        if let Some(e) = self.entries.remove(k) {
            self.bytes -= e.bytes;
        }
    }

    /// Drop the least recently drawn textures (not drawn this frame) until the cache is within its budget.
    fn evict(&mut self) {
        if self.bytes <= BUDGET {
            return;
        }
        let mut old: Vec<(u64, Key)> = self
            .entries
            .iter()
            .filter(|(_, e)| e.drawn < self.frame && e.bytes > 0)
            .map(|(k, e)| (e.drawn, k.clone()))
            .collect();
        old.sort_by_key(|(d, _)| *d);
        for (_, k) in old {
            if self.bytes <= BUDGET {
                break;
            }
            self.drop_entry(&k);
        }
    }

    fn info(&mut self, path: &Path, time: f64) -> FileInfo {
        if let Some(i) = self.files.get(path) {
            if time - i.checked < RECHECK {
                return i.clone();
            }
        }
        let meta = std::fs::metadata(path).ok();
        let modified = meta.as_ref().and_then(|m| m.modified().ok());
        let known = self.files.get(path).filter(|i| i.modified == modified && i.exists == meta.is_some());
        let info = match known {
            Some(i) => FileInfo { checked: time, ..i.clone() },
            None => FileInfo {
                checked: time,
                modified,
                format: ImageFormat::of(path),
                natural: natural_size(path),
                exists: meta.is_some(),
            },
        };
        self.files.insert(path.to_path_buf(), info.clone());
        info
    }

    /// The picture's own size (points at 100 %; `None`: it cannot be read).
    pub fn natural(&mut self, path: &Path, time: f64) -> Option<(u32, u32)> {
        self.info(path, time).natural
    }

    /// The file a picture at `path` is drawn from: a plot's PNG from its SVG sibling (sharp at any scale, in the
    /// theme), anything else from itself.
    pub fn source(&mut self, path: &Path, time: f64) -> PathBuf {
        if self.info(path, time).format == Some(ImageFormat::Png) {
            let svg = path.with_extension("svg");
            if self.info(&svg, time).exists {
                return svg;
            }
        }
        path.to_path_buf()
    }

    /// Whether the picture at `path` is drawn from a drawing (rasterised at the pixels it covers).
    pub fn is_vector(&mut self, path: &Path, time: f64) -> bool {
        let s = self.source(path, time);
        self.info(&s, time).format == Some(ImageFormat::Svg)
    }

    /// The picture at `path` for this frame; a drawing is rasterised `width` device pixels wide (exactly).
    pub fn picture(&mut self, path: &Path, width: u32, time: f64) -> Picture {
        let path = &self.source(path, time);
        let info = self.info(path, time);
        if !info.exists {
            return Picture::Missing("missing".into());
        }
        let Some(format) = info.format else { return Picture::Missing("not a PNG or SVG picture".into()) };
        let key = Key::of(path, info.modified, format, width, self.theme.as_ref());
        let frame = self.frame;
        let theme = self.theme;
        let e = self.entries.entry(key.clone()).or_insert_with(|| {
            let css = if format == ImageFormat::Svg { theme.map(|t| t.css()) } else { None };
            let _ = self.jobs.send(Job { key: key.clone(), format, css });
            Entry { state: State::Loading, bytes: 0, drawn: frame }
        });
        e.drawn = frame;
        match &e.state {
            State::Ready(t) => return Picture::Ready(t.id(), t.size()),
            State::Failed(why) => return Picture::Missing(why.clone()),
            State::Loading => {}
        }
        // an older raster of the same file shows (stretched) until the new one is ready: the one drawn last, of
        // the same theme if there is one
        let older = self
            .entries
            .iter_mut()
            .filter(|(k, e)| k.path == key.path && matches!(e.state, State::Ready(_)))
            .max_by_key(|(k, e)| (k.theme == key.theme, e.drawn));
        if let Some((_, e)) = older {
            e.drawn = frame;
            if let State::Ready(t) = &e.state {
                return Picture::Ready(t.id(), t.size());
            }
        }
        Picture::Loading
    }

    /// The bytes of pixels held, the textures kept and the textures uploaded so far (for the self-test's log).
    pub fn stats(&self) -> (usize, usize, u64) {
        (self.bytes, self.entries.len(), self.uploads)
    }
}

/// The text of the SVG drawing at `path`; a gzip-compressed one (`.svgz`) is decompressed.
fn read_drawing(path: &Path) -> Result<Vec<u8>, String> {
    let data = std::fs::read(path).map_err(|e| e.to_string())?;
    if !data.starts_with(&[0x1f, 0x8b]) {
        return Ok(data);
    }
    let mut svg = Vec::new();
    std::io::Read::read_to_end(&mut flate2::read::GzDecoder::new(data.as_slice()), &mut svg)
        .map_err(|e| format!("not a readable SVG drawing: {e}"))?;
    Ok(svg)
}

/// The size in points of the picture at `path` ([`mc_nbfile::images::picture_size`]); of a `.svgz` drawing (gzip-compressed
/// SVG, as the documentation stores its plots) from the root element of the decompressed text.
fn natural_size(path: &Path) -> Option<(u32, u32)> {
    if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("svgz")) {
        let svg = read_drawing(path).ok()?;
        return mc_nbfile::images::svg_size(&String::from_utf8_lossy(&svg));
    }
    mc_nbfile::images::picture_size(path)
}

/// The picture at `path` as RGBA pixels (not premultiplied), decoded now, for the clipboard: PNG files at their
/// size, SVG drawings at `svg_width` pixels wide, in their own (light) colours.
pub fn pixels(path: &Path, svg_width: u32) -> Result<(Vec<u8>, u32, u32), String> {
    match ImageFormat::of(path).ok_or("not a PNG or SVG picture")? {
        ImageFormat::Png => decode_png(path).map(|(p, w, h, _)| (p, w, h)),
        ImageFormat::Svg => {
            let tree = parse_svg(path, None, &mut None)?;
            let (mut p, w, h, _) = rasterise(&tree, svg_width.clamp(1, MAX_SIDE))?;
            for px in p.chunks_mut(4) {
                let a = px[3] as u32;
                if a > 0 && a < 255 {
                    for c in &mut px[..3] {
                        *c = ((*c as u32 * 255 + a / 2) / a).min(255) as u8;
                    }
                }
            }
            Ok((p, w, h))
        }
    }
}

/// Rasterises drawings given as bytes (the documentation window's pictures are not files): it keeps the font
/// database between calls.
#[derive(Default)]
pub struct SvgRaster {
    fonts: Option<Arc<usvg::fontdb::Database>>,
}

impl SvgRaster {
    /// The SVG drawing `data` (plain text, not compressed) `width` pixels wide, its chrome in `theme` (`None`: its
    /// own light colours): premultiplied RGBA pixels, width and height.
    pub fn pixels(&mut self, data: &[u8], width: u32, theme: Option<&Theme>) -> Result<(Vec<u8>, u32, u32), String> {
        let fontdb = self
            .fonts
            .get_or_insert_with(|| {
                let mut db = usvg::fontdb::Database::new();
                db.load_system_fonts();
                Arc::new(db)
            })
            .clone();
        let opt = usvg::Options { fontdb, style_sheet: theme.map(Theme::css), ..usvg::Options::default() };
        let tree = usvg::Tree::from_data(data, &opt).map_err(|e| format!("not a readable SVG drawing: {e}"))?;
        let (p, w, h, _) = rasterise(&tree, width.clamp(1, MAX_SIDE))?;
        Ok((p, w, h))
    }
}

/// A PNG file as RGBA pixels (not premultiplied): any colour type and depth, scaled down beyond [`MAX_SIDE`].
fn decode_png(path: &Path) -> Result<(Vec<u8>, u32, u32, bool), String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut dec = png::Decoder::new(std::io::BufReader::new(file));
    dec.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = dec.read_info().map_err(|e| format!("not a readable PNG file: {e}"))?;
    let (w, h) = (reader.info().width, reader.info().height);
    if u64::from(w) * u64::from(h) > MAX_PIXELS {
        return Err(format!("too large to show ({w} x {h} pixels)"));
    }
    let mut buf = vec![0; reader.output_buffer_size().ok_or("too large to show")?];
    let info = reader.next_frame(&mut buf).map_err(|e| format!("not a readable PNG file: {e}"))?;
    let px = &buf[..info.buffer_size()];
    let rgba: Vec<u8> = match info.color_type {
        png::ColorType::Rgba => px.to_vec(),
        png::ColorType::Rgb => px.chunks(3).flat_map(|c| [c[0], c[1], c[2], 255]).collect(),
        png::ColorType::GrayscaleAlpha => px.chunks(2).flat_map(|c| [c[0], c[0], c[0], c[1]]).collect(),
        png::ColorType::Grayscale => px.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::Indexed => return Err("an indexed PNG the decoder did not expand".into()),
    };
    let (rgba, w, h) = shrink(rgba, info.width, info.height);
    Ok((rgba, w, h, false))
}

/// Scale RGBA pixels down by a whole factor (averaging blocks) until both sides are at most [`MAX_SIDE`].
fn shrink(rgba: Vec<u8>, w: u32, h: u32) -> (Vec<u8>, u32, u32) {
    let f = w.max(h).div_ceil(MAX_SIDE);
    if f <= 1 {
        return (rgba, w, h);
    }
    let (nw, nh) = (w.div_ceil(f), h.div_ceil(f));
    let mut out = vec![0u8; (nw * nh * 4) as usize];
    for y in 0..nh {
        for x in 0..nw {
            let mut sum = [0u32; 4];
            let mut n = 0;
            for sy in y * f..((y + 1) * f).min(h) {
                for sx in x * f..((x + 1) * f).min(w) {
                    let i = ((sy * w + sx) * 4) as usize;
                    for c in 0..4 {
                        sum[c] += u32::from(rgba[i + c]);
                    }
                    n += 1;
                }
            }
            let o = ((y * nw + x) * 4) as usize;
            for c in 0..4 {
                out[o + c] = (sum[c] / n.max(1)) as u8;
            }
        }
    }
    (out, nw, nh)
}

/// A parsed drawing's identity: file, version and theme (the style sheet is applied while parsing).
type TreeId = (PathBuf, Option<SystemTime>, u64);

/// The worker's last parsed drawings.
#[derive(Default)]
struct Trees {
    trees: Vec<(TreeId, Arc<usvg::Tree>)>,
}

impl Trees {
    /// The drawing `key` names rasterised at its width, in the style sheet `css`.
    fn raster(
        &mut self,
        key: &Key,
        css: Option<&str>,
        fonts: &mut Option<Arc<usvg::fontdb::Database>>,
    ) -> Result<(Vec<u8>, u32, u32, bool), String> {
        let id = (key.path.clone(), key.modified, key.theme);
        let tree = match self.trees.iter().position(|(k, _)| *k == id) {
            Some(i) => {
                let t = self.trees.remove(i);
                let tree = t.1.clone();
                self.trees.push(t);
                tree
            }
            None => {
                let tree = Arc::new(parse_svg(&key.path, css, fonts)?);
                self.trees.push((id, tree.clone()));
                if self.trees.len() > TREES {
                    self.trees.remove(0);
                }
                tree
            }
        };
        rasterise(&tree, key.width)
    }
}

/// An SVG file parsed with `css` injected (it overrides the drawing's presentation attributes); text in the
/// system's fonts, loaded once, on the first drawing, into `fonts`.
fn parse_svg(path: &Path, css: Option<&str>, fonts: &mut Option<Arc<usvg::fontdb::Database>>) -> Result<usvg::Tree, String> {
    let data = read_drawing(path)?;
    let fontdb = fonts
        .get_or_insert_with(|| {
            let mut db = usvg::fontdb::Database::new();
            db.load_system_fonts();
            Arc::new(db)
        })
        .clone();
    let opt = usvg::Options {
        resources_dir: path.parent().map(Path::to_path_buf),
        fontdb,
        style_sheet: css.map(str::to_string),
        ..usvg::Options::default()
    };
    usvg::Tree::from_data(&data, &opt).map_err(|e| format!("not a readable SVG drawing: {e}"))
}

/// A parsed drawing rasterised `width` pixels wide (premultiplied RGBA; the height rounded to whole pixels).
fn rasterise(tree: &usvg::Tree, width: u32) -> Result<(Vec<u8>, u32, u32, bool), String> {
    let size = tree.size();
    let scale = width as f32 / size.width();
    let w = width.min(MAX_SIDE);
    let h = ((size.height() * scale).round() as u32).clamp(1, MAX_SIDE);
    let mut pixmap = tiny_skia::Pixmap::new(w, h).ok_or("an empty drawing")?;
    resvg::render(tree, tiny_skia::Transform::from_scale(scale, scale), &mut pixmap.as_mut());
    Ok((pixmap.take(), w, h, true))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn large_pictures_shrink_by_whole_factors() {
        let (p, w, h) = shrink(vec![255; (5000 * 3 * 4) as usize], 5000, 3);
        assert_eq!((w, h), (2500, 2));
        assert_eq!(p.len(), (2500 * 2 * 4) as usize);
        assert!(p.iter().all(|&c| c == 255));
        let (p, w, h) = shrink(vec![7; 16], 2, 2);
        assert_eq!((p.len(), w, h), (16, 2, 2));
    }

    fn no_fonts() -> Option<Arc<usvg::fontdb::Database>> {
        Some(Arc::new(usvg::fontdb::Database::new()))
    }

    #[test]
    fn svg_drawings_rasterise_at_the_asked_width() {
        let dir = std::env::temp_dir().join(format!("mc-notebook-svg-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let svg = dir.join("d.svg");
        std::fs::write(
            &svg,
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"100\" height=\"50\"><rect width=\"100\" height=\"50\" fill=\"#ff0000\"/></svg>",
        )
        .unwrap();
        let (p, w, h, pre) = rasterise(&parse_svg(&svg, None, &mut no_fonts()).unwrap(), 300).unwrap();
        assert_eq!((w, h, pre), (300, 150, true));
        assert_eq!(&p[..4], &[255, 0, 0, 255]);
        // any width, not steps: 1.5 x 0.9 x 100 points
        let (_, w, h, _) = rasterise(&parse_svg(&svg, None, &mut no_fonts()).unwrap(), 135).unwrap();
        assert_eq!((w, h), (135, 68));
        let (_, w, h) = pixels(&svg, 64).unwrap();
        assert_eq!((w, h), (64, 32));
        assert!(decode_png(&svg).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_theme_recolours_the_chrome_of_our_drawings() {
        let dir = std::env::temp_dir().join(format!("mc-notebook-theme-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let svg = dir.join("plot.svg");
        // a page as mc-graphics writes it: white background (chrome), a grey frame (chrome), a red curve (data)
        std::fs::write(
            &svg,
            "<svg xmlns=\"http://www.w3.org/2000/svg\" class=\"mc-graphic\" width=\"40\" height=\"20\" viewBox=\"0 0 40 20\">\
             <path class=\"mcf-bg\" d=\"M0 0L40 0L40 20L0 20Z\" fill=\"#ffffff\"/>\
             <path class=\"mcs-frame\" d=\"M2 2L38 2\" fill=\"none\" stroke=\"#3c3c3c\" stroke-width=\"2\"/>\
             <path d=\"M2 12L38 12\" fill=\"none\" stroke=\"#ff0000\" stroke-width=\"4\"/></svg>",
        )
        .unwrap();
        let dark = Theme {
            background: None,
            text: mc_graphics::color::Rgba::rgb(0.8, 0.8, 0.8),
            frame: mc_graphics::color::Rgba::rgb(0.6, 0.6, 0.6),
            grid: mc_graphics::color::Rgba::rgb(0.2, 0.2, 0.2),
            panel: mc_graphics::color::Rgba::rgb(0.1, 0.1, 0.1),
        };
        let mut trees = Trees::default();
        let modified = std::fs::metadata(&svg).and_then(|m| m.modified()).ok();
        let key = Key::of(&svg, modified, ImageFormat::Svg, 80, Some(&dark));
        let (p, w, _, _) = trees.raster(&key, Some(&dark.css()), &mut no_fonts()).unwrap();
        let at = |x: u32, y: u32| &p[((y * w + x) * 4) as usize..((y * w + x) * 4 + 4) as usize];
        assert_eq!(at(1, 38)[3], 0, "the background is transparent");
        assert_eq!(at(40, 4), &[153, 153, 153, 255], "the frame in the theme's colour");
        assert_eq!(at(40, 24), &[255, 0, 0, 255], "the curve as drawn");
        // without a theme: the file's own colours
        let plain = Key::of(&svg, modified, ImageFormat::Svg, 80, None);
        let (p, w, _, _) = trees.raster(&plain, None, &mut no_fonts()).unwrap();
        assert_eq!(&p[((38 * w + 1) * 4) as usize..((38 * w + 1) * 4 + 4) as usize], &[255, 255, 255, 255]);
        // parsed once per theme: a new width reuses the tree
        assert_eq!(trees.trees.len(), 2);
        let wider = Key::of(&svg, modified, ImageFormat::Svg, 120, Some(&dark));
        let (_, w, h, _) = trees.raster(&wider, Some(&dark.css()), &mut no_fonts()).unwrap();
        assert_eq!((w, h, trees.trees.len()), (120, 60, 2));
        let _ = std::fs::remove_dir_all(&dir);
    }

    const PLOT: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" class=\"mc-graphic\" width=\"40\" height=\"20\" viewBox=\"0 0 40 20\">         <path class=\"mcf-bg\" d=\"M0 0L40 0L40 20L0 20Z\" fill=\"#ffffff\"/>         <path class=\"mcs-frame\" d=\"M2 2L38 2\" fill=\"none\" stroke=\"#3c3c3c\" stroke-width=\"2\"/>         <path d=\"M2 12L38 12\" fill=\"none\" stroke=\"#ff0000\" stroke-width=\"4\"/></svg>";

    fn gzip(text: &str) -> Vec<u8> {
        use std::io::Write;
        let mut enc = flate2::GzBuilder::new().mtime(0).write(Vec::new(), flate2::Compression::best());
        enc.write_all(text.as_bytes()).unwrap();
        enc.finish().unwrap()
    }

    #[test]
    fn svgz_drawings_are_svg_with_a_natural_size_and_a_theme() {
        let dir = std::env::temp_dir().join(format!("mc-notebook-svgz-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let z = dir.join("plot-1.svgz");
        std::fs::write(&z, gzip(PLOT)).unwrap();
        assert_eq!(ImageFormat::of(&z), Some(ImageFormat::Svg));
        // the natural size comes from the decompressed drawing
        assert_eq!(natural_size(&z), Some((40, 20)));
        let mut images = Images::new(&egui::Context::default());
        assert_eq!(images.natural(&z, 0.0), Some((40, 20)));
        assert!(images.is_vector(&z, 0.0));
        // parsed and rasterised as an .svg is, in the theme
        let dark = Theme {
            background: None,
            text: mc_graphics::color::Rgba::rgb(0.8, 0.8, 0.8),
            frame: mc_graphics::color::Rgba::rgb(0.6, 0.6, 0.6),
            grid: mc_graphics::color::Rgba::rgb(0.2, 0.2, 0.2),
            panel: mc_graphics::color::Rgba::rgb(0.1, 0.1, 0.1),
        };
        let mut trees = Trees::default();
        let modified = std::fs::metadata(&z).and_then(|m| m.modified()).ok();
        let key = Key::of(&z, modified, ImageFormat::Svg, 80, Some(&dark));
        let (p, w, h, _) = trees.raster(&key, Some(&dark.css()), &mut no_fonts()).unwrap();
        assert_eq!((w, h), (80, 40));
        let at = |x: u32, y: u32| &p[((y * w + x) * 4) as usize..((y * w + x) * 4 + 4) as usize];
        assert_eq!(at(1, 38)[3], 0, "the background is transparent");
        assert_eq!(at(40, 4), &[153, 153, 153, 255], "the frame in the theme's colour");
        assert_eq!(at(40, 24), &[255, 0, 0, 255], "the data colour as drawn");
        // by theme and width: its own raster each
        assert_ne!(key, Key::of(&z, modified, ImageFormat::Svg, 80, None));
        assert_ne!(key, Key::of(&z, modified, ImageFormat::Svg, 81, Some(&dark)));
        // an unreadable one has no size
        std::fs::write(dir.join("bad.svgz"), b"not gzip").unwrap();
        assert_eq!(natural_size(&dir.join("bad.svgz")), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cache_keys_tell_sizes_and_themes_apart() {
        let p = Path::new("plot-1.svg");
        let dark = Theme { background: None, ..Theme::LIGHT };
        let a = Key::of(p, None, ImageFormat::Svg, 960, Some(&Theme::LIGHT));
        assert_eq!(a, Key::of(p, None, ImageFormat::Svg, 960, Some(&Theme::LIGHT)));
        assert_ne!(a, Key::of(p, None, ImageFormat::Svg, 961, Some(&Theme::LIGHT)), "every pixel width is its own raster");
        assert_ne!(a, Key::of(p, None, ImageFormat::Svg, 960, Some(&dark)));
        assert_ne!(a, Key::of(p, None, ImageFormat::Svg, 960, None));
        // a PNG is one texture whatever its size and theme
        let q = Path::new("photo.png");
        assert_eq!(Key::of(q, None, ImageFormat::Png, 10, None), Key::of(q, None, ImageFormat::Png, 900, Some(&dark)));
        // widths are kept within the texture limit
        assert_eq!(Key::of(p, None, ImageFormat::Svg, 100_000, None).width, MAX_SIDE);
    }
}
