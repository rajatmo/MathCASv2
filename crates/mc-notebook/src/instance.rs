//! Single instance: the first window listens on a loopback socket whose port and a random token are written to
//! `instance.json` in the user's settings folder (readable by this user only). A later launch with files sends them
//! there and exits; the running window opens them as tabs. When nothing answers (no window, a stale file, a
//! firewall), the launch opens its own window and becomes the instance. The documentation window uses the same
//! mechanism under its own file name ([`Server::start_as`], [`send_to`]) so that F1 in the notebook reuses an open
//! documentation window.
use eframe::egui;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// How long a launch waits for the running window to answer.
const ANSWER: Duration = Duration::from_millis(1500);

/// The notebook's instance file.
const NOTEBOOK: &str = "instance.json";

fn instance_file(name: &str) -> Option<PathBuf> {
    Some(crate::settings::config_dir()?.join(name))
}

/// A random token (the standard library's per-process random hash keys).
fn token() -> String {
    use std::hash::{BuildHasher, Hasher};
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos()));
    h.write_u32(std::process::id());
    let a = h.finish();
    let mut h2 = std::collections::hash_map::RandomState::new().build_hasher();
    h2.write_u64(a);
    format!("{a:016x}{:016x}", h2.finish())
}

/// The running window's end: files sent by other launches.
pub struct Server {
    received: Arc<Mutex<Vec<PathBuf>>>,
    port: u16,
    file: &'static str,
}

impl Server {
    /// Listen, and announce the port (`None` when the socket cannot be opened: the window works alone).
    pub fn start(ctx: &egui::Context) -> Option<Server> {
        Server::start_as(ctx, NOTEBOOK)
    }

    /// Listen, and announce the port in the settings folder's file `name` (another program's instance).
    pub fn start_as(ctx: &egui::Context, name: &'static str) -> Option<Server> {
        let file = instance_file(name)?;
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).ok()?;
        let port = listener.local_addr().ok()?.port();
        let token = token();
        let _ = std::fs::create_dir_all(file.parent()?);
        mc_nbfile::write_atomic(&file, &json!({"port": port, "token": token, "pid": std::process::id()}).to_string())
            .ok()?;
        let received = Arc::new(Mutex::new(vec![]));
        let sink = received.clone();
        let ctx = ctx.clone();
        std::thread::Builder::new()
            .name("mc-notebook-instance".into())
            .spawn(move || {
                for stream in listener.incoming().flatten() {
                    let _ = stream.set_read_timeout(Some(ANSWER));
                    let mut reader = BufReader::new(&stream);
                    let mut line = String::new();
                    if reader.read_line(&mut line).is_err() {
                        continue;
                    }
                    let Ok(v) = serde_json::from_str::<Value>(&line) else {
                        continue;
                    };
                    if v.get("token").and_then(Value::as_str) != Some(token.as_str()) {
                        continue;
                    }
                    let paths: Vec<PathBuf> = v
                        .get("paths")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .filter_map(Value::as_str)
                        .map(PathBuf::from)
                        .collect();
                    if let Ok(mut r) = sink.lock() {
                        r.extend(paths);
                    }
                    let _ = (&stream).write_all(b"ok\n");
                    ctx.request_repaint();
                }
            })
            .ok()?;
        Some(Server { received, port, file: name })
    }

    /// The files received since the last call.
    pub fn take(&self) -> Vec<PathBuf> {
        self.received.lock().map(|mut r| std::mem::take(&mut *r)).unwrap_or_default()
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        // the file goes when it still names this window
        if let Some(f) = instance_file(self.file) {
            let ours = std::fs::read_to_string(&f)
                .ok()
                .and_then(|t| serde_json::from_str::<Value>(&t).ok())
                .and_then(|v| v.get("port").and_then(Value::as_u64))
                == Some(u64::from(self.port));
            if ours {
                let _ = std::fs::remove_file(f);
            }
        }
    }
}

/// Send `paths` to a running window; `true` when it took them (this launch then exits).
pub fn send(paths: &[PathBuf]) -> bool {
    let abs: Vec<String> = paths
        .iter()
        .map(|p| std::fs::canonicalize(p).unwrap_or_else(|_| std::env::current_dir().unwrap_or_default().join(p)))
        .map(|p| strip_verbatim(&p.display().to_string()))
        .collect();
    send_to(NOTEBOOK, &abs)
}

/// Send `items` to the running instance announced in the settings folder's file `name`; `true` when it took them.
pub fn send_to(name: &str, items: &[String]) -> bool {
    let Some(file) = instance_file(name) else {
        return false;
    };
    let Some(v) = std::fs::read_to_string(file).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()) else {
        return false;
    };
    let (Some(port), Some(token)) = (v.get("port").and_then(Value::as_u64), v.get("token").and_then(Value::as_str))
    else {
        return false;
    };
    let Ok(port) = u16::try_from(port) else {
        return false;
    };
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let Ok(mut stream) = TcpStream::connect_timeout(&addr, Duration::from_millis(300)) else {
        return false;
    };
    let _ = stream.set_read_timeout(Some(ANSWER));
    let msg = format!("{}\n", json!({"token": token, "paths": items}));
    if stream.write_all(msg.as_bytes()).is_err() {
        return false;
    }
    let mut answer = String::new();
    BufReader::new(&stream).read_line(&mut answer).is_ok() && answer.trim() == "ok"
}

/// `C:\x` for the `\\?\C:\x` that canonicalisation gives on Windows.
fn strip_verbatim(p: &str) -> String {
    p.strip_prefix(r"\\?\").map_or_else(|| p.to_string(), str::to_string)
}
