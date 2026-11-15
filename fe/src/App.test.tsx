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

test('1a.2: until the session answers, main shows loading and the nav only Overview', async () => {
  const session = deferred()
  const bff = fakeBff({ session: () => session.promise })
  render(<App />)
  const main = screen.getByRole('main')
  const status = within(main).getByRole('status')
  expect(status.tagName).toBe('P')
  expect(status.textContent).toBe(en('loading'))
  expect(navLinks()).toEqual([en('nav-overview')])
  await act(async () => {
    session.resolve(reply(200, { user: 'alice', canConfigure: true }))
  })
  await screen.findByRole('button', { name: en('session-sign-out') })
  expect(bff.to('GET', '/api/session')).toHaveLength(1)
})

test('1a.4: a failed instances read leaves the list out of the nav, the Overview shows the error', async () => {
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
    within(nav).queryByRole('list', { name: en('nav-instances') }),
  ).toBeNull()
  // Models stays (fe/briefs/02-models.md, 2a criterion 1).
  expect(navLinks()).toEqual([en('nav-overview'), en('nav-models')])
})

test('1a.3: any session reply but 200 leaves the app signed out and the Overview whole', async () => {
  // The bff's sign-in may not exist yet (404); a failure is signed out too.
  const answers: [string, () => Response | Promise<Response>][] = [
    ['404', () => reply(404, 'Not Found')],
    ['network', () => Promise.reject(new TypeError('Failed to fetch'))],
    ['500', () => reply(500, { key: 'secret-detail', args: [] })],
    ['200 not json', () => reply(200, 'secret-detail')],
    ['200 {}', () => reply(200, {})],
  ]
  for (const [what, session] of answers) {
    fakeBff({ session })
    render(<App />)
    const main = screen.getByRole('main')
    expect(
      await within(main).findByRole('heading', {
        level: 1,
        name: en('overview-title'),
      }),
      what,
    ).toBeTruthy()
    expect(
      await within(main).findByText(en('version-label', '2.99.0')),
      what,
    ).toBeTruthy()
    const table = await within(main).findByRole('table')
    expect(
      within(table).getByRole('link', { name: 'default' }),
      what,
    ).toBeTruthy()
    const banner = screen.getByRole('banner')
    expect(
      within(banner).queryByRole('button', { name: en('session-sign-in') }) ??
        within(banner).queryByRole('link', { name: en('session-sign-in') }),
      what,
    ).not.toBeNull()
    expect(
      within(banner).queryByRole('button', { name: en('session-sign-out') }),
      what,
    ).toBeNull()
    expect(navLinks(), what).toEqual([en('nav-overview')])
    expect(document.body.textContent, what).not.toContain('secret-detail')
    cleanup()
    vi.unstubAllGlobals()
  }
})

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

test('1a.3: signed out, the banner offers Sign in and the nav holds only Overview', async () => {
  fakeBff({ session: 'signed-out' })
  render(<App />)
  const banner = screen.getByRole('banner')
  // A button or a link.
  const signIn = await waitFor(() => {
    const found =
      within(banner).queryByRole('button', { name: en('session-sign-in') }) ??
      within(banner).queryByRole('link', { name: en('session-sign-in') })
    if (found === null) throw new Error('no Sign in in the banner')
    return found
  })
  expect(
    within(banner).queryByRole('button', { name: en('session-sign-out') }),
  ).toBeNull()
  expect(navLinks()).toEqual([en('nav-overview')])
  // It leads to the sign-in form.
  fireEvent.click(signIn)
  expect(
    await screen.findByRole('heading', { level: 1, name: en('signin-title') }),
  ).toBeTruthy()
})

test('1a.4: signed in as an administrator, the banner names the user, the role, then Sign out', async () => {
  fakeBff({ session: 'admin' })
  render(<App />)
  const banner = screen.getByRole('banner')
  await within(banner).findByRole('button', { name: en('session-sign-out') })
  const text = banner.textContent ?? ''
  const name = text.indexOf(en('session-signed-in-as', 'alice'))
  const role = text.indexOf(en('session-role-admin'))
  const out = text.indexOf(en('session-sign-out'))
  expect(name).toBeGreaterThanOrEqual(0)
  expect(role).toBeGreaterThan(name)
  expect(out).toBeGreaterThan(role)
  expect(text).not.toContain(en('session-role-user'))
  expect(
    within(banner).queryByRole('button', { name: en('session-sign-in') }),
  ).toBeNull()
})

test('1a.4: signed in as a user, the banner shows the User role', async () => {
  fakeBff({ session: 'user' })
  render(<App />)
  const banner = screen.getByRole('banner')
  await within(banner).findByRole('button', { name: en('session-sign-out') })
  const text = banner.textContent ?? ''
  expect(text).toContain(en('session-signed-in-as', 'bob'))
  expect(text).toContain(en('session-role-user'))
  expect(text).not.toContain(en('session-role-admin'))
})

test('1a.4: signed in, the nav holds Overview then an Instances list with one link per instance, in order', async () => {
  fakeBff({
    instances: [
      { id: 'coder', engine: 'llama.cpp', state: 'active' },
      { id: 'default', engine: 'llama.cpp', state: 'inactive' },
      { id: 'big-model', engine: 'llama.cpp', state: 'failed' },
    ],
  })
  render(<App />)
  const nav = screen.getByRole('navigation', { name: en('nav-label') })
  const list = await within(nav).findByRole('list', {
    name: en('nav-instances'),
  })
  const links = within(list).getAllByRole('link')
  expect(links.map((a) => a.textContent)).toEqual([
    'coder',
    'default',
    'big-model',
  ])
  expect(links.map((a) => a.getAttribute('href'))).toEqual([
    '#/instances/coder',
    '#/instances/default',
    '#/instances/big-model',
  ])
  expect(navLinks()).toEqual([
    en('nav-overview'),
    'coder',
    'default',
    'big-model',
    // After the Instances group (fe/briefs/02-models.md, 2a criterion 1).
    en('nav-models'),
  ])
  const overview = within(nav).getByRole('link', { name: en('nav-overview') })
  expect(before(overview, list)).toBe(true)
})

test('1a.5: the link of the current page has aria-current="page"', async () => {
  fakeBff()
  render(<App />)
  const nav = screen.getByRole('navigation', { name: en('nav-label') })
  const list = await within(nav).findByRole('list', {
    name: en('nav-instances'),
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

test('1b.5a (2.7): on #/sign-in the banner Sign in link is the current page; not while the form stands in for a protected page', async () => {
  startAt('#/sign-in')
  fakeBff({ session: 'signed-out' })
  render(<App />)
  await screen.findByRole('heading', { level: 1, name: en('signin-title') })
  const banner = screen.getByRole('banner')
  const link = within(banner).getByRole('link', { name: en('session-sign-in') })
  expect(link.getAttribute('aria-current')).toBe('page')
  const nav = screen.getByRole('navigation', { name: en('nav-label') })
  expect(
    within(nav)
      .getByRole('link', { name: en('nav-overview') })
      .getAttribute('aria-current'),
  ).toBeNull()
  // The form in place of a protected page: the route is that page's.
  navigate('#/instances/default')
  await screen.findByText(en('signin-required'))
  expect(
    within(banner)
      .getByRole('link', { name: en('session-sign-in') })
      .getAttribute('aria-current'),
  ).toBeNull()
  navigate('#/')
  await screen.findByRole('heading', { level: 1, name: en('overview-title') })
  expect(
    within(banner)
      .getByRole('link', { name: en('session-sign-in') })
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

test('1a.7: the sign-in page is titled Sign in', async () => {
  startAt('#/sign-in')
  fakeBff({ session: 'signed-out' })
  render(<App />)
  await screen.findByRole('heading', { level: 1, name: en('signin-title') })
  await waitFor(() =>
    expect(document.title).toBe(en('page-title', en('signin-title'))),
  )
  navigate('#/')
  await screen.findByRole('heading', { level: 1, name: en('overview-title') })
  navigate('#/sign-in')
  await waitFor(() =>
    expect(document.activeElement).toBe(
      screen.getByRole('heading', { level: 1, name: en('signin-title') }),
    ),
  )
  expect(document.title).toBe(en('page-title', en('signin-title')))
})

test('1a.19: the Overview shows the version and the instances signed out too', async () => {
  fakeBff({ session: 'signed-out' })
  render(<App />)
  const main = screen.getByRole('main')
  expect(
    await within(main).findByRole('heading', {
      level: 1,
      name: en('overview-title'),
    }),
  ).toBeTruthy()
  expect(
    await within(main).findByText(en('version-label', '2.99.0')),
  ).toBeTruthy()
  const table = await within(main).findByRole('table')
  expect(within(table).getByRole('link', { name: 'default' })).toBeTruthy()
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
  // Notice and the API key has no withheld sentence.
  const removed = ['server-read-only', 'server-api-key-withheld']
  expect(
    Object.keys(table).filter((key) => removed.includes(key)),
    'strings the brief removed',
  ).toEqual([])
})

// ── The page's own file ─────────────────────────────────────────────────

/** The inline icon of brief criterion 8a, exactly as the brief gives it. */
const ICON_HREF =
  "data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 16 16'%3E%3Crect width='16' height='16' rx='3' fill='%230a5cc2'/%3E%3Cpath d='M4 12 8 4l4 8M5.5 9h5' fill='none' stroke='%23fff' stroke-width='2' stroke-linecap='round' stroke-linejoin='round'/%3E%3C/svg%3E"

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
