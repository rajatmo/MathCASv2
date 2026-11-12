//! R23-P1: a session's plot files are removed when the session ends (own process: the guard is per process).
use mc_api::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};

fn mine() -> Vec<String> {
    let dir = std::env::temp_dir().join("mathcas-plots");
    let prefix = format!("plot-{}-", std::process::id());
    std::fs::read_dir(dir)
        .map(|d| d.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).filter(|n| n.starts_with(&prefix)).collect())
        .unwrap_or_default()
}

#[test]
fn plot_files_are_removed_when_the_last_session_ends() {
    let cfg = || EngineConfig { threads: 1, display: true, ..EngineConfig::default() };
    let stop = StopFlag::new();
    let mut a = CoreEngine::with_config(cfg());
    let mut b = CoreEngine::with_config(cfg());
    a.eval("plot(sin(x), [x, 0, 3])", &stop).unwrap();
    b.eval("plot(cos(x), [x, 0, 3])", &stop).unwrap();
    assert!(mine().iter().any(|n| n.ends_with(".png")) && mine().iter().any(|n| n.ends_with(".svg")), "{:?}", mine());
    // one session ends, the other still shows its plot
    drop(a);
    assert!(!mine().is_empty(), "files stay while a session lives");
    drop(b);
    assert!(mine().is_empty(), "the last session's end removes the files: {:?}", mine());
}
