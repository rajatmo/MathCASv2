//! Definite integrals (package 2C, item 4). OWNED BY THE DEFINITE-INTEGRATION WORK PACKAGE.
use crate::term::Build;
use mc_api::T;
use mc_eval::{NativeCx, Res};

/// The integral of `f` in `x` from `lo` to `hi` (either may be `inf` or `-inf`), else `None`. Uses
/// [`super::antiderivative`].
pub fn integrate<'h>(_cx: &mut NativeCx<'_, '_, 'h>, _b: &Build<'h>, _f: T<'h>, _x: T<'h>, _lo: T<'h>, _hi: T<'h>) -> Res<Option<T<'h>>> {
    Ok(None)
}
