// An instance's page (#/instances/<id>, signed in): the header with the id,
// the engine, the state and the run controls, kept live from the app's one
// read of /api/instances, and Start at boot, then the tags field and the
// server profile the instance runs. The Server form is on the profile's own
// page (fe/briefs/16-server-profiles-and-instances.md, 16 17a.10; the header:
// fe/briefs/01-server.md, sections 2.4 and 4).
import {
  useCallback,
  useEffect,
  useId,
  useRef,
  useState,
  type KeyboardEvent,
  type ReactNode,
} from 'react'
import { flushSync } from 'react-dom'
import { apiError, call, isRecord, sessionRevoked } from './api.ts'
import { Chip, ChipList } from './Chip.tsx'
import Dialog from './Dialog.tsx'
import {
  CheckboxField,
  FieldError,
  GroupField,
  Hint,
  HintToggle,
} from './Field.tsx'
import { Icon } from './Icon.tsx'
import {
  errorText,
  findInstance,
  instanceName,
  readInstanceTagSuggestions,
  readServerProfiles,
  RunButtons,
  writeLabel,
  writeTags,
  type Answer,
  type Instance,
  type InstancesStore,
  type TagsRefusal,
} from './Instances.tsx'
import PageHeading from './PageHeading.tsx'
import {
  failureText,
  inactiveIn,
  runningText,
  sendAction,
  settledText,
  type Action,
} from './run.ts'
import { serverProfileHref } from './route.ts'
import { useDirtyReport, useWarnOnLeave } from './SaveBar.tsx'
import { Lines, Loading } from './Skeleton.tsx'
import type { Runs } from './ServerForm.tsx'
import type { Session, SignedOut } from './session.ts'
import SignIn from './SignIn.tsx'
import { StateShape, stateLabel } from './StateBadge.tsx'
import { t } from './strings.ts'

/** The most labels the new-tag pill's list shows at once (15 decision 10). */
const MAX_SUGGESTIONS = 8

/** The suggestions' order: by label, by code point (15 decision 10). */
const compareSuggestions = (a: string, b: string) =>
  a < b ? -1 : a > b ? 1 : 0

/** The idle poll's wait after an answer, the action poll's, and the limit of
 * each action: a Start, and a Stop or Restart under the service, which can
 * save slot snapshots for minutes before it stops (41b, 41c, 4d S2.1). */
const IDLE_MS = 5000
const ACTION_MS = 1000
const ACTION_LIMIT_MS = 60_000
const ACTION_LIMIT_STOP_MS = 360_000

/** When an action's poll ends, counted from its 2xx answer (now). */
function actionDeadline(action: Action): number {
  const limit = action === 'start' ? ACTION_LIMIT_MS : ACTION_LIMIT_STOP_MS
  return Date.now() + limit
}

/** The status an action's deadline shows (41c, 4d S2.6). */
function timeoutText(action: Action): string {
  return action === 'start' ? t('run-timeout') : t('run-timeout-stop')
}

/** The text of a refusal the tags route answered (12 T1.9, T1.10): the 422 of
 * the rule is the field's error, a key the contract has a text for its own
 * text, a 422 with a key nobody knows `api-other` with that key, and anything
 * else a request that did not go through. */
function refusalText(id: string, refusal: TagsRefusal): string {
  const { key, args, status } = refusal
  switch (key) {
    case 'server-bad-tags':
      return t('api-server-bad-tags', ...args)
    case 'client-not-authorized':
      return t('api-client-not-authorized')
    case 'client-no-instance':
      return t('instance-not-found', args[0] ?? id)
    case 'client-unavailable':
      return t('api-client-unavailable')
    // A 504 reached the manager: the list may have changed (16 17f.10).
    case 'client-timeout':
      return t('write-timeout')
    default:
      return status === 422 ? t('api-other', key) : t('request-failed')
  }
}

/** The text of the tags field's alert when the answer is not a refusal body:
 * an ended session asks for a sign-in (12 T1.10, T1.11). */
function alertText(key: string): string {
  switch (key) {
    case 'signed-out':
      return t('session-ended-retry')
    case 'revoked':
      return t('session-revoked')
    default:
      return t('request-failed')
  }
}

/** The next read: after `delay`, for the running action (none: the idle poll). */
type Next = {
  delay: number
  run: { action: Action; deadline: number; n: number } | null
}

export default function InstancePage({
  id,
  store,
  onSignedOut,
  onRenewed,
  onDirty,
}: {
  id: string
  store: InstancesStore
  onSignedOut: (reason: SignedOut) => void
  onRenewed: (session: Session) => boolean
  onDirty: (dirty: boolean) => void
}) {
  const uid = useId()
  // The answer of the read asked at opening; undefined while it is awaited.
  const [answer, setAnswer] = useState<Answer | undefined>(undefined)
  // The latest state of this instance, once the header is shown.
  const [instance, setInstance] = useState<Instance | undefined>(undefined)
  const [next, setNext] = useState<Next | null>(null)
  const [busy, setBusy] = useState<Action | null>(null)
  const [status, setStatus] = useState('')
  const [alert, setAlert] = useState<{ text: string; signIn: boolean } | null>(
    null,
  )
  // The Sign in whose dialog is open (43a, 45e).
  const [asking, setAsking] = useState<
    'run' | 'boot' | 'tags' | 'label' | null
  >(null)
  const [runs, setRuns] = useState<Runs>({ started: 0, settled: 0 })
  // Sign-ins in the page's dialogs (43b): after each one as another session, the
  // tags field is remounted (keyed by `rereads`), so it shows what the page reads
  // for the new session.
  const [signIns, setSignIns] = useState({ rereads: 0, kept: 0 })
  const clicked = useRef<Action>('start')
  // The page's unsaved changes, which are the tags field's (12 T1.13): leaving
  // asks for them.
  const dirty = useRef(false)

  useEffect(() => {
    let live = true
    store.read().then((a) => {
      if (!live) return
      if (!Array.isArray(a) && a.error === 'session-required') {
        // The page's first read is the one that says the session is gone
        // (09 S1b.1, the review of S1b 8 C1): the app signs out and the
        // sign-in page says why, in place of this page. The later reads are
        // the polls of the other effect, and a 401 there changes nothing
        // (S1b.2).
        onSignedOut('ended')
        return
      }
      setAnswer(a)
      const found = findInstance(a, id)
      if (found === undefined) return
      setInstance(found)
      setNext({ delay: IDLE_MS, run: null })
    })
    return () => {
      live = false
    }
  }, [store, id, onSignedOut])

  // The polls (41b, 41c): one timer, then one read; a new `next` (or null at
  // a click) cancels the timers and ignores a read still in flight. An action
  // ends at its deadline whatever is pending.
  useEffect(() => {
    if (next === null) return
    let live = true
    const { run } = next
    const ending =
      run === null
        ? undefined
        : setTimeout(
            () => {
              live = false
              setBusy(null)
              setStatus(timeoutText(run.action))
              setNext({ delay: IDLE_MS, run: null })
            },
            Math.max(0, run.deadline - Date.now()),
          )
    const timer = setTimeout(() => {
      if (run !== null && Date.now() >= run.deadline) return
      // An action is settled only by a read sent after its 2xx (41c).
      const reading = run === null ? store.read() : store.fresh(() => live)
      reading.then((a) => {
        if (!live) return
        const found = findInstance(a, id)
        if (found !== undefined) setInstance(found)
        if (run === null) {
          setNext({ delay: IDLE_MS, run: null })
          return
        }
        const outcome =
          found === undefined ? null : settledText(run.action, found.state)
        if (outcome !== null) {
          setBusy(null)
          setStatus(outcome)
          setRuns((r) => ({ ...r, settled: run.n }))
          setNext({ delay: IDLE_MS, run: null })
          return
        }
        const left = run.deadline - Date.now()
        setNext({ delay: Math.max(0, Math.min(ACTION_MS, left)), run })
      })
    }, next.delay)
    return () => {
      live = false
      clearTimeout(timer)
      clearTimeout(ending)
    }
  }, [next, store, id])

  /** The tags field's unsaved changes (12 T1.13): the page is dirty while it has
   * any, and the app hears about it only when the field's own state changes. The
   * field arms the browser's own ask for itself. */
  const trackTags = useCallback(
    (d: boolean) => {
      if (dirty.current === d) return
      dirty.current = d
      onDirty(d)
    },
    [onDirty],
  )

  /** A save wrote the tags: the page reads the list again, so the profile
   * line, the nav and the Overview follow the file (12 T1.8). */
  const reread = useCallback(() => {
    store.fresh().then((a) => {
      const found = findInstance(a, id)
      if (found !== undefined) setInstance(found)
    })
  }, [store, id])

  const inactive = (action: Action) =>
    busy !== null ||
    instance === undefined ||
    inactiveIn(instance.state, action)

  async function run(action: Action) {
    if (inactive(action)) return
    // A refusal names the instance as the page names it (24 A1.4 D1).
    const shownName = instance === undefined ? id : instanceName(instance)
    clicked.current = action
    const n = runs.started + 1
    setNext(null)
    setBusy(action)
    setAlert(null)
    setStatus(runningText(action))
    setRuns((r) => ({ ...r, started: n }))
    const result = await sendAction(id, action)
    if (result === 'ok') {
      setNext({
        delay: ACTION_MS,
        run: { action, deadline: actionDeadline(action), n },
      })
      return
    }
    setBusy(null)
    setStatus('')
    setAlert({
      text: failureText(shownName, result),
      signIn: result === 'signed-out' || result === 'revoked',
    })
    // A 504 is not confirmed: the state shown must follow the manager, so the
    // list is read now, not at the next idle poll (16 17f.10).
    setNext({ delay: result === 'client-timeout' ? 0 : IDLE_MS, run: null })
  }

  /** A sign-in in any of the page's dialogs (43b): true when the page stays. */
  const renewed = (session: Session) => {
    const kept = onRenewed(session)
    setAlert((a) => (a?.signIn ? null : a))
    setSignIns((s) =>
      kept ? { ...s, kept: s.kept + 1 } : { ...s, rereads: s.rereads + 1 },
    )
    return kept
  }

  function closeAsking() {
    const from = asking
    flushSync(() => setAsking(null))
    const signIn =
      from === 'boot'
        ? `${uid}-boot-sign-in`
        : from === 'tags'
          ? `${uid}-tags-sign-in`
          : from === 'label'
            ? `${uid}-name-sign-in`
            : `${uid}-sign-in`
    document.getElementById(signIn)?.focus()
  }

  function signedIn(session: Session) {
    const from = asking
    const kept = renewed(session)
    flushSync(() => setAsking(null))
    if (!kept) return
    // Back on the control the sign-in answers for: the run button that was
    // clicked, Start at boot's checkbox, in the tags field the New tag box
    // while its pill is open, else the Add tag button (15 C1.10), and in the
    // display name form Save with what was typed kept (16 17f2.6). The box is
    // in the document only while the pill is open.
    const pillOpen = document.getElementById(`${uid}-tags-box`) !== null
    const control =
      from === 'boot'
        ? `${uid}-boot-control`
        : from === 'tags'
          ? `${uid}-tags-${pillOpen ? 'box' : 'add'}`
          : from === 'label'
            ? `${uid}-name-save`
            : `${uid}-${clicked.current}`
    document.getElementById(control)?.focus()
  }

  const edit = useNameEdit({
    id,
    fieldId: `${uid}-name`,
    instance,
    renewals: signIns.rereads + signIns.kept,
    onSignIn: () => setAsking('label'),
    onSaved: reread,
  })

  return (
    <>
      {/* The name, the pencil that sets it in the same row, and the id as the
          stable key under them (16 17f.4, 17f2.1 and 17f2.2): the title
          follows the name, the route stays the id. */}
      <PageHeading
        text={instance === undefined ? id : instanceName(instance)}
        actions={edit.pencil}
      />
      {edit.block}
      <Body answer={answer} id={id}>
        {instance !== undefined && (
          <>
            <div className="instance-header">
              <p className="engine">{t('instance-engine', instance.engine)}</p>
              {instance.port > 0 && (
                <p className="port">
                  {t('instance-port', String(instance.port))}
                </p>
              )}
              <p>
                <StateShape state={instance.state} />
                {t('instance-state', stateLabel(instance.state))}
              </p>
              <RunButtons
                state={instance.state}
                busy={busy}
                ids={(action) => `${uid}-${action}`}
                onRun={(action) => void run(action)}
              />
              <p role="status" className="run-status">
                {status}
              </p>
              {alert !== null && (
                <p role="alert" className="run-alert">
                  {alert.text}
                </p>
              )}
              {alert?.signIn && (
                <button
                  id={`${uid}-sign-in`}
                  type="button"
                  className="secondary"
                  onClick={() => setAsking('run')}
                >
                  {t('session-sign-in')}
                </button>
              )}
              <Boot
                id={id}
                fieldId={`${uid}-boot`}
                renewals={signIns.rereads + signIns.kept}
                onSignIn={() => setAsking('boot')}
              />
            </div>
            <TagsField
              key={`tags-${signIns.rereads}`}
              id={id}
              fieldId={`${uid}-tags`}
              list={answer}
              instance={instance}
              renewals={signIns.rereads + signIns.kept}
              onSignIn={() => setAsking('tags')}
              onSaved={reread}
              onDirty={trackTags}
            />
            <section>
              <h2>{t('instance-server-title')}</h2>
              {/* The server profile the instance runs, from the list's item
                  (16 17a.10): the form is on the profile's page, so this row
                  only names it and leads there. */}
              <dl className="assets">
                <dt>{t('instance-server-title')}</dt>
                {instance.serverProfile ? (
                  <dd>
                    <a href={serverProfileHref(instance.serverProfile)}>
                      {instance.serverProfile}
                    </a>
                  </dd>
                ) : (
                  <dd className="muted">{t('instance-server-none')}</dd>
                )}
              </dl>
            </section>
          </>
        )}
      </Body>
      {asking !== null && (
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

/** Loading, the read's message, instance-not-found, or the header and section. */
function Body({
  answer,
  id,
  children,
}: {
  answer: Answer | undefined
  id: string
  children: ReactNode
}) {
  if (answer === undefined) {
    // The header's skeleton (17a.4, and 16 17a.10): three lines where the
    // instance's name and its facts will be. The Server form left the page, so
    // no card is coming.
    return <Loading text={t('loading')} shape={<Lines n={3} />} />
  }
  if (!Array.isArray(answer)) {
    return <p>{errorText(answer.error)}</p>
  }
  if (findInstance(answer, id) === undefined) {
    return (
      <>
        <p>{t('instance-not-found', id)}</p>
        <p>
          <a href="#/">{t('not-found-home')}</a>
        </p>
      </>
    )
  }
  return children
}

/** Reads whether the instance starts at boot: the value, or the error key: a
 * read shows its own paragraph (`bootReadText`), a 401 like any other failure
 * (09 S1b.6, the review of S1b 8 C2). */
async function readBoot(id: string): Promise<boolean | string> {
  try {
    const reply = await call(`/api/instances/${encodeURIComponent(id)}/boot`)
    if (reply.ok) return enabledIn(reply.body) ?? 'failed'
    return apiError(reply.body)?.key ?? 'failed'
  } catch {
    return 'failed'
  }
}

/** Writes it: the value the reply gives, else 'signed-out' or the error key. */
async function writeBoot(
  id: string,
  enabled: boolean,
): Promise<boolean | string> {
  try {
    const reply = await call(`/api/instances/${encodeURIComponent(id)}/boot`, {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ enabled }),
    })
    if (reply.ok) return enabledIn(reply.body) ?? 'failed'
    if (reply.status === 401) return 'signed-out'
    if (sessionRevoked(reply)) return 'revoked'
    return apiError(reply.body)?.key ?? 'failed'
  } catch {
    return 'failed'
  }
}

/** The `enabled` of a `{"enabled": <boolean>}` body, null for any other. */
function enabledIn(body: unknown): boolean | null {
  return isRecord(body) && typeof body.enabled === 'boolean'
    ? body.enabled
    : null
}

/** The paragraph of a failed boot read, by its key (45a). */
function bootReadText(key: string): string {
  switch (key) {
    case 'client-unavailable':
      return t('api-client-unavailable')
    case 'client-not-authorized':
      return t('api-client-not-authorized')
    default:
      return t('boot-load-failed')
  }
}

/**
 * Start at boot (45): read once at mounting, written at once by a change;
 * not part of the Server form. Its checkbox is `<fieldId>-control` and its
 * Sign in `<fieldId>-sign-in`.
 */
function Boot({
  id,
  fieldId,
  renewals,
  onSignIn,
}: {
  id: string
  fieldId: string
  /** Raised by every sign-in that kept the page: a session-ended alert goes. */
  renewals: number
  onSignIn: () => void
}) {
  // The value read, or the read's error key; undefined while it is awaited.
  const [value, setValue] = useState<boolean | string | undefined>(undefined)
  const [busy, setBusy] = useState(false)
  const [status, setStatus] = useState('')
  const [stored, setAlert] = useState<{
    text: string
    signIn: boolean
    at: number
  } | null>(null)
  const alert = stored?.signIn && stored.at !== renewals ? null : stored

  useEffect(() => {
    let live = true
    readBoot(id).then((v) => {
      if (!live) return
      // A 401 here is a failed read like any other, shown by `bootReadText`
      // (09 S1b.6, the review of S1b 8 C2): this is not the page's first read,
      // and signing out would throw away the Server form's unsaved edits with
      // no question.
      setValue(v)
    })
    return () => {
      live = false
    }
  }, [id])

  // Read by a write's answer: whether the page is still shown, and the
  // sign-in count when the answer arrives (43b).
  const mounted = useRef(true)
  const renewalsNow = useRef(renewals)
  useEffect(() => {
    mounted.current = true
    return () => {
      mounted.current = false
    }
  }, [])
  useEffect(() => {
    renewalsNow.current = renewals
  }, [renewals])

  if (value === undefined) return null
  if (typeof value === 'string') {
    return (
      <GroupField id={fieldId} label={t('boot-label')} hint={t('boot-hint')}>
        <p>{bootReadText(value)}</p>
      </GroupField>
    )
  }

  async function change(enabled: boolean) {
    if (busy || typeof value !== 'boolean') return
    const previous = value
    setValue(enabled)
    setBusy(true)
    setStatus('')
    setAlert(null)
    const result = await writeBoot(id, enabled)
    if (!mounted.current) return
    if (typeof result === 'boolean') {
      setValue(result)
      setStatus(result ? t('boot-on') : t('boot-off'))
      setBusy(false)
      return
    }
    setAlert({
      text: failureText(id, result),
      signIn: result === 'signed-out' || result === 'revoked',
      at: renewalsNow.current,
    })
    const again = await readBoot(id)
    if (!mounted.current) return
    setValue(typeof again === 'boolean' ? again : previous)
    setBusy(false)
  }

  return (
    <CheckboxField
      id={fieldId}
      label={t('boot-label')}
      hint={t('boot-hint')}
      toggle={t('boot-toggle')}
      checked={value}
      inactive={busy}
      onChange={(enabled) => void change(enabled)}
    >
      <p role="status" className="boot-status">
        {status}
      </p>
      {alert !== null && (
        <p role="alert" className="boot-alert">
          {alert.text}
        </p>
      )}
      {alert?.signIn && (
        <button
          id={`${fieldId}-sign-in`}
          type="button"
          className="secondary"
          onClick={onSignIn}
        >
          {t('session-sign-in')}
        </button>
      )}
    </CheckboxField>
  )
}

/**
 * The instance's display name (fe/briefs/16-server-profiles-and-instances.md,
 * 16 17f2.1 to 17f2.8): the `pencil` icon button that opens the form, and the
 * form with the Display name box, Save and Cancel, its status line and its
 * alert place. The pencil sits in the page's title row, right after the heading
 * it changes, and the form opens right under the id line, so the two are handed
 * to the page apart and share one state here: the heading stays a plain
 * heading (17f2.1), and typing in the form arms the unsaved-changes question of
 * nothing.
 *
 * The outcome of a write is committed to the form's status while the form is
 * open, and the form closes one commit later, where the line right after the
 * title row carries it: the page shows the outcome once, whether the form is
 * open or closed (17f2.6).
 */
function useNameEdit({
  id,
  fieldId,
  instance,
  renewals,
  onSignIn,
  onSaved,
}: {
  id: string
  fieldId: string
  /** The instance as the page last read it: its display name, engine and port.
   *  Undefined while the page's first read is on its way: nothing shows. */
  instance: Instance | undefined
  /** Raised by every sign-in that kept the page: a session alert goes. */
  renewals: number
  /** The Sign in of the 401 and the 403: the page opens its dialog. */
  onSignIn: () => void
  /** Raised when the name shown must follow the manager's: the page reads the
   * list again. */
  onSaved: () => void
}): { pencil: ReactNode; block: ReactNode } {
  const [open, setOpen] = useState(false)
  const [text, setText] = useState('')
  const [saving, setSaving] = useState(false)
  // The outcome of the last write, in the form's status while it is open and in
  // the line after the title row once it is closed (17f2.6).
  const [outcome, setOutcome] = useState('')
  // The 422 and the 409, under the box (17f2.7); one keystroke clears it.
  const [error, setError] = useState<string | undefined>(undefined)
  const [stored, setAlert] = useState<{
    text: string
    signIn: boolean
    at: number
  } | null>(null)
  const alert = stored?.signIn && stored.at !== renewals ? null : stored
  // The write in flight, guarded by a ref: Enter in the box and the form's
  // submit are the same save, and a press while it runs sends nothing (17f2.6).
  const sent = useRef(false)
  const mounted = useRef(true)
  const renewalsNow = useRef(renewals)
  useEffect(() => {
    mounted.current = true
    return () => {
      mounted.current = false
    }
  }, [])
  useEffect(() => {
    renewalsNow.current = renewals
  }, [renewals])

  /** The name the instance shows with no display name: the engine and the
   *  port, or the id when there is no port (17f.1). The hint and the cleared
   *  line name it (17f2.4, 17f2.6). */
  const auto =
    instance === undefined || instance.port === 0
      ? id
      : t('instance-name-auto', instance.engine, String(instance.port))

  /** The line a write leaves: `instance-name-saved` with the name the display
   *  name gives, or `instance-name-cleared` with the name the instance is back
   *  to for the empty display name (17f2.6). */
  function outcomeOf(trimmed: string): string {
    if (trimmed !== '') return t('instance-name-saved', trimmed)
    return t('instance-name-cleared', auto)
  }

  /** Opens the form with the current display name, its text selected and the
   *  focus in the box, so typing replaces it (17f2.3). */
  function openForm() {
    flushSync(() => {
      setOutcome('')
      setError(undefined)
      setAlert(null)
      setText(instance?.label ?? '')
      setOpen(true)
    })
    const box = document.getElementById(`${fieldId}-box`)
    if (box instanceof HTMLInputElement) {
      box.focus()
      box.setSelectionRange(0, box.value.length)
    }
  }

  /** Closes the form at once: the typing goes, and the outcome stays in the
   *  line after the title row (17f2.6). A write on its way is not cancelled:
   *  the three ways of closing are refused while it runs, so the answer lands
   *  in the form the reader is looking at, with the typing they made. */
  function closeForm() {
    if (saving || sent.current) return
    leaveForm()
  }

  /** The closing of every path, the write's own included: the form goes, the
   *  typing goes, and the pencil takes the focus so it never drops. */
  function leaveForm() {
    setOpen(false)
    setText('')
    setError(undefined)
    setAlert(null)
    setSaving(false)
    sent.current = false
    document.getElementById(fieldId)?.focus()
  }

  /** Leaves a write's outcome and closes the form. The open form's status is
   *  committed first (flushSync), so the line the reader hears is the outcome
   *  and not the text of the closing form; the line after the title row carries
   *  it from there (17f2.6). */
  function showOutcome(line: string) {
    flushSync(() => setOutcome(line))
    leaveForm()
  }

  /** Saves what is in the box: the text trimmed, `""` for the clear (17f2.6). A
   *  text that trims to the display name the instance has sends nothing. The
   *  trim is the manager's, spaces and tabs only (17f review R2, `checks.rs`
   *  `instance_label`): `String.prototype.trim` also strips a no-break space,
   *  which the manager keeps, so the form would send a name the manager never
   *  received and take such a name for the one the instance has. */
  async function save() {
    if (saving || sent.current) return
    const trimmed = text.replace(/^[ \t]+|[ \t]+$/g, '')
    if (trimmed === (instance?.label ?? '').replace(/^[ \t]+|[ \t]+$/g, '')) {
      showOutcome(outcomeOf(trimmed))
      return
    }
    sent.current = true
    setSaving(true)
    setError(undefined)
    setAlert(null)
    setOutcome(t('instance-name-saving'))
    const result = await writeLabel(id, trimmed)
    if (!mounted.current) return
    setSaving(false)
    sent.current = false
    if (result === 'saved') {
      showOutcome(outcomeOf(trimmed))
      onSaved()
      return
    }
    setOutcome('')
    const failure = labelFailure(id, result)
    if (failure.at === 'field') {
      setError(failure.text)
      document.getElementById(`${fieldId}-box`)?.focus()
      return
    }
    setAlert({
      text: failure.text,
      signIn: failure.signIn,
      at: renewalsNow.current,
    })
    // A 504 is not confirmed: the name shown is the manager's, so the list is
    // read again (17f2.7, 17f.10).
    if (failure.again) onSaved()
  }

  if (instance === undefined) return { pencil: null, block: null }

  const describedBy = [
    `${fieldId}-hint`,
    ...(error === undefined ? [] : [`${fieldId}-error`]),
  ].join(' ')

  return {
    pencil: (
      <button
        id={fieldId}
        type="button"
        className="ot-add instance-name-edit"
        aria-expanded={open}
        aria-controls={`${fieldId}-form`}
        onClick={() => (open ? closeForm() : openForm())}
      >
        <Icon name="pencil" />
        <span className="visually-hidden">{t('instance-name-edit')}</span>
      </button>
    ),
    block: (
      <>
        {/* The line the write leaves, kept once the form is closed (17f2.6). */}
        {!open && outcome !== '' && <p role="status">{outcome}</p>}
        {/* The id as the stable key, and the word that says it stays (17f2.2). */}
        <p className="instance-id">{t('instance-id-fixed', id)}</p>
        {open && (
          <form
            id={`${fieldId}-form`}
            className="instance-name-form"
            aria-label={t('instance-name-label')}
            onSubmit={(e) => {
              e.preventDefault()
              void save()
            }}
          >
            <label htmlFor={`${fieldId}-box`}>{t('instance-name-label')}</label>
            <input
              id={`${fieldId}-box`}
              type="text"
              autoComplete="off"
              spellCheck={false}
              value={text}
              aria-describedby={describedBy}
              {...(error === undefined ? {} : { 'aria-invalid': true })}
              onChange={(e) => {
                setText(e.target.value)
                setError(undefined)
              }}
              onKeyDown={(e) => {
                if (e.key === 'Enter') {
                  // The key is stopped (17f review D2): the save closes the form
                  // and focuses the pencil, and the same Enter would press it.
                  e.preventDefault()
                  void save()
                } else if (e.key === 'Escape') closeForm()
              }}
            />
            <Hint
              id={`${fieldId}-hint`}
              text={t('instance-name-hint', auto, id)}
              open
            />
            {error !== undefined && (
              <FieldError id={`${fieldId}-error`} text={error} />
            )}
            <div>
              <button
                id={`${fieldId}-save`}
                type="button"
                className="primary"
                aria-disabled={saving || undefined}
                onClick={() => void save()}
              >
                {t('instance-name-save')}
              </button>
              <button
                id={`${fieldId}-cancel`}
                type="button"
                className="secondary"
                aria-disabled={saving || undefined}
                onClick={closeForm}
              >
                {t('instance-label-cancel')}
              </button>
            </div>
            <p role="status">{outcome}</p>
          </form>
        )}
        {alert !== null && <p role="alert">{alert.text}</p>}
        {alert?.signIn && (
          <button
            id={`${fieldId}-sign-in`}
            type="button"
            className="secondary"
            onClick={onSignIn}
          >
            {t('session-sign-in')}
          </button>
        )}
      </>
    ),
  }
}

/**
 * What a refused label write is (16 17f.7): `field` is the text under the box,
 * with the box marked and the focus back in it; `alert` is the line in the form,
 * with the Sign in button when the session ended, and `again` says the list is
 * read because the outcome is not confirmed. The keys are the ones the contract
 * of the slice answers for the route.
 */
function labelFailure(
  id: string,
  result: TagsRefusal | string,
):
  | { at: 'field'; text: string }
  | { at: 'alert'; text: string; signIn: boolean; again: boolean } {
  if (typeof result !== 'object')
    return {
      at: 'alert',
      text: alertText(result),
      signIn: result === 'signed-out' || result === 'revoked',
      again: false,
    }
  const { key, args, status } = result
  switch (key) {
    case 'instance-bad-label':
      return { at: 'field', text: t('api-instance-bad-label', ...args) }
    case 'comment-markers':
      return { at: 'field', text: t('api-comment-markers', ...args) }
    case 'instance-label-taken':
      return { at: 'field', text: t('api-instance-label-taken', ...args) }
    case 'client-no-instance':
      return {
        at: 'alert',
        text: t('instance-not-found', args[0] ?? id),
        signIn: false,
        again: false,
      }
    case 'client-unavailable':
      return {
        at: 'alert',
        text: t('api-client-unavailable'),
        signIn: false,
        again: false,
      }
    case 'client-timeout':
      return {
        at: 'alert',
        text: t('write-timeout'),
        signIn: false,
        again: true,
      }
    default:
      return {
        at: 'alert',
        text: status === 422 ? t('api-other', key) : t('request-failed'),
        signIn: false,
        again: false,
      }
  }
}

/**
 * The instance's own tags as chips (fe/briefs/15-tag-chips.md, slice C1, as its
 * section 10 rules): each chip's `x` and each label the pill adds write the
 * whole list at once with `PUT /api/instances/<id>/tags`, beside the fixed
 * chips of the tags its server profile brings. The chips are React's optimistic
 * list (R6): a write takes them at once, and the list it sends is latched with
 * it, so a poll that lands on the way changes nothing and a refusal puts the
 * chips back (15 C1.11). The page keys the field by its sign-ins, so a sign-in
 * as another account starts it from the read (12 T1.11). Its controls are
 * `<fieldId>-add`, `<fieldId>-box`, `<fieldId>-remove-<tag>` and
 * `<fieldId>-sign-in`.
 */
function TagsField({
  id,
  fieldId,
  list,
  instance,
  renewals,
  onSignIn,
  onSaved,
  onDirty,
}: {
  id: string
  fieldId: string
  /** The page's latest read of the instances list: every instance's tags. */
  list: Answer | undefined
  /** The instance as the page last read it: its own tags, and the union. */
  instance: Instance
  /** Raised by every sign-in that kept the page: a session alert goes. */
  renewals: number
  onSignIn: () => void
  /** Raised after a 204: the page reads the list again. */
  onSaved: () => void
  /** Whether the box holds a list other than the one read or last saved. */
  onDirty: (dirty: boolean) => void
}) {
  // The chips (15 section 10 R6): the field's own list, latched at the read
  // that opened the page and moved only by a write. The page reads the list
  // again after every 204 and on its own poll, and no read touches this, so no
  // poll moves a chip (15 C1.11). A write puts the list it sends over the chips
  // at once, and the answer keeps it (a 204, no visible step) or puts back the
  // list before the write (a refusal, 15 C1.9). Both moves are flushed: C1.8
  // and C1.9 ask for the chip and the focus to move in the same flush as the
  // click, and as the answer, and React's `useOptimistic` list is given up a
  // task after the answer, at the transition's end, so the chips are this
  // state and nothing more.
  const [latched, setLatched] = useState<string[]>(instance.instanceTags)
  // The pill: open, and what is typed in it. A label is trimmed only when it
  // is sent, so what the user typed stays in the box (15 C1.4).
  const [open, setOpen] = useState(false)
  const [draft, setDraft] = useState('')
  // One write at a time, drawn as busy (15 C1.5): the flag is the field's, set
  // and cleared in the same flush as the chips and the line that go with it, so
  // the chips read busy exactly while the write is on its way.
  const [saving, setSaving] = useState(false)
  const [status, setStatus] = useState('')
  // The hint of the field, shown by its About toggle.
  const [about, setAbout] = useState(false)
  // The 422 of the rule and the duplicate check, under the box; one keystroke
  // clears them.
  const [error, setError] = useState<string | undefined>(undefined)
  const [stored, setAlert] = useState<{
    text: string
    signIn: boolean
    at: number
  } | null>(null)
  const alert = stored?.signIn && stored.at !== renewals ? null : stored
  // The tags the profile brings: the ones this read carries that the field
  // does not show as chips (12 T1.5).
  const profile = instance.tags.filter((tag) => !latched.includes(tag))
  // The suggestions (15 C2, decision 10): the tags the node's server profiles
  // carry, read once at each opening of the pill, added to what the page's
  // latest list read carries. A list that failed or has not answered brings
  // nothing, and says nothing: the suggestion is a help, never an error.
  const [profiles, setProfiles] = useState<unknown>(null)
  // The list, and the option the arrows are on: `open` is the pill's, so this
  // says whether the list shows. `active` is the index in `options` (`null`:
  // on none, as after typing) kept with the list it was walked onto, so a list
  // that moved (a poll took a label out) leaves no option active: the index
  // would name a label nobody walked onto (15 C2.3 review).
  const [listOpen, setListOpen] = useState(false)
  const [active, setActive] = useState<{ at: number; of: string } | null>(null)

  // The tags this instance shows, as chips of its own or as the profile's:
  // the suggestions leave them out (decision 10).
  const shown = [...new Set([...instance.tags, ...latched])]
  // Every label to offer, sorted by code point (decision 10).
  const options = readInstanceTagSuggestions(list ?? [], profiles, shown).sort(
    compareSuggestions,
  )
  // The list as one string: the fact an option was walked onto THIS list.
  const optionsOf = options.join('\n')
  // The index of the active option, `null` when none is, and when the list
  // moved under it (15 C2.3 review: the option the arrows are on belongs to the
  // list that shows it, so a poll that changed the labels ends it).
  const activeAt = active === null || active.of !== optionsOf ? null : active.at
  const typed = draft.trim().toLowerCase()
  // The options that show, as indices in `options` (15 C2.2): the labels
  // containing the text, compared lower-cased, so an empty box takes every
  // one, and at most `MAX_SUGGESTIONS` of them (decision 10: the cap is of the
  // labels the text takes, so a label the visitor's text matches never hides
  // behind a label the text threw away). These are the only ones the arrows
  // walk, so a key never lands on a label the text hides (15 C2.3).
  const shownAt = options
    .map((tag, at) => ({ tag, at }))
    .filter(({ tag }) => tag.toLowerCase().includes(typed))
    .slice(0, MAX_SUGGESTIONS)
    .map(({ at }) => at)
  // The combobox is expanded when its listbox shows, which needs a label to
  // show (15 C2.2).
  const expanded = listOpen && shownAt.length > 0

  // A label typed in the box is the only thing leaving would lose (15 C1.12):
  // it is the field's unsaved change, said at once to the browser and to the
  // page, as the Server form says its own. A write answers at once, so it is
  // never a change waiting to be saved.
  const dirty = draft.trim() !== ''
  useWarnOnLeave(dirty)
  useDirtyReport(dirty, onDirty)

  // Read by a send's answer: the sign-in count when the answer arrives (43b).
  const renewalsNow = useRef(renewals)
  useEffect(() => {
    renewalsNow.current = renewals
  }, [renewals])

  /** Puts the focus on the field's control `suffix` names (`-add`, `-box`,
   * `-remove-<tag>`): the caller has flushed the state that puts it there. */
  function focusControl(suffix: string) {
    document.getElementById(`${fieldId}${suffix}`)?.focus()
  }

  /** Opens the pill with an empty box, and the focus in it (15 C1.3), the
   *  suggestions read for this opening (15 C2.1: once per opening, nothing
   *  else reads them) and its list collapsed. */
  function openPill() {
    if (saving) return
    readServerProfiles().then(setProfiles)
    flushSync(() => {
      setError(undefined)
      setDraft('')
      setOpen(true)
      setListOpen(false)
      setActive(null)
    })
    focusControl('-box')
  }

  /** Closes it: the box is empty, the `+` is back, and has the focus (15
   *  C1.3), so nothing is left that leaving would lose (15 C1.12). */
  function closePill() {
    flushSync(() => {
      setOpen(false)
      setDraft('')
      setListOpen(false)
      setActive(null)
    })
    focusControl('-add')
  }

  /** Moves the active option with ArrowDown (`down`) or ArrowUp, from the last
   *  back to the first and from the first to the last, the list opening when it
   *  is closed (15 C2.3). Focus stays in the box: it names the active option. */
  function walkOptions(down: boolean) {
    const n = shownAt.length
    if (n === 0) return
    // Where the active option sits in the shown ones, -1 when on none: the
    // first with ArrowDown and the last with ArrowUp, the next with the key
    // again, wrapping at the ends (15 C2.3).
    const at = activeAt === null ? -1 : shownAt.indexOf(activeAt)
    setActive({
      at:
        at === -1
          ? shownAt[down ? 0 : n - 1]!
          : shownAt[(at + (down ? 1 : n - 1)) % n]!,
      of: optionsOf,
    })
    setListOpen(true)
  }

  /** The box's keys, as the APG combobox with a list popup gives them
   *  (15 decision 10): the arrows walk the list, Enter adds the active
   *  suggestion or the text, Escape closes the list and then the pill, Tab
   *  leaves with the text and adds nothing. */
  function onOptionKey(e: KeyboardEvent<HTMLInputElement>) {
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      // The key moves the active option, and with it the page's caret: kept.
      e.preventDefault()
      walkOptions(e.key === 'ArrowDown')
    } else if (e.key === 'Enter') {
      void addTag(false)
    } else if (e.key === 'Escape') {
      // A list that shows goes first, the box and its text kept (15 C2.5), and
      // the active option goes with the list it was on: the box naming an
      // option inside a hidden list is a lie to a screen reader, and the next
      // Enter would add the label behind that id (15 C2.5 review). A box whose
      // text matches no label has no list to hide, so Escape ends the pill as
      // it always did (15 C1.3).
      if (expanded) {
        setListOpen(false)
        setActive(null)
      } else {
        closePill()
      }
    } else if (e.key === 'Tab') {
      // Leaving the box: the list goes, nothing is added, and an empty box
      // closes as its own blur does (15 C1.3, C2.5).
      flushSync(() => {
        setListOpen(false)
        setActive(null)
      })
      if (e.currentTarget.value.trim() === '') closePill()
    }
  }

  /** Puts `tags` over the chips at once and sends `write` with it (15 section
   *  10 R6): the chip moves, the box and the `+` and `x` read busy, before the
   *  handler returns. When the write answers, `settled` says what the field is
   *  left with, in one flush: `keep` is the list the chips stay on, the tags
   *  sent for a 204 (the chips do not move again), the list from before the
   *  write for a refusal (the chip that left comes back, the one that was
   *  added goes), and `then` is the line or the alert that goes with it. The
   *  answer reaches the caller once that has landed, so it can put the focus
   *  where the answer asks. */
  function send<T>(
    tags: string[],
    write: () => Promise<T>,
    settled: (result: T) => { keep: string[]; then: () => void },
  ): Promise<T> {
    flushSync(() => {
      setSaving(true)
      setLatched(tags)
    })
    return write().then((result) => {
      const { keep, then } = settled(result)
      flushSync(() => {
        setSaving(false)
        setLatched(keep)
        then()
      })
      return result
    })
  }

  /** A 504: the manager got the write and did not answer in time, so the chips
   *  shown are not the manager's and the list is read again (16 17f.10). */
  const notConfirmed = (result: TagsRefusal | string) =>
    typeof result === 'object' && result.key === 'client-timeout'

  /** The alert a refused write is: the text of its key, and the Sign in
   *  button when the session ended (15 C1.9, C1.10). */
  function refusalAlert(result: TagsRefusal | string) {
    return {
      text:
        typeof result === 'object'
          ? refusalText(id, result)
          : alertText(result),
      signIn: result === 'signed-out' || result === 'revoked',
      at: renewalsNow.current,
    }
  }

  /** Adds a tag: the active suggestion when the arrows are on one, else the
   *  text in the box, one write at once (15 C1.4, C2.4, decision 10). A box
   *  with nothing to add sends nothing: the pill is over when the click on Add
   *  ends it, and stays when the key only submits the box. */
  async function addTag(byButton: boolean) {
    if (saving) return
    const chosen = activeAt === null ? undefined : options[activeAt]
    if (chosen !== undefined) {
      // The active suggestion, whatever the box holds, added exactly as typing
      // it and pressing Enter: the box shows what goes in, the list is done.
      setActive(null)
      setListOpen(false)
      setDraft(chosen)
      void sendTag(chosen)
      return
    }
    const tag = draft.trim()
    if (tag === '') {
      if (byButton) closePill()
      return
    }
    await sendTag(tag)
  }

  /** Appends `tag` to the chips and sends the list (15 C1.4). */
  async function sendTag(tag: string) {
    const key = tag.toLowerCase()
    if (instance.tags.some((kept) => kept.toLowerCase() === key)) {
      // The only check the field makes (decision 7, 15 C1.4): the tag is one
      // the read shows, as a chip of its own or as the profile's, in any
      // spelling. The limits of a tag stay the manager's: a label this field
      // added in a write is not a reason to refuse, and a label whose letters
      // the manager will not take (an upper-case one) goes to it as it is
      // typed, where its rule answers in the box.
      setError(t('instance-tag-shown', tag))
      focusControl('-box')
      return
    }
    // A label is one tag whatever its spelling, so the one typed takes the
    // place of the chip that carries it in another spelling.
    const next = [...latched.filter((kept) => kept.toLowerCase() !== key), tag]
    setError(undefined)
    setAlert(null)
    setStatus(t('instance-tags-saving'))
    const result = await send(
      next,
      () => writeTags(id, next),
      (answer) =>
        answer === 'saved'
          ? {
              // The chip is there before the page reads the list again (15
              // C1.6): the write put it there, the pill is over, and the `+`
              // takes the focus.
              keep: next,
              then: () => {
                setOpen(false)
                setDraft('')
                setStatus(t('instance-tag-added', tag))
              },
            }
          : typeof answer === 'object' && answer.key === 'server-bad-tags'
            ? {
                // The rule's refusal is the box's error, and the focus goes to
                // the box it describes, which keeps what was typed (15 C1.7).
                keep: latched,
                then: () => {
                  setStatus('')
                  setError(refusalText(id, answer))
                },
              }
            : {
                // The chip goes back, the pill stays open with its text: the
                // field keeps what the user had (15 C1.9).
                keep: latched,
                then: () => {
                  setStatus('')
                  setAlert(refusalAlert(answer))
                },
              },
    )
    if (result === 'saved') {
      focusControl('-add')
      onSaved()
      return
    }
    if (typeof result === 'object' && result.key === 'server-bad-tags') {
      focusControl('-box')
    }
    if (notConfirmed(result)) onSaved()
  }

  /** Sends the list without `tag` (15 C1.8). */
  async function removeTag(tag: string) {
    if (saving) return
    // The focus moves the moment the chip leaves, not when the write answers
    // (R6, decision 5): to the chip that takes its place, to the one before it
    // at the end of the row, and to the `+` when the row is empty (15 C1.8).
    const rest = latched.filter((kept) => kept !== tag)
    const next = rest[Math.min(latched.indexOf(tag), rest.length - 1)]
    focusControl(next === undefined ? '-add' : `-remove-${next}`)
    setError(undefined)
    setAlert(null)
    setStatus(t('instance-tags-saving'))
    const result = await send(
      rest,
      () => writeTags(id, rest),
      (answer) =>
        answer === 'saved'
          ? {
              keep: rest,
              then: () => setStatus(t('instance-tag-removed', tag)),
            }
          : {
              // The chip is back in its place (R6, 15 C1.9): the latched list
              // never left, and the alert says why.
              keep: latched,
              then: () => {
                setStatus('')
                setAlert(refusalAlert(answer))
              },
            },
    )
    if (result === 'saved') {
      onSaved()
      return
    }
    if (notConfirmed(result)) onSaved()
    // The restored chip's `x` takes the focus back (15 C1.9).
    focusControl(`-remove-${tag}`)
  }

  return (
    <div className="field">
      {/* The label row is outside the group (brief 15 section 3): the group
          holds the chips and the pill only, so the About toggle comes before
          it and the group's tab stops are the chips' x and the +. */}
      <div className="field-label">
        <span id={`${fieldId}-label`}>{t('instance-tags-own')}</span>
        <HintToggle
          label={t('instance-tags-own')}
          open={about}
          hintId={`${fieldId}-hint`}
          onToggle={() => setAbout((o) => !o)}
        />
      </div>
      <fieldset className="field-control" aria-labelledby={`${fieldId}-label`}>
        <div className="chips-row">
          {latched.length > 0 && (
            <ChipList>
              {latched.map((tag) => (
                <Chip key={tag} tag={tag}>
                  <button
                    type="button"
                    id={`${fieldId}-remove-${tag}`}
                    aria-disabled={saving || undefined}
                    onClick={() => void removeTag(tag)}
                  >
                    <Icon name="x" />
                    <span className="visually-hidden">
                      {t('instance-tag-remove', tag)}
                    </span>
                  </button>
                </Chip>
              ))}
            </ChipList>
          )}
          {open ? (
            <div className="tag-new">
              <label className="visually-hidden" htmlFor={`${fieldId}-box`}>
                {t('instance-tag-new')}
              </label>
              <input
                id={`${fieldId}-box`}
                type="text"
                role="combobox"
                aria-autocomplete="list"
                aria-controls={`${fieldId}-options`}
                aria-expanded={expanded}
                {...(activeAt === null
                  ? {}
                  : {
                      'aria-activedescendant': `${fieldId}-option-${activeAt}`,
                    })}
                spellCheck={false}
                autoCapitalize="off"
                autoCorrect="off"
                autoComplete="off"
                readOnly={saving}
                value={draft}
                {...(error === undefined
                  ? {}
                  : {
                      'aria-invalid': true,
                      'aria-describedby': `${fieldId}-error`,
                    })}
                onChange={(e) => {
                  setDraft(e.target.value)
                  setError(undefined)
                  // Typing filters the list and leaves no option active, the
                  // APG way (15 C2.2, C2.3).
                  setListOpen(true)
                  setActive(null)
                }}
                onBlur={(e) => {
                  // A box holding a label stays (15 C1.3, decision 6).
                  if (e.target.value.trim() === '') closePill()
                }}
                onKeyDown={onOptionKey}
              />
              <button
                type="button"
                aria-disabled={saving || draft.trim() === '' || undefined}
                onClick={() => void addTag(true)}
              >
                <Icon name="check" />
                <span className="visually-hidden">
                  {t('instance-tag-confirm')}
                </span>
              </button>
              <button type="button" onClick={closePill}>
                <Icon name="x" />
                <span className="visually-hidden">
                  {t('instance-tag-cancel')}
                </span>
              </button>
            </div>
          ) : (
            <button
              type="button"
              id={`${fieldId}-add`}
              className="tag-add"
              aria-disabled={saving || undefined}
              onClick={openPill}
            >
              <Icon name="plus" />
              <span className="visually-hidden">{t('instance-tag-add')}</span>
            </button>
          )}
        </div>
        {/* The used tags the box points at (15 C2, decision 10, section 3): a
            listbox in the page's flow, under the chip row, named by its own
            label, `hidden` while the box is collapsed. Of its options the box's
            text takes the ones containing it, and the one the arrows are on
            always shows: a key that moves it lands where the visitor sees it. */}
        <ul
          className="tag-options"
          id={`${fieldId}-options`}
          role="listbox"
          aria-label={t('instance-tag-suggestions')}
          hidden={!expanded}
        >
          {options.map((tag, at) => (
            <li
              key={tag}
              id={`${fieldId}-option-${at}`}
              role="option"
              aria-selected={at === activeAt}
              hidden={!shownAt.includes(at)}
              aria-disabled={saving || undefined}
              onMouseDown={(e) => {
                // Chosen as the click ends, so the box's blur does not close
                // the pill first (15 C2.4).
                e.preventDefault()
              }}
              onClick={() => {
                if (saving) return
                setDraft(tag)
                setActive(null)
                setListOpen(false)
                void sendTag(tag)
              }}
            >
              {tag}
            </li>
          ))}
        </ul>
        {profile.length > 0 && (
          <>
            <p className="tags-profile" id={`${fieldId}-profile`}>
              {t('instance-tags-from-profile')}
            </p>
            <ChipList
              className="chips-profile"
              labelledBy={`${fieldId}-profile`}
            >
              {profile.map((tag) => (
                <Chip key={tag} tag={tag} focusable />
              ))}
            </ChipList>
          </>
        )}
        {error !== undefined && (
          <FieldError id={`${fieldId}-error`} text={error} />
        )}
      </fieldset>
      <Hint
        id={`${fieldId}-hint`}
        text={t('instance-tags-hint')}
        open={about}
      />
      <p role="status" className="tags-status">
        {status}
      </p>
      {alert !== null && (
        <p role="alert" className="tags-alert">
          {alert.text}
        </p>
      )}
      {alert?.signIn && (
        <button
          id={`${fieldId}-sign-in`}
          type="button"
          className="secondary tags-sign-in"
          onClick={onSignIn}
        >
          {t('session-sign-in')}
        </button>
      )}
    </div>
  )
}
