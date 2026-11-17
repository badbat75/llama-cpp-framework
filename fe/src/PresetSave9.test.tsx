// The second part of sub-slice 2h.5 of fe/briefs/02h-preset-save.md: the
// Sampling overrides card editable (164: every field of the card always
// enabled, its six number fields behind the default box built in 2h.4), the
// decimal check of its five float keys (165: the text trimmed and with `,` read
// as `.`, a finite decimal, sent as that text; its integer key `top-k` by the
// any-whole-number row of 157, so `field-error-whole-number`), the input mode
// of the card (158: floats and the any-whole-number key get none) and the new
// string (169: `field-error-decimal`). Advanced, the chat-kwargs JSON check
// (166) and `spec-draft-p-min` (2h.9) are not here. The card's read-only look
// is 2e's 97, in PresetCards2.test.tsx; the helpers are those of PresetSave6,
// PresetSave7 and PresetSave8.test.tsx, which this file repeats (a test file
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
  presetDoc,
  reply,
  startAt,
  stubDialog,
  type BffOptions,
} from './test/bff.ts'

const ID = 'Qwen3-27B'
const HASH = `#/models/${ID}`
const PUT = `/api/presets/${ID}`
const MODE = en('preset-embedding-mode')
const TEMP = en('preset-temperature')
const TOPK = 'Top-K'
const TOPP = 'Top-P'
const MINP = 'Min-P'
const REPEAT = en('preset-repeat-penalty')
const PRESENCE = en('preset-presence-penalty')
const BACKEND = en('preset-backend-sampling')
const DEFAULT = en('field-default')
const DECIMAL = en('field-error-decimal')
const WHOLE = en('field-error-whole-number')
/** The card's six number fields, all behind a default box (2e's 97: each
 * absent key shows "box checked, ''"), by key, with the label. `integer` marks
 * the one integer key (157's any-whole-number row); the five float keys take
 * the decimal check of 165. */
const BOXED: { key: string; label: string; integer: boolean }[] = [
  { key: 'temp', label: TEMP, integer: false },
  { key: 'top-k', label: TOPK, integer: true },
  { key: 'top-p', label: TOPP, integer: false },
  { key: 'min-p', label: MINP, integer: false },
  { key: 'repeat-penalty', label: REPEAT, integer: false },
  { key: 'presence-penalty', label: PRESENCE, integer: false },
]
/** The card's fields, in the order of 2e's table 97. Backend sampling is the
 * card's one select (a tri-state, 133). */
const FIELDS: { key: string; label: string }[] = [
  ...BOXED.map(({ key, label }) => ({ key, label })),
  { key: 'backend-sampling', label: BACKEND },
]
/** What the read gives. `min-p` and `backend-sampling` are absent, so Min-P's
 * box starts ticked and the tri-state shows default. Presence penalty is the
 * brief's `1,5` (165): a comma the check reads as a dot. */
const AS_LOADED: Record<string, string> = {
  temp: '0.6',
  'top-k': '20',
  'top-p': '0.95',
  'repeat-penalty': '1.05',
  'presence-penalty': '1,5',
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

// ── Helpers (as PresetSave6, PresetSave7 and PresetSave8.test.tsx) ───────

/** Waits for the page read with revision `r1` and the card's keys as `values`. */
async function form(
  values: Record<string, string> = AS_LOADED,
  options: BffOptions = {},
) {
  const bff = fakeBff({
    ...options,
    preset: () =>
      reply(200, presetDoc({}, { revision: 'r1', values: { ...values } })),
  })
  render(<App />)
  const main = screen.getByRole('main')
  await within(main).findByRole('heading', { level: 1, name: ID })
  await within(main).findByRole('heading', {
    level: 3,
    name: en('preset-assets'),
  })
  return { bff, main }
}

/** `AS_LOADED` without the named keys: the file does not hold them. */
function without(...keys: string[]): Record<string, string> {
  const out = { ...AS_LOADED }
  for (const k of keys) delete out[k]
  return out
}

/** The field whose label is `label` (its group). */
function field(main: HTMLElement, label: string): HTMLElement {
  return within(main).getByRole('group', { name: label })
}

/** The number input of a field, by its accessible name. */
function box(main: HTMLElement, label: string): HTMLInputElement {
  return within(field(main, label)).getByRole('textbox', {
    name: label,
  }) as HTMLInputElement
}

/** The field's default box. */
function checkBox(main: HTMLElement, label: string): HTMLInputElement {
  return within(field(main, label)).getByRole('checkbox', {
    name: DEFAULT,
  }) as HTMLInputElement
}

/** Focuses and clicks the default box, as a pointer or Enter does. */
function toggle(main: HTMLElement, label: string): HTMLInputElement {
  const c = checkBox(main, label)
  c.focus()
  fireEvent.click(c)
  return c
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

/** Clicks Save changes and waits for the n-th PUT of this preset. */
async function save(main: HTMLElement, bff: ReturnType<typeof fakeBff>, n = 1) {
  fireEvent.click(saveButton(main))
  await waitFor(() => expect(bff.to('PUT', PUT)).toHaveLength(n))
}

/** The `values` of the n-th recorded PUT of this preset. */
function sentValues(
  bff: ReturnType<typeof fakeBff>,
  n = 1,
): Record<string, unknown> {
  const body = bff.to('PUT', PUT)[n - 1]!.body as Record<string, unknown>
  return body.values as Record<string, unknown>
}

/** The save bar's status line (the only role="status" in the settings). */
function status(main: HTMLElement): HTMLElement {
  const h2 = within(main).getByRole('heading', {
    level: 2,
    name: en('preset-settings-title'),
  })
  const all = within(main)
    .queryAllByRole('status')
    .filter((s) => before(h2, s))
  expect(all, 'one status line once the page is loaded').toHaveLength(1)
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

/** Asserts `el` is marked invalid with the error `text` under it (142, 165). */
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

/** Types `text` in the field, ticking its box first when the key is absent
 * (the box starts ticked, so the input is disabled and empty). */
function enter(main: HTMLElement, key: string, label: string, text: string) {
  if (!(key in AS_LOADED)) toggle(main, label)
  type(box(main, label), text)
}

// ── 164: every field of the card is enabled, always ─────────────────────

test('2h.164: on a read that holds every key of the card, all seven controls are enabled and the six boxes are unticked with the input holding the file text', async () => {
  const every: Record<string, string> = {
    ...AS_LOADED,
    'min-p': '0.05',
    'backend-sampling': 'true',
  }
  const { main } = await form(every)
  for (const { key, label } of FIELDS) {
    const el =
      key === 'backend-sampling' ? selectOf(main, label) : box(main, label)
    expect(el.disabled, `${key}: enabled (164)`).toBe(false)
  }
  for (const { key, label } of BOXED) {
    expect(checkBox(main, label).disabled, `${key}: the box enabled`).toBe(
      false,
    )
    expect(
      checkBox(main, label).checked,
      `${key}: the box unticked for a key the file holds`,
    ).toBe(false)
    expect(
      box(main, label).value,
      `${key}: the input holds the file text`,
    ).toBe(every[key])
  }
  expect(
    selectOf(main, BACKEND).value,
    'backend sampling shows the file value',
  ).toBe('true')
})

test('2h.164: on a read that holds none of the card keys, the six boxes are ticked and their inputs disabled and empty, and the select shows default: all enabled', async () => {
  const { main } = await form(without(...FIELDS.map((f) => f.key)))
  for (const { key, label } of BOXED) {
    expect(checkBox(main, label).disabled, `${key}: the box enabled`).toBe(
      false,
    )
    expect(
      checkBox(main, label).checked,
      `${key}: the box ticked for a key the file lacks`,
    ).toBe(true)
    expect(box(main, label).disabled, `${key}: the input follows the box`).toBe(
      true,
    )
    expect(box(main, label).value, `${key}: the input is empty`).toBe('')
  }
  const select = selectOf(main, BACKEND)
  expect(select.disabled, 'backend sampling enabled (164)').toBe(false)
  expect(select.value, 'an absent key shows default').toBe('default')
})

// ── 165: the decimal check accepts, and what it sends ───────────────────

/** Texts the float keys take (165), with what goes out: the text trimmed, with
 * `,` replaced by `.` (a leading sign and an exponent kept as written). The
 * `top-k` rows are 157's any-whole-number row: any sign passes, and what goes
 * out is the canonical number (no sign for a positive, no leading zeros). An
 * exponent that underflows is still a finite number (165), so `1e-999` is 0 and
 * travels as typed. */
const GOOD: { key: string; label: string; text: string; sends: string }[] = [
  { key: 'temp', label: TEMP, text: '0,7', sends: '0.7' },
  { key: 'top-p', label: TOPP, text: ' 0,7 ', sends: '0.7' },
  { key: 'min-p', label: MINP, text: '1e-3', sends: '1e-3' },
  { key: 'min-p', label: MINP, text: '1e-999', sends: '1e-999' },
  { key: 'repeat-penalty', label: REPEAT, text: '-0,5', sends: '-0.5' },
  { key: 'temp', label: TEMP, text: '5.', sends: '5.' },
  { key: 'top-p', label: TOPP, text: '.5', sends: '.5' },
  { key: 'top-k', label: TOPK, text: '-5', sends: '-5' },
  { key: 'top-k', label: TOPK, text: '+08', sends: '8' },
]

test.each(GOOD)(
  '2h.165: $key takes "$text" and sends $sends (trimmed, the comma read as a dot)',
  async (c) => {
    const { bff, main } = await form()
    enter(main, c.key, c.label, c.text)
    await save(main, bff)
    expect(sentValues(bff)[c.key], `the text as sent`).toBe(c.sends)
  },
)

// ── 165, 157: the check refuses ─────────────────────────────────────────

/** Texts that fail (165 for the float keys, 157's any-whole-number row for
 * `top-k`). `untick` is the box of a key the file lacks: unticking it says
 * "not the default", so an empty text is asked for. `inf` and `nan` are no
 * decimal number; the `1e999` rows are the decimal shape as text and infinity
 * as a number, so 165's finite rule refuses them (the manager does too). */
const BAD: {
  key: string
  label: string
  text: string
  error: string
  untick?: boolean
}[] = [
  { key: 'temp', label: TEMP, text: 'hot', error: DECIMAL },
  { key: 'temp', label: TEMP, text: 'inf', error: DECIMAL },
  { key: 'top-p', label: TOPP, text: 'nan', error: DECIMAL },
  { key: 'repeat-penalty', label: REPEAT, text: '1,05.2', error: DECIMAL },
  { key: 'temp', label: TEMP, text: '1e999', error: DECIMAL },
  { key: 'temp', label: TEMP, text: '-1e999', error: DECIMAL },
  { key: 'min-p', label: MINP, text: '', error: DECIMAL, untick: true },
  { key: 'top-k', label: TOPK, text: '1,5', error: WHOLE },
]

test.each(BAD)(
  '2h.165: $key rejects "$text" with $error, marks it, focuses it, alerts save-invalid and sends nothing',
  async (c) => {
    const { bff, main } = await form()
    if (c.untick) toggle(main, c.label)
    type(box(main, c.label), c.text)
    fireEvent.click(saveButton(main))
    await within(main).findByText(en('save-invalid'))
    const el = box(main, c.label)
    expectError(el, c.error, c.label)
    expect(document.activeElement, `focus on ${c.label}`).toBe(el)
    expectAlert(main, en('save-invalid'))
    expect(bff.to('PUT', PUT), 'nothing sent').toHaveLength(0)
  },
)

test('2h.165: two float keys fail, so the focus goes to the first of them in DOM order: both are marked and nothing is sent', async () => {
  const { bff, main } = await form()
  type(box(main, TOPP), 'nan')
  type(box(main, TEMP), 'hot')
  fireEvent.click(saveButton(main))
  await within(main).findByText(en('save-invalid'))
  const temp = box(main, TEMP)
  expectError(temp, DECIMAL, TEMP)
  expectError(box(main, TOPP), DECIMAL, TOPP)
  expect(document.activeElement, 'focus on the first failing control').toBe(
    temp,
  )
  expectAlert(main, en('save-invalid'))
  expect(bff.to('PUT', PUT), 'nothing sent').toHaveLength(0)
})

// ── 165: the loaded comma value is a value, not an error, and not a change ─

test('2h.165: the loaded presence-penalty 1,5 shows as written, passes the check, and unchanged is not sent', async () => {
  const { bff, main } = await form()
  const el = box(main, PRESENCE)
  expect(el.disabled, 'the input enabled (164)').toBe(false)
  expect(el.value, 'the file text as written').toBe('1,5')
  expect(status(main).textContent, 'untouched: clean').toBe('')
  fireEvent.click(saveButton(main))
  expect(bff.to('PUT', PUT), 'a clean Save sends nothing').toHaveLength(0)
  // A change elsewhere carries the save: the untouched 1,5 stays out of it.
  pick(main, MODE, 'true')
  await save(main, bff)
  expect(Object.keys(sentValues(bff)), 'only the changed key travels').toEqual([
    'embeddings',
  ])
})

// ── 158: the card has no input mode ───────────────────────────────────

test('2h.158: the card is editable, and none of its inputs carries an input mode: the five float keys and the any-whole-number top-k get none', async () => {
  const { main } = await form({ ...AS_LOADED, 'min-p': '0.05' })
  for (const { key, label, integer } of BOXED) {
    const el = box(main, label)
    expect(el.disabled, `${key}: the input enabled (164)`).toBe(false)
    expect(el.id, `${key}: the field id`).toBe(`preset-${key}-control`)
    expect(
      el.getAttribute('inputmode'),
      `${key}: ${integer ? 'the any-whole-number row' : 'a float key'} has no input mode`,
    ).toBeNull()
  }
})

// ── 133: the card's boxes remove a key the file holds ──────────────────

test('2h.133, 2h.156: ticking the box of temp and of the comma-written presence-penalty sends null for both keys', async () => {
  const { bff, main } = await form()
  toggle(main, TEMP)
  toggle(main, PRESENCE)
  await save(main, bff)
  expect(sentValues(bff), 'both keys the file held are removed').toEqual({
    temp: null,
    'presence-penalty': null,
  })
})

// ── 169: the new string ───────────────────────────────────────────────

test('2h.169: strings/en_US.json carries field-error-decimal with the brief text of 169', () => {
  const table: Record<string, string> = strings
  expect(
    table['field-error-decimal'],
    'the strings file holds the new key, as the test English does',
  ).toBe(en('field-error-decimal'))
  expect(
    en('field-error-decimal'),
    'the brief text, which names the comma',
  ).toBe('Enter a number, such as 0.7 (a decimal comma also works).')
})
