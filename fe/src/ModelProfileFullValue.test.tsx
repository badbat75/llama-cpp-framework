// The full-value line of the model profile page (fe/briefs/02e-model-profile-cards.md,
// criteria 118 to 121): a value longer than its disabled control shows is
// repeated in full as text under the control, because a disabled input or
// select cannot wrap, scroll or be read past its edge. It is built once in the
// shared field renderer of fe/src/ModelProfileCards.tsx, so every field of 2e.1 and
// 2e.2 gets it. 2h.4's 160 and 2h.5's 164 keep the rule (134: the line is
// only for a control that is disabled now, a rule's lock of 176 included), and
// 2h.8's 183 to 186 left no read-only field anywhere on the model profile page: every
// card is editable now. So every example here is of a control a rule disables,
// one per kind: Fit free VRAM of the Hardware card as the text input and as
// the number box, both disabled while the Fit to VRAM box is off (186), and
// the Draft K-cache type of the Speculative decoding card, the select 2h.9's
// 190 disables while the model profile names no drafter. The fields, their controls
// and how a value is shown are ModelProfileCards1.test.tsx and
// ModelProfileCards2.test.tsx; the route and the read are ModelProfilePage.test.tsx. The
// colour and the wrap at 320 px are the designer's review, not jsdom:
// criterion 118's style is pinned as the CSS rule, the way App.test.tsx pins
// the banner's.
// The line is only for a disabled control, so the last test pins that the
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
  modelProfileDoc,
  reply,
  serverDoc,
  startAt,
  type BffOptions,
} from './test/bff.ts'

const ID = 'Qwen3-27B'
const HASH = `#/profiles/model/${ID}`
const SERVER = '#/profiles/server/default'
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
const SELECT_20 = 'mean-attention-pools'
const SELECT_21 = 'Last-Attention-Pool-X'
/** An unlisted select value past the limit, with spaces and capitals. The
 * select that carries it, the Draft K-cache type of the Speculative decoding
 * card, is disabled by `NO_DRAFTERS` below; the brief's pooling stub has the
 * same shape, but pooling sits on the editable Embeddings card, which shows no
 * line (134). */
const SELECT_LONG = 'Last Attention Pool X  '

/** 2h.9's 190 and 191: every group of the Speculative decoding card is locked
 * when the read names no drafter, no `model-draft` and no `draft-` entry of
 * `spec-type`, so 134 leaves the full-value line under its select. The stub's
 * own `spec-type` (`ngram-mod,draft-mtp`) opens the draft group and the
 * `ngram-mod` group, and the line goes. */
const NO_DRAFTERS = { 'spec-type': 'none' }

/** A number value a trim would change, past the number limit. */
const NUM_PAD = ' 172032000 '

const CTX = en('model-profile-context-size')
const MOE = en('model-profile-moe-cache')
/** The fields that carry the examples (134): Fit free VRAM is the text
 * `fit-target`, the Draft K-cache type the select `spec-draft-type-k` (2h.7's
 * 175 made the Hardware card's Lazy tensors and Load mode editable, so 134
 * takes their line away; 2h.9's 189 made the Speculative decoding card
 * editable, so its select keeps the line only under `NO_DRAFTERS`). */
const FIT = en('model-profile-fit-free-vram')
const MIN_CTX = en('model-profile-fit-min-context')
const DRAFT_K = en('model-profile-draft-k-cache-type')
/** The other editable number fields of the Resource / context card (2h.4). */
const SLOT = en('model-profile-per-slot-context')
const BATCH = en('model-profile-batch-size')
const UBATCH = en('model-profile-u-batch-size')

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

/** Waits for the loaded page's content. */
async function loaded(options: BffOptions = {}) {
  const { bff, main } = open(options)
  await within(main).findByRole('heading', { level: 1, name: ID })
  await within(main).findByRole('heading', {
    level: 3,
    name: en('model-profile-assets'),
  })
  return { bff, main }
}

/** The page read with `values` on top of the fixture's Asset keys. */
function page(values: Record<string, string>) {
  return loaded({ modelProfile: () => reply(200, docOf(values)) })
}

/** A read with `values` added to the Assets keys. */
function docOf(values: Record<string, string>) {
  return modelProfileDoc({ model: MODEL, ...values })
}

/** A read whose `values` are exactly `values` (the `{}` stub of 121). */
function exactOf(values: Record<string, string>) {
  return modelProfileDoc({}, { values })
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
  const id = `model-profile-full-${key}`
  const line = main.querySelector(`#${id}`)
  expect(line, `#${id}: the full-value line`).toBeTruthy()
  return line as HTMLElement
}

function expectNoLine(main: HTMLElement, key: string): void {
  expect(
    main.querySelector(`#model-profile-full-${key}`),
    `${key}: no line`,
  ).toBeNull()
}

/** The line's hint, named by the field's id (Field.tsx). */
function hintOf(main: HTMLElement, key: string): HTMLElement {
  const hint = main.querySelector(`#model-profile-${key}-hint`)
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

test('2e.118: a MoE cache of 9 characters shows the line: a paragraph with the id model-profile-full-moe-cache-mib, shown, holding the value', async () => {
  const { main } = await page({ 'moe-cache-mib': NUM_9 })
  const line = lineOf(main, 'moe-cache-mib')
  expect(line.tagName, 'the line is a paragraph').toBe('P')
  expect(line.textContent, 'the line holds the value').toBe(NUM_9)
  expect(isHidden(line), 'the line is shown').toBe(false)
})

test('2e.118: the line holds the value exactly as written, for a number, a text and an unlisted select value with capitals, a leading space and trailing spaces', async () => {
  const { main } = await page({
    'moe-cache-mib': NUM_PAD,
    'fit-target': TEXT_21,
    // 2h.8's 186: the Fit fields are editable, so the free-VRAM input is
    // disabled only while the Fit to VRAM box is off.
    fit: 'off',
    'spec-draft-type-k': SELECT_LONG,
    // 2h.9's 190: no drafter, so the draft group and its select are disabled.
    ...NO_DRAFTERS,
  })
  // Every one of these changes when trimmed or lower-cased, so a line built
  // from a trimmed or normalised copy reads red.
  for (const value of [NUM_PAD, TEXT_21, SELECT_LONG])
    expect(
      value.trim() === value && value.toLowerCase() === value,
      `${value}: trimming and lower-casing both leave it alone, so it pins nothing`,
    ).toBe(false)
  const cases: [string, 'number' | 'text' | 'select', string, string][] = [
    ['moe-cache-mib', 'number', MOE, NUM_PAD],
    ['fit-target', 'text', FIT, TEXT_21],
    ['spec-draft-type-k', 'select', DRAFT_K, SELECT_LONG],
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
  const { main } = await page({
    'spec-draft-type-k': SELECT_LONG,
    ...NO_DRAFTERS,
  })
  const select = selectOf(main, DRAFT_K)
  const line = lineOf(main, 'spec-draft-type-k')
  // The extra option of an unlisted value carries the value as its text, and
  // the line repeats that text as written. A trimmed comparison can never
  // hold: SELECT_LONG ends in spaces, and the green 118 test above pins the
  // line's text to the value as the reply has it.
  expect(selectedText(select), 'the option is the value').toBe(SELECT_LONG)
  expect(line.textContent, 'the line is the selected option text').toBe(
    selectedText(select),
  )
})

test('2e.118: the line sits under its own control, after its unit, before its hint, inside its own field', async () => {
  const { main } = await page({
    'moe-cache-mib': NUM_9,
    'fit-target': TEXT_21,
    fit: 'off',
  })
  const cases: [string, 'number' | 'text', string, boolean][] = [
    ['moe-cache-mib', 'number', MOE, true],
    ['fit-target', 'text', FIT, true],
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

test('2e.118, 2h.160, 2h.170, 2h.175, 2h.190: one line per long field: each line is in its own field, a short field of the same card has none, and no field of the editable card has one', async () => {
  const { main } = await page({
    'moe-cache-mib': NUM_9,
    'spec-draft-type-k': SELECT_LONG,
    // 2h.9's 190: the model profile names no drafter, so the draft group is locked
    // and its select keeps the line.
    ...NO_DRAFTERS,
    // The Resource / context card, every key past its limit: 160 says an
    // enabled control shows no line, so none of these has one. Parallel slots
    // is present, so its box is unticked and Unified KV is enabled too.
    'ctx-size': NUM_9,
    parallel: '817203200',
    'kv-unified': SELECT_LONG,
    'kv-unified-per-slot': NUM_9,
    // The KV cache card, editable since 2h.5's 164: 134 keeps its long
    // values off the lines.
    'cache-ram': NUM_9,
    'cache-type-k': SELECT_LONG,
    // The Advanced card, editable since 2h.5's 164: the text and the off / on
    // select show a long value the same way, so 134 keeps both off the lines.
    'chat-template-kwargs': TEXT_21,
    'load-on-startup': SELECT_LONG,
    // The Chat / reasoning card, editable since 2h.6's 170: its text, its
    // selects and its two numbers are enabled, so 134 keeps every long value
    // off the lines.
    'reasoning-budget-message': TEXT_21,
    'reasoning-effort': SELECT_LONG,
    'reasoning-budget': NUM_9,
    'n-predict': NUM_9,
    // The Hardware card, editable since 2h.7's 175 and 2h.8's 183: its
    // number's box is unticked by the key, so its input is enabled, and its
    // three selects are always enabled. The MoE pair and the Fit trio are the
    // exceptions: 178 keeps the MoE cache box disabled until 2g, and with the
    // Fit to VRAM box off 186 leaves the min-context input and the free-VRAM
    // input disabled. 134 keeps the line under a control disabled now, so these
    // three long values get one.
    'n-gpu-layers': NUM_9,
    'lazy-mode': SELECT_LONG,
    'load-mode': SELECT_LONG,
    fit: 'off',
    'fit-ctx': NUM_9,
    'fit-target': TEXT_21,
  })
  const lines = [...main.querySelectorAll('[id^="model-profile-full-"]')]
  expect(
    lines.length,
    'exactly the four long disabled fields have a line',
  ).toBe(4)
  for (const key of [
    'ctx-size',
    'parallel',
    'kv-unified',
    'kv-unified-per-slot',
    'cache-ram',
    'cache-type-k',
    'chat-template-kwargs',
    'load-on-startup',
    'reasoning-budget-message',
    'reasoning-effort',
    'reasoning-budget',
    'n-predict',
    'n-gpu-layers',
    'lazy-mode',
    'load-mode',
  ])
    expectNoLine(main, key)
  const owners = new Set(lines.map((l) => l.closest('.field')))
  expect(owners.size, 'no field holds both lines').toBe(4)
  // The four owners, each of them a control disabled right now in this fixture,
  // which is what 134 asks for: MoE cache, disabled until 2g (178); the Draft
  // K-cache type, its card editable since 189, locked by 190 with this read's
  // `NO_DRAFTERS`; and Fit min context and Fit free VRAM, locked by 186 while
  // the Fit to VRAM box is off.
  for (const [key, kind, label] of [
    ['moe-cache-mib', 'number', MOE],
    ['spec-draft-type-k', 'select', DRAFT_K],
    ['fit-ctx', 'number', MIN_CTX],
    ['fit-target', 'text', FIT],
  ] as const) {
    const field = fieldOf(main, label)
    expect(
      field.querySelector(`#model-profile-full-${key}`),
      `${key}: its line is in its own field`,
    ).toBeTruthy()
    const control = controlOf(main, kind, label) as HTMLInputElement
    expect(
      control.disabled,
      `${key}: 134: the line sits under a control that is disabled right now`,
    ).toBe(true)
  }
})

test('2h.160: in the Resource / context card only Unified KV is ever disabled with a value, a ticked box leaves its input empty, and the card shows no line', async () => {
  // Every key present, each value past the limit of 119: Parallel slots' box
  // is unticked, so Unified KV is enabled (155) and every input is enabled
  // (156). 134: an enabled control shows no line, so the card has none.
  const long = {
    'ctx-size': NUM_9,
    parallel: '817203200',
    'kv-unified': SELECT_LONG,
    'kv-unified-per-slot': NUM_9,
    'batch-size': NUM_9,
    'ubatch-size': NUM_9,
  }
  const enabled = await page(long)
  for (const label of [CTX, SLOT, BATCH, UBATCH]) {
    const input = within(enabled.main).getByRole('textbox', {
      name: label,
    }) as HTMLInputElement
    expect(
      input.disabled,
      `${label}: the input is enabled, it can scroll`,
    ).toBe(false)
  }
  expect(
    selectOf(enabled.main, en('model-profile-unified-kv')).disabled,
    'Unified KV is enabled while Parallel slots is unticked',
  ).toBe(false)
  expect(
    enabled.main.querySelectorAll('[id^="model-profile-full-"]').length,
    'an enabled control shows no line',
  ).toBe(0)
  cleanup()
  // Every key absent: every box is ticked (156), so every input is disabled
  // AND EMPTY (160: a ticked box has no value to show, so it never has a
  // line), and Unified KV is the one control disabled while holding a value.
  const dflt = await loaded({ modelProfile: () => reply(200, exactOf({})) })
  for (const label of [CTX, SLOT, BATCH, UBATCH]) {
    const input = within(dflt.main).getByRole('textbox', {
      name: label,
    }) as HTMLInputElement
    expect(input.disabled, `${label}: a ticked box disables its input`).toBe(
      true,
    )
    expect(input.value, `${label}: and leaves it empty`).toBe('')
  }
  const kv = selectOf(dflt.main, en('model-profile-unified-kv'))
  expect(
    kv.disabled,
    'Unified KV: the card control disabled with a value',
  ).toBe(true)
  expect(selectedText(kv), 'and the value it holds is what it shows').not.toBe(
    '',
  )
  expect(
    dflt.main.querySelectorAll('[id^="model-profile-full-"]').length,
    'the card shows no line: 134 needs a disabled control, 160 a long value',
  ).toBe(0)
})

test('2e.118: index.css draws the line in --muted, the monospace font, 0.875rem and wrapping anywhere', () => {
  // jsdom does no layout, so the style of criterion 118 is pinned as the CSS
  // rule, as App.test.tsx pins the banner's. Comments are dropped first, so a
  // comment that quotes the properties is not a green.
  const raw = readFileSync(join(import.meta.dirname, 'index.css'), 'utf8')
  const css = raw.replace(/\/\*[\s\S]*?\*\//g, '')
  const rules = [...css.matchAll(/([^{}]+)\{([^{}]*)\}/g)].filter((m) =>
    /model-profile-full|full-value/.test(m[1]),
  )
  expect(
    rules.length,
    'a CSS rule whose selector names the full-value line (its id prefix model-profile-full)',
  ).toBeGreaterThan(0)
  const body = rules.map((m) => m[2]).join(' ')
  for (const decl of [
    /(^|;)\s*color:\s*var\(--muted\)/,
    /font-family:\s*var\(--mono\)/,
    /font-size:\s*var\(--fs-sm\)/,
    /overflow-wrap:\s*anywhere/,
  ])
    expect(body, `the line's CSS has ${decl}`).toMatch(decl)
})

// ── 119: the limits ─────────────────────────────────────────────────────

test('2e.119: a number shows no line at 8 characters and its line at 9', async () => {
  expect(NUM_8.length, 'the number stub is 8 characters').toBe(8)
  expect(NUM_9.length, 'the number stub is 9 characters').toBe(9)
  const short = await page({ 'moe-cache-mib': NUM_8 })
  expectNoLine(short.main, 'moe-cache-mib')
  cleanup()
  const long = await page({ 'moe-cache-mib': NUM_9 })
  expect(
    lineOf(long.main, 'moe-cache-mib').textContent,
    'the 9-character line',
  ).toBe(NUM_9)
})

test('2e.119: a text value shows no line at 20 characters and its line at 21', async () => {
  expect(TEXT_20.length, 'the text stub is 20 characters').toBe(20)
  expect(TEXT_21.length, 'the text stub is 21 characters').toBe(21)
  const short = await page({ 'fit-target': TEXT_20, fit: 'off' })
  expectNoLine(short.main, 'fit-target')
  cleanup()
  const long = await page({ 'fit-target': TEXT_21, fit: 'off' })
  expect(
    lineOf(long.main, 'fit-target').textContent,
    'the 21-character line',
  ).toBe(TEXT_21)
})

test('2e.119: an unlisted select value shows no line at 20 characters and its line at 21', async () => {
  expect(SELECT_20.length, 'the select stub is 20 characters').toBe(20)
  expect(SELECT_21.length, 'the select stub is 21 characters').toBe(21)
  const short = await page({ 'spec-draft-type-k': SELECT_20, ...NO_DRAFTERS })
  expectNoLine(short.main, 'spec-draft-type-k')
  cleanup()
  const long = await page({ 'spec-draft-type-k': SELECT_21, ...NO_DRAFTERS })
  expect(
    lineOf(long.main, 'spec-draft-type-k').textContent,
    'the 21-character line',
  ).toBe(SELECT_21)
})

test('2e.119: no key, no line: an empty values object shows no line on any field of the page', async () => {
  const { main } = await loaded({ modelProfile: () => reply(200, exactOf({})) })
  expect(
    main.querySelectorAll('[id^="model-profile-full-"]').length,
    'no field has a line',
  ).toBe(0)
})

test('2e.119: a short value shows no line: a 1-character number, a short text and a select showing a short option text', async () => {
  const { main } = await page({
    parallel: '8',
    'fit-target': '{"short":1}',
    fit: 'off',
    'kv-unified': 'auto',
    'spec-draft-type-k': 'auto',
    ...NO_DRAFTERS,
  })
  expectNoLine(main, 'parallel')
  expectNoLine(main, 'fit-target')
  expectNoLine(main, 'kv-unified')
  expectNoLine(main, 'spec-draft-type-k')
})

// ── 120: accessibility ──────────────────────────────────────────────────

/** Long fields of every kind the line covers, and the short ones. */
const LONG = {
  'moe-cache-mib': NUM_9,
  // Fit min context, the number disabled by 2h.8's 186 rule, and Fit free
  // VRAM, the text disabled by the same rule.
  'fit-ctx': NUM_9,
  'fit-target': TEXT_21,
  fit: 'off',
  // The Draft K-cache type select, disabled by 190 on a read with no drafter.
  ...NO_DRAFTERS,
  'spec-draft-type-k': SELECT_LONG,
}
const SHORT = {
  'moe-cache-mib': NUM_8,
  'fit-ctx': NUM_8,
  'fit-target': TEXT_20,
  fit: 'off',
  ...NO_DRAFTERS,
  'spec-draft-type-k': SELECT_20,
}
const LONG_KEYS = [
  'moe-cache-mib',
  'fit-ctx',
  'fit-target',
  'spec-draft-type-k',
] as const

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
      `model-profile-full-${key}`,
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

const FULL_VALUE_CASES: [Record<string, string>, string | null][] = [
  [{ 'moe-cache-mib': '172032000' }, '172032000'],
  [{ 'moe-cache-mib': '17203200' }, null],
  [{}, null],
  [{ 'spec-draft-type-k': SELECT_21 }, SELECT_21],
  [{ 'spec-draft-type-k': SELECT_20 }, null],
  [{ 'kv-unified': 'auto' }, null],
  [
    // The text example of the brief's list is chat-template-kwargs, editable
    // since 2h.5's 164, so the same 25-character JSON is shown on the one
    // text field a rule disables (186).
    { 'fit-target': '{"enable_thinking":false}', fit: 'off' },
    '{"enable_thinking":false}',
  ],
]

test.each(FULL_VALUE_CASES)(
  '2e.121: the stub values of the brief show exactly these lines and no others: %s',
  async (values, want) => {
    const { main } = await loaded({
      modelProfile: () => reply(200, exactOf(values)),
    })
    if (want === null) {
      expect(
        main.querySelectorAll('[id^="model-profile-full-"]').length,
        `${JSON.stringify(values)}: no line anywhere`,
      ).toBe(0)
    } else {
      // The one line of the page, by its id, hidden from a screen reader, with
      // the text equal to the value.
      const lines = [...main.querySelectorAll('[id^="model-profile-full-"]')]
      expect(lines.length, `${JSON.stringify(values)}: one line`).toBe(1)
      const line = lines[0] as HTMLElement
      expect(line.getAttribute('aria-hidden'), 'the line is aria-hidden').toBe(
        'true',
      )
      expect(line.textContent, 'the line is the value').toBe(want)
    }
  },
)

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
    serverProfile: () =>
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
