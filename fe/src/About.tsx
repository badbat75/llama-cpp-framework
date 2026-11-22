// The About page (#/about, fe/briefs/18-about-node-info.md, 18 AN.2 to AN.4):
// the two facts that stayed, the framework's version and the operating system
// with its version, read from GET /api/version at every arrival, since an
// upgrade does not restart the web service. The engines and the GPUs are the
// Node info page's now (NodePage.tsx).
import { useEffect, useState } from 'react'
import PageHeading from './PageHeading.tsx'
import { Lines, Loading } from './Skeleton.tsx'
import type { SignedOut } from './session.ts'
import { t } from './strings.ts'
import { joined, Value } from './Value.tsx'
import { readVersionInfo, type VersionInfo } from './version.ts'

export default function About({
  onSignedOut,
}: {
  /** Raised when the read answers 401, the session being gone, or 403, the
   * manager refusing its user (09 S1b.1). */
  onSignedOut: (reason: SignedOut) => void
}) {
  // undefined while the read is open, null when it has nothing to show.
  const [info, setInfo] = useState<VersionInfo | null | undefined>(undefined)

  useEffect(() => {
    let live = true
    readVersionInfo().then((r) => {
      if (!live) return
      if (r.ended) {
        onSignedOut(r.revoked ? 'revoked' : 'ended')
        return
      }
      setInfo(r.info)
    })
    return () => {
      live = false
    }
  }, [onSignedOut])

  return (
    <>
      <PageHeading text={t('about-title')} />
      {info === undefined ? (
        // The two lines' skeleton (17a.4, 18 AN.2): the rows to come.
        <Loading text={t('about-loading')} shape={<Lines n={2} />} />
      ) : info === null ? (
        // Nobody acted to bring the failure about, so it is no alert (13 N.10).
        <p>{t('about-failed')}</p>
      ) : (
        <dl className="assets">
          <dt>{t('app-name')}</dt>
          <Value value={info.version} />
          <dt>{t('system-os')}</dt>
          <Value
            value={joined([
              info.os === null ? null : info.os.name,
              info.os === null ? null : info.os.version,
            ])}
          />
        </dl>
      )}
    </>
  )
}
