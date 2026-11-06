//! `file_exists(path)` and `file_names(pattern)`, `file_names(pattern, dir)` (gap queue G52): testing for optional
//! data files before importing them. Patterns use `*` (any run of characters) and `?` (one character) in the file
//! name; a directory part is taken literally. Paths are relative to the working directory.
use mc_api::{Store, View, T};
use mc_eval::Res;
use mc_numeric::formula::{Def, Fx};

type R<'h> = Res<Option<T<'h>>>;

fn text<'h>(fx: &Fx<'_, '_, '_, 'h>, t: T<'h>) -> Option<String> {
    match fx.h.view(t) {
        View::Str(s) => Some(s.to_string()),
        _ => None,
    }
}

/// Does `name` match the glob `pat`?
fn glob(pat: &[char], name: &[char]) -> bool {
    match (pat.first(), name.first()) {
        (None, None) => true,
        (Some('*'), _) => glob(&pat[1..], name) || (!name.is_empty() && glob(pat, &name[1..])),
        (Some('?'), Some(_)) => glob(&pat[1..], &name[1..]),
        (Some(a), Some(b)) if a == b => glob(&pat[1..], &name[1..]),
        _ => false,
    }
}

fn file_exists<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    let Some(p) = text(fx, a[0]) else { return Ok(None) };
    Ok(Some(fx.bool(std::path::Path::new(&p).exists())))
}

fn file_names<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    let pattern = match a.first() {
        Some(&p) => match text(fx, p) {
            Some(s) => s,
            None => return Ok(None),
        },
        None => "*".to_string(),
    };
    let base = match a.get(1) {
        Some(&d) => match text(fx, d) {
            Some(s) => std::path::PathBuf::from(s),
            None => return Ok(None),
        },
        None => std::path::PathBuf::new(),
    };
    // a directory part of the pattern is literal
    let pp = std::path::Path::new(&pattern);
    let (dir, name_pat) = match (pp.parent(), pp.file_name()) {
        (Some(d), Some(n)) => (base.join(d), n.to_string_lossy().to_string()),
        _ => (base.clone(), pattern.clone()),
    };
    let listing = if dir.as_os_str().is_empty() { std::path::PathBuf::from(".") } else { dir.clone() };
    let pat: Vec<char> = name_pat.chars().collect();
    let mut out = vec![];
    if let Ok(rd) = std::fs::read_dir(&listing) {
        for e in rd.flatten() {
            let n = e.file_name().to_string_lossy().to_string();
            let chars: Vec<char> = n.chars().collect();
            if glob(&pat, &chars) {
                let shown = if dir.as_os_str().is_empty() { n } else { dir.join(&n).to_string_lossy().replace('\\', "/") };
                out.push(shown);
            }
            fx.tick(1)?;
        }
    }
    out.sort();
    let items: Vec<T<'h>> = out.iter().map(|s| fx.h.string(s)).collect();
    Ok(Some(fx.list(&items)))
}

/// The definitions of this module.
pub static DEFS: &[Def] = &[
    Def {
        name: "file_exists",
        arities: &[1],
        listable: true,
        exact: Some(file_exists),
        numeric: None,
        summary: "file_exists(path): true when the file or directory exists",
    },
    Def {
        name: "file_names",
        arities: &[0, 1, 2],
        listable: false,
        exact: Some(file_names),
        numeric: None,
        summary: "file_names(pattern), file_names(pattern, dir): the sorted names of the files matching a pattern with * and ?",
    },
];
