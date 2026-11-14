//! Colours (design 8): the Atom One Light and One Dark palettes, their roles in the notebook, and the egui style of
//! the window chrome.
use eframe::egui::{self, Color32, CornerRadius, FontFamily, FontId, Shadow, Stroke, TextStyle, Visuals};

const fn hex(v: u32) -> Color32 {
    Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

/// A palette (design 8.1).
#[derive(Clone, Debug, PartialEq)]
pub struct Palette {
    /// dark or light
    pub dark: bool,
    /// the notebook background
    pub bg: Color32,
    /// toolbar, menu bar, status bar
    pub panel: Color32,
    /// popups and cards
    pub card: Color32,
    /// borders and rules
    pub border: Color32,
    /// text (mono-1)
    pub text: Color32,
    /// weak text (mono-2)
    pub weak: Color32,
    /// faint text (mono-3)
    pub faint: Color32,
    /// hue-1 cyan
    pub cyan: Color32,
    /// hue-2 blue
    pub blue: Color32,
    /// hue-3 purple
    pub purple: Color32,
    /// hue-4 green
    pub green: Color32,
    /// hue-5 red
    pub red: Color32,
    /// hue-5-2 dark red
    pub dark_red: Color32,
    /// hue-6 orange
    pub orange: Color32,
    /// hue-6-2 yellow
    pub yellow: Color32,
    /// accent
    pub accent: Color32,
    /// the caret
    pub cursor: Color32,
    /// text selection
    pub selection: Color32,
    /// the tint of the caret's cell
    pub current_line: Color32,
    /// labels and brackets
    pub gutter: Color32,
    /// the insertion line
    pub guide: Color32,
    /// prose text (text cells, items): softer than code
    pub prose: Color32,
    /// titles and section headings: an accent no syntax colour uses
    pub heading: Color32,
    /// the panel behind input cells
    pub input_bg: Color32,
}

impl Palette {
    /// The light scheme (after One Light, with every text colour at 4.5:1 or more against the page and the cells).
    pub fn light() -> Palette {
        Palette {
            dark: false,
            bg: hex(0xfafbfc),
            panel: hex(0xeceef1),
            card: hex(0xffffff),
            border: hex(0xd6dae0),
            text: hex(0x24292f),
            weak: hex(0x57606a),
            faint: hex(0x8c959f),
            cyan: hex(0x0b7285),
            blue: hex(0x1f5fd6),
            purple: hex(0x8b2fc9),
            green: hex(0x2b7a2e),
            red: hex(0xc62828),
            dark_red: hex(0xa3123a),
            orange: hex(0x9a5600),
            yellow: hex(0x806200),
            accent: hex(0x3b5bdb),
            cursor: hex(0x3b5bdb),
            selection: hex(0xd3e1fb),
            current_line: hex(0xf1f3f6),
            gutter: hex(0x8c959f),
            guide: hex(0xcfd4da),
            prose: hex(0x3b4351),
            heading: hex(0x3d3a8c),
            input_bg: hex(0xf0f2f7),
        }
    }

    /// The dark scheme: One Dark's text and syntax colours (brightened, each at 4.5:1 or more) on near-black
    /// surfaces (the page `#0e1013`). The one dark theme (user decision 2026-11-11: the lighter One Dark page went).
    pub fn dark() -> Palette {
        Palette {
            dark: true,
            bg: hex(0x0e1013),
            panel: hex(0x08090b),
            card: hex(0x1a1d22),
            border: hex(0x000000),
            text: hex(0xd0d5dd),
            weak: hex(0xa0a8b5),
            faint: hex(0x7d8594),
            cyan: hex(0x5fc6d3),
            blue: hex(0x74b9ff),
            purple: hex(0xd49afc),
            green: hex(0xa8d98c),
            red: hex(0xff8089),
            dark_red: hex(0xf47a6f),
            orange: hex(0xe8a96f),
            yellow: hex(0xf0c674),
            accent: hex(0x5c9dff),
            cursor: hex(0x7aafff),
            selection: hex(0x2c3448),
            current_line: hex(0x14171c),
            gutter: hex(0x6f788b),
            guide: hex(0x2a2e36),
            prose: hex(0xbcc3ce),
            heading: hex(0xb4b9ff),
            input_bg: hex(0x16191e),
        }
    }

    /// The phone layout's version of this palette: the window's surfaces, text, outlines and accent are the roles
    /// of the Material scheme ([`Material`]); the colours of code and mathematics stay.
    pub fn material(mut self) -> Palette {
        let m = Material::of(self.dark);
        self.bg = m.surface;
        self.panel = m.surface_container;
        self.card = m.surface_high;
        self.border = m.outline_variant;
        self.text = m.on_surface;
        self.weak = m.on_surface_variant;
        self.faint = m.outline;
        self.accent = m.primary;
        self.cursor = m.primary;
        self.red = m.error;
        self.gutter = m.outline;
        self.guide = m.outline_variant;
        self.input_bg = m.surface_low;
        self.current_line = m.surface_low;
        self.selection = m.primary_container;
        self
    }

    /// `c` at `alpha` over the background.
    pub fn over_bg(&self, c: Color32, alpha: f32) -> Color32 {
        lerp(self.bg, c, alpha)
    }

    /// The text selection colour (light: accent at 22 % for contrast).
    pub fn text_selection(&self) -> Color32 {
        if self.dark {
            self.selection
        } else {
            self.over_bg(self.accent, 0.22)
        }
    }

    /// Lines in the chrome that must show on the panel colour (One Dark's border is darker than its panel).
    pub fn border_strong(&self) -> Color32 {
        if self.dark {
            hex(0x23272e)
        } else {
            hex(0xd0d0d2)
        }
    }

    /// The insertion line between cells: the guide token in One Light; in One Dark the guide is too close to the
    /// background for a line one pixel or two thick, so faint text is used.
    pub fn guide_line(&self) -> Color32 {
        if self.dark {
            self.faint
        } else {
            self.guide
        }
    }

    /// Section and Program rules: the border token in One Light; One Dark's border is darker than its background,
    /// so a lighter grey keeps the rule visible.
    pub fn rule(&self) -> Color32 {
        if self.dark {
            hex(0x3a404b)
        } else {
            hex(0xd0d0d2)
        }
    }

    /// An inactive tab or a small button under the pointer.
    pub fn tab_hover(&self) -> Color32 {
        if self.dark {
            hex(0x1c2026)
        } else {
            hex(0xe0e0e1)
        }
    }

    /// A small button pressed, or the close button of a tab under the pointer.
    pub fn tab_close_hover(&self) -> Color32 {
        if self.dark {
            hex(0x262b33)
        } else {
            hex(0xd4d4d6)
        }
    }

    /// The labels and legends drawn around typeset values (`labeled`, `legended`; round 13): weak text, so that
    /// the value reads first and the label as its caption.
    pub fn label(&self) -> Color32 {
        self.weak
    }

    /// The colours of a picture's chrome (round nb-svg): no background of its own (the page shows through), text
    /// and primitives without a colour in the text colour, frame, axes and ticks in weak text, grid lines faint
    /// (12 % of the text over the page), the legend's box in the page colour. Data keeps its colours.
    pub fn plot_theme(&self) -> mc_graphics::theme::Theme {
        let c = |c: Color32| mc_graphics::color::Rgba::rgb(f64::from(c.r()) / 255.0, f64::from(c.g()) / 255.0, f64::from(c.b()) / 255.0);
        mc_graphics::theme::Theme {
            background: None,
            text: c(self.text),
            frame: c(self.weak),
            grid: c(lerp(self.bg, self.text, 0.12)),
            panel: c(self.bg),
        }
    }

    /// The Code cell background: panel at 50 % over the background.
    pub fn code_bg(&self) -> Color32 {
        lerp(self.bg, self.panel, 0.5)
    }

    /// The colours and spacing of egui's widgets for the chrome.
    pub fn apply(&self, ctx: &egui::Context) {
        let mut v = if self.dark { Visuals::dark() } else { Visuals::light() };
        v.panel_fill = self.panel;
        v.window_fill = self.card;
        v.extreme_bg_color = self.card;
        v.text_edit_bg_color = Some(self.card);
        v.faint_bg_color = self.panel;
        v.code_bg_color = self.code_bg();
        v.window_stroke = Stroke::new(1.0, if self.dark { hex(0x30353e) } else { self.border });
        v.override_text_color = Some(self.text);
        v.weak_text_color = Some(self.weak);
        v.warn_fg_color = self.orange;
        v.error_fg_color = self.red;
        v.selection.bg_fill = self.text_selection();
        v.selection.stroke = Stroke::new(1.0, self.accent);
        v.hyperlink_color = self.accent;
        v.text_cursor.stroke = Stroke::new(2.0, self.cursor);
        let shadow = if self.dark { Color32::from_black_alpha(90) } else { Color32::from_black_alpha(28) };
        v.window_shadow = Shadow { offset: [0, 6], blur: 18, spread: 0, color: shadow };
        v.popup_shadow = Shadow { offset: [0, 4], blur: 12, spread: 0, color: shadow };
        let hover = if self.dark { hex(0x262b33) } else { hex(0xdcdcdd) };
        let press = if self.dark { hex(0x343a45) } else { hex(0xc8c8ca) };
        let w = &mut v.widgets;
        for (st, fill, stroke) in [
            (&mut w.noninteractive, self.card, self.border),
            (&mut w.inactive, self.panel, self.border),
            (&mut w.hovered, hover, self.accent.gamma_multiply(0.6)),
            (&mut w.active, press, self.accent),
            (&mut w.open, hover, self.border),
        ] {
            st.corner_radius = CornerRadius::same(4);
            st.bg_fill = fill;
            st.weak_bg_fill = fill;
            st.bg_stroke = Stroke::new(1.0, stroke);
            st.fg_stroke.color = self.text;
        }
        // buttons in dialogs have a face and an outline (menus and the toolbar make theirs flat)
        let face = if self.dark { hex(0x20242b) } else { hex(0xffffff) };
        let outline = if self.dark { hex(0x3a404b) } else { hex(0xc8c8ca) };
        w.inactive.weak_bg_fill = face;
        w.inactive.bg_fill = face;
        w.inactive.bg_stroke = Stroke::new(1.0, outline);
        w.hovered.bg_stroke = Stroke::new(1.0, self.accent.gamma_multiply(0.7));
        // links, buttons and the "+" show the pointing hand
        v.interact_cursor = Some(egui::CursorIcon::PointingHand);
        w.noninteractive.bg_stroke = Stroke::new(1.0, if self.dark { hex(0x30353e) } else { self.border });
        v.window_corner_radius = CornerRadius::same(6);
        v.menu_corner_radius = CornerRadius::same(5);
        ctx.set_visuals(v);
        ctx.all_styles_mut(|s| {
            s.spacing.item_spacing = egui::vec2(8.0, 4.0);
            s.spacing.button_padding = egui::vec2(8.0, 3.0);
            // a slim scroll bar that widens under the pointer
            s.spacing.scroll = egui::style::ScrollStyle::thin();
            s.spacing.scroll.bar_width = 8.0;
            s.spacing.scroll.floating_allocated_width = 0.0;
            s.spacing.interact_size.y = 22.0;
            s.spacing.menu_margin = egui::Margin::symmetric(6, 5);
            s.text_styles.insert(TextStyle::Body, FontId::new(14.0, FontFamily::Proportional));
            s.text_styles.insert(TextStyle::Button, FontId::new(14.0, FontFamily::Proportional));
            s.text_styles.insert(TextStyle::Small, FontId::new(11.0, FontFamily::Proportional));
            s.text_styles.insert(TextStyle::Heading, FontId::new(18.0, FontFamily::Proportional));
            s.text_styles.insert(TextStyle::Monospace, FontId::new(13.0, FontFamily::Monospace));
            // the phone layout: targets a finger can take, text read at arm's length
            if crate::layout::mobile() {
                s.spacing.item_spacing = egui::vec2(10.0, 8.0);
                s.spacing.button_padding = egui::vec2(14.0, 9.0);
                s.spacing.interact_size.y = 44.0;
                s.text_styles.insert(TextStyle::Body, FontId::new(16.0, FontFamily::Proportional));
                s.text_styles.insert(TextStyle::Button, FontId::new(16.0, FontFamily::Proportional));
                s.text_styles.insert(TextStyle::Small, FontId::new(13.0, FontFamily::Proportional));
                s.text_styles.insert(TextStyle::Heading, FontId::new(20.0, FontFamily::Proportional));
                s.text_styles.insert(TextStyle::Monospace, FontId::new(15.0, FontFamily::Monospace));
                // Material shapes and states: full-round buttons, large corners on what floats, a tint for a
                // control that is pressed
                let m = Material::of(self.dark);
                let w = &mut s.visuals.widgets;
                for st in [&mut w.inactive, &mut w.hovered, &mut w.active, &mut w.open] {
                    st.corner_radius = CornerRadius::same(20);
                    st.bg_stroke = Stroke::new(1.0, m.outline);
                }
                w.inactive.weak_bg_fill = m.surface;
                w.inactive.bg_fill = m.surface;
                w.hovered.weak_bg_fill = Material::state(m.surface, m.on_surface, 0.08);
                w.hovered.bg_fill = w.hovered.weak_bg_fill;
                w.active.weak_bg_fill = Material::state(m.surface, m.on_surface, 0.12);
                w.active.bg_fill = w.active.weak_bg_fill;
                s.visuals.window_corner_radius = CornerRadius::same(28);
                s.visuals.window_fill = m.surface_high;
                s.visuals.window_stroke = Stroke::NONE;
                s.visuals.text_edit_bg_color = Some(m.surface_high);
                s.visuals.extreme_bg_color = m.surface_high;
            }
        });
    }
}

/// The colour roles of Material Design 3 the phone layout draws with: a scheme made from the notebook's blue, in
/// its light and its dark form. Each "on" colour is the text and icon colour for the surface it is named after.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Material {
    /// the main actions (filled buttons), links, the caret
    pub primary: Color32,
    /// text and icons on `primary`
    pub on_primary: Color32,
    /// standout surfaces: the floating action button, the leading circle of a list row
    pub primary_container: Color32,
    /// text and icons on `primary_container`
    pub on_primary_container: Color32,
    /// quieter filled controls (tonal buttons)
    pub secondary_container: Color32,
    /// text and icons on `secondary_container`
    pub on_secondary_container: Color32,
    /// the page
    pub surface: Color32,
    /// sheets, the cards of input cells
    pub surface_low: Color32,
    /// bars
    pub surface_container: Color32,
    /// dialogs
    pub surface_high: Color32,
    /// text and icons on the surfaces
    pub on_surface: Color32,
    /// secondary text, quiet icons
    pub on_surface_variant: Color32,
    /// outlines of controls
    pub outline: Color32,
    /// dividers, quiet outlines
    pub outline_variant: Color32,
    /// destructive actions, failures
    pub error: Color32,
    /// what dims the page behind a sheet or a dialog
    pub scrim: Color32,
}

impl Material {
    /// The scheme for a light (`dark`: a dark) screen.
    pub fn of(dark: bool) -> Material {
        if dark {
            Material {
                primary: hex(0xaac7ff),
                on_primary: hex(0x0a305f),
                primary_container: hex(0x284777),
                on_primary_container: hex(0xd6e3ff),
                secondary_container: hex(0x3e4759),
                on_secondary_container: hex(0xdae2f9),
                surface: hex(0x111318),
                surface_low: hex(0x191c20),
                surface_container: hex(0x1d2024),
                surface_high: hex(0x282a2f),
                on_surface: hex(0xe2e2e9),
                on_surface_variant: hex(0xc4c6d0),
                outline: hex(0x8e9099),
                outline_variant: hex(0x44474e),
                error: hex(0xffb4ab),
                scrim: Color32::from_black_alpha(150),
            }
        } else {
            Material {
                primary: hex(0x415f91),
                on_primary: hex(0xffffff),
                primary_container: hex(0xd6e3ff),
                on_primary_container: hex(0x284777),
                secondary_container: hex(0xdae2f9),
                on_secondary_container: hex(0x3e4759),
                surface: hex(0xf9f9ff),
                surface_low: hex(0xf3f3fa),
                surface_container: hex(0xededf4),
                surface_high: hex(0xe7e8ee),
                on_surface: hex(0x191c20),
                on_surface_variant: hex(0x44474e),
                outline: hex(0x74777f),
                outline_variant: hex(0xc4c6d0),
                error: hex(0xba1a1a),
                scrim: Color32::from_black_alpha(82),
            }
        }
    }

    /// `layer` at `alpha` over `base`: the tint of a control that is pressed or hovered (a state layer).
    pub fn state(base: Color32, layer: Color32, alpha: f32) -> Color32 {
        lerp(base, layer, alpha)
    }
}

/// `a` mixed with `b` (0: `a`, 1: `b`).
pub fn lerp(a: Color32, b: Color32, t: f32) -> Color32 {
    let m = |x: u8, y: u8| (f32::from(x) + (f32::from(y) - f32::from(x)) * t).round() as u8;
    Color32::from_rgb(m(a.r(), b.r()), m(a.g(), b.g()), m(a.b(), b.b()))
}

/// The theme the user chose (design 8.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemeChoice {
    /// follow the operating system, live
    System,
    /// One Light
    Light,
    /// the dark theme (One Dark's colours on near-black)
    Dark,
}

impl ThemeChoice {
    /// The name saved in the settings.
    pub fn name(self) -> &'static str {
        match self {
            ThemeChoice::System => "system",
            ThemeChoice::Light => "light",
            ThemeChoice::Dark => "dark",
        }
    }

    /// The choice saved as `name`.
    pub fn from_name(name: &str) -> ThemeChoice {
        match name {
            "light" => ThemeChoice::Light,
            // "deep" was a separate choice for a few builds: it is the dark theme now
            "dark" | "deep" => ThemeChoice::Dark,
            _ => ThemeChoice::System,
        }
    }

    /// The palette to use now (`system_dark`: what the operating system shows).
    pub fn palette(self, system_dark: bool) -> Palette {
        let palette = match self {
            ThemeChoice::System if system_dark => Palette::dark(),
            ThemeChoice::System | ThemeChoice::Light => Palette::light(),
            ThemeChoice::Dark => Palette::dark(),
        };
        if crate::layout::mobile() {
            palette.material()
        } else {
            palette
        }
    }
}

#[cfg(test)]
mod contrast {
    use super::*;

    fn lum(c: Color32) -> f64 {
        let f = |v: u8| {
            let v = v as f64 / 255.0;
            if v <= 0.03928 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
        };
        0.2126 * f(c.r()) + 0.7152 * f(c.g()) + 0.0722 * f(c.b())
    }

    fn ratio(a: Color32, b: Color32) -> f64 {
        let (x, y) = (lum(a), lum(b));
        (x.max(y) + 0.05) / (x.min(y) + 0.05)
    }

    /// Every colour text is drawn in reads at 4.5:1 or more on the page, the cells and the current line; the
    /// selection still shows the text at 4.5:1.
    #[test]
    fn text_colours_contrast_with_their_backgrounds() {
        for p in [Palette::light(), Palette::dark()] {
            for (name, c) in [("text", p.text), ("weak", p.weak), ("prose", p.prose), ("heading", p.heading), ("cyan", p.cyan), ("blue", p.blue),
                ("purple", p.purple), ("green", p.green), ("red", p.red), ("dark_red", p.dark_red),
                ("orange", p.orange), ("yellow", p.yellow), ("accent", p.accent)] {
                for (bname, b) in [("bg", p.bg), ("card", p.card), ("current_line", p.current_line), ("input_bg", p.input_bg)] {
                    let r = ratio(c, b);
                    assert!(r >= 4.5, "{} {name} on {bname}: {r:.2}", if p.dark { "dark" } else { "light" });
                }
            }
            assert!(ratio(p.text, p.selection) >= 4.5);
        }
    }

    /// A picture's chrome in both themes (round nb-svg): tick and axis labels at 4.5:1 or more on the page (and on
    /// the cell's current-line tint), the frame at 3:1 or more (non-text contrast), grid lines visible but faint,
    /// the legend's box the page itself.
    #[test]
    fn plot_chrome_contrasts_in_both_themes() {
        use mc_graphics::theme::Theme;
        let c = |v: mc_graphics::color::Rgba| {
            let [r, g, b, _] = v.bytes();
            Color32::from_rgb(r, g, b)
        };
        for p in [Palette::light(), Palette::dark()] {
            let t = p.plot_theme();
            assert_eq!(t.background, None);
            for b in [p.bg, p.current_line] {
                assert!(ratio(c(t.text), b) >= 4.5, "text {:.2}", ratio(c(t.text), b));
                assert!(ratio(c(t.frame), b) >= 3.0, "frame {:.2}", ratio(c(t.frame), b));
            }
            let g = ratio(c(t.grid), p.bg);
            assert!((1.1..2.0).contains(&g), "grid lines faint but there: {g:.2}");
            assert_eq!(c(t.panel), p.bg);
            // the same measure as the graphics crate's
            assert!((Theme::contrast(t.text, t.panel) - ratio(c(t.text), p.bg)).abs() < 0.01);
        }
    }
}
