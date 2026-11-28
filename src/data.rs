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
    ("ac4_ims", &[64, 96, 128, 144, 256, 320]),
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
    LowPassFilterFormat,
    Surround3dbFormat,
    WarpModeFormat,
    BedConformFormat,
    WarpMode,
    PipeMode,
    OnOff,
    Ac4Drc,
    AtmosInput,
    AtmosNoAtmos,
    Ac4DialnormChannels,
    InputLayout,
    AacInput,
    ShimAtmos,
    ShimThd,
    ShimThd96,
    ShimOld,
    ShimVersion,
    TruehddOld,
    TruehddVersion,
    HarlettyMissing,
    HarlettyOld,
    HarlettyVersion,
    HarlettyInfo,
    ShimDdp71,
    ShimDdp70,
    ShimDdp50,
    ShimMeasure,
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
    Ac4ImsMode,
    Ac4ImsMusicDrc,
    Ac4ImsLfe,
    Ac4ImsDownmix,
    Ac4ImsInput,
    Ac4ImsMixed,
    MasterFormat,
    MasterMixed,
    OutputNameSingle,
    OutputNameAndTemplate,
    OutputNameMeasureOnly,
    OutputName,
    OutputTemplate,
    OutputClash,
    OutputIsInput,
}

impl ErrorKind {
    pub fn message(&self) -> &'static str {
        match self {
            Self::Changelog => "couldn't fetch changelog from GitHub.",
            Self::Delay => "unsupported delay value.",
            Self::CreateDir => "failed to create [bold yellow]🤠[/bold yellow].",
            Self::Format => {
                "[bold yellow]-f[/bold yellow]/[bold yellow]--format[/bold yellow] can only be [bold yellow]dd[/bold yellow], [bold yellow]ddp[/bold yellow], [bold yellow]ddp-atmos[/bold yellow], [bold yellow]thd[/bold yellow], [bold yellow]thd-atmos[/bold yellow], [bold yellow]ac4[/bold yellow], [bold yellow]ac4-atmos[/bold yellow], [bold yellow]ac4-ims[/bold yellow] or [bold yellow]aac[/bold yellow]."
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
                "[bold yellow]-sps[/bold yellow]/[bold yellow]--surround-phase-shift[/bold yellow] only works for [bold yellow]DD[/bold yellow], [bold yellow]DDP[/bold yellow] and [bold yellow]DDP Atmos[/bold yellow]."
            }
            Self::LowPassFilterFormat => {
                "[bold yellow]-lpf[/bold yellow]/[bold yellow]--low-pass-filter[/bold yellow] only works for [bold yellow]DD[/bold yellow] and [bold yellow]DDP[/bold yellow]."
            }
            Self::Surround3dbFormat => {
                "[bold yellow]-s3a[/bold yellow]/[bold yellow]--surround-3db-attenuation[/bold yellow] only works for [bold yellow]DD[/bold yellow], [bold yellow]DDP[/bold yellow] and [bold yellow]TrueHD[/bold yellow]."
            }
            Self::WarpModeFormat => {
                "[bold yellow]-wm[/bold yellow]/[bold yellow]--warp-mode[/bold yellow] only works for [bold yellow]DDP Atmos[/bold yellow], [bold yellow]TrueHD Atmos[/bold yellow], [bold yellow]AC4 Atmos[/bold yellow], [bold yellow]AC4 IMS[/bold yellow] and [bold yellow]-mo[/bold yellow]."
            }
            Self::BedConformFormat => {
                "[bold yellow]-bc[/bold yellow]/[bold yellow]--bed-conform[/bold yellow] only works for [bold yellow]DDP Atmos[/bold yellow], [bold yellow]TrueHD Atmos[/bold yellow], [bold yellow]AC4 Atmos[/bold yellow], [bold yellow]AC4 IMS[/bold yellow] and [bold yellow]-mo[/bold yellow]."
            }
            Self::WarpMode => {
                "[bold yellow]-wm[/bold yellow]/[bold yellow]--warp-mode[/bold yellow] can only be [bold yellow]normal[/bold yellow], [bold yellow]warping[/bold yellow], [bold yellow]prologiciix[/bold yellow] or [bold yellow]loro[/bold yellow]."
            }
            Self::PipeMode => {
                "[bold yellow]-p[/bold yellow]/[bold yellow]--pipe[/bold yellow] can only be [bold yellow]auto[/bold yellow], [bold yellow]always[/bold yellow] or [bold yellow]never[/bold yellow]."
            }
            Self::OnOff => {
                "[bold yellow]-sps[/bold yellow], [bold yellow]-lpf[/bold yellow], [bold yellow]-s3a[/bold yellow] and [bold yellow]-bc[/bold yellow] can only be [bold yellow]on[/bold yellow] or [bold yellow]off[/bold yellow]."
            }
            Self::Ac4Drc => {
                "DRC [bold yellow]none[/bold yellow] is not supported for AC4 encoding (AC4 Atmos supports it)."
            }
            Self::ShimAtmos => {
                "dee_shim is required for [bold yellow]🤠[/bold yellow], put dee_shim next to deew or in your PATH (https://github.com/truehdd/dee-shim)"
            }
            Self::ShimThd => {
                "dee_shim is required for TrueHD with DRC [bold yellow]none[/bold yellow] (the default), put dee_shim next to deew or in your PATH, or use [bold yellow]-r[/bold yellow] with another profile"
            }
            Self::ShimThd96 => {
                "dee_shim is required for 96 kHz TrueHD (DEE's own encoder applies its apodising filter, so it wouldn't be lossless), put dee_shim next to deew or in your PATH (https://github.com/truehdd/dee-shim)"
            }
            Self::ShimOld => {
                "dee_shim [bold yellow]🤠[/bold yellow] is too old, deew needs 0.5.0 or newer: update it with [bold yellow]-gt[/bold yellow]"
            }
            Self::ShimVersion => {
                "can't read the version of dee_shim, deew needs 0.5.0 or newer: update it with [bold yellow]-gt[/bold yellow]"
            }
            Self::TruehddOld => {
                "truehdd [bold yellow]🤠[/bold yellow] is too old, deew needs 0.6.3 or newer: update it with [bold yellow]-gt[/bold yellow]"
            }
            Self::TruehddVersion => {
                "can't read the version of truehdd, deew needs 0.6.3 or newer: update it with [bold yellow]-gt[/bold yellow]"
            }
            Self::HarlettyMissing => {
                "[bold yellow]harletty[/bold yellow] is required for DTS:X and Auro-3D sources of the Atmos formats and AC-4 IMS (to read and decode them), put harletty next to deew or in your PATH, or get it with [bold yellow]-gt[/bold yellow] (https://github.com/harletty/harletty-bridge)"
            }
            Self::HarlettyOld => {
                "harletty [bold yellow]🤠[/bold yellow] is too old, deew needs 0.9.0 or newer: update it with [bold yellow]-gt[/bold yellow]"
            }
            Self::HarlettyVersion => {
                "can't read the version of harletty, deew needs 0.9.0 or newer: update it with [bold yellow]-gt[/bold yellow]"
            }
            Self::HarlettyInfo => "harletty couldn't read 🤠",
            Self::ShimDdp71 => {
                "dee_shim is required for 7.1 DDP with the standard profile, put dee_shim next to deew or in your PATH, or use [bold yellow]--ddp-mode bluray[/bold yellow] (768-1664 kbps)"
            }
            Self::ShimDdp70 => {
                "dee_shim is required for 7.0 DDP, put dee_shim next to deew or in your PATH (https://github.com/truehdd/dee-shim)"
            }
            Self::ShimDdp50 => {
                "dee_shim is required for 5.0 DD and DDP, put dee_shim next to deew or in your PATH, or get it with [bold yellow]-gt[/bold yellow] (https://github.com/truehdd/dee-shim)"
            }
            Self::ShimMeasure => {
                "dee_shim is required for [bold yellow]-mo[/bold yellow] of 7.1 and 7.0 input (DEE's own loudness meter refuses 8 channels), put dee_shim next to deew or in your PATH (https://github.com/truehdd/dee-shim)"
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
                "[bold yellow]ddp-atmos[/bold yellow], [bold yellow]thd-atmos[/bold yellow] and [bold yellow]ac4-atmos[/bold yellow] need a TrueHD Atmos, DTS:X or Auro-3D (24-bit DTS-HD MA, FLAC, PCM or TrueHD with a harletty that reads PCM) input or an Atmos master, [bold yellow]🤠[/bold yellow] is none of them."
            }
            Self::AtmosNoAtmos => {
                "[bold yellow]ddp-atmos[/bold yellow], [bold yellow]thd-atmos[/bold yellow] and [bold yellow]ac4-atmos[/bold yellow] need a TrueHD Atmos, DTS:X or Auro-3D (24-bit DTS-HD MA, FLAC, PCM or TrueHD with a harletty that reads PCM) input or an Atmos master, [bold yellow]🤠[/bold yellow] has no Atmos, DTS:X or Auro-3D."
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
            Self::Ac4ImsMode => {
                "[bold yellow]--ac4-ims-mode[/bold yellow] can only be [bold yellow]general[/bold yellow] or [bold yellow]music[/bold yellow]."
            }
            Self::Ac4ImsMusicDrc => {
                "[bold yellow]--ac4-ims-mode music[/bold yellow] (or [bold yellow]ac4_ims = 'music'[/bold yellow] in the config) needs the DRC profile [bold yellow]music_light[/bold yellow] (DEE's music mode takes no other), leave out [bold yellow]-r[/bold yellow] or use [bold yellow]-r music_light[/bold yellow]."
            }
            Self::Ac4ImsDownmix => {
                "[bold yellow]-dm[/bold yellow]/[bold yellow]--downmix[/bold yellow] doesn't work for [bold yellow]ac4-ims[/bold yellow], it always encodes immersive stereo (a 7.1 input is downmixed to 5.1 first)."
            }
            Self::Ac4ImsLfe => {
                "[bold yellow]-dl[/bold yellow]/[bold yellow]--disable-lfe[/bold yellow] doesn't work for [bold yellow]ac4-ims[/bold yellow], it needs the LFE channel of a 5.1 source."
            }
            Self::Ac4ImsInput => {
                "[bold yellow]ac4-ims[/bold yellow] needs a 5.1 (or 5.0, 7.0, 7.1) or an Atmos input, stereo and mono can't be encoded to immersive stereo."
            }
            Self::Ac4ImsMixed => {
                "[bold yellow]ac4-ims[/bold yellow] can't encode Atmos inputs (TrueHD Atmos, DTS:X, Auro-3D, ADM BWF, DAMF) and channel-based inputs in the same run."
            }
            Self::MasterFormat => {
                "Atmos masters (ADM BWF, DAMF) can't be encoded to [bold yellow]🤠[/bold yellow], use [bold yellow]ddp-atmos[/bold yellow], [bold yellow]thd-atmos[/bold yellow], [bold yellow]ac4-atmos[/bold yellow] or [bold yellow]ac4-ims[/bold yellow] (or [bold yellow]dd[/bold yellow]/[bold yellow]ddp[/bold yellow] for a 5.1 render)."
            }
            Self::MasterMixed => {
                "Atmos masters (ADM BWF, DAMF) can't be encoded to [bold yellow]dd[/bold yellow]/[bold yellow]ddp[/bold yellow] together with other inputs."
            }
            Self::OutputNameSingle => {
                "[bold yellow]-on[/bold yellow]/[bold yellow]--output-name[/bold yellow] only works with a single input file (found 🤠), use [bold yellow]-ot[/bold yellow]/[bold yellow]--output-template[/bold yellow] for several."
            }
            Self::OutputNameAndTemplate => {
                "[bold yellow]-on[/bold yellow]/[bold yellow]--output-name[/bold yellow] and [bold yellow]-ot[/bold yellow]/[bold yellow]--output-template[/bold yellow] can't be used together."
            }
            Self::OutputNameMeasureOnly => {
                "[bold yellow]-on[/bold yellow]/[bold yellow]--output-name[/bold yellow] and [bold yellow]-ot[/bold yellow]/[bold yellow]--output-template[/bold yellow] don't work with [bold yellow]-mo[/bold yellow], it writes no output file."
            }
            Self::OutputName => "invalid output name: 🤠",
            Self::OutputTemplate => "invalid output template: 🤠",
            Self::OutputClash => {
                "more than one output would be written to the same file, use [bold yellow]-ot[/bold yellow]/[bold yellow]--output-template[/bold yellow] to name them differently:\n🤠"
            }
            Self::OutputIsInput => "an output would overwrite its own input file:\n🤠",
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
        assert_eq!(BITRATES.len(), 16);
        assert_eq!(allowed_bitrates("ac4_ims"), &[64, 96, 128, 144, 256, 320]);
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
