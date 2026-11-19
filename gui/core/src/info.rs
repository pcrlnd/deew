//! The Utilities commands (`deew -v` and the like): the allowlist of flags,
//! the rules that keep them from clashing with an encode and the spawning of
//! such a command in its own pseudo-terminal (independent of the encode's).

use crate::pty_session::{self, PtySession, SpawnConfig};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};

/// The only arguments a Utilities run accepts (one flag, nothing else).
pub const INFO_FLAGS: [&str; 9] = ["-h", "-fh", "-v", "-cl", "-lb", "-pl", "-c", "-gc", "-gt"];

/// The allowlisted flag, as a static string; `None` for anything else.
pub fn allowed_flag(flag: &str) -> Option<&'static str> {
    INFO_FLAGS.iter().copied().find(|f| *f == flag)
}

/// `-gt` replaces the tools an encode uses and `-gc` rewrites the config, so
/// they exclude an encode (and the other way round).
pub fn excludes_encode(flag: &str) -> bool {
    matches!(flag, "-gt" | "-gc")
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

/// The arguments `deew` is started with for an allowlisted `flag`: `-gc` gets
/// `-np`, as the GUI has asked for the confirmation already.
fn info_args(flag: &str) -> Vec<String> {
    if flag == "-gc" {
        vec!["-gc".into(), "-np".into()]
    } else {
        vec![flag.into()]
    }
}

/// Starts `deew <flag>` in its own pseudo-terminal. `flag` must be
/// allowlisted and the [`Gate`] must allow it. The gate slot is freed before
/// `on_exit` is called (and when starting fails).
pub fn spawn_info(
    gate: &Arc<Gate>,
    program: PathBuf,
    flag: &str,
    (cols, rows): (u16, u16),
    cwd: Option<PathBuf>,
    on_output: impl FnMut(&[u8]) + Send + 'static,
    on_exit: impl FnOnce(i32) + Send + 'static,
) -> Result<PtySession, String> {
    let flag = gate.begin_info(flag)?;
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
    fn generate_config_runs_without_its_own_question() {
        assert_eq!(info_args("-gc"), ["-gc", "-np"]);
        assert_eq!(info_args("-v"), ["-v"]);
    }

    #[test]
    fn download_and_config_exclude_an_encode() {
        let gate = Gate::default();
        gate.begin_encode().unwrap();
        assert!(gate.begin_info("-gt").unwrap_err().contains("encode"));
        assert!(gate.begin_info("-gc").is_err());
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
            PathBuf::from("/bin/echo"),
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
                PathBuf::from("/bin/echo"),
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
                PathBuf::from("/nonexistent/deew"),
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
