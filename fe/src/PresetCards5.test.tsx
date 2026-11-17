// Slice 2e.5 (fe/briefs/02e-preset-cards.md, criteria 112 to 117): the last
// card of the read-only preset page, Speculative decoding, in the five named
// groups of 112. Everything is read-only, as in the cards of 2e.1 to 2e.4:
// every control carries the native disabled attribute (85) and the About
// toggles are the only tab stops (91). The card adds no flag of its own (the
// brief, 2e.4 and 2e.5), so 2e.3b's flag readings reach no field here.
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
  EN,
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
  fieldTabStops,
  saveBarTabStops,
} from './test/presetCards.ts'

const ID = 'Qwen3-27B'
const HASH = `#/models/${ID}`
const MODEL = '/var/lib/private-ai/.llama.cpp/models/Qwen3-27B-Q8_0.gguf'

/** The stub of criterion 115: every key of 112 set, each n-gram key a
 * distinct number, so the value a group shows names the group's own key. */
const SET: Record<string, string> = {
  'spec-draft-n-max': '16',
  'spec-draft-p-min': '0.75',
  'spec-draft-n-min': '2',
  'spec-draft-n-ubatch': '512',
  'spec-draft-type-k': 'q8_0',
  'spec-draft-type-v': 'q8_0',
  'spec-ngram-mod-n-match': '101',
  'spec-ngram-mod-n-min': '102',
  'spec-ngram-mod-n-max': '103',
  'spec-ngram-simple-size-n': '104',
  'spec-ngram-simple-size-m': '105',
  'spec-ngram-map-k-size-n': '106',
  'spec-ngram-map-k-size-m': '107',
  'spec-ngram-map-k4v-size-n': '108',
  'spec-ngram-map-k4v-size-m': '109',
  'spec-ngram-map-k4v-min-hits': '110',
}

/** The cache types of criterion 96, which 2e.5 applies to the two draft
 * cache-type selects: `default` is a string, the ten types literals. */
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

const CARD = 'preset-speculative-decoding'

/** The five groups of 112, in the table's order. `heading` is the group's h4
 * text: the strings text of `preset-draft-model-group`, and the four
 * speculator names as the INI literals they are (114, D7). */
type Group = { heading: string; headingKey?: string; keys: string[] }
const GROUPS: Group[] = [
  {
    heading: en('preset-draft-model-group'),
    headingKey: 'preset-draft-model-group',
    keys: [
      'spec-draft-n-max',
      'spec-draft-p-min',
      'spec-draft-n-min',
      'spec-draft-n-ubatch',
      'spec-draft-type-k',
      'spec-draft-type-v',
    ],
  },
  {
    heading: 'ngram-mod',
    keys: [
      'spec-ngram-mod-n-match',
      'spec-ngram-mod-n-min',
      'spec-ngram-mod-n-max',
    ],
  },
  {
    heading: 'ngram-simple',
    keys: ['spec-ngram-simple-size-n', 'spec-ngram-simple-size-m'],
  },
  {
    heading: 'ngram-map-k',
    keys: ['spec-ngram-map-k-size-n', 'spec-ngram-map-k-size-m'],
  },
  {
    heading: 'ngram-map-k4v',
    keys: [
      'spec-ngram-map-k4v-size-n',
      'spec-ngram-map-k4v-size-m',
      'spec-ngram-map-k4v-min-hits',
    ],
  },
]
const DRAFT = GROUPS[0]!.heading
/** The group headings, in page order (114). */
const GROUP_HEADINGS = GROUPS.map((g) => g.heading)

/** One row of the field table of 112. */
type Row = {
  /** The key of the section's `values`. */
  key: string
  /** The group of 112 the field sits in: its h4 text. */
  group: string
  /** The label as the page shows it. */
  label: string
  /** The strings key of the field's hint (88). */
  hint: string
  kind: 'number' | 'select'
  /** A number: an integer key takes the numeric input form (87); the table
   * gives one float key, `spec-draft-p-min`. */
  integer?: boolean
  /** A select's option texts, in the table's order. */
  options?: string[]
  /** A select: the option text it shows when the key is absent. */
  absentOption?: string
  /** The value of the 115 stub. */
  set: string
  /** What the field shows for it: the input's value, or the selected
   * option's text. */
  shown: string
}

/** A Number field of 112, integer (the table's `Number, integer` rows). */
function num(
  key: string,
  group: string,
  labelKey: string,
  hintKey: string,
  set: string,
): Row {
  return {
    key,
    group,
    label: en(labelKey),
    hint: hintKey,
    kind: 'number',
    integer: true,
    set,
    shown: set,
  }
}

/** The card's one float row, `Number, float` (spec-draft-p-min). */
function flt(key: string, group: string, labelKey: string, set: string): Row {
  return {
    key,
    group,
    label: en(labelKey),
    hint: `${labelKey}-hint`,
    kind: 'number',
    set,
    shown: set,
  }
}

/** A Select, cache types row of 112 (the options of 96). */
function cacheType(key: string, labelKey: string): Row {
  return {
    key,
    group: DRAFT,
    label: en(labelKey),
    hint: `${labelKey}-hint`,
    kind: 'select',
    options: CACHE_TYPES,
    absentOption: en('option-default'),
    set: 'q8_0',
    shown: 'q8_0',
  }
}

/** The field table of 112, in the GUI's order. */
const FIELDS: Row[] = [
  num(
    'spec-draft-n-max',
    DRAFT,
    'preset-draft-n-max',
    'preset-draft-n-max-hint',
    '16',
  ),
  flt('spec-draft-p-min', DRAFT, 'preset-draft-p-min', '0.75'),
  num(
    'spec-draft-n-min',
    DRAFT,
    'preset-draft-n-min',
    'preset-draft-n-min-hint',
    '2',
  ),
  num(
    'spec-draft-n-ubatch',
    DRAFT,
    'preset-draft-n-ubatch',
    'preset-draft-n-ubatch-hint',
    '512',
  ),
  cacheType('spec-draft-type-k', 'preset-draft-k-cache-type'),
  cacheType('spec-draft-type-v', 'preset-draft-v-cache-type'),
  num(
    'spec-ngram-mod-n-match',
    'ngram-mod',
    'preset-match-length',
    'preset-match-length-hint',
    '101',
  ),
  num(
    'spec-ngram-mod-n-min',
    'ngram-mod',
    'preset-min-draft',
    'preset-min-draft-hint',
    '102',
  ),
  num(
    'spec-ngram-mod-n-max',
    'ngram-mod',
    'preset-max-draft',
    'preset-max-draft-hint',
    '103',
  ),
  num(
    'spec-ngram-simple-size-n',
    'ngram-simple',
    'preset-lookup-n-gram',
    'preset-ngram-simple-lookup-hint',
    '104',
  ),
  num(
    'spec-ngram-simple-size-m',
    'ngram-simple',
    'preset-draft-m-gram',
    'preset-ngram-simple-draft-hint',
    '105',
  ),
  num(
    'spec-ngram-map-k-size-n',
    'ngram-map-k',
    'preset-lookup-n-gram',
    'preset-ngram-map-lookup-hint',
    '106',
  ),
  num(
    'spec-ngram-map-k-size-m',
    'ngram-map-k',
    'preset-draft-m-gram',
    'preset-ngram-map-draft-hint',
    '107',
  ),
  num(
    'spec-ngram-map-k4v-size-n',
    'ngram-map-k4v',
    'preset-lookup-n-gram',
    'preset-ngram-map-values-lookup-hint',
    '108',
  ),
  num(
    'spec-ngram-map-k4v-size-m',
    'ngram-map-k4v',
    'preset-draft-m-gram',
    'preset-ngram-map-values-draft-hint',
    '109',
  ),
  num(
    'spec-ngram-map-k4v-min-hits',
    'ngram-map-k4v',
    'preset-min-hits',
    'preset-min-hits-hint',
    '110',
  ),
]

const row = (key: string): Row => FIELDS.find((f) => f.key === key)!

/** The 31 new strings of 116. */
const NEW_STRINGS = [
  'preset-speculative-decoding',
  'preset-draft-model-group',
  'preset-draft-n-max',
  'preset-draft-n-max-hint',
  'preset-draft-p-min',
  'preset-draft-p-min-hint',
  'preset-draft-n-min',
  'preset-draft-n-min-hint',
  'preset-draft-n-ubatch',
  'preset-draft-n-ubatch-hint',
  'preset-draft-cache-note',
  'preset-draft-k-cache-type',
  'preset-draft-k-cache-type-hint',
  'preset-draft-v-cache-type',
  'preset-draft-v-cache-type-hint',
  'preset-match-length',
  'preset-match-length-hint',
  'preset-min-draft',
  'preset-min-draft-hint',
  'preset-max-draft',
  'preset-max-draft-hint',
  'preset-min-hits',
  'preset-min-hits-hint',
  'preset-lookup-n-gram',
  'preset-draft-m-gram',
  'preset-ngram-simple-lookup-hint',
  'preset-ngram-simple-draft-hint',
  'preset-ngram-map-lookup-hint',
  'preset-ngram-map-draft-hint',
  'preset-ngram-map-values-lookup-hint',
  'preset-ngram-map-values-draft-hint',
]

beforeEach(() => startAt(HASH))

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
  startAt('')
})

// ── Helpers (as PresetCards4.test.tsx) ──────────────────────────────────

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

/** A read whose `values` are the Assets keys plus `values`. */
function docOf(values: Record<string, string>) {
  return presetDoc({ model: MODEL, ...values })
}

/** A read whose `values` are exactly `values` (the stub of 115). */
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

/** The card of 112. */
function card(main: HTMLElement): HTMLElement {
  return cardOf(main, CARD)
}

/** The About toggle of the field named `label`, inside `scope` (88). */
function aboutIn(scope: HTMLElement, label: string): HTMLButtonElement {
  return within(scope).getByRole('button', {
    name: `${en('field-about')} ${label}`,
  }) as HTMLButtonElement
}

/** The field that holds the About toggle of `label` inside `scope` (87). */
function fieldIn(scope: HTMLElement, label: string): HTMLElement {
  const field = aboutIn(scope, label).closest('.field')
  expect(field, `${label}: its Field wrapper`).toBeTruthy()
  return field as HTMLElement
}

/** The card's group divisions of 90: a `role="group"` that is no field (a
 * field is a `fieldset.field`, whose role is group and whose name is its
 * label, so the two kinds come out of the same role query). */
function cardGroups(cardEl: HTMLElement): HTMLElement[] {
  return [
    ...cardEl.querySelectorAll<HTMLElement>('[role="group"], fieldset'),
  ].filter((g) => !g.classList.contains('field'))
}

/** The group division of 90 whose name is the h4 `heading`. */
function groupOf(cardEl: HTMLElement, heading: string): HTMLElement {
  const groups = within(cardEl)
    .getAllByRole('group', { name: heading })
    .filter((g) => !g.classList.contains('field'))
  expect(groups.length, `${heading}: one group division named by its h4`).toBe(
    1,
  )
  return groups[0] as HTMLElement
}

/** A number field's field, named by the label, inside `scope` (87). */
function fieldGroupIn(scope: HTMLElement, label: string): HTMLElement {
  return within(scope).getByRole('group', { name: label }) as HTMLElement
}

function boxOf(field: HTMLElement): HTMLInputElement {
  return within(field).getByRole('checkbox', {
    name: en('field-default'),
  }) as HTMLInputElement
}

function inputOf(field: HTMLElement, label: string): HTMLInputElement {
  return within(field).getByRole('textbox', { name: label }) as HTMLInputElement
}

function selectIn(scope: HTMLElement, label: string): HTMLSelectElement {
  return within(scope).getByRole('combobox', {
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

/** The selected option's text exactly as the page writes it (87). */
function selectedTextExact(select: HTMLSelectElement): string {
  expect(select.selectedIndex, 'an option is selected').toBeGreaterThanOrEqual(
    0,
  )
  return optionsOf(select)[select.selectedIndex]!.textContent ?? ''
}

function text(el: Element): string {
  return (el.textContent ?? '').replace(/\s+/g, ' ').trim()
}

/** The text of `cardEl` as a user reads it: what a closed hint holds is left
 * out, since jsdom keeps the text of a `hidden` element in `textContent`. */
function cardText(cardEl: HTMLElement): string {
  const open = cardEl.cloneNode(true) as HTMLElement
  open.querySelectorAll('[hidden]').forEach((el) => el.remove())
  return text(open)
}

/** The note paragraphs of a card: a `p` outside any field (90). */
function notesOf(cardEl: HTMLElement): HTMLElement[] {
  return [...cardEl.querySelectorAll('p')].filter(
    (p) => p.closest('.field') === null,
  )
}

/** The elements Tab reaches inside `root`, in DOM order (91). */
function mainTabbable(root: HTMLElement): string[] {
  return [
    ...root.querySelectorAll<HTMLElement>(
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

/** The accessible name of a group division of 90: the text of the element its
 * `aria-labelledby` names (the `h4`), which is what a screen reader reads on
 * entry. Not the group's own text: the group holds its fields, so their labels
 * and shown values are part of its `textContent` and none of its name. */
function groupName(group: HTMLElement): string {
  const ids = (group.getAttribute('aria-labelledby') ?? '')
    .split(/\s+/)
    .filter(Boolean)
  expect(
    ids.length,
    'the group is named through aria-labelledby',
  ).toBeGreaterThan(0)
  return ids
    .map((id) => document.getElementById(id)?.textContent ?? '')
    .join(' ')
    .replace(/\s+/g, ' ')
    .trim()
}

/** The group division of 90 the row sits in, in the card. */
function rowGroup(main: HTMLElement, f: Row): HTMLElement {
  return groupOf(card(main), f.group)
}

/** The control of a row's field, by its kind, inside its group. */
function controlOf(main: HTMLElement, f: Row): HTMLElement {
  const group = rowGroup(main, f)
  if (f.kind === 'number') return inputOf(fieldGroupIn(group, f.label), f.label)
  return selectIn(group, f.label)
}

/** What a field shows when its key is absent, whatever its kind (86, 87). */
function expectAbsent(main: HTMLElement, f: Row): void {
  const group = rowGroup(main, f)
  if (f.kind === 'number') {
    const field = fieldGroupIn(group, f.label)
    expect(boxOf(field).checked, `${f.key}: the default box ticked`).toBe(true)
    expect(inputOf(field, f.label).value, `${f.key}: an empty input`).toBe('')
  } else {
    expect(
      selectedText(selectIn(group, f.label)),
      `${f.key}: absent shows ${f.absentOption}`,
    ).toBe(f.absentOption)
  }
}

/** The unit elements of a field, with their text (89). */
function unitsOf(main: HTMLElement, f: Row): string[] {
  return [...fieldIn(rowGroup(main, f), f.label).querySelectorAll('.unit')].map(
    (u) => text(u),
  )
}

// ── One test per field (85, 86, 87, 88, 112, 115) ──────────────────────

test.each(FIELDS)(
  '2e.112, 2e.115, 2e.87, 2e.85: $key is a $kind of the $group group named by $label, shows the set value $set as the reply holds it, and every control is natively disabled',
  async (f) => {
    const { main } = await loaded({ preset: () => reply(200, docOf(SET)) })
    const group = rowGroup(main, f)
    const field = fieldIn(group, f.label)
    expect(
      card(main).contains(field),
      `${f.key}: its field is inside the ${en(CARD)} card`,
    ).toBe(true)
    // 112: the table's unit column is empty, and 89 puts a unit only there.
    expect(unitsOf(main, f), `${f.key}: no unit in the table`).toEqual([])
    if (f.kind === 'number') {
      const fieldGroup = fieldGroupIn(group, f.label)
      const box = boxOf(fieldGroup)
      const input = inputOf(fieldGroup, f.label)
      expect(input.value, `${f.key}: the string of the file exactly`).toBe(
        f.shown,
      )
      expect(box.checked, `${f.key}: the default box unticked`).toBe(false)
      expect(box.disabled, `${f.key}: the box disabled`).toBe(true)
      expect(input.disabled, `${f.key}: the input disabled`).toBe(true)
      // 87: an integer key takes the numeric form, the float key not.
      expect(
        input.getAttribute('inputmode'),
        `${f.key}: ${f.integer ? 'numeric' : 'plain'}`,
      ).toBe(f.integer ? 'numeric' : null)
    } else {
      const select = selectIn(group, f.label)
      expect(optionTexts(select), `${f.key}: the options in order`).toEqual(
        f.options,
      )
      expect(selectedText(select), `${f.key}: the mapping selects`).toBe(
        f.shown,
      )
      expect(select.disabled, `${f.key}: the select disabled`).toBe(true)
    }
    // 85: the native disabled attribute, never aria-disabled, never readonly.
    for (const el of [
      controlOf(main, f),
      ...(f.kind === 'number'
        ? [fieldGroupIn(group, f.label).querySelector('input') as Element]
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
      aboutIn(group, f.label).disabled,
      `${f.key}: About stays enabled`,
    ).toBe(false)
  },
)

test.each(FIELDS)(
  '2e.115, 2e.86: $key is absent from a values of {} so its field shows the absent form of $kind',
  async (f) => {
    const { main } = await loaded({ preset: () => reply(200, exactOf({})) })
    expectAbsent(main, f)
  },
)

test.each(FIELDS)(
  '2e.88, 2e.115: $key has an About toggle named About its label, with aria-expanded and aria-controls, and its hint opens as one paragraph per line of the GUI text and closes again',
  async (f) => {
    const { main } = await loaded({ preset: () => reply(200, docOf(SET)) })
    const group = rowGroup(main, f)
    const lines = en(f.hint).split('\n')
    expect(lines.length, `${f.hint}: the hint has lines`).toBeGreaterThan(0)
    const about = aboutIn(group, f.label)
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

// ── The five groups of 112 and 114, and the note of 90 ─────────────────

test('2e.90a: each of the five groups of 112 is drawn as the Server page subgroup: one div.subgroup, a direct child of the card section, holding exactly its h4 and then the role=group division named by that h4', async () => {
  const { main } = await loaded({ preset: () => reply(200, docOf(SET)) })
  const el = card(main)
  const subs = [...el.children].filter(
    (c) => c.tagName === 'DIV' && c.classList.contains('subgroup'),
  )
  expect(subs.length, `${en(CARD)}: one div.subgroup per group of 112`).toBe(
    GROUPS.length,
  )
  for (const g of GROUPS) {
    const h4 = within(el).getByRole('heading', { level: 4, name: g.heading })
    const sub = h4.parentElement
    expect(sub, `${g.heading}: the h4 has a parent`).toBeTruthy()
    expect(sub!.tagName, `${g.heading}: the h4 sits in a div`).toBe('DIV')
    expect(
      String(sub!.className),
      `${g.heading}: the h4 sits in a .subgroup`,
    ).toContain('subgroup')
    expect(
      sub!.parentElement,
      `${g.heading}: the .subgroup is a direct child of the card section`,
    ).toBe(el)
    const kids = [...sub!.children]
    expect(
      kids.length,
      `${g.heading}: the .subgroup holds exactly two children`,
    ).toBe(2)
    expect(kids[0], `${g.heading}: the first child is the h4`).toBe(h4)
    expect(
      kids[1],
      `${g.heading}: the second child is the role=group division`,
    ).toBe(groupOf(el, g.heading))
    expect(
      kids[1]!.getAttribute('role'),
      `${g.heading}: the second child is a role=group division`,
    ).toBe('group')
    const id = h4.getAttribute('id') ?? ''
    expect(id, `${g.heading}: the h4 carries an id`).not.toBe('')
    expect(
      kids[1]!.getAttribute('aria-labelledby'),
      `${g.heading}: the division is named by the h4 id of 90a`,
    ).toBe(id)
  }
})

test('2e.112, 2e.83: Speculative decoding is a D5.2 Card inside the section of the h2, the last card after Vision, and the page shows every card in the order of 83', async () => {
  const { main, h1 } = await loaded({ preset: () => reply(200, docOf(SET)) })
  const h2 = settingsTitle(main)
  const section = h2.closest('section')
  expect(section, 'the h2 sits in a section').toBeTruthy()
  const nodes: [string, Node][] = [
    ['the h1', h1],
    ['the h2', h2],
  ]
  for (const title of cardHeadingKeys()) {
    const el = cardOf(main, title)
    expect(
      section!.contains(el),
      `${en(title)}: the card is inside the section of the h2`,
    ).toBe(true)
    nodes.push([`the ${en(title)} card`, el])
  }
  for (let i = 0; i + 1 < nodes.length; i++)
    expect(
      before(nodes[i]![1], nodes[i + 1]![1]),
      `${nodes[i]![0]} before ${nodes[i + 1]![0]}`,
    ).toBe(true)
  const headings = within(main)
    .getAllByRole('heading', { level: 3 })
    .map((h) => text(h))
  expect(
    headings,
    'the cards, in the order of 83 (the list of test/presetCards.ts)',
  ).toEqual(cardHeadings())
  expect(headings[headings.length - 1], 'the last card of 83').toBe(en(CARD))
})

test('2e.112, 2e.114, 2e.90: the card is the five groups of 112 in the table order, each an h4 and a role=group division named by it through aria-labelledby, and the card holds no other group', async () => {
  const { main } = await loaded({ preset: () => reply(200, docOf(SET)) })
  const el = card(main)
  const h4s = [...el.querySelectorAll('h4')]
  expect(
    h4s.map((h) => text(h)),
    `${en(CARD)}: the five h4 of 114, in order`,
  ).toEqual(GROUP_HEADINGS)
  const groups = cardGroups(el)
  expect(groups.length, `${en(CARD)}: five role=group divisions`).toBe(
    GROUPS.length,
  )
  GROUPS.forEach((g, i) => {
    const group = groupOf(el, g.heading)
    const h4 = h4s[i]!
    expect(text(h4), `${g.heading}: the h4 text of 114`).toBe(g.heading)
    // 90: the group is named by its h4, through aria-labelledby.
    const ids = (group.getAttribute('aria-labelledby') ?? '').split(/\s+/)
    expect(
      ids.length,
      `${g.heading}: aria-labelledby names something`,
    ).toBeGreaterThan(0)
    const named = ids.map((id) => document.getElementById(id))
    expect(
      named.some((n) => n === h4),
      `${g.heading}: the division is named by the ${i + 1}th h4`,
    ).toBe(true)
    expect(
      groupName(group),
      `${g.heading}: the group's accessible name is its h4`,
    ).toBe(g.heading)
    // The order of 112: an h4 opens its group, and the group ends before the
    // next group's h4.
    expect(before(h4, group), `${g.heading}: the h4 before its group`).toBe(
      true,
    )
    if (i + 1 < GROUPS.length)
      expect(
        before(group, h4s[i + 1]!),
        `${g.heading} before ${GROUPS[i + 1]!.heading}`,
      ).toBe(true)
    expect(
      el.contains(group),
      `${g.heading}: the group is inside the card`,
    ).toBe(true)
  })
})

test('2e.112, 2e.90: every field of 112 sits in the group its row names, in the row order, so each group holds exactly the fields of its rows and no field is outside a group', async () => {
  const { main } = await loaded({ preset: () => reply(200, docOf(SET)) })
  const el = card(main)
  for (const g of GROUPS) {
    const group = groupOf(el, g.heading)
    const fields = [...group.querySelectorAll('.field')]
    expect(
      fields.length,
      `${g.heading}: the fields of its ${g.keys.length} rows`,
    ).toBe(g.keys.length)
    g.keys.forEach((key, i) => {
      const f = row(key)
      expect(
        fields[i]!.contains(aboutIn(group, f.label)),
        `${key}: the ${i + 1}th field of the ${g.heading} group is ${f.label}`,
      ).toBe(true)
    })
  }
  const all = [...el.querySelectorAll('.field')]
  expect(all.length, `${en(CARD)}: as many fields as the table has rows`).toBe(
    FIELDS.length,
  )
  for (const field of all)
    expect(
      field.closest('[role="group"]:not(.field), fieldset:not(.field)') ??
        (field.matches('[role="group"]')
          ? field.parentElement
          : field.closest('[role="group"]')),
      'every field of 112 sits in one of the card groups',
    ).toBeTruthy()
})

test('2e.112, 2e.90: preset-draft-cache-note is the one note of the card, a muted paragraph of the Draft model group after Draft n-ubatch and before Draft K-cache type, and it is no field', async () => {
  const { main } = await loaded({ preset: () => reply(200, docOf(SET)) })
  const el = card(main)
  const notes = notesOf(el)
  expect(notes.length, `${en(CARD)}: one note between the fields`).toBe(1)
  const note = notes[0]!
  expect(note.tagName, `${en(CARD)}: the note is a paragraph`).toBe('P')
  expect(text(note), 'preset-draft-cache-note: the text of toml 377').toBe(
    en('preset-draft-cache-note'),
  )
  expect(
    note.className,
    'preset-draft-cache-note: the note style of 90',
  ).toContain('note')
  expect(
    note.closest('.field'),
    'preset-draft-cache-note: the note is not inside a field',
  ).toBeNull()
  const group = groupOf(el, DRAFT)
  expect(
    group.contains(note),
    'preset-draft-cache-note: inside the Draft model group',
  ).toBe(true)
  expect(
    before(fieldIn(group, en('preset-draft-n-ubatch')), note),
    'Draft n-ubatch before the note',
  ).toBe(true)
  expect(
    before(note, fieldIn(group, en('preset-draft-k-cache-type'))),
    'the note before Draft K-cache type',
  ).toBe(true)
  expect(
    within(el).queryByRole('button', {
      name: `${en('field-about')} ${en('preset-draft-cache-note')}`,
    }),
    'preset-draft-cache-note: no About toggle for a note',
  ).toBeNull()
})

// ── The same label in several groups (113) ──────────────────────────────

const SAME_LABELS = ['preset-lookup-n-gram', 'preset-draft-m-gram']

test('2e.113: the three Lookup n-gram and the three Draft m-gram fields carry one label each, are told apart by their group, and each group shows the value of its own key', async () => {
  const { main } = await loaded({ preset: () => reply(200, docOf(SET)) })
  const el = card(main)
  for (const key of SAME_LABELS)
    expect(
      within(el).getAllByRole('group', { name: en(key) }).length,
      `${en(key)}: three fields of that label in the card`,
    ).toBe(3)
  // 113: the lookup of the brief, the field's group within the card group.
  for (const g of GROUPS.slice(2)) {
    const group = groupOf(el, g.heading)
    // The key of the group's own row of 112: the speculator name with its
    // `spec-` prefix, so the value it shows names the group.
    for (const [labelKey, key] of [
      ['preset-lookup-n-gram', `spec-${g.heading}-size-n`],
      ['preset-draft-m-gram', `spec-${g.heading}-size-m`],
    ] as [string, string][]) {
      const fieldGroup = within(group).getByRole('group', {
        name: en(labelKey),
      })
      expect(
        inputOf(fieldGroup as HTMLElement, en(labelKey)).value,
        `${key}: the ${g.heading} group shows its own key's value`,
      ).toBe(SET[key])
    }
  }
})

test('2e.113: the About toggles are About Lookup n-gram thrice and About Draft m-gram thrice, each controlling the hint of its own group, and opening one shows that group hint and no other', async () => {
  const { main } = await loaded({ preset: () => reply(200, docOf(SET)) })
  const el = card(main)
  for (const key of SAME_LABELS)
    expect(
      within(el).getAllByRole('button', {
        name: `${en('field-about')} ${en(key)}`,
      }).length,
      `About ${en(key)}: three toggles, one per group`,
    ).toBe(3)
  // Each n-gram group's own hint of 116, per label.
  const HINTS = new Map<string, string>([
    ['ngram-simple preset-lookup-n-gram', 'preset-ngram-simple-lookup-hint'],
    ['ngram-simple preset-draft-m-gram', 'preset-ngram-simple-draft-hint'],
    ['ngram-map-k preset-lookup-n-gram', 'preset-ngram-map-lookup-hint'],
    ['ngram-map-k preset-draft-m-gram', 'preset-ngram-map-draft-hint'],
    [
      'ngram-map-k4v preset-lookup-n-gram',
      'preset-ngram-map-values-lookup-hint',
    ],
    ['ngram-map-k4v preset-draft-m-gram', 'preset-ngram-map-values-draft-hint'],
  ])
  for (const g of GROUPS.slice(2))
    for (const labelKey of SAME_LABELS) {
      const hintKey = HINTS.get(`${g.heading} ${labelKey}`)!
      const group = groupOf(el, g.heading)
      const about = aboutIn(group, en(labelKey))
      const hintId = about.getAttribute('aria-controls')
      expect(
        hintId,
        `${g.heading} ${en(labelKey)}: aria-controls names a hint`,
      ).toBeTruthy()
      const hint = document.getElementById(hintId!)
      expect(
        hint,
        `${g.heading} ${en(labelKey)}: the hint is in the page`,
      ).toBeTruthy()
      expect(
        group.contains(hint!),
        `${g.heading} ${en(labelKey)}: the hint is next to its field`,
      ).toBe(true)
      expect(isHidden(hint), `${hintKey}: closed at first`).toBe(true)
      fireEvent.click(about)
      expect(
        [...hint!.querySelectorAll('p')].map((p) => text(p)),
        `${g.heading} ${en(labelKey)}: the hint is the group's own text of 116`,
      ).toEqual(
        en(hintKey)
          .split('\n')
          .map((l) => l.trim()),
      )
      fireEvent.click(about)
      expect(isHidden(hint), `${hintKey}: closed again`).toBe(true)
    }
})

test('2e.114: the card has exactly five h4 whose texts are the GUI texts of 114: the string preset-draft-model-group, and the four speculator names as the INI literals they are, each naming its group', async () => {
  const { main } = await loaded({ preset: () => reply(200, docOf(SET)) })
  const el = card(main)
  expect(
    [...el.querySelectorAll('h4')].map((h) => text(h)),
    'the five h4 texts of 114',
  ).toEqual(GROUP_HEADINGS)
  expect(
    el.querySelector('h4')!.textContent,
    'the Draft model heading is the strings text of toml 368',
  ).toBe(en('preset-draft-model-group'))
  // 114 and D7: the four names are INI values, not strings: each is a part of
  // every key of its group, and no strings text of the test's English is one.
  for (const g of GROUPS.slice(1)) {
    for (const key of g.keys)
      expect(
        key.includes(g.heading),
        `${key}: its key carries the group name ${g.heading}`,
      ).toBe(true)
    expect(
      Object.values(EN).includes(g.heading),
      `${g.heading}: no strings text is a speculator name`,
    ).toBe(false)
  }
  for (const g of GROUPS)
    expect(
      groupOf(el, g.heading),
      `${g.heading}: the group division carries the heading's name`,
    ).toBeTruthy()
})

// ── The cache-type selects (96, applied by 2e.5) ───────────────────────

test('2e.112, 2e.96: Draft K-cache type and Draft V-cache type list default then the ten literals of 96 in order with the file values, absent shows default, and each listed value selects its own option', async () => {
  for (const f of FIELDS.filter((x) => x.kind === 'select')) {
    const { main } = await loaded({ preset: () => reply(200, exactOf(SET)) })
    const select = selectIn(rowGroup(main, f), f.label)
    expect(
      optionTexts(select),
      `${f.key}: the eleven options in the table's order`,
    ).toEqual(CACHE_TYPES)
    expect(
      optionsOf(select).map((o) => o.value),
      `${f.key}: each option carries the value the file holds`,
    ).toEqual(CACHE_TYPE_VALUES)
    expect(selectedText(select), `${f.key}: q8_0 selects its own option`).toBe(
      'q8_0',
    )
    cleanup()
    vi.unstubAllGlobals()
    const { main: absent } = await loaded({
      preset: () => reply(200, exactOf({})),
    })
    expect(
      selectedText(selectIn(rowGroup(absent, f), f.label)),
      `${f.key}: absent shows default`,
    ).toBe(en('option-default'))
    cleanup()
    vi.unstubAllGlobals()
    for (const value of CACHE_TYPE_VALUES.slice(1)) {
      const { main: one } = await loaded({
        preset: () => reply(200, exactOf({ [f.key]: value })),
      })
      expect(
        selectedText(selectIn(rowGroup(one, f), f.label)),
        `${f.key}: ${value} selects its own option`,
      ).toBe(value)
      cleanup()
      vi.unstubAllGlobals()
    }
  }
})

test('2e.112, 2e.96, 2e.87: a draft cache type no option lists (q3_k, and F16 which differs only in case) becomes one extra last option carrying it as its value and its text, selected', async () => {
  for (const value of ['q3_k', 'F16']) {
    const { main } = await loaded({
      preset: () => reply(200, exactOf({ 'spec-draft-type-k': value })),
    })
    const select = selectIn(
      rowGroup(main, row('spec-draft-type-k')),
      row('spec-draft-type-k').label,
    )
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
    cleanup()
    vi.unstubAllGlobals()
  }
})

// ── Values exactly as the file holds them, and absent keys (86, 87, 89) ─

test('2e.87, 2e.115: a value of 2e.5 shows exactly as the reply holds it, with no parsing, no trimming and no case change: a number with spaces and a leading zero, a float with a trailing zero, and a cache type with spaces', async () => {
  for (const [key, value] of [
    ['spec-draft-n-max', ' 16 '],
    ['spec-draft-n-max', '0172032'],
    ['spec-draft-p-min', ' 0.75 '],
    ['spec-draft-p-min', '0.750'],
    ['spec-draft-n-ubatch', '512.0'],
    ['spec-ngram-map-k4v-min-hits', '007'],
  ] as [string, string][]) {
    const f = row(key)
    const { main } = await loaded({
      preset: () => reply(200, exactOf({ [key]: value })),
    })
    const fieldGroup = fieldGroupIn(rowGroup(main, f), f.label)
    expect(
      inputOf(fieldGroup, f.label).value,
      `${key} ${JSON.stringify(value)}`,
    ).toBe(value)
    expect(boxOf(fieldGroup).checked, `${key} ${value}: the box unticked`).toBe(
      false,
    )
    cleanup()
    vi.unstubAllGlobals()
  }
  // A select shows the string the page adds for it as written.
  const { main } = await loaded({
    preset: () => reply(200, exactOf({ 'spec-draft-type-v': ' q8_0 ' })),
  })
  expect(
    selectedTextExact(
      selectIn(
        rowGroup(main, row('spec-draft-type-v')),
        row('spec-draft-type-v').label,
      ),
    ),
    'an unlisted draft cache type keeps its spaces and its case',
  ).toBe(' q8_0 ')
})

test('2e.86, 2e.115: a key of 112 whose value is the empty string reads as absent, for every key of the card (2e.5 adds no flag of its own, so 2e.3b reads no key of this card as a flag)', async () => {
  for (const f of FIELDS) {
    const { main } = await loaded({
      preset: () => reply(200, exactOf({ ...SET, [f.key]: '' })),
    })
    expectAbsent(main, f)
    cleanup()
    vi.unstubAllGlobals()
  }
})

test('2e.89, 2e.112: no field of 112 carries a unit, so the card holds no unit element, and no field label carries MiB', async () => {
  const { main } = await loaded({ preset: () => reply(200, docOf(SET)) })
  const el = card(main)
  expect(
    el.querySelectorAll('.unit').length,
    `${en(CARD)}: no unit element`,
  ).toBe(0)
  const labels = [...el.querySelectorAll('.field-label')].map((l) => text(l))
  expect(labels.length, `${en(CARD)}: every field has a label`).toBe(
    FIELDS.length,
  )
  for (const label of labels)
    expect(label, `${label}: no unit in the label`).not.toContain('MiB')
})

test('2e.86, 2e.81: a key the page does not show adds no field, so the Speculative decoding card holds exactly the 16 fields of 112 in the table order', async () => {
  const { main } = await loaded({
    preset: () =>
      reply(
        200,
        docOf({
          ...SET,
          // 2g keys and the Engine key of 2e.1: none is a field of 112.
          'draft-max': 'zz-secret',
          'tensor-split': 'zz-secret',
          gpu_split: 'zz-secret',
        }),
      ),
  })
  const el = card(main)
  const fields = [...el.querySelectorAll('.field')]
  expect(fields.length, `${en(CARD)}: one Field per row of its table`).toBe(
    FIELDS.length,
  )
  // 81: the key adds no Field, so the card carries no id, no label and no
  // shown text for it. The ids are what a Field gives a control
  // (`preset-<key>-control`, `-box`, `-hint`).
  const shown = cardText(el)
  for (const secret of ['zz-secret', 'draft-max', 'tensor-split']) {
    expect(shown, `${en(CARD)}: ${secret} is not shown`).not.toContain(secret)
    for (const node of el.querySelectorAll('[id]'))
      expect(node.id, `${en(CARD)}: no id names ${secret}`).not.toContain(
        secret,
      )
  }
  for (let i = 0; i < FIELDS.length; i++)
    expect(
      // Scoped to the field's group of 112: "Lookup n-gram" and "Draft
      // m-gram" each name three fields of the card (113), so a card-wide
      // About query is ambiguous.
      fields[i]!.contains(
        aboutIn(rowGroup(main, FIELDS[i]!), FIELDS[i]!.label),
      ),
      `${FIELDS[i]!.key} is the ${i + 1}th field of ${en(CARD)}`,
    ).toBe(true)
})

// ── Keyboard (91) and the strings (116) ────────────────────────────────

test('2e.91, 2e.113: Tab in the card reaches only the 16 About toggles of 112 in the table order, About Lookup n-gram and About Draft m-gram three times each, and no control of the card', async () => {
  const { main } = await loaded({ preset: () => reply(200, docOf(SET)) })
  const el = card(main)
  const toggles = FIELDS.map((f) => `${en('field-about')} ${f.label}`)
  expect(
    mainTabbable(el),
    `${en(CARD)}: the About toggles in card and field order`,
  ).toEqual(toggles)
  for (const key of SAME_LABELS)
    expect(
      toggles.filter((n) => n.endsWith(en(key))).length,
      `About ${en(key)}: thrice`,
    ).toBe(3)
  // The whole page: the card's toggles are the tail of the tab order of 91.
  const tabbed = mainTabbable(main)
  expect(
    tabbed.slice(0, 3),
    'the back link, the Engine About toggle and the Engine select come first',
  ).toEqual([
    en('preset-back'),
    `${en('field-about')} ${en('preset-engine')}`,
    en('preset-engine'),
  ])
  expect(
    tabbed.slice(3),
    'the page tab order is the list of test/presetCards.ts',
  ).toEqual([...fieldTabStops(), ...saveBarTabStops()])
})

test('2e.116: the card, its five group headings, every label, hint and the note show the English of the 31 new strings of 116', async () => {
  expect(NEW_STRINGS.length, 'the strings of 116').toBe(31)
  expect(new Set(NEW_STRINGS).size, 'each of them named once').toBe(31)
  for (const key of NEW_STRINGS)
    expect(
      en(key).length,
      `${key}: the test's English is not empty`,
    ).toBeGreaterThan(0)
  const { main } = await loaded({ preset: () => reply(200, docOf(SET)) })
  const el = card(main)
  expect(text(el.querySelector('h3')!), 'the h3 of 116').toBe(en(CARD))
  const h4s = [...el.querySelectorAll('h4')].map((h) => text(h))
  for (const g of GROUPS) {
    expect(
      h4s.includes(g.heading),
      `${g.heading}: the group heading is shown`,
    ).toBe(true)
    if (g.headingKey)
      expect(
        Object.values(EN).includes(g.heading),
        `${g.headingKey}: the Draft model heading is a strings text`,
      ).toBe(true)
  }
  for (const f of FIELDS) {
    const field = fieldIn(rowGroup(main, f), f.label)
    expect(
      text(field).includes(f.label),
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
  expect(
    notesOf(el).map((p) => text(p)),
    'the note shows the English of toml 377',
  ).toEqual([en('preset-draft-cache-note')])
})
