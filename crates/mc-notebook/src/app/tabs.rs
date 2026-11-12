//! Tabs and the session: parking the active notebook in its tab, closing with unsaved prompts, reopening, the
//! tabs restored at the next start.
use super::*;

impl App {
    /// Put the active notebook back into its tab (before the tab list changes).
    pub(super) fn park(&mut self) {
        let blank = Doc::new(Notebook::new(), None, &self.ctx, &self.images, &self.glyphs, self.doc.view.palette.clone(), 1.0);
        let d = std::mem::replace(&mut self.doc, blank);
        if let Some(t) = self.tabs.current_mut() {
            t.doc = Some(d);
        }
    }

    /// Take the active tab's notebook out of the list (after the tab list changed); an empty list gets a new
    /// Untitled tab.
    pub(super) fn unpark(&mut self) {
        if self.tabs.is_empty() {
            self.tabs.add(None, None);
        }
        let d = self.tabs.current_mut().and_then(|t| t.doc.take());
        self.doc = match d {
            Some(d) => d,
            None => self.blank_doc(),
        };
        // the palette may have changed while the tab was in the background
        if let Some(p) = &self.applied {
            if &self.doc.view.palette != p {
                self.doc.view.palette = p.clone();
                self.doc.view.clear();
            }
        }
        self.tab_switched();
    }

    pub(super) fn tab_switched(&mut self) {
        self.status.clear();
        self.doc.completion = None;
        self.doc.auto_complete_at = None;
        self.drag = None;
        self.tab_scroll_to_active = true;
        self.doc.view.blink_from = self.ctx.input(|i| i.time);
    }

    pub(super) fn blank_doc(&self) -> Doc {
        Doc::new(Notebook::new(), None, &self.ctx, &self.images, &self.glyphs, self.doc.view.palette.clone(), self.settings.magnification)
    }

    /// Change the tab list with the active notebook parked in it.
    pub(crate) fn with_tabs<R>(&mut self, f: impl FnOnce(&mut Tabs<Option<Doc>>) -> R) -> R {
        self.park();
        let r = f(&mut self.tabs);
        self.unpark();
        r
    }

    /// Make tab `i` active.
    pub fn activate(&mut self, i: usize) {
        if i != self.tabs.active() {
            self.with_tabs(|t| t.select(i));
        }
    }

    /// A new Untitled tab.
    pub fn new_tab(&mut self) {
        let d = self.blank_doc();
        self.with_tabs(|t| t.add(None, Some(d)));
    }

    /// The path of the active tab.
    pub fn path(&self) -> Option<PathBuf> {
        self.tabs.current().and_then(|t| t.path.clone())
    }

    /// Close the tabs with these ids (asking about unsaved changes first).
    pub fn close_tabs(&mut self, ids: &[u64]) {
        self.sync_modified();
        let ids = ids.to_vec();
        let closed = self.with_tabs(|t| t.close(&ids));
        self.closed(closed);
    }

    pub(super) fn closed(&mut self, c: Closed<Option<Doc>>) {
        // the closed tabs' documents drop here: their kernels get `shutdown` with `interrupt`
        drop(c.docs);
        if let Some(Ask::Save(id)) = &c.ask {
            if let Some(i) = self.tabs.index_of(*id) {
                self.activate(i);
            }
        }
        if c.window {
            self.confirmed_close = true;
            self.ctx.send_viewport_cmd(ViewportCommand::Close);
        }
    }

    /// Answer the unsaved-changes question.
    pub(super) fn answer_close(&mut self, a: Answer) {
        let a = match (a, self.tabs.asked().cloned()) {
            (Answer::Saved, Some(Ask::Save(id))) => {
                if let Some(i) = self.tabs.index_of(id) {
                    self.activate(i);
                }
                if self.save() {
                    Answer::Saved
                } else {
                    Answer::Cancel
                }
            }
            (Answer::Saved, Some(Ask::SaveAll(ids))) => {
                let mut ok = true;
                for id in ids {
                    if let Some(i) = self.tabs.index_of(id) {
                        self.activate(i);
                        ok = self.save();
                    }
                    if !ok {
                        break;
                    }
                }
                if ok {
                    Answer::Saved
                } else {
                    Answer::Cancel
                }
            }
            (a, _) => a,
        };
        let c = self.with_tabs(|t| t.answer(a));
        self.closed(c);
    }

    /// Reopen the last closed file.
    pub(super) fn reopen_tab(&mut self) {
        if let Some(p) = self.tabs.reopen() {
            self.open(&p);
        }
    }

    pub(super) fn sync_modified(&mut self) {
        let modified = self.doc.nb.is_modified();
        if let Some(t) = self.tabs.current_mut() {
            t.modified = modified;
        }
    }

    /// Keep the tabs' modified marks current, let background tabs talk to their kernels, take files sent by other
    /// launches.
    pub(super) fn sync_tabs(&mut self) {
        self.sync_modified();
        let active = self.tabs.active();
        for (i, t) in self.tabs.iter_mut().enumerate() {
            if i != active {
                if let Some(d) = t.doc.as_mut() {
                    d.nb.pump(&mut d.kernel);
                    t.modified = d.nb.is_modified();
                }
            }
        }
        let sent = self.instance.as_ref().map(|s| s.take()).unwrap_or_default();
        for p in sent {
            self.open(&p);
            self.ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
            self.ctx.send_viewport_cmd(ViewportCommand::Focus);
        }
    }

    /// The kernel of tab `i` is busy (the tab shows a spinner).
    pub(crate) fn tab_busy(&self, i: usize) -> bool {
        let state = if i == self.tabs.active() {
            self.doc.nb.kernel_state()
        } else {
            match self.tabs.get(i).and_then(|t| t.doc.as_ref()) {
                Some(d) => d.nb.kernel_state(),
                None => return false,
            }
        };
        state == KernelState::Busy
    }


    pub(super) fn restore_session(&mut self) {
        let session = self.settings.session.clone();
        let mut active = None;
        for (k, s) in session.tabs.iter().enumerate() {
            if !s.path.exists() {
                continue;
            }
            self.open(&s.path);
            if self.path().as_deref() != Some(s.path.as_path()) {
                continue;
            }
            self.doc.restore_offset = Some(s.offset);
            if let Some((i, pos)) = s.caret {
                let target = self.doc.nb.cells().get(i).filter(|c| c.text.is_char_boundary(pos.min(c.text.len())));
                if let Some((id, pos)) = target.map(|c| (c.id, pos.min(c.text.len()))) {
                    let ctx = self.ctx.clone();
                    self.run(&ctx, Command::ClickText(id, pos, false));
                }
            }
            self.doc.scroll_pending = false;
            if k == session.active {
                active = Some(self.tabs.active());
            }
        }
        if let Some(i) = active {
            self.activate(i);
        }
    }

    /// The open file tabs, for the next start.
    pub(super) fn session(&self) -> crate::settings::Session {
        let mut tabs = vec![];
        let mut active = 0;
        for (i, t) in self.tabs.iter().enumerate() {
            let Some(path) = t.path.clone() else { continue };
            let d = if i == self.tabs.active() { Some(&self.doc) } else { t.doc.as_ref() };
            let Some(d) = d else { continue };
            let caret = match d.nb.selection() {
                Selection::Caret { cell, pos, .. } => d.nb.index_of(*cell).map(|i| (i, *pos)),
                _ => None,
            };
            if i == self.tabs.active() {
                active = tabs.len();
            }
            tabs.push(crate::settings::SessionTab { path, offset: d.offset, caret });
        }
        crate::settings::Session { tabs, active }
    }
}
