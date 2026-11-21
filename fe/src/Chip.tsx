// The tags as chips: the list of brief 15 section 10 and fe/DESIGN.md 5.2
// "Chip", shared by the instance page's tags field and, since brief 17 slice
// U17b, by the Overview's tiles. The rules of that section live here: R3 (the
// text of a chip is a span of its own, cut with an ellipsis, the 15em wrap its
// last resort), R5 (the whole label shows as a pill over a chip whose text was
// cut, while the pointer or the keyboard is on it) and R7 (the pill is a child
// of the chip, so the sheet's child selector outranks `.chip > span`). A chip
// with a control of its own, its x, is written by the page that edits the tags;
// a fixed chip carries nothing but its text.
import { useEffect, useRef, useState, type ReactNode } from 'react'
import { flushSync } from 'react-dom'

/**
 * The list of chips. `role="list"`: WebKit drops the list semantics of a ul
 * whose `list-style` is none, on purpose, and advises the explicit role (the
 * follow-up of H6, 08-hearth-h6-overview.md section 4): the tags are a list the
 * visitor counts.
 */
export function ChipList({
  className = '',
  labelledBy,
  children,
}: {
  /** The chips' own class, when the list carries one beside `chips` (the
   *  profile's row, 15 section 10 R4). */
  className?: string
  /** The id of the text that names this list (15 C1.2). */
  labelledBy?: string
  children: ReactNode
}) {
  return (
    <ul
      className={className === '' ? 'chips' : `chips ${className}`}
      role="list"
      {...(labelledBy === undefined ? {} : { 'aria-labelledby': labelledBy })}
    >
      {children}
    </ul>
  )
}

/**
 * The whole label of a chip whose text the ellipsis cut (15 section 10 R5): the
 * pill over the chip, opened while the pointer or the keyboard is on it, closed
 * when both have left, and by Escape. It draws the whole label, so the list
 * item that carries it is not read twice.
 */
function ChipFull({ tag }: { tag: string }) {
  return (
    <span className="chip-full" aria-hidden="true">
      {tag}
    </span>
  )
}

/**
 * One tag as a chip: its text in a span of its own (R3), and the pill of the
 * whole label while the pointer or the keyboard is on a chip whose text the
 * ellipsis cut (R5). `children` is the chip's control, the x of an editable
 * chip (15 C1.2); a fixed chip has none.
 */
export function Chip({
  tag,
  focusable,
  children,
}: {
  tag: string
  /** The keyboard reaches the text through the chip itself, so the chip takes a
   *  tab stop: the profile's chips, which carry no control (15 C1.15). A tile's
   *  chip takes none: the tile's id link leads to the page that shows every tag
   *  whole (17b.1). */
  focusable?: boolean
  children?: ReactNode
}) {
  // Only a chip whose text the ellipsis cut opens the pill, and the cut is read
  // at the moment the pointer or the focus enters, from the span's box (R5).
  const text = useRef<HTMLSpanElement | null>(null)
  const [expanded, setExpanded] = useState(false)
  // Both flush: the pointer and the keyboard reach a chip outside React's own
  // events (`focus()` of a test, a real click), and R5 asks the pill to be there
  // the moment the focus is on the chip, and gone the moment it leaves.
  const show = () => {
    const box = text.current
    if (box !== null && box.scrollWidth > box.clientWidth)
      flushSync(() => setExpanded(true))
  }
  const hide = () => flushSync(() => setExpanded(false))
  // Escape closes the pill wherever the focus is, and leaves the focus where it
  // stands (R5, WCAG 2.1.1: nothing is reachable only by the pointer), so the
  // listener is the document's while the pill shows.
  useEffect(() => {
    if (!expanded) return
    const away = (e: KeyboardEvent) => {
      if (e.key !== 'Escape') return
      setExpanded(false)
    }
    document.addEventListener('keydown', away)
    return () => document.removeEventListener('keydown', away)
  }, [expanded])

  return (
    <li
      className="chip"
      tabIndex={focusable === undefined ? undefined : 0}
      onMouseEnter={show}
      onMouseLeave={hide}
      onFocus={show}
      onBlur={hide}
    >
      <span ref={text}>{tag}</span>
      {expanded && <ChipFull tag={tag} />}
      {children}
    </li>
  )
}
