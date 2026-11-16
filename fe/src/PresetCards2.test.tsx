// The three cards of 2e.2 on the preset page (fe/briefs/02e-preset-cards.md,
// criteria 96 to 100): KV cache, Sampling overrides and Advanced, tables of
// fields only. The shared rules of section 1 are pinned for the first two
// cards in PresetCards1.test.tsx (83 to 95); here they are pinned for what
// 2e.2 adds: the cache-type options, the unit MiB, the float inputs, the text
// field, the cards' place. The route and the read are PresetPage.test.tsx, the
// Engine field and the Assets card are PresetContent.test.tsx. The colours of
// 85, the width of 91 and the words a screen reader uses are checked in the
// designer's review, not in jsdom.
import {
  cleanup,
  fireEvent,
  render,
  screen,
  within,
} from '@testing-library/react'
import { afterEach, beforeEach, expect, test, vi } from 'vitest'
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
import {
  cardHeadings,
  cardHeadingKeys,
  fieldAboutToggles,
} from './test/presetCards.ts'

const ID = 'Qwen3-27B'
const HASH = `#/models/${ID}`
const MODEL = '/var/lib/private-ai/.llama.cpp/models/Qwen3-27B-Q8_0.gguf'

/** The criterion 99 stub: every key of 96, 97 and 98 set. */
const SET: Record<string, string> = {
  'cache-type-k': 'q8_0',
  'cache-type-v': 'q8_0',
  'flash-attn': 'true',
  'cache-ram': '-1',
  'cache-reuse': '256',
  'ctx-checkpoints': '0',
  'checkpoint-min-step': '8192',
  temp: '0.6',
  'top-k': '20',
  'top-p': '0.95',
  'min-p': '0',
  'repeat-penalty': '1.05',
  'presence-penalty': '1,5',
  'backend-sampling': 'false',
  'load-on-startup': 'true',
  'chat-template-kwargs': '{"enable_thinking":false}',
}

/** The shared option texts of section 1.3. */
const TRISTATE = [en('option-default'), en('option-on'), en('option-off')]
const OFF_ON = [en('option-off'), en('option-on')]

/** The cache types of criterion 96 (MP 95-96): `default` is a string, the ten
 * types are literals. */
const CACHE_TYPES = [
  en('option-default'),
  'f32',
  'f16',
  'bf16',
  'f8_e4m3',
  'q8_0',
  'q5_1',
  'q5_0',
  'q4_1',
  'q4_0',
  'iq4_nl',
]
const CACHE_TYPE_VALUES = CACHE_TYPES.map((text, i) =>
  i === 0 ? 'default' : text,
)

/** One row of the field tables of 96, 97 and 98. */
type Row = {
  /** The strings key of the card's h3. */
  card: string
  /** The key of the section's `values`. */
  key: string
  /** The label as the page shows it: the strings text of 100, or a literal
   * (Top-K, Top-P, Min-P, as MP 987 labels them). */
  label: string
  /** The strings key of the field's hint (criterion 88). */
  hint: string
  kind: 'number' | 'select' | 'text'
  /** A number: an integer key takes the numeric input form (87). */
  integer?: boolean
  /** A number's unit, the literal after the input (89). */
  unit?: string
  /** A select's option texts, in the table's order. */
  options?: string[]
  /** A select's option values, where criterion 96 gives them. */
  optionValues?: string[]
  /** The option text a select shows when the key is absent. */
  absentOption?: string
  /** The value of the criterion 99 stub, and what the field shows for it: the
   * input's value, or the selected option's text. */
  set: string
  shown: string
}

/** The field table of 96, 97 and 98, in the cards' and the GUI's order. */
const FIELDS: Row[] = [
  {
    card: 'preset-kv-cache',
    key: 'cache-type-k',
    label: en('preset-k-cache-type'),
    hint: 'preset-k-cache-type-hint',
    kind: 'select',
    options: CACHE_TYPES,
    optionValues: CACHE_TYPE_VALUES,
    absentOption: en('option-default'),
    set: 'q8_0',
    shown: 'q8_0',
  },
  {
    card: 'preset-kv-cache',
    key: 'cache-type-v',
    label: en('preset-v-cache-type'),
    hint: 'preset-v-cache-type-hint',
    kind: 'select',
    options: CACHE_TYPES,
    optionValues: CACHE_TYPE_VALUES,
    absentOption: en('option-default'),
    set: 'q8_0',
    shown: 'q8_0',
  },
  {
    card: 'preset-kv-cache',
    key: 'flash-attn',
    label: en('preset-flash-attention'),
    hint: 'preset-flash-attention-hint',
    kind: 'select',
    options: TRISTATE,
    absentOption: en('option-default'),
    set: 'true',
    shown: en('option-on'),
  },
  {
    card: 'preset-kv-cache',
    key: 'cache-ram',
    label: en('preset-cache-ram'),
    hint: 'preset-cache-ram-hint',
    kind: 'number',
    integer: true,
    unit: 'MiB',
    set: '-1',
    shown: '-1',
  },
  {
    card: 'preset-kv-cache',
    key: 'cache-reuse',
    label: en('preset-cache-reuse'),
    hint: 'preset-cache-reuse-hint',
    kind: 'number',
    integer: true,
    set: '256',
    shown: '256',
  },
  {
    card: 'preset-kv-cache',
    key: 'ctx-checkpoints',
    label: en('preset-context-checkpoints'),
    hint: 'preset-context-checkpoints-hint',
    kind: 'number',
    integer: true,
    set: '0',
    shown: '0',
  },
  {
    card: 'preset-kv-cache',
    key: 'checkpoint-min-step',
    label: en('preset-checkpoint-step'),
    hint: 'preset-checkpoint-step-hint',
    kind: 'number',
    integer: true,
    set: '8192',
    shown: '8192',
  },
  {
    card: 'preset-sampling-overrides',
    key: 'temp',
    label: en('preset-temperature'),
    hint: 'preset-temperature-hint',
    kind: 'number',
    integer: false,
    set: '0.6',
    shown: '0.6',
  },
  {
    card: 'preset-sampling-overrides',
    key: 'top-k',
    label: 'Top-K',
    hint: 'preset-top-k-hint',
    kind: 'number',
    integer: true,
    set: '20',
    shown: '20',
  },
  {
    card: 'preset-sampling-overrides',
    key: 'top-p',
    label: 'Top-P',
    hint: 'preset-top-p-hint',
    kind: 'number',
    integer: false,
    set: '0.95',
    shown: '0.95',
  },
  {
    card: 'preset-sampling-overrides',
    key: 'min-p',
    label: 'Min-P',
    hint: 'preset-min-p-hint',
    kind: 'number',
    integer: false,
    set: '0',
    shown: '0',
  },
  {
    card: 'preset-sampling-overrides',
    key: 'repeat-penalty',
    label: en('preset-repeat-penalty'),
    hint: 'preset-repeat-penalty-hint',
    kind: 'number',
    integer: false,
    set: '1.05',
    shown: '1.05',
  },
  {
    card: 'preset-sampling-overrides',
    key: 'presence-penalty',
    label: en('preset-presence-penalty'),
    hint: 'preset-presence-penalty-hint',
    kind: 'number',
    integer: false,
    set: '1,5',
    shown: '1,5',
  },
  {
    card: 'preset-sampling-overrides',
    key: 'backend-sampling',
    label: en('preset-backend-sampling'),
    hint: 'preset-backend-sampling-hint',
    kind: 'select',
    options: TRISTATE,
    absentOption: en('option-default'),
    set: 'false',
    shown: en('option-off'),
  },
  {
    card: 'preset-advanced',
    key: 'load-on-startup',
    label: en('preset-load-on-startup'),
    hint: 'preset-load-on-startup-hint',
    kind: 'select',
    options: OFF_ON,
    absentOption: en('option-off'),
    set: 'true',
    shown: en('option-on'),
  },
  {
    card: 'preset-advanced',
    key: 'chat-template-kwargs',
    label: en('preset-chat-kwargs'),
    hint: 'preset-chat-kwargs-hint',
    kind: 'text',
    set: '{"enable_thinking":false}',
    shown: '{"enable_thinking":false}',
  },
]

/** The cards of 2e.2, in the order of criterion 83. */
const CARDS = [
  'preset-kv-cache',
  'preset-sampling-overrides',
  'preset-advanced',
]

/** The controls 96, 97 and 98 put in the three cards: a box and an input per
 * number, one control per select and per text. */
const CONTROLS = FIELDS.reduce((n, f) => n + (f.kind === 'number' ? 2 : 1), 0)

beforeEach(() => startAt(HASH))

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
  startAt('')
})

// ── Helpers (as PresetCards1.test.tsx) ──────────────────────────────────

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
    name: en('preset-assets'),
  })
  return { bff, main, h1, h3 }
}

/** A read with `values` added to the Assets keys, `extra` on top. */
function docOf(
  values: Record<string, string>,
  extra: Record<string, unknown> = {},
) {
  return presetDoc({ model: MODEL, ...values }, extra)
}

/** A read whose `values` are exactly `values` (the `{}` stub of 99). */
function exactOf(values: Record<string, string>) {
  return presetDoc({}, { values })
}

function settingsTitle(main: HTMLElement): HTMLElement {
  return within(main).getByRole('heading', {
    level: 2,
    name: en('preset-settings-title'),
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

/** A text field's input, named by the label (87). */
function inputOfField(main: HTMLElement, label: string): HTMLInputElement {
  return within(main).getByRole('textbox', { name: label }) as HTMLInputElement
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

/** The control of a row's field, by its kind. */
function controlOf(main: HTMLElement, f: Row): HTMLElement {
  if (f.kind === 'number')
    return inputOf(groupBox(main, f.label), f.label) as HTMLElement
  if (f.kind === 'select') return selectOf(main, f.label)
  return inputOfField(main, f.label)
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
    selectedText(selectOf(main, f.label)),
    `${f.key}: absent shows ${f.absentOption}`,
  ).toBe(f.absentOption)
}

/** What a text field shows when its key is absent: an empty input with no
 * placeholder (87). */
function expectAbsentText(main: HTMLElement, f: Row): void {
  const input = inputOfField(main, f.label)
  expect(input.value, `${f.key}: an empty input`).toBe('')
  expect(
    input.getAttribute('placeholder') ?? '',
    `${f.key}: no placeholder in 2e`,
  ).toBe('')
}

/** The absent form of a row, whatever its kind. */
function expectAbsent(main: HTMLElement, f: Row): void {
  if (f.kind === 'number') expectAbsentNumber(main, f.label)
  else if (f.kind === 'select') expectAbsentSelect(main, f)
  else expectAbsentText(main, f)
}

// ── One test per field (85, 86, 87, 88, 96, 97, 98, 99) ────────────────

test.each(FIELDS)(
  '2e.96, 2e.97, 2e.98, 2e.87, 2e.85: $key is a $kind named by $label inside its card, shows the set value $set exactly as the reply holds it, and every control is natively disabled',
  async (f) => {
    const { main } = await loaded({ preset: () => reply(200, docOf(SET)) })
    const field = fieldOf(main, f.label)
    expect(
      cardOf(main, f.card).contains(field),
      `${f.key}: its field is inside the ${en(f.card)} card`,
    ).toBe(true)
    // 89: a unit sits after the input, only where the table gives one.
    const units = [...field.querySelectorAll('.unit')].map((u) => text(u))
    expect(units, `${f.key}: the unit of ${f.unit ?? 'none'}`).toEqual(
      f.unit === undefined ? [] : [f.unit],
    )
    const control = controlOf(main, f)
    if (f.kind === 'number') {
      const group = groupBox(main, f.label)
      const box = boxOf(group)
      const input = inputOf(group, f.label)
      expect(input.value, `${f.key}: the string of the file exactly`).toBe(
        f.shown,
      )
      expect(box.checked, `${f.key}: the default box unticked`).toBe(false)
      expect(box.disabled, `${f.key}: the box disabled`).toBe(true)
      expect(input.disabled, `${f.key}: the input disabled`).toBe(true)
      // 87: an integer key takes the numeric form, a float key a plain input.
      expect(
        input.getAttribute('inputmode'),
        `${f.key}: ${f.integer ? 'numeric' : 'plain'}`,
      ).toBe(f.integer ? 'numeric' : null)
    } else if (f.kind === 'select') {
      const select = selectOf(main, f.label)
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
      expect(select.disabled, `${f.key}: the select disabled`).toBe(true)
    } else {
      const input = inputOfField(main, f.label)
      expect(input.value, `${f.key}: the string of the file exactly`).toBe(
        f.shown,
      )
      expect(input.disabled, `${f.key}: the input disabled`).toBe(true)
      expect(
        input.getAttribute('placeholder') ?? '',
        `${f.key}: no placeholder in 2e`,
      ).toBe('')
    }
    // 85: the native disabled attribute, never aria-disabled, never readonly.
    for (const el of [
      control,
      ...(f.kind === 'number'
        ? [groupBox(main, f.label).querySelector('input') as Element]
        : []),
    ]) {
      expect(
        (el as HTMLInputElement).disabled,
        `${f.key}: ${el.tagName} disabled`,
      ).toBe(true)
      expect(
        el.getAttribute('aria-disabled'),
        `${f.key}: never aria-disabled`,
      ).toBeNull()
      expect(el.getAttribute('readonly'), `${f.key}: never readonly`).toBeNull()
    }
    expect(
      aboutOf(main, f.label).disabled,
      `${f.key}: About stays enabled`,
    ).toBe(false)
  },
)

test.each(FIELDS)(
  '2e.86, 2e.99: $key is absent from a values of {} so its field shows the absent form of $kind',
  async (f) => {
    const { main } = await loaded({ preset: () => reply(200, exactOf({})) })
    expectAbsent(main, f)
  },
)

test.each(FIELDS)(
  '2e.88: $key has an About toggle named About its label, with aria-expanded and aria-controls, and its hint opens as one paragraph per line of the GUI text and closes again',
  async (f) => {
    const { main } = await loaded({ preset: () => reply(200, docOf(SET)) })
    const lines = en(f.hint).split('\n')
    expect(lines.length, `${f.hint}: the hint has lines`).toBeGreaterThan(0)
    const about = aboutOf(main, f.label)
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
      `${f.hint}: the text, unchanged`,
    ).toEqual(lines.map((line) => line.trim()))
    fireEvent.click(about)
    expect(about.getAttribute('aria-expanded'), `${f.key}: closed again`).toBe(
      'false',
    )
    expect(isHidden(hint), `${f.key}: the hint is hidden again`).toBe(true)
  },
)

// ── Absent keys and the fields of the cards (81, 86) ────────────────────

test('2e.86, 2e.99: a key of 96, 97 or 98 whose value is the empty string reads as absent, for every key of 2e.2 that is not a flag (2e.3b: a flag reads "" as on, criterion 123)', async () => {
  for (const f of FIELDS) {
    if (f.key === 'backend-sampling' || f.key === 'load-on-startup') continue
    const { main } = await loaded({
      preset: () => reply(200, exactOf({ ...SET, [f.key]: '' })),
    })
    expectAbsent(main, f)
    cleanup()
    vi.unstubAllGlobals()
  }
})

test('2e.86, 2e.81: a key the page does not show adds no field, so the three cards of 2e.2 hold exactly the fields of 96, 97 and 98 in the table order', async () => {
  const { main } = await loaded({
    preset: () =>
      reply(
        200,
        docOf({ ...SET, 'n-cpu-moe': 'zz-secret', faqs: 'zz-secret' }),
      ),
  })
  for (const title of CARDS) {
    const card = cardOf(main, title)
    const rows = FIELDS.filter((f) => f.card === title)
    expect(
      card.querySelectorAll('.field').length,
      `${en(title)}: one Field per row of its table`,
    ).toBe(rows.length)
    for (const secret of ['n-cpu-moe', 'zz-secret', 'faqs'])
      expect(
        card.textContent,
        `${en(title)}: ${secret} is not shown`,
      ).not.toContain(secret)
    const fields = [...card.querySelectorAll('.field')]
    for (let i = 0; i < rows.length; i++)
      expect(
        fields[i]!.contains(aboutOf(main, rows[i]!.label)),
        `${rows[i]!.key} is the ${i + 1}th field of ${en(title)}`,
      ).toBe(true)
  }
})

// ── The cache types (96) ────────────────────────────────────────────────

test('2e.96: both cache-type selects list default then the ten literals of MP 95-96, in order and with the file values, and each listed value selects its own option', async () => {
  for (const key of ['cache-type-k', 'cache-type-v']) {
    const label = en(
      key === 'cache-type-k' ? 'preset-k-cache-type' : 'preset-v-cache-type',
    )
    const { main } = await loaded({
      preset: () => reply(200, exactOf({ [key]: 'q8_0' })),
    })
    const select = selectOf(main, label)
    expect(
      optionTexts(select),
      `${key}: the eleven options in the table's order`,
    ).toEqual(CACHE_TYPES)
    expect(
      optionsOf(select).map((o) => o.value),
      `${key}: each option carries the value the file holds`,
    ).toEqual(CACHE_TYPE_VALUES)
    expect(selectedText(select), `${key}: q8_0 selects its own option`).toBe(
      'q8_0',
    )
    cleanup()
    vi.unstubAllGlobals()
    for (const value of CACHE_TYPE_VALUES.slice(1)) {
      const { main } = await loaded({
        preset: () => reply(200, exactOf({ [key]: value })),
      })
      expect(
        selectedText(selectOf(main, label)),
        `${key}: ${value} selects its own option`,
      ).toBe(value)
      cleanup()
      vi.unstubAllGlobals()
    }
  }
})

test('2e.96, 2e.99: a cache type no option lists (q3_k) becomes one extra last option carrying it as its value and its text, selected, and so does one differing only in case', async () => {
  for (const value of ['q3_k', 'F16']) {
    const { main } = await loaded({
      preset: () => reply(200, exactOf({ 'cache-type-k': value })),
    })
    const select = selectOf(main, en('preset-k-cache-type'))
    const opts = optionsOf(select)
    expect(opts.length, `${value}: the listed options plus one`).toBe(
      CACHE_TYPES.length + 1,
    )
    expect(
      optionTexts(select).slice(0, CACHE_TYPES.length),
      `${value}: the listed options keep their place`,
    ).toEqual(CACHE_TYPES)
    const extra = opts[opts.length - 1]!
    expect(extra.value, `${value}: the extra option carries the value`).toBe(
      value,
    )
    expect((extra.textContent ?? '').trim(), `${value}: and its text`).toBe(
      value,
    )
    expect(selectedText(select), `${value}: the extra option is selected`).toBe(
      value,
    )
    expect(select.disabled, `${value}: the select disabled`).toBe(true)
    cleanup()
    vi.unstubAllGlobals()
  }
})

test('2e.97, 2e.1.3: Flash attention and Backend sampling are the tri-state and Load on startup is off / on, with the shared option texts and their mappings', async () => {
  for (const [key, options] of [
    ['flash-attn', TRISTATE],
    ['backend-sampling', TRISTATE],
    ['load-on-startup', OFF_ON],
  ] as [string, string[]][]) {
    const f = FIELDS.find((x) => x.key === key)!
    const { main } = await loaded({
      preset: () => reply(200, exactOf({ [key]: 'false' })),
    })
    const select = selectOf(main, f.label)
    expect(optionTexts(select), `${key}: the shared options of 1.3`).toEqual(
      options,
    )
    expect(selectedText(select), `${key}: false shows off`).toBe(
      en('option-off'),
    )
    cleanup()
    vi.unstubAllGlobals()
    const { main: absentMain } = await loaded({
      preset: () => reply(200, exactOf({})),
    })
    expectAbsentSelect(absentMain, f)
    cleanup()
    vi.unstubAllGlobals()
  }
})

// ── The unit, the numbers and the text field (87, 89, 99) ──────────────

test('2e.89, 2e.96: Cache RAM shows the unit MiB after the input as a literal, and no other field of 2e.2 carries a unit', async () => {
  const { main } = await loaded({ preset: () => reply(200, docOf(SET)) })
  const field = fieldOf(main, en('preset-cache-ram'))
  const units = [...field.querySelectorAll('.unit')].map((u) => text(u))
  expect(units, 'one unit in the Cache RAM field').toEqual(['MiB'])
  const group = groupBox(main, en('preset-cache-ram'))
  expect(
    before(
      inputOf(group, en('preset-cache-ram')),
      field.querySelector('.unit')!,
    ),
    'the unit sits after the input',
  ).toBe(true)
  // The label is the strings text, with no unit in it (89: "Cache RAM (MiB)"
  // becomes "Cache RAM" plus MiB).
  expect(text(field), 'the label is shown').toContain(en('preset-cache-ram'))
  expect(text(field), 'no unit in the label').not.toContain('(MiB)')
  for (const f of FIELDS)
    if (f.key !== 'cache-ram')
      expect(
        fieldOf(main, f.label).querySelectorAll('.unit').length,
        `${f.key}: no unit`,
      ).toBe(0)
})

test('2e.87, 2e.96, 2e.97: an integer key of 2e.2 takes the numeric form of the input and a float key a plain text input', async () => {
  const { main } = await loaded({ preset: () => reply(200, docOf(SET)) })
  for (const f of FIELDS.filter((x) => x.kind === 'number')) {
    const input = inputOf(groupBox(main, f.label), f.label)
    expect(
      input.getAttribute('inputmode'),
      `${f.key}: ${f.integer ? 'numeric' : 'plain'}`,
    ).toBe(f.integer ? 'numeric' : null)
  }
})

test('2e.87, 2e.99: a number of 2e.2 shows the reply string exactly, with no parsing and no reformatting: a comma decimal, a negative, a trailing zero, a leading zero, spaces', async () => {
  for (const [key, value] of [
    ['presence-penalty', '1,5'],
    ['temp', '0.60'],
    ['cache-ram', '-1'],
    ['top-k', '020'],
    ['min-p', ' 0 '],
  ] as [string, string][]) {
    const f = FIELDS.find((x) => x.key === key)!
    const { main } = await loaded({
      preset: () => reply(200, exactOf({ [key]: value })),
    })
    const group = groupBox(main, f.label)
    expect(inputOf(group, f.label).value, `${key} ${value}`).toBe(value)
    expect(boxOf(group).checked, `${key} ${value}: the box unticked`).toBe(
      false,
    )
    cleanup()
    vi.unstubAllGlobals()
  }
})

test('2e.98, 2e.87: Chat kwargs is a text field: a monospace input named by its label showing the reply string exactly, absent an empty input with no placeholder, disabled and with no unit', async () => {
  const f = FIELDS.find((x) => x.key === 'chat-template-kwargs')!
  for (const [value, shown] of [
    ['{"enable_thinking":false}', '{"enable_thinking":false}'],
    ['a b  c', 'a b  c'],
    // Spaces and case kept: a value trimmed or lower-cased on the way to the
    // input is not the file's string.
    [' {"A_b":TRUE} ', ' {"A_b":TRUE} '],
    ['', ''],
  ] as [string, string][]) {
    const { main } = await loaded({
      preset: () => reply(200, exactOf({ [f.key]: value })),
    })
    const field = fieldOf(main, f.label)
    const input = inputOfField(main, f.label)
    expect(input.value, `${f.key} ${value}: shown as written`).toBe(shown)
    expect(
      input.getAttribute('placeholder') ?? '',
      `${f.key} ${value}: no placeholder in 2e`,
    ).toBe('')
    expect(input.disabled, `${f.key}: the input disabled`).toBe(true)
    expect(
      input.getAttribute('aria-disabled'),
      `${f.key}: never aria-disabled`,
    ).toBeNull()
    expect(
      input.getAttribute('readonly'),
      `${f.key}: never readonly`,
    ).toBeNull()
    // Monospace is drawn by Field.tsx's mono class: jsdom reads no font.
    expect(input.className, `${f.key}: monospace`).toContain('mono')
    expect(field.querySelectorAll('.unit').length, `${f.key}: no unit`).toBe(0)
    cleanup()
    vi.unstubAllGlobals()
  }
})

// ── The cards: tables only, disabled, in their place (83, 85, 90) ──────

test('2e.90: the three cards of 2e.2 are tables only, so no card has an h4, a role=group division, a note paragraph or an intro', async () => {
  const { main } = await loaded({ preset: () => reply(200, docOf(SET)) })
  for (const title of CARDS) {
    const card = cardOf(main, title)
    for (const sel of ['h4', '[role="group"]', '.note', '.intro'])
      expect(card.querySelectorAll(sel).length, `${en(title)}: no ${sel}`).toBe(
        0,
      )
    // A note sits between the fields, so it is a paragraph of the card itself:
    // every paragraph is inside a field (its hint), none is between them.
    for (const p of [...card.querySelectorAll('p')])
      expect(
        p.closest('.field') !== null,
        `${en(title)}: the paragraph is a hint, not a note`,
      ).toBe(true)
  }
})

test('2e.85: every control of the three cards of 2e.2 is natively disabled, the default boxes included, while every About toggle of 2e.2 stays enabled', async () => {
  const { main } = await loaded({ preset: () => reply(200, docOf(SET)) })
  const controls = CARDS.flatMap((title) => [
    ...cardOf(main, title).querySelectorAll<HTMLElement>(
      'input, select, textarea',
    ),
  ])
  expect(controls.length, 'the controls of 96, 97 and 98').toBe(CONTROLS)
  for (const el of controls) {
    const id = el.getAttribute('id') ?? el.tagName
    expect((el as HTMLInputElement).disabled, `${id}: disabled`).toBe(true)
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
      aboutOf(main, f.label).disabled,
      `${f.key}: its About toggle stays enabled`,
    ).toBe(false)
})

test('2e.83, 2e.96, 2e.97, 2e.98: the three cards of 2e.2 are D5.2 Cards inside the section of the h2, after the Embeddings card, in the order KV cache, Sampling overrides, Advanced', async () => {
  const { main, h1 } = await loaded({ preset: () => reply(200, docOf(SET)) })
  const h2 = settingsTitle(main)
  const section = h2.closest('section')
  expect(section, 'the h2 sits in a section').toBeTruthy()
  const nodes: [string, Node][] = [
    ['the h1', h1],
    ['the h2', h2],
  ]
  for (const title of cardHeadingKeys()) {
    const card = cardOf(main, title)
    expect(
      section!.contains(card),
      `${en(title)}: the card is inside the section of the h2`,
    ).toBe(true)
    nodes.push([`the ${en(title)} card`, card])
  }
  for (let i = 0; i + 1 < nodes.length; i++)
    expect(
      before(nodes[i]![1], nodes[i + 1]![1]),
      `${nodes[i]![0]} before ${nodes[i + 1]![0]}`,
    ).toBe(true)
  expect(
    within(main)
      .getAllByRole('heading', { level: 3 })
      .map((h) => text(h)),
    'the cards, in the order of 83 (the list of test/presetCards.ts)',
  ).toEqual(cardHeadings())
})

// ── Keyboard (91) ───────────────────────────────────────────────────────

test('2e.91: Tab in main reaches the back link, the Engine About toggle and select, then every field About toggle of the page in card and field order (the list of test/presetCards.ts), and no control of a 2e card', async () => {
  const { main } = await loaded({ preset: () => reply(200, docOf(SET)) })
  expect(mainTabbable(main)).toEqual([
    en('preset-back'),
    `${en('field-about')} ${en('preset-engine')}`,
    en('preset-engine'),
    ...fieldAboutToggles(),
  ])
})

// ── The strings (100) ───────────────────────────────────────────────────

test('2e.100: the three cards, every label, every hint and every option of 2e.2 show the English of the 32 new strings of 100, Top-K, Top-P and Min-P are literals, and Cache RAM carries no unit in its label', async () => {
  const { main } = await loaded({ preset: () => reply(200, docOf(SET)) })
  expect(
    text(cardOf(main, 'preset-kv-cache').querySelector('h3')!),
    'the KV cache card',
  ).toBe(en('preset-kv-cache'))
  expect(
    text(cardOf(main, 'preset-sampling-overrides').querySelector('h3')!),
    'the Sampling overrides card',
  ).toBe(en('preset-sampling-overrides'))
  expect(
    text(cardOf(main, 'preset-advanced').querySelector('h3')!),
    'the Advanced card',
  ).toBe(en('preset-advanced'))
  // Every label is the strings text of 100 (a literal for Top-K, Top-P,
  // Min-P), it names its control, and the hint's English is the strings one.
  for (const f of FIELDS) {
    expect(
      text(fieldOf(main, f.label)).includes(f.label),
      `${f.key}: the label ${f.label} is shown`,
    ).toBe(true)
    expect(
      controlOf(main, f).getAttribute('id'),
      `${f.key}: the control is named`,
    ).toBeTruthy()
    expect(
      en(f.hint).length,
      `${f.hint}: the strings key has English`,
    ).toBeGreaterThan(0)
  }
  // The literals of 96 and 97 are the literals of the brief, not strings.
  expect(
    optionTexts(selectOf(main, en('preset-k-cache-type'))).slice(1),
    'the ten cache types, as the file writes them',
  ).toEqual(CACHE_TYPES.slice(1))
  // Top-K, Top-P and Min-P carry the literal of MP 987 as their label: each is
  // the group of its number field, named by that literal.
  for (const literal of ['Top-K', 'Top-P', 'Min-P'])
    expect(
      groupBox(main, literal).getAttribute('aria-labelledby'),
      `${literal}: the literal labels its number field`,
    ).toBeTruthy()
})
