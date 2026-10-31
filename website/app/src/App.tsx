import { CircleCheckIcon, DownloadIcon, ExternalLinkIcon } from "lucide-react"

import { GithubIcon, LinuxIcon, WindowsIcon } from "@/components/brand-icons"
import { GetStarted } from "@/components/get-started"
import { Badge } from "@/components/ui/badge"
import { buttonVariants } from "@/components/ui/button"
import { Separator } from "@/components/ui/separator"
import { features } from "@/features"
import { detectPlatform, LATEST, RELEASES, REPO } from "@/lib/platform"
import { cn } from "@/lib/utils"

function DownloadButton({
  platform,
}: {
  platform: ReturnType<typeof detectPlatform>
}) {
  const win = platform === "windows"
  const known = platform !== "other"
  const href = known ? LATEST + (win ? "deew.exe" : "deew") : RELEASES
  const label = !known
    ? "Download"
    : win
      ? "Download for Windows"
      : "Download for Linux"
  const Icon = !known ? DownloadIcon : win ? WindowsIcon : LinuxIcon
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
              A fast CLI and GUI that wraps{" "}
              <b className="text-foreground">ffmpeg</b>,{" "}
              <b className="text-foreground">ffprobe</b> and{" "}
              <b className="text-foreground">Dolby Encoding Engine</b> for
              Dolby-certified audio encoding (DD, DDP, AC4, TrueHD).
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
            DEE's XML files.
          </p>
          <ul className="mt-6 grid gap-x-10 gap-y-4 lg:grid-cols-2">
            {features.map((f) => (
              <li
                key={f.text}
                className={cn(
                  "flex items-start gap-3",
                  f.tall && "lg:row-span-3"
                )}
              >
                <CircleCheckIcon className="mt-0.5 size-5 shrink-0 text-primary dark:text-blue-400" />
                <div className="min-w-0 text-sm">
                  <p>{f.text}</p>
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
            <b className="text-foreground">Windows</b> and{" "}
            <b className="text-foreground">Linux</b>, both as a command line
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
                {["ffmpeg", "ffprobe", "Dolby Encoding Engine"].map((r) => (
                  <li key={r}>
                    <Badge variant="outline" className="h-6 px-2.5 text-sm">
                      {r}
                    </Badge>
                  </li>
                ))}
              </ul>
              <a
                className="inline-flex items-center gap-1.5 text-sm text-primary underline-offset-4 hover:underline dark:text-blue-400"
                href="https://github.com/pcrlnd/deew#dolby-encoding-engine-installation"
              >
                How to set up Dolby Encoding Engine
                <ExternalLinkIcon className="size-3.5" aria-hidden="true" />
              </a>
            </div>
          </div>
        </section>
      </main>

      <footer className="border-t">
        <div className="mx-auto flex max-w-6xl items-center justify-between gap-4 px-4 py-6 text-sm text-muted-foreground sm:px-6">
          <span>deew - Dolby Encoding Engine Wrapper</span>
          <a
            className="inline-flex items-center gap-2 hover:text-foreground"
            href={REPO}
          >
            <GithubIcon className="size-4" />
            GitHub
          </a>
        </div>
      </footer>
    </div>
  )
}
