// The Models page, slice 2a (fe/briefs/02-models.md, section 2): the nav's
// Models link, the #/models route and its read-only preset list. The whole
// app renders against a fake bff (mocked fetch) answering GET /api/presets
// with [{"id", "engine", "engineAvailable", "enabled"}] in the bff's order,
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
  type Preset,
} from './test/bff.ts'

beforeEach(() => startAt('#/models'))

afterEach(() => {
  cleanup()
  vi.useRealTimers()
  vi.unstubAllGlobals()
  startAt('')
})

// ── Helpers ─────────────────────────────────────────────────────────────

function preset(
  id: string,
  enabled = true,
  engine = 'llama.cpp',
  engineAvailable = true,
): Preset {
  return { id, engine, engineAvailable, enabled }
}

/** The app at #/models; returns the fake bff and main. */
function open(options: BffOptions = {}) {
  const bff = fakeBff(options)
  render(<App />)
  return { bff, main: screen.getByRole('main') }
}

function nav(): HTMLElement {
  return screen.getByRole('navigation', { name: en('nav-label') })
}

/** The names of the links in the nav, in order. */
function navLinks(): string[] {
  return within(nav())
    .queryAllByRole('link')
    .map((a) => a.textContent ?? '')
}

function presetsReads(bff: ReturnType<typeof fakeBff>): number {
  return bff.to('GET', '/api/presets').length
}

async function heading1(main: HTMLElement): Promise<HTMLElement> {
  return within(main).findByRole('heading', {
    level: 1,
    name: en('models-title'),
  })
}

function heading2(main: HTMLElement): HTMLElement {
  return within(main).getByRole('heading', {
    level: 2,
    name: en('presets-title'),
  })
}

/** The presets table, found by its name (the h2). */
async function table(main: HTMLElement): Promise<HTMLElement> {
  return within(main).findByRole('table', { name: en('presets-title') })
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

/** The h2, the intro and the note are on the page, in that order around `content`. */
function framed(main: HTMLElement, content: HTMLElement, what: string) {
  const h2 = heading2(main)
  const intro = within(main).getByText(en('presets-intro'))
  const note = within(main).getByText(en('presets-off-note'))
  expect(before(h2, intro), `${what}: intro after the h2`).toBe(true)
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
  return (el.textContent ?? '').replace(/\s+/g, ' ').trim()
}

// ── Failure paths first: errors (17) ────────────────────────────────────

test('2a.17: 403 client-not-authorized and 503 client-unavailable show their api- message as a plain paragraph in place of the table; h2, intro and note kept', async () => {
  for (const [status, key, english] of [
    [403, 'client-not-authorized', en('api-client-not-authorized')],
    [503, 'client-unavailable', en('api-client-unavailable')],
  ] as const) {
    const { main } = open({ presets: () => reply(status, { key, args: [] }) })
    const shown = await message(main, english)
    expect(shown.tagName, key).toBe('P')
    expect(shown.getAttribute('role'), key).toBeNull()
    expect(within(main).queryByRole('alert'), key).toBeNull()
    expect(within(main).queryByRole('table'), key).toBeNull()
    expect(within(main).queryByText(en('presets-load-failed')), key).toBeNull()
    framed(main, shown, key)
    cleanup()
    vi.unstubAllGlobals()
  }
})

test('2a.17: any other status or key, a malformed body or a network failure shows presets-load-failed, never the body', async () => {
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
    ['200 an object of rows', () => reply(200, { presets: [preset('a')] })],
    ['200 [string]', () => reply(200, ['secret-detail'])],
    ['200 [null]', () => reply(200, [null])],
    [
      '200 a row without engineAvailable',
      () =>
        reply(200, [
          { id: 'secret-detail', engine: 'llama.cpp', enabled: true },
        ]),
    ],
    [
      '200 a row without enabled',
      () =>
        reply(200, [
          { id: 'secret-detail', engine: 'llama.cpp', engineAvailable: true },
        ]),
    ],
    [
      '200 a row without id',
      () =>
        reply(200, [
          { engine: 'secret-detail', engineAvailable: true, enabled: true },
        ]),
    ],
    [
      '200 a row without engine',
      () =>
        reply(200, [
          { id: 'secret-detail', engineAvailable: true, enabled: true },
        ]),
    ],
    [
      '200 id a number',
      () =>
        reply(200, [
          {
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
            id: 'secret-detail',
            engine: 'llama.cpp',
            engineAvailable: 1,
            enabled: true,
          },
        ]),
    ],
    [
      '200 one good row, one bad',
      () =>
        reply(200, [
          preset('good-one'),
          { id: 'secret-detail', engine: 'llama.cpp', enabled: false },
        ]),
    ],
  ]
  for (const [what, presets] of answers) {
    const { main } = open({ presets })
    const shown = await message(main, en('presets-load-failed'))
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

test('2a.17: a presets read held 15 000 ms is aborted and shows presets-load-failed', async () => {
  fakeClock()
  const h = held()
  open({ presets: h.answer })
  await flush()
  const main = screen.getByRole('main')
  expect(h.signals, 'the presets were read once').toHaveLength(1)
  expect(h.signals[0], 'the read passes a signal').toBeInstanceOf(AbortSignal)
  await advance(14_999)
  expect(h.signals[0]!.aborted, 'not aborted at 14 999 ms').toBe(false)
  expect(within(main).queryByText(en('presets-load-failed'))).toBeNull()
  await advance(1)
  expect(h.signals[0]!.aborted, 'aborted at 15 000 ms').toBe(true)
  const shown = within(main).getByText(en('presets-load-failed'))
  expect(within(main).queryByText(en('presets-loading'))).toBeNull()
  framed(main, shown, 'timeout')
})

// ── 401 and signed out (4, 16) ──────────────────────────────────────────

test('2a.16: a 401 on the presets read signs the whole app out; after sign-in the page reads again', async () => {
  let reads = 0
  const { bff, main } = open({
    presets: () =>
      ++reads === 1
        ? reply(401, { key: 'session-required', args: [] })
        : reply(200, [preset('after-sign-in')]),
  })
  const username = (await within(main).findByRole('textbox', {
    name: en('signin-username'),
  })) as HTMLInputElement
  const password = within(main).getByLabelText(en('signin-password'), {
    selector: 'input',
  }) as HTMLInputElement
  expect(window.location.hash).toBe('#/models')
  expect(within(main).getByText(en('signin-required'))).toBeTruthy()
  expect(within(main).queryByRole('table')).toBeNull()
  expect(
    within(main).queryByRole('heading', {
      level: 2,
      name: en('presets-title'),
    }),
  ).toBeNull()
  const banner = screen.getByRole('banner')
  expect(
    within(banner).queryByRole('button', { name: en('session-sign-out') }),
  ).toBeNull()
  expect(
    within(banner).queryByRole('button', { name: en('session-sign-in') }) ??
      within(banner).queryByRole('link', { name: en('session-sign-in') }),
  ).not.toBeNull()
  expect(
    within(nav()).queryByRole('list', { name: en('nav-instances') }),
  ).toBeNull()
  expect(
    within(nav()).queryByRole('link', { name: en('nav-models') }),
  ).toBeNull()
  fireEvent.change(username, { target: { value: 'alice' } })
  fireEvent.change(password, { target: { value: 'hunter2' } })
  fireEvent.click(
    within(main).getByRole('button', { name: en('signin-submit') }),
  )
  const t = await table(main)
  expect(rows(t)).toEqual([
    ['after-sign-in', 'llama.cpp', en('presets-status-on')],
  ])
  expect(window.location.hash).toBe('#/models')
  expect(presetsReads(bff)).toBe(2)
  expect(
    await within(nav()).findByRole('link', { name: en('nav-models') }),
  ).toBeTruthy()
})

test('2a.4: signed out, #/models shows the sign-in form with signin-required, keeps the route and reads no presets', async () => {
  const { bff, main } = open({ session: 'signed-out' })
  const username = await within(main).findByRole('textbox', {
    name: en('signin-username'),
  })
  const required = within(main).getByText(en('signin-required'))
  expect(before(required, username)).toBe(true)
  expect(window.location.hash).toBe('#/models')
  expect(
    within(main).queryByRole('heading', { level: 1, name: en('models-title') }),
  ).toBeNull()
  expect(within(main).queryByRole('table')).toBeNull()
  expect(presetsReads(bff)).toBe(0)
})

test('2a.4, 2a.18: after sign-in from #/models the Models page opens with focus on its h1 and reads the presets once', async () => {
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
  expect(window.location.hash).toBe('#/models')
  await table(main)
  expect(presetsReads(bff)).toBe(1)
})

// ── Nav and route (1 to 5) ──────────────────────────────────────────────

test('2a.1: while the session read is pending the nav holds only Overview, and no presets are read', async () => {
  const session = deferred()
  const { bff } = open({ session: () => session.promise })
  expect(navLinks()).toEqual([en('nav-overview')])
  expect(presetsReads(bff)).toBe(0)
  await act(async () => {
    session.resolve(reply(200, { user: 'alice', canConfigure: true }))
  })
  expect(
    await within(nav()).findByRole('link', { name: en('nav-models') }),
  ).toBeTruthy()
})

test('2a.1: signed out the nav has no Models link', async () => {
  startAt('#/')
  open({ session: 'signed-out' })
  await screen.findByRole('heading', { level: 1, name: en('overview-title') })
  const banner = screen.getByRole('banner')
  await waitFor(() =>
    expect(
      within(banner).queryByRole('button', { name: en('session-sign-in') }) ??
        within(banner).queryByRole('link', { name: en('session-sign-in') }),
    ).not.toBeNull(),
  )
  expect(navLinks()).toEqual([en('nav-overview')])
})

test('2a.1: signed in, the nav holds Overview, the instance links, then Models to #/models, outside the Instances group', async () => {
  for (const session of ['admin', 'user'] as const) {
    startAt('#/')
    open({ session })
    const list = await within(nav()).findByRole('list', {
      name: en('nav-instances'),
    })
    const models = await within(nav()).findByRole('link', {
      name: en('nav-models'),
    })
    expect(models.getAttribute('href'), session).toBe('#/models')
    expect(navLinks(), session).toEqual([
      en('nav-overview'),
      'default',
      'coder',
      en('nav-models'),
    ])
    expect(list.contains(models), session).toBe(false)
    expect(before(list, models), session).toBe(true)
    cleanup()
    vi.unstubAllGlobals()
  }
})

test('2a.1: a failed instances read leaves out the Instances group, not Models', async () => {
  startAt('#/')
  open({ instances: () => reply(503, { key: 'client-unavailable', args: [] }) })
  await screen.findByText(en('instances-manager-unavailable'))
  await within(nav()).findByRole('link', { name: en('nav-models') })
  expect(
    within(nav()).queryByRole('list', { name: en('nav-instances') }),
  ).toBeNull()
  expect(navLinks()).toEqual([en('nav-overview'), en('nav-models')])
})

test('2a.2: on #/models the Models link has aria-current="page", and loses it on another route', async () => {
  open()
  const models = await within(nav()).findByRole('link', {
    name: en('nav-models'),
  })
  await waitFor(() => expect(models.getAttribute('aria-current')).toBe('page'))
  expect(
    within(nav())
      .getByRole('link', { name: en('nav-overview') })
      .getAttribute('aria-current'),
  ).toBeNull()
  navigate('#/')
  await waitFor(() => expect(models.getAttribute('aria-current')).toBeNull())
  expect(
    within(nav())
      .getByRole('link', { name: en('nav-overview') })
      .getAttribute('aria-current'),
  ).toBe('page')
})

test('2a.3: a route change to #/models sets the h1, the page title and the focus', async () => {
  startAt('#/')
  const { main } = open()
  await screen.findByRole('heading', { level: 1, name: en('overview-title') })
  navigate('#/models')
  const h1 = await heading1(main)
  await waitFor(() => expect(document.activeElement).toBe(h1))
  expect(h1.getAttribute('tabindex')).toBe('-1')
  expect(document.title).toBe(en('page-title', en('models-title')))
})

test('2a.5: #/models/<anything> is the not-found page', async () => {
  for (const hash of ['#/models/Qwen3.6-35B-A3B-UD-Q4_K_XL', '#/models/a/b']) {
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
})

// ── Page structure and states (6, 14, 15) ───────────────────────────────

test('2a.14: while the presets load, a status paragraph stands in for the table; it goes when they arrive', async () => {
  const pending = deferred()
  const { main } = open({ presets: () => pending.promise })
  // The session's own loading status comes first; wait for the page's.
  const status = await within(main).findByText(en('presets-loading'))
  expect(status.tagName).toBe('P')
  expect(status.getAttribute('role')).toBe('status')
  expect(within(main).queryByRole('table')).toBeNull()
  expect(heading2(main)).toBeTruthy()
  expect(within(main).getByText(en('presets-intro'))).toBeTruthy()
  expect(before(heading2(main), status)).toBe(true)
  await act(async () => {
    pending.resolve(reply(200, [preset('one')]))
  })
  await table(main)
  expect(within(main).queryByText(en('presets-loading'))).toBeNull()
})

test('2a.15: an empty list shows presets-empty as a plain paragraph, no table, then the note', async () => {
  const { main } = open({ presets: [] })
  const empty = await message(main, en('presets-empty'))
  expect(empty.tagName).toBe('P')
  expect(empty.getAttribute('role')).toBeNull()
  expect(within(main).queryByRole('alert')).toBeNull()
  expect(within(main).queryByRole('table')).toBeNull()
  framed(main, empty, 'empty')
})

test('2a.6: below the h1 come the h2, the intro, the table, then the note', async () => {
  const { main } = open()
  const t = await table(main)
  const h1 = await heading1(main)
  expect(before(h1, heading2(main))).toBe(true)
  framed(main, t, 'table')
  expect(within(main).getByText(en('presets-off-note')).tagName).toBe('P')
})

// ── The table (7, 8) ────────────────────────────────────────────────────

test('2a.7: a native table named by the h2, with one header row of Preset, Engine, Status as scope="col"', async () => {
  const { main } = open()
  const t = await table(main)
  expect(t.tagName).toBe('TABLE')
  const labelledBy = t.getAttribute('aria-labelledby')
  expect(labelledBy).toBe(heading2(main).id)
  expect(labelledBy).not.toBe('')
  const headers = within(t).getAllByRole('columnheader')
  expect(headers.map((h) => (h.textContent ?? '').trim())).toEqual([
    en('presets-column-name'),
    en('presets-column-engine'),
    en('presets-column-status'),
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

test('2a.8: the Engine column shows even when every preset has the same engine', async () => {
  const { main } = open({
    presets: [preset('a'), preset('b', false), preset('c')],
  })
  const t = await table(main)
  expect(
    within(t).getByRole('columnheader', { name: en('presets-column-engine') }),
  ).toBeTruthy()
  expect(rows(t).map((r) => r[1])).toEqual([
    'llama.cpp',
    'llama.cpp',
    'llama.cpp',
  ])
})

// ── Rows (9 to 13) ──────────────────────────────────────────────────────

test('2a.9: one row per preset in the order received; the fe does not sort', async () => {
  const { main } = open({
    presets: [
      preset('zeta'),
      preset('Alpha'),
      preset('mid-off', false),
      preset('a-off', false),
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

test('2a.10, 2a.12: each row is the id as plain text, the engine, then On or Off', async () => {
  const { main } = open()
  const t = await table(main)
  expect(rows(t)).toEqual([
    ['Qwen3.6-35B-A3B-UD-Q4_K_XL', 'llama.cpp', en('presets-status-on')],
    ['gemma-3-12b-it', 'llama.cpp', en('presets-status-off')],
  ])
  expect(within(t).queryAllByRole('link')).toHaveLength(0)
})

test('2a.11: the engine is shown as data, and an engine not available reads "<engine> (not available)"', async () => {
  const { main } = open({
    presets: [
      preset('on-strata', true, 'Strata', false),
      // An engine id that is also a strings key is still shown as is.
      preset('keyish', true, 'loading', true),
      preset('off-strata', false, 'Strata', false),
    ],
  })
  const t = await table(main)
  expect(rows(t)).toEqual([
    [
      'on-strata',
      en('presets-engine-not-available', 'Strata'),
      en('presets-status-on'),
    ],
    ['keyish', 'loading', en('presets-status-on')],
    [
      'off-strata',
      en('presets-engine-not-available', 'Strata'),
      en('presets-status-off'),
    ],
  ])
})

test('2a.13: nothing in the page acts, for an administrator or a user', async () => {
  for (const session of ['admin', 'user'] as const) {
    const { main } = open({ session })
    const t = await table(main)
    for (const role of ['button', 'link', 'checkbox', 'switch'] as const) {
      expect(within(t).queryAllByRole(role), `${session} ${role}`).toHaveLength(
        0,
      )
      expect(
        within(main).queryAllByRole(role),
        `${session} ${role} in main`,
      ).toHaveLength(0)
    }
    expect(
      t.querySelectorAll('input, select, textarea, [tabindex]'),
    ).toHaveLength(0)
    cleanup()
    vi.unstubAllGlobals()
  }
})

// ── When it reads (18) ──────────────────────────────────────────────────

test('2a.18: the presets are read once on open, with no polling', async () => {
  fakeClock()
  const { bff } = open()
  await flush()
  expect(
    screen.queryByRole('table', { name: en('presets-title') }),
  ).not.toBeNull()
  expect(presetsReads(bff)).toBe(1)
  await advance(60_000)
  expect(presetsReads(bff)).toBe(1)
})

test('2a.18: leaving #/models and coming back reads the presets again, once', async () => {
  const { bff, main } = open()
  await table(main)
  expect(presetsReads(bff)).toBe(1)
  navigate('#/')
  await screen.findByRole('heading', { level: 1, name: en('overview-title') })
  expect(presetsReads(bff)).toBe(1)
  navigate('#/models')
  await heading1(main)
  await table(main)
  expect(presetsReads(bff)).toBe(2)
})

// ── Keyboard and screen reader (20, 21) ─────────────────────────────────

test('2a.20: Tab goes skip link, banner, nav (Overview, instances, Models), and reaches nothing in the page', async () => {
  const { main } = open({ session: 'user' })
  await table(main)
  await within(nav()).findByRole('list', { name: en('nav-instances') })
  expect(tabbable().map(nameOf)).toEqual([
    en('skip-to-content'),
    en('app-name'),
    en('nav-menu'),
    en('session-sign-out'),
    en('nav-overview'),
    'default',
    'coder',
    en('nav-models'),
  ])
})

test('2a.21: the table has 3 columns and each data row 3 cells under them; the load announces nothing but the status going away', async () => {
  const { main } = open()
  const t = await table(main)
  expect(within(t).getAllByRole('columnheader')).toHaveLength(3)
  const dataRows = within(t)
    .getAllByRole('row')
    .filter((row) => within(row).queryAllByRole('columnheader').length === 0)
  expect(dataRows).toHaveLength(2)
  for (const row of dataRows) expect(cellsOf(row)).toHaveLength(3)
  expect(within(main).queryByRole('status')).toBeNull()
  expect(within(main).queryByRole('alert')).toBeNull()
  expect(main.querySelectorAll('[aria-live]')).toHaveLength(0)
})
