// The Models page (#/models, fe/briefs/02-models.md, slice 2a): the presets of
// every engine read from the bff's /api/presets, shown read-only.
import { useEffect, useState } from 'react'
import { apiError, call, isRecord } from './api.ts'
import PageHeading from './PageHeading.tsx'
import { t } from './strings.ts'

type Preset = {
  id: string
  engine: string
  engineAvailable: boolean
  enabled: boolean
}

/** What the read gave: the presets, or the error key of its message. */
type Answer =
  | Preset[]
  | { error: 'client-not-authorized' | 'client-unavailable' | 'failed' }
  | 'signed-out'

function isPreset(value: unknown): value is Preset {
  return (
    isRecord(value) &&
    typeof value.id === 'string' &&
    typeof value.engine === 'string' &&
    typeof value.engineAvailable === 'boolean' &&
    typeof value.enabled === 'boolean'
  )
}

async function readPresets(): Promise<Answer> {
  try {
    const reply = await call('/api/presets')
    if (reply.status === 401) return 'signed-out'
    if (!reply.ok) {
      const key = apiError(reply.body)?.key
      switch (key) {
        case 'client-not-authorized':
        case 'client-unavailable':
          return { error: key }
        default:
          return { error: 'failed' }
      }
    }
    if (!Array.isArray(reply.body) || !reply.body.every(isPreset))
      return { error: 'failed' }
    return reply.body
  } catch {
    return { error: 'failed' }
  }
}

export default function Models({ onSignedOut }: { onSignedOut: () => void }) {
  // undefined while the reply is awaited.
  const [answer, setAnswer] = useState<Answer | undefined>(undefined)

  useEffect(() => {
    let live = true
    readPresets().then((a) => {
      if (!live) return
      if (a === 'signed-out') onSignedOut()
      else setAnswer(a)
    })
    return () => {
      live = false
    }
  }, [onSignedOut])

  return (
    <>
      <PageHeading text={t('models-title')} />
      <section>
        <h2 id="presets-title">{t('presets-title')}</h2>
        <p className="intro">{t('presets-intro')}</p>
        <Body answer={answer} />
        {answer !== undefined && (
          <p className="presets-note">{t('presets-off-note')}</p>
        )}
      </section>
    </>
  )
}

/** The presets as a table, or the message in their place. */
function Body({ answer }: { answer: Answer | undefined }) {
  if (answer === undefined || answer === 'signed-out') {
    return <p role="status">{t('presets-loading')}</p>
  }
  if (!Array.isArray(answer)) {
    if (answer.error === 'client-not-authorized') {
      return <p>{t('api-client-not-authorized')}</p>
    }
    if (answer.error === 'client-unavailable') {
      return <p>{t('api-client-unavailable')}</p>
    }
    return <p>{t('presets-load-failed')}</p>
  }
  if (answer.length === 0) {
    return <p>{t('presets-empty')}</p>
  }
  return (
    <table className="presets" aria-labelledby="presets-title">
      <thead>
        <tr>
          <th scope="col">{t('presets-column-name')}</th>
          <th scope="col">{t('presets-column-engine')}</th>
          <th scope="col">{t('presets-column-status')}</th>
        </tr>
      </thead>
      <tbody>
        {answer.map((p) => (
          <tr key={p.id} className={p.enabled ? undefined : 'off'}>
            <td>{p.id}</td>
            <td>
              {p.engineAvailable
                ? p.engine
                : t('presets-engine-not-available', p.engine)}
            </td>
            <td>
              {p.enabled ? t('presets-status-on') : t('presets-status-off')}
            </td>
          </tr>
        ))}
      </tbody>
    </table>
  )
}
