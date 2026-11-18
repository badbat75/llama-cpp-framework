// A card of a form (fe/DESIGN.md, section 5.2): a section with an h3 and an
// optional intro.
import type { ReactNode } from 'react'

export default function Card({
  title,
  intro,
  headingId,
  children,
}: {
  title: string
  intro?: string
  /** A page whose jump list reaches this card gives the heading its id: that
   * is the jump target, and the heading carries the tabindex -1 that keeps it
   * out of the Tab path. Absent on a card no jump list reaches: its heading
   * gets no id and no tabindex. */
  headingId?: string
  children: ReactNode
}) {
  return (
    <section className="card">
      <h3 id={headingId} tabIndex={headingId === undefined ? undefined : -1}>
        {title}
      </h3>
      {intro !== undefined && <p className="intro">{intro}</p>}
      {children}
    </section>
  )
}
