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
  Fragment,
  type KeyboardEvent,
  type ReactNode,
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
import { FieldError, Hint } from './Field.tsx'
import { Icon } from './Icon.tsx'
import { refusalOf, type RefusalTable } from './refusal.ts'
import {
  failureText,
  switchedText,
  switchedTextIds,
  writeEnabled,
} from './modelProfileSwitch.ts'
import PageHeading from './PageHeading.tsx'
import { RestartOffer } from './RestartOffer.tsx'
import {
  ConfirmRow,
  DeleteReasons,
  RowDelete,
  useRowDelete,
} from './RowDelete.tsx'
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
  nameOf,
  onInstances,
}: {
  /** The text kept by the shell, for the tab's life (02-models.md, 62). */
  filterText: string
  onFilterText: (text: string) => void
  /** The sort kept by the shell for the tab's life, as the filter is (U17c). */
  sort: Sort | null
  onSort: (sort: Sort | null) => void
  onSignedOut: (reason: SignedOut) => void
  onRenewed: (session: Session) => boolean
  /** The name of an instance by its id, for the restart offer (16 17d.1a). */
  nameOf: (id: string) => string
  /** The app's instance list read again (16 17d.2a). */
  onInstances: () => void
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

  /** The list read again with the screen left as it is: a create whose answer
   * did not arrive in time shows the new profile if the manager made one, and
   * the form the visitor typed in stays open (22 C.7). */
  const refresh = () => {
    void readModelProfiles().then((a) => {
      if (a === 'signed-out' || a === 'revoked')
        onSignedOut(a === 'revoked' ? 'revoked' : 'ended')
      else setAnswer(a)
    })
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
          nameOf={nameOf}
          onInstances={onInstances}
        />
        {answer !== undefined && (
          <p className="model-profiles-note">{t('model-profiles-off-note')}</p>
        )}
        {Array.isArray(answer) && (
          <NewProfile rows={answer} onRenewed={onRenewed} onRefresh={refresh} />
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
  nameOf,
  onInstances,
}: {
  answer: Answer | undefined
  filter: string
  onFilter: (text: string) => void
  sort: Sort | null
  onSort: (sort: Sort | null) => void
  onRenewed: (session: Session) => boolean
  onReread: () => void
  /** The name of an instance by its id, for the restart offer (16 17d.1a). */
  nameOf: (id: string) => string
  /** The app's instance list read again (16 17d.2a). */
  onInstances: () => void
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
      nameOf={nameOf}
      onInstances={onInstances}
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
  nameOf,
  onInstances,
}: {
  read: ModelProfile[]
  filter: string
  onFilter: (text: string) => void
  sort: Sort | null
  onSort: (sort: Sort | null) => void
  onRenewed: (session: Session) => boolean
  onReread: () => void
  /** The name of an instance by its id, for the restart offer (16 17d.1a). */
  nameOf: (id: string) => string
  /** The app's instance list read again (16 17d.2a). */
  onInstances: () => void
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
  // The instances the last switch left needing a restart (16 17d.5b): the
  // offer above the table names them. The next switch replaces it when its
  // answer lands, and never clears it while its request runs, so the rows move
  // at most once per switch.
  const [restart, setRestart] = useState<string[]>([])
  const [asking, setAsking] = useState(false)
  // The sign-ins this list kept (17d review D1): each one raises the count the
  // restart offer's alert was raised against, so the offer's alert and its Sign
  // in go with the sign-in, as the switch region's own alert does.
  const [renewals, setRenewals] = useState(0)
  // The offer's button that asked for the sign-in: the dialog gives the focus
  // back to it, not to the row of the last switch (17d review D1).
  const offerControl = useRef<string | null>(null)
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
        // 16 17d.5b: the reply naming the instances to restart says them, and
        // the offer above the table offers them; one naming none keeps today's
        // words, with nothing to offer. The answer lands, so the offer the
        // previous switch left is replaced by this one's.
        if (result.restart.length > 0) {
          setStatus(
            switchedTextIds(
              p.id,
              result.enabled,
              result.restart.map((id) => nameOf(id)).join(', '),
            ),
          )
        } else {
          setStatus(
            switchedText(p.id, p.engine, result.enabled, result.running),
          )
        }
        setRestart(result.restart)
        setBusy(false)
        return
      }
      setAlert({
        text: failureText(p.id, result),
        signIn: result === 'signed-out' || result === 'revoked',
      })
      setRestart([])
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
    [busy, modelProfiles, nameOf],
  )
  // The table's rows are memoised (U17c), so the callback it is handed is stable.
  const onChange = useCallback(
    (i: number, enabled: boolean) => void change(i, enabled),
    [change],
  )

  /** The row delete (22 D4): the − of a row, its confirm row, and the answers.
   *  The list is read again in place, never through the skeleton, so the words
   *  of a refusal outlive the read that brings them (22 D4.7). */
  const del = useRowDelete({
    path: (id) => `/api/profiles/model/${encodeURIComponent(id)}`,
    deleted: (id) => t('model-profiles-deleted', id),
    remove: (id) => setModelProfiles((ps) => ps.filter((p) => p.id !== id)),
    reread: async () => {
      const again = await readModelProfiles()
      if (!Array.isArray(again)) return null
      setModelProfiles(again)
      return again.map((p) => p.id)
    },
    order: shown.map(({ p }) => p.id),
    minus: (id) => document.getElementById(`${uid}-delete-${id}`),
    // The manager refused because the profile is switched on: the row's switch
    // says so, over the file the read brought back (22 D4.7).
    onInUse: (id) =>
      setModelProfiles((ps) =>
        ps.map((p) => (p.id === id ? { ...p, enabled: true } : p)),
      ),
    // The list has no control of its own after the rows: with no row left the
    // focus stays where the browser puts it.
    lastResort: () => null,
    onStatus: setStatus,
    onAlert: setAlert,
    onSignIn: () => setAsking(true),
  })

  function closeAsking() {
    flushSync(() => setAsking(false))
    document.getElementById(`${uid}-sign-in`)?.focus()
  }

  // The group's Sign in (22 D4.7): the same dialog the switch's alert opens.
  const askSignIn = useCallback(() => setAsking(true), [])

  function signedIn(session: Session) {
    const kept = onRenewed(session)
    const offer = offerControl.current
    offerControl.current = null
    flushSync(() => {
      setAsking(false)
      setAlert(null)
      if (kept) setRenewals((n) => n + 1)
    })
    if (kept)
      document.getElementById(offer ?? `${uid}-switch-${last.current}`)?.focus()
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
      {/* 16 17d.5b: the switch's region, its alert, its Sign in and the restart
          the last switch left to make, between the filter and the table, so
          they are seen at once in a long list. The status keeps its reserved
          line, so only the offer's row of buttons can push the rows; the focus
          stays on the switch used, and the offer is replaced only when the
          next answer lands. */}
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
      {/* Keyed by its ids (17d review D2): the next switch's offer is a new
          one, with no status line and no alert carried from the offer it
          replaced. */}
      <RestartOffer
        key={restart.join(',')}
        ids={restart}
        nameOf={nameOf}
        renewals={renewals}
        onChanged={onInstances}
        onSignIn={(control) => {
          offerControl.current = control
          setAsking(true)
        }}
      />
      {shown.length > 0 && (
        <Table
          uid={uid}
          rows={shown}
          busy={busy}
          deleting={del.deleting}
          confirming={del.confirming}
          sort={sort}
          onSort={sortBy}
          onChange={onChange}
          onDelete={del.open}
          onConfirm={del.confirm}
          onCancel={del.close}
          onSignIn={askSignIn}
          confirmStatus={del.status}
          confirmAlert={del.alert}
        />
      )}
      {/* The reasons the disabled − describe (22 D4.2): out of the cells, and
          after the table, so the list's lines are all that stands above it. */}
      <DeleteReasons
        items={shown
          .filter(({ p }) => p.enabled)
          .map(({ p }) => ({
            id: `${uid}-delete-${p.id}`,
            reason: t('model-profiles-delete-needs-off'),
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
  deleting,
  confirming,
  sort,
  onSort,
  onChange,
  onDelete,
  onConfirm,
  onCancel,
  onSignIn,
  confirmStatus,
  confirmAlert,
}: {
  uid: string
  rows: { p: ModelProfile; i: number }[]
  /** A write runs: every switch aria-disabled, its changes ignored (2c.45). */
  busy: boolean
  /** A delete runs: every row's − and switch aria-disabled (22 D4.5). */
  deleting: boolean
  /** The row whose confirm row is open, null when none is (22 D4.4). */
  confirming: string | null
  sort: Sort | null
  onSort: (column: Column) => void
  onChange: (i: number, enabled: boolean) => void
  onDelete: (id: string) => void
  onConfirm: () => void
  onCancel: () => void
  onSignIn: () => void
  confirmStatus: string
  confirmAlert: { text: string; signIn: boolean } | null
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
            {/* The column of the row's actions (22 D4.23): its word is for a
                reader, so the column is announced; the eye sees no heading over
                an icon. */}
            <th scope="col">
              <span className="visually-hidden">
                {t('profiles-column-actions')}
              </span>
            </th>
          </tr>
        </thead>
        <tbody>
          {rows.map(({ p, i }) => (
            <Fragment key={p.id}>
              <tr className={p.enabled ? undefined : 'off'}>
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
                <td className="status-cell">
                  {/* The switch and its word are one line, carried by their own
                      span (22 D4.25): the cell stays a plain table cell, so it
                      centres its content in the row. */}
                  <span>
                    <label className="model-profiles-switch">
                      <input
                        id={`${uid}-switch-${i}`}
                        type="checkbox"
                        role="switch"
                        checked={p.enabled}
                        aria-labelledby={`${uid}-name-${i}`}
                        aria-disabled={busy || deleting || undefined}
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
                  </span>
                </td>
                {/* The row's actions (22 D4.23): the trash can alone, in the
                    column every row ends with. */}
                <td>
                  <RowDelete
                    domId={`${uid}-delete-${p.id}`}
                    name={t('model-profiles-delete-row', p.id)}
                    reason={
                      p.enabled ? t('model-profiles-delete-needs-off') : null
                    }
                    disabled={p.enabled}
                    busy={deleting}
                    open={confirming === p.id}
                    onPress={() => onDelete(p.id)}
                  />
                </td>
              </tr>
              {confirming === p.id && (
                <ConfirmRow
                  columns={4}
                  question={t('model-profiles-delete-question', p.id)}
                  status={confirmStatus}
                  alert={confirmAlert}
                  running={deleting}
                  onConfirm={onConfirm}
                  onCancel={onCancel}
                  onSignIn={onSignIn}
                />
              )}
            </Fragment>
          ))}
        </tbody>
      </table>
    </div>
  )
})

/* ── The New model profile form (brief 22, slice 22C1) ─────────────────── */

/**
 * One row of `GET /api/node/model-files` (fe/briefs/api-contract.md 226): the
 * full path of the manager's walk, its bytes, the shard count of a split model
 * and the first subfolder it sits in.
 */
type ModelFile = {
  path: string
  size: number
  shards: number
  category: string | null
}

/** The folder read the file picker works with, and why it gave nothing. */
type Folder =
  | { state: 'loading' }
  | { state: 'ready'; dir: string; rows: ModelFile[]; truncated: boolean }
  | { state: 'empty'; dir: string }
  | { state: 'unreadable'; arg: string }
  | { state: 'failed' }
  | { state: 'session' }

function isModelFile(value: unknown): value is ModelFile {
  return (
    isRecord(value) &&
    typeof value.path === 'string' &&
    typeof value.size === 'number' &&
    typeof value.shards === 'number'
  )
}

/** A file's own name, the place 22 P.5a's six words are looked for: the part
 * after the last `/` or `\`. */
function fileBaseName(path: string): string {
  const at = Math.max(path.lastIndexOf('/'), path.lastIndexOf('\\'))
  return path.slice(at + 1)
}

/** The files no picker offers: a projector, an imatrix and a draft's file,
 * named as llama.cpp names them (22 P.5a, `NOT_A_MODEL` in
 * `crates/core/src/hf.rs`): the file's own name, letter case kept. */
const NOT_A_MODEL = /mmproj|imatrix|mtp-|eagle3-|dflash-|dspark-/

/** The path as the folder's own `dir` spells it, what the typed text filters
 * (22 P.5a). A path outside `dir` is kept whole. */
function fileRelative(path: string, dir: string): string {
  return path.startsWith(`${dir}/`) ? path.slice(dir.length + 1) : path
}

/** The bytes as one option reads them: GiB with one decimal, MiB with none
 * below 0.1 GiB (brief 22, section 2). */
function fileSize(bytes: number): string {
  const gib = bytes / 2 ** 30
  return gib < 0.1
    ? `${Math.round(bytes / 2 ** 20)} MiB`
    : `${gib.toFixed(1)} GiB`
}

/** What one option reads: the path under `dir`, the size, and the shard count
 * of a split model (22 P.5). */
function fileOption(row: ModelFile, dir: string): string {
  const parts =
    row.shards > 1 ? `, ${t('model-file-parts', String(row.shards))}` : ''
  return `${fileRelative(row.path, dir)}, ${fileSize(row.size)}${parts}`
}

/** The options the list shows at most (22 P.5). */
const OPTION_MAX = 50

/** The name a model profile accepts (22 C.4), the rule the manager answers with
 * in `invalid-model-profile-id`. */
const NAME_ALLOWED = /^[A-Za-z0-9._-]+$/

/** The create answers with the words of its own keys; each key is one literal
 * t() call, as every page's table is. The field a refusal marks is the one the
 * reply names, except the taken name: the manager's 409 names no field and it
 * is Name's (22 C.6). */
const CREATE_REFUSALS: RefusalTable = {
  text: (key, args) => {
    switch (key) {
      case 'invalid-model-profile-id':
        return t('api-invalid-model-profile-id', ...args)
      case 'model-profile-name-taken':
        return t('api-model-profile-name-taken', ...args)
      case 'no-model-profile-named':
        return t('api-no-model-profile-named', ...args)
      case 'comment-markers':
        return t('api-comment-markers', ...args)
      case 'model-profile-no-model':
        return t('api-model-profile-no-model', ...args)
      default:
        return undefined
    }
  },
  field: (key) => (key === 'model-profile-name-taken' ? 'id' : undefined),
}

/**
 * The list's add (22 C.1a): a `+` right after the note that belongs to the
 * table, disclosing the New model profile form after it. Everything the visitor
 * typed is the form's own, so closing it clears all of it (22 C.3).
 */
function NewProfile({
  rows,
  onRenewed,
  onRefresh,
}: {
  rows: ModelProfile[]
  onRenewed: (session: Session) => boolean
  onRefresh: () => void
}) {
  const opener = useRef<HTMLButtonElement>(null)
  const [open, setOpen] = useState(false)
  // A create is in flight: the form stays open with it (17b review R1).
  const sendingRef = useRef(false)

  /** Cancel, Escape, or the `+` pressed again: the form goes with every field,
   * error and alert, `aria-expanded` is false, and the focus returns to the `+`
   * (22 C.3). Nothing closes while a create runs. */
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
        id="model-profiles-new"
        className="ot-add"
        aria-expanded={open}
        onClick={() => (open ? close() : setOpen(true))}
      >
        <Icon name="plus" />
        <span className="visually-hidden">{t('model-profiles-new')}</span>
      </button>
      {open && (
        <NewProfileForm
          rows={rows}
          sendingRef={sendingRef}
          onClose={close}
          onRenewed={onRenewed}
          onRefresh={onRefresh}
        />
      )}
    </>
  )
}

/**
 * The form itself, mounted while it is open (22 C.1): the note, the Name, the
 * Start from picker with this slice's two choices (a model file, a copy of
 * another profile), Create and Cancel, the folder's state line, the form's own
 * status line and alert place.
 */
function NewProfileForm({
  rows,
  sendingRef,
  onClose,
  onRenewed,
  onRefresh,
}: {
  rows: ModelProfile[]
  /** Set while the POST runs, read by the close (17b review R1). */
  sendingRef: { current: boolean }
  onClose: () => void
  onRenewed: (session: Session) => boolean
  /** The list read again with the form left open (22 C.7). */
  onRefresh: () => void
}) {
  const uid = useId()
  const titleId = `${uid}-title`
  const nameId = `${uid}-name`
  const fileRadioId = `${uid}-file-radio`
  const copyRadioId = `${uid}-copy-radio`
  const fileId = `${uid}-file`
  const listId = `${uid}-list`
  const copyId = `${uid}-copy`
  const [name, setName] = useState('')
  const [source, setSource] = useState<'file' | 'copy'>('file')
  const [from, setFrom] = useState('')
  const [text, setText] = useState('')
  const [picked, setPicked] = useState('')
  const [folder, setFolder] = useState<Folder>({ state: 'loading' })
  // The retry stays mounted while the read it started is open, so its focus
  // survives the retry (22 P.6).
  const [canRetry, setCanRetry] = useState(false)
  // The list starts closed and stays closed when the folder's rows land: the
  // state line announces them. Down, an edit or a click in the box opens it,
  // Escape, a pick or the focus leaving closes it (22 P.3, 22 P.4).
  const [listOpen, setListOpen] = useState(false)
  const [active, setActive] = useState<number | null>(null)
  const [errors, setErrors] = useState<{
    id?: string
    model?: string
    from?: string
  }>({})
  const [sent, setSent] = useState(false)
  const [status, setStatus] = useState('')
  const [alert, setAlert] = useState<{ text: string; signIn: boolean } | null>(
    null,
  )
  const [asking, setAsking] = useState(false)
  // Where a sign-in in the dialog sends the focus back to (22 C.7).
  const signedInTo = useRef<'create' | 'folder'>('create')

  // The Name takes the focus when the form opens (22 C.1).
  useEffect(() => {
    document.getElementById(nameId)?.focus()
  }, [nameId])

  /** The folder read answers the folder's state from the manager's reply, and
   * writes nothing: whoever starts the read writes the line it shows while it
   * is open (22 P.4, 22 P.6). */
  const readFiles = useCallback(async (): Promise<Folder> => {
    let answer: Reply | undefined
    try {
      answer = await call('/api/node/model-files')
    } catch {
      answer = undefined
    }
    const reply = answer
    if (reply === undefined) return { state: 'failed' }
    if (sessionEnded(reply)) return { state: 'session' }
    const body = reply.body
    if (
      !reply.ok ||
      !isRecord(body) ||
      !Array.isArray(body.files) ||
      !body.files.every(isModelFile)
    ) {
      // A read that did not answer with the folder says so and offers the
      // retry: the manager being down is one of them (22 P.6).
      return { state: 'failed' }
    }
    const dir = typeof body.dir === 'string' ? body.dir : ''
    if (isRecord(body.error) && typeof body.error.key === 'string') {
      const args = Array.isArray(body.error.args) ? body.error.args : []
      return {
        state: 'unreadable',
        arg: typeof args[0] === 'string' ? args[0] : dir,
      }
    }
    const files = body.files as ModelFile[]
    if (files.length === 0) return { state: 'empty', dir }
    return {
      state: 'ready',
      dir,
      rows: files,
      truncated: body.truncated === true,
    }
  }, [])

  // The showing of the file field reads the folder once (22 P.4): switching the
  // picker hides the field, it never unmounts, so nothing is read again.
  useEffect(() => {
    let live = true
    readFiles().then((next) => {
      if (!live) return
      setFolder(next)
      setCanRetry(next.state !== 'ready' && next.state !== 'session')
    })
    return () => {
      live = false
    }
  }, [readFiles])

  /** A read the visitor starts: the retry (22 P.6), and the sign-in on the
   * folder's line. The retry stays offered while it runs, so its focus
   * survives. */
  async function readFolder() {
    const next = await readFiles()
    setFolder(next)
    setCanRetry(next.state !== 'ready' && next.state !== 'session')
  }

  /** The rows the list shows: the folder's main models, narrowed by the typed
   * text. Text equal to a picked path keeps the list whole with that row marked
   * picked, so the visitor sees what is picked (22 P.3, 22 P.5a). */
  const shown = useMemo(() => {
    if (folder.state !== 'ready') return []
    const keep = folder.rows.filter(
      (r) => !NOT_A_MODEL.test(fileBaseName(r.path)),
    )
    if (picked !== '' && keep.some((r) => r.path === picked)) return keep
    const q = text.trim().toLowerCase()
    if (q === '') return keep
    return keep.filter((r) =>
      fileRelative(r.path, folder.dir).toLowerCase().includes(q),
    )
  }, [folder, text, picked])

  const expanded = listOpen && shown.length > 0
  const activeAt =
    expanded && active !== null ? Math.min(active, shown.length - 1) : null
  const dir = folder.state === 'ready' ? folder.dir : ''

  /** The folder's one state line (22 P.6): one text at a time, the walk's own
   * truncation said after the count. */
  const folderText = folderTextOf(folder, shown.length)

  /** A pick writes the row's whole path and closes the list (22 P.3). */
  function pick(row: ModelFile) {
    setText(row.path)
    setPicked(row.path)
    setListOpen(false)
    setActive(null)
    setErrors((e) => (e.model === undefined ? e : { ...e, model: undefined }))
  }

  function onFileKey(e: KeyboardEvent<HTMLInputElement>) {
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      if (shown.length === 0) return
      e.preventDefault()
      if (!listOpen) {
        setListOpen(true)
        setActive(0)
        return
      }
      setActive((at) => {
        const last = shown.length - 1
        const cur = at ?? (e.key === 'ArrowDown' ? -1 : last)
        const next = e.key === 'ArrowDown' ? cur + 1 : cur - 1
        return Math.max(Math.min(next, last), 0)
      })
      return
    }
    if (e.key === 'Enter') {
      // Enter picks the active option; with none active, and with the list
      // closed, it is the form's own and submits (22 C.8).
      if (activeAt === null) return
      e.preventDefault()
      const row = shown[activeAt]
      if (row !== undefined) pick(row)
      return
    }
    if (e.key === 'Escape') {
      // Escape closes the list and keeps the text, and the form stays open
      // (22 P.3): the key never reaches the form's own Escape.
      if (!listOpen) return
      e.stopPropagation()
      e.preventDefault()
      setListOpen(false)
      setActive(null)
    }
  }

  /** Create: the page's checks in their order, the focus on the first field
   * that fails, nothing sent while any fails (22 C.4). */
  function create() {
    if (sendingRef.current) return
    const id = name.trim()
    const next: { id?: string; model?: string; from?: string } = {}
    if (id === '') next.id = t('field-error-required')
    else if (!NAME_ALLOWED.test(id))
      next.id = t('api-invalid-model-profile-id', id)
    else if (rows.some((r) => r.id.toLowerCase() === id.toLowerCase()))
      next.id = t('api-model-profile-name-taken', id)
    // A file choice with no pick sends nothing, whatever the box holds. The
    // words go under the box only when it has text in them to point at (22 C.4).
    let needsPick = false
    if (source === 'file') {
      if (picked === '') {
        needsPick = true
        if (text.trim() !== '') next.model = t('field-error-pick-file')
      } else if (/[;#]/.test(picked))
        next.model = t('field-error-comment-markers')
    } else if (from === '') {
      next.from = t('field-error-choose')
    }
    const field =
      next.id !== undefined
        ? nameId
        : next.model !== undefined
          ? fileId
          : next.from !== undefined
            ? copyId
            : null
    if (field !== null) {
      // The error is in the DOM, and names the field, before the focus lands
      // (17b review D1).
      flushSync(() => setErrors(next))
      document.getElementById(field)?.focus()
      return
    }
    if (needsPick) {
      // An empty box is the one field that has to be filled: the focus goes
      // there, its state line already says what the folder holds (22 C.4).
      document.getElementById(fileId)?.focus()
      return
    }
    setErrors({})
    void send(id)
  }

  async function send(id: string) {
    sendingRef.current = true
    setSent(true)
    setAlert(null)
    setStatus(t('create-creating'))
    const body =
      source === 'copy' ? { id, from } : { id, values: { model: picked } }
    let answer: Reply | undefined
    try {
      answer = await call('/api/profiles/model', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(body),
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
      // The new profile's page, whose h1 takes the focus (22 C.5a).
      window.location.hash = modelProfileHref(id)
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
    if (key === 'client-timeout') {
      // It may or may not have been created: the list read again says (22 C.7).
      setAlert({ text: t('create-unknown'), signIn: false })
      onRefresh()
      return
    }
    if (key === 'client-unavailable') {
      setAlert({ text: t('api-client-unavailable'), signIn: false })
      return
    }
    const refusal = refusalOf(reply.body, CREATE_REFUSALS)
    const field = refusal?.field
    const box =
      field === 'id'
        ? nameId
        : field === 'from'
          ? copyId
          : field === 'model'
            ? fileId
            : null
    if (refusal !== undefined && box !== null) {
      flushSync(() => setErrors({ [field as string]: refusal.text }))
      document.getElementById(box)?.focus()
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

  /** A sign-in in the dialog (22 C.7): the form keeps what was typed and
   * picked, nothing is sent again, and the focus returns to Create. */
  function signedIn(session: Session) {
    const kept = onRenewed(session)
    const to = signedInTo.current
    flushSync(() => {
      setAsking(false)
      if (!kept) return
      if (to === 'create') setAlert(null)
      else setFolder({ state: 'loading' })
    })
    if (!kept) return
    if (to === 'create') document.getElementById('create-button')?.focus()
    else void readFolder()
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
        <h2 id={titleId}>{t('model-profiles-new-title')}</h2>
        <p className="note">{t('model-profiles-new-note')}</p>
        <CreateField
          id={nameId}
          label={t('model-profile-name')}
          hint={t('model-profile-name-hint')}
          error={errors.id}
        >
          <input
            id={nameId}
            type="text"
            className="mono"
            spellCheck={false}
            autoCapitalize="off"
            autoCorrect="off"
            autoComplete="off"
            value={name}
            onChange={(e) => {
              setName(e.target.value)
              setErrors((prev) =>
                prev.id === undefined ? prev : { ...prev, id: undefined },
              )
            }}
            {...(errors.id === undefined
              ? {}
              : {
                  'aria-invalid': true,
                  'aria-describedby': `${nameId}-error`,
                })}
          />
        </CreateField>
        <fieldset className="field">
          <legend className="field-label">
            {t('model-profile-start-from')}
          </legend>
          <div className="field-control">
            <label htmlFor={fileRadioId}>
              <input
                id={fileRadioId}
                type="radio"
                name={`${uid}-source`}
                checked={source === 'file'}
                onChange={() => setSource('file')}
              />
              {t('model-source-file')}
            </label>
            {rows.length > 0 && (
              <label htmlFor={copyRadioId}>
                <input
                  id={copyRadioId}
                  type="radio"
                  name={`${uid}-source`}
                  checked={source === 'copy'}
                  onChange={() => setSource('copy')}
                />
                {t('model-source-copy')}
              </label>
            )}
          </div>
        </fieldset>
        <CreateField
          id={fileId}
          label={t('model-profile-model-file')}
          hint={t('model-file-hint')}
          error={errors.model}
          hidden={source !== 'file'}
          after={
            <>
              {/* The suggestions the box points at (22 P.3): a listbox in the
                form's flow under the input, never floating, `hidden` while
                closed. */}
              <ul
                id={listId}
                className="tag-options"
                role="listbox"
                aria-label={t('model-profile-model-file')}
                hidden={!expanded}
              >
                {shown.slice(0, OPTION_MAX).map((row, at) => (
                  <li
                    key={row.path}
                    id={`${listId}-option-${at}`}
                    role="option"
                    aria-selected={row.path === picked}
                    onMouseDown={(e) => {
                      // Chosen as the click ends, so the box never loses it first
                      // (15 C2.4).
                      e.preventDefault()
                    }}
                    onClick={() => pick(row)}
                  >
                    {fileOption(row, dir)}
                  </li>
                ))}
              </ul>
              <p role="status" className="save-status" aria-live="polite">
                {folderText}
              </p>
              {canRetry && (
                <button
                  type="button"
                  className="secondary"
                  onClick={() => {
                    setFolder({ state: 'loading' })
                    void readFolder()
                  }}
                >
                  {t('model-files-retry')}
                </button>
              )}
              {folder.state === 'session' && (
                <button
                  type="button"
                  className="secondary"
                  onClick={() => {
                    signedInTo.current = 'folder'
                    setAsking(true)
                  }}
                >
                  {t('session-sign-in')}
                </button>
              )}
            </>
          }
        >
          <input
            id={fileId}
            type="text"
            className="mono"
            role="combobox"
            aria-autocomplete="list"
            aria-controls={listId}
            aria-expanded={expanded}
            {...(activeAt === null
              ? {}
              : { 'aria-activedescendant': `${listId}-option-${activeAt}` })}
            spellCheck={false}
            autoCapitalize="off"
            autoCorrect="off"
            autoComplete="off"
            value={text}
            onChange={(e) => {
              // Typing filters what the folder gave and leaves no option picked
              // (22 P.3).
              setText(e.target.value)
              setPicked('')
              setListOpen(true)
              setActive(null)
              setErrors((prev) =>
                prev.model === undefined ? prev : { ...prev, model: undefined },
              )
            }}
            onKeyDown={onFileKey}
            onClick={() => {
              // A click in the box asks for the list (22 P.3).
              setListOpen(true)
            }}
            onBlur={() => {
              // A box the visitor left has no list open under it (22 P.3). The
              // options keep the focus, so a click on one is no blur.
              setListOpen(false)
            }}
            {...(errors.model === undefined
              ? {}
              : {
                  'aria-invalid': true,
                  'aria-describedby': `${fileId}-error`,
                })}
          />
        </CreateField>
        {rows.length > 0 && (
          <CreateField
            id={copyId}
            label={t('model-profile-copy-from')}
            hint={t('model-profile-copy-hint')}
            error={errors.from}
            hidden={source !== 'copy'}
          >
            <select
              id={copyId}
              value={from}
              onChange={(e) => {
                setFrom(e.target.value)
                setErrors((prev) =>
                  prev.from === undefined ? prev : { ...prev, from: undefined },
                )
              }}
              {...(errors.from === undefined
                ? {}
                : {
                    'aria-invalid': true,
                    'aria-describedby': `${copyId}-error`,
                  })}
            >
              <option value="">{t('model-profile-copy-choose')}</option>
              {rows.map((r) => (
                <option key={r.id} value={r.id}>
                  {r.id}
                </option>
              ))}
            </select>
          </CreateField>
        )}
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
            onClick={() => {
              signedInTo.current = 'create'
              setAsking(true)
            }}
          >
            {t('session-sign-in')}
          </button>
        )}
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
            className="secondary"
            aria-disabled={sent || undefined}
            onClick={onClose}
          >
            {t('create-cancel')}
          </button>
        </div>
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

/** The folder's one state line's text (22 P.6): one of the states, the walk's
 * own truncation said after the count. */
function folderTextOf(folder: Folder, matching: number): string {
  switch (folder.state) {
    case 'loading':
      return t('model-files-loading')
    case 'empty':
      return t('model-files-empty', folder.dir)
    case 'unreadable':
      return t('model-files-unreadable', folder.arg)
    case 'failed':
      return t('model-files-failed')
    case 'session':
      return t('session-ended-retry')
    case 'ready': {
      const one =
        matching > OPTION_MAX
          ? t('model-files-more', String(OPTION_MAX), String(matching))
          : matching === 1
            ? t('model-files-count-one')
            : matching === 0
              ? t('model-files-no-match')
              : t('model-files-count', String(matching))
      return folder.truncated ? `${one} ${t('model-files-truncated')}` : one
    }
  }
}

/** One field of the create form (fe/DESIGN.md 5.2): its label with the About
 * toggle, the hint behind it, the control, what the control's own line adds
 * under it, and the refusal's text last. `hidden` keeps the field of the choice
 * that is not checked mounted, so what was typed in it stays (22 P.1). */
function CreateField({
  id,
  label,
  hint,
  error,
  hidden = false,
  after,
  children,
}: {
  id: string
  label: string
  hint: string
  /** The refusal's text under the control, when there is one. */
  error: string | undefined
  hidden?: boolean
  /** What the control adds under it: the listbox and the folder's line. */
  after?: ReactNode
  children: ReactNode
}) {
  const [open, setOpen] = useState(false)
  const hintId = `${id}-hint`
  return (
    <div className="field" hidden={hidden}>
      <div className="field-label">
        <label htmlFor={id}>{label}</label>
        {/* The toggle of Field.tsx's About, with its name built from the label
            as an `aria-label` instead of a hidden span: the form's field names
            the control and nothing else carries that text, so a reader and a
            test find the label once (22 C.2, 22 C.8). */}
        <button
          type="button"
          className="about"
          aria-expanded={open}
          aria-controls={hintId}
          aria-label={`${t('field-about')} ${label}`}
          onClick={() => setOpen((o) => !o)}
        >
          {t('field-about')}
        </button>
      </div>
      <Hint id={hintId} text={hint} open={open} />
      <div className="field-control">{children}</div>
      {/* What the control adds goes in one element of its own, so the 720 px
          field rule places the whole of it in the control column: left as the
          field's own children, the list and the line fell into the 200 px
          label column (22C1 review D1, as 19 LS1 D1 and 21 TP1 D1). */}
      {after !== undefined && <div className="field-after">{after}</div>}
      {error !== undefined && <FieldError id={`${id}-error`} text={error} />}
    </div>
  )
}
