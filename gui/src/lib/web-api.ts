// HTTP side of the web mode: the JSON API of deew-web (gui/web).

export interface SessionInfo {
  authRequired: boolean
  authenticated: boolean
}

type FetchFn = typeof fetch

/** Message of a thrown value (`String(error)` would prefix "Error: "). */
export const errorMessage = (error: unknown): string =>
  error instanceof Error ? error.message : String(error)

const unauthorizedListeners = new Set<() => void>()

/** Called whenever the server answers 401; returns the unsubscribe function. */
export function onUnauthorized(listener: () => void): () => void {
  unauthorizedListeners.add(listener)
  return () => unauthorizedListeners.delete(listener)
}

export const notifyUnauthorized = (): void =>
  unauthorizedListeners.forEach((listener) => listener())

/** The server's `error` message of a failed response, else a generic one. */
async function failureMessage(response: Response): Promise<string> {
  try {
    const body: unknown = await response.json()
    if (body && typeof body === "object" && "error" in body) {
      const message = (body as { error: unknown }).error
      if (typeof message === "string" && message) return message
    }
  } catch {
    // Not JSON (e.g. a proxy error page).
  }
  return `Request failed (HTTP ${response.status})`
}

/**
 * One JSON request. A non-2xx answer throws an `Error` with the server's
 * message; a 401 additionally notifies `onUnauthorized` listeners unless
 * `notify401` is false (the login request itself).
 */
export async function requestJson<T>(
  path: string,
  options: { method?: string; body?: unknown; notify401?: boolean } = {},
  fetchFn: FetchFn = (...args) => fetch(...args),
  notify: () => void = notifyUnauthorized
): Promise<T> {
  const { method = "GET", body, notify401 = true } = options
  const response = await fetchFn(path, {
    method,
    credentials: "same-origin",
    headers:
      body === undefined ? undefined : { "Content-Type": "application/json" },
    body: body === undefined ? undefined : JSON.stringify(body),
  })
  if (!response.ok) {
    if (response.status === 401 && notify401) notify()
    throw new Error(await failureMessage(response))
  }
  if (response.status === 204) return undefined as T
  return (await response.json()) as T
}

export const getSession = (): Promise<SessionInfo> =>
  requestJson("/api/session", { notify401: false })

export type Remember = "session" | "1d" | "7d" | "30d"

export const login = (
  user: string,
  pass: string,
  remember?: Remember
): Promise<void> =>
  requestJson("/api/login", {
    method: "POST",
    body: remember === undefined ? { user, pass } : { user, pass, remember },
    notify401: false,
  })

export const logout = (): Promise<void> =>
  requestJson("/api/logout", { method: "POST", notify401: false })

/** Tells the app to show the login screen if the session is gone. */
export async function checkSession(): Promise<void> {
  try {
    const session = await getSession()
    if (session.authRequired && !session.authenticated) notifyUnauthorized()
  } catch {
    // Server unreachable: nothing to conclude.
  }
}
