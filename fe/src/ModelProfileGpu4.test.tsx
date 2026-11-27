// Brief 14, sub-slice GD2, criteria GD2.1 to GD2.10 (fe/briefs/14-gpu-distribution.md):
// the weight number fields of the checked rows (their names, `inputmode`,
// place, the loaded `tensor-split` read on `,` and `/`), GD2.1's rule on when
// they show (under none, or with fewer than two rows checked, nowhere), the
// share text beside each one and its rules, the weights following their GPU through checks,
// unchecks and moves, Auto and Even, saving `tensor-split` (its composed value,
// its loaded value and when it counts as changed), the fe's decimal check and
// its error, the server's `tensor-split-format` refusal, the `tensor-split`
// with no `device`, and the keyboard and screen-reader path of the weights.
// GD1 is in ModelProfileGpu1, Gpu2 and Gpu3.test.tsx. The helpers are those of
// ModelProfileSave15 and ModelProfileSave16.test.tsx, repeated here (a test
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
  isHidden,
  modelProfileDoc,
  reply,
  startAt,
  type BffOptions,
} from './test/bff.ts'

const ID = 'Qwen3-27B'
const HASH = `#/profiles/model/${ID}`
const PUT = `/api/profiles/model/${ID}`
const GPU = en('model-profile-gpu-distribution')
const GPUS = en('model-profile-gpus')
const UP = en('model-profile-gpu-move-up')
const DOWN = en('model-profile-gpu-move-down')
const WEIGHT = en('model-profile-gpu-weight')
const AUTO = en('model-profile-gpu-auto')
const EVEN = en('model-profile-gpu-even')
const UNUSED = en('model-profile-gpu-share-unused')
const EVEN_SHARE = en('model-profile-gpu-share-even')
const AUTO_SHARE = en('model-profile-gpu-share-auto')
const INCOMPLETE = en('model-profile-gpu-weights-incomplete')
const MODE_LABEL = en('model-profile-split-mode')
/** The three GPUs of the brief's probe, as the file spells them. */
const A = 'CUDA0'
const B = 'ROCm0'
const C = 'Vulkan0'
/** The share of a weight vector, as GD2.2 composes the text: `shares('3,1')`
 * is the two share lines of `3` and `1`, in row order. */
const shareVector = (v: string) => {
  const w = v.split(',').map((s) => Number(s))
  const sum = w.reduce((a, b) => a + b, 0)
  return w.map((n) =>
    en('model-profile-gpu-share', `${Math.round((n * 100) / sum)}%`),
  )
}

beforeEach(() => {
  startAt(HASH)
})

afterEach(() => {
  cleanup()
  startAt('')
})

// ── Helpers (as ModelProfileSave15 and ModelProfileSave16.test.tsx) ────────────────

/** Waits for the page read with the values `values` (the file) and the GPU
 * probe `DEVICES` answered. A save answers 200 and applies the sent values to
 * the next read, as the bff does. */
async function form(
  values: Record<string, string> = { device: `${A},${B}` },
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

/** Waits for the device read to land with the brief's probe time. */
async function ready(main: HTMLElement): Promise<HTMLElement> {
  const fs = await within(main).findByRole('group', { name: GPUS })
  await within(fs).findByText(asOfDate(DEVICES.probedAt!))
  return fs
}

/** The as-of line the page must write for `probedAt` (GD1.7). */
function asOf(probedAt: string): string {
  return en('model-profile-gpu-probed-at', asOfDate(probedAt))
}

/** The moment the as-of line shows for `probedAt`: the locale date and short
 * time. GD1.7 puts it in the sentence's `{0}` slot inside a `time` element, so
 * it is that element's text. */
function asOfDate(probedAt: string): string {
  return new Date(probedAt).toLocaleString(undefined, {
    dateStyle: 'medium',
    timeStyle: 'short',
  })
}

/** The as-of paragraphs of `container`, read across that `time` child: the
 * text matcher reads an element's DIRECT text children, so once the moment
 * sits in a child, `getByText(asOf(..))` matches no element at all. The whole
 * sentence is read on `textContent`, which that matcher ignores. */
function asOfLines(container: HTMLElement, probedAt: string): HTMLElement[] {
  const line = asOf(probedAt)
  return [...container.querySelectorAll('p')].filter(
    (p) => (p.textContent ?? '') === line,
  )
}

/** Every device row, in the page's order. */
function rows(main: HTMLElement): {
  li: HTMLElement
  name: string
  box: HTMLInputElement
}[] {
  const fs = fieldset(main)
  const list = fs.querySelector('ul[role="list"]')
  expect(list, 'GD1.4: a ul role="list" of the rows').not.toBeNull()
  const lis = [...list!.children].filter(
    (el) => el.tagName === 'LI',
  ) as HTMLElement[]
  return lis.map((li) => {
    const boxes = within(li).getAllByRole('checkbox')
    expect(boxes, 'GD1.4: one checkbox per row').toHaveLength(1)
    return { li, name: nameOf(boxes[0]!), box: boxes[0] as HTMLInputElement }
  })
}

/** The rows' ids, in page order. */
function rowIds(main: HTMLElement): string[] {
  return rows(main).map((r) => idOf(r.name))
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
    `the row of ${id}: the rows are ${rowIds(main).join(', ')}`,
  ).toBeDefined()
  return found!
}

/** The checked rows' ids, in page order. */
function checkedIds(main: HTMLElement): string[] {
  return rows(main)
    .filter((r) => r.box.checked)
    .map((r) => idOf(r.name))
}

/** The weight field of `id` (GD2.1): a textbox named "Weight <id>". */
function weightOf(main: HTMLElement, id: string): HTMLInputElement {
  const found = within(rowOf(main, id).li).queryByRole('textbox', {
    name: `${WEIGHT} ${id}`,
  })
  expect(
    found,
    `GD2.1: the weight field of ${id}, named "${WEIGHT} ${id}"`,
  ).not.toBeNull()
  return found as HTMLInputElement
}

/** The weight fields shown, in page order, as `[id, value]`. */
function weights(main: HTMLElement): [string, string][] {
  return [...fieldset(main).querySelectorAll<HTMLInputElement>('input')]
    .filter((el) => nameOf(el).startsWith(`${WEIGHT} `))
    .map((el) => [nameOf(el).slice(WEIGHT.length + 1), el.value])
}

/** The ids of the weight fields shown, in page order. */
function weightIds(main: HTMLElement): string[] {
  return weights(main).map(([id]) => id)
}

/** Types `value` in the weight field of `id`. */
function typeWeight(main: HTMLElement, id: string, value: string) {
  fireEvent.change(weightOf(main, id), { target: { value } })
}

/** Every element of a row with no element child and a text: a leaf, where a
 * word like the share is written. */
function leaves(li: HTMLElement): HTMLElement[] {
  return [...li.querySelectorAll<HTMLElement>('*')].filter(
    (el) =>
      el.children.length === 0 &&
      !['INPUT', 'SELECT', 'TEXTAREA', 'BUTTON'].includes(el.tagName) &&
      (el.textContent ?? '').trim() !== '',
  )
}

/** The share element of `id` (GD2.2), `null` when the row shows no share. */
function shareEl(main: HTMLElement, id: string): HTMLElement | null {
  const found = leaves(rowOf(main, id).li).filter((el) => {
    const text = (el.textContent ?? '').replace(/\s+/g, ' ').trim()
    return (
      text.startsWith('Share: ') ||
      [UNUSED, EVEN_SHARE, AUTO_SHARE].includes(text)
    )
  })
  expect(
    found.length,
    `GD2.2: at most one share line on ${id}`,
  ).toBeLessThanOrEqual(1)
  return found[0] ?? null
}

/** The share texts of the checked rows, in row order; `null` where the row
 * shows none. */
function shares(main: HTMLElement): (string | null)[] {
  return rows(main)
    .filter((r) => r.box.checked)
    .map((r) => {
      const el = shareEl(main, idOf(r.name))
      return el ? (el.textContent ?? '').replace(/\s+/g, ' ').trim() : null
    })
}

/** The share text of a percentage (GD2.2). */
function share(pct: string): string {
  return en('model-profile-gpu-share', pct)
}

/** Every button of a row, with its accessible name. */
function rowButtons(main: HTMLElement, id: string): HTMLButtonElement[] {
  // `queryAllByRole`: an unchecked row holds no button at all (GD1.9), and
  // `getAllByRole` would throw the roles dump instead of naming the row.
  return within(rowOf(main, id).li).queryAllByRole(
    'button',
  ) as HTMLButtonElement[]
}

/** The Move up (or Move down) button of `id`. */
function moveButton(
  main: HTMLElement,
  id: string,
  dir: typeof UP | typeof DOWN,
): HTMLButtonElement {
  const found = rowButtons(main, id).filter((b) => nameOf(b) === `${dir} ${id}`)
  expect(
    found,
    `GD1.9: ${dir} ${id}: the row's buttons are ${rowButtons(main, id)
      .map((b) => nameOf(b))
      .join(' | ')}`,
  ).toHaveLength(1)
  return found[0]!
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
function splitButton(main: HTMLElement, label: typeof AUTO | typeof EVEN) {
  const found = within(cardOf(main, GPU))
    .getAllByRole('button')
    .filter((b) => nameOf(b) === label)
  expect(found, `GD2.4: one ${label} in the card`).toHaveLength(1)
  return found[0] as HTMLButtonElement
}

/** The name a screen reader reads (ModelProfileSave16.test.tsx's computation,
 * with `aria-label` added). */
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
  // No label of its own, the element names itself with its content, the way an
  // assistive technology reads a button (the About toggle, GD1.9's Move
  // buttons, GD1.1's jump buttons).
  return (tied?.textContent ?? wrap?.textContent ?? el.textContent ?? '')
    .replace(/\s+/g, ' ')
    .trim()
}

/** The text each id of `aria-describedby` gives, in order. */
function described(el: HTMLElement): string[] {
  return (el.getAttribute('aria-describedby') ?? '')
    .split(/\s+/)
    .filter(Boolean)
    .map((id) => document.getElementById(id)?.textContent ?? `(no #${id})`)
}

/** The save bar status line's text. The GPUs card keeps its move status in the
 * DOM from its first render (GD1.9), so the bar's line is picked inside the
 * bar, not by counting the page's status regions (ruling of 2026-11-21). */
function statusText(main: HTMLElement): string {
  const all = within(main)
    .queryAllByRole('status')
    .filter((s) => s.closest('.save-bar') !== null)
  expect(all, 'the save bar has its status line').toHaveLength(1)
  return all[0]!.textContent ?? ''
}

/** The bar's alert holds `text` (ModelProfileSave16.test.tsx's 30a shape). */
function expectAlert(main: HTMLElement, text: string, what = text) {
  const el = within(main).getByText(text)
  const alert = el.closest('[role="alert"]')
  expect(alert, `${what} is in role="alert"`).not.toBe(null)
}

/** A control carries `text`: `aria-invalid` and `aria-describedby` (02h 142). */
function expectError(el: HTMLElement, text: string, what: string) {
  expect(el.getAttribute('aria-invalid'), `${what} aria-invalid`).toBe('true')
  expect(described(el), `${what} aria-describedby`).toContain(text)
}

function saveButton(main: HTMLElement): HTMLButtonElement {
  return within(main).getByRole('button', {
    name: en('save-button'),
  }) as HTMLButtonElement
}

/** Clicks Save changes and waits for the PUT of this model profile. */
async function save(main: HTMLElement, bff: ReturnType<typeof fakeBff>) {
  fireEvent.click(saveButton(main))
  await waitFor(() => expect(bff.to('PUT', PUT)).toHaveLength(1))
}

/** The `values` of the PUT. */
function sentValues(
  bff: ReturnType<typeof fakeBff>,
  n = 1,
): Record<string, unknown> {
  const body = bff.to('PUT', PUT)[n - 1]!.body as Record<string, unknown>
  return body.values as Record<string, unknown>
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

/** Picks a split mode by its value. */
function pickMode(main: HTMLElement, value: string) {
  fireEvent.change(modeSelect(main), { target: { value } })
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

/** Edits a field that is not in the card, so a save has something to send. */
function editCtxSize() {
  fireEvent.change(document.getElementById('model-profile-ctx-size-control')!, {
    target: { value: '65536' },
  })
}

// ── GD2.1: the weight fields ───────────────────────────────────────────────

test('GD2.1: with two rows checked and split mode tensor each checked row holds, after its label and memory line, a text input labelled Weight, named Weight plus the id in a visually hidden span, inputmode decimal, then its share; the unchecked row has none', async () => {
  const { main } = await form({ device: `${A},${B}`, 'tensor-split': '3/1' })
  const row = rowOf(main, B)
  const input = weightOf(main, B)
  expect(input.tagName, 'GD2.1: an input').toBe('INPUT')
  expect(input.type, 'decision 1: a text field, not a slider').toBe('text')
  expect(input.getAttribute('inputmode'), 'GD2.1: inputmode decimal').toBe(
    'decimal',
  )
  const label = document.querySelector(`label[for="${input.id}"]`)
  expect(
    label && (label.textContent ?? '').includes(WEIGHT),
    'GD2.1: a visible label Weight',
  ).toBe(true)
  expect(nameOf(input), 'GD2.1: the accessible name').toBe(`${WEIGHT} ${B}`)
  const hidden = [...(label?.querySelectorAll('*') ?? [])].filter((el) =>
    (el.getAttribute('class') ?? '').includes('visually-hidden'),
  )
  expect(
    hidden.map((el) => el.textContent),
    'GD2.1: the id is in a visually hidden span of the label',
  ).toContain(B)
  expect(
    before(within(row.li).getByText(/GiB/), input),
    'GD2.1: the field follows the label and the memory line',
  ).toBe(true)
  expect(
    before(input, shareEl(main, B)!),
    'GD2.1: the share follows the field',
  ).toBe(true)
  expect(
    before(input, moveButton(main, B, UP)),
    'GD2.1: Move up follows it',
  ).toBe(true)
  expect(
    weightIds(main),
    'GD2.1: one field per checked row, in row order',
  ).toEqual([A, B])
})

test.each([
  ['split mode none', { 'split-mode': 'none' }],
  ['one row checked', { device: A }],
  ['no row checked', { device: '' }],
])(
  'GD2.1: with %s no weight field shows, and no Auto or Even button',
  async (_what, values) => {
    const { main } = await form({ device: `${A},${B}`, ...values })
    expect(
      weightIds(main),
      `GD2.1: ${JSON.stringify(values)}: no weight field`,
    ).toEqual([])
  },
)

test('GD2.1: picking split mode none hides every weight field, and picking tensor back shows them with the values as typed', async () => {
  const { main } = await form({ device: `${A},${B}`, 'tensor-split': '3/1' })
  typeWeight(main, B, '9.5')
  pickMode(main, 'none')
  expect(weightIds(main), 'GD2.1: none: no field').toEqual([])
  pickMode(main, 'tensor')
  expect(weights(main), 'GD2.1: the weights are kept as typed').toEqual([
    [A, '3'],
    [B, '9.5'],
  ])
})

test('GD2.1: unchecking one of the two file GPUs hides both weight fields, and checking it back shows them with the values: a split of one GPU has no weights to give', async () => {
  const { main } = await form({ device: `${A},${B}`, 'tensor-split': '3/1' })
  expect(weightIds(main), 'GD2.1: two checked rows: the two fields').toEqual([
    A,
    B,
  ])
  toggle(main, B)
  expect(
    weightIds(main),
    'GD2.1: one row checked, whatever the file names: no weight field',
  ).toEqual([])
  expect(
    within(fieldset(main))
      .queryAllByRole('button')
      .filter((b) => nameOf(b) === AUTO || nameOf(b) === EVEN),
    'GD2.4: no Auto, no Even',
  ).toHaveLength(0)
  expect(shares(main), 'GD2.2: the row left is all of it').toEqual([
    share('100%'),
  ])
  toggle(main, B)
  expect(
    weights(main),
    'GD2.1: checking it back shows the fields as typed',
  ).toEqual([
    [A, '3'],
    [B, '1'],
  ])
})

test.each([
  ['3/1', ['3', '1']],
  ['3,1', ['3', '1']],
  ['3/1/5 (extra entries are not shown)', ['3', '1']],
  ['1.5,029 (each entry as written)', ['1.5', '029']],
  [' 3 , 1 (each entry trimmed)', ['3', '1']],
  ['3, (a missing entry is the empty string)', ['3', '']],
  ['absent (both empty)', ['', '']],
])(
  'GD2.1: a read of tensor-split %s loads the weights of the device list as %j',
  async (split, want) => {
    const values: Record<string, string> = { device: `${A},${B}` }
    if (split !== 'absent (both empty)') values['tensor-split'] = split
    const { main } = await form(values)
    expect(weights(main), `GD2.1: tensor-split ${split}`).toEqual([
      [A, want[0]!],
      [B, want[1]!],
    ])
  },
)

// ── GD2.2: the share ───────────────────────────────────────────────────────

test.each([
  ['3,1', shareVector('3,1')],
  ['1,1', shareVector('1,1')],
  ['1,3', shareVector('1,3')],
  ['0.5,0.5', shareVector('0.5,0.5')],
  ['0,0', [EVEN_SHARE, EVEN_SHARE]],
  [',', [EVEN_SHARE, EVEN_SHARE]],
  ['absent', [EVEN_SHARE, EVEN_SHARE]],
  ['3,x (a malformed weight beside a number)', [null, null]],
  ['x,y', [null, null]],
])(
  'GD2.2: two rows checked under tensor, tensor-split %s: the share is %j',
  async (split, want) => {
    // GD2.2: every weight empty or 0 is `even` only under `tensor`, `auto`
    // under every other split mode, so the title's "under tensor" is in the
    // file: split-mode is the key that picks the word.
    const values: Record<string, string> = {
      device: `${A},${B}`,
      'split-mode': 'tensor',
    }
    if (split !== 'absent') values['tensor-split'] = split
    const { main } = await form(values)
    expect(shares(main), `GD2.2: tensor-split ${split}`).toEqual(want)
  },
)

test('GD2.2: one row checked shows Share: 100% on it, with no weight field beside it', async () => {
  const { main } = await form({ device: A, 'tensor-split': '3' })
  expect(shares(main), 'GD2.2: the single row is all of it').toEqual([
    share('100%'),
  ])
  expect(weightIds(main), 'GD2.1: one checked row: no weight field').toEqual([])
})

test('GD2.2: under split mode none the first checked row reads Share: 100% and the others unused', async () => {
  const { main } = await form({
    device: `${A},${B},${C}`,
    'split-mode': 'none',
  })
  expect(
    shares(main),
    'GD2.2: none: the first is all, the rest unused',
  ).toEqual([share('100%'), UNUSED, UNUSED])
})

test.each([
  ['tensor', 'tensor', [EVEN_SHARE, EVEN_SHARE]],
  ['row', 'row', [AUTO_SHARE, AUTO_SHARE]],
  // GD2.2: under `none` the share is 100% on the first checked row and
  // `model-profile-gpu-share-unused` on the others, whatever the weights.
  ['none', 'none', [share('100%'), UNUSED]],
])(
  'GD2.2: split mode %s, two rows checked, every weight empty: the share is %j',
  async (_what, mode, want) => {
    const { main } = await form({
      device: `${A},${B}`,
      'split-mode': mode,
    })
    expect(shares(main), `GD2.2: split-mode ${mode}`).toEqual(want)
  },
)

test.each([
  ['the file blank, so layer by default', ''],
  ['even', 'even'],
])(
  'GD2.2: split mode %s, two rows checked, every weight empty: the share is auto on both, only tensor cuts evenly',
  async (_what, mode) => {
    const { main } = await form({ device: `${A},${B}`, 'split-mode': mode })
    expect(shares(main), `GD2.2: split-mode ${mode || 'blank'}`).toEqual([
      AUTO_SHARE,
      AUTO_SHARE,
    ])
  },
)

test('GD2.2: with no row checked there is no share anywhere', async () => {
  const { main } = await form({ device: '', 'tensor-split': '3,1' })
  expect(shares(main), 'GD2.2: no checked row').toEqual([])
  expect(
    leaves(fieldset(main)).filter((el) =>
      (el.textContent ?? '').trim().startsWith('Share: '),
    ),
    'GD2.2: no share line in the fieldset',
  ).toHaveLength(0)
})

// ── The weight group of a row (DESIGN.md 5.2 "GPU list") ────────────────────

test('GD2.1 GD2.2 (DESIGN.md 5.2 "GPU list", brief 14 browser review): each checked row with a weight field holds its Weight label, its field and its share in one element that is a direct child of the li, so the row wraps around the group and never between the label and the field', async () => {
  const { main } = await form({
    device: `${A},${B},${C}`,
    'split-mode': 'tensor',
    'tensor-split': '3,1,2',
  })
  expect(
    weightIds(main),
    'the three checked rows each hold a weight field',
  ).toEqual([A, B, C])
  for (const id of weightIds(main)) {
    const input = weightOf(main, id)
    const label = document.querySelector(`label[for="${input.id}"]`)
    const share = shareEl(main, id)
    expect(label, `${id}: its Weight label`).not.toBeNull()
    expect(share, `${id}: its share line`).not.toBeNull()
    // The brief's locator: the row's direct child that holds the field. The
    // field, its label and its share are today three items of the li's
    // wrapping flex, so this answers the field itself.
    const group = input.closest('li > *')
    expect(group, `${id}: the field is inside the row`).not.toBeNull()
    expect(
      group === input,
      `${id}: the field, its label and its share share one wrapper (the field is a direct child of the li, so the label and the field break apart at 1280 px)`,
    ).toBe(false)
    expect(
      group!.parentElement,
      `${id}: the wrapper is a direct child of the li`,
    ).toBe(rowOf(main, id).li)
    expect(
      group!.contains(label!),
      `${id}: the Weight label is inside the wrapper`,
    ).toBe(true)
    expect(
      group!.contains(share!),
      `${id}: the share is inside the wrapper`,
    ).toBe(true)
    // The checkbox, the memory line and the Move buttons stay items of the row
    // beside the group: the group is the weight field with its share, not the
    // whole row. Green today, a guard on the fix.
    expect(
      group!.contains(rowOf(main, id).box),
      `${id}: the row checkbox stays outside the group`,
    ).toBe(false)
  }
})

/** Answers the save with the refusal of GD2.7, quoting the `tensor-split` the
 * page sent (the bff quotes the value the manager refused). */
const AS_SENT: BffOptions['modelProfileSave'] = (body) =>
  splitFormat(
    String(
      (body as { values?: Record<string, unknown> })?.values?.[
        'tensor-split'
      ] ?? '',
    ),
  )

test('GD2.2: the share is the first id of the weight aria-describedby, an error id after it, and the share is not a live region', async () => {
  const { main } = await form(
    { device: `${A},${B}`, 'tensor-split': '3/1' },
    { modelProfileSave: AS_SENT },
  )
  const a = weightOf(main, A)
  expect(described(a)[0], 'GD2.2: the share is the first id').toMatch(
    /^Share: /,
  )
  const el = shareEl(main, A)!
  expect(el.getAttribute('role'), 'GD2.2: not a status').toBeNull()
  expect(el.getAttribute('aria-live'), 'GD2.2: not live').toBeNull()
  // A refusal on tensor-split with the shares showing: its text joins the
  // description, after the share.
  move(main, B, UP)
  fireEvent.click(saveButton(main))
  const text = splitFormatText('1,3')
  await within(fieldset(main)).findByText(text)
  const now = described(weightOf(main, B))
  expect(now[0], 'GD2.2: the share is still first').toMatch(/^Share: /)
  expect(now, 'GD2.2: the error id follows it').toContain(text)
})

/**
 * GD2.2: "when every weight of the checked rows is a number of 0 or more
 * (GD2.6's pattern) ... else ... no share text on any row". A weight the
 * pattern refuses counts as no number, even when JavaScript reads it as one:
 * each of these beside a good 5 must give no share on any row.
 */
test.each([
  ['-1 (a negative weight)', '-1'],
  ['1e3 (an exponent)', '1e3'],
  ['+1 (a signed weight)', '+1'],
  ['0x10 (a hexadecimal weight)', '0x10'],
  ['a whitespace-only weight', ' '],
])(
  'GD2.2: tensor-split %s beside 5: no share on any row',
  async (_label, bad) => {
    const { main } = await form({ device: `${A},${B}`, 'tensor-split': '3/1' })
    // 5 beside it, so a negative weight cannot hide behind a sum of 0.
    typeWeight(main, B, '5')
    typeWeight(main, A, bad)
    expect(shares(main), `GD2.2: tensor-split ${bad},5: no share`).toEqual([
      null,
      null,
    ])
    expect(
      leaves(fieldset(main)).filter((el) =>
        (el.textContent ?? '').trim().startsWith('Share: '),
      ),
      `GD2.2: tensor-split ${bad},5: no share line in the fieldset`,
    ).toHaveLength(0)
  },
)

test('GD2.2: the share updates as the user types', async () => {
  const { main } = await form({ device: `${A},${B}`, 'tensor-split': '3/1' })
  expect(shares(main), 'the load').toEqual(shareVector('3,1'))
  typeWeight(main, A, '1')
  expect(shares(main), 'GD2.2: 1 and 1').toEqual(shareVector('1,1'))
  typeWeight(main, A, '')
  expect(
    shares(main),
    'GD2.2: an empty weight beside a number: no share',
  ).toEqual([null, null])
  typeWeight(main, A, '0')
  expect(shares(main), 'GD2.2: 0 and 1').toEqual(shareVector('0,1'))
})

// ── GD2.3: the weights follow their GPU ────────────────────────────────────

test('GD2.3: a newly checked row gets 1 when another checked row has a weight, and the empty string when none has', async () => {
  const { main } = await form({ device: `${A},${B}`, 'tensor-split': '3/1' })
  toggle(main, C)
  expect(weights(main), 'GD2.3: the new row joins at 1').toEqual([
    [A, '3'],
    [B, '1'],
    [C, '1'],
  ])
  fireEvent.click(splitButton(main, AUTO))
  expect(weights(main), 'GD2.3: Auto emptied every weight').toEqual([
    [A, ''],
    [B, ''],
    [C, ''],
  ])
  toggle(main, A)
  toggle(main, A)
  expect(
    weights(main),
    'GD2.3: with no weight typed, the new row is empty',
  ).toEqual([
    [B, ''],
    [C, ''],
    [A, ''],
  ])
})

test('GD2.3: unchecking a row drops its weight, checking it again follows the rule, and Move up and Move down carry the weight with the row', async () => {
  const { main } = await form({ device: `${A},${B}`, 'tensor-split': '3/1' })
  toggle(main, B)
  // GD2.1: with fewer than two rows checked no weight field shows at all, so
  // "the weight is gone with the row" is pinned as no field, and GD2.1's keep
  // as typed shows up in what comes back when the row is checked again.
  expect(
    weightIds(main),
    'GD2.3 GD2.1: one row checked: no weight field is shown',
  ).toEqual([])
  toggle(main, B)
  expect(weights(main), 'GD2.3: the row is back, last, at 1').toEqual([
    [A, '3'],
    [B, '1'],
  ])
  move(main, B, UP)
  expect(weights(main), 'GD2.3: the weight moved with the row').toEqual([
    [B, '1'],
    [A, '3'],
  ])
  move(main, B, DOWN)
  expect(weights(main), 'GD2.3: and back').toEqual([
    [A, '3'],
    [B, '1'],
  ])
})

// ── GD2.4: Auto and Even ───────────────────────────────────────────────────

test('GD2.4: Auto and Even are two secondary buttons after the list: Auto empties every weight, Even sets each to 1, and the focus stays on the pressed one', async () => {
  const { main } = await form({ device: `${A},${B}`, 'tensor-split': '3/1' })
  const list = fieldset(main).querySelector('ul[role="list"]')!
  const auto = splitButton(main, AUTO)
  const even = splitButton(main, EVEN)
  expect(before(list, auto), 'GD2.4: Auto is after the list').toBe(true)
  expect(before(list, even), 'GD2.4: Even is after the list').toBe(true)
  auto.focus()
  fireEvent.click(auto)
  expect(weights(main), 'GD2.4: Auto empties them').toEqual([
    [A, ''],
    [B, ''],
  ])
  expect(document.activeElement, 'GD2.4: the focus stayed on Auto').toBe(auto)
  even.focus()
  fireEvent.click(even)
  expect(weights(main), 'GD2.4: Even sets each to 1').toEqual([
    [A, '1'],
    [B, '1'],
  ])
  expect(document.activeElement, 'GD2.4: the focus stayed on Even').toBe(even)
})

test('GD2.4 GD1.4 GD1.12: Auto and Even are no entries of the list of rows: they sit after the list and after the fieldset status region, so they follow every row in tab order', async () => {
  const { main } = await form({ device: `${A},${B}`, 'tensor-split': '3/1' })
  const fs = fieldset(main)
  const list = fs.querySelector('ul[role="list"]')!
  const auto = splitButton(main, AUTO)
  const even = splitButton(main, EVEN)
  expect(
    list.contains(auto) || list.contains(even),
    'GD1.4: the two buttons are not children of the list, whatever is checked: the list holds the GPU rows',
  ).toBe(false)
  const status = within(fs).getAllByRole('status')
  expect(
    status,
    'GD1.9: the fieldset keeps its one status region',
  ).toHaveLength(1)
  expect(
    before(status[0]!, auto),
    'GD1.4: the status region follows the list, and Auto follows it',
  ).toBe(true)
  expect(before(auto, even), 'GD2.4: Auto first, then Even').toBe(true)
})

test.each([
  ['split mode none', { 'split-mode': 'none' }],
  ['one row checked', { device: A }],
  ['no row checked', { device: '' }],
])('GD2.4: with %s there is no Auto and no Even', async (_what, values) => {
  const { main } = await form({ device: `${A},${B}`, ...values })
  const names = within(cardOf(main, GPU))
    .getAllByRole('button')
    .map((b) => nameOf(b))
  for (const label of [AUTO, EVEN])
    expect(
      names.filter((n) => n === label),
      `GD2.4: ${label} is not shown`,
    ).toHaveLength(0)
})

test('GD2.4: Auto, Even and a change of the checked rows clear the weights error', async () => {
  for (const [what, act] of [
    ['Auto', (main: HTMLElement) => fireEvent.click(splitButton(main, AUTO))],
    ['Even', (main: HTMLElement) => fireEvent.click(splitButton(main, EVEN))],
    ['a check', (main: HTMLElement) => toggle(main, C)],
  ] as [string, (main: HTMLElement) => void][]) {
    cleanup()
    const { main } = await form({ device: `${A},${B}`, 'tensor-split': '3/1' })
    // A weight typed: tensor-split is changed, so the check applies.
    typeWeight(main, B, 'x')
    fireEvent.click(saveButton(main))
    await within(fieldset(main)).findByText(INCOMPLETE)
    act(main)
    expect(
      within(fieldset(main)).queryByText(INCOMPLETE),
      `GD2.4: ${what} clears the error`,
    ).toBeNull()
  }
})

// ── GD2.5: saving tensor-split ─────────────────────────────────────────────

test('GD2.5: device A,B with tensor-split 3/1 untouched sends neither key', async () => {
  const { bff, main } = await form({
    device: `${A},${B}`,
    'tensor-split': '3/1',
  })
  expect(statusText(main), 'GD2.5: the load is clean').toBe('')
  editCtxSize()
  await save(main, bff)
  expect(sentValues(bff), 'GD2.5: the edited key only').toEqual({
    'ctx-size': '65536',
  })
})

test('GD2.5: a short vector the user does not touch (device A,B,C, tensor-split 3,1) is not sent', async () => {
  const { bff, main } = await form({
    device: `${A},${B},${C}`,
    'tensor-split': '3,1',
  })
  expect(weights(main), 'the load: the third weight is empty').toEqual([
    [A, '3'],
    [B, '1'],
    [C, ''],
  ])
  expect(statusText(main), 'GD2.5: the load is clean').toBe('')
  editCtxSize()
  await save(main, bff)
  expect(sentValues(bff), 'GD2.5: only the edited key').toEqual({
    'ctx-size': '65536',
  })
})

test('GD2.5: Move up on the second row sends device B,A and tensor-split 1,3', async () => {
  const { bff, main } = await form({
    device: `${A},${B}`,
    'tensor-split': '3/1',
  })
  move(main, B, UP)
  await save(main, bff)
  expect(sentValues(bff)).toEqual({
    device: `${B},${A}`,
    'tensor-split': '1,3',
  })
})

test('GD2.5: unchecking the second row sends device A and tensor-split null', async () => {
  const { bff, main } = await form({
    device: `${A},${B}`,
    'tensor-split': '3/1',
  })
  toggle(main, B)
  await save(main, bff)
  expect(sentValues(bff)).toEqual({ device: A, 'tensor-split': null })
})

test('GD2.5: unchecking every row sends device null and tensor-split null', async () => {
  const { bff, main } = await form({
    device: `${A},${B}`,
    'tensor-split': '3/1',
  })
  toggle(main, A)
  toggle(main, B)
  await save(main, bff)
  expect(sentValues(bff)).toEqual({ device: null, 'tensor-split': null })
})

test('GD2.5: a weight typed as it was loaded is no change, and a weight edited is sent trimmed, with the others in row order', async () => {
  const first = await form({ device: `${A},${B}`, 'tensor-split': '3/1' })
  typeWeight(first.main, A, '3')
  expect(statusText(first.main), 'GD2.5: the same value is no change').toBe('')
  editCtxSize()
  await save(first.main, first.bff)
  expect(
    'tensor-split' in sentValues(first.bff),
    'GD2.5: an untouched weight sends nothing',
  ).toBe(false)
  cleanup()
  const second = await form({ device: `${A},${B}`, 'tensor-split': '3/1' })
  typeWeight(second.main, A, ' 5 ')
  expect(statusText(second.main), 'GD2.5: the edit is a change').toBe(
    en('save-unsaved'),
  )
  await save(second.main, second.bff)
  expect(
    sentValues(second.bff)['tensor-split'],
    'GD2.5: trimmed, in row order',
  ).toBe('5,1')
})

test('GD2.5: every weight cleared composes null, which is sent in place of the loaded vector', async () => {
  const { bff, main } = await form({
    device: `${A},${B}`,
    'tensor-split': '3/1',
  })
  fireEvent.click(splitButton(main, AUTO))
  expect(statusText(main), 'GD2.5: clearing is a change').toBe(
    en('save-unsaved'),
  )
  await save(main, bff)
  expect(
    sentValues(bff)['tensor-split'],
    'GD2.5: all empty composes null',
  ).toBeNull()
})

test('GD2.5: tensor-split counts for the save bar and Discard as any field: Discard restores the loaded weights, shares and order', async () => {
  const { main } = await form({ device: `${A},${B}`, 'tensor-split': '3/1' })
  typeWeight(main, A, '9')
  move(main, B, UP)
  expect(statusText(main), 'GD2.5: the edits are a change').toBe(
    en('save-unsaved'),
  )
  fireEvent.click(
    within(main).getByRole('button', { name: en('save-discard') }),
  )
  await waitFor(() =>
    expect(weights(main), 'GD2.5: Discard restored the weights').toEqual([
      [A, '3'],
      [B, '1'],
    ]),
  )
  expect(statusText(main), 'GD2.5: the form is clean again').toBe('')
  expect(shares(main), 'GD2.5: and the shares').toEqual(shareVector('3,1'))
})

// ── GD2.6: the fe check ────────────────────────────────────────────────────

test('GD2.6: a weight that is not a number of 0 or more stops the save: nothing sent, the error after the list, every failing field marked, focus on the first failing field, and the bar as every check', async () => {
  const { bff, main } = await form({
    device: `${A},${B}`,
    'tensor-split': '3/1',
  })
  typeWeight(main, B, 'x')
  fireEvent.click(saveButton(main))
  await within(fieldset(main)).findByText(INCOMPLETE)
  expect(bff.to('PUT', PUT), 'GD2.6: nothing sent').toHaveLength(0)
  const fs = fieldset(main)
  const line = within(fs).getByText(INCOMPLETE)
  const list = fs.querySelector('ul[role="list"]')!
  expect(line.tagName, 'GD2.6: a paragraph').toBe('P')
  expect(line.className, 'GD2.6: a D5.2 Error').toContain('field-error')
  expect(before(list, line), 'GD2.6: the error is after the list').toBe(true)
  expectError(weightOf(main, B), INCOMPLETE, 'GD2.6: the x field')
  expect(
    weightOf(main, A).getAttribute('aria-invalid'),
    'GD2.6: the good field is not marked',
  ).not.toBe('true')
  expect(
    document.activeElement,
    'GD2.6: focus on the first failing field',
  ).toBe(weightOf(main, B))
  expectAlert(main, en('save-invalid'), 'GD2.6: the bar of a check')
})

test.each(['1.5', '.5', '3.', '029', '0', '123456789012345'])(
  'GD2.6: a weight of %s passes the check and is sent as written',
  async (good) => {
    const { bff, main } = await form({
      device: `${A},${B}`,
      'tensor-split': '3/1',
    })
    typeWeight(main, A, good)
    await save(main, bff)
    expect(
      within(fieldset(main)).queryByText(INCOMPLETE),
      `GD2.6: ${good}: no error`,
    ).toBeNull()
    expect(sentValues(bff)['tensor-split'], `GD2.6: ${good} is sent`).toBe(
      `${good},1`,
    )
  },
)

test.each(['x', '1e3', '-1', '0,5', '1 2', '+1', '1.2.3', '.'])(
  'GD2.6: a weight of %s fails the check: the save sends nothing',
  async (bad) => {
    const { bff, main } = await form({
      device: `${A},${B}`,
      'tensor-split': '3/1',
    })
    typeWeight(main, A, bad)
    fireEvent.click(saveButton(main))
    await within(fieldset(main)).findByText(INCOMPLETE)
    expect(bff.to('PUT', PUT), `GD2.6: ${bad}: nothing sent`).toHaveLength(0)
    expect(document.activeElement, `GD2.6: ${bad}: focus on the field`).toBe(
      weightOf(main, A),
    )
  },
)

/**
 * GD2.6: "when `tensor-split` is changed and composed non-null: each checked
 * row's weight must match ...". Once one weight is written, the vector is
 * composed, so an empty weight beside it is a weight that does not match: it
 * fails the check and stops the save. The one exempt state is all-empty, the
 * Auto vector composed `null` (the test below this pair).
 */
test('GD2.6: an empty weight beside a number fails the check: the save sends nothing, the empty field is marked, and Auto is the exempt state', async () => {
  const { bff, main } = await form({
    device: `${A},${B}`,
    'tensor-split': '3/1',
  })
  typeWeight(main, A, '')
  expect(statusText(main), 'GD2.6: clearing a weight is a change').toBe(
    en('save-unsaved'),
  )
  fireEvent.click(saveButton(main))
  await within(fieldset(main)).findByText(INCOMPLETE)
  expect(
    bff.to('PUT', PUT),
    'GD2.6: an empty weight beside a number sends nothing',
  ).toHaveLength(0)
  expectError(weightOf(main, A), INCOMPLETE, 'GD2.6: the empty field is marked')
  expect(
    weightOf(main, B).getAttribute('aria-invalid'),
    'GD2.6: the weight beside it is not marked',
  ).not.toBe('true')
  expect(
    document.activeElement,
    'GD2.6: focus on the first failing field',
  ).toBe(weightOf(main, A))
  expectAlert(main, en('save-invalid'), 'GD2.6: the bar of a check')
  // The exempt state, in the same form: every weight cleared composes null,
  // the Auto vector, which the check does not check.
  fireEvent.click(splitButton(main, AUTO))
  await save(main, bff)
  expect(
    sentValues(bff)['tensor-split'],
    'GD2.6: all empty composes null, and goes out',
  ).toBeNull()
})

test('GD2.6: with three rows checked and a weight typed only in the first, the empty third weight fails the check and nothing is sent', async () => {
  const { bff, main } = await form({
    device: `${A},${B},${C}`,
    'tensor-split': '3,1',
  })
  expect(weights(main), 'the load: the third weight is empty').toEqual([
    [A, '3'],
    [B, '1'],
    [C, ''],
  ])
  typeWeight(main, A, '5')
  fireEvent.click(saveButton(main))
  await within(fieldset(main)).findByText(INCOMPLETE)
  expect(
    bff.to('PUT', PUT),
    'GD2.6: 5,1, composed of a short vector sends nothing',
  ).toHaveLength(0)
  expectError(weightOf(main, C), INCOMPLETE, 'GD2.6: the empty field is marked')
  for (const id of [A, B])
    expect(
      weightOf(main, id).getAttribute('aria-invalid'),
      `GD2.6: the ${id} weight is not marked`,
    ).not.toBe('true')
  expect(
    document.activeElement,
    'GD2.6: focus on the first failing field',
  ).toBe(weightOf(main, C))
})

test('GD2.6: every weight empty composes null, which the check does not check: the save goes out', async () => {
  const { bff, main } = await form({
    device: `${A},${B}`,
    'tensor-split': '3/1',
  })
  fireEvent.click(splitButton(main, AUTO))
  await save(main, bff)
  expect(within(fieldset(main)).queryByText(INCOMPLETE), 'no error').toBeNull()
  expect(sentValues(bff)['tensor-split'], 'null goes out').toBeNull()
})

test('GD2.6: typing in a weight field clears the error, and the bar alert stays', async () => {
  const { main } = await form({ device: `${A},${B}`, 'tensor-split': '3/1' })
  typeWeight(main, B, 'x')
  fireEvent.click(saveButton(main))
  await within(fieldset(main)).findByText(INCOMPLETE)
  typeWeight(main, B, '2')
  expect(
    within(fieldset(main)).queryByText(INCOMPLETE),
    'GD2.6: typing clears it',
  ).toBeNull()
  expect(
    weightOf(main, B).getAttribute('aria-invalid'),
    'GD2.6: the mark is gone',
  ).not.toBe('true')
  expect(
    within(main).queryAllByRole('alert').length,
    'GD2.6: the alert stays',
  ).toBe(1)
})

test('GD2.6: when the device error and the weights error both show, the weights error sits after the device error, and the focus goes to the failing weight field', async () => {
  const { bff, main } = await form(
    { device: `${A},CUDA9`, 'tensor-split': '3/1' },
    {
      modelProfileSave: () =>
        reply(422, {
          key: 'device-bad-name',
          args: ['device', 'CUDA9'],
          field: 'device',
        }),
    },
  )
  // Checking a third GPU changes the device state, so Save goes out and the
  // manager refuses the name: the device error shows (GD1.11).
  toggle(main, C)
  fireEvent.click(saveButton(main))
  const deviceText = en('api-device-bad-name', 'device', 'CUDA9')
  await within(fieldset(main)).findByText(deviceText)
  expect(bff.to('PUT', PUT), 'the first save went out').toHaveLength(1)
  // A bad weight, then Save: the fe check stops it, and both errors show.
  typeWeight(main, 'CUDA9', 'x')
  fireEvent.click(saveButton(main))
  await within(fieldset(main)).findByText(INCOMPLETE)
  expect(
    bff.to('PUT', PUT),
    'GD2.6: the second save is stopped by the check',
  ).toHaveLength(1)
  const deviceLine = within(fieldset(main)).getByText(deviceText)
  const weightsLine = within(fieldset(main)).getByText(INCOMPLETE)
  expect(
    before(deviceLine, weightsLine),
    'GD2.6: the weights error is after the device error',
  ).toBe(true)
  expect(
    document.activeElement,
    'GD2.6: focus on the failing weight field',
  ).toBe(weightOf(main, 'CUDA9'))
  expectAlert(main, en('save-invalid'), 'GD2.6: the bar of a check')
})

/**
 * GD2.6's clearing rule, read whole: "typing, checking, unchecking or moving a
 * checked row clears it". A Move rewrites `device` (GD1.9), so it is a change of
 * the checked rows, and the weights' error goes with it as it does under a
 * checkbox (GD1.11) and under Auto/Even (GD2.4). The `tensor-split` error of
 * GD2.7's refusal is the same error in the same place, marked on every visible
 * weight, so it clears on the Move too.
 */
test('GD2.6: Move up on a checked row clears the weights error and the weight marks, of the fe check and of the 422 tensor-split-format', async () => {
  // The fe's check, marked and focused: GD2.6's first test.
  const { main } = await form({ device: `${A},${B}`, 'tensor-split': '3/1' })
  typeWeight(main, B, 'x')
  fireEvent.click(saveButton(main))
  await within(fieldset(main)).findByText(INCOMPLETE)
  expectError(
    weightOf(main, B),
    INCOMPLETE,
    'GD2.6: the x field, before the Move',
  )
  move(main, B, UP)
  expect(
    within(fieldset(main)).queryByText(INCOMPLETE),
    'GD2.6: a Move clears the weights error',
  ).toBeNull()
  for (const id of [A, B])
    expect(
      weightOf(main, id).getAttribute('aria-invalid'),
      `GD2.6: a Move clears the ${id} mark`,
    ).not.toBe('true')

  // The server's refusal of the same key, marked on every visible weight.
  cleanup()
  const second = await form(
    { device: `${A},${B}`, 'tensor-split': '3/1' },
    { modelProfileSave: () => splitFormat('3.5,1') },
  )
  typeWeight(second.main, A, '3.5')
  fireEvent.click(saveButton(second.main))
  const text = splitFormatText('3.5,1')
  await within(fieldset(second.main)).findByText(text)
  expectError(
    weightOf(second.main, B),
    text,
    'GD2.7: the refusal marks the field, before the Move',
  )
  move(second.main, B, UP)
  expect(
    within(fieldset(second.main)).queryByText(text),
    'GD2.6: a Move clears the tensor-split-format error',
  ).toBeNull()
  for (const id of [A, B])
    expect(
      weightOf(second.main, id).getAttribute('aria-invalid'),
      `GD2.6: a Move clears the ${id} mark of the refusal`,
    ).not.toBe('true')
})

// ── GD2.7: the server refusals ─────────────────────────────────────────────

/** The refusal of GD2.7, for the value `value`. */
function splitFormat(value: string) {
  return reply(422, {
    key: 'tensor-split-format',
    args: [value],
    field: 'tensor-split',
  })
}

function splitFormatText(value: string) {
  return en('api-tensor-split-format', value)
}

test('GD2.7: a 422 tensor-split-format with field tensor-split shows the text in the weights place, marks every visible weight field, focuses the first one, and the bar alerts it', async () => {
  const { main } = await form(
    { device: `${A},${B}`, 'tensor-split': '3/1' },
    { modelProfileSave: () => splitFormat('3.5,1') },
  )
  typeWeight(main, A, '3.5')
  fireEvent.click(saveButton(main))
  const text = splitFormatText('3.5,1')
  await within(fieldset(main)).findByText(text)
  const fs = fieldset(main)
  const list = fs.querySelector('ul[role="list"]')!
  expect(
    before(list, within(fs).getByText(text)),
    'GD2.7: in the weights place, after the list',
  ).toBe(true)
  for (const id of [A, B])
    expectError(weightOf(main, id), text, `GD2.7: the ${id} field`)
  expect(document.activeElement, 'GD2.7: focus on the first weight field').toBe(
    weightOf(main, A),
  )
  expectAlert(main, en('save-refused', text), 'GD2.7: the bar alert')
})

test('GD2.7: with the weight fields hidden under split mode none, the same refusal focuses the first checked checkbox and marks nothing', async () => {
  const { main } = await form(
    { device: `${A},${B},${C}`, 'split-mode': 'none', 'tensor-split': '3,1,5' },
    { modelProfileSave: () => splitFormat('1,3,5') },
  )
  expect(weightIds(main), 'GD2.7: none: no weight field shows').toEqual([])
  move(main, B, UP)
  fireEvent.click(saveButton(main))
  const text = splitFormatText('1,3,5')
  await within(fieldset(main)).findByText(text)
  expect(
    document.activeElement,
    'GD2.7: focus on the first checked checkbox',
  ).toBe(rowOf(main, B).box)
  for (const id of [A, B, C])
    expect(
      rowOf(main, id).box.getAttribute('aria-invalid'),
      `GD2.7: ${id}: no weight field to mark`,
    ).not.toBe('true')
})

test('GD2.7: with no GPU checked, so neither a weight field nor a checkbox is there, the refusal focuses the split mode select, and the fieldset carries the text', async () => {
  const { bff, main } = await form(
    { device: '', 'tensor-split': 'x' },
    { modelProfileSave: () => splitFormat('x') },
  )
  // Any other change carries the save out; the refusal lands on tensor-split.
  pickMode(main, 'row')
  fireEvent.click(saveButton(main))
  const text = splitFormatText('x')
  await within(fieldset(main)).findByText(text)
  expect(bff.to('PUT', PUT), 'the save went out').toHaveLength(1)
  expect(weightIds(main), 'GD2.7: no weight field shows').toEqual([])
  expect(
    rows(main).filter((r) => r.box.checked),
    'GD2.7: no row checked',
  ).toHaveLength(0)
  expect(
    document.activeElement,
    'GD2.7: focus falls to the split mode select',
  ).toBe(modeSelect(main))
  for (const r of rows(main))
    expect(
      r.box.getAttribute('aria-invalid'),
      `GD2.7: ${r.name}: the checkboxes are not marked`,
    ).not.toBe('true')
})

test('GD2.7: with one GPU checked the refusal focuses its checkbox, the only control there is', async () => {
  const { main } = await form(
    { device: A, 'tensor-split': 'x' },
    { modelProfileSave: () => splitFormat('x') },
  )
  pickMode(main, 'row')
  fireEvent.click(saveButton(main))
  const text = splitFormatText('x')
  await within(fieldset(main)).findByText(text)
  expect(weightIds(main), 'GD2.7: one checked row: no weight field').toEqual([])
  expect(
    document.activeElement,
    'GD2.7: focus on the first checked checkbox',
  ).toBe(rowOf(main, A).box)
})

test('GD2.7: moe-cache-with-tensor-split is unchanged: it marks MoE cache, and the GPUs fieldset and the weights show nothing', async () => {
  const text = en('api-moe-cache-with-tensor-split')
  const { bff, main } = await form(
    { device: `${A},${B}`, 'tensor-split': '3/1' },
    {
      modelProfileSave: () =>
        reply(422, {
          key: 'moe-cache-with-tensor-split',
          args: [],
          field: 'moe-cache-mib',
        }),
    },
  )
  editCtxSize()
  fireEvent.click(saveButton(main))
  await within(main).findByText(text)
  expectError(
    document.getElementById('model-profile-moe-cache-mib-control')!,
    text,
    'GD2.7: MoE cache keeps 144',
  )
  expect(
    within(fieldset(main)).queryByText(text),
    'GD2.7: the GPUs fieldset shows nothing',
  ).toBeNull()
  for (const id of [A, B])
    expect(
      weightOf(main, id).getAttribute('aria-invalid'),
      `GD2.7: the ${id} weight is not marked`,
    ).not.toBe('true')
  expect(bff.to('PUT', PUT), 'the save went out').toHaveLength(1)
})

test('GD2.7: a 422 not-one-of on split-mode marks the split mode select and focuses it', async () => {
  const { main } = await form(
    { device: `${A},${B}`, 'split-mode': 'tensor' },
    {
      modelProfileSave: () =>
        reply(422, {
          key: 'not-one-of',
          args: ['split-mode', 'layer', 'none, tensor, row'],
          field: 'split-mode',
        }),
    },
  )
  pickMode(main, 'row')
  fireEvent.click(saveButton(main))
  const text = en('api-not-one-of', 'split-mode', 'layer', 'none, tensor, row')
  await within(main).findByText(text)
  expectError(modeSelect(main), text, 'GD2.7: split mode')
  expect(document.activeElement, 'GD2.7: focus on the split mode select').toBe(
    modeSelect(main),
  )
  expect(
    within(fieldset(main)).queryByText(text),
    'GD2.7: not on the GPUs fieldset',
  ).toBeNull()
})

// ── GD2.8: a tensor-split with no device ───────────────────────────────────

test('GD2.8: a file with tensor-split and no GPU in device says it after the as-of line, muted, with the value as written, and no row checked', async () => {
  const { main } = await form({ device: '', 'tensor-split': '3/1' })
  const fs = fieldset(main)
  const asof = asOfLines(fs, DEVICES.probedAt!)
  expect(asof, 'GD2.8: one as-of paragraph').toHaveLength(1)
  const line = within(fs).getByText(
    en('model-profile-gpu-split-without-device', '3/1'),
  )
  expect(line.tagName, 'GD2.8: a paragraph').toBe('P')
  expect(line.className, 'GD2.8: a --muted line').toContain('muted')
  expect(before(asof[0]!, line), 'GD2.8: after the as-of line').toBe(true)
  expect(
    (line.textContent ?? '').includes('3/1'),
    'GD2.8: the value as written',
  ).toBe(true)
  expect(checkedIds(main), 'GD2.8: no row is checked').toHaveLength(0)
})

test('GD2.8: checking rows makes the line go away, and the save sends the composed value, which replaces it', async () => {
  const { bff, main } = await form({ device: '', 'tensor-split': '3/1' })
  const fs = fieldset(main)
  const text = en('model-profile-gpu-split-without-device', '3/1')
  toggle(main, A)
  expect(within(fs).queryByText(text), 'GD2.8: the line is gone').toBeNull()
  toggle(main, B)
  expect(weights(main), 'GD2.3: the two new rows start empty').toEqual([
    [A, ''],
    [B, ''],
  ])
  await save(main, bff)
  expect(sentValues(bff)['tensor-split'], 'GD2.8: null while empty').toBeNull()
  await waitFor(() =>
    expect(
      within(fieldset(main)).queryByText(text),
      'GD2.8: after the save, the line is gone',
    ).toBeNull(),
  )
})

// ── GD2.9: keyboard and screen reader ──────────────────────────────────────

test('GD2.9: the weight field sits between its row checkbox and Move up in the tab order, Auto and Even after the last row, and a weight field is named, valued and described by its share', async () => {
  const { main } = await form({ device: `${A},${B}`, 'tensor-split': '3/1' })
  const card = cardOf(main, GPU)
  const names = mainTabbable(main)
    .filter((el) => card.contains(el))
    .map((el) => nameOf(el))
  const first = names.indexOf(`${WEIGHT} ${A}`)
  expect(
    first,
    'GD2.9: the weight field is in the card tab order',
  ).toBeGreaterThan(-1)
  expect(
    names[first - 1]!.toLowerCase().startsWith(A.toLowerCase()),
    'GD2.9: the row checkbox precedes the weight field',
  ).toBe(true)
  expect(names[first + 1], 'GD2.9: Move up follows the weight field').toBe(
    `${UP} ${A}`,
  )
  const last = names.indexOf(`${WEIGHT} ${B}`)
  expect(names[last + 1], 'GD2.9: Move up follows the weight field').toBe(
    `${UP} ${B}`,
  )
  expect(names[last + 2], 'GD2.9: Move down follows Move up').toBe(
    `${DOWN} ${B}`,
  )
  // GD1.4, as the Auto/Even pin of this file reads it: the two buttons sit
  // after the LIST and after the fieldset's status region, so in the tab order
  // they follow EVERY row of the list, the unchecked row included, and they
  // are the card's last two entries (GD1.12).
  const lastRow = names.indexOf(nameOf(rowOf(main, C).box))
  expect(
    lastRow,
    'GD2.9: the last row of the list is in the card tab order',
  ).toBeGreaterThan(-1)
  expect(names[lastRow + 1], 'GD2.9: Auto follows the last row').toBe(AUTO)
  expect(names[lastRow + 2], 'GD2.9: Even follows Auto').toBe(EVEN)
  expect(
    names.slice(-2),
    'GD1.12: Auto and Even end the card tab order',
  ).toEqual([AUTO, EVEN])
  const input = weightOf(main, B)
  expect(nameOf(input), 'GD2.9: the name a screen reader reads').toBe(
    `${WEIGHT} ${B}`,
  )
  expect(input.value, 'GD2.9: the value').toBe('1')
  expect(described(input), 'GD2.9: the share is read with it').toContain(
    share('25%'),
  )
})

test('GD2.9: nothing is announced while typing: the share is plain text, not a live region, and the fieldset keeps only the empty move status of GD1.9', async () => {
  const { main } = await form({ device: `${A},${B}`, 'tensor-split': '3/1' })
  typeWeight(main, A, '9')
  expect(shares(main), 'the share changed, with no live region').toEqual(
    shareVector('9,1'),
  )
  for (const id of [A, B]) {
    const el = shareEl(main, id)
    expect(el, `GD2.9: ${id} shows its share`).not.toBeNull()
    expect(el!.getAttribute('aria-live'), `GD2.9: ${id}: not live`).toBeNull()
    expect(el!.getAttribute('role'), `GD2.9: ${id}: no role`).toBeNull()
  }
  // GD1.9: the fieldset's one status region is the move line; typing a weight
  // must not announce through it.
  const live = within(fieldset(main)).queryAllByRole('status')
  expect(
    live,
    'GD2.9: the fieldset has the one status region of GD1.9',
  ).toHaveLength(1)
  for (const el of live)
    expect(el.textContent, 'GD2.9: typing announces nothing').toBe('')
})

// ── GD2.10: nothing else ───────────────────────────────────────────────────

test('GD2.10: GD2 keys of section 2 are strings of the suite, and Draft device, Vision GPU, Tensor placement and the split mode warnings are unchanged', async () => {
  const { main } = await form()
  for (const key of [
    'model-profile-gpu-weight',
    'model-profile-gpu-share',
    'model-profile-gpu-share-auto',
    'model-profile-gpu-share-even',
    'model-profile-gpu-share-unused',
    'model-profile-gpu-auto',
    'model-profile-gpu-even',
    'model-profile-gpu-weights-incomplete',
    'model-profile-gpu-split-without-device',
    'api-tensor-split-format',
  ])
    expect(en(key), `GD2.10: ${key} is in the strings table`).toBeTruthy()
  // The device selects keep their file's options: the probed GPUs are not
  // listed in them (the brief's Q2, a follow-up slice).
  for (const [id, label] of [
    ['model-profile-device-draft-control', en('model-profile-draft-device')],
    ['model-profile-mmproj-device-control', en('model-profile-vision-gpu')],
  ] as [string, string][]) {
    const el = document.getElementById(id)
    expect(el, `GD2.10: ${label} is still a field`).not.toBeNull()
    expect(nameOf(el as HTMLElement), `GD2.10: ${label} keeps its label`).toBe(
      label,
    )
    const options = [...(el as HTMLSelectElement).options].map((o) => o.text)
    for (const device of DEVICES.devices!)
      expect(
        options,
        `GD2.10: ${label} does not list the probed ${device.id}`,
      ).not.toContain(`${device.id} ${device.name}`)
  }
  // Tensor placement (override-tensor) and the split mode warning strips of 2k
  // are out of this card: brief 21 gave Tensor placement a card of its own, so
  // the GPU card holds the split mode field, with its own hint once, and the
  // GPUs fieldset.
  const card = cardOf(main, GPU)
  for (const key of [
    'model-profile-ot-tensor',
    'model-profile-ot-layers',
    'model-profile-ot-goes-on',
  ])
    expect(
      within(card).queryByRole('combobox', { name: en(key) }),
      `GD2.10: no Tensor placement control inside the GPU card: ${key}`,
    ).toBeNull()
  // `Field.tsx`'s Hint is one paragraph per `\n` of the string, so the hint is
  // counted as the field's hint block and its lines, never as one text. The
  // card's own About of 22 D4.31 opens the panel of the card's intro, and that
  // panel is a `Hint` too, so the count is scoped to the field hints by leaving
  // out the panel the toggle names.
  const about = within(card).getByRole('button', {
    name: `${en('field-about')} ${GPU}`,
  })
  const panel = about.getAttribute('aria-controls')
  expect(
    panel,
    'GD2.10: 22 D4.31: the card About names the panel of its intro',
  ).toBeTruthy()
  const hints = [...card.querySelectorAll('.field-hint')].filter(
    (el) => el.id !== panel,
  )
  expect(
    hints.length,
    'GD2.10: the card shows one hint, that of its split mode field',
  ).toBe(1)
  expect(
    hints[0]!.id,
    'GD2.10: the hint is model-profile-split-mode-hint',
  ).toBe('model-profile-split-mode-hint')
  expect(
    [...hints[0]!.children].length,
    'GD2.10: the hint shows once, as the lines of its string',
  ).toBe(en('model-profile-split-mode-hint').split('\n').length)
  expect(
    // The label text also sits in the About toggle's visually hidden span
    // (2.5.3 Label in Name), so the field is counted by its control.
    card.querySelectorAll('label[for="model-profile-split-mode-control"]')
      .length,
    'GD2.10: the split mode field shows once',
  ).toBe(1)
})
