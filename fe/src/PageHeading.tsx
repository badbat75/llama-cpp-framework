// The h1 of every page: it names the page in the document title and takes the
// focus when the page changes (fe/DESIGN.md, section 2.3). Its id lets the app
// give the focus back to it when a dialog closes.
import { createContext, useContext, useEffect, useRef } from 'react'
import { t } from './strings.ts'

/** Raised by the app on every page change; 0 until the first one. */
export const FocusRequest = createContext(0)

export default function PageHeading({
  text,
  idLine,
}: {
  text: string
  /** The quiet id line right after the heading (16 17f.4), for a page that
   * names its subject by something other than its key: the instance page. */
  idLine?: string
}) {
  const request = useContext(FocusRequest)
  const ref = useRef<HTMLHeadingElement>(null)

  useEffect(() => {
    document.title = t('page-title', text)
  }, [text])

  useEffect(() => {
    if (request > 0) ref.current?.focus()
  }, [request])

  return (
    <>
      <h1 id="page-heading" ref={ref} tabIndex={-1}>
        {text}
      </h1>
      {idLine === undefined ? null : <p className="instance-id">{idLine}</p>}
    </>
  )
}
