//! Input files: the expansion of folders and the report of the files that
//! can't be encoded.

use crate::error::escape;
use crate::probe::{ProbeError, TrackIndex, track_range};
use std::path::Path;
use unicode_width::UnicodeWidthStr;

/// One input file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Input {
    /// What is given to the tools.
    pub path: String,
    /// What lists show: the name in its folder, or the path as given.
    pub name: String,
    /// The folder (as given) the file comes from, `None` for a direct input.
    pub folder: Option<String>,
    /// Known to be unusable without probing it.
    pub bad_name: bool,
}

/// Regular, non-hidden files of a directory (like `glob(dir/*)`), by name.
fn list_directory(dir: &str) -> Vec<Input> {
    let mut names: Vec<std::ffi::OsString> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| entry.path().is_file())
        .map(|entry| entry.file_name())
        .filter(|name| !name.to_string_lossy().starts_with('.'))
        .collect();
    names.sort();
    // the `.atmos.audio` and `.atmos.metadata` of a DAMF belong to its `.atmos`
    let listed: Vec<String> = names
        .iter()
        .map(|n| n.to_string_lossy().into_owned())
        .collect();
    names
        .into_iter()
        .filter(|name| !crate::master::is_damf_part(&name.to_string_lossy(), &listed))
        .map(|name| {
            let name_text = name.to_string_lossy().into_owned();
            Input {
                path: format!("{dir}{}{name_text}", std::path::MAIN_SEPARATOR),
                bad_name: name.to_str().is_none(),
                name: name_text,
                folder: Some(dir.to_string()),
            }
        })
        .collect()
}

/// Expands a folder into its files; anything else is one direct input.
pub fn expand(input: &str) -> Vec<Input> {
    if Path::new(input).is_dir() {
        list_directory(input)
    } else {
        vec![Input {
            path: input.to_string(),
            name: input.to_string(),
            folder: None,
            bad_name: false,
        }]
    }
}

/// An input that doesn't exist and has a wildcard (`*` or `?`): no shell
/// expands it on Windows, so deew does.
pub fn is_pattern(input: &str) -> bool {
    input.contains(['*', '?']) && !Path::new(input).exists()
}

/// The pattern for the `glob` crate: only `*`, `?` and `**` are special,
/// the brackets of `[Group]` are literal.
fn glob_pattern(input: &str) -> String {
    let mut out = String::with_capacity(input.len() + 8);
    for c in input.chars() {
        match c {
            '[' => out.push_str("[[]"),
            ']' => out.push_str("[]]"),
            c => out.push(c),
        }
    }
    out
}

/// Expands a pattern into the regular, non-hidden files it matches, sorted.
/// A DAMF's `.atmos.audio` and `.atmos.metadata` are left out like in a
/// folder listing. No match is an error (the message).
pub fn expand_pattern(pattern: &str) -> Result<Vec<Input>, String> {
    let no_match = || format!("no files match '{pattern}'");
    let options = glob::MatchOptions {
        case_sensitive: !cfg!(windows),
        require_literal_separator: true,
        // not `true`: glob panics on a name that isn't UTF-8 then, hidden
        // files and folders are filtered below
        require_literal_leading_dot: false,
    };
    // the folders before the first wildcard are given, not matched
    let fixed = Path::new(pattern)
        .components()
        .take_while(|c| !c.as_os_str().to_string_lossy().contains(['*', '?']))
        .count();
    let paths = glob::glob_with(&glob_pattern(pattern), options).map_err(|_| no_match())?;
    let mut files: Vec<std::path::PathBuf> = paths
        .flatten()
        .filter(|path| path.is_file())
        .filter(|path| {
            path.components()
                .skip(fixed)
                .all(|c| !c.as_os_str().to_string_lossy().starts_with('.'))
        })
        .collect();
    files.sort();
    let mut dir_names: std::collections::HashMap<std::path::PathBuf, Vec<String>> =
        std::collections::HashMap::new();
    let mut list = Vec::new();
    for path in files {
        let Some(name) = path.file_name() else {
            continue;
        };
        let name_text = name.to_string_lossy().into_owned();
        let folder = path.parent().unwrap_or_else(|| Path::new("")).to_path_buf();
        if name_text.to_lowercase().contains(".atmos.") {
            let siblings = dir_names.entry(folder.clone()).or_insert_with(|| {
                let dir = if folder.as_os_str().is_empty() {
                    Path::new(".")
                } else {
                    &folder
                };
                std::fs::read_dir(dir)
                    .into_iter()
                    .flatten()
                    .flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect()
            });
            if crate::master::is_damf_part(&name_text, siblings) {
                continue;
            }
        }
        let folder_text = if folder.as_os_str().is_empty() {
            ".".to_string()
        } else {
            folder.to_string_lossy().into_owned()
        };
        list.push(Input {
            bad_name: name.to_str().is_none(),
            path: path.to_string_lossy().into_owned(),
            name: name_text,
            folder: Some(folder_text),
        });
    }
    if list.is_empty() {
        return Err(no_match());
    }
    Ok(list)
}

/// The path with `.`, `..` and links resolved, to tell if two inputs are
/// the same file.
fn normalized(path: &str) -> std::path::PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| std::path::PathBuf::from(path))
}

/// Leaves the first of the inputs that are the same file.
pub fn dedup(list: Vec<Input>) -> Vec<Input> {
    let mut seen = std::collections::HashSet::new();
    list.into_iter()
        .filter(|input| seen.insert(normalized(&input.path)))
        .collect()
}

/// A file that can't be encoded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    pub name: String,
    pub folder: Option<String>,
    pub error: ProbeError,
}

impl Problem {
    /// A file of a folder that isn't audio: it can be left out.
    pub fn skippable(&self) -> bool {
        self.folder.is_some() && self.error.skippable()
    }
}

fn plural(count: usize, one: &str, many: &str) -> String {
    if count == 1 {
        format!("{count} {one}")
    } else {
        format!("{count} {many}")
    }
}

/// Indented rows `name  reason` with the reasons aligned.
fn rows(items: &[(String, String)], indent: usize) -> String {
    let width = items.iter().map(|(n, _)| n.width()).max().unwrap_or(0) + 2;
    items
        .iter()
        .map(|(name, reason)| {
            let pad = " ".repeat(width - name.width());
            format!(
                "{}{}{pad}{}",
                " ".repeat(indent),
                escape(name),
                escape(reason)
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Where the file is: the folder as given joined with the name (no doubled
/// separator), or the path as given for a direct input.
fn located_name(p: &Problem) -> String {
    match &p.folder {
        Some(folder) => format!(
            "{}{}{}",
            folder.trim_end_matches(['/', '\\']),
            std::path::MAIN_SEPARATOR,
            p.name
        ),
        None => p.name.clone(),
    }
}

fn problem_rows(problems: &[&Problem], indent: usize) -> String {
    let items: Vec<(String, String)> = problems
        .iter()
        .map(|p| (p.name.clone(), p.error.message()))
        .collect();
    rows(&items, indent)
}

/// The text after `ERROR: ` for these (non-skippable) errors.
pub fn errors_text(errors: &[&Problem], track_index: TrackIndex) -> String {
    if let [only] = errors {
        let name = format!("[bold yellow]{}[/bold yellow]", escape(&located_name(only)));
        return match &only.error {
            ProbeError::NoAudio => format!("{name} has no audio track."),
            ProbeError::MissingTrack { index, count } => format!(
                "{name} has no audio track {index} (-ti), it has {}.",
                track_range(*count)
            ),
            ProbeError::Unreadable => format!("{name}: {}.", only.error.message()),
            other => format!("{name}: {}", escape(&other.message())),
        };
    }
    let count = errors.len();
    let all_missing = errors
        .iter()
        .all(|p| matches!(p.error, ProbeError::MissingTrack { .. }));
    if all_missing {
        let items: Vec<(String, String)> = errors
            .iter()
            .map(|p| match p.error {
                ProbeError::MissingTrack { count, .. } => (
                    located_name(p),
                    format!(
                        "{} {}",
                        plural(count, "audio track", "audio tracks"),
                        track_range(count).split_once(' ').map_or("", |(_, r)| r)
                    ),
                ),
                _ => unreachable!(),
            })
            .collect();
        return format!(
            "{count} files have no audio track {} (-ti {track_index}):\n{}",
            track_index.n,
            rows(&items, 2)
        );
    }
    format!("{count} files can't be encoded:\n{}", located_rows(errors))
}

fn located_rows(errors: &[&Problem]) -> String {
    let items: Vec<(String, String)> = errors
        .iter()
        .map(|p| (located_name(p), p.error.message()))
        .collect();
    rows(&items, 2)
}

/// The folders of the problems, in order of appearance.
fn folders<'a>(problems: &[&'a Problem]) -> Vec<&'a str> {
    let mut list: Vec<&str> = Vec::new();
    for folder in problems.iter().filter_map(|p| p.folder.as_deref()) {
        if !list.contains(&folder) {
            list.push(folder);
        }
    }
    list
}

/// One block per folder listing the skipped files.
pub fn skip_blocks(skipped: &[&Problem]) -> Vec<String> {
    folders(skipped)
        .into_iter()
        .map(|folder| {
            let here: Vec<&Problem> = skipped
                .iter()
                .copied()
                .filter(|p| p.folder.as_deref() == Some(folder))
                .collect();
            let count = plural(here.len(), "file", "files");
            format!(
                "[bold yellow]{count}[/bold yellow] in {} can't be encoded:\n{}",
                escape(folder),
                problem_rows(&here, 2)
            )
        })
        .collect()
}

/// The text after `ERROR: ` when no file is left.
pub fn none_left_text(skipped: &[&Problem]) -> String {
    let list = folders(skipped);
    if let [folder] = list.as_slice() {
        return format!(
            "no file in [bold yellow]{}[/bold yellow] can be encoded:\n{}",
            escape(folder),
            problem_rows(skipped, 2)
        );
    }
    let groups: Vec<String> = list
        .into_iter()
        .map(|folder| {
            let here: Vec<&Problem> = skipped
                .iter()
                .copied()
                .filter(|p| p.folder.as_deref() == Some(folder))
                .collect();
            format!("  {}\n{}", escape(folder), problem_rows(&here, 4))
        })
        .collect();
    format!("no input file can be encoded:\n{}", groups.join("\n"))
}

/// What is asked before leaving the files out, and what is said with `-np`.
pub fn skip_question(skipped: usize, remaining: usize) -> (String, String) {
    let them = if skipped == 1 { "it" } else { "them" };
    let rest = if remaining == 1 {
        "the other file".to_string()
    } else {
        format!("the other {remaining} files")
    };
    (
        format!("Skip {them} and encode {rest}?"),
        format!("Skipping {them}, encoding {rest}."),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::markup::strip;

    fn problem(name: &str, folder: Option<&str>, error: ProbeError) -> Problem {
        Problem {
            name: name.into(),
            folder: folder.map(String::from),
            error,
        }
    }

    fn ffprobe() -> ProbeError {
        ProbeError::Ffprobe("Invalid data found when processing input".into())
    }

    #[test]
    fn skip_blocks_align_and_pluralize() {
        let a = problem("info.nfo", Some("Season 1"), ProbeError::NoAudio);
        let b = problem("cover.jpg", Some("Season 1"), ProbeError::NoAudio);
        let c = problem("extras.zip", Some("Season 1"), ffprobe());
        let blocks = skip_blocks(&[&a, &b, &c]);
        assert_eq!(
            strip(&blocks[0]),
            "3 files in Season 1 can't be encoded:\n  \
             info.nfo    no audio track\n  \
             cover.jpg   no audio track\n  \
             extras.zip  ffprobe: Invalid data found when processing input"
        );
        let blocks = skip_blocks(&[&a]);
        assert_eq!(
            strip(&blocks[0]),
            "1 file in Season 1 can't be encoded:\n  info.nfo  no audio track"
        );
    }

    #[test]
    fn wide_names_are_aligned_by_display_width() {
        let a = problem("音声.nfo", Some("d"), ProbeError::NoAudio);
        let b = problem("a.nfo", Some("d"), ProbeError::NoAudio);
        let text = strip(&skip_blocks(&[&a, &b])[0]);
        // 音声.nfo is 8 columns wide
        assert!(text.contains("\n  音声.nfo  no audio track"), "{text}");
        assert!(text.contains("\n  a.nfo     no audio track"), "{text}");
    }

    #[test]
    fn one_block_per_folder() {
        let a = problem("a", Some("X"), ProbeError::NoAudio);
        let b = problem("b", Some("Y"), ProbeError::NoAudio);
        let c = problem("c", Some("X"), ProbeError::NoAudio);
        let blocks = skip_blocks(&[&a, &b, &c]);
        assert_eq!(blocks.len(), 2);
        assert!(strip(&blocks[0]).starts_with("2 files in X "));
        assert!(strip(&blocks[1]).starts_with("1 file in Y "));
    }

    #[test]
    fn nothing_left() {
        let a = problem("info.nfo", Some("S 1"), ProbeError::NoAudio);
        let b = problem("extras.zip", Some("S 1"), ffprobe());
        assert_eq!(
            strip(&none_left_text(&[&a, &b])),
            "no file in S 1 can be encoded:\n  \
             info.nfo    no audio track\n  \
             extras.zip  ffprobe: Invalid data found when processing input"
        );
        let c = problem("x.txt", Some("S 2"), ProbeError::NoAudio);
        assert_eq!(
            strip(&none_left_text(&[&a, &c])),
            "no input file can be encoded:\n  S 1\n    info.nfo  no audio track\n  S 2\n    x.txt  no audio track"
        );
    }

    #[test]
    fn one_error_is_a_sentence() {
        let text = |p: Problem| strip(&errors_text(&[&p], 2.into()));
        assert_eq!(
            text(problem("info.nfo", None, ProbeError::NoAudio)),
            "info.nfo has no audio track."
        );
        assert_eq!(
            text(problem(
                "a.wav",
                None,
                ProbeError::MissingTrack { index: 3, count: 1 }
            )),
            "a.wav has no audio track 3 (-ti), it has 1 (0)."
        );
        assert_eq!(
            text(problem(
                "a.mkv",
                None,
                ProbeError::MissingTrack { index: 3, count: 2 }
            )),
            "a.mkv has no audio track 3 (-ti), it has 2 (0-1)."
        );
        assert_eq!(
            text(problem(
                "noperm.wav",
                None,
                ProbeError::Ffprobe("Permission denied".into())
            )),
            "noperm.wav: ffprobe: Permission denied"
        );
        assert_eq!(
            text(problem("a", None, ProbeError::Unreadable)),
            "a: ffprobe couldn't read the audio track, try to remux it with ffmpeg or mkvmerge."
        );
        assert_eq!(
            text(problem("a", None, ProbeError::Spawn("boom".into()))),
            "a: failed to run ffprobe: boom"
        );
    }

    #[test]
    fn several_errors() {
        let a = problem(
            "S01E03.mkv",
            None,
            ProbeError::MissingTrack { index: 2, count: 2 },
        );
        let b = problem(
            "S01E07.mkv",
            None,
            ProbeError::MissingTrack { index: 2, count: 1 },
        );
        assert_eq!(
            strip(&errors_text(&[&a, &b], 2.into())),
            "2 files have no audio track 2 (-ti 2):\n  \
             S01E03.mkv  2 audio tracks (0-1)\n  \
             S01E07.mkv  1 audio track (0)"
        );
        let c = problem("info.nfo", None, ProbeError::NoAudio);
        assert_eq!(
            strip(&errors_text(&[&a, &c], 2.into())),
            "2 files can't be encoded:\n  \
             S01E03.mkv  no audio track 2 (-ti), it has 2 (0-1)\n  \
             info.nfo    no audio track"
        );
    }

    #[test]
    fn folder_files_in_errors_show_their_folder() {
        let sep = std::path::MAIN_SEPARATOR;
        let missing = ProbeError::MissingTrack { index: 1, count: 1 };
        let a = problem("a.wav", Some("mixed"), missing.clone());
        let b = problem("s51.wav", Some("dir71/"), missing.clone());
        let c = problem("d.wav", None, missing);
        assert_eq!(
            strip(&errors_text(&[&a], 1.into())),
            format!("mixed{sep}a.wav has no audio track 1 (-ti), it has 1 (0).")
        );
        assert_eq!(
            strip(&errors_text(&[&a, &b, &c], 1.into())),
            format!(
                "3 files have no audio track 1 (-ti 1):\n  mixed{sep}a.wav    1 audio track (0)\n  dir71{sep}s51.wav  1 audio track (0)\n  d.wav          1 audio track (0)"
            )
        );
    }

    #[test]
    fn names_are_escaped() {
        let p = problem("[bold]x", None, ProbeError::NoAudio);
        assert_eq!(
            strip(&errors_text(&[&p], 0.into())),
            "[bold]x has no audio track."
        );
    }

    fn touch(dir: &Path, name: &str) {
        let path = dir.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, b"x").unwrap();
    }

    fn names(list: &[Input]) -> Vec<String> {
        list.iter().map(|i| i.name.clone()).collect()
    }

    #[test]
    fn pattern_detection() {
        assert!(is_pattern("/no/such/dir/*.mkv"));
        assert!(is_pattern("a?.mkv"));
        assert!(!is_pattern("/no/such/a.mkv"));
        assert!(!is_pattern("/no/such/[a].mkv"));
        let dir = tempfile::tempdir().unwrap();
        // an existing path wins
        assert!(!is_pattern(dir.path().to_str().unwrap()));
        // `*` isn't allowed in a Windows file name
        if !cfg!(windows) {
            touch(dir.path(), "a*b.mkv");
            assert!(!is_pattern(dir.path().join("a*b.mkv").to_str().unwrap()));
        }
    }

    #[test]
    fn glob_matches_sorted_regular_visible_files() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        // NTFS is case-insensitive: `A.MKV` would be `a.mkv`, so there it is
        // `C.MKV`, which `*.mkv` matches (sorted by code unit: upper case first)
        let upper = if cfg!(windows) { "C.MKV" } else { "A.MKV" };
        for name in ["b.mkv", "a.mkv", "c.mp4", ".hidden.mkv", upper] {
            touch(d, name);
        }
        std::fs::create_dir(d.join("dir.mkv")).unwrap();
        let pattern = format!("{}/*.mkv", d.display());
        let found = expand_pattern(&pattern).unwrap();
        let expected: &[&str] = if cfg!(windows) {
            &["C.MKV", "a.mkv", "b.mkv"]
        } else {
            &["a.mkv", "b.mkv"]
        };
        assert_eq!(names(&found), expected);
        assert!(
            found
                .iter()
                .all(|i| i.folder.as_deref() == Some(d.to_str().unwrap()))
        );
        assert!(found.iter().all(|i| !i.bad_name));
        let question = expand_pattern(&format!("{}/?.mkv", d.display())).unwrap();
        assert_eq!(question.len(), expected.len());
        let upper = expand_pattern(&format!("{}/*.MKV", d.display())).unwrap();
        if cfg!(windows) {
            assert_eq!(names(&upper), expected);
        } else {
            assert_eq!(names(&upper), ["A.MKV"]);
        }
    }

    #[test]
    fn glob_brackets_are_literal() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        for name in [
            "[Group] a [1080p].mkv",
            "[Group] b [1080p].mkv",
            "G a 1.mkv",
            "1.mkv",
        ] {
            touch(d, name);
        }
        let found = expand_pattern(&format!("{}/[Group] * [1080p].mkv", d.display())).unwrap();
        assert_eq!(
            names(&found),
            ["[Group] a [1080p].mkv", "[Group] b [1080p].mkv"]
        );
        // a bracket expression would have matched `1.mkv`
        assert!(expand_pattern(&format!("{}/[1].mkv", d.display())).is_err());
        assert_eq!(
            expand_pattern(&format!("{}/?.mkv", d.display()))
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn glob_recursive() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        for name in ["a.mkv", "s1/b.mkv", "s1/deep/c.mkv", "s1/d.mp4"] {
            touch(d, name);
        }
        let found = expand_pattern(&format!("{}/**/*.mkv", d.display())).unwrap();
        assert_eq!(names(&found), ["a.mkv", "b.mkv", "c.mkv"]);
        let folders: Vec<_> = found.iter().map(|i| i.folder.clone().unwrap()).collect();
        assert_eq!(folders[0], d.to_str().unwrap());
        assert!(folders[2].ends_with("deep"));
        // `*` stays in its folder
        let flat = expand_pattern(&format!("{}/*/*.mkv", d.display())).unwrap();
        assert_eq!(names(&flat), ["b.mkv"]);
    }

    #[test]
    fn glob_without_match_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        touch(dir.path(), "a.mp4");
        let pattern = format!("{}/*.mkv", dir.path().display());
        assert_eq!(
            expand_pattern(&pattern),
            Err(format!("no files match '{pattern}'"))
        );
        // only hidden files and folders match
        touch(dir.path(), ".h.mkv");
        touch(dir.path(), ".hd/x.mkv");
        std::fs::create_dir(dir.path().join("f.mkv")).unwrap();
        assert!(expand_pattern(&pattern).is_err());
        assert!(expand_pattern(&format!("{}/**/*.mkv", dir.path().display())).is_err());
    }

    #[test]
    fn glob_skips_damf_parts() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        for name in [
            "x.atmos",
            "x.atmos.audio",
            "x.atmos.metadata",
            "y.atmos.audio",
        ] {
            touch(d, name);
        }
        let found = expand_pattern(&format!("{}/*", d.display())).unwrap();
        assert_eq!(names(&found), ["x.atmos", "y.atmos.audio"]);
    }

    #[test]
    fn duplicates_are_left_out() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        touch(d, "a.mkv");
        touch(d, "b.mkv");
        let a = d.join("a.mkv").to_string_lossy().into_owned();
        let mut list = expand(&a);
        list.extend(expand_pattern(&format!("{}/*.mkv", d.display())).unwrap());
        list.extend(expand(&format!("{}/./a.mkv", d.display())));
        list.extend(expand_pattern(&format!("{}/a*", d.display())).unwrap());
        assert_eq!(names(&dedup(list)), [a, "b.mkv".to_string()]);
    }

    #[cfg(unix)]
    #[test]
    fn existing_file_with_a_star_is_a_plain_input() {
        let dir = tempfile::tempdir().unwrap();
        touch(dir.path(), "a*b.mkv");
        touch(dir.path(), "axxb.mkv");
        let path = dir.path().join("a*b.mkv").to_string_lossy().into_owned();
        assert!(!is_pattern(&path));
        let list = expand(&path);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].path, path);
        assert_eq!(list[0].folder, None);
    }

    #[cfg(unix)]
    #[test]
    fn glob_survives_non_utf8_names() {
        use std::os::unix::ffi::OsStrExt;
        let dir = tempfile::tempdir().unwrap();
        let name = std::ffi::OsStr::from_bytes(b"bad\xff.mkv");
        std::fs::write(dir.path().join(name), b"x").unwrap();
        touch(dir.path(), "ok.mkv");
        // glob never matches such a name (and must not panic on it)
        let found = expand_pattern(&format!("{}/*.mkv", dir.path().display())).unwrap();
        assert_eq!(names(&found), ["ok.mkv"]);
    }

    #[test]
    fn skip_question_wording() {
        assert_eq!(
            skip_question(3, 12),
            (
                "Skip them and encode the other 12 files?".into(),
                "Skipping them, encoding the other 12 files.".into()
            )
        );
        assert_eq!(
            skip_question(1, 1),
            (
                "Skip it and encode the other file?".into(),
                "Skipping it, encoding the other file.".into()
            )
        );
    }
}
