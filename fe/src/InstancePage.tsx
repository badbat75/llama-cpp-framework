// An instance's page (#/instances/<id>, signed in): the header with the id,
// the engine, the state and the run controls, kept live from the app's one
// read of /api/instances, then the Server section (fe/briefs/01-server.md,
// sections 2.4 and 4).
import {
  useCallback,
  useEffect,
  useId,
  useRef,
  useState,
  type ReactNode,
} from 'react'
import { flushSync } from 'react-dom'
import { apiError, call } from './api.ts'
import Dialog from './Dialog.tsx'
import {
  errorText,
  type Answer,
  type Instance,
  type InstancesStore,
} from './Instances.tsx'
import PageHeading from './PageHeading.tsx'
import type { Runs } from './ServerForm.tsx'
import ServerSection, { InstanceNotFound } from './ServerSection.tsx'
import type { Session } from './session.ts'
import SignIn from './SignIn.tsx'
import { StateShape, stateLabel } from './StateBadge.tsx'
import { t } from './strings.ts'

type Action = 'start' | 'stop' | 'restart'

const ACTIONS: Action[] = ['start', 'stop', 'restart']

/** The idle poll's wait after an answer, the action poll's, and its limit (41b, 41c). */
const IDLE_MS = 5000
const ACTION_MS = 1000
const ACTION_LIMIT_MS = 60_000

/** When an action's poll ends, counted from its 2xx answer (now). */
function actionDeadline(): number {
  return Date.now() + ACTION_LIMIT_MS
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
  canConfigure,
  onSignedOut,
  onRenewed,
  onDirty,
}: {
  id: string
  store: InstancesStore
  canConfigure: boolean
  onSignedOut: () => void
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
  const [asking, setAsking] = useState(false)
  const [runs, setRuns] = useState<Runs>({ started: 0, settled: 0 })
  // Sign-ins in the page's dialogs (43b): the Server section is read again
  // (remounted) after each one as another session.
  const [signIns, setSignIns] = useState({
    rereads: 0,
    dropped: false,
    kept: 0,
  })
  const clicked = useRef<Action>('start')
  const dirty = useRef(false)

  useEffect(() => {
    let live = true
    store.read().then((a) => {
      if (!live) return
      setAnswer(a)
      const found = find(a, id)
      if (found === undefined) return
      setInstance(found)
      setNext({ delay: IDLE_MS, run: null })
    })
    return () => {
      live = false
    }
  }, [store, id])

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
              setStatus(t('run-timeout'))
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

  const trackDirty = useCallback(
    (d: boolean) => {
      dirty.current = d
      onDirty(d)
    },
    [onDirty],
  )

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
        run: { action, deadline: actionDeadline(), n },
      })
      return
    }
    setBusy(null)
    setStatus('')
    setAlert({ text: failureText(id, result), signIn: result === 'signed-out' })
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
    flushSync(() => setAsking(false))
    document.getElementById(`${uid}-sign-in`)?.focus()
  }

  function signedIn(session: Session) {
    const kept = renewed(session)
    flushSync(() => setAsking(false))
    if (kept) document.getElementById(`${uid}-${clicked.current}`)?.focus()
  }

  return (
    <>
      <PageHeading text={id} />
      <Body answer={answer} id={id}>
        {instance !== undefined && (
          <>
            <div className="instance-header">
              <p className="engine">{t('instance-engine', instance.engine)}</p>
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
                  onClick={() => setAsking(true)}
                >
                  {t('session-sign-in')}
                </button>
              )}
            </div>
            <ServerSection
              key={signIns.rereads}
              id={id}
              canConfigure={canConfigure}
              dropped={signIns.dropped}
              renewals={signIns.kept}
              runs={runs}
              onSignedOut={onSignedOut}
              onRenewed={renewed}
              onDirty={trackDirty}
            />
          </>
        )}
      </Body>
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
