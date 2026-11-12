//! The notebook over the real kernel (design 11, M1 exit test): open `examples/notebook.mcn`, evaluate every cell
//! through `mc-kernel`, and compare with the file `mc-repl --update` keeps up to date, labels and output numbers
//! aside.
use mc_nbmodel::{Command, FakeEnv, KernelState, Notebook};
use mc_notebook::kernel::KernelProcess;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// `mc-kernel` built next to this test's target directory, if it was built.
fn kernel_exe() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let name = if cfg!(windows) { "mc-kernel.exe" } else { "mc-kernel" };
    exe.ancestors().skip(1).take(2).map(|d| d.join(name)).find(|p| p.exists())
}

/// The text without labels and output-block markers (their numbering and splitting depend on the protocol mode).
fn modulo_labels(text: &str) -> String {
    text.lines()
        .filter(|l| !l.starts_with("#% output"))
        .map(|l| match l.find(" label=") {
            Some(i) if l.starts_with("#% ") => l[..i].to_string(),
            _ => l.to_string(),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn the_example_notebook_evaluates_like_the_runner() {
    let Some(exe) = kernel_exe() else {
        eprintln!("mc-kernel is not built: skipped");
        return;
    };
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let path = root.join("notebook.mcn");
    let text = std::fs::read_to_string(&path).unwrap().replace("\r\n", "\n");
    let mut nb = Notebook::parse(&text).unwrap();
    let mut link = KernelProcess::new(exe, eframe::egui::Context::default());
    link.dir = Some(root.clone());
    link.origin = Some(path.display().to_string());
    nb.apply(Command::EvaluateNotebook, &mut FakeEnv::default());
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        nb.pump(&mut link);
        if nb.kernel_state() == KernelState::Idle && nb.queue_len() == 0 && nb.running_cell().is_none() {
            break;
        }
        assert!(Instant::now() < deadline, "the kernel did not finish: {}", nb.to_text());
        std::thread::sleep(Duration::from_millis(5));
    }
    let got = nb.to_text();
    assert!(got.contains("#% cell label=1\n"), "{got}");
    assert_eq!(modulo_labels(&got), modulo_labels(&text));
    link.stop();
}
