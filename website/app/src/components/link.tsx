import { navigate } from "@/lib/router"

// An internal link: a normal <a href> (new tab, middle click and copy link
// work) that navigates without a page load on a plain left click.
export function Link({
  href,
  onClick,
  ...props
}: React.ComponentProps<"a"> & { href: string }) {
  return (
    <a
      {...props}
      href={href}
      onClick={(e) => {
        onClick?.(e)
        if (
          e.defaultPrevented ||
          e.button !== 0 ||
          e.metaKey ||
          e.ctrlKey ||
          e.shiftKey ||
          e.altKey ||
          (props.target && props.target !== "_self")
        )
          return
        const url = new URL(href, location.href)
        if (url.origin !== location.origin) return
        // Only the hash changes: let the browser jump in the page.
        if (
          url.hash &&
          url.pathname === location.pathname &&
          url.search === location.search
        )
          return
        e.preventDefault()
        navigate(url.pathname + url.search + url.hash)
      }}
    />
  )
}

// A link to another site: opens in a new tab.
export function ExternalLink(props: React.ComponentProps<"a">) {
  return <a target="_blank" rel="noopener noreferrer" {...props} />
}
