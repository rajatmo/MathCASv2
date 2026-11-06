//! Damaged notebook files never panic (round 11 robustness): random texts built from the format's own pieces,
//! mutations of the example notebook (lines dropped, repeated, swapped, cut at any character, bytes flipped) and
//! random bytes go through `Notebook::read`, `print`, `parse_cells` and the picture helpers. Whatever is read prints
//! and reads again. `MC_FUZZ_CASES` sets the number of cases of each kind (default 3,000).
use mc_nbfile::{images, parse_cells, Attrs, Notebook};
use std::panic::{catch_unwind, AssertUnwindSafe};

/// A small deterministic generator (xorshift).
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            0
        } else {
            (self.next() % n as u64) as usize
        }
    }

    fn pick<'a>(&mut self, xs: &[&'a str]) -> &'a str {
        xs[self.below(xs.len())]
    }
}

fn cases() -> usize {
    std::env::var("MC_FUZZ_CASES").ok().and_then(|v| v.parse().ok()).unwrap_or(3000)
}

fn progress(what: &str, k: usize, n: usize) {
    if n >= 10 && k.is_multiple_of(n / 10) {
        eprintln!("[{k}/{n} {}%] {what}", k * 100 / n);
    }
}

/// Everything a reader of untrusted text does, none of which may panic.
fn exercise(text: &str) -> Result<(), String> {
    let r = catch_unwind(AssertUnwindSafe(|| {
        if let Ok((nb, _warnings)) = Notebook::read(text) {
            let printed = nb.print();
            // what was read prints, and the printed text reads again
            let again = Notebook::read(&printed).map_err(|e| format!("printed text does not read: {e}"))?;
            let _ = again.0.print();
            let _ = nb.first_lines();
            for c in &nb.cells {
                let _ = c.content();
                let _ = c.source();
                let _ = images::size_of(&c.attrs);
                for l in c.output_lines() {
                    let _ = images::image_path(&l.text);
                }
            }
        }
        let _ = Notebook::parse(text);
        let _ = parse_cells(text);
        let _ = Attrs::parse(text.lines().next().unwrap_or(""));
        let _ = images::svg_size(text);
        let _ = images::image_path(text);
        let _ = images::ImageFormat::sniff(text.as_bytes());
        Ok::<(), String>(())
    }));
    match r {
        Ok(r) => r,
        Err(p) => Err(p.downcast_ref::<String>().cloned().or_else(|| p.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default()),
    }
}

const PIECES: &[&str] = &[
    "#! mathcas 2 notebook\n", "#! mathcas 1\n", "#% ", "cell", "code", "text", "title", "section", "output", "print",
    "message", "input", "item", " label=", " size=", "3x", "x4", " n=", " form=typeset", " typeset", " closed",
    " initialization", " =", "=", "\"", "\\", "#= ", "#| ", "#! ", "#? ", "# ", "#", "\n", "\r\n", "\r", "\n\n", " ",
    "1", "-1", "99999999999999999999", "x^2", "image(\"p.png\")", "image(\"", "((x)/(2))", "□", "α", "€", "\u{feff}",
    "\u{0}", "#% cells\n", "<svg width=\"9e99\" height=\"-3\">", "<svg width=\"12px\"", "«", "»", "\t",
];

#[test]
fn texts_from_the_formats_pieces_never_panic() {
    let mut r = Rng(0x9e37_79b9_7f4a_7c15);
    let n = cases();
    for k in 0..n {
        progress("pieces", k, n);
        let len = r.below(60);
        let text: String = (0..len).map(|_| r.pick(PIECES)).collect();
        if let Err(e) = exercise(&text) {
            panic!("case {k} panicked: {e}\n{text:?}");
        }
    }
}

#[test]
fn damaged_copies_of_the_example_never_panic() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/notebook.mcn");
    let base = std::fs::read_to_string(path).expect("examples/notebook.mcn");
    let lines: Vec<&str> = base.split_inclusive('\n').collect();
    let mut r = Rng(0x2545_f491_4f6c_dd1d);
    let n = cases();
    for k in 0..n {
        progress("mutations", k, n);
        let mut ls: Vec<String> = lines.iter().map(|s| s.to_string()).collect();
        for _ in 0..1 + r.below(6) {
            if ls.is_empty() {
                break;
            }
            let i = r.below(ls.len());
            match r.below(6) {
                0 => {
                    ls.remove(i);
                }
                1 => {
                    let l = ls[i].clone();
                    ls.insert(r.below(ls.len()), l);
                }
                2 => {
                    let j = r.below(ls.len());
                    ls.swap(i, j);
                }
                3 => {
                    // cut a line at a character boundary
                    let l = &ls[i];
                    let cut = l.char_indices().map(|(b, _)| b).nth(r.below(l.chars().count().max(1))).unwrap_or(0);
                    ls[i] = l[..cut].to_string();
                }
                4 => ls.insert(i, r.pick(PIECES).to_string()),
                _ => {
                    // a flipped byte, read as text the way a damaged file is (lossy)
                    let mut b = ls[i].clone().into_bytes();
                    if !b.is_empty() {
                        let j = r.below(b.len());
                        b[j] ^= 1 << r.below(8);
                    }
                    ls[i] = String::from_utf8_lossy(&b).into_owned();
                }
            }
        }
        let text: String = ls.concat();
        if let Err(e) = exercise(&text) {
            panic!("case {k} panicked: {e}\n{text:?}");
        }
    }
}

#[test]
fn random_bytes_never_panic() {
    let mut r = Rng(0xdead_beef_cafe_f00d);
    let n = cases();
    for k in 0..n {
        progress("bytes", k, n);
        let len = r.below(400);
        let bytes: Vec<u8> = (0..len).map(|_| r.next() as u8).collect();
        let text = String::from_utf8_lossy(&bytes);
        if let Err(e) = exercise(&text) {
            panic!("case {k} panicked: {e}\n{text:?}");
        }
    }
}
