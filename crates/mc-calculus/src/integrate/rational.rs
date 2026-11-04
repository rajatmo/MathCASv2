//! Integration of rational functions of `x` (package 2C, item 2). OWNED BY THE RATIONAL-INTEGRATION WORK PACKAGE.
use crate::term::Build;
use mc_api::T;
use mc_eval::{NativeCx, Res};

/// An antiderivative of `f` when it is a rational function of `x` (coefficients free of `x`), else `None`.
pub fn integrate<'h>(_cx: &mut NativeCx<'_, '_, 'h>, _b: &Build<'h>, _f: T<'h>, _x: T<'h>, _depth: u32) -> Res<Option<T<'h>>> {
    Ok(None)
}
