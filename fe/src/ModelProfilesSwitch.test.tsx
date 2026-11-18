// The model profile table's on/off switch, slice 2c (fe/briefs/02-models.md, section
// 2c): an administrator switches a model profile at #/profiles/model with PUT
// /api/profiles/model/{id}/enabled (fe/briefs/api-contract.md); one status and one
// alert for the whole table, a re-read of the row after a failure, and a
// sign-in after a 401. The whole app renders against the fake bff (mocked
// fetch) on fake timers. "2c.48" is the brief's criterion 48. Only the
// administrator is tested: only administrators sign in to the web UI (user,
// 2026-11-15). The hit area of 39, the muting of 40 and the width of 41 are
// checked in the designer's review, not in jsdom.
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
  held,
  isHidden,
  navigate,
  reply,
  startAt,
  stubDialog,
  type BffOptions,
  type ModelProfile,
} from './test/bff.ts'

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

function modelProfile(
  id: string,
  enabled: boolean,
  model = '',
  engine = 'llama.cpp',
  engineAvailable = true,
): ModelProfile {
  return { id, engine, engineAvailable, enabled, model }
}

/** Two on, one off, in the bff's order. */
const LIST: ModelProfile[] = [
  modelProfile('Qwen3-27B', true, '/m/qwen/Qwen3-27B-Q8_0.gguf'),
  modelProfile('Coder-30B', true, '/m/coder/Coder-30B.gguf'),
  modelProfile('Old-Coder', false),
]

const IDS = LIST.map((p) => p.id)

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

/** The app at #/profiles/model as an administrator, the table loaded. */
async function open(options: BffOptions = {}) {
  fakeClock()
  startAt('#/profiles/model')
  const bff = fakeBff({ modelProfiles: LIST, ...options })
  render(<App />)
  await flush()
  const main = screen.getByRole('main')
  expect(
    within(main).queryByRole('table', { name: en('model-profiles-title') }),
    'the table is loaded',
  ).not.toBeNull()
  return { bff, main }
}

/** GET /api/profiles/model: `first` on open, then `reread(n)` for the n-th read. */
function reads(
  reread: (n: number, init?: RequestInit) => Response | Promise<Response>,
  first: ModelProfile[] = LIST,
): BffOptions['modelProfiles'] {
  let n = 0
  return (init) => (++n === 1 ? reply(200, first) : reread(n, init))
}

function modelProfilesReads(bff: ReturnType<typeof fakeBff>): number {
  return bff.to('GET', '/api/profiles/model').length
}

/** The bodies of every switch PUT, with the path each went to. */
function writes(bff: ReturnType<typeof fakeBff>): [string, unknown][] {
  return bff.calls
    .filter(
      (c) => c.method === 'PUT' && c.path.startsWith('/api/profiles/model/'),
    )
    .map((c) => [c.path, c.body])
}

function path(id: string): string {
  return `/api/profiles/model/${encodeURIComponent(id)}/enabled`
}

function table(main: HTMLElement): HTMLElement {
  return within(main).getByRole('table', { name: en('model-profiles-title') })
}

function dataRows(main: HTMLElement): HTMLElement[] {
  return within(table(main))
    .getAllByRole('row')
    .filter((row) => within(row).queryAllByRole('columnheader').length === 0)
}

/** The rows' cells' text: id, engine, On/Off. */
function rows(main: HTMLElement): string[][] {
  return dataRows(main).map((row) =>
    [...row.querySelectorAll('td, th')].map((c) =>
      (c.textContent ?? '').trim(),
    ),
  )
}

function shownIds(main: HTMLElement): string[] {
  return rows(main).map((r) => r[0]!)
}

function switchOf(main: HTMLElement, id: string): HTMLInputElement {
  return within(table(main)).getByRole('switch', {
    name: id,
  }) as HTMLInputElement
}

function switches(main: HTMLElement): HTMLInputElement[] {
  return within(table(main)).queryAllByRole('switch') as HTMLInputElement[]
}

function inactive(el: HTMLElement): boolean {
  return el.getAttribute('aria-disabled') === 'true'
}

function press(el: HTMLElement) {
  el.focus()
  fireEvent.click(el)
}

function filterField(main: HTMLElement): HTMLInputElement {
  return within(main).getByRole('searchbox', {
    name: en('model-profiles-filter-label'),
  }) as HTMLInputElement
}

function type(input: HTMLElement, value: string) {
  fireEvent.change(input, { target: { value } })
}

/** The filter's status line (2b.30): the first status of the page. */
function filterStatus(main: HTMLElement): HTMLElement {
  const all = within(main).getAllByRole('status')
  expect(all, 'the filter status and the switch status').toHaveLength(2)
  return all[0]!
}

/** The switch region's status (2c.50): the second status of the page. */
function switchStatus(main: HTMLElement): HTMLElement {
  const all = within(main).getAllByRole('status')
  expect(all, 'the filter status and the switch status').toHaveLength(2)
  return all[1]!
}

function alerts(main: HTMLElement): string[] {
  return within(main)
    .queryAllByRole('alert')
    .map((a) => a.textContent ?? '')
}

/** The switch region's Sign in buttons (outside any dialog). */
function signIns(main: HTMLElement): HTMLButtonElement[] {
  return (
    within(main).queryAllByRole('button', {
      name: en('session-sign-in'),
    }) as HTMLButtonElement[]
  ).filter((b) => b.closest('dialog') === null)
}

function h1(main: HTMLElement): HTMLElement {
  return within(main).getByRole('heading', {
    level: 1,
    name: en('model-profiles-title'),
  })
}

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

const SESSION_ENDED = () => reply(401, { key: 'session-required', args: [] })

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

/** aria-labelledby, then a <label>, then the text. */
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

// ── Failure paths first: a failed write (48) ────────────────────────────

const FAILED_WRITES: [string, () => Response | Promise<Response>, string][] = [
  [
    '403 client-not-authorized',
    () => reply(403, { key: 'client-not-authorized', args: [] }),
    en('api-client-not-authorized'),
  ],
  [
    '503 client-unavailable',
    () => reply(503, { key: 'client-unavailable', args: [] }),
    en('api-client-unavailable'),
  ],
  [
    '404 no-model-profile-named',
    () => reply(404, { key: 'no-model-profile-named', args: ['Old-Coder'] }),
    en('api-no-model-profile-named', 'Old-Coder'),
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
    '422 model-profile-no-model (unknown to the fe)',
    () =>
      reply(422, { key: 'model-profile-no-model', args: ['secret-detail'] }),
    en('api-other', 'model-profile-no-model'),
  ],
  [
    '502 client-other',
    () => reply(502, { key: 'client-other', args: ['secret-detail'] }),
    en('request-failed'),
  ],
  [
    '400 model-profiles-malformed',
    () => reply(400, { key: 'model-profiles-malformed', args: [] }),
    en('request-failed'),
  ],
  [
    '403 another key',
    () => reply(403, { key: 'secret-detail', args: [] }),
    en('request-failed'),
  ],
  ['500 not json', () => reply(500, 'secret-detail'), en('request-failed')],
  [
    '503 not the error shape',
    () => reply(503, { error: 'secret-detail' }),
    en('request-failed'),
  ],
  [
    'network',
    () => Promise.reject(new TypeError('secret-detail')),
    en('request-failed'),
  ],
  ['200 {}', () => reply(200, {}), en('request-failed')],
  [
    '200 without running',
    () => reply(200, { enabled: true }),
    en('request-failed'),
  ],
  [
    '200 enabled a string',
    () => reply(200, { enabled: 'true', running: false }),
    en('request-failed'),
  ],
  [
    '200 running a string',
    () => reply(200, { enabled: true, running: 'no' }),
    en('request-failed'),
  ],
  ['204', () => reply(204), en('request-failed')],
  ['200 not json', () => reply(200, 'secret-detail'), en('request-failed')],
]
test.each(FAILED_WRITES)(
  '2c.48, 2c.55: each failed write shows its alert by key, never the body; status empty; the row is read again and the switch takes its value; focus stays: %s',
  async (what, write, text) => {
    // Old-Coder is off; the change asks on; the read after the failure says
    // on (the write happened), which the value before (off) cannot give.
    const { bff, main } = await open({
      modelProfileSwitch: write,
      modelProfiles: reads(() =>
        reply(200, [...LIST.slice(0, 2), modelProfile('Old-Coder', true)]),
      ),
    })
    press(switchOf(main, 'Old-Coder'))
    await flush()
    expect(writes(bff), what).toEqual([[path('Old-Coder'), { enabled: true }]])
    expect(alerts(main), what).toEqual([text])
    expect(switchStatus(main).textContent, `${what}: status empty`).toBe('')
    expect(modelProfilesReads(bff), `${what}: read again once`).toBe(2)
    expect(switchOf(main, 'Old-Coder').checked, `${what}: read value`).toBe(
      true,
    )
    expect(rows(main)[2], `${what}: the word follows`).toEqual([
      'Old-Coder',
      'llama.cpp',
      en('model-profiles-status-on'),
    ])
    expect(
      switches(main).map(inactive),
      `${what}: every switch active again`,
    ).toEqual([false, false, false])
    expect(document.activeElement, `${what}: focus stays`).toBe(
      switchOf(main, 'Old-Coder'),
    )
    expect(signIns(main), `${what}: no Sign in`).toEqual([])
    expect(document.body.textContent, what).not.toContain('secret-detail')
  },
)

const lacking = [LIST[0]!, LIST[2]!]
const REREADS: [
  string,
  (n: number) => Response | Promise<Response>,
  boolean,
][] = [
  ['says on', () => reply(200, LIST), true],
  [
    'says off',
    () =>
      reply(200, [
        LIST[0]!,
        modelProfile('Coder-30B', false, '/m/coder/Coder-30B.gguf'),
        LIST[2]!,
      ]),
    false,
  ],
  // Coder-30B was on: the value before the change.
  ['lacks the row', () => reply(200, lacking), true],
  [
    'fails 503',
    () => reply(503, { key: 'client-unavailable', args: [] }),
    true,
  ],
  ['fails 401', SESSION_ENDED, true],
  ['fails network', () => Promise.reject(new TypeError('x')), true],
  ['fails malformed', () => reply(200, [{ id: 'Coder-30B' }]), true],
]
test.each(REREADS)(
  '2c.48, 2c.55: the re-read after a failure: on, off, without the row, failed (401 and a 15 s hold included): the switch shows the row read, or the value before: %s',
  async (what, reread, expected) => {
    const { bff, main } = await open({
      modelProfileSwitch: () =>
        reply(503, { key: 'client-unavailable', args: [] }),
      modelProfiles: reads(reread),
    })
    press(switchOf(main, 'Coder-30B'))
    await flush()
    expect(writes(bff), what).toEqual([[path('Coder-30B'), { enabled: false }]])
    expect(modelProfilesReads(bff), `${what}: read again once`).toBe(2)
    expect(switchOf(main, 'Coder-30B').checked, what).toBe(expected)
    expect(alerts(main), `${what}: the write's alert`).toEqual([
      en('api-client-unavailable'),
    ])
    expect(
      within(main).queryByText(en('signin-required')),
      `${what}: still signed in`,
    ).toBeNull()
    expect(
      within(screen.getByRole('banner')).queryByRole('button', {
        name: en('session-sign-out'),
      }),
      `${what}: still signed in`,
    ).not.toBeNull()
    expect(shownIds(main), `${what}: rows kept`).toEqual(IDS)
    expect(switches(main).map(inactive), what).toEqual([false, false, false])
    cleanup()
    vi.useRealTimers()
    vi.unstubAllGlobals()
  },
)

test('2c.48: a re-read held 15 000 ms is aborted; the switches stay aria-disabled until then, then show the value before', async () => {
  const h = held()
  const { bff, main } = await open({
    modelProfileSwitch: () =>
      reply(503, { key: 'client-unavailable', args: [] }),
    modelProfiles: reads((_n, init) => h.answer(init)),
  })
  expect(modelProfilesReads(bff)).toBe(1)
  press(switchOf(main, 'Qwen3-27B'))
  await flush()
  expect(h.signals, 'the re-read was sent').toHaveLength(1)
  expect(switches(main).map(inactive)).toEqual([true, true, true])
  await advance(14_999)
  expect(h.signals[0]?.aborted, 'not aborted at 14 999 ms').toBe(false)
  expect(switches(main).map(inactive)).toEqual([true, true, true])
  await advance(1)
  expect(h.signals[0]?.aborted, 'aborted at 15 000 ms').toBe(true)
  expect(switchOf(main, 'Qwen3-27B').checked, 'the value before').toBe(true)
  expect(switches(main).map(inactive)).toEqual([false, false, false])
  expect(alerts(main)).toEqual([en('api-client-unavailable')])
})

test('2c.48: only that row takes the re-read value: the order, the other rows and the filter stay as shown', async () => {
  const { main } = await open({
    modelProfileSwitch: () => reply(502, { key: 'client-other', args: [] }),
    // Another order, other rows changed, one added: none of it is shown.
    modelProfiles: reads(() =>
      reply(200, [
        modelProfile('New-One', true),
        modelProfile('Coder-30B', false, '/m/coder/Coder-30B.gguf'),
        modelProfile('Qwen3-27B', false, '/m/qwen/Qwen3-27B-Q8_0.gguf'),
        modelProfile('Old-Coder', true),
      ]),
    ),
  })
  type(filterField(main), 'coder')
  expect(shownIds(main)).toEqual(['Coder-30B', 'Old-Coder'])
  press(switchOf(main, 'Old-Coder'))
  await flush()
  expect(alerts(main)).toEqual([en('request-failed')])
  expect(filterField(main).value).toBe('coder')
  expect(rows(main)).toEqual([
    ['Coder-30B', 'llama.cpp', en('model-profiles-status-on')],
    ['Old-Coder', 'llama.cpp', en('model-profiles-status-on')],
  ])
  expect(filterStatus(main).textContent).toBe(
    en('model-profiles-filter-count', '2', '3'),
  )
  type(filterField(main), '')
  expect(shownIds(main)).toEqual(IDS)
  expect(switchOf(main, 'Qwen3-27B').checked, 'not taken').toBe(true)
})

test('2c.48: while the re-read runs every switch stays aria-disabled; a click sends nothing', async () => {
  const gate = deferred()
  const { bff, main } = await open({
    modelProfileSwitch: () =>
      reply(503, { key: 'client-unavailable', args: [] }),
    modelProfiles: reads(() => gate.promise),
  })
  press(switchOf(main, 'Qwen3-27B'))
  await flush()
  expect(modelProfilesReads(bff)).toBe(2)
  expect(switches(main).map(inactive)).toEqual([true, true, true])
  expect(switches(main).map((s) => s.disabled)).toEqual([false, false, false])
  press(switchOf(main, 'Old-Coder'))
  await flush()
  expect(writes(bff)).toHaveLength(1)
  expect(switchOf(main, 'Old-Coder').checked).toBe(false)
  await act(async () => {
    gate.resolve(reply(200, LIST))
  })
  await flush()
  expect(switches(main).map(inactive)).toEqual([false, false, false])
})

test('2c.48, 2c.55: a write held to 120 000 ms is aborted and shows request-failed, then the row is read again', async () => {
  const h = held()
  const { bff, main } = await open({
    modelProfileSwitch: (_body, _id, _n, init) => h.answer(init),
    modelProfiles: reads(() =>
      reply(200, [modelProfile('Qwen3-27B', false), ...LIST.slice(1)]),
    ),
  })
  press(switchOf(main, 'Qwen3-27B'))
  await flush()
  expect(writes(bff)).toHaveLength(1)
  await advance(119_999)
  expect(h.signals[0]?.aborted, 'not aborted at 119 999 ms').toBe(false)
  expect(alerts(main)).toEqual([])
  expect(switches(main).map(inactive)).toEqual([true, true, true])
  await advance(1)
  expect(h.signals[0]?.aborted, 'aborted at 120 000 ms').toBe(true)
  expect(alerts(main)).toEqual([en('request-failed')])
  expect(modelProfilesReads(bff)).toBe(2)
  expect(switchOf(main, 'Qwen3-27B').checked).toBe(false)
  expect(switches(main).map(inactive)).toEqual([false, false, false])
})

// ── 401 and Sign in (48, 49) ────────────────────────────────────────────

test('2c.48, 2c.50: a 401 shows session-ended-retry and a Sign in after it; the switch shows the value before; nothing is read; the page stays signed in', async () => {
  const { bff, main } = await open({ modelProfileSwitch: SESSION_ENDED })
  press(switchOf(main, 'Qwen3-27B'))
  await flush()
  expect(alerts(main)).toEqual([en('session-ended-retry')])
  expect(switchStatus(main).textContent).toBe('')
  const buttons = signIns(main)
  expect(buttons, 'one Sign in').toHaveLength(1)
  const alert = within(main).getByRole('alert')
  expect(before(alert, buttons[0]!), 'Sign in after the alert').toBe(true)
  expect(before(buttons[0]!, table(main)), 'before the table').toBe(true)
  expect(switchOf(main, 'Qwen3-27B').checked, 'the value before').toBe(true)
  expect(modelProfilesReads(bff), 'nothing read again').toBe(1)
  expect(switches(main).map(inactive)).toEqual([false, false, false])
  expect(screen.getByRole('banner').textContent).toContain(
    en('session-signed-in-as', 'alice'),
  )
  expect(within(main).queryByText(en('signin-required'))).toBeNull()
  expect(document.activeElement).toBe(switchOf(main, 'Qwen3-27B'))
})

test('2c.49: the Sign in opens the sign-in dialog; Escape and Cancel close it, focus back on it, alert kept, nothing sent', async () => {
  for (const how of ['Escape', 'Cancel']) {
    const { bff, main } = await open({ modelProfileSwitch: SESSION_ENDED })
    press(switchOf(main, 'Qwen3-27B'))
    await flush()
    press(signIns(main)[0]!)
    await flush()
    const d = screen.getByRole('dialog', { name: en('signin-title') })
    expect(d.tagName, how).toBe('DIALOG')
    expect(dialogs.opened, how).toContain(d)
    const user = within(d).getByRole('textbox', {
      name: en('signin-username'),
    })
    if (how === 'Escape') {
      fireEvent.keyDown(user, { key: 'Escape', code: 'Escape' })
    } else {
      fireEvent.click(
        within(d).getByRole('button', { name: en('signin-cancel') }),
      )
    }
    await flush()
    expect(screen.queryByRole('dialog'), how).toBeNull()
    expect(document.activeElement, `${how}: focus on the Sign in`).toBe(
      signIns(main)[0],
    )
    expect(alerts(main), how).toEqual([en('session-ended-retry')])
    expect(bff.to('POST', '/api/session'), how).toHaveLength(0)
    expect(writes(bff), `${how}: nothing resent`).toHaveLength(1)
    cleanup()
    vi.useRealTimers()
    vi.unstubAllGlobals()
  }
})

test('2c.49: the same administrator signs in again: alert and Sign in go, focus on the switch showing its value before, nothing resent or read', async () => {
  const { bff, main } = await open({ modelProfileSwitch: SESSION_ENDED })
  type(filterField(main), 'q')
  press(switchOf(main, 'Qwen3-27B'))
  await flush()
  press(signIns(main)[0]!)
  await flush()
  await signInInDialog('alice')
  expect(alerts(main)).toEqual([])
  expect(signIns(main)).toEqual([])
  expect(document.activeElement).toBe(switchOf(main, 'Qwen3-27B'))
  expect(switchOf(main, 'Qwen3-27B').checked).toBe(true)
  expect(filterField(main).value, 'the filter kept').toBe('q')
  expect(modelProfilesReads(bff), 'nothing read').toBe(1)
  expect(writes(bff), 'nothing resent').toHaveLength(1)
})

test('2c.49: another administrator signs in: the banner shows it, the alert goes, the model profiles are read again, the filter emptied, focus on the h1', async () => {
  const { bff, main } = await open({
    modelProfileSwitch: SESSION_ENDED,
    signIn: () => reply(200, { user: 'carol' }),
  })
  type(filterField(main), 'q')
  press(switchOf(main, 'Qwen3-27B'))
  await flush()
  press(signIns(main)[0]!)
  await flush()
  await signInInDialog('carol')
  expect(screen.getByRole('banner').textContent).toContain(
    en('session-signed-in-as', 'carol'),
  )
  expect(alerts(main)).toEqual([])
  expect(signIns(main)).toEqual([])
  expect(modelProfilesReads(bff), 'read again for the new session').toBe(2)
  expect(filterField(main).value, 'the filter emptied').toBe('')
  expect(shownIds(main)).toEqual(IDS)
  expect(document.activeElement).toBe(h1(main))
  expect(writes(bff), 'nothing resent').toHaveLength(1)
})

// ── The control (37, 38, 39, 42) ────────────────────────────────────────

test('2c.37, 2c.12, 2d.60: each row has a native checkbox with role="switch" in its Status cell, checked when on, never aria-checked; its only other control is the name link', async () => {
  const { main } = await open()
  const rs = dataRows(main)
  expect(switches(main)).toHaveLength(3)
  rs.forEach((row, i) => {
    const cells = [...row.querySelectorAll('td, th')]
    const inside = cells[2]!.querySelectorAll('input')
    expect(inside, IDS[i]).toHaveLength(1)
    const input = inside[0]!
    expect(input.getAttribute('type'), IDS[i]).toBe('checkbox')
    expect(input.getAttribute('role'), IDS[i]).toBe('switch')
    expect(input.hasAttribute('aria-checked'), IDS[i]).toBe(false)
    expect(input.checked, IDS[i]).toBe(LIST[i]!.enabled)
    // 2d.60 replaces the plain name with a link: the row's two controls are
    // that link, in the name cell, then the switch. Nothing else acts.
    const acts = [...row.querySelectorAll('a, input, button, select, textarea')]
    expect(acts, IDS[i]).toHaveLength(2)
    expect(acts[0]!.tagName, IDS[i]).toBe('A')
    expect(
      acts[0]!.closest('td, th'),
      `${IDS[i]}: the link is in the name cell`,
    ).toBe(cells[0])
    expect(acts[1], IDS[i]).toBe(input)
    expect(cells[0]!.querySelectorAll('input'), IDS[i]).toHaveLength(0)
    expect(cells[1]!.querySelectorAll('input'), IDS[i]).toHaveLength(0)
  })
})

test('2c.38: each switch is named by the id in its row, through aria-labelledby to a unique fe-made id that is not the model profile id', async () => {
  const { main } = await open({
    modelProfiles: [...LIST, modelProfile('model-profiles-title', true)],
  })
  const seen = new Set<string>()
  for (const row of dataRows(main)) {
    const nameCell = row.querySelector('td, th')!
    const id = (nameCell.textContent ?? '').trim()
    const input = row.querySelector('input')!
    const by = input.getAttribute('aria-labelledby') ?? ''
    expect(by, id).not.toBe('')
    expect(by, id).not.toBe(id)
    expect(seen.has(by), `${id}: unique`).toBe(false)
    seen.add(by)
    const target = document.getElementById(by)
    expect(target, id).not.toBeNull()
    expect(nameCell.contains(target), `${id}: in the name cell`).toBe(true)
    expect((target!.textContent ?? '').trim(), id).toBe(id)
    expect(input.getAttribute('aria-label'), id).toBeNull()
    expect(within(table(main)).getByRole('switch', { name: id }), id).toBe(
      input,
    )
  }
})

test('2c.39, 2c.54: the Status cell holds the switch then its On/Off word, the word aria-hidden', async () => {
  const { main } = await open()
  dataRows(main).forEach((row, i) => {
    const cell = row.querySelectorAll('td, th')[2]!
    const input = cell.querySelector('input')!
    const word = en(
      LIST[i]!.enabled
        ? 'model-profiles-status-on'
        : 'model-profiles-status-off',
    )
    expect((cell.textContent ?? '').trim(), IDS[i]).toBe(word)
    const holder = [...cell.querySelectorAll('*')].find(
      (el) => el !== input && (el.textContent ?? '').trim() === word,
    )
    expect(holder, `${IDS[i]}: the word in an element`).toBeDefined()
    expect(
      holder!.closest('[aria-hidden="true"]'),
      `${IDS[i]}: aria-hidden`,
    ).not.toBeNull()
    expect(before(input, holder!), `${IDS[i]}: switch first`).toBe(true)
  })
})

test('2c.42: for an administrator every switch is enabled, a row whose engine is not available included', async () => {
  const { main } = await open({
    modelProfiles: [
      ...LIST,
      modelProfile('on-strata', true, '', 'Strata', false),
    ],
  })
  for (const s of switches(main)) {
    expect(s.disabled, nameOf(s)).toBe(false)
    expect(inactive(s), nameOf(s)).toBe(false)
  }
  expect(switches(main)).toHaveLength(4)
})

test('2c.37: Enter on a switch does nothing', async () => {
  const { bff, main } = await open()
  const s = switchOf(main, 'Qwen3-27B')
  s.focus()
  fireEvent.keyDown(s, { key: 'Enter', code: 'Enter' })
  fireEvent.keyUp(s, { key: 'Enter', code: 'Enter' })
  await flush()
  expect(writes(bff)).toEqual([])
  expect(s.checked).toBe(true)
})

// ── The write (44 to 47) ────────────────────────────────────────────────

test('2c.44: a change shows the new value at once (switch and word) and sends PUT {enabled} to the id percent-encoded as a path segment', async () => {
  for (const id of ['Qwen3-27B', 'a b#c/d?e']) {
    const gate = deferred()
    const { bff, main } = await open({
      modelProfiles: [modelProfile(id, true), LIST[2]!],
      modelProfileSwitch: () => gate.promise,
    })
    press(switchOf(main, id))
    await flush()
    expect(switchOf(main, id).checked, id).toBe(false)
    expect(rows(main)[0], id).toEqual([
      id,
      'llama.cpp',
      en('model-profiles-status-off'),
    ])
    expect(writes(bff), id).toEqual([[path(id), { enabled: false }]])
    cleanup()
    vi.useRealTimers()
    vi.unstubAllGlobals()
  }
  // An off model profile asks on.
  const { bff, main } = await open()
  press(switchOf(main, 'Old-Coder'))
  await flush()
  expect(writes(bff)).toEqual([
    ['/api/profiles/model/Old-Coder/enabled', { enabled: true }],
  ])
})

test('2c.45: while a write runs every switch is aria-disabled, never disabled, a click changes and sends nothing, focus stays, no text, the filter usable', async () => {
  const gate = deferred()
  const { bff, main } = await open({ modelProfileSwitch: () => gate.promise })
  press(switchOf(main, 'Qwen3-27B'))
  await flush()
  expect(switches(main).map(inactive)).toEqual([true, true, true])
  expect(switches(main).map((s) => s.disabled)).toEqual([false, false, false])
  expect(document.activeElement).toBe(switchOf(main, 'Qwen3-27B'))
  expect(switchStatus(main).textContent).toBe('')
  expect(alerts(main)).toEqual([])
  // The same switch again, and another one.
  press(switchOf(main, 'Qwen3-27B'))
  await flush()
  expect(switchOf(main, 'Qwen3-27B').checked).toBe(false)
  press(switchOf(main, 'Old-Coder'))
  await flush()
  expect(switchOf(main, 'Old-Coder').checked).toBe(false)
  expect(writes(bff), 'no second PUT').toHaveLength(1)
  expect(document.activeElement).toBe(switchOf(main, 'Old-Coder'))
  // The switches stay in the Tab sequence.
  const order = tabbable()
  for (const s of switches(main)) expect(order).toContain(s)
  // The filter stays usable.
  type(filterField(main), 'old')
  expect(shownIds(main)).toEqual(['Old-Coder'])
  type(filterField(main), '')
  await act(async () => {
    gate.resolve(reply(200, { enabled: false, running: false }))
  })
  await flush()
  expect(switches(main).map(inactive)).toEqual([false, false, false])
})

// The reply's two booleans, the status key they give, and the row's engine:
// every case on both engines (the model profile starts as the opposite of the reply,
// so the change asks what the reply gives).
const SWITCH_REPLY_CASES: [boolean, boolean, string][] = [
  [true, false, 'model-profiles-switched-on'],
  [false, false, 'model-profiles-switched-off'],
  [true, true, 'model-profiles-switched-on-restart'],
  [false, true, 'model-profiles-switched-off-restart'],
]
const SWITCH_REPLY_ENGINES = ['llama.cpp', 'Strata']
const SWITCH_REPLIES: [boolean, boolean, string, string][] =
  SWITCH_REPLY_CASES.flatMap<[boolean, boolean, string, string]>(
    ([enabled, running, key]) =>
      SWITCH_REPLY_ENGINES.map((engine): [boolean, boolean, string, string] => [
        enabled,
        running,
        key,
        engine,
      ]),
  )

test.each(SWITCH_REPLIES)(
  '2c.46: a 2xx {enabled, running} sets the switch from the reply and the status reads by both, with the id and the row engine; nothing is read: %s %s %s',
  async (enabled, running, key, engine) => {
    const what = `${enabled} ${running} ${engine}`
    const { bff, main } = await open({
      modelProfiles: [
        modelProfile('Qwen3-27B', !enabled, '', engine, engine === 'llama.cpp'),
      ],
      modelProfileSwitch: () => reply(200, { enabled, running }),
    })
    press(switchOf(main, 'Qwen3-27B'))
    await flush()
    expect(switchOf(main, 'Qwen3-27B').checked, what).toBe(enabled)
    expect(switchStatus(main).textContent, what).toBe(
      en(key, 'Qwen3-27B', engine),
    )
    expect(alerts(main), what).toEqual([])
    expect(modelProfilesReads(bff), `${what}: nothing read`).toBe(1)
    expect(switches(main).map(inactive), what).toEqual([false])
    expect(document.activeElement, what).toBe(switchOf(main, 'Qwen3-27B'))
  },
)

test('2c.46: the reply wins over the value asked', async () => {
  const { main } = await open({
    modelProfileSwitch: () => reply(200, { enabled: true, running: false }),
  })
  press(switchOf(main, 'Qwen3-27B'))
  await flush()
  expect(switchOf(main, 'Qwen3-27B').checked).toBe(true)
  expect(rows(main)[0]![2]).toBe(en('model-profiles-status-on'))
  expect(switchStatus(main).textContent).toBe(
    en('model-profiles-switched-on', 'Qwen3-27B'),
  )
})

test('2c.47: after a success the row keeps its place, nothing is read, the filter, its rows and its count are unchanged', async () => {
  const { bff, main } = await open()
  press(switchOf(main, 'Qwen3-27B'))
  await flush()
  expect(rows(main)).toEqual([
    ['Qwen3-27B', 'llama.cpp', en('model-profiles-status-off')],
    ['Coder-30B', 'llama.cpp', en('model-profiles-status-on')],
    ['Old-Coder', 'llama.cpp', en('model-profiles-status-off')],
  ])
  press(switchOf(main, 'Old-Coder'))
  await flush()
  expect(shownIds(main)).toEqual(IDS)
  expect(modelProfilesReads(bff)).toBe(1)
  type(filterField(main), '27b')
  press(switchOf(main, 'Qwen3-27B'))
  await flush()
  expect(filterField(main).value).toBe('27b')
  expect(shownIds(main)).toEqual(['Qwen3-27B'])
  expect(filterStatus(main).textContent).toBe(
    en('model-profiles-filter-count', '1', '3'),
  )
  expect(writes(bff).map((w) => w[1])).toEqual([
    { enabled: false },
    { enabled: true },
    { enabled: true },
  ])
})

// ── Feedback region (44, 50) ────────────────────────────────────────────

test('2c.50: the status is in the page, empty, after the filter status and before the table; no alert until there is text; with the no-match text too', async () => {
  const { main } = await open()
  const status = switchStatus(main)
  expect(status.tagName).toBe('P')
  expect(status.textContent).toBe('')
  expect(before(filterStatus(main), status)).toBe(true)
  expect(before(status, table(main))).toBe(true)
  expect(alerts(main)).toEqual([])
  expect(within(main).queryAllByRole('alert')).toHaveLength(0)
  type(filterField(main), 'nothing-like-it')
  expect(within(main).queryByRole('table')).toBeNull()
  expect(switchStatus(main)).toBe(status)
  expect(status.isConnected).toBe(true)
})

test('2c.50: the alert comes after the status and before the table', async () => {
  const { main } = await open({
    modelProfileSwitch: () =>
      reply(503, { key: 'client-unavailable', args: [] }),
  })
  press(switchOf(main, 'Qwen3-27B'))
  await flush()
  const alert = within(main).getByRole('alert')
  expect(alert.tagName).toBe('P')
  expect(before(switchStatus(main), alert)).toBe(true)
  expect(before(alert, table(main))).toBe(true)
})

test('2c.44, 2c.50: typing in the filter keeps the texts; the next change empties both; leaving the page clears them', async () => {
  let n = 0
  const gate = deferred()
  const { main } = await open({
    modelProfileSwitch: () => {
      n++
      if (n === 1) return reply(200, { enabled: false, running: false })
      if (n === 2) return reply(503, { key: 'client-unavailable', args: [] })
      return gate.promise
    },
  })
  press(switchOf(main, 'Qwen3-27B'))
  await flush()
  expect(switchStatus(main).textContent).toBe(
    en('model-profiles-switched-off', 'Qwen3-27B'),
  )
  type(filterField(main), 'q')
  type(filterField(main), '')
  expect(switchStatus(main).textContent, 'typing keeps the status').toBe(
    en('model-profiles-switched-off', 'Qwen3-27B'),
  )
  press(switchOf(main, 'Coder-30B'))
  await flush()
  expect(switchStatus(main).textContent, 'the change emptied it').toBe('')
  expect(alerts(main)).toEqual([en('api-client-unavailable')])
  type(filterField(main), 'c')
  type(filterField(main), '')
  expect(alerts(main), 'typing keeps the alert').toEqual([
    en('api-client-unavailable'),
  ])
  press(switchOf(main, 'Old-Coder'))
  await flush()
  expect(alerts(main), 'the change emptied the alert').toEqual([])
  expect(switchStatus(main).textContent).toBe('')
  await act(async () => {
    gate.resolve(reply(200, { enabled: true, running: false }))
  })
  await flush()
  expect(switchStatus(main).textContent).toBe(
    en('model-profiles-switched-on', 'Old-Coder'),
  )
  navigate('#/')
  await flush()
  navigate('#/profiles/model')
  await flush()
  expect(switchStatus(main).textContent, 'cleared on leaving').toBe('')
  expect(alerts(main)).toEqual([])
})

// ── Keyboard (52, 53) ───────────────────────────────────────────────────

test('2c.53, 2d.60: Tab goes skip link, banner, nav, the filter, Clear filter, the Sign in after a 401, then each row link and switch in row order', async () => {
  // 2d.60 puts each row's link before its switch.
  const { main } = await open({ modelProfileSwitch: SESSION_ENDED })
  const head = [
    en('skip-to-content'),
    en('app-name'),
    en('nav-menu'),
    // Brief 07 H4.2: the theme switch, between Menu and the session part.
    en('theme-dark'),
    en('session-sign-out'),
    en('nav-overview'),
    'default',
    'coder',
    en('nav-model-profiles'),
    en('model-profiles-filter-label'),
  ]
  const rowsTwice = IDS.flatMap((id) => [id, id])
  expect(tabbable().map(nameOf)).toEqual([...head, ...rowsTwice])
  // "e" is in every id, so the filter shows Clear and keeps every row.
  type(filterField(main), 'e')
  expect(tabbable().map(nameOf)).toEqual([
    ...head,
    en('model-profiles-filter-clear'),
    ...rowsTwice,
  ])
  press(switchOf(main, 'Qwen3-27B'))
  await flush()
  expect(tabbable().map(nameOf)).toEqual([
    ...head,
    en('model-profiles-filter-clear'),
    en('session-sign-in'),
    ...rowsTwice,
  ])
})

test('2c.52: focus stays on the switch through a write and a failure with its re-read; no element holding focus is replaced', async () => {
  const write = deferred()
  const reread = deferred()
  const { main } = await open({
    modelProfileSwitch: () => write.promise,
    modelProfiles: reads(() => reread.promise),
  })
  const s = switchOf(main, 'Coder-30B')
  press(s)
  await flush()
  expect(document.activeElement).toBe(s)
  await act(async () => {
    write.resolve(reply(503, { key: 'client-unavailable', args: [] }))
  })
  await flush()
  expect(document.activeElement).toBe(s)
  await act(async () => {
    reread.resolve(reply(200, LIST))
  })
  await flush()
  expect(document.activeElement).toBe(s)
  expect(s.isConnected).toBe(true)
  expect(switchOf(main, 'Coder-30B')).toBe(s)
})

test('2c.55: the fake bff answers a switch with 200, the asked enabled and running false', async () => {
  const { main } = await open()
  press(switchOf(main, 'Old-Coder'))
  await flush()
  expect(switchStatus(main).textContent).toBe(
    en('model-profiles-switched-on', 'Old-Coder'),
  )
})
