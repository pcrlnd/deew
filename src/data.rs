//! Static data: bitrate tables, logos and error messages.

/// Allowed bitrates per output layout (kbps).
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
        "ddp_71",
        &[
            384, 448, 504, 576, 640, 704, 768, 832, 896, 960, 1008, 1024, 1152, 1280, 1408, 1512,
            1536, 1664, 1792,
        ],
    ),
    ("ddp_71_bluray", &[768, 1024, 1280, 1536, 1664]),
    ("ddp_atmos_streaming", &[384, 448, 576, 640, 768, 1024]),
    (
        "ddp_atmos_standard",
        &[768, 1024, 1152, 1280, 1408, 1536, 1664, 1792, 2048],
    ),
    ("ddp_atmos_bluray", &[1152, 1280, 1408, 1512, 1536, 1664]),
    (
        "ac4_20",
        &[48, 64, 96, 128, 144, 192, 256, 288, 320, 384, 448, 512, 768],
    ),
    (
        "ac4_51",
        &[96, 128, 144, 192, 256, 288, 320, 384, 448, 512, 768],
    ),
    ("ac4_atmos", &[256, 320, 448, 768]),
    (
        "aac_20",
        &[
            56, 64, 80, 96, 112, 128, 144, 160, 192, 224, 256, 288, 320, 384, 448, 512, 576,
        ],
    ),
];

pub fn allowed_bitrates(key: &str) -> &'static [u32] {
    BITRATES
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, v)| *v)
        .unwrap_or_else(|| panic!("unknown bitrate key: {key}"))
}

/// Banner logos (rich markup), selected with the config's `logo` (1-based).
pub const LOGOS: &[&str] = &[
    r#"┌─────────────────────────┐
│ [blue]╺┳━━┓ ┏━━━╸ ┏━━━╸ ╻   ╻[/blue] │
│ [blue] ┃  ┃ ┣━━╸  ┣━━╸  ┃ ╻ ┃[/blue] │
│ [blue]╺┻━━┛ ┗━━━╸ ┗━━━╸ ┗━┻━┛[/blue] │
│  [bold color(231)]Dolby Encoding Engine[/bold color(231)]  │
│   [bold color(231)]Wrapper by pcroland[/bold color(231)]   │
└─────────────────────────┘"#,
    r#"[blue] ▄▄▄▄▄  ▄▄▄▄▄ ▄▄▄▄▄ ▄▄  ▄▄  ▄▄
 ██  ██ ██▄▄  ██▄▄  ██  ██  ██
 ██  ██ ██    ██    ██  ██  ██
 ▀▀▀▀▀  ▀▀▀▀▀ ▀▀▀▀▀  ▀▀▀▀▀▀▀▀[/blue]
 [bold color(231)]Dolby Encoding Engine Wrapper[/bold color(231)]
"#,
    r#"                 [color(231)]■           [/][color(159)]▄       [/][color(229)]▄▄▄▄▄             [/][color(231)]■[/]
              [color(213)]▀     ▄            [/][color(227)]▄[/][color(228) on color(227)]▀▀▀▀▀▀▀▀▀▀[/][color(227)]▄▄            [/][color(213)]▄[/]
 [color(159)]■         [/][color(231)]███                 [/][color(220)]▄[/][color(226) on color(220)]▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀[/][color(220)]▄[/]
           [color(195) on color(159)]▀▀▀[/]                [color(214)]▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄          [/][color(213)]▀[/]
      [color(117)]▄▄▄▄▄[/][color(123) on color(117)]▀▀▀[/]     [color(117)]▄▄▄▄▄      [/][color(208)]▀ [/][color(117)]▄▄▄▄▄ [/][color(208)]▀▀▀ [/][color(117)]▄▄▄ [/][color(208)]▀▀▀     [/][color(117)]▄▄▄[/]
    [color(69)]▄[/][color(75) on color(69)]▀▀▀▀▀[/][color(75) on color(199)]▀▀▀▀[/][color(199)]▄▄ [/][color(69)]▄[/][color(75) on color(69)]▀▀▀▀▀▀▀▀[/]    [color(69)]▄[/][color(75) on color(69)]▀▀▀▀[/][color(75) on color(199)]▀▀▀▀[/][color(199)]▄▄ [/][color(75) on color(69)]▀▀▀[/] [color(197)]▄       [/][color(75) on color(69)]▀▀▀[/]
   [color(63)]██[/][color(63) on color(199)]▀[/][color(199)]▄▄▀▀ [/][color(63)]███  [/][color(199) on color(63)]▀▀▀[/][color(199)]▄▄   [/][color(63)]▓██  ▓[/][color(63) on color(199)]▀▀[/][color(199)]▄▀▀  [/][color(63)]▓██ [/][color(199)]▀[/][color(199) on color(63)]▀▀[/][color(63) on color(199)]▀[/][color(199)]▄  [/][color(63)]▓██   ▓[/][color(63) on color(199)]▀▀[/][color(199)]▄▀▀[/]
[color(199)]▄▀▀[/][color(199) on color(171)]▀▀[/][color(135) on color(171)]▀[/]     [color(135) on color(171)]▀▀▀[/]  [color(135) on color(171)]▀▀▀▀▀[/][color(199) on color(171)]▀▀▀[/][color(199)]▀▄▄▀▀[/][color(199) on color(171)]▀[/][color(171)]▄[/][color(135) on color(171)]▀▀▀▀▀▀▀▀[/][color(135)]▀  [/][color(135) on color(171)]▀▀▀[/] [color(163)]▀[/][color(199)]▀[/][color(199) on color(171)]▀[/][color(199)]▀▄▄▀▀[/][color(199) on color(171)]▀[/][color(171)]▄[/][color(135) on color(171)]▀[/]
   [color(207)]▀[/][color(207) on color(213)]▀▀▀[/][color(213)]▄▄▄▄[/][color(207) on color(213)]▀▀▀[/]  [color(207)]▀[/][color(207) on color(213)]▀▀▀[/][color(213)]▄▄▄      [/][color(207)]▀[/][color(207) on color(213)]▀▀▀[/][color(213)]▄▄▄   [/][color(163)]▄▄ [/][color(207) on color(213)]▀▀▀[/][color(213)]▄▄[/][color(207) on color(213)]▀▀▀▀[/][color(213)]▄▄[/][color(207) on color(213)]▀▀▀[/][color(207)]▀[/]
     [color(225)]▀[/][color(225) on color(231)]▀▀▀▀▀▀▀▀[/]    [color(225)]▀[/][color(225) on color(231)]▀▀▀▀▀[/][color(225)]▀      ▀[/][color(225) on color(231)]▀▀▀▀▀[/][color(225)]▀     ▀[/][color(225) on color(231)]▀▀▀▀[/][color(225)]▀▀[/][color(225) on color(231)]▀▀▀▀[/][color(225)]▀[/]
                                                        [color(241)]pcR[/]
     [color(141)]▀▀▀▀▀▀▀▀▀    ▀▀▀▀▀▀▀      ▀▀▀▀▀▀▀     ▀▀▀▀▀▀▀▀▀▀▀▀[/]
    [color(98)]▀▀▀▀▀▀▀▀▀▀▀  ▀▀▀▀▀▀▀      ▀▀▀▀▀▀▀      ▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀[/]
[color(97)]■ [/][color(61)]▀▀▀     ▀▀▀  ▀▀▀▀▀   ▀▀▀  ▀ ▀▀▀▀▀▀▀▀▀  ▀▀▀   ▀▀▀   ▀ ▀[/]
      [color(55)]░ ░ ░ ░ ░     ░ ░ ░ ░     ░ ░ ░ ░     ░           ░  [/][color(97)]■[/]
 [color(252)]D O L B Y   E N C O D I N G   E N G I N E   W R A P P E R[/]"#,
];

/// Error messages; `🤠` is the placeholder for the inserted value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    Changelog,
    Delay,
    CreateDir,
    Format,
    Downmix,
    DownmixMismatch,
    FormatDownmix,
    ThdDownmixMono,
    DownmixValue,
    DialnormFormat,
    PhaseShiftFormat,
    Ac4Drc,
    AtmosInput,
    AtmosNoAtmos,
    Ac4DialnormChannels,
    InputLayout,
    AacInput,
    ShimAtmos,
    ShimThd,
    ShimDdp71,
    ShimDdp70,
    ThdMonoInput,
    Drc,
    LinuxDthd,
    Path,
    SampleMismatch,
    ChannelMismatch,
    Channels,
    BinaryExist,
    ConfigKey,
    Ac4InputChannels,
    DdpMode,
    DdpAtmosMode,
}

impl ErrorKind {
    pub fn message(&self) -> &'static str {
        match self {
            Self::Changelog => "couldn't fetch changelog from GitHub.",
            Self::Delay => "unsupported delay value.",
            Self::CreateDir => "failed to create [bold yellow]🤠[/bold yellow].",
            Self::Format => {
                "[bold yellow]-f[/bold yellow]/[bold yellow]--format[/bold yellow] can only be [bold yellow]dd[/bold yellow], [bold yellow]ddp[/bold yellow], [bold yellow]ddp-atmos[/bold yellow], [bold yellow]thd[/bold yellow], [bold yellow]thd-atmos[/bold yellow], [bold yellow]ac4[/bold yellow], [bold yellow]ac4-atmos[/bold yellow] or [bold yellow]aac[/bold yellow]."
            }
            Self::Downmix => {
                "[bold yellow]-dm[/bold yellow]/[bold yellow]--downmix[/bold yellow] can only be [bold yellow]1[/bold yellow], [bold yellow]2[/bold yellow] or [bold yellow]6[/bold yellow]."
            }
            Self::DownmixMismatch => {
                "downmix value has to be lower than the number of input channels."
            }
            Self::FormatDownmix => {
                "[bold yellow]-dm[/bold yellow]/[bold yellow]--downmix[/bold yellow] only works for [bold yellow]DD[/bold yellow]/[bold yellow]DDP[/bold yellow]/[bold yellow]AC4[/bold yellow]/[bold yellow]AAC[/bold yellow] encoding and for [bold yellow]TrueHD[/bold yellow] ([bold yellow]2[/bold yellow] or [bold yellow]6[/bold yellow]), not for the Atmos formats."
            }
            Self::ThdDownmixMono => {
                "[bold yellow]-dm 1[/bold yellow] doesn't work for [bold yellow]TrueHD[/bold yellow], it has no mono (2.0, 5.1 and 7.1 only)."
            }
            Self::DownmixValue => {
                "[bold yellow]-dm[/bold yellow]/[bold yellow]--downmix[/bold yellow] can only be [bold yellow]2[/bold yellow] or [bold yellow]6[/bold yellow] for AC4 and [bold yellow]2[/bold yellow] for AAC."
            }
            Self::DialnormFormat => {
                "custom dialnorm is only supported for [bold yellow]DD[/bold yellow], [bold yellow]DDP[/bold yellow], [bold yellow]DDP Atmos[/bold yellow], [bold yellow]TrueHD[/bold yellow], [bold yellow]TrueHD Atmos[/bold yellow] and [bold yellow]AC4[/bold yellow] encoding."
            }
            Self::PhaseShiftFormat => {
                "[bold yellow]-nps[/bold yellow]/[bold yellow]--no-phase-shift[/bold yellow] only works for [bold yellow]DD[/bold yellow] and [bold yellow]DDP[/bold yellow]."
            }
            Self::Ac4Drc => {
                "DRC [bold yellow]none[/bold yellow] is not supported for AC4 encoding."
            }
            Self::ShimAtmos => {
                "dee_shim is required for [bold yellow]🤠[/bold yellow], put dee_shim next to deew or in your PATH (https://github.com/truehdd/dee-shim)"
            }
            Self::ShimThd => {
                "dee_shim is required for TrueHD with DRC [bold yellow]none[/bold yellow] (the default), put dee_shim next to deew or in your PATH, or use [bold yellow]-r[/bold yellow] with another profile"
            }
            Self::ShimDdp71 => {
                "dee_shim is required for 7.1 DDP with the standard profile, put dee_shim next to deew or in your PATH, or use [bold yellow]--ddp-mode bluray[/bold yellow] (768-1664 kbps)"
            }
            Self::ShimDdp70 => {
                "dee_shim is required for 7.0 DDP, put dee_shim next to deew or in your PATH (https://github.com/truehdd/dee-shim)"
            }
            Self::Ac4DialnormChannels => {
                "custom dialnorm ([bold yellow]-dn[/bold yellow]) doesn't work for [bold yellow]AC4[/bold yellow] with 7.1 input, DEE's AC4 encoder downmixes it to 5.1 itself."
            }
            Self::AacInput => {
                "AAC needs a 2.0, 5.1 or 7.1 input (DEE's AAC encoder always outputs 2.0)"
            }
            Self::InputLayout => {
                "the channel layout [bold yellow]🤠[/bold yellow] is not supported, 5 channels have to be 5.0 and 7 channels 7.0 (no LFE)."
            }
            Self::AtmosInput => {
                "[bold yellow]ddp-atmos[/bold yellow], [bold yellow]thd-atmos[/bold yellow] and [bold yellow]ac4-atmos[/bold yellow] need a TrueHD input with an Atmos presentation, [bold yellow]🤠[/bold yellow] is not a TrueHD file."
            }
            Self::AtmosNoAtmos => {
                "[bold yellow]ddp-atmos[/bold yellow], [bold yellow]thd-atmos[/bold yellow] and [bold yellow]ac4-atmos[/bold yellow] need a TrueHD input with an Atmos presentation, [bold yellow]🤠[/bold yellow] has no Atmos."
            }
            Self::ThdMonoInput => {
                "input channels for TrueHD encoding can only be [bold yellow]2[/bold yellow]/[bold yellow]6[/bold yellow]/[bold yellow]8[/bold yellow]. "
            }
            Self::Drc => {
                "allowed DRC values: [bold yellow]none[/bold yellow], [bold yellow]film_light[/bold yellow], [bold yellow]film_standard[/bold yellow], [bold yellow]music_light[/bold yellow], [bold yellow]music_standard[/bold yellow], [bold yellow]speech[/bold yellow]."
            }
            Self::LinuxDthd => {
                "[bold yellow]🤠[/bold yellow] does not exist. The Linux DEE has no TrueHD encoder, build it with https://github.com/truehdd/dthd-mac2linux"
            }
            Self::Path => "path [bold yellow]🤠[/bold yellow] does not exist.",
            Self::SampleMismatch => "each input has to have the same sample rate:\n🤠",
            Self::ChannelMismatch => "each input has to have the same channel count:\n🤠",
            Self::Channels => {
                "number of input channels can only be [bold yellow]1[/bold yellow], [bold yellow]2[/bold yellow], [bold yellow]5[/bold yellow], [bold yellow]6[/bold yellow], [bold yellow]7[/bold yellow] or [bold yellow]8[/bold yellow]."
            }
            Self::BinaryExist => "[bold yellow]🤠[/bold yellow] does not exist.",
            Self::ConfigKey => {
                "the following keys are missing from your config file: 🤠.\nUpdate your config file."
            }
            Self::Ac4InputChannels => {
                "input channels for AC4 encoding can only be [bold yellow]2[/bold yellow]/[bold yellow]6[/bold yellow]/[bold yellow]8[/bold yellow]."
            }
            Self::DdpMode => {
                "[bold yellow]--ddp-mode[/bold yellow] can only be [bold yellow]standard[/bold yellow] or [bold yellow]bluray[/bold yellow]."
            }
            Self::DdpAtmosMode => {
                "[bold yellow]--ddp-atmos-mode[/bold yellow] can only be [bold yellow]streaming[/bold yellow], [bold yellow]standard[/bold yellow] or [bold yellow]bluray[/bold yellow]."
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bitrates() {
        assert_eq!(BITRATES.len(), 15);
        assert_eq!(allowed_bitrates("ac4_atmos"), &[256, 320, 448, 768]);
        assert_eq!(allowed_bitrates("ddp_71").last(), Some(&1792));
        assert_eq!(allowed_bitrates("ddp_71").len(), 19);
        assert_eq!(allowed_bitrates("ddp_71_bluray").last(), Some(&1664));
        assert_eq!(allowed_bitrates("ddp_atmos_streaming").first(), Some(&384));
        assert_eq!(allowed_bitrates("ddp_atmos_standard").last(), Some(&2048));
        assert_eq!(allowed_bitrates("ddp_atmos_bluray").first(), Some(&1152));
        for (key, list) in BITRATES {
            assert!(list.windows(2).all(|w| w[0] < w[1]), "{key} is not sorted");
        }
    }

    #[test]
    fn logos() {
        assert_eq!(LOGOS.len(), 3);
        for l in &LOGOS[..2] {
            assert!(l.contains("Dolby Encoding Engine") && l.contains("Wrapper"));
        }
        assert!(LOGOS[2].contains("E N G I N E   W R A P P E R"));
    }

    #[test]
    fn logos_render() {
        for l in LOGOS {
            let out = crate::markup::render(l, true);
            for bad in ["[color(", "[/]", "[bold", "[blue"] {
                assert!(!out.contains(bad), "unparsed {bad}");
            }
        }
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
}
