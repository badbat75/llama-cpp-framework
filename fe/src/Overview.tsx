// The Overview (#/): the framework version and the instances, signed in or not.
import Instances from './Instances.tsx'
import PageHeading from './PageHeading.tsx'
import { t } from './strings.ts'
import Version from './Version.tsx'

export default function Overview() {
  return (
    <>
      <PageHeading text={t('overview-title')} />
      <Version />
      <Instances />
    </>
  )
}
