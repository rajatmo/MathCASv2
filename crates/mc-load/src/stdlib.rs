//! The prelude and the standard modules, embedded from the repository's `lib/` folder (decision F.7): the binary
//! needs no extra files and startup does no file search.

/// The prelude: definitions in the bare namespace, evaluated when an engine starts.
pub const PRELUDE: &str = include_str!("../../../lib/prelude.mc");

/// The name of the prelude in messages.
pub const PRELUDE_ORIGIN: &str = "lib/prelude.mc";

/// The standard modules, by name: reached by `import name` when no file `name.mc` is found next to the importing
/// source.
pub const MODULES: &[(&str, &str)] = &[("stats", include_str!("../../../lib/stats.mc"))];

/// The name of standard module `name` in messages.
pub fn module_origin(name: &str) -> String {
    format!("lib/{name}.mc")
}
