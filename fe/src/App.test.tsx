// The app shell (fe/briefs/01-server.md, slice 1a, section 2.1): skip link,
// banner, nav and main, the session read on start, hash routes, page titles
// and focus. The whole app renders against a fake bff (mocked fetch); test
// names carry the brief's criterion number ("1a.4" is criterion 4).
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from '@testing-library/react'
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { afterEach, beforeEach, expect, test, vi } from 'vitest'
import App from './App.tsx'
import strings from './strings/en_US.json'
import {
  EN,
  ROLE_WORDS,
  before,
  deferred,
  en,
  fakeBff,
  navigate,
  reply,
  startAt,
} from './test/bff.ts'

beforeEach(() => startAt('#/'))

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
  startAt('')
})

/** The names of the links in the nav, in order. */
function navLinks(): string[] {
  const nav = screen.getByRole('navigation', { name: en('nav-label') })
  return within(nav)
    .queryAllByRole('link')
    .map((a) => a.textContent ?? '')
}

// ── Failure paths first ─────────────────────────────────────────────────

test('1a.8: an unknown route shows not-found with a link to the overview', async () => {
  startAt('#/no-such-page')
  fakeBff()
  render(<App />)
  const main = screen.getByRole('main')
  expect(await within(main).findByText(en('not-found'))).toBeTruthy()
  const home = within(main).getByRole('link', { name: en('not-found-home') })
  expect(home.getAttribute('href')).toBe('#/')
})

test('1a.8 (2.6): the not-found sentence is the h1 and the page title', async () => {
  fakeBff()
  render(<App />)
  await screen.findByRole('heading', { level: 1, name: en('overview-title') })
  navigate('#/no-such-page')
  const h1 = await screen.findByRole('heading', {
    level: 1,
    name: en('not-found'),
  })
  await waitFor(() => expect(document.activeElement).toBe(h1))
  expect(document.title).toBe(en('page-title', en('not-found')))
})

test('09 S1a.3 (1a.2): until the session answers, main shows loading and there is no nav at all', async () => {
  const session = deferred()
  const bff = fakeBff({ session: () => session.promise })
  render(<App />)
  const main = screen.getByRole('main')
  const status = within(main).getByRole('status')
  expect(status.tagName).toBe('P')
  expect(status.textContent).toBe(en('loading'))
  // 09 S1a.4: signed out or pending there is no nav (and no Menu button), so
  // there is no nav to hold an Overview entry.
  expect(document.querySelector('nav')).toBeNull()
  expect(document.getElementById('menu-button')).toBeNull()
  await act(async () => {
    session.resolve(reply(200, { user: 'alice' }))
  })
  await screen.findByRole('button', { name: en('session-sign-out') })
  // Signed in, the nav is today's shell: Overview, the host's instance links
  // with Node info under them, then Model profiles (09 S1a.4, and brief 20
  // 20 N.3: the Nodes group holds the host, its instances and Node info).
  expect(
    navLinks(),
    'the nav links once the session answers, in the order of 20 N.3 and 20 N.7',
  ).toEqual([
    en('nav-overview'),
    'default',
    'coder',
    // Brief 20 (20 N.7): Node info moved into the host's list, after them.
    en('nav-node'),
    en('nav-server-profiles'),
    en('nav-model-profiles'),
    en('nav-about'),
  ])
  expect(bff.to('GET', '/api/session')).toHaveLength(1)
})

test('1a.4: a failed instances read leaves the instance links out of the nav, the Overview shows the error', async () => {
  fakeBff({
    instances: () => reply(503, { key: 'client-unavailable', args: [] }),
  })
  render(<App />)
  expect(
    await screen.findByText(en('instances-manager-unavailable')),
  ).toBeTruthy()
  await screen.findByRole('button', { name: en('session-sign-out') })
  const nav = screen.getByRole('navigation', { name: en('nav-label') })
  expect(
    within(nav).queryByRole('link', { name: 'default' }),
    // Brief 20 (20 N.6): the group is shown, the links are what a failed read
    // leaves out.
    'no instance link',
  ).toBeNull()
  // Model profiles stays (fe/briefs/02-models.md, 2a criterion 1).
  expect(navLinks()).toEqual([
    en('nav-overview'),
    en('nav-node'),
    en('nav-server-profiles'),
    en('nav-model-profiles'),
    en('nav-about'),
  ])
})

// The bff's sign-in may not exist yet (404); a failure is signed out too.
const SESSION_FAILURES: [string, () => Response | Promise<Response>][] = [
  ['404', () => reply(404, 'Not Found')],
  ['network', () => Promise.reject(new TypeError('Failed to fetch'))],
  ['500', () => reply(500, { key: 'secret-detail', args: [] })],
  ['200 not json', () => reply(200, 'secret-detail')],
  ['200 {}', () => reply(200, {})],
]

test.each(SESSION_FAILURES)(
  '09 S1a.1 (1a.3): any session reply but 200 leaves the app signed out: the sign-in page, no Overview and no Sign in: %s',
  async (what, session) => {
    fakeBff({ session })
    render(<App />)
    const main = screen.getByRole('main')
    expect(
      await within(main).findByRole('heading', {
        level: 1,
        name: en('signin-title'),
      }),
      what,
    ).toBeTruthy()
    expect(
      [...main.children].map((el) => el.className),
      `${what}: main holds the sign-in page only`,
    ).toEqual(['signin-page'])
    expect(
      document.querySelector('ul.tiles'),
      `${what}: no instance tiles`,
    ).toBeNull()
    expect(
      document.body.textContent,
      `${what}: no framework version`,
    ).not.toContain(en('version-label', '2.99.0'))
    const banner = screen.getByRole('banner')
    expect(
      within(banner).queryByRole('button', { name: en('session-sign-in') }) ??
        within(banner).queryByRole('link', { name: en('session-sign-in') }),
      `${what}: no banner Sign in (09 S1a.4)`,
    ).toBeNull()
    expect(
      within(banner).queryByRole('button', { name: en('session-sign-out') }),
      what,
    ).toBeNull()
    expect(document.querySelector('nav'), `${what}: no nav`).toBeNull()
    expect(document.body.textContent, what).not.toContain('secret-detail')
  },
)

// ── The shell ───────────────────────────────────────────────────────────

test('1a.1: skip link, banner, navigation and main come in that order', async () => {
  fakeBff()
  render(<App />)
  await screen.findByRole('button', { name: en('session-sign-out') })
  const skip = screen.getByRole('link', { name: en('skip-to-content') })
  const banner = screen.getByRole('banner')
  const nav = screen.getByRole('navigation', { name: en('nav-label') })
  const main = screen.getByRole('main')
  expect(before(skip, banner)).toBe(true)
  expect(before(banner, nav)).toBe(true)
  expect(before(nav, main)).toBe(true)
  // The skip link is the first focusable element.
  const focusable = document.querySelectorAll('a[href], button, input, select')
  expect(focusable[0]).toBe(skip)
  const home = within(banner).getByRole('link', { name: en('app-name') })
  expect(home.getAttribute('href')).toBe('#/')
})

test('1a.1: the skip link moves focus to main without changing the route', async () => {
  fakeBff()
  render(<App />)
  await screen.findByRole('heading', { level: 1, name: en('overview-title') })
  fireEvent.click(screen.getByRole('link', { name: en('skip-to-content') }))
  const main = screen.getByRole('main')
  await waitFor(() => expect(main.contains(document.activeElement)).toBe(true))
  expect(window.location.hash).toBe('#/')
  expect(
    screen.getByRole('heading', { level: 1, name: en('overview-title') }),
  ).toBeTruthy()
})

test('09 S1a.4 (1a.3): signed out, the banner offers no Sign in, there is no nav, and the form is in main', async () => {
  fakeBff({ session: 'signed-out' })
  render(<App />)
  const main = screen.getByRole('main')
  // The page IS the form (S1a.1): the banner's Sign in pill is gone, so a
  // visitor has one way to sign in and it is where they already are.
  await within(main).findByRole('heading', {
    level: 1,
    name: en('signin-title'),
  })
  const banner = screen.getByRole('banner')
  expect(
    within(banner).queryByRole('button', { name: en('session-sign-in') }) ??
      within(banner).queryByRole('link', { name: en('session-sign-in') }),
    'no Sign in in the banner',
  ).toBeNull()
  expect(
    within(banner).queryByRole('button', { name: en('session-sign-out') }),
    'not signed in',
  ).toBeNull()
  expect(document.querySelector('nav'), 'no nav signed out').toBeNull()
  expect(
    document.getElementById('menu-button'),
    'no Menu button signed out',
  ).toBeNull()
  // The header keeps the home link, and it leads to the Overview (S1a.4).
  const home = within(banner).getByRole('link', { name: en('app-name') })
  expect(home.getAttribute('href')).toBe('#/')
})

test('1a.1, 4: a session reply of {"user"} alone signs in: the banner shows the user, then Sign out, and no role word', async () => {
  fakeBff({ session: () => reply(200, { user: 'alice' }) })
  render(<App />)
  const banner = screen.getByRole('banner')
  const signOut = await within(banner).findByRole('button', {
    name: en('session-sign-out'),
  })
  const text = banner.textContent ?? ''
  const name = text.indexOf(en('session-signed-in-as', 'alice'))
  const out = text.indexOf(en('session-sign-out'))
  expect(name, 'the user name shows').toBeGreaterThanOrEqual(0)
  expect(out, 'Sign out comes after it').toBeGreaterThan(name)
  // The role words are gone: every signed-in session is an administrator.
  for (const word of ROLE_WORDS)
    expect(text, `no ${word} in the banner`).not.toContain(word)
  expect(signOut).toBeTruthy()
  expect(
    within(banner).queryByRole('button', { name: en('session-sign-in') }),
  ).toBeNull()
})

test('1a.1: a session reply that still carries canConfigure signs in the same way (an older bff sent it)', async () => {
  for (const canConfigure of [true, false]) {
    fakeBff({ session: () => reply(200, { user: 'alice', canConfigure }) })
    render(<App />)
    const banner = screen.getByRole('banner')
    await within(banner).findByRole('button', {
      name: en('session-sign-out'),
    })
    const text = banner.textContent ?? ''
    const who = String(canConfigure)
    expect(text, who).toContain(en('session-signed-in-as', 'alice'))
    for (const word of ROLE_WORDS)
      expect(text, `${who}: no ${word}`).not.toContain(word)
    // Signed in: the nav holds the host's instance links (brief 20 20 N.4: the
    // host list is named by the host label, here the fallback "This node", the
    // default reply carrying no host name).
    const nav = screen.getByRole('navigation', { name: en('nav-label') })
    expect(
      await within(nav).findByRole('list', { name: en('nav-this-node') }),
      who,
    ).toBeTruthy()
    cleanup()
    vi.unstubAllGlobals()
  }
})

test('1a.4: signed in, the nav holds Overview then the host list with one link per instance, in order', async () => {
  fakeBff({
    instances: [
      { id: 'coder', engine: 'llama.cpp', state: 'active' },
      { id: 'default', engine: 'llama.cpp', state: 'inactive' },
      { id: 'big-model', engine: 'llama.cpp', state: 'failed' },
    ],
  })
  render(<App />)
  // 09 S1a.3: while the session read is pending there is no nav at all, so the
  // nav is taken once the session answers.
  const nav = await screen.findByRole('navigation', { name: en('nav-label') })
  const list = await within(nav).findByRole('list', {
    name: en('nav-this-node'),
  })
  const links = within(list).getAllByRole('link')
  expect(links.map((a) => a.textContent)).toEqual([
    'coder',
    'default',
    'big-model',
    // Brief 20 (20 N.7): the host list ends with Node info.
    en('nav-node'),
  ])
  expect(links.map((a) => a.getAttribute('href'))).toEqual([
    '#/instances/coder',
    '#/instances/default',
    '#/instances/big-model',
    '#/node',
  ])
  expect(navLinks()).toEqual([
    en('nav-overview'),
    'coder',
    'default',
    'big-model',
    // Brief 20 (20 N.7): Node info sits under the instances, in the host list,
    // before the Profiles group (2a criterion 1 keeps Model profiles there).
    en('nav-node'),
    en('nav-server-profiles'),
    en('nav-model-profiles'),
    en('nav-about'),
  ])
  const overview = within(nav).getByRole('link', { name: en('nav-overview') })
  expect(before(overview, list)).toBe(true)
})

test('1a.5: the link of the current page has aria-current="page"', async () => {
  fakeBff()
  render(<App />)
  // 09 S1a.3: the nav is there once the session read answered.
  const nav = await screen.findByRole('navigation', { name: en('nav-label') })
  const list = await within(nav).findByRole('list', {
    name: en('nav-this-node'),
  })
  const overview = within(nav).getByRole('link', { name: en('nav-overview') })
  const defaultLink = within(list).getByRole('link', { name: 'default' })
  expect(overview.getAttribute('aria-current')).toBe('page')
  expect(defaultLink.getAttribute('aria-current')).toBeNull()
  navigate('#/instances/default')
  await waitFor(() =>
    expect(defaultLink.getAttribute('aria-current')).toBe('page'),
  )
  expect(overview.getAttribute('aria-current')).toBeNull()
  expect(
    within(list)
      .getByRole('link', { name: 'coder' })
      .getAttribute('aria-current'),
  ).toBeNull()
})

test('1a.6: the Menu button shows and hides the nav; Escape in the nav closes it and returns focus', async () => {
  fakeBff()
  render(<App />)
  await screen.findByRole('button', { name: en('session-sign-out') })
  const menu = screen.getByRole('button', { name: en('nav-menu') })
  const nav = screen.getByRole('navigation', { name: en('nav-label') })
  expect(menu.getAttribute('aria-expanded')).toBe('false')
  const controlled = document.getElementById(
    menu.getAttribute('aria-controls') ?? '',
  )
  expect(controlled, 'aria-controls names the nav').not.toBeNull()
  expect(
    controlled === nav ||
      controlled!.contains(nav) ||
      nav.contains(controlled!),
  ).toBe(true)
  fireEvent.click(menu)
  expect(menu.getAttribute('aria-expanded')).toBe('true')
  fireEvent.click(menu)
  expect(menu.getAttribute('aria-expanded')).toBe('false')
  fireEvent.click(menu)
  const overview = within(nav).getByRole('link', { name: en('nav-overview') })
  overview.focus()
  fireEvent.keyDown(overview, { key: 'Escape', code: 'Escape' })
  expect(menu.getAttribute('aria-expanded')).toBe('false')
  expect(document.activeElement).toBe(menu)
})

test('1a.6: Escape on the Menu button right after opening closes the nav, focus stays on the button', async () => {
  fakeBff()
  render(<App />)
  await screen.findByRole('button', { name: en('session-sign-out') })
  const menu = screen.getByRole('button', { name: en('nav-menu') })
  menu.focus()
  fireEvent.click(menu)
  expect(menu.getAttribute('aria-expanded')).toBe('true')
  expect(document.activeElement).toBe(menu)
  fireEvent.keyDown(menu, { key: 'Escape', code: 'Escape' })
  expect(menu.getAttribute('aria-expanded'), 'Escape closes the menu').toBe(
    'false',
  )
  expect(document.activeElement).toBe(menu)
})

test('1a.7: a route change sets the page title and moves focus to the h1', async () => {
  fakeBff()
  render(<App />)
  await screen.findByRole('heading', { level: 1, name: en('overview-title') })
  await waitFor(() =>
    expect(document.title).toBe(en('page-title', en('overview-title'))),
  )
  navigate('#/instances/default')
  const h1 = await screen.findByRole('heading', { level: 1, name: 'default' })
  await waitFor(() => expect(document.activeElement).toBe(h1))
  expect(document.title).toBe(en('page-title', 'default'))
  expect(h1.getAttribute('tabindex')).toBe('-1')
  navigate('#/')
  const overview = await screen.findByRole('heading', {
    level: 1,
    name: en('overview-title'),
  })
  await waitFor(() => expect(document.activeElement).toBe(overview))
  expect(document.title).toBe(en('page-title', en('overview-title')))
})

test('1a.7: the sign-in page is titled Sign in, signed out at the sign-in page and at a protected one', async () => {
  startAt('#/sign-in')
  fakeBff({ session: 'signed-out' })
  render(<App />)
  await screen.findByRole('heading', { level: 1, name: en('signin-title') })
  await waitFor(() =>
    expect(document.title).toBe(en('page-title', en('signin-title'))),
  )
  // 09 S1a.6: signed out, every hash is the sign-in page, so it is the title
  // everywhere, and 09 S1a.1 leaves one h1 in the document. 09 S1a.9: an
  // in-app arrival moves the focus to that h1.
  navigate('#/instances/default')
  const h1 = await screen.findByRole('heading', {
    level: 1,
    name: en('signin-title'),
  })
  expect(
    [...document.querySelectorAll('h1')],
    'the sign-in page is the one page',
  ).toEqual([h1])
  await waitFor(() => expect(document.activeElement).toBe(h1))
  expect(document.title).toBe(en('page-title', en('signin-title')))
})

// ── Strings ─────────────────────────────────────────────────────────────

test('1a strings: en_US.json holds the English the brief gives (section 5.5)', () => {
  const table: Record<string, string> = strings
  const wrong = Object.entries(EN)
    .filter(([key, english]) => table[key] !== english)
    .map(([key]) => key)
  expect(wrong, 'keys missing or with other text').toEqual([])
  // The web UI is run by administrators only, so the English of the
  // non-administrator views goes with them (fe/briefs/01-server.md,
  // "Administrators only", 5.5 and 2.8): the Server page has no read-only
  // Notice, the API key has no withheld sentence, and the banner names no role.
  // 2h.1's 130 adds one: the model profile page's read-only Notice is replaced by
  // model-profile-partly-editable, so model-profile-read-only is out of the strings file.
  // 22 D2.10 adds one more: the muted line beside the bottom Delete button is
  // replaced by the status row's model-profile-on, so
  // model-profile-delete-needs-off is out too.
  // 22 D4.12 adds two more: the profile pages' status row leaves (22 D4.9), so
  // its two sentences leave with it.
  // 16 17f2.10 adds seven: 17f's Rename form leaves the page, and its keys
  // leave the strings file with it (the display name form uses instance-name-*
  // and keeps instance-label-cancel).
  // 24 A2.1 adds two more: the page's state paragraph becomes a state line with
  // its own switch, so the two sentences that pointed at the list and at the
  // Delete leave with it (model-profile-on-short and model-profile-off-short
  // are their short forms).
  // 22 P.9a adds one more: the bff answers a repo with no GGUF 502
  // hf-bad-answer, never a 200 with no files, so the text that named an empty
  // listing leaves the strings list with it.
  const removed = [
    'server-read-only',
    'server-api-key-withheld',
    'session-role-admin',
    'session-role-user',
    'model-profile-read-only',
    'model-profile-delete-needs-off',
    'server-profile-unused',
    'server-profile-used-by',
    'model-profile-on',
    'model-profile-off',
    'instance-rename',
    'instance-label',
    'instance-label-hint',
    'instance-label-save',
    'instance-label-saving',
    'instance-label-saved',
    'instance-label-cleared',
    'hf-quants-empty',
  ]
  expect(
    Object.keys(table).filter((key) => removed.includes(key)),
    'strings the brief removed',
  ).toEqual([])
})

// ── The page's own file ─────────────────────────────────────────────────

/** The inline icon of brief criterion 8a, replaced by the hearth mark of
 *  fe/briefs/07-hearth-h3-mark.md (H3.6): the outline path, the accent of the
 *  light scheme and, in the dark block, the dark accent. */
const ICON_HREF =
  "data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='2 3 28 28'%3E%3Cstyle%3Epath{fill:%237b3a8f}@media (prefers-color-scheme:dark){path{fill:%23dcaaf0}}%3C/style%3E%3Cpath fill-rule='evenodd' d='M5 30V15a11 11 0 0 1 22 0v15zM17.79 18.12 18.8 24h-5.6l1.01-5.88a3.6 3.6 0 1 1 3.58 0z'/%3E%3C/svg%3E"

test('1b.8a (2.7): index.html declares one inline SVG icon in the head, after the viewport meta', () => {
  const html = readFileSync(
    join(import.meta.dirname, '..', 'index.html'),
    'utf8',
  )
  const doc = new DOMParser().parseFromString(html, 'text/html')
  const icons = [...doc.querySelectorAll('link')].filter((l) =>
    (l.getAttribute('rel') ?? '').toLowerCase().split(/\s+/).includes('icon'),
  )
  expect(icons, 'one link rel="icon"').toHaveLength(1)
  const icon = icons[0]!
  expect(icon.parentElement, 'in the head').toBe(doc.head)
  expect(icon.getAttribute('rel')).toBe('icon')
  expect(icon.getAttribute('type')).toBe('image/svg+xml')
  expect(icon.getAttribute('href')).toBe(ICON_HREF)
  const viewport = doc.head.querySelector('meta[name="viewport"]')
  expect(viewport, 'the viewport meta').not.toBeNull()
  expect(before(viewport!, icon), 'after the viewport meta').toBe(true)
})

// ── The banner's CSS ────────────────────────────────────────────────────

test('46b (4b): index.css breaks a long user name inside the banner', () => {
  // A user name with no break point (32 characters, the useradd(8) maximum)
  // keeps its full width and can push the page past a 320 px viewport.
  // jsdom does no layout, so the brief lets this be pinned as the CSS rule:
  // overflow-wrap: anywhere on the signed-in span of .session. Comments are
  // dropped first, so a comment that quotes the rule is not a green.
  const raw = readFileSync(join(import.meta.dirname, 'index.css'), 'utf8')
  const css = raw.replace(/\/\*[\s\S]*?\*\//g, '')
  expect(
    /\.session\s*>?\s*span\s*\{[^}]*overflow-wrap:\s*anywhere/.test(css),
    'no overflow-wrap: anywhere rule for the signed-in span of .session',
  ).toBe(true)
})
