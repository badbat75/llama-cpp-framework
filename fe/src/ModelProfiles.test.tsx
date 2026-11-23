// The Model profiles page, slice 2a (fe/briefs/02-models.md, section 2): the nav's
// Model profiles link, the #/profiles/model route and its read-only model profile list. The whole
// app renders against a fake bff (mocked fetch) answering GET /api/profiles/model
// with [{"id", "engine", "engineAvailable", "enabled", "model"}] in the bff's
// order (the model since slice 2b, whose filter is in
// ModelProfilesFilter.test.tsx),
// errors as {"key", "args"} (fe/briefs/api-contract.md). "2a.17" is the
// brief's criterion 17 of section 2. Criterion 19 (reflow) and the muted
// colour of 12 are checked in the designer's review, not in jsdom.
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
  isHidden,
  navigate,
  reply,
  startAt,
  type BffOptions,
  type ModelProfile,
} from './test/bff.ts'

beforeEach(() => startAt('#/profiles/model'))

afterEach(() => {
  cleanup()
  vi.useRealTimers()
  vi.unstubAllGlobals()
  startAt('')
})

// ── Helpers ─────────────────────────────────────────────────────────────

function modelProfile(
  id: string,
  enabled = true,
  engine = 'llama.cpp',
  engineAvailable = true,
  model = '',
): ModelProfile {
  return { id, engine, engineAvailable, enabled, model }
}

/** The app at #/profiles/model; returns the fake bff and main. */
function open(options: BffOptions = {}) {
  const bff = fakeBff(options)
  render(<App />)
  return { bff, main: screen.getByRole('main') }
}

function nav(): HTMLElement {
  return screen.getByRole('navigation', { name: en('nav-label') })
}

/** The nav, once the session answered: 09 S1a.3, it arrives with the session. */
async function navReady(): Promise<HTMLElement> {
  return screen.findByRole('navigation', { name: en('nav-label') })
}

/** The names of the links of the nav `n`, in order. */
function navLinksIn(n: HTMLElement): string[] {
  return within(n)
    .queryAllByRole('link')
    .map((a) => a.textContent ?? '')
}

/** The names of the links in the nav, in order. */
function navLinks(): string[] {
  return within(nav())
    .queryAllByRole('link')
    .map((a) => a.textContent ?? '')
}

function modelProfilesReads(bff: ReturnType<typeof fakeBff>): number {
  return bff.to('GET', '/api/profiles/model').length
}

async function heading1(main: HTMLElement): Promise<HTMLElement> {
  return within(main).findByRole('heading', {
    level: 1,
    name: en('model-profiles-title'),
  })
}

/** The model profiles table, found by its name: the h1 labels it (5b.5). */
async function table(main: HTMLElement): Promise<HTMLElement> {
  return within(main).findByRole('table', { name: en('model-profiles-title') })
}

/** The cells of a row: data cells, or a row header if the name is one. */
function cellsOf(row: HTMLElement): HTMLElement[] {
  return [...row.querySelectorAll<HTMLElement>('td, th')].filter(
    (c) => c.getAttribute('scope') !== 'col',
  )
}

/** The data rows of the table: the cells' text per row. */
function rows(t: HTMLElement): string[][] {
  return within(t)
    .getAllByRole('row')
    .filter((row) => within(row).queryAllByRole('columnheader').length === 0)
    .map((row) => cellsOf(row).map((c) => (c.textContent ?? '').trim()))
}

/** Waits until the page shows `text` in main, then returns its element. */
async function message(main: HTMLElement, text: string): Promise<HTMLElement> {
  return within(main).findByText(text)
}

/** The h1, the intro and the note are on the page, in that order around `content`. */
function framed(main: HTMLElement, content: HTMLElement, what: string) {
  const h1 = within(main).getByRole('heading', {
    level: 1,
    name: en('model-profiles-title'),
  })
  const intro = within(main).getByText(en('model-profiles-intro'))
  const note = within(main).getByText(en('model-profiles-off-note'))
  expect(before(h1, intro), `${what}: intro after the h1`).toBe(true)
  expect(before(intro, content), `${what}: content after the intro`).toBe(true)
  expect(before(content, note), `${what}: note after the content`).toBe(true)
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

/** Lets every answer and render that is due now land; the clock stays. */
async function flush() {
  await act(async () => {
    for (let i = 0; i < 20; i++) await vi.advanceTimersByTimeAsync(0)
  })
}

async function advance(ms: number) {
  await act(async () => {
    await vi.advanceTimersByTimeAsync(ms)
  })
  await flush()
}

/** The elements Tab reaches, in DOM order (no positive tabindex is used). */
function tabbable(): HTMLElement[] {
  return [
    ...document.querySelectorAll<HTMLElement>(
      'a[href], button, input, select, textarea, [tabindex]',
    ),
  ].filter(
    (el) =>
      !(el as HTMLButtonElement).disabled &&
      el.getAttribute('tabindex') !== '-1' &&
      !isHidden(el),
  )
}

function nameOf(el: HTMLElement): string {
  // A field is named by its label (the filter, slice 2b), a switch by
  // aria-labelledby (slice 2c).
  const by = el.getAttribute('aria-labelledby')
  if (by)
    return (document.getElementById(by)?.textContent ?? '')
      .replace(/\s+/g, ' ')
      .trim()
  const text =
    el instanceof HTMLInputElement
      ? [...(el.labels ?? [])].map((l) => l.textContent).join(' ')
      : el.textContent
  return (text ?? '').replace(/\s+/g, ' ').trim()
}

// ── Failure paths first: errors (17) ────────────────────────────────────

test('2a.17: 403 client-not-authorized and 503 client-unavailable show their api- message as a plain paragraph in place of the table; h2, intro and note kept', async () => {
  for (const [status, key, english] of [
    [403, 'client-not-authorized', en('api-client-not-authorized')],
    [503, 'client-unavailable', en('api-client-unavailable')],
  ] as const) {
    const { main } = open({
      modelProfiles: () => reply(status, { key, args: [] }),
    })
    const shown = await message(main, english)
    expect(shown.tagName, key).toBe('P')
    expect(shown.getAttribute('role'), key).toBeNull()
    expect(within(main).queryByRole('alert'), key).toBeNull()
    expect(within(main).queryByRole('table'), key).toBeNull()
    expect(
      within(main).queryByText(en('model-profiles-load-failed')),
      key,
    ).toBeNull()
    framed(main, shown, key)
    cleanup()
    vi.unstubAllGlobals()
  }
})

test('2a.17, 2b.22: any other status or key, a malformed body (a row without a string model included) or a network failure shows model-profiles-load-failed, never the body', async () => {
  const answers: [string, () => Response | Promise<Response>][] = [
    ['network', () => Promise.reject(new TypeError('Failed to fetch'))],
    ['500', () => reply(500, { key: 'secret-detail', args: [] })],
    ['502 client-other', () => reply(502, { key: 'client-other', args: [] })],
    ['404 not json', () => reply(404, 'secret-detail')],
    ['403 other key', () => reply(403, { key: 'secret-detail', args: [] })],
    ['503 other key', () => reply(503, { key: 'secret-detail', args: [] })],
    ['403 not the error shape', () => reply(403, { error: 'secret-detail' })],
    ['200 not json', () => reply(200, 'secret-detail')],
    ['200 null', () => reply(200, 'null')],
    ['200 {}', () => reply(200, {})],
    [
      '200 an object of rows',
      () => reply(200, { modelProfiles: [modelProfile('a')] }),
    ],
    ['200 [string]', () => reply(200, ['secret-detail'])],
    ['200 [null]', () => reply(200, [null])],
    [
      '200 a row without engineAvailable',
      () =>
        reply(200, [
          {
            model: '',
            id: 'secret-detail',
            engine: 'llama.cpp',
            enabled: true,
          },
        ]),
    ],
    [
      '200 a row without enabled',
      () =>
        reply(200, [
          {
            model: '',
            id: 'secret-detail',
            engine: 'llama.cpp',
            engineAvailable: true,
          },
        ]),
    ],
    [
      '200 a row without id',
      () =>
        reply(200, [
          {
            model: '',
            engine: 'secret-detail',
            engineAvailable: true,
            enabled: true,
          },
        ]),
    ],
    [
      '200 a row without engine',
      () =>
        reply(200, [
          {
            model: '',
            id: 'secret-detail',
            engineAvailable: true,
            enabled: true,
          },
        ]),
    ],
    [
      '200 id a number',
      () =>
        reply(200, [
          {
            model: '',
            id: 7,
            engine: 'secret-detail',
            engineAvailable: true,
            enabled: true,
          },
        ]),
    ],
    [
      '200 engine null',
      () =>
        reply(200, [
          {
            model: '',
            id: 'secret-detail',
            engine: null,
            engineAvailable: true,
            enabled: true,
          },
        ]),
    ],
    [
      '200 enabled a string',
      () =>
        reply(200, [
          {
            model: '',
            id: 'secret-detail',
            engine: 'llama.cpp',
            engineAvailable: true,
            enabled: 'true',
          },
        ]),
    ],
    [
      '200 engineAvailable a number',
      () =>
        reply(200, [
          {
            model: '',
            id: 'secret-detail',
            engine: 'llama.cpp',
            engineAvailable: 1,
            enabled: true,
          },
        ]),
    ],
    [
      '200 a row without model',
      () =>
        reply(200, [
          {
            id: 'secret-detail',
            engine: 'llama.cpp',
            engineAvailable: true,
            enabled: true,
          },
        ]),
    ],
    [
      '200 model a number',
      () =>
        reply(200, [
          {
            id: 'secret-detail',
            engine: 'llama.cpp',
            engineAvailable: true,
            enabled: true,
            model: 7,
          },
        ]),
    ],
    [
      '200 model null',
      () =>
        reply(200, [
          {
            id: 'secret-detail',
            engine: 'llama.cpp',
            engineAvailable: true,
            enabled: true,
            model: null,
          },
        ]),
    ],
    [
      '200 one good row, one without model',
      () =>
        reply(200, [
          modelProfile('good-one'),
          {
            id: 'secret-detail',
            engine: 'llama.cpp',
            engineAvailable: true,
            enabled: true,
          },
        ]),
    ],
    [
      '200 one good row, one bad',
      () =>
        reply(200, [
          modelProfile('good-one'),
          {
            model: '',
            id: 'secret-detail',
            engine: 'llama.cpp',
            enabled: false,
          },
        ]),
    ],
  ]
  for (const [what, modelProfiles] of answers) {
    const { main } = open({ modelProfiles })
    const shown = await message(main, en('model-profiles-load-failed'))
    expect(shown.tagName, what).toBe('P')
    expect(within(main).queryByRole('alert'), what).toBeNull()
    expect(within(main).queryByRole('table'), what).toBeNull()
    expect(document.body.textContent, what).not.toContain('secret-detail')
    expect(document.body.textContent, what).not.toContain('good-one')
    framed(main, shown, what)
    cleanup()
    vi.unstubAllGlobals()
  }
})

test('2a.17: a model profiles read held 15 000 ms is aborted and shows model-profiles-load-failed', async () => {
  fakeClock()
  const h = held()
  open({ modelProfiles: h.answer })
  await flush()
  const main = screen.getByRole('main')
  expect(h.signals, 'the model profiles were read once').toHaveLength(1)
  expect(h.signals[0], 'the read passes a signal').toBeInstanceOf(AbortSignal)
  await advance(14_999)
  expect(h.signals[0]!.aborted, 'not aborted at 14 999 ms').toBe(false)
  expect(within(main).queryByText(en('model-profiles-load-failed'))).toBeNull()
  await advance(1)
  expect(h.signals[0]!.aborted, 'aborted at 15 000 ms').toBe(true)
  const shown = within(main).getByText(en('model-profiles-load-failed'))
  expect(within(main).queryByText(en('model-profiles-loading'))).toBeNull()
  framed(main, shown, 'timeout')
})

// ── 401 and signed out (4, 16) ──────────────────────────────────────────

test('2a.16: a 401 on the model profiles read signs the whole app out; after sign-in the page reads again', async () => {
  let reads = 0
  const { bff, main } = open({
    modelProfiles: () =>
      ++reads === 1
        ? reply(401, { key: 'session-required', args: [] })
        : reply(200, [modelProfile('after-sign-in')]),
  })
  const username = (await within(main).findByRole('textbox', {
    name: en('signin-username'),
  })) as HTMLInputElement
  const password = within(main).getByLabelText(en('signin-password'), {
    selector: 'input',
  }) as HTMLInputElement
  expect(window.location.hash).toBe('#/profiles/model')
  // 09 S1b.3: the read answered 401 while the app was signed in, so the
  // notice says the session ended (it is `signin-required` on a first load
  // signed out).
  expect(within(main).getByText(en('signin-session-ended'))).toBeTruthy()
  expect(within(main).queryByRole('table')).toBeNull()
  expect(
    within(main).queryByRole('heading', {
      level: 1,
      name: en('model-profiles-title'),
    }),
  ).toBeNull()
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
  const t = await table(main)
  expect(rows(t)).toEqual([
    ['after-sign-in', 'llama.cpp', en('model-profiles-status-on')],
  ])
  expect(window.location.hash).toBe('#/profiles/model')
  expect(modelProfilesReads(bff)).toBe(2)
  expect(
    await within(nav()).findByRole('link', { name: en('nav-model-profiles') }),
  ).toBeTruthy()
})

test('2a.4: signed out, #/profiles/model shows the sign-in form with signin-required, keeps the route and reads no model profiles', async () => {
  const { bff, main } = open({ session: 'signed-out' })
  const username = await within(main).findByRole('textbox', {
    name: en('signin-username'),
  })
  const required = within(main).getByText(en('signin-required'))
  expect(before(required, username)).toBe(true)
  expect(window.location.hash).toBe('#/profiles/model')
  expect(
    within(main).queryByRole('heading', {
      level: 1,
      name: en('model-profiles-title'),
    }),
  ).toBeNull()
  expect(within(main).queryByRole('table')).toBeNull()
  expect(modelProfilesReads(bff)).toBe(0)
})

test('2a.4, 2a.18: after sign-in from #/profiles/model the Model profiles page opens with focus on its h1 and reads the model profiles once', async () => {
  const { bff, main } = open({ session: 'signed-out' })
  const username = await within(main).findByRole('textbox', {
    name: en('signin-username'),
  })
  fireEvent.change(username, { target: { value: 'alice' } })
  fireEvent.change(
    within(main).getByLabelText(en('signin-password'), { selector: 'input' }),
    { target: { value: 'hunter2' } },
  )
  fireEvent.click(
    within(main).getByRole('button', { name: en('signin-submit') }),
  )
  const h1 = await heading1(main)
  await waitFor(() => expect(document.activeElement).toBe(h1))
  expect(window.location.hash).toBe('#/profiles/model')
  await table(main)
  expect(modelProfilesReads(bff)).toBe(1)
})

// ── Nav and route (1 to 5) ──────────────────────────────────────────────

test('2a.1: while the session read is pending there is no nav at all, and no model profiles are read', async () => {
  const session = deferred()
  const { bff } = open({ session: () => session.promise })
  // 09 S1a.3: pending, main is the loading line and the shell has no nav.
  expect(document.querySelector('nav')).toBeNull()
  expect(modelProfilesReads(bff)).toBe(0)
  await act(async () => {
    session.resolve(reply(200, { user: 'alice' }))
  })
  expect(
    await screen.findByRole('link', { name: en('nav-model-profiles') }),
  ).toBeTruthy()
})

test('2a.1: signed out there is no nav, so no Model profiles link', async () => {
  startAt('#/')
  open({ session: 'signed-out' })
  // 09 S1a.1 and S1a.4: signed out the sign-in page is the only content, and
  // the shell has no nav.
  await screen.findByRole('heading', { level: 1, name: en('signin-title') })
  expect(screen.queryByRole('navigation', { name: en('nav-label') })).toBeNull()
})

test('2a.1: signed in, the nav holds Overview, the instance links, then Model profiles to #/profiles/model, outside the host list', async () => {
  startAt('#/')
  open()
  // 09 S1a.3: the nav comes with the session.
  const n = await navReady()
  const list = await within(n).findByRole('list', { name: en('nav-this-node') })
  const models = await within(n).findByRole('link', {
    name: en('nav-model-profiles'),
  })
  expect(models.getAttribute('href')).toBe('#/profiles/model')
  expect(navLinksIn(n)).toEqual([
    en('nav-overview'),
    'default',
    'coder',
    // 20 N.7: Node info is in the host list, after its instances.
    en('nav-node'),
    en('nav-server-profiles'),
    en('nav-model-profiles'),
    // 13 N.6: About, last.
    en('nav-about'),
  ])
  expect(list.contains(models)).toBe(false)
  expect(before(list, models)).toBe(true)
})

test('2a.1: a failed instances read leaves out the instance links, not Model profiles', async () => {
  startAt('#/')
  open({ instances: () => reply(503, { key: 'client-unavailable', args: [] }) })
  await screen.findByText(en('instances-manager-unavailable'))
  await within(nav()).findByRole('link', { name: en('nav-model-profiles') })
  expect(
    within(nav()).queryByRole('link', { name: 'default' }),
    // 20 N.6: the group and its labels stay, the links are what is missing.
    'no instance link',
  ).toBeNull()
  expect(navLinks()).toEqual([
    en('nav-overview'),
    en('nav-node'),
    en('nav-server-profiles'),
    en('nav-model-profiles'),
    en('nav-about'),
  ])
})

test('2a.2: on #/profiles/model the Model profiles link has aria-current="page", and loses it on another route', async () => {
  open()
  // 09 S1a.3: the nav comes with the session.
  const n = await navReady()
  const models = await within(n).findByRole('link', {
    name: en('nav-model-profiles'),
  })
  await waitFor(() => expect(models.getAttribute('aria-current')).toBe('page'))
  expect(
    within(n)
      .getByRole('link', { name: en('nav-overview') })
      .getAttribute('aria-current'),
  ).toBeNull()
  navigate('#/')
  await waitFor(() => expect(models.getAttribute('aria-current')).toBeNull())
  expect(
    within(n)
      .getByRole('link', { name: en('nav-overview') })
      .getAttribute('aria-current'),
  ).toBe('page')
})

test('2a.3: a route change to #/profiles/model sets the h1, the page title and the focus', async () => {
  startAt('#/')
  const { main } = open()
  await screen.findByRole('heading', { level: 1, name: en('overview-title') })
  navigate('#/profiles/model')
  const h1 = await heading1(main)
  await waitFor(() => expect(document.activeElement).toBe(h1))
  expect(h1.getAttribute('tabindex')).toBe('-1')
  expect(document.title).toBe(en('page-title', en('model-profiles-title')))
})

test('2a.5, 2d.56, 2d.57: only a malformed #/profiles/model address is the not-found page; a real id is the model profile page, not the list', async () => {
  // 2d replaces 2a.5: the id segment is the model profile page now (56), so what is
  // not-found is what 57 lists as malformed: an empty id, more than one
  // segment, a segment that does not decode.
  for (const hash of [
    '#/profiles/model/',
    '#/profiles/model/a/b',
    '#/profiles/model/%E0%A4%A',
  ]) {
    startAt(hash)
    const { main } = open()
    expect(
      await within(main).findByRole('heading', {
        level: 1,
        name: en('not-found'),
      }),
      hash,
    ).toBeTruthy()
    expect(within(main).queryByRole('table'), hash).toBeNull()
    cleanup()
    vi.unstubAllGlobals()
  }
  // A real id is not the not-found page, and it is not the list: the list's
  // table is gone (the model profile page is the model profile page's own test, 2d.56).
  const id = 'Qwen3.6-35B-A3B-UD-Q4_K_XL'
  startAt(`#/profiles/model/${id}`)
  const { main } = open()
  expect(
    await within(main).findByRole('heading', { level: 1, name: id }),
  ).toBeTruthy()
  expect(
    within(main).queryByRole('heading', { level: 1, name: en('not-found') }),
  ).toBeNull()
  expect(within(main).queryByRole('table'), 'the list is not shown').toBeNull()
})

// ── Page structure and states (6, 14, 15) ───────────────────────────────

test('2a.14: while the model profiles load, a status paragraph stands in for the table; it goes when they arrive', async () => {
  const pending = deferred()
  const { main } = open({ modelProfiles: () => pending.promise })
  // The session's own loading status comes first; wait for the page's.
  const status = await within(main).findByText(en('model-profiles-loading'))
  expect(status.tagName).toBe('P')
  expect(status.getAttribute('role')).toBe('status')
  expect(within(main).queryByRole('table')).toBeNull()
  const h1 = await heading1(main)
  expect(h1).toBeTruthy()
  expect(within(main).getByText(en('model-profiles-intro'))).toBeTruthy()
  expect(before(h1, status)).toBe(true)
  await act(async () => {
    pending.resolve(reply(200, [modelProfile('one')]))
  })
  await table(main)
  expect(within(main).queryByText(en('model-profiles-loading'))).toBeNull()
})

test('2a.15: an empty list shows model-profiles-empty as a plain paragraph, no table, then the note', async () => {
  const { main } = open({ modelProfiles: [] })
  const empty = await message(main, en('model-profiles-empty'))
  expect(empty.tagName).toBe('P')
  expect(empty.getAttribute('role')).toBeNull()
  expect(within(main).queryByRole('alert')).toBeNull()
  expect(within(main).queryByRole('table')).toBeNull()
  framed(main, empty, 'empty')
})

test('2a.6, 5b.5: below the h1 come the intro, the table, then the note, and there is no h2', async () => {
  const { main } = open()
  const t = await table(main)
  expect(within(main).queryByRole('heading', { level: 2 })).toBeNull()
  framed(main, t, 'table')
  expect(within(main).getByText(en('model-profiles-off-note')).tagName).toBe(
    'P',
  )
})

// ── The table (7, 8) ────────────────────────────────────────────────────

test('2a.7, 5b.5: a native table named by the h1, with one header row of ModelProfile, Engine, Status as scope="col"', async () => {
  const { main } = open()
  const t = await table(main)
  expect(t.tagName).toBe('TABLE')
  const labelledBy = t.getAttribute('aria-labelledby')
  expect(labelledBy).toBe('page-heading')
  expect(labelledBy).not.toBe('')
  const headers = within(t).getAllByRole('columnheader')
  expect(headers.map((h) => (h.textContent ?? '').trim())).toEqual([
    en('model-profiles-column-name'),
    en('model-profiles-column-engine'),
    en('model-profiles-column-status'),
  ])
  for (const h of headers) {
    expect(h.tagName).toBe('TH')
    expect(h.getAttribute('scope')).toBe('col')
  }
  // One header row.
  const headerRows = within(t)
    .getAllByRole('row')
    .filter((r) => within(r).queryAllByRole('columnheader').length > 0)
  expect(headerRows).toHaveLength(1)
})

test('2a.8: the Engine column shows even when every model profile has the same engine', async () => {
  const { main } = open({
    modelProfiles: [
      modelProfile('a'),
      modelProfile('b', false),
      modelProfile('c'),
    ],
  })
  const t = await table(main)
  expect(
    within(t).getByRole('columnheader', {
      name: en('model-profiles-column-engine'),
    }),
  ).toBeTruthy()
  expect(rows(t).map((r) => r[1])).toEqual([
    'llama.cpp',
    'llama.cpp',
    'llama.cpp',
  ])
})

// ── Rows (9 to 13) ──────────────────────────────────────────────────────

test('2a.9: one row per model profile in the order received; the fe does not sort', async () => {
  const { main } = open({
    modelProfiles: [
      modelProfile('zeta'),
      modelProfile('Alpha'),
      modelProfile('mid-off', false),
      modelProfile('a-off', false),
    ],
  })
  const t = await table(main)
  expect(rows(t).map((r) => r[0])).toEqual([
    'zeta',
    'Alpha',
    'mid-off',
    'a-off',
  ])
})

test('2a.10, 2a.12, 2d.60: each row is the id as a link to the model profile page, the engine, then On or Off', async () => {
  const { main } = open()
  const t = await table(main)
  expect(rows(t)).toEqual([
    ['Qwen3.6-35B-A3B-UD-Q4_K_XL', 'llama.cpp', en('model-profiles-status-on')],
    ['gemma-3-12b-it', 'llama.cpp', en('model-profiles-status-off')],
  ])
  // 2d.60 replaces 2a.10's plain text: the id is an <a> to the model profile page.
  expect(
    within(t)
      .getAllByRole('link')
      .map((a) => [(a.textContent ?? '').trim(), a.getAttribute('href')]),
  ).toEqual([
    [
      'Qwen3.6-35B-A3B-UD-Q4_K_XL',
      '#/profiles/model/Qwen3.6-35B-A3B-UD-Q4_K_XL',
    ],
    ['gemma-3-12b-it', '#/profiles/model/gemma-3-12b-it'],
  ])
})

test('2a.11: the engine is shown as data, and an engine not available reads "<engine> (not available)"', async () => {
  const { main } = open({
    modelProfiles: [
      modelProfile('on-strata', true, 'Strata', false),
      // An engine id that is also a strings key is still shown as is.
      modelProfile('keyish', true, 'loading', true),
      modelProfile('off-strata', false, 'Strata', false),
    ],
  })
  const t = await table(main)
  expect(rows(t)).toEqual([
    [
      'on-strata',
      en('model-profiles-engine-not-available', 'Strata'),
      en('model-profiles-status-on'),
    ],
    ['keyish', 'loading', en('model-profiles-status-on')],
    [
      'off-strata',
      en('model-profiles-engine-not-available', 'Strata'),
      en('model-profiles-status-off'),
    ],
  ])
})

test('2a.13, 2c.37, 2d.60: a row acts through its link then its switch; nothing else in a row acts (U17c: the table has the two header sort buttons, in its head)', async () => {
  // 2c replaces 2a.13: each row gains its on/off switch (administrator only:
  // only administrators sign in, user 2026-11-15). 2d.60 replaces the plain
  // name with a link, so a row has two controls: the link, then the switch.
  // U17c puts a sort button in the Model profile and Engine headers: buttons are
  // in the head, never in a data row.
  const { main } = open()
  const t = await table(main)
  for (const row of dataRows(t))
    for (const role of ['button', 'checkbox'] as const)
      expect(
        within(row).queryAllByRole(role),
        `${role}: a data row holds no ${role}`,
      ).toHaveLength(0)
  expect(
    within(t).queryAllByRole('checkbox'),
    'no checkbox: the switch says switch',
  ).toHaveLength(0)
  expect(
    within(t).getAllByRole('button'),
    'the two header buttons of U17c, and no other button in the table',
  ).toHaveLength(2)
  expect(within(t).queryAllByRole('switch')).toHaveLength(2)
  expect(within(t).queryAllByRole('link')).toHaveLength(2)
  expect(t.querySelectorAll('input')).toHaveLength(2)
  expect(t.querySelectorAll('select, textarea, [tabindex]')).toHaveLength(0)
  for (const row of within(t)
    .getAllByRole('row')
    .filter((r) => within(r).queryAllByRole('columnheader').length === 0)) {
    const acts = row.querySelectorAll('a, input, button, select, textarea')
    expect(
      [...acts].map((el) => el.tagName),
      'the name link first, then the switch',
    ).toEqual(['A', 'INPUT'])
  }
})

// ── When it reads (18) ──────────────────────────────────────────────────

test('2a.18: the model profiles are read once on open, with no polling', async () => {
  fakeClock()
  const { bff } = open()
  await flush()
  expect(
    screen.queryByRole('table', { name: en('model-profiles-title') }),
  ).not.toBeNull()
  expect(modelProfilesReads(bff)).toBe(1)
  await advance(60_000)
  expect(modelProfilesReads(bff)).toBe(1)
})

test('2a.18: leaving #/profiles/model and coming back reads the model profiles again, once', async () => {
  const { bff, main } = open()
  await table(main)
  expect(modelProfilesReads(bff)).toBe(1)
  navigate('#/')
  await screen.findByRole('heading', { level: 1, name: en('overview-title') })
  expect(modelProfilesReads(bff)).toBe(1)
  navigate('#/profiles/model')
  await heading1(main)
  await table(main)
  expect(modelProfilesReads(bff)).toBe(2)
})

// ── Keyboard and screen reader (20, 21) ─────────────────────────────────

test('2a.20, 2b.35, 2c.53, 2d.60: Tab goes skip link, banner, nav (Overview, instances, Model profiles), the filter field, the two header sort buttons, then each row link and switch', async () => {
  // 2c.53 replaces 2a.20: the administrator's switches follow the filter.
  // 2d.60 puts each row's link before its switch.
  // U17c puts the sortable headers' buttons after the field, in the table's head.
  const { main } = open()
  await table(main)
  await within(nav()).findByRole('list', { name: en('nav-this-node') })
  expect(tabbable().map(nameOf)).toEqual([
    en('skip-to-content'),
    en('app-name'),
    en('nav-menu'),
    // Brief 07 H4.2: the theme switch, between Menu and the session part.
    en('theme-dark'),
    en('session-sign-out'),
    en('nav-overview'),
    'default',
    'coder',
    // 20 N.7: Node info is in the host list, after its instances.
    en('nav-node'),
    en('nav-server-profiles'),
    en('nav-model-profiles'),
    // 13 N.6: About, last, both before the page.
    en('nav-about'),
    en('model-profiles-filter-label'),
    en('model-profiles-column-name'),
    en('model-profiles-column-engine'),
    'Qwen3.6-35B-A3B-UD-Q4_K_XL',
    'Qwen3.6-35B-A3B-UD-Q4_K_XL',
    'gemma-3-12b-it',
    'gemma-3-12b-it',
  ])
})

test('2a.21, 2b.30, 2c.50: the table has 3 columns and each data row 3 cells under them; the load announces nothing: the filter status and the switch status are there, empty', async () => {
  const { main } = open()
  const t = await table(main)
  expect(within(t).getAllByRole('columnheader')).toHaveLength(3)
  const dataRows = within(t)
    .getAllByRole('row')
    .filter((row) => within(row).queryAllByRole('columnheader').length === 0)
  expect(dataRows).toHaveLength(2)
  for (const row of dataRows) expect(cellsOf(row)).toHaveLength(3)
  // 2c.50 adds the switch region's status after the filter's.
  const statuses = within(main).getAllByRole('status')
  expect(statuses).toHaveLength(2)
  for (const status of statuses) {
    expect(status.tagName).toBe('P')
    expect(status.textContent).toBe('')
  }
  expect(within(main).queryByRole('alert')).toBeNull()
  expect(
    [...main.querySelectorAll('[aria-live]')].filter(
      (el) => !statuses.includes(el as HTMLElement),
    ),
  ).toHaveLength(0)
})

// ── 10 R1.3: the table in a scroll box of its own ─────────────────────────

/** The box that holds the table, found through the table (brief 10, R1.3). */
function boxOf(t: HTMLElement): HTMLElement {
  const box = t.parentElement
  expect(box, 'the table has a parent').not.toBeNull()
  return box!
}

/** The data rows of a table. */
function dataRows(t: HTMLElement): HTMLElement[] {
  return within(t)
    .getAllByRole('row')
    .filter((row) => within(row).queryAllByRole('columnheader').length === 0)
}

test("10 R1.3: the table is the only child of a div.table-scroll, in the table's old place, and the box is generic: no tabindex, role, aria attribute or id, the table keeps its name", async () => {
  const { main } = open()
  const t = await table(main)
  const box = boxOf(t)
  expect(box.tagName, 'a div: a scroll container, not a landmark').toBe('DIV')
  expect(box.className, 'the class the box rule of R1.4 selects').toBe(
    'table-scroll',
  )
  expect(
    [...box.children],
    "the table is the box's only child: the box scrolls the table and nothing else",
  ).toEqual([t])
  expect(
    [...box.attributes].map((a) => a.name).sort(),
    'no tabindex: Tab reaches every column through the row controls and the browser scrolls the box to what it focuses (2.1.1). No role and no aria-*: a named region would repeat the h1 and add a landmark and a stop to every visit, in the common case where nothing scrolls. No id: the div is generic (R1.8: no CSS aims at an id)',
  ).toEqual(['class'])
  expect(
    t.getAttribute('aria-labelledby'),
    'the table keeps the name the h1 gives it (5b.5)',
  ).toBe('page-heading')
  // The order framed() checks is unchanged, and the box sits where the table
  // was: after the filter, its hint and the two status lines.
  framed(main, box, 'the table box')
  const hint = within(main).getByText(en('model-profiles-filter-hint'))
  expect(before(hint, box), 'the box comes after the filter').toBe(true)
  const switchStatus = main.querySelector('p.model-profiles-switch-status')
  expect(switchStatus, 'the switch status is on the page').not.toBeNull()
  expect(before(switchStatus!, box), 'and after the status lines').toBe(true)
})

test('10 R1.3: the box is no Tab stop of its own, and every row of a readable id holds its link and its switch, so Tab reaches each column and scrolls the box (Firefox adds the box as one stop only while it overflows: jsdom never scrolls)', async () => {
  const { main } = open()
  const t = await table(main)
  const box = boxOf(t)
  const stops = tabbable()
  expect(
    stops,
    'the box is not a Tab stop: no tabindex, and the page gains no stop',
  ).not.toContain(box)
  const rows = dataRows(t)
  expect(rows).toHaveLength(2)
  for (const row of rows) {
    const label = (row.textContent ?? '').trim()
    expect(
      stops.filter((el) => row.contains(el)).length,
      `the row holds a focusable control, so Tab brings the box around: ${label}`,
    ).toBeGreaterThanOrEqual(1)
    expect(within(row).getByRole('switch'), label).toBeTruthy()
    expect(within(row).getAllByRole('link'), label).toHaveLength(1)
  }
  // The two columns at the ends of a row are both reachable: the link in the
  // first cell, the switch in the last.
  const first = rows[0]!
  expect(within(first).getAllByRole('link')[0]!.closest('td')).toBe(
    cellsOf(first)[0],
  )
  expect(within(first).getByRole('switch').closest('td')).toBe(
    cellsOf(first)[2],
  )
})

test('10 R1.3: a row whose id opens no page has no link and still holds its switch, so it is reachable inside the box', async () => {
  const { main } = open({
    modelProfiles: [modelProfile('.'), modelProfile('..')],
  })
  const t = await table(main)
  const rows = dataRows(t)
  expect(rows).toHaveLength(2)
  for (const row of rows) {
    const label = (row.textContent ?? '').trim()
    expect(within(row).queryByRole('link'), label).toBeNull()
    expect(
      tabbable().filter((el) => row.contains(el)).length,
      `${label}: the switch alone carries the row into the Tab order`,
    ).toBe(1)
  }
})

test('10 R1.3: the loading and the empty state render no scroll box', async () => {
  const pending = deferred()
  const { main } = open({ modelProfiles: () => pending.promise })
  await within(main).findByText(en('model-profiles-loading'))
  expect(main.querySelector('div.table-scroll'), 'loading').toBeNull()
  cleanup()
  vi.unstubAllGlobals()

  const second = open({ modelProfiles: [] })
  await message(second.main, en('model-profiles-empty'))
  expect(within(second.main).queryByRole('table'), 'empty').toBeNull()
  expect(second.main.querySelector('div.table-scroll'), 'empty').toBeNull()
})

test('10 R1.3: the filter with no match and a failed read render no scroll box', async () => {
  const { main } = open()
  await table(main)
  const input = (await within(main).findByRole('searchbox', {
    name: en('model-profiles-filter-label'),
  })) as HTMLInputElement
  fireEvent.change(input, { target: { value: 'nothing-like-it' } })
  await within(main).findByText(en('model-profiles-no-match'))
  expect(within(main).queryByRole('table'), 'no match').toBeNull()
  expect(main.querySelector('div.table-scroll'), 'no match').toBeNull()
  cleanup()
  vi.unstubAllGlobals()

  const failed = open({
    modelProfiles: () => reply(500, { key: 'secret-detail', args: [] }),
  })
  await message(failed.main, en('model-profiles-load-failed'))
  expect(within(failed.main).queryByRole('table'), 'failed read').toBeNull()
  expect(
    failed.main.querySelector('div.table-scroll'),
    'failed read',
  ).toBeNull()
})
