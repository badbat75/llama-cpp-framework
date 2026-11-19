// The Overview (#/): the framework version and the instances. It shows to a
// signed-in visitor only (09 S1a.1): signed out the page is the sign-in form.
import Instances, { type InstancesStore } from './Instances.tsx'
import PageHeading from './PageHeading.tsx'
import type { SignedOut } from './session.ts'
import { t } from './strings.ts'
import Version from './Version.tsx'

export default function Overview({
  store,
  onSignedOut,
}: {
  store: InstancesStore
  /** Raised when one of the page's reads answers 401 (09 S1b.1). */
  onSignedOut: (reason: SignedOut) => void
}) {
  return (
    <>
      <PageHeading text={t('overview-title')} />
      <Version onSignedOut={onSignedOut} />
      <Instances store={store} onSignedOut={onSignedOut} />
    </>
  )
}
