//! Evaluation (design 5): the notebook's queue, the requests it sends one at a time, and how responses become
//! labels and generated cells. The kernel is behind [`KernelLink`]: the window's link speaks the JSON protocol to a
//! `mc-kernel` process; tests use [`crate::fake::FakeKernel`].
use crate::cell::{Cell, CellId, EvalMark};
use crate::notebook::{Notebook, Selection};
use crate::style::Style;
use std::collections::VecDeque;

/// A request to the kernel.
#[derive(Clone, Debug, PartialEq)]
pub enum KernelRequest {
    /// evaluate `src`; the request asks for a value per statement and numbered statements (design 5.2)
    Eval {
        /// the request id
        id: u64,
        /// the cell's stable name
        cell: String,
        /// the text
        src: String,
        /// the cell's first line in the notebook file
        line: u32,
    },
    /// evaluate `src` for an interactive control (`manipulate`, round 13): a value per statement, not numbered (no
    /// labels, no `out` entries)
    Side {
        /// the request id
        id: u64,
        /// the text
        src: String,
    },
    /// stop the evaluation with this id
    Interrupt {
        /// the `eval` request's id
        id: u64,
    },
    /// end the kernel process
    Shutdown,
}

/// A response from the kernel, already decoded. `n` is the statement's number when the kernel numbers statements.
#[derive(Clone, Debug, PartialEq)]
pub enum KernelMsg {
    /// the kernel is ready
    Hello {
        /// the engine's name and version
        engine: String,
    },
    /// `print` output
    Output {
        /// the request
        id: u64,
        /// the text (one line, or one call's text in per-statement mode)
        text: String,
        /// the statement's number
        n: Option<u32>,
    },
    /// a statement's value
    Value {
        /// the request
        id: u64,
        /// the printed value
        printed: String,
        /// the statement's number
        n: Option<u32>,
    },
    /// a statement's value to show as a picture (a `display` response): `printed` is its image value
    /// `image("path")`, which the Output cell holds and the window draws as the picture
    Display {
        /// the request
        id: u64,
        /// the printed value
        printed: String,
        /// the statement's number
        n: Option<u32>,
    },
    /// a statement whose value is a failure
    Failure {
        /// the request
        id: u64,
        /// the printed failure
        printed: String,
        /// the statement's number
        n: Option<u32>,
    },
    /// a read error (nothing ran) or a limit
    Error {
        /// the request
        id: u64,
        /// the error as text, with the source line and a caret
        rendered: String,
        /// the statement's number
        n: Option<u32>,
    },
    /// a warning from library code
    Message {
        /// the request
        id: u64,
        /// the text
        text: String,
        /// the statement's number
        n: Option<u32>,
    },
    /// the request ended
    Done {
        /// the request
        id: u64,
        /// whether it was interrupted
        interrupted: bool,
        /// the first statement number the request used
        first: Option<u32>,
        /// the counter after it
        next: Option<u32>,
        /// wall-clock milliseconds
        millis: u64,
    },
    /// the kernel process ended (or could not be started: `why`)
    Exited {
        /// what happened
        why: String,
    },
}

/// Something that talks to a kernel.
pub trait KernelLink {
    /// Send a request (the link starts its kernel when needed).
    fn send(&mut self, request: KernelRequest);
    /// The responses that arrived since the last call.
    fn poll(&mut self) -> Vec<KernelMsg>;
}

/// The kernel as the notebook sees it (toolbar indicator, status bar).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum KernelState {
    /// not started yet (or quit)
    #[default]
    NotStarted,
    /// started, nothing running
    Idle,
    /// evaluating
    Busy,
    /// it stopped unexpectedly
    Stopped,
}

#[derive(Clone, Debug)]
pub(crate) struct Running {
    pub cell: CellId,
    pub req: u64,
    /// the last generated cell made by this evaluation (the next one goes after it)
    pub last_out: Option<CellId>,
    /// the running cell was deleted: results are discarded
    pub discard: bool,
    /// the client-side number of this evaluation, for kernels that do not number statements
    pub number: u32,
    /// the display sizes of the graphics the cell's last evaluation showed, in order: the k-th new graphic keeps the
    /// k-th size (design 6, guess)
    pub sizes: VecDeque<Option<String>>,
    /// the line of the notebook file the cell's text was sent as starting on
    pub line: u32,
}

/// The evaluation of a `manipulate` output's controls that runs (round 13).
#[derive(Clone, Debug)]
pub(crate) struct ManipRun {
    /// the `manipulate` output
    pub cell: CellId,
    pub req: u64,
    /// the positions evaluated
    pub values: Vec<f64>,
    /// the last value (or failure) it showed
    pub result: Option<(String, bool)>,
    /// it was interrupted for a newer position
    pub interrupted: bool,
}

/// The evaluation state of a notebook.
#[derive(Clone, Debug)]
pub(crate) struct EvalState {
    pub queue: VecDeque<CellId>,
    pub running: Option<Running>,
    pub outbox: Vec<KernelRequest>,
    /// the next number when the kernel does not number statements (old kernels)
    pub counter: u32,
    pub next_req: u64,
    pub state: KernelState,
    pub engine: String,
    pub last_millis: Option<u64>,
    /// the initialization question was answered for this kernel (or need not be asked)
    pub init_settled: bool,
    /// cells waiting for the answer to the initialization question
    pub pending: Option<Vec<CellId>>,
    /// the running evaluation of controls, and the newest position waiting for it
    pub manip: Option<ManipRun>,
    pub manip_next: Option<(CellId, Vec<f64>)>,
}

impl Default for EvalState {
    fn default() -> Self {
        EvalState {
            queue: VecDeque::new(),
            running: None,
            outbox: vec![],
            counter: 1,
            next_req: 1,
            state: KernelState::NotStarted,
            engine: String::new(),
            last_millis: None,
            init_settled: false,
            pending: None,
            manip: None,
            manip_next: None,
        }
    }
}

impl EvalState {
    /// Removed cells leave the queue; a removed running cell's results are discarded.
    pub fn forget(&mut self, removed: &[Cell]) {
        self.queue.retain(|id| !removed.iter().any(|c| c.id == *id));
        if let Some(r) = &mut self.running {
            if removed.iter().any(|c| c.id == r.cell) {
                r.discard = true;
            }
            if r.last_out.is_some_and(|o| removed.iter().any(|c| c.id == o)) {
                r.last_out = None;
            }
        }
    }
}

impl Notebook {
    /// A message of the running cell with its positions in that cell (round 19): an untitled notebook's kernel names
    /// the cell `<cell ID>` (an id the reader never sees) and counts lines from the cell's line in the notebook's
    /// text (`<cell c5>:18:2` for the fourth line of a cell); shown under the cell, `line 4, column 2` reads.
    fn cell_positions(&self, text: &str) -> String {
        let Some(run) = &self.eval.running else { return text.to_string() };
        relative_positions(text, &format!("<cell {}>:", run.cell), run.line)
    }

    /// The kernel's state.
    pub fn kernel_state(&self) -> KernelState {
        self.eval.state
    }

    /// The engine named by the kernel's `hello`.
    pub fn engine(&self) -> &str {
        &self.eval.engine
    }

    /// Cells waiting in the queue (the running one not counted).
    pub fn queue_len(&self) -> usize {
        self.eval.queue.len()
    }

    /// The milliseconds of the last finished evaluation.
    pub fn last_millis(&self) -> Option<u64> {
        self.eval.last_millis
    }

    /// The cell being evaluated.
    pub fn running_cell(&self) -> Option<CellId> {
        self.eval.running.as_ref().map(|r| r.cell)
    }

    /// Send what is waiting, read the responses, start the next queued cell; `true` when the document changed.
    pub fn pump(&mut self, link: &mut dyn KernelLink) -> bool {
        let mut changed = false;
        loop {
            for r in std::mem::take(&mut self.eval.outbox) {
                link.send(r);
            }
            let msgs = link.poll();
            let had = !msgs.is_empty();
            for m in msgs {
                self.on_message(m);
                changed = true;
            }
            if self.eval.running.is_none() && self.start_next() {
                changed = true;
                continue;
            }
            if !had && self.eval.outbox.is_empty() {
                return changed;
            }
        }
    }

    /// Queue cells (evaluatable ones not already queued or running), asking first about initialization cells in a
    /// fresh kernel.
    pub(crate) fn enqueue(&mut self, ids: Vec<CellId>, fx: &mut crate::Effects) {
        let ids: Vec<CellId> = ids.into_iter().filter(|id| self.cell(*id).is_some_and(|c| c.evaluatable())).collect();
        if ids.is_empty() {
            return;
        }
        let fresh = matches!(self.eval.state, KernelState::NotStarted | KernelState::Stopped);
        if fresh && !self.eval.init_settled && self.eval.running.is_none() && self.eval.queue.is_empty() {
            let init = self.initialization_cells();
            if init.iter().any(|i| !ids.contains(i)) {
                if self.header.get("init") == Some("always") {
                    self.push_queue(init);
                } else {
                    self.eval.pending = Some(ids);
                    fx.ask_init = true;
                    return;
                }
            }
            self.eval.init_settled = true;
        }
        self.push_queue(ids);
    }

    pub(crate) fn initialization_cells(&self) -> Vec<CellId> {
        self.cells.iter().filter(|c| c.initialization() && c.evaluatable()).map(|c| c.id).collect()
    }

    /// The answer to the initialization question.
    pub(crate) fn answer_init(&mut self, answer: crate::InitAnswer) {
        let Some(pending) = self.eval.pending.take() else { return };
        self.eval.init_settled = true;
        if answer != crate::InitAnswer::No {
            if answer == crate::InitAnswer::Always {
                self.header.set("init", Some("always".into()));
                self.eval.init_settled = true;
            }
            let init = self.initialization_cells();
            self.push_queue(init);
        }
        self.push_queue(pending);
    }

    pub(crate) fn push_queue(&mut self, ids: Vec<CellId>) {
        for id in ids {
            let running = self.eval.running.as_ref().is_some_and(|r| r.cell == id);
            if running || self.eval.queue.contains(&id) {
                continue;
            }
            if let Some(i) = self.index_of(id) {
                self.cells[i].eval = EvalMark::Queued;
                self.retouch(i);
                self.eval.queue.push_back(id);
            }
        }
    }

    /// Abort: interrupt the running evaluation and clear the queue (design 5.6).
    pub(crate) fn abort(&mut self) {
        if let Some(r) = &self.eval.running {
            self.eval.outbox.push(KernelRequest::Interrupt { id: r.req });
        }
        self.clear_queue();
    }

    pub(crate) fn clear_queue(&mut self) {
        for id in std::mem::take(&mut self.eval.queue) {
            if let Some(i) = self.index_of(id) {
                self.cells[i].eval = EvalMark::None;
                self.retouch(i);
            }
        }
        self.eval.pending = None;
    }

    /// Remove the selected cells from the queue.
    pub(crate) fn unqueue(&mut self, ids: &[CellId]) {
        self.eval.queue.retain(|q| !ids.contains(q));
        for &id in ids {
            if let Some(i) = self.index_of(id) {
                if self.cells[i].eval == EvalMark::Queued {
                    self.cells[i].eval = EvalMark::None;
                    self.retouch(i);
                }
            }
        }
    }

    /// Quit the kernel: labels stay, the next evaluation starts a new kernel at 1.
    pub(crate) fn quit_kernel(&mut self) {
        self.clear_queue();
        if let Some(r) = self.eval.running.take() {
            if let Some(i) = self.index_of(r.cell) {
                self.cells[i].eval = EvalMark::None;
                self.retouch(i);
            }
        }
        if self.eval.state != KernelState::NotStarted {
            self.eval.outbox.push(KernelRequest::Shutdown);
        }
        self.reset_session(KernelState::NotStarted);
    }

    fn reset_session(&mut self, state: KernelState) {
        self.eval.state = state;
        self.eval.counter = 1;
        self.eval.init_settled = false;
    }

    /// Start the first queued cell; `false` when the queue is empty.
    fn start_next(&mut self) -> bool {
        while let Some(id) = self.eval.queue.pop_front() {
            let Some(i) = self.index_of(id) else { continue };
            if !self.cells[i].evaluatable() {
                self.cells[i].eval = EvalMark::None;
                continue;
            }
            // the old generated cells go (design 2.8); they are not undo units. Their graphics' sizes are kept for
            // the new graphics
            let end = self.outputs_end(i);
            let sizes = self.cells[i + 1..end]
                .iter()
                .filter(|c| c.graphic().is_some())
                .map(|c| c.out_extra.get("size").map(str::to_string))
                .collect();
            self.remove_cells(i + 1..end, false);
            let i = self.index_of(id).expect("the input stays");
            self.cells[i].eval = EvalMark::Running;
            self.retouch(i);
            let req = self.eval.next_req;
            self.eval.next_req += 1;
            let line = self.file_line(i);
            // typeset input with unmatched brackets carries escapes: the kernel reads the text without them
            let src = mc_boxes::source_text(&self.cells[i].text).into_owned();
            self.eval.outbox.push(KernelRequest::Eval { id: req, cell: id.to_string(), src, line });
            self.eval.running =
                Some(Running { cell: id, req, last_out: None, discard: false, number: self.eval.counter, sizes, line });
            self.eval.state = KernelState::Busy;
            return true;
        }
        false
    }

    /// Insert a generated cell after the running cell's results so far.
    fn add_generated(&mut self, style: Style, text: &str, label: Option<u32>, failure: bool, merge_print: bool) {
        let Some(r) = self.eval.running.clone() else { return };
        if r.discard {
            return;
        }
        let Some(input) = self.index_of(r.cell) else { return };
        let after = r.last_out.and_then(|o| self.index_of(o)).unwrap_or(input);
        if merge_print && self.cells[after].style == Style::Print && r.last_out.is_some() {
            let id = self.cells[after].id;
            let joined = format!("{}\n{text}", self.cells[after].text);
            self.set_text(id, joined, false);
            return;
        }
        let mut c = self.new_cell(style, text);
        c.label = label;
        c.failure = failure;
        c.input_of = Some(r.cell);
        let id = c.id;
        self.insert_cells(after + 1, vec![c], false);
        if let Some(run) = &mut self.eval.running {
            run.last_out = Some(id);
        }
    }

    fn number(&self, n: Option<u32>) -> Option<u32> {
        n.or_else(|| self.eval.running.as_ref().map(|r| r.number))
    }

    /// A response to the running evaluation of controls; `false` when `m` is not one.
    fn on_manipulate(&mut self, m: &KernelMsg) -> bool {
        let Some(run) = &mut self.eval.manip else { return false };
        let req = run.req;
        match m {
            KernelMsg::Value { id, printed, .. } | KernelMsg::Display { id, printed, .. } if *id == req => {
                run.result = Some((printed.clone(), false));
            }
            KernelMsg::Failure { id, printed, .. } if *id == req => run.result = Some((printed.clone(), true)),
            KernelMsg::Error { id, rendered, .. } if *id == req => run.result = Some((rendered.clone(), true)),
            KernelMsg::Output { id, .. } | KernelMsg::Message { id, .. } if *id == req => {}
            KernelMsg::Done { id, interrupted, .. } if *id == req => {
                let interrupted = *interrupted;
                self.finish_manipulate(interrupted);
                if self.eval.running.is_none() && self.eval.manip.is_none() {
                    self.eval.state = KernelState::Idle;
                }
            }
            _ => return false,
        }
        true
    }

    fn on_message(&mut self, m: KernelMsg) {
        if self.on_manipulate(&m) {
            return;
        }
        let current = self.eval.running.as_ref().map(|r| r.req);
        let mine = |id: u64| current == Some(id);
        match m {
            KernelMsg::Hello { engine } => {
                self.eval.engine = engine;
                if self.eval.state == KernelState::NotStarted || self.eval.state == KernelState::Stopped {
                    self.eval.state = if self.eval.running.is_some() { KernelState::Busy } else { KernelState::Idle };
                }
            }
            KernelMsg::Output { id, text, n } if mine(id) => {
                self.add_generated(Style::Print, &text, None, false, n.is_none());
            }
            KernelMsg::Value { id, printed, n } if mine(id) => {
                let label = self.number(n);
                self.add_generated(Style::Output, &printed, label, false, false);
                // a `manipulate` value: its result at the initial positions is asked for at once (round 13)
                let made = self.eval.running.as_ref().filter(|r| !r.discard).and_then(|r| r.last_out);
                if let Some(out) = made {
                    if let Some((_, values)) = self.manipulate_of(out) {
                        self.request_manipulate(out, values, false);
                    }
                }
            }
            KernelMsg::Display { id, printed, n } if mine(id) => {
                let label = self.number(n);
                self.add_generated(Style::Output, &printed, label, false, false);
                let run = self.eval.running.as_mut().filter(|r| !r.discard);
                let size = run.and_then(|r| r.sizes.pop_front().flatten().zip(r.last_out));
                if let Some((size, out)) = size {
                    if let Some(i) = self.index_of(out) {
                        self.cells[i].out_extra.set("size", Some(size));
                        self.retouch(i);
                    }
                }
            }
            KernelMsg::Failure { id, printed, n } if mine(id) => {
                let label = self.number(n);
                self.add_generated(Style::Output, &printed, label, true, false);
            }
            KernelMsg::Message { id, text, .. } if mine(id) => {
                let text = self.cell_positions(&text);
                self.add_generated(Style::Message, &text, None, false, false);
            }
            KernelMsg::Error { id, rendered, .. } if mine(id) => {
                if let Some(i) = self.eval.running.as_ref().and_then(|r| self.index_of(r.cell)) {
                    self.cells[i].eval = EvalMark::Error;
                    self.retouch(i);
                }
                let rendered = self.cell_positions(&rendered);
                self.add_generated(Style::Message, &rendered, None, true, false);
            }
            KernelMsg::Done { id, interrupted, first, next, millis } if mine(id) => {
                let r = self.eval.running.clone().expect("mine");
                let number = if interrupted && next.is_some() {
                    next.map(|n| n.saturating_sub(1).max(first.unwrap_or(1)))
                } else {
                    Some(r.number)
                };
                if interrupted {
                    self.add_generated(Style::Output, "aborted", number, true, false);
                }
                if let Some(i) = self.index_of(r.cell) {
                    if !r.discard {
                        self.cells[i].label = Some(first.unwrap_or(r.number));
                    }
                    if self.cells[i].eval != EvalMark::Error {
                        self.cells[i].eval = EvalMark::None;
                    }
                    self.retouch(i);
                    self.modified_by_eval();
                }
                self.eval.counter = next.unwrap_or(r.number + 1);
                self.eval.last_millis = Some(millis);
                self.eval.running = None;
                self.eval.state = KernelState::Idle;
            }
            KernelMsg::Exited { why } => {
                if let Some(run) = self.eval.manip.take() {
                    if let Some(i) = self.index_of(run.cell) {
                        self.cells[i].eval = EvalMark::None;
                        self.retouch(i);
                    }
                }
                self.eval.manip_next = None;
                if self.eval.running.is_some() {
                    self.add_generated(Style::Message, &format!("the kernel stopped: {why}"), None, true, false);
                    if let Some(i) = self.eval.running.as_ref().and_then(|r| self.index_of(r.cell)) {
                        self.cells[i].eval = EvalMark::None;
                        self.retouch(i);
                    }
                    self.eval.running = None;
                }
                self.clear_queue();
                let state = if self.eval.state == KernelState::NotStarted {
                    KernelState::NotStarted
                } else {
                    KernelState::Stopped
                };
                self.reset_session(state);
            }
            // responses to requests that are no longer current (an aborted or discarded run)
            _ => {}
        }
    }

    fn modified_by_eval(&mut self) {
        // labels and outputs are part of the saved document
        self.mark_modified();
    }

    /// The selection after Shift+Enter on cell `i`: the insertion point below its output group.
    pub(crate) fn after_outputs(&self, i: usize) -> Selection {
        Selection::Gap(self.outputs_end(i))
    }
}

/// `text` with every `<prefix>L:C` (a position in the notebook's text, the cell starting on line `first`) written as
/// `line l, column C`, `l` counted in the cell.
fn relative_positions(text: &str, prefix: &str, first: u32) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find(prefix) {
        out.push_str(&rest[..at]);
        let after = &rest[at + prefix.len()..];
        let digits = |s: &str| s.len() - s.trim_start_matches(|c: char| c.is_ascii_digit()).len();
        let nl = digits(after);
        let line: Option<u32> = after[..nl].parse().ok();
        let col_part = after[nl..].strip_prefix(':');
        match (line, col_part) {
            (Some(l), Some(c)) if digits(c) > 0 => {
                let nc = digits(c);
                let rel = l.saturating_sub(first) + 1;
                out.push_str(&format!("line {rel}, column {}", &c[..nc]));
                rest = &c[nc..];
            }
            _ => {
                out.push_str(prefix);
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod position_tests {
    use super::relative_positions;

    #[test]
    fn positions_in_the_cell() {
        let t = "<cell c5>:18:2: expected the end\n  }table(\n<cell c5>:19:1: more";
        assert_eq!(relative_positions(t, "<cell c5>:", 15), "line 4, column 2: expected the end\n  }table(\nline 5, column 1: more");
        assert_eq!(relative_positions("<cell c3>:7:1: x", "<cell c5>:", 15), "<cell c3>:7:1: x", "another cell stays");
        assert_eq!(relative_positions("<cell c5>: odd", "<cell c5>:", 1), "<cell c5>: odd");
    }
}
