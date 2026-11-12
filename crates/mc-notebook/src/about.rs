//! The About page (Help > About): what this build is, the system it runs on (copyable for bug reports) and the
//! components it is made from with their licences.
use eframe::egui::{self, vec2, Color32, Key, RichText, TextureHandle};

/// The page's tabs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tab {
    #[default]
    About,
    System,
    Credits,
}

/// The build: version, commit and date (embedded by `build.rs`).
pub fn build_line() -> String {
    format!(
        "Version {} \u{b7} commit {} \u{b7} built {} UTC",
        env!("CARGO_PKG_VERSION"),
        option_env!("MC_COMMIT").unwrap_or("unknown"),
        option_env!("MC_BUILD_DATE").unwrap_or("unknown")
    )
}

/// A `Debug` name in words: `NotStarted` -> "not started".
pub fn words(debug: &str) -> String {
    let mut out = String::new();
    for (i, c) in debug.chars().enumerate() {
        if c.is_uppercase() && i > 0 {
            out.push(' ');
        }
        out.extend(c.to_lowercase());
    }
    out
}

/// What the System tab shows, gathered by the app.
pub struct SystemInfo {
    pub rows: Vec<(&'static str, String)>,
}

impl SystemInfo {
    /// Gather the rows: the platform, the GPU the window draws with, the display, the kernel and the settings.
    pub fn gather(
        gpu: Option<&eframe::wgpu::AdapterInfo>,
        pixels_per_point: f32,
        magnification: f32,
        kernel: &str,
    ) -> SystemInfo {
        let mut rows = vec![
            ("MathCAS Notebook", build_line()),
            ("Platform", format!("{} {}", std::env::consts::OS, std::env::consts::ARCH)),
        ];
        match gpu {
            Some(g) => {
                rows.push(("GPU", format!("{} ({:?})", g.name, g.device_type)));
                rows.push(("Graphics interface", format!("{:?}", g.backend)));
                if !g.driver.is_empty() {
                    rows.push(("Driver", g.driver.clone()));
                }
            }
            None => rows.push(("GPU", "unknown".into())),
        }
        rows.push(("Display scale", format!("{:.0} %", pixels_per_point * 100.0)));
        rows.push(("Magnification", format!("{:.0} %", magnification * 100.0)));
        rows.push(("Kernel", kernel.to_string()));
        rows.push((
            "Settings folder",
            crate::settings::config_dir().map_or_else(|| "none".into(), |d| d.display().to_string()),
        ));
        SystemInfo { rows }
    }

    /// The rows as plain text (the Copy button).
    pub fn text(&self) -> String {
        self.rows.iter().map(|(k, v)| format!("{k}: {v}\n")).collect()
    }
}

/// The components the program is made from, with their licences.
const CREDITS: &[(&str, &str, &str)] = &[
    ("egui, eframe", "the window toolkit", "MIT or Apache-2.0"),
    ("wgpu", "drawing on the GPU (Direct3D 12, Vulkan, Metal, OpenGL)", "MIT or Apache-2.0"),
    ("winit", "windows and input", "Apache-2.0"),
    ("STIX Two Math", "the mathematics font", "SIL Open Font License 1.1"),
    ("DejaVu Sans Mono, DejaVu Sans", "the code font and symbols", "Bitstream Vera licence, public domain"),
    ("Noto Sans", "the text font", "Apache-2.0"),
    ("Atom One Light and One Dark", "the colour themes", "MIT"),
    ("Cranelift", "machine code for compiled functions (kernel)", "Apache-2.0 with LLVM exception"),
    ("tiny-skia", "plot rendering", "BSD-3-Clause"),
    ("ab_glyph, ttf-parser", "font reading and rasterising", "Apache-2.0 / MIT or Apache-2.0"),
    ("png", "image files", "MIT or Apache-2.0"),
    ("rfd", "file dialogs", "MIT"),
    ("arboard, clipboard-win", "the clipboard", "MIT or Apache-2.0 / MIT"),
    ("serde_json", "settings and the kernel protocol", "MIT or Apache-2.0"),
];

/// Show the page while `open`; Escape or OK closes it.
pub fn show(
    ctx: &egui::Context,
    open: &mut bool,
    tab: &mut Tab,
    icon: Option<&TextureHandle>,
    weak: Color32,
    info: &SystemInfo,
) {
    let shown = crate::dialog::show(ctx, "about", "About MathCAS Notebook", 480.0, true, |ui| {
        ui.horizontal(|ui| {
            for (t, name) in [(Tab::About, "About"), (Tab::System, "System"), (Tab::Credits, "Credits")] {
                ui.selectable_value(tab, t, name);
            }
        });
        ui.separator();
        egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| match tab {
            Tab::About => about(ui, icon, weak),
            Tab::System => system(ui, info, weak),
            Tab::Credits => credits(ui, weak),
        });
        ui.separator();
        let mut done = false;
        crate::dialog::buttons(ui, |ui| {
            done = crate::dialog::primary(ui, "OK").clicked();
            if *tab == Tab::System
                && crate::dialog::button(ui, "Copy").on_hover_text("Copy these lines for a bug report").clicked()
            {
                ui.ctx().copy_text(info.text());
            }
        });
        done || ui.input(|i| i.key_pressed(Key::Escape))
    });
    if shown.inner || shown.closed {
        *open = false;
    }
}

fn about(ui: &mut egui::Ui, icon: Option<&TextureHandle>, weak: Color32) {
    ui.vertical_centered(|ui| {
        ui.add_space(8.0);
        if let Some(t) = icon {
            ui.add(egui::Image::new(t).fit_to_exact_size(vec2(80.0, 80.0)));
        }
        ui.add_space(6.0);
        ui.label(RichText::new("MathCAS Notebook").heading());
        ui.label(RichText::new(build_line()).color(weak));
        ui.add_space(10.0);
        ui.label(
            "Notebooks for MathCAS, a computer algebra system: exact and symbolic mathematics, arbitrary-precision \
             and compiled numerics, data and plots.",
        );
        ui.add_space(8.0);
        ui.label(RichText::new("Each notebook tab has its own kernel process; evaluation never blocks the window.").color(weak));
        ui.add_space(8.0);
        ui.label(RichText::new("By Rajat Mandal. Free software under the GNU GPL, version 3 or later. Components: see Credits.").color(weak).small());
        ui.add_space(6.0);
    });
}

fn system(ui: &mut egui::Ui, info: &SystemInfo, weak: Color32) {
    egui::Grid::new("about-system").num_columns(2).spacing(vec2(16.0, 6.0)).show(ui, |ui| {
        for (k, v) in &info.rows {
            ui.label(RichText::new(*k).color(weak));
            ui.add(egui::Label::new(v.as_str()).wrap());
            ui.end_row();
        }
    });
}

fn credits(ui: &mut egui::Ui, weak: Color32) {
    ui.label(RichText::new("MathCAS is built with these components; thanks to their authors.").color(weak));
    ui.add_space(6.0);
    for (name, what, licence) in CREDITS {
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(vec2(270.0, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                ui.set_width(270.0);
                ui.label(RichText::new(*name).strong());
                ui.add(egui::Label::new(RichText::new(*what).color(weak).small()).wrap());
            });
            ui.add(egui::Label::new(*licence).wrap());
        });
        ui.add_space(4.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_text_lists_every_row() {
        let info = SystemInfo::gather(None, 1.5, 1.0, "mc-kernel");
        let text = info.text();
        assert!(text.starts_with("MathCAS Notebook: Version "));
        assert!(text.contains("Display scale: 150 %"));
        assert!(text.contains("Kernel: mc-kernel"));
        assert_eq!(text.lines().count(), info.rows.len());
        assert_eq!(words("NotStarted"), "not started");
        assert_eq!(words("Idle"), "idle");
    }
}
