import { useEffect, useRef, useState } from "react"
import { CheckIcon, CopyIcon } from "lucide-react"

import { Button } from "@/components/ui/button"
import { copyText } from "@/lib/clipboard"
import { cn } from "@/lib/utils"

export function CopyBlock({
  text,
  label,
  wrap = false,
}: {
  text: string
  label: string
  wrap?: boolean
}) {
  const [copied, setCopied] = useState(false)
  const timer = useRef<number | undefined>(undefined)

  useEffect(() => () => window.clearTimeout(timer.current), [])

  function onCopy() {
    copyText(text).then(
      () => {
        setCopied(true)
        window.clearTimeout(timer.current)
        timer.current = window.setTimeout(() => setCopied(false), 1800)
      },
      () => {
        /* copying is not possible here; ignore */
      }
    )
  }

  return (
    <div
      className={cn(
        "flex gap-2 border bg-muted/50 py-1 pr-1 pl-3",
        wrap ? "items-start" : "items-center"
      )}
    >
      <pre
        className={cn(
          "min-w-0 flex-1 py-1 font-mono text-sm",
          wrap ? "break-all whitespace-pre-wrap" : "overflow-x-auto"
        )}
      >
        <code>{text}</code>
      </pre>
      <Button
        variant="ghost"
        size="icon"
        onClick={onCopy}
        aria-label={copied ? "Copied" : `Copy ${label}`}
        title="Copy"
      >
        {copied ? (
          <CheckIcon className="text-primary dark:text-blue-400" />
        ) : (
          <CopyIcon />
        )}
      </Button>
      <span className="sr-only" role="status" aria-live="polite">
        {copied ? "Copied to clipboard" : ""}
      </span>
    </div>
  )
}
