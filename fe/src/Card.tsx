// A card of a form (fe/DESIGN.md, section 5.2): a section with an h3 and an
// optional intro.
import type { ReactNode } from 'react'

export default function Card({
  title,
  intro,
  children,
}: {
  title: string
  intro?: string
  children: ReactNode
}) {
  return (
    <section className="card">
      <h3>{title}</h3>
      {intro !== undefined && <p className="intro">{intro}</p>}
      {children}
    </section>
  )
}
