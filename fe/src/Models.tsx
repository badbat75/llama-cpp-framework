// The Models page (#/models, fe/briefs/02-models.md, slice 2a): the presets of
// every engine read from the bff's /api/presets, shown read-only, with a
// filter over them (slice 2b).
import { useEffect, useRef, useState } from 'react'
import { apiError, call, isRecord } from './api.ts'
import PageHeading from './PageHeading.tsx'
import { t } from './strings.ts'

type Preset = {
  id: string
  engine: string
  engineAvailable: boolean
  enabled: boolean
  model: string
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
    typeof value.enabled === 'boolean' &&
    typeof value.model === 'string'
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
  return <Filtered presets={answer} />
}

/** The GUI's rule: the whole text, lowercased, in the id or the model. */
function matches(p: Preset, filter: string): boolean {
  const text = filter.toLowerCase()
  return (
    p.id.toLowerCase().includes(text) || p.model.toLowerCase().includes(text)
  )
}

/** The filter field, its status line and the rows it lets through. */
function Filtered({ presets }: { presets: Preset[] }) {
  const [filter, setFilter] = useState('')
  const field = useRef<HTMLInputElement>(null)
  const shown = presets.filter((p) => matches(p, filter))

  function clear() {
    setFilter('')
    field.current?.focus()
  }

  let statusText = ''
  if (filter !== '') {
    statusText =
      shown.length === 0
        ? t('presets-no-match')
        : t(
            'presets-filter-count',
            String(shown.length),
            String(presets.length),
          )
  }

  return (
    <>
      <div className="presets-filter">
        <label htmlFor="presets-filter">{t('presets-filter-label')}</label>
        <div className="presets-filter-row">
          <input
            ref={field}
            id="presets-filter"
            type="search"
            autoComplete="off"
            spellCheck={false}
            aria-describedby="presets-filter-hint"
            value={filter}
            onChange={(e) => setFilter(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Escape' && filter !== '') {
                e.preventDefault()
                setFilter('')
              }
            }}
          />
          {filter !== '' && (
            <button type="button" className="secondary" onClick={clear}>
              {t('presets-filter-clear')}
            </button>
          )}
        </div>
        <p id="presets-filter-hint" className="presets-filter-hint">
          {t('presets-filter-hint')}
        </p>
      </div>
      <p role="status" className="presets-filter-status">
        {statusText}
      </p>
      {shown.length > 0 && <Table presets={shown} />}
    </>
  )
}

/** The presets as a table. */
function Table({ presets }: { presets: Preset[] }) {
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
        {presets.map((p) => (
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
