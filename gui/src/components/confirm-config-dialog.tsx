import { useRef } from "react"

import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"

interface ConfirmConfigDialogProps {
  open: boolean
  onConfirm: () => void
  onCancel: () => void
}

/** "Generate config" overwrites config.toml, so it is never run unasked. */
export function ConfirmConfigDialog({
  open,
  onConfirm,
  onCancel,
}: ConfirmConfigDialogProps) {
  const cancel = useRef<HTMLButtonElement>(null)
  return (
    <Dialog open={open} onOpenChange={(next) => !next && onCancel()}>
      <DialogContent showCloseButton={false} initialFocus={cancel}>
        <DialogHeader>
          <DialogTitle>Generate a new config?</DialogTitle>
          <DialogDescription>
            This overwrites config.toml with the defaults. Your own changes are
            lost, including the [gui] settings, the default bitrates and the
            deew-web login.
          </DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <Button ref={cancel} variant="outline" onClick={onCancel}>
            Cancel
          </Button>
          <Button variant="destructive" onClick={onConfirm}>
            Generate config
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
