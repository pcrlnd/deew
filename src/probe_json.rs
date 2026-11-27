//! Hidden `--probe-json`: the probe of the inputs as one JSON object, for the
//! GUI. The inputs go through the code of a real run (folders, patterns, DAMF
//! parts, masters, ffprobe with `-ti`), so the GUI shows what deew will do.
//!
//! ```text
//! {"version":1,"ffprobe":"<path>","files":[{"path":"..","channels":6,
//!  "sampleRate":48000,"bitDepth":24,"codec":"truehd","atmos":true,"dtsx":false,
//!  "auro":null,"language":"eng","title":null,"master":false,"error":null,"skipped":false}],
//!  "error":null}
//! ```
//!
//! `language` and `title` (the tags of the track, `null` when missing) were
//! added without a new version: a reader has to accept output without them.
//!
//! `auro` (`{"layout":"13.1","carrier":"7.1"}` for an Auro-3D track, `layout`
//! `null` when unknown, else `null`) was added too. It needs harletty next to
//! deew (or in the PATH); without it, or for a track it can't read, it is `null`.
//!
//! A fatal problem gives `"files":[]` and `"error"` (exit code 1). The
//! config isn't read, nothing else is printed.

use crate::config::Paths;
use crate::data::ErrorKind;
use crate::error::format_error;
use crate::inputs::Input;
use crate::pipeline::{check_auro, probe_one, try_collect_inputs};
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
        // a DTS:X track; its channels are those of the bed (heights folded in)
        "dtsx": probe.as_ref().is_some_and(Probe::is_dtsx),
        // an Auro-3D track (best effort, from harletty); its channels are those
        // of the layout that carries it
        "auro": probe.as_ref().and_then(Probe::auro).map(|s| json!({
            "layout": s.auro_layout,
            "carrier": s.auro_carrier,
        })),
        // the tags of the selected track, null when missing
        "language": probe.as_ref().and_then(|p| p.language.as_deref()),
        "title": probe.as_ref().and_then(|p| p.title.as_deref()),
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
    let mut results: Vec<Result<Probe, ProbeError>> = inputs
        .iter()
        .map(|input| probe_one(input, &ffprobe, None, track_index))
        .collect();
    if let Some(ffmpeg) = tools::find_program("ffmpeg", &paths.exe_dir, &cwd) {
        let harletty = tools::find_harletty(&paths.exe_dir, &cwd);
        let found: Vec<(&str, &mut Probe)> = inputs
            .iter()
            .zip(results.iter_mut())
            .filter_map(|(input, result)| Some((input.path.as_str(), result.as_mut().ok()?)))
            .collect();
        check_auro(&ffmpeg, harletty.as_deref(), found, track_index);
    }
    let files: Vec<Value> = inputs
        .iter()
        .zip(results)
        .map(|(input, result)| file_json(input, result))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dtsx_is_reported_with_the_channels_of_the_bed() {
        let json = r#"{"streams":[{"codec_type":"audio","codec_name":"dts","profile":"DTS-HD MA + DTS:X",
            "sample_rate":"48000","channels":12,"channel_layout":"7.1.4"}]}"#;
        let probe = crate::probe::parse_probe(json, TrackIndex::default()).unwrap();
        let input = Input {
            path: "a.dts".into(),
            name: "a.dts".into(),
            folder: None,
            bad_name: false,
        };
        let value = file_json(&input, Ok(probe));
        assert_eq!(value["dtsx"], true);
        assert_eq!(value["channels"], 8);
        let json = r#"{"streams":[{"codec_type":"audio","codec_name":"ac3","sample_rate":"48000","channels":6}]}"#;
        let probe = crate::probe::parse_probe(json, TrackIndex::default()).unwrap();
        assert_eq!(file_json(&input, Ok(probe))["dtsx"], false);
    }

    #[test]
    fn auro_is_reported_from_the_check() {
        let json = r#"{"streams":[{"codec_type":"audio","codec_name":"dts","profile":"DTS-HD MA",
            "sample_rate":"48000","channels":8,"channel_layout":"7.1","bits_per_raw_sample":"24"}]}"#;
        let input = Input {
            path: "a.dts".into(),
            name: "a.dts".into(),
            folder: None,
            bad_name: false,
        };
        let mut probe = crate::probe::parse_probe(json, TrackIndex::default()).unwrap();
        assert!(probe.auro_candidate());
        // harletty missing, too old or failing: the check leaves no trace
        let without = file_json(&input, Ok(probe.clone()));
        assert!(without["auro"].is_null());
        assert_eq!(without["channels"], 8);
        assert_eq!(
            without.to_string(),
            r#"{"atmos":false,"auro":null,"bitDepth":24,"channels":8,"codec":"dts","dtsx":false,"error":null,"language":null,"master":false,"path":"a.dts","sampleRate":48000,"skipped":false,"title":null}"#
        );
        // what harletty 0.9.0 said about the Auro-3D file
        let info = r#"{"schema":1,"harletty":"0.9.0","taxonomy":4,"codec":"DTS-HD MA","channels":8,"sample_rate":48000,"spatial":{"label":"Auro-3D-13.1","kind":"auro","objects":0,"fixed":null,"experimental":false},"auro":{"carrier":"7.1","original":"7.1_5H_1T"},"frames_seen":4,"seconds_seen":0.0427}"#;
        probe.harletty_check = Some(crate::harletty::parse_info(info).unwrap());
        let with = file_json(&input, Ok(probe));
        assert_eq!(
            with["auro"],
            serde_json::json!({"layout": "13.1", "carrier": "7.1"})
        );
        assert_eq!(
            with.to_string(),
            r#"{"atmos":false,"auro":{"carrier":"7.1","layout":"13.1"},"bitDepth":24,"channels":8,"codec":"dts","dtsx":false,"error":null,"language":null,"master":false,"path":"a.dts","sampleRate":48000,"skipped":false,"title":null}"#
        );
    }
}
