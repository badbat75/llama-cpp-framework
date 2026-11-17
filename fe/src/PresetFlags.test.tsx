// A preset flag reads as llama-server reads it (fe/briefs/02e-preset-cards.md,
// sub-slice 2e.3b, criteria 122 to 126). For a flag, an option without a value,
// the engine passes the plain flag for any value that is not falsey, the empty
// value included, and the negative flag for `off`, `disabled`, `false`, `0`
// (`common_arg_utils::is_falsey`, compared trimmed and case-sensitive). So a
// checkbox ticks and a tri-state or off / on select shows on or off by that
// reading, a flag's select lists no extra option (125), and a present value the
// configurator would not write is shown under the control by the written-as line
// (124), which a screen reader reaches: the control names that line in
// `aria-describedby` (124b), and a valued key's extra option needs no line, since
// the option itself shows the value (124a). The trim is the INI grammar's, spaces
// and tabs only, not JavaScript's `trim()`. Two keys read differently:
// `load-on-startup` is preset-only and the router reads it as truthy only, and
// `flash-attn` is a valued option whose extra option of 87 stays. The reading
// lives in the shared renderer of fe/src/PresetCards.tsx, so it applies to every
// card of 2e. The fields themselves, their labels, hints and options are
// PresetCards1 to PresetCards4.test.tsx; for a field that is NOT a flag the
// empty value stays absent (criterion 86, pinned there). `fit` takes a value
// (`-fit [on|off]`) and reads as llama-server reads it, with a field error for
// a value the engine refuses: sub-slice 2e.3c, criteria 127 to 129, pinned at
// the end of this file.
import {
  cleanup,
  fireEvent,
  render,
  screen,
  within,
} from '@testing-library/react'
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { afterEach, beforeEach, expect, test, vi } from 'vitest'
import App from './App.tsx'
import { CheckboxField, SelectField } from './Field.tsx'
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

const ID = 'Qwen3-27B'
const HASH = `#/models/${ID}`

/** The option texts and values of the shared selects of 1.3. */
const TRISTATE = [en('option-default'), en('option-on'), en('option-off')]
const TRISTATE_VALUES = ['default', 'true', 'false']
const OFF_ON = [en('option-off'), en('option-on')]
const OFF_ON_VALUES = ['false', 'true']

/** A value over the 20 characters a select holds (criterion 119), with capitals
 * and no space to trim: the written-as line must show it as written. */
const LONG = 'Mixed-Case-Not-Falsey-Value'

/** The same value padded with spaces: the trim, not the file's padding, is what
 * the control and its lines show (124a). */
const PADDED = `  ${LONG}  `

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
    label: 'preset-chat-template',
    kind: 'checkbox',
    toggle: 'preset-chat-template-toggle',
    absentChecked: true,
  },
  {
    key: 'prefill-assistant',
    label: 'preset-prefill-assistant',
    kind: 'checkbox',
    toggle: 'preset-prefill-assistant-toggle',
    absentChecked: true,
  },
  {
    key: 'mmproj-offload',
    label: 'preset-vision-on-gpu',
    kind: 'checkbox',
    toggle: 'preset-vision-offload-toggle',
    absentChecked: true,
  },
  {
    key: 'kv-unified',
    label: 'preset-unified-kv',
    kind: 'tristate',
    options: TRISTATE,
    optionValues: TRISTATE_VALUES,
    absentOption: en('option-default'),
  },
  {
    key: 'reasoning-preserve',
    label: 'preset-keep-past-thinking',
    kind: 'tristate',
    options: TRISTATE,
    optionValues: TRISTATE_VALUES,
    absentOption: en('option-default'),
  },
  {
    key: 'embeddings',
    label: 'preset-embedding-mode',
    kind: 'off-on',
    options: OFF_ON,
    optionValues: OFF_ON_VALUES,
    absentOption: en('option-off'),
  },
  {
    key: 'no-host',
    label: 'preset-no-host-buffer',
    kind: 'off-on',
    options: OFF_ON,
    optionValues: OFF_ON_VALUES,
    absentOption: en('option-off'),
  },
  {
    key: 'load-on-startup',
    label: 'preset-load-on-startup',
    kind: 'off-on',
    options: OFF_ON,
    optionValues: OFF_ON_VALUES,
    absentOption: en('option-off'),
    truthyOnly: true,
  },
  {
    key: 'flash-attn',
    label: 'preset-flash-attention',
    kind: 'tristate',
    options: TRISTATE,
    optionValues: TRISTATE_VALUES,
    absentOption: en('option-default'),
    valued: true,
  },
  // `fit` takes a value (`-fit [on|off]`), so it is none of the three readings
  // of 122: its own reading, and the error of a value the engine refuses, are
  // 2e.3c's (127, 128), pinned in the table at the end of this file. `valued`
  // keeps it out of the flag selects of 125, and an absent `fit` is on: what
  // llama.cpp defaults to (127).
  {
    key: 'fit',
    label: 'preset-fit-to-vram',
    kind: 'checkbox',
    toggle: 'preset-fit-allowed',
    absentChecked: true,
    valued: true,
  },
]

function flagOf(key: string): Flag {
  const f = FLAGS.find((x) => x.key === key)
  expect(f, `${key}: a flag key of table 122`).toBeTruthy()
  return f as Flag
}

/** The written-as line of table 124. */
const as_ = (value: string) => en('preset-written-as', value)
const EMPTY = en('preset-written-empty')

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
  {
    key: 'mmproj-offload',
    value: 'off',
    shows: false,
    written: as_('off'),
  },
  {
    key: 'mmproj-offload',
    value: 'TRUE',
    shows: true,
    written: as_('TRUE'),
  },
  {
    key: 'mmproj-offload',
    value: undefined,
    shows: true,
    written: undefined,
  },
  // `kv-unified`, the tri-state flag: `auto` is on with the line, not the extra
  // option of 87 (125, and the stub of 94).
  {
    key: 'kv-unified',
    value: 'auto',
    shows: en('option-on'),
    written: as_('auto'),
  },
  { key: 'kv-unified', value: '0', shows: en('option-off'), written: as_('0') },
  { key: 'kv-unified', value: '', shows: en('option-on'), written: EMPTY },
  {
    key: 'kv-unified',
    value: 'true',
    shows: en('option-on'),
    written: undefined,
  },
  {
    key: 'kv-unified',
    value: 'false',
    shows: en('option-off'),
    written: undefined,
  },
  {
    key: 'kv-unified',
    value: ' 0 ',
    shows: en('option-off'),
    written: as_('0'),
  },
  {
    key: 'kv-unified',
    value: 'TRUE',
    shows: en('option-on'),
    written: as_('TRUE'),
  },
  {
    key: 'kv-unified',
    value: undefined,
    shows: en('option-default'),
    written: undefined,
  },
  {
    key: 'kv-unified',
    value: LONG,
    shows: en('option-on'),
    written: as_(LONG),
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
  // `no-host`, the other off / on flag of 107.
  { key: 'no-host', value: '1', shows: en('option-on'), written: as_('1') },
  {
    key: 'no-host',
    value: 'disabled',
    shows: en('option-off'),
    written: as_('disabled'),
  },
  {
    key: 'no-host',
    value: undefined,
    shows: en('option-off'),
    written: undefined,
  },
  // `load-on-startup`: truthy only, so on is the short list and the empty value
  // is off (123).
  {
    key: 'load-on-startup',
    value: 'yes',
    shows: en('option-off'),
    written: as_('yes'),
  },
  {
    key: 'load-on-startup',
    value: '1',
    shows: en('option-on'),
    written: as_('1'),
  },
  {
    key: 'load-on-startup',
    value: 'on',
    shows: en('option-on'),
    written: as_('on'),
  },
  {
    key: 'load-on-startup',
    value: 'enabled',
    shows: en('option-on'),
    written: as_('enabled'),
  },
  {
    key: 'load-on-startup',
    value: 'true',
    shows: en('option-on'),
    written: undefined,
  },
  {
    key: 'load-on-startup',
    value: '',
    shows: en('option-off'),
    written: EMPTY,
  },
  {
    key: 'load-on-startup',
    value: '2',
    shows: en('option-off'),
    written: as_('2'),
  },
  {
    key: 'load-on-startup',
    value: 'TRUE',
    shows: en('option-off'),
    written: as_('TRUE'),
  },
  {
    key: 'load-on-startup',
    value: undefined,
    shows: en('option-off'),
    written: undefined,
  },
  // `flash-attn`: a valued option, so the falsey four read off, the truthy four
  // read on, and a value of neither is the extra option of 87 (122, 126).
  { key: 'flash-attn', value: '0', shows: en('option-off'), written: as_('0') },
  { key: 'flash-attn', value: '1', shows: en('option-on'), written: as_('1') },
  {
    key: 'flash-attn',
    value: 'on',
    shows: en('option-on'),
    written: as_('on'),
  },
  {
    key: 'flash-attn',
    value: 'enabled',
    shows: en('option-on'),
    written: as_('enabled'),
  },
  {
    key: 'flash-attn',
    value: ' on ',
    shows: en('option-on'),
    written: as_('on'),
  },
  {
    key: 'flash-attn',
    value: 'true',
    shows: en('option-on'),
    written: undefined,
  },
  {
    key: 'flash-attn',
    value: 'false',
    shows: en('option-off'),
    written: undefined,
  },
  // Not a flag: the empty value stays absent (86), so `default`.
  {
    key: 'flash-attn',
    value: '',
    shows: en('option-default'),
    written: undefined,
  },
  {
    key: 'flash-attn',
    value: 'auto',
    shows: 'auto',
    written: undefined,
    extra: 'auto',
  },
  // 124a: the extra option's value and text are the value trimmed (not the file's
  // padding), and no written-as line: the selected option shows the value.
  {
    key: 'flash-attn',
    value: ' auto ',
    shows: 'auto',
    written: undefined,
    extra: ' auto ',
  },
  {
    key: 'flash-attn',
    value: 'weird',
    shows: 'weird',
    written: undefined,
    extra: 'weird',
  },
  // The on and off lists are case-sensitive and compared trimmed.
  {
    key: 'flash-attn',
    value: ' TRUE ',
    shows: 'TRUE',
    written: undefined,
    extra: ' TRUE ',
  },
  {
    key: 'flash-attn',
    value: 'Off',
    shows: 'Off',
    written: undefined,
    extra: 'Off',
  },
  // 124a: past what the select holds, the value still shows on the extra
  // option, and the full-value line of 118 is pinned in the 124a test (it is
  // gone on the editable card, 2h.134).
  {
    key: 'flash-attn',
    value: LONG,
    shows: LONG,
    written: undefined,
    extra: LONG,
  },
  {
    key: 'flash-attn',
    value: PADDED,
    shows: LONG,
    written: undefined,
    extra: PADDED,
  },
  {
    key: 'flash-attn',
    value: undefined,
    shows: en('option-default'),
    written: undefined,
  },
]

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

/** Waits for the loaded page: the h1 of the model and the Assets card. */
async function loaded(options: BffOptions = {}) {
  const { bff, main } = open(options)
  await within(main).findByRole('heading', { level: 1, name: ID })
  await within(main).findByRole('heading', {
    level: 3,
    name: en('preset-assets'),
  })
  return { bff, main }
}

/** The loaded page with `values` holding exactly `values`. */
function page(values: Record<string, string>) {
  return loaded({ preset: () => reply(200, presetDoc({}, { values })) })
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
  return main.querySelector(`#preset-written-${key}`)
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
      `#preset-written-${f.key} ${shown}: the written-as line's text`,
    ).toBe(r.written)
  // 124b: the control names its line, so a reader hears the spelling after the
  // control's name and state; with no line it names no written-as id.
  const ids = describedIds(controlOf(main, f))
  const lineId = `preset-written-${f.key}`
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

test('2e.124: the line is a paragraph in its own field, after its control, not aria-hidden, taking no focus, shown, and styled as the full-value line', async () => {
  // One key per reading of 122, each with a value that gets the line. A valued
  // key's extra option needs no line (124a), so the valued key is here with a
  // value of the on / off lists, whose spelling only the line shows.
  for (const [key, value] of [
    ['jinja', 'yes'],
    ['kv-unified', 'auto'],
    ['load-on-startup', 'yes'],
    ['flash-attn', '0'],
  ] as [string, string][]) {
    const f = flagOf(key)
    const { main } = await page({ [key]: value })
    const line = writtenOf(main, key)
    expect(line, `#preset-written-${key}: the line`).toBeTruthy()
    expect(line!.tagName, `${key}: a paragraph, so no tab stop`).toBe('P')
    // 124: not aria-hidden, and shown: a checkbox or a select does not expose
    // the spelling, so this is the only place a reader can read it.
    expect(
      line!.getAttribute('aria-hidden'),
      `${key}: not aria-hidden`,
    ).toBeNull()
    expect(isHidden(line), `${key}: shown`).toBe(false)
    expect(line!.getAttribute('tabindex'), `${key}: no focus`).toBeNull()
    // Under the control, in the same field, read after it in browse order.
    const control = controlOf(main, f)
    const field = control.closest('.field')
    expect(field, `${key}: its Field`).toBeTruthy()
    expect(field!.contains(line), `${key}: the line is in the field`).toBe(true)
    expect(
      before(control, line!),
      `${key}: the line is after the control`,
    ).toBe(true)
    // 124: styled as the full-value line of 118, either by carrying its class
    // or by a CSS rule naming the written-as line with the same declarations.
    if (!line!.classList.contains('full-value')) {
      const css = readFileSync(
        join(import.meta.dirname, 'index.css'),
        'utf8',
      ).replace(/\/\*[\s\S]*?\*\//g, '')
      const rules = [...css.matchAll(/([^{}]+)\{([^{}]*)\}/g)].filter((m) =>
        /preset-written|written-as/.test(m[1]),
      )
      expect(
        rules.length,
        `${key}: a CSS rule naming the written-as line, or the full-value class`,
      ).toBeGreaterThan(0)
      const body = rules.map((m) => m[2]).join(' ')
      for (const decl of [
        /(^|;)\s*color:\s*var\(--muted\)/,
        /font-family:\s*var\(--mono\)/,
        /font-size:\s*0\.875rem/,
        /overflow-wrap:\s*anywhere/,
      ])
        expect(body, `the written-as line's CSS has ${decl}`).toMatch(decl)
    }
    cleanup()
    vi.unstubAllGlobals()
  }
})

test('2e.124, 2e.118: a flag select with a value longer than its control shows the whole value on the written-as line, and no full-value line', async () => {
  const { main } = await page({ 'kv-unified': LONG })
  expect(
    writtenOf(main, 'kv-unified')!.textContent,
    'the written-as line holds the value in full',
  ).toBe(as_(LONG))
  // The control shows the reading (on), which is short, so there is nothing for
  // the full-value line of 118 to repeat: the spelling is the written-as line's.
  expect(
    main.querySelector('#preset-full-kv-unified'),
    'no full-value line beside it',
  ).toBeNull()
  expect(
    controlOf(main, flagOf('kv-unified'))
      .closest('.field')!
      .querySelectorAll('.full-value').length,
    'one line in the field',
  ).toBe(1)
})

test('2e.124a, 2h.134, 2h.164: a valued key past what its select holds shows the value trimmed on the extra option, with no written-as line and no full-value line on the editable card', async () => {
  // 124a: the selected option already shows the value, so the written-as line
  // of 124 has nothing to add. The full-value line of 118 used to repeat what
  // the select clipped; 2h.5's 164 makes the KV cache card editable and 134
  // takes the line from an enabled control, an opened select showing its
  // whole value. Option and text show the value TRIMMED, not the file's padding.
  for (const value of [LONG, PADDED]) {
    expect(
      iniTrim(value).length,
      'the value is past the 20 characters a select holds',
    ).toBeGreaterThan(20)
    const trimmed = iniTrim(value)
    const { main } = await page({ 'flash-attn': value })
    const select = selectOf(main, en('preset-flash-attention'))
    expect(
      optionValues(select),
      `flash-attn ${JSON.stringify(value)}: the extra option carries the value trimmed`,
    ).toEqual([...TRISTATE_VALUES, trimmed])
    expect(
      selectedTextExact(select),
      `flash-attn ${JSON.stringify(value)}: the option's text is the value trimmed`,
    ).toBe(trimmed)
    expect(
      select.disabled,
      '164: the KV cache card is editable, the select is enabled',
    ).toBe(false)
    expect(
      writtenOf(main, 'flash-attn'),
      'flash-attn: the extra option shows the value, so no written-as line',
    ).toBeNull()
    expect(
      main.querySelector('#preset-full-flash-attn'),
      '134: the enabled select opens and shows the whole value, so no full-value line',
    ).toBeNull()
    expect(
      select.closest('.field')!.querySelectorAll('.full-value').length,
      'no line in the field',
    ).toBe(0)
    // 124b, 120: nothing of the kind is the control's description.
    expect(
      describedIds(select),
      'flash-attn: the select names no full-value line',
    ).not.toContain('preset-full-flash-attn')
    cleanup()
    vi.unstubAllGlobals()
  }
})

test('2e.124, 2e.124b: true and false, what the configurator writes, show no line and name no written-as id, on every flag key of 122', async () => {
  // `fit` is not in this loop: the configurator writes `on` / `off` for it, so
  // `true` and `false` are there a spelling the line shows (127), pinned in the
  // 2e.3c table at the end of this file.
  for (const spelling of ['true', 'false']) {
    const values: Record<string, string> = {}
    for (const f of FLAGS.filter((x) => x.key !== 'fit'))
      values[f.key] = spelling
    const { main } = await page(values)
    const lines = [...main.querySelectorAll('[id^="preset-written-"]')]
    expect(
      lines.map((el) => el.id),
      `${spelling}: no written-as line for the page's flags`,
    ).toEqual([])
    // 124b: with no line, no control names a written-as id.
    const named = [...main.querySelectorAll('[aria-describedby]')].flatMap(
      (el) => describedIds(el),
    )
    for (const id of named)
      expect(id, `${spelling}: no control names a written-as line`).not.toMatch(
        /^preset-written-/,
      )
    cleanup()
    vi.unstubAllGlobals()
  }
})

test('2e.124: each flag with its own value gets its own line, in its own field', async () => {
  const values: Record<string, string> = {
    jinja: 'yes',
    'kv-unified': 'auto',
    embeddings: 'no',
    'load-on-startup': 'yes',
  }
  const { main } = await page(values)
  for (const [key, value] of Object.entries(values)) {
    const f = flagOf(key)
    const field = controlOf(main, f).closest('.field')!
    const lines = [...field.querySelectorAll('[id^="preset-written-"]')].map(
      (el) => el.id,
    )
    expect(lines, `${key}: one line, in its own field`).toEqual([
      `preset-written-${key}`,
    ])
    expect(
      field.querySelector(`#preset-written-${key}`)!.textContent,
      `${key}: the value as written`,
    ).toBe(as_(value))
  }
  // A key the file does not hold has no line at all.
  expect(writtenOf(main, 'no-host'), 'no-host absent: no line').toBeNull()
})

test('2e.124b: the checkbox or select names its written-as line in aria-describedby, beside every id it already names; with no line it names none, and no control names a full-value line', async () => {
  // The keys, each with a value the configurator would not write (a line), and
  // `true` (no line): the ids named with `true` are the ids that must still be
  // there beside the written-as id, so the rule is additive (the error's id,
  // where there is one, stays).
  for (const [key, value] of [
    ['jinja', 'yes'],
    ['kv-unified', 'auto'],
    ['embeddings', 'no'],
    ['load-on-startup', 'yes'],
    ['flash-attn', '0'],
  ] as [string, string][]) {
    const f = flagOf(key)
    const { main } = await page({ [key]: value })
    const ids = describedIds(controlOf(main, f))
    const lineId = `preset-written-${key}`
    expect(ids, `${key}: the control names its line`).toContain(lineId)
    cleanup()
    vi.unstubAllGlobals()
    const plain = await page({ [key]: 'true' })
    const before = describedIds(controlOf(plain.main, f))
    expect(
      ids.filter((id) => id !== lineId),
      `${key}: the line's id joins the ids already named, none dropped`,
    ).toEqual(before)
    cleanup()
    vi.unstubAllGlobals()
  }
  // No line, no written-as id: the valued key's extra option shows the value
  // itself (124a), and `true` is what the configurator writes (124).
  for (const [key, value] of [
    ['flash-attn', 'auto'],
    ['kv-unified', 'true'],
    ['jinja', 'true'],
  ] as [string, string][]) {
    const f = flagOf(key)
    const { main } = await page({ [key]: value })
    expect(writtenOf(main, key), `${key}: no line`).toBeNull()
    expect(
      describedIds(controlOf(main, f)),
      `${key} ${value}: no line, so no written-as id named`,
    ).not.toContain(`preset-written-${key}`)
    cleanup()
    vi.unstubAllGlobals()
  }
  // 120, unchanged: the full-value line is named by no aria-describedby, on a
  // page where a full-value line and a written-as line sit side by side. The
  // valued key's example moved: 134 shows no line under an enabled control, so
  // the line here is Budget message of the read-only Chat / reasoning card.
  const { main } = await page({
    'reasoning-budget-message': '{"enable_thinking":false}',
    jinja: 'yes',
  })
  expect(
    main.querySelector('#preset-full-reasoning-budget-message'),
    'the page has a full-value line',
  ).toBeTruthy()
  expect(
    writtenOf(main, 'jinja'),
    'and a written-as line beside it',
  ).toBeTruthy()
  const named = [...main.querySelectorAll('[aria-describedby]')].flatMap((el) =>
    describedIds(el),
  )
  for (const id of named)
    expect(id, 'no control names a full-value line').not.toMatch(
      /^preset-full-/,
    )
})

// ── 124b: the tie, at its two edges ───────────────────────────────────
// The rule has an additive half (the line joins the ids already named, the
// error's among them) and a bare half (nothing to name, so no description).
// A read-only preset page has no field error and a line only where 124 puts
// one, so the two halves are pinned here at the page for the bare one, and on
// the shared renderer of fe/src/Field.tsx for the additive one.

test('2e.124b: with no written-as line and no error the control names no description: it carries no aria-describedby attribute, not an empty one', async () => {
  // An `aria-describedby=""` is a description that names nothing, not the
  // absence of one. The two controls with no line: a checkbox with the
  // spelling the configurator writes (124) and a valued key shown as its own
  // extra option (124a).
  for (const [key, value] of [
    ['jinja', 'true'],
    ['flash-attn', 'auto'],
  ] as [string, string][]) {
    const { main } = await page({ [key]: value })
    expect(writtenOf(main, key), `${key} ${value}: no line`).toBeNull()
    expect(
      controlOf(main, flagOf(key)).hasAttribute('aria-describedby'),
      `${key} ${value}: nothing to name, so no attribute`,
    ).toBe(false)
    cleanup()
    vi.unstubAllGlobals()
  }
})

test('2e.124b: a control with an error and a line names the error first and the line beside it: the line never replaces an id already there', () => {
  // The additive half of 124b, on the two controls that carry a written-as
  // line. A preset field has no error (the page is read-only), so the case is
  // reached on the shared renderer, which is where the rule lives.
  const error = en('field-error-number', '1', '10')
  const box = render(
    <CheckboxField
      id="demo-box"
      label={en('preset-chat-template')}
      hint={en('preset-chat-template-hint')}
      error={error}
      written={{ id: 'preset-written-jinja', text: as_('off') }}
      toggle={en('preset-chat-template-toggle')}
      checked={false}
      onChange={() => undefined}
    />,
  )
  const sel = render(
    <SelectField
      id="demo-select"
      label={en('preset-unified-kv')}
      hint={en('preset-unified-kv-hint')}
      error={error}
      written={{ id: 'preset-written-kv-unified', text: as_('auto') }}
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
      'preset-written-jinja',
    ],
    [
      'select',
      'demo-select',
      sel.container.querySelector('#demo-select-control')!,
      'preset-written-kv-unified',
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

// ── 122: a row whose key is not a flag keeps its own reading ────────────

test('2e.122, 2e.127, 2e.128: Fit to VRAM is none of the three readings: yes and an empty value show unchecked with the toggle text of 108, and as refused values, with the line and the field error', async () => {
  // `fit` is `-fit [on|off]`, so the plain-flag reading of 122 is not its own
  // (table 122 says so, and 2e.3c builds it): a flag reading would check the
  // box for `yes`, and would read `fit =` as absent, while the engine is given
  // `--fit ""` and refuses it (127). What 2e.3c keeps of 108 is the box
  // unchecked and the toggle text; what it adds is that the value is refused,
  // not a spelling: the written-as line, then the field error of 128.
  for (const value of ['yes', '']) {
    const { main } = await page({ fit: value })
    const box = checkBoxOf(main, flagOf('fit'))
    expect(box.checked, `fit ${JSON.stringify(value)}: unchecked`).toBe(false)
    expect(
      (box.closest('label')?.textContent ?? '').trim(),
      `fit ${JSON.stringify(value)}: the toggle text of 108, the rule untouched`,
    ).toBe(en('preset-fit-allowed'))
    expect(
      (writtenOf(main, 'fit') ?? { textContent: null }).textContent,
      `fit ${JSON.stringify(value)}: 127: the spelling the file holds`,
    ).toBe(value === '' ? EMPTY : as_(value))
    const err = errorOf(main, 'fit')
    expect(
      err,
      `fit ${JSON.stringify(value)}: 128: a refused value is an error`,
    ).toBeTruthy()
    expect(
      err!.textContent,
      `fit ${JSON.stringify(value)}: 128: the words of 129`,
    ).toContain(FIT_INVALID)
    cleanup()
    vi.unstubAllGlobals()
  }
})

// ── 2e.3c: Fit to VRAM shows what llama-server does (127, 128, 129) ────
// `fit` takes a value (`-fit [on|off]`): `common_preset::to_args` passes it as
// written and its handler (`common/arg.cpp`) sets fit on for `on`, `enabled`,
// `true`, `1` and off for `off`, `disabled`, `false`, `0`, compared trimmed
// (spaces and tabs) and case-sensitive, and refuses anything else, so
// llama-server does not start that preset. The box shows that reading, the
// absent key included (127); a refused value gets the field error of 128, not
// a spelling of its own. The allowed rule of 108 stays, and only picks the
// toggle's text.

/** One value of `fit`, with what the page must show (the table of 129). */
type FitReading = {
  /** The value of `values`; `undefined` for a key the file does not hold. */
  value: string | undefined
  /** The box's state (127). */
  checked: boolean
  /** The written-as line's text exactly, `undefined` for no line (127, 124). */
  written: string | undefined
  /** The field error's text, `undefined` for no error (128). */
  error: string | undefined
}

/** The words 129 gives for a value llama-server refuses. */
const FIT_INVALID = en('preset-fit-invalid')

/** One row per value: the four on spellings and the absent key, the four off
 * ones, the refused ones, and the trims. The INI grammar drops spaces and
 * tabs, not U+00A0, so `on` padded with tabs reads on and `on` with a U+00A0
 * is refused. */
const FIT: FitReading[] = [
  { value: undefined, checked: true, written: undefined, error: undefined },
  { value: 'on', checked: true, written: undefined, error: undefined },
  {
    value: 'enabled',
    checked: true,
    written: as_('enabled'),
    error: undefined,
  },
  { value: 'true', checked: true, written: as_('true'), error: undefined },
  { value: '1', checked: true, written: as_('1'), error: undefined },
  { value: 'off', checked: false, written: undefined, error: undefined },
  {
    value: 'disabled',
    checked: false,
    written: as_('disabled'),
    error: undefined,
  },
  { value: 'false', checked: false, written: as_('false'), error: undefined },
  { value: '0', checked: false, written: as_('0'), error: undefined },
  // Refused: the engine's two lists are case-sensitive, so these do not start
  // the preset: the box is unchecked and the field says so (128).
  { value: 'TRUE', checked: false, written: as_('TRUE'), error: FIT_INVALID },
  { value: 'yes', checked: false, written: as_('yes'), error: FIT_INVALID },
  // `fit =` reaches the engine as `--fit ""` and is refused: the absent key of
  // 127 is not the empty value, and criterion 86 does not apply to `fit`.
  { value: '', checked: false, written: EMPTY, error: FIT_INVALID },
  // The trim: the value reads on, and the line shows it trimmed, since the
  // file's padding is not the spelling.
  { value: ' on ', checked: true, written: as_('on'), error: undefined },
  { value: '\ton\t', checked: true, written: as_('on'), error: undefined },
  // U+00A0 survives the trim, so the value is not `on`: refused.
  {
    value: `on${NBSP}`,
    checked: false,
    written: as_(`on${NBSP}`),
    error: FIT_INVALID,
  },
]

/** The field error of `key`, by the id 128 gives it (`<id>-error`). */
function errorOf(main: HTMLElement, key: string): HTMLElement | null {
  return main.querySelector(`#preset-${key}-error`)
}

/** What one row of the 2e.3c table shows: the box, its line, its error. */
function expectFit(main: HTMLElement, r: FitReading): void {
  const shown = JSON.stringify(r.value)
  const box = checkBoxOf(main, flagOf('fit'))
  // 129: the box stays disabled in every row.
  expect(box.disabled, `fit ${shown}: the box is disabled`).toBe(true)
  // 127: the box shows what llama-server does.
  expect(box.checked, `fit ${shown}: 127: the box`).toBe(r.checked)
  // 108, untouched: Context size and GPU layers are both absent in these rows,
  // so the toggle text is the allowed one.
  expect(
    (box.closest('label')?.textContent ?? '').trim(),
    `fit ${shown}: 108: the toggle text`,
  ).toBe(en('preset-fit-allowed'))
  // 127, 124: the written-as line of the spelling the file holds.
  const line = writtenOf(main, 'fit')
  if (r.written === undefined)
    expect(line, `fit ${shown}: 127: no written-as line`).toBeNull()
  else
    expect(
      (line ?? { textContent: null }).textContent,
      `fit ${shown}: 127: #preset-written-fit`,
    ).toBe(r.written)
  // 128: the field error of this field, and no other field's in 2e.
  const err = errorOf(main, 'fit')
  expect(
    [...main.querySelectorAll('.field-error')].map((el) => el.id),
    `fit ${shown}: 128: this field's error, alone on the page`,
  ).toEqual(r.error === undefined ? [] : ['preset-fit-error'])
  if (r.error !== undefined) {
    expect(err, `fit ${shown}: 128: #preset-fit-error`).toBeTruthy()
    expect(
      (err!.textContent ?? '').trim(),
      `fit ${shown}: 128: the words of 129`,
    ).toBe(r.error)
    expect(err!.tagName, `fit ${shown}: 128: a paragraph`).toBe('P')
    // 128: not the alert of an action: the page as read carries it.
    expect(err!.getAttribute('role'), `fit ${shown}: 128: no role`).toBeNull()
    expect(
      main.querySelectorAll('[role="alert"]').length,
      `fit ${shown}: 128: no alert on load`,
    ).toBe(0)
    // 128: the icon and the text carry it, not the colour alone, and the
    // paragraph is the field error of DESIGN 5.2, in --danger.
    const icon = err!.querySelector('svg.error-icon')
    expect(icon, `fit ${shown}: 128: the error icon`).toBeTruthy()
    expect(
      icon!.getAttribute('aria-hidden'),
      `fit ${shown}: 128: the icon carries no text of its own`,
    ).toBe('true')
    expect(
      err!.classList.contains('field-error'),
      `fit ${shown}: 128: the field error of DESIGN 5.2`,
    ).toBe(true)
    const css = readFileSync(
      join(import.meta.dirname, 'index.css'),
      'utf8',
    ).replace(/\/\*[\s\S]*?\*\//g, '')
    expect(
      [...css.matchAll(/([^{}]+)\{([^{}]*)\}/g)].some(
        (m) =>
          /(^|,)\s*\.field-error\s*(,|$)/.test(m[1]!) &&
          /color:\s*var\(--danger\)/.test(m[2]!),
      ),
      `fit ${shown}: 128: the field error is shown in --danger`,
    ).toBe(true)
    // 128: in the field, after the control, and after the written-as line.
    const field = box.closest('.field')
    expect(field, `fit ${shown}: its Field`).toBeTruthy()
    expect(field!.contains(err), `fit ${shown}: 128: in the field`).toBe(true)
    expect(
      before(box, err!),
      `fit ${shown}: 128: the error is after the control`,
    ).toBe(true)
    if (line !== null)
      expect(
        before(line, err!),
        `fit ${shown}: 128: the line comes before the error`,
      ).toBe(true)
  }
  // 124b, 128: the control names the error first and the line beside it, and
  // with neither there is no description at all.
  const ids: string[] = []
  if (r.error !== undefined) ids.push('preset-fit-error')
  if (r.written !== undefined) ids.push('preset-written-fit')
  expect(describedIds(box), `fit ${shown}: 124b, 128: what it names`).toEqual(
    ids,
  )
  expect(
    box.getAttribute('aria-invalid'),
    `fit ${shown}: 128: aria-invalid only with the error`,
  ).toBe(r.error === undefined ? null : 'true')
}

test.each(FIT)(
  '2e.127, 2e.128, 2e.129: fit $value: the box $checked, the line $written, the error $error',
  async (r) => {
    const { main } = await page(r.value === undefined ? {} : { fit: r.value })
    expectFit(main, r)
  },
)

// ── 127: the allowed rule of 108 only picks the toggle's text ──────────

test('2e.127, 2e.108, 2e.129: Fit to VRAM shows what the engine does where the configurator is not allowed: the four combinations of the box and the two toggle texts, and the error follows the value, not the rule', async () => {
  // 108's rule stands for the toggle text; 127 takes from it the power to
  // uncheck the box: `fit` on with Context size set, or with GPU layers set, is
  // checked with the not-allowed text, and an absent `fit` is checked there
  // too, the engine's own default. A refused value keeps its error beside the
  // not-allowed text.
  for (const [values, checked, toggleKey, hasError] of [
    [{ fit: 'on' }, true, 'preset-fit-allowed', false],
    [{ fit: 'off' }, false, 'preset-fit-allowed', false],
    [{ fit: 'on', 'ctx-size': '8192' }, true, 'preset-fit-not-allowed', false],
    [
      { fit: 'on', 'n-gpu-layers': '99' },
      true,
      'preset-fit-not-allowed',
      false,
    ],
    [{ 'n-gpu-layers': '99' }, true, 'preset-fit-not-allowed', false],
    [
      { fit: 'off', 'ctx-size': '172032', 'n-gpu-layers': '99' },
      false,
      'preset-fit-not-allowed',
      false,
    ],
    [
      { fit: 'TRUE', 'ctx-size': '8192' },
      false,
      'preset-fit-not-allowed',
      true,
    ],
  ] as [Record<string, string>, boolean, string, boolean][]) {
    const { main } = await page(values)
    const box = checkBoxOf(main, flagOf('fit'), en(toggleKey))
    const shown = JSON.stringify(values)
    expect(box.checked, `${shown}: 127: the box`).toBe(checked)
    expect(
      (box.closest('label')?.textContent ?? '').trim(),
      `${shown}: 108: the toggle text`,
    ).toBe(en(toggleKey))
    expect(
      errorOf(main, 'fit') !== null,
      `${shown}: 128: the error is the value's, not the rule's`,
    ).toBe(hasError)
    cleanup()
    vi.unstubAllGlobals()
  }
})

// ── 129a: the Fit to VRAM hint reads as the box does ───────────────────

/** The hint's four lines as 129a writes them, taken from the brief: with 127
 * an absent `fit` shows checked, so the hint can no longer say the default is
 * off here. Only the second line changed, the other three stay as they are. */
const FIT_HINT = [
  'Lets llama.cpp pick the context size and GPU layers that fit your GPU memory, leaving about 1 GiB free on each GPU.',
  'On by default, as in llama.cpp. For a full fit, leave Context size and GPU layers at default.',
  'When on, also leave tensor-split, Tensor placement, MoE CPU layers and Dense FFN CPU layers empty: with any of them llama.cpp fits only the context (the log warns), which can over-fill one card.',
  'Flag: -fit',
]

test('2e.129a: the opened Fit to VRAM hint shows the four lines of 129a, and says nothing about a default that is off here', async () => {
  // The hint opens the way the other hint tests of the preset page open one
  // (PresetCards4.test.tsx, criterion 88): the About toggle of the field, then
  // the element it names.
  const { main } = await page({})
  const about = within(main).getByRole('button', {
    name: `${en('field-about')} ${en('preset-fit-to-vram')}`,
  })
  const hintId = about.getAttribute('aria-controls')
  expect(hintId, 'fit: aria-controls names the hint').toBe('preset-fit-hint')
  const hint = document.getElementById(hintId!)
  expect(hint, 'fit: the hinted element is in the page').toBeTruthy()
  expect(isHidden(hint), 'fit: the hint is hidden before the click').toBe(true)
  fireEvent.click(about)
  expect(about.getAttribute('aria-expanded'), 'fit: the hint is open').toBe(
    'true',
  )
  expect(isHidden(hint), 'fit: the hint shows').toBe(false)
  const shown = [...hint!.querySelectorAll('p')].map((p) =>
    (p.textContent ?? '').trim(),
  )
  // 88: one paragraph per line of the strings text.
  expect(shown.length, 'fit: one paragraph per line').toBe(FIT_HINT.length)
  // 129a: the second line, the one that changes, as the brief writes it.
  expect(shown[1], 'fit: 129a: the second line').toBe(FIT_HINT[1])
  // 129a: the other three lines are untouched.
  expect(shown, 'fit: 129a: the hint as the user reads it').toEqual(FIT_HINT)
  // 129a: the words the checked box contradicts are gone from the hint.
  expect(
    hint!.textContent ?? '',
    'fit: 129a: no "off by default here" left in the hint',
  ).not.toContain('Off by default here')
})
