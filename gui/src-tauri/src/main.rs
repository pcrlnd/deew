// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

// `deew -u` reads this string out of the binary to learn the installed
// version, so it must survive LTO and section GC (`#[used]` plus the
// `black_box` in `main`).
#[used]
static VERSION_MARK: &str = concat!("\0deew-gui-version=", env!("CARGO_PKG_VERSION"), "\0");

fn main() {
    std::hint::black_box(VERSION_MARK);
    deew_gui_lib::run()
}
