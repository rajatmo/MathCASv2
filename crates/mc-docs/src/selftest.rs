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
}

impl SelfTest {
    /// The self-test of the script at `path`.
    pub fn load(path: &str) -> Option<SelfTest> {
        let text = std::fs::read_to_string(path).ok()?;
        let dir = Path::new(path).parent().map(PathBuf::from).unwrap_or_default();
        let steps =
            text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')).map(str::to_string).collect();
        Some(SelfTest { steps, next: 0, wait: 5, dir, log: vec![], start: std::time::Instant::now() })
    }

    /// Run the steps due in this frame.
    pub fn step(&mut self, app: &mut DocsApp, ctx: &egui::Context) {
        ctx.request_repaint();
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
                "search" => app.type_search(arg),
                "find" => app.set_find(arg),
                "next" => app.find_next(true),
                "expand" => app.expand_all(ctx),
                "scroll" => app.scroll_by(arg.parse().unwrap_or(400.0)),
                "back" => app.back(),
                "forward" => app.forward(),
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
