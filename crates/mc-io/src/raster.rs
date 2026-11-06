//! Raster images in memory: PNG decoding and encoding, conversion to and from reals in [0, 1], and resampling.
//!
//! A [`Raster`] holds interleaved samples, row-major from the top-left pixel, with 1 (gray), 2 (gray and alpha),
//! 3 (red, green, blue) or 4 (red, green, blue, alpha) channels and 8 or 16 bits per sample. Alpha is straight
//! (not premultiplied), as in PNG.

use std::io::Cursor;

/// The most samples (pixels times channels) an image may have: an image's data as reals takes 8 bytes per
/// sample, so this caps that array at 1 GiB.
pub const MAX_SAMPLES: usize = 1 << 27;

/// The widest or tallest image written or rendered, in pixels.
pub const MAX_SIDE: u32 = 30_000;

/// The samples of a raster.
#[derive(Clone, Debug, PartialEq)]
pub enum Samples {
    /// 8 bits per sample (0..=255)
    U8(Vec<u8>),
    /// 16 bits per sample (0..=65535)
    U16(Vec<u16>),
}

/// An image in memory.
#[derive(Clone, Debug, PartialEq)]
pub struct Raster {
    /// pixels per row
    pub width: u32,
    /// rows
    pub height: u32,
    /// samples per pixel, 1 to 4
    pub channels: u8,
    /// the samples, `width * height * channels` of them
    pub samples: Samples,
    /// the resolution in dots per inch, when known (PNG `pHYs`)
    pub dpi: Option<f64>,
}

/// Why a PNG file could not be read.
#[derive(Clone, Debug, PartialEq)]
pub enum ReadError {
    /// not a PNG file, or a damaged one (the decoder's message)
    Damaged(String),
    /// a valid file whose pixels exceed [`MAX_SAMPLES`]
    TooLarge(String),
}

impl std::fmt::Display for ReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReadError::Damaged(s) | ReadError::TooLarge(s) => f.write_str(s),
        }
    }
}

/// The header of a PNG file: what [`info`] reads without decoding the pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Info {
    /// pixels per row
    pub width: u32,
    /// rows
    pub height: u32,
    /// samples per pixel after expansion (palette images count as 3 or 4)
    pub channels: u8,
    /// bits per sample after expansion (8 or 16)
    pub depth: u8,
}

const METERS_PER_INCH: f64 = 0.0254;

impl Raster {
    /// The bits per sample, 8 or 16.
    pub fn depth(&self) -> u8 {
        match self.samples {
            Samples::U8(_) => 8,
            Samples::U16(_) => 16,
        }
    }

    /// The number of samples.
    pub fn sample_count(&self) -> usize {
        match &self.samples {
            Samples::U8(v) => v.len(),
            Samples::U16(v) => v.len(),
        }
    }

    /// Does the image have an alpha channel?
    pub fn has_alpha(&self) -> bool {
        self.channels == 2 || self.channels == 4
    }

    /// Sample `i` as a real in [0, 1].
    #[inline]
    pub fn real(&self, i: usize) -> f64 {
        match &self.samples {
            Samples::U8(v) => v[i] as f64 / 255.0,
            Samples::U16(v) => v[i] as f64 / 65535.0,
        }
    }

    /// The image of `width * height * channels` reals (row-major, interleaved), each clipped to [0, 1] (NaN as 0)
    /// and rounded to the nearest of the `depth`-bit levels.
    pub fn from_reals(width: u32, height: u32, channels: u8, data: &[f64], depth: u8) -> Raster {
        let clip = |x: f64| if x.is_nan() { 0.0 } else { x.clamp(0.0, 1.0) };
        let samples = if depth == 16 {
            Samples::U16(data.iter().map(|&x| (clip(x) * 65535.0).round() as u16).collect())
        } else {
            Samples::U8(data.iter().map(|&x| (clip(x) * 255.0).round() as u8).collect())
        };
        Raster { width, height, channels, samples, dpi: None }
    }

    /// The same image with `depth` bits per sample (16 to 8 rounds to the nearest level; 8 to 16 is exact).
    pub fn with_depth(self, depth: u8) -> Raster {
        let samples = match (self.samples, depth) {
            (Samples::U8(v), 16) => Samples::U16(v.into_iter().map(|s| s as u16 * 257).collect()),
            (Samples::U16(v), 8) => {
                Samples::U8(v.into_iter().map(|s| ((s as u32 * 255 + 32_767) / 65_535) as u8).collect())
            }
            (s, _) => s,
        };
        Raster { samples, ..self }
    }

    /// The image as PNG file bytes (with a `pHYs` chunk when the resolution is known).
    pub fn to_png(&self) -> Result<Vec<u8>, String> {
        let mut out = Vec::new();
        let mut enc = png::Encoder::new(&mut out, self.width, self.height);
        enc.set_color(match self.channels {
            1 => png::ColorType::Grayscale,
            2 => png::ColorType::GrayscaleAlpha,
            3 => png::ColorType::Rgb,
            _ => png::ColorType::Rgba,
        });
        enc.set_depth(if self.depth() == 16 { png::BitDepth::Sixteen } else { png::BitDepth::Eight });
        enc.set_compression(png::Compression::Fast);
        if let Some(dpi) = self.dpi.filter(|d| d.is_finite() && *d > 0.0) {
            let ppm = (dpi / METERS_PER_INCH).round() as u32;
            enc.set_pixel_dims(Some(png::PixelDimensions { xppu: ppm, yppu: ppm, unit: png::Unit::Meter }));
        }
        let mut w = enc.write_header().map_err(|e| e.to_string())?;
        match &self.samples {
            Samples::U8(v) => w.write_image_data(v),
            Samples::U16(v) => w.write_image_data(&big_endian(v)),
        }
        .map_err(|e| e.to_string())?;
        w.finish().map_err(|e| e.to_string())?;
        Ok(out)
    }

    /// The image `scale` times as wide and as tall (at least one pixel each way): a triangle filter whose support
    /// widens with the reduction, so shrinking averages the covered pixels and enlarging interpolates linearly.
    /// Colour is filtered premultiplied by alpha, so transparent pixels do not bleed their colour. `None` when
    /// `stopped` (polled once per row) says so.
    pub fn resized(&self, scale: f64, stopped: &dyn Fn() -> bool) -> Option<Raster> {
        let w2 = ((self.width as f64 * scale).round() as u32).clamp(1, MAX_SIDE);
        let h2 = ((self.height as f64 * scale).round() as u32).clamp(1, MAX_SIDE);
        let c = self.channels as usize;
        let alpha = self.has_alpha();
        let mut px: Vec<f32> = (0..self.sample_count()).map(|i| self.real(i) as f32).collect();
        if alpha {
            for p in px.chunks_exact_mut(c) {
                let a = p[c - 1];
                p[..c - 1].iter_mut().for_each(|v| *v *= a);
            }
        }
        let (w, h) = (self.width as usize, self.height as usize);
        // rows first (width w -> w2), then columns (height h -> h2)
        let mut mid = vec![0f32; w2 as usize * h * c];
        let wx = weights(w, w2 as usize);
        for y in 0..h {
            if stopped() {
                return None;
            }
            for (x2, ws) in wx.iter().enumerate() {
                for k in 0..c {
                    mid[(y * w2 as usize + x2) * c + k] =
                        ws.iter().map(|&(x, f)| f * px[(y * w + x) * c + k]).sum();
                }
            }
        }
        let mut out = vec![0f32; w2 as usize * h2 as usize * c];
        let wy = weights(h, h2 as usize);
        for (y2, ws) in wy.iter().enumerate() {
            if stopped() {
                return None;
            }
            for x2 in 0..w2 as usize {
                for k in 0..c {
                    out[(y2 * w2 as usize + x2) * c + k] =
                        ws.iter().map(|&(y, f)| f * mid[(y * w2 as usize + x2) * c + k]).sum();
                }
            }
        }
        if alpha {
            for p in out.chunks_exact_mut(c) {
                let a = p[c - 1];
                p[..c - 1].iter_mut().for_each(|v| *v = if a > 0.0 { *v / a } else { 0.0 });
            }
        }
        let data: Vec<f64> = out.into_iter().map(f64::from).collect();
        let mut r = Raster::from_reals(w2, h2, self.channels, &data, self.depth());
        r.dpi = self.dpi;
        Some(r)
    }
}

/// For each of `n2` output positions, the input positions of `n` and their normalised weights.
fn weights(n: usize, n2: usize) -> Vec<Vec<(usize, f32)>> {
    let ratio = n as f64 / n2 as f64;
    let support = ratio.max(1.0);
    (0..n2)
        .map(|i| {
            let centre = (i as f64 + 0.5) * ratio - 0.5;
            let lo = (centre - support).floor().max(0.0) as usize;
            let hi = ((centre + support).ceil() as usize).min(n - 1);
            let mut ws: Vec<(usize, f32)> = (lo..=hi)
                .map(|j| (j, (1.0 - (j as f64 - centre).abs() / support).max(0.0) as f32))
                .filter(|&(_, f)| f > 0.0)
                .collect();
            if ws.is_empty() {
                ws.push((centre.round().clamp(0.0, (n - 1) as f64) as usize, 1.0));
            }
            let total: f32 = ws.iter().map(|&(_, f)| f).sum();
            ws.iter_mut().for_each(|(_, f)| *f /= total);
            ws
        })
        .collect()
}

fn big_endian(v: &[u16]) -> Vec<u8> {
    v.iter().flat_map(|s| s.to_be_bytes()).collect()
}

fn decoder(bytes: &[u8]) -> png::Decoder<Cursor<&[u8]>> {
    // twice the largest 16-bit RGBA image allowed, for the decoder's own buffers
    let mut d = png::Decoder::new_with_limits(Cursor::new(bytes), png::Limits { bytes: MAX_SAMPLES * 4 });
    // palette to RGB(A), gray below 8 bits to 8, tRNS to an alpha channel; 16 bits stay 16
    d.set_transformations(png::Transformations::EXPAND);
    d
}

fn damaged(e: png::DecodingError) -> ReadError {
    ReadError::Damaged(format!("damaged PNG file: {e}"))
}

/// Does `bytes` start with the PNG signature?
pub fn is_png(bytes: &[u8]) -> bool {
    bytes.starts_with(b"\x89PNG\r\n\x1a\n")
}

/// The header of the PNG file `bytes` (its chunks up to the image data are checked).
pub fn info(bytes: &[u8]) -> Result<Info, ReadError> {
    if !is_png(bytes) {
        return Err(ReadError::Damaged("not a PNG file".into()));
    }
    let r = decoder(bytes).read_info().map_err(damaged)?;
    let (color, depth) = r.output_color_type();
    let i = r.info();
    Ok(Info { width: i.width, height: i.height, channels: color.samples() as u8, depth: depth as u8 })
}

/// Decode the PNG file `bytes`.
pub fn decode(bytes: &[u8]) -> Result<Raster, ReadError> {
    let head = info(bytes)?;
    let too_large = || ReadError::TooLarge(format!("the image is too large ({} x {} pixels)", head.width, head.height));
    if head.width as usize * head.height as usize * head.channels as usize > MAX_SAMPLES {
        return Err(too_large());
    }
    let mut r = decoder(bytes).read_info().map_err(damaged)?;
    let size = r.output_buffer_size().ok_or_else(too_large)?;
    let mut buf = vec![0u8; size];
    let out = r.next_frame(&mut buf).map_err(damaged)?;
    buf.truncate(out.buffer_size());
    let dpi = r.info().pixel_dims.and_then(|p| {
        (p.unit == png::Unit::Meter && p.xppu > 0).then(|| (p.xppu as f64 * METERS_PER_INCH * 100.0).round() / 100.0)
    });
    let samples = if out.bit_depth == png::BitDepth::Sixteen {
        Samples::U16(buf.as_chunks::<2>().0.iter().map(|&b| u16::from_be_bytes(b)).collect())
    } else {
        Samples::U8(buf)
    };
    Ok(Raster { width: out.width, height: out.height, channels: out.color_type.samples() as u8, samples, dpi })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn checker(channels: u8, depth: u8) -> Raster {
        let (w, h) = (5u32, 3u32);
        let data: Vec<f64> =
            (0..w * h * channels as u32).map(|i| (i % 7) as f64 / 6.0).collect();
        Raster::from_reals(w, h, channels, &data, depth)
    }

    #[test]
    fn png_round_trips_every_layout() {
        for c in 1..=4 {
            for d in [8, 16] {
                let mut r = checker(c, d);
                r.dpi = Some(300.0);
                let back = decode(&r.to_png().unwrap()).unwrap();
                assert_eq!(back, r, "{c} channels, {d} bits");
                assert_eq!(info(&r.to_png().unwrap()).unwrap(), Info { width: 5, height: 3, channels: c, depth: d });
            }
        }
    }

    #[test]
    fn depth_conversion() {
        let r = checker(3, 8);
        assert_eq!(r.clone().with_depth(16).with_depth(8), r);
        let r = Raster::from_reals(1, 1, 1, &[0.5], 16).with_depth(8);
        assert_eq!(r.samples, Samples::U8(vec![128]));
    }

    #[test]
    fn reals_are_clipped() {
        let r = Raster::from_reals(4, 1, 1, &[-1.0, f64::NAN, 2.0, 0.5], 8);
        assert_eq!(r.samples, Samples::U8(vec![0, 0, 255, 128]));
    }

    #[test]
    fn resizing() {
        // a constant image stays constant; halving a 2-pixel stripe averages it
        let r = Raster::from_reals(4, 4, 3, &[0.25; 48], 8);
        let s = r.resized(2.5, &|| false).unwrap();
        assert_eq!((s.width, s.height), (10, 10));
        assert!((0..s.sample_count()).all(|i| (s.real(i) - 64.0 / 255.0).abs() < 1e-9));
        let r = Raster::from_reals(2, 1, 1, &[0.0, 1.0], 8);
        assert_eq!(r.resized(0.5, &|| false).unwrap().samples, Samples::U8(vec![128]));
        // transparent pixels do not darken their neighbours
        let r = Raster::from_reals(2, 1, 2, &[0.0, 0.0, 1.0, 1.0], 8);
        assert_eq!(r.resized(0.5, &|| false).unwrap().samples, Samples::U8(vec![255, 128]));
    }

    #[test]
    fn damaged_files_fail() {
        assert!(info(b"not a png").is_err());
        let png = checker(3, 8).to_png().unwrap();
        for cut in [8, 20, 33, png.len() / 2, png.len() - 13] {
            assert!(decode(&png[..cut]).is_err(), "cut at {cut}");
        }
        let mut bad = png.clone();
        let n = bad.len();
        bad[n / 2] ^= 0x55;
        let _ = decode(&bad); // a CRC or zlib error, never a panic
    }
}
