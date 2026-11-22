// The host and what is installed on it, read from the bff's GET /api/node
// (fe/briefs/13-node-about.md, section 0, and fe/briefs/18-about-node-info.md,
// section 0): the Node info page reads it at every arrival. The reply answers
// 200 with `os`, `vulkan` and `gpus` null when the manager did not answer, so
// every one of them is read as a fact the reply may not carry.
import { call, isRecord, sessionEnded, sessionRevoked } from './api.ts'

/** One GPU backend package: its suffix, and its version, null when unknown. */
export type NodeBackend = { name: string; version: string | null }

/** One installed engine: its id, its package version, its backend packages. */
export type NodeEngine = {
  engine: string
  version: string | null
  backends: NodeBackend[]
}

/** The operating system: each part null when the reply does not carry it. */
export type NodeOs = {
  name: string | null
  version: string | null
  kernel: string | null
}

/** What Vulkan reports for one device: both null below Vulkan 1.2 without
 * `VK_KHR_driver_properties`. */
export type NodeGpuVulkan = { driver: string | null; info: string | null }

/** One GPU, as the engine's probe saw it, in bus order. A null `version` means
 * the driver sets no module version, so its version is the kernel's; a null
 * `driver` means nothing is bound to the device; a null `vulkan` with the
 * reply's `vulkan` "ok" means Vulkan did not see this device. */
export type NodeGpu = {
  pci: string | null
  vendor: string | null
  name: string | null
  driver: string | null
  version: string | null
  vulkan: NodeGpuVulkan | null
}

/** What the node's Vulkan device list answered. */
export type NodeVulkan = 'ok' | 'none' | 'failed'

/** The host: its name, the framework's version, the installed engines, the
 * operating system, Vulkan and the GPUs. */
export type NodeInfo = {
  name: string
  version: string | null
  engines: NodeEngine[]
  os: NodeOs | null
  vulkan: NodeVulkan | null
  gpus: NodeGpu[] | null
  /** The bff's own answer about the manager: the key of its `nodeInfo`, null
   * when the manager answered. */
  nodeInfoKey: string | null
}

/** A version as the reply sends it: the string, null for anything else (the
 * bff sends null for an empty version file). */
function versionIn(value: unknown): string | null {
  return typeof value === 'string' && value !== '' ? value : null
}

/** Text as the reply sends it: the string, null when it is not one or holds
 * nothing but spaces (18 AN.3, AN.7, AN.15). */
export function textIn(value: unknown): string | null {
  return typeof value === 'string' && value.trim() !== '' ? value : null
}

/** The operating system of the reply: an object of the three parts, each null
 * when the reply does not carry it; null when it is not an object (an older
 * bff, or the manager down). The version reply carries the same block
 * (fe/briefs/18-about-node-info.md, section 0), so it is read the same way. */
export function osIn(value: unknown): NodeOs | null {
  if (!isRecord(value)) return null
  return {
    name: textIn(value.name),
    version: textIn(value.version),
    kernel: textIn(value.kernel),
  }
}

/** What Vulkan answered: one of the three words, null for anything else. */
function vulkanIn(value: unknown): NodeVulkan | null {
  return value === 'ok' || value === 'none' || value === 'failed' ? value : null
}

/** What Vulkan reports for one device: the object of the two parts, null when
 * it is not an object (the probe has nothing for this device). */
function gpuVulkanIn(value: unknown): NodeGpuVulkan | null {
  if (!isRecord(value)) return null
  return { driver: textIn(value.driver), info: textIn(value.info) }
}

/** One GPU row: every field as text, null when the reply does not carry it. */
function gpuIn(value: unknown): NodeGpu {
  const item = value as Record<string, unknown>
  return {
    pci: textIn(item.pci),
    vendor: textIn(item.vendor),
    name: textIn(item.name),
    driver: textIn(item.driver),
    version: textIn(item.version),
    vulkan: gpuVulkanIn(item.vulkan),
  }
}

/** The GPU rows, in the reply's order, the items that are not objects left
 * out; null when the reply has no GPU list (18 AN.7: the page says the
 * manager did not answer). */
function gpusIn(value: unknown): NodeGpu[] | null {
  if (!Array.isArray(value)) return null
  const gpus: NodeGpu[] = []
  for (const item of value) {
    if (!isRecord(item)) continue
    gpus.push(gpuIn(item))
  }
  return gpus
}

/** The key of the bff's `nodeInfo`, the words for why the manager's facts are
 * missing; null when the reply carries none (18 AN.7). */
function nodeInfoKeyIn(value: unknown): string | null {
  if (!isRecord(value)) return null
  return textIn(value.key)
}

/** The backend items with a name, in the reply's order; the rest are left out
 * (13 N.10). A missing list is the empty one: a CPU-only engine. */
function backendsIn(value: unknown): NodeBackend[] {
  if (!Array.isArray(value)) return []
  const backends: NodeBackend[] = []
  for (const item of value) {
    if (!isRecord(item)) continue
    const name = item.name
    if (typeof name !== 'string' || name === '') continue
    backends.push({ name, version: versionIn(item.version) })
  }
  return backends
}

/** The engine items with a name, in the reply's order; the rest are left out.
 * null when the reply has no engine list at all: a malformed reply (13 N.10). */
function enginesIn(value: unknown): NodeEngine[] | null {
  if (!Array.isArray(value)) return null
  const engines: NodeEngine[] = []
  for (const item of value) {
    if (!isRecord(item)) continue
    const engine = item.engine
    if (typeof engine !== 'string' || engine === '') continue
    engines.push({
      engine,
      version: versionIn(item.version),
      backends: backendsIn(item.backends),
    })
  }
  return engines
}

/**
 * The host, from the reply: `node` null when the reply says nothing to show
 * (an error, a body that is not an object, a name that is not a non-empty
 * string, engines that is not an array), `ended` when it answered 401, the
 * session being gone, or 403, the manager refusing its user, `revoked` of the
 * second (the page says which: fe/briefs/09-signin-only.md, S1b.1). The page
 * decides what either means: the shell's read shows the fallback words and
 * signs nobody out (13 N.5).
 */
export async function readNode(): Promise<{
  node: NodeInfo | null
  ended: boolean
  revoked: boolean
}> {
  try {
    const reply = await call('/api/node')
    const revoked = sessionRevoked(reply)
    if (sessionEnded(reply) || revoked)
      return { node: null, ended: true, revoked }
    if (!reply.ok || !isRecord(reply.body))
      return { node: null, ended: false, revoked: false }
    const name = reply.body.name
    const engines = enginesIn(reply.body.engines)
    // A name with nothing in it, spaces included, is no name: the shell keeps
    // its fallback words (13 N.5).
    if (typeof name !== 'string' || name.trim() === '' || engines === null)
      return { node: null, ended: false, revoked: false }
    return {
      node: {
        name,
        version: versionIn(reply.body.version),
        engines,
        os: osIn(reply.body.os),
        vulkan: vulkanIn(reply.body.vulkan),
        gpus: gpusIn(reply.body.gpus),
        nodeInfoKey: nodeInfoKeyIn(reply.body.nodeInfo),
      },
      ended: false,
      revoked: false,
    }
  } catch {
    return { node: null, ended: false, revoked: false }
  }
}
