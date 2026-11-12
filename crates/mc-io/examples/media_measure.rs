//! Timing of PNG and PDF work on the assembled engine: a 4000 x 3000 RGB PNG imported (header only, then its
//! pixels as a packed array) and written back, and a 10-page PDF rendered at 150 dpi:
//! `cargo run --release -p mc-io --example media_measure [folder]`. Run it under the guard with a memory cap; each
//! step prints its time and a `[k/N pct%]` progress mark.
use mc_api::{Engine, StopFlag};
use std::time::Instant;

/// A 4000 x 3000 RGB PNG of smooth gradients with some texture (compresses like a photograph more than a flat image).
fn big_png(path: &std::path::Path) {
    let (w, h) = (4000u32, 3000u32);
    let mut px = Vec::with_capacity((w * h * 3) as usize);
    let mut s = 0x2545_f491_4f6c_dd1du64;
    for y in 0..h {
        for x in 0..w {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            let noise = (s % 16) as u32;
            px.extend_from_slice(&[(x * 255 / w + noise) as u8, (y * 255 / h) as u8, ((x + y) % 256) as u8]);
        }
    }
    let mut out = Vec::new();
    let mut enc = png::Encoder::new(&mut out, w, h);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    let mut wr = enc.write_header().unwrap();
    wr.write_image_data(&px).unwrap();
    wr.finish().unwrap();
    std::fs::write(path, out).unwrap();
}

fn main() {
    let dir = std::env::args().nth(1).map(std::path::PathBuf::from).unwrap_or_else(|| std::env::temp_dir().join("mc-io-measure"));
    std::fs::create_dir_all(&dir).unwrap();
    let d = dir.to_string_lossy().replace('\\', "/");
    let t0 = Instant::now();
    big_png(&dir.join("big.png"));
    println!("[0/9 0%] wrote the 4000 x 3000 test PNG in {:.0} ms", t0.elapsed().as_secs_f64() * 1e3);
    // rendered pages are cached by content: start the PDF steps from an empty cache
    if let Ok(entries) = std::fs::read_dir(std::env::temp_dir().join("mathcas-images")) {
        for e in entries.flatten() {
            if e.file_name().to_string_lossy().starts_with("pdf-") {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
    let steps = [
        ("import_data (header)", format!("img = import_data(\"{d}/big.png\"); image_dimensions(img)")),
        ("image_data (packed)", "a = image_data(img); length(a)".to_string()),
        ("total of the pixels", "total(a)".to_string()),
        ("image(array) (write)", "image_dimensions(image(a))".to_string()),
        ("export_data scale 0.25", format!("export_data(\"{d}/small.png\", img, scale: 0.25)")),
        ("10 plots to a 10-page PDF", format!("export_data(\"{d}/ten.pdf\", table(plot(sin(k*x), [x, 0, 6]), k, 10))")),
        ("10 pages at 150 dpi", format!("map(image_dimensions, import_data(\"{d}/ten.pdf\", \"pages\", dpi: 150))[1]")),
        ("the same again (cached)", format!("length(import_data(\"{d}/ten.pdf\", \"pages\", dpi: 150))")),
        ("text of 10 pages", format!("length(import_data(\"{d}/ten.pdf\", \"text\"))")),
    ];
    let mut e = mc_engine::CoreEngine::new();
    let stop = StopFlag::new();
    for (k, (what, src)) in steps.iter().enumerate() {
        let t0 = Instant::now();
        let out = match e.eval(src, &stop) {
            Ok(r) => r.printed,
            Err(err) => format!("error: {err:?}"),
        };
        let ms = t0.elapsed().as_secs_f64() * 1e3;
        let shown: String = out.chars().take(50).collect();
        println!("[{}/{} {}%] {what:<26} {ms:>9.1} ms   {shown}", k + 1, steps.len(), (k + 1) * 100 / steps.len());
    }
    std::fs::remove_dir_all(&dir).ok();
}
