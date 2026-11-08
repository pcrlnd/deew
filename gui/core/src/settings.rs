//! GUI settings, stored in the deew `config.toml`: `gui.progress` (the CLI
//! ignores the `[gui]` section) and the CLI's `output.path` / `output.to_input`.
//! Saving edits only these three keys and keeps the rest of the file
//! (comments, quoting, indentation) as it is.

use serde::{Deserialize, Serialize};
use std::path::Path;
use toml_edit::{DocumentMut, Item, Table, Value};

/// What the output area of the GUI shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProgressView {
    /// The terminal output of the CLI.
    #[default]
    Cli,
    /// Progress bars, warnings and pop-up prompts in the window.
    Native,
}

impl ProgressView {
    fn as_str(self) -> &'static str {
        match self {
            ProgressView::Cli => "cli",
            ProgressView::Native => "native",
        }
    }

    fn from_str(value: &str) -> Option<ProgressView> {
        match value {
            "cli" => Some(ProgressView::Cli),
            "native" => Some(ProgressView::Native),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GuiSettings {
    pub progress: ProgressView,
    /// `output.path`: the output directory (`-o`), empty uses the default.
    #[serde(default)]
    pub output: String,
    /// `output.to_input`: save each output next to its input (`-oi`).
    #[serde(default)]
    pub to_input: bool,
}

/// The settings plus read-only values for the window.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsView {
    #[serde(flatten)]
    pub settings: GuiSettings,
    /// The folder deew runs in (the home folder), empty when unknown.
    pub default_output: String,
    /// `default_modes.ddp` of the config (program default when missing).
    pub default_ddp_mode: String,
    /// `default_modes.ddp_atmos` of the config (program default when missing).
    pub default_ddp_atmos_mode: String,
}

/// `default_modes.ddp` and `.ddp_atmos` of a config text; a missing or
/// unknown value gives the program default.
pub fn parse_default_modes(text: &str) -> (String, String) {
    let modes = text
        .parse::<toml::Table>()
        .ok()
        .and_then(|doc| doc.get("default_modes").cloned());
    let pick = |key: &str, allowed: &[&str], default: &str| {
        modes
            .as_ref()
            .and_then(|m| m.get(key)?.as_str())
            .filter(|v| allowed.contains(v))
            .unwrap_or(default)
            .to_string()
    };
    (
        pick("ddp", &["standard", "bluray"], "standard"),
        pick(
            "ddp_atmos",
            &["streaming", "standard", "bluray"],
            "streaming",
        ),
    )
}

impl SettingsView {
    pub fn new(settings: GuiSettings, config: Option<&Path>) -> SettingsView {
        let (default_ddp_mode, default_ddp_atmos_mode) = parse_default_modes(
            &config
                .and_then(|path| std::fs::read_to_string(path).ok())
                .unwrap_or_default(),
        );
        SettingsView {
            settings,
            default_ddp_mode,
            default_ddp_atmos_mode,
            default_output: std::env::home_dir()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default(),
        }
    }
}

/// Comment above the `[gui]` section when it is added to an existing config.
const SECTION_COMMENT: &str = "# Settings of deew-gui and deew-web:
# progress: 'cli' shows the terminal output of the CLI, 'native' shows progress bars, warnings and pop-up prompts in the window.
";

/// Comment above the section when it is added to an existing config.
const OUTPUT_COMMENT: &str = "# Where the encoded files are saved.
# path: the output directory, empty uses the current directory.
#   You can override it with -o/--output.
# to_input: save each output next to its input file, this overrides path.
#   You can turn it on with -oi/--output-to-input.
";

const NOT_FOUND: &str = "config.toml does not exist yet, run deew once to create it";

/// Reads the settings from the config text. Anything invalid (not TOML,
/// missing section or key, wrong type, unknown value) gives the default.
pub fn parse(text: &str) -> GuiSettings {
    let Some(doc) = text.parse::<toml::Table>().ok() else {
        return GuiSettings::default();
    };
    let gui = doc.get("gui");
    let progress = gui
        .and_then(|g| g.get("progress")?.as_str())
        .and_then(ProgressView::from_str)
        .unwrap_or_default();
    let section = doc.get("output");
    let output = section
        .and_then(|o| o.get("path")?.as_str())
        .unwrap_or_default()
        .to_string();
    let to_input = section
        .and_then(|o| o.get("to_input")?.as_bool())
        .unwrap_or_default();
    GuiSettings {
        progress,
        output,
        to_input,
    }
}

/// A TOML string value: literal (single-quoted) when possible, so Windows
/// paths keep their backslashes, else a basic string with escapes.
fn output_value(output: &str) -> Result<Value, String> {
    let text = if output.contains('\'') || output.chars().any(|c| c.is_control()) {
        Value::from(output).to_string()
    } else {
        format!("'{output}'")
    };
    text.trim()
        .parse()
        .map_err(|e| format!("internal error: {e}"))
}

/// Replaces the value keeping its decor, or inserts the key indented.
fn set_key(table: &mut Table, name: &str, value: Value) {
    match table.get_mut(name) {
        Some(Item::Value(old)) => {
            let mut new = value;
            *new.decor_mut() = old.decor().clone();
            *old = new;
        }
        _ => {
            table.insert(name, Item::Value(value));
            if let Some(mut key) = table.key_mut(name) {
                key.leaf_decor_mut().set_prefix("    ");
            }
        }
    }
}

/// Sets keys in the existing table `name`. Returns false when the document
/// has no such table.
fn edit_section(
    doc: &mut DocumentMut,
    name: &str,
    keys: Vec<(&str, Value)>,
) -> Result<bool, String> {
    match doc.get_mut(name) {
        None => Ok(false),
        Some(item) => {
            let table: &mut Table = item
                .as_table_mut()
                .ok_or_else(|| format!("[{name}] in config.toml is not a table"))?;
            for (key, value) in keys {
                set_key(table, key, value);
            }
            Ok(true)
        }
    }
}

fn bool_value(value: bool) -> Value {
    Value::from(value)
}

/// Returns the config text with `gui.progress`, `output.path` and
/// `output.to_input` set, everything else kept.
pub fn update_text(text: &str, settings: &GuiSettings) -> Result<String, String> {
    let mut doc = text
        .parse::<DocumentMut>()
        .map_err(|e| format!("config.toml is not valid TOML: {e}"))?;
    // Single-quoted literal string, like the other values of the template.
    let progress: Value = format!("'{}'", settings.progress.as_str())
        .parse()
        .map_err(|e| format!("internal error: {e}"))?;
    let path = output_value(&settings.output)?;

    let had_gui = edit_section(&mut doc, "gui", vec![("progress", progress)])?;
    let had_output = edit_section(
        &mut doc,
        "output",
        vec![
            ("path", path.clone()),
            ("to_input", bool_value(settings.to_input)),
        ],
    )?;
    let mut out = doc.to_string();
    if had_gui && had_output {
        return Ok(out);
    }
    // Missing sections are appended, each after one blank line.
    let blank = |out: &mut String| {
        if !out.is_empty() && !out.ends_with('\n') {
            out.push('\n');
        }
        out.push('\n');
    };
    if !had_output {
        blank(&mut out);
        out.push_str(OUTPUT_COMMENT);
        out.push_str(&format!(
            "[output]\n    path = {}\n    to_input = {}\n",
            path, settings.to_input
        ));
    }
    if !had_gui {
        blank(&mut out);
        out.push_str(SECTION_COMMENT);
        out.push_str(&format!(
            "[gui]\n    progress = '{}'\n",
            settings.progress.as_str()
        ));
    }
    Ok(out)
}

/// Settings from the config file; the default without a path or file.
pub fn load(path: Option<&Path>) -> GuiSettings {
    path.and_then(|path| std::fs::read_to_string(path).ok())
        .map(|text| parse(&text))
        .unwrap_or_default()
}

/// Writes the settings into the existing config file. The file is not
/// created: a config with only `[gui]` would make the CLI fail on missing keys.
pub fn save(path: Option<&Path>, settings: &GuiSettings) -> Result<(), String> {
    let path = path.filter(|path| path.is_file()).ok_or(NOT_FOUND)?;
    let text =
        std::fs::read_to_string(path).map_err(|e| format!("cannot read config.toml: {e}"))?;
    let updated = update_text(&text, settings)?;
    if updated != text {
        std::fs::write(path, updated).map_err(|e| format!("cannot write config.toml: {e}"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEMPLATE: &str = include_str!("../../../src/config_template.toml");

    fn native() -> GuiSettings {
        GuiSettings {
            progress: ProgressView::Native,
            ..GuiSettings::default()
        }
    }

    fn with_output(output: &str) -> GuiSettings {
        GuiSettings {
            progress: ProgressView::Native,
            output: output.to_string(),
            to_input: false,
        }
    }

    fn gui_block(value: &str) -> String {
        format!("{SECTION_COMMENT}[gui]\n    progress = '{value}'\n")
    }

    fn output_block(path: &str, to_input: bool) -> String {
        format!("{OUTPUT_COMMENT}[output]\n    path = {path}\n    to_input = {to_input}\n")
    }

    #[test]
    fn serde_shape() {
        assert_eq!(
            serde_json::to_string(&native()).unwrap(),
            r#"{"progress":"native","output":"","toInput":false}"#
        );
        let cli: GuiSettings = serde_json::from_str(r#"{"progress":"cli"}"#).unwrap();
        assert_eq!(cli, GuiSettings::default());
        assert!(serde_json::from_str::<GuiSettings>(r#"{"progress":"x"}"#).is_err());
        let o: GuiSettings =
            serde_json::from_str(r#"{"progress":"cli","output":"/m","defaultOutput":"/h"}"#)
                .unwrap();
        assert_eq!(o.output, "/m");
        let view = SettingsView {
            settings: o,
            default_output: "/h".into(),
            ..SettingsView::default()
        };
        assert_eq!(
            serde_json::to_string(&view).unwrap(),
            r#"{"progress":"cli","output":"/m","toInput":false,"defaultOutput":"/h","defaultDdpMode":"","defaultDdpAtmosMode":""}"#
        );
        let o: GuiSettings = serde_json::from_str(r#"{"progress":"cli","toInput":true}"#).unwrap();
        assert!(o.to_input);
    }

    #[test]
    fn output_round_trips() {
        for out in [
            "C:\\Users\\pc\\Music",
            "/home/o'neil/x",
            "a\\'b\"c",
            "",
            "line\nbreak",
            "/tmp/ü",
        ] {
            let s = with_output(out);
            for base in [
                TEMPLATE,
                "",
                "[gui]\nprogress = 'cli'\n",
                "[gui]\n",
                "[output]\n",
            ] {
                let updated = update_text(base, &s).unwrap();
                assert_eq!(parse(&updated), s, "{out:?} in {updated}");
            }
        }
        let updated = update_text("", &with_output("C:\\Users\\pc\\Music")).unwrap();
        assert!(
            updated.contains("path = 'C:\\Users\\pc\\Music'\n"),
            "{updated}"
        );
    }

    #[test]
    fn missing_output_key_is_inserted() {
        let text = "[output]\n  path = 'a'\nother = 1\n";
        let updated = update_text(text, &with_output("/x")).unwrap();
        assert!(
            updated.starts_with("[output]\n  path = '/x'\nother = 1\n    to_input = false\n"),
            "{updated}"
        );
    }

    #[test]
    fn output_parse_is_lenient() {
        assert_eq!(parse("[output]\npath = 'C:\\a'\n").output, "C:\\a");
        assert_eq!(parse("[output]\npath = 3\n").output, "");
        assert_eq!(parse("[gui]\nprogress = 'cli'\n").output, "");
        assert!(parse("[output]\nto_input = true\n").to_input);
        assert!(!parse("[output]\nto_input = 'true'\n").to_input);
        // the old key is ignored
        assert_eq!(parse("[gui]\noutput = '/old'\n").output, "");
    }

    #[test]
    fn to_input_is_saved_without_touching_the_path() {
        let base = update_text(TEMPLATE, &with_output("/keep")).unwrap();
        let on = GuiSettings {
            to_input: true,
            ..with_output("/keep")
        };
        let updated = update_text(&base, &on).unwrap();
        assert_eq!(parse(&updated), on);
        assert_eq!(updated, base.replace("to_input = false", "to_input = true"));
    }

    #[test]
    fn default_modes_come_from_the_config() {
        let d = |t: &str| parse_default_modes(t);
        assert_eq!(d(""), ("standard".into(), "streaming".into()));
        assert_eq!(d(TEMPLATE), ("standard".into(), "streaming".into()));
        assert_eq!(
            d("[default_modes]\nddp = 'bluray'\nddp_atmos = 'standard'\n"),
            ("bluray".into(), "standard".into())
        );
        // unknown values give the program default
        assert_eq!(
            d("[default_modes]\nddp = 'x'\nddp_atmos = 3\n"),
            ("standard".into(), "streaming".into())
        );
    }

    #[test]
    fn parse_is_lenient() {
        assert_eq!(parse("[gui]\nprogress = 'native'\n"), native());
        assert_eq!(parse("[gui]\nprogress = \"native\"\n"), native());
        for text in [
            "",
            "not toml [[[",
            "a = 1\n",
            "[gui]\n",
            "[gui]\nprogress = 3\n",
            "[gui]\nprogress = 'bogus'\n",
            "gui = 1\n",
        ] {
            assert_eq!(parse(text), GuiSettings::default(), "{text:?}");
        }
    }

    #[test]
    fn template_parses_as_cli() {
        assert_eq!(parse(TEMPLATE), GuiSettings::default());
    }

    #[test]
    fn template_round_trip_keeps_everything_else() {
        let updated = update_text(TEMPLATE, &native()).unwrap();
        assert_eq!(parse(&updated), native());
        assert_eq!(
            updated,
            TEMPLATE.replace("progress = 'cli'", "progress = 'native'")
        );

        let back = update_text(&updated, &GuiSettings::default()).unwrap();
        assert_eq!(parse(&back), GuiSettings::default());
        assert_eq!(
            back,
            update_text(TEMPLATE, &GuiSettings::default()).unwrap()
        );
        // Setting the same value again changes nothing.
        assert_eq!(update_text(&updated, &native()).unwrap(), updated);
    }

    #[test]
    fn section_is_appended_after_one_blank_line() {
        let text = "# keep me\na = \"x\"   # tail\n";
        assert_eq!(
            update_text(text, &native()).unwrap(),
            format!(
                "{text}\n{}\n{}",
                output_block("''", false),
                gui_block("native")
            )
        );
        // A missing final newline is added first.
        assert_eq!(
            update_text("a = 1", &native()).unwrap(),
            format!(
                "a = 1\n\n{}\n{}",
                output_block("''", false),
                gui_block("native")
            )
        );
    }

    #[test]
    fn existing_value_is_replaced_keeping_its_decor() {
        let text = "[gui]\n  progress   =   \"cli\"   # my note\nother = 1\n";
        assert_eq!(
            update_text(text, &native()).unwrap(),
            format!(
                "[gui]\n  progress   =   'native'   # my note\nother = 1\n\n{}",
                output_block("''", false)
            )
        );
    }

    #[test]
    fn missing_key_is_added_to_the_section() {
        let text = "[gui]\nother = 1\n\n[next]\nb = 2\n";
        let updated = update_text(text, &native()).unwrap();
        assert_eq!(parse(&updated), native());
        assert!(updated.contains("    progress = 'native'\n"), "{updated}");
        assert!(updated.contains("other = 1\n"));
        assert!(updated.contains("[next]\nb = 2\n"));
    }

    #[test]
    fn invalid_toml_is_an_error() {
        let err = update_text("a = [", &native()).unwrap_err();
        assert!(err.starts_with("config.toml is not valid TOML: "), "{err}");
        assert!(update_text("gui = 1\n", &native()).is_err());
    }

    #[test]
    fn save_and_load_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        assert_eq!(load(Some(&path)), GuiSettings::default());
        assert_eq!(load(None), GuiSettings::default());
        assert_eq!(save(Some(&path), &native()).unwrap_err(), NOT_FOUND);
        assert_eq!(save(None, &native()).unwrap_err(), NOT_FOUND);
        assert!(!path.exists());

        std::fs::write(&path, TEMPLATE).unwrap();
        save(Some(&path), &native()).unwrap();
        assert_eq!(load(Some(&path)), native());
        save(Some(&path), &GuiSettings::default()).unwrap();
        assert_eq!(load(Some(&path)), GuiSettings::default());
    }

    #[test]
    fn invalid_toml_leaves_the_file_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "a = [\n").unwrap();
        assert!(save(Some(&path), &native()).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "a = [\n");
        assert_eq!(load(Some(&path)), GuiSettings::default());
    }
}
