// Signed out, only the sign-in page (fe/briefs/09-signin-only.md, slice S1a):
// at every hash the sign-in page is the only content, nothing is read, the
// shell offers no nav, no Menu button and no banner Sign in, the address is
// kept, and a sign-in shows the page of the kept hash without adding a history
// entry. The whole app renders against a fake bff (mocked fetch). Test names
// cite this brief as `09 S1a.n`, so they never read like the "S1" of
// 01-server.md (SessionRevoked.test.tsx). The stylesheet and source pins of
// S1a.12 are in S1a.test.ts.
import {
  act,
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
  deferred,
  en,
  fakeBff,
  isHidden,
  reply,
  startAt,
  stubDialog,
  type BffOptions,
  type Call,
} from './test/bff.ts'

/** Every hash S1a.1 lists: the Overview, a protected page, the sign-in page,
 * an unknown address, and no address at all. */
const HASHES = [
  '#/',
  '#/instances/x',
  '#/profiles/model',
  '#/profiles/model/x',
  '#/sign-in',
  '#/nope',
  '',
]

/** The route a call is on, as one word. */
const route = (c: Call): string => `${c.method} ${c.path}`

let dialogs: ReturnType<typeof stubDialog>

beforeEach(() => {
  dialogs = stubDialog()
})

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
  dialogs.restore()
  startAt('')
})

// ── Helpers ───────────────────────────────────────────────────────────────

/**
 * The bff of today (before item 13a): it answers the three session-free reads
 * (`GET /api/version`, `GET /api/instances`, `GET /api/instances/{id}/boot`)
 * with 200, and refuses every other route with a `TypeError`. It is the bff
 * that makes "the fe sends no content request signed out" a real test: a
 * request that were sent would be answered, and it shows in the calls.
 */
function todayBff(options: BffOptions = {}) {
  return fakeBff({ session: 'signed-out', ...options })
}

/**
 * The bff from 2026-11-19 (item 13a, fe/briefs/api-contract.md): with no
 * session, every read answers 401 `session-required`, so nothing of the
 * server shows even if the fe were to ask.
 */
function noSessionBff(options: BffOptions = {}) {
  const refused = () => reply(401, { key: 'session-required', args: [] })
  return fakeBff({
    session: 'signed-out',
    version: refused,
    instances: refused,
    modelProfiles: refused,
    modelProfile: refused,
    server: refused,
    boot: { read: refused, write: refused },
    ...options,
  })
}

/** Renders the app on `hash` with `bff` behind fetch, and returns `main`. */
function open(_bff: ReturnType<typeof fakeBff>, hash = '#/') {
  startAt(hash)
  render(<App />)
  return screen.getByRole('main')
}

/** The sign-in page in `main`: the box, the h1, the two fields and the submit. */
async function signInForm(main: HTMLElement) {
  const heading = await within(main).findByRole('heading', {
    level: 1,
    name: en('signin-title'),
  })
  const page = main.querySelector('div.signin-page')
  expect(page, 'the sign-in page box of H7.1').toBeTruthy()
  const username = (await within(main).findByRole('textbox', {
    name: en('signin-username'),
  })) as HTMLInputElement
  const password = within(main).getByLabelText(en('signin-password'), {
    selector: 'input',
  }) as HTMLInputElement
  const submit = within(main).getByRole('button', {
    name: en('signin-submit'),
  })
  return { heading, page: page as HTMLElement, username, password, submit }
}

/** Signs in through the form. */
function submitSignIn(main: HTMLElement, user = 'alice', password = 'hunter2') {
  fireEvent.change(
    within(main).getByRole('textbox', {
      name: en('signin-username'),
    }) as HTMLInputElement,
    { target: { value: user } },
  )
  fireEvent.change(
    within(main).getByLabelText(en('signin-password'), {
      selector: 'input',
    }),
    { target: { value: password } },
  )
  fireEvent.click(
    within(main).getByRole('button', { name: en('signin-submit') }),
  )
}

/** Signs out through the banner. */
function clickSignOut() {
  fireEvent.click(
    within(screen.getByRole('banner')).getByRole('button', {
      name: en('session-sign-out'),
    }),
  )
}

/** Types `hash` in the address bar: jsdom fires the hashchange in a task. */
async function typeHash(hash: string) {
  await act(async () => {
    window.location.hash = hash
    await new Promise((r) => setTimeout(r, 0))
  })
}

/** The browser's Back button: jsdom traverses in a task, not synchronously. */
async function pressBack() {
  await act(async () => {
    window.history.back()
    await new Promise((r) => setTimeout(r, 0))
    await new Promise((r) => setTimeout(r, 0))
  })
}

/** The visible text of an element, spaces collapsed. */
function text(el: Element): string {
  return (el.textContent ?? '').replace(/\s+/g, ' ').trim()
}

/**
 * The focusable elements of the page in DOM order, each named by the label
 * a keyboard user reads: the text of a link or button, the label of a field.
 * `tabindex="-1"` is not a Tab stop (the h1 of the page, H7.9).
 */
function tabStops(): string[] {
  return [
    ...document.querySelectorAll<HTMLElement>(
      'a[href], button, input, select, textarea, [tabindex]',
    ),
  ]
    .filter(
      (el) =>
        el.getAttribute('tabindex') !== '-1' &&
        !(el as HTMLButtonElement).disabled &&
        !isHidden(el),
    )
    .map((el) => {
      if (el.tagName === 'INPUT') {
        const label = document.querySelector<HTMLLabelElement>(
          `label[for="${el.id}"]`,
        )
        return label ? text(label) : el.id
      }
      return text(el)
    })
}

// ── S1a.1: the sign-in page at every hash ─────────────────────────────────

test.each(HASHES)(
  '09 S1a.1: signed out at %s, main holds the sign-in page and nothing else',
  async (hash) => {
    const main = open(noSessionBff(), hash)
    const { heading, username, password, submit } = await signInForm(main)
    expect(heading.id, `${hash}: the h1 is the page heading`).toBe(
      'page-heading',
    )
    expect(username.type, `${hash}: the user name field`).toBe('text')
    expect(password.type, `${hash}: the password field`).toBe('password')
    expect(submit.getAttribute('type'), `${hash}: the submit`).toBe('submit')
    // Nothing of any page: main's one child is the sign-in box.
    expect(
      [...main.children].map((el) => el.className),
      `${hash}: main holds the sign-in page only`,
    ).toEqual(['signin-page'])
    expect(
      document.querySelector('ul.tiles'),
      `${hash}: no instance tiles`,
    ).toBe(null)
    expect(
      document.body.textContent,
      `${hash}: no framework version`,
    ).not.toContain(en('version-label', '2.99.0'))
    expect(
      within(main).queryByRole('heading', {
        level: 1,
        name: en('overview-title'),
      }),
      `${hash}: no Overview`,
    ).toBeNull()
    expect(
      within(main).queryByText(en('not-found')),
      `${hash}: no not-found page`,
    ).toBeNull()
  },
)

test('09 S1a.1: a session read that fails (503) leaves the sign-in page at every hash it shows', async () => {
  const main = open(
    todayBff({
      session: () => reply(503, { key: 'client-unavailable', args: [] }),
    }),
    '#/instances/default',
  )
  const { username } = await signInForm(main)
  expect(
    [...main.children].map((el) => el.className),
    'main holds the sign-in page only',
  ).toEqual(['signin-page'])
  expect(
    username.getAttribute('autocomplete'),
    'the field is the form of today',
  ).toBe('username')
})

// ── S1a.2: the requests before a sign-in ──────────────────────────────────

test.each(HASHES)(
  '09 S1a.2: signed out at %s, the only request is the session read (today answers the content reads)',
  async (hash) => {
    const bff = todayBff()
    const main = open(bff, hash)
    await signInForm(main)
    expect(
      bff.calls.map(route),
      `${hash}: one session read, no content read`,
    ).toEqual(['GET /api/session'])
    expect(
      bff.calls.filter((c) => !c.path.startsWith('/api/session')),
      `${hash}: nothing of the server asked for`,
    ).toEqual([])
  },
)

test('09 S1a.2: a submit sends POST /api/session, and nothing is asked for before it', async () => {
  const bff = todayBff()
  const main = open(bff, '#/instances/default')
  const { username, password, submit } = await signInForm(main)
  fireEvent.change(username, { target: { value: 'alice' } })
  fireEvent.change(password, { target: { value: 'hunter2' } })
  fireEvent.click(submit)
  await within(main).findByRole('heading', { level: 1, name: 'default' })
  expect(
    bff.to('GET', '/api/session'),
    'the session is read once',
  ).toHaveLength(1)
  expect(bff.to('POST', '/api/session'), 'one submit').toHaveLength(1)
  const first = bff.calls.findIndex((c) => c.method === 'POST')
  expect(
    bff.calls.slice(0, first).map(route),
    'before the POST: the session read only',
  ).toEqual(['GET /api/session'])
})

// ── S1a.3: while the session is read ──────────────────────────────────────

test('09 S1a.3: while the session read is in the air, main holds the loading line only, the shell is the home link and the theme button, and nothing else is asked for', async () => {
  const gate = deferred()
  const bff = fakeBff({ session: () => gate.promise })
  const main = open(bff, '#/instances/default')
  expect(main.children, 'one element: the status line').toHaveLength(1)
  const status = main.querySelector('p[role="status"]')
  expect(status, 'the loading paragraph of today').toBeTruthy()
  expect(text(status!), 'the loading text').toBe(en('loading'))
  expect(document.querySelector('nav'), 'no nav while pending').toBeNull()
  expect(
    document.getElementById('menu-button'),
    'no Menu button while pending',
  ).toBeNull()
  expect(document.querySelector('a.session'), 'no banner Sign in').toBeNull()
  const banner = screen.getByRole('banner')
  expect(
    within(banner).getAllByRole('link'),
    'the home link only',
  ).toHaveLength(1)
  expect(
    within(banner)
      .getAllByRole('button')
      .map((el) => text(el)),
    'the theme button only',
  ).toEqual([en('theme-dark')])
  expect(bff.calls.map(route), 'the session read only').toEqual([
    'GET /api/session',
  ])
  gate.resolve(reply(401, { key: 'session-required', args: [] }))
  await signInForm(main)
  expect(bff.calls.map(route), 'still nothing but the session read').toEqual([
    'GET /api/session',
  ])
})

// ── S1a.4: the shell signed out, pending, and signed in ───────────────────

test('09 S1a.4: signed out there is no nav, no Menu button and no banner Sign in; the header keeps the home link and the theme button, and the skip link stays first', async () => {
  const main = open(noSessionBff(), '#/profiles/model')
  await signInForm(main)
  expect(document.querySelector('nav'), 'no nav element at all').toBeNull()
  expect(screen.queryByRole('navigation'), 'no navigation landmark').toBeNull()
  expect(document.getElementById('menu-button'), 'no Menu button').toBeNull()
  expect(
    document.querySelector('a.session'),
    'no banner Sign in pill',
  ).toBeNull()
  const banner = screen.getByRole('banner')
  expect(
    within(banner).queryByRole('link', { name: en('session-sign-in') }),
    'no Sign in link in the banner',
  ).toBeNull()
  expect(
    within(banner).queryByRole('button', { name: en('session-sign-out') }),
    'no Sign out button in the banner',
  ).toBeNull()
  expect(banner.querySelector('div.session'), 'no session block').toBeNull()
  const home = within(banner).getByRole('link', { name: en('app-name') })
  expect(home.getAttribute('href'), 'the home link leads to the Overview').toBe(
    '#/',
  )
  expect(
    within(banner).getAllByRole('link'),
    'one link: the home one',
  ).toHaveLength(1)
  expect(
    within(banner)
      .getAllByRole('button')
      .map((el) => text(el)),
    'the theme button only',
  ).toEqual([en('theme-dark')])
  const tools = document.querySelector('div.banner-tools')
  expect(tools, 'the banner tools').toBeTruthy()
  expect(
    [...tools!.children].map((el) => el.className),
    'div.banner-tools holds the theme button only',
  ).toEqual(['secondary theme-switch'])
  // The landmarks signed out are banner and main; the skip link is the first
  // link and still targets main (WCAG 2.4.1).
  expect(screen.getByRole('banner'), 'the banner landmark').toBeTruthy()
  expect(screen.getByRole('main'), 'the main landmark').toBeTruthy()
  const skip = document.querySelector('a.skip-link')
  expect(skip, 'the skip link stays').toBeTruthy()
  expect(skip!.getAttribute('href'), 'and still targets main').toBe('#main')
  expect(
    document.querySelectorAll('a')[0],
    'it is the first link of the page',
  ).toBe(skip)
  expect(
    document.getElementById('main'),
    'the target of the skip link',
  ).toBeTruthy()
})

test('09 S1a.4: signed in, the shell is exactly today is: Menu, theme, the user with Sign out, and the nav', async () => {
  const bff = fakeBff()
  const main = open(bff, '#/profiles/model')
  await within(main).findByRole('heading', {
    level: 1,
    name: en('model-profiles-title'),
  })
  expect(
    document.querySelector('nav'),
    'the nav is there signed in',
  ).toBeTruthy()
  const nav = await screen.findByRole('navigation', { name: en('nav-label') })
  expect(
    within(nav).getByRole('link', { name: en('nav-overview') }),
    'the Overview entry',
  ).toBeTruthy()
  expect(
    within(nav).getByRole('link', { name: en('nav-model-profiles') }),
    'the Model profiles entry',
  ).toBeTruthy()
  expect(document.getElementById('menu-button'), 'the Menu button').toBeTruthy()
  const banner = screen.getByRole('banner')
  expect(
    within(banner).getByRole('button', { name: en('session-sign-out') }),
    'Sign out',
  ).toBeTruthy()
  expect(banner.textContent).toContain(en('session-signed-in-as', 'alice'))
  expect(
    document.querySelector('div.session'),
    'the session block',
  ).toBeTruthy()
  expect(
    document.querySelector('a.session'),
    'no Sign in pill signed in',
  ).toBeNull()
})

// ── S1a.5: the Tab order signed out ───────────────────────────────────────

test('09 S1a.5: Tab signed out goes skip link, home link, theme button, user name, password, Sign in, with nothing in main before the user name', async () => {
  const main = open(noSessionBff(), '#/instances/default')
  const { username, heading } = await signInForm(main)
  expect(tabStops(), 'the Tab stops of the page, in order').toEqual([
    en('skip-to-content'),
    en('app-name'),
    en('theme-dark'),
    en('signin-username'),
    en('signin-password'),
    en('signin-submit'),
  ])
  expect(
    heading.getAttribute('tabindex'),
    'the h1 is not a Tab stop (H7.9)',
  ).toBe('-1')
  const art = main.querySelector('div.signin-art')
  expect(art, 'the drawing is in the page').toBeTruthy()
  expect(
    art!.getAttribute('aria-hidden'),
    'and hidden from a screen reader',
  ).toBe('true')
  expect(
    username.getAttribute('autocomplete'),
    'the user name is the first field of the form',
  ).toBe('username')
  expect(
    document.querySelector('[autofocus]'),
    'nothing takes the focus on load',
  ).toBeNull()
})

// ── S1a.6: title and heading ──────────────────────────────────────────────

test.each(HASHES)(
  '09 S1a.6: signed out at %s the title names the sign-in, the one h1 is Sign in, and nothing names the Overview',
  async (hash) => {
    const main = open(noSessionBff(), hash)
    await signInForm(main)
    expect(document.title, hash).toBe(en('page-title', en('signin-title')))
    const h1 = [...document.querySelectorAll('h1')]
    expect(h1, `${hash}: one h1`).toHaveLength(1)
    expect(text(h1[0]!), `${hash}: the h1`).toBe(en('signin-title'))
    expect(
      document.body.textContent,
      `${hash}: nothing says Overview`,
    ).not.toContain(en('overview-title'))
    expect(
      [...document.querySelectorAll('[aria-label]')]
        .map((el) => el.getAttribute('aria-label') ?? '')
        .filter((name) => name.includes(en('nav-overview'))),
      `${hash}: no accessible name says Overview`,
    ).toEqual([])
  },
)

// ── S1a.7: the notice ─────────────────────────────────────────────────────

test.each([
  '#/instances/x',
  '#/profiles/model',
  '#/profiles/model/x',
  '#/nope',
])(
  '09 S1a.7: at %s the sign-in says why, with its icon of H8b.3',
  async (hash) => {
    const main = open(noSessionBff(), hash)
    await signInForm(main)
    const line = within(main)
      .getByText(en('signin-required'))
      .closest('p.notice')
    expect(line, `${hash}: the notice of today`).toBeTruthy()
    expect(
      [...main.querySelectorAll('p.notice')],
      `${hash}: one notice`,
    ).toHaveLength(1)
    const icon = line!.querySelector('svg')
    expect(icon, `${hash}: the info icon of H8b.3`).toBeTruthy()
    expect(line!.querySelector('img'), 'the icon carries no image').toBeNull()
    expect(
      line!.getAttribute('role'),
      'a plain paragraph: the page says it, it is not announced (H8b.3)',
    ).toBeNull()
  },
)

test.each(['#/', '#/sign-in', ''])(
  '09 S1a.7: at %s no notice: every page needs a sign-in now, and the visitor asked for this one',
  async (hash) => {
    const main = open(noSessionBff(), hash)
    await signInForm(main)
    expect(
      main.querySelectorAll('p.notice'),
      `${hash}: nothing to explain`,
    ).toHaveLength(0)
  },
)

// ── S1a.8: the address is kept ────────────────────────────────────────────

test.each(['#/instances/default', '#/profiles/model', '#/nope'])(
  '09 S1a.8: signed out at %s the address stays what the visitor asked for, and no history entry is added',
  async (hash) => {
    const entries = window.history.length
    const main = open(noSessionBff(), hash)
    await signInForm(main)
    expect(window.location.hash, `${hash}: not rewritten to #/sign-in`).toBe(
      hash,
    )
    expect(window.history.length, `${hash}: no entry added`).toBe(entries)
  },
)

test('09 S1a.8: signed out at the old #/models bookmark the address is still rewritten by 5b.8, with no history entry, and the form says why', async () => {
  const entries = window.history.length
  const bff = todayBff()
  const main = open(bff, '#/models')
  await within(main).findByText(en('signin-required'))
  expect(window.location.hash, 'the bookmark is rewritten').toBe(
    '#/profiles/model',
  )
  expect(window.history.length, 'replaceState adds no entry').toBe(entries)
  expect(bff.calls.map(route), 'and nothing is read').toEqual([
    'GET /api/session',
  ])
})

// ── S1a.9: after a sign-in ────────────────────────────────────────────────

test('09 S1a.9: at #/instances/default a sign-in shows the page of the kept hash in place of the form, with the focus on its h1, and adds no history entry', async () => {
  const entries = window.history.length
  const push = vi.spyOn(window.history, 'pushState')
  const main = open(todayBff(), '#/instances/default')
  await signInForm(main)
  submitSignIn(main)
  const heading = await within(main).findByRole('heading', {
    level: 1,
    name: 'default',
  })
  await waitFor(() => expect(document.activeElement).toBe(heading))
  expect(window.location.hash, 'the address is the kept one').toBe(
    '#/instances/default',
  )
  expect(window.history.length, 'no entry added').toBe(entries)
  expect(push, 'no pushState').not.toHaveBeenCalled()
  expect(
    within(main).queryByRole('heading', { level: 1, name: en('signin-title') }),
    'the form is gone',
  ).toBeNull()
})

test('09 S1a.9: Back after a sign-in at #/instances/default leaves the place the visitor came from, and never meets a sign-in page', async () => {
  const bff = fakeBff({ session: 'signed-out' })
  const main = open(bff, '#/')
  await signInForm(main)
  await typeHash('#/instances/default')
  await signInForm(main)
  const entries = window.history.length
  submitSignIn(main)
  await within(main).findByRole('heading', { level: 1, name: 'default' })
  expect(window.history.length, 'the sign-in added no entry').toBe(entries)
  await pressBack()
  expect(window.location.hash, 'Back leaves the Overview').toBe('#/')
  expect(
    await within(main).findByRole('heading', {
      level: 1,
      name: en('overview-title'),
    }),
    'signed in, so the Overview shows',
  ).toBeTruthy()
  expect(
    within(main).queryByRole('textbox', { name: en('signin-username') }),
  ).toBeNull()
})

test('09 S1a.9: at #/sign-in a sign-in replaces the address with #/ (no entry, no hashchange of its own) and shows the Overview, focus on its h1', async () => {
  const onHash = vi.fn()
  window.addEventListener('hashchange', onHash)
  const main = open(todayBff(), '#/sign-in')
  await signInForm(main)
  // The spies are installed AFTER the open: `open` starts the app with its own
  // replaceState (startAt), and that write is not the app's.
  const entries = window.history.length
  const push = vi.spyOn(window.history, 'pushState')
  const replace = vi.spyOn(window.history, 'replaceState')
  submitSignIn(main)
  const heading = await within(main).findByRole('heading', {
    level: 1,
    name: en('overview-title'),
  })
  expect(window.location.hash, 'the address became the Overview').toBe('#/')
  expect(window.history.length, 'history.length unchanged').toBe(entries)
  expect(push, 'no pushState: no entry').not.toHaveBeenCalled()
  expect(
    replace.mock.calls.map((c) => String(c[2] ?? '')),
    'one replaceState of #/, path kept',
  ).toEqual([`${window.location.pathname}${window.location.search}#/`])
  expect(
    onHash,
    'replaceState fires no hashchange of its own',
  ).not.toHaveBeenCalled()
  await waitFor(() => expect(document.activeElement).toBe(heading))
  window.removeEventListener('hashchange', onHash)
})

test('09 S1a.9: at an unknown hash a sign-in shows the not-found page, as for a signed-in visitor, and keeps the address', async () => {
  const main = open(todayBff(), '#/nope')
  await signInForm(main)
  submitSignIn(main)
  await within(main).findByText(en('not-found'))
  expect(window.location.hash, 'the address is the kept one').toBe('#/nope')
})

// ── S1a.10: #/sign-in while signed in ─────────────────────────────────────

test('09 S1a.10: signed in, a typed #/sign-in is an alias of #/: the address becomes #/ by replaceState with no entry, and the Overview shows with the focus on its h1', async () => {
  const bff = fakeBff()
  const main = open(bff, '#/')
  await within(main).findByRole('heading', {
    level: 1,
    name: en('overview-title'),
  })
  const push = vi.spyOn(window.history, 'pushState')
  const replace = vi.spyOn(window.history, 'replaceState')
  await typeHash('#/sign-in')
  expect(window.location.hash, 'the address is #/').toBe('#/')
  // "the alias adds no entry": the app writes the bar by replaceState only,
  // never by pushState. history.length is not asserted here: jsdom's entry
  // accounting for a fragment typed and then replaced inside the hashchange
  // handler is not the browser's (a bare replaceState there moved jsdom's
  // length by 2, the app's moved it by 0), so it cannot pin this.
  expect(
    push,
    'the app adds no history entry: replaceState only',
  ).not.toHaveBeenCalled()
  expect(
    replace.mock.calls.map((c) => String(c[2] ?? '')).at(-1),
    'replaceState of #/',
  ).toBe(`${window.location.pathname}${window.location.search}#/`)
  const heading = await within(main).findByRole('heading', {
    level: 1,
    name: en('overview-title'),
  })
  expect(
    within(main).queryByRole('textbox', { name: en('signin-username') }),
    'no sign-in form',
  ).toBeNull()
  await waitFor(() => expect(document.activeElement).toBe(heading))
})

test('09 S1a.10: signed in with unsaved edits, a typed #/sign-in is asked about first, and Discard lands on #/', async () => {
  const bff = fakeBff()
  const main = open(bff, '#/instances/default')
  await within(main).findByRole('heading', {
    level: 3,
    name: en('server-network'),
  })
  const port = within(main).getByRole('textbox', {
    name: en('server-port'),
  }) as HTMLInputElement
  fireEvent.change(port, { target: { value: '8581' } })
  await typeHash('#/sign-in')
  const dialog = await screen.findByRole('dialog')
  expect(
    within(dialog).getByText(en('discard-title')),
    '38 asks before the address moves',
  ).toBeTruthy()
  expect(window.location.hash, 'the address is held back').toBe(
    '#/instances/default',
  )
  fireEvent.click(
    within(dialog).getByRole('button', { name: en('discard-confirm') }),
  )
  expect(window.location.hash, 'Discard lands on the Overview').toBe('#/')
  await within(main).findByRole('heading', {
    level: 1,
    name: en('overview-title'),
  })
  expect(
    bff.to('PUT', '/api/instances/default/server'),
    'Discard saves nothing',
  ).toHaveLength(0)
})

// ── S1a.10 (cont.): the alias read by the nav, and the arrival that moves ──
// ── the address, not only the bar                                    ──────

test('09 S1a.10: at #/sign-in signed in, the nav reads the alias as the Overview: its Overview link is the current page', async () => {
  const bff = fakeBff()
  const main = open(bff, '#/sign-in')
  const heading = await within(main).findByRole('heading', {
    level: 1,
    name: en('overview-title'),
  })
  expect(window.location.hash, 'the alias effect moved the bar').toBe('#/')
  expect(
    within(main).queryByRole('textbox', { name: en('signin-username') }),
    'no sign-in form',
  ).toBeNull()
  // The route, the page and the nav all read the alias as the Overview, so the
  // nav says so: the Overview link is the current page, as at #/.
  const navEl = await screen.findByRole('navigation', {
    name: en('nav-label'),
  })
  expect(
    within(navEl)
      .getByRole('link', { name: en('nav-overview') })
      .getAttribute('aria-current'),
    'the nav reads the alias as the Overview',
  ).toBe('page')
  await waitFor(() => expect(document.activeElement).toBe(heading))
})

test('09 S1a.10: signed in on a clean instance page, a typed #/sign-in moves the app to the Overview and the bar with it, with no entry', async () => {
  const bff = fakeBff()
  const main = open(bff, '#/instances/default')
  await within(main).findByRole('heading', {
    level: 3,
    name: en('server-network'),
  })
  const push = vi.spyOn(window.history, 'pushState')
  const replace = vi.spyOn(window.history, 'replaceState')
  // The arrival that MOVES the address (the app is at the instance page): the
  // hashchange handler sets the Overview and writes the bar in the same step.
  await typeHash('#/sign-in')
  expect(window.location.hash, 'the bar is the Overview').toBe('#/')
  expect(
    push,
    'the alias adds no history entry: replaceState only',
  ).not.toHaveBeenCalled()
  expect(
    replace.mock.calls.map((c) => String(c[2] ?? '')).at(-1),
    'replaceState of #/',
  ).toBe(`${window.location.pathname}${window.location.search}#/`)
  const heading = await within(main).findByRole('heading', {
    level: 1,
    name: en('overview-title'),
  })
  expect(
    within(main).queryByRole('textbox', { name: en('signin-username') }),
    'no sign-in form',
  ).toBeNull()
  await waitFor(() => expect(document.activeElement).toBe(heading))
})

// ── S1a.11: Sign out ──────────────────────────────────────────────────────

test('09 S1a.11: Sign out lands on the sign-in page at #/, with no notice, the focus on its h1, and the nav and Menu button gone', async () => {
  const bff = fakeBff()
  const main = open(bff, '#/profiles/model')
  await within(main).findByRole('heading', {
    level: 1,
    name: en('model-profiles-title'),
  })
  const entries = window.history.length
  clickSignOut()
  const heading = await within(main).findByRole('heading', {
    level: 1,
    name: en('signin-title'),
  })
  expect(window.location.hash, 'the app goes to #/ as today').toBe('#/')
  expect(window.history.length, 'the go of today pushes one entry').toBe(
    entries + 1,
  )
  expect(
    main.querySelectorAll('p.notice'),
    'no notice at #/ (S1a.7)',
  ).toHaveLength(0)
  expect(document.querySelector('nav'), 'the nav is gone').toBeNull()
  expect(
    document.getElementById('menu-button'),
    'the Menu button is gone',
  ).toBeNull()
  expect(heading.id, 'the focus goes to the page heading').toBe('page-heading')
  await waitFor(() => expect(document.activeElement).toBe(heading))
  expect(bff.to('DELETE', '/api/session'), 'the session is ended').toHaveLength(
    1,
  )
})
