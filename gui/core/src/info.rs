//! The Utilities commands (`deew -v` and the like): the allowlist of flags,
//! the rules that keep them from clashing with an encode and the spawning of
//! such a command in its own pseudo-terminal (independent of the encode's).

use crate::pty_session::{self, PtySession, SpawnConfig};
use crate::{bootstrap, cli};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

/// The only arguments a Utilities run accepts (one flag, `-u all` is the
/// flag `-u` with its value; nothing else).
pub const INFO_FLAGS: [&str; 11] = [
    "-h", "-fh", "-v", "-cl", "-lb", "-pl", "-c", "-gc", "-gt", "-u", "-u all",
];

/// The allowlisted flag, as a static string; `None` for anything else.
pub fn allowed_flag(flag: &str) -> Option<&'static str> {
    INFO_FLAGS.iter().copied().find(|f| *f == flag)
}

/// `-gt` replaces the tools an encode uses and `-gc` rewrites the config, so
/// they exclude an encode (and the other way round).
pub fn excludes_encode(flag: &str) -> bool {
    matches!(flag, "-gt" | "-gc" | "-u" | "-u all")
}

#[derive(Default)]
struct GateState {
    encoding: bool,
    info: Option<&'static str>,
}

/// Who may run now: one encode and one Utilities command at a time, and no
/// `-gt` / `-gc` next to an encode. Both backends enforce it here.
#[derive(Default)]
pub struct Gate(Mutex<GateState>);

impl Gate {
    fn state(&self) -> MutexGuard<'_, GateState> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Claims the encode slot.
    pub fn begin_encode(&self) -> Result<(), String> {
        let mut state = self.state();
        if state.encoding {
            return Err("deew is already running".into());
        }
        if let Some(flag) = state.info.filter(|f| excludes_encode(f)) {
            return Err(format!(
                "cannot start an encode while `deew {flag}` is running"
            ));
        }
        state.encoding = true;
        Ok(())
    }

    pub fn end_encode(&self) {
        self.state().encoding = false;
    }

    /// Claims the Utilities slot for an allowlisted `flag`.
    pub fn begin_info(&self, flag: &str) -> Result<&'static str, String> {
        let flag = allowed_flag(flag).ok_or_else(|| format!("not a utility command: {flag}"))?;
        let mut state = self.state();
        if state.info.is_some() {
            return Err("a utility command is already running".into());
        }
        if state.encoding && excludes_encode(flag) {
            return Err(format!(
                "cannot run `deew {flag}` while an encode is running"
            ));
        }
        state.info = Some(flag);
        Ok(flag)
    }

    pub fn end_info(&self) {
        self.state().info = None;
    }

    pub fn info_running(&self) -> bool {
        self.state().info.is_some()
    }

    pub fn encode_running(&self) -> bool {
        self.state().encoding
    }
}

/// The arguments `deew` is started with for an allowlisted `flag`: `-gc` and
/// the updates get `-np`, as the GUI has asked for the confirmation already.
fn info_args(flag: &str) -> Vec<String> {
    let mut args: Vec<String> = flag.split(' ').map(String::from).collect();
    if matches!(flag, "-gc" | "-u" | "-u all") {
        args.push("-np".into());
    }
    args
}

/// Whether a Utilities flag downloads the CLI when there is none.
pub fn bootstraps(flag: &str) -> bool {
    matches!(flag, "-u" | "-u all")
}

type SharedPty = Arc<Mutex<Option<PtySession>>>;

/// A running Utilities command: the pseudo-terminal of `deew`, or, while the
/// CLI is being downloaded first, that download (and the pty once it starts).
pub enum InfoSession {
    Pty(PtySession),
    Bootstrap {
        cancel: Arc<AtomicBool>,
        pty: SharedPty,
    },
}

fn lock_pty(pty: &SharedPty) -> MutexGuard<'_, Option<PtySession>> {
    pty.lock().unwrap_or_else(|e| e.into_inner())
}

impl InfoSession {
    /// Keystrokes and terminal replies; dropped while downloading.
    pub fn write(&mut self, data: &[u8]) -> Result<(), String> {
        match self {
            InfoSession::Pty(pty) => pty.write(data),
            InfoSession::Bootstrap { pty, .. } => match lock_pty(pty).as_mut() {
                Some(pty) => pty.write(data),
                None => Ok(()),
            },
        }
    }

    pub fn resize(&self, cols: u16, rows: u16) -> Result<(), String> {
        match self {
            InfoSession::Pty(pty) => pty.resize(cols, rows),
            InfoSession::Bootstrap { pty, .. } => match lock_pty(pty).as_ref() {
                Some(pty) => pty.resize(cols, rows),
                None => Ok(()),
            },
        }
    }

    /// Interrupts the command, or the download.
    pub fn stop(&mut self) {
        match self {
            InfoSession::Pty(pty) => pty.stop(),
            InfoSession::Bootstrap { cancel, pty } => {
                cancel.store(true, Ordering::SeqCst);
                if let Some(pty) = lock_pty(pty).as_mut() {
                    pty.stop();
                }
            }
        }
    }
}

impl Drop for InfoSession {
    fn drop(&mut self) {
        if let InfoSession::Bootstrap { cancel, pty } = self {
            cancel.store(true, Ordering::SeqCst);
            // Dropping the pty session terminates the command.
            lock_pty(pty).take();
        }
    }
}

/// Starts `deew <flag>` in its own pseudo-terminal. `flag` must be
/// allowlisted and the [`Gate`] must allow it. The gate slot is freed before
/// `on_exit` is called (and when starting fails).
///
/// `program` is the CLI to run; `None` looks it up in the deew folder. When
/// that finds none, `-u` and `-u all` download it first (the progress goes
/// to `on_output` like the output of the command); other flags fail.
pub fn spawn_info(
    gate: &Arc<Gate>,
    program: Option<PathBuf>,
    flag: &str,
    size: (u16, u16),
    cwd: Option<PathBuf>,
    on_output: impl FnMut(&[u8]) + Send + 'static,
    on_exit: impl FnOnce(i32) + Send + 'static,
) -> Result<InfoSession, String> {
    let flag = gate.begin_info(flag)?;
    let program = match program.map(Ok).unwrap_or_else(cli::sidecar_path) {
        Ok(program) => Some(program),
        Err(_) if bootstraps(flag) => None,
        Err(e) => {
            gate.end_info();
            return Err(e);
        }
    };
    match program {
        Some(program) => {
            spawn_pty(gate, program, flag, size, cwd, on_output, on_exit).map(InfoSession::Pty)
        }
        None => Ok(spawn_bootstrap(
            gate,
            Arc::new(bootstrap::Github),
            flag,
            size,
            cwd,
            on_output,
            on_exit,
        )),
    }
}

fn spawn_pty(
    gate: &Arc<Gate>,
    program: PathBuf,
    flag: &'static str,
    (cols, rows): (u16, u16),
    cwd: Option<PathBuf>,
    on_output: impl FnMut(&[u8]) + Send + 'static,
    on_exit: impl FnOnce(i32) + Send + 'static,
) -> Result<PtySession, String> {
    let config = SpawnConfig {
        program,
        args: info_args(flag),
        cols,
        rows,
        cwd,
        env: vec![("COLORTERM".into(), "truecolor".into())],
        events: false,
    };
    let exit_gate = Arc::clone(gate);
    let spawned = pty_session::spawn(
        config,
        on_output,
        |_| {},
        move |code| {
            exit_gate.end_info();
            on_exit(code);
        },
    );
    if spawned.is_err() {
        gate.end_info();
    }
    spawned
}

/// Downloads the CLI on a thread, then runs the command in a pty like
/// [`spawn_pty`]. A failure is written to the terminal in red and ends the
/// run with exit code 1 (130 when stopped).
fn spawn_bootstrap(
    gate: &Arc<Gate>,
    fetch: Arc<dyn bootstrap::Fetch + Send + Sync>,
    flag: &'static str,
    size: (u16, u16),
    cwd: Option<PathBuf>,
    on_output: impl FnMut(&[u8]) + Send + 'static,
    on_exit: impl FnOnce(i32) + Send + 'static,
) -> InfoSession {
    let cancel = Arc::new(AtomicBool::new(false));
    let pty: SharedPty = Arc::new(Mutex::new(None));
    let session = InfoSession::Bootstrap {
        cancel: Arc::clone(&cancel),
        pty: Arc::clone(&pty),
    };
    let gate = Arc::clone(gate);
    let output = Arc::new(Mutex::new(on_output));
    let on_exit = Arc::new(Mutex::new(Some(on_exit)));
    let finish = Arc::new(move |code: i32| {
        let on_exit = on_exit.lock().unwrap_or_else(|e| e.into_inner()).take();
        if let Some(on_exit) = on_exit {
            on_exit(code);
        }
    });
    std::thread::spawn(move || {
        let emit = |text: &str| {
            let mut output = output.lock().unwrap_or_else(|e| e.into_inner());
            (*output)(text.as_bytes());
        };
        let installed = bootstrap::current_asset().and_then(|asset| {
            let dir = cli::deew_dir()?;
            bootstrap::install_cli(&*fetch, &dir, asset, &cancel, &mut |s| emit(s))
        });
        let program = match installed {
            Ok(program) => program,
            Err(e) => {
                let code = if e == bootstrap::CANCELLED {
                    130
                } else {
                    emit(&format!("\x1b[31m{e}\x1b[0m\r\n"));
                    1
                };
                gate.end_info();
                finish(code);
                return;
            }
        };
        emit("\r\n");
        let pty_output = Arc::clone(&output);
        let finish2 = Arc::clone(&finish);
        let started = spawn_pty(
            &gate,
            program,
            flag,
            size,
            cwd,
            move |bytes| {
                let mut output = pty_output.lock().unwrap_or_else(|e| e.into_inner());
                (*output)(bytes);
            },
            move |code| finish2(code),
        );
        match started {
            Ok(session) => {
                *lock_pty(&pty) = Some(session);
                // Stopped (or dropped) between the download and the start.
                if cancel.load(Ordering::SeqCst)
                    && let Some(session) = lock_pty(&pty).as_mut()
                {
                    session.stop();
                }
            }
            Err(e) => {
                // spawn_pty freed the gate and never reports a failed start.
                emit(&format!("\x1b[31m{e}\x1b[0m\r\n"));
                finish(1);
            }
        }
    });
    session
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_listed_flags_are_allowed() {
        for flag in INFO_FLAGS {
            assert_eq!(allowed_flag(flag), Some(flag));
        }
        for bad in ["", "-i", "-gt ", "--get-tools", "-o", "-h -v", "x", "-GT"] {
            assert_eq!(allowed_flag(bad), None, "{bad:?}");
        }
        let gate = Gate::default();
        assert!(gate.begin_info("-i").is_err());
        assert!(!gate.info_running());
    }

    #[test]
    fn only_the_updates_bootstrap() {
        for flag in INFO_FLAGS {
            assert_eq!(bootstraps(flag), matches!(flag, "-u" | "-u all"), "{flag}");
        }
    }

    #[test]
    fn generate_config_runs_without_its_own_question() {
        assert_eq!(info_args("-gc"), ["-gc", "-np"]);
        assert_eq!(info_args("-v"), ["-v"]);
        assert_eq!(info_args("-u"), ["-u", "-np"]);
        assert_eq!(info_args("-u all"), ["-u", "all", "-np"]);
    }

    #[test]
    fn download_and_config_exclude_an_encode() {
        let gate = Gate::default();
        gate.begin_encode().unwrap();
        assert!(gate.begin_info("-gt").unwrap_err().contains("encode"));
        assert!(gate.begin_info("-gc").is_err());
        assert!(gate.begin_info("-u").is_err());
        assert!(gate.begin_info("-u all").is_err());
        assert_eq!(gate.begin_info("-v"), Ok("-v"));
        gate.end_info();
        gate.end_encode();

        assert_eq!(gate.begin_info("-gt"), Ok("-gt"));
        assert!(gate.begin_encode().unwrap_err().contains("-gt"));
        gate.end_info();
        assert!(gate.begin_encode().is_ok());
    }

    #[test]
    fn harmless_utilities_run_next_to_an_encode() {
        let gate = Gate::default();
        gate.begin_encode().unwrap();
        for flag in ["-h", "-fh", "-v", "-cl", "-lb", "-pl", "-c"] {
            assert_eq!(gate.begin_info(flag), Ok(flag));
            gate.end_info();
        }
        assert!(gate.encode_running());
    }

    #[test]
    fn one_slot_each() {
        let gate = Gate::default();
        gate.begin_info("-v").unwrap();
        assert!(gate.begin_info("-h").unwrap_err().contains("already"));
        gate.begin_encode().unwrap();
        assert!(gate.begin_encode().unwrap_err().contains("already"));
    }

    #[cfg(unix)]
    #[test]
    fn spawn_info_passes_only_the_flag_and_frees_the_slot() {
        use std::sync::mpsc;
        let gate = Arc::new(Gate::default());
        let (tx, rx) = mpsc::channel();
        let _session = spawn_info(
            &gate,
            Some(PathBuf::from("/bin/echo")),
            "-v",
            (80, 24),
            None,
            |_| {},
            move |code| {
                let _ = tx.send(code);
            },
        )
        .unwrap();
        assert_eq!(rx.recv_timeout(std::time::Duration::from_secs(10)), Ok(0));
        assert!(!gate.info_running());
        // not allowlisted: nothing is spawned
        assert!(
            spawn_info(
                &gate,
                Some(PathBuf::from("/bin/echo")),
                "-i",
                (80, 24),
                None,
                |_| {},
                |_| {}
            )
            .is_err()
        );
        // a program that cannot start frees the slot
        assert!(
            spawn_info(
                &gate,
                Some(PathBuf::from("/nonexistent/deew")),
                "-v",
                (80, 24),
                None,
                |_| {},
                |_| {}
            )
            .is_err()
        );
        assert!(!gate.info_running());
    }
}
