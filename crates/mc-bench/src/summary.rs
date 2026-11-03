//! The summary: class tables, geometric means and milestone status (02-benchmarks section 4).
//!
//! Ratios are `t(v2) / t(reference)` per case and size, with reference times taken from another results file (the
//! reference system's numbers are recorded by private tooling in the same format). Without a reference the tables
//! show times only, M1 is still decided and M2 to M4 are `unknown`.
use crate::results::{Row, Status};
use crate::runner::seconds;
use std::collections::BTreeMap;
use std::fmt::Write;

/// The five scored classes; class M is measured as speedup and scored separately.
pub const SCORED_CLASSES: [&str; 5] = ["R", "P", "N", "L", "S"];

/// Whether a milestone target is met.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Milestone {
    Met,
    NotMet(String),
    Unknown(String),
}

impl Milestone {
    fn text(&self) -> String {
        match self {
            Milestone::Met => "met".into(),
            Milestone::NotMet(why) => format!("not met: {why}"),
            Milestone::Unknown(why) => format!("unknown: {why}"),
        }
    }
}

/// The computed summary.
#[derive(Clone, Debug)]
pub struct Summary {
    /// Per class: geometric mean of the ratios (or of the medians in seconds without a reference) and the count.
    pub class_means: BTreeMap<String, (f64, usize)>,
    /// Geometric mean of the scored classes' means, when there is a reference.
    pub overall: Option<f64>,
    /// M1 to M4, in order.
    pub milestones: [Milestone; 4],
    pub text: String,
}

fn geometric_mean(xs: &[f64]) -> Option<f64> {
    (!xs.is_empty()).then(|| (xs.iter().map(|x| x.ln()).sum::<f64>() / xs.len() as f64).exp())
}

/// Summarise `rows`, against `reference` rows when given (matched by case and size).
pub fn summarize(rows: &[Row], reference: Option<&[Row]>) -> Summary {
    let ratio = |r: &Row| -> Option<f64> {
        let mine = r.stats?.median;
        let theirs =
            reference?.iter().find(|x| x.case == r.case && x.size == r.size && x.status == Status::Ok)?.stats?.median;
        (theirs > 0.0 && mine > 0.0).then(|| mine / theirs)
    };
    let mut text = String::new();
    let mut by_class: BTreeMap<String, Vec<&Row>> = BTreeMap::new();
    for r in rows {
        by_class.entry(r.class.clone()).or_default().push(r);
    }
    let mut class_means = BTreeMap::new();
    let mut flags = Vec::new();
    for (class, rs) in &by_class {
        let _ = writeln!(text, "class {class}");
        let _ = writeln!(
            text,
            "  {:<10} {:<12} {:>12} {:>12} {:>12} {:>5} {:>10}  {:<6} {:<9} {:>8}",
            "case", "size", "median", "min", "iqr", "reps", "peak MB", "status", "check", "ratio"
        );
        let mut values = Vec::new();
        for r in rs {
            let (median, min, iqr) = match r.stats {
                Some(s) => (seconds(s.median), seconds(s.min), seconds(s.iqr)),
                None => ("-".into(), "-".into(), "-".into()),
            };
            let q = ratio(r);
            let peak = r.peak_bytes.map(|b| format!("{:.2}", b as f64 / 1048576.0)).unwrap_or("-".into());
            let _ = writeln!(
                text,
                "  {:<10} {:<12} {:>12} {:>12} {:>12} {:>5} {:>10}  {:<6} {:<9} {:>8}",
                r.case,
                r.size,
                median,
                min,
                iqr,
                r.reps,
                peak,
                r.status.word(),
                r.check,
                q.map(|q| format!("{q:.3}")).unwrap_or("-".into())
            );
            if reference.is_some() {
                values.extend(q);
            } else {
                values.extend(r.stats.map(|s| s.median));
            }
            if let (Some(mine), Some(theirs)) = (
                r.peak_bytes,
                reference
                    .and_then(|rf| rf.iter().find(|x| x.case == r.case && x.size == r.size))
                    .and_then(|x| x.peak_bytes),
            ) {
                if theirs > 0 && mine > 4 * theirs {
                    flags.push(format!("{}/{} {}", r.class, r.case, r.size));
                }
            }
        }
        if let Some(g) = geometric_mean(&values) {
            let shown = if reference.is_some() {
                format!("{g:.3} (ratio)")
            } else {
                format!("{} (median time, no reference)", seconds(g))
            };
            let _ = writeln!(text, "  geometric mean over {} measurements: {shown}", values.len());
            class_means.insert(class.clone(), (g, values.len()));
        }
        let _ = writeln!(text);
    }
    let scored: Vec<f64> = SCORED_CLASSES.iter().filter_map(|c| class_means.get(*c).map(|m| m.0)).collect();
    let overall =
        if reference.is_some() && scored.len() == SCORED_CLASSES.len() { geometric_mean(&scored) } else { None };
    if let Some(o) = overall {
        let _ = writeln!(text, "overall geometric mean (ratio): {o:.3}");
    }
    let milestones = milestones(
        rows,
        reference.is_some(),
        &class_means,
        overall,
        &rows.iter().filter_map(ratio).collect::<Vec<_>>(),
    );
    let _ = writeln!(text, "milestones (02-benchmarks section 4):");
    let names = [
        "M1 every case completes or reports limit cleanly",
        "M2 class means at most 2, every check passes",
        "M3 means at most 1 for R, P, N and 1.5 for L, S",
        "M4 overall mean at most 1, no case worse than 10",
    ];
    for (name, m) in names.iter().zip(&milestones) {
        let _ = writeln!(text, "  {name}: {}", m.text());
    }
    if !flags.is_empty() {
        let _ = writeln!(text, "memory above 4 times the reference: {}", flags.join(", "));
    }
    Summary { class_means, overall, milestones, text }
}

fn milestones(
    rows: &[Row],
    have_reference: bool,
    means: &BTreeMap<String, (f64, usize)>,
    overall: Option<f64>,
    ratios: &[f64],
) -> [Milestone; 4] {
    let label = |r: &Row| format!("{}/{} {}", r.class, r.case, r.size);
    let list = |rs: Vec<&Row>| rs.iter().take(5).map(|r| label(r)).collect::<Vec<_>>().join(", ");
    let broken: Vec<&Row> =
        rows.iter().filter(|r| matches!(r.status, Status::Error | Status::Crash) || r.is_hang()).collect();
    let m1 = if rows.is_empty() {
        Milestone::Unknown("no results".into())
    } else if broken.is_empty() {
        Milestone::Met
    } else {
        Milestone::NotMet(format!("{} errors, crashes or hangs: {}", broken.len(), list(broken)))
    };
    if !have_reference {
        let none = || Milestone::Unknown("no reference timings".into());
        return [m1, none(), none(), none()];
    }
    let missing: Vec<&str> = SCORED_CLASSES.iter().copied().filter(|c| !means.contains_key(*c)).collect();
    // A limit has no time, so leaving it out would flatter the mean: from M2 on every case must complete (D.9).
    let incomplete: Vec<&Row> = rows.iter().filter(|r| r.status != Status::Ok).collect();
    let failed: Vec<&Row> = rows.iter().filter(|r| r.check == "fail").collect();
    let target = |bounds: &dyn Fn(&str) -> f64| -> Milestone {
        if !missing.is_empty() {
            return Milestone::Unknown(format!("no ratios for class {}", missing.join(", ")));
        }
        if !incomplete.is_empty() {
            return Milestone::NotMet(format!(
                "{} measurements without a time: {}",
                incomplete.len(),
                list(incomplete.clone())
            ));
        }
        let over: Vec<String> = SCORED_CLASSES
            .iter()
            .filter(|c| means[**c].0 > bounds(c))
            .map(|c| format!("{c} {:.3}", means[*c].0))
            .collect();
        if over.is_empty() {
            Milestone::Met
        } else {
            Milestone::NotMet(format!("class means above target: {}", over.join(", ")))
        }
    };
    let m2 = match target(&|_| 2.0) {
        Milestone::Met if !failed.is_empty() => {
            Milestone::NotMet(format!("{} failed checks: {}", failed.len(), list(failed.clone())))
        }
        m => m,
    };
    let m3 = target(&|c| if matches!(c, "R" | "P" | "N") { 1.0 } else { 1.5 });
    let m4 = match (target(&|_| f64::INFINITY), overall) {
        (Milestone::Met, Some(o)) if o > 1.0 => Milestone::NotMet(format!("overall mean {o:.3}")),
        (Milestone::Met, _) if ratios.iter().any(|&q| q > 10.0) => {
            Milestone::NotMet("a case is more than 10 times slower".into())
        }
        (m, _) => m,
    };
    [m1, m2, m3, m4]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::results::Stats;

    fn row(class: &str, case: &str, median: Option<f64>, status: Status, check: &str) -> Row {
        Row {
            system: "s".into(),
            class: class.into(),
            case: case.into(),
            size: "1".into(),
            stats: median.map(|m| Stats { median: m, min: m, iqr: 0.0 }),
            reps: 5,
            peak_bytes: None,
            proc_peak_bytes: None,
            status,
            check: check.into(),
            detail: String::new(),
        }
    }

    fn all_classes(t: f64) -> Vec<Row> {
        SCORED_CLASSES.iter().map(|c| row(c, &format!("{c}1"), Some(t), Status::Ok, "pass")).collect()
    }

    #[test]
    fn m1_without_reference() {
        let rows = vec![row("R", "R1", Some(1.0), Status::Ok, "pass"), row("R", "R2", None, Status::Limit, "-")];
        let s = summarize(&rows, None);
        assert_eq!(s.milestones[0], Milestone::Met);
        assert!(matches!(s.milestones[1], Milestone::Unknown(_)));
        let mut hang = row("R", "R3", None, Status::Limit, "-");
        hang.detail = format!("time: {}", crate::results::KILLED);
        let s = summarize(&[rows[0].clone(), hang], None);
        assert!(matches!(s.milestones[0], Milestone::NotMet(_)), "{:?}", s.milestones);
    }

    #[test]
    fn ratios_and_targets() {
        let reference = all_classes(1.0);
        let s = summarize(&all_classes(0.5), Some(&reference));
        assert_eq!(s.overall, Some(0.5));
        assert!(s.milestones.iter().all(|m| *m == Milestone::Met), "{:?}", s.milestones);
        let mut slow = all_classes(0.5);
        slow[4].stats = Some(Stats { median: 1.8, min: 1.8, iqr: 0.0 }); // S at 1.8: M2 yes, M3 no
        let s = summarize(&slow, Some(&reference));
        assert_eq!(s.milestones[1], Milestone::Met);
        assert!(matches!(s.milestones[2], Milestone::NotMet(_)));
        let mut wrong = all_classes(0.5);
        wrong[0].check = "fail".into();
        assert!(matches!(summarize(&wrong, Some(&reference)).milestones[1], Milestone::NotMet(_)));
        let partial = &all_classes(0.5)[..3];
        assert!(matches!(summarize(partial, Some(&reference)).milestones[2], Milestone::Unknown(_)));
    }
}
