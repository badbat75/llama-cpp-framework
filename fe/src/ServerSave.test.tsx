// Editing and saving the Server form (fe/briefs/01-server.md, slice 1b:
// section 3, with sections 5.2 to 5.4 and 6): the controls an administrator
// may change, their dependencies, the save bar, the client checks, the PUT
// and every answer to it, and leaving with unsaved changes. The whole app
// renders at #/instances/default against a fake bff (mocked fetch); "1b.31"
// is the brief's criterion 31. A user's read-only form is 1a's
// (InstancePage.test.tsx).
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
  isHidden,
  reply,
  serverDoc,
  startAt,
  stubDialog,
  type BffOptions,
} from './test/bff.ts'

let dialogs: ReturnType<typeof stubDialog>

beforeEach(() => {
  startAt('#/instances/default')
  dialogs = stubDialog()
})

afterEach(async () => {
  // A link click queues its hashchange as a task: let it land on this test's
  // page, not on the next test's.
  await act(async () => {
    await new Promise((r) => setTimeout(r, 0))
  })
  cleanup()
  vi.unstubAllGlobals()
  dialogs.restore()
  startAt('')
})

/** What a save of serverDoc() sends when nothing was changed (5.4). */
const UNCHANGED = {
  Port: 8580,
  Hostname: 'localhost,10.0.0.9,192.168.1.20',
  OpencodeBaseUrl: null,
  OpencodeApiKey: 'sk-local-123',
  Threads: null,
  ThreadsBatch: 16,
  ModelsMax: null,
  ModelsMemoryMargin: 1024,
  ModelsDir: null,
  Webui: true,
  WebuiMcpProxy: false,
  LogVerbosity: 3,
  SaveStateOnShutdown: true,
  StateDir: '/data/llama-state',
  RocblasUseHipblaslt: 'off',
  RocprofAttach: false,
  CudaAllreduce: 'nccl',
  CudaDisableGraphs: false,
  AttnRotDisable: false,
}

const REVISION = 'r-1731628800-42'
const PUT = ['PUT', '/api/instances/default/server'] as const

/** The page with the server form loaded for `options`; admin by default. */
async function loaded(options: BffOptions = {}) {
  const bff = fakeBff(options)
  render(<App />)
  const main = screen.getByRole('main')
  await within(main).findByRole('heading', {
    level: 3,
    name: en('server-network'),
  })
  return { bff, main }
}

function group(main: HTMLElement, name: string) {
  return within(within(main).getByRole('group', { name }))
}

/** The text input of a field: in its group, or labelled (Models, Snapshot folder). */
function box(main: HTMLElement, label: string): HTMLInputElement {
  const g = within(main).queryByRole('group', { name: label })
  return (
    g
      ? within(g).getByRole('textbox', { name: label })
      : within(main).getByRole('textbox', { name: label })
  ) as HTMLInputElement
}

function check(main: HTMLElement, field: string, name: string) {
  return group(main, field).getByRole('checkbox', {
    name,
  }) as HTMLInputElement
}

function apiKey(main: HTMLElement): HTMLInputElement {
  return within(main).getByLabelText(en('server-api-key'), {
    selector: 'input',
  }) as HTMLInputElement
}

function show(main: HTMLElement): HTMLElement {
  return group(main, 'API key').getByRole('button', { name: en('field-show') })
}

function select(main: HTMLElement, name: string): HTMLSelectElement {
  return within(main).getByRole('combobox', { name }) as HTMLSelectElement
}

function type(input: HTMLElement, value: string) {
  fireEvent.change(input, { target: { value } })
}

function saveButton(main: HTMLElement): HTMLElement {
  return within(main).getByRole('button', { name: en('save-button') })
}

function discardButton(main: HTMLElement): HTMLElement {
  return within(main).getByRole('button', { name: en('save-discard') })
}

/** The save bar's status line: the one live status of a loaded form. */
function status(main: HTMLElement): HTMLElement {
  const all = within(main).getAllByRole('status')
  expect(all, 'one status line once the form is loaded').toHaveLength(1)
  return all[0]!
}

/** True when the control cannot be used: disabled, or aria-disabled. */
function inactive(el: HTMLElement): boolean {
  return (
    (el as HTMLButtonElement).disabled ||
    el.getAttribute('aria-disabled') === 'true'
  )
}

/** The texts of the elements the control's aria-describedby names. */
function described(el: HTMLElement): string[] {
  return (el.getAttribute('aria-describedby') ?? '')
    .split(/\s+/)
    .filter((id) => id !== '')
    .map((id) => document.getElementById(id)?.textContent ?? `(no #${id})`)
}

/** Asserts `el` is marked invalid with the error `text` under it. */
function expectError(el: HTMLElement, text: string, what: string) {
  expect(el.getAttribute('aria-invalid'), `${what} aria-invalid`).toBe('true')
  expect(described(el), `${what} aria-describedby`).toContain(text)
  // The element named by aria-describedby that holds the text (two fields
  // can show the same text, so it is found by id, not by text).
  const error = (el.getAttribute('aria-describedby') ?? '')
    .split(/\s+/)
    .map((id) => document.getElementById(id))
    .find((e) => e !== null && e.textContent === text)
  expect(error, `${what}: the error is shown`).toBeDefined()
  expect(isHidden(error!), `${what}: the error is shown`).toBe(false)
  expect(before(el, error!), `${what}: the error comes after the control`).toBe(
    true,
  )
}

function expectNoError(el: HTMLElement, what: string) {
  expect(el.getAttribute('aria-invalid') ?? 'false', what).toBe('false')
}

/**
 * The bar's alert reads `text` (exact): a role="alert" of its own, after the
 * status line and before the buttons (30a), the status line not holding it.
 */
function expectAlert(main: HTMLElement, text: string, what = text) {
  const el = within(main).getByText(text)
  const alert = el.closest('[role="alert"]')
  expect(alert, `${what} is in role="alert"`).not.toBe(null)
  const line = status(main)
  expect(line.contains(alert), `${what}: not inside the status line`).toBe(
    false,
  )
  expect(
    alert!.contains(line),
    `${what}: the alert does not hold the status line`,
  ).toBe(false)
  expect(before(line, alert!), `${what}: after the status line`).toBe(true)
  expect(before(alert!, discardButton(main)), `${what}: before Discard`).toBe(
    true,
  )
}

/** The bar's alert is not rendered (30a: only while it has text). */
function expectNoAlert(main: HTMLElement, what: string) {
  expect(
    within(main)
      .queryAllByRole('alert')
      .map((a) => a.textContent),
    what,
  ).toEqual([])
}

/** Clicks Save and waits until the PUT was sent (or `n` of them). */
async function save(main: HTMLElement, bff: ReturnType<typeof fakeBff>, n = 1) {
  fireEvent.click(saveButton(main))
  await waitFor(() => expect(bff.to(...PUT)).toHaveLength(n))
}

/** A dirty form: Port changed from 8580 to 8581. */
function editPort(main: HTMLElement) {
  type(box(main, 'Port'), '8581')
}

function dialog(name: string): HTMLElement {
  const d = screen.getByRole('dialog', { name })
  expect(d.tagName, 'a native <dialog>').toBe('DIALOG')
  expect(dialogs.opened, 'opened with showModal()').toContain(d)
  return d
}

// ── Failure paths first: client checks (1b.31) ──────────────────────────

test('1b.31: failing checks send nothing, mark each field, focus the first and alert in the bar', async () => {
  const { bff, main } = await loaded()
  type(box(main, 'Port'), 'abc')
  type(box(main, 'Batch threads'), '33')
  type(box(main, 'Models folder'), '/srv/a;b')
  fireEvent.click(saveButton(main))
  await within(main).findByText(en('save-invalid'))
  expect(bff.to(...PUT)).toHaveLength(0)
  expectError(box(main, 'Port'), en('field-error-number', '1', '65535'), 'Port')
  expectError(
    box(main, 'Batch threads'),
    en('field-error-number', '1', '32'),
    'Batch threads',
  )
  expectError(
    box(main, 'Models folder'),
    en('field-error-comment-markers'),
    'Models folder',
  )
  expect(document.activeElement).toBe(box(main, 'Port'))
  expectAlert(main, en('save-invalid'))
  // The status line says only the form's state: still unsaved (30a).
  expect(status(main).textContent).toBe(en('save-unsaved'))
  // Fields that pass are not marked.
  expectNoError(box(main, 'Memory margin'), 'Memory margin')
})

test('1b.31: editing a field clears its error and leaves the others', async () => {
  const { main } = await loaded()
  type(box(main, 'Port'), 'abc')
  type(box(main, 'Models folder'), '/srv/a;b')
  fireEvent.click(saveButton(main))
  await within(main).findByText(en('save-invalid'))
  type(box(main, 'Port'), '8581')
  expectNoError(box(main, 'Port'), 'Port after the edit')
  expect(
    described(box(main, 'Port')),
    'no error text named by Port any more',
  ).not.toContain(en('field-error-number', '1', '65535'))
  expect(
    within(main).queryByText(en('field-error-number', '1', '65535')),
  ).toBeNull()
  expectError(
    box(main, 'Models folder'),
    en('field-error-comment-markers'),
    'Models folder still',
  )
  // Editing clears only the field's error, not the bar's alert (30a).
  expectAlert(main, en('save-invalid'), 'the alert after an edit')
})

test('1b.31 (5.4): numbers must be whole and in limits; max 2147483647 says only the minimum', async () => {
  const { bff, main } = await loaded()
  // Max models starts on default: untick it to check its text.
  fireEvent.click(check(main, 'Max models', en('field-default')))
  type(box(main, 'Max models'), '1.5')
  type(box(main, 'Memory margin'), '-1')
  type(box(main, 'Port'), '1e3')
  type(box(main, 'Batch threads'), '+4')
  fireEvent.click(saveButton(main))
  await within(main).findByText(en('save-invalid'))
  expect(bff.to(...PUT)).toHaveLength(0)
  expectError(box(main, 'Port'), en('field-error-number', '1', '65535'), 'Port')
  expectError(
    box(main, 'Batch threads'),
    en('field-error-number', '1', '32'),
    'Batch threads',
  )
  expectError(
    box(main, 'Max models'),
    en('field-error-number-min', '0'),
    'Max models',
  )
  expectError(
    box(main, 'Memory margin'),
    en('field-error-number-min', '0'),
    'Memory margin',
  )
  expect(document.activeElement).toBe(box(main, 'Port'))
})

test('1b.31 (5.2): an unticked auto leaves the thread box empty and focused, and empty fails the check', async () => {
  const { bff, main } = await loaded()
  fireEvent.click(check(main, 'CPU threads', en('field-auto')))
  const threads = box(main, 'CPU threads')
  expect(threads.disabled).toBe(false)
  expect(threads.value).toBe('')
  expect(document.activeElement).toBe(threads)
  fireEvent.click(saveButton(main))
  await within(main).findByText(en('save-invalid'))
  expect(bff.to(...PUT)).toHaveLength(0)
  expectError(threads, en('field-error-number', '1', '32'), 'CPU threads')
  expect(document.activeElement).toBe(threads)
})

test('1b.31 (5.4): ; and # are refused in Base URL, API key and Snapshot folder; focus on the first', async () => {
  const { bff, main } = await loaded()
  type(box(main, 'Snapshot folder'), '/data/a#b')
  type(apiKey(main), 'sk;1')
  fireEvent.click(check(main, 'Base URL', en('field-default')))
  type(box(main, 'Base URL'), 'http://gw.example.net/#v1')
  fireEvent.click(saveButton(main))
  await within(main).findByText(en('save-invalid'))
  expect(bff.to(...PUT)).toHaveLength(0)
  const markers = en('field-error-comment-markers')
  expectError(box(main, 'Base URL'), markers, 'Base URL')
  expectError(apiKey(main), markers, 'API key')
  expectError(box(main, 'Snapshot folder'), markers, 'Snapshot folder')
  expect(document.activeElement).toBe(box(main, 'Base URL'))
})

test('1b.31 (5.4): a field on default, auto, No key or disabled by its switch is not checked', async () => {
  const { bff, main } = await loaded()
  type(box(main, 'Port'), 'abc')
  fireEvent.click(check(main, 'Port', en('field-default')))
  type(apiKey(main), 'sk;1')
  fireEvent.click(check(main, 'API key', en('field-no-key')))
  type(box(main, 'Batch threads'), 'many')
  fireEvent.click(check(main, 'Batch threads', en('field-auto')))
  type(box(main, 'Snapshot folder'), '/data/a#b')
  fireEvent.click(
    check(main, 'Save on shutdown', en('server-save-on-shutdown-toggle')),
  )
  await save(main, bff)
  expect(within(main).queryByText(en('save-invalid'))).toBeNull()
  expect(bff.to(...PUT)[0]!.body).toEqual({
    revision: REVISION,
    values: {
      ...UNCHANGED,
      Port: null,
      OpencodeApiKey: null,
      ThreadsBatch: null,
      SaveStateOnShutdown: false,
      // Disabled by its switch: kept and sent as it is (5.3).
      StateDir: '/data/a#b',
    },
  })
})

// ── Failure paths: answers to the save (1b.34 to 1b.37) ─────────────────

test('1b.34: a 422 naming a field shows its text under that field, focuses it, and alerts in the bar', async () => {
  const { bff, main } = await loaded({
    save: () =>
      reply(422, {
        key: 'client-address-in-use',
        args: ['coder'],
        field: 'Port',
      }),
  })
  editPort(main)
  await save(main, bff)
  const text = en('api-client-address-in-use', 'coder')
  await within(main).findByText(en('save-refused', text))
  expectError(box(main, 'Port'), text, 'Port')
  expect(document.activeElement).toBe(box(main, 'Port'))
  expectAlert(main, en('save-refused', text))
  // The edit stays, and the status line says so (30a).
  expect(box(main, 'Port').value).toBe('8581')
  expect(status(main).textContent).toBe(en('save-unsaved'))
})

test('1b.34: a 422 on ModelsDir lands under Models folder, its args filling the text', async () => {
  const { bff, main } = await loaded({
    save: () =>
      reply(422, {
        key: 'comment-markers',
        args: ['ModelsDir'],
        field: 'ModelsDir',
      }),
  })
  type(box(main, 'Models folder'), '/srv/models')
  await save(main, bff)
  const text = en('api-comment-markers', 'ModelsDir')
  await within(main).findByText(en('save-refused', text))
  expectError(box(main, 'Models folder'), text, 'Models folder')
  expect(document.activeElement).toBe(box(main, 'Models folder'))
})

test('1b.34 (5.6): a 422 without field goes to the field of the fallback table', async () => {
  const cases: [string, Record<string, unknown>, string, string][] = [
    [
      'not-whole-number-in-range: args[0]',
      {
        key: 'not-whole-number-in-range',
        args: ['ModelsMemoryMargin', '99999', '0', '2147483647'],
      },
      'Memory margin',
      en(
        'api-not-whole-number-in-range',
        'ModelsMemoryMargin',
        '99999',
        '0',
        '2147483647',
      ),
    ],
    [
      'comment-markers: args[0]',
      { key: 'comment-markers', args: ['StateDir'] },
      'Snapshot folder',
      en('api-comment-markers', 'StateDir'),
    ],
    [
      'load-on-startup-over-max: Max models',
      { key: 'load-on-startup-over-max', args: ['3', '2'] },
      'Max models',
      en('api-load-on-startup-over-max', '3', '2'),
    ],
    [
      'load-on-startup-unlimited: Max models',
      { key: 'load-on-startup-unlimited', args: ['2'] },
      'Max models',
      en('api-load-on-startup-unlimited', '2'),
    ],
    [
      'client-address-in-use: Port',
      { key: 'client-address-in-use', args: ['coder'] },
      'Port',
      en('api-client-address-in-use', 'coder'),
    ],
  ]
  for (const [what, body, field, text] of cases) {
    const { bff, main } = await loaded({
      // Max models set, so its textbox is enabled and can take the focus.
      server: () => reply(200, serverDoc({ ModelsMax: 2 })),
      save: () => reply(422, body),
    })
    fireEvent.click(check(main, 'KV rotation', en('server-kv-rotation-toggle')))
    await save(main, bff)
    await within(main).findByText(en('save-refused', text))
    expectError(box(main, field), text, what)
    expect(document.activeElement, what).toBe(box(main, field))
    expectAlert(main, en('save-refused', text), what)
    cleanup()
    vi.unstubAllGlobals()
  }
})

test('1b.34 (5.6): a 422 client-unresolvable-host without field shows its text in Bind to', async () => {
  const { bff, main } = await loaded({
    save: () =>
      reply(422, { key: 'client-unresolvable-host', args: ['gpu-box.lan'] }),
  })
  editPort(main)
  await save(main, bff)
  const text = en('api-client-unresolvable-host', 'gpu-box.lan')
  await within(main).findByText(en('save-refused', text))
  const bind = within(main).getByRole('group', { name: 'Bind to' })
  const error = within(bind).getByText(text)
  expect(isHidden(error)).toBe(false)
  const rows = within(bind).getAllByRole('checkbox')
  expect(before(rows[rows.length - 1]!, error)).toBe(true)
  expectAlert(main, en('save-refused', text))
})

test('1b.34: a 422 on a field the form lacks, or with none and no fallback, shows only the bar', async () => {
  const cases: [string, Record<string, unknown>, string][] = [
    [
      'unknown field',
      { key: 'comment-markers', args: ['NoSuchKey'], field: 'NoSuchKey' },
      en('save-refused', en('api-comment-markers', 'NoSuchKey')),
    ],
    [
      'unknown key',
      { key: 'frobnicated', args: ['x'] },
      en('save-refused', en('api-other', 'frobnicated')),
    ],
  ]
  for (const [what, body, text] of cases) {
    const { bff, main } = await loaded({ save: () => reply(422, body) })
    editPort(main)
    await save(main, bff)
    await within(main).findByText(text)
    expectAlert(main, text, what)
    const invalid = [...main.querySelectorAll('[aria-invalid="true"]')]
    expect(
      invalid.map((e) => e.outerHTML),
      `${what}: no field marked`,
    ).toEqual([])
    expect(document.body.textContent, what).not.toContain(en('api-other', 'x'))
    cleanup()
    vi.unstubAllGlobals()
  }
})

test('1b.35: 409 shows server-changed-elsewhere with Reload; the edits stay until Reload is confirmed', async () => {
  let reads = 0
  const { bff, main } = await loaded({
    server: () =>
      ++reads === 1
        ? reply(200, serverDoc())
        : reply(200, { ...serverDoc({ Port: 9000 }), revision: 'r-9' }),
    save: (_body, n) =>
      n === 1
        ? reply(409, { key: 'server-changed-elsewhere', args: [] })
        : reply(200, { revision: 'r-10', running: false }),
  })
  editPort(main)
  await save(main, bff)
  await within(main).findByText(en('api-server-changed-elsewhere'))
  expectAlert(main, en('api-server-changed-elsewhere'))
  expect(box(main, 'Port').value).toBe('8581')
  const reload = within(main).getByRole('button', { name: en('save-reload') })

  // Reload asks first; Keep editing leaves everything as it is.
  fireEvent.click(reload)
  let ask = dialog(en('discard-title'))
  fireEvent.click(within(ask).getByRole('button', { name: en('discard-keep') }))
  expect(screen.queryByRole('dialog')).toBeNull()
  expect(bff.to('GET', '/api/instances/default/server')).toHaveLength(1)
  expect(box(main, 'Port').value).toBe('8581')

  // Discard changes reads the file again and shows it, clean.
  fireEvent.click(within(main).getByRole('button', { name: en('save-reload') }))
  ask = dialog(en('discard-title'))
  fireEvent.click(
    within(ask).getByRole('button', { name: en('discard-confirm') }),
  )
  await waitFor(() =>
    expect(bff.to('GET', '/api/instances/default/server')).toHaveLength(2),
  )
  await waitFor(() => expect(box(main, 'Port').value).toBe('9000'))
  expect(saveButton(main).getAttribute('aria-disabled')).toBe('true')
  // A confirmed Reload ends the alert; the form is clean (30a).
  expectNoAlert(main, 'after Reload')
  expect(status(main).textContent).toBe('')
  // The next save carries the new revision.
  editPort(main)
  await save(main, bff, 2)
  expect(bff.to(...PUT)[1]!.body).toMatchObject({ revision: 'r-9' })
})

test('1b.36: a 401 on save offers Sign in in a dialog; after it the edits are intact and nothing is resent', async () => {
  const { bff, main } = await loaded({
    save: (_body, n) =>
      n === 1
        ? reply(401, { key: 'session-required', args: [] })
        : reply(200, { revision: 'r-2', running: false }),
  })
  editPort(main)
  fireEvent.click(check(main, 'Web UI', en('server-web-ui-toggle')))
  await save(main, bff)
  const ended = await within(main).findByText(en('session-ended'))
  expect(ended.closest('[role="alert"]')).not.toBeNull()
  // The page and its edits stay: no sign-in form in place of the page.
  expect(
    within(main).getByRole('heading', { level: 1, name: 'default' }),
  ).toBeTruthy()
  expect(box(main, 'Port').value).toBe('8581')
  const signIn = within(main).getByRole('button', {
    name: en('session-sign-in'),
  })
  fireEvent.click(signIn)

  const d = dialog(en('signin-title'))
  expect(
    within(d).getByRole('heading', { level: 2, name: en('signin-title') }),
  ).toBeTruthy()
  expect(within(d).queryByRole('heading', { level: 1 })).toBeNull()
  // The page's h1 is still the one h1.
  expect(screen.getAllByRole('heading', { level: 1 })).toHaveLength(1)
  type(within(d).getByRole('textbox', { name: en('signin-username') }), 'alice')
  type(
    within(d).getByLabelText(en('signin-password'), { selector: 'input' }),
    'hunter2',
  )
  fireEvent.click(within(d).getByRole('button', { name: en('signin-submit') }))
  await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull())
  expect(bff.to('POST', '/api/session')[0]!.body).toEqual({
    username: 'alice',
    password: 'hunter2',
  })
  expect(document.activeElement).toBe(saveButton(main))
  expect(box(main, 'Port').value).toBe('8581')
  expect(check(main, 'Web UI', en('server-web-ui-toggle')).checked).toBe(false)
  expect(bff.to(...PUT)).toHaveLength(1)
  expect(bff.to('GET', '/api/instances/default/server')).toHaveLength(1)
  // The sign-in ends the alert; the edits are still unsaved (30a).
  expectNoAlert(main, 'after the sign-in')
  expect(status(main).textContent).toBe(en('save-unsaved'))
  expect(within(main).queryByText(en('server-edits-dropped'))).toBeNull()
  // Saving again is the user's choice, and sends the same revision and edits.
  await save(main, bff, 2)
  expect(bff.to(...PUT)[1]!.body).toEqual({
    revision: REVISION,
    values: { ...UNCHANGED, Port: 8581, Webui: false },
  })
})

test('1b.36: a failed sign-in in the dialog keeps it open with its alert and the edits', async () => {
  const { bff, main } = await loaded({
    save: () => reply(401, { key: 'session-required', args: [] }),
    signIn: () => reply(401, { key: 'session-bad-credentials', args: [] }),
  })
  editPort(main)
  await save(main, bff)
  fireEvent.click(
    await within(main).findByRole('button', { name: en('session-sign-in') }),
  )
  const d = dialog(en('signin-title'))
  type(within(d).getByRole('textbox', { name: en('signin-username') }), 'alice')
  type(
    within(d).getByLabelText(en('signin-password'), { selector: 'input' }),
    'wrong',
  )
  fireEvent.click(within(d).getByRole('button', { name: en('signin-submit') }))
  const alert = await within(d).findByText(en('api-session-bad-credentials'))
  expect(alert.closest('[role="alert"]')).not.toBeNull()
  expect(screen.getByRole('dialog', { name: en('signin-title') })).toBe(d)
  expect(box(main, 'Port').value).toBe('8581')
  expect(bff.to(...PUT)).toHaveLength(1)
})

test('1b.37: the key picks the message, not the status; every failure keeps the edits, never the body', async () => {
  const answers: [string, () => Response | Promise<Response>, string][] = [
    [
      '403',
      () => reply(403, { key: 'client-not-authorized', args: [] }),
      en('api-client-not-authorized'),
    ],
    [
      '503',
      () => reply(503, { key: 'client-unavailable', args: [] }),
      en('api-client-unavailable'),
    ],
    // By key: the status only says it is an error.
    [
      '403 carrying client-unavailable',
      () => reply(403, { key: 'client-unavailable', args: [] }),
      en('api-client-unavailable'),
    ],
    [
      '503 carrying client-not-authorized',
      () => reply(503, { key: 'client-not-authorized', args: [] }),
      en('api-client-not-authorized'),
    ],
    [
      '409 carrying client-unavailable',
      () => reply(409, { key: 'client-unavailable', args: [] }),
      en('api-client-unavailable'),
    ],
    [
      '409 with an unknown key',
      () => reply(409, { key: 'frobnicated', args: [] }),
      en('save-failed'),
    ],
    [
      '200 without running',
      () => reply(200, { revision: 'r-2' }),
      en('save-failed'),
    ],
    ['204', () => reply(204), en('save-failed')],
    [
      '200 with a revision that is not a string',
      () => reply(200, { revision: 2, running: false }),
      en('save-failed'),
    ],
    [
      '500 with a key',
      () => reply(500, { key: 'secret-detail', args: [] }),
      en('save-failed'),
    ],
    [
      'network',
      () => Promise.reject(new TypeError('Failed to fetch')),
      en('save-failed'),
    ],
    ['422 not json', () => reply(422, 'secret-detail'), en('save-failed')],
    ['422 without key', () => reply(422, {}), en('save-failed')],
    ['200 not json', () => reply(200, 'secret-detail'), en('save-failed')],
  ]
  for (const [what, answer, text] of answers) {
    const { bff, main } = await loaded({ save: answer })
    editPort(main)
    await save(main, bff)
    await within(main).findByText(text)
    expectAlert(main, text, what)
    expect(document.body.textContent, what).not.toContain('secret-detail')
    expect(box(main, 'Port').value, what).toBe('8581')
    expect(
      saveButton(main).getAttribute('aria-disabled') ?? 'false',
      `${what}: still dirty, Save enabled`,
    ).toBe('false')
    expect(within(main).queryByText(en('save-saved')), what).toBeNull()
    expect(status(main).textContent, `${what}: status (30a)`).toBe(
      en('save-unsaved'),
    )
    expect(
      within(main).queryByRole('button', { name: en('save-reload') }),
      `${what}: no Reload`,
    ).toBeNull()
    // Still signed in: the banner keeps Sign out.
    expect(
      within(screen.getByRole('banner')).getByRole('button', {
        name: en('session-sign-out'),
      }),
      what,
    ).toBeTruthy()
    cleanup()
    vi.unstubAllGlobals()
  }
})

test('1b reading order of a reply: 401 first, then server-changed-elsewhere, the client-* keys, a check key', async () => {
  const markers = en('api-comment-markers', 'ModelsDir')
  const cases: [
    string,
    () => Response,
    string,
    { reload: boolean; field?: string },
  ][] = [
    [
      '401 carrying server-changed-elsewhere',
      () => reply(401, { key: 'server-changed-elsewhere', args: [] }),
      en('session-ended'),
      { reload: false },
    ],
    [
      '409 carrying client-not-authorized',
      () => reply(409, { key: 'client-not-authorized', args: [] }),
      en('api-client-not-authorized'),
      { reload: false },
    ],
    [
      '422 carrying server-changed-elsewhere',
      () => reply(422, { key: 'server-changed-elsewhere', args: [] }),
      en('api-server-changed-elsewhere'),
      { reload: true },
    ],
    [
      '500 carrying client-no-instance',
      () => reply(500, { key: 'client-no-instance', args: [] }),
      en('instance-not-found', 'default'),
      { reload: false },
    ],
    [
      '400 carrying a check key',
      () =>
        reply(400, {
          key: 'comment-markers',
          args: ['ModelsDir'],
          field: 'ModelsDir',
        }),
      en('save-refused', markers),
      { reload: false, field: 'Models folder' },
    ],
  ]
  for (const [what, answer, text, { reload, field }] of cases) {
    const { bff, main } = await loaded({ save: answer })
    type(box(main, 'Models folder'), '/srv/models')
    await save(main, bff)
    await within(main).findByText(text)
    expectAlert(main, text, what)
    expect(
      within(main).queryByRole('button', { name: en('save-reload') }) !== null,
      `${what}: Reload`,
    ).toBe(reload)
    if (field !== undefined) expectError(box(main, field), markers, what)
    expect(box(main, 'Models folder').value, what).toBe('/srv/models')
    cleanup()
    vi.unstubAllGlobals()
  }
})

test('1b.37a: client-no-instance on the PUT says so in the bar; the form and its edits stay, dirty', async () => {
  const { bff, main } = await loaded({
    save: () => reply(404, { key: 'client-no-instance', args: [] }),
  })
  editPort(main)
  await save(main, bff)
  const text = en('instance-not-found', 'default')
  await within(main).findByText(text)
  expectAlert(main, text)
  // The page is not replaced: header, section and form stay.
  expect(
    within(main).getByRole('heading', { level: 1, name: 'default' }),
  ).toBeTruthy()
  expect(
    within(main).getByRole('heading', { level: 2, name: en('server-title') }),
  ).toBeTruthy()
  expect(
    within(main).queryByRole('link', { name: en('not-found-home') }),
  ).toBeNull()
  expect(box(main, 'Port').value).toBe('8581')
  expect(status(main).textContent).toBe(en('save-unsaved'))
  expect(saveButton(main).getAttribute('aria-disabled') ?? 'false').toBe(
    'false',
  )
  // A new Save asks again.
  await save(main, bff, 2)
})

test('1b.34a: a refusal on Hostname sits after the last Bind to row, marks every checked row and focuses the first', async () => {
  const { bff, main } = await loaded({
    save: () =>
      reply(422, {
        key: 'client-unresolvable-host',
        args: ['gpu-box.lan'],
        field: 'Hostname',
      }),
  })
  editPort(main)
  await save(main, bff)
  const text = en('api-client-unresolvable-host', 'gpu-box.lan')
  await within(main).findByText(en('save-refused', text))
  const bind = within(main).getByRole('group', { name: 'Bind to' })
  const rows = within(bind).getAllByRole('checkbox') as HTMLInputElement[]
  // Rows: localhost, 0.0.0.0, 10.0.0.9 (gone), enp5s0; all but 0.0.0.0 checked.
  expect(rows.map((r) => r.checked)).toEqual([true, false, true, true])
  const error = within(bind).getByText(text)
  expect(isHidden(error)).toBe(false)
  expect(before(rows[rows.length - 1]!, error), 'after the last row').toBe(true)
  for (const i of [0, 2, 3]) expectError(rows[i]!, text, `checked row ${i}`)
  expectNoError(rows[1]!, 'the unchecked row')
  expect(document.activeElement).toBe(rows[0])
  expectAlert(main, en('save-refused', text))

  // Checking a row clears the error (31), not the alert (30a).
  fireEvent.click(rows[1]!)
  expect(within(bind).queryByText(text)).toBeNull()
  expect(
    within(bind)
      .getAllByRole('checkbox')
      .filter((r) => r.getAttribute('aria-invalid') === 'true'),
  ).toEqual([])
  expectAlert(main, en('save-refused', text), 'the alert after the click')
})

test('1b.34a (5.6): the fallback of client-unresolvable-host on 0.0.0.0 focuses that row; unchecking clears it', async () => {
  const { bff, main } = await loaded({
    server: () => reply(200, serverDoc({ Hostname: '0.0.0.0' })),
    save: () =>
      reply(422, { key: 'client-unresolvable-host', args: ['gpu-box.lan'] }),
  })
  editPort(main)
  await save(main, bff)
  const text = en('api-client-unresolvable-host', 'gpu-box.lan')
  await within(main).findByText(en('save-refused', text))
  const bind = within(main).getByRole('group', { name: 'Bind to' })
  const rows = within(bind).getAllByRole('checkbox') as HTMLInputElement[]
  // Rows: localhost, 0.0.0.0, enp5s0; only 0.0.0.0 checked, the others disabled.
  expect(rows.map((r) => r.checked)).toEqual([false, true, false])
  expectError(rows[1]!, text, '0.0.0.0')
  expectNoError(rows[0]!, 'localhost')
  expectNoError(rows[2]!, 'enp5s0')
  expect(document.activeElement).toBe(rows[1])
  fireEvent.click(rows[1]!)
  expect(within(bind).queryByText(text)).toBeNull()
  expectNoError(
    within(bind).getAllByRole('checkbox')[1]!,
    '0.0.0.0 after unchecking',
  )
})

test('1b.34b: a refusal on Max models with default ticked marks the disabled box and focuses its default box', async () => {
  const { bff, main } = await loaded({
    save: () =>
      reply(422, { key: 'load-on-startup-over-max', args: ['5', '4'] }),
  })
  editPort(main)
  await save(main, bff)
  const text = en('api-load-on-startup-over-max', '5', '4')
  await within(main).findByText(en('save-refused', text))
  const max = box(main, 'Max models')
  expect(max.disabled).toBe(true)
  expectError(max, text, 'Max models (disabled)')
  const def = check(main, 'Max models', en('field-default'))
  expect(document.activeElement).toBe(def)
  // Nothing is ticked or enabled for the user.
  expect(def.checked).toBe(true)
  expect(box(main, 'Max models').disabled).toBe(true)
})

test('1b.34b: a refusal on a field disabled by its switch marks it and focuses the switch', async () => {
  const cases: [string, Record<string, unknown>, string, string, string][] = [
    [
      'Snapshot folder',
      { SaveStateOnShutdown: false },
      'StateDir',
      'Save on shutdown',
      en('server-save-on-shutdown-toggle'),
    ],
    [
      'Web UI MCP proxy',
      { Webui: false },
      'WebuiMcpProxy',
      'Web UI',
      en('server-web-ui-toggle'),
    ],
  ]
  for (const [field, values, key, switchField, switchName] of cases) {
    const { bff, main } = await loaded({
      server: () => reply(200, serverDoc(values)),
      save: () =>
        reply(422, { key: 'comment-markers', args: [key], field: key }),
    })
    editPort(main)
    await save(main, bff)
    const text = en('api-comment-markers', key)
    await within(main).findByText(en('save-refused', text))
    const control =
      field === 'Snapshot folder'
        ? box(main, field)
        : check(main, field, en('server-mcp-proxy-toggle'))
    expect((control as HTMLInputElement).disabled, field).toBe(true)
    expectError(control, text, field)
    const toggle = check(main, switchField, switchName)
    expect(document.activeElement, `${field}: focus on its switch`).toBe(toggle)
    expect(toggle.checked, `${field}: the switch is not turned on`).toBe(false)
    expect((control as HTMLInputElement).disabled, field).toBe(true)
    cleanup()
    vi.unstubAllGlobals()
  }
})

test('1b.36a: after a 401 the banner and the nav stay; Sign out still asks first', async () => {
  const { bff, main } = await loaded({
    save: () => reply(401, { key: 'session-required', args: [] }),
  })
  const nav = screen.getByRole('navigation', { name: en('nav-label') })
  await within(nav).findByRole('link', { name: 'coder' })
  editPort(main)
  await save(main, bff)
  await within(main).findByText(en('session-ended'))
  const banner = screen.getByRole('banner')
  expect(banner.textContent).toContain(en('session-signed-in-as', 'alice'))
  expect(banner.textContent).toContain(en('session-role-admin'))
  const signOut = within(banner).getByRole('button', {
    name: en('session-sign-out'),
  })
  expect(
    within(banner).queryByRole('link', { name: en('session-sign-in') }),
  ).toBeNull()
  expect(within(nav).getByRole('link', { name: 'coder' })).toBeTruthy()
  expect(within(nav).getByRole('link', { name: 'default' })).toBeTruthy()
  // Sign out with the edits: the discard dialog first (15, 38).
  signOut.focus()
  fireEvent.click(signOut)
  const d = dialog(en('discard-title'))
  fireEvent.click(within(d).getByRole('button', { name: en('discard-keep') }))
  await act(async () => {})
  expect(bff.to('DELETE', '/api/session')).toHaveLength(0)
  expect(box(main, 'Port').value).toBe('8581')
})

test('1b.36a: Escape in the sign-in dialog closes it, focus on the bar Sign in, alert and edits kept, nothing sent', async () => {
  const { bff, main } = await loaded({
    save: () => reply(401, { key: 'session-required', args: [] }),
  })
  editPort(main)
  await save(main, bff)
  const signIn = await within(main).findByRole('button', {
    name: en('session-sign-in'),
  })
  fireEvent.click(signIn)
  const d = dialog(en('signin-title'))
  const user = within(d).getByRole('textbox', { name: en('signin-username') })
  type(user, 'alice')
  fireEvent.keyDown(user, { key: 'Escape', code: 'Escape' })
  await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull())
  expect(document.activeElement).toBe(
    within(main).getByRole('button', { name: en('session-sign-in') }),
  )
  expectAlert(main, en('session-ended'))
  expect(box(main, 'Port').value).toBe('8581')
  expect(status(main).textContent).toBe(en('save-unsaved'))
  expect(bff.to('POST', '/api/session')).toHaveLength(0)
  expect(bff.to(...PUT)).toHaveLength(1)
})

/** Saves into a 401 and signs in, in the dialog, with the reply `session`. */
async function signInAs(
  session: { user: string; canConfigure: boolean },
  server: () => Response | Promise<Response> = () => reply(200, serverDoc()),
) {
  let reads = 0
  const { bff, main } = await loaded({
    server: () => (++reads === 1 ? reply(200, serverDoc()) : server()),
    save: () => reply(401, { key: 'session-required', args: [] }),
    signIn: () => reply(200, session),
  })
  editPort(main)
  fireEvent.click(check(main, 'Web UI', en('server-web-ui-toggle')))
  await save(main, bff)
  fireEvent.click(
    await within(main).findByRole('button', { name: en('session-sign-in') }),
  )
  const d = dialog(en('signin-title'))
  type(
    within(d).getByRole('textbox', { name: en('signin-username') }),
    session.user,
  )
  type(
    within(d).getByLabelText(en('signin-password'), { selector: 'input' }),
    'hunter2',
  )
  fireEvent.click(within(d).getByRole('button', { name: en('signin-submit') }))
  await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull())
  return { bff, main }
}

/** The role="alert" with `server-edits-dropped`, above the first card. */
function expectDropped(main: HTMLElement) {
  const el = within(main).getByText(en('server-edits-dropped'))
  expect(el.closest('[role="alert"]'), 'role="alert"').not.toBeNull()
  const firstCard = within(main).getByRole('heading', {
    level: 3,
    name: en('server-network'),
  })
  expect(before(el, firstCard), 'above the first card').toBe(true)
}

test('1b.36b: a sign-in as another administrator drops the edits, reads the file once and says so', async () => {
  const { bff, main } = await signInAs(
    { user: 'carol', canConfigure: true },
    () => reply(200, { ...serverDoc({ Port: 9000 }), revision: 'r-9' }),
  )
  const banner = screen.getByRole('banner')
  await waitFor(() =>
    expect(banner.textContent).toContain(en('session-signed-in-as', 'carol')),
  )
  await waitFor(() =>
    expect(bff.to('GET', '/api/instances/default/server')).toHaveLength(2),
  )
  await waitFor(() => expect(box(main, 'Port').value).toBe('9000'))
  // Clean, editable, the edits gone.
  expect(check(main, 'Web UI', en('server-web-ui-toggle')).checked).toBe(true)
  expect(box(main, 'Port').disabled).toBe(false)
  expect(status(main).textContent).toBe('')
  expect(saveButton(main).getAttribute('aria-disabled')).toBe('true')
  expect(within(main).queryByText(en('session-ended'))).toBeNull()
  expectDropped(main)
  expect(document.activeElement).toBe(
    within(main).getByRole('heading', { level: 1, name: 'default' }),
  )
  // Nothing is sent but the read.
  expect(bff.to(...PUT)).toHaveLength(1)
  expect(bff.to('GET', '/api/instances/default/server')).toHaveLength(2)
  // The next save carries the new revision.
  editPort(main)
  await save(main, bff, 2)
  expect(bff.to(...PUT)[1]!.body).toMatchObject({ revision: 'r-9' })
})

test('1b.36b: a sign-in that cannot configure drops the edits and shows the read-only form, the alert above the Notice', async () => {
  for (const user of ['alice', 'bob']) {
    const { bff, main } = await signInAs({ user, canConfigure: false })
    await waitFor(() =>
      expect(bff.to('GET', '/api/instances/default/server')).toHaveLength(2),
    )
    const notice = await within(main).findByText(en('server-read-only'))
    await waitFor(() => expect(box(main, 'Port').value, user).toBe('8580'))
    expect(
      screen.getByRole('banner').textContent,
      `${user}: the banner shows the User role`,
    ).toContain(en('session-role-user'))
    const controls = [
      ...main.querySelectorAll<HTMLInputElement>('input, select, textarea'),
    ]
    expect(
      controls.filter((c) => !c.disabled).map((c) => c.outerHTML),
      `${user}: read-only`,
    ).toEqual([])
    expect(
      within(main).queryByRole('button', { name: en('save-button') }),
      user,
    ).toBeNull()
    expectDropped(main)
    expect(
      before(within(main).getByText(en('server-edits-dropped')), notice),
      `${user}: above the Notice`,
    ).toBe(true)
    expect(document.activeElement, user).toBe(
      within(main).getByRole('heading', { level: 1, name: 'default' }),
    )
    expect(bff.to(...PUT), user).toHaveLength(1)
    cleanup()
    vi.unstubAllGlobals()
  }
})

test('1b.36b: the edits-dropped alert stays until the page is left', async () => {
  const { main } = await signInAs({ user: 'carol', canConfigure: true })
  await waitFor(() => expectDropped(main))
  fireEvent.click(
    await within(
      screen.getByRole('navigation', { name: en('nav-label') }),
    ).findByRole('link', { name: 'coder' }),
  )
  await within(main).findByRole('heading', { level: 1, name: 'coder' })
  expect(within(main).queryByText(en('server-edits-dropped'))).toBeNull()
  fireEvent.click(navLink('default'))
  await within(main).findByRole('heading', {
    level: 3,
    name: en('server-network'),
  })
  expect(within(main).queryByText(en('server-edits-dropped'))).toBeNull()
})

test('1b.36b (3.1): the edits-dropped alert shows while the file is read again', async () => {
  const gate = deferred()
  const { main } = await signInAs(
    { user: 'carol', canConfigure: true },
    () => gate.promise,
  )
  const loading = await within(main).findByText(en('server-loading'))
  // No card yet: the alert is there, above the loading line.
  const dropped = within(main).getByText(en('server-edits-dropped'))
  expect(dropped.closest('[role="alert"]'), 'role="alert"').not.toBeNull()
  expect(before(dropped, loading), 'above the loading line').toBe(true)
  await act(async () => {
    gate.resolve(reply(200, serverDoc()))
  })
  await within(main).findByRole('heading', {
    level: 3,
    name: en('server-network'),
  })
  expectDropped(main)
})

test('1b.36a: the sign-in dialog has a Cancel button after Sign in that closes it and changes nothing', async () => {
  const { bff, main } = await loaded({
    save: () => reply(401, { key: 'session-required', args: [] }),
  })
  editPort(main)
  await save(main, bff)
  fireEvent.click(
    await within(main).findByRole('button', { name: en('session-sign-in') }),
  )
  const d = dialog(en('signin-title'))
  const submit = within(d).getByRole('button', { name: en('signin-submit') })
  const cancel = within(d).getByRole('button', {
    name: en('signin-cancel'),
  }) as HTMLButtonElement
  expect(cancel.type, 'type="button": it never submits').toBe('button')
  expect(before(submit, cancel), 'after the Sign in button').toBe(true)
  type(within(d).getByRole('textbox', { name: en('signin-username') }), 'alice')
  fireEvent.click(cancel)
  await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull())
  expect(document.activeElement).toBe(
    within(main).getByRole('button', { name: en('session-sign-in') }),
  )
  expectAlert(main, en('session-ended'))
  expect(box(main, 'Port').value).toBe('8581')
  expect(bff.to('POST', '/api/session')).toHaveLength(0)
  expect(bff.to(...PUT)).toHaveLength(1)
})

test('1b.36a: Cancel is shown only in the dialog, not on the sign-in page', async () => {
  startAt('#/sign-in')
  fakeBff({ session: 'signed-out' })
  render(<App />)
  const main = screen.getByRole('main')
  await within(main).findByRole('heading', {
    level: 1,
    name: en('signin-title'),
  })
  expect(
    within(main).queryByRole('button', { name: en('signin-cancel') }),
  ).toBeNull()
})

// ── The save bar (1b.28 to 1b.30) ───────────────────────────────────────

test('1b.28: an administrator gets enabled controls (under 5.3) and the save bar at the end, clean', async () => {
  const { bff, main } = await loaded()
  const controls = [
    ...main.querySelectorAll<HTMLInputElement>('input, select, textarea'),
  ]
  expect(controls.length).toBeGreaterThan(20)
  // On load only the boxes on default or auto are disabled (5.3).
  expect(controls.filter((c) => c.disabled)).toEqual([
    box(main, 'Base URL'),
    box(main, 'CPU threads'),
    box(main, 'Max models'),
  ])
  expect(inactive(show(main))).toBe(false)
  expect(within(main).queryByText(en('server-read-only'))).toBeNull()

  const save = saveButton(main)
  const discard = discardButton(main)
  const line = status(main)
  // At the end of the form: status, then Discard, then Save changes.
  const lastField = group(main, 'KV rotation').getByRole('checkbox')
  expect(before(lastField, line)).toBe(true)
  expect(before(line, discard)).toBe(true)
  expect(before(discard, save)).toBe(true)
  expect(line.textContent).toBe('')
  for (const b of [save, discard]) {
    expect(b.getAttribute('aria-disabled'), b.textContent ?? '').toBe('true')
    // aria-disabled, not disabled: focus never drops (D5.2).
    expect((b as HTMLButtonElement).disabled, b.textContent ?? '').toBe(false)
  }
  // While clean a click does nothing.
  fireEvent.click(save)
  fireEvent.click(discard)
  await act(async () => {})
  expect(bff.to(...PUT)).toHaveLength(0)
  expect(line.textContent).toBe('')
})

test('1b.28: a user keeps the read-only form and no save bar', async () => {
  const { main } = await loaded({ session: 'user' })
  const controls = [
    ...main.querySelectorAll<HTMLInputElement>('input, select, textarea'),
  ]
  expect(controls.filter((c) => !c.disabled).map((c) => c.outerHTML)).toEqual(
    [],
  )
  expect(within(main).getByText(en('server-read-only'))).toBeTruthy()
  expect(
    within(main).queryByRole('button', { name: en('save-button') }),
  ).toBeNull()
  expect(
    within(main).queryByRole('button', { name: en('save-discard') }),
  ).toBeNull()
  expect(within(main).queryAllByRole('status')).toHaveLength(0)
})

test('1b.28 (1a): a user is never dirty: no beforeunload, leaving asks nothing, Show still works', async () => {
  const { main } = await loaded({ session: 'user' })
  const e = new Event('beforeunload', { cancelable: true })
  window.dispatchEvent(e)
  expect(e.defaultPrevented, 'beforeunload not armed').toBe(false)
  fireEvent.click(show(main))
  expect(show(main).getAttribute('aria-pressed')).toBe('true')
  expect(apiKey(main).type).toBe('text')
  const link = await within(
    screen.getByRole('navigation', { name: en('nav-label') }),
  ).findByRole('link', { name: 'coder' })
  fireEvent.click(link)
  expect(screen.queryByRole('dialog')).toBeNull()
  expect(
    await within(main).findByRole('heading', { level: 1, name: 'coder' }),
  ).toBeTruthy()
  expect(dialogs.opened, 'no dialog opened').toEqual([])
})

test('1b.29: a changed value makes the form dirty; changing it back makes it clean', async () => {
  const { main } = await loaded()
  const line = status(main)
  editPort(main)
  expect(line.textContent).toBe(en('save-unsaved'))
  for (const b of [saveButton(main), discardButton(main)]) {
    expect(
      b.getAttribute('aria-disabled') ?? 'false',
      b.textContent ?? '',
    ).toBe('false')
  }
  type(box(main, 'Port'), '8580')
  expect(line.textContent).toBe('')
  expect(saveButton(main).getAttribute('aria-disabled')).toBe('true')
  expect(discardButton(main).getAttribute('aria-disabled')).toBe('true')
  // A switch, a select: the same.
  const webui = check(main, 'Web UI', en('server-web-ui-toggle'))
  fireEvent.click(webui)
  expect(line.textContent).toBe(en('save-unsaved'))
  fireEvent.click(webui)
  expect(line.textContent).toBe('')
  type(select(main, 'Log level'), '5')
  expect(line.textContent).toBe(en('save-unsaved'))
  type(select(main, 'Log level'), '3')
  expect(line.textContent).toBe('')
})

test('1b.29 (5.4): dirty compares the values a save would send, not the raw text', async () => {
  const { main } = await loaded()
  const line = status(main)
  // A loaded file with unset switches (RocprofAttach, AttnRotDisable) is clean.
  expect(line.textContent).toBe('')
  // The same number with spaces sends the same value.
  type(box(main, 'Port'), ' 8580 ')
  expect(line.textContent).toBe('')
  // Spaces in an unset folder still send null.
  type(box(main, 'Models folder'), '   ')
  expect(line.textContent).toBe('')
  // Text that is not a number differs.
  type(box(main, 'Port'), 'abc')
  expect(line.textContent).toBe(en('save-unsaved'))
})

test('1b.30: Discard puts every control back, keeps the focus on it, empties the status and sends nothing', async () => {
  const { bff, main } = await loaded()
  editPort(main)
  fireEvent.click(check(main, 'Web UI', en('server-web-ui-toggle')))
  type(select(main, 'Log level'), '5')
  fireEvent.click(check(main, 'CPU threads', en('field-auto')))
  type(box(main, 'CPU threads'), '8')
  fireEvent.click(
    group(main, 'Bind to').getByRole('checkbox', {
      name: en('server-bind-all'),
    }),
  )
  const discard = discardButton(main)
  discard.focus()
  fireEvent.click(discard)
  expect(box(main, 'Port').value).toBe('8580')
  expect(check(main, 'Web UI', en('server-web-ui-toggle')).checked).toBe(true)
  expect(select(main, 'Log level').value).toBe('3')
  expect(check(main, 'CPU threads', en('field-auto')).checked).toBe(true)
  expect(box(main, 'CPU threads').value).toBe('')
  expect(box(main, 'CPU threads').disabled).toBe(true)
  expect(
    group(main, 'Bind to')
      .getAllByRole('checkbox')
      .map((c) => (c as HTMLInputElement).checked),
  ).toEqual([true, false, true, true])
  expect(document.activeElement).toBe(discard)
  expect(status(main).textContent).toBe('')
  expect(discard.getAttribute('aria-disabled')).toBe('true')
  expect(bff.calls.filter((c) => c.method !== 'GET')).toEqual([])
})

/** Focuses the box `name` of `field` and clicks it, as a keyboard user does. */
function toggle(main: HTMLElement, field: string, name: string) {
  const c = check(main, field, name)
  c.focus()
  fireEvent.click(c)
  return c
}

test('1b.28a: unticking brings back the text held before the tick; focus stays on the box', async () => {
  const { main } = await loaded()
  // Port, loaded 8580, typed over.
  type(box(main, 'Port'), '8581')
  toggle(main, 'Port', en('field-default'))
  expect(box(main, 'Port').value).toBe('9931')
  let c = toggle(main, 'Port', en('field-default'))
  expect(box(main, 'Port').value).toBe('8581')
  expect(document.activeElement, 'Port: focus stays on the box').toBe(c)

  // Max models, loaded on default: first the default shown, then the typed text.
  c = toggle(main, 'Max models', en('field-default'))
  expect(box(main, 'Max models').value).toBe('4')
  expect(document.activeElement, 'Max models: focus stays').toBe(c)
  type(box(main, 'Max models'), '6')
  toggle(main, 'Max models', en('field-default'))
  expect(box(main, 'Max models').value).toBe('4')
  toggle(main, 'Max models', en('field-default'))
  expect(box(main, 'Max models').value).toBe('6')

  // Base URL, loaded on default.
  toggle(main, 'Base URL', en('field-default'))
  type(box(main, 'Base URL'), 'https://gw.example.net')
  toggle(main, 'Base URL', en('field-default'))
  expect(box(main, 'Base URL').value).toBe('http://127.0.0.1:8580/v1')
  c = toggle(main, 'Base URL', en('field-default'))
  expect(box(main, 'Base URL').value).toBe('https://gw.example.net')
  expect(document.activeElement, 'Base URL: focus stays').toBe(c)

  // Memory margin, loaded 1024.
  toggle(main, 'Memory margin', en('field-default'))
  expect(box(main, 'Memory margin').value).toBe('0')
  toggle(main, 'Memory margin', en('field-default'))
  expect(box(main, 'Memory margin').value).toBe('1024')

  // API key: No key empties it; unticking brings the key back.
  type(apiKey(main), 'sk-typed-9')
  toggle(main, 'API key', en('field-no-key'))
  expect(apiKey(main).value).toBe('')
  c = toggle(main, 'API key', en('field-no-key'))
  expect(apiKey(main).value).toBe('sk-typed-9')
  expect(document.activeElement, 'API key: focus stays').toBe(c)

  // CPU threads, loaded on auto: empty, focused; then the typed text.
  toggle(main, 'CPU threads', en('field-auto'))
  expect(box(main, 'CPU threads').value).toBe('')
  expect(document.activeElement).toBe(box(main, 'CPU threads'))
  type(box(main, 'CPU threads'), '8')
  toggle(main, 'CPU threads', en('field-auto'))
  expect(box(main, 'CPU threads').value).toBe('')
  toggle(main, 'CPU threads', en('field-auto'))
  expect(box(main, 'CPU threads').value).toBe('8')

  // Batch threads, loaded 16.
  toggle(main, 'Batch threads', en('field-auto'))
  expect(box(main, 'Batch threads').value).toBe('')
  toggle(main, 'Batch threads', en('field-auto'))
  expect(box(main, 'Batch threads').value).toBe('16')
})

test('1b.28a: a field loaded on No key comes back empty', async () => {
  const { main } = await loaded({
    server: () => reply(200, serverDoc({ OpencodeApiKey: null })),
  })
  toggle(main, 'API key', en('field-no-key'))
  expect(apiKey(main).disabled).toBe(false)
  expect(apiKey(main).value).toBe('')
})

test('1b.28a: Discard and a successful save reset the memory to the loaded values', async () => {
  const { bff, main } = await loaded()
  type(box(main, 'Port'), '8581')
  toggle(main, 'Port', en('field-default'))
  fireEvent.click(discardButton(main))
  expect(box(main, 'Port').value).toBe('8580')
  toggle(main, 'Port', en('field-default'))
  toggle(main, 'Port', en('field-default'))
  expect(box(main, 'Port').value, 'after Discard').toBe('8580')

  // Saved on default: the loaded value is now unset, so the default shown.
  type(box(main, 'Port'), '8581')
  toggle(main, 'Port', en('field-default'))
  await save(main, bff)
  expect(bff.to(...PUT)[0]!.body).toMatchObject({ values: { Port: null } })
  await waitFor(() => expect(status(main).textContent).toBe(en('save-saved')))
  toggle(main, 'Port', en('field-default'))
  expect(box(main, 'Port').value, 'after the save').toBe('9931')
})

test('1b.30a: no alert while there is nothing to say; Discard ends one', async () => {
  const { bff, main } = await loaded({
    save: () => reply(503, { key: 'client-unavailable', args: [] }),
  })
  expectNoAlert(main, 'on load')
  editPort(main)
  expectNoAlert(main, 'dirty')
  await save(main, bff)
  await within(main).findByText(en('api-client-unavailable'))
  // Editing a field does not end it.
  type(box(main, 'Port'), '8582')
  expectAlert(main, en('api-client-unavailable'), 'after an edit')
  fireEvent.click(discardButton(main))
  expectNoAlert(main, 'after Discard')
  expect(status(main).textContent).toBe('')
})

test('1b.30a: the next save replaces the alert with its own answer; a successful one ends it', async () => {
  const { bff, main } = await loaded({
    save: (_body, n) =>
      n === 1
        ? reply(503, { key: 'client-unavailable', args: [] })
        : n === 2
          ? reply(403, { key: 'client-not-authorized', args: [] })
          : reply(200, { revision: 'r-2', running: false }),
  })
  editPort(main)
  await save(main, bff)
  await within(main).findByText(en('api-client-unavailable'))
  await save(main, bff, 2)
  await within(main).findByText(en('api-client-not-authorized'))
  expect(within(main).queryByText(en('api-client-unavailable'))).toBeNull()
  expectAlert(main, en('api-client-not-authorized'))
  await save(main, bff, 3)
  await waitFor(() => expect(status(main).textContent).toBe(en('save-saved')))
  expectNoAlert(main, 'after a successful save')
})

test('1b.30a: a Save click that passes the checks clears the alert at once, before the answer', async () => {
  const gate = deferred()
  const { bff, main } = await loaded({
    save: (_body, n) =>
      n === 1
        ? reply(503, { key: 'client-unavailable', args: [] })
        : gate.promise,
  })
  editPort(main)
  await save(main, bff)
  await within(main).findByText(en('api-client-unavailable'))
  await save(main, bff, 2)
  expectNoAlert(main, 'while the second PUT runs')
  expect(status(main).textContent).toBe(en('save-saving'))
  await act(async () => {
    gate.resolve(reply(403, { key: 'client-not-authorized', args: [] }))
  })
  await within(main).findByText(en('api-client-not-authorized'))
})

test('1b.30a: a Save click that fails the client checks replaces the older alert with save-invalid', async () => {
  // Ruling given to the coder: every Save click replaces the previous alert.
  const { bff, main } = await loaded({
    save: () => reply(409, { key: 'server-changed-elsewhere', args: [] }),
  })
  editPort(main)
  await save(main, bff)
  await within(main).findByText(en('api-server-changed-elsewhere'))
  type(box(main, 'Port'), 'abc')
  fireEvent.click(saveButton(main))
  await within(main).findByText(en('save-invalid'))
  expectAlert(main, en('save-invalid'))
  expect(
    within(main)
      .queryAllByRole('alert')
      .map((a) => a.textContent),
    'only the new alert',
  ).toEqual([en('save-invalid')])
  expect(bff.to(...PUT)).toHaveLength(1)
})

// ── Saving (1b.32, 1b.33) ───────────────────────────────────────────────

test('1b.32: Save sends PUT with the loaded revision and every value, unset ones as null', async () => {
  const { bff, main } = await loaded()
  editPort(main)
  await save(main, bff)
  const [put] = bff.to(...PUT)
  expect(put!.body).toEqual({
    revision: REVISION,
    values: { ...UNCHANGED, Port: 8581 },
  })
  // JSON in (api-contract.md): the bff reads a body only as application/json.
  const init = bff.fetch.mock.calls.find(([, i]) => i?.method === 'PUT')![1]!
  expect(new Headers(init.headers).get('content-type') ?? '').toMatch(
    /^application\/json\b/i,
  )
})

test('1b.32: while the PUT runs Save reads save-saving, is aria-disabled and a second click sends nothing', async () => {
  const gate = deferred()
  const { bff, main } = await loaded({ save: () => gate.promise })
  editPort(main)
  const button = saveButton(main)
  fireEvent.click(button)
  await waitFor(() => expect(bff.to(...PUT)).toHaveLength(1))
  expect(button.textContent).toBe(en('save-saving'))
  expect(button.getAttribute('aria-disabled')).toBe('true')
  expect((button as HTMLButtonElement).disabled).toBe(false)
  // The status line reads it too (30a).
  expect(status(main).textContent).toBe(en('save-saving'))
  fireEvent.click(button)
  await act(async () => {})
  expect(bff.to(...PUT)).toHaveLength(1)
  await act(async () => {
    gate.resolve(reply(200, { revision: 'r-2', running: false }))
  })
  await waitFor(() => expect(button.textContent).toBe(en('save-button')))
})

test('1b.32: Discard is aria-disabled while the PUT runs and a click does nothing', async () => {
  const gate = deferred()
  const { bff, main } = await loaded({ save: () => gate.promise })
  editPort(main)
  await save(main, bff)
  const discard = discardButton(main)
  expect(discard.getAttribute('aria-disabled')).toBe('true')
  expect((discard as HTMLButtonElement).disabled).toBe(false)
  fireEvent.click(discard)
  expect(box(main, 'Port').value).toBe('8581')
  expect(status(main).textContent).toBe(en('save-saving'))
  await act(async () => {
    gate.resolve(reply(200, { revision: 'r-2', running: false }))
  })
  await waitFor(() => expect(status(main).textContent).toBe(en('save-saved')))
  expect(box(main, 'Port').value).toBe('8581')
})

test('1b.33b: edits made while the PUT runs are kept, and the form is dirty against the saved values', async () => {
  const gate = deferred()
  const { bff, main } = await loaded({
    save: (_body, n) =>
      n === 1 ? gate.promise : reply(200, { revision: 'r-3', running: false }),
  })
  editPort(main)
  await save(main, bff)
  // The fields stay editable while it runs.
  expect(box(main, 'Port').disabled).toBe(false)
  type(box(main, 'Port'), '8582')
  await act(async () => {
    gate.resolve(reply(200, { revision: 'r-2', running: false }))
  })
  await waitFor(() => expect(status(main).textContent).toBe(en('save-unsaved')))
  expect(box(main, 'Port').value, 'what the user typed stays').toBe('8582')
  // Back to what was sent is clean: the sent values are the loaded ones.
  type(box(main, 'Port'), '8581')
  expect(status(main).textContent).toBe('')
  type(box(main, 'Port'), '8582')
  await save(main, bff, 2)
  expect(bff.to(...PUT)[1]!.body).toEqual({
    revision: 'r-2',
    values: { ...UNCHANGED, Port: 8582 },
  })
})

test('1b.32 (5.4): each field is sent in its typed form', async () => {
  const { bff, main } = await loaded()
  fireEvent.click(check(main, 'Port', en('field-default')))
  fireEvent.click(
    group(main, 'Bind to').getByRole('checkbox', {
      name: en('server-bind-all'),
    }),
  )
  fireEvent.click(check(main, 'Base URL', en('field-default')))
  type(box(main, 'Base URL'), '  https://gw.example.net/v1/  ')
  type(apiKey(main), '')
  fireEvent.click(check(main, 'CPU threads', en('field-auto')))
  type(box(main, 'CPU threads'), ' 8 ')
  fireEvent.click(check(main, 'Batch threads', en('field-auto')))
  // Unticking default keeps the shown default (4) as the value (5.2).
  fireEvent.click(check(main, 'Max models', en('field-default')))
  fireEvent.click(check(main, 'Memory margin', en('field-default')))
  type(box(main, 'Models folder'), '  /srv/models  ')
  fireEvent.click(check(main, 'Web UI', en('server-web-ui-toggle')))
  type(select(main, 'Log level'), '5')
  type(box(main, 'Snapshot folder'), '   ')
  type(select(main, 'hipBLASLt'), 'default')
  fireEvent.click(
    check(main, 'Profiler attach', en('server-profiler-attach-toggle')),
  )
  type(select(main, 'Tensor-split AllReduce'), 'internal')
  fireEvent.click(
    check(main, 'Disable GPU graphs', en('server-gpu-graphs-toggle')),
  )
  await save(main, bff)
  expect(bff.to(...PUT)[0]!.body).toEqual({
    revision: REVISION,
    values: {
      Port: null,
      Hostname: '0.0.0.0',
      OpencodeBaseUrl: 'https://gw.example.net',
      OpencodeApiKey: null,
      Threads: 8,
      ThreadsBatch: null,
      ModelsMax: 4,
      ModelsMemoryMargin: null,
      ModelsDir: '/srv/models',
      Webui: false,
      WebuiMcpProxy: false,
      LogVerbosity: 5,
      SaveStateOnShutdown: true,
      StateDir: null,
      RocblasUseHipblaslt: null,
      RocprofAttach: true,
      CudaAllreduce: 'internal',
      CudaDisableGraphs: true,
      AttnRotDisable: false,
    },
  })
})

test('1b.32 (5.4): a blank Models folder or Snapshot folder is sent as null (unset)', async () => {
  const { bff, main } = await loaded({
    server: () => reply(200, serverDoc({ ModelsDir: '/srv/models' })),
  })
  type(box(main, 'Models folder'), '')
  type(box(main, 'Snapshot folder'), '  ')
  await save(main, bff)
  expect(bff.to(...PUT)[0]!.body).toEqual({
    revision: REVISION,
    values: { ...UNCHANGED, ModelsDir: null, StateDir: null },
  })
})

test('1b.32 (5.4): No key ticked sends null; Bind to sends the checked rows in row order', async () => {
  const { bff, main } = await loaded({
    server: () => reply(200, serverDoc({ Hostname: '192.168.1.20' })),
  })
  fireEvent.click(check(main, 'API key', en('field-no-key')))
  // Rows: localhost, 0.0.0.0, enp5s0 (192.168.1.20); check localhost.
  fireEvent.click(
    group(main, 'Bind to').getByRole('checkbox', {
      name: en('server-bind-localhost'),
    }),
  )
  await save(main, bff)
  expect(bff.to(...PUT)[0]!.body).toEqual({
    revision: REVISION,
    values: {
      ...UNCHANGED,
      Hostname: 'localhost,192.168.1.20',
      OpencodeApiKey: null,
    },
  })
})

test('1b.33: 200 makes the sent values and the new revision the loaded ones; running picks the message', async () => {
  const { bff, main } = await loaded({
    save: (_body, n) =>
      n === 1
        ? reply(200, { revision: 'r-2', running: false })
        : reply(200, { revision: 'r-3', running: true }),
  })
  const line = status(main)
  editPort(main)
  await save(main, bff)
  await waitFor(() => expect(line.textContent).toBe(en('save-saved')))
  expect(saveButton(main).getAttribute('aria-disabled')).toBe('true')
  expect(discardButton(main).getAttribute('aria-disabled')).toBe('true')
  // 8581 is now the loaded value: 8580 is a change.
  type(box(main, 'Port'), '8580')
  expect(line.textContent).toBe(en('save-unsaved'))
  await save(main, bff, 2)
  expect(bff.to(...PUT)[1]!.body).toEqual({
    revision: 'r-2',
    values: { ...UNCHANGED, Port: 8580 },
  })
  await waitFor(() => expect(line.textContent).toBe(en('save-saved-restart')))
  // A save never restarts the server, and the file is not read again.
  expect(bff.calls.filter((c) => c.method === 'POST')).toEqual([])
  expect(bff.to('GET', '/api/instances/default/server')).toHaveLength(1)
})

test('1b.33: Discard after a save goes back to the saved values', async () => {
  const { bff, main } = await loaded()
  editPort(main)
  await save(main, bff)
  await waitFor(() => expect(status(main).textContent).toBe(en('save-saved')))
  type(box(main, 'Port'), '1234')
  fireEvent.click(discardButton(main))
  expect(box(main, 'Port').value).toBe('8581')
})

test('1b.33a: after a 200 the controls show the values sent, focus stays on Save changes, defaults are not read again', async () => {
  const { bff, main } = await loaded()
  type(box(main, 'Port'), '08581')
  toggle(main, 'Base URL', en('field-default'))
  type(box(main, 'Base URL'), '  https://gw.example.net/v1/  ')
  type(apiKey(main), '')
  toggle(main, 'CPU threads', en('field-auto'))
  type(box(main, 'CPU threads'), ' 8 ')
  type(box(main, 'Models folder'), '  /srv/models  ')
  type(box(main, 'Snapshot folder'), '   ')
  const button = saveButton(main)
  button.focus()
  await save(main, bff)
  expect(bff.to(...PUT)[0]!.body).toEqual({
    revision: REVISION,
    values: {
      ...UNCHANGED,
      Port: 8581,
      OpencodeBaseUrl: 'https://gw.example.net',
      OpencodeApiKey: null,
      Threads: 8,
      ModelsDir: '/srv/models',
      StateDir: null,
    },
  })
  await waitFor(() => expect(status(main).textContent).toBe(en('save-saved')))
  expect(box(main, 'Port').value).toBe('8581')
  expect(check(main, 'Base URL', en('field-default')).checked).toBe(false)
  expect(box(main, 'Base URL').value).toBe('https://gw.example.net')
  expect(check(main, 'API key', en('field-no-key')).checked).toBe(true)
  expect(apiKey(main).disabled).toBe(true)
  expect(apiKey(main).value).toBe('')
  expect(box(main, 'CPU threads').value).toBe('8')
  expect(box(main, 'Models folder').value).toBe('/srv/models')
  const state = box(main, 'Snapshot folder')
  expect(state.value).toBe('')
  expect(state.placeholder).toBe(
    en('server-folder-instance-default', '/var/lib/llama.cpp/state'),
  )
  expect(document.activeElement, 'focus stays on Save changes').toBe(
    saveButton(main),
  )
  // The defaults read with the page stay; the file is not read again.
  expect(box(main, 'Max models').value).toBe('4')
  toggle(main, 'Port', en('field-default'))
  expect(box(main, 'Port').value).toBe('9931')
  expect(bff.to('GET', '/api/instances/default/server')).toHaveLength(1)
})

test('1b.32 (5.4, 3.1): Base URL loses every trailing / around /v1; a blank one is null and shows the default after the 200', async () => {
  const { bff, main } = await loaded({
    save: (_body, n) => reply(200, { revision: `r-${n + 1}`, running: false }),
  })
  toggle(main, 'Base URL', en('field-default'))
  type(box(main, 'Base URL'), 'https://gw.example.net/v1//')
  await save(main, bff)
  expect(bff.to(...PUT)[0]!.body).toMatchObject({
    values: { OpencodeBaseUrl: 'https://gw.example.net' },
  })
  await waitFor(() => expect(status(main).textContent).toBe(en('save-saved')))
  type(box(main, 'Base URL'), '   ')
  editPort(main)
  await save(main, bff, 2)
  expect(bff.to(...PUT)[1]!.body).toMatchObject({
    values: { OpencodeBaseUrl: null },
  })
  await waitFor(() => expect(status(main).textContent).toBe(en('save-saved')))
  expect(check(main, 'Base URL', en('field-default')).checked).toBe(true)
  expect(box(main, 'Base URL').value).toBe('http://127.0.0.1:8580/v1')
})

test('1b.32 (5.4, 3.1): the API key is sent as typed; a blank one (only spaces) is null', async () => {
  const { bff, main } = await loaded({
    save: (_body, n) => reply(200, { revision: `r-${n + 1}`, running: false }),
  })
  type(apiKey(main), '  sk-spaced  ')
  await save(main, bff)
  expect(bff.to(...PUT)[0]!.body).toMatchObject({
    values: { OpencodeApiKey: '  sk-spaced  ' },
  })
  await waitFor(() => expect(status(main).textContent).toBe(en('save-saved')))
  type(apiKey(main), '   ')
  await save(main, bff, 2)
  expect(bff.to(...PUT)[1]!.body).toMatchObject({
    values: { OpencodeApiKey: null },
  })
})

test('1b.31 (5.4, 3.1): a number with no limits pair for its key skips the client check', async () => {
  const doc = serverDoc()
  const limits: Record<string, unknown> = { ...doc.limits }
  delete limits.Port
  const { bff, main } = await loaded({
    server: () => reply(200, { ...doc, limits }),
  })
  type(box(main, 'Port'), '70000')
  await save(main, bff)
  expect(within(main).queryByText(en('save-invalid'))).toBeNull()
  expectNoError(box(main, 'Port'), 'Port')
  expect(bff.to(...PUT)[0]!.body).toMatchObject({ values: { Port: 70000 } })
})

// ── Dependencies (5.3) ──────────────────────────────────────────────────

test('1b 5.3: a default or auto box enables its textbox only while clear', async () => {
  const { main } = await loaded()
  // Ticking default shows the default the API gave, disabled (D5.2).
  fireEvent.click(check(main, 'Port', en('field-default')))
  expect(box(main, 'Port').disabled).toBe(true)
  expect(box(main, 'Port').value).toBe('9931')
  // Unticking keeps the shown text as the starting value (5.2).
  fireEvent.click(check(main, 'Max models', en('field-default')))
  expect(box(main, 'Max models').disabled).toBe(false)
  expect(box(main, 'Max models').value).toBe('4')
  fireEvent.click(check(main, 'Base URL', en('field-default')))
  expect(box(main, 'Base URL').disabled).toBe(false)
  expect(box(main, 'Base URL').value).toBe('http://127.0.0.1:8580/v1')
  fireEvent.click(check(main, 'Memory margin', en('field-default')))
  expect(box(main, 'Memory margin').disabled).toBe(true)
  fireEvent.click(check(main, 'Batch threads', en('field-auto')))
  expect(box(main, 'Batch threads').disabled).toBe(true)
})

test('1b 5.3: No key disables the API key and Show while ticked', async () => {
  const { main } = await loaded()
  const noKey = check(main, 'API key', en('field-no-key'))
  fireEvent.click(noKey)
  expect(apiKey(main).disabled).toBe(true)
  expect(inactive(show(main))).toBe(true)
  fireEvent.click(noKey)
  expect(apiKey(main).disabled).toBe(false)
  expect(inactive(show(main))).toBe(false)
})

test('1b 5.3: a file with no key loads with the API key and Show disabled for an administrator', async () => {
  const { main } = await loaded({
    server: () =>
      reply(
        200,
        serverDoc({
          OpencodeApiKey: null,
          Webui: false,
          SaveStateOnShutdown: false,
          Hostname: '0.0.0.0',
        }),
      ),
  })
  expect(apiKey(main).disabled).toBe(true)
  expect(inactive(show(main))).toBe(true)
  expect(
    check(main, 'Web UI MCP proxy', en('server-mcp-proxy-toggle')).disabled,
  ).toBe(true)
  expect(box(main, 'Snapshot folder').disabled).toBe(true)
  const rows = group(main, 'Bind to').getAllByRole(
    'checkbox',
  ) as HTMLInputElement[]
  expect(rows.map((r) => r.checked)).toEqual([false, true, false])
  expect(rows.map((r) => r.disabled)).toEqual([true, false, true])
})

test('1b 5.3: Bind to keeps one row checked and 0.0.0.0 alone', async () => {
  const { main } = await loaded()
  const bind = group(main, 'Bind to')
  const rows = () => bind.getAllByRole('checkbox') as HTMLInputElement[]
  const all = bind.getByRole('checkbox', { name: en('server-bind-all') })
  // Checking 0.0.0.0 unchecks and disables every other row.
  fireEvent.click(all)
  expect(rows().map((r) => r.checked)).toEqual([false, true, false, false])
  expect(rows().map((r) => r.disabled)).toEqual([true, false, true, true])
  // Unchecking it re-enables them and leaves localhost checked.
  fireEvent.click(all)
  expect(rows().map((r) => r.checked)).toEqual([true, false, false, false])
  expect(rows().map((r) => r.disabled)).toEqual([false, false, false, false])
  // Unchecking the last checked row checks localhost again.
  fireEvent.click(rows()[3]!)
  expect(rows().map((r) => r.checked)).toEqual([true, false, false, true])
  fireEvent.click(rows()[0]!)
  expect(rows().map((r) => r.checked)).toEqual([false, false, false, true])
  fireEvent.click(rows()[3]!)
  expect(rows().map((r) => r.checked)).toEqual([true, false, false, false])
  fireEvent.click(rows()[0]!)
  expect(rows().map((r) => r.checked)).toEqual([true, false, false, false])
})

test('1b 5.3: MCP proxy follows Web UI and Snapshot folder follows Save on shutdown; values kept and sent', async () => {
  const { bff, main } = await loaded()
  const proxy = check(main, 'Web UI MCP proxy', en('server-mcp-proxy-toggle'))
  fireEvent.click(proxy)
  expect(proxy.checked).toBe(true)
  fireEvent.click(check(main, 'Web UI', en('server-web-ui-toggle')))
  expect(proxy.disabled).toBe(true)
  expect(proxy.checked).toBe(true)
  fireEvent.click(
    check(main, 'Save on shutdown', en('server-save-on-shutdown-toggle')),
  )
  expect(box(main, 'Snapshot folder').disabled).toBe(true)
  expect(box(main, 'Snapshot folder').value).toBe('/data/llama-state')
  await save(main, bff)
  expect(bff.to(...PUT)[0]!.body).toEqual({
    revision: REVISION,
    values: {
      ...UNCHANGED,
      Webui: false,
      WebuiMcpProxy: true,
      SaveStateOnShutdown: false,
      StateDir: '/data/llama-state',
    },
  })
})

// ── An unset switch shows the configurator's default (5.2) ──────────────

/**
 * The switches as the shipped server@default.ini leaves them: no line for
 * any of them, so every one is null in the document.
 */
const SWITCHES_UNSET = {
  Webui: null,
  WebuiMcpProxy: null,
  SaveStateOnShutdown: null,
  RocprofAttach: null,
  CudaDisableGraphs: null,
  AttnRotDisable: null,
}

test('1b 5.2: a file with every switch unset loads clean for an administrator', async () => {
  const { bff, main } = await loaded({
    server: () => reply(200, serverDoc(SWITCHES_UNSET)),
  })
  expect(check(main, 'Web UI', en('server-web-ui-toggle')).checked).toBe(true)
  expect(status(main).textContent).toBe('')
  expect(saveButton(main).getAttribute('aria-disabled')).toBe('true')
  expect(discardButton(main).getAttribute('aria-disabled')).toBe('true')
  fireEvent.click(saveButton(main))
  await act(async () => {})
  expect(bff.to(...PUT)).toHaveLength(0)
})

test('1b 5.2: a save after an unrelated edit sends Webui true for an unset Web UI, the other unset switches false', async () => {
  const { bff, main } = await loaded({
    server: () => reply(200, serverDoc(SWITCHES_UNSET)),
  })
  editPort(main)
  await save(main, bff)
  expect(bff.to(...PUT)[0]!.body).toEqual({
    revision: REVISION,
    values: {
      ...UNCHANGED,
      Port: 8581,
      Webui: true,
      WebuiMcpProxy: false,
      SaveStateOnShutdown: false,
      RocprofAttach: false,
      CudaDisableGraphs: false,
      AttnRotDisable: false,
    },
  })
})

test('1b 5.3: with Web UI unset (ticked) the MCP proxy is enabled for an administrator, and follows Web UI', async () => {
  const { main } = await loaded({
    server: () => reply(200, serverDoc(SWITCHES_UNSET)),
  })
  const proxy = check(main, 'Web UI MCP proxy', en('server-mcp-proxy-toggle'))
  expect(proxy.disabled).toBe(false)
  expect(proxy.checked).toBe(false)
  fireEvent.click(check(main, 'Web UI', en('server-web-ui-toggle')))
  expect(proxy.disabled).toBe(true)
})

// ── Leaving with unsaved changes (1b.38) ────────────────────────────────

/** Edits, then follows `link` (focused first, as a keyboard user does). */
function leaveBy(main: HTMLElement, link: HTMLElement) {
  editPort(main)
  link.focus()
  fireEvent.click(link)
}

function navLink(name: string): HTMLElement {
  return within(
    screen.getByRole('navigation', { name: en('nav-label') }),
  ).getByRole('link', { name })
}

function productLink(): HTMLElement {
  return within(screen.getByRole('banner')).getByRole('link', {
    name: en('app-name'),
  })
}

test('1b.38: following a nav link while dirty asks first; Keep editing stays, focus back on the link', async () => {
  const { main } = await loaded()
  await within(
    screen.getByRole('navigation', { name: en('nav-label') }),
  ).findByRole('link', { name: 'coder' })
  const link = navLink(en('nav-overview'))
  leaveBy(main, link)
  const d = dialog(en('discard-title'))
  expect(within(d).getByText(en('discard-description'))).toBeTruthy()
  const keep = within(d).getByRole('button', { name: en('discard-keep') })
  const confirm = within(d).getByRole('button', {
    name: en('discard-confirm'),
  })
  expect(before(keep, confirm)).toBe(true)
  expect(document.activeElement).toBe(keep)
  expect(window.location.hash).toBe('#/instances/default')
  fireEvent.click(keep)
  expect(screen.queryByRole('dialog')).toBeNull()
  // A link click navigates in a task of its own: let it land if it was not held.
  await act(async () => {
    await new Promise((r) => setTimeout(r, 0))
  })
  expect(window.location.hash).toBe('#/instances/default')
  expect(
    within(main).getByRole('heading', { level: 1, name: 'default' }),
  ).toBeTruthy()
  expect(box(main, 'Port').value).toBe('8581')
  expect(document.activeElement).toBe(link)
})

test('1b.38: Discard changes goes on to the link', async () => {
  const { main } = await loaded()
  const link = await within(
    screen.getByRole('navigation', { name: en('nav-label') }),
  ).findByRole('link', { name: 'coder' })
  leaveBy(main, link)
  const d = dialog(en('discard-title'))
  fireEvent.click(
    within(d).getByRole('button', { name: en('discard-confirm') }),
  )
  await waitFor(() => expect(window.location.hash).toBe('#/instances/coder'))
  expect(
    await within(main).findByRole('heading', { level: 1, name: 'coder' }),
  ).toBeTruthy()
  expect(screen.queryByRole('dialog')).toBeNull()
  // The dropped edits leave nothing behind: the next page is clean.
  await within(main).findByRole('heading', {
    level: 3,
    name: en('server-network'),
  })
  fireEvent.click(navLink(en('nav-overview')))
  expect(screen.queryByRole('dialog'), 'the next link asks nothing').toBeNull()
  expect(
    await within(main).findByRole('heading', {
      level: 1,
      name: en('overview-title'),
    }),
  ).toBeTruthy()
})

test('1b.38: the product link asks too, and Discard changes lands on the Overview', async () => {
  const { main } = await loaded()
  leaveBy(main, productLink())
  const d = dialog(en('discard-title'))
  expect(window.location.hash).toBe('#/instances/default')
  fireEvent.click(
    within(d).getByRole('button', { name: en('discard-confirm') }),
  )
  expect(
    await within(main).findByRole('heading', {
      level: 1,
      name: en('overview-title'),
    }),
  ).toBeTruthy()
})

test('1b.38 (1a.15): Sign out while dirty asks first; Keep sends nothing, Discard signs out', async () => {
  const { bff, main } = await loaded()
  const signOut = within(screen.getByRole('banner')).getByRole('button', {
    name: en('session-sign-out'),
  })
  leaveBy(main, signOut)
  let d = dialog(en('discard-title'))
  fireEvent.click(within(d).getByRole('button', { name: en('discard-keep') }))
  await act(async () => {})
  expect(bff.to('DELETE', '/api/session')).toHaveLength(0)
  expect(document.activeElement).toBe(signOut)
  fireEvent.click(signOut)
  d = dialog(en('discard-title'))
  fireEvent.click(
    within(d).getByRole('button', { name: en('discard-confirm') }),
  )
  await waitFor(() => expect(bff.to('DELETE', '/api/session')).toHaveLength(1))
  const banner = screen.getByRole('banner')
  await waitFor(() =>
    expect(
      within(banner).queryByRole('button', { name: en('session-sign-in') }) ??
        within(banner).queryByRole('link', { name: en('session-sign-in') }),
    ).not.toBeNull(),
  )
})

test('1b.38a: Escape in the dialog opened by a link is Keep editing, focus back on the link', async () => {
  const { main } = await loaded()
  const link = navLink(en('nav-overview'))
  leaveBy(main, link)
  const d = dialog(en('discard-title'))
  fireEvent.keyDown(
    within(d).getByRole('button', { name: en('discard-keep') }),
    { key: 'Escape', code: 'Escape' },
  )
  await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull())
  expect(window.location.hash).toBe('#/instances/default')
  expect(box(main, 'Port').value).toBe('8581')
  expect(document.activeElement).toBe(link)
  // The dialog opens again on the next try.
  fireEvent.click(link)
  expect(dialog(en('discard-title'))).toBeTruthy()
})

test('1b.38a: Escape in the dialog opened by Sign out or Reload changes nothing, focus back on that control', async () => {
  const { bff, main } = await loaded({
    save: () => reply(409, { key: 'server-changed-elsewhere', args: [] }),
  })
  const escape = (d: HTMLElement) =>
    fireEvent.keyDown(
      within(d).getByRole('button', { name: en('discard-keep') }),
      { key: 'Escape', code: 'Escape' },
    )
  const signOut = within(screen.getByRole('banner')).getByRole('button', {
    name: en('session-sign-out'),
  })
  leaveBy(main, signOut)
  escape(dialog(en('discard-title')))
  await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull())
  expect(document.activeElement, 'back on Sign out').toBe(signOut)
  expect(bff.to('DELETE', '/api/session')).toHaveLength(0)

  await save(main, bff)
  const reload = await within(main).findByRole('button', {
    name: en('save-reload'),
  })
  reload.focus()
  fireEvent.click(reload)
  escape(dialog(en('discard-title')))
  await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull())
  expect(document.activeElement, 'back on Reload').toBe(
    within(main).getByRole('button', { name: en('save-reload') }),
  )
  expect(bff.to('GET', '/api/instances/default/server')).toHaveLength(1)
  expect(box(main, 'Port').value).toBe('8581')
  expectAlert(main, en('api-server-changed-elsewhere'))
  expect(window.location.hash).toBe('#/instances/default')
})

test('1b.38: a click on the link of the page already shown asks nothing and keeps the edits', async () => {
  const { main } = await loaded()
  const link = await within(
    screen.getByRole('navigation', { name: en('nav-label') }),
  ).findByRole('link', { name: 'default' })
  leaveBy(main, link)
  expect(screen.queryByRole('dialog')).toBeNull()
  await act(async () => {
    await new Promise((r) => setTimeout(r, 0))
  })
  expect(window.location.hash).toBe('#/instances/default')
  expect(box(main, 'Port').value).toBe('8581')
  expect(status(main).textContent).toBe(en('save-unsaved'))
})

test('1b.38: the beforeunload handler also sets returnValue to true', async () => {
  const { main } = await loaded()
  editPort(main)
  const e = new Event('beforeunload', { cancelable: true })
  let returned: unknown = undefined
  Object.defineProperty(e, 'returnValue', {
    get: () => returned,
    set: (v: unknown) => {
      returned = v
    },
  })
  window.dispatchEvent(e)
  expect(e.defaultPrevented).toBe(true)
  expect(returned).toBe(true)
})

test('1b.38: leaving a clean form asks nothing', async () => {
  const { main } = await loaded()
  fireEvent.click(navLink(en('nav-overview')))
  expect(screen.queryByRole('dialog')).toBeNull()
  expect(
    await within(main).findByRole('heading', {
      level: 1,
      name: en('overview-title'),
    }),
  ).toBeTruthy()
})

test('1b.38: beforeunload is armed only while the form is dirty', async () => {
  const unloadBlocked = () => {
    const e = new Event('beforeunload', { cancelable: true })
    window.dispatchEvent(e)
    return e.defaultPrevented
  }
  const { bff, main } = await loaded()
  expect(unloadBlocked(), 'clean on load').toBe(false)
  editPort(main)
  expect(unloadBlocked(), 'dirty').toBe(true)
  fireEvent.click(discardButton(main))
  expect(unloadBlocked(), 'after Discard').toBe(false)
  editPort(main)
  await save(main, bff)
  await waitFor(() => expect(status(main).textContent).toBe(en('save-saved')))
  expect(unloadBlocked(), 'after a save').toBe(false)
  editPort(main)
  type(box(main, 'Port'), '1234')
  cleanup()
  expect(unloadBlocked(), 'after the page is gone').toBe(false)
})

test('1b.39: Ctrl+S and Cmd+S do not save and are left to the browser', async () => {
  const { bff, main } = await loaded()
  editPort(main)
  for (const mod of [{ ctrlKey: true }, { metaKey: true }]) {
    for (const target of [box(main, 'Port'), document.body]) {
      const notPrevented = fireEvent.keyDown(target, {
        key: 's',
        code: 'KeyS',
        ...mod,
      })
      expect(notPrevented, JSON.stringify(mod)).toBe(true)
    }
  }
  await act(async () => {})
  expect(bff.to(...PUT)).toHaveLength(0)
})

// ── Keyboard, focus and live regions (section 6) ────────────────────────

test('1b 6: the save bar status is one live region, present before its text changes', async () => {
  const { bff, main } = await loaded()
  const line = status(main)
  expect(line.textContent).toBe('')
  editPort(main)
  expect(status(main)).toBe(line)
  expect(line.textContent).toBe(en('save-unsaved'))
  await save(main, bff)
  await waitFor(() => expect(line.textContent).toBe(en('save-saved')))
  expect(status(main)).toBe(line)
})

test('1b 6: Tab reaches the enabled controls field by field, then Discard and Save changes', async () => {
  const { main } = await loaded()
  const nav = screen.getByRole('navigation', { name: en('nav-label') })
  await within(nav).findByRole('link', { name: 'coder' })
  const about = (label: string) =>
    within(main).getByRole('button', { name: `${en('field-about')} ${label}` })
  const bind = group(main, 'Bind to').getAllByRole('checkbox')
  const expected: [string, HTMLElement][] = [
    ['About Port', about('Port')],
    ['Port default', check(main, 'Port', en('field-default'))],
    ['Port', box(main, 'Port')],
    ['About Bind to', about('Bind to')],
    ...bind.map((b, i) => [`Bind row ${i}`, b] as [string, HTMLElement]),
    ['About Base URL', about('Base URL')],
    ['Base URL default', check(main, 'Base URL', en('field-default'))],
    ['About API key', about('API key')],
    ['No key', check(main, 'API key', en('field-no-key'))],
    ['API key', apiKey(main)],
    ['Show', show(main)],
    ['About CPU threads', about('CPU threads')],
    ['CPU threads auto', check(main, 'CPU threads', en('field-auto'))],
    ['About Batch threads', about('Batch threads')],
    ['Batch threads auto', check(main, 'Batch threads', en('field-auto'))],
    ['Batch threads', box(main, 'Batch threads')],
    ['About Max models', about('Max models')],
    ['Max models default', check(main, 'Max models', en('field-default'))],
    ['About Memory margin', about('Memory margin')],
    [
      'Memory margin default',
      check(main, 'Memory margin', en('field-default')),
    ],
    ['Memory margin', box(main, 'Memory margin')],
    ['About Models folder', about('Models folder')],
    ['Models folder', box(main, 'Models folder')],
    ['About Web UI', about('Web UI')],
    ['Web UI', check(main, 'Web UI', en('server-web-ui-toggle'))],
    ['About Web UI MCP proxy', about('Web UI MCP proxy')],
    [
      'MCP proxy',
      check(main, 'Web UI MCP proxy', en('server-mcp-proxy-toggle')),
    ],
    ['About Log level', about('Log level')],
    ['Log level', select(main, 'Log level')],
    ['About Save on shutdown', about('Save on shutdown')],
    [
      'Save on shutdown',
      check(main, 'Save on shutdown', en('server-save-on-shutdown-toggle')),
    ],
    ['About Snapshot folder', about('Snapshot folder')],
    ['Snapshot folder', box(main, 'Snapshot folder')],
    ['About hipBLASLt', about('hipBLASLt')],
    ['hipBLASLt', select(main, 'hipBLASLt')],
    ['About Profiler attach', about('Profiler attach')],
    [
      'Profiler attach',
      check(main, 'Profiler attach', en('server-profiler-attach-toggle')),
    ],
    ['About Tensor-split AllReduce', about('Tensor-split AllReduce')],
    ['Tensor-split AllReduce', select(main, 'Tensor-split AllReduce')],
    ['About Disable GPU graphs', about('Disable GPU graphs')],
    [
      'Disable GPU graphs',
      check(main, 'Disable GPU graphs', en('server-gpu-graphs-toggle')),
    ],
    ['About KV rotation', about('KV rotation')],
    [
      'KV rotation',
      check(main, 'KV rotation', en('server-kv-rotation-toggle')),
    ],
    ['Discard', discardButton(main)],
    ['Save changes', saveButton(main)],
  ]
  const names = new Map(expected.map(([n, el]) => [el as Element, n]))
  const reached = [
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
    .map((el) => names.get(el) ?? el.outerHTML)
  expect(reached).toEqual(expected.map(([n]) => n))
})

// ── Answers that land after edits made during the PUT (33b) ─────────────

test('1b.33b, 33: Discard right after a 200 with kept edits goes back to the values just saved', async () => {
  const gate = deferred()
  const { bff, main } = await loaded({ save: () => gate.promise })
  editPort(main)
  await save(main, bff)
  type(box(main, 'Memory margin'), '2048')
  await act(async () => {
    gate.resolve(reply(200, { revision: 'r-2', running: false }))
  })
  await waitFor(() => expect(status(main).textContent).toBe(en('save-unsaved')))
  fireEvent.click(discardButton(main))
  expect(box(main, 'Port').value, 'the value saved').toBe('8581')
  expect(box(main, 'Memory margin').value, 'the value loaded').toBe('1024')
  expect(status(main).textContent).toBe('')
  expect(bff.to(...PUT)).toHaveLength(1)
})

test('1b.33b, 34: a 422 landing after an edit made during the PUT keeps the edit and marks the field', async () => {
  const gate = deferred()
  const { bff, main } = await loaded({ save: () => gate.promise })
  editPort(main)
  await save(main, bff)
  type(box(main, 'Memory margin'), '2048')
  await act(async () => {
    gate.resolve(
      reply(422, {
        key: 'client-address-in-use',
        args: ['coder'],
        field: 'Port',
      }),
    )
  })
  const text = en('api-client-address-in-use', 'coder')
  await within(main).findByText(en('save-refused', text))
  expectError(box(main, 'Port'), text, 'Port')
  expect(document.activeElement).toBe(box(main, 'Port'))
  expect(box(main, 'Port').value).toBe('8581')
  expect(box(main, 'Memory margin').value, 'the edit made during the PUT').toBe(
    '2048',
  )
  expect(status(main).textContent).toBe(en('save-unsaved'))
})

test('1b.33b, 35: a 409 landing after an edit made during the PUT keeps the edit', async () => {
  const gate = deferred()
  const { bff, main } = await loaded({ save: () => gate.promise })
  editPort(main)
  await save(main, bff)
  type(box(main, 'Memory margin'), '2048')
  await act(async () => {
    gate.resolve(reply(409, { key: 'server-changed-elsewhere', args: [] }))
  })
  await within(main).findByText(en('api-server-changed-elsewhere'))
  expect(box(main, 'Port').value).toBe('8581')
  expect(box(main, 'Memory margin').value).toBe('2048')
  expect(status(main).textContent).toBe(en('save-unsaved'))
})

test('1b.33b, 34a: a refusal on Hostname after Bind to changed during the PUT focuses the first row checked now', async () => {
  const gate = deferred()
  const { bff, main } = await loaded({ save: () => gate.promise })
  editPort(main)
  await save(main, bff)
  const bind = within(main).getByRole('group', { name: 'Bind to' })
  const rows = () => within(bind).getAllByRole('checkbox') as HTMLInputElement[]
  // Unchecks localhost while the PUT runs: 10.0.0.9 is now the first checked row.
  fireEvent.click(rows()[0]!)
  expect(rows().map((r) => r.checked)).toEqual([false, false, true, true])
  await act(async () => {
    gate.resolve(
      reply(422, {
        key: 'client-unresolvable-host',
        args: ['gpu-box.lan'],
        field: 'Hostname',
      }),
    )
  })
  const text = en('api-client-unresolvable-host', 'gpu-box.lan')
  await within(main).findByText(en('save-refused', text))
  for (const i of [2, 3]) expectError(rows()[i]!, text, `checked row ${i}`)
  expectNoError(rows()[0]!, 'the row unchecked during the PUT')
  expect(document.activeElement, 'the first row checked now').toBe(rows()[2])
})

// ── Review additions (3.1, 30, 30a) ─────────────────────────────────────

/**
 * Records, at each focusin, the target's aria-invalid and the texts its
 * aria-describedby names: what a screen reader announces at the focus move.
 */
function recordFocus() {
  const seen: {
    target: EventTarget | null
    invalid: string | null
    described: string[]
  }[] = []
  const listener = (e: FocusEvent) => {
    const el = e.target as HTMLElement
    seen.push({
      target: el,
      invalid: el.getAttribute?.('aria-invalid') ?? null,
      described: described(el),
    })
  }
  document.addEventListener('focusin', listener)
  return {
    seen,
    stop: () => document.removeEventListener('focusin', listener),
  }
}

test('1b 3.1: after failing client checks the error is rendered when focus reaches the control', async () => {
  const { main } = await loaded()
  type(box(main, 'Port'), 'abc')
  const focus = recordFocus()
  try {
    fireEvent.click(saveButton(main))
    await within(main).findByText(en('save-invalid'))
  } finally {
    focus.stop()
  }
  const port = box(main, 'Port')
  const at = focus.seen.find((s) => s.target === port)
  expect(at, 'focus reached Port').toBeDefined()
  expect(at!.invalid, 'aria-invalid at the focus move').toBe('true')
  expect(at!.described, 'the error at the focus move').toContain(
    en('field-error-number', '1', '65535'),
  )
})

test('1b 3.1: after a 422 the error is rendered when focus reaches the field', async () => {
  const { bff, main } = await loaded({
    save: () =>
      reply(422, {
        key: 'client-address-in-use',
        args: ['coder'],
        field: 'Port',
      }),
  })
  editPort(main)
  const focus = recordFocus()
  try {
    await save(main, bff)
    await within(main).findByText(
      en('save-refused', en('api-client-address-in-use', 'coder')),
    )
  } finally {
    focus.stop()
  }
  const port = box(main, 'Port')
  const at = focus.seen.find((s) => s.target === port)
  expect(at, 'focus reached Port').toBeDefined()
  expect(at!.invalid, 'aria-invalid at the focus move').toBe('true')
  expect(at!.described, 'the error at the focus move').toContain(
    en('api-client-address-in-use', 'coder'),
  )
})

test('1b.30, 30a: Discard clears the field errors of a refusal as well as the alert', async () => {
  const { bff, main } = await loaded({
    save: () =>
      reply(422, {
        key: 'client-address-in-use',
        args: ['coder'],
        field: 'Port',
      }),
  })
  editPort(main)
  await save(main, bff)
  const text = en('api-client-address-in-use', 'coder')
  await within(main).findByText(en('save-refused', text))
  fireEvent.click(discardButton(main))
  expectNoError(box(main, 'Port'), 'Port after Discard')
  expect(within(main).queryByText(text)).toBeNull()
  expectNoAlert(main, 'after Discard')
})

test('1b.30, 31: Discard clears the errors of failed client checks as well as the alert', async () => {
  const { main } = await loaded()
  type(box(main, 'Port'), 'abc')
  type(box(main, 'Batch threads'), '33')
  fireEvent.click(saveButton(main))
  await within(main).findByText(en('save-invalid'))
  fireEvent.click(discardButton(main))
  expectNoError(box(main, 'Port'), 'Port after Discard')
  expectNoError(box(main, 'Batch threads'), 'Batch threads after Discard')
  expect(
    within(main).queryByText(en('field-error-number', '1', '65535')),
  ).toBeNull()
  expect(
    within(main).queryByText(en('field-error-number', '1', '32')),
  ).toBeNull()
  expectNoAlert(main, 'after Discard')
})
