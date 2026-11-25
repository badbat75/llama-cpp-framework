// The h1 of every page: it names the page in the document title and takes the
// focus when the page changes (fe/DESIGN.md, section 2.3). Its id lets the app
// give the focus back to it when a dialog closes.
import {
  createContext,
  useContext,
  useEffect,
  useRef,
  type ReactNode,
} from 'react'
import { t } from './strings.ts'

/** Raised by the app on every page change; 0 until the first one. */
export const FocusRequest = createContext(0)

export default function PageHeading({
  text,
  actions,
}: {
  text: string
  /** The control right after the heading, in its title row (16 17f2.1), for a
   * page whose heading is the thing a control changes: the instance page's
   * display name pencil. A page with none keeps the bare heading. */
  actions?: ReactNode
}) {
  const request = useContext(FocusRequest)
  const ref = useRef<HTMLHeadingElement>(null)

  useEffect(() => {
    document.title = t('page-title', text)
  }, [text])

  useEffect(() => {
    if (request > 0) ref.current?.focus()
  }, [request])

  const heading = (
    <h1 id="page-heading" ref={ref} tabIndex={-1}>
      {text}
    </h1>
  )
  if (actions === undefined) return heading
  // The row wraps, so the control drops under the name when the name fills the
  // line (17f2.9). The control stays outside the heading: the heading's
  // accessible name is the name alone (17f2.1).
  return (
    <div className="instance-name-row">
      {heading}
      {actions}
    </div>
  )
}
