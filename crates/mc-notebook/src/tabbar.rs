//! The tab bar under the toolbar: one tab per notebook (title, modified dot, busy spinner, close button on hover and
//! on the active tab), middle-click closes, drag to reorder, a context menu, the full path as tooltip, scroll arrows
//! and a list of all tabs when they do not fit, and a "+" for a new notebook.
use crate::app::App;
use crate::fonts::Face;
use eframe::egui::{self, pos2, vec2, Color32, CursorIcon, Rect, Sense, Stroke};

/// The height of the tab strip (points).
pub const HEIGHT: f32 = 30.0;
/// A tab's width limits.
const MIN_W: f32 = 96.0;
const MAX_W: f32 = 220.0;
/// The width of the scroll and list buttons.
const BUTTON_W: f32 = 24.0;

/// What a click in the tab bar asks for (done after painting).
enum TabCmd {
    Activate(usize),
    Close(u64),
    CloseOthers(usize),
    CloseRight(usize),
    Move(usize, usize),
    New,
    CopyPath(usize),
    Reveal(usize),
}

impl App {
    /// The tab bar.
    pub(crate) fn tab_bar(&mut self, ui: &mut egui::Ui) {
        let p = self.doc.view.palette.clone();
        let full = ui.available_rect_before_wrap();
        let painter = ui.painter().clone();
        let font = Face::Ui.font(12.5);
        let time = ui.input(|i| i.time);
        let mut cmd: Option<TabCmd> = None;
        // widths
        let titles: Vec<String> = self.tabs.iter().map(|t| t.title()).collect();
        let widths: Vec<f32> = titles
            .iter()
            .map(|t| {
                let w = painter.layout_no_wrap(t.clone(), font.clone(), p.text).size().x;
                (w + 52.0).clamp(MIN_W, MAX_W)
            })
            .collect();
        let total: f32 = widths.iter().sum();
        let plus_w = 28.0;
        let overflow = total + plus_w > full.width();
        let strip_right = if overflow { full.right() - 4.0 * BUTTON_W - 8.0 } else { full.right() };
        let strip = Rect::from_min_max(full.min, pos2(strip_right, full.bottom()));
        let visible_w = strip.width() - if overflow { 0.0 } else { plus_w };
        let max_scroll = (total - visible_w).max(0.0);
        // keep the active tab in view
        if std::mem::take(&mut self.tab_scroll_to_active) {
            let a = self.tabs.active();
            let left: f32 = widths[..a.min(widths.len())].iter().sum();
            let right = left + widths.get(a).copied().unwrap_or(0.0);
            if left < self.tab_scroll {
                self.tab_scroll = left;
            } else if right > self.tab_scroll + visible_w {
                self.tab_scroll = right - visible_w;
            }
        }
        // the mouse wheel over the strip scrolls it
        let strip_resp = ui.interact(strip, egui::Id::unique("mc-tabbar").with("tab-strip"), Sense::hover());
        if strip_resp.hovered() && overflow {
            let d = ui.input(|i| i.smooth_scroll_delta);
            self.tab_scroll -= d.y + d.x;
        }
        self.tab_scroll = self.tab_scroll.clamp(0.0, max_scroll);
        let clip = painter.with_clip_rect(strip);
        // the line under the inactive tabs
        painter.hline(full.x_range(), full.bottom() - 0.5, Stroke::new(1.0, p.border_strong()));
        let mut x = strip.left() - self.tab_scroll;
        let active = self.tabs.active();
        let pointer = ui.input(|i| i.pointer.hover_pos());
        let mut rects = vec![];
        for w in &widths {
            let r = Rect::from_min_size(pos2(x, strip.top()), vec2(*w, strip.height()));
            rects.push(r);
            x += w;
        }
        for (i, r) in rects.iter().enumerate() {
            let Some(tab) = self.tabs.get(i) else {
                continue;
            };
            let (id, modified) = (tab.id, tab.modified);
            let path = tab.path.clone();
            let visible = r.intersect(strip);
            if visible.width() <= 0.0 {
                continue;
            }
            let resp = ui.interact(visible, egui::Id::unique("mc-tabbar").with(("tab", id)), Sense::click_and_drag());
            let close_r = Rect::from_center_size(pos2(r.right() - 16.0, r.center().y), vec2(18.0, 18.0));
            let close_resp = ui.interact(
                close_r.intersect(strip),
                egui::Id::unique("mc-tabbar").with(("tab-close", id)),
                Sense::click(),
            );
            let hovered = resp.hovered() || close_resp.hovered();
            let is_active = i == active;
            // the tab
            let fill = if is_active {
                p.bg
            } else if hovered {
                p.tab_hover()
            } else {
                p.panel
            };
            let body = Rect::from_min_max(pos2(r.left(), r.top() + 2.0), pos2(r.right() - 1.0, r.bottom()));
            clip.rect_filled(body, egui::CornerRadius { nw: 5, ne: 5, sw: 0, se: 0 }, fill);
            if is_active {
                clip.hline(body.left() + 3.0..=body.right() - 3.0, body.top() + 0.75, Stroke::new(1.5, p.accent));
            } else if i + 1 != active {
                // a separator between inactive tabs
                clip.vline(r.right() - 0.5, r.top() + 9.0..=r.bottom() - 7.0, Stroke::new(1.0, p.border_strong()));
            }
            // busy spinner or modified dot, then the title
            let mut tx = r.left() + 12.0;
            if self.tab_busy(i) {
                let c = pos2(tx + 5.0, r.center().y + 1.0);
                clip.circle_stroke(c, 5.0, Stroke::new(1.5, p.border_strong()));
                let a0 = (time * 6.0) as f32;
                let pts: Vec<egui::Pos2> = (0..=12)
                    .map(|k| {
                        let a = a0 + k as f32 * 0.13;
                        c + vec2(a.cos(), a.sin()) * 5.0
                    })
                    .collect();
                clip.add(egui::Shape::line(pts, Stroke::new(1.5, p.accent)));
                tx += 16.0;
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(33));
            }
            let color = if is_active { p.text } else { p.weak };
            let title_w = (r.right() - 30.0 - tx).max(10.0);
            let g = painter.layout(titles[i].clone(), font.clone(), color, f32::INFINITY);
            let g = if g.size().x > title_w {
                // elide the title with "..."
                let mut job = egui::text::LayoutJob::simple_singleline(titles[i].clone(), font.clone(), color);
                job.wrap = egui::text::TextWrapping::truncate_at_width(title_w);
                painter.layout_job(job)
            } else {
                g
            };
            clip.galley(pos2(tx, r.center().y - g.size().y / 2.0 + 1.0), g, color);
            // the close button: on the active tab and on hover; a modified tab shows a dot instead until hovered
            let close_hover = close_resp.hovered();
            if close_hover {
                clip.rect_filled(close_r.shrink(1.0), 4.0, p.tab_close_hover());
            }
            if is_active || hovered {
                if modified && !close_hover && !hovered {
                    clip.circle_filled(close_r.center(), 3.5, color);
                } else {
                    let c = close_r.center();
                    let s = 3.5;
                    let st = Stroke::new(1.3, if close_hover { p.text } else { color });
                    clip.line_segment([c + vec2(-s, -s), c + vec2(s, s)], st);
                    clip.line_segment([c + vec2(-s, s), c + vec2(s, -s)], st);
                }
            } else if modified {
                clip.circle_filled(close_r.center(), 3.5, color);
            }
            // interaction
            if close_resp.clicked() || resp.middle_clicked() {
                cmd = Some(TabCmd::Close(id));
            } else if resp.drag_started_by(egui::PointerButton::Primary) {
                cmd = Some(TabCmd::Activate(i));
                self.tab_drag = Some((i, pointer.map_or(0.0, |p| p.x - r.left())));
            } else if resp.clicked() {
                cmd = Some(TabCmd::Activate(i));
            }
            if close_hover || resp.hovered() && self.tab_drag.is_none() {
                ui.ctx().set_cursor_icon(CursorIcon::Default);
            }
            let tip = match &path {
                Some(p) => p.display().to_string(),
                None => format!("{} (not saved)", titles[i]),
            };
            let resp = if self.tab_drag.is_none() { resp.on_hover_text_at_pointer(tip) } else { resp };
            resp.context_menu(|ui| {
                if ui.button("Close").clicked() {
                    cmd = Some(TabCmd::Close(id));
                    ui.close();
                }
                if ui.add_enabled(self.tabs.len() > 1, egui::Button::new("Close others")).clicked() {
                    cmd = Some(TabCmd::CloseOthers(i));
                    ui.close();
                }
                if ui.add_enabled(i + 1 < self.tabs.len(), egui::Button::new("Close to the right")).clicked() {
                    cmd = Some(TabCmd::CloseRight(i));
                    ui.close();
                }
                ui.separator();
                if ui.add_enabled(path.is_some(), egui::Button::new("Copy path")).clicked() {
                    cmd = Some(TabCmd::CopyPath(i));
                    ui.close();
                }
                if ui.add_enabled(path.is_some(), egui::Button::new("Reveal in Explorer")).clicked() {
                    cmd = Some(TabCmd::Reveal(i));
                    ui.close();
                }
            });
        }
        // dragging a tab: it changes places as the pointer passes its neighbours' middles
        if let Some((from, _)) = self.tab_drag {
            let down = ui.input(|i| i.pointer.primary_down());
            if !down {
                self.tab_drag = None;
            } else if let Some(px) = pointer.map(|p| p.x) {
                ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
                let mut to = from;
                if from + 1 < rects.len() && px > rects[from + 1].center().x {
                    to = from + 1;
                } else if from > 0 && px < rects[from - 1].center().x {
                    to = from - 1;
                }
                if to != from {
                    cmd = Some(TabCmd::Move(from, to));
                }
            }
        }
        // the "+" after the last tab, or the buttons on the right when the tabs overflow
        let plus_rect = if overflow {
            Rect::from_min_size(pos2(full.right() - BUTTON_W, full.top() + 3.0), vec2(BUTTON_W, BUTTON_W))
        } else {
            Rect::from_min_size(pos2(x + 4.0, full.top() + 3.0), vec2(BUTTON_W, BUTTON_W))
        };
        if self
            .icon_button(ui, plus_rect, "tab-new", Glyph::Plus, true)
            .on_hover_text("New notebook (Ctrl+N)")
            .clicked()
        {
            cmd = Some(TabCmd::New);
        }
        if overflow {
            let list =
                Rect::from_min_size(pos2(full.right() - 2.0 * BUTTON_W, full.top() + 3.0), vec2(BUTTON_W, BUTTON_W));
            let left = list.translate(vec2(-2.0 * BUTTON_W - 2.0, 0.0));
            let right = list.translate(vec2(-BUTTON_W - 2.0, 0.0));
            if self.icon_button(ui, left, "tab-left", Glyph::Left, self.tab_scroll > 0.0).clicked() {
                self.tab_scroll = (self.tab_scroll - 160.0).max(0.0);
            }
            if self.icon_button(ui, right, "tab-right", Glyph::Right, self.tab_scroll < max_scroll).clicked() {
                self.tab_scroll = (self.tab_scroll + 160.0).min(max_scroll);
            }
            // the list of all tabs
            let resp = self.icon_button(ui, list, "tab-list", Glyph::Down, true).on_hover_text("All notebooks");
            egui::Popup::menu(&resp).show(|ui| {
                egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
                    for (k, t) in titles.iter().enumerate() {
                        let modified = self.tabs.get(k).is_some_and(|t| t.modified);
                        let label = if modified { format!("{t}  ●") } else { t.clone() };
                        if ui.selectable_label(k == active, label).clicked() {
                            cmd = Some(TabCmd::Activate(k));
                        }
                    }
                });
            });
        }
        if let Some(c) = cmd {
            self.tab_cmd(ui.ctx(), c);
        }
    }

    fn tab_cmd(&mut self, ctx: &egui::Context, c: TabCmd) {
        match c {
            TabCmd::Activate(i) => self.activate(i),
            TabCmd::Close(id) => self.close_tabs(&[id]),
            TabCmd::CloseOthers(i) => {
                let ids = self.tabs.others(i);
                self.close_tabs(&ids);
            }
            TabCmd::CloseRight(i) => {
                let ids = self.tabs.right_of(i);
                self.close_tabs(&ids);
            }
            TabCmd::Move(from, to) => {
                self.with_tabs(|t| t.move_tab(from, to));
                if let Some((_, off)) = self.tab_drag {
                    self.tab_drag = Some((to, off));
                }
            }
            TabCmd::New => self.new_tab(),
            TabCmd::CopyPath(i) => {
                if let Some(p) = self.tabs.get(i).and_then(|t| t.path.clone()) {
                    ctx.copy_text(p.display().to_string());
                }
            }
            TabCmd::Reveal(i) => {
                if let Some(p) = self.tabs.get(i).and_then(|t| t.path.clone()) {
                    reveal(&p);
                }
            }
        }
    }

    /// A small square button with a drawn glyph.
    pub(crate) fn icon_button(
        &self,
        ui: &mut egui::Ui,
        rect: Rect,
        id: &str,
        glyph: Glyph,
        enabled: bool,
    ) -> egui::Response {
        let p = &self.doc.view.palette;
        let sense = if enabled { Sense::click() } else { Sense::hover() };
        let resp = ui.interact(rect, egui::Id::unique("mc-tabbar").with(id), sense);
        let painter = ui.painter();
        if enabled && resp.is_pointer_button_down_on() {
            painter.rect_filled(rect.shrink(1.0), 4.0, p.tab_close_hover());
        } else if enabled && resp.hovered() {
            painter.rect_filled(rect.shrink(1.0), 4.0, p.tab_hover());
        }
        if enabled && resp.hovered() {
            ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
        }
        let color = if enabled { p.weak } else { p.faint };
        paint_glyph(painter, rect.center(), glyph, color);
        resp
    }
}

/// The glyphs of the small buttons.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Glyph {
    Plus,
    Left,
    Right,
    Down,
}

pub(crate) fn paint_glyph(painter: &egui::Painter, c: egui::Pos2, g: Glyph, color: Color32) {
    let st = Stroke::new(1.4, color);
    match g {
        Glyph::Plus => {
            painter.line_segment([c + vec2(-5.0, 0.0), c + vec2(5.0, 0.0)], st);
            painter.line_segment([c + vec2(0.0, -5.0), c + vec2(0.0, 5.0)], st);
        }
        Glyph::Left => {
            painter.line_segment([c + vec2(2.0, -4.0), c + vec2(-2.0, 0.0)], st);
            painter.line_segment([c + vec2(-2.0, 0.0), c + vec2(2.0, 4.0)], st);
        }
        Glyph::Right => {
            painter.line_segment([c + vec2(-2.0, -4.0), c + vec2(2.0, 0.0)], st);
            painter.line_segment([c + vec2(2.0, 0.0), c + vec2(-2.0, 4.0)], st);
        }
        Glyph::Down => {
            painter.line_segment([c + vec2(-4.0, -2.0), c + vec2(0.0, 2.0)], st);
            painter.line_segment([c + vec2(0.0, 2.0), c + vec2(4.0, -2.0)], st);
        }
    }
}

/// Show a file in the system's file manager.
fn reveal(p: &std::path::Path) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let _ = std::process::Command::new("explorer").raw_arg(format!("/select,\"{}\"", p.display())).spawn();
    }
    #[cfg(not(windows))]
    {
        if let Some(d) = p.parent() {
            let _ = std::process::Command::new("xdg-open").arg(d).spawn();
        }
    }
}
