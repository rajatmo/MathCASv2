//! Text positions, URIs and the code of notebooks.
//!
//! The protocol counts columns in UTF-16 code units (its default position encoding); the syntax works on byte
//! offsets. [`Lines`] converts between the two. Document URIs are `file:` URIs, converted to paths for the loader
//! and back for locations in other files.
use mc_syntax::Span;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

/// The line starts of one text, for converting byte offsets to protocol positions and back.
pub struct Lines<'t> {
    text: &'t str,
    starts: Vec<usize>,
}

impl<'t> Lines<'t> {
    /// The lines of `text`.
    pub fn new(text: &'t str) -> Lines<'t> {
        let mut starts = vec![0];
        starts.extend(text.match_indices('\n').map(|(i, _)| i + 1));
        Lines { text, starts }
    }

    /// The protocol position (0-based line, UTF-16 column) of byte `offset` (clamped to the text, and moved back to
    /// a character boundary).
    pub fn position(&self, offset: usize) -> (u32, u32) {
        let mut offset = offset.min(self.text.len());
        while !self.text.is_char_boundary(offset) {
            offset -= 1;
        }
        let line = self.starts.partition_point(|&s| s <= offset) - 1;
        let col: usize = self.text[self.starts[line]..offset].chars().map(char::len_utf16).sum();
        (line as u32, col as u32)
    }

    /// The byte offset of a protocol position; a column past the end of its line means the end of the line (before
    /// its line break), a line past the end means the end of the text.
    pub fn offset(&self, line: u32, character: u32) -> usize {
        let Some(&start) = self.starts.get(line as usize) else { return self.text.len() };
        let end = self.line_end(line as usize);
        let mut units = 0usize;
        for (i, c) in self.text[start..end].char_indices() {
            if units >= character as usize {
                return start + i;
            }
            units += c.len_utf16();
        }
        end
    }

    /// The byte offset where line `line` ends, before `\n` or `\r\n`.
    fn line_end(&self, line: usize) -> usize {
        let end = self.starts.get(line + 1).map_or(self.text.len(), |&s| s - 1);
        if end > self.starts[line] && self.text.as_bytes()[end - 1] == b'\r' {
            end - 1
        } else {
            end
        }
    }

    /// The byte offset of the start of the line holding byte `offset`.
    pub fn line_start(&self, offset: usize) -> usize {
        self.starts[self.position(offset).0 as usize]
    }

    /// A protocol `Range` for a byte span. An empty span is widened to the character after it (when that is on the
    /// same line), so that an editor can show it.
    pub fn range(&self, span: Span) -> Value {
        let (start, mut end) = (span.start as usize, span.end as usize);
        if end <= start {
            end = start;
            if let Some(c) = self.text.get(start..).and_then(|r| r.chars().next()) {
                if c != '\n' && c != '\r' {
                    end = start + c.len_utf8();
                }
            }
        }
        let (sl, sc) = self.position(start);
        let (el, ec) = self.position(end);
        json!({"start": {"line": sl, "character": sc}, "end": {"line": el, "character": ec}})
    }
}

/// The local path of a `file:` URI.
pub fn uri_to_path(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    let (authority, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    let path = percent_decode(path)?;
    if cfg!(windows) {
        let p = path.replace('/', "\\");
        if !authority.is_empty() && authority != "localhost" {
            return Some(PathBuf::from(format!("\\\\{}{p}", percent_decode(authority)?)));
        }
        // `\c:\dir` -> `c:\dir`
        let b = p.as_bytes();
        let p = if b.len() >= 3 && b[0] == b'\\' && b[1].is_ascii_alphabetic() && b[2] == b':' { &p[1..] } else { &p[..] };
        Some(PathBuf::from(p))
    } else {
        Some(PathBuf::from(path))
    }
}

/// The `file:` URI of an absolute path, in the form editors send (drive letter in lower case, `:` encoded).
pub fn path_to_uri(path: &Path) -> String {
    let s = path.to_string_lossy();
    let mut p = s.replace('\\', "/");
    if let Some(unc) = p.strip_prefix("//") {
        let (host, rest) = unc.split_once('/').unwrap_or((unc, ""));
        return format!("file://{}/{}", percent_encode(host), percent_encode(rest));
    }
    let b = p.as_bytes();
    if b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':' {
        p = format!("/{}{}", p[..1].to_ascii_lowercase(), &p[1..]);
    }
    format!("file://{}", percent_encode(&p))
}

fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for &b in s.as_bytes() {
        if b.is_ascii_alphanumeric() || b"-._~/".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn percent_decode(s: &str) -> Option<String> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            let hex = s.get(i + 1..i + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// Whether a document is a notebook (`.mcn`).
pub fn is_notebook(uri: &str) -> bool {
    uri.to_ascii_lowercase().ends_with(".mcn")
}

/// The code of a document as the reader sees it, with the same byte offsets as the document. For a notebook the
/// header line (`#! mathcas 2 notebook`) and the lines of text and output blocks are blanked: a line `#% text` or
/// `#% output` starts a block that is not code, any other `#%` marker line (`#% cell`) starts code again. Marker
/// lines are comments and stay. A script is returned unchanged.
pub fn code_view(text: &str, notebook: bool) -> String {
    if !notebook {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut code = true;
    for (i, line) in text.split_inclusive('\n').enumerate() {
        let body = line.trim_end_matches(['\r', '\n']);
        let blank = if let Some(marker) = body.strip_prefix("#%") {
            code = !matches!(marker.split_whitespace().next(), Some("text" | "output"));
            false
        } else {
            (i == 0 && body.starts_with("#!")) || !code
        };
        if blank {
            out.extend(std::iter::repeat_n(' ', body.len()));
            out.push_str(&line[body.len()..]);
        } else {
            out.push_str(line);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf16_positions() {
        // `θ` is 2 bytes and 1 UTF-16 unit; `𝑥` is 4 bytes and 2 units
        let text = "aθ𝑥b\r\nsecond\nlast";
        let l = Lines::new(text);
        assert_eq!(l.position(0), (0, 0));
        assert_eq!(l.position(1), (0, 1));
        assert_eq!(l.position(3), (0, 2));
        assert_eq!(l.position(7), (0, 4));
        assert_eq!(l.position(2), (0, 1)); // inside `θ`: moved back
        assert_eq!(l.position(10), (1, 0));
        assert_eq!(l.offset(0, 4), 7);
        assert_eq!(l.offset(0, 2), 3);
        assert_eq!(l.offset(0, 99), 8); // before `\r\n`
        assert_eq!(l.offset(1, 3), 13);
        assert_eq!(l.offset(9, 0), text.len());
        assert_eq!(l.line_start(12), 10);
        assert_eq!(l.range(Span::new(1, 1))["end"]["character"], 2);
    }

    #[test]
    fn uris() {
        if cfg!(windows) {
            let p = uri_to_path("file:///c%3A/Users/a%20b/x.mc").unwrap();
            assert_eq!(p, PathBuf::from("c:\\Users\\a b\\x.mc"));
            assert_eq!(path_to_uri(Path::new("C:\\Users\\a b\\x.mc")), "file:///c%3A/Users/a%20b/x.mc");
            assert_eq!(uri_to_path("file:///C:/x/%CE%B8.mc").unwrap(), PathBuf::from("C:\\x\\θ.mc"));
            assert_eq!(uri_to_path("file://server/share/x.mc").unwrap(), PathBuf::from("\\\\server\\share\\x.mc"));
            assert_eq!(path_to_uri(Path::new("\\\\server\\share\\x.mc")), "file://server/share/x.mc");
        } else {
            assert_eq!(uri_to_path("file:///home/a%20b/x.mc").unwrap(), PathBuf::from("/home/a b/x.mc"));
            assert_eq!(path_to_uri(Path::new("/home/a b/θ.mc")), "file:///home/a%20b/%CE%B8.mc");
        }
        assert!(uri_to_path("untitled:Untitled-1").is_none());
        assert!(uri_to_path("file:///x%zz").is_none());
    }

    #[test]
    fn notebook_code() {
        let nb = "#! mathcas 2 notebook\n#% text\nSome *text* θ\n#% cell\nx = 1\n#% output\n1\n#% cell\ny = x\n";
        let code = code_view(nb, true);
        assert_eq!(code.len(), nb.len());
        let lines: Vec<&str> = code.lines().collect();
        assert_eq!(lines[0].trim(), "");
        assert_eq!(lines[1], "#% text");
        assert_eq!(lines[2].trim(), "");
        assert_eq!(lines[4], "x = 1");
        assert_eq!(lines[6].trim(), "");
        assert_eq!(lines[8], "y = x");
        assert_eq!(code_view("#! mathcas 2\nx", false), "#! mathcas 2\nx");
    }
}
