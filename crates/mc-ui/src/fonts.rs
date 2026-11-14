//! Fonts (design 4.10) of the notebook and the documentation window: a code font for inputs, outputs and calls, a
//! text and interface font for prose, headings and labels, and fallbacks so every character has a glyph.
//!
//! The bundled set (files in `fonts/`, see `fonts/SOURCES.txt` and the licence files there) looks the same on every
//! system: DejaVu Sans Mono (regular and bold) for code, Noto Sans (regular, bold, italic) for text, a symbol subset
//! of DejaVu Sans, then STIX Two Math (from `mc-mathfont`) for the rarer mathematical symbols, then egui's emoji
//! fonts. On Windows the system's Consolas (code) and Segoe UI (text), with Segoe UI Symbol, come first when present,
//! as before; `MCUI_FONTS=bundled` uses the bundled set there too (to see what other systems show).
use eframe::egui::{self, FontData, FontDefinitions, FontFamily, FontId};
use std::sync::Arc;

/// The font faces of the windows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Face {
    /// code, regular
    Code,
    /// code, bold (Input and Code cells, the documentation's inputs and titles)
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
    /// Every face.
    pub const ALL: [Face; 6] = [Face::Code, Face::CodeBold, Face::Prose, Face::ProseBold, Face::ProseItalic, Face::Ui];

    /// The name of the egui font family of this face.
    pub fn family_name(self) -> &'static str {
        match self {
            Face::Code => "mc-code",
            Face::CodeBold => "mc-code-bold",
            Face::Prose => "mc-prose",
            Face::ProseBold => "mc-prose-bold",
            Face::ProseItalic => "mc-prose-italic",
            Face::Ui => "mc-ui",
        }
    }

    /// The egui font family of this face.
    pub fn family(self) -> FontFamily {
        FontFamily::Name(self.family_name().into())
    }

    /// The egui font of this face at `size` points.
    pub fn font(self, size: f32) -> FontId {
        FontId::new(size, self.family())
    }
}

/// A font file bundled with the program.
#[derive(Clone, Copy, Debug)]
pub struct BundledFont {
    /// the key of the font in egui's font definitions
    pub key: &'static str,
    /// the font's name
    pub name: &'static str,
    /// the file's bytes
    pub data: &'static [u8],
    /// the licence text
    pub licence: &'static str,
}

const DEJAVU_LICENCE: &str = include_str!("../fonts/LICENSE-DejaVu.txt");
const APACHE_LICENCE: &str = include_str!("../fonts/LICENSE-Apache-2.0.txt");

const CODE: BundledFont = BundledFont {
    key: "dejavu-sans-mono",
    name: "DejaVu Sans Mono",
    data: include_bytes!("../fonts/DejaVuSansMono.ttf"),
    licence: DEJAVU_LICENCE,
};
const CODE_BOLD: BundledFont = BundledFont {
    key: "dejavu-sans-mono-bold",
    name: "DejaVu Sans Mono Bold",
    data: include_bytes!("../fonts/DejaVuSansMono-Bold.ttf"),
    licence: DEJAVU_LICENCE,
};
const TEXT: BundledFont = BundledFont {
    key: "noto-sans",
    name: "Noto Sans",
    data: include_bytes!("../fonts/NotoSans-Regular.ttf"),
    licence: APACHE_LICENCE,
};
const TEXT_BOLD: BundledFont = BundledFont {
    key: "noto-sans-bold",
    name: "Noto Sans Bold",
    data: include_bytes!("../fonts/NotoSans-Bold.ttf"),
    licence: APACHE_LICENCE,
};
const TEXT_ITALIC: BundledFont = BundledFont {
    key: "noto-sans-italic",
    name: "Noto Sans Italic",
    data: include_bytes!("../fonts/NotoSans-Italic.ttf"),
    licence: APACHE_LICENCE,
};
const SYMBOLS: BundledFont = BundledFont {
    key: "dejavu-sans-symbols",
    name: "DejaVu Sans (symbols)",
    data: include_bytes!("../fonts/DejaVuSans-Symbols.ttf"),
    licence: DEJAVU_LICENCE,
};
const MATH: BundledFont =
    BundledFont { key: "stix-two-math", name: "STIX Two Math", data: mc_mathfont::FONT_DATA, licence: mc_mathfont::FONT_LICENSE };

/// The bundled fonts (with STIX Two Math, bundled by `mc-mathfont`), for an About box or a licence listing.
pub const BUNDLED: [BundledFont; 7] = [CODE, CODE_BOLD, TEXT, TEXT_BOLD, TEXT_ITALIC, SYMBOLS, MATH];

/// Where the fonts come from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    /// the system's fonts where this program knows good ones (Windows), the bundled ones elsewhere and as fallbacks
    PreferSystem,
    /// only the bundled fonts (and egui's emoji fonts)
    Bundled,
}

impl Source {
    /// From `MCUI_FONTS` (`bundled`; anything else or unset: [`Source::PreferSystem`]).
    pub fn from_env() -> Source {
        match std::env::var("MCUI_FONTS").as_deref().map(str::to_ascii_lowercase).as_deref() {
            Ok("bundled") => Source::Bundled,
            _ => Source::PreferSystem,
        }
    }
}

/// Install the fonts in `ctx` (from [`Source::from_env`]).
pub fn install(ctx: &egui::Context) {
    ctx.set_fonts(definitions(Source::from_env()));
}

/// System fonts of the faces, best first, each a list of `(key, file)` in the system's font folder.
struct SystemFonts {
    code: Vec<String>,
    code_bold: Vec<String>,
    prose: Vec<String>,
    prose_bold: Vec<String>,
    prose_italic: Vec<String>,
    symbols: Vec<String>,
}

/// Load the system fonts this program prefers into `defs` (only Windows has a set it knows).
fn system_fonts(defs: &mut FontDefinitions) -> SystemFonts {
    let mut load = |key: &str, file: &str| -> Option<String> {
        let dir = windows_font_dir()?;
        let bytes = std::fs::read(dir.join(file)).ok()?;
        defs.font_data.insert(key.to_string(), Arc::new(FontData::from_owned(bytes)));
        Some(key.to_string())
    };
    let consolas = load("consolas", "consola.ttf");
    let consolas_bold = load("consolas-bold", "consolab.ttf");
    let cascadia = load("cascadia", "CascadiaMono.ttf");
    let segoe = load("segoe", "segoeui.ttf");
    let segoe_semibold = load("segoe-semibold", "seguisb.ttf");
    let segoe_bold = load("segoe-bold", "segoeuib.ttf");
    let segoe_italic = load("segoe-italic", "segoeuii.ttf");
    let symbols = load("segoe-symbol", "seguisym.ttf");
    let list = |v: &[&Option<String>]| v.iter().filter_map(|k| (*k).clone()).collect::<Vec<_>>();
    SystemFonts {
        code: list(&[&consolas, &cascadia]),
        code_bold: list(&[&consolas_bold, &consolas, &cascadia]),
        prose: list(&[&segoe]),
        prose_bold: list(&[&segoe_semibold, &segoe_bold, &segoe]),
        prose_italic: list(&[&segoe_italic, &segoe]),
        symbols: list(&[&symbols]),
    }
}

/// Android's interface font, loaded from the system's font folder: the static Roboto where the system has it, else
/// the variable one at its regular weight. `None` on other systems, or when the file cannot be read.
fn android_ui_font(defs: &mut FontDefinitions) -> Option<String> {
    if !cfg!(target_os = "android") {
        return None;
    }
    let bytes = ["RobotoStatic-Regular.ttf", "Roboto-Regular.ttf"]
        .iter()
        .find_map(|f| std::fs::read(std::path::Path::new("/system/fonts").join(f)).ok())?;
    defs.font_data.insert("roboto".to_string(), Arc::new(FontData::from_owned(bytes)));
    Some("roboto".to_string())
}

/// The Windows font folder, on Windows.
fn windows_font_dir() -> Option<std::path::PathBuf> {
    if !cfg!(windows) {
        return None;
    }
    let windir = std::env::var_os("WINDIR").or_else(|| std::env::var_os("SystemRoot"))?;
    Some(std::path::Path::new(&windir).join("Fonts"))
}

/// The font definitions: each [`Face`]'s family, and egui's `Proportional` (the interface font) and `Monospace` (the
/// code font).
pub fn definitions(source: Source) -> FontDefinitions {
    let mut defs = FontDefinitions::default();
    // egui's emoji and icon fonts stay at the end of every chain; its text fonts (Ubuntu Light, Hack) are replaced
    let egui_tail: Vec<String> = defs
        .families
        .get(&FontFamily::Proportional)
        .into_iter()
        .flatten()
        .filter(|k| !matches!(k.as_str(), "Ubuntu-Light" | "Hack"))
        .cloned()
        .collect();
    for f in BUNDLED {
        defs.font_data.insert(f.key.to_string(), Arc::new(FontData::from_static(f.data)));
    }
    let system = match source {
        Source::PreferSystem => system_fonts(&mut defs),
        Source::Bundled => SystemFonts {
            code: vec![],
            code_bold: vec![],
            prose: vec![],
            prose_bold: vec![],
            prose_italic: vec![],
            symbols: vec![],
        },
    };
    // Android: the interface is set in the system's own font (Roboto), as the apps around it are
    let android_ui: Vec<String> = match source {
        Source::PreferSystem => android_ui_font(&mut defs).into_iter().collect(),
        Source::Bundled => vec![],
    };
    let chain = |first: &[String], bundled: &[BundledFont]| -> Vec<String> {
        let mut v: Vec<String> = first.to_vec();
        v.extend(bundled.iter().map(|f| f.key.to_string()));
        v.extend(system.symbols.iter().cloned());
        v.extend([SYMBOLS.key.to_string(), MATH.key.to_string()]);
        v.extend(egui_tail.iter().cloned());
        let mut seen = std::collections::HashSet::new();
        v.retain(|k| seen.insert(k.clone()));
        v
    };
    let code = chain(&system.code, &[CODE]);
    let lists = [
        (Face::Code, code.clone()),
        (Face::CodeBold, chain(&system.code_bold, &[CODE_BOLD, CODE])),
        (Face::Prose, chain(&system.prose, &[TEXT])),
        (Face::ProseBold, chain(&system.prose_bold, &[TEXT_BOLD, TEXT])),
        (Face::ProseItalic, chain(&system.prose_italic, &[TEXT_ITALIC, TEXT])),
        (Face::Ui, chain(if android_ui.is_empty() { &system.prose } else { &android_ui }, &[TEXT])),
    ];
    for (face, list) in lists {
        if face == Face::Ui {
            defs.families.insert(FontFamily::Proportional, list.clone());
        }
        defs.families.insert(face.family(), list);
    }
    defs.families.insert(FontFamily::Monospace, code);
    // the replaced egui text fonts are not loaded
    defs.font_data.retain(|k, _| defs.families.values().any(|l| l.contains(k)));
    defs
}

/// The extent of one glyph in ems (1 em = the font size), y up from the baseline.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GlyphBox {
    /// horizontal advance (as egui lays the character out)
    pub advance: f32,
    /// top of the ink above the baseline (0 for a glyph without ink)
    pub ascent: f32,
    /// bottom of the ink below the baseline (positive downwards)
    pub descent: f32,
}

/// The text fonts of one [`Face`] as egui uses them, without the math font and egui's emoji fonts at the end of
/// its chain: what typeset cells need to draw a character in the cell's own font (design 4.10, round nb-ui). A
/// character no font of the chain has is left to the math font.
#[derive(Clone)]
pub struct FaceFonts {
    fonts: Vec<Arc<FontData>>,
}

impl std::fmt::Debug for FaceFonts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "FaceFonts({} fonts)", self.fonts.len())
    }
}

impl FaceFonts {
    /// The text fonts of `face` in `defs` (as made by [`definitions`]): its family's chain up to the math font.
    pub fn new(defs: &FontDefinitions, face: Face) -> FaceFonts {
        let fonts = defs
            .families
            .get(&face.family())
            .into_iter()
            .flatten()
            .take_while(|k| k.as_str() != MATH.key)
            .filter_map(|k| defs.font_data.get(k).cloned())
            .collect();
        FaceFonts { fonts }
    }

    /// The extent of `c` in the first font of the chain that has it; `None` when none has it.
    pub fn glyph(&self, c: char) -> Option<GlyphBox> {
        for data in &self.fonts {
            let Ok(f) = ttf_parser::Face::parse(data.bytes(), data.index) else { continue };
            let Some(g) = f.glyph_index(c) else { continue };
            let em = f32::from(f.units_per_em().max(1));
            let scale = data.tweak.scale / em;
            let advance = f32::from(f.glyph_hor_advance(g).unwrap_or(0)) * scale;
            let (ascent, descent) = f
                .glyph_bounding_box(g)
                .map_or((0.0, 0.0), |b| (f32::from(b.y_max) * scale, -f32::from(b.y_min) * scale));
            return Some(GlyphBox { advance, ascent: ascent.max(0.0), descent: descent.max(0.0) });
        }
        None
    }

    /// The middle of the ink of the first font's minus sign (or `-`) above the baseline, in ems: the math axis of
    /// this font (fraction bars and centred delimiters line up with its operators).
    pub fn axis(&self) -> Option<f32> {
        ['\u{2212}', '-'].into_iter().find_map(|c| {
            let data = self.fonts.first()?;
            let f = ttf_parser::Face::parse(data.bytes(), data.index).ok()?;
            let g = f.glyph_index(c)?;
            let b = f.glyph_bounding_box(g)?;
            let em = f32::from(f.units_per_em().max(1));
            Some((f32::from(b.y_max) + f32::from(b.y_min)) / 2.0 / em * data.tweak.scale)
        })
    }
}

/// The text fonts of every face (see [`FaceFonts`]).
#[derive(Clone, Debug)]
pub struct FontSet {
    faces: Vec<(Face, Arc<FaceFonts>)>,
}

impl FontSet {
    /// The faces of `defs` (as made by [`definitions`]).
    pub fn new(defs: &FontDefinitions) -> FontSet {
        FontSet { faces: Face::ALL.into_iter().map(|f| (f, Arc::new(FaceFonts::new(defs, f)))).collect() }
    }

    /// The text fonts of `face`.
    pub fn face(&self, face: Face) -> Arc<FaceFonts> {
        self.faces.iter().find(|(f, _)| *f == face).map_or_else(|| Arc::new(FaceFonts { fonts: vec![] }), |(_, v)| v.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The face of `key` in `defs`.
    fn face<'a>(defs: &'a FontDefinitions, key: &str) -> ttf_parser::Face<'a> {
        let data = defs.font_data.get(key).unwrap_or_else(|| panic!("{key} is not loaded"));
        ttf_parser::Face::parse(data.bytes(), data.index).unwrap_or_else(|e| panic!("{key}: {e}"))
    }

    /// The first font of `family` with a glyph for `c`.
    fn covering(defs: &FontDefinitions, family: &FontFamily, c: char) -> Option<String> {
        defs.families[family].iter().find(|k| face(defs, k).glyph_index(c).is_some()).cloned()
    }

    #[test]
    fn bundled_fonts_parse_and_carry_licences() {
        for f in BUNDLED {
            let face = ttf_parser::Face::parse(f.data, 0).unwrap_or_else(|e| panic!("{}: {e}", f.name));
            assert!(face.number_of_glyphs() > 100, "{}", f.name);
            assert!(f.licence.len() > 1000, "{}", f.name);
        }
        assert!(DEJAVU_LICENCE.contains("Bitstream Vera"));
        assert!(APACHE_LICENCE.contains("Apache License"));
        assert!(MATH.licence.contains("SIL OPEN FONT LICENSE"));
        // the bundled set stays small (STIX Two Math is counted by mc-mathfont)
        let total: usize = BUNDLED[..6].iter().map(|f| f.data.len()).sum();
        assert!(total < 700_000, "{total} bytes");
    }

    #[test]
    fn every_face_has_a_chain_of_loaded_fonts() {
        for source in [Source::Bundled, Source::PreferSystem] {
            let defs = definitions(source);
            for f in Face::ALL {
                let list = &defs.families[&f.family()];
                assert!(!list.is_empty());
                for k in list {
                    assert!(defs.font_data.contains_key(k), "{k} of {f:?} is not loaded");
                }
            }
            assert_eq!(defs.families[&FontFamily::Monospace], defs.families[&Face::Code.family()]);
            assert_eq!(defs.families[&FontFamily::Proportional], defs.families[&Face::Ui.family()]);
            assert!(!defs.font_data.contains_key("Ubuntu-Light"));
        }
    }

    #[test]
    fn bundled_chains_cover_the_characters_of_mathematics() {
        let defs = definitions(Source::Bundled);
        let text = "AZaz09 ~!@#$%^&*()_+-=[]{}|;:'\",.<>/?\\ éßøçñ ΑΩαωπθλ∂ ∫∬∮∑∏√∛∞≈≠≤≥±×÷·∘ ∈∉⊂⊆∪∩∧∨¬∀∃∅ →←↔⇒⇔↦ ℝℂℕℤℚ \
                    ′″‰€ ⟨⟩⌈⌉⌊⌋ ⋯⋮⋱ ∇△□ 𝔸𝕏𝒜";
        for f in Face::ALL {
            for c in text.chars() {
                assert!(covering(&defs, &f.family(), c).is_some(), "{f:?} has no glyph for {c:?} (U+{:04X})", c as u32);
            }
        }
        // the primary fonts draw the common characters themselves
        assert_eq!(covering(&defs, &Face::Code.family(), 'a').as_deref(), Some(CODE.key));
        assert_eq!(covering(&defs, &Face::Code.family(), 'α').as_deref(), Some(CODE.key));
        assert_eq!(covering(&defs, &Face::CodeBold.family(), '→').as_deref(), Some(CODE_BOLD.key));
        assert_eq!(covering(&defs, &Face::Prose.family(), 'é').as_deref(), Some(TEXT.key));
        assert_eq!(covering(&defs, &Face::ProseItalic.family(), 'q').as_deref(), Some(TEXT_ITALIC.key));
        assert_eq!(covering(&defs, &Face::Prose.family(), '∮').as_deref(), Some(SYMBOLS.key));
        assert_eq!(covering(&defs, &Face::Prose.family(), '𝕏').as_deref(), Some(MATH.key));
    }

    /// egui's own fonts (Ubuntu Light, Hack, its emoji and icon fonts) are not built in: the symbols the windows
    /// draw as text (toolbar, menus, tabs, status bar, the documentation window) come from the bundled fonts.
    #[test]
    fn the_windows_symbols_need_no_egui_fonts() {
        let defs = definitions(Source::Bundled);
        assert!(defs.font_data.keys().all(|k| !matches!(k.as_str(), "Ubuntu-Light" | "Hack" | "NotoEmoji-Regular" | "emoji-icon-font")));
        for f in Face::ALL {
            let missing: String = "•→∞≈⋮⌂■□▶◀●◐☀☾✓▾▸⏵".chars().filter(|c| covering(&defs, &f.family(), *c).is_none()).collect();
            assert!(missing.is_empty(), "{f:?} has no glyph for {missing}");
        }
    }

    #[test]
    fn face_fonts_measure_the_text_font_and_leave_the_rest_to_the_math_font() {
        let set = FontSet::new(&definitions(Source::Bundled));
        let code = set.face(Face::Code);
        let x = code.glyph('x').expect("the code font has x");
        let m = code.glyph('M').expect("and M");
        // monospaced, ink of a lower-case letter below the capital's
        assert!((x.advance - m.advance).abs() < 1e-4 && x.advance > 0.5 && x.advance < 0.7, "{x:?} {m:?}");
        assert!(x.ascent < m.ascent && x.descent < 0.05, "{x:?} {m:?}");
        assert!(code.glyph('y').expect("y").descent > 0.1);
        // the symbols the chain has before the math font are text; a double-struck capital only the math font has
        assert!(code.glyph('∑').is_some());
        assert!(code.glyph('𝕏').is_none());
        let axis = code.axis().expect("a minus sign");
        assert!(axis > 0.2 && axis < 0.45, "{axis}");
    }

    #[test]
    fn the_code_font_is_monospaced_in_egui() {
        let ctx = egui::Context::default();
        ctx.set_fonts(definitions(Source::Bundled));
        let mut widths = vec![];
        let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
            for s in ["iiiiii", "WWWWWW", "αβγδεζ", "x+y=zz"] {
                let g = ui.painter().layout_no_wrap(s.into(), Face::Code.font(13.0), egui::Color32::WHITE);
                widths.push(g.size().x);
            }
            let bold = ui.painter().layout_no_wrap("mmmmmm".into(), Face::CodeBold.font(13.0), egui::Color32::WHITE);
            widths.push(bold.size().x);
        });
        assert!(widths[0] > 30.0, "{widths:?}");
        for w in &widths {
            assert!((w - widths[0]).abs() < 0.5, "{widths:?}");
        }
    }
}
