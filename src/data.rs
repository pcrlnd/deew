//! Static data copied verbatim from the Python implementation.

/// Allowed bitrates per output layout, in the order of `bitrates.py`.
pub const BITRATES: &[(&str, &[u32])] = &[
    (
        "dd_10",
        &[
            96, 112, 128, 160, 192, 224, 256, 320, 384, 448, 512, 576, 640,
        ],
    ),
    (
        "dd_20",
        &[
            96, 112, 128, 160, 192, 224, 256, 320, 384, 448, 512, 576, 640,
        ],
    ),
    ("dd_51", &[224, 256, 320, 384, 448, 512, 576, 640]),
    (
        "ddp_10",
        &[
            32, 40, 48, 56, 64, 72, 80, 88, 96, 104, 112, 120, 128, 144, 160, 176, 192, 200, 208,
            216, 224, 232, 240, 248, 256, 272, 288, 304, 320, 336, 352, 368, 384, 400, 448, 512,
            576, 640, 704, 768, 832, 896, 960, 1008, 1024,
        ],
    ),
    (
        "ddp_20",
        &[
            96, 104, 112, 120, 128, 144, 160, 176, 192, 200, 208, 216, 224, 232, 240, 248, 256,
            272, 288, 304, 320, 336, 352, 368, 384, 400, 448, 512, 576, 640, 704, 768, 832, 896,
            960, 1008, 1024,
        ],
    ),
    (
        "ddp_51",
        &[
            192, 200, 208, 216, 224, 232, 240, 248, 256, 272, 288, 304, 320, 336, 352, 368, 384,
            400, 448, 512, 576, 640, 704, 768, 832, 896, 960, 1008, 1024,
        ],
    ),
    (
        "ddp_71_standard",
        &[384, 448, 576, 640, 704, 768, 832, 896, 960, 1008, 1024],
    ),
    ("ddp_71_bluray", &[768, 1024, 1280, 1536, 1664]),
    (
        "ddp_71_combined",
        &[
            384, 448, 576, 640, 704, 768, 832, 896, 960, 1008, 1024, 1280, 1536, 1664,
        ],
    ),
    ("ac4_20", &[64, 72, 112, 144, 256, 320]),
];

pub fn allowed_bitrates(key: &str) -> &'static [u32] {
    BITRATES
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, v)| *v)
        .unwrap_or_else(|| panic!("unknown bitrate key: {key}"))
}

/// Banner logos (rich markup): a new one first, then the ones of `logos.py` in order.
pub const LOGOS: &[&str] = &[
    r#"┏━━━━━━━━━━━━━━━━━━━━━━━━━┓
┃ [blue]╺┳━━┓ ┏━━━╸ ┏━━━╸ ╻   ╻[/blue] ┃
┃ [blue] ┃  ┃ ┣━━╸  ┣━━╸  ┃ ╻ ┃[/blue] ┃
┃ [blue]╺┻━━┛ ┗━━━╸ ┗━━━╸ ┗━┻━┛[/blue] ┃
┃  Dolby Encoding Engine  ┃
┃   Wrapper by pcroland   ┃
┗━━━━━━━━━━━━━━━━━━━━━━━━━┛"#,
    r#"[yellow] ▄▄▄▄▄  ▄▄▄▄▄ ▄▄▄▄▄ ▄▄  ▄▄  ▄▄
 ██  ██ ██▄▄  ██▄▄  ██  ██  ██
 ██  ██ ██    ██    ██  ██  ██
 ▀▀▀▀▀  ▀▀▀▀▀ ▀▀▀▀▀  ▀▀▀▀▀▀▀▀[/yellow]
 [bold color(231)]Dolby Encoding Engine Wrapper[/bold color(231)]
"#,
    r#" ┌───────────────────────────────┐
 │ [yellow]▄▄▄▄▄  ▄▄▄▄▄ ▄▄▄▄▄ ▄▄  ▄▄  ▄▄[/yellow] │
 │ [yellow]██  ██ ██▄▄  ██▄▄  ██  ██  ██[/yellow] │
 │ [yellow]██  ██ ██    ██    ██  ██  ██[/yellow] │
 │ [yellow]▀▀▀▀▀  ▀▀▀▀▀ ▀▀▀▀▀  ▀▀▀▀▀▀▀▀ [/yellow] │
 │ [bold color(231)]Dolby Encoding Engine Wrapper[/bold color(231)] │
 └───────────────────────────────┘
"#,
    r#"[blue]
 ██▀▀█▄ ██▀▀▀ ██▀▀▀ ██  ██  ██
 ██  ██ ██    ██    ██  ██  ██
 ██  ██ ██▀▀  ██▀▀  ██  ██  ██
 ██  ██ ██    ██    ██  ██  ██
 ▀▀▀▀▀  ▀▀▀▀▀ ▀▀▀▀▀  ▀▀▀▀▀▀▀▀[/blue]
 [bold color(231)]Dolby Encoding Engine Wrapper[/bold color(231)]
"#,
    r#"
 ▄███████████████████████████████▄
 ██  ▄▄ ▀█  ▄▄▄█  ▄▄▄█  ██  ██  ██
 ██  ██  █  ████  ████  ██  ██  ██
 ██  ██  █  ▄▄██  ▄▄██  ██  ██  ██
 ██  ██  █  ████  ████  ██  ██  ██
 ██▄▄▄▄▄██▄▄▄▄▄█▄▄▄▄▄██▄▄▄▄▄▄▄▄███
  ▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀
   [bold green]Dolby Encoding Engine Wrapper[/bold green]
"#,
    r#"[yellow] ▄▄▄▄▄▄▄   ▄▄▄▄▄▄▄  ▄▄▄▄▄▄▄  ▄▄▄  ▄▄▄  ▄▄▄
 ███  ███  ███      ███      ███  ███  ███
 ███  ███  ███      ███      ███  ███  ███
 ███  ███  ███▄▄▄   ███▄▄▄   ███  ███  ███
 █▓█  ███  █▓█      █▓█      █▓█  █▓█  █▓█
 █▒█  █▓█  █▒█      █▒█      █▒█  █▒█  █▒█
 █░█  █░█  █░█      █░█      █░█  █░█  █░█
 ▀▀▀▀▀▀▀   ▀▀▀▀▀▀▀  ▀▀▀▀▀▀▀   ▀▀▀▀▀▀▀▀▀▀▀[/yellow]
       [bold color(231)]Dolby Encoding Engine Wrapper[/bold color(231)]
"#,
    r#" [yellow]┌───────────────────────────────────────────────┐[/yellow]
 [yellow]│[/yellow]                                               [yellow]│[/yellow]
 [yellow]:[/yellow]   ▄▄▄▄▄▄▄   ▄▄▄▄▄▄▄  ▄▄▄▄▄▄▄  ▄▄▄  ▄▄▄  ▄▄▄   [yellow]:[/yellow]
 [yellow].[/yellow]   ███  ███  ███      ███      ███  ███  ███   [yellow].[/yellow]
 [yellow].[/yellow]   ███  ███  ███      ███      ███  ███  ███   [yellow].[/yellow]
 [yellow] [/yellow]   ███  ███  ███▄▄▄   ███▄▄▄   ███  ███  ███
 [yellow] [/yellow]   █▓█  ███  █▓█      █▓█      █▓█  █▓█  █▓█
 [yellow].[/yellow]   █▒█  █▓█  █▒█      █▒█      █▒█  █▒█  █▒█   [yellow].[/yellow]
 [yellow].[/yellow]   █░█  █░█  █░█      █░█      █░█  █░█  █░█   [yellow].[/yellow]
 [yellow]:[/yellow]   ▀▀▀▀▀▀▀   ▀▀▀▀▀▀▀  ▀▀▀▀▀▀▀   ▀▀▀▀▀▀▀▀▀▀▀    [yellow]:[/yellow]
 [yellow]│[/yellow]                                               [yellow]│[/yellow]
 [yellow]└───────■[/yellow] [bold color(231)]Dolby Encoding Engine Wrapper[/bold color(231)] [yellow]■───────┘[/yellow]
"#,
    r#"  ▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄
 ███▀▀▀▀▀▀▀███▀▀▀▀▀▀▀██▀▀▀▀▀▀▀██▀▀▀██▀▀▀██▀▀▀███
 ███   ██   ██   ██████   ██████   ██   ██   ███
 ███   ██   ██   ██████   ██████   ██   ██   ███
 ███   ██   ██   ▀▀▀███   ▀▀▀███   ██   ██   ███
 ███ ░ ██ ░ ██ ░ ██████ ░ ██████ ░ ██ ░ ██ ░ ███
 ███ ▒ ██ ▒ ██ ▒ ██████ ▒ ██████ ▒ ██ ▒ ██ ▒ ███
 ███ ▓ ██ ▓ ██ ▓ ██████ ▓ ██████ ▓ ██ ▓ ██ ▓ ███
 ███▄▄▄▄▄▄▄███▄▄▄▄▄▄▄██▄▄▄▄▄▄▄███▄▄▄▄▄▄▄▄▄▄▄████
  ▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀
          [green]Dolby Encoding Engine Wrapper[/green]
"#,
    r#"  ▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄
 ███▀▀▀▀▀▀▀███▀▀▀▀▀▀▀██▀▀▀▀▀▀▀██▀▀▀██▀▀▀██▀▀▀███
 ███   ██   ██   ██████   ██████   ██   ██   ███
 ███   ██   ██   ██████   ██████   ██   ██   ███
 ███   ██   ██   ▀▀▀███   ▀▀▀███   ██   ██   ███
 ███ ░ ██ ░ ██ ░ ██████ ░ ██████ ░ ██ ░ ██ ░ ███
 ███ ▒ ██ ▒ ██ ▒ ██████ ▒ ██████ ▒ ██ ▒ ██ ▒ ███
 ███ ▓ ██ ▓ ██ ▓ ██████ ▓ ██████ ▓ ██ ▓ ██ ▓ ███
 ███▄▄▄▄▄▄▄███▄▄▄▄▄▄▄██▄▄▄▄▄▄▄███▄▄▄▄▄▄▄▄▄▄▄████
 ████████▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀████████
 ████████ [green]Dolby Encoding Engine Wrapper[/green] ████████
 ████████▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄████████
  ▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀"#,
    r#"
     ▄████▄   ▄█████▀  ▄█████▀ ▄█▀ ▄█▀ ▄█▀
    ▄█▀ ▐█▀  ▄█▀      ▄█▀     ▄█▀ ▄█▀ ▄█▀
   ▄█▀ ▄█▀  ▄███▀    ▄███▀   ▄█▀ ▄█▀ ▄█▀
  ▄█▀ ▄█▀  ▄█▀      ▄█▀     ▄█▀ ▄█▀ ▄█▀
 ▄████▀   ▄█████▀  ▄█████▀  ▐███▀▐██▀

      [green]Dolby Encoding Engine Wrapper[/green]
"#,
    r#"▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄
█████▀    ▀███▀     ▄██▀     ▄█▀ ▄█▀ ▄█▀ ▄█
████▀ ▄█▌ ▄██▀ ▄██████▀ ▄█████▀ ▄█▀ ▄█▀ ▄██
███▀ ▄█▀ ▄██▀   ▄████▀   ▄███▀ ▄█▀ ▄█▀ ▄███
██▀ ▄█▀ ▄██▀ ▄██████▀ ▄█████▀ ▄█▀ ▄█▀ ▄████
█▀    ▄███▀     ▄██▀     ▄██▌   ▄▌  ▄██████
█████▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀███████
█████ [green]Dolby Encoding Engine Wrapper[/green] ███████
█████▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄███████
"#,
];

/// Error messages; `🤠` is the placeholder for the inserted value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    Changelog,
    Delay,
    WslPath,
    CreateDir,
    Format,
    Downmix,
    DownmixMismatch,
    ThdDownmix,
    ThdMonoInput,
    Drc,
    LinuxThd,
    Path,
    Ffprobe,
    SampleMismatch,
    ChannelMismatch,
    Channels,
    BinaryExist,
    ConfigKey,
    Ac4InputChannels,
}

impl ErrorKind {
    pub fn message(&self) -> &'static str {
        match self {
            Self::Changelog => "couldn't fetch changelog from GitHub.",
            Self::Delay => "unsupported delay value.",
            Self::WslPath => "WSL path conversion doesn't work with [bold yellow]🤠[/bold yellow].",
            Self::CreateDir => "failed to create [bold yellow]🤠[/bold yellow].",
            Self::Format => {
                "[bold yellow]-f[/bold yellow]/[bold yellow]--format[/bold yellow] can only be [bold yellow]dd[/bold yellow], [bold yellow]ddp[/bold yellow], [bold yellow]ac4[/bold yellow] or [bold yellow]thd[/bold yellow]."
            }
            Self::Downmix => {
                "[bold yellow]-dm[/bold yellow]/[bold yellow]--downmix[/bold yellow] can only be [bold yellow]1[/bold yellow], [bold yellow]2[/bold yellow] or [bold yellow]6[/bold yellow]."
            }
            Self::DownmixMismatch => {
                "downmix value has to be lower than the number of input channels."
            }
            Self::ThdDownmix => {
                "[bold yellow]-m[/bold yellow]/[bold yellow]--mix[/bold yellow] can only be used for [bold yellow]DD[/bold yellow]/[bold yellow]DDP[/bold yellow] encoding."
            }
            Self::ThdMonoInput => {
                "input channels for TrueHD encoding can only be [bold yellow]2[/bold yellow]/[bold yellow]6[/bold yellow]/[bold yellow]8[/bold yellow]. "
            }
            Self::Drc => {
                "allowed DRC values: [bold yellow]film_light[/bold yellow], [bold yellow]film_standard[/bold yellow], [bold yellow]music_light[/bold yellow], [bold yellow]music_standard[/bold yellow], [bold yellow]speech[/bold yellow]."
            }
            Self::LinuxThd => "Linux version of DEE does not support TrueHD encoding.",
            Self::Path => "path [bold yellow]🤠[/bold yellow] does not exist.",
            Self::Ffprobe => {
                "input file couldn't be parsed by ffprobe, try to extract/remux it with ffmpeg or mkvmerge."
            }
            Self::SampleMismatch => "each input has to have the same sample rate:\n🤠",
            Self::ChannelMismatch => "each input has to have the same channel count:\n🤠",
            Self::Channels => {
                "number of input channels can only be [bold yellow]1[/bold yellow], [bold yellow]2[/bold yellow], [bold yellow]6[/bold yellow] or [bold yellow]8[/bold yellow]."
            }
            Self::BinaryExist => "[bold yellow]🤠[/bold yellow] does not exist.",
            Self::ConfigKey => {
                "the following keys are missing from your config file: 🤠.\nUpdate your config file."
            }
            Self::Ac4InputChannels => {
                "input channels for AC4 encoding can only be [bold yellow]6[/bold yellow]."
            }
        }
    }
}

pub const XML_DD_DDP: &str = include_str!("templates/dd_ddp.xml");
pub const XML_THD: &str = include_str!("templates/thd.xml");
pub const XML_AC4: &str = include_str!("templates/ac4.xml");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bitrates() {
        assert_eq!(BITRATES.len(), 10);
        assert_eq!(allowed_bitrates("ac4_20"), &[64, 72, 112, 144, 256, 320]);
        assert_eq!(allowed_bitrates("ddp_71_bluray").last(), Some(&1664));
    }

    #[test]
    fn logos() {
        assert_eq!(LOGOS.len(), 11);
        assert!(
            LOGOS
                .iter()
                .all(|l| l.contains("Dolby Encoding Engine") && l.contains("Wrapper"))
        );
    }

    #[test]
    fn messages() {
        assert_eq!(ErrorKind::Delay.message(), "unsupported delay value.");
        assert!(
            ErrorKind::SampleMismatch
                .message()
                .contains("sample rate:\n🤠")
        );
        assert!(
            ErrorKind::ConfigKey
                .message()
                .ends_with("Update your config file.")
        );
    }

    #[test]
    fn templates() {
        for t in [XML_DD_DDP, XML_THD, XML_AC4] {
            assert!(t.starts_with("<?xml"));
            assert!(t.ends_with("</job_config>"));
        }
    }
}
