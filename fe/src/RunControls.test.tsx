// The instance page's run controls and live state (fe/briefs/01-server.md,
// slice 1c, sections 4.1 to 4.5 and 4.7, with section 6): Start, Stop and
// Restart in the header, the one shared read of /api/instances and its polls,
// what an action shows, its errors, a sign-in after a 401, and the Server form
// left alone. Slice S2 (section 4d): a Stop or Restart that saves snapshots for
// minutes, with its 360 s deadline. Start at boot (4.6) is in Boot.test.tsx.
// The whole app renders against a fake bff (mocked fetch) on fake timers, so
// the poll times of 41d are exact; "1c.41d" is the brief's criterion 41d and
// "S2.2" is section 4d's criterion S2.2.
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  within,
} from '@testing-library/react'
import { afterEach, beforeEach, expect, it, test, vi } from 'vitest'
import App from './App.tsx'
import strings from './strings/en_US.json'
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
  type Instance,
} from './test/bff.ts'

let dialogs: ReturnType<typeof stubDialog>

beforeEach(() => {
  dialogs = stubDialog()
})

afterEach(() => {
  // Unmount first, so the page clears its own timers; then drop the fake
  // clock, which discards any timer left, so no poll reaches the next test.
  cleanup()
  vi.useRealTimers()
  vi.unstubAllGlobals()
  dialogs.restore()
  startAt('')
})

// ── Helpers ─────────────────────────────────────────────────────────────

/**
 * Fake timers for the page's polls. setImmediate and the microtask queue stay
 * real, so fetch's Response bodies are read as usual.
 */
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

/**
 * Moves the clock by `ms` in one step, then flushes. The tests step exactly
 * onto the times a read is due (41d), so a page that schedules its next read
 * after a render still sees the exact time.
 */
async function advance(ms: number) {
  await act(async () => {
    await vi.advanceTimersByTimeAsync(ms)
  })
  await flush()
}

function list(state: string, extra: Instance[] = []): Instance[] {
  return [
    { id: 'default', engine: 'llama.cpp', state },
    { id: 'coder', engine: 'llama.cpp', state: 'inactive' },
    ...extra,
  ]
}

/** The app at `hash` (signed in as an administrator by default), loaded. */
async function open(options: BffOptions = {}, hash = '#/instances/default') {
  fakeClock()
  startAt(hash)
  const bff = fakeBff(options)
  render(<App />)
  await flush()
  return { bff, main: screen.getByRole('main') }
}

/** The instance page with its header and Server form shown. */
async function loaded(options: BffOptions = {}) {
  const { bff, main } = await open(options)
  expect(
    within(main).queryByRole('heading', {
      level: 3,
      name: en('server-network'),
    }),
    'the Server form is loaded',
  ).not.toBeNull()
  return { bff, main }
}

/** How many GET /api/instances were sent. */
function reads(bff: ReturnType<typeof fakeBff>): number {
  return bff.to('GET', '/api/instances').length
}

/**
 * The Overview's instances, as the `list` of the Instances section (H6a.1 of
 * fe/briefs/08-hearth-h6-overview.md replaced the table; the `main` holds no
 * other list on the Overview).
 */
function instanceTiles(main: HTMLElement): HTMLElement {
  const heading = within(main).getByRole('heading', {
    level: 2,
    name: en('instances-title'),
  })
  const section = heading.closest('section') as HTMLElement
  expect(section).toBeTruthy()
  return within(section).getByRole('list')
}

function posts(bff: ReturnType<typeof fakeBff>): string[] {
  return bff.calls
    .filter((c) => c.method === 'POST' && c.path.startsWith('/api/instances'))
    .map((c) => c.path)
}

function button(main: HTMLElement, key: string): HTMLButtonElement {
  return within(main).getByRole('button', { name: en(key) })
}

function runButtons(main: HTMLElement): HTMLButtonElement[] {
  return ['run-start', 'run-stop', 'run-restart'].map((k) => button(main, k))
}

function inactive(el: HTMLElement): boolean {
  return el.getAttribute('aria-disabled') === 'true'
}

/** Which of Start, Stop, Restart are inactive, as [start, stop, restart]. */
function inactiveSet(main: HTMLElement): boolean[] {
  return runButtons(main).map(inactive)
}

function serverTitle(main: HTMLElement): HTMLElement {
  return within(main).getByRole('heading', {
    level: 2,
    name: en('server-title'),
  })
}

/**
 * True when `el` is in the Start at boot field, whose own status, alert and
 * Sign in (4.6, 45f) are not the run controls'.
 */
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

/** The save bar's status line: the one status of the Server section. */
function saveStatus(main: HTMLElement): HTMLElement {
  const h2 = serverTitle(main)
  const all = within(main)
    .getAllByRole('status')
    .filter((s) => before(h2, s))
  expect(all, 'one status line in the Server section').toHaveLength(1)
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

function stateText(stateKey: string): string {
  return en('instance-state', en(stateKey))
}

function expectState(main: HTMLElement, stateKey: string) {
  expect(
    within(main).queryByText(stateText(stateKey)),
    `the state line reads ${stateText(stateKey)}`,
  ).not.toBeNull()
}

/** Focuses `el` and clicks it, as a pointer or Enter does. */
function press(el: HTMLElement) {
  el.focus()
  fireEvent.click(el)
}

function type(input: HTMLElement, value: string) {
  fireEvent.change(input, { target: { value } })
}

function portBox(main: HTMLElement): HTMLInputElement {
  return within(within(main).getByRole('group', { name: 'Port' })).getByRole(
    'textbox',
    { name: 'Port' },
  )
}

function h1(main: HTMLElement): HTMLElement {
  return within(main).getByRole('heading', { level: 1, name: 'default' })
}

/** Signs in as `user` in the open sign-in dialog. */
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

// ── Failure paths first: errors of an action (43) ───────────────────────

/** 1c.43: one test per row, one mount each (as 2h.139 in
 * ModelProfileSave2.test.tsx: the cleanup between rows is the file's afterEach). */
const ANSWERS_43: [string, () => Response | Promise<Response>, string][] = [
  [
    '403 client-not-authorized',
    () => reply(403, { key: 'client-not-authorized', args: [] }),
    en('api-client-not-authorized'),
  ],
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
    '502 client-service-failed',
    () =>
      reply(502, {
        key: 'client-service-failed',
        args: ['unit failed: secret detail'],
      }),
    en('api-client-service-failed'),
  ],
  [
    '403 carrying client-unavailable (by key)',
    () => reply(403, { key: 'client-unavailable', args: [] }),
    en('api-client-unavailable'),
  ],
  [
    '502 client-other',
    () => reply(502, { key: 'client-other', args: ['secret detail'] }),
    en('request-failed'),
  ],
  [
    '422 with an unknown key (no api-other here)',
    () => reply(422, { key: 'some-check', args: ['secret detail'] }),
    en('request-failed'),
  ],
  [
    '500 with no key',
    () => reply(500, { error: 'secret detail' }),
    en('request-failed'),
  ],
  [
    '500 not JSON',
    () => reply(500, 'secret detail <b>panic</b>'),
    en('request-failed'),
  ],
  [
    'network failure',
    () => Promise.reject(new TypeError('Failed to fetch')),
    en('request-failed'),
  ],
]

it.each(ANSWERS_43)(
  '1c.43: %s: the key picks the message, never the status or the body; status emptied, focus kept, buttons by the last state',
  async (what, run, text) => {
    const { bff, main } = await loaded({ run })
    const stop = button(main, 'run-stop')
    press(stop)
    await flush()
    expect(posts(bff), what).toEqual(['/api/instances/default/stop'])
    expect(runAlerts(main), what).toEqual([text])
    expect(runStatus(main).textContent, `${what}: status emptied`).toBe('')
    expect(document.activeElement, `${what}: focus stays on Stop`).toBe(stop)
    // The last state is active: only Start is inactive (40a).
    expect(inactiveSet(main), what).toEqual([true, false, false])
    expect(runSignIns(main), `${what}: no Sign in without a 401`).toEqual([])
    expect(document.body.textContent, `${what}: never the body`).not.toContain(
      'secret detail',
    )
    expect(document.body.textContent, what).not.toContain(
      en('api-other', 'some-check'),
    )
  },
)

test('1c.43, 41c: after a failed POST the idle poll resumes, 5 s after the last answer', async () => {
  const { bff, main } = await loaded({
    run: () => reply(503, { key: 'client-unavailable', args: [] }),
  })
  const opening = reads(bff)
  press(button(main, 'run-restart'))
  await flush()
  expect(runAlerts(main)).toEqual([en('api-client-unavailable')])
  await advance(4999)
  expect(reads(bff), 'nothing before 5 s').toBe(opening)
  await advance(1)
  expect(reads(bff), 'one read at 5 s').toBe(opening + 1)
})

test('1c.43a: a 401 shows session-ended-retry and a Sign in button after the alert; banner, nav and edits stay', async () => {
  const { bff, main } = await loaded({ run: SESSION_ENDED })
  type(portBox(main), '8581')
  const stop = button(main, 'run-stop')
  press(stop)
  await flush()
  expect(runAlerts(main)).toEqual([en('session-ended-retry')])
  expect(runStatus(main).textContent).toBe('')
  expect(document.activeElement).toBe(stop)
  const signIns = runSignIns(main)
  expect(signIns, 'one Sign in in the header').toHaveLength(1)
  const alert = within(main).getByText(en('session-ended-retry'))
  expect(before(alert, signIns[0]!), 'Sign in after the alert').toBe(true)
  expect(
    before(signIns[0]!, serverTitle(main)),
    'before the Server section',
  ).toBe(true)
  // 36a's state: the page is not signed out.
  const banner = screen.getByRole('banner')
  expect(banner.textContent).toContain(en('session-signed-in-as', 'alice'))
  expect(
    within(banner).getByRole('button', { name: en('session-sign-out') }),
  ).toBeTruthy()
  const nav = screen.getByRole('navigation', { name: en('nav-label') })
  expect(within(nav).getByRole('link', { name: 'coder' })).toBeTruthy()
  expect(h1(main)).toBeTruthy()
  expect(portBox(main).value, 'the edit is untouched').toBe('8581')
  expect(saveStatus(main).textContent).toBe(en('save-unsaved'))
  expect(within(main).queryByText(en('signin-required'))).toBeNull()
  expect(posts(bff)).toHaveLength(1)
})

test('1c.43a, 41b: while the session has ended the idle poll goes on', async () => {
  const { bff, main } = await loaded({ run: SESSION_ENDED })
  const opening = reads(bff)
  press(button(main, 'run-stop'))
  await flush()
  expect(runAlerts(main)).toEqual([en('session-ended-retry')])
  await advance(5000)
  expect(reads(bff)).toBe(opening + 1)
  await advance(5000)
  expect(reads(bff)).toBe(opening + 2)
})

test('1c.43a: the run Sign in opens the sign-in dialog; Escape and Cancel close it, focus back, alert kept, nothing sent', async () => {
  for (const how of ['Escape', 'Cancel']) {
    const { bff, main } = await loaded({ run: SESSION_ENDED })
    press(button(main, 'run-stop'))
    await flush()
    const signIn = runSignIns(main)[0]!
    press(signIn)
    await flush()
    const d = screen.getByRole('dialog', { name: en('signin-title') })
    expect(d.tagName, `${how}: a native <dialog>`).toBe('DIALOG')
    expect(dialogs.opened, `${how}: opened with showModal()`).toContain(d)
    expect(
      within(d).getByRole('heading', { level: 2, name: en('signin-title') }),
    ).toBeTruthy()
    const user = within(d).getByRole('textbox', {
      name: en('signin-username'),
    })
    type(user, 'alice')
    if (how === 'Escape') {
      fireEvent.keyDown(user, { key: 'Escape', code: 'Escape' })
    } else {
      fireEvent.click(
        within(d).getByRole('button', { name: en('signin-cancel') }),
      )
    }
    await flush()
    expect(screen.queryByRole('dialog'), how).toBeNull()
    expect(document.activeElement, `${how}: focus on the run Sign in`).toBe(
      runSignIns(main)[0],
    )
    expect(runAlerts(main), how).toEqual([en('session-ended-retry')])
    expect(bff.to('POST', '/api/session'), how).toHaveLength(0)
    expect(posts(bff), `${how}: nothing resent`).toHaveLength(1)
    cleanup()
    vi.useRealTimers()
    vi.unstubAllGlobals()
  }
})

// ── A sign-in after the 401 (43b) ───────────────────────────────────────

test('1c.43b: the same administrator signs in again: alert and Sign in go, focus on the clicked button, edits intact, nothing resent', async () => {
  const { bff, main } = await loaded({ run: SESSION_ENDED })
  type(portBox(main), '8581')
  const restart = button(main, 'run-restart')
  press(restart)
  await flush()
  press(runSignIns(main)[0]!)
  await flush()
  await signInInDialog('alice')
  expect(runAlerts(main)).toEqual([])
  expect(runSignIns(main)).toEqual([])
  expect(document.activeElement).toBe(button(main, 'run-restart'))
  expect(portBox(main).value).toBe('8581')
  expect(saveStatus(main).textContent).toBe(en('save-unsaved'))
  expect(within(main).queryByText(en('server-edits-dropped'))).toBeNull()
  expect(bff.to('GET', '/api/instances/default/server')).toHaveLength(1)
  expect(posts(bff), 'nothing resent').toHaveLength(1)
})

// "Administrators only" (R3): 43b compares by `user` only, so every case here
// changes user, and the form it brings is the editable one (28).
test('1c.43b: a sign-in as another user after a run 401 re-reads the file; edits-dropped only when the form was dirty; focus on the h1', async () => {
  const cases: { what: string; user: string; dirty: boolean }[] = [
    { what: 'dirty form', user: 'carol', dirty: true },
    { what: 'clean form', user: 'carol', dirty: false },
  ]
  for (const c of cases) {
    const { bff, main } = await loaded({
      run: SESSION_ENDED,
      signIn: () => reply(200, { user: c.user }),
    })
    if (c.dirty) type(portBox(main), '8581')
    press(button(main, 'run-stop'))
    await flush()
    press(runSignIns(main)[0]!)
    await flush()
    await signInInDialog(c.user)
    expect(
      screen.getByRole('banner').textContent,
      `${c.what}: the banner shows the new user`,
    ).toContain(en('session-signed-in-as', c.user))
    expect(
      bff.to('GET', '/api/instances/default/server'),
      `${c.what}: the file is read again`,
    ).toHaveLength(2)
    expect(runAlerts(main), c.what).toEqual([])
    expect(runSignIns(main), c.what).toEqual([])
    expect(
      within(main).queryByText(en('server-edits-dropped')) !== null,
      `${c.what}: edits-dropped only when dirty`,
    ).toBe(c.dirty)
    expect(portBox(main).value, c.what).toBe('8580')
    expect(portBox(main).disabled, `${c.what}: the form is editable`).toBe(
      false,
    )
    expect(document.activeElement, `${c.what}: focus on the h1`).toBe(h1(main))
    expect(posts(bff), c.what).toHaveLength(1)
    cleanup()
    vi.useRealTimers()
    vi.unstubAllGlobals()
  }
})

test('1c.43b: a sign-in from the run alert also ends the save bar session-ended alert, and the other way round', async () => {
  for (const from of ['run', 'save']) {
    const { bff, main } = await loaded({
      run: SESSION_ENDED,
      save: () => reply(401, { key: 'session-required', args: [] }),
    })
    type(portBox(main), '8581')
    const save = button(main, 'save-button')
    press(save)
    await flush()
    expect(within(main).getByText(en('session-ended')), from).toBeTruthy()
    const stop = button(main, 'run-stop')
    press(stop)
    await flush()
    expect(runAlerts(main), from).toEqual([en('session-ended-retry')])
    const signIns = within(main).getAllByRole('button', {
      name: en('session-sign-in'),
    })
    expect(signIns, `${from}: two Sign in buttons`).toHaveLength(2)
    press(from === 'run' ? runSignIns(main)[0]! : signIns[1]!)
    await flush()
    await signInInDialog('alice')
    expect(
      within(main)
        .queryAllByRole('alert')
        .map((a) => a.textContent),
      `${from}: every session-ended alert goes`,
    ).toEqual([])
    expect(
      within(main).queryAllByRole('button', { name: en('session-sign-in') }),
      from,
    ).toEqual([])
    expect(document.activeElement, from).toBe(
      from === 'run' ? button(main, 'run-stop') : button(main, 'save-button'),
    )
    expect(portBox(main).value, from).toBe('8581')
    expect(posts(bff), from).toHaveLength(1)
    expect(bff.to('PUT', '/api/instances/default/server'), from).toHaveLength(1)
    cleanup()
    vi.useRealTimers()
    vi.unstubAllGlobals()
  }
})

// ── Inactive buttons (40a, 40b) ─────────────────────────────────────────

test('1c.40b: a click, Enter or Space on an inactive button sends nothing and changes no message; focus stays', async () => {
  const { bff, main } = await loaded({
    run: () => reply(403, { key: 'client-not-authorized', args: [] }),
  })
  // A first action leaves an alert, to see that nothing changes it.
  press(button(main, 'run-stop'))
  await flush()
  expect(runAlerts(main)).toEqual([en('api-client-not-authorized')])
  const start = button(main, 'run-start')
  expect(inactive(start), 'Start is inactive while active').toBe(true)
  const opening = reads(bff)
  start.focus()
  fireEvent.keyDown(start, { key: 'Enter', code: 'Enter' })
  fireEvent.keyDown(start, { key: ' ', code: 'Space' })
  fireEvent.keyUp(start, { key: ' ', code: 'Space' })
  fireEvent.click(start)
  await flush()
  expect(posts(bff), 'no request').toHaveLength(1)
  expect(reads(bff), 'no read either').toBe(opening)
  expect(runAlerts(main), 'the alert is unchanged').toEqual([
    en('api-client-not-authorized'),
  ])
  expect(runStatus(main).textContent).toBe('')
  expect(document.activeElement).toBe(start)
})

// The instance state and which of Start, Stop, Restart are inactive.
const INACTIVE_TABLE: [string, boolean[]][] = [
  ['active', [true, false, false]],
  ['inactive', [false, true, true]],
  ['failed', [false, true, true]],
  ['activating', [true, true, true]],
  ['deactivating', [true, true, true]],
  ['reloading', [false, false, false]],
]

test.each(INACTIVE_TABLE)(
  '1c.40a: which buttons are inactive follows the state; never disabled: %s',
  async (state, expected) => {
    const { main } = await loaded({ instances: list(state) })
    expect(inactiveSet(main), state).toEqual(expected)
    for (const b of runButtons(main)) {
      expect(b.disabled, `${state}: ${b.textContent} never disabled`).toBe(
        false,
      )
      if (!inactive(b))
        expect(b.getAttribute('aria-disabled'), state).not.toBe('true')
    }
  },
)

test('1c.40a, 40b: while an action runs all three are inactive, stay focusable, and send nothing', async () => {
  const gate = deferred()
  const { bff, main } = await loaded({ run: () => gate.promise })
  const stop = button(main, 'run-stop')
  press(stop)
  await flush()
  expect(inactiveSet(main)).toEqual([true, true, true])
  expect(document.activeElement, 'focus stays on Stop').toBe(stop)
  for (const b of runButtons(main)) {
    expect(b.disabled).toBe(false)
    press(b)
  }
  await flush()
  expect(posts(bff)).toEqual(['/api/instances/default/stop'])
  expect(runStatus(main).textContent).toBe(en('run-stopping'))
})

// ── One read of the instances (41, 41a) ─────────────────────────────────

test('1c.41: opening an instance page sends exactly one GET /api/instances before the first poll; the nav and header share it', async () => {
  const { bff, main } = await loaded()
  expect(reads(bff)).toBe(1)
  const nav = screen.getByRole('navigation', { name: en('nav-label') })
  expect(within(nav).getByRole('link', { name: 'coder' })).toBeTruthy()
  expectState(main, 'instance-state-active')
})

test('1c.41: opening the Overview signed in sends exactly one GET /api/instances', async () => {
  const { bff, main } = await open({}, '#/')
  expect(instanceTiles(main)).toBeTruthy()
  const nav = screen.getByRole('navigation', { name: en('nav-label') })
  expect(within(nav).getByRole('link', { name: 'coder' })).toBeTruthy()
  expect(reads(bff)).toBe(1)
  // The Overview is not polled (41b).
  await advance(10_000)
  expect(reads(bff)).toBe(1)
})

test('1c.41: a read asked while one runs gets its answer, with no second request', async () => {
  const gates: ReturnType<typeof deferred>[] = []
  const { bff, main } = await open({
    instances: () => {
      const g = deferred()
      gates.push(g)
      return g.promise
    },
  })
  expect(reads(bff), 'one request in flight').toBe(1)
  await act(async () => {
    for (const g of gates) g.resolve(reply(200, list('active')))
  })
  await flush()
  expect(reads(bff)).toBe(1)
  expectState(main, 'instance-state-active')
  const nav = screen.getByRole('navigation', { name: en('nav-label') })
  expect(within(nav).getByRole('link', { name: 'coder' })).toBeTruthy()
})

test('1c.41a: a failed poll, or a list without the id, changes nothing on the page and shows nothing', async () => {
  const { bff, main } = await loaded()
  bff.setInstances(() => reply(503, { key: 'client-unavailable', args: [] }))
  await advance(5000)
  expect(reads(bff)).toBe(2)
  expectState(main, 'instance-state-active')
  expect(inactiveSet(main)).toEqual([true, false, false])
  expect(runAlerts(main)).toEqual([])
  expect(runStatus(main).textContent).toBe('')
  expect(
    within(main).queryByText(en('instances-manager-unavailable')),
  ).toBeNull()
  const nav = screen.getByRole('navigation', { name: en('nav-label') })
  expect(
    within(nav).getByRole('link', { name: 'coder' }),
    'the nav keeps the last list',
  ).toBeTruthy()

  bff.setInstances([{ id: 'coder', engine: 'llama.cpp', state: 'inactive' }])
  await advance(5000)
  expect(reads(bff)).toBe(3)
  expectState(main, 'instance-state-active')
  expect(inactiveSet(main)).toEqual([true, false, false])
  expect(
    within(main).queryByText(en('instance-not-found', 'default')),
  ).toBeNull()
  expect(
    within(main).getByRole('heading', { level: 3, name: en('server-network') }),
  ).toBeTruthy()
  expect(
    within(nav).queryByRole('link', { name: 'default' }),
    'the nav takes the id-less list and drops the link',
  ).toBeNull()
  // The poll goes on.
  await advance(5000)
  expect(reads(bff)).toBe(4)
})

test('1c.41a: an idle poll updates the state line and the buttons; the nav takes every successful list', async () => {
  const { bff, main } = await loaded({ instances: list('inactive') })
  expect(inactiveSet(main)).toEqual([false, true, true])
  bff.setInstances(
    list('active', [{ id: 'big-model', engine: 'llama.cpp', state: 'active' }]),
  )
  await advance(5000)
  expectState(main, 'instance-state-active')
  expect(
    within(main).queryByText(stateText('instance-state-inactive')),
  ).toBeNull()
  expect(inactiveSet(main)).toEqual([true, false, false])
  const nav = screen.getByRole('navigation', { name: en('nav-label') })
  expect(within(nav).getByRole('link', { name: 'big-model' })).toBeTruthy()
  // Not announced (42b): the state line is not in a live region.
  const line = within(main).getByText(stateText('instance-state-active'))
  expect(
    line.closest('[role="status"], [role="alert"], [aria-live]'),
    'the state line is not a live region',
  ).toBeNull()
  expect(runStatus(main).textContent).toBe('')
})

test('1c.4, 41a: the nav lists the instances once a later read succeeds', async () => {
  const { bff } = await open(
    { instances: () => reply(503, { key: 'client-unavailable', args: [] }) },
    '#/',
  )
  const nav = screen.getByRole('navigation', { name: en('nav-label') })
  expect(within(nav).queryByRole('link', { name: 'coder' })).toBeNull()
  bff.setInstances(list('active'))
  navigate('#/instances/default')
  await flush()
  expect(within(nav).getByRole('link', { name: 'coder' })).toBeTruthy()
})

// ── The idle poll (41b, 41d) ────────────────────────────────────────────

test('1c.41b, 41d: the idle poll reads 5 s after the previous answer, as a chain', async () => {
  const { bff } = await loaded()
  expect(reads(bff)).toBe(1)
  await advance(4999)
  expect(reads(bff), 'nothing at 4999 ms').toBe(1)
  await advance(1)
  expect(reads(bff), 'one read at 5000 ms').toBe(2)
  await advance(4999)
  expect(reads(bff)).toBe(2)
  await advance(1)
  expect(reads(bff)).toBe(3)
})

test('1c.41b: a slow read holds the next one: 5 s after it answered, never overlapping', async () => {
  const { bff } = await loaded()
  const gate = deferred()
  bff.setInstances(() => gate.promise)
  await advance(5000)
  expect(reads(bff)).toBe(2)
  // Pending for 7 s: no second read meanwhile.
  await advance(7000)
  expect(reads(bff), 'no read while one is in flight').toBe(2)
  bff.setInstances(list('active'))
  await act(async () => gate.resolve(reply(200, list('active'))))
  await flush()
  await advance(4999)
  expect(reads(bff)).toBe(2)
  await advance(1)
  expect(reads(bff)).toBe(3)
})

test('1c.41b, 41d: the idle poll stops when the page is left', async () => {
  const { bff } = await loaded()
  navigate('#/nope')
  await flush()
  const left = reads(bff)
  await advance(10_000)
  expect(reads(bff)).toBe(left)
})

test('1c.41b: the idle poll stops at sign-out (the Overview reads once on opening, then nothing)', async () => {
  const { bff } = await loaded()
  fireEvent.click(
    within(screen.getByRole('banner')).getByRole('button', {
      name: en('session-sign-out'),
    }),
  )
  await flush()
  expect(bff.to('DELETE', '/api/session')).toHaveLength(1)
  expect(
    screen.getByRole('heading', { level: 1, name: en('overview-title') }),
  ).toBeTruthy()
  const after = reads(bff)
  expect(after, 'the Overview read on opening').toBe(2)
  await advance(10_000)
  expect(reads(bff)).toBe(after)
})

test('1c.41b: the idle poll stops when a server read signs the app out', async () => {
  const { bff, main } = await open({ server: () => SESSION_ENDED() })
  expect(within(main).getByText(en('signin-required'))).toBeTruthy()
  const now = reads(bff)
  await advance(10_000)
  expect(reads(bff)).toBe(now)
})

test('1c.41b: no poll when the page shows a message or instance-not-found in place of the header', async () => {
  const answers: [string, BffOptions['instances']][] = [
    ['failed read', () => reply(503, { key: 'client-unavailable', args: [] })],
    ['no such id', [{ id: 'coder', engine: 'llama.cpp', state: 'inactive' }]],
  ]
  for (const [what, instances] of answers) {
    const { bff } = await open({ instances })
    expect(
      screen.queryByRole('button', { name: en('run-stop') }),
      what,
    ).toBeNull()
    const now = reads(bff)
    await advance(10_000)
    expect(reads(bff), what).toBe(now)
    cleanup()
    vi.useRealTimers()
    vi.unstubAllGlobals()
  }
})

// ── An action's poll (41c, 41d, 42, 42a) ────────────────────────────────

test('1c.41d, 42, 42a: Stop polls 1 s after the 204, then every second until inactive, then idles at 5 s', async () => {
  const gate = deferred()
  const { bff, main } = await loaded({ run: () => gate.promise })
  const opening = reads(bff)
  const stop = button(main, 'run-stop')
  press(stop)
  await flush()
  expect(posts(bff)).toEqual(['/api/instances/default/stop'])
  expect(runStatus(main).textContent).toBe(en('run-stopping'))
  // While the POST runs, nothing is read, the idle timer included.
  await advance(10_000)
  expect(reads(bff), 'no read while the POST is pending').toBe(opening)

  bff.setInstances(list('deactivating'))
  await act(async () => gate.resolve(reply(204)))
  await flush()
  await advance(999)
  expect(reads(bff), 'nothing at 999 ms').toBe(opening)
  await advance(1)
  expect(reads(bff), 'one read at 1000 ms').toBe(opening + 1)
  expectState(main, 'instance-state-deactivating')
  expect(runStatus(main).textContent).toBe(en('run-stopping'))
  expect(inactiveSet(main)).toEqual([true, true, true])

  bff.setInstances(list('inactive'))
  await advance(999)
  expect(reads(bff)).toBe(opening + 1)
  await advance(1)
  expect(reads(bff)).toBe(opening + 2)
  expectState(main, 'instance-state-inactive')
  expect(runStatus(main).textContent).toBe(en('run-stopped'))
  expect(inactiveSet(main)).toEqual([false, true, true])
  expect(document.activeElement, 'focus stayed on Stop').toBe(stop)

  // Settled: back to the idle chain.
  await advance(1000)
  expect(reads(bff), 'not 1 s any more').toBe(opening + 2)
  await advance(3999)
  expect(reads(bff)).toBe(opening + 2)
  await advance(1)
  expect(reads(bff), '5 s after the settling read').toBe(opening + 3)
})

test('1c.42: the POST carries no body and goes to the clicked action of this instance', async () => {
  for (const [state, key, action] of [
    ['inactive', 'run-start', 'start'],
    ['active', 'run-stop', 'stop'],
    ['active', 'run-restart', 'restart'],
  ] as const) {
    const { bff, main } = await loaded({ instances: list(state) })
    press(button(main, key))
    await flush()
    expect(posts(bff), key).toEqual([`/api/instances/default/${action}`])
    const init = bff.fetch.mock.calls.find(
      ([, i]) => i?.method?.toUpperCase() === 'POST',
    )?.[1]
    expect(init?.body ?? null, `${key}: no body`).toBeNull()
    cleanup()
    vi.useRealTimers()
    vi.unstubAllGlobals()
  }
})

test('1c.42, 42a: Start keeps polling through the old state, settles on active with run-started; any 2xx is accepted', async () => {
  const { bff, main } = await loaded({
    instances: list('inactive'),
    run: () => reply(202, { accepted: 'yes' }),
  })
  const start = button(main, 'run-start')
  press(start)
  await flush()
  expect(runStatus(main).textContent).toBe(en('run-starting'))
  const n = reads(bff)
  // The job may not have run yet: the old state keeps the poll going.
  await advance(1000)
  expect(reads(bff)).toBe(n + 1)
  expect(runStatus(main).textContent).toBe(en('run-starting'))
  bff.setInstances(list('activating'))
  await advance(1000)
  expect(reads(bff)).toBe(n + 2)
  expectState(main, 'instance-state-activating')
  expect(runStatus(main).textContent).toBe(en('run-starting'))
  bff.setInstances(list('active'))
  await advance(1000)
  expect(reads(bff)).toBe(n + 3)
  expect(runStatus(main).textContent).toBe(en('run-started'))
  expect(inactiveSet(main)).toEqual([true, false, false])
  expect(document.activeElement).toBe(start)
  expect(runAlerts(main)).toEqual([])
})

test('1c.42a: Restart settles on active with run-restarted', async () => {
  const { bff, main } = await loaded()
  press(button(main, 'run-restart'))
  await flush()
  expect(runStatus(main).textContent).toBe(en('run-restarting'))
  bff.setInstances(list('activating'))
  await advance(1000)
  expect(runStatus(main).textContent).toBe(en('run-restarting'))
  bff.setInstances(list('active'))
  await advance(1000)
  expect(runStatus(main).textContent).toBe(en('run-restarted'))
})

test('1c.42c: a Restart whose first read still sees active reads run-restarted (the known limit)', async () => {
  const { main } = await loaded()
  press(button(main, 'run-restart'))
  await flush()
  await advance(1000)
  expect(runStatus(main).textContent).toBe(en('run-restarted'))
})

test('1c.42a: failed settles any action with run-ended-failed', async () => {
  for (const [state, key] of [
    ['inactive', 'run-start'],
    ['active', 'run-stop'],
    ['active', 'run-restart'],
  ] as const) {
    const { bff, main } = await loaded({ instances: list(state) })
    press(button(main, key))
    await flush()
    bff.setInstances(list('failed'))
    await advance(1000)
    expect(runStatus(main).textContent, key).toBe(en('run-ended-failed'))
    expect(inactiveSet(main), key).toEqual([false, true, true])
    expect(document.activeElement, key).toBe(button(main, key))
    cleanup()
    vi.useRealTimers()
    vi.unstubAllGlobals()
  }
})

test('1c.41d, 42a: no settled state within 60 s: 59 reads, run-timeout at 60 000 ms, the next read 5 s later', async () => {
  const { bff, main } = await loaded({ instances: list('inactive') })
  press(button(main, 'run-start'))
  await flush()
  bff.setInstances(list('activating'))
  const n = reads(bff)
  for (let s = 1; s <= 59; s++) {
    await advance(1000)
    expect(reads(bff), `read ${s} at ${s} s`).toBe(n + s)
    expect(runStatus(main).textContent, `${s} s`).toBe(en('run-starting'))
  }
  await advance(1000)
  expect(reads(bff), 'no read starts at 60 s').toBe(n + 59)
  expect(runStatus(main).textContent).toBe(en('run-timeout'))
  expect(runAlerts(main), 'a timeout is not an alert').toEqual([])
  // The last state is activating: all three stay inactive (40a).
  expect(inactiveSet(main)).toEqual([true, true, true])
  await advance(4999)
  expect(reads(bff)).toBe(n + 59)
  await advance(1)
  expect(reads(bff), 'the idle chain, 5 s after the 60 s mark').toBe(n + 60)
  // A later read still updates the state line (run-timeout says so).
  bff.setInstances(list('active'))
  await advance(5000)
  expectState(main, 'instance-state-active')
})

test('1c.42a, 40a: after run-timeout the buttons follow the last state read again and a click sends', async () => {
  const { bff, main } = await loaded({ instances: list('inactive') })
  press(button(main, 'run-start'))
  await flush()
  bff.setInstances(list('activating'))
  for (let s = 1; s <= 60; s++) await advance(1000)
  expect(runStatus(main).textContent).toBe(en('run-timeout'))
  // The idle read 5 s after the 60 s mark says active.
  bff.setInstances(list('active'))
  await advance(5000)
  expectState(main, 'instance-state-active')
  expect(inactiveSet(main), 'Start inactive; Stop and Restart active').toEqual([
    true,
    false,
    false,
  ])
  press(button(main, 'run-stop'))
  await flush()
  expect(posts(bff)).toEqual([
    '/api/instances/default/start',
    '/api/instances/default/stop',
  ])
  expect(runStatus(main).textContent).toBe(en('run-stopping'))
})

test('1c.41c, 41d: an idle read held across a click is ignored by the page', async () => {
  const { bff, main } = await loaded()
  const idle = deferred()
  bff.setInstances(() => idle.promise)
  await advance(5000)
  expect(reads(bff), 'the idle read is in flight').toBe(2)
  bff.setInstances(list('active'))
  press(button(main, 'run-stop'))
  await flush()
  await act(async () => idle.resolve(reply(200, list('inactive'))))
  await flush()
  expectState(main, 'instance-state-active')
  expect(
    within(main).queryByText(stateText('instance-state-inactive')),
  ).toBeNull()
  expect(runStatus(main).textContent, 'not settled by a stale read').toBe(
    en('run-stopping'),
  )
  // The action's own reads decide.
  bff.setInstances(list('inactive'))
  await advance(1000)
  expect(runStatus(main).textContent).toBe(en('run-stopped'))
})

test('1c.41, 41c: an idle read still in flight at the action read waits; only a read sent after the 2xx settles', async () => {
  const { bff, main } = await loaded()
  const idle = deferred()
  bff.setInstances(() => idle.promise)
  await advance(5000)
  expect(reads(bff), 'the idle read is in flight').toBe(2)
  // The reads sent from now on: deactivating first, then inactive.
  let fresh = 0
  bff.setInstances(() =>
    reply(200, list(++fresh === 1 ? 'deactivating' : 'inactive')),
  )
  press(button(main, 'run-stop'))
  await flush()
  expect(posts(bff)).toEqual(['/api/instances/default/stop'])
  // The 204 has answered; the action's first read is due at 1000 ms, while
  // the idle read is still pending: at most one request in flight.
  await advance(1000)
  expect(reads(bff), 'no second request while one is in flight').toBe(2)
  expect(runStatus(main).textContent).toBe(en('run-stopping'))

  // The pre-click read answers inactive: it settles nothing and leaves the
  // header alone; the action sends its own read at once.
  await act(async () => idle.resolve(reply(200, list('inactive'))))
  await flush()
  expect(runStatus(main).textContent, 'not settled by the held read').toBe(
    en('run-stopping'),
  )
  expect(
    within(main).queryByText(stateText('instance-state-inactive')),
    'the held answer does not reach the header',
  ).toBeNull()
  expect(reads(bff), 'the action read went out after the held one').toBe(3)
  expectState(main, 'instance-state-deactivating')
  expect(runStatus(main).textContent).toBe(en('run-stopping'))
  expect(inactiveSet(main)).toEqual([true, true, true])

  // Only a read sent after the 2xx settles the action.
  await advance(999)
  expect(reads(bff)).toBe(3)
  await advance(1)
  expect(reads(bff)).toBe(4)
  expectState(main, 'instance-state-inactive')
  expect(runStatus(main).textContent).toBe(en('run-stopped'))
})

test('1c.41d: a held read across a click on Start from failed: only the read sent after the 2xx settles', async () => {
  const { bff, main } = await loaded({ instances: list('failed') })
  expect(inactiveSet(main), 'Start is active from failed').toEqual([
    false,
    true,
    true,
  ])
  const idle = deferred()
  bff.setInstances(() => idle.promise)
  await advance(5000)
  expect(reads(bff), 'the idle read is in flight').toBe(2)
  const action = deferred()
  bff.setInstances(() => action.promise)
  press(button(main, 'run-start'))
  await flush()
  expect(posts(bff)).toEqual(['/api/instances/default/start'])
  await advance(1000)
  expect(reads(bff), 'no second GET yet').toBe(2)

  await act(async () => idle.resolve(reply(200, list('failed'))))
  await flush()
  expect(runStatus(main).textContent, 'not run-ended-failed').toBe(
    en('run-starting'),
  )
  expect(reads(bff), 'a new GET goes out at once').toBe(3)
  await act(async () => action.resolve(reply(200, list('active'))))
  await flush()
  expect(runStatus(main).textContent).toBe(en('run-started'))
})

// The 60 s deadline holds whatever is pending (decided by the coordinator
// after the second rerun of 1c, part 1): at 60 000 ms after the 2xx the status
// reads run-timeout, no action read starts at or after it, and a later answer
// settles nothing.

test('1c.41c, 42a: an action read that never answers: run-timeout at 60 000 ms, then a click sends', async () => {
  const { bff, main } = await loaded({ instances: list('inactive') })
  press(button(main, 'run-start'))
  await flush()
  const n = reads(bff)
  const held = deferred()
  bff.setInstances(() => held.promise)
  await advance(1000)
  expect(reads(bff), 'the action read is sent and stays pending').toBe(n + 1)
  await advance(58_999)
  expect(runStatus(main).textContent).toBe(en('run-starting'))
  expect(inactiveSet(main)).toEqual([true, true, true])
  await advance(1)
  expect(runStatus(main).textContent, 'at 60 000 ms').toBe(en('run-timeout'))
  expect(runAlerts(main)).toEqual([])
  expect(inactiveSet(main), 'the buttons follow inactive (40a)').toEqual([
    false,
    true,
    true,
  ])
  // Not stuck busy: a click on Start sends.
  const start = button(main, 'run-start')
  press(start)
  await flush()
  expect(posts(bff)).toEqual([
    '/api/instances/default/start',
    '/api/instances/default/start',
  ])
  expect(runStatus(main).textContent).toBe(en('run-starting'))
  expect(document.activeElement).toBe(start)
})

test('1c.41d (a1), 47c: failed, an idle read held across Start: no GET to 14 999 ms; at 15 000 ms it is aborted and the action GET goes at once; answered active: run-started, the next GET 5 000 ms later', async () => {
  const { bff, main } = await loaded({ instances: list('failed') })
  const idle = held()
  bff.setInstances(idle.answer)
  await advance(5000)
  expect(reads(bff), 'the idle read is in flight').toBe(2)
  // The click in the same tick; the 204 answers at once: times are from now.
  const action = held()
  bff.setInstances(action.answer)
  press(button(main, 'run-start'))
  await flush()
  expect(posts(bff)).toEqual(['/api/instances/default/start'])
  await advance(14_999)
  expect(reads(bff), 'no second GET up to 14 999 ms').toBe(2)
  expect(idle.signals[0]?.aborted, 'the idle read at 14 999 ms').toBe(false)
  expect(runStatus(main).textContent).toBe(en('run-starting'))
  await advance(1)
  expect(idle.signals[0]?.aborted, 'the idle read aborted at 15 000 ms').toBe(
    true,
  )
  expect(runStatus(main).textContent, 'not settled by the abort').toBe(
    en('run-starting'),
  )
  expect(reads(bff), 'the action GET is sent at once').toBe(3)
  expect(action.signals, 'it is the action read').toHaveLength(1)

  bff.setInstances(list('failed'))
  await act(async () => action.resolve(reply(200, list('active'))))
  await flush()
  expect(runStatus(main).textContent).toBe(en('run-started'))
  expectState(main, 'instance-state-active')
  await advance(4999)
  expect(reads(bff), 'none at 19 999 ms').toBe(3)
  await advance(1)
  expect(reads(bff), 'the next GET at 20 000 ms').toBe(4)
})

test('1c.41d (a2), 47c: failed, the idle read and every action read held: aborts at 15 000, 30 000, 46 000; GETs at 15 000, 31 000, 47 000; run-timeout at 60 000; the answer at 60 500 changes nothing; the next GET at 65 000', async () => {
  const { bff, main } = await loaded({ instances: list('failed') })
  const h = held()
  bff.setInstances(h.answer)
  await advance(5000)
  expect(reads(bff), 'the idle read is in flight').toBe(2)
  // The click in the same tick; the 204 answers at once: times are from now.
  press(button(main, 'run-start'))
  await flush()
  expect(posts(bff)).toEqual(['/api/instances/default/start'])
  await advance(14_999)
  expect(reads(bff), 'no second GET up to 14 999 ms').toBe(2)
  expect(h.signals[0]?.aborted, 'the idle read at 14 999 ms').toBe(false)
  await advance(1)
  expect(h.signals[0]?.aborted, 'the idle read aborted at 15 000 ms').toBe(true)
  expect(runStatus(main).textContent).toBe(en('run-starting'))
  expect(reads(bff), 'the first action GET at 15 000 ms').toBe(3)

  for (const [i, abortAt, nextAt] of [
    [1, 30_000, 31_000],
    [2, 46_000, 47_000],
  ] as const) {
    await advance(14_999)
    expect(h.signals[i]?.aborted, `read ${i} before ${abortAt} ms`).toBe(false)
    await advance(1)
    expect(h.signals[i]?.aborted, `read ${i} aborted at ${abortAt} ms`).toBe(
      true,
    )
    expect(runStatus(main).textContent).toBe(en('run-starting'))
    await advance(999)
    expect(reads(bff), `none before ${nextAt} ms`).toBe(2 + i)
    await advance(1)
    expect(reads(bff), `the next action GET at ${nextAt} ms`).toBe(3 + i)
  }

  await advance(12_999)
  expect(runStatus(main).textContent, 'at 59 999 ms').toBe(en('run-starting'))
  await advance(1)
  expect(runStatus(main).textContent, 'at 60 000 ms').toBe(en('run-timeout'))
  expect(inactiveSet(main), '40a for failed: Start active').toEqual([
    false,
    true,
    true,
  ])
  expect(reads(bff), 'no GET at the deadline').toBe(5)

  await advance(500)
  expect(h.signals[3]?.aborted, 'the 47 000 ms read, at 60 500 ms').toBe(false)
  bff.setInstances(list('failed'))
  await act(async () => h.resolve(reply(200, list('active'))))
  await flush()
  expect(runStatus(main).textContent, 'at 60 500 ms').toBe(en('run-timeout'))
  expectState(main, 'instance-state-failed')
  expect(
    within(main).queryByText(stateText('instance-state-active')),
    'the late answer does not reach the header',
  ).toBeNull()
  expect(reads(bff), 'no GET at the late answer').toBe(5)
  await advance(4499)
  expect(reads(bff), 'none at 64 999 ms').toBe(5)
  await advance(1)
  expect(reads(bff), 'the next GET at 65 000 ms').toBe(6)
  expect(runStatus(main).textContent).toBe(en('run-timeout'))
})

test('1c.41d (b), 47e: every action read held: aborted at 15 s, GETs at 1 000, 17 000, 33 000, 49 000; run-timeout at 60 000; the last read aborted at 64 000; a new idle GET at 65 000', async () => {
  const { bff, main } = await loaded({ instances: list('inactive') })
  press(button(main, 'run-start'))
  await flush()
  const n = reads(bff)
  const h = held()
  bff.setInstances(h.answer)
  // The 204 answered at once: times are from now.
  await advance(1000)
  expect(reads(bff), 'the first action read at 1 000 ms').toBe(n + 1)
  for (const [i, at] of [17_000, 33_000, 49_000].entries()) {
    // The read sent 16 000 ms before `at` is aborted 15 000 ms after it was sent.
    await advance(14_999)
    expect(h.signals[i]?.aborted, `read ${i + 1}, 14 999 ms after`).toBe(false)
    await advance(1)
    expect(h.signals[i]?.aborted, `read ${i + 1} aborted at 15 s`).toBe(true)
    await advance(999)
    expect(reads(bff), `none before ${at} ms`).toBe(n + 1 + i)
    await advance(1)
    expect(reads(bff), `the next action read at ${at} ms`).toBe(n + 2 + i)
  }
  await advance(10_999)
  expect(runStatus(main).textContent, 'at 59 999 ms').toBe(en('run-starting'))
  await advance(1)
  expect(runStatus(main).textContent, 'at 60 000 ms').toBe(en('run-timeout'))
  expect(inactiveSet(main), 'the buttons follow inactive (40a)').toEqual([
    false,
    true,
    true,
  ])
  expect(reads(bff), 'no action read at the deadline').toBe(n + 4)
  await advance(3999)
  expect(h.signals[3]?.aborted, 'the 49 000 ms read at 63 999 ms').toBe(false)
  await advance(1)
  expect(h.signals[3]?.aborted, 'aborted at 64 000 ms').toBe(true)
  expect(reads(bff), 'nothing sent at the abort').toBe(n + 4)
  await advance(999)
  expect(reads(bff), 'none at 64 999 ms').toBe(n + 4)
  await advance(1)
  expect(reads(bff), 'at 65 000 ms one new idle GET, not a shared one').toBe(
    n + 5,
  )
  expect(runStatus(main).textContent).toBe(en('run-timeout'))
})

test('1c.42a: an action answer that lands in the same tick as the deadline changes the status no more', async () => {
  const { bff, main } = await loaded({ instances: list('inactive') })
  press(button(main, 'run-start'))
  await flush()
  const n = reads(bff)
  const h = held()
  bff.setInstances(h.answer)
  // Action reads at 1 000, 17 000, 33 000 and 49 000 ms (each aborted at
  // 15 s, 47c): the one sent at 49 000 ms is in flight at the deadline.
  await advance(1000)
  for (let i = 0; i < 3; i++) {
    await advance(15_000)
    await advance(1000)
  }
  expect(reads(bff), 'the read sent at 49 000 ms').toBe(n + 4)
  expect(h.signals[3]?.aborted).toBe(false)
  // The deadline fires and that read answers active before React renders
  // again: the poll must already count as ended.
  await act(async () => {
    vi.advanceTimersByTime(11_000)
    h.resolve(reply(200, list('active')))
    for (let i = 0; i < 20; i++) await Promise.resolve()
  })
  await flush()
  expect(runStatus(main).textContent).toBe(en('run-timeout'))
})

test('1c.42a: an action settled before 60 s leaves no deadline behind', async () => {
  const { bff, main } = await loaded({ instances: list('inactive') })
  bff.setInstances(list('activating'))
  press(button(main, 'run-start'))
  await flush()
  await advance(1000)
  bff.setInstances(list('active'))
  await advance(1000)
  expect(runStatus(main).textContent).toBe(en('run-started'))
  const n = reads(bff)
  // 12 idle reads take the clock to 62 000 ms after the 2xx.
  for (let i = 0; i < 12; i++) await advance(5000)
  expect(runStatus(main).textContent, 'past the old deadline').toBe(
    en('run-started'),
  )
  expect(reads(bff), 'the idle chain ran').toBe(n + 12)
})

test('1c.42a: a second action after a settled one is not ended by the first one deadline', async () => {
  const { bff, main } = await loaded({ instances: list('inactive') })
  bff.setInstances(list('active'))
  press(button(main, 'run-start'))
  await flush()
  await advance(1000)
  expect(runStatus(main).textContent).toBe(en('run-started'))
  await advance(30_000)
  bff.setInstances(list('deactivating'))
  press(button(main, 'run-stop'))
  await flush()
  for (let s = 1; s <= 30; s++) await advance(1000)
  expect(
    runStatus(main).textContent,
    '61 000 ms after the first 2xx, 30 000 ms after the second',
  ).toBe(en('run-stopping'))
  expect(inactiveSet(main)).toEqual([true, true, true])
})

test('1c.41b, 41d: leaving the page while the action read waits behind a held read sends nothing more', async () => {
  const { bff, main } = await loaded()
  const idle = deferred()
  bff.setInstances(() => idle.promise)
  await advance(5000)
  expect(reads(bff), 'the idle read is in flight').toBe(2)
  bff.setInstances(list('inactive'))
  press(button(main, 'run-stop'))
  await flush()
  expect(posts(bff)).toEqual(['/api/instances/default/stop'])
  await advance(1000)
  expect(reads(bff), 'the action read waits').toBe(2)

  navigate('#/nope')
  await flush()
  const left = reads(bff)
  expect(
    left,
    'only the held idle read was ever sent after the opening one',
  ).toBe(2)
  // Counted once the held read answered: until then its own time limit (47a)
  // is a timer of call(), not of the page.
  await act(async () => idle.resolve(reply(200, list('active'))))
  await flush()
  expect(vi.getTimerCount(), 'no deadline or poll timer left').toBe(0)
  await advance(10_000)
  expect(reads(bff), 'no GET after leaving').toBe(left)
})

test('1c.41c: a failed read in the action poll keeps the last state and the poll goes on', async () => {
  const { bff, main } = await loaded()
  press(button(main, 'run-stop'))
  await flush()
  const n = reads(bff)
  bff.setInstances(() => reply(503, { key: 'client-unavailable', args: [] }))
  await advance(1000)
  expect(reads(bff)).toBe(n + 1)
  expectState(main, 'instance-state-active')
  expect(runStatus(main).textContent).toBe(en('run-stopping'))
  expect(runAlerts(main)).toEqual([])
  bff.setInstances(list('inactive'))
  await advance(1000)
  expect(reads(bff)).toBe(n + 2)
  expect(runStatus(main).textContent).toBe(en('run-stopped'))
})

test('1c.42b: the outcome stays until the next click; the click clears the alert and its Sign in at once', async () => {
  const { bff, main } = await loaded({
    run: (_a, _id, n) => (n === 1 ? SESSION_ENDED() : reply(204)),
  })
  press(button(main, 'run-stop'))
  await flush()
  expect(runAlerts(main)).toEqual([en('session-ended-retry')])
  expect(runSignIns(main)).toHaveLength(1)
  // A click on an active button: the alert and Sign in go at the click.
  press(button(main, 'run-restart'))
  await flush()
  expect(runAlerts(main)).toEqual([])
  expect(runSignIns(main)).toEqual([])
  expect(runStatus(main).textContent).toBe(en('run-restarting'))
  bff.setInstances(list('activating'))
  await advance(1000)
  bff.setInstances(list('active'))
  await advance(1000)
  expect(runStatus(main).textContent).toBe(en('run-restarted'))
  // Idle polls do not clear it.
  await advance(20_000)
  expect(runStatus(main).textContent).toBe(en('run-restarted'))
  // The next click replaces it.
  press(button(main, 'run-stop'))
  await flush()
  expect(runStatus(main).textContent).toBe(en('run-stopping'))
})

test('1c.42b: leaving the page ends the outcome', async () => {
  const { bff, main } = await loaded()
  press(button(main, 'run-stop'))
  await flush()
  bff.setInstances(list('inactive'))
  await advance(1000)
  expect(runStatus(main).textContent).toBe(en('run-stopped'))
  navigate('#/instances/coder')
  await flush()
  navigate('#/instances/default')
  await flush()
  expect(runStatus(main).textContent).toBe('')
})

// ── The header's controls (40) ──────────────────────────────────────────

test('1c.40: Start, Stop, Restart come after the state line, then the empty run status, before the Server section', async () => {
  const { main } = await loaded()
  const state = within(main).getByText(stateText('instance-state-active'))
  const [start, stop, restart] = runButtons(main)
  expect(before(state, start!), 'Start after the state line').toBe(true)
  expect(before(start!, stop!)).toBe(true)
  expect(before(stop!, restart!)).toBe(true)
  const status = runStatus(main)
  expect(status.tagName).toBe('P')
  expect(status.textContent, 'empty until an action').toBe('')
  expect(before(restart!, status), 'status after Restart').toBe(true)
  expect(before(status, serverTitle(main))).toBe(true)
  expect(runAlerts(main), 'no alert until there is text').toEqual([])
})

test('1c.40: the run alert is a paragraph after the run status', async () => {
  const { main } = await loaded({
    run: () => reply(503, { key: 'client-unavailable', args: [] }),
  })
  press(button(main, 'run-stop'))
  await flush()
  const alert = within(main).getByText(en('api-client-unavailable'))
  const el = alert.closest('[role="alert"]') as HTMLElement
  expect(el).not.toBeNull()
  expect(el.tagName).toBe('P')
  expect(before(runStatus(main), el)).toBe(true)
  expect(before(el, serverTitle(main))).toBe(true)
})

// ── With the Server form (44, 44a) ──────────────────────────────────────

test('1c.44: a run action leaves the draft, its errors, the save alert and the dirty state alone, and asks nothing', async () => {
  const { bff, main } = await loaded()
  type(portBox(main), 'abc')
  press(button(main, 'save-button'))
  await flush()
  expect(within(main).getByText(en('save-invalid'))).toBeTruthy()
  expect(portBox(main).getAttribute('aria-invalid')).toBe('true')
  press(button(main, 'run-stop'))
  await flush()
  expect(screen.queryByRole('dialog'), 'no discard dialog').toBeNull()
  bff.setInstances(list('inactive'))
  await advance(1000)
  expect(runStatus(main).textContent).toBe(en('run-stopped'))
  expect(portBox(main).value).toBe('abc')
  expect(portBox(main).getAttribute('aria-invalid')).toBe('true')
  expect(within(main).getByText(en('save-invalid'))).toBeTruthy()
  expect(saveStatus(main).textContent).toBe(en('save-unsaved'))
  expect(bff.to('PUT', '/api/instances/default/server')).toHaveLength(0)
  expect(posts(bff)).toEqual(['/api/instances/default/stop'])
})

/** Saves a changed Port with `running: true`: the bar says restart to apply. */
async function savedRestart(main: HTMLElement) {
  type(portBox(main), '8581')
  press(button(main, 'save-button'))
  await flush()
  expect(saveStatus(main).textContent).toBe(en('save-saved-restart'))
}

const RUNNING_SAVE = () => reply(200, { revision: 'r-2', running: true })

test('1c.44a: an action started after a save-saved-restart ends it when it settles; an undo does not bring it back', async () => {
  for (const [end, key] of [
    ['active', 'run-restart'],
    ['failed', 'run-restart'],
    ['inactive', 'run-stop'],
  ] as const) {
    const { bff, main } = await loaded({ save: RUNNING_SAVE })
    await savedRestart(main)
    press(button(main, key))
    await flush()
    expect(saveStatus(main).textContent, `${end}: kept while it runs`).toBe(
      en('save-saved-restart'),
    )
    bff.setInstances(list('activating'))
    await advance(1000)
    bff.setInstances(list(end))
    await advance(1000)
    expect(saveStatus(main).textContent, `${end}: ended, empty`).toBe('')
    type(portBox(main), '8582')
    expect(saveStatus(main).textContent, end).toBe(en('save-unsaved'))
    type(portBox(main), '8581')
    expect(saveStatus(main).textContent, `${end}: not back after an undo`).toBe(
      '',
    )
    cleanup()
    vi.useRealTimers()
    vi.unstubAllGlobals()
  }
})

test('1c.44a: on a timeout or an error, save-saved-restart stays', async () => {
  // A timeout. A Restart that saves snapshots runs to its 360 s deadline
  // (slice S2), not to the 60 s of a Start.
  {
    const { bff, main } = await loaded({ save: RUNNING_SAVE })
    await savedRestart(main)
    press(button(main, 'run-restart'))
    await flush()
    bff.setInstances(list('activating'))
    for (let s = 0; s < 60; s++) await advance(1000)
    expect(runStatus(main).textContent, 'no timeout at 60 000 ms').toBe(
      en('run-restarting'),
    )
    for (let s = 0; s < 300; s++) await advance(1000)
    expect(runStatus(main).textContent).toBe(en('run-timeout-stop'))
    expect(saveStatus(main).textContent, 'timeout').toBe(
      en('save-saved-restart'),
    )
    cleanup()
    vi.useRealTimers()
    vi.unstubAllGlobals()
  }
  // An error.
  {
    const { main } = await loaded({
      save: RUNNING_SAVE,
      run: () => reply(503, { key: 'client-unavailable', args: [] }),
    })
    await savedRestart(main)
    press(button(main, 'run-restart'))
    await flush()
    expect(runAlerts(main)).toEqual([en('api-client-unavailable')])
    expect(saveStatus(main).textContent, 'error').toBe(en('save-saved-restart'))
  }
})

// ── Keyboard (46a, 6) ───────────────────────────────────────────────────

/** The names of what Tab reaches in main, in DOM order. */
function tabNames(main: HTMLElement): string[] {
  return [
    ...main.querySelectorAll<HTMLElement>(
      'a[href], button, input, select, textarea, [tabindex]',
    ),
  ]
    .filter(
      (el) =>
        !(el as HTMLButtonElement).disabled &&
        el.getAttribute('tabindex') !== '-1' &&
        !isHidden(el),
    )
    .map((el) => (el.textContent ?? '').replace(/\s+/g, ' ').trim())
}

test('1c.46a: Tab reaches Start, Stop, Restart (inactive ones included), then the run Sign in, before the Server section', async () => {
  const { main } = await loaded({ run: SESSION_ENDED })
  // Start is inactive (state active) and still in the sequence.
  let names = tabNames(main)
  const first = names.indexOf(en('run-start'))
  expect(first, 'Start is reached').toBe(0)
  expect(names.slice(0, 3)).toEqual([
    en('run-start'),
    en('run-stop'),
    en('run-restart'),
  ])
  // The Server section comes after (Start at boot will sit between).
  expect(names.indexOf(`${en('field-about')} Port`)).toBeGreaterThan(2)

  press(button(main, 'run-stop'))
  await flush()
  names = tabNames(main)
  expect(
    names.slice(0, 4),
    'during a 401 every button stays reachable',
  ).toEqual([
    en('run-start'),
    en('run-stop'),
    en('run-restart'),
    en('session-sign-in'),
  ])
  expect(names.indexOf(`${en('field-about')} Port`)).toBeGreaterThan(3)
})

// ── Slice S2: a Stop or Restart that saves snapshots for minutes (4d) ────
//
// A Stop or Restart with "Save on shutdown" on lasts up to 300 s in
// deactivating, so its deadline is 360 s after the POST's 2xx (S2.1) and the
// status must not read a failure at 60 s. Times below are counted from the 2xx.

/**
 * Advances the clock 1 s at a time for `seconds`, sampling the run status at
 * each step (the action poll reads every 1000 ms, 41c): sample n is what the
 * status shows at n seconds after the 2xx. Every step is sampled, so a status
 * that changed text for one second is caught (S2.5).
 *
 * The status node is taken once, before the loop, and read through
 * `textContent`: `runStatus()` walks the tree for the ONE role="status"
 * region, which costs more than the 360 polls it would watch. After the loop
 * the same query pins that the node is still the single run status, so the
 * samples are all of that one live region.
 */
async function statusEachSecond(
  main: HTMLElement,
  seconds: number,
): Promise<string[]> {
  const status = runStatus(main)
  const seen: string[] = []
  for (let s = 1; s <= seconds; s++) {
    await advance(1000)
    seen.push(status.textContent ?? '')
  }
  expect(runStatus(main), 'the run status stayed one node').toBe(status)
  return seen
}

/** The samples that are a timeout text: none belong before the deadline. */
function timeoutSamples(seen: string[]): string[] {
  return seen.filter(
    (s) => s === en('run-timeout') || s === en('run-timeout-stop'),
  )
}

/** The distinct texts of `seen`, in the order they appear. */
function distinct(seen: string[]): string[] {
  return [...new Set(seen)]
}

test('S2.1: the deadline follows the action: a Stop and a Restart read on past 60 s, a Start ends at 60 s', async () => {
  for (const [state, key, action, readAt60, textAt60] of [
    ['inactive', 'run-start', 'start', 59, en('run-timeout')],
    ['active', 'run-stop', 'stop', 60, en('run-stopping')],
    ['active', 'run-restart', 'restart', 60, en('run-restarting')],
  ] as const) {
    const { bff, main } = await loaded({ instances: list(state) })
    press(button(main, key))
    await flush()
    expect(posts(bff), key).toEqual([`/api/instances/default/${action}`])
    bff.setInstances(list(state === 'active' ? 'deactivating' : 'activating'))
    const n = reads(bff)
    const seen = await statusEachSecond(main, 60)
    expect(
      timeoutSamples(seen.slice(0, 59)),
      `${key}: no timeout before 60 s`,
    ).toEqual([])
    expect(seen[59], `${key} at 60 000 ms`).toBe(textAt60)
    expect(reads(bff), `${key}: ${readAt60} reads at 60 000 ms`).toBe(
      n + readAt60,
    )
    cleanup()
    vi.useRealTimers()
    vi.unstubAllGlobals()
  }
})

test('S2.2: a Stop every read says deactivating: 359 reads, run-stopping at 60 000 and 359 999 ms, run-timeout-stop at 360 000 ms, the next read 5 s later', async () => {
  const { bff, main } = await loaded({ instances: list('active') })
  const stop = button(main, 'run-stop')
  press(stop)
  await flush()
  expect(posts(bff)).toEqual(['/api/instances/default/stop'])
  bff.setInstances(list('deactivating'))
  const n = reads(bff)
  const seen = await statusEachSecond(main, 359)
  expect(reads(bff), '359 reads by 359 000 ms').toBe(n + 359)
  expect(
    distinct(seen),
    'the status stays run-stopping for 359 s (S2.5)',
  ).toEqual([en('run-stopping')])
  expect(
    inactiveSet(main),
    'deactivating leaves all three inactive (40a)',
  ).toEqual([true, true, true])
  expectState(main, 'instance-state-deactivating')

  // One ms before the deadline: nothing has happened.
  await advance(999)
  expect(runStatus(main).textContent, 'at 359 999 ms').toBe(en('run-stopping'))
  expect(reads(bff), 'no read at 359 999 ms').toBe(n + 359)

  // The deadline: the status says the six minute text, no read goes.
  await advance(1)
  expect(runStatus(main).textContent, 'at 360 000 ms').toBe(
    en('run-timeout-stop'),
  )
  expect(reads(bff), 'no action read at or after the deadline').toBe(n + 359)
  expectState(main, 'instance-state-deactivating')
  expect(
    inactiveSet(main),
    'the buttons follow the state the header shows (40a)',
  ).toEqual([true, true, true])
  expect(document.activeElement, 'focus stayed on Stop (40b)').toBe(stop)
  expect(runAlerts(main), 'a timeout is not an alert').toEqual([])

  // The idle chain resumes 5 s after the deadline (41c).
  await advance(4999)
  expect(reads(bff)).toBe(n + 359)
  await advance(1)
  expect(reads(bff), 'the idle read 5 s after the 360 000 ms mark').toBe(
    n + 360,
  )
})

test('S2.2: a Stop whose read answers inactive at 240 000 ms settles with run-stopped', async () => {
  const { bff, main } = await loaded({ instances: list('active') })
  const stop = button(main, 'run-stop')
  press(stop)
  await flush()
  bff.setInstances(list('deactivating'))
  const n = reads(bff)
  const seen = await statusEachSecond(main, 239)
  expect(distinct(seen), 'run-stopping to 239 000 ms').toEqual([
    en('run-stopping'),
  ])
  bff.setInstances(list('inactive'))
  await advance(1000)
  expect(reads(bff), 'the read at 240 000 ms').toBe(n + 240)
  expect(runStatus(main).textContent).toBe(en('run-stopped'))
  expectState(main, 'instance-state-inactive')
  expect(inactiveSet(main)).toEqual([false, true, true])
  expect(document.activeElement, 'focus stayed on Stop').toBe(stop)
  await advance(4000)
  expect(reads(bff), 'not 1 s any more').toBe(n + 240)
  await advance(1000)
  expect(reads(bff), 'the idle chain, 5 s after the settling read').toBe(
    n + 241,
  )
})

test('S2.3: a Restart reading deactivating to 200 000 ms and activating to 290 000 ms: run-restarting throughout, no run-timeout at 60 000 ms, run-restarted when it reads active', async () => {
  const { bff, main } = await loaded({ instances: list('active') })
  const restart = button(main, 'run-restart')
  press(restart)
  await flush()
  expect(posts(bff)).toEqual(['/api/instances/default/restart'])
  const n = reads(bff)

  bff.setInstances(list('deactivating'))
  const stopping = await statusEachSecond(main, 200)
  expect(distinct(stopping), 'run-restarting through deactivating').toEqual([
    en('run-restarting'),
  ])
  expect(stopping[59], 'at 60 000 ms').toBe(en('run-restarting'))
  expect(timeoutSamples(stopping), 'no run-timeout* in 200 s').toEqual([])
  expect(inactiveSet(main), 'deactivating leaves all three inactive').toEqual([
    true,
    true,
    true,
  ])

  bff.setInstances(list('activating'))
  const starting = await statusEachSecond(main, 90)
  expect(distinct(starting), 'run-restarting through activating').toEqual([
    en('run-restarting'),
  ])
  expect(timeoutSamples(starting), 'no run-timeout* by 290 000 ms').toEqual([])
  expect(reads(bff), 'one read each second to 290 000 ms').toBe(n + 290)
  expect(
    within(main).queryByText(/did not reach the expected state/),
    'no timeout text anywhere on the page',
  ).toBeNull()

  bff.setInstances(list('active'))
  await advance(1000)
  expect(runStatus(main).textContent).toBe(en('run-restarted'))
  expectState(main, 'instance-state-active')
  expect(inactiveSet(main)).toEqual([true, false, false])
  expect(document.activeElement, 'focus stayed on Restart (40b)').toBe(restart)
})

test('S2.3b: a Restart that reads inactive in the Windows gap keeps run-restarting and all three buttons until a read says active', async () => {
  const { bff, main } = await loaded({ instances: list('active') })
  const restart = button(main, 'run-restart')
  press(restart)
  await flush()
  bff.setInstances(list('deactivating'))
  const seen = await statusEachSecond(main, 65)
  expect(distinct(seen), 'past the old 60 s mark').toEqual([
    en('run-restarting'),
  ])
  expect(inactiveSet(main)).toEqual([true, true, true])

  // The gap on Windows: the service stopped the instance and has not started
  // it yet, so a read answers inactive between the stop and the start.
  bff.setInstances(list('inactive'))
  for (const s of [66, 67]) {
    await advance(1000)
    expect(
      runStatus(main).textContent,
      `${s} 000 ms: a Restart does not settle on inactive`,
    ).toBe(en('run-restarting'))
    expect(
      inactiveSet(main),
      `${s} 000 ms: Start stays inactive through the gap`,
    ).toEqual([true, true, true])
    expectState(main, 'instance-state-inactive')
  }

  bff.setInstances(list('activating'))
  await advance(1000)
  expect(runStatus(main).textContent).toBe(en('run-restarting'))
  expect(inactiveSet(main)).toEqual([true, true, true])

  bff.setInstances(list('active'))
  await advance(1000)
  expect(
    runStatus(main).textContent,
    'the gap ends at the first active read',
  ).toBe(en('run-restarted'))
  expect(inactiveSet(main)).toEqual([true, false, false])
  expect(document.activeElement, 'focus stayed on Restart').toBe(restart)
})

test('S2.3b: a Restart stuck at inactive in the gap: run-restarting to 359 999 ms, run-timeout-stop at 360 000 ms', async () => {
  const { bff, main } = await loaded({ instances: list('active') })
  const restart = button(main, 'run-restart')
  press(restart)
  await flush()
  bff.setInstances(list('deactivating'))
  await advance(1000)
  const n = reads(bff)
  bff.setInstances(list('inactive'))
  const seen = await statusEachSecond(main, 358)
  expect(reads(bff), 'the reads of the gap go on each second').toBe(n + 358)
  expect(timeoutSamples(seen.slice(0, 59)), 'no timeout before 60 s').toEqual(
    [],
  )
  expect(distinct(seen), 'run-restarting through the gap').toEqual([
    en('run-restarting'),
  ])
  expect(inactiveSet(main), 'the gap keeps all three inactive').toEqual([
    true,
    true,
    true,
  ])

  await advance(999)
  expect(runStatus(main).textContent, 'at 359 999 ms').toBe(
    en('run-restarting'),
  )
  expect(reads(bff), 'no read at 359 999 ms').toBe(n + 358)
  await advance(1)
  expect(runStatus(main).textContent, 'at 360 000 ms').toBe(
    en('run-timeout-stop'),
  )
  expect(reads(bff), 'no action read at the deadline').toBe(n + 358)
  expect(
    inactiveSet(main),
    'at the deadline the buttons follow the state shown (40a)',
  ).toEqual([false, true, true])
  expect(document.activeElement, 'focus stayed on Restart (40b)').toBe(restart)
})

test('S2.4: Start is unchanged: 59 reads, run-timeout at 60 000 ms and not run-timeout-stop, the next read 5 s later', async () => {
  const { bff, main } = await loaded({ instances: list('inactive') })
  press(button(main, 'run-start'))
  await flush()
  bff.setInstances(list('activating'))
  const n = reads(bff)
  const seen = await statusEachSecond(main, 59)
  expect(reads(bff)).toBe(n + 59)
  expect(distinct(seen), 'run-starting to 59 000 ms').toEqual([
    en('run-starting'),
  ])
  await advance(1000)
  expect(reads(bff), 'no read at 60 000 ms').toBe(n + 59)
  const text = runStatus(main).textContent
  expect(text).toBe(en('run-timeout'))
  expect(text, 'a Start keeps the one minute text').not.toBe(
    en('run-timeout-stop'),
  )
  expect(inactiveSet(main), 'the last state is activating: all three').toEqual([
    true,
    true,
    true,
  ])
  await advance(4999)
  expect(reads(bff)).toBe(n + 59)
  await advance(1)
  expect(reads(bff), 'the idle chain, 5 s after the 60 000 ms mark').toBe(
    n + 60,
  )
})

test('S2.5: the one run status changes text only at the outcome and at the deadline, never each second', async () => {
  // To the deadline: run-stopping at every read, the new text at 360 000 ms.
  const first = await loaded({ instances: list('active') })
  press(button(first.main, 'run-stop'))
  await flush()
  first.bff.setInstances(list('deactivating'))
  const toDeadline = await statusEachSecond(first.main, 359)
  expect(
    distinct(toDeadline),
    '359 reads, one text: nothing announced each second',
  ).toEqual([en('run-stopping')])
  await advance(1000)
  expect(
    runStatus(first.main).textContent,
    'the deadline is the only change',
  ).toBe(en('run-timeout-stop'))
  cleanup()
  vi.useRealTimers()
  vi.unstubAllGlobals()

  // Settled: the outcome is the only change, and 60 000 ms changes nothing.
  const second = await loaded({ instances: list('active') })
  press(button(second.main, 'run-stop'))
  await flush()
  second.bff.setInstances(list('deactivating'))
  const toSettle = await statusEachSecond(second.main, 61)
  expect(toSettle[59], 'at 60 000 ms').toBe(en('run-stopping'))
  expect(distinct(toSettle), 'no change past the old 60 s mark').toEqual([
    en('run-stopping'),
  ])
  second.bff.setInstances(list('inactive'))
  await advance(1000)
  expect(runStatus(second.main).textContent, 'the outcome').toBe(
    en('run-stopped'),
  )
})

test('S2.6: strings/en_US.json carries run-timeout-stop with the brief text: six minutes, not a minute', () => {
  const table: Record<string, string> = strings
  expect(
    table['run-timeout-stop'],
    'the strings file holds the new key, as the test English does',
  ).toBe(en('run-timeout-stop'))
  expect(en('run-timeout-stop')).toContain('six minutes')
  expect(en('run-timeout-stop')).toContain('The state above keeps updating.')
  expect(en('run-timeout-stop'), 'a text of its own').not.toBe(
    en('run-timeout'),
  )
})
