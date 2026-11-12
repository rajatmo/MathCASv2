//! [`Loader`]: the reader state of a session (module table, session imports) and the reading of one input:
//! header check, parse and lowering, module and import statements, loading of module files, and name resolution
//! of every statement (decisions F.1 to F.6).
use crate::error::LoadError;
use crate::modules::{qualify, split_qualified, Imports, Module, Table};
use crate::resolve::{defined_name, reserved, Ctx, Env, Fail, Walk};
use crate::sites::{Definition, DefinitionKind, Recorder, Resolution, Sites};
use crate::source::{FileSystem, Origin, Source, SourceProvider};
use crate::stdlib;
use mc_api::{names, Store, Sym, View, T};
use mc_syntax::{as_sym, Diagnostic, Head, Heads, Span, Spanned};
use std::collections::{BTreeSet, HashSet};
use std::path::Path;

/// The statements of one input, resolved and ready to evaluate in order.
#[derive(Debug)]
pub struct Read<'h> {
    /// The statements: those of module files loaded by its imports first, then its own. A module statement becomes
    /// the definitions of its body followed by `null`; an import becomes `null`.
    pub statements: Vec<T<'h>>,
    /// Qualified names (`M.f`) of the members of modules this input defines again (a reload): their definitions
    /// must be cleared before the statements run, so that clauses are not duplicated (decision F.8).
    pub reloaded: Vec<String>,
    /// (holder, outer) pairs of the argument-head rules that the modules this input defines again attached the last
    /// time (`@on(holder)` on a rule for `outer`): they must be cleared too, since the holder need not be a member
    /// (decision 9.6 of the library and kernel gaps).
    pub detached: Vec<(String, String)>,
    /// For each top-level statement of the input itself (not of the module files it loads), in order: the index in
    /// `statements` of its last term (a module or import statement's closing `null`, any other statement's term),
    /// so that front ends can tell the input's statements apart (per-statement values, line numbers).
    pub ends: Vec<usize>,
}

/// The reader state of a session.
pub struct Loader {
    modules: Table,
    /// the session's imports: they persist from one input to the next (decision F.4)
    imports: Imports,
    provider: Box<dyn SourceProvider>,
}

impl Default for Loader {
    /// A loader reading from the file system.
    fn default() -> Loader {
        Loader::new(Box::new(FileSystem))
    }
}

impl Loader {
    /// A loader reading module files through `provider`.
    pub fn new(provider: Box<dyn SourceProvider>) -> Loader {
        Loader { modules: Table::new(), imports: Imports::default(), provider }
    }

    /// Read module files through `provider` from now on.
    pub fn set_provider(&mut self, provider: Box<dyn SourceProvider>) {
        self.provider = provider;
    }

    /// The loaded modules, by name.
    pub fn modules(&self) -> impl Iterator<Item = &Module> {
        self.modules.values()
    }

    /// The loaded module `name`.
    pub fn module(&self, name: &str) -> Option<&Module> {
        self.modules.get(name)
    }

    /// Read `text` from `origin` and resolve it, loading the module files its imports need. `known` says whether
    /// the kernel knows a symbol (a native, rule, attribute or own value). On an error, the reader state is as
    /// before the call and nothing should be evaluated.
    pub fn read<'h, S: Store + ?Sized>(
        &mut self,
        store: &'h S,
        text: &str,
        origin: &Origin,
        known: &dyn Fn(Sym) -> bool,
    ) -> Result<Read<'h>, LoadError> {
        self.read_with(store, text, origin, known, None).0
    }

    /// [`Loader::read`], also recording where every name is defined and what the names of each statement resolve
    /// to, in the input and in the module files it reads (for editor support: go to definition, hover). The
    /// sites are returned after an error too, as far as reading got.
    pub fn read_recorded<'h, S: Store + ?Sized>(
        &mut self,
        store: &'h S,
        text: &str,
        origin: &Origin,
        known: &dyn Fn(Sym) -> bool,
    ) -> (Result<Read<'h>, LoadError>, Sites) {
        let (r, rec) = self.read_with(store, text, origin, known, Some(Recorder::default()));
        (r, rec.map(|r| r.sites).unwrap_or_default())
    }

    fn read_with<'h, S: Store + ?Sized>(
        &mut self,
        store: &'h S,
        text: &str,
        origin: &Origin,
        known: &dyn Fn(Sym) -> bool,
        recorder: Option<Recorder>,
    ) -> (Result<Read<'h>, LoadError>, Option<Recorder>) {
        let heads = Heads::new(store);
        let reserved = reserved(store);
        let saved = self.modules.clone();
        let mut imports = std::mem::take(&mut self.imports);
        let saved_imports = imports.clone();
        let mut rd = Reader {
            store,
            heads: &heads,
            reserved: &reserved,
            known,
            modules: &mut self.modules,
            provider: &*self.provider,
            loading: vec![],
            out: vec![],
            ends: vec![],
            reloaded: vec![],
            recorder,
            detached: vec![],
        };
        let r = rd.source(text, origin, &mut imports, true);
        let recorder = rd.recorder.take();
        let r = match r {
            Ok(()) => {
                let (statements, ends, mut reloaded, mut detached) = (rd.out, rd.ends, rd.reloaded, rd.detached);
                reloaded.sort();
                reloaded.dedup();
                detached.sort();
                detached.dedup();
                self.imports = imports;
                Ok(Read { statements, reloaded, detached, ends })
            }
            Err(e) => {
                self.modules = saved;
                self.imports = saved_imports;
                Err(e)
            }
        };
        (r, recorder)
    }

    /// [`Loader::read`] on the file at `path` (a script, or a module file loaded again).
    pub fn read_file<'h, S: Store + ?Sized>(
        &mut self,
        store: &'h S,
        path: &Path,
        known: &dyn Fn(Sym) -> bool,
    ) -> Result<Read<'h>, LoadError> {
        let origin = Origin::file(path);
        let text = self
            .provider
            .file(path)
            .map_err(|e| LoadError::plain(&origin.name, format!("cannot read the file: {e}"), true))?;
        self.read(store, &text, &origin, known)
    }
}

/// An error while reading one source: at a position of this source (the statement's span locates it), or a
/// complete error from a module file it loaded.
enum Error {
    Here(Fail, Span),
    Elsewhere(LoadError),
}

/// The state of one [`Loader::read`].
struct Reader<'a, 'h, S: Store + ?Sized> {
    store: &'h S,
    heads: &'a Heads,
    reserved: &'a HashSet<Sym>,
    known: &'a dyn Fn(Sym) -> bool,
    modules: &'a mut Table,
    provider: &'a dyn SourceProvider,
    /// the modules whose files are being read, outermost first (cycle detection)
    loading: Vec<String>,
    out: Vec<T<'h>>,
    /// the index in `out` of the last term of each top-level statement of the input ([`Read::ends`])
    ends: Vec<usize>,
    reloaded: Vec<String>,
    /// the sites being recorded, for [`Loader::read_recorded`]
    recorder: Option<Recorder>,
    detached: Vec<(String, String)>,
}

impl<'a, 'h, S: Store + ?Sized> Reader<'a, 'h, S> {
    /// Read one source text: its statements go to `out` in order. `imports` are the imports of its top level.
    fn source(&mut self, text: &str, origin: &Origin, imports: &mut Imports, in_input: bool) -> Result<(), LoadError> {
        if let Some(rec) = self.recorder.as_mut() {
            rec.identifiers.push(mc_syntax::identifiers(text));
        }
        let r = self.source_here(text, origin, imports, in_input);
        if let Some(rec) = self.recorder.as_mut() {
            rec.identifiers.pop();
        }
        r
    }

    fn source_here(&mut self, text: &str, origin: &Origin, imports: &mut Imports, in_input: bool) -> Result<(), LoadError> {
        let at = |ds: Vec<Diagnostic>| LoadError::at(&origin.name, text, ds, in_input);
        if let Some(d) = header_error(text) {
            return Err(at(vec![d]));
        }
        let stmts = mc_syntax::read_spanned(self.store, text).map_err(at)?;
        // bare names this source defines at its top level count as known inside its modules (decision F.11)
        let defined: HashSet<Sym> = stmts
            .iter()
            .filter_map(|s| defined_name(self.store, self.heads, unwrap_public(self.store, self.heads, s.term).0))
            .filter(|&s| split_qualified(&self.store.sym_name(s)).is_none())
            .collect();
        for s in &stmts {
            match self.statement(s, origin, imports, &defined) {
                Ok(()) if in_input => self.ends.push(self.out.len() - 1),
                Ok(()) => {}
                Err(Error::Here(fail, span)) => {
                    let span = locate(text, &fail.name, span);
                    return Err(at(vec![Diagnostic::new(span, fail.message)]));
                }
                Err(Error::Elsewhere(e)) => return Err(e),
            }
        }
        Ok(())
    }

    fn null(&self) -> T<'h> {
        self.store.sym_term(self.store.sym(names::NULL))
    }

    /// One top-level statement.
    fn statement(&mut self, s: &Spanned<'h>, origin: &Origin, imports: &mut Imports, defined: &HashSet<Sym>) -> Result<(), Error> {
        let here = |f: Fail| Error::Here(f, s.span);
        match self.heads.classify(self.store, s.term) {
            Some((Head::Module, &[name, body])) => self.module(name, body, s, origin, defined),
            Some((h @ (Head::Import | Head::ImportExcept), args)) => {
                self.import(h, args, origin, imports, s.span)?;
                self.out.push(self.null());
                Ok(())
            }
            Some((Head::Public, _)) => Err(here(Fail::new("pub", "`pub` is only allowed in a module body"))),
            _ => {
                // defining an imported name at the top level would change the module's function (or define a
                // bare symbol that every use bypasses): an error (decision F.14)
                if let Some(d) = defined_name(self.store, self.heads, s.term) {
                    let n = self.store.sym_name(d);
                    if let Some(ms) = imports.get(&n) {
                        let message = format!("cannot define `{n}` here: the name is imported from module {}", ms.join(" and "));
                        return Err(here(Fail::new(n, message)));
                    }
                }
                let t = self.resolve(Ctx::Top { imports }, s.term, origin, s.span).map_err(here)?;
                self.out.push(t);
                Ok(())
            }
        }
    }

    /// Statement `t` (at `span` of `origin`) with its names resolved; recorded when recording.
    fn resolve(&mut self, ctx: Ctx<'_>, t: T<'h>, origin: &Origin, span: Span) -> Result<T<'h>, Fail> {
        let (st, hs) = (self.store, self.heads);
        let env = Env { store: st, heads: hs, modules: &*self.modules, known: self.known, reserved: self.reserved };
        let mut walk = Walk::new(&env, ctx);
        let r = walk.statement(t)?;
        let Some(rec) = self.recorder.as_mut() else { return Ok(r) };
        let mut names: Vec<(String, String)> = walk.resolved().map(|(w, to)| (st.sym_name(w), st.sym_name(to))).collect();
        names.sort();
        rec.sites.resolutions.push(Resolution { origin: origin.clone(), span, names });
        if let Some(d) = defined_name(st, hs, r) {
            let written = defined_name(st, hs, t).map(|w| st.sym_name(w)).unwrap_or_default();
            let name_span = rec.locate(&written, span);
            let kind = DefinitionKind::Value;
            rec.sites.definitions.push(Definition { name: st.sym_name(d), kind, origin: origin.clone(), span, name_span });
        }
        Ok(r)
    }

    /// `module M { ... }` (decisions F.2 to F.4): the members are collected and the module registered first, so
    /// forward references work; then each definition is resolved and emitted in order, then `null`.
    fn module(&mut self, name: T<'h>, body: T<'h>, s: &Spanned<'h>, origin: &Origin, defined: &HashSet<Sym>) -> Result<(), Error> {
        let (st, hs) = (self.store, self.heads);
        let here = |f: Fail| Error::Here(f, s.span);
        let mname = match as_sym(st, name) {
            Some(n) => st.sym_name(n),
            None => return Err(here(Fail::new("module", "a module name must be a name"))),
        };
        if mname.contains('.') {
            return Err(here(Fail::new(mname.clone(), format!("nested modules are not supported yet: `{mname}`"))));
        }
        let items: &[T<'h>] = match hs.classify(st, body) {
            Some((Head::List, items)) => items,
            _ => &[],
        };
        let span_of = |i: usize| s.body.get(i).copied().unwrap_or(s.span);
        let mut members = BTreeSet::new();
        let mut public = BTreeSet::new();
        let mut imports = Imports::default();
        for (i, &item) in items.iter().enumerate() {
            let (inner, is_pub) = unwrap_public(st, hs, item);
            let fail = |f: Fail| Error::Here(f, span_of(i));
            match hs.classify(st, inner) {
                Some((Head::Module, _)) => return Err(fail(Fail::new("module", "nested modules are not supported yet"))),
                Some((h @ (Head::Import | Head::ImportExcept), args)) => {
                    if is_pub {
                        return Err(fail(Fail::new("pub", "an import cannot be `pub` (re-exports are not supported)")));
                    }
                    self.import(h, args, origin, &mut imports, span_of(i))?;
                }
                _ => match defined_name(st, hs, inner) {
                    Some(d) => {
                        let n = st.sym_name(d);
                        let member = match split_qualified(&n) {
                            Some((m, f)) if m == mname => f.to_string(),
                            // a definition for another module's member: not a member of this one
                            Some(_) => continue,
                            None => n,
                        };
                        if self.reserved.contains(&st.sym(&member)) {
                            let message = format!("`{member}` is a name of the core syntax and cannot be defined in a module");
                            return Err(fail(Fail::new(member, message)));
                        }
                        if is_pub {
                            public.insert(member.clone());
                        }
                        members.insert(member);
                    }
                    None if is_pub => {
                        return Err(fail(Fail::new("pub", "`pub` marks a definition: fn, rule, sym, x = v, let or var")))
                    }
                    None => {}
                },
            }
        }
        if let Some(old) = self.modules.get(&mname) {
            self.reloaded.extend(old.members.iter().map(|f| qualify(&mname, f)));
            self.detached.extend(old.attached.iter().cloned());
        }
        let module = Module {
            name: mname.clone(),
            origin: origin.name.clone(),
            members: members.clone(),
            public,
            attached: BTreeSet::new(),
        };
        self.modules.insert(mname.clone(), module);
        if let Some(rec) = self.recorder.as_mut() {
            let name_span = rec.locate(&mname, s.span);
            let (kind, span) = (DefinitionKind::Module, s.span);
            rec.sites.definitions.push(Definition { name: mname.clone(), kind, origin: origin.clone(), span, name_span });
        }
        let mut attached = BTreeSet::new();
        for (i, &item) in items.iter().enumerate() {
            let inner = unwrap_public(st, hs, item).0;
            if matches!(hs.classify(st, inner), Some((Head::Import | Head::ImportExcept, _))) {
                continue;
            }
            let ctx = Ctx::Module { name: &mname, members: &members, imports: &imports, defined };
            let t = self.resolve(ctx, inner, origin, span_of(i)).map_err(|f| Error::Here(f, span_of(i)))?;
            if let Some((holder, outer)) = attachment(st, hs, t) {
                attached.insert((st.sym_name(holder), st.sym_name(outer)));
            }
            self.out.push(t);
        }
        if let Some(m) = self.modules.get_mut(&mname) {
            m.attached = attached;
        }
        self.out.push(self.null());
        Ok(())
    }

    /// `import M`, `import M.{f, g}`, `import M except h` into `imports` (decision F.4).
    fn import(&mut self, h: Head, args: &[T<'h>], origin: &Origin, imports: &mut Imports, span: Span) -> Result<(), Error> {
        let (st, hs) = (self.store, self.heads);
        let here = |f: Fail| Error::Here(f, span);
        let Some(path) = args.first().and_then(|&p| as_sym(st, p)) else {
            return Err(here(Fail::new("import", "an import names a module")));
        };
        let m = st.sym_name(path);
        self.ensure(&m, origin, span)?;
        let module = &self.modules[&m];
        let listed: Vec<String> = match args.get(1).map(|&l| hs.classify(st, l)) {
            Some(Some((Head::List, names))) => names.iter().filter_map(|&n| as_sym(st, n)).map(|n| st.sym_name(n)).collect(),
            _ => vec![],
        };
        for n in &listed {
            if !module.public.contains(n) {
                let why = if module.members.contains(n) {
                    format!("`{n}` is private to module {m} (mark it `pub` to export it)")
                } else {
                    format!("module {m} has no member `{n}`")
                };
                return Err(here(Fail::new(n.clone(), why)));
            }
        }
        match (h, args.len()) {
            (Head::Import, 2) => listed.iter().for_each(|n| imports.add(n, &m)),
            (Head::ImportExcept, _) => module.public.iter().filter(|n| !listed.contains(n)).for_each(|n| imports.add(n, &m)),
            _ => module.public.iter().for_each(|n| imports.add(n, &m)),
        }
        Ok(())
    }

    /// Make sure module `m` is in the table, reading its file if needed (decision F.5).
    fn ensure(&mut self, m: &str, origin: &Origin, span: Span) -> Result<(), Error> {
        let here = |message: String| Error::Here(Fail::new(m, message), span);
        if self.modules.contains_key(m) {
            return Ok(());
        }
        if m.contains('.') {
            return Err(here(format!("nested modules are not supported yet: `{m}`")));
        }
        if let Some(i) = self.loading.iter().position(|x| x == m) {
            let mut chain = self.loading[i..].to_vec();
            chain.push(m.to_string());
            return Err(here(format!("import cycle: {}", chain.join(" -> "))));
        }
        let file = match &origin.dir {
            Some(dir) => self.provider.module(m, dir).map_err(|e| here(format!("cannot read the file of module {m}: {e}")))?,
            None => None,
        };
        let standard = || {
            stdlib::MODULES
                .iter()
                .find(|(n, _)| *n == m)
                .map(|(n, text)| Source { origin: Origin::embedded(stdlib::module_origin(n)), text: text.to_string() })
        };
        let Some(src) = file.or_else(standard) else {
            let place = match &origin.dir {
                Some(d) if d.as_os_str().is_empty() => " in the current directory".to_string(),
                Some(d) => format!(" in {}", d.display()),
                None => String::new(),
            };
            return Err(here(format!("no module `{m}`: no file {m}.mc{place} and no standard module of that name")));
        };
        self.loading.push(m.to_string());
        // the top-level imports of a module file are its own, not the session's
        let r = self.source(&src.text, &src.origin, &mut Imports::default(), false);
        self.loading.pop();
        r.map_err(Error::Elsewhere)?;
        if !self.modules.contains_key(m) {
            return Err(here(format!("{} does not define module `{m}`", src.origin.name)));
        }
        Ok(())
    }
}

/// (holder, outer) for a resolved definition attached with the decorator `on(holder)` to a rule for `outer`:
/// `define_fn(lhs, body, options, type, list(decorators))` or `define_rule(rule, list(decorators))`.
fn attachment<'h, S: Store + ?Sized>(st: &'h S, hs: &Heads, t: T<'h>) -> Option<(Sym, Sym)> {
    let decs = match hs.classify(st, t)? {
        (Head::DefineFn, [_, _, _, _, decs]) | (Head::DefineRule, [_, decs]) => *decs,
        _ => return None,
    };
    let Some((Head::List, decs)) = hs.classify(st, decs) else { return None };
    let holder = decs.iter().find_map(|&d| match st.view(d) {
        View::App { head, args: &[s] } if as_sym(st, head).is_some_and(|h| st.sym_name(h) == "on") => as_sym(st, s),
        _ => None,
    })?;
    Some((holder, defined_name(st, hs, t)?))
}

/// The statement under `pub`, and whether it was there.
fn unwrap_public<'h, S: Store + ?Sized>(st: &'h S, hs: &Heads, t: T<'h>) -> (T<'h>, bool) {
    match hs.is(st, t, Head::Public, 1) {
        Some(a) => (a[0], true),
        None => (t, false),
    }
}

/// An error for a first line `#!...` other than `#! mathcas 2` (decision F.6) or `#! mathcas 2 notebook`: every line
/// of a notebook but its code cells is a comment, so it reads as the script of its code cells (`mc-kernel`, 2.6).
fn header_error(text: &str) -> Option<Diagnostic> {
    let first = text.lines().next().unwrap_or("");
    let rest = first.strip_prefix("#!")?;
    let mut words = rest.split_whitespace();
    if words.next() == Some("mathcas") && words.next() == Some("2") && matches!(words.next(), None | Some("notebook")) && words.next().is_none() {
        return None;
    }
    Some(Diagnostic::new(
        Span::new(0, first.len()),
        format!("unknown file header `{}`: a MathCAS v2 file starts with `#! mathcas 2`", first.trim_end()),
    ))
}

/// The position of an error about `name` in the statement at `span`: the first occurrence of the name there, or
/// the start of the statement.
fn locate(text: &str, name: &str, span: Span) -> Span {
    mc_syntax::identifiers(text)
        .into_iter()
        .find(|(n, sp)| n == name && sp.start >= span.start && sp.start < span.end)
        .map_or(Span { start: span.start, end: span.start + 1 }, |(_, sp)| sp)
}

