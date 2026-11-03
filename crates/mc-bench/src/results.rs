//! Results: one row per case and size (02-benchmarks section 7), stored per machine and date as
//! `bench/results/<machine>/<date>.tsv`.
//!
//! The file is tab-separated text: `# key: value` header lines describing the machine and the run, one line of
//! column names, then the rows. A run that covers only some cases replaces their rows and keeps the others, so
//! partial reruns on the same day update one file (decision D.8).
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// How a measurement ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// Completed; times are valid.
    Ok,
    /// Stopped at a time or memory limit; reported as `limit`, never as a time.
    Limit,
    /// The engine reported an error (parse error, unsupported construct, error in the setup).
    Error,
    /// The child process failed without reporting a result.
    Crash,
}

impl Status {
    pub fn word(self) -> &'static str {
        match self {
            Status::Ok => "ok",
            Status::Limit => "limit",
            Status::Error => "error",
            Status::Crash => "crash",
        }
    }
    pub fn from_word(s: &str) -> Option<Status> {
        Some(match s {
            "ok" => Status::Ok,
            "limit" => Status::Limit,
            "error" => Status::Error,
            "crash" => Status::Crash,
            _ => return None,
        })
    }
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.word())
    }
}

/// The detail text of a time limit the engine did not honour: the guard had to kill it. The summary counts these
/// as hangs (02-benchmarks section 1: a case that hangs past the limit is a bug).
pub const KILLED: &str = "killed by the guard";

/// Median, minimum and interquartile range of the repetition times, in seconds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stats {
    pub median: f64,
    pub min: f64,
    pub iqr: f64,
}

impl Stats {
    /// Statistics of a nonempty sample; quartiles by linear interpolation.
    pub fn of(times: &[f64]) -> Option<Stats> {
        if times.is_empty() {
            return None;
        }
        let mut t = times.to_vec();
        t.sort_by(f64::total_cmp);
        let q = |p: f64| {
            let x = p * (t.len() - 1) as f64;
            let (lo, hi) = (x.floor() as usize, x.ceil() as usize);
            t[lo] + (t[hi] - t[lo]) * (x - lo as f64)
        };
        Some(Stats { median: q(0.5), min: t[0], iqr: q(0.75) - q(0.25) })
    }
}

/// One results row.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub system: String,
    pub class: String,
    pub case: String,
    pub size: String,
    pub stats: Option<Stats>,
    pub reps: usize,
    /// Peak heap bytes of one measured evaluation above the session baseline.
    pub peak_bytes: Option<u64>,
    /// Peak memory of the whole child process, as the guard saw it.
    pub proc_peak_bytes: Option<u64>,
    pub status: Status,
    /// `pass`, `fail`, `unchecked`, or `-` when no result was produced.
    pub check: String,
    pub detail: String,
}

impl Row {
    /// Whether the guard had to kill the case at its time limit.
    pub fn is_hang(&self) -> bool {
        self.status == Status::Limit && self.detail.contains(KILLED)
    }
}

const COLUMNS: [&str; 13] = [
    "system",
    "class",
    "case",
    "size",
    "median_s",
    "min_s",
    "iqr_s",
    "reps",
    "peak_bytes",
    "proc_peak_bytes",
    "status",
    "check",
    "detail",
];

/// A results file: header fields and rows.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ResultsFile {
    pub header: Vec<(String, String)>,
    pub rows: Vec<Row>,
}

impl ResultsFile {
    /// The path for a machine and date under a results directory.
    pub fn path_for(dir: &Path, machine: &str, date: &str) -> PathBuf {
        dir.join(machine).join(format!("{date}.tsv"))
    }

    pub fn read(path: &Path) -> io::Result<ResultsFile> {
        ResultsFile::parse(&fs::read_to_string(path)?)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("{}: {e}", path.display())))
    }

    pub fn parse(text: &str) -> Result<ResultsFile, String> {
        let mut file = ResultsFile::default();
        for (i, line) in text.lines().enumerate() {
            if let Some(h) = line.strip_prefix("# ") {
                let (k, v) = h.split_once(": ").unwrap_or((h, ""));
                file.header.push((k.to_string(), v.to_string()));
                continue;
            }
            if line.is_empty() || line.starts_with("system\t") {
                continue;
            }
            file.rows.push(parse_row(line).map_err(|e| format!("line {}: {e}", i + 1))?);
        }
        Ok(file)
    }

    /// Replace rows with the same system, case and size, and append new ones; order follows the existing file.
    pub fn merge(&mut self, rows: Vec<Row>) {
        for row in rows {
            let key = |r: &Row| (r.system.clone(), r.case.clone(), r.size.clone());
            match self.rows.iter_mut().find(|r| key(r) == key(&row)) {
                Some(slot) => *slot = row,
                None => self.rows.push(row),
            }
        }
    }

    pub fn write(&self, path: &Path) -> io::Result<()> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        fs::write(path, self.to_string())
    }
}

impl fmt::Display for ResultsFile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (k, v) in &self.header {
            writeln!(f, "# {k}: {}", clean(v))?;
        }
        writeln!(f, "{}", COLUMNS.join("\t"))?;
        for r in &self.rows {
            // Shortest round-trip formatting, so a file read back gives the same numbers.
            let secs =
                |pick: fn(&Stats) -> f64| r.stats.as_ref().map(|s| format!("{:e}", pick(s))).unwrap_or("-".into());
            let fields = [
                clean(&r.system),
                clean(&r.class),
                clean(&r.case),
                clean(&r.size),
                secs(|s| s.median),
                secs(|s| s.min),
                secs(|s| s.iqr),
                r.reps.to_string(),
                bytes(r.peak_bytes),
                bytes(r.proc_peak_bytes),
                r.status.to_string(),
                clean(&r.check),
                clean(&r.detail),
            ];
            writeln!(f, "{}", fields.join("\t"))?;
        }
        Ok(())
    }
}

/// Fields cannot hold tabs or line breaks.
pub fn clean(s: &str) -> String {
    s.replace(['\t', '\n', '\r'], " ")
}

fn bytes(b: Option<u64>) -> String {
    b.map(|b| b.to_string()).unwrap_or("-".into())
}

fn parse_row(line: &str) -> Result<Row, String> {
    let f: Vec<&str> = line.split('\t').collect();
    if f.len() != COLUMNS.len() {
        return Err(format!("{} fields, expected {}", f.len(), COLUMNS.len()));
    }
    let num = |s: &str| -> Result<Option<f64>, String> {
        if s == "-" {
            Ok(None)
        } else {
            s.parse().map(Some).map_err(|_| format!("bad number {s}"))
        }
    };
    let stats = match (num(f[4])?, num(f[5])?, num(f[6])?) {
        (Some(median), Some(min), Some(iqr)) => Some(Stats { median, min, iqr }),
        _ => None,
    };
    let bytes = |s: &str| -> Result<Option<u64>, String> {
        if s == "-" {
            Ok(None)
        } else {
            s.parse().map(Some).map_err(|_| format!("bad byte count {s}"))
        }
    };
    Ok(Row {
        system: f[0].into(),
        class: f[1].into(),
        case: f[2].into(),
        size: f[3].into(),
        stats,
        reps: f[7].parse().map_err(|_| format!("bad repetition count {}", f[7]))?,
        peak_bytes: bytes(f[8])?,
        proc_peak_bytes: bytes(f[9])?,
        status: Status::from_word(f[10]).ok_or_else(|| format!("bad status {}", f[10]))?,
        check: f[11].into(),
        detail: f[12].into(),
    })
}

/// The reference machine description recorded with every run (02-benchmarks section 1).
pub fn machine_header() -> Vec<(String, String)> {
    let cores = std::thread::available_parallelism().map(|n| n.get().to_string()).unwrap_or("?".into());
    vec![
        ("machine".into(), machine_name()),
        ("cpu".into(), cpu_model()),
        ("cores".into(), cores),
        ("ram".into(), total_ram_bytes().map(|b| format!("{} MB", b >> 20)).unwrap_or("?".into())),
        ("os".into(), format!("{} {}", std::env::consts::OS, std::env::consts::ARCH)),
        ("compiler".into(), env!("MC_BENCH_RUSTC").to_string()),
    ]
}

/// The machine name used in results paths: letters, digits, `-` and `_` only.
pub fn machine_name() -> String {
    let raw = std::env::var("COMPUTERNAME").or_else(|_| std::env::var("HOSTNAME")).unwrap_or_default();
    let name: String = raw.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' }).collect();
    if name.is_empty() {
        "unknown".into()
    } else {
        name.to_ascii_lowercase()
    }
}

fn cpu_model() -> String {
    if let Ok(id) = std::env::var("PROCESSOR_IDENTIFIER") {
        return id;
    }
    fs::read_to_string("/proc/cpuinfo")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("model name"))
                .and_then(|l| l.split_once(':'))
                .map(|(_, m)| m.trim().to_string())
        })
        .unwrap_or("?".into())
}

#[cfg(windows)]
fn total_ram_bytes() -> Option<u64> {
    /// `MEMORYSTATUSEX`.
    #[repr(C)]
    struct MemoryStatus {
        length: u32,
        memory_load: u32,
        total_phys: u64,
        avail_phys: u64,
        total_page_file: u64,
        avail_page_file: u64,
        total_virtual: u64,
        avail_virtual: u64,
        avail_extended_virtual: u64,
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn GlobalMemoryStatusEx(buffer: *mut MemoryStatus) -> i32;
    }
    let mut m = MemoryStatus {
        length: size_of::<MemoryStatus>() as u32,
        memory_load: 0,
        total_phys: 0,
        avail_phys: 0,
        total_page_file: 0,
        avail_page_file: 0,
        total_virtual: 0,
        avail_virtual: 0,
        avail_extended_virtual: 0,
    };
    // SAFETY: `m` is a MEMORYSTATUSEX with its length field set, as the call requires.
    (unsafe { GlobalMemoryStatusEx(&mut m) } != 0).then_some(m.total_phys)
}

#[cfg(not(windows))]
fn total_ram_bytes() -> Option<u64> {
    let info = fs::read_to_string("/proc/meminfo").ok()?;
    let kb: u64 = info.lines().find(|l| l.starts_with("MemTotal:"))?.split_whitespace().nth(1)?.parse().ok()?;
    Some(kb << 10)
}

/// Today's date in UTC as `YYYY-MM-DD`.
pub fn today() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    civil_date((secs / 86_400) as i64)
}

/// The proleptic Gregorian date of a day count since 1970-01-01 (H. Hinnant's `civil_from_days`).
fn civil_date(days: i64) -> String {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(case: &str, size: &str, status: Status) -> Row {
        Row {
            system: "stub 0".into(),
            class: "R".into(),
            case: case.into(),
            size: size.into(),
            stats: (status == Status::Ok).then_some(Stats { median: 0.5, min: 0.25, iqr: 0.125 }),
            reps: 5,
            peak_bytes: Some(3 << 20),
            proc_peak_bytes: None,
            status,
            check: "pass".into(),
            detail: "a\tb".into(),
        }
    }

    #[test]
    fn stats() {
        let s = Stats::of(&[5.0, 1.0, 3.0, 2.0, 4.0]).unwrap();
        assert_eq!((s.median, s.min, s.iqr), (3.0, 1.0, 2.0));
        assert_eq!(Stats::of(&[2.0, 1.0]).unwrap().median, 1.5);
        assert!(Stats::of(&[]).is_none());
    }

    #[test]
    fn round_trip_and_merge() {
        let mut f =
            ResultsFile { header: vec![("machine".into(), "m".into())], rows: vec![row("R1", "20", Status::Ok)] };
        f.merge(vec![row("R1", "20", Status::Limit), row("R1", "25", Status::Ok)]);
        assert_eq!(f.rows.len(), 2);
        assert_eq!(f.rows[0].status, Status::Limit);
        let back = ResultsFile::parse(&f.to_string()).unwrap();
        assert_eq!(back.header, f.header);
        assert_eq!(back.rows[1].stats, f.rows[1].stats);
        assert_eq!(back.rows[1].peak_bytes, Some(3 << 20));
        assert_eq!(back.rows[0].detail, "a b");
    }

    #[test]
    fn dates() {
        assert_eq!(civil_date(0), "1970-01-01");
        assert_eq!(civil_date(19_782), "2024-02-29");
        assert_eq!(civil_date(20_760), "2026-11-03");
    }
}
