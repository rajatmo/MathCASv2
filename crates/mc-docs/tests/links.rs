//! Every link of the documentation resolves: the names linked in the pages (usage calls, code spans of the prose,
//! example inputs and outputs, See also) and in the guides and the index open a function the catalogue knows, so
//! the window shows its page or its description.
use mc_docs::catalogue::{Area, Catalogue};
use mc_docs::inline::{spans, Span};
use mc_docs::library::{source_dir, Library};
use mc_docs::links::links_of;
use mc_docs::page::{Block, Page};

#[test]
fn every_link_resolves() {
    let cat = Catalogue::new();
    let known = |n: &str| cat.get(n).is_some();
    let lib = Library::from_dir(&source_dir());
    let mut links = 0usize;
    let mut bad = vec![];
    let mut check = |page: &str, target: &str, links: &mut usize| {
        *links += 1;
        if cat.get(target).is_none() {
            bad.push(format!("{page}: link to unknown `{target}`"));
        }
    };
    let names = lib.names();
    for (k, name) in names.iter().enumerate() {
        if k % 50 == 0 {
            eprintln!("[{}/{} {:.0}%] {name}", k + 1, names.len(), 100.0 * (k + 1) as f64 / names.len() as f64);
        }
        let (page, _) = Page::read(name, &lib.page_text(name).expect("listed"));
        let mut code: Vec<String> = page.usage.iter().map(|u| u.call.clone()).collect();
        let mut prose: Vec<&str> = page.usage.iter().map(|u| u.text.as_str()).collect();
        prose.extend(page.details.iter().map(|d| d.text.as_str()));
        for g in &page.examples {
            for b in &g.blocks {
                match b {
                    Block::Text(t) => prose.push(t),
                    Block::Input { source, output, .. } => {
                        code.push(source.clone());
                        code.extend(output.iter().map(|o| o.text.clone()));
                    }
                }
            }
        }
        for t in prose {
            for s in spans(t).unwrap_or_default() {
                if let Span::Code(c) = s {
                    code.push(c.to_string());
                }
            }
        }
        for src in &code {
            for (r, target) in links_of(src, &known) {
                assert_eq!(&src[r], target, "a link's range is its name");
                check(name, &target, &mut links);
            }
        }
        for target in &page.see_also {
            check(name, target, &mut links);
        }
    }
    // the guides and the index link every catalogued function
    for area in Area::ALL {
        for (_, entries) in cat.topics(area) {
            for e in entries {
                check("guide", e.name, &mut links);
            }
        }
    }
    eprintln!("{links} links checked");
    assert!(bad.is_empty(), "links that do not resolve:\n{}", bad.join("\n"));
    assert!(links > 2000, "only {links} links: the pages should link their functions");
}

/// Names of local variables and single letters that are not calls do not link; calls of documented functions do.
#[test]
fn links_in_a_page() {
    let cat = Catalogue::new();
    let known = |n: &str| cat.get(n).is_some();
    let got: Vec<String> = links_of("table(n^2, [n, 1, 5]) + n_integrate(sin(x), [x, 0, pi])", &known)
        .into_iter()
        .map(|(_, n)| n)
        .collect();
    assert_eq!(got, ["table", "n_integrate", "sin", "pi"]);
}
