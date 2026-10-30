//! Error reporting: port of the Python `print_exit`.

use crate::data::ErrorKind;
use crate::rprintln;

/// Escape `[` so arbitrary text is never parsed as markup.
pub fn escape(s: &str) -> String {
    s.replace('[', "\\[")
}

/// Print `ERROR: <markup>` to stdout and exit with status 1.
pub fn error_line(markup: &str) -> ! {
    crate::events::error(&crate::markup::strip(markup));
    rprintln!("[color(231) on red]ERROR:[/color(231) on red] {}", markup);
    std::process::exit(1);
}

/// Build the message for `kind`, substituting `insert` for the placeholder.
pub fn format_error(kind: ErrorKind, insert: Option<&str>) -> String {
    let message = kind.message();
    match insert {
        Some(insert) if !insert.is_empty() && message.contains('🤠') => {
            let mut parts = message.split('🤠');
            let before = parts.next().unwrap_or("");
            let after = parts.next().unwrap_or("");
            format!("{before}{insert}{after}")
        }
        _ => message.to_string(),
    }
}

/// Exactly the Python `print_exit`: print the error message and exit with 1.
pub fn print_exit(kind: ErrorKind, insert: Option<&str>) -> ! {
    error_line(&format_error(kind, insert))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn substitutes_placeholder() {
        assert_eq!(
            format_error(ErrorKind::Path, Some("a.mkv")),
            "path [bold yellow]a.mkv[/bold yellow] does not exist."
        );
        assert_eq!(
            format_error(ErrorKind::SampleMismatch, Some("[48000, 44100]")),
            "each input has to have the same sample rate:\n[48000, 44100]"
        );
    }

    #[test]
    fn no_insert_keeps_message() {
        assert_eq!(
            format_error(ErrorKind::Delay, None),
            "unsupported delay value."
        );
        assert_eq!(
            format_error(ErrorKind::Delay, Some("x")),
            "unsupported delay value."
        );
        // An empty insert is falsy in Python.
        assert_eq!(
            format_error(ErrorKind::Path, Some("")),
            ErrorKind::Path.message()
        );
    }

    #[test]
    fn escapes_brackets() {
        assert_eq!(escape("a[b]"), "a\\[b]");
        assert_eq!(crate::markup::strip(&escape("[bold]x")), "[bold]x");
    }
}
