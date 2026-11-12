//! The self-test: with `MCDOCS_SELFTEST=<script>` the window runs a script, one step per frame batch, and takes
//! screenshots. Steps (one per line, `#` comments):
//!
//! - `out <dir>`: where screenshots and the log go (default: the script's folder)
//! - `size <w> <h>`: the window's inner size; `theme light|dark|system`
//! - `go <place>`: `home`, `guide:<area>` (`core`, `data`, `visualisation`, `symbolic`, `higher`), `page:<name>`
//! - `search <text>`: type a query in the search box (results show incrementally); `find <text>`: find in page
//!   (`find` alone closes the find bar); `next`: the next match
//! - `expand`: open every collapsible part of the page; `scroll <points>`: scroll the content
//! - `back` / `forward`
//! - `ppp <x>`: emulate a display scale (1.5 = 150 %)
//! - `move <x> <y>`, `click <x> <y>`, `ctrlclick <x> <y>`, `mclick <x> <y>`: the pointer at window coordinates
//! - `instance`: take places sent by other launches (`mc-docs <name>`, F1 in the notebook), as a window outside a
//!   self-test does; `waitpage <name>`: wait (at most 20 s) until the window shows the page of `name`
//! - `hoverlink <name>` / `clicklink <name>`: the pointer onto (and a click on) the first link to `name` drawn in the
//!   last frame; `links`: log how many links the page drew and the one under the pointer
//! - `typesearch <text>`: type a query one character at a time (a search per character, as typing does) and log
//!   the median and largest search time; `copybutton <k>`: click the copy button of the `k`-th example input drawn
//!   (from 1); `clip`: log the clipboard's text (run with `MCUI_CLIPBOARD=local`)
//! - `place <name#anchor>`: go to a place as `mc-docs` and F1 name it (round 20: a page's part, an option's
//!   example); `clickvalue <k>`: click the `k`-th typeset value drawn (from 1); `copy`: the Copy key (Ctrl+C)
//! - `wait <frames>`: let frames pass; `shot <name>`: a screenshot `<name>.png`; `log <text>`; `quit`
use crate::app::DocsApp;
use eframe::egui::{self, ViewportCommand};
use std::io::Write;
use std::path::{Path, PathBuf};

/// A running self-test.
pub struct SelfTest {
    steps: Vec<String>,
    next: usize,
    wait: u32,
    dir: PathBuf,
    log: Vec<String>,
    start: std::time::Instant,
    /// since when a `waitpage` step waits (seconds since the start)
    waiting: Option<f64>,
    /// a pointer move due after some frames
    nudge: Option<(egui::Pos2, u32)>,
}

impl SelfTest {
    /// The self-test of the script at `path`.
    pub fn load(path: &str) -> Option<SelfTest> {
        let text = std::fs::read_to_string(path).ok()?;
        // editors and shells on Windows often start a UTF-8 file with a byte-order mark
        let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
        let dir = Path::new(path).parent().map(PathBuf::from).unwrap_or_default();
        let steps =
            text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')).map(str::to_string).collect();
        Some(SelfTest { steps, next: 0, wait: 5, dir, log: vec![], start: std::time::Instant::now(), waiting: None, nudge: None })
    }

    /// Run the steps due in this frame.
    pub fn step(&mut self, app: &mut DocsApp, ctx: &egui::Context) {
        ctx.request_repaint();
        match self.nudge {
            Some((at, 0)) => {
                app.events.push(egui::Event::PointerMoved(at));
                self.nudge = None;
            }
            Some((at, n)) => self.nudge = Some((at, n - 1)),
            None => {}
        }
        if self.wait > 0 {
            self.wait -= 1;
            return;
        }
        while self.next < self.steps.len() && self.wait == 0 {
            let line = self.steps[self.next].clone();
            self.next += 1;
            let (cmd, arg) = line.split_once(' ').map_or((line.as_str(), ""), |(c, a)| (c, a.trim()));
            // most steps change what is drawn: let two frames pass
            self.wait = 2;
            match cmd {
                "out" => {
                    self.dir = PathBuf::from(arg);
                    let _ = std::fs::create_dir_all(&self.dir);
                    self.wait = 0;
                }
                "size" => {
                    let v: Vec<f32> = arg.split(' ').filter_map(|s| s.parse().ok()).collect();
                    if let [w, h] = v[..] {
                        ctx.send_viewport_cmd(ViewportCommand::InnerSize(egui::vec2(w, h)));
                    }
                    self.wait = 6;
                }
                "theme" => app.set_theme(mc_notebook::theme::ThemeChoice::from_name(arg)),
                "go" => app.go(crate::view::Loc::parse(arg)),
                "place" => {
                    app.go_place(arg);
                    self.log.push(format!("place {arg}: target {:?}", app.target()));
                    self.wait = 8;
                }
                "clickvalue" => {
                    let k: usize = arg.parse().unwrap_or(1);
                    match app.value_rects.get(k.saturating_sub(1)).cloned() {
                        Some((r, text)) => {
                            self.log.push(format!("value {k} of {} is {text:?}", app.value_rects.len()));
                            pointer(app, "click", r.center());
                        }
                        None => self.log.push(format!("no value {k} ({} drawn)", app.value_rects.len())),
                    }
                    self.wait = 3;
                }
                "copy" => {
                    app.events.push(egui::Event::Copy);
                    self.wait = 3;
                }
                "search" => app.type_search(arg),
                "find" => app.set_find(arg),
                "next" => app.find_next(true),
                "expand" => app.expand_all(ctx),
                "scroll" => app.scroll_by(arg.parse().unwrap_or(400.0)),
                "back" => app.back(),
                "forward" => app.forward(),
                "ppp" => {
                    let want: f32 = arg.parse().unwrap_or(1.0);
                    let native = ctx.pixels_per_point() / ctx.zoom_factor();
                    ctx.set_zoom_factor(want / native);
                    self.wait = 6;
                }
                "move" | "click" | "ctrlclick" | "mclick" => {
                    let v: Vec<f32> = arg.split(' ').filter_map(|s| s.parse().ok()).collect();
                    if let [x, y] = v[..] {
                        pointer(app, cmd, egui::pos2(x, y));
                    }
                    self.wait = 3;
                }
                "hoverlink" | "clicklink" => {
                    match app.link_rects.iter().find(|(_, n)| n == arg).map(|(r, _)| r.center()) {
                        Some(at) => {
                            self.log.push(format!("link `{arg}` at {:.0}, {:.0}", at.x, at.y));
                            pointer(app, if cmd == "hoverlink" { "move" } else { "click" }, at);
                            // a second, small move a few frames later: egui shows a tooltip once the pointer moved
                            // over a widget it already hovers
                            self.nudge = Some((at + egui::vec2(1.0, 0.0), 4));
                        }
                        None => self.log.push(format!("no link to `{arg}` on the screen")),
                    }
                    self.wait = 3;
                }
                "typesearch" => {
                    let mut times = vec![];
                    let mut typed = String::new();
                    for ch in arg.chars() {
                        typed.push(ch);
                        app.type_search(&typed);
                        times.push(app.search_ms);
                    }
                    times.sort_by(f64::total_cmp);
                    if let (Some(mid), Some(max)) = (times.get(times.len() / 2), times.last()) {
                        self.log.push(format!("typesearch `{arg}`: {} searches, median {mid:.2} ms, max {max:.2} ms", times.len()));
                    }
                }
                "copybutton" => {
                    let k: usize = arg.parse().unwrap_or(1);
                    match app.copy_rects.get(k.saturating_sub(1)).cloned() {
                        Some((r, src)) => {
                            self.log.push(format!("copy button {k} of {} copies {src:?}", app.copy_rects.len()));
                            pointer(app, "click", r.center());
                        }
                        None => self.log.push(format!("no copy button {k} ({} drawn)", app.copy_rects.len())),
                    }
                    self.wait = 3;
                }
                "clip" => {
                    let t = app.clipboard_text(ctx);
                    self.log.push(format!("clipboard: {t:?}"));
                }
                "instance" => app.start_instance(ctx),
                "waitpage" => {
                    let want = crate::view::Loc::Page(arg.to_string());
                    let now = self.start.elapsed().as_secs_f64();
                    let since = *self.waiting.get_or_insert(now);
                    if *app.here() != want && now < since + 20.0 {
                        // try again next frame
                        self.next -= 1;
                        self.wait = 1;
                        continue;
                    }
                    self.waiting = None;
                }
                "links" => {
                    let mut names: Vec<&str> = app.link_rects.iter().map(|(_, n)| n.as_str()).collect();
                    names.sort_unstable();
                    names.dedup();
                    self.log.push(format!("links: {} drawn, {} names: {}", app.link_rects.len(), names.len(), names.join(" ")));
                }
                "wait" => self.wait = arg.parse().unwrap_or(10),
                "shot" => {
                    let path = self.dir.join(format!("{arg}.png"));
                    ctx.send_viewport_cmd(ViewportCommand::Screenshot(egui::ScreenshotCallback::new(move |img| {
                        save_png(&path, &img)
                    })));
                    self.log.push(format!("shot {arg}"));
                    self.wait = 4;
                }
                "log" => {
                    self.log.push(arg.to_string());
                    self.wait = 0;
                }
                "quit" => {
                    self.write_log();
                    ctx.send_viewport_cmd(ViewportCommand::Close);
                }
                other => self.log.push(format!("unknown step `{other}`")),
            }
            self.log.push(format!("{:7.2} s  {line}: {}", self.start.elapsed().as_secs_f64(), app.describe()));
            self.write_log();
        }
    }

    fn write_log(&self) {
        if let Ok(mut f) = std::fs::File::create(self.dir.join("selftest.log")) {
            for l in &self.log {
                let _ = writeln!(f, "{l}");
            }
        }
    }
}

/// The pointer moves to `at`, and with `click`, `ctrlclick` or `mclick` clicks there.
fn pointer(app: &mut DocsApp, what: &str, at: egui::Pos2) {
    app.events.push(egui::Event::PointerMoved(at));
    let (button, modifiers) = match what {
        "click" => (egui::PointerButton::Primary, egui::Modifiers::NONE),
        "ctrlclick" => (egui::PointerButton::Primary, egui::Modifiers::COMMAND),
        "mclick" => (egui::PointerButton::Middle, egui::Modifiers::NONE),
        _ => return,
    };
    for pressed in [true, false] {
        app.events.push(egui::Event::PointerButton { pos: at, button, pressed, modifiers });
    }
    if !modifiers.is_none() {
        app.events.push(egui::Event::ModifiersChanged(egui::Modifiers::NONE));
    }
}

fn save_png(path: &Path, img: &egui::ColorImage) {
    if let Ok(file) = std::fs::File::create(path) {
        let mut enc = png::Encoder::new(std::io::BufWriter::new(file), img.width() as u32, img.height() as u32);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        if let Ok(mut w) = enc.write_header() {
            let bytes: Vec<u8> = img.pixels.iter().flat_map(|c| c.to_array()).collect();
            let _ = w.write_image_data(&bytes);
        }
    }
}
