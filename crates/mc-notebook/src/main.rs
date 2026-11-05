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
    let start = |options: eframe::NativeOptions| {
        let state = state.clone();
        eframe::run_native(
            "MathCAS Notebook",
            options,
            Box::new(move |cc| {
                let (paths, selftest) = state.borrow_mut().take().unwrap_or_default();
                Ok(Box::new(mc_notebook::app::App::new(cc, paths, selftest)))
            }),
        )
    };
    let options = |gpu: Option<wgpu::Backends>| {
        let mut wgpu_options = eframe::WgpuConfiguration::default();
        // vsync: at most one frame per display refresh
        wgpu_options.surface.present_mode = wgpu::PresentMode::AutoVsync;
        if let (Some(b), eframe::egui_wgpu::WgpuSetup::CreateNew(c)) = (gpu, &mut wgpu_options.wgpu_setup) {
            c.instance_descriptor.backends = b;
            c.native_adapter_selector = Some(std::sync::Arc::new(choose_adapter));
        }
        eframe::NativeOptions { viewport: viewport.clone(), wgpu_options, ..Default::default() }
    };
    match preferred_backends() {
        Some(b) => match start(options(Some(b))) {
            // no Direct3D 12 device: the default choice
            Err(_) if state.borrow().is_some() => start(options(None)),
            r => r,
        },
        None => start(options(None)),
    }
}

use eframe::wgpu;

/// The GPU to draw on: one that can present to the window, discrete before integrated before anything else, a
/// software renderer last, whatever order the driver lists them in.
fn choose_adapter(adapters: &[wgpu::Adapter], surface: Option<&wgpu::Surface<'_>>) -> Result<wgpu::Adapter, String> {
    let rank = |a: &wgpu::Adapter| match a.get_info().device_type {
        wgpu::DeviceType::DiscreteGpu => 0,
        wgpu::DeviceType::IntegratedGpu => 1,
        wgpu::DeviceType::VirtualGpu | wgpu::DeviceType::Other => 2,
        wgpu::DeviceType::Cpu => 3,
    };
    adapters
        .iter()
        .filter(|a| surface.is_none_or(|s| a.is_surface_supported(s)))
        .min_by_key(|a| rank(a))
        .cloned()
        .ok_or_else(|| "no graphics adapter can draw to the window".to_string())
}

/// The graphics interface to draw with. On Windows Direct3D 12, the most robust driver path (a GPU driver hang
/// that reset a machine was seen with the default choice, which may pick Vulkan); `WGPU_BACKEND` overrides it.
fn preferred_backends() -> Option<wgpu::Backends> {
    if cfg!(windows) && std::env::var_os("WGPU_BACKEND").is_none() {
        Some(wgpu::Backends::DX12)
    } else {
        None
    }
}

/// The application icon for the window's title bar and taskbar button (the executable's own icon, for Explorer and
/// shortcuts, is embedded by `build.rs`).
fn window_icon() -> eframe::egui::IconData {
    mc_notebook::app::icon_data().unwrap_or_default()
}
