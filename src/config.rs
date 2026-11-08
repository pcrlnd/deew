//! Config file handling: locations, generation, display and loading.

use crate::data::{ErrorKind, LOGOS};
use crate::error::{error_line, escape, print_exit};
use crate::rprintln;
use crate::toml_highlight::highlight;
use serde::Deserialize;
use std::path::{Path, PathBuf};

pub const CONFIG_TEMPLATE: &str = include_str!("config_template.toml");

/// Top-level keys that have to be present in the config.
pub const CONFIG_KEYS: [&str; 5] = [
    "temp_path",
    "logo",
    "max_instances",
    "default_bitrates",
    "summary_sections",
];

/// `max_instances` is either a number or a string like `'50%'`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum MaxInstances {
    Count(i64),
    Text(String),
}

/// Defaults of the keys that were added later: configs without them still load.
fn d_7_1_standard() -> i64 {
    1536
}
fn d_7_1_bluray() -> i64 {
    1536
}
fn d_atmos_streaming() -> i64 {
    1024
}
fn d_atmos_standard() -> i64 {
    1536
}
fn d_atmos_bluray() -> i64 {
    1536
}

#[derive(Debug, Clone, Deserialize)]
pub struct DefaultBitrates {
    pub dd_1_0: i64,
    pub dd_2_0: i64,
    pub dd_5_1: i64,
    pub ddp_1_0: i64,
    pub ddp_2_0: i64,
    pub ddp_5_1: i64,
    #[serde(default = "d_7_1_standard")]
    pub ddp_7_1_standard: i64,
    #[serde(default = "d_7_1_bluray")]
    pub ddp_7_1_bluray: i64,
    #[serde(default = "d_atmos_streaming")]
    pub ddp_atmos_streaming: i64,
    #[serde(default = "d_atmos_standard")]
    pub ddp_atmos_standard: i64,
    #[serde(default = "d_atmos_bluray")]
    pub ddp_atmos_bluray: i64,
    pub ac4_2_0: i64,
    pub ac4_5_1: i64,
    pub ac4_atmos: i64,
    pub aac_2_0: i64,
}

/// The profile of 7.1 DDP (`--ddp-mode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DdpMode {
    Standard,
    Bluray,
}

impl DdpMode {
    pub fn parse(s: &str) -> Option<DdpMode> {
        match s {
            "standard" => Some(DdpMode::Standard),
            "bluray" => Some(DdpMode::Bluray),
            _ => None,
        }
    }
}

/// The profile of DDP Atmos (`--ddp-atmos-mode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DdpAtmosMode {
    Streaming,
    Standard,
    Bluray,
}

impl DdpAtmosMode {
    pub fn parse(s: &str) -> Option<DdpAtmosMode> {
        match s {
            "streaming" => Some(DdpAtmosMode::Streaming),
            "standard" => Some(DdpAtmosMode::Standard),
            "bluray" => Some(DdpAtmosMode::Bluray),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            DdpAtmosMode::Streaming => "streaming",
            DdpAtmosMode::Standard => "standard",
            DdpAtmosMode::Bluray => "bluray",
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct DefaultModes {
    #[serde(default = "default_ddp_mode")]
    pub ddp: DdpMode,
    #[serde(default = "default_atmos_mode")]
    pub ddp_atmos: DdpAtmosMode,
}

fn default_ddp_mode() -> DdpMode {
    DdpMode::Standard
}

fn default_atmos_mode() -> DdpAtmosMode {
    DdpAtmosMode::Streaming
}

impl Default for DefaultModes {
    fn default() -> Self {
        DefaultModes {
            ddp: default_ddp_mode(),
            ddp_atmos: default_atmos_mode(),
        }
    }
}

/// `[output]`: where the outputs go.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct OutputConfig {
    /// Empty: the current directory. `-o` overrides it.
    #[serde(default)]
    pub path: String,
    /// Write each output next to its input (`-oi`).
    #[serde(default)]
    pub to_input: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SummarySections {
    pub deew_info: bool,
    pub binaries: bool,
    pub input_info: bool,
    pub output_info: bool,
    pub other: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub temp_path: String,
    /// Only checked to be a number; the logo is printed from the raw table
    /// before the other keys are validated.
    #[serde(rename = "logo")]
    pub _logo: i64,
    pub max_instances: MaxInstances,
    pub default_bitrates: DefaultBitrates,
    pub summary_sections: SummarySections,
    #[serde(default)]
    pub output: OutputConfig,
    #[serde(default)]
    pub default_modes: DefaultModes,
}

#[derive(Debug, Clone)]
pub struct Paths {
    /// The only config location: `config.toml` next to the executable.
    pub config: PathBuf,
    pub exe_dir: PathBuf,
}

impl Paths {
    /// Paths of the standalone build, based on the running executable.
    pub fn resolve() -> Paths {
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(Path::to_path_buf))
            .unwrap_or_else(|| PathBuf::from("."));
        Paths {
            config: exe_dir.join("config.toml"),
            exe_dir,
        }
    }
}

/// `-c`: show the config location and the current config.
pub fn show_config(paths: &Paths) -> ! {
    rprintln!(
        "[bold cyan]Your config location:[/bold cyan]\n{}\n\n[bold cyan]Your current config:[/bold cyan]",
        escape(&paths.config.display().to_string())
    );
    if !paths.config.exists() {
        rprintln!("You don't have a config currently.");
        std::process::exit(0);
    }
    match std::fs::read_to_string(&paths.config) {
        Ok(content) => rprintln!("{}", highlight(&content)),
        Err(e) => error_line(&escape(&format!(
            "failed to read {}: {e}",
            paths.config.display()
        ))),
    }
    std::process::exit(0);
}

/// Write the template next to the executable and exit with `exit_code`:
/// 0 for an explicit `-gc` (it did what was asked), 1 when the config was
/// auto-created because it was missing (the requested run did not happen).
/// A failed write always exits 1 via `error_line`.
pub fn generate_config(paths: &Paths, exit_code: i32) -> ! {
    let target = &paths.config;
    if let Err(e) = std::fs::write(target, CONFIG_TEMPLATE) {
        error_line(&escape(&format!(
            "failed to write {}: {e}",
            target.display()
        )));
    }
    println!();
    rprintln!("{}", highlight(CONFIG_TEMPLATE));
    rprintln!(
        "\n[bold cyan]The above config has been created at:[/bold cyan]\n{}",
        escape(&target.display().to_string())
    );
    std::process::exit(exit_code);
}

fn read_table(path: &Path) -> Result<toml::Table, String> {
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    text.parse::<toml::Table>().map_err(|e| e.to_string())
}

/// The default bitrates and modes for `-lb`: the config's values, the program
/// defaults (the template's) for what is missing or when there is no usable config.
pub fn default_values(paths: &Paths) -> (DefaultBitrates, DefaultModes) {
    default_values_of(read_table(&paths.config).ok())
}

pub fn default_values_of(user: Option<toml::Table>) -> (DefaultBitrates, DefaultModes) {
    let template: toml::Table = CONFIG_TEMPLATE.parse().expect("template parses");
    let fallback: Config = template
        .clone()
        .try_into()
        .expect("template is a valid config");
    let Some(user) = user else {
        return (fallback.default_bitrates, fallback.default_modes);
    };
    let mut merged = template;
    for section in ["default_bitrates", "default_modes"] {
        if let (Some(toml::Value::Table(base)), Some(toml::Value::Table(over))) =
            (merged.get_mut(section), user.get(section))
        {
            for (k, v) in over {
                base.insert(k.clone(), v.clone());
            }
        }
    }
    match merged.try_into::<Config>() {
        Ok(c) => (c.default_bitrates, c.default_modes),
        Err(_) => (fallback.default_bitrates, fallback.default_modes),
    }
}

/// Load the raw config table from the config file.
fn load_table(paths: &Paths) -> Result<(PathBuf, toml::Table), String> {
    let path = &paths.config;
    match read_table(path) {
        Ok(table) => Ok((path.clone(), table)),
        Err(e) => Err(format!("{}:\n{}", path.display(), e.trim_end())),
    }
}

/// Missing top-level keys, in config order.
pub fn missing_keys(table: &toml::Table) -> Vec<&'static str> {
    CONFIG_KEYS
        .into_iter()
        .filter(|k| !table.contains_key(*k))
        .collect()
}

/// Load the config (printing the logo), exiting with an error message if it
/// is unusable. Generates a config first if none exists.
pub fn load(paths: &Paths) -> Config {
    if !paths.config.exists() {
        rprintln!(
            "[bold yellow]config.toml[/bold yellow] [not bold white]is missing, creating one...[/not bold white]"
        );
        generate_config(paths, 1);
    }

    let (path, table) = match load_table(paths) {
        Ok(loaded) => loaded,
        Err(e) => error_line(&escape(&format!("failed to parse config.toml: {e}"))),
    };

    if let Some(logo) = table.get("logo").and_then(|v| v.as_integer())
        && 0 < logo
        && logo <= LOGOS.len() as i64
    {
        rprintln!("{}", LOGOS[(logo - 1) as usize]);
    }

    let missing = missing_keys(&table);
    if !missing.is_empty() {
        print_exit(
            ErrorKind::ConfigKey,
            Some(&format!(
                "[bold yellow]{}[/bold yellow]",
                missing.join("[not bold white], [/not bold white]")
            )),
        );
    }

    match table.try_into::<Config>() {
        Ok(config) => config,
        Err(e) => error_line(&escape(&format!(
            "invalid config {}: {}",
            path.display(),
            e.message()
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_deserializes() {
        let config: Config = toml::from_str(CONFIG_TEMPLATE).unwrap();
        assert_eq!(config.temp_path, "");
        assert_eq!(config.max_instances, MaxInstances::Text("50%".into()));
        assert_eq!(config.default_bitrates.ddp_5_1, 1024);
        let d = &config.default_bitrates;
        assert_eq!((d.ac4_2_0, d.ac4_5_1, d.ac4_atmos), (256, 448, 448));
        assert_eq!((d.ddp_atmos_streaming, d.aac_2_0), (1024, 256));
        assert_eq!((d.ddp_7_1_standard, d.ddp_atmos_bluray), (1536, 1536));
        assert_eq!(config.output.path, "");
        assert!(!config.output.to_input);
        assert_eq!(config.default_modes.ddp, DdpMode::Standard);
        assert_eq!(config.default_modes.ddp_atmos, DdpAtmosMode::Streaming);
        assert!(config.summary_sections.deew_info && config.summary_sections.other);
    }

    #[test]
    fn deew_web_section_is_optional() {
        assert!(!CONFIG_KEYS.contains(&"deew_web"));
        assert!(CONFIG_TEMPLATE.contains("[deew_web]"));
        let t = CONFIG_TEMPLATE
            .split("# Login for deew-web")
            .next()
            .unwrap();
        assert!(!t.contains("deew_web"));
        toml::from_str::<Config>(t).unwrap();
    }

    #[test]
    fn max_instances_number() {
        let t = CONFIG_TEMPLATE.replace("max_instances = '50%'", "max_instances = 4");
        let config: Config = toml::from_str(&t).unwrap();
        assert_eq!(config.max_instances, MaxInstances::Count(4));
    }

    #[test]
    fn missing_sub_key_is_an_error() {
        for key in ["ac4_atmos", "ddp_5_1", "aac_2_0", "ac4_5_1"] {
            let t: String = CONFIG_TEMPLATE
                .lines()
                .filter(|l| !l.trim_start().starts_with(&format!("{key} =")))
                .map(|l| format!("{l}\n"))
                .collect();
            let table: toml::Table = t.parse().unwrap();
            let e = table.try_into::<Config>().unwrap_err();
            assert!(e.message().contains(key), "{}", e.message());
        }
    }

    #[test]
    fn old_tool_keys_are_ignored() {
        let old = format!("dee_path = '/x'\nffmpeg_path = 'C:/ffmpeg.exe'\n{CONFIG_TEMPLATE}");
        toml::from_str::<Config>(&old).unwrap();
    }

    #[test]
    fn old_and_missing_new_keys_use_the_defaults() {
        let old = "temp_path = ''\nlogo = 1\nmax_instances = 4\n\
            [default_bitrates]\ndd_1_0 = 128\ndd_2_0 = 256\ndd_5_1 = 640\nddp_1_0 = 128\n\
            ddp_2_0 = 256\nddp_5_1 = 1024\nddp_7_1 = 1536\nddp_atmos = 768\nac4_2_0 = 256\n\
            ac4_5_1 = 448\nac4_atmos = 448\naac_2_0 = 256\n\
            [summary_sections]\ndeew_info = true\nbinaries = true\ninput_info = true\n\
            output_info = true\nother = true\n[gui]\noutput = '/x'\n";
        let config: Config = toml::from_str(old).unwrap();
        assert_eq!(config.default_bitrates.ddp_atmos_streaming, 1024);
        assert_eq!(config.default_bitrates.ddp_7_1_standard, 1536);
        assert_eq!(config.default_modes.ddp_atmos, DdpAtmosMode::Streaming);
        assert!(config.output.path.is_empty());
    }

    #[test]
    fn invalid_mode_is_an_error() {
        let t = CONFIG_TEMPLATE.replace("ddp = 'standard'", "ddp = 'nope'");
        let e = toml::from_str::<Config>(&t).unwrap_err();
        assert!(e.message().contains("nope"), "{}", e.message());
        let t = CONFIG_TEMPLATE.replace("ddp_atmos = 'streaming'", "ddp_atmos = 'x'");
        assert!(toml::from_str::<Config>(&t).is_err());
    }

    #[test]
    fn finds_missing_keys() {
        let table: toml::Table = "logo = 1\ntemp_path = ''\n".parse().unwrap();
        assert_eq!(
            missing_keys(&table),
            ["max_instances", "default_bitrates", "summary_sections"]
        );
        let full: toml::Table = CONFIG_TEMPLATE.parse().unwrap();
        assert!(missing_keys(&full).is_empty());
    }
}
