//! Key scripts (design 9.8): a notebook with the selection marked, keys, and the expected notebook, run through the
//! same key table the window uses, with [`FakeEnv`] and [`FakeKernel`].
//!
//! A script file holds tests; each starts with `=== title` and has sections `--- before`, `--- keys`, `--- after`
//! (expected `.mcn`), and optionally `--- groups` (expected group notation, cells named by their first word) and
//! `--- kernel old` (a kernel without the protocol additions) and `--- boxes` (the tree of the typeset cell with the
//! caret in `mc_boxes`' notation, `none` for no such cell). Notebook texts mark the selection: `▮` the caret,
//! `⟦` `⟧` a text selection (anchor at `⟦`), a line `‖` the insertion point before the next cell, ` ◆` at the end of
//! a marker line (or of a generated cell's first output line) a selected cell. The header line may be left out.
//!
//! Keys are separated by spaces: chords as in the key table (`Shift+Enter`, `Ctrl+Shift+D`, `Alt+4`), text in
//! quotes (`"1+1"`, with `\n` and `\"`), and actions: `Bracket(i)` / `Bracket(s,e)` (click a cell's or a group's
//! bracket; `Shift+` and `Ctrl+` prefixes allowed), `DoubleBracket(s,e)`, `Gap(i)`, `Click(i,pos)`, `Move(i)` /
//! `CopyTo(i)` (drop the selected cells before cell i), `Resize(i,w,h)` / `Resize(i)` (resize the graphic of cell i,
//! or restore its natural size), `InitYes`, `InitNo`, `InitAlways`, `Clip("text")` (put
//! text on the clipboard), `Pause` (two seconds pass), `Crash` (the kernel dies), `Quit`, `DeleteAllOutput`,
//! `EvalNotebook`, `AutoClose` (automatic closing of brackets and quotes is on from there).
use crate::cell::CellId;
use crate::command::{Bracket, Command, InitAnswer};
use crate::env::{Clip, FakeEnv};
use crate::fake::FakeKernel;
use crate::keys::{Chord, Context, Key, KeyTable};
use crate::notebook::{Notebook, Selection};
use crate::style::Style;
use mc_nbfile::{Notebook as File, HEADER};

const CARET: char = '▮';
const OPEN: char = '⟦';
const CLOSE: char = '⟧';
const GAP: &str = "‖";
const CELL_MARK: &str = " ◆";

/// A step of a key script.
#[derive(Clone, Debug, PartialEq)]
pub enum Step {
    /// a key chord
    Key(Chord),
    /// typed text
    Type(String),
    /// a command (mouse actions, dialog answers)
    Command(Command),
    /// put text on the clipboard
    Clip(String),
    /// two seconds pass (typing runs end)
    Pause,
    /// the kernel dies
    Crash,
    /// automatic closing of brackets and quotes is on from here (the window's default; off in scripts)
    AutoClose,
}

/// Where each file cell's lines are in a printed notebook: (marker line, first body line), 0-based.
fn cell_lines(f: &File) -> Vec<(usize, usize)> {
    f.first_lines().iter().map(|&l| (l as usize - 2, l as usize - 1)).collect()
}

/// The length in bytes of the comment prefix of a prose line (`# ` or `#`).
fn prefix_len(line: &str) -> usize {
    if line.starts_with("# ") {
        2
    } else if line.starts_with('#') {
        1
    } else {
        0
    }
}

/// The byte offset in `s` of character column `col`.
fn byte_at(s: &str, col: usize) -> usize {
    s.char_indices().nth(col).map_or(s.len(), |(i, _)| i)
}

/// A notebook from a marked text.
pub fn parse_marked(text: &str) -> Result<Notebook, String> {
    let text = if text.trim_start().starts_with("#!") { text.to_string() } else { format!("{HEADER}\n\n{text}") };
    let mut clean: Vec<String> = vec![];
    let mut caret: Option<(usize, usize)> = None;
    let mut open: Option<(usize, usize)> = None;
    let mut close: Option<(usize, usize)> = None;
    let mut gap_line: Option<usize> = None;
    let mut cell_marks: Vec<usize> = vec![];
    for line in text.lines() {
        if line.trim() == GAP {
            gap_line = Some(clean.len());
            continue;
        }
        let mut line = line.to_string();
        if let Some(l) = line.strip_suffix(CELL_MARK) {
            cell_marks.push(clean.len());
            line = l.to_string();
        }
        let mut out = String::new();
        for c in line.chars() {
            let at = (clean.len(), out.chars().count());
            match c {
                CARET => caret = Some(at),
                OPEN => open = Some(at),
                CLOSE => close = Some(at),
                c => out.push(c),
            }
        }
        clean.push(out);
    }
    let clean_text = clean.join("\n") + "\n";
    let f = File::parse(&clean_text).map_err(|e| e.to_string())?;
    let nb = Notebook::from_file(&f);
    let (_, map) = nb.to_file_mapped();
    let lines = cell_lines(&f);
    // the model index of each file cell
    let model_of = |k: usize| map.iter().position(|m| *m == Some(k)).expect("every file cell is a model cell");
    let cell_at = |line: usize| lines.iter().rposition(|&(m, _)| m < line);
    let generated = generated_lines(&nb, &f);
    let offset = |(line, col): (usize, usize)| -> Result<(CellId, usize), String> {
        let k = cell_at(line).ok_or("a mark before the first cell")?;
        let fc = &f.cells[k];
        let body = line - lines[k].1;
        if body >= fc.lines.len() && !(fc.lines.is_empty() && body == 0) {
            // a mark in a generated cell's output lines
            let (id, first, _) = generated
                .iter()
                .find(|&&(_, first, n)| first <= line && line < first + n)
                .ok_or(format!("a text mark outside a cell's content on line {}", line + 1))?;
            let mut at = 0;
            for l in &clean[*first..line] {
                at += l.len().saturating_sub(3) + 1;
            }
            let this = &clean[line];
            return Ok((*id, at + byte_at(this, col).saturating_sub(3.min(this.len()))));
        }
        let content: Vec<&str> = (0..fc.lines.len()).map(|j| clean[lines[k].1 + j].as_str()).collect();
        let pre = |l: &str| if fc.is_code() { 0 } else { prefix_len(l) };
        let mut at = 0;
        for l in &content[..body] {
            at += l.len() - pre(l) + 1;
        }
        let this = content.get(body).copied().unwrap_or("");
        let in_line = byte_at(this, col).saturating_sub(pre(this));
        Ok((nb.cells()[model_of(k)].id, at + in_line))
    };
    let sel = if let Some(c) = caret {
        let (cell, pos) = offset(c)?;
        Some(Selection::Caret { cell, anchor: pos, pos })
    } else if let (Some(a), Some(b)) = (open, close) {
        let (cell, anchor) = offset(a)?;
        let (cell2, pos) = offset(b)?;
        if cell != cell2 {
            return Err("a text selection across cells".into());
        }
        Some(Selection::Caret { cell, anchor, pos })
    } else if let Some(g) = gap_line {
        let k = lines.iter().position(|&(m, _)| m >= g);
        Some(Selection::Gap(k.map_or(nb.cells().len(), model_of)))
    } else if !cell_marks.is_empty() {
        let mut ids = vec![];
        for m in cell_marks {
            match lines.iter().position(|&(ml, _)| ml == m) {
                Some(k) => ids.push(nb.cells()[model_of(k)].id),
                None => {
                    // a generated cell's first output line
                    let found = generated.iter().find(|&&(_, l, _)| l == m);
                    ids.push(found.ok_or(format!("a cell mark on line {} that starts no cell", m + 1))?.0);
                }
            }
        }
        Some(Selection::Cells { anchor: ids[0], ids })
    } else {
        None
    };
    let mut nb = nb;
    if let Some(sel) = sel {
        nb.select(sel);
    }
    nb.mark_saved();
    Ok(nb)
}

/// The first output line (0-based) and the number of lines of every generated cell that is written in the file.
fn generated_lines(nb: &Notebook, f: &File) -> Vec<(CellId, usize, usize)> {
    let (_, map) = nb.to_file_mapped();
    let lines = cell_lines(f);
    let mut out = vec![];
    let mut line = 0;
    let mut block_open = false;
    for (i, c) in nb.cells().iter().enumerate() {
        let Some(k) = map[i] else { continue };
        if !c.generated() {
            line = lines[k].1 + f.cells[k].lines.len();
            block_open = false;
            continue;
        }
        if !block_open {
            line += 1;
            block_open = true;
        }
        let n = c.text.split('\n').count();
        out.push((c.id, line, n));
        line += n;
        if c.style == Style::Output && c.label.is_some() {
            block_open = false;
        }
    }
    out
}

/// The text of a notebook with its selection marked.
pub fn print_marked(nb: &Notebook) -> String {
    let (f, map) = nb.to_file_mapped();
    let mut lines: Vec<String> = f.print().lines().map(str::to_string).collect();
    let cl = cell_lines(&f);
    let pos_of = |id: CellId, pos: usize| -> Option<(usize, usize)> {
        let i = nb.index_of(id)?;
        let k = map[i]?;
        let c = &nb.cells()[i];
        let before = &c.text[..pos];
        if c.generated() {
            let (_, first, _) = generated_lines(nb, &f).into_iter().find(|g| g.0 == id)?;
            let row = before.matches('\n').count();
            let col = before[before.rfind('\n').map_or(0, |j| j + 1)..].chars().count();
            return Some((first + row, col + 3));
        }
        let row = before.matches('\n').count();
        let col = before[before.rfind('\n').map_or(0, |j| j + 1)..].chars().count();
        let line = cl[k].1 + row;
        let pre = if f.cells[k].is_code() { 0 } else { prefix_len(&lines[line]) };
        Some((line, col + pre))
    };
    let mut inserts: Vec<(usize, usize, char)> = vec![];
    let mut gap_before: Option<usize> = None;
    let mut cell_marks: Vec<usize> = vec![];
    let empty = |id: &CellId| nb.cell(*id).filter(|c| c.text.is_empty() && !c.generated());
    match nb.selection() {
        Selection::Caret { cell, .. } if empty(cell).is_some() => {
            // an empty cell has no line in the file: the caret gets one
            let k = map[nb.index_of(*cell).expect("selected")].expect("written");
            let line = if f.cells[k].is_code() { CARET.to_string() } else { format!("# {CARET}") };
            lines.insert(cl[k].1, line);
        }
        Selection::Caret { cell, anchor, pos } if anchor == pos => {
            inserts.extend(pos_of(*cell, *pos).map(|(l, c)| (l, c, CARET)))
        }
        Selection::Caret { cell, anchor, pos } => {
            let (a, b) = (*anchor.min(pos), *anchor.max(pos));
            inserts.extend(pos_of(*cell, a).map(|(l, c)| (l, c, OPEN)));
            inserts.extend(pos_of(*cell, b).map(|(l, c)| (l, c, CLOSE)));
        }
        Selection::Gap(g) => {
            gap_before =
                Some(match (*g..nb.cells().len()).find_map(|i| map[i].filter(|_| !nb.cells()[i].generated())) {
                    Some(k) => cl[k].0,
                    None => lines.len(),
                });
        }
        Selection::Cells { ids, .. } => {
            let gen = generated_lines(nb, &f);
            for id in ids {
                let i = nb.index_of(*id).expect("selected cells exist");
                if let Some(k) = map[i] {
                    if nb.cells()[i].generated() {
                        cell_marks.extend(gen.iter().find(|g| g.0 == *id).map(|g| g.1));
                    } else {
                        cell_marks.push(cl[k].0);
                    }
                }
            }
        }
    }
    inserts.sort_by_key(|&(l, c, _)| std::cmp::Reverse((l, c)));
    for (l, c, mark) in inserts {
        let at = byte_at(&lines[l], c);
        lines[l].insert(at, mark);
    }
    for m in cell_marks {
        lines[m].push_str(CELL_MARK);
    }
    if let Some(g) = gap_before {
        lines.insert(g, GAP.to_string());
    }
    let mut out = lines.join("\n");
    out.push('\n');
    out
}

/// The group notation of a notebook, cells named by the first word of their text (or their style).
pub fn group_notation(nb: &Notebook) -> String {
    let names: Vec<String> = nb
        .cells()
        .iter()
        .map(|c| c.text.split_whitespace().next().map_or_else(|| c.style.info().name.to_string(), str::to_string))
        .collect();
    nb.groups().notation(&names)
}

/// Read the keys of a script.
pub fn parse_steps(keys: &str) -> Result<Vec<Step>, String> {
    let mut steps = vec![];
    let mut chars = keys.chars().peekable();
    loop {
        while chars.peek().is_some_and(|c| c.is_whitespace()) {
            chars.next();
        }
        let Some(&c) = chars.peek() else { break };
        if c == '"' {
            chars.next();
            steps.push(Step::Type(quoted(&mut chars)?));
            continue;
        }
        let mut word = String::new();
        while let Some(&c) = chars.peek() {
            if c.is_whitespace() {
                break;
            }
            chars.next();
            word.push(c);
            if c == '(' {
                // an action's arguments, possibly a quoted string with spaces
                if chars.peek() == Some(&'"') {
                    chars.next();
                    word.push('"');
                    word.push_str(&quoted(&mut chars)?.replace('"', "\u{1}"));
                    word.push('"');
                }
            }
        }
        steps.push(step(&word)?);
    }
    Ok(steps)
}

fn quoted(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> Result<String, String> {
    let mut s = String::new();
    loop {
        match chars.next() {
            None => return Err("unterminated string".into()),
            Some('"') => return Ok(s),
            Some('\\') => match chars.next() {
                Some('n') => s.push('\n'),
                Some('t') => s.push('\t'),
                Some(c) => s.push(c),
                None => return Err("unterminated string".into()),
            },
            Some(c) => s.push(c),
        }
    }
}

fn step(word: &str) -> Result<Step, String> {
    let (mods, rest) = match word.split_once('(') {
        Some(_) => {
            let shift = word.starts_with("Shift+");
            let ctrl = word.starts_with("Ctrl+");
            (Some((shift, ctrl)), word.trim_start_matches("Shift+").trim_start_matches("Ctrl+"))
        }
        None => (None, word),
    };
    let Some((shift, ctrl)) = mods else {
        return Ok(match word {
            "InitYes" => Step::Command(Command::InitAnswer(InitAnswer::Yes)),
            "InitNo" => Step::Command(Command::InitAnswer(InitAnswer::No)),
            "InitAlways" => Step::Command(Command::InitAnswer(InitAnswer::Always)),
            "Pause" => Step::Pause,
            "AutoClose" => Step::AutoClose,
            "Crash" => Step::Crash,
            "Quit" => Step::Command(Command::QuitKernel),
            "DeleteAllOutput" => Step::Command(Command::DeleteAllOutput),
            "EvalNotebook" => Step::Command(Command::EvaluateNotebook),
            _ => Step::Key(Chord::parse(word)?),
        });
    };
    let (name, args) = rest.split_once('(').expect("checked");
    let args = args.strip_suffix(')').ok_or(format!("`{word}`: missing `)`"))?;
    if name == "Clip" {
        let s = args.strip_prefix('"').and_then(|a| a.strip_suffix('"')).ok_or("Clip(\"text\")")?;
        return Ok(Step::Clip(s.replace('\u{1}', "\"")));
    }
    let nums: Vec<usize> = args
        .split(',')
        .map(|a| a.trim().parse::<usize>().map_err(|e| format!("`{word}`: {e}")))
        .collect::<Result<_, _>>()?;
    let cmd = match (name, nums.as_slice()) {
        ("Bracket", [i]) => Command::ClickBracket(Bracket::Group(*i, *i + 1), shift, ctrl),
        ("Bracket", [s, e]) => Command::ClickBracket(Bracket::Group(*s, *e), shift, ctrl),
        ("DoubleBracket", [s, e]) => Command::DoubleClickBracket(Bracket::Group(*s, *e)),
        ("Gap", [i]) => Command::ClickGap(*i),
        ("Move", [i]) => Command::MoveCells(*i, false),
        ("CopyTo", [i]) => Command::MoveCells(*i, true),
        ("Click", [i, p]) => return Ok(Step::Command(Command::ClickText(CellId(u64::MAX - *i as u64), *p, shift))),
        // cells named by index, as `Click`
        ("Resize", [i]) => Command::ResizeGraphic(CellId(u64::MAX - *i as u64), None),
        ("Resize", [i, w, h]) => {
            Command::ResizeGraphic(CellId(u64::MAX - *i as u64), Some((*w as u32, *h as u32)))
        }
        _ => return Err(format!("unknown action `{word}`")),
    };
    Ok(Step::Command(cmd))
}

/// The context of the selection for key lookup.
pub fn context(nb: &Notebook) -> Context {
    match nb.selection() {
        Selection::Caret { .. } => Context::Text,
        Selection::Cells { .. } => Context::Cells,
        Selection::Gap(_) => Context::Gap,
    }
}

/// The command a chord means here: the key table's binding, or typing for an unbound character key.
pub fn command_for(keys: &KeyTable, nb: &Notebook, chord: Chord) -> Option<Command> {
    if let Some(c) = keys.lookup(chord, context(nb)) {
        return Some(c.clone());
    }
    match chord.key {
        Key::Char(c) if !chord.mods.ctrl && !chord.mods.alt => {
            Some(Command::Insert(if chord.mods.shift { c.to_uppercase().collect() } else { c.to_string() }))
        }
        Key::Space if !chord.mods.ctrl && !chord.mods.alt => Some(Command::Insert(" ".into())),
        _ => None,
    }
}

/// Run steps on a notebook with a fake environment and kernel.
pub fn run_steps(nb: &mut Notebook, steps: &[Step], env: &mut FakeEnv, kernel: &mut FakeKernel) {
    let keys = KeyTable::default();
    for s in steps {
        let cmd = match s {
            Step::Key(chord) => command_for(&keys, nb, *chord),
            Step::Type(t) => {
                for ch in t.chars() {
                    let c = if ch == '\n' { Command::Enter } else { Command::Insert(ch.to_string()) };
                    nb.apply(c, env);
                    nb.pump(kernel);
                }
                None
            }
            Step::Command(Command::ClickText(id, p, shift)) => {
                // `Click(i,pos)` names the cell by index
                let i = (u64::MAX - id.0) as usize;
                nb.cells().get(i).map(|c| Command::ClickText(c.id, *p, *shift))
            }
            Step::Command(Command::ResizeGraphic(id, size)) => {
                let i = (u64::MAX - id.0) as usize;
                nb.cells().get(i).map(|c| Command::ResizeGraphic(c.id, *size))
            }
            Step::Command(c) => Some(c.clone()),
            Step::Clip(t) => {
                env.clip = Clip { text: Some(t.clone()), cells: None };
                None
            }
            Step::Pause => {
                env.time += 2.0;
                None
            }
            Step::AutoClose => {
                nb.set_auto_close(true);
                None
            }
            Step::Crash => {
                kernel.crash();
                None
            }
        };
        if let Some(c) = cmd {
            nb.apply(c, env);
        }
        env.time += 0.01;
        nb.pump(kernel);
    }
}

/// One test of a script file.
#[derive(Clone, Debug, Default)]
pub struct Script {
    /// the title
    pub title: String,
    /// the notebook before
    pub before: String,
    /// the keys
    pub keys: String,
    /// the expected notebook
    pub after: Option<String>,
    /// the expected groups
    pub groups: Option<String>,
    /// use a kernel without the protocol additions
    pub old_kernel: bool,
    /// the expected tree of the typeset cell with the caret, in the box notation of `mc_boxes` (`none`: no
    /// typeset cell has the caret)
    pub boxes: Option<String>,
}

/// The tests of a script file.
pub fn parse_scripts(text: &str) -> Result<Vec<Script>, String> {
    let mut scripts: Vec<Script> = vec![];
    let mut section: Option<String> = None;
    let mut buf = String::new();
    let flush = |scripts: &mut Vec<Script>, section: &Option<String>, buf: &mut String| -> Result<(), String> {
        let content = std::mem::take(buf);
        let Some(sec) = section else { return Ok(()) };
        let s = scripts.last_mut().ok_or("a section before the first `===` title")?;
        match sec.as_str() {
            "before" => s.before = content,
            "keys" => s.keys = content,
            "after" => s.after = Some(content),
            "groups" => s.groups = Some(content.trim().to_string()),
            "kernel old" => s.old_kernel = true,
            "boxes" => s.boxes = Some(content.trim().to_string()),
            other => return Err(format!("unknown section `{other}`")),
        }
        Ok(())
    };
    for line in text.lines() {
        if let Some(t) = line.strip_prefix("=== ") {
            flush(&mut scripts, &section, &mut buf)?;
            section = None;
            scripts.push(Script { title: t.trim().to_string(), ..Script::default() });
        } else if let Some(s) = line.strip_prefix("--- ") {
            flush(&mut scripts, &section, &mut buf)?;
            section = Some(s.trim().to_string());
        } else if section.is_some() {
            buf.push_str(line);
            buf.push('\n');
        }
    }
    flush(&mut scripts, &section, &mut buf)?;
    Ok(scripts)
}

/// Run one test; the error describes the difference.
pub fn run_script(s: &Script) -> Result<(), String> {
    let mut nb = parse_marked(&s.before).map_err(|e| format!("before: {e}"))?;
    let steps = parse_steps(&s.keys)?;
    let mut env = FakeEnv::default();
    let mut kernel = FakeKernel::new(!s.old_kernel);
    run_steps(&mut nb, &steps, &mut env, &mut kernel);
    if let Some(after) = &s.after {
        let expected = print_marked(&parse_marked(after).map_err(|e| format!("after: {e}"))?);
        let got = print_marked(&nb);
        if got != expected {
            return Err(format!("expected:\n{expected}\ngot:\n{got}"));
        }
    }
    if let Some(b) = &s.boxes {
        let got = nb.box_editor().map_or("none".to_string(), |(_, e)| e.notation());
        if &got != b {
            return Err(format!("boxes: expected `{b}`, got `{got}`"));
        }
    }
    if let Some(g) = &s.groups {
        let got = group_notation(&nb);
        if &got != g {
            return Err(format!("groups: expected `{g}`, got `{got}`"));
        }
    }
    Ok(())
}
