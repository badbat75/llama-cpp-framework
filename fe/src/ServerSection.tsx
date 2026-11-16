// The Server section of an instance page (fe/briefs/01-server.md, sections
// 2.5 and 3): the server file read from /api/instances/{id}/server, shown as
// the form, or the message saying why not.
import { useEffect, useState } from 'react'
import { apiError, call, isRecord, sessionRevoked } from './api.ts'
import ServerForm, {
  type Runs,
  type ServerDoc,
  withholdsKey,
} from './ServerForm.tsx'
import type { Session } from './session.ts'
import { t } from './strings.ts'

/** What the read gave: the document, or the error key of its message. */
type Answer =
  | { doc: ServerDoc }
  | {
      error:
        | 'client-not-authorized'
        | 'client-unavailable'
        | 'client-no-instance'
        | 'failed'
    }
  | 'signed-out'
  | 'revoked'

function isServerDoc(value: unknown): value is ServerDoc {
  if (!isRecord(value)) return false
  const { values, defaults, interfaces } = value
  return (
    isRecord(values) &&
    isRecord(defaults) &&
    Array.isArray(interfaces) &&
    interfaces.every(
      (i) =>
        isRecord(i) &&
        typeof i.value === 'string' &&
        typeof i.label === 'string',
    )
  )
}

async function readServer(id: string): Promise<Answer> {
  try {
    const reply = await call(`/api/instances/${encodeURIComponent(id)}/server`)
    if (reply.status === 401) return 'signed-out'
    if (sessionRevoked(reply)) return 'revoked'
    if (!reply.ok) {
      const key = apiError(reply.body)?.key
      switch (key) {
        case 'client-not-authorized':
        case 'client-unavailable':
        case 'client-no-instance':
          return { error: key }
        default:
          return { error: 'failed' }
      }
    }
    return isServerDoc(reply.body) ? { doc: reply.body } : { error: 'failed' }
  } catch {
    return { error: 'failed' }
  }
}

/**
 * The page remounts it (a new `key`) to read the file again after a sign-in
 * as another session.
 */
export default function ServerSection({
  id,
  dropped,
  renewals,
  runs,
  onSignedOut,
  onRenewed,
  onDirty,
}: {
  id: string
  /** The edits were dropped by a sign-in as another session (36b). */
  dropped: boolean
  /** Raised by every sign-in that kept the page (43b). */
  renewals: number
  runs: Runs
  onSignedOut: (byManager: boolean) => void
  /** A sign-in in the save's dialog: true when the edits stay with it. */
  onRenewed: (session: Session) => boolean
  onDirty: (dirty: boolean) => void
}) {
  // undefined while the reply is awaited.
  const [answer, setAnswer] = useState<Answer | undefined>(undefined)
  // Raised to read the file again.
  const [reads, setReads] = useState(0)

  const reload = () => {
    setAnswer(undefined)
    setReads((n) => n + 1)
  }

  useEffect(() => {
    let live = true
    readServer(id).then((a) => {
      if (!live) return
      if (a === 'signed-out' || a === 'revoked') onSignedOut(a === 'revoked')
      else setAnswer(a)
    })
    return () => {
      live = false
    }
  }, [id, onSignedOut, reads])

  return (
    <section>
      <h2>{t('server-title')}</h2>
      <p className="intro">{t('server-intro', `server@${id}.ini`)}</p>
      {dropped && (
        <p role="alert" className="alert">
          {t('server-edits-dropped')}
        </p>
      )}
      <Body
        id={id}
        answer={answer}
        renewals={renewals}
        runs={runs}
        onDirty={onDirty}
        onReload={reload}
        onRenewed={onRenewed}
      />
    </section>
  )
}

function Body({
  id,
  answer,
  renewals,
  runs,
  onDirty,
  onReload,
  onRenewed,
}: {
  id: string
  answer: Answer | undefined
  renewals: number
  runs: Runs
  onDirty: (dirty: boolean) => void
  onReload: () => void
  onRenewed: (session: Session) => boolean
}) {
  if (answer === undefined || answer === 'signed-out' || answer === 'revoked') {
    return <p role="status">{t('server-loading')}</p>
  }
  if ('doc' in answer) {
    // A withheld key (27d): the form would show No key, and a save would
    // erase the key, so the whole read counts as failed.
    if (withholdsKey(answer.doc)) {
      return <p>{t('server-load-failed')}</p>
    }
    return (
      <ServerForm
        id={id}
        doc={answer.doc}
        renewals={renewals}
        runs={runs}
        onDirty={onDirty}
        onReload={onReload}
        onRenewed={onRenewed}
      />
    )
  }
  if (answer.error === 'client-no-instance') {
    return <InstanceNotFound id={id} />
  }
  return <p>{errorText(answer.error)}</p>
}

/** The message of a read error, by its key. */
function errorText(key: string): string {
  switch (key) {
    case 'client-not-authorized':
      return t('api-client-not-authorized')
    case 'client-unavailable':
      return t('api-client-unavailable')
    default:
      return t('server-load-failed')
  }
}

/** No instance by that id: say so and lead back to the Overview. */
export function InstanceNotFound({ id }: { id: string }) {
  return (
    <>
      <p>{t('instance-not-found', id)}</p>
      <p>
        <a href="#/">{t('not-found-home')}</a>
      </p>
    </>
  )
}
