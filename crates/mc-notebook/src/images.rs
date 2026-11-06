//! Pictures in Output cells (design 6): PNG files and SVG drawings, decoded on a worker thread and kept as GPU
//! textures keyed by path, modification time and, for SVG, the raster width. A texture is uploaded once, when its
//! decode finishes, never per frame; at most [`BUDGET`] bytes of pixels are kept, the least recently drawn going
//! first; no texture side exceeds [`MAX_SIDE`] (larger pictures are scaled down while decoding).
use eframe::egui::{self, ColorImage, TextureHandle, TextureId, TextureOptions};
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
/// SVG drawings are rasterised at widths in steps of this many pixels, so a small change of the display size does
/// not make a new raster.
const SVG_STEP: u32 = 64;

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

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct Key {
    path: PathBuf,
    modified: Option<SystemTime>,
    /// the raster width of an SVG drawing; 0 for PNG
    width: u32,
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
}

/// The picture cache of a window.
pub struct Images {
    ctx: egui::Context,
    entries: HashMap<Key, Entry>,
    files: HashMap<PathBuf, FileInfo>,
    jobs: Sender<Job>,
    done: Receiver<(Key, Result<ColorImage, String>)>,
    bytes: usize,
    frame: u64,
    uploads: u64,
}

impl Images {
    /// An empty cache with its decoding thread.
    pub fn new(ctx: &egui::Context) -> Images {
        let (jobs, rx) = mpsc::channel::<Job>();
        let (tx, done) = mpsc::channel();
        let repaint = ctx.clone();
        let spawned = std::thread::Builder::new().name("mc-notebook-images".into()).spawn(move || {
            let mut fonts = None;
            for job in rx {
                let r = match job.format {
                    ImageFormat::Png => decode_png(&job.key.path),
                    ImageFormat::Svg => decode_svg(&job.key.path, job.key.width, &mut fonts),
                };
                let r = r.map(|(rgba, w, h, premultiplied)| {
                    let size = [w as usize, h as usize];
                    if premultiplied {
                        ColorImage::from_rgba_premultiplied(size, &rgba)
                    } else {
                        ColorImage::from_rgba_unmultiplied(size, &rgba)
                    }
                });
                if tx.send((job.key, r)).is_err() {
                    break;
                }
                repaint.request_repaint();
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
        }
    }

    /// At the start of a frame: upload the pictures decoded since the last one, then keep within the budget.
    pub fn begin_frame(&mut self) {
        self.frame += 1;
        while let Ok((key, result)) = self.done.try_recv() {
            let Some(e) = self.entries.get_mut(&key) else { continue };
            match result {
                Ok(img) => {
                    let bytes = img.width() * img.height() * 4;
                    let name = format!("picture:{}", key.path.display());
                    e.state = State::Ready(self.ctx.load_texture(name, img, TextureOptions::LINEAR));
                    e.bytes = bytes;
                    self.bytes += bytes;
                    self.uploads += 1;
                    // older rasters and versions of the same file are no longer needed
                    let stale: Vec<Key> = self
                        .entries
                        .keys()
                        .filter(|k| k.path == key.path && **k != key && matches!(self.entries[*k].state, State::Ready(_)))
                        .cloned()
                        .collect();
                    for k in stale {
                        self.drop_entry(&k);
                    }
                }
                Err(why) => e.state = State::Failed(why),
            }
        }
        self.evict();
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
                natural: mc_nbfile::images::picture_size(path),
                exists: meta.is_some(),
            },
        };
        self.files.insert(path.to_path_buf(), info.clone());
        info
    }

    /// The picture's own size in pixels (`None`: it cannot be read).
    pub fn natural(&mut self, path: &Path, time: f64) -> Option<(u32, u32)> {
        self.info(path, time).natural
    }

    /// The picture at `path` for this frame; an SVG drawing is rasterised `width` pixels wide (in steps).
    pub fn picture(&mut self, path: &Path, width: u32, time: f64) -> Picture {
        let info = self.info(path, time);
        if !info.exists {
            return Picture::Missing("missing".into());
        }
        let Some(format) = info.format else { return Picture::Missing("not a PNG or SVG picture".into()) };
        // a plot or graphics PNG drawn larger than its pixels (high-DPI displays, magnification): its SVG sibling,
        // rasterised at the drawn width, stays sharp
        if let (ImageFormat::Png, Some((natural, _))) = (format, info.natural) {
            let svg = path.with_extension("svg");
            if width > natural + 1 && self.info(&svg, time).exists {
                return self.picture(&svg, width, time);
            }
        }
        let width = match format {
            ImageFormat::Png => 0,
            ImageFormat::Svg => width.clamp(1, MAX_SIDE).div_ceil(SVG_STEP) * SVG_STEP,
        };
        let key = Key { path: path.to_path_buf(), modified: info.modified, width };
        let frame = self.frame;
        let e = self.entries.entry(key.clone()).or_insert_with(|| {
            let _ = self.jobs.send(Job { key: key.clone(), format });
            Entry { state: State::Loading, bytes: 0, drawn: frame }
        });
        e.drawn = frame;
        match &e.state {
            State::Ready(t) => return Picture::Ready(t.id(), t.size()),
            State::Failed(why) => return Picture::Missing(why.clone()),
            State::Loading => {}
        }
        // an older raster of the same file shows until the new one is ready
        for (k, e) in &mut self.entries {
            if let (true, State::Ready(t)) = (k.path == key.path, &e.state) {
                e.drawn = frame;
                return Picture::Ready(t.id(), t.size());
            }
        }
        Picture::Loading
    }

    /// The bytes of pixels held and the textures uploaded so far (for the self-test's log).
    pub fn stats(&self) -> (usize, usize, u64) {
        (self.bytes, self.entries.len(), self.uploads)
    }
}

/// The picture at `path` as RGBA pixels (not premultiplied), decoded now, for the clipboard: PNG files at their
/// size, SVG drawings at `svg_width` pixels wide.
pub fn pixels(path: &Path, svg_width: u32) -> Result<(Vec<u8>, u32, u32), String> {
    match ImageFormat::of(path).ok_or("not a PNG or SVG picture")? {
        ImageFormat::Png => decode_png(path).map(|(p, w, h, _)| (p, w, h)),
        ImageFormat::Svg => {
            let (mut p, w, h, _) = decode_svg(path, svg_width.clamp(1, MAX_SIDE), &mut None)?;
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

/// An SVG drawing rasterised `width` pixels wide (premultiplied RGBA), text in the system's fonts (loaded once,
/// on the first drawing with text, into `fonts`).
fn decode_svg(
    path: &Path,
    width: u32,
    fonts: &mut Option<Arc<usvg::fontdb::Database>>,
) -> Result<(Vec<u8>, u32, u32, bool), String> {
    let data = std::fs::read(path).map_err(|e| e.to_string())?;
    let fontdb = fonts
        .get_or_insert_with(|| {
            let mut db = usvg::fontdb::Database::new();
            db.load_system_fonts();
            Arc::new(db)
        })
        .clone();
    let opt = usvg::Options { resources_dir: path.parent().map(Path::to_path_buf), fontdb, ..usvg::Options::default() };
    let tree = usvg::Tree::from_data(&data, &opt).map_err(|e| format!("not a readable SVG drawing: {e}"))?;
    let size = tree.size();
    let scale = width as f32 / size.width();
    let w = width.min(MAX_SIDE);
    let h = ((size.height() * scale).ceil() as u32).clamp(1, MAX_SIDE);
    let mut pixmap = tiny_skia::Pixmap::new(w, h).ok_or("an empty drawing")?;
    resvg::render(&tree, tiny_skia::Transform::from_scale(scale, scale), &mut pixmap.as_mut());
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
        let (p, w, h, pre) = decode_svg(&svg, 300, &mut Some(Arc::new(usvg::fontdb::Database::new()))).unwrap();
        assert_eq!((w, h, pre), (300, 150, true));
        assert_eq!(&p[..4], &[255, 0, 0, 255]);
        let (_, w, h) = pixels(&svg, 64).unwrap();
        assert_eq!((w, h), (64, 32));
        assert!(decode_png(&svg).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
