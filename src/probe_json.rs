//! Hidden `--probe-json`: the probe of the inputs as one JSON object, for the
//! GUI. The inputs go through the code of a real run (folders, patterns, DAMF
//! parts, masters, ffprobe with `-ti`), so the GUI shows what deew will do.
//!
//! ```text
//! {"version":1,"ffprobe":"<path>","files":[{"path":"..","channels":6,
//!  "sampleRate":48000,"bitDepth":24,"codec":"truehd","atmos":true,
//!  "master":false,"error":null,"skipped":false}],"error":null}
//! ```
//!
//! A fatal problem gives `"files":[]` and `"error"` (exit code 1). The
//! config isn't read, nothing else is printed.

use crate::config::Paths;
use crate::data::ErrorKind;
use crate::error::format_error;
use crate::inputs::Input;
use crate::pipeline::{probe_one, try_collect_inputs};
use crate::probe::{Probe, ProbeError, TrackIndex};
use crate::{cli, markup, tools};
use serde_json::{Value, json};

/// The version of the JSON, the GUI refuses one it doesn't know.
const VERSION: u32 = 1;

fn file_json(input: &Input, result: Result<Probe, ProbeError>) -> Value {
    let (probe, error, skipped) = match result {
        Ok(probe) => (Some(probe), None, false),
        Err(error) => {
            let skipped = input.folder.is_some() && error.skippable();
            (None, Some(error.message()), skipped)
        }
    };
    json!({
        "path": input.path,
        "channels": probe.as_ref().map(|p| p.channels),
        "sampleRate": probe.as_ref().map(|p| p.sample_rate),
        // raw, before the normalisation to 16/24/32
        "bitDepth": probe.as_ref().map(|p| p.bit_depth),
        "codec": probe.as_ref().map(|p| p.codec.as_str()).filter(|c| !c.is_empty()),
        "atmos": probe.as_ref().is_some_and(|p| p.is_atmos),
        "master": probe.as_ref().is_some_and(|p| p.master.is_some()),
        "error": error,
        "skipped": skipped,
    })
}

fn fatal(ffprobe: Option<&str>, message: &str) -> Value {
    json!({"version": VERSION, "ffprobe": ffprobe, "files": [], "error": message})
}

/// The JSON for the command line and whether it went well.
fn probe(argv: &[String], paths: &Paths) -> (Value, bool) {
    let args = match cli::parse_quiet(argv) {
        Ok(args) => args,
        Err(message) => return (fatal(None, &message), false),
    };
    let cwd = std::env::current_dir().unwrap_or_default();
    let Some(ffprobe) = tools::find_program("ffprobe", &paths.exe_dir, &cwd) else {
        let message = format_error(ErrorKind::BinaryExist, Some("ffprobe"));
        return (fatal(None, &markup::strip(&message)), false);
    };
    let input = args.input.unwrap_or_default();
    if input.is_empty() {
        let message = "the following arguments are required: -i/--input";
        return (fatal(Some(&ffprobe), message), false);
    }
    let inputs = match try_collect_inputs(&input) {
        Ok(inputs) if inputs.is_empty() => {
            return (fatal(Some(&ffprobe), "no input files found."), false);
        }
        Ok(inputs) => inputs,
        Err(message) => return (fatal(Some(&ffprobe), &markup::strip(&message)), false),
    };
    let track_index: TrackIndex = args.track_index;
    // no atmos_info: the duration of a DAMF isn't shown
    let files: Vec<Value> = inputs
        .iter()
        .map(|input| file_json(input, probe_one(input, &ffprobe, None, track_index)))
        .collect();
    let value = json!({"version": VERSION, "ffprobe": ffprobe, "files": files, "error": null});
    (value, true)
}

/// Print the JSON and exit.
pub fn run(argv: &[String], paths: &Paths) -> ! {
    let (value, success) = probe(argv, paths);
    println!("{value}");
    std::process::exit(i32::from(!success));
}
