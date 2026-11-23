// Brief 21, sub-slice TP1 (fe/briefs/21-tensor-placement.md), criteria 21 T.2 to
// 21 T.9: the Tensor placement card, its rules list, a rule's Tensor / Layers /
// Goes on / Remove / Pattern controls, Add rule and Remove, and the
// `override-tensor` value the page saves. TP2 (Rules as text, the checks at
// Save, the manager's refusals) and TP3 (the coverage lines) are later slices:
// nothing here tests them. The helpers are those of
// ModelProfileLayers2.test.tsx and ModelProfileGpu1.test.tsx, repeated here (a
// test file imports no other test file). The 320 px and 200 % half of 21 T.9 is
// the designer's measurement (`fe/ux` state `model-profile-placement`): jsdom
// lays nothing out. The 2026-11-23 D4 re-check adds the stylesheet half: the
// file reads `index.css` as text, the way `Reflow.test.ts` does (the reader is
// a small local copy, as in every sheet test).
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
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
import { cardHeadings } from './test/modelProfileCards.ts'
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
const CARD = en('model-profile-tensor-placement')
const GPU = en('model-profile-gpu-distribution')
const HARDWARE = en('model-profile-hardware-config')
const RULES = en('model-profile-ot-rules')
const NONE = en('model-profile-ot-none')
const FORMAT_KEY = 'model-profile-ot-format'
const FORMAT_TEXT = en(FORMAT_KEY)
const TENSOR = en('model-profile-ot-tensor')
const LAYERS = en('model-profile-ot-layers')
const GOES = en('model-profile-ot-goes-on')
const REMOVE = en('model-profile-ot-remove')
const PATTERN = en('model-profile-ot-pattern')
const ADD = en('model-profile-ot-add')
/** 21 T.13 (TP2): the Rules as text Field follows Add rule, so its About toggle
 *  and its input end every tab path of this card. The full path, with the rules,
 *  is pinned in `TensorPlacement2.test.tsx`. */
const TEXT_STOPS = [
  `${en('field-about')} ${en('model-profile-ot-text')}`,
  en('model-profile-ot-text'),
]
const CPU_TEXT = en('model-profile-ot-cpu')
const NO_DEVICE = en('model-profile-ot-no-device')
/** 21 T.11 (TP2): the bar's alert when the page's own check refuses the save. */
const INVALID = en('save-invalid')
const NONE_FOUND = en('model-profile-gpu-none-found')
/** The key this card owns. */
const OT = 'override-tensor'
/** The two canned patterns the tests write into the file. */
const EMBD = String.raw`^token_embd\.weight`
const EXPS = String.raw`\.ffn_(up|down|gate|gate_up)_(ch|)exps`
const EXPS_DOWN = String.raw`\.ffn_down_exps`
/** A hand-written pattern no kind owns (21 T.4: it selects Custom). */
const HAND = String.raw`blk\.1[0-9]\..*_exps`
/** 21 T.2: the intro, each `\n` a paragraph. */
const INTRO_LINES = en('model-profile-tensor-placement-intro').split('\n')
/** The 17 Tensor options, in the table's order, value the kind's id. */
const KIND_IDS = [
  'embd',
  'ple',
  'output',
  'layer',
  'attn',
  'ssm',
  'ffn',
  'ffn_up',
  'ffn_gate',
  'ffn_down',
  'exps',
  'exps_up',
  'exps_gate',
  'exps_down',
  'shexp',
  'router',
  'custom',
]

/** What the read gives: a model file, and nothing for `override-tensor` (the
 * key absent, which is no rule). */
const AS_LOADED: Record<string, string> = {
  model: MODEL_FILE,
  'n-gpu-layers': '29',
}

/** `AS_LOADED` with these rules. */
function file(...rules: string[]): Record<string, string> {
  return { ...AS_LOADED, [OT]: rules.join(',') }
}

/** The model file's facts: the GPU card's line, the landing marker of `form`. */
function facts(): ModelInfo {
  return {
    model: MODEL_FILE,
    info: { blockCount: 33, moe: true, expertCount: 128, expertUsed: 8 },
    reason: null,
  }
}

beforeEach(() => {
  startAt(HASH)
  // jsdom has no scrollIntoView; the jump button of 21 T.2 calls it on the h3.
  Element.prototype.scrollIntoView = () => {}
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

/** Waits for the device read of `probe` to have landed, so a select read after
 * it is the answered list and not the pending one. */
async function landed(main: HTMLElement, probe: DeviceProbe = DEVICES) {
  const fs = await within(main).findByRole('group', {
    name: en('model-profile-gpus'),
  })
  if ((probe.devices ?? []).length === 0) {
    await within(fs).findByText(NONE_FOUND)
    return
  }
  if (typeof probe.probedAt === 'string') {
    await within(fs).findByText(asOfDate(probe.probedAt))
    return
  }
  await within(fs).findByText(en('model-profile-gpu-no-probe'))
}

/** The Card whose h3 is `title` (fe/DESIGN.md 5.2: a `section.card`). */
function cardOf(main: HTMLElement, title: string): HTMLElement {
  const h3 = within(main).getByRole('heading', { level: 3, name: title })
  const card = h3.closest('section')
  expect(card, `${title}: the card element`).not.toBeNull()
  return card as HTMLElement
}

/** The accessible name of a control, as the page gives it. */
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

/** The select's options as (value, text) pairs, in the order it lists them. */
function optionsOf(sel: HTMLSelectElement) {
  return [...sel.options].map((o) => ({
    value: o.value,
    text: o.textContent ?? '',
  }))
}

/** One listed GPU's option (21 T.5): its id as the value, `id: name` as the
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

/** The device options of 21 T.5 in order: CPU, then every probed GPU. */
function deviceOptions(probe: DeviceProbe = DEVICES) {
  return [
    { value: 'CPU', text: CPU_TEXT },
    ...(probe.devices ?? []).map((d) =>
      gpuOption(d.id ?? '', typeof d.name === 'string' ? d.name : null),
    ),
  ]
}

/** A device no option's value equals exactly (21 T.5): the last option, the
 * "(not detected)" text once the probe answered with a time, the device
 * itself while it gave none. */
function missingOption(device: string, probe: DeviceProbe = DEVICES) {
  return {
    value: device,
    text:
      typeof probe.probedAt === 'string'
        ? en('model-profile-device-option-missing', device)
        : device,
  }
}

/** The rule with device `""` shows this first (21 T.5). */
const NO_DEVICE_OPTION = { value: '', text: NO_DEVICE }

/** Every tabbable element of `root`, in document order. */
function tabbable(root: HTMLElement): HTMLElement[] {
  return [
    ...root.querySelectorAll<HTMLElement>(
      'a[href], button, input, select, textarea, [tabindex]',
    ),
  ].filter(
    (el) =>
      !(el as HTMLButtonElement).disabled &&
      el.getAttribute('tabindex') !== '-1' &&
      !isHidden(el),
  )
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

/** The texts of the elements `aria-describedby` names. */
function described(el: HTMLElement): string[] {
  return describedIds(el).map(
    (id) => document.getElementById(id)?.textContent ?? `(no #${id})`,
  )
}

/** The Field Error element the control names, by id. */
function errorOf(el: HTMLElement, text: string): HTMLElement | null {
  return (
    describedIds(el)
      .map((id) => document.getElementById(id))
      .find((e) => e !== null && e.textContent === text) ?? null
  )
}

/** Asserts `el` is marked invalid with the error `text` under it (21 T.11,
 *  02h 142). The Field Error's place in the rule row is 21 T.11's own test in
 *  `TensorPlacement2.test.tsx`. */
function expectError(el: HTMLElement, text: string, what: string) {
  expect(el.getAttribute('aria-invalid'), `${what} aria-invalid`).toBe('true')
  expect(described(el), `${what} aria-describedby`).toContain(text)
  const error = errorOf(el, text)
  expect(error, `${what}: the error is shown`).toBeDefined()
  expect(isHidden(error!), `${what}: the error is shown`).toBe(false)
  expect(before(el, error!), `${what}: the error comes after the control`).toBe(
    true,
  )
  return error!
}

/** Clicks Save changes and waits for the PUT of this model profile. */
async function save(main: HTMLElement, bff: ReturnType<typeof fakeBff>) {
  within(main)
    .getByRole('button', { name: en('save-button') })
    .focus()
  fireEvent.click(within(main).getByRole('button', { name: en('save-button') }))
  await waitFor(() => expect(bff.to('PUT', PUT)).toHaveLength(1))
}

/** Clicks Discard. */
function discard(main: HTMLElement) {
  fireEvent.click(
    within(main).getByRole('button', { name: en('save-discard') }),
  )
}

/** Clicks Save changes and waits for the bar's `save-invalid`: the page's own
 *  check (21 T.11, TP2) refused the save, so nothing went out. */
async function saveInvalid(main: HTMLElement, bff: ReturnType<typeof fakeBff>) {
  fireEvent.click(within(main).getByRole('button', { name: en('save-button') }))
  await within(main).findByText(INVALID)
  expect(bff.to('PUT', PUT), '21 T.11: nothing is sent').toHaveLength(0)
}

/** The values one save sent, keyed. */
function sentValues(bff: ReturnType<typeof fakeBff>): Record<string, unknown> {
  const calls = bff.to('PUT', PUT)
  expect(calls, 'one save').toHaveLength(1)
  return (calls[0]!.body as { values?: Record<string, unknown> })?.values ?? {}
}

/** The card's Rules fieldset and its wrapper div (21 T.2). */
function rulesFieldset(card: HTMLElement): HTMLElement {
  return within(card).getByRole('group', { name: RULES })
}

/** The format paragraph, by the id 21 T.4 links to. */
function formatParagraph(): HTMLElement {
  const p = document.getElementById(FORMAT_KEY)
  expect(p, 'the format paragraph keeps its id').not.toBeNull()
  return p as HTMLElement
}

/** The rules' `li`, in document order. */
function ruleItems(card: HTMLElement): HTMLElement[] {
  const list = within(card).queryByRole('list')
  expect(list, 'the rules are a list').not.toBeNull()
  return within(list as HTMLElement).getAllByRole('listitem')
}

/** One rule's fieldset, named by its legend, and its legend's text. */
function ruleGroup(card: HTMLElement, n: number): HTMLElement {
  const name = en('model-profile-ot-rule', `${n}`)
  return within(card).getByRole('group', { name })
}

function legendOf(group: HTMLElement): string {
  const legend = group.querySelector('legend')
  return (legend?.textContent ?? '').replace(/\s+/g, ' ').trim()
}

function tensorOf(group: HTMLElement): HTMLSelectElement {
  return within(group).getByRole('combobox', {
    name: TENSOR,
  }) as HTMLSelectElement
}

function layersOf(group: HTMLElement): HTMLInputElement | null {
  return within(group).queryByRole('textbox', {
    name: LAYERS,
  }) as HTMLInputElement | null
}

function goesOnOf(group: HTMLElement): HTMLSelectElement {
  return within(group).getByRole('combobox', {
    name: GOES,
  }) as HTMLSelectElement
}

function patternOf(group: HTMLElement): HTMLInputElement | null {
  return within(group).queryByRole('textbox', {
    name: PATTERN,
  }) as HTMLInputElement | null
}

function removeOf(group: HTMLElement, n: number): HTMLElement {
  return within(group).getByRole('button', {
    name: `${REMOVE} ${en('model-profile-ot-rule', `${n}`)}`,
  })
}

function addOf(card: HTMLElement): HTMLElement {
  return within(card).getByRole('button', { name: ADD })
}

/** 21 T.10 (TP2): the Rules as text input. It shows the loaded value as the
 *  file writes it while the key is unchanged, and `render` of the rows once
 *  they are an edit, so it is how a test reads the rule a row stands for. */
function textField(card: HTMLElement): HTMLInputElement {
  return within(card).getByRole('textbox', {
    name: en('model-profile-ot-text'),
  }) as HTMLInputElement
}

/** The one flex item of a rule's row that holds `control` and its label
 *  (21 T.4, and its TP1 review D1: a label and its control are never two
 *  items, side by side when they fit and split apart when they wrap). The
 *  item's name is the coder's, its element is the `span` the review names. */
function itemOf(control: HTMLElement, row: HTMLElement, what: string): void {
  const item = control.parentElement
  expect(
    item,
    `21 TP1 review D1: ${what}: its control is inside an item`,
  ).not.toBeNull()
  const parent = item!.parentElement
  expect(
    parent,
    `21 TP1 review D1: ${what}: the item is a direct item of the rule's flex row`,
  ).toBe(row)
  expect(
    item!.tagName,
    `21 TP1 review D1: ${what}: the item is one span wrapping the label and its control`,
  ).toBe('SPAN')
  const labels = [...item!.querySelectorAll('label')]
  expect(
    labels.length,
    `21 TP1 review D1: ${what}: the item holds exactly one label, its own`,
  ).toBe(1)
  expect(
    labels[0]!.getAttribute('for'),
    `21 TP1 review D1: ${what}: the label inside the item names the control beside it`,
  ).toBe(control.id)
}

// ── 21 T.2: the card ──────────────────────────────────────────────────────

test('21 T.2: the card is a Card titled model-profile-tensor-placement, its heading id the tensor one, right after GPU distribution and before Hardware config among the page h3 list', async () => {
  const { main } = await form()
  const titles = cardHeadings()
  const n = titles.indexOf(CARD)
  expect(n, '21 T.2: the card is in the list of cards').toBeGreaterThan(-1)
  expect(titles[n - 1], '21 T.2: right after GPU distribution').toBe(GPU)
  expect(titles[n + 1], '21 T.2: before Hardware config').toBe(HARDWARE)
  const card = cardOf(main, CARD)
  const h3 = within(main).getByRole('heading', { level: 3, name: CARD })
  expect(card.tagName, '21 T.2: a section, as every Card').toBe('SECTION')
  expect(
    h3.getAttribute('tabindex'),
    '21 T.2: the h3 takes focus, as the jump list reaches it',
  ).toBe('-1')
  const heading = document.getElementById('model-profile-card-tensor')
  expect(heading, '21 T.2: the card id is tensor').toBe(h3)
  expect(
    before(cardOf(main, GPU), card),
    '21 T.2: after the GPU distribution card',
  ).toBe(true)
  expect(
    before(card, cardOf(main, HARDWARE)),
    '21 T.2: before the Hardware card',
  ).toBe(true)
})

test('21 T.2: the jump list gains its entry between GPU distribution and Hardware config, and its button focuses the card h3', async () => {
  const { main } = await form()
  const nav = within(main).getByRole('navigation', {
    name: en('model-profile-jump-label'),
  })
  const list = nav.querySelector('ul')
  expect(list, '21 T.2: the nav holds the ul of entries').not.toBeNull()
  const buttons = within(list as HTMLElement)
    .getAllByRole('button')
    .map((b) => (b.textContent ?? '').replace(/\s+/g, ' ').trim())
  expect(buttons, '21 T.2: one jump button per card, in card order').toEqual(
    cardHeadings(),
  )
  const n = buttons.indexOf(CARD)
  expect(buttons[n - 1], '21 T.2: the entry follows GPU distribution').toBe(GPU)
  expect(buttons[n + 1], '21 T.2: the entry precedes Hardware config').toBe(
    HARDWARE,
  )
  fireEvent.click(within(list as HTMLElement).getAllByRole('button')[n]!)
  expect(document.activeElement, '21 T.2: focus on the card h3').toBe(
    within(main).getByRole('heading', { level: 3, name: CARD }),
  )
  expect(window.location.hash, '21 T.2: the hash is untouched').toBe(HASH)
})

test('21 T.2: the intro is the two lines of the strings key, one paragraph per newline', async () => {
  const { main } = await form()
  expect(INTRO_LINES.length, 'the intro of section 2 is two lines').toBe(2)
  const card = cardOf(main, CARD)
  const texts = [...card.querySelectorAll('p')].map((p) => p.textContent ?? '')
  for (const line of INTRO_LINES)
    expect(texts, `21 T.2: an intro paragraph: ${line.slice(0, 24)}`).toContain(
      line,
    )
  const first = [...card.querySelectorAll('p')].find(
    (p) => (p.textContent ?? '') === INTRO_LINES[0],
  )!
  expect(isHidden(first), '21 T.2: the intro is shown').toBe(false)
  const second = [...card.querySelectorAll('p')].find(
    (p) => (p.textContent ?? '') === INTRO_LINES[1],
  )!
  expect(before(first, second), '21 T.2: the lines in order').toBe(true)
})

test('21 T.2: one fieldset legend Rules; everything after the legend is in one wrapper div holding the format paragraph, the rules, and Add rule, in that order; the Rules as text Field of 21 T.10 follows the fieldset', async () => {
  const { main } = await form(file(EXPS, `${EMBD}=CPU`))
  const card = cardOf(main, CARD)
  const fs = rulesFieldset(card)
  // TP1 pinned this structurally as an absence (nothing follows the Rules
  // fieldset, so TP2's Field had no place to sit); 21 T.10 retargets it onto
  // the Field, with nothing between the two.
  const after = fs.nextElementSibling
  expect(after, '21 T.10: the Field after the Rules fieldset').not.toBeNull()
  expect(
    within(after as HTMLElement).queryByRole('button', {
      name: `${en('field-about')} ${en('model-profile-ot-text')}`,
    }),
    '21 T.10: it is the Rules as text Field',
  ).not.toBeNull()
  const legend = fs.querySelector('legend')
  expect(
    (legend?.textContent ?? '').replace(/\s+/g, ' ').trim(),
    '21 T.2: the visible legend is model-profile-ot-rules',
  ).toBe(RULES)
  const kids = [...fs.children].filter((el) => el.tagName !== 'LEGEND')
  expect(kids.length, '21 T.2: one wrapper div after the legend').toBe(1)
  const wrapper = kids[0] as HTMLElement
  expect(wrapper.tagName, '21 T.2: a div').toBe('DIV')
  const format = formatParagraph()
  expect(
    (format.textContent ?? '').replace(/\s+/g, ' ').trim(),
    '21 T.2: the format paragraph, the English of the key',
  ).toBe(FORMAT_TEXT.replace(/\s+/g, ' ').trim())
  expect(format.closest('fieldset'), '21 T.2: inside the fieldset').toBe(fs)
  expect(wrapper.contains(format), '21 T.2: in the wrapper div').toBe(true)
  const list = within(card).getByRole('list')
  expect(wrapper.contains(list), '21 T.2: the list is in the wrapper div').toBe(
    true,
  )
  expect(before(format, list), '21 T.2: the format paragraph comes first').toBe(
    true,
  )
  const add = addOf(card)
  expect(wrapper.contains(add), '21 T.2: Add rule is in the wrapper div').toBe(
    true,
  )
  expect(before(list, add), '21 T.2: Add rule comes last').toBe(true)
})

// ── 21 T.3: the list ──────────────────────────────────────────────────────

test('21 T.3: the rules of the value are one li each in file order, each a fieldset whose visible legend is model-profile-ot-rule with its 1-based position', async () => {
  const { main } = await form(file(HAND, `${EMBD}=ROCm0`, `${EXPS}=CPU`))
  // The first piece has no `=`, so its rule has no device, and that list is the
  // answered one (21 T.5): the device read has to have landed.
  await landed(main)
  const card = cardOf(main, CARD)
  const items = ruleItems(card)
  expect(items.length, '21 T.3: one li per rule, in file order').toBe(3)
  for (const [n, group] of ruleItems(card)
    .map((li) => li.querySelector('fieldset') as HTMLElement)
    .entries())
    expect(
      legendOf(group),
      '21 T.3: the legend names the rule by its position',
    ).toBe(en('model-profile-ot-rule', `${n + 1}`))
  expect(
    tensorOf(ruleGroup(card, 1)).value,
    '21 T.3: the first rule is the hand-written one, no kind owns it',
  ).toBe('custom')
  expect(
    [
      goesOnOf(ruleGroup(card, 1)).value,
      optionsOf(goesOnOf(ruleGroup(card, 1)))[0],
    ],
    '21 T.3: its piece has no device: (no device) leads, selected',
  ).toEqual(['', NO_DEVICE_OPTION])
  expect(
    tensorOf(ruleGroup(card, 2)).value,
    '21 T.3: the second rule is the embedding',
  ).toBe('embd')
  expect(goesOnOf(ruleGroup(card, 2)).value, '21 T.3: on ROCm0').toBe('ROCm0')
  expect(
    tensorOf(ruleGroup(card, 3)).value,
    '21 T.3: the third rule is the experts',
  ).toBe('exps')
})

test('21 T.3: the legends are renumbered after a removal', async () => {
  const { main } = await form(file(`${EMBD}=CUDA0`, `${EXPS}=CPU`, 'x=CPU'))
  const card = cardOf(main, CARD)
  fireEvent.click(removeOf(ruleGroup(card, 2), 2))
  expect(
    ruleItems(card).map((li) =>
      legendOf(li.querySelector('fieldset') as HTMLElement),
    ),
    '21 T.3: two rules, named Rule 1 and Rule 2',
  ).toEqual([
    en('model-profile-ot-rule', '1'),
    en('model-profile-ot-rule', '2'),
  ])
})

test('21 T.3: no rule (the key absent, empty, or only commas) shows the plain paragraph model-profile-ot-none in place of a list, not a live region', async () => {
  for (const values of [
    { ...AS_LOADED },
    { ...AS_LOADED, [OT]: '' },
    { ...AS_LOADED, [OT]: ' , , ' },
  ] as Record<string, string>[]) {
    // One page per case: the loop's second render needs the first one gone.
    cleanup()
    const { main } = await form(values)
    const card = cardOf(main, CARD)
    expect(
      within(card).queryByRole('list'),
      '21 T.3: no list when there is no rule',
    ).toBeNull()
    const line = within(card).getByText(NONE)
    expect(line.tagName, '21 T.2: a plain paragraph').toBe('P')
    expect(
      line.getAttribute('aria-live'),
      'section 5: not a live region',
    ).toBeNull()
    expect(isHidden(line), '21 T.3: the empty line is shown').toBe(false)
    expect(addOf(card), '21 T.7: Add rule is shown with no rule').toBeTruthy()
  }
})

test('21 TP1 review D2: removing the last rule leaves the model-profile-ot-none paragraph alone: no list of zero items beside it, and focus on Add rule', async () => {
  // 21 T.3: the paragraph comes INSTEAD of the list. The card kept its
  // `ul role="list"` while the page was dirty with no rule left, so the
  // screen reader heard "list, 0 items" next to the words that say why there
  // is none (the review's D2; focus goes to Add rule either way, 21 T.7).
  const { main } = await form(file(`${EMBD}=CPU`))
  const card = cardOf(main, CARD)
  fireEvent.click(removeOf(ruleGroup(card, 1), 1))
  expect(
    within(card).queryByRole('list'),
    '21 TP1 review D2: the last rule is gone: no empty list is left beside the paragraph',
  ).toBeNull()
  const line = within(card).getByText(NONE)
  expect(line.tagName, '21 T.3: the paragraph is shown, a plain p').toBe('P')
  expect(isHidden(line), '21 T.3: it is shown, not hidden').toBe(false)
  expect(
    document.activeElement,
    '21 T.7: with the last rule gone focus goes to Add rule, list or no list',
  ).toBe(addOf(card))
})

// ── 21 T.4: a rule's controls ─────────────────────────────────────────────

test('21 T.4: the Tensor select is labelled Tensor alone and holds one option per kind, value its id, text its label key, in the table order', async () => {
  const { main } = await form(file(`${EMBD}=CUDA0`))
  const group = ruleGroup(cardOf(main, CARD), 1)
  const sel = tensorOf(group)
  expect(
    within(main).getByRole('combobox', { name: TENSOR }),
    '21 T.4: the group is "Rule 1", the control is named "Tensor"',
  ).toBe(sel)
  expect(
    optionsOf(sel).map((o) => o.value),
    '21 T.4: one option per kind, in order',
  ).toEqual(KIND_IDS)
  expect(
    optionsOf(sel).map((o) => o.text),
    '21 T.4: each option is its kind label',
  ).toEqual(KIND_IDS.map((id) => en(`model-profile-ot-kind-${id}`)))
})

test('21 T.4: Tensor selects the kind of a canned pattern, the kind and layers of a composed one, and Custom for a pattern no kind owns', async () => {
  const { main } = await form(
    file(
      `${EMBD}=CUDA0`,
      `${EXPS}=CPU`,
      `blk\\.(0|1|2)\\.${String.raw`ffn_down_(ch|)exps`}=CPU`,
      `${HAND}=CPU`,
      `blk\\.7\\.${String.raw`attn_`}=ROCm0`,
      `${EXPS_DOWN}=CPU`,
    ),
  )
  const card = cardOf(main, CARD)
  const values = [1, 2, 3, 4, 5, 6].map(
    (n) => tensorOf(ruleGroup(card, n)).value,
  )
  expect(values, '21 T.4: the kind each pattern selects').toEqual([
    'embd',
    'exps',
    'exps_down',
    'custom',
    'attn',
    // The experts families' own text carries the `(ch|)` group (21 T.1):
    // the same field written without it belongs to no kind.
    'custom',
  ])
})

test('21 T.4: Layers is shown only for a kind with a block, labelled Layers, aria-describedby the format paragraph, showing layersText of the rule, empty for every layer', async () => {
  const { main } = await form(
    file(
      `${EMBD}=CUDA0`,
      `blk\\.(10|11|12|15)\\.${String.raw`ffn_(up|down|gate|gate_up)_(ch|)exps`}=CPU`,
      `${EXPS}=CPU`,
    ),
  )
  const card = cardOf(main, CARD)
  expect(
    layersOf(ruleGroup(card, 1)),
    '21 T.4: no Layers for a kind with no block',
  ).toBeNull()
  expect(
    patternOf(ruleGroup(card, 1)),
    '21 T.4: no Pattern for a kind',
  ).toBeNull()
  const withLayers = layersOf(ruleGroup(card, 2))!
  expect(withLayers.value, '21 T.4: layersText of the rule').toBe('10-12, 15')
  expect(
    describedIds(withLayers),
    '21 T.4: the format paragraph describes Layers',
  ).toContain(FORMAT_KEY)
  expect(
    layersOf(ruleGroup(card, 3))!.value,
    '21 T.4: every layer is an empty Layers field',
  ).toBe('')
})

test('21 T.4: Remove is a secondary button named "Remove Rule 2" whose name starts with its visible text, its icon aria-hidden', async () => {
  const { main } = await form(file(`${EMBD}=CUDA0`, `${EXPS}=CPU`))
  const card = cardOf(main, CARD)
  const button = removeOf(ruleGroup(card, 2), 2)
  expect(
    nameOf(button),
    '21 T.4: the name is the visible text plus the legend (WCAG 2.5.3)',
  ).toBe(`${REMOVE} ${en('model-profile-ot-rule', '2')}`)
  const visible = [...button.childNodes]
    .filter(
      (n) =>
        !(n instanceof Element && n.getAttribute('aria-hidden') === 'true'),
    )
    .map((n) => n.textContent ?? '')
    .join('')
    .replace(/\s+/g, ' ')
    .trim()
  expect(visible, '21 T.4: the visible text is model-profile-ot-remove').toBe(
    REMOVE,
  )
  expect(
    button.querySelector('[aria-hidden="true"]'),
    '21 T.4: the x icon is hidden from the screen reader',
  ).not.toBeNull()
})

test('21 T.4: Pattern is shown only when Tensor shows Custom: labelled Pattern, aria-describedby the format paragraph, spellcheck off, autocapitalize off, monospace, holding the rule pattern', async () => {
  const { main } = await form(file(`${HAND}=CPU`, `${EMBD}=CUDA0`))
  const card = cardOf(main, CARD)
  expect(
    patternOf(ruleGroup(card, 2)),
    '21 T.4: no Pattern for a kind row',
  ).toBeNull()
  const input = patternOf(ruleGroup(card, 1))!
  expect(input.value, '21 T.4: the rule pattern, as written').toBe(HAND)
  expect(input.className, '21 T.4: the pattern is monospace').toContain('mono')
  expect(
    describedIds(input),
    '21 T.4: the format paragraph describes Pattern',
  ).toContain(FORMAT_KEY)
  expect(input.getAttribute('spellcheck'), '21 T.4: no spellcheck').toBe(
    'false',
  )
  expect(
    input.getAttribute('autocapitalize'),
    '21 T.4: no autocapitalize',
  ).toBe('off')
  const family = tensorOf(ruleGroup(card, 1))
  expect(family.value, '21 T.4: Custom selected').toBe('custom')
  fireEvent.click(addOf(card))
  const added = ruleGroup(cardOf(main, CARD), 3)
  expect(
    patternOf(added),
    '21 T.4: a rule of a canned kind shows no Pattern',
  ).toBeNull()
})

// ── 21 TP1 review (the designer, 2026-11-23): D1, the shape of a rule row ───

test('21 TP1 review D1: the rule is a plain fieldset with its legend above, and every visible label and its control sit in one span item of the row, so a label never ends beside another control', async () => {
  // The rule row is the markup half of the review's D1 (its CSS half is
  // `Reflow.test.ts`). A `fieldset className="field"` row is a grid from 720 px
  // whose label cell is 200 px, so its control cell is 0 px wide and the
  // controls are 18 px; label and control as two items of the wrapping row
  // split at 320 px, the "Pattern" label beside Remove and its input on the
  // next line (build/ux-review/tp1/card-320.png). The review's fix: a plain
  // fieldset, and one item per label and control (21 T.4).
  const { main } = await form(file(`${EXPS}=CPU`, `${HAND}=CPU`))
  const card = cardOf(main, CARD)
  // A kind with a block shows Layers, Custom shows Pattern: the four label and
  // control pairs of 21 T.4 across the two rows.
  for (const [n, keys] of [
    [1, [TENSOR, LAYERS, GOES]],
    [2, [TENSOR, GOES, PATTERN]],
  ] as Array<[number, string[]]>) {
    const group = ruleGroup(card, n)
    expect(group.tagName, '21 T.3: the rule is a fieldset').toBe('FIELDSET')
    expect(
      group.classList.contains('field'),
      '21 TP1 review D1: a plain fieldset, not a .field: from 720 px a .field row is a grid of 200px and 0px, so its control is 0 px wide',
    ).toBe(false)
    expect(
      group.firstElementChild?.tagName,
      '21 TP1 review D1: the legend is above the controls, the first child of the fieldset',
    ).toBe('LEGEND')
    const kids = [...group.children].filter((el) => el.tagName !== 'LEGEND')
    expect(
      kids.length,
      '21 T.4: the fieldset holds one wrapping flex row after its legend',
    ).toBe(1)
    const row = kids[0] as HTMLElement
    expect(row.tagName, '21 T.4: the row is a div').toBe('DIV')
    const controls: Record<string, HTMLElement | null> = {
      [TENSOR]: tensorOf(group),
      [LAYERS]: layersOf(group),
      [GOES]: goesOnOf(group),
      [PATTERN]: patternOf(group),
    }
    for (const key of keys) {
      const control = controls[key]
      expect(control, `21 T.4: ${key} is shown for this rule`).not.toBeNull()
      itemOf(
        control as HTMLElement,
        row,
        `${key} of ${en('model-profile-ot-rule', `${n}`)}`,
      )
    }
  }
})

// ── 21 TP1 review D4 and D5 (the designer, 2026-11-23): the sheet, the widths ──

// The stylesheet half of the D4 re-check, read the way `Reflow.test.ts` reads
// `index.css` (comments dropped, the same rule regex, the same nesting depth;
// a test file exports no helper, so the reader is a small local copy).

const CSS = readFileSync(
  join(import.meta.dirname, 'index.css'),
  'utf8',
).replace(/\/\*[\s\S]*?\*\//g, '')

type Rule = {
  sel: string
  compact: string
  flat: string
  body: string
  depth: number
}

// Rules opened before this offset: the nesting level of the rule that starts
// there, 0 at the top level.
const opened = (upto: number): number =>
  (CSS.slice(0, upto).match(/\{/g) ?? []).length -
  (CSS.slice(0, upto).match(/\}/g) ?? []).length

// SHEET_RULES, not RULES: this file's RULES is the Rules legend text.
const SHEET_RULES: Rule[] = [...CSS.matchAll(/([^{}]+)\{([^{}]*)\}/g)].map(
  (m) => ({
    sel: m[1].trim(),
    compact: m[1].replace(/\s+/g, ''),
    flat: m[1].trim().replace(/\s+/g, ' '),
    body: m[2],
    depth: opened(m.index ?? 0),
  }),
)

// Top-level rules only: the fix applies at every width, not in a media block.
const TOP = SHEET_RULES.filter((r) => r.depth === 0)

// The value of a declaration; the last one when a block repeats the property.
const prop = (body: string, name: string): string => {
  let val = ''
  for (const d of body.split(';')) {
    const i = d.indexOf(':')
    if (i >= 0 && d.slice(0, i).trim() === name) val = d.slice(i + 1).trim()
  }
  return val
}

const norm = (v: string): string => v.replace(/\s+/g, ' ').trim()

const single = (list: Rule[], what: string): Rule => {
  if (list.length !== 1)
    throw new Error(
      `expected exactly one rule: ${what}, found ${list.length}` +
        (list.length
          ? ` (${list.map((r) => r.sel.replace(/\s+/g, ' ')).join(' | ')})`
          : ''),
    )
  return list[0]
}

// The top-level rule styling this selector, as one part of a comma group:
// the sheet's own style for a shared declaration (index.css 1399), so the
// coder's one rule for both selects is as good as two.
const topStyling = (sel: string, what: string): Rule =>
  single(
    TOP.filter((r) =>
      r.sel
        .split(',')
        .some((p) => p.replace(/\s+/g, '') === sel.replace(/\s+/g, '')),
    ),
    `the top-level rule styling "${sel}" (${what})`,
  )

test('21 TP1 review D4 and D5 (the sheet): .ot-tensor, .ot-goes-on and .ot-layers are flex rows themselves (display: flex, flex-wrap: wrap, align-items: center, gap: var(--space-2)) and carry min-width: 0, so the label and the control wrap inside the item', () => {
  // D4 (the re-check of 2026-11-23, `npm --prefix fe/ux run check`): the
  // select sits inside the item span the D1 fix made (TensorPlacement.tsx
  // 310, 337), not directly in the rule row, and the shared `select` rule
  // sets no width: the item has `min-width: 0` but the select inside it
  // keeps its longest option's width, so `model-profile-placement` scrolls
  // sideways at 320 px in all three engines (430 of 320 in Chromium,
  // build/ux-review/tp1/run2.txt), and the 64-character device select spills
  // 59 px at 1280 px too. The review's fix: the item is a flex row of its
  // own, as `.ot-rule-row` is, so its select is an item of the item.
  // D5 (the second re-check of the same day): the Layers span gets the same
  // row and min-width: 0 on all three. Today `.ot-layers` has no rule of its
  // own: its min-content is the input's `10ch` and the input's
  // `max-width: 100%` is measured against that same span, so at 200 % text
  // on a 1280 px window both Layers inputs reach x 725 while the rule's
  // column ends at 695, 30 px past it, in all three engines
  // (build/ux-review/tp1/run3.txt). The pin is per name (topStyling): the
  // coder's own rule for the span, or the sheet's comma group beside the
  // two items, both pass.
  for (const sel of ['.ot-tensor', '.ot-goes-on', '.ot-layers']) {
    const item = topStyling(
      sel,
      'the Tensor / Goes on / Layers item of a rule row',
    )
    for (const [name, value] of [
      ['display', 'flex'],
      ['flex-wrap', 'wrap'],
      ['align-items', 'center'],
      ['gap', 'var(--space-2)'],
      ['min-width', '0'],
    ] as Array<[string, string]>)
      expect(
        norm(prop(item.body, name)),
        `D4/D5: ${sel}: the item is a flex row of its own (the four of .ot-rule-row) with min-width: 0, so its control is an item of it and can shrink: ${name}`,
      ).toBe(value)
  }
})

test('21 TP1 review D4 (the sheet): the Tensor and Goes on selects declare flex: 1 1 12rem and min-width: 0, so a select never keeps the width of its longest option', () => {
  // The item is a flex row (the test above) and the select is one of its
  // items; a flex item is floored at its min-content, which for a select is
  // the width of its longest option. The basis gives it the room beside the
  // label and `min-width: 0` lets it go below its content (CSS Flexbox 1),
  // the same pair the 15 section 11 C2 review D2 pins for the suggestions
  // list. One comma group for both selects is as good as two rules: the pin
  // is per selector.
  for (const sel of ['.ot-tensor select', '.ot-goes-on select']) {
    const rule = topStyling(sel, 'the select the item holds')
    for (const [name, value] of [
      ['flex', '1 1 12rem'],
      ['min-width', '0'],
    ] as Array<[string, string]>)
      expect(
        norm(prop(rule.body, name)),
        `D4: ${sel}: the select is an item of the item, floored at its longest option without this: ${name}`,
      ).toBe(value)
  }
})

test('21 TP1 review D4 and D5 (the sheet): .ot-layers input.number declares max-width: 100% and min-width: 0, so the Layers input never spills the column at 200 % text', () => {
  // At 200 % text on a 1280 px window the card's control column is 190 px
  // and the Layers input, about 10 characters wide, spills it (the review).
  // The cap keeps it inside the item, as `.ot-pattern input { width: 100% }`
  // keeps the Pattern input. D5: the cap is measured against the span, so
  // the input needs `min-width: 0` beside it: once the span is a flex row
  // (the test above) the input is a flex item floored at its min-content,
  // the `10ch` of input.number, without it.
  const input = topStyling(
    '.ot-layers input.number',
    'the Layers input of a rule row',
  )
  for (const [name, value] of [
    ['max-width', '100%'],
    ['min-width', '0'],
  ] as Array<[string, string]>)
    expect(
      norm(prop(input.body, name)),
      `D4/D5: .ot-layers input.number: the cap alone is measured against the span, the floor is what lets it shrink: ${name}`,
    ).toBe(value)
})

// ── 21 T.5: Goes on options ───────────────────────────────────────────────

test('21 T.5: once the device read answered 200, the options are CPU as model-profile-ot-cpu, then every probed GPU in probe order, value its id, text model-profile-device-option, the id alone when its name is null', async () => {
  const probe: DeviceProbe = {
    probedAt: DEVICES.probedAt,
    devices: (DEVICES.devices ?? []).map((d) =>
      d.id === 'Vulkan0' ? { ...d, name: null } : d,
    ),
  }
  const { main } = await form(file(`${EMBD}=CUDA0`), { devices: probe })
  await landed(main, probe)
  expect(
    optionsOf(goesOnOf(ruleGroup(cardOf(main, CARD), 1))),
    '21 T.5: CPU first, then the GPUs in probe order',
  ).toEqual([
    { value: 'CPU', text: CPU_TEXT },
    gpuOption('CUDA0', 'NVIDIA GeForce RTX 5080'),
    gpuOption('ROCm0', 'AMD Radeon AI PRO R9700'),
    gpuOption('Vulkan0', null),
  ])
})

test('21 T.5: the rule device no listed value equals exactly (case included) is the last option, model-profile-device-option-missing when the probe answered with a time, the device itself when it gave no time', async () => {
  const noTime: DeviceProbe = { probedAt: null, devices: DEVICES.devices }
  for (const [probe, detected] of [
    [DEVICES, true],
    [noTime, false],
  ] as [DeviceProbe, boolean][]) {
    // One page per case: the loop's second render needs the first one gone.
    cleanup()
    const { main } = await form(file(`${EMBD}=rocm0`), { devices: probe })
    await landed(main, probe)
    expect(
      optionsOf(goesOnOf(ruleGroup(cardOf(main, CARD), 1))),
      '21 T.5: the file device is the last option, spelling kept',
    ).toEqual([...deviceOptions(probe), missingOption('rocm0', probe)])
    expect(
      goesOnOf(ruleGroup(cardOf(main, CARD), 1)).value,
      '21 T.5: the rule keeps its own device',
    ).toBe('rocm0')
    const opts = optionsOf(goesOnOf(ruleGroup(cardOf(main, CARD), 1)))
    expect(
      opts[opts.length - 1]!.text,
      `21 T.5: the ${detected ? 'not detected' : 'bare'} text, as the last option`,
    ).toBe(
      detected ? en('model-profile-device-option-missing', 'rocm0') : 'rocm0',
    )
  }
})

test('21 T.5: a rule with no device shows a first option value "" with the text model-profile-ot-no-device, selected', async () => {
  const { main } = await form(file(`${EMBD}`))
  await landed(main)
  const sel = goesOnOf(ruleGroup(cardOf(main, CARD), 1))
  expect(optionsOf(sel)[0], '21 T.5: the no-device option is first').toEqual(
    NO_DEVICE_OPTION,
  )
  expect(sel.value, '21 T.5: it is selected').toBe('')
  expect(
    optionsOf(sel).slice(1),
    '21 T.5: then CPU and the probed GPUs',
  ).toEqual(deviceOptions())
})

test('21 T.5: while the device read is pending, and when it failed, the options are CPU and the rule own device', async () => {
  const h = held()
  for (const options of [
    { devices: h.answer },
    { devices: reply(503) },
  ] as BffOptions[]) {
    // One page per case: the loop's second render needs the first one gone.
    cleanup()
    const { main } = await form(file(`${EMBD}=CUDA9`, `${EXPS}=CPU`), options)
    const card = cardOf(main, CARD)
    expect(
      optionsOf(goesOnOf(ruleGroup(card, 1))),
      '21 T.5: pending or failed: CPU and the file device',
    ).toEqual([
      { value: 'CPU', text: CPU_TEXT },
      { value: 'CUDA9', text: 'CUDA9' },
    ])
    expect(
      optionsOf(goesOnOf(ruleGroup(card, 2))),
      '21 T.5: a rule on CPU needs no extra option',
    ).toEqual([{ value: 'CPU', text: CPU_TEXT }])
  }
})

test('21 TP1 review D3: a rule with no device shows the model-profile-ot-no-device option first, selected, while the device read is pending and when it failed', async () => {
  // The `""` option was added only once the read had answered, so while it was
  // pending the select had no option matching the rule's value: the page showed
  // "CPU: system memory" for a rule with NO device (the review's D3). 21 T.5:
  // the `""` option comes first whatever the read.
  for (const options of [
    { devices: held().answer },
    { devices: reply(503) },
  ] as BffOptions[]) {
    // One page per case: the loop's second render needs the first one gone.
    cleanup()
    const { main } = await form(file(`${EMBD}`), options)
    const sel = goesOnOf(ruleGroup(cardOf(main, CARD), 1))
    expect(
      optionsOf(sel)[0],
      '21 TP1 review D3: pending or failed: the rule with no device leads with (no device), as the answered read does',
    ).toEqual(NO_DEVICE_OPTION)
    expect(
      sel.value,
      '21 TP1 review D3: the select reads (no device), not CPU: its value has an option to match',
    ).toBe('')
    expect(
      optionsOf(sel).slice(1),
      '21 T.5: pending: CPU follows, and the rule owns no other device to list',
    ).toEqual([{ value: 'CPU', text: CPU_TEXT }])
  }
})

// ── 21 T.6: editing a rule ────────────────────────────────────────────────

test('21 T.6: a Tensor change rewrites that rule only, keeps the order of the rules, and leaves the focus on the select', async () => {
  const { bff, main } = await form(
    file(`${EXPS}=CPU`, `${EMBD}=ROCm0`, `${EXPS}=CUDA0`),
  )
  await landed(main)
  const card = cardOf(main, CARD)
  const sel = tensorOf(ruleGroup(card, 2))
  sel.focus()
  fireEvent.change(sel, { target: { value: 'output' } })
  expect(
    tensorOf(ruleGroup(cardOf(main, CARD), 2)).value,
    '21 T.6: the row shows the new kind',
  ).toBe('output')
  expect(
    document.activeElement,
    '21 T.6: focus stays on the Tensor select',
  ).toBe(tensorOf(ruleGroup(cardOf(main, CARD), 2)))
  await save(main, bff)
  expect(
    sentValues(bff)[OT],
    '21 T.6: the rule is the output head, the other two rules in place',
  ).toBe(`${EXPS}=CPU,${String.raw`^output\.weight`}=ROCm0,${EXPS}=CUDA0`)
})

test('21 T.6: a kind change composes with the layers the row keeps: Layers hidden for a kind without a block and shown again with them', async () => {
  // The row's experts family limited to blocks 0 to 2, as `compose` writes it.
  const withLayers = `blk\\.(0|1|2)${EXPS}`
  const { bff, main } = await form(file(`${withLayers}=CPU`))
  const card = cardOf(main, CARD)
  expect(
    layersOf(ruleGroup(card, 1))!.value,
    'the row starts with layers',
  ).toBe('0-2')
  fireEvent.change(tensorOf(ruleGroup(card, 1)), { target: { value: 'embd' } })
  expect(
    layersOf(ruleGroup(cardOf(main, CARD), 1)),
    '21 T.6: no Layers field for a kind with no block',
  ).toBeNull()
  fireEvent.change(tensorOf(ruleGroup(cardOf(main, CARD), 1)), {
    target: { value: 'exps' },
  })
  expect(
    layersOf(ruleGroup(cardOf(main, CARD), 1))!.value,
    '21 T.6: the layers are kept while the field is hidden',
  ).toBe('0-2')
  // The kind round trip on its own is an edit undone (21 T.8: exps, embd, exps
  // with the layers kept renders `render(parse(loaded))` exactly, so Save would
  // send no key), so the edit ends in a change of its own: the device. The key
  // is changed, and the value sent carries the layers the row kept.
  fireEvent.change(goesOnOf(ruleGroup(cardOf(main, CARD), 1)), {
    target: { value: 'CUDA0' },
  })
  await save(main, bff)
  expect(
    sentValues(bff)[OT],
    '21 T.6: back to the experts family with its layers',
  ).toBe(`${withLayers}=CUDA0`)
})

test('21 T.6: Layers composes the pattern at each input, and a text parseLayers cannot read leaves the pattern as it was (21 TP2 review D1: with 21 T.11 built, Save refuses it)', async () => {
  const { bff, main } = await form(file(`${EXPS}=CPU`))
  const card = cardOf(main, CARD)
  const layers = layersOf(ruleGroup(card, 1))!
  layers.focus()
  type(layers, '10-12, 15')
  expect(document.activeElement, '21 T.6: focus stays on Layers').toBe(
    layersOf(ruleGroup(cardOf(main, CARD), 1)),
  )
  type(layers, '10-')
  expect(
    layersOf(ruleGroup(cardOf(main, CARD), 1))!.value,
    '21 T.6: the half-typed text is shown as typed',
  ).toBe('10-')
  // 21 T.6, ruling of 2026-11-22 (reading (a)): a text `parseLayers` cannot
  // read leaves the pattern as the last readable text composed it in this
  // visit, not the loaded one. `10-12, 15` composed the rule to the experts
  // family on blocks 10, 11, 12 and 15, and `10-` leaves it there. The Rules
  // as text field shows `render` of the rows once they are an edit (21 T.10),
  // so it is how this reads the pattern the row kept.
  expect(
    textField(cardOf(main, CARD)).value,
    '21 T.6: the pattern is the last readable text composed',
  ).toBe(`blk\\.(10|11|12|15)${EXPS}=CPU`)
  // 21 T.11, as the TP2 review's D1 rules it: once the page's own checks are
  // built, Save is refused while the Layers text on screen does not read,
  // whatever was typed before it. Nothing goes out, the input carries
  // model-profile-ot-layers-bad with its text, and the focus lands on it.
  await saveInvalid(main, bff)
  expectError(
    layersOf(ruleGroup(cardOf(main, CARD), 1))!,
    en('model-profile-ot-layers-bad', '10-'),
    '21 T.11: Layers',
  )
  expect(document.activeElement, '21 T.11: focus on Layers').toBe(
    layersOf(ruleGroup(cardOf(main, CARD), 1)),
  )
})

test('21 T.6: Layers of a readable text composes the kind with the layers it reads', async () => {
  const { bff, main } = await form(file(`${EXPS}=CPU`))
  type(layersOf(ruleGroup(cardOf(main, CARD), 1))!, '3, 7-9')
  await save(main, bff)
  expect(sentValues(bff)[OT]).toBe(
    `blk\\.(3|7|8|9)\\.${String.raw`ffn_(up|down|gate|gate_up)_(ch|)exps`}=CPU`,
  )
})

test('21 T.6: Custom keeps the row pattern in Pattern, the typed text becomes the rule pattern, the select keeps showing Custom when the text is a family pattern, and the row remembers its Custom text for the visit', async () => {
  const { bff, main } = await form(file(`${HAND}=CPU`))
  const card = cardOf(main, CARD)
  const input = patternOf(ruleGroup(card, 1))!
  input.focus()
  type(input, 'x.*y')
  expect(
    patternOf(ruleGroup(cardOf(main, CARD), 1))!.value,
    '21 T.6: the text as typed',
  ).toBe('x.*y')
  expect(
    tensorOf(ruleGroup(cardOf(main, CARD), 1)).value,
    '21 T.6: focus and the select stay',
  ).toBe('custom')
  expect(document.activeElement, '21 T.6: focus stays on Pattern').toBe(
    patternOf(ruleGroup(cardOf(main, CARD), 1)),
  )
  type(input, EXPS)
  expect(
    tensorOf(ruleGroup(cardOf(main, CARD), 1)).value,
    '21 T.6: a family pattern typed in Custom keeps the row Custom',
  ).toBe('custom')
  await save(main, bff)
  expect(sentValues(bff)[OT], '21 T.6: the typed text is the rule').toBe(
    `${EXPS}=CPU`,
  )
  discard(main)
  fireEvent.change(tensorOf(ruleGroup(cardOf(main, CARD), 1)), {
    target: { value: 'layer' },
  })
  fireEvent.change(tensorOf(ruleGroup(cardOf(main, CARD), 1)), {
    target: { value: 'custom' },
  })
  expect(
    patternOf(ruleGroup(cardOf(main, CARD), 1))!.value,
    '21 T.6: the row shows its remembered Custom text',
  ).toBe(EXPS)
})

test('21 T.6: Goes on takes the option value as the rule device, and focus stays on the select', async () => {
  const { bff, main } = await form(file(`${EMBD}=CPU`, `${EXPS}=CPU`))
  await landed(main)
  const card = cardOf(main, CARD)
  const sel = goesOnOf(ruleGroup(card, 1))
  sel.focus()
  fireEvent.change(sel, { target: { value: 'ROCm0' } })
  expect(
    goesOnOf(ruleGroup(cardOf(main, CARD), 1)).value,
    '21 T.6: the rule is on ROCm0',
  ).toBe('ROCm0')
  expect(document.activeElement, '21 T.6: focus stays on Goes on').toBe(
    goesOnOf(ruleGroup(cardOf(main, CARD), 1)),
  )
  await save(main, bff)
  expect(sentValues(bff)[OT], '21 T.6: only the first rule changed').toBe(
    `${EMBD}=ROCm0,${EXPS}=CPU`,
  )
})

test('21 T.6: rows are keyed by an id of the page, not by index: a removal renumbers the rows and the row that moved keeps its typed text (its focus is 21 T.7)', async () => {
  const { main } = await form(
    file(`${EMBD}=CUDA0`, `${EXPS}=CPU`, `${EXPS}=ROCm0`),
  )
  await landed(main)
  const card = cardOf(main, CARD)
  const third = layersOf(ruleGroup(card, 3))!
  type(third, '10-')
  fireEvent.click(removeOf(ruleGroup(card, 2), 2))
  const moved = ruleGroup(cardOf(main, CARD), 2)
  expect(legendOf(moved), '21 T.6: the third rule is now Rule 2').toBe(
    en('model-profile-ot-rule', '2'),
  )
  expect(
    layersOf(moved)!.value,
    '21 T.6: the row is the same control, its text kept',
  ).toBe('10-')
})

// ── 21 T.7: Add rule and Remove ────────────────────────────────────────────

test('21 T.7: Add rule appends the embedding table on the first probed GPU, and focuses the new rule Tensor select', async () => {
  const { bff, main } = await form(file(`${EXPS}=CPU`))
  await landed(main)
  const card = cardOf(main, CARD)
  fireEvent.click(addOf(card))
  const added = ruleGroup(cardOf(main, CARD), 2)
  expect(
    tensorOf(added).value,
    '21 T.7: the new rule is the embedding table',
  ).toBe('embd')
  expect(goesOnOf(added).value, '21 T.7: on the first probed GPU').toBe('CUDA0')
  expect(
    layersOf(added),
    '21 T.7: the embedding kind has no Layers field',
  ).toBeNull()
  expect(document.activeElement, '21 T.7: focus on the new Tensor select').toBe(
    tensorOf(added),
  )
  expect(
    legendOf(ruleGroup(cardOf(main, CARD), 1)),
    '21 T.7: the rules of the file keep their place',
  ).toBe(en('model-profile-ot-rule', '1'))
  fireEvent.click(addOf(cardOf(main, CARD)))
  expect(
    ruleItems(cardOf(main, CARD)).length,
    '21 T.7: Add appends, it never replaces',
  ).toBe(3)
  await save(main, bff)
  expect(sentValues(bff)[OT]).toBe(`${EXPS}=CPU,${EMBD}=CUDA0,${EMBD}=CUDA0`)
})

test('21 T.7: with no GPU in the probe, and while the probe has not answered, Add rule appends the embedding table on CPU', async () => {
  const noGpu: DeviceProbe = { probedAt: DEVICES.probedAt, devices: [] }
  const h = held()
  for (const options of [
    { devices: noGpu },
    { devices: h.answer },
  ] as BffOptions[]) {
    // One page per case: the loop's second render needs the first one gone.
    cleanup()
    const { bff, main } = await form({ ...AS_LOADED }, options)
    if ((options.devices as DeviceProbe)?.devices) await landed(main, noGpu)
    fireEvent.click(addOf(cardOf(main, CARD)))
    const added = ruleGroup(cardOf(main, CARD), 1)
    expect(goesOnOf(added).value, '21 T.7: CPU, never a blank device').toBe(
      'CPU',
    )
    await save(main, bff)
    expect(sentValues(bff)[OT]).toBe(`${EMBD}=CPU`)
  }
})

test('21 T.7: while the device read has not answered, Add rule appends the new rule on CPU, not on the device of the last rule on screen (21 TP2 review D2)', async () => {
  // The file's rule is on a GPU, so a new rule that carries on the last
  // rule's device lands on a GPU the page has not seen the probe confirm.
  const h = held()
  const { bff, main } = await form(file(`${EXPS}=ROCm0`), {
    devices: h.answer,
  })
  fireEvent.click(addOf(cardOf(main, CARD)))
  const added = ruleGroup(cardOf(main, CARD), 2)
  expect(
    tensorOf(added).value,
    '21 T.7: the new rule is the embedding table',
  ).toBe('embd')
  expect(
    goesOnOf(added).value,
    '21 T.7: CPU while the read has not answered, never the last rule device',
  ).toBe('CPU')
  await save(main, bff)
  expect(
    sentValues(bff)[OT],
    '21 T.7: the new rule is saved on CPU, the loaded rule keeps ROCm0',
  ).toBe(`${EXPS}=ROCm0,${EMBD}=CPU`)
})

test('21 T.7: Remove drops its rule: focus to the next rule Tensor select, else the previous, else Add rule', async () => {
  const { bff, main } = await form(
    file(`${EMBD}=CUDA0`, `${EXPS}=CPU`, `${EXPS}=ROCm0`),
  )
  await landed(main)
  const card = cardOf(main, CARD)
  fireEvent.click(removeOf(ruleGroup(card, 1), 1))
  expect(
    tensorOf(ruleGroup(cardOf(main, CARD), 1)),
    '21 T.7: focus on the rule that moved up',
  ).toBe(document.activeElement)
  expect(
    tensorOf(ruleGroup(cardOf(main, CARD), 1)).value,
    '21 T.7: the experts rule is first now',
  ).toBe('exps')
  fireEvent.click(removeOf(ruleGroup(cardOf(main, CARD), 2), 2))
  expect(
    tensorOf(ruleGroup(cardOf(main, CARD), 1)),
    '21 T.7: the last rule: focus on the previous one',
  ).toBe(document.activeElement)
  fireEvent.click(removeOf(ruleGroup(cardOf(main, CARD), 1), 1))
  expect(
    within(cardOf(main, CARD)).queryAllByRole('listitem'),
    '21 T.7: no rule left',
  ).toHaveLength(0)
  expect(
    within(cardOf(main, CARD)).getByText(NONE),
    '21 T.3: the empty line is back',
  ).toBeTruthy()
  expect(document.activeElement, '21 T.7: focus on Add rule').toBe(
    addOf(cardOf(main, CARD)),
  )
  await save(main, bff)
  expect(sentValues(bff)[OT], '21 T.8: no rule left sends null').toBeNull()
})

// ── 21 T.8: saving ────────────────────────────────────────────────────────

test('21 T.8: an untouched value is never sent: the alias, the spaces, and a hand-written order all read unchanged, and Save sends no key', async () => {
  const values = {
    ...AS_LOADED,
    [OT]: `  ${String.raw`token_embd\.weight`}=CUDA0 , , ${HAND}=CPU , `,
  }
  const { bff, main } = await form(values)
  await landed(main)
  const card = cardOf(main, CARD)
  expect(
    ruleItems(card).length,
    '21 T.8: the rules are read, spaces and empty pieces dropped',
  ).toBe(2)
  fireEvent.click(within(main).getByRole('button', { name: en('save-button') }))
  expect(bff.to('PUT', PUT), '21 T.8: nothing to save').toHaveLength(0)
})

test('21 T.8: a save sends render of the rules on screen, as the only key, and a 200 makes them the loaded ones', async () => {
  const { bff, main } = await form(file(`${EMBD}=CUDA0`))
  await landed(main)
  fireEvent.click(addOf(cardOf(main, CARD)))
  fireEvent.change(goesOnOf(ruleGroup(cardOf(main, CARD), 2)), {
    target: { value: 'CPU' },
  })
  await save(main, bff)
  const sent = sentValues(bff)
  expect(Object.keys(sent), '21 T.8: no other key changes').toEqual([OT])
  expect(sent[OT], '21 T.8: render of the rows').toBe(
    `${EMBD}=CUDA0,${EMBD}=CPU`,
  )
  fireEvent.click(within(main).getByRole('button', { name: en('save-button') }))
  expect(
    bff.to('PUT', PUT).length,
    '21 T.8: the saved rules are the loaded ones: nothing is left to send',
  ).toBe(1)
  expect(
    ruleItems(cardOf(main, CARD)).length,
    '21 T.8: the rows are the loaded ones',
  ).toBe(2)
})

test('21 T.8: an edit undone is unchanged, and Discard restores the loaded rules with the rows remembered texts dropped', async () => {
  const { bff, main } = await form(file(`${HAND}=CPU`))
  await landed(main)
  const card = cardOf(main, CARD)
  const input = patternOf(ruleGroup(card, 1))!
  // 21 T.8 spells the rule as a comparison, not as a "something was edited"
  // flag: the key is changed when `render` of the rows on screen differs from
  // `render(parse(loaded value))`, so a pattern typed and typed back leaves
  // nothing to send (a typed edit is a change: 21 T.8's other test).
  type(input, 'x.*y')
  type(input, HAND)
  fireEvent.click(within(main).getByRole('button', { name: en('save-button') }))
  expect(
    bff.to('PUT', PUT),
    '21 T.8: an edit undone is unchanged',
  ).toHaveLength(0)
  fireEvent.click(addOf(cardOf(main, CARD)))
  discard(main)
  expect(
    ruleItems(cardOf(main, CARD)).length,
    '21 T.8: Discard restored the one loaded rule',
  ).toBe(1)
  expect(
    patternOf(ruleGroup(cardOf(main, CARD), 1))!.value,
    '21 T.8: the remembered text is dropped with the row',
  ).toBe(HAND)
  fireEvent.click(within(main).getByRole('button', { name: en('save-button') }))
  expect(
    bff.to('PUT', PUT),
    '21 T.8: after Discard, nothing to send',
  ).toHaveLength(0)
})

// ── 21 T.9: the keyboard ──────────────────────────────────────────────────

test('21 T.9: Tab in a rule goes Tensor, Layers, Goes on, Remove, and after the last rule Add rule, then Rules as text (21 T.13)', async () => {
  const { main } = await form(file(`${EXPS}=CPU`))
  await landed(main)
  expect(
    tabbable(cardOf(main, CARD)).map(nameOf),
    '21 T.9: one rule of a kind with a block',
  ).toEqual([
    TENSOR,
    LAYERS,
    GOES,
    `${REMOVE} ${en('model-profile-ot-rule', '1')}`,
    ADD,
    ...TEXT_STOPS,
  ])
})

test('21 T.9: Tab in a Custom rule goes Tensor, Goes on, Remove, Pattern, and then Add rule, then Rules as text (21 T.13)', async () => {
  const { main } = await form(file(`${HAND}=CPU`, `${EMBD}=CUDA0`))
  await landed(main)
  const card = cardOf(main, CARD)
  const stops = tabbable(card).map(nameOf)
  expect(
    stops,
    '21 T.9: the Custom row, then the embd row, then Add rule',
  ).toEqual([
    TENSOR,
    GOES,
    `${REMOVE} ${en('model-profile-ot-rule', '1')}`,
    PATTERN,
    TENSOR,
    GOES,
    `${REMOVE} ${en('model-profile-ot-rule', '2')}`,
    ADD,
    ...TEXT_STOPS,
  ])
})
