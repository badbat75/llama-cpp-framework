// The row delete the two profile lists share (brief 22, 22 D4): the − in a row's
// cell, the confirm row it opens, and the answers of the manager's DELETE. The
// pages keep their own delete (22 D.2 to D.7); this is the list's, whose words
// land in the list's own lines, above the table. Every word is a key of the
// strings file: the caller names the row's own keys, the answers are mapped
// here, one literal t() call per key, as every page's table is.
import {
  useCallback,
  useEffect,
  useId,
  useRef,
  useState,
  type ComponentProps,
} from 'react'
import { flushSync } from 'react-dom'
import {
  apiError,
  call,
  sessionEnded,
  sessionRevoked,
  type Reply,
} from './api.ts'
import { HintTip, useHint } from './Hint.tsx'
import { Icon, type IconName } from './Icon.tsx'
import { t } from './strings.ts'

/** Where the answer of a DELETE goes:
 *  - `done`: the profile is gone, the row goes with it.
 *  - `group`: the words stay in the open group, the verb active again, the list
 *    not read again (22 D4.7, 22 D.5).
 *  - `region`: the list is read again and the words go on the line above the
 *    table. `provisional` is the 504: the words show in the group first, and
 *    the read again says whether they move (22 D.7). `focus` is where the focus
 *    goes once the read has landed: `row` is that row's own −, `gone` the next
 *    row's, as after a delete. */
export type DeleteAnswer =
  | { kind: 'done' }
  | { kind: 'group'; text: string; signIn: boolean }
  | {
      kind: 'region'
      text: string
      provisional: boolean
      focus: 'row' | 'gone'
    }

/** The refusals that keep the row and disable its −: the read again shows why
 *  (22 D.4, 22 D4.7). IP5.0 took the model profile's switch out, so the manager
 *  answers the switch-on refusal no more. */
const IN_USE: Record<string, (args: string[]) => string> = {
  'client-server-profile-in-use': (args) =>
    t('api-client-server-profile-in-use', ...args),
  // Brief 26, 26.3: the instance profile an instance runs.
  'instance-profile-in-use': (args) =>
    t('api-instance-profile-in-use', ...args),
}

/** The refusals that say the profile is already gone: the read again shows the
 *  row gone (22 D.6, 22 D4.7). */
const GONE: Record<string, (args: string[]) => string> = {
  'no-model-profile-named': (args) => t('api-no-model-profile-named', ...args),
  'client-no-server-profile': () => t('api-client-no-server-profile'),
}

/** The refusal the group keeps, naming what holds the profile (22 D.5). */
const KEPT: Record<string, (args: string[]) => string> = {
  'model-profile-assigned': (args) => t('api-model-profile-assigned', ...args),
}

/** The DELETE of `path`, its answer read into the three places (22 D4.7). */
export async function deleteRow(path: string): Promise<DeleteAnswer> {
  let answer: Reply | undefined
  try {
    answer = await call(path, { method: 'DELETE' })
  } catch {
    answer = undefined
  }
  if (answer === undefined)
    return { kind: 'group', text: t('request-failed'), signIn: false }
  if (answer.ok) return { kind: 'done' }
  if (sessionEnded(answer))
    return { kind: 'group', text: t('session-ended-retry'), signIn: true }
  if (sessionRevoked(answer))
    return { kind: 'group', text: t('session-revoked'), signIn: false }
  const error = apiError(answer.body)
  const key = error?.key ?? ''
  const args = error?.args ?? []
  const inUse = IN_USE[key]
  if (inUse !== undefined)
    return {
      kind: 'region',
      text: inUse(args),
      provisional: false,
      focus: 'row',
    }
  const gone = GONE[key]
  if (gone !== undefined)
    return {
      kind: 'region',
      text: gone(args),
      provisional: false,
      focus: 'gone',
    }
  if (key === 'client-timeout')
    return {
      kind: 'region',
      text: t('delete-unknown'),
      provisional: true,
      focus: 'gone',
    }
  const kept = KEPT[key]
  if (kept !== undefined)
    return { kind: 'group', text: kept(args), signIn: false }
  if (key === 'client-unavailable')
    return { kind: 'group', text: t('api-client-unavailable'), signIn: false }
  return {
    kind: 'group',
    text:
      answer.status === 422 && key !== ''
        ? t('api-other', key)
        : t('request-failed'),
    signIn: false,
  }
}

/** The − of a row (22 D4.1): the shared 40 px round square of a list's add, the
 *  trash can (a minus where the row removes an entry and its record stays, R6),
 *  and the row's own words in a visually hidden span, so every row's − has a
 *  name of its own (2.4.6). A row that cannot be deleted keeps it
 *  focusable and `aria-disabled`, described by the reason (22 D4.2), drawn
 *  inactive (22 D4 review D2), and shows that reason as a box floating over the
 *  table on hover and on focus (22 D4.3, 22 D4.13, 22 D4.16). The box holds the
 *  button and the tip as its two children and owns the pointer and focus
 *  handlers (22 D4.13): the tip is the button's next sibling, so the pointer
 *  reaches it without crossing a gap, and it floats, so it takes no room. */
export function RowDelete({
  domId,
  name,
  reason,
  disabled,
  busy,
  open,
  onPress,
  icon = 'trash-2',
}: {
  /** The id of the button: the list focuses a row's − by it. */
  domId: string
  name: string
  /** The reason it cannot act, null when it can. */
  reason: string | null
  disabled: boolean
  /** A delete runs: every row's − goes inactive with it (22 D4.5). */
  busy: boolean
  /** Its confirm row is open. */
  open: boolean
  onPress: () => void
  /** The sign it draws. The can is a row that deletes the record the list
   *  names; a row that removes one entry of a record, the record staying, draws
   *  DESIGN.md 5.2's minus (brief 26, the IP6b review R6). */
  icon?: IconName
}) {
  const box = useRef<HTMLSpanElement>(null)
  const anchor = useRef<HTMLButtonElement>(null)
  const tip = useRef<HTMLSpanElement>(null)
  const hint = useHint(disabled, box)
  const inert = disabled || busy
  return (
    <span
      ref={box}
      className="hint-box"
      onMouseOver={hint.show}
      onMouseOut={(e) => hint.leave(e, tip)}
      onMouseLeave={hint.hide}
      onFocus={hint.show}
      onBlur={hint.hide}
      onKeyDown={(e) => {
        if (e.key === 'Escape') hint.hide()
      }}
    >
      <button
        type="button"
        id={domId}
        ref={anchor}
        className={inert ? 'ot-add ot-delete inactive' : 'ot-add ot-delete'}
        aria-expanded={open}
        aria-disabled={inert || undefined}
        aria-describedby={disabled ? `${domId}-reason` : undefined}
        onClick={() => {
          // A tap on the − that cannot act shows its reason (22 D4.18): a touch
          // has no hover, and the click of a tap does nothing else.
          if (inert) hint.show()
          else onPress()
        }}
      >
        <Icon name={icon} />
        <span className="visually-hidden">{name}</span>
      </button>
      {reason !== null && hint.shown && (
        <HintTip tipRef={tip} anchorRef={anchor} reason={reason} />
      )}
    </span>
  )
}

/** The reasons the rows' − cannot act (22 D4.2): the descriptions of the
 *  disabled ones, one span per row, out of sight and out of the cells, so a
 *  cell holds its value alone. `items` is the list's disabled rows, each named
 *  by the id of its −. */
export function DeleteReasons({
  items,
}: {
  items: { id: string; reason: string }[]
}) {
  return (
    <>
      {items.map(({ id, reason }) => (
        <span key={id} id={`${id}-reason`} className="visually-hidden" hidden>
          {reason}
        </span>
      ))}
    </>
  )
}

/** The confirm of a row's delete (22 D4.4): the question that names the row,
 *  the verb then Cancel, the status line and the alert place. It is mounted
 *  while the delete is asked, so its words are its own. */
export function RowConfirm({
  question,
  verb = t('confirm-delete'),
  status,
  alert,
  running,
  onConfirm,
  onCancel,
  onSignIn,
}: {
  question: string
  /** The verb's word: Delete, or the one a list gives this row (brief 26 26.8
   *  removes a model entry from an instance profile, the profile staying). */
  verb?: string
  status: string
  alert: { text: string; signIn: boolean } | null
  /** The delete runs: both buttons go inactive, Escape is inert (22 D4.5). */
  running: boolean
  onConfirm: () => void
  onCancel: () => void
  onSignIn: () => void
}) {
  const uid = useId()
  const questionId = `${uid}-question`

  // The focus moves to Cancel, the verb is one Tab away (22 D4.4).
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
      <p id={questionId}>{question}</p>
      <div className="save-actions">
        <button
          type="button"
          id="confirm-delete"
          className="primary danger"
          aria-disabled={running || undefined}
          onClick={onConfirm}
        >
          {verb}
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

/** The confirm as a row of the table, right after the row it asks about: one td
 *  spanning every column, so the group wraps inside the table's scroll box
 *  (22 D4.4, 22 D4.8). */
export function ConfirmRow({
  columns,
  ...confirm
}: { columns: number } & ComponentProps<typeof RowConfirm>) {
  return (
    <tr className="confirm-row">
      <td colSpan={columns}>
        <RowConfirm {...confirm} />
      </td>
    </tr>
  )
}

/** What the list gives the delete: its own path and words, and the handles the
 *  answers need. The list keeps its rows, its lines and its focus, this only
 *  moves between them. */
export type RowDeleteHost = {
  /** The DELETE of one profile. */
  path: (id: string) => string
  /** The line above the table, once the profile is gone. */
  deleted: (id: string) => string
  /** The row leaves the list, with no read (22 D4.6). */
  remove: (id: string) => void
  /** The list read again in place: the ids it has now, null when the read
   *  failed or the session is gone. */
  reread: () => Promise<string[] | null>
  /** The manager says the profile is in use, so the list shows it as it is:
   *  the Models list draws that row's switch on, where the read of the file
   *  that was saved says otherwise (22 D4.7). */
  onInUse?: (id: string) => void
  /** The ids of the rows, in the order the list shows them. */
  order: string[]
  /** A row's −, by its id. */
  minus: (id: string) => HTMLElement | null
  /** Where the focus goes when the list has no row left. */
  lastResort: () => HTMLElement | null
  /** The line above the table, and the alert place beside it. */
  onStatus: (text: string) => void
  onAlert: (alert: { text: string; signIn: boolean } | null) => void
  /** A 401: the list is asked for a sign-in. */
  onSignIn: () => void
}

/** The delete of a list's row: which row is asked, whether the delete runs, the
 *  group's status and alert, and the three handles. One at a time (22 D4.4),
 *  and one in flight (22 D4.5). */
export function useRowDelete(host: RowDeleteHost) {
  const [confirming, setConfirming] = useState<string | null>(null)
  const [deleting, setDeleting] = useState(false)
  const [status, setStatus] = useState('')
  const [alert, setAlert] = useState<{ text: string; signIn: boolean } | null>(
    null,
  )
  // The delete in flight, read by the close: a ref, so the handles stay the
  // same object from render to render (the rows are memoised).
  const running = useRef(false)
  const asked = useRef<string | null>(null)
  const kept = useRef(host)
  // The host of the last render, so the handles below stay one object from
  // render to render (the rows are memoised) and still read the current rows.
  useEffect(() => {
    kept.current = host
  })

  /** The row's − of a row that is no longer there: the next one shown, else the
   *  previous one, else the list's own control (22 D4.6). `order` is the one
   *  the list showed while the row was there, read off the host captured before
   *  the delete, not the live one: the row is out of it by then. */
  const focusNeighbour = (order: string[], id: string) => {
    const i = order.indexOf(id)
    const next = i < 0 ? null : (order[i + 1] ?? order[i - 1] ?? null)
    const el =
      next === null ? kept.current.lastResort() : kept.current.minus(next)
    el?.focus()
  }

  const open = useCallback((id: string) => {
    asked.current = id
    setConfirming(id)
    setAlert(null)
    setStatus('')
    // The line above the table is this delete's too: an older refusal that
    // left words there goes when a new one is asked (22 D4 review D4), so the
    // was-deleted line of a success stands alone.
    kept.current.onAlert(null)
  }, [])

  /** Cancel, Escape, or the − pressed again: the row goes and the focus returns
   *  to the − (22 D4.4). While the delete runs nothing closes. */
  const close = useCallback(() => {
    if (running.current) return
    const id = asked.current
    flushSync(() => {
      asked.current = null
      setConfirming(null)
      setAlert(null)
      setStatus('')
    })
    if (id !== null) kept.current.minus(id)?.focus()
  }, [])

  const confirm = useCallback(async () => {
    const id = asked.current
    if (id === null || running.current) return
    running.current = true
    setDeleting(true)
    setAlert(null)
    setStatus(t('confirm-deleting'))
    const h = kept.current
    const answer = await deleteRow(h.path(id))
    running.current = false
    setDeleting(false)
    setStatus('')
    if (answer.kind === 'done') {
      flushSync(() => {
        asked.current = null
        setConfirming(null)
        h.remove(id)
        h.onStatus(h.deleted(id))
      })
      focusNeighbour(h.order, id)
      return
    }
    if (answer.kind === 'group') {
      setAlert({ text: answer.text, signIn: answer.signIn })
      return
    }
    // The 504: the words show in the group first, so the reader of the row sees
    // them where the delete was, before the read says whether the row is gone.
    if (answer.provisional)
      flushSync(() => setAlert({ text: answer.text, signIn: false }))
    const ids = await h.reread()
    if (answer.provisional && ids !== null && ids.includes(id)) {
      // The row is still there: the group keeps the words, the focus stays on
      // the verb (22 D4.7).
      return
    }
    flushSync(() => {
      asked.current = null
      setConfirming(null)
      setAlert(null)
      h.onAlert({ text: answer.text, signIn: false })
    })
    if (answer.focus === 'row') {
      // The manager says the profile is on: the row shows it, over what the
      // read of the file gave (22 D4.7).
      h.onInUse?.(id)
      h.minus(id)?.focus()
      return
    }
    focusNeighbour(h.order, id)
  }, [])

  return { confirming, deleting, status, alert, open, close, confirm }
}
