// fe/src/AboutPage.test.tsx
// Slice 13's About page (fe/briefs/13-node-about.md, 13 N.7, 13 N.8, 13 N.9,
// 13 N.10): the page at #/about, its one read per arrival, the versions as a
// description list per engine, the unknown and empty answers, and the failures.
// "13 N.8" is the brief's criterion 8.
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
import {
  en,
  fakeBff,
  held,
  navigate,
  NODE,
  reply,
  startAt,
  type NodeEngine,
  type NodeInfo,
} from './test/bff.ts'

// The page is reached by a navigation, as a visitor reaches it: the app moves
// the focus on a navigation, and a direct load takes none (09: "nothing takes
// the focus on load", SignInOnly.test.tsx).
beforeEach(() => startAt('#/'))

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
  startAt('')
})

/** The host of the brief, with these engines. */
function nodeWith(engines: unknown[], over: Partial<NodeInfo> = {}): Response {
  return reply(200, { ...NODE, ...over, engines })
}

/** The app at `#/about`, its page read answered by `node`. */
async function open(over: Parameters<typeof fakeBff>[0] = {}) {
  const bff = fakeBff({ node: NODE, ...over })
  render(<App />)
  navigate('#/about')
  const main = screen.getByRole('main')
  const h1 = await within(main).findByRole('heading', {
    level: 1,
    name: en('about-title'),
  })
  return { bff, main, h1 }
}

/**
 * The page's read has landed: its reply's host `h2` is on screen. The h1 is
 * there while the read is open, so a test that reads the lists has to wait for
 * the reply, the way a visitor waits for the words.
 */
async function settled(main: HTMLElement): Promise<void> {
  await within(main).findByRole('heading', { level: 2 })
}

/** The h2s of main, in order, as text. */
function hostHeadings(main: HTMLElement): string[] {
  return within(main)
    .getAllByRole('heading', { level: 2 })
    .map((h) => (h.textContent ?? '').trim())
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

// ── 13 N.7: the page, and its one read per arrival ──────────────────────────

test('13 N.7: at #/about the document title is About with the app name, the h1 reads About and takes the focus on arrival', async () => {
  await open()
  expect(document.title, 'the page name and the app name, as every page').toBe(
    en('page-title', en('about-title')),
  )
  const h1 = screen.getByRole('heading', { level: 1, name: en('about-title') })
  expect(
    h1.getAttribute('tabindex'),
    'the h1 is focusable, as every page heading',
  ).toBe('-1')
  await waitFor(() =>
    expect(document.activeElement, 'the focus is on the h1').toBe(h1),
  )
})

test('13 N.7: while the read is open the page shows the loading text in a status paragraph and no list', async () => {
  const h = held()
  const { main } = await open({ node: h.answer })
  const status = await within(main).findByRole('status')
  expect(status.tagName, 'a paragraph').toBe('P')
  expect(status.textContent?.trim(), 'the loading words').toBe(
    en('about-loading'),
  )
  expect(
    main.querySelectorAll('dl').length,
    'no list while the read is open',
  ).toBe(0)
  expect(
    within(main).queryByText(en('about-failed')),
    'not the failure words',
  ).toBeNull()
})

test('13 N.7: arriving at #/about sends one GET /api/node of the page own, beside the shell one; leaving and coming back sends another, staying sends none', async () => {
  const { bff } = await open()
  expect(
    bff.to('GET', '/api/node').length,
    'the session read, and the page read of this arrival',
  ).toBe(2)
  navigate('#/')
  await screen.findByRole('heading', { level: 1, name: en('overview-title') })
  expect(bff.to('GET', '/api/node').length, 'the Overview reads nothing').toBe(
    2,
  )
  navigate('#/about')
  await screen.findByRole('heading', { level: 1, name: en('about-title') })
  expect(
    bff.to('GET', '/api/node').length,
    'the page reads again on the next arrival',
  ).toBe(3)
  // Staying on it: nothing re-reads.
  fireEvent.click(
    screen.getByRole('heading', { level: 1, name: en('about-title') }),
  )
  expect(bff.to('GET', '/api/node').length, 'no read for a click').toBe(3)
})

test('13 N.7: signed out at #/about the sign-in page shows with the notice, and no GET /api/node is sent', async () => {
  startAt('#/about')
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

// ── 13 N.8: the versions ────────────────────────────────────────────────────

test('13 N.8: the reply of the brief shows the host as a h2, the framework row, then the engine as a h3 with its version and its backends', async () => {
  const { main } = await open()
  await settled(main)
  expect(hostHeadings(main), 'one host section, named by the host').toEqual([
    NODE.name,
  ])
  const on = lists(main)
  expect(on.length, 'the framework list and the engine list').toBe(2)
  expect(
    on.map((dl) => dl.classList.contains('assets')),
    'both are the Value list, class assets, no new class',
  ).toEqual([true, true])
  expect(
    rows(on[0]),
    'the framework row: the app name, and its version',
  ).toEqual([[en('app-name'), NODE.version as string]])
  expect(
    within(main)
      .getAllByRole('heading', { level: 3 })
      .map((h) => (h.textContent ?? '').trim()),
    'one engine, named by its id as sent, in the reply order',
  ).toEqual(['llama.cpp'])
  expect(
    rows(on[1]),
    'the engine rows, in order: Version, then the backends by display name',
  ).toEqual([
    [en('about-version'), '1:1.0.0+pai2'],
    [en('backend-cuda'), '13.5.0+nccl.2.32.3'],
    [en('backend-rocm'), '10.1.0'],
    [en('backend-vulkan'), '1.4.363.1'],
  ])
  expect(
    ddClasses(on[1]).map(([muted, mono]) => [muted, mono]),
    'every version is a mono value, not a muted one',
  ).toEqual([
    [false, true],
    [false, true],
    [false, true],
    [false, true],
  ])
})

test('13 N.8: two engines give two h3 and two lists, in the reply order; a backend the strings do not know shows the name it was sent with', async () => {
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
  expect(
    within(main)
      .getAllByRole('heading', { level: 3 })
      .map((h) => (h.textContent ?? '').trim()),
    'the engines in the reply order',
  ).toEqual(['llama.cpp', 'other.cpp'])
  const on = lists(main)
  expect(on.length, 'one list per engine').toBe(3)
  expect(rows(on[1]), 'the first engine').toEqual([
    [en('about-version'), '1:1.0.0+pai2'],
    [en('backend-cuda'), '13.5.0'],
  ])
  expect(rows(on[2]), 'the second, its unknown backend named as it is').toEqual(
    [
      [en('about-version'), '0.9.0'],
      ['opencl', '7.7'],
    ],
  )
})

test('13 N.8: the page is text: its only focusable element is the h1 the page took the focus on', async () => {
  const { main, h1 } = await open()
  await settled(main)
  await waitFor(() =>
    expect(document.activeElement, 'the h1 keeps it').toBe(h1),
  )
  fireEvent.click(within(main).getByText(NODE.name))
  expect(document.activeElement, 'a click on the host name takes nothing').toBe(
    h1,
  )
  const focusable = Array.from(
    main.querySelectorAll('a, button, input, select, textarea, [tabindex]'),
  )
  expect(
    focusable.map((el) => el.tagName),
    'no link, no control, and the h1 as the only tabindex',
  ).toEqual(['H1'])
})

// ── 13 N.9: unknown and empty ───────────────────────────────────────────────

const UNKNOWN_VERSIONS: Array<[string, unknown]> = [
  ['null', null],
  ['the empty string', ''],
  ['a number', 7],
]

test.each(UNKNOWN_VERSIONS)(
  '13 N.9: a framework, engine or backend version of %s reads Unknown in a muted dd, not a mono one',
  async (_what, version) => {
    const { main } = await open({
      node: nodeWith(
        [
          {
            engine: 'llama.cpp',
            version,
            backends: [{ name: 'cuda', version }],
          },
        ],
        // The framework's own version, the same.
        { version: version as string | null },
      ),
    })
    await settled(main)
    const on = lists(main)
    expect(rows(on[0]), 'the framework row reads Unknown').toEqual([
      [en('app-name'), en('about-version-unknown')],
    ])
    expect(ddClasses(on[0]), 'muted, and not mono').toEqual([[true, false]])
    expect(
      rows(on[1]),
      'the engine version, and the backend, the same way',
    ).toEqual([
      [en('about-version'), en('about-version-unknown')],
      [en('backend-cuda'), en('about-version-unknown')],
    ])
    expect(ddClasses(on[1]), 'both muted, neither mono').toEqual([
      [true, false],
      [true, false],
    ])
  },
)

test('13 N.9: engines [] shows the framework list, one paragraph saying no engine was found, and no h3', async () => {
  const { main } = await open({ node: nodeWith([]) })
  await settled(main)
  expect(lists(main).length, 'the framework list only').toBe(1)
  expect(rows(lists(main)[0]), 'the framework row').toEqual([
    [en('app-name'), NODE.version as string],
  ])
  expect(
    within(main).queryAllByRole('heading', { level: 3 }).length,
    'no engine heading',
  ).toBe(0)
  const p = within(main).getByText(en('about-no-engine'))
  expect(p.tagName, 'a paragraph').toBe('P')
  expect(
    within(main).getAllByRole('paragraph').length,
    'the words are there once, alone',
  ).toBe(1)
})

test('13 N.9: an engine with no backends, sent as [] or left out, shows the GPU backends row reading None, CPU only, in a muted dd', async () => {
  for (const [what, engine] of [
    [
      'backends []',
      { engine: 'llama.cpp', version: '1:1.0.0+pai2', backends: [] },
    ],
    ['no backends field', { engine: 'llama.cpp', version: '1:1.0.0+pai2' }],
  ] as Array<[string, NodeEngine]>) {
    const { main } = await open({ node: nodeWith([engine]) })
    await settled(main)
    const on = lists(main)
    expect(
      rows(on[1]),
      `${what}: the Version row, then the backends row`,
    ).toEqual([
      [en('about-version'), '1:1.0.0+pai2'],
      [en('about-backends'), en('about-backends-none')],
    ])
    expect(
      ddClasses(on[1]),
      `${what}: the version mono, the backends value muted and not mono`,
    ).toEqual([
      [false, true],
      [true, false],
    ])
    cleanup()
    vi.unstubAllGlobals()
  }
})

// ── 13 N.10: the failures ───────────────────────────────────────────────────

test('13 N.10: a page read answering 401 signs the app out: the sign-in page at #/about, its notice, the focus on its h1', async () => {
  const { bff } = await open({
    node: () => reply(401, { key: 'session-required', args: [] }),
  })
  expect(bff.to('GET', '/api/node').length, 'one page read, no retry').toBe(2)
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
  expect(window.location.hash, 'the place is kept').toBe('#/about')
})

const ABOUT_FAILURES: Array<[string, () => Response | Promise<Response>]> = [
  [
    '403 session-not-administrator',
    () => reply(403, { key: 'session-not-administrator', args: [] }),
  ],
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
  ['a name that is null', () => reply(200, { ...NODE, name: null })],
  [
    'engines that is not an array',
    () => reply(200, { ...NODE, engines: 'no' }),
  ],
]

test.each(ABOUT_FAILURES)(
  '13 N.10: a page read answering %s shows the failure words as a plain paragraph in place of the lists, and keeps the h1',
  async (_what, node) => {
    const { main, h1 } = await open({ node })
    const p = await within(main).findByText(en('about-failed'))
    expect(p.tagName, 'a paragraph').toBe('P')
    expect(p.closest('[role="alert"]'), 'not an alert: nobody acted').toBeNull()
    expect(p.getAttribute('role'), 'no role at all').toBeNull()
    expect(lists(main).length, 'no list in place of the versions').toBe(0)
    expect(
      within(main).queryAllByRole('heading', { level: 2 }).length,
      'no host section',
    ).toBe(0)
    expect(
      within(main).queryByText(en('about-loading')),
      'the loading text is gone',
    ).toBeNull()
    expect(
      within(main).getByRole('heading', { level: 1, name: en('about-title') }),
      'the h1 is kept',
    ).toBe(h1)
  },
)

test('13 N.10: inside a good reply an engine without a name, and a backend without a name, are left out; the rest of the page shows', async () => {
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
    { engine: 'other.cpp', version: '0.9.0', backends: [] },
  ]
  const { main } = await open({ node: nodeWith(engines) })
  await settled(main)
  expect(
    within(main)
      .getAllByRole('heading', { level: 3 })
      .map((h) => (h.textContent ?? '').trim()),
    'the engine with no name is gone, the two others stay in order',
  ).toEqual(['llama.cpp', 'other.cpp'])
  const on = lists(main)
  expect(rows(on[1]), 'the backend with no name is gone').toEqual([
    [en('about-version'), '1:1.0.0+pai2'],
    [en('backend-cuda'), '13.5.0'],
  ])
  expect(
    rows(on[2]),
    'the engine with no backends shows its backends row',
  ).toEqual([
    [en('about-version'), '0.9.0'],
    [en('about-backends'), en('about-backends-none')],
  ])
  expect(
    hostHeadings(main),
    'the host section is there, with the framework row',
  ).toEqual([NODE.name])
  expect(rows(on[0]), 'the framework row is untouched').toEqual([
    [en('app-name'), NODE.version as string],
  ])
})

test('13 N.12: the About page uses no class beside the ones of the lists: assets, mono and muted', async () => {
  const { main } = await open()
  await settled(main)
  const classes = new Set<string>()
  for (const el of Array.from(main.querySelectorAll('*')))
    for (const c of Array.from(el.classList)) classes.add(c)
  expect(
    [...classes].sort(),
    'the classes of the page (the h1 of a page carries none; R1.8 freezes the sheet)',
  ).toEqual(['assets', 'mono', 'muted'].filter((c) => classes.has(c)).sort())
})

// ── 13 N.12, the one rule the sheet gains ───────────────────────────────────
//
// jsdom applies no CSS, so the rule the brief names is read off index.css as
// text, the way HearthTilesBoot.test.tsx and Reflow.test.ts read it (comments
// dropped, the same rule regex, the nesting depth from the braces before the
// rule, prop() the last value of a property in a block); those files export no
// helper, and importing a test file would register its tests here, so the
// reader is a small local copy.

const CSS = readFileSync(
  join(import.meta.dirname, 'index.css'),
  'utf8',
).replace(/\/\*[\s\S]*?\*\//g, '')

// The top-level rules: selector as written, selector with whitespace dropped,
// body.
const TOP = [...CSS.matchAll(/([^{}]+)\{([^{}]*)\}/g)].map((m) => ({
  sel: (m[1] ?? '').trim(),
  compact: (m[1] ?? '').replace(/\s+/g, ''),
  body: m[2] ?? '',
  depth:
    (CSS.slice(0, m.index ?? 0).match(/\{/g) ?? []).length -
    (CSS.slice(0, m.index ?? 0).match(/\}/g) ?? []).length,
}))

// The value of a declaration; the last one when a block repeats the property.
function prop(body: string, name: string): string {
  let val = ''
  for (const d of body.split(';')) {
    const i = d.indexOf(':')
    if (i >= 0 && d.slice(0, i).trim() === name) val = d.slice(i + 1).trim()
  }
  return val
}

test('13 N.12: the sheet gains exactly the one rule of the brief, .assets + h3, and it declares only the gap above the engine heading', () => {
  const rules = TOP.filter((r) => r.depth === 0 && r.compact === '.assets+h3')
  expect(rules, 'the rule, once, at the top level').toHaveLength(1)
  const rule = rules[0]!
  expect(
    rule.sel.replace(/\s+/g, ' '),
    'the selector as the brief writes it',
  ).toBe('.assets + h3')
  expect(prop(rule.body, 'margin-top'), 'the gap of the list above').toBe(
    'var(--space-6)',
  )
  expect(
    rule.body
      .split(';')
      .map((d) => d.slice(0, d.indexOf(':')).trim())
      .filter((name) => name !== '' && !name.startsWith('--')),
    'the rule adds the gap and nothing else on the heading',
  ).toEqual(['margin-top'])
})
