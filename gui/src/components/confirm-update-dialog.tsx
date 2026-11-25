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

interface ConfirmUpdateDialogProps {
  /** `null` when closed; `all` also installs the missing programs. */
  request: { all: boolean } | null
  onConfirm: () => void
  onCancel: () => void
}

/** "Update" replaces program files, so it is never run unasked. */
export function ConfirmUpdateDialog({
  request,
  onConfirm,
  onCancel,
}: ConfirmUpdateDialogProps) {
  const cancel = useRef<HTMLButtonElement>(null)
  const all = request?.all ?? false
  return (
    <Dialog
      open={request !== null}
      onOpenChange={(next) => !next && onCancel()}
    >
      <DialogContent showCloseButton={false} initialFocus={cancel}>
        <DialogHeader>
          <DialogTitle>
            {all ? "Update and install all?" : "Update deew?"}
          </DialogTitle>
          <DialogDescription>
            This downloads the latest release from GitHub and replaces deew (and
            deew-gui / deew-web next to it).
            {all ? " The missing ones are installed too." : ""} After updating
            deew-gui or deew-web, restart them to use the new version.
          </DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <Button ref={cancel} variant="outline" onClick={onCancel}>
            Cancel
          </Button>
          <Button onClick={onConfirm}>
            {all ? "Update and install all" : "Update"}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
