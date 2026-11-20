// Brief 12, slice T1 (fe/briefs/12-instance-tags.md): the instance's port on
// its page, and its own tags, edited outside the Server form. The whole app
// renders at #/instances/default against a fake bff (mocked fetch); "12 T1.n"
// is the brief's criterion n. The tile criteria (T1.2, T1.3) are in
// InstanceTiles.test.tsx; App.test.tsx ("1a strings") checks the EN table of
// test/bff.ts against strings/en_US.json, so the nine keys of section 2 are
// pinned from both sides.
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import App from './App.tsx'
import strings from './strings/en_US.json'
import {
  before,
  en,
  fakeBff,
  held,
  reply,
  serverDoc,
  startAt,
  stubDialog,
  type BffOptions,
} from './test/bff.ts'

/** The brief's fixture: the item carries port, the union and its own tags. */
const TAG_ITEM = {
  id: 'default',
  engine: 'llama.cpp',
  state: 'active',
  port: 8080,
  tags: ['edge', 'v2', 'gpu'],
  instanceTags: ['v2', 'gpu'],
}

/** The tags route (brief 12 section 0, fe/briefs/api-contract.md). */
const TAGS = '/api/instances/default/tags'

/** The Server form's own save route: the tags field never uses it. */
const SERVER = '/api/instances/default/server'

/** The nine keys of section 2, as the EN table spells them. */
const KEYS = [
  'instance-port',
  'instance-tags',
  'instance-tags-own',
  'instance-tags-hint',
  'instance-tags-profile',
  'instance-tags-save',
  'instance-tags-saving',
  'instance-tags-saved',
  'api-server-bad-tags',
]

/** Lists of items answered as bytes, so a test can answer any shape. */
function itemsAnswer(items: () => unknown[]): BffOptions['instances'] {
  return () => reply(200, JSON.stringify(items()))
}

/** An answer of the tags route: the bff answers {"key", "args"} (plus "field"). */
function refused(
  status: number,
  key: string,
  args: string[] = [],
  field?: string,
) {
  return () =>
    reply(status, field === undefined ? { key, args } : { key, args, field })
}

const SESSION_ENDED = refused(401, 'session-required')

let dialogs: ReturnType<typeof stubDialog>

beforeEach(() => {
  startAt('#/instances/default')
  dialogs = stubDialog()
})

afterEach(async () => {
  // The fake clock goes first: a queued task of a clock test is a real task
  // for the next test, and flushTasks below runs with real timers.
  vi.useRealTimers()
  // A hash set lands as a hashchange in a task of its own: let it finish on
  // this test's page, not on the next test's.
  await flushTasks()
  cleanup()
  vi.unstubAllGlobals()
  dialogs.restore()
  startAt('')
})

// ── Helpers ─────────────────────────────────────────────────────────────

/** Lets jsdom's queued tasks (a hashchange, a history traversal) run. */
async function flushTasks(n = 2) {
  await act(async () => {
    for (let i = 0; i < n; i++) await new Promise((r) => setTimeout(r, 0))
  })
}

/** Fake timers for the page's 5 s read; microtasks stay real. */
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

/** The app at the instance page, with the brief's fixture, loaded. */
async function openPage(options: BffOptions = {}, clock = false) {
  if (clock) fakeClock()
  const bff = fakeBff({
    instances: [TAG_ITEM],
    boot: { read: () => reply(200, { enabled: true }) },
    ...options,
  })
  render(<App />)
  const main = screen.getByRole('main')
  if (clock) {
    // Under the fake clock a findBy never resolves: the flush runs the reads
    // due now, then the look is synchronous.
    await flush()
    within(main).getByRole('button', { name: en('save-button') })
  } else {
    await screen.findByRole('heading', { level: 1, name: 'default' })
    await within(main).findByRole('button', { name: en('save-button') })
  }
  return { bff, main }
}

/** The box, named by its visible label "Instance tags". */
function tagInput(): HTMLInputElement {
  return screen.getByRole('textbox', {
    name: en('instance-tags-own'),
  }) as HTMLInputElement
}

/** The field's form: its own, not the Server form. */
function tagForm(): HTMLElement {
  const form = tagInput().closest('form')
  expect(form, 'the field is in a form of its own').not.toBeNull()
  return form as HTMLElement
}

/** The field's Save tags button. */
function tagSave(): HTMLButtonElement {
  return within(tagForm()).getByRole('button', {
    name: en('instance-tags-save'),
  }) as HTMLButtonElement
}

/** The field's status line, always in the DOM. */
function tagStatus(): HTMLElement {
  return within(tagForm()).getByRole('status')
}

/** The texts of the field's alerts. */
function tagAlerts(): string[] {
  return within(tagForm())
    .queryAllByRole('alert')
    .map((a) => a.textContent ?? '')
}

/** The texts of the elements the box points at with aria-describedby. */
function described(el: HTMLElement): string[] {
  return (el.getAttribute('aria-describedby') ?? '')
    .split(/\s+/)
    .filter((id) => id !== '')
    .map((id) => document.getElementById(id)?.textContent ?? `(no #${id})`)
}

/** The Server section's status line (its save bar's). */
function serverStatus(main: HTMLElement): HTMLElement {
  const h2 = within(main).getByRole('heading', {
    level: 2,
    name: en('server-title'),
  })
  const all = within(main)
    .getAllByRole('status')
    .filter((s) => before(h2, s))
  expect(all, 'one status line in the Server section').toHaveLength(1)
  return all[0]!
}

/** The Server form's Port box (the field the edits-dropped notice is about). */
function portBox(main: HTMLElement): HTMLInputElement {
  return within(main).getByRole('textbox', {
    name: en('server-port'),
  }) as HTMLInputElement
}

/** Types in the box. */
function type(value: string) {
  fireEvent.change(tagInput(), { target: { value } })
}

/** Sends the field's form, as Enter in the box does (a native submit). */
function submitForm() {
  fireEvent.submit(tagForm())
}

/** Asks the browser whether leaving the page would be held. */
function asksBrowser(): boolean {
  const event = new Event('beforeunload', { cancelable: true, bubbles: true })
  window.dispatchEvent(event)
  return event.defaultPrevented
}

// ── T1.1: the texts

test('12 T1.1: en_US.json has the nine keys of section 2 with exactly their text', () => {
  const table: Record<string, string> = strings
  const missing = KEYS.filter((key) => table[key] === undefined)
  expect(missing, 'keys missing from strings/en_US.json').toEqual([])
  const wrong = KEYS.filter((key) => table[key] !== en(key))
  expect(
    wrong,
    'keys whose en_US.json text is not the English of the brief',
  ).toEqual([])
})

// ── T1.4, T1.5: the header line and the field as it opens

test('12 T1.4: the paragraph Port 8080 follows the engine line; with port 0 there is no such paragraph', async () => {
  const { main } = await openPage()
  const engine = within(main).getByText(
    en('instance-engine').replace('{0}', 'llama.cpp'),
  )
  const port = within(main).getByText(en('instance-port', '8080'))
  expect(port.tagName, 'a paragraph').toBe('P')
  expect(before(engine, port), 'the port line follows the engine line').toBe(
    true,
  )
  cleanup()
  const second = await openPage({
    instances: itemsAnswer(() => [{ ...TAG_ITEM, port: 0 }]),
  })
  expect(
    within(second.main).queryByText(/^Port \d/),
    'port 0: no port paragraph',
  ).toBeNull()
})

test('12 T1.5: the box holds the instance tags, the profile line follows it; the box is after Start at boot and before Save tags, outside the Server form', async () => {
  const { main } = await openPage()
  expect(tagInput().value, 'the box holds instanceTags, not the union').toBe(
    'v2, gpu',
  )
  const line = within(main).getByText(en('instance-tags-profile', 'edge'))
  expect(before(tagInput(), line), 'the profile line is after the box').toBe(
    true,
  )
  const boot = within(main).getByRole('checkbox', { name: en('boot-toggle') })
  const stops = [
    ...main.querySelectorAll<HTMLElement>(
      'a[href], button, input, select, textarea',
    ),
  ].filter((el) => !el.hasAttribute('disabled'))
  const at = (el: Element) => stops.indexOf(el as HTMLElement)
  expect(at(boot), 'the Start at boot checkbox is a stop').toBeGreaterThan(-1)
  expect(at(tagInput()), 'the box comes after it').toBeGreaterThan(at(boot))
  expect(at(tagSave()), 'Save tags comes after the box').toBeGreaterThan(
    at(tagInput()),
  )
  // Outside the Server form: typing in the box leaves the save bar alone.
  const server = within(main).getByRole('button', { name: en('save-button') })
  expect(
    tagInput().closest('form') === server.closest('form'),
    'the box is not in the Server form',
  ).toBe(false)
  const clean = serverStatus(main).textContent
  type('v2, gpu, new-tag')
  expect(
    serverStatus(main).textContent,
    'the Server save bar says nothing about the tags',
  ).toBe(clean)
})

test('12 T1.5: tags equal to instanceTags: no profile line', async () => {
  const { main } = await openPage({
    instances: itemsAnswer(() => [
      { ...TAG_ITEM, tags: ['v2', 'gpu'], instanceTags: ['v2', 'gpu'] },
    ]),
  })
  expect(
    within(main).queryByText(/^From the server profile/),
    'the profile brings nothing',
  ).toBeNull()
})

// ── T1.6, T1.7: the send

test('12 T1.6: the input is split on commas and white space, kept as typed, and an emptied box sends an empty list', async () => {
  const bodies: unknown[] = []
  const types: (string | null)[] = []
  const { bff } = await openPage({
    instanceTags: (body, _id, _n, init) => {
      bodies.push(body)
      types.push(new Headers(init?.headers).get('content-type'))
      return new Response(null, { status: 204 })
    },
  })
  type('v2,  gpu fast,,edge-1')
  fireEvent.click(tagSave())
  await waitFor(() => expect(bff.to('PUT', TAGS).length).toBe(1))
  expect(bodies[0], 'one PUT with the tags split').toEqual({
    tags: ['v2', 'gpu', 'fast', 'edge-1'],
  })
  expect(types[0], 'the body is JSON').toMatch(/application\/json/)
  submitForm()
  await waitFor(() => expect(bff.to('PUT', TAGS).length).toBe(2))
  expect(bodies[1], 'Enter in the box sends the same').toEqual({
    tags: ['v2', 'gpu', 'fast', 'edge-1'],
  })
  type('Fast')
  fireEvent.click(tagSave())
  await waitFor(() => expect(bff.to('PUT', TAGS).length).toBe(3))
  expect(bodies[2], 'the case is kept, the word is not touched').toEqual({
    tags: ['Fast'],
  })
  type('')
  fireEvent.click(tagSave())
  await waitFor(() => expect(bff.to('PUT', TAGS).length).toBe(4))
  expect(bodies[3], 'an emptied box clears the list').toEqual({ tags: [] })
  expect(bff.to('PUT', SERVER).length, 'the Server form is never sent').toBe(0)
})

test('12 T1.7: while the PUT is pending the status reads Saving tags, Save tags is aria-disabled, and a second click or Enter sends nothing', async () => {
  const h = held()
  let seen = 0
  await openPage({
    instanceTags: (_body, _id, _n, init) => {
      seen += 1
      return h.answer(init)
    },
  })
  type('new-tag')
  fireEvent.click(tagSave())
  await waitFor(() => expect(seen).toBe(1))
  expect(tagStatus().textContent, 'the status announces the send').toBe(
    en('instance-tags-saving'),
  )
  expect(
    tagSave().getAttribute('aria-disabled'),
    'Save tags reads as disabled',
  ).toBe('true')
  fireEvent.click(tagSave())
  submitForm()
  expect(seen, 'nothing else is sent').toBe(1)
  expect(tagInput().disabled, 'the box stays editable').toBe(false)
  h.resolve(new Response(null, { status: 204 }))
  await flushTasks()
  expect(tagStatus().textContent, 'the status carries on to the result').toBe(
    en('instance-tags-saved'),
  )
})

// ── T1.8, T1.9: the answers of a send

test('12 T1.8: on 204 the status reads Tags saved., the list is read again after the PUT, and the box keeps the text typed', async () => {
  const { bff } = await openPage()
  type('new-tag')
  fireEvent.click(tagSave())
  expect(await screen.findByText(en('instance-tags-saved'))).toBeTruthy()
  expect(tagStatus().textContent).toBe(en('instance-tags-saved'))
  await waitFor(() =>
    expect(bff.to('GET', '/api/instances').length).toBeGreaterThanOrEqual(2),
  )
  const calls = bff.calls
  const put = calls.findIndex((c) => c.method === 'PUT' && c.path === TAGS)
  const readAfter = calls.findIndex(
    (c, i) => i > put && c.method === 'GET' && c.path === '/api/instances',
  )
  expect(readAfter, 'the page asks a fresh read of the list').toBeGreaterThan(
    put,
  )
  expect(tagInput().value, 'the box keeps what was typed').toBe('new-tag')
})

test('12 T1.8: the read after a 204 answers the tags moved: the profile line follows it, the box keeps the text', async () => {
  let items: unknown[] = [TAG_ITEM]
  await openPage({
    instances: itemsAnswer(() => items),
    instanceTags: () => {
      items = [
        { ...TAG_ITEM, tags: ['edge', 'v2', 'gpu'], instanceTags: ['gpu'] },
      ]
      return new Response(null, { status: 204 })
    },
  })
  type('new-tag')
  fireEvent.click(tagSave())
  expect(
    await screen.findByText(en('instance-tags-profile', 'edge, v2')),
    'the profile line follows the fresh read',
  ).toBeTruthy()
  expect(tagInput().value, "the box is the user's").toBe('new-tag')
})

test("12 T1.9: a 422 refusal is the field's error: under the box, the box aria-invalid and described by it, focus in the box, the status empty, no alert; one keystroke clears it", async () => {
  const text = en(
    'api-server-bad-tags',
    'bad characters',
    'Fast',
    '8',
    '1',
    '32',
  )
  await openPage({
    instanceTags: refused(422, 'server-bad-tags', [
      'bad characters',
      'Fast',
      '8',
      '1',
      '32',
    ]),
  })
  type('Fast')
  fireEvent.click(tagSave())
  const error = await screen.findByText(text)
  const input = tagInput()
  expect(before(input, error), 'the text is under the box').toBe(true)
  expect(input.getAttribute('aria-invalid'), 'the box is invalid').toBe('true')
  expect(described(input), 'the box is described by the text').toContain(text)
  expect(document.activeElement, 'the focus moved to the box').toBe(input)
  expect(tagStatus().textContent, 'the status line is empty').toBe('')
  expect(within(tagForm()).queryAllByRole('alert'), 'no alert').toEqual([])
  expect(input.value, 'the box keeps its text').toBe('Fast')
  fireEvent.change(input, { target: { value: 'Fast fast' } })
  expect(
    input.getAttribute('aria-invalid'),
    'one keystroke clears the invalid mark',
  ).toBe(null)
  expect(described(input), 'and the error').not.toContain(text)
})

describe('12 T1.10: each other answer is the alert of the field, the box keeps its text, the status is empty', () => {
  const shapes: [string, () => Response | Promise<Response>, string][] = [
    ['401', SESSION_ENDED, en('session-ended-retry')],
    [
      '403 session-not-administrator',
      refused(403, 'session-not-administrator'),
      en('session-revoked'),
    ],
    [
      '404 client-no-instance',
      refused(404, 'client-no-instance'),
      en('instance-not-found', 'default'),
    ],
    [
      '503 client-unavailable',
      refused(503, 'client-unavailable'),
      en('api-client-unavailable'),
    ],
    [
      '403 client-not-authorized',
      refused(403, 'client-not-authorized'),
      en('api-client-not-authorized'),
    ],
    [
      '422 with an unknown key',
      refused(422, 'x-y', [], 'tags'),
      en('api-other', 'x-y'),
    ],
    [
      '400 tags-malformed',
      refused(400, 'tags-malformed'),
      en('request-failed'),
    ],
    ['502 client-other', refused(502, 'client-other'), en('request-failed')],
    [
      'a body that is not JSON',
      () => reply(500, 'not json'),
      en('request-failed'),
    ],
    [
      'a network failure',
      () => Promise.reject(new TypeError('Failed to fetch')),
      en('request-failed'),
    ],
  ]
  for (const [what, answer, text] of shapes) {
    test(`12 T1.10: ${what} -> ${text}`, async () => {
      await openPage({ instanceTags: answer })
      type('new-tag')
      fireEvent.click(tagSave())
      const el = await screen.findByText(text)
      const alert = el.closest('[role="alert"]')
      expect(alert, `${what}: the text is in a role="alert"`).not.toBeNull()
      expect(
        tagForm().contains(alert),
        `${what}: the alert is the field's`,
      ).toBe(true)
      expect(tagInput().value, `${what}: the box keeps its text`).toBe(
        'new-tag',
      )
      expect(tagStatus().textContent, `${what}: the status line is empty`).toBe(
        '',
      )
    })
  }
})

test('12 T1.10: the 401 and the 403 session-not-administrator give the field a Sign in button after its alert', async () => {
  for (const [answer, text] of [
    [SESSION_ENDED, en('session-ended-retry')],
    [refused(403, 'session-not-administrator'), en('session-revoked')],
  ] as Array<[() => Response, string]>) {
    await openPage({ instanceTags: answer })
    type('new-tag')
    fireEvent.click(tagSave())
    // The answer lands in a task of its own: the alert is the first thing the
    // field shows, and the Sign in is the element right after it.
    await screen.findByText(text)
    const signIns = within(tagForm()).getAllByRole('button', {
      name: en('session-sign-in'),
    })
    expect(signIns, 'one Sign in in the field').toHaveLength(1)
    expect(
      before(within(tagForm()).getAllByRole('alert')[0]!, signIns[0]!),
      'Sign in after the alert',
    ).toBe(true)
    cleanup()
    vi.unstubAllGlobals()
  }
})

// ── T1.11: the sign-in from the field

/** Types the password in the open sign-in dialog and submits it. */
async function signInInDialog(user: string) {
  const d = screen.getByRole('dialog', { name: en('signin-title') })
  expect(dialogs.opened, 'opened with showModal()').toContain(d)
  fireEvent.change(
    within(d).getByRole('textbox', { name: en('signin-username') }),
    {
      target: { value: user },
    },
  )
  fireEvent.change(
    within(d).getByLabelText(en('signin-password'), { selector: 'input' }),
    { target: { value: 'hunter2' } },
  )
  fireEvent.click(within(d).getByRole('button', { name: en('signin-submit') }))
  await flushTasks()
  expect(screen.queryByRole('dialog'), 'the dialog closed').toBeNull()
}

test('12 T1.11: Sign in opens the sign-in dialog; signing in as the same user closes it, removes the alert and puts focus on Save tags', async () => {
  await openPage({ instanceTags: SESSION_ENDED })
  type('new-tag')
  fireEvent.click(tagSave())
  await screen.findByText(en('session-ended-retry'))
  const sign = within(tagForm()).getByRole('button', {
    name: en('session-sign-in'),
  })
  fireEvent.click(sign)
  await flushTasks()
  await signInInDialog('alice')
  expect(tagAlerts(), 'the alert is gone').toEqual([])
  expect(
    within(tagForm()).queryAllByRole('button', { name: en('session-sign-in') }),
    'and its Sign in',
  ).toEqual([])
  expect(document.activeElement, 'focus on Save tags').toBe(tagSave())
  expect(tagInput().value, 'the text typed is kept').toBe('new-tag')
})

test('12 T1.11: Cancel closes the dialog and puts the focus back on Sign in', async () => {
  await openPage({ instanceTags: SESSION_ENDED })
  type('new-tag')
  fireEvent.click(tagSave())
  await screen.findByText(en('session-ended-retry'))
  const sign = within(tagForm()).getByRole('button', {
    name: en('session-sign-in'),
  })
  fireEvent.click(sign)
  await flushTasks()
  const d = screen.getByRole('dialog', { name: en('signin-title') })
  fireEvent.click(within(d).getByRole('button', { name: en('signin-cancel') }))
  await flushTasks()
  expect(screen.queryByRole('dialog'), 'the dialog closed').toBeNull()
  expect(document.activeElement, 'focus back on Sign in').toBe(
    within(tagForm()).getByRole('button', { name: en('session-sign-in') }),
  )
  expect(tagAlerts(), 'the alert is kept').toEqual([en('session-ended-retry')])
})

test('12 T1.11: signing in as a different account in the dialog puts the saved tags back in the box and leaves the page nothing to ask about, and the edits-dropped notice is true of both forms', async () => {
  let reads = 0
  const { bff, main } = await openPage({
    // The file the new account sees: its saved port is another one.
    server: () => reply(200, serverDoc({ Port: reads++ === 0 ? 8580 : 9000 })),
    instanceTags: SESSION_ENDED,
    signIn: () => reply(200, { user: 'carol' }),
  })
  type('new-tag')
  fireEvent.click(tagSave())
  await screen.findByText(en('session-ended-retry'))
  fireEvent.click(
    within(tagForm()).getByRole('button', { name: en('session-sign-in') }),
  )
  await flushTasks()
  await signInInDialog('carol')
  await waitFor(() =>
    expect(
      screen.getByRole('banner').textContent,
      'the session is a different account',
    ).toContain(en('session-signed-in-as', 'carol')),
  )
  // The notice says the form shows the saved settings. True of the Server
  // form: the section was read again, clean and editable (1b.36b).
  const dropped = await within(main).findByText(en('server-edits-dropped'))
  expect(dropped.closest('[role="alert"]'), 'role="alert"').not.toBeNull()
  await waitFor(() =>
    expect(portBox(main).value, 'the Server form shows the saved file').toBe(
      '9000',
    ),
  )
  expect(portBox(main).disabled, 'the Server form is editable').toBe(false)
  // And true of the tags field: its box holds the tags the page read, and the
  // field says nothing about saving.
  expect(
    tagInput().value,
    'a different account: the box goes back to the saved tags',
  ).toBe('v2, gpu')
  expect(tagAlerts(), "the field's alert went with the old session").toEqual([])
  expect(tagStatus().textContent, 'and its status line is empty').toBe('')
  expect(asksBrowser(), 'the page has no unsaved changes left').toBe(false)
  await goOverview()
  expect(
    screen.queryByRole('dialog', { name: en('discard-title') }),
    'leaving asks nothing',
  ).toBeNull()
  await screen.findByRole('heading', {
    level: 1,
    name: en('overview-title'),
  })
  expect(bff.to('PUT', TAGS).length, 'the refused write was not resent').toBe(1)
})

// ── T1.12: the 5 s read never touches the box

test('12 T1.12: the page read never changes the box: a poll answering other tags leaves the typed text', async () => {
  let items: unknown[] = [TAG_ITEM]
  const bff = fakeBff({
    instances: itemsAnswer(() => items),
    boot: { read: () => reply(200, { enabled: true }) },
  })
  fakeClock()
  render(<App />)
  await flush()
  within(screen.getByRole('main')).getByRole('button', {
    name: en('save-button'),
  })
  type('new-tag')
  items = [{ ...TAG_ITEM, instanceTags: ['other'] }]
  await advance(5_000)
  expect(
    tagInput().value,
    'the read of the page does not rewrite what was typed',
  ).toBe('new-tag')
  expect(
    bff.to('GET', '/api/instances').length,
    'the page read again',
  ).toBeGreaterThanOrEqual(2)
})

test('12 T1.12 (decision 7): a box nobody touched is filled from the first read, and a poll answering other tags keeps that first value', async () => {
  let items: unknown[] = [TAG_ITEM]
  const bff = fakeBff({
    instances: itemsAnswer(() => items),
    boot: { read: () => reply(200, { enabled: true }) },
  })
  fakeClock()
  render(<App />)
  await flush()
  const main = within(screen.getByRole('main'))
  main.getByRole('button', { name: en('save-button') })
  // Nothing typed: the box holds what the page opened with.
  expect(tagInput().value, 'the first read fills the box').toBe('v2, gpu')
  items = [{ ...TAG_ITEM, instanceTags: ['other'] }]
  await advance(5_000)
  expect(
    tagInput().value,
    'the box is filled by the first read: a later read never rewrites it',
  ).toBe('v2, gpu')
  // The reads do move the rest of the header: the profile line is the union
  // less the box list, so with the list the file now reports, the line shows
  // every tag of the union (decision 7).
  expect(
    main.getByText(en('instance-tags-profile', 'edge, v2, gpu')).textContent,
    'the profile line follows the read',
  ).toBe(en('instance-tags-profile', 'edge, v2, gpu'))
  expect(
    bff.to('GET', '/api/instances').length,
    'the page read again',
  ).toBeGreaterThanOrEqual(2)
})

// ── T1.13, T1.14: unsaved tags guard the page

/** The browser's Back button: jsdom traverses in a task. */
async function pressBack() {
  await act(async () => {
    window.history.back()
    await new Promise((r) => setTimeout(r, 0))
    await new Promise((r) => setTimeout(r, 0))
  })
}

/** A nav link. */
function navLink(name: string): HTMLElement {
  return within(
    screen.getByRole('navigation', { name: en('nav-label') }),
  ).getByRole('link', { name })
}

/** Clicks the nav's Overview link, so the router sees it (12 T1.13). */
async function goOverview() {
  await act(async () => {
    fireEvent.click(navLink(en('nav-overview')))
    await new Promise((r) => setTimeout(r, 0))
  })
}

/** The Unsaved changes dialog of the Server form, open. */
async function discardDialog(): Promise<HTMLElement> {
  const d = await screen.findByRole('dialog', { name: en('discard-title') })
  expect(dialogs.opened, 'opened with showModal()').toContain(d)
  return d
}

/** Types a hash, so the browser gains a history entry (the LeaveGuard shape). */
async function typeHash(hash: string) {
  await act(async () => {
    window.location.hash = hash
    await new Promise((r) => setTimeout(r, 0))
  })
}

/**
 * The page reached by a typed hash from the Overview, so the browser has an
 * entry to go Back to (the shape LeaveGuard 1b.38b uses).
 */
async function openPageFromOverview(options: BffOptions = {}) {
  startAt('#/')
  const bff = fakeBff({
    instances: [TAG_ITEM],
    boot: { read: () => reply(200, { enabled: true }) },
    ...options,
  })
  render(<App />)
  const main = screen.getByRole('main')
  await within(main).findByRole('heading', {
    level: 1,
    name: en('overview-title'),
  })
  await typeHash('#/instances/default')
  await screen.findByRole('heading', { level: 1, name: 'default' })
  return { bff, main }
}

test("12 T1.13: unsaved tags guard the page: a nav link and the browser's Back open the Unsaved changes dialog, and a beforeunload is cancelled", async () => {
  await openPageFromOverview()
  expect(asksBrowser(), 'nothing typed: the browser is not asked').toBe(false)
  type('new-tag')
  expect(asksBrowser(), 'a beforeunload is cancelled').toBe(true)
  await goOverview()
  const d = await discardDialog()
  expect(window.location.hash, 'the address stayed on the instance').toBe(
    '#/instances/default',
  )
  expect(tagInput().value, 'the text is kept').toBe('new-tag')
  fireEvent.click(within(d).getByRole('button', { name: en('discard-keep') }))
  await flushTasks()
  expect(asksBrowser(), 'still unsaved: the browser is still asked').toBe(true)
  await pressBack()
  const back = await discardDialog()
  fireEvent.click(
    within(back).getByRole('button', { name: en('discard-confirm') }),
  )
  await screen.findByRole('heading', { level: 1, name: en('overview-title') })
})

test('12 T1.13: typing the text back to the list read makes leaving ask nothing again', async () => {
  await openPage()
  type('new-tag')
  expect(asksBrowser(), 'unsaved: the browser is asked').toBe(true)
  type('v2,gpu')
  expect(
    asksBrowser(),
    'the same list as read (v2,gpu is v2, gpu): nothing is asked',
  ).toBe(false)
  await goOverview()
  expect(
    screen.queryByRole('dialog', { name: en('discard-title') }),
    'the nav goes through',
  ).toBeNull()
  await screen.findByRole('heading', { level: 1, name: en('overview-title') })
})

test('12 T1.14: after a 204 leaving asks nothing; after a refusal it still asks; the Server save bar keeps its status and buttons', async () => {
  const { bff, main } = await openPage()
  const cleanStatus = serverStatus(main).textContent
  const cleanDiscard = within(main)
    .getByRole('button', { name: en('save-discard') })
    .hasAttribute('disabled')
  type('new-tag')
  expect(asksBrowser(), 'unsaved: asked').toBe(true)
  fireEvent.click(tagSave())
  await screen.findByText(en('instance-tags-saved'))
  expect(asksBrowser(), 'saved: nothing is asked').toBe(false)
  expect(serverStatus(main).textContent, 'the Server bar said nothing').toBe(
    cleanStatus,
  )
  await goOverview()
  await screen.findByRole('heading', { level: 1, name: en('overview-title') })
  expect(bff.to('PUT', TAGS).length).toBe(1)
  expect(bff.to('PUT', SERVER).length).toBe(0)

  // A refusal: the tags stay unsaved, and the Server bar is untouched. A 204
  // left the page clean, so the Overview above really went: put the address
  // back on the instance before the second mount.
  cleanup()
  startAt('#/instances/default')
  await flushTasks()
  const second = await openPage({
    instanceTags: refused(422, 'server-bad-tags', [
      'too long',
      'x',
      '8',
      '1',
      '32',
    ]),
  })
  type('new-tag')
  fireEvent.click(tagSave())
  await screen.findByText(
    en('api-server-bad-tags', 'too long', 'x', '8', '1', '32'),
  )
  expect(asksBrowser(), 'refused: the browser is still asked').toBe(true)
  expect(
    serverStatus(second.main).textContent,
    'the Server save bar status is unchanged',
  ).toBe(cleanStatus)
  expect(
    within(second.main)
      .getByRole('button', { name: en('save-discard') })
      .hasAttribute('disabled'),
    'the Server Discard button is as it was',
  ).toBe(cleanDiscard)
})
