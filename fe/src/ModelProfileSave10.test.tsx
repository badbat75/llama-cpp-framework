// The third part of sub-slice 2h.5 of fe/briefs/02h-model-profile-save.md: the
// Advanced card editable (164: its two fields, the Load on startup select and
// the Chat kwargs text, always enabled), what each sends by its kind (133: the
// off / on select sends "true" on and null off, the text sends the text
// trimmed and null when empty after trimming), the truthy reading of
// load-on-startup (167, 2e's 122: on, enabled, true, 1 are on, everything else
// off), the chat-kwargs check (166: the comment markers first, then one JSON
// object) and the new string (169: `field-error-json-object`). Cache RAM's
// input mode (167) is the KV cache card's, in ModelProfileSave8.test.tsx. The card's
// read-only look is 2e's 98, in ModelProfileCards2.test.tsx; the helpers are those of
// ModelProfileSave8 and ModelProfileSave9.test.tsx, which this file repeats (a test file
// imports no other test file).
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from '@testing-library/react'
import { afterEach, beforeEach, expect, test, vi } from 'vitest'
import App from './App.tsx'
import strings from './strings/en_US.json'
import {
  before,
  en,
  fakeBff,
  isHidden,
  modelProfileDoc,
  reply,
  startAt,
  stubDialog,
  type BffOptions,
} from './test/bff.ts'

const ID = 'Qwen3-27B'
const HASH = `#/profiles/model/${ID}`
const PUT = `/api/profiles/model/${ID}`
const LOAD = en('model-profile-load-on-startup')
const KWARGS = en('model-profile-chat-kwargs')
const MARKERS = en('field-error-comment-markers')
const JSONOBJ = en('field-error-json-object')
/** A kwargs value the file holds: a real JSON object, the brief's example. */
const FILE_KWARGS = '{"enable_thinking": false}'
/** What the read gives: both keys of the card present. */
const AS_LOADED: Record<string, string> = {
  'load-on-startup': 'true',
  'chat-template-kwargs': FILE_KWARGS,
}

let dialogs: ReturnType<typeof stubDialog>

beforeEach(() => {
  startAt(HASH)
  dialogs = stubDialog()
})

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
  dialogs.restore()
  startAt('')
})

// ── Helpers (as ModelProfileSave8 and ModelProfileSave9.test.tsx) ──────────────────

/** Waits for the page read with revision `r1` and the card's keys as `values`. */
async function form(
  values: Record<string, string> = AS_LOADED,
  options: BffOptions = {},
) {
  const bff = fakeBff({
    ...options,
    modelProfile: () =>
      reply(
        200,
        modelProfileDoc({}, { revision: 'r1', values: { ...values } }),
      ),
  })
  render(<App />)
  const main = screen.getByRole('main')
  await within(main).findByRole('heading', { level: 1, name: ID })
  await within(main).findByRole('heading', {
    level: 3,
    name: en('model-profile-assets'),
  })
  return { bff, main }
}

/** `AS_LOADED` without the named keys: the file does not hold them. */
function without(...keys: string[]): Record<string, string> {
  const out = { ...AS_LOADED }
  for (const k of keys) delete out[k]
  return out
}

/** The text input of a field, by its accessible name: the label, tied to the
 * input by a `<label>`. The designer's decision for an editable text field
 * with no default box: a plain labelled input, NOT a group named by the label,
 * which would make a screen reader say the name twice. */
function input(main: HTMLElement, label: string): HTMLInputElement {
  return within(main).getByRole('textbox', {
    name: label,
  }) as HTMLInputElement
}

function type(input: HTMLElement, value: string) {
  fireEvent.change(input, { target: { value } })
}

function selectOf(main: HTMLElement, label: string): HTMLSelectElement {
  return within(main).getByRole('combobox', {
    name: label,
  }) as HTMLSelectElement
}

function pick(main: HTMLElement, label: string, value: string) {
  fireEvent.change(selectOf(main, label), { target: { value } })
}

function saveButton(main: HTMLElement): HTMLElement {
  return within(main).getByRole('button', { name: en('save-button') })
}

function discardButton(main: HTMLElement): HTMLElement {
  return within(main).getByRole('button', { name: en('save-discard') })
}

/** Clicks Save changes and waits for the n-th PUT of this model profile. */
async function save(main: HTMLElement, bff: ReturnType<typeof fakeBff>, n = 1) {
  fireEvent.click(saveButton(main))
  await waitFor(() => expect(bff.to('PUT', PUT)).toHaveLength(n))
}

/** The `values` of the n-th recorded PUT of this model profile. */
function sentValues(
  bff: ReturnType<typeof fakeBff>,
  n = 1,
): Record<string, unknown> {
  const body = bff.to('PUT', PUT)[n - 1]!.body as Record<string, unknown>
  return body.values as Record<string, unknown>
}

/**
 * The save bar's status line. The GPUs card keeps its own status region
 * in the DOM from its first render (brief 14 GD1.9), so the bar's line is
 * picked inside the bar, not by counting the page's status regions (section
 * 6 of fe/briefs/14-gpu-distribution.md, ruling of 2026-11-21).
 */
function status(main: HTMLElement): HTMLElement {
  within(main).getByRole('heading', {
    level: 2,
    name: en('model-profile-settings-title'),
  })
  const all = within(main)
    .queryAllByRole('status')
    .filter((s) => s.closest('.save-bar') !== null)
  expect(all, 'the save bar has its status line').toHaveLength(1)
  return all[0]!
}

/** The bar's alert reads `text` (exact): a role="alert" of its own, after the
 * status line and before the buttons (135, 01's 30a). */
function expectAlert(main: HTMLElement, text: string, what = text) {
  const el = within(main).getByText(text)
  const alert = el.closest('[role="alert"]')
  expect(alert, `${what} is in role="alert"`).not.toBe(null)
  const line = status(main)
  expect(line.contains(alert), `${what}: not inside the status line`).toBe(
    false,
  )
  expect(before(line, alert!), `${what}: after the status line`).toBe(true)
  expect(
    before(alert!, discardButton(main)),
    `${what}: before the buttons`,
  ).toBe(true)
}

/** The ids of `aria-describedby`, in order. */
function describedIds(el: HTMLElement): string[] {
  return (el.getAttribute('aria-describedby') ?? '')
    .split(/\s+/)
    .filter(Boolean)
}

/** The text each id of `aria-describedby` gives. */
function described(el: HTMLElement): string[] {
  return describedIds(el).map(
    (id) => document.getElementById(id)?.textContent ?? `(no #${id})`,
  )
}

/** Asserts `el` is marked invalid with the error `text` under it (142, 166). */
function expectError(el: HTMLElement, text: string, what: string) {
  expect(el.getAttribute('aria-invalid'), `${what} aria-invalid`).toBe('true')
  expect(described(el), `${what} aria-describedby`).toContain(text)
  const error = describedIds(el)
    .map((id) => document.getElementById(id))
    .find((e) => e !== null && e.textContent === text)
  expect(error, `${what}: the error is shown`).toBeDefined()
  expect(isHidden(error!), `${what}: the error is shown`).toBe(false)
  expect(before(el, error!), `${what}: the error comes after the control`).toBe(
    true,
  )
}

/** Asserts `el` carries no error. */
function expectNoError(el: HTMLElement, what: string) {
  expect(el.getAttribute('aria-invalid') ?? 'false', what).toBe('false')
}

// ── 164: both fields of the card are enabled, always ───────────────────

test('2h.164: on a read that holds both keys of the card, the select shows the file value and the text input holds the file text: both enabled', async () => {
  const { main } = await form()
  const select = selectOf(main, LOAD)
  expect(select.disabled, 'Load on startup enabled (164)').toBe(false)
  expect(select.value, 'the file value shows').toBe('true')
  const el = input(main, KWARGS)
  expect(el.disabled, 'Chat kwargs enabled (164)').toBe(false)
  expect(el.id, 'the field id').toBe(
    'model-profile-chat-template-kwargs-control',
  )
  expect(el.value, 'the file text as written').toBe(FILE_KWARGS)
  expectNoError(el, 'a valid object loaded: no error')
})

test('2h.164: on a read that holds neither key, both controls are enabled: the select shows off and the text input is empty', async () => {
  const { main } = await form(
    without('load-on-startup', 'chat-template-kwargs'),
  )
  const select = selectOf(main, LOAD)
  expect(select.disabled, 'Load on startup enabled (164)').toBe(false)
  expect(select.value, 'an absent key shows off').toBe('false')
  const el = input(main, KWARGS)
  expect(el.disabled, 'Chat kwargs enabled (164)').toBe(false)
  expect(el.value, 'an absent key shows the empty text').toBe('')
})

// ── 167, 2e's 122: the truthy reading decides what the select shows ─────

/** A present value and the option the select shows for it: `on`, `enabled`,
 * `true`, `1` are on; every other value, `""` included, is off. */
const READINGS: { set: string; shows: string }[] = [
  { set: 'true', shows: 'true' },
  { set: '1', shows: 'true' },
  { set: 'on', shows: 'true' },
  { set: 'yes', shows: 'false' },
  { set: '', shows: 'false' },
]

test.each(READINGS)(
  '2h.167: load-on-startup = $set is read as $shows',
  async (c) => {
    const { main } = await form({ ...AS_LOADED, 'load-on-startup': c.set })
    expect(selectOf(main, LOAD).value, `the select shows ${c.shows}`).toBe(
      c.shows,
    )
  },
)

// ── 133: the off / on select, and the text ─────────────────────────────

test('2h.133: load-on-startup off sends null for the key the file holds, and on sends "true"', async () => {
  const { bff, main } = await form()
  pick(main, LOAD, 'false')
  await save(main, bff)
  expect(sentValues(bff), 'off removes the key').toEqual({
    'load-on-startup': null,
  })
  pick(main, LOAD, 'true')
  await save(main, bff, 2)
  expect(sentValues(bff, 2), 'on writes the key').toEqual({
    'load-on-startup': 'true',
  })
})

/** A valid kwargs text the file holds for the GOOD table, different from every
 * text typed there: 132 makes a text field changed only when its TRIMMED text
 * differs from the loaded value, so a typed text equal to the file's sends
 * nothing. */
const LOADED_OTHER_KWARGS = '{"b":2}'

/** Texts the check accepts (166), with what goes out: the text trimmed of
 * every white space at both ends, and nothing reformatted (133). A U+00A0 and
 * a U+3000 at the ends are white space for `trim`, and `{}` is a JSON object.
 * The field is loaded with `LOADED_OTHER_KWARGS`, so each text is a change. */
const GOOD: { text: string; sends: string }[] = [
  { text: ' {"enable_thinking": false} ', sends: FILE_KWARGS },
  { text: ' {"a":1}　', sends: '{"a":1}' },
  { text: '{}', sends: '{}' },
]

test.each(GOOD)(
  '2h.166, 2h.133: chat kwargs $sends is accepted and sent trimmed, not reformatted',
  async (c) => {
    const { bff, main } = await form({
      ...AS_LOADED,
      'chat-template-kwargs': LOADED_OTHER_KWARGS,
    })
    type(input(main, KWARGS), c.text)
    await save(main, bff)
    expect(sentValues(bff)['chat-template-kwargs'], 'the text as sent').toBe(
      c.sends,
    )
  },
)

// ── 166: the check refuses ─────────────────────────────────────────────

/** Texts that are not one JSON object (166): an array, a bare value, broken
 * JSON, an object with trailing text. */
const BAD_JSON: string[] = [
  '[1]',
  'null',
  '1',
  '"x"',
  '{bad',
  '{"a":1} trailing',
]

test.each(BAD_JSON)(
  '2h.166: chat kwargs %s is refused with field-error-json-object, marked, focused, save-invalid, nothing sent',
  async (text) => {
    const { bff, main } = await form()
    type(input(main, KWARGS), text)
    fireEvent.click(saveButton(main))
    await within(main).findByText(en('save-invalid'))
    const el = input(main, KWARGS)
    expectError(el, JSONOBJ, `chat kwargs ${text}`)
    expect(document.activeElement, 'focus on Chat kwargs').toBe(el)
    expectAlert(main, en('save-invalid'))
    expect(bff.to('PUT', PUT), 'nothing sent').toHaveLength(0)
  },
)

/** Texts with a `;` or a `#` that are also no JSON object, or would pass the
 * JSON check: the comment check comes first (166). */
const BAD_MARKERS: string[] = ['{"a":"#"}', '{"a":1} ; note']

test.each(BAD_MARKERS)(
  '2h.166: chat kwargs %s is refused with the comment message, the marker check first',
  async (text) => {
    const { bff, main } = await form()
    type(input(main, KWARGS), text)
    fireEvent.click(saveButton(main))
    await within(main).findByText(en('save-invalid'))
    const el = input(main, KWARGS)
    expectError(el, MARKERS, `chat kwargs ${text}`)
    expect(described(el), 'not the JSON message').not.toContain(JSONOBJ)
    expect(document.activeElement, 'focus on Chat kwargs').toBe(el)
    expect(bff.to('PUT', PUT), 'nothing sent').toHaveLength(0)
  },
)

// ── 166, 133: the empty text, and a value the file holds ───────────────

test('2h.166, 2h.133: an emptied Chat kwargs of a key the file holds is not checked and sends null', async () => {
  const { bff, main } = await form()
  type(input(main, KWARGS), '')
  expectNoError(input(main, KWARGS), 'the empty text is not checked')
  await save(main, bff)
  expectNoError(input(main, KWARGS), 'the empty text is not checked on Save')
  expect(sentValues(bff), 'the key the file held is removed').toEqual({
    'chat-template-kwargs': null,
  })
})

test('2h.133: a Chat kwargs the file lacks, typed and emptied again, is clean: nothing is sent and no check runs', async () => {
  const { bff, main } = await form(without('chat-template-kwargs'))
  type(input(main, KWARGS), '{"a":1}')
  type(input(main, KWARGS), '')
  expect(status(main).textContent, 'back to the loaded text: clean').toBe('')
  fireEvent.click(saveButton(main))
  expectNoError(input(main, KWARGS), 'no check on the empty text')
  expect(bff.to('PUT', PUT), 'a key the file lacks sends nothing').toHaveLength(
    0,
  )
})

test('2h.166: a Chat kwargs unchanged from the file is not sent, and the loaded value is still checked on Save', async () => {
  // A value the file holds that is no JSON object: untouched, so nothing to
  // send, yet the check runs and Save refuses.
  const first = await form({ ...AS_LOADED, 'chat-template-kwargs': '[1]' })
  expect(status(first.main).textContent, 'untouched: clean').toBe('')
  fireEvent.click(saveButton(first.main))
  await within(first.main).findByText(en('save-invalid'))
  const el = input(first.main, KWARGS)
  expectError(el, JSONOBJ, 'the loaded [1] is checked')
  expectAlert(first.main, en('save-invalid'))
  expect(first.bff.to('PUT', PUT), 'nothing sent').toHaveLength(0)
  // A valid value left untouched: a change elsewhere carries the save, and the
  // kwargs stay out of the body. One page at a time: the first form unmounts
  // before the second render, so a single <main> is on screen.
  cleanup()
  const second = await form()
  pick(second.main, LOAD, 'false')
  await save(second.main, second.bff)
  expect(
    Object.keys(sentValues(second.bff)),
    'only the changed key travels',
  ).toEqual(['load-on-startup'])
})

// ── 169: the new string ───────────────────────────────────────────────

test('2h.169: strings/en_US.json carries field-error-json-object with the brief text of 169', () => {
  const table: Record<string, string> = strings
  expect(
    table['field-error-json-object'],
    'the strings file holds the new key, as the test English does',
  ).toBe(en('field-error-json-object'))
  expect(
    en('field-error-json-object'),
    'the brief text, which names the braces and the example',
  ).toBe('Enter one JSON object in braces, such as {"enable_thinking": false}.')
})

// ── 132 (decided): "changed" is judged on the trimmed text ─────────────
// Spaces alone never make the form dirty: the typed text, trimmed, is
// compared with the loaded value, and a trimmed-equal text travels in no
// body.

test('2h.132: a Chat kwargs the file lacks, typed with spaces only, keeps the form clean: nothing about the key is ever sent', async () => {
  const { bff, main } = await form(without('chat-template-kwargs'))
  type(input(main, KWARGS), '   ')
  expect(status(main).textContent, 'spaces only: clean').toBe('')
  fireEvent.click(saveButton(main))
  expect(bff.to('PUT', PUT), 'the key is never sent').toHaveLength(0)
})

test('2h.132: a trailing space added to a Chat kwargs equal to the file text keeps the form clean: nothing is sent', async () => {
  const { bff, main } = await form({
    ...AS_LOADED,
    'chat-template-kwargs': '{"a":1}',
  })
  type(input(main, KWARGS), '{"a":1} ')
  expect(status(main).textContent, 'a trailing space: clean').toBe('')
  fireEvent.click(saveButton(main))
  expect(bff.to('PUT', PUT), 'nothing sent').toHaveLength(0)
})

test('2h.132: a Chat kwargs equal to the file text when trimmed travels in no body: the PUT of a save from a changed select carries only that key', async () => {
  const { bff, main } = await form()
  type(input(main, KWARGS), `${FILE_KWARGS} `)
  pick(main, LOAD, 'false')
  await save(main, bff)
  expect(
    Object.keys(sentValues(bff)),
    'the trimmed-equal kwargs stay out of the body',
  ).toEqual(['load-on-startup'])
})

// ── 164: the Chat kwargs input is a code input ──────────────────────────
// JSON the user types by hand: nothing must autocorrect it, and no length
// limit may cut it.

test('2h.164: the Chat kwargs input has spellcheck false, autocapitalize off, autocorrect off, autocomplete off, no inputmode and no maxlength', async () => {
  const { main } = await form()
  expect(
    within(main).queryByRole('group', { name: KWARGS }),
    'no element with role="group" named by the label (a plain labelled input)',
  ).toBeNull()
  const el = input(main, KWARGS)
  expect(el.getAttribute('spellcheck'), 'no spell check on typed JSON').toBe(
    'false',
  )
  expect(el.getAttribute('autocapitalize'), 'no capitalizing').toBe('off')
  expect(el.getAttribute('autocorrect'), 'no auto-correction').toBe('off')
  expect(el.getAttribute('autocomplete'), 'no autofill').toBe('off')
  expect(el.getAttribute('inputmode'), 'the plain text keyboard').toBe(null)
  expect(el.getAttribute('maxlength'), 'no length limit').toBe(null)
})
