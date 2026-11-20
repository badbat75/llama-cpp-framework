// Slice 13, the host in the navigation (fe/briefs/13-node-about.md, 13 N.3,
// 13 N.4, 13 N.6, 13 N.11): the shell reads GET /api/node once per session and
// the host name becomes the label over the instance links; About is the nav's
// last entry, in a list of its own, with the info icon; the tab order and the
// closed menu hold. "13 N.3" is the brief's criterion 3.
//
// The fake answers GET /api/node only when a test asks for it (brief section 6),
// so every other suite keeps the fallback label "Instances".
import {
  cleanup,
  fireEvent,
  render,
  screen,
  within,
} from '@testing-library/react'
import { afterEach, beforeEach, expect, test, vi } from 'vitest'
import { StrictMode } from 'react'
import App from './App.tsx'
import {
  MODEL_PROFILES,
  NODE,
  en,
  fakeBff,
  held,
  isHidden,
  navigate,
  reply,
  startAt,
  stubDialog,
  type BffOptions,
} from './test/bff.ts'

let dialogs: ReturnType<typeof stubDialog>

beforeEach(() => {
  startAt('#/')
  dialogs = stubDialog()
})

afterEach(() => {
  dialogs.restore()
  cleanup()
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

/** The Menu button. */
const menuButton = (): HTMLElement =>
  screen.getByRole('button', { name: en('nav-menu') })

/** How many times the shell read the host. */
const nodeReads = (bff: ReturnType<typeof fakeBff>): number =>
  bff.to('GET', '/api/node').length

/** The app, signed in, with the nav and its instance links on screen. */
async function open(options: BffOptions = {}) {
  const bff = fakeBff(options)
  render(<App />)
  const nav = await navOf()
  await within(nav).findByRole('link', { name: 'default' })
  return { bff, nav }
}

/** The nav's list that holds the link named `id`, and the element naming it. */
async function instanceList(
  nav: HTMLElement,
  id = 'default',
): Promise<{ list: HTMLUListElement; label: HTMLElement | null }> {
  const link = await within(nav).findByRole('link', { name: id })
  const list = link.closest('ul')
  expect(list, 'the instance links sit in a list').not.toBeNull()
  const named = list!.getAttribute('aria-labelledby')
  return {
    list: list as HTMLUListElement,
    label: named ? document.getElementById(named) : null,
  }
}

/** The words over the instance links. */
async function hostLabel(nav: HTMLElement): Promise<string> {
  const { label } = await instanceList(nav)
  expect(label, 'the instance list is named by an element').not.toBeNull()
  return (label?.textContent ?? '').trim()
}

/** The sign-in page of `main`, once it is there. */
async function signInPage(main: HTMLElement): Promise<HTMLElement> {
  await within(main).findByRole('heading', {
    level: 1,
    name: en('signin-title'),
  })
  return main
}

/** Fills the sign-in form of `scope` as `user` and submits it. */
async function signIn(scope: HTMLElement, user: string) {
  const name = await within(scope).findByRole('textbox', {
    name: en('signin-username'),
  })
  fireEvent.change(name, { target: { value: user } })
  fireEvent.change(
    within(scope).getByLabelText(en('signin-password'), { selector: 'input' }),
    { target: { value: 'hunter2' } },
  )
  fireEvent.click(
    within(scope).getByRole('button', { name: en('signin-submit') }),
  )
}

/** The sign-in of a POST body, as the app sends it (session.ts). */
const asPosted = (init?: RequestInit): Response =>
  reply(200, {
    user: String(
      (JSON.parse(String(init?.body ?? '{}')) as { username?: string })
        .username ?? '',
    ),
  })

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

/** aria-labelledby, then a label, then the text. */
function nameOf(el: HTMLElement): string {
  const by = el.getAttribute('aria-labelledby')
  const text = by
    ? by
        .split(/\s+/)
        .map((id) => document.getElementById(id)?.textContent ?? '')
        .join(' ')
    : el instanceof HTMLInputElement && (el.labels?.length ?? 0) > 0
      ? [...el.labels!].map((l) => l.textContent).join(' ')
      : el.textContent
  return (text ?? '').replace(/\s+/g, ' ').trim()
}

// ── 13 N.3: one read of the host per session ────────────────────────────────

test('13 N.3: a session at the start sends exactly one GET /api/node, and moving through the Overview, an instance, Model profiles, About and back sends no other from the shell', async () => {
  const { bff } = await open({ node: NODE })
  expect(nodeReads(bff), 'the session brings one read').toBe(1)
  // The reads the shell owes, counting the About page's own read of every
  // arrival at `#/about` (13 N.7): the shell adds nothing at any page (13 N.3).
  let owed = 1
  for (const hash of [
    '#/instances/default',
    '#/profiles/model',
    '#/about',
    '#/',
    '#/instances/coder',
    '#/',
  ]) {
    navigate(hash)
    await screen.findByRole('main')
    if (hash === '#/about') owed += 1
    expect(
      nodeReads(bff),
      `no shell read for ${hash}: About's page reads once of its own`,
    ).toBe(owed)
  }
})

test('13 N.3: signed out nothing is read about the host; a sign-in on the sign-in page sends exactly one', async () => {
  const bff = fakeBff({ node: NODE, session: 'signed-out', signIn: asPosted })
  render(<App />)
  const main = await signInPage(screen.getByRole('main'))
  expect(nodeReads(bff), 'nothing about the host signed out').toBe(0)
  await signIn(main, 'alice')
  const nav = await navOf()
  await within(nav).findByRole('link', { name: 'default' })
  expect(nodeReads(bff), 'the sign-in starts a session: one read').toBe(1)
})

test('13 N.3: a sign-out and a sign-in as another user send one more', async () => {
  const bff = fakeBff({ node: NODE, session: 'signed-out', signIn: asPosted })
  render(<App />)
  await signIn(await signInPage(screen.getByRole('main')), 'alice')
  const nav = await navOf()
  await within(nav).findByRole('link', { name: 'default' })
  expect(nodeReads(bff), "alice's session reads once").toBe(1)
  fireEvent.click(
    await screen.findByRole('button', { name: en('session-sign-out') }),
  )
  await signInPage(screen.getByRole('main'))
  expect(nodeReads(bff), 'the sign-out reads nothing about the host').toBe(1)
  await signIn(screen.getByRole('main'), 'bob')
  await navOf()
  expect(nodeReads(bff), "bob's session reads again").toBe(2)
})

test('13 N.3: a sign-in in a page dialog as the same user sends no shell read; as another user it sends one', async () => {
  // The dialog: a page's write answering 401 leaves a Sign in on the page
  // (fe/briefs/09-signin-only.md S1b.6); the visitor signs in there, with no
  // page change.
  for (const [user, want] of [
    ['alice', 1],
    ['bob', 2],
  ] as Array<[string, number]>) {
    startAt('#/profiles/model')
    const bff = fakeBff({
      node: NODE,
      signIn: asPosted,
      modelProfileSwitch: () =>
        reply(401, { key: 'session-required', args: [] }),
    })
    render(<App />)
    const nav = await navOf()
    await within(nav).findByRole('link', { name: 'default' })
    const main = screen.getByRole('main')
    const box = await within(main).findByRole('switch', {
      name: MODEL_PROFILES[0]!.id,
    })
    expect(nodeReads(bff), `${user}: the session read`).toBe(1)
    fireEvent.click(box)
    const ask = await within(main).findByRole('button', {
      name: en('session-sign-in'),
    })
    fireEvent.click(ask)
    const dialog = screen.getByRole('dialog', { name: en('signin-title') })
    await signIn(dialog, user)
    await within(main).findByRole('switch', { name: MODEL_PROFILES[0]!.id })
    expect(
      nodeReads(bff),
      `${user}: a dialog sign-in as ${user} ${want === 1 ? 'keeps the one read' : 'is a new session'}`,
    ).toBe(want)
    cleanup()
    vi.unstubAllGlobals()
  }
})

// The three shapes of 13 N.3 the review of the built slice (2026-11-20) found
// untested: the tree main.tsx really renders, StrictMode included, where every
// effect runs twice; a sign-out and a sign-in as the same user, which 13 N.3
// calls a new session; and a dialog sign-in as the same user while the shell's
// read is still open, which costs no read and must not drop the answer of the
// read already on its way.

test('13 N.3: under StrictMode, the tree main.tsx renders, the shell sends exactly one GET /api/node and both labels show the host', async () => {
  const bff = fakeBff({ node: NODE })
  render(
    <StrictMode>
      <App />
    </StrictMode>,
  )
  const nav = await navOf()
  await within(nav).findByRole('link', { name: 'default' })
  expect(await hostLabel(nav), 'the nav label carries the name').toBe(NODE.name)
  expect(
    (await screen.findByRole('heading', { level: 2, name: NODE.name }))
      .textContent,
    'the Overview h2 carries it too',
  ).toBe(NODE.name)
  expect(nodeReads(bff), 'the double invoke costs no second read').toBe(1)
})

test('13 N.3: a sign-out then a sign-in as the same user is a new session: one more read, the labels at their fallback until it answers, the host after', async () => {
  let read = 0
  const second = held()
  const bff = fakeBff({
    session: 'signed-out',
    signIn: asPosted,
    node: (init) => (++read === 1 ? reply(200, NODE) : second.answer(init)),
  })
  render(<App />)
  await signIn(await signInPage(screen.getByRole('main')), 'alice')
  let nav = await navOf()
  expect(await hostLabel(nav), "alice's first session reads the host").toBe(
    NODE.name,
  )
  fireEvent.click(
    await screen.findByRole('button', { name: en('session-sign-out') }),
  )
  await signInPage(screen.getByRole('main'))
  expect(nodeReads(bff), 'the sign-out reads nothing about the host').toBe(1)
  await signIn(screen.getByRole('main'), 'alice')
  nav = await navOf()
  expect(
    nodeReads(bff),
    'the same user signing back in starts a session: one more read',
  ).toBe(2)
  expect(
    await hostLabel(nav),
    "the old session's name does not carry over: the fallback while the new read is open",
  ).toBe(en('nav-instances'))
  second.resolve(reply(200, NODE))
  expect(await hostLabel(nav), 'and the host once it answers').toBe(NODE.name)
})

test('13 N.3: a sign-in in a page dialog as the same user while the host read is open sends no second read, and the name of the read already on its way arrives', async () => {
  // The dialog: a page write answering 401 (S1b.6), signed in as the user of
  // the session that is already there.
  startAt('#/profiles/model')
  const h = held()
  const bff = fakeBff({
    node: h.answer,
    signIn: asPosted,
    modelProfileSwitch: () => reply(401, { key: 'session-required', args: [] }),
  })
  render(<App />)
  const nav = await navOf()
  const main = screen.getByRole('main')
  const box = await within(main).findByRole('switch', {
    name: MODEL_PROFILES[0]!.id,
  })
  expect(await hostLabel(nav), 'the fallback while the read is open').toBe(
    en('nav-instances'),
  )
  fireEvent.click(box)
  fireEvent.click(
    await within(main).findByRole('button', { name: en('session-sign-in') }),
  )
  await signIn(
    screen.getByRole('dialog', { name: en('signin-title') }),
    'alice',
  )
  await within(main).findByRole('switch', { name: MODEL_PROFILES[0]!.id })
  expect(nodeReads(bff), 'the same user in a dialog is not a new session').toBe(
    1,
  )
  h.resolve(reply(200, NODE))
  expect(
    await hostLabel(nav),
    'the read that was already on its way names the nav: the sign-in drops nothing',
  ).toBe(NODE.name)
})

// Every way the host read can go wrong. None of them ends the session.
const NODE_FAILURES: Array<[string, () => Response | Promise<Response>]> = [
  [
    '401 session-required',
    () => reply(401, { key: 'session-required', args: [] }),
  ],
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
  ['a name that is not a string', () => reply(200, { name: 7, engines: [] })],
  ['a name that is empty', () => reply(200, { name: '', engines: [] })],
  ['engines that is not an array', () => reply(200, { name: 'gpu-box' })],
]

test.each(NODE_FAILURES)(
  '13 N.3: a host read answering %s signs nobody out, keeps the page, and leaves the labels at their fallback',
  async (what, node) => {
    const { bff, nav } = await open({ node })
    expect(
      await screen.findByRole('button', { name: en('session-sign-out') }),
      `${what}: the session lives, the banner offers Sign out`,
    ).toBeTruthy()
    expect(
      within(screen.getByRole('main')).queryByRole('textbox', {
        name: en('signin-username'),
      }),
      `${what}: no sign-in form`,
    ).toBeNull()
    expect(await hostLabel(nav), `${what}: the nav label`).toBe(
      en('nav-instances'),
    )
    expect(
      (
        await screen.findByRole('heading', {
          level: 2,
          name: en('instances-title'),
        })
      ).textContent,
      `${what}: the Overview h2 keeps its fallback`,
    ).toBe(en('instances-title'))
    expect(nodeReads(bff), `${what}: one read, no retry`).toBe(1)
  },
)

// ── 13 N.4: the host names the instance group ───────────────────────────────

test('13 N.4: once the read answered, the label over the instance links is the host and the list accessible name is it, the links and their order unchanged', async () => {
  const { nav } = await open({ node: NODE })
  expect(await hostLabel(nav), 'the name as sent').toBe(NODE.name)
  const { list, label } = await instanceList(nav)
  expect(label?.classList.contains('nav-label'), 'a p.nav-label').toBe(true)
  expect(label?.tagName, 'the label stays a paragraph, not a heading').toBe('P')
  expect(list.getAttribute('aria-labelledby'), 'the list is named by it').toBe(
    label?.id,
  )
  expect(list.getAttribute('aria-label'), 'not by an aria-label').toBeNull()
  expect(
    Array.from(list.querySelectorAll('a')).map((a) => a.textContent?.trim()),
    'the links are the instances, in the order of the list',
  ).toEqual(['default', 'coder'])
  expect(
    Array.from(list.querySelectorAll('a')).map((a) => a.getAttribute('href')),
    'their addresses are the instance pages',
  ).toEqual(['#/instances/default', '#/instances/coder'])
  expect(
    within(nav).getAllByRole('list', { name: NODE.name }).length,
    'one list carries the host name: the instance group',
  ).toBe(1)
})

test('13 N.4: the label is text, no link, no icon, no button, and the nav holds no disclosure of its own', async () => {
  const { nav } = await open({ node: NODE })
  const { label } = await instanceList(nav)
  expect(label?.querySelector('a'), 'the host is not a link').toBeNull()
  expect(label?.querySelector('button'), 'not a button').toBeNull()
  expect(label?.querySelector('svg'), 'no icon beside it').toBeNull()
  expect(
    nav.querySelectorAll('button').length,
    'no button in the nav: one host has nothing to fold',
  ).toBe(0)
  expect(nav.querySelectorAll('[aria-expanded]').length, 'no disclosure').toBe(
    0,
  )
})

test('13 N.4: before the answer the label is "Instances", and the same list and links carry it; when the answer arrives the label moves', async () => {
  const h = held()
  const { nav } = await open({ node: h.answer })
  const before = await instanceList(nav)
  expect(
    (before.label?.textContent ?? '').trim(),
    'the fallback while the read is open',
  ).toBe(en('nav-instances'))
  h.resolve(reply(200, NODE))
  // The host name lands in the nav label and, on the Overview, in the h2 over
  // the tiles: the nav is the place this test watches.
  await within(nav).findByText(NODE.name, {}, { timeout: 2_000 })
  const after = await instanceList(nav)
  expect(after.list, 'the same list element').toBe(before.list)
  expect(
    Array.from(after.list.querySelectorAll('a')).map((a) =>
      a.textContent?.trim(),
    ),
    'the same links',
  ).toEqual(['default', 'coder'])
  expect(after.label, 'the same label element').toBe(before.label)
  expect(
    (after.label?.textContent ?? '').trim(),
    'the words moved to the host',
  ).toBe(NODE.name)
})

test('13 N.4: no instances, no label and no instance list, host name or not', async () => {
  // `open` waits for an instance link, and there is none here: the nav arrives
  // with the session, and the Overview with its own read.
  fakeBff({ node: NODE, instances: [] })
  render(<App />)
  const nav = await navOf()
  await screen.findByRole('heading', {
    level: 1,
    name: en('overview-title'),
  })
  expect(
    within(nav).queryByText(NODE.name),
    'no host label over an empty list',
  ).toBeNull()
  expect(
    within(nav).queryByText(en('nav-instances')),
    'no fallback label either',
  ).toBeNull()
  expect(
    navLinks(nav).map((a) => a.textContent?.trim()),
    'Overview, the model profiles, About',
  ).toEqual([en('nav-overview'), en('nav-model-profiles'), en('nav-about')])
})

// ── 13 N.6: the About entry ─────────────────────────────────────────────────

test('13 N.6: signed in, the nav last item is the About link to #/about, in a list of its own after the Profiles list, with the info icon first', async () => {
  const { nav } = await open({ node: NODE })
  const groups = Array.from(nav.querySelectorAll(':scope > ul'))
  const about = groups[groups.length - 1]
  expect(about, 'About sits in a list of its own').not.toBeNull()
  expect(
    about.hasAttribute('aria-labelledby'),
    'and the list has no label: About is not a group of many',
  ).toBe(false)
  const links = Array.from(about.querySelectorAll('a'))
  expect(
    links.map((a) => a.textContent?.trim()),
    'one link, named About',
  ).toEqual([en('nav-about')])
  expect(links[0]?.getAttribute('href'), 'its address').toBe('#/about')
  const icon = links[0]?.firstElementChild
  expect(icon?.tagName, 'the info icon is the first thing in the link').toBe(
    'svg',
  )
  expect(icon?.classList.contains('icon'), 'the icon the sheet sizes').toBe(
    true,
  )
  expect(icon?.getAttribute('aria-hidden'), 'the icon is decorative').toBe(
    'true',
  )
  expect(links[0]?.getAttribute('aria-label'), 'named by its text').toBeNull()
  const profiles = await within(nav).findByRole('list', {
    name: en('nav-profiles'),
  })
  expect(
    profiles.compareDocumentPosition(about) & Node.DOCUMENT_POSITION_FOLLOWING,
    'the About list comes after the Profiles group',
  ).toBeTruthy()
  expect(
    navLinks(nav).at(-1)?.textContent?.trim(),
    'About is the last thing the nav reads',
  ).toBe(en('nav-about'))
})

test('13 N.6: the About link has aria-current="page" on #/about and none on the other pages', async () => {
  const { nav } = await open({ node: NODE })
  const link = () => within(nav).getByRole('link', { name: en('nav-about') })
  expect(link().hasAttribute('aria-current'), 'not on the Overview').toBe(false)
  for (const hash of ['#/instances/default', '#/profiles/model']) {
    navigate(hash)
    await screen.findByRole('main')
    expect(link().hasAttribute('aria-current'), `not on ${hash}`).toBe(false)
  }
  navigate('#/about')
  await screen.findByRole('heading', { level: 1, name: en('about-title') })
  expect(link().getAttribute('aria-current'), 'on its own page').toBe('page')
})

test('13 N.6: signed out at #/about there is no nav, so no About entry', async () => {
  startAt('#/about')
  const bff = fakeBff({ node: NODE, session: 'signed-out' })
  render(<App />)
  const main = await signInPage(screen.getByRole('main'))
  expect(
    within(main).getByText(en('signin-required')),
    'the notice',
  ).toBeTruthy()
  expect(screen.queryByRole('navigation'), 'no nav signed out').toBeNull()
  expect(nodeReads(bff), 'and no read signed out').toBe(0)
})

// ── 13 N.11: the order, and the menu ────────────────────────────────────────

test('13 N.11: at #/about the Tab path is the shell and its links with About last, and the page adds no stop of its own', async () => {
  const { nav } = await open({ node: NODE })
  navigate('#/about')
  const h1 = await screen.findByRole('heading', {
    level: 1,
    name: en('about-title'),
  })
  expect(document.activeElement, 'the page takes the focus on its h1').toBe(h1)
  expect(
    tabbable().map(nameOf),
    'the shell order, About the last stop',
  ).toEqual([
    en('skip-to-content'),
    en('app-name'),
    en('nav-menu'),
    en('theme-dark'),
    en('session-sign-out'),
    en('nav-overview'),
    'default',
    'coder',
    en('nav-model-profiles'),
    en('nav-about'),
  ])
  expect(
    tabbable().filter((el) => nameOf(el) === en('nav-about')).length,
    'About is one stop: the page does not repeat it',
  ).toBe(1)
  expect(
    nav.contains(document.activeElement as Node),
    'the focus is not in the nav',
  ).toBe(false)
})

test('13 N.11: the Menu button opens the nav, its links read in the same order with About last, and a click on About closes it', async () => {
  const { nav } = await open({ node: NODE })
  const button = menuButton()
  expect(button.getAttribute('aria-expanded'), 'closed at first').toBe('false')
  fireEvent.click(button)
  expect(button.getAttribute('aria-expanded'), 'the click opens it').toBe(
    'true',
  )
  expect(
    nav.classList.contains('open'),
    'the region carries the open class',
  ).toBe(true)
  expect(
    navLinks(nav).map((a) => a.textContent?.trim()),
    'the same links, in the same order, About last',
  ).toEqual([
    en('nav-overview'),
    'default',
    'coder',
    en('nav-model-profiles'),
    en('nav-about'),
  ])
  const about = navLinks(nav).at(-1)!
  expect(about.textContent?.trim(), 'the last link is About').toBe(
    en('nav-about'),
  )
  expect(about.getAttribute('href'), 'and it opens the About page').toBe(
    '#/about',
  )
  fireEvent.click(about)
  // jsdom follows no anchor href on a click, so the address is pinned by the
  // href above and the arrival by the navigation below.
  expect(button.getAttribute('aria-expanded'), 'the link closed the menu').toBe(
    'false',
  )
  navigate('#/about')
  await screen.findByRole('heading', {
    level: 1,
    name: en('about-title'),
  })
})
