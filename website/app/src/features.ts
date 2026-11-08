export type Feature = {
  text: string
  sub?: string[]
  /** A link shown after the text. */
  link?: { label: string; href: string }
}

export const features: Feature[] = [
  {
    text: "drives the Dolby Encoding Engine 6.7 command line tools and creates the intermediate files in the background, giving you a proper tool",
  },
  {
    text: "automatic channel swapping for 7.1 sources (DEE swaps Ls Rs with Lrs Rrs for some reason)",
  },
  {
    text: "automatic sample rate conversion using ffmpeg's soxr resampler in case of an unsupported sample rate",
    sub: [
      "for DD/DDP/AC4 unsupported rates will be converted to 48 kHz",
      "for TrueHD unsupported rates will be converted to 48 kHz if source sample rate is lower than 72 kHz, otherwise will be converted to 96 kHz",
    ],
  },
  {
    text: "encodes DD, DDP, DDP Atmos, TrueHD, TrueHD Atmos, AC4, AC4 Atmos and AAC",
    sub: [
      "TrueHD Atmos input is decoded with truehdd and re-encoded from the Atmos master to DDP Atmos, TrueHD Atmos or AC4 Atmos",
      "dee_shim is required for DDP Atmos and TrueHD Atmos (it stops DEE from smoothing the object positions a second time) and for TrueHD with DRC none (the default)",
    ],
  },
  { text: "runs multiple encodes in parallel for batch encoding (see config)" },
  {
    text: "profiles for 7.1 DDP and DDP Atmos (--ddp-mode, --ddp-atmos-mode, defaults in the config)",
    sub: [
      "7.1 DDP: standard (E-AC-3 only, 384-1792 kbps, uses dee_shim, the default) or bluray (AC-3 core, 768-1664 kbps)",
      "DDP Atmos: streaming (5.1 with Atmos, 384-1024 kbps, the default), standard (7.1 with Atmos, E-AC-3 only, 768-2048 kbps) or bluray (7.1 with Atmos, AC-3 core, 1152-1664 kbps)",
    ],
  },
  { text: "works on Windows, Linux and macOS" },
  { text: "works even with video inputs" },
  {
    text: "automatically compensates for DEE's 256 sample delay (DD, DDP and AC4 encoding)",
  },
  {
    text: "versatile delay option that supports ms, s and also frame@fps formats, parsing delay from filenames also supported",
  },
  {
    text: "the tool chain page shows how deew connects ffprobe, ffmpeg, truehdd, dee_shim and the DEE tools, with the exact options it passes to each one for every format:",
    link: { label: "deew.app/tool-chain", href: "https://deew.app/tool-chain" },
  },
]
