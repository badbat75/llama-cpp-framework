// A route the page does not know: say so and lead back to the Overview.
import { useEffect } from 'react'
import PageHeading from './PageHeading.tsx'
import { t } from './strings.ts'

export default function NotFound() {
  // The arrival focus (fe/DESIGN.md 2.3, 16 17a.1): the address the app opens
  // at is an arrival, so this page gives its h1 the focus (the shell raises its
  // focus request only on a navigation).
  useEffect(() => {
    document.getElementById('page-heading')?.focus()
  }, [])
  return (
    <>
      <PageHeading text={t('not-found')} />
      <p>
        <a href="#/">{t('not-found-home')}</a>
      </p>
    </>
  )
}
