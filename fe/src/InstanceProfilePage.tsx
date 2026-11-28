// One instance profile's page (#/profiles/instance/<id>, signed in). Brief 26:
// 26.5 is its head, the link back to the list, the id as the h1 and the row of
// the instances that run it; 26.6 is the Server profile section, the profile it
// runs as a link to that profile and the switch to every other profile of the
// pool; 26.7 and 26.8 are the Model profiles section, the entries in the file's
// order with their placement count, the add of a model profile the node has and
// the remove of one; and 26.12 is the restart offer a write leaves when its
// answer names the instances running the profile. The wire is
// fe/briefs/api-contract.md IP4, whose three writes carry no revision. Every
// text is in strings/en_US.json.
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
  type Reply,
  sessionEnded,
  sessionRevoked,
} from './api.ts'
import { FieldError, Hint } from './Field.tsx'
import { Icon } from './Icon.tsx'
import {
  InstanceLink,
  readPool,
  type NameOf,
  type PoolRow,
} from './Instances.tsx'
import PageHeading from './PageHeading.tsx'
import { instanceProfileModelHref, serverProfileHref } from './route.ts'
import { RestartOffer } from './RestartOffer.tsx'
import { ConfirmRow, RowDelete } from './RowDelete.tsx'
import { SignInDialog } from './SignIn.tsx'
import { LoadingText } from './Skeleton.tsx'
import type { Session, SignedOut } from './session.ts'
import { t } from './strings.ts'

/** One model entry of the profile: its id, and how many placement keys it has
 *  (26.7's Placement column). */
type Entry = { id: string; keys: number }

/** What one profile's read gave: the head's facts and the two sections' file,
 *  a message, or the session. */
type Answer =
  | { server: string; models: Entry[]; usedBy: string[] }
  /** 404 `no-instance-profile-named`: the page says so (26.5). */
  | 'missing'
  | 'failed'
  | 'signed-out'
  | 'revoked'

/** The pool of one section's select: the rows, or the words of a read that
 *  failed, with the Sign in when it says the session is gone. */
type Pool = { ids: string[] } | { failed: string; signIn: boolean }

/** A write's refusal: its words, where they go (the field it names, with the
 *  field's name, or the section's alert), whether a Sign in belongs to them,
 *  and whether the file is read again. */
type Refusal = {
  text: string
  signIn: boolean
  /** The field the reply names, and the field of the section that shows it. */
  field: string | null
  /** Section 3's 409 `stale-revision`: the words say the page has read the
   *  file again, so it does. */
  reread: boolean
}

/** The answer of one of the page's three writes: the ids the answer says run
 *  the profile (26.12's offer), or the refusal with its words. */
type Written = { ok: true; running: string[] } | ({ ok: false } & Refusal)

/** A request that did not go through. */
const FAILED: Refusal = {
  text: t('request-failed'),
  signIn: false,
  field: null,
  reread: false,
}

/** The session's two answers, shared by every write (api-contract.md 3). */
function sessionRefusal(reply: Reply): Refusal | null {
  if (sessionEnded(reply))
    return {
      text: t('session-ended-retry'),
      signIn: true,
      field: null,
      reread: false,
    }
  if (sessionRevoked(reply))
    return {
      text: t('session-revoked'),
      signIn: false,
      field: null,
      reread: false,
    }
  return null
}

/** The refusals of one write: for each api key it can answer with, the words
 *  (the key named in the code, as the strings rule asks) and the field of the
 *  section it names, '' when it names none. `reread` is a refusal that says the
 *  record the page shows is not the manager's: the page reads it again, so the
 *  stale row or the stale option goes (section 3). */
type RefusalKeys = Record<
  string,
  { words: (args: string[]) => string; field: string; reread?: boolean }
>

/** The answer of a write that was refused: the words of the keys of 26.6 and
 *  26.8, each with the field of the page it marks, and the generic words for
 *  the rest. `profile` is the id in the address: a reply that says the record
 *  of the address is gone carries no args, so the page names it. */
function refusalOf(profile: string, reply: Reply, keys: RefusalKeys): Refusal {
  const refusal = apiError(reply.body)
  const key = refusal?.key ?? ''
  const named = keys[key]
  const text =
    key === 'no-instance-profile-named'
      ? t('api-no-instance-profile-named', profile)
      : named !== undefined
        ? named.words(refusalArgs(refusal))
        : reply.status === 422 && key !== ''
          ? t('api-other', key)
          : t('request-failed')
  return {
    text,
    signIn: false,
    reread: named?.reread === true,
    field:
      named !== undefined &&
      named.field !== '' &&
      isRecord(reply.body) &&
      reply.body.field === named.field
        ? named.field
        : null,
  }
}

const refusalArgs = (refusal: { args: string[] } | null): string[] =>
  refusal?.args ?? []

/** The ids the answer says run the profile (26.12). */
function runningIds(body: unknown): string[] {
  if (!isRecord(body) || !Array.isArray(body.running)) return []
  return body.running.filter((id): id is string => typeof id === 'string')
}

/** One write of the contract: the path, the method, the body the path takes
 *  (none for the delete), and the keys of its refusals. */
async function write(
  path: string,
  method: 'PUT' | 'POST' | 'DELETE',
  body: Record<string, string> | null,
  profile: string,
  keys: RefusalKeys,
): Promise<Written> {
  try {
    const reply = await call(path, {
      method,
      ...(body === null
        ? {}
        : {
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify(body),
          }),
    })
    const gone = sessionRefusal(reply)
    if (gone !== null) return { ok: false, ...gone }
    // The three writes carry no revision (section 3, the coordinator's ruling
    // after api-contract.md IP4), so no answer here is a `stale-revision`: the
    // refusals that say the page is behind are the keyed ones of `keys`.
    if (!reply.ok) return { ok: false, ...refusalOf(profile, reply, keys) }
    return { ok: true, running: runningIds(reply.body) }
  } catch {
    return { ok: false, ...FAILED }
  }
}

/** The path of one profile, and of its three model routes (IP4). */
const profilePath = (profile: string) =>
  `/api/profiles/instance/${encodeURIComponent(profile)}`
const modelsPath = (profile: string) => `${profilePath(profile)}/models`
const modelPath = (profile: string, model: string) =>
  `${modelsPath(profile)}/${encodeURIComponent(model)}`

/** The count of an entry's placement keys: a reply that names none has none. */
function count(value: unknown): number {
  return typeof value === 'number' && value > 0 ? Math.trunc(value) : 0
}

async function readProfile(id: string): Promise<Answer> {
  try {
    const reply = await call(profilePath(id))
    if (sessionEnded(reply)) return 'signed-out'
    if (sessionRevoked(reply)) return 'revoked'
    if (!reply.ok) {
      return apiError(reply.body)?.key === 'no-instance-profile-named'
        ? 'missing'
        : 'failed'
    }
    const body = reply.body
    if (!isRecord(body)) return 'failed'
    const usedBy = Array.isArray(body.usedBy)
      ? body.usedBy.filter((id): id is string => typeof id === 'string')
      : []
    const models = Array.isArray(body.models)
      ? body.models.flatMap((entry) =>
          isRecord(entry) && typeof entry.id === 'string'
            ? [{ id: entry.id, keys: count(entry.keys) }]
            : [],
        )
      : []
    return {
      server: typeof body.server === 'string' ? body.server : '',
      models,
      usedBy,
    }
  } catch {
    return 'failed'
  }
}

/** The model profiles of the node, for the add's select. */
async function readModelPool(): Promise<Pool> {
  try {
    const reply = await call('/api/profiles/model')
    if (sessionEnded(reply))
      return { failed: t('session-ended-retry'), signIn: true }
    if (sessionRevoked(reply))
      return { failed: t('session-revoked'), signIn: false }
    if (!reply.ok || !Array.isArray(reply.body))
      return { failed: t('instance-profile-models-load-failed'), signIn: false }
    return {
      ids: reply.body.flatMap((row) =>
        isRecord(row) && typeof row.id === 'string' ? [row.id] : [],
      ),
    }
  } catch {
    return { failed: t('instance-profile-models-load-failed'), signIn: false }
  }
}

/** The DOM id prefix of one entry's row: after an add or a remove the focus
 *  moves to a row's link, and a row's − keeps its own (26.8). */
const rowId = (model: string) => `model-${encodeURIComponent(model)}`

/** The DOM id of the add: the focus lands on it when the last row goes (26.8). */
const ADD_ID = 'instance-profile-models-add'

export default function InstanceProfilePage({
  id,
  nameOf,
  onSignedOut,
  onRenewed,
}: {
  /** The id in the route: the h1, and the path of the read. */
  id: string
  /** The name of an instance by its id: the Used by row says the word the
   *  menu uses, with the shared-name rule of 22 D4.37. */
  nameOf: NameOf
  /** A read answered 401, or a session read revoked. */
  onSignedOut: (reason: SignedOut) => void
  /** A sign-in one section's dialog answers: true when the page kept it. */
  onRenewed: (session: Session) => boolean
}) {
  // undefined while the reply is awaited.
  const [answer, setAnswer] = useState<Answer | undefined>(undefined)
  // The model profiles the add lists: null while the read is awaited.
  const [pool, setPool] = useState<Pool | null>(null)
  // The sign-ins the page kept: each section clears the alert the ended session
  // left, and reads what it lists again.
  const [renewals, setRenewed] = useState(0)

  useEffect(() => {
    let live = true
    readProfile(id).then((a) => {
      if (!live) return
      if (a === 'signed-out' || a === 'revoked')
        onSignedOut(a === 'revoked' ? 'revoked' : 'ended')
      else setAnswer(a)
    })
    return () => {
      live = false
    }
  }, [id, onSignedOut])

  // The pool of the add's select, at the opening and after each sign-in: the
  // model profiles are the manager's, never the page's.
  useEffect(() => {
    let live = true
    readModelPool().then((p) => {
      if (live) setPool(p)
    })
    return () => {
      live = false
    }
  }, [renewals])

  /** The file read again after a write, so the sections show what the manager
   *  holds (section 3). The render is flushed: a section moves the focus to a
   *  row the read brought, and that row must be on screen when it does. This
   *  moves no focus itself: a write's focus is its own. The answer is false when
   *  the read brought no record back (a 503, a network fault): the write may
   *  have worked, so the page keeps the record it shows, and the section that
   *  asked says its read failed (section 3, the review). */
  const reread = useCallback(async (): Promise<boolean> => {
    const a = await readProfile(id)
    if (a === 'signed-out' || a === 'revoked') {
      onSignedOut(a === 'revoked' ? 'revoked' : 'ended')
      return false
    }
    if (a === 'failed') return false
    flushSync(() => setAnswer(a))
    return true
  }, [id, onSignedOut])

  return (
    <>
      {/* The link back to the list, before the h1 (26.5, as the server
          profile's page does, 16 17a.5). */}
      <p className="model-profile-back">
        <a href="#/profiles/instance">
          <Icon name="chevron-left" />
          {t('instance-profile-back')}
        </a>
      </p>
      {/* The id alone as the h1 (26.5): the id is the profile's name. */}
      <PageHeading text={id} />
      {answer === undefined ? (
        <LoadingText text={t('instance-profiles-loading')} />
      ) : typeof answer === 'object' ? (
        <>
          <dl className="assets">
            <dt>{t('instance-profiles-column-used-by')}</dt>
            {answer.usedBy.length === 0 ? (
              <dd className="muted">{t('instance-profiles-unused')}</dd>
            ) : (
              <dd>
                {answer.usedBy.map((used, n) => (
                  <span key={used}>
                    {n > 0 && ', '}
                    <InstanceLink id={used} nameOf={nameOf} />
                  </span>
                ))}
              </dd>
            )}
          </dl>
          <ServerSection
            profile={id}
            server={answer.server}
            nameOf={nameOf}
            renewals={renewals}
            onRenewed={onRenewed}
            onKept={() => setRenewed((n) => n + 1)}
            onChanged={reread}
          />
          <ModelSection
            profile={id}
            models={answer.models}
            pool={pool}
            nameOf={nameOf}
            renewals={renewals}
            onRenewed={onRenewed}
            onKept={() => setRenewed((n) => n + 1)}
            onChanged={reread}
          />
        </>
      ) : answer === 'missing' ? (
        // The read says the profile is not there: the page keeps the back link
        // and the id, and says so (26.5).
        <p>{t('instance-profile-missing', id)}</p>
      ) : answer === 'failed' ? (
        <p>{t('instance-profiles-failed')}</p>
      ) : (
        <LoadingText text={t('instance-profiles-loading')} />
      )}
    </>
  )
}

/** The sign-in dialog of one section: the Sign in that opens it gives the focus
 *  back on the close, and a sign-in the dialog kept is the page's to keep, so
 *  the section clears the words the ended session left and reads what it lists
 *  again. */
function useSectionSignIn(
  onRenewed: (session: Session) => boolean,
  onKept: () => void,
) {
  const [asking, setAsking] = useState(false)
  // The Sign in that opened the dialog.
  const fromId = useRef('')
  const back = () => document.getElementById(fromId.current)?.focus()
  return {
    asking,
    /** The Sign in with this DOM id opens the dialog. */
    ask: (controlId: string) => {
      fromId.current = controlId
      setAsking(true)
    },
    close: () => {
      flushSync(() => setAsking(false))
      back()
    },
    signedIn: (session: Session) => {
      const kept = onRenewed(session)
      flushSync(() => {
        setAsking(false)
        if (kept) onKept()
      })
      if (kept) back()
    },
  }
}

/** The section of 26.6: the server profile the instance profile runs, as a link
 *  to that profile, and the switch to every other profile of the pool. The
 *  select alone sends nothing: the button writes the choice. A refusal that
 *  names the field marks the select, says its words under it and takes the
 *  focus there; every other one goes to the section's alert, with a Sign in
 *  when the session ended. A 200 that names running instances leaves the
 *  restart offer (26.12). */
function ServerSection({
  profile,
  server,
  nameOf,
  renewals,
  onRenewed,
  onKept,
  onChanged,
}: {
  profile: string
  /** The server profile the file says it runs, '' when it names none. */
  server: string
  nameOf: NameOf
  renewals: number
  onRenewed: (session: Session) => boolean
  /** The page kept a sign-in this section's dialog answered. */
  onKept: () => void
  /** The profile read again: false when the read brought no record back. */
  onChanged: () => Promise<boolean>
}) {
  const fieldId = useId()
  // The server profile the section shows it runs. It starts as the file says
  // and a switch sets it to the profile chosen: the write is the section's own
  // knowledge, and the read that follows it shows the same.
  const [runs, setRuns] = useState(server)
  // The pool: null while the read is awaited, and the words of one that failed.
  const [rows, setRows] = useState<PoolRow[] | null>(null)
  const [failed, setFailed] = useState<{
    text: string
    signIn: boolean
  } | null>(null)
  const [chosen, setChosen] = useState('')
  const [busy, setBusy] = useState(false)
  // The section's one status line, its alert, the words under the select, and
  // the instances the last switch said run this profile (26.12).
  const [status, setStatus] = useState('')
  const [alert, setAlert] = useState<{ text: string; signIn: boolean } | null>(
    null,
  )
  const [error, setError] = useState('')
  const [offer, setOffer] = useState<string[]>([])
  const signIn = useSectionSignIn(onRenewed, onKept)

  useEffect(() => {
    let live = true
    readPool().then((answer) => {
      if (!live) return
      if ('failed' in answer) {
        setRows(null)
        setFailed({ text: answer.failed, signIn: answer.signIn })
      } else {
        setRows(answer.rows)
        setFailed(null)
      }
    })
    return () => {
      live = false
    }
  }, [renewals])

  const free = (rows ?? []).filter((row) => row.id !== runs)
  const canSwitch = !busy && chosen !== ''

  const send = async () => {
    // The button is switched off while the write runs and until there is a
    // choice: a press of such a button sends nothing.
    if (!canSwitch) return
    const chosenId = chosen
    setBusy(true)
    setAlert(null)
    setError('')
    setOffer([])
    setStatus(t('instance-profile-server-switching'))
    const result = await write(
      `${profilePath(profile)}/server`,
      'PUT',
      { server: chosenId },
      profile,
      {
        'no-server-profile-named': {
          words: (args) => t('api-no-server-profile-named', ...args),
          field: 'server',
        },
        // R9 (26.6): the device rule here names the model, the key and the
        // device, no instance: this page chooses the server profile, so the
        // words say "the chosen server profile".
        'model-profile-device-outside': {
          words: (args) => t('api-instance-profile-device-outside', ...args),
          field: '',
        },
      },
    )
    if (result.ok) {
      setRuns(chosenId)
      setStatus(t('instance-profile-server-switched', chosenId))
      setChosen('')
      setOffer(result.running)
      if (!(await onChanged())) {
        // The write worked and the read did not: the section keeps what it
        // shows and says its read failed (section 3, the review).
        setAlert({ text: t('request-failed'), signIn: false })
      }
    } else {
      setStatus('')
      if (result.field === 'server') {
        // The refusal names the select: its words go under it, and the focus
        // moves there once they are rendered.
        flushSync(() => setError(result.text))
        document.getElementById(`${fieldId}-control`)?.focus()
      } else {
        setAlert({ text: result.text, signIn: result.signIn })
      }
    }
    setBusy(false)
  }

  return (
    <section>
      <h2>{t('instance-profile-server-title')}</h2>
      {/* The profile it runs, as a link to that profile (26.6). */}
      {runs === '' ? (
        <p>{t('instance-server-none')}</p>
      ) : (
        <p>
          <a href={serverProfileHref(runs)}>{runs}</a>
        </p>
      )}
      {failed !== null ? (
        <>
          <p>{failed.text}</p>
          {failed.signIn && (
            <button
              type="button"
              id={`${fieldId}-pool-sign-in`}
              className="secondary"
              onClick={() => signIn.ask(`${fieldId}-pool-sign-in`)}
            >
              {t('session-sign-in')}
            </button>
          )}
        </>
      ) : rows === null || free.length === 0 ? null : (
        <div className="field">
          <div className="field-label">
            <label htmlFor={`${fieldId}-control`}>
              {t('instance-profile-server-switch-label')}
            </label>
          </div>
          <div className="field-control">
            <select
              id={`${fieldId}-control`}
              value={chosen}
              aria-disabled={busy || undefined}
              aria-invalid={error === '' ? undefined : true}
              aria-describedby={
                error === ''
                  ? `${fieldId}-hint`
                  : `${fieldId}-hint ${fieldId}-error`
              }
              onChange={(e) => {
                // The choice alone sends nothing.
                setChosen(e.target.value)
                setError('')
              }}
            >
              <option value="">{t('instance-profile-server-choose')}</option>
              {free.map((row) => (
                // R8 (26.6): the id alone. The port belongs to an instance
                // since IP6, and this profile has none: naming one here would
                // name a port the profile does not have.
                <option key={row.id} value={row.id}>
                  {row.id}
                </option>
              ))}
            </select>
            <button
              type="button"
              id={`${fieldId}-switch`}
              className="secondary"
              aria-disabled={canSwitch ? undefined : 'true'}
              onClick={() => void send()}
            >
              {t('instance-profile-server-switch')}
            </button>
          </div>
          <Hint
            id={`${fieldId}-hint`}
            text={t('instance-profile-server-switch-hint')}
            open
          />
          {error !== '' && (
            <FieldError id={`${fieldId}-error`} text={error} alert />
          )}
        </div>
      )}
      {/* The section's one status line, and its alert, with the Sign in the
          session's end asks for after it (26.6). */}
      <p role="status" className="save-status">
        {status}
      </p>
      {alert !== null && <p role="alert">{alert.text}</p>}
      {alert?.signIn && (
        <button
          type="button"
          id={`${fieldId}-sign-in`}
          className="secondary"
          onClick={() => signIn.ask(`${fieldId}-sign-in`)}
        >
          {t('session-sign-in')}
        </button>
      )}
      <RestartOffer
        ids={offer}
        nameOf={nameOf}
        renewals={renewals}
        onChanged={onChanged}
        onSignIn={signIn.ask}
      />
      {signIn.asking && (
        <SignInDialog onSignedIn={signIn.signedIn} onClose={signIn.close} />
      )}
    </section>
  )
}

/** The section of 26.7 and 26.8: the model entries in the file's order, each row
 *  the id as a link to this profile's entry page, its Placement cell the count
 *  of its keys, and its minus; the add of a model profile the node has and the
 *  remove of one, each leaving the restart offer when the answer names running
 *  instances (26.12). */
function ModelSection({
  profile,
  models,
  pool,
  nameOf,
  renewals,
  onRenewed,
  onKept,
  onChanged,
}: {
  profile: string
  models: Entry[]
  /** The model profiles of the node, null while the read is awaited. */
  pool: Pool | null
  nameOf: NameOf
  renewals: number
  onRenewed: (session: Session) => boolean
  /** The page kept a sign-in this section's dialog answered. */
  onKept: () => void
  /** The profile read again: false when the read brought no record back. */
  onChanged: () => Promise<boolean>
}) {
  const fieldId = useId()
  // The add: open, the model chosen, the words under the select, the add in
  // flight.
  const [adding, setAdding] = useState(false)
  const [chosen, setChosen] = useState('')
  const [error, setError] = useState('')
  const [sending, setSending] = useState(false)
  // The add in flight, read by the press. A ref, set before the write is sent:
  // the state lands on the next render, so two presses in one go would each see
  // `sending` false and send the same model twice (the review's fourth
  // finding, as the switch has it for its button, 26.6).
  const inFlight = useRef(false)
  // The remove: the row asked about, the delete in flight, and the refusal the
  // confirm answers with (22 D4.7).
  const [removing, setRemoving] = useState<string | null>(null)
  const [deleting, setDeleting] = useState(false)
  const [confirmAlert, setConfirmAlert] = useState<{
    text: string
    signIn: boolean
  } | null>(null)
  // The section's one status line, its alert, and the instances the last write
  // said run this profile (26.12).
  const [status, setStatus] = useState('')
  const [alert, setAlert] = useState<{ text: string; signIn: boolean } | null>(
    null,
  )
  const [offer, setOffer] = useState<string[]>([])
  const signIn = useSectionSignIn(onRenewed, onKept)

  const listed = models.map((entry) => entry.id)
  // The add lists the node's model profiles the profile does not list yet
  // (26.8), in the pool's order.
  const left =
    pool !== null && 'ids' in pool
      ? pool.ids.filter((id) => !listed.includes(id))
      : []

  /** The add's Cancel, and Escape: the form goes with its choice and its
   *  check's words. */
  function closeAdd() {
    if (sending) return
    setAdding(false)
    setChosen('')
    setError('')
  }

  /** The add: the model chosen goes to the record, the form closes, and the
   *  focus lands on the row the read gives back (26.8). */
  const add = async () => {
    if (inFlight.current) return
    if (chosen === '') {
      // A page check: the select is marked, described by the words, and takes
      // the focus once they are rendered; nothing is sent.
      flushSync(() => setError(t('field-error-choose')))
      document.getElementById(`${fieldId}-add-control`)?.focus()
      return
    }
    const model = chosen
    inFlight.current = true
    setSending(true)
    setAlert(null)
    setStatus('')
    setOffer([])
    const result = await write(
      modelsPath(profile),
      'POST',
      { model },
      profile,
      {
        'no-model-profile-named': {
          words: (args) => t('api-no-model-profile-named', ...args),
          field: 'model',
        },
        'instance-profile-model-listed': {
          words: (args) => t('api-instance-profile-model-listed', ...args),
          field: '',
          // The review: the 409 says the entry is on the manager after all, so
          // the read shows the file and the open form stops listing it.
          reread: true,
        },
      },
    )
    if (result.ok) {
      setAdding(false)
      setChosen('')
      setError('')
      setStatus(t('instance-profile-model-added', model))
      setOffer(result.running)
      const kept = await onChanged()
      if (kept) {
        // The read is in and rendered: the focus goes to the link of the row it
        // added (26.8).
        document.getElementById(`${rowId(model)}-link`)?.focus()
      } else {
        // The write worked and the read did not: the page keeps the rows it
        // shows and this section says its read failed (section 3, the review).
        setAlert({ text: t('request-failed'), signIn: false })
      }
    } else {
      if (result.field === 'model') {
        // The refusal names the select: it is marked, described, and takes the
        // focus; the form stays with its choice.
        flushSync(() => setError(result.text))
        document.getElementById(`${fieldId}-add-control`)?.focus()
      } else {
        setAlert({ text: result.text, signIn: result.signIn })
      }
      // Section 3: the 409 that says the entry is on the manager leaves the
      // record read again, so the option the file already lists goes out of the
      // open form. The refusal's words stay: they are what the manager answered.
      if (result.reread && !(await onChanged()))
        setAlert((now) => now ?? { text: t('request-failed'), signIn: false })
    }
    inFlight.current = false
    setSending(false)
  }

  /** The remove of `model`: the row leaves with the read, and the focus goes to
   *  the next row's link, the add when it was the last (26.8). */
  const remove = async (model: string) => {
    if (deleting) return
    const next = listed[listed.indexOf(model) + 1] ?? null
    setDeleting(true)
    setAlert(null)
    setStatus('')
    setOffer([])
    setConfirmAlert(null)
    const result = await write(
      modelPath(profile, model),
      'DELETE',
      null,
      profile,
      {
        'no-instance-profile-model': {
          words: (args) => t('api-no-instance-profile-model', ...args),
          field: '',
          // The review: the 404 says the entry is not on the manager, so the
          // row on the screen is stale: the read shows the file.
          reread: true,
        },
      },
    )
    if (result.ok) {
      setRemoving(null)
      setStatus(t('instance-profile-model-removed', model))
      setOffer(result.running)
      const kept = await onChanged()
      if (kept) {
        // The read is in and rendered: the focus goes to the row that follows
        // the one that went, and to the add when the list is empty now (26.8).
        const to = next
          ? document.getElementById(`${rowId(next)}-link`)
          : document.getElementById(ADD_ID)
        to?.focus()
      } else {
        // The write worked and the read did not: the page keeps the rows it
        // shows and this section says its read failed (section 3, the review).
        setAlert({ text: t('request-failed'), signIn: false })
      }
    } else if (result.reread) {
      // The 404 says the entry is not on the manager: the words go on the
      // section's own line, where they stay when the read takes the row the
      // confirm asks about, and the record is read again (section 3).
      setAlert({ text: result.text, signIn: result.signIn })
      setConfirmAlert(null)
      if (!(await onChanged()))
        setAlert((now) => now ?? { text: t('request-failed'), signIn: false })
    } else {
      // The confirm keeps the words where the remove was asked, with a Sign in
      // when the session ended (22 D4.7).
      setConfirmAlert({ text: result.text, signIn: result.signIn })
    }
    setDeleting(false)
  }

  /** The minus: the confirm opens in place after the row. The line above the
   *  table is this remove's too, so an older refusal's words go (22 D4
   *  review D4). */
  function openRemove(model: string) {
    if (deleting) return
    setRemoving(model)
    setConfirmAlert(null)
    setStatus('')
    setAlert(null)
  }

  /** Cancel, Escape, or the minus pressed again: the confirm goes and the focus
   *  returns to the minus (22 D4.4). */
  function closeRemove(model: string) {
    if (deleting) return
    flushSync(() => {
      setRemoving(null)
      setConfirmAlert(null)
    })
    document.getElementById(`${rowId(model)}-minus`)?.focus()
  }

  const showTable = models.length > 0

  return (
    <section>
      <h2>{t('instance-profile-models-title')}</h2>
      <p className="intro">{t('instance-profile-models-intro')}</p>
      {/* With no entry the section says so in place of a table (26.7), the add
          following the line. The row of a remove leaving goes with the read, so
          an emptied list leaves this line and no table of heads over nothing
          (section 3, the review). */}
      {!showTable && <p>{t('instance-profile-models-empty')}</p>}
      {showTable && (
        <div className="table-scroll">
          <table className="instance-profile-models">
            <thead>
              <tr>
                <th scope="col">{t('instance-profile-models-column-name')}</th>
                <th scope="col">
                  {t('instance-profile-models-column-placement')}
                </th>
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
              {models.map((entry) => (
                <Fragment key={entry.id}>
                  <tr>
                    <td>
                      <a
                        id={`${rowId(entry.id)}-link`}
                        href={instanceProfileModelHref(profile, entry.id)}
                      >
                        {entry.id}
                      </a>
                    </td>
                    <td>
                      {entry.keys === 0 ? (
                        <span className="muted">
                          {t('instance-profile-placement-default')}
                        </span>
                      ) : entry.keys === 1 ? (
                        t('instance-profile-placement-count-one')
                      ) : (
                        t(
                          'instance-profile-placement-count',
                          String(entry.keys),
                        )
                      )}
                    </td>
                    <td>
                      <RowDelete
                        domId={`${rowId(entry.id)}-minus`}
                        name={t('instance-profile-models-remove-row', entry.id)}
                        // A model profile an instance runs is removable here
                        // (26.8): only this profile's entry goes.
                        reason={null}
                        disabled={false}
                        busy={deleting}
                        open={removing === entry.id}
                        onPress={() => openRemove(entry.id)}
                        // R6: the row removes the entry, the model profile
                        // stays: the minus, not the can of a list delete.
                        icon="minus"
                      />
                    </td>
                  </tr>
                  {removing === entry.id && (
                    // The confirm in place, right after the row it asks about
                    // (22 D4.4): the question that names the row, the red verb
                    // then Cancel, the alert where the refusal answers.
                    <ConfirmRow
                      columns={3}
                      question={t(
                        'instance-profile-models-remove-question',
                        entry.id,
                      )}
                      verb={t('instance-profile-models-remove')}
                      // R10: the row removes an entry, the model profile
                      // stays, so the running line says a remove, not a delete.
                      status={
                        deleting ? t('instance-profile-model-removing') : ''
                      }
                      alert={confirmAlert}
                      running={deleting}
                      onConfirm={() => void remove(entry.id)}
                      onCancel={() => closeRemove(entry.id)}
                      onSignIn={() => signIn.ask('session-sign-in')}
                    />
                  )}
                </Fragment>
              ))}
            </tbody>
          </table>
        </div>
      )}
      {/* The add, after the table's place (26.8): the same button opens the form
          and closes it as Cancel does. */}
      <button
        type="button"
        id={ADD_ID}
        className="ot-add"
        aria-expanded={adding}
        onClick={() => {
          if (adding) closeAdd()
          else {
            setAdding(true)
            setChosen('')
            setError('')
          }
        }}
      >
        <Icon name="plus" />
        <span className="visually-hidden">
          {t('instance-profile-models-add')}
        </span>
      </button>
      {adding && (
        <form
          aria-labelledby={`${fieldId}-add-title`}
          onSubmit={(e) => {
            e.preventDefault()
            void add()
          }}
          onKeyDown={(e) => {
            // Escape closes the form, as Cancel does; not while the add runs.
            if (e.key === 'Escape' && !sending) closeAdd()
          }}
        >
          <h3 id={`${fieldId}-add-title`}>
            {t('instance-profile-models-add-title')}
          </h3>
          {pool === null ? (
            <LoadingText text={t('instance-models-loading')} />
          ) : 'failed' in pool ? (
            <>
              <p>{pool.failed}</p>
              {pool.signIn && (
                <button
                  type="button"
                  id={`${fieldId}-add-sign-in`}
                  className="secondary"
                  onClick={() => signIn.ask(`${fieldId}-add-sign-in`)}
                >
                  {t('session-sign-in')}
                </button>
              )}
            </>
          ) : left.length === 0 ? (
            // Every model profile is listed already: the form says so, with the
            // way to create one, and the add stays (26.8).
            <>
              <p>{t('instance-profile-models-add-none')}</p>
              <p>
                <a href="#/profiles/model">{t('nav-model-profiles')}</a>
              </p>
            </>
          ) : (
            <div className="field">
              <div className="field-label">
                <label htmlFor={`${fieldId}-add-control`}>
                  {t('instance-profile-models-add-label')}
                </label>
              </div>
              <div className="field-control">
                <select
                  id={`${fieldId}-add-control`}
                  value={chosen}
                  aria-disabled={sending || undefined}
                  aria-invalid={error === '' ? undefined : true}
                  aria-describedby={
                    error === '' ? undefined : `${fieldId}-add-error`
                  }
                  onChange={(e) => {
                    setChosen(e.target.value)
                    setError('')
                  }}
                >
                  <option value="">
                    {t('instance-profile-models-add-choose')}
                  </option>
                  {left.map((id) => (
                    <option key={id} value={id}>
                      {id}
                    </option>
                  ))}
                </select>
                {/* The words under the select, not under the buttons: the field
                    is one flex row that wraps, and `.field-control .field-error`
                    gives the line a row of its own, so the refusal the select
                    answers sits right under it at every width (the designer's
                    advice with the IP6b review). */}
                {error !== '' && (
                  <FieldError id={`${fieldId}-add-error`} text={error} alert />
                )}
                <div className="save-actions">
                  <button
                    type="button"
                    id={`${fieldId}-add-button`}
                    className="primary"
                    aria-disabled={sending ? 'true' : undefined}
                    onClick={() => void add()}
                  >
                    {t('instance-profile-models-add-button')}
                  </button>
                  <button
                    type="button"
                    className="secondary"
                    onClick={closeAdd}
                  >
                    {t('confirm-cancel')}
                  </button>
                </div>
              </div>
            </div>
          )}
        </form>
      )}
      {/* The section's one status line, and its alert, with the Sign in the
          session's end asks for after it. */}
      <p role="status" className="save-status">
        {status}
      </p>
      {alert !== null && <p role="alert">{alert.text}</p>}
      {alert?.signIn && (
        <button
          type="button"
          id={`${fieldId}-models-sign-in`}
          className="secondary"
          onClick={() => signIn.ask(`${fieldId}-models-sign-in`)}
        >
          {t('session-sign-in')}
        </button>
      )}
      <RestartOffer
        ids={offer}
        nameOf={nameOf}
        renewals={renewals}
        onChanged={onChanged}
        onSignIn={signIn.ask}
      />
      {signIn.asking && (
        <SignInDialog onSignedIn={signIn.signedIn} onClose={signIn.close} />
      )}
    </section>
  )
}
