// Brief 16, slice 17b (fe/briefs/16-server-profiles-and-instances.md): creating
// a server profile from the pool list page. The list page, the nav and the table
// are ServerProfiles1.test.tsx; a profile's page and its delete are
// ServerProfilePage2.test.tsx; 17b.10 (the strings file gains exactly 17b's keys)
// is `1a strings` in App.test.tsx, which compares that file against the test's EN
// map, where 17b's keys are listed. Test names cite the brief first, `16 17b.1`,
// because brief 17's UX pass owns the bare `17b.n` names.
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
  startAt,
  stubDialog,
  type BffOptions,
  type ServerProfile,
} from './test/bff.ts'

const LIST = '/api/profiles/server'

let dialogs: ReturnType<typeof stubDialog>

beforeEach(() => {
  startAt('#/profiles/server')
  dialogs = stubDialog()
})

afterEach(() => {
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

/** One row of GET /api/profiles/server. */
function row(
  id: string,
  port = 8580,
  instance: string | null = 'default',
): ServerProfile {
  return { id, engine: 'llama.cpp', port, tags: [], instance }
}

/** The two rows most tests read: one an instance runs, one free. */
const ROWS = [row('default', 8580, 'default'), row('qwen-27b', 9931, null)]

/** The app at #/profiles/server; returns the fake bff and main. */
function open(options: BffOptions = {}) {
  const bff = fakeBff(options)
  render(<App />)
  return { bff, main: screen.getByRole('main') }
}

/** The app with the pool read answered (a table, or the empty text). */
async function loaded(options: BffOptions = {}) {
  const r = open(options)
  await waitFor(() => {
    if (within(r.main).queryByRole('table') !== null) return
    if (within(r.main).queryByText(en('server-profiles-empty')) !== null) return
    throw new Error('the pool read has not answered')
  })
  return r
}

function press(el: HTMLElement) {
  el.focus()
  fireEvent.click(el)
}

function type(input: HTMLElement, value: string) {
  fireEvent.change(input, { target: { value } })
}

/** The opener of the create form. */
function newButton(main: HTMLElement): HTMLElement {
  return within(main).getByRole('button', { name: en('server-profiles-new') })
}

/** The create form, once the opener is pressed. */
async function openForm(main: HTMLElement): Promise<HTMLElement> {
  press(newButton(main))
  return within(main).findByRole('form', {
    name: en('server-profiles-new-title'),
  })
}

/** A control of the create form, by its label. */
function boxIn(form: HTMLElement, label: string): HTMLInputElement {
  return within(form).getByRole('textbox', { name: label }) as HTMLInputElement
}

function buttonIn(form: HTMLElement, key: string): HTMLElement {
  return within(form).getByRole('button', { name: en(key) })
}

/** The form's status line (17b.1: the form holds one). */
function formStatus(form: HTMLElement): HTMLElement {
  const all = [...form.querySelectorAll<HTMLElement>('[role="status"]')]
  expect(all, 'the form has one status line').toHaveLength(1)
  return all[0]!
}

/** The text of every `role="alert"` inside `root`. */
function alertTexts(root: HTMLElement): string[] {
  return [...root.querySelectorAll('[role="alert"]')].map(
    (a) => a.textContent ?? '',
  )
}

/** The text of every element named by `el`'s `aria-describedby`. */
function described(el: HTMLElement): string[] {
  return (el.getAttribute('aria-describedby') ?? '')
    .split(/\s+/)
    .filter(Boolean)
    .map((id) => document.getElementById(id)?.textContent ?? `(no #${id})`)
}

/** The list reads the bff was asked for, in order. */
function listReads(bff: ReturnType<typeof fakeBff>) {
  return bff.to('GET', LIST)
}

/** The creates the bff was asked for, in order. */
function posts(bff: ReturnType<typeof fakeBff>) {
  return bff.to('POST', LIST)
}

/** What `el` says about itself at the moment the focus lands on it: whether it
 *  was focused at all, the text of what its `aria-describedby` named then, and
 *  whether `text` was in the form then. The 17b review's D1 (17b.3, 17b.5): the
 *  focus must move after the error rendered, so a screen reader at the field
 *  reads "Name, edit text, Enter a value." and not "Name, edit text". */
function watchFocus(el: HTMLElement, form: HTMLElement, text: string) {
  const at = { focused: false, described: [] as string[], inForm: false }
  el.addEventListener('focus', () => {
    at.focused = true
    at.described = described(el)
    at.inForm = (form.textContent ?? '').includes(text)
  })
  return at
}

/** Signs in as `user` in the open sign-in dialog. */
async function signInInDialog(user: string) {
  const d = screen.getByRole('dialog', { name: en('signin-title') })
  expect(d.tagName, 'a native <dialog>').toBe('DIALOG')
  expect(dialogs.opened, 'opened with showModal()').toContain(d)
  type(within(d).getByRole('textbox', { name: en('signin-username') }), user)
  type(
    within(d).getByLabelText(en('signin-password'), { selector: 'input' }),
    'hunter2',
  )
  fireEvent.click(within(d).getByRole('button', { name: en('signin-submit') }))
  await flushTasks()
  expect(screen.queryByRole('dialog'), 'the dialog closed').toBeNull()
}

// ── 17b.1: the create form opens ────────────────────────────────────────

test('16 17b.1: the list page puts a secondary server-profiles-new button with the plus icon and aria-expanded false after the intro and before the table, and pressing it opens right after it a form named by its h2, with the Name and Port fields and their hints, Create primary then Cancel, one empty status line, no alert, and the focus on Name', async () => {
  const { bff, main } = await loaded({ serverProfiles: ROWS })
  const t = within(main).getByRole('table')
  const intro = within(main).getByText(en('server-profiles-intro'))
  const box = t.closest('div.table-scroll')
  expect(box, 'the table is in its scrolling box').not.toBeNull()
  const opener = newButton(main)
  expect(opener.classList.contains('secondary'), 'a secondary button').toBe(
    true,
  )
  expect(opener.getAttribute('aria-expanded'), 'a disclosure, closed').toBe(
    'false',
  )
  const icon = opener.querySelector('svg.icon')
  expect(icon, 'the opener carries its icon').not.toBeNull()
  expect(icon!.getAttribute('aria-hidden'), 'decorative').toBe('true')
  expect(
    [...icon!.querySelectorAll('path')].map((p) => p.getAttribute('d')),
    'the plus of Icon.tsx (brief 15)',
  ).toEqual(['M5 12h14', 'M12 5v14'])
  expect(before(intro, opener), 'the opener is after the intro').toBe(true)
  expect(before(opener, box!), 'and before the table').toBe(true)
  // Opening.
  press(opener)
  expect(
    opener.getAttribute('aria-expanded'),
    'the press opens the disclosure',
  ).toBe('true')
  const form = await within(main).findByRole('form', {
    name: en('server-profiles-new-title'),
  })
  const h2 = within(form).getByRole('heading', {
    level: 2,
    name: en('server-profiles-new-title'),
  })
  expect(h2.getAttribute('id'), 'the form is named by its h2').toBe(
    form.getAttribute('aria-labelledby'),
  )
  expect(opener.nextElementSibling, 'the form is right after the opener').toBe(
    form,
  )
  expect(before(form, box!), 'and the form is before the table').toBe(true)
  // The fields.
  const name = boxIn(form, en('server-profile-name'))
  // The opening's focus, read before the hint toggles below take it.
  expect(document.activeElement, 'the focus moves to Name').toBe(name)
  expect(name.tagName, 'Name is a text input').toBe('INPUT')
  expect(name.getAttribute('type') ?? 'text', 'a plain text input').toBe('text')
  expect(
    name.getAttribute('autocomplete'),
    'the browser does not fill a name',
  ).toBe('off')
  const port = boxIn(form, en('server-port'))
  expect(port.getAttribute('inputmode'), 'Port is a numeric keypad').toBe(
    'numeric',
  )
  // The hints: in the DOM, shown by the field's About toggle (DESIGN 5.2).
  for (const [label, key] of [
    [en('server-profile-name'), en('server-profile-name-hint')],
    [en('server-port'), en('server-profile-port-hint')],
  ] as const) {
    const hint = within(form).getByText(key)
    expect(isHidden(hint), `${label}: the hint starts closed`).toBe(true)
    press(
      within(form).getByRole('button', {
        name: `${en('field-about')} ${label}`,
      }),
    )
    expect(isHidden(hint), `${label}: About shows the hint`).toBe(false)
  }
  // The buttons, the status line and the alert place.
  const create = buttonIn(form, 'create-button')
  const cancel = buttonIn(form, 'create-cancel')
  expect(create.classList.contains('primary'), 'Create is the primary').toBe(
    true,
  )
  expect(before(name, create), 'Create is after the fields').toBe(true)
  expect(before(create, cancel), 'Cancel is after Create').toBe(true)
  expect(formStatus(form).textContent, 'the status line is empty at open').toBe(
    '',
  )
  expect(alertTexts(form), 'no alert: the place is empty').toEqual([])
  expect(posts(bff), 'opening sends nothing').toHaveLength(0)
  expect(listReads(bff), 'and reads nothing').toHaveLength(1)
})

test('16 17b.1: every time the form opens Port holds the smallest whole number from 9931 up that no row of the list the page last read has, a row with port 0 counting for nothing, computed with no request; the administrator may change or clear it', async () => {
  const cases: [string, ServerProfile[], string][] = [
    // The four lists of the criterion. The second carries a row whose file
    // cannot be read (port 0), which counts for nothing.
    ['no rows', [], '9931'],
    [
      '9931, 9932 and an unreadable file',
      [row('a', 9931), row('b', 9932), row('c', 0)],
      '9933',
    ],
    ['9931 and 9933', [row('a', 9931), row('b', 9933)], '9932'],
    ['9930 only', [row('a', 9930)], '9931'],
  ]
  for (const [what, rows, want] of cases) {
    const { bff, main } = await loaded({ serverProfiles: rows })
    const form = await openForm(main)
    const port = boxIn(form, en('server-port'))
    expect(port.value, `${what}: Port holds ${want}`).toBe(want)
    expect(
      listReads(bff),
      `${what}: the default asks for nothing`,
    ).toHaveLength(1)
    expect(posts(bff), `${what}: nothing is sent`).toHaveLength(0)
    // The administrator may change it, and clear it: the check is on Create.
    type(port, '9931')
    expect(port.value, `${what}: it can be changed`).toBe('9931')
    type(port, '')
    expect(port.value, `${what}: it can be cleared`).toBe('')
    cleanup()
    vi.unstubAllGlobals()
  }
})

test('16 17b.1: the opener is there when the list is empty, before the empty text, and is not there while the list is loading or after a failed read', async () => {
  // The empty pool.
  const empty = await loaded({ serverProfiles: [] })
  const opener = newButton(empty.main)
  expect(
    before(opener, within(empty.main).getByText(en('server-profiles-empty'))),
    'the opener is before the empty text',
  ).toBe(true)
  const form = await openForm(empty.main)
  expect(boxIn(form, en('server-port')).value, 'and its default').toBe('9931')
  cleanup()
  vi.unstubAllGlobals()
  // While the read is open.
  const gate = deferred()
  const loading = open({ serverProfiles: () => gate.promise })
  await within(loading.main).findByRole('heading', {
    level: 1,
    name: en('server-profiles-title'),
  })
  expect(
    within(loading.main).queryByRole('button', {
      name: en('server-profiles-new'),
    }),
    'no opener while loading',
  ).toBeNull()
  gate.resolve(reply(200, ROWS))
  cleanup()
  vi.unstubAllGlobals()
  // After a failed read.
  const failed = open({
    serverProfiles: () => reply(503, { key: 'client-unavailable', args: [] }),
  })
  await within(failed.main).findByText(en('api-client-unavailable'))
  expect(
    within(failed.main).queryByRole('button', {
      name: en('server-profiles-new'),
    }),
    'no opener after a failure',
  ).toBeNull()
})

// ── 17b.2: Cancel ───────────────────────────────────────────────────────

test('16 17b.2: Cancel closes the form, clears its fields, its errors and its alert, and focuses the opener with aria-expanded false; pressing the opener again closes it too; opening fills Port with the default anew', async () => {
  const { main } = await loaded({
    serverProfiles: ROWS,
    serverProfileCreate: () =>
      reply(409, { key: 'client-server-profile-exists', args: ['edge-box'] }),
  })
  const opener = newButton(main)
  let form = await openForm(main)
  type(boxIn(form, en('server-profile-name')), 'edge-box')
  press(buttonIn(form, 'create-button'))
  const alert = await within(form).findByText(
    en('api-client-server-profile-exists', 'edge-box'),
  )
  expect(
    alert.closest('[role="alert"]'),
    'the refusal is an alert',
  ).not.toBeNull()
  // Cancel: closed, cleared, focus on the opener.
  press(buttonIn(form, 'create-cancel'))
  expect(within(main).queryByRole('form'), 'the form is gone').toBeNull()
  expect(opener.getAttribute('aria-expanded'), 'the opener is closed').toBe(
    'false',
  )
  expect(document.activeElement, 'the focus is on the opener').toBe(opener)
  // Opening: empty, the default anew, no alert, no error.
  form = await openForm(main)
  expect(boxIn(form, en('server-profile-name')).value, 'Name is empty').toBe('')
  expect(boxIn(form, en('server-port')).value, 'Port is the default').toBe(
    '9932',
  )
  expect(alertTexts(form), 'the alert is gone').toEqual([])
  expect(
    within(form).queryByText(en('field-error-required')),
    'no error',
  ).toBeNull()
  // An error, then the opener closes it and the next opening is clean.
  type(boxIn(form, en('server-port')), '9999')
  press(buttonIn(form, 'create-button'))
  await within(form).findByText(en('field-error-required'))
  press(opener)
  expect(within(main).queryByRole('form'), 'the opener closed it').toBeNull()
  expect(document.activeElement, 'focus on the opener').toBe(opener)
  form = await openForm(main)
  expect(
    within(form).queryByText(en('field-error-required')),
    'the error is gone',
  ).toBeNull()
  expect(boxIn(form, en('server-profile-name')).value, 'Name is empty').toBe('')
  expect(boxIn(form, en('server-port')).value, 'Port is the default anew').toBe(
    '9932',
  )
})

// ── 17b.3: client checks ────────────────────────────────────────────────

test('16 17b.3: Create with an empty Name shows field-error-required under Name, marks Name aria-invalid and described by it, moves the focus there, and sends nothing', async () => {
  const { bff, main } = await loaded({ serverProfiles: ROWS })
  const form = await openForm(main)
  const name = boxIn(form, en('server-profile-name'))
  const port = boxIn(form, en('server-port'))
  press(buttonIn(form, 'create-button'))
  const error = await within(form).findByText(en('field-error-required'))
  expect(before(name, error), 'the error is under the control').toBe(true)
  expect(name.getAttribute('aria-invalid'), 'Name is invalid').toBe('true')
  expect(described(name), 'Name is described by its error').toContain(
    en('field-error-required'),
  )
  expect(document.activeElement, 'the focus moves to Name').toBe(name)
  expect(port.getAttribute('aria-invalid'), 'Port is not marked').toBeNull()
  expect(within(form).queryAllByRole('alert'), 'no alert').toHaveLength(0)
  expect(posts(bff), 'nothing is sent').toHaveLength(0)
})

test('16 17b.3: Create with Port cleared, or a value that is not a whole number from 1 to 65535, shows field-error-number with 1 and 65535 under Port, marks and focuses it, and sends nothing', async () => {
  const want = en('field-error-number', '1', '65535')
  for (const value of ['', '9931.5', 'abc', '0', '65536', '-1']) {
    const { bff, main } = await loaded({ serverProfiles: ROWS })
    const form = await openForm(main)
    const name = boxIn(form, en('server-profile-name'))
    const port = boxIn(form, en('server-port'))
    type(name, 'edge-box')
    type(port, value)
    press(buttonIn(form, 'create-button'))
    const error = await within(form).findByText(want)
    expect(before(port, error), `${value}: the error is under Port`).toBe(true)
    expect(port.getAttribute('aria-invalid'), `${value}: Port is invalid`).toBe(
      'true',
    )
    expect(described(port), `${value}: described by its error`).toContain(want)
    expect(
      name.getAttribute('aria-invalid'),
      `${value}: Name is not marked`,
    ).toBeNull()
    expect(document.activeElement, `${value}: the focus moves to Port`).toBe(
      port,
    )
    expect(posts(bff), `${value}: nothing is sent`).toHaveLength(0)
    cleanup()
    vi.unstubAllGlobals()
  }
})

test('16 17b.3: with Name empty and Port cleared both are marked and each shows its own error, and the focus moves to Name, the first of them', async () => {
  const { bff, main } = await loaded({ serverProfiles: ROWS })
  const form = await openForm(main)
  const name = boxIn(form, en('server-profile-name'))
  const port = boxIn(form, en('server-port'))
  type(port, '')
  press(buttonIn(form, 'create-button'))
  await within(form).findByText(en('field-error-number', '1', '65535'))
  expect(
    within(form).getByText(en('field-error-required')),
    'Name speaks too',
  ).toBeTruthy()
  expect(name.getAttribute('aria-invalid'), 'Name is invalid').toBe('true')
  expect(port.getAttribute('aria-invalid'), 'Port is invalid').toBe('true')
  expect(
    document.activeElement,
    'the focus is on the first invalid field',
  ).toBe(name)
  expect(posts(bff), 'nothing is sent').toHaveLength(0)
})

// ── 17b.4: the request ──────────────────────────────────────────────────

test('16 17b.3: a client check moves the focus to the field after its error rendered: the error is in the DOM, and named by the field, at the moment the focus lands on it (17b review D1)', async () => {
  // Name empty: the focus goes to Name.
  {
    const { main } = await loaded({ serverProfiles: ROWS })
    const form = await openForm(main)
    const name = boxIn(form, en('server-profile-name'))
    const text = en('field-error-required')
    const at = watchFocus(name, form, text)
    press(buttonIn(form, 'create-button'))
    await within(form).findByText(text)
    expect(at.focused, 'the focus landed on Name').toBe(true)
    expect(at.inForm, 'the error was in the DOM when the focus landed').toBe(
      true,
    )
    expect(at.described, 'Name named it when the focus landed').toContain(text)
    cleanup()
    vi.unstubAllGlobals()
  }
  // Port cleared, Name filled: the focus goes to Port.
  {
    const { main } = await loaded({ serverProfiles: ROWS })
    const form = await openForm(main)
    const port = boxIn(form, en('server-port'))
    const text = en('field-error-number', '1', '65535')
    const at = watchFocus(port, form, text)
    type(boxIn(form, en('server-profile-name')), 'edge-box')
    type(port, '')
    press(buttonIn(form, 'create-button'))
    await within(form).findByText(text)
    expect(at.focused, 'the focus landed on Port').toBe(true)
    expect(at.inForm, 'the error was in the DOM when the focus landed').toBe(
      true,
    )
    expect(at.described, 'Port named it when the focus landed').toContain(text)
    cleanup()
    vi.unstubAllGlobals()
  }
})

// ── 17b.5: refusals ─────────────────────────────────────────────────────

test('16 17b.4: a valid form sends POST /api/profiles/server with the engine, the trimmed Name and the port as a number; the status reads create-creating with both buttons aria-disabled and a second Create sends nothing; 201 goes to the reply’s page and its h1 takes the focus', async () => {
  const gate = deferred()
  const { bff, main } = await loaded({
    serverProfiles: ROWS,
    serverProfileCreate: () => gate.promise,
  })
  const form = await openForm(main)
  type(boxIn(form, en('server-profile-name')), '  edge-box  ')
  type(boxIn(form, en('server-port')), '9932')
  const create = buttonIn(form, 'create-button')
  const cancel = buttonIn(form, 'create-cancel')
  press(create)
  expect(
    (await within(form).findByText(en('create-creating'))).closest(
      '[role="status"]',
    ),
    'the status line says what is happening',
  ).not.toBeNull()
  expect(create.getAttribute('aria-disabled'), 'Create is inactive').toBe(
    'true',
  )
  expect(cancel.getAttribute('aria-disabled'), 'Cancel is inactive').toBe(
    'true',
  )
  press(create)
  press(cancel)
  expect(posts(bff), 'a second Create sends nothing').toHaveLength(1)
  expect(posts(bff)[0]!.body, 'the body of the contract').toEqual({
    engine: 'llama.cpp',
    id: 'edge-box',
    port: 9932,
  })
  gate.resolve(reply(201, { id: 'edge-box' }))
  await flushTasks()
  expect(window.location.hash, 'the new profile’s page').toBe(
    '#/profiles/server/edge-box',
  )
  const h1 = await within(main).findByRole('heading', {
    level: 1,
    name: 'edge-box',
  })
  expect(document.activeElement, 'its h1 takes the focus').toBe(h1)
})

test('16 17b.4: while the POST runs Cancel is aria-disabled and a click on it closes nothing, the form, its status line and the disclosure staying as they are; the 201 that lands afterwards is handled as 17b.4 says (17b review R1)', async () => {
  const gate = deferred()
  const { bff, main } = await loaded({
    serverProfiles: ROWS,
    serverProfileCreate: () => gate.promise,
  })
  const form = await openForm(main)
  const opener = newButton(main)
  type(boxIn(form, en('server-profile-name')), 'edge-box')
  const cancel = buttonIn(form, 'create-cancel')
  press(buttonIn(form, 'create-button'))
  await within(form).findByText(en('create-creating'))
  expect(cancel.getAttribute('aria-disabled'), 'Cancel is inactive').toBe(
    'true',
  )
  // The click: an inactive control answers nothing (aria-disabled is the whole
  // of it, so its handler must be inert too: 17b.2 closes a form that is idle).
  press(cancel)
  expect(
    within(main).queryByRole('form', {
      name: en('server-profiles-new-title'),
    }),
    'the form is still there',
  ).toBe(form)
  expect(opener.getAttribute('aria-expanded'), 'the disclosure is open').toBe(
    'true',
  )
  expect(
    formStatus(form).textContent,
    'the status line still says what is happening',
  ).toBe(en('create-creating'))
  expect(
    document.activeElement,
    'the focus did not move to the opener',
  ).not.toBe(opener)
  expect(posts(bff), 'and the click sent nothing').toHaveLength(1)
  // The create that was already sent is answered as 17b.4.
  gate.resolve(reply(201, { id: 'edge-box' }))
  await flushTasks()
  expect(window.location.hash, 'the 201 goes to the new profile’s page').toBe(
    '#/profiles/server/edge-box',
  )
  const h1 = await within(main).findByRole('heading', {
    level: 1,
    name: 'edge-box',
  })
  expect(document.activeElement, 'whose h1 takes the focus').toBe(h1)
})

// ── 17b.5: refusals ─────────────────────────────────────────────────────

/** A create form typed with `name` and `port`, Create pressed, answered `answer`. */
async function refused(
  answer: () => Response | Promise<Response>,
  name = 'edge-box',
  port = '9932',
) {
  const { bff, main } = await loaded({
    serverProfiles: ROWS,
    serverProfileCreate: () => answer(),
  })
  const form = await openForm(main)
  type(boxIn(form, en('server-profile-name')), name)
  type(boxIn(form, en('server-port')), port)
  press(buttonIn(form, 'create-button'))
  return { bff, main, form }
}

test('16 17b.5: a 422 with field id puts api-server-profile-bad-id under Name, marks it and focuses it, and a 422 with field port puts api-not-whole-number-in-range under Port, marks it and focuses it', async () => {
  // field id.
  {
    const { bff, form } = await refused(
      () =>
        reply(422, {
          key: 'server-profile-bad-id',
          args: ['Edge Box'],
          field: 'id',
        }),
      'Edge Box',
    )
    const text = en('api-server-profile-bad-id', 'Edge Box')
    const name = boxIn(form, en('server-profile-name'))
    const error = await within(form).findByText(text)
    expect(before(name, error), 'the text is under Name').toBe(true)
    expect(name.getAttribute('aria-invalid'), 'Name is marked').toBe('true')
    expect(described(name), 'Name is described by it').toContain(text)
    expect(document.activeElement, 'the focus is on Name').toBe(name)
    expect(
      within(form).queryAllByRole('alert'),
      'the field error is not the alert',
    ).toHaveLength(0)
    expect(
      boxIn(form, en('server-port')).value,
      'Port keeps what was typed',
    ).toBe('9932')
    expect(
      boxIn(form, en('server-profile-name')).value,
      'Name keeps it too',
    ).toBe('Edge Box')
    expect(posts(bff), 'one request').toHaveLength(1)
    cleanup()
    vi.unstubAllGlobals()
  }
  // field port. The args are the bff's `[key, text, min, max]`. The port typed
  // is one the client check of 17b.3 accepts (a whole number of the range):
  // anything else is refused in words before anything is sent, so no 422 with
  // `field` port could ever arrive.
  {
    const args = ['port', '9932', '1', '65535']
    const { form } = await refused(
      () =>
        reply(422, { key: 'not-whole-number-in-range', args, field: 'port' }),
      'Edge Box',
      '9932',
    )
    const text = en('api-not-whole-number-in-range', ...args)
    const port = boxIn(form, en('server-port'))
    const error = await within(form).findByText(text)
    expect(before(port, error), 'the text is under Port').toBe(true)
    expect(port.getAttribute('aria-invalid'), 'Port is marked').toBe('true')
    expect(described(port), 'Port is described by it').toContain(text)
    expect(document.activeElement, 'the focus is on Port').toBe(port)
    expect(within(form).queryAllByRole('alert'), 'not the alert').toHaveLength(
      0,
    )
  }
})

test('16 17b.5: a refusal with a field moves the focus there after its text rendered: the text is in the DOM, and named by the field, at the moment the focus lands on it (17b review D1)', async () => {
  // field id: the focus goes to Name.
  {
    const { main } = await loaded({
      serverProfiles: ROWS,
      serverProfileCreate: () =>
        reply(422, {
          key: 'server-profile-bad-id',
          args: ['Edge Box'],
          field: 'id',
        }),
    })
    const form = await openForm(main)
    const name = boxIn(form, en('server-profile-name'))
    const text = en('api-server-profile-bad-id', 'Edge Box')
    const at = watchFocus(name, form, text)
    type(name, 'Edge Box')
    press(buttonIn(form, 'create-button'))
    await within(form).findByText(text)
    expect(at.focused, 'the focus landed on Name').toBe(true)
    expect(at.inForm, 'the text was in the DOM when the focus landed').toBe(
      true,
    )
    expect(at.described, 'Name named it when the focus landed').toContain(text)
    cleanup()
    vi.unstubAllGlobals()
  }
  // field port: the focus goes to Port. The port typed is one the client check
  // of 17b.3 accepts, so the reply can arrive.
  {
    const args = ['port', '9932', '1', '65535']
    const text = en('api-not-whole-number-in-range', ...args)
    const { main } = await loaded({
      serverProfiles: ROWS,
      serverProfileCreate: () =>
        reply(422, { key: 'not-whole-number-in-range', args, field: 'port' }),
    })
    const form = await openForm(main)
    const port = boxIn(form, en('server-port'))
    const at = watchFocus(port, form, text)
    type(boxIn(form, en('server-profile-name')), 'edge-box')
    type(port, '9932')
    press(buttonIn(form, 'create-button'))
    await within(form).findByText(text)
    expect(at.focused, 'the focus landed on Port').toBe(true)
    expect(at.inForm, 'the text was in the DOM when the focus landed').toBe(
      true,
    )
    expect(at.described, 'Port named it when the focus landed').toContain(text)
  }
})

test('16 17b.5: field engine, 409 client-server-profile-exists, a 422 with an unknown key, a 400 and a 403 session-not-administrator each show their text in the form’s alert, keep the form open with what was typed, and mark no field', async () => {
  const cases: [string, () => Response | Promise<Response>, string][] = [
    [
      '422 unknown-engine with field engine',
      () =>
        reply(422, {
          key: 'unknown-engine',
          args: ['llama.cpp'],
          field: 'engine',
        }),
      en('api-unknown-engine', 'llama.cpp'),
    ],
    [
      '409 client-server-profile-exists',
      () =>
        reply(409, { key: 'client-server-profile-exists', args: ['edge-box'] }),
      en('api-client-server-profile-exists', 'edge-box'),
    ],
    [
      '422 with a key the fe does not know',
      () => reply(422, { key: 'x-y', args: [] }),
      en('api-other', 'x-y'),
    ],
    [
      '400 server-profile-malformed',
      () => reply(400, { key: 'server-profile-malformed', args: [] }),
      en('request-failed'),
    ],
    [
      '403 session-not-administrator',
      () => reply(403, { key: 'session-not-administrator', args: [] }),
      en('session-revoked'),
    ],
    [
      'a network failure',
      () => Promise.reject(new TypeError('Failed to fetch')),
      en('request-failed'),
    ],
  ]
  for (const [what, answer, text] of cases) {
    const { bff, main, form } = await refused(answer)
    const alert = await within(form).findByText(text)
    expect(
      alert.closest('[role="alert"]'),
      `${what}: the text is in a role="alert"`,
    ).not.toBeNull()
    expect(
      within(main).queryAllByRole('alert').length,
      `${what}: one alert`,
    ).toBe(1)
    expect(
      main.querySelectorAll('[aria-invalid="true"]').length,
      `${what}: no field is marked`,
    ).toBe(0)
    expect(
      newButton(main).getAttribute('aria-expanded'),
      `${what}: the form stays open`,
    ).toBe('true')
    expect(
      boxIn(form, en('server-profile-name')).value,
      `${what}: Name keeps what was typed`,
    ).toBe('edge-box')
    expect(
      boxIn(form, en('server-port')).value,
      `${what}: Port keeps what was typed`,
    ).toBe('9932')
    expect(posts(bff), `${what}: one request`).toHaveLength(1)
    cleanup()
    vi.unstubAllGlobals()
  }
})

test('16 17b.5: a 503 says create-unknown in the form’s alert and reads the list again', async () => {
  const { bff, main, form } = await refused(() =>
    reply(503, { key: 'client-unavailable', args: [] }),
  )
  const alert = await within(form).findByText(en('create-unknown'))
  expect(
    alert.closest('[role="alert"]'),
    'the alert of the form',
  ).not.toBeNull()
  await within(main).findByRole('table')
  expect(listReads(bff), 'the list is read again').toHaveLength(2)
  expect(
    boxIn(form, en('server-profile-name')).value,
    'the typed name stays',
  ).toBe('edge-box')
})

test('16 17b.5: a 401 says session-ended-retry with a Sign in after it in the form; the Sign in opens the sign-in Dialog, and after a sign-in the focus returns to Create with the typed values', async () => {
  const { bff, form } = await refused(() =>
    reply(401, { key: 'session-required', args: [] }),
  )
  const alert = await within(form).findByText(en('session-ended-retry'))
  expect(
    alert.closest('[role="alert"]'),
    'the alert of the form',
  ).not.toBeNull()
  const signIns = within(form).getAllByRole('button', {
    name: en('session-sign-in'),
  })
  expect(signIns, 'one Sign in in the form').toHaveLength(1)
  expect(before(alert, signIns[0]!), 'the Sign in is after the alert').toBe(
    true,
  )
  press(signIns[0]!)
  await screen.findByRole('dialog', { name: en('signin-title') })
  await signInInDialog('alice')
  expect(
    within(form).queryByText(en('session-ended-retry')),
    'the alert is gone',
  ).toBeNull()
  expect(boxIn(form, en('server-profile-name')).value, 'the name stays').toBe(
    'edge-box',
  )
  expect(boxIn(form, en('server-port')).value, 'the port stays').toBe('9932')
  expect(document.activeElement, 'the focus returns to Create').toBe(
    buttonIn(form, 'create-button'),
  )
  expect(posts(bff), 'nothing is sent again').toHaveLength(1)
})
