// The Overview (#/): the framework version and the instances. It shows to a
// signed-in visitor only (09 S1a.1): signed out the page is the sign-in form.
import Instances, { type InstancesStore } from './Instances.tsx'
import PageHeading from './PageHeading.tsx'
import { t } from './strings.ts'
import Version from './Version.tsx'

export default function Overview({ store }: { store: InstancesStore }) {
  return (
    <>
      <PageHeading text={t('overview-title')} />
      <Version />
      <Instances store={store} />
    </>
  )
}
