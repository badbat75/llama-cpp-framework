// The Dialog of fe/DESIGN.md, section 5.2: a native <dialog> opened with
// showModal() while it is rendered, named by its h2. Escape closes it, which
// is the choice that changes nothing; the opener puts the focus back.
import { useEffect, useId, useRef, type ReactNode } from 'react'
import { t } from './strings.ts'

export default function Dialog({
  title,
  onClose,
  children,
}: {
  title: string
  /** Escape, or the dialog closed by the browser. */
  onClose: () => void
  children: ReactNode
}) {
  const id = useId()
  const ref = useRef<HTMLDialogElement>(null)

  useEffect(() => {
    const dialog = ref.current
    if (dialog === null || dialog.open) return
    dialog.showModal()
    dialog.querySelector<HTMLElement>('button, input')?.focus()
  }, [])

  return (
    <dialog
      ref={ref}
      className="dialog"
      aria-labelledby={`${id}-title`}
      onClose={onClose}
    >
      <h2 id={`${id}-title`}>{title}</h2>
      {children}
    </dialog>
  )
}

/** Asks before unsaved changes are lost; Keep editing comes first. */
export function DiscardDialog({
  onKeep,
  onDiscard,
}: {
  onKeep: () => void
  onDiscard: () => void
}) {
  return (
    <Dialog title={t('discard-title')} onClose={onKeep}>
      <p>{t('discard-description')}</p>
      <div className="dialog-actions">
        <button type="button" className="primary" onClick={onKeep}>
          {t('discard-keep')}
        </button>
        <button type="button" className="secondary" onClick={onDiscard}>
          {t('discard-confirm')}
        </button>
      </div>
    </Dialog>
  )
}
