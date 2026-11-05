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
            .with_title("MathCAS Notebook"),
        ..Default::default()
    };
    eframe::run_native(
        "MathCAS Notebook",
        options,
        Box::new(move |cc| Ok(Box::new(mc_notebook::app::App::new(cc, path, selftest)))),
    )
}
