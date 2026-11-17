// The Hardware card of 2e.4 on the preset page (fe/briefs/02e-preset-cards.md,
// criteria 107 to 111): the field table of 107 (six numbers, four selects, the
// device select of 103, one checkbox, one text, two MiB units), what 2e.4
// brings that the earlier sub-slices did not have: the Fit to VRAM checkbox of
// 108, whose toggle text follows the GUI's rule (allowed only with
// Context size and GPU layers both absent) while its state is the engine's
// reading of 127 (2e.3c), the lazy-mode select matched
// exactly, and the load-mode select matched ignoring case and space while
// disabled (2e's 107), now exactly, since 2h.7 enables it (180). The
// shared rules of section 1 are pinned for the cards of 2e.1 in
// PresetCards1.test.tsx, for those of 2e.2 in PresetCards2.test.tsx and for
// what 2e.3 adds in PresetCards3.test.tsx; here they are pinned for the
// Hardware card. 2h.7's 175 to 180 retarget what 2e left read-only: the seven
// `always` rows and the draft pair (behind `model-draft`, 177) are enabled, the
// Fit trio (2h.8) and the two MoE rows (178) stay natively disabled, the card
// holds the two note rows of 177 and 178, and load-mode is matched exactly
// (180): `MLOCK` is the extra option `MLOCK`. The editing itself (the draft,
// what a save sends, the checks) is PresetSave13.test.tsx.
// The checkbox is pinned here for the `on` value of the 109 stub
// and for the absent key only: every row of the 129 table, with the written-as
// line and the field error, is pinned in PresetFlags.test.tsx. The route and
// the read are
// PresetPage.test.tsx, the Engine field and the Assets card are
// PresetContent.test.tsx, the full-value line as a rule is
// PresetFullValue.test.tsx. The colours of 85, the width of 91 and the muted
// style of 90 are checked in review, not in jsdom.
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
  controlEnabled,
  expectedInputMode,
  fieldTabStops,
  saveBarTabStops,
} from './test/presetCards.ts'

const ID = 'Qwen3-27B'
const HASH = `#/models/${ID}`
const MODEL = '/var/lib/private-ai/.llama.cpp/models/Qwen3-27B-Q8_0.gguf'

/** The criterion 109 stub: every key of 107 set. */
const SET: Record<string, string> = {
  'n-gpu-layers': '99',
  'n-cpu-moe': '20',
  'moe-cache-mib': '2048',
  'n-cpu-ffn': '4',
  'op-offload': 'false',
  repack: 'true',
  'no-host': 'true',
  'lazy-mode': 'auto',
  'load-mode': 'MLOCK',
  fit: 'on',
  'fit-ctx': '8192',
  'fit-target': '2048,1024',
  'device-draft': 'ROCm1',
  'n-gpu-layers-draft': '0',
}

/** The shared option texts of section 1.3. */
const TRISTATE = [en('option-default'), en('option-on'), en('option-off')]
const OFF_ON = [en('option-off'), en('option-on')]

/** The Lazy tensors options of 107 (MP 564-565). */
const LAZY = [
  en('option-default'),
  en('option-on'),
  en('option-auto'),
  en('option-off'),
]

/** The Load mode options of 107 (MP 102-103): three strings, four literals. */
const LOAD = [
  en('option-default'),
  en('option-auto'),
  en('option-none'),
  'mmap',
  'mlock',
  'mmap+mlock',
  'dio',
]

/** The load-mode values that match one of the table's options exactly (180),
 * each with the option text it selects. */
const LOAD_MODES: [string, string][] = [
  ['auto', en('option-auto')],
  ['none', en('option-none')],
  ['mmap', 'mmap'],
  ['mlock', 'mlock'],
  ['mmap+mlock', 'mmap+mlock'],
  ['dio', 'dio'],
]

/** The empty option of the Draft device select (107, 110). */
const SAME = en('preset-device-same-as-model')

/** The card of 107. */
const CARD = 'preset-hardware-config'

/** When 2h.7's 175 to 178 leaves a row's controls enabled. */
type Enable = 'always' | 'model-draft' | 'read-only'

/** One row of the field table of 107. */
type Row = {
  /** The key of the section's `values`. */
  key: string
  /** The table's last column (107), as 2h.7's 175 to 178 retargets it:
   * `always` is editable; `model-draft` is enabled while the read's
   * `model-draft` is present and not empty (177); `read-only` stays disabled
   * (the Fit trio, until 2h.8; the two MoE rows, until 2g, 178). */
  enable: Enable
  /** The label as the page shows it: the strings text of 110. */
  label: string
  /** The strings key of the field's hint (criterion 88). */
  hint: string
  kind: 'checkbox' | 'select' | 'number' | 'text' | 'device'
  /** A number: an integer key takes the numeric input form (87). */
  integer?: boolean
  /** The literal unit after the input (89), where the table gives one. */
  unit?: string
  /** A checkbox: the strings key of the toggle's text (87). Fit to VRAM has
   * none: 108 gives two texts, one per state. */
  toggle?: string
  /** A select's option texts, in the table's order. */
  options?: string[]
  /** A select: the option text it shows when the key is absent. */
  absentOption?: string
  /** A checkbox: the state its key absent shows (107; for `fit`, 127). */
  absentChecked?: boolean
  /** The value of the criterion 109 stub. */
  set: string
  /** What the field shows for it: the input's value, the selected option's
   * text, or the checkbox's state as a word. */
  shown: string
  shownChecked?: boolean
}

/** The field table of 107, in the GUI's order, each row with its 2h.7 enable
 * rule. */
const FIELDS: Row[] = [
  {
    key: 'n-gpu-layers',
    enable: 'always',
    label: en('preset-gpu-layers'),
    hint: 'preset-gpu-layers-hint',
    kind: 'number',
    integer: true,
    set: '99',
    shown: '99',
  },
  {
    key: 'n-cpu-moe',
    label: en('preset-moe-cpu-layers'),
    enable: 'read-only',
    hint: 'preset-moe-cpu-layers-hint',
    kind: 'number',
    integer: true,
    set: '20',
    shown: '20',
  },
  {
    key: 'moe-cache-mib',
    label: en('preset-moe-cache'),
    enable: 'read-only',
    hint: 'preset-moe-cache-hint',
    kind: 'number',
    integer: true,
    unit: 'MiB',
    set: '2048',
    shown: '2048',
  },
  {
    key: 'n-cpu-ffn',
    label: en('preset-dense-ffn-cpu-layers'),
    enable: 'always',
    hint: 'preset-dense-ffn-cpu-layers-hint',
    kind: 'number',
    integer: true,
    set: '4',
    shown: '4',
  },
  {
    key: 'op-offload',
    label: en('preset-op-offload'),
    enable: 'always',
    hint: 'preset-op-offload-hint',
    kind: 'select',
    options: TRISTATE,
    absentOption: en('option-default'),
    set: 'false',
    shown: en('option-off'),
  },
  {
    key: 'repack',
    label: en('preset-repack'),
    enable: 'always',
    hint: 'preset-repack-hint',
    kind: 'select',
    options: TRISTATE,
    absentOption: en('option-default'),
    set: 'true',
    shown: en('option-on'),
  },
  {
    key: 'no-host',
    label: en('preset-no-host-buffer'),
    enable: 'always',
    hint: 'preset-no-host-buffer-hint',
    kind: 'select',
    options: OFF_ON,
    absentOption: en('option-off'),
    set: 'true',
    shown: en('option-on'),
  },
  {
    key: 'lazy-mode',
    label: en('preset-lazy-tensors'),
    enable: 'always',
    hint: 'preset-lazy-tensors-hint',
    kind: 'select',
    options: LAZY,
    absentOption: en('option-default'),
    set: 'auto',
    shown: en('option-auto'),
  },
  {
    key: 'load-mode',
    label: en('preset-load-mode'),
    enable: 'always',
    hint: 'preset-load-mode-hint',
    kind: 'select',
    // 180: the select is enabled, so the match is exact and the stub's `MLOCK`
    // stands as the extra option the file's value, listed last.
    options: [...LOAD, 'MLOCK'],
    absentOption: en('option-default'),
    set: 'MLOCK',
    shown: 'MLOCK',
  },
  {
    key: 'fit',
    label: en('preset-fit-to-vram'),
    enable: 'read-only',
    hint: 'preset-fit-to-vram-hint',
    kind: 'checkbox',
    // 127: the box is checked when `fit` is absent (llama.cpp's default) and
    // when it is one of the four on values, so `on` reads checked and the
    // absent key reads checked: 2e.3c replaces 108's rule for the state.
    absentChecked: true,
    set: 'on',
    shown: 'checked',
    shownChecked: true,
  },
  {
    key: 'fit-ctx',
    label: en('preset-fit-min-context'),
    enable: 'read-only',
    hint: 'preset-fit-min-context-hint',
    kind: 'number',
    integer: true,
    set: '8192',
    shown: '8192',
  },
  {
    key: 'fit-target',
    label: en('preset-fit-free-vram'),
    enable: 'read-only',
    hint: 'preset-fit-free-vram-hint',
    kind: 'text',
    unit: 'MiB',
    set: '2048,1024',
    shown: '2048,1024',
  },
  {
    key: 'device-draft',
    label: en('preset-draft-device'),
    enable: 'model-draft',
    hint: 'preset-draft-device-hint',
    kind: 'device',
    absentOption: SAME,
    set: 'ROCm1',
    shown: 'ROCm1',
  },
  {
    key: 'n-gpu-layers-draft',
    label: en('preset-draft-gpu-layers'),
    enable: 'model-draft',
    hint: 'preset-draft-gpu-layers-hint',
    kind: 'number',
    integer: true,
    set: '0',
    shown: '0',
  },
]

/** The 32 new strings of 110. */
const NEW_STRINGS = [
  'preset-hardware-config',
  'preset-gpu-layers',
  'preset-gpu-layers-hint',
  'preset-moe-cpu-layers',
  'preset-moe-cpu-layers-hint',
  'preset-moe-cache',
  'preset-moe-cache-hint',
  'preset-dense-ffn-cpu-layers',
  'preset-dense-ffn-cpu-layers-hint',
  'preset-op-offload',
  'preset-op-offload-hint',
  'preset-repack',
  'preset-repack-hint',
  'preset-no-host-buffer',
  'preset-no-host-buffer-hint',
  'preset-lazy-tensors',
  'preset-lazy-tensors-hint',
  'preset-load-mode',
  'preset-load-mode-hint',
  'preset-fit-to-vram',
  'preset-fit-to-vram-hint',
  'preset-fit-allowed',
  'preset-fit-not-allowed',
  'preset-fit-min-context',
  'preset-fit-min-context-hint',
  'preset-fit-free-vram',
  'preset-fit-free-vram-hint',
  'preset-draft-device',
  'preset-draft-device-hint',
  'preset-draft-gpu-layers',
  'preset-draft-gpu-layers-hint',
  'preset-device-same-as-model',
]

/** The controls 107 puts in the card: a box and an input per number, one
 * control per checkbox, select, device and text. */
const CONTROLS = FIELDS.reduce((n, f) => n + (f.kind === 'number' ? 2 : 1), 0)

beforeEach(() => startAt(HASH))

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
  startAt('')
})

// ── Helpers (as PresetCards3.test.tsx) ──────────────────────────────────

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
function docOf(
  values: Record<string, string>,
  extra: Record<string, unknown> = {},
) {
  return presetDoc({ model: MODEL, ...values }, extra)
}

/** A read whose `values` are exactly `values` (the stubs of 109). */
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

/** A checkbox field's checkbox (87). Fit to VRAM has no fixed text (108), so
 * its checkbox is the only one in the group. */
function checkBoxOf(main: HTMLElement, f: Row): HTMLInputElement {
  const group = groupBox(main, f.label)
  if (f.toggle === undefined) {
    const boxes = within(group).getAllByRole('checkbox')
    expect(boxes.length, `${f.key}: one checkbox in its Field`).toBe(1)
    return boxes[0] as HTMLInputElement
  }
  return within(group).getByRole('checkbox', {
    name: en(f.toggle),
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

/** The text of `card` as a user reads it: what a closed hint holds is left
 * out, since jsdom keeps the text of a `hidden` element in `textContent`.
 * A hint is prose and names keys of other cards (the Fit to VRAM hint names
 * `tensor-split`), so the whole `textContent` is no test for "the page does
 * not show this key". */
function cardText(card: HTMLElement): string {
  const open = card.cloneNode(true) as HTMLElement
  open.querySelectorAll('[hidden]').forEach((el) => el.remove())
  return text(open)
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
  if (f.kind === 'select' || f.kind === 'device') return selectOf(main, f.label)
  return inputOfField(main, f.label)
}

/** What a select shows when its key is absent: the table's absent option. */
function expectAbsentSelect(main: HTMLElement, f: Row): void {
  expect(
    selectedText(selectOf(main, f.label)),
    `${f.key}: absent shows ${f.absentOption}`,
  ).toBe(f.absentOption)
}

/** What a device select shows when its key is absent (103): only the empty
 * option, whose value is the empty string. */
function expectAbsentDevice(main: HTMLElement, f: Row): void {
  const select = selectOf(main, f.label)
  const opts = optionsOf(select)
  expect(opts.length, `${f.key}: absent holds only the empty option`).toBe(1)
  expect(opts[0]!.value, `${f.key}: the empty option carries ''`).toBe('')
  expect(opts[0]!.textContent, `${f.key}: its text`).toBe(f.absentOption)
  expect(select.selectedIndex, `${f.key}: the empty option is selected`).toBe(0)
}

/** What a field shows when its key is absent, whatever its kind. */
function expectAbsent(main: HTMLElement, f: Row): void {
  if (f.kind === 'number') {
    const group = groupBox(main, f.label)
    expect(boxOf(group).checked, `${f.key}: the default box ticked`).toBe(true)
    expect(inputOf(group, f.label).value, `${f.key}: an empty input`).toBe('')
  } else if (f.kind === 'checkbox') {
    expect(
      checkBoxOf(main, f).checked,
      `${f.key}: absent shows ${f.absentChecked ? 'checked' : 'unchecked'}`,
    ).toBe(f.absentChecked)
  } else if (f.kind === 'select') {
    expectAbsentSelect(main, f)
  } else if (f.kind === 'device') {
    expectAbsentDevice(main, f)
  } else {
    const input = inputOfField(main, f.label)
    expect(input.value, `${f.key}: an empty input`).toBe('')
    expect(
      input.getAttribute('placeholder') ?? '',
      `${f.key}: no placeholder in 2e`,
    ).toBe('')
  }
}

/** The unit elements of a field, with their text (89). */
function unitsOf(main: HTMLElement, f: Row): string[] {
  return [...fieldOf(main, f.label).querySelectorAll('.unit')].map((u) =>
    text(u),
  )
}

/** What Fit to VRAM shows: the checkbox's state and the text that names it,
 * which 108 picks between `preset-fit-allowed` and `preset-fit-not-allowed`. */
function fitState(main: HTMLElement): [boolean, string] {
  const box = checkBoxOf(
    main,
    FIELDS.find((f) => f.key === 'fit')!,
  )
  return [box.checked, text(box.closest('label')!)]
}

/** Fit to VRAM's checkbox: named by the rule of 108, so found in its group. */
function fitShows(
  main: HTMLElement,
  checked: boolean,
  toggleKey: string,
  what: string,
): void {
  const [isChecked, toggle] = fitState(main)
  expect(isChecked, `${what}: checked`).toBe(checked)
  expect(toggle, `${what}: the toggle text`).toBe(en(toggleKey))
}

// ── One test per field (85, 86, 87, 88, 89, 107, 109) ──────────────────

/** True when 2h.7's 175 to 178 leaves this row's controls enabled on a read
 * that holds `values`: the seven `always` rows (175), and the draft pair while
 * the read's `model-draft` is present and not empty (177). The Fit trio (kept
 * read-only until 2h.8) and the two MoE rows (kept read-only until 2g, 178)
 * are never enabled. The box of an editable number is enabled too (156);
 * whether its input is, follows the box, which is `controlEnabled`. */
function rowEnabled(f: Row, values: Record<string, string>): boolean {
  const on =
    f.enable === 'always' ||
    (f.enable === 'model-draft' && (values['model-draft'] ?? '') !== '')
  if (controlEnabled(f.label, values))
    expect(
      on,
      `${f.key}: the shared list of the editable fields agrees (175)`,
    ).toBe(true)
  return on
}

test.each(FIELDS)(
  '2e.107, 2e.87, 2h.158, 2h.175: $key is a $kind named by $label inside the Hardware card, shows the set value $set as the reply holds it, and its controls are natively enabled exactly as $enable says',
  async (f) => {
    const { main } = await loaded({ preset: () => reply(200, docOf(SET)) })
    const on = rowEnabled(f, docOf(SET).values)
    const field = fieldOf(main, f.label)
    expect(
      cardOf(main, CARD).contains(field),
      `${f.key}: its field is inside the ${en(CARD)} card`,
    ).toBe(true)
    // 89: the unit sits after the input, only where the table gives one.
    expect(unitsOf(main, f), `${f.key}: the unit of the table`).toEqual(
      f.unit === undefined ? [] : [f.unit],
    )
    if (f.kind === 'number') {
      const group = groupBox(main, f.label)
      const box = boxOf(group)
      const input = inputOf(group, f.label)
      expect(input.value, `${f.key}: the string of the file exactly`).toBe(
        f.shown,
      )
      expect(box.checked, `${f.key}: the default box unticked`).toBe(false)
      expect(box.disabled, `${f.key}: the box disabled`).toBe(!on)
      expect(input.disabled, `${f.key}: the input disabled`).toBe(!on)
      // 87 as retargeted by 2h.158: every number of 107 is an integer key;
      // the ranged two take the numeric form, the layer counts none.
      expect(
        input.getAttribute('inputmode'),
        `${f.key}: ${expectedInputMode(f.key, f.integer) ?? 'plain'}`,
      ).toBe(expectedInputMode(f.key, f.integer))
    } else if (f.kind === 'checkbox') {
      const box = checkBoxOf(main, f)
      expect(box.checked, `${f.key}: ${f.set} shows ${f.shown}`).toBe(
        f.shownChecked,
      )
      expect(box.disabled, `${f.key}: the checkbox disabled`).toBe(!on)
    } else if (f.kind === 'select') {
      const select = selectOf(main, f.label)
      expect(optionTexts(select), `${f.key}: the options in order`).toEqual(
        f.options,
      )
      expect(selectedText(select), `${f.key}: the mapping selects`).toBe(
        f.shown,
      )
      expect(select.disabled, `${f.key}: the select disabled`).toBe(!on)
    } else if (f.kind === 'device') {
      // 103 in 2e: the empty option, then the device of the file.
      const select = selectOf(main, f.label)
      const opts = optionsOf(select)
      expect(opts.length, `${f.key}: the empty option plus the file's`).toBe(2)
      expect(opts[0]!.value, `${f.key}: the empty option stays first`).toBe('')
      expect(opts[0]!.textContent, `${f.key}: its text of 110`).toBe(SAME)
      expect(
        opts[1]!.value,
        `${f.key}: the option carries the file's value`,
      ).toBe(f.set)
      expect(selectedTextExact(select), `${f.key}: and is selected`).toBe(
        f.shown,
      )
      expect(select.disabled, `${f.key}: the select disabled`).toBe(!on)
    } else {
      const input = inputOfField(main, f.label)
      expect(input.value, `${f.key}: the string of the file exactly`).toBe(
        f.shown,
      )
      expect(input.disabled, `${f.key}: the input disabled`).toBe(!on)
      expect(
        input.getAttribute('placeholder') ?? '',
        `${f.key}: no placeholder in 2e`,
      ).toBe('')
    }
    // 85: the native disabled attribute, never aria-disabled, never readonly.
    for (const el of [
      controlOf(main, f),
      ...(f.kind === 'number'
        ? [groupBox(main, f.label).querySelector('input') as Element]
        : []),
    ]) {
      expect(
        (el as HTMLInputElement).disabled,
        `${f.key}: ${el.tagName} ${on ? 'enabled' : 'disabled'}`,
      ).toBe(!on)
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
  '2e.86, 2e.109: $key is absent from a values of {} so its field shows the absent form of $kind',
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

// ── Absent keys and the fields of the card (81, 86) ─────────────────────

test('2e.86, 2e.109: a key of 107 whose value is the empty string reads as absent, for every key of the Hardware card that is not a flag (2e.3b: a flag reads "" as on, criterion 123) and not `fit` (2e.3c: `""` reaches the engine as `--fit ""`, which it refuses, criterion 127)', async () => {
  for (const f of FIELDS) {
    // op-offload, repack, no-host: the flag reading of 123. fit: `-fit
    // [on|off]` takes a value, so `""` is invalid, not absent (127, 128; the
    // rows of 129 are pinned in PresetFlags.test.tsx).
    if (['op-offload', 'repack', 'no-host', 'fit'].includes(f.key)) continue
    const { main } = await loaded({
      preset: () => reply(200, exactOf({ ...SET, [f.key]: '' })),
    })
    expectAbsent(main, f)
    cleanup()
    vi.unstubAllGlobals()
  }
})

test('2e.86, 2e.81: a key the page does not show adds no field, so the Hardware card holds exactly the 14 fields of 107 in the table order', async () => {
  const { main } = await loaded({
    preset: () =>
      reply(
        200,
        docOf({
          ...SET,
          // 2e.5 and 2g keys, and the Engine key: none is a field of 107.
          'draft-max': 'zz-secret',
          'tensor-split': 'zz-secret',
          gpu_split: 'zz-secret',
        }),
      ),
  })
  const card = cardOf(main, CARD)
  expect(
    card.querySelectorAll('.field').length,
    `${en(CARD)}: one Field per row of its table`,
  ).toBe(FIELDS.length)
  // 81: the key adds no Field, so the card carries no id, no label and no
  // shown text for it. The values are the reply's, and a missing strings key
  // shows as its key, so all three secrets read as plain text; the ids are
  // what a Field gives a control (`preset-<key>-control`, `-box`, `-hint`).
  const shown = cardText(card)
  for (const secret of ['zz-secret', 'draft-max', 'tensor-split']) {
    expect(shown, `${en(CARD)}: ${secret} is not shown`).not.toContain(secret)
    for (const el of card.querySelectorAll('[id]'))
      expect(el.id, `${en(CARD)}: no id names ${secret}`).not.toContain(secret)
  }
  const fields = [...card.querySelectorAll('.field')]
  for (let i = 0; i < FIELDS.length; i++)
    expect(
      fields[i]!.contains(aboutOf(main, FIELDS[i]!.label)),
      `${FIELDS[i]!.key} is the ${i + 1}th field of ${en(CARD)}`,
    ).toBe(true)
})

// ── Fit to VRAM, the rule 2e.4 takes from the GUI (108, 109) ────────────

test('2e.108, 2e.127: the rule of 108 picks only the toggle text, preset-fit-allowed when Context size and GPU layers are both absent and preset-fit-not-allowed when not: the box keeps the file reading of 127, so `fit: true` shows checked in every row', async () => {
  for (const [ctx, layers, allowed] of [
    [undefined, undefined, true],
    ['172032', undefined, false],
    [undefined, '99', false],
    ['172032', '99', false],
  ] as [string | undefined, string | undefined, boolean][]) {
    const values: Record<string, string> = { fit: 'true' }
    if (ctx !== undefined) values['ctx-size'] = ctx
    if (layers !== undefined) values['n-gpu-layers'] = layers
    const { main } = await loaded({
      preset: () => reply(200, exactOf(values)),
    })
    fitShows(
      main,
      // 127: `true` is an on value, so the box is checked whatever the rule
      // says: llama-server runs the fit there and the text tells the user
      // what the configurator needs for a full one.
      true,
      allowed ? 'preset-fit-allowed' : 'preset-fit-not-allowed',
      `ctx-size ${ctx ?? 'absent'}, n-gpu-layers ${layers ?? 'absent'}`,
    )
    cleanup()
    vi.unstubAllGlobals()
  }
})

test('2e.109: the stub of 109 shows as 127 and 128 read it: every key set checks Fit to VRAM with preset-fit-not-allowed, values {fit: on} checks it with preset-fit-allowed, values {} checks it, fit TRUE alone is refused so unchecked with the field error', async () => {
  // Every key of 107 set, and the read holds n-gpu-layers: not allowed, so the
  // not-allowed text of 108. The file's `fit` is `on`, so the box is checked:
  // 127 replaces the unchecked 109 read from 108.
  const { main } = await loaded({ preset: () => reply(200, docOf(SET)) })
  fitShows(main, true, 'preset-fit-not-allowed', 'the stub of 109')
  cleanup()
  vi.unstubAllGlobals()
  // `values` {fit: "on"}: allowed, and `on` is on.
  const on = await loaded({ preset: () => reply(200, exactOf({ fit: 'on' })) })
  fitShows(on.main, true, 'preset-fit-allowed', 'values {fit: "on"}')
  cleanup()
  vi.unstubAllGlobals()
  // `values` {}: allowed, and an absent `fit` is on: llama.cpp's own default
  // (127 replaces the unchecked 109 read from 108).
  const none = await loaded({ preset: () => reply(200, exactOf({})) })
  fitShows(none.main, true, 'preset-fit-allowed', 'values {}')
  cleanup()
  vi.unstubAllGlobals()
  // `fit` TRUE with nothing else: the engine's two lists are case-sensitive, so
  // it refuses the value: unchecked, with the field error of 128 (109 read it
  // checked, 127 and 128 replace that).
  const true_ = await loaded({
    preset: () => reply(200, exactOf({ fit: 'TRUE' })),
  })
  fitShows(true_.main, false, 'preset-fit-allowed', 'fit TRUE')
  expect(
    true_.main.querySelector('#preset-fit-error')?.textContent,
    'fit TRUE: 128: #preset-fit-error names the refused value',
  ).toContain(en('preset-fit-invalid'))
  cleanup()
  vi.unstubAllGlobals()
  // `false` with nothing else: allowed, unchecked.
  const off = await loaded({
    preset: () => reply(200, exactOf({ fit: 'false' })),
  })
  fitShows(off.main, false, 'preset-fit-allowed', 'fit false')
  cleanup()
  vi.unstubAllGlobals()
  // `fit` on with GPU layers set: checked, with the GUI's text: the engine
  // still fits (127), the text says what the configurator needs (108).
  const layers = await loaded({
    preset: () => reply(200, exactOf({ fit: 'on', 'n-gpu-layers': '99' })),
  })
  fitShows(layers.main, true, 'preset-fit-not-allowed', 'fit on, layers set')
})

// ── The two selects 2e.4 adds (107) ────────────────────────────────────

test('2e.107: Lazy tensors lists default, on, auto, off in that order, absent shows default, each value selects its own option, and the match is exact: Auto and " auto " are one extra last option as written, not auto', async () => {
  const f = FIELDS.find((x) => x.key === 'lazy-mode')!
  const { main } = await loaded({ preset: () => reply(200, exactOf({})) })
  expect(optionTexts(selectOf(main, f.label)), 'the options of 107').toEqual(
    LAZY,
  )
  expectAbsentSelect(main, f)
  cleanup()
  vi.unstubAllGlobals()
  for (const [value, shown] of [
    ['default', LAZY[0]],
    ['on', LAZY[1]],
    ['auto', LAZY[2]],
    ['off', LAZY[3]],
  ] as [string, string][]) {
    const { main: m } = await loaded({
      preset: () => reply(200, exactOf({ 'lazy-mode': value })),
    })
    const select = selectOf(m, f.label)
    expect(
      optionTexts(select),
      `lazy-mode ${value}: the options stay the table's`,
    ).toEqual(LAZY)
    expect(selectedText(select), `lazy-mode ${value}: its own option`).toBe(
      shown,
    )
    cleanup()
    vi.unstubAllGlobals()
  }
  // The match is exact here and, since 2h.7's 180, for Load mode too: the case
  // and the spaces stand.
  for (const value of ['Auto', ' auto ', 'AUTO+']) {
    const { main: m } = await loaded({
      preset: () => reply(200, exactOf({ 'lazy-mode': value })),
    })
    const select = selectOf(m, f.label)
    expect(
      optionsOf(select).length,
      `lazy-mode ${JSON.stringify(value)}: the four options plus one`,
    ).toBe(LAZY.length + 1)
    expect(
      selectedTextExact(select),
      `lazy-mode ${JSON.stringify(value)}: its own option, as written`,
    ).toBe(value)
    cleanup()
    vi.unstubAllGlobals()
  }
})

test('2e.107, 2h.180: Load mode lists default, auto, none and the literals mmap, mlock, mmap+mlock, dio in that order, absent shows default, each mode selects its own option, and the match is exact: MLOCK, " MLOCK ", MMAP+MLOCK and "  Dio  " are each one extra last option as written, never the mode they resemble', async () => {
  const f = FIELDS.find((x) => x.key === 'load-mode')!
  const { main } = await loaded({ preset: () => reply(200, exactOf({})) })
  expect(optionTexts(selectOf(main, f.label)), 'the options of 107').toEqual(
    LOAD,
  )
  expectAbsentSelect(main, f)
  cleanup()
  vi.unstubAllGlobals()
  for (const [value, shown] of LOAD_MODES) {
    const { main: m } = await loaded({
      preset: () => reply(200, exactOf({ 'load-mode': value })),
    })
    const select = selectOf(m, f.label)
    expect(
      optionTexts(select),
      `load-mode ${value}: the options stay the table's`,
    ).toEqual(LOAD)
    expect(selectedText(select), `load-mode ${value}: its own option`).toBe(
      shown,
    )
    cleanup()
    vi.unstubAllGlobals()
  }
  // 180: the match is EXACT, as llama.cpp's `--load-mode` and the manager's
  // check compare it: the case and the surrounding space stand, so each of
  // these is its own extra option, never the mode it resembles.
  for (const value of ['MLOCK', ' MLOCK ', 'MMAP+MLOCK', '  Dio  ']) {
    const { main: m } = await loaded({
      preset: () => reply(200, exactOf({ 'load-mode': value })),
    })
    const select = selectOf(m, f.label)
    expect(
      optionTexts(select),
      `load-mode ${JSON.stringify(value)}: the seven options, then its own`,
    ).toEqual([...LOAD, value.trim()])
    expect(
      selectedTextExact(select),
      `load-mode ${JSON.stringify(value)}: its own option, as written`,
    ).toBe(value)
    cleanup()
    vi.unstubAllGlobals()
  }
  // The stub of 109: load-mode MLOCK is the extra option MLOCK, listed last.
  const set = await loaded({ preset: () => reply(200, docOf(SET)) })
  const sel = selectOf(set.main, f.label)
  expect(optionTexts(sel), 'the stub: the seven options, then MLOCK').toEqual([
    ...LOAD,
    'MLOCK',
  ])
  expect(
    selectedTextExact(sel),
    'the stub: MLOCK selects MLOCK, not mlock (180)',
  ).toBe('MLOCK')
  cleanup()
  vi.unstubAllGlobals()
  // A value no mode is: one extra last option, not the GUI's fallback to auto.
  const miss = await loaded({
    preset: () => reply(200, exactOf({ 'load-mode': 'MLOCKED' })),
  })
  const select = selectOf(miss.main, f.label)
  expect(
    optionsOf(select).length,
    'load-mode MLOCKED: the seven options plus one',
  ).toBe(LOAD.length + 1)
  expect(selectedTextExact(select), 'and it is the value as written').toBe(
    'MLOCKED',
  )
})

test('2e.107, 2e.1.3, 2e.122, 2e.125: Op offload and Repack are the shared tri-state (absent default, true on, false off, an unlisted flag value on with no extra option: 2e.3b), No host buffer is off / on (absent and false off, true on)', async () => {
  for (const key of ['op-offload', 'repack']) {
    const f = FIELDS.find((x) => x.key === key)!
    expect(f.options, `${key}: the tri-state of 1.3`).toEqual(TRISTATE)
    for (const [value, shown] of [
      [undefined, en('option-default')],
      ['true', en('option-on')],
      ['false', en('option-off')],
    ] as [string | undefined, string][]) {
      const { main } = await loaded({
        preset: () =>
          reply(200, exactOf(value === undefined ? {} : { [key]: value })),
      })
      const select = selectOf(main, f.label)
      expect(optionTexts(select), `${key} ${value}: the three options`).toEqual(
        TRISTATE,
      )
      expect(selectedText(select), `${key} ${value}: shows ${shown}`).toBe(
        shown,
      )
      cleanup()
      vi.unstubAllGlobals()
    }
    const { main } = await loaded({
      preset: () => reply(200, exactOf({ [key]: 'yes' })),
    })
    const select = selectOf(main, f.label)
    expect(
      optionTexts(select),
      `${key}: a flag select keeps exactly the options of 1.3`,
    ).toEqual(TRISTATE)
    expect(selectedText(select), `${key}: yes is on, not an extra option`).toBe(
      en('option-on'),
    )
    cleanup()
    vi.unstubAllGlobals()
  }
  const f = FIELDS.find((x) => x.key === 'no-host')!
  expect(f.options, 'no-host: the off / on of 1.3').toEqual(OFF_ON)
  for (const [value, shown] of [
    [undefined, en('option-off')],
    ['false', en('option-off')],
    ['true', en('option-on')],
  ] as [string | undefined, string][]) {
    const { main } = await loaded({
      preset: () =>
        reply(200, exactOf(value === undefined ? {} : { 'no-host': value })),
    })
    const select = selectOf(main, f.label)
    expect(optionTexts(select), `no-host ${value}: the two options`).toEqual(
      OFF_ON,
    )
    expect(selectedText(select), `no-host ${value}: shows ${shown}`).toBe(shown)
    cleanup()
    vi.unstubAllGlobals()
  }
})

// ── The device select of 103 with its own empty option (107, 110) ───────

test('2e.103, 2e.107: Draft device absent holds only the empty option, whose value is the empty string and whose text is preset-device-same-as-model, selected; the page lists no device, it has no device list in 2e', async () => {
  const { main } = await loaded({ preset: () => reply(200, exactOf({})) })
  const select = selectOf(main, en('preset-draft-device'))
  expectAbsentDevice(
    main,
    FIELDS.find((f) => f.key === 'device-draft')!,
  )
  expect(select.disabled, 'the select disabled').toBe(true)
})

test('2e.103, 2e.107: Draft device present keeps the empty option first and adds one option whose value and text are the reply string as written, selected; and each device select carries its own empty option of 110', async () => {
  for (const value of ['ROCm1', 'CUDA0', ' ROCm1 ', 'cpu']) {
    const { main } = await loaded({
      preset: () => reply(200, exactOf({ 'device-draft': value })),
    })
    const select = selectOf(main, en('preset-draft-device'))
    const opts = optionsOf(select)
    expect(opts.length, `${value}: the empty option plus one`).toBe(2)
    expect(opts[0]!.value, `${value}: the empty option stays first`).toBe('')
    expect(opts[0]!.textContent, `${value}: and its text of 110`).toBe(SAME)
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
    cleanup()
    vi.unstubAllGlobals()
  }
  // 110: Vision GPU keeps preset-device-follows-model, Draft device its own.
  const { main } = await loaded({
    preset: () =>
      reply(
        200,
        exactOf({ 'mmproj-device': 'CUDA0', 'device-draft': 'CUDA1' }),
      ),
  })
  expect(
    optionsOf(selectOf(main, en('preset-draft-device')))[0]!.textContent,
    'the empty option of the Draft device select of 110',
  ).toBe(SAME)
  expect(
    optionsOf(selectOf(main, en('preset-vision-gpu')))[0]!.textContent,
    'the empty option of the Vision GPU select stays its own',
  ).toBe(en('preset-device-follows-model'))
})

// ── The card: units, numbers, disabled, in its place (83, 85, 87, 89, 91) ──

test('2e.89: MoE cache and Fit free VRAM carry the literal MiB after their input and their label has no MiB, and no other field of 107 carries a unit', async () => {
  const { main } = await loaded({ preset: () => reply(200, docOf(SET)) })
  for (const key of ['moe-cache-mib', 'fit-target']) {
    const f = FIELDS.find((x) => x.key === key)!
    const field = fieldOf(main, f.label)
    const units = [...field.querySelectorAll('.unit')]
    expect(units.length, `${f.key}: one unit`).toBe(1)
    expect(text(units[0]!), `${f.key}: the unit is the literal MiB`).toBe('MiB')
    // 89: after the input, and never in the label.
    const control = controlOf(main, f)
    expect(
      before(field.querySelectorAll('input')[0] ?? control, units[0]!),
      `${f.key}: the unit sits after the input`,
    ).toBe(true)
    expect(f.label, `${f.key}: the label of 110 has no unit`).not.toContain(
      'MiB',
    )
  }
  for (const f of FIELDS)
    if (f.unit === undefined)
      expect(unitsOf(main, f), `${f.key}: no unit in the table`).toEqual([])
  expect(
    cardOf(main, CARD).querySelectorAll('.unit').length,
    `${en(CARD)}: two units, of the two rows of the table`,
  ).toBe(2)
})

test('2e.87, 2h.158: the six number keys of 107 are all integer keys: the numeric form on the ranged two, none on the four layer counts', async () => {
  const { main } = await loaded({ preset: () => reply(200, docOf(SET)) })
  const numbers = FIELDS.filter((f) => f.kind === 'number')
  expect(
    numbers.map((f) => f.key),
    'the number keys of 107',
  ).toEqual([
    'n-gpu-layers',
    'n-cpu-moe',
    'moe-cache-mib',
    'n-cpu-ffn',
    'fit-ctx',
    'n-gpu-layers-draft',
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

test('2e.85, 2h.175, 2h.177, 2h.178: the controls of the Hardware card are natively disabled exactly where 2h.7 leaves the row read-only (the Fit trio, the two MoE rows, and the draft pair while the read holds no model-draft), never with aria-disabled or readonly, and every About toggle of the card stays enabled', async () => {
  const doc = docOf(SET)
  const { main } = await loaded({ preset: () => reply(200, doc) })
  const controls = [
    ...cardOf(main, CARD).querySelectorAll<HTMLElement>(
      'input, select, textarea',
    ),
  ]
  expect(controls.length, 'the controls of 107').toBe(CONTROLS)
  // Each row's own controls, and the state 2h.7 gives them.
  const fromRows = FIELDS.flatMap((f) => {
    const on = rowEnabled(f, doc.values)
    const cells =
      f.kind === 'number'
        ? [
            boxOf(groupBox(main, f.label)),
            inputOf(groupBox(main, f.label), f.label),
          ]
        : [controlOf(main, f) as HTMLInputElement]
    for (const el of cells)
      expect(
        el.disabled,
        `${f.key} (${f.enable}): ${el.tagName} ${on ? 'enabled' : 'disabled'}`,
      ).toBe(!on)
    return cells
  })
  expect(fromRows.length, 'the rows of 107 are every control of the card').toBe(
    CONTROLS,
  )
  for (const el of controls) {
    const id = el.getAttribute('id') ?? el.tagName
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

test('2e.83: the Hardware card is a D5.2 Card inside the section of the h2, after Assets and before Resource / context, and the page shows every card in the order of 83', async () => {
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

test('2e.90, 2h.177, 2h.178: the Hardware card holds no group and no h4, and the only paragraphs outside a Field are the two note rows of 2h.7 (this read has no model-draft): preset-moe-later before MoE CPU layers, preset-draft-file-off before Draft device', async () => {
  const doc = docOf(SET)
  const { main } = await loaded({ preset: () => reply(200, doc) })
  const card = cardOf(main, CARD)
  const notes = [...card.querySelectorAll('p')].filter(
    (p) => p.closest('.field') === null,
  )
  expect(
    notes.map((p) => text(p)),
    `${en(CARD)}: the note rows of 177 and 178, in reading order`,
  ).toEqual([en('preset-moe-later'), en('preset-draft-file-off')])
  const h3 = card.querySelector('h3')!
  expect(before(h3, notes[0]!), '181: after the card h3').toBe(true)
  expect(
    before(notes[0]!, fieldOf(main, en('preset-moe-cpu-layers'))),
    '178: the MoE note before MoE CPU layers',
  ).toBe(true)
  expect(
    before(notes[1]!, fieldOf(main, en('preset-draft-device'))),
    '177: the draft note before Draft device',
  ).toBe(true)
  expect(card.querySelectorAll('h4').length, `${en(CARD)}: no h4`).toBe(0)
  expect(
    card.querySelectorAll('[role="group"]').length,
    `${en(CARD)}: the fields of 107 are not grouped`,
  ).toBe(0)
})

test('2e.91, 2h.175, 2h.177: Tab in main reaches the back link, the Engine About toggle and select, then every field About toggle of the page in card and field order (the list of test/presetCards.ts), each followed by the controls 2h.7 enabled; in the Hardware card that is the two boxed numbers and the five selects, not the draft pair, the Fit trio or the MoE rows', async () => {
  const doc = docOf(SET)
  const { main } = await loaded({ preset: () => reply(200, doc) })
  expect(mainTabbable(main)).toEqual([
    en('preset-back'),
    `${en('field-about')} ${en('preset-engine')}`,
    en('preset-engine'),
    ...fieldTabStops(doc.values),
    ...saveBarTabStops(),
  ])
  // A disabled control takes no focus, so a read-only row contributes only
  // its About toggle; an enabled boxed row contributes its box and its input,
  // an enabled select its own name.
  const stops: string[] = []
  for (const f of FIELDS) {
    stops.push(`${en('field-about')} ${f.label}`)
    if (!rowEnabled(f, doc.values)) continue
    if (f.kind === 'number') stops.push(en('field-default'))
    stops.push(f.label)
  }
  expect(
    mainTabbable(cardOf(main, CARD)),
    `${en(CARD)}: the About toggles and the enabled controls, in order`,
  ).toEqual(stops)
})

// ── The strings (110) and what is out of 2e.4 (111) ────────────────────

test('2e.110: the card, every label, hint, toggle, option and the empty device option of 2e.4 show the English of the 32 new strings of 110', async () => {
  expect(NEW_STRINGS.length, 'the strings of 110').toBe(32)
  expect(new Set(NEW_STRINGS).size, 'each of them named once').toBe(32)
  for (const key of NEW_STRINGS)
    expect(
      en(key).length,
      `${key}: the test's English is not empty`,
    ).toBeGreaterThan(0)
  const { main } = await loaded({ preset: () => reply(200, docOf(SET)) })
  expect(text(cardOf(main, CARD).querySelector('h3')!), 'the h3 of 110').toBe(
    en('preset-hardware-config'),
  )
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
    if (f.kind === 'select')
      expect(
        optionTexts(selectOf(main, f.label)),
        `${f.key}: the options of the table`,
      ).toEqual(f.options)
  }
  // The two texts of 108, each in the state that shows it.
  expect(fitState(main)[1], 'not allowed: the toggle text of 110').toBe(
    en('preset-fit-not-allowed'),
  )
  cleanup()
  vi.unstubAllGlobals()
  const { main: allowed } = await loaded({
    preset: () => reply(200, exactOf({ fit: 'true' })),
  })
  expect(fitState(allowed)[1], 'allowed: the toggle text of 110').toBe(
    en('preset-fit-allowed'),
  )
  cleanup()
  vi.unstubAllGlobals()
  const { main: dev } = await loaded({
    preset: () => reply(200, exactOf({})),
  })
  expect(
    optionsOf(selectOf(dev, en('preset-draft-device')))[0]!.textContent,
    'the empty device option of 110',
  ).toBe(en('preset-device-same-as-model'))
})

test('2e.111, 2e.87: no field of 2e.4 shows a placeholder, and the card shows no text of 111: the embedded-MTP note, the slider maxima, the CPU/GPU switch, a probed device list', async () => {
  const { main } = await loaded({ preset: () => reply(200, docOf(SET)) })
  const inputs = [
    ...cardOf(main, CARD).querySelectorAll<HTMLInputElement>('input'),
  ]
  expect(inputs.length, 'the inputs of 107').toBeGreaterThan(0)
  for (const input of inputs)
    expect(
      input.getAttribute('placeholder') ?? '',
      `${input.getAttribute('id')}: no placeholder in 2e`,
    ).toBe('')
  // The reply holds the 2e.5 and 2g keys of the same preset; the card shows
  // none of the texts 111 leaves out.
  const shown = (cardOf(main, CARD).textContent ?? '').toLowerCase()
  for (const out of [
    // The embedded-MTP note of MP 620-625 (2g, it needs the model read).
    'apply only to a separate draft model file',
    'so they stay disabled',
    // The slider maxima and the blockless drafter's switch (2g).
    'slider',
    'cpu/gpu switch',
    // The probed device list (2f): the card lists no device of its own.
    'rocm0',
    'cuda0',
    'vulkan0',
  ])
    expect(shown, `the Hardware card shows no ${out}`).not.toContain(out)
})

test('2e.87, 2e.109: a value of 2e.4 shows exactly as the reply holds it, with no parsing, no trimming and no case change: a number with spaces and a leading zero, a fit-target list with spaces, an unlisted lazy-mode value and a device with spaces', async () => {
  for (const [key, value] of [
    ['n-gpu-layers', ' 99 '],
    ['n-gpu-layers', '0172032'],
    ['n-cpu-moe', '-1'],
    ['moe-cache-mib', '2048.0'],
    ['n-cpu-ffn', '00'],
    ['fit-ctx', ' 8192 '],
    ['fit-target', ' 2048, 1024 '],
  ] as [string, string][]) {
    const f = FIELDS.find((x) => x.key === key)!
    const { main } = await loaded({
      preset: () => reply(200, exactOf({ [key]: value })),
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
    preset: () =>
      reply(200, exactOf({ 'lazy-mode': ' Auto ', 'device-draft': ' ROCm1 ' })),
  })
  expect(
    selectedTextExact(selectOf(main, en('preset-lazy-tensors'))),
    'an unlisted lazy-mode value keeps its spaces and its case',
  ).toBe(' Auto ')
  expect(
    selectedTextExact(selectOf(main, en('preset-draft-device'))),
    'the device keeps its spaces and its case',
  ).toBe(' ROCm1 ')
})
