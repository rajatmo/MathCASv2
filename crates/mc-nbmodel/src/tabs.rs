//! The tabs of a notebook window: several notebooks in one window, in an order the user can change, one of them
//! active; closing with the unsaved-changes questions; reopening closed tabs. No user interface: the window keeps
//! each tab's document in `T` and reports the tab's path and modified state; this list decides what is active, what
//! closes and what must be asked first.
//!
//! The questions (design guesses of this crate): closing one modified tab asks about that tab; closing several
//! (Close others, Close to the right) asks about each modified one in turn, and Cancel stops the remaining closes;
//! closing the window asks **once** for all modified tabs (Save all / Don't save / Cancel). Closing the active tab
//! activates its right neighbour, else its left one.
use std::collections::VecDeque;
use std::path::{Path, PathBuf};

/// How many closed tabs can be reopened.
const CLOSED_KEPT: usize = 20;

/// One tab.
#[derive(Clone, Debug)]
pub struct Tab<T> {
    /// a stable identity (indices change when tabs move or close)
    pub id: u64,
    /// the notebook's file, if it has one
    pub path: Option<PathBuf>,
    /// the number of an untitled notebook (`Untitled-N`)
    pub untitled: u32,
    /// unsaved changes
    pub modified: bool,
    /// the window's document
    pub doc: T,
}

impl<T> Tab<T> {
    /// The title shown on the tab: the file name, or `Untitled-N`.
    pub fn title(&self) -> String {
        match &self.path {
            Some(p) => p.file_name().map_or_else(|| p.display().to_string(), |n| n.to_string_lossy().to_string()),
            None => format!("Untitled-{}", self.untitled),
        }
    }
}

/// A question the window must ask before tabs close.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ask {
    /// "Save the changes to this notebook?" for one tab (by id)
    Save(u64),
    /// "Save the changes to these notebooks?" once for all of them, before the window closes
    SaveAll(Vec<u64>),
}

/// The answer to a question.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Answer {
    /// the notebooks were saved (the window saved them): close
    Saved,
    /// close without saving
    Discard,
    /// do not close (this and the remaining closes of the same request)
    Cancel,
}

/// The tabs of a window.
#[derive(Clone, Debug)]
pub struct Tabs<T> {
    tabs: Vec<Tab<T>>,
    active: usize,
    next_id: u64,
    next_untitled: u32,
    closed: Vec<PathBuf>,
    /// tabs waiting to close, in order
    closing: VecDeque<u64>,
    /// the window closes once the tabs are closed
    closing_window: bool,
    asked: Option<Ask>,
}

impl<T> Default for Tabs<T> {
    fn default() -> Self {
        Tabs {
            tabs: vec![],
            active: 0,
            next_id: 1,
            next_untitled: 1,
            closed: vec![],
            closing: VecDeque::new(),
            closing_window: false,
            asked: None,
        }
    }
}

/// What a close request did.
#[derive(Debug)]
pub struct Closed<T> {
    /// the documents of the tabs that closed (the window drops them: their kernels stop)
    pub docs: Vec<T>,
    /// the question to ask now, if a modified tab is next
    pub ask: Option<Ask>,
    /// the window should close now
    pub window: bool,
}

impl<T> Tabs<T> {
    /// No tabs.
    pub fn new() -> Tabs<T> {
        Tabs::default()
    }

    /// The number of tabs.
    pub fn len(&self) -> usize {
        self.tabs.len()
    }

    /// No tabs at all.
    pub fn is_empty(&self) -> bool {
        self.tabs.is_empty()
    }

    /// The index of the active tab.
    pub fn active(&self) -> usize {
        self.active
    }

    /// The tabs in order.
    pub fn iter(&self) -> impl Iterator<Item = &Tab<T>> {
        self.tabs.iter()
    }

    /// The tabs in order, mutable.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut Tab<T>> {
        self.tabs.iter_mut()
    }

    /// Tab `i`.
    pub fn get(&self, i: usize) -> Option<&Tab<T>> {
        self.tabs.get(i)
    }

    /// Tab `i`, mutable.
    pub fn get_mut(&mut self, i: usize) -> Option<&mut Tab<T>> {
        self.tabs.get_mut(i)
    }

    /// The active tab.
    pub fn current(&self) -> Option<&Tab<T>> {
        self.tabs.get(self.active)
    }

    /// The active tab, mutable.
    pub fn current_mut(&mut self) -> Option<&mut Tab<T>> {
        self.tabs.get_mut(self.active)
    }

    /// The index of the tab with id `id`.
    pub fn index_of(&self, id: u64) -> Option<usize> {
        self.tabs.iter().position(|t| t.id == id)
    }

    /// The index of the tab showing the file `path` (compared after canonicalisation when the file exists).
    pub fn find_path(&self, path: &Path) -> Option<usize> {
        let canon = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
        let want = canon(path);
        self.tabs.iter().position(|t| t.path.as_deref().is_some_and(|p| canon(p) == want))
    }

    /// Add a tab after the last one and make it active; its index.
    pub fn add(&mut self, path: Option<PathBuf>, doc: T) -> usize {
        let untitled = if path.is_none() {
            self.next_untitled += 1;
            self.next_untitled - 1
        } else {
            0
        };
        self.tabs.push(Tab { id: self.next_id, path, untitled, modified: false, doc });
        self.next_id += 1;
        self.active = self.tabs.len() - 1;
        self.active
    }

    /// Tab `i` now shows a file: it gives its Untitled number back when that was the last one given.
    pub fn set_path(&mut self, i: usize, path: PathBuf) {
        let Some(t) = self.tabs.get_mut(i) else {
            return;
        };
        if t.path.is_none() && t.untitled + 1 == self.next_untitled {
            self.next_untitled -= 1;
        }
        t.path = Some(path);
        t.untitled = 0;
    }

    /// Make tab `i` active (ignored when out of range).
    pub fn select(&mut self, i: usize) {
        if i < self.tabs.len() {
            self.active = i;
        }
    }

    /// The next tab, wrapping around (`Ctrl+Tab`, `Ctrl+PageDown`).
    pub fn next(&mut self) {
        if !self.tabs.is_empty() {
            self.active = (self.active + 1) % self.tabs.len();
        }
    }

    /// The previous tab, wrapping around (`Ctrl+Shift+Tab`, `Ctrl+PageUp`).
    pub fn prev(&mut self) {
        if !self.tabs.is_empty() {
            self.active = (self.active + self.tabs.len() - 1) % self.tabs.len();
        }
    }

    /// The `n`-th tab (1-based; 9 is the last tab, as in browsers).
    pub fn nth(&mut self, n: usize) {
        if n == 9 {
            self.active = self.tabs.len().saturating_sub(1);
        } else if n >= 1 {
            self.select(n - 1);
        }
    }

    /// Move tab `from` to position `to`; the active tab stays active.
    pub fn move_tab(&mut self, from: usize, to: usize) {
        if from >= self.tabs.len() || to >= self.tabs.len() || from == to {
            return;
        }
        let id = self.tabs[self.active].id;
        let t = self.tabs.remove(from);
        self.tabs.insert(to, t);
        self.active = self.index_of(id).unwrap_or(0);
    }

    /// The question being asked, if any.
    pub fn asked(&self) -> Option<&Ask> {
        self.asked.as_ref()
    }

    /// Close the tabs with these ids, in order: unmodified ones at once, modified ones after a question each.
    pub fn close(&mut self, ids: &[u64]) -> Closed<T> {
        if self.asked.is_some() {
            return Closed { docs: vec![], ask: self.asked.clone(), window: false };
        }
        self.closing = ids.iter().copied().filter(|id| self.index_of(*id).is_some()).collect();
        self.closing_window = false;
        self.run_closing()
    }

    /// Close every tab and then the window: one question for all modified tabs.
    pub fn close_window(&mut self) -> Closed<T> {
        if self.asked.is_some() {
            return Closed { docs: vec![], ask: self.asked.clone(), window: false };
        }
        let modified: Vec<u64> = self.tabs.iter().filter(|t| t.modified).map(|t| t.id).collect();
        if !modified.is_empty() {
            self.closing_window = true;
            self.asked = Some(Ask::SaveAll(modified));
            return Closed { docs: vec![], ask: self.asked.clone(), window: false };
        }
        Closed { docs: vec![], ask: None, window: true }
    }

    /// The answer to the question asked.
    pub fn answer(&mut self, a: Answer) -> Closed<T> {
        let Some(ask) = self.asked.take() else {
            return Closed { docs: vec![], ask: None, window: false };
        };
        match (ask, a) {
            (_, Answer::Cancel) => {
                self.closing.clear();
                self.closing_window = false;
                Closed { docs: vec![], ask: None, window: false }
            }
            (Ask::SaveAll(_), _) => {
                self.closing_window = false;
                Closed { docs: vec![], ask: None, window: true }
            }
            (Ask::Save(id), _) => {
                let mut docs = vec![];
                if let Some(i) = self.index_of(id) {
                    docs.push(self.remove(i));
                }
                let mut rest = self.run_closing();
                docs.append(&mut rest.docs);
                rest.docs = docs;
                rest
            }
        }
    }

    fn run_closing(&mut self) -> Closed<T> {
        let mut docs = vec![];
        while let Some(id) = self.closing.pop_front() {
            let Some(i) = self.index_of(id) else { continue };
            if self.tabs[i].modified {
                self.asked = Some(Ask::Save(id));
                return Closed { docs, ask: self.asked.clone(), window: false };
            }
            docs.push(self.remove(i));
        }
        Closed { docs, ask: None, window: false }
    }

    /// Remove tab `i` at once (no question); a file tab can be reopened later.
    pub fn remove(&mut self, i: usize) -> T {
        let t = self.tabs.remove(i);
        if let Some(p) = &t.path {
            self.closed.retain(|q| q != p);
            self.closed.push(p.clone());
            if self.closed.len() > CLOSED_KEPT {
                self.closed.remove(0);
            }
        }
        if i < self.active || self.active >= self.tabs.len() && self.active > 0 {
            self.active -= 1;
        }
        t.doc
    }

    /// The most recently closed file that is not open, taken off the list (`Ctrl+Shift+T`).
    pub fn reopen(&mut self) -> Option<PathBuf> {
        while let Some(p) = self.closed.pop() {
            if self.find_path(&p).is_none() {
                return Some(p);
            }
        }
        None
    }

    /// The ids of the tabs other than `i`.
    pub fn others(&self, i: usize) -> Vec<u64> {
        self.tabs.iter().enumerate().filter(|(k, _)| *k != i).map(|(_, t)| t.id).collect()
    }

    /// The ids of the tabs to the right of `i`.
    pub fn right_of(&self, i: usize) -> Vec<u64> {
        self.tabs.iter().skip(i + 1).map(|t| t.id).collect()
    }
}

/// A tab script (the tab model's tests): one step per line, the state after each `expect`.
///
/// Steps: `new`, `open <file>`, `openhere <file>` (the active tab shows the file now), `edit` (the active tab gets unsaved changes), `save` (the active tab is saved),
/// `select <i>` (0-based), `next`, `prev`, `nth <n>`, `move <from> <to>`, `close` (the active tab), `close <i>`,
/// `close-others <i>`, `close-right <i>`, `close-window`, `answer saved|discard|cancel`, `reopen`; `expect <state>`
/// compares the tabs as `a.mcn [*b.mcn] Untitled-1` (active in brackets, `*` modified), with ` ? <title>` or
/// ` ?all <titles>` for the question asked and ` !window` once the window would close.
pub fn run_tab_script(script: &str) -> Result<(), String> {
    let mut tabs: Tabs<()> = Tabs::new();
    let mut window = false;
    for (n, line) in script.lines().enumerate().map(|(n, l)| (n + 1, l.trim())) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (cmd, arg) = line.split_once(' ').unwrap_or((line, ""));
        let num = |s: &str| s.trim().parse::<usize>().map_err(|_| format!("line {n}: bad number `{s}`"));
        let id_at = |tabs: &Tabs<()>, i: usize| tabs.get(i).map(|t| t.id).ok_or(format!("line {n}: no tab {i}"));
        let mut done = |c: Closed<()>| window |= c.window;
        match cmd {
            "new" => {
                tabs.add(None, ());
            }
            "open" => match tabs.find_path(Path::new(arg)) {
                Some(i) => tabs.select(i),
                None => {
                    tabs.add(Some(PathBuf::from(arg)), ());
                }
            },
            "openhere" => {
                let i = tabs.active();
                tabs.set_path(i, PathBuf::from(arg));
            }
            "edit" => tabs.current_mut().ok_or(format!("line {n}: no tab"))?.modified = true,
            "save" => tabs.current_mut().ok_or(format!("line {n}: no tab"))?.modified = false,
            "select" => tabs.select(num(arg)?),
            "next" => tabs.next(),
            "prev" => tabs.prev(),
            "nth" => tabs.nth(num(arg)?),
            "move" => {
                let (a, b) = arg.split_once(' ').ok_or(format!("line {n}: move <from> <to>"))?;
                tabs.move_tab(num(a)?, num(b)?);
            }
            "close" => {
                let i = if arg.is_empty() { tabs.active() } else { num(arg)? };
                let id = id_at(&tabs, i)?;
                done(tabs.close(&[id]));
            }
            "close-others" => {
                let ids = tabs.others(num(arg)?);
                done(tabs.close(&ids));
            }
            "close-right" => {
                let ids = tabs.right_of(num(arg)?);
                done(tabs.close(&ids));
            }
            "close-window" => done(tabs.close_window()),
            "answer" => {
                let a = match arg {
                    "saved" => Answer::Saved,
                    "discard" => Answer::Discard,
                    "cancel" => Answer::Cancel,
                    _ => return Err(format!("line {n}: answer saved|discard|cancel")),
                };
                if a == Answer::Saved {
                    // the window saved what was asked about
                    let ids: Vec<u64> = match tabs.asked() {
                        Some(Ask::Save(id)) => vec![*id],
                        Some(Ask::SaveAll(ids)) => ids.clone(),
                        None => vec![],
                    };
                    for t in tabs.iter_mut().filter(|t| ids.contains(&t.id)) {
                        t.modified = false;
                    }
                }
                done(tabs.answer(a));
            }
            "reopen" => {
                if let Some(p) = tabs.reopen() {
                    tabs.add(Some(p), ());
                }
            }
            "expect" => {
                let got = describe(&tabs, window);
                if got != arg {
                    return Err(format!("line {n}: expected `{arg}`, got `{got}`"));
                }
            }
            other => return Err(format!("line {n}: unknown step `{other}`")),
        }
    }
    Ok(())
}

/// The state of the tabs in the notation of [`run_tab_script`].
fn describe<T>(tabs: &Tabs<T>, window: bool) -> String {
    let title = |id: u64| tabs.index_of(id).and_then(|i| tabs.get(i)).map_or("?".to_string(), Tab::title);
    let mut parts: Vec<String> = tabs
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let s = format!("{}{}", if t.modified { "*" } else { "" }, t.title());
            if i == tabs.active() {
                format!("[{s}]")
            } else {
                s
            }
        })
        .collect();
    if parts.is_empty() {
        parts.push("(none)".into());
    }
    match tabs.asked() {
        Some(Ask::Save(id)) => parts.push(format!("? {}", title(*id))),
        Some(Ask::SaveAll(ids)) => {
            parts.push(format!("?all {}", ids.iter().map(|&id| title(id)).collect::<Vec<_>>().join(",")))
        }
        None => {}
    }
    if window {
        parts.push("!window".into());
    }
    parts.join(" ")
}
