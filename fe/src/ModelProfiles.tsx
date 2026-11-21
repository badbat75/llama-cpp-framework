// The Model profiles page (#/profiles/model, fe/briefs/02-models.md, slice 2a,
// under the nav's Profiles group since 05b): the model profiles of every engine
// read from the bff's /api/profiles/model, with a filter over them (slice 2b),
// and each row's on/off switch (slice 2c), and its Model profile and Engine
// headers sorting the list (brief 17, U17c).
import {
  memo,
  useCallback,
  useDeferredValue,
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
} from 'react'
import { flushSync } from 'react-dom'
import {
  apiError,
  call,
  isRecord,
  sessionRevoked,
  type ApiError,
} from './api.ts'
import Dialog from './Dialog.tsx'
import { Icon } from './Icon.tsx'
import PageHeading from './PageHeading.tsx'
import { modelProfileHref, modelProfileIdUnreadable } from './route.ts'
import type { Session, SignedOut } from './session.ts'
import SignIn from './SignIn.tsx'
import { Lines, Loading, LoadingText } from './Skeleton.tsx'
import { t } from './strings.ts'

type ModelProfile = {
  id: string
  engine: string
  engineAvailable: boolean
  enabled: boolean
  model: string
}

/**
 * The sort of the list (brief 17 U17c): one column, ascending or descending,
 * and `null` for the file order, the order the list was read in. Only the
 * Model profile and Engine columns sort: switched-on first is already the file
 * order's grouping, so Status has nothing to reorder.
 */
export type Sort = { column: Column; dir: Dir }

type Column = 'name' | 'engine'
type Dir = 'asc' | 'desc'

/**
 * The header clicked (U17c.3, U17c.4, U17c.5): a column not sorted sorts
 * ascending; ascending turns to descending; descending is the file order
 * again.
 */
function nextSort(sort: Sort | null, column: Column): Sort | null {
  if (sort === null || sort.column !== column) return { column, dir: 'asc' }
  return sort.dir === 'asc' ? { column, dir: 'desc' } : null
}

/** The words of a column, in the header and in the status line (U17c). */
function columnLabel(column: Column): string {
  return column === 'name'
    ? t('model-profiles-column-name')
    : t('model-profiles-column-engine')
}

/** The words of a sort, said on the filter's own status line (U17c). */
function sortText(sort: Sort | null): string {
  if (sort === null) return t('model-profiles-sorted-file')
  const column = columnLabel(sort.column)
  return sort.dir === 'asc'
    ? t('model-profiles-sorted-asc', column)
    : t('model-profiles-sorted-desc', column)
}

/**
 * The comparison of the brief: `localeCompare` with the base sensitivity and
 * numbers read as numbers (`Qwen3` before `Qwen3.6`), turned for descending.
 */
function compare(a: string, b: string, dir: Dir): number {
  const cmp = a.localeCompare(b, undefined, {
    sensitivity: 'base',
    numeric: true,
  })
  return dir === 'asc' ? cmp : -cmp
}

/** What the read gave: the model profiles, or the error key of its message. */
type Answer =
  | ModelProfile[]
  | { error: 'client-not-authorized' | 'client-unavailable' | 'failed' }
  | 'signed-out'
  | 'revoked'

function isModelProfile(value: unknown): value is ModelProfile {
  return (
    isRecord(value) &&
    typeof value.id === 'string' &&
    typeof value.engine === 'string' &&
    typeof value.engineAvailable === 'boolean' &&
    typeof value.enabled === 'boolean' &&
    typeof value.model === 'string'
  )
}

async function readModelProfiles(): Promise<Answer> {
  try {
    const reply = await call('/api/profiles/model')
    if (reply.status === 401) return 'signed-out'
    if (sessionRevoked(reply)) return 'revoked'
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
    if (!Array.isArray(reply.body) || !reply.body.every(isModelProfile))
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
  | 'revoked'
  | (ApiError & { status: number })
  | 'failed'

async function writeEnabled(id: string, enabled: boolean): Promise<Written> {
  try {
    const reply = await call(
      `/api/profiles/model/${encodeURIComponent(id)}/enabled`,
      {
        method: 'PUT',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ enabled }),
      },
    )
    if (reply.ok) {
      const body = reply.body
      return isRecord(body) &&
        typeof body.enabled === 'boolean' &&
        typeof body.running === 'boolean'
        ? { enabled: body.enabled, running: body.running }
        : 'failed'
    }
    if (reply.status === 401) return 'signed-out'
    if (sessionRevoked(reply)) return 'revoked'
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
  if (result === 'revoked') return t('session-revoked')
  if (result === 'failed') return t('request-failed')
  switch (result.key) {
    case 'client-not-authorized':
      return t('api-client-not-authorized')
    case 'client-unavailable':
      return t('api-client-unavailable')
    case 'no-model-profile-named':
      return t('api-no-model-profile-named', id)
    case 'load-on-startup-over-max':
      return t('api-load-on-startup-over-max', ...result.args)
    case 'load-on-startup-unlimited':
      return t('api-load-on-startup-unlimited', ...result.args)
    case 'model-profile-over-memory':
      return t('api-model-profile-over-memory', ...result.args)
    case 'model-profile-model-missing':
      return t('api-model-profile-model-missing', ...result.args)
    default:
      return result.status === 422
        ? t('api-other', result.key)
        : t('request-failed')
  }
}

/** The status of a switch that held (2c.46). */
function switchedText(p: ModelProfile, running: boolean): string {
  if (p.enabled) {
    return running
      ? t('model-profiles-switched-on-restart', p.id, p.engine)
      : t('model-profiles-switched-on', p.id, p.engine)
  }
  return running
    ? t('model-profiles-switched-off-restart', p.id, p.engine)
    : t('model-profiles-switched-off', p.id, p.engine)
}

/** One row's switch value, kept in place in the list as read (2c.47). */
function rowEnabled(
  rows: ModelProfile[],
  i: number,
  enabled: boolean,
): ModelProfile[] {
  return rows.map((p, j) => (j === i ? { ...p, enabled } : p))
}

export default function ModelProfiles({
  filterText,
  onFilterText,
  sort,
  onSort,
  onSignedOut,
  onRenewed,
}: {
  /** The text kept by the shell, for the tab's life (02-models.md, 62). */
  filterText: string
  onFilterText: (text: string) => void
  /** The sort kept by the shell for the tab's life, as the filter is (U17c). */
  sort: Sort | null
  onSort: (sort: Sort | null) => void
  onSignedOut: (reason: SignedOut) => void
  onRenewed: (session: Session) => boolean
}) {
  // undefined while the reply is awaited.
  const [answer, setAnswer] = useState<Answer | undefined>(undefined)
  // Raised by a sign-in as another session: the model profiles are read again.
  const [reads, setReads] = useState(0)

  useEffect(() => {
    let live = true
    readModelProfiles().then((a) => {
      if (!live) return
      if (a === 'signed-out' || a === 'revoked')
        onSignedOut(a === 'revoked' ? 'revoked' : 'ended')
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
      <PageHeading text={t('model-profiles-title')} />
      <section>
        <p className="intro">{t('model-profiles-intro')}</p>
        <Body
          answer={answer}
          filter={filterText}
          onFilter={onFilterText}
          sort={sort}
          onSort={onSort}
          onRenewed={onRenewed}
          onReread={reread}
        />
        {answer !== undefined && (
          <p className="model-profiles-note">{t('model-profiles-off-note')}</p>
        )}
      </section>
    </>
  )
}

/** The model profiles as a table, or the message in their place. */
function Body({
  answer,
  filter,
  onFilter,
  sort,
  onSort,
  onRenewed,
  onReread,
}: {
  answer: Answer | undefined
  filter: string
  onFilter: (text: string) => void
  sort: Sort | null
  onSort: (sort: Sort | null) => void
  onRenewed: (session: Session) => boolean
  onReread: () => void
}) {
  if (answer === undefined) {
    // The rows' skeleton (17a.4): three lines in the table's place.
    return (
      <Loading text={t('model-profiles-loading')} shape={<Lines n={3} />} />
    )
  }
  if (answer === 'signed-out' || answer === 'revoked') {
    return <LoadingText text={t('model-profiles-loading')} />
  }
  if (!Array.isArray(answer)) {
    if (answer.error === 'client-not-authorized') {
      return <p>{t('api-client-not-authorized')}</p>
    }
    if (answer.error === 'client-unavailable') {
      return <p>{t('api-client-unavailable')}</p>
    }
    return <p>{t('model-profiles-load-failed')}</p>
  }
  if (answer.length === 0) {
    return <p>{t('model-profiles-empty')}</p>
  }
  return (
    <Filtered
      read={answer}
      filter={filter}
      onFilter={onFilter}
      sort={sort}
      onSort={onSort}
      onRenewed={onRenewed}
      onReread={onReread}
    />
  )
}

/** The GUI's rule: the whole text, lowercased, in the id or the model. */
function matches(p: ModelProfile, filter: string): boolean {
  const text = filter.toLowerCase()
  return (
    p.id.toLowerCase().includes(text) || p.model.toLowerCase().includes(text)
  )
}

/**
 * The filter field, its status line, the switches' feedback and the rows the
 * filter lets through and the sort orders, each with its switch (2c). The text
 * and the sort are the shell's (62), never this component's: they outlive the
 * page. The rows read the deferred text, so typing stays smooth on a long list
 * (U17c: `useDeferredValue`); the field itself shows what is typed.
 */
function Filtered({
  read,
  filter,
  onFilter,
  sort,
  onSort,
  onRenewed,
  onReread,
}: {
  read: ModelProfile[]
  filter: string
  onFilter: (text: string) => void
  sort: Sort | null
  onSort: (sort: Sort | null) => void
  onRenewed: (session: Session) => boolean
  onReread: () => void
}) {
  const uid = useId()
  // The rows as read, each switch's value kept up to date in place (2c.47).
  const [modelProfiles, setModelProfiles] = useState(read)
  const [busy, setBusy] = useState(false)
  const [status, setStatus] = useState('')
  // What the last header click sorted, said on the filter's status line until
  // the field is typed again (U17c).
  const [announced, setAnnounced] = useState('')
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
  const deferred = useDeferredValue(filter)
  // The rows the filter lets through, in the sort's order: the file order is
  // the order they were read in, so a tie keeps it (U17c.5).
  const shown = useMemo(() => {
    const rows = modelProfiles
      .map((p, i) => ({ p, i }))
      .filter(({ p }) => matches(p, deferred))
    if (sort === null) return rows
    const key = (p: ModelProfile) => (sort.column === 'name' ? p.id : p.engine)
    return rows.sort(
      (a, b) => compare(key(a.p), key(b.p), sort.dir) || a.i - b.i,
    )
  }, [modelProfiles, deferred, sort])
  // The header clicked: the shell keeps the new sort, the status line says it.
  const sortBy = useCallback(
    (column: Column) => {
      const next = nextSort(sort, column)
      setAnnounced(sortText(next))
      onSort(next)
    },
    [sort, onSort],
  )

  // The switch written (2c.45, 2c.47): the row's value kept in place, the
  // write, its word, and the row's value as the machine left it.
  const change = useCallback(
    async (i: number, enabled: boolean) => {
      if (busy) return
      const p = modelProfiles[i]!
      const previous = p.enabled
      last.current = i
      setModelProfiles((ps) => rowEnabled(ps, i, enabled))
      setBusy(true)
      setStatus('')
      setAlert(null)
      const result = await writeEnabled(p.id, enabled)
      if (!mounted.current) return
      if (typeof result === 'object' && 'running' in result) {
        setModelProfiles((ps) => rowEnabled(ps, i, result.enabled))
        setStatus(
          switchedText({ ...p, enabled: result.enabled }, result.running),
        )
        setBusy(false)
        return
      }
      setAlert({
        text: failureText(p.id, result),
        signIn: result === 'signed-out' || result === 'revoked',
      })
      if (result === 'signed-out' || result === 'revoked') {
        setModelProfiles((ps) => rowEnabled(ps, i, previous))
        setBusy(false)
        return
      }
      // The write may have happened: that row's value as read again (2c.48).
      const again = await readModelProfiles()
      if (!mounted.current) return
      const row = Array.isArray(again)
        ? again.find((r) => r.id === p.id)
        : undefined
      setModelProfiles((ps) =>
        rowEnabled(ps, i, row === undefined ? previous : row.enabled),
      )
      setBusy(false)
    },
    [busy, modelProfiles],
  )
  // The table's rows are memoised (U17c), so the callback it is handed is stable.
  const onChange = useCallback(
    (i: number, enabled: boolean) => void change(i, enabled),
    [change],
  )

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

  /** The field typed: the count of the rows replaces the sort's word (U17c). */
  function typed(text: string) {
    setAnnounced('')
    onFilter(text)
  }

  function clear() {
    typed('')
    field.current?.focus()
  }

  // The status line: the sort that was just made, ahead of the count of the
  // rows the filter lets through (U17c: typing clears the sort's words, so a
  // later filter change leaves the count as it was before the headers).
  const count =
    filter === ''
      ? ''
      : shown.length === 0
        ? t('model-profiles-no-match')
        : t(
            'model-profiles-filter-count',
            String(shown.length),
            String(modelProfiles.length),
          )
  const statusText = [announced, count].filter((s) => s !== '').join(' ')

  return (
    <>
      <div className="model-profiles-filter">
        <label htmlFor="model-profiles-filter">
          {t('model-profiles-filter-label')}
        </label>
        <div className="model-profiles-filter-row">
          <Icon name="search" />
          <input
            ref={field}
            id="model-profiles-filter"
            type="search"
            autoComplete="off"
            spellCheck={false}
            aria-describedby="model-profiles-filter-hint"
            value={filter}
            onChange={(e) => typed(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Escape' && filter !== '') {
                e.preventDefault()
                typed('')
              }
            }}
          />
          {filter !== '' && (
            <button type="button" className="secondary" onClick={clear}>
              {t('model-profiles-filter-clear')}
            </button>
          )}
        </div>
        <p
          id="model-profiles-filter-hint"
          className="model-profiles-filter-hint"
        >
          {t('model-profiles-filter-hint')}
        </p>
      </div>
      <p role="status" className="model-profiles-filter-status">
        {statusText}
      </p>
      <p role="status" className="model-profiles-switch-status">
        {status}
      </p>
      {alert !== null && (
        <p role="alert" className="model-profiles-switch-alert">
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
          sort={sort}
          onSort={sortBy}
          onChange={onChange}
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
 * The model profiles as a table; row `i` (its place in the list read) names its
 * switch `<uid>-switch-<i>` by its name `<uid>-name-<i>`. The table is the only
 * child of the scroll box of 10 R1.3: the box is generic (no role, no name, no
 * tabindex), every row holding a focusable control, so Tab reaches each column
 * and scrolls the box to what it focuses.
 *
 * The Model profile and Engine headers are buttons that sort the list (brief 17
 * U17c, the APG's sortable table): the sorted column's `th` carries `aria-sort`
 * and its arrow points one way, the others carry none and point both ways. The
 * rows are memoised so a keystroke in the filter, whose deferred value lands in
 * a later render, redraws them once.
 */
const Table = memo(function Table({
  uid,
  rows,
  busy,
  sort,
  onSort,
  onChange,
}: {
  uid: string
  rows: { p: ModelProfile; i: number }[]
  /** A write runs: every switch aria-disabled, its changes ignored (2c.45). */
  busy: boolean
  sort: Sort | null
  onSort: (column: Column) => void
  onChange: (i: number, enabled: boolean) => void
}) {
  /** One sortable header cell: the column's text in a button, its arrow. */
  const header = (column: Column, label: string) => {
    const dir = sort !== null && sort.column === column ? sort.dir : null
    return (
      <th
        scope="col"
        aria-sort={
          dir === null ? undefined : dir === 'asc' ? 'ascending' : 'descending'
        }
      >
        <button type="button" onClick={() => onSort(column)}>
          {label}
          <Icon
            name={
              dir === null
                ? 'arrow-up-down'
                : dir === 'asc'
                  ? 'arrow-up'
                  : 'arrow-down'
            }
          />
        </button>
      </th>
    )
  }
  return (
    <div className="table-scroll">
      <table className="model-profiles" aria-labelledby="page-heading">
        <thead>
          <tr>
            {header('name', columnLabel('name'))}
            {header('engine', columnLabel('engine'))}
            <th scope="col">{t('model-profiles-column-status')}</th>
          </tr>
        </thead>
        <tbody>
          {rows.map(({ p, i }) => (
            <tr key={p.id} className={p.enabled ? undefined : 'off'}>
              <td>
                <span id={`${uid}-name-${i}`}>
                  {modelProfileIdUnreadable(p.id) ? (
                    p.id
                  ) : (
                    <a href={modelProfileHref(p.id)}>{p.id}</a>
                  )}
                </span>
              </td>
              <td>
                {p.engineAvailable
                  ? p.engine
                  : t('model-profiles-engine-not-available', p.engine)}
              </td>
              <td>
                <label className="model-profiles-switch">
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
                  {p.enabled
                    ? t('model-profiles-status-on')
                    : t('model-profiles-status-off')}
                </span>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  )
})
