// Part of sub-slice 2h.4 of fe/briefs/02h-model-profile-save.md, as brief 22
// IP5 narrows it: the input mode of a number field (158: `numeric` only where
// the range starts at 0 or above), the unit after an input as the input's
// description (159: id `<field id>-unit`, first in `aria-describedby`, the
// error's id after it), and the string `field-error-whole-number` (163). The
// Resource / context card this file read at first is gone (its five keys are
// `PLACEMENT_KEYS`), so the rules are read at the number fields that stay:
// cache-reuse, the Vision pair, the Speculative groups. The focus after a 422
// names a field whose default box is ticked (161) is pinned by
// ModelProfileSave3.test.tsx, whose helpers this file repeats (a test file
// imports no other test file).
import {
  cleanup,
  fireEvent,
  render,
  screen,
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
const SERVER = '#/profiles/server/default'
const PUT = `/api/profiles/model/${ID}`
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
              'cache-reuse': '256',
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

function type(input: HTMLElement, value: string) {
  fireEvent.change(input, { target: { value } })
}

function saveButton(main: HTMLElement): HTMLElement {
  return within(main).getByRole('button', { name: en('save-button') })
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

/** The editable integer fields that IP5 leaves on the page, whose range starts
 * at 0 or above (158, read from `INT_KEYS`): cache-reuse, the Vision pair, and
 * the number rows of the Speculative decoding card's groups. */
const MINNED = [
  'cache-reuse',
  'image-min-tokens',
  'image-max-tokens',
  'spec-draft-n-max',
  'spec-draft-n-min',
  'spec-ngram-mod-n-match',
  'spec-ngram-mod-n-min',
  'spec-ngram-map-k4v-min-hits',
]

test.each(MINNED)(
  '2h.158: the field $key, whose range starts at 0 or above, has inputmode="numeric"',
  async (key) => {
    await form()
    const el = boxOfKey(key)
    expect(el.id, `${key}: the field id`).toBe(`model-profile-${key}-control`)
    expect(el.getAttribute('inputmode'), `${key}: input mode`).toBe('numeric')
  },
)

/** The any-whole-number keys of 157's first row that stay on the page: a
 * phone's numeric keyboard may have no minus key, so they get no input mode. */
const FREE = ['top-k', 'n-predict', 'reasoning-budget']

test('2h.158: the any-whole-number keys that stay get no input mode', async () => {
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

// ── 163: the string, and who can show it ────────────────────────────────

test('2h.163: field-error-whole-number has the brief text: a bounded field says its minimum, an any-whole-number field says this', async () => {
  const whole = en('field-error-whole-number')
  expect(whole, 'the test table').toBe(
    'Enter a whole number, without a decimal point.',
  )
  expect(t('field-error-whole-number'), 'the fe strings file').toBe(whole)
  // Cache-reuse has a minimum (157), so a decimal says the minimum, not this
  // text; top-k is 157's first row (any whole number), so it says this text.
  const { bff, main } = await form()
  type(boxOfKey('cache-reuse'), '1.5')
  fireEvent.click(saveButton(main))
  await within(main).findByText(en('save-invalid'))
  expectError(
    boxOfKey('cache-reuse'),
    en('field-error-number-min', '1'),
    'cache-reuse',
  )
  expect(
    within(main).queryByText(whole),
    'the whole-number text is not shown by a bounded field',
  ).toBeNull()
  type(boxOfKey('top-k'), '1.5')
  fireEvent.click(saveButton(main))
  await within(main).findByText(en('save-invalid'))
  expectError(boxOfKey('top-k'), whole, 'top-k')
  expect(bff.to('PUT', PUT), 'nothing sent').toHaveLength(0)
})
