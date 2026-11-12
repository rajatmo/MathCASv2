//! The last tier of the decision service (`mc_decide::Reals`) on the decomposition: a conjunction of polynomial
//! conditions in several real variables is empty iff the sentence `exists vars: conds` is false (item F3's
//! hook, phase 11B).
use crate::cad::{decompose, Limits};
use crate::front::Front;
use crate::natives::metered;
use mc_api::{Store, T};
use mc_decide::{Reals, RealsFactory};
use mc_eval::{NativeCx, Res};

/// The decomposition as the decision service's hook.
pub struct CadReals;

impl<'h> Reals<'h> for CadReals {
    fn empty(&mut self, cx: &mut NativeCx<'_, '_, 'h>, conds: &[T<'h>], vars: &[T<'h>]) -> Res<Option<bool>> {
        if vars.is_empty() || conds.is_empty() {
            return Ok(None);
        }
        let h = cx.heap();
        let c = if conds.len() == 1 { conds[0] } else { h.call(h.sym("and"), conds) };
        let vl = cx.list(vars);
        let sentence = h.call(h.sym("exists"), &[vl, c]);
        let stop = cx.stop_flag().clone();
        let r = metered(cx, &stop, |m| {
            let fr = Front::new(h);
            let Ok(p) = fr.problem(sentence, &[], m)? else { return Ok(None) };
            if p.prob.nfree > 0 {
                return Ok(None);
            }
            let lim = Limits { max_factors: 60, max_cells: 5000, ..Limits::default() };
            let Ok(o) = decompose(&p.prob, &lim, &mut |_, _| {}, m)? else { return Ok(None) };
            Ok(o.truth.map(|t| !t))
        })?;
        Ok(r.flatten())
    }
}

/// The factory registered with the decision service.
pub struct Factory;

impl RealsFactory for Factory {
    fn make<'h>(&self) -> Box<dyn Reals<'h>> {
        Box::new(CadReals)
    }
}
