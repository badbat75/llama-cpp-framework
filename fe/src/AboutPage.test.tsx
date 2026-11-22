// fe/src/AboutPage.test.tsx
// Slice 18 AN5a (fe/briefs/18-about-node-info.md, 18 AN.2, 18 AN.3, 18 AN.4,
// with 13 N.7's one read per arrival and 13 N.12's class rules): the About page
// at #/about is two lines, the framework's name and version and the operating
// system, read from GET /api/version. The engines and the GPUs live on the Node
// info page now (NodePage.test.tsx). "18 AN.2" is the brief's criterion 2.
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
  OS,
  reply,
  startAt,
  VERSION_HOST,
  type BffOptions,
  type VersionBody,
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

/** GET /api/version with these fields over the reply of brief 18 section 0. */
function versionWith(over: Partial<VersionBody>): Response {
  return reply(200, { ...VERSION_HOST, ...over })
}

/** The app at `#/about`. The default answers the host, the version and the
 * operating system, so the shell's label and the page's lines both land. */
async function open(over: BffOptions = {}) {
  const bff = fakeBff({ version: VERSION_HOST, node: NODE, ...over })
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
 * The page's read has landed: a value of the list is on screen. The h1 is
 * there while the read is open, so a test that reads the rows has to wait for
 * the reply, the way a visitor waits for the words.
 */
async function settled(main: HTMLElement): Promise<void> {
  await within(main).findByText(VERSION_HOST.version as string)
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
  const { main } = await open({ version: h.answer })
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
  h.resolve(versionWith({}))
})

test('18 AN.2: arriving at #/about sends one GET /api/version of the page own, beside the shell one; leaving and coming back sends another, staying sends none', async () => {
  startAt('#/')
  const bff = fakeBff({ version: VERSION_HOST, node: NODE })
  render(<App />)
  await screen.findByRole('heading', { level: 1, name: en('overview-title') })
  // What the app read before the page ever appeared: the shell's read (AN.15)
  // and the Overview's own version line.
  const reads = () => bff.to('GET', '/api/version').length
  const start = reads()
  expect(start, 'the shell reads once, and the Overview has its own line').toBe(
    2,
  )
  navigate('#/about')
  await screen.findByRole('heading', { level: 1, name: en('about-title') })
  expect(reads(), 'the page reads once on arrival').toBe(start + 1)
  navigate('#/')
  await screen.findByRole('heading', { level: 1, name: en('overview-title') })
  expect(reads(), 'the Overview sends its own one, the About page none').toBe(
    start + 2,
  )
  navigate('#/about')
  await screen.findByRole('heading', { level: 1, name: en('about-title') })
  expect(reads(), 'the page reads again').toBe(start + 3)
  fireEvent.click(
    screen.getByRole('heading', { level: 1, name: en('about-title') }),
  )
  expect(reads(), 'no read for a click').toBe(start + 3)
})

// ── 18 AN.2 and AN.3: the two lines, read off GET /api/version ──────────────

test('18 AN.2: the reply of section 0 shows one list, class assets, of exactly two rows: the app name with the version, and the operating system joined', async () => {
  const { main } = await open()
  await settled(main)
  const on = lists(main)
  expect(on.length, 'one list, alone').toBe(1)
  expect(
    on[0]!.classList.contains('assets'),
    'the Value list, class assets, no new class',
  ).toBe(true)
  expect(rows(on[0]!), 'the two rows, in this order').toEqual([
    [en('app-name'), VERSION_HOST.version as string],
    [en('system-os'), `${OS.name} ${OS.version}`],
  ])
  expect(
    ddClasses(on[0]!)[0],
    'the version a mono value, not a muted one (13 N.8)',
  ).toEqual([false, true])
  expect(
    within(main).queryAllByRole('heading', { level: 2 }).length,
    'no section heading: the engines and the GPUs are the node page',
  ).toBe(0)
  expect(
    within(main).queryAllByRole('heading', { level: 3 }).length,
    'no engine heading',
  ).toBe(0)
})

/** The operating system answers of AN.3, and the text the row must show. */
const OS_ANSWERS: Array<[string, VersionBody['os'], string]> = [
  [
    'os Ubuntu and version 26.04',
    { name: 'Ubuntu', version: '26.04', kernel: 'x' },
    'Ubuntu 26.04',
  ],
  [
    'os Ubuntu and version null',
    { name: 'Ubuntu', version: null, kernel: 'x' },
    'Ubuntu',
  ],
  [
    'os null and version 26.04',
    { name: null, version: '26.04', kernel: 'x' },
    '26.04',
  ],
  [
    'os and version null',
    { name: null, version: null, kernel: 'x' },
    en('about-version-unknown'),
  ],
  ['the whole os block null', null, en('about-version-unknown')],
]

test.each(OS_ANSWERS)(
  '18 AN.3: the operating system row of %s',
  async (what, os, shown) => {
    const { main } = await open({ version: versionWith({ os }) })
    const p = await within(main).findByText(shown)
    expect(p.tagName, `the row of ${what} shows "${shown}" as one text`).toBe(
      'DD',
    )
    const on = lists(main)[0]!
    expect(rows(on).length, 'the two rows stay').toBe(2)
    expect(rows(on)[1], 'the operating system row').toEqual([
      en('system-os'),
      shown,
    ])
    expect(
      ddClasses(on)[1],
      shown === en('about-version-unknown')
        ? 'Unknown is muted, not mono'
        : 'a value, not muted',
    ).toEqual(
      shown === en('about-version-unknown') ? [true, false] : [false, true],
    )
  },
)

// The unknown version answers of 13 N.9, on the framework line.
const UNKNOWN_VERSIONS: Array<[string, unknown]> = [
  ['null', null],
  ['the empty string', ''],
  ['a number', 7],
]

test.each(UNKNOWN_VERSIONS)(
  '18 AN.4: the version of %s shows the framework line reading Unknown, muted, the operating system line kept',
  async (_what, version) => {
    const { main } = await open({
      version: versionWith({ version: version as string | null }),
    })
    await within(main).findByText(en('about-version-unknown'))
    const on = lists(main)[0]!
    expect(rows(on), 'the framework line reads Unknown').toEqual([
      [en('app-name'), en('about-version-unknown')],
      [en('system-os'), `${OS.name} ${OS.version}`],
    ])
    expect(
      ddClasses(on),
      'Unknown muted and not mono, the operating system kept mono',
    ).toEqual([
      [true, false],
      [false, true],
    ])
  },
)

test('18 AN.2: the page is text: its only focusable element is the h1 the page took the focus on', async () => {
  const { main, h1 } = await open()
  await settled(main)
  await waitFor(() =>
    expect(document.activeElement, 'the h1 keeps it').toBe(h1),
  )
  fireEvent.click(within(main).getByText(`${OS.name} ${OS.version}`))
  expect(
    document.activeElement,
    'a click on the operating system line takes nothing',
  ).toBe(h1)
  const focusable = Array.from(
    main.querySelectorAll('a, button, input, select, textarea, [tabindex]'),
  )
  expect(
    focusable.map((el) => el.tagName),
    'no link, no control, and the h1 as the only tabindex',
  ).toEqual(['H1'])
})

// ── 18 AN.4: the failures ───────────────────────────────────────────────────

test('18 AN.4: a version read answering 401 signs the app out: the sign-in page at #/about, its notice, the focus on its h1', async () => {
  const { bff } = await open({
    version: () => reply(401, { key: 'session-required', args: [] }),
  })
  expect(
    bff.to('GET', '/api/version').length,
    'the page read once, beside the shell one of the session (18 AN.15), and it does not retry a 401',
  ).toBe(2)
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

test('18 AN.4: a version read answering 403 session-not-administrator ends the session as every read does: the sign-in page with the revoked notice', async () => {
  const { bff } = await open({
    version: () => reply(403, { key: 'session-not-administrator', args: [] }),
  })
  expect(
    bff.to('GET', '/api/version').length,
    'the page read once, beside the shell one of the session (18 AN.15), and it does not retry a 403',
  ).toBe(2)
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

const VERSION_FAILURES: Array<[string, () => Response | Promise<Response>]> = [
  ['500', () => reply(500, 'a manager detail')],
  [
    '503 client-unavailable',
    () => reply(503, { key: 'client-unavailable', args: [] }),
  ],
  ['a network failure', () => Promise.reject(new TypeError('failed to fetch'))],
  ['a body that is not json', () => reply(200, 'not json')],
  ['a body that is not an object', () => reply(200, [])],
]

test.each(VERSION_FAILURES)(
  '18 AN.4: a version read answering %s shows the failure words as a plain paragraph in place of both lines, and keeps the h1',
  async (_what, version) => {
    const { main, h1 } = await open({ version })
    const p = await within(main).findByText(en('about-failed'))
    expect(p.tagName, 'a paragraph').toBe('P')
    expect(p.closest('[role="alert"]'), 'not an alert: nobody acted').toBeNull()
    expect(p.getAttribute('role'), 'no role at all').toBeNull()
    expect(lists(main).length, 'no list in place of the lines').toBe(0)
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

// ── 13 N.12: the classes, and the one rule the sheet gains ──────────────────

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
