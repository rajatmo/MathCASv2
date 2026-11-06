//! Help while writing code (round 12): the usage hint inside a call, hover over a function name, and the name F1
//! looks up. The usage lines come from the help index (`mc_help`), the one source shared with the documentation
//! window and the language server. Completion is in `keyboard.rs`.
use super::*;
use mc_help::{scan, Index};

/// How long the pointer rests on a function name before its usage line shows (seconds).
const HOVER_DELAY: f64 = 0.45;
/// The room the usage hint needs above the caret's line (points); with less it shows below the line.
const HINT_ROOM: f32 = 64.0;

/// The caret in a code cell: the cell, its text (the canonical text of a cell with boxes), and the caret and the
/// other end of the selection as bytes of that text.
pub(super) struct CodeCaret {
    pub cell: mc_nbmodel::CellId,
    pub text: String,
    pub pos: usize,
    pub anchor: usize,
}

/// What the help of the window is doing (the self-test's `help` step logs it).
#[derive(Default)]
pub(crate) struct HelpState {
    /// the usage hint is hidden (Esc) for the call opened at this byte of this cell
    hint_hidden: Option<(mc_nbmodel::CellId, usize)>,
    /// the hint shown in the last frame: its call (cell, opening bracket) and a description
    hint_at: Option<(mc_nbmodel::CellId, usize)>,
    hint_shown: Option<String>,
    /// the documented name under the pointer and since when
    hover: Option<(String, f64)>,
    hover_shown: Option<String>,
    /// the pages F1, Help > Look up and Ctrl+click asked for
    pub(crate) docs_opened: Vec<String>,
}

impl App {
    /// The caret of the code cell with the caret (Input or Code), in its text.
    pub(super) fn code_caret(&self) -> Option<CodeCaret> {
        let Selection::Caret { cell, anchor, pos } = *self.doc.nb.selection() else { return None };
        let c = self.doc.nb.cell(cell)?;
        if !c.style.is_code() || c.expr_view {
            return None;
        }
        if mc_nbmodel::is_boxed(c) {
            let (id, ed) = self.doc.nb.box_editor()?;
            if id != cell {
                return None;
            }
            let (text, map) = mc_boxes::to_text_mapped(ed.root());
            let caret = ed.caret();
            let pos = map.offset(&caret.path, caret.offset)?;
            let anchor = match ed.selection() {
                Some((path, r)) => {
                    let (a, b) = (map.offset(&path, r.start)?, map.offset(&path, r.end)?);
                    if pos == a {
                        b
                    } else {
                        a
                    }
                }
                None => pos,
            };
            return Some(CodeCaret { cell, text, pos, anchor });
        }
        Some(CodeCaret { cell, text: c.text.clone(), pos, anchor })
    }

    /// The usage hint: inside the brackets of a call of a documented function, its usage line with the argument
    /// being typed highlighted, above the caret's line. Esc hides it for that call.
    pub(super) fn usage_hint(&mut self, ctx: &egui::Context) {
        self.help.hint_shown = None;
        self.help.hint_at = None;
        if self.doc.completion.is_some() || self.modal_open() {
            return;
        }
        let Some(cc) = self.code_caret().filter(|c| c.anchor == c.pos) else {
            self.help.hint_hidden = None;
            return;
        };
        // in a cell with boxes, a square root box is written `((sqrt(...)))` underneath: that is not a call typed
        let boxed = self.doc.nb.cell(cc.cell).is_some_and(mc_nbmodel::is_boxed);
        let box_mark = |c: &scan::Call| boxed && c.name == "sqrt" && cc.text[..c.name_range.start].ends_with("((");
        let Some(call) = scan::calls_at(&cc.text, cc.pos).into_iter().find(|c| !box_mark(c)) else {
            self.help.hint_hidden = None;
            return;
        };
        let at = (cc.cell, call.open);
        if self.help.hint_hidden.is_some_and(|h| h != at) {
            self.help.hint_hidden = None;
        }
        if self.help.hint_hidden == Some(at) {
            return;
        }
        let simple = call.name.rsplit('.').next().unwrap_or(&call.name);
        let Some(entry) = Index::embedded().get(simple) else { return };
        let calls = entry.calls();
        let k = entry.best_call(call.arg);
        let Some(line) = calls.get(k) else { return };
        let hl = scan::highlight(line, call.arg);
        let Some(r) = self.doc.view.caret_rect(ctx, &self.doc.nb, cc.cell, cc.pos) else { return };
        // above the caret's line, or below it when the line is near the top of the canvas
        let r = r.translate(self.doc.view.origin.to_vec2());
        let below = r.top() - HINT_ROOM < self.canvas_clip.top();
        let (anchor, pivot) = if below {
            (r.left_bottom() + vec2(0.0, 3.0), egui::Align2::LEFT_TOP)
        } else {
            (r.left_top() - vec2(0.0, 3.0), egui::Align2::LEFT_BOTTOM)
        };
        let p = self.doc.view.palette.clone();
        let text = entry.text_of(k);
        let mono = egui::FontId::monospace(13.0);
        let mut job = egui::text::LayoutJob::default();
        let plain = egui::TextFormat { font_id: mono.clone(), color: p.text, ..Default::default() };
        let strong = egui::TextFormat {
            font_id: mono,
            color: p.accent,
            background: p.over_bg(p.accent, 0.14),
            underline: egui::Stroke::new(1.0, p.accent),
            ..Default::default()
        };
        match &hl {
            Some(h) => {
                job.append(&line[..h.start], 0.0, plain.clone());
                job.append(&line[h.clone()], 0.0, strong);
                job.append(&line[h.end..], 0.0, plain);
            }
            None => job.append(line, 0.0, plain),
        }
        egui::Area::new(egui::Id::unique("mc-notebook-usage-hint"))
            .fixed_pos(anchor)
            .pivot(pivot)
            .order(egui::Order::Foreground)
            .interactable(false)
            .show(ctx, |ui| {
                Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_max_width(560.0);
                    ui.horizontal(|ui| {
                        ui.label(job);
                        if calls.len() > 1 {
                            ui.label(RichText::new(format!("{} of {}", k + 1, calls.len())).size(11.0).color(p.faint));
                        }
                    });
                    if !text.is_empty() {
                        ui.label(RichText::new(&text).size(12.0).color(p.weak));
                    }
                });
            });
        self.help.hint_at = Some(at);
        self.help.hint_shown = Some(format!(
            "{} argument {} `{}`",
            line,
            call.arg + 1,
            hl.map_or(String::new(), |h| line[h].to_string())
        ));
    }

    /// Esc while the usage hint shows hides it (for this call); `true` when the key was used.
    pub(super) fn hint_key(&mut self, e: &egui::Event) -> bool {
        let Some(at) = self.help.hint_at else { return false };
        if matches!(e, egui::Event::Key { key: Key::Escape, pressed: true, modifiers, .. } if !modifiers.any()) {
            self.help.hint_hidden = Some(at);
            self.help.hint_at = None;
            self.help.hint_shown = None;
            return true;
        }
        false
    }

    /// The documented function name under the pointer at canvas point `p` over `hit`, if any: a global name of an
    /// Input or Code cell (a local or a parameter of the same name is not the function).
    pub(super) fn name_under(&mut self, ctx: &egui::Context, hit: Hit, p: egui::Pos2) -> Option<String> {
        let (id, boxes) = match hit {
            Hit::Text(id, _) => (id, false),
            Hit::Boxes(id) => (id, true),
            _ => return None,
        };
        let c = self.doc.nb.cell(id)?;
        if !c.style.is_code() || c.expr_view {
            return None;
        }
        let name = if boxes {
            let (path, k) = self.doc.view.box_hit(ctx, &self.doc.nb, id, p)?;
            let (text, map) = mc_boxes::to_text_mapped(&mc_boxes::from_text(&c.text));
            let off = map.offset(&path, k)?;
            text[scan::word_at(&text, off)?].to_string()
        } else {
            let Hit::Text(_, byte) = hit else { return None };
            let range = scan::word_at(&c.text, byte)?;
            // the pointer is over the name itself (not past the end of its line)
            let a = self.doc.view.caret_rect(ctx, &self.doc.nb, id, range.start)?;
            let b = self.doc.view.caret_rect(ctx, &self.doc.nb, id, range.end)?;
            if p.x < a.left() || p.x > b.left() || p.y < a.top() || p.y > b.bottom() {
                return None;
            }
            let c = self.doc.nb.cell(id)?;
            let colouring = self.doc.view.colouring(c);
            let global = colouring
                .tokens
                .iter()
                .find(|(r, _)| r.start == range.start)
                .is_none_or(|(_, k)| matches!(k, mc_syntax::TokenClass::Global | mc_syntax::TokenClass::Undefined));
            if !global {
                return None;
            }
            c.text[range].to_string()
        };
        Index::embedded().get(&name).map(|_| name)
    }

    /// Keep the hovered name and its time (the usage line shows after [`HOVER_DELAY`]).
    pub(super) fn set_hover(&mut self, name: Option<String>, time: f64) {
        match name {
            Some(n) if self.help.hover.as_ref().is_some_and(|(h, _)| *h == n) => {}
            Some(n) => self.help.hover = Some((n, time)),
            None => self.help.hover = None,
        }
    }

    /// The name under the pointer, when there is one.
    pub(super) fn hovered_name(&self) -> Option<String> {
        self.help.hover.as_ref().map(|(n, _)| n.clone())
    }

    /// The hover's tooltip: the usage line of the name under the pointer once it has rested there.
    pub(super) fn hover_tooltip(&mut self, ctx: &egui::Context, resp: egui::Response) -> egui::Response {
        self.help.hover_shown = None;
        let Some((name, since)) = self.help.hover.clone() else { return resp };
        // not while dragging, nor over text being typed (the pointer hides while typing)
        if self.drag.is_some() || self.pointer_hidden {
            return resp;
        }
        let now = ctx.input(|i| i.time);
        if now - since < HOVER_DELAY {
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(HOVER_DELAY - (now - since) + 0.01));
            return resp;
        }
        let Some(e) = Index::embedded().get(&name) else { return resp };
        let line = e.line();
        let p = self.doc.view.palette.clone();
        self.help.hover_shown = Some(line.clone());
        resp.on_hover_ui_at_pointer(|ui| {
            ui.set_max_width(520.0);
            ui.label(RichText::new(&line).monospace().color(p.text));
            ui.label(RichText::new("Ctrl+click opens its page").size(11.5).color(p.faint));
        })
    }

    /// Where on the screen the middle of the first `name` of the plain cell with the caret is (self-test).
    pub fn screen_point_of(&mut self, name: &str) -> Option<egui::Pos2> {
        let cc = self.code_caret()?;
        let at = cc.text.find(name)?;
        let ctx = self.ctx.clone();
        let a = self.doc.view.caret_rect(&ctx, &self.doc.nb, cc.cell, at)?;
        let b = self.doc.view.caret_rect(&ctx, &self.doc.nb, cc.cell, at + name.len())?;
        Some(self.doc.view.origin + a.union(b).center().to_vec2())
    }

    /// A line for the self-test: completion, the usage hint, the hover and the pages asked for.
    pub fn help_state(&self) -> String {
        let completion = match &self.doc.completion {
            Some(c) => {
                let prefix = self.word_at_caret().map(|w| w.3).unwrap_or_default();
                let items = rank(&c.names, &prefix);
                let first: Vec<&str> = items.iter().take(3).map(|(n, _)| n.as_str()).collect();
                let selected = items.get(c.selected.min(items.len().saturating_sub(1))).map_or("", |(n, _)| n.as_str());
                format!("open, {} items, first {first:?}, selected `{selected}`", items.len())
            }
            None => "closed".into(),
        };
        format!(
            "completion {completion}; hint {}; hover {}; docs {:?}",
            self.help.hint_shown.as_deref().unwrap_or("none"),
            self.help.hover_shown.as_deref().unwrap_or("none"),
            self.help.docs_opened
        )
    }
}

/// The name F1 looks up in `text` with the selection `anchor..pos`: the selected name, or the name around the
/// caret when it is a documented one (`known`), else the function whose call the caret is in (`sin(x|)` looks up
/// `sin`), else the name around the caret.
pub(super) fn help_target(text: &str, anchor: usize, pos: usize, known: &dyn Fn(&str) -> bool) -> Option<String> {
    let at = help_name(text, anchor, pos);
    if anchor != pos || at.as_deref().is_some_and(known) {
        return at;
    }
    scan::call_at(text, pos).map(|c| c.name.rsplit('.').next().unwrap_or(&c.name).to_string()).or(at)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn f1_finds_the_call_around_the_caret() {
        let known = |n: &str| ["n_integrate", "sin", "pi"].contains(&n);
        let f1 = |t: &str, a: usize, b: usize| help_target(t, a, b, &known);
        let t = "y = n_integrate(sin(x) + pi, [x, 0, 1])";
        assert_eq!(f1(t, 6, 6).as_deref(), Some("n_integrate"));
        assert_eq!(f1(t, 23, 23).as_deref(), Some("n_integrate"), "after `+ `, inside n_integrate's call");
        assert_eq!(f1(t, 20, 20).as_deref(), Some("sin"), "a variable at the caret: the call around it");
        assert_eq!(f1(t, 26, 26).as_deref(), Some("pi"), "a documented name at the caret wins");
        assert_eq!(f1(t, 22, 22).as_deref(), Some("n_integrate"), "after `sin(x)` the closed call is left");
        assert_eq!(f1("f(a, b)", 4, 4).as_deref(), Some("f"));
        assert_eq!(f1("abc", 3, 3).as_deref(), Some("abc"), "an unknown name outside calls is still looked up");
        assert_eq!(f1(t, 4, 15).as_deref(), Some("n_integrate"), "a selected name");
        assert_eq!(f1("1 + 2", 2, 2), None);
    }
}
