//! Running one input through ffmpeg and DEE, with progress reporting.

use crate::error::escape;
use crate::events;
use crate::plan::Format;
use crate::ui::{Bar, DESCRIPTION_WIDTH, Stage, Ui};
use crate::util::{basename, clamp_i, file_name, trim_names, version_ge};
use regex::Regex;
use std::io::Read;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::LazyLock;
use std::time::Duration;

/// Everything needed to encode one input.
#[derive(Debug, Clone)]
pub struct Job {
    pub input: String,
    /// Duration in seconds, -1 if unknown.
    pub length: f64,
    pub ffmpeg_args: Vec<String>,
    pub dee_args: Vec<String>,
    pub intermediate_exists: bool,
    pub format: Format,
    pub wav_path: String,
    pub xml_path: String,
    pub output_dir: String,
}

/// Settings shared by all jobs.
#[derive(Debug, Clone, Copy)]
pub struct Options {
    pub dialnorm_set: bool,
    pub measure_only: bool,
    pub keep_temp: bool,
    /// DEE reports 4 stages of progress for the measurement (version 5.2.0+).
    pub dee_reports_stages: bool,
}

/// DEE 5.2.0 and newer report the measurement as a quarter of the progress.
pub fn reports_stages(dee_version: Option<&str>) -> bool {
    dee_version.is_some_and(|v| version_ge(&v.replace("-master", ""), "5.2.0"))
}

// ---------------------------------------------------------------------------
// Descriptions of the progress line

pub fn desc_starting() -> String {
    // Padded to the width of the other descriptions.
    let text = "starting...";
    format!(
        "[bold][cyan]starting[/cyan][/bold]...{}",
        " ".repeat(DESCRIPTION_WIDTH - text.len())
    )
}

pub fn desc_ffmpeg(name: &str) -> String {
    format!(
        "[bold][cyan]ffmpeg[/cyan][/bold] | {}",
        escape(&trim_names(name, 6))
    )
}

pub fn desc_measure(name: &str) -> String {
    format!(
        "[bold cyan]DEE[/bold cyan]: measure | {}",
        escape(&trim_names(name, 12))
    )
}

pub fn desc_encode(name: &str) -> String {
    format!(
        "[bold cyan]DEE[/bold cyan]: encode | {}",
        escape(&trim_names(name, 11))
    )
}

pub fn desc_measured(name: &str, db: i64, measure_only: bool) -> String {
    let db = db.to_string();
    if measure_only {
        format!(
            "[bold cyan]DEE[/bold cyan]: measure | {} ({db} dB)",
            escape(&trim_names(name, 18 + db.len()))
        )
    } else {
        format!(
            "[bold cyan]DEE[/bold cyan]: encode | {} ({db} dB)",
            escape(&trim_names(name, 17 + db.len()))
        )
    }
}

// ---------------------------------------------------------------------------
// Output parsing

static FFMPEG_TIME: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"time=(\S+)").expect("valid regex"));
static LOUDNESS_WORD: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(measured_loudness|speech gated loudness)").expect("valid regex")
});
static LOUDNESS_VALUE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(measured_loudness|speech gated loudness)(=|: )(-?.+)").expect("valid regex")
});
static STAGE_PROGRESS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"Stage progress: ([0-9]+\.[0-9])").expect("valid regex"));

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
    /// Progress, 0 to 100.
    Progress(f64),
    /// The message of an error line.
    Error(String),
}

/// Interprets the lines printed by DEE.
#[derive(Debug, Clone)]
pub struct DeeParser {
    encoding_step: bool,
    /// Progress before the measurement is done only counts for a quarter.
    quarter_measure: bool,
}

impl DeeParser {
    pub fn new(format: Format, dee_reports_stages: bool) -> DeeParser {
        DeeParser {
            encoding_step: false,
            quarter_measure: format != Format::Thd && dee_reports_stages,
        }
    }

    pub fn feed(&mut self, line: &str) -> Vec<DeeEvent> {
        let mut events = Vec::new();
        if !self.encoding_step
            && LOUDNESS_WORD.is_match(line)
            && let Some(value) = LOUDNESS_VALUE
                .captures(line)
                .and_then(|c| c[3].trim_matches('.').trim().parse::<f64>().ok())
                .filter(|v| v.is_finite())
        {
            self.encoding_step = true;
            let db = clamp_i(value.round_ties_even() as i64, -31, 0);
            events.push(DeeEvent::Measured(db));
        }
        if let Some(c) = STAGE_PROGRESS.captures(line)
            && &c[1] != "100.0"
            && let Ok(value) = c[1].parse::<f64>()
        {
            let value = if self.quarter_measure && !self.encoding_step {
                value / 4.0
            } else {
                value
            };
            events.push(DeeEvent::Progress(value));
        }
        if line.to_lowercase().contains("error") {
            let line = line.trim_end();
            let message = line.split_once(": ").map_or(line, |(_, rest)| rest);
            events.push(DeeEvent::Error(message.trim().to_string()));
        }
        events
    }
}

/// Call `f` for every line of `reader`, where both `\r` and `\n` end a line
/// (ffmpeg ends its statistics with `\r`). Invalid UTF-8 is replaced.
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

// ---------------------------------------------------------------------------
// Files

/// Paths DEE writes next to a TrueHD output that are not wanted: both
/// the names with and without the DELAY part, `.thd.log` and `.thd.mll`.
pub fn thd_leftovers(input: &str, output_dir: &str) -> Vec<String> {
    let dir = Path::new(output_dir);
    let mut paths = Vec::new();
    for ext in ["thd.log", "thd.mll"] {
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

/// Remove the intermediate wav, which DEE may still hold for a moment on
/// Windows. Returns false if it could not be removed.
fn remove_wav(path: &str) -> bool {
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

fn cleanup(job: &Job, options: &Options, ui: &Ui) {
    if !options.keep_temp {
        if !remove_wav(&job.wav_path) {
            ui.say(&format!(
                "[bold yellow]Failed to delete:[/bold yellow] {}",
                escape(&job.wav_path)
            ));
            events::warning(&format!("Failed to delete: {}", job.wav_path));
        }
        let _ = std::fs::remove_file(&job.xml_path);
    }
    if job.format == Format::Thd {
        for path in thd_leftovers(&job.input, &job.output_dir) {
            let _ = std::fs::remove_file(path);
        }
    }
}

// ---------------------------------------------------------------------------
// Processes

fn spawn(args: &[String], stdout: Stdio, stderr: Stdio) -> Result<Child, String> {
    Command::new(&args[0])
        .args(&args[1..])
        .stdin(Stdio::null())
        .stdout(stdout)
        .stderr(stderr)
        .spawn()
        .map_err(|e| format!("failed to run {}: {e}", args[0]))
}

/// ffmpeg: converts the input to the intermediate wav.
fn run_ffmpeg(job: &Job, name: &str, bar: &Bar) -> Result<(), String> {
    let mut child = spawn(&job.ffmpeg_args, Stdio::null(), Stdio::piped())?;
    let stderr = child.stderr.take().expect("piped stderr");
    bar.describe(&desc_ffmpeg(name), Stage::Ffmpeg, None);
    if job.length == -1.0 {
        bar.indeterminate();
        for_each_line(stderr, |_| {});
    } else {
        bar.percent(0.0);
        for_each_line(stderr, |line| {
            if let Some(percent) = ffmpeg_percent(line, job.length) {
                bar.percent(percent);
            }
        });
    }
    let _ = child.wait();
    bar.percent(100.0);
    std::thread::sleep(Duration::from_millis(500));
    Ok(())
}

/// DEE: measures the loudness and encodes.
fn run_dee(job: &Job, options: &Options, name: &str, bar: &Bar, ui: &Ui) -> Result<(), String> {
    if options.dialnorm_set && job.format == Format::Thd {
        bar.describe(&desc_encode(name), Stage::Encode, None);
    } else {
        bar.describe(&desc_measure(name), Stage::Measure, None);
    }
    bar.percent(0.0);
    let mut child = spawn(&job.dee_args, Stdio::piped(), Stdio::null())?;
    let stdout = child.stdout.take().expect("piped stdout");
    let mut parser = DeeParser::new(job.format, options.dee_reports_stages);
    for_each_line(stdout, |line| {
        for event in parser.feed(line) {
            match event {
                DeeEvent::Measured(db) => {
                    let stage = if options.measure_only {
                        Stage::Measure
                    } else {
                        Stage::Encode
                    };
                    bar.describe(
                        &desc_measured(name, db, options.measure_only),
                        stage,
                        Some(db),
                    );
                    if options.measure_only {
                        bar.percent(100.0);
                        let _ = child.kill();
                    }
                }
                DeeEvent::Progress(percent) => bar.percent(percent),
                DeeEvent::Error(message) => {
                    events::error(&message);
                    ui.say_raw(&message);
                }
            }
        }
    });
    let _ = child.wait();
    bar.percent(100.0);
    Ok(())
}

/// Encode one input. `Err` if a program could not be started.
pub fn run_job(job: &Job, options: &Options, bar: &Bar, ui: &Ui) -> Result<(), String> {
    let name = file_name(&job.input);
    bar.describe(&desc_starting(), Stage::Starting, None);
    let result = (|| {
        if !job.intermediate_exists {
            run_ffmpeg(job, name, bar)?;
        }
        run_dee(job, options, name, bar, ui)
    })();
    if result.is_ok() {
        cleanup(job, options, ui);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::markup::strip;

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

    #[test]
    fn dee_version_gate() {
        assert!(reports_stages(Some("5.2.1")));
        assert!(reports_stages(Some("5.2.0")));
        assert!(reports_stages(Some("5.3.0-master")));
        assert!(!reports_stages(Some("5.1.9")));
        assert!(!reports_stages(None));
        assert!(!reports_stages(Some("garbage")));
    }

    fn events(parser: &mut DeeParser, lines: &[&str]) -> Vec<DeeEvent> {
        lines.iter().flat_map(|l| parser.feed(l)).collect()
    }

    #[test]
    fn dee_progress_is_scaled_while_measuring() {
        let mut p = DeeParser::new(Format::Ddp, true);
        let got = events(
            &mut p,
            &[
                "Stage progress: 40.0",
                "Stage progress: 100.0",
                "measured_loudness=-23.4",
                "Stage progress: 40.0",
            ],
        );
        assert_eq!(
            got,
            [
                DeeEvent::Progress(10.0),
                DeeEvent::Measured(-23),
                DeeEvent::Progress(40.0)
            ]
        );
        // older DEE and TrueHD report plain progress
        let mut p = DeeParser::new(Format::Ddp, false);
        assert_eq!(p.feed("Stage progress: 40.0"), [DeeEvent::Progress(40.0)]);
        let mut p = DeeParser::new(Format::Thd, true);
        assert_eq!(p.feed("Stage progress: 40.0"), [DeeEvent::Progress(40.0)]);
    }

    #[test]
    fn measured_loudness_forms() {
        let m = |line: &str| DeeParser::new(Format::Ddp, true).feed(line);
        assert_eq!(m("measured_loudness=-23.4"), [DeeEvent::Measured(-23)]);
        assert_eq!(m("measured_loudness=-23.6"), [DeeEvent::Measured(-24)]);
        assert_eq!(m("speech gated loudness: -27."), [DeeEvent::Measured(-27)]);
        assert_eq!(m("measured_loudness=-40"), [DeeEvent::Measured(-31)]);
        assert_eq!(m("measured_loudness=3.2"), [DeeEvent::Measured(0)]);
        // rounds half to even like Python
        assert_eq!(m("measured_loudness=-22.5"), [DeeEvent::Measured(-22)]);
        assert_eq!(m("measured_loudness=-23.5"), [DeeEvent::Measured(-24)]);
        assert_eq!(m("measured_loudness"), []);
        assert_eq!(m("measured_loudness=abc"), []);
    }

    #[test]
    fn measurement_is_only_taken_once() {
        let mut p = DeeParser::new(Format::Ddp, true);
        assert_eq!(p.feed("measured_loudness=-20"), [DeeEvent::Measured(-20)]);
        assert_eq!(p.feed("measured_loudness=-25"), []);
    }

    #[test]
    fn error_lines() {
        let mut p = DeeParser::new(Format::Ddp, true);
        assert_eq!(
            p.feed("2022-01-01 ERROR: something broke  "),
            [DeeEvent::Error("something broke".into())]
        );
        assert_eq!(
            p.feed("Error without separator"),
            [DeeEvent::Error("Error without separator".into())]
        );
        assert_eq!(
            p.feed("a: b: error: c"),
            [DeeEvent::Error("b: error: c".into())]
        );
        assert_eq!(p.feed("all fine"), []);
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
    fn descriptions_have_one_width() {
        let name = file_name("/in/some where/a very long file name that does not fit.mkv");
        let descs = [
            desc_starting(),
            desc_ffmpeg(name),
            desc_measure(name),
            desc_encode(name),
            desc_measured(name, -7, false),
            desc_measured(name, -27, true),
            desc_measured("x.wav", -27, false),
        ];
        for d in &descs {
            assert_eq!(strip(d).chars().count(), DESCRIPTION_WIDTH, "{}", strip(d));
        }
        assert_eq!(
            strip(&desc_measured("a.wav", -23, true)),
            format!("DEE: measure | {} (-23 dB)", trim_names("a.wav", 21))
        );
        assert_eq!(
            strip(&desc_ffmpeg("a [x].wav")),
            format!("ffmpeg | {}", trim_names("a [x].wav", 6))
        );
    }

    #[test]
    fn file_names() {
        assert_eq!(file_name("/a/b/c.mkv"), "c.mkv");
        assert_eq!(file_name("c.mkv"), "c.mkv");
        assert_eq!(file_name("dir/"), "");
    }

    #[test]
    fn thd_leftover_names() {
        let in_out = |names: &[&str]| -> Vec<String> {
            names
                .iter()
                .map(|n| Path::new("out").join(n).to_string_lossy().into_owned())
                .collect()
        };
        let list = thd_leftovers("in/a DELAY -5ms.mkv", "out");
        assert_eq!(
            list,
            in_out(&[
                "a.thd.log",
                "a DELAY -5ms.thd.log",
                "a.thd.mll",
                "a DELAY -5ms.thd.mll"
            ])
        );
        assert_eq!(
            thd_leftovers("in/a.mkv", "out"),
            in_out(&["a.thd.log", "a.thd.mll"])
        );
    }
}
