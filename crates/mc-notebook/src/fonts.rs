//! Fonts (design 4.10): the code font (Consolas, then Cascadia Mono, then egui's Hack) for inputs and outputs, the
//! prose and interface font (Segoe UI, then egui's Ubuntu), each with symbol fallbacks so every character has a
//! glyph. System fonts are read from the Windows font folder when present.
use eframe::egui::{self, FontData, FontDefinitions, FontFamily, FontId};
use std::sync::Arc;

/// The font families of the notebook.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Face {
    /// code, regular
    Code,
    /// code, bold (Input and Code cells)
    CodeBold,
    /// prose, regular
    Prose,
    /// prose, bold (headings)
    ProseBold,
    /// prose, italic
    ProseItalic,
    /// the interface font (labels, messages)
    Ui,
}

impl Face {
    fn family_name(self) -> &'static str {
        match self {
            Face::Code => "nb-code",
            Face::CodeBold => "nb-code-bold",
            Face::Prose => "nb-prose",
            Face::ProseBold => "nb-prose-bold",
            Face::ProseItalic => "nb-prose-italic",
            Face::Ui => "nb-ui",
        }
    }

    /// The egui font of this face at `size` points.
    pub fn font(self, size: f32) -> FontId {
        FontId::new(size, FontFamily::Name(self.family_name().into()))
    }
}

fn font_dir() -> std::path::PathBuf {
    let windir = std::env::var_os("WINDIR").unwrap_or_else(|| "C:\\Windows".into());
    std::path::Path::new(&windir).join("Fonts")
}

/// Install the notebook's fonts.
pub fn install(ctx: &egui::Context) {
    let mut defs = FontDefinitions::default();
    let dir = font_dir();
    let mut load = |name: &str, file: &str| -> Option<String> {
        let bytes = std::fs::read(dir.join(file)).ok()?;
        defs.font_data.insert(name.to_string(), Arc::new(FontData::from_owned(bytes)));
        Some(name.to_string())
    };
    let consolas = load("consolas", "consola.ttf");
    let consolas_bold = load("consolas-bold", "consolab.ttf");
    let cascadia = load("cascadia", "CascadiaMono.ttf");
    let segoe = load("segoe", "segoeui.ttf");
    let segoe_semibold = load("segoe-semibold", "seguisb.ttf");
    let segoe_bold = load("segoe-bold", "segoeuib.ttf");
    let segoe_italic = load("segoe-italic", "segoeuii.ttf");
    let symbols = load("segoe-symbol", "seguisym.ttf");
    let egui_mono = defs.families.get(&FontFamily::Monospace).cloned().unwrap_or_default();
    let egui_prop = defs.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let fallbacks: Vec<String> = symbols.iter().cloned().collect();
    let chain = |first: &[Option<String>], egui_list: &[String]| -> Vec<String> {
        let mut v: Vec<String> = first.iter().flatten().cloned().collect();
        v.extend(egui_list.iter().cloned());
        v.extend(fallbacks.iter().cloned());
        v.dedup();
        v
    };
    let code = chain(&[consolas.clone(), cascadia.clone()], &egui_mono);
    let code_bold = chain(&[consolas_bold, consolas, cascadia], &egui_mono);
    let prose = chain(std::slice::from_ref(&segoe), &egui_prop);
    let prose_bold = chain(&[segoe_semibold, segoe_bold, segoe.clone()], &egui_prop);
    let prose_italic = chain(&[segoe_italic, segoe.clone()], &egui_prop);
    let ui = chain(&[segoe], &egui_prop);
    for (face, list) in [
        (Face::Code, code.clone()),
        (Face::CodeBold, code_bold),
        (Face::Prose, prose),
        (Face::ProseBold, prose_bold),
        (Face::ProseItalic, prose_italic),
        (Face::Ui, ui.clone()),
    ] {
        defs.families.insert(FontFamily::Name(face.family_name().into()), list);
    }
    defs.families.insert(FontFamily::Proportional, ui);
    defs.families.insert(FontFamily::Monospace, code);
    ctx.set_fonts(defs);
}
