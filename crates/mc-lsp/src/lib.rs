//! The MathCAS v2 language server (item 3): the Language Server Protocol over stdio, written directly on
//! `serde_json` with the protocol's `Content-Length` framing.
//!
//! - [`rpc`]: message framing and error codes;
//! - [`server`]: the message loop and the dispatch of requests and notifications;
//! - [`analysis`]: the open documents and the answers (diagnostics, go to definition, hover, completion, document
//!   symbols), on `mc-syntax` (parse with recovery, spans) and `mc-load` (resolution with recorded definition
//!   sites, imports through the open buffers and the file system);
//! - [`outline`]: the definitions of a text from its syntax tree;
//! - [`library`]: the names a session knows before reading a file (keywords, library `FUNCTIONS`, prelude);
//! - [`text`]: UTF-16 positions, `file:` URIs, the code of notebooks.
//!
//! Decisions of this crate are numbered 3.1, 3.2, ... in the crate's notes.
#![warn(missing_docs)]
pub mod analysis;
pub mod library;
pub mod outline;
pub mod rpc;
pub mod server;
pub mod text;

pub use analysis::Workspace;
pub use server::Server;
