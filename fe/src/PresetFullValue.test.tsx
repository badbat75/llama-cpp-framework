// The full-value line of the read-only preset page (fe/briefs/02e-preset-cards.md,
// criteria 118 to 121): a value longer than its disabled control shows is
// repeated in full as text under the control, because a disabled input or
// select cannot wrap, scroll or be read past its edge. It is built once in the
// shared field renderer of fe/src/PresetCards.tsx, so every field of 2e.1 and
// 2e.2 gets it. The fields, their controls and how a value is shown are
// PresetCards1.test.tsx and PresetCards2.test.tsx; the route and the read are
// PresetPage.test.tsx. The colour and the wrap at 320 px are the designer's
// review, not jsdom: criterion 118's style is pinned as the CSS rule, the way
// App.test.tsx pins the banner's.
// The line is only for a read-only page, so the last test pins that the
// editable Server form shows none, whatever its values hold.
import { cleanup, render, screen, within } from '@testing-library/react'
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { afterEach, beforeEach, expect, test, vi } from 'vitest'
import App from './App.tsx'
import strings from './strings/en_US.json'
import {
  before,
  en,
  fakeBff,
  isHidden,
  presetDoc,
  reply,
  serverDoc,
  startAt,
  type BffOptions,
} from './test/bff.ts'

const ID = 'Qwen3-27B'
const HASH = `#/models/${ID}`
const SERVER = '#/instances/default'
const MODEL = '/var/lib/private-ai/.llama.cpp/models/Qwen3-27B-Q8_0.gguf'

/** The two sides of criterion 119's number limit: the input is 10ch wide, so
 * 8 characters fit (index.css `input.number`). */
const NUM_8 = '17203200'
const NUM_9 = '172032000'
/** The two sides of the text and select limit: 20 characters, what a control
 * holds in the field column at 320 px. The text pair keeps capitals and two
 * trailing spaces, so trimming or lower-casing changes it. */
const TEXT_20 = '{"MixedCase":TRUE}  '
const TEXT_21 = '{"MixedCaseX":TRUE}  '
const POOL_20 = 'mean-attention-pools'
const POOL_21 = 'Last-Attention-Pool-X'
/** An unlisted Pooling value past the limit, with spaces and capitals. */
const POOL_LONG = 'Last Attention Pool X  '

/** A number value a trim would change, past the number limit. */
const NUM_PAD = ' 172032000 '

const CTX = en('preset-context-size')
const RAM = en('preset-cache-ram')
const KCACHE = en('preset-k-cache-type')
const KWARGS = en('preset-chat-kwargs')

beforeEach(() => startAt(HASH))

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
  startAt('')
})

// ── Helpers (as PresetCards2.test.tsx) ──────────────────────────────────

/** The app at the address in the location; returns the fake bff and main. */
function open(options: BffOptions = {}) {
  const bff = fakeBff(options)
  render(<App />)
  return { bff, main: screen.getByRole('main') }
}

/** Waits for the loaded page's content. */
async function loaded(options: BffOptions = {}) {
  const { bff, main } = open(options)
  await within(main).findByRole('heading', { level: 1, name: ID })
  await within(main).findByRole('heading', {
    level: 3,
    name: en('preset-assets'),
  })
  return { bff, main }
}

/** The page read with `values` on top of the fixture's Asset keys. */
function page(values: Record<string, string>) {
  return loaded({ preset: () => reply(200, docOf(values)) })
}

/** A read with `values` added to the Assets keys. */
function docOf(values: Record<string, string>) {
  return presetDoc({ model: MODEL, ...values })
}

/** A read whose `values` are exactly `values` (the `{}` stub of 121). */
function exactOf(values: Record<string, string>) {
  return presetDoc({}, { values })
}

/** The field that holds the About toggle of `label` (Field.tsx `.field`). */
function fieldOf(main: HTMLElement, label: string): HTMLElement {
  const about = within(main).getByRole('button', {
    name: `${en('field-about')} ${label}`,
  })
  const field = about.closest('.field')
  expect(field, `${label}: its Field wrapper`).toBeTruthy()
  return field as HTMLElement
}

/** The control of the field named `label`, by its kind. */
function controlOf(
  main: HTMLElement,
  kind: 'number' | 'text' | 'select',
  label: string,
): HTMLElement {
  if (kind === 'select')
    return within(main).getByRole('combobox', { name: label })
  return within(main).getByRole('textbox', { name: label })
}

function selectOf(main: HTMLElement, label: string): HTMLSelectElement {
  return within(main).getByRole('combobox', {
    name: label,
  }) as HTMLSelectElement
}

/** The text of the selected option as written: `textContent`, which keeps the
 *  whitespace. The IDL `option.text` (the label the browser draws) collapses
 *  spaces, `textContent` and `value` do not, so criterion 118's "as written"
 *  comparison is this one. */
function selectedText(select: HTMLSelectElement): string {
  const options = [...select.querySelectorAll('option')]
  expect(select.selectedIndex, 'an option is selected').toBeGreaterThanOrEqual(
    0,
  )
  return options[select.selectedIndex]!.textContent ?? ''
}

/** The full-value line of `key`, found by the id criterion 118 names. */
function lineOf(main: HTMLElement, key: string): HTMLElement {
  const id = `preset-full-${key}`
  const line = main.querySelector(`#${id}`)
  expect(line, `#${id}: the full-value line`).toBeTruthy()
  return line as HTMLElement
}

function expectNoLine(main: HTMLElement, key: string): void {
  expect(
    main.querySelector(`#preset-full-${key}`),
    `${key}: no line`,
  ).toBeNull()
}

/** The line's hint, named by the field's id (Field.tsx). */
function hintOf(main: HTMLElement, key: string): HTMLElement {
  const hint = main.querySelector(`#preset-${key}-hint`)
  expect(hint, `${key}: its hint`).toBeTruthy()
  return hint as HTMLElement
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

// ── 118: the line, its text and its place ───────────────────────────────

test('2e.118: a Context size of 9 characters shows the line: a paragraph with the id preset-full-ctx-size, shown, holding the value', async () => {
  const { main } = await page({ 'ctx-size': NUM_9 })
  const line = lineOf(main, 'ctx-size')
  expect(line.tagName, 'the line is a paragraph').toBe('P')
  expect(line.textContent, 'the line holds the value').toBe(NUM_9)
  expect(isHidden(line), 'the line is shown').toBe(false)
})

test('2e.118: the line holds the value exactly as written, for a number, a text and an unlisted select value with capitals, a leading space and trailing spaces', async () => {
  const { main } = await page({
    'ctx-size': NUM_PAD,
    'chat-template-kwargs': TEXT_21,
    'cache-type-k': POOL_LONG,
  })
  // Every one of these changes when trimmed or lower-cased, so a line built
  // from a trimmed or normalised copy reads red.
  for (const value of [NUM_PAD, TEXT_21, POOL_LONG])
    expect(
      value.trim() === value && value.toLowerCase() === value,
      `${value}: trimming and lower-casing both leave it alone, so it pins nothing`,
    ).toBe(false)
  const cases: [string, 'number' | 'text' | 'select', string, string][] = [
    ['ctx-size', 'number', CTX, NUM_PAD],
    ['chat-template-kwargs', 'text', KWARGS, TEXT_21],
    ['cache-type-k', 'select', KCACHE, POOL_LONG],
  ]
  for (const [key, kind, label, value] of cases) {
    const line = lineOf(main, key)
    expect(line.textContent, `${key}: the line is the value, as written`).toBe(
      value,
    )
    const control = controlOf(main, kind, label)
    if ('value' in control)
      expect(
        control.value,
        `${key}: the line repeats what the control holds`,
      ).toBe(value)
  }
})

test('2e.118: an unlisted select value shows on the line the text of the selected option', async () => {
  const { main } = await page({ 'cache-type-k': POOL_LONG })
  const select = selectOf(main, KCACHE)
  const line = lineOf(main, 'cache-type-k')
  // The extra option of an unlisted value carries the value as its text, and
  // the line repeats that text as written. A trimmed comparison can never
  // hold: POOL_LONG ends in spaces, and the green 118 test above pins the
  // line's text to the value as the reply has it.
  expect(selectedText(select), 'the option is the value').toBe(POOL_LONG)
  expect(line.textContent, 'the line is the selected option text').toBe(
    selectedText(select),
  )
})

test('2e.118: the line sits under its own control, after its unit, before its hint, inside its own field', async () => {
  const { main } = await page({
    'cache-ram': NUM_9,
    'chat-template-kwargs': TEXT_21,
  })
  const cases: [string, 'number' | 'text', string, boolean][] = [
    ['cache-ram', 'number', RAM, true],
    ['chat-template-kwargs', 'text', KWARGS, false],
  ]
  for (const [key, kind, label, hasUnit] of cases) {
    const field = fieldOf(main, label)
    const control = controlOf(main, kind, label)
    const line = lineOf(main, key)
    expect(
      line.closest('.field'),
      `${key}: the line is inside the field of its control`,
    ).toBe(field)
    expect(before(control, line), `${key}: the line is under the control`).toBe(
      true,
    )
    const unit = field.querySelector('.unit')
    expect(unit !== null, `${key}: the unit is ${hasUnit}`).toBe(hasUnit)
    if (unit !== null)
      expect(before(unit, line), `${key}: the line is after the unit`).toBe(
        true,
      )
    expect(
      before(line, hintOf(main, key)),
      `${key}: the line is before the hint`,
    ).toBe(true)
  }
})

test('2e.118: one line per long field: each line is in its own field and a short field of the same card has none', async () => {
  const { main } = await page({
    'ctx-size': NUM_9,
    'cache-type-k': POOL_LONG,
    parallel: '8',
    'kv-unified': 'auto',
  })
  const lines = [...main.querySelectorAll('[id^="preset-full-"]')]
  expect(lines.length, 'exactly the two long fields have a line').toBe(2)
  expectNoLine(main, 'parallel')
  expectNoLine(main, 'kv-unified')
  const owners = new Set(lines.map((l) => l.closest('.field')))
  expect(owners.size, 'no field holds both lines').toBe(2)
  for (const [key, label] of [
    ['ctx-size', CTX],
    ['cache-type-k', KCACHE],
  ] as const) {
    const field = fieldOf(main, label)
    expect(
      field.querySelector(`#preset-full-${key}`),
      `${key}: its line is in its own field`,
    ).toBeTruthy()
  }
})

test('2e.118: index.css draws the line in --muted, the monospace font, 0.875rem and wrapping anywhere', () => {
  // jsdom does no layout, so the style of criterion 118 is pinned as the CSS
  // rule, as App.test.tsx pins the banner's. Comments are dropped first, so a
  // comment that quotes the properties is not a green.
  const raw = readFileSync(join(import.meta.dirname, 'index.css'), 'utf8')
  const css = raw.replace(/\/\*[\s\S]*?\*\//g, '')
  const rules = [...css.matchAll(/([^{}]+)\{([^{}]*)\}/g)].filter((m) =>
    /preset-full|full-value/.test(m[1]),
  )
  expect(
    rules.length,
    'a CSS rule whose selector names the full-value line (its id prefix preset-full)',
  ).toBeGreaterThan(0)
  const body = rules.map((m) => m[2]).join(' ')
  for (const decl of [
    /(^|;)\s*color:\s*var\(--muted\)/,
    /font-family:\s*var\(--mono\)/,
    /font-size:\s*0\.875rem/,
    /overflow-wrap:\s*anywhere/,
  ])
    expect(body, `the line's CSS has ${decl}`).toMatch(decl)
})

// ── 119: the limits ─────────────────────────────────────────────────────

test('2e.119: a number shows no line at 8 characters and its line at 9', async () => {
  expect(NUM_8.length, 'the number stub is 8 characters').toBe(8)
  expect(NUM_9.length, 'the number stub is 9 characters').toBe(9)
  const short = await page({ 'ctx-size': NUM_8 })
  expectNoLine(short.main, 'ctx-size')
  cleanup()
  const long = await page({ 'ctx-size': NUM_9 })
  expect(
    lineOf(long.main, 'ctx-size').textContent,
    'the 9-character line',
  ).toBe(NUM_9)
})

test('2e.119: a text value shows no line at 20 characters and its line at 21', async () => {
  expect(TEXT_20.length, 'the text stub is 20 characters').toBe(20)
  expect(TEXT_21.length, 'the text stub is 21 characters').toBe(21)
  const short = await page({ 'chat-template-kwargs': TEXT_20 })
  expectNoLine(short.main, 'chat-template-kwargs')
  cleanup()
  const long = await page({ 'chat-template-kwargs': TEXT_21 })
  expect(
    lineOf(long.main, 'chat-template-kwargs').textContent,
    'the 21-character line',
  ).toBe(TEXT_21)
})

test('2e.119: an unlisted select value shows no line at 20 characters and its line at 21', async () => {
  expect(POOL_20.length, 'the select stub is 20 characters').toBe(20)
  expect(POOL_21.length, 'the select stub is 21 characters').toBe(21)
  const short = await page({ 'cache-type-k': POOL_20 })
  expectNoLine(short.main, 'cache-type-k')
  cleanup()
  const long = await page({ 'cache-type-k': POOL_21 })
  expect(
    lineOf(long.main, 'cache-type-k').textContent,
    'the 21-character line',
  ).toBe(POOL_21)
})

test('2e.119: no key, no line: an empty values object shows no line on any field of the page', async () => {
  const { main } = await loaded({ preset: () => reply(200, exactOf({})) })
  expect(
    main.querySelectorAll('[id^="preset-full-"]').length,
    'no field has a line',
  ).toBe(0)
})

test('2e.119: a short value shows no line: a 1-character number, a short text and a select showing a short option text', async () => {
  const { main } = await page({
    parallel: '8',
    'chat-template-kwargs': '{"short":1}',
    'kv-unified': 'auto',
    'cache-type-k': 'mean',
  })
  expectNoLine(main, 'parallel')
  expectNoLine(main, 'chat-template-kwargs')
  expectNoLine(main, 'kv-unified')
  expectNoLine(main, 'cache-type-k')
})

// ── 120: accessibility ──────────────────────────────────────────────────

/** Three long fields of three kinds, the line of each. */
const LONG = {
  'ctx-size': NUM_9,
  'chat-template-kwargs': TEXT_21,
  'cache-type-k': POOL_LONG,
}
const SHORT = {
  'ctx-size': NUM_8,
  'chat-template-kwargs': TEXT_20,
  'cache-type-k': POOL_20,
}
const LONG_KEYS = ['ctx-size', 'chat-template-kwargs', 'cache-type-k'] as const

test('2e.120: the line is aria-hidden, so a screen reader reads the value once from the control', async () => {
  const { main } = await page(LONG)
  for (const key of LONG_KEYS)
    expect(
      lineOf(main, key).getAttribute('aria-hidden'),
      `${key}: the line is hidden from assistive technology`,
    ).toBe('true')
})

test('2e.120: the line is not the control description: nothing points its aria-describedby at the line', async () => {
  const { main } = await page(LONG)
  const named = [...main.querySelectorAll('[aria-describedby]')]
    .map((el) => el.getAttribute('aria-describedby') ?? '')
    .flatMap((ids) => ids.split(/\s+/))
  for (const key of LONG_KEYS) {
    expect(named, `${key}: no aria-describedby names the line`).not.toContain(
      `preset-full-${key}`,
    )
  }
})

test('2e.120: the line takes no focus: it is not a tab stop, and the tab order of the page is that of the same page with short values', async () => {
  const long = await page(LONG)
  const withLines = mainTabbable(long.main)
  expect(withLines.length, 'the page has tab stops').toBeGreaterThan(0)
  for (const key of LONG_KEYS) {
    const line = lineOf(long.main, key)
    expect(line.tabIndex, `${key}: the line takes no focus`).toBeLessThan(0)
    expect(
      line.matches('a[href], button, input, select, textarea, [tabindex]'),
      `${key}: the line is not a focusable element`,
    ).toBe(false)
  }
  cleanup()
  const short = await page(SHORT)
  expect(mainTabbable(short.main), 'a long value adds no tab stop').toEqual(
    withLines,
  )
})

// ── 121: the stub values, and no new string ─────────────────────────────

test('2e.121: the stub values of the brief show exactly these lines and no others', async () => {
  const cases: [Record<string, string>, string | null][] = [
    [{ 'ctx-size': '172032000' }, '172032000'],
    [{ 'ctx-size': '17203200' }, null],
    [{}, null],
    [{ 'cache-type-k': POOL_21 }, POOL_21],
    [{ 'cache-type-k': POOL_20 }, null],
    [{ 'kv-unified': 'auto' }, null],
    [
      { 'chat-template-kwargs': '{"enable_thinking":false}' },
      '{"enable_thinking":false}',
    ],
  ]
  for (const [values, want] of cases) {
    const { main } = await loaded({
      preset: () => reply(200, exactOf(values)),
    })
    if (want === null) {
      expect(
        main.querySelectorAll('[id^="preset-full-"]').length,
        `${JSON.stringify(values)}: no line anywhere`,
      ).toBe(0)
    } else {
      // The one line of the page, by its id, hidden from a screen reader, with
      // the text equal to the value.
      const lines = [...main.querySelectorAll('[id^="preset-full-"]')]
      expect(lines.length, `${JSON.stringify(values)}: one line`).toBe(1)
      const line = lines[0] as HTMLElement
      expect(line.getAttribute('aria-hidden'), 'the line is aria-hidden').toBe(
        'true',
      )
      expect(line.textContent, 'the line is the value').toBe(want)
    }
    cleanup()
  }
})

test('2e.121: no new string: what the line shows is data, not a text of the strings file', async () => {
  const { main } = await page(LONG)
  const all = Object.values(strings).join('\n')
  for (const key of LONG_KEYS) {
    const shown = lineOf(main, key).textContent ?? ''
    expect(
      all,
      `${shown}: the line is the file value, not a strings text`,
    ).not.toContain(shown)
  }
})

// ── 118: the line is only for the read-only page ────────────────────────

/** The Server form's values, each past the limit of 119 for its control: a
 *  38-character path, a 41-character URL and two 10-digit numbers (the
 *  `limits` of serverDoc allow a number that long). */
const DIR_LONG = '/var/lib/private-ai/.llama.cpp/models'
const URL_LONG = 'http://opencode-gateway.internal:18789/v1'
const INT_10 = 2147483647

/** The instance page with its Server form loaded, every field above filled. */
async function serverForm(): Promise<HTMLElement> {
  startAt(SERVER)
  fakeBff({
    server: () =>
      reply(
        200,
        serverDoc({
          ModelsDir: DIR_LONG,
          OpencodeBaseUrl: URL_LONG,
          ModelsMax: INT_10,
          ModelsMemoryMargin: INT_10,
        }),
      ),
  })
  render(<App />)
  const main = screen.getByRole('main')
  await within(main).findByRole('heading', {
    level: 3,
    name: en('server-network'),
  })
  return main
}

/** The text input of a Server field: in its group, or labelled. */
function serverBox(main: HTMLElement, label: string): HTMLInputElement {
  const g = within(main).queryByRole('group', { name: label })
  return (
    g
      ? within(g).getByRole('textbox', { name: label })
      : within(main).getByRole('textbox', { name: label })
  ) as HTMLInputElement
}

test('2e.118: the Server page shows no full-value line: its form holds a long path, a long URL and two long numbers, and shows no .full-value element and no id ending in -full', async () => {
  const main = await serverForm()
  // The values landed first: a form that never loaded shows no line for the
  // wrong reason.
  const cases: [string, string][] = [
    [en('server-models-folder'), DIR_LONG],
    [en('server-base-url'), URL_LONG],
    [en('server-max-models'), String(INT_10)],
    [en('server-memory-margin'), String(INT_10)],
  ]
  for (const [label, value] of cases)
    expect(
      serverBox(main, label).value,
      `${label}: the control holds the ${value.length} characters of 119's long value`,
    ).toBe(value)
  expect(
    main.querySelectorAll('.full-value').length,
    'an editable field shows no full-value line',
  ).toBe(0)
  const ids = [...main.querySelectorAll('[id]')]
    .map((el) => el.id)
    .filter((id) => id.endsWith('-full'))
  expect(ids, 'no element of the page carries a full-value id').toEqual([])
})
