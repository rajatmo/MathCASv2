//! The MathCAS v2 notebook window (design `08-notebook.md`, sections 8 - 10): the egui application around the
//! document model of `mc-nbmodel`, a client for the `mc-kernel` process, the Atom One themes and the self-test.
pub mod about;
pub mod dialog;
pub mod app;
/// The graphics set-up, shared with the documentation window.
pub use mc_ui::gpu;
mod highlight;
pub mod images;
mod input;
pub mod instance;
pub mod kernel;
/// Which layout the window has.
pub mod layout {
    /// The layout for phones and tablets: always on Android, elsewhere with `MCNB_LAYOUT=mobile` (to try it out and
    /// to test it on a desktop).
    pub fn mobile() -> bool {
        static MOBILE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        *MOBILE.get_or_init(|| {
            cfg!(target_os = "android") || std::env::var_os("MCNB_LAYOUT").is_some_and(|v| v == "mobile")
        })
    }
}
pub mod outputs;
pub mod plothover;
pub mod selftest;
pub mod settings;
mod tabbar;
mod touch;
mod typeset;
pub mod theme;
mod view;
pub mod zoom;
pub mod rotate3d;

/// Open the notebook window with the notebooks at `paths` and run it until it is closed: what `main` does on a
/// desktop system and `android_main` on Android. The window's place and size of the last session are restored
/// (not in a self-test).
pub fn run(paths: Vec<std::path::PathBuf>, selftest: Option<selftest::SelfTest>) -> eframe::Result {
    let settings = settings::Settings::load();
    // the application icon for the window's title bar and taskbar button (the executable's own icon, for Explorer
    // and shortcuts, is embedded by `build.rs`)
    let mut viewport = eframe::egui::ViewportBuilder::default()
        .with_inner_size([1000.0, 760.0])
        .with_min_inner_size([360.0, 260.0])
        .with_title("MathCAS Notebook")
        .with_icon(app::icon_data().unwrap_or_default());
    if let (Some([x, y, w, h]), None) = (settings.window, &selftest) {
        viewport = viewport.with_inner_size([w, h]).with_position([x, y]);
    }
    mc_ui::gpu::run_native("MathCAS Notebook", viewport, (paths, selftest), |cc, (paths, selftest)| {
        let mut app = app::App::new(cc, paths, selftest);
        app.set_gpu(cc.wgpu_render_state.as_ref().map(|r| r.adapter.get_info()));
        Box::new(app)
    })
}
