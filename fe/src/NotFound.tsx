// A route the page does not know: say so and lead back to the Overview.
import PageHeading from './PageHeading.tsx'
import { t } from './strings.ts'

export default function NotFound() {
  return (
    <>
      <PageHeading text={t('not-found')} />
      <p>
        <a href="#/">{t('not-found-home')}</a>
      </p>
    </>
  )
}
