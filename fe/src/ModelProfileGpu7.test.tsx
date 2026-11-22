// Brief 19, sub-slice LS1, criteria 19 G.9, 19 G.10, 19 G.11 and 19 G.12
// (fe/briefs/19-layer-sliders.md): checking, unchecking and moving rows in
// blocks mode (the counts follow the weights and the GPU, the range texts are
// recomputed in the new order, one row left leaves blocks mode), the save of a
// block edit (the counts as `tensor-split`, the 422 `tensor-split-format` in
// the blocks place, Discard putting the projected counts back), GPU layers
// re-projecting the rows at once with no write of its own, and the keyboard,
// screen-reader and tab order of a blocks row. 19 G.1 and G.2 are in
// ModelProfileGpu5, 19 G.3 in gpuBlocks.test.ts, 19 G.4 to G.8 and the
// untouched-ratio half of G.10 in ModelProfileGpu6.test.tsx. The helpers are
// those of ModelProfileGpu4.test.tsx, repeated here (a test file imports no
// other test file).
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
const GPU = en('model-profile-gpu-distribution')
const GPUS = en('model-profile-gpus')
const BLOCKS = en('model-profile-gpu-blocks')
const WEIGHT = en('model-profile-gpu-weight')
const AUTO = en('model-profile-gpu-auto')
const EVEN = en('model-profile-gpu-even')
const UP = en('model-profile-gpu-move-up')
const DOWN = en('model-profile-gpu-move-down')
const AUTO_SHARE = en('model-profile-gpu-share-auto')
const UNUSED_SHARE = en('model-profile-gpu-share-unused')
const EVEN_SHARE = en('model-profile-gpu-share-even')
/** GPU layers: the field whose value is the layout's `ngl` (19 G.11). */
const LAYERS = 'n-gpu-layers'
/** The three GPUs of the brief's probe, as the file spells them. */
const A = 'CUDA0'
const B = 'ROCm0'
const C = 'Vulkan0'
/** Ornith-1.0-9B, the brief's file: 33 blocks, so 34 positions. */
const BLOCK_COUNT = 33
const PLACES = 34
/** The file of the brief's worked examples: 85 and 15, the counts 29 and 5. */
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
  const out: HTMLInputElement[] = []
  for (const r of rows(main)) {
    const el = within(r.li).queryByRole('textbox', {
      name: `${BLOCKS} ${idOf(r.name)}`,
    })
    if (el) out.push(el as HTMLInputElement)
  }
  return out
}

/** The counts the blocks inputs show, in page order, as `[id, value]`. */
function counts(main: HTMLElement): [string, string][] {
  return rows(main)
    .filter((r) => r.box.checked)
    .map((r) => {
      const id = idOf(r.name)
      const input = within(r.li).queryByRole('textbox', {
        name: `${BLOCKS} ${id}`,
      }) as HTMLInputElement | null
      return [id, input ? input.value : '(no input)']
    })
}

/** The weight fields shown, in page order, as their ids (GD2.1). */
function weightIds(main: HTMLElement): string[] {
  return [...fieldset(main).querySelectorAll<HTMLInputElement>('input')]
    .filter((el) => nameOf(el).startsWith(`${WEIGHT} `))
    .map((el) => nameOf(el).slice(WEIGHT.length + 1))
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
      [UNUSED_SHARE, EVEN_SHARE, AUTO_SHARE].includes(text)
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

/** The Move up (or Move down) button of `id` (GD1.9). */
function moveButton(
  main: HTMLElement,
  id: string,
  dir: typeof UP | typeof DOWN,
): HTMLButtonElement {
  const found = within(rowOf(main, id).li)
    .queryAllByRole('button')
    .filter((b) => nameOf(b) === `${dir} ${id}`)
  expect(found, `GD1.9: ${dir} ${id}`).toHaveLength(1)
  return found[0] as HTMLButtonElement
}

/** Presses a row's Move up / Move down. */
function move(
  main: HTMLElement,
  id: string,
  dir: typeof UP | typeof DOWN = UP,
) {
  const button = moveButton(main, id, dir)
  button.focus()
  fireEvent.click(button)
  return button
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

/** The budget line of 19 G.5 for `places`, before the list. */
function budgetLine(main: HTMLElement, places: number): HTMLElement {
  const text = en(
    'model-profile-gpu-blocks-budget',
    `${places}`,
    `${places - 1}`,
  )
  const found = [...fieldset(main).querySelectorAll('p')].filter(
    (p) => (p.textContent ?? '').replace(/\s+/g, ' ').trim() === text,
  )
  expect(
    found.map((p) => p.textContent),
    '19 G.5: the budget line, once',
  ).toHaveLength(1)
  const el = found[0]!
  expect(el.className, '19 G.5: the budget line is muted').toContain('muted')
  expect(before(el, listEl(main)), '19 G.5: before the list').toBe(true)
  return el
}

/** A number field's control and default box (`<id>-control`, `<id>-box`). */
function control(key: string): HTMLInputElement {
  const el = document.getElementById(`model-profile-${key}-control`)
  expect(el, `the control of ${key}`).not.toBeNull()
  return el as HTMLInputElement
}

function boxOf(key: string): HTMLInputElement {
  const el = document.getElementById(`model-profile-${key}-box`)
  expect(el, `the default box of ${key}`).not.toBeNull()
  return el as HTMLInputElement
}

/** Writes GPU layers, taking the field off its default when it sits there. */
function setLayers(value: string) {
  const box = boxOf(LAYERS)
  if (box.checked) fireEvent.click(box)
  fireEvent.change(control(LAYERS), { target: { value } })
}

/** Puts GPU layers back on its default. */
function layersDefault() {
  const box = boxOf(LAYERS)
  if (!box.checked) fireEvent.click(box)
}

/** Every tabbable element of `main`, in document order (02h 196, 197). */
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

/** The bar's Save button. */
function saveButton(main: HTMLElement): HTMLButtonElement {
  return within(main).getByRole('button', {
    name: en('save-button'),
  }) as HTMLButtonElement
}

function discardButton(main: HTMLElement): HTMLButtonElement {
  return within(main).getByRole('button', {
    name: en('save-discard'),
  }) as HTMLButtonElement
}

/** The bar's alert carrying `text` (GD2.7). */
function expectAlert(main: HTMLElement, text: string, what = text) {
  const el = within(main).getByText(text)
  const alert = el.closest('[role="alert"]')
  expect(alert, `${what} is in role="alert"`).not.toBe(null)
}

/** A field marked with `text`: `aria-invalid` and its description (GD2.7). */
function expectError(el: HTMLElement, text: string, what: string) {
  expect(el.getAttribute('aria-invalid'), `${what} aria-invalid`).toBe('true')
  expect(described(el), `${what} aria-describedby`).toContain(text)
}

/** The `values` of the n-th PUT of this model profile. */
function sentValues(
  bff: ReturnType<typeof fakeBff>,
  n = 1,
): Record<string, unknown> {
  const body = bff.to('PUT', PUT)[n - 1]!.body as Record<string, unknown>
  return body.values as Record<string, unknown>
}

/** A 422 `tensor-split-format` refusing the vector `value`. */
function splitFormat(value: string) {
  return reply(422, {
    key: 'tensor-split-format',
    args: [value],
    field: 'tensor-split',
  })
}

/** Moves a row's slider to `value`. */
function drag(main: HTMLElement, id: string, value: string): HTMLInputElement {
  const slider = blocksSlider(main, id)
  fireEvent.change(slider, { target: { value } })
  return slider
}

// ── 19 G.9: checking, unchecking, moving in blocks mode ─────────────────────

test('19 G.9: checking a third row gives it the weight 1, so its count is the projection of that vector, and its range text and share follow', async () => {
  const { main } = await form(FILE_VALUES, read())
  toggle(main, C)
  await waitFor(() => expect(checkedIds(main)).toEqual([A, B, C]))
  expect(
    counts(main),
    '19 G.9: 85,15,1 over 34 places is 29, 5, 0 (the new row takes none)',
  ).toEqual([
    [A, '29'],
    [B, '5'],
    [C, '0'],
  ])
  expect(
    textItem(main, C, rangeOf(0, 34), share(0)).textContent,
    '19 G.5: the row that takes nothing says nothing',
  ).toContain(rangeOf(0, 34))
  expect(
    (shareEl(main, C)?.textContent ?? '').trim(),
    '19 G.5: its share is 0 of 34',
  ).toBe(share(0))
  expect(
    blocksSlider(main, C).value,
    '19 G.6: its slider sits at its count',
  ).toBe('0')
})

test('19 G.9: unchecking a row drops its weight, so the counts are the projection of what is left, and the ranges are recomputed', async () => {
  const { main } = await form(
    { device: `${A},${B},${C}`, 'tensor-split': '1,1,1' },
    read(),
  )
  expect(counts(main), '19 G.3: 1,1,1 is 12, 11, 11').toEqual([
    [A, '12'],
    [B, '11'],
    [C, '11'],
  ])
  toggle(main, B)
  await waitFor(() =>
    expect(counts(main), '19 G.9: 1,1 over 34 places is 17, 17').toEqual([
      [A, '17'],
      [C, '17'],
    ]),
  )
  expect(
    textItem(main, A, rangeOf(17, 0), share(17)).textContent,
    '19 G.5: the first row keeps the head of the model',
  ).toContain(rangeOf(17, 0))
  expect(
    textItem(main, C, rangeOf(17, 17), share(17)).textContent,
    '19 G.5: the row that follows takes the tail and the output layer',
  ).toContain(rangeOf(17, 17))
  expect(
    rows(main).map((r) => idOf(r.name)),
    'GD1.4 with GD1.5 and GD1.8 (19 G.9: brief 14 holds unchanged): the unchecked row stays in the list, moved back among the unchecked rows at its GD1.5 place',
  ).toEqual([A, C, B])
})

test('19 G.9: with one row checked blocks mode is over: no blocks control, no budget line, no weight field, and the share is 100 %', async () => {
  const { main } = await form(FILE_VALUES, read())
  toggle(main, B)
  await waitFor(() => expect(checkedIds(main)).toEqual([A]))
  expect(blocksInputs(main).length, '19 G.4: one row: no blocks input').toBe(0)
  expect(
    [...fieldset(main).querySelectorAll('p')].filter((p) =>
      (p.textContent ?? '').includes(
        en('model-profile-gpu-blocks-budget', `${PLACES}`, `${PLACES - 1}`),
      ),
    ).length,
    '19 G.5: no budget line',
  ).toBe(0)
  expect(weightIds(main), 'GD2.1: and no weight field').toEqual([])
  expect(
    (shareEl(main, A)?.textContent ?? '').trim(),
    'GD2.2: the one row takes everything',
  ).toBe(en('model-profile-gpu-share', '100%'))
})

test('19 G.9: Move up carries the weight with the GPU, so the counts move and the range texts follow the new order', async () => {
  const { main } = await form(FILE_VALUES, read())
  expect(counts(main), '19 G.4: 85,15 is 29, 5').toEqual([
    [A, '29'],
    [B, '5'],
  ])
  const button = move(main, B, UP)
  await waitFor(() =>
    expect(
      counts(main),
      '19 G.9: the weights move with the rows, so 15,85 projects to 6, 28',
    ).toEqual([
      [B, '6'],
      [A, '28'],
    ]),
  )
  expect(
    textItem(main, B, rangeOf(6, 0), share(6)).textContent,
    '19 G.9: the row that moved up takes the head',
  ).toContain(rangeOf(6, 0))
  expect(
    textItem(main, A, rangeOf(28, 6), share(28)).textContent,
    '19 G.9: the row that moved down takes the tail and the output layer',
  ).toContain(rangeOf(28, 6))
  expect(
    within(fieldset(main)).getByRole('status').textContent,
    'GD1.9: the move is announced in the card status region',
  ).toBe(en('model-profile-gpu-moved', B, '1', '2'))
  expect(document.activeElement, 'GD1.9: the focus stays on Move up').toBe(
    button,
  )
})

// ── 19 G.10: saving a block edit ────────────────────────────────────────────

test('19 G.10: a 422 tensor-split-format in blocks mode shows the text in the weights place, marks every visible blocks input, focuses the first one, and the bar alerts it', async () => {
  const { main } = await form(
    FILE_VALUES,
    read({
      modelProfileSave: () => splitFormat('20,14'),
    }),
  )
  drag(main, A, '20')
  fireEvent.click(saveButton(main))
  const text = en('api-tensor-split-format', '20,14')
  await within(fieldset(main)).findByText(text)
  const fs = fieldset(main)
  expect(
    before(listEl(main), within(fs).getByText(text)),
    '19 G.10: in the weights place, after the list',
  ).toBe(true)
  for (const id of [A, B])
    expectError(blocksInput(main, id), text, `19 G.10: the ${id} blocks input`)
  expect(
    document.activeElement,
    '19 G.10: focus on the first blocks input',
  ).toBe(blocksInput(main, A))
  expectAlert(main, en('save-refused', text), '19 G.10: the bar alert')
})

test('19 G.10: Discard restores the loaded weights, so the counts come back as the file projects them', async () => {
  const { main } = await form(FILE_VALUES, read())
  drag(main, A, '20')
  expect(counts(main), 'the edit').toEqual([
    [A, '20'],
    [B, '14'],
  ])
  fireEvent.click(discardButton(main))
  await waitFor(() =>
    expect(
      counts(main),
      '19 G.10: the file 85,15 projects back to 29, 5',
    ).toEqual([
      [A, '29'],
      [B, '5'],
    ]),
  )
  expect(blocksSlider(main, A).value, '19 G.10: the slider follows').toBe('29')
  expect(
    textItem(main, A, rangeOf(29, 0), share(29)).textContent,
    '19 G.10: with the range text and the share',
  ).toContain(rangeOf(29, 0))
})

// ── 19 G.11: GPU layers re-projects the rows ────────────────────────────────

test('19 G.11: GPU layers 10 moves the layout at once: the budget says 10 places, the counts are the projection over 10, and the ranges start at 24', async () => {
  const { main } = await form(FILE_VALUES, read())
  setLayers('10')
  await waitFor(() => budgetLine(main, 10))
  expect(counts(main), '19 G.11: 85,15 over 10 places is 9, 1').toEqual([
    [A, '9'],
    [B, '1'],
  ])
  expect(
    textItem(main, A, rangeOf(9, 24, 10, 24), share(9, 10)).textContent,
    '19 G.11: the blocks that stay on the GPU start where the CPU ones end',
  ).toContain(rangeOf(9, 24, 10, 24))
  expect(
    textItem(main, B, rangeOf(1, 33, 10, 24), share(1, 10)).textContent,
    '19 G.11: the last position is the output layer alone',
  ).toContain(rangeOf(1, 33, 10, 24))
  expect(
    blocksSlider(main, A).getAttribute('max'),
    '19 G.5: the slider spans the places of the layout',
  ).toBe('10')
})

test('19 G.11: a change of GPU layers alone writes no tensor-split, and the rows follow every value back', async () => {
  const { bff, main } = await form(FILE_VALUES, read())
  setLayers('10')
  await waitFor(() => budgetLine(main, 10))
  fireEvent.click(saveButton(main))
  await waitFor(() => expect(bff.to('PUT', PUT).length).toBe(1))
  const sent = sentValues(bff)
  expect(
    sent,
    '19 G.11: GPU layers is sent as the field of 02h',
  ).toHaveProperty(LAYERS, '10')
  expect(
    sent,
    '19 G.11: the re-projection writes nothing beside it',
  ).not.toHaveProperty('tensor-split')
  layersDefault()
  await waitFor(() =>
    expect(
      counts(main),
      '19 G.11: back to every layer, the counts are the whole file again',
    ).toEqual([
      [A, '29'],
      [B, '5'],
    ]),
  )
  budgetLine(main, PLACES)
})

test('19 G.11: GPU layers 0 leaves blocks mode: the weight fields come back, with the file values, and no budget line', async () => {
  const { main } = await form(FILE_VALUES, read())
  setLayers('0')
  await waitFor(() =>
    expect(weightIds(main), '19 G.4: no layout, so brief 14 as built').toEqual([
      A,
      B,
    ]),
  )
  expect(blocksInputs(main).length, '19 G.11: no blocks input').toBe(0)
  expect(
    [...fieldset(main).querySelectorAll('p')].filter((p) =>
      (p.textContent ?? '').includes(
        en('model-profile-gpu-blocks-budget', `${PLACES}`, `${PLACES - 1}`),
      ),
    ).length,
    '19 G.11: no budget line',
  ).toBe(0)
})

// ── 19 G.12: keyboard, screen reader ────────────────────────────────────────

test('19 G.12: in a blocks row the tab order is the checkbox, the Blocks text input, the slider, Move up, Move down, and Auto and Even follow the last row', async () => {
  const { main } = await form(
    { device: `${A},${B},${C}`, 'tensor-split': '1,1,1' },
    read(),
  )
  const card = cardOf(main, GPU)
  const order = mainTabbable(main).filter((el) => card.contains(el))
  const at = (el: HTMLElement) => {
    const n = order.indexOf(el)
    expect(n, `${nameOf(el)}: in the card tab order`).toBeGreaterThan(-1)
    return n
  }
  const box = rowOf(main, B).box
  const input = blocksInput(main, B)
  const slider = blocksSlider(main, B)
  const up = moveButton(main, B, UP)
  const down = moveButton(main, B, DOWN)
  const seq = [box, input, slider, up, down]
  for (const [n, el] of seq.entries())
    if (n > 0)
      expect(
        at(el) > at(seq[n - 1]!),
        `19 G.12: ${nameOf(el)} follows ${nameOf(seq[n - 1]!)}`,
      ).toBe(true)
  expect(
    at(rowOf(main, C).box) > at(down),
    '19 G.12: the next row follows the row',
  ).toBe(true)
  const auto = at(splitButton(main, AUTO))
  expect(
    auto > at(rowOf(main, C).box),
    '19 G.12: Auto follows the last row',
  ).toBe(true)
  expect(
    at(splitButton(main, EVEN)) > auto,
    '19 G.12: Even follows Auto, and they end the card',
  ).toBe(true)
})

test('19 G.12: a screen reader on the slider reads its name, its role, and the count with the places and the range, and moving it announces nothing beside its own value', async () => {
  const { main } = await form(FILE_VALUES, read())
  const slider = blocksSlider(main, A)
  expect(nameOf(slider), '19 G.12: the name').toBe(`${BLOCKS} ${A}`)
  expect(slider.getAttribute('aria-valuetext'), '19 G.12: the value line').toBe(
    en('model-profile-gpu-blocks-value', '29', `${PLACES}`, rangeOf(29, 0)),
  )
  drag(main, A, '20')
  const items = [textItem(main, A, rangeOf(20, 0), share(20)), shareEl(main, A)]
  for (const el of items) {
    expect(el, '19 G.12: the range text and the share are shown').not.toBeNull()
    expect(
      el!.getAttribute('aria-live'),
      '19 G.12: the range text and the share are plain text',
    ).toBeNull()
    expect(el!.getAttribute('role'), '19 G.12: with no role').toBeNull()
  }
  for (const s of within(fieldset(main)).queryAllByRole('status'))
    expect(s.textContent, '19 G.12: nothing announced while it moves').toBe('')
})

test('19 G.12: the blocks controls are not tabbable from anywhere else: the row holds them, and the card keeps the one status region of GD1.9', async () => {
  const { main } = await form(FILE_VALUES, read())
  for (const id of [A, B]) {
    const li = rowOf(main, id).li
    expect(
      li.contains(blocksInput(main, id)),
      `19 G.12: the ${id} input is inside its row`,
    ).toBe(true)
    expect(
      li.contains(blocksSlider(main, id)),
      `19 G.12: the ${id} slider is inside its row`,
    ).toBe(true)
    expect(
      blocksInput(main, id).tabIndex,
      `19 G.12: the ${id} input is in the tab order once`,
    ).toBe(0)
  }
  expect(
    within(fieldset(main)).queryAllByRole('status').length,
    'GD1.9: the fieldset keeps the one status region',
  ).toBe(1)
})
