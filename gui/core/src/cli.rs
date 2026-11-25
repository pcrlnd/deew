//! Discovery and version of the deew CLI that the GUI runs.

use crate::settings::{GuiSettings, SettingsView};
use std::path::{Path, PathBuf};

/// File names under which the deew CLI is looked up in the deew folder.
///
/// Tauri places the bundled `externalBin` sidecar next to the main executable
/// without the target triple, so it is named `deew` (`deew.exe` on Windows).
/// On other platforms the CLI release assets are also accepted, so that
/// downloading the CLI and the GUI into one folder is enough:
/// `deew-macos`, `deew-linux`, and `deew_linux`.
pub fn sidecar_candidates(dir: &Path, windows: bool) -> Vec<PathBuf> {
    let names: &[&str] = if windows {
        &["deew.exe"]
    } else {
        &["deew", "deew-macos", "deew-linux", "deew_linux"]
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
                "the deew CLI was not found in {}. Looked for:{tried}\nUse Utilities \u{2192} Update and install all to download it.",
                dir.display()
            ))
        }
    }
}

/// Path of the deew CLI, located in the deew folder (see [`deew_dir`] and
/// [`sidecar_candidates`] for the accepted names).
pub fn sidecar_path() -> Result<PathBuf, String> {
    find_sidecar(&deew_dir()?, cfg!(windows))
}

/// Whether the deew CLI is present in the deew folder.
pub fn cli_found() -> bool {
    sidecar_path().is_ok()
}

/// The deew folder: where the CLI, `config.toml`, `dee/`, the tools and
/// `temp` live. See [`deew_dir_for`] for the rules.
pub fn deew_dir() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| format!("cannot locate the app: {e}"))?;
    let dir = exe
        .parent()
        .ok_or_else(|| "cannot locate the app directory".to_string())?;
    Ok(deew_dir_for(dir, std::env::home_dir().as_deref()))
}

/// The deew folder for a program in `exe_dir`:
/// - not inside a macOS app bundle (`<name>.app/Contents/MacOS`): `exe_dir`
///   itself (Windows, Linux, deew-web);
/// - in a bundle in `/Applications` or `$HOME/Applications`, or run
///   translocated by macOS (a path with an `AppTranslocation` component):
///   `$HOME/Library/Application Support/deew`;
/// - in a bundle elsewhere (portable): the folder containing the `.app`.
///
/// Without a home folder the last rule applies to every bundle.
pub fn deew_dir_for(exe_dir: &Path, home: Option<&Path>) -> PathBuf {
    if let Some(app) = app_bundle(exe_dir)
        && let Some(home) = home
        && !home.as_os_str().is_empty()
    {
        let parent = app.parent();
        let installed = parent == Some(Path::new("/Applications"))
            || parent == Some(&home.join("Applications"));
        if installed || is_translocated_dir(exe_dir) {
            return home.join("Library/Application Support/deew");
        }
    }
    outside_app_bundle(exe_dir)
}

/// Whether `dir` is under the random folder macOS moves a downloaded,
/// quarantined app to when it is run from where it was unpacked.
pub fn is_translocated_dir(dir: &Path) -> bool {
    dir.components()
        .any(|c| c.as_os_str() == "AppTranslocation")
}

/// Whether this program runs translocated (macOS only in practice).
pub fn is_translocated() -> bool {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(is_translocated_dir))
        .unwrap_or(false)
}

/// The `.app` folder when `dir` is the `Contents/MacOS` of an app bundle.
fn app_bundle(dir: &Path) -> Option<&Path> {
    let name = |p: &Path| p.file_name().map(|n| n.to_string_lossy().into_owned());
    let contents = dir.parent();
    let app = contents.and_then(Path::parent)?;
    (name(dir).as_deref() == Some("MacOS")
        && contents.and_then(name).as_deref() == Some("Contents")
        && name(app).is_some_and(|n| n.len() > ".app".len() && n.ends_with(".app")))
    .then_some(app)
}

/// The directory containing the app, for a program in `dir`: `dir` itself,
/// except inside a macOS app bundle (`<name>.app/Contents/MacOS`), where it
/// is the directory containing the `.app`.
pub fn outside_app_bundle(dir: &Path) -> PathBuf {
    if let Some(app) = app_bundle(dir)
        && let Some(outside) = app.parent()
    {
        return if outside.as_os_str().is_empty() {
            PathBuf::from(".")
        } else {
            outside.to_path_buf()
        };
    }
    dir.to_path_buf()
}

/// The config next to the CLI (the only config location); `None` without a
/// CLI. The CLI is never inside an app bundle, so its folder is used as is.
pub fn config_path() -> Option<PathBuf> {
    let cli_dir = sidecar_path()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf));
    crate::probe::config_path(cli_dir.as_deref())
}

/// Parses the output of `deew -v` (`deew X.Y.Z`, or `deew     X.Y.Z  /path`)
/// into the version.
pub fn parse_cli_version(stdout: &str) -> Option<String> {
    stdout.lines().find_map(|line| {
        let rest = line.trim().strip_prefix("deew")?;
        // "deew" must be followed by whitespace, not be the start of a word
        rest.starts_with(char::is_whitespace).then_some(())?;
        let version = rest.split_whitespace().next()?;
        version
            .starts_with(|c: char| c.is_ascii_digit())
            .then(|| version.to_string())
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

/// The GUI settings of the config next to the CLI (defaults when missing).
pub fn gui_settings() -> SettingsView {
    let config = config_path();
    SettingsView::new(crate::settings::load(config.as_deref()), config.as_deref())
}

/// Saves the GUI settings into the config next to the CLI.
pub fn save_gui_settings(settings: &GuiSettings) -> Result<(), String> {
    crate::settings::save(config_path().as_deref(), settings)
}

/// Saves only `gui.theme` into the config next to the CLI.
pub fn save_gui_theme(theme: crate::settings::ThemeMode) -> Result<(), String> {
    crate::settings::save_theme(config_path().as_deref(), theme)
}

/// Probes the inputs by asking the deew CLI next to this program
/// (`deew --probe-json`), so the answer is exactly what deew will do:
/// folders and patterns are expanded, the ffprobe is the one deew uses.
/// `track_index` is the `-ti` text, omitted when empty.
pub fn probe_inputs(
    inputs: &[String],
    track_index: &str,
) -> Result<crate::probe::ProbeResult, String> {
    let mut command = std::process::Command::new(sidecar_path()?);
    command.arg("--probe-json");
    if !track_index.trim().is_empty() {
        command.arg("-ti").arg(track_index.trim());
    }
    command
        .arg("-i")
        .args(inputs)
        .env("NO_COLOR", "1")
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // The release GUI has no console; do not flash one for deew.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let output = command
        .output()
        .map_err(|e| format!("failed to run the deew CLI: {e}"))?;
    crate::probe::parse_probe_json(&String::from_utf8_lossy(&output.stdout))
}

#[cfg(test)]
mod tests {
    #[test]
    fn cli_lookup_dir_is_outside_the_app() {
        let dir = outside_app_bundle(Path::new("/Users/x/deew/deew-gui.app/Contents/MacOS"));
        assert_eq!(dir, PathBuf::from("/Users/x/deew"));
        let candidates = sidecar_candidates(&dir, false);
        assert!(candidates.contains(&PathBuf::from("/Users/x/deew/deew-macos")));
        assert!(
            candidates
                .iter()
                .all(|c| !c.starts_with("/Users/x/deew/deew-gui.app"))
        );
    }

    #[test]
    fn app_bundle_dir_maps_to_its_parent() {
        let p = |s: &str| outside_app_bundle(Path::new(s));
        assert_eq!(p("/opt/tools"), PathBuf::from("/opt/tools"));
        assert_eq!(
            p("/Users/x/deew/deew-gui.app/Contents/MacOS"),
            PathBuf::from("/Users/x/deew")
        );
        assert_eq!(p("/deew-gui.app/Contents/MacOS"), PathBuf::from("/"));
        assert_eq!(
            p("/a/b/Contents/MacOS"),
            PathBuf::from("/a/b/Contents/MacOS")
        );
        assert_eq!(
            p("/a/x.app/Contents/Other"),
            PathBuf::from("/a/x.app/Contents/Other")
        );
        assert_eq!(
            p("/a/.app/Contents/MacOS"),
            PathBuf::from("/a/.app/Contents/MacOS")
        );
        assert_eq!(p("/"), PathBuf::from("/"));
        assert_eq!(p("MacOS"), PathBuf::from("MacOS"));
    }

    use super::*;

    #[test]
    fn deew_dir_rules() {
        let home = Path::new("/Users/x");
        let d = |exe: &str, home: Option<&Path>| deew_dir_for(Path::new(exe), home);
        let support = PathBuf::from("/Users/x/Library/Application Support/deew");
        // not a bundle: unchanged
        assert_eq!(d("/opt/deew", Some(home)), PathBuf::from("/opt/deew"));
        assert_eq!(
            d("/Applications", Some(home)),
            PathBuf::from("/Applications")
        );
        // installed
        assert_eq!(
            d("/Applications/deew-gui.app/Contents/MacOS", Some(home)),
            support
        );
        assert_eq!(
            d(
                "/Users/x/Applications/deew-gui.app/Contents/MacOS",
                Some(home)
            ),
            support
        );
        // translocated
        assert_eq!(
            d(
                "/private/var/folders/ab/T/AppTranslocation/1234/d/deew-gui.app/Contents/MacOS",
                Some(home)
            ),
            support
        );
        // portable
        assert_eq!(
            d("/Users/x/deew/deew-gui.app/Contents/MacOS", Some(home)),
            PathBuf::from("/Users/x/deew")
        );
        assert_eq!(
            d("/Applications/sub/deew-gui.app/Contents/MacOS", Some(home)),
            PathBuf::from("/Applications/sub")
        );
        assert_eq!(
            d(
                "/Users/y/Applications/deew-gui.app/Contents/MacOS",
                Some(home)
            ),
            PathBuf::from("/Users/y/Applications")
        );
        // no home: portable rule
        assert_eq!(
            d("/Applications/deew-gui.app/Contents/MacOS", None),
            PathBuf::from("/Applications")
        );
        assert_eq!(
            d("/tmp/AppTranslocation/1/d/x.app/Contents/MacOS", None),
            PathBuf::from("/tmp/AppTranslocation/1/d")
        );
        // a plain folder named Applications is no bundle
        assert_eq!(
            d("/Applications/Contents/MacOS", Some(home)),
            PathBuf::from("/Applications/Contents/MacOS")
        );
    }

    #[test]
    fn translocation_is_detected_by_component() {
        assert!(is_translocated_dir(Path::new(
            "/private/var/folders/x/AppTranslocation/1/d/a.app/Contents/MacOS"
        )));
        assert!(!is_translocated_dir(Path::new(
            "/Applications/AppTranslocationX"
        )));
        assert!(!is_translocated_dir(Path::new("/Applications/a.app")));
    }

    #[test]
    fn parses_cli_version() {
        assert_eq!(parse_cli_version("deew 3.2.2\n"), Some("3.2.2".into()));
        assert_eq!(
            parse_cli_version("\r\n deew 1.0.0-rc1 \r\n"),
            Some("1.0.0-rc1".into())
        );
        assert_eq!(
            parse_cli_version("deew     5.6.0  /some path/deew\nDEE      6.7.0  /x\n"),
            Some("5.6.0".into())
        );
        assert_eq!(parse_cli_version("deew     5.6.0\n"), Some("5.6.0".into()));
        assert_eq!(parse_cli_version("deewx 1.0\n"), None);
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
                dir.join("deew-macos"),
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
    fn sidecar_falls_back_to_deew_macos() {
        let tmp = tempfile::tempdir().unwrap();
        touch(tmp.path(), "deew-macos");
        touch(tmp.path(), "deew-linux");
        assert_eq!(
            find_sidecar(tmp.path(), false).unwrap(),
            tmp.path().join("deew-macos")
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
                "the deew CLI was not found in {d}. Looked for:\n  {d}/deew\n  {d}/deew-macos\n  {d}/deew-linux\n  {d}/deew_linux\nUse Utilities \u{2192} Update and install all to download it."
            )
        );
    }
}
