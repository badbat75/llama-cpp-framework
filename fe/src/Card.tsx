// A card of a form (fe/DESIGN.md, section 5.2): a section with an h3 and an
// optional intro.
import type { ReactNode } from 'react'

export default function Card({
  title,
  intro,
  headingId,
  headingExtra,
  children,
}: {
  title: string
  intro?: string
  /** A page whose jump list reaches this card gives the heading its id: that
   * is the jump target, and the heading carries the tabindex -1 that keeps it
   * out of the Tab path. Absent on a card no jump list reaches: its heading
   * gets no id and no tabindex. */
  headingId?: string
  /** A control beside the heading, in the row the heading stands in (22 D4.31
   * review A2, 2026-11-27: the card's About sits next to its title, the way a
   * field's About follows its label). The row is a box of its own, since a card
   * lays its children in a column: it is only there when there is something to
   * stand beside the heading, so a card without it keeps the heading as the
   * card's own first child. */
  headingExtra?: ReactNode
  children: ReactNode
}) {
  const heading = (
    <h3 id={headingId} tabIndex={headingId === undefined ? undefined : -1}>
      {title}
    </h3>
  )
  return (
    <section className="card">
      {headingExtra === undefined ? (
        heading
      ) : (
        <div className="card-heading">
          {heading}
          {headingExtra}
        </div>
      )}
      {intro !== undefined && <p className="intro">{intro}</p>}
      {children}
    </section>
  )
}
