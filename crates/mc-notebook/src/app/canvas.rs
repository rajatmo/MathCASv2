//! The canvas: scrolling, painting, the pointer, mouse clicks and drags, the input method, the context menu.
use super::*;

impl App {
    pub(super) fn canvas(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let mut area = ScrollArea::vertical().auto_shrink([false, false]).id_salt("notebook");
        // while the magnification changes, the point under the pointer stays: the cells near it are measured first
        // and the offset is the one that puts the anchor where it was (a scroll, click or key lets go of it)
        let time = ctx.input(|i| i.time);
        let user = ctx.input(|i| {
            i.smooth_scroll_delta != egui::Vec2::ZERO || i.pointer.any_pressed() || !i.keys_down.is_empty()
        });
        if self.doc.zoom_anchor.is_some_and(|a| time > a.until || user || a.cell >= self.doc.nb.cells().len()) {
            self.doc.zoom_anchor = None;
        }
        self.doc.view.hold = false;
        if let Some(a) = self.doc.zoom_anchor {
            let h = self.canvas_clip.height().max(1.0);
            let mut off = self.doc.offset;
            for _ in 0..4 {
                self.doc.view.update_positions(&ctx, &self.doc.nb, off..off + h);
                let r = self.doc.view.cell_rect(a.cell);
                let want = (r.top() + a.frac * r.height() - a.screen).max(0.0);
                let done = (want - off).abs() < 0.5;
                off = want;
                if done {
                    break;
                }
            }
            area = area.vertical_scroll_offset(off);
            // the canvas's own pass measures what shows but refines nothing else, so the anchor does not move
            self.doc.view.hold = true;
            ctx.request_repaint();
        } else if let Some(off) = self.doc.restore_offset.take() {
            area = area.vertical_scroll_offset(off.max(0.0));
        } else if self.doc.scroll_by != 0.0 {
            area = area.vertical_scroll_offset((self.doc.offset + self.doc.scroll_by).max(0.0));
            self.doc.scroll_by = 0.0;
            // the caret shows at once after a scroll
            self.doc.view.blink_from = ctx.input(|i| i.time);
        }
        let out = area.show_viewport(ui, |ui, viewport| {
            let width = ui.available_width();
            if (self.doc.view.width - width).abs() > 0.5 {
                self.doc.view.width = width;
            }
            self.doc.view.update_positions(&ctx, &self.doc.nb, viewport.min.y..viewport.max.y);
            let total = self.doc.view.total_height() + (viewport.height() * 0.4).max(80.0);
            let (rect, resp) = ui.allocate_exact_size(vec2(width, total), Sense::click_and_drag());
            let origin = rect.min;
            self.mouse(&ctx, &resp, origin);
            self.auto_scroll(&ctx, ui.clip_rect());
            // a bracket under the pointer names its cell's style (design 2.5)
            let tip = match self.doc.view.hover {
                Some(Bracket::Cell(id)) if self.drag.is_none() => self.doc.nb.cell(id).map(|c| {
                    let mut t = c.style.info().name.to_string();
                    // no In/Out labels (user decision): an output names the history entry that gives it back
                    if let Some(n) = c.label.filter(|_| c.style == Style::Output) {
                        t.push_str(&format!("  out({n})"));
                    }
                    t
                }),
                Some(Bracket::Group(..)) if self.drag.is_none() => {
                    Some("Group (double-click to open or close)".to_string())
                }
                _ => None,
            };
            let resp = match tip {
                Some(t) => resp.on_hover_text_at_pointer(t),
                None => self.hover_tooltip(&ctx, resp),
            };
            if resp.secondary_clicked() {
                self.help.menu_warning = self.help.warning.clone();
            }
            resp.context_menu(|ui| self.context_menu(ui));
            let time = ctx.input(|i| i.time);
            let clip = ui.clip_rect();
            self.canvas_clip = clip;
            self.doc.view.caret = match self.doc.nb.selection() {
                Selection::Caret { cell, anchor, pos } if anchor == pos => Some((*cell, *pos)),
                _ => None,
            };
            self.doc.view.paint(&ctx, ui.painter(), &self.doc.nb, origin, clip, time);
            // a long cell coloured by moving its colouring over an edit: a frame when typing has paused, which
            // colours it again
            if let Some(due) = self.doc.view.recolour_due.take() {
                let wait = due.saturating_duration_since(std::time::Instant::now());
                ctx.request_repaint_after(wait + std::time::Duration::from_millis(5));
            }
            self.doc.view.accessibility(&ctx, &self.doc.nb, origin, clip);
            if resp.hovered() && self.drag.is_none() {
                self.plot_hover(&ctx, ui.painter(), origin);
            }
            self.ime(&ctx, ui.painter(), rect);
            // the selected picture, when it turns: the wheel over it zooms it (taken from the input next frame)
            self.wheel3d = self.doc.view.graphic.and_then(|id| {
                let i = self.doc.nb.index_of(id)?;
                let path = self.doc.view.graphic_path(&self.doc.nb, i)?;
                let r = self.doc.view.graphic_rect(&self.doc.nb, i)?;
                self.rot.borrow_mut().turns(&path).then(|| (id, r.translate(origin.to_vec2()).intersect(clip)))
            });
            if std::mem::take(&mut self.doc.scroll_pending) {
                if let Some(r) = self.selection_rect(&ctx) {
                    ui.scroll_to_rect(r.translate(origin.to_vec2()).expand(8.0), None);
                }
            }
        });
        self.doc.offset = out.state.clamped_offset().y;
    }

    /// Coordinates under the pointer on a plot picture that has a description (NB5.2).
    pub(super) fn plot_hover(&mut self, ctx: &egui::Context, painter: &egui::Painter, origin: egui::Pos2) {
        let Some(screen) = ctx.input(|i| i.pointer.hover_pos()) else { return };
        let p = (screen - origin).to_pos2();
        let id = match self.doc.view.hit(ctx, &self.doc.nb, p) {
            Hit::Graphic(id) => id,
            _ => return,
        };
        let Some(i) = self.doc.nb.index_of(id) else { return };
        let Some(path) = self.doc.view.graphic_path(&self.doc.nb, i) else { return };
        // a 3D picture turns under the pointer: its plane coordinates mean nothing
        if self.rot.borrow_mut().turns(&path) {
            return;
        }
        let Some(rect) = self.doc.view.graphic_rect(&self.doc.nb, i) else { return };
        let Some(info) = self.plots.get(&path, ctx.input(|i| i.time)) else { return };
        let rect = rect.translate(origin.to_vec2());
        crate::plothover::paint(ctx, painter, &info, rect, screen, &self.doc.view.palette);
    }

    /// `x` (canvas coordinates) is over the content of cell `id`, not in its margins or label.
    pub(super) fn over_content(&self, id: mc_nbmodel::CellId, x: f32) -> bool {
        let Some(c) = self.doc.nb.cell(id) else {
            return false;
        };
        let (l, r) = self.doc.view.content_x(c.style);
        x >= l - 4.0 && x <= r + 4.0
    }

    /// Tell the input method where the caret is, and draw the text being composed there (underlined).
    pub(super) fn ime(&mut self, ctx: &egui::Context, painter: &egui::Painter, canvas: egui::Rect) {
        let Selection::Caret { cell, pos, .. } = *self.doc.nb.selection() else {
            self.preedit.clear();
            return;
        };
        if self.modal_open() {
            return;
        }
        let Some(r) = self.doc.view.caret_rect(ctx, &self.doc.nb, cell, pos) else {
            return;
        };
        let r = r.translate(self.doc.view.origin.to_vec2());
        ctx.output_mut(|o| {
            o.ime = Some(egui::output::IMEOutput {
                purpose: egui::IMEPurpose::Normal,
                rect: canvas,
                cursor_rect: egui::Rect::from_min_size(r.min, vec2(1.0, r.height())),
                should_interrupt_composition: false,
            })
        });
        if self.preedit.is_empty() {
            return;
        }
        let font = self.doc.nb.cell(cell).map_or(fonts::Face::Code.font(13.0), |c| {
            let lk = crate::view::look(c, &self.doc.view.palette);
            lk.face.font(lk.size * self.doc.view.zoom)
        });
        let p = &self.doc.view.palette;
        let g = painter.layout_no_wrap(self.preedit.clone(), font, p.text);
        let at = egui::pos2(r.left(), r.top());
        let bg = egui::Rect::from_min_size(at, vec2(g.size().x, r.height().max(g.size().y))).expand2(vec2(2.0, 0.0));
        painter.rect_filled(bg, 2.0, p.card);
        painter.galley(at, g.clone(), p.text);
        painter.hline(at.x..=at.x + g.size().x, bg.bottom() - 1.0, egui::Stroke::new(1.0, p.accent));
        painter.vline(at.x + g.size().x + 1.0, r.top()..=r.bottom(), egui::Stroke::new(2.0, p.cursor));
    }

    /// The insertion point's "+" menu (design 2.7): Input, Text, Other style.
    pub(super) fn plus_popup(&mut self, ctx: &egui::Context) {
        let Some(at) = self.plus_menu else { return };
        let mut fire = None;
        let mut close = false;
        let area = egui::Area::new(egui::Id::unique("mc-notebook-plus"))
            .fixed_pos(at + vec2(4.0, 8.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                Frame::menu(ui.style()).show(ui, |ui| {
                    ui.set_min_width(170.0);
                    for (label, cmd, key) in [
                        ("Input", Command::NewCell(Style::Input), "Alt+9"),
                        ("Text", Command::NewCell(Style::Text), "Alt+7"),
                        ("Other style...", Command::App(AppAction::OtherStyle), "Alt+0"),
                    ] {
                        if ui.add(egui::Button::new(label).shortcut_text(key).frame(false)).clicked() {
                            fire = Some(cmd);
                            close = true;
                        }
                    }
                });
            });
        if area.response.clicked_elsewhere() || ctx.input(|i| i.key_pressed(Key::Escape)) {
            close = true;
        }
        if close {
            self.plus_menu = None;
        }
        if let Some(c) = fire {
            self.run(ctx, c);
        }
    }

    /// Dragging a selection past the top or bottom edge scrolls, faster the further the pointer is.
    pub(super) fn auto_scroll(&mut self, ctx: &egui::Context, clip: egui::Rect) {
        if !matches!(self.drag, Some(Drag::Text) | Some(Drag::Brackets) | Some(Drag::Cells(_))) {
            return;
        }
        let Some(p) = ctx.input(|i| if i.pointer.primary_down() { i.pointer.latest_pos() } else { None }) else {
            return;
        };
        let d = if p.y < clip.top() + 4.0 {
            p.y - (clip.top() + 4.0)
        } else if p.y > clip.bottom() - 4.0 {
            p.y - (clip.bottom() - 4.0)
        } else {
            0.0
        };
        if d != 0.0 {
            self.doc.scroll_by += (d * 0.35).clamp(-40.0, 40.0);
            ctx.request_repaint();
        }
    }

    /// The rectangle (canvas coordinates) the selection occupies.
    pub(super) fn selection_rect(&mut self, ctx: &egui::Context) -> Option<egui::Rect> {
        match self.doc.nb.selection().clone() {
            Selection::Caret { cell, pos, .. } => self.doc.view.caret_rect(ctx, &self.doc.nb, cell, pos),
            Selection::Gap(g) => {
                let y = self.doc.view.gap_y(g);
                Some(egui::Rect::from_min_max(egui::pos2(0.0, y - 10.0), egui::pos2(10.0, y + 10.0)))
            }
            Selection::Cells { ids, .. } => {
                let i = self.doc.nb.index_of(*ids.first()?)?;
                let r = self.doc.view.cell_rect(i);
                Some(r.intersect(egui::Rect::from_min_size(r.min, vec2(r.width(), 40.0))))
            }
        }
    }

    pub(super) fn mouse(&mut self, ctx: &egui::Context, resp: &egui::Response, origin: egui::Pos2) {
        let (pointer, pressed, released, secondary, mods, time) = ctx.input(|i| {
            // the modifiers held when the button went down (a Shift released in the same frame still counts)
            let press = i.events.iter().find_map(|e| match e {
                egui::Event::PointerButton { button: egui::PointerButton::Primary, pressed: true, modifiers, .. } => {
                    Some(*modifiers)
                }
                _ => None,
            });
            (
                i.pointer.interact_pos(),
                i.pointer.primary_pressed(),
                i.pointer.primary_released(),
                i.pointer.secondary_pressed(),
                press.unwrap_or(i.modifiers),
                i.time,
            )
        });
        let Some(screen) = pointer else { return };
        let p = (screen - origin).to_pos2();
        let over = resp.hovered() || resp.dragged();
        let hit = if over { Some(self.doc.view.hit(ctx, &self.doc.nb, p)) } else { None };
        self.doc.view.hover = match hit {
            Some(Hit::Bracket(b)) if resp.hovered() => Some(b),
            _ => None,
        };
        self.doc.view.plus_hover = matches!(hit, Some(Hit::Plus(_))) && resp.hovered();
        // a documented function name under the pointer: its usage line after a pause, Ctrl+click opens its page
        let name = match hit {
            Some(h) if resp.hovered() && self.drag.is_none() => self.name_under(ctx, h, p),
            _ => None,
        };
        self.set_hover(name, time);
        self.help.warning = match hit {
            Some(h) if resp.hovered() && self.drag.is_none() => self.warning_under(ctx, h, p),
            _ => None,
        };
        let ctrl_now = ctx.input(|i| i.modifiers.command || i.modifiers.ctrl);
        if over {
            // the pointer per region (the polish design): I-beam over text, the arrow over brackets,
            // margins and the gap between cells, a hand on the "+" button, a closed hand while moving cells; hidden
            // while typing until the mouse moves
            let icon = match (self.drag, hit) {
                (Some(Drag::Resize(_, h, _, _)), _) | (_, Some(Hit::Handle(_, h))) => h.cursor(),
                (_, Some(Hit::Large(..))) => egui::CursorIcon::PointingHand,
                (Some(Drag::Text), _) => egui::CursorIcon::Text,
                (Some(Drag::Cells(_)), _) if self.doc.view.drop_gap.is_some() => egui::CursorIcon::Grabbing,
                (_, Some(Hit::Text(id, _) | Hit::Boxes(id))) if self.over_content(id, p.x) => egui::CursorIcon::Text,
                (_, Some(Hit::Plus(_))) => egui::CursorIcon::PointingHand,
                _ => egui::CursorIcon::Default,
            };
            let icon = if ctrl_now && self.drag.is_none() && self.hovered_name().is_some() {
                egui::CursorIcon::PointingHand
            } else {
                icon
            };
            let icon =
                if self.pointer_hidden && icon == egui::CursorIcon::Text { egui::CursorIcon::None } else { icon };
            ctx.set_cursor_icon(icon);
        }
        if secondary && resp.hovered() {
            // a right-click selects what it is on, unless that is already selected
            match self.doc.view.hit(ctx, &self.doc.nb, p) {
                Hit::Bracket(b) if !self.bracket_selected(b) => self.run(ctx, Command::ClickBracket(b, false, false)),
                Hit::Graphic(id) => {
                    self.run(ctx, Command::ClickText(id, 0, false));
                    self.doc.view.graphic = Some(id);
                }
                Hit::Boxes(id) => {
                    let editor = self.doc.nb.any_editor();
                    let selected = matches!(editor, Some((b, e)) if b == id && e.selection().is_some());
                    if !selected {
                        if let Some((path, k)) = self.doc.view.box_hit(ctx, &self.doc.nb, id, p) {
                            self.run(ctx, Command::ClickBox(id, path, k, false));
                        }
                    }
                }
                Hit::Text(id, pos) => {
                    let inside = matches!(self.doc.nb.selection(), Selection::Caret { cell, anchor, pos: q } if *cell == id && anchor != q);
                    if !inside {
                        self.run(ctx, Command::ClickText(id, pos, false));
                    }
                }
                _ => {}
            }
        }
        if pressed && resp.hovered() && (mods.ctrl || mods.command) && !mods.shift {
            if let Some(name) = self.hovered_name() {
                self.open_docs(Some(name));
                self.drag = None;
                return;
            }
        }
        if pressed && resp.hovered() {
            let (t0, p0, n0) = self.last_click;
            let count = if time - t0 < DOUBLE_CLICK && (screen - p0).length() < 5.0 { n0 + 1 } else { 1 };
            self.last_click = (time, screen, count);
            // any press lets go of the keyboard slider (a press on a slider takes it again below)
            self.slider = None;
            match self.doc.view.hit(ctx, &self.doc.nb, p) {
                Hit::Text(id, pos) => {
                    let cmd = match count {
                        1 => Command::ClickText(id, pos, mods.shift),
                        2 => Command::SelectWord(id, pos),
                        _ => Command::SelectLine(id, pos),
                    };
                    self.run(ctx, cmd);
                    self.drag = Some(Drag::Text);
                }
                Hit::Bracket(b) => {
                    if count >= 2 {
                        self.run(ctx, Command::DoubleClickBracket(b));
                        self.drag = None;
                    } else if self.bracket_selected(b) && !mods.shift && !mods.ctrl {
                        self.drag = Some(Drag::Cells(screen));
                    } else {
                        self.run(ctx, Command::ClickBracket(b, mods.shift, mods.ctrl));
                        self.drag = Some(Drag::Brackets);
                    }
                }
                Hit::Gap(g) => {
                    self.run(ctx, Command::ClickGap(g));
                    self.drag = None;
                }
                Hit::Plus(_) => {
                    self.plus_menu = Some(screen);
                    self.drag = None;
                }
                Hit::Graphic(id) => {
                    // a click selects the picture (the caret goes before it, hidden); a double-click restores its
                    // own size (design 6), or a 3D picture's own view; dragging a 3D picture turns it
                    self.run(ctx, Command::ClickText(id, 0, false));
                    self.doc.view.graphic = Some(id);
                    let double = count >= 2 || resp.double_clicked();
                    let turning = self.view3d_of(id);
                    let turned = self.doc.nb.cell(id).is_some_and(|c| crate::rotate3d::VIEW_ATTRS.iter().any(|a| c.out_extra.has(a)));
                    if double && turned {
                        let reset = crate::rotate3d::VIEW_ATTRS.iter().map(|a| (a.to_string(), None)).collect();
                        self.run(ctx, Command::SetOutputAttrs(id, reset));
                        self.rot.borrow_mut().forget(id);
                        self.doc.view.graphic = Some(id);
                    } else if double && self.doc.nb.cell(id).is_some_and(|c| c.graphic_size().is_some()) {
                        self.run(ctx, Command::ResizeGraphic(id, None));
                        self.doc.view.graphic = Some(id);
                    }
                    let rect = self.doc.nb.index_of(id).and_then(|i| self.doc.view.graphic_rect(&self.doc.nb, i));
                    self.drag = match (turning, rect) {
                        (Some((v, _)), Some(r)) if !double => Some(Drag::Rotate(id, screen, v, r.size())),
                        _ => None,
                    };
                }
                Hit::Control(id, k) => {
                    // a press on a slider moves it there; dragging moves it on; the release lets go (round 13)
                    if let Some(i) = self.doc.nb.index_of(id) {
                        if let Some(v) = self.doc.view.control_value_at(&self.doc.nb, i, k, p.x) {
                            self.run(ctx, Command::SetControl(id, k, v, false));
                        }
                    }
                    self.drag = Some(Drag::Control(id, k));
                    self.slider = Some((id, k));
                }
                Hit::Handle(id, h) => {
                    let rect = self.doc.nb.index_of(id).and_then(|i| self.doc.view.graphic_rect(&self.doc.nb, i));
                    let size = rect.map_or(egui::Vec2::ZERO, |r| r.size());
                    // the edge opposite the handle stays put while dragging (a middle handle keeps the right edge)
                    self.doc.view.live_anchor = rect.map(|r| if h.x > 0 { (r.left(), 1) } else { (r.right(), -1) });
                    self.drag = Some(Drag::Resize(id, h, screen, size));
                }
                Hit::Large(id, a) => {
                    self.doc.view.large_action(id, a);
                    self.drag = None;
                }
                Hit::Boxes(id) => {
                    if let Some((path, k)) = self.doc.view.box_hit(ctx, &self.doc.nb, id, p) {
                        if count <= 2 {
                            self.run(ctx, Command::ClickBox(id, path, k, mods.shift && count == 1));
                            if count == 2 {
                                // a double-click selects the word or the innermost box there (Ctrl+.)
                                self.run(ctx, Command::ExtendSelection);
                            }
                        } else if let Some(line) = self.doc.view.box_line_items(ctx, &self.doc.nb, id, p) {
                            // a triple-click selects the visual line of the cell
                            self.run(ctx, Command::ClickBox(id, vec![], line.start, false));
                            self.run(ctx, Command::ClickBox(id, vec![], line.end, true));
                        }
                    }
                    self.drag = Some(Drag::Text);
                }
            }
        } else if resp.dragged() {
            match self.drag {
                Some(Drag::Resize(id, h, start, from)) => {
                    let size = resize(from, screen - start, h, mods.shift);
                    self.doc.view.live_size = Some((id, size));
                }
                Some(Drag::Control(id, k)) => {
                    if let Some(i) = self.doc.nb.index_of(id) {
                        if let Some(v) = self.doc.view.control_value_at(&self.doc.nb, i, k, p.x) {
                            let now = self.doc.nb.manipulate_of(id).and_then(|(_, vs)| vs.get(k).copied());
                            if now != Some(v) {
                                self.run(ctx, Command::SetControl(id, k, v, false));
                            }
                        }
                    }
                }
                Some(Drag::Rotate(id, start, from, size)) => {
                    let d = screen - start;
                    if d.length() > 2.0 {
                        let (w, h) = (f64::from(size.x.max(1.0)), f64::from(size.y.max(1.0)));
                        let v = mc_graphics::view3d::orbit_drag(&from, f64::from(d.x), f64::from(d.y), w, h);
                        self.doc.view.live_view = Some((id, v));
                    }
                }
                Some(Drag::Text) => {
                    let hit = self.doc.view.hit(ctx, &self.doc.nb, p);
                    // a drag that started in typeset boxes stays in their cell: beyond its edges it selects to the
                    // start (above) or the end (below)
                    let editor = self.doc.nb.any_editor().map(|(b, _)| b);
                    let target = match (hit, editor) {
                        (Hit::Boxes(id), _) | (_, Some(id)) => Some(id),
                        _ => None,
                    };
                    if let Some(id) = target {
                        if let Some((path, k)) = self.doc.view.box_hit(ctx, &self.doc.nb, id, p) {
                            let editor = self.doc.nb.any_editor();
                            let at = matches!(editor, Some((b, e)) if b == id && e.caret().path == path && e.caret().offset == k);
                            if !at {
                                self.run(ctx, Command::ClickBox(id, path, k, true));
                            }
                        }
                    }
                    if let (Hit::Text(id, pos), None) = (hit, target) {
                        if !matches!(self.doc.nb.selection(), Selection::Caret { cell, pos: q, .. } if *cell == id && *q == pos)
                        {
                            self.run(ctx, Command::ClickText(id, pos, true));
                        }
                    }
                }
                Some(Drag::Brackets) => {
                    if let Some(i) = self.cell_at_y(p.y) {
                        let id = self.doc.nb.cells()[i].id;
                        let b = Bracket::Cell(id);
                        if !self.bracket_selected(b) {
                            self.run(ctx, Command::ClickBracket(b, true, false));
                        }
                    }
                }
                Some(Drag::Cells(start)) if (screen - start).length() > 4.0 => {
                    self.doc.view.drop_gap = Some(self.gap_at_y(p.y));
                }
                _ => {}
            }
        }
        if released {
            if let Some(Drag::Resize(id, ..)) = self.drag {
                // the size is kept in points at 100 % (the file's `size=WxH`)
                self.doc.view.live_anchor = None;
                if let Some((_, s)) = self.doc.view.live_size.take() {
                    let m = self.doc.view.magnification();
                    let (w, h) = ((s.x / m).round().max(1.0) as u32, (s.y / m).round().max(1.0) as u32);
                    self.run(ctx, Command::ResizeGraphic(id, Some((w, h))));
                    self.doc.view.graphic = Some(id);
                }
            } else if let Some(Drag::Control(id, k)) = self.drag {
                if let Some(i) = self.doc.nb.index_of(id) {
                    if let Some(v) = self.doc.view.control_value_at(&self.doc.nb, i, k, p.x) {
                        self.run(ctx, Command::SetControl(id, k, v, true));
                    }
                }
            } else if let Some(Drag::Rotate(id, ..)) = self.drag {
                // the view is kept on the output (an undo unit)
                if let Some((_, v)) = self.doc.view.live_view {
                    self.keep_view(ctx, id, v);
                }
            } else if let (Some(Drag::Cells(_)), Some(g)) = (self.drag, self.doc.view.drop_gap.take()) {
                self.run(ctx, Command::MoveCells(g, mods.ctrl));
            } else if let Some(Drag::Cells(_)) = self.drag {
                // a plain click on a selected bracket selects just it
                if let Hit::Bracket(b) = self.doc.view.hit(ctx, &self.doc.nb, p) {
                    self.run(ctx, Command::ClickBracket(b, false, false));
                }
            }
            self.drag = None;
        }
    }

    pub(super) fn bracket_selected(&self, b: Bracket) -> bool {
        let Selection::Cells { ids, .. } = self.doc.nb.selection() else {
            return false;
        };
        match b {
            Bracket::Cell(id) => ids.contains(&id),
            Bracket::Group(s, e) => (s..e).all(|i| self.doc.nb.cells().get(i).is_some_and(|c| ids.contains(&c.id))),
        }
    }

    pub(super) fn cell_at_y(&self, y: f32) -> Option<usize> {
        (0..self.doc.nb.cells().len()).find(|&i| {
            self.doc.nb.is_visible(i) && {
                let r = self.doc.view.cell_rect(i);
                y >= r.top() && y < r.bottom()
            }
        })
    }

    /// The insertion point nearest to `y` (between visible cells).
    pub(super) fn gap_at_y(&self, y: f32) -> usize {
        let n = self.doc.nb.cells().len();
        match self.cell_at_y(y) {
            Some(i) => {
                let r = self.doc.view.cell_rect(i);
                if y < r.center().y {
                    i
                } else {
                    let mut j = i + 1;
                    while j < n && !self.doc.nb.is_visible(j) {
                        j += 1;
                    }
                    j
                }
            }
            None => {
                if y < 10.0 {
                    0
                } else {
                    n
                }
            }
        }
    }

    pub(super) fn context_menu(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let mut fire = None;
        let cells = matches!(self.doc.nb.selection(), Selection::Cells { .. });
        ui.set_min_width(super::chrome::MENU_MIN_WIDTH);
        // the quick fix of the warning the menu was opened on (round 20)
        if let Some((id, w)) = self.help.menu_warning.clone().filter(|(_, w)| !w.fix.is_empty()) {
            let mut label = w.fix_label.clone();
            if let Some(first) = label.get(..1) {
                label.replace_range(..1, &first.to_uppercase());
            }
            if let Some((_, m)) = w.message.split_once("did you mean ") {
                label = format!("{label}: {}", m.trim_end_matches('?'));
            }
            if ui.add(super::chrome::item(ui, &label)).clicked() {
                self.help.menu_warning = None;
                self.apply_fix(&ctx, id, &w);
                ui.close();
                return;
            }
            ui.separator();
        }
        let items: Vec<(&str, Command)> = vec![
            ("Cut", Command::Cut),
            ("Copy", Command::Copy),
            ("Copy as plain text", Command::CopyPlain),
            ("Copy as LaTeX", Command::CopyLatex),
            ("Paste", Command::Paste),
            ("Evaluate cells", Command::Evaluate),
        ];
        for (label, cmd) in items {
            self.menu_item(ui, label, cmd, &mut fire);
        }
        ui.separator();
        self.style_menu(ui, &mut fire);
        self.menu_item(ui, "Divide cell", Command::Divide, &mut fire);
        if cells {
            self.menu_item(ui, "Merge cells", Command::Merge, &mut fire);
            self.menu_item(ui, "Group together", Command::GroupTogether, &mut fire);
            self.menu_item(ui, "Ungroup", Command::Ungroup, &mut fire);
            self.menu_item(ui, "Delete cells", Command::Delete, &mut fire);
        }
        self.menu_item(ui, "Open/close group", Command::ToggleGroup, &mut fire);
        self.menu_item(ui, "Show expression", Command::ShowExpression, &mut fire);
        if let Some(id) = self.doc.view.graphic {
            ui.separator();
            if ui.add(super::chrome::item(ui, "Copy graphic")).clicked() {
                self.copy_graphic(id);
                ui.close();
            }
            let sized = self.doc.nb.cell(id).is_some_and(|c| c.graphic_size().is_some());
            if ui.add_enabled(sized, super::chrome::item(ui, "Natural size")).clicked() {
                fire = Some(Command::ResizeGraphic(id, None));
                ui.close();
            }
        }
        if let Some(c) = fire {
            self.run(&ctx, c);
        }
    }
}
