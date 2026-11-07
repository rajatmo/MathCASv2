//! Turning 3D pictures (round 13): a `graphics3d` or `plot3d` picture whose scene travels with it
//! (`plot-N.scene3d`, `mc_graphics::view3d`) is drawn again from another view point without evaluating anything.
//! Dragging the picture turns it, the wheel zooms it while it is selected, a double-click goes back to the view it
//! was made with; the view is kept on the output (`view=x:y:z zoom=z vertical=x:y:z`) and saved with the notebook.
//!
//! Rendering runs on a worker thread of its own: the window asks for the newest view of a cell and the worker
//! renders only the newest request of each cell (the ones overtaken while it was busy are dropped), so a fast drag
//! never queues frames. The scene is read once (`Scene3d::cached`).
use eframe::egui::{self, ColorImage, TextureHandle, TextureOptions};
use mc_graphics::theme::Theme;
use mc_graphics::view3d::{Scene3d, View3d};
use mc_nbfile::Attrs;
use mc_nbmodel::CellId;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;

/// The output attributes that hold a view.
pub const VIEW_ATTRS: [&str; 3] = ["view", "zoom", "vertical"];

/// A render request.
struct Job {
    id: CellId,
    scene: Arc<Scene3d>,
    view: View3d,
    scale: f32,
    /// a cheap render while the picture is being dragged (large scenes only)
    draft: bool,
    /// the chrome's colours (round nb-svg: the window's theme)
    theme: Option<Theme>,
}

/// A finished render.
struct Done {
    id: CellId,
    view: View3d,
    scale: f32,
    draft: bool,
    theme: u64,
    image: (u32, u32, Vec<u8>),
}

/// The rendered view of one cell.
struct Shown {
    tex: TextureHandle,
    view: View3d,
    scale: f32,
    draft: bool,
    theme: u64,
}

/// Requests for one cell closer together than this are a drag: large scenes are drawn as drafts meanwhile.
const DRAG_GAP: std::time::Duration = std::time::Duration::from_millis(250);

/// The 3D pictures of a window: their scenes, the views asked for and the textures rendered.
pub struct Rotations {
    ctx: egui::Context,
    jobs: Sender<Job>,
    done: Receiver<Done>,
    /// scenes by picture path (`None`: the picture has no scene)
    scenes: HashMap<PathBuf, Option<Arc<Scene3d>>>,
    shown: HashMap<CellId, Shown>,
    /// the newest view asked for per cell (with its scale and theme), not yet rendered
    asked: HashMap<CellId, (View3d, f32, u64)>,
    /// when each cell last asked for a new view
    last_ask: HashMap<CellId, std::time::Instant>,
    /// render times of the last frames (milliseconds), for the self-test
    pub times: Vec<f64>,
    times_rx: Receiver<f64>,
}

impl Rotations {
    /// A worker thread for the window.
    pub fn new(ctx: &egui::Context) -> Rotations {
        let (jobs, jobs_rx) = channel::<Job>();
        let (done_tx, done) = channel::<Done>();
        let (times_tx, times_rx) = channel::<f64>();
        let repaint = ctx.clone();
        std::thread::Builder::new()
            .name("rotate3d".into())
            .spawn(move || {
                while let Ok(first) = jobs_rx.recv() {
                    // only the newest request of each cell
                    let mut newest: Vec<Job> = vec![first];
                    while let Ok(j) = jobs_rx.try_recv() {
                        newest.retain(|o| o.id != j.id);
                        newest.push(j);
                    }
                    for j in newest {
                        let start = std::time::Instant::now();
                        let theme = j.theme.as_ref();
                        let image = if j.draft {
                            j.scene.render_draft_themed(&j.view, j.scale, theme)
                        } else {
                            j.scene.render_themed(&j.view, j.scale, theme)
                        };
                        let _ = times_tx.send(start.elapsed().as_secs_f64() * 1000.0);
                        let key = theme.map_or(0, Theme::key);
                        if done_tx.send(Done { id: j.id, view: j.view, scale: j.scale, draft: j.draft, theme: key, image }).is_err() {
                            return;
                        }
                        repaint.request_repaint();
                    }
                }
            })
            .expect("a thread for 3D pictures");
        Rotations {
            ctx: ctx.clone(),
            jobs,
            done,
            scenes: HashMap::new(),
            shown: HashMap::new(),
            asked: HashMap::new(),
            last_ask: HashMap::new(),
            times: vec![],
            times_rx,
        }
    }

    /// The scene of the picture at `png`, read once (`None`: a picture without one, a 2D plot).
    pub fn scene(&mut self, png: &Path) -> Option<Arc<Scene3d>> {
        if let Some(s) = self.scenes.get(png) {
            return s.clone();
        }
        let s = Scene3d::exists_for(png).then(|| Scene3d::cached(&Scene3d::sidecar_of(png)).ok()).flatten();
        self.scenes.insert(png.to_path_buf(), s.clone());
        s
    }

    /// Whether the picture at `png` can be turned.
    pub fn turns(&mut self, png: &Path) -> bool {
        self.scene(png).is_some()
    }

    /// Take the finished renders as textures.
    pub fn poll(&mut self) {
        while let Ok(t) = self.times_rx.try_recv() {
            self.times.push(t);
            if self.times.len() > 200 {
                self.times.remove(0);
            }
        }
        while let Ok(d) = self.done.try_recv() {
            let (w, h, rgba) = d.image;
            if w == 0 || h == 0 {
                continue;
            }
            let img = ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &rgba);
            let opts = TextureOptions::LINEAR;
            match self.shown.get_mut(&d.id) {
                Some(s) => {
                    s.tex.set(img, opts);
                    s.view = d.view;
                    s.scale = d.scale;
                    s.draft = d.draft;
                    s.theme = d.theme;
                }
                None => {
                    let tex = self.ctx.load_texture(format!("view3d-{}", d.id), img, opts);
                    self.shown.insert(d.id, Shown { tex, view: d.view, scale: d.scale, draft: d.draft, theme: d.theme });
                }
            }
            if self.asked.get(&d.id).is_some_and(|(v, s, t)| same(v, &d.view) && *s == d.scale && *t == d.theme) {
                self.asked.remove(&d.id);
            }
        }
    }

    /// The texture of cell `id` showing `view` at `scale` (device pixels per point of the picture's page) with
    /// its chrome in `theme`: the newest render, asking for the view when it differs (the older render shows
    /// meanwhile). `None` until a first render of the cell is there.
    pub fn texture(&mut self, id: CellId, png: &Path, view: View3d, scale: f32, theme: Option<Theme>) -> Option<TextureHandle> {
        let key = theme.as_ref().map_or(0, Theme::key);
        let current = self.shown.get(&id).is_some_and(|s| same(&s.view, &view) && (s.scale - scale).abs() < 1e-4 && s.theme == key);
        let asked = self.asked.get(&id).is_some_and(|(v, s, t)| same(v, &view) && (*s - scale).abs() < 1e-4 && *t == key);
        let now = std::time::Instant::now();
        let dragging = self.last_ask.get(&id).is_some_and(|t| now.duration_since(*t) < DRAG_GAP);
        if !current && !asked {
            if let Some(scene) = self.scene(png) {
                self.asked.insert(id, (view, scale, key));
                self.last_ask.insert(id, now);
                // a large scene while it is dragged: drafts, then the full picture once the drag stops
                let draft = scene.is_large() && dragging;
                if draft {
                    self.ctx.request_repaint_after(DRAG_GAP);
                }
                let _ = self.jobs.send(Job { id, scene, view, scale, draft, theme });
            }
        } else if current && !asked && !dragging && self.shown.get(&id).is_some_and(|s| s.draft) {
            if let Some(scene) = self.scene(png) {
                self.asked.insert(id, (view, scale, key));
                let _ = self.jobs.send(Job { id, scene, view, scale, draft: false, theme });
            }
        } else if current && self.shown.get(&id).is_some_and(|s| s.draft) {
            self.ctx.request_repaint_after(DRAG_GAP);
        }
        self.shown.get(&id).map(|s| s.tex.clone())
    }

    /// Forget the render of cell `id` (its view went back to the picture's own).
    pub fn forget(&mut self, id: CellId) {
        self.shown.remove(&id);
        self.asked.remove(&id);
        self.last_ask.remove(&id);
    }
}

fn same(a: &View3d, b: &View3d) -> bool {
    let close = |x: &[f64; 3], y: &[f64; 3]| x.iter().zip(y).all(|(p, q)| (p - q).abs() < 1e-9);
    close(&a.view_point, &b.view_point) && close(&a.view_vertical, &b.view_vertical) && (a.zoom - b.zoom).abs() < 1e-9
}

fn triple(s: &str) -> Option<[f64; 3]> {
    let v: Vec<f64> = s.split(':').map(|x| x.parse::<f64>().ok().filter(|x| x.is_finite())).collect::<Option<_>>()?;
    (v.len() == 3).then(|| [v[0], v[1], v[2]])
}

fn number(x: f64) -> String {
    let s = format!("{x:.4}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" {
        "0".into()
    } else {
        s.to_string()
    }
}

/// The view an output keeps (`view`, `zoom`, `vertical` on its block), over the picture's own view `base`; `None`
/// when it keeps none.
pub fn view_of(attrs: &Attrs, base: View3d) -> Option<View3d> {
    if !VIEW_ATTRS.iter().any(|a| attrs.has(a)) {
        return None;
    }
    let mut v = base;
    if let Some(p) = attrs.get("view").and_then(triple) {
        v.view_point = p;
    }
    if let Some(p) = attrs.get("vertical").and_then(triple) {
        v.view_vertical = p;
    }
    if let Some(z) = attrs.number::<f64>("zoom").filter(|z| z.is_finite() && *z > 0.0) {
        v.zoom = z;
    }
    Some(v)
}

/// The attributes that keep `view` (those equal to the picture's own view `base` left out).
pub fn attrs_of(view: &View3d, base: &View3d) -> Vec<(String, Option<String>)> {
    let t = |p: &[f64; 3]| format!("{}:{}:{}", number(p[0]), number(p[1]), number(p[2]));
    let differs = |a: &[f64; 3], b: &[f64; 3]| a.iter().zip(b).any(|(x, y)| (x - y).abs() > 1e-6);
    vec![
        ("view".into(), differs(&view.view_point, &base.view_point).then(|| t(&view.view_point))),
        ("zoom".into(), ((view.zoom - base.zoom).abs() > 1e-6).then(|| number(view.zoom))),
        ("vertical".into(), differs(&view.view_vertical, &base.view_vertical).then(|| t(&view.view_vertical))),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn views_are_kept_as_attributes_and_read_back() {
        let base = View3d { view_point: [1.3, -2.4, 2.0], view_vertical: [0.0, 0.0, 1.0], zoom: 1.0 };
        let mut attrs = Attrs::new();
        assert!(view_of(&attrs, base).is_none(), "no attributes: the picture's own view");
        let turned = View3d { view_point: [-0.5, 3.25, 1.0], view_vertical: [0.0, 0.0, 1.0], zoom: 1.21 };
        for (n, v) in attrs_of(&turned, &base) {
            if let Some(v) = v {
                attrs.set(&n, Some(v));
            }
        }
        assert_eq!(attrs.to_string(), " view=-0.5:3.25:1 zoom=1.21");
        let back = view_of(&attrs, base).unwrap();
        assert!(same(&back, &turned));
        // the attribute values are valid in the file
        let (parsed, bad) = Attrs::parse(&attrs.to_string());
        assert!(bad.is_empty());
        assert_eq!(parsed, attrs);
        // a malformed value is ignored
        let (odd, _) = Attrs::parse("view=1:2 zoom=-3");
        assert!(same(&view_of(&odd, base).unwrap(), &base));
    }
}
