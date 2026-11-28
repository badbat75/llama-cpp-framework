// Part of sub-slice 2h.7 of fe/briefs/02h-model-profile-save.md, as brief 22
// IP5 narrows it (176, 181, 182): every control the card still shows is enabled
// only while the read's `mmproj` is present and not empty, the note
// `model-profile-vision-off` sits after the card's h3 while `mmproj` is missing
// (a plain paragraph, not a live region, not in any aria-describedby), what each
// remaining field of 2e's table 102 sends by its kind (133: the two 1-or-more
// integers of 157), and the exact text of the new string (182). The Vision on
// GPU checkbox and the Vision GPU device select are `PLACEMENT_KEYS` and left
// the page with them, so the checkbox rule of 176 and the device select's list
// of 179 left too. The Hardware card of 175 went to the instance profile. The
// card's read-only look is 2e's 102, in ModelProfileCards3.test.tsx; the helpers
// are those of ModelProfileSave8 to ModelProfileSave11.test.tsx, which this file
// repeats (a test file imports no other test file).
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
  modelProfileDoc,
  reply,
  startAt,
  stubDialog,
  type BffOptions,
} from './test/bff.ts'

const ID = 'Qwen3-27B'
const HASH = `#/profiles/model/${ID}`
const PUT = `/api/profiles/model/${ID}`
const HEADING = en('model-profile-vision')
const NEXT = en('model-profile-speculative-decoding')
const MIN = en('model-profile-min-image-tokens')
const MAX = en('model-profile-max-image-tokens')
const DEFAULT = en('field-default')
/** 182: the brief's exact text of the new string, as a literal. */
const OFF_TEXT =
  'Used only with an MMProj file, and this model profile has none (Assets).'
/** 157's row of image-min-tokens and image-max-tokens: 1 or more. */
const MIN_ONE = en('field-error-number-min', '1')
const MMPROJ = 'Qwen3-VL-mmproj-BF16.gguf'
/** What the read gives: every key the card still shows present, each a value
 * the configurator itself would write (so no written-as line). */
const AS_LOADED: Record<string, string> = {
  mmproj: MMPROJ,
  'image-min-tokens': '1024',
  'image-max-tokens': '2048',
}

/** `AS_LOADED` without the named keys: the file does not hold them. */
function without(...keys: string[]): Record<string, string> {
  const out = { ...AS_LOADED }
  for (const k of keys) delete out[k]
  return out
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

// ── Helpers (as ModelProfileSave8 to ModelProfileSave11.test.tsx) ───────────────────

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
  await within(main).findByRole('heading', { level: 3, name: HEADING })
  return { bff, main }
}

function headingOf(main: HTMLElement, name: string): HTMLElement {
  return within(main).getByRole('heading', { level: 3, name })
}

function selectOf(main: HTMLElement, label: string): HTMLSelectElement {
  return within(main).getByRole('combobox', {
    name: label,
  }) as HTMLSelectElement
}

function pick(main: HTMLElement, label: string, value: string) {
  fireEvent.change(selectOf(main, label), { target: { value } })
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

/** Every control of the card: the box and the input of each number field (the
 * order of 2e's table 102, as IP5 leaves it). */
function cardControls(
  main: HTMLElement,
): Array<HTMLInputElement | HTMLSelectElement> {
  return [
    checkBox(main, MIN),
    box(main, MIN),
    checkBox(main, MAX),
    box(main, MAX),
  ]
}

function saveButton(main: HTMLElement): HTMLElement {
  return within(main).getByRole('button', { name: en('save-button') })
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

/** The card's note (176), by its text. */
function note(main: HTMLElement): HTMLElement {
  return within(main).getByText(OFF_TEXT)
}

// ── 176, 182: no MMProj: every control disabled, the note shown ─────────

/** The two reads 176 counts as "no MMProj": the key absent, and the key
 * present with an empty value. */
const NO_MMPROJ = [
  { name: 'absent', values: without('mmproj') },
  { name: 'empty', values: { ...AS_LOADED, mmproj: '' } },
]

test.each(NO_MMPROJ)(
  '2h.176, 2h.182: mmproj $name: every Vision control is natively disabled and the note model-profile-vision-off sits right after the card h3, before the first field',
  async (c) => {
    const { main } = await form(c.values)
    for (const el of cardControls(main))
      expect(el.disabled, `${el.id}: disabled while mmproj is ${c.name}`).toBe(
        true,
      )
    const line = note(main)
    expect(isHidden(line), 'the note is shown').toBe(false)
    expect(line.textContent, '182: the string as the brief words it').toBe(
      OFF_TEXT,
    )
    const h3 = headingOf(main, HEADING)
    expect(before(h3, line), 'the note comes after the card h3').toBe(true)
    expect(
      before(line, box(main, MIN)),
      'the note comes before the first field',
    ).toBe(true)
    expect(
      before(line, headingOf(main, NEXT)),
      'the note is inside the Vision card',
    ).toBe(true)
  },
)

test('2h.182: the new string model-profile-vision-off is the briefs sentence, word for word', () => {
  expect(en('model-profile-vision-off')).toBe(OFF_TEXT)
})

test('2h.181: the note is a plain paragraph: no live region, no role, not in any control of the card aria-describedby', async () => {
  const { main } = await form(without('mmproj'))
  const line = note(main)
  expect(line.getAttribute('role'), 'no role').toBeNull()
  expect(line.getAttribute('aria-live'), 'not a live region').toBeNull()
  expect(
    line.closest('[role="status"],[role="alert"],[role="log"],[aria-live]'),
    'not inside a live region',
  ).toBeNull()
  for (const el of cardControls(main))
    expect(
      described(el),
      `${el.id}: the note is not its description`,
    ).not.toContain(OFF_TEXT)
})

// ── 176: the enable rule of the card ───────────────────────────────────

test('2h.176, 2h.158: with mmproj present, no note and every control enabled: each box unticked with its input on the file text and inputmode numeric', async () => {
  const { main } = await form()
  expect(
    within(main).queryByText(OFF_TEXT),
    'an MMProj: no note (176)',
  ).toBeNull()
  for (const [label, key] of [
    [MIN, 'image-min-tokens'],
    [MAX, 'image-max-tokens'],
  ]) {
    const b = checkBox(main, label)
    expect(b.disabled, `${key}: the box enabled (176)`).toBe(false)
    expect(b.checked, `${key}: a key the file has: box unticked`).toBe(false)
    const el = box(main, label)
    expect(
      el.disabled,
      `${key}: the input follows the unticked box (156)`,
    ).toBe(false)
    expect(el.value, `${key}: the input holds the file text`).toBe(
      AS_LOADED[key],
    )
    expect(el.id, `${key}: the field id`).toBe(`model-profile-${key}-control`)
    expect(el.getAttribute('inputmode'), `${key}: input mode (158)`).toBe(
      'numeric',
    )
  }
})

// ── 175, 133: what each field sends by its kind ────────────────────────

/** The card's two integer fields (2e's 102, 157's 1-or-more row). */
const NUMBERS = [
  { key: 'image-min-tokens', label: MIN },
  { key: 'image-max-tokens', label: MAX },
]

test.each(NUMBERS)(
  '2h.175, 2h.157: $key with its box unticked sends the number in decimal with no sign and no leading zeros',
  async (c) => {
    const { bff, main } = await form()
    type(box(main, c.label), '+01024')
    await save(main, bff)
    expect(sentValues(bff)[c.key], '+01024 sends 1024').toBe('1024')
  },
)

test.each(NUMBERS)(
  '2h.175, 2h.157: $key rejects "0" with the 1-or-more error, marks it, focuses it, alerts save-invalid and sends nothing',
  async (c) => {
    const { bff, main } = await form()
    type(box(main, c.label), '0')
    fireEvent.click(saveButton(main))
    await within(main).findByText(en('save-invalid'))
    const el = box(main, c.label)
    expectError(el, MIN_ONE, c.label)
    expect(document.activeElement, `focus on ${c.label}`).toBe(el)
    expect(bff.to('PUT', PUT), 'nothing sent').toHaveLength(0)
  },
)

test('2h.133, 2h.156: a Vision number whose default box is ticked sends null for a key the file had', async () => {
  const { bff, main } = await form()
  // Both keys are in the file: ticking their boxes sends null for each.
  toggle(main, MIN)
  toggle(main, MAX)
  await save(main, bff)
  expect(sentValues(bff), 'the keys the file had, removed').toEqual({
    'image-min-tokens': null,
    'image-max-tokens': null,
  })
})

test('2h.156, 2h.133: an absent Vision number starts with its box ticked, its input disabled and empty, and a ticked box on a key the file lacks sends nothing', async () => {
  const { bff, main } = await form(without('image-max-tokens'))
  const c = checkBox(main, MAX)
  expect(c.disabled, 'the box enabled (mmproj present)').toBe(false)
  expect(c.checked, 'an absent key ticks the box (156)').toBe(true)
  const el = box(main, MAX)
  expect(el.disabled, 'a ticked box disables the input (156)').toBe(true)
  expect(el.value, 'a ticked box empties the input').toBe('')
  fireEvent.click(saveButton(main))
  expect(bff.to('PUT', PUT), 'nothing to send').toHaveLength(0)
})

// ── 179: the device select's list does not follow the pick ─────────────
// The Vision GPU device select is a `PLACEMENT_KEYS` key (brief 22 IP5.5), so
// the card shows no device select and 179's list pin leaves with it.

/** The Pooling select of the Embeddings card: a field of another card, used
 * below as the change the user does make (nothing else on the page moves). */
const POOLING = en('model-profile-pooling')

// ── 157, 176: a field a rule locks is not checked at Save ──────────────

test('2h.157, 2h.176: a locked number field whose file value is bad is not checked at Save: a change of another card is sent, the bar does not complain, the field is not marked and the focus does not move to it', async () => {
  // No mmproj: the rule of 176 locks the card, so the user cannot edit the
  // 0 the file holds for image-min-tokens.
  const { bff, main } = await form({
    ...without('mmproj'),
    'image-min-tokens': '0',
  })
  const input = box(main, MIN)
  const tick = checkBox(main, MIN)
  expect(
    input.disabled,
    'image-min-tokens locked by the rule (no mmproj)',
  ).toBe(true)
  expect(input.value, 'the locked field still shows the file value').toBe('0')
  pick(main, POOLING, 'rank')
  await save(main, bff)
  expect(sentValues(bff), 'the change of the other card is sent').toEqual({
    pooling: 'rank',
  })
  expect(
    within(main).queryByText(en('save-invalid')),
    'the bar does not complain about a field nobody can edit',
  ).toBeNull()
  expect(
    input.getAttribute('aria-invalid'),
    'the locked field is not marked',
  ).toBe(null)
  expect(
    described(input),
    'no error text under the locked field',
  ).not.toContain(MIN_ONE)
  expect(
    document.activeElement,
    'the focus did not move to the locked field',
  ).not.toBe(input)
  expect(document.activeElement, 'nor to its default box').not.toBe(tick)
})

test('2h.157: the same bad value with mmproj present, where the field is enabled: Save is refused with its own error, marked, focused, nothing sent (the field is checked changed or not)', async () => {
  const { bff, main } = await form({ ...AS_LOADED, 'image-min-tokens': '0' })
  const el = box(main, MIN)
  expect(el.disabled, 'image-min-tokens enabled (mmproj present)').toBe(false)
  expect(el.value, 'the loaded value shows untouched').toBe('0')
  fireEvent.click(saveButton(main))
  await within(main).findByText(en('save-invalid'))
  expectError(el, MIN_ONE, MIN)
  expect(document.activeElement, 'focus on the enabled field').toBe(el)
  expect(bff.to('PUT', PUT), 'nothing sent').toHaveLength(0)
})
