// The loading state of a page's first read (fe/briefs/17-ux-pass.md, U17a;
// fe/DESIGN.md 5.2 Skeleton and its section 6 Loading): the read's text in the
// DOM and out of sight from the first frame, and 300 ms in, the shape of the
// page to come. The shape is under an `aria-hidden="true"` element, so it
// carries no text to a screen reader; its blocks are spans, so they hold no
// tab stop; the sheet gives them no motion.
import { useEffect, useState, type ReactNode } from 'react'

/** How long a read has to stay open before the shape shows: an answer faster
 * than this changes nothing on the screen (17a.3). */
const SKELETON_MS = 300

/** The read's text, in the DOM and not shown: a screen reader is told it when
 * the read starts, and it goes with the answer (17a.2, 17a.5). */
export function LoadingText({ text }: { text: string }) {
  return (
    <p className="visually-hidden" role="status">
      {text}
    </p>
  )
}

/** One line of the shape, as high as a line of text. */
export function Line() {
  return <span aria-hidden="true" className="skeleton" />
}

/** `n` lines, one under the other: the rows of a list, the lines of a header. */
export function Lines({ n }: { n: number }) {
  return Array.from({ length: n }, (_, i) => <Line key={i} />)
}

/** The shape of a block of the page, a card's body, a form. */
export function Box() {
  return <span aria-hidden="true" className="skeleton-box" />
}

/** The card that stands where a card of the form will. */
export function CardBox() {
  return (
    <section className="card">
      <Box />
    </section>
  )
}

/**
 * The loading state of a read that is open: `text` at once, `shape` 300 ms in.
 * A page renders this for as long as its read is open, so the text and the
 * shape are gone in the one render the answer brings (17a.5).
 */
export function Loading({ text, shape }: { text: string; shape: ReactNode }) {
  const [slow, setSlow] = useState(false)
  useEffect(() => {
    const timer = setTimeout(() => setSlow(true), SKELETON_MS)
    return () => {
      clearTimeout(timer)
    }
  }, [])
  return (
    <>
      <LoadingText text={text} />
      {slow && <div aria-hidden="true">{shape}</div>}
    </>
  )
}
