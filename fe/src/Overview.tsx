// The Overview (#/): the framework version and the instances, signed in or not.
import Instances, { type InstancesStore } from './Instances.tsx'
import PageHeading from './PageHeading.tsx'
import { t } from './strings.ts'
import Version from './Version.tsx'

export default function Overview({
  store,
  signedIn,
}: {
  store: InstancesStore
  /** The session is known and current: the tiles read Start at boot
   *  (08-hearth-h6-overview.md, H6b.1). */
  signedIn: boolean
}) {
  return (
    <>
      <PageHeading text={t('overview-title')} />
      <Version />
      <Instances store={store} signedIn={signedIn} />
    </>
  )
}
