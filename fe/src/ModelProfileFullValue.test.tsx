// The full-value line of the model profile page (fe/briefs/02e-model-profile-cards.md,
// criteria 118 to 121): a value longer than its disabled control shows is
// repeated in full as text under the control, because a disabled input or
// select cannot wrap, scroll or be read past its edge. It is built once in the
// shared field renderer of fe/src/ModelProfileCards.tsx, so every field of 2e.1 and
// 2e.2 gets it. 2h.4's 160 and 2h.5's 164 keep the rule (134: the line is
// only for a control that is disabled now, a rule's lock of 176 included), and
// 2h.8's 183 to 186 left no read-only field anywhere on the model profile page: every
// card is editable now. So every example here is of a control a rule disables.
// Brief 22 IP5 took the placement cards away, and with them every disabled text
// input and select of the page: the controls a rule still leaves disabled are
// the number rows of the Speculative decoding card's locked groups (190, 191)
// and of the Vision card while the read names no mmproj file (176), so the
// examples are number boxes. The 20-character edge of a text input or a select
// has no disabled control left on this page to show it on; the shared renderer
// keeps the rule (119). The fields, their controls
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

/** 2h.9's 190 and 191: every group of the Speculative decoding card is locked
 * when the read names no drafter, no `model-draft` and no `draft-` entry of
 * `spec-type`, and names no n-gram type, so 134 leaves the full-value line
 * under the locked group's number rows. The stub's own `spec-type`
 * (`ngram-mod,draft-mtp`) opens the draft group and the `ngram-mod` group, and
 * the lines go. */
const NO_DRAFTERS = { 'spec-type': 'none' }

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

/** The full-value line of `key`, found by the id criterion 118 names. */
function lineOf(main: HTMLElement, key: string): HTMLElement {
  const id = `model-profile-full-${key}`
  const line = main.querySelector(`#${id}`)
  expect(line, `#${id}: the full-value line`).toBeTruthy()
  return line as HTMLElement
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

test('2e.119: no key, no line: an empty values object shows no line on any field of the page', async () => {
  const { main } = await loaded({ modelProfile: () => reply(200, exactOf({})) })
  expect(
    main.querySelectorAll('[id^="model-profile-full-"]').length,
    'no field has a line',
  ).toBe(0)
})

// ── 120: accessibility ──────────────────────────────────────────────────

/** Long fields of every kind the line covers, and the short ones. All are
 * number rows of a group a rule locks: the Draft max number of the draft group
 * the `NO_DRAFTERS` spec-type locks (190), the n-gram mod max number of the
 * `ngram-mod` group the same spec-type leaves off the list (191), and the
 * Minimum image tokens row the missing `mmproj` disables (176). */
const LONG = {
  ...NO_DRAFTERS,
  'spec-draft-n-max': NUM_9,
  'spec-ngram-mod-n-max': NUM_9,
  'image-min-tokens': NUM_9,
}
const SHORT = {
  ...NO_DRAFTERS,
  'spec-draft-n-max': NUM_8,
  'spec-ngram-mod-n-max': NUM_8,
  'image-min-tokens': NUM_8,
}
const LONG_KEYS = [
  'spec-draft-n-max',
  'spec-ngram-mod-n-max',
  'image-min-tokens',
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
  [{ 'spec-draft-n-max': '172032000' }, '172032000'],
  [{ 'spec-draft-n-max': '17203200' }, null],
  [{}, null],
  // The `exactOf` stub has no `spec-type` at all, so 191 locks every n-gram
  // group and 190 the draft group: each number row is a disabled control.
  [{ 'spec-ngram-mod-n-max': NUM_9 }, NUM_9],
  [{ 'spec-ngram-mod-n-max': NUM_8 }, null],
  // The Vision card of 176: with no `mmproj` in the read, its number rows are
  // disabled, so a long one shows the line.
  [{ 'image-min-tokens': NUM_9 }, NUM_9],
  [{ 'image-min-tokens': NUM_8 }, null],
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
