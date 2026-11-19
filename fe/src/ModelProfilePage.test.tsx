// The model profile page #/profiles/model/<id>, slice 2d part 1 (fe/briefs/02-models.md,
// sections 2d.1 to 2d.4): the route and its address, coming from the list and
// back, heading, title, nav, focus, and the states: loading, 401, 403, 503,
// 404 no-model-profile-named, everything else, the 15 s read limit. The page reads
// only GET /api/profiles/model/<id> (fe/briefs/api-contract.md, the proposed
// section); "2d.56" is the brief's criterion 56. The loaded page's Engine
// field and Assets card (2d.5 to 2d.8) are not tested here, so a loaded page
// is checked only for what 56 to 67 say: the h1, the title, the back link, the
// focus. The colours of 60 and the scroll position of 62 are checked in the
// designer's review, not in jsdom, and 62a's reload of the tab has no
// equivalent in jsdom (2b.34 in ModelProfilesFilter.test.tsx pins that the text is in
// no storage and not in the address).
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
  before,
  deferred,
  en,
  fakeBff,
  held,
  navigate,
  MODEL_PROFILES,
  modelProfileDoc,
  reply,
  startAt,
  heavyTest,
  type BffOptions,
} from './test/bff.ts'
import { cardHeadings, fieldLabels } from './test/modelProfileCards.ts'

const ID = 'Qwen3-27B'
const HASH = `#/profiles/model/${ID}`
const OFF_ID = 'gemma-3-12b-it'
const OFF_HASH = `#/profiles/model/${OFF_ID}`
const LIST_ID = 'Qwen3.6-35B-A3B-UD-Q4_K_XL'

beforeEach(() => startAt(HASH))

afterEach(() => {
  cleanup()
  vi.useRealTimers()
  vi.unstubAllGlobals()
  startAt('')
})

// ── Helpers ─────────────────────────────────────────────────────────────

/** The app at #/profiles/model/Qwen3-27B; returns the fake bff and main. */
function open(options: BffOptions = {}) {
  const bff = fakeBff(options)
  render(<App />)
  return { bff, main: screen.getByRole('main') }
}

function nav(): HTMLElement {
  return screen.getByRole('navigation', { name: en('nav-label') })
}

async function heading1(main: HTMLElement, name: string): Promise<HTMLElement> {
  return within(main).findByRole('heading', { level: 1, name })
}

/**
 * The h1 on screen now, found synchronously: under fake timers `findByRole`
 * never polls (its timers are faked), so the fake-clock tests flush the queue
 * and then query (as ModelProfiles.test.tsx's 2a.18 does).
 */
function shownH1(main: HTMLElement, name: string): HTMLElement {
  return within(main).getByRole('heading', { level: 1, name })
}

/**
 * Focus sits on the page's h1 for `name` (63): the focused element is a
 * heading, level 1, inside main, whose accessible name is `name`. The node
 * itself is not pinned: the page remounts on each arrival.
 */
async function focusOnH1(main: HTMLElement, name: string) {
  const h1 = await heading1(main, name)
  await waitFor(() => expect(document.activeElement).toBe(h1))
}

/** The way back (criterion 61). */
function backLink(main: HTMLElement): HTMLElement {
  return within(main).getByRole('link', { name: en('model-profile-back') })
}

/** Waits until the page shows `text` in main, then returns its element. */
async function message(main: HTMLElement, text: string): Promise<HTMLElement> {
  return within(main).findByText(text)
}

/** The model profile page's read of `id`. */
function modelProfileReads(bff: ReturnType<typeof fakeBff>, id = ID): number {
  return bff.to('GET', `/api/profiles/model/${encodeURIComponent(id)}`).length
}

/** Every call under /api/profiles/model, list and single reads. */
function modelProfileCalls(bff: ReturnType<typeof fakeBff>) {
  return bff.calls.filter((c) => c.path.startsWith('/api/profiles/model'))
}

// List-page helpers (criteria 60, 62).
async function table(main: HTMLElement): Promise<HTMLElement> {
  return within(main).findByRole('table', { name: en('model-profiles-title') })
}

function dataRows(t: HTMLElement): HTMLElement[] {
  return within(t)
    .getAllByRole('row')
    .filter((row) => within(row).queryAllByRole('columnheader').length === 0)
}

function shownIds(t: HTMLElement): string[] {
  return dataRows(t).map((row) =>
    (row.querySelector('td, th')?.textContent ?? '').trim(),
  )
}

function filterField(main: HTMLElement): HTMLInputElement {
  return within(main).getByRole('searchbox', {
    name: en('model-profiles-filter-label'),
  }) as HTMLInputElement
}

function clearFilter(main: HTMLElement): HTMLElement | null {
  return within(main).queryByRole('button', {
    name: en('model-profiles-filter-clear'),
  })
}

/** The filter's status line: the first status (2c.50 adds the switch one after it). */
function filterStatus(main: HTMLElement): HTMLElement {
  return within(main).getAllByRole('status')[0]!
}

/** Leaves for the overview and opens the list again (any other page, 62). */
async function openList(main: HTMLElement): Promise<HTMLElement> {
  navigate('#/')
  await heading1(main, en('overview-title'))
  navigate('#/profiles/model')
  return table(main)
}

/** The elements Tab reaches (no positive tabindex is used). */
function tabbable(): HTMLElement[] {
  return [
    ...document.querySelectorAll<HTMLElement>(
      'a[href], button, input, select, textarea, [tabindex]',
    ),
  ].filter(
    (el) =>
      !(el as HTMLButtonElement).disabled &&
      el.getAttribute('tabindex') !== '-1' &&
      el.closest('[hidden]') === null,
  )
}

/** A click on a `#` link: the address moves in a later task. */
async function clickLink(link: HTMLElement) {
  await act(async () => {
    fireEvent.click(link)
    await new Promise((r) => setTimeout(r, 0))
  })
}

/**
 * Signs in as `user` in the form standing in for the protected page. The field
 * is waited for: right after render main still holds the session's loading
 * paragraph, not the form.
 */
async function signInto(main: HTMLElement, user = 'alice') {
  const username = await within(main).findByRole('textbox', {
    name: en('signin-username'),
  })
  fireEvent.change(username, { target: { value: user } })
  fireEvent.change(
    within(main).getByLabelText(en('signin-password'), { selector: 'input' }),
    { target: { value: 'hunter2' } },
  )
  fireEvent.click(
    within(main).getByRole('button', { name: en('signin-submit') }),
  )
}

/** The banner's Sign out; waits for the signed-out banner. */
/** Signs out from the banner, and waits for the app to be signed out: 09
 * S1a.11 lands it on the sign-in page at `#/`, so its h1 is what answers. */
async function signOutFromBanner() {
  fireEvent.click(screen.getByRole('button', { name: en('session-sign-out') }))
  await screen.findByRole('heading', { level: 1, name: en('signin-title') })
}

/** Fake timers and Date only: the microtask queue stays real. */
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
 * Lets every answer and render that is due now land; the clock stays. The
 * answer chain (fetch, body, render) is a few links deep, so a few rounds
 * land it; the per-second stepping tests call this on every fake second and
 * each round is real time.
 */
async function flush() {
  await act(async () => {
    for (let i = 0; i < 4; i++) await vi.advanceTimersByTimeAsync(0)
  })
}

async function advance(ms: number) {
  await act(async () => {
    await vi.advanceTimersByTimeAsync(ms)
  })
  await flush()
}

// ── Failure paths first: read errors (66, 59, 58) ──────────────────────

// Warm-up: the app's first render of the file costs many times a later one;
// this test absorbs that cost (a synchronous mount, no finder), so the real
// tests find a warm render and their finders' real-time deadlines hold.
heavyTest('warm-up: the app mounts and shows main', () => {
  fakeBff()
  render(<App />)
  expect(screen.getByRole('main')).toBeTruthy()
  cleanup()
})

test('2d.66: 403 and 503 show their api- message as a plain paragraph in place of the content; the back link and the h1 stay', async () => {
  for (const [status, key, english] of [
    [403, 'client-not-authorized', en('api-client-not-authorized')],
    [503, 'client-unavailable', en('api-client-unavailable')],
  ] as const) {
    const { main } = open({
      modelProfile: () => reply(status, { key, args: [] }),
    })
    const shown = await message(main, english)
    expect(shown.tagName, key).toBe('P')
    expect(shown.getAttribute('role'), key).toBeNull()
    expect(within(main).queryByRole('alert'), key).toBeNull()
    expect(
      within(main).queryByText(en('model-profile-load-failed')),
      key,
    ).toBeNull()
    const h1 = await heading1(main, ID)
    expect(h1.getAttribute('tabindex'), key).toBe('-1')
    expect(document.title, key).toBe(en('page-title', ID))
    const b = backLink(main)
    expect(before(b, h1), `${key}: back before the h1`).toBe(true)
    expect(before(h1, shown), `${key}: message below the h1`).toBe(true)
    cleanup()
    vi.unstubAllGlobals()
  }
})

// heavyTest: it mounts the whole app 25 times, once per answer it checks.
heavyTest(
  '2d.66: any other status, key, body, reply shape, a network failure shows model-profile-load-failed, never the body; a field the fe does not know is ignored',
  async () => {
    const answers: [string, () => Response | Promise<Response>][] = [
      ['network', () => Promise.reject(new TypeError('Failed to fetch'))],
      ['500', () => reply(500, { key: 'secret-detail', args: [] })],
      ['502 client-other', () => reply(502, { key: 'client-other', args: [] })],
      ['404 no body', () => reply(404)],
      ['404 other key', () => reply(404, { key: 'no-such-key', args: [] })],
      ['403 other key', () => reply(403, { key: 'secret-detail', args: [] })],
      ['503 other key', () => reply(503, { key: 'secret-detail', args: [] })],
      ['403 not the error shape', () => reply(403, { error: 'secret-detail' })],
      ['200 not json', () => reply(200, 'secret-detail')],
      ['200 null', () => reply(200, 'null')],
      ['200 []', () => reply(200, [])],
      ['200 {}', () => reply(200, {})],
      ['200 no id', () => reply(200, modelProfileDoc({}, { id: undefined }))],
      [
        '200 no engine',
        () => reply(200, modelProfileDoc({}, { engine: undefined })),
      ],
      [
        '200 no engineAvailable',
        () => reply(200, modelProfileDoc({}, { engineAvailable: undefined })),
      ],
      [
        '200 no enabled',
        () => reply(200, modelProfileDoc({}, { enabled: undefined })),
      ],
      [
        '200 no projectUrl',
        () => reply(200, modelProfileDoc({}, { projectUrl: undefined })),
      ],
      ['200 id a number', () => reply(200, modelProfileDoc({}, { id: 7 }))],
      [
        '200 enabled a string',
        () => reply(200, modelProfileDoc({}, { enabled: 'true' })),
      ],
      [
        '200 engineAvailable a number',
        () => reply(200, modelProfileDoc({}, { engineAvailable: 1 })),
      ],
      [
        '200 projectUrl null',
        () => reply(200, modelProfileDoc({}, { projectUrl: null })),
      ],
      [
        '200 values a string',
        () => reply(200, modelProfileDoc({}, { values: 'secret-detail' })),
      ],
      [
        '200 values null',
        () => reply(200, modelProfileDoc({}, { values: null })),
      ],
      [
        '200 values holds a number',
        () => reply(200, modelProfileDoc({}, { values: { model: 7 } })),
      ],
    ]
    for (const [what, modelProfile] of answers) {
      const { main } = open({ modelProfile })
      const shown = await message(main, en('model-profile-load-failed'))
      expect(shown.tagName, what).toBe('P')
      expect(shown.getAttribute('role'), what).toBeNull()
      expect(within(main).queryByRole('alert'), what).toBeNull()
      expect(await heading1(main, ID), what).toBeTruthy()
      expect(backLink(main), what).toBeTruthy()
      expect(document.body.textContent, what).not.toContain('secret-detail')
      cleanup()
      vi.unstubAllGlobals()
    }
    // Criterion 81: the shape is id, engine, engineAvailable, enabled, values,
    // projectUrl; every other field is ignored. 2h.1's 131 took `revision` out
    // of that set (a read without one is model-profile-load-failed, pinned by 131 in
    // ModelProfileSave1.test.tsx), so the field ignored here is one the fe never
    // reads. The content is awaited: the h1 of the id stands while the read is
    // in flight, so checking the text next to it alone is a race.
    const { main } = open({
      modelProfile: () => reply(200, modelProfileDoc({}, { note: 'zzz' })),
    })
    await within(main).findByRole('heading', {
      level: 3,
      name: en('model-profile-assets'),
    })
    expect(await heading1(main, ID)).toBeTruthy()
    expect(within(main).queryByText(en('model-profile-load-failed'))).toBeNull()
  },
)

test('2d.66: a model profile read held 15 000 ms is aborted and shows model-profile-load-failed, and the loading status goes', async () => {
  fakeClock()
  const h = held()
  open({ modelProfile: (_id, init) => h.answer(init) })
  await flush()
  const main = screen.getByRole('main')
  expect(h.signals, 'the model profile was read once').toHaveLength(1)
  expect(h.signals[0], 'the read passes a signal').toBeInstanceOf(AbortSignal)
  expect(within(main).getByText(en('model-profile-loading'))).toBeTruthy()
  await advance(14_999)
  expect(h.signals[0]!.aborted, 'not aborted at 14 999 ms').toBe(false)
  expect(within(main).queryByText(en('model-profile-load-failed'))).toBeNull()
  await advance(1)
  expect(h.signals[0]!.aborted, 'aborted at 15 000 ms').toBe(true)
  const shown = within(main).getByText(en('model-profile-load-failed'))
  expect(shown.tagName).toBe('P')
  expect(within(main).queryByText(en('model-profile-loading'))).toBeNull()
})

test('2d.66: a 401 on the read signs the whole app out; after sign-in the page reads again, focus on its h1', async () => {
  let reads = 0
  const { bff, main } = open({
    modelProfile: () =>
      ++reads === 1
        ? reply(401, { key: 'session-required', args: [] })
        : reply(200, modelProfileDoc()),
  })
  const username = (await within(main).findByRole('textbox', {
    name: en('signin-username'),
  })) as HTMLInputElement
  const password = within(main).getByLabelText(en('signin-password'), {
    selector: 'input',
  }) as HTMLInputElement
  expect(window.location.hash).toBe(HASH)
  // 09 S1b.3: a 401 on the read of a signed-in app ends the session.
  expect(within(main).getByText(en('signin-session-ended'))).toBeTruthy()
  expect(within(main).queryByRole('heading', { level: 1, name: ID })).toBeNull()
  const banner = screen.getByRole('banner')
  expect(
    within(banner).queryByRole('button', { name: en('session-sign-out') }),
  ).toBeNull()
  expect(
    within(banner).queryByRole('button', { name: en('session-sign-in') }) ??
      within(banner).queryByRole('link', { name: en('session-sign-in') }),
    '09 S1a.4: signed out the banner offers no Sign in',
  ).toBeNull()
  expect(
    screen.queryByRole('navigation', { name: en('nav-label') }),
    '09 S1a.4: signed out there is no nav',
  ).toBeNull()
  fireEvent.change(username, { target: { value: 'alice' } })
  fireEvent.change(password, { target: { value: 'hunter2' } })
  fireEvent.click(
    within(main).getByRole('button', { name: en('signin-submit') }),
  )
  const h1 = await heading1(main, ID)
  await waitFor(() => expect(document.activeElement).toBe(h1))
  expect(window.location.hash).toBe(HASH)
  expect(modelProfileReads(bff)).toBe(2)
  expect(
    await within(nav()).findByRole('link', { name: en('nav-model-profiles') }),
  ).toBeTruthy()
})

test('2d.59: a 404 no-model-profile-named shows model-profile-not-found filled with the id from the address, a plain paragraph below the h1; the back link stays', async () => {
  startAt('#/profiles/model/Qwen3-28B')
  const { main } = open({
    modelProfile: () =>
      reply(404, { key: 'no-model-profile-named', args: ['Qwen3-28B'] }),
  })
  const shown = await message(main, en('model-profile-not-found', 'Qwen3-28B'))
  expect(shown.tagName).toBe('P')
  expect(shown.getAttribute('role')).toBeNull()
  expect(within(main).queryByRole('alert')).toBeNull()
  expect(within(main).queryByText(en('model-profile-load-failed'))).toBeNull()
  const h1 = await heading1(main, 'Qwen3-28B')
  const b = backLink(main)
  expect(b.getAttribute('href')).toBe('#/profiles/model')
  expect(before(b, h1), 'back before the h1').toBe(true)
  expect(before(h1, shown), 'the sentence below the h1').toBe(true)
})

test('2d.59a: the ids . and .. send no read at all (the URL parser resolves a dot segment): model-profile-not-found with the id, with no loading status in between; the back link and the h1 as in 59', async () => {
  for (const [hash, id] of [
    ['#/profiles/model/.', '.'],
    ['#/profiles/model/..', '..'],
    ['#/profiles/model/%2E%2E', '..'],
  ] as const) {
    startAt(hash)
    fakeClock()
    const { bff, main } = open({
      modelProfile: () => reply(200, modelProfileDoc({}, { id })),
    })
    // Every text main ever received: the loading status must never be among
    // them, so an implementation that shows it for one render is caught.
    const seen: string[] = []
    const observer = new MutationObserver((list) => {
      for (const m of list) {
        if (m.type === 'characterData') seen.push(m.target.textContent ?? '')
        for (const n of m.addedNodes) seen.push(n.textContent ?? '')
      }
    })
    observer.observe(main, {
      childList: true,
      subtree: true,
      characterData: true,
    })
    // The session read lands; the model profile page is rendered with its message,
    // with no read to answer it.
    await flush()
    observer.disconnect()
    const shown = within(main).getByText(en('model-profile-not-found', id))
    expect(
      seen.join(' '),
      `${hash}: no loading status in between`,
    ).not.toContain(en('model-profile-loading'))
    expect(shown.tagName, hash).toBe('P')
    expect(shown.getAttribute('role'), hash).toBeNull()
    expect(within(main).queryByRole('alert'), hash).toBeNull()
    expect(
      within(main).queryByText(en('model-profile-load-failed')),
      hash,
    ).toBeNull()
    expect(
      within(main).queryByText(en('model-profile-loading')),
      hash,
    ).toBeNull()
    expect(within(main).queryAllByRole('status'), hash).toHaveLength(0)
    const h1 = within(main).getByRole('heading', { level: 1, name: id })
    expect(h1.getAttribute('tabindex'), hash).toBe('-1')
    expect(document.title, hash).toBe(en('page-title', id))
    const b = backLink(main)
    expect(b.getAttribute('href'), hash).toBe('#/profiles/model')
    expect(before(b, h1), `${hash}: back before the h1`).toBe(true)
    expect(before(h1, shown), `${hash}: the sentence below the h1`).toBe(true)
    // Nothing is sent, not once, not later: past the read limit the fake saw
    // no model profile read, and neither the single route nor the list route (where
    // a /api/profiles/model/.. request would land).
    await advance(15_500)
    expect(
      modelProfileCalls(bff),
      `${hash}: no model profile read at all`,
    ).toHaveLength(0)
    expect(
      bff.to('GET', '/api/profiles/model'),
      `${hash}: not the list`,
    ).toHaveLength(0)
    expect(
      within(main).getByText(en('model-profile-not-found', id)),
      hash,
    ).toBe(shown)
    expect(within(main).queryAllByRole('status'), hash).toHaveLength(0)
    cleanup()
    vi.unstubAllGlobals()
    vi.useRealTimers()
  }
})

test('2d.59a: only exactly . and .. are refused: ... and a.b read as usual, with the loading status and one read of the id', async () => {
  for (const id of ['...', 'a.b'] as const) {
    startAt(`#/profiles/model/${id}`)
    const { bff, main } = open()
    const status = await within(main).findByText(en('model-profile-loading'))
    expect(status.getAttribute('role'), id).toBe('status')
    const h1 = await heading1(main, id)
    expect(h1.getAttribute('tabindex'), id).toBe('-1')
    expect(document.title, id).toBe(en('page-title', id))
    expect(
      within(main).queryByText(en('model-profile-not-found', id)),
      id,
    ).toBeNull()
    expect(
      modelProfileCalls(bff).map((c) => `${c.method} ${c.path}`),
      id,
    ).toEqual([`GET /api/profiles/model/${id}`])
    cleanup()
    vi.unstubAllGlobals()
  }
})

test('2d.58: signed out, #/profiles/model/<id> shows the sign-in form with signin-required, keeps the address and reads nothing', async () => {
  const { bff, main } = open({ session: 'signed-out' })
  const username = await within(main).findByRole('textbox', {
    name: en('signin-username'),
  })
  const required = within(main).getByText(en('signin-required'))
  expect(before(required, username)).toBe(true)
  expect(window.location.hash).toBe(HASH)
  expect(within(main).queryByRole('heading', { level: 1, name: ID })).toBeNull()
  expect(modelProfileReads(bff)).toBe(0)
  expect(modelProfileCalls(bff)).toHaveLength(0)
})

test('2d.58, 2d.63, 2d.67: after sign-in the model profile page is read, once, with focus on its h1', async () => {
  const { bff, main } = open({ session: 'signed-out' })
  await signInto(main)
  const h1 = await heading1(main, ID)
  await waitFor(() => expect(document.activeElement).toBe(h1))
  expect(h1.getAttribute('tabindex')).toBe('-1')
  expect(document.title).toBe(en('page-title', ID))
  expect(window.location.hash).toBe(HASH)
  expect(modelProfileReads(bff)).toBe(1)
})

// ── Route and address (56, 57) ──────────────────────────────────────────

test('2d.56: the address reads GET /api/profiles/model/<id>, the id percent-encoded as one path segment, and the page reads nothing else', async () => {
  for (const [id, encoded] of [
    [ID, ID],
    ['a b', 'a%20b'],
    ['a%b', 'a%25b'],
  ] as const) {
    startAt(`#/profiles/model/${encoded}`)
    const { bff, main } = open()
    expect(await heading1(main, id), id).toBeTruthy()
    // The h1 is there from the address, before the read is sent: the call is
    // waited for, the way the switch tests wait for their write.
    await waitFor(() => expect(modelProfileReads(bff, id), id).toBe(1))
    // Criterion 80: the page reads only this, not the list, not the engines.
    expect(
      modelProfileCalls(bff).map((c) => `${c.method} ${c.path}`),
      id,
    ).toEqual([`GET /api/profiles/model/${encoded}`])
    cleanup()
    vi.unstubAllGlobals()
  }
})

test('2d.57: #/profiles/model/, #/profiles/model/a/b and a segment that does not decode show the not-found page; #/profiles/model stays the list', async () => {
  for (const hash of [
    '#/profiles/model/',
    '#/profiles/model/a/b',
    '#/profiles/model/%E0%A4%A',
  ]) {
    startAt(hash)
    const { bff, main } = open()
    expect(
      await within(main).findByRole('heading', {
        level: 1,
        name: en('not-found'),
      }),
      hash,
    ).toBeTruthy()
    const home = within(main).getByRole('link', {
      name: en('not-found-home'),
    })
    expect(home.getAttribute('href'), hash).toBe('#/')
    expect(modelProfileCalls(bff), hash).toHaveLength(0)
    cleanup()
    vi.unstubAllGlobals()
  }
  startAt('#/profiles/model')
  const { main } = open()
  expect(
    await within(main).findByRole('heading', {
      level: 1,
      name: en('model-profiles-title'),
    }),
  ).toBeTruthy()
})

// ── From the list and back (60, 61, 62) ─────────────────────────────────

test('2d.60: in the list each row name is a link to the model profile page, its text the id, inside the element that names the row switch, in the switched-off row too; clicking it opens the page, focus on its h1', async () => {
  startAt('#/profiles/model')
  const { main } = open()
  const t = await table(main)
  const links = within(t).getAllByRole('link')
  expect(links.map((a) => (a.textContent ?? '').trim())).toEqual([
    LIST_ID,
    OFF_ID,
  ])
  expect(links.map((a) => a.getAttribute('href'))).toEqual([
    `#/profiles/model/${LIST_ID}`,
    `#/profiles/model/${OFF_ID}`,
  ])
  for (const row of dataRows(t)) {
    const nameCell = row.querySelector('td, th') as HTMLElement
    const id = (nameCell.textContent ?? '').trim()
    const input = row.querySelector('input')!
    const by = input.getAttribute('aria-labelledby') ?? ''
    const target = document.getElementById(by)
    expect(target, `${id}: the switch is named by an element`).not.toBeNull()
    const link = within(nameCell).getByRole('link')
    expect(
      target!.contains(link),
      `${id}: the link stays inside the element that names the switch`,
    ).toBe(true)
    expect(within(t).getByRole('switch', { name: id }), id).toBe(input)
  }
  // The way in: the row link opens the page (criterion 63: focus on the h1).
  await clickLink(within(t).getByRole('link', { name: OFF_ID }))
  expect(window.location.hash).toBe(OFF_HASH)
  const h1 = await heading1(main, OFF_ID)
  await waitFor(() => expect(document.activeElement).toBe(h1))
})

test('2d.59a: in the list, a row whose id is . or .. shows its id as plain text, not a link, still inside the element that names its switch; the other rows stay links', async () => {
  startAt('#/profiles/model')
  const { main } = open({
    modelProfiles: [
      {
        id: '..',
        engine: 'llama.cpp',
        engineAvailable: true,
        enabled: true,
        model: '',
      },
      {
        id: '.',
        engine: 'llama.cpp',
        engineAvailable: false,
        enabled: false,
        model: '',
      },
      ...MODEL_PROFILES,
    ],
  })
  const t = await table(main)
  const rows = dataRows(t)
  expect(shownIds(t), 'every row shows').toEqual(['..', '.', LIST_ID, OFF_ID])
  // The links are only the readable ids (criterion 60).
  expect(
    within(t)
      .getAllByRole('link')
      .map((a) => (a.textContent ?? '').trim()),
    'no link for . or ..',
  ).toEqual([LIST_ID, OFF_ID])
  for (const id of ['..', '.']) {
    const row = rows.find(
      (r) => (r.querySelector('td, th')?.textContent ?? '').trim() === id,
    )!
    expect(
      within(row).queryByRole('link'),
      `${id}: the row holds no link`,
    ).toBe(null)
    const input = row.querySelector('input')!
    const target = document.getElementById(
      input.getAttribute('aria-labelledby') ?? '',
    )
    expect(target, `${id}: the switch is named by an element`).not.toBeNull()
    expect(
      (target!.textContent ?? '').trim(),
      `${id}: the element naming the switch holds the id as text`,
    ).toBe(id)
    expect(
      within(t).getByRole('switch', { name: id }),
      `${id}: the switch is still named by the id`,
    ).toBe(input)
  }
  for (const id of [LIST_ID, OFF_ID]) {
    const link = within(t).getByRole('link', { name: id })
    expect(link.getAttribute('href'), id).toBe(`#/profiles/model/${id}`)
    const row = link.closest('tr') as HTMLElement
    const input = row.querySelector('input')!
    const target = document.getElementById(
      input.getAttribute('aria-labelledby') ?? '',
    )!
    expect(
      target.contains(link),
      `${id}: the link stays inside the element that names the switch`,
    ).toBe(true)
  }
})

test('2d.61: the model-profile-back link is the first thing in main, before the h1, in every state: loading, 403, 503, not found, load failed, loaded', async () => {
  const states: [string, BffOptions][] = [
    ['loading', { modelProfile: () => deferred().promise }],
    [
      '403',
      {
        modelProfile: () =>
          reply(403, { key: 'client-not-authorized', args: [] }),
      },
    ],
    [
      '503',
      {
        modelProfile: () => reply(503, { key: 'client-unavailable', args: [] }),
      },
    ],
    [
      'not found',
      {
        modelProfile: () =>
          reply(404, { key: 'no-model-profile-named', args: [ID] }),
      },
    ],
    ['load failed', { modelProfile: () => reply(500, { key: 'x', args: [] }) }],
    ['loaded', {}],
  ]
  for (const [what, options] of states) {
    const { main } = open(options)
    const h1 = await heading1(main, ID)
    const b = backLink(main)
    expect((b.textContent ?? '').trim(), what).toBe(en('model-profile-back'))
    expect(b.getAttribute('href'), what).toBe('#/profiles/model')
    expect(before(b, h1), `${what}: back before the h1`).toBe(true)
    expect(
      tabbable().find((el) => main.contains(el)),
      `${what}: the first focusable in main is the back link`,
    ).toBe(b)
    cleanup()
    vi.unstubAllGlobals()
  }
})

test('2d.62: back to the list keeps the filter: read again, the field holds the text last typed, the rows are filtered, Clear filter and the count are there, focus on the list h1', async () => {
  startAt('#/profiles/model')
  const { bff, main } = open()
  await table(main)
  fireEvent.change(filterField(main), { target: { value: 'gemma' } })
  navigate(OFF_HASH)
  await heading1(main, OFF_ID)
  expect(modelProfileReads(bff, OFF_ID)).toBe(1)
  await clickLink(backLink(main))
  expect(window.location.hash).toBe('#/profiles/model')
  expect(
    bff.to('GET', '/api/profiles/model'),
    'the list is read again',
  ).toHaveLength(2)
  const t = await table(main)
  expect(filterField(main).value, 'the text last typed is kept').toBe('gemma')
  expect(shownIds(t), 'the rows are filtered by it').toEqual([OFF_ID])
  expect(clearFilter(main), 'Clear filter is shown').not.toBeNull()
  expect(filterStatus(main).textContent).toBe(
    en('model-profiles-filter-count', '1', '2'),
  )
  const h1 = await heading1(main, en('model-profiles-title'))
  expect(document.activeElement, 'focus is not in the field').not.toBe(
    filterField(main),
  )
  await waitFor(() => expect(document.activeElement).toBe(h1))
})

test('2d.62: the nav Model profiles link opens the list with the kept text too; while the list loads the field is not shown, and the text is kept for when it does', async () => {
  startAt('#/profiles/model')
  const pending = deferred()
  let reads = 0
  const { main } = open({
    modelProfiles: () =>
      ++reads === 1 ? reply(200, MODEL_PROFILES) : pending.promise,
  })
  await table(main)
  fireEvent.change(filterField(main), { target: { value: 'gemma' } })
  navigate('#/')
  await heading1(main, en('overview-title'))
  fireEvent.click(
    within(nav()).getByRole('link', { name: en('nav-model-profiles') }),
  )
  expect(
    within(main).queryByRole('searchbox'),
    'no field while the list loads',
  ).toBeNull()
  await act(async () => {
    pending.resolve(reply(200, MODEL_PROFILES))
  })
  const t = await table(main)
  expect(filterField(main).value, 'the text kept across the load').toBe('gemma')
  expect(shownIds(t)).toEqual([OFF_ID])
})

test('2d.62a: Sign out empties the kept filter', async () => {
  startAt('#/profiles/model')
  const { main } = open()
  await table(main)
  fireEvent.change(filterField(main), { target: { value: 'gemma' } })
  await openList(main)
  expect(filterField(main).value, 'kept across a page change').toBe('gemma')
  await signOutFromBanner()
  // 09 S1a.11: Sign out lands on the sign-in page at #/, so the form to sign in
  // again is in main; the list is opened again after the Overview shows.
  await signInto(main)
  await heading1(main, en('overview-title'))
  navigate('#/profiles/model')
  const t = await table(main)
  expect(filterField(main).value, 'Sign out emptied it').toBe('')
  expect(shownIds(t), 'every row shows').toEqual([LIST_ID, OFF_ID])
})

test('2d.62a: a sign-in after a 401 keeps the filter for the same user', async () => {
  startAt('#/profiles/model')
  let reads = 0
  const { main } = open({
    modelProfiles: () =>
      ++reads === 3
        ? reply(401, { key: 'session-required', args: [] })
        : reply(200, MODEL_PROFILES),
  })
  await table(main)
  fireEvent.change(filterField(main), { target: { value: 'gemma' } })
  await openList(main)
  expect(filterField(main).value, 'kept across a page change').toBe('gemma')
  navigate('#/')
  await heading1(main, en('overview-title'))
  navigate('#/profiles/model')
  await within(main).findByText(en('signin-session-ended'))
  expect(window.location.hash).toBe('#/profiles/model')
  await signInto(main, 'alice')
  const t = await table(main)
  expect(filterField(main).value, 'the same user keeps the text').toBe('gemma')
  expect(shownIds(t)).toEqual([OFF_ID])
})

test('2d.62a: a sign-in as another user after a 401 empties the filter', async () => {
  startAt('#/profiles/model')
  let reads = 0
  const { main } = open({
    modelProfiles: () =>
      ++reads === 3
        ? reply(401, { key: 'session-required', args: [] })
        : reply(200, MODEL_PROFILES),
    signIn: () => reply(200, { user: 'carol' }),
  })
  await table(main)
  fireEvent.change(filterField(main), { target: { value: 'gemma' } })
  await openList(main)
  expect(filterField(main).value, 'kept across a page change').toBe('gemma')
  navigate('#/')
  await heading1(main, en('overview-title'))
  navigate('#/profiles/model')
  await within(main).findByText(en('signin-session-ended'))
  await signInto(main, 'carol')
  const t = await table(main)
  expect(filterField(main).value, 'another user starts empty').toBe('')
  expect(shownIds(t)).toEqual([LIST_ID, OFF_ID])
})

// ── Heading, title, nav, focus (63, 64) ─────────────────────────────────

test('2d.63: on arrival from the address the h1 and the title are the id from the address, focus on the h1', async () => {
  startAt('#/')
  const pending = deferred()
  const { main } = open({ modelProfile: () => pending.promise })
  await screen.findByRole('heading', { level: 1, name: en('overview-title') })
  navigate(HASH)
  const h1 = await heading1(main, ID)
  expect(h1.getAttribute('tabindex')).toBe('-1')
  expect(document.title).toBe(en('page-title', ID))
  await waitFor(() => expect(document.activeElement).toBe(h1))
  // Still loading: the h1 is the address id (the reply cannot be asked for).
  expect(within(main).getByText(en('model-profile-loading'))).toBeTruthy()
})

test('2d.63: once read, the h1 and the title take the id of the reply, the file spelling', async () => {
  startAt('#/profiles/model/qwen3-27b')
  const pending = deferred()
  const { main } = open({ modelProfile: () => pending.promise })
  await heading1(main, 'qwen3-27b')
  expect(document.title).toBe(en('page-title', 'qwen3-27b'))
  await act(async () => {
    pending.resolve(reply(200, modelProfileDoc({}, { id: ID })))
  })
  const h1 = await heading1(main, ID)
  expect(h1.getAttribute('tabindex')).toBe('-1')
  expect(document.title).toBe(en('page-title', ID))
})

test('2d.63: focus goes to the h1 from the address bar, from Back and from Forward', async () => {
  // The page remounts on every arrival, which is what makes the model profile read
  // once per opening (67), so the h1 is a new element each time: what is pinned
  // is that the focused element is the page's h1, carrying the id (63).
  startAt('#/profiles/model')
  const { main } = open()
  await table(main)
  navigate(HASH)
  await focusOnH1(main, ID)
  navigate('#/profiles/model')
  await focusOnH1(main, en('model-profiles-title'))
  navigate(HASH)
  await focusOnH1(main, ID)
})

test('2d.64: on the model profile page the nav Models link has aria-current="true"; on #/profiles/model it keeps "page"', async () => {
  open()
  // 09 S1a.3: the nav arrives with the session, so it is taken once it is there.
  const navEl = await screen.findByRole('navigation', {
    name: en('nav-label'),
  })
  const models = await within(navEl).findByRole('link', {
    name: en('nav-model-profiles'),
  })
  await waitFor(() => expect(models.getAttribute('aria-current')).toBe('true'))
  expect(
    within(navEl)
      .getByRole('link', { name: en('nav-overview') })
      .getAttribute('aria-current'),
  ).toBeNull()
  navigate('#/profiles/model')
  await waitFor(() => expect(models.getAttribute('aria-current')).toBe('page'))
})

// ── States (65, 67) ─────────────────────────────────────────────────────

test('2d.65: while the model profile loads, a status paragraph model-profile-loading below the h1 stands in for the content; it goes when the read answers', async () => {
  const pending = deferred()
  const { main } = open({ modelProfile: () => pending.promise })
  const status = await within(main).findByText(en('model-profile-loading'))
  expect(status.tagName).toBe('P')
  expect(status.getAttribute('role')).toBe('status')
  expect(within(main).getAllByRole('status')).toEqual([status])
  const h1 = await heading1(main, ID)
  expect(before(backLink(main), h1), 'back first').toBe(true)
  expect(before(h1, status), 'the status below the h1').toBe(true)
  await act(async () => {
    pending.resolve(reply(200, modelProfileDoc()))
  })
  expect(within(main).queryByText(en('model-profile-loading'))).toBeNull()
})

test('2d.67: the model profile is read once on open and once on each route change to it, with no polling and no refresh button', async () => {
  fakeClock()
  const { bff, main } = open()
  await flush()
  expect(modelProfileReads(bff)).toBe(1)
  // Criterion 67: no refresh button. 2h.10's 196: the page's buttons start
  // with the ten section jump buttons, named by the card titles; criterion 88:
  // then the Assets disclosure and one About toggle per 2e field, in
  // DOM order (the list of test/modelProfileCards.ts; H11b.1 retires the
  // Engine About toggle), and 2h.1's 135 adds
  // the save bar's two buttons at the end of the settings section: so no other
  // button (a refresh) is in main.
  expect(
    within(main)
      .queryAllByRole('button')
      .map((b) => (b.textContent ?? '').replace(/\s+/g, ' ').trim()),
    'main holds the jump disclosure of H10.10, the ten jump buttons, the Assets disclosure, one About toggle per field, then the bar',
  ).toEqual([
    en('model-profile-jump-label'),
    ...cardHeadings(),
    en('model-profile-assets'),
    ...fieldLabels().map((label) => `${en('field-about')} ${label}`),
    en('save-discard'),
    en('save-button'),
  ])
  await advance(60_000)
  expect(modelProfileReads(bff), 'no polling').toBe(1)
  navigate(OFF_HASH)
  await flush()
  expect(shownH1(main, OFF_ID), 'the new id shows').toBeTruthy()
  expect(modelProfileReads(bff, OFF_ID), 'the new id is read').toBe(1)
  expect(modelProfileReads(bff), 'the old id is not read again').toBe(1)
  navigate(HASH)
  await flush()
  expect(shownH1(main, ID), 'the first id shows again').toBeTruthy()
  expect(modelProfileReads(bff), 'opening the first id reads it again').toBe(2)
  await advance(60_000)
  expect(modelProfileReads(bff), 'no polling after the second open').toBe(2)
})
