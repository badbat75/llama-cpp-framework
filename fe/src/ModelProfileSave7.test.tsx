// The second half of sub-slice 2h.4 of fe/briefs/02h-model-profile-save.md: the input
// mode of a number field (158: `numeric` only where the range starts at 0 or
// above), the unit after an input as the input's description (159: id
// `<field id>-unit`, first in `aria-describedby`, the error's id after it), the
// focus after a 422 names a field whose default box is ticked (161, the rule of
// 144), and the string `field-error-whole-number` (163). The card's controls,
// its default box and its integer check are 155 to 157, in
// ModelProfileSave6.test.tsx, whose helpers this file repeats (a test file imports no
// other test file); the 422 answers are 142 to 144, in ModelProfileSave3.test.tsx.
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
import { t } from './strings.ts'
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
const SERVER = '#/instances/default'
const PUT = `/api/profiles/model/${ID}`
const CTX = en('model-profile-context-size')
const PAR = en('model-profile-parallel-slots')
const SLOT = en('model-profile-per-slot-context')
const BATCH = en('model-profile-batch-size')
const RAM = en('model-profile-cache-ram')
const MOE = en('model-profile-moe-cache')
const FITT = en('model-profile-fit-free-vram')
const DEFAULT = en('field-default')
/** The literal unit the tables give (criterion 89), not a strings key. */
const MIB = 'MiB'

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

// ── Helpers (as ModelProfileSave6.test.tsx) ───────────────────────────────────

/** Waits for the page read as `AS_LOADED` with revision `r1`, plus `options`. */
async function form(options: BffOptions = {}) {
  const bff = fakeBff({
    modelProfile: () =>
      reply(
        200,
        modelProfileDoc(
          {},
          {
            revision: 'r1',
            values: {
              'ctx-size': '172032',
              parallel: '4',
              'kv-unified': 'true',
              'ubatch-size': '2048',
            },
          },
        ),
      ),
    ...options,
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

/** A number field's input by the key it reads: the field id is `model-profile-<key>`,
 * so its control is `model-profile-<key>-control` (a row of a group, as the
 * Speculative card's, shares its label with a sibling). */
function boxOfKey(key: string): HTMLInputElement {
  const el = document.getElementById(`model-profile-${key}-control`)
  expect(el, `#${key}-control exists`).not.toBe(null)
  return el as HTMLInputElement
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

function saveButton(main: HTMLElement): HTMLElement {
  return within(main).getByRole('button', { name: en('save-button') })
}

function discardButton(main: HTMLElement): HTMLElement {
  return within(main).getByRole('button', { name: en('save-discard') })
}

/** Clicks Save changes and waits for the PUT of this model profile. */
async function save(main: HTMLElement, bff: ReturnType<typeof fakeBff>) {
  fireEvent.click(saveButton(main))
  await waitFor(() => expect(bff.to('PUT', PUT)).toHaveLength(1))
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

/** The id the unit of a field must take (159): `<field id>-unit`. */
function unitId(el: HTMLElement): string {
  return `${el.id.replace(/-control$/, '')}-unit`
}

/** Asserts `el` is marked invalid with the error `text` under it (142, 157). */
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

/** Asserts the input of `key` shows the unit and is described by it (159). */
function expectUnit(el: HTMLInputElement, what: string) {
  const uid = unitId(el)
  const unit = document.getElementById(uid)
  expect(unit, `${what}: the unit has the id ${uid}`).not.toBe(null)
  expect(unit!.textContent, `${what}: the unit's text`).toBe(MIB)
  expect(
    unit!.getAttribute('aria-hidden'),
    `${what}: the unit is visible text`,
  ).not.toBe('true')
  expect(isHidden(unit!), `${what}: the unit is visible text`).toBe(false)
  expect(before(el, unit!), `${what}: the unit sits after the input`).toBe(true)
  expect(describedIds(el)[0], `${what}: the unit is the description`).toBe(uid)
}

// ── 158: the input mode of a number field ───────────────────────────────

/** The five number fields of the Resource / context card (155): all of them
 * have a range starting at 0 or above, so all keep `inputmode="numeric"`. */
const CARD = [
  { key: 'ctx-size', label: CTX },
  { key: 'parallel', label: PAR },
  { key: 'kv-unified-per-slot', label: SLOT },
  { key: 'batch-size', label: BATCH },
  { key: 'ubatch-size', label: en('model-profile-u-batch-size') },
]

test.each(CARD)(
  '2h.158: the card field $key, whose range starts at 0 or above, has inputmode="numeric"',
  async (n) => {
    const { main } = await form()
    const el = box(main, n.label)
    expect(el.id, `${n.key}: the field id`).toBe(
      `model-profile-${n.key}-control`,
    )
    expect(el.getAttribute('inputmode'), `${n.key}: input mode`).toBe('numeric')
  },
)

/** Read-only fields of the other cards whose range starts at 0 or above. */
const MINNED = [
  'moe-cache-mib',
  'cache-reuse',
  'fit-ctx',
  'ctx-checkpoints',
  'image-min-tokens',
  'spec-draft-n-max',
]

test('2h.158: a read-only number field whose range starts at 0 or above keeps inputmode="numeric" too', async () => {
  await form()
  for (const key of MINNED) {
    const el = boxOfKey(key)
    expect(el.getAttribute('inputmode'), `${key}: input mode`).toBe('numeric')
  }
})

test('2h.158, 19 G.14: MoE CPU layers, editable with the range 0 to 4096, takes inputmode="numeric"', async () => {
  await form()
  const el = boxOfKey('n-cpu-moe')
  expect(el.getAttribute('inputmode'), `${el.id}: input mode`).toBe('numeric')
})

/** The any-whole-number keys (157's first row) and the layer counts of its
 * last: a phone's numeric keyboard may have no minus key, so they get no
 * input mode. All are read-only rows today. Brief 19's 19 G.14 gave
 * `n-cpu-moe` the range 0 to 4096, so it is not one of them. */
const FREE = [
  'cache-ram',
  'top-k',
  'n-predict',
  'reasoning-budget',
  'n-gpu-layers',
  'n-cpu-ffn',
  'n-gpu-layers-draft',
]

test('2h.158: an any-whole-number key and the layer counts get no input mode', async () => {
  await form()
  for (const key of FREE) {
    const el = boxOfKey(key)
    expect(el.getAttribute('inputmode'), `${key}: no input mode`).toBeNull()
  }
})

/** The float fields (2e: none, kept by 158). */
const FLOATS = [
  'temp',
  'top-p',
  'min-p',
  'repeat-penalty',
  'presence-penalty',
  'spec-draft-p-min',
]

test('2h.158: a float field keeps no input mode', async () => {
  await form()
  for (const key of FLOATS) {
    const el = boxOfKey(key)
    expect(el.getAttribute('inputmode'), `${key}: no input mode`).toBeNull()
  }
})

// ── 159: the unit is the input's description ────────────────────────────

test('2h.159: the unit after an input takes the id <field id>-unit, stays visible text, and is the input first description', async () => {
  const { main } = await form()
  // A number row (Cache RAM, the brief's example) and a text row (Free VRAM
  // of Fit to VRAM), both on cards that are not editable yet.
  expectUnit(boxOfKey('cache-ram'), RAM)
  expectUnit(boxOfKey('moe-cache-mib'), MOE)
  expectUnit(boxOfKey('fit-target'), FITT)
  expectUnit(box(main, RAM), RAM)
})

test('2h.159: with an error on a field that has a unit, the input is described by the unit first and then the error', async () => {
  const { bff, main } = await form({
    modelProfileSave: () =>
      reply(422, {
        key: 'not-a-number',
        args: ['cache-ram', '-1'],
        field: 'cache-ram',
      }),
  })
  type(box(main, CTX), '65536')
  saveButton(main).focus()
  await save(main, bff)
  const text = en('api-not-a-number', 'cache-ram', '-1')
  await within(main).findByText(en('save-refused', text))
  const el = boxOfKey('cache-ram')
  expectError(el, text, RAM)
  expect(describedIds(el).slice(0, 2), 'unit first, then the error').toEqual([
    unitId(el),
    'model-profile-cache-ram-error',
  ])
  expect(
    described(el).slice(0, 2),
    'Cache RAM, edit text, MiB, then the error',
  ).toEqual([MIB, text])
})

test('2h.159: the Server page Memory margin is described by its unit, and by the error after it', async () => {
  startAt(SERVER)
  fakeBff()
  const margin = en('server-memory-margin')
  render(<App />)
  const main = screen.getByRole('main')
  await within(main).findByRole('heading', {
    level: 3,
    name: en('server-network'),
  })
  const el = box(main, margin)
  expectUnit(el, margin)
  // With an error: the unit stays first, the error's id follows it.
  type(el, '-1')
  fireEvent.click(saveButton(main))
  await within(main).findByText(en('save-invalid'))
  const text = en('field-error-number-min', '0')
  expectError(el, text, margin)
  expect(describedIds(el).slice(0, 2), 'unit first, then the error').toEqual([
    unitId(el),
    `${el.id.replace(/-control$/, '')}-error`,
  ])
})

// ── 161: the focus after a 422 on a ticked box's field ──────────────────

test('2h.161: a 422 naming a field whose default box is ticked marks the field and puts the focus on the box (144)', async () => {
  const { bff, main } = await form({
    modelProfileSave: () =>
      reply(422, {
        key: 'not-whole-number-in-range',
        args: ['ctx-size', '-1', '0', '2147483647'],
        field: 'ctx-size',
      }),
  })
  // The box ticked: the input is disabled, so the field's first enabled
  // control is the box (144).
  toggle(main, CTX)
  const el = box(main, CTX)
  expect(el.disabled, 'ticked: the input is disabled').toBe(true)
  saveButton(main).focus()
  await save(main, bff)
  const text = en(
    'api-not-whole-number-in-range',
    'ctx-size',
    '-1',
    '0',
    '2147483647',
  )
  await within(main).findByText(en('save-refused', text))
  expectError(el, text, 'Context size with its box ticked')
  expect(document.activeElement, 'focus on the default box').toBe(
    checkBox(main, CTX),
  )
  expectAlert(main, en('save-refused', text), 'the ticked field refused')
})

// ── 163: the string, and who can show it ────────────────────────────────

test('2h.163: field-error-whole-number has the brief text, and no field of this card shows it', async () => {
  const whole = en('field-error-whole-number')
  expect(whole, 'the test table').toBe(
    'Enter a whole number, without a decimal point.',
  )
  expect(t('field-error-whole-number'), 'the fe strings file').toBe(whole)
  // Every field of this card has a range with a minimum (157), so a bad value
  // says the minimum: the whole-number text is not shown by this card.
  const { bff, main } = await form()
  type(box(main, CTX), '1.5')
  type(box(main, PAR), '1.5')
  fireEvent.click(saveButton(main))
  await within(main).findByText(en('save-invalid'))
  expectError(box(main, CTX), en('field-error-number-min', '0'), CTX)
  expectError(box(main, PAR), en('field-error-number-min', '1'), PAR)
  expect(
    within(main).queryByText(whole),
    'the whole-number text is not shown',
  ).toBeNull()
  expect(bff.to('PUT', PUT), 'nothing sent').toHaveLength(0)
})
