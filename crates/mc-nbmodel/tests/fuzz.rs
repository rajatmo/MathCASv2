//! The model never panics (round 11 robustness): damaged notebook texts are read, then random command sequences
//! (typing, motions with and without Shift, clicks anywhere, clipboard, undo and redo, structure commands, cell
//! operations, typeset input and the caret in typeset outputs) run on them; after each command the selection must
//! name a cell that exists and positions on character boundaries. `MC_FUZZ_CASES` sets the number of sequences
//! (default 400).
use mc_nbmodel::{Bracket, Command, FakeEnv, Motion, Notebook, Selection, Style};
use std::panic::{catch_unwind, AssertUnwindSafe};

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
}

fn cases() -> usize {
    std::env::var("MC_FUZZ_CASES").ok().and_then(|v| v.parse().ok()).unwrap_or(400)
}

const NOTEBOOKS: &[&str] = &[
    "#! mathcas 2 notebook\n\n#% title\n# T\n\n#% section\n# S\n\n#% cell label=1\nx/2 + sqrt(y)\n#% output n=1\n#= (1/2)*x + y^(1/2)\n\n#% cell typeset\n((a)/(b))+((sqrt(c)))\n\n#% text\n# words here\n\n#% cell\n[[1, 2], [3, 4]]\n#% output n=2\n#= [[1, 2], [3, 4]]\n",
    "#! mathcas 2 notebook\n\n#% section closed\n# Closed\n\n#% cell\n1 + 1\n#% output\n#= 2\n#| printed\n#! failure(x)\n",
    "#! mathcas 2 notebook\n\n#% cell typeset\n(([[a, □], [□, d]]))\n\n#% output\n#= sum(k^2, [k, 1, n]) + integrate(f(x), x) + limit(sin(x)/x, x -> 0)\n",
    "",
    "#! mathcas 2 notebook\n\n#% cell\nα→β «3» \"str\"\n#% output\n#= \"a\\\"b\" -> derivative(2)(f)(x)\n",
];

const TEXTS: &[&str] = &["x", "1", " ", "(", ")", "[", "]", "/", "^", "\"", "#", "α", "□", "\n", "ab", "sin", "->", "€"];

fn command(r: &mut Rng, nb: &Notebook) -> Command {
    let n = nb.cells().len();
    let cell = |r: &mut Rng| nb.cells().get(r.below(n.max(1))).map(|c| (c.id, c.text.len()));
    let motions =
        [Motion::Left, Motion::Right, Motion::WordLeft, Motion::WordRight, Motion::Up, Motion::Down, Motion::Home, Motion::End, Motion::DocStart, Motion::DocEnd];
    let structures = [
        mc_boxes::Structure::Fraction,
        mc_boxes::Structure::Superscript,
        mc_boxes::Structure::Radical,
    ];
    match r.below(34) {
        0..=5 => Command::Insert(TEXTS[r.below(TEXTS.len())].to_string()),
        6..=11 => Command::Move(motions[r.below(motions.len())], r.below(3) == 0),
        12 => Command::Enter,
        13 => Command::Backspace,
        14 => Command::Delete,
        15 => Command::Undo,
        16 => Command::Redo,
        17 => [Command::Copy, Command::Cut, Command::Paste, Command::CopyPlain, Command::CopyLatex][r.below(5)].clone(),
        18 => Command::Structure(structures[r.below(structures.len())]),
        19 => [Command::ExitStructure, Command::Backspace, Command::Tab, Command::Outdent, Command::Alias][r.below(5)].clone(),
        20 => [Command::Delete, Command::Backspace, Command::ExtendSelection, Command::CheckBalance, Command::SelectAll][r.below(5)]
            .clone(),
        21 | 22 => match cell(r) {
            Some((id, len)) => Command::ClickText(id, r.below(len + 2), r.below(4) == 0),
            None => Command::ClickGap(0),
        },
        23 => match cell(r) {
            Some((id, _)) => {
                let depth = r.below(3);
                let path = (0..depth).map(|_| mc_boxes::Step { item: r.below(6), slot: r.below(4) }).collect();
                Command::ClickBox(id, path, r.below(8), r.below(3) == 0)
            }
            None => Command::ClickGap(0),
        },
        24 => Command::ClickGap(r.below(n + 2)),
        25 => match cell(r) {
            Some((id, _)) => Command::ClickBracket(Bracket::Cell(id), r.below(2) == 0, r.below(3) == 0),
            None => Command::ClickBracket(Bracket::Group(r.below(4), r.below(6)), false, false),
        },
        26 => [Command::Divide, Command::Merge, Command::ToggleGroup, Command::DeleteAllOutput, Command::ShowExpression][r.below(5)]
            .clone(),
        27 => [Command::InputFromAbove, Command::OutputFromAbove, Command::SameStyleBelow, Command::ToggleComment][r.below(4)].clone(),
        28 => Command::SetStyle(Style::ALL[r.below(Style::ALL.len())]),
        29 => Command::ReplaceWord(["sin", "", "x_1"][r.below(3)].to_string()),
        30 => match cell(r) {
            Some((id, len)) => [Command::SelectWord(id, r.below(len + 1)), Command::SelectLine(id, r.below(len + 1))][r.below(2)].clone(),
            None => Command::Nop,
        },
        31 => match cell(r) {
            Some((id, len)) if r.below(2) == 0 => Command::SelectText(id, r.below(len + 2), r.below(len + 2)),
            _ => Command::MoveCells(r.below(n + 2), r.below(2) == 0),
        },
        32 => Command::DeleteWordBack,
        _ => Command::NewCell([Style::Input, Style::Text][r.below(2)]),
    }
}

/// The selection names cells that exist, at character boundaries.
fn valid(nb: &Notebook) -> Result<(), String> {
    match nb.selection() {
        Selection::Caret { cell, anchor, pos } => {
            let c = nb.cell(*cell).ok_or("caret in a cell that is gone")?;
            for p in [anchor, pos] {
                if *p > c.text.len() || !c.text.is_char_boundary(*p) {
                    return Err(format!("position {p} in {:?}", c.text));
                }
            }
        }
        Selection::Cells { ids, .. } => {
            if ids.iter().any(|id| nb.cell(*id).is_none()) {
                return Err("selected cell is gone".into());
            }
        }
        Selection::Gap(g) => {
            if *g > nb.cells().len() {
                return Err(format!("insertion point {g} past {} cells", nb.cells().len()));
            }
        }
    }
    Ok(())
}

#[test]
fn random_commands_never_panic() {
    let mut r = Rng(0x1234_5678_9abc_def1);
    let n = cases();
    for k in 0..n {
        if n >= 10 && k.is_multiple_of(n / 10) {
            eprintln!("[{k}/{n} {}%] command sequences", k * 100 / n);
        }
        let text = NOTEBOOKS[r.below(NOTEBOOKS.len())];
        let mut nb = Notebook::parse(text).unwrap_or_default();
        nb.set_auto_close(r.below(2) == 0);
        // outputs shown typeset take the box caret
        let atomic = nb.cells().iter().filter(|c| c.style == Style::Output).map(|c| c.id).collect();
        let mut env = FakeEnv { atomic, ..FakeEnv::default() };
        let mut log = vec![];
        for _ in 0..80 {
            let cmd = command(&mut r, &nb);
            log.push(format!("{cmd:?}"));
            env.time += 0.3;
            let res = catch_unwind(AssertUnwindSafe(|| {
                nb.apply(cmd, &mut env);
            }));
            if let Err(p) = res {
                let msg = p.downcast_ref::<String>().cloned().or_else(|| p.downcast_ref::<&str>().map(|s| s.to_string()));
                panic!("case {k}: panic {msg:?} after\n{}", log.join("\n"));
            }
            if let Err(e) = valid(&nb) {
                panic!("case {k}: {e} after\n{}", log.join("\n"));
            }
        }
        // the result still writes and reads
        let t = nb.to_text();
        Notebook::parse(&t).unwrap_or_else(|e| panic!("case {k}: written notebook does not read: {e}\n{t}"));
    }
}

#[test]
fn damaged_texts_read_as_notebooks_without_panics() {
    let mut r = Rng(0x0f0f_1234_aaaa_5555);
    for k in 0..cases() * 5 {
        let base = NOTEBOOKS[r.below(NOTEBOOKS.len())];
        let mut b = base.as_bytes().to_vec();
        for _ in 0..1 + r.below(8) {
            if b.is_empty() {
                break;
            }
            let i = r.below(b.len());
            match r.below(3) {
                0 => {
                    b.remove(i);
                }
                1 => b[i] ^= 1 << r.below(8),
                _ => b.insert(i, b"#%= \n(\"["[r.below(8)]),
            }
        }
        let text = String::from_utf8_lossy(&b).into_owned();
        let res = catch_unwind(|| {
            if let Ok(nb) = Notebook::parse(&text) {
                let _ = nb.to_text();
            }
        });
        assert!(res.is_ok(), "case {k}: {text:?}");
    }
}
