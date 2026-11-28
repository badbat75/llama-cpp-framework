// Brief 22, slice 22C1 (fe/briefs/22-model-sources.md, as the "22P/22C check
// (2026-11-26)" amends it): the New model profile form on the Models list, with
// its two first choices, a model file and a copy of another profile. This file
// is the form itself: 22 C.1a (the + after the off-note, the form, the status
// regions), 22 C.1 (what the form holds, the empty list), 22 C.3 (Cancel,
// Escape, a new opening), 22 P.1 (the picker's shape) and 22 C.2 (the copy
// select). The picker's file half (22 P.3 to P.6a) is ModelProfilesNew2.test.tsx;
// the checks, the request and its answers (22 C.4 to C.8, C.10a) are
// ModelProfilesNew3.test.tsx. 22 C.9 (the strings file gains exactly 22C's keys)
// is `1a strings` in App.test.tsx, which compares en_US.json against the test's
// EN map. Hugging Face (22 P.7 to P.12) is slice 22C2.
//
// What jsdom cannot see is the `fe/ux` check's: the + drawn as a 40 px round
// button, the form's widths at 320 px and 200 % text (22 C.10a's state).
import {
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
  isHidden,
  navigate,
  reply,
  startAt,
  stubDialog,
  type BffOptions,
  type ModelProfile,
} from './test/bff.ts'

const FILES = '/api/node/model-files'

/** Three profiles, in the file order the list is read in. */
function modelProfile(id: string, enabled: boolean, model = ''): ModelProfile {
  return { id, engine: 'llama.cpp', engineAvailable: true, enabled, model }
}

const LIST: ModelProfile[] = [
  modelProfile('Qwen3-27B', true, '/m/qwen/Qwen3-27B-Q8_0.gguf'),
  modelProfile('Coder-30B', true, '/m/coder/Coder-30B.gguf'),
  modelProfile('Old-Coder', false),
]

let dialogs: ReturnType<typeof stubDialog>

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

/** Lets jsdom's queued tasks (a hashchange, a promise chain) run. */
async function flushTasks(n = 3) {
  await Promise.all(
    Array.from({ length: n }, () => new Promise((r) => setTimeout(r, 0))),
  )
}

/** The app at #/profiles/model, the table loaded. */
async function loaded(options: BffOptions = {}) {
  const bff = fakeBff({ modelProfiles: LIST, ...options })
  render(<App />)
  const main = screen.getByRole('main')
  await within(main).findByRole('table', { name: en('model-profiles-title') })
  await flushTasks()
  return { bff, main }
}

function table(main: HTMLElement): HTMLElement {
  return within(main).getByRole('table', { name: en('model-profiles-title') })
}

function press(el: HTMLElement) {
  el.focus()
  fireEvent.click(el)
}

function type(input: HTMLElement, value: string) {
  fireEvent.change(input, { target: { value } })
}

/** The opener of the create form, named by `model-profiles-new`. */
function newButton(main: HTMLElement): HTMLElement {
  return within(main).getByRole('button', { name: en('model-profiles-new') })
}

/** The create form, once the opener is pressed. */
async function openForm(main: HTMLElement): Promise<HTMLElement> {
  press(newButton(main))
  return within(main).findByRole('form', {
    name: en('model-profiles-new-title'),
  })
}

function boxIn(form: HTMLElement, label: string): HTMLInputElement {
  return within(form).getByRole('textbox', { name: label }) as HTMLInputElement
}

function buttonIn(form: HTMLElement, key: string): HTMLElement {
  return within(form).getByRole('button', { name: en(key) })
}

function radioIn(form: HTMLElement, key: string): HTMLInputElement {
  return within(form).getByRole('radio', { name: en(key) }) as HTMLInputElement
}

/** The picker's fieldset, named by `model-profile-start-from`. */
function picker(form: HTMLElement): HTMLElement {
  return within(form).getByRole('group', {
    name: en('model-profile-start-from'),
  })
}

/** The text of every `role="alert"` inside `root`. */
function alertTexts(root: HTMLElement): string[] {
  return [...root.querySelectorAll('[role="alert"]')].map(
    (a) => a.textContent ?? '',
  )
}

/** The folder reads the bff was asked for, in order. */
function folderReads(bff: ReturnType<typeof fakeBff>) {
  return bff.to('GET', FILES)
}

// ── 22 C.1a, C.1: the opener and the form ───────────────────────────────

test('22 C.1: the + is shown when the list is empty, and the form then holds no copy radio; it is not shown while the list is loading or after a failed read', async () => {
  // The empty list: the + is there, the form has one choice.
  fakeBff({ modelProfiles: [] })
  render(<App />)
  const main = screen.getByRole('main')
  const empty = await within(main).findByText(en('model-profiles-empty'))
  await flushTasks()
  const opener = newButton(main)
  expect(before(empty, opener), 'the + is after the empty line').toBe(true)
  const form = await openForm(main)
  expect(
    within(form).queryAllByRole('radio'),
    '22 C.1: nothing to copy, so no copy radio (the file and the Hugging Face ' +
      'choices remain, 22C2)',
  ).toHaveLength(2)
  expect(
    radioIn(form, 'model-source-file').checked,
    'A model file is checked',
  ).toBe(true)
  cleanup()
  vi.unstubAllGlobals()
  // While the read is open.
  const gate = deferred()
  fakeBff({ modelProfiles: () => gate.promise })
  render(<App />)
  const loading = screen.getByRole('main')
  await within(loading).findByRole('heading', {
    level: 1,
    name: en('model-profiles-title'),
  })
  expect(
    within(loading).queryByRole('button', { name: en('model-profiles-new') }),
    '22 C.1: no + while loading',
  ).toBeNull()
  gate.resolve(reply(200, LIST))
  cleanup()
  vi.unstubAllGlobals()
  // After a failed read.
  fakeBff({
    modelProfiles: () => reply(503, { key: 'client-unavailable', args: [] }),
  })
  render(<App />)
  const failed = screen.getByRole('main')
  await within(failed).findByText(en('api-client-unavailable'))
  expect(
    within(failed).queryByRole('button', { name: en('model-profiles-new') }),
    '22 C.1: no + after a failed read',
  ).toBeNull()
})

// ── 22 C.3: Cancel, Escape, a new opening ───────────────────────────────

test('22 C.3: Cancel closes the form, clears every field, error and alert, sets aria-expanded false and focuses the +; Escape inside the form does the same; the + pressed again closes it; a new opening is empty with A model file checked and reads the folder again', async () => {
  const { bff, main } = await loaded()
  const opener = newButton(main)
  let form = await openForm(main)
  const name = boxIn(form, en('model-profile-name'))
  type(name, 'Qwen3-27B')
  // A page check leaves an error to clear.
  press(buttonIn(form, 'create-button'))
  await within(form).findByText(en('api-model-profile-name-taken', 'Qwen3-27B'))
  press(buttonIn(form, 'create-cancel'))
  expect(within(main).queryByRole('form'), 'the form is gone').toBeNull()
  expect(opener.getAttribute('aria-expanded'), 'the + is closed').toBe('false')
  expect(document.activeElement, 'the focus is on the +').toBe(opener)
  // A new opening: empty, the file choice checked, the folder read again.
  const before2 = folderReads(bff).length
  form = await openForm(main)
  expect(boxIn(form, en('model-profile-name')).value, 'Name is empty').toBe('')
  expect(
    within(form).queryByText(en('api-model-profile-name-taken', 'Qwen3-27B')),
    'the error is gone',
  ).toBeNull()
  expect(alertTexts(form), 'the alert is gone').toEqual([])
  expect(
    radioIn(form, 'model-source-file').checked,
    'A model file is checked again',
  ).toBe(true)
  expect(
    radioIn(form, 'model-source-copy').checked,
    'the copy choice is not',
  ).toBe(false)
  expect(
    folderReads(bff).length,
    '22 C.3: the new opening reads the folder again',
  ).toBe(before2 + 1)
  // Escape inside the form closes it, as Cancel does.
  type(boxIn(form, en('model-profile-name')), 'edge-box')
  fireEvent.keyDown(form, { key: 'Escape' })
  expect(within(main).queryByRole('form'), 'Escape closed it').toBeNull()
  expect(opener.getAttribute('aria-expanded'), 'the + is closed').toBe('false')
  expect(document.activeElement, 'the focus is on the +').toBe(opener)
  // The + pressed again closes it, and the next opening is clean.
  form = await openForm(main)
  type(boxIn(form, en('model-profile-name')), 'edge-box')
  press(opener)
  expect(within(main).queryByRole('form'), 'the + closed it').toBeNull()
  expect(document.activeElement, 'the focus is on the +').toBe(opener)
  form = await openForm(main)
  expect(boxIn(form, en('model-profile-name')).value, 'Name is empty').toBe('')
})

// ── 22 P.1: the picker's shape ──────────────────────────────────────────

test('22 P.1: the picker is a fieldset named model-profile-start-from whose radios are native inputs named by their labels; only the checked choice controls show, switching keeps what each choice holds, and switching moves no focus', async () => {
  const { main } = await loaded()
  const form = await openForm(main)
  const group = picker(form)
  expect(group.tagName, 'the picker is a fieldset').toBe('FIELDSET')
  const legend = group.querySelector('legend')
  expect(legend, 'the fieldset has a legend').not.toBeNull()
  expect(legend!.textContent?.trim(), 'the legend is the key').toBe(
    en('model-profile-start-from'),
  )
  const file = radioIn(form, 'model-source-file')
  const copy = radioIn(form, 'model-source-copy')
  for (const [input, key] of [
    [file, 'model-source-file'],
    [copy, 'model-source-copy'],
  ] as const) {
    expect(input.tagName, `${key}: a native input`).toBe('INPUT')
    expect(input.getAttribute('type'), `${key}: a radio`).toBe('radio')
    const label = document.querySelector(
      `label[for="${input.id}"]`,
    ) as HTMLLabelElement | null
    expect(label, `${key}: a label names it`).not.toBeNull()
    expect(label!.textContent?.trim(), `${key}: the label is its key`).toBe(
      en(key),
    )
  }
  // Only the checked choice's controls show (22 P.1).
  const fileBox = within(form).getByRole('combobox', {
    name: en('model-profile-model-file'),
  })
  expect(isHidden(fileBox), 'the file field shows with the file choice').toBe(
    false,
  )
  expect(
    within(form).queryByRole('combobox', {
      name: en('model-profile-copy-from'),
    }),
    'the copy select does not show yet',
  ).toBeNull()
  // Switching to the copy: the file field goes, the copy field shows, and the
  // focus stays where it was (22 P.1).
  press(copy)
  expect(copy.checked, 'the copy choice is checked').toBe(true)
  expect(file.checked, 'the file choice is not').toBe(false)
  expect(isHidden(fileBox), 'the file field is gone from the flow').toBe(true)
  const select = within(form).getByRole('combobox', {
    name: en('model-profile-copy-from'),
  })
  expect(document.activeElement, '22 P.1: switching moves no focus').toBe(copy)
  // What each choice keeps: the file field's text, and the copy select's pick.
  press(file)
  expect(isHidden(fileBox), 'the file field is back').toBe(false)
  type(fileBox, 'qwen')
  press(copy)
  expect(
    (select as HTMLSelectElement).value,
    'the copy select is back at its first option',
  ).toBe('')
  // The typed text is pinned where a reader can type it: the field shows
  // again, and the text typed above is kept through the switch (22 P.1).
  press(file)
  expect(
    (
      within(form).getByRole('combobox', {
        name: en('model-profile-model-file'),
      }) as HTMLInputElement
    ).value,
    '22 P.1: the file field keeps what was typed',
  ).toBe('qwen')
})

// ── 22 C.2: the copy choice ─────────────────────────────────────────────

test('22 C.2: the copy choice shows the model-profile-copy-from field with its hint, a native select whose first option is model-profile-copy-choose with value "" selected on opening, then every id of the list the page last read in the list file order, each option text the id alone', async () => {
  const { main } = await loaded()
  const form = await openForm(main)
  press(radioIn(form, 'model-source-copy'))
  const label = within(form).getByText(en('model-profile-copy-from'))
  const hint = within(form).getByText(en('model-profile-copy-hint'))
  expect(isHidden(hint), 'the hint starts closed').toBe(true)
  const select = within(form).getByRole('combobox', {
    name: en('model-profile-copy-from'),
  }) as HTMLSelectElement
  expect(select.tagName, 'the copy choice is a native select').toBe('SELECT')
  expect(
    label.closest('label')?.getAttribute('for'),
    'the label names it',
  ).toBe(select.id)
  expect(
    select.options[0]?.textContent?.trim(),
    'the first option is the choose line',
  ).toBe(en('model-profile-copy-choose'))
  expect(select.options[0]?.value, 'its value is empty').toBe('')
  expect(select.value, 'selected on opening').toBe('')
  expect(
    [...select.options].slice(1).map((o) => o.textContent?.trim()),
    'each option text is the id alone',
  ).toEqual(LIST.map((p) => p.id))
  expect(
    [...select.options].slice(1).map((o) => o.value),
    'each option value is the id',
  ).toEqual(LIST.map((p) => p.id))
})

test('22 C.2: the copy select lists the ids in the list file order, not the table sort, and follows the list the page last read', async () => {
  // The "choices taken" of the 2026-11-26 check: the select keeps the file
  // order, so sorting the table does not change it, and it is the list as the
  // page read it, so a re-read is what the next opening shows.
  let reads = 0
  const bff = fakeBff({
    modelProfiles: () =>
      reply(200, ++reads === 1 ? LIST : [modelProfile('Only-One', false)]),
  })
  render(<App />)
  const main = screen.getByRole('main')
  await within(main).findByRole('table', { name: en('model-profiles-title') })
  await flushTasks()
  // Sort the Name column, twice: the table's order changes, the select's does
  // not.
  const head = within(table(main)).getByRole('button', {
    name: en('model-profiles-column-name'),
  })
  press(head)
  press(head)
  const rows = () =>
    [...table(main).querySelectorAll('tbody tr')].map((tr) =>
      (tr.querySelector('td')?.textContent ?? '').trim(),
    )
  expect(
    rows(),
    'the table is sorted, so it is not in the file order',
  ).not.toEqual(LIST.map((p) => p.id))
  const options = async () => {
    const form = await openForm(screen.getByRole('main'))
    press(radioIn(form, 'model-source-copy'))
    const select = within(form).getByRole('combobox', {
      name: en('model-profile-copy-from'),
    }) as HTMLSelectElement
    return [...select.options].slice(1).map((o) => o.value)
  }
  expect(await options(), 'the select keeps the file order').toEqual(
    LIST.map((p) => p.id),
  )
  // The list read again (the page arrives again): the select follows it.
  navigate('#/profiles/server')
  await flushTasks()
  navigate('#/profiles/model')
  await within(main).findByRole('table', { name: en('model-profiles-title') })
  await flushTasks()
  expect(
    await options(),
    '22 C.2: the ids of the list as the page last read it',
  ).toEqual(['Only-One'])
  expect(bff.to('GET', '/api/profiles/model').length, 'read twice').toBe(2)
})
