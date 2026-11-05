//! The MathCAS v2 notebook window (design `08-notebook.md`, sections 8 - 10): the egui application around the
//! document model of `mc-nbmodel`, a client for the `mc-kernel` process, the Atom One themes and the self-test.
pub mod app;
mod clip;
mod fonts;
mod highlight;
mod input;
pub mod instance;
pub mod kernel;
pub mod selftest;
pub mod settings;
mod tabbar;
pub mod theme;
mod view;
