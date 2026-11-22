// Brief 19, sub-slice LS2b (fe/briefs/19-layer-sliders.md), criteria 19 G.16
// to 19 G.19: the Draft device and Vision GPU selects listing the node's GPUs
// once the device probe answered (the empty option, the GPUs in probe order as
// `id: name`, the id alone when the name is null, a file value that matches no
// listed id as the extra last option, the file's spelling kept for a
// case-insensitive match, `none` for Vision GPU), the 422
// `model-profile-device-outside` marking the select of the key its `field`
// names, else `args[2]`, and the keyboard, screen-reader and strings rules of
// the layer fields and the device lists. 19 G.13 to G.15 are in
// ModelProfileLayers1, the facts read and the blocks in ModelProfileGpu5, 6, 7
// and 8. The helpers are those of ModelProfileLayers1.test.tsx and
// ModelProfileGpu7.test.tsx, repeated here (a test file imports no other test
// file). The 320 px and 200 % half of 19 G.19 is the designer's measurement
// (`fe/ux`): jsdom lays nothing out.
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from '@testing-library/react'
import { afterEach, beforeEach, expect, test } from 'vitest'
import App from './App.tsx'
import strings from './strings/en_US.json'
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
  type DeviceProbe,
  type ModelInfo,
} from './test/bff.ts'

const ID = 'Qwen3-27B'
const HASH = `#/profiles/model/${ID}`
const PUT = `/api/profiles/model/${ID}`
const CARD = en('model-profile-hardware-config')
const GPUS = en('model-profile-gpus')
const GPU_LAYERS = en('model-profile-gpu-layers')
const DRAFT_LAYERS = en('model-profile-draft-gpu-layers')
const FFN = en('model-profile-dense-ffn-cpu-layers')
const DEFAULT = en('field-default')
const ABOUT = en('field-about')
const SAVE = en('save-button')
/** 19 G.16: the Draft device select, its label and its empty option. */
const DRAFT = 'device-draft'
const DRAFT_LABEL = en('model-profile-draft-device')
const EMPTY_DRAFT = en('model-profile-device-same-as-model')
/** 19 G.17: the Vision GPU select, its card, and the switch that enables it. */
const VISION = 'mmproj-device'
const VISION_LABEL = en('model-profile-vision-gpu')
const VISION_CARD = en('model-profile-vision')
const EMPTY_VISION = en('model-profile-device-follows-model')
const NONE_TEXT = en('model-profile-device-none')
const OFFLOAD = 'mmproj-offload'
/** 19 G.19: the longest device name the criterion names. */
const LONG_NAME = 'N'.repeat(64)
const BLOCKS = 33
const EXPERTS = 128
const USED = 8

/** What the read gives: the layer counts, and a draft pair so the Draft device
 * select is enabled (its enable rule is 19 G.16's, unchanged). */
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

/** The values that open the Vision GPU select (176: an MMProj file and Vision
 * on GPU checked). */
const VISION_ON: Record<string, string> = {
  mmproj: 'mmprojs/mmproj.gguf',
  [OFFLOAD]: 'true',
}

/** The model file's facts, as the read answers them: 33 blocks, experts, so
 * the layer fields carry their sliders (19 G.19). */
function facts(): ModelInfo {
  return {
    model: MODEL_FILE,
    info: {
      blockCount: BLOCKS,
      moe: true,
      expertCount: EXPERTS,
      expertUsed: USED,
    },
    reason: null,
  }
}

/** The line the GPU card shows once the facts are read (19 G.2): the tests
 * that count a field's tab stops wait for it, so they miss a slider that
 * arrives later. */
const FACTS_MOE = en(
  'model-profile-model-facts-moe',
  `${BLOCKS}`,
  `${EXPERTS}`,
  `${USED}`,
)

beforeEach(() => {
  startAt(HASH)
})

afterEach(() => {
  cleanup()
  startAt('')
})

// ── Helpers ───────────────────────────────────────────────────────────────

/** Waits for the page read with the file `values`, the facts `facts()` and the
 * device probe `DEVICES`, unless `options` says otherwise. A save answers 200
 * and applies the sent values to the next read, as the bff does. */
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

/** The moment the as-of line shows for `probedAt` (GD1.7). */
function asOfDate(probedAt: string): string {
  return new Date(probedAt).toLocaleString(undefined, {
    dateStyle: 'medium',
    timeStyle: 'short',
  })
}

/** Waits for the device read of `probe` to have landed: with a time, the
 * as-of line is the last thing the reply writes; with no time, the line the
 * fieldset shows for it (GD1.7) is. Both are needed: the read is a fetch, so
 * a select read before its answer lands is still the pending list (the empty
 * option and the file's value), which is what 19 G.16's no-time case refuses. */
async function landed(main: HTMLElement, probe: DeviceProbe = DEVICES) {
  const fs = await within(main).findByRole('group', { name: GPUS })
  if (typeof probe.probedAt === 'string') {
    await within(fs).findByText(asOfDate(probe.probedAt))
    return
  }
  await within(fs).findByText(en('model-profile-gpu-no-probe'))
}

/** The device select of `key` (ModelProfileCards' field ids). */
function selectOf(key: string): HTMLSelectElement {
  const el = document.getElementById(`model-profile-${key}-control`)
  expect(el, `the ${key} select`).not.toBeNull()
  return el as HTMLSelectElement
}

/** The select's options as (value, text) pairs, in the order it lists them. */
function optionsOf(sel: HTMLSelectElement) {
  return [...sel.options].map((o) => ({
    value: o.value,
    text: o.textContent ?? '',
  }))
}

/** 19 G.16: one listed GPU's option: its id as the value, `id: name` as the
 * text, the id alone when the name is null. */
function gpuOption(id: string, name?: string | null) {
  return {
    value: id,
    text:
      name === null || name === undefined
        ? id
        : en('model-profile-device-option', id, name),
  }
}

/** Every GPU of `probe`, as 19 G.16 lists it. A name that is not a word
 * (GD1.7's malformed reply) reads as the id alone, as a null one does. */
function gpuOptions(probe: DeviceProbe = DEVICES) {
  return (probe.devices ?? []).map((d) =>
    gpuOption(d.id ?? '', typeof d.name === 'string' ? d.name : null),
  )
}

/** Today's options of a device select: the empty one, and the file's value as
 * its own option when the file names a device (criterion 103). */
function todayOptions(empty: string, file?: string) {
  return [
    { value: '', text: empty },
    ...(file === undefined ? [] : [{ value: file, text: file }]),
  ]
}

/** The field whose label is `label`: the group named by its legend. */
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
  expect(found, `19 G.13: the ${label} slider`).not.toBeNull()
  return found as HTMLInputElement
}

/** Every tabbable element of `main`, in document order
 * (ModelProfileGpu7.test.tsx's). */
function mainTabbable(main: HTMLElement): HTMLElement[] {
  return [
    ...main.querySelectorAll<HTMLElement>(
      'a[href], button, input, select, textarea, [tabindex]',
    ),
  ].filter(
    (el) =>
      !(el as HTMLButtonElement).disabled &&
      el.getAttribute('tabindex') !== '-1' &&
      !isHidden(el),
  )
}

/** Clicks Save changes and waits for the PUT of this model profile. */
async function save(main: HTMLElement, bff: ReturnType<typeof fakeBff>) {
  within(main).getByRole('button', { name: SAVE }).focus()
  fireEvent.click(within(main).getByRole('button', { name: SAVE }))
  await waitFor(() => expect(bff.to('PUT', PUT)).toHaveLength(1))
}

/** Types into a control, as a keystroke does: one change event. */
function type(el: HTMLElement, value: string) {
  fireEvent.change(el, { target: { value } })
}

function describedIds(el: HTMLElement): string[] {
  return (el.getAttribute('aria-describedby') ?? '')
    .split(/\s+/)
    .filter(Boolean)
}

/** Asserts `el` is marked invalid with the error `text` under it (02h 142). */
function expectError(el: HTMLElement, text: string, what: string) {
  expect(el.getAttribute('aria-invalid'), `${what}: aria-invalid`).toBe('true')
  const ids = describedIds(el)
  const texts = ids.map(
    (id) => document.getElementById(id)?.textContent ?? `(no #${id})`,
  )
  expect(texts, `${what}: aria-describedby`).toContain(text)
  const error = ids
    .map((id) => document.getElementById(id))
    .find((e) => e !== null && e.textContent === text)
  expect(error, `${what}: the error is shown`).toBeDefined()
  expect(isHidden(error!), `${what}: the error is shown`).toBe(false)
  expect(before(el, error!), `${what}: the error comes after the control`).toBe(
    true,
  )
}

/** 19 G.18: the refusal of the L4 route, `field` said or not. The args are
 * what L4 sends: [profile, instance, key, device]. */
function outside(fieldKey: string | undefined, device: string) {
  const args = [ID, 'default', fieldKey ?? DRAFT, device]
  const body: Record<string, unknown> = {
    key: 'model-profile-device-outside',
    args,
  }
  if (fieldKey !== undefined) body.field = fieldKey
  return { body, text: en('api-model-profile-device-outside', ...args) }
}

/** Dirties one enabled field of the Hardware card, so a save goes out. */
async function dirty(main: HTMLElement) {
  type(input(main, FFN), '6')
}

// ── 19 G.16: the Draft device list ────────────────────────────────────────

test('19 G.16: once the device read answered 200 with a list, the Draft device select holds the empty option model-profile-device-same-as-model, then every listed GPU in probe order, value its id, text model-profile-device-option', async () => {
  const { main } = await form()
  await landed(main)
  const sel = selectOf(DRAFT)
  expect(
    within(main).getByRole('combobox', { name: DRAFT_LABEL }),
    '19 G.16: the select is named by its label',
  ).toBe(sel)
  expect(
    optionsOf(sel),
    '19 G.16: the empty option, then the GPUs in probe order',
  ).toEqual([
    { value: '', text: EMPTY_DRAFT },
    gpuOption('CUDA0', 'NVIDIA GeForce RTX 5080'),
    gpuOption('ROCm0', 'AMD Radeon AI PRO R9700'),
    gpuOption('Vulkan0', 'Intel Arc Graphics'),
  ])
})

test('19 G.16: a listed GPU whose name is null is the option whose text is the id alone', async () => {
  const probe: DeviceProbe = {
    probedAt: DEVICES.probedAt,
    devices: (DEVICES.devices ?? []).map((d) =>
      d.id === 'CUDA0' ? { ...d, name: null } : d,
    ),
  }
  const { main } = await form({ ...AS_LOADED }, { devices: probe })
  await landed(main, probe)
  expect(
    optionsOf(selectOf(DRAFT)),
    '19 G.16: a GPU with no name is the id alone',
  ).toEqual([
    { value: '', text: EMPTY_DRAFT },
    gpuOption('CUDA0', null),
    gpuOption('ROCm0', 'AMD Radeon AI PRO R9700'),
    gpuOption('Vulkan0', 'Intel Arc Graphics'),
  ])
})

test('19 G.16: a file value matching no listed id is the last option, text model-profile-device-option-missing when the probe answered with a time', async () => {
  const { main } = await form({ ...AS_LOADED, [DRAFT]: 'CUDA9' })
  await landed(main)
  expect(optionsOf(selectOf(DRAFT)), '19 G.16: the extra last option').toEqual([
    { value: '', text: EMPTY_DRAFT },
    ...gpuOptions(),
    {
      value: 'CUDA9',
      text: en('model-profile-device-option-missing', 'CUDA9'),
    },
  ])
})

test('19 G.16: a probe that answered with no time lists an unlisted file value as the value itself, no (not detected)', async () => {
  const probe: DeviceProbe = { probedAt: null, devices: DEVICES.devices }
  const { main } = await form(
    { ...AS_LOADED, [DRAFT]: 'CUDA9' },
    { devices: probe },
  )
  await landed(main, probe)
  expect(
    optionsOf(selectOf(DRAFT)),
    '19 G.16: no time, the bare value',
  ).toEqual([
    { value: '', text: EMPTY_DRAFT },
    ...gpuOptions(probe),
    { value: 'CUDA9', text: 'CUDA9' },
  ])
})

test('19 G.16: a listed GPU matching the file value case-insensitively takes the file spelling as its value, so the untouched select is clean: the bar is inert and nothing went out', async () => {
  const { bff, main } = await form({ ...AS_LOADED, [DRAFT]: 'cuda0' })
  await landed(main)
  const sel = selectOf(DRAFT)
  expect(optionsOf(sel), '19 G.16: the match keeps the file spelling').toEqual([
    { value: '', text: EMPTY_DRAFT },
    {
      value: 'cuda0',
      text: en(
        'model-profile-device-option',
        'CUDA0',
        'NVIDIA GeForce RTX 5080',
      ),
    },
    gpuOption('ROCm0', 'AMD Radeon AI PRO R9700'),
    gpuOption('Vulkan0', 'Intel Arc Graphics'),
  ])
  expect(sel.value, '19 G.16: the select shows the file value').toBe('cuda0')
  expect(
    within(main)
      .getByRole('button', { name: SAVE })
      .getAttribute('aria-disabled'),
    '19 G.16: the untouched select leaves the form clean',
  ).toBe('true')
  expect(bff.to('PUT', PUT), '19 G.16: nothing was sent').toHaveLength(0)
})

test('19 G.16: while the device read is pending, the options stay today (the empty option and the file value)', async () => {
  const h = held()
  await form({ ...AS_LOADED, [DRAFT]: 'CUDA9' }, { devices: h.answer })
  const today = todayOptions(EMPTY_DRAFT, 'CUDA9')
  expect(optionsOf(selectOf(DRAFT)), '19 G.16: pending keeps today').toEqual(
    today,
  )
})

test('19 G.16: a device read that failed keeps today (the empty option and the file value)', async () => {
  await form({ ...AS_LOADED, [DRAFT]: 'CUDA9' }, { devices: reply(500) })
  expect(
    optionsOf(selectOf(DRAFT)),
    '19 G.16: a failed read keeps today',
  ).toEqual(todayOptions(EMPTY_DRAFT, 'CUDA9'))
})

test('19 G.16: the enable rule is unchanged: with no Draft model file the select is disabled, the GPU list and all', async () => {
  const { main } = await form(without('model-draft', 'n-gpu-layers-draft'))
  await landed(main)
  const sel = selectOf(DRAFT)
  expect(
    sel.disabled,
    '19 G.16: a file with no Draft model file disables the select',
  ).toBe(true)
  expect(optionsOf(sel), '19 G.16: a disabled select keeps the list').toEqual([
    { value: '', text: EMPTY_DRAFT },
    ...gpuOptions(),
  ])
})

// ── 19 G.17: the Vision GPU list ──────────────────────────────────────────

test('19 G.17: the Vision GPU select holds model-profile-device-follows-model empty, then none with text model-profile-device-none, then the listed GPUs; a file value none selects it, never an extra option, leaving the form clean', async () => {
  const { bff, main } = await form({
    ...AS_LOADED,
    ...VISION_ON,
    [VISION]: 'none',
  })
  await landed(main)
  const sel = selectOf(VISION)
  expect(
    within(main).getByRole('combobox', { name: VISION_LABEL }),
    '19 G.17: the select is named by its label',
  ).toBe(sel)
  expect(optionsOf(sel), '19 G.17: none is its option, no extra one').toEqual([
    { value: '', text: EMPTY_VISION },
    { value: 'none', text: NONE_TEXT },
    ...gpuOptions(),
  ])
  expect(sel.value, '19 G.17: the file value none selects it').toBe('none')
  expect(
    within(main)
      .getByRole('button', { name: SAVE })
      .getAttribute('aria-disabled'),
    '19 G.17: the untouched select leaves the form clean',
  ).toBe('true')
  expect(bff.to('PUT', PUT), '19 G.17: nothing was sent').toHaveLength(0)
})

test('19 G.17: a Vision file value matching no listed GPU is the extra last option as 19 G.16, with the none option kept', async () => {
  const { main } = await form({ ...AS_LOADED, ...VISION_ON, [VISION]: 'CUDA9' })
  await landed(main)
  expect(optionsOf(selectOf(VISION)), '19 G.17: the extra last option').toEqual(
    [
      { value: '', text: EMPTY_VISION },
      { value: 'none', text: NONE_TEXT },
      ...gpuOptions(),
      {
        value: 'CUDA9',
        text: en('model-profile-device-option-missing', 'CUDA9'),
      },
    ],
  )
})

test('19 G.17: while the device read is pending, the Vision options are today (the empty option and the file value, none riding as its own)', async () => {
  const h = held()
  await form(
    { ...AS_LOADED, ...VISION_ON, [VISION]: 'none' },
    { devices: h.answer },
  )
  expect(optionsOf(selectOf(VISION)), '19 G.17: pending keeps today').toEqual(
    todayOptions(EMPTY_VISION, 'none'),
  )
})

test('19 G.17: the enable rules are unchanged: with Vision on GPU unchecked the select is disabled, none and the GPU list and all', async () => {
  const { main } = await form({
    ...AS_LOADED,
    ...VISION_ON,
    [OFFLOAD]: 'false',
    [VISION]: 'none',
  })
  await landed(main)
  const sel = selectOf(VISION)
  expect(
    sel.disabled,
    '19 G.17: unchecked Vision on GPU disables the select',
  ).toBe(true)
  expect(optionsOf(sel), '19 G.17: a disabled select keeps the list').toEqual([
    { value: '', text: EMPTY_VISION },
    { value: 'none', text: NONE_TEXT },
    ...gpuOptions(),
  ])
})

// ── 19 G.18: the device bound refusal ─────────────────────────────────────

test('19 G.18: a 422 model-profile-device-outside with field device-draft marks the Draft device select with api-model-profile-device-outside, aria-invalid and focused, the bar tells save-refused, and the Vision GPU select and the GPUs fieldset stay clean', async () => {
  const out = outside(DRAFT, 'ROCm1')
  const { bff, main } = await form(
    { ...AS_LOADED, ...VISION_ON, [DRAFT]: 'ROCm1' },
    { modelProfileSave: () => reply(422, out.body) },
  )
  await landed(main)
  await dirty(main)
  await save(main, bff)
  await within(main).findByText(en('save-refused', out.text))
  const sel = selectOf(DRAFT)
  expectError(sel, out.text, '19 G.18: the refused Draft device')
  expect(document.activeElement, '19 G.18: focus on the select').toBe(sel)
  expect(
    selectOf(VISION).getAttribute('aria-invalid'),
    '19 G.18: the Vision GPU select is not marked',
  ).not.toBe('true')
  const fs = within(main).getByRole('group', { name: GPUS })
  expect(
    within(fs).queryByText(out.text),
    '19 G.18: the GPUs fieldset shows no error',
  ).toBeNull()
})

test('19 G.18: a 422 model-profile-device-outside naming mmproj-device only in args[2] marks the Vision GPU select and focuses it, and the Draft device select stays clean', async () => {
  const out = outside(VISION, 'ROCm1')
  const { bff, main } = await form(
    { ...AS_LOADED, ...VISION_ON, [VISION]: 'ROCm1' },
    // No `field` in the body: the key and args only, as L4 answers.
    { modelProfileSave: () => reply(422, out.body) },
  )
  await landed(main)
  await dirty(main)
  await save(main, bff)
  await within(main).findByText(en('save-refused', out.text))
  const sel = selectOf(VISION)
  expectError(
    sel,
    out.text,
    '19 G.18: the refused Vision GPU, named by args[2]',
  )
  expect(document.activeElement, '19 G.18: focus on the select').toBe(sel)
  expect(
    selectOf(DRAFT).getAttribute('aria-invalid'),
    '19 G.18: the Draft device select is not marked',
  ).not.toBe('true')
})

test('19 G.18: a refusal on a device select that is disabled keeps its mark, takes no focus on it, and the bar names the field and its card (02h 198a)', async () => {
  const out = outside(VISION, 'ROCm1')
  const { bff, main } = await form(
    {
      ...AS_LOADED,
      mmproj: 'mmprojs/mmproj.gguf',
      [OFFLOAD]: 'false',
      [VISION]: 'ROCm1',
    },
    { modelProfileSave: () => reply(422, out.body) },
  )
  await landed(main)
  const sel = selectOf(VISION)
  expect(sel.disabled, 'the Vision GPU select is disabled').toBe(true)
  await dirty(main)
  await save(main, bff)
  const at = en('save-refused-at', out.text, VISION_LABEL, VISION_CARD)
  await within(main).findByText(at)
  expectError(sel, out.text, '19 G.18: the refused disabled Vision GPU')
  expect(
    document.activeElement,
    '19 G.18: a disabled select takes no focus',
  ).not.toBe(sel)
  expect(
    document.activeElement,
    '19 G.18: 198a: focus stays on Save changes',
  ).toBe(within(main).getByRole('button', { name: SAVE }))
})

// ── 19 G.19: keyboard, screen reader, strings ─────────────────────────────

test('19 G.19: the tab order inside GPU layers is the About toggle, the default box, the text input, the slider, in that order, and the field holds no other tab stop', async () => {
  const { main } = await form()
  await within(main).findByText(FACTS_MOE)
  const f = field(main, GPU_LAYERS)
  const seq = [
    within(f).getByRole('button', { name: `${ABOUT} ${GPU_LAYERS}` }),
    checkBox(main, GPU_LAYERS),
    input(main, GPU_LAYERS),
    slider(main, GPU_LAYERS),
  ]
  const stops = mainTabbable(main).filter((el) => f.contains(el))
  expect(stops, '19 G.19: four tab stops in the field').toHaveLength(4)
  for (const n of [0, 1, 2, 3])
    expect(stops[n], `19 G.19: tab stop ${n + 1} of the field`).toBe(seq[n])
})

test('19 G.19: nothing else in Hardware changed: Draft GPU layers keeps no slider with the facts read, and its device select is the only one the GPU list opens', async () => {
  const { main } = await form()
  await within(main).findByText(FACTS_MOE)
  expect(
    within(field(main, DRAFT_LAYERS)).queryByRole('slider', {
      name: DRAFT_LAYERS,
    }),
    '19 G.19: Draft GPU layers has no slider (the draft file facts are not read)',
  ).toBeNull()
})

test('19 G.16, 19 G.19: a 64-character device name is the whole option text, and the control stays a native select whose closed width follows the field', async () => {
  const probe: DeviceProbe = {
    probedAt: DEVICES.probedAt,
    devices: [{ id: 'CUDA0', name: LONG_NAME, backend: 'CUDA' }],
  }
  const { main } = await form({ ...AS_LOADED }, { devices: probe })
  await landed(main, probe)
  const sel = selectOf(DRAFT)
  expect(sel.tagName, '19 G.19: a native select, no custom widget').toBe(
    'SELECT',
  )
  expect(optionsOf(sel), '19 G.19: the name shows whole').toEqual([
    { value: '', text: EMPTY_DRAFT },
    gpuOption('CUDA0', LONG_NAME),
  ])
})

test('19 G.19: the strings file holds LS2b section 2 word for word, the refusal line with no closing full stop (02h 150)', () => {
  const table = strings as Record<string, string>
  const want: Record<string, string> = {
    'model-profile-device-option': '{0}: {1}',
    'model-profile-device-option-missing': '{0} (not detected)',
    'model-profile-device-none': 'none: not on a GPU',
    'api-model-profile-device-outside':
      "{2} = {3} names a GPU outside the Devices of instance {1}'s server profile, so model profile {0} may not use it. Pick a GPU of those Devices, or widen the server profile's Devices first",
  }
  const missing = Object.entries(want)
    .filter(([key, english]) => table[key] !== english)
    .map(([key]) => key)
  expect(
    missing,
    '19 G.19: keys missing from en_US.json, or with other text',
  ).toEqual([])
})
