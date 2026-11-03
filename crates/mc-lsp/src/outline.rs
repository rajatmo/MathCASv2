//! The definitions of one text, read from its syntax tree: the document outline, and what go to definition,
//! hover and completion fall back on when the text does not resolve (a syntax error while typing). The parser
//! recovers after an error, so the tree holds every statement it could read.
use mc_syntax::ast::{BinOp, Expr, ExprKind, ImportKind, Program, Stmt, StmtKind};
use mc_syntax::Span;

/// What an [`Item`] defines.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemKind {
    /// `fn f(...) = ...`
    Function,
    /// `rule f(...) := ...` or `f(x_) := ...`
    Rule,
    /// `sym s`
    Symbol,
    /// `x = v`, `x := v`, `let x = v`, `var x = v`
    Value,
    /// `module M { ... }`
    Module,
}

/// One definition statement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    /// the defined name as written
    pub name: String,
    /// what it defines
    pub kind: ItemKind,
    /// marked `pub` (in a module body)
    pub public: bool,
    /// the statement
    pub span: Span,
    /// the name in the statement
    pub name_span: Span,
    /// the definitions of a module body
    pub children: Vec<Item>,
}

/// An import statement: the module path, what it brings in, and the statement's span.
#[derive(Clone, Debug)]
pub struct Import {
    /// the imported module
    pub module: String,
    /// `import M`, `import M.{...}`, `import M except ...`
    pub kind: ImportKind,
    /// the statement
    pub span: Span,
}

/// The definitions of a parsed text, in order. `identifiers` are the text's name tokens
/// ([`mc_syntax::identifiers`]), used to place names.
pub fn items(program: &Program, identifiers: &[(String, Span)]) -> Vec<Item> {
    program.stmts.iter().filter_map(|s| item(s, false, identifiers)).collect()
}

fn item(s: &Stmt, public: bool, ids: &[(String, Span)]) -> Option<Item> {
    let (name, kind, children) = match &s.kind {
        StmtKind::Pub(inner) => {
            let mut it = item(inner, true, ids)?;
            it.span = s.span;
            return Some(it);
        }
        StmtKind::Fn { name, .. } => (name.clone(), ItemKind::Function, vec![]),
        StmtKind::Rule { lhs, .. } => (head_name(lhs)?, ItemKind::Rule, vec![]),
        StmtKind::Sym { name, .. } => (name.clone(), ItemKind::Symbol, vec![]),
        StmtKind::Let { name, .. } => (name.clone(), ItemKind::Value, vec![]),
        StmtKind::Module { name, body } => {
            (name.clone(), ItemKind::Module, body.iter().filter_map(|b| item(b, false, ids)).collect())
        }
        StmtKind::Expr(Expr { kind: ExprKind::Binary(BinOp::Set | BinOp::SetDelayed, l, _), .. }) => match &l.kind {
            ExprKind::Name(n) => (n.clone(), ItemKind::Value, vec![]),
            _ => (head_name(l)?, ItemKind::Rule, vec![]),
        },
        StmtKind::Expr(_) | StmtKind::Import { .. } => return None,
    };
    let name_span = locate(ids, &name, s.span);
    Some(Item { name, kind, public, span: s.span, name_span, children })
}

/// The head name of a definition's left side: `f` of `f(x_)`, also under `when`.
fn head_name(e: &Expr) -> Option<String> {
    let mut e = e;
    // a few levels at most: one per `when`
    for _ in 0..8 {
        match &e.kind {
            ExprKind::Binary(BinOp::When, l, _) => e = l,
            ExprKind::Call(h, _) => match &h.kind {
                ExprKind::Name(n) => return Some(n.clone()),
                _ => return None,
            },
            ExprKind::Name(n) => return Some(n.clone()),
            _ => return None,
        }
    }
    None
}

/// The first name token `name` inside `span`, or the start of the span.
pub fn locate(ids: &[(String, Span)], name: &str, span: Span) -> Span {
    // the tokens are in text order: start at the first one inside the span
    let first = ids.partition_point(|(_, sp)| sp.start < span.start);
    ids[first..]
        .iter()
        .take_while(|(_, sp)| sp.start < span.end)
        .find(|(n, _)| n == name)
        .map_or(Span { start: span.start, end: span.start }, |(_, sp)| *sp)
}

/// The import statements of a text: at the top level, and in module bodies (with the module's name).
pub fn imports(program: &Program) -> Vec<(Option<String>, Import)> {
    let mut out = vec![];
    for s in &program.stmts {
        match &s.kind {
            StmtKind::Import { path, kind } => out.push((None, Import { module: path.clone(), kind: kind.clone(), span: s.span })),
            StmtKind::Module { name, body } => {
                for b in body {
                    if let StmtKind::Import { path, kind } = &b.kind {
                        out.push((Some(name.clone()), Import { module: path.clone(), kind: kind.clone(), span: b.span }));
                    }
                }
            }
            _ => {}
        }
    }
    out
}

/// Whether an import of the given kind brings in `name`.
pub fn imports_name(kind: &ImportKind, name: &str) -> bool {
    match kind {
        ImportKind::All => true,
        ImportKind::Only(names) => names.iter().any(|n| n == name),
        ImportKind::Except(names) => !names.iter().any(|n| n == name),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outline_of_a_text() {
        let src = "# doc\nfn f(x) = x\nrule g(a_) when a > 0 := a\nh(x_) := 1\nc = 2\nsym s\nmodule M {\n  pub fn p(x) = x\n  let q = 1\n}\nimport M.{p}\nf(1)\n";
        let parsed = mc_syntax::parse(src);
        assert!(parsed.errors.is_empty());
        let ids = mc_syntax::identifiers(src);
        let items = items(&parsed.program, &ids);
        let names: Vec<(&str, ItemKind)> = items.iter().map(|i| (i.name.as_str(), i.kind)).collect();
        assert_eq!(
            names,
            [
                ("f", ItemKind::Function),
                ("g", ItemKind::Rule),
                ("h", ItemKind::Rule),
                ("c", ItemKind::Value),
                ("s", ItemKind::Symbol),
                ("M", ItemKind::Module)
            ]
        );
        let m = &items[5];
        assert_eq!(m.children.len(), 2);
        assert!(m.children[0].public && !m.children[1].public);
        assert_eq!(&src[m.children[0].name_span.start as usize..m.children[0].name_span.end as usize], "p");
        let imps = imports(&parsed.program);
        assert_eq!(imps.len(), 1);
        assert!(imports_name(&imps[0].1.kind, "p") && !imports_name(&imps[0].1.kind, "q"));
    }
}
