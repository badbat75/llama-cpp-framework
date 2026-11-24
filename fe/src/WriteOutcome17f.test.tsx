// Brief 16, slice 17f (fe/briefs/16-server-profiles-and-instances.md), criterion
// 17f.10: a 504 `client-timeout` is "not confirmed" for every write, never
// `request-failed`, and the place that got it is read again. The whole app
// renders against a fake bff (mocked fetch). The label's own 504 is in
// InstanceRename17f.test.tsx; 17d's and 17e's writes have no page yet, so they
// are not pinned here. "16 17f.10" is the brief's criterion.
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
  en,
  fakeBff,
  reply,
  serverProfileDoc,
  startAt,
  stubDialog,
  type BffOptions,
  type Instance,
  type ServerProfile,
} from './test/bff.ts'

/** The instance every instance-page test shows: stopped, so Start is live. */
const ITEM: Instance = {
  id: 'default',
  engine: 'llama.cpp',
  state: 'inactive',
  port: 9931,
  tags: ['edge'],
  instanceTags: ['v2'],
  label: 'Coding assistant',
}

const LIST = '/api/instances'
const BOOT = '/api/instances/default/boot'
const POOL = '/api/profiles/server'
const PROFILE = '/api/profiles/server/qwen-27b'
const PROFILE_ID = 'qwen-27b'

/** The 504 of the bff's W1 and W2: the call reached the manager. */
const TIMEOUT = () => reply(504, { key: 'client-timeout', args: [] })
/** A 503: the manager did not answer at all, so nothing was done. */
const UNAVAILABLE = () => reply(503, { key: 'client-unavailable', args: [] })

let dialogs: ReturnType<typeof stubDialog>

beforeEach(() => {
  startAt('#/instances/default')
  dialogs = stubDialog()
})

afterEach(async () => {
  // A link click queues its hashchange as a task: let it land on this test's
  // page, not on the next test's.
  await flushTasks()
  cleanup()
  vi.unstubAllGlobals()
  dialogs.restore()
  startAt('')
})

// ── Helpers ─────────────────────────────────────────────────────────────

/** Lets jsdom's queued tasks (a hashchange, a dialog closing) run. */
async function flushTasks(n = 2) {
  await act(async () => {
    for (let i = 0; i < n; i++) await new Promise((r) => setTimeout(r, 0))
  })
}

/** The app at `hash`, loaded to the point its content shows. */
async function open(hash: string, options: BffOptions = {}) {
  startAt(hash)
  const bff = fakeBff({
    instances: [ITEM],
    boot: { read: () => reply(200, { enabled: false }) },
    ...options,
  })
  render(<App />)
  const main = screen.getByRole('main')
  return { bff, main }
}

/** The instance page, with its run controls, Start at boot and tags field. */
async function instancePage(options: BffOptions = {}) {
  const page = await open('#/instances/default', options)
  await within(page.main).findByRole('button', { name: en('run-start') })
  // The two fields are read with the header: waiting for them keeps every
  // write of this file on the same point of the page.
  await within(page.main).findByRole('group', { name: en('boot-label') })
  await within(page.main).findByRole('group', {
    name: en('instance-tags-own'),
  })
  return page
}

/** The text of every alert of one class, as an array. */
function alertsOf(main: HTMLElement, className: string): string[] {
  return within(main)
    .queryAllByRole('alert')
    .filter((a) => a.classList.contains(className))
    .map((a) => a.textContent ?? '')
}

/** Focuses `el` and clicks it, as a pointer, Enter or Space does. */
function press(el: HTMLElement) {
  el.focus()
  fireEvent.click(el)
}

/** Types `value` in a box. */
function type(input: HTMLElement, value: string) {
  fireEvent.change(input, { target: { value } })
}

/** The reads of one route the bff saw. */
const reads = (bff: ReturnType<typeof fakeBff>, path: string) =>
  bff.to('GET', path)

// ── The run verbs ───────────────────────────────────────────────────────

test('16 17f.10: a 504 client-timeout on Start, Stop or Restart shows write-timeout in the run alert, never request-failed, and reads the list again', async () => {
  for (const verb of ['start', 'stop', 'restart'] as const) {
    // The verb needs its state: Start on a stopped instance, the others on a
    // running one.
    const { bff, main } = await instancePage({
      instances: [{ ...ITEM, state: verb === 'start' ? 'inactive' : 'active' }],
      run: () => TIMEOUT(),
    })
    const before = reads(bff, LIST).length
    press(within(main).getByRole('button', { name: en(`run-${verb}`) }))
    const alert = await within(main).findByText(en('write-timeout'))
    expect(
      alert.classList.contains('run-alert'),
      `${verb}: the text is the run alert's`,
    ).toBe(true)
    expect(
      alertsOf(main, 'run-alert'),
      `${verb}: one line, and it is not the plain failure`,
    ).toEqual([en('write-timeout')])
    expect(
      alertsOf(main, 'run-alert').includes(en('request-failed')),
      `${verb}: no 504 reaches request-failed`,
    ).toBe(false)
    await flushTasks()
    expect(
      reads(bff, LIST).length,
      `${verb}: the list is read again, so the state shown is the manager's`,
    ).toBeGreaterThan(before)
    cleanup()
    vi.unstubAllGlobals()
  }
})

test('16 17f.10: a 503 on a run verb stays api-client-unavailable, and a 502 client-other stays request-failed', async () => {
  for (const [what, answer, text] of [
    ['503 client-unavailable', UNAVAILABLE, en('api-client-unavailable')],
    [
      '502 client-other',
      () => reply(502, { key: 'client-other', args: [] }),
      en('request-failed'),
    ],
  ] as Array<[string, () => Response, string]>) {
    const { main } = await instancePage({ run: () => answer() })
    press(within(main).getByRole('button', { name: en('run-start') }))
    const alert = await within(main).findByText(text)
    expect(
      alert.classList.contains('run-alert'),
      `${what}: the run alert says it`,
    ).toBe(true)
    expect(alertsOf(main, 'run-alert'), `${what}: that line alone`).toEqual([
      text,
    ])
    cleanup()
    vi.unstubAllGlobals()
  }
})

// ── Start at boot ───────────────────────────────────────────────────────

test('16 17f.10: a 504 on the Start at boot write shows write-timeout in the field’s alert, never request-failed, and reads the boot state again', async () => {
  const { bff, main } = await instancePage({
    boot: {
      read: () => reply(200, { enabled: false }),
      write: () => TIMEOUT(),
    },
  })
  const group = within(main).getByRole('group', { name: en('boot-label') })
  const box = within(group).getByRole('checkbox', {
    name: en('boot-toggle'),
  }) as HTMLInputElement
  const before = reads(bff, BOOT).length
  press(box)
  const alert = await within(group).findByText(en('write-timeout'))
  expect(
    alert.classList.contains('boot-alert'),
    'the text is the field’s alert',
  ).toBe(true)
  expect(
    within(group)
      .queryAllByRole('alert')
      .map((a) => a.textContent ?? ''),
    'one line, and it is not the plain failure',
  ).toEqual([en('write-timeout')])
  await flushTasks()
  expect(
    reads(bff, BOOT).length,
    'the boot state is read again, so the box shows what the manager holds',
  ).toBeGreaterThan(before)
})

test('16 17f.10: a 503 on the Start at boot write stays api-client-unavailable', async () => {
  const { main } = await instancePage({
    boot: {
      read: () => reply(200, { enabled: false }),
      write: () => UNAVAILABLE(),
    },
  })
  const group = within(main).getByRole('group', { name: en('boot-label') })
  press(
    within(group).getByRole('checkbox', {
      name: en('boot-toggle'),
    }) as HTMLInputElement,
  )
  const alert = await within(group).findByText(en('api-client-unavailable'))
  expect(alert.classList.contains('boot-alert'), 'the field’s alert').toBe(true)
})

// ── The tags field ──────────────────────────────────────────────────────

/** Opens the add pill and returns its box. */
async function openPill(main: HTMLElement): Promise<HTMLInputElement> {
  press(within(main).getByRole('button', { name: en('instance-tag-add') }))
  await flushTasks()
  return within(main).getByRole('combobox', {
    name: en('instance-tag-new'),
  }) as HTMLInputElement
}

test('16 17f.10: a 504 on a tag add shows write-timeout in the tags field’s alert, never request-failed, keeps what was typed, and reads the list again', async () => {
  const { bff, main } = await instancePage({ instanceTags: () => TIMEOUT() })
  const box = await openPill(main)
  type(box, 'fast')
  const before = reads(bff, LIST).length
  fireEvent.keyDown(box, { key: 'Enter', code: 'Enter' })
  const alert = await within(main).findByText(en('write-timeout'))
  expect(
    alert.classList.contains('tags-alert'),
    'the text is the field’s alert',
  ).toBe(true)
  expect(
    alertsOf(main, 'tags-alert'),
    'one line, and it is not the plain failure',
  ).toEqual([en('write-timeout')])
  expect(
    within(main)
      .getByRole('combobox', {
        name: en('instance-tag-new'),
      })
      .getAttribute('value'),
    'the pill keeps what was typed',
  ).toBe('fast')
  await flushTasks()
  expect(
    reads(bff, LIST).length,
    'the list is read again, so the chips show what the manager holds',
  ).toBeGreaterThan(before)
})

test('16 17f.10: a 503 on a tag add stays api-client-unavailable', async () => {
  const { main } = await instancePage({ instanceTags: () => UNAVAILABLE() })
  const box = await openPill(main)
  type(box, 'fast')
  fireEvent.keyDown(box, { key: 'Enter', code: 'Enter' })
  const alert = await within(main).findByText(en('api-client-unavailable'))
  expect(alert.classList.contains('tags-alert'), 'the field’s alert').toBe(true)
})

// ── A create (17b) ──────────────────────────────────────────────────────

/** One row of the pool. */
const row = (id: string, instance: string | null): ServerProfile => ({
  id,
  engine: 'llama.cpp',
  port: 8580,
  tags: [],
  instance,
})

/** The pool page, with its create form typed and Create pressed. */
async function createForm(answer: () => Response) {
  startAt('#/profiles/server')
  const bff = fakeBff({
    serverProfiles: [row('default', 'default'), row(PROFILE_ID, null)],
    serverProfileCreate: () => answer(),
  })
  render(<App />)
  const main = screen.getByRole('main')
  await within(main).findByRole('table')
  press(within(main).getByRole('button', { name: en('server-profiles-new') }))
  const form = await within(main).findByRole('form', {
    name: en('server-profiles-new-title'),
  })
  type(
    within(form).getByRole('textbox', { name: en('server-profile-name') }),
    'edge-box',
  )
  type(within(form).getByRole('textbox', { name: en('server-port') }), '9932')
  press(within(form).getByRole('button', { name: en('create-button') }))
  return { bff, main, form }
}

test('16 17f.10: a 504 on a create shows create-unknown in the form’s alert, never request-failed, and reads the pool again, as a 503 does', async () => {
  for (const [what, answer] of [
    ['504 client-timeout', TIMEOUT],
    ['503 client-unavailable', UNAVAILABLE],
  ] as Array<[string, () => Response]>) {
    const { bff, main, form } = await createForm(answer)
    const alert = await within(form).findByText(en('create-unknown'))
    expect(
      alert.closest('[role="alert"]'),
      `${what}: the text is in a role="alert" of the form`,
    ).not.toBeNull()
    expect(
      [...form.querySelectorAll('[role="alert"]')].map(
        (a) => a.textContent ?? '',
      ),
      `${what}: one line, and it is not the plain failure`,
    ).toEqual([en('create-unknown')])
    await within(main).findByRole('table')
    expect(
      reads(bff, POOL).length,
      `${what}: the pool is read again, so the row shows what the manager holds`,
    ).toBeGreaterThanOrEqual(2)
    cleanup()
    vi.unstubAllGlobals()
  }
})

// ── A delete (17b) ──────────────────────────────────────────────────────

/** The profile page of a free profile, with its delete confirmed. */
async function confirmDelete(answer: () => Response) {
  startAt(`#/profiles/server/${PROFILE_ID}`)
  const bff = fakeBff({
    serverProfile: () => reply(200, serverProfileDoc({}, { instance: null })),
    serverProfileDelete: () => answer(),
  })
  render(<App />)
  const main = screen.getByRole('main')
  await within(main).findByRole('heading', {
    level: 3,
    name: en('server-network'),
  })
  press(within(main).getByRole('button', { name: en('server-profile-delete') }))
  const group = await within(main).findByRole('group', {
    name: en('server-profile-delete-question', PROFILE_ID),
  })
  press(within(group).getByRole('button', { name: en('confirm-delete') }))
  return { bff, main, group }
}

test('16 17f.10: a 504 on a delete shows delete-unknown in the group’s alert, never request-failed, and reads the document again', async () => {
  const { bff, group } = await confirmDelete(TIMEOUT)
  const alert = await within(group).findByText(en('delete-unknown'))
  expect(
    group.contains(alert) && alert.closest('[role="alert"]') !== null,
    'the text is in a role="alert" of the group',
  ).toBe(true)
  expect(
    [...group.querySelectorAll('[role="alert"]')].map(
      (a) => a.textContent ?? '',
    ),
    'one line, and it is not the plain failure',
  ).toEqual([en('delete-unknown')])
  await flushTasks()
  expect(
    reads(bff, PROFILE).length,
    'the document is read again, so the page shows what the manager holds',
  ).toBeGreaterThanOrEqual(2)
})

test('16 17f.10: a 503 on a delete stays api-client-unavailable and reads nothing again', async () => {
  const { bff, group } = await confirmDelete(UNAVAILABLE)
  const alert = await within(group).findByText(en('api-client-unavailable'))
  expect(
    [...group.querySelectorAll('[role="alert"]')].map(
      (a) => a.textContent ?? '',
    ),
    'the field’s alert, unchanged',
  ).toEqual([en('api-client-unavailable')])
  expect(alert.closest('[role="alert"]'), 'a role="alert"').not.toBeNull()
  expect(reads(bff, PROFILE), 'nothing is read again').toHaveLength(1)
})
