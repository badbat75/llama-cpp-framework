// The machine the web service runs on, read from the bff's GET /api/version
// (fe/briefs/18-about-node-info.md, section 0): the shell takes the host name
// from this one read per session (18 AN.15), and the About page its two lines
// (18 AN.2). The route answers with no manager, so it is never 503; a fact the
// bff does not have is null.
import { call, isRecord, sessionEnded, sessionRevoked } from './api.ts'
import { osIn, textIn, type NodeOs } from './node.ts'

/** What the reply answers: the host name, the framework's version, the
 * operating system. */
export type VersionInfo = {
  name: string | null
  version: string | null
  os: NodeOs | null
}

/**
 * The reply read into `info`: null when the reply says nothing to show (an
 * error, a body that is not an object, a network failure), `ended` when it
 * answered 401, the session being gone, `revoked` of the 403 the manager
 * answers when it refuses its user (the page says which: 09 S1b.1). The name
 * is null when the reply has none: the shell keeps its fallback words and
 * signs nobody out (13 N.5, 18 AN.15), which is the caller's call.
 */
export async function readVersionInfo(): Promise<{
  info: VersionInfo | null
  ended: boolean
  revoked: boolean
}> {
  try {
    const reply = await call('/api/version')
    const revoked = sessionRevoked(reply)
    if (sessionEnded(reply) || revoked)
      return { info: null, ended: true, revoked }
    if (!reply.ok || !isRecord(reply.body))
      return { info: null, ended: false, revoked: false }
    return {
      info: {
        name: textIn(reply.body.name),
        version: textIn(reply.body.version),
        os: osIn(reply.body.os),
      },
      ended: false,
      revoked: false,
    }
  } catch {
    return { info: null, ended: false, revoked: false }
  }
}
