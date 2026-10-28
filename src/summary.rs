//! The "Encoding summary" table: content and a rich-compatible renderer.

use crate::config::SummarySections;
use crate::error::escape;
use crate::markup::{balanced, strip, visible_width};
use crate::plan::{Format, channel_name, filename_delay};
use crate::tools::COULDNT_PARSE;
use crate::util::{convert_delay_to_ms, format_g, version_lt};
use std::time::Duration;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

const RELEASES_URL: &str = "https://api.github.com/repos/pcrlnd/deew/releases/latest";

/// Widest the table gets, whatever the console width.
pub const MAX_WIDTH: usize = 60;

/// One table row: up to two cells (markup) and whether a separator follows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub cells: Vec<String>,
    pub end_section: bool,
    /// Break the value at any character instead of at spaces, over at most
    /// [`FOLD_LINES`] lines (for paths).
    pub fold: bool,
}

fn row(cells: &[&str], end_section: bool) -> Row {
    Row {
        cells: cells.iter().map(|c| c.to_string()).collect(),
        end_section,
        fold: false,
    }
}

/// Everything shown in the table.
#[derive(Debug, Clone)]
pub struct SummaryData<'a> {
    pub sections: &'a SummarySections,
    /// Markup for the "Latest" row (see [`latest_markup`]).
    pub latest: String,
    pub dee_version: Option<&'a str>,
    pub ffmpeg_version: Option<&'a str>,
    pub ffprobe_version: Option<&'a str>,
    pub channels: i64,
    /// Normalised bit depth of every input.
    pub bit_depths: &'a [u32],
    pub measure_only: bool,
    pub format: Format,
    pub outchannels: i64,
    pub bitrate: Option<u32>,
    pub dialnorm: i64,
    /// Resolved absolute output directory.
    pub output_dir: &'a str,
    pub files: usize,
    pub instances: f64,
    /// Text of the "Delay" row, see [`delay_summary`].
    pub delay: String,
    pub temp_path: &'a str,
}

fn version_cell(version: Option<&str>) -> String {
    version.map_or_else(|| COULDNT_PARSE.to_string(), escape)
}

fn bit_depth_cell(depths: &[u32]) -> String {
    match depths {
        [first, rest @ ..] if rest.iter().all(|d| d == first) => first.to_string(),
        _ => "mixed bit depth".to_string(),
    }
}

/// A delay in milliseconds as shown in the summary: at most 3 decimals
/// without trailing zeros, no `-0`, e.g. `-5.1ms`. `Err` for an unsupported
/// delay.
fn delay_ms_text(delay: &str) -> Result<String, ()> {
    let (print, _, _) = convert_delay_to_ms(delay, false)?;
    // `print` is "{:.3} ms"
    let number = print.trim_end_matches(" ms");
    let number = if number.contains('.') {
        number.trim_end_matches('0').trim_end_matches('.')
    } else {
        number
    };
    Ok(if number == "-0" { "0" } else { number }.to_string())
}

/// The "Delay" row: the `-d` value, else what the file names say.
pub fn delay_summary(arg: Option<&str>, files: &[String]) -> Result<String, ()> {
    if let Some(delay) = arg.filter(|d| !d.is_empty()) {
        return delay_ms_text(delay).map(|n| format!("{n}ms"));
    }
    let tags: Vec<Option<String>> = files.iter().map(|f| filename_delay(f)).collect();
    if tags.iter().all(Option::is_none) {
        return Ok("0ms".to_string());
    }
    // files without a tag have no delay
    let delays = tags
        .iter()
        .map(|t| delay_ms_text(t.as_deref().unwrap_or("+0ms")))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(if delays.iter().all(|d| *d == delays[0]) {
        format!("{}ms (parsed from filenames)", delays[0])
    } else {
        "mixed (parsed from filenames)".to_string()
    })
}

pub fn summary_rows(d: &SummaryData) -> Vec<Row> {
    let mut rows = Vec::new();
    if d.sections.deew_info {
        rows.push(row(
            &["[bold cyan]Version", env!("CARGO_PKG_VERSION")],
            false,
        ));
        rows.push(row(&["[bold cyan]Latest", &d.latest], true));
    }
    if d.sections.binaries {
        rows.push(row(
            &["[cyan]DEE version", &version_cell(d.dee_version)],
            false,
        ));
        rows.push(row(
            &["[cyan]ffmpeg version", &version_cell(d.ffmpeg_version)],
            false,
        ));
        rows.push(row(
            &["[cyan]ffprobe version", &version_cell(d.ffprobe_version)],
            true,
        ));
    }
    if d.sections.input_info {
        rows.push(row(&["[bold yellow]Input(s)"], false));
        rows.push(row(&["Channels", channel_name(d.channels)], false));
        rows.push(row(&["Bit depth", &bit_depth_cell(d.bit_depths)], true));
    }
    if !d.measure_only && d.sections.output_info {
        rows.push(row(&["[bold yellow]Output"], false));
        rows.push(row(&["Format", d.format.summary_name()], false));
        let channels = if d.format == Format::Ac4 {
            "immersive stereo"
        } else {
            channel_name(d.outchannels)
        };
        rows.push(row(&["Channels", channels], false));
        let bitrate = match (d.format, d.bitrate) {
            (Format::Thd, _) | (_, None) => "N/A".to_string(),
            (_, Some(b)) => format!("{b} kbps"),
        };
        rows.push(row(&["Bitrate", &bitrate], false));
        let dialnorm = if d.dialnorm == 0 {
            "auto (0)".to_string()
        } else {
            format!("{} dB", d.dialnorm)
        };
        rows.push(row(&["Dialnorm", &dialnorm], false));
        rows.push(row(&["Directory", &escape(d.output_dir)], true));
    }
    if d.sections.other {
        rows.push(row(&["[bold yellow]Other"], false));
        rows.push(row(&["Files", &d.files.to_string()], false));
        rows.push(row(&["Max instances", &format_g(d.instances)], false));
        rows.push(row(&["Delay", &d.delay], false));
        rows.push(Row {
            fold: true,
            ..row(&["Temp path", &escape(d.temp_path)], false)
        });
    }
    rows
}

pub fn any_section(s: &SummarySections) -> bool {
    s.deew_info || s.binaries || s.input_info || s.output_info || s.other
}

/// The "Latest" cell: highlighted when a newer release exists.
pub fn latest_markup(current: &str, latest: Result<String, String>) -> String {
    match latest {
        Ok(tag) if version_lt(current, &tag) => {
            format!("[bold green]{}[/bold green] !!!", escape(&tag))
        }
        Ok(tag) => escape(&tag),
        Err(_) => "[red]couldn't fetch".to_string(),
    }
}

/// `tag_name` of the latest GitHub release.
pub fn fetch_latest() -> Result<String, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(10)))
        .build()
        .into();
    let mut response = agent
        .get(RELEASES_URL)
        .header("User-Agent", format!("deew/{}", env!("CARGO_PKG_VERSION")))
        .call()
        .map_err(|e| e.to_string())?;
    let body = response
        .body_mut()
        .read_to_string()
        .map_err(|e| e.to_string())?;
    let json: serde_json::Value = serde_json::from_str(&body).map_err(|e| e.to_string())?;
    json.get("tag_name")
        .and_then(|t| t.as_str())
        .map(str::to_string)
        .ok_or_else(|| "no tag_name".to_string())
}

// ---------------------------------------------------------------------------
// Table renderer (port of the parts of rich.table.Table used by deew)

const PADDING: usize = 2; // one space on both sides of a cell
const COLUMN_STYLES: [&str; 2] = ["green", "color(231)"];

/// `rich.helpers.ratio_reduce`.
fn ratio_reduce(
    total: usize,
    ratios: &[usize],
    maximums: &[usize],
    values: &[usize],
) -> Vec<usize> {
    let ratios: Vec<usize> = ratios
        .iter()
        .zip(maximums)
        .map(|(&r, &m)| if m > 0 { r } else { 0 })
        .collect();
    let mut total_ratio: usize = ratios.iter().sum();
    if total_ratio == 0 {
        return values.to_vec();
    }
    let mut remaining = total;
    let mut result = Vec::new();
    for ((&ratio, &maximum), &value) in ratios.iter().zip(maximums).zip(values) {
        if ratio > 0 && total_ratio > 0 {
            // Python's round() is half-to-even.
            let share = (ratio as f64 * remaining as f64 / total_ratio as f64).round_ties_even();
            let distributed = maximum.min(share as usize);
            result.push(value.saturating_sub(distributed));
            remaining = remaining.saturating_sub(distributed);
            total_ratio -= ratio;
        } else {
            result.push(value);
        }
    }
    result
}

/// `Table._collapse_widths` with every column wrapable.
fn collapse_widths(mut widths: Vec<usize>, max_width: usize) -> Vec<usize> {
    let mut total: usize = widths.iter().sum();
    while total > 0 && total > max_width {
        let excess = total - max_width;
        let max_column = widths.iter().copied().max().unwrap_or(0);
        let second_max = widths
            .iter()
            .copied()
            .filter(|&w| w != max_column)
            .max()
            .unwrap_or(0);
        let difference = max_column - second_max;
        let ratios: Vec<usize> = widths
            .iter()
            .map(|&w| usize::from(w == max_column))
            .collect();
        if difference == 0 {
            break;
        }
        let max_reduce = vec![excess.min(difference); widths.len()];
        widths = ratio_reduce(excess, &ratios, &max_reduce, &widths);
        total = widths.iter().sum();
    }
    widths
}

fn column_widths(rows: &[Row], max_width: usize) -> Vec<usize> {
    let mut widths = vec![0usize; 2];
    for row in rows {
        for (i, cell) in row.cells.iter().enumerate() {
            widths[i] = widths[i].max(visible_width(cell) + PADDING);
        }
    }
    for w in &mut widths {
        *w = (*w).max(1);
    }
    let extra = widths.len() + 1; // borders
    let max_width = max_width.saturating_sub(extra);
    if widths.iter().sum::<usize>() > max_width {
        widths = collapse_widths(widths, max_width);
        let total: usize = widths.iter().sum();
        if total > max_width {
            let ones = vec![1; widths.len()];
            widths = ratio_reduce(total - max_width, &ones, &widths.clone(), &widths);
        }
    }
    widths
}

/// Split `text` into words the way rich does (`\s*\S+\s*`).
fn words(text: &[char]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < text.len() {
        let start = i;
        while i < text.len() && text[i].is_whitespace() {
            i += 1;
        }
        while i < text.len() && !text[i].is_whitespace() {
            i += 1;
        }
        while i < text.len() && text[i].is_whitespace() {
            i += 1;
        }
        out.push((start, i));
    }
    out
}

fn cells_width(s: &[char]) -> usize {
    s.iter().map(|c| c.width().unwrap_or(0)).sum()
}

/// Crop to `width` columns, ending in an ellipsis if something was cut.
fn truncate_ellipsis(line: &str, width: usize) -> String {
    if line.width() <= width {
        return line.to_string();
    }
    let mut out = String::new();
    let mut used = 0;
    for c in line.chars() {
        let w = c.width().unwrap_or(0);
        if used + w > width.saturating_sub(1) {
            break;
        }
        out.push(c);
        used += w;
    }
    // A wide character that does not fit is replaced by a space.
    out.push_str(&" ".repeat(width.saturating_sub(1) - used));
    out.push('…');
    out
}

/// Wrap plain text into lines of at most `width` columns (rich's
/// `Text.wrap` with `overflow="ellipsis"`).
fn wrap(text: &str, width: usize) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut breaks = Vec::new();
    let mut offset = 0;
    for (start, end) in words(&chars) {
        let word = &chars[start..end];
        let trimmed_len = {
            let mut e = word.len();
            while e > 0 && word[e - 1].is_whitespace() {
                e -= 1;
            }
            cells_width(&word[..e])
        };
        if width.saturating_sub(offset) >= trimmed_len {
            offset += cells_width(word);
        } else if trimmed_len > width {
            if start > 0 {
                breaks.push(start);
            }
            offset = cells_width(word);
        } else if offset > 0 && start > 0 {
            breaks.push(start);
            offset = cells_width(word);
        }
    }
    breaks.push(chars.len());
    let mut lines = Vec::new();
    let mut from = 0;
    for b in breaks {
        if b < from {
            continue;
        }
        let line: String = chars[from..b].iter().collect();
        lines.push(truncate_ellipsis(line.trim_end(), width));
        from = b;
    }
    lines
}

/// Lines a folded cell may take.
const FOLD_LINES: usize = 2;

/// The first `width` columns of `line`.
fn crop(line: &str, width: usize) -> String {
    let mut out = String::new();
    let mut used = 0;
    for c in line.chars() {
        used += c.width().unwrap_or(0);
        if used > width {
            break;
        }
        out.push(c);
    }
    out
}

/// Break plain text into lines of at most `width` columns at any character,
/// `max_lines` at most; the last one ends in `...` if something was cut.
fn fold_lines(text: &str, width: usize, max_lines: usize) -> Vec<String> {
    let mut lines = vec![String::new()];
    let mut used = 0;
    for c in text.chars() {
        let w = c.width().unwrap_or(0);
        if used + w > width {
            if lines.len() == max_lines {
                let last = lines.pop().unwrap_or_default();
                let dots = &"..."[..width.min(3)];
                lines.push(format!("{}{dots}", crop(&last, width - dots.len())));
                return lines;
            }
            lines.push(String::new());
            used = 0;
        }
        lines.last_mut().unwrap().push(c);
        used += w;
    }
    lines
}

/// Lines of the cell content for column width `width` (markup).
fn cell_lines(cell: &str, width: usize, fold: bool) -> Vec<String> {
    let inner = width.saturating_sub(PADDING);
    if visible_width(cell) <= inner {
        return vec![cell.to_string()];
    }
    let text = strip(cell);
    let lines = if fold {
        fold_lines(&text, inner, FOLD_LINES)
    } else {
        wrap(&text, inner)
    };
    lines.iter().map(|l| escape(l)).collect()
}

/// Render the table as markup, `console_width` columns at most.
pub fn render_table(title: &str, rows: &[Row], console_width: usize) -> String {
    let widths = column_widths(rows, console_width);
    let table_width = widths.iter().sum::<usize>() + widths.len() + 1;

    let rule = |left: &str, mid: &str, right: &str| {
        let bars: Vec<String> = widths.iter().map(|&w| "─".repeat(w)).collect();
        format!("{left}{}{right}", bars.join(mid))
    };
    let mut out = Vec::new();

    let left = table_width.saturating_sub(title.width()) / 2;
    out.push(format!(
        "{}[not italic bold magenta]{}[/not italic bold magenta]{}",
        " ".repeat(left),
        balanced(title),
        " ".repeat(table_width.saturating_sub(title.width() + left))
    ));
    out.push(rule("┌", "┬", "┐"));
    for (i, row) in rows.iter().enumerate() {
        let cells: Vec<Vec<String>> = (0..widths.len())
            .map(|c| match row.cells.get(c) {
                // only the value column folds
                Some(cell) => cell_lines(cell, widths[c], row.fold && c == 1),
                None => vec![String::new()],
            })
            .collect();
        let height = cells.iter().map(Vec::len).max().unwrap_or(1);
        for line in 0..height {
            let mut text = String::from("│");
            for (c, lines) in cells.iter().enumerate() {
                let content = lines.get(line).map_or("", String::as_str);
                let fill = widths[c].saturating_sub(PADDING + visible_width(content));
                let style = COLUMN_STYLES[c];
                // Cells may carry unclosed tags; rich scopes styles per cell.
                text.push_str(&format!(
                    "[{style}] {}{} [/{style}]│",
                    balanced(content),
                    " ".repeat(fill)
                ));
            }
            out.push(text);
        }
        if row.end_section && i + 1 < rows.len() {
            out.push(rule("├", "┼", "┤"));
        }
    }
    out.push(rule("└", "┴", "┘"));
    out.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::markup;

    fn sections(all: bool) -> SummarySections {
        SummarySections {
            deew_info: all,
            binaries: all,
            input_info: all,
            output_info: all,
            other: all,
        }
    }

    fn data(sections: &SummarySections) -> SummaryData<'_> {
        SummaryData {
            sections,
            latest: "[red]couldn't fetch".into(),
            dee_version: Some("5.2.1"),
            ffmpeg_version: Some("7.1.5"),
            ffprobe_version: None,
            channels: 6,
            bit_depths: &[24, 24],
            measure_only: false,
            format: Format::Ddp,
            outchannels: 6,
            bitrate: Some(1024),
            dialnorm: 0,
            output_dir: "/out/di[r]",
            files: 2,
            instances: 2.5,
            delay: "0ms".into(),
            temp_path: "/tmp/de[ew",
        }
    }

    /// Tables rendered by `rich` (from the Python implementation) for
    /// several settings and console widths.
    #[test]
    fn tables_match_rich() {
        let cases: serde_json::Value =
            serde_json::from_str(include_str!("fixtures/summary_cases.json")).unwrap();
        for case in cases.as_array().unwrap() {
            let p = &case["params"];
            let s = |key: &str| p[key].as_str().unwrap();
            let sec = |key: &str| p["sections"][key].as_bool().unwrap();
            let sections = SummarySections {
                deew_info: sec("deew_info"),
                binaries: sec("binaries"),
                input_info: sec("input_info"),
                output_info: sec("output_info"),
                other: sec("other"),
            };
            let bit_depths: Vec<u32> = p["bit_depths"]
                .as_array()
                .unwrap()
                .iter()
                .map(|d| d.as_u64().unwrap() as u32)
                .collect();
            let version = |key: &str| (s(key) != COULDNT_PARSE).then(|| s(key));
            let data = SummaryData {
                sections: &sections,
                latest: s("latest").to_string(),
                dee_version: version("dee"),
                ffmpeg_version: version("ffmpeg"),
                ffprobe_version: version("ffprobe"),
                channels: p["channels"].as_i64().unwrap(),
                bit_depths: &bit_depths,
                measure_only: p["measure_only"].as_bool().unwrap(),
                format: Format::parse(s("format")).unwrap(),
                outchannels: p["outchannels"].as_i64().unwrap(),
                bitrate: p["bitrate"].as_u64().map(|b| b as u32),
                dialnorm: p["dialnorm"].as_i64().unwrap(),
                output_dir: s("output"),
                files: p["files"].as_u64().unwrap() as usize,
                instances: p["instances"].as_f64().unwrap(),
                delay: s("delay_text").to_string(),
                temp_path: s("temp"),
            };
            let width = case["width"].as_u64().unwrap() as usize;
            let mut rows = summary_rows(&data);
            // rich crops instead of folding the temp path (see `folds_temp_path`)
            for r in &mut rows {
                r.fold = false;
            }
            let text = strip(&render_table("Encoding summary", &rows, width));
            let expected = case["expected"].as_str().unwrap().trim_end_matches('\n');
            assert_eq!(text, expected, "width {width}: {p}");
        }
    }

    #[test]
    fn delay_values() {
        for (arg, want) in [
            ("-5.1ms", "-5.1ms"),
            ("+1,52s", "1520ms"),
            ("-24@pal", "-960ms"),
            ("+10@24000/1001", "417.083ms"),
            ("+0ms", "0ms"),
            ("-0ms", "0ms"),
            ("-0.0004ms", "0ms"),
            ("+100ms", "100ms"),
            ("+5s", "5000ms"),
        ] {
            assert_eq!(delay_summary(Some(arg), &[]).as_deref(), Ok(want), "{arg}");
        }
        assert_eq!(delay_summary(Some("10ms"), &[]), Err(()));
        assert_eq!(delay_summary(Some("+10"), &["a.mkv".into()]), Err(()));
    }

    #[test]
    fn delay_from_filenames() {
        let f = |names: &[&str]| {
            let files: Vec<String> = names.iter().map(|n| n.to_string()).collect();
            delay_summary(None, &files).unwrap()
        };
        assert_eq!(f(&["a.mkv", "b.mkv"]), "0ms");
        assert_eq!(f(&["a.mkv"]), "0ms");
        assert_eq!(f(&["x/a DELAY -24ms.mka"]), "-24ms (parsed from filenames)");
        assert_eq!(
            f(&[
                "a DELAY -24ms.mka",
                "b DELAY -24ms.mka",
                "c DELAY -24ms.mka"
            ]),
            "-24ms (parsed from filenames)"
        );
        assert_eq!(f(&["a DELAY 1s.mka"]), "1000ms (parsed from filenames)");
        // same value written differently
        assert_eq!(
            f(&["a DELAY 1000ms.mka", "b DELAY +1s.mka"]),
            "1000ms (parsed from filenames)"
        );
        assert_eq!(
            f(&["a DELAY -24ms.mka", "b DELAY +24ms.mka"]),
            "mixed (parsed from filenames)"
        );
        // untagged files count as 0ms
        assert_eq!(
            f(&["a DELAY -24ms.mka", "b.mka"]),
            "mixed (parsed from filenames)"
        );
        assert_eq!(
            f(&["a DELAY +0ms.mka", "b.mka"]),
            "0ms (parsed from filenames)"
        );
        // the -d value wins over the names
        assert_eq!(
            delay_summary(Some("-5.1ms"), &["a DELAY -24ms.mka".into()]).unwrap(),
            "-5.1ms"
        );
        assert_eq!(delay_summary(Some(""), &["a.mka".into()]).unwrap(), "0ms");
    }

    #[test]
    fn bit_depth_cell_values() {
        assert_eq!(bit_depth_cell(&[24]), "24");
        assert_eq!(bit_depth_cell(&[16, 16, 16]), "16");
        assert_eq!(bit_depth_cell(&[16, 24]), "mixed bit depth");
        assert_eq!(bit_depth_cell(&[24, 24, 32]), "mixed bit depth");
    }

    /// Characters of `ansi` that are rendered while an SGR style is active.
    fn styled_chars(ansi: &str, targets: &str) -> Vec<(usize, char)> {
        let mut bad = Vec::new();
        let mut last_sgr: Option<String> = None;
        let mut chars = ansi.chars().enumerate().peekable();
        while let Some((i, ch)) = chars.next() {
            if ch == '\x1b' {
                let mut seq = String::from(ch);
                for (_, c) in chars.by_ref() {
                    seq.push(c);
                    if c.is_ascii_alphabetic() {
                        break;
                    }
                }
                if seq.ends_with('m') {
                    // Reset followed by re-applied styles shows up as the
                    // last sequence not being a plain reset.
                    last_sgr = Some(seq);
                }
            } else if targets.contains(ch) && last_sgr.as_deref().is_some_and(|s| s != "\x1b[0m") {
                bad.push((i, ch));
            }
        }
        bad
    }

    #[test]
    fn borders_are_never_styled() {
        let s = sections(true);
        let mut d = data(&s);
        d.ffprobe_version = Some("7.1.5");
        let mut rows = summary_rows(&d);
        rows.push(row(&["[bold cyan]Version", "[cyan]DEE version"], true));
        rows.push(row(&["[bold yellow]Input", "[red]couldn't fetch"], false));
        let table = render_table("[bold]Encoding summary", &rows, 80);
        let ansi = markup::render(&table, true);
        assert!(ansi.contains("\x1b[1;36m"), "colors expected");
        let borders = "│├┼┤└┴┘┌┬┐─";
        let bad = styled_chars(&ansi, borders);
        assert!(bad.is_empty(), "styled border chars: {bad:?}\n{ansi}");
        // The helper does detect leaks.
        let leaky = markup::render("[bold cyan]a│b", true);
        assert!(!styled_chars(&leaky, borders).is_empty());
        println!("{}", ansi.replace('\x1b', "^["));
    }

    #[test]
    fn sections_and_output_variants() {
        let mut s = sections(false);
        s.output_info = true;
        let mut d = data(&s);
        d.format = Format::Thd;
        d.bitrate = None;
        d.dialnorm = -27;
        let rows = summary_rows(&d);
        let texts: Vec<Vec<String>> = rows.iter().map(|r| r.cells.clone()).collect();
        assert_eq!(texts[1], ["Format", "TrueHD"]);
        assert_eq!(texts[3], ["Bitrate", "N/A"]);
        assert_eq!(texts[4], ["Dialnorm", "-27 dB"]);
        assert_eq!(texts[5], ["Directory", "/out/di\\[r]"]);
        assert!(!rows[4].end_section && rows[5].end_section);
        d.format = Format::Ac4;
        d.outchannels = 2;
        d.bitrate = Some(320);
        let rows = summary_rows(&d);
        assert_eq!(rows[2].cells, ["Channels", "immersive stereo"]);
        assert_eq!(rows[3].cells, ["Bitrate", "320 kbps"]);
        d.measure_only = true;
        assert!(summary_rows(&d).is_empty());
        assert!(!any_section(&sections(false)));
        assert!(any_section(&s));
    }

    #[test]
    fn latest_cell() {
        assert_eq!(latest_markup("3.2.2", Ok("3.2.2".into())), "3.2.2");
        assert_eq!(latest_markup("3.2.2", Ok("3.1.0".into())), "3.1.0");
        assert_eq!(
            latest_markup("3.2.2", Ok("3.3.0".into())),
            "[bold green]3.3.0[/bold green] !!!"
        );
        assert_eq!(
            latest_markup("3.2.2", Err("x".into())),
            "[red]couldn't fetch"
        );
    }

    #[test]
    fn narrow_console_wraps_and_truncates() {
        let rows = vec![row(
            &[
                "Temp path",
                &escape("/mnt/x/code/deew4 a b/target/e2e/temp"),
            ],
            false,
        )];
        let text = strip(&render_table("T", &rows, 40));
        assert!(text.lines().all(|l| l.width() <= 40), "{text}");
        assert!(text.contains("/mnt/x/code/deew4 a"), "{text}");
        assert!(text.contains("b/target/e2e/temp"), "{text}");
        let rows = vec![row(
            &[
                "Temp path",
                "/mnt/x/code/deew4/target/e2e/temp/some/long/path",
            ],
            false,
        )];
        let text = strip(&render_table("T", &rows, 40));
        assert!(text.contains("…"), "{text}");
    }

    #[test]
    fn long_directory_keeps_layout() {
        let mut s = sections(false);
        s.output_info = true;
        s.other = true;
        let mut d = data(&s);
        let long = "/mnt/x/some very/long output directory/with many/nested/components/and/more";
        d.output_dir = long;
        let rows = summary_rows(&d);
        for width in [80, 46, 34, 25] {
            let text = strip(&render_table("Encoding summary", &rows, width));
            let lines: Vec<&str> = text.lines().skip(1).collect();
            assert!(lines.iter().any(|l| l.contains("Directory")), "{text}");
            let w = lines[0].width();
            assert!(w <= width, "{text}");
            for l in &lines {
                assert_eq!(l.width(), w, "width {width}:\n{text}");
            }
            // borders line up with the top rule
            let junction = |l: &str| -> Vec<usize> {
                l.chars()
                    .enumerate()
                    .filter(|(_, c)| "┌┬┐├┼┤└┴┘│".contains(*c))
                    .map(|(i, _)| i)
                    .collect()
            };
            let top = junction(lines[0]);
            assert_eq!(top.len(), 3);
            for l in &lines {
                let j = junction(l);
                assert!(j.len() == 3 || j.len() == 2, "{text}");
                assert!(j.iter().all(|i| top.contains(i)), "{text}");
            }
        }
    }

    #[test]
    fn fold_behaviour() {
        assert_eq!(fold_lines("abcdefgh", 4, 2), ["abcd", "efgh"]);
        assert_eq!(fold_lines("abcdefghi", 4, 2), ["abcd", "e..."]);
        assert_eq!(fold_lines("abc", 4, 2), ["abc"]);
        assert_eq!(fold_lines("日本語で", 4, 2), ["日本", "語で"]);
        assert_eq!(fold_lines("日本語ですか", 5, 2), ["日本", "語..."]);
    }

    #[test]
    fn folds_temp_path() {
        let s = sections(true);
        let mut d = data(&s);
        let temp = format!("X:\\deew_temp_{}", "a".repeat(58));
        d.temp_path = &temp;
        d.files = 4;
        d.instances = 4.0;
        let rows = summary_rows(&d);
        let text = strip(&render_table("Encoding summary", &rows, MAX_WIDTH));
        let lines: Vec<&str> = text.lines().collect();
        assert!(lines[1..].iter().all(|l| l.width() == MAX_WIDTH), "{text}");
        assert_eq!(
            lines[lines.len() - 3..],
            [
                format!("│ Temp path       │ X:\\deew_temp_{} │", "a".repeat(25)),
                format!("│                 │ {}      │", "a".repeat(33)),
                format!("└{}┴{}┘", "─".repeat(17), "─".repeat(40)),
            ]
        );

        // longer than two lines: the second one ends in "..."
        let temp = format!("X:\\deew_temp_{}", "a".repeat(100));
        d.temp_path = &temp;
        let rows = summary_rows(&d);
        let text = strip(&render_table("Encoding summary", &rows, MAX_WIDTH));
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(
            lines[lines.len() - 2],
            format!("│                 │ {}... │", "a".repeat(35))
        );
        // other rows still crop to one line
        let d = SummaryData {
            temp_path: "temp",
            output_dir: &temp,
            ..d
        };
        let text = strip(&render_table(
            "Encoding summary",
            &summary_rows(&d),
            MAX_WIDTH,
        ));
        assert_eq!(
            text.lines().filter(|l| l.contains("aaa")).count(),
            1,
            "{text}"
        );
    }

    #[test]
    fn wrap_behaviour() {
        assert_eq!(wrap("a b c", 10), ["a b c"]);
        assert_eq!(wrap("aaa bbb ccc", 7), ["aaa bbb", "ccc"]);
        assert_eq!(wrap("abcdefghij", 5), ["abcd…"]);
        assert_eq!(wrap("ab cdefghij kl", 5), ["ab", "cdef…", "kl"]);
    }
}
