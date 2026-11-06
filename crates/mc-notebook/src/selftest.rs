//! The self-test (design 9.8): with `MCNB_SELFTEST=<script>` the window runs a script that injects input through
//! egui's raw input hook, takes screenshots and measures frame times. Script lines (one step per line, `#` comments):
//!
//! - `out <dir>`: where screenshots and the log go (default: the script's folder)
//! - `open <file>`: open a notebook in a tab (as File > Open); `new`: replace the active tab's notebook by an empty
//!   one; `newtab`: a new Untitled tab
//! - `ppp <x>`: emulate a display scale (1 = 100 %, 1.5 = 150 %)
//! - `rclick <x> <y>` / `mclick <x> <y>` / `shiftclick <x> <y>` / `tripleclick <x> <y>`; `drag <x0> <y0> <x1> <y1>`
//!   (a primary drag in steps); `move <x> <y>` (the pointer moves; the log gets the pointer shape a frame later)
//! - `generate <n>`: load a generated notebook of `n` cells of mixed styles (the performance probe)
//! - `theme light|dark|system`, `zoom <factor>`, `size <w> <h>`
//! - `keys <keys>`: chords and quoted text as in the key scripts, sent as egui key and text events
//! - `click <x> <y>` / `dblclick <x> <y>`: a primary click at window coordinates
//! - `wait <frames>`: let frames pass (kernel responses arrive)
//! - `shot <name>`: a screenshot `<name>.png`
//! - `probe <label>`: log the frame time of the next frames
//! - `scroll <pixels>`: scroll the notebook
//! - `ime <text>` / `imecommit <text>`: an input method composes / commits text; `tabs`: write the tabs to the log
//! - `sel`: write the selection to the log; `hit <x> <y>`: what is at a window position; `images`: the picture
//!   cache's size; `about` / `about off`: the About dialog; `prefs` / `prefs off`: the preferences dialog; `clip`:
//!   write the clipboard's text to the log; `focus`: the focused widget and whether a popup is open
//! - `save <file>`: save the notebook there; `log <text>`: write a line to the log; `dump <name>`: write the
//!   notebook text to `<name>.mcn`
//! - `init yes|no|always`: answer the initialization question
//! - `quit`
use crate::app::App;
use crate::theme::ThemeChoice;
use eframe::egui::{self, Event, Modifiers, PointerButton, ViewportCommand};
use mc_nbmodel::keys::{Chord, Key};
use mc_nbmodel::script::{parse_steps, Step};
use mc_nbmodel::{Cell, CellId, Notebook, Style};
use std::io::Write;
use std::path::PathBuf;

/// A running self-test.
pub struct SelfTest {
    steps: Vec<String>,
    next: usize,
    wait: u32,
    dir: PathBuf,
    log: Vec<String>,
    probe: Option<(String, Vec<f64>, u32)>,
    pending: Vec<Event>,
    /// events for the following frames, one batch per frame
    later: Vec<Vec<Event>>,
    /// log the pointer shape after this frame (the `move` step)
    cursor_log: Option<String>,
    frame: u64,
}

impl SelfTest {
    /// The self-test of the script at `path`.
    pub fn load(path: &str) -> Option<SelfTest> {
        let text = std::fs::read_to_string(path).ok()?;
        let dir = std::path::Path::new(path).parent().map(PathBuf::from).unwrap_or_default();
        let steps =
            text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')).map(str::to_string).collect();
        Some(SelfTest {
            steps,
            next: 0,
            wait: 3,
            dir,
            log: vec![],
            probe: None,
            pending: vec![],
            later: vec![],
            cursor_log: None,
            frame: 0,
        })
    }

    fn write_log(&self) {
        if let Ok(mut f) = std::fs::File::create(self.dir.join("selftest.log")) {
            for l in &self.log {
                let _ = writeln!(f, "{l}");
            }
        }
    }

    /// Put the pending events into this frame's input.
    pub fn inject(&mut self, _app: &mut App, _ctx: &egui::Context, raw: &mut egui::RawInput) {
        if self.pending.is_empty() && !self.later.is_empty() {
            self.pending = self.later.remove(0);
        }
        for e in self.pending.drain(..) {
            if let Event::Key { modifiers, pressed: true, .. } = &e {
                raw.events.push(Event::ModifiersChanged(*modifiers));
            }
            raw.events.push(e);
        }
    }

    /// Run the next step after a frame.
    pub fn after_frame(&mut self, app: &mut App, ctx: &egui::Context) {
        self.frame += 1;
        ctx.request_repaint();
        if let Some(at) = self.cursor_log.take() {
            let icon = ctx.output(|o| o.cursor_icon);
            self.log.push(format!("pointer at {at}: {icon:?}"));
        }
        if let Some((label, times, left)) = &mut self.probe {
            times.push(app.frame_ms);
            *left -= 1;
            if *left == 0 {
                let mut t = times.clone();
                t.sort_by(|a, b| a.total_cmp(b));
                let line = format!(
                    "probe {label}: {} frames, median {:.2} ms, p90 {:.2} ms, max {:.2} ms",
                    t.len(),
                    t[t.len() / 2],
                    t[(t.len() * 9 / 10).min(t.len() - 1)],
                    t[t.len() - 1]
                );
                self.log.push(line);
                self.probe = None;
                self.write_log();
            }
        }
        if self.wait > 0 {
            self.wait -= 1;
            return;
        }
        let Some(step) = self.steps.get(self.next).cloned() else {
            return;
        };
        self.next += 1;
        self.wait = 2;
        let (cmd, arg) = step.split_once(' ').unwrap_or((step.as_str(), ""));
        match cmd {
            "out" => {
                self.dir = PathBuf::from(arg);
                let _ = std::fs::create_dir_all(&self.dir);
            }
            "open" => {
                let start = std::time::Instant::now();
                app.open(std::path::Path::new(arg));
                self.log.push(format!("open {arg}: {:.1} ms", start.elapsed().as_secs_f64() * 1000.0));
            }
            "new" => app.load_notebook(Notebook::new()),
            "newtab" => app.new_tab(),
            "ppp" => {
                let want: f32 = arg.parse().unwrap_or(1.0);
                let native = ctx.pixels_per_point() / ctx.zoom_factor();
                ctx.set_zoom_factor(want / native);
                self.wait = 6;
            }
            "move" => {
                let v: Vec<f32> = arg.split(' ').filter_map(|x| x.parse().ok()).collect();
                if v.len() == 2 {
                    self.pending.push(Event::PointerMoved(egui::pos2(v[0], v[1])));
                    self.cursor_log = Some(arg.to_string());
                }
            }
            "rclick" | "mclick" | "shiftclick" | "tripleclick" => {
                let v: Vec<f32> = arg.split(' ').filter_map(|x| x.parse().ok()).collect();
                if v.len() == 2 {
                    let p = egui::pos2(v[0], v[1]);
                    let (button, modifiers, n) = match cmd {
                        "rclick" => (PointerButton::Secondary, Modifiers::NONE, 1),
                        "mclick" => (PointerButton::Middle, Modifiers::NONE, 1),
                        "shiftclick" => (PointerButton::Primary, Modifiers::SHIFT, 1),
                        _ => (PointerButton::Primary, Modifiers::NONE, 3),
                    };
                    self.pending.push(Event::PointerMoved(p));
                    if modifiers.shift {
                        self.pending.push(Event::ModifiersChanged(modifiers));
                    }
                    for _ in 0..n {
                        for pressed in [true, false] {
                            self.pending.push(Event::PointerButton { pos: p, button, pressed, modifiers });
                        }
                    }
                    if modifiers.shift {
                        self.pending.push(Event::ModifiersChanged(Modifiers::NONE));
                    }
                }
            }
            "drag" => {
                let v: Vec<f32> = arg.split(' ').filter_map(|x| x.parse().ok()).collect();
                if v.len() == 4 {
                    let (a, b) = (egui::pos2(v[0], v[1]), egui::pos2(v[2], v[3]));
                    self.pending.push(Event::PointerMoved(a));
                    self.pending.push(Event::PointerButton {
                        pos: a,
                        button: PointerButton::Primary,
                        pressed: true,
                        modifiers: Modifiers::NONE,
                    });
                    // one move per frame, then the release
                    for k in 1..=8 {
                        self.later.push(vec![Event::PointerMoved(a + (b - a) * (k as f32 / 8.0))]);
                    }
                    self.later.push(vec![Event::PointerButton {
                        pos: b,
                        button: PointerButton::Primary,
                        pressed: false,
                        modifiers: Modifiers::NONE,
                    }]);
                    self.wait = 12;
                }
            }
            "generate" => {
                let n: usize = arg.parse().unwrap_or(1000);
                let start = std::time::Instant::now();
                app.load_notebook(generated(n));
                self.log.push(format!("generate {n}: {:.1} ms", start.elapsed().as_secs_f64() * 1000.0));
            }
            "theme" => app.set_theme(ThemeChoice::from_name(arg)),
            "zoom" => app.set_magnification(arg.parse().unwrap_or(1.0)),
            "size" => {
                let v: Vec<f32> = arg.split(' ').filter_map(|x| x.parse().ok()).collect();
                if v.len() == 2 {
                    ctx.send_viewport_cmd(ViewportCommand::InnerSize(egui::vec2(v[0], v[1])));
                }
                self.wait = 6;
            }
            "keys" => match parse_steps(arg) {
                Ok(steps) => {
                    for s in steps {
                        match s {
                            Step::Key(c) => self.pending.extend(key_events(c)),
                            Step::Type(t) => {
                                for ch in t.chars() {
                                    if ch == '\n' {
                                        self.pending.extend(key_events(Chord::parse("Enter").expect("valid")));
                                    } else {
                                        self.pending.push(Event::Text(ch.to_string()));
                                    }
                                }
                            }
                            _ => self.log.push(format!("keys: unsupported step {s:?}")),
                        }
                    }
                }
                Err(e) => self.log.push(format!("keys: {e}")),
            },
            "click" | "dblclick" => {
                let v: Vec<f32> = arg.split(' ').filter_map(|x| x.parse().ok()).collect();
                if v.len() == 2 {
                    let p = egui::pos2(v[0], v[1]);
                    self.pending.push(Event::PointerMoved(p));
                    for _ in 0..if cmd == "click" { 1 } else { 2 } {
                        for pressed in [true, false] {
                            self.pending.push(Event::PointerButton {
                                pos: p,
                                button: PointerButton::Primary,
                                pressed,
                                modifiers: Modifiers::NONE,
                            });
                        }
                    }
                }
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
            "probe" => self.probe = Some((arg.to_string(), vec![], 30)),
            "scroll" => {
                app.scroll(arg.parse().unwrap_or(400.0));
            }
            "save" => {
                let start = std::time::Instant::now();
                let ok = app.save_path(std::path::Path::new(arg));
                self.log.push(format!("save {arg}: {ok}, {:.1} ms", start.elapsed().as_secs_f64() * 1000.0));
            }
            "log" => self.log.push(arg.to_string()),
            "ime" => self
                .pending
                .push(Event::Ime(egui::ImeEvent::Preedit { text: arg.to_string(), active_range_chars: None })),
            "imecommit" => self.pending.push(Event::Ime(egui::ImeEvent::Commit(arg.to_string()))),
            "tabs" => {
                let list: Vec<String> = app.tabs_summary();
                self.log.push(format!("tabs: {}", list.join(" ")));
            }
            "about" => app.show_about(arg != "off"),
            "prefs" => app.show_preferences(arg != "off"),
            "clip" => self.log.push(format!("clipboard: {:?}", app.clipboard_text())),
            "focus" => {
                let (focused, popup) = (ctx.memory(|m| m.focused()), egui::Popup::is_any_open(ctx));
                self.log.push(format!("focus: {focused:?}, popup open: {popup}"));
            }
            "sel" => self.log.push(format!("selection: {:?}", app.notebook().selection())),
            "hit" => {
                let v: Vec<f32> = arg.split(' ').filter_map(|x| x.parse().ok()).collect();
                if v.len() == 2 {
                    let what = app.hit_at(egui::pos2(v[0], v[1]));
                    self.log.push(format!("hit {arg}: {what}"));
                }
            }
            "images" => {
                let (bytes, entries, uploads) = app.images.borrow().stats();
                self.log.push(format!("images: {} MB in {entries} entries, {uploads} uploads", bytes >> 20));
            }
            "init" => app.answer_init(
                match arg {
                    "no" => mc_nbmodel::InitAnswer::No,
                    "always" => mc_nbmodel::InitAnswer::Always,
                    _ => mc_nbmodel::InitAnswer::Yes,
                },
                ctx,
            ),
            "dump" => {
                let text = app.notebook().to_text();
                let _ = std::fs::write(self.dir.join(format!("{arg}.mcn")), text);
            }
            "quit" => {
                self.write_log();
                app.discard_changes();
                ctx.send_viewport_cmd(ViewportCommand::Close);
            }
            other => self.log.push(format!("unknown step `{other}`")),
        }
        self.write_log();
    }
}

/// The egui events of a chord: the key press with its modifiers.
fn key_events(c: Chord) -> Vec<Event> {
    use egui::Key as E;
    let key = match c.key {
        Key::Enter | Key::KeypadEnter => E::Enter,
        Key::Tab => E::Tab,
        Key::Backspace => E::Backspace,
        Key::Delete => E::Delete,
        Key::Insert => E::Insert,
        Key::Escape => E::Escape,
        Key::Space => E::Space,
        Key::Left => E::ArrowLeft,
        Key::Right => E::ArrowRight,
        Key::Up => E::ArrowUp,
        Key::Down => E::ArrowDown,
        Key::Home => E::Home,
        Key::End => E::End,
        Key::PageUp => E::PageUp,
        Key::PageDown => E::PageDown,
        Key::KeypadPeriod => E::Period,
        Key::F(n) => match E::from_name(&format!("F{n}")) {
            Some(k) => k,
            None => return vec![],
        },
        Key::Char(ch) => match ch {
            '[' => E::OpenBracket,
            ']' => E::CloseBracket,
            '\'' => E::Quote,
            '.' => E::Period,
            ',' => E::Comma,
            '/' => E::Slash,
            '-' => E::Minus,
            '=' => E::Equals,
            ';' => E::Semicolon,
            _ => match E::from_name(&ch.to_ascii_uppercase().to_string()) {
                Some(k) => k,
                None => return vec![],
            },
        },
    };
    let modifiers =
        Modifiers { alt: c.mods.alt, ctrl: c.mods.ctrl, shift: c.mods.shift, mac_cmd: false, command: c.mods.ctrl };
    vec![
        Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers },
        Event::Key { key, physical_key: None, pressed: false, repeat: false, modifiers },
    ]
}

/// A notebook of `n` cells of mixed styles, for the performance probe.
pub fn generated(n: usize) -> Notebook {
    let mut cells = vec![];
    let mut k = 0u64;
    while cells.len() < n {
        let id = CellId(k);
        let cell = match k % 10 {
            0 => Cell::new(id, Style::Section, &format!("Section {}", k / 10)),
            1 | 5 => Cell::new(id, Style::Text, "Some prose explaining the computation below, long enough to wrap onto a second line when the window is narrow."),
            2 | 6 | 8 => Cell::new(id, Style::Input, &format!("f{k}(x_) := x^2 + {k}\nsum(f{k}(i), [i, 1, 100])")),
            3 | 7 | 9 => {
                let mut c = Cell::new(id, Style::Output, &format!("{}", k * k + 338_350));
                c.label = Some(k as u32);
                c
            }
            _ => Cell::new(id, Style::Subsection, "Subsection"),
        };
        cells.push(cell);
        k += 1;
    }
    Notebook::with_cells(cells)
}

fn save_png(path: &std::path::Path, img: &egui::ColorImage) {
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
