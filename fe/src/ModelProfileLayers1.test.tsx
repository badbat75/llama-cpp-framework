// Brief 19, sub-slice LS2a (fe/briefs/19-layer-sliders.md), criteria 19 G.13,
// 19 G.14 and 19 G.15: the range input beside the two layer number fields (its
// name, range, value and `aria-valuetext`, the text it reads and writes), MoE
// CPU layers becoming editable with its state by the model file's facts
// (pending, experts, a dense model, a reason, a failed read) and its reason
// paragraphs, the note `model-profile-moe-later` moved to MoE cache, the
// 0 to 4096 check of the field, and its save (changed, the ticked box, the 422
// `not-whole-number-in-range`). 19 G.1 and G.2 are in ModelProfileGpu5, 19 G.3
// in gpuBlocks.test.ts, 19 G.4 to G.12 in ModelProfileGpu5, 6 and 7; 19 G.16 to
// G.18 (the device lists and their refusal) are the next slice. The helpers are
// those of ModelProfileGpu6.test.tsx and ModelProfileSave13.test.tsx, repeated
// here (a test file imports no other test file).
import {
  cleanup,
  fireEvent,
  render,
  screen,
  within,
  waitFor,
} from '@testing-library/react'
import { afterEach, beforeEach, expect, test } from 'vitest'
import App from './App.tsx'
import {
  before,
  DEVICES,
  en,
  fakeBff,
  held,
  isHidden,
  MODEL_FILE,
  modelProfileDoc,
  reply,
  startAt,
  type BffOptions,
  type ModelInfo,
} from './test/bff.ts'

const ID = 'Qwen3-27B'
const HASH = `#/profiles/model/${ID}`
const PUT = `/api/profiles/model/${ID}`
const CARD = en('model-profile-hardware-config')
const GPU_LAYERS = en('model-profile-gpu-layers')
const MOE = en('model-profile-moe-cpu-layers')
const MOE_CACHE = en('model-profile-moe-cache')
const FFN = en('model-profile-dense-ffn-cpu-layers')
const DRAFT_LAYERS = en('model-profile-draft-gpu-layers')
const DEFAULT = en('field-default')
const SAVE = en('save-button')
const DISCARD = en('save-discard')
/** 19 G.14: the two reason paragraphs, and the note that moved. */
const READING = en('model-profile-moe-reading')
const DENSE = en('model-profile-moe-dense')
const LATER = en('model-profile-moe-later')
/** 19 G.14: the key joins the fe's integer check as 0 to 4096. */
const RANGE = en('field-error-number', '0', '4096')
const BLOCKS = 33
const EXPERTS = 128
const USED = 8
/** The id of each field's row (Field.tsx): `<id>-control` is its text input,
 * `<id>-label` the legend that names the slider (19 G.13). */
const GPU_ID = 'model-profile-n-gpu-layers'
const MOE_ID = 'model-profile-n-cpu-moe'

/** What the read gives: the two layer counts, MoE cache, Dense FFN CPU layers,
 * and a draft pair so the draft row is enabled (19 G.19: it keeps no slider). */
const AS_LOADED: Record<string, string> = {
  model: MODEL_FILE,
  'n-gpu-layers': '29',
  'n-cpu-moe': '20',
  'moe-cache-mib': '2048',
  'n-cpu-ffn': '4',
  'model-draft': 'Qwen3-27B-DFlash-Q8_0.gguf',
  'n-gpu-layers-draft': '99',
}

/** `AS_LOADED` without the named keys: the file does not hold them. */
function without(...keys: string[]): Record<string, string> {
  const out = { ...AS_LOADED }
  for (const k of keys) delete out[k]
  return out
}

/** The model file's facts, as the read answers them: 33 blocks, experts. */
function facts(moe = true): ModelInfo {
  return {
    model: MODEL_FILE,
    info: {
      blockCount: BLOCKS,
      moe,
      expertCount: EXPERTS,
      expertUsed: USED,
    },
    reason: null,
  }
}

/** The state of the facts read, named by the line the GPU card shows (19 G.2):
 * the tests that expect no slider wait for this line, so they miss a slider
 * that arrives later. */
type Facts = 'moe' | 'dense' | 'reason' | 'failed' | 'reading'

function factsLine(kind: Facts): string {
  if (kind === 'moe')
    return en(
      'model-profile-model-facts-moe',
      `${BLOCKS}`,
      `${EXPERTS}`,
      `${USED}`,
    )
  if (kind === 'dense') return en('model-profile-model-facts', `${BLOCKS}`)
  if (kind === 'failed') return en('model-profile-model-facts-failed')
  if (kind === 'reading') return en('model-profile-model-facts-reading')
  return en(
    'model-profile-model-facts-none',
    en('api-model-info-missing', MODEL_FILE),
  )
}

beforeEach(() => {
  startAt(HASH)
})

afterEach(() => {
  cleanup()
  startAt('')
})

// ── Helpers ───────────────────────────────────────────────────────────────

/** Waits for the page read with the file `values` and the model facts
 * `facts()` (33 blocks, experts), unless `options` says otherwise. A save
 * answers 200 and applies the sent values to the next read, as the bff does. */
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

/** Waits until this page's facts read has landed, showing `kind`'s line. */
async function settled(main: HTMLElement, kind: Facts): Promise<void> {
  await within(main).findByText(factsLine(kind))
}

/** The field whose label is `label`: the group named by the legend that also
 * names its slider (19 G.13). */
function field(main: HTMLElement, label: string): HTMLElement {
  return within(main).getByRole('group', { name: label })
}

/** The field's text input, named by the legend. */
function input(main: HTMLElement, label: string): HTMLInputElement {
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

/** The field's slider, named by the legend (19 G.13), where one is expected. */
function slider(main: HTMLElement, label: string): HTMLInputElement {
  const found = within(field(main, label)).queryByRole('slider', {
    name: label,
  })
  expect(
    found,
    `19 G.13: the ${label} slider: role slider named "${label}"`,
  ).not.toBeNull()
  return found as HTMLInputElement
}

/** No slider in the field (19 G.13, 19 G.14). */
function expectNoSlider(main: HTMLElement, label: string): void {
  expect(
    within(field(main, label)).queryByRole('slider', { name: label }),
    `19 G.13: ${label} has no slider`,
  ).toBeNull()
}

/** What 19 G.13 pins of a slider: an `input type="range"` with `min` 0, `max`
 * the block count, `step` 1, the value, `aria-valuetext`
 * `model-profile-layers-value`, the legend's name, in the field's control row
 * after the text input. */
function expectSlider(
  main: HTMLElement,
  label: string,
  id: string,
  value: number,
): HTMLInputElement {
  const s = slider(main, label)
  expect(s.type, `${label}: an input type="range"`).toBe('range')
  expect(s.getAttribute('min'), `${label}: min 0`).toBe('0')
  expect(s.getAttribute('max'), `${label}: max the block count`).toBe(
    `${BLOCKS}`,
  )
  expect(s.getAttribute('step'), `${label}: step 1`).toBe('1')
  expect(s.value, `${label}: the slider's value`).toBe(`${value}`)
  expect(
    s.getAttribute('aria-valuetext'),
    `${label}: aria-valuetext "value of block count"`,
  ).toBe(en('model-profile-layers-value', `${value}`, `${BLOCKS}`))
  expect(
    s.getAttribute('aria-labelledby'),
    `${label}: named by the field's legend`,
  ).toBe(`${id}-label`)
  const box = input(main, label)
  expect(
    s.closest('.field-control'),
    `${label}: the slider is in the field's control row`,
  ).toBe(box.closest('.field-control'))
  expect(before(box, s), `${label}: the slider sits after the text input`).toBe(
    true,
  )
  return s
}

/** The card's note paragraphs (the note rows of 2h.7), in reading order. */
function notes(main: HTMLElement): HTMLElement[] {
  const h3 = within(main).getByRole('heading', { level: 3, name: CARD })
  const card = h3.closest('section')
  expect(card, `${CARD}: the card`).not.toBeNull()
  return [...card!.querySelectorAll<HTMLElement>('p.note')]
}

/** The note `text`, shown once, right before the field `label` (19 G.14). */
function expectNoteBefore(
  main: HTMLElement,
  text: string,
  label: string,
): HTMLElement {
  const all = notes(main).filter((p) => (p.textContent ?? '') === text)
  expect(all, `19 G.14: the note ${text} shows once`).toHaveLength(1)
  const note = all[0]!
  const f = field(main, label)
  expect(
    note.nextElementSibling,
    `19 G.14: ${text} is right before ${label}: the element after it is ${
      note.nextElementSibling?.textContent ?? '(nothing)'
    }`,
  ).toBe(f)
  expect(isHidden(note), `${text}: shown`).toBe(false)
  return note
}

/** Clicks Save changes and waits for the n-th PUT of this model profile. */
async function save(
  main: HTMLElement,
  bff: ReturnType<typeof fakeBff>,
  n = 1,
): Promise<void> {
  fireEvent.click(within(main).getByRole('button', { name: SAVE }))
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

/** Types into a control, as a keystroke does: one change event. */
function type(el: HTMLElement, value: string): void {
  fireEvent.change(el, { target: { value } })
}

/** The text each id of `aria-describedby` gives. */
function described(el: HTMLElement): string[] {
  return (el.getAttribute('aria-describedby') ?? '')
    .split(/\s+/)
    .filter(Boolean)
    .map((id) => document.getElementById(id)?.textContent ?? `(no #${id})`)
}

/** Asserts `el` is marked invalid with the error `text` under it (02h 142). */
function expectError(el: HTMLElement, text: string, what: string): void {
  expect(el.getAttribute('aria-invalid'), `${what}: aria-invalid`).toBe('true')
  expect(described(el), `${what}: aria-describedby`).toContain(text)
  const error = describedIds(el)
    .map((id) => document.getElementById(id))
    .find((e) => e !== null && e.textContent === text)
  expect(error, `${what}: the error is shown`).toBeDefined()
  expect(isHidden(error!), `${what}: the error is shown`).toBe(false)
  expect(before(el, error!), `${what}: the error comes after the control`).toBe(
    true,
  )
}

function describedIds(el: HTMLElement): string[] {
  return (el.getAttribute('aria-describedby') ?? '')
    .split(/\s+/)
    .filter(Boolean)
}

// ── 19 G.13: the range beside a number field ──────────────────────────────

test('19 G.13: with the facts read, GPU layers and MoE CPU layers each carry a range input after their text input, in the control row, named by the field legend, min 0, max the block count, step 1, aria-valuetext model-profile-layers-value', async () => {
  const main = (await form()).main
  await settled(main, 'moe')
  expectSlider(main, GPU_LAYERS, GPU_ID, 29)
  expectSlider(main, MOE, MOE_ID, 20)
})

test.each([
  ['29', 29],
  ['0', 0],
  ['99', BLOCKS],
  ['auto', BLOCKS],
  ['all', BLOCKS],
  ['-5', BLOCKS],
] as [string, number][])(
  '19 G.13: the GPU layers slider reads %s as %i: a whole number clamped to 0..blockCount, and auto, all and a negative number as the block count',
  async (text, value) => {
    const main = (await form({ ...AS_LOADED, 'n-gpu-layers': text })).main
    await settled(main, 'moe')
    expect(input(main, GPU_LAYERS).value, `${text}: the text shows`).toBe(text)
    expectSlider(main, GPU_LAYERS, GPU_ID, value)
  },
)

test.each([
  ['20', 20],
  ['0', 0],
  ['99', BLOCKS],
  ['-3', 0],
] as [string, number][])(
  '19 G.13: the MoE CPU layers slider reads %s as %i: the same clamp, a negative number to 0 (no words: 19 G.14)',
  async (text, value) => {
    const main = (await form({ ...AS_LOADED, 'n-cpu-moe': text })).main
    await settled(main, 'moe')
    expect(input(main, MOE).value, `${text}: the text shows`).toBe(text)
    expectSlider(main, MOE, MOE_ID, value)
  },
)

test('19 G.13: a ticked default box is the block count for GPU layers and 0 for MoE CPU layers, and disables the slider with the input; unticking it enables both on the text the file has, and a key the file lacks starts ticked', async () => {
  const main = (await form()).main
  await settled(main, 'moe')
  for (const [label, id, keyed, ticked] of [
    [GPU_LAYERS, GPU_ID, '29', BLOCKS],
    [MOE, MOE_ID, '20', 0],
  ] as [string, string, string, number][]) {
    const box = checkBox(main, label)
    expect(box.checked, `${label}: the file has the key: unticked`).toBe(false)
    expect(slider(main, label).disabled, `${label}: slider enabled`).toBe(false)
    fireEvent.click(box)
    const s = expectSlider(main, label, id, ticked)
    expect(s.disabled, `${label}: a ticked box disables the slider`).toBe(true)
    expect(input(main, label).disabled, `${label}: the input too`).toBe(true)
    fireEvent.click(box)
    expectSlider(main, label, id, Number(keyed))
    expect(slider(main, label).disabled, `${label}: enabled again`).toBe(false)
  }
  // A key the file lacks: its box is ticked at load, so the slider sits at the
  // default it draws from, disabled.
  cleanup()
  const second = (await form(without('n-gpu-layers', 'n-cpu-moe'))).main
  await settled(second, 'moe')
  expect(checkBox(second, GPU_LAYERS).checked, 'GPU layers: box ticked').toBe(
    true,
  )
  expectSlider(second, GPU_LAYERS, GPU_ID, BLOCKS)
  expect(slider(second, GPU_LAYERS).disabled, 'GPU layers: disabled').toBe(true)
  expect(checkBox(second, MOE).checked, 'MoE: box ticked').toBe(true)
  expectSlider(second, MOE, MOE_ID, 0)
  expect(slider(second, MOE).disabled, 'MoE: disabled').toBe(true)
})

test('19 G.13: text that is no number leaves the slider where it was: at first the block count for GPU layers and 0 for MoE CPU layers, and after a keystroke the value that keystroke set', async () => {
  const main = (
    await form({ ...AS_LOADED, 'n-gpu-layers': 'x', 'n-cpu-moe': 'x' })
  ).main
  await settled(main, 'moe')
  expectSlider(main, GPU_LAYERS, GPU_ID, BLOCKS)
  expectSlider(main, MOE, MOE_ID, 0)
  // Typed, then nonsense: the slider keeps the last number it read.
  type(input(main, GPU_LAYERS), '7')
  expectSlider(main, GPU_LAYERS, GPU_ID, 7)
  type(input(main, GPU_LAYERS), 'x7')
  expectSlider(main, GPU_LAYERS, GPU_ID, 7)
  type(input(main, MOE), '9')
  expectSlider(main, MOE, MOE_ID, 9)
  type(input(main, MOE), '')
  expectSlider(main, MOE, MOE_ID, 9)
})

/** The number each field's file holds in the two tests below: a position away
 * from the one a word, a negative number and the field's first position draw,
 * so a slider that never moved off where it started cannot pass them. */
const GPU_TEXT = '7'
const MOE_TEXT = '9'

test.each([
  ['all', BLOCKS],
  ['auto', BLOCKS],
  [' all ', BLOCKS],
  ['+5', 5],
  ['-1', BLOCKS],
] as [string, number][])(
  '19 G.13: the GPU layers slider, moved to 7 first, reads %s as %i: the words (trimmed, as the key reads them) and a negative number are the block count, a signed number is the number',
  async (text, value) => {
    const main = (await form({ ...AS_LOADED, 'n-gpu-layers': GPU_TEXT })).main
    await settled(main, 'moe')
    // The slider starts on the file's number, so the position read below is a
    // move and not the place the slider sits before it has read anything.
    expectSlider(main, GPU_LAYERS, GPU_ID, Number(GPU_TEXT))
    type(input(main, GPU_LAYERS), text)
    expect(input(main, GPU_LAYERS).value, `${text}: the text shows`).toBe(text)
    expectSlider(main, GPU_LAYERS, GPU_ID, value)
  },
)

test.each([
  ['-1', 0],
  ['+5', 5],
  ['all', Number(MOE_TEXT)],
  ['auto', Number(MOE_TEXT)],
] as [string, number][])(
  '19 G.13, 19 G.14: the MoE CPU layers slider, moved to 9 first, reads %s as %i: a negative number to 0 (its clamp has no words to lift it to the top), a signed number is the number, and the GPU layers words are no number here so the slider stays where it was',
  async (text, value) => {
    const main = (await form({ ...AS_LOADED, 'n-cpu-moe': MOE_TEXT })).main
    await settled(main, 'moe')
    expectSlider(main, MOE, MOE_ID, Number(MOE_TEXT))
    type(input(main, MOE), text)
    expect(input(main, MOE).value, `${text}: the text shows`).toBe(text)
    expectSlider(main, MOE, MOE_ID, value)
  },
)

test('19 G.13: typing updates the slider at each keystroke, and moving the slider writes its value as the text: an edit like typing, kept by the save bar and put back by Discard', async () => {
  const main = (await form()).main
  await settled(main, 'moe')
  for (const [text, value] of [
    ['1', 1],
    ['12', 12],
    ['123', BLOCKS],
  ] as [string, number][]) {
    type(input(main, GPU_LAYERS), text)
    expectSlider(main, GPU_LAYERS, GPU_ID, value)
  }
  // Moving the slider writes the text, and is an edit like typing.
  const s = slider(main, MOE)
  type(s, '7')
  expect(
    input(main, MOE).value,
    'the slider writes its value as the text',
  ).toBe('7')
  expectSlider(main, MOE, MOE_ID, 7)
  expect(
    within(main).getByText(en('save-unsaved')),
    '19 G.13: moving the slider is an unsaved change',
  ).toBeTruthy()
  fireEvent.click(within(main).getByRole('button', { name: DISCARD }))
  await waitFor(() =>
    expect(input(main, MOE).value, 'Discard puts the file value back').toBe(
      '20',
    ),
  )
  expectSlider(main, MOE, MOE_ID, 20)
})

test('19 G.13, 19 G.14: while the facts read is pending, failed, or answers a reason, neither layer field has a slider', async () => {
  // Pending: the facts line says it is reading.
  const h = held()
  const pending = (await form({ ...AS_LOADED }, { modelInfo: h.answer })).main
  await settled(pending, 'reading')
  expectNoSlider(pending, GPU_LAYERS)
  expectNoSlider(pending, MOE)
  // Failed: no answer.
  cleanup()
  const failed = (await form({ ...AS_LOADED }, { modelInfo: reply(500) })).main
  await settled(failed, 'failed')
  expectNoSlider(failed, GPU_LAYERS)
  expectNoSlider(failed, MOE)
  // A reason: the bff fake's default, the model file is not there.
  cleanup()
  const reasoned = (await form({ ...AS_LOADED }, { modelInfo: undefined })).main
  await settled(reasoned, 'reason')
  expectNoSlider(reasoned, GPU_LAYERS)
  expectNoSlider(reasoned, MOE)
})

test("19 G.19: Draft GPU layers keeps no slider once the facts are read: the draft model file's facts are not read", async () => {
  const main = (await form()).main
  await settled(main, 'moe')
  expect(
    input(main, DRAFT_LAYERS).disabled,
    'this read has a Draft model file: the row is enabled',
  ).toBe(false)
  expectNoSlider(main, DRAFT_LAYERS)
})

// ── 19 G.14: MoE CPU layers by the facts ──────────────────────────────────

test('19 G.14: while the facts read is pending, MoE CPU layers is disabled, box and input, with model-profile-moe-reading right before it and no slider; the answer opens it and takes the paragraph away', async () => {
  const h = held()
  const main = (await form({ ...AS_LOADED }, { modelInfo: h.answer })).main
  await settled(main, 'reading')
  expectNoteBefore(main, READING, MOE)
  const box = input(main, MOE)
  expect(box.disabled, 'pending: the input is disabled').toBe(true)
  expect(box.value, 'pending: the file value shows').toBe('20')
  expect(checkBox(main, MOE).disabled, 'pending: the box is disabled').toBe(
    true,
  )
  expectNoSlider(main, MOE)
  // The experts answer lands: the field opens.
  h.resolve(reply(200, facts()))
  await waitFor(() =>
    expect(
      input(main, MOE).disabled,
      'the read answered: the field opens',
    ).toBe(false),
  )
  expect(
    within(main).queryByText(READING),
    'the reason paragraph goes away with the read',
  ).toBeNull()
  expectSlider(main, MOE, MOE_ID, 20)
})

test('19 G.14: with the model having experts, MoE CPU layers is enabled, box and input, with the slider of 19 G.13 and no reason paragraph; a dense model leaves GPU layers its slider', async () => {
  const main = (await form()).main
  await settled(main, 'moe')
  expect(checkBox(main, MOE).disabled, 'experts: the box is enabled').toBe(
    false,
  )
  expect(input(main, MOE).disabled, 'experts: the input is enabled').toBe(false)
  expectSlider(main, MOE, MOE_ID, 20)
  for (const text of [READING, DENSE])
    expect(
      within(main).queryByText(text),
      `19 G.14: no ${text} paragraph`,
    ).toBeNull()
  cleanup()
  const dense = (await form({ ...AS_LOADED }, { modelInfo: facts(false) })).main
  await settled(dense, 'dense')
  expectSlider(dense, GPU_LAYERS, GPU_ID, 29)
})

test('19 G.14: on a dense model MoE CPU layers is disabled, box and input, with model-profile-moe-dense right before it and no slider: the file value shows, is kept, and is never sent', async () => {
  const { bff, main } = await form(
    { ...AS_LOADED },
    { modelInfo: facts(false) },
  )
  await settled(main, 'dense')
  expectNoteBefore(main, DENSE, MOE)
  const box = input(main, MOE)
  expect(box.disabled, 'dense: the input is disabled').toBe(true)
  expect(box.value, 'dense: the file value shows').toBe('20')
  expect(checkBox(main, MOE).disabled, 'dense: the box is disabled').toBe(true)
  expectNoSlider(main, MOE)
  // The locked value is unchanged: a save of another field never sends it.
  type(input(main, FFN), '6')
  await save(main, bff)
  expect(sentValues(bff)['n-cpu-ffn'], 'the edit is sent').toBe('6')
  expect(
    sentValues(bff),
    '19 G.14: a dense model keeps n-cpu-moe out of the save',
  ).not.toHaveProperty('n-cpu-moe')
  expect(input(main, MOE).value, 'the value stays shown').toBe('20')
})

test('19 G.14: a reason and a failed read leave MoE CPU layers enabled, box and input, with no slider and no reason paragraph; its value is checked as 0 to 4096', async () => {
  // The bff fake's default: the model file is not there.
  const reasoned = (await form({ ...AS_LOADED }, { modelInfo: undefined })).main
  await settled(reasoned, 'reason')
  expect(checkBox(reasoned, MOE).disabled, 'a reason: the box is enabled').toBe(
    false,
  )
  expect(input(reasoned, MOE).disabled, 'a reason: the input is enabled').toBe(
    false,
  )
  expect(input(reasoned, MOE).value, 'a reason: the file value shows').toBe(
    '20',
  )
  expectNoSlider(reasoned, MOE)
  for (const text of [READING, DENSE])
    expect(
      within(reasoned).queryByText(text),
      `19 G.14: a reason shows no ${text} paragraph`,
    ).toBeNull()
  cleanup()
  const failed = (await form({ ...AS_LOADED }, { modelInfo: reply(500) })).main
  await settled(failed, 'failed')
  expect(
    checkBox(failed, MOE).disabled,
    'a failed read: the box is enabled',
  ).toBe(false)
  expect(
    input(failed, MOE).disabled,
    'a failed read: the input is enabled',
  ).toBe(false)
  expectNoSlider(failed, MOE)
  // A field that reads enabled is an editable field, so its save is checked as
  // 19 G.14 says: 0 to 4096, field-error-number.
  type(input(failed, MOE), '5000')
  fireEvent.click(within(failed).getByRole('button', { name: SAVE }))
  await within(failed).findByText(en('save-invalid'))
  expectError(input(failed, MOE), RANGE, 'a failed read: out of range')
})

test('19 G.14: the note model-profile-moe-later sits right before MoE cache, which stays disabled, and nothing sits between GPU layers and MoE CPU layers', async () => {
  const main = (await form()).main
  await settled(main, 'moe')
  expectNoteBefore(main, LATER, MOE_CACHE)
  expect(
    input(main, MOE_CACHE).disabled,
    'MoE cache stays disabled (19 G.14)',
  ).toBe(true)
  expect(checkBox(main, MOE_CACHE).disabled, 'MoE cache box disabled').toBe(
    true,
  )
  // MoE CPU layers is the row right after GPU layers: no note between them.
  expect(
    field(main, MOE).previousElementSibling,
    '19 G.14: MoE CPU layers follows GPU layers',
  ).toBe(field(main, GPU_LAYERS))
  expect(
    notes(main).map((p) => p.textContent),
    '19 G.14: MoE cache is the only note of the card this read leaves',
  ).toEqual([LATER])
})

// ── 19 G.15: saving MoE CPU layers ────────────────────────────────────────

test('19 G.15: MoE CPU layers changed is sent as the number, and a ticked box sends null', async () => {
  const first = await form()
  await settled(first.main, 'moe')
  type(input(first.main, MOE), '7')
  await save(first.main, first.bff)
  expect(sentValues(first.bff)['n-cpu-moe'], 'the edit goes out').toBe('7')
  cleanup()
  const second = await form()
  await settled(second.main, 'moe')
  fireEvent.click(checkBox(second.main, MOE))
  expectSlider(second.main, MOE, MOE_ID, 0)
  await save(second.main, second.bff)
  expect(
    sentValues(second.bff)['n-cpu-moe'],
    'a ticked box sends null',
  ).toBeNull()
})

test('19 G.14, 19 G.15: 4096 is sent, and 5000 is refused by the field with field-error-number 0 to 4096, marked, focused, sending nothing', async () => {
  const { bff, main } = await form()
  await settled(main, 'moe')
  type(input(main, MOE), '4096')
  await save(main, bff)
  expect(sentValues(bff)['n-cpu-moe'], 'the maximum goes out').toBe('4096')
  // The saved value is the file's now: 19 G.15, changed by its state.
  expect(input(main, MOE).value, 'the save shows the value it sent').toBe(
    '4096',
  )
  type(input(main, MOE), '5000')
  within(main).getByRole('button', { name: SAVE }).focus()
  fireEvent.click(within(main).getByRole('button', { name: SAVE }))
  await within(main).findByText(en('save-invalid'))
  const el = input(main, MOE)
  expectError(el, RANGE, '5000 out of 0 to 4096')
  expect(document.activeElement, 'focus on the marked input').toBe(el)
  expect(bff.to('PUT', PUT), 'the refused edit sends nothing').toHaveLength(1)
})

test('19 G.15: a 422 not-whole-number-in-range naming n-cpu-moe puts api-not-whole-number-in-range on the field, marks it aria-invalid, focuses it, and the bar answers save-refused', async () => {
  const body = {
    key: 'not-whole-number-in-range',
    args: ['n-cpu-moe', '9999', '0', '4096'],
    field: 'n-cpu-moe',
  }
  // The typed value is one the fe's own 0 to 4096 check (19 G.14) accepts:
  // 19 G.15 pins how the page maps the manager's 422, so the save must reach
  // the wire, and the key is in the file so its box is unticked and its input
  // enabled. The refusal's args are the fake's canned data (02h 142): the
  // field's text is built from them, whatever went out.
  const { bff, main } = await form(
    { ...AS_LOADED },
    { modelProfileSave: () => reply(422, body) },
  )
  await settled(main, 'moe')
  within(main).getByRole('button', { name: SAVE }).focus()
  type(input(main, MOE), '4096')
  await save(main, bff)
  const text = en(
    'api-not-whole-number-in-range',
    'n-cpu-moe',
    '9999',
    '0',
    '4096',
  )
  await within(main).findByText(en('save-refused', text))
  const el = input(main, MOE)
  expectError(el, text, 'the refused MoE CPU layers')
  expect(document.activeElement, 'focus on the field').toBe(el)
})
