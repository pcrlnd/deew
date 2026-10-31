export type Feature = {
  text: string
  sub?: string[]
  tall?: boolean
}

export const features: Feature[] = [
  {
    text: "handles Dolby's XML input fuckery and creating the RF64 intermediate in the background, giving you a proper tool",
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
    tall: true,
  },
  { text: "runs multiple encodes in parallel for batch encoding (see config)" },
  {
    text: "supports WSL path conversion for the Windows version of DEE (see config)",
  },
  { text: "works even with video inputs" },
  {
    text: "automatically compensates for DEE's 256 sample delay (DD and DDP encoding)",
  },
  {
    text: "versatile delay option that supports ms, s and also frame@fps formats, parsing delay from filenames also supported",
  },
]
