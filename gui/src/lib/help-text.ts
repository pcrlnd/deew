// Tooltip texts, taken from the help text of the CLI (src/help.rs).

export const HELP = {
  input: "Audio file(s) or folder(s).",
  trackIndex: "Select audio track index of input(s).\nDefault: 0",
  output:
    "Specifies output directory.\nDefault: your home folder (the GUI runs deew from there).\nThe value is saved as path in the [output] section of config.toml.",
  outputToInput:
    "Save each output next to its input file.\nThis overrides the output directory (its value is kept).\nThe setting is saved as to_input in the [output] section of config.toml.",
  format:
    "Options: DD / DDP / DDP Atmos / TrueHD / TrueHD Atmos / AC4 / AC4 Atmos / AAC (dd / ddp / ddp-atmos / thd / thd-atmos / ac4 / ac4-atmos / aac).\nThe Atmos formats need a TrueHD Atmos input: DDP Atmos and TrueHD Atmos use truehdd and dee_shim, AC4 Atmos uses truehdd. TrueHD Atmos has no bitrate.\nDefault: ddp",
  bitrate:
    "Bitrates DEE can do for the selected format (see Utilities > List bitrates).\nDefault: taken from the config (see Utilities > Show config).",
  downmix:
    "Specifies downmix, only works for DD/DDP/AC4/AAC and TrueHD (DD/DDP: 1.0, 2.0 or 5.1; AC4: 2.0 or 5.1; AAC: 2.0; TrueHD: 2.0 or 5.1, downmixed by ffmpeg).\nDD and AC4 will be automatically downmixed to 5.1 in case of a 7.1 source.\nAAC is always 2.0 (mono input is not supported).",
  delay:
    "Specifies delay as ms, s or frame@FPS.\nFPS can be a number, division or ntsc / pal.\nExamples: -5.1ms, +1,52s, -24@pal, +10@24000/1001\nDefault: 0ms or parsed from filename.\nNegative values are passed as -d=-0ms automatically.",
  drc: "Specifies drc profile.\nOptions: none / film_light / film_standard / music_light / music_standard / speech\nnone (default): no DRC, uses dee_shim for TrueHD; not available for AC4 (music_light is used there).\nNot used for AAC.",
  dialnorm:
    "Applied dialnorm value, between -31 and 0 (in case of 0 DEE's measurement will be used).\nDoesn't work for AAC and AC4 Atmos.\nDefault: 0",
  instances:
    "Specifies how many encodes can run at the same time.\nExamples: 1, 4, 50%\n50% means 4 on a cpu with 8 threads. One DEE can use 2 threads so 50% can utilize all threads.\nThis option overrides the config's number. Default: 50% (see config).",
  ddpMode:
    "Profile of 7.1 DDP, 5.1 and below are not affected.\nstandard: E-AC-3 only, 384-1792 kbps, uses dee_shim.\nbluray: AC-3 core + E-AC-3, 768-1664 kbps, also plays on AC-3 only devices.\nDefault: from the config's [default_modes] (see Utilities > Show config).",
  ddpAtmosMode:
    "Profile of DDP Atmos.\nstreaming: 5.1 with Atmos, 384-1024 kbps, the format of streaming services, best device support.\nstandard: 7.1 with Atmos, E-AC-3 only, 768-2048 kbps.\nbluray: 7.1 with Atmos, AC-3 core + E-AC-3, 1152-1664 kbps.\nDefault: from the config's [default_modes] (see Utilities > Show config).",
  disableLfe:
    "Removes the LFE channel from a 5.1 or 7.1 input. DD and DDP output real 5.0 / 7.0 (7.0 DDP needs dee_shim); TrueHD and AC4 can't leave the LFE out, so it is kept silent. Not used for AAC (always stereo) or Atmos.",
  keepTemp: "Keep temp files.",
  keepLog:
    "Keep TrueHD's encode log and write a .deew-log.html report next to each output.",
  measureOnly:
    "Only measures the loudness with DEE, no encoding.\nNot available for the Atmos formats.",
  longArgument: "Print ffmpeg and DEE arguments for each input.",
  noPrompt: "Skip confirmation questions, print them as notes and continue.",
  info: {
    "-h": "Show the help text of the CLI.",
    "-v": "Show version.",
    "-lb": "List bitrates that DEE can do for each format.",
    "-pl": "Show all logo variants you can set in the config.",
    "-cl": "Show changelog.",
    "-c": "Show config and config location.",
    "-gc": "Generate a new config next to the deew CLI.",
    "-gt": "Download ffmpeg, ffprobe, truehdd and dee_shim next to the deew CLI (everything except DEE).",
  },
} as const
