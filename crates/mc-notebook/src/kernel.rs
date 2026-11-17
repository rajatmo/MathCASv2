//! The kernel process (design 5.1): `mc-kernel` started on the first evaluation with pipes, a reader thread turning
//! JSON lines into [`KernelMsg`]s, stderr kept for the kernel log. Every `eval` asks for a value per statement and
//! numbered statements (design 5.2); a kernel without these additions ignores the fields and answers as protocol 1
//! always did, which the model handles.
use eframe::egui;
use mc_nbmodel::{KernelLink, KernelMsg, KernelRequest};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// The files of a picture the notebook reads as it shows it (the kernel's `pictures` capability): the PNG and the
/// SVG it is drawn from, the data description (hover) and the 3D scene (turning). The column twin (the plot laid
/// out for the PDF export's text column, kept in the saved notebook) is asked for when the notebook is saved or
/// exported, and before its kernel stops ([`KernelProcess::write_column_twins`]); the PDF and the print twins are
/// written when an export in the kernel reads them.
const PICTURES: [&str; 4] = ["png", "svg", "json", "scene3d"];
/// The column twin's extension.
const COLUMN_TWIN: &str = "column.svg";
/// The `id` prefix of the notebook's `picture` requests.
const PICTURE_ID: &str = "picture-";

/// How long a kernel asked to shut down may take before it is killed (design 5.6).
const SHUTDOWN_GRACE: Duration = Duration::from_secs(2);

/// The kernel log: the lines the kernel wrote to stderr, and the notebook's notes about starting and stopping it.
pub type Log = Arc<Mutex<String>>;

/// A link to one kernel process (one per notebook window, design NB.3).
pub struct KernelProcess {
    /// the kernel executable
    pub exe: PathBuf,
    /// the working directory (the notebook's folder)
    pub dir: Option<PathBuf>,
    /// the notebook's path, sent as `origin`
    pub origin: Option<String>,
    /// threads for the kernel (`--threads`)
    pub threads: Option<usize>,
    /// what the kernel wrote to stderr
    pub log: Log,
    child: Option<Arc<Mutex<Child>>>,
    /// requests for the kernel, written by a thread of their own: a busy kernel that does not read its input must
    /// never block the window
    stdin: Option<Sender<String>>,
    rx: Receiver<KernelMsg>,
    tx: Sender<KernelMsg>,
    names_rx: Receiver<Vec<(String, String)>>,
    names_tx: Sender<Vec<(String, String)>>,
    ctx: egui::Context,
    /// the generation of the running process: messages of an older one are dropped
    generation: Arc<Mutex<u64>>,
    /// the pictures the running process made (the paths its values and displays name)
    made: Arc<Mutex<Vec<PathBuf>>>,
    /// the ids of the `picture` requests the process answered
    pictures_rx: Receiver<String>,
    pictures_tx: Sender<String>,
    /// the number of `picture` requests sent
    picture_requests: u64,
}

/// The kernel executable: `MATHCAS_KERNEL`, else `mc-kernel` next to this program. On Android an app may run
/// programs only from the folder of its native libraries, so the package carries the kernel there under a
/// library's name, `libmc_kernel.so`.
pub fn default_kernel() -> PathBuf {
    if let Some(p) = std::env::var_os("MATHCAS_KERNEL") {
        return PathBuf::from(p);
    }
    #[cfg(target_os = "android")]
    if let Some(dir) = mc_ui::android::native_library_dir("libmc_notebook_android.so") {
        return dir.join("libmc_kernel.so");
    }
    let name = if cfg!(windows) { "mc-kernel.exe" } else { "mc-kernel" };
    std::env::current_exe().ok().and_then(|e| e.parent().map(|d| d.join(name))).unwrap_or_else(|| PathBuf::from(name))
}

impl KernelProcess {
    /// A link that starts `exe` when the first request is sent.
    pub fn new(exe: PathBuf, ctx: egui::Context) -> KernelProcess {
        let (tx, rx) = channel();
        let (names_tx, names_rx) = channel();
        let (pictures_tx, pictures_rx) = channel();
        KernelProcess {
            made: Arc::new(Mutex::new(vec![])),
            pictures_rx,
            pictures_tx,
            picture_requests: 0,
            names_tx,
            names_rx,
            exe,
            dir: None,
            origin: None,
            threads: None,
            log: Arc::new(Mutex::new(String::new())),
            child: None,
            stdin: None,
            rx,
            tx,
            ctx,
            generation: Arc::new(Mutex::new(0)),
        }
    }

    fn note(&self, s: &str) {
        if let Ok(mut l) = self.log.lock() {
            l.push_str(s);
            l.push('\n');
        }
    }

    fn start(&mut self) -> Result<(), String> {
        let mut cmd = Command::new(&self.exe);
        cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
        if let Some(t) = self.threads {
            cmd.arg("--threads").arg(t.to_string());
        }
        if let Some(d) = &self.dir {
            cmd.current_dir(d);
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            // no console window for the kernel
            cmd.creation_flags(0x0800_0000);
        }
        let mut child = cmd.spawn().map_err(|e| format!("cannot start {}: {e}", self.exe.display()))?;
        self.note(&format!("[started {} (pid {})]", self.exe.display(), child.id()));
        let stdout = child.stdout.take().expect("piped");
        let stderr = child.stderr.take().expect("piped");
        let mut pipe = child.stdin.take().expect("piped");
        let (lines, queue) = channel::<String>();
        let log_w = self.log.clone();
        std::thread::spawn(move || {
            for line in queue {
                if pipe.write_all(line.as_bytes()).and_then(|()| pipe.flush()).is_err() {
                    if let Ok(mut l) = log_w.lock() {
                        l.push_str("[could not write to the kernel]\n");
                    }
                    return;
                }
            }
        });
        self.stdin = Some(lines);
        let generation = {
            let mut g = self.generation.lock().expect("not poisoned");
            *g += 1;
            *g
        };
        let (tx, ctx, current) = (self.tx.clone(), self.ctx.clone(), self.generation.clone());
        let names_tx = self.names_tx.clone();
        let pictures_tx = self.pictures_tx.clone();
        let made = self.made.clone();
        if let Ok(mut m) = made.lock() {
            m.clear();
        }
        std::thread::spawn(move || {
            let mine = || current.lock().map(|g| *g == generation).unwrap_or(false);
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                if let Some(names) = decode_names(&line) {
                    let _ = names_tx.send(names);
                    ctx.request_repaint();
                    continue;
                }
                if let Some(id) = decode_picture_done(&line) {
                    let _ = pictures_tx.send(id);
                    continue;
                }
                let paths = picture_paths(&line);
                if !paths.is_empty() && mine() {
                    if let Ok(mut m) = made.lock() {
                        m.extend(paths);
                    }
                }
                if let Some(m) = decode(&line) {
                    if !mine() || tx.send(m).is_err() {
                        return;
                    }
                    ctx.request_repaint();
                }
            }
            if mine() {
                let _ = tx.send(KernelMsg::Exited { why: "the kernel process ended".into() });
                ctx.request_repaint();
            }
        });
        let log = self.log.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(stderr).lines() {
                let Ok(line) = line else { break };
                if let Ok(mut l) = log.lock() {
                    l.push_str(&line);
                    l.push('\n');
                }
            }
        });
        self.child = Some(Arc::new(Mutex::new(child)));
        Ok(())
    }

    fn write(&mut self, v: Value) {
        let Some(stdin) = self.stdin.as_ref() else { return };
        if stdin.send(format!("{v}\n")).is_err() {
            self.note("[could not write to the kernel]");
        }
    }

    /// Stop the process: `shutdown`, then a kill after a grace period. Its messages are no longer delivered. The
    /// column twins of its pictures that are not written yet are asked for first (they would be lost with it).
    pub fn stop(&mut self) {
        if self.stdin.is_some() {
            let made: Vec<PathBuf> = self.made.lock().map(|m| m.clone()).unwrap_or_default();
            self.request_column_twins(&made);
        }
        if let Ok(mut g) = self.generation.lock() {
            *g += 1;
        }
        self.write(json!({"type": "shutdown", "interrupt": true}));
        self.stdin = None;
        if let Some(child) = self.child.take() {
            self.note("[stopped]");
            std::thread::spawn(move || {
                let deadline = std::time::Instant::now() + SHUTDOWN_GRACE;
                loop {
                    let done =
                        child.lock().map(|mut c| c.try_wait().map(|s| s.is_some()).unwrap_or(true)).unwrap_or(true);
                    if done {
                        return;
                    }
                    if std::time::Instant::now() >= deadline {
                        if let Ok(mut c) = child.lock() {
                            let _ = c.kill();
                            let _ = c.wait();
                        }
                        return;
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
            });
        }
    }
}

impl KernelProcess {
    /// Ask for the defined names starting with `prefix` (the kernel starts if needed); the answer comes through
    /// [`KernelProcess::names`].
    pub fn request_names(&mut self, prefix: &str) {
        if self.child.is_none() && self.start().is_err() {
            return;
        }
        self.write(json!({"type": "names", "id": "names", "prefix": prefix}));
    }

    /// Send `picture` requests for the column twins of `pictures` (image paths) that are not on disk; the ids sent.
    fn request_column_twins(&mut self, pictures: &[PathBuf]) -> Vec<String> {
        let mut ids = vec![];
        let mut seen = std::collections::HashSet::new();
        for p in pictures {
            let png = p.with_extension("png");
            if !seen.insert(png.clone()) || png.with_extension(COLUMN_TWIN).is_file() {
                continue;
            }
            self.picture_requests += 1;
            let id = format!("{PICTURE_ID}{}", self.picture_requests);
            self.write(json!({"type": "picture", "id": id, "path": png.display().to_string(), "formats": [COLUMN_TWIN]}));
            ids.push(id);
        }
        ids
    }

    /// Have the running kernel write the column twins of `pictures` (image paths of its values) that wait in it,
    /// waiting for its answers at most `wait` (a busy kernel answers when its evaluation ends: the notebook then
    /// saves without the twins it has not got). The number of requests answered.
    pub fn write_column_twins(&mut self, pictures: &[PathBuf], wait: Duration) -> usize {
        if self.stdin.is_none() {
            return 0;
        }
        // answers to earlier requests that came too late
        while self.pictures_rx.try_recv().is_ok() {}
        // only pictures of this process wait in it
        let made: std::collections::HashSet<PathBuf> =
            self.made.lock().map(|m| m.iter().map(|p| p.with_extension("png")).collect()).unwrap_or_default();
        let ours: Vec<PathBuf> = pictures.iter().filter(|p| made.contains(&p.with_extension("png"))).cloned().collect();
        let mut ids: std::collections::HashSet<String> = self.request_column_twins(&ours).into_iter().collect();
        let n = ids.len();
        let deadline = std::time::Instant::now() + wait;
        while !ids.is_empty() {
            let left = deadline.saturating_duration_since(std::time::Instant::now());
            match self.pictures_rx.recv_timeout(left) {
                Ok(id) => {
                    ids.remove(&id);
                }
                Err(_) => break,
            }
        }
        n - ids.len()
    }

    /// Whether a kernel process is running for this tab.
    pub fn is_running(&self) -> bool {
        self.child.is_some()
    }

    /// The last answer to [`KernelProcess::request_names`]: (name, kind) pairs.
    pub fn names(&mut self) -> Option<Vec<(String, String)>> {
        self.names_rx.try_iter().last()
    }
}

impl Drop for KernelProcess {
    fn drop(&mut self) {
        self.stop();
    }
}

impl KernelLink for KernelProcess {
    fn send(&mut self, request: KernelRequest) {
        match request {
            KernelRequest::Eval { id, cell, src, line } => {
                if self.child.is_none() {
                    if let Err(e) = self.start() {
                        self.note(&format!("[{e}]"));
                        let _ = self.tx.send(KernelMsg::Exited { why: e });
                        return;
                    }
                }
                let mut v = json!({"type": "eval", "id": id, "cell": cell, "src": src, "line": line,
                                   "values": "each", "numbered": true, "pictures": PICTURES});
                if let Some(o) = &self.origin {
                    v["origin"] = json!(o);
                }
                self.write(v);
            }
            KernelRequest::Side { id, src } => {
                if self.child.is_none() {
                    if let Err(e) = self.start() {
                        self.note(&format!("[{e}]"));
                        let _ = self.tx.send(KernelMsg::Exited { why: e });
                        return;
                    }
                }
                // a control's evaluation (manipulate): a value per statement, no numbers, no history
                let mut v = json!({"type": "eval", "id": id, "cell": "manipulate", "src": src, "values": "each",
                                   "pictures": PICTURES});
                if let Some(o) = &self.origin {
                    v["origin"] = json!(o);
                }
                self.write(v);
            }
            KernelRequest::Interrupt { id } => self.write(json!({"type": "interrupt", "id": id})),
            KernelRequest::Shutdown => self.stop(),
        }
    }

    fn poll(&mut self) -> Vec<KernelMsg> {
        let msgs: Vec<KernelMsg> = self.rx.try_iter().collect();
        if msgs.iter().any(|m| matches!(m, KernelMsg::Exited { .. })) {
            self.child = None;
            self.stdin = None;
        }
        msgs
    }
}

/// The names of a `names` response line.
pub fn decode_names(line: &str) -> Option<Vec<(String, String)>> {
    if !line.contains("\"names\"") {
        return None;
    }
    let v: Value = serde_json::from_str(line).ok()?;
    if v.get("type")?.as_str()? != "names" {
        return None;
    }
    let list = v.get("names")?.as_array()?;
    Some(
        list.iter()
            .filter_map(|n| {
                let name = n.get("name").and_then(Value::as_str)?;
                Some((name.to_string(), n.get("kind").and_then(Value::as_str).unwrap_or("").to_string()))
            })
            .collect(),
    )
}

/// The id of a `done` line that answers one of the notebook's `picture` requests.
pub fn decode_picture_done(line: &str) -> Option<String> {
    if !line.contains(PICTURE_ID) {
        return None;
    }
    let v: Value = serde_json::from_str(line).ok()?;
    let id = v.get("id")?.as_str()?;
    (id.starts_with(PICTURE_ID) && v.get("type")?.as_str()? == "done").then(|| id.to_string())
}

/// The paths of the pictures a response line names (`image("path")` in a value, a display or an output).
pub fn picture_paths(line: &str) -> Vec<PathBuf> {
    if !line.contains("image(") {
        return vec![];
    }
    let Ok(v) = serde_json::from_str::<Value>(line) else { return vec![] };
    let text = ["printed", "text"].iter().find_map(|k| v.get(*k).and_then(Value::as_str)).unwrap_or("");
    const OPEN: &str = "image(\"";
    text.match_indices(OPEN)
        .filter_map(|(i, _)| {
            let rest = &text[i + OPEN.len()..];
            rest.find("\")").map(|end| PathBuf::from(&rest[..end]))
        })
        .collect()
}

/// The message of a response line (`None` for lines the notebook does not use).
pub fn decode(line: &str) -> Option<KernelMsg> {
    let v: Value = serde_json::from_str(line).ok()?;
    let id = v.get("id").and_then(Value::as_u64).unwrap_or(0);
    let s = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    let n = v.get("n").and_then(Value::as_u64).map(|n| n as u32);
    Some(match v.get("type")?.as_str()? {
        "hello" => KernelMsg::Hello { engine: s("engine") },
        "output" => KernelMsg::Output { id, text: s("text"), n },
        "value" => KernelMsg::Value { id, printed: s("printed"), n },
        // a picture: its printed form `image("path")` is what the Output cell holds and the file keeps
        "display" => KernelMsg::Display { id, printed: s("printed"), n },
        "failure" => KernelMsg::Failure { id, printed: s("printed"), n },
        "error" => KernelMsg::Error { id, rendered: s("rendered"), n },
        "message" => KernelMsg::Message { id, text: s("text"), n },
        "done" => KernelMsg::Done {
            id,
            interrupted: s("status") == "interrupted",
            first: v.get("first").and_then(Value::as_u64).map(|n| n as u32),
            next: v.get("next").and_then(Value::as_u64).map(|n| n as u32),
            millis: v.get("millis").and_then(Value::as_u64).unwrap_or(0),
        },
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decoding() {
        assert_eq!(
            decode(r#"{"id":null,"type":"hello","protocol":1,"version":"0.0.0","engine":"mathcas-v2 0.0.0"}"#),
            Some(KernelMsg::Hello { engine: "mathcas-v2 0.0.0".into() })
        );
        assert_eq!(
            decode(r#"{"id":3,"cell":"c1","type":"value","printed":"42","n":7}"#),
            Some(KernelMsg::Value { id: 3, printed: "42".into(), n: Some(7) })
        );
        assert_eq!(
            decode(r#"{"id":3,"type":"done","status":"interrupted","millis":5}"#),
            Some(KernelMsg::Done { id: 3, interrupted: true, first: None, next: None, millis: 5 })
        );
        let d = r#"{"id":4,"type":"display","kind":"svg","path":"a.svg","width":3,"height":2,"printed":"image(\"a.svg\")","n":1}"#;
        assert_eq!(decode(d), Some(KernelMsg::Display { id: 4, printed: "image(\"a.svg\")".into(), n: Some(1) }));
        assert_eq!(decode(r#"{"id":1,"type":"completions","names":[]}"#), None);
        assert_eq!(decode("not json"), None);
        let names = decode_names(r#"{"id":"names","type":"names","names":[{"name":"out","kind":"function"}]}"#);
        assert_eq!(names, Some(vec![("out".to_string(), "function".to_string())]));
        assert_eq!(decode_names(r#"{"id":1,"type":"done","status":"ok"}"#), None);
        assert_eq!(decode_picture_done(r#"{"id":"picture-3","type":"done","status":"ok"}"#), Some("picture-3".into()));
        assert_eq!(decode_picture_done(r#"{"id":"picture-3","type":"error","message":"x"}"#), None);
        assert_eq!(decode_picture_done(r#"{"id":3,"type":"done","status":"ok"}"#), None);
        assert_eq!(
            picture_paths(r#"{"id":3,"type":"value","printed":"[image(\"C:/t/plot-1.png\"), image(\"b.png\")]","n":1}"#),
            vec![PathBuf::from("C:/t/plot-1.png"), PathBuf::from("b.png")]
        );
        assert!(picture_paths(r#"{"id":3,"type":"value","printed":"42"}"#).is_empty());
    }
}
