// The Overview (#/): the framework version and the instances. It shows to a
// signed-in visitor only (09 S1a.1): signed out the page is the sign-in form.
import Instances, { type InstancesStore } from './Instances.tsx'
import PageHeading from './PageHeading.tsx'
import type { SignedOut } from './session.ts'
import { t } from './strings.ts'
import Version from './Version.tsx'

export default function Overview({
  store,
  host,
  onSignedOut,
}: {
  store: InstancesStore
  /** The host the shell read (fe/briefs/13-node-about.md, 13 N.5): it names
   * the instances section; null before the read answers and after a failure. */
  host: string | null
  /** Raised when one of the page's reads answers 401 (09 S1b.1). */
  onSignedOut: (reason: SignedOut) => void
}) {
  return (
    <>
      <PageHeading text={t('overview-title')} />
      <Version onSignedOut={onSignedOut} />
      <Instances store={store} host={host} onSignedOut={onSignedOut} />
    </>
  )
}
