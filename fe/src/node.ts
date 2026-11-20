// The host and what is installed on it, read from the bff's GET /api/node
// (fe/briefs/13-node-about.md, section 0): the shell reads it once per session
// for the labels, the About page reads it again at every arrival.
import { call, isRecord, sessionEnded } from './api.ts'

/** One GPU backend package: its suffix, and its version, null when unknown. */
export type NodeBackend = { name: string; version: string | null }

/** One installed engine: its id, its package version, its backend packages. */
export type NodeEngine = {
  engine: string
  version: string | null
  backends: NodeBackend[]
}

/** The host: its name, the framework's version, the installed engines. */
export type NodeInfo = {
  name: string
  version: string | null
  engines: NodeEngine[]
}

/** A version as the reply sends it: the string, null for anything else (the
 * bff sends null for an empty version file). */
function versionIn(value: unknown): string | null {
  return typeof value === 'string' && value !== '' ? value : null
}

/** The backend items with a name, in the reply's order; the rest are left out
 * (13 N.10). A missing list is the empty one: a CPU-only engine. */
function backendsIn(value: unknown): NodeBackend[] {
  if (!Array.isArray(value)) return []
  const backends: NodeBackend[] = []
  for (const item of value) {
    if (!isRecord(item)) continue
    const name = item.name
    if (typeof name !== 'string' || name === '') continue
    backends.push({ name, version: versionIn(item.version) })
  }
  return backends
}

/** The engine items with a name, in the reply's order; the rest are left out.
 * null when the reply has no engine list at all: a malformed reply (13 N.10). */
function enginesIn(value: unknown): NodeEngine[] | null {
  if (!Array.isArray(value)) return null
  const engines: NodeEngine[] = []
  for (const item of value) {
    if (!isRecord(item)) continue
    const engine = item.engine
    if (typeof engine !== 'string' || engine === '') continue
    engines.push({
      engine,
      version: versionIn(item.version),
      backends: backendsIn(item.backends),
    })
  }
  return engines
}

/**
 * The host, from the reply: `node` null when the reply says nothing to show
 * (an error, a body that is not an object, a name that is not a non-empty
 * string, engines that is not an array), `ended` when it answered 401
 * (fe/briefs/09-signin-only.md, S1b.1). The page decides what either means:
 * the shell's read shows the fallback words and signs nobody out (13 N.3).
 */
export async function readNode(): Promise<{
  node: NodeInfo | null
  ended: boolean
}> {
  try {
    const reply = await call('/api/node')
    if (sessionEnded(reply)) return { node: null, ended: true }
    if (!reply.ok || !isRecord(reply.body)) return { node: null, ended: false }
    const name = reply.body.name
    const engines = enginesIn(reply.body.engines)
    // A name with nothing in it, spaces included, is no name: the shell keeps
    // its fallback words (13 N.5).
    if (typeof name !== 'string' || name.trim() === '' || engines === null)
      return { node: null, ended: false }
    return {
      node: { name, version: versionIn(reply.body.version), engines },
      ended: false,
    }
  } catch {
    return { node: null, ended: false }
  }
}
