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
    // the window's state, taken by whichever start succeeds
    let state = std::rc::Rc::new(std::cell::RefCell::new(Some((paths, selftest))));
    let start = |backends: wgpu::Backends| {
        let state = state.clone();
        let mut wgpu_options = eframe::WgpuConfiguration::default();
        // vsync: at most one frame per display refresh
        wgpu_options.surface.present_mode = wgpu::PresentMode::AutoVsync;
        if let eframe::egui_wgpu::WgpuSetup::CreateNew(c) = &mut wgpu_options.wgpu_setup {
            c.instance_descriptor.backends = backends;
            c.native_adapter_selector = Some(std::sync::Arc::new(mc_notebook::gpu::choose_adapter));
        }
        let options = eframe::NativeOptions { viewport: viewport.clone(), wgpu_options, ..Default::default() };
        mc_notebook::gpu::starting(backends);
        eframe::run_native(
            "MathCAS Notebook",
            options,
            Box::new(move |cc| {
                let (paths, selftest) = state.borrow_mut().take().unwrap_or_default();
                let mut app = mc_notebook::app::App::new(cc, paths, selftest);
                app.set_gpu(cc.wgpu_render_state.as_ref().map(|r| r.adapter.get_info()));
                Ok(Box::new(app))
            }),
        )
    };
    let mut result = Ok(());
    for b in mc_notebook::gpu::to_try() {
        result = start(b);
        if result.is_ok() || state.borrow().is_none() {
            break;
        }
        // no device with this interface: the next one
        mc_notebook::gpu::failed();
    }
    result
}

use eframe::wgpu;

/// The application icon for the window's title bar and taskbar button (the executable's own icon, for Explorer and
/// shortcuts, is embedded by `build.rs`).
fn window_icon() -> eframe::egui::IconData {
    mc_notebook::app::icon_data().unwrap_or_default()
}
