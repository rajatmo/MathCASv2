//! Which graphics interface and which GPU the window draws with.
//!
//! - **GPU**: the integrated one first (a notebook needs little; it keeps the discrete card cool and out of the
//!   way), then discrete, then virtual or other, the software renderer last. `MCNB_GPU=discrete` prefers the discrete
//!   one, `MCNB_GPU=software` the software renderer.
//! - **Interface**, in order of trust per platform: Windows Direct3D 12, then GL (Vulkan only when asked: an AMD
//!   Vulkan driver crashed the process in a test, and a driver hang reset a machine while the default choice
//!   could pick Vulkan); Linux Vulkan, then GL; Android GLES, then Vulkan (older Android Vulkan drivers are the
//!   least reliable); Apple Metal. `WGPU_BACKEND` (`dx12`, `vulkan`, `gl`, `metal`) names one.
//! - **Crash memory**: a driver crash ends the process before it can try the next interface. So the start writes
//!   `gpu.json` naming the interface it tries and clears the mark after the first frame is shown; a mark left by an
//!   earlier start means that interface crashed, and it is skipped from then on (deleting `gpu.json` forgets).
use eframe::wgpu;
use serde_json::{json, Value};
use std::path::PathBuf;

/// The interfaces to try, best first, on this platform.
pub fn backend_order() -> Vec<wgpu::Backends> {
    if let Some(b) = wgpu::Backends::from_env() {
        return vec![b];
    }
    order_for(std::env::consts::OS)
}

/// The interfaces to try on an operating system (`std::env::consts::OS` names).
pub fn order_for(os: &str) -> Vec<wgpu::Backends> {
    use wgpu::Backends as B;
    match os {
        "windows" => vec![B::DX12, B::GL],
        "linux" | "freebsd" | "openbsd" | "netbsd" | "dragonfly" => vec![B::VULKAN, B::GL],
        "android" => vec![B::GL, B::VULKAN],
        "macos" | "ios" => vec![B::METAL],
        _ => vec![B::PRIMARY, B::GL],
    }
}

/// Which kind of GPU comes first.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Prefer {
    Integrated,
    Discrete,
    Software,
}

impl Prefer {
    /// From `MCNB_GPU` (`integrated`, the default; `discrete`; `software`).
    pub fn from_env() -> Prefer {
        match std::env::var("MCNB_GPU").as_deref().map(str::to_ascii_lowercase).as_deref() {
            Ok("discrete" | "high" | "dgpu") => Prefer::Discrete,
            Ok("software" | "cpu") => Prefer::Software,
            _ => Prefer::Integrated,
        }
    }

    /// The rank of a device type (lower first).
    pub fn rank(self, t: wgpu::DeviceType) -> u8 {
        use wgpu::DeviceType as D;
        let base = match t {
            D::IntegratedGpu => 0,
            D::DiscreteGpu => 1,
            D::VirtualGpu | D::Other => 2,
            D::Cpu => 3,
        };
        match (self, t) {
            (Prefer::Discrete, D::DiscreteGpu) | (Prefer::Software, D::Cpu) => 0,
            (Prefer::Discrete | Prefer::Software, _) => base + 1,
            (Prefer::Integrated, _) => base,
        }
    }
}

/// The adapter to draw with: among those that can present to the window, the best ranked; ties keep the driver's
/// order.
pub fn choose_adapter(
    adapters: &[wgpu::Adapter],
    surface: Option<&wgpu::Surface<'_>>,
) -> Result<wgpu::Adapter, String> {
    let prefer = Prefer::from_env();
    adapters
        .iter()
        .filter(|a| surface.is_none_or(|s| a.is_surface_supported(s)))
        .min_by_key(|a| prefer.rank(a.get_info().device_type))
        .cloned()
        .ok_or_else(|| "no graphics adapter can draw to the window".to_string())
}

/// The name of an interface in `gpu.json`.
pub fn name(b: wgpu::Backends) -> String {
    use wgpu::Backends as B;
    [(B::DX12, "dx12"), (B::VULKAN, "vulkan"), (B::GL, "gl"), (B::METAL, "metal"), (B::BROWSER_WEBGPU, "webgpu")]
        .iter()
        .filter(|(f, _)| b.contains(*f))
        .map(|(_, n)| *n)
        .collect::<Vec<_>>()
        .join("+")
}

fn file() -> Option<PathBuf> {
    Some(crate::settings::config_dir()?.join("gpu.json"))
}

fn read() -> Value {
    file().and_then(|f| std::fs::read_to_string(f).ok()).and_then(|t| serde_json::from_str(&t).ok()).unwrap_or(json!({}))
}

fn write(v: &Value) {
    if let Some(f) = file() {
        if let Some(d) = f.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let _ = mc_nbfile::write_atomic(&f, &v.to_string());
    }
}

/// The interfaces that crashed an earlier start, after recording a mark an earlier start left behind.
pub fn crashed() -> Vec<String> {
    let mut v = read();
    let mut bad: Vec<String> = v
        .get("crashed")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect();
    if let Some(p) = v.get("starting").and_then(Value::as_str).map(str::to_string) {
        if !bad.contains(&p) {
            bad.push(p);
        }
        v["crashed"] = json!(bad);
        v["starting"] = Value::Null;
        write(&v);
    }
    bad
}

/// Mark the start with interface `b` (cleared by `started`).
pub fn starting(b: wgpu::Backends) {
    let mut v = read();
    v["starting"] = json!(name(b));
    write(&v);
}

/// The start with the marked interface failed without a crash (no device): not a crash, the next one is tried.
pub fn failed() {
    let mut v = read();
    v["starting"] = Value::Null;
    write(&v);
}

/// The first frame is on the screen: the interface works.
pub fn started(info: &wgpu::AdapterInfo) {
    let mut v = read();
    v["starting"] = Value::Null;
    v["last"] = json!({"backend": format!("{:?}", info.backend), "adapter": info.name, "type": format!("{:?}", info.device_type), "driver": info.driver});
    write(&v);
}

/// The interfaces to try now: the platform's order without those that crashed (all of them again when every one
/// did, since a crash may have had another cause).
pub fn to_try() -> Vec<wgpu::Backends> {
    let order = backend_order();
    let bad = crashed();
    let left: Vec<_> = order.iter().copied().filter(|b| !bad.contains(&name(*b))).collect();
    if left.is_empty() {
        order
    } else {
        left
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wgpu::{Backends as B, DeviceType as D};

    #[test]
    fn orders() {
        assert_eq!(name(B::DX12), "dx12");
        assert_eq!(name(B::VULKAN | B::GL), "vulkan+gl");
        assert_eq!(order_for("windows"), vec![B::DX12, B::GL]);
        assert_eq!(order_for("linux"), vec![B::VULKAN, B::GL]);
        assert_eq!(order_for("android"), vec![B::GL, B::VULKAN]);
        assert_eq!(order_for("macos"), vec![B::METAL]);
    }

    #[test]
    fn ranks() {
        let i = Prefer::Integrated;
        assert!(i.rank(D::IntegratedGpu) < i.rank(D::DiscreteGpu));
        assert!(i.rank(D::DiscreteGpu) < i.rank(D::Cpu));
        let d = Prefer::Discrete;
        assert!(d.rank(D::DiscreteGpu) < d.rank(D::IntegratedGpu));
        assert!(d.rank(D::IntegratedGpu) < d.rank(D::Cpu));
        let s = Prefer::Software;
        assert!(s.rank(D::Cpu) < s.rank(D::IntegratedGpu));
    }

    #[test]
    fn crash_marks_are_remembered() {
        let dir = std::env::temp_dir().join(format!("mcnb-gpu-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::env::set_var("MCNB_CONFIG_DIR", &dir);
        assert!(crashed().is_empty());
        starting(B::DX12);
        // the process "crashed": the next start finds the mark
        assert_eq!(crashed(), vec!["dx12".to_string()]);
        assert_eq!(crashed(), vec!["dx12".to_string()]);
        starting(B::GL);
        let info = wgpu::AdapterInfo {
            name: "test".into(),
            vendor: 0,
            device: 0,
            device_type: D::IntegratedGpu,
            device_pci_bus_id: String::new(),
            driver: String::new(),
            driver_info: String::new(),
            backend: wgpu::Backend::Gl,
            subgroup_min_size: 4,
            subgroup_max_size: 4,
            transient_saves_memory: None,
            limit_bucket: None,
        };
        started(&info);
        assert_eq!(crashed(), vec!["dx12".to_string()]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
