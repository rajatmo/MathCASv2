//! Timing of datasets on the assembled engine: a CSV file of `n` rows (default 10^6) and five columns (an integer
//! id, a category of 20 values, two reals, a text column with some empty fields) is written, imported as a dataset,
//! queried and written back: `cargo run --release -p mc-io --example dataset_measure [n] [folder]`. Run it under the
//! guard with a memory cap; each step prints its time and a `[k/N pct%]` progress mark.
use mc_api::{Engine, StopFlag};
use std::fmt::Write as _;
use std::time::Instant;

fn big_csv(path: &std::path::Path, n: usize) -> usize {
    let mut s = String::with_capacity(n * 48);
    s.push_str("id,group,x,y,label\n");
    let mut r = 0x2545_f491_4f6c_dd1du64;
    for i in 0..n {
        r ^= r << 13;
        r ^= r >> 7;
        r ^= r << 17;
        let x = (r % 100_000) as f64 / 1000.0;
        let label = if r.is_multiple_of(10) { String::new() } else { format!("item {}", r % 997) };
        let _ = writeln!(s, "{},g{},{x},{},{label}", i + 1, r % 20, (r >> 20) % 1000);
    }
    std::fs::write(path, &s).unwrap();
    s.len()
}

fn main() {
    let mut args = std::env::args().skip(1);
    let n: usize = args.next().and_then(|a| a.parse().ok()).unwrap_or(1_000_000);
    let dir = args.next().map(std::path::PathBuf::from).unwrap_or_else(|| std::env::temp_dir().join("mc-io-dataset"));
    std::fs::create_dir_all(&dir).unwrap();
    let d = dir.to_string_lossy().replace('\\', "/");
    let t0 = Instant::now();
    let bytes = big_csv(&dir.join("big.csv"), n);
    println!("[0/9 0%] wrote {n} rows ({:.1} MB) in {:.0} ms", bytes as f64 / 1e6, t0.elapsed().as_secs_f64() * 1e3);
    let steps = [
        ("import_data as a dataset", format!("ds = import_data(\"{d}/big.csv\", \"dataset\"); length(ds)")),
        ("the same as lists of rows", format!("length(import_data(\"{d}/big.csv\"))")),
        ("aggregate by group", "length(aggregate(ds, \"group\", [\"x\" -> mean, \"y\" -> total, count]))".to_string()),
        ("sort_by a real column", "first(sort_by(ds, \"x\"))[\"id\"]".to_string()),
        ("total of a column", "total(column(ds, \"y\"))".to_string()),
        ("take 10 and print", "take(ds, 10)".to_string()),
        ("export_data csv", format!("export_data(\"{d}/out.csv\", ds)")),
        ("export_data json", format!("export_data(\"{d}/out.json\", ds)")),
        ("select 10^4 rows (f(row))", "length(select(take(ds, 10000), r => r[\"y\"] > 500))".to_string()),
    ];
    let mut e = mc_engine::CoreEngine::new();
    let stop = StopFlag::new();
    for (k, (what, src)) in steps.iter().enumerate() {
        let t0 = Instant::now();
        let out = match e.eval(src, &stop) {
            Ok(r) => r.printed,
            Err(err) => format!("error: {err:?}"),
        };
        let ms = t0.elapsed().as_secs_f64() * 1e3;
        let shown: String = out.lines().next().unwrap_or("").chars().take(40).collect();
        println!("[{}/{} {}%] {what:<28} {ms:>9.1} ms   {shown}", k + 1, steps.len(), (k + 1) * 100 / steps.len());
    }
    std::fs::remove_dir_all(&dir).ok();
}
