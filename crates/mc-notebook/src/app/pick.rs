//! The notebook's own file dialog, for systems without one a native program can call (Android): a modal card
//! listing the files of the notebooks folder (`mc_ui::dirs::documents_dir`), with a name field when saving.
//!
//! The system's dialogs block until they are answered; this one is answered in a later frame. So a command that
//! needs a file opens the dialog and ends (a Save reports "not saved" for now), and the dialog's answer does the
//! work: [`App::picked`].
use super::*;

/// What the chosen file is for.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Purpose {
    /// open notebooks
    Open,
    /// save the notebook under a name
    SaveAs,
    /// export as HTML (`html`) or PDF (`pdf`)
    Export(&'static str),
    /// export the code as a script
    Script,
}

impl Purpose {
    fn extension(self) -> &'static str {
        match self {
            Purpose::Open | Purpose::SaveAs => "mcn",
            Purpose::Export(ext) => ext,
            Purpose::Script => "mc",
        }
    }

    fn title(self) -> &'static str {
        match self {
            Purpose::Open => "Open notebook",
            Purpose::SaveAs => "Save notebook as",
            Purpose::Export("pdf") => "Export PDF document",
            Purpose::Export(_) => "Export HTML page",
            Purpose::Script => "Export script",
        }
    }
}

/// An open file dialog.
pub(super) struct Pick {
    purpose: Purpose,
    /// the folder shown
    dir: PathBuf,
    /// the name typed (saving)
    name: String,
    /// the folder's files of the purpose's kind, by name
    files: Vec<String>,
    /// the name field takes the keyboard when the dialog opens
    focus: bool,
}

/// Whether the notebook shows its own file dialogs: on Android, and anywhere with `MCNB_OWN_FILE_DIALOGS` set.
pub(super) fn own() -> bool {
    cfg!(target_os = "android") || std::env::var_os("MCNB_OWN_FILE_DIALOGS").is_some()
}

/// The files of `dir` with the extension `ext`, sorted by name without regard to case.
fn list(dir: &Path, ext: &str) -> Vec<String> {
    let mut files: Vec<String> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| Path::new(n).extension().is_some_and(|e| e.eq_ignore_ascii_case(ext)))
        .collect();
    files.sort_by_key(|n| n.to_lowercase());
    files
}

/// The file name a typed `name` stands for: no folders, the extension `ext` added when it is missing; `None` for an
/// empty name.
pub(super) fn file_name_of(name: &str, ext: &str) -> Option<String> {
    let name = name.trim().rsplit(['/', '\\']).next().unwrap_or("").trim().trim_start_matches('.');
    if name.is_empty() {
        return None;
    }
    let has = Path::new(name).extension().is_some_and(|e| e.eq_ignore_ascii_case(ext));
    Some(if has { name.to_string() } else { format!("{name}.{ext}") })
}

impl App {
    /// Open the dialog for `purpose`, in the folder of the active notebook (else the notebooks folder), with `name`
    /// in the name field.
    pub(super) fn pick(&mut self, purpose: Purpose, name: Option<String>) {
        let dir = self
            .path()
            .and_then(|p| p.parent().map(Path::to_path_buf))
            .filter(|d| d.is_dir())
            .or_else(mc_ui::dirs::documents_dir)
            .unwrap_or_else(|| PathBuf::from("."));
        let _ = std::fs::create_dir_all(&dir);
        let files = list(&dir, purpose.extension());
        self.pick = Some(Pick { purpose, dir, name: name.unwrap_or_default(), files, focus: true });
    }

    /// The file dialog, when it is open.
    pub(super) fn pick_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut pick) = self.pick.take() else { return };
        let ext = pick.purpose.extension();
        let saving = pick.purpose != Purpose::Open;
        let weak = self.doc.view.palette.weak;
        // a phone's screen is narrower than the dialogs of a desktop window
        let width = (ctx.content_rect().width() - 76.0).clamp(180.0, 420.0);
        let mut chosen: Option<String> = None;
        let mut close = false;
        let shown = dialog::show(ctx, "pick", pick.purpose.title(), width, true, |ui| {
            ui.add(egui::Label::new(RichText::new(pick.dir.display().to_string()).small().color(weak)).wrap());
            ui.add_space(8.0);
            if saving {
                let edit = ui.add(
                    egui::TextEdit::singleline(&mut pick.name).hint_text("file name").desired_width(f32::INFINITY),
                );
                if pick.focus {
                    edit.request_focus();
                    pick.focus = false;
                }
                ui.add_space(8.0);
            }
            if pick.files.is_empty() {
                let none = if saving { "No files of this kind here yet." } else { "No notebooks here yet." };
                ui.label(RichText::new(none).color(weak));
            }
            ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
                for f in &pick.files {
                    let b = egui::Button::selectable(saving && pick.name == *f, f.as_str())
                        .min_size(egui::vec2(ui.available_width(), 32.0));
                    if ui.add(b).clicked() {
                        if saving {
                            pick.name = f.clone();
                        } else {
                            chosen = Some(f.clone());
                        }
                    }
                }
            });
            let target = file_name_of(&pick.name, ext);
            dialog::buttons(ui, |ui| {
                if saving {
                    let exists = target.as_ref().is_some_and(|t| pick.dir.join(t).exists());
                    let label = if exists { "Replace" } else { "Save" };
                    let enter = ui.input(|i| i.key_pressed(Key::Enter));
                    if (dialog::primary(ui, label).clicked() || enter) && target.is_some() {
                        chosen = target.clone();
                    }
                }
                if dialog::button(ui, "Cancel").clicked() || ui.input(|i| i.key_pressed(Key::Escape)) {
                    close = true;
                }
            });
        });
        match chosen {
            Some(name) => self.picked(pick.purpose, &pick.dir.join(name)),
            None if close || shown.closed => {}
            None => self.pick = Some(pick),
        }
    }

    /// Do what the dialog was opened for with the chosen file.
    fn picked(&mut self, purpose: Purpose, path: &Path) {
        match purpose {
            Purpose::Open => self.open(path),
            Purpose::SaveAs => {
                self.save_to(path);
            }
            Purpose::Export(_) => {
                self.export_to(path);
            }
            Purpose::Script => self.export_script_to(path),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert_eq!(file_name_of("  notes ", "mcn"), Some("notes.mcn".into()));
        assert_eq!(file_name_of("notes.MCN", "mcn"), Some("notes.MCN".into()));
        assert_eq!(file_name_of("a/b\\c.pdf", "pdf"), Some("c.pdf".into()));
        assert_eq!(file_name_of("../..", "mcn"), None);
        assert_eq!(file_name_of("   ", "mcn"), None);
        assert_eq!(file_name_of("report.v2", "html"), Some("report.v2.html".into()));
    }

    #[test]
    fn lists_only_the_kind_asked_for() {
        let dir = std::env::temp_dir().join(format!("mcnb-pick-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("folder.mcn")).unwrap();
        for f in ["b.mcn", "A.MCN", "c.mc", "d.txt"] {
            std::fs::write(dir.join(f), "").unwrap();
        }
        assert_eq!(list(&dir, "mcn"), vec!["A.MCN".to_string(), "b.mcn".to_string()]);
        assert_eq!(list(&dir, "mc"), vec!["c.mc".to_string()]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
