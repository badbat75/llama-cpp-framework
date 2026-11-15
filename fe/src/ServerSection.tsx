// The Server section of an instance page (fe/briefs/01-server.md, sections
// 2.5 and 3): the server file read from /api/instances/{id}/server, shown as
// the form, or the message saying why not.
import { useEffect, useState } from 'react'
import { apiError, call, isRecord } from './api.ts'
import ServerForm, { type ServerDoc } from './ServerForm.tsx'
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

export default function ServerSection({
  id,
  canConfigure,
  onSignedOut,
  onRenewed,
  onDirty,
}: {
  id: string
  canConfigure: boolean
  onSignedOut: () => void
  /** A sign-in in the save's dialog: true when the edits stay with it. */
  onRenewed: (session: Session) => boolean
  onDirty: (dirty: boolean) => void
}) {
  // undefined while the reply is awaited.
  const [answer, setAnswer] = useState<Answer | undefined>(undefined)
  // Raised to read the file again.
  const [reads, setReads] = useState(0)
  // The edits were dropped by a sign-in as another session (36b).
  const [dropped, setDropped] = useState(false)

  const reload = () => {
    setAnswer(undefined)
    setReads((n) => n + 1)
  }

  const renewed = (session: Session) => {
    const kept = onRenewed(session)
    if (!kept) {
      setDropped(true)
      reload()
    }
    return kept
  }

  useEffect(() => {
    let live = true
    readServer(id).then((a) => {
      if (!live) return
      if (a === 'signed-out') onSignedOut()
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
        canConfigure={canConfigure}
        onDirty={onDirty}
        onReload={reload}
        onRenewed={renewed}
      />
    </section>
  )
}

function Body({
  id,
  answer,
  canConfigure,
  onDirty,
  onReload,
  onRenewed,
}: {
  id: string
  answer: Answer | undefined
  canConfigure: boolean
  onDirty: (dirty: boolean) => void
  onReload: () => void
  onRenewed: (session: Session) => boolean
}) {
  if (answer === undefined || answer === 'signed-out') {
    return <p role="status">{t('server-loading')}</p>
  }
  if ('doc' in answer) {
    return (
      <ServerForm
        id={id}
        doc={answer.doc}
        canConfigure={canConfigure}
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
