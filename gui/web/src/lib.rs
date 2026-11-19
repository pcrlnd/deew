//! deew-web: serves the deew GUI frontend over HTTP and runs the deew CLI
//! for it in a pseudo-terminal, streamed over a WebSocket.
//!
//! The frontend (`gui/dist`) is embedded at build time: run `npm run build`
//! in `gui/` before building this crate (the build script fails with a clear
//! message otherwise).

pub mod app;
pub mod assets;
pub mod auth;
pub mod config;
pub mod fsapi;
pub mod info;
pub mod job;
pub mod security;

pub use app::{AppState, router};
