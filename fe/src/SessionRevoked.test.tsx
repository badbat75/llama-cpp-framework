// The session the manager ended (fe/briefs/01-server.md, section 4c, slice
// S1): the bff answers 403 with the key session-not-administrator on any route
// but POST /api/session, and the fe treats that as a session that ended, with
// the new string session-revoked in the alert of the place it lands in. A 403
// client-not-authorized keeps today's handling. The guard's keys
// (request-cross-origin, request-bad-host, payload-too-large), boot-malformed
// and client-service-failed with no args keep the generic failure of the place
// (S1.1, S1.2). The whole app renders against the fake bff (mocked fetch);
// "S1.4" is the brief's criterion S1.4.
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
  en,
  fakeBff,
  presetDoc,
  reply,
  serverDoc,
  startAt,
  stubDialog,
  type BffOptions,
  type Preset,
} from './test/bff.ts'

/** The exact English of the one new string (S1.10). */
const REVOKED_TEXT =
  'You have been signed out: this account is no longer a member of the pai ' +
  'group. Ask whoever administers this computer to add it again; you can ' +
  'still see the overview without signing in.'

/** The bff ends the session: 403, key session-not-administrator, no args. */
const REVOKED = () => reply(403, { key: 'session-not-administrator', args: [] })

/** [name, status] of the keys that keep the place's generic failure (S1.1). */
const GUARD_KEYS: [string, number][] = [
  ['request-cross-origin', 403],
  ['request-bad-host', 403],
  ['payload-too-large', 413],
  ['boot-malformed', 400],
]

const SERVER = '/api/instances/default/server'
const BOOT = '/api/instances/default/boot'
const PUT = ['PUT', SERVER] as const

/** The reply for one row of GUARD_KEYS. */
function guard(key: string, status: number): () => Response {
  return () => reply(status, { key, args: [] })
}

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

/** Ends a loop's render: the next iteration starts from a clean page. */
function reset() {
  cleanup()
  vi.useRealTimers()
  vi.unstubAllGlobals()
}

/** The instance page at `hash`, on fake timers, with the Server form shown. */
async function loaded(options: BffOptions = {}, hash = '#/instances/default') {
  fakeClock()
  startAt(hash)
  const bff = fakeBff(options)
  render(<App />)
  await flush()
  const main = screen.getByRole('main')
  expect(
    within(main).queryByRole('heading', {
      level: 3,
      name: en('server-network'),
    }),
    'the Server form is loaded',
  ).not.toBeNull()
  return { bff, main }
}

/** The presets list at #/models, on fake timers, with the table loaded. */
async function loadedModels(options: BffOptions = {}) {
  fakeClock()
  startAt('#/models')
  const bff = fakeBff({ presets: LIST, ...options })
  render(<App />)
  await flush()
  const main = screen.getByRole('main')
  expect(
    within(main).queryByRole('table', { name: en('presets-title') }),
    'the table is loaded',
  ).not.toBeNull()
  return { bff, main }
}

/** The app at `hash`, on the real clock, against `options`. */
function openAt(hash: string, options: BffOptions = {}) {
  startAt(hash)
  const bff = fakeBff(options)
  render(<App />)
  return { bff, main: screen.getByRole('main') }
}

function nav(): HTMLElement {
  return screen.getByRole('navigation', { name: en('nav-label') })
}

/** The banner says the session lives: signed in as alice, Sign out offered. */
function expectSignedIn(what: string) {
  const banner = screen.getByRole('banner')
  expect(banner.textContent, what).toContain(
    en('session-signed-in-as', 'alice'),
  )
  expect(
    within(banner).queryByRole('button', { name: en('session-sign-out') }),
    what,
  ).not.toBeNull()
}

/** The sign-in form in place of the page: its fields and its submit. */
async function signInForm(main: HTMLElement) {
  const username = (await within(main).findByRole('textbox', {
    name: en('signin-username'),
  })) as HTMLInputElement
  const password = within(main).getByLabelText(en('signin-password'), {
    selector: 'input',
  }) as HTMLInputElement
  const submit = within(main).getByRole('button', { name: en('signin-submit') })
  return { username, password, submit }
}

function type(input: HTMLElement, value: string) {
  fireEvent.change(input, { target: { value } })
}

/** Focuses `el` and clicks it, as a pointer or Enter does. */
function press(el: HTMLElement) {
  el.focus()
  fireEvent.click(el)
}

// The instance page's header (the run controls) and the Start at boot field.

function serverTitle(main: HTMLElement): HTMLElement {
  return within(main).getByRole('heading', {
    level: 2,
    name: en('server-title'),
  })
}

function button(main: HTMLElement, key: string): HTMLButtonElement {
  return within(main).getByRole('button', { name: en(key) })
}

function runButtons(main: HTMLElement): HTMLButtonElement[] {
  return ['run-start', 'run-stop', 'run-restart'].map((k) => button(main, k))
}

/** Which of Start, Stop, Restart are inactive, as [start, stop, restart]. */
function inactiveSet(main: HTMLElement): boolean[] {
  return runButtons(main).map(
    (el) => el.getAttribute('aria-disabled') === 'true',
  )
}

/** True when `el` is in the Start at boot field, whose alerts are its own. */
function inBoot(main: HTMLElement, el: HTMLElement): boolean {
  const boot = within(main).queryByRole('group', { name: en('boot-label') })
  return boot !== null && boot.contains(el)
}

/** The run status: the one status line of the header (before the Server h2). */
function runStatus(main: HTMLElement): HTMLElement {
  const h2 = serverTitle(main)
  const all = within(main)
    .getAllByRole('status')
    .filter((s) => before(s, h2) && !inBoot(main, s))
  expect(all, 'one run status in the header').toHaveLength(1)
  return all[0]!
}

/** The texts of the run alerts (in the header, outside Start at boot). */
function runAlerts(main: HTMLElement): string[] {
  const h2 = serverTitle(main)
  return within(main)
    .queryAllByRole('alert')
    .filter((a) => before(a, h2) && !inBoot(main, a))
    .map((a) => a.textContent ?? '')
}

/** The run alert's Sign in buttons (43a), not Start at boot's. */
function runSignIns(main: HTMLElement): HTMLButtonElement[] {
  const h2 = serverTitle(main)
  return within(main)
    .queryAllByRole('button', { name: en('session-sign-in') })
    .filter((b) => before(b, h2) && !inBoot(main, b)) as HTMLButtonElement[]
}

function portBox(main: HTMLElement): HTMLInputElement {
  return within(within(main).getByRole('group', { name: 'Port' })).getByRole(
    'textbox',
    { name: 'Port' },
  ) as HTMLInputElement
}

function bootGroup(main: HTMLElement): HTMLElement {
  return within(main).getByRole('group', { name: en('boot-label') })
}

function bootBox(main: HTMLElement): HTMLInputElement {
  return within(bootGroup(main)).getByRole('checkbox', {
    name: en('boot-toggle'),
  }) as HTMLInputElement
}

function bootStatus(main: HTMLElement): HTMLElement {
  const all = within(bootGroup(main)).queryAllByRole('status')
  expect(all, 'one status line in Start at boot').toHaveLength(1)
  return all[0]!
}

function bootAlerts(main: HTMLElement): string[] {
  return within(bootGroup(main))
    .queryAllByRole('alert')
    .map((a) => a.textContent ?? '')
}

function bootSignIns(main: HTMLElement): HTMLButtonElement[] {
  return within(bootGroup(main)).queryAllByRole('button', {
    name: en('session-sign-in'),
  }) as HTMLButtonElement[]
}

function bootReads(bff: ReturnType<typeof fakeBff>): number {
  return bff.to('GET', BOOT).length
}

function bootWrites(bff: ReturnType<typeof fakeBff>): unknown[] {
  return bff.to('PUT', BOOT).map((c) => c.body)
}

function posts(bff: ReturnType<typeof fakeBff>): string[] {
  return bff.calls
    .filter((c) => c.method === 'POST' && c.path.startsWith('/api/instances'))
    .map((c) => c.path)
}

/** Boot answers: read `enabled` first, and `write` for every PUT. */
function boot(
  enabled: boolean,
  write?: (body: unknown, n: number) => Response | Promise<Response>,
): BffOptions['boot'] {
  return {
    read: () => reply(200, { enabled }),
    write: write ? (body, _id, n) => write(body, n) : undefined,
  }
}

// The presets list.

function preset(id: string, enabled: boolean, model = ''): Preset {
  return { id, engine: 'llama.cpp', engineAvailable: true, enabled, model }
}

const LIST: Preset[] = [
  preset('Qwen3-27B', true, '/m/qwen/Qwen3-27B-Q8_0.gguf'),
  preset('Coder-30B', true, '/m/coder/Coder-30B.gguf'),
  preset('Old-Coder', false),
]

function table(main: HTMLElement): HTMLElement {
  return within(main).getByRole('table', { name: en('presets-title') })
}

function switchOf(main: HTMLElement, id: string): HTMLInputElement {
  return within(table(main)).getByRole('switch', {
    name: id,
  }) as HTMLInputElement
}

function alerts(main: HTMLElement): string[] {
  return within(main)
    .queryAllByRole('alert')
    .map((a) => a.textContent ?? '')
}

/** Which presets switches are inactive (aria-disabled), in table order. */
function switchesInactive(main: HTMLElement): boolean[] {
  return within(table(main))
    .queryAllByRole('switch')
    .map((s) => s.getAttribute('aria-disabled') === 'true')
}

/** The switch region's status (2c.50): the second status of the page. */
function switchStatus(main: HTMLElement): HTMLElement {
  const all = within(main).getAllByRole('status')
  expect(all, 'the filter status and the switch status').toHaveLength(2)
  return all[1]!
}

/** The Sign in buttons of the page, outside any dialog. */
function signIns(main: HTMLElement): HTMLButtonElement[] {
  return (
    within(main).queryAllByRole('button', {
      name: en('session-sign-in'),
    }) as HTMLButtonElement[]
  ).filter((b) => b.closest('dialog') === null)
}

function presetsReads(bff: ReturnType<typeof fakeBff>): number {
  return bff.to('GET', '/api/presets').length
}

function presetWrites(bff: ReturnType<typeof fakeBff>): [string, unknown][] {
  return bff.calls
    .filter((c) => c.method === 'PUT' && c.path.startsWith('/api/presets/'))
    .map((c) => [c.path, c.body])
}

// The Server form's save bar.

function box(main: HTMLElement, label: string): HTMLInputElement {
  const g = within(main).queryByRole('group', { name: label })
  return (
    g
      ? within(g).getByRole('textbox', { name: label })
      : within(main).getByRole('textbox', { name: label })
  ) as HTMLInputElement
}

function saveButton(main: HTMLElement): HTMLElement {
  return within(main).getByRole('button', { name: en('save-button') })
}

function discardButton(main: HTMLElement): HTMLElement {
  return within(main).getByRole('button', { name: en('save-discard') })
}

/** The save bar's status line: the one status after the Server section's h2. */
function saveStatus(main: HTMLElement): HTMLElement {
  const h2 = serverTitle(main)
  const all = within(main)
    .queryAllByRole('status')
    .filter((s) => before(h2, s) && !inBoot(main, s))
  expect(all, 'one status line in the Server section').toHaveLength(1)
  return all[0]!
}

/** Clicks Save and waits until the PUT was sent (or `n` of them). */
async function clickSave(
  main: HTMLElement,
  bff: ReturnType<typeof fakeBff>,
  n = 1,
) {
  fireEvent.click(saveButton(main))
  await waitFor(() => expect(bff.to(...PUT)).toHaveLength(n))
}

/** A dirty form: Port changed from 8580 to 8581. */
function editPort(main: HTMLElement) {
  type(box(main, 'Port'), '8581')
}

/** The page with the server form loaded, on the real clock. */
async function loadedSave(options: BffOptions = {}) {
  startAt('#/instances/default')
  const bff = fakeBff(options)
  render(<App />)
  const main = screen.getByRole('main')
  await within(main).findByRole('heading', {
    level: 3,
    name: en('server-network'),
  })
  return { bff, main }
}

// ── S1.1, S1.2: the keys that keep the place's generic failure ──────────

test('S1.1: the guard keys and boot-malformed on a run action show request-failed, with no Sign in and the session kept', async () => {
  for (const [key, status] of GUARD_KEYS) {
    const what = `${status} ${key}`
    const { bff, main } = await loaded({ run: guard(key, status) })
    const stop = button(main, 'run-stop')
    press(stop)
    await flush()
    expect(runAlerts(main), what).toEqual([en('request-failed')])
    expect(runStatus(main).textContent, `${what}: status emptied`).toBe('')
    expect(document.activeElement, `${what}: focus stays on Stop`).toBe(stop)
    expect(runSignIns(main), `${what}: no Sign in`).toEqual([])
    expect(within(main).queryByText(en('session-revoked')), what).toBeNull()
    expectSignedIn(`${what}: the session lives on`)
    expect(posts(bff), what).toEqual(['/api/instances/default/stop'])
    reset()
  }
})

test('S1.1: the guard keys and boot-malformed on the boot write show request-failed, and the box shows the state read again', async () => {
  for (const [key, status] of GUARD_KEYS) {
    const what = `${status} ${key}`
    const { bff, main } = await loaded({
      boot: boot(false, guard(key, status)),
    })
    press(bootBox(main))
    await flush()
    expect(bootAlerts(main), what).toEqual([en('request-failed')])
    expect(bootStatus(main).textContent, `${what}: status emptied`).toBe('')
    expect(bootSignIns(main), `${what}: no Sign in`).toEqual([])
    expect(bootReads(bff), `${what}: the state read again`).toBe(2)
    expect(bootBox(main).checked, what).toBe(false)
    expectSignedIn(`${what}: the session lives on`)
    expect(bootWrites(bff), what).toHaveLength(1)
    reset()
  }
})

test('S1.1: the guard keys and boot-malformed on the Server save show save-failed and keep the edits', async () => {
  for (const [key, status] of GUARD_KEYS) {
    const what = `${status} ${key}`
    const { bff, main } = await loadedSave({ save: guard(key, status) })
    editPort(main)
    await clickSave(main, bff)
    const alert = await within(main).findByText(en('save-failed'))
    expect(alert.closest('[role="alert"]'), what).not.toBeNull()
    expect(box(main, 'Port').value, `${what}: the edit is kept`).toBe('8581')
    expect(saveStatus(main).textContent, what).toBe(en('save-unsaved'))
    expect(within(main).queryByText(en('session-revoked')), what).toBeNull()
    expectSignedIn(`${what}: the session lives on`)
    expect(bff.to(...PUT), what).toHaveLength(1)
    reset()
  }
})

test('S1.1: the guard keys and boot-malformed on sign-in show signin-failed and keep the app signed out', async () => {
  for (const [key, status] of GUARD_KEYS) {
    const what = `${status} ${key}`
    const { bff } = openAt('#/sign-in', {
      session: 'signed-out',
      signIn: guard(key, status),
    })
    const main = screen.getByRole('main')
    const { username, password, submit } = await signInForm(main)
    type(username, 'alice')
    type(password, 'hunter2')
    fireEvent.click(submit)
    const alert = await within(main).findByRole('alert')
    expect(alert.textContent, what).toBe(en('signin-failed'))
    expect(
      screen.queryByRole('button', { name: en('session-sign-out') }),
      what,
    ).toBeNull()
    expect(bff.to('POST', '/api/session'), what).toHaveLength(1)
    reset()
    startAt('#/sign-in')
  }
})

test('S1.1: the guard keys and boot-malformed on a preset switch show request-failed, then the row is read again, with no Sign in and the session kept', async () => {
  for (const [key, status] of GUARD_KEYS) {
    const what = `${status} ${key}`
    const { bff, main } = await loadedModels({
      presetSwitch: guard(key, status),
    })
    press(switchOf(main, 'Qwen3-27B'))
    await flush()
    expect(alerts(main), what).toEqual([en('request-failed')])
    expect(switchStatus(main).textContent, `${what}: status emptied`).toBe('')
    expect(presetsReads(bff), `${what}: the row read again`).toBe(2)
    expect(switchOf(main, 'Qwen3-27B').checked, `${what}: the list value`).toBe(
      true,
    )
    expect(signIns(main), `${what}: no Sign in`).toEqual([])
    expect(within(main).queryByText(en('session-revoked')), what).toBeNull()
    expectSignedIn(`${what}: the session lives on`)
    expect(presetWrites(bff), what).toHaveLength(1)
    reset()
  }
})

test('S1.1: the guard keys on a read show that reads generic failure, in place of the page, with no sign-in form (server, presets list, preset page)', async () => {
  const rows: [string, BffOptions, string][] = [
    [
      'GET /api/instances/{id}/server',
      { server: () => reply(403, { key: 'request-cross-origin', args: [] }) },
      en('server-load-failed'),
    ],
    [
      'GET /api/presets',
      { presets: () => reply(413, { key: 'payload-too-large', args: [] }) },
      en('presets-load-failed'),
    ],
    [
      'GET /api/presets/{id}',
      { preset: () => reply(400, { key: 'boot-malformed', args: [] }) },
      en('preset-load-failed'),
    ],
  ]
  for (const [route, options, text] of rows) {
    const hash =
      route === 'GET /api/presets'
        ? '#/models'
        : route === 'GET /api/presets/{id}'
          ? '#/models/Qwen3-27B'
          : '#/instances/default'
    openAt(hash, options)
    const main = screen.getByRole('main')
    expect(
      await within(main).findByText(text),
      `${route}: the reads message`,
    ).toBeTruthy()
    expect(
      within(main).queryByRole('textbox', { name: en('signin-username') }),
      `${route}: no sign-in form`,
    ).toBeNull()
    expect(within(main).queryByText(en('session-revoked')), route).toBeNull()
    expectSignedIn(`${route}: the session lives on`)
    reset()
  }
})

test('S1.2: client-service-failed with no args shows its own text in the run alert, with no placeholder left', async () => {
  const { main } = await loaded({
    run: () => reply(502, { key: 'client-service-failed', args: [] }),
  })
  press(button(main, 'run-stop'))
  await flush()
  const alert = within(main).getByRole('alert')
  expect(alert.textContent).toBe(en('api-client-service-failed'))
  expect(alert.textContent, 'no unfilled placeholder').not.toContain('{')
})

test('S1.2: client-service-failed with no args shows its own text in the boot alert, with no placeholder left', async () => {
  const { main } = await loaded({
    boot: boot(false, () =>
      reply(502, { key: 'client-service-failed', args: [] }),
    ),
  })
  press(bootBox(main))
  await flush()
  expect(bootAlerts(main)).toEqual([en('api-client-service-failed')])
  expect(bootAlerts(main)[0], 'no unfilled placeholder').not.toContain('{')
})

// ── S1.3: which answer means the session ended ──────────────────────────

test('S1.3: a 403 client-not-authorized keeps today handling: its own text, no Sign in, the page stays signed in', async () => {
  const { bff, main } = await loaded({
    run: () => reply(403, { key: 'client-not-authorized', args: [] }),
  })
  press(button(main, 'run-stop'))
  await flush()
  expect(runAlerts(main)).toEqual([en('api-client-not-authorized')])
  expect(runSignIns(main), 'no Sign in: the session lives').toEqual([])
  expect(within(main).queryByText(en('session-revoked'))).toBeNull()
  expectSignedIn('the session lives on')
  expect(posts(bff)).toEqual(['/api/instances/default/stop'])
})

test('S1.3: a 403 session-not-administrator on a session-free read is that reads generic failure (instances, boot)', async () => {
  openAt('#/', {
    instances: () => reply(403, { key: 'session-not-administrator', args: [] }),
  })
  const overview = screen.getByRole('main')
  expect(await within(overview).findByText(en('instances-failed'))).toBeTruthy()
  expect(
    within(overview).queryByRole('textbox', { name: en('signin-username') }),
    'no sign-in form',
  ).toBeNull()
  expectSignedIn('the Overview stays')
  reset()

  const { main } = await loaded({
    boot: { read: () => reply(403, { key: 'session-not-administrator' }) },
  })
  expect(
    within(bootGroup(main)).queryByText(en('boot-load-failed')),
    'the field shows its reads failure',
  ).toBeTruthy()
  expect(within(main).queryByText(en('session-revoked'))).toBeNull()
  expectSignedIn('the instance page stays')
})

// ── S1.4 to S1.7: a write meets the ended session ───────────────────────

test('S1.4: a 403 session-not-administrator on a run action is 43a row with session-revoked: status emptied, focus kept, Sign in after the alert, the page stays signed in', async () => {
  const { bff, main } = await loaded({ run: REVOKED })
  type(portBox(main), '8581')
  const stop = button(main, 'run-stop')
  press(stop)
  await flush()
  expect(runAlerts(main)).toEqual([en('session-revoked')])
  expect(runStatus(main).textContent, 'status emptied').toBe('')
  expect(document.activeElement, 'focus stays on Stop').toBe(stop)
  expect(inactiveSet(main), 'the buttons by the last state (40a)').toEqual([
    true,
    false,
    false,
  ])
  const signIns = runSignIns(main)
  expect(signIns, 'one Sign in in the header').toHaveLength(1)
  const alert = within(main).getByText(en('session-revoked'))
  expect(alert.getAttribute('role'), 'the alert is a role=alert').toBe('alert')
  expect(before(alert, signIns[0]!), 'Sign in after the alert').toBe(true)
  expect(
    before(signIns[0]!, serverTitle(main)),
    'before the Server section',
  ).toBe(true)
  // 36a: the page is not signed out, its edits stay.
  expectSignedIn('the page stays signed in')
  expect(within(nav()).getByRole('link', { name: 'coder' })).toBeTruthy()
  expect(portBox(main).value, 'the edit is untouched').toBe('8581')
  expect(saveStatus(main).textContent).toBe(en('save-unsaved'))
  expect(within(main).queryByText(en('signin-required'))).toBeNull()
  expect(posts(bff)).toHaveLength(1)
})

test('S1.4: after a run 403 session-not-administrator the idle poll goes on (it needs no session)', async () => {
  const { bff, main } = await loaded({ run: REVOKED })
  const opening = bff.to('GET', '/api/instances').length
  press(button(main, 'run-stop'))
  await flush()
  expect(runAlerts(main)).toEqual([en('session-revoked')])
  await advance(5000)
  expect(bff.to('GET', '/api/instances').length).toBe(opening + 1)
  await advance(5000)
  expect(bff.to('GET', '/api/instances').length).toBe(opening + 2)
})

test('S1.5: a 403 session-not-administrator on the boot write is 45e row with session-revoked: the box shows the state read again, focus on the checkbox, Sign in after the alert', async () => {
  const { bff, main } = await loaded({ boot: boot(false, REVOKED) })
  type(portBox(main), '8581')
  const boxEl = bootBox(main)
  press(boxEl)
  await flush()
  expect(bootAlerts(main)).toEqual([en('session-revoked')])
  expect(bootStatus(main).textContent, 'status emptied').toBe('')
  const signIns = bootSignIns(main)
  expect(signIns, 'one Sign in in the group').toHaveLength(1)
  const alert = within(bootGroup(main)).getByText(en('session-revoked'))
  expect(before(alert, signIns[0]!), 'Sign in after the alert').toBe(true)
  expect(document.activeElement, 'focus stays on the checkbox').toBe(
    bootBox(main),
  )
  // The box shows the state read again: GET .../boot answers, session-free.
  expect(bootReads(bff), 'read again after the refusal').toBe(2)
  expect(bootBox(main).checked).toBe(false)
  expectSignedIn('the page stays signed in')
  expect(portBox(main).value, 'the edit is untouched').toBe('8581')
  expect(within(main).queryByText(en('signin-required'))).toBeNull()
  expect(bootWrites(bff)).toHaveLength(1)
})

test('S1.6: a 403 session-not-administrator on the save is 36 row with session-revoked in the bar: edits kept and dirty, focus on Save, the bar Sign in after the alert, banner and nav stay', async () => {
  const { bff, main } = await loadedSave({ save: REVOKED })
  editPort(main)
  // press, not a bare click: a real click focuses Save, and the code must not
  // move focus away from it.
  press(saveButton(main))
  await waitFor(() => expect(bff.to(...PUT)).toHaveLength(1))
  const ended = await within(main).findByText(en('session-revoked'))
  expect(ended.closest('[role="alert"]'), 'in the bars alert').not.toBeNull()
  expect(within(main).queryByText(en('session-ended'))).toBeNull()
  expect(box(main, 'Port').value, 'the edit is kept').toBe('8581')
  expect(saveStatus(main).textContent, 'the form stays dirty').toBe(
    en('save-unsaved'),
  )
  expect(document.activeElement, 'focus stays on Save changes').toBe(
    saveButton(main),
  )
  const signIn = within(main).getByRole('button', {
    name: en('session-sign-in'),
  })
  expect(before(ended, signIn), 'the bars Sign in after the alert').toBe(true)
  expect(before(ended, discardButton(main)), 'the alert is in the bar').toBe(
    true,
  )
  // 36a: the banner and the nav stay as they were.
  expectSignedIn('the banner stays signed in')
  expect(within(nav()).getByRole('link', { name: 'coder' })).toBeTruthy()
  expect(within(main).queryByText(en('signin-required'))).toBeNull()
  expect(bff.to(...PUT)).toHaveLength(1)
})

test('S1.7: a 403 session-not-administrator on a preset switch is 2c row with session-revoked: the switch shows its value before, nothing is read again, focus on the switch, Sign in after the alert', async () => {
  const { bff, main } = await loadedModels({ presetSwitch: REVOKED })
  press(switchOf(main, 'Qwen3-27B'))
  await flush()
  expect(alerts(main)).toEqual([en('session-revoked')])
  expect(switchStatus(main).textContent, 'status emptied').toBe('')
  const buttons = signIns(main)
  expect(buttons, 'one Sign in').toHaveLength(1)
  const alert = within(main).getByRole('alert')
  expect(before(alert, buttons[0]!), 'Sign in after the alert').toBe(true)
  expect(before(buttons[0]!, table(main)), 'before the table').toBe(true)
  expect(switchOf(main, 'Qwen3-27B').checked, 'the value before').toBe(true)
  expect(presetsReads(bff), 'nothing read again').toBe(1)
  expect(switchesInactive(main), 'the switches stay active').toEqual([
    false,
    false,
    false,
  ])
  expect(document.activeElement, 'focus stays on the switch').toBe(
    switchOf(main, 'Qwen3-27B'),
  )
  expectSignedIn('the page stays signed in')
  expect(within(main).queryByText(en('signin-required'))).toBeNull()
  expect(presetWrites(bff), 'one write sent').toHaveLength(1)
})

// ── S1.8: the Sign in dialog opened from a write ────────────────────────

test('S1.8: signing in again as the same account in the dialog is 14b inside it: api-session-not-administrator, password cleared, focus on the user name, the dialog stays open with the page alert kept', async () => {
  const { bff, main } = await loaded({
    run: REVOKED,
    signIn: () => reply(403, { key: 'session-not-administrator', args: [] }),
  })
  press(button(main, 'run-stop'))
  await flush()
  expect(runAlerts(main)).toEqual([en('session-revoked')])
  press(runSignIns(main)[0]!)
  await flush()
  const d = screen.getByRole('dialog', { name: en('signin-title') })
  expect(dialogs.opened, 'opened with showModal()').toContain(d)
  const user = within(d).getByRole('textbox', {
    name: en('signin-username'),
  }) as HTMLInputElement
  const password = within(d).getByLabelText(en('signin-password'), {
    selector: 'input',
  }) as HTMLInputElement
  type(user, 'alice')
  type(password, 'hunter2')
  fireEvent.click(within(d).getByRole('button', { name: en('signin-submit') }))
  // findByRole never resolves under this file's fake clock: RTL detects the
  // fake timers and drives them through a global `jest`, which vitest has
  // not, so its promise never settles. The answer lands on microtasks.
  await flush()
  const alert = within(d).getByRole('alert')
  expect(alert.textContent, 'the dialogs own refusal').toBe(
    en('api-session-not-administrator'),
  )
  expect(password.value, 'the password is cleared').toBe('')
  expect(document.activeElement, 'focus on the user name field').toBe(user)
  expect(screen.queryByRole('dialog'), 'the dialog stays open').not.toBeNull()
  expect(runAlerts(main), 'the page keeps its alert').toEqual([
    en('session-revoked'),
  ])
  // Escape closes it as 43a: focus back on the Sign in, alert kept.
  fireEvent.keyDown(user, { key: 'Escape', code: 'Escape' })
  await flush()
  expect(screen.queryByRole('dialog'), 'Escape closed it').toBeNull()
  expect(document.activeElement, 'focus on the run Sign in').toBe(
    runSignIns(main)[0],
  )
  expect(runAlerts(main), 'the alert is kept').toEqual([en('session-revoked')])
  expect(bff.to('POST', '/api/session'), 'one sign-in sent').toHaveLength(1)
  expect(posts(bff), 'nothing resent').toHaveLength(1)
})

// ── S1.9: a read meets the ended session ────────────────────────────────

test('S1.9: a 403 session-not-administrator on the server read signs the app out, with session-revoked as an alert above the fields in place of signin-required, kept until a sign-in succeeds', async () => {
  let reads = 0
  let tries = 0
  const { bff, main } = openAt('#/instances/default', {
    server: () => (++reads === 1 ? REVOKED() : reply(200, serverDoc())),
    signIn: () =>
      ++tries === 1
        ? reply(401, { key: 'session-bad-credentials', args: [] })
        : reply(200, { user: 'bob' }),
  })
  const { username, password, submit } = await signInForm(main)
  expect(window.location.hash, 'the route stays').toBe('#/instances/default')
  const note = within(main).getByText(en('session-revoked'))
  expect(note.getAttribute('role'), 'the paragraph is an alert').toBe('alert')
  expect(before(note, username), 'above the sign-in fields').toBe(true)
  expect(within(main).queryByText(en('signin-required'))).toBeNull()
  expect(within(main).queryByText(en('server-load-failed'))).toBeNull()
  // The whole app is signed out (2.6).
  const banner = screen.getByRole('banner')
  expect(
    within(banner).queryByRole('button', { name: en('session-sign-out') }),
  ).toBeNull()
  expect(
    within(banner).queryByRole('button', { name: en('session-sign-in') }) ??
      within(banner).queryByRole('link', { name: en('session-sign-in') }),
    'the banner offers Sign in',
  ).not.toBeNull()
  expect(
    within(nav()).queryByRole('list', { name: en('nav-instances') }),
  ).toBeNull()
  // A refused sign-in keeps it.
  type(username, 'alice')
  type(password, 'hunter2')
  fireEvent.click(submit)
  expect(
    await within(main).findByText(en('api-session-bad-credentials')),
    'the refusals own text',
  ).toBeTruthy()
  expect(within(main).getByText(en('session-revoked')), 'it stays').toBeTruthy()
  // Another administrator signs in: the page comes back and reads again.
  type(username, 'bob')
  type(password, 'hunter2')
  fireEvent.click(submit)
  await within(main).findByRole('heading', {
    level: 3,
    name: en('server-network'),
  })
  expect(bff.to('GET', SERVER)).toHaveLength(2)
  expect(screen.getByRole('banner').textContent).toContain(
    en('session-signed-in-as', 'bob'),
  )
  expect(
    within(main).queryByText(en('session-revoked')),
    'it is gone',
  ).toBeNull()
})

test('S1.9: a 403 session-not-administrator on the presets read signs the app out with session-revoked in place of signin-required; after a sign-in the list reads again', async () => {
  let reads = 0
  const { bff, main } = openAt('#/models', {
    presets: () =>
      ++reads === 1 ? REVOKED() : reply(200, [preset('after-sign-in', true)]),
  })
  const { username, password, submit } = await signInForm(main)
  expect(window.location.hash, 'the route stays').toBe('#/models')
  const note = within(main).getByText(en('session-revoked'))
  expect(note.getAttribute('role'), 'the paragraph is an alert').toBe('alert')
  expect(before(note, username), 'above the sign-in fields').toBe(true)
  expect(within(main).queryByText(en('signin-required'))).toBeNull()
  expect(within(main).queryByText(en('presets-load-failed'))).toBeNull()
  expect(within(main).queryByRole('table')).toBeNull()
  const banner = screen.getByRole('banner')
  expect(
    within(banner).queryByRole('button', { name: en('session-sign-out') }),
  ).toBeNull()
  expect(
    within(nav()).queryByRole('link', { name: en('nav-models') }),
  ).toBeNull()
  type(username, 'bob')
  type(password, 'hunter2')
  fireEvent.click(submit)
  expect(
    await within(main).findByRole('table', { name: en('presets-title') }),
  ).toBeTruthy()
  expect(presetsReads(bff)).toBe(2)
  expect(within(main).queryByText(en('session-revoked'))).toBeNull()
})

test('S1.9: a 403 session-not-administrator on the preset read signs the app out with session-revoked; after a sign-in the page reads again and focus goes to its h1', async () => {
  let reads = 0
  const { bff, main } = openAt('#/models/Qwen3-27B', {
    preset: () => (++reads === 1 ? REVOKED() : reply(200, presetDoc())),
  })
  const { username, password, submit } = await signInForm(main)
  expect(window.location.hash, 'the route stays').toBe('#/models/Qwen3-27B')
  const note = within(main).getByText(en('session-revoked'))
  expect(note.getAttribute('role'), 'the paragraph is an alert').toBe('alert')
  expect(within(main).queryByText(en('signin-required'))).toBeNull()
  expect(within(main).queryByText(en('preset-load-failed'))).toBeNull()
  expect(
    within(main).queryByRole('heading', { level: 1, name: 'Qwen3-27B' }),
  ).toBeNull()
  type(username, 'bob')
  type(password, 'hunter2')
  fireEvent.click(submit)
  const h1 = await within(main).findByRole('heading', {
    level: 1,
    name: 'Qwen3-27B',
  })
  await waitFor(() => expect(document.activeElement).toBe(h1))
  expect(window.location.hash).toBe('#/models/Qwen3-27B')
  expect(
    bff.calls.filter((c) => c.path === '/api/presets/Qwen3-27B'),
  ).toHaveLength(2)
  expect(within(main).queryByText(en('session-revoked'))).toBeNull()
})

// ── S1.10: the one new string ───────────────────────────────────────────

test('S1.10: session-revoked shows the briefs English, in the run alert and in the sign-in form', async () => {
  expect(en('session-revoked')).toBe(REVOKED_TEXT)
  const { main } = await loaded({ run: REVOKED })
  press(button(main, 'run-stop'))
  await flush()
  expect(runAlerts(main)).toEqual([REVOKED_TEXT])
  reset()

  openAt('#/instances/default', {
    server: () => reply(403, { key: 'session-not-administrator', args: [] }),
  })
  const main2 = screen.getByRole('main')
  expect(await within(main2).findByText(REVOKED_TEXT)).toBeTruthy()
})
