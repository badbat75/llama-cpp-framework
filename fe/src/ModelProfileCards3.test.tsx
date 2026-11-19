// The two cards of 2e.3 on the model profile page (fe/briefs/02e-model-profile-cards.md,
// criteria 101 to 106): Chat / reasoning and Vision, tables of fields, plus
// what 2e.3 brings: the checkbox field (criterion 87's fourth kind), the two
// notes (90), and the device select of 103, which in 2e holds only the file's
// value because the device list arrives in 2f. The shared rules of section 1
// are pinned for the cards of 2e.1 in ModelProfileCards1.test.tsx and for those of
// 2e.2 in ModelProfileCards2.test.tsx; here they are pinned for what 2e.3 adds. The
// route and the read are ModelProfilePage.test.tsx, the Engine field and the Assets
// card are ModelProfileContent.test.tsx, the full-value line as a rule is
// ModelProfileFullValue.test.tsx (here only the case of 104). The colours of 85, the
// width of 91 and the muted style of a note of 90 are checked in review, not
// in jsdom.
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
  modelProfileDoc,
  reply,
  startAt,
  type BffOptions,
} from './test/bff.ts'
import {
  cardHeadings,
  cardHeadingKeys,
  expectedInputMode,
  fieldTabStops,
  pageStartTabStops,
  saveBarTabStops,
} from './test/modelProfileCards.ts'

const ID = 'Qwen3-27B'
const HASH = `#/profiles/model/${ID}`
const MODEL = '/var/lib/private-ai/.llama.cpp/models/Qwen3-27B-Q8_0.gguf'

/** The criterion 104 stub: every key of 101 and 102 set. */
const SET: Record<string, string> = {
  jinja: 'false',
  'prefill-assistant': 'false',
  reasoning: 'off',
  'reasoning-format': 'deepseek',
  'reasoning-effort': 'xhigh',
  'reasoning-preserve': 'false',
  'reasoning-budget': '-1',
  'reasoning-budget-message': 'Wrap up now.',
  'n-predict': '8192',
  'mmproj-offload': 'false',
  'mmproj-device': 'ROCm1',
  'image-min-tokens': '1024',
  'image-max-tokens': '2048',
}

/** The shared option texts of section 1.3. */
const TRISTATE = [en('option-default'), en('option-on'), en('option-off')]

/** The Reasoning select of 101: auto / on / off. */
const REASONING = [en('option-auto'), en('option-on'), en('option-off')]

/** The Reasoning format select of 101: `deepseek` is a literal. */
const REASONING_FORMAT = [en('option-auto'), en('option-none'), 'deepseek']

/** The reasoning-effort options of 101 (MP 917-918). */
const EFFORT = [
  en('option-default'),
  en('option-minimal'),
  en('option-low'),
  en('option-medium'),
  en('option-high'),
  en('option-extra-high'),
  en('option-maximum'),
]
const EFFORT_VALUES = [
  'default',
  'minimal',
  'low',
  'medium',
  'high',
  'xhigh',
  'max',
]

/** The empty option of a device select (103). */
const FOLLOW = en('model-profile-device-follows-model')

/** One row of the field tables of 101 and 102. */
type Row = {
  /** The strings key of the card's h3. */
  card: string
  /** The key of the section's `values`. */
  key: string
  /** The label as the page shows it: the strings text of 105. */
  label: string
  /** The strings key of the field's hint (criterion 88). */
  hint: string
  kind: 'checkbox' | 'select' | 'number' | 'text'
  /** A number: an integer key takes the numeric input form (87). */
  integer?: boolean
  /** A checkbox: the strings key of the toggle's text (87). */
  toggle?: string
  /** A select's option texts, in the table's order. */
  options?: string[]
  /** A select's option values, where the table gives them. */
  optionValues?: string[]
  /** A select: the option text it shows when the key is absent. */
  absentOption?: string
  /** A checkbox: the state its key absent shows (101, 102). */
  absentChecked?: boolean
  /** The value of the criterion 104 stub. */
  set: string
  /** What the field shows for it: the input's value, the selected option's
   * text, or the checkbox's state as a word. */
  shown: string
  shownChecked?: boolean
}

/** The field tables of 101 and 102, in the cards' and the GUI's order. */
const FIELDS: Row[] = [
  {
    card: 'model-profile-chat-reasoning',
    key: 'jinja',
    label: en('model-profile-chat-template'),
    hint: 'model-profile-chat-template-hint',
    kind: 'checkbox',
    toggle: 'model-profile-chat-template-toggle',
    absentChecked: true,
    set: 'false',
    shown: 'unchecked',
    shownChecked: false,
  },
  {
    card: 'model-profile-chat-reasoning',
    key: 'prefill-assistant',
    label: en('model-profile-prefill-assistant'),
    hint: 'model-profile-prefill-assistant-hint',
    kind: 'checkbox',
    toggle: 'model-profile-prefill-assistant-toggle',
    absentChecked: true,
    set: 'false',
    shown: 'unchecked',
    shownChecked: false,
  },
  {
    card: 'model-profile-chat-reasoning',
    key: 'reasoning',
    label: en('model-profile-reasoning'),
    hint: 'model-profile-reasoning-hint',
    kind: 'select',
    options: REASONING,
    optionValues: ['auto', 'on', 'off'],
    absentOption: en('option-auto'),
    set: 'off',
    shown: en('option-off'),
  },
  {
    card: 'model-profile-chat-reasoning',
    key: 'reasoning-format',
    label: en('model-profile-reasoning-format'),
    hint: 'model-profile-reasoning-format-hint',
    kind: 'select',
    options: REASONING_FORMAT,
    optionValues: ['auto', 'none', 'deepseek'],
    absentOption: en('option-auto'),
    set: 'deepseek',
    shown: 'deepseek',
  },
  {
    card: 'model-profile-chat-reasoning',
    key: 'reasoning-effort',
    label: en('model-profile-reasoning-effort'),
    hint: 'model-profile-reasoning-effort-hint',
    kind: 'select',
    options: EFFORT,
    optionValues: EFFORT_VALUES,
    absentOption: en('option-default'),
    set: 'xhigh',
    shown: en('option-extra-high'),
  },
  {
    card: 'model-profile-chat-reasoning',
    key: 'reasoning-preserve',
    label: en('model-profile-keep-past-thinking'),
    hint: 'model-profile-keep-past-thinking-hint',
    kind: 'select',
    options: TRISTATE,
    absentOption: en('option-default'),
    set: 'false',
    shown: en('option-off'),
  },
  {
    card: 'model-profile-chat-reasoning',
    key: 'reasoning-budget',
    label: en('model-profile-thinking-budget'),
    hint: 'model-profile-thinking-budget-hint',
    kind: 'number',
    integer: true,
    set: '-1',
    shown: '-1',
  },
  {
    card: 'model-profile-chat-reasoning',
    key: 'reasoning-budget-message',
    label: en('model-profile-budget-message'),
    hint: 'model-profile-budget-message-hint',
    kind: 'text',
    set: 'Wrap up now.',
    shown: 'Wrap up now.',
  },
  {
    card: 'model-profile-chat-reasoning',
    key: 'n-predict',
    label: en('model-profile-max-output'),
    hint: 'model-profile-max-output-hint',
    kind: 'number',
    integer: true,
    set: '8192',
    shown: '8192',
  },
  {
    card: 'model-profile-vision',
    key: 'mmproj-offload',
    label: en('model-profile-vision-on-gpu'),
    hint: 'model-profile-vision-on-gpu-hint',
    kind: 'checkbox',
    toggle: 'model-profile-vision-offload-toggle',
    absentChecked: true,
    set: 'false',
    shown: 'unchecked',
    shownChecked: false,
  },
  {
    card: 'model-profile-vision',
    key: 'mmproj-device',
    label: en('model-profile-vision-gpu'),
    hint: 'model-profile-vision-gpu-hint',
    kind: 'select',
    options: [FOLLOW, 'ROCm1'],
    optionValues: ['', 'ROCm1'],
    absentOption: FOLLOW,
    set: 'ROCm1',
    shown: 'ROCm1',
  },
  {
    card: 'model-profile-vision',
    key: 'image-min-tokens',
    label: en('model-profile-min-image-tokens'),
    hint: 'model-profile-min-image-tokens-hint',
    kind: 'number',
    integer: true,
    set: '1024',
    shown: '1024',
  },
  {
    card: 'model-profile-vision',
    key: 'image-max-tokens',
    label: en('model-profile-max-image-tokens'),
    hint: 'model-profile-max-image-tokens-hint',
    kind: 'number',
    integer: true,
    set: '2048',
    shown: '2048',
  },
]

/** The two cards of 2e.3. */
const CARDS = ['model-profile-chat-reasoning', 'model-profile-vision']

/** The cards of 2e.3 that 2h.170 made editable: every control of these is
 * enabled (a number's input while its default box is unticked, 156), every
 * control of a card not listed stays natively disabled (2e.85). */
const EDITABLE_CARDS = ['model-profile-chat-reasoning']

/** The notes inside the two cards (90): the card, the note's strings key, and
 * the labels of the fields each note sits between (MP 933-938, 1099-1104). */
const NOTES = [
  [
    'model-profile-chat-reasoning',
    'model-profile-output-budget-note',
    en('model-profile-keep-past-thinking'),
    en('model-profile-thinking-budget'),
  ],
  [
    'model-profile-vision',
    'model-profile-image-tokens-note',
    en('model-profile-vision-gpu'),
    en('model-profile-min-image-tokens'),
  ],
] as const

/** The 41 new strings of 105. */
const NEW_STRINGS = [
  'model-profile-chat-reasoning',
  'model-profile-chat-template',
  'model-profile-chat-template-hint',
  'model-profile-chat-template-toggle',
  'model-profile-prefill-assistant',
  'model-profile-prefill-assistant-hint',
  'model-profile-prefill-assistant-toggle',
  'model-profile-reasoning',
  'model-profile-reasoning-hint',
  'model-profile-reasoning-format',
  'model-profile-reasoning-format-hint',
  'model-profile-reasoning-effort',
  'model-profile-reasoning-effort-hint',
  'model-profile-keep-past-thinking',
  'model-profile-keep-past-thinking-hint',
  'model-profile-thinking-budget',
  'model-profile-thinking-budget-hint',
  'model-profile-budget-message',
  'model-profile-budget-message-hint',
  'model-profile-max-output',
  'model-profile-max-output-hint',
  'model-profile-output-budget-note',
  'option-auto',
  'option-minimal',
  'option-low',
  'option-medium',
  'option-high',
  'option-extra-high',
  'option-maximum',
  'model-profile-vision',
  'model-profile-vision-on-gpu',
  'model-profile-vision-on-gpu-hint',
  'model-profile-vision-offload-toggle',
  'model-profile-vision-gpu',
  'model-profile-vision-gpu-hint',
  'model-profile-image-tokens-note',
  'model-profile-min-image-tokens',
  'model-profile-min-image-tokens-hint',
  'model-profile-max-image-tokens',
  'model-profile-max-image-tokens-hint',
  'model-profile-device-follows-model',
]

/** The controls 101 and 102 put in the two cards: a box and an input per
 * number, one control per checkbox, select and text. */
const CONTROLS = FIELDS.reduce((n, f) => n + (f.kind === 'number' ? 2 : 1), 0)

beforeEach(() => startAt(HASH))

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
  startAt('')
})

// ── Helpers (as ModelProfileCards2.test.tsx) ──────────────────────────────────

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

/** A read whose `values` are exactly `values` (the `{}` stub of 104). */
function exactOf(values: Record<string, string>) {
  return modelProfileDoc({}, { values })
}

function settingsTitle(main: HTMLElement): HTMLElement {
  return within(main).getByRole('heading', {
    level: 2,
    name: en('model-profile-settings-title'),
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

/** The field that holds the About toggle of `label` (Field.tsx `.field`). */
function fieldOf(main: HTMLElement, label: string): HTMLElement {
  const field = aboutOf(main, label).closest('.field')
  expect(field, `${label}: its Field wrapper`).toBeTruthy()
  return field as HTMLElement
}

/** A number or checkbox field: the group named by the label (87). */
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

/** A checkbox field's checkbox: named by the table's toggle text (87). */
function checkBoxOf(main: HTMLElement, f: Row): HTMLInputElement {
  return within(groupBox(main, f.label)).getByRole('checkbox', {
    name: en(f.toggle ?? f.key),
  }) as HTMLInputElement
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

/** The selected option's text exactly as the page writes it: no trimming, for
 * a value the page must show as the file holds it (87). */
function selectedTextExact(select: HTMLSelectElement): string {
  expect(select.selectedIndex, 'an option is selected').toBeGreaterThanOrEqual(
    0,
  )
  return optionsOf(select)[select.selectedIndex]!.textContent ?? ''
}

function text(el: Element): string {
  return (el.textContent ?? '').replace(/\s+/g, ' ').trim()
}

/** The elements Tab reaches inside `main`, in DOM order (criterion 91). */
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

/** The control of a row's field, by its kind. */
function controlOf(main: HTMLElement, f: Row): HTMLElement {
  if (f.kind === 'number') return inputOf(groupBox(main, f.label), f.label)
  if (f.kind === 'checkbox') return checkBoxOf(main, f)
  if (f.kind === 'select') return selectOf(main, f.label)
  return inputOfField(main, f.label)
}

/** What a select shows when its key is absent: the table's absent option. */
function expectAbsentSelect(main: HTMLElement, f: Row): void {
  expect(
    selectedText(selectOf(main, f.label)),
    `${f.key}: absent shows ${f.absentOption}`,
  ).toBe(f.absentOption)
}

/** What a field shows when its key is absent, whatever its kind. */
function expectAbsent(main: HTMLElement, f: Row): void {
  if (f.kind === 'number') {
    const group = groupBox(main, f.label)
    expect(boxOf(group).checked, `${f.key}: the default box ticked`).toBe(true)
    expect(inputOf(group, f.label).value, `${f.key}: an empty input`).toBe('')
  } else if (f.kind === 'checkbox') {
    expect(checkBoxOf(main, f).checked, `${f.key}: absent checked`).toBe(
      f.absentChecked,
    )
  } else if (f.kind === 'select') {
    expectAbsentSelect(main, f)
  } else {
    const input = inputOfField(main, f.label)
    expect(input.value, `${f.key}: an empty input`).toBe('')
    expect(
      input.getAttribute('placeholder') ?? '',
      `${f.key}: no placeholder in 2e`,
    ).toBe('')
  }
}

/** Every note paragraph of a card: a paragraph not inside a field (90). */
function notesOf(card: HTMLElement): HTMLElement[] {
  return [...card.querySelectorAll('p')].filter(
    (p) => p.closest('.field') === null,
  )
}

// ── One test per field (85, 86, 87, 88, 101, 102, 104) ─────────────────

test.each(FIELDS)(
  '2e.101, 2e.102, 2e.87, 2h.158, 2e.85, 2h.170: $key is a $kind named by $label inside its card, shows the set value $set as the reply holds it, its control enabled on the editable card and natively disabled on the read-only one',
  async (f) => {
    const { main } = await loaded({
      modelProfile: () => reply(200, docOf(SET)),
    })
    // 2h.170: the Chat / reasoning card is editable, every field always
    // enabled; a card this file's rows do not list stays read-only, so the
    // table also pins the natively disabled control (2e.85).
    const editable = EDITABLE_CARDS.includes(f.card)
    const field = fieldOf(main, f.label)
    expect(
      cardOf(main, f.card).contains(field),
      `${f.key}: its field is inside the ${en(f.card)} card`,
    ).toBe(true)
    // 89: a unit sits after the input, only where the table gives one: 101 and
    // 102 give none.
    const units = [...field.querySelectorAll('.unit')].map((u) => text(u))
    expect(units, `${f.key}: no unit`).toEqual([])
    if (f.kind === 'number') {
      const group = groupBox(main, f.label)
      const box = boxOf(group)
      const input = inputOf(group, f.label)
      expect(input.value, `${f.key}: the string of the file exactly`).toBe(
        f.shown,
      )
      expect(box.checked, `${f.key}: the default box unticked`).toBe(false)
      expect(
        box.disabled,
        `${f.key}: the box ${editable ? 'enabled' : 'disabled'}`,
      ).toBe(!editable)
      // 156: on the editable card the input follows the unticked box, so it is
      // enabled and can scroll; a ticked box disables it, pinned in
      // ModelProfileSave11.test.tsx.
      expect(
        input.disabled,
        `${f.key}: the input ${editable ? 'enabled' : 'disabled'}`,
      ).toBe(!editable)
      // 87 as retargeted by 2h.158: every number of 101 and 102 is an
      // integer key; the ranged ones take the numeric form, the
      // any-whole-number ones none.
      expect(
        input.getAttribute('inputmode'),
        `${f.key}: ${expectedInputMode(f.key, f.integer) ?? 'plain'}`,
      ).toBe(expectedInputMode(f.key, f.integer))
    } else if (f.kind === 'checkbox') {
      const box = checkBoxOf(main, f)
      expect(box.checked, `${f.key}: ${f.set} shows ${f.shown}`).toBe(
        f.shownChecked,
      )
      expect(
        box.disabled,
        `${f.key}: the checkbox ${editable ? 'enabled' : 'disabled'}`,
      ).toBe(!editable)
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
      expect(
        select.disabled,
        `${f.key}: the select ${editable ? 'enabled' : 'disabled'}`,
      ).toBe(!editable)
    } else {
      const input = inputOfField(main, f.label)
      expect(input.value, `${f.key}: the string of the file exactly`).toBe(
        f.shown,
      )
      expect(
        input.disabled,
        `${f.key}: the input ${editable ? 'enabled' : 'disabled'}`,
      ).toBe(!editable)
      expect(
        input.getAttribute('placeholder') ?? '',
        `${f.key}: no placeholder in 2e`,
      ).toBe('')
    }
    // 85: the native disabled attribute, never aria-disabled, never readonly,
    // on the read-only card and its mirror on the editable one (2h.170).
    for (const el of [
      controlOf(main, f),
      ...(f.kind === 'number'
        ? [groupBox(main, f.label).querySelector('input') as Element]
        : []),
    ]) {
      expect(
        (el as HTMLInputElement).disabled,
        `${f.key}: ${el.tagName} ${editable ? 'enabled' : 'disabled'}`,
      ).toBe(!editable)
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
  '2e.86, 2e.104: $key is absent from a values of {} so its field shows the absent form of $kind',
  async (f) => {
    const { main } = await loaded({
      modelProfile: () => reply(200, exactOf({})),
    })
    expectAbsent(main, f)
  },
)

test.each(FIELDS)(
  '2e.88: $key has an About toggle named About its label, with aria-expanded and aria-controls, and its hint opens as one paragraph per line of the GUI text and closes again',
  async (f) => {
    const { main } = await loaded({
      modelProfile: () => reply(200, docOf(SET)),
    })
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

test.each(FIELDS.filter((f) => f.key !== 'reasoning-preserve'))(
  '2e.86, 2e.104: a key of 101 or 102 whose value is the empty string reads as absent, for every key of 2e.3 that is not a flag (2e.3b: a flag reads "" as on, criterion 123): $key',
  async (f) => {
    const { main } = await loaded({
      modelProfile: () => reply(200, exactOf({ ...SET, [f.key]: '' })),
    })
    expectAbsent(main, f)
  },
)

test('2e.86, 2e.81: a key the page does not show adds no field, so the two cards of 2e.3 hold exactly the fields of 101 and 102 in the table order', async () => {
  const { main } = await loaded({
    modelProfile: () =>
      reply(
        200,
        docOf({
          ...SET,
          'n-cpu-moe': 'zz-secret',
          faqs: 'zz-secret',
          'reasoning-budget-message-extra': 'zz-secret',
        }),
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

// ── The checkbox field, the kind 2e.3 brings (87, 101, 102, 104) ───────

/** The checkbox rows of the 101 and 102 tables: the three flags of 2e.3. */
const CHECKBOXES = FIELDS.filter((f) => f.kind === 'checkbox')

/** 2e.3b (criterion 122): a flag is off for exactly the falsey words, and on
 * for every other value, "" included. The GUI's reading (form.rs 258, 296,
 * 160: unwrap_or(true)) holds for a value that is not falsey. */
const FLAG_READS = [
  [undefined, true],
  ['false', false],
  [' true ', true],
  [' false ', false],
  ['true', true],
  ['yes', true],
  ['True', true],
  ['False', true],
  ['0', false],
  [' ', true],
] as [string | undefined, boolean][]

/** One case per checkbox field and value to read: 2h.170 makes the Chat /
 * reasoning boxes enabled and leaves the Vision one natively disabled (2e.85),
 * so each case carries its own `editable`. One mount per case: the whole page
 * is heavy, and a test that mounts it scores of times overruns the default
 * timeout when the suite runs the files in parallel. */
const CHECKBOX_CASES = CHECKBOXES.flatMap((f) =>
  FLAG_READS.map(([value, checked]) => ({
    key: f.key,
    row: f,
    value,
    checked,
    editable: EDITABLE_CARDS.includes(f.card),
  })),
)

test('2e.87, 2e.101, 2e.102: the checkbox fields of the 101 and 102 tables are exactly these three', () => {
  expect(
    CHECKBOXES.map((f) => f.key),
    'the checkbox fields of 101 and 102',
  ).toEqual(['jinja', 'prefill-assistant', 'mmproj-offload'])
})

test.each(CHECKBOX_CASES)(
  '2e.87, 2e.101, 2e.102, 2e.122, 2h.170: $key is a group named by its label holding one checkbox whose text is the table toggle, $value is checked $checked, and its control is enabled on the editable card and natively disabled on the read-only one',
  async ({ row: f, value, checked, editable }) => {
    const { main } = await loaded({
      modelProfile: () =>
        reply(200, exactOf(value === undefined ? {} : { [f.key]: value })),
    })
    const group = groupBox(main, f.label)
    expect(
      group.classList.contains('field'),
      `${f.key}: the group is the Field`,
    ).toBe(true)
    expect(
      group.querySelectorAll('input').length,
      `${f.key}: one control in the group`,
    ).toBe(1)
    const box = checkBoxOf(main, f)
    expect(text(box.closest('label')!), `${f.key}: the toggle text`).toBe(
      en(f.toggle ?? ''),
    )
    expect(box.checked, `${f.key} ${JSON.stringify(value)}: checked`).toBe(
      checked,
    )
    expect(
      box.disabled,
      `${f.key}: the checkbox ${editable ? 'enabled' : 'disabled'}`,
    ).toBe(!editable)
    expect(
      box.getAttribute('aria-disabled'),
      `${f.key}: never aria-disabled`,
    ).toBeNull()
    expect(box.getAttribute('readonly'), `${f.key}: never readonly`).toBeNull()
  },
)

test('2e.104: the stub of 104 shows as the tables say, every key set: the three checkboxes unchecked and mmproj-device as the file writes it', async () => {
  const of_ = (key: string) => FIELDS.find((f) => f.key === key)!
  // Every key set.
  const { main } = await loaded({
    modelProfile: () => reply(200, exactOf(SET)),
  })
  for (const key of ['jinja', 'prefill-assistant', 'mmproj-offload'])
    expect(checkBoxOf(main, of_(key)).checked, `${key} false: unchecked`).toBe(
      false,
    )
  expect(
    selectedTextExact(selectOf(main, en('model-profile-vision-gpu'))),
    'mmproj-device ROCm1: the device as the file writes it',
  ).toBe('ROCm1')
})

test('2e.104: the stub of 104 shows as the tables say, values {}: every checkbox checked', async () => {
  const of_ = (key: string) => FIELDS.find((f) => f.key === key)!
  // values {}: every checkbox checked.
  const { main: empty } = await loaded({
    modelProfile: () => reply(200, exactOf({})),
  })
  for (const key of ['jinja', 'prefill-assistant', 'mmproj-offload'])
    expect(checkBoxOf(empty, of_(key)).checked, `${key}: absent checked`).toBe(
      true,
    )
})

test('2e.104: the stub of 104 shows as the tables say, jinja yes: checked as the GUI reads it', async () => {
  const of_ = (key: string) => FIELDS.find((f) => f.key === key)!
  // jinja yes: checked, as the GUI.
  const { main: yes } = await loaded({
    modelProfile: () => reply(200, exactOf({ jinja: 'yes' })),
  })
  expect(
    checkBoxOf(yes, of_('jinja')).checked,
    'jinja yes: checked as the GUI reads it',
  ).toBe(true)
})

test('2e.104: the stub of 104 shows as the tables say, reasoning maybe: one extra option carrying it', async () => {
  // reasoning maybe: one extra option carrying it.
  const { main: maybe } = await loaded({
    modelProfile: () => reply(200, exactOf({ reasoning: 'maybe' })),
  })
  const select = selectOf(maybe, en('model-profile-reasoning'))
  const opts = optionsOf(select)
  expect(opts.length, 'reasoning maybe: the three options plus one').toBe(
    REASONING.length + 1,
  )
  expect(
    optionTexts(select).slice(0, REASONING.length),
    'reasoning maybe: the listed options keep their place',
  ).toEqual(REASONING)
  const extra = opts[opts.length - 1]!
  expect(
    extra.value,
    'reasoning maybe: the extra option carries the value',
  ).toBe('maybe')
  expect(extra.textContent, 'reasoning maybe: and its text, as written').toBe(
    'maybe',
  )
  expect(
    selectedTextExact(select),
    'reasoning maybe: the extra option is selected',
  ).toBe('maybe')
})

// ── The selects of 101 (87, 101) ────────────────────────────────────────

test('2e.101: Reasoning lists auto / on / off and Reasoning format lists auto, none and the literal deepseek, each option carrying its own value, and each last value selects its own option', async () => {
  for (const [key, label, options, values, shown] of [
    [
      'reasoning',
      en('model-profile-reasoning'),
      REASONING,
      ['auto', 'on', 'off'],
      en('option-off'),
    ],
    [
      'reasoning-format',
      en('model-profile-reasoning-format'),
      REASONING_FORMAT,
      ['auto', 'none', 'deepseek'],
      'deepseek',
    ],
  ] as [string, string, string[], string[], string][]) {
    const last = values[values.length - 1]!
    const { main } = await loaded({
      modelProfile: () => reply(200, exactOf({ [key]: last })),
    })
    const select = selectOf(main, label)
    expect(optionTexts(select), `${key}: the options of 101`).toEqual(options)
    expect(
      optionsOf(select).map((o) => o.value),
      `${key}: each option carries the value the file holds`,
    ).toEqual(values)
    expect(selectedText(select), `${key}: ${last} selects its own option`).toBe(
      shown,
    )
    cleanup()
    vi.unstubAllGlobals()
  }
})

test('2e.101, 2e.87: a value the mapping of Reasoning format does not list is one extra last option carrying it as its value and its text, selected, and is not read as auto as the GUI reads it', async () => {
  const { main } = await loaded({
    modelProfile: () =>
      reply(200, exactOf({ 'reasoning-format': 'deepseekR1' })),
  })
  const select = selectOf(main, en('model-profile-reasoning-format'))
  const opts = optionsOf(select)
  expect(opts.length, 'an unlisted format: the three options plus one').toBe(
    REASONING_FORMAT.length + 1,
  )
  expect(
    optionTexts(select).slice(0, REASONING_FORMAT.length),
    'an unlisted format: the listed options keep their place',
  ).toEqual(REASONING_FORMAT)
  const extra = opts[opts.length - 1]!
  expect(extra.value, 'the extra option carries the value').toBe('deepseekR1')
  expect(extra.textContent, 'and its text').toBe('deepseekR1')
  expect(
    selectedTextExact(select),
    'the extra option is selected, not auto',
  ).toBe('deepseekR1')
})

test('2e.101: Reasoning effort lists the seven reasoning-effort options of MP 917-918, in order and with the file values, and xhigh selects its own option', async () => {
  const label = en('model-profile-reasoning-effort')
  const { main } = await loaded({
    modelProfile: () => reply(200, exactOf({ 'reasoning-effort': 'xhigh' })),
  })
  const select = selectOf(main, label)
  expect(optionTexts(select), 'the seven effort texts of 105').toEqual(EFFORT)
  expect(
    optionsOf(select).map((o) => o.value),
    'each option carries the value the file holds',
  ).toEqual(EFFORT_VALUES)
  expect(selectedText(select), 'xhigh selects its own option').toBe(
    en('option-extra-high'),
  )
})

test.each(EFFORT_VALUES)(
  '2e.101: each listed Reasoning effort value selects its own option: %s',
  async (value) => {
    const label = en('model-profile-reasoning-effort')
    const { main } = await loaded({
      modelProfile: () => reply(200, exactOf({ 'reasoning-effort': value })),
    })
    expect(selectedTextExact(selectOf(main, label)), `${value}: its own`).toBe(
      EFFORT[EFFORT_VALUES.indexOf(value)],
    )
  },
)

test.each([
  ['true', en('option-on')],
  ['false', en('option-off')],
] as [string, string][])(
  '2e.101, 2e.1.3, 2e.122, 2e.125: Keep past thinking is the shared tri-state of 1.3: default, on, off; %s shows %s',
  async (value, shown) => {
    const f = FIELDS.find((x) => x.key === 'reasoning-preserve')!
    const { main } = await loaded({
      modelProfile: () => reply(200, exactOf({ [f.key]: value })),
    })
    const select = selectOf(main, f.label)
    expect(optionTexts(select), `${f.key}: the shared options of 1.3`).toEqual(
      TRISTATE,
    )
    expect(selectedText(select), `${f.key}: ${value} shows ${shown}`).toBe(
      shown,
    )
  },
)

test('2e.101, 2e.1.3, 2e.122, 2e.125: Keep past thinking: a flag value the mapping does not list is on with no extra option (2e.3b)', async () => {
  const f = FIELDS.find((x) => x.key === 'reasoning-preserve')!
  const { main } = await loaded({
    modelProfile: () => reply(200, exactOf({ [f.key]: 'yes' })),
  })
  const select = selectOf(main, f.label)
  expect(
    optionTexts(select),
    `${f.key}: a flag select keeps exactly the options of 1.3`,
  ).toEqual(TRISTATE)
  expect(selectedText(select), `${f.key}: yes is on, not an extra option`).toBe(
    en('option-on'),
  )
})

test('2e.101, 2e.1.3, 2e.122, 2e.125: Keep past thinking absent shows default', async () => {
  const f = FIELDS.find((x) => x.key === 'reasoning-preserve')!
  const { main: absent } = await loaded({
    modelProfile: () => reply(200, exactOf({})),
  })
  expectAbsentSelect(absent, f)
})

// ── The device select of 103 ────────────────────────────────────────────

test('2e.103: Vision GPU absent holds only the empty option, whose value is the empty string and whose text is model-profile-device-follows-model, selected; the page lists no device, it has no device list in 2e', async () => {
  const { main } = await loaded({ modelProfile: () => reply(200, exactOf({})) })
  const select = selectOf(main, en('model-profile-vision-gpu'))
  const opts = optionsOf(select)
  expect(opts.length, 'absent: only the empty option').toBe(1)
  expect(opts[0]!.value, 'the empty option carries the empty value').toBe('')
  expect(opts[0]!.textContent, 'and the text of 105').toBe(FOLLOW)
  expect(select.selectedIndex, 'the empty option is selected').toBe(0)
  expect(select.disabled, 'the select disabled').toBe(true)
})

test.each(['ROCm1', 'CUDA0', ' ROCm1 ', 'cpu'])(
  '2e.103: Vision GPU present keeps the empty option first and adds one option whose value and text are the reply string as written, selected: no device of a list, no (custom) marker: %s',
  async (value) => {
    const { main } = await loaded({
      modelProfile: () => reply(200, exactOf({ 'mmproj-device': value })),
    })
    const select = selectOf(main, en('model-profile-vision-gpu'))
    const opts = optionsOf(select)
    expect(opts.length, `${value}: the empty option plus one`).toBe(2)
    expect(opts[0]!.value, `${value}: the empty option stays first`).toBe('')
    expect(opts[0]!.textContent, `${value}: and its text`).toBe(FOLLOW)
    expect(
      opts[1]!.value,
      `${value}: the option carries the file's value`,
    ).toBe(value)
    expect(opts[1]!.textContent, `${value}: and its text, as written`).toBe(
      value,
    )
    expect(
      selectedTextExact(select),
      `${value}: the file's device is the selected one`,
    ).toBe(value)
    expect(
      optionTexts(select).join(' '),
      `${value}: no (custom) marker on the data`,
    ).not.toContain('(custom)')
  },
)

// ── The notes inside the cards (90) ─────────────────────────────────────

test('2e.90: the two notes of 2e.3 are paragraphs of their card between the fields the table names, in the muted note style, and are no field: no About toggle; and the two cards hold no group, so no h4 and no role=group division', async () => {
  const { main } = await loaded({ modelProfile: () => reply(200, docOf(SET)) })
  for (const [cardKey, noteKey, afterLabel, beforeLabel] of NOTES) {
    const card = cardOf(main, cardKey)
    // 2h.176: the SET stub has no mmproj, so the Vision card opens with its
    // model-profile-vision-off paragraph right after the h3, before the first field
    // (the paragraph is pinned by 176 and 181 in ModelProfileSave12.test.tsx). The
    // notes of 90 sit between fields, so a 90 note is a card paragraph that
    // starts after the card's first field; the intro is not one of them.
    const firstField = card.querySelector('.field')
    const notes = notesOf(card).filter(
      (p) => firstField !== null && before(firstField, p),
    )
    expect(notes.length, `${en(cardKey)}: one note between the fields`).toBe(1)
    const note = notes[0]!
    expect(note.tagName, `${en(cardKey)}: the note is a paragraph`).toBe('P')
    expect(text(note), `${noteKey}: the text of 105, unchanged`).toBe(
      en(noteKey),
    )
    // The muted style of 90 is the note class (as the card's intro is drawn).
    expect(note.className, `${noteKey}: the note style`).toContain('note')
    expect(
      note.closest('.field'),
      `${noteKey}: the note is not inside a field`,
    ).toBeNull()
    // The GUI place: after the field named, before the field named.
    expect(
      before(fieldOf(main, afterLabel), note),
      `${afterLabel} before ${noteKey}`,
    ).toBe(true)
    expect(
      before(note, fieldOf(main, beforeLabel)),
      `${noteKey} before ${beforeLabel}`,
    ).toBe(true)
    // Not a field: nothing to open for it.
    expect(
      within(card).queryByRole('button', {
        name: `${en('field-about')} ${en(noteKey)}`,
      }),
      `${noteKey}: no About toggle for a note`,
    ).toBeNull()
  }
  // 90: a group of a card is an h4 and a role=group division named by it; 101
  // and 102 have none (the groups arrive with Lookup n-gram, 2e.5).
  for (const cardKey of CARDS) {
    const card = cardOf(main, cardKey)
    for (const sel of ['h4', '[role="group"]'])
      expect(
        card.querySelectorAll(sel).length,
        `${en(cardKey)}: no ${sel}`,
      ).toBe(0)
  }
})

// ── Values shown exactly, the full-value line of 104 (87, 118, 119) ─────

test('2e.87, 2e.104: a value of 2e.3 shows exactly as the reply holds it, with no parsing, no trimming and no case change: a number with spaces and a leading zero, a budget message with spaces and capitals, an unlisted device and effort', async () => {
  for (const [key, value] of [
    ['reasoning-budget', ' -1 '],
    ['reasoning-budget', '08192'],
    ['n-predict', '8192 '],
    ['image-min-tokens', '1024.0'],
    ['reasoning-budget-message', '  Wrap Up Now. '],
  ] as [string, string][]) {
    const f = FIELDS.find((x) => x.key === key)!
    const { main } = await loaded({
      modelProfile: () => reply(200, exactOf({ [key]: value })),
    })
    if (f.kind === 'number') {
      const group = groupBox(main, f.label)
      expect(
        inputOf(group, f.label).value,
        `${key} ${JSON.stringify(value)}`,
      ).toBe(value)
      expect(boxOf(group).checked, `${key} ${value}: the box unticked`).toBe(
        false,
      )
    } else {
      expect(
        inputOfField(main, f.label).value,
        `${key} ${JSON.stringify(value)}`,
      ).toBe(value)
    }
    cleanup()
    vi.unstubAllGlobals()
  }
  // A select shows the string the page adds for it as written.
  const { main } = await loaded({
    modelProfile: () =>
      reply(
        200,
        exactOf({ 'mmproj-device': ' ROCm1 ', 'reasoning-effort': 'XHigh' }),
      ),
  })
  expect(
    selectedTextExact(selectOf(main, en('model-profile-vision-gpu'))),
    'the device keeps its spaces and its case',
  ).toBe(' ROCm1 ')
  expect(
    selectedTextExact(selectOf(main, en('model-profile-reasoning-effort'))),
    'an effort no option lists keeps its case',
  ).toBe('XHigh')
})

test("2e.118, 2e.119, 2e.104, 2h.134, 2h.176: a long Budget message shows no full-value line, and the Vision GPU device shows its line only while the select is disabled by 176's rule", async () => {
  // 134: the full-value line is only for a disabled control, because an
  // enabled input can be scrolled and an enabled select opened. The Budget
  // message of 104, past the 20 characters of a text input, sits under an
  // enabled input since 2h.170: no line.
  const LONG = 'Wrap up now. Answers.'
  const DEVICE_LONG = 'ROCm1-Attention-Pool-X'
  expect(
    LONG.length,
    'the 104 case: over the 20 characters of a text input',
  ).toBe(21)
  expect(
    DEVICE_LONG.length,
    'and a device value past the same limit of a select',
  ).toBeGreaterThan(20)
  const { main } = await loaded({
    modelProfile: () =>
      reply(200, exactOf({ 'reasoning-budget-message': LONG })),
  })
  const msg = within(main).getByRole('textbox', {
    name: en('model-profile-budget-message'),
  }) as HTMLInputElement
  expect(msg.disabled, 'the budget message is editable (170)').toBe(false)
  expect(msg.value, 'and it holds the long value').toBe(LONG)
  expect(
    main.querySelector('#model-profile-full-reasoning-budget-message'),
    'an enabled input shows no line (134)',
  ).toBeNull()
  // 2h.176: with the read's mmproj present and Vision on GPU checked, the
  // device select is an enabled control: 134 takes its line away, the opened
  // select shows the whole value.
  cleanup()
  vi.unstubAllGlobals()
  const { main: on } = await loaded({
    modelProfile: () =>
      reply(
        200,
        exactOf({
          mmproj: 'Qwen3-VL-mmproj-BF16.gguf',
          'mmproj-offload': 'true',
          'mmproj-device': DEVICE_LONG,
        }),
      ),
  })
  const gpu = within(on).getByRole('combobox', {
    name: en('model-profile-vision-gpu'),
  }) as HTMLSelectElement
  expect(
    gpu.disabled,
    'the Vision GPU select is editable with an MMProj (176)',
  ).toBe(false)
  expect(gpu.value, 'the enabled select holds the long value').toBe(DEVICE_LONG)
  expect(
    on.querySelector('#model-profile-full-mmproj-device'),
    'an enabled select shows no line (134)',
  ).toBeNull()
  // While mmproj is absent the select is disabled (176), and 134 keeps the
  // line of 118 under it: a disabled select cannot be opened.
  cleanup()
  vi.unstubAllGlobals()
  const { main: off } = await loaded({
    modelProfile: () => reply(200, exactOf({ 'mmproj-device': DEVICE_LONG })),
  })
  const device = within(off).getByRole('combobox', {
    name: en('model-profile-vision-gpu'),
  }) as HTMLSelectElement
  expect(
    device.disabled,
    'the Vision GPU select is disabled without an MMProj (176)',
  ).toBe(true)
  const line = off.querySelector('#model-profile-full-mmproj-device')
  expect(
    line,
    'the disabled control has its full-value line (134, 118)',
  ).toBeTruthy()
  expect(line!.tagName, 'the line is a paragraph').toBe('P')
  expect(line!.textContent, 'the line holds the value').toBe(DEVICE_LONG)
  expect(isHidden(line!), 'the line is shown').toBe(false)
})

// ── The cards: numbers, units, disabled, in their place (83, 85, 87, 89, 91) ──

test('2e.87, 2h.158: the four number keys of 2e.3 are all integer keys: the numeric form on the ranged two, none on the two any-whole-number keys', async () => {
  const { main } = await loaded({ modelProfile: () => reply(200, docOf(SET)) })
  const numbers = FIELDS.filter((f) => f.kind === 'number')
  expect(
    numbers.map((f) => f.key),
    'the number keys of 101 and 102',
  ).toEqual([
    'reasoning-budget',
    'n-predict',
    'image-min-tokens',
    'image-max-tokens',
  ])
  for (const f of numbers) {
    expect(f.integer, `${f.key}: the table says integer`).toBe(true)
    const input = inputOf(groupBox(main, f.label), f.label)
    expect(
      input.getAttribute('inputmode'),
      `${f.key}: the input mode of 158`,
    ).toBe(expectedInputMode(f.key, f.integer))
  }
})

test('2e.89: no field of 2e.3 carries a unit, so neither card shows a .unit element', async () => {
  const { main } = await loaded({ modelProfile: () => reply(200, docOf(SET)) })
  for (const cardKey of CARDS)
    expect(
      cardOf(main, cardKey).querySelectorAll('.unit').length,
      `${en(cardKey)}: no unit`,
    ).toBe(0)
})

test('2e.85, 2h.170: every control of the Chat / reasoning card is enabled, every control of the Vision card is natively disabled, the checkboxes included, while every About toggle of 2e.3 stays enabled', async () => {
  const { main } = await loaded({ modelProfile: () => reply(200, docOf(SET)) })
  const controls = CARDS.flatMap((cardKey) => [
    ...cardOf(main, cardKey).querySelectorAll<HTMLElement>(
      'input, select, textarea',
    ),
  ])
  expect(controls.length, 'the controls of 101 and 102').toBe(CONTROLS)
  for (const cardKey of CARDS) {
    // 2h.170 made the Chat / reasoning card editable, with every key present
    // in the stub each number's box is unticked so its input is enabled (156).
    const disabled = !EDITABLE_CARDS.includes(cardKey)
    for (const el of cardOf(main, cardKey).querySelectorAll<HTMLElement>(
      'input, select, textarea',
    )) {
      const id = el.getAttribute('id') ?? el.tagName
      expect(
        (el as HTMLInputElement).disabled,
        `${id}: ${disabled ? 'disabled' : 'enabled'}`,
      ).toBe(disabled)
      expect(
        el.getAttribute('aria-disabled'),
        `${id}: never aria-disabled`,
      ).toBeNull()
      // `readonly` is a text-input attribute (section 0), so the attribute is
      // the check: a select has no readOnly property.
      expect(el.getAttribute('readonly'), `${id}: never readonly`).toBeNull()
    }
  }
  for (const f of FIELDS)
    expect(
      aboutOf(main, f.label).disabled,
      `${f.key}: its About toggle stays enabled`,
    ).toBe(false)
})

test('2e.83: the two cards of 2e.3 are D5.2 Cards inside the section of the h2, Chat / reasoning after KV cache and before Sampling overrides, Vision after Advanced, and the page shows every card in the order of 83', async () => {
  const { main, h1 } = await loaded({
    modelProfile: () => reply(200, docOf(SET)),
  })
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
    'the cards, in the order of 83 (the list of test/modelProfileCards.ts)',
  ).toEqual(cardHeadings())
})

test("2e.91, 2h.170: Tab in main reaches the back link, the ten jump buttons of 2h.10 in card order, the Engine About toggle and select, then every field About toggle of the page in card and field order (the list of test/modelProfileCards.ts), then the Chat / reasoning card's controls; the Vision card offers no control", async () => {
  const { main } = await loaded({ modelProfile: () => reply(200, docOf(SET)) })
  expect(mainTabbable(main)).toEqual([
    ...pageStartTabStops(),
    `${en('field-about')} ${en('model-profile-engine')}`,
    en('model-profile-engine'),
    // H11a.3: the Assets disclosure, the card sits after the Engine field.
    en('model-profile-assets'),
    ...fieldTabStops(docOf(SET).values),
    ...saveBarTabStops(),
  ])
  // A disabled control takes no focus, so the read-only Vision card offers
  // only its fields' About toggles; on the editable Chat / reasoning card
  // every control follows its toggle (170), and a number adds its box and,
  // with the box unticked, its input (156).
  for (const cardKey of CARDS) {
    const fields = FIELDS.filter((f) => f.card === cardKey)
    const stops = fields
      .map((f) => (f.kind === 'number' ? 2 : 1))
      .reduce((n, f) => n + f, 0)
    expect(
      mainTabbable(cardOf(main, cardKey)).length,
      `${en(cardKey)}: ${EDITABLE_CARDS.includes(cardKey) ? 'its About toggles and every control' : 'only its About toggles'}`,
    ).toBe(fields.length + (EDITABLE_CARDS.includes(cardKey) ? stops : 0))
  }
})

// ── The strings (105) and what is out of 2e.3 (106) ────────────────────

test('2e.105: the two cards, every label, hint, toggle, option, note and the empty device option of 2e.3 show the English of the 41 new strings of 105', async () => {
  expect(NEW_STRINGS.length, 'the strings of 105').toBe(41)
  expect(new Set(NEW_STRINGS).size, 'each of them named once').toBe(41)
  for (const key of NEW_STRINGS)
    expect(
      en(key).length,
      `${key}: the test's English is not empty`,
    ).toBeGreaterThan(0)
  const { main } = await loaded({ modelProfile: () => reply(200, docOf(SET)) })
  for (const cardKey of CARDS)
    expect(
      text(cardOf(main, cardKey).querySelector('h3')!),
      `the ${en(cardKey)} card`,
    ).toBe(en(cardKey))
  for (const f of FIELDS) {
    const field = fieldOf(main, f.label)
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
    if (f.kind === 'checkbox')
      expect(
        text(checkBoxOf(main, f).closest('label')!),
        `${f.key}: the toggle`,
      ).toBe(en(f.toggle ?? ''))
    if (f.kind === 'select')
      expect(
        optionTexts(selectOf(main, f.label)),
        `${f.key}: the options of the table`,
      ).toEqual(f.options)
  }
  for (const [cardKey, noteKey] of NOTES)
    expect(
      notesOf(cardOf(main, cardKey)).map((p) => text(p)),
      `${en(cardKey)}: the note shows the English of 105`,
    ).toEqual(
      // 2h.176: the SET stub has no mmproj, so the Vision card opens with
      // its model-profile-vision-off paragraph (the string of 2h.7's 182) before
      // 90's note; the Chat / reasoning card has only the note of 90.
      cardKey === 'model-profile-vision'
        ? [en('model-profile-vision-off'), en(noteKey)]
        : [en(noteKey)],
    )
  expect(
    optionsOf(selectOf(main, en('model-profile-vision-gpu')))[0]!.textContent,
    'the empty device option of 105',
  ).toBe(en('model-profile-device-follows-model'))
})

test('2e.106, 2e.87: no field of 2e.3 shows a placeholder, and the two cards show no text of 106: the budget message example of toml 337, the KV warning strip, the embedded-MTP note', async () => {
  const { main } = await loaded({ modelProfile: () => reply(200, docOf(SET)) })
  const inputs = CARDS.flatMap((cardKey) => [
    ...cardOf(main, cardKey).querySelectorAll<HTMLInputElement>('input'),
  ])
  expect(inputs.length, 'the inputs of 101 and 102').toBeGreaterThan(0)
  for (const input of inputs)
    expect(
      input.getAttribute('placeholder') ?? '',
      `${input.getAttribute('id')}: no placeholder in 2e`,
    ).toBe('')
  // The reply holds the speculative keys of 2e.5 and a budget message; the
  // two cards of 2e.3 show none of the texts 106 leaves out.
  const shown = CARDS.map((cardKey) => cardOf(main, cardKey).textContent ?? '')
    .join('|')
    .toLowerCase()
  for (const out of [
    // toml 337, 2h's to judge: the Budget message's example.
    'budget reached, write the final answer now',
    // The embedded-MTP note (2g) and the KV warning strip (2k).
    'draft-mtp',
    'ngram',
  ])
    expect(shown, `the two cards of 2e.3 show no ${out}`).not.toContain(out)
})
