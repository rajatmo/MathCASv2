//! The MathCAS documentation window. `mc-docs [name]` opens the home page, or the page of function `name` (also
//! `guide:<area>`, `search:<text>`). `MCDOCS_DIR` reads the pages from a folder instead of the copy built in;
//! `MCDOCS_SELFTEST=<script>` runs a self-test (`mc_docs::selftest`).
// Release builds are GUI programs on Windows: no console window opens with them.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

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
    // the notebook's graphics set-up: the integrated GPU first, the platform's trusted interfaces in order, an
    // interface that crashed an earlier start skipped
    mc_ui::gpu::run_native("MathCAS Documentation", viewport, (start, selftest), |cc, (start, selftest)| {
        Box::new(DocsApp::new(cc, Library::load(), start, selftest))
    })
}
