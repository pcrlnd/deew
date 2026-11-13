import { useLayoutEffect } from "react"

import App from "./App.tsx"
import ToolChain from "./ToolChain.tsx"
import { SiteFooter, SiteHeader } from "@/components/site-shell"
import { applyHead } from "@/lib/pages"
import { applyScroll, consumeScrollTarget, useRoute } from "@/lib/router"

// Both pages are imported eagerly so switching never shows a loading state.
export default function Site() {
  const route = useRoute()

  useLayoutEffect(() => {
    applyHead(route)
    applyScroll(consumeScrollTarget())
  })

  return (
    <div className="min-h-screen">
      <SiteHeader current={route} />
      {route === "tool-chain" ? <ToolChain /> : <App />}
      <SiteFooter />
    </div>
  )
}
