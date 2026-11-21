//! Hand-written command line: parsing plus help/version text styled like the
//! deew CLI (same markup and SGR sequences as deew's `markup` module).

use std::io::IsTerminal;

const RESET: &str = "\x1b[0m";
const COLORS: [&str; 8] = [
    "black", "red", "green", "yellow", "blue", "magenta", "cyan", "white",
];

const HELP: &str = concat!(
    "[bold cyan]deew-web[/bold cyan] ",
    env!("CARGO_PKG_VERSION"),
    "

[yellow]USAGE[/yellow]: [bold cyan]deew-web[/bold cyan] [[bold color(231)]FLAGS[/bold color(231)]]

[yellow]FLAGS[/yellow]:
  [green]-h[/green], [green]--help[/green]       print help
  [green]-v[/green], [green]--version[/green]    print version
  [green]--host[/green] [bold color(231)]HOST[/bold color(231)]      address to listen on [[underline magenta]default:[/underline magenta] [bold color(231)]127.0.0.1[/bold color(231)]]
  [green]--port[/green] [bold color(231)]PORT[/bold color(231)]      port to listen on [[underline magenta]default:[/underline magenta] [bold color(231)]8765[/bold color(231)]]
  [green]--hash-password[/green]  ask for a password and print its argon2 hash for the config"
);

fn version_markup() -> String {
    format!(
        "[bold cyan]deew-web[/bold cyan] [not bold white]{}[/not bold white]",
        env!("CARGO_PKG_VERSION")
    )
}

fn parse_color(word: &str, base: u32) -> Option<String> {
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
        "underline" => 4,
        _ => return None,
    })
}

fn parse_style(text: &str) -> Option<String> {
    let mut words = text.split(' ');
    let mut codes = Vec::new();
    while let Some(word) = words.next() {
        let code = match word {
            "not" => match attribute(words.next()?)? {
                1 => "22".to_string(),
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

/// Render `[style]text[/style]` markup the way deew's `markup::render` does.
/// `\[` and a `[` that does not start a valid tag are literal text.
pub fn render(s: &str, color: bool) -> String {
    let mut out = String::with_capacity(s.len());
    let mut stack: Vec<(&str, String)> = Vec::new();
    let mut emitted = false;
    let restyle = |out: &mut String, stack: &[(&str, String)]| {
        if color {
            out.push_str(RESET);
            for (_, codes) in stack {
                out.push_str(&format!("\x1b[{codes}m"));
            }
        }
    };
    let mut rest = s;
    while !rest.is_empty() {
        if let Some(r) = rest.strip_prefix("\\[") {
            out.push('[');
            rest = r;
            continue;
        }
        if rest.starts_with('[')
            && let Some(end) = rest.find(']')
        {
            let inner = &rest[1..end];
            if let Some(closing) = inner.strip_prefix('/') {
                if closing.is_empty() || parse_style(closing).is_some() {
                    let idx = stack
                        .iter()
                        .rposition(|(t, _)| *t == closing)
                        .or(stack.len().checked_sub(1));
                    if let Some(i) = idx {
                        stack.remove(i);
                    }
                    restyle(&mut out, &stack);
                    emitted = true;
                    rest = &rest[end + 1..];
                    continue;
                }
            } else if let Some(codes) = parse_style(inner) {
                stack.push((inner, codes));
                restyle(&mut out, &stack);
                emitted = true;
                rest = &rest[end + 1..];
                continue;
            }
        }
        let len = rest.chars().next().map_or(1, char::len_utf8);
        out.push_str(&rest[..len]);
        rest = &rest[len..];
    }
    if color && emitted {
        out.push_str(RESET);
    }
    out
}

fn no_color_env() -> bool {
    std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty())
}

pub fn color_enabled() -> bool {
    std::io::stdout().is_terminal() && !no_color_env()
}

fn stderr_color_enabled() -> bool {
    std::io::stderr().is_terminal() && !no_color_env()
}

/// Print markup to stdout (colored only if stdout is a terminal).
pub fn out(markup: &str) {
    println!("{}", render(markup, color_enabled()));
}

/// Print markup to stderr (colored only if stderr is a terminal).
pub fn err(markup: &str) {
    eprintln!("{}", render(markup, stderr_color_enabled()));
}

/// Escape dynamic text so that it is never taken for a markup tag.
pub fn esc(text: &str) -> String {
    text.replace('[', "\\[")
}

/// Print `ERROR: message` to stderr in deew's error style.
pub fn error(message: &str) {
    err(&format!(
        "[color(231) on red]ERROR:[/color(231) on red] {}",
        esc(message)
    ));
}

/// The output of `--hash-password` after the prompts.
pub fn hash_output(hash: &str, color: bool) -> String {
    let markup = format!(
        "[bold color(231)]{}[/bold color(231)]

[bold cyan]Paste this into your config file like this:[/bold cyan]
[bold color(81)]\\[deew_web][/bold color(81)]
    [color(231)]user[/color(231)] [color(197)]=[/color(197)] [color(186)]'your username here'[/color(186)]
    [color(231)]pass[/color(231)] [color(197)]=[/color(197)] [color(186)]'generated hash here'[/color(186)]",
        esc(hash)
    );
    render(&markup, color)
}

fn help_text(color: bool) -> String {
    render(&format!("[not bold white]{HELP}[/not bold white]"), color)
}

fn version_text(color: bool) -> String {
    render(&version_markup(), color)
}

pub fn print_help() {
    println!("{}", help_text(color_enabled()));
}

pub fn print_version() {
    println!("{}", version_text(color_enabled()));
}

/// Print an error line plus a hint and exit with 2 (deew's argument-error code).
pub fn usage_error(message: &str) -> ! {
    let color = color_enabled();
    println!(
        "{}",
        render(
            &format!(
                "[color(231) on red]ERROR:[/color(231) on red] {}",
                esc(message)
            ),
            color
        )
    );
    println!(
        "{}",
        render("run [bold cyan]deew-web -h[/bold cyan] for help.", color)
    );
    std::process::exit(2);
}

#[derive(Debug, PartialEq, Eq)]
pub struct Options {
    pub host: String,
    pub port: u16,
    pub hash_password: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    Help,
    Version,
    Run(Options),
}

fn parse_port(value: &str) -> Result<u16, String> {
    value
        .parse()
        .map_err(|_| format!("invalid port '{value}' (expected 0-65535)"))
}

/// Parse the arguments (without the program name). Help and version win as
/// soon as they are seen, like in deew.
pub fn parse<I: IntoIterator<Item = String>>(args: I) -> Result<Command, String> {
    let mut options = Options {
        host: "127.0.0.1".into(),
        port: 8765,
        hash_password: false,
    };
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        let (name, inline) = match arg.split_once('=') {
            Some((n, v)) if n.starts_with("--") => (n.to_string(), Some(v.to_string())),
            _ => (arg.clone(), None),
        };
        match name.as_str() {
            "-h" | "--help" if inline.is_none() => return Ok(Command::Help),
            "-v" | "--version" if inline.is_none() => return Ok(Command::Version),
            "--hash-password" if inline.is_none() => options.hash_password = true,
            "--host" | "--port" => {
                let value = inline
                    .or_else(|| args.next())
                    .ok_or_else(|| format!("{name} needs a value"))?;
                if name == "--host" {
                    if value.is_empty() {
                        return Err("--host needs a value".into());
                    }
                    options.host = value;
                } else {
                    options.port = parse_port(&value)?;
                }
            }
            _ => return Err(format!("unknown argument '{arg}'")),
        }
    }
    Ok(Command::Run(options))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(args: &[&str]) -> Result<Command, String> {
        parse(args.iter().map(|s| s.to_string()))
    }

    fn run(host: &str, port: u16, hash_password: bool) -> Command {
        Command::Run(Options {
            host: host.into(),
            port,
            hash_password,
        })
    }

    #[test]
    fn parses_all_forms() {
        assert_eq!(p(&[]), Ok(run("127.0.0.1", 8765, false)));
        assert_eq!(
            p(&["--host", "0.0.0.0", "--port", "80"]),
            Ok(run("0.0.0.0", 80, false))
        );
        assert_eq!(
            p(&["--host=::1", "--port=9000", "--hash-password"]),
            Ok(run("::1", 9000, true))
        );
        assert_eq!(p(&["-h"]), Ok(Command::Help));
        assert_eq!(p(&["--help"]), Ok(Command::Help));
        assert_eq!(p(&["-v"]), Ok(Command::Version));
        assert_eq!(p(&["--version"]), Ok(Command::Version));
    }

    #[test]
    fn rejects_bad_input() {
        assert!(p(&["--bogus"]).unwrap_err().contains("unknown argument"));
        assert!(p(&["-V"]).is_err());
        assert!(p(&["--host"]).unwrap_err().contains("needs a value"));
        assert!(p(&["--port"]).unwrap_err().contains("needs a value"));
        assert!(p(&["--port", "abc"]).unwrap_err().contains("invalid port"));
        assert!(p(&["--port=70000"]).unwrap_err().contains("invalid port"));
        assert!(p(&["--hash-password=1"]).is_err());
    }

    #[test]
    fn plain_help_is_exact() {
        let expected = "deew-web 1.0.0

USAGE: deew-web [FLAGS]

FLAGS:
  -h, --help       print help
  -v, --version    print version
  --host HOST      address to listen on [default: 127.0.0.1]
  --port PORT      port to listen on [default: 8765]
  --hash-password  ask for a password and print its argon2 hash for the config";
        assert_eq!(help_text(false), expected);
    }

    #[test]
    fn plain_version() {
        assert_eq!(version_text(false), "deew-web 1.0.0");
    }

    #[test]
    fn colored_output_uses_deew_sequences() {
        assert!(version_text(true).starts_with("\x1b[0m\x1b[1;36mdeew-web\x1b[0m"));
        assert!(help_text(true).contains("\x1b[0m\x1b[22;37m\x1b[4;35mdefault:"));
    }

    #[test]
    fn escaped_bracket_is_literal() {
        assert_eq!(render("\\[bold]x", true), "[bold]x");
        assert_eq!(render("a [b] c", false), "a [b] c");
    }

    const HASH: &str = "$argon2id$v=19$m=19456,t=2,p=1$Q9iiODqVQTwvEI1bJaIteg$/aH957ECQjR9ZgDtB96eKzQjcJWPukYOcjPnUdrzBSQ";

    fn strip_ansi(s: &str) -> String {
        let mut out = String::new();
        let mut chars = s.chars();
        while let Some(c) = chars.next() {
            if c == '\x1b' {
                for c in chars.by_ref() {
                    if c == 'm' {
                        break;
                    }
                }
            } else {
                out.push(c);
            }
        }
        out
    }

    #[test]
    fn plain_hash_output_is_exact() {
        let expected = format!(
            "{HASH}

Paste this into your config file like this:
[deew_web]
    user = 'your username here'
    pass = 'generated hash here'"
        );
        assert_eq!(hash_output(HASH, false), expected);
    }

    #[test]
    fn colored_hash_output_has_toml_colors() {
        let colored = hash_output(HASH, true);
        assert!(colored.contains("\x1b[1;38;5;81m[deew_web]"));
        assert!(colored.contains("\x1b[38;5;197m=\x1b[0m"));
        assert!(colored.contains("\x1b[38;5;186m'your username here'"));
        assert!(colored.starts_with("\x1b[0m\x1b[1;38;5;231m$argon2id"));
        assert_eq!(strip_ansi(&colored), hash_output(HASH, false));
    }
}
