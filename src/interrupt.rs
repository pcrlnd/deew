//! Ctrl-C (SIGINT, SIGTERM and SIGHUP on Unix, Ctrl-Break and console close
//! on Windows): the flag the workers check, the registry of the running child
//! processes and the helpers that stop them.

use crate::events;
use crate::rprintln;
use std::process::Child;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

/// Exit status after an interruption (128 + SIGINT).
pub const EXIT_CODE: i32 = 130;

const MESSAGE: &str =
    "[bold red]interrupted[/bold red], stopping the running jobs and removing temp files...";

/// A signal arrived.
static INTERRUPTED: AtomicBool = AtomicBool::new(false);
/// The jobs are running (before that a signal just ends deew).
static JOBS_ACTIVE: AtomicBool = AtomicBool::new(false);
/// PIDs of the running child processes.
static CHILDREN: Mutex<Vec<u32>> = Mutex::new(Vec::new());
/// Prints a line of markup above the progress bars.
type Printer = Box<dyn Fn(&str) + Send>;
static PRINTER: Mutex<Option<Printer>> = Mutex::new(None);

/// Install the signal handler.
pub fn install() {
    // Fails only if a handler exists already.
    let _ = ctrlc::set_handler(on_signal);
}

fn on_signal() {
    if INTERRUPTED.swap(true, Ordering::SeqCst) {
        // second signal: no more cleanup
        kill_all();
        std::process::exit(EXIT_CODE);
    }
    if !JOBS_ACTIVE.load(Ordering::SeqCst) {
        // prompt or preparation: nothing runs, nothing to clean
        println!();
        std::process::exit(EXIT_CODE);
    }
    events::error("interrupted");
    match PRINTER.lock().expect("printer").as_ref() {
        Some(print) => print(MESSAGE),
        None => rprintln!("{MESSAGE}"),
    }
    kill_all();
}

/// A signal arrived; no new job may start.
pub fn is_interrupted() -> bool {
    INTERRUPTED.load(Ordering::SeqCst)
}

/// The jobs start: from now on a signal stops them cleanly. `print` shows a
/// line of markup above the progress bars.
pub fn jobs_started(print: impl Fn(&str) + Send + 'static) {
    *PRINTER.lock().expect("printer") = Some(Box::new(print));
    JOBS_ACTIVE.store(true, Ordering::SeqCst);
}

/// All jobs returned.
pub fn jobs_ended() {
    JOBS_ACTIVE.store(false, Ordering::SeqCst);
    *PRINTER.lock().expect("printer") = None;
}

/// Remember a running child; it is killed at once if a signal came already.
pub fn register(child: &Child) {
    let mut children = CHILDREN.lock().expect("children");
    children.push(child.id());
    if is_interrupted() {
        kill_tree(child.id());
    }
}

/// The child was waited for.
pub fn unregister(pid: u32) {
    CHILDREN
        .lock()
        .expect("children")
        .retain(|&running| running != pid);
}

/// Kill every running child (with its own children).
fn kill_all() {
    for &pid in CHILDREN.lock().expect("children").iter() {
        kill_tree(pid);
    }
}

/// Children run in their own process group (see [`isolate`]), so one signal
/// reaches a tool and everything it started.
#[cfg(unix)]
pub fn kill_tree(pid: u32) {
    let pid = pid as i32;
    // SAFETY: kill(2) with a PID/process group of a child of this process.
    unsafe {
        libc::kill(-pid, libc::SIGKILL);
        libc::kill(pid, libc::SIGKILL);
    }
}

#[cfg(windows)]
pub fn kill_tree(pid: u32) {
    use std::process::{Command, Stdio};
    let _ = Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

/// Start the child in its own process group: the terminal's Ctrl-C then
/// reaches only deew, which stops the children when it is time.
pub fn isolate(command: &mut std::process::Command) {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    #[cfg(not(unix))]
    let _ = command;
}

/// The final line of an interrupted run, e.g.
/// `interrupted: 2 jobs finished, 1 stopped, 3 not started`.
pub fn final_line(finished: usize, failed: usize, stopped: usize, not_started: usize) -> String {
    let jobs = if finished == 1 { "job" } else { "jobs" };
    let mut line = format!("{finished} {jobs} finished");
    if failed > 0 {
        line.push_str(&format!(", {failed} failed"));
    }
    line.push_str(&format!(", {stopped} stopped, {not_started} not started"));
    format!("interrupted: {line}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn final_line_texts() {
        assert_eq!(
            final_line(2, 0, 1, 3),
            "interrupted: 2 jobs finished, 1 stopped, 3 not started"
        );
        assert_eq!(
            final_line(1, 1, 1, 0),
            "interrupted: 1 job finished, 1 failed, 1 stopped, 0 not started"
        );
        assert_eq!(
            final_line(0, 0, 2, 0),
            "interrupted: 0 jobs finished, 2 stopped, 0 not started"
        );
    }

    #[test]
    fn registry_adds_and_removes() {
        let mut command = std::process::Command::new(if cfg!(windows) { "cmd" } else { "true" });
        isolate(&mut command);
        let mut child = command.spawn().unwrap();
        let pid = child.id();
        register(&child);
        assert!(CHILDREN.lock().unwrap().contains(&pid));
        child.wait().unwrap();
        unregister(pid);
        assert!(!CHILDREN.lock().unwrap().contains(&pid));
    }
}
