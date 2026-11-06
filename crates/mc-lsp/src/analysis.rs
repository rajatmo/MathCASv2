//! The open documents and what the server says about them: diagnostics, definitions, hover, completion and the
//! outline. Every answer reads the document afresh (parse with recovery, then the loader with a recorder), so it
//! always reflects the current buffers; reading a file of a few thousand lines takes milliseconds.
//!
//! Imports are resolved by `mc-load` exactly as the engine does, through an overlay that serves open buffers
//! before the file system: an unsaved change to a module file is seen by the files importing it.
use crate::library::{self, Library, Signatures};
use crate::outline::{self, Item, ItemKind};
use crate::text::{code_view, is_notebook, path_to_uri, uri_to_path, Lines};
use mc_api::{RefStore, Store, Sym};
use mc_load::{stdlib, DefinitionKind, FileSystem, LoadError, Loader, Origin, Sites, Source, SourceProvider};
use mc_syntax::ast::Program;
use mc_syntax::{Diagnostic, Span};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::io;
use std::path::{Path, PathBuf};

/// The protocol's symbol and completion kinds used here.
mod kind {
    pub const SYMBOL_MODULE: u32 = 2;
    pub const SYMBOL_FUNCTION: u32 = 12;
    pub const SYMBOL_VARIABLE: u32 = 13;
    pub const SYMBOL_CONSTANT: u32 = 14;
    pub const COMPLETION_FUNCTION: u32 = 3;
    pub const COMPLETION_VARIABLE: u32 = 6;
    pub const COMPLETION_MODULE: u32 = 9;
    pub const COMPLETION_KEYWORD: u32 = 14;
    pub const COMPLETION_CONSTANT: u32 = 21;
}

/// An open document.
#[derive(Clone, Debug)]
pub struct Document {
    /// its version, as the client numbers them
    pub version: i64,
    /// its full text
    pub text: String,
}

/// The open documents, by URI, and the known names.
#[derive(Default)]
pub struct Workspace {
    docs: BTreeMap<String, Document>,
    library: Library,
}

/// One reading of a document.
struct Analysis {
    /// the code as the reader sees it (a notebook's non-code lines blanked; same byte offsets as the text)
    code: String,
    origin: Origin,
    /// the syntax errors of the code
    errors: Vec<Diagnostic>,
    /// the statements: of the code, or after a syntax error of the repaired code if that parses (see
    /// [`repair`]), else what the parser recovered
    program: Program,
    /// the name tokens of the code
    ids: Vec<(String, Span)>,
    /// the loader's error, if the code parsed and did not resolve
    error: Option<LoadError>,
    /// what the loader recorded on the code or the repaired code
    sites: Sites,
    /// whether the code or the repaired code resolved
    resolved: bool,
}

/// A definition site found for a name.
struct Target {
    origin: Origin,
    span: Span,
    name_span: Span,
}

/// A member of a module, for completion.
struct Member {
    name: String,
    detail: String,
    doc: Option<String>,
    function: bool,
}

/// Open buffers first, then the file system: what the loader reads module files through.
struct Overlay {
    files: HashMap<String, String>,
}

/// A path as a map key: on Windows paths compare without regard to case or separator.
fn path_key(p: &Path) -> String {
    let s = p.to_string_lossy();
    if cfg!(windows) {
        s.replace('/', "\\").to_lowercase()
    } else {
        s.into_owned()
    }
}

impl SourceProvider for Overlay {
    fn module(&self, name: &str, dir: &Path) -> io::Result<Option<Source>> {
        let path = dir.join(format!("{name}.mc"));
        match self.files.get(&path_key(&path)) {
            Some(text) => Ok(Some(Source { origin: Origin::file(&path), text: text.clone() })),
            None => FileSystem.module(name, dir),
        }
    }

    fn file(&self, path: &Path) -> io::Result<String> {
        match self.files.get(&path_key(path)) {
            Some(text) => Ok(text.clone()),
            None => FileSystem.file(path),
        }
    }
}

/// A protocol diagnostic (an error).
fn error(range: Value, message: &str) -> Value {
    json!({"range": range, "severity": 1, "source": "mathcas", "message": message})
}

impl Workspace {
    /// No documents.
    pub fn new() -> Workspace {
        Workspace::default()
    }

    /// Open or replace a document.
    pub fn open(&mut self, uri: &str, version: i64, text: String) {
        self.docs.insert(uri.to_string(), Document { version, text });
    }

    /// Close a document; whether it was open.
    pub fn close(&mut self, uri: &str) -> bool {
        self.docs.remove(uri).is_some()
    }

    /// An open document.
    pub fn document(&self, uri: &str) -> Option<&Document> {
        self.docs.get(uri)
    }

    /// The URIs of the open documents.
    pub fn uris(&self) -> Vec<String> {
        self.docs.keys().cloned().collect()
    }

    fn overlay(&self) -> Overlay {
        let files = self
            .docs
            .iter()
            .filter(|(uri, _)| !is_notebook(uri))
            .filter_map(|(uri, d)| Some((path_key(&uri_to_path(uri)?), d.text.clone())))
            .collect();
        Overlay { files }
    }

    /// The origin of a document: its file, importing from its folder; or, without a file, a name that imports
    /// only standard modules.
    fn origin(uri: &str) -> Origin {
        match uri_to_path(uri) {
            Some(p) => Origin::file(&p),
            None => Origin::new(uri, None),
        }
    }

    fn analyse(&self, uri: &str) -> Option<Analysis> {
        let doc = self.docs.get(uri)?;
        let code = code_view(&doc.text, is_notebook(uri));
        let origin = Workspace::origin(uri);
        let parsed = mc_syntax::parse(&code);
        let ids = mc_syntax::identifiers(&code);
        let errors = parsed.errors;
        let (mut program, mut error, mut sites, mut resolved) = (parsed.program, None, Sites::default(), false);
        let repaired = if errors.is_empty() { Some((code.clone(), None)) } else { repair(&code, &errors) };
        if let Some((text, tree)) = repaired {
            let store = RefStore::new();
            let mut loader = Loader::new(Box::new(self.overlay()));
            let known = |s: Sym| self.library.knows(&store.sym_name(s));
            let (read, recorded) = loader.read_recorded(&store, &text, &origin, &known);
            sites = recorded;
            match read {
                Ok(_) => resolved = true,
                // errors of a repaired text may be artefacts of the repair: only the real text's are reported
                Err(e) if errors.is_empty() => error = Some(e),
                Err(_) => {}
            }
            if let Some(tree) = tree {
                program = tree;
            }
        }
        Some(Analysis { code, origin, errors, program, ids, error, sites, resolved })
    }

    /// The text of a source the loader read: an open buffer, a file, or the embedded prelude or standard module.
    fn source_text(&self, origin: &Origin) -> Option<String> {
        if origin.dir.is_none() {
            if origin.name == stdlib::PRELUDE_ORIGIN {
                return Some(stdlib::PRELUDE.to_string());
            }
            return stdlib::MODULES.iter().find(|(n, _)| stdlib::module_origin(n) == origin.name).map(|(_, t)| t.to_string());
        }
        let path = PathBuf::from(&origin.name);
        let key = path_key(&path);
        for (uri, d) in &self.docs {
            if uri_to_path(uri).is_some_and(|p| path_key(&p) == key) {
                return Some(code_view(&d.text, is_notebook(uri)));
            }
        }
        FileSystem.file(&path).ok()
    }

    /// The protocol `Location` of a definition site, if it is in a file.
    fn location(&self, t: &Target) -> Option<Value> {
        let path = Path::new(&t.origin.name);
        if t.origin.dir.is_none() || !path.is_absolute() {
            return None;
        }
        let text = self.source_text(&t.origin)?;
        Some(json!({"uri": path_to_uri(path), "range": Lines::new(&text).range(t.name_span)}))
    }

    // ---------------------------------------------------------------------------------------------------------
    // diagnostics

    /// The diagnostics of a document: its syntax errors, or else its first resolution or load error.
    pub fn diagnostics(&self, uri: &str) -> Vec<Value> {
        let Some(an) = self.analyse(uri) else { return vec![] };
        let lines = Lines::new(&an.code);
        let mut out: Vec<Value> = an.errors.iter().map(|d| error(lines.range(d.span), &d.message)).collect();
        let Some(e) = &an.error else { return out };
        if e.in_input {
            out.extend(e.diagnostics.iter().map(|d| error(lines.range(d.span), &d.message)));
            if let Some(m) = &e.message {
                out.push(error(lines.range(Span::new(0, 0)), m));
            }
            return out;
        }
        // an error in a module file this one imports: shown at the import, with the place in that file
        let stem = Path::new(&e.origin).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let imports = outline::imports(&an.program);
        let site = imports
            .iter()
            .find(|(_, i)| i.module == stem)
            .or(imports.first())
            .map_or(Span::new(0, 0), |(_, i)| outline::locate(&an.ids, &i.module, i.span));
        let file_lines = Lines::new(&e.text);
        let (message, related) = match (&e.message, e.diagnostics.first()) {
            (Some(m), _) => (format!("{}: {m}", e.origin), None),
            (None, Some(d)) => {
                let (l, c) = file_lines.position(d.span.start as usize);
                let path = Path::new(&e.origin);
                let related = path.is_absolute().then(|| {
                    json!([{"location": {"uri": path_to_uri(path), "range": file_lines.range(d.span)}, "message": d.message}])
                });
                (format!("{}:{}:{}: {}", e.origin, l + 1, c + 1, d.message), related)
            }
            (None, None) => (format!("{}: error", e.origin), None),
        };
        let mut d = error(lines.range(site), &message);
        if let Some(r) = related {
            d["relatedInformation"] = r;
        }
        out.push(d);
        out
    }

    // ---------------------------------------------------------------------------------------------------------
    // definitions and hover

    /// The name token at byte `offset` (the cursor may also be just after it).
    fn token_at(an: &Analysis, offset: usize) -> Option<(String, Span)> {
        let off = offset as u32;
        let hit = |(_, sp): &&(String, Span)| sp.start <= off && off <= sp.end;
        an.ids.iter().find(|t| hit(t) && off < t.1.end).or_else(|| an.ids.iter().find(hit)).cloned()
    }

    /// The module item whose body holds byte `offset`.
    fn enclosing_module(items: &[Item], offset: u32) -> Option<&Item> {
        items.iter().find(|i| i.kind == ItemKind::Module && i.span.start <= offset && offset <= i.span.end)
    }

    /// The name at `offset` (as written, and its span) and its definition sites: through the loader's record, or,
    /// when the text did not resolve, the document's own definitions.
    fn lookup(&self, an: &Analysis, offset: usize) -> Option<(String, Span, String, Vec<Target>)> {
        let (written, span) = Workspace::token_at(an, offset)?;
        let off = span.start;
        let items = outline::items(&an.program, &an.ids);
        let import = outline::imports(&an.program).into_iter().find(|(_, i)| i.span.start <= off && off < i.span.end);
        let (resolved, kinds) = match &import {
            Some((_, i)) if i.module == written => (written.clone(), [DefinitionKind::Module, DefinitionKind::Module]),
            Some((_, i)) => (format!("{}.{written}", i.module), [DefinitionKind::Value, DefinitionKind::Value]),
            None => {
                let recorded = an.sites.resolution_at(&an.origin.name, off).and_then(|r| r.get(&written)).map(str::to_string);
                let fallback = || match Workspace::enclosing_module(&items, off) {
                    Some(m) if m.children.iter().any(|c| c.name == written) => format!("{}.{written}", m.name),
                    _ => written.clone(),
                };
                (recorded.unwrap_or_else(fallback), [DefinitionKind::Value, DefinitionKind::Module])
            }
        };
        let mut targets: Vec<Target> = vec![];
        for k in kinds {
            if targets.is_empty() {
                let found = an.sites.definitions_of(&resolved, k);
                targets.extend(found.map(|d| Target { origin: d.origin.clone(), span: d.span, name_span: d.name_span }));
            }
        }
        if targets.is_empty() && !an.resolved {
            // the document's own definitions, from its syntax tree
            let here = |i: &Item| Target { origin: an.origin.clone(), span: i.span, name_span: i.name_span };
            match resolved.split_once('.') {
                Some((m, f)) => {
                    let module = items.iter().find(|i| i.kind == ItemKind::Module && i.name == m);
                    targets.extend(module.into_iter().flat_map(|m| m.children.iter()).filter(|c| c.name == f).map(here));
                }
                None => targets.extend(items.iter().filter(|i| i.name == resolved).map(here)),
            }
        }
        Some((written, span, resolved, targets))
    }

    /// Go to definition: the definition sites of the name at a position.
    pub fn definition(&self, uri: &str, line: u32, character: u32) -> Value {
        let Some(an) = self.analyse(uri) else { return Value::Null };
        let offset = Lines::new(&an.code).offset(line, character);
        let Some((_, _, _, targets)) = self.lookup(&an, offset) else { return Value::Null };
        let locations: Vec<Value> = targets.iter().filter_map(|t| self.location(t)).collect();
        if locations.is_empty() {
            Value::Null
        } else {
            Value::Array(locations)
        }
    }

    /// Hover: the definition's signature line and the comment above it, or a library function's description.
    pub fn hover(&self, uri: &str, line: u32, character: u32) -> Value {
        let Some(an) = self.analyse(uri) else { return Value::Null };
        let lines = Lines::new(&an.code);
        let offset = lines.offset(line, character);
        let Some((written, span, resolved, targets)) = self.lookup(&an, offset) else { return Value::Null };
        let markdown = if let Some(t) = targets.first() {
            let Some(text) = self.source_text(&t.origin) else { return Value::Null };
            let (sig, doc) = Signatures::new(&text).at(t.span.start as usize, t.name_span.start as usize);
            let mut md = code_block(&sig);
            if let Some(doc) = doc {
                md.push_str(&format!("\n\n{doc}"));
            }
            if t.origin != an.origin {
                let file = Path::new(&t.origin.name).file_name().map_or(t.origin.name.clone(), |f| f.to_string_lossy().into_owned());
                md.push_str(&format!("\n\n*{file}*"));
            }
            md
        } else if let Some(e) = self.library.get(&resolved).or_else(|| self.library.get(&written)) {
            if let Some(h) = e.help.filter(|_| e.origin == library::Origin::Library) {
                return json!({"contents": {"kind": "markdown", "value": help_markdown(h)}, "range": lines.range(span)});
            }
            match e.origin {
                library::Origin::Prelude => {
                    let mut md = code_block(&e.detail);
                    if let Some(doc) = &e.doc {
                        md.push_str(&format!("\n\n{doc}"));
                    }
                    md + "\n\n*prelude*"
                }
                library::Origin::Library => format!("{}\n\n*library*", e.detail),
            }
        } else {
            return Value::Null;
        };
        json!({"contents": {"kind": "markdown", "value": markdown}, "range": lines.range(span)})
    }

    // ---------------------------------------------------------------------------------------------------------
    // completion

    /// The members of module `name` seen from this document at `offset`: a module of the document itself (all
    /// members inside it, the public ones outside), or else the module file `import name` would load.
    fn members(&self, an: &Analysis, name: &str, offset: u32) -> Vec<Member> {
        let items = outline::items(&an.program, &an.ids);
        if let Some(m) = items.iter().find(|i| i.kind == ItemKind::Module && i.name == name) {
            let inside = m.span.start <= offset && offset <= m.span.end;
            let sigs = Signatures::new(&an.code);
            return m
                .children
                .iter()
                .filter(|c| inside || c.public)
                .map(|c| member(&sigs, c))
                .collect();
        }
        let store = RefStore::new();
        let mut loader = Loader::new(Box::new(self.overlay()));
        let known = |s: Sym| self.library.knows(&store.sym_name(s));
        let (read, sites) = loader.read_recorded(&store, &format!("import {name}"), &an.origin, &known);
        if read.is_err() {
            return vec![];
        }
        if loader.module(name).is_none() {
            return vec![];
        }
        // the module's file, read once: its outline gives each member's kind and signature
        let Some(def) = sites.definitions_of(name, DefinitionKind::Module).next() else { return vec![] };
        let Some(text) = self.source_text(&def.origin) else { return vec![] };
        let ids = mc_syntax::identifiers(&text);
        let items = outline::items(&mc_syntax::parse(&text).program, &ids);
        let Some(m) = items.iter().find(|i| i.kind == ItemKind::Module && i.name == name) else { return vec![] };
        let sigs = Signatures::new(&text);
        m.children.iter().filter(|c| c.public).map(|c| member(&sigs, c)).collect()
    }

    /// Completion at a position: the members of `M` after `M.`; otherwise keywords, library and prelude names, the
    /// document's definitions, its modules and the names its imports bring in.
    pub fn completion(&self, uri: &str, line: u32, character: u32) -> Value {
        let Some(an) = self.analyse(uri) else { return Value::Null };
        let lines = Lines::new(&an.code);
        let offset = lines.offset(line, character);
        let before = &an.code[lines.line_start(offset)..offset];
        let member_item = |m: Member| {
            let k = if m.function { kind::COMPLETION_FUNCTION } else { kind::COMPLETION_VARIABLE };
            let mut v = json!({"label": m.name, "kind": k, "detail": m.detail});
            if let Some(doc) = m.doc {
                v["documentation"] = json!({"kind": "markdown", "value": doc});
            }
            v
        };
        if let Some(module) = qualifier(before) {
            let items: Vec<Value> = self.members(&an, module, offset as u32).into_iter().map(member_item).collect();
            return json!({"isIncomplete": false, "items": items});
        }

        let mut seen = HashSet::new();
        let mut out = vec![];
        let mut push = |label: &str, v: Value| {
            if seen.insert(label.to_string()) {
                out.push(v);
            }
        };
        let items = outline::items(&an.program, &an.ids);
        let off = offset as u32;
        let module = Workspace::enclosing_module(&items, off);
        let sigs = Signatures::new(&an.code);
        // the document's definitions, and inside a module its own members
        for it in module.into_iter().flat_map(|m| m.children.iter()).chain(items.iter()) {
            let k = match it.kind {
                ItemKind::Function | ItemKind::Rule => kind::COMPLETION_FUNCTION,
                ItemKind::Module => kind::COMPLETION_MODULE,
                ItemKind::Symbol => kind::COMPLETION_CONSTANT,
                ItemKind::Value => kind::COMPLETION_VARIABLE,
            };
            let (detail, _) = sigs.of(it);
            push(&it.name, json!({"label": it.name, "kind": k, "detail": detail}));
        }
        // imported names (the top-level imports, and those of the module around the cursor) and module names
        for (inside, imp) in outline::imports(&an.program) {
            if inside.is_some() && inside.as_deref() != module.map(|m| m.name.as_str()) {
                continue;
            }
            push(&imp.module, json!({"label": imp.module, "kind": kind::COMPLETION_MODULE, "detail": "module"}));
            for m in self.members(&an, &imp.module, u32::MAX) {
                if outline::imports_name(&imp.kind, &m.name) {
                    let label = m.name.clone();
                    push(&label, member_item(m));
                }
            }
        }
        for (name, e) in self.library.iter() {
            let function = e.help.map_or(
                e.origin == library::Origin::Prelude || e.detail.starts_with(&format!("{name}(")),
                |h| h.is_function(),
            );
            let k = if function { kind::COMPLETION_FUNCTION } else { kind::COMPLETION_CONSTANT };
            // the one-line usage of the documentation, with its page's usage lines and a link to the page
            let mut v = match e.help {
                Some(h) => json!({"label": name, "kind": k, "detail": h.line(),
                    "documentation": {"kind": "markdown", "value": help_markdown(h)}}),
                None => json!({"label": name, "kind": k, "detail": e.detail}),
            };
            if let (Some(doc), None) = (&e.doc, e.help) {
                v["documentation"] = json!({"kind": "markdown", "value": doc});
            }
            push(name, v);
        }
        for kw in mc_syntax::KEYWORDS {
            push(kw, json!({"label": kw, "kind": kind::COMPLETION_KEYWORD}));
        }
        // only the names matching the name being typed (as the notebook matches them: `mc_help::score`); typing on
        // narrows the list, so the client may keep filtering it
        let typed = mc_help::scan::word_before(&an.code, offset).map(|r| an.code[r].to_string()).unwrap_or_default();
        if !typed.is_empty() {
            out.retain(|v| v["label"].as_str().is_some_and(|l| mc_help::score(l, &typed).is_some()));
        }
        json!({"isIncomplete": false, "items": out})
    }

    // ---------------------------------------------------------------------------------------------------------
    // signature help

    /// Inside the brackets of a call of a documented function: its usage lines, the one fitting the arguments
    /// typed so far active, the parameter being typed highlighted (parameter labels as UTF-16 offsets).
    pub fn signature_help(&self, uri: &str, line: u32, character: u32) -> Value {
        let Some(an) = self.analyse(uri) else { return Value::Null };
        let lines = Lines::new(&an.code);
        let offset = lines.offset(line, character);
        for call in mc_help::scan::calls_at(&an.code, offset) {
            let simple = call.name.rsplit('.').next().unwrap_or(&call.name);
            let Some(h) = self.library.get(simple).and_then(|e| e.help) else { continue };
            let calls = h.calls();
            let active = h.best_call(call.arg);
            let signatures: Vec<Value> = calls
                .iter()
                .enumerate()
                .map(|(k, c)| {
                    let utf16 = |b: usize| c[..b].encode_utf16().count();
                    let params: Vec<Value> = mc_help::scan::params(c)
                        .into_iter()
                        .map(|r| json!({"label": [utf16(r.start), utf16(r.end)]}))
                        .collect();
                    json!({"label": c, "documentation": {"kind": "markdown", "value": h.text_of(k)},
                        "parameters": params})
                })
                .collect();
            let n = mc_help::scan::params(&calls[active]).len();
            let param = if call.arg < n { call.arg } else { n.saturating_sub(1) };
            return json!({"signatures": signatures, "activeSignature": active, "activeParameter": param});
        }
        Value::Null
    }

    // ---------------------------------------------------------------------------------------------------------
    // outline

    /// The document's symbols: functions, rules, symbols, values and modules (with their members).
    pub fn symbols(&self, uri: &str) -> Value {
        let Some(an) = self.analyse(uri) else { return Value::Null };
        let lines = Lines::new(&an.code);
        let items = outline::items(&an.program, &an.ids);
        let sigs = Signatures::new(&an.code);
        Value::Array(items.iter().map(|i| symbol(&sigs, &lines, i)).collect())
    }
}

/// The command that opens a function's page in the documentation window (the VS Code extension registers it).
pub const OPEN_DOCS: &str = "mathcas.openDocs";

/// The hover and completion text of a documented function: its usage calls, what each gives, and a link that opens
/// its page in the documentation window.
pub fn help_markdown(h: &mc_help::Entry) -> String {
    let calls = h.calls();
    let mut md = code_block(&calls.join("\n"));
    for (k, c) in calls.iter().enumerate() {
        let text = h.text_of(k);
        if text.is_empty() {
            continue;
        }
        if calls.len() > 1 {
            md.push_str(&format!("\n\n`{c}` {text}"));
        } else {
            md.push_str(&format!("\n\n{text}"));
        }
    }
    let args = percent_encode(&json!([h.name]).to_string());
    let what = if h.page { "Open the documentation page" } else { "Open in the documentation" };
    md.push_str(&format!("\n\n[{what}](command:{OPEN_DOCS}?{args})"));
    md
}

/// `s` with every byte outside the unreserved URI characters percent-encoded.
fn percent_encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// A module member for completion, from its definition.
fn member(sigs: &Signatures<'_>, it: &Item) -> Member {
    let (detail, doc) = sigs.of(it);
    Member { name: it.name.clone(), detail, doc, function: matches!(it.kind, ItemKind::Function | ItemKind::Rule) }
}

/// A protocol `DocumentSymbol`.
fn symbol(sigs: &Signatures<'_>, lines: &Lines<'_>, it: &Item) -> Value {
    let k = match it.kind {
        ItemKind::Function | ItemKind::Rule => kind::SYMBOL_FUNCTION,
        ItemKind::Symbol => kind::SYMBOL_CONSTANT,
        ItemKind::Value => kind::SYMBOL_VARIABLE,
        ItemKind::Module => kind::SYMBOL_MODULE,
    };
    let detail = match it.kind {
        ItemKind::Module => "module".to_string(),
        _ => sigs.of(it).0,
    };
    let children: Vec<Value> = it.children.iter().map(|c| symbol(sigs, lines, c)).collect();
    json!({
        "name": it.name,
        "detail": detail,
        "kind": k,
        "range": lines.range(it.span),
        "selectionRange": lines.range(it.name_span),
        "children": children,
    })
}

/// The code with its syntax errors blanked out (same byte offsets), and its syntax tree: so that the rest of a text
/// being edited still resolves, including the line being typed. The first round blanks each error's line from the
/// error on (`y = area(2) + Geo.` keeps `y = area(2) + Geo`); later rounds blank whole lines (an error at the end of
/// a line or of the input blanks the last line with text before it). `None` if a few rounds do not give a text
/// without syntax errors.
fn repair(code: &str, errors: &[Diagnostic]) -> Option<(String, Option<Program>)> {
    let mut text = code.to_string();
    let mut errors = errors.to_vec();
    for round in 0..4 {
        let mut bytes = std::mem::take(&mut text).into_bytes();
        for d in &errors {
            let mut at = (d.span.start as usize).min(bytes.len());
            let line_start = |at: usize| bytes[..at].iter().rposition(|&b| b == b'\n').map_or(0, |i| i + 1);
            let line_end = |at: usize| bytes[at..].iter().position(|&b| b == b'\n').map_or(bytes.len(), |i| at + i);
            if round > 0 {
                // the whole line, or the last line with text before it
                at = line_start(at);
                while at > 0 && bytes[at..line_end(at)].iter().all(u8::is_ascii_whitespace) {
                    at = line_start(at - 1);
                }
            }
            let end = line_end(at);
            bytes[at..end].iter_mut().for_each(|b| *b = b' ');
        }
        // error positions are character boundaries, so the bytes stay UTF-8
        text = String::from_utf8(bytes).ok()?;
        let parsed = mc_syntax::parse(&text);
        if parsed.errors.is_empty() {
            return Some((text, Some(parsed.program)));
        }
        errors = parsed.errors;
    }
    None
}

/// A fenced code block of MathCAS source.
fn code_block(src: &str) -> String {
    format!("```mathcas\n{src}\n```")
}

/// The module name `M` when the text before the cursor ends in `M.` or `M.partial`.
fn qualifier(before: &str) -> Option<&str> {
    let rest = before.trim_end_matches(|c: char| c.is_alphanumeric() || c == '_');
    let rest = rest.strip_suffix('.')?;
    let start = rest.trim_end_matches(|c: char| c.is_alphanumeric() || c == '_').len();
    let m = &rest[start..];
    m.chars().next().filter(|c| c.is_alphabetic() || *c == '_').map(|_| m)
}

#[cfg(test)]
mod tests {
    use super::{qualifier, repair};

    fn repaired(code: &str) -> Option<String> {
        let errors = mc_syntax::parse(code).errors;
        assert!(!errors.is_empty());
        let (text, _) = repair(code, &errors)?;
        assert_eq!(text.len(), code.len());
        Some(text)
    }

    #[test]
    fn repairs() {
        // the line being typed keeps what comes before the error
        assert_eq!(repaired("import Geo\ny = area(2) + Geo.\n").unwrap().lines().nth(1).unwrap().trim_end(), "y = area(2) + Geo");
        assert_eq!(repaired("y = 2 x\nz = 1").unwrap(), "y = 2  \nz = 1");
        // an error at the end of the input: the last line with text goes
        let t = repaired("θ = 1\ny = x +\n\n").unwrap();
        assert!(t.starts_with("θ = 1\n") && t[8..].trim().is_empty(), "{t:?}");
        // unbalanced brackets over several lines
        assert!(repaired("f(x) = x\ng(y) = (y +\n  2\nh = 3\n").is_some());
    }

    #[test]
    fn qualifiers() {
        assert_eq!(qualifier("x = Geo."), Some("Geo"));
        assert_eq!(qualifier("  Geo.ar"), Some("Geo"));
        assert_eq!(qualifier("f(θm.x"), Some("θm"));
        assert_eq!(qualifier("1."), None);
        assert_eq!(qualifier("x."), Some("x"));
        assert_eq!(qualifier("area"), None);
        assert_eq!(qualifier("(a)."), None);
    }
}
