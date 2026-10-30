//! Tauri-independent backend of the deew GUI: CLI discovery, ffprobe
//! probing, the GUI settings and the pseudo-terminal session.

pub mod cli;
pub mod probe;
pub mod pty_session;
pub mod settings;
