// The hint of a list row's − (brief 22, 22 D4.3, 22 D4.13, as the 22D4
// amendment's 22 D4.16 to 22 D4.20 amend it): its reason, shown on hover and on
// focus, so a pointer user learns what to change (WCAG 1.4.13: the hint stays
// while the pointer is over it). The box that holds the control owns the pointer
// handlers and the tip is its next sibling (22 D4.13), and the tip floats over
// the table (22 D4.16): `position: fixed`, placed against the viewport by a
// script (22 D4.17), so showing it changes no row's height and no column's
// width, and the table's scroll box neither clips it nor counts it. The pages'
// Delete shows its reason as a line of the Save bar at all times (22 D4.14), so
// it has no tip and no hint state.
import { useEffect, useLayoutEffect, useState, type RefObject } from 'react'

/** The room the tip keeps from the viewport's edges, and the gap it never
 *  leaves between itself and the − (22 D4.17). */
const GUTTER = 8

/** The hint's state, read by the box's own handlers: `show` on hover and on
 *  focus, `leave` when the pointer goes away, `hide` on the pointer leaving the
 *  box, on blur and on Escape. The tip shows only while the control cannot act,
 *  so `cannotAct` gates `show`. `leave` takes the tip, which the box owns: the
 *  tip is the box's child, so the box keeps the reference. `box` is the box
 *  itself: while the tip shows, a `pointerdown` outside it hides the tip (22
 *  D4.18, a touch has no hover to leave). */
export function useHint(
  cannotAct: boolean,
  box: RefObject<HTMLElement | null>,
): {
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
  // The listener is on the document, not on the box: a pointerdown on a node
  // outside the React tree never reaches the box's own handler, and the tip
  // floats over the whole page (22 D4.16). The capture phase, so it is read
  // before the control under the pointer acts. The keydown is the document's
  // for the same reason (22 D4.18 D3): a tip shown by a hover has no focused
  // control of its own, so Escape reaches it wherever the focus is, and
  // nothing moves the focus: the user stays where they were.
  useEffect(() => {
    if (!shown) return
    const onDown = (e: PointerEvent) => {
      const held = box.current
      if (held !== null && e.target instanceof Node && held.contains(e.target))
        return
      setShown(false)
    }
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') setShown(false)
    }
    document.addEventListener('pointerdown', onDown, true)
    document.addEventListener('keydown', onKey)
    return () => {
      document.removeEventListener('pointerdown', onDown, true)
      document.removeEventListener('keydown', onKey)
    }
  }, [shown, box])
  return { shown, show, leave, hide: () => setShown(false) }
}

/** The tip: the control's reason, a small box floating over the table under the
 *  − (22 D4.16). The control's description already carries these words, so the
 *  tip is hidden from the accessibility tree (22 D4.20). The box owns the
 *  pointer handlers (22 D4.13), so the tip needs none.
 *
 *  22 D4.17 places it: `top = r.bottom`, `left = r.left`, the −'s rect read from
 *  `anchorRef`, so the tip touches the − and the pointer crosses nothing to
 *  reach it (1.4.13). Near the right edge its own right edge goes to the −'s,
 *  near the bottom it opens above the −, and the gutter keeps it inside the
 *  viewport. A scroll anywhere, the table's own box included, and a resize place
 *  it again, so it never covers the − (2.4.11). The tip's size is its own, after
 *  the sheet has wrapped its text at `max-width`. */
export function HintTip({
  tipRef,
  anchorRef,
  reason,
}: {
  tipRef: RefObject<HTMLSpanElement | null>
  anchorRef: RefObject<HTMLElement | null>
  reason: string
}) {
  useLayoutEffect(() => {
    const place = () => {
      const anchor = anchorRef.current
      const tip = tipRef.current
      if (anchor === null || tip === null) return
      // The size is read at the origin (22 D4.17 D1): the tip wraps at the
      // width the place it is in leaves it, so a size read at the previous
      // place decides the flip from a box the tip no longer has.
      tip.style.top = '0px'
      tip.style.left = '0px'
      const r = anchor.getBoundingClientRect()
      const width = tip.offsetWidth
      const height = tip.offsetHeight
      let left = r.left
      if (left + width > window.innerWidth - GUTTER) left = r.right - width
      if (left < GUTTER) left = GUTTER
      let top = r.bottom
      if (top + height > window.innerHeight - GUTTER) top = r.top - height
      // A tip taller than the room above the −: the gutter keeps it in the
      // viewport (22 D4.17 D2).
      if (top < GUTTER) top = GUTTER
      tip.style.top = `${top}px`
      tip.style.left = `${left}px`
    }
    place()
    // The capture phase: `scroll` does not bubble, and the table's own scroll
    // box is a descendant of window.
    window.addEventListener('scroll', place, true)
    window.addEventListener('resize', place)
    return () => {
      window.removeEventListener('scroll', place, true)
      window.removeEventListener('resize', place)
    }
  }, [tipRef, anchorRef])
  return (
    <span ref={tipRef} className="hint-tip" aria-hidden="true">
      {reason}
    </span>
  )
}
