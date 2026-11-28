//! Informational commands: `-lb`, `-pl` and `-cl`.

use crate::config::{self, DdpAtmosMode, DdpMode, DefaultBitrates, DefaultModes, Paths};
use crate::data::{ErrorKind, LOGOS, allowed_bitrates};
use crate::error::{escape, print_exit};
use crate::live::Live;
use crate::markup::{balanced, color_enabled, render};
use crate::rprintln;
use crate::summary::version_cell;
use crate::tools;
#[cfg(test)]
use crate::tools::Versions;
use crate::util::find_closest_allowed;
use base64::Engine;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use unicode_width::UnicodeWidthChar;

const CHANGELOG_URL: &str = "https://api.github.com/repos/pcrlnd/deew/contents/changelog.md";

/// The tools whose `--help` tells the DEE version, the first one that exists
/// is asked.
const DEE_TOOLS: [&str; 8] = [
    "dee_ddp_encoder",
    "dee_ddpjoc_encoder",
    "dee_ac4_encoder",
    "dee_ac4ajoc_encoder",
    "dee_ac4ims_encoder",
    "dee_aac_encoder",
    "dee_dthd_encoder",
    "dee_measure_loudness",
];

/// The lines of `deew -v`: deew itself, then the versions of the tools as in
/// the encoding summary, each followed by where it was found (`exe` for deew,
/// `paths` in the order of the tools; nothing for one that was not found).
/// deew-gui and deew-web (`apps`, read from their files) come right after deew. The first line is
/// read by deew-gui.
#[cfg(test)]
pub fn version_lines(
    versions: &Versions,
    paths: &[Option<String>; 6],
    apps: &AppRows,
    exe: Option<&str>,
) -> Vec<String> {
    let known = [
        &versions.dee,
        &versions.truehdd,
        &versions.harletty,
        &versions.shim,
        &versions.ffmpeg,
        &versions.ffprobe,
    ]
    .map(|v| Some(v.clone()));
    partial_version_lines(&known, paths, apps, exe)
}

/// Shown for a tool whose probe has not finished.
const PENDING: &str = "[dim]…[/dim]";

/// Version and path of deew-gui and deew-web.
pub type AppRows = [(tools::Version, Option<String>); 2];

/// [`version_lines`] while the probes are still running: a tool without a
/// result (`None`) shows a placeholder and no path, and does not count for
/// the width of the version column.
pub fn partial_version_lines(
    versions: &[Option<tools::Version>; 6],
    paths: &[Option<String>; 6],
    apps: &AppRows,
    exe: Option<&str>,
) -> Vec<String> {
    // deew-gui and deew-web come right after deew
    let versions: Vec<Option<tools::Version>> = apps
        .iter()
        .map(|(v, _)| Some(v.clone()))
        .chain(versions.iter().cloned())
        .collect();
    let paths: Vec<Option<String>> = apps
        .iter()
        .map(|(_, p)| p.clone())
        .chain(paths.iter().cloned())
        .collect();
    let own = env!("CARGO_PKG_VERSION");
    let found = |i: usize| {
        paths[i].is_some()
            && versions[i]
                .as_ref()
                .is_some_and(|v| *v != tools::Version::NotFound)
    };
    let plain: Vec<usize> = versions
        .iter()
        .map(|v| v.as_ref().map_or(0, |v| v.text().chars().count()))
        .collect();
    // the paths start two columns after the longest version that has one
    let own_width = if exe.is_some() {
        own.chars().count()
    } else {
        0
    };
    let column = (0..versions.len())
        .filter(|&i| found(i))
        .map(|i| plain[i])
        .max()
        .unwrap_or(0)
        .max(own_width)
        + 2;
    let own_path = match exe {
        Some(exe) => format!(
            "{}[not bold white]{}[/not bold white]",
            " ".repeat(column - own.chars().count()),
            escape(exe)
        ),
        None => String::new(),
    };
    let mut lines = vec![format!(
        "[bold cyan]{:<9}[/bold cyan][not bold white]{own}[/not bold white]{own_path}",
        "deew"
    )];
    let labels = [
        "deew-gui", "deew-web", "DEE", "truehdd", "harletty", "dee_shim", "ffmpeg", "ffprobe",
    ];
    for (i, label) in labels.into_iter().enumerate() {
        let cell = versions[i]
            .as_ref()
            .map_or(PENDING.to_string(), version_cell);
        let path = match &paths[i] {
            Some(path) if found(i) => format!(
                "{}[not bold white]{}[/not bold white]",
                " ".repeat(column - plain[i]),
                escape(path)
            ),
            _ => String::new(),
        };
        lines.push(format!(
            "[bold cyan]{label:<9}[/bold cyan][not bold white]{}[/not bold white]{path}",
            balanced(&cell)
        ));
    }
    lines
}

/// `-v`: print the version and the versions of the tools (probed at the same
/// time, a missing tool just shows as not found).
pub fn print_version() -> ! {
    let paths = config::Paths::resolve();
    let (cwd, exe_dir) = (std::env::current_dir().unwrap_or_default(), &paths.exe_dir);
    let dee = DEE_TOOLS
        .iter()
        .find_map(|tool| tools::dee_tool(tool, &cwd, exe_dir));
    let truehdd = tools::find_truehdd(exe_dir, &cwd);
    let harletty = tools::find_harletty(exe_dir, &cwd);
    let shim = tools::find_shim(&cwd, exe_dir);
    // the programs that would run
    let program = |name: &str| tools::find_program(name, exe_dir, &cwd).unwrap_or(name.to_string());
    let (ffmpeg, ffprobe) = (program("ffmpeg"), program("ffprobe"));
    let dee_dir = dee.as_deref().and_then(|tool| {
        let parent = Path::new(tool).parent()?;
        Some(parent.to_string_lossy().into_owned())
    });
    let paths = [
        dee_dir,
        truehdd.clone(),
        harletty.clone(),
        shim.as_ref().map(|shim| shim.run.clone()),
        Some(ffmpeg.clone()),
        Some(ffprobe.clone()),
    ];
    let exe = std::env::current_exe().ok().map(|exe| {
        let exe = exe.canonicalize().unwrap_or(exe);
        let text = exe.to_string_lossy().into_owned();
        // Windows canonical paths come with a verbatim prefix
        match text.strip_prefix(r"\\?\") {
            Some(plain) => plain.to_string(),
            None => text,
        }
    });
    let apps = crate::update::installed_apps(exe_dir);
    let results: Arc<Mutex<[Option<tools::Version>; 6]>> = Arc::default();
    let live = {
        let (results, paths, apps, exe) =
            (results.clone(), paths.clone(), apps.clone(), exe.clone());
        let color = color_enabled();
        Live::new(move |_| {
            let known = results.lock().expect("versions").clone();
            partial_version_lines(&known, &paths, &apps, exe.as_deref())
                .iter()
                .map(|line| render(line, color))
                .collect()
        })
    };
    live.redraw();
    tools::probe_versions_each(
        dee.as_deref(),
        truehdd.as_deref(),
        harletty.as_deref(),
        shim.as_ref(),
        &ffmpeg,
        &ffprobe,
        |index, version| {
            results.lock().expect("versions")[index] = Some(version);
            live.redraw();
        },
    );
    if live.is_hidden() {
        live.finish(false);
        let known = results.lock().expect("versions").clone();
        for line in partial_version_lines(&known, &paths, &apps, exe.as_deref()) {
            rprintln!("{}", line);
        }
    } else {
        live.finish(true);
    }
    std::process::exit(0);
}

/// Widest line of `-lb`, in visible characters, on a wide terminal.
const BITRATE_LINE_WIDTH: usize = 78;

/// Lay out `items` (markup, visible width) after `prefix` (markup, visible
/// width), separated by ", ". A line, including the trailing comma, is at
/// most `max` visible characters wide; the continuation lines are indented to
/// the column where the values start.
pub fn wrap_items(
    prefix: &str,
    prefix_width: usize,
    items: &[(String, usize)],
    max: usize,
) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = prefix.to_string();
    let mut width = prefix_width;
    let mut first = true;
    for (i, (markup, w)) in items.iter().enumerate() {
        let comma = usize::from(i + 1 < items.len());
        // the separating space needs one more column
        let needed = w + comma + usize::from(!first);
        if !first && width + needed > max {
            lines.push(line);
            line = " ".repeat(prefix_width);
            width = prefix_width;
            first = true;
        }
        if !first {
            line.push(' ');
            width += 1;
        }
        line.push_str(markup);
        width += w;
        if comma == 1 {
            line.push_str("[white],[/white]");
            width += 1;
        }
        first = false;
    }
    lines.push(line);
    lines
}

/// A node of the `-lb` tree.
enum Node {
    Leaf {
        label: &'static str,
        /// The `--ddp-mode` / `--ddp-atmos-mode` value that is the default.
        default_mode: bool,
        table: &'static str,
        /// The configured default bitrate.
        default: i64,
        /// All rates need dee_shim.
        blue: bool,
    },
    Parent {
        label: &'static str,
        children: Vec<Node>,
    },
}

fn leaf(label: &'static str, table: &'static str, default: i64) -> Node {
    Node::Leaf {
        label,
        default_mode: false,
        table,
        default,
        blue: false,
    }
}

fn mode_leaf(label: &'static str, table: &'static str, default: i64, default_mode: bool) -> Node {
    Node::Leaf {
        label,
        default_mode,
        table,
        default,
        blue: table.starts_with("ddp_atmos") || table == "ddp_71",
    }
}

fn render_nodes(nodes: &[Node], prefix: &str, width: usize, lines: &mut Vec<String>) {
    const DEFAULT: &str = " (default)";
    let label_width = nodes
        .iter()
        .filter_map(|n| match n {
            Node::Leaf {
                label,
                default_mode,
                ..
            } => Some(label.len() + if *default_mode { DEFAULT.len() } else { 0 }),
            Node::Parent { .. } => None,
        })
        .max()
        .unwrap_or(0);
    for (i, node) in nodes.iter().enumerate() {
        let last = i + 1 == nodes.len();
        let connector = if last { "└─ " } else { "├─ " };
        let cont = if last { "   " } else { "│  " };
        match node {
            Node::Parent { label, children } => {
                lines.push(format!(
                    "{prefix}{connector}[bold magenta]{label}[/bold magenta]:"
                ));
                render_nodes(children, &format!("{prefix}{cont}"), width, lines);
            }
            Node::Leaf {
                label,
                default_mode,
                table,
                default,
                blue,
            } => {
                let list = allowed_bitrates(table);
                let closest = find_closest_allowed(*default, list);
                let items: Vec<(String, usize)> = list
                    .iter()
                    .map(|b| {
                        let width = b.to_string().len();
                        let value = if *blue {
                            format!("[bold bright_blue]{b}[/bold bright_blue]")
                        } else {
                            format!("[bold color(231)]{b}[/bold color(231)]")
                        };
                        if *b == closest {
                            (
                                format!("{value}[color(231)]{DEFAULT}[/color(231)]"),
                                width + DEFAULT.len(),
                            )
                        } else {
                            (value, width)
                        }
                    })
                    .collect();
                let suffix = if *default_mode { DEFAULT } else { "" };
                let used = label.len() + suffix.len();
                let pad = " ".repeat(label_width - used + 1);
                let head = format!("{prefix}{connector}");
                let head_width = head.chars().count();
                let line_prefix =
                    format!("{head}[bold magenta]{label}[/bold magenta]{suffix}:{pad}");
                let prefix_width = head_width + label_width + 2;
                let mut wrapped = wrap_items(&line_prefix, prefix_width, &items, width);
                let indent = format!("{prefix}{cont}");
                let blank = " ".repeat(head_width);
                for line in wrapped.iter_mut().skip(1) {
                    if let Some(rest) = line.strip_prefix(&blank) {
                        *line = format!("{indent}{rest}");
                    }
                }
                lines.extend(wrapped);
            }
        }
    }
}

/// The lines of `-lb`, with the configured default bitrates and modes
/// marked. The rates that need dee_shim are blue.
pub fn bitrate_lines(d: &DefaultBitrates, modes: &DefaultModes, width: usize) -> Vec<String> {
    let groups = [
        (
            "DD",
            vec![
                leaf("1.0", "dd_10", d.dd_1_0),
                leaf("2.0", "dd_20", d.dd_2_0),
                leaf("5.1", "dd_51", d.dd_5_1),
            ],
        ),
        (
            "DDP",
            vec![
                leaf("1.0", "ddp_10", d.ddp_1_0),
                leaf("2.0", "ddp_20", d.ddp_2_0),
                leaf("5.1", "ddp_51", d.ddp_5_1),
                Node::Parent {
                    label: "7.1 (--ddp-mode)",
                    children: vec![
                        mode_leaf(
                            "standard",
                            "ddp_71",
                            d.ddp_7_1_standard,
                            modes.ddp == DdpMode::Standard,
                        ),
                        mode_leaf(
                            "bluray",
                            "ddp_71_bluray",
                            d.ddp_7_1_bluray,
                            modes.ddp == DdpMode::Bluray,
                        ),
                    ],
                },
                Node::Parent {
                    label: "Atmos (--ddp-atmos-mode)",
                    children: vec![
                        mode_leaf(
                            "streaming",
                            "ddp_atmos_streaming",
                            d.ddp_atmos_streaming,
                            modes.ddp_atmos == DdpAtmosMode::Streaming,
                        ),
                        mode_leaf(
                            "standard",
                            "ddp_atmos_standard",
                            d.ddp_atmos_standard,
                            modes.ddp_atmos == DdpAtmosMode::Standard,
                        ),
                        mode_leaf(
                            "bluray",
                            "ddp_atmos_bluray",
                            d.ddp_atmos_bluray,
                            modes.ddp_atmos == DdpAtmosMode::Bluray,
                        ),
                    ],
                },
            ],
        ),
        (
            "AC4",
            vec![
                leaf("2.0", "ac4_20", d.ac4_2_0),
                leaf("5.1", "ac4_51", d.ac4_5_1),
                leaf("Atmos", "ac4_atmos", d.ac4_atmos),
                leaf("IMS", "ac4_ims", d.ac4_ims),
            ],
        ),
        ("AAC", vec![leaf("2.0", "aac_20", d.aac_2_0)]),
    ];
    let mut lines = Vec::new();
    for (name, nodes) in &groups {
        lines.push(format!("[bold magenta]{name}[/bold magenta]:"));
        render_nodes(nodes, "", width, &mut lines);
    }
    lines.push("[bold bright_blue]blue[/bold bright_blue]: uses dee_shim".to_string());
    lines
}

pub fn list_bitrates(paths: &Paths) -> ! {
    let (bitrates, modes) = config::default_values(paths);
    let width = BITRATE_LINE_WIDTH.min(crate::term::console_width().saturating_sub(2));
    for line in bitrate_lines(&bitrates, &modes, width) {
        rprintln!("{}", line);
    }
    std::process::exit(0);
}

pub fn print_logos() -> ! {
    for (i, logo) in LOGOS.iter().enumerate() {
        rprintln!("logo {}:\n{}", i + 1, logo);
    }
    std::process::exit(0);
}

/// Keep the last 10 entries of the reversed changelog (so the 10 oldest
/// entries of the file, newest of those first), split into lines like the
/// Python version.
pub fn changelog_lines(content: &str) -> Vec<&str> {
    let mut entries: Vec<&str> = content.split("\n\n").collect();
    entries.reverse();
    let start = entries.len().saturating_sub(10);
    let kept = &entries[start..];
    let mut lines = Vec::new();
    for (i, entry) in kept.iter().enumerate() {
        if i > 0 {
            // the "\n\n" separator between entries
            lines.push("");
        }
        lines.extend(entry.split('\n'));
    }
    lines
}

/// Transform one changelog line into printable markup.
///
/// A trailing backslash (markdown line break) is dropped, `[` is escaped so
/// the raw text is never parsed as markup, `# ` headings become bold and
/// backticks alternate between opening and closing a yellow span.
pub fn changelog_markup(line: &str) -> String {
    markup_raw(line.strip_suffix('\\').unwrap_or(line))
}

fn markup_raw(line: &str) -> String {
    let mut line = escape(line);
    if line.starts_with("# ") {
        line = format!(
            "[bold color(231)]{}[/bold color(231)]",
            line.replace("# ", "")
        );
    }
    let mut out = String::with_capacity(line.len());
    let mut open = false;
    for c in line.chars() {
        if c == '`' {
            out.push_str(if open {
                "[/bold yellow]"
            } else {
                "[bold yellow]"
            });
            open = !open;
        } else {
            out.push(c);
        }
    }
    format!("[not bold white]{out}[/not bold white]")
}

/// Columns the text of a line starts at (after the indent and a `- `, `* `
/// or `1. ` list marker).
fn hanging_indent(line: &str) -> usize {
    let rest = line.trim_start_matches(' ');
    let lead = line.len() - rest.len();
    let marker = if rest.starts_with("- ") || rest.starts_with("* ") {
        2
    } else {
        let digits = rest.chars().take_while(char::is_ascii_digit).count();
        if digits > 0 && rest[digits..].starts_with(". ") {
            digits + 2
        } else {
            0
        }
    };
    lead + marker
}

/// Display width of raw text, not counting the backticks (never printed).
fn raw_width(s: &str) -> usize {
    s.chars()
        .filter(|&c| c != '`')
        .map(|c| c.width().unwrap_or(0))
        .sum()
}

/// Wrap one raw line at spaces to `width` columns, continuation lines
/// indented by the hanging indent. A word wider than `width` stays whole.
fn wrap_raw(line: &str, width: usize) -> Vec<String> {
    if raw_width(line) <= width {
        return vec![line.to_string()];
    }
    let hang = hanging_indent(line);
    let indent = " ".repeat(hang);
    let mut words = line[hang.min(line.len())..].split(' ');
    let mut cur = line[..hang.min(line.len())].to_string();
    let mut cur_width = hang;
    let mut has_word = false;
    let mut lines = Vec::new();
    // The leading indent and marker belong to the first word.
    if let Some(first) = words.next() {
        cur.push_str(first);
        cur_width += raw_width(first);
        has_word = true;
    }
    for word in words {
        let w = raw_width(word);
        if cur_width + 1 + w > width && has_word {
            lines.push(std::mem::replace(&mut cur, indent.clone()));
            cur_width = hang;
            cur.push_str(word);
            cur_width += w;
        } else {
            cur.push(' ');
            cur.push_str(word);
            cur_width += 1 + w;
        }
    }
    lines.push(cur);
    lines
}

/// The printable markup lines of the changelog, long lines wrapped to
/// `width` columns. A code span split over lines is closed at the end of
/// each piece and reopened at the start of the next.
pub fn changelog_wrapped(content: &str, width: usize) -> Vec<String> {
    let mut out = Vec::new();
    for line in changelog_lines(content) {
        if line.starts_with("# ") {
            out.push(changelog_markup(line));
            continue;
        }
        let line = line.strip_suffix('\\').unwrap_or(line);
        let mut open = false;
        for piece in wrap_raw(line, width) {
            let mut text = String::new();
            let body = piece.trim_start_matches(' ');
            text.push_str(&piece[..piece.len() - body.len()]);
            if open {
                text.push('`');
            }
            text.push_str(body);
            if piece.chars().filter(|&c| c == '`').count() % 2 == 1 {
                open = !open;
            }
            if open {
                text.push('`');
            }
            out.push(markup_raw(&text));
        }
    }
    out
}

fn fetch_changelog() -> Result<String, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(10)))
        .build()
        .into();
    let mut response = agent
        .get(CHANGELOG_URL)
        .header("User-Agent", format!("deew/{}", env!("CARGO_PKG_VERSION")))
        .call()
        .map_err(|e| e.to_string())?;
    let body = response
        .body_mut()
        .read_to_string()
        .map_err(|e| e.to_string())?;
    let json: serde_json::Value = serde_json::from_str(&body).map_err(|e| e.to_string())?;
    let encoded = json
        .get("content")
        .and_then(|c| c.as_str())
        .ok_or("no content field")?;
    let compact: String = encoded.split_whitespace().collect();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(compact)
        .map_err(|e| e.to_string())?;
    String::from_utf8(bytes).map_err(|e| e.to_string())
}

/// Wrap width for a terminal of `term` columns: the terminal width, at most
/// 120 and at least 20 so the hanging indent always leaves room for text.
fn changelog_width(term: usize) -> usize {
    term.clamp(20, 120)
}

pub fn print_changelog() -> ! {
    let content = match fetch_changelog() {
        Ok(content) => content,
        Err(_) => print_exit(ErrorKind::Changelog, None),
    };
    let width = changelog_width(crate::term::console_width());
    for line in changelog_wrapped(&content, width) {
        rprintln!("{}", line);
    }
    std::process::exit(0);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::markup::strip;
    use crate::tools::Version;

    const NO_APPS: AppRows = [(Version::NotFound, None), (Version::NotFound, None)];

    #[test]
    fn version_rows_of_the_gui_and_web() {
        let versions = Versions::default();
        let paths: [Option<String>; 6] = Default::default();
        let plain = |apps: &AppRows| -> Vec<String> {
            version_lines(&versions, &paths, apps, None)
                .iter()
                .map(|l| strip(l))
                .collect()
        };
        // neither installed
        let lines = plain(&NO_APPS);
        assert_eq!(lines.len(), 9);
        assert_eq!(lines[1], "deew-gui not found");
        assert_eq!(lines[2], "deew-web not found");
        // both installed; a file without a readable version says so
        let apps: AppRows = [
            (Version::Found("1.2.3".into()), Some("/x/deew-gui".into())),
            (Version::CouldntParse, Some("/x/deew-web".into())),
        ];
        let lines = plain(&apps);
        assert_eq!(lines[1], "deew-gui 1.2.3           /x/deew-gui");
        assert_eq!(lines[2], "deew-web couldn't parse  /x/deew-web");
    }

    fn default_lines() -> Vec<String> {
        let (d, m) = config::default_values_of(None);
        bitrate_lines(&d, &m, 78)
    }

    #[test]
    fn bitrate_list_marks_the_shim_rates() {
        let lines = default_lines();
        let find = |text: &str| lines.iter().find(|l| strip(l).contains(text)).unwrap();
        // 7.1 standard needs dee_shim for every rate, bluray for none
        let std71 = find("standard (default): 384");
        assert_eq!(std71.matches("bright_blue]").count(), 2 * 10);
        assert!(std71.contains("[bold bright_blue]384[/bold bright_blue]"));
        assert!(!find("bluray:             768").contains("bright_blue"));
        // all three Atmos modes
        for t in [
            "streaming (default): 384",
            "standard:            768",
            "bluray:              1152",
        ] {
            assert!(find(t).contains("[bold bright_blue]"), "{t}");
        }
        // only values are blue, not the labels
        assert!(
            lines
                .iter()
                .all(|l| !l.starts_with("[bold bright_blue]") || strip(l).starts_with("blue"))
        );
        assert_eq!(strip(lines.last().unwrap()), "blue: uses dee_shim");
        assert!(lines.iter().all(|l| strip(l).chars().count() <= 78));
    }

    #[test]
    fn wrapped_non_shim_leaf_is_styled_on_every_line() {
        let lines = default_lines();
        let ddp = lines.iter().position(|l| strip(l).contains("DDP")).unwrap();
        let start = ddp
            + 1
            + lines[ddp + 1..]
                .iter()
                .position(|l| strip(l).contains("1.0:"))
                .unwrap();
        let mut leaf = vec![&lines[start]];
        leaf.extend(lines[start + 1..].iter().take_while(|l| {
            !strip(l).contains(": ")
                && !strip(l)
                    .trim_start_matches(['│', ' '])
                    .starts_with(['├', '└'])
        }));
        assert!(leaf.len() > 1, "expected a wrapped leaf");
        for l in &leaf {
            assert!(l.contains("[bold color(231)]"), "{l}");
            assert!(!l.contains("not bold color(231)"), "{l}");
        }
    }

    #[test]
    fn narrow_width_wraps_and_keeps_the_indent() {
        let (d, m) = config::default_values_of(None);
        for width in [50, 20, 1, 0] {
            let plain: Vec<String> = bitrate_lines(&d, &m, width)
                .iter()
                .map(|l| strip(l))
                .collect();
            let items = plain
                .iter()
                .filter(|l| l.contains(", ") || l.ends_with(','))
                .count();
            assert!(items > 0);
            for l in &plain {
                // a line is either within the width or a single item
                let body = l.trim_start_matches(['│', ' ', '├', '└', '─']);
                let single = !body.trim_end_matches(',').contains(", ");
                assert!(l.chars().count() <= width || single, "{width}: {l}");
            }
        }
        let plain: Vec<String> = bitrate_lines(&d, &m, 50).iter().map(|l| strip(l)).collect();
        assert!(plain.iter().any(|l| l.starts_with("│       ")));
        assert!(plain.iter().any(|l| l.starts_with("│  │   ")));
    }

    #[test]
    fn bitrate_list_matches_the_design() {
        let plain: Vec<String> = default_lines().iter().map(|l| strip(l)).collect();
        let text = plain.join("\n");
        assert!(text.contains("├─ 5.1: 192, 200, 208, 216, 224, 232, 240, 248, 256, 272, 288, 304, 320, 336,\n│       352,"));
        assert!(text.contains(
            "├─ 7.1 (--ddp-mode):\n│  ├─ standard (default): 384, 448, 504, 576, 640, 704, 768, 832, 896, 960,\n│  │                      1008,"
        ));
        assert!(text.contains("└─ Atmos (--ddp-atmos-mode):\n   ├─ streaming (default): 384, 448, 576, 640, 768, 1024 (default)\n"));
    }

    #[test]
    fn wrap_items_wraps_on_visible_width() {
        let items: Vec<(String, usize)> = ["100", "200", "300", "400"]
            .iter()
            .map(|s| (format!("[bold]{s}[/bold]"), 3))
            .collect();
        // "ab: 100, 200," is 13 wide, a third value would not fit in 16
        let lines = wrap_items("ab: ", 4, &items, 16);
        let plain: Vec<String> = lines.iter().map(|l| strip(l)).collect();
        assert_eq!(plain, ["ab: 100, 200,", "    300, 400"]);
        assert_eq!(wrap_items("x ", 2, &[], 10), ["x "]);
    }

    #[test]
    fn version_output() {
        let versions = Versions {
            dee: Version::Found("6.7.0".into()),
            truehdd: Version::NotFound,
            harletty: Version::Found("0.9.0".into()),
            shim: Version::Found("0.1.0".into()),
            ffmpeg: Version::CouldntParse,
            ffprobe: Version::Found("7.1".into()),
        };
        let paths = [
            Some("/opt/deew/dee".to_string()),
            Some("/x/truehdd".to_string()),
            Some("/x/harletty".to_string()),
            Some("/opt/deew/dee_shim".to_string()),
            Some("/usr/bin/ffmpeg".to_string()),
            None,
        ];
        let apps: AppRows = [
            (
                Version::Found("1.1.1".into()),
                Some("/opt/deew/deew-gui".into()),
            ),
            (Version::NotFound, None),
        ];
        let lines: Vec<String> = version_lines(&versions, &paths, &apps, Some("/opt/my deew/deew"))
            .iter()
            .map(|l| strip(l))
            .collect();
        // deew-gui reads the first line
        assert_eq!(
            lines[0],
            format!(
                "deew     {}{}/opt/my deew/deew",
                env!("CARGO_PKG_VERSION"),
                " ".repeat(16 - env!("CARGO_PKG_VERSION").len())
            )
        );
        assert_eq!(
            lines[1..],
            [
                "deew-gui 1.1.1           /opt/deew/deew-gui",
                "deew-web not found",
                "DEE      6.7.0           /opt/deew/dee",
                "truehdd  not found",
                "harletty 0.9.0           /x/harletty",
                "dee_shim 0.1.0           /opt/deew/dee_shim",
                "ffmpeg   couldn't parse  /usr/bin/ffmpeg",
                "ffprobe  7.1",
            ]
        );
    }

    #[test]
    fn version_rows_fill_in_as_probes_finish() {
        let paths = [
            Some("/a/dee".to_string()),
            Some("/a/truehdd".to_string()),
            Some("/a/harletty".to_string()),
            Some("/a/shim".to_string()),
            Some("/a/ffmpeg".to_string()),
            Some("/a/ffprobe".to_string()),
        ];
        let exe = Some("/a/deew");
        let plain = |known: &[Option<Version>; 6]| -> Vec<String> {
            partial_version_lines(known, &paths, &NO_APPS, exe)
                .iter()
                .map(|l| strip(l))
                .collect()
        };
        // nothing known: placeholders, no paths, the column follows deew
        let mut known: [Option<Version>; 6] = Default::default();
        let own = env!("CARGO_PKG_VERSION");
        let lines = plain(&known);
        assert_eq!(lines.len(), 9);
        assert_eq!(lines[3], "DEE      …");
        assert_eq!(lines[8], "ffprobe  …");
        assert_eq!(lines[0], format!("deew     {own}  /a/deew"));
        // one long version widens the column, the others stay pending
        known[4] = Some(Version::Found("7.1.1-very-long".into()));
        let lines = plain(&known);
        assert_eq!(lines[7], "ffmpeg   7.1.1-very-long  /a/ffmpeg");
        assert_eq!(lines[4], "truehdd  …");
        assert!(lines[0].ends_with("/a/deew") && lines[0].len() > own.len() + 17);
        // all known: exactly the final output
        let versions = Versions {
            dee: Version::Found("6.7.0".into()),
            truehdd: Version::NotFound,
            harletty: Version::NotFound,
            shim: Version::Found("0.1.0".into()),
            ffmpeg: Version::CouldntParse,
            ffprobe: Version::Found("7.1".into()),
        };
        let all = [
            Some(versions.dee.clone()),
            Some(versions.truehdd.clone()),
            Some(versions.harletty.clone()),
            Some(versions.shim.clone()),
            Some(versions.ffmpeg.clone()),
            Some(versions.ffprobe.clone()),
        ];
        assert_eq!(
            partial_version_lines(&all, &paths, &NO_APPS, exe),
            version_lines(&versions, &paths, &NO_APPS, exe)
        );
    }

    #[test]
    fn version_without_the_deew_path() {
        let versions = Versions {
            dee: Version::NotFound,
            truehdd: Version::NotFound,
            harletty: Version::NotFound,
            shim: Version::NotFound,
            ffmpeg: Version::NotFound,
            ffprobe: Version::NotFound,
        };
        let paths = [None, None, None, None, None, None];
        let lines = version_lines(&versions, &paths, &NO_APPS, None);
        assert_eq!(
            strip(&lines[0]),
            format!("deew     {}", env!("CARGO_PKG_VERSION"))
        );
        // the brackets of a path are escaped
        let lines = version_lines(&versions, &paths, &NO_APPS, Some("/a[b]/deew"));
        assert_eq!(
            strip(&lines[0]),
            format!("deew     {}  /a[b]/deew", env!("CARGO_PKG_VERSION"))
        );
    }

    #[test]
    fn heading() {
        assert_eq!(
            changelog_markup("# deew 3.2.2:"),
            "[not bold white][bold color(231)]deew 3.2.2:[/bold color(231)][/not bold white]"
        );
    }

    #[test]
    fn backticks_alternate() {
        assert_eq!(
            changelog_markup("use `-b` and `-d` now"),
            "[not bold white]use [bold yellow]-b[/bold yellow] and [bold yellow]-d[/bold yellow] now[/not bold white]"
        );
        // an unbalanced backtick stays open, like in Python
        assert_eq!(
            changelog_markup("a ` b"),
            "[not bold white]a [bold yellow] b[/not bold white]"
        );
    }

    #[test]
    fn trailing_backslash_dropped() {
        assert_eq!(
            changelog_markup("line\\"),
            "[not bold white]line[/not bold white]"
        );
        assert_eq!(
            changelog_markup("a\\b"),
            "[not bold white]a\\b[/not bold white]"
        );
    }

    #[test]
    fn brackets_are_literal() {
        let m = changelog_markup("- fix `[x]` and [bold]no[/bold]");
        assert_eq!(strip(&m), "- fix [x] and [bold]no[/bold]");
        assert_eq!(strip(&changelog_markup("[a:0] stuff")), "[a:0] stuff");
    }

    #[test]
    fn heading_replaces_every_hash_space() {
        assert_eq!(
            changelog_markup("# a # b"),
            "[not bold white][bold color(231)]a b[/bold color(231)][/not bold white]"
        );
    }

    #[test]
    fn plain_and_empty_lines() {
        assert_eq!(changelog_markup(""), "[not bold white][/not bold white]");
        assert_eq!(
            changelog_markup("- item"),
            "[not bold white]- item[/not bold white]"
        );
    }

    #[test]
    fn keeps_last_ten_entries_reversed() {
        let entries: Vec<String> = (1..=12).map(|i| format!("# e{i}\n- x{i}")).collect();
        let content = entries.join("\n\n");
        let lines = changelog_lines(&content);
        // Python: reversed(entries)[-10:] == [e10, e9, ..., e1] joined by blank lines
        assert_eq!(lines.len(), 10 * 2 + 9);
        assert_eq!(lines[0], "# e10");
        assert_eq!(lines[1], "- x10");
        assert_eq!(lines[2], "");
        assert_eq!(lines[3], "# e9");
        assert_eq!(*lines.last().unwrap(), "- x1");
    }

    #[test]
    fn fewer_than_ten_entries() {
        let lines = changelog_lines("# a\n- 1\n\n# b\n- 2");
        assert_eq!(lines, vec!["# b", "- 2", "", "# a", "- 1"]);
    }

    fn plain(lines: &[String]) -> Vec<String> {
        lines.iter().map(|l| strip(l)).collect()
    }

    #[test]
    fn wrap_hanging_indents() {
        let long = "aaa bbb ccc ddd eee fff";
        let got = plain(&changelog_wrapped(&format!("- {long}"), 10));
        assert_eq!(got, vec!["- aaa bbb", "  ccc ddd", "  eee fff"]);
        let got = plain(&changelog_wrapped(&format!("  - {long}"), 12));
        assert_eq!(got, vec!["  - aaa bbb", "    ccc ddd", "    eee fff"]);
        let got = plain(&changelog_wrapped(&format!("    - {long}"), 14));
        assert_eq!(got, vec!["    - aaa bbb", "      ccc ddd", "      eee fff"]);
        let got = plain(&changelog_wrapped("  aaa bbb ccc", 9));
        assert_eq!(got, vec!["  aaa bbb", "  ccc"]);
        let got = plain(&changelog_wrapped("1. aaa bbb ccc", 9));
        assert_eq!(got, vec!["1. aaa", "   bbb", "   ccc"]);
    }

    #[test]
    fn width_is_terminal_capped_at_120() {
        assert_eq!(changelog_width(60), 60);
        assert_eq!(changelog_width(150), 120);
        assert_eq!(changelog_width(0), 20);
    }

    #[test]
    fn wrap_long_word_unbroken() {
        let got = plain(&changelog_wrapped(
            "- see https://example.com/very/long/url now",
            12,
        ));
        assert_eq!(
            got,
            vec!["- see", "  https://example.com/very/long/url", "  now"]
        );
    }

    #[test]
    fn wrap_code_span_split_stays_balanced() {
        let got = changelog_wrapped("- aa `bb cc dd ee` ff", 10);
        assert_eq!(
            got,
            vec![
                "[not bold white]- aa [bold yellow]bb cc[/bold yellow][/not bold white]",
                "[not bold white]  [bold yellow]dd ee[/bold yellow] ff[/not bold white]",
            ]
        );
        for l in &got {
            assert_eq!(&balanced(l), l);
        }
    }

    #[test]
    fn wrap_leaves_short_lines_and_headings() {
        let content =
            "# deew 1.0.0: a very long heading that is wider than ten\n- short\n- also short";
        let got = plain(&changelog_wrapped(content, 80));
        assert_eq!(
            got,
            vec![
                "deew 1.0.0: a very long heading that is wider than ten",
                "- short",
                "- also short"
            ]
        );
        let got = plain(&changelog_wrapped("# deew 1.0.0: long heading here", 10));
        assert_eq!(got, vec!["deew 1.0.0: long heading here"]);
    }

    #[test]
    fn wrap_real_changelog_fits() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("changelog.md");
        let content = std::fs::read_to_string(path).unwrap();
        for width in [60, 80, 120] {
            for l in plain(&changelog_wrapped(&content, width)) {
                if l.starts_with("deew ") || crate::markup::visible_width(&l) <= width {
                    continue;
                }
                assert_eq!(
                    l.trim().split(' ').count(),
                    if l.trim_start().starts_with("- ") {
                        2
                    } else {
                        1
                    },
                    "too wide at {width}: {l}"
                );
            }
        }
    }
}
