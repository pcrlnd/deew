import { useId, useState } from "react"
import { Loader2 } from "lucide-react"

import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { SimpleSelect } from "@/components/simple-select"
import { errorMessage, login, type Remember } from "@/lib/web-api"

const REMEMBER_KEY = "deew-web-remember"
const REMEMBER_DEFAULT: Remember = "7d"
const REMEMBER_CHOICES: { value: Remember; label: string }[] = [
  { value: "session", label: "Until the browser is closed" },
  { value: "1d", label: "1 day" },
  { value: "7d", label: "7 days" },
  { value: "30d", label: "30 days" },
]

function loadRemember(): Remember {
  try {
    const v = localStorage.getItem(REMEMBER_KEY)
    return (
      REMEMBER_CHOICES.find((c) => c.value === v)?.value ?? REMEMBER_DEFAULT
    )
  } catch {
    return REMEMBER_DEFAULT
  }
}

function saveRemember(value: Remember) {
  try {
    localStorage.setItem(REMEMBER_KEY, value)
  } catch {
    // Storage unavailable: the choice just is not remembered.
  }
}

/** Centered login form of the web mode. */
export function LoginCard({ onLoggedIn }: { onLoggedIn: () => void }) {
  const uid = useId()
  const [user, setUser] = useState("")
  const [pass, setPass] = useState("")
  const [remember, setRemember] = useState<Remember>(loadRemember)
  const [error, setError] = useState<string | null>(null)
  const [submitting, setSubmitting] = useState(false)

  const submit = async (event: React.FormEvent) => {
    event.preventDefault()
    setSubmitting(true)
    setError(null)
    try {
      await login(user, pass, remember)
      setPass("")
      onLoggedIn()
    } catch (e) {
      setError(errorMessage(e))
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <div className="flex h-full min-h-[560px] items-center justify-center p-4">
      <form
        onSubmit={submit}
        className="flex w-full max-w-xs flex-col gap-3 rounded-xl border border-border bg-card p-6"
      >
        <div className="flex items-center justify-center gap-2 pb-2">
          <img src="/icon.png" alt="" className="size-8" />
          <span className="text-xl font-semibold">deew</span>
        </div>
        <div className="flex flex-col gap-1">
          <Label htmlFor={`${uid}-user`}>Username</Label>
          <Input
            id={`${uid}-user`}
            autoFocus
            autoComplete="username"
            value={user}
            onChange={(e) => setUser(e.target.value)}
            disabled={submitting}
          />
        </div>
        <div className="flex flex-col gap-1">
          <Label htmlFor={`${uid}-pass`}>Password</Label>
          <Input
            id={`${uid}-pass`}
            type="password"
            autoComplete="current-password"
            value={pass}
            onChange={(e) => setPass(e.target.value)}
            disabled={submitting}
          />
        </div>
        <div className="flex flex-col gap-1">
          <Label htmlFor={`${uid}-remember`}>Keep me logged in</Label>
          <SimpleSelect
            id={`${uid}-remember`}
            value={remember}
            onChange={(v) => {
              const value = v as Remember
              setRemember(value)
              saveRemember(value)
            }}
            choices={REMEMBER_CHOICES}
            disabled={submitting}
          />
          <p className="text-xs text-muted-foreground">
            Restarting deew-web logs everyone out.
          </p>
        </div>
        {error && <p className="text-xs text-destructive">{error}</p>}
        <Button type="submit" disabled={submitting}>
          {submitting && (
            <Loader2 data-icon="inline-start" className="animate-spin" />
          )}
          Log in
        </Button>
      </form>
    </div>
  )
}
