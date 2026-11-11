//! Pure decisions of the encoding pipeline: argument validation, instance
//! count, channel layout, prompts and bitrate selection.

use crate::cli::Args;
use crate::config::{DdpAtmosMode, DdpMode, DefaultBitrates, DefaultModes, MaxInstances};
use crate::data::{ErrorKind, allowed_bitrates};
use crate::error::{escape, print_exit};
use crate::probe::Probe;
use crate::util::{clamp_f, clamp_i, find_closest_allowed};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Dd,
    Ddp,
    DdpAtmos,
    Thd,
    ThdAtmos,
    Ac4,
    Ac4Atmos,
    Aac,
}

impl Format {
    /// Parse an (already lowercased) format name.
    pub fn parse(s: &str) -> Option<Format> {
        Some(match s {
            "dd" => Format::Dd,
            "ddp" => Format::Ddp,
            // the underscore spellings are silent aliases of the old releases
            "ddp-atmos" | "ddp_atmos" => Format::DdpAtmos,
            "thd" => Format::Thd,
            "thd-atmos" | "thd_atmos" => Format::ThdAtmos,
            "ac4" => Format::Ac4,
            "ac4-atmos" | "ac4_atmos" => Format::Ac4Atmos,
            "aac" => Format::Aac,
            _ => return None,
        })
    }

    /// Name shown in the summary table.
    pub fn summary_name(self) -> &'static str {
        match self {
            Format::Dd => "DD",
            Format::Ddp => "DDP",
            Format::DdpAtmos => "DDP Atmos",
            Format::Thd => "TrueHD",
            Format::ThdAtmos => "TrueHD Atmos",
            Format::Ac4 => "AC4",
            Format::Ac4Atmos => "AC4 Atmos",
            Format::Aac => "AAC",
        }
    }

    /// The name for `-f`.
    pub fn name(self) -> &'static str {
        match self {
            Format::Dd => "dd",
            Format::Ddp => "ddp",
            Format::DdpAtmos => "ddp-atmos",
            Format::Thd => "thd",
            Format::ThdAtmos => "thd-atmos",
            Format::Ac4 => "ac4",
            Format::Ac4Atmos => "ac4-atmos",
            Format::Aac => "aac",
        }
    }

    /// The DEE tool (file name without the extension) that encodes the format.
    pub fn dee_tool(self) -> &'static str {
        match self {
            Format::Dd | Format::Ddp => "dee_ddp_encoder",
            Format::DdpAtmos => "dee_ddpjoc_encoder",
            Format::Ac4 => "dee_ac4_encoder",
            Format::Ac4Atmos => "dee_ac4ajoc_encoder",
            Format::Aac => "dee_aac_encoder",
            Format::Thd | Format::ThdAtmos => "dee_dthd_encoder",
        }
    }

    /// Formats that are encoded from a TrueHD Atmos input decoded by truehdd.
    pub fn is_atmos(self) -> bool {
        matches!(self, Format::DdpAtmos | Format::ThdAtmos | Format::Ac4Atmos)
    }

    /// Formats that accept a custom dialnorm.
    pub fn has_dialnorm(self) -> bool {
        matches!(
            self,
            Format::Dd
                | Format::Ddp
                | Format::DdpAtmos
                | Format::Thd
                | Format::ThdAtmos
                | Format::Ac4
        )
    }

    /// DEE's AC4 encoders don't accept `skip:dialnorm`, so AC4 measures and
    /// gets the value through the loudness manifest.
    pub fn dialnorm_via_manifest(self) -> bool {
        matches!(self, Format::Ac4)
    }

    /// The encoder's output delay in samples at 48 kHz that deew trims from
    /// the start (measured: DD/DDP decode 256 samples late; DEE's AAC encoder
    /// primes 1600 samples in ADTS; the AC-4 encoder's manifest gives an
    /// offset of -2048).
    pub fn encoder_delay(self) -> u32 {
        match self {
            Format::Dd | Format::Ddp | Format::DdpAtmos => 256,
            Format::Ac4 | Format::Ac4Atmos => 2048,
            Format::Aac => 1600,
            Format::Thd | Format::ThdAtmos => 0,
        }
    }

    /// Extension of the output file.
    pub fn extension(self, outchannels: i64, bluray: bool) -> &'static str {
        match self {
            Format::Dd => "ac3",
            Format::Ddp if outchannels == 8 && bluray => "eb3",
            Format::DdpAtmos if bluray => "eb3",
            Format::Ddp | Format::DdpAtmos => "ec3",
            Format::Ac4 | Format::Ac4Atmos => "ac4",
            Format::Aac => "aac",
            Format::Thd | Format::ThdAtmos => "thd",
        }
    }
}

/// An error that ends the program through `print_exit`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fail {
    pub kind: ErrorKind,
    pub insert: Option<String>,
}

impl Fail {
    pub fn new(kind: ErrorKind) -> Fail {
        Fail { kind, insert: None }
    }

    pub fn with(kind: ErrorKind, insert: impl Into<String>) -> Fail {
        Fail {
            kind,
            insert: Some(insert.into()),
        }
    }

    pub fn exit(self) -> ! {
        print_exit(self.kind, self.insert.as_deref())
    }
}

/// `channel_number_to_name`. Only called with validated channel counts.
pub fn channel_name(channels: i64) -> &'static str {
    match channels {
        1 => "mono",
        2 => "stereo",
        6 => "5.1",
        8 => "7.1",
        5 => "5.0",
        7 => "7.0",
        _ => unreachable!("channel count {channels} was validated"),
    }
}

/// Channels of the wav DEE gets: 5.0 and 7.0 inputs get a silent LFE.
pub fn wav_channels(channels: i64) -> i64 {
    match channels {
        5 => 6,
        7 => 8,
        other => other,
    }
}

fn parse_int(text: &str) -> Result<i64, String> {
    text.trim()
        .parse::<i64>()
        .map_err(|_| format!("invalid number of instances: {text}"))
}

/// Number of simultaneous encodes. `arg` is `-in` (an empty string counts as
/// not given), `config` the `max_instances` setting. The result stays a
/// float, `50%` of 5 cpus is 2.5.
pub fn compute_instances(
    arg: Option<&str>,
    config: &MaxInstances,
    cpu: usize,
) -> Result<f64, String> {
    let text = match arg.filter(|s| !s.is_empty()) {
        Some(s) => s.to_string(),
        None => match config {
            MaxInstances::Count(n) => n.to_string(),
            MaxInstances::Text(s) => s.clone(),
        },
    };
    let cpu = cpu as f64;
    let mut instances = if text.ends_with('%') {
        cpu * (parse_int(&text.replace('%', ""))? as f64 / 100.0)
    } else {
        parse_int(&text)? as f64
    };
    instances = clamp_f(instances, 1.0, cpu);
    if instances == 0.0 {
        instances = 1.0;
    }
    Ok(instances)
}

/// Arguments after validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Validated {
    pub format: Format,
    /// `None` when not given or 0 (Python truthiness).
    pub downmix: Option<i64>,
    /// 0 (DEE's measurement) for formats without custom dialnorm and with `-mo`.
    pub dialnorm: i64,
    pub track_index: i64,
    /// The DRC profile: `-r`, else the default of the format.
    pub drc: String,
    /// `--ddp-mode`, else the config's default (7.1 DDP only).
    pub ddp_mode: DdpMode,
    /// `--ddp-atmos-mode`, else the config's default (DDP Atmos only).
    pub atmos_mode: DdpAtmosMode,
}

pub const DRC_PROFILES: [&str; 6] = [
    "none",
    "film_light",
    "film_standard",
    "music_light",
    "music_standard",
    "speech",
];

/// The DRC profile when `-r` is not given: `none`, except for the AC4
/// formats, which have no such preset.
pub fn default_drc(format: Format) -> &'static str {
    match format {
        Format::Ac4 | Format::Ac4Atmos => "music_light",
        _ => "none",
    }
}

/// Validate the arguments, the first problem wins.
pub fn validate_args(args: &Args, modes: &DefaultModes) -> Result<Validated, Fail> {
    let dialnorm = clamp_i(args.dialnorm, -31, 0);
    let track_index = args.track_index.max(0);
    let downmix = args.downmix.filter(|&d| d != 0);

    let format = Format::parse(&args.format.to_lowercase()).ok_or(Fail::new(ErrorKind::Format))?;
    if let Some(d) = downmix {
        if ![1, 2, 6].contains(&d) {
            return Err(Fail::new(ErrorKind::Downmix));
        }
        if format.is_atmos() {
            return Err(Fail::new(ErrorKind::FormatDownmix));
        }
        if format == Format::Thd && d == 1 {
            return Err(Fail::new(ErrorKind::ThdDownmixMono));
        }
    }
    if args
        .drc
        .as_deref()
        .is_some_and(|d| !DRC_PROFILES.contains(&d))
    {
        return Err(Fail::new(ErrorKind::Drc));
    }
    let drc = args
        .drc
        .clone()
        .unwrap_or_else(|| default_drc(format).to_string());
    let ddp_mode = match args.ddp_mode.as_deref() {
        Some(m) => DdpMode::parse(m).ok_or(Fail::new(ErrorKind::DdpMode))?,
        None => modes.ddp,
    };
    let atmos_mode = match args.ddp_atmos_mode.as_deref() {
        Some(m) => DdpAtmosMode::parse(m).ok_or(Fail::new(ErrorKind::DdpAtmosMode))?,
        None => modes.ddp_atmos,
    };
    if args.measure_only && format.is_atmos() {
        return Err(Fail::new(ErrorKind::MeasureAtmos));
    }
    if matches!(format, Format::Ac4 | Format::Ac4Atmos)
        && args.drc.as_deref() == Some("none")
        && !args.measure_only
    {
        return Err(Fail::new(ErrorKind::Ac4Drc));
    }
    if dialnorm != 0 && !args.measure_only && !format.has_dialnorm() {
        return Err(Fail::new(ErrorKind::DialnormFormat));
    }
    Ok(Validated {
        format,
        downmix,
        dialnorm: if args.measure_only { 0 } else { dialnorm },
        track_index,
        drc,
        ddp_mode,
        atmos_mode,
    })
}

/// What the probed inputs have in common, and their bit depths.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputInfo {
    pub samplerate: i64,
    pub channels: i64,
    /// Per input, normalised to 16, 24 or 32.
    pub bit_depths: Vec<u32>,
}

fn all_equal(list: &[i64]) -> bool {
    list.iter().all(|&x| x == list[0])
}

/// Sample rate and channel mismatch checks, bit depth normalisation. `probes` is not empty.
pub fn check_inputs(probes: &[Probe]) -> Result<InputInfo, Fail> {
    let rates: Vec<i64> = probes.iter().map(|p| p.sample_rate).collect();
    let channels: Vec<i64> = probes.iter().map(|p| p.channels).collect();
    if !all_equal(&rates) {
        return Err(Fail::with(
            ErrorKind::SampleMismatch,
            crate::util::py_int_list(&rates),
        ));
    }
    if !all_equal(&channels) {
        return Err(Fail::with(
            ErrorKind::ChannelMismatch,
            crate::util::py_int_list(&channels),
        ));
    }
    Ok(InputInfo {
        samplerate: rates[0],
        channels: channels[0],
        bit_depths: probes
            .iter()
            .map(|p| normalize_bit_depth(p.bit_depth))
            .collect(),
    })
}

pub fn normalize_bit_depth(depth: i64) -> u32 {
    match depth {
        16 | 24 | 32 => depth as u32,
        d if d < 16 => 16,
        d if 16 < d && d < 24 => 24,
        _ => 32,
    }
}

/// The Atmos formats need TrueHD inputs with an Atmos presentation;
/// `files` and `probes` belong together.
pub fn check_atmos_inputs(files: &[String], probes: &[Probe]) -> Result<(), Fail> {
    let pairs = || files.iter().zip(probes);
    if let Some((file, _)) = pairs().find(|(_, probe)| !probe.is_truehd) {
        return Err(Fail::with(ErrorKind::AtmosInput, escape(file)));
    }
    match pairs().find(|(_, probe)| !probe.is_atmos) {
        Some((file, _)) => Err(Fail::with(ErrorKind::AtmosNoAtmos, escape(file))),
        None => Ok(()),
    }
}

/// The AC4 `-dn` flow measures the wav with `dee_measure_loudness`, which
/// does not take 7.1 audio; and DEE's AC4 encoder downmixes 7.1 to 5.1 itself,
/// so a measurement of the 7.1 wav (or of a downmix) would not match what is
/// encoded. `measures` is true when that manifest flow is used.
pub fn check_ac4_dialnorm_channels(channels: i64, measures: bool) -> Result<(), Fail> {
    if measures && wav_channels(channels) == 8 {
        return Err(Fail::new(ErrorKind::Ac4DialnormChannels));
    }
    Ok(())
}

/// What happens with the LFE channel of the output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lfe {
    /// The output has an LFE channel.
    Normal,
    /// A real 5.0 / 7.0 output, the stream has no LFE channel.
    Removed,
    /// 5.1 / 7.1 with a silent LFE channel (the encoder can't leave it out).
    Silent,
}

/// Output channel layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    /// Output channels (of the wav for DEE, so 6 for 5.0), 0 for the Atmos formats.
    pub outchannels: i64,
    /// Value of the `-c` option of DEE; `None` if the output has the channels
    /// of the input (or DEE downmixes by itself, AAC).
    pub layout_arg: Option<&'static str>,
    pub lfe: Lfe,
    /// What `-dl` did or could not do, as notes (markup).
    pub notes: Vec<String>,
}

impl Layout {
    /// The channels of the output as shown: `5.0`, `7.1`, `stereo`, ...
    pub fn output_name(&self) -> String {
        match (self.outchannels, self.lfe) {
            (6, Lfe::Removed) => "5.0".to_string(),
            (8, Lfe::Removed) => "7.0".to_string(),
            (6 | 8, Lfe::Silent) => format!("{} (silent LFE)", channel_name(self.outchannels)),
            (0, _) => "Atmos".to_string(),
            (n, _) => channel_name(n).to_string(),
        }
    }
}

/// Inputs with 5 or 7 channels have to be 5.0 / 7.0 (no LFE). `files` and
/// `probes` belong together.
pub fn check_input_layouts(files: &[String], probes: &[Probe]) -> Result<(), Fail> {
    for (file, probe) in files.iter().zip(probes) {
        let layout = probe.channel_layout.as_str();
        let ok = match probe.channels {
            5 => ["", "5.0", "5.0(side)"].contains(&layout),
            7 => ["", "7.0"].contains(&layout),
            _ => true,
        };
        if !ok {
            return Err(Fail::with(
                ErrorKind::InputLayout,
                format!("{} ({})", layout, escape(file)),
            ));
        }
    }
    Ok(())
}

/// The jobs that encode with dee_shim: `ddp-atmos`, `thd-atmos`, and TrueHD without DRC
/// (the encoder has no such preset). `-mo` does not encode.
pub fn needs_shim(format: Format, drc: &str, measure_only: bool) -> Option<Fail> {
    match format {
        Format::DdpAtmos | Format::ThdAtmos if !measure_only => {
            Some(Fail::with(ErrorKind::ShimAtmos, format.name()))
        }
        Format::Thd if drc == "none" && !measure_only => Some(Fail::new(ErrorKind::ShimThd)),
        _ => None,
    }
}

const NOTE_DL: &str = "[bold yellow]-dl[/bold yellow]";

/// Channel checks and downmix decisions. `disable_lfe` is `-dl`.
pub fn resolve_layout(
    format: Format,
    downmix: Option<i64>,
    channels: i64,
    disable_lfe: bool,
) -> Result<Layout, Fail> {
    if format.is_atmos() {
        let notes = disable_lfe
            .then(|| format!("{NOTE_DL} is ignored for the Atmos formats."))
            .into_iter()
            .collect();
        return Ok(Layout {
            outchannels: 0,
            layout_arg: None,
            lfe: Lfe::Normal,
            notes,
        });
    }
    if ![1, 2, 5, 6, 7, 8].contains(&channels) {
        return Err(Fail::new(ErrorKind::Channels));
    }
    if format == Format::Aac && channels == 1 {
        return Err(Fail::new(ErrorKind::AacInput));
    }
    if downmix.is_some_and(|d| d >= channels) {
        return Err(Fail::new(ErrorKind::DownmixMismatch));
    }
    let valid = match format {
        Format::Ac4 => &[2, 6][..],
        Format::Aac => &[2][..],
        Format::Thd => &[2, 6][..],
        _ => &[1, 2, 6][..],
    };
    if downmix.is_some_and(|d| !valid.contains(&d)) {
        return Err(Fail::new(ErrorKind::DownmixValue));
    }
    if format == Format::Ac4 && channels == 1 {
        return Err(Fail::new(ErrorKind::Ac4InputChannels));
    }
    if format == Format::Thd && channels == 1 {
        return Err(Fail::new(ErrorKind::ThdMonoInput));
    }
    let slots = wav_channels(channels);
    let downmix = match (format, downmix) {
        (Format::Dd | Format::Ac4, None) if slots == 8 => Some(6),
        (_, downmix) => downmix,
    };
    let outchannels = match format {
        // DEE's AAC encoder always outputs 2.0 and downmixes by itself
        Format::Aac => 2,
        _ => downmix.unwrap_or(slots),
    };
    // DEE downmixes by itself, except for TrueHD (ffmpeg does, see `audio_plan`)
    let layout_arg = (!matches!(format, Format::Aac | Format::Thd) && outchannels != slots)
        .then(|| channel_name(outchannels));

    // the LFE channel
    let has_lfe_input = [6, 8].contains(&channels);
    let no_lfe_input = [5, 7].contains(&channels);
    let mut notes = Vec::new();
    let mut lfe = Lfe::Normal;
    if disable_lfe && !has_lfe_input {
        notes.push(if no_lfe_input {
            format!("{NOTE_DL} is not needed, a 5.0 or 7.0 input has no LFE channel.")
        } else {
            format!("{NOTE_DL} only applies to 5.1 and 7.1 inputs, it is ignored.")
        });
    } else if disable_lfe && outchannels < 6 {
        notes.push(format!(
            "{NOTE_DL} is ignored, the output has no LFE channel."
        ));
    }
    if outchannels >= 6 && (no_lfe_input || (disable_lfe && has_lfe_input)) {
        lfe = match format {
            Format::Dd | Format::Ddp => Lfe::Removed,
            _ => {
                notes.push(format!(
                    "{} can't leave out the LFE channel, it is kept silent.",
                    format.summary_name()
                ));
                Lfe::Silent
            }
        };
    }
    Ok(Layout {
        outchannels,
        layout_arg,
        lfe,
        notes,
    })
}

/// A recommendation shown (`-np`) or asked about before encoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Notice {
    pub note: &'static str,
    pub question: &'static str,
}

/// The notices to show, in order.
pub fn notices(format: Format, outchannels: i64, measure_only: bool, dialnorm: i64) -> Vec<Notice> {
    let mut list = Vec::new();
    if [1, 2].contains(&outchannels) && matches!(format, Format::Dd | Format::Ddp) && !measure_only
    {
        list.push(Notice {
            note: "Consider using [bold cyan]qaac[/bold cyan] or [bold cyan]opus[/bold cyan] for [bold yellow]mono[/bold yellow] and [bold yellow]stereo[/bold yellow] encoding.",
            question: "Consider using [bold cyan]qaac[/bold cyan] or [bold cyan]opus[/bold cyan] for [bold yellow]mono[/bold yellow] and [bold yellow]stereo[/bold yellow] encoding, are you sure you want to use [bold cyan]DEE[/bold cyan]?",
        });
    }
    if format == Format::Aac && !measure_only {
        list.push(Notice {
            note: "Consider using [bold cyan]qaac[/bold cyan] for [bold yellow]AAC[/bold yellow] encoding.",
            question: "Consider using [bold cyan]qaac[/bold cyan] for [bold yellow]AAC[/bold yellow] encoding, are you sure you want to use [bold cyan]DEE[/bold cyan]?",
        });
    }
    if outchannels == 2 && format == Format::Thd {
        list.push(Notice {
            note: "Consider using [bold cyan]FLAC[/bold cyan] for lossless [bold yellow]stereo[/bold yellow] encoding.",
            question: "Consider using [bold cyan]FLAC[/bold cyan] for lossless [bold yellow]stereo[/bold yellow] encoding, are you sure you want to use [bold cyan]DEE[/bold cyan]?",
        });
    }
    if dialnorm != 0 {
        list.push(Notice {
            note: "Consider leaving the dialnorm value at 0 (auto), setting it manually can be dangerous.",
            question: "Consider leaving the dialnorm value at 0 (auto), setting it manually can be dangerous, are you sure you want to do it?",
        });
    }
    list
}

/// The effective `--ddp-mode` and `--ddp-atmos-mode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Modes {
    pub ddp: DdpMode,
    pub atmos: DdpAtmosMode,
}

/// Bitrate table and configured default of an output layout; `None` for
/// TrueHD, which has no bitrate.
fn bitrate_entry(
    format: Format,
    outchannels: i64,
    modes: Modes,
    defaults: &DefaultBitrates,
) -> Option<(&'static str, i64)> {
    Some(match (format, outchannels) {
        (Format::Dd, 1) => ("dd_10", defaults.dd_1_0),
        (Format::Dd, 2) => ("dd_20", defaults.dd_2_0),
        (Format::Dd, 6) => ("dd_51", defaults.dd_5_1),
        (Format::Ddp, 1) => ("ddp_10", defaults.ddp_1_0),
        (Format::Ddp, 2) => ("ddp_20", defaults.ddp_2_0),
        (Format::Ddp, 6) => ("ddp_51", defaults.ddp_5_1),
        (Format::Ddp, 8) => match modes.ddp {
            DdpMode::Standard => ("ddp_71", defaults.ddp_7_1_standard),
            DdpMode::Bluray => ("ddp_71_bluray", defaults.ddp_7_1_bluray),
        },
        (Format::DdpAtmos, _) => match modes.atmos {
            DdpAtmosMode::Streaming => ("ddp_atmos_streaming", defaults.ddp_atmos_streaming),
            DdpAtmosMode::Standard => ("ddp_atmos_standard", defaults.ddp_atmos_standard),
            DdpAtmosMode::Bluray => ("ddp_atmos_bluray", defaults.ddp_atmos_bluray),
        },
        (Format::Ac4, 2) => ("ac4_20", defaults.ac4_2_0),
        (Format::Ac4, 6) => ("ac4_51", defaults.ac4_5_1),
        (Format::Ac4Atmos, _) => ("ac4_atmos", defaults.ac4_atmos),
        (Format::Aac, _) => ("aac_20", defaults.aac_2_0),
        (Format::Thd | Format::ThdAtmos, _) => return None,
        // The layouts are resolved before, so this cannot happen.
        _ => unreachable!("no {format:?} bitrates for {outchannels} channels"),
    })
}

/// Pick the bitrate: the requested one (or the configured default) rounded to
/// the closest value DEE accepts. `None` for TrueHD.
pub fn select_bitrate(
    format: Format,
    outchannels: i64,
    modes: Modes,
    requested: Option<i64>,
    defaults: &DefaultBitrates,
) -> Option<u32> {
    let (table, default) = bitrate_entry(format, outchannels, modes, defaults)?;
    let requested = requested.filter(|&b| b != 0).unwrap_or(default);
    Some(find_closest_allowed(requested, allowed_bitrates(table)))
}

/// How 7.1 DDP is encoded: the standard profile (E-AC-3 independent and
/// dependent substreams, through dee_shim) or the Blu-ray profile (AC-3 core
/// and dependent substream, native).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Profile71 {
    Standard,
    Bluray,
}

impl Profile71 {
    pub fn name(self) -> &'static str {
        match self {
            Profile71::Standard => "Standard",
            Profile71::Bluray => "Blu-ray",
        }
    }
}

/// The profile of a 7.1 DDP output; `None` for everything else.
/// `--ddp-mode bluray` gives the Blu-ray profile, otherwise it is the standard one.
pub fn profile_71(format: Format, outchannels: i64, mode: DdpMode) -> Option<Profile71> {
    if format != Format::Ddp || outchannels != 8 {
        return None;
    }
    Some(match mode {
        DdpMode::Bluray => Profile71::Bluray,
        DdpMode::Standard => Profile71::Standard,
    })
}

/// DDP with 8 output channels needs dee_shim, except 7.1 with `--ddp-mode bluray`.
pub fn check_shim_for_ddp(
    format: Format,
    layout: &Layout,
    mode: DdpMode,
    measure_only: bool,
    shim_found: bool,
) -> Result<(), Fail> {
    if measure_only || shim_found || format != Format::Ddp || layout.outchannels != 8 {
        return Ok(());
    }
    if layout.lfe == Lfe::Removed {
        Err(Fail::new(ErrorKind::ShimDdp70))
    } else if mode == DdpMode::Standard {
        Err(Fail::new(ErrorKind::ShimDdp71))
    } else {
        Ok(())
    }
}

static FILENAME_DELAY_RE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
    regex::Regex::new(r"^.+DELAY ([-|+]?[0-9]+m?s)\..+").expect("valid regex")
});

/// The delay parsed from a `... DELAY -24ms.ext` file name, if it has one.
pub fn filename_delay(path: &str) -> Option<String> {
    FILENAME_DELAY_RE.captures(path).map(|c| {
        let delay = &c[1];
        if delay.starts_with(['-', '+']) {
            delay.to_string()
        } else {
            format!("+{delay}")
        }
    })
}

/// Delay of an input: `args.delay`, else parsed from the file name, else
/// `+0ms`.
pub fn input_delay(path: &str, arg: Option<&str>) -> String {
    if let Some(delay) = arg.filter(|d| !d.is_empty()) {
        return delay.to_string();
    }
    filename_delay(path).unwrap_or_else(|| "+0ms".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::DefaultBitrates;

    #[test]
    fn static_regexes_compile() {
        std::sync::LazyLock::force(&super::FILENAME_DELAY_RE);
    }

    fn defaults() -> DefaultBitrates {
        DefaultBitrates {
            dd_1_0: 128,
            dd_2_0: 256,
            dd_5_1: 640,
            ddp_1_0: 128,
            ddp_2_0: 256,
            ddp_5_1: 1024,
            ddp_7_1_standard: 1536,
            ddp_7_1_bluray: 1536,
            ddp_atmos_streaming: 1024,
            ddp_atmos_standard: 1536,
            ddp_atmos_bluray: 1536,
            ac4_2_0: 256,
            ac4_5_1: 448,
            ac4_atmos: 448,
            aac_2_0: 256,
        }
    }

    fn validate_args_d(a: &Args) -> Result<Validated, Fail> {
        validate_args(a, &DefaultModes::default())
    }

    fn pct(s: &str) -> MaxInstances {
        MaxInstances::Text(s.to_string())
    }

    #[test]
    fn instances_percent_and_int() {
        let i =
            |arg: Option<&str>, cfg: MaxInstances, cpu| compute_instances(arg, &cfg, cpu).unwrap();
        assert_eq!(i(None, pct("50%"), 8), 4.0);
        assert_eq!(i(None, pct("50%"), 5), 2.5);
        assert_eq!(i(None, MaxInstances::Count(3), 8), 3.0);
        assert_eq!(i(None, pct("4"), 8), 4.0);
        assert_eq!(i(Some("2"), pct("50%"), 8), 2.0);
        assert_eq!(i(Some(""), pct("50%"), 8), 4.0);
        assert_eq!(i(Some("100%"), pct("50%"), 8), 8.0);
        assert_eq!(i(Some("250%"), pct("50%"), 8), 8.0);
        assert_eq!(i(Some("0"), pct("50%"), 8), 1.0);
        assert_eq!(i(Some("0%"), pct("50%"), 8), 1.0);
        assert_eq!(i(Some("-5"), pct("50%"), 8), 1.0);
        // no Windows clamp anymore
        assert_eq!(i(Some("100%"), pct("50%"), 16), 16.0);
        assert_eq!(i(Some("4"), pct("50%"), 1), 1.0);
    }

    #[test]
    fn instances_errors() {
        assert!(compute_instances(Some("abc"), &pct("50%"), 8).is_err());
        assert!(compute_instances(Some("x%"), &pct("50%"), 8).is_err());
        assert!(compute_instances(Some("1.5"), &pct("50%"), 8).is_err());
        assert_eq!(compute_instances(Some(" 3 "), &pct("50%"), 8), Ok(3.0));
    }

    fn args(f: impl FnOnce(&mut Args)) -> Args {
        let mut a = Args::default();
        f(&mut a);
        a
    }

    #[test]
    fn format_names() {
        for (name, format, summary, tool, ext) in [
            ("dd", Format::Dd, "DD", "dee_ddp_encoder", "ac3"),
            ("ddp", Format::Ddp, "DDP", "dee_ddp_encoder", "ec3"),
            (
                "ddp-atmos",
                Format::DdpAtmos,
                "DDP Atmos",
                "dee_ddpjoc_encoder",
                "ec3",
            ),
            ("ac4", Format::Ac4, "AC4", "dee_ac4_encoder", "ac4"),
            (
                "ac4-atmos",
                Format::Ac4Atmos,
                "AC4 Atmos",
                "dee_ac4ajoc_encoder",
                "ac4",
            ),
            ("aac", Format::Aac, "AAC", "dee_aac_encoder", "aac"),
            ("thd", Format::Thd, "TrueHD", "dee_dthd_encoder", "thd"),
        ] {
            assert_eq!(Format::parse(name), Some(format));
            assert_eq!(format.summary_name(), summary);
            assert_eq!(format.dee_tool(), tool);
            assert_eq!(format.extension(6, false), ext);
        }
        // the old underscore spellings stay accepted, but are shown with hyphens
        for (old, format) in [
            ("ddp_atmos", Format::DdpAtmos),
            ("thd_atmos", Format::ThdAtmos),
            ("ac4_atmos", Format::Ac4Atmos),
        ] {
            assert_eq!(Format::parse(old), Some(format));
            assert_eq!(format.name(), old.replace('_', "-"));
        }
        assert_eq!(Format::parse("DDP"), None);
        assert_eq!(Format::parse("ddp_71"), None);
        assert_eq!(Format::Ddp.extension(8, true), "eb3");
        assert_eq!(Format::Ddp.extension(8, false), "ec3");
        assert_eq!(Format::Ddp.extension(2, true), "ec3");
        assert_eq!(Format::Dd.extension(8, true), "ac3");
    }

    #[test]
    fn validation_order_and_values() {
        let v = validate_args_d(&args(|a| a.format = "DDP_Atmos".into())).unwrap();
        assert_eq!(v.format, Format::DdpAtmos);
        let v = validate_args_d(&args(|a| {
            a.dialnorm = -50;
            a.track_index = -3;
            a.downmix = Some(0);
        }))
        .unwrap();
        assert_eq!((v.dialnorm, v.track_index, v.downmix), (-31, 0, None));
        let v = validate_args_d(&args(|a| a.dialnorm = 5)).unwrap();
        assert_eq!(v.dialnorm, 0);

        let err = |f: fn(&mut Args)| validate_args_d(&args(f)).unwrap_err().kind;
        assert_eq!(err(|a| a.format = "mp3".into()), ErrorKind::Format);
        assert_eq!(err(|a| a.downmix = Some(3)), ErrorKind::Downmix);
        assert_eq!(err(|a| a.downmix = Some(-1)), ErrorKind::Downmix);
        for format in ["ddp-atmos", "thd-atmos", "ac4-atmos"] {
            for dm in [1, 2, 6] {
                let e = validate_args_d(&args(|a| {
                    a.downmix = Some(dm);
                    a.format = format.into()
                }))
                .unwrap_err();
                assert_eq!(e.kind, ErrorKind::FormatDownmix, "{format} {dm}");
            }
        }
        // TrueHD takes 2 and 6, it has no mono
        for dm in [2, 6] {
            let v = validate_args_d(&args(|a| {
                a.downmix = Some(dm);
                a.format = "thd".into()
            }))
            .unwrap();
            assert_eq!(v.downmix, Some(dm));
        }
        let e = validate_args_d(&args(|a| {
            a.downmix = Some(1);
            a.format = "thd".into()
        }))
        .unwrap_err();
        assert_eq!(e.kind, ErrorKind::ThdDownmixMono);
        assert_eq!(err(|a| a.drc = Some("loud".into())), ErrorKind::Drc);
        assert!(validate_args_d(&args(|a| a.drc = Some("none".into()))).is_ok());
        assert!(validate_args_d(&args(|a| a.format = "thd".into())).is_ok());
        // format error comes before drc error
        assert_eq!(
            err(|a| {
                a.format = "x".into();
                a.drc = Some("y".into())
            }),
            ErrorKind::Format
        );
    }

    #[test]
    fn drc_defaults_per_format() {
        let drc = |format: &str, given: Option<&str>| {
            validate_args_d(&args(|a| {
                a.format = format.into();
                a.drc = given.map(String::from)
            }))
            .unwrap()
            .drc
        };
        for format in ["dd", "ddp", "ddp-atmos", "thd", "aac"] {
            assert_eq!(drc(format, None), "none", "{format}");
        }
        for format in ["ac4", "ac4-atmos"] {
            assert_eq!(drc(format, None), "music_light", "{format}");
        }
        assert_eq!(drc("thd", Some("speech")), "speech");
        assert_eq!(drc("ddp", Some("music_light")), "music_light");
        // an explicit none is still an error for AC4
        assert_eq!(
            validate_args_d(&args(|a| {
                a.format = "ac4".into();
                a.drc = Some("none".into())
            }))
            .unwrap_err()
            .kind,
            ErrorKind::Ac4Drc
        );
    }

    #[test]
    fn format_specific_restrictions() {
        let err = |f: fn(&mut Args)| validate_args_d(&args(f)).unwrap_err().kind;
        assert_eq!(
            err(|a| {
                a.format = "ddp-atmos".into();
                a.measure_only = true
            }),
            ErrorKind::MeasureAtmos
        );
        for format in ["ac4", "ac4-atmos"] {
            let e = validate_args_d(&args(|a| {
                a.format = format.into();
                a.drc = Some("none".into())
            }))
            .unwrap_err();
            assert_eq!(e.kind, ErrorKind::Ac4Drc);
        }
        // only measuring does not care about the DRC
        assert!(
            validate_args_d(&args(|a| {
                a.format = "ac4".into();
                a.drc = Some("none".into());
                a.measure_only = true
            }))
            .is_ok()
        );
        for format in ["aac", "ac4-atmos"] {
            let e = validate_args_d(&args(|a| {
                a.format = format.into();
                a.dialnorm = -27
            }))
            .unwrap_err();
            assert_eq!(e.kind, ErrorKind::DialnormFormat, "{format}");
        }
        for format in ["dd", "ddp", "ddp-atmos", "thd", "thd-atmos", "ac4"] {
            let v = validate_args_d(&args(|a| {
                a.format = format.into();
                a.dialnorm = -27
            }))
            .unwrap();
            assert_eq!(v.dialnorm, -27);
        }
    }

    #[test]
    fn measure_only_keeps_the_format() {
        let v = validate_args_d(&args(|a| {
            a.measure_only = true;
            a.format = "thd".into();
            a.dialnorm = -27
        }))
        .unwrap();
        assert_eq!(v.format, Format::Thd);
        assert_eq!(v.dialnorm, 0);
    }

    fn probe(rate: i64, ch: i64, depth: i64) -> Probe {
        Probe {
            sample_rate: rate,
            channels: ch,
            duration: 2.0,
            bit_depth: depth,
            is_truehd: false,
            channel_layout: String::new(),
            codec: "pcm_s24le".into(),
            raw_truehd: false,
            profile: String::new(),
            is_atmos: false,
        }
    }

    #[test]
    fn input_checks() {
        let info = check_inputs(&[probe(48000, 6, 24), probe(48000, 6, 24)]).unwrap();
        assert_eq!((info.samplerate, info.channels), (48000, 6));
        assert_eq!(info.bit_depths, [24, 24]);
        // different bit depths are fine, each normalised on its own
        let info = check_inputs(&[probe(48000, 6, 24), probe(48000, 6, 16)]).unwrap();
        assert_eq!(info.bit_depths, [24, 16]);
        let info = check_inputs(&[probe(48000, 6, 20), probe(48000, 6, 64)]).unwrap();
        assert_eq!(info.bit_depths, [24, 32]);

        let e = check_inputs(&[probe(48000, 6, 24), probe(44100, 6, 24)]).unwrap_err();
        assert_eq!(e, Fail::with(ErrorKind::SampleMismatch, "[48000, 44100]"));
        let e = check_inputs(&[probe(48000, 6, 24), probe(48000, 2, 24)]).unwrap_err();
        assert_eq!(e, Fail::with(ErrorKind::ChannelMismatch, "[6, 2]"));
        // the order of the checks
        let e = check_inputs(&[probe(48000, 6, 24), probe(44100, 2, 16)]).unwrap_err();
        assert_eq!(e.kind, ErrorKind::SampleMismatch);
    }

    #[test]
    fn atmos_inputs_have_to_be_truehd() {
        let mut thd = probe(48000, 8, 24);
        thd.is_truehd = true;
        thd.is_atmos = true;
        let files = ["a.thd".to_string(), "b [1].wav".to_string()];
        assert_eq!(
            check_atmos_inputs(&files[..1], std::slice::from_ref(&thd)),
            Ok(())
        );
        let e = check_atmos_inputs(&files, &[thd.clone(), probe(48000, 8, 24)]).unwrap_err();
        assert_eq!(e, Fail::with(ErrorKind::AtmosInput, "b \\[1].wav"));
        // DDP Atmos is not TrueHD either
        let mut eac3 = probe(48000, 6, 24);
        eac3.is_atmos = true;
        let e = check_atmos_inputs(&files[..1], &[eac3]).unwrap_err();
        assert_eq!(e.kind, ErrorKind::AtmosInput);
        // TrueHD without an Atmos presentation
        let mut plain = thd.clone();
        plain.is_atmos = false;
        let e = check_atmos_inputs(&files, &[thd.clone(), plain]).unwrap_err();
        assert_eq!(e, Fail::with(ErrorKind::AtmosNoAtmos, "b \\[1].wav"));
    }

    #[test]
    fn measuring_needs_at_most_5_1() {
        assert!(check_ac4_dialnorm_channels(8, false).is_ok());
        assert!(check_ac4_dialnorm_channels(6, true).is_ok());
        assert_eq!(
            check_ac4_dialnorm_channels(8, true).unwrap_err().kind,
            ErrorKind::Ac4DialnormChannels
        );
    }

    #[test]
    fn bit_depth_normalisation() {
        for (d, want) in [
            (8, 16),
            (15, 16),
            (16, 16),
            (17, 24),
            (20, 24),
            (23, 24),
            (24, 24),
            (25, 32),
            (32, 32),
            (64, 32),
        ] {
            assert_eq!(normalize_bit_depth(d), want, "{d}");
        }
    }

    #[test]
    fn layouts() {
        let l = |f, d, c| resolve_layout(f, d, c, false).map(|l| (l.outchannels, l.layout_arg));
        assert_eq!(l(Format::Ddp, None, 6), Ok((6, None)));
        assert_eq!(l(Format::Ddp, None, 8), Ok((8, None)));
        assert_eq!(l(Format::Ddp, Some(2), 8), Ok((2, Some("stereo"))));
        assert_eq!(l(Format::Ddp, Some(6), 8), Ok((6, Some("5.1"))));
        assert_eq!(l(Format::Ddp, Some(1), 2), Ok((1, Some("mono"))));
        assert_eq!(l(Format::Dd, None, 8), Ok((6, Some("5.1"))));
        assert_eq!(l(Format::Dd, Some(1), 8), Ok((1, Some("mono"))));
        assert_eq!(l(Format::Dd, None, 6), Ok((6, None)));
        assert_eq!(l(Format::Ac4, None, 6), Ok((6, None)));
        assert_eq!(l(Format::Ac4, None, 2), Ok((2, None)));
        assert_eq!(l(Format::Ac4, None, 8), Ok((6, Some("5.1"))));
        assert_eq!(l(Format::Ac4, Some(2), 6), Ok((2, Some("stereo"))));
        assert_eq!(l(Format::Ac4, Some(6), 8), Ok((6, Some("5.1"))));
        // AAC downmixes by itself
        assert_eq!(l(Format::Aac, None, 6), Ok((2, None)));
        assert_eq!(l(Format::Aac, None, 8), Ok((2, None)));
        assert_eq!(l(Format::Aac, Some(2), 6), Ok((2, None)));
        assert_eq!(l(Format::Thd, None, 8), Ok((8, None)));
        // the Atmos input is not looked at
        assert_eq!(l(Format::DdpAtmos, None, 3), Ok((0, None)));
        assert_eq!(l(Format::Ac4Atmos, None, 8), Ok((0, None)));

        let kind = |f, d, c| resolve_layout(f, d, c, false).unwrap_err().kind;
        assert_eq!(kind(Format::Ddp, None, 3), ErrorKind::Channels);
        assert_eq!(kind(Format::Ddp, Some(6), 6), ErrorKind::DownmixMismatch);
        assert_eq!(kind(Format::Ddp, Some(2), 2), ErrorKind::DownmixMismatch);
        assert_eq!(kind(Format::Ac4, None, 1), ErrorKind::Ac4InputChannels);
        assert_eq!(kind(Format::Ac4, Some(1), 6), ErrorKind::DownmixValue);
        assert_eq!(kind(Format::Aac, Some(6), 8), ErrorKind::DownmixValue);
        assert_eq!(kind(Format::Aac, Some(1), 6), ErrorKind::DownmixValue);
        assert_eq!(kind(Format::Aac, None, 1), ErrorKind::AacInput);
        assert_eq!(kind(Format::Aac, Some(1), 1), ErrorKind::AacInput);
        assert_eq!(kind(Format::Thd, None, 1), ErrorKind::ThdMonoInput);
        assert!(resolve_layout(Format::Thd, None, 2, false).is_ok());
        // TrueHD downmixes in ffmpeg: no `-c` for DEE
        assert_eq!(l(Format::Thd, Some(6), 8), Ok((6, None)));
        assert_eq!(l(Format::Thd, Some(2), 8), Ok((2, None)));
        assert_eq!(l(Format::Thd, Some(2), 6), Ok((2, None)));
        assert_eq!(l(Format::Thd, Some(6), 7), Ok((6, None)));
        assert_eq!(l(Format::Thd, Some(2), 5), Ok((2, None)));
        assert_eq!(kind(Format::Thd, Some(6), 6), ErrorKind::DownmixMismatch);
        assert_eq!(kind(Format::Thd, Some(6), 5), ErrorKind::DownmixMismatch);
        assert_eq!(kind(Format::Thd, Some(2), 2), ErrorKind::DownmixMismatch);
        assert_eq!(kind(Format::Thd, Some(1), 8), ErrorKind::DownmixValue);
        // the channel check comes before the downmix check
        assert_eq!(kind(Format::Ddp, Some(6), 3), ErrorKind::Channels);
    }

    fn lfe_layout(format: Format, dm: Option<i64>, channels: i64, dl: bool) -> Layout {
        resolve_layout(format, dm, channels, dl).unwrap()
    }

    #[test]
    fn thd_downmix_lfe() {
        // -dm 2 has no LFE, -dl is ignored as for the other formats
        let l = lfe_layout(Format::Thd, Some(2), 8, true);
        assert_eq!((l.outchannels, l.lfe), (2, Lfe::Normal));
        assert_eq!(l.output_name(), "stereo");
        assert_eq!(l.notes.len(), 1);
        assert!(l.notes[0].contains("the output has no LFE channel"));
        // -dm 6: the LFE stays (silent with -dl, TrueHD can't leave it out)
        let l = lfe_layout(Format::Thd, Some(6), 8, false);
        assert_eq!((l.outchannels, l.lfe), (6, Lfe::Normal));
        assert_eq!(l.output_name(), "5.1");
        let l = lfe_layout(Format::Thd, Some(6), 8, true);
        assert_eq!((l.outchannels, l.lfe), (6, Lfe::Silent));
        assert_eq!(l.output_name(), "5.1 (silent LFE)");
        assert!(l.notes[0].contains("TrueHD can't leave out the LFE channel"));
        // a 7.0 input has none either
        let l = lfe_layout(Format::Thd, Some(6), 7, false);
        assert_eq!((l.outchannels, l.lfe), (6, Lfe::Silent));
        let l = lfe_layout(Format::Thd, Some(2), 7, false);
        assert_eq!((l.outchannels, l.lfe), (2, Lfe::Normal));
        assert!(l.notes.is_empty());
    }

    #[test]
    fn five_and_seven_channel_inputs() {
        // 5.0 and 7.0 are padded to the slots of 5.1 and 7.1
        assert_eq!(wav_channels(5), 6);
        assert_eq!(wav_channels(7), 8);
        assert_eq!(wav_channels(6), 6);
        let l = lfe_layout(Format::Ddp, None, 5, false);
        assert_eq!(
            (l.outchannels, l.layout_arg, l.lfe),
            (6, None, Lfe::Removed)
        );
        assert_eq!(l.output_name(), "5.0");
        let l = lfe_layout(Format::Ddp, None, 7, false);
        assert_eq!(
            (l.outchannels, l.layout_arg, l.lfe),
            (8, None, Lfe::Removed)
        );
        assert_eq!(l.output_name(), "7.0");
        // Dd and AC4 take 7.0 down to 5.0 / 5.1
        let l = lfe_layout(Format::Dd, None, 7, false);
        assert_eq!(
            (l.outchannels, l.layout_arg, l.lfe),
            (6, Some("5.1"), Lfe::Removed)
        );
        let l = lfe_layout(Format::Ac4, None, 7, false);
        assert_eq!((l.outchannels, l.lfe), (6, Lfe::Silent));
        // downmix from 5.0 / 7.0 still works
        let l = lfe_layout(Format::Ddp, Some(6), 7, false);
        assert_eq!(
            (l.outchannels, l.layout_arg, l.lfe),
            (6, Some("5.1"), Lfe::Removed)
        );
        assert_eq!(l.output_name(), "5.0");
        let l = lfe_layout(Format::Ddp, Some(2), 7, false);
        assert_eq!(
            (l.outchannels, l.layout_arg, l.lfe),
            (2, Some("stereo"), Lfe::Normal)
        );
        assert!(resolve_layout(Format::Ddp, Some(6), 5, false).is_err());
        // TrueHD keeps a silent LFE
        let l = lfe_layout(Format::Thd, None, 5, false);
        assert_eq!((l.outchannels, l.lfe), (6, Lfe::Silent));
        assert!(l.notes[0].contains("TrueHD can't leave out the LFE channel, it is kept silent."));
        assert_eq!(l.output_name(), "5.1 (silent LFE)");
        // AAC is stereo anyway
        let l = lfe_layout(Format::Aac, None, 7, false);
        assert_eq!((l.outchannels, l.lfe), (2, Lfe::Normal));
        // other counts are still rejected
        for c in [3, 4, 9] {
            assert_eq!(
                resolve_layout(Format::Ddp, None, c, false)
                    .unwrap_err()
                    .kind,
                ErrorKind::Channels
            );
        }
    }

    #[test]
    fn disable_lfe_per_format() {
        let dl = |f, dm, c| lfe_layout(f, dm, c, true);
        // DD / DDP: real 5.0 and 7.0
        assert_eq!(dl(Format::Dd, None, 6).lfe, Lfe::Removed);
        assert_eq!(dl(Format::Ddp, None, 6).lfe, Lfe::Removed);
        assert_eq!(dl(Format::Ddp, None, 8).lfe, Lfe::Removed);
        assert_eq!(dl(Format::Ddp, None, 8).output_name(), "7.0");
        // 7.1 -dl -dm 6: a 5.0 output
        let l = dl(Format::Ddp, Some(6), 8);
        assert_eq!((l.outchannels, l.lfe), (6, Lfe::Removed));
        assert_eq!(l.output_name(), "5.0");
        // the others can't, one note
        for (format, name) in [(Format::Thd, "TrueHD"), (Format::Ac4, "AC4")] {
            let l = dl(format, None, 6);
            assert_eq!(l.lfe, Lfe::Silent, "{name}");
            assert_eq!(
                l.notes,
                [format!(
                    "{name} can't leave out the LFE channel, it is kept silent."
                )]
            );
        }
        // nothing to remove: ignored with a note
        for (format, dm, c, text) in [
            (Format::Ddp, Some(2), 6, "the output has no LFE channel"),
            (Format::Ddp, Some(1), 8, "the output has no LFE channel"),
            (Format::Aac, None, 6, "the output has no LFE channel"),
            (Format::Ddp, None, 2, "only applies to 5.1 and 7.1 inputs"),
            (Format::Ddp, None, 1, "only applies to 5.1 and 7.1 inputs"),
            (Format::Ddp, None, 5, "5.0 or 7.0 input has no LFE"),
            (Format::Ddp, None, 7, "5.0 or 7.0 input has no LFE"),
            (Format::DdpAtmos, None, 8, "ignored for the Atmos formats"),
            (Format::Ac4Atmos, None, 8, "ignored for the Atmos formats"),
        ] {
            let l = dl(format, dm, c);
            assert!(
                l.notes.iter().any(|n| n.contains(text)),
                "{format:?} {c}: {:?}",
                l.notes
            );
            assert!(l.outchannels < 6 || c == 5 || c == 7 || l.lfe == Lfe::Normal);
        }
        // without -dl a 5.1 / 7.1 input keeps its LFE
        let l = lfe_layout(Format::Ddp, None, 8, false);
        assert_eq!((l.lfe, l.notes.len()), (Lfe::Normal, 0));
    }

    #[test]
    fn five_and_seven_channel_layouts_are_checked() {
        let probe = |channels, layout: &str| Probe {
            channels,
            channel_layout: layout.to_string(),
            ..probe(48000, channels, 24)
        };
        let check = |channels, layout: &str| {
            check_input_layouts(&["a.mka".to_string()], &[probe(channels, layout)])
        };
        for (c, l) in [
            (5, "5.0"),
            (5, "5.0(side)"),
            (5, ""),
            (7, "7.0"),
            (7, ""),
            (6, "5.1"),
            (8, "7.1"),
        ] {
            assert_eq!(check(c, l), Ok(()), "{c} {l}");
        }
        for (c, l) in [(5, "4.1"), (5, "5.0(wide)"), (7, "7.0(front)"), (7, "6.1")] {
            let e = check(c, l).unwrap_err();
            assert_eq!(e.kind, ErrorKind::InputLayout, "{c} {l}");
            assert!(e.insert.unwrap().starts_with(l), "{l}");
        }
        assert_eq!(
            check_ac4_dialnorm_channels(7, true).unwrap_err().kind,
            ErrorKind::Ac4DialnormChannels
        );
        assert!(check_ac4_dialnorm_channels(5, true).is_ok());
    }

    #[test]
    fn notice_list() {
        let n = |f, ch, mo, dn| notices(f, ch, mo, dn);
        assert!(n(Format::Ddp, 6, false, 0).is_empty());
        assert_eq!(n(Format::Dd, 2, false, 0).len(), 1);
        assert!(n(Format::Dd, 2, true, 0).is_empty());
        assert_eq!(n(Format::Thd, 2, false, 0).len(), 1);
        assert!(n(Format::Thd, 6, false, 0).is_empty());
        assert_eq!(
            n(Format::Thd, 2, false, 0)[0].note,
            "Consider using [bold cyan]FLAC[/bold cyan] for lossless [bold yellow]stereo[/bold yellow] encoding."
        );
        assert!(
            n(Format::Thd, 2, false, 0)[0].question.starts_with(
                "Consider using [bold cyan]FLAC[/bold cyan] for lossless [bold yellow]stereo[/bold yellow] encoding, are"
            )
        );
        assert!(n(Format::Ac4, 2, false, 0).is_empty());
        let all = n(Format::Ddp, 1, false, -27);
        assert_eq!(all.len(), 2);
        assert!(all[1].note.starts_with("Consider leaving the dialnorm"));
        assert_eq!(n(Format::Thd, 2, false, -3).len(), 2);
        assert_eq!(
            all[0].note,
            "Consider using [bold cyan]qaac[/bold cyan] or [bold cyan]opus[/bold cyan] for [bold yellow]mono[/bold yellow] and [bold yellow]stereo[/bold yellow] encoding."
        );
        // AAC suggests qaac (not when only measuring)
        let aac = n(Format::Aac, 2, false, 0);
        assert_eq!(aac.len(), 1);
        assert_eq!(
            aac[0].note,
            "Consider using [bold cyan]qaac[/bold cyan] for [bold yellow]AAC[/bold yellow] encoding."
        );
        assert!(
            aac[0]
                .question
                .ends_with("are you sure you want to use [bold cyan]DEE[/bold cyan]?")
        );
        assert!(n(Format::Aac, 2, true, 0).is_empty());
        // the Atmos formats have no notices
        assert!(n(Format::DdpAtmos, 0, false, 0).is_empty());
    }

    #[test]
    fn dee_shim_requirements() {
        assert_eq!(
            needs_shim(Format::DdpAtmos, "music_light", false),
            Some(Fail::with(ErrorKind::ShimAtmos, "ddp-atmos"))
        );
        assert_eq!(
            needs_shim(Format::ThdAtmos, "none", false),
            Some(Fail::with(ErrorKind::ShimAtmos, "thd-atmos"))
        );
        assert_eq!(
            needs_shim(Format::Thd, "none", false),
            Some(Fail::new(ErrorKind::ShimThd))
        );
        assert_eq!(needs_shim(Format::Thd, "speech", false), None);
        assert_eq!(needs_shim(Format::Ddp, "none", false), None);
        assert_eq!(needs_shim(Format::Ac4Atmos, "music_light", false), None);
        assert_eq!(needs_shim(Format::Thd, "none", true), None);
    }

    #[test]
    fn encoder_delay_per_format() {
        for (f, samples) in [
            (Format::Dd, 256),
            (Format::Ddp, 256),
            (Format::DdpAtmos, 256),
            (Format::Thd, 0),
            (Format::ThdAtmos, 0),
            (Format::Ac4, 2048),
            (Format::Ac4Atmos, 2048),
            (Format::Aac, 1600),
        ] {
            assert_eq!(f.encoder_delay(), samples, "{f:?}");
        }
    }

    fn std_modes() -> Modes {
        Modes {
            ddp: DdpMode::Standard,
            atmos: DdpAtmosMode::Streaming,
        }
    }

    fn br(f: Format, ch: i64, req: Option<i64>) -> Option<u32> {
        select_bitrate(f, ch, std_modes(), req, &defaults())
    }

    #[test]
    fn bitrate_defaults() {
        for (format, channels, want) in [
            (Format::Dd, 1, 128),
            (Format::Dd, 2, 256),
            (Format::Dd, 6, 640),
            (Format::Ddp, 1, 128),
            (Format::Ddp, 2, 256),
            (Format::Ddp, 6, 1024),
            (Format::Ddp, 8, 1536),
            (Format::DdpAtmos, 0, 1024),
            (Format::Ac4, 2, 256),
            (Format::Ac4, 6, 448),
            (Format::Ac4Atmos, 0, 448),
            (Format::Aac, 2, 256),
        ] {
            assert_eq!(br(format, channels, None), Some(want), "{format:?}");
            // 0 counts as not given
            assert_eq!(br(format, channels, Some(0)), Some(want), "{format:?}");
        }
        assert_eq!(br(Format::Thd, 6, None), None);
        assert_eq!(br(Format::Thd, 6, Some(640)), None);
    }

    #[test]
    fn ddp_shim_check() {
        let lay = |lfe| Layout {
            outchannels: 8,
            layout_arg: None,
            lfe,
            notes: Vec::new(),
        };
        let m = |fb: bool| {
            if fb {
                DdpMode::Bluray
            } else {
                DdpMode::Standard
            }
        };
        let check = |f, l: &Layout, fb, mo, shim| check_shim_for_ddp(f, l, m(fb), mo, shim);
        let sil = lay(Lfe::Silent);
        let rem = lay(Lfe::Removed);
        assert_eq!(
            check(Format::Ddp, &sil, false, false, false),
            Err(Fail::new(ErrorKind::ShimDdp71))
        );
        assert_eq!(check(Format::Ddp, &sil, true, false, false), Ok(()));
        for fb in [false, true] {
            assert_eq!(
                check(Format::Ddp, &rem, fb, false, false),
                Err(Fail::new(ErrorKind::ShimDdp70))
            );
            assert_eq!(check(Format::Ddp, &rem, fb, false, true), Ok(()));
            assert_eq!(check(Format::Ddp, &rem, fb, true, false), Ok(()));
        }
        assert_eq!(check(Format::Ddp, &sil, false, false, true), Ok(()));
        assert_eq!(check(Format::Ddp, &sil, false, true, false), Ok(()));
        assert_eq!(check(Format::Dd, &sil, false, false, false), Ok(()));
        let six = Layout {
            outchannels: 6,
            layout_arg: None,
            lfe: Lfe::Silent,
            notes: Vec::new(),
        };
        assert_eq!(check(Format::Ddp, &six, false, false, false), Ok(()));
    }

    #[test]
    fn seven_one_ddp_profiles() {
        // standard, Blu-ray with -fb
        assert_eq!(
            profile_71(Format::Ddp, 8, DdpMode::Standard),
            Some(Profile71::Standard)
        );
        assert_eq!(
            profile_71(Format::Ddp, 8, DdpMode::Bluray),
            Some(Profile71::Bluray)
        );
        assert_eq!(profile_71(Format::Ddp, 6, DdpMode::Bluray), None);
        assert_eq!(profile_71(Format::Dd, 8, DdpMode::Standard), None);
        let rate = |bluray, req| {
            let ddp = if bluray {
                DdpMode::Bluray
            } else {
                DdpMode::Standard
            };
            select_bitrate(
                Format::Ddp,
                8,
                Modes { ddp, ..std_modes() },
                req,
                &defaults(),
            )
        };
        assert_eq!(rate(false, None), Some(1536));
        assert_eq!(rate(true, None), Some(1536));
        assert_eq!(rate(false, Some(1152)), Some(1152));
        assert_eq!(rate(true, Some(1152)), Some(1024));
        assert_eq!(rate(true, Some(100)), Some(768));
        assert_eq!(rate(true, Some(5000)), Some(1664));
        assert_eq!(rate(false, Some(5000)), Some(1792));
        assert_eq!(Format::Ddp.extension(8, true), "eb3");
        assert_eq!(Format::Ddp.extension(8, false), "ec3");
        // the default is valid for both profiles
        for name in ["ddp_71", "ddp_71_bluray"] {
            assert!(allowed_bitrates(name).contains(&(defaults().ddp_7_1_standard as u32)));
        }
    }

    #[test]
    fn atmos_modes_have_their_own_rates_and_extensions() {
        let rate = |atmos, req| {
            let modes = Modes {
                atmos,
                ..std_modes()
            };
            select_bitrate(Format::DdpAtmos, 0, modes, req, &defaults())
        };
        assert_eq!(rate(DdpAtmosMode::Streaming, None), Some(1024));
        assert_eq!(rate(DdpAtmosMode::Streaming, Some(5000)), Some(1024));
        assert_eq!(rate(DdpAtmosMode::Standard, None), Some(1536));
        assert_eq!(rate(DdpAtmosMode::Standard, Some(5000)), Some(2048));
        assert_eq!(rate(DdpAtmosMode::Standard, Some(100)), Some(768));
        assert_eq!(rate(DdpAtmosMode::Bluray, Some(100)), Some(1152));
        assert_eq!(rate(DdpAtmosMode::Bluray, Some(2000)), Some(1664));
        assert_eq!(Format::DdpAtmos.extension(0, true), "eb3");
        assert_eq!(Format::DdpAtmos.extension(0, false), "ec3");
    }

    #[test]
    fn mode_values_are_validated() {
        let mut a = Args {
            ddp_mode: Some("streaming".into()),
            ..Args::default()
        };
        assert_eq!(
            validate_args(&a, &DefaultModes::default())
                .unwrap_err()
                .kind,
            ErrorKind::DdpMode
        );
        a.ddp_mode = Some("bluray".into());
        a.ddp_atmos_mode = Some("x".into());
        assert_eq!(
            validate_args(&a, &DefaultModes::default())
                .unwrap_err()
                .kind,
            ErrorKind::DdpAtmosMode
        );
        a.ddp_atmos_mode = Some("standard".into());
        let v = validate_args(&a, &DefaultModes::default()).unwrap();
        assert_eq!(
            (v.ddp_mode, v.atmos_mode),
            (DdpMode::Bluray, DdpAtmosMode::Standard)
        );
        let v = validate_args(&Args::default(), &DefaultModes::default()).unwrap();
        assert_eq!(
            (v.ddp_mode, v.atmos_mode),
            (DdpMode::Standard, DdpAtmosMode::Streaming)
        );
    }

    #[test]
    fn bitrates_are_rounded_to_the_closest_allowed() {
        for (format, channels, requested, want) in [
            (Format::Dd, 6, -5, 224),
            (Format::Dd, 2, 100, 96),
            (Format::Dd, 6, 5000, 640),
            (Format::Ddp, 1, 1, 32),
            (Format::Ddp, 6, 1100, 1024),
            (Format::Ddp, 8, 100, 384),
            (Format::Ddp, 8, 1200, 1152),
            (Format::Ddp, 8, 1700, 1664),
            (Format::Ddp, 8, 5000, 1792),
            (Format::DdpAtmos, 0, 100, 384),
            (Format::DdpAtmos, 0, 900, 1024),
            (Format::DdpAtmos, 0, 800, 768),
            (Format::DdpAtmos, 0, 5000, 1024),
            (Format::Ac4, 2, 1, 48),
            (Format::Ac4, 2, 5000, 768),
            (Format::Ac4, 6, 100, 96),
            (Format::Ac4Atmos, 0, 1, 256),
            (Format::Ac4Atmos, 0, 380, 320),
            (Format::Aac, 2, 1, 56),
            (Format::Aac, 2, 5000, 576),
            (Format::Aac, 2, 300, 288),
        ] {
            assert_eq!(
                br(format, channels, Some(requested)),
                Some(want),
                "{format:?} {channels} {requested}"
            );
        }
    }

    #[test]
    fn delay_from_name() {
        assert_eq!(input_delay("a/b.mkv", None), "+0ms");
        assert_eq!(input_delay("a/b DELAY -24ms.wav", None), "-24ms");
        assert_eq!(input_delay("a/b DELAY +5ms.wav", None), "+5ms");
        assert_eq!(input_delay("a/b DELAY 5s.wav", None), "+5s");
        assert_eq!(input_delay("a/b DELAY 5s.wav", Some("-d")), "-d");
        assert_eq!(input_delay("a/b DELAY 5s.wav", Some("")), "+5s");
        assert_eq!(input_delay("a/b DELAY 5s", None), "+0ms");
        assert_eq!(input_delay("DELAY 5s.wav", None), "+0ms");
        assert_eq!(input_delay("x DELAY 1ms.a DELAY -2ms.wav", None), "-2ms");
        assert_eq!(input_delay("a/b.mkv", Some("+1s")), "+1s");
    }
}
