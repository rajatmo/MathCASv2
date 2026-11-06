//! The page checker: every page of `docs/functions/` has the page format's structure, documents a function of the
//! catalogue, links only to functions, and its stored outputs are what a fresh run of its inputs gives (images by
//! existence and pixel size). Regenerate outputs with `mc-docs-pages update [name ...]`.
use mc_docs::catalogue::Catalogue;
use mc_docs::library::{source_dir, Library};
use mc_docs::page::{Page, GROUPS};
use mc_docs::runner;

/// The pages the documentation must have, with their example groups' sizes checked.
const REQUIRED: [&str; 10] =
    ["table", "plot", "d", "integrate", "solve", "simplify", "expand", "n_integrate", "n_dsolve", "find_root"];

#[test]
fn every_page_matches_a_fresh_run() {
    let dir = source_dir();
    let names = Library::from_dir(&dir).names();
    assert!(!names.is_empty(), "no pages in {}", dir.display());
    let catalogue = Catalogue::new();
    let known = |n: &str| catalogue.get(n).is_some();
    let mut report = vec![];
    for (k, name) in names.iter().enumerate() {
        let mut c = runner::check(name, &dir, &known, &mut |_, _| {});
        // a page whose only problems are slow inputs is checked once more: under a loaded machine (the suite,
        // parallel builds) a single input can pass the time limit without the page being slow
        if !c.problems.is_empty() && c.problems.iter().all(|p| p.to_string().contains("the input took")) {
            c = runner::check(name, &dir, &known, &mut |_, _| {});
        }
        eprintln!(
            "[{}/{} {:.0}%] {name}: {} inputs, slowest {:.3} s, {} problems",
            k + 1,
            names.len(),
            100.0 * (k + 1) as f64 / names.len() as f64,
            c.inputs,
            c.slowest,
            c.problems.len()
        );
        report.extend(c.problems.iter().map(|p| format!("{name}.mcn: {p}")));
    }
    assert!(report.is_empty(), "page problems:\n{}", report.join("\n"));
}

#[test]
fn the_required_pages_are_complete() {
    let lib = Library::from_dir(&source_dir());
    for name in REQUIRED {
        let text = lib.page_text(name).unwrap_or_else(|| panic!("no page for `{name}`"));
        let (page, problems) = Page::read(name, &text);
        assert!(problems.is_empty(), "{name}: {problems:?}");
        let basic = page.examples[0].inputs();
        assert!((3..=6).contains(&basic), "{name}: {basic} basic examples (3 to 6)");
        let more = page.examples.iter().filter(|g| g.title != GROUPS[0]).count();
        assert!(more >= 2, "{name}: {more} example groups besides the basic ones (at least 2)");
        assert!(!page.see_also.is_empty() && !page.details.is_empty(), "{name}: details and see also");
    }
}

/// The pages built into the program are the ones in the repository (a stale build would show old pages).
#[test]
fn the_embedded_pages_are_the_repository_pages() {
    let disk = Library::from_dir(&source_dir());
    let built = Library::embedded();
    assert_eq!(built.names(), disk.names());
    for n in disk.names() {
        assert_eq!(built.page_text(&n), disk.page_text(&n), "{n}");
    }
}
