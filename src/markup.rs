//! Minimal renderer for the `rich`-style markup used by the Python version
//! (`[bold cyan]text[/bold cyan]`, `[color(231) on red]`, `\[escaped]`).

use std::io::IsTerminal;
use std::sync::OnceLock;
use unicode_width::UnicodeWidthStr;

const RESET: &str = "\x1b[0m";
const COLORS: [&str; 8] = [
    "black", "red", "green", "yellow", "blue", "magenta", "cyan", "white",
];

/// Parse a single color word (`red` or `color(N)`) into its palette form.
fn parse_color(word: &str, base: u32) -> Option<String> {
    // bright blue marks what uses dee_shim
    if word == "bright_blue" {
        return Some((base + 64).to_string());
    }
    if let Some(i) = COLORS.iter().position(|c| *c == word) {
        return Some((base + i as u32).to_string());
    }
    let n: u8 = word
        .strip_prefix("color(")?
        .strip_suffix(')')?
        .parse()
        .ok()?;
    Some(format!("{};5;{n}", base + 8))
}

fn attribute(word: &str) -> Option<u32> {
    Some(match word {
        "bold" => 1,
        "dim" => 2,
        "italic" => 3,
        "underline" => 4,
        _ => return None,
    })
}

/// Turn the text of a tag (without brackets) into an SGR parameter string,
/// or `None` if it is not a valid style.
fn parse_style(text: &str) -> Option<String> {
    let mut words = text.split(' ');
    let mut codes = Vec::new();
    while let Some(word) = words.next() {
        let code = match word {
            "not" => match attribute(words.next()?)? {
                1 | 2 => "22".to_string(),
                n => (n + 20).to_string(),
            },
            "on" => parse_color(words.next()?, 40)?,
            _ => match attribute(word) {
                Some(n) => n.to_string(),
                None => parse_color(word, 30)?,
            },
        };
        codes.push(code);
    }
    Some(codes.join(";"))
}

/// One lexical element of a markup string.
pub(crate) enum Token<'a> {
    /// Literal text (escapes already resolved to their literal form).
    Text(&'a str),
    /// A valid opening tag: its text and SGR parameters.
    Open(&'a str, String),
    /// A valid closing tag: the text after `/` (empty for `[/]`).
    Close(&'a str),
}

/// The single markup grammar shared by `render` and `balanced`.
pub(crate) fn tokens(s: &str) -> impl Iterator<Item = Token<'_>> {
    let mut rest = s;
    std::iter::from_fn(move || {
        if rest.is_empty() {
            return None;
        }
        if !rest.starts_with(['[', '\\']) {
            let pos = rest.find(['[', '\\']).unwrap_or(rest.len());
            let (text, tail) = rest.split_at(pos);
            rest = tail;
            return Some(Token::Text(text));
        }
        if let Some(r) = rest.strip_prefix("\\[") {
            let text = &rest[1..2];
            rest = r;
            return Some(Token::Text(text));
        }
        if rest.starts_with('\\') {
            let text = &rest[..1];
            rest = &rest[1..];
            return Some(Token::Text(text));
        }
        if let Some(end) = rest.find(']') {
            let inner = &rest[1..end];
            if let Some(closing) = inner.strip_prefix('/') {
                if closing.is_empty() || parse_style(closing).is_some() {
                    rest = &rest[end + 1..];
                    return Some(Token::Close(closing));
                }
            } else if let Some(codes) = parse_style(inner) {
                rest = &rest[end + 1..];
                return Some(Token::Open(inner, codes));
            }
        }
        let text = &rest[..1];
        rest = &rest[1..];
        Some(Token::Text(text))
    })
}

/// Pop the tag matching `closing`, or the most recent one if none matches.
fn close_tag<T>(stack: &mut Vec<(&str, T)>, closing: &str) {
    let idx = stack
        .iter()
        .rposition(|(t, _)| *t == closing)
        .or(stack.len().checked_sub(1));
    if let Some(i) = idx {
        stack.remove(i);
    }
}

pub fn render(s: &str, color: bool) -> String {
    let mut out = String::with_capacity(s.len());
    // (tag text, SGR parameters) of the currently open tags.
    let mut stack: Vec<(&str, String)> = Vec::new();
    let mut emitted = false;
    // Set after a line break while a style is open: the style has to be
    // re-applied before the next visible text on the new line.
    let mut pending = false;

    let restyle = |out: &mut String, stack: &[(&str, String)]| {
        if color {
            out.push_str(RESET);
            for (_, codes) in stack {
                out.push_str(&format!("\x1b[{codes}m"));
            }
        }
    };

    for token in tokens(s) {
        match token {
            Token::Text(text) if !color || stack.is_empty() => out.push_str(text),
            Token::Text(text) => {
                // Every line must end with default attributes, otherwise
                // some terminals (ConPTY) pad it with styled spaces.
                for piece in text.split_inclusive('\n') {
                    let line = piece.strip_suffix('\n');
                    let body = line.unwrap_or(piece);
                    let body = body.strip_suffix('\r').unwrap_or(body);
                    if !body.is_empty() {
                        if pending {
                            for (_, codes) in &stack {
                                out.push_str(&format!("\x1b[{codes}m"));
                            }
                            pending = false;
                        }
                        out.push_str(body);
                    }
                    if line.is_some() {
                        if !pending {
                            out.push_str(RESET);
                        }
                        out.push_str(&piece[body.len()..]);
                        pending = true;
                    }
                }
            }
            Token::Open(tag, codes) => {
                stack.push((tag, codes));
                restyle(&mut out, &stack);
                pending = false;
                emitted = true;
            }
            Token::Close(closing) => {
                close_tag(&mut stack, closing);
                restyle(&mut out, &stack);
                pending = false;
                emitted = true;
            }
        }
    }
    if color && emitted {
        out.push_str(RESET);
    }
    out
}

/// Return `s` with a `[/]` appended for every tag still open at its end, so
/// that styles cannot leak out of a fragment embedded in a larger string.
pub fn balanced(s: &str) -> String {
    let mut stack: Vec<(&str, ())> = Vec::new();
    for token in tokens(s) {
        match token {
            Token::Text(_) => {}
            Token::Open(tag, _) => stack.push((tag, ())),
            Token::Close(closing) => close_tag(&mut stack, closing),
        }
    }
    let mut out = s.to_string();
    out.push_str(&"[/]".repeat(stack.len()));
    out
}

pub fn strip(s: &str) -> String {
    render(s, false)
}

pub fn visible_width(s: &str) -> usize {
    strip(s).width()
}

fn no_color_env() -> bool {
    std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty())
}

/// Lets the terminal interpret the escape codes. On Windows the classic
/// console (conhost, used by a plain cmd.exe) shows them as text unless
/// virtual terminal processing is switched on; `console` does that and
/// returns false where it cannot (old Windows 10 builds), so those get
/// plain text. Other platforms need nothing.
fn enable_ansi() -> bool {
    #[cfg(windows)]
    return console::Term::stdout().features().colors_supported();
    #[cfg(not(windows))]
    true
}

pub fn color_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::io::stdout().is_terminal() && !no_color_env() && enable_ansi())
}

#[macro_export]
macro_rules! rprintln {
    ($($arg:tt)*) => {
        println!("{}", $crate::markup::render(&format!($($arg)*), $crate::markup::color_enabled()))
    };
}

#[macro_export]
macro_rules! rprint {
    ($($arg:tt)*) => {
        print!("{}", $crate::markup::render(&format!($($arg)*), $crate::markup::color_enabled()))
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_nested_tags() {
        let s = "[bold cyan]deew[/bold cyan] [underline magenta]a [not bold white]b[/not bold white][/underline magenta] c";
        assert_eq!(strip(s), "deew a b c");
        assert_eq!(strip("[bold][italic]x[/][/]"), "x");
    }

    #[test]
    fn escapes() {
        assert_eq!(strip("\\[input]"), "[input]");
        assert_eq!(strip("a\\b \\[a:0]"), "a\\b [a:0]");
        assert_eq!(render("\\[input]", true), "[input]");
    }

    #[test]
    fn invalid_tags_are_literal() {
        for s in [
            "[Group] file.mkv",
            "[input]",
            "[1/2]",
            "[y/n]",
            "[a:0]",
            "[bold",
            "[]",
            "[/x]",
            "[color(256)]",
            "[not red]",
        ] {
            assert_eq!(strip(s), s);
            assert_eq!(render(s, true), s);
        }
        assert_eq!(strip("[[bold]x[/bold]"), "[x");
    }

    #[test]
    fn sgr_output() {
        assert_eq!(
            render("[color(231) on red]ERROR:[/color(231) on red] x", true),
            "\x1b[0m\x1b[38;5;231;41mERROR:\x1b[0m x\x1b[0m"
        );
        assert_eq!(
            render("[bold green]a[/bold green]", true),
            "\x1b[0m\x1b[1;32ma\x1b[0m\x1b[0m"
        );
        assert_eq!(
            render("[bold]a[red]b[/red]c[/bold]", true),
            "\x1b[0m\x1b[1ma\x1b[0m\x1b[1m\x1b[31mb\x1b[0m\x1b[1mc\x1b[0m\x1b[0m"
        );
        assert_eq!(render("plain", true), "plain");
        assert_eq!(
            render("[on color(17) blue]x", true),
            "\x1b[0m\x1b[48;5;17;34mx\x1b[0m"
        );
    }

    #[test]
    fn bright_blue_marks_what_needs_dee_shim() {
        assert_eq!(
            render("[bright_blue]x[/bright_blue]", true),
            "\x1b[0m\x1b[94mx\x1b[0m\x1b[0m"
        );
        assert_eq!(
            render("[bold bright_blue]x", true),
            "\x1b[0m\x1b[1;94mx\x1b[0m"
        );
        assert_eq!(render("[bright_blue]x[/bright_blue]", false), "x");
        assert_eq!(strip("[bright_blue]x[/bright_blue]"), "x");
    }

    #[test]
    fn not_attributes() {
        assert_eq!(render("[not bold]x", true), "\x1b[0m\x1b[22mx\x1b[0m");
        assert_eq!(
            render("[not italic bold magenta]x", true),
            "\x1b[0m\x1b[23;1;35mx\x1b[0m"
        );
        assert_eq!(render("[not underline]x", true), "\x1b[0m\x1b[24mx\x1b[0m");
    }

    #[test]
    fn unbalanced_tags_do_not_panic() {
        assert_eq!(strip("[bold magenta]a[bold magenta]b"), "ab");
        assert_eq!(strip("a[/bold]b[/]c"), "abc");
        assert_eq!(strip("[bold]a[/red]b"), "ab");
        // A closing tag that matches nothing pops the most recent tag.
        assert_eq!(
            render("[bold]a[/red]b", true),
            "\x1b[0m\x1b[1ma\x1b[0mb\x1b[0m"
        );
        // A matching closing tag pops that tag, not the latest.
        assert_eq!(
            render("[bold][red]a[/bold]b", true),
            "\x1b[0m\x1b[1m\x1b[0m\x1b[1m\x1b[31ma\x1b[0m\x1b[31mb\x1b[0m"
        );
    }

    #[test]
    fn balanced_closes_open_tags() {
        assert_eq!(balanced("[bold cyan]Version"), "[bold cyan]Version[/]");
        assert_eq!(balanced("[bold][red]x"), "[bold][red]x[/][/]");
        assert_eq!(balanced("[bold][red]x[/red]"), "[bold][red]x[/red][/]");
        assert_eq!(balanced("[bold]x[/bold]"), "[bold]x[/bold]");
        assert_eq!(balanced("plain"), "plain");
        assert_eq!(balanced(""), "");
        assert_eq!(balanced("\\[bold]x"), "\\[bold]x");
        assert_eq!(
            balanced("[Group] file [1/2] [a:0]"),
            "[Group] file [1/2] [a:0]"
        );
        assert_eq!(balanced("a[/red]b"), "a[/red]b");
        // The result never leaks styles.
        assert_eq!(
            render(&balanced("[bold cyan]x"), true),
            "\x1b[0m\x1b[1;36mx\x1b[0m\x1b[0m"
        );
    }

    /// Every line must end in default attributes or contain no escapes.
    fn assert_lines_reset(ansi: &str) {
        for line in ansi.split('\n') {
            let line = line.strip_suffix('\r').unwrap_or(line);
            assert!(
                !line.contains('\x1b') || line.ends_with(RESET),
                "line ends styled: {line:?}"
            );
        }
    }

    #[test]
    fn lines_end_with_reset() {
        let r = render("[not bold white]a\nb[/not bold white]", true);
        assert_lines_reset(&r);
        assert!(r.contains("\x1b[22;37mb"), "{r:?}");
        assert_eq!(render("a\nb", true), "a\nb");
        assert_eq!(render("a\nb", false), "a\nb");
        assert_eq!(
            render("[bold]a\r\nb[/bold]", true),
            "\x1b[0m\x1b[1ma\x1b[0m\r\n\x1b[1mb\x1b[0m\x1b[0m"
        );
    }

    #[test]
    fn nested_styles_survive_newline() {
        let r = render("[bold cyan]x [red]y\nz[/red] w[/bold cyan]", true);
        assert_lines_reset(&r);
        let style_y = "\x1b[0m\x1b[1;36m\x1b[31my";
        assert!(r.contains(style_y), "{r:?}");
        assert!(r.contains("\n\x1b[1;36m\x1b[31mz"), "{r:?}");
    }

    #[test]
    fn blank_lines_in_styled_block() {
        let r = render("[bold]a\n\n\nb[/bold]", true);
        assert_lines_reset(&r);
        assert_eq!(r, "\x1b[0m\x1b[1ma\x1b[0m\n\n\n\x1b[1mb\x1b[0m\x1b[0m");
        assert_lines_reset(&render("[red]a\n[/red]\n[blue]\n", true));
    }

    #[test]
    fn help_lines_end_with_reset() {
        let help = crate::help::wrap_help(crate::help::HELP, 80);
        assert_lines_reset(&render(
            &format!("[not bold white]{help}[/not bold white]"),
            true,
        ));
    }

    #[test]
    fn macros_compile() {
        crate::rprintln!("[bold]{}[/bold] \\[x]", 1);
        crate::rprint!("a{}\n", 2);
    }

    #[test]
    fn width() {
        assert_eq!(visible_width("[bold]abc[/bold]"), 3);
        assert_eq!(visible_width("[Group] x"), 9);
        assert_eq!(visible_width("[yellow]日本[/yellow]"), 4);
        assert_eq!(visible_width("▄▄ \\[x]"), 6);
    }
}
