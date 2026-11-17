// The Hardware card editable, the second part of sub-slice 2h.7 of
// fe/briefs/02h-preset-save.md (175, 177, 178, 179, 180, 181, 182): the seven
// always rows (the two layer counts behind a default box with 157's whole
// numbers and their exact `auto` / `all`, the tri-state flags with 124's
// written-as line, the off / on flag, the two word selects matched exactly),
// the draft pair behind `model-draft` with its note, MoE CPU layers and MoE
// cache always disabled with their note, their values kept and never sent and
// never checked, the Fit trio still read-only with its full-value lines (134),
// the load-mode exact match (180), the two notes as plain paragraphs (181)
// and their exact strings (182). The card's read-only look is 2e's 107, in
// PresetCards4.test.tsx; the helpers are those of PresetSave12.test.tsx,
// which this file repeats (a test file imports no other test file).
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from '@testing-library/react'
import { afterEach, beforeEach, expect, it, test, vi } from 'vitest'
import App from './App.tsx'
import {
  before,
  en,
  fakeBff,
  isHidden,
  presetDoc,
  reply,
  startAt,
  type BffOptions,
} from './test/bff.ts'

const ID = 'Qwen3-27B'
const HASH = `#/models/${ID}`
const PUT = `/api/presets/${ID}`
const HEADING = en('preset-hardware-config')
const NEXT = en('preset-resource-context')
const GPU_LAYERS = en('preset-gpu-layers')
const MOE = en('preset-moe-cpu-layers')
const MOE_CACHE = en('preset-moe-cache')
const FFN = en('preset-dense-ffn-cpu-layers')
const OP = en('preset-op-offload')
const REPACK = en('preset-repack')
const NO_HOST = en('preset-no-host-buffer')
const LAZY = en('preset-lazy-tensors')
const LOAD = en('preset-load-mode')
const FIT_ON = en('preset-fit-allowed')
const FIT_CTX = en('preset-fit-min-context')
const FIT_TARGET = en('preset-fit-free-vram')
const DRAFT_DEV = en('preset-draft-device')
const DRAFT_LAYERS = en('preset-draft-gpu-layers')
/** The text of Draft device's empty option (2e's 103, 107; follows-model is
 *  Vision GPU's). */
const SAME = en('preset-device-same-as-model')
const DEFAULT = en('field-default')
/** 157's last row: the layer counts are checked as whole numbers. */
const WHOLE = en('field-error-whole-number')
/** 182: the brief's exact text of the two new strings, as literals. */
const DRAFT_OFF_TEXT =
  'Draft device and Draft GPU layers apply only to a Draft model file, and this preset has none (Assets).'
const MOE_LATER_TEXT =
  'MoE CPU layers and MoE cache stay as the file has them: this page cannot yet read whether the model has experts. Change them in pai-config on the server machine.'
/** The number and the text past the full-value line's limits (119). */
const NUM_9 = '172032000'
const TEXT_21 = '{"MixedCaseX":TRUE}  '

/** What the read gives: every key of the card present, each a value the
 * configurator itself would write (so no written-as line). */
const AS_LOADED: Record<string, string> = {
  'n-gpu-layers': '99',
  'n-cpu-ffn': '4',
  'op-offload': 'true',
  repack: 'false',
  'no-host': 'true',
  'lazy-mode': 'auto',
  'load-mode': 'MLOCK',
  fit: 'on',
  'fit-ctx': '4096',
  'fit-target': '1024',
  'model-draft': 'Qwen3-27B-DFlash-Q8_0.gguf',
  'device-draft': 'ROCm1',
  'n-gpu-layers-draft': '99',
}

/** `AS_LOADED` without the named keys: the file does not hold them. */
function without(...keys: string[]): Record<string, string> {
  const out = { ...AS_LOADED }
  for (const k of keys) delete out[k]
  return out
}

beforeEach(() => {
  startAt(HASH)
})

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
  startAt('')
})

// ── Helpers (as PresetSave12.test.tsx) ─────────────────────────────────

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

function saveButton(main: HTMLElement): HTMLElement {
  return within(main).getByRole('button', { name: en('save-button') })
}

function discardButton(main: HTMLElement): HTMLElement {
  return within(main).getByRole('button', { name: en('save-discard') })
}

/** The save bar's buttons are inert: the form is clean. */
function expectInert(main: HTMLElement, what: string) {
  for (const b of [saveButton(main), discardButton(main)])
    expect(b.getAttribute('aria-disabled'), `${what}: the buttons inert`).toBe(
      'true',
    )
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

/** The option values of a select, in the order they are listed. */
function optionValues(select: HTMLSelectElement): string[] {
  return Array.from(select.options).map((o) => o.value)
}

/** The texts of a select's options, in order. */
function optionTexts(select: HTMLSelectElement): string[] {
  return Array.from(select.options).map((o) => o.text)
}

/** The text of a select's selected option. */
function selectedText(select: HTMLSelectElement): string {
  return select.options[select.selectedIndex]?.text ?? '(none selected)'
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

// ── 175, 156, 158: the two layer counts, always enabled ───────────────

test('2h.175, 2h.156, 2h.158: GPU layers and Dense FFN CPU layers are enabled, each box unticked with its input on the file text, no input mode, no full-value line under the enabled input; an absent key starts with its box ticked and its input disabled and empty', async () => {
  const { main } = await form({ ...AS_LOADED, 'n-gpu-layers': NUM_9 })
  for (const [label, key] of [
    [GPU_LAYERS, 'n-gpu-layers'],
    [FFN, 'n-cpu-ffn'],
  ] as [string, string][]) {
    const c = checkBox(main, label)
    expect(c.disabled, `${key}: the box is enabled (175)`).toBe(false)
    expect(c.checked, `${key}: a key the file has: box unticked`).toBe(false)
    const el = box(main, label)
    expect(
      el.disabled,
      `${key}: the input follows the unticked box (156)`,
    ).toBe(false)
    expect(el.id, `${key}: the field id`).toBe(`preset-${key}-control`)
    expect(el.getAttribute('inputmode'), `${key}: no input mode (158)`).toBe(
      null,
    )
  }
  expect(box(main, GPU_LAYERS).value, 'the input holds the file text').toBe(
    NUM_9,
  )
  expect(
    document.getElementById('preset-full-n-gpu-layers'),
    '134: an enabled input shows no full-value line',
  ).toBeNull()
  cleanup()
  const second = await form(without('n-gpu-layers', 'n-cpu-ffn'))
  for (const [label, key] of [
    [GPU_LAYERS, 'n-gpu-layers'],
    [FFN, 'n-cpu-ffn'],
  ] as [string, string][]) {
    expect(
      checkBox(second.main, label).checked,
      `${key}: an absent key ticks the box (156)`,
    ).toBe(true)
    const el = box(second.main, label)
    expect(el.disabled, `${key}: a ticked box disables the input (156)`).toBe(
      true,
    )
    expect(el.value, `${key}: a ticked box empties the input`).toBe('')
  }
})

/** The card's two always-enabled integer fields (2e's 107, 157's last row). */
const LAYERS = [
  { key: 'n-gpu-layers', label: GPU_LAYERS },
  { key: 'n-cpu-ffn', label: FFN },
]

test.each(LAYERS)(
  '2h.175, 2h.133, 2h.157: $key sends "+0172032" as "172032", and a ticked box sends null for a key the file had',
  async (c) => {
    const { bff, main } = await form()
    type(box(main, c.label), '+0172032')
    await save(main, bff)
    expect(sentValues(bff)[c.key], '+0172032 sends 172032').toBe('172032')
    cleanup()
    const second = await form()
    toggle(second.main, c.label)
    await save(second.main, second.bff)
    expect(sentValues(second.bff)[c.key], 'the key the file had, removed').toBe(
      null,
    )
  },
)

test('2h.157: GPU layers takes the exact texts auto and all and sends them as written', async () => {
  const first = await form()
  type(box(first.main, GPU_LAYERS), 'auto')
  await save(first.main, first.bff)
  expect(sentValues(first.bff)['n-gpu-layers'], 'auto is sent as written').toBe(
    'auto',
  )
  cleanup()
  const second = await form()
  type(box(second.main, GPU_LAYERS), 'all')
  await save(second.main, second.bff)
  expect(sentValues(second.bff)['n-gpu-layers'], 'all is sent as written').toBe(
    'all',
  )
})

/** 157: the two words typed with a space around them, and a plain number the
 * same way (the guard: a number is already sent trimmed). */
const GPU_LAYERS_SPACED = [
  { typed: ' auto ', sent: 'auto' },
  { typed: ' all ', sent: 'all' },
  { typed: ' 12 ', sent: '12' },
]

it.each(GPU_LAYERS_SPACED)(
  '2h.157: GPU layers typed "$typed" is accepted and the PUT sends "$sent"',
  async ({ typed, sent }) => {
    const { bff, main } = await form()
    const el = box(main, GPU_LAYERS)
    type(el, typed)
    await save(main, bff)
    expect(el.getAttribute('aria-invalid'), `"${typed}": not marked`).toBe(null)
    expect(
      within(main).queryByText(en('save-invalid')),
      `"${typed}": the save is not blocked`,
    ).toBeNull()
    expect(
      sentValues(bff)['n-gpu-layers'],
      `"${typed}": the PUT sends the trimmed "${sent}"`,
    ).toBe(sent)
  },
)

test('2h.157: Dense FFN CPU layers rejects "abc" with the whole-number error, marks it, focuses it, alerts save-invalid and sends nothing', async () => {
  const { bff, main } = await form()
  type(box(main, FFN), 'abc')
  fireEvent.click(saveButton(main))
  await within(main).findByText(en('save-invalid'))
  const el = box(main, FFN)
  expectError(el, WHOLE, FFN)
  expect(document.activeElement, 'focus on Dense FFN CPU layers').toBe(el)
  expect(bff.to('PUT', PUT), 'nothing sent').toHaveLength(0)
})

// ── 175, 133, 124: the two tri-state flags, always enabled ────────────

test.each([
  { key: 'op-offload', label: OP },
  { key: 'repack', label: REPACK },
])(
  '2h.175, 2h.133, 2h.124: $key: the file word default shows Default with its written-as line, picking On shows On and sends "true", picking Off sends "false", and from On picking Default sends null',
  async (c) => {
    const { bff, main } = await form({ ...AS_LOADED, [c.key]: 'default' })
    const sel = selectOf(main, c.label)
    expect(sel.disabled, `${c.key}: enabled (175)`).toBe(false)
    expect(sel.value, 'the file word default shows Default').toBe('default')
    expect(optionTexts(sel), 'the tri-state options of 122').toEqual([
      en('option-default'),
      en('option-on'),
      en('option-off'),
    ])
    expect(
      describedIds(sel),
      '124b: the written-as line is the select description',
    ).toContain(`preset-written-${c.key}`)
    pick(main, c.label, 'true')
    expect(selectedText(sel), 'the select shows the picked option').toBe(
      en('option-on'),
    )
    await save(main, bff)
    expect(sentValues(bff)[c.key], 'On sends "true"').toBe('true')
    cleanup()
    const second = await form({ ...AS_LOADED, [c.key]: 'default' })
    pick(second.main, c.label, 'false')
    await save(second.main, second.bff)
    expect(sentValues(second.bff)[c.key], 'Off sends "false"').toBe('false')
    cleanup()
    const third = await form({ ...AS_LOADED, [c.key]: 'true' })
    expect(selectOf(third.main, c.label).value, 'true shows On').toBe('true')
    pick(third.main, c.label, 'default')
    await save(third.main, third.bff)
    expect(sentValues(third.bff)[c.key], 'Default sends null').toBeNull()
  },
)

// ── 175, 133: the off / on flag ───────────────────────────────────────

test('2h.175, 2h.133: No host buffer shows off when absent, picking On sends "true", and from On picking Off sends null', async () => {
  const { bff, main } = await form(without('no-host'))
  const sel = selectOf(main, NO_HOST)
  expect(sel.disabled, 'enabled (175)').toBe(false)
  expect(sel.value, 'absent shows off').toBe('false')
  expect(optionTexts(sel), 'the off / on options of 122').toEqual([
    en('option-off'),
    en('option-on'),
  ])
  pick(main, NO_HOST, 'true')
  await save(main, bff)
  expect(sentValues(bff)['no-host'], 'On sends "true"').toBe('true')
  cleanup()
  const second = await form({ ...AS_LOADED, 'no-host': 'true' })
  pick(second.main, NO_HOST, 'false')
  await save(second.main, second.bff)
  expect(sentValues(second.bff)['no-host'], 'Off sends null').toBeNull()
})

// ── 175, 133: the two word selects, matched exactly ──────────────────

test('2h.175, 2h.122, 2h.124, 2h.125: No host buffer at the file word default is the on the engine acts as: On selected, no extra option, the spelling on the written-as line, and leaving it is clean', async () => {
  const { bff, main } = await form({ ...AS_LOADED, 'no-host': 'default' })
  const sel = selectOf(main, NO_HOST)
  expect(sel.disabled, 'enabled (175)').toBe(false)
  // 122's flag row: the Off words are off, disabled, false and 0, so the word
  // `default` is not one of them and llama-server passes the plain flag.
  expect(sel.value, 'the word default is not falsey, so the flag is on').toBe(
    'true',
  )
  expect(selectedText(sel), 'the control reads On').toBe(en('option-on'))
  expect(optionValues(sel), 'a flag lists no extra option (125)').toEqual([
    'false',
    'true',
  ])
  const written = document.getElementById('preset-written-no-host')
  expect(written, 'the spelling the file holds (124)').not.toBeNull()
  expect(written!.textContent, 'the spelling the file holds (124)').toBe(
    en('preset-written-as', 'default'),
  )
  expect(
    describedIds(sel),
    'the control names the written-as line (124b)',
  ).toContain('preset-written-no-host')
  // The word is a value the configurator keeps as it is, so the page is clean.
  expectInert(main, 'the file word default is the state the read gave')
  pick(main, NO_HOST, 'false')
  await save(main, bff)
  expect(sentValues(bff)['no-host'], 'Off sends null').toBeNull()
})

test('2h.175, 2h.133, 2h.107: Lazy tensors lists default, on, auto, off, matched exactly: auto selects its option and picking off sends "off"; Auto is one extra last option as written and picking Default sends null', async () => {
  const { bff, main } = await form()
  const sel = selectOf(main, LAZY)
  expect(sel.disabled, 'enabled (175)').toBe(false)
  expect(optionValues(sel), 'the lazy-mode options of 107').toEqual([
    'default',
    'on',
    'auto',
    'off',
  ])
  expect(sel.value, 'auto selects its own option').toBe('auto')
  pick(main, LAZY, 'off')
  await save(main, bff)
  expect(sentValues(bff)['lazy-mode'], 'off sends its value').toBe('off')
  cleanup()
  const second = await form({ ...AS_LOADED, 'lazy-mode': 'Auto' })
  const extra = selectOf(second.main, LAZY)
  expect(optionValues(extra), 'the four options plus Auto, last').toEqual([
    'default',
    'on',
    'auto',
    'off',
    'Auto',
  ])
  expect(extra.value, 'Auto is selected, as written').toBe('Auto')
  pick(second.main, LAZY, 'default')
  await save(second.main, second.bff)
  expect(sentValues(second.bff)['lazy-mode'], 'Default sends null').toBeNull()
})

test('2h.180, 2h.133: Load mode matches the file value exactly: MLOCK is the extra option MLOCK, listed last and selected, and picking mlock is a change that sends {"load-mode":"mlock"}', async () => {
  const { bff, main } = await form()
  const sel = selectOf(main, LOAD)
  expect(sel.disabled, 'enabled (175)').toBe(false)
  expect(optionValues(sel), 'the load-mode options of 107, then MLOCK').toEqual(
    ['default', 'auto', 'none', 'mmap', 'mlock', 'mmap+mlock', 'dio', 'MLOCK'],
  )
  expect(sel.value, 'MLOCK is selected, not mlock').toBe('MLOCK')
  expect(sel.options[7]?.text, 'the extra option is the file text').toBe(
    'MLOCK',
  )
  expect(
    document.getElementById('preset-full-load-mode'),
    '5 characters: no full-value line (119)',
  ).toBeNull()
  pick(main, LOAD, 'mlock')
  await save(main, bff)
  expect(sentValues(bff), 'picking mlock is a change').toEqual({
    'load-mode': 'mlock',
  })
})

// ── 177, 179: the draft pair behind model-draft ──────────────────────

/** The two reads 177 counts as "no draft file": the key absent, and present
 * with an empty value. */
const NO_DRAFT = [
  { name: 'absent', values: without('model-draft') },
  { name: 'empty', values: { ...AS_LOADED, 'model-draft': '' } },
]

test.each(NO_DRAFT)(
  '2h.177, 2h.182: model-draft $name: Draft device and Draft GPU layers are natively disabled and the note preset-draft-file-off sits before Draft device',
  async (c) => {
    const { main } = await form(c.values)
    expect(selectOf(main, DRAFT_DEV).disabled, 'Draft device disabled').toBe(
      true,
    )
    expect(
      checkBox(main, DRAFT_LAYERS).disabled,
      'Draft GPU layers box disabled',
    ).toBe(true)
    expect(
      box(main, DRAFT_LAYERS).disabled,
      'Draft GPU layers input disabled',
    ).toBe(true)
    const line = within(main).getByText(DRAFT_OFF_TEXT)
    expect(isHidden(line), 'the note is shown').toBe(false)
    expect(line.textContent, '182: the string as the brief words it').toBe(
      DRAFT_OFF_TEXT,
    )
    expect(before(headingOf(main, HEADING), line), 'after the card h3').toBe(
      true,
    )
    expect(
      before(line, selectOf(main, DRAFT_DEV)),
      '177: before Draft device',
    ).toBe(true)
    expect(
      before(line, headingOf(main, NEXT)),
      'inside the Hardware card',
    ).toBe(true)
  },
)

test('2h.177, 2h.179, 2h.133: model-draft present: no note, Draft device is a device select listing the empty option and the file value, both still listed after the empty one is picked, and the empty option sends null', async () => {
  const { bff, main } = await form()
  expect(within(main).queryByText(DRAFT_OFF_TEXT), 'no note (177)').toBeNull()
  const sel = selectOf(main, DRAFT_DEV)
  expect(sel.disabled, 'Draft device enabled (177)').toBe(false)
  expect(sel.id, 'the field id').toBe('preset-device-draft-control')
  expect(
    optionValues(sel),
    '103: the empty option first, then the file value only',
  ).toEqual(['', 'ROCm1'])
  expect(
    sel.options[0]?.text,
    'the empty option is the one that is the same as the model',
  ).toBe(SAME)
  expect(sel.value, 'the file value selected').toBe('ROCm1')
  const c = checkBox(main, DRAFT_LAYERS)
  expect(c.disabled, 'Draft GPU layers box enabled (177)').toBe(false)
  expect(c.checked, 'a key the file has: box unticked').toBe(false)
  const el = box(main, DRAFT_LAYERS)
  expect(el.disabled, 'Draft GPU layers input enabled').toBe(false)
  expect(el.value, 'the input holds the file text').toBe('99')
  expect(el.getAttribute('inputmode'), 'no input mode (158)').toBeNull()
  pick(main, DRAFT_DEV, '')
  expect(
    optionValues(sel),
    '179: both still listed after the empty option is picked',
  ).toEqual(['', 'ROCm1'])
  await save(main, bff)
  expect(sentValues(bff), 'the empty option sends null').toEqual({
    'device-draft': null,
  })
})

// ── 178, 182: the MoE rows, always disabled ──────────────────────────

test('2h.178, 2h.182, 2h.157: MoE CPU layers and MoE cache are always disabled with the note preset-moe-later before MoE CPU layers; a bad file value in them does not block a save, their values show and are never sent', async () => {
  const { bff, main } = await form({
    ...AS_LOADED,
    'n-cpu-moe': 'abc',
    'moe-cache-mib': 'x',
  })
  for (const label of [MOE, MOE_CACHE]) {
    expect(
      checkBox(main, label).disabled,
      `${label}: box always disabled`,
    ).toBe(true)
    expect(box(main, label).disabled, `${label}: input always disabled`).toBe(
      true,
    )
  }
  expect(box(main, MOE).value, 'the locked field keeps the file value').toBe(
    'abc',
  )
  const line = within(main).getByText(MOE_LATER_TEXT)
  expect(isHidden(line), 'the note is shown').toBe(false)
  expect(line.textContent, '182: the string as the brief words it').toBe(
    MOE_LATER_TEXT,
  )
  expect(before(headingOf(main, HEADING), line), 'after the card h3').toBe(true)
  expect(before(line, field(main, MOE)), '178: before MoE CPU layers').toBe(
    true,
  )
  expect(before(line, headingOf(main, NEXT)), 'inside the Hardware card').toBe(
    true,
  )
  pick(main, NO_HOST, 'false')
  await save(main, bff)
  expect(sentValues(bff), 'the change is sent, the MoE values never').toEqual({
    'no-host': null,
  })
  expect(
    within(main).queryByText(en('save-invalid')),
    'the bar does not complain about a field nobody can edit (157)',
  ).toBeNull()
  expect(box(main, MOE).getAttribute('aria-invalid'), 'not marked').toBe(null)
})

// ── 181: the two notes are plain paragraphs ──────────────────────────

test('2h.181: both notes are plain paragraphs: no role, no live region, not in any aria-describedby', async () => {
  const { main } = await form(without('model-draft'))
  for (const text of [MOE_LATER_TEXT, DRAFT_OFF_TEXT]) {
    const line = within(main).getByText(text)
    expect(line.getAttribute('role'), 'no role').toBeNull()
    expect(line.getAttribute('aria-live'), 'not a live region').toBeNull()
    expect(
      line.closest('[role="status"],[role="alert"],[role="log"],[aria-live]'),
      'not inside a live region',
    ).toBeNull()
  }
  for (const el of Array.from(
    main.querySelectorAll('input, select'),
  ) as HTMLElement[])
    for (const text of [MOE_LATER_TEXT, DRAFT_OFF_TEXT])
      expect(
        described(el),
        `${el.id}: the note is not its description`,
      ).not.toContain(text)
})

// ── 182: the two new strings ─────────────────────────────────────────

test('2h.182: the new strings preset-draft-file-off and preset-moe-later are the briefs sentences, word for word', () => {
  expect(en('preset-draft-file-off')).toBe(DRAFT_OFF_TEXT)
  expect(en('preset-moe-later')).toBe(MOE_LATER_TEXT)
})

// ── 175, 134: the Fit trio stays read-only ───────────────────────────

test('2h.175, 2h.134: Fit to VRAM, Fit min context and Fit free VRAM stay natively disabled, and the long values under them show the full-value line', async () => {
  const { main } = await form({
    ...without('n-gpu-layers'),
    fit: 'on',
    'fit-ctx': NUM_9,
    'fit-target': TEXT_21,
  })
  const fit = within(main).getByRole('checkbox', {
    name: FIT_ON,
  }) as HTMLInputElement
  expect(fit.disabled, 'Fit to VRAM stays disabled (2h.8)').toBe(true)
  expect(fit.checked, 'fit on shows checked').toBe(true)
  expect(
    checkBox(main, FIT_CTX).disabled,
    'Fit min context box stays disabled (2h.8)',
  ).toBe(true)
  const ctx = box(main, FIT_CTX)
  expect(ctx.disabled, 'Fit min context input stays disabled').toBe(true)
  expect(ctx.value, 'the input keeps the file text').toBe(NUM_9)
  const line = document.getElementById('preset-full-fit-ctx')
  expect(
    line,
    '134: a disabled control shows the full-value line',
  ).not.toBeNull()
  expect(
    line!.getAttribute('aria-hidden'),
    '120: the line is aria-hidden',
  ).toBe('true')
  expect(line!.textContent, 'the line holds the value as written').toBe(NUM_9)
  const target = within(main).getByRole('textbox', {
    name: FIT_TARGET,
  }) as HTMLInputElement
  expect(target.disabled, 'Fit free VRAM input stays disabled').toBe(true)
  expect(target.value, 'the input keeps the file text').toBe(TEXT_21)
  const tline = document.getElementById('preset-full-fit-target')
  expect(tline, '134: the text shows its full-value line too').not.toBeNull()
  expect(tline!.textContent, 'the line holds the value as written').toBe(
    TEXT_21,
  )
})
