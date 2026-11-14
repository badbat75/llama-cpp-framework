// The signed-in user, read and changed through the bff's /api/session.
import { apiError, call, type ApiError } from './api.ts'

export type Session = { user: string; canConfigure: boolean }

function isSession(value: unknown): value is Session {
  if (typeof value !== 'object' || value === null) return false
  const { user, canConfigure } = value as {
    user?: unknown
    canConfigure?: unknown
  }
  return typeof user === 'string' && typeof canConfigure === 'boolean'
}

/** The session, or null when signed out or when it cannot be read. */
export async function readSession(): Promise<Session | null> {
  try {
    const reply = await call('/api/session')
    return reply.ok && isSession(reply.body) ? reply.body : null
  } catch {
    return null
  }
}

/**
 * Signs in: the session, or the error the bff answered (null when the reply
 * carried none, or the service could not be reached).
 */
export async function signIn(
  username: string,
  password: string,
): Promise<Session | { error: ApiError | null }> {
  try {
    const reply = await call('/api/session', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ username, password }),
    })
    if (reply.ok) return isSession(reply.body) ? reply.body : { error: null }
    return { error: apiError(reply.body) }
  } catch {
    return { error: null }
  }
}

/** Signs out; any answer, a failure included, leaves the page signed out. */
export async function signOut(): Promise<void> {
  try {
    await call('/api/session', { method: 'DELETE' })
  } catch {
    // The cookie may already be gone: signed out either way.
  }
}
