//! The MathCAS v2 notebook window (design `08-notebook.md`). `mc-notebook [file.mcn]` opens a notebook; the
//! kernel (`mc-kernel` next to this program, or `MATHCAS_KERNEL`) starts on the first evaluation.
// Release builds are GUI programs on Windows: no console window opens with them.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

fn main() -> eframe::Result {
    let paths: Vec<std::path::PathBuf> = std::env::args_os().skip(1).map(std::path::PathBuf::from).collect();
    let selftest = std::env::var("MCNB_SELFTEST").ok().and_then(|p| mc_notebook::selftest::SelfTest::load(&p));
    // a window is running: it opens the files as tabs
    if selftest.is_none() && !paths.is_empty() && mc_notebook::instance::send(&paths) {
        return Ok(());
    }
    let settings = mc_notebook::settings::Settings::load();
    let mut viewport = eframe::egui::ViewportBuilder::default()
        .with_inner_size([1000.0, 760.0])
        .with_min_inner_size([360.0, 260.0])
        .with_title("MathCAS Notebook")
        .with_icon(window_icon());
    if let (Some([x, y, w, h]), None) = (settings.window, &selftest) {
        viewport = viewport.with_inner_size([w, h]).with_position([x, y]);
    }
    let options = eframe::NativeOptions { viewport, ..Default::default() };
    eframe::run_native(
        "MathCAS Notebook",
        options,
        Box::new(move |cc| Ok(Box::new(mc_notebook::app::App::new(cc, paths, selftest)))),
    )
}

/// The application icon for the window's title bar and taskbar button (the executable's own icon, for Explorer and
/// shortcuts, is embedded by `build.rs`).
fn window_icon() -> eframe::egui::IconData {
    mc_notebook::app::icon_data().unwrap_or_default()
}
