//! Keeps the computer from sleeping while deew encodes or measures.
//!
//! Windows: `SetThreadExecutionState` (the display may still turn off),
//! released at the latest when the process ends. Linux: a `systemd-inhibit`
//! that runs `cat` on a pipe deew holds, so the lock ends with deew however
//! it ends (idle-only if polkit denies the sleep lock). macOS: `caffeinate` bound to deew's pid. Where none of that is
//! available nothing happens.

/// Holds the lock while it lives.
pub struct Awake {
    _inner: Inner,
}

/// Keep the system awake until the returned guard is dropped (or deew ends).
pub fn keep_awake() -> Awake {
    Awake {
        _inner: Inner::start(),
    }
}

#[cfg(windows)]
struct Inner;

#[cfg(windows)]
impl Inner {
    fn start() -> Self {
        use windows_sys::Win32::System::Power::{
            ES_CONTINUOUS, ES_SYSTEM_REQUIRED, SetThreadExecutionState,
        };
        // SAFETY: plain Win32 call without pointers. The state belongs to
        // this thread, which is the one that waits for the jobs and ends
        // the process.
        unsafe {
            SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED);
        }
        Inner
    }
}

#[cfg(windows)]
impl Drop for Inner {
    fn drop(&mut self) {
        use windows_sys::Win32::System::Power::{ES_CONTINUOUS, SetThreadExecutionState};
        // SAFETY: as above, clears the request of this thread.
        unsafe {
            SetThreadExecutionState(ES_CONTINUOUS);
        }
    }
}

#[cfg(unix)]
struct Inner {
    child: Option<std::process::Child>,
}

/// `systemd-inhibit` holding a lock of the `what` kinds (`idle:sleep`,
/// `idle`) for as long as `cat` reads its stdin.
#[cfg(unix)]
fn linux_command(what: &str) -> std::process::Command {
    let mut command = std::process::Command::new("systemd-inhibit");
    command.args([
        format!("--what={what}").as_str(),
        "--mode=block",
        "--who=deew",
        "--why=encoding audio",
        "cat",
    ]);
    command
}

/// `caffeinate` that prevents idle sleep until process `pid` ends.
#[cfg(unix)]
fn macos_command(pid: u32) -> std::process::Command {
    let mut command = std::process::Command::new("caffeinate");
    command.args(["-i", "-w", &pid.to_string()]);
    command
}

#[cfg(unix)]
impl Inner {
    fn start() -> Self {
        if cfg!(target_os = "macos") {
            return Inner {
                child: spawn(macos_command(std::process::id()), false),
            };
        }
        // not found, or no logind: nothing to do
        let Some(mut child) = spawn(linux_command("idle:sleep"), true) else {
            return Inner { child: None };
        };
        // polkit may deny the sleep lock (WSL, no active session): the tool
        // then exits at once, and an idle-only lock is still worth taking
        for _ in 0..20 {
            if child.try_wait().is_ok_and(|status| status.is_some()) {
                return Inner {
                    child: spawn(linux_command("idle"), true),
                };
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        Inner { child: Some(child) }
    }
}

/// Spawns the helper with null output and, on Linux, a piped stdin: the pipe
/// is the lock, its write end closes when deew ends.
#[cfg(unix)]
fn spawn(mut command: std::process::Command, piped: bool) -> Option<std::process::Child> {
    use std::process::Stdio;
    command
        .stdin(if piped { Stdio::piped() } else { Stdio::null() })
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    // its own process group: the Ctrl-C of the terminal doesn't reach it
    crate::interrupt::isolate(&mut command);
    command.spawn().ok()
}

#[cfg(unix)]
impl Drop for Inner {
    fn drop(&mut self) {
        let Some(mut child) = self.child.take() else {
            return;
        };
        if cfg!(target_os = "macos") {
            let _ = child.kill();
            let _ = child.wait();
            return;
        }
        // closing the pipe ends `cat` and with it the inhibitor
        drop(child.stdin.take());
        for _ in 0..50 {
            if child.try_wait().is_ok_and(|status| status.is_some()) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    fn line(command: &std::process::Command) -> Vec<String> {
        std::iter::once(command.get_program())
            .chain(command.get_args())
            .map(|s| s.to_string_lossy().into_owned())
            .collect()
    }

    #[cfg(unix)]
    #[test]
    fn linux_inhibitor_command() {
        assert_eq!(
            line(&linux_command("idle:sleep")),
            [
                "systemd-inhibit",
                "--what=idle:sleep",
                "--mode=block",
                "--who=deew",
                "--why=encoding audio",
                "cat"
            ]
        );
        assert_eq!(line(&linux_command("idle"))[1], "--what=idle");
    }

    #[cfg(unix)]
    #[test]
    fn macos_caffeinate_command() {
        assert_eq!(
            line(&macos_command(4242)),
            ["caffeinate", "-i", "-w", "4242"]
        );
    }

    #[test]
    fn guard_starts_and_drops_without_a_tool() {
        // works wherever the tool is missing or fails
        drop(keep_awake());
    }
}
