// The About page (#/about, fe/briefs/13-node-about.md, 13 N.7 to N.10): every
// version installed on this host, read again at every arrival, since an engine
// upgrade does not restart the web service.
import { Fragment, useEffect, useState } from 'react'
import { readNode, type NodeInfo } from './node.ts'
import PageHeading from './PageHeading.tsx'
import type { SignedOut } from './session.ts'
import { t } from './strings.ts'

/** A backend package's display name, by its suffix (13 N.8). A suffix the
 * strings do not know is shown as it was sent: it is data. */
function backendName(suffix: string): string {
  switch (suffix) {
    case 'cuda':
      return t('backend-cuda')
    case 'rocm':
      return t('backend-rocm')
    case 'vulkan':
      return t('backend-vulkan')
    default:
      return suffix
  }
}

/** One version as a row's value: shown as sent, never parsed, in monospace;
 * the unknown one is the muted word (13 N.9), as the Value list's "(none)". */
function VersionValue({ version }: { version: string | null }) {
  return version === null ? (
    <dd className="muted">{t('about-version-unknown')}</dd>
  ) : (
    <dd className="mono">{version}</dd>
  )
}

/** One installed engine: its heading, its version, its backend packages. */
function Engine({
  engine,
  version,
  backends,
}: {
  engine: string
  version: string | null
  backends: NodeInfo['engines'][number]['backends']
}) {
  return (
    // Each engine's rows are siblings of its heading, so the terms and values
    // stay the direct children of the list (H40).
    <Fragment>
      <h3>{engine}</h3>
      <dl className="assets">
        <dt>{t('about-version')}</dt>
        <VersionValue version={version} />
        {backends.length === 0 && (
          <Fragment>
            <dt>{t('about-backends')}</dt>
            <dd className="muted">{t('about-backends-none')}</dd>
          </Fragment>
        )}
        {backends.map((backend, i) => (
          <Fragment key={i}>
            <dt>{backendName(backend.name)}</dt>
            <VersionValue version={backend.version} />
          </Fragment>
        ))}
      </dl>
    </Fragment>
  )
}

export default function About({
  onSignedOut,
}: {
  /** Raised when the read answers 401, the session being gone (09 S1b.1). */
  onSignedOut: (reason: SignedOut) => void
}) {
  // undefined while the read is open, null when it has nothing to show.
  const [node, setNode] = useState<NodeInfo | null | undefined>(undefined)

  useEffect(() => {
    let live = true
    readNode().then((r) => {
      if (!live) return
      if (r.ended) {
        onSignedOut('ended')
        return
      }
      setNode(r.node)
    })
    return () => {
      live = false
    }
  }, [onSignedOut])

  return (
    <>
      <PageHeading text={t('about-title')} />
      {node === undefined ? (
        <p role="status">{t('about-loading')}</p>
      ) : node === null ? (
        // Nobody acted to bring the failure about, so it is no alert (13 N.10).
        <p>{t('about-failed')}</p>
      ) : (
        <>
          <h2>{node.name}</h2>
          <dl className="assets">
            <dt>{t('app-name')}</dt>
            <VersionValue version={node.version} />
          </dl>
          {node.engines.length === 0 && <p>{t('about-no-engine')}</p>}
          {node.engines.map((engine, i) => (
            <Engine key={i} {...engine} />
          ))}
        </>
      )}
    </>
  )
}
