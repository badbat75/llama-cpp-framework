// The Speculative decoding card editable, sub-slice 2h.9 of
// fe/briefs/02h-model-profile-save.md (189, 190, 191, 192, 193): the draft group
// enabled while the read's `model-draft` is not empty or `spec-type` holds a
// `draft-` entry (190, split as 2-models' 75: on `,`, each entry trimmed,
// empty entries and `none` dropped), each n-gram group enabled while
// `spec-type` lists its name (191), one reason line per DISABLED group right
// after its h4 inside its role="group" (192: `model-profile-draft-group-off`, and
// `model-profile-ngram-off` filled with the group's name), what each field of 2e's
// table 112 sends by its kind (133), the ranges of 157 and the decimal check of
// 165 on `spec-draft-p-min`, the full-value line of 134 kept only under a
// disabled control, `model-draft` and `spec-type` never sent, and the two new
// strings (193). The card's read-only look is 2e's 112, in
// ModelProfileCards5.test.tsx; the helpers are those of ModelProfileSave12, ModelProfileSave13
// and ModelProfileSave14.test.tsx, which this file repeats (a test file imports no
// other test file).
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
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

const ID = 'Qwen3-27B'
const HASH = `#/profiles/model/${ID}`
const PUT = `/api/profiles/model/${ID}`
const CARD = en('model-profile-speculative-decoding')
/** The five groups of 2e's table 112: the draft group's h4 is a string, the
 * four n-gram groups' are literal. */
const DRAFT = en('model-profile-draft-model-group')
const MOD = 'ngram-mod'
const SIMPLE = 'ngram-simple'
const MAP_K = 'ngram-map-k'
const MAP_K4V = 'ngram-map-k4v'
const DRAFT_K = en('model-profile-draft-k-cache-type')
const DRAFT_V = en('model-profile-draft-v-cache-type')
const DEFAULT = en('field-default')
/** 193: the two new strings, the brief's exact English as a literal. 198
 * changed the draft one: the Assets row it names is `model-profile-draft-model`,
 * "Draft model", not a file. */
const OFF_DRAFT =
  'Used only with a draft: a Draft model or a Drafter, and this model profile has neither (Assets).'
const offNgram = (group: string) =>
  `Used only when N-gram drafters (Assets) includes ${group}.`
/** 157's rows the card's number fields sit in, and 165's decimal text. */
const MIN_ONE = en('field-error-number-min', '1')
const MIN_ZERO = en('field-error-number-min', '0')
const RANGE_1_1024 = en('field-error-number', '1', '1024')
const RANGE_0_1024 = en('field-error-number', '0', '1024')
const RANGE_1_65535 = en('field-error-number', '1', '65535')
const DECIMAL = en('field-error-decimal')
/** The cache types of 2e's 96, the two draft selects (133: a select of words,
 * `default` sends null). */
const CACHE_VALUES = [
  'default',
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
/** 2e's 119: a select value past 20 characters needs the full-value line. */
const SELECT_21 = 'Last-Attention-Pool-X'

/** One field of table 112: its key, and the kind of its control. */
type SpecField = { key: string; kind: 'int' | 'float' | 'select' }

/** The five groups of 112, in the page's order, each with its fields. */
const GROUPS: { name: string; fields: SpecField[] }[] = [
  {
    name: DRAFT,
    fields: [
      { key: 'spec-draft-n-max', kind: 'int' },
      { key: 'spec-draft-p-min', kind: 'float' },
      { key: 'spec-draft-n-min', kind: 'int' },
      { key: 'spec-draft-n-ubatch', kind: 'int' },
      { key: 'spec-draft-type-k', kind: 'select' },
      { key: 'spec-draft-type-v', kind: 'select' },
    ],
  },
  {
    name: MOD,
    fields: [
      { key: 'spec-ngram-mod-n-match', kind: 'int' },
      { key: 'spec-ngram-mod-n-min', kind: 'int' },
      { key: 'spec-ngram-mod-n-max', kind: 'int' },
    ],
  },
  {
    name: SIMPLE,
    fields: [
      { key: 'spec-ngram-simple-size-n', kind: 'int' },
      { key: 'spec-ngram-simple-size-m', kind: 'int' },
    ],
  },
  {
    name: MAP_K,
    fields: [
      { key: 'spec-ngram-map-k-size-n', kind: 'int' },
      { key: 'spec-ngram-map-k-size-m', kind: 'int' },
    ],
  },
  {
    name: MAP_K4V,
    fields: [
      { key: 'spec-ngram-map-k4v-size-n', kind: 'int' },
      { key: 'spec-ngram-map-k4v-size-m', kind: 'int' },
      { key: 'spec-ngram-map-k4v-min-hits', kind: 'int' },
    ],
  },
]
/** The reason line a group shows while disabled (192). */
const offText = (group: string) =>
  group === DRAFT ? OFF_DRAFT : offNgram(group)

/** The 115 stub: every key of the card set, each n-gram key a distinct number,
 * so the value a group shows names the group's own key. */
const SPEC: Record<string, string> = {
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

/** A read: the card's keys as the 115 stub, with `spec-type` (absent when
 * `undefined`) and `extra` on top. */
function read(
  specType: string | undefined,
  extra: Record<string, string> = {},
): Record<string, string> {
  const values: Record<string, string> = { ...SPEC, ...extra }
  if (specType === undefined) delete values['spec-type']
  else values['spec-type'] = specType
  return values
}

/** The draft group enabled by 190: `model-draft` not empty, or a `draft-`
 * entry of the 75 split. The emptiness is of the trimmed value: the manager's
 * INI reader trims every value it reads (`crates/core/src/ini.rs`), so the
 * read hands the page a `model-draft` of spaces as an empty one. */
function draftOn(values: Record<string, string>): boolean {
  const entries = (values['spec-type'] ?? '')
    .split(',')
    .map((e) => e.trim())
    .filter((e) => e !== '' && e !== 'none')
  return (
    (values['model-draft'] ?? '').trim() !== '' ||
    entries.some((e) => e.startsWith('draft-'))
  )
}

beforeEach(() => {
  startAt(HASH)
})

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
  startAt('')
})

// ── Helpers (as ModelProfileSave12, 13 and 14.test.tsx) ──────────────────────

/** Waits for the page read with revision `r1` and the card's keys as `values`. */
async function form(
  values: Record<string, string> = read('ngram-mod,draft-mtp'),
  options: BffOptions = {},
) {
  const bff = fakeBff({
    ...options,
    modelProfile: () =>
      reply(
        200,
        modelProfileDoc({}, { revision: 'r1', values: { ...values } }),
      ),
  })
  render(<App />)
  const main = screen.getByRole('main')
  await within(main).findByRole('heading', { level: 1, name: ID })
  await within(main).findByRole('heading', { level: 3, name: CARD })
  return { bff, main }
}

/** The group of 112 named `name`: the `role="group"` division its h4 names. */
function groupOf(main: HTMLElement, name: string): HTMLElement {
  return within(main).getByRole('group', { name })
}

/** The group's h4 (112: each group is an h4 and the division it names). */
function headingOf(main: HTMLElement, name: string): HTMLElement {
  return within(main).getByRole('heading', { level: 4, name })
}

/** A field's control, by the id of its row (a Field's control is
 * `<id>-control`). */
function control(key: string): HTMLInputElement | HTMLSelectElement {
  const el = document.getElementById(`model-profile-${key}-control`)
  expect(el, `the control of ${key}`).not.toBeNull()
  return el as HTMLInputElement | HTMLSelectElement
}

/** A number field's default box (`<id>-box`). */
function boxOf(key: string): HTMLInputElement {
  const el = document.getElementById(`model-profile-${key}-box`)
  expect(el, `the default box of ${key}`).not.toBeNull()
  return el as HTMLInputElement
}

/** Every control of a group: a number field's box and input, a select alone
 * (190, 191: a group's fields move together). */
function controlsOf(
  group: (typeof GROUPS)[number],
): Array<HTMLInputElement | HTMLSelectElement> {
  const out: Array<HTMLInputElement | HTMLSelectElement> = []
  for (const f of group.fields) {
    if (f.kind !== 'select') out.push(boxOf(f.key))
    out.push(control(f.key))
  }
  return out
}

function type(input: HTMLElement, value: string) {
  fireEvent.change(input, { target: { value } })
}

function pick(main: HTMLElement, label: string, value: string) {
  const el = within(main).getByRole('combobox', {
    name: label,
  }) as HTMLSelectElement
  fireEvent.change(el, { target: { value } })
}

function saveButton(main: HTMLElement): HTMLElement {
  return within(main).getByRole('button', { name: en('save-button') })
}

/** Clicks Save changes and waits for the n-th PUT of this model profile. */
async function save(main: HTMLElement, bff: ReturnType<typeof fakeBff>, n = 1) {
  fireEvent.click(saveButton(main))
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

/** The ids of `aria-describedby`, in order. */
function describedIds(el: HTMLElement): string[] {
  return (el.getAttribute('aria-describedby') ?? '')
    .split(/\s+/)
    .filter(Boolean)
}

/** The text each id of `aria-describedby` gives. */
function described(el: HTMLElement): string[] {
  return describedIds(el).map(
    (id) => document.getElementById(id)?.textContent ?? `(no #${id})`,
  )
}

/** Asserts `el` is marked invalid with the error `text` under it (142, 157). */
function expectError(el: HTMLElement, text: string, what: string) {
  expect(el.getAttribute('aria-invalid'), `${what} aria-invalid`).toBe('true')
  expect(described(el), `${what} aria-describedby`).toContain(text)
  const error = describedIds(el)
    .map((id) => document.getElementById(id))
    .find((e) => e !== null && e.textContent === text)
  expect(error, `${what}: the error is shown`).toBeDefined()
  expect(isHidden(error!), `${what}: the error is shown`).toBe(false)
  expect(before(el, error!), `${what}: the error comes after the control`).toBe(
    true,
  )
}

/** Asserts `el` carries no mark and no error text (the field is clean). */
function expectNoError(el: HTMLElement, text: string, what: string) {
  expect(el.getAttribute('aria-invalid'), `${what}: no mark`).toBe(null)
  expect(described(el), `${what}: no error`).not.toContain(text)
}

/** The save bar's alert of 01's 31. */
function expectAlert(main: HTMLElement, text: string) {
  const el = within(main).getByText(text)
  expect(el.closest('[role="alert"]'), `${text} is in role="alert"`).not.toBe(
    null,
  )
}

/** The save bar's two buttons are inert: the form is clean. */
function expectInert(main: HTMLElement, what: string) {
  for (const name of ['save-button', 'save-discard'])
    expect(
      within(main)
        .getByRole('button', { name: en(name) })
        .getAttribute('aria-disabled'),
      `${what}: the buttons inert`,
    ).toBe('true')
}

/** The reason line of a group, by its text, inside the group (192). */
function offLine(main: HTMLElement, group: string): HTMLElement {
  const el = within(groupOf(main, group)).getByText(offText(group))
  expect(isHidden(el), `${group}: the line is shown`).toBe(false)
  return el
}

// ── 190: the draft group ──────────────────────────────────────────────

/** The reads 190 counts as "has a draft", and the ones it does not. */
const DRAFT_ON = [
  { name: 'spec-type draft-mtp, no model-draft', values: read('draft-mtp') },
  {
    // 75: the empty entries the commas leave are dropped, so nothing beside the
    // drafter changes the answer.
    name: 'spec-type ",draft-mtp,": empty entries beside a drafter',
    values: read(',draft-mtp,'),
  },
  {
    name: 'spec-type ngram-mod,draft-mtp, no model-draft',
    values: read('ngram-mod,draft-mtp'),
  },
  {
    name: 'model-draft present, spec-type ngram-only',
    values: read('ngram-mod', {
      'model-draft': 'Qwen3-27B-DFlash-Q8_0.gguf',
    }),
  },
  {
    // 75: each entry trimmed, so the spaces around the comma change nothing.
    name: 'spec-type " ngram-simple , draft-eagle3 "',
    values: read(' ngram-simple , draft-eagle3 '),
  },
  {
    // The read hands the page the file's value trimmed (the manager's INI
    // reader, crates/core/src/ini.rs): the spaces around this name are not
    // part of it, what is left is a file name.
    name: 'model-draft "  Qwen3-27B-DFlash-Q8_0.gguf " (spaces around), spec-type ngram-only',
    values: read('ngram-mod', {
      'model-draft': '  Qwen3-27B-DFlash-Q8_0.gguf ',
    }),
  },
]

const DRAFT_OFF = [
  {
    name: 'spec-type ngram-mod only, no model-draft',
    values: read('ngram-mod'),
  },
  { name: 'no spec-type at all', values: read(undefined) },
  {
    name: 'model-draft empty and spec-type none (75 drops none)',
    values: read('none', { 'model-draft': '' }),
  },
  {
    // The manager's INI reader trims every value (crates/core/src/ini.rs), so
    // a model-draft of only spaces reaches the page as the empty value: 190
    // has no draft, and no `draft-` entry stands in for one.
    name: 'model-draft "   " (spaces only), spec-type ngram-mod',
    values: read('ngram-mod', { 'model-draft': '   ' }),
  },
]

test.each(DRAFT_ON)(
  '2h.190: $name: the draft group is enabled, every one of its controls, and its line model-profile-draft-group-off is away',
  async (c) => {
    const { main } = await form(c.values)
    expect(
      draftOn(c.values),
      `${c.name}: 190's own rule says the group is enabled`,
    ).toBe(true)
    for (const el of controlsOf(GROUPS[0]!))
      expect(el.disabled, `${el.id}: enabled (190)`).toBe(false)
    expect(
      within(main).queryByText(OFF_DRAFT),
      '192: no line for an enabled group',
    ).toBeNull()
  },
)

test.each(DRAFT_OFF)(
  '2h.190, 2h.192: $name: every control of the draft group is natively disabled and its line model-profile-draft-group-off shows',
  async (c) => {
    const { main } = await form(c.values)
    expect(
      draftOn(c.values),
      `${c.name}: 190's own rule says the group is disabled`,
    ).toBe(false)
    for (const el of controlsOf(GROUPS[0]!))
      expect(el.disabled, `${el.id}: disabled (190)`).toBe(true)
    expect(
      offLine(main, DRAFT).textContent,
      '193: the string as the brief words it',
    ).toBe(OFF_DRAFT)
  },
)

test('2h.190: model-draft and spec-type are never sent, whatever is saved: the draft fields, a cache select and a ticked box go, the two keys do not', async () => {
  const values = read('ngram-mod,draft-mtp', {
    'model-draft': 'Qwen3-27B-DFlash-Q8_0.gguf',
  })
  const { bff, main } = await form(values)
  type(control('spec-draft-n-max'), '7')
  pick(main, DRAFT_K, 'f16')
  fireEvent.click(boxOf('spec-draft-n-ubatch'))
  expect(
    (control('spec-draft-n-ubatch') as HTMLInputElement).disabled,
    '156: a ticked box disables its input',
  ).toBe(true)
  await save(main, bff)
  const sent = sentValues(bff)
  expect(sent['spec-draft-n-max'], 'the draft field is sent').toBe('7')
  expect(sent['spec-draft-type-k'], 'the cache select is sent').toBe('f16')
  expect(sent['spec-draft-n-ubatch'], 'a ticked box sends null').toBeNull()
  expect('model-draft' in sent, '190: model-draft is never sent').toBe(false)
  expect('spec-type' in sent, '190: spec-type is never sent').toBe(false)
})

// ── 191, 192: the four n-gram groups ──────────────────────────────────

/** The `spec-type` reads, with the groups 191 enables (193's two cases first). */
const MATRIX: { specType: string; on: string[]; draft: boolean }[] = [
  {
    // 193: the brief's own read.
    specType: 'ngram-mod,draft-mtp',
    on: [MOD],
    draft: true,
  },
  { specType: MOD, on: [MOD], draft: false },
  { specType: MAP_K, on: [MAP_K], draft: false },
  { specType: MAP_K4V, on: [MAP_K4V], draft: false },
  {
    specType: ' ngram-simple , ngram-map-k ',
    on: [SIMPLE, MAP_K],
    draft: false,
  },
  { specType: 'ngram-cache,none,', on: [], draft: false },
  {
    specType: `draft-mtp,${MOD},${SIMPLE},${MAP_K},${MAP_K4V}`,
    on: [MOD, SIMPLE, MAP_K, MAP_K4V],
    draft: true,
  },
]

test.each(MATRIX)(
  '2h.191, 2h.192: spec-type "$specType": each n-gram group enabled with all its fields while listed and its line away, each other one disabled with its line, the draft group $draft',
  async (c) => {
    const { main } = await form(read(c.specType))
    for (const group of GROUPS.slice(1)) {
      const listed = c.on.includes(group.name)
      for (const el of controlsOf(group))
        expect(
          el.disabled,
          `${el.id}: ${listed ? 'enabled: listed (191)' : 'disabled: not listed (191)'}`,
        ).toBe(!listed)
      if (listed)
        expect(
          within(main).queryByText(offText(group.name)),
          `192: no line for the enabled ${group.name}`,
        ).toBeNull()
      else
        expect(
          offLine(main, group.name).textContent,
          `193: ${group.name}'s line names the group`,
        ).toBe(offText(group.name))
    }
    for (const el of controlsOf(GROUPS[0]!))
      expect(
        el.disabled,
        `the draft group is ${c.draft ? 'open' : 'locked'}`,
      ).toBe(!c.draft)
    if (c.draft)
      expect(
        within(main).queryByText(OFF_DRAFT),
        '192: no line for the enabled draft group',
      ).toBeNull()
    else offLine(main, DRAFT)
  },
)

test('2h.192: the reason line is a P inside the group role="group", right after its h4 (the division is the h4 next sibling, the line the division first element child, 2e 90a), before the group first field', async () => {
  const { main } = await form(read('ngram-mod'))
  for (const group of GROUPS) {
    const listed =
      group.name === DRAFT ? draftOn(read('ngram-mod')) : group.name === MOD
    if (listed) continue
    const g = groupOf(main, group.name)
    const line = offLine(main, group.name)
    expect(
      line.closest('[role="group"]'),
      `${group.name}: the line is inside the group division`,
    ).toBe(g)
    const h4 = headingOf(main, group.name)
    // 192 puts the line "right after its h4, inside the role="group" div".
    // 2e's 90a, pinned in ModelProfileCards5.test.tsx, is what makes that a pair of
    // pins and not one: the h4 is not inside the division it names (the
    // division names itself through aria-labelledby), it sits above it. So the
    // division is the element right after the h4, and the line the first
    // element inside it: in tree order the line follows the h4 at once.
    expect(
      h4.nextElementSibling,
      `${group.name}: 90a - the group division is right after the h4`,
    ).toBe(g)
    expect(
      g.firstElementChild,
      `${group.name}: 192 - the line is right after the h4, inside the division`,
    ).toBe(line)
    expect(
      line.tagName,
      `${group.name}: 192 - the line is a paragraph, not a span`,
    ).toBe('P')
    expect(before(h4, line), `${group.name}: the line is after the h4`).toBe(
      true,
    )
    const first = controlsOf(group)[0]!
    expect(
      before(line, first),
      `${group.name}: the line is before the first field`,
    ).toBe(true)
  }
  expect(
    within(groupOf(main, MOD)).queryByText(offNgram(MOD)),
    '192: the enabled ngram-mod group has no line',
  ).toBeNull()
})

test('2h.192: the lines are plain paragraphs: no role, no live region, and in no aria-describedby of the page', async () => {
  const { main } = await form(read(undefined))
  const texts: string[] = []
  for (const group of GROUPS) {
    const line = offLine(main, group.name)
    texts.push(line.textContent ?? '')
    expect(line.tagName, `${group.name}: a paragraph`).toBe('P')
    expect(line.getAttribute('role'), `${group.name}: no role`).toBeNull()
    expect(
      line.getAttribute('aria-live'),
      `${group.name}: not a live region`,
    ).toBeNull()
    expect(
      line.closest('[role="status"],[role="alert"],[role="log"],[aria-live]'),
      `${group.name}: not inside a live region`,
    ).toBeNull()
  }
  // Every element of the page that carries a description: controls, units,
  // hints, the bar, the headings. A reason line describes its group to the eye,
  // it is never the accessible description of anything (192, 159).
  const describedElements =
    document.querySelectorAll<HTMLElement>('[aria-describedby]')
  expect(
    describedElements.length,
    'the sweep has elements to check',
  ).toBeGreaterThan(0)
  for (const el of describedElements)
    for (const text of texts)
      expect(
        described(el),
        `${el.id || el.tagName}: the line "${text.slice(0, 20)}..." is not its description`,
      ).not.toContain(text)
})

// ── 193, 176: values {} and a locked group holding junk ───────────────

test('2h.190, 2h.176: a locked card with junk in its fields: no mark, nothing focused, and Save sends a PUT of the rest with none of its keys', async () => {
  // The card is locked by taking spec-type away, and its fields hold values far
  // out of table 157's bounds.
  const values: Record<string, string> = {
    ...SPEC,
    'spec-draft-n-max': '999999999999',
    'spec-draft-n-min': '-1',
    'spec-ngram-mod-n-match': '0',
    'spec-ngram-mod-n-min': '2000',
    'spec-ngram-simple-size-n': '1025',
    'spec-ngram-map-k4v-min-hits': '65536',
  }
  delete values['spec-type']
  const { bff, main } = await form(values)

  // All five groups are off, each with its line, every control natively
  // disabled, and the junk is not checked.
  const locked: HTMLElement[] = []
  for (const group of GROUPS) {
    offLine(main, group.name)
    for (const el of controlsOf(group)) {
      expect(
        el.disabled,
        `${el.id}: 193 - ${group.name} is disabled on values without spec-type`,
      ).toBe(true)
      if (el.tagName === 'INPUT' || el.tagName === 'SELECT') locked.push(el)
    }
  }
  for (const [key, text] of [
    ['spec-draft-n-max', MIN_ONE],
    ['spec-draft-n-min', MIN_ZERO],
    ['spec-ngram-mod-n-match', RANGE_1_1024],
    ['spec-ngram-mod-n-min', RANGE_0_1024],
    ['spec-ngram-simple-size-n', RANGE_1_1024],
    ['spec-ngram-map-k4v-min-hits', RANGE_1_65535],
  ] as [string, string][])
    expectNoError(control(key), text, `176: the locked ${key} is not checked`)
  expectInert(main, '193: a fully locked card is clean')

  // A field of another card, the Embeddings pooling select, makes the form
  // dirty. Nothing of the locked cards is touched: the Save that follows can
  // only be blocked by them. The focus is recorded before the save, so the
  // pin below says the save moved it nowhere (a page that focuses nothing on
  // load leaves it on the body, which is as valid a place to stay as the
  // page's own focus target).
  const focusBefore = document.activeElement
  pick(main, en('model-profile-pooling'), 'rank')
  await save(main, bff)
  const sent = sentValues(bff)
  // The key of the Embeddings card's Pooling row (2h.1's 133: "Pool `rank`
  // sends `{"pooling":"rank"}`"), the one control of another card this test
  // changes to make the form dirty.
  expect(sent['pooling'], '133: the field of the other card is sent').toBe(
    'rank',
  )

  // 176: the locked card sends none of its keys.
  for (const group of GROUPS)
    for (const f of group.fields)
      expect(
        f.key in sent,
        `176: the locked ${f.key} is not in the payload`,
      ).toBe(false)

  // 190, 176: nothing is marked, the focus did not move (176 sends it to the
  // one control that unlocks a field, and only when a changed draft of a
  // locked field is refused: no draft of this card is changed here), and the
  // bar says nothing.
  for (const el of locked)
    expect(
      el.getAttribute('aria-invalid'),
      `190: the locked ${el.id} is not marked`,
    ).toBeNull()
  expect(
    locked,
    '190: the focus did not start on a locked control, so the pin below is a real pin',
  ).not.toContain(document.activeElement)
  expect(document.activeElement, '190: the save did not move the focus').toBe(
    focusBefore,
  )
  expect(
    within(main).queryByText(en('save-invalid')),
    '190: the bar says nothing',
  ).toBeNull()
})

test('2h.193: values {} - the page is the blank form: all five groups disabled, each with its reason line, every box ticked, every input empty', async () => {
  // Today a value the form has no control for is kept as hidden data and sent
  // back unchanged. With no values at all there is nothing to keep, so the page
  // has to be the blank form of the rule.
  const { bff, main } = await form({})

  for (const group of GROUPS) {
    // The switch is off, and the group says so in the strings file's words.
    offLine(main, group.name)
    // Every control of the group is natively disabled, and none of them is a
    // required field: a disabled group cannot make the page invalid.
    for (const el of controlsOf(group)) {
      expect(el.disabled, `${group.name} ${el.id}`).toBe(true)
      expect(el.required, `${group.name} ${el.id}`).toBe(false)
    }
  }

  // Every box is ticked (176: an empty value means the box is on, so a ticked
  // box means its input is empty), and every input is empty.
  for (const group of GROUPS)
    for (const f of group.fields)
      if (f.kind !== 'select') {
        expect(boxOf(f.key).checked, f.key).toBe(true)
        expect(control(f.key).value, f.key).toBe('')
      }
  // The card's two selects, which have no box, are at their default. The two
  // keys of 190 that rule the card, `spec-type` and `model-draft`, are not
  // here: the card has no control for them (read, never sent, and their editor
  // is the Assets card of 2i), so the blank form has nothing of them to show.
  for (const key of ['spec-draft-type-k', 'spec-draft-type-v'])
    expect(control(key).value, key).toBe('default')

  // Nothing is inert, and nothing was sent.
  expectInert(main, '193: the blank form is clean')
  expect(bff.to('PUT', PUT), '193: nothing was sent').toHaveLength(0)
})

// ── 193: the checks of 157 and 165 on the card fields ─────────────────

test('2h.193, 2h.165: spec-draft-p-min typed 0,5 sends 0.5; typed hot is refused with field-error-decimal, marked, focused, save-invalid, nothing sent', async () => {
  const first = await form(read('draft-mtp'))
  type(control('spec-draft-p-min') as HTMLInputElement, '0,5')
  await save(first.main, first.bff)
  expect(sentValues(first.bff)['spec-draft-p-min'], '165: 0,5 sends 0.5').toBe(
    '0.5',
  )
  cleanup()
  const second = await form(read('draft-mtp'))
  type(control('spec-draft-p-min'), 'hot')
  fireEvent.click(saveButton(second.main))
  await within(second.main).findByText(en('save-invalid'))
  const el = control('spec-draft-p-min')
  expectError(el, DECIMAL, '165: spec-draft-p-min hot')
  expect(document.activeElement, 'focus on Draft p-min').toBe(el)
  expectAlert(second.main, en('save-invalid'))
  expect(second.bff.to('PUT', PUT), 'nothing sent').toHaveLength(0)
})

test('2h.193: spec-ngram-mod-n-min 2000 blocks with field-error-number 0, 1024, marked, focused, save-invalid, nothing sent, and 1024 is sent', async () => {
  const first = await form(read(MOD))
  type(control('spec-ngram-mod-n-min'), '2000')
  fireEvent.click(saveButton(first.main))
  await within(first.main).findByText(en('save-invalid'))
  const el = control('spec-ngram-mod-n-min')
  expectError(el, RANGE_0_1024, '193: spec-ngram-mod-n-min 2000')
  expect(document.activeElement, 'focus on Min draft').toBe(el)
  expectAlert(first.main, en('save-invalid'))
  expect(first.bff.to('PUT', PUT), 'nothing sent').toHaveLength(0)
  cleanup()
  const second = await form(read(MOD))
  type(control('spec-ngram-mod-n-min'), '1024')
  await save(second.main, second.bff)
  expect(sentValues(second.bff)['spec-ngram-mod-n-min'], '1024 is sent').toBe(
    '1024',
  )
})

/** 157's rows the card's keys sit in: each row's bounds tested on a key of it,
 * 1 to 1024 on the size key of each of the three n-gram groups that sits there. */
const RANGES: {
  key: string
  specType: string
  bad: string
  text: string
  good: string
}[] = [
  {
    key: 'spec-draft-n-max',
    specType: 'draft-mtp',
    bad: '0',
    text: MIN_ONE,
    good: '1',
  },
  {
    key: 'spec-draft-n-min',
    specType: 'draft-mtp',
    bad: '-1',
    text: MIN_ZERO,
    good: '0',
  },
  {
    // 157's "0 or more" row, second key of it on this card.
    key: 'spec-draft-n-ubatch',
    specType: 'draft-mtp',
    bad: '-1',
    text: MIN_ZERO,
    good: '0',
  },
  {
    key: 'spec-ngram-mod-n-match',
    specType: MOD,
    bad: '1025',
    text: RANGE_1_1024,
    good: '1',
  },
  {
    key: 'spec-ngram-mod-n-max',
    specType: MOD,
    bad: '-2',
    text: RANGE_0_1024,
    good: '0',
  },
  {
    // 157's "1 to 1024" row: the size key of each of the three n-gram groups
    // that sits in it, at each of its bounds.
    key: 'spec-ngram-simple-size-n',
    specType: SIMPLE,
    bad: '1025',
    text: RANGE_1_1024,
    good: '1024',
  },
  {
    // 157's "1 to 1024" row, the Lookup n-gram key of `ngram-map-k`: its minimum
    // is 1 like the other six size keys, so 0 is refused. The key sits on no
    // bound of any other case here, so without this row a minimum lowered to 0
    // on it is a rule nothing reads.
    key: 'spec-ngram-map-k-size-n',
    specType: MAP_K,
    bad: '0',
    text: RANGE_1_1024,
    good: '1',
  },
  {
    key: 'spec-ngram-map-k-size-m',
    specType: MAP_K,
    bad: '0',
    text: RANGE_1_1024,
    good: '1',
  },
  {
    key: 'spec-ngram-map-k4v-size-n',
    specType: MAP_K4V,
    bad: '1025',
    text: RANGE_1_1024,
    good: '1',
  },
  {
    key: 'spec-ngram-map-k4v-min-hits',
    specType: MAP_K4V,
    bad: '65536',
    text: RANGE_1_65535,
    good: '65535',
  },
]

test.each(RANGES)(
  '2h.157, 2h.133: $key refuses "$bad" with its row text, marks it, focuses it, alerts save-invalid, sends nothing, and "$good" is sent',
  async (c) => {
    const first = await form(read(c.specType))
    type(control(c.key), c.bad)
    fireEvent.click(saveButton(first.main))
    await within(first.main).findByText(en('save-invalid'))
    const el = control(c.key)
    expectError(el, c.text, `${c.key} ${c.bad}`)
    expect(document.activeElement, `focus on ${c.key}`).toBe(el)
    expectAlert(first.main, en('save-invalid'))
    expect(first.bff.to('PUT', PUT), 'nothing sent').toHaveLength(0)
    cleanup()
    const second = await form(read(c.specType))
    type(control(c.key), c.good)
    await save(second.main, second.bff)
    expect(sentValues(second.bff)[c.key], `${c.key} ${c.good} is sent`).toBe(
      c.good,
    )
  },
)

test('2h.133, 2h.157, 2h.165: every field of table 112 sends per its kind: each number its own text, a ticked box null, +0016 as 16, a cache select its type and default null', async () => {
  const { bff, main } = await form(
    read(`draft-mtp,${MOD},${SIMPLE},${MAP_K},${MAP_K4V}`),
  )
  const typed: Record<string, string> = {
    'spec-draft-n-max': '7',
    'spec-draft-p-min': '0,25',
    'spec-draft-n-min': '3',
    'spec-ngram-mod-n-match': '+0016',
    'spec-ngram-mod-n-min': '8',
    'spec-ngram-mod-n-max': '64',
    'spec-ngram-simple-size-n': '5',
    'spec-ngram-simple-size-m': '6',
    'spec-ngram-map-k-size-n': '7',
    'spec-ngram-map-k-size-m': '8',
    'spec-ngram-map-k4v-size-n': '9',
    'spec-ngram-map-k4v-size-m': '10',
    'spec-ngram-map-k4v-min-hits': '3',
  }
  for (const key of Object.keys(typed)) {
    const el = control(key) as HTMLInputElement
    expect(el.disabled, `${key}: enabled on this read`).toBe(false)
    type(el, typed[key]!)
  }
  fireEvent.click(boxOf('spec-draft-n-ubatch'))
  pick(main, DRAFT_K, 'f16')
  pick(main, DRAFT_V, 'default')
  // 133: the cache types are a select of words, the option list of 2e's 96, and
  // its first option is the default that sends null. Guarded here so the two
  // picks above cannot pass on a select that lists nothing.
  for (const label of [DRAFT_K, DRAFT_V]) {
    const el = within(main).getByRole('combobox', {
      name: label,
    }) as HTMLSelectElement
    expect(
      [...el.options].map((o) => o.value),
      `133: ${label} lists the cache types of 96`,
    ).toEqual(CACHE_VALUES)
    expect(
      el.options[0]?.textContent,
      `133: ${label}'s first option is the default`,
    ).toBe(DEFAULT)
  }
  await save(main, bff)
  expect(sentValues(bff)).toEqual({
    'spec-draft-n-max': '7',
    'spec-draft-p-min': '0.25',
    'spec-draft-n-min': '3',
    'spec-draft-n-ubatch': null,
    'spec-draft-type-k': 'f16',
    'spec-draft-type-v': null,
    'spec-ngram-mod-n-match': '16',
    'spec-ngram-mod-n-min': '8',
    'spec-ngram-mod-n-max': '64',
    'spec-ngram-simple-size-n': '5',
    'spec-ngram-simple-size-m': '6',
    'spec-ngram-map-k-size-n': '7',
    'spec-ngram-map-k-size-m': '8',
    'spec-ngram-map-k4v-size-n': '9',
    'spec-ngram-map-k4v-size-m': '10',
    'spec-ngram-map-k4v-min-hits': '3',
  })
})

// ── 134: the full-value line stays only under a disabled control ──────

test('2h.134, 2h.118: the draft group disabled, a long Draft K-cache type keeps its full-value line; the group enabled, the line is gone', async () => {
  await form(read(MOD, { 'spec-draft-type-k': SELECT_21 }))
  const el = control('spec-draft-type-k') as HTMLSelectElement
  expect(el.disabled, '190: no draft, the select is locked').toBe(true)
  const line = document.getElementById('model-profile-full-spec-draft-type-k')
  expect(
    line,
    '118: the long value is repeated in full under the control',
  ).not.toBeNull()
  expect(line!.textContent, '118: the line is the whole value').toBe(SELECT_21)
  expect(isHidden(line!), '118: the line is shown').toBe(false)
  cleanup()
  await form(read('draft-mtp', { 'spec-draft-type-k': SELECT_21 }))
  expect(
    (control('spec-draft-type-k') as HTMLSelectElement).disabled,
    '190: a drafter named, the select is open',
  ).toBe(false)
  expect(
    document.getElementById('model-profile-full-spec-draft-type-k'),
    '134: an open select shows no full-value line',
  ).toBeNull()
})

// ── 193: the two new strings ──────────────────────────────────────────

test('2h.193: the new strings model-profile-draft-group-off and model-profile-ngram-off are the briefs sentences, word for word', () => {
  expect(en('model-profile-draft-group-off')).toBe(OFF_DRAFT)
  expect(en('model-profile-ngram-off', 'ngram-mod')).toBe(offNgram('ngram-mod'))
  expect(t('model-profile-draft-group-off'), 'the fe strings file').toBe(
    OFF_DRAFT,
  )
  expect(
    t('model-profile-ngram-off', 'ngram-map-k4v'),
    'the fe strings file',
  ).toBe(offNgram('ngram-map-k4v'))
})
