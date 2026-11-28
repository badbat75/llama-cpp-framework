// The Model profiles page (#/profiles/model, fe/briefs/02-models.md, slice 2a,
// under the nav's Profiles group since 05b): the model profiles of every engine
// read from the bff's /api/profiles/model, with a filter over them (slice 2b),
// and its Model profile and Engine headers sorting the list (brief 17, U17c).
// Brief 22 slice IP5 (IP5.1, IP5.2, IP5.3) took the row switch and its Status
// column out: the row says which instance profiles list the model, and a row
// one lists cannot be deleted.
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
} from 'react'
import { flushSync } from 'react-dom'
import { apiError, call, isRecord, sessionRevoked } from './api.ts'
import Dialog from './Dialog.tsx'
import { Icon } from './Icon.tsx'
import { NewProfile } from './NewModelProfile.tsx'
import PageHeading from './PageHeading.tsx'
import {
  modelProfileInUse,
  modelProfileInUseReason,
} from './modelProfileDelete.ts'
import {
  ConfirmRow,
  DeleteReasons,
  RowDelete,
  useRowDelete,
} from './RowDelete.tsx'
import {
  instanceProfileHref,
  modelProfileHref,
  modelProfileIdUnreadable,
} from './route.ts'
import type { Session, SignedOut } from './session.ts'
import SignIn from './SignIn.tsx'
import { Lines, Loading, LoadingText } from './Skeleton.tsx'
import { t } from './strings.ts'

export type ModelProfile = {
  id: string
  engine: string
  engineAvailable: boolean
  /** The instance profiles that list this model, in their file order (IP5.0).
   *  `removable` is this list being empty. */
  usedBy?: string[]
  model: string
}

/**
 * The sort of the list (brief 17 U17c): one column, ascending or descending,
 * and `null` for the file order, the order the list was read in. Only the
 * Model profile and Engine columns sort: IP5.1 took the Status column out, and
 * Used by has no order of its own to give.
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
        />
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
 * The filter field, its status line, and the rows the filter lets through and
 * the sort orders (2c, U17c). The text and the sort are the shell's (62), never
 * this component's: they outlive the page. The rows read the deferred text, so
 * typing stays smooth on a long list (U17c: `useDeferredValue`); the field
 * itself shows what is typed.
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
  // The rows as read; a delete takes one out in place (22 D4.6).
  const [rows, setRows] = useState(read)
  const [status, setStatus] = useState('')
  // What the last header click sorted, said on the filter's status line until
  // the field is typed again (U17c).
  const [announced, setAnnounced] = useState('')
  const [alert, setAlert] = useState<{ text: string; signIn: boolean } | null>(
    null,
  )
  const [asking, setAsking] = useState(false)
  const field = useRef<HTMLInputElement>(null)
  const deferred = useDeferredValue(filter)
  // The rows the filter lets through, in the sort's order: the file order is
  // the order they were read in, so a tie keeps it (U17c.5).
  const shown = useMemo(() => {
    const listed = rows
      .map((p, i) => ({ p, i }))
      .filter(({ p }) => matches(p, deferred))
    if (sort === null) return listed
    const key = (p: ModelProfile) => (sort.column === 'name' ? p.id : p.engine)
    return listed.sort(
      (a, b) => compare(key(a.p), key(b.p), sort.dir) || a.i - b.i,
    )
  }, [rows, deferred, sort])
  // The header clicked: the shell keeps the new sort, the status line says it.
  const sortBy = useCallback(
    (column: Column) => {
      const next = nextSort(sort, column)
      setAnnounced(sortText(next))
      onSort(next)
    },
    [sort, onSort],
  )

  /** The row delete (22 D4): the − of a row, its confirm row, and the answers.
   *  The list is read again in place, never through the skeleton, so the words
   *  of a refusal outlive the read that brings them (22 D4.7). */
  const del = useRowDelete({
    path: (id) => `/api/profiles/model/${encodeURIComponent(id)}`,
    deleted: (id) => t('model-profiles-deleted', id),
    remove: (id) => setRows((ps) => ps.filter((p) => p.id !== id)),
    reread: async () => {
      const again = await readModelProfiles()
      if (!Array.isArray(again)) return null
      setRows(again)
      return again.map((p) => p.id)
    },
    order: shown.map(({ p }) => p.id),
    minus: (id) => document.getElementById(`${uid}-delete-${id}`),
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

  // The group's Sign in (22 D4.7): the same dialog the alert opens.
  const askSignIn = useCallback(() => setAsking(true), [])

  /** A sign-in in the dialog: the list keeps its rows, the words of the refusal
   *  go with the dialog, and the focus returns to the Sign in (22 D4.7). */
  function signedIn(session: Session) {
    const kept = onRenewed(session)
    flushSync(() => {
      setAsking(false)
      setAlert(null)
    })
    if (kept) document.getElementById(`${uid}-sign-in`)?.focus()
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
            String(rows.length),
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
      {/* IP5.1: the switch's region, its alert and the restart offer of 17d.5b
          go with the row switch. The row delete's words (22 D4.7) keep a line
          above the table, as on the Server profiles list. */}
      <p role="status" className="save-status">
        {status}
      </p>
      {alert !== null && <p role="alert">{alert.text}</p>}
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
          deleting={del.deleting}
          confirming={del.confirming}
          sort={sort}
          onSort={sortBy}
          onDelete={del.open}
          onConfirm={del.confirm}
          onCancel={del.close}
          onSignIn={askSignIn}
          confirmStatus={del.status}
          confirmAlert={del.alert}
        />
      )}
      {/* The reasons the disabled − describe (22 D4.2): out of the cells, and
          after the table, so the list's lines are all that stands above it. A
          row an instance profile lists cannot be deleted (IP5.3). */}
      <DeleteReasons
        items={shown
          .filter(({ p }) => (p.usedBy ?? []).length > 0)
          .map(({ p }) => ({
            id: `${uid}-delete-${p.id}`,
            reason: t('model-profiles-delete-in-use'),
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
  deleting,
  confirming,
  sort,
  onSort,
  onDelete,
  onConfirm,
  onCancel,
  onSignIn,
  confirmStatus,
  confirmAlert,
}: {
  uid: string
  rows: { p: ModelProfile; i: number }[]
  /** A delete runs: every row's − is aria-disabled (22 D4.5). */
  deleting: boolean
  /** The row whose confirm row is open, null when none is (22 D4.4). */
  confirming: string | null
  sort: Sort | null
  onSort: (column: Column) => void
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
            <th scope="col">{t('model-profiles-column-used-by')}</th>
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
          {rows.map(({ p, i }) => {
            // The engine as the name cell's second line (22 D4.35): the sheet
            // hides the Engine column below 480 px, where this line is what
            // says what runs the profile. It stands beside the span that names
            // the row's link and delete (2d.60, the designer's ruling of
            // 2026-11-27).
            const engine = p.engineAvailable
              ? p.engine
              : t('model-profiles-engine-not-available', p.engine)
            // The instance profiles that list this model (IP5.2): each is a
            // link named by its id alone, the D4.37 rule.
            const usedBy = p.usedBy ?? []
            return (
              <Fragment key={p.id}>
                <tr>
                  <td>
                    <span id={`${uid}-name-${i}`}>
                      {modelProfileIdUnreadable(p.id) ? (
                        p.id
                      ) : (
                        <a href={modelProfileHref(p.id)}>{p.id}</a>
                      )}
                    </span>
                    <span className="muted">{engine}</span>
                  </td>
                  <td>{engine}</td>
                  <td>
                    {usedBy.length === 0 ? (
                      <span className="muted">
                        {t('model-profiles-unused')}
                      </span>
                    ) : (
                      usedBy.map((id, n) => (
                        <Fragment key={id}>
                          {n > 0 && ', '}
                          <a href={instanceProfileHref(id)}>{id}</a>
                        </Fragment>
                      ))
                    )}
                  </td>
                  {/* The row's actions (22 D4.23): the trash can alone, in the
                    column every row ends with. */}
                  <td>
                    <RowDelete
                      domId={`${uid}-delete-${p.id}`}
                      name={t('model-profiles-delete-row', p.id)}
                      reason={modelProfileInUseReason(p.usedBy)}
                      disabled={modelProfileInUse(p.usedBy)}
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
            )
          })}
        </tbody>
      </table>
    </div>
  )
})
