// fe/src/NodesMenu1.test.tsx
// Brief 20, slice N1 (fe/briefs/20-nodes-menu.md), criteria 20 N.1 to 20 N.6:
// the four strings keys of section 2, the reader's `serverProfile`, the nav's
// Nodes group, the host label with its `computer` icon, the instance links
// carrying their server profile, and the group when there is no instance to
// show. 20 N.7 to 20 N.12 are in NodesMenu2.test.tsx.
//
// The app is rendered whole against the fake bff, as the shell's suites do. The
// 20 N.2 reader is pinned through the nav link that shows the profile and
// through the Overview tiles that must not, because that is what the browser
// shows: the tiles never carry the profile (20 N.2), so the link is its only
// face, and the other fields of the reader are pinned by Instances.test.tsx.
// jsdom does no layout: the two rules of 20 N.11, which are what makes a long
// id and profile wrap inside the pill, are pinned in the stylesheet text in
// NodesMenu2, and their effect at 320 px is the designer's UX pass (brief
// section 6).
import {
  act,
  cleanup,
  render,
  screen,
  waitFor,
  within,
} from '@testing-library/react'
import { afterEach, beforeEach, expect, test, vi } from 'vitest'
import App from './App.tsx'
import strings from './strings/en_US.json'
import {
  HOST_FAILURES,
  VERSION_HOST,
  en,
  fakeBff,
  held,
  isHidden,
  navigate,
  reply,
  startAt,
  type BffOptions,
  type Instance,
} from './test/bff.ts'

beforeEach(() => startAt('#/'))

afterEach(() => {
  cleanup()
  vi.useRealTimers()
  vi.unstubAllGlobals()
  startAt('')
})

// ── Helpers ─────────────────────────────────────────────────────────────────

/** The nav region, once it is there. */
const navOf = async (): Promise<HTMLElement> =>
  (await screen.findByRole('navigation', {
    name: en('nav-label'),
  })) as HTMLElement

/** The nav's links, in the order the document holds them: the tab order. */
const navLinks = (nav: HTMLElement): HTMLAnchorElement[] =>
  Array.from(nav.querySelectorAll('a'))

/** The names the nav's links read, in that order. */
const navNames = (nav: HTMLElement): string[] =>
  navLinks(nav).map((a) => (a.textContent ?? '').replace(/\s+/g, ' ').trim())

const byId = (id: string): HTMLElement | null => document.getElementById(id)

/** The nav, read synchronously (the suites that advance a fake clock). */
const navEl = (): HTMLElement | null =>
  document.querySelector('nav[aria-label]') as HTMLElement | null

/** The app, signed in, with the nav and its instance links on screen. The nav
 * arrives with the session and the links with the list read, so the helper
 * waits for one. It waits on the address, not on the name: a fixture that
 * carries a server profile names its link "id, server profile x" (20 N.5), and
 * a name of `default` matches nothing there (brief section 5). */
async function open(options: BffOptions = {}) {
  const bff = fakeBff(options)
  render(<App />)
  const nav = await navOf()
  await waitFor(() =>
    expect(
      nav.querySelector(
        'ul[aria-labelledby="nav-host"] a[href^="#/instances/"]',
      ),
    ).not.toBeNull(),
  )
  return { bff, nav }
}

/** The words of an element, whitespace collapsed. */
const words = (el: Element | null): string =>
  (el?.textContent ?? '').replace(/\s+/g, ' ').trim()

/** The text an eye reads: every text node outside a `.visually-hidden` run. */
function shown(root: Node): string {
  let out = ''
  for (const node of Array.from(root.childNodes)) {
    if (node.nodeType === Node.TEXT_NODE) out += node.textContent ?? ''
    else if (
      node.nodeType === Node.ELEMENT_NODE &&
      !(node as Element).matches('.visually-hidden')
    )
      out += shown(node)
  }
  return out.replace(/\s+/g, ' ').trim()
}

/** The children of a node, each as its tag, or `#text` and its text. */
const childrenAsText = (root: Node): string[] =>
  Array.from(root.childNodes).map((node) =>
    node.nodeType === Node.TEXT_NODE
      ? `#text ${JSON.stringify(node.textContent ?? '')}`
      : `${node.nodeName.toLowerCase()} ${JSON.stringify(
          words(node as Element),
        )}`,
  )

/** The focusable elements, in document order, each named as Tab names it. */
function tabbable(): HTMLElement[] {
  return [
    ...document.querySelectorAll<HTMLElement>(
      'a[href], button, input, select, textarea, [tabindex]',
    ),
  ].filter(
    (el) =>
      !(el as HTMLButtonElement).disabled &&
      el.getAttribute('tabindex') !== '-1' &&
      !isHidden(el) &&
      el.closest('dialog:not([open])') === null,
  )
}

/** Fake timers for the idle poll; setImmediate and microtasks stay real. */
function fakeClock(): void {
  vi.useFakeTimers({
    toFake: [
      'setTimeout',
      'clearTimeout',
      'setInterval',
      'clearInterval',
      'Date',
    ],
  })
}

/** Lets every answer and render that is due now land; the clock stays. */
async function flush(): Promise<void> {
  await act(async () => {
    for (let i = 0; i < 20; i++) await vi.advanceTimersByTimeAsync(0)
  })
}

async function advance(ms: number): Promise<void> {
  await act(async () => {
    await vi.advanceTimersByTimeAsync(ms)
  })
  await flush()
}

// ── 20 N.1: the keys of section 2 ───────────────────────────────────────────

test('20 N.1: en_US.json gains the four keys of section 2 with that English, and loses nav-instances', () => {
  const table = strings as Record<string, string>
  // The English of section 2, typed from the brief.
  //
  // 16 17f.11 takes the two profile keys of that section out again: 17f.2 makes
  // the nav entry the name alone, so nothing shows a server profile in the
  // menu. Their retirement is pinned in InstanceName17f.test.tsx.
  const SECTION_2: Array<[string, string]> = [
    ['nav-nodes', 'Nodes'],
    ['nav-this-node', 'This node'],
  ]
  expect(
    SECTION_2.filter(([key, english]) => table[key] !== english).map(
      ([key]) => key,
    ),
    'the keys the file gains, each with the English of section 2',
  ).toEqual([])
  expect(
    table['nav-instances'],
    'nav-instances is gone: the nav was its only user (App.tsx 618)',
  ).toBeUndefined()
  // The keys the nav keeps from briefs 13 and 18 (section 2: "reused,
  // unchanged"), and the Overview h2's fallback.
  for (const key of [
    'nav-label',
    'nav-overview',
    'nav-profiles',
    'nav-server-profiles',
    'nav-model-profiles',
    'nav-node',
    'nav-about',
    'instances-title',
  ])
    expect(table[key], `${key} is kept`).toBeTruthy()
})

// ── 20 N.2: the reader ──────────────────────────────────────────────────────

test('20 N.2 (as 16 17f.2 amends it): no serverProfile shape reaches the nav link, and the tiles never show the profile', async () => {
  // 20 N.2 pinned the reader through the nav link, the profile's only face.
  // 16 17f.2 takes that face away: the entry is the name alone, so a
  // serverProfile of any shape must read as nothing there. The reader's own
  // fields are pinned by Instances.test.tsx.
  const withProfile = (
    id: string,
    serverProfile: string | null | number | undefined,
  ): Instance => ({
    id,
    engine: 'llama.cpp',
    state: 'active',
    ...(serverProfile === undefined ? {} : { serverProfile }),
  })
  fakeBff({
    version: VERSION_HOST,
    instances: [
      withProfile('with', 'qwen-27b'),
      withProfile('nully', null),
      withProfile('empty', ''),
      withProfile('missing', undefined),
      withProfile('numeric', 7),
    ],
  })
  render(<App />)
  const nav = await navOf()
  // The links arrive with the list read, after the nav does, so each is
  // awaited by name.
  for (const id of ['with', 'nully', 'empty', 'missing', 'numeric'])
    expect(
      shown(await within(nav).findByRole('link', { name: id })),
      `${id}: the id alone, whatever the serverProfile`,
    ).toBe(id)
  const main = await screen.findByRole('main')
  await within(main).findByText('with')
  expect(
    main.textContent,
    'the Overview tiles do not show the server profile (20 N.2)',
  ).not.toContain('qwen-27b')
  expect(main.textContent).not.toContain('server profile')
})

// ── 20 N.3: the nav structure ───────────────────────────────────────────────

test('20 N.3: signed in, the nav is the Overview list, the Nodes label and its list of one host holding the host label and the host list, the Profiles label and its list, and a last unlabelled list, in this order, with no button and no aria-expanded', async () => {
  const { nav } = await open({ version: VERSION_HOST })
  expect(
    Array.from(nav.children).map(
      (el) => `${el.tagName.toLowerCase()}${el.id ? `#${el.id}` : ''}`,
    ),
    'the nav children in the DOM order of 20 N.3',
  ).toEqual(['ul', 'p#nav-nodes', 'ul', 'p#nav-profiles', 'ul', 'ul'])
  const overview = nav.children[0]!
  expect(
    overview.hasAttribute('aria-labelledby'),
    'the Overview list carries no label (decision 1)',
  ).toBe(false)
  expect(
    Array.from(overview.querySelectorAll('a')).map((a) =>
      a.getAttribute('href'),
    ),
    'and holds the Overview link',
  ).toEqual(['#/'])

  const nodes = byId('nav-nodes')
  expect(nodes, 'the Nodes label exists').not.toBeNull()
  expect(nodes?.tagName, 'a paragraph, like every nav label').toBe('P')
  expect(nodes?.classList.contains('nav-label'), 'the .nav-label class').toBe(
    true,
  )
  expect(words(nodes), 'reading nav-nodes').toBe(en('nav-nodes'))
  expect(
    nodes?.querySelector('svg'),
    'the group labels stay text only (decision 7)',
  ).toBeNull()

  const nodesList = nav.querySelector('ul[aria-labelledby="nav-nodes"]')
  expect(nodesList, 'the Nodes list is named by the label').not.toBeNull()
  expect(nodesList?.contains(nodes), 'the label sits outside the list').toBe(
    false,
  )
  const hosts = Array.from(nodesList!.children)
  expect(
    hosts.map((el) => el.tagName.toLowerCase()),
    'the Nodes list holds exactly one li, the one host',
  ).toEqual(['li'])
  expect(
    Array.from(hosts[0]!.children).map(
      (el) => `${el.tagName.toLowerCase()}${el.id ? `#${el.id}` : ''}`,
    ),
    'the host li is the host label then the host list',
  ).toEqual(['p#nav-host', 'ul'])
  const hostList = hosts[0]!.querySelector('ul[aria-labelledby="nav-host"]')
  expect(hostList, 'the host list is named by the host label').not.toBeNull()
  expect(
    Array.from(hostList!.querySelectorAll('a')).map((a) =>
      a.getAttribute('href'),
    ),
    'the host list is the instance links and then Node info',
  ).toEqual(['#/instances/default', '#/instances/coder', '#/node'])

  const profiles = byId('nav-profiles')
  expect(words(profiles), 'the Profiles label reads nav-profiles').toBe(
    en('nav-profiles'),
  )
  const profilesList = nav.querySelector('ul[aria-labelledby="nav-profiles"]')
  expect(
    Array.from(profilesList?.querySelectorAll('a') ?? []).map((a) =>
      a.getAttribute('href'),
    ),
    'the Profiles list holds Server profiles then Model profiles (17a.2)',
  ).toEqual(['#/profiles/server', '#/profiles/model'])

  const top = Array.from(nav.querySelectorAll(':scope > ul'))
  const last = top[top.length - 1]!
  expect(
    last.hasAttribute('aria-labelledby'),
    'the last list has no label',
  ).toBe(false)
  expect(
    Array.from(last.querySelectorAll('a')).map((a) => a.getAttribute('href')),
    'and holds About only',
  ).toEqual(['#/about'])

  expect(
    nav.querySelectorAll('button').length,
    'no button in the nav: one host has nothing to fold (decision 2)',
  ).toBe(0)
  expect(nav.querySelectorAll('[aria-expanded]').length, 'no disclosure').toBe(
    0,
  )
})

// ── 20 N.4: the host label ──────────────────────────────────────────────────

test('20 N.4: the host label is the host name the shell read, after a first child svg.icon of computer that is decorative, its text alone naming the host list; it is text, no link, no button and no tab stop', async () => {
  const { nav } = await open({ version: VERSION_HOST })
  const host = byId('nav-host')
  expect(host, 'the host entry exists').not.toBeNull()
  expect(host?.tagName, 'a paragraph, not a heading').toBe('P')
  expect(host?.classList.contains('nav-label'), 'reusing .nav-label').toBe(true)
  const icon = host?.firstElementChild
  expect(
    host?.firstChild,
    'the icon is the FIRST child, text nodes included',
  ).toBe(icon)
  expect(icon?.tagName.toLowerCase(), 'an svg').toBe('svg')
  expect(icon?.classList.contains('icon'), 'the class the sheet sizes').toBe(
    true,
  )
  expect(icon?.getAttribute('aria-hidden'), 'decorative').toBe('true')
  expect(words(host), 'then the host name, as the read answered it').toBe(
    VERSION_HOST.name,
  )
  expect(
    host?.children.length,
    'the icon is the only element child of the label (the computer icon itself ' +
      'holds its paths and rects, so this counts children, not descendants)',
  ).toBe(1)
  expect(
    within(nav).getByRole('list', { name: VERSION_HOST.name }) ===
      nav.querySelector('ul[aria-labelledby="nav-host"]'),
    'the host list is named by the label, the icon adding nothing',
  ).toBe(true)
  expect(host?.querySelector('a'), 'not a link').toBeNull()
  expect(host?.querySelector('button'), 'not a button').toBeNull()
  expect(
    tabbable().filter((el) => host?.contains(el)).length,
    'it takes no Tab stop',
  ).toBe(0)
  expect(
    tabbable().filter((el) => el.closest('.nav-label') !== null).length,
    'no nav label takes a stop: Nodes, the host and Profiles are words',
  ).toBe(0)
})

test('20 N.4: before the host read answers the label is "This node", and when the answer lands the same list and the same links carry the host name', async () => {
  const h = held()
  const { nav } = await open({ version: h.answer })
  const host = byId('nav-host')
  expect(words(host), 'the fallback while the read is open').toBe(
    en('nav-this-node'),
  )
  expect(
    within(nav).getByRole('list', { name: en('nav-this-node') }),
    'the host list carries the fallback name too',
  ).toBeTruthy()
  const before = nav.querySelector('ul[aria-labelledby="nav-host"]')
  h.resolve(reply(200, VERSION_HOST))
  await within(nav).findByText(VERSION_HOST.name, {}, { timeout: 2_000 })
  const after = nav.querySelector('ul[aria-labelledby="nav-host"]')
  expect(after, 'the same list element').toBe(before)
  expect(words(byId('nav-host')), 'the words moved to the host').toBe(
    VERSION_HOST.name,
  )
  expect(navNames(nav), 'the links are the ones the fixture answers').toEqual([
    en('nav-overview'),
    'default',
    'coder',
    en('nav-node'),
    en('nav-server-profiles'),
    en('nav-model-profiles'),
    en('nav-about'),
  ])
})

test.each(HOST_FAILURES)(
  '20 N.4: a host read answering %s leaves the host label at "This node", signs nobody out, and sends no retry',
  async (what, version) => {
    // The model profiles page reads nothing about the host of its own, so the
    // one GET /api/version here is the shell's (18 AN.15).
    startAt('#/profiles/model')
    const bff = fakeBff({ version })
    render(<App />)
    const nav = await navOf()
    await within(nav).findByRole('link', { name: 'default' })
    expect(
      await screen.findByRole('button', { name: en('session-sign-out') }),
      `${what}: the session lives`,
    ).toBeTruthy()
    expect(
      within(screen.getByRole('main')).queryByRole('textbox', {
        name: en('signin-username'),
      }),
      `${what}: no sign-in form`,
    ).toBeNull()
    expect(words(byId('nav-host')), `${what}: the host label`).toBe(
      en('nav-this-node'),
    )
    expect(hostReads(bff), `${what}: one read, no retry`).toBe(1)
    cleanup()
    vi.unstubAllGlobals()
  },
)

/** How many times the shell read the host (18 AN.15: GET /api/version). */
const hostReads = (bff: ReturnType<typeof fakeBff>): number =>
  bff.to('GET', '/api/version').length

// ── 20 N.5: the instance links ──────────────────────────────────────────────

test('20 N.5 (as 16 17f.2 amends it): the link of an instance is a decorative box icon then a span.nav-instance holding the name alone, heard and seen as the name, with no server profile in it', async () => {
  fakeBff({
    version: VERSION_HOST,
    instances: [
      {
        id: 'default',
        engine: 'llama.cpp',
        state: 'active',
        serverProfile: 'qwen-27b',
      },
    ],
  })
  render(<App />)
  const nav = await navOf()
  const link = await within(nav).findByRole('link', { name: 'default' })
  expect(
    Array.from(link.children).map((el) => el.tagName.toLowerCase()),
    'the link holds two children: the icon, then the span',
  ).toEqual(['svg', 'span'])
  const icon = link.firstElementChild!
  expect(link.firstChild, 'the icon is the FIRST child').toBe(icon)
  expect(icon.classList.contains('icon'), 'the class the sheet sizes').toBe(
    true,
  )
  expect(icon.getAttribute('aria-hidden'), 'decorative').toBe('true')
  const span = link.querySelector('.nav-instance')
  expect(span, 'the text run is the span the sheet wraps').not.toBeNull()
  expect(
    childrenAsText(span!),
    'the span holds the name and nothing else: no reader phrase, no shown ' +
      'profile, no id run (17f.2)',
  ).toEqual(['#text "default"'])
  expect(
    span!.querySelector('.visually-hidden'),
    'nothing is hidden from the reader: the name is the whole text',
  ).toBeNull()
  expect(shown(link), 'what the eye reads').toBe('default')
})

test('20 N.5 (as 16 17f.2 amends it): the links are the instances of the list in the manager order, at the instance addresses, aria-current page on that page only, and each is the name alone, with no server profile in it', async () => {
  fakeBff({
    version: VERSION_HOST,
    instances: [
      {
        id: 'alpha',
        engine: 'llama.cpp',
        state: 'active',
        serverProfile: 'qwen-27b',
      },
      {
        id: 'beta',
        engine: 'llama.cpp',
        state: 'inactive',
        serverProfile: null,
      },
      {
        id: 'gamma',
        engine: 'llama.cpp',
        state: 'active',
        serverProfile: 'gamma',
      },
    ],
  })
  render(<App />)
  const nav = await navOf()
  const list = nav.querySelector('ul[aria-labelledby="nav-host"]')
  expect(list, 'the host list exists').not.toBeNull()
  // The links come with the list read, after the nav: wait for the last one of
  // the reply, so the order below is the whole list, not a partial one.
  await waitFor(() =>
    expect(list?.querySelector('a[href="#/instances/gamma"]')).not.toBeNull(),
  )
  const links = Array.from(list!.querySelectorAll('a')).filter(
    (a) => a.getAttribute('href') !== '#/node',
  )
  expect(
    links.map((a) => (a.getAttribute('aria-label') ?? null) === null),
    'no link is named by an aria-label: the name is the content (2.4.4)',
  ).toEqual([true, true, true])
  expect(
    links.map((a) => a.getAttribute('href')),
    'one link per instance, in the order the list answers',
  ).toEqual(['#/instances/alpha', '#/instances/beta', '#/instances/gamma'])
  expect(
    shown(links[0]!),
    'the profile is not shown even when the item has one (17f.2)',
  ).toBe('alpha')
  expect(shown(links[1]!), 'no profile: the id alone, as today').toBe('beta')
  expect(
    shown(
      within(nav).getByRole('link', {
        name: 'gamma',
      }),
    ),
    'gamma: the id alone, its profile of the same name left out',
  ).toBe('gamma')
  for (const a of links)
    expect(a.hasAttribute('aria-current'), 'none on the Overview').toBe(false)
  navigate('#/instances/beta')
  await screen.findByRole(
    'heading',
    { level: 1, name: 'beta' },
    { timeout: 2_000 },
  )
  expect(links[1]!.getAttribute('aria-current'), 'its own page').toBe('page')
  expect(links[0]!.hasAttribute('aria-current'), 'alpha: not its page').toBe(
    false,
  )
  expect(links[2]!.hasAttribute('aria-current'), 'gamma: not its page').toBe(
    false,
  )
})

// ── 20 N.6: no instances to show ────────────────────────────────────────────

/** The nav and its host list when there is no instance link to be found. */
async function noInstances(options: BffOptions): Promise<HTMLElement> {
  fakeBff({ version: VERSION_HOST, ...options })
  render(<App />)
  const nav = await navOf()
  await screen.findByRole('heading', {
    level: 1,
    name: en('overview-title'),
  })
  expect(words(byId('nav-nodes')), 'the Nodes label is there').toBe(
    en('nav-nodes'),
  )
  expect(words(byId('nav-host')), 'the host label is there').toBe(
    VERSION_HOST.name,
  )
  expect(
    within(nav).getByRole('link', { name: en('nav-node') }),
    'Node info is there',
  ).toBeTruthy()
  expect(
    nav.querySelectorAll('a[href^="#/instances/"]').length,
    'no instance link',
  ).toBe(0)
  const hostList = nav.querySelector('ul[aria-labelledby="nav-host"]')
  expect(hostList, 'the host list is there').not.toBeNull()
  expect(
    Array.from(hostList!.querySelectorAll('a')).map((a) =>
      a.getAttribute('href'),
    ),
    'and holds the Node info link only',
  ).toEqual(['#/node'])
  return nav
}

test('20 N.6: while the first list read is open, after it failed, and when it answered [] the Nodes label, the host label and Node info are there, and the host list holds the Node info link only', async () => {
  const h = held()
  const pending = await noInstances({ instances: h.answer })
  expect(
    words(byId('nav-host')),
    'while the read is open the host label is the fallback',
  ).toBe(VERSION_HOST.name)
  expect(navNames(pending)).toEqual([
    en('nav-overview'),
    en('nav-node'),
    en('nav-server-profiles'),
    en('nav-model-profiles'),
    en('nav-about'),
  ])
  cleanup()
  vi.unstubAllGlobals()
  const failed = await noInstances({
    instances: () => reply(503, { key: 'client-unavailable', args: [] }),
  })
  expect(navNames(failed)).toEqual([
    en('nav-overview'),
    en('nav-node'),
    en('nav-server-profiles'),
    en('nav-model-profiles'),
    en('nav-about'),
  ])
  cleanup()
  vi.unstubAllGlobals()
  const empty = await noInstances({ instances: [] })
  expect(navNames(empty)).toEqual([
    en('nav-overview'),
    en('nav-node'),
    en('nav-server-profiles'),
    en('nav-model-profiles'),
    en('nav-about'),
  ])
})

test('20 N.6: a later failed list read keeps the instance links of the last success', async () => {
  // The idle poll of the instance page reads the list again (1c.45a). Fake
  // timers with the short poll, and the opening settled by advancing, so the
  // query is synchronous: waitFor polls with a faked interval and never
  // resolves between the advances.
  fakeClock()
  startAt('#/instances/default')
  let calls = 0
  fakeBff({
    version: VERSION_HOST,
    instances: () => {
      calls += 1
      return calls === 1
        ? reply(200, [
            {
              id: 'default',
              engine: 'llama.cpp',
              state: 'active',
              serverProfile: 'qwen-27b',
            },
          ])
        : reply(503, { key: 'client-unavailable', args: [] })
    },
  })
  render(<App />)
  await advance(0)
  const nav = navEl()
  expect(nav, 'the nav is there').not.toBeNull()
  const link = nav!.querySelector('a[href="#/instances/default"]')
  expect(link, 'the link of the instance is there').not.toBeNull()
  // 16 17f.2: the entry is the name alone. This item has no label and no port,
  // so the name is the id, and the server profile it carries is not shown.
  expect(shown(link!), 'and is the name alone').toBe('default')
  expect(calls, 'the list is read once at opening').toBe(1)
  await advance(5_000)
  expect(calls, 'the poll read again').toBeGreaterThan(1)
  expect(
    shown(nav!.querySelector('a[href="#/instances/default"]')!),
    'the link of the last success stays, the name alone',
  ).toBe('default')
})
