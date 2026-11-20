//! What the GUI shows of the input files comes from the deew CLI itself
//! (`deew --probe-json`, see `cli::probe_inputs`), so it always matches what
//! deew will do. This module has the types of that answer, its parsing, and
//! the config reading of the GUI.

use serde::Deserialize;
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The `version` of the JSON of `deew --probe-json` that is understood.
pub const PROBE_JSON_VERSION: u64 = 1;

/// Result for one input file. Either the properties or `error`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileProbe {
    pub path: String,
    pub channels: Option<i64>,
    pub sample_rate: Option<i64>,
    /// Raw bit depth, before the CLI's normalisation to 16/24/32.
    pub bit_depth: Option<i64>,
    /// ffprobe's `codec_name` of the stream (e.g. `truehd`), or the label of
    /// an Atmos master.
    pub codec: Option<String>,
    /// ffprobe's stream `profile` mentions Dolby Atmos.
    pub atmos: bool,
    /// The `language` tag of the selected track, `None` when missing (also
    /// from a deew that doesn't report it).
    #[serde(default)]
    pub language: Option<String>,
    /// The `title` tag of the selected track.
    #[serde(default)]
    pub title: Option<String>,
    /// An Atmos master (ADM BWF / DAMF) that DEE reads directly: `codec` is
    /// its label, never probed with ffprobe.
    pub master: bool,
    pub error: Option<String>,
    /// A file of a folder that isn't audio: left out, not an error.
    pub skipped: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeResult {
    /// The resolved ffprobe executable.
    pub ffprobe: String,
    pub files: Vec<FileProbe>,
}

/// The raw answer of `deew --probe-json`.
#[derive(Debug, Deserialize)]
struct CliProbe {
    version: Option<u64>,
    ffprobe: Option<String>,
    #[serde(default)]
    files: Vec<FileProbe>,
    error: Option<String>,
}

const TOO_OLD: &str = "this deew is too old for this deew-gui, update deew";

/// Reads the stdout of `deew --probe-json`. An output that isn't that JSON
/// (an older deew prints its usage for the unknown flag), or of an unknown
/// version, means the deew is too old; the `error` of the answer is an error.
pub fn parse_probe_json(stdout: &str) -> Result<ProbeResult, String> {
    let probe: CliProbe = serde_json::from_str(stdout.trim()).map_err(|_| TOO_OLD.to_string())?;
    if probe.version != Some(PROBE_JSON_VERSION) {
        return Err(TOO_OLD.to_string());
    }
    if let Some(error) = probe.error {
        return Err(error);
    }
    Ok(ProbeResult {
        ffprobe: probe.ffprobe.unwrap_or_default(),
        files: probe.files,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, text: &str) {
        std::fs::write(path, text).unwrap();
    }

    const GOOD: &str = r#"{"version":1,"ffprobe":"/x/ffprobe","files":[
        {"path":"a.mka","channels":6,"sampleRate":48000,"bitDepth":24,"codec":"truehd","atmos":true,"language":"eng","title":"Main","master":false,"error":null,"skipped":false},
        {"path":"d/n.txt","channels":null,"sampleRate":null,"bitDepth":null,"codec":null,"atmos":false,"master":false,"error":"ffprobe: Invalid data","skipped":true}
    ],"error":null}"#;

    #[test]
    fn parses_the_cli_answer() {
        let result = parse_probe_json(&format!("{GOOD}\n")).unwrap();
        assert_eq!(result.ffprobe, "/x/ffprobe");
        assert_eq!(result.files.len(), 2);
        assert_eq!(
            result.files[0],
            FileProbe {
                path: "a.mka".into(),
                channels: Some(6),
                sample_rate: Some(48000),
                bit_depth: Some(24),
                codec: Some("truehd".into()),
                atmos: true,
                language: Some("eng".into()),
                title: Some("Main".into()),
                master: false,
                error: None,
                skipped: false,
            }
        );
        // no tags: None
        assert_eq!(
            (
                result.files[1].language.as_deref(),
                result.files[1].title.as_deref()
            ),
            (None, None)
        );
        assert!(result.files[1].skipped);
        assert_eq!(
            result.files[1].error.as_deref(),
            Some("ffprobe: Invalid data")
        );
        assert_eq!(result.files[1].channels, None);
    }

    #[test]
    fn output_without_the_tags_still_works() {
        // `language` and `title` were added to version 1 later
        let json = r#"{"version":1,"ffprobe":"/x","files":[{"path":"a.wav","channels":2,"sampleRate":48000,"bitDepth":16,"codec":"pcm_s16le","atmos":false,"master":false,"error":null,"skipped":false}],"error":null}"#;
        let result = parse_probe_json(json).unwrap();
        assert_eq!(result.files[0].language, None);
        assert_eq!(result.files[0].title, None);
        let out = serde_json::to_value(&result).unwrap();
        assert_eq!(out["files"][0]["language"], serde_json::Value::Null);
    }

    #[test]
    fn a_top_level_error_is_an_error() {
        let json = r#"{"version":1,"ffprobe":null,"files":[],"error":"path x does not exist."}"#;
        assert_eq!(parse_probe_json(json), Err("path x does not exist.".into()));
    }

    #[test]
    fn an_old_deew_or_garbage_is_too_old() {
        let usage =
            "deew 5.3.0\nusage: deew [-h]\ndeew: error: unrecognized arguments: --probe-json\n";
        for text in [
            usage,
            "",
            "{",
            "[]",
            "{\"files\":[]}",
            GOOD.replace("\"version\":1", "\"version\":2").as_str(),
        ] {
            assert_eq!(parse_probe_json(text), Err(TOO_OLD.to_string()), "{text}");
        }
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

    #[test]
    fn config_is_next_to_the_cli() {
        assert_eq!(
            config_path(Some(Path::new("/cli"))),
            Some(Path::new("/cli").join("config.toml"))
        );
        assert_eq!(config_path(None), None);
    }
}
