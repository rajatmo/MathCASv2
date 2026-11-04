//! Partial fractions: `apart` (package 2C, item 1). OWNED BY THE APART WORK PACKAGE.
use crate::term::Build;
use mc_api::T;
use mc_eval::{NativeCx, Res};

/// The partial fraction decomposition of `e` in `x` (`x` = `None`: the only variable of `e`), else `None`
/// (the call stays as it is).
pub fn apart<'h>(_cx: &mut NativeCx<'_, '_, 'h>, _b: &Build<'h>, _e: T<'h>, _x: Option<T<'h>>) -> Res<Option<T<'h>>> {
    Ok(None)
}
