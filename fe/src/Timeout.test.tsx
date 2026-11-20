// A time limit for every request (fe/briefs/01-server.md, slice 1c, section
// 4.8, criteria 47 to 47e): call() aborts a GET after 15 s and any other
// method after 120 s, and the page treats a timed-out call as that call's
// network failure; a save that times out says so (save-timeout). The limits
// run on fake timers, against a fetch that rejects with its signal's reason
// when the request is aborted (held(), 47e). 41d (b) is in
// RunControls.test.tsx. "1c.47e" is the brief's criterion 47e.
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  within,
} from '@testing-library/react'
import { afterEach, expect, test, vi } from 'vitest'
import App from './App.tsx'
import Version from './Version.tsx'
import { call } from './api.ts'
import {
  before,
  en,
  fakeBff,
  held,
  reply,
  startAt,
  type BffOptions,
  type Instance,
} from './test/bff.ts'

afterEach(() => {
  // Unmount first, so the page clears its own timers; then drop the fake
  // clock, which discards any timer left.
  cleanup()
  vi.useRealTimers()
  vi.unstubAllGlobals()
  startAt('')
})

// ── Helpers (the fake-timer conventions of RunControls.test.tsx) ─────────

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

/** Moves the clock by `ms` in one step, then flushes. */
async function advance(ms: number) {
  await act(async () => {
    await vi.advanceTimersByTimeAsync(ms)
  })
  await flush()
}

/** fetch stubbed with one held reply for every request. */
function heldFetch() {
  const h = held()
  vi.stubGlobal(
    'fetch',
    vi.fn<typeof globalThis.fetch>((_input, init) => h.answer(init)),
  )
  return h
}

/** What a call settled with: its reply, or the error it rejected with. */
function outcome(p: Promise<unknown>) {
  const seen: { value?: unknown; error?: unknown; done: boolean } = {
    done: false,
  }
  p.then(
    (value) => Object.assign(seen, { value, done: true }),
    (error: unknown) => Object.assign(seen, { error, done: true }),
  )
  return seen
}

function errorName(e: unknown): string | undefined {
  return (e as { name?: unknown } | undefined)?.name as string | undefined
}

function list(state: string): Instance[] {
  return [
    { id: 'default', engine: 'llama.cpp', state },
    { id: 'coder', engine: 'llama.cpp', state: 'inactive' },
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

function reads(bff: ReturnType<typeof fakeBff>): number {
  return bff.to('GET', '/api/instances').length
}

function posts(bff: ReturnType<typeof fakeBff>): string[] {
  return bff.calls
    .filter((c) => c.method === 'POST' && c.path.startsWith('/api/instances'))
    .map((c) => c.path)
}

function button(main: HTMLElement, key: string): HTMLButtonElement {
  return within(main).getByRole('button', { name: en(key) })
}

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

function inactiveSet(main: HTMLElement): boolean[] {
  return ['run-start', 'run-stop', 'run-restart'].map(
    (k) => button(main, k).getAttribute('aria-disabled') === 'true',
  )
}

function serverTitle(main: HTMLElement): HTMLElement {
  return within(main).getByRole('heading', {
    level: 2,
    name: en('server-title'),
  })
}

function inBoot(main: HTMLElement, el: HTMLElement): boolean {
  const boot = within(main).queryByRole('group', { name: en('boot-label') })
  return boot !== null && boot.contains(el)
}

/**
 * True when `el` is in the Instance tags form (brief 12, 12 T1.5), whose own
 * status, alert and Sign in (the Action feedback of fe/DESIGN.md 5.2) are not
 * the run controls'.
 */
function inTags(main: HTMLElement, el: HTMLElement): boolean {
  const box = within(main).queryByRole('textbox', {
    name: en('instance-tags-own'),
  })
  const form = box?.closest('form') ?? null
  return form !== null && form.contains(el)
}

/** The run status: the one status line of the header (before the Server h2),
 * outside Start at boot and outside the Instance tags form. */
function runStatus(main: HTMLElement): HTMLElement {
  const h2 = serverTitle(main)
  const all = within(main)
    .getAllByRole('status')
    .filter((s) => before(s, h2) && !inBoot(main, s) && !inTags(main, s))
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

/** The save bar's status line: the one status of the Server section. */
function saveStatus(main: HTMLElement): HTMLElement {
  const h2 = serverTitle(main)
  const all = within(main)
    .getAllByRole('status')
    .filter((s) => before(h2, s))
  expect(all, 'one status line in the Server section').toHaveLength(1)
  return all[0]!
}

function stateText(stateKey: string): string {
  return en('instance-state', en(stateKey))
}

const PUT = ['PUT', '/api/instances/default/server'] as const

// ── Failure paths first: what a timed-out call shows (47b to 47d) ───────

test('1c.47d, 47e: a save held to 120 000 ms shows save-timeout in the bar alert; edits kept, dirty, the next Save sends the same revision', async () => {
  const h = held()
  const { bff, main } = await loaded({
    save: (_body, n, init) =>
      n === 1
        ? h.answer(init)
        : reply(200, { revision: 'r-2', running: false }),
  })
  type(portBox(main), '8581')
  fireEvent.click(button(main, 'save-button'))
  await flush()
  expect(bff.to(...PUT)).toHaveLength(1)
  await advance(119_999)
  expect(h.signals[0]?.aborted, 'not aborted at 119 999 ms').toBe(false)
  expect(within(main).queryByText(en('save-timeout'))).toBeNull()
  await advance(1)
  expect(h.signals[0]?.aborted, 'aborted at 120 000 ms').toBe(true)
  const text = within(main).queryByText(en('save-timeout'))
  expect(text, 'the bar reads save-timeout').not.toBeNull()
  const alert = text!.closest('[role="alert"]')
  expect(alert, 'in a role="alert"').not.toBeNull()
  expect(before(serverTitle(main), alert!), 'the bar alert').toBe(true)
  expect(within(main).queryByText(en('save-failed')), 'not "Not saved"').toBe(
    null,
  )
  expect(portBox(main).value, 'the edit is kept').toBe('8581')
  expect(saveStatus(main).textContent, 'still dirty').toBe(en('save-unsaved'))
  expect(
    button(main, 'save-button').getAttribute('aria-disabled') ?? 'false',
    'Save is usable',
  ).toBe('false')
  expect(
    within(screen.getByRole('banner')).queryByRole('button', {
      name: en('session-sign-out'),
    }),
    'still signed in',
  ).not.toBeNull()
  fireEvent.click(button(main, 'save-button'))
  await flush()
  expect(bff.to(...PUT)).toHaveLength(2)
  expect(bff.to(...PUT)[1]!.body, 'the same revision').toMatchObject({
    revision: 'r-1731628800-42',
    values: { Port: 8581 },
  })
})

test('1c.47b, 47e: a run POST held to 120 000 ms shows request-failed, empties the status, and the idle poll reads 5 000 ms later', async () => {
  const h = held()
  const { bff, main } = await loaded({
    instances: list('active'),
    run: (_action, _id, _n, init) => h.answer(init),
  })
  press(button(main, 'run-stop'))
  await flush()
  expect(posts(bff)).toEqual(['/api/instances/default/stop'])
  const n = reads(bff)
  await advance(119_999)
  expect(h.signals[0]?.aborted, 'not aborted at 119 999 ms').toBe(false)
  expect(runStatus(main).textContent).toBe(en('run-stopping'))
  expect(runAlerts(main)).toEqual([])
  await advance(1)
  expect(h.signals[0]?.aborted, 'aborted at 120 000 ms').toBe(true)
  expect(runAlerts(main)).toEqual([en('request-failed')])
  expect(runStatus(main).textContent, 'status emptied').toBe('')
  expect(inactiveSet(main), '40a for the last state, active').toEqual([
    true,
    false,
    false,
  ])
  expect(reads(bff), 'nothing read while the POST ran').toBe(n)
  await advance(4999)
  expect(reads(bff), 'none at 4 999 ms').toBe(n)
  await advance(1)
  expect(reads(bff), 'one GET 5 000 ms after the abort').toBe(n + 1)
})

test('1c.47b, 47e: a sign-in held to 120 000 ms shows signin-failed and the button is usable again', async () => {
  const h = held()
  const { bff, main } = await open(
    { session: 'signed-out', signIn: h.answer },
    '#/sign-in',
  )
  type(
    within(main).getByRole('textbox', { name: en('signin-username') }),
    'alice',
  )
  type(
    within(main).getByLabelText(en('signin-password'), { selector: 'input' }),
    'hunter2',
  )
  fireEvent.click(button(main, 'signin-submit'))
  await flush()
  expect(bff.to('POST', '/api/session')).toHaveLength(1)
  await advance(119_999)
  expect(h.signals[0]?.aborted, 'not aborted at 119 999 ms').toBe(false)
  expect(within(main).queryByRole('alert')).toBeNull()
  expect(
    within(main).queryByRole('button', { name: en('signin-busy') }),
    'still busy',
  ).not.toBeNull()
  await advance(1)
  expect(h.signals[0]?.aborted, 'aborted at 120 000 ms').toBe(true)
  expect(within(main).getByRole('alert').textContent).toBe(en('signin-failed'))
  const submit = button(main, 'signin-submit')
  expect(submit.disabled).toBe(false)
  expect(submit.getAttribute('aria-disabled')).toBeNull()
  fireEvent.click(submit)
  await flush()
  expect(bff.to('POST', '/api/session'), 'a retry is sent').toHaveLength(2)
})

test('1c.47c, 47e: a held idle read is aborted 15 000 ms after it was sent; the state stays, nothing shown; one new GET 5 000 ms later', async () => {
  const { bff, main } = await loaded({ instances: list('active') })
  const h = held()
  bff.setInstances(h.answer)
  await advance(5000)
  expect(reads(bff), 'the idle read after the opening one').toBe(2)
  await advance(14_999)
  expect(h.signals[0]?.aborted, 'not aborted at 14 999 ms').toBe(false)
  bff.setInstances(list('inactive'))
  await advance(1)
  expect(h.signals[0]?.aborted, 'aborted at 15 000 ms').toBe(true)
  expect(errorName(h.signals[0]?.reason)).toBe('TimeoutError')
  expect(
    within(main).queryByText(stateText('instance-state-active')),
    'the state line unchanged',
  ).not.toBeNull()
  expect(
    within(main)
      .queryAllByRole('alert')
      .map((a) => a.textContent),
    'nothing shown',
  ).toEqual([])
  expect(runStatus(main).textContent).toBe('')
  expect(reads(bff), 'nothing sent at the abort').toBe(2)
  await advance(4999)
  expect(reads(bff), 'none 4 999 ms after the abort').toBe(2)
  await advance(1)
  expect(reads(bff), 'one new GET 5 000 ms after the abort').toBe(3)
})

test('1c.47b, 47e: a held version read is not aborted at 14 999 ms; at 15 000 ms it is, and the page shows version-unavailable', async () => {
  fakeClock()
  const h = heldFetch()
  render(<Version onSignedOut={() => {}} />)
  await flush()
  const signal = h.signals[0]
  expect(signal, 'the version read passes a signal to fetch').toBeInstanceOf(
    AbortSignal,
  )
  await advance(14_999)
  expect(signal!.aborted, 'not aborted at 14 999 ms').toBe(false)
  expect(screen.queryByText(en('version-unavailable'))).toBeNull()
  await advance(1)
  expect(signal!.aborted, 'aborted at 15 000 ms').toBe(true)
  expect(errorName(signal!.reason)).toBe('TimeoutError')
  expect(
    screen.queryByText(en('version-unavailable')),
    'the page shows version-unavailable',
  ).not.toBeNull()
})

// ── call(): the limits (47, 47a) ────────────────────────────────────────

test('1c.47, 47a, 47e: a GET held is not aborted at 14 999 ms; at 15 000 ms fetch signal is aborted with TimeoutError and call() rejects with it', async () => {
  fakeClock()
  const h = heldFetch()
  const seen = outcome(call('/api/version'))
  await advance(0)
  const signal = h.signals[0]
  expect(signal, 'call() passes a signal to fetch').toBeInstanceOf(AbortSignal)
  await advance(14_999)
  expect(signal!.aborted, 'at 14 999 ms').toBe(false)
  expect(seen.done).toBe(false)
  await advance(1)
  expect(signal!.aborted, 'at 15 000 ms').toBe(true)
  expect(errorName(signal!.reason), 'the reason').toBe('TimeoutError')
  expect(seen.done, 'call() settled').toBe(true)
  expect(errorName(seen.error), 'call() rejects with TimeoutError').toBe(
    'TimeoutError',
  )
})

test('1c.47, 47e: a PUT held is not aborted at 119 999 ms; at 120 000 ms it is, with TimeoutError', async () => {
  fakeClock()
  const h = heldFetch()
  const seen = outcome(
    call('/api/instances/default/server', { method: 'PUT', body: '{}' }),
  )
  await advance(0)
  const signal = h.signals[0]
  expect(signal, 'call() passes a signal to fetch').toBeInstanceOf(AbortSignal)
  await advance(15_000)
  expect(signal!.aborted, 'not the GET limit').toBe(false)
  await advance(104_999)
  expect(signal!.aborted, 'at 119 999 ms').toBe(false)
  expect(seen.done).toBe(false)
  await advance(1)
  expect(signal!.aborted, 'at 120 000 ms').toBe(true)
  expect(errorName(seen.error), 'call() rejects with TimeoutError').toBe(
    'TimeoutError',
  )
})

test('1c.47a, 47e: a reply at 1 000 ms is returned and never aborted later; no timer is left', async () => {
  fakeClock()
  const h = heldFetch()
  const seen = outcome(call('/api/version'))
  await advance(1000)
  h.resolve(reply(200, { version: '2.99.0' }))
  await advance(0)
  expect(seen.done, 'call() settled').toBe(true)
  expect(seen.value).toEqual({
    status: 200,
    ok: true,
    body: { version: '2.99.0' },
  })
  expect(vi.getTimerCount(), 'the timer was cleared').toBe(0)
  await advance(200_000)
  expect(h.signals[0]!.aborted, 'no abort after the answer').toBe(false)
})
