//! The module table and import lists: reader state, by name, independent of any store.
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// A loaded module (decision F.2): its members are the symbols `"M.f"`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Module {
    /// the module's name `M`
    pub name: String,
    /// where it was defined (a file path or `<repl>`, ...)
    pub origin: String,
    /// every name defined at the top level of the module body, without the `M.` prefix
    pub members: BTreeSet<String>,
    /// the members marked `pub`
    pub public: BTreeSet<String>,
}

impl Module {
    /// The qualified name `M.f` of member `f`.
    pub fn qualified(&self, member: &str) -> String {
        qualify(&self.name, member)
    }
}

/// `M.f`.
pub(crate) fn qualify(module: &str, member: &str) -> String {
    format!("{module}.{member}")
}

/// `(M, f)` for a qualified name `M.f` (split at the last dot; `M` may itself contain dots).
pub(crate) fn split_qualified(name: &str) -> Option<(&str, &str)> {
    let i = name.rfind('.')?;
    let (m, f) = (&name[..i], &name[i + 1..]);
    (!m.is_empty() && !f.is_empty()).then_some((m, f))
}

/// The loaded modules, by name.
pub(crate) type Table = BTreeMap<String, Module>;

/// The names an import list brings into scope: each bare name with the modules that provide it, in import order
/// (more than one module is an ambiguity, reported where the name is used: decision F.4).
#[derive(Clone, Debug, Default)]
pub(crate) struct Imports {
    names: HashMap<String, Vec<String>>,
}

impl Imports {
    /// `name` as provided by `module`.
    pub(crate) fn add(&mut self, name: &str, module: &str) {
        let v = self.names.entry(name.to_string()).or_default();
        if !v.iter().any(|m| m == module) {
            v.push(module.to_string());
        }
    }

    /// The modules providing `name`.
    pub(crate) fn get(&self, name: &str) -> Option<&[String]> {
        self.names.get(name).map(Vec::as_slice)
    }
}
