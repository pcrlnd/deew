//! Tauri-independent backend of the deew GUI: CLI discovery, the probing of
//! the inputs (done by the CLI), the GUI settings and the pseudo-terminal session.

pub mod bootstrap;
pub mod cli;
pub mod info;
pub mod probe;
pub mod pty_session;
pub mod settings;
