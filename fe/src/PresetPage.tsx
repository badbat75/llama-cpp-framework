// A preset's page (#/models/<id>, signed in), read-only: the way back to the
// list, the id as the heading, and the preset read once each time the page
// opens from GET /api/presets/<id>, or the message saying why it is not there
// (fe/briefs/02-models.md, sections 2d.1 to 2d.4). The settings below the
// heading come with the content of the page.
import { useEffect, useState } from 'react'
import { apiError, call, isRecord } from './api.ts'
import PageHeading from './PageHeading.tsx'
import { presetIdUnreadable } from './route.ts'
import { t } from './strings.ts'

/** One preset as the read gives it, the shape of criterion 81. */
type PresetDoc = {
  id: string
  engine: string
  engineAvailable: boolean
  enabled: boolean
  values: Record<string, string>
  projectUrl: string
}

/** What the read gave: the preset, the key of its message, signed out. */
type Answer =
  | { doc: PresetDoc }
  | {
      error:
        'client-not-authorized' | 'client-unavailable' | 'not-found' | 'failed'
    }
  | 'signed-out'

function isPresetDoc(value: unknown): value is PresetDoc {
  return (
    isRecord(value) &&
    typeof value.id === 'string' &&
    typeof value.engine === 'string' &&
    typeof value.engineAvailable === 'boolean' &&
    typeof value.enabled === 'boolean' &&
    typeof value.projectUrl === 'string' &&
    isRecord(value.values) &&
    Object.values(value.values).every((v) => typeof v === 'string')
  )
}

async function readPreset(id: string): Promise<Answer> {
  try {
    const reply = await call(`/api/presets/${encodeURIComponent(id)}`)
    if (reply.status === 401) return 'signed-out'
    if (!reply.ok) {
      const key = apiError(reply.body)?.key
      switch (key) {
        case 'client-not-authorized':
        case 'client-unavailable':
          return { error: key }
        case 'no-preset-named':
          return { error: 'not-found' }
        default:
          return { error: 'failed' }
      }
    }
    return isPresetDoc(reply.body) ? { doc: reply.body } : { error: 'failed' }
  } catch {
    return { error: 'failed' }
  }
}

export default function PresetPage({
  id,
  onSignedOut,
}: {
  /** The id from the address, while the read has not answered. */
  id: string
  onSignedOut: () => void
}) {
  // undefined while the reply is awaited.
  const [answer, setAnswer] = useState<Answer | undefined>(undefined)

  useEffect(() => {
    // `.` and `..` are sent to no read at all (59a): the page says there is no
    // such preset, as a 404 would.
    if (presetIdUnreadable(id)) return
    let live = true
    readPreset(id).then((a) => {
      if (!live) return
      if (a === 'signed-out') onSignedOut()
      else setAnswer(a)
    })
    return () => {
      live = false
    }
  }, [id, onSignedOut])

  // The heading is the id from the address, then the file's spelling as the
  // read gave it (the bff matches ids case-insensitively).
  const shown =
    answer !== undefined && typeof answer === 'object' && 'doc' in answer
      ? answer.doc.id
      : id
  return (
    <>
      <p className="preset-back">
        <a href="#/models">{t('preset-back')}</a>
      </p>
      <PageHeading text={shown} />
      <Body id={id} answer={answer} />
    </>
  )
}

/** The page's content: nothing below the heading yet, or the message. */
function Body({ id, answer }: { id: string; answer: Answer | undefined }) {
  if (presetIdUnreadable(id)) {
    return <p>{t('preset-not-found', id)}</p>
  }
  if (answer === undefined || answer === 'signed-out') {
    return <p role="status">{t('preset-loading')}</p>
  }
  if ('doc' in answer) return null
  if (answer.error === 'client-not-authorized') {
    return <p>{t('api-client-not-authorized')}</p>
  }
  if (answer.error === 'client-unavailable') {
    return <p>{t('api-client-unavailable')}</p>
  }
  if (answer.error === 'not-found') {
    return <p>{t('preset-not-found', id)}</p>
  }
  return <p>{t('preset-load-failed')}</p>
}
