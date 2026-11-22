// Brief 19, sub-slice LS1, criteria 19 G.4, 19 G.5, 19 G.6, 19 G.7 and 19 G.8
// (fe/briefs/19-layer-sliders.md): when the checked GPU rows show blocks
// instead of brief 14's weight fields (the split mode, the facts read, the
// layout, two rows checked, and the vector of the checked rows), the controls
// of a blocks row (their order, names, `inputmode`, the slider's range,
// `aria-valuetext`, `aria-describedby`, the budget line), moving a slider
// (`set_blocks` from the weights as they stood when the pointer went down),
// typing a count (applied on Enter or on blur, clamped, nonsense put back),
// and Auto and Even in blocks mode. 19 G.1 and G.2 are in ModelProfileGpu5,
// 19 G.3 in gpuBlocks.test.ts, 19 G.9 to G.12 in ModelProfileGpu7.test.tsx.
// The helpers are those of ModelProfileGpu4.test.tsx, repeated here (a test
// file imports no other test file).
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
import {
  before,
  DEVICES,
  en,
  fakeBff,
  held,
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
const GPU = en('model-profile-gpu-distribution')
const GPUS = en('model-profile-gpus')
const BLOCKS = en('model-profile-gpu-blocks')
const WEIGHT = en('model-profile-gpu-weight')
const AUTO = en('model-profile-gpu-auto')
const EVEN = en('model-profile-gpu-even')
const AUTO_SHARE = en('model-profile-gpu-share-auto')
const MODE_LABEL = en('model-profile-split-mode')
/** The three GPUs of the brief's probe, as the file spells them. */
const A = 'CUDA0'
const B = 'ROCm0'
const C = 'Vulkan0'
/** Ornith-1.0-9B, the brief's file: 33 blocks, so 34 positions. */
const BLOCK_COUNT = 33
const PLACES = 34
/** The file of the brief's worked examples: a ratio of 85 and 15, which blocks
 * mode projects to the counts 29 and 5 of 34. */
const FILE_VALUES = { device: `${A},${B}`, 'tensor-split': '85,15' }

/** The 200 body of a read that answered `blockCount` blocks and no experts. */
function readInfo(blockCount = BLOCK_COUNT): ModelInfo {
  return { model: MODEL_FILE, info: { blockCount, moe: false }, reason: null }
}

/** The options of a page whose model file is read: 33 blocks, no experts. */
function read(overrides: BffOptions = {}): BffOptions {
  return { modelInfo: readInfo(), ...overrides }
}

beforeEach(() => {
  startAt(HASH)
})

afterEach(() => {
  cleanup()
  startAt('')
})

// ── Helpers (as ModelProfileGpu4.test.tsx) ──────────────────────────────────

/** Waits for the page read with the file `values` and the GPU probe `DEVICES`
 * answered. A save answers 200 and applies the sent values to the next read,
 * as the bff does. */
async function form(
  values: Record<string, string> = { ...FILE_VALUES },
  options: BffOptions = {},
) {
  const loaded = { ...values }
  const bff = fakeBff({
    devices: DEVICES,
    modelProfile: () =>
      reply(
        200,
        modelProfileDoc({}, { revision: 'r1', values: { ...loaded } }),
      ),
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
  await within(main).findByRole('heading', { level: 3, name: GPU })
  await ready(main)
  return { bff, main }
}

/** Waits for the device read to land with the brief's probe time. */
async function ready(main: HTMLElement): Promise<HTMLElement> {
  const fs = await within(main).findByRole('group', { name: GPUS })
  await within(fs).findByText(asOfDate(DEVICES.probedAt!))
  return fs
}

/** The moment the as-of line shows for `probedAt`. */
function asOfDate(probedAt: string): string {
  return new Date(probedAt).toLocaleString(undefined, {
    dateStyle: 'medium',
    timeStyle: 'short',
  })
}

/** The as-of line's element (GD1.7): its sentence, read across the `time`
 * child that carries the moment. */
function asOfEl(main: HTMLElement): HTMLElement {
  const line = en('model-profile-gpu-probed-at', asOfDate(DEVICES.probedAt!))
  const found = [...fieldset(main).querySelectorAll('p')].filter(
    (p) => (p.textContent ?? '') === line,
  )
  expect(found, 'GD1.7: the as-of line, once').toHaveLength(1)
  return found[0]!
}

/** The card's `section`, from its h3 (Card.tsx). */
function cardOf(main: HTMLElement, title: string): HTMLElement {
  const h3 = within(main).getByRole('heading', { level: 3, name: title })
  const card = h3.closest('section')
  expect(card, `${title}: the card element`).not.toBeNull()
  return card as HTMLElement
}

/** The GPUs fieldset, named by its legend (GD1.4). */
function fieldset(main: HTMLElement): HTMLElement {
  return within(main).getByRole('group', { name: GPUS })
}

/** The list of rows (GD1.4). */
function listEl(main: HTMLElement): HTMLElement {
  const list = fieldset(main).querySelector('ul[role="list"]')
  expect(list, 'GD1.4: a ul role="list" of the rows').not.toBeNull()
  return list as HTMLElement
}

/** The name a screen reader reads (ModelProfileGpu4.test.tsx's computation). */
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

/** Every device row, in the page's order. */
function rows(main: HTMLElement): {
  li: HTMLElement
  name: string
  box: HTMLInputElement
}[] {
  const lis = [...listEl(main).children].filter(
    (el) => el.tagName === 'LI',
  ) as HTMLElement[]
  return lis.map((li) => {
    const boxes = within(li).getAllByRole('checkbox')
    expect(boxes, 'GD1.4: one checkbox per row').toHaveLength(1)
    return { li, name: nameOf(boxes[0]!), box: boxes[0] as HTMLInputElement }
  })
}

/** The id a row's label starts with, up to the first space. */
function idOf(name: string): string {
  return name.split(' ')[0]!
}

/** The row whose label starts with `id`. */
function rowOf(main: HTMLElement, id: string) {
  const found = rows(main).find((r) => idOf(r.name) === id)
  expect(
    found,
    `the row of ${id}: the rows are ${rows(main)
      .map((r) => idOf(r.name))
      .join(', ')}`,
  ).toBeDefined()
  return found!
}

/** The checked rows' ids, in page order. */
function checkedIds(main: HTMLElement): string[] {
  return rows(main)
    .filter((r) => r.box.checked)
    .map((r) => idOf(r.name))
}

/** Clicks a row's checkbox. */
function toggle(main: HTMLElement, id: string) {
  fireEvent.click(rowOf(main, id).box)
}

/** The split mode select (GD1.2). */
function modeSelect(main: HTMLElement): HTMLSelectElement {
  return within(cardOf(main, GPU)).getByRole('combobox', {
    name: MODE_LABEL,
  }) as HTMLSelectElement
}

/** Picks a split mode by its value (`''` is the first, layer). */
function pickMode(main: HTMLElement, value: string) {
  fireEvent.change(modeSelect(main), { target: { value } })
}

/** Every control whose accessible name starts with `prefix`, in page order. */
function controlsNamed(
  main: HTMLElement,
  role: 'textbox' | 'slider',
  prefix: string,
): HTMLInputElement[] {
  const out: HTMLInputElement[] = []
  for (const r of rows(main)) {
    const id = idOf(r.name)
    const el = within(r.li).queryByRole(role, { name: `${prefix} ${id}` })
    if (el) out.push(el as HTMLInputElement)
  }
  return out
}

/** The Blocks text input of `id` (19 G.5), named "Blocks <id>". */
function blocksInput(main: HTMLElement, id: string): HTMLInputElement {
  const found = within(rowOf(main, id).li).queryByRole('textbox', {
    name: `${BLOCKS} ${id}`,
  })
  expect(
    found,
    `19 G.5: the blocks input of ${id}, named "${BLOCKS} ${id}"`,
  ).not.toBeNull()
  return found as HTMLInputElement
}

/** The blocks slider of `id` (19 G.5), named "Blocks <id>". */
function blocksSlider(main: HTMLElement, id: string): HTMLInputElement {
  const found = within(rowOf(main, id).li).queryByRole('slider', {
    name: `${BLOCKS} ${id}`,
  })
  expect(
    found,
    `19 G.5: the blocks slider of ${id}, named "${BLOCKS} ${id}"`,
  ).not.toBeNull()
  return found as HTMLInputElement
}

/** The blocks inputs of the checked rows, in page order. */
function blocksInputs(main: HTMLElement): HTMLInputElement[] {
  return controlsNamed(main, 'textbox', BLOCKS)
}

/** The blocks sliders of the checked rows, in page order. */
function blocksSliders(main: HTMLElement): HTMLInputElement[] {
  return controlsNamed(main, 'slider', BLOCKS)
}

/** The counts the blocks controls show, in page order, as
 * `[id, input, slider]`. */
function blockValues(main: HTMLElement): [string, string, string][] {
  return rows(main)
    .filter((r) => r.box.checked)
    .map((r) => {
      const id = idOf(r.name)
      const input = within(r.li).queryByRole('textbox', {
        name: `${BLOCKS} ${id}`,
      }) as HTMLInputElement | null
      const slider = within(r.li).queryByRole('slider', {
        name: `${BLOCKS} ${id}`,
      }) as HTMLInputElement | null
      return [
        id,
        input ? input.value : '(no input)',
        slider ? slider.value : '(no slider)',
      ]
    })
}

/** The weight fields shown, in page order, as `[id, value]` (GD2.1). */
function weights(main: HTMLElement): [string, string][] {
  return [...fieldset(main).querySelectorAll<HTMLInputElement>('input')]
    .filter((el) => nameOf(el).startsWith(`${WEIGHT} `))
    .map((el) => [nameOf(el).slice(WEIGHT.length + 1), el.value])
}

/** The weight fields of the checked rows (19 G.4: gone in blocks mode). */
function weightIds(main: HTMLElement): string[] {
  return weights(main).map(([id]) => id)
}

/** Every element of `li` with no element children and a text (GD2.2's leaf). */
function leaves(li: HTMLElement): HTMLElement[] {
  return [...li.querySelectorAll<HTMLElement>('*')].filter(
    (el) =>
      el.children.length === 0 &&
      !['INPUT', 'SELECT', 'TEXTAREA', 'BUTTON'].includes(el.tagName) &&
      (el.textContent ?? '').trim() !== '',
  )
}

/** 19 G.5's one plain text item of `id`'s row, carrying `range` and
 * `shareText`. The row's DOM shape is free, so the item is found by text: of
 * the elements whose text carries the range, the deepest is the last in
 * document order, a parent carrying its children's text. */
function textItem(
  main: HTMLElement,
  id: string,
  range: string,
  shareText: string,
): HTMLElement {
  const found = [...rowOf(main, id).li.querySelectorAll<HTMLElement>('*')].map(
    (el) => ({
      el,
      text: (el.textContent ?? '').replace(/\s+/g, ' ').trim(),
    }),
  )
  const withRange = found.filter((t) => t.text.includes(range))
  expect(
    withRange.length,
    `19 G.5: ${id}: the row shows the range text "${range}"`,
  ).toBeGreaterThan(0)
  const item = withRange[withRange.length - 1]!
  expect(
    item.text,
    `19 G.5: ${id}: the item of "${range}" carries the share "${shareText}"`,
  ).toContain(shareText)
  return item.el
}

/** The share element of `id` (GD2.2), `null` where the row shows none: the
 * leaf whose text is a share, the share element 19 G.5 describes with the
 * text input's `aria-describedby` (its id the first). */
function shareEl(main: HTMLElement, id: string): HTMLElement | null {
  const found = leaves(rowOf(main, id).li).filter((el) => {
    const text = (el.textContent ?? '').replace(/\s+/g, ' ').trim()
    return (
      text.startsWith('Share: ') ||
      [
        en('model-profile-gpu-share-unused'),
        en('model-profile-gpu-share-even'),
        AUTO_SHARE,
      ].includes(text)
    )
  })
  expect(
    found.length,
    `GD2.2: at most one share line on ${id}`,
  ).toBeLessThanOrEqual(1)
  return found[0] ?? null
}

/** The share text of a count of `places` (19 G.5). */
function share(count: number, places = PLACES): string {
  return en('model-profile-gpu-share', `${Math.round((count * 100) / places)}%`)
}

/** The range text of `count` blocks of a layout starting at `start` with
 * `places` positions, as `block_ranges` spells it (19 G.3, 19 G.5). */
function rangeOf(
  count: number,
  start: number,
  places = PLACES,
  layoutStart = 0,
): string {
  if (count <= 0) return en('model-profile-gpu-blocks-nothing')
  const end = start + count - 1
  if (end >= layoutStart + places - 1) {
    const blocks = count - 1
    if (blocks === 1)
      return en('model-profile-gpu-block-plus-output', `${start}`)
    if (blocks >= 2)
      return en(
        'model-profile-gpu-blocks-plus-output',
        `${start}`,
        `${start + blocks - 1}`,
      )
    return en('model-profile-gpu-output-layer')
  }
  if (count === 1) return en('model-profile-gpu-block-one', `${start}`)
  return en('model-profile-gpu-blocks-range', `${start}`, `${end}`)
}

/** The text each id of `aria-describedby` gives, in order, collapsed. */
function described(el: HTMLElement): string[] {
  return (el.getAttribute('aria-describedby') ?? '')
    .split(/\s+/)
    .filter(Boolean)
    .map((id) =>
      (document.getElementById(id)?.textContent ?? `(no #${id})`)
        .replace(/\s+/g, ' ')
        .trim(),
    )
}

/** The card's Auto (or Even) button (GD2.4). */
function splitButton(
  main: HTMLElement,
  label: typeof AUTO | typeof EVEN,
): HTMLButtonElement {
  const found = within(cardOf(main, GPU))
    .getAllByRole('button')
    .filter((b) => nameOf(b) === label)
  expect(found, `GD2.4: one ${label} in the card`).toHaveLength(1)
  return found[0] as HTMLButtonElement
}

/** The Auto / Even buttons the card shows, by name. */
function splitButtons(main: HTMLElement): string[] {
  return within(cardOf(main, GPU))
    .queryAllByRole('button')
    .filter((b) => nameOf(b) === AUTO || nameOf(b) === EVEN)
    .map((b) => nameOf(b))
}

/** A muted paragraph of the fieldset whose text is exactly `text`, once, after
 * the as-of line and before the list (19 G.4, 19 G.5). */
function lineBeforeList(main: HTMLElement, text: string): HTMLElement {
  const fs = fieldset(main)
  const found = [...fs.querySelectorAll('p')].filter(
    (p) => (p.textContent ?? '').replace(/\s+/g, ' ').trim() === text,
  )
  expect(
    found.map((p) => p.textContent),
    `${text}: once`,
  ).toHaveLength(1)
  const el = found[0]!
  expect(el.className, `${text}: muted`).toContain('muted')
  expect(el.getAttribute('role'), `${text}: not a live region`).toBeNull()
  expect(before(asOfEl(main), el), `${text}: after the as-of line`).toBe(true)
  expect(before(el, listEl(main)), `${text}: before the list`).toBe(true)
  return el
}

/** The budget line of 19 G.5 for `places`. */
function budgetLine(main: HTMLElement, places: number): HTMLElement {
  return lineBeforeList(
    main,
    en('model-profile-gpu-blocks-budget', `${places}`, `${places - 1}`),
  )
}

/** The Auto paragraph of 19 G.4 for `places`. */
function autoLine(main: HTMLElement, places: number): HTMLElement {
  return lineBeforeList(main, en('model-profile-gpu-blocks-auto', `${places}`))
}

/** The malformed paragraph of 19 G.4 for the vector `vector`. */
function malformedLine(main: HTMLElement, vector: string): HTMLElement {
  return lineBeforeList(main, en('model-profile-gpu-blocks-malformed', vector))
}

/** The bar's Save button. */
function saveButton(main: HTMLElement): HTMLButtonElement {
  return within(main).getByRole('button', {
    name: en('save-button'),
  }) as HTMLButtonElement
}

/** The `values` of the n-th PUT of this model profile. */
function sentValues(
  bff: ReturnType<typeof fakeBff>,
  n = 1,
): Record<string, unknown> {
  const body = bff.to('PUT', PUT)[n - 1]!.body as Record<string, unknown>
  return body.values as Record<string, unknown>
}

/** Moves a row's slider to `value`, the way a pointer drag does. */
function drag(main: HTMLElement, id: string, value: string): HTMLInputElement {
  const slider = blocksSlider(main, id)
  fireEvent.change(slider, { target: { value } })
  return slider
}

/** Types `value` in a row's blocks input, changing only its text. */
function typeBlocks(main: HTMLElement, id: string, value: string) {
  const input = blocksInput(main, id)
  fireEvent.change(input, { target: { value } })
  return input
}

/** Edits a field that is not in the card, so a save has something to send. */
function editCtxSize() {
  fireEvent.change(document.getElementById('model-profile-ctx-size-control')!, {
    target: { value: '65536' },
  })
}

// ── 19 G.4: when the rows show blocks ───────────────────────────────────────

test('19 G.4: with the file mode, a read model, a layout and two rows checked, each checked row shows a blocks input and slider in place of its weight field, and the budget line counts the places', async () => {
  const { main } = await form(FILE_VALUES, read())
  expect(checkedIds(main), '19 G.4: two rows are checked').toEqual([A, B])
  expect(
    weightIds(main),
    '19 G.4: the weight fields are gone in blocks mode',
  ).toEqual([])
  expect(
    blocksInputs(main).map((el) => el.value),
    '19 G.4: the counts of the file 85,15 over 34 places are 29 and 5',
  ).toEqual(['29', '5'])
  expect(
    blocksSliders(main).map((el) => el.value),
    '19 G.5: the sliders show the same counts',
  ).toEqual(['29', '5'])
  budgetLine(main, PLACES)
})

test.each([
  ['85,15', [A, B], ['29', '5'], [rangeOf(29, 0), rangeOf(5, 29)]],
  ['82,18', [A, B], ['28', '6'], [rangeOf(28, 0), rangeOf(6, 28)]],
  ['86,14', [A, B], ['30', '4'], [rangeOf(30, 0), rangeOf(4, 30)]],
  ['0.6,0.4', [A, B], ['21', '13'], [rangeOf(21, 0), rangeOf(13, 21)]],
  ['3,2', [A, B], ['21', '13'], [rangeOf(21, 0), rangeOf(13, 21)]],
  ['1,33', [A, B], ['1', '33'], [rangeOf(1, 0), rangeOf(33, 1)]],
  ['0,1', [A, B], ['0', '34'], [rangeOf(0, 0), rangeOf(34, 0)]],
  ['85,', [A, B], ['34', '0'], [rangeOf(34, 0), rangeOf(0, 34)]],
  [
    '1,1,1',
    [A, B, C],
    ['12', '11', '11'],
    [rangeOf(12, 0), rangeOf(11, 12), rangeOf(11, 23)],
  ],
])(
  '19 G.4: the file vector %s projects to the counts %s, with the ranges and shares of each row',
  async (vector, ids, counts, ranges) => {
    const device = ids.join(',')
    const { main } = await form({ device, 'tensor-split': vector }, read())
    expect(
      blocksInputs(main).map((el) => el.value),
      `19 G.4: ${vector}: the counts`,
    ).toEqual(counts)
    for (const [n, id] of ids.entries()) {
      const count = Number(counts[n])
      expect(
        textItem(main, id, ranges[n]!, share(count)).textContent,
        `19 G.5: ${id}: the range text ${ranges[n]} and the share`,
      ).toContain(ranges[n]!)
      expect(
        (shareEl(main, id)?.textContent ?? '').replace(/\s+/g, ' ').trim(),
        `19 G.5: ${id}: the share of ${count} of ${PLACES}`,
      ).toBe(share(count))
    }
  },
)

/** The states where blocks mode is over, and brief 14 stays as built. */
const noBlocks: [
  string,
  { values: Record<string, string>; mode?: string; modelInfo?: ModelInfo },
][] = [
  ['the split mode is tensor', { values: FILE_VALUES, mode: 'tensor' }],
  ['the split mode is row', { values: FILE_VALUES, mode: 'row' }],
  ['one GPU is checked', { values: { device: A, 'tensor-split': '85' } }],
  ['no GPU is checked', { values: { device: '', 'tensor-split': '85,15' } }],
  [
    'the facts read answered a reason',
    {
      values: FILE_VALUES,
      modelInfo: {
        model: MODEL_FILE,
        info: null,
        reason: { key: 'model-info-unreadable', args: ['/m/x.gguf'] },
      },
    },
  ],
  [
    'GPU layers is 0, so there is no layout',
    { values: { ...FILE_VALUES, 'n-gpu-layers': '0' } },
  ],
]

test.each(noBlocks)(
  '19 G.4: %s: no blocks control, no budget line, and brief 14 as built',
  async (_what, c) => {
    const { main } = await form(
      c.values,
      read(c.modelInfo ? { modelInfo: c.modelInfo } : {}),
    )
    if (c.mode !== undefined) pickMode(main, c.mode)
    await waitFor(() =>
      expect(blocksInputs(main).length, '19 G.4: no blocks input').toBe(0),
    )
    expect(blocksSliders(main).length, '19 G.4: no blocks slider').toBe(0)
    expect(
      [...fieldset(main).querySelectorAll('p')].filter((p) =>
        (p.textContent ?? '').includes(
          en('model-profile-gpu-blocks-budget', `${PLACES}`, `${PLACES - 1}`),
        ),
      ).length,
      '19 G.4: no budget line',
    ).toBe(0)
    // GD2.1: with fewer than two rows checked there is no weight field; with
    // two, the weight fields are brief 14 as built, with the file's values.
    const ids = c.values.device
      .split(',')
      .map((s) => s.trim())
      .filter((s) => s !== '')
    expect(
      weightIds(main),
      `19 G.4: ${_what}: the weight fields of brief 14`,
    ).toEqual(ids.length >= 2 ? ids : [])
  },
)

test.each(['auto', 'all', '-3'])(
  '19 G.4: GPU layers %s is every layer, so the layout is the whole file and the rows show blocks',
  async (layers) => {
    const { main } = await form(
      { ...FILE_VALUES, 'n-gpu-layers': layers },
      read(),
    )
    expect(
      blocksInputs(main).map((el) => el.value),
      `19 G.4: n-gpu-layers ${layers}: the counts of 34 places`,
    ).toEqual(['29', '5'])
    budgetLine(main, PLACES)
  },
)

test('19 G.4: split mode none shows no editor at all, no blocks control, no budget line and no Auto or Even', async () => {
  const { main } = await form(FILE_VALUES, read())
  pickMode(main, 'none')
  await waitFor(() =>
    expect(blocksInputs(main).length, '19 G.4: no blocks input').toBe(0),
  )
  expect(weightIds(main), 'GD2.1: none shows no weight field').toEqual([])
  expect(
    [...fieldset(main).querySelectorAll('p')].filter((p) =>
      (p.textContent ?? '').includes(
        en('model-profile-gpu-blocks-budget', `${PLACES}`, `${PLACES - 1}`),
      ),
    ).length,
    '19 G.4: no budget line in mode none',
  ).toBe(0)
  expect(splitButtons(main), '19 G.8: no Auto, no Even in mode none').toEqual(
    [],
  )
})

test('19 G.4: an all-zero vector shows no editor, a muted line saying Even is the way in, and the shares read auto', async () => {
  const { main } = await form(
    { device: `${A},${B}`, 'tensor-split': '0,0' },
    read(),
  )
  expect(blocksInputs(main).length, '19 G.4: Auto has no blocks input').toBe(0)
  expect(blocksSliders(main).length, '19 G.4: and no slider').toBe(0)
  expect(weightIds(main), '19 G.4: and no weight field').toEqual([])
  autoLine(main, PLACES)
  for (const id of [A, B])
    expect(
      (shareEl(main, id)?.textContent ?? '').trim(),
      `19 G.4: ${id}: the share reads auto (GD2.2)`,
    ).toBe(AUTO_SHARE)
  expect(
    splitButtons(main).sort(),
    '19 G.8: Even is there to leave Auto',
  ).toEqual([AUTO, EVEN].sort())
})

test('19 G.4: a weight that is not a GD2.6 number shows no editor, the malformed line filled with the checked vector, and no share', async () => {
  const { main } = await form(
    { device: `${A},${B}`, 'tensor-split': '85,x' },
    read(),
  )
  expect(
    blocksInputs(main).length,
    '19 G.4: no blocks input for a malformed vector',
  ).toBe(0)
  expect(weightIds(main), '19 G.4: no weight field').toEqual([])
  malformedLine(main, '85,x')
  for (const id of [A, B])
    expect(shareEl(main, id), `19 G.4: ${id}: no share`).toBeNull()
})

test('19 G.4: while the facts read is pending a checked row of a two GPU split shows no weight editor and keeps its share', async () => {
  const h = held()
  const { main } = await form(FILE_VALUES, { modelInfo: h.answer })
  expect(
    weightIds(main),
    '19 G.4: no weight field while the read is pending',
  ).toEqual([])
  expect(blocksInputs(main).length, '19 G.4: no blocks input yet').toBe(0)
  expect(
    (shareEl(main, A)?.textContent ?? '').trim(),
    '19 G.4: the row keeps its GD2.2 share of the ratio',
  ).toBe(en('model-profile-gpu-share', '85%'))
  expect(
    [...fieldset(main).querySelectorAll('p')].filter((p) =>
      (p.textContent ?? '').includes(
        en('model-profile-gpu-blocks-budget', `${PLACES}`, `${PLACES - 1}`),
      ),
    ).length,
    '19 G.4: no budget line while the read is pending',
  ).toBe(0)
  h.resolve(reply(200, readInfo()))
  await waitFor(() =>
    expect(
      blocksInputs(main).map((el) => el.value),
      '19 G.4: the counts arrive with the facts',
    ).toEqual(['29', '5']),
  )
})

test('19 G.4: an extra split mode option such as Layer is not the file mode, so the rows keep their weights', async () => {
  const { main } = await form({ ...FILE_VALUES, 'split-mode': 'Layer' }, read())
  expect(blocksInputs(main).length, '19 G.4: no blocks input').toBe(0)
  expect(weightIds(main), '19 G.4: the weight fields stay').toEqual([A, B])
})

// ── 19 G.5: the controls of a blocks row ────────────────────────────────────

test.each([
  [A, '29', 0, 29],
  [B, '5', 29, 5],
])(
  '19 G.5: %s: a label and a text input inputmode numeric showing the count, then the range slider min 0 max the places step 1, then the range text and the share',
  async (id, value, start, count) => {
    const { main } = await form(FILE_VALUES, read())
    const li = rowOf(main, id).li
    const input = blocksInput(main, id)
    const slider = blocksSlider(main, id)
    expect(input.getAttribute('type'), '19 G.5: a text input').toBe('text')
    expect(input.getAttribute('inputmode'), '19 G.5: numeric').toBe('numeric')
    expect(input.value, '19 G.5: the input shows the count').toBe(value)
    expect(slider.tagName, '19 G.5: an input element').toBe('INPUT')
    expect(slider.getAttribute('type'), '19 G.5: a native range').toBe('range')
    expect(slider.getAttribute('min'), '19 G.5: min 0').toBe('0')
    expect(slider.getAttribute('max'), '19 G.5: max the places').toBe(
      `${PLACES}`,
    )
    expect(slider.getAttribute('step'), '19 G.5: step 1').toBe('1')
    expect(slider.value, '19 G.5: the slider shows the count').toBe(value)
    for (const a of ['aria-valuemin', 'aria-valuemax', 'aria-valuenow'])
      expect(
        slider.getAttribute(a),
        `19 G.5: the native slider carries no ${a}`,
      ).toBeNull()
    const range = rangeOf(count, start)
    const shareText = share(count)
    expect(
      slider.getAttribute('aria-valuetext'),
      '19 G.5: the value, the places and the range',
    ).toBe(en('model-profile-gpu-blocks-value', value, `${PLACES}`, range))
    const item = textItem(main, id, range, shareText)
    // The order inside the row: the label and its input, the slider, the text.
    expect(
      before(input, slider),
      `19 G.5: ${id}: the text input is before the slider`,
    ).toBe(true)
    expect(
      before(slider, item),
      `19 G.5: ${id}: the text item follows the slider`,
    ).toBe(true)
    expect(
      li.contains(input) && li.contains(slider) && li.contains(item),
      '19 G.5: inside the row',
    ).toBe(true)
    const d = described(input)
    expect(
      d[0],
      '19 G.5: the share is the first description (GD2.2)',
    ).toContain(shareText)
    expect(d.join(' '), '19 G.5: the range text describes it too').toContain(
      range,
    )
  },
)

test('19 G.5: the budget line is shown with the controls, says the places and the blocks, and is muted and not a live region', async () => {
  const { main } = await form(FILE_VALUES, read())
  const el = budgetLine(main, PLACES)
  expect(el.tagName, '19 G.5: a paragraph').toBe('P')
  expect(el.textContent).toBe(
    en('model-profile-gpu-blocks-budget', `${PLACES}`, `${PLACES - 1}`),
  )
})

// ── 19 G.6: moving a slider ─────────────────────────────────────────────────

test('19 G.6: moving a row slider writes the counts of every row at once, keeps the focus on the slider, and keeps the places', async () => {
  const { main } = await form(FILE_VALUES, read())
  const slider = drag(main, A, '20')
  expect(document.activeElement, '19 G.6: the slider keeps the focus').toBe(
    slider,
  )
  expect(
    blockValues(main),
    '19 G.6: 20 for the first row leaves 14 for the other, both controls at once',
  ).toEqual([
    [A, '20', '20'],
    [B, '14', '14'],
  ])
  for (const [id, count, start] of [
    [A, 20, 0],
    [B, 14, 20],
  ] as const) {
    expect(
      textItem(main, id, rangeOf(count, start), share(count)).textContent,
      `19 G.6: ${id}: the range text and the share follow`,
    ).toContain(rangeOf(count, start))
  }
  expect(
    slider.getAttribute('aria-valuetext'),
    '19 G.6: the slider says its new value',
  ).toBe(
    en('model-profile-gpu-blocks-value', '20', `${PLACES}`, rangeOf(20, 0)),
  )
  budgetLine(main, PLACES)
})

test('19 G.6: a drag counts from the weights as they stood when the pointer went down, kept until the pointer goes up', async () => {
  const { main } = await form(
    { device: `${A},${B},${C}`, 'tensor-split': '1,1,1' },
    read(),
  )
  expect(
    blocksInputs(main).map((el) => el.value),
    '19 G.3: 1,1,1 over 34 places is 12, 11, 11',
  ).toEqual(['12', '11', '11'])
  const slider = blocksSlider(main, A)
  fireEvent.pointerDown(slider)
  fireEvent.change(slider, { target: { value: '30' } })
  expect(
    blocksInputs(main).map((el) => el.value),
    '19 G.6: mid-drag the back rows pay, from the tail first',
  ).toEqual(['30', '4', '0'])
  fireEvent.change(slider, { target: { value: '12' } })
  fireEvent.pointerUp(slider)
  expect(
    blocksInputs(main).map((el) => el.value),
    '19 G.6: back where it started, the other rows as they were',
  ).toEqual(['12', '11', '11'])
  fireEvent.change(blocksSlider(main, A), { target: { value: '20' } })
  expect(
    blocksInputs(main).map((el) => el.value),
    '19 G.6: with the pointer up the next move starts from the current counts',
  ).toEqual(['20', '11', '3'])
  expect(
    blocksInputs(main)
      .map((el) => Number(el.value))
      .reduce((a, b) => a + b, 0),
    '19 G.6: the counts always sum to the places',
  ).toBe(PLACES)
})

test.each([
  ['the row that holds the output layer', B, '10', ['24', '10']],
  ['a row to 0', A, '0', ['0', '34']],
  ['a row to the places', A, '34', ['34', '0']],
])(
  '19 G.6: moving %s of the file 85,15 gives the counts %s',
  async (_what, id, value, counts) => {
    const { main } = await form(FILE_VALUES, read())
    drag(main, id, value)
    expect(
      blocksInputs(main).map((el) => el.value),
      `19 G.6: ${_what}`,
    ).toEqual(counts)
    expect(
      blocksInputs(main)
        .map((el) => Number(el.value))
        .reduce((a, b) => a + b, 0),
      '19 G.6: the budget is kept',
    ).toBe(PLACES)
  },
)

// ── 19 G.7: typing a count ──────────────────────────────────────────────────

test('19 G.7: typing changes only the text of that input; Enter applies it and moves the other rows', async () => {
  const { main } = await form(FILE_VALUES, read())
  const input = typeBlocks(main, A, '20')
  expect(input.value, '19 G.7: the text typed').toBe('20')
  expect(blocksSlider(main, A).value, '19 G.7: the slider has not moved').toBe(
    '29',
  )
  expect(
    blocksInput(main, B).value,
    '19 G.7: the other row has not moved',
  ).toBe('5')
  fireEvent.keyDown(input, { key: 'Enter' })
  await waitFor(() =>
    expect(blockValues(main), '19 G.7: Enter applies the count').toEqual([
      [A, '20', '20'],
      [B, '14', '14'],
    ]),
  )
})

test('19 G.7: losing the focus applies the count, and a number past the places is clamped to them', async () => {
  const { main } = await form(FILE_VALUES, read())
  const input = typeBlocks(main, A, '40')
  fireEvent.blur(input)
  await waitFor(() =>
    expect(
      blockValues(main),
      '19 G.7: clamped to the places, the other row to 0',
    ).toEqual([
      [A, '34', '34'],
      [B, '0', '0'],
    ]),
  )
})

test.each([
  ['an empty field', ''],
  ['a word', 'x'],
  ['a decimal', '2.5'],
  ['a negative number', '-1'],
])(
  '19 G.7: %s puts the count back on blur, with no error and no move of the other rows',
  async (_what, typed) => {
    const { main } = await form(FILE_VALUES, read())
    const input = typeBlocks(main, A, typed)
    fireEvent.blur(input)
    await waitFor(() =>
      expect(input.value, '19 G.7: the count is put back').toBe('29'),
    )
    expect(blockValues(main), '19 G.7: the vector is as it was').toEqual([
      [A, '29', '29'],
      [B, '5', '5'],
    ])
    expect(
      input.getAttribute('aria-invalid'),
      '19 G.7: no error on the field',
    ).not.toBe('true')
    expect(
      within(main).queryAllByRole('alert').length,
      '19 G.7: nothing announced',
    ).toBe(0)
  },
)

test('19 G.7: Escape does nothing special: the text stays as typed, the counts and the other rows do not move', async () => {
  const { main } = await form(FILE_VALUES, read())
  const input = typeBlocks(main, A, '20')
  fireEvent.keyDown(input, { key: 'Escape' })
  expect(input.value, '19 G.7: the text stays').toBe('20')
  // Escape does nothing special, so the typed text is NOT put back: row A's
  // text input shows what was typed while its slider and every other control
  // stay at the untouched counts (the same shape the typing test pins).
  expect(
    blockValues(main),
    '19 G.7: nothing applied: the sliders stay at the projected counts, row B stays, row A keeps the typed text',
  ).toEqual([
    [A, '20', '29'],
    [B, '5', '5'],
  ])
})

/** The two ways a typed count is committed (19 G.7). */
const commits: [string, (input: HTMLInputElement) => void][] = [
  ['Enter', (input) => fireEvent.keyDown(input, { key: 'Enter' })],
  ['the focus leaving', (input) => fireEvent.blur(input)],
]

test.each(commits)(
  '19 G.7: typing the count the row already has and committing by %s is no edit: the counts stay and tensor-split is not sent',
  async (_how, commit) => {
    const { bff, main } = await form(
      { device: `${A},${B}`, 'tensor-split': '1,1' },
      read(),
    )
    expect(
      blocksInputs(main).map((el) => el.value),
      '19 G.3: the file 1,1 over 34 places is the counts 17 and 17',
    ).toEqual(['17', '17'])
    // The count it shows, typed (the space makes it a text edit React sees).
    const input = typeBlocks(main, A, '17 ')
    commit(input)
    await waitFor(() =>
      expect(
        blockValues(main),
        '19 G.7: the count committed is the one it already had, so every row stays',
      ).toEqual([
        [A, '17', '17'],
        [B, '17', '17'],
      ]),
    )
    editCtxSize()
    fireEvent.click(saveButton(main))
    await waitFor(() => expect(bff.to('PUT', PUT).length).toBe(1))
    expect(
      sentValues(bff),
      '19 G.10 (GD2.5): the file 1,1 is not rewritten by a commit of the counts it projects to',
    ).not.toHaveProperty('tensor-split')
  },
)

// ── 19 G.8: Auto and Even in blocks mode ────────────────────────────────────

test('19 G.8: Auto empties every weight so the block controls go and the Auto line shows; Even sets the counts to the projection of ones, and the focus stays on the button pressed', async () => {
  const { main } = await form(FILE_VALUES, read())
  const auto = splitButton(main, AUTO)
  auto.focus()
  fireEvent.click(auto)
  await waitFor(() =>
    expect(
      blocksInputs(main).length,
      '19 G.8: Auto leaves no block control',
    ).toBe(0),
  )
  expect(blocksSliders(main).length, '19 G.8: and no slider').toBe(0)
  autoLine(main, PLACES)
  expect(document.activeElement, '19 G.8: the focus stays on Auto').toBe(auto)
  const even = splitButton(main, EVEN)
  even.focus()
  fireEvent.click(even)
  await waitFor(() =>
    expect(
      blockValues(main),
      '19 G.8: Even is the projection of all ones: 17 and 17',
    ).toEqual([
      [A, '17', '17'],
      [B, '17', '17'],
    ]),
  )
  expect(document.activeElement, '19 G.8: the focus stays on Even').toBe(even)
  expect(
    textItem(main, A, rangeOf(17, 0), share(17)).textContent,
    '19 G.5: the ranges and shares are recomputed',
  ).toContain(rangeOf(17, 0))
})

test('19 G.8: with three rows checked, Even is 12, 11, 11', async () => {
  const { main } = await form(
    { device: `${A},${B},${C}`, 'tensor-split': '85,15,1' },
    read(),
  )
  fireEvent.click(splitButton(main, EVEN))
  await waitFor(() =>
    expect(
      blocksInputs(main).map((el) => el.value),
      '19 G.8: the projection of ones over 34 places with three rows',
    ).toEqual(['12', '11', '11']),
  )
})

test('19 G.8: Auto and Even show with two rows checked in every editor state, and nowhere with one row checked', async () => {
  const { main } = await form(FILE_VALUES, read())
  expect(splitButtons(main).sort(), '19 G.8: with the block controls').toEqual(
    [AUTO, EVEN].sort(),
  )
  fireEvent.click(splitButton(main, AUTO))
  await waitFor(() => expect(blocksInputs(main).length, 'Auto state').toBe(0))
  expect(
    splitButtons(main).sort(),
    '19 G.8: in Auto, where Even is the way back',
  ).toEqual([AUTO, EVEN].sort())
  toggle(main, A)
  await waitFor(() => expect(checkedIds(main)).toEqual([B]))
  expect(
    splitButtons(main),
    '19 G.8: one row checked: no Auto, no Even',
  ).toEqual([])
})

test('19 G.8: a malformed vector still shows Auto and Even, and Auto leaves that state', async () => {
  const { main } = await form(
    { device: `${A},${B}`, 'tensor-split': '85,x' },
    read(),
  )
  expect(splitButtons(main).sort(), '19 G.8: with the malformed line').toEqual(
    [AUTO, EVEN].sort(),
  )
  fireEvent.click(splitButton(main, AUTO))
  await waitFor(() =>
    expect(
      [...fieldset(main).querySelectorAll('p')].filter((p) =>
        (p.textContent ?? '').includes(
          en('model-profile-gpu-blocks-auto', `${PLACES}`),
        ),
      ).length,
      '19 G.8: Auto replaces the malformed line',
    ).toBe(1),
  )
  expect(
    [...fieldset(main).querySelectorAll('p')].filter((p) =>
      (p.textContent ?? '').includes(
        en('model-profile-gpu-blocks-malformed', '85,x'),
      ),
    ).length,
    '19 G.8: the malformed line is gone',
  ).toBe(0)
})

// ── 19 G.10 (the save of a block edit): its weights are GD2.5 weights ───────

test('19 G.10: a slider move composes tensor-split as the counts, and an untouched ratio of the file is never sent', async () => {
  const { bff, main } = await form(FILE_VALUES, read())
  editCtxSize()
  fireEvent.click(saveButton(main))
  await waitFor(() => expect(bff.to('PUT', PUT).length).toBe(1))
  expect(
    sentValues(bff),
    '19 G.10: the counts the mode shows are no edit',
  ).not.toHaveProperty('tensor-split')
  drag(main, A, '20')
  fireEvent.click(saveButton(main))
  await waitFor(() => expect(bff.to('PUT', PUT).length).toBe(2))
  expect(
    sentValues(bff, 2)['tensor-split'],
    '19 G.10: the slider to 20 sends 20,14',
  ).toBe('20,14')
})
