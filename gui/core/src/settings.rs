//! GUI settings, stored in the deew `config.toml`: `gui.progress`,
//! `gui.scaling`, `gui.use_system_font` and `gui.theme` (the CLI ignores the
//! `[gui]` section) and the CLI's `output.path` / `output.to_input`.
//! Saving edits only `gui.progress` and the two `output` keys (`scaling`,
//! `use_system_font` and `theme` are only written when a new `[gui]` section
//! is added); `save_theme` edits only `gui.theme`. Both keep the rest of the
//! file (comments, quoting, indentation) as it is.

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

/// `gui.theme`: the colour theme of deew-gui.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeMode {
    /// Follows the operating system.
    #[default]
    System,
    Light,
    Dark,
}

impl ThemeMode {
    fn as_str(self) -> &'static str {
        match self {
            ThemeMode::System => "system",
            ThemeMode::Light => "light",
            ThemeMode::Dark => "dark",
        }
    }

    fn from_str(value: &str) -> Option<ThemeMode> {
        match value {
            "system" => Some(ThemeMode::System),
            "light" => Some(ThemeMode::Light),
            "dark" => Some(ThemeMode::Dark),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GuiSettings {
    pub progress: ProgressView,
    /// `output.path`: the output directory (`-o`), empty uses the default.
    #[serde(default)]
    pub output: String,
    /// `output.to_input`: save each output next to its input (`-oi`).
    #[serde(default)]
    pub to_input: bool,
    /// `gui.scaling`: zoom of the deew-gui window in percent (80 to 150).
    /// Read-only for the GUI: saving keeps the value in the file.
    #[serde(default = "default_scaling")]
    pub scaling: u16,
    /// `gui.use_system_font`: the operating system's font instead of IBM Plex
    /// Sans. Read-only for the GUI: saving keeps the value in the file.
    #[serde(default)]
    pub use_system_font: bool,
    /// `gui.theme`: 'system', 'light' or 'dark'. Saved on its own by
    /// `save_theme`; saving the other settings keeps the value in the file.
    #[serde(default)]
    pub theme: ThemeMode,
}

fn default_scaling() -> u16 {
    100
}

impl Default for GuiSettings {
    fn default() -> Self {
        GuiSettings {
            progress: ProgressView::default(),
            output: String::new(),
            to_input: false,
            scaling: default_scaling(),
            use_system_font: false,
            theme: ThemeMode::default(),
        }
    }
}

/// `gui.scaling` as a percent: `'125%'`, `'125'` or `125`, clamped to 80..=150.
/// Anything else gives 100.
fn parse_scaling(value: Option<&toml::Value>) -> u16 {
    let number = match value {
        Some(toml::Value::String(s)) => s
            .trim()
            .strip_suffix('%')
            .unwrap_or(s.trim())
            .trim()
            .parse::<i64>()
            .ok(),
        Some(toml::Value::Integer(i)) => Some(*i),
        _ => None,
    };
    number.map_or(100, |n| n.clamp(80, 150) as u16)
}

/// The settings plus read-only values for the window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
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
    /// `default_modes.ac4_ims` of the config (program default when missing).
    pub default_ac4_ims_mode: String,
    /// `surround_phase_shift` of the config (true when missing).
    pub surround_phase_shift: bool,
    /// `low_pass_filter` of the config (true when missing).
    pub low_pass_filter: bool,
    /// `surround_3db_attenuation` of the config (false when missing).
    pub surround_3db_attenuation: bool,
    /// `truehdd_warp_mode` of the config (empty when missing or unknown).
    pub truehdd_warp_mode: String,
    /// `truehdd_bed_conform` of the config (false when missing).
    pub truehdd_bed_conform: bool,
    /// `output.template` of the config (`{stem}` when missing, empty or not a string).
    pub output_template: String,
    /// macOS runs this app from a temporary copy (see `cli::is_translocated`).
    pub translocated: bool,
}

impl Default for SettingsView {
    fn default() -> Self {
        SettingsView {
            settings: GuiSettings::default(),
            default_output: String::new(),
            default_ddp_mode: String::new(),
            default_ddp_atmos_mode: String::new(),
            default_ac4_ims_mode: String::new(),
            surround_phase_shift: true,
            low_pass_filter: true,
            surround_3db_attenuation: false,
            truehdd_warp_mode: String::new(),
            truehdd_bed_conform: false,
            output_template: DEFAULT_OUTPUT_TEMPLATE.to_string(),
            translocated: false,
        }
    }
}

/// The output name template of a config without one.
const DEFAULT_OUTPUT_TEMPLATE: &str = "{stem}";

/// `output.template` of a config text; a missing, empty or non-string value
/// gives `{stem}`.
pub fn parse_output_template(text: &str) -> String {
    text.parse::<toml::Table>()
        .ok()
        .and_then(|d| {
            d.get("output")?
                .get("template")?
                .as_str()
                .map(str::to_string)
        })
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| DEFAULT_OUTPUT_TEMPLATE.to_string())
}

/// The top-level `surround_phase_shift`, `low_pass_filter` and
/// `surround_3db_attenuation` of a config text; a missing key or a value of
/// the wrong type gives the program default (true, true, false).
pub fn parse_dd_filters(text: &str) -> (bool, bool, bool) {
    let doc = text.parse::<toml::Table>().ok();
    let pick = |key: &str, default: bool| {
        doc.as_ref()
            .and_then(|d| d.get(key)?.as_bool())
            .unwrap_or(default)
    };
    (
        pick("surround_phase_shift", true),
        pick("low_pass_filter", true),
        pick("surround_3db_attenuation", false),
    )
}

/// The top-level `truehdd_warp_mode` (empty when missing, empty or not one
/// of `normal`, `warping`, `prologiciix`, `loro`) and `truehdd_bed_conform`
/// (false when missing or of the wrong type) of a config text.
pub fn parse_truehdd_options(text: &str) -> (String, bool) {
    let doc = text.parse::<toml::Table>().ok();
    let warp_mode = doc
        .as_ref()
        .and_then(|d| d.get("truehdd_warp_mode")?.as_str())
        .filter(|v| ["normal", "warping", "prologiciix", "loro"].contains(v))
        .unwrap_or_default()
        .to_string();
    let bed_conform = doc
        .as_ref()
        .and_then(|d| d.get("truehdd_bed_conform")?.as_bool())
        .unwrap_or(false);
    (warp_mode, bed_conform)
}

/// `default_modes.ddp`, `.ddp_atmos` and `.ac4_ims` of a config text; a missing or
/// unknown value gives the program default.
pub fn parse_default_modes(text: &str) -> (String, String, String) {
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
        pick("ac4_ims", &["general", "music"], "general"),
    )
}

impl SettingsView {
    pub fn new(settings: GuiSettings, config: Option<&Path>) -> SettingsView {
        let text = config
            .and_then(|path| std::fs::read_to_string(path).ok())
            .unwrap_or_default();
        let (default_ddp_mode, default_ddp_atmos_mode, default_ac4_ims_mode) =
            parse_default_modes(&text);
        let (surround_phase_shift, low_pass_filter, surround_3db_attenuation) =
            parse_dd_filters(&text);
        let (truehdd_warp_mode, truehdd_bed_conform) = parse_truehdd_options(&text);
        let output_template = parse_output_template(&text);
        SettingsView {
            settings,
            default_ddp_mode,
            default_ddp_atmos_mode,
            default_ac4_ims_mode,
            surround_phase_shift,
            low_pass_filter,
            surround_3db_attenuation,
            truehdd_warp_mode,
            truehdd_bed_conform,
            output_template,
            translocated: crate::cli::is_translocated(),
            default_output: std::env::home_dir()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default(),
        }
    }
}

/// Comment above the `[gui]` section when it is added to an existing config.
const SECTION_COMMENT: &str = "# Settings of deew-gui and deew-web:
# progress: 'cli' shows the terminal output of the CLI, 'native' shows progress bars, warnings and pop-up prompts in the window.
# scaling: zoom of the deew-gui window, '80%' to '150%' (deew-web uses the browser's zoom).
# use_system_font: true uses the system's font instead of the bundled IBM Plex Sans.
# theme: 'system' follows the operating system, 'light' or 'dark' (deew-gui; deew-web keeps its own per browser).
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
    let scaling = parse_scaling(gui.and_then(|g| g.get("scaling")));
    let use_system_font = gui
        .and_then(|g| g.get("use_system_font")?.as_bool())
        .unwrap_or_default();
    let theme = gui
        .and_then(|g| g.get("theme")?.as_str())
        .and_then(ThemeMode::from_str)
        .unwrap_or_default();
    GuiSettings {
        progress,
        output,
        to_input,
        scaling,
        use_system_font,
        theme,
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
            "[gui]\n    progress = '{}'\n    scaling = '{}%'\n    use_system_font = {}\n    theme = '{}'\n",
            settings.progress.as_str(),
            settings.scaling,
            settings.use_system_font,
            settings.theme.as_str()
        ));
    }
    Ok(out)
}

/// Returns the config text with only `gui.theme` set, everything else kept.
/// A missing `[gui]` section is appended with the default values.
pub fn update_theme_text(text: &str, theme: ThemeMode) -> Result<String, String> {
    let mut doc = text
        .parse::<DocumentMut>()
        .map_err(|e| format!("config.toml is not valid TOML: {e}"))?;
    let value: Value = format!("'{}'", theme.as_str())
        .parse()
        .map_err(|e| format!("internal error: {e}"))?;
    if edit_section(&mut doc, "gui", vec![("theme", value)])? {
        return Ok(doc.to_string());
    }
    let mut out = doc.to_string();
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    out.push('\n');
    out.push_str(SECTION_COMMENT);
    let defaults = GuiSettings::default();
    out.push_str(&format!(
        "[gui]\n    progress = '{}'\n    scaling = '{}%'\n    use_system_font = {}\n    theme = '{}'\n",
        defaults.progress.as_str(),
        defaults.scaling,
        defaults.use_system_font,
        theme.as_str()
    ));
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

/// Writes only `gui.theme` into the existing config file.
pub fn save_theme(path: Option<&Path>, theme: ThemeMode) -> Result<(), String> {
    let path = path.filter(|path| path.is_file()).ok_or(NOT_FOUND)?;
    let text =
        std::fs::read_to_string(path).map_err(|e| format!("cannot read config.toml: {e}"))?;
    let updated = update_theme_text(&text, theme)?;
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
            ..GuiSettings::default()
        }
    }

    fn gui_block(value: &str) -> String {
        format!(
            "{SECTION_COMMENT}[gui]\n    progress = '{value}'\n    scaling = '100%'\n    use_system_font = false\n    theme = 'system'\n"
        )
    }

    fn output_block(path: &str, to_input: bool) -> String {
        format!("{OUTPUT_COMMENT}[output]\n    path = {path}\n    to_input = {to_input}\n")
    }

    #[test]
    fn serde_shape() {
        assert_eq!(
            serde_json::to_string(&native()).unwrap(),
            r#"{"progress":"native","output":"","toInput":false,"scaling":100,"useSystemFont":false,"theme":"system"}"#
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
            r#"{"progress":"cli","output":"/m","toInput":false,"scaling":100,"useSystemFont":false,"theme":"system","defaultOutput":"/h","defaultDdpMode":"","defaultDdpAtmosMode":"","defaultAc4ImsMode":"","surroundPhaseShift":true,"lowPassFilter":true,"surround3dbAttenuation":false,"truehddWarpMode":"","truehddBedConform":false,"outputTemplate":"{stem}","translocated":false}"#
        );
        let o: GuiSettings = serde_json::from_str(r#"{"progress":"cli","toInput":true}"#).unwrap();
        assert!(o.to_input);
    }

    #[test]
    fn theme_parse() {
        let t = |text: &str| parse(text).theme;
        assert_eq!(t("[gui]\ntheme = 'dark'\n"), ThemeMode::Dark);
        assert_eq!(t("[gui]\ntheme = \"light\"\n"), ThemeMode::Light);
        assert_eq!(t("[gui]\ntheme = 'system'\n"), ThemeMode::System);
        assert_eq!(t("[gui]\ntheme = 'Dark'\n"), ThemeMode::System);
        assert_eq!(t("[gui]\ntheme = 'x'\n"), ThemeMode::System);
        assert_eq!(t("[gui]\ntheme = 3\n"), ThemeMode::System);
        assert_eq!(t("[gui]\n"), ThemeMode::System);
        assert_eq!(t(""), ThemeMode::System);
        assert_eq!(t("theme = 'dark'\n"), ThemeMode::System);
        let o: GuiSettings = serde_json::from_str(r#"{"progress":"cli","theme":"dark"}"#).unwrap();
        assert_eq!(o.theme, ThemeMode::Dark);
    }

    #[test]
    fn save_theme_edits_only_the_theme_key() {
        let text = "# top\n[gui]\n  progress = \"native\" # a\n  theme = \"light\"  # b\n  scaling = '110%'\n\n[output]\n  path = 'x'\n";
        let updated = update_theme_text(text, ThemeMode::Dark).unwrap();
        assert_eq!(updated, text.replace("\"light\"", "'dark'"));
        assert_eq!(
            update_theme_text(&updated, ThemeMode::Dark).unwrap(),
            updated
        );
        let updated = update_theme_text(TEMPLATE, ThemeMode::Light).unwrap();
        assert_eq!(
            updated,
            TEMPLATE.replace("theme = 'system'", "theme = 'light'")
        );
        assert_eq!(parse(&updated).theme, ThemeMode::Light);
    }

    #[test]
    fn save_theme_adds_the_key_or_section() {
        let text = "[gui]\n  progress = 'cli'\nother = 1\n";
        let updated = update_theme_text(text, ThemeMode::Dark).unwrap();
        assert!(
            updated.starts_with("[gui]\n  progress = 'cli'\nother = 1\n    theme = 'dark'\n"),
            "{updated}"
        );
        let updated = update_theme_text("a = 1", ThemeMode::Dark).unwrap();
        assert_eq!(
            updated,
            format!(
                "a = 1\n\n{}",
                gui_block("cli").replace("theme = 'system'", "theme = 'dark'")
            )
        );
        assert!(update_theme_text("a = [", ThemeMode::Dark).is_err());
        assert!(update_theme_text("gui = 1\n", ThemeMode::Dark).is_err());
    }

    #[test]
    fn saving_progress_and_output_keeps_the_theme() {
        let text = "[gui]\n  progress = 'cli'\n  theme = 'dark'\n\n[output]\n  path = ''\n  to_input = false\n";
        // the settings of the web (no theme in the body) must not touch it
        let body: GuiSettings =
            serde_json::from_str(r#"{"progress":"native","output":"/m","toInput":true}"#).unwrap();
        let updated = update_text(text, &body).unwrap();
        assert_eq!(parse(&updated).theme, ThemeMode::Dark);
        assert!(updated.contains("  theme = 'dark'\n"), "{updated}");
        let other = GuiSettings {
            theme: ThemeMode::Light,
            ..native()
        };
        assert_eq!(
            parse(&update_text(text, &other).unwrap()).theme,
            ThemeMode::Dark
        );
    }

    #[test]
    fn save_theme_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        assert_eq!(
            save_theme(Some(&path), ThemeMode::Dark).unwrap_err(),
            NOT_FOUND
        );
        assert_eq!(save_theme(None, ThemeMode::Dark).unwrap_err(), NOT_FOUND);
        std::fs::write(&path, TEMPLATE).unwrap();
        save_theme(Some(&path), ThemeMode::Dark).unwrap();
        assert_eq!(load(Some(&path)).theme, ThemeMode::Dark);
    }

    #[test]
    fn scaling_parse() {
        let sc = |t: &str| parse(t).scaling;
        assert_eq!(sc(""), 100);
        assert_eq!(sc("[gui]\n"), 100);
        assert_eq!(sc("[gui]\nscaling = '125%'\n"), 125);
        assert_eq!(sc("[gui]\nscaling = ' 125 % '\n"), 125);
        assert_eq!(sc("[gui]\nscaling = '125'\n"), 125);
        assert_eq!(sc("[gui]\nscaling = 125\n"), 125);
        assert_eq!(sc("[gui]\nscaling = '70%'\n"), 80);
        assert_eq!(sc("[gui]\nscaling = '200%'\n"), 150);
        assert_eq!(sc("[gui]\nscaling = -5\n"), 80);
        assert_eq!(sc("[gui]\nscaling = 'abc'\n"), 100);
        assert_eq!(sc("[gui]\nscaling = '12.5%'\n"), 100);
        assert_eq!(sc("[gui]\nscaling = true\n"), 100);
        assert_eq!(sc("[gui]\nscaling = 1.5\n"), 100);
    }

    #[test]
    fn use_system_font_parse() {
        let f = |t: &str| parse(t).use_system_font;
        assert!(f("[gui]\nuse_system_font = true\n"));
        assert!(!f("[gui]\nuse_system_font = false\n"));
        assert!(!f("[gui]\n"));
        assert!(!f(""));
        assert!(!f("[gui]\nuse_system_font = 'true'\n"));
    }

    #[test]
    fn scaling_and_font_are_kept_in_an_existing_gui_section() {
        let text =
            "[gui]\n  progress = 'cli'\n  scaling = \"130%\" # mine\n  use_system_font = true\n";
        let updated = update_text(text, &native()).unwrap();
        assert!(
            updated.starts_with(
                "[gui]\n  progress = 'native'\n  scaling = \"130%\" # mine\n  use_system_font = true\n"
            ),
            "{updated}"
        );
        // a section without the keys does not get them
        let updated = update_text("[gui]\n    progress = 'cli'\n", &native()).unwrap();
        assert!(
            updated.starts_with("[gui]\n    progress = 'native'\n\n"),
            "{updated}"
        );
        assert!(!updated.contains("scaling"), "{updated}");
        assert!(!updated.contains("use_system_font"), "{updated}");
        // other settings in the call do not change the file's values
        let other = GuiSettings {
            scaling: 90,
            use_system_font: false,
            ..native()
        };
        let updated = update_text(text, &other).unwrap();
        assert!(updated.contains("scaling = \"130%\" # mine"), "{updated}");
        assert!(updated.contains("use_system_font = true"), "{updated}");
    }

    #[test]
    fn new_gui_section_has_all_keys() {
        let updated = update_text("", &native()).unwrap();
        assert!(
            updated.ends_with("[gui]\n    progress = 'native'\n    scaling = '100%'\n    use_system_font = false\n    theme = 'system'\n"),
            "{updated}"
        );
        let custom = GuiSettings {
            scaling: 125,
            use_system_font: true,
            theme: ThemeMode::Dark,
            ..native()
        };
        let updated = update_text("", &custom).unwrap();
        assert!(
            updated.ends_with(
                "    scaling = '125%'\n    use_system_font = true\n    theme = 'dark'\n"
            ),
            "{updated}"
        );
        assert_eq!(parse(&updated), custom);
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
        let s = |a: &str, b: &str, c: &str| (a.to_string(), b.to_string(), c.to_string());
        assert_eq!(d(""), s("standard", "streaming", "general"));
        assert_eq!(d(TEMPLATE), s("standard", "streaming", "general"));
        assert_eq!(
            d("[default_modes]\nddp = 'bluray'\nddp_atmos = 'standard'\nac4_ims = 'music'\n"),
            s("bluray", "standard", "music")
        );
        // unknown values give the program default
        assert_eq!(
            d("[default_modes]\nddp = 'x'\nddp_atmos = 3\nac4_ims = 'x'\n"),
            s("standard", "streaming", "general")
        );
    }

    #[test]
    fn dd_filters_come_from_the_config() {
        let f = |t: &str| parse_dd_filters(t);
        assert_eq!(f(""), (true, true, false));
        assert_eq!(f("not toml [[["), (true, true, false));
        assert_eq!(f(TEMPLATE), (true, true, false));
        assert_eq!(
            f(
                "surround_phase_shift = false\nlow_pass_filter = false\nsurround_3db_attenuation = true\n"
            ),
            (false, false, true)
        );
        // wrong types give the default
        assert_eq!(
            f(
                "surround_phase_shift = 'no'\nlow_pass_filter = 0\nsurround_3db_attenuation = 'yes'\n"
            ),
            (true, true, false)
        );
        // only top-level keys count
        assert_eq!(f("[gui]\nlow_pass_filter = false\n"), (true, true, false));
    }

    #[test]
    fn truehdd_options_come_from_the_config() {
        let f = |t: &str| parse_truehdd_options(t);
        assert_eq!(f(""), (String::new(), false));
        assert_eq!(f("not toml [[["), (String::new(), false));
        assert_eq!(f(TEMPLATE), (String::new(), false));
        assert_eq!(
            f("truehdd_warp_mode = 'loro'\ntruehdd_bed_conform = true\n"),
            ("loro".to_string(), true)
        );
        for mode in ["normal", "warping", "prologiciix", "loro"] {
            assert_eq!(f(&format!("truehdd_warp_mode = '{mode}'\n")).0, mode);
        }
        // wrong types and unknown values give the default
        assert_eq!(
            f("truehdd_warp_mode = 'Loro'\ntruehdd_bed_conform = 'yes'\n"),
            (String::new(), false)
        );
        assert_eq!(f("truehdd_warp_mode = 3\n"), (String::new(), false));
        // only top-level keys count
        assert_eq!(
            f("[gui]\ntruehdd_warp_mode = 'loro'\ntruehdd_bed_conform = true\n"),
            (String::new(), false)
        );
    }

    #[test]
    fn output_template_comes_from_the_config() {
        let f = |t: &str| parse_output_template(t);
        assert_eq!(f(""), "{stem}");
        assert_eq!(f("not toml [[["), "{stem}");
        assert_eq!(f(TEMPLATE), "{stem}");
        assert_eq!(f("[output]\npath = ''\n"), "{stem}");
        assert_eq!(f("[output]\ntemplate = ''\n"), "{stem}");
        assert_eq!(f("[output]\ntemplate = 3\n"), "{stem}");
        assert_eq!(f("[output]\ntemplate = '{stem}.{lang}'\n"), "{stem}.{lang}");
        // only the [output] table counts
        assert_eq!(f("template = '{name}'\n[gui]\ntemplate = 'x'\n"), "{stem}");
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "[output]\ntemplate = '{name} {codec}'\n").unwrap();
        let view = SettingsView::new(GuiSettings::default(), Some(&path));
        assert_eq!(view.output_template, "{name} {codec}");
        let view = SettingsView::new(GuiSettings::default(), None);
        assert_eq!(view.output_template, "{stem}");
    }

    #[test]
    fn settings_view_reads_the_truehdd_options_from_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(
            &path,
            "truehdd_warp_mode = 'warping'\ntruehdd_bed_conform = true\n",
        )
        .unwrap();
        let view = SettingsView::new(GuiSettings::default(), Some(&path));
        assert_eq!(view.truehdd_warp_mode, "warping");
        assert!(view.truehdd_bed_conform);
        let view = SettingsView::new(GuiSettings::default(), None);
        assert!(view.truehdd_warp_mode.is_empty() && !view.truehdd_bed_conform);
    }

    #[test]
    fn settings_view_reads_the_filters_from_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(
            &path,
            "surround_phase_shift = false\nsurround_3db_attenuation = true\n",
        )
        .unwrap();
        let view = SettingsView::new(GuiSettings::default(), Some(&path));
        assert!(!view.surround_phase_shift);
        assert!(view.low_pass_filter);
        assert!(view.surround_3db_attenuation);
        let view = SettingsView::new(GuiSettings::default(), None);
        assert!(
            view.surround_phase_shift && view.low_pass_filter && !view.surround_3db_attenuation
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
