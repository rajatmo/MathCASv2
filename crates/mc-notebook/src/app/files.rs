//! Files: opening and saving (pictures embedded in the file, `mc_nbfile::embed`), the file dialogs, exporting.
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
            Ok((mut f, warnings)) => {
                // pictures embedded in the file are shown from the cache they are written to
                let cache = mc_nbfile::embed::cache_dir();
                if let Err(e) = mc_nbfile::embed::extract(&mut f, &cache) {
                    self.error = Some(format!("Cannot write the pictures of {} to {}: {e}", path.display(), cache.display()));
                }
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
        // the magnification (the view's `zoom` is it times the points-to-pixels factor: passing that grew the text by a
        // third with every notebook opened into the tab)
        let zoom = self.doc.view.magnification();
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
        // the pictures go into the file itself (design 7.5, round 13: one file per notebook); the document keeps
        // showing them from where they are, and an older notebook's sidecar folder is left alone
        let base = self.doc.view.base.clone();
        // pictures named relative to the old folder (an older notebook's sidecar folder) are shown by their full
        // path from now on, so a Save As into another folder still finds them
        let absolute: Vec<(String, String)> = f
            .cells
            .iter()
            .flat_map(|c| c.output_lines())
            .filter(|l| l.kind == mc_nbfile::OutputKind::Value)
            .filter_map(|l| mc_nbfile::images::image_path(&l.text))
            .filter(|p| Path::new(p).is_relative())
            .map(|p| {
                let abs = mc_nbfile::images::resolve(&p, base.as_deref()).display().to_string();
                (mc_nbfile::images::image_value(&p), mc_nbfile::images::image_value(&abs))
            })
            .collect();
        let side = mc_nbfile::embed::embed(&mut f, base.as_deref());
        let text = f.print();
        match mc_nbfile::write_atomic(path, &text) {
            Ok(()) => {
                self.doc.nb.rename_values(&absolute);
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
                if side.pictures > 0 {
                    // as stored (drawings and descriptions deflated), before base64
                    notes.push(format!("{} picture(s) embedded, {} KB", side.pictures, side.stored.div_ceil(1024)));
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
    /// File > Export > HTML page / PDF document: the notebook as shown (outputs included, 3D pictures and
    /// controls as their pictures), written by `mc_nbexport`.
    pub(super) fn export_document(&mut self, ext: &str) {
        let (name, filter) = if ext == "pdf" { ("PDF document", "pdf") } else { ("HTML page", "html") };
        let stem = self.path().and_then(|p| p.file_stem().map(|s| s.to_string_lossy().to_string()));
        let mut dialog = rfd::FileDialog::new().add_filter(name, &[filter]);
        if let Some(s) = &stem {
            dialog = dialog.set_file_name(format!("{s}.{ext}"));
        }
        let Some(p) = dialog.save_file() else { return };
        self.export_to(&p);
    }

    /// Export the notebook to `path` (`.html`/`.htm` or `.pdf`; also the self-test's `export` step).
    pub fn export_to(&mut self, path: &Path) -> bool {
        let start = std::time::Instant::now();
        let mut f = self.doc.nb.to_file();
        let base = self.doc.view.base.clone().unwrap_or_default();
        // 3D pictures turned in the window are exported as they show
        let turned = self.turned_pictures(&base);
        for line in f.cells.iter_mut().flat_map(|c| c.outputs.iter_mut()).flat_map(|b| b.lines.iter_mut()) {
            if let Some((_, new)) = turned.iter().find(|(old, _)| *old == line.text) {
                line.text = new.clone();
            }
        }
        let ext = path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
        let bytes = if ext == "pdf" {
            mc_nbexport::to_pdf(&f, &base)
        } else {
            mc_nbexport::to_html(&f, &base, path, &mc_nbexport::Options::default()).map(String::into_bytes)
        };
        match bytes.and_then(|b| std::fs::write(path, b)) {
            Ok(()) => {
                self.status = format!("Exported {} ({:.0} ms)", file_name(path), start.elapsed().as_secs_f64() * 1000.0);
                true
            }
            Err(e) => {
                self.error = Some(format!("Cannot export {}: {e}", path.display()));
                false
            }
        }
    }

    /// The 3D pictures that keep a view of their own, drawn from it into the pictures cache: (value, new value).
    fn turned_pictures(&mut self, base: &Path) -> Vec<(String, String)> {
        let mut out = vec![];
        for c in self.doc.nb.cells() {
            let Some(p) = c.graphic() else { continue };
            let path = mc_nbfile::images::resolve(&p, Some(base));
            let Some(scene) = self.rot.borrow_mut().scene(&path) else { continue };
            let Some(view) = crate::rotate3d::view_of(&c.out_extra, scene.view()) else { continue };
            let Ok(png) = scene.render_png(&view, 2.0, &|| false) else { continue };
            // two pixels a point: marked, so the export shows it at the picture's size
            let png = mc_nbfile::images::with_density(&png, 2);
            let dir = mc_nbfile::embed::cache_dir();
            let name = format!("view-{}-{}.png", std::process::id(), c.id);
            if std::fs::create_dir_all(&dir).and_then(|()| std::fs::write(dir.join(&name), &png)).is_ok() {
                out.push((c.text.clone(), mc_nbfile::images::image_value(&dir.join(&name).display().to_string())));
            }
        }
        out
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
