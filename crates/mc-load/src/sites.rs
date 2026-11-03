//! Definition sites and name resolutions recorded by [`Loader::read_recorded`](crate::Loader::read_recorded), for
//! editor support (go to definition, hover): lowered terms carry no positions (decision F.10), so the reader
//! records, while it resolves, where each name is defined and what each statement's names resolve to.
use crate::source::Origin;
use mc_syntax::Span;

/// What a [`Definition`] defines.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DefinitionKind {
    /// a module: `module M { ... }`
    Module,
    /// a definition statement (`fn`, `rule`, `sym`, `x = v`, `f(x) := e`, top-level `let`/`var`), at the top level
    /// of a source or in a module body
    Value,
}

/// One definition site.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Definition {
    /// the defined name as resolved: `M.f` for a member of module `M`, the bare name otherwise; the module's name
    /// for [`DefinitionKind::Module`]
    pub name: String,
    /// what is defined
    pub kind: DefinitionKind,
    /// the source the definition is in
    pub origin: Origin,
    /// the whole definition statement
    pub span: Span,
    /// the defined name as written in the statement (its first occurrence there; the statement's start if the
    /// name is not found)
    pub name_span: Span,
}

/// The resolutions of the names of one statement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resolution {
    /// the source the statement is in
    pub origin: Origin,
    /// the statement (a top-level statement, or one statement of a module body)
    pub span: Span,
    /// every name of the statement that is not locally bound, as written (after aliases such as `π`), with the
    /// name it resolves to, sorted by the written name
    pub names: Vec<(String, String)>,
}

impl Resolution {
    /// What the name written as `written` resolves to in this statement, if the statement uses it outside a local
    /// binding.
    pub fn get(&self, written: &str) -> Option<&str> {
        self.names
            .binary_search_by(|(w, _)| w.as_str().cmp(written))
            .ok()
            .map(|i| self.names[i].1.as_str())
    }
}

/// Everything [`Loader::read_recorded`](crate::Loader::read_recorded) recorded: in the input and in every module
/// file it read, in reading order. After an error, what was recorded before it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Sites {
    /// definition sites
    pub definitions: Vec<Definition>,
    /// name resolutions, one per resolved statement
    pub resolutions: Vec<Resolution>,
}

impl Sites {
    /// The definitions of the resolved name `name` of the given kind.
    pub fn definitions_of<'s>(&'s self, name: &'s str, kind: DefinitionKind) -> impl Iterator<Item = &'s Definition> {
        self.definitions.iter().filter(move |d| d.name == name && d.kind == kind)
    }

    /// The resolution of the innermost statement of the source named `origin` containing byte `offset`.
    pub fn resolution_at(&self, origin: &str, offset: u32) -> Option<&Resolution> {
        self.resolutions
            .iter()
            .filter(|r| r.origin.name == origin && r.span.start <= offset && offset <= r.span.end)
            .min_by_key(|r| r.span.end - r.span.start)
    }
}

/// The recorder the reader carries while recording: the sites, and the name tokens of the sources being read
/// (innermost last), to place definition names.
#[derive(Default)]
pub(crate) struct Recorder {
    pub sites: Sites,
    pub identifiers: Vec<Vec<(String, Span)>>,
}

impl Recorder {
    /// The first occurrence of `name` inside `span` in the source being read, or the start of the span.
    pub fn locate(&self, name: &str, span: Span) -> Span {
        // the tokens are in text order: start at the first one inside the span
        let ids = self.identifiers.last().map_or(&[][..], Vec::as_slice);
        let first = ids.partition_point(|(_, sp)| sp.start < span.start);
        ids[first..]
            .iter()
            .take_while(|(_, sp)| sp.start < span.end)
            .find(|(n, _)| n == name)
            .map_or(Span { start: span.start, end: span.start }, |(_, sp)| *sp)
    }
}
