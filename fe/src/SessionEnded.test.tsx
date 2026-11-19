// fe/src/SessionEnded.test.tsx
// 09 S1b of fe/briefs/09-signin-only.md section 2: a session that ends while
// signed in on a page WITHOUT a form. The DOM plane: S1b.1 the Overview's
// three reads, S1b.2 the answers that are not a session end, S1b.3 the notice
// and when it shows, S1b.5 its precedence, S1b.6 what does not move, and the
// S1a.10 hashchange record the S1a review asked for. The review of S1b
// (section 8) adds its two conditions: C1, the instance page whose first read
// is the list, and C2, its boot read, which keeps the field it belongs to.
//
// The string of S1b.4 is pinned by "1a strings" in App.test.tsx, which
// compares EN in src/test/bff.ts (it now holds `signin-session-ended`) with
// strings/en_US.json: the English the brief gives is in the strings file, and
// only there, when the slice is done.
//
// The whole app renders against a fake bff (mocked fetch). Today, the
// designer's baseline of brief section 7, a 401 on those three reads leaves
// the Overview standing with "The version could not be read from the
// service." and "The instances could not be read.", the nav and Sign out kept:
// S1b.1 is what replaces that, so the session ends come first.
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  within,
} from '@testing-library/react'
import { afterEach, beforeEach, expect, test, vi } from 'vitest'
import App from './App.tsx'
import {
  before,
  deferred,
  en,
  fakeBff,
  INSTANCES,
  MODEL_PROFILES,
  modelProfileDoc,
  reply,
  serverDoc,
  startAt,
  stubDialog,
  type BffOptions,
  type Call,
} from './test/bff.ts'

/** 401 `session-required`: the contract's session end, by status (S1b.1). */
const ENDED = () => reply(401, { key: 'session-required', args: [] })
/** 403 `session-not-administrator`: the manager ended the session (01 S1.9). */
const REVOKED = () => reply(403, { key: 'session-not-administrator', args: [] })

/** The Overview's three reads (S1b.1), as `route` spells them. */
const VERSION = 'GET /api/version'
const LIST = 'GET /api/instances'
const TILE_BOOT = 'GET /api/instances/default/boot'

const INSTANCE = '#/instances/default'

let dialogs: ReturnType<typeof stubDialog>

beforeEach(() => {
  dialogs = stubDialog()
})

afterEach(() => {
  cleanup()
  vi.useRealTimers()
  vi.unstubAllGlobals()
  dialogs.restore()
  delete (window as unknown as { isSecureContext?: boolean }).isSecureContext
  startAt('')
})

// ── Helpers ───────────────────────────────────────────────────────────────

/** Fake timers for the polls; setImmediate and microtasks stay real. */
function fakeClock() {
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

/**
 * Lets every answer and render that is due now land. The clock moves a
 * millisecond at a time, so a `findBy` of the testing library, which polls on
 * an interval, answers on a fake clock too.
 */
async function flush() {
  await act(async () => {
    for (let i = 0; i < 40; i++) await vi.advanceTimersByTimeAsync(1)
  })
}

async function advance(ms: number) {
  await act(async () => {
    await vi.advanceTimersByTimeAsync(ms)
  })
  await flush()
}

/** The route of a call, as one word. */
const route = (c: Call): string => `${c.method} ${c.path}`

/** How many calls to one route the bff saw. */
function reads(bff: ReturnType<typeof fakeBff>, path: string): number {
  return bff.calls.filter((c) => route(c) === path).length
}

/**
 * The bff of a session that is gone: every protected read answers 401
 * `session-required`, the answer of the whole bff and not of one route (the
 * bff after item 13a, fe/briefs/api-contract.md). A successful
 * `POST /api/session` ends that: from then on every route answers what
 * `answers` says, the signed-in answers of the fake bff by default, so the
 * sign-in of S1b.1 ("after a sign-in the Overview reads again") reads a page
 * that works.
 *
 * With `oneRead`, only the routes the test names answer the 401 and the rest
 * read as a signed-in page reads them today: the row that is about one read
 * pins that one read.
 */
function endedBff(answers: BffOptions = {}, oneRead = false) {
  let gone = true
  const asUsual: BffOptions = {
    version: () => reply(200, { version: '2.99.0' }),
    instances: INSTANCES,
    modelProfiles: MODEL_PROFILES,
    modelProfile: () => reply(200, modelProfileDoc()),
    server: () => reply(200, serverDoc()),
    boot: { read: () => reply(200, { enabled: false }) },
    run: () => reply(204),
    save: () => reply(200, { revision: 'r-2', running: false }),
  }
  const merged = oneRead ? answers : { ...asUsual, ...answers }
  const gate =
    (after: unknown) =>
    (...args: unknown[]): unknown =>
      gone
        ? ENDED()
        : typeof after === 'function'
          ? (after as (...a: unknown[]) => unknown)(...args)
          : // A plain answer (the shared tables) is a body, not a Response:
            // the fake's contract takes a Response, so wrap it (a raw array
            // lands as a fetch that resolves to an array and reads as a
            // failed body).
            reply(200, after)
  const { boot, ...rest } = merged
  const g: Record<string, unknown> = {}
  for (const [key, after] of Object.entries(rest)) g[key] = gate(after)
  if (boot)
    g.boot = {
      read: boot.read ? gate(boot.read) : undefined,
      write: boot.write ? gate(boot.write) : undefined,
    }
  const bff = fakeBff(g as BffOptions)
  const inner = globalThis.fetch
  vi.stubGlobal('fetch', (input: RequestInfo | URL, init?: RequestInit) => {
    if (
      (init?.method ?? 'GET').toUpperCase() === 'POST' &&
      String(input) === '/api/session'
    )
      return inner(input, init).then((r) => {
        if (r.ok) gone = false
        return r
      })
    return inner(input, init)
  })
  return bff
}

/**
 * The bff of today for a visitor with no session, which is what a first load
 * signed out meets (the shape of the helper of SignInOnly.test.tsx: a test
 * file imports no other test file).
 */
function noSessionBff(options: BffOptions = {}) {
  return fakeBff({ session: 'signed-out', ...options })
}

/** The shape of `fakeBff`. */
type Bff = ReturnType<typeof fakeBff>

/**
 * The app at `hash` with `bff` (a `fakeBff` installs itself as
 * `globalThis.fetch`), on a fake clock.
 */
async function at(bff: Bff, hash = '#/') {
  fakeClock()
  startAt(hash)
  render(<App />)
  await flush()
  return { bff, main: screen.getByRole('main') }
}

/**
 * The app at `hash` with `bff`, on the real clock: for the cases that move
 * the address, which the browser answers in a task.
 */
async function atReal(bff: Bff, hash = '#/') {
  startAt(hash)
  render(<App />)
  await act(async () => {
    for (let i = 0; i < 20; i++) await new Promise((r) => setTimeout(r, 0))
  })
  return { bff, main: screen.getByRole('main') }
}

/** Types `hash` in the address bar: jsdom fires the hashchange in a task. */
async function typeHash(hash: string) {
  await act(async () => {
    window.location.hash = hash
    await new Promise((r) => setTimeout(r, 0))
  })
}

/**
 * Waits for `find` to answer, on the fake clock: a `findBy` of the testing
 * library polls on that clock, so nothing would move it forward.
 */
async function appear<T>(find: () => T): Promise<T> {
  let last: unknown = new Error('never appeared')
  for (let i = 0; i < 6; i++) {
    try {
      return find()
    } catch (e) {
      last = e
    }
    await flush()
  }
  throw last
}

/** The sign-in page in `main`, by its h1. */
function signInHeading(main: HTMLElement): HTMLElement {
  return within(main).getByRole('heading', {
    level: 1,
    name: en('signin-title'),
  })
}

/** Signs in through the form in `main`; the answer lands. */
async function signIn(main: HTMLElement, user = 'alice') {
  fireEvent.change(
    within(main).getByRole('textbox', {
      name: en('signin-username'),
    }) as HTMLInputElement,
    { target: { value: user } },
  )
  fireEvent.change(
    within(main).getByLabelText(en('signin-password'), {
      selector: 'input',
    }) as HTMLInputElement,
    { target: { value: 'hunter2' } },
  )
  fireEvent.click(
    within(main).getByRole('button', { name: en('signin-submit') }),
  )
  await flush()
}

/** Signs out through the banner. */
function clickSignOut() {
  fireEvent.click(
    within(screen.getByRole('banner')).getByRole('button', {
      name: en('session-sign-out'),
    }),
  )
}

/** The visible text of an element, spaces collapsed. */
function text(el: Element): string {
  return (el.textContent ?? '').replace(/\s+/g, ' ').trim()
}

/** The notices of `root`, in the order they show (the sign-in page's, S1b.3). */
function notices(root: Element): Element[] {
  return [...root.querySelectorAll('p.notice')]
}

/** Their texts: one word per notice, for "exactly this one shows". */
function noticeTexts(root: Element): string[] {
  return notices(root).map(text)
}

/** The live regions of `root` (WCAG 4.1.3): alerts, statuses, aria-live. */
function liveRegions(root: Element): string[] {
  return [
    ...root.querySelectorAll('[role="alert"], [role="status"], [aria-live]'),
  ].map(text)
}

/** `[tag, every attribute]` of an element: the shape of one child of an icon. */
function childOf(el: Element): [string, Record<string, string>] {
  return [
    el.tagName.toLowerCase(),
    Object.fromEntries([...el.attributes].map((a) => [a.name, a.value])),
  ]
}

/** The `triangle-alert` of the brief's shapes block (08, H8b.3): the outline
 * and the exclamation, the shape `Icon.tsx` ships (HearthIcons.test.tsx). */
const TRIANGLE_ALERT: [string, Record<string, string>][] = [
  [
    'path',
    {
      d: 'm21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3',
    },
  ],
  ['path', { d: 'M12 9v4' }],
  ['path', { d: 'M12 17h.01' }],
]

/** Sets the browser's `window.isSecureContext` (brief 03 does the same). */
function secure(value: boolean) {
  ;(window as unknown as { isSecureContext: boolean }).isSecureContext = value
}

/** The Instances section's list of tiles (the helper of RunControls). */
function tilesOf(main: HTMLElement): HTMLElement {
  const heading = within(main).getByRole('heading', {
    level: 2,
    name: en('instances-title'),
  })
  const section = heading.closest('section') as HTMLElement
  expect(section, 'the Instances section').toBeTruthy()
  return within(section).getByRole('list')
}

/** What the signed-out shell must not offer (09 S1a.4). */
function expectShellGone() {
  expect(
    screen.queryByRole('navigation', { name: en('nav-label') }),
    'no nav',
  ).toBeNull()
  const banner = screen.getByRole('banner')
  expect(
    within(banner).queryByRole('button', { name: en('session-sign-out') }),
    'no Sign out',
  ).toBeNull()
  expect(
    within(banner).queryByRole('button', { name: en('nav-menu') }),
    'no Menu button',
  ).toBeNull()
}

/** The banner's Sign out: the session is still there. */
function expectSignedInBanner() {
  expect(
    within(screen.getByRole('banner')).getByRole('button', {
      name: en('session-sign-out'),
    }),
    'the session is kept',
  ).toBeTruthy()
}

// ── S1b.1: the Overview's reads end the session ───────────────────────────

const SESSION_ENDS: [string, string, BffOptions][] = [
  ['the framework version', VERSION, { version: ENDED }],
  ['the instances', LIST, { instances: ENDED }],
  ['a tile boot state', TILE_BOOT, { boot: { read: ENDED } }],
]

test.each(SESSION_ENDS)(
  '09 S1b.1: signed in on the Overview, a 401 on %s signs the app out: the sign-in page with the notice at #/, no nav, no Menu button, no dialog, the focus on its h1',
  async (_what, path, options) => {
    const { bff, main } = await at(endedBff(options, true))
    expect(reads(bff, path), `${path}: the page read it`).toBeGreaterThan(0)
    const heading = signInHeading(main)
    expect(
      within(main).getByText(en('signin-session-ended')),
      'the notice of S1b.3 says why',
    ).toBeTruthy()
    expect(window.location.hash, 'the hash stays #/').toBe('#/')
    expect(document.activeElement, 'the focus on the h1').toBe(heading)
    expectShellGone()
    expect(screen.queryByRole('dialog'), 'no dialog').toBeNull()
    expect(
      within(main).queryByRole('heading', {
        level: 2,
        name: en('instances-title'),
      }),
      'the Overview is gone, not left standing above the form',
    ).toBeNull()
  },
)

test('09 S1b.1: the three reads answering 401 together give one sign-in page, one notice and one focus move, and the app stops reading', async () => {
  const moves: EventTarget[] = []
  const onFocus = (e: FocusEvent) => e.target && moves.push(e.target)
  document.addEventListener('focusin', onFocus)
  const { bff, main } = await at(endedBff())
  document.removeEventListener('focusin', onFocus)
  const heading = signInHeading(main)
  expect(
    within(main).queryAllByRole('heading', {
      level: 1,
      name: en('signin-title'),
    }),
    'one sign-in page',
  ).toHaveLength(1)
  expect(noticeTexts(main), 'one notice').toEqual([en('signin-session-ended')])
  expect(
    moves.filter((t) => t === heading),
    'one focus move, onto the h1',
  ).toHaveLength(1)
  const seen = bff.calls.length
  await advance(10_000)
  expect(bff.calls.length, 'nothing reads on: no retry loop').toBe(seen)
  expect(noticeTexts(main), 'still one notice').toEqual([
    en('signin-session-ended'),
  ])
  expect(document.activeElement, 'the focus stays on the h1').toBe(heading)
})

test('09 S1b.1: after the notice, a sign-in reads the Overview again, and the notice goes', async () => {
  const { bff, main } = await at(endedBff())
  expect(within(main).getByText(en('signin-session-ended'))).toBeTruthy()
  await signIn(main)
  expect(
    within(main).queryByText(en('signin-session-ended')),
    'gone at a successful sign-in (S1b.3)',
  ).toBeNull()
  await appear(() => screen.getByRole('navigation', { name: en('nav-label') }))
  expect(
    within(main).getByText(en('version-label', '2.99.0')),
    'the version is read again',
  ).toBeTruthy()
  // The tiles' read answers after the version's: await the list (the
  // synchronous query races it, as the loading state of S1a.3).
  const tiles = await appear(() => tilesOf(main))
  expect(text(tiles), 'the tiles are read again').toContain('default')
  expect(
    reads(bff, VERSION),
    'the Overview read more than once',
  ).toBeGreaterThan(1)
})

// ── S1b.1 C1: the instance page, whose first read is the list ─────────────

/** The instance page's boot field group, once its read has answered. */
function bootGroup(main: HTMLElement): HTMLElement {
  return within(main).getByRole('group', { name: en('boot-label') })
}

/** The Server form's Port box. */
function portBox(main: HTMLElement): HTMLInputElement {
  return within(
    within(main).getByRole('group', { name: en('server-port') }),
  ).getByRole('textbox', { name: en('server-port') }) as HTMLInputElement
}

/** Types `value` in `input`, as a visitor does. */
function typeIn(input: HTMLElement, value: string) {
  fireEvent.change(input, { target: { value } })
}

test('09 S1b.1 (C1): signed in on the instance page, a session gone at opening signs the app out at that address: the sign-in page with the notice, the focus on its h1, nothing read on', async () => {
  // The page's first read is GET /api/instances (the polls of S1b.2 are the
  // later ones), so with the whole bff answering 401 this is the read that
  // ends the session: the brief's S1b.3 named only the server read, which
  // here is never even sent (designer's review, 8 C1).
  const { bff, main } = await at(endedBff(), INSTANCE)
  expect(reads(bff, LIST), 'the page read the list').toBeGreaterThan(0)
  const heading = signInHeading(main)
  expect(
    within(main).queryByText(en('instances-failed')),
    'not the list left standing with a message of its own over a page of a session that is gone',
  ).toBeNull()
  expect(noticeTexts(main), 'one notice, the one that says why').toEqual([
    en('signin-session-ended'),
  ])
  expect(window.location.hash, 'the address it was at').toBe(INSTANCE)
  expect(document.activeElement, 'the focus on the h1').toBe(heading)
  expectShellGone()
  expect(screen.queryByRole('dialog'), 'no dialog').toBeNull()
  const seen = bff.calls.length
  await advance(10_000)
  expect(bff.calls.length, 'nothing reads on: no retry loop').toBe(seen)
  expect(reads(bff, LIST), 'no poll of a list of a page that is gone').toBe(1)
  expect(noticeTexts(main), 'still one notice').toEqual([
    en('signin-session-ended'),
  ])
})

// ── S1b.2: an answer that is not a session end ────────────────────────────

const NOT_AN_END: [string, string, BffOptions, string][] = [
  [
    '403 session-not-administrator',
    VERSION,
    { version: REVOKED },
    en('version-unavailable'),
  ],
  [
    '403 client-not-authorized',
    LIST,
    { instances: () => reply(403, { key: 'client-not-authorized', args: [] }) },
    en('instances-not-authorized'),
  ],
  [
    '502 client-not-authorized',
    LIST,
    { instances: () => reply(502, { key: 'client-not-authorized', args: [] }) },
    en('instances-not-authorized'),
  ],
  [
    '503 client-unavailable',
    VERSION,
    { version: () => reply(503, { key: 'client-unavailable', args: [] }) },
    en('version-unavailable'),
  ],
  [
    '404 client-no-instance',
    TILE_BOOT,
    {
      boot: {
        read: () => reply(404, { key: 'client-no-instance', args: [] }),
      },
    },
    en('instance-state-active'),
  ],
  [
    'a malformed body',
    LIST,
    { instances: () => reply(200, 'secret-detail') },
    en('instances-failed'),
  ],
  [
    'a network failure',
    VERSION,
    { version: () => Promise.reject(new TypeError('secret-detail')) },
    en('version-unavailable'),
  ],
]

test.each(NOT_AN_END)(
  '09 S1b.2: on the Overview, %s on %s keeps the handling of the place (%s) and the session: no sign-in page and no notice',
  async (_what, path, options, shown) => {
    const { bff, main } = await at(fakeBff(options))
    expect(reads(bff, path), `${path}: the page read it`).toBeGreaterThan(0)
    expect(
      within(main).getByText(shown),
      'the place keeps its own message',
    ).toBeTruthy()
    expect(
      within(main).queryByRole('heading', {
        level: 1,
        name: en('signin-title'),
      }),
      'the page is not replaced',
    ).toBeNull()
    expect(noticeTexts(main), 'none of the three notices').toEqual([])
    expectSignedInBanner()
    expect(
      screen.getByRole('navigation', { name: en('nav-label') }),
      'the nav stays',
    ).toBeTruthy()
  },
)

test('09 S1b.2: on the instance page, the polled GET /api/instances answering 401 changes nothing: the form and the run controls stay, the session stays, no notice', async () => {
  let times = 0
  const { bff, main } = await at(
    fakeBff({
      instances: () => (++times === 1 ? reply(200, INSTANCES) : ENDED()),
    }),
    INSTANCE,
  )
  expect(
    within(main).getByRole('heading', { level: 3, name: en('server-network') }),
    'the Server form is shown',
  ).toBeTruthy()
  const before = bff.calls.length
  await advance(5_000)
  expect(reads(bff, LIST), 'the poll read the list again').toBe(2)
  expect(
    within(main).queryByRole('heading', {
      level: 1,
      name: en('signin-title'),
    }),
    'no sign-in page',
  ).toBeNull()
  expect(noticeTexts(main), 'no notice').toEqual([])
  expect(
    within(main).getByRole('heading', { level: 3, name: en('server-network') }),
    'the form is where it was',
  ).toBeTruthy()
  expect(
    within(main).getByRole('button', { name: en('run-stop') }),
    'the run controls are where they were',
  ).toBeTruthy()
  expectSignedInBanner()
  expect(
    bff.calls.length,
    'the failed poll sent nothing else (36)',
  ).toBeLessThanOrEqual(before + 1)
})

test('09 S1b.6: a boot read of the instance page that fails for another reason keeps its own paragraph (bootReadText) and the session', async () => {
  const { bff, main } = await at(
    fakeBff({
      boot: { read: () => reply(503, { key: 'client-unavailable', args: [] }) },
    }),
    INSTANCE,
  )
  expect(reads(bff, TILE_BOOT), 'the page read the boot state').toBeGreaterThan(
    0,
  )
  const group = within(main).getByRole('group', { name: en('boot-label') })
  const p = within(group).getByText(en('api-client-unavailable'))
  expect(p.tagName, 'the read paragraph of the field, as today (45a)').toBe('P')
  expect(
    p.getAttribute('role'),
    'a plain paragraph: the failed read announces nothing (S1b.6)',
  ).toBeNull()
  expect(
    within(main).queryByRole('heading', {
      level: 1,
      name: en('signin-title'),
    }),
    'the page is not replaced',
  ).toBeNull()
  expect(noticeTexts(main), 'no notice').toEqual([])
  expectSignedInBanner()
})

test('09 S1b.6 (C2): on the instance page, a 401 on its boot read keeps the page and the session: the field shows its own failed-read paragraph, and the Port typed while it was awaited stays', async () => {
  // The designer's review, 8 C2: the boot read is not the page's first read
  // and not a form, so it keeps today's handling of a failed read (S1b.6,
  // `bootReadText`). Signing out there threw away a Port typed while the read
  // was on its way, with no question (fe/DESIGN.md 2.2, WCAG 3.3.7).
  const pending = deferred()
  const { bff, main } = await at(
    fakeBff({ boot: { read: () => pending.promise } }),
    INSTANCE,
  )
  expect(
    within(main).getByRole('heading', { level: 3, name: en('server-network') }),
    'the Server form is shown',
  ).toBeTruthy()
  expect(
    within(main).queryByRole('group', { name: en('boot-label') }),
    'the field waits for its read',
  ).toBeNull()
  typeIn(portBox(main), '8581')
  await advance(3_000)
  await act(async () => {
    pending.resolve(ENDED())
  })
  await flush()
  const group = bootGroup(main)
  const p = within(group).getByText(en('boot-load-failed'))
  expect(p.tagName, 'the read paragraph of the field, as any failed read').toBe(
    'P',
  )
  expect(
    p.getAttribute('role'),
    'a plain paragraph: the failed read announces nothing (S1b.6)',
  ).toBeNull()
  expect(
    within(main).queryByRole('heading', {
      level: 1,
      name: en('signin-title'),
    }),
    'the page is not replaced by the sign-in page',
  ).toBeNull()
  expect(
    within(main).getByRole('heading', { level: 3, name: en('server-network') }),
    'the form is where it was',
  ).toBeTruthy()
  expect(portBox(main).value, 'the typed Port stays').toBe('8581')
  expect(
    within(main).getByText(en('save-unsaved')),
    'the page is still asking about its unsaved changes',
  ).toBeTruthy()
  expect(noticeTexts(main), 'none of the three notices').toEqual([])
  expectSignedInBanner()
  expect(bff.to('DELETE', '/api/session'), 'nothing signed out').toHaveLength(0)
  expect(reads(bff, TILE_BOOT), 'the failed boot read is not retried').toBe(1)
})

// ── S1b.3: the notice ─────────────────────────────────────────────────────

test('09 S1b.3: the notice is a p.notice with role=alert and the triangle-alert icon, the text exactly, in the place of signin-required, with the focus on the h1', async () => {
  const { main } = await at(endedBff())
  const heading = signInHeading(main)
  const list = notices(main)
  expect(list, 'one notice').toHaveLength(1)
  const p = list[0]!
  expect(text(p), 'the text is the string, and the icon adds nothing').toBe(
    en('signin-session-ended'),
  )
  expect(p.tagName, 'a paragraph').toBe('P')
  expect(p.classList.contains('notice'), 'the .notice look').toBe(true)
  expect(
    p.getAttribute('role'),
    'announced, as the revoked notice is (4.1.3)',
  ).toBe('alert')
  expect(p.querySelector('img'), 'no image').toBeNull()
  const svgs = p.querySelectorAll('svg')
  expect(svgs, 'one icon').toHaveLength(1)
  const icon = svgs[0]!
  expect(p.firstElementChild, 'the icon is the first child').toBe(icon)
  expect(icon.getAttribute('class'), 'the shared icon class').toBe('icon')
  expect(icon.getAttribute('aria-hidden'), 'decorative').toBe('true')
  expect(icon.getAttribute('focusable'), 'takes no tab place').toBe('false')
  expect(
    [...icon.children].map(childOf),
    'the triangle-alert of H8b.3',
  ).toEqual(TRIANGLE_ALERT)
  expect(
    within(main).queryByText(en('signin-required')),
    'in the place of signin-required, not beside it',
  ).toBeNull()
  expect(document.activeElement, 'the focus on the h1').toBe(heading)
})

test('09 S1b.3: the notice is before the plain-HTTP notice, which keeps its plain paragraph of brief 03 (I4)', async () => {
  secure(false)
  const { main } = await at(endedBff())
  const ended = within(main).getByText(en('signin-session-ended'))
  const insecure = within(main).getByText(en('signin-insecure'))
  expect(noticeTexts(main), 'both notices, in the brief order').toEqual([
    en('signin-session-ended'),
    en('signin-insecure'),
  ])
  expect(before(ended, insecure), 'the new notice first').toBe(true)
  expect(
    insecure.getAttribute('role'),
    'the plain-HTTP notice stays unannounced (03 I4)',
  ).toBeNull()
  expect(ended.getAttribute('role'), 'the new notice is announced').toBe(
    'alert',
  )
})

test('09 S1b.3: the notice never shows after Sign out, and never on a first load signed out', async () => {
  const { main } = await at(fakeBff(), INSTANCE)
  await appear(() =>
    within(screen.getByRole('banner')).getByRole('button', {
      name: en('session-sign-out'),
    }),
  )
  clickSignOut()
  await flush()
  expect(window.location.hash, 'Sign out lands at #/ (S1a.11)').toBe('#/')
  expect(signInHeading(main), 'the sign-in page').toBeTruthy()
  expect(
    noticeTexts(main),
    'a Sign out of your own shows no notice at all, so not the new one',
  ).toEqual([])
  cleanup()
  vi.unstubAllGlobals()
  // The same signed out from the page Sign out lands on: the Overview has no
  // form of its own, so this is the page of this slice, and the sign-out ends
  // the session there.
  const { main: fromHome } = await at(fakeBff(), '#/')
  await appear(() =>
    within(screen.getByRole('banner')).getByRole('button', {
      name: en('session-sign-out'),
    }),
  )
  clickSignOut()
  await flush()
  expect(window.location.hash, 'and it stays at #/').toBe('#/')
  expect(signInHeading(fromHome), 'the sign-in page').toBeTruthy()
  expect(
    noticeTexts(fromHome),
    'a Sign out at the Overview shows no notice at all',
  ).toEqual([])
  expectShellGone()
  cleanup()
  vi.unstubAllGlobals()
  const { main: signedOut } = await at(noSessionBff(), INSTANCE)
  expect(signInHeading(signedOut)).toBeTruthy()
  expect(
    within(signedOut).queryByText(en('signin-session-ended')),
    'a first load signed out never says "your session has ended"',
  ).toBeNull()
  expect(noticeTexts(signedOut), 'the plain notice of the asked page').toEqual([
    en('signin-required'),
  ])
})

test('09 S1b.3: the reads of the server page, the model profiles list and one model profile, answering 401, show the notice at the address of that page, and it goes at the next address change', async () => {
  const { main: server } = await at(endedBff({ server: ENDED }, true), INSTANCE)
  expect(window.location.hash, 'the address it was at').toBe(INSTANCE)
  expect(within(server).getByText(en('signin-session-ended'))).toBeTruthy()
  expect(
    within(server).queryByRole('heading', {
      level: 3,
      name: en('server-network'),
    }),
    'the form is gone with the session',
  ).toBeNull()
  // An address change on a fake clock: `typeHash` waits on a real
  // `setTimeout`, which the fake clock never moves. The hash is set in an
  // act, jsdom's hashchange task runs on `setImmediate` (not faked), and the
  // renders land on `flush`. A protected address: at `#/` no notice shows at
  // all (S1a.7), so the cleared "ended" is seen beside the plain "required".
  await act(async () => {
    window.location.hash = '#/profiles/model'
    await new Promise((r) => setImmediate(r))
  })
  await flush()
  expect(window.location.hash, 'the address changed').toBe('#/profiles/model')
  expect(within(server).getByText(en('signin-required'))).toBeTruthy()
  expect(
    within(server).queryByText(en('signin-session-ended')),
    'cleared at the address change',
  ).toBeNull()
  cleanup()
  vi.unstubAllGlobals()
  // The rest mounts on the real clock (`atReal`): the fake clock of the
  // first mount would stall its `setTimeout` loop into a timeout.
  vi.useRealTimers()
  const { main: list } = await atReal(
    endedBff({ modelProfiles: ENDED }, true),
    '#/profiles/model',
  )
  expect(within(list).getByText(en('signin-session-ended'))).toBeTruthy()
  expect(window.location.hash).toBe('#/profiles/model')
  cleanup()
  vi.unstubAllGlobals()
  const { main: one } = await atReal(
    endedBff({ modelProfile: ENDED }, true),
    '#/profiles/model/Qwen3-27B',
  )
  expect(within(one).getByText(en('signin-session-ended'))).toBeTruthy()
  expect(window.location.hash).toBe('#/profiles/model/Qwen3-27B')
})

// ── S1b.5: precedence ─────────────────────────────────────────────────────

const ONE_OF_THE_THREE: [
  string,
  string,
  () => Promise<{ main: HTMLElement }>,
][] = [
  [
    'a read 401 ends a session signed in on the Overview',
    en('signin-session-ended'),
    () => at(endedBff(), '#/'),
  ],
  [
    'the manager ended the session (a read 403 session-not-administrator)',
    en('session-revoked'),
    () => at(fakeBff({ server: REVOKED, boot: { read: REVOKED } }), INSTANCE),
  ],
  [
    'a first load with no session at a protected address',
    en('signin-required'),
    () => at(noSessionBff(), INSTANCE),
  ],
]

test.each(ONE_OF_THE_THREE)(
  '09 S1b.5: %s: exactly one of the three notices shows, %s',
  async (_what, expected, build) => {
    const { main } = await build()
    expect(noticeTexts(main), 'one notice, the one that says why').toEqual([
      expected,
    ])
  },
)

test('09 S1b.5: where one read answers 403 session-not-administrator and another 401, the revoked notice wins: only it shows', async () => {
  const { main } = await at(
    fakeBff({ server: REVOKED, boot: { read: ENDED } }),
    INSTANCE,
  )
  expect(noticeTexts(main), 'the revoked notice alone').toEqual([
    en('session-revoked'),
  ])
  expect(
    within(main).queryByText(en('signin-session-ended')),
    'S1b.3 never joins a revoked session',
  ).toBeNull()
})

// ── S1b.6: nothing else moves ─────────────────────────────────────────────

test('09 S1b.6: the tiles of the Overview keep their markup and carry no live region of their own (H6b.4)', async () => {
  const { main } = await at(fakeBff())
  const tiles = tilesOf(main)
  expect(text(tiles), 'the tiles are shown').toContain('default')
  expect(liveRegions(tiles), 'the tiles announce nothing').toEqual([])
  expect(
    within(tiles).getAllByRole('listitem'),
    'one tile per instance, as H6b.4 pins their markup',
  ).toHaveLength(2)
})

test('09 S1b.6: a run verb that answers 401 keeps its alert and its Sign in, opens no notice of S1b.3, and its dialog keeps the page under it (36, 43a)', async () => {
  const { bff, main } = await at(fakeBff({ run: ENDED }), INSTANCE)
  fireEvent.click(within(main).getByRole('button', { name: en('run-stop') }))
  await advance(1_000)
  expect(
    within(main).getByText(en('session-ended-retry')),
    'the run alert, as today (43a)',
  ).toBeTruthy()
  expect(noticeTexts(main), 'an action 401 opens no notice').toEqual([])
  const ask = within(main).getByRole('button', { name: en('session-sign-in') })
  const alert = within(main).getByText(en('session-ended-retry'))
  expect(before(alert, ask), 'the Sign in after the alert, as today').toBe(true)
  fireEvent.click(ask)
  await flush()
  const dialog = screen.getByRole('dialog')
  expect(
    within(dialog).getByRole('textbox', {
      name: en('signin-username'),
    }),
    'the dialog holds the sign-in form',
  ).toBeTruthy()
  expect(
    within(dialog).queryByText(en('signin-session-ended')),
    'and no notice: the visitor is not signed out (43a)',
  ).toBeNull()
  expect(
    within(main).getByRole('heading', { level: 3, name: en('server-network') }),
    'the page stays under the dialog',
  ).toBeTruthy()
  expectSignedInBanner()
  expect(bff.to('DELETE', '/api/session'), 'nothing signed out').toHaveLength(0)
})

// ── S1a.10: the hashchange record ─────────────────────────────────────────

test('09 S1a.10: signed in, typing an address in the bar records one hashchange, and the alias page shows', async () => {
  // One event is the record the browser needs. An in-app arrival that set
  // `location.hash` itself would add an entry with no pushState and no event
  // of its own, and Back would land on a page that never showed.
  const seen: string[] = []
  const onHash = () => seen.push(window.location.hash)
  window.addEventListener('hashchange', onHash)
  const { main } = await atReal(fakeBff())
  await screen.findByRole('navigation', { name: en('nav-label') })
  await typeHash('#/sign-in')
  window.removeEventListener('hashchange', onHash)
  expect(seen, 'the typed address, and only it').toEqual(['#/sign-in'])
  expect(
    within(main).queryByRole('heading', {
      level: 1,
      name: en('signin-title'),
    }),
    'signed in, #/sign-in is the Overview (09 S1a.10)',
  ).toBeNull()
  expect(window.location.hash, 'the alias follows with the address').toBe('#/')
})
