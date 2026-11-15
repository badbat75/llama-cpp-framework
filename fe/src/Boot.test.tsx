// Start at boot on the instance page (fe/briefs/01-server.md, slice 1c,
// section 4.6, criteria 45 to 45g, with 40, 43 to 43b, 46a and section 6):
// the checkbox field below the run controls, read once with the header,
// written at once by an administrator, disabled for a user, its errors and a
// sign-in after a 401. The whole app renders against a fake bff (mocked
// fetch) on fake timers, as RunControls.test.tsx, so the idle poll is
// counted exactly; "1c.45a" is the brief's criterion 45a.
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  within,
} from '@testing-library/react'
import { StrictMode } from 'react'
import { afterEach, beforeEach, expect, test, vi } from 'vitest'
import App from './App.tsx'
import {
  before,
  deferred,
  en,
  fakeBff,
  isHidden,
  navigate,
  reply,
  startAt,
  stubDialog,
  type BffOptions,
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

/** The app at `hash` (an administrator by default), loaded. */
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

const BOOT = '/api/instances/default/boot'
const SERVER = '/api/instances/default/server'

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

function bootGroup(main: HTMLElement): HTMLElement {
  return within(main).getByRole('group', { name: en('boot-label') })
}

function bootBox(main: HTMLElement): HTMLInputElement {
  return within(bootGroup(main)).getByRole('checkbox', {
    name: en('boot-toggle'),
  }) as HTMLInputElement
}

/** The field's status line: the one status of the group (45f). */
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

function aboutBoot(main: HTMLElement): HTMLButtonElement {
  return within(bootGroup(main)).getByRole('button', {
    name: `${en('field-about')} ${en('boot-label')}`,
  }) as HTMLButtonElement
}

function serverTitle(main: HTMLElement): HTMLElement {
  return within(main).getByRole('heading', {
    level: 2,
    name: en('server-title'),
  })
}

function button(main: HTMLElement, key: string): HTMLButtonElement {
  return within(main).getByRole('button', { name: en(key) })
}

/** The run status: the header's status outside Start at boot. */
function runStatus(main: HTMLElement): HTMLElement {
  const h2 = serverTitle(main)
  const boot = within(main).queryByRole('group', { name: en('boot-label') })
  const all = within(main)
    .getAllByRole('status')
    .filter((s) => before(s, h2) && !(boot?.contains(s) ?? false))
  expect(all, 'one run status').toHaveLength(1)
  return all[0]!
}

/** The run alert's Sign in (43a), outside Start at boot. */
function runSignIn(main: HTMLElement): HTMLButtonElement {
  const boot = bootGroup(main)
  const all = within(main)
    .queryAllByRole('button', { name: en('session-sign-in') })
    .filter((b) => before(b, serverTitle(main)) && !boot.contains(b))
  expect(all, 'one run Sign in').toHaveLength(1)
  return all[0] as HTMLButtonElement
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

function h1(main: HTMLElement): HTMLElement {
  return within(main).getByRole('heading', { level: 1, name: 'default' })
}

function inactive(el: HTMLElement): boolean {
  return el.getAttribute('aria-disabled') === 'true'
}

/** Focuses `el` and clicks it, as a pointer, Enter or Space does. */
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

/** Boot answers: read `enabled` first, and `write` for every PUT. */
function boot(
  enabled: boolean,
  write?: (body: unknown, n: number) => Response | Promise<Response>,
  reread?: (n: number) => Response | Promise<Response>,
): BffOptions['boot'] {
  return {
    read: (_id, n) =>
      n === 1 || reread === undefined ? reply(200, { enabled }) : reread(n),
    write: write ? (body, _id, n) => write(body, n) : undefined,
  }
}

// ── Failure paths first: a failed read (45a) ────────────────────────────

test('1c.45a: a failed read shows the legend, its About toggle and a plain paragraph by key in place of the checkbox, never the body', async () => {
  const answers: [string, () => Response | Promise<Response>, string][] = [
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
      '404 client-no-instance',
      () => reply(404, { key: 'client-no-instance', args: [] }),
      en('boot-load-failed'),
    ],
    [
      '502 client-other',
      () => reply(502, { key: 'client-other', args: ['secret-detail'] }),
      en('boot-load-failed'),
    ],
    [
      '500 no key',
      () => reply(500, { error: 'secret-detail' }),
      en('boot-load-failed'),
    ],
    ['200 {}', () => reply(200, {}), en('boot-load-failed')],
    [
      '200 enabled a string',
      () => reply(200, { enabled: 'true' }),
      en('boot-load-failed'),
    ],
    ['200 null', () => reply(200, 'null'), en('boot-load-failed')],
    ['200 not json', () => reply(200, 'secret-detail'), en('boot-load-failed')],
    [
      'network',
      () => Promise.reject(new TypeError('secret-detail')),
      en('boot-load-failed'),
    ],
  ]
  for (const [what, read, text] of answers) {
    const { main } = await loaded({ boot: { read } })
    const group = within(main).queryByRole('group', { name: en('boot-label') })
    expect(group, `${what}: the group is shown`).not.toBeNull()
    const g = within(group!)
    expect(g.queryByRole('checkbox'), `${what}: no checkbox`).toBeNull()
    const message = g.getByText(text)
    expect(message.tagName, `${what}: a paragraph`).toBe('P')
    expect(
      message.closest('[role="alert"], [role="status"]'),
      `${what}: not an alert nor a status`,
    ).toBeNull()
    // The legend's About toggle stays; no retry button (45a).
    expect(
      g.getAllByRole('button').map((b) => b.textContent?.trim()),
      `${what}: only the About toggle`,
    ).toEqual([`${en('field-about')} ${en('boot-label')}`])
    expect(document.body.textContent, what).not.toContain('secret-detail')
    cleanup()
    vi.useRealTimers()
    vi.unstubAllGlobals()
  }
})

test('1c.45a: until the read answers the field is not rendered; then the checkbox shows enabled', async () => {
  for (const enabled of [true, false]) {
    const gate = deferred()
    const { main } = await loaded({ boot: { read: () => gate.promise } })
    expect(button(main, 'run-start'), 'the header is shown').toBeTruthy()
    expect(
      within(main).queryByRole('group', { name: en('boot-label') }),
      `${enabled}: no group while the read runs`,
    ).toBeNull()
    expect(
      within(main).queryByRole('checkbox', { name: en('boot-toggle') }),
      `${enabled}: no checkbox while the read runs`,
    ).toBeNull()
    await act(async () => {
      gate.resolve(reply(200, { enabled }))
    })
    await flush()
    expect(bootBox(main).checked, String(enabled)).toBe(enabled)
    cleanup()
    vi.useRealTimers()
    vi.unstubAllGlobals()
  }
})

test('1c.45a: read once when the header is shown, not polled; not read without a header; read again when the page opens again', async () => {
  const { bff, main } = await loaded()
  expect(bootReads(bff), 'one read at opening').toBe(1)
  await advance(5000)
  await advance(5000)
  await advance(5000)
  expect(bff.to('GET', '/api/instances').length, 'the idle poll ran').toBe(4)
  expect(bootReads(bff), 'not polled').toBe(1)
  // Opening the page again reads again.
  navigate('#/')
  await flush()
  navigate('#/instances/default')
  await flush()
  expect(bootGroup(main)).toBeTruthy()
  expect(bootReads(bff)).toBe(2)
  cleanup()
  vi.useRealTimers()
  vi.unstubAllGlobals()

  const missing = await open({}, '#/instances/ghost')
  expect(
    within(missing.main).queryByText(en('instance-not-found', 'ghost')),
  ).not.toBeNull()
  expect(missing.bff.to('GET', '/api/instances/ghost/boot')).toHaveLength(0)
  expect(
    within(missing.main).queryByRole('group', { name: en('boot-label') }),
  ).toBeNull()
})

// ── Who can change it (45b) ─────────────────────────────────────────────

test('1c.45b: a user gets the checkbox disabled, showing the state read; a click sends nothing', async () => {
  for (const enabled of [true, false]) {
    const { bff, main } = await loaded({
      session: 'user',
      boot: boot(enabled),
    })
    const box = bootBox(main)
    expect(box.disabled, String(enabled)).toBe(true)
    expect(box.checked, String(enabled)).toBe(enabled)
    fireEvent.click(box)
    await flush()
    expect(bootWrites(bff), `${enabled}: no write`).toEqual([])
    expect(bootBox(main).checked, String(enabled)).toBe(enabled)
    cleanup()
    vi.useRealTimers()
    vi.unstubAllGlobals()
  }
})

test('1c.45b: an administrator gets the checkbox enabled', async () => {
  const { main } = await loaded()
  const box = bootBox(main)
  expect(box.disabled).toBe(false)
  expect(inactive(box)).toBe(false)
})

// ── Failure paths of the write (45e) ────────────────────────────────────

test('1c.45e: a failed write shows its alert by key, never the body; status empty; the box shows the state read again', async () => {
  const answers: [string, () => Response | Promise<Response>, string][] = [
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
      '404 client-no-instance',
      () => reply(404, { key: 'client-no-instance', args: [] }),
      en('instance-not-found', 'default'),
    ],
    [
      '502 client-service-failed',
      () =>
        reply(502, {
          key: 'client-service-failed',
          args: ['secret-detail'],
        }),
      en('api-client-service-failed'),
    ],
    [
      '502 client-other',
      () => reply(502, { key: 'client-other', args: ['secret-detail'] }),
      en('request-failed'),
    ],
    [
      '422 another key',
      () => reply(422, { key: 'secret-detail', args: [] }),
      en('request-failed'),
    ],
    ['500 not json', () => reply(500, 'secret-detail'), en('request-failed')],
    [
      'network',
      () => Promise.reject(new TypeError('secret-detail')),
      en('request-failed'),
    ],
    // A 2xx of another shape is a failure too (45d).
    ['200 {}', () => reply(200, {}), en('request-failed')],
    [
      '200 enabled a string',
      () => reply(200, { enabled: 'true' }),
      en('request-failed'),
    ],
    ['204', () => reply(204), en('request-failed')],
    ['200 not json', () => reply(200, 'secret-detail'), en('request-failed')],
  ]
  for (const [what, write, text] of answers) {
    // Read off; the change asks on; the read after the failure says on (set
    // elsewhere meanwhile), which the previous value (off) cannot give.
    const { bff, main } = await loaded({
      boot: boot(false, write, () => reply(200, { enabled: true })),
    })
    press(bootBox(main))
    await flush()
    expect(bootWrites(bff), what).toEqual([{ enabled: true }])
    expect(bootAlerts(main), what).toEqual([text])
    expect(bootStatus(main).textContent, `${what}: status empty`).toBe('')
    expect(bootReads(bff), `${what}: read again`).toBe(2)
    expect(bootBox(main).checked, `${what}: the state read again`).toBe(true)
    expect(inactive(bootBox(main)), `${what}: active again`).toBe(false)
    expect(document.activeElement, `${what}: focus stays`).toBe(bootBox(main))
    expect(bootSignIns(main), `${what}: no Sign in`).toEqual([])
    expect(document.body.textContent, what).not.toContain('secret-detail')
    cleanup()
    vi.useRealTimers()
    vi.unstubAllGlobals()
  }
})

test('1c.45e: while the read after a failure runs the box stays aria-disabled; when it fails the box shows its value before the change', async () => {
  for (const before_ of [false, true]) {
    const gate = deferred()
    const { bff, main } = await loaded({
      boot: boot(
        before_,
        () => reply(503, { key: 'client-unavailable', args: [] }),
        () => gate.promise,
      ),
    })
    press(bootBox(main))
    await flush()
    expect(bootReads(bff), String(before_)).toBe(2)
    expect(inactive(bootBox(main)), `${before_}: until the read answers`).toBe(
      true,
    )
    expect(bootBox(main).disabled, `${before_}: never disabled`).toBe(false)
    await act(async () => {
      gate.resolve(reply(503, { key: 'client-unavailable', args: [] }))
    })
    await flush()
    expect(bootBox(main).checked, `${before_}: the value before`).toBe(before_)
    expect(inactive(bootBox(main)), String(before_)).toBe(false)
    expect(bootAlerts(main), String(before_)).toEqual([
      en('api-client-unavailable'),
    ])
    expect(bootStatus(main).textContent, String(before_)).toBe('')
    cleanup()
    vi.useRealTimers()
    vi.unstubAllGlobals()
  }
})

test('1c.45e, 43a: a 401 shows session-ended-retry and a Sign in after the alert, inside the group; the page stays signed in', async () => {
  const { bff, main } = await loaded({ boot: boot(false, SESSION_ENDED) })
  type(portBox(main), '8581')
  press(bootBox(main))
  await flush()
  expect(bootAlerts(main)).toEqual([en('session-ended-retry')])
  expect(bootStatus(main).textContent).toBe('')
  const signIns = bootSignIns(main)
  expect(signIns, 'one Sign in in the group').toHaveLength(1)
  const alert = within(bootGroup(main)).getByText(en('session-ended-retry'))
  expect(before(alert, signIns[0]!), 'Sign in after the alert').toBe(true)
  // The box shows the state read again, off.
  expect(bootReads(bff)).toBe(2)
  expect(bootBox(main).checked).toBe(false)
  // 36a's state: the banner, the nav and the edits stay.
  const banner = screen.getByRole('banner')
  expect(banner.textContent).toContain(en('session-signed-in-as', 'alice'))
  expect(h1(main)).toBeTruthy()
  expect(portBox(main).value).toBe('8581')
  expect(saveStatus(main).textContent).toBe(en('save-unsaved'))
  expect(within(main).queryByText(en('signin-required'))).toBeNull()
})

test('1c.45e, 43a: the boot Sign in opens the sign-in dialog; Escape and Cancel close it, focus back on it, alert kept, nothing sent', async () => {
  for (const how of ['Escape', 'Cancel']) {
    const { bff, main } = await loaded({ boot: boot(false, SESSION_ENDED) })
    press(bootBox(main))
    await flush()
    press(bootSignIns(main)[0]!)
    await flush()
    const d = screen.getByRole('dialog', { name: en('signin-title') })
    expect(d.tagName, `${how}: a native <dialog>`).toBe('DIALOG')
    expect(dialogs.opened, `${how}: opened with showModal()`).toContain(d)
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
    expect(document.activeElement, `${how}: focus on the boot Sign in`).toBe(
      bootSignIns(main)[0],
    )
    expect(bootAlerts(main), how).toEqual([en('session-ended-retry')])
    expect(bff.to('POST', '/api/session'), how).toHaveLength(0)
    expect(bootWrites(bff), `${how}: nothing resent`).toHaveLength(1)
    cleanup()
    vi.useRealTimers()
    vi.unstubAllGlobals()
  }
})

// ── A sign-in after the 401 (43b) ───────────────────────────────────────

test('1c.43b, 45e: the same administrator signs in again: the alert and Sign in go, focus on the checkbox, edits intact, nothing resent or read', async () => {
  const { bff, main } = await loaded({ boot: boot(false, SESSION_ENDED) })
  type(portBox(main), '8581')
  press(bootBox(main))
  await flush()
  press(bootSignIns(main)[0]!)
  await flush()
  await signInInDialog('alice')
  expect(bootAlerts(main)).toEqual([])
  expect(bootSignIns(main)).toEqual([])
  expect(document.activeElement).toBe(bootBox(main))
  expect(portBox(main).value).toBe('8581')
  expect(saveStatus(main).textContent).toBe(en('save-unsaved'))
  expect(bff.to('GET', SERVER)).toHaveLength(1)
  expect(bootReads(bff), 'no new boot read').toBe(2)
  expect(bootWrites(bff), 'nothing resent').toHaveLength(1)
})

test('1c.43b, 45b: another session after a boot 401: the box follows the new role without a new read; focus on the h1', async () => {
  for (const c of [
    { user: 'carol', canConfigure: true },
    { user: 'alice', canConfigure: false },
  ]) {
    const what = `${c.user} ${c.canConfigure}`
    const { bff, main } = await loaded({
      boot: boot(false, SESSION_ENDED),
      signIn: () => reply(200, c),
    })
    press(bootBox(main))
    await flush()
    press(bootSignIns(main)[0]!)
    await flush()
    await signInInDialog(c.user)
    expect(screen.getByRole('banner').textContent, what).toContain(
      en('session-signed-in-as', c.user),
    )
    expect(bootAlerts(main), what).toEqual([])
    expect(bootSignIns(main), what).toEqual([])
    expect(bootBox(main).disabled, `${what}: the box for the new role`).toBe(
      !c.canConfigure,
    )
    expect(bootReads(bff), `${what}: no new boot read`).toBe(2)
    expect(document.activeElement, `${what}: focus on the h1`).toBe(h1(main))
    expect(bootWrites(bff), what).toHaveLength(1)
    cleanup()
    vi.useRealTimers()
    vi.unstubAllGlobals()
  }
})

test('1c.43b, 45b: a user who signs in as an administrator after a run 401 gets the box enabled, without a new read', async () => {
  const { bff, main } = await loaded({
    session: 'user',
    run: SESSION_ENDED,
    signIn: () => reply(200, { user: 'alice', canConfigure: true }),
  })
  expect(bootBox(main).disabled).toBe(true)
  press(button(main, 'run-stop'))
  await flush()
  press(runSignIn(main))
  await flush()
  await signInInDialog('alice')
  expect(bootBox(main).disabled, 'enabled for the administrator').toBe(false)
  expect(bootReads(bff), 'no new boot read').toBe(1)
  expect(document.activeElement).toBe(h1(main))
})

test('1c.43b: a sign-in from either Sign in ends both the run and the boot session-ended alerts; focus on the control of that dialog', async () => {
  for (const from of ['boot', 'run'] as const) {
    const { bff, main } = await loaded({
      run: SESSION_ENDED,
      boot: boot(false, SESSION_ENDED),
    })
    press(button(main, 'run-stop'))
    await flush()
    press(bootBox(main))
    await flush()
    expect(bootAlerts(main), from).toEqual([en('session-ended-retry')])
    expect(runSignIn(main), from).toBeTruthy()
    press(from === 'boot' ? bootSignIns(main)[0]! : runSignIn(main))
    await flush()
    await signInInDialog('alice')
    expect(bootAlerts(main), `${from}: boot alert gone`).toEqual([])
    expect(bootSignIns(main), from).toEqual([])
    const left = within(main)
      .queryAllByRole('alert')
      .filter((a) => before(a, serverTitle(main)))
    expect(left, `${from}: run alert gone`).toEqual([])
    expect(
      within(main).queryAllByRole('button', { name: en('session-sign-in') }),
      `${from}: no Sign in left in main`,
    ).toEqual([])
    expect(document.activeElement, from).toBe(
      from === 'boot' ? bootBox(main) : button(main, 'run-stop'),
    )
    expect(posts(bff), from).toHaveLength(1)
    expect(bootWrites(bff), from).toHaveLength(1)
    cleanup()
    vi.useRealTimers()
    vi.unstubAllGlobals()
  }
})

test('1c.43b, 45e: a boot 401 that answers after a sign-in from the run Sign in still shows session-ended-retry and the boot Sign in; the box shows the value read again', async () => {
  const gate = deferred()
  const { bff, main } = await loaded({
    run: SESSION_ENDED,
    // Read off; the change asks on; the read after the 401 says on.
    boot: boot(
      false,
      () => gate.promise,
      () => reply(200, { enabled: true }),
    ),
  })
  press(bootBox(main))
  await flush()
  expect(bootWrites(bff), 'the boot PUT is pending').toEqual([
    { enabled: true },
  ])
  // A run action gets a 401; the same administrator signs in again there.
  press(button(main, 'run-stop'))
  await flush()
  press(runSignIn(main))
  await flush()
  await signInInDialog('alice')
  expect(
    screen.getByRole('banner').textContent,
    'the same user, page kept',
  ).toContain(en('session-signed-in-as', 'alice'))
  expect(h1(main)).toBeTruthy()
  // Now the boot PUT answers 401: its alert is stamped when it arrives.
  await act(async () => {
    gate.resolve(SESSION_ENDED())
  })
  await flush()
  expect(bootAlerts(main), 'the boot alert shows').toEqual([
    en('session-ended-retry'),
  ])
  const signIns = bootSignIns(main)
  expect(signIns, 'one Sign in in the group').toHaveLength(1)
  const alert = within(bootGroup(main)).getByText(en('session-ended-retry'))
  expect(before(alert, signIns[0]!), 'Sign in after the alert').toBe(true)
  expect(bootStatus(main).textContent).toBe('')
  expect(bootReads(bff), 'read again after the 401').toBe(2)
  expect(bootBox(main).checked, 'the value read again').toBe(true)
  expect(inactive(bootBox(main)), 'active again').toBe(false)
})

test('1c.43b: a sign-in ends only session-ended alerts: a boot 503 alert stays after a sign-in from the run Sign in, the run alert goes', async () => {
  const { bff, main } = await loaded({
    run: SESSION_ENDED,
    boot: boot(false, () =>
      reply(503, { key: 'client-unavailable', args: [] }),
    ),
  })
  press(bootBox(main))
  await flush()
  expect(bootAlerts(main)).toEqual([en('api-client-unavailable')])
  press(button(main, 'run-stop'))
  await flush()
  expect(runSignIn(main)).toBeTruthy()
  press(runSignIn(main))
  await flush()
  await signInInDialog('alice')
  expect(bootAlerts(main), 'the boot 503 alert stays').toEqual([
    en('api-client-unavailable'),
  ])
  expect(bootSignIns(main), 'no boot Sign in').toEqual([])
  const runAlerts = within(main)
    .queryAllByRole('alert')
    .filter((a) => before(a, serverTitle(main)) && !bootGroup(main).contains(a))
  expect(runAlerts, 'the run alert is gone').toEqual([])
  expect(
    within(main).queryAllByRole('button', { name: en('session-sign-in') }),
    'no Sign in left in main',
  ).toEqual([])
  expect(bootWrites(bff), 'nothing resent').toHaveLength(1)
  expect(posts(bff), 'nothing resent').toHaveLength(1)
})

// ── The write (45c, 45d) ────────────────────────────────────────────────

test('1c.45c: a change shows the new value at once and sends PUT {enabled}; while it runs the box is aria-disabled, focus stays, a further click sends nothing, no text', async () => {
  for (const from of [false, true]) {
    const gate = deferred()
    const { bff, main } = await loaded({
      boot: boot(from, () => gate.promise),
    })
    const box = bootBox(main)
    press(box)
    await flush()
    expect(bootBox(main).checked, `${from}: the new value at once`).toBe(!from)
    expect(bootWrites(bff), String(from)).toEqual([{ enabled: !from }])
    expect(inactive(bootBox(main)), `${from}: aria-disabled`).toBe(true)
    expect(bootBox(main).disabled, `${from}: never disabled`).toBe(false)
    expect(document.activeElement, `${from}: focus stays`).toBe(bootBox(main))
    expect(bootStatus(main).textContent, `${from}: no text`).toBe('')
    expect(bootAlerts(main), String(from)).toEqual([])
    // A click (or Space, which clicks) while it runs changes nothing.
    press(bootBox(main))
    await flush()
    expect(bootWrites(bff), `${from}: no second PUT`).toHaveLength(1)
    expect(bootBox(main).checked, `${from}: unchanged`).toBe(!from)
    expect(document.activeElement, String(from)).toBe(bootBox(main))
    await act(async () => {
      gate.resolve(reply(200, { enabled: !from }))
    })
    await flush()
    expect(inactive(bootBox(main)), `${from}: active again`).toBe(false)
    cleanup()
    vi.useRealTimers()
    vi.unstubAllGlobals()
  }
})

test('1c.45d: a 2xx {enabled} sets the box from the reply and the status reads boot-on or boot-off', async () => {
  const cases: [boolean, boolean, string][] = [
    [false, true, en('boot-on')],
    [true, false, en('boot-off')],
    // The reply wins over the value asked.
    [false, false, en('boot-off')],
    [true, true, en('boot-on')],
  ]
  for (const [from, replied, status] of cases) {
    const what = `from ${from}, replied ${replied}`
    const { bff, main } = await loaded({
      boot: boot(from, () => reply(200, { enabled: replied })),
    })
    press(bootBox(main))
    await flush()
    expect(bootWrites(bff), what).toEqual([{ enabled: !from }])
    expect(bootBox(main).checked, what).toBe(replied)
    expect(bootStatus(main).textContent, what).toBe(status)
    expect(bootAlerts(main), what).toEqual([])
    expect(inactive(bootBox(main)), what).toBe(false)
    expect(bootReads(bff), `${what}: no read after a success`).toBe(1)
    cleanup()
    vi.useRealTimers()
    vi.unstubAllGlobals()
  }
})

test('1c.45d: under StrictMode a 2xx {enabled: true} still reads boot-on and the box is active again', async () => {
  fakeClock()
  startAt('#/instances/default')
  const bff = fakeBff({
    boot: boot(false, () => reply(200, { enabled: true })),
  })
  render(
    <StrictMode>
      <App />
    </StrictMode>,
  )
  await flush()
  const main = screen.getByRole('main')
  press(bootBox(main))
  await flush()
  expect(bootWrites(bff)).toEqual([{ enabled: true }])
  expect(bootStatus(main).textContent).toBe(en('boot-on'))
  expect(inactive(bootBox(main)), 'not aria-disabled').toBe(false)
})

// ── Where its messages are (45f, 40) ────────────────────────────────────

test('1c.45f, 40: the field comes after the run messages, before the Server section; its status, alert and Sign in follow the checkbox inside the group', async () => {
  const { main } = await loaded({
    run: SESSION_ENDED,
    boot: boot(false, SESSION_ENDED),
  })
  const group = bootGroup(main)
  expect(before(runStatus(main), group), 'after the run status').toBe(true)
  expect(before(group, serverTitle(main)), 'before the Server h2').toBe(true)
  expect(group.contains(runStatus(main)), 'not the run status').toBe(false)
  // The status is in the DOM before any change, empty.
  const status = bootStatus(main)
  expect(status.tagName).toBe('P')
  expect(status.textContent).toBe('')
  expect(before(bootBox(main), status), 'status after the checkbox').toBe(true)
  expect(bootAlerts(main), 'no alert until there is text').toEqual([])

  press(button(main, 'run-stop'))
  await flush()
  expect(before(runSignIn(main), group), 'after the run Sign in').toBe(true)
  press(bootBox(main))
  await flush()
  const alert = within(group).getByRole('alert')
  expect(alert.tagName).toBe('P')
  expect(before(bootStatus(main), alert), 'alert after the status').toBe(true)
  expect(before(alert, bootSignIns(main)[0]!), 'Sign in after the alert').toBe(
    true,
  )
})

test('1c.45f: the status and the alert go at the next change, and when the page is left', async () => {
  const gate = deferred()
  const { main } = await loaded({
    boot: boot(false, (_body, n) =>
      n === 1
        ? reply(200, { enabled: true })
        : n === 2
          ? reply(503, { key: 'client-unavailable', args: [] })
          : gate.promise,
    ),
  })
  press(bootBox(main))
  await flush()
  expect(bootStatus(main).textContent).toBe(en('boot-on'))
  press(bootBox(main))
  expect(bootStatus(main).textContent, 'status cleared at the change').toBe('')
  await flush()
  expect(bootAlerts(main)).toEqual([en('api-client-unavailable')])
  press(bootBox(main))
  expect(bootAlerts(main), 'alert cleared at the change').toEqual([])
  await act(async () => {
    gate.resolve(reply(200, { enabled: false }))
  })
  await flush()
  expect(bootStatus(main).textContent).toBe(en('boot-off'))
  navigate('#/')
  await flush()
  navigate('#/instances/default')
  await flush()
  expect(bootStatus(main).textContent, 'left and back: empty').toBe('')
})

test('1c.45e, 45f: a boot write that fails after the page was left sends nothing more: no read again, no other request', async () => {
  const gate = deferred()
  const { bff, main } = await loaded({ boot: boot(false, () => gate.promise) })
  press(bootBox(main))
  await flush()
  expect(bootWrites(bff), 'the boot PUT is pending').toEqual([
    { enabled: true },
  ])
  navigate('#/nope')
  await flush()
  expect(
    within(main).queryByRole('group', { name: en('boot-label') }),
    'the page is gone',
  ).toBeNull()
  const reads = bootReads(bff)
  const sent = bff.calls.length
  await act(async () => {
    gate.resolve(reply(503, { key: 'client-unavailable', args: [] }))
  })
  await flush()
  expect(bootReads(bff), 'no boot read for a page that was left').toBe(reads)
  expect(
    bff.calls.slice(sent).map((c) => `${c.method} ${c.path}`),
    'no request at all',
  ).toEqual([])
})

// ── Not part of the Server form (45) ────────────────────────────────────

test('1c.45: a boot change on a clean form leaves it clean: no unsaved status, Save and Discard inactive, no beforeunload, leaving asks nothing', async () => {
  const { bff, main } = await loaded()
  press(bootBox(main))
  await flush()
  expect(bootWrites(bff)).toHaveLength(1)
  expect(saveStatus(main).textContent).toBe('')
  expect(inactive(button(main, 'save-button'))).toBe(true)
  expect(inactive(button(main, 'save-discard'))).toBe(true)
  const e = new Event('beforeunload', { cancelable: true })
  window.dispatchEvent(e)
  expect(e.defaultPrevented, 'beforeunload not armed').toBe(false)
  const nav = screen.getByRole('navigation', { name: en('nav-label') })
  fireEvent.click(within(nav).getByRole('link', { name: 'coder' }))
  await flush()
  expect(screen.queryByRole('dialog')).toBeNull()
  expect(dialogs.opened, 'no dialog opened').toEqual([])
  expect(
    within(main).queryByRole('heading', { level: 1, name: 'coder' }),
    'the link was followed',
  ).not.toBeNull()
})

test('1c.45: with a dirty form a boot change keeps the form as it was; Discard and Save ignore the box', async () => {
  const { bff, main } = await loaded()
  type(portBox(main), '8581')
  expect(saveStatus(main).textContent).toBe(en('save-unsaved'))
  press(bootBox(main))
  await flush()
  expect(bootBox(main).checked).toBe(true)
  expect(saveStatus(main).textContent, 'still unsaved').toBe(en('save-unsaved'))
  expect(portBox(main).value).toBe('8581')
  // Discard reverts the form, not the box.
  fireEvent.click(button(main, 'save-discard'))
  await flush()
  expect(portBox(main).value).toBe('8580')
  expect(bootBox(main).checked, 'Discard leaves the box').toBe(true)
  expect(saveStatus(main).textContent, 'clean after Discard').toBe('')
  // Save sends the form only.
  type(portBox(main), '8582')
  fireEvent.click(button(main, 'save-button'))
  await flush()
  const saves = bff.to('PUT', SERVER)
  expect(saves).toHaveLength(1)
  const values = (saves[0]!.body as { values: Record<string, unknown> }).values
  expect(values.Port).toBe(8582)
  expect(
    Object.keys(values).filter((k) => /boot|enabled/i.test(k)),
    'no boot value in the save',
  ).toEqual([])
  expect(
    Object.keys(saves[0]!.body as object).sort(),
    'only revision and values',
  ).toEqual(['revision', 'values'])
  expect(bootWrites(bff), 'Save sends no boot write').toHaveLength(1)
  expect(bootBox(main).checked).toBe(true)
})

// ── Names and keyboard (45, 46a, section 6) ─────────────────────────────

test('1c.45, 6: group "Start at boot" with button "About Start at boot", which shows the hint, and checkbox "start llama-server when the system starts"', async () => {
  const { main } = await loaded()
  const group = bootGroup(main)
  const about = aboutBoot(main)
  expect(about.tagName).toBe('BUTTON')
  expect(about.getAttribute('aria-expanded')).toBe('false')
  const [first, second] = en('boot-hint').split('\n')
  expect(isHidden(within(group).queryByText(first!))).toBe(true)
  fireEvent.click(about)
  expect(about.getAttribute('aria-expanded')).toBe('true')
  expect(isHidden(within(group).queryByText(first!))).toBe(false)
  expect(isHidden(within(group).queryByText(second!))).toBe(false)
  expect(before(about, bootBox(main)), 'About before the checkbox').toBe(true)
  expect(bootBox(main).type).toBe('checkbox')
})

/** The elements Tab reaches in main, in DOM order. */
function tabbable(main: HTMLElement): HTMLElement[] {
  return [
    ...main.querySelectorAll<HTMLElement>(
      'a[href], button, input, select, textarea, [tabindex]',
    ),
  ].filter(
    (el) =>
      !(el as HTMLButtonElement).disabled &&
      el.getAttribute('tabindex') !== '-1' &&
      !isHidden(el),
  )
}

test('1c.46a: Tab goes Start, Stop, Restart, the run Sign in, About Start at boot, the checkbox, the boot Sign in, then the Server section', async () => {
  const { main } = await loaded({
    run: SESSION_ENDED,
    boot: boot(false, SESSION_ENDED),
  })
  const aboutPort = within(main).getByRole('button', {
    name: `${en('field-about')} Port`,
  })
  const names = (expected: [string, HTMLElement][]) => {
    const map = new Map(expected.map(([n, el]) => [el as Element, n]))
    return tabbable(main)
      .slice(0, expected.length)
      .map((el) => map.get(el) ?? el.outerHTML)
  }
  let expected: [string, HTMLElement][] = [
    ['Start', button(main, 'run-start')],
    ['Stop', button(main, 'run-stop')],
    ['Restart', button(main, 'run-restart')],
    ['About Start at boot', aboutBoot(main)],
    ['Start at boot', bootBox(main)],
    ['About Port', aboutPort],
  ]
  expect(names(expected)).toEqual(expected.map(([n]) => n))

  press(button(main, 'run-stop'))
  await flush()
  press(bootBox(main))
  await flush()
  expected = [
    ['Start', button(main, 'run-start')],
    ['Stop', button(main, 'run-stop')],
    ['Restart', button(main, 'run-restart')],
    ['run Sign in', runSignIn(main)],
    ['About Start at boot', aboutBoot(main)],
    ['Start at boot', bootBox(main)],
    ['boot Sign in', bootSignIns(main)[0]!],
    ['About Port', aboutPort],
  ]
  expect(names(expected)).toEqual(expected.map(([n]) => n))
})

test('1c.46a, 45c: the checkbox stays in the Tab sequence while its write runs', async () => {
  const gate = deferred()
  const { main } = await loaded({ boot: boot(false, () => gate.promise) })
  press(bootBox(main))
  await flush()
  expect(inactive(bootBox(main))).toBe(true)
  expect(tabbable(main)).toContain(bootBox(main))
})

test('1c.46a, 45b: for a user Tab goes Restart, About Start at boot, then the Server section (the checkbox is disabled)', async () => {
  const { main } = await loaded({ session: 'user' })
  const reached = tabbable(main)
  const i = reached.indexOf(button(main, 'run-restart'))
  expect(i).toBeGreaterThanOrEqual(0)
  expect(reached[i + 1]).toBe(aboutBoot(main))
  expect(reached[i + 2]).toBe(
    within(main).getByRole('button', { name: `${en('field-about')} Port` }),
  )
})
