// Brief 14, sub-slice GD1, criteria GD1.1 to GD1.4 (fe/briefs/14-gpu-distribution.md):
// the GPU distribution card in its reserved place (02e's 83) with its intro and
// its jump entry; the split mode Select Field with the GUI's four options and
// the file's odd value as a last extra option, never disabled (decision 6); the
// one GET /api/node/devices per arrival beside the profile read, with the
// GPUs fieldset showing "Detecting GPUs…" in a plain paragraph while it is
// pending; and
// the fieldset's own shape: a legend, the read's paragraphs, a `ul role="list"`
// of `li` rows, each with its checkbox named by id and product name and
// described by its muted memory line. GD1.5 to GD1.12 are in
// ModelProfileGpu2 and ModelProfileGpu3.test.tsx, GD2 in ModelProfileGpu4. The
// helpers are those of ModelProfileSave15 and ModelProfileSave16.test.tsx,
// which this file repeats (a test file imports no other test file).
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
  isHidden,
  modelProfileDoc,
  reply,
  startAt,
  type BffOptions,
  type Device,
} from './test/bff.ts'
import { cardHeadings } from './test/modelProfileCards.ts'

const ID = 'Qwen3-27B'
const HASH = `#/profiles/model/${ID}`
const LIST_HASH = '#/profiles/model'
const PUT = `/api/profiles/model/${ID}`
const DEVICES_ROUTE = '/api/node/devices'
const GPU = en('model-profile-gpu-distribution')
const GPUS = en('model-profile-gpus')
const MODE_LABEL = en('model-profile-split-mode')
const ASSETS = en('model-profile-assets')
const HARDWARE = en('model-profile-hardware-config')
/** GD1.2's options, in the GUI's order (MODE_VALUES), with the values the file
 * holds. */
const MODES = [
  { value: '', text: en('model-profile-split-layer') },
  { value: 'none', text: en('model-profile-split-none') },
  { value: 'tensor', text: en('model-profile-split-tensor') },
  { value: 'row', text: en('model-profile-split-row') },
]
/** GD1.1: the intro, each `\n` a paragraph. */
const INTRO_LINES = en('model-profile-gpu-distribution-intro').split('\n')
/** jsdom has no scrollIntoView: record the elements it is called on. */
const scrolled: Element[] = []

beforeEach(() => {
  startAt(HASH)
  scrolled.length = 0
  Element.prototype.scrollIntoView = function (this: Element) {
    scrolled.push(this)
  }
})

afterEach(() => {
  cleanup()
  startAt('')
})

// ── Helpers (as ModelProfileSave15 and ModelProfileSave16.test.tsx) ────────────────

/** Waits for the page read with revision `r1`, the values `values`, and the GPU
 * probe `DEVICES` (the brief's section 0 reply) answered. */
async function form(
  values: Record<string, string> = {},
  options: BffOptions = {},
) {
  const bff = fakeBff({
    devices: DEVICES,
    modelProfile: () =>
      reply(
        200,
        modelProfileDoc({}, { revision: 'r1', values: { ...values } }),
      ),
    ...options,
  })
  render(<App />)
  const main = screen.getByRole('main')
  await within(main).findByRole('heading', { level: 1, name: ID })
  await within(main).findByRole('heading', { level: 3, name: GPU })
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

/** Waits for the device read to have landed with a probe time: the as-of line
 * of GD1.7 is the last thing the reply writes. */
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

/** The memory line of GD1.4: `totalMib` and `freeMib` over 1024, to one
 * decimal. */
function memory(totalMib: number, freeMib: number): string {
  return en(
    'model-profile-gpu-memory',
    (totalMib / 1024).toFixed(1),
    (freeMib / 1024).toFixed(1),
  )
}

/** The split mode select (GD1.2). */
function modeSelect(main: HTMLElement): HTMLSelectElement {
  return within(cardOf(main, GPU)).getByRole('combobox', {
    name: MODE_LABEL,
  }) as HTMLSelectElement
}

/** Every device row, in the page's order: the `li` of GD1.4's list, its
 * checkbox and the checkbox's accessible name. */
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
    const box = boxes[0] as HTMLInputElement
    return { li, name: nameOf(box), box }
  })
}

/** The row whose checkbox names `id` (its label starts with the id, GD1.4). */
function rowOf(main: HTMLElement, id: string) {
  const found = rows(main).find((r) =>
    r.name.toLowerCase().startsWith(id.toLowerCase()),
  )
  expect(found, `the row of ${id}`).toBeDefined()
  return found!
}

/** The name a screen reader reads: the label tied to the control, the label
 * wrapping it, `aria-label`, or the elements `aria-labelledby` names (the
 * computation of ModelProfileSave16.test.tsx, with `aria-label` added). */
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

/** The save bar's status line. The GPUs card keeps its move status in the DOM
 * from its first render (GD1.9), so the bar's line is picked inside the bar,
 * not by counting the page's status regions (ruling of 2026-11-21). */
function status(main: HTMLElement): HTMLElement {
  const all = within(main)
    .queryAllByRole('status')
    .filter((s) => s.closest('.save-bar') !== null)
  expect(all, 'the save bar has its status line').toHaveLength(1)
  return all[0]!
}

function saveButton(main: HTMLElement): HTMLElement {
  return within(main).getByRole('button', { name: en('save-button') })
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

/** Picks a split mode by its value. */
function pickMode(main: HTMLElement, value: string) {
  fireEvent.change(modeSelect(main), { target: { value } })
}

/** A probe reply answered 200 with `devices` and the brief's time. */
function probe(devices: Device[]) {
  return reply(200, { probedAt: DEVICES.probedAt, devices })
}

// ── GD1.1: the card ──────────────────────────────────────────────────────────

test('GD1.1: the card is a D5.2 Card with h3 GPU distribution, right after the Assets card and before Hardware config, among the page h3 list', async () => {
  const { main } = await form()
  const titles = cardHeadings()
  const n = titles.indexOf(GPU)
  expect(n, 'GD1.1: the card is in the list of cards').toBeGreaterThan(-1)
  expect(titles[n - 1], 'GD1.1: right after Assets').toBe(ASSETS)
  expect(titles[n + 1], 'GD1.1: before Hardware config').toBe(HARDWARE)
  const h3s = within(main)
    .getAllByRole('heading', { level: 3 })
    .map((h) => (h.textContent ?? '').replace(/\s+/g, ' ').trim())
  expect(h3s, 'GD1.1: the page h3 list, in page order').toEqual(titles)
  const card = cardOf(main, GPU)
  expect(card.tagName, 'GD1.1: a section, as every Card').toBe('SECTION')
  expect(
    before(cardOf(main, ASSETS), card),
    'GD1.1: after the Assets card',
  ).toBe(true)
  expect(
    before(card, cardOf(main, HARDWARE)),
    'GD1.1: before the Hardware card',
  ).toBe(true)
})

test('GD1.1: the card intro model-profile-gpu-distribution-intro follows its h3, each line of it a paragraph, shown', async () => {
  const { main } = await form()
  expect(INTRO_LINES.length, 'the intro of section 2 is two lines').toBe(2)
  const card = cardOf(main, GPU)
  const h3 = within(main).getByRole('heading', { level: 3, name: GPU })
  const paras = [...card.querySelectorAll('p')]
  const texts = paras.map((p) => p.textContent ?? '')
  for (const line of INTRO_LINES)
    expect(
      texts,
      `GD1.1: the intro line is a paragraph of its own: ${line.slice(0, 24)}…`,
    ).toContain(line)
  const first = paras.find((p) => (p.textContent ?? '') === INTRO_LINES[0])!
  expect(isHidden(first), 'GD1.1: the intro is shown').toBe(false)
  expect(before(h3, first), 'GD1.1: the intro follows the h3').toBe(true)
  const second = paras.find((p) => (p.textContent ?? '') === INTRO_LINES[1])!
  expect(before(first, second), 'GD1.1: the lines in order').toBe(true)
})

test('GD1.1: the jump list gains GPU distribution between Assets and Hardware, and its button moves focus to the card h3 and scrolls it into view, leaving the hash alone', async () => {
  const { main } = await form()
  const titles = cardHeadings()
  const nav = within(main).getByRole('navigation', {
    name: en('model-profile-jump-label'),
  })
  const list = nav.querySelector('ul')
  expect(list, 'GD1.1: the nav holds the ul of entries').not.toBeNull()
  const entries = () => within(list as HTMLElement).getAllByRole('button')
  const buttons = entries().map((b) =>
    (b.textContent ?? '').replace(/\s+/g, ' ').trim(),
  )
  expect(buttons, 'GD1.1: one jump button per card, in card order').toEqual(
    titles,
  )
  const n = titles.indexOf(GPU)
  expect(buttons[n - 1], 'GD1.1: the entry follows Assets').toBe(ASSETS)
  expect(buttons[n + 1], 'GD1.1: the entry precedes Hardware config').toBe(
    HARDWARE,
  )
  const h3 = within(main).getByRole('heading', { level: 3, name: GPU })
  expect(h3.getAttribute('tabindex'), 'GD1.1: the h3 takes focus').toBe('-1')
  fireEvent.click(entries()[n]!)
  expect(document.activeElement, 'GD1.1: focus on the card h3').toBe(h3)
  expect(scrolled, 'GD1.1: the card scrolled into view').toContain(h3)
  expect(window.location.hash, 'GD1.1: the hash is untouched').toBe(HASH)
})

test('GD1.1: the card is editable as the others: no read-only sentence in it, and no control in it disabled', async () => {
  const { main } = await form()
  await ready(main)
  const card = cardOf(main, GPU)
  expect(
    within(card).queryByText(/greyed out|cannot be changed here|read.only/i),
    'GD1.1: no Notice, no read-only sentence',
  ).toBeNull()
  const controls = [
    ...card.querySelectorAll<HTMLElement>('select, input, button, textarea'),
  ]
  expect(controls.length, 'the card holds controls').toBeGreaterThan(0)
  for (const el of controls)
    expect(
      (el as HTMLButtonElement).disabled,
      `GD1.1: no disabled control (${el.tagName} ${nameOf(el)})`,
    ).toBe(false)
})

// ── GD1.2: split mode ────────────────────────────────────────────────────────

test('GD1.2: the card first field is a Select Field labelled GPU split mode, its first field, with the GUI four options in order and value', async () => {
  const { main } = await form()
  await ready(main)
  const card = cardOf(main, GPU)
  const sel = modeSelect(main)
  expect(sel.tagName, 'GD1.2: a select').toBe('SELECT')
  expect(nameOf(sel), 'GD1.2: the field label names the select').toBe(
    MODE_LABEL,
  )
  const options = [...sel.options].map((o) => ({
    value: o.value,
    text: o.textContent?.replace(/\s+/g, ' ').trim(),
  }))
  expect(
    options,
    'GD1.2: the four options, in order, with the file values',
  ).toEqual(MODES.map((m) => ({ value: m.value, text: m.text })))
  // The field is the card's first: its label precedes the GPUs fieldset.
  const fs = fieldset(main)
  expect(before(sel, fs), 'GD1.2: split mode is before the GPUs fieldset').toBe(
    true,
  )
  expect(card.contains(fs), 'GD1.2: the fieldset is in the card').toBe(true)
})

test.each([
  ['absent, so the first option shows', undefined, ''],
  ['the empty string, so the first option shows', '', ''],
  ['layer, so the first option shows', 'layer', ''],
  ['none', 'none', 'none'],
  ['tensor', 'tensor', 'tensor'],
  ['row', 'row', 'row'],
])(
  'GD1.2: a read with split-mode %s, and the four options stay',
  async (what, value, shown) => {
    const values: Record<string, string> = {}
    if (value !== undefined) values['split-mode'] = value
    const { main } = await form(values)
    await ready(main)
    const sel = modeSelect(main)
    expect(sel.value, `GD1.2: split-mode ${what}`).toBe(shown)
    expect(
      [...sel.options].map((o) => o.value),
      `GD1.2: split-mode ${what} keeps the four options`,
    ).toEqual(MODES.map((m) => m.value))
  },
)

test.each(['Layer', 'foo', 'LAYER '])(
  'GD1.2: a read with split-mode %s is one extra last option carrying the value as written, and it is the selected one',
  async (value) => {
    const { main } = await form({ 'split-mode': value })
    await ready(main)
    const sel = modeSelect(main)
    const options = [...sel.options]
    expect(
      options.length,
      `GD1.2: split-mode ${value}: the four plus one`,
    ).toBe(5)
    expect(
      options.slice(0, 4).map((o) => o.text),
      'GD1.2: the four options keep their place',
    ).toEqual(MODES.map((m) => m.text))
    const extra = options[4]!
    expect(extra.value, `GD1.2: the extra option value is ${value}`).toBe(value)
    expect(extra.textContent, 'GD1.2: it carries the value as written').toBe(
      value,
    )
    expect(sel.value, 'GD1.2: the file value is the selected one').toBe(value)
  },
)

test('GD1.2: the hint model-profile-split-mode-hint is behind the field About toggle', async () => {
  const { main } = await form({ device: 'CUDA0' })
  await ready(main)
  const card = cardOf(main, GPU)
  const toggle = within(card).getByRole('button', {
    name: `${en('field-about')} ${MODE_LABEL}`,
  })
  const hint = en('model-profile-split-mode-hint')
  // `Field.tsx`'s Hint is one paragraph per `\n` of the string, so the whole
  // string is the text of no element: each line is read on its own.
  const lines = hint.split('\n')
  expect(
    lines.length,
    'GD1.2: the hint of section 2 is more than one line',
  ).toBeGreaterThan(1)
  const box = document.getElementById('model-profile-split-mode-hint')
  expect(box, 'GD1.2: the hint element exists').not.toBeNull()
  expect(
    toggle.getAttribute('aria-controls'),
    'GD1.2: the toggle controls the hint',
  ).toBe('model-profile-split-mode-hint')
  expect(
    toggle.getAttribute('aria-expanded'),
    'GD1.2: the hint is closed at first',
  ).toBe('false')
  expect(isHidden(box!), 'GD1.2: the hint is hidden at first').toBe(true)
  fireEvent.click(toggle)
  expect(
    toggle.getAttribute('aria-expanded'),
    'GD1.2: About opens the hint',
  ).toBe('true')
  expect(isHidden(box!), 'GD1.2: About shows the hint').toBe(false)
  for (const line of lines)
    expect(
      within(card).queryByText(line),
      `GD1.2: the hint line is read: ${line.slice(0, 30)}`,
    ).not.toBeNull()
})

test.each([
  ['no GPU checked', ''],
  ['one GPU checked', 'CUDA0'],
  ['three GPUs checked', 'CUDA0,ROCm0,Vulkan0'],
])(
  'GD1.2: split mode is never disabled (decision 6), with %s',
  async (_what, device) => {
    const { main } = await form({ device })
    await ready(main)
    expect(
      modeSelect(main).disabled,
      `GD1.2: enabled with ${device || 'no GPU checked'}`,
    ).toBe(false)
  },
)

test('GD1.2: an explicit layer stays untouched until another option is picked: picking tensor sends split-mode tensor, picking back the first option leaves the form clean', async () => {
  const { bff, main } = await form({ 'split-mode': 'layer' })
  await ready(main)
  expect(status(main).textContent, 'GD1.2: the load is no change').toBe('')
  pickMode(main, 'tensor')
  expect(status(main).textContent, 'GD1.2: a new option is a change').toBe(
    en('save-unsaved'),
  )
  await save(main, bff)
  expect(
    sentValues(bff)['split-mode'],
    'GD1.2: tensor is sent as its value',
  ).toBe('tensor')
})

test('GD1.2: picking the first option back removes the key: a read of split-mode tensor picked to layer sends split-mode null, and picking it back is clean again', async () => {
  const { bff, main } = await form({ 'split-mode': 'tensor' })
  await ready(main)
  pickMode(main, '')
  expect(status(main).textContent, 'GD1.2: the first option is a change').toBe(
    en('save-unsaved'),
  )
  await save(main, bff)
  expect(
    sentValues(bff)['split-mode'],
    'GD1.2: the first option sends null',
  ).toBeNull()
})

test('GD1.2: picking tensor with the weights untouched sends no tensor-split, and a split-mode change alone is a change the save bar counts', async () => {
  const { bff, main } = await form({
    device: 'CUDA0,ROCm0',
    'tensor-split': '3,1',
  })
  await ready(main)
  expect(status(main).textContent, 'GD1.2: the load is clean').toBe('')
  pickMode(main, 'tensor')
  await save(main, bff)
  const sent = sentValues(bff)
  expect(sent['split-mode'], 'GD1.2: the mode is sent').toBe('tensor')
  expect(
    'tensor-split' in sent,
    'GD1.2: a mode change sends no tensor-split',
  ).toBe(false)
  expect('device' in sent, 'GD1.2: an untouched device is not sent').toBe(false)
})

test('GD1.2: split mode is enabled when the device read failed (decision 5, decision 6)', async () => {
  const { main } = await form(
    {},
    { devices: reply(503, { key: 'node-unavailable' }) },
  )
  await within(fieldset(main)).findByText(en('model-profile-gpu-failed'))
  expect(modeSelect(main).disabled, 'GD1.2: enabled with no list').toBe(false)
})

// ── GD1.3: the device read ───────────────────────────────────────────────────

test('GD1.3: arriving at the page sends one GET /api/node/devices beside the profile read, and nothing else asks for it', async () => {
  const { bff, main } = await form()
  await ready(main)
  expect(
    bff.to('GET', DEVICES_ROUTE).length,
    'GD1.3: one device read per arrival',
  ).toBe(1)
  expect(bff.to('GET', PUT).length, 'GD1.3: the profile was read').toBe(1)
  // An edit, a save, and Discard send no device read.
  fireEvent.change(document.getElementById('model-profile-ctx-size-control')!, {
    target: { value: '65536' },
  })
  await save(main, bff)
  expect(bff.to('GET', DEVICES_ROUTE).length, 'GD1.3: a save sends none').toBe(
    1,
  )
  // The save was accepted, so the loaded value is now 65536: Discard restores
  // that, the value the form was last saved with.
  fireEvent.change(document.getElementById('model-profile-ctx-size-control')!, {
    target: { value: '131072' },
  })
  fireEvent.click(
    within(main).getByRole('button', { name: en('save-discard') }),
  )
  await waitFor(() =>
    expect(
      (
        document.getElementById(
          'model-profile-ctx-size-control',
        ) as HTMLInputElement
      ).value,
      'GD1.3: Discard restored the loaded value',
    ).toBe('65536'),
  )
  expect(bff.to('GET', DEVICES_ROUTE).length, 'GD1.3: Discard sends none').toBe(
    1,
  )
})

test('GD1.3: leaving the page and coming back sends another device read', async () => {
  const { bff, main } = await form()
  await ready(main)
  expect(bff.to('GET', DEVICES_ROUTE).length, 'GD1.3: one on arrival').toBe(1)
  window.location.hash = LIST_HASH
  await within(main).findByRole('heading', {
    level: 1,
    name: en('model-profiles-title'),
  })
  window.location.hash = HASH
  await within(main).findByRole('heading', { level: 1, name: ID })
  await ready(main)
  expect(bff.to('GET', DEVICES_ROUTE).length, 'GD1.3: another on arrival').toBe(
    2,
  )
})

test('GD1.3: while the read is pending the GPUs fieldset shows Detecting GPUs in a plain paragraph (no role=status) after its legend, the profile GPUs as rows with no name and no memory line, and the rest of the page is usable and saves', async () => {
  const h = held()
  const { bff, main } = await form({ device: 'CUDA0' }, { devices: h.answer })
  const fs = fieldset(main)
  const legend = fs.querySelector('legend')
  expect(legend, 'GD1.4: the fieldset has a legend').not.toBeNull()
  expect((legend!.textContent ?? '').trim(), 'GD1.4: the legend is GPUs').toBe(
    GPUS,
  )
  const pending = within(fs).getByText(en('model-profile-gpu-detecting'))
  expect(pending.tagName, 'GD1.3: a paragraph').toBe('P')
  expect(pending.getAttribute('role'), 'GD1.3: not a status').toBeNull()
  expect(before(legend!, pending), 'GD1.3: the line follows the legend').toBe(
    true,
  )
  // GD1.9: the card's one status region is the move line, empty from its first
  // render, so the pending line is not a live region the user is announced.
  const live = within(fs).queryAllByRole('status')
  expect(
    live,
    'GD1.3: the fieldset keeps the one status region of GD1.9',
  ).toHaveLength(1)
  expect(live[0]!.textContent, 'GD1.3: the move region is empty').toBe('')
  const cuda = rowOf(main, 'CUDA0')
  expect(cuda.name, 'GD1.3: the row of a profile GPU, no name yet').toBe(
    'CUDA0',
  )
  expect(cuda.box.checked, 'GD1.3: its row is checked').toBe(true)
  expect(described(cuda.box), 'GD1.3: no memory line to describe it').toEqual(
    [],
  )
  // The rest of the page is usable and saves.
  fireEvent.change(document.getElementById('model-profile-ctx-size-control')!, {
    target: { value: '65536' },
  })
  await save(main, bff)
  expect(
    sentValues(bff)['ctx-size'],
    'GD1.3: the page saves while the read is pending',
  ).toBe('65536')
  h.resolve(reply(200, DEVICES))
  await within(fs).findByText(asOfDate(DEVICES.probedAt!))
  expect(
    within(fs).queryByText(en('model-profile-gpu-detecting')),
    'GD1.3: the line is gone once the read lands',
  ).toBeNull()
  expect(rowOf(main, 'CUDA0').name, 'GD1.3: the name arrives').toContain(
    'RTX 5080',
  )
})

// ── GD1.4: the GPUs fieldset ─────────────────────────────────────────────────

test('GD1.4: the fieldset holds after the legend the read paragraph, then a ul role="list" of one li per GPU, in the reply order with no device', async () => {
  const { main } = await form()
  const fs = await ready(main)
  const legend = fs.querySelector('legend')!
  const asof = asOfLines(fs, DEVICES.probedAt!)
  expect(asof, 'GD1.4: one as-of paragraph').toHaveLength(1)
  const list = fs.querySelector('ul[role="list"]')
  expect(list, 'GD1.4: a ul role="list"').not.toBeNull()
  expect(
    before(legend, asof[0]!),
    'GD1.4: the read paragraph follows the legend',
  ).toBe(true)
  expect(
    before(asof[0]!, list!),
    'GD1.4: the list follows the read paragraphs',
  ).toBe(true)
  expect(
    // The row's name is the id and the product name (GD1.4, DESIGN.md 5.2
    // "GPU list"), so the id alone never names a row.
    rows(main).map((r) => r.name),
    'GD1.4: no device: every GPU, in the reply order, unchecked',
  ).toEqual([
    'CUDA0 NVIDIA GeForce RTX 5080',
    'ROCm0 AMD Radeon AI PRO R9700',
    'Vulkan0 Intel Arc Graphics',
  ])
  for (const r of rows(main))
    expect(r.box.checked, `${r.name}: unchecked`).toBe(false)
})

test.each(DEVICES.devices!.map((d) => [d.id!, d] as [string, Device]))(
  'GD1.4: the row of %s is a checkbox labelled with the id then the product name, described by its muted memory line of GiB to one decimal',
  async (id, device) => {
    const { main } = await form()
    await ready(main)
    const row = rowOf(main, id)
    expect(row.li.tagName, 'GD1.4: a li').toBe('LI')
    expect(row.name, 'GD1.4: the label holds the id then the name').toBe(
      `${device.id} ${device.name}`,
    )
    const line = memory(Number(device.totalMib), Number(device.freeMib))
    expect(
      row.li.textContent,
      'GD1.4: the memory line is in the row',
    ).toContain(line)
    expect(
      described(row.box),
      'GD1.4: the checkbox is described by it',
    ).toContain(line)
    const el = [...row.li.querySelectorAll('p')].find(
      (p) => (p.textContent ?? '') === line,
    )
    expect(el, 'GD1.4: the memory line is a paragraph of the row').toBeDefined()
    expect(el!.className, 'GD1.4: the line is muted').toContain('muted')
  },
)

test.each([
  ['totalMib 0', { id: 'CUDA0', name: 'A', totalMib: 0, freeMib: 100 }],
  ['no totalMib', { id: 'CUDA0', name: 'A', freeMib: 100 }],
  [
    'totalMib a string',
    { id: 'CUDA0', name: 'A', totalMib: '16304', freeMib: 100 },
  ],
  ['totalMib negative', { id: 'CUDA0', name: 'A', totalMib: -1, freeMib: 100 }],
] as [string, Device][])(
  'GD1.4: a device with %s has no memory line and no aria-describedby',
  async (_what, device) => {
    const { main } = await form({}, { devices: probe([device]) })
    await ready(main)
    const row = rowOf(main, device.id!)
    expect(row.li.textContent, 'GD1.4: no GiB line').not.toContain('GiB')
    expect(described(row.box), 'GD1.4: nothing describes the checkbox').toEqual(
      [],
    )
  },
)

test('GD1.4: the fieldset is in the card, after the split mode field', async () => {
  const { main } = await form()
  const fs = await ready(main)
  const card = cardOf(main, GPU)
  expect(card.contains(fs), 'GD1.4: the fieldset is in the card').toBe(true)
  expect(
    before(modeSelect(main), fs),
    'GD1.4: the fieldset is after the split mode select',
  ).toBe(true)
})
