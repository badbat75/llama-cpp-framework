// The Node info page (#/node, fe/briefs/18-about-node-info.md, 18 AN.5, AN.8,
// AN.9, AN.10 to AN.14): what is installed on this host, read from GET
// /api/node at every arrival, since an engine upgrade does not restart the web
// service. The read asks the manager, which may spawn its Vulkan child, so the
// skeleton (17a.4) is seen.
import { Fragment, useEffect, useState } from 'react'
import {
  readNode,
  type NodeBackend,
  type NodeGpu,
  type NodeGpuVulkan,
  type NodeInfo,
  type NodeOs,
  type NodeVulkan,
} from './node.ts'
import PageHeading from './PageHeading.tsx'
import { Lines, Loading } from './Skeleton.tsx'
import type { SignedOut } from './session.ts'
import { t } from './strings.ts'
import { joined, Value } from './Value.tsx'

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

/** One installed engine: its heading, its version, its backend packages. */
function Engine({
  engine,
  version,
  backends,
}: {
  engine: string
  version: string | null
  backends: NodeBackend[]
}) {
  return (
    // Each engine's rows are siblings of its heading, so the terms and values
    // stay the direct children of the list (H40).
    <Fragment>
      <h3>{engine}</h3>
      <dl className="assets">
        <dt>{t('about-version')}</dt>
        <Value value={version} />
        {backends.length === 0 && (
          <Fragment>
            <dt>{t('about-backends')}</dt>
            <dd className="muted">{t('about-backends-none')}</dd>
          </Fragment>
        )}
        {backends.map((backend, i) => (
          <Fragment key={i}>
            <dt>{backendName(backend.name)}</dt>
            <Value value={backend.version} />
          </Fragment>
        ))}
      </dl>
    </Fragment>
  )
}

/** The operating system value: its name and its version, in that order, joined
 * by one space, or the one that is there (18 decision 5). */
function osValue(os: NodeOs | null): string | null {
  return os === null ? null : joined([os.name, os.version])
}

/** The vendor of a GPU (18 AN.10, decision 6): the name the fe gives the id, in
 * the body font. An id it does not name is data, shown as sent in mono, and
 * null is the muted word. */
function Vendor({ vendor }: { vendor: string | null }) {
  switch (vendor) {
    case '1002':
      return <dd>{t('vendor-amd')}</dd>
    case '10de':
      return <dd>{t('vendor-nvidia')}</dd>
    case '8086':
      return <dd>{t('vendor-intel')}</dd>
    default:
      return <Value value={vendor} />
  }
}

/** The driver row (18 AN.11): the driver and its module version, joined. No
 * module version means the driver is part of the kernel, so its version is the
 * kernel's, said in its own words; no driver is the muted word. */
function Driver({
  driver,
  version,
}: {
  driver: string | null
  version: string | null
}) {
  if (driver === null) return <dd className="muted">{t('node-driver-none')}</dd>
  if (version === null)
    // The two parts are inline spans so the words sit in the same line of the
    // value, and the dd carries neither class: the name is data, the words are
    // not.
    return (
      <dd>
        <span className="mono">{driver}</span>{' '}
        <span className="muted">{t('node-driver-in-kernel')}</span>
      </dd>
    )
  return <dd className="mono">{joined([driver, version])}</dd>
}

/** The word for a device Vulkan has nothing for (18 AN.12), by what the node's
 * device list answered: a device the list did not reach, no Vulkan driver, and
 * the rest, the read including an answer that is none of the three words. */
function vulkanWord(vulkan: NodeVulkan | null): string {
  switch (vulkan) {
    case 'ok':
      return t('node-vulkan-not-seen')
    case 'none':
      return t('node-vulkan-none')
    default:
      return t('node-vulkan-failed')
  }
}

/** The Vulkan row (18 AN.12): what the device list reports for this GPU, the
 * driver and the info joined; a device it reports nothing for, and a device it
 * has no row for, are the muted words. */
function Vulkan({
  row,
  node,
}: {
  row: NodeGpuVulkan | null
  node: NodeVulkan | null
}) {
  if (row !== null) {
    const value = joined([row.driver, row.info])
    return value === null ? (
      <dd className="muted">{t('node-vulkan-unreported')}</dd>
    ) : (
      <dd className="mono">{value}</dd>
    )
  }
  return <dd className="muted">{vulkanWord(node)}</dd>
}

/** One GPU: its heading, and the four facts the page always shows, each a word
 * when the reply has none (18 AN.10 to AN.12). */
function Gpu({
  gpu,
  index,
  vulkan,
}: {
  gpu: NodeGpu
  /** The row's place in the reply, from 0. */
  index: number
  /** What the node's Vulkan device list answered. */
  vulkan: NodeVulkan | null
}) {
  // The heading is numbered from 1 in the reply's order, so two cards of the
  // same model read apart in a screen reader's heading list (WCAG 2.4.6).
  const number = String(index + 1)
  return (
    // The rows are siblings of the heading, as the engine's (H40).
    <Fragment>
      <h3>
        {gpu.name === null
          ? t('node-gpu-title-unnamed', number)
          : t('node-gpu-title', number, gpu.name)}
      </h3>
      <dl className="assets">
        <dt>{t('node-gpu-vendor')}</dt>
        <Vendor vendor={gpu.vendor} />
        <dt>{t('node-gpu-pci')}</dt>
        <Value value={gpu.pci} />
        <dt>{t('node-gpu-driver')}</dt>
        <Driver driver={gpu.driver} version={gpu.version} />
        <dt>{t('node-gpu-vulkan')}</dt>
        <Vulkan row={gpu.vulkan} node={vulkan} />
      </dl>
    </Fragment>
  )
}

/** Under the GPUs heading (18 AN.13): one block per row, the word for a node
 * with none, and the words for the manager's facts that did not arrive. A
 * paragraph, never an alert: nobody acted. */
function GpuSection({ node }: { node: NodeInfo }) {
  if (node.gpus === null)
    return (
      <p>
        {node.nodeInfoKey === 'client-unavailable'
          ? t('node-info-unavailable')
          : t('node-info-failed')}
      </p>
    )
  if (node.gpus.length === 0) return <p>{t('node-gpus-none')}</p>
  return (
    <>
      {node.gpus.map((gpu, i) => (
        <Gpu key={i} gpu={gpu} index={i} vulkan={node.vulkan} />
      ))}
    </>
  )
}

export default function NodePage({
  onSignedOut,
}: {
  /** Raised when the read answers 401, the session being gone, or 403, the
   * manager refusing its user (18 AN.9, 09 S1b.1). */
  onSignedOut: (reason: SignedOut) => void
}) {
  // undefined while the read is open, null when it has nothing to show.
  const [node, setNode] = useState<NodeInfo | null | undefined>(undefined)

  useEffect(() => {
    let live = true
    readNode().then((r) => {
      if (!live) return
      if (r.ended) {
        onSignedOut(r.revoked ? 'revoked' : 'ended')
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
      <PageHeading text={t('node-title')} />
      {node === undefined ? (
        // The page's skeleton (17a.4, 18 AN.9): six lines, the four rows of
        // the host list and the engine's to come.
        <Loading text={t('node-loading')} shape={<Lines n={6} />} />
      ) : node === null ? (
        // Nobody acted to bring the failure about, so it is no alert (13 N.10).
        <p>{t('node-failed')}</p>
      ) : (
        <>
          <dl className="assets">
            <dt>{t('node-host')}</dt>
            <Value value={node.name} />
            <dt>{t('system-os')}</dt>
            <Value value={osValue(node.os)} />
            <dt>{t('system-kernel')}</dt>
            <Value value={node.os === null ? null : node.os.kernel} />
            <dt>{t('app-name')}</dt>
            <Value value={node.version} />
          </dl>
          <h2>{t('node-engines')}</h2>
          {node.engines.length === 0 && <p>{t('about-no-engine')}</p>}
          {node.engines.map((engine, i) => (
            <Engine key={i} {...engine} />
          ))}
          <h2>{t('node-gpus')}</h2>
          <GpuSection node={node} />
        </>
      )}
    </>
  )
}
