//! The external programs (DEE tools, truehdd, dee_shim, ffmpeg, ffprobe) and
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

/// A program by its fixed name: first next to the deew executable
/// (`exe_dir`), then in the `PATH`. `None` if it does not exist.
pub fn find_program(name: &str, exe_dir: &Path, cwd: &Path) -> Option<String> {
    let local = exe_dir.join(exe_name(name));
    if local.is_file() {
        return Some(abspath(&local.to_string_lossy(), cwd));
    }
    let path = which::which(name).ok()?;
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
    which::which(tool)
        .ok()
        .map(|p| abspath(&p.to_string_lossy(), cwd))
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
    shim_at(&exe_dir.join(&run_name), cwd).or_else(|| shim_at(&which::which(&run_name).ok()?, cwd))
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
    Regex::new(r"Dolby Encoding Engine version (\d+\.\d+\.\d+)").expect("valid regex")
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

/// `0.1.0+282-g0301b3e` from `dee_shim v0.1.0+282-g0301b3e`.
pub fn parse_shim_version(output: &str) -> Option<String> {
    let mut words = output.lines().next()?.split_whitespace();
    (words.next()? == "dee_shim").then_some(())?;
    let version = words.next()?.strip_prefix('v')?;
    (!version.is_empty()).then(|| version.to_string())
}

/// Version of the DEE tools, read from the `--help` of one of them.
pub fn dee_version(tool: &str) -> Version {
    Version::from_output(output_of(tool, &["--help"]), parse_dee_version)
}

pub fn truehdd_version(truehdd: &str) -> Version {
    Version::from_output(output_of(truehdd, &["--version"]), parse_truehdd_version)
}

pub fn shim_version(shim: &Shim) -> Version {
    Version::from_output(output_of(&shim.run, &["--version"]), parse_shim_version)
}

/// Versions of the programs, in the order of the summary.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Versions {
    pub dee: Version,
    pub truehdd: Version,
    pub shim: Version,
    pub ffmpeg: Version,
    pub ffprobe: Version,
}

/// Run the five version probes at the same time. `dee` is one of the DEE
/// tools, `truehdd` its binary, each `None` if it was not found.
pub fn probe_versions(
    dee: Option<&str>,
    truehdd: Option<&str>,
    shim: Option<&Shim>,
    ffmpeg: &str,
    ffprobe: &str,
) -> Versions {
    std::thread::scope(|scope| {
        let dee = scope.spawn(|| dee.map_or(Version::NotFound, dee_version));
        let truehdd = scope.spawn(|| truehdd.map_or(Version::NotFound, truehdd_version));
        let shim = scope.spawn(|| shim.map_or(Version::NotFound, shim_version));
        let ffmpeg = scope.spawn(|| version_of(ffmpeg, &["-version"]));
        let ffprobe = scope.spawn(|| version_of(ffprobe, &["-version"]));
        let get = |h: std::thread::ScopedJoinHandle<Version>| h.join().unwrap_or_default();
        Versions {
            dee: get(dee),
            truehdd: get(truehdd),
            shim: get(shim),
            ffmpeg: get(ffmpeg),
            ffprobe: get(ffprobe),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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
            "/nonexistent/ffmpeg",
            "/nonexistent/ffprobe",
        );
        assert_eq!(v, Versions::default());
        let v = probe_versions(
            Some("/nonexistent/dee"),
            Some("/nonexistent/t"),
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
