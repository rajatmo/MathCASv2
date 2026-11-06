//! Fonts, as the notebook's: the code font (Consolas, then Cascadia Mono, then egui's Hack) for calls, inputs and
//! outputs; the prose font (Segoe UI, then egui's Ubuntu) for text and headings; a symbol font as the fallback of
//! both. System fonts are read from the Windows font folder when present. (The notebook's font module is private
//! to it, so this is a copy of the part the documentation needs.)
use eframe::egui::{self, FontData, FontDefinitions, FontFamily, FontId};
use std::sync::Arc;

/// The font families of the documentation window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Face {
    /// code, regular
    Code,
    /// code, bold (inputs and the title)
    CodeBold,
    /// prose, regular
    Prose,
    /// prose, semibold (headings)
    ProseBold,
}

impl Face {
    fn family_name(self) -> &'static str {
        match self {
            Face::Code => "doc-code",
            Face::CodeBold => "doc-code-bold",
            Face::Prose => "doc-prose",
            Face::ProseBold => "doc-prose-bold",
        }
    }

    /// The egui font of this face at `size` points.
    pub fn font(self, size: f32) -> FontId {
        FontId::new(size, FontFamily::Name(self.family_name().into()))
    }
}

/// Install the fonts.
pub fn install(ctx: &egui::Context) {
    let mut defs = FontDefinitions::default();
    let windir = std::env::var_os("WINDIR").unwrap_or_else(|| "C:\\Windows".into());
    let dir = std::path::Path::new(&windir).join("Fonts");
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
    let symbols = load("segoe-symbol", "seguisym.ttf");
    let egui_mono = defs.families.get(&FontFamily::Monospace).cloned().unwrap_or_default();
    let egui_prop = defs.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let chain = |first: &[Option<String>], egui_list: &[String]| -> Vec<String> {
        let mut v: Vec<String> = first.iter().flatten().cloned().collect();
        v.extend(egui_list.iter().cloned());
        v.extend(symbols.iter().cloned());
        v.dedup();
        v
    };
    let code = chain(&[consolas.clone(), cascadia.clone()], &egui_mono);
    let code_bold = chain(&[consolas_bold, consolas, cascadia], &egui_mono);
    let prose = chain(std::slice::from_ref(&segoe), &egui_prop);
    let prose_bold = chain(&[segoe_semibold, segoe], &egui_prop);
    for (face, list) in [
        (Face::Code, code.clone()),
        (Face::CodeBold, code_bold),
        (Face::Prose, prose.clone()),
        (Face::ProseBold, prose_bold),
    ] {
        defs.families.insert(FontFamily::Name(face.family_name().into()), list);
    }
    defs.families.insert(FontFamily::Proportional, prose);
    defs.families.insert(FontFamily::Monospace, code);
    ctx.set_fonts(defs);
}
