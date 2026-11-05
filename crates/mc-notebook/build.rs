// Embeds the application icon (assets/app/icon.ico at the repository root) into the Windows executables, where
// Explorer, the taskbar and shortcuts show it.
fn main() {
    build_info();
    println!("cargo:rerun-if-changed=../../assets/app/icon.ico");
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("../../assets/app/icon.ico");
        if let Err(e) = res.compile() {
            println!("cargo:warning=application icon not embedded: {e}");
        }
    }
}

/// The commit (with "+changes" when the tree differs from it) and the build date, for the About page.
fn build_info() {
    println!("cargo:rerun-if-changed=../../.git/HEAD");
    println!("cargo:rerun-if-changed=../../.git/index");
    let git = |args: &[&str]| {
        std::process::Command::new("git")
            .args(args)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
    };
    if let Some(c) = git(&["rev-parse", "--short", "HEAD"]) {
        let dirty = git(&["status", "--porcelain", "--untracked-files=no"]).is_some_and(|s| !s.is_empty());
        println!("cargo:rustc-env=MC_COMMIT={c}{}", if dirty { "+changes" } else { "" });
    }
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let (y, m, d) = civil(secs as i64 / 86_400);
    println!("cargo:rustc-env=MC_BUILD_DATE={y:04}-{m:02}-{d:02}");
}

/// Days since 1970-01-01 to a calendar date (proleptic Gregorian).
fn civil(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}
