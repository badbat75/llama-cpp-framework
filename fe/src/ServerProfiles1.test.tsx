// Brief 16, slice 17a (fe/briefs/16-server-profiles-and-instances.md): the
// Server profiles list page and the nav's entry for it. The whole app renders
// against a fake bff (mocked fetch) answering GET /api/profiles/server with
// [{"id", "engine", "port", "tags", "instance"}] (crates/bff/README.md, "The
// server profile pool"), errors as {"key", "args"}. Test names cite the brief
// first, `16 17a.4`, because brief 17's UX pass owns the bare `17a.n` names.
// The profile's own page is ServerProfilePage1.test.tsx; the skeletons are
// U17aSkeletons.test.tsx; what jsdom cannot see (the sideways scroll at 320 px
// and at 200 % text) is the ux check's `server-profiles` state (17a.11), whose
// stylesheet half is pinned here.
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
  arrived,
  before,
  deferred,
  en,
  fakeBff,
  isHidden,
  quiet,
  reply,
  startAt,
  type BffOptions,
  type ServerProfile,
} from './test/bff.ts'
import { serverProfileHref } from './route.ts'

beforeEach(() => startAt('#/profiles/server'))

afterEach(() => {
  cleanup()
  vi.useRealTimers()
  vi.unstubAllGlobals()
  startAt('')
})

// ── Helpers ─────────────────────────────────────────────────────────────

/** One row of GET /api/profiles/server. */
function row(
  id: string,
  port = 8580,
  tags: string[] = [],
  instance: string | null = 'default',
): ServerProfile {
  return { id, engine: 'llama.cpp', port, tags, instance }
}

/** The two rows most tests read: one an instance runs, one free. */
const ROWS = [
  row('default', 8580, ['edge', 'v2'], 'default'),
  row('qwen-27b', 9931, [], null),
]

/** The app at #/profiles/server; returns the fake bff and main. */
function open(options: BffOptions = {}) {
  const bff = fakeBff(options)
  render(<App />)
  return { bff, main: screen.getByRole('main') }
}

/** The app at a hash of the caller's, on the list page's file. */
function openAt(hash: string, options: BffOptions = {}) {
  startAt(hash)
  return open(options)
}

function nav(): HTMLElement {
  return screen.getByRole('navigation', { name: en('nav-label') })
}

/** The Profiles group's links: text, href, aria-current, first child. */
function profileLinks() {
  const ul = nav().querySelector('ul[aria-labelledby="nav-profiles"]')
  expect(
    ul,
    'the Profiles group is a list labelled nav-profiles',
  ).not.toBeNull()
  return [...ul!.querySelectorAll<HTMLAnchorElement>('a')].map((a) => ({
    text: (a.textContent ?? '').trim(),
    href: a.getAttribute('href'),
    current: a.getAttribute('aria-current'),
    icon: a.firstElementChild,
  }))
}

/** The names of the elements Tab reaches, in order. */
function tabbableNames(): string[] {
  return [
    ...document.querySelectorAll<HTMLElement>(
      'a[href], button, input, select, textarea, [tabindex]',
    ),
  ]
    .filter(
      (el) =>
        !(el as HTMLButtonElement).disabled &&
        el.getAttribute('tabindex') !== '-1' &&
        !isHidden(el),
    )
    .map((el) => (el.textContent ?? '').replace(/\s+/g, ' ').trim())
}

/** The table, once the read answered. */
function tableOf(main: HTMLElement): Promise<HTMLElement> {
  return within(main).findByRole('table')
}

/** The data rows of `t`, as the cells' text. */
/** The rows of the table: the cells' text per row. 22 D4.1 puts a control in
 *  the Used by cell: an icon button carries no text of its own, and its reason
 *  (22 D4.2) is out of sight, so the text of the buttons in a cell is not part
 *  of what the row reads. */
function rowsOf(t: HTMLElement): string[][] {
  return [...t.querySelectorAll('tbody tr')].map((row) =>
    [...row.querySelectorAll('td')].map((c) => {
      const clone = c.cloneNode(true) as HTMLElement
      for (const b of clone.querySelectorAll('button')) b.remove()
      return (clone.textContent ?? '').trim()
    }),
  )
}

/** The cells of the row whose first cell holds `id`. */
function cellsOf(main: HTMLElement, id: string): HTMLElement[] {
  const t = within(main).getByRole('table')
  const row = [...t.querySelectorAll('tbody tr')].find((r) =>
    (r.querySelector('td')?.textContent ?? '').includes(id),
  )
  expect(row, `a row for ${id}`).not.toBeUndefined()
  return [...row!.querySelectorAll('td')]
}

/** The list reads the bff was asked for, in order. */
function listReads(bff: ReturnType<typeof fakeBff>) {
  return bff.to('GET', '/api/profiles/server')
}

// ── 17a.1: the route ────────────────────────────────────────────────────

test('16 17a.1: #/profiles/server is the list page: its h1 is server-profiles-title, takes the focus on arrival, the document title is the page, and it sends one GET /api/profiles/server', async () => {
  const { bff, main } = open({ serverProfiles: ROWS })
  const h1 = await within(main).findByRole('heading', {
    level: 1,
    name: en('server-profiles-title'),
  })
  // The focus and the document title are written by the heading's effects,
  // which run after the render that put the h1 in the tree: an async finder
  // resolves on that render, so both are waited for (arrived).
  await arrived(h1, en('page-title', en('server-profiles-title')))
  expect(h1.getAttribute('tabindex'), 'so it is focusable').toBe('-1')
  await quiet(bff)
  expect(listReads(bff).length, 'one read of the pool').toBe(1)
})

test('16 17a.1: signed out, #/profiles/server is the sign-in page with signin-required, and the pool is not read', async () => {
  const { bff, main } = open({
    session: 'signed-out',
    serverProfiles: ROWS,
  })
  // The session read resolves after the render: wait for the sign-in page,
  // then the assertions are of a page that has settled.
  expect(
    await within(main).findByText(en('signin-required')),
    'the notice that says why',
  ).toBeTruthy()
  expect(
    within(main).getByRole('textbox', { name: en('signin-username') }),
    'the sign-in form is the whole of main',
  ).toBeTruthy()
  expect(
    within(main).queryByRole('heading', {
      level: 1,
      name: en('server-profiles-title'),
    }),
    'the list page is not there',
  ).toBeNull()
  expect(listReads(bff), 'nothing is read').toHaveLength(0)
})

test('16 17a.1: the ids . and .. of a server profile show server-profile-not-found with the id, the h1 and the back link, and send no read at all', async () => {
  for (const [hash, id] of [
    ['#/profiles/server/.', '.'],
    ['#/profiles/server/..', '..'],
    ['#/profiles/server/%2E%2E', '..'],
  ] as const) {
    const { bff, main } = openAt(hash, {
      serverProfile: () => reply(200, {}),
    })
    const shown = await within(main).findByText(
      en('server-profile-not-found', id),
    )
    const h1 = within(main).getByRole('heading', { level: 1, name: id })
    // The heading's effect writes the document title after the render that
    // put the h1 in the tree, so it is waited for.
    await waitFor(() => expect(document.title, hash).toBe(en('page-title', id)))
    const back = within(main).getByRole('link', {
      name: en('server-profile-back'),
    })
    expect(back.getAttribute('href'), hash).toBe('#/profiles/server')
    expect(before(back, h1), `${hash}: the back link is before the h1`).toBe(
      true,
    )
    expect(before(h1, shown), `${hash}: the sentence is below the h1`).toBe(
      true,
    )
    expect(listReads(bff), `${hash}: not even the pool`).toHaveLength(0)
    expect(
      bff.calls.filter((c) => c.path.startsWith('/api/profiles/server/')),
      `${hash}: no read of a profile`,
    ).toHaveLength(0)
    cleanup()
    vi.unstubAllGlobals()
  }
})

test('16 17a.1: a bad escape in the id is the not-found page, and nothing is read', async () => {
  const { bff, main } = openAt('#/profiles/server/%zz', {
    serverProfile: () => reply(200, {}),
  })
  const h1 = await within(main).findByRole('heading', {
    level: 1,
    name: en('not-found'),
  })
  expect(document.title).toBe(en('page-title', en('not-found')))
  expect(document.activeElement, 'the not-found h1 takes the focus').toBe(h1)
  expect(
    bff.calls.filter((c) => c.path.startsWith('/api/profiles/server/')),
  ).toHaveLength(0)
})

test('16 17a.1: serverProfileHref is the profile page of an id, the id percent-encoded', () => {
  expect(serverProfileHref('default')).toBe('#/profiles/server/default')
  expect(serverProfileHref('qwen-27b')).toBe('#/profiles/server/qwen-27b')
  expect(
    serverProfileHref('a b/c?d'),
    'a name with the characters a path carries is encoded',
  ).toBe('#/profiles/server/a%20b%2Fc%3Fd')
})

// ── 17a.2: the nav ──────────────────────────────────────────────────────

test('16 17a.2: the Profiles group holds Server profiles then Model profiles, the first with the server icon and, on the list, aria-current page', async () => {
  const { main } = open({ serverProfiles: ROWS })
  await within(main).findByRole('heading', {
    level: 1,
    name: en('server-profiles-title'),
  })
  const links = profileLinks()
  expect(
    links.map((l) => [l.text, l.href]),
    'the two entries, in the order of decision 1',
  ).toEqual([
    [en('nav-server-profiles'), '#/profiles/server'],
    [en('nav-model-profiles'), '#/profiles/model'],
  ])
  const server = links[0]!
  expect(server.current, 'on the list page').toBe('page')
  const icon = server.icon
  expect(icon, 'the entry opens with its icon').not.toBeNull()
  expect(icon!.tagName, 'an svg').toBe('svg')
  expect(
    icon!.classList.contains('icon'),
    'the icon class, as every nav icon',
  ).toBe(true)
  expect(icon!.getAttribute('aria-hidden'), 'decorative').toBe('true')
  expect(
    links[1]!.current,
    'Model profiles keeps 2d.64: nothing here',
  ).toBeNull()
})

test('16 17a.2: on a profile page the Server profiles entry is aria-current true; on Overview and Model profiles it is nothing', async () => {
  for (const [hash, current] of [
    ['#/profiles/server/default', 'true'],
    ['#/', null],
    ['#/profiles/model', null],
  ] as const) {
    startAt(hash)
    open({ serverProfiles: ROWS, serverProfile: () => reply(200, {}) })
    const links = await waitForLinks()
    expect(links[0]!.current, `${hash}: Server profiles`).toBe(current)
    expect(
      links[1]!.current,
      `${hash}: Model profiles keeps its rule (2d.64)`,
    ).toBe(hash === '#/profiles/model' ? 'page' : null)
    cleanup()
    vi.unstubAllGlobals()
  }
})

/** The Profiles group's links, once the nav arrived with the session. */
async function waitForLinks() {
  await screen.findByRole('navigation', { name: en('nav-label') })
  return profileLinks()
}

test('16 17a.2: at 1280 px with two instances the Tab path is the shell, Overview, the instances, Node info, Server profiles, Model profiles, About', async () => {
  const { main } = open({ serverProfiles: ROWS })
  await within(main).findByRole('heading', {
    level: 1,
    name: en('server-profiles-title'),
  })
  // The nav's instances land when the shell's list read answers, a moment
  // after the page heading: settle the nav before taking the Tab path.
  await within(nav()).findByRole('link', { name: 'coder' })
  expect(
    tabbableNames().slice(0, 12),
    'the shell in 20 N.10 order with Server profiles added before Model profiles',
  ).toEqual([
    en('skip-to-content'),
    en('app-name'),
    en('nav-menu'),
    en('theme-dark'),
    en('session-sign-out'),
    en('nav-overview'),
    'default',
    'coder',
    en('nav-node'),
    en('nav-server-profiles'),
    en('nav-model-profiles'),
    en('nav-about'),
  ])
})

test('16 17a.2: the Menu button opens the nav, and a click on Server profiles closes the open menu (20 N.10)', async () => {
  const { main } = open({ serverProfiles: ROWS })
  await within(main).findByRole('heading', {
    level: 1,
    name: en('server-profiles-title'),
  })
  const button = screen.getByRole('button', { name: en('nav-menu') })
  expect(button.getAttribute('aria-expanded'), 'closed at first').toBe('false')
  fireEvent.click(button)
  expect(button.getAttribute('aria-expanded'), 'the click opens it').toBe(
    'true',
  )
  const link = nav().querySelector<HTMLAnchorElement>(
    'ul[aria-labelledby="nav-profiles"] a[href="#/profiles/server"]',
  )!
  expect(link, 'the entry is in the open menu').toBeTruthy()
  fireEvent.click(link)
  expect(
    button.getAttribute('aria-expanded'),
    'following the entry closes the menu',
  ).toBe('false')
})

// ── 17a.3: the read and the table ───────────────────────────────────────

test('16 17a.3, 22 D4.1: the table has the four columns in order, one row per item in the reply order, the id a link to its page, the port as text, the tags joined by ", ", the instance a link, and a free profile says No instance muted', async () => {
  const { main } = open({ serverProfiles: ROWS })
  const t = await tableOf(main)
  expect(
    [...t.querySelectorAll('thead th')].map((th) =>
      (th.textContent ?? '').trim(),
    ),
    'the columns, in the brief order',
  ).toEqual([
    en('server-profiles-column-name'),
    en('server-profiles-column-port'),
    en('server-profiles-column-tags'),
    en('server-profiles-column-used-by'),
  ])
  expect(rowsOf(t), 'the rows, in the reply order').toEqual([
    ['default', '8580', 'edge, v2', 'default'],
    ['qwen-27b', '9931', '', en('server-profiles-unused')],
  ])
  const first = cellsOf(main, 'default')
  const idLink = within(first[0]!).getByRole('link', { name: 'default' })
  expect(idLink.getAttribute('href'), 'the id links to the profile page').toBe(
    serverProfileHref('default'),
  )
  const used = within(first[3]!).getByRole('link', { name: 'default' })
  expect(used.getAttribute('href'), 'the instance links to its page').toBe(
    '#/instances/default',
  )
  const free = cellsOf(main, 'qwen-27b')
  expect(
    within(free[3]!).queryAllByRole('link'),
    'a free profile is not a link',
  ).toHaveLength(0)
  expect(
    within(free[3]!).getByText(en('server-profiles-unused')).className,
    'the word sits in the muted style, as a tile with no tag',
  ).toContain('muted')
  // 22 D4.1: the row's minus follows the value in the same Used by cell, on
  // the in-use row too (there it is switched off, 22 D4.2).
  const inUse = cellsOf(main, 'default')
  const minus = within(inUse[3]!).getByRole('button', {
    name: en('server-profiles-delete-row', 'default'),
  })
  expect(minus.closest('td'), 'the minus is in the Used by cell').toBe(inUse[3])
  expect(before(used, minus), 'and after the value, never before it').toBe(true)
})

test('16 17a.3: the columns do not sort: no header holds a control', async () => {
  const { main } = open({ serverProfiles: ROWS })
  const t = await tableOf(main)
  for (const th of [...t.querySelectorAll('thead th')])
    expect(
      th.querySelectorAll('button, input, select, a').length,
      'a plain header cell',
    ).toBe(0)
})

test('16 17a.3: the tags cell is text, not chips, and shows every tag whole', async () => {
  const { main } = open({
    serverProfiles: [row('default', 8580, ['edge', 'v2', 'gpu'], 'default')],
  })
  await tableOf(main)
  const cell = cellsOf(main, 'default')[2]!
  expect(cell.textContent, 'the join, in the reply order').toBe('edge, v2, gpu')
  expect(
    cell.querySelectorAll('button, [role="button"], .chip').length,
    'no chip: the tags are text that wraps',
  ).toBe(0)
})

test('16 17a.3: the table stands in the scrolling box, and the h1, the intro and the table are in that order', async () => {
  const { main } = open({ serverProfiles: ROWS })
  const t = await tableOf(main)
  const box = t.closest('div.table-scroll')
  expect(
    box,
    'the table is inside div.table-scroll (fe/DESIGN.md Table)',
  ).not.toBeNull()
  const h1 = within(main).getByRole('heading', {
    level: 1,
    name: en('server-profiles-title'),
  })
  const intro = within(main).getByText(en('server-profiles-intro'))
  expect(before(h1, intro), 'the intro after the h1').toBe(true)
  expect(before(intro, box!), 'the table after the intro').toBe(true)
})

test('16 17a.11 as its 17a review D1 amends it: the table carries the class server-profiles, the one the sheet styles so its columns stop breaking, and it stays in the box that scrolls', async () => {
  const { main } = open({ serverProfiles: ROWS })
  const t = await tableOf(main)
  expect(
    t.classList.contains('server-profiles'),
    "the class 17a.11 gains in the review (beside .model-profiles): the declarations are pinned in Reflow.test.ts, the scroll in the ux check's server-profiles state",
  ).toBe(true)
  expect(
    t.closest('div.table-scroll'),
    'the wide table scrolls inside its box, so the page never scrolls sideways',
  ).not.toBeNull()
})

test('16 17a.3: while the read is open the loading text is a role=status kept out of sight, and the table is not there', async () => {
  const gate = deferred()
  const { main } = open({ serverProfiles: () => gate.promise })
  // Wait for the list page to mount: before that the status on the page is
  // the boot skeleton's, not the pool read's.
  await within(main).findByRole('heading', {
    level: 1,
    name: en('server-profiles-title'),
  })
  // 22 D4.6 adds the delete region's line above the table, so the page holds
  // more than one status: this one is the one that says what the read is doing.
  const status = [...main.querySelectorAll('[role="status"]')].find((el) =>
    (el.textContent ?? '').includes(en('server-profiles-loading')),
  )
  expect(status, 'the read says what it is doing at once').not.toBeNull()
  expect(status!.textContent, "the pool's own words").toContain(
    en('server-profiles-loading'),
  )
  expect(
    status!.classList.contains('visually-hidden'),
    'and is out of sight, as U17a rules',
  ).toBe(true)
  expect(within(main).queryByRole('table'), 'no table yet').toBeNull()
  gate.resolve(reply(200, ROWS))
})

test('16 17a.3: [] shows server-profiles-empty and no table', async () => {
  const { main } = open({ serverProfiles: [] })
  expect(
    await within(main).findByText(en('server-profiles-empty')),
    'the empty page says so',
  ).toBeTruthy()
  expect(within(main).queryByRole('table'), 'no table').toBeNull()
  expect(
    within(main).getByRole('heading', {
      level: 1,
      name: en('server-profiles-title'),
    }),
    'the page is the page',
  ).toBeTruthy()
})

// ── 17a.4: the failures, and the answers that are not quite right ───────

test('16 17a.4: a 401 signs the app out at the same hash with signin-session-ended, and the table is gone', async () => {
  const { main } = open({
    serverProfiles: () => reply(401, { key: 'session-required' }),
  })
  expect(
    await within(main).findByText(en('signin-session-ended')),
    'the page says the session ended',
  ).toBeTruthy()
  expect(within(main).queryByRole('table'), 'in place of the table').toBeNull()
  expect(
    within(main).queryByRole('heading', {
      level: 1,
      name: en('server-profiles-title'),
    }),
    'the list page is gone',
  ).toBeNull()
  expect(window.location.hash, 'at the same address').toBe('#/profiles/server')
})

test('16 17a.4: a 403 session-not-administrator signs the app out with session-revoked, with no table', async () => {
  const { main } = open({
    serverProfiles: () =>
      reply(403, { key: 'session-not-administrator', args: [] }),
  })
  expect(await screen.findByText(en('session-revoked'))).toBeTruthy()
  expect(within(main).queryByRole('table'), 'in place of the table').toBeNull()
})

test('16 17a.4: 503 client-unavailable shows api-client-unavailable, and 500, a network failure and a reply that is not an array show server-profiles-failed, each a plain paragraph in place of the table', async () => {
  const cases: [string, () => Response | Promise<Response>][] = [
    [
      en('api-client-unavailable'),
      () => reply(503, { key: 'client-unavailable', args: [] }),
    ],
    [
      en('server-profiles-failed'),
      () => reply(500, { key: 'client-other', args: [] }),
    ],
    [
      // The reviewer's note, pinned the way 17a.4 rules it: the list's read maps
      // 503 client-unavailable and nothing beside it, so every other key shows
      // the generic sentence. `client-not-authorized` is the 502 the instance's
      // model-profiles read maps to api-client-not-authorized (17e.2), not this
      // one, and section 0 does not list it among this route's answers.
      en('server-profiles-failed'),
      () => reply(502, { key: 'client-not-authorized', args: [] }),
    ],
    [
      en('server-profiles-failed'),
      () => Promise.reject(new TypeError('network down')),
    ],
    [en('server-profiles-failed'), () => reply(200, { key: 'nope', args: [] })],
  ]
  for (const [text, answer] of cases) {
    const { main } = open({ serverProfiles: () => answer() })
    const shown = await within(main).findByText(text)
    expect(shown.tagName, `${text}: a paragraph`).toBe('P')
    expect(within(main).queryByRole('table'), `${text}: no table`).toBeNull()
    cleanup()
    vi.unstubAllGlobals()
  }
})

test('16 17a.4: an item with no readable id is left out; a non-number port reads as 0, a non-array tags as none with its non-strings dropped, a non-string instance as none', async () => {
  const { main } = open({
    serverProfiles: [
      {
        id: 'ok',
        engine: 'llama.cpp',
        port: 8580,
        tags: ['edge', 7, null, 'v2'],
        instance: 'default',
      },
      { port: 8580, tags: [], instance: 'default' },
      { id: 5, engine: 'llama.cpp', port: 8580, tags: [], instance: null },
      { id: '', engine: 'llama.cpp', port: 8580, tags: [], instance: null },
      { id: 'bad', engine: 'llama.cpp', port: 'x', tags: 'edge', instance: 7 },
    ] as unknown as ServerProfile[],
  })
  const t = await tableOf(main)
  expect(rowsOf(t), 'the rows the page can read, in the reply order').toEqual([
    ['ok', '8580', 'edge, v2', 'default'],
    // 17a.4: an id that is a non-empty string is kept; port 'x' reads as 0,
    // tags 'edge' (not an array) as none, instance 7 (not a string) as none.
    ['bad', '0', '', 'No instance'],
  ])
})

// ── 17a.11: what the sheet must keep (the layout is the ux check's) ─────

/**
 * index.css with its comments dropped, the reader Reflow.test.ts uses: 17a.11
 * rules no token, and its 17a review D1 adds exactly one class
 * (`server-profiles`), so every class the page renders has to be one the sheet
 * declares: that is the pin, and the rules of the new class are pinned there.
 */
const CSS = readFileSync(
  join(import.meta.dirname, 'index.css'),
  'utf8',
).replace(/\/\*[\s\S]*?\*\//g, '')

/** Every rule of the sheet: its selector list and its body. */
const RULES = [...CSS.matchAll(/([^{}]+)\{([^{}]*)\}/g)].map((m) => ({
  sel: m[1].trim(),
  body: m[2],
}))

/** The values of `property` in every rule whose selector list is `selector`. */
function propsOf(selector: RegExp, property: string): string[] {
  return RULES.filter((r) => selector.test(r.sel))
    .map((r) =>
      [
        ...r.body.matchAll(
          new RegExp(`(?:^|;)\\s*${property}:\\s*([^;]*)`, 'g'),
        ),
      ].map((m) => m[1]!.trim()),
    )
    .flat()
}

test('16 17a.11: the list page renders no class the stylesheet does not declare, and the box it needs is the one that scrolls', async () => {
  const { main } = open({ serverProfiles: ROWS })
  const t = await tableOf(main)
  const declared = new Set(
    [...CSS.matchAll(/\.([a-z][a-z0-9-]*)/g)].map((m) => m[1]!),
  )
  const used = new Set<string>()
  for (const el of main.querySelectorAll('*'))
    for (const c of el.classList) used.add(c)
  expect(
    [...used].filter((c) => !declared.has(c)),
    'no new class: every class the page renders is one the sheet declares (17a.11, its 17a review D1 included: `server-profiles` is styled there)',
  ).toEqual([])
  expect(
    propsOf(/(^|,)\s*\.table-scroll\s*(,|$)/, 'overflow-x'),
    'the box scrolls sideways, so the page never does (10 R1.3)',
  ).toContain('auto')
  expect(t.closest('div.table-scroll'), 'and the table is in it').not.toBeNull()
  expect(
    propsOf(/(^|,)\s*td\s*(,|$)/, 'overflow-wrap'),
    'a cell breaks a word with no break point, so a long tag and a long id wrap',
  ).toContain('anywhere')
})
