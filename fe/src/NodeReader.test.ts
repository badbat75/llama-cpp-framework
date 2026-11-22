// fe/src/NodeReader.test.ts
// Slice 18 AN5a (fe/briefs/18-about-node-info.md, 18 AN.7): the reader of
// GET /api/node. `readNode` keeps today's rules for `name` and `engines` and
// reads the three fields brief 18 adds, forgivingly: `os`, `vulkan`, `gpus`,
// and the `nodeInfo` key. "18 AN.7" is the brief's criterion 7.
import { afterEach, expect, test, vi } from 'vitest'
import { readNode } from './node.ts'
import {
  fakeBff,
  GPUS,
  NODE,
  OS,
  reply,
  type NodeEngine,
  type NodeExtra,
  type NodeGpu,
  type NodeInfo,
} from './test/bff.ts'

afterEach(() => vi.unstubAllGlobals())

/**
 * What AN.7 says `readNode` answers: today's `NodeInfo`, with the three fields
 * brief 18 adds and the `nodeInfo` key. The cast is the test's promise that the
 * reader declares these; it goes when `node.ts` does.
 */
type ReadNode = {
  node: (NodeInfo & NodeExtra & { nodeInfoKey: string | null }) | null
  ended: boolean
}

/** The reader, on a 200 with this body. */
async function readOf(body: unknown): Promise<ReadNode> {
  fakeBff({ node: () => reply(200, body) })
  return (await readNode()) as unknown as ReadNode
}

/** The reply of section 0, with these fields over it (an `undefined` drops). */
function bodyWith(over: Record<string, unknown>) {
  const base = { ...NODE } as Record<string, unknown>
  for (const [key, value] of Object.entries(over)) {
    if (value === undefined) delete base[key]
    else base[key] = value
  }
  return base
}

/** The reader on the reply of section 0 with one field replaced. */
async function readField(name: string, value: unknown): Promise<ReadNode> {
  return readOf(bodyWith({ [name]: value }))
}

// ── The reply of the brief, and today's reply ───────────────────────────────

test('18 AN.7: the reply of section 0 gives the name, the version, the engines, the os, vulkan ok, the three gpus in order, and no nodeInfoKey', async () => {
  const { node, ended } = await readOf({ ...NODE })
  expect(ended, 'a good reply ends nothing').toBe(false)
  expect(node, 'the node is there').not.toBeNull()
  expect(node!.name, 'today keeps the name as it is').toBe(NODE.name)
  expect(node!.version, 'today keeps the version').toBe(NODE.version)
  expect(node!.engines, 'today keeps the engines').toEqual(NODE.engines)
  expect(
    node!.os,
    'the operating system, as an object of three strings',
  ).toEqual(OS)
  expect(node!.vulkan, 'the Vulkan answer').toBe('ok')
  expect(node!.gpus, 'the GPUs, in the reply order').toEqual(GPUS)
  expect(
    node!.nodeInfoKey,
    'the bff answered the manager, so no key',
  ).toBeNull()
})

test('18 AN.7: a reply with only today’s four fields still gives the node, with os, vulkan and gpus null', async () => {
  const { node, ended } = await readOf(
    bodyWith({ os: undefined, vulkan: undefined, gpus: undefined }),
  )
  expect(ended, 'a good reply ends nothing').toBe(false)
  expect(node, 'the node is there').not.toBeNull()
  expect(node!.name).toBe(NODE.name)
  expect(node!.version).toBe(NODE.version)
  expect(node!.engines, 'the engines of today, unchanged').toEqual(NODE.engines)
  expect(node!.os, 'no operating system').toBeNull()
  expect(node!.vulkan, 'no Vulkan answer').toBeNull()
  expect(node!.gpus, 'no GPUs').toBeNull()
  expect(node!.nodeInfoKey, 'no key').toBeNull()
})

// ── AN.7: the operating system ──────────────────────────────────────────────

const BAD_OS: Array<[string, unknown]> = [
  ['a string', 'Ubuntu'],
  ['a number', 7],
  ['an array', ['Ubuntu']],
  ['true', true],
  ['null', null],
  ['left out', undefined],
]

test.each(BAD_OS)(
  '18 AN.7: an os that is %s reads null, the node kept',
  async (_what, os) => {
    const { node } = await readField('os', os)
    expect(node, 'the node is there').not.toBeNull()
    expect(node!.os, 'no operating system').toBeNull()
  },
)

test('18 AN.7: an os of a name that is a number, a version that is empty and a kernel of spaces, each reads null', async () => {
  const { node } = await readField('os', {
    name: 7,
    version: '',
    kernel: '   ',
  })
  expect(node!.os, 'the object, of three nulls').toEqual({
    name: null,
    version: null,
    kernel: null,
  })
})

test('18 AN.7: an os with extra fields keeps the three it knows', async () => {
  const { node } = await readField('os', {
    name: 'Ubuntu',
    version: '26.04',
    kernel: '7.0.0-43-generic',
    pretty: 'Ubuntu 26.04 LTS',
    release: 26.04,
  })
  expect(node!.os).toEqual(OS)
})

// ── AN.7: the Vulkan answer ─────────────────────────────────────────────────

const VULKANS: Array<[string, unknown, 'ok' | 'none' | 'failed' | null]> = [
  ['ok', 'ok', 'ok'],
  ['none', 'none', 'none'],
  ['failed', 'failed', 'failed'],
  ['OK', 'OK', null],
  ['the empty string', '', null],
  ['a string with spaces', '   ', null],
  ['a number', 7, null],
  ['true', true, null],
  ['null', null, null],
  ['left out', undefined, null],
]

test.each(VULKANS)('18 AN.7: a vulkan of %s', async (what, vulkan, read) => {
  const { node } = await readField('vulkan', vulkan)
  expect(node, 'the node is there').not.toBeNull()
  expect(node!.vulkan, `the Vulkan answer of ${what} reads ${read}`).toBe(read)
})

// ── AN.7: the GPUs ──────────────────────────────────────────────────────────

const BAD_GPUS: Array<[string, unknown]> = [
  ['a string', 'no'],
  ['an object', { 0: { pci: 'x' } }],
  ['a number', 7],
  ['true', true],
  ['null', null],
  ['left out', undefined],
]

test.each(BAD_GPUS)(
  '18 AN.7: gpus that is %s reads null, the node kept',
  async (_what, gpus) => {
    const { node } = await readField('gpus', gpus)
    expect(node, 'the node is there').not.toBeNull()
    expect(node!.gpus, 'no GPU list').toBeNull()
  },
)

test('18 AN.7: gpus [] reads the empty list, the node kept', async () => {
  const { node } = await readField('gpus', [])
  expect(node!.gpus, 'no GPU, and the page says so (AN.13)').toEqual([])
})

test('18 AN.7: an item that is not an object is left out, the ones that are stay in order', async () => {
  const good: NodeGpu = GPUS[0]!
  const { node } = await readField('gpus', [
    null,
    7,
    'a GPU',
    ['pci'],
    good,
    undefined,
    GPUS[2]!,
  ])
  expect(node!.gpus, 'the two items that are objects, in order').toEqual([
    good,
    GPUS[2]!,
  ])
})

test('18 AN.7: a gpu row of fields that are not strings, an empty string, or left out, reads each field null', async () => {
  const { node } = await readField('gpus', [
    {
      pci: 7,
      vendor: '',
      name: '   ',
      driver: true,
      version: [],
      vulkan: 'radv',
    },
  ])
  expect(node!.gpus, 'one row, every field null').toEqual([
    {
      pci: null,
      vendor: null,
      name: null,
      driver: null,
      version: null,
      vulkan: null,
    },
  ])
})

test('18 AN.7: a gpu row leaving every field out reads a row of nulls', async () => {
  const { node } = await readField('gpus', [{}])
  expect(node!.gpus).toEqual([
    {
      pci: null,
      vendor: null,
      name: null,
      driver: null,
      version: null,
      vulkan: null,
    },
  ])
})

test('18 AN.7: a row vulkan of driver radv and info null keeps the driver, one of both null is the object, one that is not an object is null', async () => {
  const one = (vulkan: unknown): Promise<NodeGpu | undefined> =>
    readField('gpus', [{ pci: '0000:03:00.0', vulkan }]).then(
      (out) => out.node?.gpus?.[0],
    )
  expect(await one({ driver: 'radv', info: null })).toEqual({
    pci: '0000:03:00.0',
    vendor: null,
    name: null,
    driver: null,
    version: null,
    vulkan: { driver: 'radv', info: null },
  })
  expect(await one({ driver: '', info: 7 })).toEqual({
    pci: '0000:03:00.0',
    vendor: null,
    name: null,
    driver: null,
    version: null,
    vulkan: { driver: null, info: null },
  })
  for (const bad of ['radv', 7, [], null, undefined])
    expect(
      (await one(bad))?.vulkan,
      `a vulkan of ${JSON.stringify(bad)}: null`,
    ).toBeNull()
})

test('18 AN.7: the whole reply of section 7, sent as the bff sends it, reads the three rows field by field', async () => {
  const { node } = await readOf(bodyWith({ gpus: GPUS }))
  expect(node!.gpus).toEqual([
    {
      pci: '0000:03:00.0',
      vendor: '1002',
      name: 'AMD Radeon AI PRO R9700',
      driver: 'amdgpu',
      version: '7.1.9.31600000',
      vulkan: { driver: 'radv', info: 'Mesa 25.2.0' },
    },
    {
      pci: '0000:0a:00.0',
      vendor: '10de',
      name: 'NVIDIA GeForce RTX 5090',
      driver: 'nvidia',
      version: '580.95.05',
      vulkan: { driver: 'NVIDIA', info: '580.95.05' },
    },
    {
      pci: '0000:0f:00.0',
      vendor: '8086',
      name: 'Intel Arc B580',
      driver: 'xe',
      version: null,
      vulkan: null,
    },
  ])
})

// ── AN.7: the nodeInfo key ──────────────────────────────────────────────────

const NODE_INFO_KEYS: Array<[string, unknown, string | null]> = [
  [
    'client-unavailable',
    { key: 'client-unavailable', args: [] },
    'client-unavailable',
  ],
  [
    'os-read-failed',
    { key: 'os-read-failed', args: ['passwd'] },
    'os-read-failed',
  ],
  ['a key that is empty', { key: '', args: [] }, null],
  ['a key that is a number', { key: 7, args: [] }, null],
  ['no key at all', { args: [] }, null],
  ['a string', 'client-unavailable', null],
  ['an array', [], null],
  ['null', null, null],
  ['left out', undefined, null],
]

test.each(NODE_INFO_KEYS)(
  '18 AN.7: a nodeInfo of %s',
  async (what, nodeInfo, key) => {
    const { node } = await readField('nodeInfo', nodeInfo)
    expect(node, 'the node is there: the reply is a good one').not.toBeNull()
    expect(node!.nodeInfoKey, `the key of ${what} reads ${key}`).toBe(key)
  },
)

test('18 AN.7: the manager-down reply of section 0 gives the node, the three fields null, and the key client-unavailable', async () => {
  const { node, ended } = await readOf(
    bodyWith({
      os: null,
      vulkan: null,
      gpus: null,
      nodeInfo: { key: 'client-unavailable', args: [] },
    }),
  )
  expect(ended, 'not the end of a session').toBe(false)
  expect(node, 'the node of today is still there').not.toBeNull()
  expect(node!.name).toBe(NODE.name)
  expect(node!.engines).toEqual(NODE.engines)
  expect(node!.os).toBeNull()
  expect(node!.vulkan).toBeNull()
  expect(node!.gpus).toBeNull()
  expect(node!.nodeInfoKey).toBe('client-unavailable')
})

// ── AN.7: today’s rules, kept ───────────────────────────────────────────────

const NO_NODE: Array<[string, unknown]> = [
  ['a name that is not a string', 7],
  ['a name that is null', null],
  ['a name that is empty', ''],
  ['a name of spaces', '   '],
  ['no name at all', undefined],
]

test.each(NO_NODE)(
  '18 AN.7: %s, with the new fields good, is node null, ended false',
  async (_what, name) => {
    const { node, ended } = await readOf(bodyWith({ name }))
    expect(node, 'nothing to show').toBeNull()
    expect(ended, 'not the end of a session').toBe(false)
  },
)

test('18 AN.7: engines that is not an array, with the new fields good, is node null', async () => {
  const { node } = await readField('engines', 'no')
  expect(node, 'a malformed reply (13 N.10, kept)').toBeNull()
})

test('18 AN.7: an engine with no name, and a backend with no name, are left out, the rest kept, as today', async () => {
  const engines = [
    { engine: '', version: '1', backends: [{ name: 'cuda', version: '1' }] },
    {
      engine: 'llama.cpp',
      version: '1:1.0.0+pai2',
      backends: [
        { name: '', version: '1' },
        { name: 'cuda', version: '13.5.0' },
      ],
    },
  ] as NodeEngine[]
  const { node } = await readField('engines', engines)
  expect(node!.engines, 'the named ones only').toEqual([
    {
      engine: 'llama.cpp',
      version: '1:1.0.0+pai2',
      backends: [{ name: 'cuda', version: '13.5.0' }],
    },
  ])
})

test('18 AN.7: 401 is ended true and node null, and a 503 is ended false and node null', async () => {
  for (const [status, key, ended] of [
    [401, 'session-required', true],
    [503, 'client-unavailable', false],
  ] as Array<[number, string, boolean]>) {
    fakeBff({ node: () => reply(status, { key, args: [] }) })
    const out = (await readNode()) as unknown as ReadNode
    expect(out.ended, `${status}: ended`).toBe(ended)
    expect(out.node, `${status}: nothing to show`).toBeNull()
  }
})

test('18 AN.7: a body that is not JSON, a body that is not an object, and a network failure, are node null, ended false', async () => {
  for (const answer of [
    () => reply(200, 'not json'),
    () => reply(200, []),
    () => Promise.reject(new TypeError('failed to fetch')),
  ]) {
    fakeBff({ node: answer })
    const out = (await readNode()) as unknown as ReadNode
    expect(out.node, 'nothing to show').toBeNull()
    expect(out.ended, 'not the end of a session').toBe(false)
  }
})
