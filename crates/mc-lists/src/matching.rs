//! Matching a pattern term against subjects, for `cases`, `count`, `match_q` and the other pattern functions.
//!
//! The pattern term (`mc-syntax`'s pattern shapes) is checked and lowered by `mc_eval::syntax::pattern` and
//! compiled by the evaluator ([`NativeCx::compile_pattern`]) on first use, so these functions match exactly like
//! rules do: modulo associativity and commutativity (`x_ + y_` against any sum of two or more terms, semantics 6.2),
//! with runs (`repeated`), alternatives, conditions and the evaluator's enumeration order (decision C9B.10,
//! replacing the local matcher of decision I.10). The bindings of a match are read by variable number and
//! substituted into right sides by [`Matcher::substitute`]: a sequence variable stands for the list of its terms, or
//! for the associative-commutative head applied to them when it took arguments of such a call (semantics 6.2).
use crate::ctx::{Lib, Meter};
use mc_core::{Heap, Store, Sym, View, T};
use mc_eval::{Binds, NativeCx, Pattern, Res, Val};
use rustc_hash::FxHashMap;

/// A compiled pattern with its bindings.
pub(crate) struct Matcher<'l, 'h> {
    lib: &'l Lib,
    h: &'h Heap,
    /// the lowered pattern, until it is compiled at the first match (where a native context is at hand)
    lowered: Option<mc_eval::Pat>,
    pat: Option<Pattern>,
    /// the symbol of each pattern variable, by number
    names: Vec<Sym>,
    out: Binds<'h>,
    meter: Meter,
}

impl<'l, 'h> Matcher<'l, 'h> {
    /// The matcher of the pattern term `p`, or `None` when it is not a valid pattern (the evaluator's definition-time
    /// checks: variable kinds, alternatives, condition scope).
    pub fn new(lib: &'l Lib, h: &'h Heap, p: T<'h>) -> Option<Matcher<'l, 'h>> {
        let low = mc_eval::syntax::pattern(h, p).ok()?;
        mc_eval::pat::check(&low.pat).ok()?;
        Some(Matcher { lib, h, lowered: Some(low.pat), pat: None, names: low.names, out: Binds::default(), meter: Meter::default() })
    }

    /// Does the pattern match `t`? On success the bindings stay until the next call. A pattern the evaluator does
    /// not compile (for example a run under an associative-commutative head) matches nothing.
    pub fn matches(&mut self, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>) -> Res<bool> {
        self.meter.step(cx)?;
        if let Some(low) = self.lowered.take() {
            self.pat = cx.compile_pattern(low).ok();
        }
        let Some(pat) = &self.pat else { return Ok(false) };
        cx.match_binds(pat, t, &mut self.out)
    }

    /// `t` with every variable bound by the last match replaced by its value (a sequence by the list of its terms,
    /// or by the associative-commutative head applied to them, semantics 6.1 and 6.2). Iterative, each shared
    /// subterm once.
    pub fn substitute(&mut self, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>) -> Res<T<'h>> {
        let h = self.h;
        let list = h.sym_term(self.lib.s.list);
        let mut binds: Vec<(Sym, T<'h>)> = Vec::with_capacity(self.names.len());
        for (i, &s) in self.names.iter().enumerate() {
            let v = i as u16;
            match self.out.get(v) {
                Val::One(x) => binds.push((s, x)),
                Val::Seq(..) => {
                    let run = self.out.seq(v);
                    binds.push((s, h.app(self.out.seq_head(v).unwrap_or(list), run)));
                }
                Val::Unbound => {}
            }
        }
        let bm = binds.iter().fold(0, |m, &(s, _)| m | h.mask(h.sym_term(s)));
        if h.mask(t) & bm == 0 {
            return Ok(t);
        }
        let mut done: FxHashMap<T<'h>, T<'h>> = FxHashMap::default();
        let mut stack = vec![(t, false)];
        while let Some((u, expanded)) = stack.pop() {
            self.meter.step(cx)?;
            if done.contains_key(&u) {
                continue;
            }
            if h.mask(u) & bm == 0 {
                done.insert(u, u);
                continue;
            }
            let v = match h.view(u) {
                View::Sym(s) => binds.iter().find(|(v, _)| *v == s).map_or(u, |&(_, x)| x),
                View::App { head, args } if !expanded => {
                    stack.push((u, true));
                    stack.push((head, false));
                    stack.extend(args.iter().map(|&a| (a, false)));
                    continue;
                }
                View::App { head, args } => {
                    let args: Vec<T<'h>> = args.iter().map(|a| done[a]).collect();
                    h.app(done[&head], &args)
                }
                _ => u,
            };
            done.insert(u, v);
        }
        Ok(done[&t])
    }
}
