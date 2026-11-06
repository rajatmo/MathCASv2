//! Phase 6 (package p6-numeric, gap queue G52): file_exists and file_names.
use mc_bench::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};

#[test]
fn files() {
    let dir = std::env::temp_dir().join(format!("mc-p6-files-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("a1.txt"), "1").unwrap();
    std::fs::write(dir.join("a2.txt"), "2").unwrap();
    std::fs::write(dir.join("b.csv"), "3").unwrap();
    let d = dir.to_string_lossy().replace(std::path::MAIN_SEPARATOR, "/");
    let mut e = CoreEngine::with_config(EngineConfig { threads: 1, ..EngineConfig::default() });
    let mut run = |src: String| e.eval(&src, &StopFlag::new()).map(|v| v.printed).unwrap_or_else(|err| format!("error: {err}"));
    assert_eq!(run(format!("file_exists(\"{d}/a1.txt\")")), "true");
    assert_eq!(run(format!("file_exists(\"{d}/zz.txt\")")), "false");
    assert_eq!(run(format!("file_names(\"a?.txt\", \"{d}\")")), format!("[\"{d}/a1.txt\", \"{d}/a2.txt\"]"));
    assert_eq!(run(format!("length(file_names(\"{d}/*\"))")), "3");
    std::fs::remove_dir_all(&dir).unwrap();
}
