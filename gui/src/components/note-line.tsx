import { CircleX, TriangleAlert } from "lucide-react"

import { cn } from "@/lib/utils"

interface NoteLineProps {
  tone: "warning" | "error"
  text: string
  title?: string
  /** One line with an ellipsis instead of wrapping. */
  truncate?: boolean
}

/** A warning (amber) or error (red) line with an icon. */
export function NoteLine({
  tone,
  text,
  title,
  truncate = false,
}: NoteLineProps) {
  const Icon = tone === "error" ? CircleX : TriangleAlert
  return (
    <p
      title={title ?? text}
      className={cn(
        "flex gap-1 text-xs",
        tone === "error" ? "text-destructive" : "text-amber-500",
        truncate
          ? "items-center truncate whitespace-nowrap"
          : "items-start break-words"
      )}
    >
      <Icon className={cn("size-3 shrink-0", !truncate && "mt-[3px]")} />
      <span className={cn("min-w-0", truncate && "truncate")}>{text}</span>
    </p>
  )
}
