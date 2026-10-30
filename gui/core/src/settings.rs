//! GUI settings, stored in the `[gui]` section of the deew `config.toml`.
//!
//! The CLI ignores this section. Saving edits only `gui.progress` and keeps
//! the rest of the file (comments, quoting, indentation) as it is.

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GuiSettings {
    pub progress: ProgressView,
}

/// Comment above the section when it is added to an existing config.
const SECTION_COMMENT: &str = "# Output of deew-gui and deew-web: 'cli' shows the terminal output of the CLI,\n# 'native' shows progress bars, warnings and pop-up prompts in the window.\n";

const NOT_FOUND: &str = "config.toml does not exist yet, run deew once to create it";

/// Reads the settings from the config text. Anything invalid (not TOML,
/// missing section or key, wrong type, unknown value) gives the default.
pub fn parse(text: &str) -> GuiSettings {
    let progress = text
        .parse::<toml::Table>()
        .ok()
        .and_then(|doc| {
            doc.get("gui")?
                .get("progress")?
                .as_str()
                .and_then(ProgressView::from_str)
        })
        .unwrap_or_default();
    GuiSettings { progress }
}

/// Returns the config text with `gui.progress` set, everything else kept.
pub fn update_text(text: &str, settings: &GuiSettings) -> Result<String, String> {
    let mut doc = text
        .parse::<DocumentMut>()
        .map_err(|e| format!("config.toml is not valid TOML: {e}"))?;
    // Single-quoted literal string, like the other values of the template.
    let value: Value = format!("'{}'", settings.progress.as_str())
        .parse()
        .map_err(|e| format!("internal error: {e}"))?;

    match doc.get_mut("gui") {
        None => {
            let mut out = text.to_string();
            if !out.is_empty() && !out.ends_with('\n') {
                out.push('\n');
            }
            out.push('\n');
            out.push_str(SECTION_COMMENT);
            out.push_str(&format!(
                "[gui]\n    progress = '{}'\n",
                settings.progress.as_str()
            ));
            Ok(out)
        }
        Some(item) => {
            let table: &mut Table = item
                .as_table_mut()
                .ok_or_else(|| "[gui] in config.toml is not a table".to_string())?;
            match table.get_mut("progress") {
                Some(Item::Value(old)) => {
                    let mut new = value;
                    *new.decor_mut() = old.decor().clone();
                    *old = new;
                }
                _ => {
                    table.insert("progress", Item::Value(value));
                    if let Some(mut key) = table.key_mut("progress") {
                        key.leaf_decor_mut().set_prefix("    ");
                    }
                }
            }
            Ok(doc.to_string())
        }
    }
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
        }
    }

    fn block(value: &str) -> String {
        format!("{SECTION_COMMENT}[gui]\n    progress = '{value}'\n")
    }

    #[test]
    fn serde_shape() {
        assert_eq!(
            serde_json::to_string(&native()).unwrap(),
            r#"{"progress":"native"}"#
        );
        let cli: GuiSettings = serde_json::from_str(r#"{"progress":"cli"}"#).unwrap();
        assert_eq!(cli, GuiSettings::default());
        assert!(serde_json::from_str::<GuiSettings>(r#"{"progress":"x"}"#).is_err());
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
        let expected = if TEMPLATE.contains("[gui]") {
            TEMPLATE.replace("progress = 'cli'", "progress = 'native'")
        } else {
            format!("{TEMPLATE}\n{}", block("native"))
        };
        assert_eq!(updated, expected);

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
            format!("{text}\n{}", block("native"))
        );
        // A missing final newline is added first.
        assert_eq!(
            update_text("a = 1", &native()).unwrap(),
            format!("a = 1\n\n{}", block("native"))
        );
    }

    #[test]
    fn existing_value_is_replaced_keeping_its_decor() {
        let text = "[gui]\n  progress   =   \"cli\"   # my note\nother = 1\n";
        assert_eq!(
            update_text(text, &native()).unwrap(),
            "[gui]\n  progress   =   'native'   # my note\nother = 1\n"
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
