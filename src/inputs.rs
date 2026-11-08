//! Input files: the expansion of folders and the report of the files that
//! can't be encoded.

use crate::error::escape;
use crate::probe::{ProbeError, track_range};
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
    names
        .into_iter()
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
pub fn errors_text(errors: &[&Problem], track_index: i64) -> String {
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
            "{count} files have no audio track {track_index} (-ti {track_index}):\n{}",
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
        let text = |p: Problem| strip(&errors_text(&[&p], 2));
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
            strip(&errors_text(&[&a, &b], 2)),
            "2 files have no audio track 2 (-ti 2):\n  \
             S01E03.mkv  2 audio tracks (0-1)\n  \
             S01E07.mkv  1 audio track (0)"
        );
        let c = problem("info.nfo", None, ProbeError::NoAudio);
        assert_eq!(
            strip(&errors_text(&[&a, &c], 2)),
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
            strip(&errors_text(&[&a], 1)),
            format!("mixed{sep}a.wav has no audio track 1 (-ti), it has 1 (0).")
        );
        assert_eq!(
            strip(&errors_text(&[&a, &b, &c], 1)),
            format!(
                "3 files have no audio track 1 (-ti 1):\n  mixed{sep}a.wav    1 audio track (0)\n  dir71{sep}s51.wav  1 audio track (0)\n  d.wav          1 audio track (0)"
            )
        );
    }

    #[test]
    fn names_are_escaped() {
        let p = problem("[bold]x", None, ProbeError::NoAudio);
        assert_eq!(strip(&errors_text(&[&p], 0)), "[bold]x has no audio track.");
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
