//! Runs a program inside a pseudo-terminal and streams its output.
//!
//! This module deliberately does not depend on Tauri so that it can be unit
//! tested with plain `cargo test`.

use portable_pty::{ChildKiller, CommandBuilder, MasterPty, PtySize, native_pty_system};
use std::fs::File;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub type Result<T> = std::result::Result<T, String>;

/// How long to wait for the last output after the child exited before the
/// pseudo-terminal is closed forcibly (needed on Windows, where ConPTY keeps
/// the output pipe open until the console is closed).
const DRAIN_GRACE: Duration = Duration::from_millis(500);
/// Time between two reads of the events file while it has no new data.
const EVENTS_POLL: Duration = Duration::from_millis(50);
/// Time between the polite interrupt and the hard kill in [`PtySession::stop`]
/// (deew removes its temp files and partial outputs when interrupted).
const STOP_GRACE: Duration = Duration::from_secs(10);

pub struct SpawnConfig {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub cols: u16,
    pub rows: u16,
    pub cwd: Option<PathBuf>,
    pub env: Vec<(String, String)>,
    /// Give the program a file in `DEEW_EVENTS_FILE` to write JSON lines to
    /// and report them through `on_event`.
    pub events: bool,
}

type SharedMaster = Arc<Mutex<Option<Box<dyn MasterPty + Send>>>>;

pub struct PtySession {
    master: SharedMaster,
    writer: Box<dyn Write + Send>,
    killer: Box<dyn ChildKiller + Send + Sync>,
    pid: Option<u32>,
    exited: Arc<AtomicBool>,
}

fn size(cols: u16, rows: u16) -> PtySize {
    PtySize {
        rows: rows.max(1),
        cols: cols.max(1),
        pixel_width: 0,
        pixel_height: 0,
    }
}

/// Spawns `config.program` attached to a new pseudo-terminal.
///
/// `on_output` receives the raw bytes the program writes (on a reader
/// thread). `on_exit` is called exactly once, after all output has been
/// delivered, with the exit code.
///
/// With `config.events` the program also gets an empty temp file whose path
/// is in `DEEW_EVENTS_FILE`. Every complete line the program appends to it is
/// passed to `on_event` (without the line ending, empty lines skipped), on a
/// tail thread. The last lines are delivered before `on_exit` is called, and
/// the file is deleted afterwards.
pub fn spawn(
    config: SpawnConfig,
    mut on_output: impl FnMut(&[u8]) + Send + 'static,
    on_event: impl FnMut(&str) + Send + 'static,
    on_exit: impl FnOnce(i32) + Send + 'static,
) -> Result<PtySession> {
    let pair = native_pty_system()
        .openpty(size(config.cols, config.rows))
        .map_err(|e| format!("could not open a pseudo-terminal: {e}"))?;

    let mut cmd = CommandBuilder::new(&config.program);
    cmd.args(&config.args);
    cmd.env("TERM", "xterm-256color");
    // Never pass on a value inherited from our own environment.
    cmd.env_remove("DEEW_EVENTS_FILE");
    for (key, value) in &config.env {
        cmd.env(key, value);
    }
    // The read handle is opened before the child starts, so that no fallible
    // step is left once it runs. std opens files with all share modes on
    // Windows, so the CLI can keep appending to the file while we read it.
    let events = if config.events {
        let path = tempfile::Builder::new()
            .prefix("deew-events-")
            .suffix(".jsonl")
            .tempfile()
            .map_err(|e| format!("could not create the events file: {e}"))?
            .into_temp_path();
        let file = File::open(&path).map_err(|e| format!("could not open the events file: {e}"))?;
        cmd.env("DEEW_EVENTS_FILE", path.as_os_str());
        Some((file, path))
    } else {
        None
    };
    if let Some(cwd) = &config.cwd {
        cmd.cwd(cwd);
    }

    let mut child = pair
        .slave
        .spawn_command(cmd)
        .map_err(|e| format!("could not start {}: {e}", config.program.display()))?;
    // The child owns the slave side now; keeping ours open would stop the
    // reader from seeing EOF on Unix.
    drop(pair.slave);

    let mut reader = pair
        .master
        .try_clone_reader()
        .map_err(|e| format!("could not read from the pseudo-terminal: {e}"))?;
    let writer = pair
        .master
        .take_writer()
        .map_err(|e| format!("could not write to the pseudo-terminal: {e}"))?;
    let killer = child.clone_killer();
    let pid = child.process_id();
    let master: SharedMaster = Arc::new(Mutex::new(Some(pair.master)));
    let exited = Arc::new(AtomicBool::new(false));

    let (done_tx, done_rx) = mpsc::channel::<()>();
    std::thread::spawn(move || {
        let mut buf = [0u8; 8192];
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break, // EOF, or EIO once the child is gone
                Ok(n) => on_output(&buf[..n]),
            }
        }
        let _ = done_tx.send(());
    });

    // The tail ends (after a last read) when `events_done` is set.
    let events_done = Arc::new(AtomicBool::new(false));
    let tail = match events {
        Some((file, path)) => {
            let done = Arc::clone(&events_done);
            let handle = std::thread::spawn(move || tail_lines(file, &done, on_event));
            Some((handle, path))
        }
        None => None,
    };

    let waiter_master = Arc::clone(&master);
    let waiter_exited = Arc::clone(&exited);
    std::thread::spawn(move || {
        let code = match child.wait() {
            Ok(status) => status.exit_code() as i32,
            Err(_) => -1,
        };
        waiter_exited.store(true, Ordering::SeqCst);
        if done_rx.recv_timeout(DRAIN_GRACE).is_err() {
            // Close the pseudo-terminal, which ends the reader.
            if let Ok(mut guard) = waiter_master.lock() {
                guard.take();
            }
            let _ = done_rx.recv_timeout(Duration::from_secs(2));
        }
        if let Some((handle, path)) = tail {
            events_done.store(true, Ordering::SeqCst);
            let _ = handle.join();
            drop(path); // deletes the file
        }
        on_exit(code);
    });

    Ok(PtySession {
        master,
        writer,
        killer,
        pid,
        exited,
    })
}

/// Reads `file` as it grows and calls `on_line` for every line. Once `done`
/// is set it reads the rest, delivers a last line without line ending too,
/// and returns.
fn tail_lines(mut file: File, done: &AtomicBool, mut on_line: impl FnMut(&str)) {
    let mut pending: Vec<u8> = Vec::new();
    let mut buf = [0u8; 8192];
    loop {
        // Checked before reading, so the final read sees everything written
        // before the flag was set.
        let finishing = done.load(Ordering::SeqCst);
        let mut got_data = false;
        loop {
            match file.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    got_data = true;
                    pending.extend_from_slice(&buf[..n]);
                }
            }
        }
        let mut start = 0;
        while let Some(pos) = pending[start..].iter().position(|&b| b == b'\n') {
            emit_line(&pending[start..start + pos], &mut on_line);
            start += pos + 1;
        }
        pending.drain(..start);
        if finishing {
            emit_line(&pending, &mut on_line);
            return;
        }
        if !got_data {
            std::thread::sleep(EVENTS_POLL);
        }
    }
}

fn emit_line(line: &[u8], on_line: &mut impl FnMut(&str)) {
    let line = line.strip_suffix(b"\r").unwrap_or(line);
    if !line.is_empty() {
        on_line(&String::from_utf8_lossy(line));
    }
}

impl PtySession {
    pub fn is_finished(&self) -> bool {
        self.exited.load(Ordering::SeqCst)
    }

    /// Sends keystrokes / pasted text to the program.
    pub fn write(&mut self, data: &[u8]) -> Result<()> {
        self.writer
            .write_all(data)
            .and_then(|()| self.writer.flush())
            .map_err(|e| format!("could not write to the process: {e}"))
    }

    pub fn resize(&self, cols: u16, rows: u16) -> Result<()> {
        let guard = self.master.lock().map_err(|e| e.to_string())?;
        match guard.as_ref() {
            Some(master) => master
                .resize(size(cols, rows))
                .map_err(|e| format!("could not resize the pseudo-terminal: {e}")),
            None => Ok(()),
        }
    }

    /// Asks the program (and the processes it started) to stop, like Ctrl+C
    /// in a terminal, and kills them if they are still alive after a grace
    /// period.
    pub fn stop(&mut self) {
        self.stop_with_grace(STOP_GRACE);
    }

    fn stop_with_grace(&mut self, grace: Duration) {
        if self.is_finished() {
            return;
        }
        #[cfg(unix)]
        if let Some(pid) = self.pid {
            // The child is a session leader, so its pid is the id of its
            // process group (which includes DEE, ffmpeg, ...).
            signal_group(pid, libc::SIGINT);
            let exited = Arc::clone(&self.exited);
            std::thread::spawn(move || {
                std::thread::sleep(grace);
                if !exited.load(Ordering::SeqCst) {
                    signal_group(pid, libc::SIGKILL);
                    kill_session(pid);
                }
            });
            return;
        }
        #[cfg(windows)]
        if let Some(pid) = self.pid {
            // ConPTY turns the Ctrl+C byte into a CTRL_C_EVENT for the
            // processes of its console.
            if self.write(&[0x03]).is_ok() {
                let exited = Arc::clone(&self.exited);
                std::thread::spawn(move || {
                    std::thread::sleep(grace);
                    if !exited.load(Ordering::SeqCst) {
                        taskkill_tree(pid);
                    }
                });
                return;
            }
        }
        self.hard_kill();
    }

    fn hard_kill(&mut self) {
        #[cfg(windows)]
        if let Some(pid) = self.pid {
            taskkill_tree(pid);
        }
        let _ = self.killer.kill();
    }
}

/// Kills the process and its children (DEE, ffmpeg, ...).
#[cfg(windows)]
fn taskkill_tree(pid: u32) {
    let _ = std::process::Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .output();
}

#[cfg(unix)]
fn signal_group(pid: u32, signal: i32) {
    if let Ok(pgid) = i32::try_from(pid) {
        // SAFETY: plain syscall without memory access.
        unsafe {
            libc::killpg(pgid, signal);
        }
    }
}

/// Kills every process of the session the child leads. deew runs its tools in
/// their own process groups, which a kill of deew's group does not reach.
#[cfg(unix)]
fn kill_session(sid: u32) {
    let _ = std::process::Command::new("pkill")
        .args(["-KILL", "-s", &sid.to_string()])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
}

impl Drop for PtySession {
    fn drop(&mut self) {
        if self.is_finished() {
            return;
        }
        #[cfg(unix)]
        if let Some(pid) = self.pid {
            // deew handles SIGTERM: it kills its tools, cleans up and exits.
            signal_group(pid, libc::SIGTERM);
            return;
        }
        self.hard_kill();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    #[cfg(unix)]
    use std::time::Instant;

    struct Run {
        #[cfg_attr(windows, allow(dead_code))] // keeps the session alive; only read by unix tests
        session: PtySession,
        output: Arc<Mutex<Vec<u8>>>,
        exit: mpsc::Receiver<(i32, Vec<String>)>,
    }

    impl Run {
        fn start(program: &Path, args: &[&str], env: Vec<(String, String)>) -> Run {
            Run::start_with(program, args, env, false)
        }

        fn start_with(
            program: &Path,
            args: &[&str],
            env: Vec<(String, String)>,
            events: bool,
        ) -> Run {
            let output = Arc::new(Mutex::new(Vec::new()));
            let sink = Arc::clone(&output);
            let events_log = Arc::new(Mutex::new(Vec::new()));
            let event_sink = Arc::clone(&events_log);
            let event_log = Arc::clone(&events_log);
            let (tx, exit) = mpsc::channel();
            let session = spawn(
                SpawnConfig {
                    program: program.to_path_buf(),
                    args: args.iter().map(|s| s.to_string()).collect(),
                    cols: 100,
                    rows: 30,
                    cwd: None,
                    env,
                    events,
                },
                move |bytes| sink.lock().unwrap().extend_from_slice(bytes),
                move |line| event_sink.lock().unwrap().push(line.to_string()),
                move |code| {
                    // Everything must have been delivered by now.
                    let _ = tx.send((code, event_log.lock().unwrap().clone()));
                },
            )
            .expect("spawn");
            Run {
                session,
                output,
                exit,
            }
        }

        fn text(&self) -> String {
            String::from_utf8_lossy(&self.output.lock().unwrap()).into_owned()
        }

        /// Exit code and the events delivered until the exit callback ran.
        fn wait_exit_events(&self) -> (i32, Vec<String>) {
            self.exit
                .recv_timeout(Duration::from_secs(20))
                .expect("process did not exit in time")
        }

        fn wait_exit(&self) -> i32 {
            self.wait_exit_events().0
        }

        #[cfg(unix)]
        fn wait_for(&self, needle: &str) {
            let start = Instant::now();
            while !self.text().contains(needle) {
                assert!(
                    start.elapsed() < Duration::from_secs(10),
                    "timed out waiting for {needle:?}, got {:?}",
                    self.text()
                );
                std::thread::sleep(Duration::from_millis(20));
            }
        }
    }

    /// The deew sidecar prepared by `scripts/build-sidecar.mjs`.
    fn sidecar() -> PathBuf {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../src-tauri/binaries");
        std::fs::read_dir(&dir)
            .unwrap_or_else(|e| panic!("{}: {e} (run `npm run sidecar` in gui/)", dir.display()))
            .filter_map(|entry| entry.ok().map(|e| e.path()))
            .find(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with("deew-") && n.ends_with(".exe") == cfg!(windows))
            })
            .unwrap_or_else(|| panic!("no deew-<triple> sidecar in {}", dir.display()))
    }

    #[test]
    fn runs_the_real_sidecar() {
        let config_home = tempfile::tempdir().unwrap();
        let run = Run::start(
            &sidecar(),
            &["-lb"],
            vec![(
                "XDG_CONFIG_HOME".into(),
                config_home.path().display().to_string(),
            )],
        );
        assert_eq!(run.wait_exit(), 0);
        // The output is a tree (`DDP:` header, `├─ 5.1: ...` leaves) that may
        // carry ANSI colors; strip CSI sequences before matching.
        let raw = run.text();
        let mut text = String::new();
        let mut chars = raw.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '\x1b' && chars.peek() == Some(&'[') {
                chars.next();
                for n in chars.by_ref() {
                    if ('@'..='~').contains(&n) {
                        break;
                    }
                }
            } else {
                text.push(c);
            }
        }
        for needle in ["DDP:", "AC4:", "├─ 5.1: ", "(default)"] {
            assert!(text.contains(needle), "missing {needle:?} in: {text:?}");
        }
    }

    #[test]
    fn reports_the_exit_code_of_the_sidecar() {
        let config_home = tempfile::tempdir().unwrap();
        let run = Run::start(
            &sidecar(),
            &["--bogus"],
            vec![(
                "XDG_CONFIG_HOME".into(),
                config_home.path().display().to_string(),
            )],
        );
        assert_eq!(run.wait_exit(), 2);
        assert!(run.text().contains("unrecognized arguments"));
    }

    #[test]
    fn missing_program_is_an_error() {
        let result = spawn(
            SpawnConfig {
                program: PathBuf::from("/definitely/not/here"),
                args: vec![],
                cols: 80,
                rows: 24,
                cwd: None,
                env: vec![],
                events: false,
            },
            |_| {},
            |_| {},
            |_| {},
        );
        assert!(result.is_err());
    }

    #[cfg(unix)]
    mod unix {
        use super::*;

        fn sh(script: &str) -> Run {
            Run::start(Path::new("/bin/sh"), &["-c", script], vec![])
        }

        fn sh_events(script: &str) -> Run {
            Run::start_with(Path::new("/bin/sh"), &["-c", script], vec![], true)
        }

        #[test]
        fn events_arrive_in_order_before_the_exit() {
            let run = sh_events(
                "echo \"path:$DEEW_EVENTS_FILE\"; \
                 echo '{\"a\":1}' >> \"$DEEW_EVENTS_FILE\"; \
                 printf '\\n{\"a\":2}\\r\\n' >> \"$DEEW_EVENTS_FILE\"; \
                 sleep 0.3; \
                 printf '{\"a\":3}' >> \"$DEEW_EVENTS_FILE\"; \
                 printf '{\"a\":4}\\n{\"a\"' >> \"$DEEW_EVENTS_FILE\"; \
                 printf ':5}' >> \"$DEEW_EVENTS_FILE\"",
            );
            let (code, events) = run.wait_exit_events();
            assert_eq!(code, 0);
            assert_eq!(
                events,
                [
                    r#"{"a":1}"#,
                    r#"{"a":2}"#,
                    r#"{"a":3}{"a":4}"#,
                    r#"{"a":5}"#
                ]
            );
            let text = run.text();
            let path = text
                .lines()
                .find_map(|l| l.trim().strip_prefix("path:"))
                .expect("path printed");
            let name = Path::new(path).file_name().unwrap().to_str().unwrap();
            assert!(name.starts_with("deew-events-") && name.ends_with(".jsonl"));
            assert!(Path::new(path).is_absolute());
            assert!(!Path::new(path).exists(), "events file was not removed");
        }

        #[test]
        fn no_events_file_without_events() {
            let run = sh("test -z \"$DEEW_EVENTS_FILE\"");
            assert_eq!(run.wait_exit_events(), (0, vec![]));
        }

        #[test]
        fn collects_output_and_exit_code() {
            let run = sh("printf 'hello\\n'; exit 3");
            assert_eq!(run.wait_exit(), 3);
            assert!(run.text().contains("hello"));
        }

        #[test]
        fn child_sees_a_tty_with_term_and_size() {
            let run = sh("test -t 0 && test -t 1 && echo tty:$TERM:$(stty size)");
            assert_eq!(run.wait_exit(), 0);
            assert!(
                run.text().contains("tty:xterm-256color:30 100"),
                "{:?}",
                run.text()
            );
        }

        #[test]
        fn stdin_reaches_the_child() {
            let mut run = sh("read answer; echo got:$answer");
            run.session.write(b"yes\r").unwrap();
            assert_eq!(run.wait_exit(), 0);
            assert!(run.text().contains("got:yes"), "{:?}", run.text());
        }

        #[test]
        fn resize_changes_the_window_size() {
            let mut run = sh("read x; stty size");
            run.session.resize(120, 40).unwrap();
            run.session.write(b"\r").unwrap();
            assert_eq!(run.wait_exit(), 0);
            assert!(run.text().contains("40 120"), "{:?}", run.text());
        }

        #[test]
        fn stop_interrupts_the_whole_process_group() {
            let mut run = sh("echo started; sleep 60; echo not-reached");
            run.wait_for("started");
            run.session.stop();
            let code = run.wait_exit();
            assert_ne!(code, 0);
        }

        #[test]
        fn drop_sends_sigterm() {
            let dir = tempfile::tempdir().unwrap();
            let marker = dir.path().join("marker");
            let script = format!(
                "trap 'echo term > {}; exit 0' TERM; echo started; while :; do sleep 0.1; done",
                marker.display()
            );
            let run = sh(&script);
            run.wait_for("started");
            drop(run.session);
            let start = Instant::now();
            while !marker.exists() {
                assert!(start.elapsed() < Duration::from_secs(5), "no SIGTERM");
                std::thread::sleep(Duration::from_millis(20));
            }
        }

        #[test]
        fn escalation_kills_processes_in_other_groups() {
            let mut run = sh("trap '' INT; set -m; sleep 100 & echo grandchild:$!; wait");
            run.wait_for("grandchild:");
            let text = run.text();
            let pid: i32 = text
                .split("grandchild:")
                .nth(1)
                .and_then(|t| t.split_whitespace().next())
                .and_then(|t| t.parse().ok())
                .expect("pid");
            let alive = |pid: i32| unsafe { libc::kill(pid, 0) == 0 };
            assert!(alive(pid));
            run.session.stop_with_grace(Duration::from_secs(1));
            run.wait_exit();
            let start = Instant::now();
            // A killed process may linger as a zombie until its parent reaps it.
            while alive(pid) && !is_zombie(pid) {
                assert!(start.elapsed() < Duration::from_secs(5), "still alive");
                std::thread::sleep(Duration::from_millis(50));
            }
        }

        fn is_zombie(pid: i32) -> bool {
            std::fs::read_to_string(format!("/proc/{pid}/stat"))
                .map(|s| s.rsplit(") ").next().is_some_and(|r| r.starts_with('Z')))
                .unwrap_or(true)
        }

        #[test]
        fn stop_escalates_to_kill() {
            let mut run = sh("trap '' INT; echo started; while :; do sleep 1; done");
            run.wait_for("started");
            run.session.stop_with_grace(Duration::from_secs(1));
            let start = Instant::now();
            let code = run.wait_exit();
            assert_ne!(code, 0);
            assert!(start.elapsed() >= Duration::from_millis(500));
        }
    }
}
