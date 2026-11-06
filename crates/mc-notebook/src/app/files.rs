//! Files: opening, saving (with the pictures' sidecar folder), the file dialogs, exporting a script.
use super::*;

impl App {
    /// Open a notebook: activate its tab when it is open, else a new tab (or this one when it is a blank Untitled).
    pub fn open(&mut self, path: &Path) {
        if let Some(i) = self.tabs.find_path(path) {
            self.activate(i);
            return;
        }
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) => {
                self.error = Some(format!("Cannot read {}: {e}", path.display()));
                return;
            }
        };
        match mc_nbfile::Notebook::read(&text) {
            Ok((f, warnings)) => {
                let zoom =
                    f.header.number::<f32>("magnification").map_or(self.settings.magnification, |m| m.clamp(0.5, 3.0));
                let doc = Doc::new(Notebook::from_file(&f), Some(path), &self.ctx, &self.images, &self.glyphs, self.doc.view.palette.clone(), zoom);
                let path = path.to_path_buf();
                if self.doc.is_blank() && self.path().is_none() {
                    self.doc = doc;
                    let i = self.tabs.active();
                    self.tabs.set_path(i, path.clone());
                    self.tab_switched();
                } else {
                    self.with_tabs(|t| t.add(Some(path.clone()), Some(doc)));
                }
                self.status = match warnings.first() {
                    Some(w) => format!("{} warning(s); first: {w}", warnings.len()),
                    None => format!("Opened {}", file_name(&path)),
                };
                self.settings.add_recent(&path);
            }
            Err(e) => self.error = Some(format!("{} is not a notebook: {e}", path.display())),
        }
    }

    /// Replace the active tab's notebook (the old kernel session ends).
    pub fn set_notebook(&mut self, nb: Notebook, path: Option<PathBuf>) {
        let zoom = self.doc.view.zoom;
        self.doc = Doc::new(nb, path.as_deref(), &self.ctx, &self.images, &self.glyphs, self.doc.view.palette.clone(), zoom);
        if let Some(t) = self.tabs.current_mut() {
            t.path = path;
            t.modified = false;
        }
        self.tab_switched();
    }

    pub(super) fn save_to(&mut self, path: &Path) -> bool {
        let start = std::time::Instant::now();
        let mut f = if self.settings.save_without_outputs {
            let mut copy = self.doc.nb.clone();
            copy.apply(Command::DeleteAllOutput, &mut mc_nbmodel::FakeEnv::default());
            let mut f = copy.to_file();
            f.cells.iter_mut().for_each(|c| c.attrs.remove("label"));
            f
        } else {
            self.doc.nb.to_file()
        };
        // the pictures go into the sidecar folder first, so the file never names one that is not there (design 7.5)
        let side = match mc_nbfile::images::store_images(&mut f, self.doc.view.base.as_deref(), path) {
            Ok(s) => s,
            Err(e) => {
                self.error = Some(format!("Cannot copy the pictures next to {}: {e}", path.display()));
                return false;
            }
        };
        let text = f.print();
        match mc_nbfile::write_atomic(path, &text) {
            Ok(()) => {
                let removed = mc_nbfile::images::remove_unreferenced(&f, path).unwrap_or(0);
                self.doc.nb.rename_values(&side.renamed);
                self.doc.view.base = path.parent().map(Path::to_path_buf);
                self.doc.nb.mark_saved();
                if self.path().as_deref() != Some(path) {
                    self.doc.kernel.dir = path.parent().map(Path::to_path_buf);
                    self.doc.kernel.origin = Some(path.display().to_string());
                    if let Some(t) = self.tabs.current_mut() {
                        t.path = Some(path.to_path_buf());
                    }
                }
                self.sync_modified();
                self.settings.add_recent(path);
                let mut notes = vec![];
                if side.copied + removed > 0 {
                    notes.push(format!("{} picture(s) copied, {removed} removed", side.copied));
                }
                if !side.missing.is_empty() {
                    notes.push(format!("{} picture(s) not found", side.missing.len()));
                }
                let notes = if notes.is_empty() { String::new() } else { format!("; {}", notes.join(", ")) };
                self.status = format!(
                    "Saved {} ({:.1} ms{notes})",
                    file_name(path),
                    start.elapsed().as_secs_f64() * 1000.0
                );
                true
            }
            Err(e) => {
                self.error = Some(format!("Cannot save {}: {e}", path.display()));
                false
            }
        }
    }

    pub(super) fn save(&mut self) -> bool {
        match self.path() {
            Some(p) => self.save_to(&p),
            None => self.save_as(),
        }
    }

    pub(super) fn save_as(&mut self) -> bool {
        let mut dialog = rfd::FileDialog::new().add_filter("MathCAS notebook", &["mcn"]);
        if let Some(p) = self.path() {
            if let Some(d) = p.parent() {
                dialog = dialog.set_directory(d);
            }
            if let Some(n) = p.file_name() {
                dialog = dialog.set_file_name(n.to_string_lossy());
            }
        } else {
            let name = self.tabs.current().map_or("Untitled".to_string(), |t| t.title());
            dialog = dialog.set_file_name(format!("{name}.mcn"));
        }
        match dialog.save_file() {
            Some(p) => self.save_to(&p),
            None => false,
        }
    }

    pub(super) fn open_dialog(&mut self) {
        let mut dialog = rfd::FileDialog::new().add_filter("MathCAS notebook", &["mcn"]);
        if let Some(d) = self.path().and_then(|p| p.parent().map(Path::to_path_buf)) {
            dialog = dialog.set_directory(d);
        }
        if let Some(ps) = dialog.pick_files() {
            for p in ps {
                self.open(&p);
            }
        }
    }
    pub(super) fn export_script(&mut self) {
        let dialog = rfd::FileDialog::new().add_filter("MathCAS script", &["mc"]);
        let Some(p) = dialog.save_file() else { return };
        let mut out = String::from("#! mathcas 2\n");
        for c in self.doc.nb.cells() {
            if c.style.is_code() {
                out.push_str(&c.text);
                out.push_str("\n\n");
            } else if !c.generated() {
                for l in c.text.lines() {
                    out.push_str(&format!("# {l}\n"));
                }
                out.push('\n');
            }
        }
        if let Err(e) = mc_nbfile::write_atomic(&p, &out) {
            self.error = Some(format!("Cannot write {}: {e}", p.display()));
        }
    }
}
