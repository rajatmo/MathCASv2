//! Binary data (round 29): `byte_array`, `binary_write`, `binary_read_list`.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn eval(e: &mut CoreEngine, src: &str) -> String {
    match e.eval(src, &StopFlag::new()) {
        Ok(v) => v.printed,
        Err(err) => format!("error: {err}"),
    }
}

#[test]
fn bytes_and_files() {
    let dir = std::env::temp_dir().join(format!("mc-io-binary-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("t.bin").to_string_lossy().replace('\\', "/");
    let mut e = CoreEngine::new();
    let mut check = |src: &str, want: &str| assert_eq!(eval(&mut e, src), want, "{src}");
    check("byte_array([1, 2])", "byte_array([1, 2])");
    check("byte_array([1, 300])", "failure(argument, byte_array, 1)");
    check("binary_read_list(byte_array([1, 2, 255, 0]), \"Integer16\")", "[513, 255]");
    check("binary_read_list(byte_array([1, 2, 255, 0]), \"Integer16\", byte_order: 1)", "[258, -256]");
    check("binary_read_list(byte_array([65, 66, 67]), \"Character8\", 2)", "[\"A\", \"B\"]");
    check("binary_read_list(byte_array([1, 2, 3, 4, 5, 6]), [\"Byte\", \"Integer16\"])", "[[1, 770], [4, 1541]]");
    check(&format!("binary_write(\"{file}\", [65, 66, 200])"), &format!("\"{file}\""));
    check(&format!("binary_read_list(\"{file}\")"), "[65, 66, 200]");
    check(&format!("binary_read_list(\"{file}\", \"Integer8\")"), "[65, 66, -56]");
    check(&format!("binary_write(\"{file}\", [1.5, 2.5], \"Real32\")"), &format!("\"{file}\""));
    check(&format!("binary_read_list(\"{file}\", \"Real32\")"), "[1.5, 2.5]");
    assert!(eval(&mut e, "binary_read_list(\"/no/such/file.bin\")").contains("failure(io"));
    let _ = std::fs::remove_dir_all(&dir);
}
