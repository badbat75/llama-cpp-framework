// The Resource / context card of the model profile page editable (fe/briefs/
// 02h-model-profile-save.md, sub-slice 2h.4): the card's controls enabled, with
// Unified KV enabled only while Parallel slots' default box is unticked (155);
// the default box disabling and emptying the input, giving back the text it
// held, the focus moving to it when it comes back empty, and Discard and a 200
// resetting that memory (156); and at Save the integer check of every enabled
// unticked number field, its error text by range, the focus on the first
// failing control, the bar's alert and the number the PUT sends (157). The
// Embeddings card (130 to 154) is ModelProfileSave1 to 5, whose helpers this file
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
const CTX = en('model-profile-context-size')
const PAR = en('model-profile-parallel-slots')
const KVU = en('model-profile-unified-kv')
const SLOT = en('model-profile-per-slot-context')
const BATCH = en('model-profile-batch-size')
const UBATCH = en('model-profile-u-batch-size')
/** The default box of every number field (the Server page's 28a). */
const DEFAULT = en('field-default')
/** What the read gives. Per-slot context and Batch size are absent from the
 * file, so their box starts ticked (156). */
const AS_LOADED: Record<string, string> = {
  'ctx-size': '172032',
  parallel: '4',
  'kv-unified': 'true',
  'ubatch-size': '2048',
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

// ── Helpers (as ModelProfileSave2 to 5.test.tsx) ──────────────────────────────

/** The app at the address in the location; returns the fake bff and main. */
function open(options: BffOptions = {}) {
  const bff = fakeBff(options)
  render(<App />)
  return { bff, main: screen.getByRole('main') }
}

/** Waits for the loaded page: the h1 of the model and the Assets card. */
async function loaded(options: BffOptions = {}) {
  const { bff, main } = open(options)
  await within(main).findByRole('heading', { level: 1, name: ID })
  await within(main).findByRole('heading', {
    level: 3,
    name: en('model-profile-assets'),
  })
  return { bff, main }
}

/** The page read as `AS_LOADED` with revision `r1`, plus `options`. */
function form(options: BffOptions = {}) {
  return loaded({
    modelProfile: () =>
      reply(200, modelProfileDoc({}, { revision: 'r1', values: AS_LOADED })),
    ...options,
  })
}

/** The field whose label is `label` (its group). */
function field(main: HTMLElement, label: string): HTMLElement {
  return within(main).getByRole('group', { name: label })
}

/** The number input of a field. */
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

/** The save bar's status line (the only role="status" in the settings). */
function status(main: HTMLElement): HTMLElement {
  const h2 = within(main).getByRole('heading', {
    level: 2,
    name: en('model-profile-settings-title'),
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

/** The bar has no alert. */
function expectNoAlert(main: HTMLElement, what: string) {
  expect(
    within(main)
      .queryAllByRole('alert')
      .map((a) => a.textContent),
    what,
  ).toEqual([])
}

function described(el: HTMLElement): string[] {
  return (el.getAttribute('aria-describedby') ?? '')
    .split(/\s+/)
    .filter((id) => id !== '')
    .map((id) => document.getElementById(id)?.textContent ?? `(no #${id})`)
}

/** Asserts `el` is marked invalid with the error `text` under it (142, 157). */
function expectError(el: HTMLElement, text: string, what: string) {
  expect(el.getAttribute('aria-invalid'), `${what} aria-invalid`).toBe('true')
  expect(described(el), `${what} aria-describedby`).toContain(text)
  const error = (el.getAttribute('aria-describedby') ?? '')
    .split(/\s+/)
    .map((id) => document.getElementById(id))
    .find((e) => e !== null && e.textContent === text)
  expect(error, `${what}: the error is shown`).toBeDefined()
  expect(isHidden(error!), `${what}: the error is shown`).toBe(false)
  expect(before(el, error!), `${what}: the error comes after the control`).toBe(
    true,
  )
}

/** The field is not marked. */
function expectNoError(el: HTMLElement, what: string) {
  expect(el.getAttribute('aria-invalid') ?? 'false', what).toBe('false')
}

/** The card's number fields, with the minimum of 157's range table. */
const NUMS: { key: string; label: string; min: string }[] = [
  { key: 'ctx-size', label: CTX, min: '0' },
  { key: 'parallel', label: PAR, min: '1' },
  { key: 'kv-unified-per-slot', label: SLOT, min: '1' },
  { key: 'batch-size', label: BATCH, min: '1' },
  { key: 'ubatch-size', label: UBATCH, min: '1' },
]

/** Ticks the box of a field the read left absent, so its input is writable. */
function untickIfAbsent(main: HTMLElement, key: string, label: string) {
  if (key in AS_LOADED) return
  toggle(main, label)
}

// ── 155: the card is editable ───────────────────────────────────────────

test.each(NUMS)(
  '2h.155, 2h.156: $key has an enabled default box: ticked for a key the file lacks, so its input is disabled; unticked for one it has, so its input is enabled',
  async (n) => {
    const { main } = await form()
    const present = n.key in AS_LOADED
    const c = checkBox(main, n.label)
    // 155: the field's own control, the default box, is always enabled.
    expect(c.disabled, `${n.key}: the box enabled`).toBe(false)
    // 156: a key the file lacks starts with its box ticked.
    expect(c.checked, `${n.key}: the box ticked when the key is absent`).toBe(
      !present,
    )
    // 156: a ticked box disables and empties its input, an unticked one
    // leaves it enabled and holding the file's text.
    const input = box(main, n.label)
    expect(input.disabled, `${n.key}: the input follows the box`).toBe(!present)
    expect(input.value, `${n.key}: the input holds the file's text`).toBe(
      present ? (AS_LOADED[n.key] ?? '') : '',
    )
  },
)

test('2h.155: Unified KV is enabled while Parallel slots has its box unticked, and follows the box as the user edits; the other fields keep their state', async () => {
  const { main } = await form()
  const kv = selectOf(main, KVU)
  expect(kv.disabled, 'unticked Parallel: Unified KV enabled').toBe(false)
  // Parallel slots on default: Unified KV is disabled (MP 704).
  toggle(main, PAR)
  expect(selectOf(main, KVU).disabled, 'ticked: Unified KV disabled').toBe(true)
  // 155: what the rule takes away is the control, not the state: the select
  // keeps showing the value the file holds (162: "keeps its state").
  expect(selectOf(main, KVU).value, 'ticked: Unified KV keeps its state').toBe(
    'true',
  )
  // The other fields of the card are not touched by the rule (155: the other
  // five are always enabled): each field's default box stays enabled, and so
  // does the input of a field the file holds (156: its box is unticked).
  for (const n of NUMS.filter((x) => x.key !== 'parallel')) {
    expect(
      checkBox(main, n.label).disabled,
      `${n.label}: the box stays enabled`,
    ).toBe(false)
    if (n.key in AS_LOADED)
      expect(
        box(main, n.label).disabled,
        `${n.label}: the input stays enabled`,
      ).toBe(false)
  }
  // Live: unticking enables it again, no reload.
  toggle(main, PAR)
  expect(selectOf(main, KVU).disabled, 'unticked again: enabled').toBe(false)
})

// ── 156: the default box ────────────────────────────────────────────────

test('2h.156: ticking the box disables the input and leaves it empty, for a key the file had and for one it lacked', async () => {
  const { main } = await form()
  // ctx-size loaded 172032: ticking empties the input and disables it.
  expect(box(main, CTX).value, 'loaded value').toBe('172032')
  toggle(main, CTX)
  expect(box(main, CTX).value, 'ticked: empty').toBe('')
  expect(box(main, CTX).disabled, 'ticked: disabled').toBe(true)
  expect(checkBox(main, CTX).checked, 'the box is ticked').toBe(true)
  // A key the file lacked: ticked from the start, so empty and disabled.
  expect(checkBox(main, BATCH).checked, 'absent key: ticked').toBe(true)
  expect(box(main, BATCH).value, 'absent key: empty').toBe('')
  expect(box(main, BATCH).disabled, 'absent key: disabled').toBe(true)
})

test('2h.156: unticking gives the input back the text it held: the file value, then what the user typed', async () => {
  const { main } = await form()
  // The file value, typed over.
  type(box(main, CTX), '65536')
  toggle(main, CTX)
  expect(box(main, CTX).value, 'ticked over a typed value').toBe('')
  const c = toggle(main, CTX)
  expect(box(main, CTX).value, 'unticked: the typed text').toBe('65536')
  expect(box(main, CTX).disabled, 'unticked: enabled again').toBe(false)
  expect(document.activeElement, 'focus stays on the box').toBe(c)
  // A valueless field: the file value comes back.
  toggle(main, UBATCH)
  expect(box(main, UBATCH).value, 'ticked').toBe('')
  toggle(main, UBATCH)
  expect(box(main, UBATCH).value, 'unticked: the file value').toBe('2048')
})

test('2h.156: a key the file lacked comes back empty with the focus on the input; the typed text is what comes back next', async () => {
  const { main } = await form()
  const input = box(main, SLOT)
  expect(checkBox(main, SLOT).checked, 'absent key: ticked').toBe(true)
  toggle(main, SLOT)
  expect(box(main, SLOT).value, 'unticked: empty').toBe('')
  expect(box(main, SLOT).disabled, 'unticked: enabled').toBe(false)
  expect(document.activeElement, 'focus on the input').toBe(input)
  type(box(main, SLOT), '4096')
  toggle(main, SLOT)
  expect(box(main, SLOT).value, 'ticked: empty again').toBe('')
  toggle(main, SLOT)
  expect(box(main, SLOT).value, 'unticked: the typed text').toBe('4096')
})

test('2h.156: Discard and a 200 reset that memory to the loaded values', async () => {
  const { bff, main } = await form()
  type(box(main, CTX), '65536')
  toggle(main, CTX)
  fireEvent.click(discardButton(main))
  expect(checkBox(main, CTX).checked, 'Discard: unticked').toBe(false)
  expect(box(main, CTX).value, 'Discard: the loaded text').toBe('172032')
  // Saved with the box ticked: the key is gone from the loaded values, so
  // unticking gives an empty input, not the text held before the save.
  toggle(main, CTX)
  await save(main, bff)
  expect(sentValues(bff), 'the ticked key goes out as null').toEqual({
    'ctx-size': null,
  })
  await waitFor(() => expect(status(main).textContent).toBe(en('save-saved')))
  expect(checkBox(main, CTX).checked, 'after the save: ticked').toBe(true)
  toggle(main, CTX)
  expect(box(main, CTX).value, 'after the save: empty').toBe('')
})

// ── 157: the integer check at Save ──────────────────────────────────────

/** Texts that fail their field's range (157's table). */
const BAD: { key: string; label: string; text: string; min: string }[] = [
  { key: 'ctx-size', label: CTX, text: '-1', min: '0' },
  { key: 'ctx-size', label: CTX, text: 'abc', min: '0' },
  { key: 'ctx-size', label: CTX, text: '2147483648', min: '0' },
  { key: 'parallel', label: PAR, text: '0', min: '1' },
  { key: 'batch-size', label: BATCH, text: '1.5', min: '1' },
  { key: 'ubatch-size', label: UBATCH, text: '12x', min: '1' },
  { key: 'kv-unified-per-slot', label: SLOT, text: '', min: '1' },
]

test.each(BAD)(
  '2h.157: $key rejects "$text" with field-error-number-min of $min, marks it, focuses it, alerts save-invalid and sends nothing',
  async (c) => {
    const { bff, main } = await form()
    untickIfAbsent(main, c.key, c.label)
    type(box(main, c.label), c.text)
    fireEvent.click(saveButton(main))
    await within(main).findByText(en('save-invalid'))
    const error = en('field-error-number-min', c.min)
    expectError(box(main, c.label), error, c.label)
    expect(document.activeElement, `focus on ${c.label}`).toBe(
      box(main, c.label),
    )
    expectAlert(main, en('save-invalid'))
    expect(bff.to('PUT', PUT), 'nothing sent').toHaveLength(0)
  },
)

test('2h.157: the focus goes to the first failing control in DOM order, every failing field is marked and the passing ones are not', async () => {
  const { bff, main } = await form()
  expect(
    before(field(main, CTX), field(main, UBATCH)),
    'Context size is the first field of the card',
  ).toBe(true)
  type(box(main, CTX), '-5')
  type(box(main, UBATCH), '0')
  fireEvent.click(saveButton(main))
  await within(main).findByText(en('save-invalid'))
  expectError(box(main, CTX), en('field-error-number-min', '0'), CTX)
  expectError(box(main, UBATCH), en('field-error-number-min', '1'), UBATCH)
  expect(document.activeElement, 'focus on the first failing control').toBe(
    box(main, CTX),
  )
  expectAlert(main, en('save-invalid'))
  // The fields left alone pass and are not marked.
  expectNoError(box(main, PAR), PAR)
  expectNoError(box(main, SLOT), SLOT)
  expectNoError(box(main, BATCH), BATCH)
  expect(bff.to('PUT', PUT), 'nothing sent').toHaveLength(0)
  expect(status(main).textContent, 'the bar still says unsaved').toBe(
    en('save-unsaved'),
  )
})

test('2h.157: every enabled unticked field is checked, changed or not: a value the read gave out of range blocks the save', async () => {
  const { bff, main } = await loaded({
    modelProfile: () =>
      reply(
        200,
        modelProfileDoc(
          {},
          { revision: 'r1', values: { ...AS_LOADED, parallel: '0' } },
        ),
      ),
  })
  type(box(main, CTX), '65536')
  fireEvent.click(saveButton(main))
  await within(main).findByText(en('save-invalid'))
  expectError(box(main, PAR), en('field-error-number-min', '1'), PAR)
  expectNoError(box(main, CTX), CTX)
  expect(document.activeElement, 'focus on Parallel slots').toBe(box(main, PAR))
  expectAlert(main, en('save-invalid'))
  expect(bff.to('PUT', PUT), 'nothing sent').toHaveLength(0)
})

test('2h.157: a field whose box is ticked is not checked: its text is out of range and the save goes through', async () => {
  const { bff, main } = await form()
  type(box(main, CTX), 'abc')
  toggle(main, CTX)
  await save(main, bff)
  expectNoAlert(main, 'a ticked field says nothing')
  expect(sentValues(bff), 'the ticked key goes out as null').toEqual({
    'ctx-size': null,
  })
  await waitFor(() => expect(status(main).textContent).toBe(en('save-saved')))
})

// ── 157: what is sent ───────────────────────────────────────────────────

/** Text typed in Context size and the number the PUT must carry (157). */
const SENT: { text: string; sends: string }[] = [
  { text: '+0172032', sends: '172032' },
  { text: '  44  ', sends: '44' },
  { text: '000', sends: '0' },
  { text: '+8', sends: '8' },
]

test.each(SENT)(
  '2h.157: Context size typed "$text" is sent as $sends',
  async (c) => {
    const { bff, main } = await form()
    type(box(main, CTX), c.text)
    await save(main, bff)
    expect(sentValues(bff), 'decimal, no sign, no leading zeros').toEqual({
      'ctx-size': c.sends,
    })
  },
)

test('2h.157: a ticked box sends null for a key the file had, and nothing for a key it lacked', async () => {
  const { bff, main } = await form()
  // ctx-size is in the file: its tick sends null.
  toggle(main, CTX)
  // batch-size is not: untick it, type, tick it back, and the key is absent
  // from the body, not null.
  toggle(main, BATCH)
  type(box(main, BATCH), '512')
  toggle(main, BATCH)
  await save(main, bff)
  const values = sentValues(bff)
  expect(values, 'the key the file had, removed').toMatchObject({
    'ctx-size': null,
  })
  expect('batch-size' in values, 'the key it lacked is not sent').toBe(false)
})
