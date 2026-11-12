//! The HTML export: one file with inline CSS, no scripts and no external links. Headings, paragraphs and items are
//! HTML elements; code cells are `<pre>` blocks with syntax colouring (the light theme's colours); outputs and
//! typeset inputs keep their characters as text in the code font, with every square root, fraction and power as an
//! inline MathML `<math>` element (`msqrt`, `mfrac`, `msup` over `mi`/`mn`/`mo` tokens), as the notebook draws
//! them; pictures are `<img>` elements with data URIs or files in a folder next to the HTML file.
use crate::model::{code_spans, heading_level, parts, Out, Part};
use crate::{base64, Images, Options};
use mc_boxes::{Item, Node, Row};
use crate::gzip::{picture_size, read_picture};
use mc_nbfile::images::{resolve, ImageFormat};
use mc_nbfile::{Kind, Notebook};
use mc_syntax::TokenClass;
use std::collections::HashMap;
use std::fmt::Write as _;
use std::io;
use std::path::{Path, PathBuf};

/// The style sheet: the notebook's light theme.
const CSS: &str = "\
:root { --text: #383a42; --weak: #696c77; --faint: #a0a1a7; --border: #dbdbdc; --code-bg: #f6f6f7; --init-bg: #eef1fb;
  --code: 'DejaVu Sans Mono', Consolas, Menlo, monospace; --prose: 'Noto Sans', 'Segoe UI', Helvetica, Arial, sans-serif; }
body { margin: 0; background: #ffffff; color: var(--text); font-family: var(--prose); font-size: 15px; line-height: 1.45; }
main { max-width: 52rem; margin: 0 auto; padding: 2rem 1.25rem 4rem; }
h1, h2, h3, h4, h5, .subtitle, .subsubtitle { font-weight: 700; margin: 1.4em 0 0.4em; line-height: 1.2; }
h1 { font-size: 2.4em; margin-top: 0.6em; } h2 { font-size: 1.9em; } h2.subchapter { font-size: 1.6em; }
h3 { font-size: 1.55em; border-top: 1px solid var(--border); padding-top: 0.4em; } h4 { font-size: 1.3em; } h5 { font-size: 1.15em; }
.subtitle { font-size: 1.5em; font-weight: 400; margin-top: 0.1em; } .subsubtitle { font-size: 1.1em; font-weight: 400; margin-top: 0.1em; }
p, li { white-space: pre-wrap; } p { margin: 0.5em 0 0.5em 2.5rem; }
ul.items { list-style: none; margin: 0.3em 0; padding: 0 0 0 2.5rem; }
li { margin: 0.2em 0; padding-left: 1.6em; } li.l2 { margin-left: 1.5rem; } li.l3 { margin-left: 3rem; }
li .mark { display: inline-block; min-width: 1.2em; margin-left: -1.6em; padding-right: 0.4em; color: var(--weak); }
code, pre, .out { font-family: var(--code); font-size: 13px; }
p code, li code { background: var(--code-bg); padding: 0 0.2em; border-radius: 3px; }
pre { margin: 0.6em 0 0.3em 2.5rem; padding: 0.45em 0.7em; white-space: pre-wrap; overflow-wrap: anywhere; line-height: 1.4; }
pre.input { background: var(--code-bg); font-weight: 600; border-radius: 4px; }
pre.init { background: var(--init-bg); font-weight: 700; border-radius: 4px; }
pre.program { border-top: 1px solid var(--border); border-bottom: 1px solid var(--border); }
.out { margin: 0.15em 0 0.15em 2.5rem; padding: 0 0.7em; white-space: pre-wrap; overflow-wrap: anywhere; }
.value { text-align: right; } .print { text-align: left; }
.failure { color: #ca1243; font-family: var(--prose); font-size: 13px; }
.note { color: #986801; font-family: var(--prose); font-size: 13px; }
.formula { text-align: center; margin: 0.6em 0; font-family: var(--code); }
.picture { text-align: right; } .picture img, .picture svg { max-width: 100%; height: auto; }
.missing { display: inline-block; border: 1px solid var(--faint); color: var(--weak); padding: 1em; font-family: var(--prose); font-size: 13px; }
math { font-size: 1em; math-style: normal; font-family: 'STIX Two Math', 'Cambria Math', math; }
math mi, math mn, math mo, math mtext { font-family: var(--code); }
.k { color: #a626a4; } .n { color: #986801; } .s { color: #50a14f; } .c { color: #a0a1a7; font-style: italic; }
.l { color: #0184bc; } .p { color: #0184bc; font-style: italic; } .u { color: #4078f2; } .h { color: #ca1243; }
.e { color: #e45649; background: #fbe4e2; } .b { color: #ca1243; font-weight: 700; } .ph { color: #a0a1a7; } .x { color: #a0a1a7; }
";

/// `s` with the HTML special characters escaped (for text and attribute values).
pub(crate) fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

/// The CSS class of a token class (`None`: the default text colour).
fn class(c: TokenClass) -> Option<&'static str> {
    use TokenClass::*;
    Some(match c {
        Operator | Global => return None,
        Undefined => "u",
        Local => "l",
        PatternVar => "p",
        Shadowing => "h",
        Keyword => "k",
        Constant | Number => "n",
        String => "s",
        Comment => "c",
        Placeholder => "ph",
        SyntaxError => "e",
        UnmatchedBracket => "b",
    })
}

/// The faint multiplication sign of a space that multiplies.
const TIMES: &str = "<span class=\"x\">\u{d7}</span>";

/// The text `src[a..b]` between two tokens, escaped, with the sign in place of each space of `marks` in it.
fn between(src: &str, a: usize, b: usize, marks: &[usize]) -> String {
    let mut out = String::new();
    let mut at = a;
    for &m in marks.iter().filter(|&&m| m >= a && m < b) {
        out.push_str(&escape(&src[at..m]));
        out.push_str(TIMES);
        at = m + 1;
    }
    out.push_str(&escape(&src[at..b]));
    out
}

/// Source text with syntax colouring, escaped; a space that multiplies shows the faint sign the notebook draws.
fn coloured(src: &str) -> String {
    let mut out = String::with_capacity(src.len() * 2);
    let marks = crate::model::product_marks(src);
    let mut at = 0;
    for (range, c) in mc_syntax::classify(src) {
        if range.start < at || range.end > src.len() {
            continue;
        }
        out.push_str(&between(src, at, range.start, &marks));
        match class(c) {
            Some(k) => {
                let _ = write!(out, "<span class=\"{k}\">{}</span>", escape(&src[range.clone()]));
            }
            None => out.push_str(&escape(&src[range.clone()])),
        }
        at = range.end;
    }
    out.push_str(&between(src, at, src.len(), &marks));
    out
}

/// Prose with its code spans as `<code>`, escaped.
fn prose(text: &str) -> String {
    code_spans(text)
        .into_iter()
        .map(|(t, code)| if code { format!("<code>{}</code>", escape(&t)) } else { escape(&t) })
        .collect()
}

/// A row as text with its boxes as inline MathML: characters stay text (the code font of the block), each box is
/// one `<math>` element.
pub(crate) fn row_html(row: &Row) -> String {
    row_html_marked(row, false)
}

/// [`row_html`]; with `products`, a space of the top row that multiplies (typeset input) shows the faint sign.
fn row_html_marked(row: &Row, products: bool) -> String {
    // the top row as text, a box standing as a one-letter name (an operand): the items whose space multiplies
    let marked: Vec<usize> = if products {
        let shadow: String = row.0.iter().map(|it| if let Item::Char(c) = it { *c } else { 'x' }).collect();
        let marks = crate::model::product_marks(&shadow);
        shadow.char_indices().enumerate().filter(|(_, (b, _))| marks.contains(b)).map(|(k, _)| k).collect()
    } else {
        vec![]
    };
    let mut out = String::new();
    let mut text = String::new();
    for (k, it) in row.0.iter().enumerate() {
        match it {
            Item::Char(' ') if marked.contains(&k) => {
                out.push_str(&escape(&text));
                text.clear();
                out.push_str(TIMES);
            }
            Item::Char(c) => text.push(*c),
            Item::Placeholder => text.push('\u{25a1}'),
            Item::Box(b) => {
                out.push_str(&escape(&text));
                text.clear();
                out.push_str("<math>");
                node_mathml(b, &mut out);
                out.push_str("</math>");
            }
        }
    }
    out.push_str(&escape(&text));
    out
}

/// The MathML of a slot: an `mrow` of tokens and boxes.
fn row_mathml(row: &Row, out: &mut String) {
    out.push_str("<mrow>");
    let items = &row.0;
    let mut i = 0;
    while i < items.len() {
        match &items[i] {
            Item::Box(b) => {
                node_mathml(b, out);
                i += 1;
            }
            Item::Placeholder => {
                out.push_str("<mi>\u{25a1}</mi>");
                i += 1;
            }
            Item::Char(c) => {
                let c = *c;
                let run = |pred: &dyn Fn(char) -> bool| {
                    let mut j = i;
                    let mut s = String::new();
                    while let Some(Item::Char(d)) = items.get(j) {
                        if !pred(*d) {
                            break;
                        }
                        s.push(*d);
                        j += 1;
                    }
                    (s, j)
                };
                if c.is_ascii_digit() {
                    let (s, j) = run(&|d: char| d.is_ascii_digit() || d == '.');
                    let _ = write!(out, "<mn>{}</mn>", escape(&s));
                    i = j;
                } else if c.is_alphabetic() || c == '_' || c == '$' {
                    let (s, j) = run(&|d: char| d.is_alphanumeric() || d == '_' || d == '$');
                    let _ = write!(out, "<mi mathvariant=\"normal\">{}</mi>", escape(&s));
                    i = j;
                } else if c.is_whitespace() {
                    out.push_str("<mspace width=\"0.3em\"></mspace>");
                    i += 1;
                } else if c == '"' {
                    // a string literal as text
                    let mut j = i + 1;
                    let mut s = String::from('"');
                    while let Some(Item::Char(d)) = items.get(j) {
                        s.push(*d);
                        j += 1;
                        if *d == '"' {
                            break;
                        }
                    }
                    let _ = write!(out, "<mtext>{}</mtext>", escape(&s));
                    i = j;
                } else {
                    let _ = write!(out, "<mo>{}</mo>", escape(&c.to_string()));
                    i += 1;
                }
            }
        }
    }
    out.push_str("</mrow>");
}

/// The MathML of a box.
fn node_mathml(node: &Node, out: &mut String) {
    match node {
        Node::Fraction { num, den } => {
            out.push_str("<mfrac>");
            row_mathml(num, out);
            row_mathml(den, out);
            out.push_str("</mfrac>");
        }
        Node::Superscript { base, exp } => {
            out.push_str("<msup>");
            row_mathml(base, out);
            row_mathml(exp, out);
            out.push_str("</msup>");
        }
        Node::Radical { radicand, index: None } => {
            out.push_str("<msqrt>");
            row_mathml(radicand, out);
            out.push_str("</msqrt>");
        }
        Node::Radical { radicand, index: Some(index) } => {
            out.push_str("<mroot>");
            row_mathml(radicand, out);
            row_mathml(index, out);
            out.push_str("</mroot>");
        }
        Node::Subscript { base, index } => {
            out.push_str("<msub>");
            row_mathml(base, out);
            row_mathml(index, out);
            out.push_str("</msub>");
        }
        other => {
            let text = mc_boxes::linear_text(&Row::boxed(other.clone()));
            let _ = write!(out, "<mtext>{}</mtext>", escape(&text));
        }
    }
}

/// Characters kept as they are in a URL path segment.
fn url_segment(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            let _ = write!(out, "%{b:02X}");
        }
    }
    out
}

/// Where pictures go and what has been placed there already.
struct Pictures<'a> {
    base: &'a Path,
    mode: Images,
    /// the folder for [`Images::Folder`] (created on first use) and its name relative to the HTML file
    folder: PathBuf,
    folder_name: String,
    /// source file to the name it was copied to
    copied: HashMap<PathBuf, String>,
    /// drawings inlined so far (their ids get a prefix of their own)
    inlined: usize,
}

impl Pictures<'_> {
    /// The `<img>`, the inline drawing (or a placeholder frame) for the picture `path` at `size` points. A plot
    /// is its SVG (the sibling `mc-graphics` wrote next to its PNG): inline when pictures are embedded, so it is
    /// sharp at any zoom and costs no base64; its light colours are kept.
    fn html(&mut self, path: &str, size: Option<(u32, u32)>) -> io::Result<String> {
        let mut file = resolve(path, Some(self.base));
        if ImageFormat::of(&file) == Some(ImageFormat::Png) {
            let svg = file.with_extension("svg");
            if std::fs::read(&svg).is_ok_and(|b| mc_nbfile::embed::is_ours(&b)) {
                file = svg;
            }
        }
        let format = ImageFormat::of(&file);
        if let (Images::Embed, Some(ImageFormat::Svg), Ok(bytes)) = (self.mode, format, read_picture(&file)) {
            if mc_nbfile::embed::is_ours(&bytes) {
                let (w, h) = size.or_else(|| picture_size(&file, &bytes)).unwrap_or((360, 225));
                self.inlined += 1;
                if let Some(svg) = inline_svg(&String::from_utf8_lossy(&bytes), self.inlined, w, h, path) {
                    return Ok(svg);
                }
            }
        }
        let bytes = match (format, read_picture(&file)) {
            (Some(f), Ok(b)) => (f, b),
            _ => return Ok(format!("<div class=\"missing\">picture not found: {}</div>", escape(path))),
        };
        let (w, h) = size.or_else(|| picture_size(&file, &bytes.1)).unwrap_or((360, 225));
        let src = match self.mode {
            Images::Embed => {
                let mime = match bytes.0 {
                    ImageFormat::Png => "image/png",
                    ImageFormat::Svg => "image/svg+xml",
                };
                format!("data:{mime};base64,{}", base64(&bytes.1))
            }
            Images::Folder => {
                let name = match self.copied.get(&file) {
                    Some(n) => n.clone(),
                    None => {
                        std::fs::create_dir_all(&self.folder)?;
                        // a compressed drawing is written as the plain `.svg` it holds
                        let named = if file.extension().is_some_and(|e| e.eq_ignore_ascii_case("svgz")) { file.with_extension("svg") } else { file.clone() };
                        let stem = named.file_name().map_or_else(|| "picture".into(), |n| n.to_string_lossy().to_string());
                        let mut name = stem.clone();
                        let mut k = 1;
                        while self.copied.values().any(|v| *v == name) {
                            k += 1;
                            name = format!("{k}-{stem}");
                        }
                        std::fs::write(self.folder.join(&name), &bytes.1)?;
                        self.copied.insert(file.clone(), name.clone());
                        name
                    }
                };
                format!("{}/{}", url_segment(&self.folder_name), url_segment(&name))
            }
        };
        Ok(format!(
            "<img src=\"{}\" width=\"{w}\" height=\"{h}\" alt=\"{}\">",
            escape(&src),
            escape(path)
        ))
    }
}

/// A drawing of `mc-graphics` as an inline `<svg>` element `w` x `h` CSS pixels: its ids (glyphs, clip paths)
/// prefixed with `p<n>-` so several drawings in one page do not share them, the namespace declarations (which HTML
/// does not need) left out, labelled with `path`. `None` when it does not have the shape this crate writes.
fn inline_svg(svg: &str, n: usize, w: u32, h: u32, path: &str) -> Option<String> {
    let start = svg.find("<svg")?;
    let end = start + svg[start..].find('>')?;
    let mut tag = svg[start..end].to_string();
    for ns in [r#" xmlns="http://www.w3.org/2000/svg""#, r#" xmlns:xlink="http://www.w3.org/1999/xlink""#] {
        tag = tag.replace(ns, "");
    }
    let set = |tag: &str, name: &str, v: u32| -> Option<String> {
        let at = tag.find(&format!(" {name}=\""))? + name.len() + 3;
        let close = at + tag[at..].find('"')?;
        Some(format!("{}{v}{}", &tag[..at], &tag[close..]))
    };
    let tag = set(&set(&tag, "width", w)?, "height", h)?;
    let body = svg[end..]
        .replace(" id=\"", &format!(" id=\"p{n}-"))
        .replace("href=\"#", &format!("href=\"#p{n}-"))
        .replace("url(#", &format!("url(#p{n}-"));
    Some(format!("{tag} role=\"img\" aria-label=\"{}\"{}", escape(path), body.trim_end()))
}

/// The HTML document (see [`crate::to_html`]).
pub(crate) fn document(nb: &Notebook, base: &Path, out: &Path, opts: &Options) -> io::Result<String> {
    let title = opts
        .title
        .clone()
        .or_else(|| crate::model::first_title(nb))
        .or_else(|| out.file_stem().map(|s| s.to_string_lossy().to_string()))
        .unwrap_or_else(|| "Notebook".into());
    let stem = out.file_stem().map_or_else(|| "notebook".into(), |s| s.to_string_lossy().to_string());
    let folder_name = format!("{stem}.files");
    let folder = out.parent().unwrap_or(Path::new("")).join(&folder_name);
    let mut pics = Pictures { base, mode: opts.images, folder, folder_name, copied: HashMap::new(), inlined: 0 };
    let mut h = String::new();
    h.push_str("<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n");
    h.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n");
    h.push_str("<meta name=\"generator\" content=\"MathCAS\">\n");
    let _ = writeln!(h, "<title>{}</title>", escape(&title));
    let _ = writeln!(h, "<style>\n{CSS}</style>\n</head>\n<body>\n<main>");
    let mut in_list = false;
    for part in parts(nb) {
        let is_item = matches!(part, Part::Item { .. });
        if in_list && !is_item {
            h.push_str("</ul>\n");
            in_list = false;
        }
        match part {
            Part::Heading { kind, text } => match heading_level(kind) {
                Some(n) => {
                    let cls = if kind == Kind::Subchapter { " class=\"subchapter\"" } else { "" };
                    let _ = writeln!(h, "<h{n}{cls}>{}</h{n}>", prose(&text));
                }
                None => {
                    let _ = writeln!(h, "<p class=\"{}\">{}</p>", kind.name(), prose(&text));
                }
            },
            Part::Para { text } => {
                let _ = writeln!(h, "<p>{}</p>", prose(&text));
            }
            Part::Item { level, number, bullet, text } => {
                if !in_list {
                    h.push_str("<ul class=\"items\">\n");
                    in_list = true;
                }
                let mark = match (number, bullet) {
                    (Some(n), _) => n,
                    (None, true) => "\u{25aa}".into(),
                    (None, false) => String::new(),
                };
                let _ = writeln!(h, "<li class=\"l{level}\"><span class=\"mark\">{}</span>{}</li>", escape(&mark), prose(&text));
            }
            Part::Formula { text } => {
                let _ = writeln!(h, "<div class=\"formula\">{}</div>", row_html(&mc_typeset::math_text(&text).row));
            }
            Part::Program { text } => {
                let _ = writeln!(h, "<pre class=\"program\">{}</pre>", escape(&text));
            }
            Part::Code { source, init, typeset } => {
                let cls = if init { "init" } else { "input" };
                let body = if typeset {
                    row_html_marked(&mc_boxes::restrict(&mc_boxes::from_text(&source)), true)
                } else {
                    coloured(&source)
                };
                let _ = writeln!(h, "<pre class=\"{cls}\">{body}</pre>");
            }
            Part::Out(o) => match o {
                Out::Value(text) => {
                    let t = mc_typeset::math_text(&text);
                    if t.message {
                        let _ = writeln!(h, "<div class=\"out failure\">{}</div>", escape(&text));
                    } else {
                        let _ = writeln!(h, "<div class=\"out value\">{}</div>", row_html(&t.row));
                    }
                }
                Out::Picture { path, size } => {
                    let img = pics.html(&path, size)?;
                    let _ = writeln!(h, "<div class=\"out picture\">{img}</div>");
                }
                Out::Print(text) => {
                    let _ = writeln!(h, "<div class=\"out print\">{}</div>", escape(&text));
                }
                Out::Failure(text) => {
                    let _ = writeln!(h, "<div class=\"out failure\">{}</div>", escape(&text));
                }
                Out::Note(text) => {
                    let _ = writeln!(h, "<div class=\"out note\">{}</div>", escape(&text));
                }
            },
        }
    }
    if in_list {
        h.push_str("</ul>\n");
    }
    h.push_str("</main>\n</body>\n</html>\n");
    Ok(h)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boxes_become_mathml() {
        let t = mc_typeset::math_text("x^2 + 1/3");
        let h = row_html(&t.row);
        assert!(h.contains("<msup><mrow><mi mathvariant=\"normal\">x</mi></mrow><mrow><mn>2</mn></mrow></msup>"), "{h}");
        assert!(h.contains("<mfrac><mrow><mn>1</mn></mrow><mrow><mn>3</mn></mrow></mfrac>"), "{h}");
        let r = row_html(&mc_typeset::math_text("sqrt(a < b)").row);
        assert!(r.contains("<msqrt>") && r.contains("<mo>&lt;</mo>"), "{r}");
    }

    #[test]
    fn a_space_that_multiplies_shows_the_sign() {
        let x = "<span class=\"x\">\u{d7}</span>";
        assert_eq!(coloured("m c"), format!("m{x}c"));
        // the middle space of a wider gap; an operator's spaces stay
        assert_eq!(coloured("a   b + c"), format!("a {x} b + c"));
        assert!(!coloured("f(x) + 1").contains("class=\"x\""));
        // typeset input: beside a box
        let row = mc_boxes::from_text("2 ((sqrt(x))) + a b");
        let h = super::row_html_marked(&row, true);
        assert_eq!(h.matches("class=\"x\"").count(), 2, "{h}");
        assert!(!row_html(&row).contains("class=\"x\""));
    }

    #[test]
    fn code_is_coloured_and_escaped() {
        let c = coloured("let s = \"<&>\" # note");
        assert!(c.contains("<span class=\"k\">let</span>"), "{c}");
        assert!(c.contains("<span class=\"s\">&quot;&lt;&amp;&gt;&quot;</span>"), "{c}");
        assert!(c.contains("<span class=\"c\"># note</span>"), "{c}");
    }

    #[test]
    fn url_segments_are_encoded() {
        assert_eq!(url_segment("a b#.png"), "a%20b%23.png");
    }
}
