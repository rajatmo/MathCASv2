//! The topic guides of `docs/guides/`: the twelve topics exist and have the guide structure, every example output is
//! what a fresh run gives (images by existence and pixel size), the embedded copy is the repository copy, and the
//! guides show no path of the machine they were recorded on. Regenerate outputs with
//! `mc-docs-pages update --guides [stem ...]`.
use mc_docs::catalogue::Catalogue;
use mc_docs::guide::{all, Guide};
use mc_docs::library::{guides_dir, Library};
use mc_docs::runner;

/// The topics the documentation must have, in order.
const TOPICS: &[&str] = &[
    "getting-started",
    "solving-equations",
    "calculus",
    "simplification-and-assumptions",
    "differential-equations",
    "precision-and-numerics",
    "plotting-and-graphics",
    "lists-and-data",
    "special-functions",
    "transforms-and-series",
    "function-properties",
    "finite-fields-and-number-theory",
    "groups-and-combinatorics",
    "symmetries-and-differential-algebra",
    "analytical-mechanics",
];

fn source() -> Library {
    Library::default().with_guides_from(&guides_dir())
}

#[test]
fn the_twelve_topics_exist_in_order() {
    let guides = all(&source());
    let slugs: Vec<&str> = guides.iter().map(|g| g.slug.as_str()).collect();
    assert_eq!(slugs, TOPICS);
    for g in &guides {
        assert!(!g.title.is_empty() && !g.summary().is_empty(), "{}", g.stem);
    }
}

#[test]
fn every_guide_has_the_guide_structure() {
    let lib = source();
    let cat = Catalogue::new();
    let mut report = vec![];
    for stem in lib.guide_stems() {
        let text = lib.guide_text(&stem).expect("listed");
        let (_, problems) = Guide::read(&stem, &text, &|n| cat.get(n).is_some());
        report.extend(problems.iter().map(|p| format!("{stem}.mcn: {p}")));
    }
    assert!(report.is_empty(), "guide problems:\n{}", report.join("\n"));
}

#[test]
fn every_guide_matches_a_fresh_run() {
    // the stored outputs are written on Windows (machine-real last bits, operating-system texts)
    if !cfg!(windows) {
        eprintln!("guide outputs are checked on Windows only");
        return;
    }
    let dir = guides_dir();
    let cat = Catalogue::new();
    let known = |n: &str| cat.get(n).is_some();
    let stems = source().guide_stems();
    let mut report = vec![];
    for (k, stem) in stems.iter().enumerate() {
        let mut c = runner::check_guide(stem, &dir, &known, &mut |_, _| {});
        // a guide whose only problems are slow inputs is checked once more: under load a single input can pass the
        // time limit without being slow
        if !c.problems.is_empty() && c.problems.iter().all(|p| p.to_string().contains("the input took")) {
            c = runner::check_guide(stem, &dir, &known, &mut |_, _| {});
        }
        eprintln!("[{}/{} {:.0}%] {stem}: {} inputs, {} problems", k + 1, stems.len(), 100.0 * (k + 1) as f64 / stems.len() as f64, c.inputs, c.problems.len());
        report.extend(c.problems.iter().map(|p| format!("{stem}.mcn: {p}")));
    }
    assert!(report.is_empty(), "guide problems:\n{}", report.join("\n"));
}

#[test]
fn the_embedded_guides_are_the_repository_guides() {
    let disk = source();
    let built = Library::embedded();
    assert_eq!(built.guide_stems(), disk.guide_stems());
    for s in disk.guide_stems() {
        assert_eq!(built.guide_text(&s), disk.guide_text(&s), "{s}");
    }
}

#[test]
fn no_guide_shows_a_local_path() {
    let lib = source();
    let mut found = vec![];
    for stem in lib.guide_stems() {
        for (i, line) in lib.guide_text(&stem).unwrap_or_default().lines().enumerate() {
            let l = line.replace('\\', "/").to_lowercase();
            if l.contains(":/users/") || l.contains("/home/") || l.contains("/appdata/") {
                found.push(format!("{stem}.mcn: line {}: {line}", i + 1));
            }
        }
    }
    assert!(found.is_empty(), "guides with local paths:\n{}", found.join("\n"));
}
