// The Server profiles page (#/profiles/server, signed in): the pool read once
// each time the page opens from GET /api/profiles/server, one row per profile
// in the reply's order, the id a link to its page and the instance it runs a
// link to that page. The columns do not sort: the pool is a handful of files,
// and the row's own link is where the detail is
// (fe/briefs/16-server-profiles-and-instances.md, 17a.1 to 17a.4). The list's
// read is its own: `readServerProfiles`, the tags pill's silent read, stays as
// it is (17a.3). The pool is also where a profile is made: the Create form in
// place of 17b, between the intro and the table (fe/DESIGN.md 5.2).
import { useCallback, useEffect, useId, useRef, useState } from 'react'
import { flushSync } from 'react-dom'
import {
  apiError,
  call,
  sessionEnded,
  sessionRevoked,
  type Reply,
} from './api.ts'
import Dialog from './Dialog.tsx'
import { FieldError, Hint, HintToggle } from './Field.tsx'
import { Icon } from './Icon.tsx'
import { InstanceLink } from './Instances.tsx'
import PageHeading from './PageHeading.tsx'
import { refusalOf, type RefusalTable } from './refusal.ts'
import { serverProfileHref } from './route.ts'
import SignIn from './SignIn.tsx'
import type { Session, SignedOut } from './session.ts'
import { Lines, Loading, LoadingText } from './Skeleton.tsx'
import { t } from './strings.ts'

/** One row of the pool, as the page reads it. */
type ServerProfile = {
  id: string
  /** The port, null when the reply's is not a number (17a.4 reads it as 0,
   *  which is not the same as a file naming no port). */
  port: number | null
  tags: string[]
  /** The instance that runs it, null when none does. */
  instance: string | null
}

/** The row of a reply body, null when the item names nothing to open: an id
 * that is not a non-empty string has no page (17a.4). */
function rowOf(value: unknown): ServerProfile | null {
  if (typeof value !== 'object' || value === null) return null
  const item = value as Record<string, unknown>
  if (typeof item.id !== 'string' || item.id === '') return null
  return {
    id: item.id,
    port: typeof item.port === 'number' ? item.port : null,
    tags: Array.isArray(item.tags)
      ? item.tags.filter((tag): tag is string => typeof tag === 'string')
      : [],
    instance:
      typeof item.instance === 'string' && item.instance !== ''
        ? item.instance
        : null,
  }
}

/** What the read gave: the rows, the error of its message, or the session. */
type Answer =
  | ServerProfile[]
  | { error: 'client-unavailable' | 'failed' }
  | 'signed-out'
  | 'revoked'

async function readServerPool(): Promise<Answer> {
  try {
    const reply = await call('/api/profiles/server')
    if (reply.status === 401) return 'signed-out'
    if (sessionRevoked(reply)) return 'revoked'
    if (!reply.ok) {
      return apiError(reply.body)?.key === 'client-unavailable'
        ? { error: 'client-unavailable' }
        : { error: 'failed' }
    }
    if (!Array.isArray(reply.body)) return { error: 'failed' }
    return reply.body
      .map(rowOf)
      .filter((row): row is ServerProfile => row !== null)
  } catch {
    return { error: 'failed' }
  }
}

export default function ServerProfiles({
  onSignedOut,
  onRenewed,
  nameOf,
}: {
  /** The session a read says is gone. */
  onSignedOut: (reason: SignedOut) => void
  /** A sign-in in the form's dialog: true when the page stays. */
  onRenewed: (session: Session) => boolean
  /** The name of an instance by its id (16 17f.4): the Used by cell says the
   * word the menu uses, not the key. */
  nameOf: (id: string) => string
}) {
  // undefined while the reply is awaited.
  const [answer, setAnswer] = useState<Answer | undefined>(undefined)

  // The arrival focus (fe/DESIGN.md 2.3, 16 17a.1): the shell raises its focus
  // request on a navigation, and the session read that brings this page is an
  // arrival too, so the page gives its h1 the focus when it opens.
  useEffect(() => {
    document.getElementById('page-heading')?.focus()
  }, [])

  useEffect(() => {
    let live = true
    readServerPool().then((a) => {
      if (!live) return
      if (a === 'signed-out' || a === 'revoked')
        onSignedOut(a === 'revoked' ? 'revoked' : 'ended')
      else setAnswer(a)
    })
    return () => {
      live = false
    }
  }, [onSignedOut])

  /** The pool read again for the rows already on screen: the page never goes
   * back to the skeleton, so a create the manager could not answer (17b.5) is
   * answered by the list, and the form the reader was typing stays open. */
  const reload = useCallback(() => {
    readServerPool().then((a) => {
      if (a === 'signed-out' || a === 'revoked')
        onSignedOut(a === 'revoked' ? 'revoked' : 'ended')
      else setAnswer(a)
    })
  }, [onSignedOut])

  return (
    <>
      <PageHeading text={t('server-profiles-title')} />
      <section>
        <p className="intro">{t('server-profiles-intro')}</p>
        <Body
          answer={answer}
          onRenewed={onRenewed}
          onReload={reload}
          nameOf={nameOf}
        />
      </section>
    </>
  )
}

/** The table with the create form before it, or the message in its place. */
function Body({
  answer,
  onRenewed,
  onReload,
  nameOf,
}: {
  answer: Answer | undefined
  onRenewed: (session: Session) => boolean
  onReload: () => void
  nameOf: (id: string) => string
}) {
  if (answer === undefined) {
    // The rows' skeleton (17a.3): three lines in the table's place, the read's
    // words in the DOM and out of sight from the first frame. No opener: a
    // profile is made in a pool that is there (17b.1).
    return (
      <Loading text={t('server-profiles-loading')} shape={<Lines n={3} />} />
    )
  }
  if (answer === 'signed-out' || answer === 'revoked') {
    return <LoadingText text={t('server-profiles-loading')} />
  }
  if (!Array.isArray(answer)) {
    return (
      <p>
        {answer.error === 'client-unavailable'
          ? t('api-client-unavailable')
          : t('server-profiles-failed')}
      </p>
    )
  }
  const rows = answer
  return (
    <>
      <Create rows={rows} onRenewed={onRenewed} onReload={onReload} />
      {rows.length === 0 ? (
        <p>{t('server-profiles-empty')}</p>
      ) : (
        <div className="table-scroll">
          <table className="server-profiles" aria-labelledby="page-heading">
            <thead>
              <tr>
                <th scope="col">{t('server-profiles-column-name')}</th>
                <th scope="col">{t('server-profiles-column-port')}</th>
                <th scope="col">{t('server-profiles-column-tags')}</th>
                <th scope="col">{t('server-profiles-column-used-by')}</th>
              </tr>
            </thead>
            <tbody>
              {rows.map((p) => (
                <tr key={p.id}>
                  <td>
                    <a href={serverProfileHref(p.id)}>{p.id}</a>
                  </td>
                  {/* The port as text: 0 is the file that names none, and an
                      empty cell says that better than a zero; a port that is
                      not a number shows 0 (17a.4). */}
                  <td>
                    {p.port === null ? '0' : p.port === 0 ? '' : String(p.port)}
                  </td>
                  <td>{p.tags.join(', ')}</td>
                  {p.instance === null ? (
                    <td className="muted">{t('server-profiles-unused')}</td>
                  ) : (
                    // The name the menu uses, then the id it is the key of
                    // (16 17f.4): two instances named alike are told apart.
                    <td>
                      <InstanceLink id={p.instance} nameOf={nameOf} />
                    </td>
                  )}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </>
  )
}

/** The range of the port a create accepts, the bounds the field's check names
 * in its words, and the first port of the default (17b.1, 17b.3). */
const PORT_MIN = 1
const PORT_MAX = 65535
const PORT_DEFAULT_FROM = 9931

/** The Port the form fills in: the smallest whole number of the default's
 * range that no row of the list the page last read uses, computed here with no
 * request; empty when every one is taken (17b.1). The file naming no port (0)
 * counts for nothing. */
function defaultPort(rows: ServerProfile[]): string {
  const taken = new Set<number>()
  for (const row of rows) {
    if (typeof row.port === 'number' && row.port > 0) taken.add(row.port)
  }
  for (let port = PORT_DEFAULT_FROM; port <= PORT_MAX; port++) {
    if (!taken.has(port)) return String(port)
  }
  return ''
}

/** The create answers with the words of its own keys; each key is one literal
 * t() call, as every page's table is (fe/briefs/16-...md, 17b.5). The field a
 * refusal marks is the one the reply names, so the table names none. */
const CREATE_REFUSALS: RefusalTable = {
  text: (key, args) => {
    switch (key) {
      case 'server-profile-bad-id':
        return t('api-server-profile-bad-id', ...args)
      case 'not-whole-number-in-range':
        return t('api-not-whole-number-in-range', ...args)
      case 'unknown-engine':
        return t('api-unknown-engine', ...args)
      case 'client-server-profile-exists':
        return t('api-client-server-profile-exists', ...args)
      default:
        return undefined
    }
  },
  field: () => undefined,
}

/** The create form in place (fe/DESIGN.md 5.2, 16 17b.1 to 17b.5): the opener
 * discloses a form right after it, whose Port holds the default of the rows
 * anew every time it opens. Everything the reader typed is the form's own, so
 * closing it clears all of it. */
function Create({
  rows,
  onRenewed,
  onReload,
}: {
  rows: ServerProfile[]
  onRenewed: (session: Session) => boolean
  onReload: () => void
}) {
  const uid = useId()
  const nameId = `${uid}-name`
  const portId = `${uid}-port`
  const opener = useRef<HTMLButtonElement>(null)
  const [open, setOpen] = useState(false)
  // A create is in flight: the form stays open with it (17b review R1).
  const sendingRef = useRef(false)

  /** Cancel, or the opener pressed again: the form goes, its fields, errors and
   * alert with it, and the focus returns to the opener (17b.2). While a create
   * runs nothing closes: the form, its status line and the request stay as they
   * are, and the focus does not move (17b review R1). */
  function close() {
    if (sendingRef.current) return
    setOpen(false)
    opener.current?.focus()
  }

  return (
    <>
      <button
        ref={opener}
        type="button"
        id="server-profiles-new"
        className="secondary"
        aria-expanded={open}
        onClick={() => (open ? close() : setOpen(true))}
      >
        <Icon name="plus" />
        {t('server-profiles-new')}
      </button>
      {open && (
        <CreateForm
          nameId={nameId}
          portId={portId}
          rows={rows}
          sendingRef={sendingRef}
          onClose={close}
          onRenewed={onRenewed}
          onReload={onReload}
        />
      )}
    </>
  )
}

/** The form itself, mounted while it is open: its Name, Port, the two buttons,
 * the status line and the alert place (17b.1). */
function CreateForm({
  nameId,
  portId,
  rows,
  sendingRef,
  onClose,
  onRenewed,
  onReload,
}: {
  nameId: string
  portId: string
  rows: ServerProfile[]
  /** Set while the POST runs, read by the close (17b review R1). */
  sendingRef: { current: boolean }
  onClose: () => void
  onRenewed: (session: Session) => boolean
  onReload: () => void
}) {
  const uid = useId()
  const titleId = `${uid}-title`
  const [name, setName] = useState('')
  const [port, setPort] = useState(() => defaultPort(rows))
  const [errors, setErrors] = useState<{ id?: string; port?: string }>({})
  const [status, setStatus] = useState('')
  const [alert, setAlert] = useState<{ text: string; signIn: boolean } | null>(
    null,
  )
  // The sign-in in the form's dialog (17b.5: a 401 answers in the form).
  const [asking, setAsking] = useState(false)
  const [sent, setSent] = useState(false)

  // The focus moves to Name when the form opens (17b.1).
  useEffect(() => {
    document.getElementById(nameId)?.focus()
  }, [nameId])

  /** The create, the client checks first (17b.3): an empty Name, and a Port
   * that is no whole number of the range, each answer under their field; the
   * focus goes to the first of them after its error rendered. */
  function create() {
    if (sendingRef.current) return
    const id = name.trim()
    const text = port.trim()
    const value = Number(text)
    const next: { id?: string; port?: string } = {}
    if (id === '') next.id = t('field-error-required')
    if (
      text === '' ||
      !Number.isInteger(value) ||
      value < PORT_MIN ||
      value > PORT_MAX
    )
      next.port = t('field-error-number', String(PORT_MIN), String(PORT_MAX))
    const field =
      next.id !== undefined ? nameId : next.port !== undefined ? portId : null
    if (field !== null) {
      // The error is in the DOM, and names the field, before the focus lands (17b review D1).
      flushSync(() => setErrors(next))
      document.getElementById(field)?.focus()
      return
    }
    setErrors(next)
    void send(id, value)
  }

  async function send(id: string, value: number) {
    sendingRef.current = true
    setSent(true)
    setAlert(null)
    setStatus(t('create-creating'))
    let answer: Reply | undefined
    try {
      answer = await call('/api/profiles/server', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ engine: 'llama.cpp', id, port: value }),
      })
    } catch {
      answer = undefined
    }
    sendingRef.current = false
    setSent(false)
    setStatus('')
    const reply = answer
    if (reply === undefined) {
      setAlert({ text: t('request-failed'), signIn: false })
      return
    }
    if (reply.status === 201) {
      // The new profile's page, whose h1 takes the focus (17b.4).
      window.location.hash = serverProfileHref(id)
      return
    }
    if (sessionEnded(reply)) {
      setAlert({ text: t('session-ended-retry'), signIn: true })
      return
    }
    if (sessionRevoked(reply)) {
      setAlert({ text: t('session-revoked'), signIn: false })
      return
    }
    const key = apiError(reply.body)?.key
    if (key === 'client-unavailable' || key === 'client-timeout') {
      // The manager did not answer, or did not answer in time: the list read
      // again says whether the profile is there (17b.5, 16 17f.10).
      setAlert({ text: t('create-unknown'), signIn: false })
      onReload()
      return
    }
    const refusal = refusalOf(reply.body, CREATE_REFUSALS)
    if (refusal !== undefined && refusal.field === 'id') {
      // The text is in the DOM, and names the field, before the focus lands (17b review D1).
      flushSync(() => setErrors({ id: refusal.text }))
      document.getElementById(nameId)?.focus()
      return
    }
    if (refusal !== undefined && refusal.field === 'port') {
      flushSync(() => setErrors({ port: refusal.text }))
      document.getElementById(portId)?.focus()
      return
    }
    setAlert({
      text:
        refusal?.text ??
        (reply.status === 422 && key !== undefined
          ? t('api-other', key)
          : t('request-failed')),
      signIn: false,
    })
  }

  /** A sign-in in the form's dialog (17b.5): the form keeps what was typed and
   * the focus returns to Create; nothing is sent again. */
  function signedIn(session: Session) {
    const kept = onRenewed(session)
    flushSync(() => {
      setAsking(false)
      if (kept) setAlert(null)
    })
    if (!kept) return
    document.getElementById('create-button')?.focus()
  }

  function closeAsking() {
    flushSync(() => setAsking(false))
    document.getElementById('session-sign-in')?.focus()
  }

  return (
    <>
      <form
        aria-labelledby={titleId}
        onSubmit={(e) => {
          e.preventDefault()
          create()
        }}
        onKeyDown={(e) => {
          // Escape closes the form, as Cancel does; not while a create runs.
          if (e.key === 'Escape' && !sendingRef.current) onClose()
        }}
      >
        <h2 id={titleId}>{t('server-profiles-new-title')}</h2>
        <CreateField
          id={nameId}
          label={t('server-profile-name')}
          hint={t('server-profile-name-hint')}
          error={errors.id}
          value={name}
          onValue={(v) => {
            setName(v)
            setErrors((e) => (e.id === undefined ? e : { ...e, id: undefined }))
          }}
        />
        <CreateField
          id={portId}
          label={t('server-port')}
          hint={t('server-profile-port-hint')}
          error={errors.port}
          value={port}
          inputMode="numeric"
          onValue={(v) => {
            setPort(v)
            setErrors((e) =>
              e.port === undefined ? e : { ...e, port: undefined },
            )
          }}
        />
        <div className="save-actions">
          <button
            type="submit"
            id="create-button"
            className="primary"
            aria-disabled={sent || undefined}
          >
            {t('create-button')}
          </button>
          <button
            type="button"
            id="create-cancel"
            className="secondary"
            aria-disabled={sent || undefined}
            onClick={onClose}
          >
            {t('create-cancel')}
          </button>
        </div>
        <p role="status" className="save-status" aria-live="polite">
          {status}
        </p>
        {alert !== null && (
          <p role="alert" className="save-alert">
            {alert.text}
          </p>
        )}
        {alert?.signIn && (
          <button
            type="button"
            id="session-sign-in"
            className="secondary"
            onClick={() => setAsking(true)}
          >
            {t('session-sign-in')}
          </button>
        )}
      </form>
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

/** A field of the create form: one box named by its label, with the About
 * toggle of its hint (fe/DESIGN.md 5.2). The box carries the field's id, so no
 * `<id>-box` stands in the form (17b.1). */
function CreateField({
  id,
  label,
  hint,
  error,
  value,
  inputMode,
  onValue,
}: {
  id: string
  label: string
  hint: string
  /** The refusal's text under the box, when there is one. */
  error: string | undefined
  value: string
  /** The keypad of a port (17b.1). */
  inputMode?: 'numeric'
  onValue: (value: string) => void
}) {
  const [open, setOpen] = useState(false)
  const hintId = `${id}-hint`
  return (
    <div className="field">
      <div className="field-label">
        <label htmlFor={id}>{label}</label>
        <HintToggle
          label={label}
          open={open}
          hintId={hintId}
          onToggle={() => setOpen((o) => !o)}
        />
      </div>
      <Hint id={hintId} text={hint} open={open} />
      <div className="field-control">
        <input
          id={id}
          type="text"
          className="mono"
          spellCheck={false}
          autoCapitalize="off"
          autoCorrect="off"
          autoComplete="off"
          inputMode={inputMode}
          {...(error === undefined
            ? {}
            : { 'aria-invalid': true, 'aria-describedby': `${id}-error` })}
          value={value}
          onChange={(e) => onValue(e.target.value)}
        />
      </div>
      {error !== undefined && <FieldError id={`${id}-error`} text={error} />}
    </div>
  )
}
