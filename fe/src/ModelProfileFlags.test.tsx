// A model profile flag reads as llama-server reads it (fe/briefs/02e-model-profile-cards.md,
// sub-slice 2e.3b, criteria 122 to 126). For a flag, an option without a value,
// the engine passes the plain flag for any value that is not falsey, the empty
// value included, and the negative flag for `off`, `disabled`, `false`, `0`
// (`common_arg_utils::is_falsey`, compared trimmed and case-sensitive). So a
// checkbox ticks and a tri-state or off / on select shows on or off by that
// reading, a flag's select lists no extra option (125), and a present value the
// configurator would not write is shown under the control by the written-as line
// (124), which a screen reader reaches: the control names that line in
// `aria-describedby` (124b). The trim is the INI grammar's, spaces
// and tabs only, not JavaScript's `trim()`. The reading
// lives in the shared renderer of fe/src/ModelProfileCards.tsx, so it applies to every
// card of 2e. The fields themselves, their labels, hints and options are
// ModelProfileCards1 and ModelProfileCards2.test.tsx; for a field that is NOT a flag the
// empty value stays absent (criterion 86, pinned there). Brief 22 IP5 left on
// the page the checkbox jinja and prefill-assistant, the tri-state
// reasoning-preserve and the off / on embeddings; the flag keys of
// `PLACEMENT_KEYS` (no-host, load-on-startup, flash-attn, kv-unified,
// mmproj-offload) and the valued key `fit` left with the Hardware card, and
// their readings of 123, 124a and 127 to 129 leave with them.
import { cleanup, render, screen, within } from '@testing-library/react'
import { afterEach, beforeEach, expect, test, vi } from 'vitest'
import App from './App.tsx'
import { CheckboxField, SelectField } from './Field.tsx'
import {
  en,
  fakeBff,
  modelProfileDoc,
  reply,
  startAt,
  type BffOptions,
} from './test/bff.ts'

const ID = 'Qwen3-27B'
const HASH = `#/profiles/model/${ID}`

/** The option texts and values of the shared selects of 1.3. */
const TRISTATE = [en('option-default'), en('option-on'), en('option-off')]
const TRISTATE_VALUES = ['default', 'true', 'false']
const OFF_ON = [en('option-off'), en('option-on')]
const OFF_ON_VALUES = ['false', 'true']

/** A value over the 20 characters a select holds (criterion 119), with capitals
 * and no space to trim: the written-as line must show it as written. */
const LONG = 'Mixed-Case-Not-Falsey-Value'

/** U+00A0, a space the INI grammar does NOT drop (criterion 122's intro), and
 * JavaScript's `trim()` does: the value that tells the two trims apart. */
const NBSP = '\u00A0'

/** The trim of the INI grammar (`common/preset.cpp`, criterion 122's intro):
 * spaces (U+0020) and tabs (U+0009) at the ends, nothing else. */
function iniTrim(value: string): string {
  return value.replace(/^[ \t]+/, '').replace(/[ \t]+$/, '')
}

/** One flag row of table 122. */
type Flag = {
  key: string
  /** The strings key of the field's label. */
  label: string
  kind: 'checkbox' | 'tristate' | 'off-on'
  /** A checkbox: the strings key of the toggle's text. */
  toggle?: string
  /** A select: the option texts and values of 1.3, its absent option. */
  options?: string[]
  optionValues?: string[]
  absentOption?: string
  /** A checkbox: the state its key absent shows (101, 102). */
  absentChecked?: boolean
  /** `load-on-startup`: the router reads it as truthy only (table 122). */
  truthyOnly?: boolean
  /** `flash-attn`: a valued option, so an unlisted value is the extra option. */
  valued?: boolean
}

/** The flag keys of table 122 built so far, by key. */
const FLAGS: Flag[] = [
  {
    key: 'jinja',
    label: 'model-profile-chat-template',
    kind: 'checkbox',
    toggle: 'model-profile-chat-template-toggle',
    absentChecked: true,
  },
  {
    key: 'prefill-assistant',
    label: 'model-profile-prefill-assistant',
    kind: 'checkbox',
    toggle: 'model-profile-prefill-assistant-toggle',
    absentChecked: true,
  },
  {
    key: 'reasoning-preserve',
    label: 'model-profile-keep-past-thinking',
    kind: 'tristate',
    options: TRISTATE,
    optionValues: TRISTATE_VALUES,
    absentOption: en('option-default'),
  },
  {
    key: 'embeddings',
    label: 'model-profile-embedding-mode',
    kind: 'off-on',
    options: OFF_ON,
    optionValues: OFF_ON_VALUES,
    absentOption: en('option-off'),
  },
  // `fit` takes a value (`-fit [on|off]`), so it is none of the three readings
  // of 122: its own reading, and the error of a value the engine refuses, are
  // 2e.3c's (127, 128), pinned in the table at the end of this file. `valued`
  // keeps it out of the flag selects of 125, and an absent `fit` is on: what
  // llama.cpp defaults to (127).
]

function flagOf(key: string): Flag {
  const f = FLAGS.find((x) => x.key === key)
  expect(f, `${key}: a flag key of table 122`).toBeTruthy()
  return f as Flag
}

/** The written-as line of table 124. */
const as_ = (value: string) => en('model-profile-written-as', value)
const EMPTY = en('model-profile-written-empty')

/** One value of one key, with what the page must show. */
type Reading = {
  key: string
  /** The value of `values`; `undefined` for a key the file does not hold. */
  value: string | undefined
  /** What the control shows: the checkbox's state, or the selected option's
   * text. */
  shows: string | boolean
  /** The written-as line's text, exactly; `undefined` for no line. */
  written: string | undefined
  /** A valued option: the value shows as one extra last option (87). */
  extra?: string
}

/** The table of criterion 126, plus one row per key of table 122. */
const READINGS: Reading[] = [
  // `jinja`, the checkbox of 101: the falsey four, and everything else on.
  { key: 'jinja', value: 'false', shows: false, written: undefined },
  { key: 'jinja', value: 'off', shows: false, written: as_('off') },
  { key: 'jinja', value: 'disabled', shows: false, written: as_('disabled') },
  { key: 'jinja', value: '0', shows: false, written: as_('0') },
  { key: 'jinja', value: 'true', shows: true, written: undefined },
  { key: 'jinja', value: 'yes', shows: true, written: as_('yes') },
  { key: 'jinja', value: '1', shows: true, written: as_('1') },
  { key: 'jinja', value: 'FALSE', shows: true, written: as_('FALSE') },
  { key: 'jinja', value: 'Off', shows: true, written: as_('Off') },
  { key: 'jinja', value: '', shows: true, written: EMPTY },
  { key: 'jinja', value: ' 0 ', shows: false, written: as_('0') },
  // The INI grammar drops tabs as it drops spaces.
  { key: 'jinja', value: '\toff\t', shows: false, written: as_('off') },
  // A value of one space trims to the empty value: on, with the empty line.
  { key: 'jinja', value: ' ', shows: true, written: EMPTY },
  // Not JavaScript's trim: U+00A0 survives, so the value is not `off` and the
  // plain flag is passed, and the line shows the value with that space.
  {
    key: 'jinja',
    value: `off${NBSP}`,
    shows: true,
    written: as_(`off${NBSP}`),
  },
  { key: 'jinja', value: ' true ', shows: true, written: undefined },
  { key: 'jinja', value: ' False ', shows: true, written: as_('False') },
  { key: 'jinja', value: undefined, shows: true, written: undefined },
  { key: 'jinja', value: LONG, shows: true, written: as_(LONG) },
  // The other two checkbox flags of 101 and 102.
  {
    key: 'prefill-assistant',
    value: '0',
    shows: false,
    written: as_('0'),
  },
  {
    key: 'prefill-assistant',
    value: '',
    shows: true,
    written: EMPTY,
  },
  {
    key: 'prefill-assistant',
    value: undefined,
    shows: true,
    written: undefined,
  },
  // `reasoning-preserve`, the other tri-state flag of 101.
  {
    key: 'reasoning-preserve',
    value: 'yes',
    shows: en('option-on'),
    written: as_('yes'),
  },
  {
    key: 'reasoning-preserve',
    value: 'Disabled',
    shows: en('option-on'),
    written: as_('Disabled'),
  },
  {
    key: 'reasoning-preserve',
    value: undefined,
    shows: en('option-default'),
    written: undefined,
  },
  // `embeddings`, the off / on flag of 93.
  { key: 'embeddings', value: '', shows: en('option-on'), written: EMPTY },
  {
    key: 'embeddings',
    value: 'off',
    shows: en('option-off'),
    written: as_('off'),
  },
  {
    key: 'embeddings',
    value: 'true',
    shows: en('option-on'),
    written: undefined,
  },
  // A value that reads as "no" in English is on: the engine sees a flag.
  {
    key: 'embeddings',
    value: 'no',
    shows: en('option-on'),
    written: as_('no'),
  },
  {
    key: 'embeddings',
    value: undefined,
    shows: en('option-off'),
    written: undefined,
  },
]

beforeEach(() => startAt(HASH))

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
  startAt('')
})

// ── Helpers (as ModelProfileCards3.test.tsx) ──────────────────────────────────

/** The app at the address in the location; returns the fake bff and main. */
function open(options: BffOptions = {}) {
  const bff = fakeBff(options)
  render(<App />)
  return { bff, main: screen.getByRole('main') }
}

/** Waits for the loaded page: the h1 of the model and the Assets card. */
async function loaded(options: BffOptions = {}) {
  const { bff, main } = open(options)
  await within(main).findByRole('heading', { level: 1, name: ID })
  await within(main).findByRole('heading', {
    level: 3,
    name: en('model-profile-assets'),
  })
  return { bff, main }
}

/** The loaded page with `values` holding exactly `values`. */
function page(values: Record<string, string>) {
  return loaded({
    modelProfile: () => reply(200, modelProfileDoc({}, { values })),
  })
}

function selectOf(main: HTMLElement, label: string): HTMLSelectElement {
  return within(main).getByRole('combobox', {
    name: label,
  }) as HTMLSelectElement
}

/** A checkbox field's checkbox: named by the table's toggle text (87). 108
 * picks between two texts for Fit to VRAM, so `toggle` names the text the page
 * shows when the row's own does not (the not-allowed state of 127). */
function checkBoxOf(
  main: HTMLElement,
  f: Flag,
  toggle?: string,
): HTMLInputElement {
  const group = within(main).getByRole('group', { name: en(f.label) })
  return within(group).getByRole('checkbox', {
    name: toggle ?? en(f.toggle ?? ''),
  }) as HTMLInputElement
}

function optionsOf(select: HTMLSelectElement): HTMLOptionElement[] {
  return [...select.querySelectorAll('option')]
}

function optionTexts(select: HTMLSelectElement): string[] {
  return optionsOf(select).map((o) => (o.textContent ?? '').trim())
}

function optionValues(select: HTMLSelectElement): string[] {
  return optionsOf(select).map((o) => o.value)
}

/** The selected option's text exactly as the page writes it: no trimming, for
 * a value the page must show as the file holds it (87). */
function selectedTextExact(select: HTMLSelectElement): string {
  expect(select.selectedIndex, 'an option is selected').toBeGreaterThanOrEqual(
    0,
  )
  return optionsOf(select)[select.selectedIndex]!.textContent ?? ''
}

/** The control of a flag: its checkbox or its select. */
function controlOf(main: HTMLElement, f: Flag): HTMLElement {
  return f.kind === 'checkbox'
    ? checkBoxOf(main, f)
    : selectOf(main, en(f.label))
}

/** The written-as line of `key`, found by the id of 124. */
function writtenOf(main: HTMLElement, key: string): HTMLElement | null {
  return main.querySelector(`#model-profile-written-${key}`)
}

/** The ids the control's `aria-describedby` names, in order (124b). */
function describedIds(control: Element): string[] {
  return (control.getAttribute('aria-describedby') ?? '')
    .split(/\s+/)
    .filter(Boolean)
}

/** What one reading of the table shows: the control, the options, the line. */
function expectReading(main: HTMLElement, r: Reading): void {
  const f = flagOf(r.key)
  const shown = JSON.stringify(r.value)
  if (f.kind === 'checkbox') {
    const box = checkBoxOf(main, f)
    expect(box.checked, `${f.key} ${shown}: the checkbox`).toBe(r.shows)
  } else {
    const select = selectOf(main, en(f.label))
    const listed = f.options ?? []
    const values = f.optionValues ?? []
    if (r.extra === undefined) {
      // 125: a flag's select keeps the options of 1.3 and adds nothing.
      expect(
        optionTexts(select),
        `${f.key} ${shown}: the options of 1.3, no extra option`,
      ).toEqual(listed)
      expect(
        optionValues(select),
        `${f.key} ${shown}: no option carries the value`,
      ).toEqual(values)
    } else {
      // 124a: the extra option's value and text are the value trimmed, as the
      // INI grammar trims it.
      expect(
        optionTexts(select),
        `${f.key} ${shown}: the listed options, then the value trimmed`,
      ).toEqual([...listed, iniTrim(r.extra)])
      expect(
        optionValues(select),
        `${f.key} ${shown}: the extra option carries the value trimmed`,
      ).toEqual([...values, iniTrim(r.extra)])
    }
    expect(
      selectedTextExact(select),
      `${f.key} ${shown}: the control shows`,
    ).toBe(r.shows)
  }
  const line = writtenOf(main, f.key)
  if (r.written === undefined)
    expect(line, `${f.key} ${shown}: no written-as line`).toBeNull()
  else
    expect(
      (line ?? { textContent: null }).textContent,
      `#model-profile-written-${f.key} ${shown}: the written-as line's text`,
    ).toBe(r.written)
  // 124b: the control names its line, so a reader hears the spelling after the
  // control's name and state; with no line it names no written-as id.
  const ids = describedIds(controlOf(main, f))
  const lineId = `model-profile-written-${f.key}`
  if (r.written === undefined)
    expect(
      ids,
      `${f.key} ${shown}: 124b: no line, so no written-as id named`,
    ).not.toContain(lineId)
  else {
    expect(
      ids,
      `${f.key} ${shown}: 124b: the control names the written-as line`,
    ).toContain(lineId)
    expect(
      main.querySelector(`#${lineId}`)?.textContent,
      `${f.key} ${shown}: 124b: the named id is the line, not a phantom id`,
    ).toBe(r.written)
  }
}

// ── 122, 123, 125: the three readings, and 124: the written-as line ─────

test.each(READINGS)(
  '2e.122, 2e.123, 2e.125, 2e.124, 2e.124b: $key with $value shows $shows and the line $written',
  async (r) => {
    const { main } = await page(
      r.value === undefined ? {} : { [r.key]: r.value },
    )
    expectReading(main, r)
  },
)

// ── 124: the line as the screen reader reaches it ───────────────────────

// The keys, each with a value the configurator would not write (a line), and
// `true` (no line): the ids named with `true` are the ids that must still be
// there beside the written-as id, so the rule is additive (the error's id,
// where there is one, stays).
const WRITTEN_LINE_IDS: [string, string][] = [
  ['jinja', 'yes'],
  ['embeddings', 'no'],
]

// No line, no written-as id: `true` is what the configurator writes (124).
const NO_LINE_VALUES: [string, string][] = [['jinja', 'true']]

test.each(WRITTEN_LINE_IDS)(
  '2e.124b: the checkbox or select names its written-as line in aria-describedby, beside every id it already names: %s = %s',
  async (key, value) => {
    const f = flagOf(key)
    const { main } = await page({ [key]: value })
    const ids = describedIds(controlOf(main, f))
    const lineId = `model-profile-written-${key}`
    expect(ids, `${key}: the control names its line`).toContain(lineId)
    cleanup()
    vi.unstubAllGlobals()
    const plain = await page({ [key]: 'true' })
    const before = describedIds(controlOf(plain.main, f))
    expect(
      ids.filter((id) => id !== lineId),
      `${key}: the line's id joins the ids already named, none dropped`,
    ).toEqual(before)
  },
)

test.each(NO_LINE_VALUES)(
  '2e.124b: with no line it names none: %s = %s',
  async (key, value) => {
    const f = flagOf(key)
    const { main } = await page({ [key]: value })
    expect(writtenOf(main, key), `${key}: no line`).toBeNull()
    expect(
      describedIds(controlOf(main, f)),
      `${key} ${value}: no line, so no written-as id named`,
    ).not.toContain(`model-profile-written-${key}`)
  },
)

// ── 124b: the tie, at its two edges ───────────────────────────────────
// The rule has an additive half (the line joins the ids already named, the
// error's among them) and a bare half (nothing to name, so no description).
// A read-only model profile page has no field error and a line only where 124 puts
// one, so the two halves are pinned here at the page for the bare one, and on
// the shared renderer of fe/src/Field.tsx for the additive one.

test('2e.124b: a control with an error and a line names the error first and the line beside it: the line never replaces an id already there', () => {
  // The additive half of 124b, on the two controls that carry a written-as
  // line. A model profile field has no error (the page is read-only), so the case is
  // reached on the shared renderer, which is where the rule lives.
  const error = en('field-error-number', '1', '10')
  const box = render(
    <CheckboxField
      id="demo-box"
      label={en('model-profile-chat-template')}
      hint={en('model-profile-chat-template-hint')}
      error={error}
      written={{ id: 'model-profile-written-jinja', text: as_('off') }}
      toggle={en('model-profile-chat-template-toggle')}
      checked={false}
      onChange={() => undefined}
    />,
  )
  const sel = render(
    <SelectField
      id="demo-select"
      label={en('model-profile-unified-kv')}
      hint={en('model-profile-unified-kv-hint')}
      error={error}
      written={{ id: 'model-profile-written-kv-unified', text: as_('auto') }}
      options={[{ value: 'true', text: en('option-on') }]}
      value="true"
      onChange={() => undefined}
    />,
  )
  for (const [what, id, control, lineId] of [
    [
      'checkbox',
      'demo-box',
      box.container.querySelector('#demo-box-control')!,
      'model-profile-written-jinja',
    ],
    [
      'select',
      'demo-select',
      sel.container.querySelector('#demo-select-control')!,
      'model-profile-written-kv-unified',
    ],
  ] as [string, string, Element, string][]) {
    expect(
      describedIds(control),
      `${what}: 124b: the error's id stays, the line's joins it after`,
    ).toEqual([`${id}-error`, lineId])
    const field = control.closest('.field')!
    for (const named of describedIds(control))
      expect(
        field.querySelector(`#${named}`),
        `${what}: the id ${named} names an element of this field`,
      ).toBeTruthy()
    expect(
      control.getAttribute('aria-invalid'),
      `${what}: an invalid control stays marked`,
    ).toBe('true')
  }
  cleanup()
})

// ── 125: no flag select shows an extra option, whatever its value ───────

test('2e.125: no flag select shows an extra option: every key of 122 with auto, yes and a long value keeps the options of 1.3 and names the spelling on the line', async () => {
  for (const f of FLAGS.filter((x) => x.kind !== 'checkbox' && !x.valued)) {
    for (const value of ['auto', 'yes', LONG]) {
      const { main } = await page({ [f.key]: value })
      const select = selectOf(main, en(f.label))
      expect(
        optionTexts(select),
        `${f.key} ${value}: the options of 1.3, no extra`,
      ).toEqual(f.options)
      // `load-on-startup` reads truthy only, and none of these three values is
      // truthy; every other flag key reads them as the plain flag, so on.
      expect(
        selectedTextExact(select),
        `${f.key} ${value}: the select shows on or off, not the value`,
      ).toBe(f.truthyOnly ? en('option-off') : en('option-on'))
      expect(
        writtenOf(main, f.key)!.textContent,
        `${f.key} ${value}: the spelling on the line`,
      ).toBe(as_(value))
      cleanup()
      vi.unstubAllGlobals()
    }
  }
})
