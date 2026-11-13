import { GithubIcon, TelegramIcon } from "@/components/brand-icons"
import { Link } from "@/components/link"
import { cn } from "@/lib/utils"
import { HOME, REPO, TELEGRAM, TOOL_CHAIN } from "@/lib/platform"

export type SitePage = "home" | "tool-chain"

function NavLink({
  href,
  active,
  internal,
  children,
}: {
  href: string
  active?: boolean
  internal?: boolean
  children: React.ReactNode
}) {
  const Anchor = internal ? Link : "a"
  return (
    <Anchor
      href={href}
      aria-current={active ? "page" : undefined}
      className={cn(
        "inline-flex items-center gap-2 px-2 py-1 text-sm text-muted-foreground hover:text-foreground",
        active && "bg-muted text-foreground"
      )}
    >
      {children}
    </Anchor>
  )
}

export function SiteHeader({ current }: { current: SitePage }) {
  return (
    <nav
      aria-label="Site"
      className="sticky top-0 z-50 border-b bg-background/85 backdrop-blur"
    >
      <div className="mx-auto flex max-w-6xl items-center justify-between gap-4 px-4 py-3 sm:px-6">
        <Link href={HOME} className="inline-flex items-center gap-2">
          <img
            src="logo.svg"
            alt=""
            width={640}
            height={640}
            className="size-6 shrink-0"
          />
          <span className="text-base font-semibold tracking-tight">deew</span>
        </Link>
        <div className="flex items-center gap-1">
          <NavLink internal href={HOME} active={current === "home"}>
            Home
          </NavLink>
          <NavLink internal href={TOOL_CHAIN} active={current === "tool-chain"}>
            Tool chain
          </NavLink>
          <NavLink href={REPO}>
            <GithubIcon className="size-4" />
            GitHub
          </NavLink>
        </div>
      </div>
    </nav>
  )
}

export function SiteFooter() {
  return (
    <footer className="border-t">
      <div className="mx-auto flex max-w-6xl items-center justify-between gap-4 px-4 py-6 text-sm text-muted-foreground sm:px-6">
        <span>deew - Dolby Encoding Engine Wrapper</span>
        <a
          className="inline-flex items-center gap-2 hover:text-foreground"
          href={TELEGRAM}
        >
          <TelegramIcon className="size-4" />
          Telegram support
        </a>
      </div>
    </footer>
  )
}
