//! Read access to definitions for natives that analyse them (added for package 1b, the numeric compiler, which
//! turns a function's rules into machine code and must notice when they change).
//!
//! These are additions to [`NativeCx`] only: they read what the evaluator already keeps and change no behaviour.
//! Every read is recorded in the task's read set, like [`NativeCx::value`], so a memoised result that depended on
//! what a native read is invalidated when it changes.
use crate::eval::bit;
use crate::index::RuleRhs;
use crate::native::NativeCx;
use crate::pat::Pat;
use mc_core::{Sym, T};

/// One of a symbol's own rules, as a native sees it.
#[derive(Clone)]
pub struct RuleView<'h> {
    /// The compiled left side.
    pub lhs: Pat,
    /// The right side when it is a term: symbol `names[i]` stands for pattern variable `vars[i]` in it. `None` for a
    /// native right side.
    pub body: Option<T<'h>>,
    /// The symbols that stand for pattern variables in `body`.
    pub names: Vec<Sym>,
    /// The pattern variables `names` stand for.
    pub vars: Vec<u16>,
}

impl<'h> NativeCx<'_, '_, 'h> {
    /// Mark this evaluation as depending on state that is not a definition (for example which compiled
    /// specialisations are registered, or their run counters): its result is never memoised, as for cells.
    pub fn volatile(&mut self) {
        self.cx.volatile += 1;
    }

    /// The version counter of `s` (semantics 1.2: bumped by every change to its definitions); the read is recorded.
    pub fn version(&mut self, s: Sym) -> u32 {
        self.cx.reads |= bit(s);
        self.ev.k.defs.version(s)
    }

    /// The rules `s` holds for itself, in rule order; the read is recorded. Argument-head rules other symbols attach
    /// to `s` (semantics 6.5) are not included.
    pub fn rules(&mut self, s: Sym) -> Vec<RuleView<'h>> {
        self.cx.reads |= bit(s);
        let h = self.ev.heap;
        let Some(rs) = self.ev.k.defs.get(s).and_then(|d| d.rules()) else { return vec![] };
        rs.order()
            .into_iter()
            .map(|id| {
                let r = rs.rule(id);
                let (body, names, vars) = match &r.rhs {
                    RuleRhs::Term { body, names, vars } => (Some(body.get(h)), names.to_vec(), vars.to_vec()),
                    RuleRhs::Native(_) => (None, vec![], vec![]),
                };
                RuleView { lhs: r.lhs.clone(), body, names, vars }
            })
            .collect()
    }

    /// Whether `s` has a native implementation.
    pub fn has_native(&self, s: Sym) -> bool {
        self.ev.k.has_native(s)
    }
}
