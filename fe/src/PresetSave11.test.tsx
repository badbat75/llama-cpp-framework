// The Chat / reasoning card editable, sub-slice 2h.6 of
// fe/briefs/02h-preset-save.md (170 to 174): every field of the card always
// enabled (170, 2e's table 101), what each sends by its kind (171, 133: the
// `jinja` and `prefill-assistant` checkboxes, the `reasoning`,
// `reasoning-format`, `reasoning-effort` and `reasoning-preserve` selects), the
// Budget message text (172: the comment markers refused, no placeholder, the
// text sent trimmed, null when emptied), the two integer fields (173: 157's
// any-whole-number row, 158's no input mode), and the loaded states of 174 (a
// `jinja` written `0` with its written-as line, an absent `prefill-assistant`
// checked). The designer's decision for an editable text field with no default
// box (a plain labelled input, no group named by the label) is pinned here for
// Budget message and in PresetSave10.test.tsx for Chat kwargs. The card's
// read-only look is 2e's 101, in PresetCards3.test.tsx; the helpers are those of
// PresetSave8, PresetSave9 and PresetSave10.test.tsx, which this file repeats (a
// test file imports no other test file).
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
const JINJA = en('preset-chat-template')
const JINJA_TOGGLE = en('preset-chat-template-toggle')
const PRE = en('preset-prefill-assistant')
const PRE_TOGGLE = en('preset-prefill-assistant-toggle')
const REASON = en('preset-reasoning')
const FORMAT = en('preset-reasoning-format')
const EFFORT = en('preset-reasoning-effort')
const PRESERVE = en('preset-keep-past-thinking')
/** The KV cache card's tri-state, editable since 2h.5's 164: the same reading
 * as `reasoning-preserve`, pinned beside it. */
const FLASH = en('preset-flash-attention')
const BUDGET = en('preset-thinking-budget')
const MSG = en('preset-budget-message')
const MAXOUT = en('preset-max-output')
const MARKERS = en('field-error-comment-markers')
const WHOLE = en('field-error-whole-number')
/** What the read gives: every key of the card present, each a value the
 * configurator itself would write (so no written-as line). */
const AS_LOADED: Record<string, string> = {
  jinja: 'true',
  'prefill-assistant': 'true',
  reasoning: 'on',
  'reasoning-format': 'none',
  'reasoning-effort': 'low',
  'reasoning-preserve': 'true',
  'reasoning-budget': '8192',
  'reasoning-budget-message': 'Wrap up now.',
  'n-predict': '8192',
}

/** One field of 2e's table 101: its key, the label as the page shows it, the
 * kind of its control, and what the field shows when the file lacks the key. */
type CardField = {
  key: string
  label: string
  kind: 'checkbox' | 'select' | 'number' | 'text'
  /** A checkbox: the text of its toggle, which names the control. */
  toggle?: string
  /** A checkbox: the state an absent key shows (101). */
  absentChecked?: boolean
  /** A select: the option value an absent key shows (101). */
  absent?: string
}

/** The card's fields in the order of table 101. */
const FIELDS: CardField[] = [
  {
    key: 'jinja',
    label: JINJA,
    kind: 'checkbox',
    toggle: JINJA_TOGGLE,
    absentChecked: true,
  },
  {
    key: 'prefill-assistant',
    label: PRE,
    kind: 'checkbox',
    toggle: PRE_TOGGLE,
    absentChecked: true,
  },
  { key: 'reasoning', label: REASON, kind: 'select', absent: 'auto' },
  { key: 'reasoning-format', label: FORMAT, kind: 'select', absent: 'auto' },
  {
    key: 'reasoning-effort',
    label: EFFORT,
    kind: 'select',
    absent: 'default',
  },
  {
    key: 'reasoning-preserve',
    label: PRESERVE,
    kind: 'select',
    absent: 'default',
  },
  { key: 'reasoning-budget', label: BUDGET, kind: 'number' },
  { key: 'reasoning-budget-message', label: MSG, kind: 'text' },
  { key: 'n-predict', label: MAXOUT, kind: 'number' },
]

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

// ── Helpers (as PresetSave8, PresetSave9 and PresetSave10.test.tsx) ─────

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

function selectOf(main: HTMLElement, label: string): HTMLSelectElement {
  return within(main).getByRole('combobox', {
    name: label,
  }) as HTMLSelectElement
}

/** A card checkbox, named by its own toggle text (87). */
function checkBoxOf(main: HTMLElement, toggle: string): HTMLInputElement {
  return within(main).getByRole('checkbox', {
    name: toggle,
  }) as HTMLInputElement
}

/** A field's input, by its accessible name: the label, tied to the input by a
 * `<label>` (a text field with no default box) or naming the fieldset. */
function inputOf(main: HTMLElement, label: string): HTMLInputElement {
  return within(main).getByRole('textbox', {
    name: label,
  }) as HTMLInputElement
}

/** A number field's default box, by the page's id for the key. */
function boxOf(key: string): HTMLInputElement {
  return document.getElementById(`preset-${key}-box`) as HTMLInputElement
}

function pick(main: HTMLElement, label: string, value: string) {
  fireEvent.change(selectOf(main, label), { target: { value } })
}

function type(main: HTMLElement, label: string, value: string) {
  fireEvent.change(inputOf(main, label), { target: { value } })
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

/** Asserts `el` is marked invalid with the error `text` under it (142). */
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

/** The written-as line of a field (124, 134), by the page's id for its key. */
function writtenLine(key: string): HTMLElement | null {
  return document.getElementById(`preset-written-${key}`)
}

/** The save bar's buttons are inert: the form is clean. */
function expectInert(main: HTMLElement, what: string) {
  for (const b of [saveButton(main), discardButton(main)])
    expect(b.getAttribute('aria-disabled'), `${what}: the buttons inert`).toBe(
      'true',
    )
}

// ── 170: every field of the card is enabled, always ─────────────────────

test('2h.170: every field of the card has its control enabled, on a read that holds every key and on one that holds none, and each shows what the read gave', async () => {
  const { main } = await form()
  for (const f of FIELDS) {
    if (f.kind === 'checkbox') {
      const el = checkBoxOf(main, f.toggle!)
      expect(el.disabled, `${f.key}: the checkbox enabled (170)`).toBe(false)
      expect(el.id, `${f.key}: the field id`).toBe(`preset-${f.key}-control`)
      expect(el.checked, `${f.key}: the file value shows checked`).toBe(true)
    } else if (f.kind === 'select') {
      const el = selectOf(main, f.label)
      expect(el.disabled, `${f.key}: the select enabled (170)`).toBe(false)
      expect(el.id, `${f.key}: the field id`).toBe(`preset-${f.key}-control`)
      expect(el.value, `${f.key}: the file value shows`).toBe(AS_LOADED[f.key])
    } else if (f.kind === 'text') {
      const el = inputOf(main, f.label)
      expect(el.disabled, `${f.key}: the input enabled (170)`).toBe(false)
      expect(el.id, `${f.key}: the field id`).toBe(`preset-${f.key}-control`)
      expect(el.value, `${f.key}: the file text shows`).toBe(AS_LOADED[f.key])
    } else {
      const c = boxOf(f.key)
      expect(c, `${f.key}: the field has its default box`).not.toBeNull()
      expect(c.disabled, `${f.key}: the box enabled (170)`).toBe(false)
      expect(c.checked, `${f.key}: a key the file has: box unticked`).toBe(
        false,
      )
      const el = inputOf(main, f.label)
      expect(
        el.disabled,
        `${f.key}: the input follows the unticked box (156)`,
      ).toBe(false)
      expect(el.value, `${f.key}: the input holds the file text`).toBe(
        AS_LOADED[f.key],
      )
    }
  }
  // Tear the first page down first: a second form() with both pages in the
  // document finds the first page's fields.
  cleanup()
  const absent = await form(without(...FIELDS.map((f) => f.key)))
  for (const f of FIELDS) {
    if (f.kind === 'checkbox')
      expect(
        checkBoxOf(absent.main, f.toggle!).checked,
        `${f.key}: an absent key shows it ${f.absentChecked ? 'checked' : 'unchecked'}`,
      ).toBe(f.absentChecked)
    else if (f.kind === 'select')
      expect(
        selectOf(absent.main, f.label).value,
        `${f.key}: an absent key shows ${f.absent}`,
      ).toBe(f.absent)
    else if (f.kind === 'text')
      expect(inputOf(absent.main, f.label).value, `${f.key}: no text`).toBe('')
    else {
      expect(
        boxOf(f.key).checked,
        `${f.key}: an absent key ticks the box (156)`,
      ).toBe(true)
      const el = inputOf(absent.main, f.label)
      expect(
        el.disabled,
        `${f.key}: a ticked box disables the input (156)`,
      ).toBe(true)
      expect(el.value, `${f.key}: a ticked box empties the input`).toBe('')
    }
  }
})

// ── 171, 174: the two checkboxes ────────────────────────────────────────

test('2h.171, 2h.174: jinja checked sends "true" and unchecked sends "false", and a loaded 0 is unchecked with its written-as line: checking it sends "true"', async () => {
  // The file writes true: checked, and no written-as line (the configurator
  // writes that spelling); unchecking is a change and sends "false".
  const first = await form()
  const jinja = checkBoxOf(first.main, JINJA_TOGGLE)
  expect(jinja.checked, 'jinja = true shows checked').toBe(true)
  expect(
    writtenLine('jinja'),
    'true is a spelling the configurator writes: no written-as line',
  ).toBeNull()
  fireEvent.click(jinja)
  await save(first.main, first.bff)
  expect(sentValues(first.bff), 'unchecked sends "false"').toEqual({
    jinja: 'false',
  })
  // An absent key shows checked (101: the default is on), and unchecking sends
  // "false" (133's checkbox row).
  cleanup()
  const second = await form(without('jinja'))
  const on = checkBoxOf(second.main, JINJA_TOGGLE)
  expect(on.checked, 'jinja absent shows checked').toBe(true)
  fireEvent.click(on)
  await save(second.main, second.bff)
  expect(sentValues(second.bff), 'unchecked sends "false"').toEqual({
    jinja: 'false',
  })
  // The file writes 0: the flag reading is unchecked (2e's 122) and the
  // written-as line spells it (124); checking sends "true", and 134 takes the
  // line away with the change.
  cleanup()
  const third = await form({ ...AS_LOADED, jinja: '0' })
  const zero = checkBoxOf(third.main, JINJA_TOGGLE)
  expect(zero.checked, 'jinja = 0 is read as unchecked').toBe(false)
  const line = writtenLine('jinja')
  expect(line, 'the written-as line shows the spelling').not.toBeNull()
  expect(line!.textContent, 'the spelling as the file writes it').toBe(
    en('preset-written-as', '0'),
  )
  expect(isHidden(line!), 'the line is read by assistive technology').toBe(
    false,
  )
  expect(described(zero), 'the checkbox is described by the line').toContain(
    en('preset-written-as', '0'),
  )
  fireEvent.click(zero)
  expect(
    writtenLine('jinja'),
    'the field changed: the line is gone (134)',
  ).toBe(null)
  await save(third.main, third.bff)
  expect(sentValues(third.bff), 'checking sends "true"').toEqual({
    jinja: 'true',
  })
})

test('2h.171, 2h.174: prefill-assistant absent shows checked, unchecking is a change and checking it back is clean, unchecking then sends "false"; a loaded false is unchecked and checking sends null', async () => {
  // Absent: the key is not in the file, so the checked control is the loaded
  // state (101). Unchecking is a change, checking it back is not (174): no save
  // in between, so nothing was written and the form is clean again.
  const first = await form(without('prefill-assistant'))
  const pre = checkBoxOf(first.main, PRE_TOGGLE)
  expect(pre.disabled, 'prefill-assistant enabled (170)').toBe(false)
  expect(pre.checked, 'absent shows checked').toBe(true)
  fireEvent.click(pre)
  expect(status(first.main).textContent, 'unchecked: a change').toBe(
    en('save-unsaved'),
  )
  expect(pre.checked, 'unchecked shows unchecked').toBe(false)
  fireEvent.click(checkBoxOf(first.main, PRE_TOGGLE))
  expect(pre.checked, 'checked back').toBe(true)
  expect(status(first.main).textContent, 'checked again: clean').toBe('')
  expectInert(first.main, 'checked again')
  fireEvent.click(saveButton(first.main))
  expect(
    first.bff.to('PUT', PUT),
    'the loaded state sends nothing: no PUT at all',
  ).toHaveLength(0)
  cleanup()
  // Unchecking the same absent key and saving sends "false" (133's row).
  const second = await form(without('prefill-assistant'))
  fireEvent.click(checkBoxOf(second.main, PRE_TOGGLE))
  await save(second.main, second.bff)
  expect(sentValues(second.bff), 'unchecked sends "false"').toEqual({
    'prefill-assistant': 'false',
  })
  cleanup()
  const third = await form({ ...AS_LOADED, 'prefill-assistant': 'false' })
  const off = checkBoxOf(third.main, PRE_TOGGLE)
  expect(off.checked, 'prefill-assistant = false shows unchecked').toBe(false)
  fireEvent.click(off)
  await save(third.main, third.bff)
  expect(sentValues(third.bff), 'checked sends null (133)').toEqual({
    'prefill-assistant': null,
  })
})

// ── 171, 133: the selects of words, each option its own value ───────────

/** Picks of `reasoning` and `reasoning-format` (133's row: no `default`
 * option, so every option sends its own value, `auto` included). Each row
 * loads another value, so the pick is a change. */
const PICKS = [
  { key: 'reasoning', label: REASON, load: 'off', to: 'auto', sends: 'auto' },
  { key: 'reasoning', label: REASON, load: 'auto', to: 'off', sends: 'off' },
  {
    key: 'reasoning-format',
    label: FORMAT,
    load: 'none',
    to: 'deepseek',
    sends: 'deepseek',
  },
  // `auto` is this row's absent option too, and it is still a value the file
  // holds (133's row of the selects with no `default` option).
  {
    key: 'reasoning-format',
    label: FORMAT,
    load: 'off',
    to: 'auto',
    sends: 'auto',
  },
]

test.each(PICKS)(
  '2h.171, 2h.133: $key loaded $load picks $to and sends its own value $sends, not null',
  async (c) => {
    const { bff, main } = await form({ ...AS_LOADED, [c.key]: c.load })
    expect(
      selectOf(main, c.label).disabled,
      `${c.key}: the select enabled (170)`,
    ).toBe(false)
    pick(main, c.label, c.to)
    await save(main, bff)
    expect(sentValues(bff)[c.key], `the option's own value`).toBe(c.sends)
  },
)

test('2h.132, 2h.133: an absent reasoning and reasoning-format show auto, and picking auto back is clean: the buttons inert, nothing sent', async () => {
  const first = await form(without('reasoning'))
  const reason = selectOf(first.main, REASON)
  expect(reason.value, 'an absent key shows auto').toBe('auto')
  expect(
    Array.from(reason.options).filter((o) => o.value === 'auto').length,
    'auto is listed once: no extra option added',
  ).toBe(1)
  pick(first.main, REASON, 'auto')
  expect(status(first.main).textContent, 'picking auto is clean').toBe('')
  expectInert(first.main, 'auto back')
  fireEvent.click(saveButton(first.main))
  expect(first.bff.to('PUT', PUT), 'a clean Save sends nothing').toHaveLength(0)
  cleanup()
  const second = await form(without('reasoning-format'))
  const format = selectOf(second.main, FORMAT)
  expect(format.value, 'an absent key shows auto').toBe('auto')
  pick(second.main, FORMAT, 'auto')
  expect(status(second.main).textContent, 'picking auto is clean').toBe('')
  fireEvent.click(saveButton(second.main))
  expect(second.bff.to('PUT', PUT), 'a clean Save sends nothing').toHaveLength(
    0,
  )
})

test('2h.171, 2h.133: reasoning-effort default sends null and another option sends its value', async () => {
  const first = await form()
  expect(selectOf(first.main, EFFORT).value, 'the file value shows').toBe('low')
  pick(first.main, EFFORT, 'default')
  await save(first.main, first.bff)
  expect(sentValues(first.bff)['reasoning-effort'], 'default removes it').toBe(
    null,
  )
  cleanup()
  const second = await form(without('reasoning-effort'))
  expect(
    selectOf(second.main, EFFORT).value,
    'an absent key shows default',
  ).toBe('default')
  pick(second.main, EFFORT, 'xhigh')
  await save(second.main, second.bff)
  expect(sentValues(second.bff)['reasoning-effort'], 'the option value').toBe(
    'xhigh',
  )
})

// ── 171, 133: reasoning-preserve as the tri-state ───────────────────────

/** The tri-state row of 133 read through 132: picking the option the select
 * already shows is no change, so each row loads the other state first. */
const TRI_STATE = [
  { load: 'false', to: 'true', sends: 'true' },
  { load: 'true', to: 'false', sends: 'false' },
  { load: 'true', to: 'default', sends: null },
]

test.each(TRI_STATE)(
  '2h.171, 2h.133: reasoning-preserve loaded $load picks $to and sends $sends (the tri-state row)',
  async (c) => {
    const { bff, main } = await form({
      ...AS_LOADED,
      'reasoning-preserve': c.load,
    })
    pick(main, PRESERVE, c.to)
    await save(main, bff)
    expect(sentValues(bff)['reasoning-preserve'], `the tri-state sends`).toBe(
      c.sends,
    )
  },
)

// ── 171, 133: the tri-state reading, at a card editable before 2h.6 ─────

test('2h.133, 2h.171: the tri-state reading of a card that was editable before this one, flash-attn: a file default is the default option and clean, not the on', async () => {
  // 133's tri-state row: `default` is the row's absent option, so a file that
  // holds `default` is the absence, and the control shows `default` where a
  // `true` shows on. Pinned on the KV cache card's select, editable since
  // 2h.5's 164, so 2h.6 cannot move the reading of the card it opens.
  const { bff, main } = await form({ ...AS_LOADED, 'flash-attn': 'default' })
  const flash = selectOf(main, FLASH)
  expect(flash.value, 'a file default shows default, not on').toBe('default')
  expect(status(main).textContent, 'loaded as read, so the form is clean').toBe(
    '',
  )
  pick(main, FLASH, 'true')
  await save(main, bff)
  expect(sentValues(bff)['flash-attn'], 'on sends "true"').toBe('true')
})

// ── 172: the Budget message text ────────────────────────────────────────

/** Texts with a `;` or a `#`: the comment markers check refuses them (172). */
const BAD_MARKERS: string[] = ['Wrap up; now', 'Wrap up # now']

test.each(BAD_MARKERS)(
  '2h.172: Budget message %s is refused with field-error-comment-markers, marked, focused, save-invalid, nothing sent',
  async (text) => {
    const { bff, main } = await form()
    type(main, MSG, text)
    fireEvent.click(saveButton(main))
    await within(main).findByText(en('save-invalid'))
    const el = inputOf(main, MSG)
    expectError(el, MARKERS, `Budget message ${text}`)
    expect(document.activeElement, 'focus on Budget message').toBe(el)
    expectAlert(main, en('save-invalid'))
    expect(bff.to('PUT', PUT), 'nothing sent').toHaveLength(0)
  },
)

test('2h.133, 2h.172: a Budget message is sent trimmed of every white space at both ends, not reformatted', async () => {
  const first = await form()
  type(first.main, MSG, ' Wrap up later. ')
  await save(first.main, first.bff)
  expect(
    sentValues(first.bff)['reasoning-budget-message'],
    'plain spaces',
  ).toBe('Wrap up later.')
  // A U+3000 and a U+00A0 at the ends are white space for `trim` (133), which
  // is also what the bff refuses at an end, so they go out cut.
  cleanup()
  const second = await form()
  type(second.main, MSG, '\u3000Go now.\u00a0')
  await save(second.main, second.bff)
  expect(
    sentValues(second.bff)['reasoning-budget-message'],
    'the ends cut',
  ).toBe('Go now.')
})

test('2h.133, 2h.132: a Budget message the file holds sends null when emptied and is not checked, and spaces alone in one the file lacks never dirty the form', async () => {
  const first = await form()
  type(first.main, MSG, '')
  expectNoError(inputOf(first.main, MSG), 'the empty text is not checked')
  await save(first.main, first.bff)
  expectNoError(
    inputOf(first.main, MSG),
    'the empty text is not checked on Save',
  )
  expect(sentValues(first.bff), 'the key the file held is removed').toEqual({
    'reasoning-budget-message': null,
  })
  cleanup()
  const second = await form(without('reasoning-budget-message'))
  type(second.main, MSG, '   ')
  expect(status(second.main).textContent, 'spaces only: clean').toBe('')
  fireEvent.click(saveButton(second.main))
  expect(second.bff.to('PUT', PUT), 'the key is never sent').toHaveLength(0)
})

// ── The designer's decision for a text field with no default box ────────
// A plain labelled input: a `<label>` tied to the input, and no element groups
// the field under the label, which would make a screen reader say the name
// twice. The same pin for Chat kwargs is in PresetSave10.test.tsx.

test('2h (design): Budget message is a plain labelled input, not a group named by the label, and its empty box carries no example', async () => {
  const { main } = await form(without('reasoning-budget-message'))
  expect(
    within(main).queryByRole('group', { name: MSG }),
    'no element with role="group" named by the label',
  ).toBeNull()
  const el = inputOf(main, MSG)
  expect(el.disabled, 'the input enabled (170)').toBe(false)
  expect(el.getAttribute('spellcheck'), 'no spell check').toBe('false')
  expect(el.getAttribute('autocapitalize'), 'no capitalizing').toBe('off')
  expect(el.getAttribute('autocorrect'), 'no auto-correction').toBe('off')
  expect(el.getAttribute('autocomplete'), 'no autofill').toBe('off')
  // 172, keeping 2e's 87 as a preference: the GUI's example stays out.
  expect(el.getAttribute('placeholder') ?? '', 'no placeholder').toBe('')
})

// ── 173: the two integer fields ─────────────────────────────────────────

test('2h.173, 2h.158: reasoning-budget and n-predict take any whole number, a -1 included, and send it as typed, with no input mode', async () => {
  const first = await form()
  const budget = inputOf(first.main, BUDGET)
  expect(budget.getAttribute('inputmode'), 'no input mode (158)').toBeNull()
  expect(budget.id, 'the field id').toBe('preset-reasoning-budget-control')
  type(first.main, BUDGET, '-1')
  await save(first.main, first.bff)
  expect(
    sentValues(first.bff)['reasoning-budget'],
    '-1 goes out as typed',
  ).toBe('-1')
  cleanup()
  const second = await form()
  const predict = inputOf(second.main, MAXOUT)
  expect(predict.getAttribute('inputmode'), 'no input mode (158)').toBeNull()
  expect(predict.id, 'the field id').toBe('preset-n-predict-control')
  type(second.main, MAXOUT, '-1')
  await save(second.main, second.bff)
  expect(sentValues(second.bff)['n-predict'], '-1 goes out as typed').toBe('-1')
})

/** The card's two integer fields (173). */
const NUMBERS = [
  { key: 'reasoning-budget', label: BUDGET },
  { key: 'n-predict', label: MAXOUT },
]

test.each(NUMBERS)(
  '2h.173: $key rejects "1.5" with the whole-number error, marked, focused, save-invalid, nothing sent',
  async (c) => {
    const { bff, main } = await form()
    type(main, c.label, '1.5')
    fireEvent.click(saveButton(main))
    await within(main).findByText(en('save-invalid'))
    const el = inputOf(main, c.label)
    expectError(el, WHOLE, c.label)
    expect(document.activeElement, `focus on ${c.label}`).toBe(el)
    expectAlert(main, en('save-invalid'))
    expect(bff.to('PUT', PUT), 'nothing sent').toHaveLength(0)
  },
)

// ── 132, 133: the tri-state row read at every select that reads a flag ───
// The tri-state selects of 133's flag row that the form edits (kv-unified,
// backend-sampling, reasoning-preserve), read at the control as well as at the
// body: what the select SHOWS is the state the form is dirty against (132), so a
// pick of `default` from a loaded `true` has to show `default` while the save
// removes the key, and a file that holds the word `default` is that same absence
// (133's last paragraph: the row's absent option, so an untouched form sends
// nothing). The row's `off` sends the string `"false"` (`emit_bool`), which is
// NOT the key's removal, so it is pinned beside the `default` row. The row is
// read at an enabled control only: a disabled flag select shows the state the
// engine acts as (122, 125), so the word `default` there reads as the on it
// stands for, and no option can be picked from it. The two keys of the row that
// sit on the Hardware card (`op-offload`, `repack`) have no `editable` yet
// (2e.4), so they are read there as the read-only card of 122, whose states are
// pinned in PresetCards4.test.tsx (absent, true, false and an unlisted
// spelling, the word `default` not among them: observed on 2026-11-17, the
// disabled select reads the word `default` as the on it stands for, exactly as
// the paragraph above, and that read is left unpinned).

/** The tri-state selects of 133's flag row that the form edits, each with the
 * label of its field. */
const FLAGS = [
  { key: 'kv-unified', label: en('preset-unified-kv') },
  { key: 'backend-sampling', label: en('preset-backend-sampling') },
  { key: 'reasoning-preserve', label: PRESERVE },
]

/** The card's keys as loaded, with the flag `key` at `value`. `parallel` is
 * present so no number field's default box locks a select (155's
 * `needsUnticked`). */
function flags(key: string, value: string): Record<string, string> {
  return { ...AS_LOADED, parallel: '1', [key]: value }
}

/** The text of the option a select shows the user (132: what the user reads is
 * the selected option's text, 119). */
function shownOption(el: HTMLSelectElement): string {
  return el.options[el.selectedIndex]?.text ?? '(nothing selected)'
}

test.each(FLAGS)(
  '2h.132, 2h.133: $key loaded true picks default: the select SHOWS default and the save removes the key',
  async (c) => {
    const { bff, main } = await form(flags(c.key, 'true'))
    const el = selectOf(main, c.label)
    expect(el.disabled, `${c.key}: the select is enabled (130)`).toBe(false)
    expect(el.value, `${c.key}: a loaded true shows on`).toBe('true')
    pick(main, c.label, 'default')
    expect(el.value, `${c.key}: picking default shows default (132)`).toBe(
      'default',
    )
    expect(shownOption(el), `${c.key}: the option the user reads`).toBe(
      en('option-default'),
    )
    expect(
      status(main).textContent,
      `${c.key}: default from a loaded true is a change (132)`,
    ).toBe(en('save-unsaved'))
    await save(main, bff)
    expect(sentValues(bff)[c.key], `${c.key}: default removes the key`).toBe(
      null,
    )
  },
)

test.each(FLAGS)(
  '2h.133: $key loaded false picks on: shows on, sends "true"; loaded true picks off: shows off, sends "false" (emit_bool, not the removal)',
  async (c) => {
    const first = await form(flags(c.key, 'false'))
    const on = selectOf(first.main, c.label)
    expect(on.disabled, `${c.key}: the select is enabled (130)`).toBe(false)
    pick(first.main, c.label, 'true')
    expect(on.value, `${c.key}: picking on shows on`).toBe('true')
    expect(shownOption(on), `${c.key}: the option the user reads`).toBe(
      en('option-on'),
    )
    await save(first.main, first.bff)
    expect(sentValues(first.bff)[c.key], `${c.key}: on sends "true"`).toBe(
      'true',
    )
    // The row the reviewer suspects least: `off` is a value the configurator
    // writes (`emit_bool`), so it is sent as the string, never as null.
    cleanup()
    const second = await form(flags(c.key, 'true'))
    const off = selectOf(second.main, c.label)
    pick(second.main, c.label, 'false')
    expect(off.value, `${c.key}: picking off shows off`).toBe('false')
    expect(shownOption(off), `${c.key}: the option the user reads`).toBe(
      en('option-off'),
    )
    await save(second.main, second.bff)
    expect(
      sentValues(second.bff)[c.key],
      `${c.key}: off sends "false", not the key's removal`,
    ).toBe('false')
  },
)

test.each(FLAGS)(
  '2h.133, 2h.132: $key with the file holding the word default: the select shows default (not on), the form is clean, an untouched Save sends nothing',
  async (c) => {
    const { bff, main } = await form(flags(c.key, 'default'))
    const el = selectOf(main, c.label)
    expect(
      el.value,
      `${c.key}: the word default is the row absent option, not on`,
    ).toBe('default')
    expect(shownOption(el), `${c.key}: the option the user reads`).toBe(
      en('option-default'),
    )
    expect(
      status(main).textContent,
      `${c.key}: loaded as read, so the form is clean`,
    ).toBe('')
    expectInert(main, `${c.key} untouched`)
    fireEvent.click(saveButton(main))
    expect(
      bff.to('PUT', PUT),
      `${c.key}: an untouched form sends nothing`,
    ).toHaveLength(0)
  },
)
