//! ffprobe: argument list, output parsing and the process call.

use crate::master::{Master, MasterKind};
use serde_json::Value;
use std::process::{Command, Stdio};

/// Which stream `-ti` selects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrackIndex {
    /// `s:N`: the stream index N of the container, not the Nth audio track.
    pub stream: bool,
    pub n: usize,
    /// Written with its `a:` / `s:` prefix (shown as given).
    pub prefixed: bool,
}

impl Default for TrackIndex {
    fn default() -> Self {
        TrackIndex::audio(0)
    }
}

impl From<i64> for TrackIndex {
    /// The Nth audio track; a negative number is the first one.
    fn from(n: i64) -> Self {
        TrackIndex::audio(usize::try_from(n).unwrap_or(0))
    }
}

impl std::fmt::Display for TrackIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match (self.stream, self.prefixed) {
            (true, _) => write!(f, "s:{}", self.n),
            (false, true) => write!(f, "a:{}", self.n),
            (false, false) => write!(f, "{}", self.n),
        }
    }
}

impl TrackIndex {
    /// The Nth audio track, written without a prefix.
    pub const fn audio(n: usize) -> Self {
        TrackIndex {
            stream: false,
            n,
            prefixed: false,
        }
    }

    /// The `-map` value.
    pub fn map_spec(&self) -> String {
        if self.stream {
            format!("0:{}", self.n)
        } else {
            format!("0:a:{}", self.n)
        }
    }

    /// The input label of a filter graph.
    pub fn filter_label(&self) -> String {
        if self.stream {
            format!("0:{}", self.n)
        } else {
            format!("a:{}", self.n)
        }
    }

    /// Parse `N`, `a:N` or `s:N` (the prefix in any case).
    pub fn parse(text: &str) -> Result<Self, TrackParseError> {
        let t = text.trim();
        let (stream, prefixed, number) = match t.split_once(':') {
            Some((prefix, rest)) => match prefix.to_ascii_lowercase().as_str() {
                "a" => (false, true, rest),
                "s" => (true, true, rest),
                _ => return Err(TrackParseError::Malformed),
            },
            None => (false, false, t),
        };
        let digits = number.strip_prefix('+').unwrap_or(number);
        let all_digits = |d: &str| !d.is_empty() && d.bytes().all(|b| b.is_ascii_digit());
        if prefixed {
            if !all_digits(number) {
                return Err(TrackParseError::Malformed);
            }
        } else if number.strip_prefix('-').is_some_and(all_digits) {
            return Err(TrackParseError::Negative);
        } else if !all_digits(digits) {
            return Err(TrackParseError::NotAnInt);
        }
        // too big for a track: saturate, nothing has that many
        let n = digits.parse::<usize>().unwrap_or(usize::MAX);
        Ok(TrackIndex {
            stream,
            n,
            prefixed,
        })
    }
}

/// Why `-ti` can't be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackParseError {
    /// A plain value that isn't an integer.
    NotAnInt,
    Negative,
    /// A prefix other than `a:` / `s:`, or no number after it.
    Malformed,
}

/// The properties of the selected audio stream that deew needs.
#[derive(Debug, Clone, PartialEq)]
pub struct Probe {
    pub sample_rate: i64,
    pub channels: i64,
    /// Seconds, -1 if ffprobe did not report it.
    pub duration: f64,
    /// Raw bit depth (before normalisation).
    pub bit_depth: i64,
    /// The codec is TrueHD (the only input of the Atmos formats).
    pub is_truehd: bool,
    /// `channel_layout` of the stream (empty if ffprobe has none), e.g. `5.0(side)`.
    pub channel_layout: String,
    /// `codec_name` of the stream (empty if ffprobe has none).
    pub codec: String,
    /// The input is a raw TrueHD stream (ffprobe format `truehd`), which
    /// truehdd reads directly.
    pub raw_truehd: bool,
    /// `profile` of the stream (empty if ffprobe has none), e.g. `Dolby TrueHD + Dolby Atmos`.
    pub profile: String,
    /// The profile mentions Atmos.
    pub is_atmos: bool,
    /// An Atmos master (ADM BWF or DAMF) that DEE reads directly, never
    /// probed with ffprobe.
    pub master: Option<MasterKind>,
}

impl Probe {
    /// What stands in for the probe of an Atmos master: DEE takes 48 kHz,
    /// and renders a 5.1 from it for DD and DDP.
    pub fn of_master(master: Master) -> Probe {
        Probe {
            sample_rate: 48000,
            channels: 6,
            duration: master.duration.unwrap_or(-1.0),
            bit_depth: 24,
            is_truehd: false,
            channel_layout: String::new(),
            codec: master.kind.label().to_string(),
            raw_truehd: false,
            profile: String::new(),
            is_atmos: true,
            master: Some(master.kind),
        }
    }
}

pub fn probe_args(ffprobe: &str, file: &str) -> Vec<String> {
    [
        ffprobe,
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

const UNREADABLE: &str =
    "ffprobe couldn't read the audio track, try to remux it with ffmpeg or mkvmerge";

/// Why a file gave no usable audio track.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeError {
    /// ffprobe failed; the reason is its last stderr line.
    Ffprobe(String),
    /// ffprobe could not be started (the io error).
    Spawn(String),
    /// No audio stream at all.
    NoAudio,
    /// Only `count` audio streams, fewer than the track index + 1.
    MissingTrack { index: usize, count: usize },
    /// `-ti s:N`: the container has only `count` streams.
    MissingStream { index: usize, count: usize },
    /// `-ti s:N`: stream N is of another type (`video`, `subtitle`, ...).
    NotAudioStream { index: usize, kind: String },
    /// Not JSON, or the chosen stream lacks the sample rate / channels.
    Unreadable,
    /// A file name that isn't valid UTF-8 (never probed).
    BadName,
    /// A DAMF whose `.atmos.audio` / `.atmos.metadata` is missing (the list of paths).
    DamfMissing(Vec<String>),
}

impl ProbeError {
    /// The reason as shown in lists.
    pub fn message(&self) -> String {
        match self {
            Self::Ffprobe(reason) => format!("ffprobe: {reason}"),
            Self::Spawn(error) => format!("failed to run ffprobe: {error}"),
            Self::NoAudio => "no audio track".into(),
            Self::MissingTrack { index, count } => {
                format!(
                    "no audio track {index} (-ti), it has {}",
                    track_range(*count)
                )
            }
            Self::MissingStream { index, count } => format!(
                "no stream {index} (-ti s:{index}), it has {count} (0-{})",
                count.saturating_sub(1)
            ),
            Self::NotAudioStream { index, kind } => {
                let article = if kind.starts_with(['a', 'e', 'i', 'o', 'u']) {
                    "an"
                } else {
                    "a"
                };
                format!("stream {index} (-ti s:{index}) is {article} {kind} stream, not audio")
            }
            Self::Unreadable => UNREADABLE.into(),
            Self::BadName => "the file name isn't valid UTF-8".into(),
            Self::DamfMissing(files) => format!(
                "the DAMF Atmos master is incomplete, it needs {}",
                files.join(", ")
            ),
        }
    }

    /// A file of a folder with this error can be left out; any other
    /// problem is an error.
    pub fn skippable(&self) -> bool {
        matches!(self, Self::NoAudio | Self::Ffprobe(_) | Self::BadName)
    }
}

/// `1 (0)` or `3 (0-2)`.
pub fn track_range(count: usize) -> String {
    if count == 1 {
        "1 (0)".into()
    } else {
        format!("{count} (0-{})", count.saturating_sub(1))
    }
}

/// ffprobe prints some numbers as JSON numbers and some as strings.
fn as_i64(v: &Value) -> Option<i64> {
    match v {
        Value::Number(n) => n.as_i64().or_else(|| n.as_f64().map(|f| f as i64)),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

fn as_f64(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

/// Parse the JSON printed by ffprobe: the `track_index`th audio stream
/// (0-based) of all the streams.
pub fn parse_probe(json: &str, track: impl Into<TrackIndex>) -> Result<Probe, ProbeError> {
    let track = track.into();
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
    let audio = if track.stream {
        // the `index` of a stream, its position when ffprobe gives none
        let found = streams
            .iter()
            .enumerate()
            .find(|(i, s)| s.get("index").and_then(as_i64).unwrap_or(*i as i64) == track.n as i64);
        let Some((_, stream)) = found else {
            return Err(ProbeError::MissingStream {
                index: track.n,
                count: streams.len(),
            });
        };
        let kind = stream.get("codec_type").and_then(Value::as_str);
        if kind != Some("audio") {
            return Err(ProbeError::NotAudioStream {
                index: track.n,
                kind: kind.unwrap_or("unknown").to_string(),
            });
        }
        stream
    } else {
        let Some(audio) = audios.get(track.n) else {
            return Err(ProbeError::MissingTrack {
                index: track.n,
                count: audios.len(),
            });
        };
        *audio
    };
    let sample_rate = audio
        .get("sample_rate")
        .and_then(as_i64)
        .ok_or(ProbeError::Unreadable)?;
    let channels = audio
        .get("channels")
        .and_then(as_i64)
        .ok_or(ProbeError::Unreadable)?;
    // Containers like mka often have the duration only in the format.
    let duration = audio
        .get("duration")
        .and_then(as_f64)
        .or_else(|| value.get("format")?.get("duration").and_then(as_f64))
        .filter(|d| d.is_finite())
        .unwrap_or(-1.0);
    let mut bit_depth = audio.get("bits_per_sample").and_then(as_i64).unwrap_or(0);
    if bit_depth == 0 {
        bit_depth = audio
            .get("bits_per_raw_sample")
            .and_then(as_i64)
            .unwrap_or(32);
    }
    let codec = audio
        .get("codec_name")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let channel_layout = audio
        .get("channel_layout")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let profile = audio
        .get("profile")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let is_atmos = profile.contains("Atmos");
    let is_truehd = codec == "truehd";
    let raw_truehd = value
        .get("format")
        .and_then(|f| f.get("format_name"))
        .and_then(Value::as_str)
        == Some("truehd");
    Ok(Probe {
        sample_rate,
        channels,
        duration,
        bit_depth,
        is_truehd,
        channel_layout,
        codec,
        raw_truehd,
        profile,
        is_atmos,
        master: None,
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

/// Run ffprobe on `file`.
pub fn run(ffprobe: &str, track: TrackIndex, file: &str) -> Result<Probe, ProbeError> {
    let args = probe_args(ffprobe, file);
    let output = Command::new(&args[0])
        .args(&args[1..])
        .stdin(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| ProbeError::Spawn(e.to_string()))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(ProbeError::Ffprobe(stderr_reason(
            &stderr,
            file,
            output.status.code(),
        )));
    }
    parse_probe(&String::from_utf8_lossy(&output.stdout), track)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn args() {
        assert_eq!(
            probe_args("ffprobe", "a b.mkv"),
            [
                "ffprobe",
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

    #[test]
    fn typical_ffprobe_output() {
        let json = r#"{"streams":[{"index":0,"codec_type":"audio","codec_name":"pcm_s24le","sample_rate":"48000",
            "channels":6,"bits_per_sample":24,"bits_per_raw_sample":"24","duration":"2.000000"}],
            "format":{"duration":"2.0"}}"#;
        assert_eq!(
            parse_probe(json, 0),
            Ok(Probe {
                sample_rate: 48000,
                channels: 6,
                duration: 2.0,
                bit_depth: 24,
                is_truehd: false,
                channel_layout: String::new(),
                codec: "pcm_s24le".into(),
                raw_truehd: false,
                profile: String::new(),
                is_atmos: false,
                master: None
            })
        );
    }

    #[test]
    fn numbers_in_either_form() {
        let json = r#"{"streams":[{"codec_type":"audio","sample_rate":44100,"channels":"2",
            "bits_per_sample":"16","duration":3}]}"#;
        assert_eq!(
            parse_probe(json, 0),
            Ok(Probe {
                sample_rate: 44100,
                channels: 2,
                duration: 3.0,
                bit_depth: 16,
                is_truehd: false,
                channel_layout: String::new(),
                codec: String::new(),
                raw_truehd: false,
                profile: String::new(),
                is_atmos: false,
                master: None
            })
        );
    }

    #[test]
    fn truehd_codec() {
        let json = r#"{"streams":[{"codec_type":"audio","codec_name":"truehd","sample_rate":"48000","channels":8}],
            "format":{"format_name":"truehd"}}"#;
        let probe = parse_probe(json, 0).unwrap();
        assert!(probe.is_truehd && probe.raw_truehd);
        // inside a container, with the duration only in the format
        let json = r#"{"streams":[{"codec_type":"audio","codec_name":"truehd","sample_rate":"48000","channels":8}],
            "format":{"format_name":"matroska,webm","duration":"20.0"}}"#;
        let probe = parse_probe(json, 0).unwrap();
        assert!(probe.is_truehd && !probe.raw_truehd);
        assert_eq!(probe.duration, 20.0);
        let json = r#"{"streams":[{"codec_type":"audio","codec_name":"eac3","sample_rate":"48000","channels":6}]}"#;
        assert!(!parse_probe(json, 0).unwrap().is_truehd);
    }

    #[test]
    fn atmos_profile() {
        let probe = |profile: &str, codec: &str| {
            let json = format!(
                r#"{{"streams":[{{"codec_type":"audio","codec_name":"{codec}","profile":"{profile}",
                "sample_rate":"48000","channels":8}}]}}"#
            );
            parse_probe(&json, 0).unwrap()
        };
        let p = probe("Dolby TrueHD + Dolby Atmos", "truehd");
        assert!(p.is_atmos && p.is_truehd);
        assert_eq!(p.profile, "Dolby TrueHD + Dolby Atmos");
        assert!(probe("Dolby Digital Plus + Dolby Atmos", "eac3").is_atmos);
        assert!(!probe("Dolby Digital Plus", "eac3").is_atmos);
        // no profile at all
        let json = r#"{"streams":[{"codec_type":"audio","codec_name":"truehd","sample_rate":"48000","channels":8}]}"#;
        let p = parse_probe(json, 0).unwrap();
        assert!(p.profile.is_empty() && !p.is_atmos);
    }

    #[test]
    fn defaults() {
        // no duration -> -1; bits_per_sample 0 -> raw -> 32
        let json = r#"{"streams":[{"codec_type":"audio","sample_rate":"48000","channels":2,"bits_per_sample":0}]}"#;
        let p = parse_probe(json, 0).unwrap();
        assert_eq!((p.duration, p.bit_depth), (-1.0, 32));
        let json = r#"{"streams":[{"codec_type":"audio","sample_rate":"48000","channels":2,
            "bits_per_sample":0,"bits_per_raw_sample":"20","duration":"N/A"}]}"#;
        let p = parse_probe(json, 0).unwrap();
        assert_eq!((p.duration, p.bit_depth), (-1.0, 20));
    }

    #[test]
    fn nth_audio_stream_is_picked() {
        let json = r#"{"streams":[
            {"codec_type":"video","codec_name":"h264","sample_rate":"1","channels":1},
            {"codec_type":"audio","codec_name":"ac3","sample_rate":"48000","channels":6},
            {"codec_type":"audio","codec_name":"aac","sample_rate":"44100","channels":2}]}"#;
        assert_eq!(parse_probe(json, 0).unwrap().codec, "ac3");
        let second = parse_probe(json, 1).unwrap();
        assert_eq!((second.codec.as_str(), second.channels), ("aac", 2));
    }

    #[test]
    fn failure_reasons() {
        let message = |json: &str, ti: i64| parse_probe(json, ti).unwrap_err().message();
        let video = r#"{"streams":[{"codec_type":"video","codec_name":"ansi"}]}"#;
        assert_eq!(message(video, 0), "no audio track");
        assert_eq!(message(r#"{"streams":[]}"#, 0), "no audio track");
        let one = r#"{"streams":[{"codec_type":"audio","sample_rate":"1","channels":2}]}"#;
        assert_eq!(message(one, 2), "no audio track 2 (-ti), it has 1 (0)");
        let two = r#"{"streams":[{"codec_type":"audio"},{"codec_type":"audio"}]}"#;
        assert_eq!(message(two, 3), "no audio track 3 (-ti), it has 2 (0-1)");
        let unreadable =
            "ffprobe couldn't read the audio track, try to remux it with ffmpeg or mkvmerge";
        assert_eq!(message(two, 0), unreadable);
        assert_eq!(message("not json", 0), unreadable);
        assert_eq!(message("", 0), unreadable);
        assert_eq!(message("{}", 0), unreadable);
        let no_channels = r#"{"streams":[{"codec_type":"audio","sample_rate":"1"}]}"#;
        assert_eq!(message(no_channels, 0), unreadable);
        assert_eq!(
            ProbeError::Ffprobe("Permission denied".into()).message(),
            "ffprobe: Permission denied"
        );
        assert_eq!(
            ProbeError::Spawn("No such file".into()).message(),
            "failed to run ffprobe: No such file"
        );
        assert_eq!(
            ProbeError::BadName.message(),
            "the file name isn't valid UTF-8"
        );
    }

    /// video 0, audio 1 (6 ch), subtitle 2, audio 3 (2 ch)
    const MIXED: &str = r#"{"streams":[
        {"index":0,"codec_type":"video","codec_name":"h264"},
        {"index":1,"codec_type":"audio","codec_name":"ac3","sample_rate":"48000","channels":6},
        {"index":2,"codec_type":"subtitle","codec_name":"subrip"},
        {"index":3,"codec_type":"audio","codec_name":"aac","sample_rate":"44100","channels":2}]}"#;

    fn ti(text: &str) -> TrackIndex {
        TrackIndex::parse(text).unwrap()
    }

    #[test]
    fn track_index_parsing() {
        assert_eq!(ti("0"), TrackIndex::audio(0));
        assert_eq!(ti("+3"), TrackIndex::audio(3));
        assert_eq!(ti("a:2").to_string(), "a:2");
        assert_eq!(ti("A:2").map_spec(), "0:a:2");
        assert_eq!(ti("S:7").to_string(), "s:7");
        assert_eq!(ti("s:7").map_spec(), "0:7");
        assert_eq!(ti("s:7").filter_label(), "0:7");
        assert_eq!(ti("5").filter_label(), "a:5");
        assert_eq!(TrackIndex::parse("-2"), Err(TrackParseError::Negative));
        assert_eq!(TrackIndex::parse("x"), Err(TrackParseError::NotAnInt));
        assert_eq!(TrackIndex::parse("1.5"), Err(TrackParseError::NotAnInt));
        for bad in ["a:", "s:", "s:-1", "a:x", "v:1", ":1", "s:1:2", "s: 1"] {
            assert_eq!(
                TrackIndex::parse(bad),
                Err(TrackParseError::Malformed),
                "{bad}"
            );
        }
        assert_eq!(TrackIndex::from(-4), TrackIndex::audio(0));
    }

    #[test]
    fn audio_track_and_stream_selection() {
        // the Nth audio track, with or without the prefix
        assert_eq!(parse_probe(MIXED, ti("1")).unwrap().channels, 2);
        assert_eq!(parse_probe(MIXED, ti("a:0")).unwrap().channels, 6);
        assert_eq!(parse_probe(MIXED, ti("a:1")).unwrap().codec, "aac");
        // the stream index of the container
        assert_eq!(parse_probe(MIXED, ti("s:1")).unwrap().channels, 6);
        assert_eq!(parse_probe(MIXED, ti("s:3")).unwrap().channels, 2);
        assert_eq!(
            parse_probe(MIXED, ti("a:2")).unwrap_err().message(),
            "no audio track 2 (-ti), it has 2 (0-1)"
        );
    }

    #[test]
    fn stream_selection_errors() {
        let message = |text: &str| parse_probe(MIXED, ti(text)).unwrap_err().message();
        assert_eq!(
            message("s:0"),
            "stream 0 (-ti s:0) is a video stream, not audio"
        );
        assert_eq!(
            message("s:2"),
            "stream 2 (-ti s:2) is a subtitle stream, not audio"
        );
        assert_eq!(message("s:9"), "no stream 9 (-ti s:9), it has 4 (0-3)");
        // a file without audio is "no audio track" whatever the selection
        let video = r#"{"streams":[{"index":0,"codec_type":"video"}]}"#;
        assert_eq!(
            parse_probe(video, ti("s:0")).unwrap_err(),
            ProbeError::NoAudio
        );
        assert!(!parse_probe(MIXED, ti("s:0")).unwrap_err().skippable());
        // no `index` in the output: the position counts
        let bare = r#"{"streams":[{"codec_type":"video"},{"codec_type":"audio","sample_rate":"48000","channels":2}]}"#;
        assert_eq!(parse_probe(bare, ti("s:1")).unwrap().channels, 2);
    }

    #[test]
    fn skippable_errors() {
        assert!(ProbeError::NoAudio.skippable());
        assert!(ProbeError::Ffprobe(String::new()).skippable());
        assert!(ProbeError::BadName.skippable());
        assert!(!ProbeError::Unreadable.skippable());
        assert!(!ProbeError::Spawn(String::new()).skippable());
        assert!(!ProbeError::MissingTrack { index: 1, count: 1 }.skippable());
    }

    #[test]
    fn stderr_reasons() {
        let reason = |stderr, file| stderr_reason(stderr, file, Some(1));
        assert_eq!(
            reason(
                "fp/archive.zip: Invalid data found when processing input\n",
                "fp/archive.zip"
            ),
            "Invalid data found when processing input"
        );
        assert_eq!(
            reason(
                "[matroska,webm @ 0x5add0de56100] EBML header parsing failed\nfp/empty.mkv: Invalid data found when processing input\n",
                "fp/empty.mkv"
            ),
            "Invalid data found when processing input"
        );
        assert_eq!(
            reason("fp/noperm.wav: Permission denied\n\n", "fp/noperm.wav"),
            "Permission denied"
        );
        assert_eq!(
            reason("[mov @ 0x1f] moov atom not found\n", "x.mp4"),
            "moov atom not found"
        );
        assert_eq!(stderr_reason("", "x", Some(3)), "failed (exit code 3)");
        assert_eq!(stderr_reason(" \n", "x", Some(1)), "failed (exit code 1)");
    }

    #[test]
    fn unusable_output() {
        assert!(parse_probe("", 0).is_err());
        assert!(parse_probe(r#"{"streams":[{"codec_type":"audio","channels":2}]}"#, 0).is_err());
        assert!(
            parse_probe(
                r#"{"streams":[{"codec_type":"audio","sample_rate":"1"}]}"#,
                0
            )
            .is_err()
        );
    }
}
