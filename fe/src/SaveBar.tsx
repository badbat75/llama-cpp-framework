// The bottom save bar of a form page: the status line, the alert and its
// answer button, Discard and Save, and the scroll padding that follows the
// bar's rendered height. ServerForm (the Server page) and the model profile
// page share it, so a save answers the same way on both.
// The text is in en_US.json.
import {
  useEffect,
  useRef,
  useState,
  type ReactNode,
  type RefObject,
} from 'react'
import { flushSync } from 'react-dom'
import { t } from './strings.ts'

/** What the status line says. `saving` while a save runs; `unsaved` while
 * there is something to save; `saved` and `saved-restart` are what a save
 * left behind, kept until the next save; `idle` shows nothing. */
export type SaveStatus =
  'idle' | 'saving' | 'saved' | 'saved-restart' | 'unsaved'

/** The bar's alert, with the button that answers it when there is one. */
export type SaveAlert = {
  text: string
  action?: 'reload' | 'sign-in'
  /** A `sign-in` alert: the page's sign-ins kept when it was set. */
  at?: number
}

/** The status line's text: empty for `idle`. */
function statusLine(status: SaveStatus): string {
  return {
    idle: '',
    saving: t('save-saving'),
    unsaved: t('save-unsaved'),
    saved: t('save-saved'),
    'saved-restart': t('save-saved-restart'),
  }[status]
}

/** Announces unsaved edits in the Overview's banner (it is the one that
 * leaves). It says it on the way out too, so the banner of a page that is
 * gone is never left claiming edits. */
export function useDirtyReport(
  dirty: boolean,
  onDirty: (dirty: boolean) => void,
) {
  useEffect(() => {
    onDirty(dirty)
    return () => onDirty(false)
  }, [dirty, onDirty])
}

/** Closing or reloading the tab asks only while there is something to lose. */
export function useWarnOnLeave(dirty: boolean) {
  useEffect(() => {
    if (!dirty) return
    const ask = (e: BeforeUnloadEvent) => {
      e.preventDefault()
      // For browsers that predate preventDefault() here (brief, 38).
      e.returnValue = true
    }
    window.addEventListener('beforeunload', ask)
    return () => window.removeEventListener('beforeunload', ask)
  }, [dirty])
}

/** The ask a bar's answer button makes before it acts (01's 38, 2h's 145):
 * with something to lose (`ask`), pressing it opens the discard dialog,
 * keeping the answer closes it and puts the focus back on the button,
 * discarding runs the action; with nothing to lose the action runs at once. */
export function useAsk(
  ask: boolean,
  onGo: () => void,
  onFocusBack: () => void,
): {
  asking: boolean
  press: () => void
  keep: () => void
  confirm: () => void
} {
  const [asking, setAsking] = useState(false)
  return {
    asking,
    press: () => {
      if (ask) setAsking(true)
      else onGo()
    },
    keep: () => {
      flushSync(() => setAsking(false))
      onFocusBack()
    },
    confirm: () => {
      setAsking(false)
      onGo()
    },
  }
}

/** The bar's alert of a save, read the same way by the Server page's form and
 * the model profile page (01's 36, 2h's 151 to 153): a `sign-in` alert set when the
 * page's sign-ins were at `at` goes when a later sign-in kept the page, which
 * is what raises `renewals`.
 *
 * The one rule of both pages (2h.4b's 198b, 198c, 198g, 198d): an alert with no
 * answer button speaks of the edits a save did not keep, so a form with no
 * edits left (`dirty` false) has nothing for it to say. Every route to a clean
 * form counts, the edit typed back and Discard alike, so a page's Discard
 * clears only the field marks and leaves the bar's alert to this rule. The
 * answer of a save in flight (`saving`) that lands on a clean form sets none,
 * and the alert does not come back when the form turns dirty again: the next
 * Save shows its own (01's 30a). The alerts with a button stay, a clean form
 * still holds the stale revision the 409 names, and Sign in is still the way
 * back to a session. A Save click the client checks refuse is not the answer
 * of a save, so it shows its own alert even on a form with nothing to save
 * (2h's 157, 166: a loaded value). */
export function useSaveAlert(
  renewals: number,
  dirty: boolean,
  saving: boolean,
): {
  alert: SaveAlert | null
  setAlert: (alert: SaveAlert | null) => void
  /** A save the session ended: 401 (151), or 403 session-not-administrator (153). */
  sessionEnded: (revoked: boolean) => void
} {
  const [stored, setStored] = useState<SaveAlert | null>(null)
  const alert =
    stored?.action === 'sign-in' && stored.at !== renewals ? null : stored
  // The sign-in count the answer of a save arrives at (01's 43b).
  const at = useRef(renewals)
  useEffect(() => {
    at.current = renewals
  }, [renewals])
  // The dirtiness and the running save as the page last rendered them: the
  // answer of a save reads them after an edit made while it ran (01's 33b).
  const form = useRef({ dirty, saving })
  useEffect(() => {
    form.current = { dirty, saving }
  }, [dirty, saving])
  const [wasDirty, setWasDirty] = useState(dirty)
  if (wasDirty !== dirty) {
    // An edit is the answer to an alert that speaks of the edits a save did not
    // keep, so the form with no edits left has nothing for it to say: the alert
    // goes in the same pass the form turns clean, and stays gone when it turns
    // dirty again. React re-renders before the screen is written.
    setWasDirty(dirty)
    if (!dirty)
      setStored((a) => (a === null || a.action !== undefined ? a : null))
  }
  const setAlert = (a: SaveAlert | null) =>
    setStored(
      a?.action === undefined && form.current.saving && !form.current.dirty
        ? null
        : a,
    )
  return {
    alert,
    setAlert,
    sessionEnded: (revoked: boolean) =>
      setAlert({
        text: revoked ? t('session-revoked') : t('session-ended'),
        action: 'sign-in',
        at: at.current,
      }),
  }
}

/** The bar's element: the scroll padding follows the height the bar renders
 * at (DESIGN.md 4). The ref stays unused on a page with no bar; whether the
 * bar renders is the page's choice, not the bar's. */
export function useSaveBarPadding(): RefObject<HTMLDivElement | null> {
  const bar = useRef<HTMLDivElement>(null)
  useEffect(() => {
    const el = bar.current
    if (el === null || typeof ResizeObserver === 'undefined') return
    const root = document.documentElement.style
    const observer = new ResizeObserver(() =>
      root.setProperty('--save-bar-height', `${el.offsetHeight}px`),
    )
    observer.observe(el)
    return () => {
      observer.disconnect()
      root.removeProperty('--save-bar-height')
    }
  }, [])
  return bar
}

/** The bar, in the bottom strip of the form. `reloadId`, `signInId`,
 * `onReload` and `onSignIn` are the page's own answer to a `reload` or
 * `sign-in` alert, by id so the banner of the page behind never answers
 * first; `saveId` names the Save button. Discard and Save are never
 * `disabled` while on screen, so focus never drops out from under the
 * keyboard: a press with nothing to do does nothing (the caller guards it).
 * `saveDisabled` is a write of the page's own that a save has to wait for: it
 * holds Save switched off beside the bar's own rules, and the caller's save
 * refuses too.
 *
 * A profile page adds its Delete (22 D4.10): `deleteId`, `deleteLabel` and
 * `onDelete` put the verb first in the bar's group, before Discard and Save,
 * `deleteDisabled` keeps it there switched off, and `deleteHint` is the reason
 * it cannot act: while the verb is switched off the bar shows that reason as a
 * line under its buttons, with the id `<deleteId>-reason`, and names it with
 * the verb's description (22 D4.14). A verb switched off with no `deleteHint`
 * is held for a reason that is no reason to delete nothing: the bar shows no
 * line and names nothing. The page owns what the verb says and does, the bar
 * only places it. `children` is what the page puts under the
 * bar's buttons (22 D4.11): the delete's confirm, and the paragraph a refused
 * delete leaves. */
export function SaveBar({
  bar,
  status,
  alert,
  dirty,
  saving,
  reloadId,
  onReload,
  signInId,
  onSignIn,
  saveId,
  saveDisabled,
  onDiscard,
  onSave,
  deleteId,
  deleteLabel,
  onDelete,
  deleteDisabled,
  deleteExpanded,
  deleteHint,
  children,
}: {
  bar: RefObject<HTMLDivElement | null>
  status: SaveStatus
  alert: SaveAlert | null
  dirty: boolean
  saving: boolean
  reloadId?: string
  onReload?: () => void
  signInId?: string
  onSignIn?: () => void
  saveId: string
  /** A write of the page's own that a save has to wait for (24 A2 review D1). */
  saveDisabled?: boolean
  onDiscard: () => void
  onSave: () => void
  deleteId?: string
  deleteLabel?: string
  onDelete?: () => void
  deleteDisabled?: boolean
  deleteExpanded?: boolean
  /** The reason it cannot act, the line the bar shows under its buttons. */
  deleteHint?: string
  children?: ReactNode
}) {
  const disabled = !dirty || saving || undefined
  // Save waits for a write of the page's own too (24 A2 review D1); Discard is
  // local, so it is not held.
  const saveHeld = !dirty || saving || saveDisabled || undefined
  return (
    <div className="save-bar" ref={bar}>
      <div className="save-messages">
        <p
          role="status"
          className={
            status === 'saved' || status === 'saved-restart'
              ? `save-status ${status}`
              : 'save-status'
          }
        >
          {statusLine(status)}
        </p>
        {alert !== null && (
          <p role="alert" className="save-alert">
            {alert.text}
          </p>
        )}
        {alert?.action === 'reload' && onReload && (
          <button
            id={reloadId}
            type="button"
            className="secondary"
            onClick={onReload}
          >
            {t('save-reload')}
          </button>
        )}
        {alert?.action === 'sign-in' && onSignIn && (
          <button
            id={signInId}
            type="button"
            className="secondary"
            onClick={onSignIn}
          >
            {t('session-sign-in')}
          </button>
        )}
      </div>
      <div className="save-actions">
        {onDelete === undefined ? null : (
          <button
            id={deleteId}
            type="button"
            className="secondary"
            onClick={onDelete}
            aria-disabled={deleteDisabled ? 'true' : undefined}
            aria-expanded={deleteExpanded ? 'true' : 'false'}
            aria-describedby={
              deleteDisabled && deleteHint !== undefined
                ? `${deleteId}-reason`
                : undefined
            }
          >
            {deleteLabel}
          </button>
        )}
        <button
          type="button"
          className="secondary"
          aria-disabled={disabled}
          onClick={onDiscard}
        >
          {t('save-discard')}
        </button>
        <button
          id={saveId}
          type="button"
          className="primary"
          aria-disabled={saveHeld}
          onClick={onSave}
        >
          {saving ? t('save-saving') : t('save-button')}
        </button>
      </div>
      {/* The reason the Delete cannot act (22 D4.14): a line of the bar, after
          its buttons where the confirm opens, shown all the time while the
          verb is switched off, and named by its description. No tip: the words
          are seen without pointing, so the page has no hover state. */}
      {onDelete === undefined ||
      deleteDisabled !== true ||
      deleteHint === undefined ? null : (
        <p id={`${deleteId}-reason`}>{deleteHint}</p>
      )}
      {children}
    </div>
  )
}
