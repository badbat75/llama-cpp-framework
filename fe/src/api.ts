// Calls to the bff and the error shape every route shares
// (fe/briefs/api-contract.md): {"key", "args"} with a non-2xx status.

/** A reply: its status and its JSON body, undefined when the body is not JSON. */
export type Reply = { status: number; ok: boolean; body: unknown }

/** An error reply's strings key and the values for its placeholders. */
export type ApiError = { key: string; args: string[] }

/** Calls `path`; rejects only when the service cannot be reached. */
export async function call(path: string, init?: RequestInit): Promise<Reply> {
  const reply = await fetch(path, init)
  let body: unknown
  try {
    body = await reply.json()
  } catch {
    body = undefined
  }
  return { status: reply.status, ok: reply.ok, body }
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
