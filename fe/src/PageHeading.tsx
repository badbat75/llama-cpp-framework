// The h1 of every page: it names the page in the document title and takes the
// focus when the page changes (fe/DESIGN.md, section 2.3).
import { createContext, useContext, useEffect, useRef } from 'react'
import { t } from './strings.ts'

/** Raised by the app on every page change; 0 until the first one. */
export const FocusRequest = createContext(0)

export default function PageHeading({ text }: { text: string }) {
  const request = useContext(FocusRequest)
  const ref = useRef<HTMLHeadingElement>(null)

  useEffect(() => {
    document.title = t('page-title', text)
  }, [text])

  useEffect(() => {
    if (request > 0) ref.current?.focus()
  }, [request])

  return (
    <h1 ref={ref} tabIndex={-1}>
      {text}
    </h1>
  )
}
