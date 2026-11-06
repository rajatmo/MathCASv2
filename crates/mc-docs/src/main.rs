//! The MathCAS documentation window. `mc-docs [name]` opens the home page, or the page of function `name` (also
//! `guide:<area>`, `search:<text>`). `MCDOCS_DIR` reads the pages from a folder instead of the copy built in;
//! `MCDOCS_SELFTEST=<script>` runs a self-test (`mc_docs::selftest`).
// Release builds are GUI programs on Windows: no console window opens with them.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use eframe::wgpu;
use mc_docs::app::DocsApp;
use mc_docs::library::Library;
use mc_docs::selftest::SelfTest;
use mc_docs::view::Loc;

fn main() -> eframe::Result {
    let start = std::env::args().nth(1).map(|a| Loc::parse(&a));
    let selftest = std::env::var("MCDOCS_SELFTEST").ok().and_then(|p| SelfTest::load(&p));
    let viewport = eframe::egui::ViewportBuilder::default()
        .with_inner_size([1100.0, 800.0])
        .with_min_inner_size([420.0, 300.0])
        .with_title("MathCAS Documentation")
        .with_icon(mc_notebook::app::icon_data().unwrap_or_default());
    // the window's state, taken by whichever start succeeds
    let state = std::rc::Rc::new(std::cell::RefCell::new(Some((start, selftest))));
    // the graphics set-up of the notebook: the integrated GPU first, the platform's trusted interfaces in order,
    // an interface that crashed an earlier start skipped
    let run = |backends: wgpu::Backends| {
        let state = state.clone();
        let mut wgpu_options = eframe::WgpuConfiguration::default();
        wgpu_options.surface.present_mode = wgpu::PresentMode::AutoVsync;
        if let eframe::egui_wgpu::WgpuSetup::CreateNew(c) = &mut wgpu_options.wgpu_setup {
            c.instance_descriptor.backends = backends;
            c.native_adapter_selector = Some(std::sync::Arc::new(mc_notebook::gpu::choose_adapter));
        }
        let options = eframe::NativeOptions { viewport: viewport.clone(), wgpu_options, ..Default::default() };
        mc_notebook::gpu::starting(backends);
        eframe::run_native(
            "MathCAS Documentation",
            options,
            Box::new(move |cc| {
                let (start, selftest) = state.borrow_mut().take().unwrap_or_default();
                Ok(Box::new(DocsApp::new(cc, Library::load(), start, selftest)))
            }),
        )
    };
    let mut result = Ok(());
    for b in mc_notebook::gpu::to_try() {
        result = run(b);
        if result.is_ok() || state.borrow().is_none() {
            break;
        }
        // no device with this interface: the next one
        mc_notebook::gpu::failed();
    }
    result
}
