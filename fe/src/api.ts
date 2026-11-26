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

/**
 * The session this request was made with is gone: 401 `session-required`. The
 * one answer of the contract read by its status and not by its key
 * (fe/briefs/09-signin-only.md, S1b.1).
 */
export function sessionEnded(reply: Reply): boolean {
  return reply.status === 401
}

/** One tensor of the model file's list, as the model-info route answers it
 * (brief 21, 21 T.14): its name and its size in bytes. */
export type ModelTensor = { name: string; bytes: number }

/** The layer facts of one model file, as the model-info route answers them
 * (brief 19 section 0): `blockCount` every block of the file, `moe` whether it
 * has experts, and the two expert counts, 0 when the header carries none. The
 * tensor list rides on the same read (brief 21, 21 T.14): `tensors` is `null`
 * when the reply carries none of that shape, and the layer facts stand. */
export type ModelFacts = {
  blockCount: number
  moe: boolean
  expertCount: number
  expertUsed: number
  /** The file's tensors, in the reply's order; `null` for no tensor list. */
  tensors: ModelTensor[] | null
  /** Whether the manager cut the list (21 T.14): absent, or not a boolean,
   *  reads false (21 T.14a). */
  tensorsTruncated: boolean
}

/** One model-info read: the file's facts, the reason there are none (the key of
 * section 0's four and its args), or `null` for a read that answered nothing. */
export type ModelInfoRead = { facts: ModelFacts } | { reason: ApiError } | null

/** The reason keys of section 0. A reply naming any other is a failed read. */
const MODEL_INFO_REASONS = [
  'model-info-fetched',
  'model-profile-no-model',
  'model-info-missing',
  'model-info-unreadable',
]

/** A whole number, `null` for anything else. */
function whole(value: unknown): number | null {
  return typeof value === 'number' && Number.isInteger(value) ? value : null
}

/** An expert count of 0 or more: any other value reads as 0 (19 G.1). */
function count(value: unknown): number {
  const n = whole(value)
  return n === null || n < 0 ? 0 : n
}

/** The `tensors` of an `info`, read (brief 21, 21 T.14): the `{name, bytes}`
 * rows in the reply's order, `bytes` a whole number of 0 or more. `null` for
 * absent and for anything that is not a list of such rows. The list is a detail
 * of the read: a shape it does not have leaves the layer facts standing, so
 * this never fails the read. */
function modelTensors(value: unknown): ModelTensor[] | null {
  if (!Array.isArray(value)) return null
  const rows: ModelTensor[] = []
  for (const one of value) {
    if (!isRecord(one)) return null
    const bytes = whole(one.bytes)
    if (typeof one.name !== 'string' || bytes === null || bytes < 0) return null
    rows.push({ name: one.name, bytes })
  }
  return rows
}

/** The `info` of a reply, read: `null` for anything that is not the object of
 * section 0. */
function modelFacts(value: unknown): ModelFacts | null {
  if (!isRecord(value)) return null
  const blockCount = whole(value.blockCount)
  if (blockCount === null || blockCount <= 0) return null
  if (typeof value.moe !== 'boolean') return null
  return {
    blockCount,
    moe: value.moe,
    expertCount: count(value.expertCount),
    expertUsed: count(value.expertUsed),
    tensors: modelTensors(value.tensors),
    tensorsTruncated: value.tensorsTruncated === true,
  }
}

/** The `reason` of a reply, read: its key and args when the key is one of the
 * four of section 0, `null` for anything else. */
function modelReason(value: unknown): ApiError | null {
  if (!isRecord(value) || typeof value.key !== 'string') return null
  if (!MODEL_INFO_REASONS.includes(value.key)) return null
  if (!Array.isArray(value.args)) return null
  return { key: value.key, args: value.args.map(String) }
}

/** A model-info body, read (19 G.1): `info` is set exactly when `reason` is
 * not, so a reply with both, or with neither, is a read that answered nothing,
 * as is a body that is no object at all or carries no `model`. */
export function parseModelInfo(body: unknown): ModelInfoRead {
  if (
    !isRecord(body) ||
    !(typeof body.model === 'string' || body.model === null)
  )
    return null
  const facts = body.info === null ? null : modelFacts(body.info)
  const reason = body.reason === null ? null : modelReason(body.reason)
  if ((facts === null) === (reason === null)) return null
  return facts === null ? { reason: reason! } : { facts }
}

/**
 * The layer facts of one model profile's model file (brief 19, 19 G.1):
 * `GET /api/profiles/model/<id>/model-info`, the id encoded as the profile read
 * encodes it, within the GET limit. `null` is a read that answered nothing: no
 * session, no manager, a body that is not of that shape, no answer in time, no
 * network. It is no session-ended answer: a page that cannot read its model
 * file keeps its session (decision 1).
 */
export async function readModelInfo(id: string): Promise<ModelInfoRead> {
  try {
    const reply = await call(
      `/api/profiles/model/${encodeURIComponent(id)}/model-info`,
    )
    return reply.status === 200 ? parseModelInfo(reply.body) : null
  } catch {
    return null
  }
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
