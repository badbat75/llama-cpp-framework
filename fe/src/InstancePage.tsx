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
  type FormEvent,
  type ReactNode,
} from 'react'
import { flushSync } from 'react-dom'
import { apiError, call, isRecord, sessionRevoked } from './api.ts'
import Dialog from './Dialog.tsx'
import { CheckboxField, GroupField, TextField } from './Field.tsx'
import { Icon, type IconName } from './Icon.tsx'
import {
  errorText,
  sameTags,
  tagsFrom,
  writeTags,
  type Answer,
  type Instance,
  type InstancesStore,
  type TagsRefusal,
} from './Instances.tsx'
import PageHeading from './PageHeading.tsx'
import { useWarnOnLeave } from './SaveBar.tsx'
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
   * when the page's own state changes. The Server form arms the browser's own
   * ask for itself, so the page arms it for the tags: one section, one
   * listener. */
  const [warnTags, setWarnTags] = useState(false)
  useWarnOnLeave(warnTags)
  const report = useCallback(
    (section: 'server' | 'tags', d: boolean) => {
      if (section === 'server') serverDirty.current = d
      else {
        tagsDirty.current = d
        setWarnTags(d)
      }
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
    // clicked, Start at boot's checkbox, the tags field's Save tags (12 T1.11).
    const control =
      from === 'boot'
        ? `${uid}-boot-control`
        : from === 'tags'
          ? `${uid}-tags-save`
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
    return <p role="status">{t('loading')}</p>
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
 * The instance's own tags (fe/briefs/12-instance-tags.md, 12 T1.5 to T1.14):
 * one text box, outside the Server form, in a `form` of its own: Save tags or
 * Enter writes it with `PUT /api/instances/<id>/tags`. The box is the user's:
 * filled by the first read of the page, never rewritten by a poll or by the
 * answer of a save; only the profile line follows the reads. The page keys it
 * by its sign-ins, so a sign-in as another account mounts a box holding the
 * tags of the read (12 T1.11). Its controls are
 * `<fieldId>-control`, `<fieldId>-save` and `<fieldId>-sign-in`.
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
  // The box's text: null while nothing was typed, so the box holds the list
  // the page opened with (T1.12).
  const [text, setText] = useState<string | null>(null)
  // The list of the read that opened the page, latched at mounting: the box is
  // filled from it and no later read rewrites it (decision 7, T1.12).
  const [base] = useState(() => instance.instanceTags.join(', '))
  // The list a 204 wrote, null while nothing has been saved on this page.
  const [saved, setSaved] = useState<string[] | null>(null)
  const [saving, setSaving] = useState(false)
  const [status, setStatus] = useState('')
  // The 422 of the rule, under the box; one keystroke clears it.
  const [error, setError] = useState<string | undefined>(undefined)
  const [stored, setAlert] = useState<{
    text: string
    signIn: boolean
    at: number
  } | null>(null)
  const alert = stored?.signIn && stored.at !== renewals ? null : stored
  const value = text ?? base
  // The tags the profile brings: the union's ones that are not in the list
  // this read reports (T1.5), in their order.
  const profile = instance.tags.filter(
    (tag) => !instance.instanceTags.includes(tag),
  )

  // The tags are unsaved while the box, split as the manager reads it, differs
  // from the list read at opening or last saved with a 204, so `v2,gpu` is
  // `v2, gpu` (12 T1.13). It is said at once, in the turn that changed it:
  // the page's guard for leaving is the app's (38), and an answer that lands
  // outside a render has to reach it before the answer is over. Null while
  // nothing has been said, so the mounting says it too: the page remounts the
  // field at a sign-in as another account (12 T1.11), and the box it fills
  // from the read is clean.
  const reported = useRef<boolean | null>(null)
  const report = useCallback(
    (text: string, savedList: string[] | null) => {
      const d = !sameTags(tagsFrom(text), savedList ?? tagsFrom(base))
      if (reported.current === d) return
      reported.current = d
      onDirty(d)
    },
    [base, onDirty],
  )

  // What the box holds and what a 204 wrote are all the judgement reads: the
  // list of the opening read is latched, so a page read moves nothing, and the
  // box of a field nobody typed in is the list it was filled with (T1.12).
  useEffect(() => {
    report(value, saved)
  }, [value, saved, report])

  // Read by a send's answer: the sign-in count when the answer arrives (43b).
  const renewalsNow = useRef(renewals)
  useEffect(() => {
    renewalsNow.current = renewals
  }, [renewals])

  async function submit(e: FormEvent<HTMLFormElement>) {
    e.preventDefault()
    if (saving) return
    const tags = tagsFrom(value)
    setError(undefined)
    setAlert(null)
    setStatus(t('instance-tags-saving'))
    setSaving(true)
    const result = await writeTags(id, tags)
    if (typeof result === 'object' && result.key === 'server-bad-tags') {
      // The rule's refusal is the field's error, and the focus goes to the box
      // the error describes (12 T1.9). The answer's state lands at once: the
      // page's unsaved changes are what the answer left, and the app hears
      // about them in the same turn.
      flushSync(() => {
        setSaving(false)
        setStatus('')
        setError(refusalText(id, result))
      })
      document.getElementById(`${fieldId}-control`)?.focus()
      return
    }
    if (result === 'saved') {
      // The list written is the box's own from now on, so the tags are saved
      // and the page's unsaved changes go with them (12 T1.14).
      flushSync(() => {
        setSaving(false)
        setStatus(t('instance-tags-saved'))
        setSaved(tags)
        report(tags.join(', '), tags)
      })
      onSaved()
      return
    }
    flushSync(() => {
      setSaving(false)
      setStatus('')
      setAlert({
        text:
          typeof result === 'object'
            ? refusalText(id, result)
            : alertText(result),
        signIn: result === 'signed-out' || result === 'revoked',
        at: renewalsNow.current,
      })
    })
  }

  return (
    <form onSubmit={(e) => void submit(e)}>
      <TextField
        id={fieldId}
        label={t('instance-tags-own')}
        hint={t('instance-tags-hint')}
        placeholder=""
        value={value}
        error={error}
        onChange={(v) => {
          setText(v)
          setError(undefined)
          report(v, saved)
        }}
      />
      {profile.length > 0 && (
        <p className="tags-profile muted">
          {t('instance-tags-profile', profile.join(', '))}
        </p>
      )}
      <button
        id={`${fieldId}-save`}
        type="submit"
        className="secondary tags-save"
        aria-disabled={saving || undefined}
      >
        {t('instance-tags-save')}
      </button>
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
    </form>
  )
}
