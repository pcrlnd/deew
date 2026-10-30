import { useRef, useState } from "react"

import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"

interface PromptDialogProps {
  /** The question; null while there is none (dialog closed). */
  question: string | null
  onAnswer: (yes: boolean) => void
}

/** Modal y/n question of the CLI. Escape / closing answers "No". */
export function PromptDialog({ question, onAnswer }: PromptDialogProps) {
  const yes = useRef<HTMLButtonElement>(null)
  // Keeps the text while the dialog fades out.
  const [shown, setShown] = useState("")
  if (question !== null && question !== shown) setShown(question)

  return (
    <Dialog
      open={question !== null}
      onOpenChange={(open) => {
        if (!open) onAnswer(false)
      }}
    >
      <DialogContent showCloseButton={false} initialFocus={yes}>
        <DialogHeader>
          <DialogTitle>deew asks</DialogTitle>
        </DialogHeader>
        <p className="text-sm break-words whitespace-pre-wrap">{shown}</p>
        <DialogFooter>
          <Button variant="outline" onClick={() => onAnswer(false)}>
            No
          </Button>
          <Button ref={yes} onClick={() => onAnswer(true)}>
            Yes
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
