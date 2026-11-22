// fe/src/NodePage.test.tsx
// Slice 18 AN5a and AN5b (fe/briefs/18-about-node-info.md, 18 AN.1, 18 AN.5,
// 18 AN.8, 18 AN.9, 18 AN.10 to AN.14): the strings of section 2, the `#/node`
// route, and the Node info page: its one read per arrival, the host list, the
// engines under their `h2`, the GPU blocks under the `h2` GPUs, the loading and
// the failures. The engine cases are the ones of 13 N.8 and 13 N.9, which brief
// 18 section 6 moves here from About: the engines live on this page now.
// "18 AN.8" is the brief's criterion 8.
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from '@testing-library/react'
import { afterEach, beforeEach, expect, test, vi } from 'vitest'
import App from './App.tsx'
import { parseRoute } from './route.ts'
import strings from './strings/en_US.json'
import {
  en,
  fakeBff,
  GPUS,
  held,
  navigate,
  NODE,
  OS,
  reply,
  startAt,
  type BffOptions,
  type NodeEngine,
  type NodeExtra,
  type NodeGpu,
  type NodeInfo,
} from './test/bff.ts'

// The page is reached by a navigation, as a visitor reaches it: the app moves
// the focus on a navigation, and a direct load takes none (09, AboutPage).
beforeEach(() => startAt('#/'))

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
  startAt('')
})

/** The node of the brief, with these engines, and this over the rest. */
function nodeWith(
  engines: unknown[],
  over: Partial<NodeInfo & NodeExtra> = {},
): Response {
  return reply(200, { ...NODE, ...over, engines })
}

/** The app at `#/node`, its page read answered by `node`. */
async function open(over: BffOptions = {}) {
  const bff = fakeBff({ node: NODE, ...over })
  render(<App />)
  navigate('#/node')
  const main = screen.getByRole('main')
  const h1 = await within(main).findByRole('heading', {
    level: 1,
    name: en('node-title'),
  })
  return { bff, main, h1 }
}

/** The page's read has landed: the `h2` over the engines is on screen. The h1
 * is there while the read is open, so a test that reads the lists waits, the
 * way a visitor waits for the words. */
async function settled(main: HTMLElement): Promise<void> {
  await within(main).findByRole('heading', {
    level: 2,
    name: en('node-engines'),
  })
}

/** The description lists of main, in document order. */
function lists(main: HTMLElement): HTMLDListElement[] {
  return Array.from(main.querySelectorAll('dl'))
}

/** The rows of a description list as `[term, value]`, in order (H40). */
function rows(list: HTMLElement): Array<[string, string]> {
  const out: Array<[string, string]> = []
  const kids = Array.from(list.children)
  for (let i = 0; i < kids.length; i++) {
    const dt = kids[i]
    if (dt.tagName !== 'DT') continue
    const dd = kids[i + 1]
    expect(
      dd?.tagName,
      `the term "${dt.textContent}" is followed by its dd`,
    ).toBe('DD')
    out.push([(dt.textContent ?? '').trim(), (dd?.textContent ?? '').trim()])
  }
  return out
}

/** The classes of a list's values, `[muted, mono]` per dd, in order. */
function ddClasses(list: HTMLElement): Array<[boolean, boolean]> {
  return Array.from(list.querySelectorAll('dd')).map((dd) => [
    dd.classList.contains('muted'),
    dd.classList.contains('mono'),
  ])
}

/** The flat elements of main above the `h2` GPUs: the host list and the
 * engine blocks. The GPU blocks (18 AN.10) sit under that heading, so what
 * AN.8 counts is here, never in the whole page. */
function enginePart(main: HTMLElement): HTMLElement[] {
  const h2 = within(main).getByRole('heading', {
    level: 2,
    name: en('node-gpus'),
  })
  const out: Element[] = []
  for (
    let el = main.firstElementChild;
    el && el !== h2;
    el = el.nextElementSibling
  )
    out.push(el)
  // Every element of main is an HTMLElement: the page builds it in the document.
  return out as HTMLElement[]
}

/** The engine lists: one per engine, each under its `h3`, above the GPUs. */
function engineLists(main: HTMLElement): HTMLDListElement[] {
  return enginePart(main).filter(
    (dl) => dl.tagName === 'DL' && dl.previousElementSibling?.tagName === 'H3',
  ) as HTMLDListElement[]
}

/** The engine headings: the `h3`s above the GPUs, as text, in order. */
function engineH3s(main: HTMLElement): string[] {
  return enginePart(main)
    .filter((el) => el.tagName === 'H3')
    .map((el) => (el.textContent ?? '').trim())
}

/** The `h3`s of main, in order, as text: the engine headings then the GPU ones. */
function h3s(main: HTMLElement): string[] {
  return within(main)
    .getAllByRole('heading', { level: 3 })
    .map((h) => (h.textContent ?? '').trim())
}

/** The node of the brief, with these GPUs, and this over the rest. */
function nodeGpus(
  gpus: unknown[],
  over: Partial<NodeInfo & NodeExtra> = {},
): Response {
  return reply(200, { ...NODE, ...over, gpus })
}

/** The AMD row of brief 18 section 7, with these fields changed. */
function gpu(over: Partial<NodeGpu> = {}): NodeGpu {
  return { ...GPUS[0]!, ...over }
}

/** The elements under the `h2` GPUs, in document order: the GPU blocks sit as
 * flat siblings of the heading, as the engine blocks do (18 decision 4). */
function gpuSection(main: HTMLElement): HTMLElement[] {
  const h2 = within(main).getByRole('heading', {
    level: 2,
    name: en('node-gpus'),
  })
  const out: Element[] = []
  for (let el = h2.nextElementSibling; el; el = el.nextElementSibling)
    out.push(el)
  // Every element of main is an HTMLElement: the page builds it in the document.
  return out as HTMLElement[]
}

/** The `h3` of the GPU blocks, as text, in order. */
function gpuHeadings(main: HTMLElement): string[] {
  return gpuSection(main)
    .filter((el) => el.tagName === 'H3')
    .map((el) => (el.textContent ?? '').trim())
}

/** The `dl` of the GPU blocks, in order. Every caller expects the blocks to be
 * there, so the empty list fails here, with the reason. */
function gpuLists(main: HTMLElement): HTMLDListElement[] {
  const on = gpuSection(main).filter(
    (el) => el.tagName === 'DL',
  ) as HTMLDListElement[]
  expect(on.length, 'the GPUs heading has a list under it').toBeGreaterThan(0)
  return on
}

/** The value `dd` of the row of `term`, in a list. */
function valueOf(list: HTMLElement, term: string): HTMLElement {
  const dts = Array.from(list.querySelectorAll('dt'))
  const i = dts.findIndex((dt) => (dt.textContent ?? '').trim() === term)
  expect(i, `the list has a ${term} row`).toBeGreaterThanOrEqual(0)
  const dd = dts[i]!.nextElementSibling
  expect(dd?.tagName, `${term}: its value is a dd`).toBe('DD')
  return dd as HTMLElement
}

/** The classes `[muted, mono]` of one value. */
function classesOf(el: HTMLElement): [boolean, boolean] {
  return [el.classList.contains('muted'), el.classList.contains('mono')]
}

// The sheet, comments out (18 AN.14): the GPU blocks ask for no rule of their
// own, so what breaks a 256-byte word is the rule the Value list already has.
const CSS = readFileSync(
  join(import.meta.dirname, 'index.css'),
  'utf8',
).replace(/\/\*[\s\S]*?\*\//g, '')
const RULES = [...CSS.matchAll(/([^{}]+)\{([^{}]*)\}/g)].map((m) => ({
  // The selector list as written, and each of its comma separated selectors
  // with the whitespace of the source folded away.
  sel: (m[1] ?? '').replace(/\s+/g, ' ').trim(),
  parts: (m[1] ?? '').split(',').map((s) => s.replace(/\s+/g, ' ').trim()),
  body: m[2] ?? '',
}))

/** The body of the rule that carries `sel` as one of its selectors. */
function bodyOf(sel: string): string {
  const rule = RULES.find((r) => r.parts.includes(sel))
  expect(rule, `the ${sel} rule of the sheet`).toBeTruthy()
  return rule!.body
}

/** The value of a declaration of a rule body, '' when the rule has none. */
function decl(body: string, name: string): string {
  let val = ''
  for (const d of body.split(';')) {
    const i = d.indexOf(':')
    if (i >= 0 && d.slice(0, i).trim() === name) val = d.slice(i + 1).trim()
  }
  return val
}

// ── 18 AN.1: the keys of section 2, with that English ───────────────────────

/** The 27 keys of section 2's table, with the English it gives. */
const SECTION_2: Array<[string, string]> = [
  ['nav-node', 'Node info'],
  ['node-title', 'Node info'],
  ['node-loading', "Loading the node's information…"],
  ['node-failed', "The node's information could not be read."],
  ['node-host', 'Host name'],
  ['system-os', 'Operating system'],
  ['system-kernel', 'Kernel'],
  ['node-engines', 'Engines'],
  ['node-gpus', 'GPUs'],
  ['node-gpus-none', 'No GPU was found on this node.'],
  [
    'node-info-unavailable',
    'The manager did not answer, so the operating system, the kernel and the GPUs are not shown. Reload the page to try again.',
  ],
  [
    'node-info-failed',
    'The manager could not read the operating system, the kernel and the GPUs.',
  ],
  ['node-gpu-title', 'GPU {0}: {1}'],
  ['node-gpu-title-unnamed', 'GPU {0}'],
  ['node-gpu-vendor', 'Vendor'],
  ['node-gpu-pci', 'PCI address'],
  ['node-gpu-driver', 'Driver'],
  [
    'node-driver-in-kernel',
    "(part of the kernel, so its version is the kernel's)",
  ],
  ['node-driver-none', 'No driver in use'],
  ['node-gpu-vulkan', 'Vulkan driver'],
  ['node-vulkan-unreported', 'Not reported by the device'],
  ['node-vulkan-not-seen', 'Not seen by Vulkan'],
  ['node-vulkan-none', 'No Vulkan driver installed'],
  ['node-vulkan-failed', 'Could not be read'],
  ['vendor-amd', 'AMD'],
  ['vendor-nvidia', 'NVIDIA'],
  ['vendor-intel', 'Intel'],
]

test('18 AN.1: en_US.json has the 27 keys of section 2 with that English, and changes no other', () => {
  const table = strings as Record<string, string>
  expect(SECTION_2.length, 'the table holds the brief\u2019s 27 keys').toBe(27)
  expect(
    SECTION_2.filter(([key, text]) => table[key] !== text).map(([k]) => k),
    'keys missing, or with other English than the brief',
  ).toEqual([])
  // No key is removed (section 2): the strings the pages of 13 read are there.
  for (const key of [
    'nav-about',
    'about-title',
    'about-loading',
    'about-failed',
    'app-name',
    'about-version-unknown',
    'about-version',
    'about-backends',
    'about-backends-none',
    'about-no-engine',
    'backend-cuda',
    'backend-rocm',
    'backend-vulkan',
  ])
    expect(table[key], `${key} is kept`).toBeTruthy()
})

// ── 18 AN.5: the route ─────────────────────────────────────────────────────

test('18 AN.5: the route #/node is the node page, and #/node/ and #/node/x are not found', () => {
  expect(parseRoute('#/node')).toEqual({ page: 'node' })
  expect(parseRoute('#/node/')).toEqual({ page: 'not-found' })
  expect(parseRoute('#/node/x')).toEqual({ page: 'not-found' })
})

test('18 AN.5: at #/node the document title is Node info with the app name, the h1 reads node-title and takes the focus on arrival', async () => {
  await open()
  expect(document.title, 'the page name and the app name, as every page').toBe(
    en('page-title', en('node-title')),
  )
  const h1 = screen.getByRole('heading', { level: 1, name: en('node-title') })
  expect(h1.getAttribute('tabindex'), 'the h1 is focusable').toBe('-1')
  await waitFor(() =>
    expect(document.activeElement, 'the focus is on the h1').toBe(h1),
  )
})

test('18 AN.5: signed out at #/node the sign-in page shows with the notice, and no GET /api/node is sent', async () => {
  startAt('#/node')
  const bff = fakeBff({ node: NODE, session: 'signed-out' })
  render(<App />)
  const main = screen.getByRole('main')
  await within(main).findByRole('heading', {
    level: 1,
    name: en('signin-title'),
  })
  expect(
    within(main).getByText(en('signin-required')),
    'the notice',
  ).toBeTruthy()
  expect(bff.to('GET', '/api/node').length, 'nothing read signed out').toBe(0)
})

// ── 18 AN.8: one read per arrival, and the top list ────────────────────────

test('18 AN.8: arriving at #/node sends one GET /api/node, leaving and coming back sends another, staying sends none', async () => {
  const { bff } = await open()
  expect(bff.to('GET', '/api/node').length, 'the read of this arrival').toBe(1)
  navigate('#/')
  await screen.findByRole('heading', { level: 1, name: en('overview-title') })
  expect(
    bff.to('GET', '/api/node').length,
    'the Overview reads the node nothing',
  ).toBe(1)
  navigate('#/node')
  await screen.findByRole('heading', { level: 1, name: en('node-title') })
  expect(bff.to('GET', '/api/node').length, 'the page reads again').toBe(2)
  fireEvent.click(
    screen.getByRole('heading', { level: 1, name: en('node-title') }),
  )
  expect(bff.to('GET', '/api/node').length, 'no read for a click').toBe(2)
})

test('18 AN.8: the reply of section 0 shows one dl.assets with the host, the operating system, the kernel and the framework, in that order', async () => {
  const { main } = await open()
  await settled(main)
  const top = lists(main)[0]
  expect(top, 'the page starts with a list').toBeTruthy()
  expect(top.classList.contains('assets'), 'the Value list, class assets').toBe(
    true,
  )
  expect(rows(top), 'the four rows, in the order of the brief').toEqual([
    [en('node-host'), NODE.name as string],
    [en('system-os'), `${OS.name} ${OS.version}`],
    [en('system-kernel'), OS.kernel as string],
    [en('app-name'), NODE.version as string],
  ])
  expect(ddClasses(top), 'all four values mono, none muted (AN.8)').toEqual([
    [false, true],
    [false, true],
    [false, true],
    [false, true],
  ])
})

/** The operating system answers of AN.3, and what the two rows show. */
const NODE_OS_ANSWERS: Array<[string, (NodeInfo & NodeExtra)['os']]> = [
  [
    'os of null name, version and kernel',
    { name: null, version: null, kernel: null },
  ],
  ['the whole os block null', null],
]

test.each(NODE_OS_ANSWERS)(
  '18 AN.8: %s: the operating system and the kernel read Unknown, muted and not mono, the host and the framework keeping theirs',
  async (_what, os) => {
    const { main } = await open({ node: nodeWith(NODE.engines, { os }) })
    await settled(main)
    const top = lists(main)[0]!
    expect(rows(top)).toEqual([
      [en('node-host'), NODE.name as string],
      [en('system-os'), en('about-version-unknown')],
      [en('system-kernel'), en('about-version-unknown')],
      [en('app-name'), NODE.version as string],
    ])
    const dds = Array.from(top.querySelectorAll('dd'))
    expect(dds.length, 'four values').toBe(4)
    for (const i of [1, 2])
      expect(
        [
          dds[i]!.classList.contains('muted'),
          dds[i]!.classList.contains('mono'),
        ],
        `the Unknown of row ${i}: muted, never mono`,
      ).toEqual([true, false])
    expect(
      ddClasses(top)[3]!,
      'the framework version stays a mono value',
    ).toEqual([false, true])
  },
)

// ── 18 AN.8: the engines under their h2 (13 N.8 and N.9, moved here) ───────

test('18 AN.8: the engines of the brief sit under an h2 Engines, as a h3 with the Version and the backend rows, before the h2 GPUs', async () => {
  const { main } = await open()
  await settled(main)
  const order = Array.from(main.querySelectorAll('h2, h3, dl')).map((el) =>
    [el.tagName, (el.textContent ?? '').trim()].join(' '),
  )
  // The host list first (decision 4), the engines between the two `h2`, and the
  // GPUs heading: what sits under it is AN.10's, pinned below.
  expect(
    order.slice(0, 5),
    'the page, in the order it reads, to the GPUs',
  ).toEqual([
    `DL ${en('node-host')}${NODE.name as string}${en('system-os')}${OS.name} ${OS.version}${en('system-kernel')}${OS.kernel as string}${en('app-name')}${NODE.version as string}`,
    'H2 ' + en('node-engines'),
    'H3 llama.cpp',
    `DL ${en('about-version')}1:1.0.0+pai2${en('backend-cuda')}13.5.0+nccl.2.32.3${en('backend-rocm')}10.1.0${en('backend-vulkan')}1.4.363.1`,
    'H2 ' + en('node-gpus'),
  ])
  const engines = engineLists(main)
  expect(engines.length, 'one engine list').toBe(1)
  expect(engines[0]!.classList.contains('assets'), 'the Value list').toBe(true)
  expect(ddClasses(engines[0]!), 'every version mono').toEqual([
    [false, true],
    [false, true],
    [false, true],
    [false, true],
  ])
})

test('18 AN.8: two engines give two h3 and two lists in the reply order, and a backend the strings do not know shows the name it was sent with', async () => {
  const engines: NodeEngine[] = [
    {
      engine: 'llama.cpp',
      version: '1:1.0.0+pai2',
      backends: [{ name: 'cuda', version: '13.5.0' }],
    },
    {
      engine: 'other.cpp',
      version: '0.9.0',
      backends: [{ name: 'opencl', version: '7.7' }],
    },
  ]
  const { main } = await open({ node: nodeWith(engines) })
  await settled(main)
  expect(h3s(main), 'the engines in the reply order, then the GPUs').toEqual([
    'llama.cpp',
    'other.cpp',
    en('node-gpu-title', '1', 'AMD Radeon AI PRO R9700'),
    en('node-gpu-title', '2', 'NVIDIA GeForce RTX 5090'),
    en('node-gpu-title', '3', 'Intel Arc B580'),
  ])
  const on = engineLists(main)
  expect(on.length, 'one list per engine').toBe(2)
  expect(rows(on[0]!), 'the first engine').toEqual([
    [en('about-version'), '1:1.0.0+pai2'],
    [en('backend-cuda'), '13.5.0'],
  ])
  expect(
    rows(on[1]!),
    'the second, its unknown backend named as it is',
  ).toEqual([
    [en('about-version'), '0.9.0'],
    ['opencl', '7.7'],
  ])
})

test('18 AN.8: a framework, engine or backend version of null, the empty string or a number reads Unknown in a muted dd, not a mono one', async () => {
  for (const [what, version] of [
    ['null', null],
    ['the empty string', ''],
    ['a number', 7],
  ] as Array<[string, unknown]>) {
    const { main } = await open({
      node: nodeWith(
        [
          {
            engine: 'llama.cpp',
            version,
            backends: [{ name: 'cuda', version }],
          },
        ],
        { version: version as string | null },
      ),
    })
    await settled(main)
    const on = lists(main)
    expect(rows(on[0]!), `${what}: the framework row reads Unknown`).toEqual([
      [en('node-host'), NODE.name as string],
      [en('system-os'), `${OS.name} ${OS.version}`],
      [en('system-kernel'), OS.kernel as string],
      [en('app-name'), en('about-version-unknown')],
    ])
    expect(
      ddClasses(on[0]!).at(-1),
      `${what}: the framework value muted, not mono`,
    ).toEqual([true, false])
    expect(
      rows(on[1]!),
      `${what}: the engine version, and the backend`,
    ).toEqual([
      [en('about-version'), en('about-version-unknown')],
      [en('backend-cuda'), en('about-version-unknown')],
    ])
    expect(ddClasses(on[1]!), `${what}: both muted, neither mono`).toEqual([
      [true, false],
      [true, false],
    ])
    cleanup()
    vi.unstubAllGlobals()
  }
})

test('18 AN.8: an engine with no backends, sent as [] or left out, shows the GPU backends row reading None, CPU only, in a muted dd', async () => {
  for (const [what, engine] of [
    [
      'backends []',
      { engine: 'llama.cpp', version: '1:1.0.0+pai2', backends: [] },
    ],
    ['no backends field', { engine: 'llama.cpp', version: '1:1.0.0+pai2' }],
  ] as Array<[string, NodeEngine]>) {
    const { main } = await open({ node: nodeWith([engine]) })
    await settled(main)
    const on = engineLists(main)
    expect(
      rows(on[0]!),
      `${what}: the Version row, then the backends row`,
    ).toEqual([
      [en('about-version'), '1:1.0.0+pai2'],
      [en('about-backends'), en('about-backends-none')],
    ])
    expect(
      ddClasses(on[0]!),
      `${what}: the version mono, the backends value muted and not mono`,
    ).toEqual([
      [false, true],
      [true, false],
    ])
    cleanup()
    vi.unstubAllGlobals()
  }
})

test('18 AN.8: engines [] shows the host list, the h2 Engines, one paragraph saying no engine was found, and no h3', async () => {
  const { main } = await open({ node: nodeWith([]) })
  await settled(main)
  expect(
    within(main).getByText(en('about-no-engine')),
    'the words of 13 N.9, kept',
  ).toBeTruthy()
  expect(engineH3s(main).length, 'no engine heading').toBe(0)
  expect(
    enginePart(main).filter((el) => el.tagName === 'DL').length,
    'the host list only above the GPUs',
  ).toBe(1)
})

test('18 AN.8: inside a good reply an engine without a name, and a backend without a name, are left out; the rest of the page shows', async () => {
  const { main } = await open({
    node: nodeWith([
      { engine: '', version: '1', backends: [{ name: 'cuda', version: '1' }] },
      {
        engine: 'llama.cpp',
        version: '1:1.0.0+pai2',
        backends: [
          { name: '', version: '1' },
          { name: 'cuda', version: '13.5.0' },
        ],
      },
      { engine: 'other.cpp', version: '0.9.0', backends: [] },
    ]),
  })
  await settled(main)
  expect(
    h3s(main),
    'the engine with no name is gone, the two others stay in order, then the GPUs',
  ).toEqual([
    'llama.cpp',
    'other.cpp',
    en('node-gpu-title', '1', 'AMD Radeon AI PRO R9700'),
    en('node-gpu-title', '2', 'NVIDIA GeForce RTX 5090'),
    en('node-gpu-title', '3', 'Intel Arc B580'),
  ])
  const on = engineLists(main)
  expect(rows(on[0]!), 'the backend with no name is gone').toEqual([
    [en('about-version'), '1:1.0.0+pai2'],
    [en('backend-cuda'), '13.5.0'],
  ])
  expect(
    rows(on[1]!),
    'the engine with no backends shows its backends row',
  ).toEqual([
    [en('about-version'), '0.9.0'],
    [en('about-backends'), en('about-backends-none')],
  ])
})

test('18 AN.8: the page is text: its only focusable element is the h1 the page took the focus on', async () => {
  const { main, h1 } = await open()
  await settled(main)
  await waitFor(() =>
    expect(document.activeElement, 'the h1 keeps it').toBe(h1),
  )
  const focusable = Array.from(
    main.querySelectorAll('a, button, input, select, textarea, [tabindex]'),
  )
  expect(
    focusable.map((el) => el.tagName),
    'no link, no control, and the h1 as the only tabindex',
  ).toEqual(['H1'])
})

test('18 AN.8: the page uses no class beside the ones of the lists: assets, mono and muted', async () => {
  const { main } = await open()
  await settled(main)
  const classes = new Set<string>()
  for (const el of Array.from(main.querySelectorAll('*')))
    for (const c of Array.from(el.classList)) classes.add(c)
  expect(
    [...classes].sort(),
    'the classes of the page (13 N.12 held, no new class for brief 18)',
  ).toEqual(['assets', 'mono', 'muted'].filter((c) => classes.has(c)).sort())
})

// ── 18 AN.9: the states ────────────────────────────────────────────────────

test('18 AN.9: while the read is open the page shows the loading words in a status paragraph and no list', async () => {
  const h = held()
  const { main } = await open({ node: h.answer })
  const status = await within(main).findByRole('status')
  expect(status.tagName, 'a paragraph').toBe('P')
  expect(status.textContent?.trim(), 'the loading words of section 2').toBe(
    en('node-loading'),
  )
  expect(
    main.querySelectorAll('dl').length,
    'no list while the read is open',
  ).toBe(0)
  expect(
    within(main).queryByText(en('node-failed')),
    'not the failure words',
  ).toBeNull()
  h.resolve(reply(200, NODE))
})

test('18 AN.9: a page read answering 401 signs the app out: the sign-in page at #/node, its notice, the focus on its h1', async () => {
  const { bff } = await open({
    node: () => reply(401, { key: 'session-required', args: [] }),
  })
  expect(bff.to('GET', '/api/node').length, 'one page read, no retry').toBe(1)
  const main = screen.getByRole('main')
  const h1 = await within(main).findByRole('heading', {
    level: 1,
    name: en('signin-title'),
  })
  expect(
    within(main).getByText(en('signin-session-ended')),
    'the notice of an ended session',
  ).toBeTruthy()
  await waitFor(() =>
    expect(document.activeElement, 'the focus is on the sign-in h1').toBe(h1),
  )
  expect(window.location.hash, 'the place is kept').toBe('#/node')
})

test('18 AN.9: a page read answering 403 session-not-administrator ends the session as every read does: the sign-in page with the revoked notice', async () => {
  const { bff } = await open({
    node: () => reply(403, { key: 'session-not-administrator', args: [] }),
  })
  expect(bff.to('GET', '/api/node').length, 'one page read, no retry').toBe(1)
  const main = screen.getByRole('main')
  await within(main).findByRole('heading', {
    level: 1,
    name: en('signin-title'),
  })
  expect(
    within(main).getByText(en('session-revoked')),
    'the notice of a session the manager refused',
  ).toBeTruthy()
})

const NODE_FAILURES: Array<[string, () => Response | Promise<Response>]> = [
  ['500', () => reply(500, 'a manager detail')],
  [
    '503 client-unavailable',
    () => reply(503, { key: 'client-unavailable', args: [] }),
  ],
  ['a network failure', () => Promise.reject(new TypeError('failed to fetch'))],
  ['a body that is not json', () => reply(200, 'not json')],
  ['a body that is not an object', () => reply(200, [])],
  ['a name that is not a string', () => reply(200, { ...NODE, name: 7 })],
  ['a name that is empty', () => reply(200, { ...NODE, name: '' })],
  [
    'engines that is not an array',
    () => reply(200, { ...NODE, engines: 'no' }),
  ],
]

test.each(NODE_FAILURES)(
  '18 AN.9: a page read answering %s shows the failure words as a plain paragraph in place of every list, and keeps the h1',
  async (_what, node) => {
    const { main, h1 } = await open({ node })
    const p = await within(main).findByText(en('node-failed'))
    expect(p.tagName, 'a paragraph').toBe('P')
    expect(p.closest('[role="alert"]'), 'not an alert: nobody acted').toBeNull()
    expect(p.getAttribute('role'), 'no role at all').toBeNull()
    expect(lists(main).length, 'no list in place of the facts').toBe(0)
    expect(
      within(main).queryAllByRole('heading', { level: 2 }).length,
      'no section heading',
    ).toBe(0)
    expect(
      within(main).queryByText(en('node-loading')),
      'the loading text is gone',
    ).toBeNull()
    expect(
      within(main).getByRole('heading', { level: 1, name: en('node-title') }),
      'the h1 is kept',
    ).toBe(h1)
  },
)

// ── 18 AN.10: one block per GPU ─────────────────────────────────────────────

test('18 AN.10: the three GPUs of section 7 stand under the h2 as three blocks, in the reply order, each an h3 and a dl.assets of the four rows, the headings numbered from 1', async () => {
  const { main } = await open()
  await settled(main)
  expect(GPUS.length, 'the fake answers the three rows the stub sends').toBe(3)
  expect(
    gpuSection(main).map((el) => el.tagName),
    'heading, list, heading, list, heading, list, and nothing else under the GPUs',
  ).toEqual(['H3', 'DL', 'H3', 'DL', 'H3', 'DL'])
  expect(
    gpuHeadings(main),
    'the headings of the brief, numbered from 1 in the reply order',
  ).toEqual([
    en('node-gpu-title', '1', 'AMD Radeon AI PRO R9700'),
    en('node-gpu-title', '2', 'NVIDIA GeForce RTX 5090'),
    en('node-gpu-title', '3', 'Intel Arc B580'),
  ])
  const on = gpuLists(main)
  expect(on.length, 'one list per GPU').toBe(3)
  for (const [i, dl] of on.entries()) {
    expect(
      dl.classList.contains('assets'),
      `GPU ${i + 1}: the Value list`,
    ).toBe(true)
    expect(
      rows(dl).map(([term]) => term),
      `GPU ${i + 1}: exactly the four rows of the brief, in that order`,
    ).toEqual([
      en('node-gpu-vendor'),
      en('node-gpu-pci'),
      en('node-gpu-driver'),
      en('node-gpu-vulkan'),
    ])
  }
  expect(rows(on[0]!), 'the AMD row, as the brief reads it').toEqual([
    [en('node-gpu-vendor'), 'AMD'],
    [en('node-gpu-pci'), '0000:03:00.0'],
    [en('node-gpu-driver'), 'amdgpu 7.1.9.31600000'],
    [en('node-gpu-vulkan'), 'radv Mesa 25.2.0'],
  ])
  expect(rows(on[1]!), 'the NVIDIA row').toEqual([
    [en('node-gpu-vendor'), 'NVIDIA'],
    [en('node-gpu-pci'), '0000:0a:00.0'],
    [en('node-gpu-driver'), 'nvidia 580.95.05'],
    [en('node-gpu-vulkan'), 'NVIDIA 580.95.05'],
  ])
  expect(
    rows(on[2]!),
    'the Intel row: an in-kernel driver, and Vulkan does not see the card',
  ).toEqual([
    [en('node-gpu-vendor'), 'Intel'],
    [en('node-gpu-pci'), '0000:0f:00.0'],
    [en('node-gpu-driver'), `xe ${en('node-driver-in-kernel')}`],
    [en('node-gpu-vulkan'), en('node-vulkan-not-seen')],
  ])
})

test('18 AN.10: a GPU with no name, sent as null or as spaces, is headed GPU with its number alone, and the numbering keeps the reply order', async () => {
  const { main } = await open({
    node: nodeGpus([
      gpu({ name: null }),
      gpu({ name: '   ' }),
      gpu({ name: 'Intel Arc B580' }),
    ]),
  })
  await settled(main)
  expect(
    gpuHeadings(main),
    'the index is the row, not the count of named ones',
  ).toEqual([
    en('node-gpu-title-unnamed', '1'),
    en('node-gpu-title-unnamed', '2'),
    en('node-gpu-title', '3', 'Intel Arc B580'),
  ])
})

/** The vendors of AN.10: what the reply sends, the text, the classes `[muted, mono]`. */
const VENDORS: Array<[string, NodeGpu['vendor'], string, [boolean, boolean]]> =
  [
    ['1002', '1002', 'AMD', [false, false]],
    ['10de', '10de', 'NVIDIA', [false, false]],
    ['8086', '8086', 'Intel', [false, false]],
    ['1af4', '1af4', '1af4', [false, true]],
    ['a vendor nobody named', 'Red Sun', 'Red Sun', [false, true]],
    ['null', null, en('about-version-unknown'), [true, false]],
  ]

test.each(VENDORS)(
  '18 AN.10: the vendor %s shows as %s, in the classes it owes',
  async (_what, sent, want, classes) => {
    const { main } = await open({ node: nodeGpus([gpu({ vendor: sent })]) })
    await settled(main)
    const dl = gpuLists(main)[0]!
    expect(rows(dl)[0], `${sent}: the vendor row`).toEqual([
      en('node-gpu-vendor'),
      want,
    ])
    expect(
      classesOf(valueOf(dl, en('node-gpu-vendor'))),
      `${sent}: named in the body font, an unknown one as sent in mono, null muted`,
    ).toEqual(classes)
  },
)

test('18 AN.10: the PCI address is a mono value, and null reads Unknown muted', async () => {
  const { main } = await open({ node: nodeGpus([gpu(), gpu({ pci: null })]) })
  await settled(main)
  const on = gpuLists(main)
  expect(rows(on[0]!)[1], 'the address as sent').toEqual([
    en('node-gpu-pci'),
    '0000:03:00.0',
  ])
  expect(
    classesOf(valueOf(on[0]!, en('node-gpu-pci'))),
    'the address is data: mono, not muted',
  ).toEqual([false, true])
  expect(rows(on[1]!)[1], 'no address: the word, never a blank').toEqual([
    en('node-gpu-pci'),
    en('about-version-unknown'),
  ])
  expect(
    classesOf(valueOf(on[1]!, en('node-gpu-pci'))),
    'the Unknown is muted, not mono',
  ).toEqual([true, false])
})

// ── 18 AN.11: the driver row ────────────────────────────────────────────────

test('18 AN.11: a driver with its module version is one mono text, the two joined by one space', async () => {
  const { main } = await open({ node: nodeGpus([gpu()]) })
  await settled(main)
  const dl = gpuLists(main)[0]!
  const dd = valueOf(dl, en('node-gpu-driver'))
  expect((dd.textContent ?? '').trim(), 'amdgpu and its version').toBe(
    'amdgpu 7.1.9.31600000',
  )
  expect(classesOf(dd), 'the whole value is data: mono, not muted').toEqual([
    false,
    true,
  ])
  expect(dd.children.length, 'one text, no span inside it').toBe(0)
})

test('18 AN.11: a driver with no module version is its name in a span.mono, a space, then the in-kernel words in a span.muted', async () => {
  // The in-kernel row of brief 18 section 7: Intel Arc B580, driver `xe`, no
  // module version, so the version is the kernel's.
  const { main } = await open({ node: nodeGpus([{ ...GPUS[2]! }]) })
  await settled(main)
  const dl = gpuLists(main)[0]!
  const dd = valueOf(dl, en('node-gpu-driver'))
  expect(
    dd.textContent,
    'the name, one space, the words: the version is the kernel',
  ).toBe(`xe ${en('node-driver-in-kernel')}`)
  expect(
    Array.from(dd.children).map((el) => [
      el.tagName,
      (el.textContent ?? '').trim(),
    ]),
    'the two parts, in order',
  ).toEqual([
    ['SPAN', 'xe'],
    ['SPAN', en('node-driver-in-kernel')],
  ])
  expect(
    Array.from(dd.children).map((el) => [
      el.classList.contains('mono'),
      el.classList.contains('muted'),
    ]),
    'the name mono, the words muted',
  ).toEqual([
    [true, false],
    [false, true],
  ])
  expect(classesOf(dd), 'the dd carries neither class: the spans do').toEqual([
    false,
    false,
  ])
})

test.each([
  ['with its version set', '7.1.9.31600000'],
  ['with no version', null],
] as Array<[string, string | null]>)(
  '18 AN.11: a driver of null, %s, reads No driver in use in a muted dd, never mono',
  async (_what, version) => {
    const { main } = await open({
      node: nodeGpus([gpu({ driver: null, version })]),
    })
    await settled(main)
    const dl = gpuLists(main)[0]!
    const dd = valueOf(dl, en('node-gpu-driver'))
    expect(
      (dd.textContent ?? '').trim(),
      'the words of section 2, the version left out',
    ).toBe(en('node-driver-none'))
    expect(dd.children.length, 'one text, no span').toBe(0)
    expect(classesOf(dd), 'muted, not mono: it is no value').toEqual([
      true,
      false,
    ])
  },
)

// ── 18 AN.12: the Vulkan row ────────────────────────────────────────────────

/** The Vulkan answers of AN.12: the row, the reply, the text, the classes. The
 * reply's value is `unknown`: the page must answer a word the bff does not send. */
const VULKANS: Array<
  [string, NodeGpu['vulkan'], unknown, string, [boolean, boolean]]
> = [
  [
    'the driver and the info',
    { driver: 'radv', info: 'Mesa 25.2.0' },
    'ok',
    'radv Mesa 25.2.0',
    [false, true],
  ],
  [
    'only the driver',
    { driver: 'radv', info: null },
    'ok',
    'radv',
    [false, true],
  ],
  [
    'only the info',
    { driver: null, info: 'Mesa 25.2.0' },
    'ok',
    'Mesa 25.2.0',
    [false, true],
  ],
  [
    'both null',
    { driver: null, info: null },
    'ok',
    en('node-vulkan-unreported'),
    [true, false],
  ],
  [
    'the row null and the reply ok',
    null,
    'ok',
    en('node-vulkan-not-seen'),
    [true, false],
  ],
  [
    'the row null and the reply none',
    null,
    'none',
    en('node-vulkan-none'),
    [true, false],
  ],
  [
    'the row null and the reply failed',
    null,
    'failed',
    en('node-vulkan-failed'),
    [true, false],
  ],
  [
    'the row null and the reply any other word',
    null,
    'sideways',
    en('node-vulkan-failed'),
    [true, false],
  ],
]

test.each(VULKANS)(
  '18 AN.12: the Vulkan row, %s, shows %s in the classes it owes',
  async (_what, row, replyVulkan, want, classes) => {
    const { main } = await open({
      node: nodeGpus([gpu({ vulkan: row })], {
        vulkan: replyVulkan as NodeExtra['vulkan'],
      }),
    })
    await settled(main)
    const dl = gpuLists(main)[0]!
    expect(rows(dl)[3], 'the Vulkan row').toEqual([en('node-gpu-vulkan'), want])
    expect(
      classesOf(valueOf(dl, en('node-gpu-vulkan'))),
      'a value is mono; a word for a missing fact is muted',
    ).toEqual(classes)
  },
)

// ── 18 AN.13: no GPU, or no answer ──────────────────────────────────────────

test('18 AN.13: gpus [] is one paragraph under the h2 saying no GPU was found, no h3 there, the engines above untouched', async () => {
  const { main } = await open({ node: nodeGpus([]) })
  await settled(main)
  const section = gpuSection(main)
  expect(
    section.map((el) => el.tagName),
    'one paragraph and nothing else under the GPUs',
  ).toEqual(['P'])
  const p = section[0]!
  expect((p.textContent ?? '').trim(), 'the words of section 2').toBe(
    en('node-gpus-none'),
  )
  expect(p.getAttribute('role'), 'no role: nobody acted').toBeNull()
  expect(p.closest('[role="alert"]'), 'not an alert').toBeNull()
  expect(
    h3s(main),
    'the engine heading stays, no GPU heading is added',
  ).toEqual(['llama.cpp'])
  expect(engineLists(main).length, 'the engine list stays').toBe(1)
})

/** The no-answer shapes of AN.13: what the reply carries for the GPUs, and the
 * words the page owes. */
const NO_ANSWER: Array<[string, unknown, NodeExtra['nodeInfo'], string]> = [
  [
    'gpus null with nodeInfo client-unavailable',
    null,
    { key: 'client-unavailable', args: [] },
    en('node-info-unavailable'),
  ],
  [
    'gpus null with nodeInfo client-other',
    null,
    { key: 'client-other', args: [] },
    en('node-info-failed'),
  ],
  ['gpus null with no nodeInfo', null, undefined, en('node-info-failed')],
  [
    'gpus not an array, nodeInfo client-unavailable',
    'no',
    { key: 'client-unavailable', args: [] },
    en('node-info-unavailable'),
  ],
]

test.each(NO_ANSWER)(
  '18 AN.13: %s: one plain paragraph under the GPUs, no h3 there, the top list Unknown for the operating system and the kernel, the host, the framework and the engines still shown',
  async (_what, gpus, nodeInfo, want) => {
    const { main } = await open({
      node: reply(200, {
        ...NODE,
        os: null,
        vulkan: null,
        gpus,
        ...(nodeInfo ? { nodeInfo } : {}),
      }),
    })
    await settled(main)
    const section = gpuSection(main)
    expect(
      section.map((el) => el.tagName),
      'the paragraph is all there is under the GPUs',
    ).toEqual(['P'])
    expect(
      (section[0]!.textContent ?? '').trim(),
      'the words the reply answers',
    ).toBe(want)
    expect(section[0]!.getAttribute('role'), 'no role: nobody acted').toBeNull()
    expect(h3s(main), 'the engine heading, and no GPU heading').toEqual([
      'llama.cpp',
    ])
    const top = lists(main)[0]!
    expect(
      rows(top),
      'the bff keeps its own facts, the manager’s are words',
    ).toEqual([
      [en('node-host'), NODE.name as string],
      [en('system-os'), en('about-version-unknown')],
      [en('system-kernel'), en('about-version-unknown')],
      [en('app-name'), NODE.version as string],
    ])
    expect(engineLists(main).length, 'the engines the bff knows stay').toBe(1)
  },
)

// ── 18 AN.14: wrapping ──────────────────────────────────────────────────────

test('18 AN.14: the 32 GPUs of node=long render 32 blocks, each heading numbered, every 256-byte text a value of its own list', async () => {
  const long = '0123456789abcdef'.repeat(16)
  expect(long.length, 'the 256 unbroken characters of the stub').toBe(256)
  const rowsOf = Array.from({ length: 32 }, (_unused, i) =>
    gpu({
      pci: `0000:${i.toString(16).padStart(2, '0')}:00.0`,
      vendor: long,
      name: long,
      driver: long,
      version: long,
      vulkan: { driver: long, info: long },
    }),
  )
  const { main } = await open({
    node: nodeGpus(rowsOf, { name: long.slice(0, 253) }),
  })
  await settled(main)
  expect(gpuLists(main).length, 'one list per row').toBe(32)
  expect(
    gpuSection(main).map((el) => el.tagName),
    'a heading and a list for every row',
  ).toHaveLength(64)
  expect(
    gpuHeadings(main).map((h) => /^GPU (\d+): /.exec(h)?.[1]),
    'the headings numbered 1 to 32, in the reply order',
  ).toEqual(Array.from({ length: 32 }, (_unused, i) => String(i + 1)))
  const last = gpuLists(main)[31]!
  expect(
    rows(last).map(([, value]) => value),
    'the long texts, one per row of the block: the driver and the Vulkan driver each joined with their version, as AN.11 and AN.12 pin, the stub sending `version` 256 too',
  ).toEqual([long, `0000:1f:00.0`, `${long} ${long}`, `${long} ${long}`])
  expect(
    rows(last).map(([term]) => term),
    'the four rows, as every other block',
  ).toEqual([
    en('node-gpu-vendor'),
    en('node-gpu-pci'),
    en('node-gpu-driver'),
    en('node-gpu-vulkan'),
  ])
})

test('18 AN.14: the sheet needs no rule for a GPU: the Value list breaks anywhere, h2 and h3 inherit the body break, the driver spans stay inline, and the sheet gains no node class', () => {
  expect(
    decl(bodyOf('.assets dd'), 'overflow-wrap'),
    'a 256-byte value breaks inside its dd, wherever it has to',
  ).toBe('anywhere')
  expect(
    decl(bodyOf('body'), 'overflow-wrap'),
    'the body break the headings inherit',
  ).toBe('break-word')
  for (const sel of ['h2', 'h3'])
    expect(
      decl(bodyOf(sel), 'overflow-wrap'),
      `${sel}: declares none of its own, so it inherits the body break-word`,
    ).toBe('')
  expect(
    decl(bodyOf('.assets + h3'), 'margin-top'),
    'the gap above a heading under a list: it never reads as one more row',
  ).toBe('var(--space-6)')
  for (const sel of ['.muted', '.mono'])
    expect(
      decl(bodyOf(sel), 'display'),
      `${sel}: no display, so the driver row’s spans stay inline inside the dd, where anywhere applies`,
    ).toBe('')
  expect(
    RULES.filter((r) => /\bgpu\b/.test(r.sel)).map((r) => r.sel),
    'the sheet gains no rule for the GPU blocks: the only selectors naming a gpu are the Models page check-list, there before this slice. The blocks ride on the lists of 13 (18 AN.8, 13 N.12).',
  ).toEqual([
    '#model-profile-gpu-devices > .gpu-blocks-line, #model-profile-gpu-devices > .check-list',
    '.check-list .gpu-id',
    '.check-list .gpu-weight',
  ])
})
