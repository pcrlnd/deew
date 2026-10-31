import { DownloadIcon, HammerIcon, PackageIcon } from "lucide-react"
import type { ReactNode } from "react"

import { LinuxIcon, WindowsIcon } from "@/components/brand-icons"
import { CopyBlock } from "@/components/copy-block"
import { Badge } from "@/components/ui/badge"
import { buttonVariants } from "@/components/ui/button"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { LATEST, type Platform } from "@/lib/platform"

function Panel({
  title,
  desc,
  children,
}: {
  title: string
  desc: string
  children: ReactNode
}) {
  return (
    <div className="border bg-card text-card-foreground">
      <div className="border-b p-4">
        <h3 className="text-base font-semibold">{title}</h3>
        <p className="text-sm text-muted-foreground">{desc}</p>
      </div>
      <div className="flex flex-col gap-4 p-4">{children}</div>
    </div>
  )
}

function DownloadRow({
  title,
  desc,
  file,
  primary,
}: {
  title: string
  desc: string
  file: string
  primary?: boolean
}) {
  return (
    <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
      <div className="min-w-0">
        <p className="text-sm font-medium">{title}</p>
        <p className="text-sm text-muted-foreground">{desc}</p>
      </div>
      <a
        href={LATEST + file}
        className={buttonVariants({
          size: "lg",
          variant: primary ? "default" : "outline",
          className: "w-full shrink-0 sm:w-40",
        })}
      >
        <DownloadIcon />
        {file}
      </a>
    </div>
  )
}

function Note({ children }: { children: ReactNode }) {
  return <p className="text-sm text-muted-foreground">{children}</p>
}

function Code({ children }: { children: ReactNode }) {
  return (
    <code className="bg-muted px-1 py-0.5 font-mono text-[0.85em] text-foreground">
      {children}
    </code>
  )
}

export function GetStarted({ platform }: { platform: Platform }) {
  return (
    <Tabs defaultValue={platform === "linux" ? "linux" : "windows"}>
      <TabsList
        aria-label="Platform"
        className="h-auto! w-full max-w-full sm:w-fit"
      >
        <TabsTrigger value="windows" className="h-8 px-3">
          <WindowsIcon className="size-3.5" /> Windows
        </TabsTrigger>
        <TabsTrigger value="linux" className="h-8 px-3">
          <LinuxIcon className="size-4" /> Linux
        </TabsTrigger>
        <TabsTrigger value="source" className="h-8 px-3">
          <HammerIcon /> <span className="sm:hidden">Source</span>
          <span className="hidden sm:inline">Build from source</span>
        </TabsTrigger>
      </TabsList>

      <TabsContent value="windows">
        <Panel
          title="Windows 10 / 11"
          desc="Download the builds, no installer needed."
        >
          <DownloadRow
            primary
            title="CLI"
            desc="The command line tool."
            file="deew.exe"
          />
          <DownloadRow
            title="GUI"
            desc="The desktop app. Put it in the same folder as deew.exe."
            file="deew-gui.exe"
          />
          <DownloadRow
            title="Web UI"
            desc="The GUI in your browser, e.g. for a server. Put it in the same folder as deew.exe."
            file="deew-web.exe"
          />
          <div
            aria-disabled="true"
            className="flex items-center justify-between gap-3 border border-dashed p-3 opacity-70"
          >
            <div className="flex min-w-0 items-center gap-3">
              <PackageIcon className="size-5 shrink-0 text-muted-foreground" />
              <div className="min-w-0">
                <p className="text-sm font-medium">scoop / choco / winget</p>
                <p className="text-sm text-muted-foreground">
                  Package manager installs are not available yet.
                </p>
              </div>
            </div>
            <Badge variant="secondary">WIP</Badge>
          </div>
        </Panel>
      </TabsContent>

      <TabsContent value="linux">
        <Panel title="Linux" desc="Standalone binaries for x86-64 Linux.">
          <DownloadRow
            primary
            title="CLI"
            desc="The command line tool."
            file="deew"
          />
          <DownloadRow
            title="GUI"
            desc="The desktop app. Put it in the same folder as deew."
            file="deew-gui"
          />
          <DownloadRow
            title="Web UI"
            desc="The GUI in your browser, no WebKit needed (e.g. for a server or NAS). Put it in the same folder as deew."
            file="deew-web"
          />
          <Note>Make the downloads executable:</Note>
          <CopyBlock
            text="chmod +x deew deew-gui deew-web"
            label="chmod command"
          />
          <Note>
            The GUI needs the WebKitGTK runtime:{" "}
            <Code>libwebkit2gtk-4.1-0</Code> on Debian/Ubuntu,{" "}
            <Code>webkit2gtk4.1</Code> on Fedora, <Code>webkit2gtk-4.1</Code> on
            Arch.
          </Note>
        </Panel>
      </TabsContent>

      <TabsContent value="source">
        <Panel
          title="Build from source"
          desc="Works on Windows, Linux and macOS with a Rust toolchain."
        >
          <CopyBlock text="cargo install deew" label="cargo install command" />
        </Panel>
      </TabsContent>
    </Tabs>
  )
}
