//! Static help text, captured from the markup the Python version passes to rich.

/// Full help text (without the outer `[not bold white]` wrapper).
pub const HELP: &str = concat!(
    r#"[bold cyan][bold cyan]deew[/bold cyan][/bold cyan] "#,
    env!("CARGO_PKG_VERSION"),
    r#"

[yellow]USAGE[/yellow]: [bold cyan]deew[/bold cyan] [[green]-h[/green]] [[green]-v[/green]] [[green]-i[/green] [[bold color(231)]INPUT[/bold color(231)] ...]] [[green]-ti[/green] [bold color(231)]INDEX[/bold color(231)]] [[green]-o[/green] [bold color(231)]DIRECTORY[/bold color(231)]] [[green]-oi[/green]] [[green]-f[/green] [bold color(231)]FORMAT[/bold color(231)]] [[green]--ddp-mode[/green] [bold color(231)]MODE[/bold color(231)]] [[green]--ddp-atmos-mode[/green] [bold color(231)]MODE[/bold color(231)]] [[green]-b[/green] [bold color(231)]BITRATE[/bold color(231)]] [[green]-dm[/green] [bold color(231)]CHANNELS[/bold color(231)]] [[green]-dl[/green]] [[green]-nps[/green]] [[green]-d[/green] [bold color(231)]DELAY[/bold color(231)]] [[green]-r[/green] [bold color(231)]DRC[/bold color(231)]] [[green]-dn[/green] [bold color(231)]DIALNORM[/bold color(231)]] [[green]-in[/green] [bold color(231)]NUMBER[/bold color(231)]] [[green]-k[/green]] [[green]-kl[/green]] [[green]-mo[/green]] [[green]-lb[/green]] [[green]-la[/green]] [[green]-np[/green]] [[green]-pl[/green]] [[green]-cl[/green]] [[green]-c[/green]] [[green]-gc[/green]] [[green]-gt[/green]]

[yellow]FLAGS[/yellow]:
  [green]-h[/green], [green]--help[/green]                show this help message.
  [green]-v[/green], [green]--version[/green]             show version and the versions of the tools.
  [green]-i[/green], [green]--input[/green] [[bold color(231)]INPUT[/bold color(231)] ...]   audio file(s) or folder(s)
  [green]-ti[/green], [green]--track[/green][green]-index[/green] [bold color(231)]INDEX[/bold color(231)]  [underline magenta]default:[/underline magenta] [bold color(231)]0[/bold color(231)]
                            select audio track index of input(s)
  [green]-o[/green], [green]--output[/green] [bold color(231)]DIRECTORY[/bold color(231)]    [underline magenta]default:[/underline magenta] output path of the config, else the current directory
                            specifies output directory
  [green]-oi[/green], [green]--output[/green][green]-to-input[/green]    save each output next to its input file
                            overrides [bold color(231)]-o[/bold color(231)] and the output path of the config
  [green]-f[/green], [green]--format[/green] [bold color(231)]FORMAT[/bold color(231)]       [underline magenta]options:[/underline magenta] [bold color(231)]dd[/bold color(231)] / [bold color(231)]ddp[/bold color(231)] / [bold bright_blue]ddp-atmos[/bold bright_blue] / [bold color(231)]thd[/bold color(231)] / [bold bright_blue]thd-atmos[/bold bright_blue] / [bold color(231)]ac4[/bold color(231)] / [bold color(231)]ac4-atmos[/bold color(231)] / [bold color(231)]aac[/bold color(231)]
                            [underline magenta]default:[/underline magenta] [bold color(231)]ddp[/bold color(231)]
                            [bold bright_blue]ddp-atmos[/bold bright_blue] and [bold bright_blue]thd-atmos[/bold bright_blue] use truehdd and dee_shim, ac4-atmos uses truehdd
  [green]--ddp-mode[/green] [bold color(231)]MODE[/bold color(231)]           [underline magenta]options:[/underline magenta] [bold bright_blue]standard[/bold bright_blue] / [bold color(231)]bluray[/bold color(231)]
                            [underline magenta]default:[/underline magenta] run [green]-c[/green]/[green]--config[/green]
                            profile of 7.1 DDP, 5.1 and below are not affected
                            [bold bright_blue]standard[/bold bright_blue]: E-AC-3 only, 384-1792 kbps
                            [bold color(231)]bluray[/bold color(231)]: AC-3 core + E-AC-3, 768-1664 kbps
  [green]--ddp-atmos-mode[/green] [bold color(231)]MODE[/bold color(231)]     [underline magenta]options:[/underline magenta] [bold bright_blue]streaming[/bold bright_blue] / [bold bright_blue]standard[/bold bright_blue] / [bold bright_blue]bluray[/bold bright_blue]
                            [underline magenta]default:[/underline magenta] run [green]-c[/green]/[green]--config[/green]
                            profile of [bold bright_blue]ddp-atmos[/bold bright_blue]
                            [bold bright_blue]streaming[/bold bright_blue]: 5.1 with Atmos, 384-1024 kbps
                            [bold bright_blue]standard[/bold bright_blue]: 7.1 with Atmos, E-AC-3 only, 768-2048 kbps
                            [bold bright_blue]bluray[/bold bright_blue]: 7.1 with Atmos, AC-3 core + E-AC-3, 1152-1664 kbps
  [green]-b[/green], [green]--bitrate[/green] [bold color(231)]BITRATE[/bold color(231)]     [underline magenta]options:[/underline magenta] run [green][green]-lb[/green][/green]/[green][green]--list[/green][green]-bitrates[/green][/green]
                            [underline magenta]default:[/underline magenta] run [green][green]-c[/green][/green]/[green][green]--config[/green][/green]
  [green]-dm[/green], [green]--downmix[/green] [bold color(231)]CHANNELS[/bold color(231)]   [underline magenta]options:[/underline magenta] [bold color(231)]1[/bold color(231)] / [bold color(231)]2[/bold color(231)] / [bold color(231)]6[/bold color(231)]
                            specifies downmix, only works for DD/DDP/AC4/AAC and TrueHD (2 / 6)
                            DD and AC4 will be automatically downmixed to 5.1 in case of a 7.1 source
                            AAC output is always 2.0
  [green]-dl[/green], [green]--disable-lfe[/green]        remove the LFE channel of 5.1 and 7.1 inputs
                            DD and DDP get a real 5.0 or [bold bright_blue]7.0[/bold bright_blue], the others keep a silent LFE
                            5.0 and 7.0 inputs are supported, they have no LFE
  [green]-nps[/green], [green]--no-phase-shift[/green]    turn off DEE's 90° phase shift of surround channels
                            only works for DD/DDP/DDP Atmos, config: [bold color(231)]surround_phase_shift[/bold color(231)]
  [green]-d[/green], [green]--delay[/green] [bold color(231)]DELAY[/bold color(231)]         [underline magenta]examples:[/underline magenta] [bold color(231)]-5.1ms[/bold color(231)], [bold color(231)]+1,52s[/bold color(231)], [bold color(231)]-24@pal[/bold color(231)], [bold color(231)]+10@24000/1001[/bold color(231)]
                            [underline magenta]default:[/underline magenta] [bold color(231)]0ms[/bold color(231)] or parsed from filename
                            specifies delay as ms, s or frame@FPS
                            FPS can be a number, division or ntsc / pal
                            you have to specify negative values as [bold color(231)]-[/bold color(231)][bold color(231)]d=-0ms[/bold color(231)]
  [green]-r[/green], [green]--drc[/green] [bold color(231)]DRC[/bold color(231)]             [underline magenta]options:[/underline magenta] [bold bright_blue]none[/bold bright_blue] / [bold color(231)]film_light[/bold color(231)] / [bold color(231)]film_standard[/bold color(231)] / [bold color(231)]music_light[/bold color(231)] / [bold color(231)]music_standard[/bold color(231)] / [bold color(231)]speech[/bold color(231)]
                            [underline magenta]default:[/underline magenta] [bold color(231)]none[/bold color(231)] ([bold color(231)]music_light[/bold color(231)] for AC4)
                            specifies drc profile
                            [bold bright_blue]none[/bold bright_blue] uses dee_shim for TrueHD, not available
                            for AC4 (AC4 Atmos has it)
  [green]-dn[/green], [green]--dialnorm[/green] [bold color(231)]DIALNORM[/bold color(231)]  [underline magenta]options:[/underline magenta] between [bold color(231)]-31[/bold color(231)] and [bold color(231)]0[/bold color(231)] (in case of [bold color(231)]0[/bold color(231)] DEE's measurement will be used)
                            [underline magenta]default:[/underline magenta] [bold color(231)]0[/bold color(231)]
                            applied dialnorm value, doesn't work for AAC and
                            AC4 Atmos
  [green]-in[/green], [green]--instances[/green] [bold color(231)]NUMBER[/bold color(231)]   [underline magenta]examples:[/underline magenta] [bold color(231)]1[/bold color(231)], [bold color(231)]4[/bold color(231)], [bold color(231)]50%[/bold color(231)]
                            [underline magenta]default:[/underline magenta] [bold color(231)]50%[/bold color(231)]
                            specifies how many encodes can run at the same time
                            [bold color(231)]50%[/bold color(231)] means [bold color(231)]4[/bold color(231)] on a cpu with 8 threads
                            one DEE can use 2 threads so [bold color(231)]50%[/bold color(231)] can utilize all threads
                            (this option overrides the config's number)
  [green]-k[/green], [green]--keeptemp[/green]            keep temp files
  [green]-kl[/green], [green]--keep[/green][green]-log[/green]           keep TrueHD's log and write a .deew-log.html report for each output
  [green]-mo[/green], [green]--measure[/green][green]-only[/green]       only measures the loudness of the source with DEE, no encoding, -f and the encoding options are ignored, TrueHD Atmos sources are measured as Atmos
  [green]-lb[/green], [green]--list[/green][green]-bitrates[/green]      list bitrates that DEE can do for each format
  [green]-la[/green], [green]--long[/green][green]-argument[/green]      print ffmpeg, truehdd and DEE arguments for each input
  [green]-np[/green], [green]--no[/green][green]-prompt[/green]          skip confirmation questions, print them as notes and continue
  [green]-pl[/green], [green]--print[/green][green]-logos[/green]        show all logo variants you can set in the config
  [green]-cl[/green], [green]--changelog[/green]          show changelog
  [green]-c[/green], [green]--config[/green]              show config and config location
  [green]-gc[/green], [green]--generate[/green][green]-config[/green]    generate a new config
  [green]-gt[/green], [green]--get[/green][green]-tools[/green]          download ffmpeg, ffprobe, truehdd and dee_shim next to deew

[bold bright_blue]blue[/bold bright_blue]: uses dee_shim"#
);

/// Column where option descriptions start in the FLAGS section.
const DESC_COL: usize = 28;
/// Continuation indent of the USAGE block.
const USAGE_INDENT: usize = 12;
/// Below this width option rows are split and descriptions indented less.
const NARROW: usize = 80;
/// Maximum width of the USAGE block.
const USAGE_MAX: usize = 80;
/// Indent used in narrow mode.
const NARROW_INDENT: usize = 4;

/// One character of a markup line: its source text and visible width.
struct Atom {
    raw: String,
    width: usize,
}

fn atoms(line: &str) -> Vec<Atom> {
    use crate::markup::{Token, tokens};
    use unicode_width::UnicodeWidthChar;
    let mut out = Vec::new();
    for token in tokens(line) {
        match token {
            Token::Text(text) => out.extend(text.chars().map(|c| Atom {
                raw: c.to_string(),
                width: c.width().unwrap_or(0),
            })),
            Token::Open(tag, _) => out.push(Atom {
                raw: format!("[{tag}]"),
                width: 0,
            }),
            Token::Close(tag) => out.push(Atom {
                raw: format!("[/{tag}]"),
                width: 0,
            }),
        }
    }
    out
}

/// Greedy wrap of `prefix` (already visible `prefix_width` wide) followed by
/// the words of `body`, breaking at spaces and indenting with `indent`.
fn wrap_line(
    prefix: &str,
    prefix_width: usize,
    body: &[Atom],
    indent: usize,
    width: usize,
    groups: bool,
) -> String {
    // words: source text and visible width; tags stay with their word
    let mut words: Vec<(String, usize)> = Vec::new();
    let mut cur = (String::new(), 0usize);
    let mut started = false;
    let mut depth = 0usize;
    for a in body {
        if groups {
            match a.raw.as_str() {
                "[" => depth += 1,
                "]" => depth = depth.saturating_sub(1),
                _ => {}
            }
        }
        if a.raw == " " && depth > 0 {
            cur.0.push(' ');
            cur.1 += 1;
            continue;
        }
        if a.raw == " " {
            if started {
                words.push(std::mem::take(&mut cur));
                cur = (String::new(), 0);
                started = false;
            }
            continue;
        }
        cur.0.push_str(&a.raw);
        cur.1 += a.width;
        started |= a.width > 0;
    }
    if started || !cur.0.is_empty() {
        words.push(cur);
    }
    let pad = " ".repeat(indent);
    let mut out = String::from(prefix);
    let mut line_width = prefix_width;
    let mut has_word = false;
    for (text, w) in words {
        if has_word && line_width + 1 + w > width {
            out.push('\n');
            out.push_str(&pad);
            line_width = indent;
            has_word = false;
        }
        if has_word {
            out.push(' ');
            line_width += 1;
        }
        out.push_str(&text);
        line_width += w;
        has_word = true;
    }
    out
}

#[derive(Clone, Copy, PartialEq)]
enum Section {
    Other,
    Usage,
    Flags,
}

/// Wrap the help markup to `width` columns. Existing line breaks are kept;
/// only lines wider than `width` get more. Below 80 columns each option row
/// is split into its flags and an indented description.
pub fn wrap_help(text: &str, width: usize) -> String {
    let narrow = width < NARROW;
    let mut section = Section::Other;
    let mut lines: Vec<String> = Vec::new();
    for line in text.split('\n') {
        let lead = line.len() - line.trim_start_matches(' ').len();
        if line.is_empty() {
            section = Section::Other;
        } else if line.starts_with("[yellow]USAGE") {
            section = Section::Usage;
        } else if line.starts_with("[yellow]FLAGS") {
            section = Section::Flags;
        }
        let full = width;
        let width = if section == Section::Usage {
            full.min(USAGE_MAX)
        } else {
            full
        };
        let at = atoms(line);
        let total: usize = at.iter().map(|a| a.width).sum();
        let (indent, option_row) = match section {
            Section::Usage => (if narrow { NARROW_INDENT } else { USAGE_INDENT }, false),
            Section::Flags if lead == 2 => (if narrow { NARROW_INDENT } else { DESC_COL }, true),
            Section::Flags if lead >= DESC_COL => {
                (if narrow { NARROW_INDENT } else { DESC_COL }, false)
            }
            _ => (lead, false),
        };
        if option_row && narrow {
            // split flags and description at the description column
            let mut w = 0;
            let mut split = at.len();
            for (i, a) in at.iter().enumerate() {
                if w >= DESC_COL {
                    split = i;
                    break;
                }
                w += a.width;
            }
            let flags: String = at[..split].iter().map(|a| a.raw.as_str()).collect();
            lines.push(flags.trim_end().to_string());
            lines.push(wrap_line(
                &" ".repeat(NARROW_INDENT),
                NARROW_INDENT,
                &at[split..],
                NARROW_INDENT,
                width,
                false,
            ));
        } else if option_row {
            if total <= width {
                lines.push(line.to_string());
                continue;
            }
            let mut w = 0;
            let mut split = at.len();
            for (i, a) in at.iter().enumerate() {
                if w >= DESC_COL {
                    split = i;
                    break;
                }
                w += a.width;
            }
            let prefix: String = at[..split].iter().map(|a| a.raw.as_str()).collect();
            lines.push(wrap_line(
                &prefix,
                DESC_COL,
                &at[split..],
                indent,
                width,
                false,
            ));
        } else if narrow && lead > 0 && indent != lead && section != Section::Other {
            // re-indent an existing continuation line
            let body = &at[lead..];
            lines.push(wrap_line(
                &" ".repeat(indent),
                indent,
                body,
                indent,
                width,
                section == Section::Usage,
            ));
        } else if total <= width {
            lines.push(line.to_string());
        } else {
            let body = &at[lead..];
            lines.push(wrap_line(
                &" ".repeat(lead),
                lead,
                body,
                indent,
                width,
                section == Section::Usage,
            ));
        }
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::markup::{render, strip, visible_width};

    /// Lines wider than `width` must be a single word on their own.
    fn assert_fits(out: &str, width: usize) {
        for line in strip(out).lines() {
            if line.chars().count() > width {
                assert_eq!(line.trim().split(' ').count(), 1, "too wide: {line:?}");
            }
        }
    }

    #[test]
    fn wide_is_identity() {
        let flags = |t: &str| t.split_once("\n\n[yellow]FLAGS").unwrap().1.to_string();
        assert_eq!(flags(&wrap_help(HELP, 200)), flags(HELP));
        let rest = flags(HELP);
        let longest = rest.lines().map(visible_width).max().unwrap();
        assert_eq!(flags(&wrap_help(HELP, longest.max(80))), rest);
    }

    fn usage_lines(out: &str) -> Vec<String> {
        let plain = strip(out);
        let usage = plain.split("\n\nFLAGS").next().unwrap();
        usage.lines().skip(2).map(str::to_string).collect()
    }

    #[test]
    fn usage_wraps_at_80_max() {
        for width in [80, 120, 200] {
            let lines = usage_lines(&wrap_help(HELP, width));
            assert!(lines.len() > 1);
            for (i, l) in lines.iter().enumerate() {
                assert!(l.chars().count() <= 80, "{width}: {l:?}");
                if i > 0 {
                    assert!(l.starts_with(&" ".repeat(12)), "{l:?}");
                    assert_ne!(l.as_bytes()[12], b' ', "{l:?}");
                }
            }
        }
        for width in [79, 50] {
            let lines = usage_lines(&wrap_help(HELP, width));
            for (i, l) in lines.iter().enumerate() {
                assert!(l.chars().count() <= width, "{width}: {l:?}");
                if i > 0 {
                    assert!(l.starts_with("    "), "{l:?}");
                    assert_ne!(l.as_bytes()[4], b' ', "{l:?}");
                }
            }
        }
    }

    #[test]
    fn wraps_with_description_indent() {
        for width in [200, 100, 80] {
            let out = wrap_help(HELP, width);
            assert_fits(&out, width);
            let plain = strip(&out);
            assert!(plain.lines().count() > strip(HELP).lines().count());
            let mut in_flags = false;
            for line in plain.lines() {
                if line.starts_with("FLAGS") {
                    in_flags = true;
                } else if line.is_empty() {
                    in_flags = false;
                } else if in_flags && !line.starts_with("  -") {
                    assert!(line.starts_with(&" ".repeat(28)), "{line:?}");
                    assert_ne!(line.as_bytes()[28], b' ', "{line:?}");
                }
            }
            let want = format!("{}standard: E-AC-3 only", " ".repeat(28));
            assert!(plain.lines().any(|l| l.starts_with(&want)));
        }
    }

    #[test]
    fn narrow_splits_option_rows() {
        for width in [79, 60, 40] {
            let out = wrap_help(HELP, width);
            assert_fits(&out, width);
            let plain = strip(&out);
            let lines: Vec<&str> = plain.lines().collect();
            let i = lines
                .iter()
                .position(|l| *l == "  -f, --format FORMAT")
                .unwrap();
            assert!(lines[i + 1].starts_with("    options: dd"));
            assert!(!lines[i + 1].starts_with("     "));
            let flags = lines.iter().position(|l| l.starts_with("FLAGS")).unwrap();
            for l in &lines[flags + 1..] {
                if l.is_empty() {
                    break;
                }
                assert!(
                    l.starts_with("  -") || (l.starts_with("    ") && !l.starts_with("     ")),
                    "{l:?}"
                );
            }
            assert!(lines.contains(&"    standard: E-AC-3 only, 384-1792 kbps"));
            for l in lines
                .iter()
                .filter(|l| l.starts_with("    [") || l.starts_with("    -"))
            {
                assert!(!l.starts_with("     "), "{l:?}");
            }
        }
    }

    #[test]
    fn usage_groups_stay_whole() {
        for width in 40..=200 {
            let out = strip(&wrap_help(HELP, width));
            let usage = out.split("\n\nFLAGS").next().unwrap();
            for line in usage.lines().skip(2) {
                let mut depth = 0i32;
                for c in line.chars() {
                    match c {
                        '[' => depth += 1,
                        ']' => depth -= 1,
                        _ => {}
                    }
                    assert!(depth >= 0, "{width}: {line:?}");
                }
                assert_eq!(depth, 0, "{width}: {line:?}");
            }
        }
    }

    #[test]
    fn styles_survive_breaks() {
        let text = "[yellow]USAGE[/yellow]: x\n\n[yellow]FLAGS[/yellow]:\n  -z, --zed             [underline magenta]alpha beta gamma delta[/underline magenta] tail";
        let out = wrap_help(text, 40);
        assert!(out.matches('\n').count() > 3);
        let ansi = render(&format!("[not bold white]{out}[/not bold white]"), true);
        // the text after each break carries the same SGR state as before it
        let at = ansi.find("gamma").unwrap();
        let head = &ansi[..at];
        let from = (head.rfind("\x1b[0m").unwrap() + 4).max(head.rfind('\n').unwrap() + 1);
        let state = &ansi[from..at];
        assert!(state.starts_with("\x1b[22;37m\x1b[4;35m"), "{state:?}");
        assert!(ansi[..at].contains('\n'));
        assert_eq!(strip(&out).split("\n").count(), out.split('\n').count());
    }
}
