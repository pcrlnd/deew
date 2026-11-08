//! ffprobe: argument list, output parsing and the process call.

use serde_json::Value;
use std::process::{Command, Stdio};

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
    /// Not JSON, or the chosen stream lacks the sample rate / channels.
    Unreadable,
    /// A file name that isn't valid UTF-8 (never probed).
    BadName,
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
            Self::Unreadable => UNREADABLE.into(),
            Self::BadName => "the file name isn't valid UTF-8".into(),
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
pub fn parse_probe(json: &str, track_index: usize) -> Result<Probe, ProbeError> {
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
    let Some(audio) = audios.get(track_index) else {
        return Err(ProbeError::MissingTrack {
            index: track_index,
            count: audios.len(),
        });
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
pub fn run(ffprobe: &str, track_index: i64, file: &str) -> Result<Probe, ProbeError> {
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
    parse_probe(
        &String::from_utf8_lossy(&output.stdout),
        usize::try_from(track_index).unwrap_or(0),
    )
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
                is_atmos: false
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
                is_atmos: false
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
        let message = |json: &str, ti: usize| parse_probe(json, ti).unwrap_err().message();
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
