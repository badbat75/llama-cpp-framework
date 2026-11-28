// The Instance profiles page (#/profiles/instance, signed in): the pool read
// once each time the page opens from GET /api/profiles/instance, one row per
// profile in the reply's order, the id a link to its page, its server profile
// a link to that page, the model count, and the instances that run it (brief
// 26, 26.2). The columns do not sort, as the server list's do not. Below 480 px
// the Server profile column folds into the name cell as a muted second line,
// the way 22 D4.35 folds the Engine. The row's trash can is the shared one
// (26.3, 22 D4), and the list's + at the end of the list discloses the create
// form of 26.4, whose shape is the server list's create of 17b.
import {
  Fragment,
  useCallback,
  useEffect,
  useId,
  useRef,
  useState,
} from 'react'
import { flushSync } from 'react-dom'
import {
  apiError,
  call,
  isRecord,
  sessionEnded,
  sessionRevoked,
  type Reply,
} from './api.ts'
import Dialog from './Dialog.tsx'
import { FieldError, Hint, HintToggle } from './Field.tsx'
import { Icon } from './Icon.tsx'
import { type NameOf } from './Instances.tsx'
import PageHeading from './PageHeading.tsx'
import { refusalOf, type RefusalTable } from './refusal.ts'
import {
  ConfirmRow,
  DeleteReasons,
  RowDelete,
  useRowDelete,
} from './RowDelete.tsx'
import {
  instanceHref,
  instanceProfileHref,
  serverProfileHref,
} from './route.ts'
import SignIn from './SignIn.tsx'
import type { Session, SignedOut } from './session.ts'
import { Lines, Loading, LoadingText } from './Skeleton.tsx'
import { t } from './strings.ts'

/** One row of the pool, as the page reads it. */
type InstanceProfile = {
  id: string
  /** The server profile it runs. */
  server: string
  /** The model profiles it serves. */
  models: string[]
  /** The instances that run it. */
  usedBy: string[]
}

/** The row of a reply body, null when the item names nothing to open: an id
 *  that is not a non-empty string has no page (17a.4). */
function rowOf(value: unknown): InstanceProfile | null {
  if (typeof value !== 'object' || value === null) return null
  const item = value as Record<string, unknown>
  if (typeof item.id !== 'string' || item.id === '') return null
  const words = (value: unknown): string[] =>
    Array.isArray(value)
      ? value.filter((word): word is string => typeof word === 'string')
      : []
  return {
    id: item.id,
    server: typeof item.server === 'string' ? item.server : '',
    models: words(item.models),
    usedBy: words(item.usedBy),
  }
}

/** The Model profiles column's words (26.2): the count, its singular, the
 *  none. The name cell's muted line repeats them (the IP6a review R2), so the
 *  cell and the line are one function and not two copies that can drift. */
function modelWords(models: number): string {
  if (models === 0) return t('instance-profiles-models-none')
  if (models === 1) return t('instance-profiles-models-count-one')
  return t('instance-profiles-models-count', String(models))
}

/** What the read gave: the rows, the error of its message, or the session. */
type Answer =
  | InstanceProfile[]
  | { error: 'client-unavailable' | 'failed' }
  | 'signed-out'
  | 'revoked'

async function readInstancePool(): Promise<Answer> {
  try {
    const reply = await call('/api/profiles/instance')
    if (reply.status === 401) return 'signed-out'
    if (sessionRevoked(reply)) return 'revoked'
    if (!reply.ok) {
      return apiError(reply.body)?.key === 'client-unavailable'
        ? { error: 'client-unavailable' }
        : { error: 'failed' }
    }
    const body = reply.body
    if (!isRecord(body) || !Array.isArray(body.rows)) return { error: 'failed' }
    return body.rows
      .map(rowOf)
      .filter((row): row is InstanceProfile => row !== null)
  } catch {
    return { error: 'failed' }
  }
}

/** The ids of the server profiles the create form's select lists (26.4), empty
 *  when the read did not answer: the form says a server profile has to be
 *  created first, which is true of a pool the page cannot read. */
async function readServerIds(): Promise<string[]> {
  try {
    const reply = await call('/api/profiles/server')
    if (!reply.ok || !Array.isArray(reply.body)) return []
    return reply.body
      .filter(isRecord)
      .map((item) => item.id)
      .filter((id): id is string => typeof id === 'string' && id !== '')
  } catch {
    return []
  }
}

export default function InstanceProfiles({
  onSignedOut,
  onRenewed,
  nameOf,
}: {
  /** The session a read says is gone. */
  onSignedOut: (reason: SignedOut) => void
  /** A sign-in in the form's dialog: true when the page stays. */
  onRenewed: (session: Session) => boolean
  /** The name of an instance by its id: the Used by cell says the word the
   *  menu uses, with the shared-name rule of 22 D4.37. */
  nameOf: NameOf
}) {
  // undefined while the reply is awaited.
  const [answer, setAnswer] = useState<Answer | undefined>(undefined)
  // The server profiles the create form chooses from, read with the list.
  const [servers, setServers] = useState<string[]>([])

  // The arrival focus (fe/DESIGN.md 2.3, as 16 17a.1): the page gives its h1
  // the focus when it opens.
  useEffect(() => {
    document.getElementById('page-heading')?.focus()
  }, [])

  useEffect(() => {
    let live = true
    readInstancePool().then((a) => {
      if (!live) return
      if (a === 'signed-out' || a === 'revoked')
        onSignedOut(a === 'revoked' ? 'revoked' : 'ended')
      else setAnswer(a)
    })
    readServerIds().then((ids) => {
      if (live) setServers(ids)
    })
    return () => {
      live = false
    }
  }, [onSignedOut])

  /** The pool read again for the rows already on screen: a create the manager
   *  could not answer is answered by the list, and the form stays open (17b.5). */
  const reload = useCallback(() => {
    readInstancePool().then((a) => {
      if (a === 'signed-out' || a === 'revoked')
        onSignedOut(a === 'revoked' ? 'revoked' : 'ended')
      else setAnswer(a)
    })
  }, [onSignedOut])

  /** The pool read again by a row delete (22 D4.7): the same read, and the ids
   *  it brought, so the list knows whether the row is there. */
  const reread = useCallback(async (): Promise<string[] | null> => {
    const a = await readInstancePool()
    if (a === 'signed-out' || a === 'revoked') {
      onSignedOut(a === 'revoked' ? 'revoked' : 'ended')
      return null
    }
    setAnswer(a)
    return Array.isArray(a) ? a.map((p) => p.id) : null
  }, [onSignedOut])

  return (
    <>
      <PageHeading text={t('instance-profiles-title')} />
      <section>
        <p className="intro">{t('instance-profiles-intro')}</p>
        <Body
          answer={answer}
          servers={servers}
          onRenewed={onRenewed}
          onReload={reload}
          onReread={reread}
          nameOf={nameOf}
        />
      </section>
    </>
  )
}

/** The list's own line, then the table with a trash can per row, then the
 *  opener of the create form after it, or the message in the table's place. The
 *  delete and its confirm row are 22 D4: the words land on the line above the
 *  table, and the pool is read again in place. */
function Body({
  answer,
  servers,
  onRenewed,
  onReload,
  onReread,
  nameOf,
}: {
  answer: Answer | undefined
  /** The server profiles the create form lists. */
  servers: string[]
  onRenewed: (session: Session) => boolean
  onReload: () => void
  onReread: () => Promise<string[] | null>
  nameOf: NameOf
}) {
  const uid = useId()
  // The list's own lines, above the table (22 D4.6).
  const [status, setStatus] = useState('')
  const [alert, setAlert] = useState<{ text: string; signIn: boolean } | null>(
    null,
  )
  // The sign-in the group's 401 asks for (22 D4.7): the dialog of the create form.
  const [asking, setAsking] = useState(false)
  // The rows a delete removed: a 204 takes the row out with no read (22 D4.6),
  // and the next read of the pool replaces the list.
  const [removed, setRemoved] = useState<string[]>([])
  const rows = Array.isArray(answer)
    ? answer.filter((p) => !removed.includes(p.id))
    : []
  const del = useRowDelete({
    path: (id) => `/api/profiles/instance/${encodeURIComponent(id)}`,
    deleted: (id) => t('instance-profiles-deleted', id),
    remove: (id) => setRemoved((ids) => [...ids, id]),
    reread: onReread,
    order: rows.map((p) => p.id),
    minus: (id) => document.getElementById(`${uid}-delete-${id}`),
    // The list's own control: the + of the create, after the table (22 D4.6).
    lastResort: () => document.getElementById('instance-profiles-new'),
    onStatus: setStatus,
    onAlert: setAlert,
    onSignIn: () => setAsking(true),
  })

  if (answer === undefined) {
    // The rows' skeleton (17a.3): three lines in the table's place, the read's
    // words in the DOM and out of sight from the first frame. No opener: a
    // profile is made in a pool that is there (17b.1).
    return (
      <Loading text={t('instance-profiles-loading')} shape={<Lines n={3} />} />
    )
  }
  if (answer === 'signed-out' || answer === 'revoked') {
    return <LoadingText text={t('instance-profiles-loading')} />
  }
  if (!Array.isArray(answer)) {
    return (
      <p>
        {answer.error === 'client-unavailable'
          ? t('api-client-unavailable')
          : t('instance-profiles-failed')}
      </p>
    )
  }
  /** The pool the read gave empty: the message in the table's place (17a.3). A
   *  pool emptied by the reader's own deletes keeps the table, whose rows the
   *  list is answering for (22 D4.6). */
  const empty = rows.length === 0 && removed.length === 0

  function closeAsking() {
    flushSync(() => setAsking(false))
    document.getElementById('session-sign-in')?.focus()
  }

  /** A sign-in in the dialog: the list keeps its rows, the words of the refusal
   *  go with the dialog, and the focus returns to the Sign in (22 D4.7). */
  function signedIn(session: Session) {
    const kept = onRenewed(session)
    flushSync(() => {
      setAsking(false)
      if (kept) setAlert(null)
    })
    if (!kept) return
    document.getElementById('session-sign-in')?.focus()
  }

  return (
    <>
      <p role="status" className="save-status" aria-live="polite">
        {status}
      </p>
      {alert !== null && <p role="alert">{alert.text}</p>}
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
      {empty ? (
        <p>{t('instance-profiles-empty')}</p>
      ) : (
        <div className="table-scroll">
          <table className="instance-profiles" aria-labelledby="page-heading">
            <thead>
              <tr>
                <th scope="col">{t('instance-profiles-column-name')}</th>
                <th scope="col">{t('instance-profiles-column-server')}</th>
                <th scope="col">{t('instance-profiles-column-models')}</th>
                <th scope="col">{t('instance-profiles-column-used-by')}</th>
                {/* The column of the row's actions (22 D4.23): its word is for
                    a reader, so the column is announced; the eye sees no
                    heading over an icon. */}
                <th scope="col">
                  <span className="visually-hidden">
                    {t('profiles-column-actions')}
                  </span>
                </th>
              </tr>
            </thead>
            <tbody>
              {rows.map((p) => {
                // The model count of the Model profiles column (26.2), the
                // words of `modelWords` so the name cell's muted line says the
                // same words (IP6a review R2): a row read at 320 px, where both
                // columns are folded, says what the row says at 1280 px.
                const models =
                  p.models.length === 0 ? (
                    <span className="muted">
                      {t('instance-profiles-models-none')}
                    </span>
                  ) : (
                    modelWords(p.models.length)
                  )
                // The server profile: the link to its page, in its own column
                // and in the name cell's second line (26.2, the way 22 D4.35
                // folds the Engine): the sheet hides the two columns below 480
                // px, where the line is what says what runs the profile. The
                // line's link takes the line's colour (the sheet's `.muted a`),
                // so a row shows one purple link, the row's (IP6a review R2).
                // The DOM always holds both copies.
                const server =
                  p.server === '' ? null : (
                    <a href={serverProfileHref(p.server)}>{p.server}</a>
                  )
                const line =
                  server === null ? null : (
                    <span className="muted">
                      {server}
                      {` · ${modelWords(p.models.length)}`}
                    </span>
                  )
                return (
                  <Fragment key={p.id}>
                    <tr>
                      <td>
                        <a href={instanceProfileHref(p.id)}>{p.id}</a>
                        {line}
                      </td>
                      <td>{server}</td>
                      <td>{models}</td>
                      <td>
                        {p.usedBy.length === 0 ? (
                          <span className="muted">
                            {t('instance-profiles-unused')}
                          </span>
                        ) : (
                          // The name the menu uses, comma-separated (26.2). The
                          // cell is the name alone: a list of two instances
                          // sharing a name says it twice, and the row's own key
                          // is the profile. The id beside the name is the
                          // InstanceLink's rule (22 D4.37), kept for the
                          // sentence of a page that names one instance.
                          p.usedBy.map((id, n) => (
                            <Fragment key={id}>
                              {n > 0 && ', '}
                              <a href={instanceHref(id)}>{nameOf(id)}</a>
                            </Fragment>
                          ))
                        )}
                      </td>
                      {/* The row's actions (22 D4.23): the trash can alone, in
                          the column every row ends with. */}
                      <td>
                        <RowDelete
                          domId={`${uid}-delete-${p.id}`}
                          name={t('instance-profiles-delete-row', p.id)}
                          reason={
                            p.usedBy.length === 0
                              ? null
                              : t('instance-profiles-delete-in-use')
                          }
                          disabled={p.usedBy.length > 0}
                          busy={del.deleting}
                          open={del.confirming === p.id}
                          onPress={() => del.open(p.id)}
                        />
                      </td>
                    </tr>
                    {del.confirming === p.id && (
                      <ConfirmRow
                        columns={5}
                        question={t('instance-profiles-delete-question', p.id)}
                        status={del.status}
                        alert={del.alert}
                        running={del.deleting}
                        onConfirm={del.confirm}
                        onCancel={del.close}
                        onSignIn={() => setAsking(true)}
                      />
                    )}
                  </Fragment>
                )
              })}
            </tbody>
          </table>
        </div>
      )}
      <Create
        rows={rows}
        servers={servers}
        onRenewed={onRenewed}
        onReload={onReload}
      />
      {/* The reasons the disabled trash can describes (22 D4.2): out of the
          cells, and after the list's own controls, so the line is all that
          stands above the table and the + keeps its place after the box. */}
      <DeleteReasons
        items={rows
          .filter((p) => p.usedBy.length > 0)
          .map((p) => ({
            id: `${uid}-delete-${p.id}`,
            reason: t('instance-profiles-delete-in-use'),
          }))}
      />
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

/** The id an instance profile name accepts: the shape `api-invalid-instance-
 *  profile-id` describes, the check of 26.4. */
const ID_SHAPE = /^[A-Za-z0-9][A-Za-z0-9._-]*$/

/** The create answers with the words of its own keys; each key is one literal
 *  t() call, as every page's table is (17b.5). The field a refusal marks is the
 *  one the reply names, so the table names none. */
const CREATE_REFUSALS: RefusalTable = {
  text: (key, args) => {
    switch (key) {
      case 'invalid-instance-profile-id':
        return t('api-invalid-instance-profile-id', ...args)
      case 'instance-profile-name-taken':
        return t('api-instance-profile-name-taken', ...args)
      default:
        return undefined
    }
  },
  field: () => undefined,
}

/** The create form in place (26.4, fe/DESIGN.md 5.2, as 16 17b2.1): the opener
 *  discloses a form right after it. Everything the reader typed is the form's
 *  own, so closing it clears all of it. */
function Create({
  rows,
  servers,
  onRenewed,
  onReload,
}: {
  rows: InstanceProfile[]
  /** The server profiles to choose from. */
  servers: string[]
  onRenewed: (session: Session) => boolean
  onReload: () => void
}) {
  const uid = useId()
  const nameId = `${uid}-name`
  const serverId = `${uid}-server`
  const opener = useRef<HTMLButtonElement>(null)
  const [open, setOpen] = useState(false)
  // A create is in flight: the form stays open with it (17b review R1).
  const sendingRef = useRef(false)

  /** Cancel, or the opener pressed again: the form goes, its fields, errors and
   *  alert with it, and the focus returns to the opener (17b.2). While a create
   *  runs nothing closes: the form, its status line and the request stay as they
   *  are, and the focus does not move (17b review R1). */
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
        id="instance-profiles-new"
        className="ot-add"
        aria-expanded={open}
        onClick={() => (open ? close() : setOpen(true))}
      >
        <Icon name="plus" />
        <span className="visually-hidden">{t('instance-profiles-new')}</span>
      </button>
      {open && (
        <CreateForm
          nameId={nameId}
          serverId={serverId}
          rows={rows}
          servers={servers}
          sendingRef={sendingRef}
          onClose={close}
          onRenewed={onRenewed}
          onReload={onReload}
        />
      )}
    </>
  )
}

/** The form itself, mounted while it is open (26.4): its Name, its Server
 *  profile select, the two buttons, the status line and the alert place. */
function CreateForm({
  nameId,
  serverId,
  rows,
  servers,
  sendingRef,
  onClose,
  onRenewed,
  onReload,
}: {
  nameId: string
  serverId: string
  /** The pool's rows: a Name the pool has letter case aside is taken (26.4). */
  rows: InstanceProfile[]
  /** The server profiles to choose from. */
  servers: string[]
  /** Set while the POST runs, read by the close (17b review R1). */
  sendingRef: { current: boolean }
  onClose: () => void
  onRenewed: (session: Session) => boolean
  onReload: () => void
}) {
  const uid = useId()
  const titleId = `${uid}-title`
  const [name, setName] = useState('')
  const [server, setServer] = useState('')
  const [errors, setErrors] = useState<{ id?: string; server?: string }>({})
  const [status, setStatus] = useState('')
  const [alert, setAlert] = useState<{ text: string; signIn: boolean } | null>(
    null,
  )
  // The sign-in in the form's dialog (17b.5: a 401 answers in the form).
  const [asking, setAsking] = useState(false)
  const [sent, setSent] = useState(false)
  // The note of a pool with nothing to choose (26.4): the create moves the focus
  // to its link, the control that answers the choose check (IP6a review R3).
  const noteRef = useRef<HTMLParagraphElement>(null)

  // The focus moves to Name when the form opens (26.4, as 17b.1).
  useEffect(() => {
    document.getElementById(nameId)?.focus()
  }, [nameId])

  /** The create, the client checks first (26.4, in the CP.3 order the IP6a
   *  review R3 reads): an empty Name first, also when the pool has nothing to
   *  choose; then no server profile chosen, when there is a pool to choose
   *  from; then a Name that is no id; then a Name the pool has letter case
   *  aside; then no server profile chosen from that pool. Each answers in the
   *  status line and under its field, and the focus goes to the control that
   *  answers the check, after its words rendered (17b review D1): the name
   *  input, the select, and for a pool with nothing to choose the note's link
   *  to the server profiles, the control that does answer it. */
  function create() {
    if (sendingRef.current) return
    const id = name.trim()
    const chosen = server
    const next: { id?: string; server?: string } = {}
    if (id === '') next.id = t('field-error-required')
    else if (servers.length === 0)
      // The pool has nothing to choose (26.4): Create answers the choose check,
      // the server profile being the one thing the profile cannot do without.
      next.server = t('field-error-choose')
    else if (!ID_SHAPE.test(id))
      next.id = t('api-invalid-instance-profile-id', id)
    else if (rows.some((row) => row.id.toLowerCase() === id.toLowerCase()))
      next.id = t('api-instance-profile-name-taken', id)
    else if (chosen === '') next.server = t('field-error-choose')
    if (next.id === undefined && next.server === undefined) {
      setErrors({})
      setStatus('')
      void send(id, chosen)
      return
    }
    // The words are in the DOM, and name the field, before the focus lands
    // (17b review D1).
    flushSync(() => {
      setErrors(next)
      setStatus(next.id ?? next.server ?? '')
    })
    const note =
      next.server !== undefined && servers.length === 0
        ? noteRef.current?.querySelector('a')
        : undefined
    if (note) note.focus()
    else
      document
        .getElementById(next.id === undefined ? serverId : nameId)
        ?.focus()
  }

  async function send(id: string, chosen: string) {
    sendingRef.current = true
    setSent(true)
    setAlert(null)
    setStatus(t('create-creating'))
    let answer: Reply | undefined
    try {
      answer = await call('/api/profiles/instance', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ id, server: chosen }),
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
      // The new profile's page, whose h1 takes the focus (26.4).
      window.location.hash = instanceProfileHref(id)
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
      // again says whether the profile is there (17b.5).
      setAlert({ text: t('create-unknown'), signIn: false })
      onReload()
      return
    }
    const refusal = refusalOf(reply.body, CREATE_REFUSALS)
    if (refusal !== undefined && refusal.field === 'id') {
      flushSync(() => {
        setErrors({ id: refusal.text })
        setStatus(refusal.text)
      })
      document.getElementById(nameId)?.focus()
      return
    }
    if (refusal !== undefined && refusal.field === 'server') {
      flushSync(() => {
        setErrors({ server: refusal.text })
        setStatus(refusal.text)
      })
      document.getElementById(serverId)?.focus()
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
   *  the focus returns to Create; nothing is sent again. */
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
        <h2 id={titleId}>{t('instance-profiles-new-title')}</h2>
        <NameField
          id={nameId}
          error={errors.id}
          value={name}
          onValue={(v) => {
            setName(v)
            setErrors((e) => (e.id === undefined ? e : { ...e, id: undefined }))
            setStatus('')
          }}
        />
        <div className="field">
          <div className="field-label">
            <label htmlFor={serverId}>
              {t('instance-profiles-column-server')}
            </label>
          </div>
          <div className="field-control">
            {servers.length === 0 ? (
              // No pool to choose from (26.4): the sentence with the link to
              // the server profiles, and Create answers the choose check by
              // moving the focus to that link (the IP6a review R3).
              <p ref={noteRef}>
                {t('instance-profile-no-server')}{' '}
                <a href="#/profiles/server">{t('nav-server-profiles')}</a>
              </p>
            ) : (
              <select
                id={serverId}
                value={server}
                {...(errors.server === undefined
                  ? {}
                  : {
                      'aria-invalid': true,
                      'aria-describedby': `${serverId}-error`,
                    })}
                onChange={(e) => {
                  setServer(e.target.value)
                  setErrors((prev) =>
                    prev.server === undefined
                      ? prev
                      : { ...prev, server: undefined },
                  )
                  setStatus('')
                }}
              >
                <option value="">{t('instance-profile-server-choose')}</option>
                {servers.map((id) => (
                  <option key={id} value={id}>
                    {id}
                  </option>
                ))}
              </select>
            )}
          </div>
          {errors.server !== undefined && (
            <FieldError id={`${serverId}-error`} text={errors.server} />
          )}
        </div>
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

/** The Name of the create form: one box named by its label, with the About
 *  toggle of its hint (fe/DESIGN.md 5.2). The box carries the field's id, so no
 *  `<id>-box` stands in the form (17b.1). */
function NameField({
  id,
  error,
  value,
  onValue,
}: {
  id: string
  /** The refusal's text under the box, when there is one. */
  error: string | undefined
  value: string
  onValue: (value: string) => void
}) {
  const [open, setOpen] = useState(false)
  const hintId = `${id}-hint`
  const label = t('instance-profile-name')
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
      <Hint id={hintId} text={t('instance-profile-name-hint')} open={open} />
      <div className="field-control">
        <input
          id={id}
          type="text"
          className="mono"
          spellCheck={false}
          autoCapitalize="off"
          autoCorrect="off"
          autoComplete="off"
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
