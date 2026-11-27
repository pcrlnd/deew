//! harletty reads the DTS:X and Auro-3D sources of the Atmos formats: the
//! detection (`harletty info --json`), the lines of its JSON log and the
//! progress of its decode (the CAF it writes).
//!
//! The formats are taken from harletty's source (docs/harletty-cli.md,
//! `cli/info_report.rs`, `main.rs`, `cli/decode/dts_handler.rs`), not from a
//! run: no DTS:X sample is at hand.

use crate::commands::{harletty_info_args, pipe_args, wav_pipe_args};
use crate::deewlog::strip_ansi;
use crate::probe::{Probe, TrackIndex};
use serde_json::Value;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};

/// What a DTS track carries beyond its bed, as `harletty info --json` reports
/// it (its `spatial` object).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spatial {
    /// The `sourceCodec` harletty writes, `DTS:X-7.1.4+3`, `Auro-3D-13.1`.
    pub label: String,
    /// `dtsx` or `auro`.
    pub kind: String,
    /// Waveforms presented as objects, when the stream says.
    pub objects: Option<u32>,
    /// The feed identities rest on corpus evidence, not an established layout.
    pub experimental: bool,
    /// The decoder's name for a DTS:X presentation (`ObjectsD0`).
    pub presentation: Option<String>,
    /// The speaker layout of an Auro-3D source (`13.1` of `Auro-3D-13.1`),
    /// `None` for DTS:X or a label without one.
    pub auro_layout: Option<String>,
    /// The layout that carries the Auro-3D channels (`7.1`), from harletty's
    /// `auro.carrier`.
    pub auro_carrier: Option<String>,
}

impl Spatial {
    /// The warning for an experimental presentation, `None` for the others.
    pub fn experimental_warning(&self, name: &str) -> Option<String> {
        if !self.experimental {
            return None;
        }
        let what = match &self.presentation {
            Some(presentation) => format!("{} ({presentation})", self.label),
            None => self.label.clone(),
        };
        Some(format!(
            "{name}: {what} is decoded experimentally by harletty, check the result."
        ))
    }
}

/// The `spatial` of the `info --json` line of harletty, when the DTS track
/// is a DTS:X or Auro-3D source (so an Atmos source of deew); `Ok(None)` for
/// a track that carries neither. `Err` is harletty's own reason when it found
/// no frame, or the problem with its output.
pub fn parse_info(stdout: &str) -> Result<Option<Spatial>, String> {
    let value: Value = stdout
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with('{'))
        .find_map(|line| serde_json::from_str(line).ok())
        .ok_or("it printed no JSON")?;
    match value.get("codec") {
        None => return Err("unexpected output (no codec)".to_string()),
        Some(Value::Null) => {
            return Err(value
                .get("error")
                .and_then(Value::as_str)
                .map_or("found no DTS frame".to_string(), str::to_string));
        }
        Some(_) => {}
    }
    let Some(spatial) = value.get("spatial").and_then(Value::as_object) else {
        return Ok(None);
    };
    let kind = spatial
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if kind != "dtsx" && kind != "auro" {
        return Ok(None);
    }
    let label = spatial
        .get("label")
        .and_then(Value::as_str)
        .filter(|label| !label.is_empty())
        .unwrap_or(kind)
        .to_string();
    let auro_layout = (kind == "auro")
        .then(|| label.strip_prefix("Auro-3D-").filter(|l| !l.is_empty()))
        .flatten()
        .map(str::to_string);
    let auro_carrier = (kind == "auro")
        .then(|| {
            value
                .get("auro")
                .and_then(|a| a.get("carrier"))
                .and_then(Value::as_str)
                .filter(|c| !c.is_empty())
                .map(str::to_string)
        })
        .flatten();
    Ok(Some(Spatial {
        label,
        kind: kind.to_string(),
        auro_layout,
        auro_carrier,
        objects: spatial
            .get("objects")
            .and_then(Value::as_u64)
            .and_then(|n| u32::try_from(n).ok()),
        experimental: spatial
            .get("experimental")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        presentation: spatial
            .get("presentation")
            .and_then(Value::as_str)
            .filter(|p| !p.is_empty())
            .map(str::to_string),
    }))
}

/// The last line harletty printed on stderr, without `Error: `.
fn last_error(stderr: &str) -> Option<String> {
    if let Some(message) = stderr.lines().find_map(clap_error) {
        return Some(message);
    }
    let line = stderr.lines().map(str::trim).rfind(|l| !l.is_empty())?;
    let line = strip_ansi(line);
    Some(line.strip_prefix("Error: ").unwrap_or(&line).to_string())
}

/// The `error: ...` line clap prints for a usage error, without the prefix.
fn clap_error(line: &str) -> Option<String> {
    let line = strip_ansi(line);
    let message = line.trim().strip_prefix("error: ")?.trim();
    (!message.is_empty()).then(|| message.to_string())
}

/// How the audio of an input reaches harletty.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Feed {
    /// A raw DTS file, read directly.
    RawDts,
    /// A DTS track of a container, copied by ffmpeg as a raw DTS stream.
    Dts,
    /// 24-bit lossless PCM (FLAC, PCM, TrueHD without Atmos) decoded by
    /// ffmpeg to a wav, for `--codec wav`; `truehd` for its decoder options.
    Wav { truehd: bool },
}

impl Feed {
    /// The way harletty gets this input.
    pub fn of(probe: &Probe) -> Feed {
        if probe.pcm_auro_candidate() {
            Feed::Wav {
                truehd: probe.is_truehd,
            }
        } else if probe.raw_dts {
            Feed::RawDts
        } else {
            Feed::Dts
        }
    }

    /// Whether ffmpeg feeds the stdin of harletty.
    pub fn piped(self) -> bool {
        self != Feed::RawDts
    }

    pub fn is_wav(self) -> bool {
        matches!(self, Feed::Wav { .. })
    }

    /// The ffmpeg command that feeds harletty, `None` for a raw DTS file.
    pub fn ffmpeg_args(self, ffmpeg: &str, file: &str, track: TrackIndex) -> Option<Vec<String>> {
        match self {
            Feed::RawDts => None,
            Feed::Dts => Some(pipe_args(ffmpeg, file, track, "dts")),
            Feed::Wav { truehd } => Some(wav_pipe_args(ffmpeg, file, track, truehd)),
        }
    }
}

/// Ask harletty about the track of `file`: a raw DTS stream is read
/// directly, any other track is fed by ffmpeg into its stdin (see [`Feed`]).
/// harletty stops reading after at most 20 seconds of audio; ffmpeg is then
/// stopped too, its broken pipe is not an error.
pub fn detect(
    harletty: &str,
    ffmpeg: &str,
    file: &str,
    track: TrackIndex,
    feed: Feed,
) -> Result<Option<Spatial>, String> {
    let args = harletty_info_args(
        harletty,
        if feed.piped() { "-" } else { file },
        feed.is_wav(),
    );
    let mut command = Command::new(&args[0]);
    command
        .args(&args[1..])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut feeder = None;
    if let Some(copy) = feed.ffmpeg_args(ffmpeg, file, track) {
        let mut child = Command::new(&copy[0])
            .args(&copy[1..])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("failed to run ffmpeg: {e}"))?;
        command.stdin(Stdio::from(child.stdout.take().expect("piped stdout")));
        feeder = Some(child);
    } else {
        command.stdin(Stdio::null());
    }
    let output = command.output();
    // the stdin of harletty is closed: ffmpeg would only fail on it, or wait
    // for a reader that is gone
    drop(command);
    if let Some(mut child) = feeder {
        let _ = child.kill();
        let _ = child.wait();
    }
    let output = output.map_err(|e| format!("failed to run harletty: {e}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(
            last_error(&stderr).unwrap_or_else(|| match output.status.code() {
                Some(code) => format!("failed (exit code {code})"),
                None => "was terminated by a signal".to_string(),
            }),
        );
    }
    parse_info(&stdout)
}

// ---------------------------------------------------------------------------
// The log of a decode

/// A line of the log of `harletty decode` (stderr).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LogLine {
    Error(String),
    Warn(String),
    Other,
}

/// The level and message of one record of `--log-format json`:
/// `{"ts":2026-11-13T16:55:49Z,"lvl":"WARN","msg":"..."}`. harletty writes the
/// timestamp without quotes and the message without escapes (`main.rs`), so
/// the line is generally no valid JSON; it is tried as such first, then cut
/// by hand: the message is everything up to the closing `"}`.
fn parse_record(line: &str) -> Option<(String, String)> {
    if let Ok(value) = serde_json::from_str::<Value>(line) {
        return Some((
            value.get("lvl")?.as_str()?.to_string(),
            value.get("msg")?.as_str()?.to_string(),
        ));
    }
    let rest = line.strip_prefix('{')?;
    let after = |key: &str, from: &str| -> Option<usize> { Some(from.find(key)? + key.len()) };
    let level_start = after("\"lvl\":\"", rest)?;
    let level_len = rest[level_start..].find('"')?;
    let level = rest[level_start..level_start + level_len].to_string();
    let message_start =
        level_start + level_len + after("\"msg\":\"", &rest[level_start + level_len..])?;
    let message = rest[message_start..].trim_end();
    let message = message
        .strip_suffix("\"}")
        .or_else(|| message.strip_suffix('}'))
        .unwrap_or(message);
    Some((level, message.to_string()))
}

/// What a line of the stderr of `harletty decode` is: an ERROR or a WARN
/// record of the log, or the `Error: ...` the process prints when it fails.
pub fn classify(line: &str) -> LogLine {
    let plain = strip_ansi(line);
    let plain = plain.trim();
    if let Some(message) = clap_error(plain) {
        return LogLine::Error(message);
    }
    if let Some(message) = plain.strip_prefix("Error: ") {
        let message = message.trim();
        return if message.is_empty() {
            LogLine::Other
        } else {
            LogLine::Error(message.to_string())
        };
    }
    match parse_record(plain) {
        Some((level, message)) if level.eq_ignore_ascii_case("error") => LogLine::Error(message),
        Some((level, message)) if level.eq_ignore_ascii_case("warn") => LogLine::Warn(message),
        _ => LogLine::Other,
    }
}

/// The experimental-presentation warning of the log: deew says it itself
/// (from the detection) before it decodes.
fn is_experimental_notice(message: &str) -> bool {
    message.contains("is an experimental presentation")
}

/// The one warning of a file for the WARN records of its decode (harletty
/// replaced or dropped damaged frames, changed layout, ...), `None` without
/// any.
pub fn warning_text(name: &str, warnings: &[String]) -> Option<String> {
    let mut distinct: Vec<&str> = Vec::new();
    for message in warnings {
        let message = message.trim();
        if !message.is_empty() && !is_experimental_notice(message) && !distinct.contains(&message) {
            distinct.push(message);
        }
    }
    if distinct.is_empty() {
        return None;
    }
    const SHOWN: usize = 3;
    let mut text = distinct[..distinct.len().min(SHOWN)].join("; ");
    if distinct.len() > SHOWN {
        text.push_str(&format!(" (and {} more)", distinct.len() - SHOWN));
    }
    Some(format!(
        "harletty reported problems in {name}: {text}. The output may have a gap or be slightly out of sync; check or re-rip the source."
    ))
}

/// What an exit code of harletty says when it printed no error message (it
/// exits with 1 for any error).
pub fn exit_phrase(code: Option<i32>) -> Option<&'static str> {
    match code? {
        2 => Some("invalid arguments"),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// The decoded audio

/// What the header of a CAF file says, and how much audio it holds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Audio {
    pub rate: f64,
    pub channels: usize,
    pub bytes_per_sample: usize,
    /// Bytes of sample data written so far.
    pub data_bytes: u64,
}

impl Audio {
    /// Seconds of audio in the data written so far.
    pub fn seconds(&self) -> f64 {
        let frame = (self.channels * self.bytes_per_sample) as f64;
        self.data_bytes as f64 / frame / self.rate
    }
}

/// Read the `desc` chunk and the start of the `data` chunk of the CAF in
/// `head` (the first bytes of a file of `file_len` bytes).
pub fn parse_caf(head: &[u8], file_len: u64) -> Option<Audio> {
    if head.get(..4)? != b"caff" {
        return None;
    }
    let mut format: Option<(f64, usize, usize)> = None;
    let mut position = 8usize;
    while position + 12 <= head.len() {
        let kind = &head[position..position + 4];
        let size = i64::from_be_bytes(head[position + 4..position + 12].try_into().ok()?);
        let body = position + 12;
        if kind == b"desc" {
            let desc = head.get(body..body + 32)?;
            let rate = f64::from_be_bytes(desc[..8].try_into().ok()?);
            let channels = u32::from_be_bytes(desc[24..28].try_into().ok()?) as usize;
            let bits = u32::from_be_bytes(desc[28..32].try_into().ok()?) as usize;
            format = Some((rate, channels, bits / 8));
        } else if kind == b"data" {
            let (rate, channels, bytes_per_sample) = format?;
            // the chunk starts with a 4-byte edit count
            let data_bytes = file_len.saturating_sub((body + 4) as u64);
            return valid(rate, channels, bytes_per_sample, data_bytes);
        }
        position = body.checked_add(usize::try_from(size).ok()?)?;
    }
    let (rate, channels, bytes_per_sample) = format?;
    valid(rate, channels, bytes_per_sample, 0)
}

fn valid(rate: f64, channels: usize, bytes_per_sample: usize, data_bytes: u64) -> Option<Audio> {
    (rate > 0.0 && rate.is_finite() && channels > 0 && bytes_per_sample > 0).then_some(Audio {
        rate,
        channels,
        bytes_per_sample,
        data_bytes,
    })
}

/// The audio written so far to the CAF at `path`; `None` while the file or
/// its header is not there yet.
pub fn read_audio(path: impl AsRef<Path>) -> Option<Audio> {
    let mut file = File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    let mut head = Vec::new();
    file.by_ref().take(4096).read_to_end(&mut head).ok()?;
    parse_caf(&head, len)
}

/// Progress (0 to 99) of the decode: the seconds written of the `duration`
/// of the input (-1 if unknown). `None` when either is not known.
pub fn decode_percent(audio: Option<Audio>, duration: f64) -> Option<f64> {
    if duration <= 0.0 {
        return None;
    }
    Some((audio?.seconds() / duration * 100.0).clamp(0.0, 99.0))
}

/// Check what was decoded against the input. harletty has no result to read
/// (unlike truehdd's `--json`), so the audio file is the evidence: it must
/// hold audio, and as much as the `length` of the input (seconds, -1 if
/// unknown; `check_length` is false when it is only an estimate).
pub fn check_decoded(audio: Option<Audio>, length: f64, check_length: bool) -> Result<(), String> {
    let Some(audio) = audio else {
        return Err("harletty failed: no audio file was written.".to_string());
    };
    let decoded = audio.seconds();
    if audio.data_bytes == 0 {
        return Err("harletty decoded no audio.".to_string());
    }
    if check_length && length > 0.0 && (decoded - length).abs() > 1.0 {
        return Err(if decoded < length {
            format!("harletty decoded only {decoded:.1} s of {length:.1} s.")
        } else {
            format!("harletty decoded {decoded:.1} s, the input is {length:.1} s.")
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const DTSX: &str = r#"{"schema":1,"harletty":"0.9.0","build":"x","taxonomy":4,"codec":"DTS-HD MA","channels":8,"sample_rate":48000,"spatial":{"label":"DTS:X-7.1.4+3","kind":"dtsx","objects":3,"fixed":4,"experimental":true,"presentation":"ObjectsD0"},"frames_seen":1,"seconds_seen":0.0106}"#;
    const AURO: &str = r#"{"schema":1,"harletty":"0.9.0","build":"x","taxonomy":4,"codec":"DTS-HD MA","channels":8,"sample_rate":48000,"spatial":{"label":"Auro-3D-13.1","kind":"auro","objects":null,"fixed":null,"experimental":false},"auro":{"carrier":"7.1","original":"7.1_5H_1T"},"frames_seen":33,"seconds_seen":0.4}"#;
    const PLAIN: &str = r#"{"schema":1,"harletty":"0.9.0","build":"x","taxonomy":4,"codec":"DTS-HD MA","channels":6,"sample_rate":48000,"spatial":null,"frames_seen":90,"seconds_seen":1.0}"#;

    #[test]
    fn info_of_dtsx() {
        let spatial = parse_info(&format!("{DTSX}\n")).unwrap().unwrap();
        assert_eq!(spatial.label, "DTS:X-7.1.4+3");
        assert_eq!(spatial.kind, "dtsx");
        assert_eq!(spatial.objects, Some(3));
        assert!(spatial.experimental);
        assert_eq!(spatial.presentation.as_deref(), Some("ObjectsD0"));
    }

    #[test]
    fn info_of_auro() {
        let spatial = parse_info(AURO).unwrap().unwrap();
        assert_eq!(spatial.label, "Auro-3D-13.1");
        assert_eq!(spatial.kind, "auro");
        assert_eq!(spatial.objects, None);
        assert!(!spatial.experimental);
        assert_eq!(spatial.presentation, None);
        assert_eq!(spatial.auro_layout.as_deref(), Some("13.1"));
        assert_eq!(spatial.auro_carrier.as_deref(), Some("7.1"));
        // DTS:X has neither
        let dtsx = parse_info(DTSX).unwrap().unwrap();
        assert_eq!((dtsx.auro_layout, dtsx.auro_carrier), (None, None));
    }

    #[test]
    fn auro_without_layout_or_carrier() {
        let bare = AURO
            .replace("Auro-3D-13.1", "Auro-3D")
            .replace(r#","auro":{"carrier":"7.1","original":"7.1_5H_1T"}"#, "");
        let spatial = parse_info(&bare).unwrap().unwrap();
        assert_eq!(spatial.label, "Auro-3D");
        assert_eq!((spatial.auro_layout, spatial.auro_carrier), (None, None));
        let odd = AURO.replace(r#""carrier":"7.1""#, r#""carrier":7"#);
        assert_eq!(parse_info(&odd).unwrap().unwrap().auro_carrier, None);
    }

    #[test]
    fn info_of_plain_tracks_and_other_kinds() {
        assert_eq!(parse_info(PLAIN), Ok(None));
        // a missing `spatial` is a plain track too
        assert_eq!(
            parse_info(r#"{"schema":1,"codec":"DTS","channels":6}"#),
            Ok(None)
        );
        // spatial, but not DTS:X or Auro-3D
        let joc = PLAIN.replace(
            r#""spatial":null"#,
            r#""spatial":{"label":"EAC3-JOC","kind":"joc","experimental":false}"#,
        );
        assert_eq!(parse_info(&joc), Ok(None));
    }

    #[test]
    fn info_errors() {
        let no_frame = r#"{"schema":1,"harletty":"0.9.0","codec":null,"error":"no DTS frame found in the first 20 s","channels":null,"sample_rate":null,"spatial":null}"#;
        assert_eq!(
            parse_info(no_frame),
            Err("no DTS frame found in the first 20 s".to_string())
        );
        assert_eq!(
            parse_info(r#"{"codec":null}"#),
            Err("found no DTS frame".to_string())
        );
        assert!(parse_info("").is_err());
        assert!(parse_info("not json").is_err());
        assert!(parse_info("{}").is_err());
        // log lines before the object are skipped
        assert!(parse_info(&format!("INFO hi\n{DTSX}")).unwrap().is_some());
    }

    #[test]
    fn missing_label_falls_back_to_the_kind() {
        let json = PLAIN.replace(r#""spatial":null"#, r#""spatial":{"kind":"auro"}"#);
        let spatial = parse_info(&json).unwrap().unwrap();
        assert_eq!(spatial.label, "auro");
        assert!(!spatial.experimental && spatial.presentation.is_none());
    }

    #[test]
    fn experimental_warning_text() {
        let mut spatial = parse_info(DTSX).unwrap().unwrap();
        assert_eq!(
            spatial.experimental_warning("a.mkv").as_deref(),
            Some(
                "a.mkv: DTS:X-7.1.4+3 (ObjectsD0) is decoded experimentally by harletty, check the result."
            )
        );
        spatial.presentation = None;
        assert_eq!(
            spatial.experimental_warning("a.mkv").as_deref(),
            Some("a.mkv: DTS:X-7.1.4+3 is decoded experimentally by harletty, check the result.")
        );
        spatial.experimental = false;
        assert_eq!(spatial.experimental_warning("a.mkv"), None);
    }

    #[test]
    fn log_records() {
        // what harletty writes: the timestamp is not quoted
        let record = |lvl: &str, msg: &str| {
            format!(r#"{{"ts":2026-11-13T16:55:49Z,"lvl":"{lvl}","msg":"{msg}"}}"#)
        };
        assert_eq!(
            classify(&record("ERROR", "failed to write .atmos header: disk full")),
            LogLine::Error("failed to write .atmos header: disk full".into())
        );
        // quotes in the message are not escaped
        assert_eq!(
            classify(&record(
                "WARN",
                "DTS core decode error (dropping frame): Err(\"x\")"
            )),
            LogLine::Warn("DTS core decode error (dropping frame): Err(\"x\")".into())
        );
        assert_eq!(
            classify(&record(
                "INFO",
                "Decoding DTS stream: a.dts (strict mode: false)"
            )),
            LogLine::Other
        );
        // valid JSON works too
        assert_eq!(
            classify(r#"{"ts":"x","lvl":"WARN","msg":"a \"b\""}"#),
            LogLine::Warn("a \"b\"".into())
        );
        // what the process prints when it fails
        assert_eq!(
            classify("Error: invalid DTS header: Bad"),
            LogLine::Error("invalid DTS header: Bad".into())
        );
        assert_eq!(classify("Error: "), LogLine::Other);
        assert_eq!(
            classify("\u{1b}[31mError: boom\u{1b}[0m"),
            LogLine::Error("boom".into())
        );
        assert_eq!(classify("not a record"), LogLine::Other);
        assert_eq!(classify(""), LogLine::Other);
        assert_eq!(classify(r#"{"lvl":"WARN"}"#), LogLine::Other);
    }

    #[test]
    fn warnings_of_a_file() {
        let list = |items: &[&str]| items.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(warning_text("a.mkv", &[]), None);
        // deew says the experimental presentation itself
        assert_eq!(
            warning_text(
                "a.mkv",
                &list(&[
                    "DTS:X ObjectsD0 is an experimental presentation: its feed identities rest on corpus evidence"
                ])
            ),
            None
        );
        assert_eq!(
            warning_text("a.mkv", &list(&["x", "x", " y "])).as_deref(),
            Some(
                "harletty reported problems in a.mkv: x; y. The output may have a gap or be slightly out of sync; check or re-rip the source."
            )
        );
        let text = warning_text("a.mkv", &list(&["a", "b", "c", "d", "e"])).unwrap();
        assert!(text.contains(": a; b; c (and 2 more)."), "{text}");
    }

    #[test]
    fn clap_usage_error() {
        let stderr = "error: unexpected argument '--threads' found\n\n  tip: to pass '--threads' as a value, use '-- --threads'\n\nUsage: harletty decode [OPTIONS] <INPUT>\n\nFor more information, try '--help'.\n";
        assert_eq!(
            last_error(stderr).as_deref(),
            Some("unexpected argument '--threads' found")
        );
        assert_eq!(
            classify("\x1b[1m\x1b[31merror:\x1b[0m unexpected argument '--threads' found"),
            LogLine::Error("unexpected argument '--threads' found".to_string())
        );
        assert_eq!(
            classify("error: unexpected argument '--threads' found"),
            LogLine::Error("unexpected argument '--threads' found".to_string())
        );
        assert_eq!(last_error("Error: boom\n").as_deref(), Some("boom"));
    }

    #[test]
    fn exit_phrases() {
        assert_eq!(exit_phrase(Some(2)), Some("invalid arguments"));
        assert_eq!(exit_phrase(Some(1)), None);
        assert_eq!(exit_phrase(None), None);
    }

    /// A CAF header as the writer makes it: `desc`, an empty `chan`, `data`.
    fn caf(rate: f64, channels: u32, bits: u32, chan: usize, samples: usize) -> Vec<u8> {
        let mut out = b"caff".to_vec();
        out.extend(1u16.to_be_bytes());
        out.extend(0u16.to_be_bytes());
        out.extend(b"desc");
        out.extend(32i64.to_be_bytes());
        out.extend(rate.to_be_bytes());
        out.extend(b"lpcm");
        out.extend(4u32.to_be_bytes());
        out.extend((channels * bits / 8).to_be_bytes());
        out.extend(1u32.to_be_bytes());
        out.extend(channels.to_be_bytes());
        out.extend(bits.to_be_bytes());
        out.extend(b"chan");
        out.extend((chan as i64).to_be_bytes());
        out.extend(vec![0u8; chan]);
        out.extend(b"data");
        out.extend((-1i64).to_be_bytes());
        out.extend(0u32.to_be_bytes());
        out.extend(vec![0u8; samples * (bits / 8) as usize]);
        out
    }

    #[test]
    fn caf_header_and_progress() {
        // one second of 12 channels of 24 bits
        let bytes = caf(48000.0, 12, 24, 12, 48000 * 12);
        let audio = parse_caf(&bytes[..4096.min(bytes.len())], bytes.len() as u64).unwrap();
        assert_eq!(audio.rate, 48000.0);
        assert_eq!(audio.channels, 12);
        assert_eq!(audio.bytes_per_sample, 3);
        assert_eq!(audio.data_bytes, 48000 * 12 * 3);
        assert!((audio.seconds() - 1.0).abs() < 1e-9);
        // half of 20 s
        let half = Audio {
            data_bytes: 48000 * 12 * 3 * 10,
            ..audio
        };
        assert_eq!(decode_percent(Some(half), 20.0), Some(50.0));
        // never full before harletty exits
        assert_eq!(decode_percent(Some(half), 5.0), Some(99.0));
        // unknown: no duration, no file
        assert_eq!(decode_percent(Some(half), -1.0), None);
        assert_eq!(decode_percent(None, 20.0), None);
        // the header only
        let header = caf(48000.0, 8, 24, 0, 0);
        let audio = parse_caf(&header, header.len() as u64).unwrap();
        assert_eq!((audio.channels, audio.data_bytes), (8, 0));
        // a header that is not complete yet, or not a CAF
        assert_eq!(parse_caf(&header[..30], 30), None);
        assert_eq!(parse_caf(b"RIFFxxxxxxxxxxxxxxxxxxxx", 24), None);
        assert_eq!(parse_caf(b"", 0), None);
        // 0 channels never divide
        let zero = caf(48000.0, 0, 24, 0, 0);
        assert_eq!(parse_caf(&zero, zero.len() as u64), None);
    }

    #[test]
    fn audio_is_read_from_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.atmos.audio");
        assert_eq!(read_audio(&path), None);
        std::fs::write(&path, caf(48000.0, 6, 24, 4, 48000 * 6 * 2)).unwrap();
        let audio = read_audio(&path).unwrap();
        assert_eq!(audio.channels, 6);
        assert!((audio.seconds() - 2.0).abs() < 1e-9);
    }

    #[test]
    fn decoded_length_is_checked() {
        let at = |seconds: f64| {
            Some(Audio {
                rate: 48000.0,
                channels: 12,
                bytes_per_sample: 3,
                data_bytes: (seconds * 48000.0) as u64 * 36,
            })
        };
        assert_eq!(check_decoded(at(20.0), 20.0, true), Ok(()));
        assert_eq!(check_decoded(at(20.9), 20.0, true), Ok(()));
        assert_eq!(
            check_decoded(at(0.2), 20.0, true),
            Err("harletty decoded only 0.2 s of 20.0 s.".to_string())
        );
        assert_eq!(
            check_decoded(at(25.0), 20.0, true),
            Err("harletty decoded 25.0 s, the input is 20.0 s.".to_string())
        );
        // no duration, or only an estimate of it
        assert_eq!(check_decoded(at(5.0), -1.0, true), Ok(()));
        assert_eq!(check_decoded(at(5.0), 20.0, false), Ok(()));
        assert_eq!(
            check_decoded(at(0.0), 20.0, false),
            Err("harletty decoded no audio.".to_string())
        );
        assert_eq!(
            check_decoded(None, 20.0, true),
            Err("harletty failed: no audio file was written.".to_string())
        );
    }

    /// A fake harletty and ffmpeg: detection of a track copied by ffmpeg ends
    /// when harletty is done, although ffmpeg would write forever.
    #[cfg(unix)]
    mod fakes {
        use super::*;
        use std::os::unix::fs::PermissionsExt;

        fn script(dir: &Path, name: &str, body: &str) -> String {
            let path = dir.join(name);
            std::fs::write(
                &path,
                format!("#!/bin/sh\n[ -n \"$DEEW_PROBE\" ] && exit 0\n{body}\n"),
            )
            .unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
            // A process forked by another test thread can hold the write fd
            // for a moment, so exec fails with ETXTBSY: wait until it works.
            for _ in 0..200 {
                match std::process::Command::new(&path)
                    .env("DEEW_PROBE", "1")
                    .status()
                {
                    Err(e) if e.raw_os_error() == Some(26) => {
                        std::thread::sleep(Duration::from_millis(10))
                    }
                    _ => break,
                }
            }
            path.to_string_lossy().into_owned()
        }

        #[test]
        fn detection_does_not_wait_for_ffmpeg() {
            let dir = tempfile::tempdir().unwrap();
            // harletty answers after reading a little of its stdin
            let harletty = script(
                dir.path(),
                "harletty",
                &format!("head -c 10 > /dev/null\necho '{DTSX}'"),
            );
            let ffmpeg = script(dir.path(), "ffmpeg", "exec yes");
            let started = std::time::Instant::now();
            let found = detect(&harletty, &ffmpeg, "a.mkv", TrackIndex::audio(0), Feed::Dts);
            assert!(started.elapsed() < Duration::from_secs(20));
            assert_eq!(found.unwrap().unwrap().kind, "dtsx");
        }

        #[test]
        fn detection_of_a_raw_file_and_of_failures() {
            let dir = tempfile::tempdir().unwrap();
            let ok = script(dir.path(), "ok", &format!("echo '{PLAIN}'"));
            assert_eq!(
                detect(&ok, "unused", "a.dts", TrackIndex::audio(0), Feed::RawDts),
                Ok(None)
            );
            let failing = script(
                dir.path(),
                "failing",
                "echo 'Error: No such file or directory (os error 2)' >&2\nexit 1",
            );
            assert_eq!(
                detect(
                    &failing,
                    "unused",
                    "a.dts",
                    TrackIndex::audio(0),
                    Feed::RawDts
                ),
                Err("No such file or directory (os error 2)".to_string())
            );
            let silent = script(dir.path(), "silent", "exit 3");
            assert_eq!(
                detect(
                    &silent,
                    "unused",
                    "a.dts",
                    TrackIndex::audio(0),
                    Feed::RawDts
                ),
                Err("failed (exit code 3)".to_string())
            );
            let missing = dir.path().join("none").to_string_lossy().into_owned();
            assert!(
                detect(
                    &missing,
                    "unused",
                    "a.dts",
                    TrackIndex::audio(0),
                    Feed::RawDts
                )
                .unwrap_err()
                .starts_with("failed to run harletty")
            );
        }
    }

    use std::time::Duration;
}
