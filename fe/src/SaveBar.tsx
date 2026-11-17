// The bottom save bar of a form page: the status line, the alert and its
// answer button, Discard and Save, and the scroll padding that follows the
// bar's rendered height. ServerForm (the Server page) and the preset page
// share it, so a save answers the same way on both.
// The text is in en_US.json.
import { useEffect, useRef, useState, type RefObject } from 'react'
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
 * keyboard: a press with nothing to do does nothing (the caller guards it). */
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
  onDiscard,
  onSave,
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
  onDiscard: () => void
  onSave: () => void
}) {
  const disabled = !dirty || saving || undefined
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
          aria-disabled={disabled}
          onClick={onSave}
        >
          {saving ? t('save-saving') : t('save-button')}
        </button>
      </div>
    </div>
  )
}
