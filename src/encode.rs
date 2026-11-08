//! Running one input through ffmpeg / truehdd and the DEE tools, with
//! progress reporting.

use crate::deewlog::{self, LogData, LogSpec, StageLog, Stream};
use crate::error::escape;
use crate::events;
use crate::interrupt;
use crate::plan::Format;
use crate::ui::{Bar, Stage, Ui};
use crate::util::{basename, clamp_i, file_name};
use regex::Regex;
use serde_json::{Value, json};
use std::io::Read;
use std::ops::{Deref, DerefMut};
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, SystemTime};

/// Everything needed to encode one input. The command lines are empty for
/// the stages a job does not have.
#[derive(Debug, Clone)]
pub struct Job {
    pub input: String,
    /// Duration in seconds, -1 if unknown.
    pub length: f64,
    pub format: Format,
    /// ffmpeg: converts the input to the wav, or for the Atmos formats copies
    /// the TrueHD track to stdout, which is piped into truehdd.
    pub ffmpeg_args: Vec<String>,
    /// truehdd: decodes the Atmos formats' input.
    pub decode_args: Vec<String>,
    /// `dee_measure_loudness`, for `-mo` and a custom dialnorm.
    pub measure_args: Vec<String>,
    /// The encoder (empty for `-mo`).
    pub encode_args: Vec<String>,
    /// The wav, or the `.atmos` file, already exists.
    pub intermediate_exists: bool,
    /// The wav, or the `.atmos` file.
    pub intermediate: String,
    /// Loudness manifest written by the measurement.
    pub loudness_path: String,
    /// Custom dialnorm that goes into the manifest, 0 for none.
    pub dialnorm: i64,
    pub output_path: String,
    pub output_dir: String,
    /// Files in the temp directory that are removed afterwards (not with `-k`).
    pub temp_files: Vec<String>,
    /// DEE's `--temp-dir` for this job, removed with everything in it when the job ends.
    pub dee_temp: String,
    /// The report of `-kl`.
    pub log: Option<LogSpec>,
}

/// Settings shared by all jobs.
#[derive(Debug, Clone, Copy)]
pub struct Options {
    pub keep_temp: bool,
    /// `-kl`: keep TrueHD's log and write the report of the job.
    pub keep_log: bool,
}

// ---------------------------------------------------------------------------
// Progress of one step

/// Where DEE's overall percentage leaves the first of its two steps.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FirstStep {
    /// The first step ends at this percentage (50 for most formats).
    EndsAt(f64),
    /// The first step does not report a usable percentage (a pulsing bar).
    Pulsing,
}

impl FirstStep {
    /// The first step of a DEE run that measures and encodes in one process
    /// (`Step 1/2`). DEE 6.7 switches DD, DDP, AC4 and DDP Atmos at 50 %,
    /// TrueHD Atmos at 42 %; non-Atmos TrueHD preprocesses for about 2 %.
    pub fn of_format(format: Format) -> FirstStep {
        match format {
            Format::ThdAtmos => FirstStep::EndsAt(42.0),
            Format::Thd => FirstStep::Pulsing,
            _ => FirstStep::EndsAt(50.0),
        }
    }
}

/// What the bar shows.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Shown {
    /// 0 to 100 within the current step.
    Percent(f64),
    Pulse,
}

/// Maps the overall percentage of a DEE run to 0 to 100 % per step.
#[derive(Debug, Clone)]
pub struct StepMap {
    first: FirstStep,
    /// The measurement (first step) is running.
    measuring: bool,
    /// Overall percentage where the second step began.
    start: f64,
    /// The last overall percentage.
    raw: f64,
    shown: f64,
}

impl StepMap {
    pub fn new(stage: Stage, first: FirstStep) -> StepMap {
        StepMap {
            first,
            measuring: stage == Stage::Measure,
            start: 0.0,
            raw: 0.0,
            shown: 0.0,
        }
    }

    /// What the bar shows at the beginning of the current step.
    pub fn initial(&self) -> Shown {
        if self.measuring && self.first == FirstStep::Pulsing {
            Shown::Pulse
        } else {
            Shown::Percent(0.0)
        }
    }

    /// DEE began a step; the bar starts again at 0.
    pub fn step(&mut self, stage: Stage) -> Shown {
        self.measuring = stage == Stage::Measure;
        if !self.measuring {
            self.start = self.raw;
        }
        self.shown = 0.0;
        self.initial()
    }

    /// DEE reported `percent` (0 to 100) overall.
    pub fn progress(&mut self, percent: f64) -> Shown {
        self.raw = percent.clamp(0.0, 100.0);
        let value = if self.measuring {
            match self.first {
                FirstStep::Pulsing => return Shown::Pulse,
                FirstStep::EndsAt(end) => self.raw / end * 100.0,
            }
        } else if self.start >= 100.0 {
            100.0
        } else {
            (self.raw - self.start) / (100.0 - self.start) * 100.0
        };
        let value = if value.is_nan() { 0.0 } else { value };
        self.shown = self.shown.max(value.clamp(0.0, 100.0));
        Shown::Percent(self.shown)
    }
}

fn show(bar: &Bar, shown: Shown) {
    match shown {
        Shown::Percent(percent) => bar.percent(percent),
        Shown::Pulse => bar.indeterminate(),
    }
}

// ---------------------------------------------------------------------------
// Output parsing

static FFMPEG_TIME: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"time=(\S+)").expect("valid regex"));
static FFMPEG_PREFIX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\[[^\]]*@ 0x[0-9a-fA-F]+\]\s*").expect("valid regex"));
static DEE_PROGRESS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[[=> ]*\]\s*(\d+) %").expect("valid regex"));
static DEE_STEP: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"Step (\d+)/(\d+) : (.+)").expect("valid regex"));
static DEE_LOUDNESS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"measured_loudness=(-?\d+(?:\.\d+)?)").expect("valid regex"));
static DEE_ERROR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"ERROR: (.*)").expect("valid regex"));
static DEE_WARNING: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"WARNING: (.*)").expect("valid regex"));
static ATMOS_FRAMES: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"Found (\d+) extractable frames").expect("valid regex"));
static ATMOS_CHANNELS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(\d+) bed channels? \+ (\d+) objects?").expect("valid regex"));

/// `HH:MM:SS.xx` to seconds.
pub fn stamp_to_sec(stamp: &str) -> Option<f64> {
    let mut parts = stamp.split(':');
    let hours: i64 = parts.next()?.parse().ok()?;
    let minutes: i64 = parts.next()?.parse().ok()?;
    let seconds: f64 = parts.next()?.parse().ok()?;
    if !seconds.is_finite() {
        return None;
    }
    Some((hours * 3600 + minutes * 60) as f64 + seconds)
}

/// Progress (0 to 100) reported by a line of the ffmpeg statistics.
pub fn ffmpeg_percent(line: &str, length: f64) -> Option<f64> {
    if !line.contains('=') {
        return None;
    }
    let stamp = FFMPEG_TIME.captures(line)?;
    let seconds = stamp_to_sec(&stamp[1])?;
    let percent = seconds / (length / 100.0);
    percent.is_finite().then(|| percent.clamp(0.0, 100.0))
}

#[derive(Debug, Clone, PartialEq)]
pub enum DeeEvent {
    /// Measured loudness, rounded and clamped to -31..0.
    Measured(i64),
    /// Overall progress of the tool, 0 to 100.
    Progress(f64),
    /// A new step began (`Step 2/2 : Encoding`).
    Step(Stage),
    /// The message of an error line.
    Error(String),
    /// The message of a warning line.
    Warning(String),
}

/// Interprets the lines printed by the DEE tools (also through dee_shim,
/// which can glue its own lines in front of one of DEE).
#[derive(Debug, Clone, Default)]
pub struct DeeParser {
    measured: bool,
    /// DEE repeats errors and warnings at the end.
    seen: Vec<String>,
}

impl DeeParser {
    pub fn new() -> DeeParser {
        DeeParser::default()
    }

    fn new_message(&mut self, message: &str) -> bool {
        let message = message.trim().to_string();
        if message.is_empty() || self.seen.contains(&message) {
            return false;
        }
        self.seen.push(message);
        true
    }

    pub fn feed(&mut self, line: &str) -> Vec<DeeEvent> {
        let mut events = Vec::new();
        if let Some(c) = DEE_STEP.captures(line) {
            let name = c[3].to_lowercase();
            if name.contains("measurement") {
                events.push(DeeEvent::Step(Stage::Measure));
            } else if name.contains("encoding") {
                events.push(DeeEvent::Step(Stage::Encode));
            }
        }
        if !self.measured
            && let Some(value) = DEE_LOUDNESS
                .captures(line)
                .and_then(|c| c[1].parse::<f64>().ok())
                .filter(|v| v.is_finite())
        {
            self.measured = true;
            events.push(DeeEvent::Measured(clamp_i(
                value.round_ties_even() as i64,
                -31,
                0,
            )));
        }
        if let Some(c) = DEE_PROGRESS.captures(line)
            && let Ok(value) = c[1].parse::<f64>()
        {
            events.push(DeeEvent::Progress(value.min(100.0)));
        }
        if let Some(c) = DEE_ERROR.captures(line)
            && self.new_message(&c[1])
        {
            events.push(DeeEvent::Error(c[1].trim().to_string()));
        }
        if let Some(c) = DEE_WARNING.captures(line)
            && self.new_message(&c[1])
        {
            events.push(DeeEvent::Warning(c[1].trim().to_string()));
        }
        events
    }
}

impl DeeParser {
    /// Whether the parser takes the line as progress, stage, measurement,
    /// error or warning output, whatever it makes of it.
    pub fn recognizes(line: &str) -> bool {
        DEE_STEP.is_match(line)
            || DEE_LOUDNESS.is_match(line)
            || DEE_PROGRESS.is_match(line)
            || DEE_ERROR.is_match(line)
            || DEE_WARNING.is_match(line)
    }
}

/// An ffmpeg log line without its `[out#0/wav @ 0x5da79dccbe40] ` prefix.
pub fn strip_ffmpeg_prefix(line: &str) -> &str {
    let line = line.trim();
    match FFMPEG_PREFIX.find(line) {
        Some(m) => &line[m.end()..],
        None => line,
    }
}

/// The detail of a failed ffmpeg: the last line that says "error", else the
/// last message (not progress), without ffmpeg's prefix. A full disk names
/// the `temp` path, where ffmpeg writes.
pub fn ffmpeg_detail(last_error: Option<&str>, last_message: Option<&str>, temp: &str) -> String {
    let detail = strip_ffmpeg_prefix(last_error.or(last_message).unwrap_or_default());
    if detail.contains("No space left on device") {
        format!("{detail} (temp path: {temp})")
    } else {
        detail.to_string()
    }
}

/// Whether a line of ffmpeg's log is the last error candidate.
fn is_ffmpeg_error(line: &str) -> bool {
    line.to_lowercase().contains("error")
}

/// The directory of the job's intermediate file, where ffmpeg writes.
fn temp_of(job: &Job) -> String {
    Path::new(&job.intermediate)
        .parent()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Call `f` for every line of `reader`, where both `\r` and `\n` end a line
/// (ffmpeg and DEE end their statistics with `\r`). Invalid UTF-8 is replaced.
pub fn for_each_line<R: Read>(mut reader: R, mut f: impl FnMut(&str)) {
    let mut buf = [0u8; 4096];
    let mut line: Vec<u8> = Vec::new();
    loop {
        let n = match reader.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        for &byte in &buf[..n] {
            if byte == b'\r' || byte == b'\n' {
                if !line.is_empty() {
                    f(&String::from_utf8_lossy(&line));
                    line.clear();
                }
            } else {
                line.push(byte);
            }
        }
    }
    if !line.is_empty() {
        f(&String::from_utf8_lossy(&line));
    }
}

/// Channels (bed channels plus objects) of the decode from a truehdd log line.
pub fn parse_atmos_channels(line: &str) -> Option<usize> {
    let c = ATMOS_CHANNELS.captures(line)?;
    Some(c[1].parse::<usize>().ok()? + c[2].parse::<usize>().ok()?)
}

/// Samples per TrueHD frame at 48 kHz.
const SAMPLES_PER_FRAME: f64 = 40.0;

/// Frames of the stream from the log of truehdd (`Found 24000 extractable
/// frames in ...`), which it counts before it decodes.
pub fn parse_atmos_frames(line: &str) -> Option<u64> {
    ATMOS_FRAMES.captures(line)?[1].parse().ok()
}

/// The message of an error in the (JSON) log of truehdd.
fn truehdd_error(line: &str) -> Option<String> {
    let value: Value = serde_json::from_str(line).ok()?;
    let level = value.get("lvl")?.as_str()?;
    (level.eq_ignore_ascii_case("error")).then(|| value.get("msg")?.as_str().map(str::to_string))?
}

/// Whether the `--json` result of truehdd lists the Atmos presentation.
pub fn has_atmos_presentation(result: &str) -> bool {
    let Ok(value) = serde_json::from_str::<Value>(result) else {
        return false;
    };
    value
        .get("presentations")
        .and_then(Value::as_array)
        .is_some_and(|list| {
            list.iter().any(|p| {
                p.get("index").and_then(Value::as_i64) == Some(3)
                    && p.get("format").and_then(Value::as_str) == Some("damf")
            })
        })
}

/// Progress (0 to 100) of the decode from the size of the `.atmos.audio`
/// file: 24-bit samples of all channels. `frames` is what truehdd counted,
/// `duration` (seconds, -1 if unknown) what ffprobe reported at 48 kHz.
/// `None` as long as the total is not known.
pub fn decode_percent(size: u64, frames: u64, duration: f64, channels: usize) -> Option<f64> {
    let samples = if frames > 0 {
        frames as f64 * SAMPLES_PER_FRAME
    } else if duration > 0.0 {
        duration * 48000.0
    } else {
        return None;
    };
    if channels == 0 {
        return None;
    }
    let expected = samples * channels as f64 * 3.0;
    Some((size as f64 / expected * 100.0).clamp(0.0, 99.0))
}

/// Check the `--json` result of truehdd against the input: it has to have
/// decoded frames, and as much audio as the input has (when its duration is
/// known, seconds, -1 otherwise). A mismatch means truehdd read garbage.
pub fn check_decoded(result: &str, length: f64) -> Result<(), String> {
    let value: Value = serde_json::from_str(result).map_err(|e| format!("truehdd: {e}"))?;
    if value.get("frames").and_then(Value::as_u64) == Some(0) {
        return Err("truehdd decoded no frames.".to_string());
    }
    let samples = value.get("samples").and_then(Value::as_f64);
    let rate = value.get("sampleRate").and_then(Value::as_f64);
    if let (Some(samples), Some(rate)) = (samples, rate)
        && rate > 0.0
        && length > 0.0
    {
        let decoded = samples / rate;
        if (decoded - length).abs() > 1.0 {
            return Err(format!(
                "truehdd decoded only {decoded:.1} s of {length:.1} s."
            ));
        }
    }
    Ok(())
}

/// `manifest` (the output of `dee_measure_loudness`) with the dialnorm as
/// its loudness.
pub fn manifest_with_dialnorm(manifest: &str, dialnorm: i64) -> Result<String, String> {
    let mut value: Value =
        serde_json::from_str(manifest).map_err(|e| format!("invalid loudness manifest: {e}"))?;
    let loudness = value
        .pointer_mut("/loudness_manifest/tracks/0/channel_based_loudness")
        .and_then(Value::as_object_mut)
        .filter(|entries| !entries.is_empty())
        .ok_or("unexpected loudness manifest")?;
    for entry in loudness.values_mut() {
        let entry = entry
            .as_object_mut()
            .ok_or("unexpected loudness manifest")?;
        for key in ["integrated_loudness", "integrated_speech_gated_loudness"] {
            entry.insert(key.to_string(), json!(dialnorm as f64));
        }
    }
    serde_json::to_string_pretty(&value).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Files

/// Paths DEE writes next to a TrueHD output that are not wanted: both
/// the names with and without the DELAY part, `.thd.log.html` (DEE 6.7),
/// `.thd.log` and `.thd.mll`.
pub fn thd_leftovers(input: &str, output_dir: &str, keep_log: bool) -> Vec<String> {
    let dir = Path::new(output_dir);
    let mut paths = Vec::new();
    for ext in ["thd.log.html", "thd.log", "thd.mll"] {
        // `-kl` keeps DEE's html log
        if keep_log && ext == "thd.log.html" {
            continue;
        }
        for stripdelay in [true, false] {
            let name = basename(input, ext, false, false, stripdelay);
            let path = dir.join(name).to_string_lossy().into_owned();
            if !paths.contains(&path) {
                paths.push(path);
            }
        }
    }
    paths
}

/// Remove a file, which DEE may still hold for a moment on Windows. Returns
/// false if it could not be removed.
fn remove_file(path: &str) -> bool {
    let path = Path::new(path);
    let mut attempts = 0;
    while path.exists() && attempts < 10 {
        if std::fs::remove_file(path).is_err() {
            attempts += 1;
            std::thread::sleep(Duration::from_secs(1));
        }
    }
    !path.exists()
}

/// Remove DEE's directory of the job (whatever the outcome, also with `-k`),
/// retrying while Windows still holds a file.
fn remove_dee_temp(job: &Job, ui: &Ui) {
    let path = Path::new(&job.dee_temp);
    let mut attempts = 0;
    while path.exists() && attempts < 10 {
        if std::fs::remove_dir_all(path).is_err() {
            attempts += 1;
            std::thread::sleep(Duration::from_secs(1));
        }
    }
    if path.exists() {
        ui.say(&format!(
            "[bold yellow]Failed to delete:[/bold yellow] {}",
            escape(&job.dee_temp)
        ));
        events::warning(&format!("Failed to delete: {}", job.dee_temp));
    }
}

fn cleanup(job: &Job, options: &Options, ui: &Ui) {
    if !options.keep_temp {
        for path in &job.temp_files {
            if !remove_file(path) {
                ui.say(&format!(
                    "[bold yellow]Failed to delete:[/bold yellow] {}",
                    escape(path)
                ));
                events::warning(&format!("Failed to delete: {path}"));
            }
        }
    }
    if matches!(job.format, Format::Thd | Format::ThdAtmos) {
        for path in thd_leftovers(&job.input, &job.output_dir, options.keep_log) {
            let _ = std::fs::remove_file(path);
        }
    }
}

/// Remove what an interrupted job left behind: the temp files (also with `-k`,
/// a partial intermediate must not be taken for a finished one), and, once the
/// encoder had started, the partial output.
fn cleanup_interrupted(job: &Job, stages: &[StageLog], ui: &Ui) {
    let mut paths: Vec<&String> = job
        .temp_files
        .iter()
        // an intermediate from an earlier run was not touched
        .filter(|path| !(job.intermediate_exists && **path == job.intermediate))
        .collect();
    let encoding = stages.iter().any(|stage| stage.name == "encode");
    if encoding {
        paths.push(&job.output_path);
    }
    for path in paths {
        if !remove_file(path) {
            ui.say(&format!(
                "[bold yellow]Failed to delete:[/bold yellow] {}",
                escape(path)
            ));
            events::warning(&format!("Failed to delete: {path}"));
        }
    }
    if encoding && matches!(job.format, Format::Thd | Format::ThdAtmos) {
        for path in thd_leftovers(&job.input, &job.output_dir, false) {
            remove_file(&path);
        }
    }
}

// ---------------------------------------------------------------------------
// Processes

/// A running child process that is known to [`interrupt`], so a signal can
/// stop it.
struct Tracked(Child);

impl Deref for Tracked {
    type Target = Child;
    fn deref(&self) -> &Child {
        &self.0
    }
}

impl DerefMut for Tracked {
    fn deref_mut(&mut self) -> &mut Child {
        &mut self.0
    }
}

impl Tracked {
    fn wait(&mut self) -> std::io::Result<ExitStatus> {
        let status = self.0.wait();
        interrupt::unregister(self.0.id());
        status
    }

    fn try_wait(&mut self) -> std::io::Result<Option<ExitStatus>> {
        let status = self.0.try_wait();
        if matches!(status, Ok(Some(_))) {
            interrupt::unregister(self.0.id());
        }
        status
    }
}

impl Drop for Tracked {
    fn drop(&mut self) {
        interrupt::unregister(self.0.id());
    }
}

fn spawn(args: &[String], stdout: Stdio, stderr: Stdio) -> Result<Tracked, String> {
    spawn_with_stdin(args, Stdio::null(), stdout, stderr)
}

fn spawn_with_stdin(
    args: &[String],
    stdin: Stdio,
    stdout: Stdio,
    stderr: Stdio,
) -> Result<Tracked, String> {
    if interrupt::is_interrupted() {
        return Err("interrupted".to_string());
    }
    let mut command = Command::new(&args[0]);
    command
        .args(&args[1..])
        .stdin(stdin)
        .stdout(stdout)
        .stderr(stderr);
    if is_shim(&args[0]) {
        for name in shim_env_names(std::env::vars_os().map(|(name, _)| name)) {
            command.env_remove(name);
        }
    }
    interrupt::isolate(&mut command);
    let child = command
        .spawn()
        .map_err(|e| format!("failed to run {}: {e}", args[0]))?;
    interrupt::register(&child);
    Ok(Tracked(child))
}

/// The line of a failed tool's stderr that tells the reason: the last one,
/// but dee_shim's usage errors (clap) are `error: <message>` followed by
/// hints and a usage block, so its first `error:` line is the message.
fn stderr_detail(lines: &[String], shim: bool) -> Option<String> {
    if shim
        && let Some(message) = lines
            .iter()
            .find_map(|line| {
                let line = line.trim();
                let prefix = line.get(..6)?;
                prefix
                    .eq_ignore_ascii_case("error:")
                    .then(|| line[6..].trim().to_string())
            })
            .filter(|m| !m.is_empty())
    {
        return Some(message);
    }
    lines.last().cloned()
}

/// Whether the program is `dee_shim` (with or without `.exe`).
fn is_shim(program: &str) -> bool {
    let name = program
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or_default()
        .to_lowercase();
    name == "dee_shim" || name == "dee_shim.exe"
}

/// The environment variables dee_shim must not inherit: its logging
/// settings (they apply when the command line has no matching option) and
/// its internal `DEE_SHIM_*` names.
fn shim_env_names(names: impl Iterator<Item = std::ffi::OsString>) -> Vec<std::ffi::OsString> {
    names
        .filter(|name| {
            let upper = name.to_string_lossy().to_uppercase();
            matches!(
                upper.as_str(),
                "DSHIM_LOG" | "DSHIM_LOG_LEVEL" | "DSHIM_LOG_FORMAT" | "RUST_LOG"
            ) || upper.starts_with("DEE_SHIM_")
        })
        .collect()
}

/// `<tool> failed (exit code N): <detail>`
pub fn failure_message(tool: &str, status: ExitStatus, detail: Option<&str>) -> String {
    let code = status.code().map_or_else(
        || "terminated by a signal".to_string(),
        |c| format!("exit code {c}"),
    );
    match detail.map(str::trim).filter(|d| !d.is_empty()) {
        Some(detail) => format!("{tool} failed ({code}): {detail}"),
        None => format!("{tool} failed ({code})"),
    }
}

/// Read the lines of `reader` on another thread (without colors, progress
/// collapsed, see [`Stream`]).
fn drain_stream<R: Read + Send + 'static>(reader: R, label: &str) -> JoinHandle<Stream> {
    let label = label.to_string();
    std::thread::spawn(move || {
        let mut stream = Stream::new(&label);
        for_each_line(reader, |line| stream.push(line));
        stream
    })
}

fn joined(handle: JoinHandle<Stream>) -> Stream {
    handle.join().unwrap_or_default()
}

/// ffmpeg: converts the input to the intermediate wav.
fn run_ffmpeg(job: &Job, bar: &Bar, log: &mut StageLog) -> Result<(), String> {
    let mut child = spawn(&job.ffmpeg_args, Stdio::null(), Stdio::piped())?;
    let stderr = child.stderr.take().expect("piped stderr");
    bar.describe(Stage::Ffmpeg, None);
    let mut last_message = String::new();
    let mut last_error: Option<String> = None;
    if job.length == -1.0 {
        bar.indeterminate();
    } else {
        bar.percent(0.0);
    }
    let captured = log.stream("stderr");
    for_each_line(stderr, |line| {
        log.line(captured, line);
        if line.contains("time=") {
            if job.length != -1.0
                && let Some(percent) = ffmpeg_percent(line, job.length)
            {
                bar.percent(percent);
            }
        } else {
            last_message = line.trim().to_string();
            if is_ffmpeg_error(line) {
                last_error = Some(last_message.clone());
            }
        }
    });
    let status = child.wait().map_err(|e| format!("ffmpeg: {e}"))?;
    log.exit = status.code();
    if !status.success() {
        let detail = ffmpeg_detail(last_error.as_deref(), Some(&last_message), &temp_of(job));
        return Err(failure_message("ffmpeg", status, Some(&detail)));
    }
    bar.percent(100.0);
    std::thread::sleep(Duration::from_millis(500));
    Ok(())
}

/// ffmpeg | truehdd: decodes the Atmos presentation of the input.
fn run_decode(job: &Job, bar: &Bar, log: &mut StageLog) -> Result<(), String> {
    // ffmpeg copies the TrueHD track (of a container or a raw stream) into
    // the stdin of truehdd, which reads raw streams only.
    let mut ffmpeg = if job.ffmpeg_args.is_empty() {
        None
    } else {
        Some(spawn(&job.ffmpeg_args, Stdio::piped(), Stdio::piped())?)
    };
    let (stdin, ffmpeg_log) = match ffmpeg.as_mut() {
        Some(ff) => (
            Stdio::from(ff.stdout.take().expect("piped stdout")),
            Some(drain_stream(
                ff.stderr.take().expect("piped stderr"),
                "ffmpeg stderr",
            )),
        ),
        None => (Stdio::null(), None),
    };
    let mut child = match spawn_with_stdin(&job.decode_args, stdin, Stdio::piped(), Stdio::piped())
    {
        Ok(child) => child,
        Err(e) => {
            if let Some(ff) = ffmpeg.as_mut() {
                let _ = ff.kill();
                let _ = ff.wait();
            }
            return Err(e);
        }
    };
    let mut stdout = child.stdout.take().expect("piped stdout");
    let stderr = child.stderr.take().expect("piped stderr");
    bar.describe(Stage::Decode, None);
    // The total is known from the start with a duration, else as soon as
    // truehdd has counted the frames.
    if job.length > 0.0 {
        bar.percent(0.0);
    } else {
        bar.indeterminate();
    }

    let channels = Arc::new(AtomicUsize::new(0));
    let frames = Arc::new(AtomicU64::new(0));
    let errors = Arc::new(Mutex::new(Vec::<String>::new()));
    let truehdd_log = {
        let (channels, frames, errors) = (channels.clone(), frames.clone(), errors.clone());
        std::thread::spawn(move || {
            let mut stream = Stream::new("truehdd stderr");
            for_each_line(stderr, |line| {
                stream.push(line);
                if let Some(count) = parse_atmos_channels(line) {
                    channels.store(count, Ordering::SeqCst);
                }
                if let Some(count) = parse_atmos_frames(line) {
                    frames.store(count, Ordering::SeqCst);
                }
                if let Some(message) = truehdd_error(line) {
                    errors.lock().expect("errors").push(message);
                }
            });
            stream
        })
    };
    let result = std::thread::spawn(move || {
        let mut text = String::new();
        let _ = stdout.read_to_string(&mut text);
        text
    });

    // truehdd has no progress output when piped, the size of the decoded
    // audio tells how far it is.
    let audio = format!("{}.audio", job.intermediate);
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                let size = std::fs::metadata(&audio).map_or(0, |m| m.len());
                if let Some(percent) = decode_percent(
                    size,
                    frames.load(Ordering::SeqCst),
                    job.length,
                    channels.load(Ordering::SeqCst),
                ) {
                    bar.percent(percent);
                }
                std::thread::sleep(Duration::from_millis(250));
            }
            Err(e) => {
                if let Some(ff) = ffmpeg.as_mut() {
                    let _ = ff.kill();
                    let _ = ff.wait();
                }
                return Err(format!("truehdd: {e}"));
            }
        }
    };
    let truehdd_log = truehdd_log.join().unwrap_or_default();
    let result = result.join().unwrap_or_default();
    log.exit = status.code();

    // If truehdd gave up, ffmpeg would hang on the closed pipe.
    let mut ffmpeg_failure = None;
    if let Some(ff) = ffmpeg.as_mut() {
        let killed = !status.success();
        if killed {
            let _ = ff.kill();
        }
        let ffmpeg_status = ff.wait().map_err(|e| format!("ffmpeg: {e}"))?;
        let mut ffmpeg_stream = ffmpeg_log.map(joined).unwrap_or_default();
        ffmpeg_stream.push(&match ffmpeg_status.code() {
            Some(code) => format!("exit code {code}{}", if killed { " (killed)" } else { "" }),
            None => "terminated by a signal".to_string(),
        });
        if !killed && !ffmpeg_status.success() {
            // the last line is the exit code just pushed
            let logged = &ffmpeg_stream.lines[..ffmpeg_stream.lines.len().saturating_sub(1)];
            let detail = ffmpeg_detail(
                logged
                    .iter()
                    .rev()
                    .map(String::as_str)
                    .find(|l| is_ffmpeg_error(l)),
                logged.last().map(String::as_str),
                &temp_of(job),
            );
            ffmpeg_failure = Some(failure_message("ffmpeg", ffmpeg_status, Some(&detail)));
        }
        log.streams.push(ffmpeg_stream);
    }
    log.streams.push(truehdd_log);
    let mut stdout_stream = Stream::new("truehdd stdout");
    stdout_stream.push(&result);
    log.streams.push(stdout_stream);
    if let Some(message) = ffmpeg_failure {
        return Err(message);
    }
    if !status.success() {
        let errors = errors.lock().expect("errors");
        return Err(failure_message(
            "truehdd",
            status,
            errors.last().map(String::as_str),
        ));
    }
    if !has_atmos_presentation(&result) {
        return Err("the input has no Atmos presentation.".to_string());
    }
    check_decoded(&result, job.length)?;
    if !Path::new(&job.intermediate).exists() {
        return Err("truehdd failed: no .atmos file was written.".to_string());
    }
    bar.percent(100.0);
    Ok(())
}

/// One run of a DEE tool.
struct Phase<'a> {
    args: &'a [String],
    /// The stage until the tool says otherwise.
    stage: Stage,
    /// Where the first of two steps ends.
    first: FirstStep,
}

/// Run a DEE tool (also through dee_shim) and follow its output.
fn run_dee(phase: &Phase, bar: &Bar, ui: &Ui, log: &mut StageLog) -> Result<(), String> {
    let tool = file_name(&phase.args[0]);
    let mut child = spawn(phase.args, Stdio::piped(), Stdio::piped())?;
    let stdout = child.stdout.take().expect("piped stdout");
    let stderr = drain_stream(child.stderr.take().expect("piped stderr"), "stderr");
    let mut stage = phase.stage;
    let mut db = None;
    let mut errors: Vec<String> = Vec::new();
    // the last line of stdout that is none of the output the parser knows
    let mut last_other: Option<String> = None;
    bar.describe(stage, db);
    let mut map = StepMap::new(stage, phase.first);
    show(bar, map.initial());
    let mut parser = DeeParser::new();
    let captured = log.stream("stdout");
    for_each_line(stdout, |line| {
        log.line(captured, line);
        let events = parser.feed(line);
        if events.is_empty() && !DeeParser::recognizes(line) {
            let plain = crate::deewlog::strip_ansi(line);
            if !plain.trim().is_empty() {
                last_other = Some(plain.trim().to_string());
            }
        }
        for event in events {
            match event {
                DeeEvent::Step(next) => {
                    stage = next;
                    bar.describe(stage, db);
                    show(bar, map.step(stage));
                }
                DeeEvent::Measured(measured) => {
                    db = Some(measured);
                    log.measured.get_or_insert(measured);
                    bar.describe(stage, db);
                }
                DeeEvent::Progress(percent) => show(bar, map.progress(percent)),
                DeeEvent::Error(message) => errors.push(message),
                DeeEvent::Warning(message) => {
                    log.warnings.push(message.clone());
                    events::warning(&message);
                    ui.say(&format!(
                        "[bold yellow]WARNING:[/bold yellow] {}",
                        escape(&message)
                    ));
                }
            }
        }
    });
    let status = child.wait().map_err(|e| format!("{tool}: {e}"))?;
    let stderr = joined(stderr);
    log.exit = status.code();
    let detail = errors
        .last()
        .cloned()
        .or_else(|| stderr_detail(&stderr.lines, is_shim(&phase.args[0])))
        .or(last_other);
    log.streams.push(stderr);
    if !status.success() {
        return Err(failure_message(tool, status, detail.as_deref()));
    }
    bar.percent(100.0);
    Ok(())
}

/// Set the custom dialnorm in the manifest the measurement wrote.
fn apply_dialnorm(path: &str, dialnorm: i64) -> Result<(), String> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("failed to read the loudness manifest {path}: {e}"))?;
    let text = manifest_with_dialnorm(&text, dialnorm)?;
    std::fs::write(path, text).map_err(|e| format!("failed to write {path}: {e}"))
}

/// Run one stage and keep its log, also when it failed.
fn staged(
    stages: &mut Vec<StageLog>,
    name: &'static str,
    commands: &[&[String]],
    enabled: bool,
    run: impl FnOnce(&mut StageLog) -> Result<(), String>,
) -> Result<(), String> {
    let mut stage = StageLog::new(name, commands, enabled);
    let result = run(&mut stage);
    stage.finish();
    stages.push(stage);
    result
}

fn run_stages(job: &Job, bar: &Bar, ui: &Ui, stages: &mut Vec<StageLog>) -> Result<(), String> {
    let on = job.log.is_some();
    if !job.intermediate_exists {
        if job.format.is_atmos() {
            let mut commands = vec![&job.decode_args[..]];
            if !job.ffmpeg_args.is_empty() {
                commands.insert(0, &job.ffmpeg_args[..]);
            }
            staged(stages, "decode", &commands, on, |log| {
                run_decode(job, bar, log)
            })?;
        } else {
            staged(stages, "ffmpeg", &[&job.ffmpeg_args], on, |log| {
                run_ffmpeg(job, bar, log)
            })?;
        }
    }
    let measuring = !job.measure_args.is_empty();
    let encoding = !job.encode_args.is_empty();
    if measuring {
        // A step of its own, 0 to 100 %.
        let phase = Phase {
            args: &job.measure_args,
            stage: Stage::Measure,
            first: FirstStep::EndsAt(100.0),
        };
        staged(stages, "measure", &[&job.measure_args], on, |log| {
            run_dee(&phase, bar, ui, log)
        })?;
        if job.dialnorm != 0 {
            apply_dialnorm(&job.loudness_path, job.dialnorm)?;
        }
    }
    if encoding {
        // DEE only measures itself when it got no loudness manifest, and
        // AAC and TrueHD are not measured at all.
        let measures = !measuring && !matches!(job.format, Format::Aac | Format::Thd);
        let phase = Phase {
            args: &job.encode_args,
            stage: if measures {
                Stage::Measure
            } else {
                Stage::Encode
            },
            first: FirstStep::of_format(job.format),
        };
        staged(stages, "encode", &[&job.encode_args], on, |log| {
            run_dee(&phase, bar, ui, log)
        })?;
        if !Path::new(&job.output_path).exists() {
            return Err(format!(
                "{} failed: the output file {} was not written.",
                file_name(&job.encode_args[0]),
                job.output_path
            ));
        }
    }
    bar.percent(100.0);
    Ok(())
}

/// Write the report of a job.
fn write_log(
    job: &Job,
    spec: &LogSpec,
    started: SystemTime,
    stages: Vec<StageLog>,
    result: &Result<(), String>,
    interrupted: bool,
    ui: &Ui,
) {
    let mut output = Vec::new();
    if let Some(db) = stages.iter().find_map(|s| s.measured) {
        output.push(("Measured loudness".to_string(), format!("{db} dB")));
    }
    if !job.encode_args.is_empty()
        && !interrupted
        && let Ok(meta) = std::fs::metadata(&job.output_path)
    {
        output.insert(0, ("Path".to_string(), job.output_path.clone()));
        output.insert(1, ("Size".to_string(), deewlog::format_size(meta.len())));
    }
    let warnings = stages.iter().flat_map(|s| s.warnings.clone()).collect();
    let data = LogData {
        file_name: file_name(&job.input).to_string(),
        ok: result.is_ok(),
        interrupted,
        started,
        duration: started.elapsed().unwrap_or_default().as_secs_f64(),
        deew_version: env!("CARGO_PKG_VERSION").to_string(),
        versions: spec.versions.clone(),
        input: spec.input.clone(),
        settings: spec.settings.clone(),
        stages,
        output,
        notices: spec.notices.clone(),
        warnings,
        errors: if interrupted {
            vec!["interrupted".to_string()]
        } else {
            result.as_ref().err().cloned().into_iter().collect()
        },
    };
    if let Err(e) = std::fs::write(&spec.path, deewlog::render(&data)) {
        ui.say(&format!(
            "[bold yellow]Failed to write:[/bold yellow] {}",
            escape(&format!("{}: {e}", spec.path))
        ));
        events::warning(&format!("Failed to write {}: {e}", spec.path));
    }
}

/// How a job ended.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    Done,
    /// The message of the failed step.
    Failed(String),
    /// Stopped by Ctrl-C; its partial files are removed.
    Interrupted,
}

/// Encode one input.
pub fn run_job(job: &Job, options: &Options, bar: &Bar, ui: &Ui) -> Outcome {
    let started = SystemTime::now();
    bar.describe(Stage::Starting, None);
    let mut stages = Vec::new();
    if job.encode_args.iter().any(|a| a == "--temp-dir") {
        let _ = std::fs::create_dir_all(&job.dee_temp);
    }
    let result = run_stages(job, bar, ui, &mut stages);
    remove_dee_temp(job, ui);
    // a job that finished before the signal came is done
    let interrupted = result.is_err() && interrupt::is_interrupted();
    if interrupted {
        cleanup_interrupted(job, &stages, ui);
    } else {
        cleanup(job, options, ui);
    }
    if let Some(spec) = job.log.as_ref().filter(|_| options.keep_log) {
        write_log(job, spec, started, stages, &result, interrupted, ui);
    }
    match result {
        Ok(()) => Outcome::Done,
        Err(_) if interrupted => Outcome::Interrupted,
        Err(message) => Outcome::Failed(message),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stamps() {
        assert_eq!(stamp_to_sec("00:00:02.00"), Some(2.0));
        assert_eq!(stamp_to_sec("01:02:03.50"), Some(3723.5));
        assert_eq!(stamp_to_sec("N/A"), None);
        assert_eq!(stamp_to_sec("00:00"), None);
        assert_eq!(stamp_to_sec("00:00:x"), None);
        // Python: int('-00') is 0
        assert_eq!(stamp_to_sec("-00:00:00.03"), Some(0.03));
    }

    #[test]
    fn ffmpeg_progress_lines() {
        let line = "size=     256kB time=00:00:01.00 bitrate=2097.2kbits/s speed= 100x";
        assert_eq!(ffmpeg_percent(line, 2.0), Some(50.0));
        // above 100 is clamped
        assert_eq!(ffmpeg_percent(line, 0.5), Some(100.0));
        assert_eq!(ffmpeg_percent("size=N/A time=N/A bitrate=N/A", 2.0), None);
        assert_eq!(ffmpeg_percent("Press [q] to stop", 2.0), None);
        assert_eq!(ffmpeg_percent("time 00:00:01.00", 2.0), None);
        assert_eq!(ffmpeg_percent("x=1 time=00:00:00.50", 2.0), Some(25.0));
    }

    fn events(parser: &mut DeeParser, lines: &[&str]) -> Vec<DeeEvent> {
        lines.iter().flat_map(|l| parser.feed(l)).collect()
    }

    #[test]
    fn dee_run_with_measurement() {
        let mut p = DeeParser::new();
        let got = events(
            &mut p,
            &[
                "[2026-11-01 22:01:24.690] Step 1/2 : Loudness Measurement",
                "[>                                       ] 0 %",
                "[=>                                      ] 3 %",
                "[====================>                   ] 50 %",
                "[2026-11-01 22:01:24.838] [Source loudness] measured_loudness=-24.1",
                "[2026-11-01 22:01:24.838] [Source loudness] gating=dialogue",
                "[2026-11-01 22:01:24.900] Step 2/2 : Encoding",
                "[=====================>                  ] 52 %",
                "[========================================>] 100 %",
            ],
        );
        assert_eq!(
            got,
            [
                DeeEvent::Step(Stage::Measure),
                DeeEvent::Progress(0.0),
                DeeEvent::Progress(3.0),
                DeeEvent::Progress(50.0),
                DeeEvent::Measured(-24),
                DeeEvent::Step(Stage::Encode),
                DeeEvent::Progress(52.0),
                DeeEvent::Progress(100.0),
            ]
        );
    }

    #[test]
    fn dshim_lines_can_be_glued_in_front() {
        let mut p = DeeParser::new();
        assert_eq!(
            p.feed("dshim ok   cluster: patch @ 0x1521fb0[=====>     ] 12 %"),
            [DeeEvent::Progress(12.0)]
        );
        assert_eq!(
            p.feed("dshim info loaded[2026-11-01 22:07:32.228] Step 1/2 : Loudness Measurement"),
            [DeeEvent::Step(Stage::Measure)]
        );
        assert_eq!(p.feed("dshim info only"), []);
        assert_eq!(p.feed("Step 3/3 : Something else"), []);
    }

    #[test]
    fn measured_loudness_forms() {
        let m = |line: &str| DeeParser::new().feed(line);
        assert_eq!(m("measured_loudness=-23.4"), [DeeEvent::Measured(-23)]);
        assert_eq!(m("measured_loudness=-23.6"), [DeeEvent::Measured(-24)]);
        assert_eq!(m("measured_loudness=-40"), [DeeEvent::Measured(-31)]);
        assert_eq!(m("measured_loudness=3.2"), [DeeEvent::Measured(0)]);
        assert_eq!(
            m("[Source loudness] measured_loudness=-27.0"),
            [DeeEvent::Measured(-27)]
        );
        // rounds half to even like Python
        assert_eq!(m("measured_loudness=-22.5"), [DeeEvent::Measured(-22)]);
        assert_eq!(m("measured_loudness=-23.5"), [DeeEvent::Measured(-24)]);
        assert_eq!(m("measured_loudness"), []);
        assert_eq!(m("measured_loudness=abc"), []);
    }

    #[test]
    fn measurement_is_only_taken_once() {
        let mut p = DeeParser::new();
        assert_eq!(p.feed("measured_loudness=-20"), [DeeEvent::Measured(-20)]);
        assert_eq!(p.feed("measured_loudness=-25"), []);
    }

    #[test]
    fn errors_and_warnings_are_reported_once() {
        let mut p = DeeParser::new();
        assert_eq!(
            p.feed("[2026-11-01 22:11:54.300] ERROR: File \"/x.wav\" does not exist."),
            [DeeEvent::Error("File \"/x.wav\" does not exist.".into())]
        );
        // the summary at the end of the output repeats it
        assert_eq!(p.feed("---- Summary of errors and warnings ----"), []);
        assert_eq!(p.feed("ERROR: File \"/x.wav\" does not exist."), []);
        assert_eq!(
            p.feed("[2026] WARNING: Input is clipping  "),
            [DeeEvent::Warning("Input is clipping".into())]
        );
        assert_eq!(p.feed("WARNING: Input is clipping"), []);
        assert_eq!(p.feed("error without the marker"), []);
        assert_eq!(p.feed("Exit code: 1"), []);
    }

    #[test]
    fn lines_split_on_cr_and_lf() {
        let mut got = Vec::new();
        for_each_line(&b"a=1\rb=2\r\nc\n\nd"[..], |l| got.push(l.to_string()));
        assert_eq!(got, ["a=1", "b=2", "c", "d"]);
        let mut got = Vec::new();
        for_each_line(&b"\xff\xfeok"[..], |l| got.push(l.to_string()));
        assert_eq!(got, ["\u{fffd}\u{fffd}ok"]);
    }

    #[test]
    fn atmos_channels_from_the_log() {
        let line = r#"{"lvl":"INFO","msg":"Creating audio file: /t/a.atmos.audio (1 bed channel + 11 objects)"}"#;
        assert_eq!(parse_atmos_channels(line), Some(12));
        assert_eq!(
            parse_atmos_channels("(10 bed channels + 1 object)"),
            Some(11)
        );
        assert_eq!(
            parse_atmos_channels("Decoding completed successfully"),
            None
        );
    }

    #[test]
    fn atmos_presentation_in_the_result() {
        let ok =
            r#"{"version":"0.6.2","presentations":[{"index":3,"format":"damf","channels":12}]}"#;
        assert!(has_atmos_presentation(ok));
        let other = r#"{"presentations":[{"index":2,"format":"pcm"}]}"#;
        assert!(!has_atmos_presentation(other));
        assert!(!has_atmos_presentation(r#"{"presentations":[]}"#));
        assert!(!has_atmos_presentation(
            r#"{"presentations":[{"index":3,"format":"caf"}]}"#
        ));
        assert!(!has_atmos_presentation(""));
    }

    #[test]
    fn truehdd_errors() {
        assert_eq!(
            truehdd_error(r#"{"ts":"x","lvl":"ERROR","msg":"Write error: No such file"}"#)
                .as_deref(),
            Some("Write error: No such file")
        );
        assert_eq!(truehdd_error(r#"{"lvl":"INFO","msg":"fine"}"#), None);
        assert_eq!(truehdd_error("not json"), None);
    }

    #[test]
    fn decode_progress_from_the_file_size() {
        // 20 s of 1 bed channel + 11 objects, 24 bit: 34.56 MB
        let total = 34_560_000;
        assert_eq!(decode_percent(total / 2, 0, 20.0, 12), Some(50.0));
        assert_eq!(decode_percent(0, 0, 20.0, 12), Some(0.0));
        // never full before truehdd exits
        assert_eq!(decode_percent(total * 2, 0, 20.0, 12), Some(99.0));
        // the frames truehdd counted win, a frame has 40 samples
        assert_eq!(decode_percent(total / 4, 24000, -1.0, 12), Some(25.0));
        assert_eq!(decode_percent(total / 4, 24000, 5.0, 12), Some(25.0));
        assert_eq!(decode_percent(100, 0, -1.0, 12), None);
        assert_eq!(decode_percent(100, 0, 20.0, 0), None);
    }

    #[test]
    fn frames_from_the_log() {
        let line = r#"{"lvl":"INFO","msg":"Found 24000 extractable frames in 0.221s (49.2 MB/s, 10892034 bytes)"}"#;
        assert_eq!(parse_atmos_frames(line), Some(24000));
        assert_eq!(
            parse_atmos_frames("Counting frames for progress estimation"),
            None
        );
    }

    #[test]
    fn decoded_audio_has_to_match_the_input() {
        let r = |frames: u64, samples: u64| {
            format!(r#"{{"frames":{frames},"samples":{samples},"sampleRate":48000}}"#)
        };
        assert_eq!(check_decoded(&r(24000, 960000), 20.0), Ok(()));
        assert_eq!(check_decoded(&r(24000, 960000), 20.9), Ok(()));
        assert_eq!(check_decoded(&r(24000, 960000), -1.0), Ok(()));
        assert_eq!(
            check_decoded(&r(215, 8600), 20.0),
            Err("truehdd decoded only 0.2 s of 20.0 s.".to_string())
        );
        assert!(check_decoded(&r(24000, 960000), 5.0).is_err());
        assert_eq!(
            check_decoded(&r(0, 0), -1.0),
            Err("truehdd decoded no frames.".to_string())
        );
        // fields that are missing cannot be checked
        assert_eq!(check_decoded(r#"{"presentations":[]}"#, 20.0), Ok(()));
        assert!(check_decoded("nope", 20.0).is_err());
    }

    #[test]
    fn dialnorm_goes_into_the_manifest() {
        let manifest = r#"{"loudness_manifest":{"version":"2.0.0","tracks":[{"id":"x",
            "source_sample_rate":48000,"channel_based_loudness":{"multichannel_5_1":{
            "integrated_loudness":-24.136349,"integrated_speech_gated_loudness":-24.136349,
            "integrated_level_gated_loudness":-22.8,"true_peaks":[-1.0]}}}]}}"#;
        let changed = manifest_with_dialnorm(manifest, -27).unwrap();
        let value: Value = serde_json::from_str(&changed).unwrap();
        let entry =
            &value["loudness_manifest"]["tracks"][0]["channel_based_loudness"]["multichannel_5_1"];
        assert_eq!(entry["integrated_loudness"], -27.0);
        assert_eq!(entry["integrated_speech_gated_loudness"], -27.0);
        // everything else stays
        assert_eq!(entry["integrated_level_gated_loudness"], -22.8);
        assert_eq!(entry["true_peaks"][0], -1.0);
        assert_eq!(value["loudness_manifest"]["tracks"][0]["id"], "x");
        // as a float
        assert!(changed.contains("-27.0"), "{changed}");

        assert!(manifest_with_dialnorm("nope", -27).is_err());
        assert!(manifest_with_dialnorm("{}", -27).is_err());
        let empty = r#"{"loudness_manifest":{"tracks":[{"channel_based_loudness":{}}]}}"#;
        assert!(manifest_with_dialnorm(empty, -27).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn ffmpeg_details() {
        let log = [
            "[out#0/wav @ 0x5da79dccbe40] Error muxing a packet",
            "[out#0/wav @ 0x5da79dccbe40] Task finished with error: No space left on device",
            "[out#0/wav @ 0x5da79dccbe40] Terminating thread with error: No space left on device",
            "[out#0/wav @ 0x5da79dccbe40] Error writing trailer: No space left on device",
            "[out#0/wav @ 0x5da79dccbe40] Error closing file: No space left on device",
            "[out#0/wav @ 0x5da79dccbe40] video:0KiB audio:3348KiB subtitle:0KiB other streams:0KiB global headers:0KiB muxing overhead: unknown",
            "Conversion failed!",
        ];
        let error = log.iter().rev().find(|l| is_ffmpeg_error(l)).copied();
        assert_eq!(
            ffmpeg_detail(error, log.last().copied(), "/t"),
            "Error closing file: No space left on device (temp path: /t)"
        );
        // no error line: the last message
        assert_eq!(
            ffmpeg_detail(None, Some("Conversion failed!"), "/t"),
            "Conversion failed!"
        );
        assert_eq!(ffmpeg_detail(None, None, "/t"), "");
        assert_eq!(
            ffmpeg_detail(
                Some("[matroska,webm @ 0x5add0de56100] EBML header parsing failed Error"),
                None,
                "/t"
            ),
            "EBML header parsing failed Error"
        );
    }

    #[test]
    fn ffmpeg_prefixes() {
        assert_eq!(
            strip_ffmpeg_prefix("[out#0/wav @ 0x5da79dccbe40] Error closing file"),
            "Error closing file"
        );
        assert_eq!(
            strip_ffmpeg_prefix("[matroska,webm @ 0x5add0de56100] Invalid data"),
            "Invalid data"
        );
        assert_eq!(
            strip_ffmpeg_prefix("  Conversion failed!"),
            "Conversion failed!"
        );
        assert_eq!(strip_ffmpeg_prefix("[a:0] no address"), "[a:0] no address");
    }

    #[test]
    fn dee_output_is_recognized() {
        assert!(DeeParser::recognizes("Step 1/2 : Measurement"));
        assert!(DeeParser::recognizes("ERROR: x"));
        assert!(!DeeParser::recognizes("Could not build workflow."));
    }

    #[test]
    fn shim_runs_lose_their_logging_environment() {
        assert!(is_shim("/opt/deew/dee_shim"));
        assert!(is_shim("C:\\deew\\dee_shim.exe"));
        assert!(!is_shim("/opt/dee/dee_ddp_encoder"));
        let names = [
            "PATH",
            "HOME",
            "DSHIM_LOG",
            "DSHIM_LOG_LEVEL",
            "DSHIM_LOG_FORMAT",
            "RUST_LOG",
            "RUST_BACKTRACE",
            "DEE_SHIM_ANYTHING",
            "DEE_SHIMMER",
        ]
        .map(std::ffi::OsString::from);
        let removed: Vec<_> = shim_env_names(names.into_iter())
            .iter()
            .map(|n| n.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            removed,
            [
                "DSHIM_LOG",
                "DSHIM_LOG_LEVEL",
                "DSHIM_LOG_FORMAT",
                "RUST_LOG",
                "DEE_SHIM_ANYTHING"
            ]
        );
    }

    #[test]
    fn shim_usage_errors_show_their_message() {
        let lines: Vec<String> = [
            "error: unexpected argument '--bogus' found",
            "tip: to pass '--bogus' as a value, use '-- --bogus'",
            "Usage: dee_shim ddpjoc_encoder [OPTIONS] [-- <NATIVE>...]",
            "For more information, try '--help'.",
        ]
        .map(String::from)
        .to_vec();
        assert_eq!(
            stderr_detail(&lines, true).as_deref(),
            Some("unexpected argument '--bogus' found")
        );
        // other tools: the last line
        assert_eq!(
            stderr_detail(&lines, false).as_deref(),
            Some("For more information, try '--help'.")
        );
        assert_eq!(stderr_detail(&[], true), None);
        assert_eq!(
            stderr_detail(&["plain".to_string()], true).as_deref(),
            Some("plain")
        );
    }

    #[test]
    fn failure_messages() {
        use std::os::unix::process::ExitStatusExt;
        let exit = |code: i32| ExitStatus::from_raw(code << 8);
        assert_eq!(
            failure_message("dee_ddp_encoder", exit(1), Some("File does not exist.")),
            "dee_ddp_encoder failed (exit code 1): File does not exist."
        );
        assert_eq!(
            failure_message("truehdd", exit(3), Some("  ")),
            "truehdd failed (exit code 3)"
        );
        assert_eq!(
            failure_message("ffmpeg", exit(1), None),
            "ffmpeg failed (exit code 1)"
        );
        assert_eq!(
            failure_message("ffmpeg", ExitStatus::from_raw(9), None),
            "ffmpeg failed (terminated by a signal)"
        );
    }

    fn shown(map: &mut StepMap, p: f64) -> f64 {
        match map.progress(p) {
            Shown::Percent(v) => v,
            Shown::Pulse => panic!("pulsing at {p}"),
        }
    }

    #[test]
    fn dee_steps_split_at_50_percent() {
        let mut map = StepMap::new(Stage::Measure, FirstStep::of_format(Format::Ddp));
        assert_eq!(map.initial(), Shown::Percent(0.0));
        assert_eq!(shown(&mut map, 0.0), 0.0);
        assert_eq!(shown(&mut map, 12.0), 24.0);
        assert_eq!(shown(&mut map, 50.0), 100.0);
        assert_eq!(map.step(Stage::Encode), Shown::Percent(0.0));
        assert_eq!(shown(&mut map, 50.0), 0.0);
        assert_eq!(shown(&mut map, 75.0), 50.0);
        assert_eq!(shown(&mut map, 100.0), 100.0);
    }

    #[test]
    fn thd_atmos_splits_at_42_percent() {
        let mut map = StepMap::new(Stage::Encode, FirstStep::of_format(Format::ThdAtmos));
        assert_eq!(map.step(Stage::Measure), Shown::Percent(0.0));
        assert_eq!(shown(&mut map, 21.0), 50.0);
        assert_eq!(shown(&mut map, 42.0), 100.0);
        map.step(Stage::Encode);
        assert_eq!(shown(&mut map, 42.0), 0.0);
        assert_eq!(shown(&mut map, 71.0), 50.0);
    }

    #[test]
    fn the_second_step_starts_where_dee_switched() {
        // whatever the split, the percentage at the step line is the start
        let mut map = StepMap::new(Stage::Measure, FirstStep::EndsAt(50.0));
        shown(&mut map, 30.0);
        shown(&mut map, 64.0);
        map.step(Stage::Encode);
        assert_eq!(shown(&mut map, 64.0), 0.0);
        assert_eq!(shown(&mut map, 82.0), 50.0);
        assert_eq!(shown(&mut map, 100.0), 100.0);
        // a switch at 100 % cannot divide by zero
        let mut map = StepMap::new(Stage::Measure, FirstStep::EndsAt(50.0));
        shown(&mut map, 100.0);
        map.step(Stage::Encode);
        assert_eq!(shown(&mut map, 100.0), 100.0);
    }

    #[test]
    fn non_atmos_thd_pulses_while_preprocessing() {
        let mut map = StepMap::new(Stage::Encode, FirstStep::of_format(Format::Thd));
        assert_eq!(map.initial(), Shown::Percent(0.0));
        assert_eq!(map.step(Stage::Measure), Shown::Pulse);
        assert_eq!(map.progress(1.0), Shown::Pulse);
        assert_eq!(map.progress(2.0), Shown::Pulse);
        assert_eq!(map.step(Stage::Encode), Shown::Percent(0.0));
        assert_eq!(shown(&mut map, 2.0), 0.0);
        assert_eq!(shown(&mut map, 51.0), 50.0);
    }

    #[test]
    fn single_steps_run_from_0_to_100() {
        // dee_measure_loudness, the encoder with a manifest, AAC
        let mut map = StepMap::new(Stage::Measure, FirstStep::EndsAt(100.0));
        assert_eq!(shown(&mut map, 40.0), 40.0);
        let mut map = StepMap::new(Stage::Encode, FirstStep::of_format(Format::Ddp));
        assert_eq!(map.step(Stage::Encode), Shown::Percent(0.0));
        assert_eq!(shown(&mut map, 40.0), 40.0);
    }

    #[test]
    fn steps_never_go_backwards() {
        let mut map = StepMap::new(Stage::Measure, FirstStep::EndsAt(50.0));
        assert_eq!(shown(&mut map, 20.0), 40.0);
        assert_eq!(shown(&mut map, 10.0), 40.0);
        assert_eq!(shown(&mut map, 250.0), 100.0);
        assert_eq!(shown(&mut map, -5.0), 100.0);
        // and a new step starts again at 0
        map.step(Stage::Encode);
        assert_eq!(shown(&mut map, 100.0), 100.0);
        map.step(Stage::Encode);
        assert_eq!(shown(&mut map, 100.0), 100.0);
    }

    #[test]
    fn file_names() {
        assert_eq!(file_name("/a/b/c.mkv"), "c.mkv");
        assert_eq!(file_name("c.mkv"), "c.mkv");
        assert_eq!(file_name("dir/"), "");
    }

    #[test]
    fn keep_log_keeps_the_html_log_only() {
        let names = |keep| -> Vec<String> {
            thd_leftovers("in/a.mkv", "out", keep)
                .iter()
                .map(|p| file_name(p).to_string())
                .collect()
        };
        assert_eq!(names(false), ["a.thd.log.html", "a.thd.log", "a.thd.mll"]);
        assert_eq!(names(true), ["a.thd.log", "a.thd.mll"]);
    }

    #[test]
    fn thd_leftover_names() {
        let in_out = |names: &[&str]| -> Vec<String> {
            names
                .iter()
                .map(|n| Path::new("out").join(n).to_string_lossy().into_owned())
                .collect()
        };
        let list = thd_leftovers("in/a DELAY -5ms.mkv", "out", false);
        assert_eq!(
            list,
            in_out(&[
                "a.thd.log.html",
                "a DELAY -5ms.thd.log.html",
                "a.thd.log",
                "a DELAY -5ms.thd.log",
                "a.thd.mll",
                "a DELAY -5ms.thd.mll"
            ])
        );
        assert_eq!(
            thd_leftovers("in/a.mkv", "out", false),
            in_out(&["a.thd.log.html", "a.thd.log", "a.thd.mll"])
        );
    }
}
