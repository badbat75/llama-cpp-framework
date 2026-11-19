// The shared rules of the model profile page's plain cards and the first two cards
// (fe/briefs/02e-model-profile-cards.md, criteria 83 to 95): the section that holds
// the h2 and the jump list of 2h.10, every control natively disabled, how a number
// and a select show the strings of the file, absent keys, empty keys, values a
// mapping does not list, the hints, and units. The cards are Resource /
// context (92) and Embeddings (93). The route, the read and its states are
// ModelProfilePage.test.tsx (2d.1 to 2d.4), the Engine field and the Assets card are
// ModelProfileContent.test.tsx (2d.5 to 2d.9). The colours of 85, the width of 91
// and the words a screen reader uses are checked in the designer's review,
// not in jsdom.
import {
  cleanup,
  fireEvent,
  render,
  screen,
  within,
} from '@testing-library/react'
import { afterEach, beforeEach, expect, test, vi } from 'vitest'
import App from './App.tsx'
import { t } from './strings.ts'
import {
  before,
  en,
  fakeBff,
  isHidden,
  modelProfileDoc,
  reply,
  startAt,
  type BffOptions,
} from './test/bff.ts'
import {
  cardHeadings,
  cardHeadingKeys,
  fieldTabStops,
  pageStartTabStops,
  saveBarTabStops,
  isEditableField,
  controlEnabled,
} from './test/modelProfileCards.ts'

const ID = 'Qwen3-27B'
const HASH = `#/profiles/model/${ID}`
const STRATA = 'Strata'
/** 2h.10's 194: the text of the Notice that left the page and the strings
 * file, kept as a literal so its absence can be pinned. */
const NOTICE =
  'Some settings cannot be changed here yet: their controls are greyed out. Change those in pai-config on the server machine.'
const MODEL = '/var/lib/private-ai/.llama.cpp/models/Qwen3-27B-Q8_0.gguf'

/** The criterion 94 stub: every key of 92 and 93 set. */
const SET: Record<string, string> = {
  'ctx-size': '172032',
  parallel: '2',
  'kv-unified': 'true',
  'kv-unified-per-slot': '32768',
  'batch-size': '4096',
  'ubatch-size': '1024',
  embeddings: 'true',
  pooling: 'last',
}

/** The shared option texts of section 1.3. */
const TRISTATE = [en('option-default'), en('option-on'), en('option-off')]
const OFF_ON = [en('option-off'), en('option-on')]
/** Pooling's options, in the GUI's order (MP 763-764); `cls` is no string. */
const POOLING = [
  en('option-default'),
  en('option-none'),
  en('option-mean'),
  'cls',
  en('option-last'),
  en('option-rank'),
]

/** One row of the field tables of 92 and 93. */
type Row = {
  /** The strings key of the card's h3. */
  card: string
  /** The key of the section's `values`. */
  key: string
  /** The strings key of the field's label. */
  label: string
  kind: 'number' | 'select'
  /** A select's option texts, in the table's order. */
  options?: string[]
  /** A select's option values, where criterion 93 gives them. */
  optionValues?: string[]
  /** The option text a select shows when the key is absent. */
  absentOption?: string
  /** The value of the criterion 94 stub, and what the field shows for it:
   * the input's value, or the selected option's text. */
  set: string
  shown: string
}

/** The field table of 92 and 93, in the cards' and the GUI's order. */
const FIELDS: Row[] = [
  {
    card: 'model-profile-resource-context',
    key: 'ctx-size',
    label: 'model-profile-context-size',
    kind: 'number',
    set: '172032',
    shown: '172032',
  },
  {
    card: 'model-profile-resource-context',
    key: 'parallel',
    label: 'model-profile-parallel-slots',
    kind: 'number',
    set: '2',
    shown: '2',
  },
  {
    card: 'model-profile-resource-context',
    key: 'kv-unified',
    label: 'model-profile-unified-kv',
    kind: 'select',
    options: TRISTATE,
    absentOption: en('option-default'),
    set: 'true',
    shown: en('option-on'),
  },
  {
    card: 'model-profile-resource-context',
    key: 'kv-unified-per-slot',
    label: 'model-profile-per-slot-context',
    kind: 'number',
    set: '32768',
    shown: '32768',
  },
  {
    card: 'model-profile-resource-context',
    key: 'batch-size',
    label: 'model-profile-batch-size',
    kind: 'number',
    set: '4096',
    shown: '4096',
  },
  {
    card: 'model-profile-resource-context',
    key: 'ubatch-size',
    label: 'model-profile-u-batch-size',
    kind: 'number',
    set: '1024',
    shown: '1024',
  },
  {
    card: 'model-profile-embeddings',
    key: 'embeddings',
    label: 'model-profile-embedding-mode',
    kind: 'select',
    options: OFF_ON,
    absentOption: en('option-off'),
    set: 'true',
    shown: en('option-on'),
  },
  {
    card: 'model-profile-embeddings',
    key: 'pooling',
    label: 'model-profile-pooling',
    kind: 'select',
    options: POOLING,
    optionValues: ['default', 'none', 'mean', 'cls', 'last', 'rank'],
    absentOption: en('option-default'),
    set: 'last',
    shown: en('option-last'),
  },
]

/** The controls 92 and 93 put in the two cards: a box and an input per number. */
const CONTROLS =
  FIELDS.filter((f) => f.kind === 'number').length * 2 +
  FIELDS.filter((f) => f.kind === 'select').length

const RESOURCE_CARDS = [
  'model-profile-resource-context',
  'model-profile-embeddings',
]

beforeEach(() => startAt(HASH))

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
  startAt('')
})

// ── Helpers (as ModelProfileContent.test.tsx) ─────────────────────────────────

/** The app at the address in the location; returns the fake bff and main. */
function open(options: BffOptions = {}) {
  const bff = fakeBff(options)
  render(<App />)
  return { bff, main: screen.getByRole('main') }
}

/** Waits for the loaded page's content; the Assets card of 2d is shown. */
async function loaded(options: BffOptions = {}) {
  const { bff, main } = open(options)
  const h1 = await within(main).findByRole('heading', { level: 1, name: ID })
  const h3 = await within(main).findByRole('heading', {
    level: 3,
    name: en('model-profile-assets'),
  })
  return { bff, main, h1, h3 }
}

/** A read with `values` added to the Assets keys, `extra` on top. */
function docOf(
  values: Record<string, string>,
  extra: Record<string, unknown> = {},
) {
  return modelProfileDoc({ model: MODEL, ...values }, extra)
}

/** A read whose `values` are exactly `values` (the `{}` stub of 94). */
function exactOf(values: Record<string, string>) {
  return modelProfileDoc({}, { values })
}

function settingsTitle(main: HTMLElement): HTMLElement {
  return within(main).getByRole('heading', {
    level: 2,
    name: en('model-profile-settings-title'),
  })
}

/** The bordered Notice paragraphs of the page (D5.2: `p.notice`), in DOM
 * order. 2h.10's 194 took the read-only one away, so only the engine Notice
 * of 72 is left. */
function notices(main: HTMLElement): Element[] {
  return [...main.querySelectorAll('.notice')]
}

/** The section jump list of 2h.10's 196, found by its name. */
function navOf(main: HTMLElement): HTMLElement {
  return within(main).getByRole('navigation', {
    name: en('model-profile-jump-label'),
  })
}

/** The Card whose h3 is `titleKey` (D5.2 Card: a `section.card`). */
function cardOf(main: HTMLElement, titleKey: string): HTMLElement {
  const h3 = within(main).getByRole('heading', {
    level: 3,
    name: en(titleKey),
  })
  const card = h3.closest('section')
  expect(card, `${en(titleKey)}: its Card section`).toBeTruthy()
  expect(String(card!.className), `${en(titleKey)}: the Card class`).toContain(
    'card',
  )
  return card as HTMLElement
}

/** The About toggle of the field named `label` (criterion 88). */
function aboutOf(main: HTMLElement, label: string): HTMLButtonElement {
  return within(main).getByRole('button', {
    name: `${en('field-about')} ${label}`,
  }) as HTMLButtonElement
}

/** The field that holds the About toggle of `label` (Field.tsx `div.field`). */
function fieldOf(main: HTMLElement, label: string): HTMLElement {
  const field = aboutOf(main, label).closest('.field')
  expect(field, `${label}: its Field wrapper`).toBeTruthy()
  return field as HTMLElement
}

/** A number field: the group named by the label, its box and its input (87). */
function groupBox(main: HTMLElement, label: string): HTMLElement {
  return within(main).getByRole('group', { name: label })
}

function boxOf(group: HTMLElement): HTMLInputElement {
  return within(group).getByRole('checkbox', {
    name: en('field-default'),
  }) as HTMLInputElement
}

function inputOf(group: HTMLElement, label: string): HTMLInputElement {
  return within(group).getByRole('textbox', { name: label }) as HTMLInputElement
}

function selectOf(main: HTMLElement, label: string): HTMLSelectElement {
  return within(main).getByRole('combobox', {
    name: label,
  }) as HTMLSelectElement
}

function optionsOf(select: HTMLSelectElement): HTMLOptionElement[] {
  return [...select.querySelectorAll('option')]
}

function optionTexts(select: HTMLSelectElement): string[] {
  return optionsOf(select).map((o) => (o.textContent ?? '').trim())
}

/** The text of the selected option, the only thing the user reads. */
function selectedText(select: HTMLSelectElement): string {
  expect(select.selectedIndex, 'an option is selected').toBeGreaterThanOrEqual(
    0,
  )
  return (optionsOf(select)[select.selectedIndex]!.textContent ?? '').trim()
}

function text(el: Element): string {
  return (el.textContent ?? '').replace(/\s+/g, ' ').trim()
}

/** The elements Tab reaches inside `main`, in DOM order (criterion 91). */
function mainTabbable(main: HTMLElement): string[] {
  return [
    ...main.querySelectorAll<HTMLElement>(
      'a[href], button, input, select, textarea, [tabindex]',
    ),
  ]
    .filter(
      (el) =>
        !(el as HTMLButtonElement).disabled &&
        el.getAttribute('tabindex') !== '-1' &&
        !isHidden(el),
    )
    .map((el) => nameOf(el))
}

/** The name Tab shows: a control's text, or the label that names an input. */
function nameOf(el: HTMLElement): string {
  if (!['INPUT', 'SELECT', 'TEXTAREA'].includes(el.tagName))
    return (el.textContent ?? '').replace(/\s+/g, ' ').trim()
  const labelled = el.getAttribute('aria-labelledby')
  if (labelled)
    return labelled
      .split(/\s+/)
      .map((part) => document.getElementById(part)?.textContent ?? '')
      .join(' ')
      .replace(/\s+/g, ' ')
      .trim()
  const id = el.getAttribute('id')
  const label = id ? document.querySelector(`label[for="${id}"]`) : null
  const wrap = el.closest('label')
  return (label?.textContent ?? wrap?.textContent ?? '')
    .replace(/\s+/g, ' ')
    .trim()
}

/** What a number field shows when its key is absent: the box ticked, empty. */
function expectAbsentNumber(main: HTMLElement, label: string): void {
  const group = groupBox(main, label)
  expect(boxOf(group).checked, `${label}: the default box ticked`).toBe(true)
  expect(inputOf(group, label).value, `${label}: an empty input`).toBe('')
}

/** What a select shows when its key is absent: the table's absent option. */
function expectAbsentSelect(main: HTMLElement, f: Row): void {
  expect(
    selectedText(selectOf(main, en(f.label))),
    `${f.key}: absent shows ${f.absentOption}`,
  ).toBe(f.absentOption)
}

// ── The page structure (83, 84) ─────────────────────────────────────────

test('2e.83: the section of the h2 holds, in order, the h2, the jump list of 2h.10, then every card of 83 in the page order (the list of test/modelProfileCards.ts), and the off line stays above the h2', async () => {
  for (const [what, extra, off] of [
    ['a switched-on model profile', {}, false],
    ['a switched-off model profile', { enabled: false }, true],
  ] as [string, Record<string, unknown>, boolean][]) {
    const { main, h1 } = await loaded({
      modelProfile: () => reply(200, docOf(SET, extra)),
    })
    const h2 = settingsTitle(main)
    const section = h2.closest('section')
    expect(section, `${what}: the h2 sits in a section`).toBeTruthy()
    const nodes: [string, Node][] = [['the h1', h1]]
    if (off)
      nodes.push([
        'the off line',
        within(main).getByText(en('model-profile-off')),
      ])
    else
      expect(
        within(main).queryByText(en('model-profile-off')),
        `${what}: no off line`,
      ).toBeNull()
    nodes.push(['the h2', h2], ['the jump list', navOf(main)])
    for (const title of cardHeadingKeys()) {
      const card = cardOf(main, title)
      expect(
        section!.contains(card),
        `${what}: the ${en(title)} card is inside the section`,
      ).toBe(true)
      nodes.push([`the ${en(title)} card`, card])
    }
    for (let i = 0; i + 1 < nodes.length; i++)
      expect(
        before(nodes[i]![1], nodes[i + 1]![1]),
        `${what}: ${nodes[i]![0]} before ${nodes[i + 1]![0]}`,
      ).toBe(true)
    expect(
      notices(main).length,
      `${what}: 2h.10's 194 took the read-only Notice away`,
    ).toBe(0)
    cleanup()
    vi.unstubAllGlobals()
  }
  // The engine Notice of 72 keeps its place: 2h.10's 196 puts the jump list
  // between the h2 and the engine Notice, and H11b.2 keeps the Notice above
  // the Assets card, the row's card.
  const { main } = await loaded({
    modelProfile: () =>
      reply(200, docOf(SET, { engine: STRATA, engineAvailable: false })),
  })
  const engine = notices(main)
  expect(engine.length, '194: the engine Notice is the only Notice').toBe(1)
  expect(text(engine[0]!), 'the engine Notice of 72').toBe(
    en('model-profile-engine-unavailable', STRATA),
  )
  const assets = within(main).getByRole('heading', {
    level: 3,
    name: en('model-profile-assets'),
  })
  expect(
    before(engine[0]!, assets),
    'H11b.2: the engine Notice is above the Assets card',
  ).toBe(true)
  expect(
    before(navOf(main), engine[0]!),
    '196: the jump list sits between the h2 and the engine Notice',
  ).toBe(true)
})

test('2e.84 (2h.10 194, 198): the Notice model-profile-partly-editable is gone: no bordered paragraph and no word of its text on any loaded model profile, and the key is out of the strings file', async () => {
  for (const [what, doc] of [
    ['the all-set stub', docOf(SET)],
    ['a values of {}', exactOf({})],
    ['a switched-off model profile', docOf(SET, { enabled: false })],
  ] as [string, unknown][]) {
    const { main } = await loaded({ modelProfile: () => reply(200, doc) })
    expect(
      notices(main).length,
      `${what}: 194, no bordered Notice paragraph`,
    ).toBe(0)
    expect(
      within(main).queryByText(NOTICE),
      `${what}: 198, no word of the removed string`,
    ).toBeNull()
    expect(
      t('model-profile-partly-editable'),
      `${what}: 198, the key shows as itself`,
    ).toBe('model-profile-partly-editable')
    cleanup()
    vi.unstubAllGlobals()
  }
})

// ── The shared rules of the cards (85, 86, 89, 90) ──────────────────────

test('2e.85 (2h.1 130, 2h.4 155, 156): every control of the two cards is natively disabled unless the card is editable, the default boxes included, while every About toggle stays enabled', async () => {
  const { main } = await loaded({ modelProfile: () => reply(200, docOf(SET)) })
  const values = docOf(SET).values
  const controls = RESOURCE_CARDS.flatMap((title) => [
    ...cardOf(main, title).querySelectorAll<HTMLElement>(
      'input, select, textarea',
    ),
  ])
  expect(controls.length, 'the controls of 92 and 93').toBe(CONTROLS)
  // 2h.1's 130 and 2h.4's 155: the two cards are editable, so their controls
  // lose disabled. A number's default box is always enabled (155); its input
  // follows the box, which a key the file holds leaves unticked (156).
  const rowOf = (id: string): Row | undefined =>
    FIELDS.find(
      (f) => `model-profile-${f.key}` === id.replace(/-(box|control)$/, ''),
    )
  for (const el of controls) {
    const id = el.getAttribute('id') ?? el.tagName
    const f = rowOf(id)
    expect(f, `${id}: the control of a field of 92 or 93`).toBeTruthy()
    const enabled = id.endsWith('-box')
      ? isEditableField(f!.label)
      : controlEnabled(f!.label, values)
    expect((el as HTMLInputElement).disabled, `${id}: enabled`).toBe(!enabled)
    expect(
      el.getAttribute('aria-disabled'),
      `${id}: never aria-disabled`,
    ).toBeNull()
    // `readonly` is a text-input attribute (section 0), so the attribute is
    // the check: a select has no readOnly property.
    expect(el.getAttribute('readonly'), `${id}: never readonly`).toBeNull()
  }
  for (const f of FIELDS)
    expect(
      aboutOf(main, en(f.label)).disabled,
      `${f.key}: its About toggle stays enabled`,
    ).toBe(false)
})

test('2e.86, 2e.81: a key the page does not show adds no field, so the two cards hold exactly the fields of 92 and 93 in the table order', async () => {
  const { main } = await loaded({
    modelProfile: () =>
      reply(200, docOf({ ...SET, temp: '0.7', faqs: 'zz-secret' })),
  })
  for (const title of RESOURCE_CARDS) {
    const card = cardOf(main, title)
    expect(
      card.querySelectorAll('.field').length,
      `${en(title)}: one Field per row of its table`,
    ).toBe(FIELDS.filter((f) => f.card === title).length)
    for (const secret of ['temp', '0.7', 'zz-secret', 'faqs'])
      expect(
        card.textContent,
        `${en(title)}: ${secret} is not shown`,
      ).not.toContain(secret)
  }
  // The fields of a card follow its table.
  const fields = [
    ...cardOf(main, 'model-profile-resource-context').querySelectorAll(
      '.field',
    ),
  ]
  const inOrder = FIELDS.filter(
    (f) => f.card === 'model-profile-resource-context',
  )
  for (let i = 0; i < inOrder.length; i++)
    expect(
      fields[i]!.contains(aboutOf(main, en(inOrder[i]!.label))),
      `${inOrder[i]!.key} is the ${i + 1}th field of its card`,
    ).toBe(true)
})

test.each(
  FIELDS.filter((f) => f.key !== 'kv-unified' && f.key !== 'embeddings'),
)(
  '2e.86: $key is the empty string so it reads as absent, and its field shows the absent form (2e.3b: a flag reads "" as on, criterion 123)',
  async (f) => {
    const { main } = await loaded({
      modelProfile: () => reply(200, exactOf({ ...SET, [f.key]: '' })),
    })
    if (f.kind === 'number') expectAbsentNumber(main, en(f.label))
    else expectAbsentSelect(main, f)
  },
)

test('2e.89, 2e.90: no 2e.1 field carries a unit and no card of 2e.1 has a note or a group, so no unit, no h4 and no role=group division in them', async () => {
  const { main } = await loaded({ modelProfile: () => reply(200, docOf(SET)) })
  for (const title of RESOURCE_CARDS) {
    const card = cardOf(main, title)
    for (const sel of ['.unit', 'h4', '[role="group"]'])
      expect(card.querySelectorAll(sel).length, `${en(title)}: no ${sel}`).toBe(
        0,
      )
  }
  // The label is the strings text of 95 as it is, with no unit in it.
  for (const f of FIELDS) {
    const label = en(f.label)
    expect(fieldOf(main, label).textContent, `${f.key}: the label`).toContain(
      label,
    )
    expect(
      fieldOf(main, label).textContent,
      `${f.key}: no unit word in the label`,
    ).not.toMatch(/\((tokens|MiB|ms)\)/)
  }
})

// ── The field table (87, 92, 93, 94) ────────────────────────────────────

test.each(FIELDS)(
  '2e.92, 2e.93, 2e.87, 2e.85 (2h.1 130, 2h.4 155): $key is a $kind named by its label inside its card and shows the set value $set, its controls enabled in an editable card',
  async (f) => {
    const { main } = await loaded({
      modelProfile: () => reply(200, docOf(SET)),
    })
    const label = en(f.label)
    const field = fieldOf(main, label)
    expect(
      cardOf(main, f.card).contains(field),
      `${f.key}: its field is inside the ${en(f.card)} card`,
    ).toBe(true)
    // 89: a unit sits after the input; no 2e.1 field has one.
    expect(field.querySelectorAll('.unit').length, `${f.key}: no unit`).toBe(0)
    if (f.kind === 'number') {
      const group = groupBox(main, label)
      const box = boxOf(group)
      const input = inputOf(group, label)
      expect(input.value, `${f.key}: the string of the file exactly`).toBe(
        f.shown,
      )
      expect(box.checked, `${f.key}: the default box unticked`).toBe(false)
      // 2h.4's 155: an editable card's box is enabled, and 156 leaves its
      // unticked input enabled; a read-only card keeps both disabled.
      const editable = isEditableField(f.label)
      expect(box.disabled, `${f.key}: the box disabled`).toBe(!editable)
      expect(input.disabled, `${f.key}: the input disabled`).toBe(!editable)
      // 87: an integer key takes the numeric form of the input.
      expect(input.getAttribute('inputmode'), `${f.key}: numeric`).toBe(
        'numeric',
      )
    } else {
      const select = selectOf(main, label)
      expect(optionTexts(select), `${f.key}: the options in order`).toEqual(
        f.options,
      )
      if (f.optionValues !== undefined)
        expect(
          optionsOf(select).map((o) => o.value),
          `${f.key}: the options carry the file's values`,
        ).toEqual(f.optionValues)
      expect(selectedText(select), `${f.key}: the mapping selects`).toBe(
        f.shown,
      )
      expect(
        select.disabled,
        `${f.key}: ${isEditableField(f.label) ? 'editable (2h.1 130)' : 'disabled'}`,
      ).toBe(!isEditableField(f.label))
    }
    expect(aboutOf(main, label).disabled, `${f.key}: About stays enabled`).toBe(
      false,
    )
  },
)

test.each(FIELDS)(
  '2e.86, 2e.94: $key is absent from a values of {} so its field shows the absent form',
  async (f) => {
    const { main } = await loaded({
      modelProfile: () => reply(200, exactOf({})),
    })
    if (f.kind === 'number') expectAbsentNumber(main, en(f.label))
    else expectAbsentSelect(main, f)
  },
)

test.each(['0172032', 'abc', '-1', '4096.50', ' 4096 '])(
  '2e.87, 2e.94: a number shows the reply string exactly, with no parsing and no reformatting, and its default box unticked: %s',
  async (value) => {
    const label = en('model-profile-context-size')
    const { main } = await loaded({
      modelProfile: () => reply(200, exactOf({ 'ctx-size': value })),
    })
    const group = groupBox(main, label)
    expect(inputOf(group, label).value, `ctx-size ${value}`).toBe(value)
    expect(boxOf(group).checked, `ctx-size ${value}: the box unticked`).toBe(
      false,
    )
  },
)

test('2e.87, 2e.94: a value the mapping does not list becomes one extra last option carrying the reply string as its value and its text, selected: pooling weird (2e.3b: a flag select has no extra option, criterion 125, and kv-unified auto shows on with the line of 126)', async () => {
  for (const [f, value] of [
    [FIELDS.find((x) => x.key === 'pooling')!, 'weird'],
  ] as [Row, string][]) {
    const { main } = await loaded({
      modelProfile: () => reply(200, exactOf({ ...SET, [f.key]: value })),
    })
    const select = selectOf(main, en(f.label))
    const opts = optionsOf(select)
    const listed = f.options!
    expect(opts.length, `${f.key}: the listed options plus one`).toBe(
      listed.length + 1,
    )
    expect(
      optionTexts(select).slice(0, listed.length),
      `${f.key}: the listed options keep their place`,
    ).toEqual(listed)
    const extra = opts[opts.length - 1]!
    expect(extra.value, `${f.key}: the extra option carries the value`).toBe(
      value,
    )
    expect((extra.textContent ?? '').trim(), `${f.key}: and its text`).toBe(
      value,
    )
    expect(selectedText(select), `${f.key}: the extra option is selected`).toBe(
      value,
    )
    expect(select.disabled, `${f.key}: editable (2h.1 130)`).toBe(
      !isEditableField(f.label),
    )
    cleanup()
    vi.unstubAllGlobals()
  }
})

test.each([
  ['cls', 'cls'],
  ['rank', en('option-rank')],
  ['MEAN', 'MEAN'],
  ['Last', 'Last'],
] as [string, string][])(
  '2e.93, 2e.87: Pooling selects the option whose value equals the reply string exactly, and a value that differs only in case is the extra option: %s',
  async (value, shown) => {
    const pooling = FIELDS.find((f) => f.key === 'pooling')!
    const label = en(pooling.label)
    const { main } = await loaded({
      modelProfile: () => reply(200, exactOf({ ...SET, pooling: value })),
    })
    expect(selectedText(selectOf(main, label)), `pooling ${value}`).toBe(shown)
  },
)

test('2e.93, 2e.1.3: Embedding mode is off / on and Unified KV is the tri-state: false shows off, and the shared option texts are the ones of 1.3', async () => {
  for (const f of [
    FIELDS.find((x) => x.key === 'embeddings')!,
    FIELDS.find((x) => x.key === 'kv-unified')!,
  ]) {
    const { main } = await loaded({
      modelProfile: () => reply(200, exactOf({ ...SET, [f.key]: 'false' })),
    })
    const select = selectOf(main, en(f.label))
    expect(optionTexts(select), `${f.key}: the shared options of 1.3`).toEqual(
      f.options,
    )
    expect(selectedText(select), `${f.key}: false shows off`).toBe(
      en('option-off'),
    )
    cleanup()
    vi.unstubAllGlobals()
  }
})

// ── The hints (88) ──────────────────────────────────────────────────────

test.each(FIELDS)(
  '2e.88: $key has an About toggle named About its label, with aria-expanded and aria-controls, and its hint opens as one paragraph per line of the GUI text and closes again',
  async (f) => {
    const { main } = await loaded({
      modelProfile: () => reply(200, docOf(SET)),
    })
    const label = en(f.label)
    const hintKey = `${f.label}-hint`
    const lines = en(hintKey).split('\n')
    expect(lines.length, `${hintKey}: the hint has lines`).toBeGreaterThan(0)
    const about = aboutOf(main, label)
    expect(about.tagName, `${f.key}: a button`).toBe('BUTTON')
    expect(about.disabled, `${f.key}: the toggle is not disabled`).toBe(false)
    const hintId = about.getAttribute('aria-controls')
    expect(hintId, `${f.key}: aria-controls names the hint`).toBeTruthy()
    const hint = document.getElementById(hintId!)
    expect(hint, `${f.key}: the hinted element is in the page`).toBeTruthy()
    expect(about.getAttribute('aria-expanded'), `${f.key}: closed`).toBe(
      'false',
    )
    expect(isHidden(hint), `${f.key}: the hint is hidden`).toBe(true)
    fireEvent.click(about)
    expect(about.getAttribute('aria-expanded'), `${f.key}: open`).toBe('true')
    expect(isHidden(hint), `${f.key}: the hint shows`).toBe(false)
    const paragraphs = [...hint!.querySelectorAll('p')]
    expect(paragraphs.length, `${f.key}: one paragraph per line`).toBe(
      lines.length,
    )
    expect(
      paragraphs.map((p) => text(p)),
      `${hintKey}: the text, unchanged`,
    ).toEqual(lines.map((line) => line.trim()))
    fireEvent.click(about)
    expect(about.getAttribute('aria-expanded'), `${f.key}: closed again`).toBe(
      'false',
    )
    expect(isHidden(hint), `${f.key}: the hint is hidden again`).toBe(true)
  },
)

// ── Keyboard (91) ───────────────────────────────────────────────────────

test('2e.91: Tab in main reaches the back link, the ten jump buttons of 2h.10 in card order, the Assets toggle, then every field About toggle in card and field order, and no control of a 2e card', async () => {
  const { main } = await loaded({ modelProfile: () => reply(200, docOf(SET)) })
  expect(mainTabbable(main)).toEqual([
    ...pageStartTabStops(),
    ...fieldTabStops(docOf(SET).values),
    ...saveBarTabStops(),
  ])
})

// ── The strings (95) ────────────────────────────────────────────────────

test('2e.95: every card of 83 in the page order (the list of test/modelProfileCards.ts), the Assets intro of 2h.10, every label and every option show the English of the 22 new strings of 95', async () => {
  const { main } = await loaded({ modelProfile: () => reply(200, docOf(SET)) })
  expect(
    within(main)
      .getAllByRole('heading', { level: 3 })
      .map((h) => text(h)),
    'the cards, in the order of 83',
  ).toEqual(cardHeadings())
  // 2h.10's 194: the read-only Notice is off the page; its place among the
  // strings of 95 is taken by the Assets intro of 195.
  expect(notices(main).length, '194: no Notice on the page').toBe(0)
  expect(
    within(main).queryByText(NOTICE),
    '198: no word of the removed string',
  ).toBeNull()
  expect(
    within(cardOf(main, 'model-profile-assets')).getByText(
      en('model-profile-assets-read-only'),
    ),
    '195: the Assets intro shows',
  ).toBeTruthy()
  // Every label is the strings text of 95, and names its control.
  for (const f of FIELDS) {
    const label = en(f.label)
    const control =
      f.kind === 'number'
        ? inputOf(groupBox(main, label), label)
        : selectOf(main, label)
    expect(
      text(fieldOf(main, label)).includes(label),
      `${f.key}: the label is shown`,
    ).toBe(true)
    expect(
      control.getAttribute('id'),
      `${f.key}: the control is named`,
    ).toBeTruthy()
  }
  // The option texts of 1.3 and of 93, as the brief gives them.
  const pooling = selectOf(main, en('model-profile-pooling'))
  expect(optionTexts(pooling)).toEqual(POOLING)
  const mode = selectOf(main, en('model-profile-embedding-mode'))
  expect(optionTexts(mode)).toEqual(OFF_ON)
})
