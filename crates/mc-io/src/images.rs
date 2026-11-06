//! Image values and the image functions.
//!
//! An image is the value `image("path")` naming a PNG file (decision IO.1): plots are already such values and front
//! ends display them, and a large image stays one small term. `image(array)` writes the array as a PNG into the
//! system's temporary folder (`mathcas-images/`, the file named by a hash of its content, so equal arrays give
//! equal values) and gives `image(path)`; `image_data` decodes the file into a packed array of reals.

use crate::raster::{self, Raster, ReadError, MAX_SAMPLES, MAX_SIDE};
use crate::{failure, Opts};
use mc_api::names;
use mc_core::{Heap, Packed, Store, View, T};
use mc_eval::{NativeCx, Native, Reply, Res};
use std::path::{Path, PathBuf};

/// The head of image values.
pub const IMAGE: &str = "image";

/// The file of an `image(path)` value.
pub fn image_path<'h>(h: &'h Heap, t: T<'h>) -> Option<&'h str> {
    let View::App { head, args: [p] } = h.view(t) else { return None };
    if head != h.sym_term(h.sym(IMAGE)) {
        return None;
    }
    match h.view(*p) {
        View::Str(s) => Some(s),
        _ => None,
    }
}

/// The value `image(path)`, the path with forward slashes (it prints without escapes on every system).
pub fn image_term<'h>(h: &'h Heap, path: &Path) -> T<'h> {
    let p = path.to_string_lossy();
    let p = p.trim_start_matches(r"\\?\").replace('\\', "/");
    h.call(h.sym(IMAGE), &[h.string(&p)])
}

/// The folder for image files this package writes.
pub fn image_dir() -> PathBuf {
    std::env::temp_dir().join("mathcas-images")
}

/// FNV-1a, 64 bits: names content-addressed files (not a security boundary).
pub fn fnv(bytes: &[u8], seed: u64) -> u64 {
    bytes.iter().fold(seed ^ 0xcbf2_9ce4_8422_2325, |h, &b| (h ^ b as u64).wrapping_mul(0x0100_0000_01b3))
}

/// Write `bytes` to `image_dir()/<stem>-<hash of bytes>.<ext>` unless that file already exists with this size;
/// the path.
pub fn save_content(stem: &str, ext: &str, bytes: &[u8]) -> Result<PathBuf, String> {
    let dir = image_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!("{stem}-{:016x}.{ext}", fnv(bytes, 0)));
    if std::fs::metadata(&path).map(|m| m.len() != bytes.len() as u64).unwrap_or(true) {
        std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
    }
    Ok(path)
}

/// Save `r` as a PNG file in [`image_dir`] and give its image value.
pub fn save<'h>(h: &'h Heap, r: &Raster) -> Result<T<'h>, String> {
    let png = r.to_png()?;
    Ok(image_term(h, &save_content("img", "png", &png)?))
}

/// The failure for an image file that could not be read: `failure(io, file, message)` when the file could not be
/// read, `failure(damaged, file, message)` when it is not a valid PNG file, `failure(too_large, file, message)`.
pub fn read_failure<'h>(h: &'h Heap, file: T<'h>, e: ReadError) -> T<'h> {
    match e {
        ReadError::Damaged(m) => failure(h, "damaged", &[file, h.string(&m)]),
        ReadError::TooLarge(m) => failure(h, "too_large", &[file, h.string(&m)]),
    }
}

/// The bytes of the file `path`, or `failure(io, file, message)`.
pub fn read_file<'h>(h: &'h Heap, file: T<'h>, path: &str) -> Result<Vec<u8>, T<'h>> {
    std::fs::read(path).map_err(|e| failure(h, "io", &[file, h.string(&e.to_string())]))
}

/// The decoded image of the PNG file `path`, or a failure naming `file`.
pub fn load<'h>(h: &'h Heap, file: T<'h>, path: &str) -> Result<Raster, T<'h>> {
    let bytes = read_file(h, file, path)?;
    raster::decode(&bytes).map_err(|e| read_failure(h, file, e))
}

/// The header of the PNG file `path`, or a failure naming `file`.
pub fn header<'h>(h: &'h Heap, file: T<'h>, path: &str) -> Result<raster::Info, T<'h>> {
    let bytes = read_file(h, file, path)?;
    raster::info(&bytes).map_err(|e| read_failure(h, file, e))
}

/// The pixels of `r` as the packed array height x width x channels of reals in [0, 1].
pub fn to_array<'h>(cx: &mut NativeCx<'_, '_, 'h>, r: &Raster) -> Res<T<'h>> {
    let n = r.sample_count();
    let mut data = Vec::with_capacity(n);
    const CHUNK: usize = 1 << 16;
    for lo in (0..n).step_by(CHUNK) {
        cx.tick(1)?;
        data.extend((lo..(lo + CHUNK).min(n)).map(|i| r.real(i)));
    }
    let dims = [r.height as usize, r.width as usize, r.channels as usize];
    Ok(cx.heap().real_array(&dims, &data))
}

/// The dimensions (outermost first) and leaves of a rectangular array of real numbers, rank at most `max_rank`:
/// packed arrays are read directly, nested lists level by level, exact numbers through `n`. `None` when `t` is not
/// such an array.
pub fn numeric_array<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    t: T<'h>,
    max_rank: usize,
) -> Res<Option<(Vec<usize>, Vec<f64>)>> {
    let h = cx.heap();
    // the shape, from the first element at each level
    let mut dims = Vec::new();
    let mut cur = t;
    loop {
        if let Some(p) = h.packed(cur) {
            dims.extend(p.dims().iter().map(|&d| d as usize));
            break;
        }
        match cx.as_list(cur) {
            Some(a) if !a.is_empty() && dims.len() < max_rank => {
                dims.push(a.len());
                cur = a[0];
            }
            _ => break,
        }
    }
    if dims.is_empty() || dims.len() > max_rank {
        return Ok(None);
    }
    let total: usize = dims.iter().product();
    if total > MAX_SAMPLES {
        return Ok(None);
    }
    let mut data = Vec::with_capacity(total);
    // depth-first over an explicit stack, children pushed in reverse so leaves come out in order
    let mut stack = vec![(t, 0usize)];
    let mut steps = 0usize;
    while let Some((u, d)) = stack.pop() {
        steps += 1;
        if steps.is_multiple_of(4096) {
            cx.tick(1)?;
        }
        if d == dims.len() {
            let x = match h.view(u) {
                View::Int(i) => i as f64,
                View::Real(x) => x,
                View::Sym(_) | View::Str(_) => return Ok(None),
                _ => match h.view(cx.eval(h.call(h.sym("n"), &[u]))?) {
                    View::Real(x) => x,
                    View::Int(i) => i as f64,
                    _ => return Ok(None),
                },
            };
            data.push(x);
            continue;
        }
        if let Some(p) = h.packed(u) {
            if p.dims().len() != dims.len() - d || p.dims().iter().zip(&dims[d..]).any(|(&a, &b)| a as usize != b) {
                return Ok(None);
            }
            match p {
                Packed::Reals { data: v, .. } => data.extend_from_slice(v),
                Packed::Ints { data: v, .. } => data.extend(v.iter().map(|&i| i as f64)),
            }
            continue;
        }
        match cx.as_list(u) {
            Some(a) if a.len() == dims[d] => stack.extend(a.iter().rev().map(|&e| (e, d + 1))),
            _ => return Ok(None),
        }
    }
    Ok(Some((dims, data)))
}

/// The image of an array height x width (gray) or height x width x channels (1 to 4), or `None`.
pub fn raster_of<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, depth: u8) -> Res<Option<Raster>> {
    let Some((dims, data)) = numeric_array(cx, t, 3)? else { return Ok(None) };
    let (hgt, w, c) = match dims[..] {
        [hgt, w] => (hgt, w, 1),
        [hgt, w, c] if (1..=4).contains(&c) => (hgt, w, c),
        _ => return Ok(None),
    };
    if w > MAX_SIDE as usize || hgt > MAX_SIDE as usize {
        return Ok(None);
    }
    Ok(Some(Raster::from_reals(w as u32, hgt as u32, c as u8, &data, depth)))
}

/// `image(array)`, `image(array, bit_depth: 16)`: the image of an array of reals in [0, 1]. `image("path")` is
/// an image value already and stays as it is.
pub struct ImageOf;

impl Native for ImageOf {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let (a, o) = match args {
            [a] => (*a, None),
            [a, o] => (*a, Some(*o)),
            _ => return Ok(Reply::Decline),
        };
        if o.is_none() && matches!(h.view(a), View::Str(_)) {
            return Ok(Reply::Decline);
        }
        let opts = match Opts::parse(h, o, &["bit_depth"]) {
            Ok(o) => o,
            Err(f) => return Ok(Reply::Done(f)),
        };
        let depth = match opts.bit_depth(h) {
            Ok(d) => d.unwrap_or(8),
            Err(f) => return Ok(Reply::Done(f)),
        };
        let Some(r) = raster_of(cx, a, depth)? else {
            return Ok(Reply::Done(failure(h, "argument", &[h.sym_term(h.sym(IMAGE)), h.int(1)])));
        };
        cx.effect()?;
        Ok(Reply::Done(save(h, &r).unwrap_or_else(|e| failure(h, "io", &[h.string(&e)]))))
    }
}

/// The function names of the image functions taking one image value.
#[derive(Clone, Copy)]
pub enum Query {
    /// `image_data(img)`
    Data,
    /// `image_dimensions(img)`
    Dimensions,
    /// `image_channels(img)`
    Channels,
}

/// `image_data(img)`, `image_dimensions(img)`, `image_channels(img)`.
pub struct ImageQuery(pub Query);

impl ImageQuery {
    fn name(&self) -> &'static str {
        match self.0 {
            Query::Data => "image_data",
            Query::Dimensions => "image_dimensions",
            Query::Channels => "image_channels",
        }
    }
}

impl Native for ImageQuery {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let [img] = args else { return Ok(Reply::Decline) };
        let Some(path) = image_path(h, *img) else {
            return Ok(Reply::Done(failure(h, "argument", &[h.sym_term(h.sym(self.name())), h.int(1)])));
        };
        // reading a file depends on the world: in program order, never memoised
        cx.effect()?;
        cx.volatile();
        let file = h.string(path);
        Ok(Reply::Done(match self.0 {
            Query::Data => match load(h, file, path) {
                Ok(r) => to_array(cx, &r)?,
                Err(f) => f,
            },
            Query::Dimensions => match header(h, file, path) {
                Ok(i) => h.call(h.sym(names::LIST), &[h.int(i.width as i64), h.int(i.height as i64)]),
                Err(f) => f,
            },
            Query::Channels => match header(h, file, path) {
                Ok(i) => h.int(i.channels as i64),
                Err(f) => f,
            },
        }))
    }
}
