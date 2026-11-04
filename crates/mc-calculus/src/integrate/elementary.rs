//! Integration of elementary functions by a table, substitution and parts (package 2C, item 3). OWNED BY THE
//! ELEMENTARY-INTEGRATION WORK PACKAGE.
use crate::term::Build;
use mc_api::T;
use mc_eval::{NativeCx, Res};

/// An antiderivative of `f` by the elementary methods, else `None`. May call
/// [`super::antiderivative`] with `depth + 1`.
pub fn integrate<'h>(_cx: &mut NativeCx<'_, '_, 'h>, _b: &Build<'h>, _f: T<'h>, _x: T<'h>, _depth: u32) -> Res<Option<T<'h>>> {
    Ok(None)
}
