import { useEffect, useRef, useState } from "react"
import { CheckIcon, CopyIcon } from "lucide-react"

import { Button } from "@/components/ui/button"
import { copyText } from "@/lib/clipboard"

export function CopyBlock({ text, label }: { text: string; label: string }) {
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
    <div className="flex items-center gap-2 border bg-muted/50 py-1 pr-1 pl-3">
      <pre className="min-w-0 flex-1 overflow-x-auto py-1 font-mono text-sm">
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
