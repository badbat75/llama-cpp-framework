// The framework version, read from the bff's /api/version.
import { useEffect, useState } from 'react'
import { call } from './api.ts'
import { t } from './strings.ts'

/** The version from the reply, or null when the reply is not one. */
async function readVersion(): Promise<string | null> {
  try {
    const { ok, body } = await call('/api/version')
    if (!ok) return null
    if (typeof body !== 'object' || body === null) return null
    const version = (body as { version?: unknown }).version
    return typeof version === 'string' && version !== '' ? version : null
  } catch {
    return null
  }
}

export default function Version() {
  // undefined while the reply is awaited, null when it failed.
  const [version, setVersion] = useState<string | null | undefined>(undefined)

  useEffect(() => {
    let live = true
    readVersion().then((v) => {
      if (live) setVersion(v)
    })
    return () => {
      live = false
    }
  }, [])

  if (version === undefined) return null
  return (
    <p>
      {version === null
        ? t('version-unavailable')
        : t('version-label', version)}
    </p>
  )
}
