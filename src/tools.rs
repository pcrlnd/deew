//! The external programs (DEE tools, truehdd, harletty, dee_shim, ffmpeg, ffprobe) and
//! the temp directory.

use crate::data::ErrorKind;
use crate::error::print_exit;
use regex::Regex;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::LazyLock;

/// Executable suffix of the platform.
const EXE_SUFFIX: &str = if cfg!(windows) { ".exe" } else { "" };

/// `name` with the executable suffix of the platform (not twice).
fn exe_name(name: &str) -> String {
    if cfg!(windows) && !name.to_lowercase().ends_with(".exe") {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

/// Extra directories searched after the `PATH`. An app started from Finder
/// only gets `/usr/bin:/bin:/usr/sbin:/sbin`, so on macOS the Homebrew
/// locations are tried too.
#[cfg(target_os = "macos")]
const EXTRA_PATH_DIRS: &[&str] = &["/opt/homebrew/bin", "/usr/local/bin"];
#[cfg(not(target_os = "macos"))]
const EXTRA_PATH_DIRS: &[&str] = &[];

/// `which::which`, then the [`EXTRA_PATH_DIRS`].
fn which_extra(name: &str) -> Option<PathBuf> {
    which::which(name)
        .ok()
        .or_else(|| find_in_dirs(name, EXTRA_PATH_DIRS))
}

fn find_in_dirs(name: &str, dirs: &[&str]) -> Option<PathBuf> {
    dirs.iter()
        .map(|dir| Path::new(dir).join(name))
        .find(|path| path.is_file())
}

/// A program by its fixed name: first next to the deew executable
/// (`exe_dir`), then in the `PATH`. `None` if it does not exist.
pub fn find_program(name: &str, exe_dir: &Path, cwd: &Path) -> Option<String> {
    let local = exe_dir.join(exe_name(name));
    if local.is_file() {
        return Some(abspath(&local.to_string_lossy(), cwd));
    }
    let path = which_extra(name)?;
    Some(abspath(&path.to_string_lossy(), cwd))
}

/// The programs every run needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Binaries {
    pub ffmpeg: String,
    pub ffprobe: String,
}

/// Look up ffmpeg and ffprobe (see [`find_program`]). Exits with
/// `BinaryExist` if one is missing.
pub fn require_binaries(exe_dir: &Path, cwd: &Path) -> Binaries {
    let require = |name: &str| {
        find_program(name, exe_dir, cwd)
            .unwrap_or_else(|| print_exit(ErrorKind::BinaryExist, Some(name)))
    };
    Binaries {
        ffmpeg: require("ffmpeg"),
        ffprobe: require("ffprobe"),
    }
}

/// Path of a DEE tool: `<exe_dir>/dee/<tool>` (`.exe` on Windows), else the
/// first match in the `PATH`. `None` if it does not exist.
pub fn dee_tool(tool: &str, cwd: &Path, exe_dir: &Path) -> Option<String> {
    let local = exe_dir.join("dee").join(exe_name(tool));
    if local.is_file() {
        return Some(abspath(&local.to_string_lossy(), cwd));
    }
    which_extra(tool).map(|p| abspath(&p.to_string_lossy(), cwd))
}

/// Like [`dee_tool`] but exits with an error when the tool is missing.
pub fn require_dee_tool(tool: &str, cwd: &Path, exe_dir: &Path) -> String {
    dee_tool(tool, cwd, exe_dir).unwrap_or_else(|| {
        let name = format!("{tool}{EXE_SUFFIX}");
        if tool == "dee_dthd_encoder" && cfg!(target_os = "linux") {
            print_exit(ErrorKind::LinuxDthd, Some(&name))
        }
        print_exit(ErrorKind::BinaryExist, Some(&name))
    })
}

/// The truehdd binary (see [`find_program`]).
pub fn find_truehdd(exe_dir: &Path, cwd: &Path) -> Option<String> {
    find_program("truehdd", exe_dir, cwd)
}

/// Like [`find_truehdd`] but exits with an error when it is missing.
pub fn require_truehdd(exe_dir: &Path, cwd: &Path) -> String {
    find_truehdd(exe_dir, cwd)
        .unwrap_or_else(|| print_exit(ErrorKind::BinaryExist, Some("truehdd")))
}

/// The harletty binary (see [`find_program`]).
pub fn find_harletty(exe_dir: &Path, cwd: &Path) -> Option<String> {
    find_program("harletty", exe_dir, cwd)
}

/// Like [`find_harletty`] but exits with an error when it is missing.
pub fn require_harletty(exe_dir: &Path, cwd: &Path) -> String {
    find_harletty(exe_dir, cwd).unwrap_or_else(|| print_exit(ErrorKind::HarlettyMissing, None))
}

/// dee_shim: the `dee_shim` runner (it injects the library next to it).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shim {
    pub run: String,
}

/// The shim at `run`, if it is a file (`dee_shim` is a single executable).
fn shim_at(run: &Path, cwd: &Path) -> Option<Shim> {
    let run = PathBuf::from(abspath(&run.to_string_lossy(), cwd));
    run.is_file().then(|| Shim {
        run: run.to_string_lossy().into_owned(),
    })
}

/// Look for dee_shim: `dee_shim` next to the deew executable, else in the
/// `PATH`.
pub fn find_shim(cwd: &Path, exe_dir: &Path) -> Option<Shim> {
    let run_name = format!("dee_shim{EXE_SUFFIX}");
    shim_at(&exe_dir.join(&run_name), cwd).or_else(|| shim_at(&which_extra(&run_name)?, cwd))
}

/// `os.path.abspath`: absolute and lexically normalised. On Windows both
/// separators are accepted and the result uses `\`.
pub fn abspath(path: &str, cwd: &Path) -> String {
    let joined = cwd.join(path);
    // `D:dir` (a drive without a root) is relative to the current directory
    // of drive D, which only the OS knows.
    #[cfg(windows)]
    let joined = if joined.has_root() {
        joined
    } else {
        std::path::absolute(&joined).unwrap_or(joined)
    };
    let mut out = PathBuf::new();
    for component in joined.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                // `..` above the root stays at the root.
                if matches!(out.components().next_back(), Some(Component::Normal(_))) {
                    out.pop();
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    out.to_string_lossy().into_owned()
}

/// The temp directory: the configured one, else `temp` next to the
/// executable. Absolute and normalised, not yet created.
pub fn temp_path(configured: &str, exe_dir: &Path) -> String {
    let chosen = if configured.is_empty() {
        exe_dir.join("temp").to_string_lossy().into_owned()
    } else {
        configured.to_string()
    };
    let cwd = std::env::current_dir().unwrap_or_default();
    abspath(&chosen, &cwd)
}

/// Third word of the first line, without commas and `-static`, at most 30
/// characters.
pub fn parse_version_output(stdout: &str) -> Option<String> {
    let line = stdout.split('\n').next()?;
    let word = line.split(' ').nth(2)?;
    let version = word.replace(',', "").replace("-static", "");
    Some(if version.chars().count() > 30 {
        let head: String = version.chars().take(27).collect();
        format!("{head}...")
    } else {
        version
    })
}

pub const COULDNT_PARSE: &str = "[red]couldn't parse";

/// What a program prints on stdout (it is run with `args`); `None` if it
/// cannot be run.
fn output_of(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    Some(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// What is known about the version of a program.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Version {
    Found(String),
    /// The program doesn't exist or could not be started.
    #[default]
    NotFound,
    /// It ran, but its output has no version.
    CouldntParse,
}

impl Version {
    /// Plain text, for the report of `-kl`.
    pub fn text(&self) -> String {
        match self {
            Version::Found(version) => version.clone(),
            Version::NotFound => "not found".to_string(),
            Version::CouldntParse => "couldn't parse".to_string(),
        }
    }

    fn from_output(output: Option<String>, parse: impl Fn(&str) -> Option<String>) -> Version {
        match output {
            None => Version::NotFound,
            Some(text) => parse(&text).map_or(Version::CouldntParse, Version::Found),
        }
    }
}

/// Version of ffmpeg or ffprobe.
pub fn version_of(program: &str, args: &[&str]) -> Version {
    Version::from_output(output_of(program, args), parse_version_output)
}

static DEE_VERSION: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"Dolby Encoding Engine version ([0-9]+\.[0-9]+\.[0-9]+)").expect("valid regex")
});

/// `6.7.0` from `This tool belongs to the Dolby Encoding Engine version 6.7.0+8e353f99`.
pub fn parse_dee_version(help: &str) -> Option<String> {
    DEE_VERSION.captures(help).map(|c| c[1].to_string())
}

/// `0.6.2` from `truehdd 0.6.2 (truehd library 0.7.2) Built: ...`.
pub fn parse_truehdd_version(output: &str) -> Option<String> {
    let mut words = output.lines().next()?.split_whitespace();
    (words.next()? == "truehdd").then_some(())?;
    words.next().map(str::to_string)
}

/// `0.9.0` from the output of `harletty taxonomy`, one JSON object whose
/// `harletty` field is the crate version. (`harletty --version` only prints
/// a git hash.)
pub fn parse_harletty_version(output: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(output.lines().next()?).ok()?;
    let version = value.get("harletty")?.as_str()?.trim();
    (!version.is_empty()).then(|| version.to_string())
}

/// `0.1.0+282-g0301b3e` from `dee_shim v0.1.0+282-g0301b3e`.
pub fn parse_shim_version(output: &str) -> Option<String> {
    let mut words = output.lines().next()?.split_whitespace();
    (words.next()? == "dee_shim").then_some(())?;
    let version = words.next()?.strip_prefix('v')?;
    (!version.is_empty()).then(|| version.to_string())
}

/// The oldest dee_shim deew works with: it passes a DEE `named_pipe://` URI on
/// (0.2.0), has `measure_loudness` with `--layout` (0.3.0) and
/// `--unpadded-wav` (0.4.0), and has `--unpadded` instead (0.5.0), which also
/// reads raw PCM, through a pipe too.
pub const MIN_SHIM: [u64; 3] = [0, 5, 0];
/// The oldest truehdd deew works with.
pub const MIN_TRUEHDD: [u64; 3] = [0, 6, 3];
/// The oldest harletty deew works with (the first release with `info --json`,
/// `taxonomy` and the DTS input of `decode`).
pub const MIN_HARLETTY: [u64; 3] = [0, 9, 0];

/// `[0, 1, 0]` from `0.1.0+282-g0301b3e`: the numbers before the first `+`
/// or `-` (build metadata and pre-release tags are ignored). Missing parts
/// are 0, anything that is not a number is `None`.
pub fn version_numbers(version: &str) -> Option<[u64; 3]> {
    let core = version.split(['+', '-']).next()?;
    let mut numbers = [0; 3];
    let mut parts = core.split('.');
    for number in &mut numbers {
        match parts.next() {
            Some(part) => *number = part.parse().ok()?,
            None => break,
        }
    }
    parts.next().is_none().then_some(numbers)
}

/// `Ok` if `version` is at least `min`; else `Err` with the version when it
/// is too old, or `None` when it is unknown or not a version number.
pub fn check_min_version(version: &Version, min: [u64; 3]) -> Result<(), Option<String>> {
    let Version::Found(text) = version else {
        return Err(None);
    };
    match version_numbers(text) {
        Some(numbers) if numbers >= min => Ok(()),
        Some(_) => Err(Some(text.clone())),
        None => Err(None),
    }
}

/// Exits with an error if dee_shim is older than [`MIN_SHIM`] or its version
/// can't be read. Run once, for a run that uses it.
pub fn require_shim_version(shim: &Shim) {
    match check_min_version(&shim_version(shim), MIN_SHIM) {
        Ok(()) => {}
        Err(Some(found)) => print_exit(ErrorKind::ShimOld, Some(&found)),
        Err(None) => print_exit(ErrorKind::ShimVersion, None),
    }
}

/// Exits with an error if truehdd is older than [`MIN_TRUEHDD`] or its
/// version can't be read. Run once, for a run that uses it.
pub fn require_truehdd_version(truehdd: &str) {
    match check_min_version(&truehdd_version(truehdd), MIN_TRUEHDD) {
        Ok(()) => {}
        Err(Some(found)) => print_exit(ErrorKind::TruehddOld, Some(&found)),
        Err(None) => print_exit(ErrorKind::TruehddVersion, None),
    }
}

/// Exits with an error if harletty is older than [`MIN_HARLETTY`] or its
/// version can't be read. Run once, for a run that uses it.
pub fn require_harletty_version(harletty: &str) {
    match check_min_version(&harletty_version(harletty), MIN_HARLETTY) {
        Ok(()) => {}
        Err(Some(found)) => print_exit(ErrorKind::HarlettyOld, Some(&found)),
        Err(None) => print_exit(ErrorKind::HarlettyVersion, None),
    }
}

/// Version of the DEE tools, read from the `--help` of one of them.
pub fn dee_version(tool: &str) -> Version {
    Version::from_output(output_of(tool, &["--help"]), parse_dee_version)
}

pub fn truehdd_version(truehdd: &str) -> Version {
    Version::from_output(output_of(truehdd, &["--version"]), parse_truehdd_version)
}

/// The version of harletty, from `harletty taxonomy`.
pub fn harletty_version(harletty: &str) -> Version {
    Version::from_output(output_of(harletty, &["taxonomy"]), parse_harletty_version)
}

pub fn shim_version(shim: &Shim) -> Version {
    Version::from_output(output_of(&shim.run, &["--version"]), parse_shim_version)
}

/// Versions of the programs, in the order of the summary.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Versions {
    pub dee: Version,
    pub truehdd: Version,
    pub harletty: Version,
    pub shim: Version,
    pub ffmpeg: Version,
    pub ffprobe: Version,
}

/// Run the six version probes at the same time. `dee` is one of the DEE
/// tools, `truehdd` and `harletty` their binaries, each `None` if it was not
/// found.
pub fn probe_versions(
    dee: Option<&str>,
    truehdd: Option<&str>,
    harletty: Option<&str>,
    shim: Option<&Shim>,
    ffmpeg: &str,
    ffprobe: &str,
) -> Versions {
    let found = std::sync::Mutex::new(Versions::default());
    probe_versions_each(
        dee,
        truehdd,
        harletty,
        shim,
        ffmpeg,
        ffprobe,
        |index, version| {
            let mut all = found.lock().expect("versions");
            match index {
                0 => all.dee = version,
                1 => all.truehdd = version,
                2 => all.harletty = version,
                3 => all.shim = version,
                4 => all.ffmpeg = version,
                _ => all.ffprobe = version,
            }
        },
    );
    found.into_inner().expect("versions")
}

/// Like [`probe_versions`], but `done(index, version)` is called from the
/// probe's thread as soon as each probe finishes (`index` in the order of
/// [`Versions`]). Returns when all are done.
pub fn probe_versions_each(
    dee: Option<&str>,
    truehdd: Option<&str>,
    harletty: Option<&str>,
    shim: Option<&Shim>,
    ffmpeg: &str,
    ffprobe: &str,
    done: impl Fn(usize, Version) + Sync,
) {
    let done = &done;
    std::thread::scope(|scope| {
        scope.spawn(move || done(0, dee.map_or(Version::NotFound, dee_version)));
        scope.spawn(move || done(1, truehdd.map_or(Version::NotFound, truehdd_version)));
        scope.spawn(move || done(2, harletty.map_or(Version::NotFound, harletty_version)));
        scope.spawn(move || done(3, shim.map_or(Version::NotFound, shim_version)));
        scope.spawn(move || done(4, version_of(ffmpeg, &["-version"])));
        scope.spawn(move || done(5, version_of(ffprobe, &["-version"])));
    });
}

#[cfg(test)]
mod tests {
    #[test]
    fn find_in_dirs_checks_each_dir() {
        let dir = temp_dir("extra_dirs");
        let d = dir.to_str().unwrap();
        assert_eq!(find_in_dirs("deew_extra_tool", &[d]), None);
        std::fs::write(dir.join("deew_extra_tool"), "").unwrap();
        assert_eq!(
            find_in_dirs("deew_extra_tool", &["/nonexistent", d]),
            Some(dir.join("deew_extra_tool"))
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn macos_extra_dirs_are_homebrew() {
        assert_eq!(EXTRA_PATH_DIRS, ["/opt/homebrew/bin", "/usr/local/bin"]);
    }

    #[test]
    #[cfg(not(target_os = "macos"))]
    fn other_systems_have_no_extra_dirs() {
        assert!(EXTRA_PATH_DIRS.is_empty());
    }

    use super::*;

    #[test]
    fn version_numbers_ignore_the_build() {
        assert_eq!(version_numbers("0.1.0+282-g0301b3e"), Some([0, 1, 0]));
        assert_eq!(version_numbers("0.4.0+g790d050"), Some([0, 4, 0]));
        assert_eq!(version_numbers("0.10.2"), Some([0, 10, 2]));
        assert_eq!(version_numbers("1.0.0-rc1"), Some([1, 0, 0]));
        assert_eq!(version_numbers("0.6"), Some([0, 6, 0]));
        assert_eq!(version_numbers("1.2.3.4"), None);
        assert_eq!(version_numbers("x.y.z"), None);
        assert_eq!(version_numbers(""), None);
    }

    #[test]
    fn minimum_versions_are_compared_by_number() {
        let found = |v: &str| Version::Found(v.to_string());
        assert_eq!(check_min_version(&found("0.5.0"), MIN_SHIM), Ok(()));
        assert_eq!(
            check_min_version(&found("0.5.0+g790d050"), MIN_SHIM),
            Ok(())
        );
        // 0.4.x has `--unpadded-wav` only
        assert_eq!(
            check_min_version(&found("0.4.9"), MIN_SHIM),
            Err(Some("0.4.9".to_string()))
        );
        // 10 is more than 4, not a text comparison
        assert_eq!(check_min_version(&found("0.10.0"), MIN_SHIM), Ok(()));
        assert_eq!(check_min_version(&found("1.0.0"), MIN_SHIM), Ok(()));
        assert_eq!(
            check_min_version(&found("0.3.0+12-gabc"), MIN_SHIM),
            Err(Some("0.3.0+12-gabc".to_string()))
        );
        assert_eq!(
            check_min_version(&found("0.2.1+gc1141a0"), MIN_SHIM),
            Err(Some("0.2.1+gc1141a0".to_string()))
        );
        assert_eq!(check_min_version(&found("0.6.3"), MIN_TRUEHDD), Ok(()));
        assert_eq!(
            check_min_version(&found("0.6.2"), MIN_TRUEHDD),
            Err(Some("0.6.2".to_string()))
        );
        assert_eq!(check_min_version(&Version::NotFound, MIN_SHIM), Err(None));
        assert_eq!(
            check_min_version(&Version::CouldntParse, MIN_TRUEHDD),
            Err(None)
        );
        assert_eq!(check_min_version(&found("dev"), MIN_SHIM), Err(None));
    }

    #[test]
    fn static_regexes_compile() {
        LazyLock::force(&DEE_VERSION);
    }

    #[cfg(unix)]
    #[test]
    fn abspath_normalises() {
        let cwd = Path::new("/home/u/work");
        assert_eq!(abspath("temp", cwd), "/home/u/work/temp");
        assert_eq!(abspath("./a/../b/./c", cwd), "/home/u/work/b/c");
        assert_eq!(abspath("../x", cwd), "/home/u/x");
        assert_eq!(abspath("/abs//path/", cwd), "/abs/path");
        assert_eq!(abspath("/a/b/../../..", cwd), "/");
        assert_eq!(abspath("/..", cwd), "/");
        assert_eq!(
            abspath("/mnt/c/Users/me/AppData/Local/Temp/deew", cwd),
            "/mnt/c/Users/me/AppData/Local/Temp/deew"
        );
    }

    #[cfg(windows)]
    #[test]
    fn abspath_on_windows() {
        let cwd = Path::new("C:\\Users\\me\\work");
        for (path, expected) in [
            ("temp", "C:\\Users\\me\\work\\temp"),
            ("C:\\dir\\file.wav", "C:\\dir\\file.wav"),
            ("C:/dir/file.wav", "C:\\dir\\file.wav"),
            ("c:/dir//sub/../x/", "c:\\dir\\x"),
            ("D:/tmp/", "D:\\tmp"),
            ("..\\x", "C:\\Users\\me\\x"),
            ("./a/../b", "C:\\Users\\me\\work\\b"),
            ("\\foo", "C:\\foo"),
            ("/foo/bar", "C:\\foo\\bar"),
            ("C:\\", "C:\\"),
            ("C:\\a\\b\\..\\..\\..", "C:\\"),
            ("C:\\..", "C:\\"),
            ("\\\\srv\\share\\x\\..\\y", "\\\\srv\\share\\y"),
        ] {
            assert_eq!(abspath(path, cwd), expected, "{path:?}");
        }
        // A drive-relative path of another drive is resolved by the OS.
        let other = abspath("D:foo", cwd);
        assert!(
            other.starts_with("D:\\") && other.ends_with("\\foo"),
            "{other}"
        );
    }

    #[test]
    fn temp_directory() {
        let exe_dir = Path::new(if cfg!(windows) {
            "C:\\Program Files\\deew"
        } else {
            "/opt/deew"
        });
        let default = exe_dir.join("temp").to_string_lossy().into_owned();
        assert_eq!(temp_path("", exe_dir), default);
        #[cfg(unix)]
        assert_eq!(temp_path("/data/tmp/", exe_dir), "/data/tmp");
        #[cfg(windows)]
        assert_eq!(temp_path("D:/work/tmp/", exe_dir), "D:\\work\\tmp");
    }

    #[test]
    fn version_parsing() {
        assert_eq!(
            parse_version_output("dee, Version 5.2.1, Jun 10 2022\nmore").as_deref(),
            Some("5.2.1")
        );
        assert_eq!(
            parse_version_output("ffmpeg version 7.1.5 Copyright (c) 2000-2026\n").as_deref(),
            Some("7.1.5")
        );
        assert_eq!(
            parse_version_output("ffmpeg version 6.0-static https://x\n").as_deref(),
            Some("6.0")
        );
        let long = format!("ffmpeg version {} x", "a".repeat(40));
        assert_eq!(
            parse_version_output(&long),
            Some(format!("{}...", "a".repeat(27)))
        );
        let exact = format!("ffmpeg version {} x", "a".repeat(30));
        assert_eq!(parse_version_output(&exact), Some("a".repeat(30)));
        assert_eq!(parse_version_output(""), None);
        assert_eq!(parse_version_output("one two"), None);
    }

    #[test]
    fn missing_programs_have_no_versions() {
        let v = probe_versions(
            None,
            None,
            None,
            None,
            "/nonexistent/ffmpeg",
            "/nonexistent/ffprobe",
        );
        assert_eq!(v, Versions::default());
        let v = probe_versions(
            Some("/nonexistent/dee"),
            Some("/nonexistent/t"),
            Some("/nonexistent/h"),
            None,
            "x",
            "y",
        );
        assert_eq!(v, Versions::default());
    }

    #[test]
    fn missing_program_is_reported() {
        assert_eq!(version_of("/nonexistent/program", &[]), Version::NotFound);
        assert_eq!(dee_version("/nonexistent/program"), Version::NotFound);
    }

    #[cfg(unix)]
    #[test]
    fn program_without_a_version_is_not_parsed() {
        // it runs, but prints no version
        assert_eq!(version_of("true", &[]), Version::CouldntParse);
        assert_eq!(dee_version("true"), Version::CouldntParse);
        assert_eq!(truehdd_version("true"), Version::CouldntParse);
    }

    #[test]
    fn dee_version_from_help() {
        let help = "\nDolby Digital Plus encoder.\n\nThis tool belongs to the Dolby Encoding Engine version 6.7.0+8e353f99\nInterface version: 2.0.0\n";
        assert_eq!(parse_dee_version(help).as_deref(), Some("6.7.0"));
        assert_eq!(
            parse_dee_version("Dolby Encoding Engine version 6.10.12"),
            Some("6.10.12".into())
        );
        assert_eq!(parse_dee_version("no version here"), None);
    }

    #[test]
    fn harletty_version_from_taxonomy() {
        let taxonomy = r#"{"schema":1,"harletty":"0.9.0","build":"9556bce (truehd library 0.7.2) Built: 2026-11-01","taxonomy":4}"#;
        assert_eq!(
            parse_harletty_version(&format!("{taxonomy}\n")).as_deref(),
            Some("0.9.0")
        );
        // the version output is a git hash, not a version
        assert_eq!(
            parse_harletty_version("harletty 9556bce (truehd library 0.7.2) Built: 2026-11-01"),
            None
        );
        assert_eq!(parse_harletty_version(""), None);
        assert_eq!(parse_harletty_version(r#"{"harletty":""}"#), None);
        assert_eq!(parse_harletty_version(r#"{"harletty":7}"#), None);
        assert_eq!(parse_harletty_version(r#"{"schema":1}"#), None);
        let found = |v: &str| Version::Found(v.to_string());
        assert_eq!(check_min_version(&found("0.9.0"), MIN_HARLETTY), Ok(()));
        assert_eq!(check_min_version(&found("0.10.1"), MIN_HARLETTY), Ok(()));
        assert_eq!(
            check_min_version(&found("0.8.9"), MIN_HARLETTY),
            Err(Some("0.8.9".into()))
        );
        assert_eq!(
            check_min_version(&Version::NotFound, MIN_HARLETTY),
            Err(None)
        );
        #[cfg(unix)]
        assert_eq!(harletty_version("true"), Version::CouldntParse);
        assert_eq!(harletty_version("/nonexistent/h"), Version::NotFound);
    }

    #[test]
    fn truehdd_and_shim_versions() {
        assert_eq!(
            parse_truehdd_version("truehdd 0.6.2 (truehd library 0.7.2) Built: 2026-09-15\n")
                .as_deref(),
            Some("0.6.2")
        );
        assert_eq!(parse_truehdd_version("something else 1.0"), None);
        assert_eq!(parse_truehdd_version(""), None);
        assert_eq!(
            parse_shim_version("dee_shim v0.1.0+g294c9d1\n").as_deref(),
            Some("0.1.0+g294c9d1")
        );
        assert_eq!(
            parse_shim_version("dee_shim v0.1.0+282-g0301b3e\n").as_deref(),
            Some("0.1.0+282-g0301b3e")
        );
        assert_eq!(parse_shim_version("dee_shim 0.1.0"), None);
        assert_eq!(parse_shim_version("dee_shim v"), None);
        assert_eq!(parse_shim_version("truehdd v1.0"), None);
        assert_eq!(parse_shim_version(""), None);
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("deew_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn programs_are_looked_for_next_to_deew_first() {
        let exe_dir = temp_dir("exe");
        let elsewhere = temp_dir("cwd");
        let name = format!("deew-test-program-{}", std::process::id());
        // not anywhere
        assert_eq!(find_program(&name, &exe_dir, &elsewhere), None);
        let file = exe_dir.join(exe_name(&name));
        std::fs::write(&file, "").unwrap();
        assert_eq!(
            find_program(&name, &exe_dir, &elsewhere),
            Some(file.to_string_lossy().into_owned())
        );
        std::fs::remove_dir_all(&exe_dir).unwrap();
        std::fs::remove_dir_all(&elsewhere).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn programs_next_to_deew_win_over_the_path() {
        use std::os::unix::fs::PermissionsExt;
        let exe_dir = temp_dir("exe_path");
        // `sh` is in the PATH
        assert!(
            find_program("sh", &exe_dir, Path::new("/"))
                .is_some_and(|p| !p.starts_with(exe_dir.to_str().unwrap()))
        );
        let local = exe_dir.join("sh");
        std::fs::write(&local, "#!/bin/sh\n").unwrap();
        std::fs::set_permissions(&local, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(
            find_program("sh", &exe_dir, Path::new("/")),
            Some(local.to_string_lossy().into_owned())
        );
        std::fs::remove_dir_all(&exe_dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn dee_tools_and_shim_next_to_deew() {
        let exe_dir = temp_dir("exe_dee");
        let cwd = Path::new("/");
        assert_eq!(dee_tool("dee_deew_test_tool", cwd, &exe_dir), None);
        std::fs::create_dir_all(exe_dir.join("dee")).unwrap();
        std::fs::write(exe_dir.join("dee/dee_deew_test_tool"), "").unwrap();
        assert_eq!(
            dee_tool("dee_deew_test_tool", cwd, &exe_dir),
            Some(
                exe_dir
                    .join("dee/dee_deew_test_tool")
                    .to_string_lossy()
                    .into_owned()
            )
        );
        // a single executable, no library
        assert_eq!(find_shim(cwd, &exe_dir), None);
        std::fs::write(exe_dir.join("dee_shim"), "").unwrap();
        let shim = find_shim(cwd, &exe_dir).unwrap();
        assert_eq!(shim.run, exe_dir.join("dee_shim").to_string_lossy());
        std::fs::remove_dir_all(&exe_dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn shim_version_from_the_runner() {
        let dir = temp_dir("shim_version");
        let shim = Shim {
            run: dir.join("dee_shim").to_string_lossy().into_owned(),
        };
        // not runnable
        std::fs::write(dir.join("dee_shim"), "").unwrap();
        assert_eq!(shim_version(&shim), Version::NotFound);
        let script = |text: &str| {
            use std::os::unix::fs::PermissionsExt;
            std::fs::write(dir.join("dee_shim"), text).unwrap();
            std::fs::set_permissions(dir.join("dee_shim"), std::fs::Permissions::from_mode(0o755))
                .unwrap();
        };
        // a fresh script can be busy for a moment while other threads fork
        let version = || {
            (0..50)
                .map(|_| shim_version(&shim))
                .find(|v| *v != Version::NotFound)
                .unwrap()
        };
        script("#!/bin/sh\necho hello\n");
        assert_eq!(version(), Version::CouldntParse);
        script("#!/bin/sh\necho 'dee_shim v0.1.0+282-g0301b3e'\n");
        assert_eq!(version(), Version::Found("0.1.0+282-g0301b3e".into()));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
