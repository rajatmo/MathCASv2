//! The help index's catalogue table (`mc-help`'s `data/index.txt`): every function of the catalogue with its area,
//! topic and description, the part of the help index the notebook and the language server cannot compute without
//! the libraries. `mc-docs-pages index` writes it; a test fails when the file is out of date.
use crate::catalogue::Catalogue;
use std::path::PathBuf;

/// The table for `cat`.
pub fn table(cat: &Catalogue) -> String {
    let entries: Vec<mc_help::Entry> = cat
        .entries
        .iter()
        .map(|e| mc_help::Entry {
            name: e.name.to_string(),
            area: e.area.key().to_string(),
            topic: e.topic.to_string(),
            page: false,
            summary: e.summary.to_string(),
            usages: vec![],
        })
        .collect();
    mc_help::Index::write(&entries)
}

/// Where the table lives in the repository this was built from.
pub fn table_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("mc-help").join("data").join("index.txt")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The help index built into the notebook and the language server lists the catalogue as it is (regenerate
    /// with `mc-docs-pages index`).
    #[test]
    fn the_help_index_is_current() {
        let want = table(&Catalogue::new());
        let have = mc_help::Index::embedded_text().replace("\r\n", "\n");
        assert!(have == want, "crates/mc-help/data/index.txt is out of date: run `mc-docs-pages index`");
        // every catalogued function is in the index, and every page's usage lines reached it
        let ix = mc_help::Index::embedded();
        let lib = crate::library::Library::from_dir(&crate::library::source_dir());
        for name in lib.names() {
            let e = ix.get(&name).unwrap_or_else(|| panic!("page `{name}` is not in the help index"));
            assert!(e.page && !e.usages.is_empty(), "{name}: no usage lines in the help index");
        }
    }
}
