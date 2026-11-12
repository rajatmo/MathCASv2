//! Transactions and history (design 11, section 3.2): every command is one transaction, a list of primitive changes
//! (splices of a row's items) with the selection before and after; applying the splices backwards with their items
//! swapped undoes it exactly.
use crate::pos::Selection;
use mc_boxes::{Item, Row, RowPath};

/// One primitive change: in the row at `path`, the items `removed` starting at `at` were replaced by `inserted`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Splice {
    /// the row
    pub path: RowPath,
    /// where in the row
    pub at: usize,
    /// the items that were there
    pub removed: Vec<Item>,
    /// the items put there
    pub inserted: Vec<Item>,
}

impl Splice {
    /// Applies the change to `root` (it must be the tree the change was made on).
    pub fn apply(&self, root: &mut Row) {
        if let Some(row) = root.at_mut(&self.path) {
            let end = (self.at + self.removed.len()).min(row.len());
            let at = self.at.min(end);
            row.0.splice(at..end, self.inserted.iter().cloned());
        }
    }

    /// The change that undoes this one.
    pub fn inverse(&self) -> Splice {
        Splice { path: self.path.clone(), at: self.at, removed: self.inserted.clone(), inserted: self.removed.clone() }
    }
}

/// What kind of edit a transaction is, for merging typing into one undo step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// typed text; whether it ended with white space
    Typing {
        /// the last typed character was white space
        space: bool,
    },
    /// characters deleted one by one
    Deleting,
    /// anything else (a structure key, a paste, an alias, ...)
    Other,
}

/// One command's change of a tree.
#[derive(Clone, Debug, PartialEq)]
pub struct Transaction {
    /// the primitive changes in the order they were made
    pub splices: Vec<Splice>,
    /// the selection before
    pub before: Selection,
    /// the selection after
    pub after: Selection,
    /// what kind of edit
    pub kind: Kind,
}

impl Transaction {
    /// Applies the transaction to the tree it was made on (redo).
    pub fn apply(&self, root: &mut Row) {
        for s in &self.splices {
            s.apply(root);
        }
    }

    /// Undoes the transaction on the tree it left.
    pub fn revert(&self, root: &mut Row) {
        for s in self.splices.iter().rev() {
            s.inverse().apply(root);
        }
    }

    /// The transaction that undoes this one (its selections swapped).
    pub fn inverse(&self) -> Transaction {
        Transaction {
            splices: self.splices.iter().rev().map(Splice::inverse).collect(),
            before: self.after.clone(),
            after: self.before.clone(),
            kind: Kind::Other,
        }
    }

    /// Whether `next`, made right after this one, continues it as one undo step: typing after typing (a word
    /// boundary after white space ends the run), deleting after deleting.
    pub fn continued_by(&self, next: &Transaction) -> bool {
        match (self.kind, next.kind) {
            (Kind::Typing { space: a }, Kind::Typing { space: b }) => !(a && !b),
            (Kind::Deleting, Kind::Deleting) => true,
            _ => false,
        }
    }

    /// Appends `next` to this transaction (one undo step).
    pub fn merge(&mut self, next: Transaction) {
        self.splices.extend(next.splices);
        self.after = next.after;
        self.kind = next.kind;
    }
}

/// The undo and redo stacks of one tree.
#[derive(Clone, Debug, Default)]
pub struct History {
    undo: Vec<Transaction>,
    redo: Vec<Transaction>,
    /// the last transaction may take the next typing
    open: bool,
}

impl History {
    /// Records a transaction; typing continuing the last one merges into it.
    pub fn push(&mut self, tx: Transaction) {
        self.redo.clear();
        match self.undo.last_mut() {
            Some(last) if self.open && last.continued_by(&tx) => last.merge(tx),
            _ => self.undo.push(tx),
        }
        self.open = true;
    }

    /// Ends the current typing run (a caret jump, a pause, another command).
    pub fn close(&mut self) {
        self.open = false;
    }

    /// Undoes the last step on `root`; the selection to restore.
    pub fn undo(&mut self, root: &mut Row) -> Option<Selection> {
        let tx = self.undo.pop()?;
        tx.revert(root);
        let sel = tx.before.clone();
        self.redo.push(tx);
        self.open = false;
        Some(sel)
    }

    /// Redoes the last undone step on `root`; the selection to restore.
    pub fn redo(&mut self, root: &mut Row) -> Option<Selection> {
        let tx = self.redo.pop()?;
        tx.apply(root);
        let sel = tx.after.clone();
        self.undo.push(tx);
        self.open = false;
        Some(sel)
    }

    /// Steps that can be undone.
    pub fn undo_len(&self) -> usize {
        self.undo.len()
    }

    /// Steps that can be redone.
    pub fn redo_len(&self) -> usize {
        self.redo.len()
    }
}
