/** Copies text to the clipboard, rejecting if that fails. */
export async function copyText(text: string): Promise<void> {
  // The Clipboard API needs a secure context, and deew-web is often opened
  // over plain http on a LAN, so fall back to execCommand (run before any
  // await to keep the click's user activation).
  if (window.isSecureContext && navigator.clipboard?.writeText) {
    try {
      await navigator.clipboard.writeText(text)
      return
    } catch {
      // fall through to the fallback
    }
  }
  execCopy(text)
}

function execCopy(text: string): void {
  const el = document.createElement("textarea")
  el.value = text
  el.readOnly = true
  const style = el.style
  style.position = "fixed"
  style.top = "0"
  style.left = "0"
  style.width = "1px"
  style.height = "1px"
  style.padding = "0"
  style.border = "0"
  style.opacity = "0"
  style.fontSize = "16px" // avoids iOS zoom-on-focus
  const active = document.activeElement
  document.body.appendChild(el)
  let ok = false
  try {
    el.select()
    el.setSelectionRange(0, text.length)
    ok = document.execCommand("copy")
  } catch {
    // ok stays false
  } finally {
    el.remove()
    if (active instanceof HTMLElement) active.focus({ preventScroll: true })
  }
  if (!ok) throw new Error("copy failed")
}
