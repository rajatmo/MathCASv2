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

    /// The dark scheme (after One Dark, with a deeper page and brighter text colours, each at 4.5:1 or more).
    pub fn dark() -> Palette {
        Palette {
            dark: true,
            bg: hex(0x22262d),
            panel: hex(0x1c1f25),
            card: hex(0x292e37),
            border: hex(0x14161a),
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
            selection: hex(0x3b4660),
            current_line: hex(0x2a2f39),
            gutter: hex(0x6f788b),
            guide: hex(0x3a404b),
            prose: hex(0xbcc3ce),
            heading: hex(0xb4b9ff),
            input_bg: hex(0x2a303a),
        }
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
            hex(0x181a1f)
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
            hex(0x4b5263)
        } else {
            hex(0xd0d0d2)
        }
    }

    /// An inactive tab or a small button under the pointer.
    pub fn tab_hover(&self) -> Color32 {
        if self.dark {
            hex(0x2c313a)
        } else {
            hex(0xe0e0e1)
        }
    }

    /// A small button pressed, or the close button of a tab under the pointer.
    pub fn tab_close_hover(&self) -> Color32 {
        if self.dark {
            hex(0x3a3f4b)
        } else {
            hex(0xd4d4d6)
        }
    }

    /// The labels and legends drawn around typeset values (`labeled`, `legended`; round 13): weak text, so that
    /// the value reads first and the label as its caption.
    pub fn label(&self) -> Color32 {
        self.weak
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
        v.window_stroke = Stroke::new(1.0, if self.dark { hex(0x3e4451) } else { self.border });
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
        let hover = if self.dark { hex(0x3a3f4b) } else { hex(0xdcdcdd) };
        let press = if self.dark { hex(0x4b5263) } else { hex(0xc8c8ca) };
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
        let face = if self.dark { hex(0x333842) } else { hex(0xffffff) };
        let outline = if self.dark { hex(0x4b5263) } else { hex(0xc8c8ca) };
        w.inactive.weak_bg_fill = face;
        w.inactive.bg_fill = face;
        w.inactive.bg_stroke = Stroke::new(1.0, outline);
        w.hovered.bg_stroke = Stroke::new(1.0, self.accent.gamma_multiply(0.7));
        // links, buttons and the "+" show the pointing hand
        v.interact_cursor = Some(egui::CursorIcon::PointingHand);
        w.noninteractive.bg_stroke = Stroke::new(1.0, if self.dark { hex(0x3e4451) } else { self.border });
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
        });
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
    /// One Dark
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
            "dark" => ThemeChoice::Dark,
            _ => ThemeChoice::System,
        }
    }

    /// The palette to use now (`system_dark`: what the operating system shows).
    pub fn palette(self, system_dark: bool) -> Palette {
        let dark = match self {
            ThemeChoice::System => system_dark,
            ThemeChoice::Light => false,
            ThemeChoice::Dark => true,
        };
        if dark {
            Palette::dark()
        } else {
            Palette::light()
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
}
