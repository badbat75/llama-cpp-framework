// The instance page's Model profiles section (fe/briefs/16-server-profiles-and-instances.md,
// slice 17e, as its "17e check (2026-11-25)" amends it: 17e.4a, 17e.7a, 17e.8a and 17e.10a
// replace 17e.4, 17e.7, 17e.8 and 17e.10): an administrator reads the model profiles one
// instance serves, switches the section's `*` and one row at a time through the three
// association routes (fe/briefs/api-contract.md 868-898, served since 2d9a4c8). "17e.6" is
// the brief's criterion 6. The whole app renders against the fake bff (mocked fetch) on
// fake timers, since the instance page polls the instances list on a timer.
//
// A row write's reply carries `effective` as the ROW of the record as the write left it
// (api-contract.md 878), so the tests answer a row and the switch reads the row's own
// `effective`.
//
// The 17e review D1 pins: when a refused write's re-read also fails, the control goes
// back to its start state and the alert keeps the write's own refusal text, plus a Sign
// in only when the re-read answered 401 or 403.
//
// What jsdom cannot see is left to the designer's `fe/ux` run of the `instance-models`
// state: the table's box scrolling inside itself at 320 px and 200 % text, and every
// control within 1 px of its row's centre (17e.4a, 17e.10).
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
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
  INSTANCE_MODELS,
  before,
  deferred,
  en,
  fakeBff,
  held,
  isHidden,
  reply,
  startAt,
  stubDialog,
  type BffOptions,
  type InstanceModelRow,
  type InstanceModels,
} from './test/bff.ts'
import { modelProfileHref } from './route.ts'
import strings from './strings/en_US.json'

let dialogs: ReturnType<typeof stubDialog>

beforeEach(() => {
  dialogs = stubDialog()
})

afterEach(() => {
  cleanup()
  vi.useRealTimers()
  vi.unstubAllGlobals()
  dialogs.restore()
  startAt('')
})

// ── Helpers ─────────────────────────────────────────────────────────────

/** One row of the read, over the served defaults of a row. */
function row(
  id: string,
  over: Partial<InstanceModelRow> = {},
): InstanceModelRow {
  return {
    id,
    engine: 'llama.cpp',
    engineAvailable: true,
    enabled: true,
    model: `/m/${id}.gguf`,
    value: 'none',
    viaAll: true,
    effective: true,
    ...over,
  }
}

/** A read's body: the section's `*`, and the rows. */
function doc(
  profiles: InstanceModelRow[],
  all: InstanceModels['all'] = 'inherit',
): InstanceModels {
  return { all, profiles }
}

/** The rows of the fake's default answer: one served through `*`, one served
 *  by an explicit on while off in the list, one refused by an explicit off. */
const DEFAULT_ROWS = INSTANCE_MODELS.profiles

function fakeClock() {
  // shouldAdvanceTime keeps the findBy* waits running (InstancePage.test.tsx
  // 405): the section's h2 is found with a findBy* while the page polls.
  vi.useFakeTimers({ shouldAdvanceTime: true })
}

async function flush() {
  await act(async () => {
    for (let i = 0; i < 20; i++) await vi.advanceTimersByTimeAsync(0)
  })
}

/** The clock moves `ms` (the instances list polls every IDLE_MS, 5 000). */
async function advance(ms: number) {
  await act(async () => {
    await vi.advanceTimersByTimeAsync(ms)
  })
  await flush()
}

/** The section: the element holding its h2. */
async function modelsSection(main: HTMLElement): Promise<HTMLElement> {
  const h2 = await within(main).findByRole('heading', {
    level: 2,
    name: en('instance-models-title'),
  })
  return h2.closest('section') ?? h2.parentElement!
}

/**
 * The instance page at `#/instances/default`, with the Model profiles section
 * present: the h2 is there and the read has landed (the flush settles it),
 * whatever its outcome. The tolerant twin of `loadedModels`, for the fixtures
 * with no table: a failed read (17e.2) and `profiles: []` (17e.4).
 */
async function loadedSection(options: BffOptions = {}) {
  fakeClock()
  startAt('#/instances/default')
  const bff = fakeBff(options)
  render(<App />)
  await flush()
  const main = screen.getByRole('main')
  const section = await modelsSection(main)
  return { bff, main, section }
}

/**
 * The instance page at `#/instances/default`, with the Model profiles section
 * loaded: the table is there, so every test starts from a read that landed.
 */
async function loadedModels(options: BffOptions = {}) {
  const { bff, main, section } = await loadedSection(options)
  expect(
    within(section).queryByRole('table', {
      name: en('instance-models-title'),
    }),
    'the table is loaded',
  ).not.toBeNull()
  return { bff, main, section }
}

function table(section: HTMLElement): HTMLElement {
  return within(section).getByRole('table', {
    name: en('instance-models-title'),
  })
}

function dataRows(section: HTMLElement): HTMLElement[] {
  return within(table(section))
    .getAllByRole('row')
    .filter((row) => within(row).queryAllByRole('columnheader').length === 0)
}

/** The rows' cells, as the eye reads them. */
function cells(section: HTMLElement): string[][] {
  return dataRows(section).map((row) =>
    [...row.querySelectorAll('td, th')].map((c) =>
      (c.textContent ?? '').replace(/\s+/g, ' ').trim(),
    ),
  )
}

function switchOf(section: HTMLElement, id: string): HTMLInputElement {
  return within(table(section)).getByRole('switch', {
    name: id,
  }) as HTMLInputElement
}

function switches(section: HTMLElement): HTMLInputElement[] {
  return within(table(section)).queryAllByRole('switch') as HTMLInputElement[]
}

/** The `*` checkbox, in the group the brief labels. */
function allBox(section: HTMLElement): HTMLInputElement {
  const group = within(section).getByRole('group', {
    name: en('instance-models-all-label'),
  })
  return within(group).getByRole('checkbox', {
    name: en('instance-models-all'),
  }) as HTMLInputElement
}

function inactive(el: HTMLElement): boolean {
  return el.getAttribute('aria-disabled') === 'true'
}

function press(el: HTMLElement) {
  el.focus()
  fireEvent.click(el)
}

function type(input: HTMLElement, value: string) {
  fireEvent.change(input, { target: { value } })
}

/** The section's one status line (17e.4: one, just above the table). */
function statusLine(section: HTMLElement): HTMLElement {
  const all = within(section).queryAllByRole('status')
  expect(all, 'the section has one status region').toHaveLength(1)
  return all[0]!
}

function statusText(section: HTMLElement): string {
  return (statusLine(section).textContent ?? '').replace(/\s+/g, ' ').trim()
}

function alerts(section: HTMLElement): string[] {
  return within(section)
    .queryAllByRole('alert')
    .map((a) => a.textContent ?? '')
}

/** The section's Sign in buttons, outside any dialog. */
function signIns(section: HTMLElement): HTMLButtonElement[] {
  return (
    within(section).queryAllByRole('button', {
      name: en('session-sign-in'),
    }) as HTMLButtonElement[]
  ).filter((b) => b.closest('dialog') === null)
}

const MODELS_PATH = '/api/instances/default/profiles/model'

function modelsReads(bff: ReturnType<typeof fakeBff>): number {
  return bff.to('GET', MODELS_PATH).length
}

/** Every write of the section, with the path it went to and its body. */
function writes(bff: ReturnType<typeof fakeBff>): [string, unknown][] {
  return bff.calls
    .filter((c) => c.method === 'PUT' && c.path.startsWith(MODELS_PATH))
    .map((c) => [c.path, c.body])
}

/** The row write's path, the profile percent-encoded. */
function rowPath(id: string): string {
  return `${MODELS_PATH}/${encodeURIComponent(id)}`
}

const SESSION_ENDED = () => reply(401, { key: 'session-required', args: [] })
const REVOKED = () => reply(403, { key: 'session-not-administrator', args: [] })

async function signInInDialog(user: string) {
  const d = screen.getByRole('dialog', { name: en('signin-title') })
  expect(dialogs.opened, 'opened with showModal()').toContain(d)
  type(within(d).getByRole('textbox', { name: en('signin-username') }), user)
  type(
    within(d).getByLabelText(en('signin-password'), { selector: 'input' }),
    'hunter2',
  )
  fireEvent.click(within(d).getByRole('button', { name: en('signin-submit') }))
  await flush()
  expect(screen.queryByRole('dialog'), 'the dialog closed').toBeNull()
}

/** The elements Tab reaches, in DOM order. */
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

/** aria-labelledby, then an aria-label, then a `<label>`, then the text. */
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

// ── 17e.1: the read ─────────────────────────────────────────────────────

test('16 17e.1: the page sends one GET of the instance’s model profiles; while it runs the section says instance-models-loading in a role="status", then the intro and the table', async () => {
  fakeClock()
  const gate = deferred()
  startAt('#/instances/default')
  const bff = fakeBff({ instanceModels: () => gate.promise })
  render(<App />)
  await flush()
  const main = screen.getByRole('main')
  const section = await modelsSection(main)
  const loading = statusLine(section)
  expect(loading.getAttribute('role'), 'a status region').toBe('status')
  expect(loading.textContent, 'the loading text').toBe(
    en('instance-models-loading'),
  )
  expect(
    within(section).queryByRole('table'),
    'no table while pending',
  ).toBeNull()
  gate.resolve(reply(200, doc(DEFAULT_ROWS)))
  await flush()
  const intro = within(section).getByText(en('instance-models-intro'))
  expect(intro.tagName, 'the intro is a paragraph').toBe('P')
  expect(
    within(section).getAllByRole('row').length,
    'the table, one row per profile',
  ).toBe(DEFAULT_ROWS.length + 1)
  expect(modelsReads(bff), 'one read').toBe(1)
})

test('16 17e.1: the section is read once and never on the idle poll: a poll of the instances list that lands does not read it again', async () => {
  const { bff } = await loadedModels()
  expect(modelsReads(bff), 'one read at opening').toBe(1)
  await advance(6_000)
  expect(
    bff.to('GET', '/api/instances').length,
    'the list is polled',
  ).toBeGreaterThan(1)
  expect(modelsReads(bff), 'the section is not polled').toBe(1)
})

// ── 17e.2: read failures, first ─────────────────────────────────────────

const READ_FAILURES: [string, () => Response | Promise<Response>, string][] = [
  [
    '404 client-no-instance',
    () => reply(404, { key: 'client-no-instance', args: [] }),
    en('instance-not-found', 'default'),
  ],
  [
    '503 client-unavailable',
    () => reply(503, { key: 'client-unavailable', args: [] }),
    en('api-client-unavailable'),
  ],
  [
    '502 client-not-authorized',
    () => reply(502, { key: 'client-not-authorized', args: [] }),
    en('api-client-not-authorized'),
  ],
  [
    '502 client-other',
    () => reply(502, { key: 'client-other', args: ['zz'] }),
    en('instance-models-failed'),
  ],
  [
    '500 no key',
    () => reply(500, { error: 'zz' }),
    en('instance-models-failed'),
  ],
  ['200 {}', () => reply(200, {}), en('instance-models-failed')],
  [
    '200 with no profiles',
    () => reply(200, { all: 'inherit' }),
    en('instance-models-failed'),
  ],
  [
    '200 with no all',
    () => reply(200, { profiles: [] }),
    en('instance-models-failed'),
  ],
  [
    '200 all a number, profiles a string',
    () => reply(200, { all: 3, profiles: 'zz' }),
    en('instance-models-failed'),
  ],
  ['200 null', () => reply(200, 'null'), en('instance-models-failed')],
  ['200 not json', () => reply(200, 'zz'), en('instance-models-failed')],
  [
    'network',
    () => Promise.reject(new TypeError('zz')),
    en('instance-models-failed'),
  ],
]

test.each(READ_FAILURES)(
  '16 17e.2: a failed read shows its text as a plain paragraph in the section, with no table and no control, and never the body: %s',
  async (what, instanceModels, text) => {
    const { section } = await loadedSection({ instanceModels })
    expect(within(section).queryByRole('table'), `${what}: no table`).toBeNull()
    expect(
      within(section).queryAllByRole('switch'),
      `${what}: no switch`,
    ).toEqual([])
    expect(
      within(section).queryByRole('checkbox'),
      `${what}: no checkbox`,
    ).toBeNull()
    const message = within(section).getByText(text)
    expect(message.tagName, `${what}: a paragraph`).toBe('P')
    expect(
      message.closest('[role="alert"], [role="status"]'),
      `${what}: not an alert nor a status`,
    ).toBeNull()
    expect(document.body.textContent, what).not.toContain('zz')
  },
)

test('16 17e.2, 17e.1: a 401 on the read signs the app out as the page’s other reads do, and a sign-in as another administrator reads the section again', async () => {
  let reads = 0
  fakeClock()
  startAt('#/instances/default')
  const bff = fakeBff({
    instanceModels: () =>
      ++reads === 1
        ? SESSION_ENDED()
        : reply(200, doc([row('after-sign-in', { effective: false })])),
    signIn: () => reply(200, { user: 'carol' }),
  })
  render(<App />)
  await flush()
  const main = screen.getByRole('main')
  expect(
    // 09 S1b.3, as InstancePage.test.tsx 288 pins for the page's other reads:
    // the 401 answered a signed-in app, so the notice says the session ended
    // (`signin-required` is only for a first load signed out).
    await within(main).findByText(en('signin-session-ended')),
    'the sign-in page',
  ).toBeTruthy()
  expect(window.location.hash, 'the route stays').toBe('#/instances/default')
  expect(within(main).queryByRole('table'), 'no table').toBeNull()
  type(
    within(main).getByRole('textbox', { name: en('signin-username') }),
    'carol',
  )
  type(
    within(main).getByLabelText(en('signin-password'), { selector: 'input' }),
    'hunter2',
  )
  fireEvent.click(
    within(main).getByRole('button', { name: en('signin-submit') }),
  )
  const section = await modelsSection(main)
  // The h2 is on screen while the section's read is open (the loading
  // render), so the switch arrives with the read: an async finder.
  expect(
    await within(section).findByRole('switch', { name: 'after-sign-in' }),
    'the section read for the new session',
  ).toBeTruthy()
  expect(modelsReads(bff), 'read again after the sign-in').toBe(2)
})

test('16 17e.2: a 403 session-not-administrator on the read is a session that ended: the app signs out with session-revoked in place of signin-required', async () => {
  fakeClock()
  startAt('#/instances/default')
  fakeBff({ instanceModels: REVOKED })
  render(<App />)
  await flush()
  const main = screen.getByRole('main')
  const note = await within(main).findByText(en('session-revoked'))
  expect(note.getAttribute('role'), 'the paragraph is an alert').toBe('alert')
  expect(
    within(main).queryByText(en('signin-required')),
    'in place of signin-required',
  ).toBeNull()
  expect(within(main).queryByRole('table'), 'no table').toBeNull()
})

test('16 17e.2: a row without a non-empty string id is left out, and a non-boolean effective or enabled reads false', async () => {
  const rows = [
    row('Kept'),
    { ...row('Empty id'), id: '' },
    { ...row('Null id'), id: null as unknown as string },
    { ...row('Number id'), id: 7 as unknown as string },
    row('Words', {
      enabled: 'true' as unknown as boolean,
      effective: 'true' as unknown as boolean,
    }),
  ]
  const { section } = await loadedModels({
    instanceModels: doc(rows),
  })
  expect(
    cells(section).map((c) => c[0]),
    'the ids of the rows that are shown, in order',
  ).toEqual(['Kept', 'Words'])
  expect(
    switches(section).map((s) => s.checked),
    'a non-boolean effective reads false',
  ).toEqual([true, false])
  expect(
    cells(section).map((c) => c[1]),
    'a non-boolean enabled reads false',
  ).toEqual([en('model-profiles-status-on'), en('model-profiles-status-off')])
})

// ── 17e.3: the `*` checkbox ─────────────────────────────────────────────

test('16 17e.3: the checkbox is a statement in a group labelled instance-models-all-label, its own label instance-models-all, ticked when all is inherit; unchecking sends {"all":"none"}, the status says instance-models-all-off, and the section is read again', async () => {
  const { bff, section } = await loadedModels()
  const box = allBox(section)
  expect(box.getAttribute('type'), 'a checkbox').toBe('checkbox')
  expect(box.checked, 'all is inherit').toBe(true)
  press(box)
  await flush()
  expect(writes(bff), 'one write').toEqual([[MODELS_PATH, { all: 'none' }]])
  expect(statusText(section), 'the status').toBe(en('instance-models-all-off'))
  expect(alerts(section), 'no alert').toEqual([])
  expect(modelsReads(bff), 'read again').toBe(2)
})

test('16 17e.3: all is none: the box is unticked; ticking it sends {"all":"inherit"} and the status says instance-models-all-on', async () => {
  const { bff, section } = await loadedModels({
    instanceModels: doc(DEFAULT_ROWS, 'none'),
  })
  const box = allBox(section)
  expect(box.checked, 'all is none').toBe(false)
  press(box)
  await flush()
  expect(writes(bff), 'one write').toEqual([[MODELS_PATH, { all: 'inherit' }]])
  expect(statusText(section)).toBe(en('instance-models-all-on'))
})

test('16 17e.3 (17e.7a): a 200 with running true says instance-models-all-restart, and the section offers no Restart button of its own', async () => {
  const { bff, section } = await loadedModels({
    instanceModelsAll: () => reply(200, { running: true }),
  })
  press(allBox(section))
  await flush()
  expect(statusText(section)).toBe(en('instance-models-all-restart'))
  expect(
    within(section).queryByRole('button', { name: en('run-restart') }),
    '17e.7a: no second Restart on a page that has one',
  ).toBeNull()
  expect(modelsReads(bff), 'read again').toBe(2)
})

// ── 17e.4a: the table ───────────────────────────────────────────────────

test('16 17e.4a: the table has the three columns in order and no Actions column, one row per profile in the reply’s order, the id a link to its page, In the list the On/Off of the pool, and the switch checked by effective with its word beside it', async () => {
  const { section } = await loadedModels()
  expect(
    within(table(section))
      .getAllByRole('columnheader')
      .map((h) => (h.textContent ?? '').trim()),
    'the columns, and no Actions column',
  ).toEqual([
    en('model-profiles-column-name'),
    en('instance-models-column-pool'),
    en('instance-models-column-here'),
  ])
  expect(dataRows(section)).toHaveLength(DEFAULT_ROWS.length)
  expect(
    cells(section).map((c) => c[0]),
    'the rows in the reply’s order',
  ).toEqual(DEFAULT_ROWS.map((p) => p.id))
  DEFAULT_ROWS.forEach((p, i) => {
    const r = dataRows(section)[i]!
    expect(
      within(r).getByRole('link', { name: p.id }).getAttribute('href'),
      `${p.id}: the link to the profile’s page`,
    ).toBe(modelProfileHref(p.id))
    expect(cells(section)[i]![1], `${p.id}: In the list`).toBe(
      p.enabled
        ? en('model-profiles-status-on')
        : en('model-profiles-status-off'),
    )
    const sw = switchOf(section, p.id)
    expect(sw.checked, `${p.id}: checked by effective`).toBe(p.effective)
    expect(nameOf(sw), `${p.id}: the switch is named by the id`).toBe(p.id)
  })
})

test('16 17e.4a: the switch is named with aria-labelledby the span holding the id link, never with aria-label, and its On/Off word is aria-hidden', async () => {
  const { section } = await loadedModels()
  const sw = switchOf(section, 'Qwen3-27B')
  const by = sw.getAttribute('aria-labelledby')
  expect(by, 'aria-labelledby names the id’s span').not.toBeNull()
  expect(sw.getAttribute('aria-label'), 'no aria-label').toBeNull()
  const span = document.getElementById(by!)
  expect(span, 'a span with that id').not.toBeNull()
  expect(span!.tagName, 'a span').toBe('SPAN')
  expect(span!.querySelector('a')?.textContent, 'it holds the id link').toBe(
    'Qwen3-27B',
  )
  const word = sw.closest('td')!.querySelector('span[aria-hidden="true"]')
  expect(word, 'the word beside the switch').not.toBeNull()
  expect(word!.textContent, 'the word').toBe(en('model-profiles-status-on'))
})

test('16 17e.4a: the Served here cell is a .status-cell whose switch and word are carried by an inner span, and the sheet keeps the cell narrow and its span a flex row (22 D4.25)', async () => {
  const { section } = await loadedModels()
  const cell = switchOf(section, 'Qwen3-27B').closest('td')!
  expect(cell.className, 'the cell').toContain('status-cell')
  const inner = cell.querySelector(':scope > span')
  expect(inner, 'an inner span carries the switch and its word').not.toBeNull()
  expect(
    inner!.contains(switchOf(section, 'Qwen3-27B')),
    'the switch is inside it',
  ).toBe(true)
  const css = readFileSync(join(import.meta.dirname, 'index.css'), 'utf8')
    .replace(/\/\*[\s\S]*?\*\//g, '')
    .replace(/\s+/g, ' ')
  const cellRule = /\.status-cell \{([^}]*)\}/.exec(css)
  expect(cellRule, 'the sheet styles .status-cell').not.toBeNull()
  expect(cellRule![1]!, 'as narrow as its content').toContain('width: 1%')
  expect(cellRule![1]!, 'one line').toContain('white-space: nowrap')
  const spanRule = /\.status-cell > span \{([^}]*)\}/.exec(css)
  expect(spanRule, 'the sheet styles the inner span').not.toBeNull()
  expect(spanRule![1]!, 'the layout is the span’s, never the td’s').toContain(
    'display: flex',
  )
})

test('16 17e.4a, 17e.12: the table is inside a .table-scroll box, and the section’s one status line sits right after that box', async () => {
  const { section } = await loadedModels()
  const t = table(section)
  const scroll = t.closest('.table-scroll')
  expect(scroll, 'the box that scrolls').not.toBeNull()
  expect(before(scroll!, statusLine(section)), 'after the table').toBe(true)
})

test('16 17e.12: the alert and its Sign in land after the table too, and the sheet gives the status line margin-top --space-2 and no min-height', async () => {
  const css = readFileSync(join(import.meta.dirname, 'index.css'), 'utf8')
    .replace(/\/\*[\s\S]*?\*\//g, '')
    .replace(/\s+/g, ' ')
  const rule = /\.instance-models-status \{([^}]*)\}/.exec(css)
  expect(rule, 'the sheet styles the status line').not.toBeNull()
  expect(rule![1]!, 'the space above the line').toContain(
    'margin-top: var(--space-2)',
  )
  expect(
    rule![1]!,
    'no line is reserved: text landing below the table moves nothing above it',
  ).not.toContain('min-height')
  // The alert and the Sign in of a write that ended the session.
  const { section } = await loadedModels({
    instanceModelsProfile: SESSION_ENDED,
  })
  const scroll = table(section).closest('.table-scroll')!
  press(switchOf(section, 'Qwen3-27B'))
  await flush()
  const alert = within(section).getByRole('alert')
  expect(before(scroll, alert), 'the alert is after the table').toBe(true)
  const signIn = signIns(section)[0]!
  expect(before(scroll, signIn), 'the Sign in is after the table').toBe(true)
  expect(before(statusLine(section), alert), 'the status, then the alert').toBe(
    true,
  )
})

test('16 17e.13: the "In the list" column is as narrow as its content, so it sits beside the state column at the right', async () => {
  const css = readFileSync(join(import.meta.dirname, 'index.css'), 'utf8')
    .replace(/\/\*[\s\S]*?\*\//g, '')
    .replace(/\s+/g, ' ')
  const { section } = await loadedModels()
  const t = table(section)
  const heads = within(t).getAllByRole('columnheader')
  const at = heads.findIndex(
    (h) => (h.textContent ?? '').trim() === en('instance-models-column-pool'),
  )
  expect(at, 'the table has the In the list column').toBeGreaterThan(-1)
  const cell = dataRows(section)[0]!.querySelectorAll('td, th')[at]!
  // The declarations that reach the cell: the sheet styles it through a class,
  // so the pin follows whatever name the coder gives it.
  const classes = [...cell.classList]
  expect(classes, 'the cell names a class the sheet styles').not.toEqual([])
  const body = classes
    .map((c) => new RegExp(`\\.${c} \\{([^}]*)\\}`).exec(css)?.[1] ?? '')
    .filter((b) => b !== '')
    .join('; ')
  expect(body, 'the sheet styles the cell').not.toBe('')
  expect(body, 'as narrow as its content').toContain('width: 1%')
  expect(body, 'one line, so the two state columns sit together').toContain(
    'white-space: nowrap',
  )
})

test('16 17e.4: profiles [] shows instance-models-empty and no table, with the checkbox kept', async () => {
  const { section } = await loadedSection({ instanceModels: doc([]) })
  expect(within(section).queryByRole('table'), 'no table').toBeNull()
  expect(within(section).getByText(en('instance-models-empty'))).toBeTruthy()
  expect(allBox(section), 'the checkbox stays').toBeTruthy()
})

// ── 17e.5: one write at a time ──────────────────────────────────────────

test('16 17e.5: while a row write runs the checkbox and every switch are aria-disabled, a click sends nothing, and the status reads instance-models-saving', async () => {
  const gate = held()
  const { bff, section } = await loadedModels({
    instanceModelsProfile: (_body, _id, _profile, _n, init) =>
      gate.answer(init),
  })
  press(switchOf(section, 'Qwen3-27B'))
  await flush()
  expect(statusText(section), 'the status').toBe(en('instance-models-saving'))
  const box = allBox(section)
  expect(inactive(box), 'the checkbox is aria-disabled').toBe(true)
  expect(switches(section).map(inactive), 'every switch').toEqual([
    true,
    true,
    true,
  ])
  press(switchOf(section, 'Coder-30B'))
  press(box)
  await flush()
  expect(writes(bff), 'a click sends nothing').toHaveLength(1)
  gate.resolve(reply(200, { effective: row('Qwen3-27B'), running: false }))
  await flush()
  expect(switches(section).map(inactive), 'live again').toEqual([
    false,
    false,
    false,
  ])
  expect(inactive(box), 'the checkbox too').toBe(false)
  expect(statusText(section), 'the status keeps the answer').toBe(
    en('instance-models-on', 'Qwen3-27B'),
  )
})

test('16 17e.5: while the `*` write runs, every row switch is aria-disabled and a click sends nothing', async () => {
  const gate = held()
  const { bff, section } = await loadedModels({
    instanceModelsAll: (_body, _id, _n, init) => gate.answer(init),
  })
  press(allBox(section))
  await flush()
  expect(statusText(section)).toBe(en('instance-models-saving'))
  expect(switches(section).map(inactive), 'every switch').toEqual([
    true,
    true,
    true,
  ])
  press(switchOf(section, 'Qwen3-27B'))
  await flush()
  expect(writes(bff), 'a click sends nothing').toHaveLength(1)
  gate.resolve(reply(200, { running: false }))
  await flush()
})

// ── 17e.6: the word sent ────────────────────────────────────────────────

const WORDS: [
  string,
  string,
  InstanceModelRow,
  InstanceModels['all'],
  boolean,
][] = [
  // On in the list, served through `*`: switching off says off, so the row
  // keeps following the list's own switch.
  [
    'off a row on in the list with all inherit',
    'off',
    row('Qwen3-27B'),
    'inherit',
    false,
  ],
  // Off in the list, served by an explicit on: switching off drops the key.
  [
    'off a row off in the list served by an explicit on',
    'none',
    row('Coder-30B', {
      enabled: false,
      value: 'on',
      viaAll: false,
      effective: true,
    }),
    'inherit',
    false,
  ],
  // Off in the list, not served: switching on says on, since the list's
  // switch is off.
  [
    'on a row off in the list',
    'on',
    row('Old-Coder', {
      enabled: false,
      value: 'off',
      viaAll: false,
      effective: false,
    }),
    'inherit',
    true,
  ],
  // On in the list, refused by an explicit off: switching on says inherit.
  [
    'on a row on in the list refused by an explicit off',
    'inherit',
    row('Qwen3-27B', { value: 'off', viaAll: false, effective: false }),
    'inherit',
    true,
  ],
  // With `*` none, switching off an on row says none. The row is served
  // (effective true), else the press switches it ON and sends the on word.
  [
    'off a row on in the list with all none',
    'none',
    row('Qwen3-27B', { effective: true }),
    'none',
    false,
  ],
]

test.each(WORDS)(
  '16 17e.6: switching a row %s sends {"value":"%s"} to the percent-encoded path',
  async (_what, value, r, all, checked) => {
    const { bff, section } = await loadedModels({
      instanceModels: doc([r], all),
    })
    const sw = switchOf(section, r.id)
    expect(sw.checked, 'the row starts as effective').toBe(r.effective)
    press(sw)
    await flush()
    expect(sw.checked, 'the switch moved').toBe(checked)
    expect(writes(bff), 'the write').toEqual([[rowPath(r.id), { value }]])
  },
)

test('16 17e.6: a profile id with a slash and a space is percent-encoded in the path, and the fake decodes it back', async () => {
  const id = 'edge/box 1'
  const { bff, section } = await loadedModels({
    instanceModels: doc([
      row(id, {
        enabled: false,
        value: 'off',
        viaAll: false,
        effective: false,
      }),
    ]),
  })
  press(switchOf(section, id))
  await flush()
  expect(
    bff.calls.filter((c) => c.method === 'PUT').map((c) => c.path),
    'the path as it went on the wire',
  ).toEqual([`${MODELS_PATH}/edge%2Fbox%201`])
})

// ── 17e.7a: the answer ──────────────────────────────────────────────────

test('16 17e.7a: 200: the switch takes the reply’s effective, not the one asked, the status says instance-models-off with the id, and nothing is read again', async () => {
  const { bff, section } = await loadedModels({
    instanceModelsProfile: () =>
      reply(200, {
        effective: row('Old-Coder', {
          enabled: false,
          value: 'on',
          effective: false,
        }),
        running: false,
      }),
  })
  const sw = switchOf(section, 'Old-Coder')
  press(sw)
  await flush()
  expect(sw.checked, 'the reply’s effective, not the on asked').toBe(false)
  expect(statusText(section)).toBe(en('instance-models-off', 'Old-Coder'))
  expect(modelsReads(bff), 'no re-read on success').toBe(1)
})

test('16 17e.7a: running true: the status says instance-models-on-restart for a row that lands served and -off-restart for one that lands not served, with no Restart button in the section and no focus move', async () => {
  for (const [effective, text] of [
    [true, en('instance-models-on-restart', 'Old-Coder')],
    [false, en('instance-models-off-restart', 'Old-Coder')],
  ] as const) {
    const { bff, section } = await loadedModels({
      instanceModelsProfile: () =>
        reply(200, {
          effective: row('Old-Coder', { enabled: false, effective }),
          running: true,
        }),
    })
    const sw = switchOf(section, 'Old-Coder')
    press(sw)
    await flush()
    expect(statusText(section), text).toBe(text)
    expect(
      within(section).queryByRole('button', { name: en('run-restart') }),
      '17e.7a: no second Restart',
    ).toBeNull()
    expect(document.activeElement, 'the focus stays on the switch').toBe(sw)
    expect(modelsReads(bff), 'no re-read').toBe(1)
    cleanup()
    vi.useRealTimers()
    vi.unstubAllGlobals()
  }
})

// ── 17e.8: refusals, first ──────────────────────────────────────────────

const WRITE_REFUSALS: [string, () => Response | Promise<Response>, string][] = [
  [
    '422 model-profile-over-memory',
    () =>
      reply(422, {
        key: 'model-profile-over-memory',
        args: ['Qwen3-27B', 'default', '40000', '20000'],
      }),
    en(
      'api-model-profile-over-memory',
      'Qwen3-27B',
      'default',
      '40000',
      '20000',
    ),
  ],
  [
    '422 model-profile-model-missing',
    () =>
      reply(422, {
        key: 'model-profile-model-missing',
        args: ['Qwen3-27B', '/m/qwen.gguf'],
      }),
    en('api-model-profile-model-missing', 'Qwen3-27B', '/m/qwen.gguf'),
  ],
  [
    '422 load-on-startup-over-max',
    () => reply(422, { key: 'load-on-startup-over-max', args: ['3', '2'] }),
    en('api-load-on-startup-over-max', '3', '2'),
  ],
  [
    '422 load-on-startup-unlimited',
    () => reply(422, { key: 'load-on-startup-unlimited', args: ['3'] }),
    en('api-load-on-startup-unlimited', '3'),
  ],
  [
    '422 a key the fe has no text for',
    () => reply(422, { key: 'model-profile-no-model', args: ['zz'] }),
    en('api-other', 'model-profile-no-model'),
  ],
  [
    '422 model-profile-bad-value (17e.8a: the page never sends a wrong word)',
    () => reply(422, { key: 'model-profile-bad-value', args: ['zz'] }),
    en('request-failed'),
  ],
  [
    '404 no-model-profile-named',
    () => reply(404, { key: 'no-model-profile-named', args: ['Qwen3-27B'] }),
    en('api-no-model-profile-named', 'Qwen3-27B'),
  ],
  [
    '404 client-no-instance',
    () => reply(404, { key: 'client-no-instance', args: [] }),
    en('instance-not-found', 'default'),
  ],
  [
    '503 client-unavailable (17e.8a: the write may have landed; the re-read shows the record)',
    () => reply(503, { key: 'client-unavailable', args: [] }),
    en('api-client-unavailable'),
  ],
  [
    '503 not the error shape (17e.8a: the outcome is unknown, so the same text)',
    () => reply(503, { error: 'zz' }),
    en('api-client-unavailable'),
  ],
  [
    '502 client-not-authorized',
    () => reply(502, { key: 'client-not-authorized', args: [] }),
    en('api-client-not-authorized'),
  ],
  [
    '502 client-other',
    () => reply(502, { key: 'client-other', args: ['zz'] }),
    en('request-failed'),
  ],
  [
    '400 assignment-malformed (17e.8a)',
    () => reply(400, { key: 'assignment-malformed', args: [] }),
    en('request-failed'),
  ],
  [
    '403 client-not-authorized',
    () => reply(403, { key: 'client-not-authorized', args: [] }),
    en('api-client-not-authorized'),
  ],
  [
    '403 another key',
    () => reply(403, { key: 'zz', args: [] }),
    en('request-failed'),
  ],
  ['500 not json', () => reply(500, 'zz'), en('request-failed')],
  ['network', () => Promise.reject(new TypeError('zz')), en('request-failed')],
]

test.each(WRITE_REFUSALS)(
  '16 17e.8 (17e.8a): a refused row write shows its text in the section’s alert, the section is read again so each switch shows what the record holds, and the focus stays on the switch used: %s',
  async (what, instanceModelsProfile, text) => {
    let reads = 0
    const { bff, section } = await loadedModels({
      instanceModelsProfile,
      // The re-read answers the record: the row is served, and it is the only
      // one, so a table of one row shows the test read the record again.
      instanceModels: () =>
        ++reads === 1
          ? reply(200, doc(DEFAULT_ROWS))
          : reply(
              200,
              doc([row('Qwen3-27B', { value: 'none', effective: true })]),
            ),
    })
    const sw = switchOf(section, 'Qwen3-27B')
    press(sw)
    await flush()
    expect(alerts(section), `${what}: the alert`).toEqual([text])
    expect(statusText(section), `${what}: the status is empty`).toBe('')
    expect(modelsReads(bff), `${what}: read again`).toBe(2)
    expect(dataRows(section)).toHaveLength(1)
    expect(
      switchOf(section, 'Qwen3-27B').checked,
      `${what}: the switch shows the record`,
    ).toBe(true)
    expect(document.activeElement, `${what}: the focus stays`).toBe(sw)
    expect(document.body.textContent, what).not.toContain('zz')
  },
)

test('16 17e.8: a 401 on a row write shows session-ended-retry and a Sign in after the alert, the focus back on the switch used, the section read again, and the page stays signed in', async () => {
  const { bff, section } = await loadedModels({
    instanceModelsProfile: SESSION_ENDED,
  })
  const sw = switchOf(section, 'Qwen3-27B')
  press(sw)
  await flush()
  expect(alerts(section)).toEqual([en('session-ended-retry')])
  expect(statusText(section)).toBe('')
  const buttons = signIns(section)
  expect(buttons, 'one Sign in').toHaveLength(1)
  expect(
    before(within(section).getByRole('alert'), buttons[0]!),
    'Sign in after the alert',
  ).toBe(true)
  expect(sw.checked, 'the switch shows the value read again').toBe(true)
  expect(modelsReads(bff), 'read again').toBe(2)
  expect(document.activeElement, 'the focus stays on the switch').toBe(sw)
  expect(screen.getByRole('banner').textContent).toContain(
    en('session-signed-in-as', 'alice'),
  )
})

test('16 17e.1, 17e.8: signing in as another administrator after a row 401 reads the section again for the new session', async () => {
  const { bff, section } = await loadedModels({
    instanceModelsProfile: SESSION_ENDED,
    signIn: () => reply(200, { user: 'carol' }),
  })
  press(switchOf(section, 'Qwen3-27B'))
  await flush()
  press(signIns(section)[0]!)
  await flush()
  await signInInDialog('carol')
  expect(screen.getByRole('banner').textContent).toContain(
    en('session-signed-in-as', 'carol'),
  )
  expect(alerts(section), 'the alert is gone').toEqual([])
  expect(signIns(section), 'no Sign in').toEqual([])
  expect(modelsReads(bff), 'read again for the new session').toBe(3)
})

test('16 17e.8: a 403 session-not-administrator on a row write is the session ended: session-revoked in the alert, a Sign in after it, the section read again, the focus on the switch', async () => {
  const { bff, section } = await loadedModels({
    instanceModelsProfile: REVOKED,
  })
  const sw = switchOf(section, 'Qwen3-27B')
  press(sw)
  await flush()
  expect(alerts(section)).toEqual([en('session-revoked')])
  expect(signIns(section), 'a Sign in').toHaveLength(1)
  expect(modelsReads(bff), 'read again').toBe(2)
  expect(document.activeElement).toBe(sw)
})

/** A re-read that fails after a refused write: its text, and whether it ends
 *  the session (401/403), which is when the alert gains a Sign in. */
const REREAD_FAILURES: [string, () => Response, string, boolean][] = [
  [
    '503 client-unavailable',
    () => reply(503, { key: 'client-unavailable', args: [] }),
    en('api-client-unavailable'),
    false,
  ],
  ['401 session-required', SESSION_ENDED, en('session-ended-retry'), true],
  ['403 session-not-administrator', REVOKED, en('session-revoked'), true],
]

test.each(REREAD_FAILURES)(
  '16 17e.8 (17e review D1): a refused row write whose re-read also fails (%s) leaves the switch back at its start state, and the alert keeps the write’s own refusal text',
  async (what, rereadReply, rereadText, signIn) => {
    let reads = 0
    const { bff, section } = await loadedModels({
      instanceModelsProfile: () =>
        reply(422, {
          key: 'model-profile-over-memory',
          args: ['Qwen3-27B', 'default', '40000', '20000'],
        }),
      instanceModels: () =>
        ++reads === 1 ? reply(200, doc(DEFAULT_ROWS)) : rereadReply(),
    })
    const sw = switchOf(section, 'Qwen3-27B')
    expect(sw.checked, 'the row starts served').toBe(true)
    press(sw)
    await flush()
    expect(
      sw.checked,
      `${what}: the switch is back at its start state, not the optimistic off`,
    ).toBe(true)
    expect(alerts(section), `${what}: the write’s own refusal text`).toEqual([
      en(
        'api-model-profile-over-memory',
        'Qwen3-27B',
        'default',
        '40000',
        '20000',
      ),
    ])
    expect(
      within(section).queryByText(rereadText),
      `${what}: the re-read’s text does not replace it`,
    ).toBeNull()
    expect(signIns(section), `${what}: Sign in`).toHaveLength(signIn ? 1 : 0)
    expect(statusText(section), `${what}: the status is empty`).toBe('')
    expect(modelsReads(bff), `${what}: the record was read again`).toBe(2)
  },
)

// ── 17e.9: the checkbox's failures ──────────────────────────────────────

test('16 17e.9: a refused `*` write shows its text in the same alert, the section is read again so the checkbox shows what the record holds, and the focus stays on the checkbox', async () => {
  let reads = 0
  const { bff, section } = await loadedModels({
    instanceModelsAll: () =>
      reply(400, { key: 'assignment-malformed', args: [] }),
    instanceModels: () =>
      ++reads === 1
        ? reply(200, doc(DEFAULT_ROWS, 'inherit'))
        : reply(200, doc(DEFAULT_ROWS, 'none')),
  })
  const box = allBox(section)
  press(box)
  await flush()
  expect(alerts(section)).toEqual([en('request-failed')])
  expect(statusText(section)).toBe('')
  expect(box.checked, 'the record says none: unticked').toBe(false)
  expect(modelsReads(bff), 'read again').toBe(2)
  expect(document.activeElement, 'the focus stays on the checkbox').toBe(box)
})

test('16 17e.9: a 401 on the `*` write shows session-ended-retry and a Sign in, the section read again, the focus on the checkbox', async () => {
  const { bff, section } = await loadedModels({
    instanceModelsAll: SESSION_ENDED,
  })
  const box = allBox(section)
  press(box)
  await flush()
  expect(alerts(section)).toEqual([en('session-ended-retry')])
  expect(signIns(section), 'a Sign in').toHaveLength(1)
  expect(modelsReads(bff), 'read again').toBe(2)
  expect(document.activeElement).toBe(box)
})

test.each(REREAD_FAILURES)(
  '16 17e.9 (17e review D1): a refused `*` write whose re-read also fails (%s) leaves the checkbox back at its start state, and the alert keeps the write’s own refusal text',
  async (what, rereadReply, rereadText, signIn) => {
    let reads = 0
    const { bff, section } = await loadedModels({
      instanceModelsAll: () =>
        reply(400, { key: 'assignment-malformed', args: [] }),
      instanceModels: () =>
        ++reads === 1
          ? reply(200, doc(DEFAULT_ROWS, 'inherit'))
          : rereadReply(),
    })
    const box = allBox(section)
    expect(box.checked, 'the record says inherit: ticked').toBe(true)
    press(box)
    await flush()
    expect(
      box.checked,
      `${what}: the box is back at its start state, not the optimistic unticked`,
    ).toBe(true)
    expect(alerts(section), `${what}: the write’s own refusal text`).toEqual([
      en('request-failed'),
    ])
    expect(
      within(section).queryByText(rereadText),
      `${what}: the re-read’s text does not replace it`,
    ).toBeNull()
    expect(signIns(section), `${what}: Sign in`).toHaveLength(signIn ? 1 : 0)
    expect(statusText(section), `${what}: the status is empty`).toBe('')
    expect(modelsReads(bff), `${what}: the record was read again`).toBe(2)
  },
)

test('16 17e.9: the page never blocks Start on its own count of served profiles: with none served, Start is offered', async () => {
  const { main } = await loadedSection({ instanceModels: doc([]) })
  expect(
    within(main).getByRole('button', { name: en('run-start') }),
    'the manager decides (17c.9)',
  ).toBeTruthy()
})

// ── 17e.10a: keyboard ───────────────────────────────────────────────────

test('16 17e.10a: Tab goes the checkbox, then each row’s id link and switch in row order', async () => {
  const { section } = await loadedModels()
  expect(
    tabbable()
      .filter((el) => section.contains(el))
      .map(nameOf),
    'the section’s stops',
  ).toEqual([
    en('instance-models-all'),
    ...DEFAULT_ROWS.flatMap((p) => [p.id, p.id]),
  ])
})

test('16 17e.10a: Space toggles a focused switch, and the focus stays on it through the answer', async () => {
  const { bff, section } = await loadedModels()
  const sw = switchOf(section, 'Coder-30B')
  sw.focus()
  fireEvent.keyDown(sw, { key: ' ', code: 'Space' })
  fireEvent.keyUp(sw, { key: ' ', code: 'Space' })
  fireEvent.click(sw)
  await flush()
  expect(sw.checked, 'the switch moved').toBe(false)
  expect(writes(bff)).toEqual([[rowPath('Coder-30B'), { value: 'none' }]])
  expect(document.activeElement, 'the focus stays').toBe(sw)
  expect(modelsReads(bff), 'no re-read on success').toBe(1)
})

// ── 17e.11: strings ─────────────────────────────────────────────────────

/** 17e.11: the keys the strings file gains, exactly 17e's of section 2. */
const GAINS = [
  'instance-models-title',
  'instance-models-intro',
  'instance-models-loading',
  'instance-models-failed',
  'instance-models-empty',
  'instance-models-all',
  'instance-models-all-label',
  'instance-models-column-pool',
  'instance-models-column-here',
  'instance-models-saving',
  'instance-models-on',
  'instance-models-off',
  'instance-models-on-restart',
  'instance-models-off-restart',
  'instance-models-all-on',
  'instance-models-all-off',
  'instance-models-all-restart',
]

test('16 17e.11: strings/en_US.json gains exactly 17e’s keys of section 2, with the English of the brief', () => {
  const table: Record<string, string> = strings
  const missing = GAINS.filter((key) => table[key] === undefined)
  expect(missing, 'keys missing from strings/en_US.json').toEqual([])
  const wrong = GAINS.filter((key) => table[key] !== en(key))
  expect(
    wrong,
    'keys whose en_US.json text is not the English of the brief',
  ).toEqual([])
  expect(
    Object.keys(table)
      .filter((key) => key.startsWith('instance-models-'))
      .sort(),
    'nothing else in the namespace',
  ).toEqual([...GAINS].sort())
})
