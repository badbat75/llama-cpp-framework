// One server profile's page (#/profiles/server/<id>, signed in): the back link,
// the id, the instance that runs the profile, the Delete of a profile no
// instance runs, and the Server form, moved here from the instance page
// (fe/briefs/16-server-profiles-and-instances.md, 16 17a.5 to 17a.9 and 17b.6
// to 17b.9). The file is GET /api/profiles/server/{id}: today's server document
// plus `instance`, the instance that runs it, and the save and the delete go to
// the same route.
import { useCallback, useEffect, useId, useRef, useState } from 'react'
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
import { Icon } from './Icon.tsx'
import { InstanceLink } from './Instances.tsx'
import PageHeading from './PageHeading.tsx'
import { modelProfileIdUnreadable } from './route.ts'
import SignIn from './SignIn.tsx'
import ServerForm, {
  type Runs,
  type ServerDoc,
  withholdsKey,
} from './ServerForm.tsx'
import { CardBox, Line, Loading, LoadingText } from './Skeleton.tsx'
import type { Session, SignedOut } from './session.ts'
import { t } from './strings.ts'

/** What the read gave: the document and who runs it, or the key of the message
 * saying why not. */
type Answer =
  | { doc: ServerDoc; instance: string | null }
  | {
      error:
        | 'client-not-authorized'
        | 'client-unavailable'
        | 'client-no-server-profile'
        | 'failed'
    }
  | 'signed-out'
  | 'revoked'

/** The server document of the instance that runs the profile, as the form reads
 * it (fe/briefs/01-server.md, section 2.5). */
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

/** The document's `instance`: the instance that runs the profile, null when the
 * document names none, which is a profile no instance runs (16 17a.5). */
function usedBy(body: Record<string, unknown>): string | null {
  const instance = body.instance
  return typeof instance === 'string' && instance !== '' ? instance : null
}

async function readProfile(id: string): Promise<Answer> {
  try {
    const reply = await call(`/api/profiles/server/${encodeURIComponent(id)}`)
    if (reply.status === 401) return 'signed-out'
    if (sessionRevoked(reply)) return 'revoked'
    if (!reply.ok) {
      const key = apiError(reply.body)?.key
      switch (key) {
        case 'client-not-authorized':
        case 'client-unavailable':
        case 'client-no-server-profile':
          return { error: key }
        default:
          return { error: 'failed' }
      }
    }
    return isServerDoc(reply.body)
      ? { doc: reply.body, instance: usedBy(reply.body) }
      : { error: 'failed' }
  } catch {
    return { error: 'failed' }
  }
}

/** The page's Server form is never restarted from the page: no run control
 * stands here (16 17a.5), so the save's "restart" message stays until the next
 * edit (16 17a.7). */
const NO_RUNS: Runs = { started: 0, settled: 0 }

export default function ServerProfilePage({
  id,
  onSignedOut,
  onRenewed,
  onDirty,
  nameOf,
}: {
  /** The id from the address. */
  id: string
  onSignedOut: (reason: SignedOut) => void
  /** A sign-in in the bar's dialog: true when the edits belong to it and stay. */
  onRenewed: (session: Session) => boolean
  /** Told whether the form holds unsaved changes; false when it goes. */
  onDirty: (dirty: boolean) => void
  /** The name of an instance by its id (16 17f.4): the Used by row says the
   * word the menu uses, not the key. */
  nameOf: (id: string) => string
}) {
  // undefined while the reply is awaited.
  const [answer, setAnswer] = useState<Answer | undefined>(undefined)
  // Raised by the bar's Reload: `reads` starts the read again, `asked` holds the
  // profile the read was asked for.
  const [reads, setReads] = useState(0)
  const asked = useRef<string | null>(null)
  // The sign-ins in the bar's dialog (16 17a.7): `rereads` remounts the form and
  // reads the file again for the new session, `dropped` says the edits went with
  // it, `kept` counts the sign-ins that kept the page (ends the alert).
  const [signIns, setSignIns] = useState({
    rereads: 0,
    dropped: false,
    kept: 0,
  })
  // Whether the form on screen has edits to lose.
  const dirty = useRef(false)
  // The Confirm in place of the delete (16 17b.7 to 17b.9), and the delete's
  // own answers: its status line, its alert, and the sign-in in its dialog.
  const [confirming, setConfirming] = useState(false)
  // True while the delete runs: the group's two buttons go inactive (17b.8).
  const [deleting, setDeleting] = useState(false)
  // A second press while one runs sends nothing.
  const sendingDelete = useRef(false)
  const [deleteStatus, setDeleteStatus] = useState('')
  const [deleteAlert, setDeleteAlert] = useState<{
    text: string
    signIn: boolean
  } | null>(null)
  // The sign-in in the confirm's dialog (16 17b.9).
  const [asking, setAsking] = useState(false)
  // True once the file is gone: the page is the one paragraph of 17b.8.
  const [deleted, setDeleted] = useState(false)
  // The delete's 409 outlives the group: the read again takes the delete with
  // it, so the in-use text stays in a paragraph of its own, after the Used by
  // list (17b review D2).
  const [inUse, setInUse] = useState<string | null>(null)
  // The delete's 404: the page reads again and the focus goes to the paragraph
  // saying the profile is not there (17b review D2).
  const [focusMissing, setFocusMissing] = useState(false)
  const opener = useRef<HTMLButtonElement>(null)

  // The arrival focus (fe/DESIGN.md 2.3, 16 17a.5): the shell raises its focus
  // request on a navigation, and the session read that brings this page is an
  // arrival too, so the page gives its h1 the focus when it opens.
  useEffect(() => {
    document.getElementById('page-heading')?.focus()
  }, [])

  useEffect(() => {
    // `.` and `..` are sent to no read at all (16 17a.1): the page says there is
    // no such server profile, as a 404 would.
    if (modelProfileIdUnreadable(id)) return
    // The read a Reload asked for is the one asked for THIS profile: the mark is
    // spent here, so it cannot outlive the profile that asked for it.
    const reload = asked.current === id
    asked.current = null
    let live = true
    readProfile(id).then((a) => {
      if (!live) return
      if (a === 'signed-out' || a === 'revoked') {
        onSignedOut(a === 'revoked' ? 'revoked' : 'ended')
        return
      }
      setAnswer(a)
      // The read answers the page like a first visit, so the focus goes to the
      // page's h1.
      if (reload) document.getElementById('page-heading')?.focus()
    })
    return () => {
      live = false
    }
  }, [id, onSignedOut, reads])

  const trackDirty = useCallback(
    (d: boolean) => {
      dirty.current = d
      onDirty(d)
    },
    [onDirty],
  )

  /** Reads the file again, the edits discarded: the form gives place to the
   * page's loading status until the read lands. */
  function reload() {
    asked.current = id
    setAnswer(undefined)
    setReads((n) => n + 1)
  }

  /** The file read again beside what is on screen, the page keeping its form
   * and the confirm its alert: the read of a 409 and a 404 answers the row the
   * refusal speaks of (16 17b.9). */
  function reread() {
    setReads((n) => n + 1)
  }

  /** A sign-in in the bar's dialog: the form stays for the same user; for
   * another one its edits are dropped and the file is read again. */
  function renewed(session: Session) {
    const dropped = dirty.current
    const kept = onRenewed(session)
    setSignIns((s) =>
      kept
        ? { ...s, kept: s.kept + 1 }
        : { ...s, rereads: s.rereads + 1, dropped: s.dropped || dropped },
    )
    if (!kept) {
      setAnswer(undefined)
      setReads((n) => n + 1)
    }
    return kept
  }

  // The paragraph of a delete that went through takes the focus (16 17b.8).
  useEffect(() => {
    if (deleted) document.getElementById('server-profile-deleted')?.focus()
  }, [deleted])

  // The paragraph that takes the group's place when its answer outlives it
  // takes the focus, so the focus does not end on the body (17b review D2).
  useEffect(() => {
    if (inUse !== null) {
      document.getElementById('server-profile-in-use')?.focus()
    } else if (focusMissing) {
      document.getElementById('server-profile-missing')?.focus()
    }
  }, [inUse, focusMissing, answer])

  /** Cancel, or Escape: the group goes and the focus returns to the opener;
   * Escape is inert while the delete runs (16 17b.7). While the delete runs
   * nothing closes: the group, its question and its status line stay as they
   * are and the focus does not move (17b review R1). */
  function closeConfirm() {
    if (sendingDelete.current) return
    setConfirming(false)
    setDeleteAlert(null)
    setDeleteStatus('')
    setInUse(null)
    opener.current?.focus()
  }

  /** The delete (16 17b.8): `DELETE` of the profile's file, its answers
   * 16 17b.9. */
  async function doDelete() {
    if (sendingDelete.current) return
    sendingDelete.current = true
    setDeleting(true)
    setDeleteAlert(null)
    setInUse(null)
    setFocusMissing(false)
    setDeleteStatus(t('confirm-deleting'))
    let answer: Reply | undefined
    try {
      answer = await call(`/api/profiles/server/${encodeURIComponent(id)}`, {
        method: 'DELETE',
      })
    } catch {
      answer = undefined
    }
    sendingDelete.current = false
    setDeleting(false)
    setDeleteStatus('')
    if (answer === undefined) {
      setDeleteAlert({ text: t('request-failed'), signIn: false })
      return
    }
    if (answer.ok) {
      // The file is gone: the page is the paragraph saying so (16 17b.8).
      setDeleted(true)
      return
    }
    if (sessionEnded(answer)) {
      setDeleteAlert({ text: t('session-ended-retry'), signIn: true })
      return
    }
    if (sessionRevoked(answer)) {
      setDeleteAlert({ text: t('session-revoked'), signIn: false })
      return
    }
    const error = apiError(answer.body)
    if (error !== null && error.key === 'client-server-profile-in-use') {
      // An instance took the profile while the page was open: the alert says
      // so, and the read again names that instance (16 17b.9). The read again
      // takes the delete and its group with it, so the words go on living in a
      // paragraph of their own, which takes the focus (17b review D2).
      const text = t('api-client-server-profile-in-use', ...error.args)
      setDeleteAlert({ text, signIn: false })
      setInUse(text)
      reread()
      return
    }
    if (error !== null && error.key === 'client-no-server-profile') {
      // The profile is gone: the read again shows the page that says so, and
      // that page's paragraph takes the focus (17b review D2).
      setDeleteAlert({ text: t('api-client-no-server-profile'), signIn: false })
      setFocusMissing(true)
      reread()
      return
    }
    if (error !== null && error.key === 'client-timeout') {
      // The manager got the delete and did not answer in time: it may or may
      // not have been removed, so the document is read again to show what the
      // manager holds (16 17f.10).
      setDeleteAlert({ text: t('delete-unknown'), signIn: false })
      reread()
      return
    }
    setDeleteAlert({
      text:
        error?.key === 'client-unavailable'
          ? t('api-client-unavailable')
          : t('request-failed'),
      signIn: false,
    })
  }

  /** A sign-in in the confirm's dialog (16 17b.9): the page keeps what it
   * holds, the group stays open and the focus returns to the verb; nothing is
   * sent again. */
  function signedIn(session: Session) {
    const kept = renewed(session)
    flushSync(() => {
      setAsking(false)
      if (kept) setDeleteAlert(null)
    })
    if (!kept) return
    document.getElementById('confirm-delete')?.focus()
  }

  function closeAsking() {
    flushSync(() => setAsking(false))
    document.getElementById('confirm-delete')?.focus()
  }

  if (deleted) {
    // The file is gone: the page is the one paragraph that says so, focused,
    // and the link back to the list (16 17b.8). The form went with the profile,
    // so the page asks nothing about unsaved changes any more, and no message
    // travels to the list.
    return (
      <>
        <PageHeading text={id} />
        <p id="server-profile-deleted" tabIndex={-1}>
          {t('server-profile-deleted', id)}
        </p>
        <p>
          <a href="#/profiles/server">{t('server-profile-back')}</a>
        </p>
      </>
    )
  }
  if (modelProfileIdUnreadable(id)) {
    return (
      <>
        <Back />
        <PageHeading text={id} />
        <p>{t('server-profile-not-found', id)}</p>
      </>
    )
  }
  const read =
    answer !== undefined && typeof answer === 'object' && 'doc' in answer
      ? answer
      : null
  const missing =
    answer !== undefined &&
    typeof answer === 'object' &&
    !('doc' in answer) &&
    answer.error === 'client-no-server-profile'
  return (
    <>
      <Back />
      <PageHeading text={id} />
      {missing ? (
        // No profile by that name: the message, and nothing else (16 17a.6).
        // The paragraph is a place the focus can take: the read again after a
        // delete lands here and the group is gone (17b review D2).
        <p id="server-profile-missing" tabIndex={-1}>
          {t('server-profile-not-found', id)}
        </p>
      ) : (
        <>
          {read !== null && (
            <dl className="assets">
              <dt>{t('server-profile-used-by')}</dt>
              {read.instance === null ? (
                <dd className="muted">{t('server-profiles-unused')}</dd>
              ) : (
                // The name the menu uses, then the id it is the key of (16 17f.4).
                <dd>
                  <InstanceLink id={read.instance} nameOf={nameOf} />
                </dd>
              )}
            </dl>
          )}
          {inUse !== null && (
            <p
              id="server-profile-in-use"
              role="alert"
              tabIndex={-1}
              className="alert"
            >
              {inUse}
            </p>
          )}
          {/* A profile no instance runs is deleted from its own page, the
              delete asked in the page (16 17b.6, fe/DESIGN.md 5.2). */}
          {read !== null && read.instance === null && (
            <>
              <button
                ref={opener}
                type="button"
                id="server-profile-delete"
                className="secondary"
                aria-expanded={confirming}
                onClick={() => setConfirming(true)}
              >
                {t('server-profile-delete')}
              </button>
              {confirming && (
                <ConfirmDelete
                  id={id}
                  status={deleteStatus}
                  alert={deleteAlert}
                  running={deleting}
                  onConfirm={() => void doDelete()}
                  onCancel={closeConfirm}
                  onSignIn={() => setAsking(true)}
                />
              )}
            </>
          )}
          <section>
            <h2>{t('server-title')}</h2>
            <p className="intro">
              {t('server-profile-intro', `server@${id}.ini`)}
            </p>
            {signIns.dropped && (
              <p role="alert" className="alert">
                {t('server-edits-dropped')}
              </p>
            )}
            <Body
              key={signIns.rereads}
              id={id}
              answer={answer}
              renewals={signIns.kept}
              onDirty={trackDirty}
              onReload={reload}
              onRenewed={renewed}
            />
          </section>
        </>
      )}
      {/* The sign-in the confirm's alert offers (16 17b.9): the page and its
          group stay behind it. */}
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

/** The confirm in place of the delete (fe/DESIGN.md 5.2, 16 17b.7 to 17b.9):
 *  the question that names the profile, the verb and Cancel, the status line
 *  and the alert place. It is mounted while the delete is asked, so its words
 *  and its alert are its own: closing it clears all of them. */
function ConfirmDelete({
  id,
  status,
  alert,
  running,
  onConfirm,
  onCancel,
  onSignIn,
}: {
  /** The profile's id, named in the question. */
  id: string
  status: string
  alert: { text: string; signIn: boolean } | null
  /** True while the delete runs: both buttons go inactive, Escape is inert. */
  running: boolean
  onConfirm: () => void
  onCancel: () => void
  onSignIn: () => void
}) {
  const uid = useId()
  const questionId = `${uid}-question`

  // The focus moves to Cancel, the verb is one Tab away (16 17b.7).
  useEffect(() => {
    document.getElementById('confirm-cancel')?.focus()
  }, [])

  return (
    <div
      role="group"
      className="confirm-group"
      aria-labelledby={questionId}
      onKeyDown={(e) => {
        if (e.key === 'Escape' && !running) onCancel()
      }}
    >
      <p id={questionId}>{t('server-profile-delete-question', id)}</p>
      <div className="save-actions">
        <button
          type="button"
          id="confirm-delete"
          className="primary"
          aria-disabled={running || undefined}
          onClick={onConfirm}
        >
          {t('confirm-delete')}
        </button>
        <button
          type="button"
          id="confirm-cancel"
          className="secondary"
          aria-disabled={running || undefined}
          onClick={onCancel}
        >
          {t('confirm-cancel')}
        </button>
      </div>
      <p role="status" className="save-status" aria-live="polite">
        {status}
      </p>
      {alert !== null && <p role="alert">{alert.text}</p>}
      {alert?.signIn && (
        <button
          type="button"
          id="session-sign-in"
          className="secondary"
          onClick={onSignIn}
        >
          {t('session-sign-in')}
        </button>
      )}
    </div>
  )
}

/** The link back to the list, before the h1 (16 17a.5, as `model-profile-back`). */
function Back() {
  return (
    <p className="model-profile-back">
      <a href="#/profiles/server">
        <Icon name="chevron-left" />
        {t('server-profile-back')}
      </a>
    </p>
  )
}

/** The Server section's content: the form, the loading shape, or the message. */
function Body({
  id,
  answer,
  renewals,
  onDirty,
  onReload,
  onRenewed,
}: {
  id: string
  answer: Answer | undefined
  /** Raised by every sign-in that kept the page: a session-ended alert goes. */
  renewals: number
  onDirty: (dirty: boolean) => void
  onReload: () => void
  onRenewed: (session: Session) => boolean
}) {
  if (answer === undefined) {
    // The shape of the page to come (16 17a.6): the Used by row, and the card
    // the form fills.
    return (
      <Loading
        text={t('server-loading')}
        shape={
          <>
            <Line />
            <CardBox />
          </>
        }
      />
    )
  }
  if (answer === 'signed-out' || answer === 'revoked') {
    return <LoadingText text={t('server-loading')} />
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
        runs={NO_RUNS}
        onDirty={onDirty}
        onReload={onReload}
        onRenewed={onRenewed}
      />
    )
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
