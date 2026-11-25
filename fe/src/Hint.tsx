// The hint of a list row's − (brief 22, 22 D4.3, 22 D4.13): its reason, shown
// under it on hover and on focus, so a pointer user learns what to change
// (WCAG 1.4.13: the hint stays while the pointer is over it). The box that
// holds the control owns the pointer handlers and the tip is its next sibling
// (22 D4.13), in the flow under the control (22 D4.15), so the reason is never
// clipped by the table's scroll box and the pointer crosses no gap to reach it.
// The pages' Delete shows its reason as a line of the Save bar at all times
// (22 D4.14), so it has no tip and no hint state.
import { useState, type RefObject } from 'react'

/** The hint's state, read by the box's own handlers: `show` on hover and on
 *  focus, `leave` when the pointer goes away, `hide` on the pointer leaving the
 *  box, on blur and on Escape. The tip shows only while the control cannot act,
 *  so `cannotAct` gates `show`. `leave` takes the tip, which the box owns: the
 *  tip is the box's child, so the box keeps the reference. */
export function useHint(cannotAct: boolean): {
  shown: boolean
  show: () => void
  leave: (
    e: { relatedTarget: EventTarget | null },
    tip: RefObject<HTMLElement | null>,
  ) => void
  hide: () => void
} {
  const [shown, setShown] = useState(false)
  const show = () => {
    if (cannotAct) setShown(true)
  }
  /** The pointer leaving the box or the control: the tip stays when the pointer
   *  is on its way to it, and goes otherwise. A pointer that leaves the window
   *  names no target it came to: the tip stays, and Escape, the focus leaving,
   *  or the pointer crossing the tip's own edge closes it. */
  const leave = (
    e: { relatedTarget: EventTarget | null },
    tip: RefObject<HTMLElement | null>,
  ) => {
    const to = e.relatedTarget
    if (to instanceof Node && tip.current?.contains(to)) return
    if (to === null) return
    setShown(false)
  }
  return { shown, show, leave, hide: () => setShown(false) }
}

/** The tip: the control's reason, a line of the box under it. The control's
 *  description already carries these words, so the tip is hidden from the
 *  accessibility tree. The box owns the pointer handlers (22 D4.13), so the
 *  tip needs none. */
export function HintTip({
  tipRef,
  reason,
}: {
  tipRef: RefObject<HTMLSpanElement | null>
  reason: string
}) {
  return (
    <span ref={tipRef} className="hint-tip" aria-hidden="true">
      {reason}
    </span>
  )
}
