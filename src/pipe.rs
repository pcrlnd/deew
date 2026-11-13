//! The pipe that carries ffmpeg's raw PCM into a DEE tool.
//!
//! DEE does not take `-` as its input. On Unix its stdin is ffmpeg's stdout
//! and the input is `/dev/stdin`. On Windows DEE creates a named pipe itself
//! (`named_pipe://\\.\pipe\NAME`, the way Dolby's scripts use it) and ffmpeg
//! writes to the pipe like to a file. DEE has to be started first, and ffmpeg
//! only once the pipe exists, see [`exists`].

/// Whether DEE reads a named pipe that ffmpeg opens (Windows), instead of
/// the stdin that ffmpeg's stdout is connected to (Unix).
pub const NAMED: bool = cfg!(windows);

#[cfg(unix)]
const STDIN_PATH: &str = "/dev/stdin";
#[cfg(windows)]
const PREFIX: &str = r"\\.\pipe\deew-";
#[cfg(windows)]
const DEE_SCHEME: &str = "named_pipe://";

/// The pipe of job `job` (the index of the job in the run) as ffmpeg writes
/// to it: a path on Windows, `-` (its stdout) on Unix.
pub fn ffmpeg_output(job: usize) -> String {
    imp::name(job).unwrap_or_else(|| "-".to_string())
}

/// `-i` of DEE for the pipe of job `job`.
pub fn path(job: usize) -> String {
    match imp::name(job) {
        #[cfg(windows)]
        Some(name) => format!("{DEE_SCHEME}{name}"),
        #[cfg(not(windows))]
        Some(name) => name,
        None => imp::dee_stdin(),
    }
}

/// Whether `text` is a path made by [`path`] or [`ffmpeg_output`] (of this or
/// another job).
pub fn is_pipe_path(text: &str) -> bool {
    imp::is_pipe_path(text)
}

/// Whether the pipe of `dee_path` (made by [`path`]) has been created. It
/// waits a moment for it. This must not open the pipe: that would connect as
/// a client and could end the stream of DEE. Always true on Unix.
pub fn exists(dee_path: &str) -> bool {
    imp::exists(dee_path)
}

#[cfg(unix)]
mod imp {
    use super::STDIN_PATH;

    pub fn name(_job: usize) -> Option<String> {
        None
    }

    pub fn dee_stdin() -> String {
        STDIN_PATH.to_string()
    }

    pub fn is_pipe_path(text: &str) -> bool {
        text == STDIN_PATH
    }

    pub fn exists(_dee_path: &str) -> bool {
        true
    }
}

#[cfg(windows)]
mod imp {
    use super::{DEE_SCHEME, PREFIX};
    use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, GetLastError};
    use windows_sys::Win32::System::Pipes::WaitNamedPipeW;

    /// How long one check waits for a busy pipe, in milliseconds.
    const WAIT_MS: u32 = 50;

    pub fn name(job: usize) -> Option<String> {
        Some(format!("{PREFIX}{}-{job}", std::process::id()))
    }

    pub fn dee_stdin() -> String {
        unreachable!("Windows pipes have a name")
    }

    pub fn is_pipe_path(text: &str) -> bool {
        text.strip_prefix(DEE_SCHEME)
            .unwrap_or(text)
            .starts_with(PREFIX)
    }

    pub fn exists(dee_path: &str) -> bool {
        let name = dee_path.strip_prefix(DEE_SCHEME).unwrap_or(dee_path);
        let wide: Vec<u16> = name.encode_utf16().chain([0]).collect();
        // SAFETY: `wide` is null terminated. WaitNamedPipe does not connect.
        // It fails at once when no instance exists; a timeout means that the
        // instances are busy, so the pipe is there.
        unsafe {
            WaitNamedPipeW(wide.as_ptr(), WAIT_MS) != 0 || GetLastError() != ERROR_FILE_NOT_FOUND
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_path_is_recognized() {
        let first = path(0);
        assert!(is_pipe_path(&first));
        assert!(is_pipe_path(&path(7)));
        assert!(!is_pipe_path("/t/a.wav"));
        assert!(!is_pipe_path("-"));
        #[cfg(unix)]
        {
            assert_eq!(first, "/dev/stdin");
            assert_eq!(ffmpeg_output(0), "-");
        }
        #[cfg(windows)]
        {
            let pid = std::process::id();
            assert_eq!(first, format!(r"named_pipe://\\.\pipe\deew-{pid}-0"));
            assert_eq!(ffmpeg_output(0), format!(r"\\.\pipe\deew-{pid}-0"));
            assert!(is_pipe_path(&ffmpeg_output(7)));
            // jobs that run at the same time have their own pipes
            assert_ne!(first, path(1));
            assert_ne!(ffmpeg_output(0), ffmpeg_output(1));
        }
    }

    #[test]
    fn a_missing_pipe_does_not_exist_on_windows() {
        if cfg!(windows) {
            assert!(!exists(&path(987_654)));
        } else {
            assert!(exists(&path(0)));
        }
    }
}
