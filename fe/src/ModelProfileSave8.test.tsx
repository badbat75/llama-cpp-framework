// The first part of sub-slice 2h.5 of fe/briefs/02h-model-profile-save.md: the KV cache
// card editable (164: every field of the card always enabled), what each of its
// fields sends (133: the cache types as a select of words, flash-attn as the
// tri-state, the number fields by their box and their range), the input mode and
// the unit of Cache RAM (158, 159) and the integer check of the card's four
// number fields (157: cache-ram any whole number, ctx-checkpoints and
// checkpoint-min-step 0 or more, cache-reuse 1 or more). Sampling overrides,
// Advanced and the decimal and JSON checks (165, 166) are not here. The card's
// read-only look is 2e's 96, in ModelProfileCards2.test.tsx; the helpers are those of
// ModelProfileSave6 and ModelProfileSave7.test.tsx, which this file repeats (a test file
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
const MODE = en('model-profile-embedding-mode')
const KCACHE = en('model-profile-k-cache-type')
const VCACHE = en('model-profile-v-cache-type')
const FLASH = en('model-profile-flash-attention')
const RAM = en('model-profile-cache-ram')
const REUSE = en('model-profile-cache-reuse')
const CHECK = en('model-profile-context-checkpoints')
const STEP = en('model-profile-checkpoint-step')
const DEFAULT = en('field-default')
/** The literal unit of Cache RAM (2e's 89), not a strings key. */
const MIB = 'MiB'
/** A flash-attn value the tri-state does not list: the extra option of 87. */
const AUTO = 'auto'
/** The card's number fields whose input sits behind a default box (156), by
 * key, with the label. The two cache types and flash-attn have no box. */
const BOXED: Record<string, string> = {
  'cache-ram': RAM,
  'cache-reuse': REUSE,
  'ctx-checkpoints': CHECK,
  'checkpoint-min-step': STEP,
}
/** What the read gives (2e's 99, the card's keys). Checkpoint step is absent,
 * so its box starts ticked (156). */
const AS_LOADED: Record<string, string> = {
  'cache-type-k': 'q8_0',
  'cache-type-v': 'q8_0',
  'flash-attn': 'true',
  'cache-ram': '-1',
  'cache-reuse': '256',
  'ctx-checkpoints': '0',
}
/** The card's fields, in the order of 2e's table 96. */
const FIELDS: { key: string; label: string }[] = [
  { key: 'cache-type-k', label: KCACHE },
  { key: 'cache-type-v', label: VCACHE },
  { key: 'flash-attn', label: FLASH },
  ...Object.entries(BOXED).map(([key, label]) => ({ key, label })),
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

// ── Helpers (as ModelProfileSave6 and ModelProfileSave7.test.tsx) ───────────────────

/** `AS_LOADED` without the named keys: the file does not hold them. */
function without(...keys: string[]): Record<string, string> {
  const out = { ...AS_LOADED }
  for (const k of keys) delete out[k]
  return out
}

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

// ── 164: every field of the card is enabled, always ─────────────────────

test.each(FIELDS)(
  '2h.164: the card field $key has its control enabled always, and a number field has its default box enabled: ticked for a key the file lacks, unticked for one it has',
  async (n) => {
    const { main } = await form()
    const present = n.key in AS_LOADED
    if (n.key in BOXED) {
      const c = checkBox(main, n.label)
      expect(c.disabled, `${n.key}: the box enabled`).toBe(false)
      expect(c.checked, `${n.key}: the box ticked when the key is absent`).toBe(
        !present,
      )
      const input = box(main, n.label)
      expect(input.disabled, `${n.key}: the input follows the box`).toBe(
        !present,
      )
      expect(input.value, `${n.key}: the input holds the file's text`).toBe(
        present ? AS_LOADED[n.key] : '',
      )
      return
    }
    expect(
      selectOf(main, n.label).disabled,
      `${n.key}: the select enabled`,
    ).toBe(false)
  },
)

test('2h.164: on a read that holds every key of the card, all seven controls are enabled', async () => {
  const { main } = await form({ ...AS_LOADED, 'checkpoint-min-step': '8192' })
  for (const n of FIELDS) {
    const el = n.key in BOXED ? box(main, n.label) : selectOf(main, n.label)
    expect(el.disabled, `${n.key}: enabled on a read that holds the key`).toBe(
      false,
    )
  }
  for (const [key, label] of Object.entries(BOXED))
    expect(checkBox(main, label).disabled, `${key}: the box enabled`).toBe(
      false,
    )
})

// ── 133: what the two cache-type selects send ───────────────────────────

test('2h.133: K-cache type q8_0 back to default sends null, for a key the file had', async () => {
  const { bff, main } = await form()
  expect(selectOf(main, KCACHE).disabled, 'the select enabled').toBe(false)
  pick(main, KCACHE, 'default')
  await save(main, bff)
  expect(sentValues(bff)['cache-type-k'], 'default removes the key').toBeNull()
})

test.each([
  { label: VCACHE, key: 'cache-type-v', to: 'f16' },
  { label: KCACHE, key: 'cache-type-k', to: 'iq4_nl' },
])(
  '2h.133: $key picks a listed cache type and sends $to, the option value',
  async (c) => {
    const { bff, main } = await form()
    pick(main, c.label, c.to)
    await save(main, bff)
    expect(sentValues(bff)[c.key], 'the option value').toBe(c.to)
  },
)

test('2h.133: a cache type the file lacks shows default: picking default again is not a change, so only the changed key travels', async () => {
  const { bff, main } = await form(without('cache-type-k'))
  // cache-type-k is absent from the file, so it shows default.
  const select = selectOf(main, KCACHE)
  expect(select.value, 'absent shows default').toBe('default')
  pick(main, KCACHE, 'default')
  // Embedding mode (2h.1) carries the save.
  pick(main, MODE, 'true')
  await save(main, bff)
  expect(
    Object.keys(sentValues(bff)),
    'the unchanged key is absent, only the changed one travels',
  ).toEqual(['embeddings'])
})

// ── 133: what the flash-attn tri-state sends ────────────────────────────

test.each([
  // 133's tri-state row read through 132: picking the option the select
  // already shows is no change, so each row loads the other state first.
  { load: 'false', to: 'true', sends: 'true' },
  { load: 'true', to: 'false', sends: 'false' },
  { load: 'true', to: 'default', sends: null },
])(
  '2h.133: flash-attn loaded $load picks $to and sends $sends (the tri-state row of 133)',
  async (c) => {
    const { bff, main } = await form({ ...AS_LOADED, 'flash-attn': c.load })
    pick(main, FLASH, c.to)
    await save(main, bff)
    expect(sentValues(bff)['flash-attn'], `the tri-state sends ${c.to}`).toBe(
      c.sends,
    )
  },
)

test('2h.133: flash-attn loaded auto (the extra option) is not sent until another option is picked', async () => {
  const { bff, main } = await form({ ...AS_LOADED, 'flash-attn': AUTO })
  const select = selectOf(main, FLASH)
  expect(select.value, 'the extra option carries the loaded value').toBe(AUTO)
  expect(select.disabled, 'the select enabled').toBe(false)
  // Picking the extra option again is the loaded state: never sent.
  pick(main, FLASH, AUTO)
  pick(main, MODE, 'true')
  await save(main, bff)
  expect(
    Object.keys(sentValues(bff)),
    'the extra option is never sent',
  ).toEqual(['embeddings'])
  // A listed option is sent.
  pick(main, FLASH, 'false')
  await save(main, bff, 2)
  expect(
    sentValues(bff, 2)['flash-attn'],
    'a listed option sends its value',
  ).toBe('false')
})

// ── 158, 159, 134: Cache RAM ────────────────────────────────────────────

test('2h.158, 2h.159, 2h.134: Cache RAM has no input mode, its unit MiB is the input first description, and the enabled input has no full-value line', async () => {
  const { main } = await form()
  const el = box(main, RAM)
  expect(el.disabled, 'the input enabled (164)').toBe(false)
  expect(el.id, 'the field id').toBe('model-profile-cache-ram-control')
  expect(el.getAttribute('inputmode'), 'no input mode (158)').toBeNull()
  const uid = unitId(el)
  const unit = document.getElementById(uid)
  expect(unit, `the unit has the id ${uid}`).not.toBe(null)
  expect(unit!.textContent, 'the unit text').toBe(MIB)
  expect(isHidden(unit!), 'the unit is visible text').toBe(false)
  expect(before(el, unit!), 'the unit sits after the input').toBe(true)
  expect(describedIds(el)[0], 'the unit is the first description').toBe(uid)
  expect(described(el)[0], 'the unit is read before the hint').toBe(MIB)
  // 134: the full-value line shows only under a disabled control.
  expect(
    document.getElementById('model-profile-full-cache-ram'),
    'no full-value line under an enabled input',
  ).toBeNull()
})

test('2h.158: the card is editable, so its three ranged integer inputs keep inputmode="numeric"', async () => {
  const { main } = await form({ ...AS_LOADED, 'checkpoint-min-step': '8192' })
  for (const [key, label] of [
    ['cache-reuse', REUSE],
    ['ctx-checkpoints', CHECK],
    ['checkpoint-min-step', STEP],
  ] as [string, string][]) {
    const el = box(main, label)
    expect(el.id, `${key}: the field id`).toBe(`model-profile-${key}-control`)
    expect(el.getAttribute('inputmode'), `${key}: input mode`).toBe('numeric')
  }
})

// ── 157: the integer check of the card ──────────────────────────────────

/** Texts that fail their field (157): cache-ram is the any-whole-number row,
 * every other key its range. */
const BAD: { key: string; label: string; text: string; error: string }[] = [
  {
    key: 'cache-ram',
    label: RAM,
    text: '1.5',
    error: en('field-error-whole-number'),
  },
  {
    key: 'cache-ram',
    label: RAM,
    text: 'abc',
    error: en('field-error-whole-number'),
  },
  {
    key: 'ctx-checkpoints',
    label: CHECK,
    text: '-1',
    error: en('field-error-number-min', '0'),
  },
  {
    key: 'checkpoint-min-step',
    label: STEP,
    text: '-1',
    error: en('field-error-number-min', '0'),
  },
  {
    key: 'cache-reuse',
    label: REUSE,
    text: '0',
    error: en('field-error-number-min', '1'),
  },
]

test.each(BAD)(
  '2h.157: $key rejects "$text" with $error, marks it, focuses it, alerts save-invalid and sends nothing',
  async (c) => {
    const { bff, main } = await form()
    if (!(c.key in AS_LOADED)) toggle(main, c.label)
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

test('2h.157: Cache RAM takes any whole number, a negative included, and sends it in decimal with no sign and no leading zeros', async () => {
  const { bff, main } = await form()
  type(box(main, RAM), '-1024')
  await save(main, bff)
  expect(sentValues(bff)['cache-ram'], 'a negative goes out as typed').toBe(
    '-1024',
  )
  // Tear the first page down first: a second form() with both pages in the
  // document finds the first page's Cache RAM.
  cleanup()
  const second = await form()
  type(box(second.main, RAM), '+08')
  await save(second.main, second.bff)
  expect(sentValues(second.bff)['cache-ram'], '+08 sends 8').toBe('8')
})

test('2h.157: a 0 is accepted where the range starts at 0: checkpoints and checkpoint step both go out', async () => {
  const { bff, main } = await form()
  type(box(main, CHECK), '4')
  toggle(main, STEP)
  type(box(main, STEP), '0')
  await save(main, bff)
  expect(sentValues(bff), 'both keys go out').toEqual({
    'ctx-checkpoints': '4',
    'checkpoint-min-step': '0',
  })
})

test('2h.133, 2h.156: a number field whose default box is ticked sends null for a key the file had, and nothing for a key it lacked', async () => {
  const { bff, main } = await form()
  // cache-ram is in the file (-1): its tick sends null.
  toggle(main, RAM)
  // checkpoint-min-step is not: untick, type, tick it back, and the key is
  // absent from the body, not null.
  toggle(main, STEP)
  type(box(main, STEP), '512')
  toggle(main, STEP)
  await save(main, bff)
  const values = sentValues(bff)
  expect(values, 'the key the file had, removed').toMatchObject({
    'cache-ram': null,
  })
  expect('checkpoint-min-step' in values, 'the key it lacked is not sent').toBe(
    false,
  )
})

// ── 132, 133: a key the card lacks shows its absent option ───────────────
// A key the file does not hold has the row's absent option as its state (132),
// so the control shows that one option for the absence and picking it is no
// change. In this card the absent option is also the option listed first (the
// row's `absent` is the value of `options[0]`): the test pins that equality for
// the card's three selects, so a row whose absent option is not the first one
// reds here, and the state a read of an absent key gives is pinned by name.

test('2h.132, 2h.133: a key the card lacks shows its absent option, which is the first option the select lists: picking it back is clean and a save sends nothing', async () => {
  const { bff, main } = await form(
    without('cache-type-k', 'cache-type-v', 'flash-attn'),
  )
  for (const label of [KCACHE, VCACHE, FLASH]) {
    const select = selectOf(main, label)
    expect(select.value, `${label}: an absent key shows default`).toBe(
      'default',
    )
    expect(
      select.options[0]?.value,
      `${label}: default is the first option listed, so the absent option is it`,
    ).toBe(select.value)
    expect(
      Array.from(select.options).filter((o) => o.value === 'default').length,
      `${label}: the absent option is listed once, no extra option added`,
    ).toBe(1)
  }
  // Picking the option that stands for the absence is no change (132).
  pick(main, KCACHE, 'default')
  pick(main, VCACHE, 'default')
  pick(main, FLASH, 'default')
  expect(status(main).textContent, 'picking default is clean').toBe('')
  for (const b of [saveButton(main), discardButton(main)])
    expect(b.getAttribute('aria-disabled'), 'clean: the buttons inert').toBe(
      'true',
    )
  fireEvent.click(saveButton(main))
  expect(bff.to('PUT', PUT), 'a clean Save sends nothing').toHaveLength(0)
})

// ── 132: a flag key the file lacks, set back to the option that shows it ──
// The off / on row's absent option is `off` (2e's table), the tri-state's is
// `default`, so picking the option the select already shows for a key the file
// does not hold is no change (132): the form stays clean and the save sends
// nothing, not a removal.

test('2h.132: Embedding mode with no key picks off and flash-attn with no key picks default: clean, the buttons inert, nothing sent', async () => {
  const { bff, main } = await form(without('flash-attn'))
  const mode = selectOf(main, MODE)
  expect(mode.value, 'no embeddings key: the select shows off').toBe('false')
  const flash = selectOf(main, FLASH)
  expect(flash.disabled, 'flash-attn enabled (164)').toBe(false)
  expect(flash.value, 'no flash-attn key: the tri-state shows default').toBe(
    'default',
  )
  pick(main, MODE, 'false')
  pick(main, FLASH, 'default')
  expect(
    status(main).textContent,
    'both back to the option that shows the absence: clean',
  ).toBe('')
  for (const b of [saveButton(main), discardButton(main)])
    expect(b.getAttribute('aria-disabled'), 'clean: the buttons inert').toBe(
      'true',
    )
  fireEvent.click(saveButton(main))
  expect(bff.to('PUT', PUT), 'a clean Save sends nothing').toHaveLength(0)
})

// ── 133: embeddings = false in the file, on then off again ────────────────
// The off of a key the file holds is the loaded state, so it is not a change
// and the save sends nothing at all: `embeddings: null` would remove the line
// the user never asked to remove (132, 133's off / on row).

test('2h.133: embeddings = false in the file: untouched clean, on dirty, off again clean with no PUT and no embeddings: null', async () => {
  const { bff, main } = await form({ ...AS_LOADED, embeddings: 'false' })
  expect(selectOf(main, MODE).value, 'the file spells off').toBe('false')
  expect(status(main).textContent, 'untouched: clean').toBe('')
  pick(main, MODE, 'true')
  expect(status(main).textContent, 'on: dirty').toBe(en('save-unsaved'))
  expect(
    saveButton(main).getAttribute('aria-disabled') ?? 'false',
    'dirty: Save enabled',
  ).toBe('false')
  pick(main, MODE, 'false')
  expect(status(main).textContent, 'off again: clean').toBe('')
  for (const b of [saveButton(main), discardButton(main)])
    expect(b.getAttribute('aria-disabled'), 'clean: the buttons inert').toBe(
      'true',
    )
  fireEvent.click(saveButton(main))
  expect(
    bff.to('PUT', PUT),
    'no PUT, so no embeddings: null either',
  ).toHaveLength(0)
})

// ── 157: the any-whole-number range is the 32-bit one ─────────────────────
// 157 gives 2147483647 as the maximum every row writes (`emit_i32`), so the
// row with no minimum runs to the 32-bit minimum too.

test('2h.157: Cache RAM takes the 32-bit minimum as written, and one below it is refused with the whole-number error', async () => {
  const { bff, main } = await form()
  type(box(main, RAM), '-2147483648')
  await save(main, bff)
  expect(sentValues(bff)['cache-ram'], 'the minimum goes out as typed').toBe(
    '-2147483648',
  )
  // Tear the first page down first: a second form() with both pages in the
  // document finds the first page's Cache RAM.
  cleanup()
  const second = await form()
  type(box(second.main, RAM), '-2147483649')
  fireEvent.click(saveButton(second.main))
  await within(second.main).findByText(en('save-invalid'))
  const el = box(second.main, RAM)
  expectError(el, en('field-error-whole-number'), RAM)
  expect(document.activeElement, 'focus on Cache RAM').toBe(el)
  expectAlert(second.main, en('save-invalid'))
  expect(second.bff.to('PUT', PUT), 'nothing sent').toHaveLength(0)
})

test('2h.157, 2h.159: a refused Cache RAM is described by the unit first and then its error', async () => {
  const { bff, main } = await form()
  type(box(main, RAM), '1.5')
  fireEvent.click(saveButton(main))
  await within(main).findByText(en('save-invalid'))
  const el = box(main, RAM)
  expectError(el, en('field-error-whole-number'), RAM)
  expect(describedIds(el).slice(0, 2), 'unit first, then the error').toEqual([
    unitId(el),
    'model-profile-cache-ram-error',
  ])
  expect(bff.to('PUT', PUT), 'nothing sent').toHaveLength(0)
})
