// An instance's page (#/instances/<id>, signed in): the header with the id,
// the engine, the state and the run controls, kept live from the app's one
// read of /api/instances, and Start at boot, then the Server section
// (fe/briefs/01-server.md, sections 2.4 and 4).
import {
  useCallback,
  useEffect,
  useId,
  useRef,
  useState,
  type ReactNode,
} from 'react'
import { flushSync } from 'react-dom'
import { apiError, call, isRecord, sessionRevoked } from './api.ts'
import Dialog from './Dialog.tsx'
import {
  CheckboxField,
  FieldError,
  GroupField,
  Hint,
  HintToggle,
} from './Field.tsx'
import { Icon, type IconName } from './Icon.tsx'
import {
  errorText,
  writeTags,
  type Answer,
  type Instance,
  type InstancesStore,
  type TagsRefusal,
} from './Instances.tsx'
import PageHeading from './PageHeading.tsx'
import { useDirtyReport, useWarnOnLeave } from './SaveBar.tsx'
import { CardBox, Lines, Loading } from './Skeleton.tsx'
import type { Runs } from './ServerForm.tsx'
import ServerSection, { InstanceNotFound } from './ServerSection.tsx'
import type { Session, SignedOut } from './session.ts'
import SignIn from './SignIn.tsx'
import { StateShape, stateLabel } from './StateBadge.tsx'
import { t } from './strings.ts'

type Action = 'start' | 'stop' | 'restart'

const ACTIONS: Action[] = ['start', 'stop', 'restart']

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

function label(action: Action): string {
  switch (action) {
    case 'start':
      return t('run-start')
    case 'stop':
      return t('run-stop')
    case 'restart':
      return t('run-restart')
  }
}

/** The verb's icon (08-hearth-h8-icons.md H8b.1), beside the word it says. */
function icon(action: Action): IconName {
  switch (action) {
    case 'start':
      return 'play'
    case 'stop':
      return 'square'
    case 'restart':
      return 'rotate-cw'
  }
}

function runningText(action: Action): string {
  switch (action) {
    case 'start':
      return t('run-starting')
    case 'stop':
      return t('run-stopping')
    case 'restart':
      return t('run-restarting')
  }
}

/** The outcome of `action` once a read gives `state`, null while it runs (42a). */
function settledText(action: Action, state: string): string | null {
  if (state === 'failed') return t('run-ended-failed')
  if (action === 'start' && state === 'active') return t('run-started')
  if (action === 'restart' && state === 'active') return t('run-restarted')
  if (action === 'stop' && state === 'inactive') return t('run-stopped')
  return null
}

/** The buttons a state leaves inactive (40a); an unknown state leaves none. */
function inactiveIn(state: string, action: Action): boolean {
  switch (state) {
    case 'active':
      return action === 'start'
    case 'inactive':
    case 'failed':
      return action !== 'start'
    case 'activating':
    case 'deactivating':
      return true
    default:
      return false
  }
}

/** Sends the action: 'ok' on any 2xx, else 'signed-out' or the error key. */
async function sendAction(id: string, action: Action): Promise<string> {
  try {
    const reply = await call(
      `/api/instances/${encodeURIComponent(id)}/${action}`,
      { method: 'POST' },
    )
    if (reply.ok) return 'ok'
    if (reply.status === 401) return 'signed-out'
    if (sessionRevoked(reply)) return 'revoked'
    return apiError(reply.body)?.key ?? 'failed'
  } catch {
    return 'failed'
  }
}

/** The run alert of a failed action, by its key (43). */
function failureText(id: string, key: string): string {
  switch (key) {
    case 'signed-out':
      return t('session-ended-retry')
    case 'revoked':
      return t('session-revoked')
    case 'client-not-authorized':
      return t('api-client-not-authorized')
    case 'client-no-instance':
      return t('instance-not-found', id)
    case 'client-unavailable':
      return t('api-client-unavailable')
    case 'client-service-failed':
      return t('api-client-service-failed')
    default:
      return t('request-failed')
  }
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

/** This page's instance in a successful answer, undefined otherwise. */
function find(answer: Answer, id: string): Instance | undefined {
  return Array.isArray(answer) ? answer.find((i) => i.id === id) : undefined
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
  const [asking, setAsking] = useState<'run' | 'boot' | 'tags' | null>(null)
  const [runs, setRuns] = useState<Runs>({ started: 0, settled: 0 })
  // Sign-ins in the page's dialogs (43b): after each one as another session,
  // the Server section and the tags field are remounted (each keyed by
  // `rereads`), so both show what the page reads for the new session.
  const [signIns, setSignIns] = useState({
    rereads: 0,
    dropped: false,
    kept: 0,
  })
  const clicked = useRef<Action>('start')
  // The page's unsaved changes: the Server form's and the tags field's (12
  // T1.13): leaving asks for either, and a sign-in that drops them is what
  // `renewed` reports (43b).
  const dirty = useRef(false)
  const serverDirty = useRef(false)
  const tagsDirty = useRef(false)

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
      const found = find(a, id)
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
        const found = find(a, id)
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

  /** One section's unsaved changes: the page is dirty while either the Server
   * form or the tags field has any (12 T1.13), and the app hears about it only
   * when the page's own state changes. Each section arms the browser's own ask
   * for itself. */
  const report = useCallback(
    (section: 'server' | 'tags', d: boolean) => {
      if (section === 'server') serverDirty.current = d
      else tagsDirty.current = d
      const any = serverDirty.current || tagsDirty.current
      if (dirty.current === any) return
      dirty.current = any
      onDirty(any)
    },
    [onDirty],
  )
  const trackServer = useCallback((d: boolean) => report('server', d), [report])
  const trackTags = useCallback((d: boolean) => report('tags', d), [report])

  /** A save wrote the tags: the page reads the list again, so the profile
   * line, the nav and the Overview follow the file (12 T1.8). */
  const reread = useCallback(() => {
    store.fresh().then((a) => {
      const found = find(a, id)
      if (found !== undefined) setInstance(found)
    })
  }, [store, id])

  const inactive = (action: Action) =>
    busy !== null ||
    instance === undefined ||
    inactiveIn(instance.state, action)

  async function run(action: Action) {
    if (inactive(action)) return
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
      text: failureText(id, result),
      signIn: result === 'signed-out' || result === 'revoked',
    })
    setNext({ delay: IDLE_MS, run: null })
  }

  /** A sign-in in any of the page's dialogs (43b): true when the page stays. */
  const renewed = (session: Session) => {
    const dropped = dirty.current
    const kept = onRenewed(session)
    setAlert((a) => (a?.signIn ? null : a))
    setSignIns((s) =>
      kept
        ? { ...s, kept: s.kept + 1 }
        : { ...s, rereads: s.rereads + 1, dropped: s.dropped || dropped },
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
          : `${uid}-sign-in`
    document.getElementById(signIn)?.focus()
  }

  function signedIn(session: Session) {
    const from = asking
    const kept = renewed(session)
    flushSync(() => setAsking(null))
    if (!kept) return
    // Back on the control the sign-in answers for: the run button that was
    // clicked, Start at boot's checkbox, and in the tags field the New tag box
    // while its pill is open, else the Add tag button (15 C1.10). The box is
    // in the document only while the pill is open.
    const pillOpen = document.getElementById(`${uid}-tags-box`) !== null
    const control =
      from === 'boot'
        ? `${uid}-boot-control`
        : from === 'tags'
          ? `${uid}-tags-${pillOpen ? 'box' : 'add'}`
          : `${uid}-${clicked.current}`
    document.getElementById(control)?.focus()
  }

  return (
    <>
      <PageHeading text={id} />
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
              <div className="run-actions">
                {ACTIONS.map((a) => (
                  <button
                    key={a}
                    id={`${uid}-${a}`}
                    type="button"
                    className="secondary"
                    aria-disabled={inactive(a) || undefined}
                    onClick={() => void run(a)}
                  >
                    <Icon name={icon(a)} />
                    {label(a)}
                  </button>
                ))}
              </div>
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
              instance={instance}
              renewals={signIns.rereads + signIns.kept}
              onSignIn={() => setAsking('tags')}
              onSaved={reread}
              onDirty={trackTags}
            />
            <ServerSection
              key={signIns.rereads}
              id={id}
              dropped={signIns.dropped}
              renewals={signIns.kept}
              runs={runs}
              onSignedOut={onSignedOut}
              onRenewed={renewed}
              onDirty={trackServer}
            />
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
    // The header's skeleton (17a.4): three lines where the instance's name and
    // its facts will be, and the card the Server form will fill.
    return (
      <Loading
        text={t('loading')}
        shape={
          <>
            <Lines n={3} />
            <CardBox />
          </>
        }
      />
    )
  }
  if (!Array.isArray(answer)) {
    return <p>{errorText(answer.error)}</p>
  }
  if (find(answer, id) === undefined) {
    return <InstanceNotFound id={id} />
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
  instance,
  renewals,
  onSignIn,
  onSaved,
  onDirty,
}: {
  id: string
  fieldId: string
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

  // The chip whose full label shows (15 section 10 R5), by its tag: only a
  // chip whose text the ellipsis cut opens the pill, and the cut is read at
  // the moment the pointer or the focus enters, from the span's box.
  const [expanded, setExpanded] = useState<string | null>(null)
  // Escape keeps the pill closed until the pointer or the focus enters the
  // chip again (R5: Content on Hover or Focus, dismissible).
  const dismissed = useRef(false)
  // Each chip's text span, by its tag, for that measure.
  const spans = useRef(new Map<string, HTMLElement | null>())
  const isCut = (tag: string) => {
    const box = spans.current.get(tag)
    return (
      box !== null && box !== undefined && box.scrollWidth > box.clientWidth
    )
  }
  // Both flush: the pointer and the keyboard reach a chip outside React's own
  // events (`focus()` of a test, a real click), and R5 asks the pill to be
  // there the moment the focus is on the `x`, and gone the moment it leaves.
  const showFull = (tag: string) => {
    dismissed.current = false
    if (isCut(tag)) flushSync(() => setExpanded(tag))
  }
  const hideFull = (tag: string) =>
    flushSync(() => setExpanded((shown) => (shown === tag ? null : shown)))
  // Escape closes it, wherever the focus is, and leaves the focus where it
  // stands (R5, WCAG 2.1.1: nothing is reachable only by the pointer).
  useEffect(() => {
    if (expanded === null) return
    const away = (e: KeyboardEvent) => {
      if (e.key !== 'Escape') return
      dismissed.current = true
      setExpanded(null)
    }
    document.addEventListener('keydown', away)
    return () => document.removeEventListener('keydown', away)
  }, [expanded])

  /** Puts the focus on the field's control `suffix` names (`-add`, `-box`,
   * `-remove-<tag>`): the caller has flushed the state that puts it there. */
  function focusControl(suffix: string) {
    document.getElementById(`${fieldId}${suffix}`)?.focus()
  }

  /** Opens the pill with an empty box, and the focus in it (15 C1.3). */
  function openPill() {
    if (saving) return
    flushSync(() => {
      setError(undefined)
      setDraft('')
      setOpen(true)
    })
    focusControl('-box')
  }

  /** Closes it: the box is empty, the `+` is back, and has the focus (15
   *  C1.3), so nothing is left that leaving would lose (15 C1.12). */
  function closePill() {
    flushSync(() => {
      setOpen(false)
      setDraft('')
    })
    focusControl('-add')
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

  /** Sends the list with the label in the box appended: one write, at once
   * (15 C1.4). A box with nothing to add sends nothing: the pill is over when
   * the click on Add ends it, and stays when the key only submits the box. */
  async function addTag(byButton: boolean) {
    if (saving) return
    const tag = draft.trim()
    if (tag === '') {
      if (byButton) closePill()
      return
    }
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
            <ul className="chips" role="list">
              {latched.map((tag) => (
                <li
                  className="chip"
                  key={tag}
                  onMouseEnter={() => showFull(tag)}
                  onMouseLeave={() => hideFull(tag)}
                  onFocus={() => showFull(tag)}
                  onBlur={() => hideFull(tag)}
                >
                  <span
                    ref={(el) => {
                      spans.current.set(tag, el)
                    }}
                  >
                    {tag}
                  </span>
                  {expanded === tag && <ChipFull tag={tag} />}
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
                </li>
              ))}
            </ul>
          )}
          {open ? (
            <div className="tag-new">
              <label className="visually-hidden" htmlFor={`${fieldId}-box`}>
                {t('instance-tag-new')}
              </label>
              <input
                id={`${fieldId}-box`}
                type="text"
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
                }}
                onBlur={(e) => {
                  // A box holding a label stays (15 C1.3, decision 6).
                  if (e.target.value.trim() === '') closePill()
                }}
                onKeyDown={(e) => {
                  if (e.key === 'Enter') void addTag(false)
                  else if (e.key === 'Escape') closePill()
                }}
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
        {profile.length > 0 && (
          <>
            <p className="tags-profile" id={`${fieldId}-profile`}>
              {t('instance-tags-from-profile')}
            </p>
            <ul
              className="chips chips-profile"
              role="list"
              aria-labelledby={`${fieldId}-profile`}
            >
              {profile.map((tag) => (
                <li
                  className="chip"
                  key={tag}
                  tabIndex={0}
                  onMouseEnter={() => showFull(tag)}
                  onMouseLeave={() => hideFull(tag)}
                  onFocus={() => showFull(tag)}
                  onBlur={() => hideFull(tag)}
                >
                  <span
                    ref={(el) => {
                      spans.current.set(tag, el)
                    }}
                  >
                    {tag}
                  </span>
                  {expanded === tag && <ChipFull tag={tag} />}
                </li>
              ))}
            </ul>
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

/**
 * The whole label of a chip whose text the ellipsis cut (fe/briefs/15-tag-chips.md
 * section 10 R5): the pill over the chip, opened while the pointer or the
 * keyboard is on it, closed when both have left, and by Escape. It draws the
 * whole label, so the list item that carries it is not read twice.
 */
function ChipFull({ tag }: { tag: string }) {
  return (
    <span className="chip-full" aria-hidden="true">
      {tag}
    </span>
  )
}
