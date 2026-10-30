//! Discovery and version of the deew CLI that the GUI runs.

use crate::settings::GuiSettings;
use std::path::{Path, PathBuf};

/// File names under which the deew CLI is looked up next to the executable.
///
/// Tauri places the bundled `externalBin` sidecar next to the main executable
/// without the target triple, so it is named `deew` (`deew.exe` on Windows).
/// On other platforms the CLI release assets are also accepted, so that
/// downloading the CLI and the GUI into one folder is enough:
/// `deew-linux`, and `deew_linux` (the current release asset name).
pub fn sidecar_candidates(dir: &Path, windows: bool) -> Vec<PathBuf> {
    let names: &[&str] = if windows {
        &["deew.exe"]
    } else {
        &["deew", "deew-linux", "deew_linux"]
    };
    names.iter().map(|name| dir.join(name)).collect()
}

/// The first candidate in `dir` that is a file, or an error listing them all.
pub fn find_sidecar(dir: &Path, windows: bool) -> Result<PathBuf, String> {
    let candidates = sidecar_candidates(dir, windows);
    match candidates.iter().find(|path| path.is_file()) {
        Some(path) => Ok(path.clone()),
        None => {
            let tried: String = candidates
                .iter()
                .map(|path| format!("\n  {}", path.display()))
                .collect();
            Err(format!(
                "the deew CLI was not found next to this program. Looked for:{tried}"
            ))
        }
    }
}

/// Path of the deew CLI, located next to the running executable (see
/// [`sidecar_candidates`] for the accepted names).
pub fn sidecar_path() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| format!("cannot locate the app: {e}"))?;
    let dir = exe
        .parent()
        .ok_or_else(|| "cannot locate the app directory".to_string())?;
    find_sidecar(dir, cfg!(windows))
}

/// The config next to the CLI (the only config location); `None` without a CLI.
pub fn config_path() -> Option<PathBuf> {
    let cli_dir = sidecar_path()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf));
    crate::probe::config_path(cli_dir.as_deref())
}

/// Parses the output of `deew -v` (`deew X.Y.Z`) into the version.
pub fn parse_cli_version(stdout: &str) -> Option<String> {
    stdout.lines().find_map(|line| {
        let version = line.trim().strip_prefix("deew ")?.trim();
        let valid = !version.is_empty()
            && version.starts_with(|c: char| c.is_ascii_digit())
            && !version.contains(char::is_whitespace);
        valid.then(|| version.to_string())
    })
}

/// The version of the deew CLI found next to the running executable
/// (`deew -v`). Blocks while the CLI runs.
pub fn cli_version() -> Result<String, String> {
    let mut command = std::process::Command::new(sidecar_path()?);
    command
        .arg("-v")
        .env("NO_COLOR", "1")
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let output = command
        .output()
        .map_err(|e| format!("failed to run the deew CLI: {e}"))?;
    parse_cli_version(&String::from_utf8_lossy(&output.stdout))
        .ok_or_else(|| "unexpected output of `deew -v`".to_string())
}

/// The `[default_bitrates]` of the config next to the CLI (key -> kbps).
/// Empty when there is no CLI, config or table: nothing the UI has to show.
pub fn default_bitrates() -> std::collections::BTreeMap<String, i64> {
    crate::probe::default_bitrates(config_path().as_deref())
}

/// The `[gui]` settings of the config next to the CLI (defaults when missing).
pub fn gui_settings() -> GuiSettings {
    crate::settings::load(config_path().as_deref())
}

/// Saves the `[gui]` settings into the config next to the CLI.
pub fn save_gui_settings(settings: &GuiSettings) -> Result<(), String> {
    crate::settings::save(config_path().as_deref(), settings)
}

/// Probes the inputs (directories are expanded like the CLI does) with the
/// ffprobe that the deew config points to.
pub fn probe_inputs(
    inputs: &[String],
    track_index: u32,
) -> Result<crate::probe::ProbeResult, String> {
    let config = config_path();
    let ffprobe = crate::probe::resolve_ffprobe(config.as_deref())?;
    Ok(crate::probe::probe_files(&ffprobe, inputs, track_index))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_cli_version() {
        assert_eq!(parse_cli_version("deew 3.2.2\n"), Some("3.2.2".into()));
        assert_eq!(
            parse_cli_version("\r\n deew 1.0.0-rc1 \r\n"),
            Some("1.0.0-rc1".into())
        );
        assert_eq!(parse_cli_version("deew\n"), None);
        assert_eq!(parse_cli_version("deew x y\n"), None);
        assert_eq!(parse_cli_version("other 1.2.3"), None);
        assert_eq!(parse_cli_version(""), None);
    }

    fn touch(dir: &Path, name: &str) {
        std::fs::write(dir.join(name), b"").unwrap();
    }

    #[test]
    fn sidecar_candidates_order() {
        let dir = Path::new("/x");
        assert_eq!(
            sidecar_candidates(dir, false),
            vec![
                dir.join("deew"),
                dir.join("deew-linux"),
                dir.join("deew_linux")
            ]
        );
        assert_eq!(sidecar_candidates(dir, true), vec![dir.join("deew.exe")]);
    }

    #[test]
    fn sidecar_prefers_deew() {
        let tmp = tempfile::tempdir().unwrap();
        touch(tmp.path(), "deew-linux");
        touch(tmp.path(), "deew");
        assert_eq!(
            find_sidecar(tmp.path(), false).unwrap(),
            tmp.path().join("deew")
        );
    }

    #[test]
    fn sidecar_falls_back_to_deew_linux() {
        let tmp = tempfile::tempdir().unwrap();
        touch(tmp.path(), "deew-linux");
        touch(tmp.path(), "deew_linux");
        assert_eq!(
            find_sidecar(tmp.path(), false).unwrap(),
            tmp.path().join("deew-linux")
        );
    }

    #[test]
    fn sidecar_falls_back_to_deew_underscore_linux() {
        let tmp = tempfile::tempdir().unwrap();
        touch(tmp.path(), "deew_linux");
        assert_eq!(
            find_sidecar(tmp.path(), false).unwrap(),
            tmp.path().join("deew_linux")
        );
    }

    #[test]
    fn sidecar_error_lists_all_candidates() {
        let tmp = tempfile::tempdir().unwrap();
        // A directory with a candidate's name is not a file.
        std::fs::create_dir(tmp.path().join("deew")).unwrap();
        let err = find_sidecar(tmp.path(), false).unwrap_err();
        let d = tmp.path().display();
        assert_eq!(
            err,
            format!(
                "the deew CLI was not found next to this program. Looked for:\n  {d}/deew\n  {d}/deew-linux\n  {d}/deew_linux"
            )
        );
    }
}
