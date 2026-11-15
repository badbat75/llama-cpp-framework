// The Models page (#/models, fe/briefs/02-models.md, slice 2a): the presets of
// every engine read from the bff's /api/presets, with a
// filter over them (slice 2b), and each row's on/off switch (slice 2c).
import { useEffect, useId, useRef, useState } from 'react'
import { flushSync } from 'react-dom'
import { apiError, call, isRecord, type ApiError } from './api.ts'
import Dialog from './Dialog.tsx'
import PageHeading from './PageHeading.tsx'
import { presetHref, presetIdUnreadable } from './route.ts'
import type { Session } from './session.ts'
import SignIn from './SignIn.tsx'
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

/** What a switch write gave: the reply, 'signed-out', the error, or 'failed'. */
type Written =
  | { enabled: boolean; running: boolean }
  | 'signed-out'
  | (ApiError & { status: number })
  | 'failed'

async function writeEnabled(id: string, enabled: boolean): Promise<Written> {
  try {
    const reply = await call(`/api/presets/${encodeURIComponent(id)}/enabled`, {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ enabled }),
    })
    if (reply.ok) {
      const body = reply.body
      return isRecord(body) &&
        typeof body.enabled === 'boolean' &&
        typeof body.running === 'boolean'
        ? { enabled: body.enabled, running: body.running }
        : 'failed'
    }
    if (reply.status === 401) return 'signed-out'
    const error = apiError(reply.body)
    return error === null ? 'failed' : { ...error, status: reply.status }
  } catch {
    return 'failed'
  }
}

/** The alert of a failed switch write (2c.48). */
function failureText(
  id: string,
  result: Exclude<Written, { running: boolean }>,
): string {
  if (result === 'signed-out') return t('session-ended-retry')
  if (result === 'failed') return t('request-failed')
  switch (result.key) {
    case 'client-not-authorized':
      return t('api-client-not-authorized')
    case 'client-unavailable':
      return t('api-client-unavailable')
    case 'no-preset-named':
      return t('api-no-preset-named', id)
    case 'load-on-startup-over-max':
      return t('api-load-on-startup-over-max', ...result.args)
    case 'load-on-startup-unlimited':
      return t('api-load-on-startup-unlimited', ...result.args)
    default:
      return result.status === 422
        ? t('api-other', result.key)
        : t('request-failed')
  }
}

/** The status of a switch that held (2c.46). */
function switchedText(p: Preset, running: boolean): string {
  if (p.enabled) {
    return running
      ? t('presets-switched-on-restart', p.id, p.engine)
      : t('presets-switched-on', p.id, p.engine)
  }
  return running
    ? t('presets-switched-off-restart', p.id, p.engine)
    : t('presets-switched-off', p.id, p.engine)
}

export default function Models({
  filterText,
  onFilterText,
  onSignedOut,
  onRenewed,
}: {
  /** The text kept by the shell, for the tab's life (02-models.md, 62). */
  filterText: string
  onFilterText: (text: string) => void
  onSignedOut: () => void
  onRenewed: (session: Session) => boolean
}) {
  // undefined while the reply is awaited.
  const [answer, setAnswer] = useState<Answer | undefined>(undefined)
  // Raised by a sign-in as another session: the presets are read again.
  const [reads, setReads] = useState(0)

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
  }, [onSignedOut, reads])

  const reread = () => {
    setAnswer(undefined)
    setReads((n) => n + 1)
  }

  return (
    <>
      <PageHeading text={t('models-title')} />
      <section>
        <h2 id="presets-title">{t('presets-title')}</h2>
        <p className="intro">{t('presets-intro')}</p>
        <Body
          answer={answer}
          filter={filterText}
          onFilter={onFilterText}
          onRenewed={onRenewed}
          onReread={reread}
        />
        {answer !== undefined && (
          <p className="presets-note">{t('presets-off-note')}</p>
        )}
      </section>
    </>
  )
}

/** The presets as a table, or the message in their place. */
function Body({
  answer,
  filter,
  onFilter,
  onRenewed,
  onReread,
}: {
  answer: Answer | undefined
  filter: string
  onFilter: (text: string) => void
  onRenewed: (session: Session) => boolean
  onReread: () => void
}) {
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
    <Filtered
      read={answer}
      filter={filter}
      onFilter={onFilter}
      onRenewed={onRenewed}
      onReread={onReread}
    />
  )
}

/** The GUI's rule: the whole text, lowercased, in the id or the model. */
function matches(p: Preset, filter: string): boolean {
  const text = filter.toLowerCase()
  return (
    p.id.toLowerCase().includes(text) || p.model.toLowerCase().includes(text)
  )
}

/**
 * The filter field, its status line, the switches' feedback and the rows the
 * filter lets through, each with its switch (2c). The text is the shell's
 * (62), never this component's: it outlives the page.
 */
function Filtered({
  read,
  filter,
  onFilter,
  onRenewed,
  onReread,
}: {
  read: Preset[]
  filter: string
  onFilter: (text: string) => void
  onRenewed: (session: Session) => boolean
  onReread: () => void
}) {
  const uid = useId()
  // The rows as read, each switch's value kept up to date in place (2c.47).
  const [presets, setPresets] = useState(read)
  const [busy, setBusy] = useState(false)
  const [status, setStatus] = useState('')
  const [alert, setAlert] = useState<{ text: string; signIn: boolean } | null>(
    null,
  )
  const [asking, setAsking] = useState(false)
  const field = useRef<HTMLInputElement>(null)
  // The row of the latest write: where focus returns after a sign-in (2c.49).
  const last = useRef(0)
  const mounted = useRef(true)
  useEffect(() => {
    mounted.current = true
    return () => {
      mounted.current = false
    }
  }, [])
  const shown = presets
    .map((p, i) => ({ p, i }))
    .filter(({ p }) => matches(p, filter))

  function setEnabled(i: number, enabled: boolean) {
    setPresets((ps) => ps.map((p, j) => (j === i ? { ...p, enabled } : p)))
  }

  async function change(i: number, enabled: boolean) {
    if (busy) return
    const p = presets[i]!
    const previous = p.enabled
    last.current = i
    setEnabled(i, enabled)
    setBusy(true)
    setStatus('')
    setAlert(null)
    const result = await writeEnabled(p.id, enabled)
    if (!mounted.current) return
    if (typeof result === 'object' && 'running' in result) {
      setEnabled(i, result.enabled)
      setStatus(switchedText({ ...p, enabled: result.enabled }, result.running))
      setBusy(false)
      return
    }
    setAlert({
      text: failureText(p.id, result),
      signIn: result === 'signed-out',
    })
    if (result === 'signed-out') {
      setEnabled(i, previous)
      setBusy(false)
      return
    }
    // The write may have happened: that row's value as read again (2c.48).
    const again = await readPresets()
    if (!mounted.current) return
    const row = Array.isArray(again)
      ? again.find((r) => r.id === p.id)
      : undefined
    setEnabled(i, row === undefined ? previous : row.enabled)
    setBusy(false)
  }

  function closeAsking() {
    flushSync(() => setAsking(false))
    document.getElementById(`${uid}-sign-in`)?.focus()
  }

  function signedIn(session: Session) {
    const kept = onRenewed(session)
    flushSync(() => {
      setAsking(false)
      setAlert(null)
    })
    if (kept) document.getElementById(`${uid}-switch-${last.current}`)?.focus()
    else onReread()
  }

  function clear() {
    onFilter('')
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
            onChange={(e) => onFilter(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Escape' && filter !== '') {
                e.preventDefault()
                onFilter('')
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
      <p role="status" className="presets-switch-status">
        {status}
      </p>
      {alert !== null && (
        <p role="alert" className="presets-switch-alert">
          {alert.text}
        </p>
      )}
      {alert?.signIn && (
        <button
          id={`${uid}-sign-in`}
          type="button"
          className="secondary"
          onClick={() => setAsking(true)}
        >
          {t('session-sign-in')}
        </button>
      )}
      {shown.length > 0 && (
        <Table
          uid={uid}
          rows={shown}
          busy={busy}
          onChange={(i, enabled) => void change(i, enabled)}
        />
      )}
      {asking && (
        <Dialog title={t('signin-title')} onClose={closeAsking}>
          <SignIn
            required={false}
            inDialog
            onSignedIn={signedIn}
            onCancel={closeAsking}
          />
        </Dialog>
      )}
    </>
  )
}

/**
 * The presets as a table; row `i` (its place in the list read) names its
 * switch `<uid>-switch-<i>` by its name `<uid>-name-<i>`.
 */
function Table({
  uid,
  rows,
  busy,
  onChange,
}: {
  uid: string
  rows: { p: Preset; i: number }[]
  /** A write runs: every switch aria-disabled, its changes ignored (2c.45). */
  busy: boolean
  onChange: (i: number, enabled: boolean) => void
}) {
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
        {rows.map(({ p, i }) => (
          <tr key={p.id} className={p.enabled ? undefined : 'off'}>
            <td>
              <span id={`${uid}-name-${i}`}>
                {presetIdUnreadable(p.id) ? (
                  p.id
                ) : (
                  <a href={presetHref(p.id)}>{p.id}</a>
                )}
              </span>
            </td>
            <td>
              {p.engineAvailable
                ? p.engine
                : t('presets-engine-not-available', p.engine)}
            </td>
            <td>
              <label className="presets-switch">
                <input
                  id={`${uid}-switch-${i}`}
                  type="checkbox"
                  role="switch"
                  checked={p.enabled}
                  aria-labelledby={`${uid}-name-${i}`}
                  aria-disabled={busy || undefined}
                  onChange={(e) => {
                    if (!busy) onChange(i, e.target.checked)
                  }}
                />
              </label>
              <span aria-hidden="true">
                {p.enabled ? t('presets-status-on') : t('presets-status-off')}
              </span>
            </td>
          </tr>
        ))}
      </tbody>
    </table>
  )
}
