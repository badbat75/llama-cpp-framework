// The framework version, read from the bff's /api/version.
import { useEffect, useState } from 'react'
import { call, sessionEnded } from './api.ts'
import type { SignedOut } from './session.ts'
import { t } from './strings.ts'

/**
 * The version from the reply, null when the reply is not one, and whether the
 * reply ended the session (09 S1b.1).
 */
async function readVersion(): Promise<{
  version: string | null
  ended: boolean
}> {
  try {
    const reply = await call('/api/version')
    if (!reply.ok) return { version: null, ended: sessionEnded(reply) }
    const { body } = reply
    if (typeof body !== 'object' || body === null)
      return { version: null, ended: false }
    const version = (body as { version?: unknown }).version
    return typeof version === 'string' && version !== ''
      ? { version, ended: false }
      : { version: null, ended: false }
  } catch {
    return { version: null, ended: false }
  }
}

export default function Version({
  onSignedOut,
}: {
  /** Raised when the read answers 401, the session being gone (09 S1b.1). */
  onSignedOut: (reason: SignedOut) => void
}) {
  // undefined while the reply is awaited, null when it failed.
  const [version, setVersion] = useState<string | null | undefined>(undefined)

  useEffect(() => {
    let live = true
    readVersion().then((r) => {
      if (!live) return
      if (r.ended) onSignedOut('ended')
      else setVersion(r.version)
    })
    return () => {
      live = false
    }
  }, [onSignedOut])

  if (version === undefined) return null
  return (
    <p>
      {version === null
        ? t('version-unavailable')
        : t('version-label', version)}
    </p>
  )
}
