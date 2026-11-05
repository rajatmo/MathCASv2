//! The MathCAS v2 notebook window (design `08-notebook.md`). `mc-notebook [file.mcn]` opens a notebook; the
//! kernel (`mc-kernel` next to this program, or `MATHCAS_KERNEL`) starts on the first evaluation.
// Release builds are GUI programs on Windows: no console window opens with them.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

fn main() -> eframe::Result {
    let path = std::env::args_os().nth(1).map(std::path::PathBuf::from);
    let selftest = std::env::var("MCNB_SELFTEST").ok().and_then(|p| mc_notebook::selftest::SelfTest::load(&p));
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1000.0, 760.0])
            .with_min_inner_size([360.0, 260.0])
            .with_title("MathCAS Notebook")
            .with_icon(window_icon()),
        ..Default::default()
    };
    eframe::run_native(
        "MathCAS Notebook",
        options,
        Box::new(move |cc| Ok(Box::new(mc_notebook::app::App::new(cc, path, selftest)))),
    )
}

/// The application icon for the window's title bar and taskbar button (the executable's own icon, for Explorer and
/// shortcuts, is embedded by `build.rs`).
fn window_icon() -> eframe::egui::IconData {
    const PNG: &[u8] = include_bytes!("../../../assets/app/icon_256.png");
    let decode = || -> Option<eframe::egui::IconData> {
        let mut dec = png::Decoder::new(std::io::Cursor::new(PNG));
        dec.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
        let mut reader = dec.read_info().ok()?;
        let mut buf = vec![0; reader.output_buffer_size()?];
        let info = reader.next_frame(&mut buf).ok()?;
        let px = &buf[..info.buffer_size()];
        let rgba = match info.color_type {
            png::ColorType::Rgba => px.to_vec(),
            png::ColorType::Rgb => px.chunks(3).flat_map(|c| [c[0], c[1], c[2], 255]).collect(),
            png::ColorType::GrayscaleAlpha => px.chunks(2).flat_map(|c| [c[0], c[0], c[0], c[1]]).collect(),
            png::ColorType::Grayscale => px.iter().flat_map(|&g| [g, g, g, 255]).collect(),
            png::ColorType::Indexed => return None,
        };
        Some(eframe::egui::IconData { rgba, width: info.width, height: info.height })
    };
    decode().unwrap_or_default()
}
