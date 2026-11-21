//! Pure decisions of the encoding pipeline: argument validation, instance
//! count, channel layout, prompts and bitrate selection.

use crate::cli::Args;
use crate::config::{
    Ac4ImsMode, DdpAtmosMode, DdpMode, DefaultBitrates, DefaultModes, MaxInstances, WarpMode,
};
use crate::data::{ErrorKind, allowed_bitrates};
use crate::error::{escape, print_exit};
use crate::probe::{Probe, TrackIndex};
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
    Ac4Ims,
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
            "ac4-ims" | "ac4_ims" => Format::Ac4Ims,
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
            Format::Ac4Ims => "AC4 IMS",
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
            Format::Ac4Ims => "ac4-ims",
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
            Format::Ac4Ims => "dee_ac4ims_encoder",
            Format::Aac => "dee_aac_encoder",
            Format::Thd | Format::ThdAtmos => "dee_dthd_encoder",
        }
    }

    /// Formats that are encoded from a TrueHD Atmos input decoded by truehdd.
    pub fn is_atmos(self) -> bool {
        matches!(self, Format::DdpAtmos | Format::ThdAtmos | Format::Ac4Atmos)
    }

    /// Formats whose encoder reads an Atmos master (ADM BWF, DAMF) directly:
    /// the Atmos formats and AC4 IMS, and DD and DDP, which DEE renders to
    /// 5.1 (or 2.0, 1.0) from it. TrueHD (DEE's `presentation` takes no
    /// master), AC4 and AAC don't.
    pub fn takes_master(self) -> bool {
        matches!(
            self,
            Format::Dd
                | Format::Ddp
                | Format::DdpAtmos
                | Format::ThdAtmos
                | Format::Ac4Atmos
                | Format::Ac4Ims
        )
    }

    /// Formats with a 90 degree phase shift of the surround channels: DD,
    /// DDP and DDP Atmos (via dee_shim's `--phase90`).
    pub fn has_phase_shift(self) -> bool {
        matches!(self, Format::Dd | Format::Ddp | Format::DdpAtmos)
    }

    /// Formats with DEE's LFE low-pass filter and the surround 3 dB
    /// attenuation (`-e` options of dee_ddp_encoder): DD and DDP, not DDP
    /// Atmos (its encoder has neither).
    pub fn has_dd_filters(self) -> bool {
        matches!(self, Format::Dd | Format::Ddp)
    }

    /// Formats with the surround 3 dB attenuation (`-s3a`): DD, DDP and
    /// TrueHD (a presentation option of the 6ch and 8ch outputs), not the
    /// Atmos downmixes.
    pub fn has_surround_3db(self) -> bool {
        matches!(self, Format::Dd | Format::Ddp | Format::Thd)
    }

    /// Formats whose Atmos source goes through truehdd (`-wm`, `-bc`): the
    /// Atmos formats and AC4 IMS (from a TrueHD Atmos source).
    pub fn has_truehdd(self) -> bool {
        self.is_atmos() || self == Format::Ac4Ims
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
    /// the start (measured with DEE 6.7: DD/DDP decode 256 samples late; the
    /// AC-4 encoder's manifest gives an offset of -2048; DEE's AAC encoder
    /// has a codec delay, in its debug log, that depends on the bitrate:
    /// 1710 up to 71 kbps, 1702 for 72..=87, 1600 from 88). When DEE itself
    /// downmixes (`dee_downmix`) the output is 32 samples later for DD, DDP
    /// and AAC (AC-4 is unverified, TrueHD and the Atmos formats have none).
    pub fn encoder_delay(self, bitrate: u32, dee_downmix: bool) -> u32 {
        let base = match self {
            Format::Dd | Format::Ddp | Format::DdpAtmos => 256,
            Format::Ac4 | Format::Ac4Atmos | Format::Ac4Ims => 2048,
            Format::Aac => match bitrate {
                ..=71 => 1710,
                72..=87 => 1702,
                _ => 1600,
            },
            Format::Thd | Format::ThdAtmos => 0,
        };
        let extra = dee_downmix && matches!(self, Format::Dd | Format::Ddp | Format::Aac);
        base + if extra { 32 } else { 0 }
    }

    /// Extension of the output file.
    pub fn extension(self, outchannels: i64, bluray: bool) -> &'static str {
        match self {
            Format::Dd => "ac3",
            Format::Ddp if outchannels == 8 && bluray => "eb3",
            Format::DdpAtmos if bluray => "eb3",
            Format::Ddp | Format::DdpAtmos => "ec3",
            Format::Ac4 | Format::Ac4Atmos | Format::Ac4Ims => "ac4",
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
    pub track_index: TrackIndex,
    /// The DRC profile: `-r`, else the default of the format.
    pub drc: String,
    /// `--ddp-mode`, else the config's default (7.1 DDP only).
    pub ddp_mode: DdpMode,
    /// `--ddp-atmos-mode`, else the config's default (DDP Atmos only).
    pub atmos_mode: DdpAtmosMode,
    /// `--ac4-ims-mode`, else general (AC4 IMS only).
    pub ims_mode: Ac4ImsMode,
    /// `-sps` (DD, DDP and DDP Atmos only); `None` leaves it to the
    /// config's `surround_phase_shift`.
    pub surround_phase_shift: Option<bool>,
    /// `-lpf` (DD and DDP only); `None` leaves it to the config's `low_pass_filter`.
    pub low_pass_filter: Option<bool>,
    /// `-s3a` (DD and DDP only); `None` leaves it to the config's `surround_3db_attenuation`.
    pub surround_3db_attenuation: Option<bool>,
    /// `-wm`; `None` leaves it to the config's `truehdd_warp_mode`.
    pub warp_mode: Option<WarpMode>,
    /// `-bc`; `None` leaves it to the config's `truehdd_bed_conform`.
    pub bed_conform: Option<bool>,
}

pub const DRC_PROFILES: [&str; 6] = [
    "none",
    "film_light",
    "film_standard",
    "music_light",
    "music_standard",
    "speech",
];

/// The DRC profile when `-r` is not given: `none`, except for plain AC4,
/// which has no such preset (AC4 Atmos has it).
pub fn default_drc(format: Format) -> &'static str {
    match format {
        Format::Ac4 => "music_light",
        _ => "none",
    }
}

/// Validate the arguments, the first problem wins.
pub fn validate_args(args: &Args, modes: &DefaultModes) -> Result<Validated, Fail> {
    let dialnorm = clamp_i(args.dialnorm, -31, 0);
    let track_index = args.track_index;
    let downmix = args.downmix.filter(|&d| d != 0);

    // `-mo` measures the source: the format and the encoding options are ignored
    if args.measure_only {
        // a TrueHD Atmos source is decoded by truehdd for the measurement
        let (warp_mode, bed_conform) = truehdd_options(args)?;
        return Ok(Validated {
            format: Format::Ddp,
            downmix: None,
            dialnorm: 0,
            track_index,
            drc: default_drc(Format::Ddp).to_string(),
            ddp_mode: modes.ddp,
            atmos_mode: modes.ddp_atmos,
            ims_mode: Ac4ImsMode::General,
            surround_phase_shift: None,
            low_pass_filter: None,
            surround_3db_attenuation: None,
            warp_mode,
            bed_conform,
        });
    }

    let format = Format::parse(&args.format.to_lowercase()).ok_or(Fail::new(ErrorKind::Format))?;
    if let Some(d) = downmix {
        if ![1, 2, 6].contains(&d) {
            return Err(Fail::new(ErrorKind::Downmix));
        }
        if format == Format::Ac4Ims {
            return Err(Fail::new(ErrorKind::Ac4ImsDownmix));
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
    let ims_mode = match args.ac4_ims_mode.as_deref() {
        Some(m) => Ac4ImsMode::parse(m).ok_or(Fail::new(ErrorKind::Ac4ImsMode))?,
        None => modes.ac4_ims,
    };
    // DEE's music mode only takes music_light
    let music = format == Format::Ac4Ims && ims_mode == Ac4ImsMode::Music;
    if music && args.drc.as_deref().is_some_and(|d| d != "music_light") {
        return Err(Fail::new(ErrorKind::Ac4ImsMusicDrc));
    }
    if format == Format::Ac4Ims && args.disable_lfe {
        return Err(Fail::new(ErrorKind::Ac4ImsLfe));
    }
    let drc = if music {
        "music_light".to_string()
    } else {
        args.drc
            .clone()
            .unwrap_or_else(|| default_drc(format).to_string())
    };
    let ddp_mode = match args.ddp_mode.as_deref() {
        Some(m) => DdpMode::parse(m).ok_or(Fail::new(ErrorKind::DdpMode))?,
        None => modes.ddp,
    };
    let atmos_mode = match args.ddp_atmos_mode.as_deref() {
        Some(m) => DdpAtmosMode::parse(m).ok_or(Fail::new(ErrorKind::DdpAtmosMode))?,
        None => modes.ddp_atmos,
    };
    if format == Format::Ac4 && args.drc.as_deref() == Some("none") {
        return Err(Fail::new(ErrorKind::Ac4Drc));
    }
    if dialnorm != 0 && !format.has_dialnorm() {
        return Err(Fail::new(ErrorKind::DialnormFormat));
    }
    let on_off = |v: &Option<String>| match v.as_deref().map(str::to_lowercase).as_deref() {
        None => Ok(None),
        Some("on") => Ok(Some(true)),
        Some("off") => Ok(Some(false)),
        Some(_) => Err(Fail::new(ErrorKind::OnOff)),
    };
    let surround_phase_shift = on_off(&args.surround_phase_shift)?;
    let low_pass_filter = on_off(&args.low_pass_filter)?;
    let surround_3db_attenuation = on_off(&args.surround_3db_attenuation)?;
    if surround_phase_shift.is_some() && !format.has_phase_shift() {
        return Err(Fail::new(ErrorKind::PhaseShiftFormat));
    }
    if low_pass_filter.is_some() && !format.has_dd_filters() {
        return Err(Fail::new(ErrorKind::LowPassFilterFormat));
    }
    if surround_3db_attenuation.is_some() && !format.has_surround_3db() {
        return Err(Fail::new(ErrorKind::Surround3dbFormat));
    }
    let (warp_mode, bed_conform) = truehdd_options(args)?;
    if warp_mode.is_some() && !format.has_truehdd() {
        return Err(Fail::new(ErrorKind::WarpModeFormat));
    }
    if bed_conform.is_some() && !format.has_truehdd() {
        return Err(Fail::new(ErrorKind::BedConformFormat));
    }
    Ok(Validated {
        format,
        downmix,
        dialnorm,
        track_index,
        drc,
        ddp_mode,
        atmos_mode,
        ims_mode,
        surround_phase_shift,
        low_pass_filter,
        surround_3db_attenuation,
        warp_mode,
        bed_conform,
    })
}

/// `-wm` and `-bc` (the format is checked by the caller).
fn truehdd_options(args: &Args) -> Result<(Option<WarpMode>, Option<bool>), Fail> {
    let warp_mode = match args.warp_mode.as_deref() {
        Some(m) => Some(WarpMode::parse(m).ok_or(Fail::new(ErrorKind::WarpMode))?),
        None => None,
    };
    let bed_conform = match args
        .bed_conform
        .as_deref()
        .map(str::to_lowercase)
        .as_deref()
    {
        None => None,
        Some("on") => Some(true),
        Some("off") => Some(false),
        Some(_) => return Err(Fail::new(ErrorKind::OnOff)),
    };
    Ok((warp_mode, bed_conform))
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
    // an Atmos master has no say in the rate and channels of the others
    let real: Vec<&Probe> = probes.iter().filter(|p| p.master.is_none()).collect();
    let basis: Vec<&Probe> = if real.is_empty() {
        probes.iter().collect()
    } else {
        real
    };
    let rates: Vec<i64> = basis.iter().map(|p| p.sample_rate).collect();
    let channels: Vec<i64> = basis.iter().map(|p| p.channels).collect();
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
    if let Some((file, _)) = pairs().find(|(_, probe)| !probe.is_truehd && probe.master.is_none()) {
        return Err(Fail::with(ErrorKind::AtmosInput, escape(file)));
    }
    match pairs().find(|(_, probe)| !probe.is_atmos) {
        Some((file, _)) => Err(Fail::with(ErrorKind::AtmosNoAtmos, escape(file))),
        None => Ok(()),
    }
}

/// Whether the source of AC4 IMS is Atmos: a master, or TrueHD with an Atmos
/// presentation (decoded by truehdd). Any other source is 5.1 / 7.1 audio.
pub fn ims_atmos_source(probe: &Probe) -> bool {
    probe.master.is_some() || (probe.is_truehd && probe.is_atmos)
}

/// AC4 IMS takes Atmos or channel-based inputs, not both in one run (the
/// layout of the run depends on it).
pub fn check_ims_inputs(probes: &[Probe]) -> Result<(), Fail> {
    let atmos = probes.iter().filter(|p| ims_atmos_source(p)).count();
    if atmos == 0 || atmos == probes.len() {
        Ok(())
    } else {
        Err(Fail::new(ErrorKind::Ac4ImsMixed))
    }
}

/// Atmos masters (ADM BWF, DAMF) go to the formats whose encoder reads them.
/// DD and DDP render a 5.1 / 2.0 / 1.0 from them, which can't be combined
/// with other inputs (their layout follows the source). `-mo` takes any master.
pub fn check_master_inputs(
    format: Format,
    probes: &[Probe],
    measure_only: bool,
) -> Result<(), Fail> {
    if measure_only {
        return Ok(());
    }
    if !probes.iter().any(|p| p.master.is_some()) {
        return Ok(());
    }
    if !format.takes_master() {
        return Err(Fail::with(ErrorKind::MasterFormat, format.name()));
    }
    let masters = probes.iter().filter(|p| p.master.is_some()).count();
    if matches!(format, Format::Dd | Format::Ddp) && masters != probes.len() {
        return Err(Fail::new(ErrorKind::MasterMixed));
    }
    Ok(())
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
    /// DEE downmixes by itself (fewer output channels than the wav it gets),
    /// which delays DD, DDP and AAC output by 32 samples.
    pub dee_downmix: bool,
    /// What `-dl` did or could not do, as notes (markup).
    pub notes: Vec<String>,
}

impl Layout {
    /// The channels of the output for `{channels}` of an output name: `2.0`,
    /// `5.1`, `7.1`, `5.0`, `7.0`. Atmos outputs have the layout deew knows
    /// they are coded in, empty if it does not know one.
    pub fn name_label(&self, format: Format, atmos_mode: DdpAtmosMode) -> String {
        match format {
            Format::Ac4Ims => return "2.0".to_string(),
            Format::DdpAtmos => {
                return match atmos_mode {
                    DdpAtmosMode::Streaming => "5.1",
                    DdpAtmosMode::Standard | DdpAtmosMode::Bluray => "7.1",
                }
                .to_string();
            }
            Format::ThdAtmos => return "7.1".to_string(),
            Format::Ac4Atmos => return String::new(),
            _ => {}
        }
        match (self.outchannels, self.lfe) {
            (6, Lfe::Removed) => "5.0",
            (8, Lfe::Removed) => "7.0",
            (1, _) => "1.0",
            (2, _) => "2.0",
            (6, _) => "5.1",
            (8, _) => "7.1",
            _ => "",
        }
        .to_string()
    }

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
            dee_downmix: false,
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
    // AC4 IMS needs the 5.1 of the source (5.0 gets a silent LFE, 7.x is downmixed)
    if format == Format::Ac4Ims && channels <= 2 {
        return Err(Fail::new(ErrorKind::Ac4ImsInput));
    }
    if format == Format::Thd && channels == 1 {
        return Err(Fail::new(ErrorKind::ThdMonoInput));
    }
    let slots = wav_channels(channels);
    let downmix = match (format, downmix) {
        (Format::Dd | Format::Ac4 | Format::Ac4Ims, None) if slots == 8 => Some(6),
        (_, downmix) => downmix,
    };
    let outchannels = match format {
        // DEE's AAC encoder always outputs 2.0 and downmixes by itself
        Format::Aac => 2,
        _ => downmix.unwrap_or(slots),
    };
    // DEE downmixes by itself, except for TrueHD and AC4 IMS (ffmpeg does, see `audio_plan`)
    let layout_arg = (!matches!(format, Format::Aac | Format::Thd | Format::Ac4Ims)
        && outchannels != slots)
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
    let dee_downmix = match format {
        Format::Dd | Format::Ddp => outchannels < slots,
        Format::Aac => slots > 2,
        _ => false,
    };
    Ok(Layout {
        outchannels,
        layout_arg,
        lfe,
        dee_downmix,
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
pub fn notices(
    format: Format,
    outchannels: i64,
    measure_only: bool,
    dialnorm: i64,
    surround_3db: bool,
) -> Vec<Notice> {
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
    if surround_3db && format == Format::Thd && outchannels >= 6 && !measure_only {
        list.push(Notice {
            note: "[bold yellow]Surround 3 dB attenuation[/bold yellow] turns the surround channels down before the lossless TrueHD encode, the output won't be identical to the source.",
            question: "[bold yellow]Surround 3 dB attenuation[/bold yellow] turns the surround channels down before the lossless TrueHD encode, the output won't be identical to the source. Continue?",
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
        (Format::Ac4Ims, _) => ("ac4_ims", defaults.ac4_ims),
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

/// TrueHD encoded at 96 kHz (`apodise_off`) needs dee_shim to be lossless.
pub fn check_shim_for_thd96(
    apodise_off: bool,
    measure_only: bool,
    shim_found: bool,
) -> Result<(), Fail> {
    if apodise_off && !measure_only && !shim_found {
        Err(Fail::new(ErrorKind::ShimThd96))
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
            ac4_ims: 256,
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
            a.track_index = TrackIndex::audio(0);
            a.downmix = Some(0);
        }))
        .unwrap();
        assert_eq!(
            (v.dialnorm, v.track_index, v.downmix),
            (-31, TrackIndex::audio(0), None)
        );
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
        for format in ["dd", "ddp", "ddp-atmos", "thd", "aac", "ac4-atmos"] {
            assert_eq!(drc(format, None), "none", "{format}");
        }
        assert_eq!(drc("ac4", None), "music_light");
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
        let e = validate_args_d(&args(|a| {
            a.format = "ac4".into();
            a.drc = Some("none".into())
        }))
        .unwrap_err();
        assert_eq!(e.kind, ErrorKind::Ac4Drc);
        // AC4 Atmos has the none preset
        let v = validate_args_d(&args(|a| {
            a.format = "ac4-atmos".into();
            a.drc = Some("none".into())
        }))
        .unwrap();
        assert_eq!(v.drc, "none");
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
    fn surround_phase_shift_is_for_dd_ddp_and_ddp_atmos() {
        for format in ["dd", "ddp", "DDP", "ddp-atmos"] {
            for (value, want) in [("off", false), ("ON", true), ("On", true)] {
                let v = validate_args_d(&args(|a| {
                    a.format = format.into();
                    a.surround_phase_shift = Some(value.into())
                }))
                .unwrap();
                assert_eq!(v.surround_phase_shift, Some(want), "{format} {value}");
            }
        }
        assert_eq!(
            validate_args_d(&Args::default())
                .unwrap()
                .surround_phase_shift,
            None
        );
        for format in ["thd", "thd-atmos", "ac4", "ac4-atmos", "aac"] {
            let e = validate_args_d(&args(|a| {
                a.format = format.into();
                a.surround_phase_shift = Some("off".into())
            }))
            .unwrap_err();
            assert_eq!(e.kind, ErrorKind::PhaseShiftFormat, "{format}");
        }
        // -mo ignores it
        let v = validate_args_d(&args(|a| {
            a.measure_only = true;
            a.format = "thd".into();
            a.surround_phase_shift = Some("off".into())
        }))
        .unwrap();
        assert_eq!(v.surround_phase_shift, None);
    }

    #[test]
    fn low_pass_filter_and_3db_are_for_dd_and_ddp() {
        for format in ["dd", "ddp", "DD"] {
            let v = validate_args_d(&args(|a| {
                a.format = format.into();
                a.low_pass_filter = Some("off".into());
                a.surround_3db_attenuation = Some("ON".into())
            }))
            .unwrap();
            assert_eq!(v.low_pass_filter, Some(false), "{format}");
            assert_eq!(v.surround_3db_attenuation, Some(true), "{format}");
        }
        let v = validate_args_d(&Args::default()).unwrap();
        assert_eq!(
            (v.low_pass_filter, v.surround_3db_attenuation),
            (None, None)
        );
        for format in ["ddp-atmos", "thd", "thd-atmos", "ac4", "ac4-atmos", "aac"] {
            let e = validate_args_d(&args(|a| {
                a.format = format.into();
                a.low_pass_filter = Some("off".into())
            }))
            .unwrap_err();
            assert_eq!(e.kind, ErrorKind::LowPassFilterFormat, "{format}");
            let thd = format == "thd";
            let v = validate_args_d(&args(|a| {
                a.format = format.into();
                a.surround_3db_attenuation = Some("on".into())
            }));
            if thd {
                // -s3a also works for TrueHD
                assert_eq!(v.unwrap().surround_3db_attenuation, Some(true));
            } else {
                assert_eq!(
                    v.unwrap_err().kind,
                    ErrorKind::Surround3dbFormat,
                    "{format}"
                );
            }
        }
    }

    #[test]
    fn warp_mode_and_bed_conform_are_for_the_truehdd_formats() {
        for format in ["ddp-atmos", "thd-atmos", "ac4-atmos", "ac4-ims", "AC4-IMS"] {
            let v = validate_args_d(&args(|a| {
                a.format = format.into();
                a.warp_mode = Some("LoRo".into());
                a.bed_conform = Some("ON".into())
            }))
            .unwrap();
            assert_eq!(v.warp_mode, Some(WarpMode::Loro), "{format}");
            assert_eq!(v.bed_conform, Some(true), "{format}");
        }
        let v = validate_args_d(&args(|a| {
            a.format = "thd-atmos".into();
            a.bed_conform = Some("off".into())
        }))
        .unwrap();
        assert_eq!((v.warp_mode, v.bed_conform), (None, Some(false)));
        let v = validate_args_d(&Args::default()).unwrap();
        assert_eq!((v.warp_mode, v.bed_conform), (None, None));
        for format in ["dd", "ddp", "thd", "ac4", "aac"] {
            let e = validate_args_d(&args(|a| {
                a.format = format.into();
                a.warp_mode = Some("normal".into())
            }))
            .unwrap_err();
            assert_eq!(e.kind, ErrorKind::WarpModeFormat, "{format}");
            let e = validate_args_d(&args(|a| {
                a.format = format.into();
                a.bed_conform = Some("on".into())
            }))
            .unwrap_err();
            assert_eq!(e.kind, ErrorKind::BedConformFormat, "{format}");
        }
        // -mo decodes a TrueHD Atmos source with truehdd, any format
        for format in ["dd", "thd", "aac"] {
            let v = validate_args_d(&args(|a| {
                a.measure_only = true;
                a.format = format.into();
                a.warp_mode = Some("warping".into());
                a.bed_conform = Some("on".into())
            }))
            .unwrap();
            assert_eq!(v.warp_mode, Some(WarpMode::Warping), "{format}");
            assert_eq!(v.bed_conform, Some(true), "{format}");
        }
    }

    #[test]
    fn bad_warp_mode_and_bed_conform_values() {
        let e = validate_args_d(&args(|a| {
            a.format = "ddp-atmos".into();
            a.warp_mode = Some("x".into())
        }))
        .unwrap_err();
        assert_eq!(e.kind, ErrorKind::WarpMode);
        let e = validate_args_d(&args(|a| {
            a.format = "ddp-atmos".into();
            a.bed_conform = Some("maybe".into())
        }))
        .unwrap_err();
        assert_eq!(e.kind, ErrorKind::OnOff);
        // bad values of -mo are errors too
        let e = validate_args_d(&args(|a| {
            a.measure_only = true;
            a.warp_mode = Some("x".into())
        }))
        .unwrap_err();
        assert_eq!(e.kind, ErrorKind::WarpMode);
    }

    #[test]
    fn thd_surround_3db_notice() {
        let n = |f, ch, mo, s3a| notices(f, ch, mo, 0, s3a);
        for ch in [6, 8] {
            let list = n(Format::Thd, ch, false, true);
            assert_eq!(list.len(), 1);
            assert!(
                list[0]
                    .question
                    .ends_with("won't be identical to the source. Continue?")
            );
            assert!(list[0].note.ends_with("won't be identical to the source."));
        }
        assert!(n(Format::Thd, 2, false, true).len() == 1); // the FLAC one only
        assert!(!n(Format::Thd, 2, false, true)[0].note.contains("3 dB"));
        assert!(n(Format::Thd, 6, false, false).is_empty());
        assert!(n(Format::Thd, 6, true, true).is_empty());
        assert!(n(Format::Ddp, 6, false, true).is_empty());
    }

    #[test]
    fn on_off_values_are_validated() {
        for set in [
            (|a: &mut Args| a.surround_phase_shift = Some("yes".into())) as fn(&mut Args),
            |a| a.low_pass_filter = Some("1".into()),
            |a| a.surround_3db_attenuation = Some("".into()),
        ] {
            let e = validate_args_d(&args(set)).unwrap_err();
            assert_eq!(e.kind, ErrorKind::OnOff);
        }
    }

    #[test]
    fn measure_only_ignores_the_encoding_options() {
        let v = validate_args_d(&args(|a| {
            a.measure_only = true;
            a.format = "thd".into();
            a.dialnorm = -27
        }))
        .unwrap();
        assert_eq!(v.format, Format::Ddp);
        assert_eq!(v.dialnorm, 0);
        // nothing format dependent is checked, not even the format
        for format in ["ddp-atmos", "thd-atmos", "ac4-atmos", "aac", "nope"] {
            let v = validate_args_d(&args(|a| {
                a.measure_only = true;
                a.format = format.into();
                a.downmix = Some(2);
                a.drc = Some("bogus".into());
                a.ddp_mode = Some("bogus".into());
                a.ddp_atmos_mode = Some("bogus".into());
                a.dialnorm = -27;
            }))
            .unwrap();
            assert_eq!(v.format, Format::Ddp, "{format}");
            assert_eq!(v.downmix, None, "{format}");
            assert_eq!(v.dialnorm, 0, "{format}");
        }
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
            master: None,
            language: None,
            title: None,
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
        let n = |f, ch, mo, dn| notices(f, ch, mo, dn, false);
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
    fn dee_downmix_is_set_when_dee_downmixes() {
        let f = |format, dm, ch, dl| resolve_layout(format, dm, ch, dl).unwrap().dee_downmix;
        assert!(f(Format::Dd, Some(2), 6, false));
        assert!(f(Format::Ddp, Some(1), 6, false));
        assert!(f(Format::Dd, None, 8, false));
        assert!(f(Format::Ddp, Some(6), 8, false));
        assert!(f(Format::Aac, None, 6, false));
        assert!(f(Format::Aac, None, 8, false));
        assert!(!f(Format::Ddp, None, 6, false));
        assert!(!f(Format::Dd, None, 6, true));
        assert!(!f(Format::Ddp, None, 6, true));
        assert!(!f(Format::Aac, None, 2, false));
        assert!(!f(Format::Thd, Some(2), 6, false));
        assert!(!f(Format::Ac4, None, 8, false));
        assert!(!f(Format::Ac4Ims, None, 6, false));
        assert!(!f(Format::DdpAtmos, None, 6, false));
        assert!(!f(Format::ThdAtmos, None, 8, false));
        assert!(!f(Format::Ac4Atmos, None, 6, false));
        assert!(!f(Format::Dd, None, 5, false));
        assert!(!f(Format::Ddp, None, 7, false));
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
            (Format::Ac4Ims, 2048),
        ] {
            assert_eq!(f.encoder_delay(256, false), samples, "{f:?}");
            let extra = if matches!(f, Format::Dd | Format::Ddp) {
                32
            } else {
                0
            };
            assert_eq!(f.encoder_delay(256, true), samples + extra, "{f:?}");
        }
        for (rate, samples) in [
            (56, 1710),
            (64, 1710),
            (71, 1710),
            (72, 1702),
            (80, 1702),
            (87, 1702),
            (88, 1600),
            (96, 1600),
            (256, 1600),
        ] {
            assert_eq!(Format::Aac.encoder_delay(rate, false), samples, "{rate}");
            assert_eq!(
                Format::Aac.encoder_delay(rate, true),
                samples + 32,
                "{rate}"
            );
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
    fn thd_96khz_needs_dee_shim() {
        let err = Err(Fail::new(ErrorKind::ShimThd96));
        assert_eq!(check_shim_for_thd96(true, false, false), err);
        assert_eq!(check_shim_for_thd96(true, false, true), Ok(()));
        assert_eq!(check_shim_for_thd96(true, true, false), Ok(()));
        assert_eq!(check_shim_for_thd96(false, false, false), Ok(()));
    }

    #[test]
    fn ddp_shim_check() {
        let lay = |lfe| Layout {
            outchannels: 8,
            layout_arg: None,
            lfe,
            dee_downmix: false,
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
            dee_downmix: false,
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

    // ---- AC4 IMS and Atmos masters ----

    fn ims_args(edit: impl FnOnce(&mut Args)) -> Args {
        args(|a| {
            a.format = "ac4-ims".into();
            edit(a);
        })
    }

    #[test]
    fn ac4_ims_format_names() {
        for name in ["ac4-ims", "ac4_ims"] {
            assert_eq!(Format::parse(name), Some(Format::Ac4Ims));
        }
        assert_eq!(Format::Ac4Ims.name(), "ac4-ims");
        assert_eq!(Format::Ac4Ims.dee_tool(), "dee_ac4ims_encoder");
        assert_eq!(Format::Ac4Ims.extension(6, false), "ac4");
        assert!(!Format::Ac4Ims.is_atmos());
        assert!(!Format::Ac4Ims.has_dialnorm());
        // the format list accepts it case-insensitively
        let v = validate_args_d(&ims_args(|a| a.format = "AC4-IMS".into())).unwrap();
        assert_eq!(v.format, Format::Ac4Ims);
    }

    #[test]
    fn ac4_ims_defaults_and_modes() {
        let v = validate_args_d(&ims_args(|_| {})).unwrap();
        assert_eq!((v.drc.as_str(), v.ims_mode), ("none", Ac4ImsMode::General));
        let v = validate_args_d(&ims_args(|a| a.drc = Some("speech".into()))).unwrap();
        assert_eq!(v.drc, "speech");
        let v = validate_args_d(&ims_args(|a| a.ac4_ims_mode = Some("Music".into()))).unwrap();
        assert_eq!(
            (v.drc.as_str(), v.ims_mode),
            ("music_light", Ac4ImsMode::Music)
        );
        let v = validate_args_d(&ims_args(|a| {
            a.ac4_ims_mode = Some("MUSIC".into());
            a.drc = Some("music_light".into());
        }))
        .unwrap();
        assert_eq!(v.drc, "music_light");
        let e = validate_args_d(&ims_args(|a| {
            a.ac4_ims_mode = Some("music".into());
            a.drc = Some("none".into());
        }))
        .unwrap_err();
        assert_eq!(e.kind, ErrorKind::Ac4ImsMusicDrc);
        let e = validate_args_d(&ims_args(|a| a.ac4_ims_mode = Some("x".into()))).unwrap_err();
        assert_eq!(e.kind, ErrorKind::Ac4ImsMode);
        // other formats ignore the mode (a note is shown), also with a music mode
        let v = validate_args_d(&args(|a| {
            a.format = "ddp".into();
            a.ac4_ims_mode = Some("music".into());
        }))
        .unwrap();
        assert_eq!(v.drc, "none");
    }

    #[test]
    fn ac4_ims_mode_comes_from_the_config() {
        let music = DefaultModes {
            ac4_ims: Ac4ImsMode::Music,
            ..DefaultModes::default()
        };
        let v = validate_args(&ims_args(|_| {}), &music).unwrap();
        assert_eq!(
            (v.drc.as_str(), v.ims_mode),
            ("music_light", Ac4ImsMode::Music)
        );
        let v = validate_args(
            &ims_args(|a| a.ac4_ims_mode = Some("general".into())),
            &music,
        )
        .unwrap();
        assert_eq!(v.ims_mode, Ac4ImsMode::General);
        let e =
            validate_args(&ims_args(|a| a.drc = Some("film_light".into())), &music).unwrap_err();
        assert_eq!(e.kind, ErrorKind::Ac4ImsMusicDrc);
        let v = validate_args_d(&ims_args(|_| {})).unwrap();
        assert_eq!(v.ims_mode, Ac4ImsMode::General);
    }

    #[test]
    fn ac4_ims_rejects_downmix_lfe_and_dialnorm() {
        let kind = |edit: fn(&mut Args)| validate_args_d(&ims_args(edit)).unwrap_err().kind;
        assert_eq!(kind(|a| a.downmix = Some(2)), ErrorKind::Ac4ImsDownmix);
        assert_eq!(kind(|a| a.disable_lfe = true), ErrorKind::Ac4ImsLfe);
        assert_eq!(kind(|a| a.dialnorm = -27), ErrorKind::DialnormFormat);
        let kind = |edit: fn(&mut Args)| validate_args_d(&ims_args(edit)).unwrap_err().kind;
        assert_eq!(
            kind(|a| a.surround_phase_shift = Some("off".into())),
            ErrorKind::PhaseShiftFormat
        );
        // -mo ignores them like the other options
        let v = validate_args_d(&args(|a| {
            a.format = "ac4-ims".into();
            a.measure_only = true;
            a.downmix = Some(2);
        }))
        .unwrap();
        assert_eq!(v.format, Format::Ddp);
    }

    #[test]
    fn ac4_ims_layout() {
        let l = |channels, lfe| resolve_layout(Format::Ac4Ims, None, channels, lfe);
        let layout = l(6, false).unwrap();
        assert_eq!(
            (layout.outchannels, layout.layout_arg, layout.lfe),
            (6, None, Lfe::Normal)
        );
        // 7.1 is downmixed by ffmpeg, DEE gets no `-c`
        let layout = l(8, false).unwrap();
        assert_eq!((layout.outchannels, layout.layout_arg), (6, None));
        // 5.0 gets a silent LFE, 7.0 a downmix with it
        let layout = l(5, false).unwrap();
        assert_eq!((layout.outchannels, layout.lfe), (6, Lfe::Silent));
        let layout = l(7, false).unwrap();
        assert_eq!((layout.outchannels, layout.lfe), (6, Lfe::Silent));
        for channels in [1, 2] {
            assert_eq!(l(channels, false).unwrap_err().kind, ErrorKind::Ac4ImsInput);
        }
        assert_eq!(l(3, false).unwrap_err().kind, ErrorKind::Channels);
    }

    #[test]
    fn ac4_ims_bitrates() {
        assert_eq!(br(Format::Ac4Ims, 6, None), Some(256));
        assert_eq!(br(Format::Ac4Ims, 0, None), Some(256));
        assert_eq!(br(Format::Ac4Ims, 6, Some(64)), Some(64));
        assert_eq!(br(Format::Ac4Ims, 6, Some(320)), Some(320));
        // the closest value DEE takes
        assert_eq!(br(Format::Ac4Ims, 6, Some(200)), Some(144));
        assert_eq!(br(Format::Ac4Ims, 6, Some(1000)), Some(320));
        assert_eq!(br(Format::Ac4Ims, 6, Some(10)), Some(64));
        let mut d = defaults();
        d.ac4_ims = 96;
        assert_eq!(
            select_bitrate(Format::Ac4Ims, 6, std_modes(), None, &d),
            Some(96)
        );
    }

    fn master_probe() -> Probe {
        Probe::of_master(crate::master::Master {
            kind: crate::master::MasterKind::Adm,
            duration: Some(12.0),
        })
    }

    fn truehd_probe(atmos: bool) -> Probe {
        Probe {
            is_truehd: true,
            is_atmos: atmos,
            ..probe(48000, 8, 24)
        }
    }

    #[test]
    fn masters_and_formats() {
        let masters = [master_probe()];
        for format in [
            Format::Dd,
            Format::Ddp,
            Format::DdpAtmos,
            Format::ThdAtmos,
            Format::Ac4Atmos,
            Format::Ac4Ims,
        ] {
            assert!(
                check_master_inputs(format, &masters, false).is_ok(),
                "{format:?}"
            );
        }
        for format in [Format::Thd, Format::Ac4, Format::Aac] {
            let e = check_master_inputs(format, &masters, false).unwrap_err();
            assert_eq!(e.kind, ErrorKind::MasterFormat);
            assert_eq!(e.insert.as_deref(), Some(format.name()));
            // `-mo` measures any master
            assert!(check_master_inputs(format, &masters, true).is_ok());
        }
        // no master, no check
        assert!(check_master_inputs(Format::Aac, &[probe(48000, 2, 24)], false).is_ok());
        // DD and DDP take a master alone
        let mixed = [master_probe(), probe(48000, 6, 24)];
        for format in [Format::Dd, Format::Ddp] {
            assert_eq!(
                check_master_inputs(format, &mixed, false).unwrap_err().kind,
                ErrorKind::MasterMixed
            );
        }
        assert!(check_master_inputs(Format::DdpAtmos, &mixed, false).is_ok());
    }

    #[test]
    fn masters_are_atmos_inputs_and_do_not_set_the_layout() {
        let files = ["a.wav".to_string(), "b.thd".to_string()];
        let probes = [master_probe(), truehd_probe(true)];
        assert!(check_atmos_inputs(&files, &probes).is_ok());
        // TrueHD is 8 channels, a master has none to compare
        assert_eq!(check_inputs(&probes).unwrap().channels, 8);
        let all = check_inputs(&[master_probe(), master_probe()]).unwrap();
        assert_eq!(
            (all.samplerate, all.channels, all.bit_depths.len()),
            (48000, 6, 2)
        );
        // a master next to a wav that is not Atmos
        let files = ["a.wav".to_string(), "b.wav".to_string()];
        let probes = [master_probe(), probe(48000, 2, 24)];
        assert_eq!(
            check_atmos_inputs(&files, &probes).unwrap_err().kind,
            ErrorKind::AtmosInput
        );
    }

    #[test]
    fn ac4_ims_atmos_and_channel_inputs_do_not_mix() {
        assert!(ims_atmos_source(&master_probe()));
        assert!(ims_atmos_source(&truehd_probe(true)));
        assert!(!ims_atmos_source(&truehd_probe(false)));
        assert!(check_ims_inputs(&[master_probe(), truehd_probe(true)]).is_ok());
        assert!(check_ims_inputs(&[probe(48000, 6, 24), truehd_probe(false)]).is_ok());
        assert_eq!(
            check_ims_inputs(&[master_probe(), probe(48000, 6, 24)])
                .unwrap_err()
                .kind,
            ErrorKind::Ac4ImsMixed
        );
    }
}
