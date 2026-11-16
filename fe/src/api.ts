// Calls to the bff and the error shape every route shares
// (fe/briefs/api-contract.md): {"key", "args"} with a non-2xx status.

/** A reply: its status and its JSON body, undefined when the body is not JSON. */
export type Reply = { status: number; ok: boolean; body: unknown }

/** An error reply's strings key and the values for its placeholders. */
export type ApiError = { key: string; args: string[] }

/** How long a GET and any other method may take, body included (4.8, 47). */
const GET_LIMIT_MS = 15_000
const OTHER_LIMIT_MS = 120_000

/**
 * Calls `path`; rejects when the service cannot be reached, or with a
 * `DOMException` named `TimeoutError` when it has not answered, body
 * included, within the method's limit.
 */
export async function call(path: string, init?: RequestInit): Promise<Reply> {
  const method = (init?.method ?? 'GET').toUpperCase()
  const controller = new AbortController()
  const timer = setTimeout(
    () => controller.abort(new DOMException('timed out', 'TimeoutError')),
    method === 'GET' ? GET_LIMIT_MS : OTHER_LIMIT_MS,
  )
  try {
    const reply = await fetch(path, { ...init, signal: controller.signal })
    const text = await reply.text()
    let body: unknown
    try {
      body = JSON.parse(text)
    } catch {
      body = undefined
    }
    return { status: reply.status, ok: reply.ok, body }
  } finally {
    clearTimeout(timer)
  }
}

export function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}

/**
 * The bff ended this session: 403 with `session-not-administrator`, the key it
 * answers when the manager refuses the session's user (it left the group pai).
 * Every route but POST /api/session sends it for that, where the same key keeps
 * its sign-in meaning (the form shows api-session-not-administrator).
 */
export function sessionRevoked(reply: Reply): boolean {
  return (
    reply.status === 403 &&
    apiError(reply.body)?.key === 'session-not-administrator'
  )
}

/** The key and args of an error body, or null when the body is not one. */
export function apiError(body: unknown): ApiError | null {
  if (typeof body !== 'object' || body === null) return null
  const { key, args } = body as { key?: unknown; args?: unknown }
  if (typeof key !== 'string' || key === '') return null
  if (args === undefined) return { key, args: [] }
  if (!Array.isArray(args)) return null
  return { key, args: args.map(String) }
}
