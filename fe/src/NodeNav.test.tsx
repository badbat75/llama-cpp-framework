// Slice 13, the host in the navigation (fe/briefs/13-node-about.md, 13 N.3,
// 13 N.4, 13 N.6, 13 N.11), as slice 18 read it (fe/briefs/18-about-node-info.md,
// 18 AN.6, 18 AN.15) and brief 20 reorganises it (fe/briefs/20-nodes-menu.md,
// 20 N.4, N.6, N.9, N.10): the shell reads GET /api/version once per session and
// the host name becomes the label of the host's list; Node info sits at the end
// of that list, About alone in the nav's last list; the tab order and the closed
// menu hold. "13 N.3" and "18 AN.6" are the briefs' criteria.
//
// The shell's read moved off GET /api/node (AN.15), and the fake answers
// GET /api/node only when a test asks for it (brief section 6), so every other
// suite keeps the fallback label "This node" (20 N.4).
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
  HOST_FAILURES,
  VERSION_HOST,
  en,
  fakeBff,
  held,
  isHidden,
  navigate,
  quiet,
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

/** How many times the shell read the host (18 AN.15: GET /api/version). */
const hostReads = (bff: ReturnType<typeof fakeBff>): number =>
  bff.to('GET', '/api/version').length

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

// ── 13 N.3 (18 AN.15): one read of the host per session ─────────────────────

test('13 N.3: a session at the start sends exactly one GET /api/version, and moving through the Overview, an instance, Model profiles, Node info, About and back sends no other from the shell', async () => {
  const { bff } = await open({ version: VERSION_HOST })
  // The shell's read of the host is GET /api/version (18 AN.15), and the
  // Overview's version line of 13 N.10 keeps its own read beside it (AN.15).
  // Both go out from an effect that runs after the render the nav and the
  // links appear in, so the count is read once the app is quiet.
  await quiet(bff)
  expect(
    hostReads(bff),
    'the session brings the shell one and the page one',
  ).toBe(2)
  // The reads owed at each address: the shell adds nothing at any page (13 N.3),
  // and the pages that show the host read it of their own on every arrival (the
  // Overview's version line 13 N.10, About 13 N.7 and 18 AN.2). The Node info
  // page reads the manager, never the host (18 AN.8, AN.15), so it adds none.
  let owed = 2
  for (const hash of [
    '#/instances/default',
    '#/profiles/model',
    '#/node',
    '#/about',
    '#/',
    '#/instances/coder',
    '#/',
  ]) {
    navigate(hash)
    await screen.findByRole('main')
    if (hash === '#/about' || hash === '#/') owed += 1
    // A page that shows the host sends its own read from an effect after it
    // mounts, so the count is read once the app is quiet: that is also the
    // evidence the shell added nothing at this address.
    await quiet(bff)
    expect(
      hostReads(bff),
      `no shell read for ${hash}: a page that shows the host reads once of its own`,
    ).toBe(owed)
  }
})

test('13 N.3: signed out nothing is read about the host; a sign-in on the sign-in page sends exactly one', async () => {
  // The model profiles page reads nothing about the host, so every
  // GET /api/version here is the shell's.
  startAt('#/profiles/model')
  const bff = fakeBff({
    version: VERSION_HOST,
    session: 'signed-out',
    signIn: asPosted,
  })
  render(<App />)
  const main = await signInPage(screen.getByRole('main'))
  expect(hostReads(bff), 'nothing about the host signed out').toBe(0)
  await signIn(main, 'alice')
  const nav = await navOf()
  await within(nav).findByRole('link', { name: 'default' })
  await quiet(bff)
  expect(hostReads(bff), 'the sign-in starts a session: one read').toBe(1)
})

test('13 N.3: a sign-out and a sign-in as another user send one more', async () => {
  startAt('#/profiles/model')
  const bff = fakeBff({
    version: VERSION_HOST,
    session: 'signed-out',
    signIn: asPosted,
  })
  render(<App />)
  await signIn(await signInPage(screen.getByRole('main')), 'alice')
  const nav = await navOf()
  await within(nav).findByRole('link', { name: 'default' })
  await quiet(bff)
  expect(hostReads(bff), "alice's session reads once").toBe(1)
  fireEvent.click(
    await screen.findByRole('button', { name: en('session-sign-out') }),
  )
  await signInPage(screen.getByRole('main'))
  await quiet(bff)
  expect(hostReads(bff), 'the sign-out reads nothing about the host').toBe(1)
  await signIn(screen.getByRole('main'), 'bob')
  await navOf()
  // A sign-in lands on the Overview (09 S1a.9), so bob's session brings the
  // shell's read and the Overview's own one (13 N.10, 18 AN.15). Both go out
  // from an effect, so the count is read once the app is quiet.
  await quiet(bff)
  expect(hostReads(bff), "bob's session reads again").toBe(3)
})

// The three shapes of 13 N.3 the review of the built slice (2026-11-20) found
// untested: the tree main.tsx really renders, StrictMode included, where every
// effect runs twice; a sign-out and a sign-in as the same user, which 13 N.3
// calls a new session; and a dialog sign-in as the same user while the shell's
// read is still open, which costs no read and must not drop the answer of the
// read already on its way.

test('13 N.3: under StrictMode, the tree main.tsx renders, the shell sends exactly one GET /api/version and the nav label shows the host', async () => {
  // The instance page reads nothing about the host, so the count here is the
  // shell's alone. The name on the Overview's h2 is 13 N.5's (OverviewHost).
  startAt('#/instances/default')
  const bff = fakeBff({ version: VERSION_HOST })
  render(
    <StrictMode>
      <App />
    </StrictMode>,
  )
  const nav = await navOf()
  await within(nav).findByRole('link', { name: 'default' })
  expect(await hostLabel(nav), 'the nav label carries the name').toBe(
    VERSION_HOST.name,
  )
  expect(hostReads(bff), 'the double invoke costs no second read').toBe(1)
})

test('13 N.3: a sign-out then a sign-in as the same user is a new session: one more read, the labels at their fallback until it answers, the host after', async () => {
  startAt('#/profiles/model')
  let read = 0
  const second = held()
  const bff = fakeBff({
    session: 'signed-out',
    signIn: asPosted,
    version: (init) =>
      ++read === 1 ? reply(200, VERSION_HOST) : second.answer(init),
  })
  render(<App />)
  await signIn(await signInPage(screen.getByRole('main')), 'alice')
  let nav = await navOf()
  expect(await hostLabel(nav), "alice's first session reads the host").toBe(
    VERSION_HOST.name,
  )
  fireEvent.click(
    await screen.findByRole('button', { name: en('session-sign-out') }),
  )
  await signInPage(screen.getByRole('main'))
  await quiet(bff)
  expect(hostReads(bff), 'the sign-out reads nothing about the host').toBe(1)
  await signIn(screen.getByRole('main'), 'alice')
  nav = await navOf()
  await quiet(bff)
  expect(
    hostReads(bff),
    'the same user signing back in starts a session: the shell one and the Overview line one (13 N.10, 18 AN.15)',
  ).toBe(3)
  expect(
    await hostLabel(nav),
    "the old session's name does not carry over: the fallback while the new read is open",
  ).toBe(en('nav-this-node'))
  second.resolve(reply(200, VERSION_HOST))
  expect(await hostLabel(nav), 'and the host once it answers').toBe(
    VERSION_HOST.name,
  )
})

test.each(HOST_FAILURES)(
  '13 N.3: a host read answering %s signs nobody out, keeps the page, and leaves the labels at their fallback',
  async (what, version) => {
    // The model profiles page reads nothing about the host of its own (and has
    // no version line), so the one GET /api/version here is the shell's
    // (18 AN.15), and a 401 on it ends no session.
    startAt('#/profiles/model')
    const bff = fakeBff({ version })
    render(<App />)
    const nav = await navOf()
    await within(nav).findByRole('link', { name: 'default' })
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
      en('nav-this-node'),
    )
    await quiet(bff)
    expect(hostReads(bff), `${what}: one read, no retry`).toBe(1)
    expect(
      bff.to('GET', '/api/node').length,
      `${what}: the shell sends no GET /api/node (18 AN.15)`,
    ).toBe(0)
    cleanup()
    vi.unstubAllGlobals()
  },
)

// The Overview side of 18 AN.15: the name rides the same read, and the shell
// sends no GET /api/node anywhere. The h2 itself is 13 N.5's (OverviewHost).
test('18 AN.15: the shell reads the host from GET /api/version, the Overview h2 and the nav label carry its name, and no page of the shell sends GET /api/node', async () => {
  const { bff, nav } = await open({ version: VERSION_HOST })
  expect(await hostLabel(nav), 'the nav label').toBe(VERSION_HOST.name)
  expect(
    (await screen.findByRole('heading', { level: 2, name: VERSION_HOST.name }))
      .textContent,
    'the Overview h2 over the tiles',
  ).toBe(VERSION_HOST.name)
  for (const hash of [
    '#/instances/default',
    '#/profiles/model',
    '#/about',
    '#/',
  ]) {
    navigate(hash)
    await screen.findByRole('main')
  }
  expect(
    bff.to('GET', '/api/node').length,
    'the shell and the pages beside Node info send no GET /api/node',
  ).toBe(0)
})

// ── 13 N.4: the host names the instance group ───────────────────────────────

test('13 N.4: once the read answered, the label over the instance links is the host and the list accessible name is it, the links and their order unchanged', async () => {
  const { nav } = await open({ version: VERSION_HOST })
  expect(await hostLabel(nav), 'the name as sent').toBe(VERSION_HOST.name)
  const { list, label } = await instanceList(nav)
  expect(label?.classList.contains('nav-label'), 'a p.nav-label').toBe(true)
  expect(label?.tagName, 'the label stays a paragraph, not a heading').toBe('P')
  expect(list.getAttribute('aria-labelledby'), 'the list is named by it').toBe(
    label?.id,
  )
  expect(list.getAttribute('aria-label'), 'not by an aria-label').toBeNull()
  expect(
    Array.from(list.querySelectorAll('a')).map((a) => a.textContent?.trim()),
    'the links are the instances, in the order of the list, then Node info (20 N.7)',
  ).toEqual(['default', 'coder', en('nav-node')])
  expect(
    Array.from(list.querySelectorAll('a')).map((a) => a.getAttribute('href')),
    'their addresses are the instance pages, and #/node',
  ).toEqual(['#/instances/default', '#/instances/coder', '#/node'])
  expect(
    within(nav).getAllByRole('list', { name: VERSION_HOST.name }).length,
    'one list carries the host name: the host list (20 N.4)',
  ).toBe(1)
})

test('13 N.4: the label is text, no link and no button, its one icon decorative (20 N.4), and the nav holds no disclosure of its own', async () => {
  const { nav } = await open({ version: VERSION_HOST })
  const { label } = await instanceList(nav)
  expect(label?.querySelector('a'), 'the host is not a link').toBeNull()
  expect(label?.querySelector('button'), 'not a button').toBeNull()
  const icon = label?.querySelector('svg')
  expect(icon, '20 N.4: the computer icon beside the host').not.toBeNull()
  expect(icon?.classList.contains('icon'), 'the class the sheet sizes').toBe(
    true,
  )
  expect(icon?.getAttribute('aria-hidden'), 'decorative').toBe('true')
  expect(
    label?.firstElementChild,
    'the icon is the first child, before the text',
  ).toBe(icon)
  expect(
    nav.querySelectorAll('button').length,
    'no button in the nav: one host has nothing to fold',
  ).toBe(0)
  expect(nav.querySelectorAll('[aria-expanded]').length, 'no disclosure').toBe(
    0,
  )
})

test('13 N.4: before the answer the label is "This node" (20 N.4), and the same list and links carry it; when the answer arrives the label moves', async () => {
  const h = held()
  const { nav } = await open({ version: h.answer })
  const before = await instanceList(nav)
  expect(
    (before.label?.textContent ?? '').trim(),
    'the fallback while the read is open',
  ).toBe(en('nav-this-node'))
  h.resolve(reply(200, VERSION_HOST))
  // The host name lands in the nav label and, on the Overview, in the h2 over
  // the tiles: the nav is the place this test watches.
  await within(nav).findByText(VERSION_HOST.name, {}, { timeout: 2_000 })
  const after = await instanceList(nav)
  expect(after.list, 'the same list element').toBe(before.list)
  expect(
    Array.from(after.list.querySelectorAll('a')).map((a) =>
      a.textContent?.trim(),
    ),
    'the same links, Node info included (20 N.7)',
  ).toEqual(['default', 'coder', en('nav-node')])
  expect(after.label, 'the same label element').toBe(before.label)
  expect(
    (after.label?.textContent ?? '').trim(),
    'the words moved to the host',
  ).toBe(VERSION_HOST.name)
})

test('20 N.6: no instances to show, the Nodes label, the host label and the Node info entry stay, no instance link', async () => {
  // `open` waits for an instance link, and there is none here: the nav arrives
  // with the session, and the Overview with its own read.
  fakeBff({ version: VERSION_HOST, instances: [] })
  render(<App />)
  const nav = await navOf()
  await screen.findByRole('heading', {
    level: 1,
    name: en('overview-title'),
  })
  expect(
    within(nav).getByText(VERSION_HOST.name),
    'the host label is there (20 N.6)',
  ).toBeTruthy()
  expect(
    within(nav).getByText(en('nav-nodes')),
    'and so is the Nodes label',
  ).toBeTruthy()
  expect(
    within(nav).queryByRole('link', { name: 'default' }),
    'no instance link',
  ).toBeNull()
  expect(
    navLinks(nav).map((a) => a.textContent?.trim()),
    'Overview, Node info (alone in the host list), the model profiles, About',
  ).toEqual([
    en('nav-overview'),
    en('nav-node'),
    // Brief 26 (26.1): Instance profiles leads the Profiles group.
    en('nav-instance-profiles'),
    en('nav-server-profiles'),
    en('nav-model-profiles'),
    en('nav-about'),
  ])
})

// ── 13 N.6 (18 AN.6, now 20 N.9): the last list of the nav ──────────────────

test('20 N.9: signed in, the nav last list holds the About link only, with its icon first, About still last', async () => {
  const { nav } = await open({ version: VERSION_HOST })
  const groups = Array.from(nav.querySelectorAll(':scope > ul'))
  const last = groups[groups.length - 1]
  expect(last, 'About sits in the last list').not.toBeNull()
  expect(
    last.hasAttribute('aria-labelledby'),
    'and the list has no label: one entry is not a group of many',
  ).toBe(false)
  const links = Array.from(last.querySelectorAll('a'))
  expect(
    links.map((a) => a.textContent?.trim()),
    'About alone (20 N.9: Node info moved into the host list, 20 N.7)',
  ).toEqual([en('nav-about')])
  expect(
    links.map((a) => a.getAttribute('href')),
    'its address',
  ).toEqual(['#/about'])
  const pairs: [HTMLAnchorElement, string][] = [[links[0], en('nav-about')]]
  for (const [link, name] of pairs) {
    const icon = link?.firstElementChild
    expect(
      icon?.tagName,
      `${name}: the icon is the first thing in the link`,
    ).toBe('svg')
    expect(
      icon?.classList.contains('icon'),
      `${name}: the icon the sheet sizes`,
    ).toBe(true)
    expect(
      icon?.getAttribute('aria-hidden'),
      `${name}: the icon is decorative`,
    ).toBe('true')
    expect(
      link?.getAttribute('aria-label'),
      `${name}: named by its text`,
    ).toBeNull()
  }
  const profiles = await within(nav).findByRole('list', {
    name: en('nav-profiles'),
  })
  expect(
    profiles.compareDocumentPosition(last) & Node.DOCUMENT_POSITION_FOLLOWING,
    'the list comes after the Profiles group',
  ).toBeTruthy()
  expect(
    navLinks(nav).at(-1)?.textContent?.trim(),
    'About is the last thing the nav reads',
  ).toBe(en('nav-about'))
})

test('18 AN.6: the Node info link carries aria-current="page" on #/node and none on the other pages, and the About link keeps its own', async () => {
  const { nav } = await open({ version: VERSION_HOST })
  const node = () => within(nav).getByRole('link', { name: en('nav-node') })
  const about = () => within(nav).getByRole('link', { name: en('nav-about') })
  expect(node().hasAttribute('aria-current'), 'not on the Overview').toBe(false)
  for (const hash of ['#/instances/default', '#/profiles/model', '#/about']) {
    navigate(hash)
    await screen.findByRole('main')
    expect(node().hasAttribute('aria-current'), `not on ${hash}`).toBe(false)
    expect(
      about().getAttribute('aria-current'),
      `About has it on #/about, nowhere else: ${hash}`,
    ).toBe(hash === '#/about' ? 'page' : null)
  }
  navigate('#/node')
  await screen.findByRole('heading', { level: 1, name: en('node-title') })
  expect(node().getAttribute('aria-current'), 'on its own page').toBe('page')
})

test('13 N.6: the About link has aria-current="page" on #/about and none on the other pages', async () => {
  const { nav } = await open({ version: VERSION_HOST })
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
  const bff = fakeBff({ version: VERSION_HOST, session: 'signed-out' })
  render(<App />)
  const main = await signInPage(screen.getByRole('main'))
  expect(
    within(main).getByText(en('signin-required')),
    'the notice',
  ).toBeTruthy()
  expect(screen.queryByRole('navigation'), 'no nav signed out').toBeNull()
  expect(hostReads(bff), 'and no read signed out').toBe(0)
})

// ── 13 N.11: the order, and the menu ────────────────────────────────────────

test('13 N.11: at #/about the Tab path is the shell and its links with About last, and the page adds no stop of its own', async () => {
  const { nav } = await open({ version: VERSION_HOST })
  navigate('#/about')
  const h1 = await screen.findByRole('heading', {
    level: 1,
    name: en('about-title'),
  })
  expect(document.activeElement, 'the page takes the focus on its h1').toBe(h1)
  expect(
    tabbable().map(nameOf),
    'the shell order, About the last stop (20 N.10: Node info follows the instances, Model profiles after it)',
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
    // Brief 26 (26.1): Instance profiles leads the Profiles group.
    en('nav-instance-profiles'),
    en('nav-server-profiles'),
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
  const { nav } = await open({ version: VERSION_HOST })
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
    'the same links, in the same order, About last (20 N.10)',
  ).toEqual([
    en('nav-overview'),
    'default',
    'coder',
    en('nav-node'),
    // Brief 26 (26.1): Instance profiles leads the Profiles group.
    en('nav-instance-profiles'),
    en('nav-server-profiles'),
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
