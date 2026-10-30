//! Structured events for deew-gui and deew-web: when `DEEW_EVENTS_FILE` is
//! set, one compact JSON object per line is appended to that file.

use serde::Serialize;
use serde_json::{Value, json};
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::sync::{Mutex, OnceLock};

static SINK: OnceLock<Option<Mutex<File>>> = OnceLock::new();

/// What a job is doing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Stage {
    Starting,
    Ffmpeg,
    Measure,
    Encode,
}

/// Open the file named by `DEEW_EVENTS_FILE` (if any) and write `hello`.
pub fn init() {
    let file = std::env::var_os("DEEW_EVENTS_FILE")
        .filter(|path| !path.is_empty())
        .and_then(|path| OpenOptions::new().append(true).create(true).open(path).ok());
    let _ = SINK.set(file.map(Mutex::new));
    emit(json!({"type": "hello", "version": env!("CARGO_PKG_VERSION")}));
}

/// Whether events are written.
pub fn enabled() -> bool {
    matches!(SINK.get(), Some(Some(_)))
}

/// The line written for `value`, with the trailing newline.
fn line(value: &Value) -> String {
    format!("{value}\n")
}

fn emit(value: Value) {
    if let Some(Some(file)) = SINK.get() {
        let mut file = file.lock().expect("events file");
        let _ = file.write_all(line(&value).as_bytes());
        let _ = file.flush();
    }
}

fn round2(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

fn jobs_value(files: &[&str]) -> Value {
    json!({"type": "jobs", "files": files})
}

fn progress_value(job: usize, stage: Stage, percent: Option<f64>, db: Option<i64>) -> Value {
    json!({
        "type": "progress",
        "job": job,
        "stage": stage,
        "percent": percent.map(round2),
        "db": db,
    })
}

fn text_value(kind: &str, text: &str) -> Value {
    json!({"type": kind, "text": text})
}

/// The file names of the jobs, in job order.
pub fn jobs(files: &[&str]) {
    if enabled() {
        emit(jobs_value(files));
    }
}

/// Progress of a job; `percent` is 0 to 100, `None` for an unknown total.
pub fn progress(job: usize, stage: Stage, percent: Option<f64>, db: Option<i64>) {
    if enabled() {
        emit(progress_value(job, stage, percent, db));
    }
}

/// The job's bar finished.
pub fn done(job: usize) {
    if enabled() {
        emit(json!({"type": "done", "job": job}));
    }
}

/// `text` is plain text.
pub fn warning(text: &str) {
    if enabled() {
        emit(text_value("warning", text));
    }
}

/// `text` is plain text.
pub fn error(text: &str) {
    if enabled() {
        emit(text_value("error", text));
    }
}

/// A y/n question is waiting for stdin; `text` is plain text.
pub fn prompt(text: &str) {
    if enabled() {
        emit(text_value("prompt", text));
    }
}

/// The prompt got a valid answer.
pub fn answered() {
    if enabled() {
        emit(json!({"type": "answered"}));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_are_compact_and_terminated() {
        assert_eq!(
            line(&json!({"type": "hello", "version": "1.2.3"})),
            "{\"type\":\"hello\",\"version\":\"1.2.3\"}\n"
        );
    }

    #[test]
    fn jobs_shape() {
        assert_eq!(
            jobs_value(&["a.mkv", "b.mkv"]).to_string(),
            r#"{"files":["a.mkv","b.mkv"],"type":"jobs"}"#
        );
    }

    #[test]
    fn progress_shape() {
        let v = progress_value(2, Stage::Encode, Some(12.3456), Some(-24));
        assert_eq!(v["type"], "progress");
        assert_eq!(v["job"], 2);
        assert_eq!(v["stage"], "encode");
        assert_eq!(v["percent"], 12.35);
        assert_eq!(v["db"], -24);
        let v = progress_value(0, Stage::Starting, None, None);
        assert_eq!(v["stage"], "starting");
        assert!(v["percent"].is_null() && v["db"].is_null());
        assert!(v.as_object().unwrap().contains_key("percent"));
        assert_eq!(
            progress_value(0, Stage::Ffmpeg, Some(0.0), None)["stage"],
            "ffmpeg"
        );
        assert_eq!(
            progress_value(0, Stage::Measure, Some(100.0), None)["stage"],
            "measure"
        );
    }

    #[test]
    fn text_events_shape() {
        for kind in ["warning", "error", "prompt"] {
            assert_eq!(
                text_value(kind, "a \"b\"\nc"),
                json!({"type": kind, "text": "a \"b\"\nc"})
            );
        }
        // newlines are escaped, so one event is one line
        assert_eq!(line(&text_value("error", "a\nb")).matches('\n').count(), 1);
    }
}
