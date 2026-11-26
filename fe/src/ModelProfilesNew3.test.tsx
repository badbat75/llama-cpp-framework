// Brief 22, slice 22C1 (fe/briefs/22-model-sources.md, as the "22P/22C check
// (2026-11-26)" amends it): the New model profile form's checks, its request
// and its answers. This file is 22 C.4 (the page checks, in their order), 22
// C.5 with C.5a and C.5b (the request, and where a 201 lands), 22 C.6 (the
// refusals that mark a field), 22 C.7 with C.7a (the refusals in the alert)
// and 22 C.8 (the keyboard order). The form's shape is
// ModelProfilesNew1.test.tsx, the Model file picker is
// ModelProfilesNew2.test.tsx, and 22 C.9 (the strings file) is `1a strings` in
// App.test.tsx.
//
// Enter in a text field submits a form in a browser; jsdom does not do it, so
// 22 C.8 pins the two halves that are observable here: Create is a `submit`
// button inside the `form` (so a browser submits on Enter), the keydown is not
// cancelled, and `fireEvent.submit` (what the browser does) sends the create.
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
  MODELS_DIR,
  before,
  deferred,
  en,
  fakeBff,
  modelFilesBody,
  quiet,
  reply,
  startAt,
  stubDialog,
  type BffOptions,
  type ModelFileRow,
  type ModelProfile,
} from './test/bff.ts'

const CREATE = '/api/profiles/model'
const LIST = '/api/profiles/model'
const QWEN = `${MODELS_DIR}/qwen/Qwen3-27B-Q8_0.gguf`

let dialogs: ReturnType<typeof stubDialog>

function modelProfile(id: string, enabled: boolean, model = ''): ModelProfile {
  return { id, engine: 'llama.cpp', engineAvailable: true, enabled, model }
}

/** The list most tests read: one on, one off, one with a model. */
const LIST_ROWS: ModelProfile[] = [
  modelProfile('Qwen3-27B', true, QWEN),
  modelProfile('Old-Coder', false),
]

beforeEach(() => {
  startAt('#/profiles/model')
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

/** The app at #/profiles/model, the table loaded. */
async function loaded(options: BffOptions = {}) {
  const bff = fakeBff({ modelProfiles: LIST_ROWS, ...options })
  render(<App />)
  const main = screen.getByRole('main')
  await within(main).findByRole('table', { name: en('model-profiles-title') })
  await flushTasks()
  return { bff, main }
}

function press(el: HTMLElement) {
  el.focus()
  fireEvent.click(el)
}

function type(input: HTMLElement, value: string) {
  fireEvent.change(input, { target: { value } })
}

function key(el: HTMLElement, k: string) {
  fireEvent.keyDown(el, { key: k, code: k })
}

/** The create form, with the file choice open. */
async function openForm(main: HTMLElement): Promise<HTMLElement> {
  press(within(main).getByRole('button', { name: en('model-profiles-new') }))
  return within(main).findByRole('form', {
    name: en('model-profiles-new-title'),
  })
}

function boxIn(form: HTMLElement, label: string): HTMLInputElement {
  return within(form).getByRole('textbox', { name: label }) as HTMLInputElement
}

function buttonIn(form: HTMLElement, keyName: string): HTMLElement {
  return within(form).getByRole('button', { name: en(keyName) })
}

function radioIn(form: HTMLElement, keyName: string): HTMLInputElement {
  return within(form).getByRole('radio', {
    name: en(keyName),
  }) as HTMLInputElement
}

function selectIn(form: HTMLElement): HTMLSelectElement {
  return within(form).getByRole('combobox', {
    name: en('model-profile-copy-from'),
  }) as HTMLSelectElement
}

/** The Model file combobox. */
function fileBox(form: HTMLElement): HTMLInputElement {
  return within(form).getByRole('combobox', {
    name: en('model-profile-model-file'),
  }) as HTMLInputElement
}

/** The text of every element named by `el`'s `aria-describedby`. */
function described(el: HTMLElement): string[] {
  return (el.getAttribute('aria-describedby') ?? '')
    .split(/\s+/)
    .filter(Boolean)
    .map((id) => document.getElementById(id)?.textContent ?? `(no #${id})`)
}

/** The creates the bff was asked for, in order. */
function posts(bff: ReturnType<typeof fakeBff>) {
  return bff.to('POST', CREATE)
}

/** The list reads the bff was asked for, in order. */
function listReads(bff: ReturnType<typeof fakeBff>) {
  return bff.to('GET', LIST)
}

/** Every option in the DOM, open list or not (a closed listbox is `hidden`, so
 *  its options are out of the accessibility tree). */
function optionsIn(root: HTMLElement): HTMLElement[] {
  return [...root.querySelectorAll<HTMLElement>('[role="option"]')]
}

/** The first option of the list, picked with the keys. */
async function pickFirst(form: HTMLElement): Promise<HTMLInputElement> {
  const box = fileBox(form)
  await waitFor(() => expect(optionsIn(form).length).toBeGreaterThan(0))
  key(box, 'ArrowDown')
  key(box, 'Enter')
  return box
}

/** A form filled with `name` and a picked file, Create pressed. */
async function sentFile(
  answer: () => Response | Promise<Response>,
  name = 'New-Model',
) {
  const { bff, main } = await loaded({ modelProfileCreate: () => answer() })
  const form = await openForm(main)
  type(boxIn(form, en('model-profile-name')), name)
  const box = await pickFirst(form)
  press(buttonIn(form, 'create-button'))
  return { bff, main, form, box }
}

/** A form filled with `name` and the copy of `from`, Create pressed. */
async function sentCopy(
  answer: () => Response | Promise<Response>,
  name = 'New-Model',
  from = 'Old-Coder',
) {
  const { bff, main } = await loaded({ modelProfileCreate: () => answer() })
  const form = await openForm(main)
  type(boxIn(form, en('model-profile-name')), name)
  press(radioIn(form, 'model-source-copy'))
  const select = selectIn(form)
  fireEvent.change(select, { target: { value: from } })
  press(buttonIn(form, 'create-button'))
  return { bff, main, form, select }
}

/** Every tab stop inside `root`, in the document's order. */
function tabbables(root: HTMLElement): HTMLElement[] {
  return [
    ...root.querySelectorAll<HTMLElement>(
      'a[href], button, input, select, textarea, [tabindex]',
    ),
  ].filter(
    (el) =>
      !(el as HTMLButtonElement).disabled &&
      el.getAttribute('tabindex') !== '-1' &&
      el.closest('[hidden]') === null,
  )
}

// ── 22 C.4: the page checks, in their order ─────────────────────────────

test('22 C.4: Create with Name empty shows field-error-required under Name, marks Name aria-invalid and described by it, moves the focus there, and sends nothing', async () => {
  const { bff, main } = await loaded()
  const form = await openForm(main)
  const name = boxIn(form, en('model-profile-name'))
  press(buttonIn(form, 'create-button'))
  const error = await within(form).findByText(en('field-error-required'))
  expect(before(name, error), 'the error is under the control').toBe(true)
  expect(name.getAttribute('aria-invalid'), 'Name is invalid').toBe('true')
  expect(described(name), 'Name is described by its error').toContain(
    en('field-error-required'),
  )
  expect(document.activeElement, 'the focus moves to Name').toBe(name)
  expect(within(form).queryAllByRole('alert'), 'no alert').toHaveLength(0)
  expect(posts(bff), 'nothing is sent').toHaveLength(0)
  expect(main).toBeTruthy()
})

test('22 C.4: a Name with a character outside A-Z a-z 0-9 . - _ shows api-invalid-model-profile-id with the name under Name, and a Name equal to a list id with letter case aside shows api-model-profile-name-taken under Name; each marks and focuses Name and sends nothing', async () => {
  const cases: [string, string][] = [
    ['a space', 'Qwen 27B'],
    ['a slash', 'qwen/27B'],
    ['a semicolon', 'a;b'],
    ['a letter outside A-Z a-z', 'Ünïcode'],
  ]
  for (const [what, value] of cases) {
    const { bff, main } = await loaded()
    const form = await openForm(main)
    const name = boxIn(form, en('model-profile-name'))
    type(name, value)
    press(buttonIn(form, 'create-button'))
    const text = en('api-invalid-model-profile-id', value)
    const error = await within(form).findByText(text)
    expect(before(name, error), `${what}: the error is under Name`).toBe(true)
    expect(name.getAttribute('aria-invalid'), `${what}: Name is marked`).toBe(
      'true',
    )
    expect(described(name), `${what}: Name is described by it`).toContain(text)
    expect(document.activeElement, `${what}: the focus is on Name`).toBe(name)
    expect(
      fileBox(form).getAttribute('aria-invalid'),
      `${what}: Model file is not marked`,
    ).toBeNull()
    expect(
      within(form).queryAllByRole('alert'),
      `${what}: no alert`,
    ).toHaveLength(0)
    expect(posts(bff), `${what}: nothing is sent`).toHaveLength(0)
    cleanup()
    vi.unstubAllGlobals()
  }
  // The name taken, letter case aside, is said under Name (decision 9), not in
  // the alert.
  const { bff, main } = await loaded()
  const form = await openForm(main)
  const name = boxIn(form, en('model-profile-name'))
  type(name, 'qwen3-27b')
  press(buttonIn(form, 'create-button'))
  const text = en('api-model-profile-name-taken', 'qwen3-27b')
  const error = await within(form).findByText(text)
  expect(before(name, error), 'the error is under Name').toBe(true)
  expect(name.getAttribute('aria-invalid'), 'Name is marked').toBe('true')
  expect(described(name), 'Name is described by it').toContain(text)
  expect(document.activeElement, 'the focus is on Name').toBe(name)
  expect(
    within(form).queryAllByRole('alert'),
    '22 C.4: the taken name is not the alert',
  ).toHaveLength(0)
  expect(posts(bff), 'nothing is sent').toHaveLength(0)
})

test('22 C.4: a file choice with no pick shows field-error-pick-file under Model file, marks and focuses it and sends nothing, whatever is typed and also when the folder is empty, unreadable or failed; a copy with the choose option shows field-error-choose under the select', async () => {
  const cases: [string, BffOptions][] = [
    ['the folder listed', {}],
    ['an empty folder', { modelFiles: [] }],
    [
      'an unreadable folder',
      {
        modelFiles: () =>
          reply(
            200,
            modelFilesBody([], {
              error: { key: 'models-dir-unreadable', args: [MODELS_DIR] },
            }),
          ),
      },
    ],
    [
      'a failed read',
      {
        modelFiles: () => reply(503, { key: 'client-unavailable', args: [] }),
      },
    ],
  ]
  for (const [what, options] of cases) {
    const { bff, main } = await loaded(options)
    const form = await openForm(main)
    const name = boxIn(form, en('model-profile-name'))
    const box = fileBox(form)
    type(name, 'New-Model')
    type(box, 'qwen')
    press(buttonIn(form, 'create-button'))
    const text = en('field-error-pick-file')
    const error = await within(form).findByText(text)
    expect(before(box, error), `${what}: the error is under Model file`).toBe(
      true,
    )
    expect(
      box.getAttribute('aria-invalid'),
      `${what}: Model file is marked`,
    ).toBe('true')
    expect(described(box), `${what}: Model file is described by it`).toContain(
      text,
    )
    expect(document.activeElement, `${what}: the focus is on Model file`).toBe(
      box,
    )
    expect(
      name.getAttribute('aria-invalid'),
      `${what}: Name is not marked`,
    ).toBeNull()
    expect(
      within(form).queryAllByRole('alert'),
      `${what}: no alert`,
    ).toHaveLength(0)
    expect(posts(bff), `${what}: nothing is sent`).toHaveLength(0)
    cleanup()
    vi.unstubAllGlobals()
  }
  // The copy choice with the `model-profile-copy-choose` option selected.
  const { bff, main } = await loaded()
  const form = await openForm(main)
  type(boxIn(form, en('model-profile-name')), 'New-Model')
  press(radioIn(form, 'model-source-copy'))
  const select = selectIn(form)
  expect(select.value, 'the choose option is selected').toBe('')
  press(buttonIn(form, 'create-button'))
  const text = en('field-error-choose')
  const error = await within(form).findByText(text)
  expect(before(select, error), 'the error is under the select').toBe(true)
  expect(select.getAttribute('aria-invalid'), 'the select is marked').toBe(
    'true',
  )
  expect(described(select), 'the select is described by it').toContain(text)
  expect(document.activeElement, 'the focus is on the select').toBe(select)
  expect(posts(bff), 'nothing is sent').toHaveLength(0)
})

test('22 C.4: the checks run in their order, so a Name that fails and no pick mark both fields and the focus goes to Name, the first of them', async () => {
  const { bff, main } = await loaded()
  const form = await openForm(main)
  const name = boxIn(form, en('model-profile-name'))
  const box = fileBox(form)
  type(name, 'qwen3-27b')
  type(box, 'qwen')
  press(buttonIn(form, 'create-button'))
  await within(form).findByText(en('api-model-profile-name-taken', 'qwen3-27b'))
  expect(
    within(form).getByText(en('field-error-pick-file')),
    'Model file speaks too',
  ).toBeTruthy()
  expect(name.getAttribute('aria-invalid'), 'Name is marked').toBe('true')
  expect(box.getAttribute('aria-invalid'), 'Model file is marked').toBe('true')
  expect(
    document.activeElement,
    'the focus is on the first invalid field',
  ).toBe(name)
  expect(posts(bff), 'nothing is sent').toHaveLength(0)
})

test('22 C.4 (the reviewer, 22C1 G1): a picked file path holding ; or # shows field-error-comment-markers under Model file, marks and focuses it, and sends nothing', async () => {
  // The INI file cuts the value at either marker (the key's own words), so the
  // page refuses both. The reviewer's gap: the `#` half was unpinned, and only
  // the `;` half was.
  for (const mark of [';', '#']) {
    const row: ModelFileRow = {
      path: `${MODELS_DIR}/odd/weird${mark}name.gguf`,
      size: 2 ** 30,
      shards: 1,
      category: 'odd',
    }
    const { bff, main } = await loaded({ modelFiles: [row] })
    const form = await openForm(main)
    type(boxIn(form, en('model-profile-name')), 'New-Model')
    const box = await pickFirst(form)
    expect(box.value, `the path holding ${mark} is picked`).toBe(row.path)
    press(buttonIn(form, 'create-button'))
    const text = en('field-error-comment-markers')
    const error = await within(form).findByText(text)
    expect(before(box, error), `the error is under Model file`).toBe(true)
    expect(box.getAttribute('aria-invalid'), `Model file is marked`).toBe(
      'true',
    )
    expect(described(box), `Model file is described by it`).toContain(text)
    expect(document.activeElement, 'the focus is on Model file').toBe(box)
    expect(
      posts(bff),
      `nothing is sent for a path holding ${mark}`,
    ).toHaveLength(0)
    cleanup()
    vi.unstubAllGlobals()
  }
})

// ── 22 C.5, C.5a, C.5b: the request ─────────────────────────────────────

test('22 C.5: a valid file form sends POST /api/profiles/model with the trimmed id and values model of the picked path; the status reads create-creating with Create and Cancel aria-disabled and a second Create sends nothing; 22 C.5a: the 201 goes to the new profile page with its h1 focused and no restart offer; 22 C.5b: Create is plain primary', async () => {
  const gate = deferred()
  const { bff, main } = await loaded({ modelProfileCreate: () => gate.promise })
  const form = await openForm(main)
  const create = buttonIn(form, 'create-button')
  const cancel = buttonIn(form, 'create-cancel')
  expect(
    create.classList.contains('primary'),
    '22 C.5b: Create is primary',
  ).toBe(true)
  expect(
    create.classList.contains('danger'),
    '22 C.5b: it is not a destructive verb',
  ).toBe(false)
  type(boxIn(form, en('model-profile-name')), '  New-Model  ')
  const box = await pickFirst(form)
  expect(box.value, 'a file is picked').toBe(QWEN)
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
    id: 'New-Model',
    values: { model: QWEN },
  })
  gate.resolve(reply(201, { id: 'New-Model' }))
  await flushTasks()
  expect(window.location.hash, 'the new profile page').toBe(
    '#/profiles/model/New-Model',
  )
  const h1 = await within(main).findByRole('heading', {
    level: 1,
    name: 'New-Model',
  })
  expect(document.activeElement, '22 C.5a: its h1 takes the focus').toBe(h1)
  expect(
    main.querySelector('.restart-offer'),
    '22 C.5a: a new profile is off, so no restart offer follows',
  ).toBeNull()
  expect(
    main.querySelectorAll('.restart-offer-status').length,
    '22 C.5a: and no restart status',
  ).toBe(0)
  expect(
    within(main).queryByRole('form', { name: en('model-profiles-new-title') }),
    'the form is gone with the page',
  ).toBeNull()
})

test('22 C.5: a valid copy form sends POST /api/profiles/model with the trimmed id and from of the selected id, and no values', async () => {
  const gate = deferred()
  const { bff, main } = await loaded({ modelProfileCreate: () => gate.promise })
  const form = await openForm(main)
  type(boxIn(form, en('model-profile-name')), '  Copy-Of-Coder  ')
  press(radioIn(form, 'model-source-copy'))
  const select = selectIn(form)
  fireEvent.change(select, { target: { value: 'Old-Coder' } })
  press(buttonIn(form, 'create-button'))
  await within(form).findByText(en('create-creating'))
  expect(posts(bff), 'one request').toHaveLength(1)
  expect(posts(bff)[0]!.body, 'the body of the contract').toEqual({
    id: 'Copy-Of-Coder',
    from: 'Old-Coder',
  })
  gate.resolve(reply(201, { id: 'Copy-Of-Coder' }))
  await flushTasks()
  expect(window.location.hash, 'the new profile page').toBe(
    '#/profiles/model/Copy-Of-Coder',
  )
})

// ── 22 C.6: the refusals that mark a field ──────────────────────────────

test('22 C.6: a 422 with field id, field from, field model and a 409 model-profile-name-taken each put their text under that field, mark it, describe it and focus it, keep what was typed, and raise no alert', async () => {
  // field id.
  {
    const { bff, form } = await sentFile(() =>
      reply(422, {
        key: 'invalid-model-profile-id',
        args: ['New-Model'],
        field: 'id',
      }),
    )
    const text = en('api-invalid-model-profile-id', 'New-Model')
    const name = boxIn(form, en('model-profile-name'))
    const error = await within(form).findByText(text)
    expect(before(name, error), 'field id: the text is under Name').toBe(true)
    expect(name.getAttribute('aria-invalid'), 'field id: Name is marked').toBe(
      'true',
    )
    expect(described(name), 'field id: Name is described by it').toContain(text)
    expect(document.activeElement, 'field id: the focus is on Name').toBe(name)
    expect(
      within(form).queryAllByRole('alert'),
      'field id: not the alert',
    ).toHaveLength(0)
    expect(
      boxIn(form, en('model-profile-name')).value,
      'field id: Name keeps what was typed',
    ).toBe('New-Model')
    expect(posts(bff), 'field id: one request').toHaveLength(1)
    cleanup()
    vi.unstubAllGlobals()
  }
  // field from.
  {
    const { form } = await sentCopy(() =>
      reply(422, {
        key: 'no-model-profile-named',
        args: ['Old-Coder'],
        field: 'from',
      }),
    )
    const text = en('api-no-model-profile-named', 'Old-Coder')
    const select = selectIn(form)
    const error = await within(form).findByText(text)
    expect(
      before(select, error),
      'field from: the text is under the select',
    ).toBe(true)
    expect(
      select.getAttribute('aria-invalid'),
      'field from: the select is marked',
    ).toBe('true')
    expect(
      described(select),
      'field from: the select is described by it',
    ).toContain(text)
    expect(
      document.activeElement,
      'field from: the focus is on the select',
    ).toBe(select)
    expect(
      within(form).queryAllByRole('alert'),
      'field from: not the alert',
    ).toHaveLength(0)
    cleanup()
    vi.unstubAllGlobals()
  }
  // field model.
  {
    const { form } = await sentFile(() =>
      reply(422, {
        key: 'comment-markers',
        args: [QWEN],
        field: 'model',
      }),
    )
    const text = en('api-comment-markers', QWEN)
    const box = fileBox(form)
    const error = await within(form).findByText(text)
    expect(
      before(box, error),
      'field model: the text is under Model file',
    ).toBe(true)
    expect(
      box.getAttribute('aria-invalid'),
      'field model: Model file is marked',
    ).toBe('true')
    expect(
      described(box),
      'field model: Model file is described by it',
    ).toContain(text)
    expect(
      document.activeElement,
      'field model: the focus is on Model file',
    ).toBe(box)
    expect(
      within(form).queryAllByRole('alert'),
      'field model: not the alert',
    ).toHaveLength(0)
    expect(box.value, 'field model: the pick stays').toBe(QWEN)
    cleanup()
    vi.unstubAllGlobals()
  }
  // 409 model-profile-name-taken, the manager's words under Name.
  {
    const { form } = await sentFile(() =>
      reply(409, { key: 'model-profile-name-taken', args: ['New-Model'] }),
    )
    const text = en('api-model-profile-name-taken', 'New-Model')
    const name = boxIn(form, en('model-profile-name'))
    const error = await within(form).findByText(text)
    expect(before(name, error), '409: the text is under Name').toBe(true)
    expect(name.getAttribute('aria-invalid'), '409: Name is marked').toBe(
      'true',
    )
    expect(described(name), '409: Name is described by it').toContain(text)
    expect(document.activeElement, '409: the focus is on Name').toBe(name)
    expect(
      within(form).queryAllByRole('alert'),
      '409: not the alert',
    ).toHaveLength(0)
  }
})

// ── 22 C.7, C.7a: the refusals in the alert ─────────────────────────────

test('22 C.7, C.7a: field values, a 422 with no field, a 400, a 413, a 503, a 403 and a network failure each say their text in the form alert, keep the form open with what was typed and picked, and mark no field', async () => {
  const cases: [string, () => Response | Promise<Response>, string][] = [
    [
      '422 model-profile-no-model with field values',
      () =>
        reply(422, {
          key: 'model-profile-no-model',
          args: ['New-Model'],
          field: 'values',
        }),
      en('api-model-profile-no-model', 'New-Model'),
    ],
    [
      '422 model-profile-bad-source with field values',
      () =>
        reply(422, {
          key: 'model-profile-bad-source',
          args: ['New-Model'],
          field: 'values',
        }),
      en('api-other', 'model-profile-bad-source'),
    ],
    [
      '422 with no field',
      () => reply(422, { key: 'comment-markers', args: [QWEN] }),
      en('api-comment-markers', QWEN),
    ],
    [
      '400 model-profiles-malformed',
      () => reply(400, { key: 'model-profiles-malformed', args: [] }),
      en('request-failed'),
    ],
    [
      '413 payload-too-large',
      () => reply(413, { key: 'payload-too-large', args: [] }),
      en('request-failed'),
    ],
    [
      '503 client-unavailable',
      () => reply(503, { key: 'client-unavailable', args: [] }),
      en('api-client-unavailable'),
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
    const { bff, main, form, box } = await sentFile(answer)
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
      within(main).getByRole('form', { name: en('model-profiles-new-title') }),
      `${what}: the form stays open`,
    ).toBe(form)
    expect(
      boxIn(form, en('model-profile-name')).value,
      `${what}: Name keeps what was typed`,
    ).toBe('New-Model')
    expect(box.value, `${what}: the pick stays`).toBe(QWEN)
    expect(posts(bff), `${what}: one request`).toHaveLength(1)
    cleanup()
    vi.unstubAllGlobals()
  }
})

test('22 C.7: a 504 client-timeout says create-unknown in the alert and reads the list again, and never request-failed', async () => {
  const { bff, main, form } = await sentFile(() =>
    reply(504, { key: 'client-timeout', args: [] }),
  )
  const alert = await within(form).findByText(en('create-unknown'))
  expect(
    alert.closest('[role="alert"]'),
    'the alert of the form',
  ).not.toBeNull()
  expect(
    form.textContent,
    '22 C.7a: the create is not said to have failed',
  ).not.toContain(en('request-failed'))
  await within(main).findByRole('table', { name: en('model-profiles-title') })
  expect(listReads(bff), 'the list is read again').toHaveLength(2)
  expect(
    boxIn(form, en('model-profile-name')).value,
    'the typed name stays',
  ).toBe('New-Model')
})

test('22 C.7: a 401 says session-ended-retry with a Sign in after it in the form; the Sign in opens the sign-in Dialog, and after a sign-in the focus returns to Create with the typed values and nothing sent again', async () => {
  const { bff, form } = await sentFile(() =>
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
  const d = screen.getByRole('dialog', { name: en('signin-title') })
  expect(dialogs.opened, 'opened with showModal()').toContain(d)
  type(within(d).getByRole('textbox', { name: en('signin-username') }), 'alice')
  type(
    within(d).getByLabelText(en('signin-password'), { selector: 'input' }),
    'hunter2',
  )
  fireEvent.click(within(d).getByRole('button', { name: en('signin-submit') }))
  await flushTasks()
  expect(screen.queryByRole('dialog'), 'the dialog closed').toBeNull()
  expect(
    within(form).queryByText(en('session-ended-retry')),
    'the alert is gone',
  ).toBeNull()
  expect(boxIn(form, en('model-profile-name')).value, 'the name stays').toBe(
    'New-Model',
  )
  expect(document.activeElement, 'the focus returns to Create').toBe(
    buttonIn(form, 'create-button'),
  )
  expect(posts(bff), 'nothing is sent again').toHaveLength(1)
})

// ── 22 C.8: the keyboard order ──────────────────────────────────────────

test('22 C.8: the Tab path is the opener, Name and its About toggle, the radios, the file field and its About toggle, Create, Cancel; the radios are one native group; Enter in Name and in the closed file field is not cancelled, Create is the submit button, and submitting the form sends the create', async () => {
  const { bff, main } = await loaded()
  const opener = within(main).getByRole('button', {
    name: en('model-profiles-new'),
  })
  const form = await openForm(main)
  const name = boxIn(form, en('model-profile-name'))
  const nameAbout = within(form).getByRole('button', {
    name: `${en('field-about')} ${en('model-profile-name')}`,
  })
  const file = fileBox(form)
  const fileAbout = within(form).getByRole('button', {
    name: `${en('field-about')} ${en('model-profile-model-file')}`,
  })
  const create = buttonIn(form, 'create-button')
  const cancel = buttonIn(form, 'create-cancel')
  const fileRadio = radioIn(form, 'model-source-file')
  const hfRadio = radioIn(form, 'model-source-hf')
  const copyRadio = radioIn(form, 'model-source-copy')
  // Every async element is taken before the list (the shell lands with the
  // session, the table with the read).
  const stops = tabbables(main)
  const at = (el: HTMLElement) => stops.indexOf(el)
  for (const el of [
    name,
    nameAbout,
    fileRadio,
    hfRadio,
    copyRadio,
    file,
    fileAbout,
    create,
    cancel,
  ])
    expect(
      at(el),
      `${el.getAttribute('aria-label') ?? el.textContent ?? ''}: a tab stop`,
    ).toBeGreaterThan(-1)
  // The order the brief gives: the opener, Name and its About, the radios, the
  // checked choice's controls, Create, Cancel. The two of each field keep their
  // own order (fe/DESIGN.md 5.2), so only the groups are pinned here.
  expect(at(opener), 'the opener is a tab stop').toBeGreaterThan(-1)
  expect(at(name), 'Name comes after the opener').toBeGreaterThan(at(opener))
  expect(at(nameAbout), "Name's About comes after the opener").toBeGreaterThan(
    at(opener),
  )
  expect(
    at(fileRadio),
    'the radios come after Name and its About',
  ).toBeGreaterThan(Math.max(at(name), at(nameAbout)))
  // 22C2 puts A Hugging Face repo between the file and the copy choices.
  expect(at(hfRadio), 'the Hugging Face radio follows the file radio').toBe(
    at(fileRadio) + 1,
  )
  expect(at(copyRadio), 'the copy radio follows the Hugging Face radio').toBe(
    at(hfRadio) + 1,
  )
  expect(
    at(file),
    "the checked choice's field follows the radios",
  ).toBeGreaterThan(at(copyRadio))
  expect(at(fileAbout), 'and its About too').toBeGreaterThan(at(copyRadio))
  expect(at(create), 'Create follows the field').toBeGreaterThan(
    Math.max(at(file), at(fileAbout)),
  )
  expect(at(cancel), 'Cancel follows Create').toBe(at(create) + 1)
  // One tab stop for the radios: a native group with one name.
  expect(fileRadio.name, 'the radios are one native group').toBe(hfRadio.name)
  expect(fileRadio.type, 'native radio inputs').toBe('radio')
  // Enter submits: Create is the form's submit button, and the keydown of a
  // text field is not cancelled, so the browser submits.
  expect(create.getAttribute('type'), 'Create submits the form').toBe('submit')
  expect(
    fireEvent.keyDown(name, { key: 'Enter', code: 'Enter' }),
    '22 C.8: Enter in Name is left to the form',
  ).toBe(true)
  expect(
    fireEvent.keyDown(file, { key: 'Enter', code: 'Enter' }),
    '22 C.8: Enter in the file field with the list closed is left to the form',
  ).toBe(true)
  // With the list open, Enter picks the option instead of submitting. The
  // list's options arrive with the folder read, which the form sends when it
  // opens: a key sent while that read is open races it, and ArrowDown with no
  // options leaves the list closed, so Enter is not cancelled (red once in
  // four full runs). Let the read land, open, and await the open before the
  // key, as D.7 awaits the focus effect that follows its render.
  await quiet(bff)
  key(file, 'ArrowDown')
  await waitFor(() =>
    expect(file.getAttribute('aria-expanded'), 'the list is open').toBe('true'),
  )
  expect(
    fireEvent.keyDown(file, { key: 'Enter', code: 'Enter' }),
    '22 C.8: Enter with the list open picks, it does not submit',
  ).toBe(false)
  expect(posts(bff), 'nothing sent by the keys').toHaveLength(0)
  // What the browser does on Enter, in a form with a submit button.
  type(name, 'New-Model')
  fireEvent.submit(form)
  expect(posts(bff), 'the submit sends the create').toHaveLength(1)
  expect(posts(bff)[0]!.body, 'with the body of the contract').toEqual({
    id: 'New-Model',
    values: { model: QWEN },
  })
})
