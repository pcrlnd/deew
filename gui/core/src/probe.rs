//! Probing of the input files with ffprobe, mirroring the deew CLI
//! (`src/probe.rs`, `src/pipeline.rs` and `src/config.rs` of the root crate).

use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Result for one input file. Either the three properties or `error`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileProbe {
    pub path: String,
    pub channels: Option<i64>,
    pub sample_rate: Option<i64>,
    /// Raw bit depth, before the CLI's normalisation to 16/24/32.
    pub bit_depth: Option<i64>,
    /// ffprobe's `codec_name` of the stream (e.g. `truehd`).
    pub codec: Option<String>,
    /// ffprobe's stream `profile` mentions Dolby Atmos.
    pub atmos: bool,
    pub error: Option<String>,
    /// A file of a folder that isn't audio: left out, not an error.
    pub skipped: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeResult {
    /// The resolved ffprobe executable.
    pub ffprobe: String,
    pub files: Vec<FileProbe>,
}

/// The properties of the selected audio stream that the GUI needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamInfo {
    pub sample_rate: i64,
    pub channels: i64,
    pub bit_depth: i64,
    pub codec: Option<String>,
    pub atmos: bool,
}

/// An input file after expanding folders.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputFile {
    pub path: String,
    /// Came from expanding a folder (a failure to probe it skips the file).
    pub from_folder: bool,
    /// Known to be unusable without probing it.
    pub error: Option<String>,
}

const NON_UTF8_NAME: &str = "the file name isn't valid UTF-8";

/// Regular, non-hidden files of a directory (like `glob(dir/*)`), by name.
fn list_directory(dir: &str) -> Vec<InputFile> {
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
            let error = name.to_str().is_none().then(|| NON_UTF8_NAME.to_string());
            InputFile {
                path: format!(
                    "{dir}{}{}",
                    std::path::MAIN_SEPARATOR,
                    name.to_string_lossy()
                ),
                from_folder: true,
                error,
            }
        })
        .collect()
}

/// Expands directories into their files exactly like the CLI does; other
/// inputs are kept as they are (a missing one is reported when probing).
pub fn expand_inputs(inputs: &[String]) -> Vec<InputFile> {
    let mut list = Vec::new();
    for input in inputs {
        if Path::new(input).is_dir() {
            list.extend(list_directory(input));
        } else {
            list.push(InputFile {
                path: input.clone(),
                from_folder: false,
                error: None,
            });
        }
    }
    list
}

/// The ffprobe command line, the same as the CLI's.
pub fn probe_args(file: &str) -> Vec<String> {
    [
        "-v",
        "error",
        "-print_format",
        "json",
        "-show_format",
        "-show_streams",
        file,
    ]
    .map(String::from)
    .to_vec()
}

/// ffprobe prints some numbers as JSON numbers and some as strings.
fn as_i64(v: &Value) -> Option<i64> {
    match v {
        Value::Number(n) => n.as_i64().or_else(|| n.as_f64().map(|f| f as i64)),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

const NO_AUDIO_TRACK: &str = "no audio track";
const UNREADABLE: &str =
    "ffprobe couldn't read the audio track, try to remux it with ffmpeg or mkvmerge";

/// Why the JSON of ffprobe gave no usable audio track.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeError {
    /// No audio stream at all.
    NoAudio,
    /// Only `count` audio streams, fewer than the track index + 1.
    MissingTrack { index: u32, count: usize },
    /// Not JSON, or the chosen stream lacks the sample rate / channels.
    Unreadable,
}

impl ProbeError {
    /// The message, the same as the CLI's.
    pub fn message(&self) -> String {
        match self {
            Self::NoAudio => NO_AUDIO_TRACK.into(),
            Self::MissingTrack { index, count } => {
                let has = if *count == 1 {
                    "1 (0)".to_string()
                } else {
                    format!("{count} (0-{})", count - 1)
                };
                format!("no audio track {index} (-ti), it has {has}")
            }
            Self::Unreadable => UNREADABLE.into(),
        }
    }

    /// A file of a folder with this error is skipped, not an error.
    fn skippable(&self) -> bool {
        matches!(self, Self::NoAudio)
    }
}

/// Parses the JSON printed by ffprobe like the CLI does: the `track_index`th
/// audio stream (0-based) of all the streams.
pub fn parse_probe(json: &str, track_index: u32) -> Result<StreamInfo, ProbeError> {
    let value: Value = serde_json::from_str(json).map_err(|_| ProbeError::Unreadable)?;
    let streams = value
        .get("streams")
        .and_then(Value::as_array)
        .ok_or(ProbeError::Unreadable)?;
    let audios: Vec<&Value> = streams
        .iter()
        .filter(|s| s.get("codec_type").and_then(Value::as_str) == Some("audio"))
        .collect();
    if audios.is_empty() {
        return Err(ProbeError::NoAudio);
    }
    let Some(audio) = audios.get(track_index as usize) else {
        return Err(ProbeError::MissingTrack {
            index: track_index,
            count: audios.len(),
        });
    };
    let sample_rate = audio.get("sample_rate").and_then(as_i64);
    let channels = audio.get("channels").and_then(as_i64);
    let (Some(sample_rate), Some(channels)) = (sample_rate, channels) else {
        return Err(ProbeError::Unreadable);
    };
    let mut bit_depth = audio.get("bits_per_sample").and_then(as_i64).unwrap_or(0);
    if bit_depth == 0 {
        bit_depth = audio
            .get("bits_per_raw_sample")
            .and_then(as_i64)
            .unwrap_or(32);
    }
    let codec = audio
        .get("codec_name")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let atmos = audio
        .get("profile")
        .and_then(|v| v.as_str())
        .is_some_and(|p| p.to_lowercase().contains("atmos"));
    Ok(StreamInfo {
        sample_rate,
        channels,
        bit_depth,
        codec,
        atmos,
    })
}

/// The reason of a failed ffprobe: the last non-empty stderr line without a
/// leading `[... @ 0x...] ` and `<file>: `.
fn stderr_reason(stderr: &str, file: &str, code: Option<i32>) -> String {
    let Some(line) = stderr.lines().map(str::trim).rfind(|l| !l.is_empty()) else {
        return format!("failed (exit code {})", code.unwrap_or(-1));
    };
    let mut reason = line;
    if reason.starts_with('[')
        && let Some(end) = reason.find("] ")
        && reason[..end].contains(" @ 0x")
    {
        reason = reason[end + 2..].trim_start();
    }
    reason = reason
        .strip_prefix(file)
        .and_then(|r| r.strip_prefix(": "))
        .unwrap_or(reason);
    reason.to_string()
}

/// A failed probe; `skip` for a file of a folder that is just not audio.
struct Failure {
    message: String,
    skip: bool,
}

impl Failure {
    fn error(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            skip: false,
        }
    }
}

/// Runs ffprobe on `file`.
fn run_ffprobe(
    ffprobe: &Path,
    track_index: u32,
    file: &str,
    from_folder: bool,
) -> Result<StreamInfo, Failure> {
    if !Path::new(file).exists() {
        return Err(Failure::error("path does not exist"));
    }
    let mut command = Command::new(ffprobe);
    command
        .args(probe_args(file))
        .stdin(Stdio::null())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // The release GUI has no console; do not flash one for ffprobe.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let output = command
        .output()
        .map_err(|e| Failure::error(format!("failed to run ffprobe: {e}")))?;
    if !output.status.success() {
        let reason = stderr_reason(
            &String::from_utf8_lossy(&output.stderr),
            file,
            output.status.code(),
        );
        return Err(Failure {
            message: format!("ffprobe: {reason}"),
            skip: from_folder,
        });
    }
    parse_probe(&String::from_utf8_lossy(&output.stdout), track_index).map_err(|e| Failure {
        message: e.message(),
        skip: from_folder && e.skippable(),
    })
}

/// The only config location, like the CLI: `config.toml` next to the CLI
/// executable. `None` without a CLI.
pub fn config_path(cli_dir: Option<&Path>) -> Option<PathBuf> {
    cli_dir.map(|dir| dir.join("config.toml"))
}

/// The `[default_bitrates]` table of a config text as key -> kbps. Empty if
/// the text is not TOML or has no such table; non-integer values are skipped.
pub fn parse_default_bitrates(text: &str) -> BTreeMap<String, i64> {
    text.parse::<toml::Table>()
        .ok()
        .and_then(|table| match table.get("default_bitrates") {
            Some(toml::Value::Table(t)) => Some(t.clone()),
            _ => None,
        })
        .map(|t| {
            t.into_iter()
                .filter_map(|(key, value)| value.as_integer().map(|n| (key, n)))
                .collect()
        })
        .unwrap_or_default()
}

/// The default bitrates of the config file (empty when there is none).
pub fn default_bitrates(config: Option<&Path>) -> BTreeMap<String, i64> {
    config
        .and_then(|path| std::fs::read_to_string(path).ok())
        .map(|text| parse_default_bitrates(&text))
        .unwrap_or_default()
}

/// Finds ffprobe like the CLI: `ffprobe` next to the deew executable
/// (`cli_dir`) first, then on PATH.
pub fn resolve_ffprobe(cli_dir: Option<&Path>) -> Result<PathBuf, String> {
    let name = if cfg!(windows) {
        "ffprobe.exe"
    } else {
        "ffprobe"
    };
    if let Some(dir) = cli_dir {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    which::which(name)
        .map_err(|_| "ffprobe was not found, put it next to deew or in your PATH".to_string())
}

/// Probes every input (directories are expanded) with the given ffprobe.
pub fn probe_files(ffprobe: &Path, inputs: &[String], track_index: u32) -> ProbeResult {
    let files = expand_inputs(inputs)
        .into_iter()
        .map(|input| {
            let failed = |path: String, error: String, skipped: bool| FileProbe {
                path,
                channels: None,
                sample_rate: None,
                bit_depth: None,
                codec: None,
                atmos: false,
                error: Some(error),
                skipped,
            };
            if let Some(error) = input.error {
                return failed(input.path, error, true);
            }
            match run_ffprobe(ffprobe, track_index, &input.path, input.from_folder) {
                Ok(info) => FileProbe {
                    path: input.path,
                    channels: Some(info.channels),
                    sample_rate: Some(info.sample_rate),
                    bit_depth: Some(info.bit_depth),
                    codec: info.codec,
                    atmos: info.atmos,
                    error: None,
                    skipped: false,
                },
                Err(f) => failed(input.path, f.message, f.skip),
            }
        })
        .collect();
    ProbeResult {
        ffprobe: ffprobe.display().to_string(),
        files,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, text: &str) {
        std::fs::write(path, text).unwrap();
    }

    #[test]
    fn args_match_the_cli() {
        assert_eq!(
            probe_args("a b.mkv"),
            [
                "-v",
                "error",
                "-print_format",
                "json",
                "-show_format",
                "-show_streams",
                "a b.mkv"
            ]
        );
    }

    fn parse(json: &str) -> Option<StreamInfo> {
        parse_probe(json, 0).ok()
    }

    #[test]
    fn typical_ffprobe_output() {
        let json = r#"{"streams":[{"index":0,"codec_type":"audio","codec_name":"pcm_s24le","sample_rate":"48000",
            "channels":6,"bits_per_sample":24,"bits_per_raw_sample":"24","duration":"2.000000"}],
            "format":{"duration":"2.0"}}"#;
        assert_eq!(
            parse(json),
            Some(StreamInfo {
                sample_rate: 48000,
                channels: 6,
                bit_depth: 24,
                codec: Some("pcm_s24le".into()),
                atmos: false
            })
        );
    }

    #[test]
    fn atmos_is_read_from_the_profile() {
        let atmos = |profile: &str| {
            let json = format!(
                r#"{{"streams":[{{"codec_type":"audio","codec_name":"truehd","profile":"{profile}",
                "sample_rate":"48000","channels":8}}]}}"#
            );
            parse(&json).unwrap().atmos
        };
        assert!(atmos("Dolby TrueHD + Dolby Atmos"));
        assert!(atmos("Dolby Digital Plus + Dolby Atmos"));
        assert!(!atmos("Dolby Digital Plus"));
        assert!(!atmos(""));
        let json = r#"{"streams":[{"codec_type":"audio","sample_rate":"48000","channels":2}]}"#;
        assert!(!parse(json).unwrap().atmos);
    }

    #[test]
    fn numbers_in_either_form() {
        let json = r#"{"streams":[{"codec_type":"audio","sample_rate":44100,"channels":"2","bits_per_sample":"16"}]}"#;
        assert_eq!(
            parse(json),
            Some(StreamInfo {
                sample_rate: 44100,
                channels: 2,
                bit_depth: 16,
                codec: None,
                atmos: false
            })
        );
    }

    #[test]
    fn bit_depth_defaults() {
        let json = r#"{"streams":[{"codec_type":"audio","sample_rate":"48000","channels":2,"bits_per_sample":0}]}"#;
        assert_eq!(parse(json).unwrap().bit_depth, 32);
        let json = r#"{"streams":[{"codec_type":"audio","sample_rate":"48000","channels":2,
            "bits_per_sample":0,"bits_per_raw_sample":"20"}]}"#;
        assert_eq!(parse(json).unwrap().bit_depth, 20);
    }

    #[test]
    fn nth_audio_stream_is_picked() {
        let json = r#"{"streams":[
            {"codec_type":"video","codec_name":"h264","sample_rate":"1","channels":1},
            {"codec_type":"audio","codec_name":"ac3","sample_rate":"48000","channels":6},
            {"codec_type":"audio","codec_name":"aac","sample_rate":"44100","channels":2}]}"#;
        assert_eq!(parse_probe(json, 0).unwrap().codec.as_deref(), Some("ac3"));
        let second = parse_probe(json, 1).unwrap();
        assert_eq!((second.codec.as_deref(), second.channels), (Some("aac"), 2));
    }

    #[test]
    fn failure_reasons() {
        let video = r#"{"streams":[{"codec_type":"video","codec_name":"ansi"}]}"#;
        let err = parse_probe(video, 0).unwrap_err();
        assert_eq!(err.message(), "no audio track");
        assert_eq!(
            parse_probe(r#"{"streams":[]}"#, 0),
            Err(ProbeError::NoAudio)
        );
        assert_eq!(parse_probe("{}", 0), Err(ProbeError::Unreadable));

        let one = r#"{"streams":[{"codec_type":"audio","sample_rate":"1","channels":2}]}"#;
        assert_eq!(
            parse_probe(one, 2).unwrap_err().message(),
            "no audio track 2 (-ti), it has 1 (0)"
        );
        let two = r#"{"streams":[{"codec_type":"audio"},{"codec_type":"audio"}]}"#;
        assert_eq!(
            parse_probe(two, 3).unwrap_err().message(),
            "no audio track 3 (-ti), it has 2 (0-1)"
        );
        let unreadable =
            "ffprobe couldn't read the audio track, try to remux it with ffmpeg or mkvmerge";
        assert_eq!(parse_probe(two, 0).unwrap_err().message(), unreadable);
        assert_eq!(
            parse_probe("not json", 0).unwrap_err().message(),
            unreadable
        );
        assert_eq!(parse_probe("", 0).unwrap_err().message(), unreadable);
        let no_channels = r#"{"streams":[{"codec_type":"audio","sample_rate":"1"}]}"#;
        assert_eq!(
            parse_probe(no_channels, 0).unwrap_err().message(),
            unreadable
        );
    }

    #[test]
    fn stderr_reasons() {
        assert_eq!(
            stderr_reason(
                "fp/archive.zip: Invalid data found when processing input\n",
                "fp/archive.zip",
                Some(1)
            ),
            "Invalid data found when processing input"
        );
        assert_eq!(
            stderr_reason(
                "[matroska,webm @ 0x5add0de56100] EBML header parsing failed\nfp/empty.mkv: Invalid data found when processing input\n",
                "fp/empty.mkv",
                Some(1)
            ),
            "Invalid data found when processing input"
        );
        assert_eq!(
            stderr_reason(
                "fp/noperm.wav: Permission denied\n\n",
                "fp/noperm.wav",
                Some(1)
            ),
            "Permission denied"
        );
        assert_eq!(
            stderr_reason("[mov @ 0x1f] moov atom not found\n", "x.mp4", Some(1)),
            "moov atom not found"
        );
        assert_eq!(stderr_reason("", "x", Some(3)), "failed (exit code 3)");
        assert_eq!(stderr_reason(" \n", "x", Some(1)), "failed (exit code 1)");
    }

    #[test]
    fn directories_expand_to_sorted_visible_files() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("in");
        std::fs::create_dir(&dir).unwrap();
        write(&dir.join("b.mkv"), "");
        write(&dir.join("a.mkv"), "");
        write(&dir.join(".hidden.mkv"), "");
        std::fs::create_dir(dir.join("sub")).unwrap();
        let single = tmp.path().join("single.wav");
        write(&single, "");
        let missing = tmp.path().join("missing.wav");

        let sep = std::path::MAIN_SEPARATOR;
        let d = dir.display();
        let inputs = vec![
            single.display().to_string(),
            d.to_string(),
            missing.display().to_string(),
        ];
        let expanded = expand_inputs(&inputs);
        assert_eq!(
            expanded
                .iter()
                .map(|f| (f.path.clone(), f.from_folder))
                .collect::<Vec<_>>(),
            vec![
                (single.display().to_string(), false),
                (format!("{d}{sep}a.mkv"), true),
                (format!("{d}{sep}b.mkv"), true),
                (missing.display().to_string(), false),
            ]
        );
    }

    #[test]
    fn default_bitrates_are_parsed() {
        let text = "logo = 1\n[default_bitrates]\ndd_5_1 = 640\nddp_7_1_standard = 1024\nbad = 'x'\nf = 1.5\n";
        let map = parse_default_bitrates(text);
        assert_eq!(map.len(), 2);
        assert_eq!(map["dd_5_1"], 640);
        assert_eq!(map["ddp_7_1_standard"], 1024);
        assert!(parse_default_bitrates("").is_empty());
        assert!(parse_default_bitrates("not [ toml").is_empty());
        assert!(parse_default_bitrates("logo = 1\n").is_empty());
        assert!(parse_default_bitrates("default_bitrates = 3\n").is_empty());
    }

    #[test]
    fn default_bitrates_are_read_from_the_file() {
        let tmp = tempfile::tempdir().unwrap();
        let config = tmp.path().join("config.toml");
        assert!(default_bitrates(Some(&config)).is_empty());
        assert!(default_bitrates(None).is_empty());
        write(&config, "[default_bitrates]\nac4_2_0 = 256\n");
        assert_eq!(default_bitrates(Some(&config))["ac4_2_0"], 256);
    }

    fn executable(path: &Path) {
        write(path, "");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
    }

    fn exe_name(name: &str) -> String {
        if cfg!(windows) {
            format!("{name}.exe")
        } else {
            name.to_string()
        }
    }

    #[test]
    fn ffprobe_next_to_deew_wins_over_the_path() {
        let tmp = tempfile::tempdir().unwrap();
        let cli_dir = tmp.path().join("cli");
        std::fs::create_dir_all(&cli_dir).unwrap();
        let beside = cli_dir.join(exe_name("ffprobe"));
        executable(&beside);
        assert_eq!(resolve_ffprobe(Some(&cli_dir)).unwrap(), beside);
    }

    #[test]
    fn missing_ffprobe_is_explained() {
        let tmp = tempfile::tempdir().unwrap();
        // nothing next to deew: whatever is on PATH, or the error
        match resolve_ffprobe(Some(tmp.path())) {
            Ok(found) => assert!(!found.starts_with(tmp.path())),
            Err(err) => {
                assert_eq!(
                    err,
                    "ffprobe was not found, put it next to deew or in your PATH"
                )
            }
        }
    }

    #[test]
    fn config_is_next_to_the_cli() {
        assert_eq!(
            config_path(Some(Path::new("/cli"))),
            Some(Path::new("/cli").join("config.toml"))
        );
        assert_eq!(config_path(None), None);
    }

    #[test]
    fn missing_file_is_a_per_file_error() {
        let result = probe_files(
            Path::new("ffprobe-unused"),
            &["/no/such/file.mkv".into()],
            0,
        );
        assert_eq!(result.files.len(), 1);
        assert_eq!(
            result.files[0].error.as_deref(),
            Some("path does not exist")
        );
        assert_eq!(result.files[0].channels, None);
    }

    #[cfg(unix)]
    fn fake_ffprobe(dir: &Path, script: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let fake = dir.join("ffprobe");
        write(&fake, script);
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        fake
    }

    #[cfg(unix)]
    #[test]
    fn probes_with_a_fake_ffprobe() {
        let tmp = tempfile::tempdir().unwrap();
        let fake = fake_ffprobe(
            tmp.path(),
            "#!/bin/sh\necho '{\"streams\":[{\"codec_type\":\"video\"},{\"codec_type\":\"audio\",\"codec_name\":\"truehd\",\"sample_rate\":\"48000\",\"channels\":6,\"bits_per_sample\":0,\"bits_per_raw_sample\":\"24\"}]}'\n",
        );
        let media = tmp.path().join("x.mkv");
        write(&media, "");
        let inputs = [media.display().to_string()];

        let ok = probe_files(&fake, &inputs, 0);
        let file = &ok.files[0];
        assert_eq!(
            (file.channels, file.sample_rate, file.bit_depth, &file.error),
            (Some(6), Some(48000), Some(24), &None)
        );
        assert_eq!(file.codec.as_deref(), Some("truehd"));
        assert!(!file.skipped);
        let bad = probe_files(&fake, &inputs, 1);
        assert_eq!(
            bad.files[0].error.as_deref(),
            Some("no audio track 1 (-ti), it has 1 (0)")
        );
        assert!(!bad.files[0].skipped);
    }

    #[cfg(unix)]
    #[test]
    fn folder_files_that_are_not_audio_are_skipped() {
        let tmp = tempfile::tempdir().unwrap();
        // Fails for `bad*`, no audio for `nfo*`, an audio stream otherwise.
        let script = "#!/bin/sh\nfor last; do :; done\ncase \"$last\" in\n*bad*) echo \"$last: Invalid data found when processing input\" >&2; exit 1;;\n*nfo*) echo '{\"streams\":[{\"codec_type\":\"video\"}]}';;\n*broken*) echo '{\"streams\":[{\"codec_type\":\"audio\"}]}';;\n*) echo '{\"streams\":[{\"codec_type\":\"audio\",\"sample_rate\":\"48000\",\"channels\":2}]}';;\nesac\n";
        let fake = fake_ffprobe(tmp.path(), script);
        let dir = tmp.path().join("in");
        std::fs::create_dir(&dir).unwrap();
        for name in ["bad.zip", "broken.wav", "good.wav", "nfo.txt"] {
            write(&dir.join(name), "");
        }
        let direct_bad = tmp.path().join("bad-direct.zip");
        let direct_nfo = tmp.path().join("nfo-direct.txt");
        write(&direct_bad, "");
        write(&direct_nfo, "");

        let inputs = [
            dir.display().to_string(),
            direct_bad.display().to_string(),
            direct_nfo.display().to_string(),
        ];
        let files = probe_files(&fake, &inputs, 0).files;
        let summary: Vec<(String, bool, Option<&str>)> = files
            .iter()
            .map(|f| {
                let name = Path::new(&f.path).file_name().unwrap();
                (
                    name.to_string_lossy().into_owned(),
                    f.skipped,
                    f.error.as_deref(),
                )
            })
            .collect();
        let unreadable =
            "ffprobe couldn't read the audio track, try to remux it with ffmpeg or mkvmerge";
        assert_eq!(
            summary,
            vec![
                (
                    "bad.zip".into(),
                    true,
                    Some("ffprobe: Invalid data found when processing input")
                ),
                ("broken.wav".into(), false, Some(unreadable)),
                ("good.wav".into(), false, None),
                ("nfo.txt".into(), true, Some("no audio track")),
                (
                    "bad-direct.zip".into(),
                    false,
                    Some("ffprobe: Invalid data found when processing input")
                ),
                ("nfo-direct.txt".into(), false, Some("no audio track")),
            ]
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn non_utf8_names_in_a_folder_are_skipped() {
        use std::os::unix::ffi::OsStrExt;
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("in");
        std::fs::create_dir(&dir).unwrap();
        let name = std::ffi::OsStr::from_bytes(b"bad\xff.wav");
        write(&dir.join(name), "");
        write(&dir.join(".hidden"), "");
        // ffprobe must not be run on it.
        let result = probe_files(Path::new("ffprobe-unused"), &[dir.display().to_string()], 0);
        assert_eq!(result.files.len(), 1);
        let file = &result.files[0];
        assert!(file.path.ends_with("bad\u{fffd}.wav"), "{}", file.path);
        assert!(file.skipped);
        assert_eq!(
            file.error.as_deref(),
            Some("the file name isn't valid UTF-8")
        );
    }
}
