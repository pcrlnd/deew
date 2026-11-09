import { CircleCheckIcon, DownloadIcon, ExternalLinkIcon } from "lucide-react"

import {
  AppleIcon,
  GithubIcon,
  KofiIcon,
  LinuxIcon,
  MoneroIcon,
  WindowsIcon,
} from "@/components/brand-icons"
import { CopyBlock } from "@/components/copy-block"
import { GetStarted } from "@/components/get-started"
import { SiteFooter, SiteHeader } from "@/components/site-shell"
import { Badge } from "@/components/ui/badge"
import { buttonVariants } from "@/components/ui/button"
import { Separator } from "@/components/ui/separator"
import { features } from "@/features"
import { detectPlatform, KOFI, LATEST, RELEASES, REPO } from "@/lib/platform"

function DownloadButton({
  platform,
}: {
  platform: ReturnType<typeof detectPlatform>
}) {
  const known = platform !== "other"
  const file =
    platform === "windows"
      ? "deew.exe"
      : platform === "macos"
        ? "deew-macos"
        : "deew"
  const href = known ? LATEST + file : RELEASES
  const label = !known
    ? "Download"
    : platform === "windows"
      ? "Download for Windows"
      : platform === "macos"
        ? "Download for macOS"
        : "Download for Linux"
  const Icon = !known
    ? DownloadIcon
    : platform === "windows"
      ? WindowsIcon
      : platform === "macos"
        ? AppleIcon
        : LinuxIcon
  return (
    <a
      href={href}
      className={buttonVariants({
        size: "lg",
        className: "h-11 px-5 text-sm",
      })}
    >
      <Icon className="size-5" />
      {label}
    </a>
  )
}

export default function App() {
  const platform = detectPlatform()

  return (
    <div className="min-h-screen">
      <SiteHeader current="home" />
      <header className="border-b">
        <div className="mx-auto grid max-w-6xl items-center gap-10 px-4 py-10 sm:px-6 sm:py-16 lg:grid-cols-2">
          <div className="min-w-0">
            <div className="flex items-center gap-4">
              <img
                src="logo.svg"
                alt="deew logo"
                width={640}
                height={640}
                className="size-14 shrink-0 sm:size-20"
              />
              <h1 className="text-2xl font-semibold tracking-tight sm:text-4xl">
                Dolby Encoding Engine Wrapper
              </h1>
            </div>
            <p className="mt-6 max-w-xl text-base text-muted-foreground sm:text-lg">
              A fast CLI and GUI for{" "}
              <b className="text-foreground">Dolby Encoding Engine</b>. deew
              prepares your audio with{" "}
              <b className="text-foreground">ffmpeg</b> and{" "}
              <b className="text-foreground">truehdd</b> and encodes it to DD,
              DDP, TrueHD, AC4 and AAC, including Atmos.
            </p>
            <div className="mt-8 flex flex-wrap gap-3">
              <DownloadButton platform={platform} />
              <a
                href={REPO}
                className={buttonVariants({
                  size: "lg",
                  variant: "outline",
                  className: "h-11 px-5 text-sm",
                })}
              >
                <GithubIcon className="size-5" />
                View on GitHub
              </a>
            </div>
            <p className="mt-4 text-sm">
              <a
                className="text-muted-foreground underline underline-offset-4 hover:text-foreground"
                href={RELEASES}
              >
                All releases &amp; changelogs
              </a>
            </p>
          </div>
          <figure className="min-w-0 border bg-card p-2">
            <img
              src="terminal.gif"
              alt="Animated terminal recording of deew encoding several files in parallel"
              width={1402}
              height={1290}
              className="block h-auto w-full"
              fetchPriority="high"
            />
          </figure>
        </div>
      </header>

      <main className="mx-auto max-w-6xl px-4 sm:px-6">
        <section
          id="features"
          aria-labelledby="features-h"
          className="py-12 sm:py-16"
        >
          <h2 id="features-h" className="text-2xl font-semibold tracking-tight">
            Features
          </h2>
          <p className="mt-2 text-muted-foreground">
            Everything deew does in the background so you don't have to fight
            DEE's command line tools.
          </p>
          <ul className="mt-6 gap-x-10 lg:columns-2">
            {features.map((f) => (
              <li
                key={f.text}
                className="mb-4 flex break-inside-avoid items-start gap-3"
              >
                <CircleCheckIcon className="mt-0.5 size-5 shrink-0 text-primary dark:text-blue-400" />
                <div className="min-w-0 text-sm">
                  <p>
                    {f.text}
                    {f.link && (
                      <>
                        {" "}
                        <a
                          className="font-medium text-primary underline underline-offset-4 dark:text-blue-400"
                          href={f.link.href}
                        >
                          {f.link.label}
                        </a>
                      </>
                    )}
                  </p>
                  {f.sub && (
                    <ul className="mt-2 list-disc space-y-1 pl-5 text-muted-foreground">
                      {f.sub.map((s) => (
                        <li key={s}>{s}</li>
                      ))}
                    </ul>
                  )}
                </div>
              </li>
            ))}
          </ul>
        </section>

        <Separator />

        <section
          id="get-started"
          aria-labelledby="get-started-h"
          className="py-12 sm:py-16"
        >
          <h2
            id="get-started-h"
            className="text-2xl font-semibold tracking-tight"
          >
            Get started
          </h2>
          <p className="mt-2 mb-6 max-w-2xl text-muted-foreground">
            deew ships as standalone builds for{" "}
            <b className="text-foreground">Windows</b>,{" "}
            <b className="text-foreground">Linux</b> and{" "}
            <b className="text-foreground">macOS</b>, both as a command line
            tool and as a desktop GUI. You can also build it from source.
          </p>

          <GetStarted platform={platform} />

          <div className="mt-6 border bg-card text-card-foreground">
            <div className="border-b p-4">
              <h3 className="text-base font-semibold">Requirements</h3>
              <p className="text-sm text-muted-foreground">
                deew drives these tools, so they need to be installed.
              </p>
            </div>
            <div className="flex flex-col gap-4 p-4 sm:flex-row sm:items-center sm:justify-between">
              <ul className="flex flex-wrap gap-2">
                {[
                  { label: "ffmpeg" },
                  { label: "ffprobe" },
                  {
                    label: "Dolby Encoding Engine 6.7",
                    href: "https://customer.dolby.com/content-creation-and-delivery/dolby-encoding-engine-v670",
                  },
                  {
                    label: "truehdd",
                    href: "https://github.com/truehdd/truehdd",
                  },
                  {
                    label: "dee_shim",
                    href: "https://github.com/truehdd/dee-shim",
                  },
                ].map(({ label, href }) => (
                  <li key={label}>
                    {href ? (
                      <Badge
                        variant="outline"
                        className="h-6 px-2.5 text-sm"
                        render={<a href={href} />}
                      >
                        {label}
                        <ExternalLinkIcon
                          className="size-3"
                          aria-hidden="true"
                        />
                      </Badge>
                    ) : (
                      <Badge variant="outline" className="h-6 px-2.5 text-sm">
                        {label}
                      </Badge>
                    )}
                  </li>
                ))}
              </ul>
            </div>
          </div>
        </section>

        <Separator />

        <section
          id="donate"
          aria-labelledby="donate-h"
          className="py-12 sm:py-16"
        >
          <h2 id="donate-h" className="text-2xl font-semibold tracking-tight">
            Donate
          </h2>
          <p className="mt-2 mb-6 max-w-2xl text-muted-foreground">
            If you like deew, you can support the people behind it on Ko-fi or
            with Monero.
          </p>

          <div className="border bg-card text-card-foreground">
            <div className="flex flex-col gap-3 border-b p-4">
              <p className="text-sm font-medium">pcroland</p>
              <a
                className="inline-flex w-fit items-center gap-2 text-sm text-muted-foreground underline-offset-4 hover:text-foreground hover:underline"
                href={KOFI}
              >
                <KofiIcon className="size-4 shrink-0 text-[#ff6433]" />
                Ko-fi
                <ExternalLinkIcon className="size-3" aria-hidden="true" />
              </a>
              <div className="flex flex-col gap-2">
                <p className="flex items-start gap-2 text-sm text-muted-foreground">
                  <MoneroIcon className="mt-0.5 size-4 shrink-0 text-[#ff6600]" />
                  Monero
                </p>
                <CopyBlock
                  text="42pcroLMNqQbs87sSuNNTk8E8XHD88NhnSAx3exA3s1mHWAAyKH1oBw7dScYVRx3VJhUZT96QZNeo9Uw2qL72kwoHxVBXmL"
                  label="pcroland Monero address"
                  wrap
                />
              </div>
            </div>
            <div className="flex flex-col gap-2 p-4">
              <p className="flex items-start gap-2 text-sm font-medium">
                <MoneroIcon className="mt-0.5 size-4 shrink-0 text-[#ff6600]" />
                rainbaby (developer of truehdd and dee_shim)
              </p>
              <CopyBlock
                text="47BgNcwYCWhSHB3WMd7CPLX3Yb4Kz8TCsGJaUmX7KqG895UKvixks6yje3GSa489XR97FLQMcUaSacAHbMbF2xWz7dvyX9U"
                label="rainbaby Monero address"
                wrap
              />
            </div>
          </div>
        </section>
      </main>

      <SiteFooter />
    </div>
  )
}
