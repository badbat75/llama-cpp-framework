// Brief 21, sub-slice TP2 (fe/briefs/21-tensor-placement.md), criteria 21 T.10 to
// 21 T.13: the Rules as text field, the page's own checks on every rule at Save,
// the manager's refusals on `override-tensor`, and the keyboard, the long value
// and the strings of the text field. The card, its rows and their controls are
// TP1 (TensorPlacement1.test.tsx); the helpers are repeated here (a test file
// imports no other test file), with the refusal readers of
// ModelProfileSave3.test.tsx. The 320 px and 200 % half of 21 T.13 is the
// designer's measurement: jsdom lays nothing out, so the sheet rules are read as
// text (the way `Reflow.test.ts` does, a small local copy of the reader) and the
// long value is measured in `fe/ux` (the state `model-profile-placement`, section
// 7, whose stub value is the brief's three rules: shorter than 300 characters, so
// the 300-character page at 320 px is stated unproven here).
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from '@testing-library/react'
import { afterEach, beforeEach, expect, it, test } from 'vitest'
import App from './App.tsx'
import strings from './strings/en_US.json'
import {
  before,
  DEVICES,
  en,
  fakeBff,
  isHidden,
  MODEL_FILE,
  modelProfileDoc,
  reply,
  startAt,
  type BffOptions,
} from './test/bff.ts'

const ID = 'Qwen3-27B'
const HASH = `#/profiles/model/${ID}`
const PUT = `/api/profiles/model/${ID}`
const CARD = en('model-profile-tensor-placement')
const RULES = en('model-profile-ot-rules')
const TENSOR = en('model-profile-ot-tensor')
const LAYERS = en('model-profile-ot-layers')
const GOES = en('model-profile-ot-goes-on')
/** 21 T.20 (TP1b): the key carries the rule's legend, so the Remove name is
 *  built from it ("Remove Rule 2"). */
const removeName = (n: number) =>
  en('model-profile-ot-remove', en('model-profile-ot-rule', `${n}`))
const PATTERN = en('model-profile-ot-pattern')
const ADD = en('model-profile-ot-add')
const TEXT = en('model-profile-ot-text')
const TEXT_HINT = en('model-profile-ot-text-hint')
/** The About toggle of the Rules as text field (Field.tsx: its name carries the
 *  label). */
const ABOUT = `${en('field-about')} ${TEXT}`
/** 22 D4.31 (the designer's ruling of 2026-11-27): the card's own About toggle,
 *  a Tab stop right after the card's h3, leads the card's tab path. */
const CARD_ABOUT = `${en('field-about')} ${CARD}`
/** 21 T.11: the bar's alert when the page's own check refuses the save. */
const INVALID = en('save-invalid')
/** The key this card owns. */
const OT = 'override-tensor'
/** The two canned patterns the tests write into the file. */
const EMBD = String.raw`^token_embd\.weight`
const EXPS = String.raw`\.ffn_(up|down|gate|gate_up)_(ch|)exps`
/** A hand-written pattern no kind owns (21 T.4: it selects Custom, so its row
 *  shows a Pattern input). */
const HAND = String.raw`blk\.1[0-9]\..*_exps`
/** 21 T.13: a pattern of 300 characters with no break point. */
const LONG = `${String.raw`blk\.`}${'x'.repeat(300)}`
/** The three rules of the refusal tests, sent as rows 1, 2 and 3 (21 T.12: the
 *  rules sent are the rows, so the manager's 1-based positions agree with them).
 *  Row 1 is Custom (its Pattern input shows), rows 2 and 3 are kinds (none). */
const R1 = `${HAND}=CUDA0`
const R2 = `${EMBD}=ROCm0`
const R3 = `${EXPS}=CPU`

/** What the read gives: a model file, and nothing for `override-tensor` (the key
 *  absent, which is no rule). */
const AS_LOADED: Record<string, string> = {
  model: MODEL_FILE,
  'n-gpu-layers': '29',
}

/** `AS_LOADED` with these rules. */
function file(...rules: string[]): Record<string, string> {
  return { ...AS_LOADED, [OT]: rules.join(',') }
}

/** The model file's facts: the GPU card's line, the landing marker of `form`. */
function facts() {
  return {
    model: MODEL_FILE,
    info: { blockCount: 33, moe: true, expertCount: 128, expertUsed: 8 },
    reason: null,
  }
}

beforeEach(() => {
  startAt(HASH)
  // jsdom has no scrollIntoView; the jump button of 21 T.2 calls it on the h3.
  Element.prototype.scrollIntoView = () => {}
})

afterEach(() => {
  cleanup()
  startAt('')
})

// ── Helpers ───────────────────────────────────────────────────────────────

/** Waits for the page read with the file `values`, the facts `facts()` and the
 *  device probe `DEVICES`, unless `options` says otherwise. A save answers 200
 *  and applies the sent values to the next read, as the bff does. */
async function form(
  values: Record<string, string> = { ...AS_LOADED },
  options: BffOptions = {},
) {
  const loaded = { ...values }
  const bff = fakeBff({
    devices: DEVICES,
    modelInfo: facts(),
    modelProfile: () =>
      reply(200, modelProfileDoc({}, { values: { ...loaded } })),
    modelProfileSave: (body) => {
      const sent = (body as { values?: Record<string, unknown> })?.values ?? {}
      for (const [key, value] of Object.entries(sent)) {
        if (value === null || value === undefined) delete loaded[key]
        else loaded[key] = String(value)
      }
      return reply(200, { revision: 'r2', running: false })
    },
    ...options,
  })
  render(<App />)
  const main = screen.getByRole('main')
  await within(main).findByRole('heading', { level: 1, name: ID })
  await within(main).findByRole('heading', { level: 3, name: CARD })
  return { bff, main }
}

/** The moment the device read's as-of line shows (GD1.7), so a Goes on select
 *  read after it is the answered list and not the pending one. */
async function landed(main: HTMLElement) {
  const fs = await within(main).findByRole('group', {
    name: en('model-profile-gpus'),
  })
  const probedAt = String(DEVICES.probedAt)
  await within(fs).findByText(
    new Date(probedAt).toLocaleString(undefined, {
      dateStyle: 'medium',
      timeStyle: 'short',
    }),
  )
}

/** The Card whose h3 is `title` (fe/DESIGN.md 5.2: a `section.card`). */
function cardOf(main: HTMLElement, title: string): HTMLElement {
  const h3 = within(main).getByRole('heading', { level: 3, name: title })
  const card = h3.closest('section')
  expect(card, `${title}: the card element`).not.toBeNull()
  return card as HTMLElement
}

/** The accessible name of a control, as the page gives it. */
function nameOf(el: HTMLElement): string {
  const labelled = el.getAttribute('aria-labelledby')
  if (labelled)
    return labelled
      .split(/\s+/)
      .map((part) => document.getElementById(part)?.textContent ?? '')
      .join(' ')
      .replace(/\s+/g, ' ')
      .trim()
  const label = el.getAttribute('aria-label')
  if (label) return label.replace(/\s+/g, ' ').trim()
  const id = el.getAttribute('id')
  const tied = id ? document.querySelector(`label[for="${id}"]`) : null
  const wrap = el.closest('label')
  return (tied?.textContent ?? wrap?.textContent ?? el.textContent ?? '')
    .replace(/\s+/g, ' ')
    .trim()
}

/** Every tabbable element of `root`, in document order. */
function tabbable(root: HTMLElement): HTMLElement[] {
  return [
    ...root.querySelectorAll<HTMLElement>(
      'a[href], button, input, select, textarea, [tabindex]',
    ),
  ].filter(
    (el) =>
      !(el as HTMLButtonElement).disabled &&
      el.getAttribute('tabindex') !== '-1' &&
      !isHidden(el),
  )
}

/** Types into a control, as a keystroke does: one change event. */
function type(el: HTMLElement, value: string) {
  fireEvent.change(el, { target: { value } })
}

/** The ids named by `aria-describedby`, in order. */
function describedIds(el: HTMLElement): string[] {
  return (el.getAttribute('aria-describedby') ?? '')
    .split(/\s+/)
    .filter((id) => id !== '')
}

/** The texts of the elements `aria-describedby` names. */
function described(el: HTMLElement): string[] {
  return describedIds(el).map(
    (id) => document.getElementById(id)?.textContent ?? `(no #${id})`,
  )
}

function saveButton(main: HTMLElement): HTMLElement {
  return within(main).getByRole('button', { name: en('save-button') })
}

function discardButton(main: HTMLElement): HTMLElement {
  return within(main).getByRole('button', { name: en('save-discard') })
}

/** Clicks Save changes and waits for the PUT of this model profile (the save
 *  went out). */
async function save(main: HTMLElement, bff: ReturnType<typeof fakeBff>) {
  fireEvent.click(saveButton(main))
  await waitFor(() => expect(bff.to('PUT', PUT)).toHaveLength(1))
}

/** The save bar's status line (the GPUs card keeps its own status region, so the
 *  bar's line is picked inside the bar). */
function status(main: HTMLElement): HTMLElement {
  const all = within(main)
    .queryAllByRole('status')
    .filter((s) => s.closest('.save-bar') !== null)
  expect(all, 'the save bar has its status line').toHaveLength(1)
  return all[0]!
}

/** The bar's alert reads `text` (exact): a role="alert" of its own, after the
 *  status line and before the buttons (135, 01's 30a). */
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

/** The Field Error element the control names, by id. */
function errorOf(el: HTMLElement, text: string): HTMLElement | null {
  return (
    describedIds(el)
      .map((id) => document.getElementById(id))
      .find((e) => e !== null && e.textContent === text) ?? null
  )
}

/** Asserts `el` is marked invalid with the error `text` under it (142). */
function expectError(el: HTMLElement, text: string, what: string) {
  expect(el.getAttribute('aria-invalid'), `${what} aria-invalid`).toBe('true')
  expect(described(el), `${what} aria-describedby`).toContain(text)
  const error = errorOf(el, text)
  expect(error, `${what}: the error is shown`).toBeDefined()
  expect(isHidden(error!), `${what}: the error is shown`).toBe(false)
  expect(before(el, error!), `${what}: the error comes after the control`).toBe(
    true,
  )
  return error!
}

/** The field is not marked (142: the edit cleared it). */
function expectNoError(el: HTMLElement, what: string) {
  expect(el.getAttribute('aria-invalid'), `${what}: not invalid`).toBe(null)
}

/** The rule row of a control (the wrapping flex row of TP1, `.ot-rule-row`):
 *  21 T.11's Field Error sits in it, on a line of its own. */
function rowOf(control: HTMLElement, what: string): HTMLElement {
  const row = control.closest('[class*="ot-rule-row"]')
  expect(row, `${what}: its control is inside a rule row`).not.toBeNull()
  return row as HTMLElement
}

/** 21 T.11: the Field Error is the rule row's own item, under the control, and
 *  the control names it. */
function expectRowError(control: HTMLElement, text: string, what: string) {
  const error = expectError(control, text, what)
  expect(error.className, `${what}: the Field Error class`).toContain(
    'field-error',
  )
  const row = rowOf(control, what)
  expect(row.contains(error), `${what}: the error is in the rule row`).toBe(
    true,
  )
}

/** Clicks Save changes and waits for the bar's `save-invalid`: the page's own
 *  check refused the save, so nothing went out (21 T.11). */
async function saveInvalid(main: HTMLElement, bff: ReturnType<typeof fakeBff>) {
  fireEvent.click(saveButton(main))
  await within(main).findByText(INVALID)
  expect(bff.to('PUT', PUT), '21 T.11: nothing is sent').toHaveLength(0)
  expectAlert(main, INVALID, '21 T.11: the bar')
}

/** The card's Rules fieldset (21 T.2). */
function rulesFieldset(card: HTMLElement): HTMLElement {
  return within(card).getByRole('group', { name: RULES })
}

/** The rules' `li`, in document order. */
function ruleItems(card: HTMLElement): HTMLElement[] {
  const list = within(card).queryByRole('list')
  expect(list, 'the rules are a list').not.toBeNull()
  return within(list as HTMLElement).getAllByRole('listitem')
}

/** One rule's fieldset, named by its legend. */
function ruleGroup(card: HTMLElement, n: number): HTMLElement {
  return within(card).getByRole('group', {
    name: en('model-profile-ot-rule', `${n}`),
  })
}

function tensorOf(group: HTMLElement): HTMLSelectElement {
  return within(group).getByRole('combobox', {
    name: TENSOR,
  }) as HTMLSelectElement
}

function layersOf(group: HTMLElement): HTMLInputElement | null {
  return within(group).queryByRole('textbox', {
    name: LAYERS,
  }) as HTMLInputElement | null
}

function goesOnOf(group: HTMLElement): HTMLSelectElement {
  return within(group).getByRole('combobox', {
    name: GOES,
  }) as HTMLSelectElement
}

function patternOf(group: HTMLElement): HTMLInputElement | null {
  return within(group).queryByRole('textbox', {
    name: PATTERN,
  }) as HTMLInputElement | null
}

function addOf(card: HTMLElement): HTMLElement {
  return within(card).getByRole('button', { name: ADD })
}

/** 21 T.10: the Rules as text input and its Field. */
function textField(card: HTMLElement): HTMLInputElement {
  return within(card).getByRole('textbox', { name: TEXT }) as HTMLInputElement
}

function fieldOf(main: HTMLElement, label: string): HTMLElement {
  const toggle = within(main).getByRole('button', {
    name: `${en('field-about')} ${label}`,
  })
  const field = toggle.closest('.field')
  expect(field, `${label}: its Field wrapper`).toBeTruthy()
  return field as HTMLElement
}

/** Puts the page's save answer at 422 `body` and returns the page with the three
 *  refusal rules, made dirty by moving rule 3's device so the save sends
 *  `override-tensor` (the rows are the rules sent). */
async function refused(body: Record<string, unknown>) {
  const { bff, main } = await form(file(R1, R2, R3), {
    modelProfileSave: () => reply(422, body),
  })
  await landed(main)
  fireEvent.change(goesOnOf(ruleGroup(cardOf(main, CARD), 3)), {
    target: { value: 'Vulkan0' },
  })
  saveButton(main).focus()
  await save(main, bff)
  return { bff, main }
}

// ── The stylesheet reader (21 T.11: the Field Error of a rule) ────────────

const CSS = readFileSync(
  join(import.meta.dirname, 'index.css'),
  'utf8',
).replace(/\/\*[\s\S]*?\*\//g, '')

type SheetRule = { sel: string; body: string; depth: number }

// Rules opened before this offset: the nesting level of the rule that starts
// there, 0 at the top level.
const opened = (upto: number): number =>
  (CSS.slice(0, upto).match(/\{/g) ?? []).length -
  (CSS.slice(0, upto).match(/\}/g) ?? []).length

const SHEET_RULES: SheetRule[] = [...CSS.matchAll(/([^{}]+)\{([^{}]*)\}/g)].map(
  (m) => ({
    sel: m[1].trim(),
    body: m[2],
    depth: opened(m.index ?? 0),
  }),
)

// Top-level rules only: the rule applies at every width, not in a media block.
const TOP = SHEET_RULES.filter((r) => r.depth === 0)

// The value of a declaration; the last one when a block repeats the property.
const prop = (body: string, name: string): string => {
  let val = ''
  for (const d of body.split(';')) {
    const i = d.indexOf(':')
    if (i >= 0 && d.slice(0, i).trim() === name) val = d.slice(i + 1).trim()
  }
  return val
}

const norm = (v: string): string => v.replace(/\s+/g, ' ').trim()

// The top-level rule styling this selector, as one part of a comma group (the
// sheet's own style for a shared declaration), throwing when it is not exactly
// one.
const topStyling = (sel: string, what: string): SheetRule => {
  const found = TOP.filter((r) =>
    r.sel
      .split(',')
      .some((p) => p.replace(/\s+/g, '') === sel.replace(/\s+/g, '')),
  )
  if (found.length !== 1)
    throw new Error(
      `expected exactly one rule: ${what} ("${sel}"), found ${found.length}` +
        (found.length ? ` (${found.map((r) => norm(r.sel)).join(' | ')})` : ''),
    )
  return found[0]
}

// ── 21 T.10: Rules as text ────────────────────────────────────────────────

test('21 T.10: after the Rules fieldset, a Field labelled model-profile-ot-text with one monospace text input (spellcheck off, autocapitalize off) and no other control; its hint model-profile-ot-text-hint shows only under the About toggle', async () => {
  const { main } = await form(file(`${HAND}=CPU`))
  const card = cardOf(main, CARD)
  const fs = rulesFieldset(card)
  const input = textField(card)
  const field = fieldOf(card, TEXT)
  expect(field.contains(input), '21 T.10: the input is in its Field').toBe(true)
  expect(
    fs.contains(input),
    '21 T.10: the field is not inside the Rules fieldset',
  ).toBe(false)
  expect(
    before(fs, field),
    '21 T.10: the field is after the Rules fieldset',
  ).toBe(true)
  expect(input.tagName, '21 T.10: an input').toBe('INPUT')
  expect(input.type, '21 T.10: a text input').toBe('text')
  expect(input.className, '21 T.10: the text is monospace').toContain('mono')
  expect(input.getAttribute('spellcheck'), '21 T.10: no spellcheck').toBe(
    'false',
  )
  expect(
    input.getAttribute('autocapitalize'),
    '21 T.10: no autocapitalize',
  ).toBe('off')
  expect(
    within(field).getAllByRole('textbox'),
    '21 T.10: one text input, no other control of the cards kind',
  ).toHaveLength(1)
  // The hint, one paragraph per line of the key, behind the About toggle.
  const lines = TEXT_HINT.split('\n')
  expect(lines.length, 'the hint of section 2 is two lines').toBe(2)
  const hint = within(field).getByText(lines[0])
  expect(isHidden(hint), '21 T.10: the hint starts hidden').toBe(true)
  const toggle = within(card).getByRole('button', { name: ABOUT })
  expect(toggle.getAttribute('aria-expanded'), '21 T.10: closed').toBe('false')
  fireEvent.click(toggle)
  expect(isHidden(hint), '21 T.10: the hint shows under the toggle').toBe(false)
  expect(toggle.getAttribute('aria-expanded'), '21 T.10: open').toBe('true')
  expect(
    within(field).getByText(lines[1]),
    '21 T.10: the second line of the hint',
  ).toBeTruthy()
  fireEvent.click(toggle)
  expect(isHidden(hint), '21 T.10: the toggle closes it again').toBe(true)
})

test('21 T.10: not focused, the field shows the loaded value as written while the key is unchanged, and render of the rows once they change', async () => {
  // The loaded spelling has spaces `parse` trims away, so "as written" and
  // "render of the rows" are two different texts.
  const written = ` ${HAND} = CUDA0 `
  const { main } = await form(file(written))
  const card = cardOf(main, CARD)
  expect(
    textField(card).value,
    '21 T.10: the loaded value as the file writes it',
  ).toBe(written)
  // 21 T.7: Add seeds the first probed GPU, so the probe must have answered
  // (pending, the component seeds CPU, as the brief says).
  await landed(main)
  fireEvent.click(addOf(card))
  expect(
    textField(card).value,
    '21 T.10: the rows changed: the field shows render of them',
  ).toBe(`${HAND}=CUDA0,${EMBD}=CUDA0`)
})

test('21 T.10: typing changes only the field text: the rows keep what they show while it is typed in, and Escape does nothing special', async () => {
  const { bff, main } = await form(file(`${HAND}=CPU`, `${EMBD}=CUDA0`))
  const card = cardOf(main, CARD)
  const input = textField(card)
  expect(ruleItems(card).length, '21 T.10: two rules loaded').toBe(2)
  const typed = `${EXPS}=CPU,garbage,,`
  type(input, typed)
  expect(textField(card).value, '21 T.10: the text typed').toBe(typed)
  expect(ruleItems(card).length, '21 T.10: the rows did not change').toBe(2)
  expect(
    tensorOf(ruleGroup(card, 1)).value,
    '21 T.10: rule 1 is still the hand-written one',
  ).toBe('custom')
  expect(
    tensorOf(ruleGroup(card, 2)).value,
    '21 T.10: rule 2 is still the embedding',
  ).toBe('embd')
  fireEvent.keyDown(input, { key: 'Escape', code: 'Escape' })
  expect(textField(card).value, '21 T.10: Escape keeps the text').toBe(typed)
  expect(ruleItems(card).length, '21 T.10: Escape changes no rule').toBe(2)
  expect(
    within(main).queryByRole('alert'),
    '21 T.10: Escape says nothing',
  ).toBeNull()
  expect(bff.to('PUT', PUT), '21 T.10: typing saves nothing').toHaveLength(0)
})

test('21 T.10: Enter applies the text: the rows become parse(text) with new ids and the remembered texts dropped, and the field shows render of them; losing the focus applies it the same way; a text that parses to the rules on screen changes nothing', async () => {
  const { bff, main } = await form(file(`${HAND}=CPU`))
  const card = cardOf(main, CARD)
  const input = textField(card)
  const oldPattern = patternOf(ruleGroup(card, 1))!
  const oldId = oldPattern.id
  // A remembered Pattern text on the row, never applied.
  type(oldPattern, 'zzz')
  type(input, ` ${HAND} = CPU `)
  fireEvent.keyDown(input, { key: 'Enter', code: 'Enter' })
  await waitFor(() =>
    expect(
      textField(card).value,
      '21 T.10: Enter shows render of the rows',
    ).toBe(`${HAND}=CPU`),
  )
  const rebuilt = patternOf(ruleGroup(card, 1))!
  expect(
    rebuilt.value,
    '21 T.10: the remembered text is dropped with the row',
  ).toBe(HAND)
  expect(rebuilt.id, '21 T.10: the rows are rebuilt, with new ids').not.toBe(
    oldId,
  )
  // The focus leaving applies the text as Enter does.
  type(input, `${EMBD}=CUDA0`)
  fireEvent.blur(input)
  await waitFor(() =>
    expect(
      tensorOf(ruleGroup(cardOf(main, CARD), 1)).value,
      '21 T.10: the blur applied the text',
    ).toBe('embd'),
  )
  expect(textField(card).value, '21 T.10: and shows render of them').toBe(
    `${EMBD}=CUDA0`,
  )
  // A text that parses to the rules already on screen changes nothing: the
  // same rows (the same ids), and Save sends exactly what was on screen
  // before the apply: the blur's change as rendered (21 T.8), with nothing
  // the apply added.
  const shown = textField(card).value
  const sameId = tensorOf(ruleGroup(cardOf(main, CARD), 1)).id
  type(input, ` ${EMBD} = CUDA0 `)
  fireEvent.keyDown(input, { key: 'Enter', code: 'Enter' })
  await waitFor(() =>
    expect(textField(card).value, '21 T.10: the text is normalized').toBe(
      shown,
    ),
  )
  expect(ruleItems(cardOf(main, CARD)).length, '21 T.10: one rule').toBe(1)
  expect(
    tensorOf(ruleGroup(cardOf(main, CARD), 1)).id,
    '21 T.10: the rows on screen are the same rows',
  ).toBe(sameId)
  await save(main, bff)
  const calls = bff.to('PUT', PUT)
  expect(
    calls,
    '21 T.10: the apply of the same text adds no second send',
  ).toHaveLength(1)
  expect(
    (calls[0]!.body as { values?: Record<string, unknown> })?.values,
    '21 T.10: the PUT is the rules on screen as rendered, nothing added',
  ).toEqual({ [OT]: `${EMBD}=CUDA0` })
})

// ── 21 T.11: the page's own checks at Save ────────────────────────────────

test('21 T.11: a Layers text parseLayers cannot read is marked model-profile-ot-layers-bad (the text) on that input, focus goes to it, the bar says save-invalid and nothing is sent; editing the input clears the mark', async () => {
  const text = en('model-profile-ot-layers-bad', '10-')
  const { bff, main } = await form(file(`${EXPS}=CPU`))
  const card = cardOf(main, CARD)
  const layers = layersOf(ruleGroup(card, 1))!
  type(layers, '10-')
  await saveInvalid(main, bff)
  expectRowError(layers, text, '21 T.11: Layers')
  expect(document.activeElement, '21 T.11: focus on the Layers input').toBe(
    layers,
  )
  // The edit: the text reads, the mark goes, the bar keeps its alert.
  type(layers, '10-12')
  expectNoError(layers, '21 T.11: Layers after the edit')
  expect(
    within(main).queryByText(text),
    '21 T.11: the error is gone',
  ).toBeNull()
  expectAlert(main, INVALID, '21 T.11: the bar keeps its alert')
})

test('21 T.11: a Layers text typed readable and then unreadable is refused at Save too: the mark, the focus, save-invalid and nothing sent (21 TP2 review D1)', async () => {
  // The ruling of 2026-11-22 in 21 T.6, read with 21 T.11: the readable text
  // typed first composes the rule, and the unreadable text that follows keeps
  // it there. The page's check does not look at what was typed before: the
  // Layers text on screen does not read, so Save is refused.
  const text = en('model-profile-ot-layers-bad', '10-')
  const withLayers = `blk\\.(0|1|2)${EXPS}`
  const { bff, main } = await form(file(`${withLayers}=CPU`))
  const card = cardOf(main, CARD)
  const layers = layersOf(ruleGroup(card, 1))!
  expect(layers.value, 'the row loads with readable layers').toBe('0-2')
  type(layers, '10-12, 15')
  expect(
    textField(card).value,
    '21 T.10: the readable text composed the rule',
  ).toBe(`blk\\.(10|11|12|15)${EXPS}=CPU`)
  type(layers, '10-')
  await saveInvalid(main, bff)
  expectRowError(layers, text, '21 T.11: Layers after a readable text')
  expect(document.activeElement, '21 T.11: focus on the Layers input').toBe(
    layers,
  )
})

test('21 T.11: a Custom rule with an empty pattern is marked model-profile-ot-pattern-empty on its Pattern input, focus goes to it, the bar says save-invalid and nothing is sent', async () => {
  const text = en('model-profile-ot-pattern-empty')
  const { bff, main } = await form(file(`${EXPS}=CPU`))
  const card = cardOf(main, CARD)
  fireEvent.change(tensorOf(ruleGroup(card, 1)), {
    target: { value: 'custom' },
  })
  const pattern = patternOf(ruleGroup(card, 1))!
  // The row keeps its pattern text when the kind becomes Custom (21 T.4): the
  // empty rule is the one whose text was cleared.
  type(pattern, '')
  expect(pattern.value, '21 T.11: a Custom rule with no pattern').toBe('')
  await saveInvalid(main, bff)
  expectRowError(pattern, text, '21 T.11: Pattern')
  expect(document.activeElement, '21 T.11: focus on Pattern').toBe(pattern)
})

test('21 T.11: a Custom pattern holding a comma, and one holding an equals sign, is marked model-profile-ot-pattern-separator on the Pattern input, focus goes to it, the bar says save-invalid and nothing is sent', async () => {
  const text = en('model-profile-ot-pattern-separator')
  for (const pattern of ['ffn,exps', 'ffn=exps']) {
    // One page per case: the loop's second render needs the first one gone.
    cleanup()
    const { bff, main } = await form(file(`${HAND}=CPU`))
    const card = cardOf(main, CARD)
    const input = patternOf(ruleGroup(card, 1))!
    type(input, pattern)
    await saveInvalid(main, bff)
    expectRowError(input, text, `21 T.11: the pattern ${pattern}`)
    expect(document.activeElement, `21 T.11: focus on ${pattern}`).toBe(input)
  }
})

test('21 T.11: a rule on no device is marked model-profile-ot-device-missing on its Goes on select, focus goes to it, the bar says save-invalid and nothing is sent; picking a device clears the mark', async () => {
  const text = en('model-profile-ot-device-missing')
  // The piece has no `=`, so the rule has no device (21 T.5: its Goes on leads
  // with the empty option).
  const { bff, main } = await form(file(`${HAND}`))
  const card = cardOf(main, CARD)
  const goes = goesOnOf(ruleGroup(card, 1))
  expect(goes.value, '21 T.11: the rule is on no device').toBe('')
  await saveInvalid(main, bff)
  expectRowError(goes, text, '21 T.11: Goes on')
  expect(document.activeElement, '21 T.11: focus on Goes on').toBe(goes)
  fireEvent.change(goes, { target: { value: 'CPU' } })
  expectNoError(goes, '21 T.11: Goes on after the pick')
  expect(
    within(main).queryByText(text),
    '21 T.11: the error is gone',
  ).toBeNull()
})

test('21 T.11: with two rules failing, focus goes to the first failing control in DOM order and both are marked with their own Field Error in their row', async () => {
  const layersText = en('model-profile-ot-layers-bad', '1000')
  const deviceText = en('model-profile-ot-device-missing')
  const { bff, main } = await form(file(`${HAND}`, `${EXPS}=CPU`))
  const card = cardOf(main, CARD)
  const goes = goesOnOf(ruleGroup(card, 1))
  const layers = layersOf(ruleGroup(card, 2))!
  type(layers, '1000')
  await saveInvalid(main, bff)
  expectRowError(goes, deviceText, '21 T.11: rule 1 Goes on')
  expectRowError(layers, layersText, '21 T.11: rule 2 Layers')
  expect(
    document.activeElement,
    '21 T.11: focus on the first failing control',
  ).toBe(goes)
})

test('21 T.11: editing a control clears only that control error: a refusal mark on Goes on survives a change of the same rule Tensor (21 TP2 review D3)', async () => {
  const args = [ID, 'default', 'override-tensor', 'CUDA0']
  const text = en('api-model-profile-device-outside', ...args)
  const { main } = await refused({ key: 'model-profile-device-outside', args })
  await within(main).findByText(en('save-refused', text))
  const goes = goesOnOf(ruleGroup(cardOf(main, CARD), 1))
  expectRowError(goes, text, '21 T.11: the refusal is on rule 1 Goes on')
  // The user edits another control of the same row: the rule's Tensor kind.
  // The mark the manager put on Goes on is about the device, which did not
  // move, so it stays.
  fireEvent.change(tensorOf(ruleGroup(cardOf(main, CARD), 1)), {
    target: { value: 'exps' },
  })
  expectRowError(
    goesOnOf(ruleGroup(cardOf(main, CARD), 1)),
    text,
    '21 T.11: Goes on keeps its mark after the Tensor edit',
  )
  expectNoError(
    tensorOf(ruleGroup(cardOf(main, CARD), 1)),
    '21 T.11: the edited Tensor is not marked',
  )
})

test('21 T.11: editing a control clears only that control error: the Layers mark of the page check survives a change of the same rule Goes on (21 TP2 review D3)', async () => {
  const text = en('model-profile-ot-layers-bad', '10-')
  const { bff, main } = await form(file(`${EXPS}=CPU`))
  const card = cardOf(main, CARD)
  const layers = layersOf(ruleGroup(card, 1))!
  type(layers, '10-')
  await saveInvalid(main, bff)
  expectRowError(layers, text, '21 T.11: the check is on Layers')
  // The device of the same rule changes: the Layers text on screen is the
  // same unreadable text, so its mark stays.
  fireEvent.change(goesOnOf(ruleGroup(cardOf(main, CARD), 1)), {
    target: { value: 'ROCm0' },
  })
  expectRowError(
    layersOf(ruleGroup(cardOf(main, CARD), 1))!,
    text,
    '21 T.11: Layers keeps its mark after the Goes on edit',
  )
  expectNoError(
    goesOnOf(ruleGroup(cardOf(main, CARD), 1)),
    '21 T.11: the edited Goes on is not marked',
  )
})

test('21 T.11: the sheet gives the Field Error of a rule a line of its own in the row: flex-basis: 100%', () => {
  const rule = topStyling(
    '.ot-rule-row .field-error',
    'the Field Error under a rule',
  )
  expect(
    norm(prop(rule.body, 'flex-basis')),
    '21 T.11: the error is a full-width item of the flex row',
  ).toBe('100%')
})

// ── 21 T.12: the manager's refusals ───────────────────────────────────────

/** 21 T.12: one row per key: the 422 body, the control the refusal goes on, and
 *  the rule's 1-based position. */
const TABLE_TP2: [
  string,
  Record<string, unknown>,
  'pattern' | 'tensor' | 'goes-on',
  number,
][] = [
  [
    'override-no-pattern on a Custom rule (its Pattern input shows)',
    { key: 'override-no-pattern', args: ['1'], field: 'override-tensor' },
    'pattern',
    1,
  ],
  [
    'override-no-device on a kind rule (no Pattern)',
    { key: 'override-no-device', args: ['2', EMBD], field: 'override-tensor' },
    'tensor',
    2,
  ],
  [
    'override-tensor-space on a kind rule (no Pattern)',
    { key: 'override-tensor-space', args: ['3'], field: 'override-tensor' },
    'tensor',
    3,
  ],
  [
    'override-bad-device',
    {
      key: 'override-bad-device',
      args: ['2', 'Rocm0'],
      field: 'override-tensor',
    },
    'goes-on',
    2,
  ],
]

it.each(TABLE_TP2)(
  '21 T.12: %s: the 422 marks that control with api-key filled from args, aria-invalid and the error under it, focus on it and the bar save-refused; a reply with no field member answers the same through the fallback',
  async (what, body, kind, n) => {
    for (const withField of [true, false]) {
      cleanup()
      const sent = withField
        ? body
        : { key: body.key, args: body.args as string[] }
      const { main } = await refused(sent)
      const text = en(`api-${String(body.key)}`, ...(body.args as string[]))
      await within(main).findByText(en('save-refused', text))
      const group = ruleGroup(cardOf(main, CARD), n)
      const control =
        kind === 'pattern'
          ? patternOf(group)
          : kind === 'tensor'
            ? tensorOf(group)
            : goesOnOf(group)
      expect(control, `${what}: the control is shown`).not.toBeNull()
      expectError(control!, text, what)
      expect(document.activeElement, `${what}: focus on it`).toBe(control)
      expectAlert(main, en('save-refused', text), what)
    }
  },
)

test('21 T.12: a refusal naming a position with no row answers on Rules as text: marked, focused, with the bar save-refused', async () => {
  for (const body of [
    { key: 'override-no-pattern', args: ['9'], field: 'override-tensor' },
    // No field member: the fallback table answers `override-tensor` for the key.
    { key: 'override-tensor-space', args: ['9'] },
  ]) {
    cleanup()
    const { main } = await refused(body)
    const text = en(`api-${String(body.key)}`, ...(body.args as string[]))
    await within(main).findByText(en('save-refused', text))
    const input = textField(cardOf(main, CARD))
    expectError(input, text, `21 T.12: ${String(body.key)} with no row`)
    expect(document.activeElement, '21 T.12: focus on Rules as text').toBe(
      input,
    )
    expectAlert(main, en('save-refused', text), '21 T.12: the bar')
  }
})

test('21 T.12: model-profile-device-outside with args[2] override-tensor marks Goes on of every rule whose device equals args[3] whatever the case, focus on the first', async () => {
  // The manager compares the device case-insensitively, so a rule spelled
  // `cuda0` answers to `CUDA0` (decision 4: the device matched exactly is the
  // option, the comparison here is the manager's).
  const args = [ID, 'default', 'override-tensor', 'CUDA0']
  const text = en('api-model-profile-device-outside', ...args)
  const { bff, main } = await form(
    file(`${HAND}=CUDA0`, `${EMBD}=cuda0`, `${EXPS}=CPU`),
    {
      modelProfileSave: () =>
        reply(422, { key: 'model-profile-device-outside', args }),
    },
  )
  await landed(main)
  fireEvent.change(goesOnOf(ruleGroup(cardOf(main, CARD), 3)), {
    target: { value: 'Vulkan0' },
  })
  const first = goesOnOf(ruleGroup(cardOf(main, CARD), 1))
  const second = goesOnOf(ruleGroup(cardOf(main, CARD), 2))
  const third = goesOnOf(ruleGroup(cardOf(main, CARD), 3))
  saveButton(main).focus()
  await save(main, bff)
  await within(main).findByText(en('save-refused', text))
  expectRowError(first, text, '21 T.12: rule 1 Goes on (CUDA0)')
  expectRowError(second, text, '21 T.12: rule 2 Goes on (cuda0)')
  expectNoError(third, '21 T.12: rule 3 Goes on (CPU, not named)')
  expect(document.activeElement, '21 T.12: focus on the first').toBe(first)
  expectAlert(main, en('save-refused', text), '21 T.12: the bar')
})

test('21 T.12: model-profile-device-outside naming a device no rule is on answers on Rules as text', async () => {
  const args = [ID, 'default', 'override-tensor', 'Vulkan9']
  const text = en('api-model-profile-device-outside', ...args)
  const { main } = await refused({ key: 'model-profile-device-outside', args })
  await within(main).findByText(en('save-refused', text))
  const input = textField(cardOf(main, CARD))
  expectError(input, text, '21 T.12: no rule on that device')
  expect(document.activeElement, '21 T.12: focus on Rules as text').toBe(input)
  for (const n of [1, 2, 3])
    expectNoError(
      goesOnOf(ruleGroup(cardOf(main, CARD), n)),
      `21 T.12: rule ${n} Goes on`,
    )
})

test('21 T.12: any other key naming override-tensor, comment-markers, answers on Rules as text (02h 143 for this key)', async () => {
  const text = en('api-comment-markers', 'override-tensor')
  const { main } = await refused({
    key: 'comment-markers',
    args: ['override-tensor'],
    field: 'override-tensor',
  })
  await within(main).findByText(en('save-refused', text))
  const input = textField(cardOf(main, CARD))
  expectError(input, text, '21 T.12: comment-markers')
  expect(document.activeElement, '21 T.12: focus on Rules as text').toBe(input)
  expectAlert(main, en('save-refused', text), '21 T.12: the bar')
})

// ── 21 T.13: the keyboard, the long value, the strings ────────────────────

test('21 T.13 (as 22 D4.31 amends it): the card opens with its own About, and after the last rule, Tab goes Add rule, the Rules as text About toggle, its input', async () => {
  const { main } = await form(file(`${EXPS}=CPU`, `${HAND}=CPU`))
  await landed(main)
  expect(
    tabbable(cardOf(main, CARD)).map(nameOf),
    '21 T.13: the tab path of the card',
  ).toEqual([
    CARD_ABOUT,
    TENSOR,
    LAYERS,
    GOES,
    removeName(1),
    TENSOR,
    GOES,
    removeName(2),
    PATTERN,
    ADD,
    ABOUT,
    TEXT,
  ])
})

test('21 T.13: a value of over 300 characters stands whole in Rules as text, in a single-line text input that scrolls inside itself (no textarea)', async () => {
  const value = `${LONG}=CPU`
  expect(
    value.length,
    'the fixture is a value of over 300 characters (21 T.13)',
  ).toBeGreaterThan(300)
  const { main } = await form(file(value))
  const card = cardOf(main, CARD)
  const input = textField(card)
  expect(input.value, '21 T.13: the whole value is in the field').toBe(value)
  expect(input.tagName, '21 T.13: an input, not a textarea').toBe('INPUT')
  expect(input.type, '21 T.13: single line: the value scrolls inside it').toBe(
    'text',
  )
  expect(input.getAttribute('rows'), '21 T.13: no rows attribute').toBeNull()
  expect(
    main.querySelectorAll('textarea').length,
    '21 T.13: no textarea anywhere on the page',
  ).toBe(0)
})

test('21 T.13: the strings file gains exactly TP2s keys, each with the English of section 2', () => {
  const table: Record<string, string> = strings
  const KEYS = [
    'model-profile-ot-text',
    'model-profile-ot-text-hint',
    'model-profile-ot-layers-bad',
    'model-profile-ot-pattern-empty',
    'model-profile-ot-pattern-separator',
    'model-profile-ot-device-missing',
    'api-override-tensor-space',
    'api-override-bad-device',
  ]
  expect(
    KEYS.filter((key) => table[key] === undefined),
    '21 T.13: keys missing from strings/en_US.json',
  ).toEqual([])
  expect(
    KEYS.filter((key) => table[key] !== en(key)),
    '21 T.13: keys whose en_US.json text is not the English of the brief',
  ).toEqual([])
})
