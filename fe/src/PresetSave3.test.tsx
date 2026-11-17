// The keyed answers of a preset save (fe/briefs/02h-preset-save.md, sub-slice
// 2h.2): a 422 on a field (142), a refusal whose field is not a control of
// this page, or has none (143), a refusal on a control that is disabled (144),
// and the table of 150 (each check key, its field and its text). The bar, the
// editable Embeddings selects, the PUT and the plain answers are 135 to 141,
// in PresetSave1.test.tsx and PresetSave2.test.tsx, whose helpers this file
// repeats (a test file imports no other test file). 409, 404, 403, 503, 504,
// the guard's keys and the session (145 to 149, 2h.3) are not here.
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from '@testing-library/react'
import { afterEach, beforeEach, expect, it, test, vi } from 'vitest'
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

const ID = 'Qwen3-27B'
const HASH = `#/models/${ID}`
const PUT = `/api/presets/${ID}`
const MODE = en('preset-embedding-mode')
const POOLING = en('preset-pooling')

beforeEach(() => {
  startAt(HASH)
})

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
  startAt('')
})

// ── Helpers (as PresetSave2.test.tsx) ───────────────────────────────────

/** The loaded page with revision `r1`, `values`, and `answer` for the save. */
function form(
  values: Record<string, string>,
  answer?: BffOptions['presetSave'],
) {
  const bff = fakeBff({
    preset: () => reply(200, presetDoc({}, { revision: 'r1', values })),
    presetSave: answer,
  })
  render(<App />)
  const main = screen.getByRole('main')
  return { bff, main }
}

/** Waits for the loaded page, then makes it dirty (Embedding mode off). */
async function loaded(
  values: Record<string, string>,
  answer?: BffOptions['presetSave'],
) {
  const { bff, main } = form(values, answer)
  await within(main).findByRole('heading', { level: 1, name: ID })
  await within(main).findByRole('heading', {
    level: 3,
    name: en('preset-assets'),
  })
  return { bff, main }
}

/** The page, dirty: Embedding mode off, and the save answered `body`. */
async function refused(body: Record<string, unknown>) {
  const { bff, main } = await loaded(
    { embeddings: 'true', pooling: 'weird' },
    () => reply(422, body),
  )
  pick(main, MODE, 'false')
  return { bff, main }
}

function selectOf(main: HTMLElement, label: string): HTMLSelectElement {
  return within(main).getByRole('combobox', {
    name: label,
  }) as HTMLSelectElement
}

function pick(main: HTMLElement, label: string, value: string) {
  fireEvent.change(selectOf(main, label), { target: { value } })
}

/** A text field's input, named by its label (a number field is a group). */
function inputOf(main: HTMLElement, label: string): HTMLInputElement {
  const g = within(main).queryByRole('group', { name: label })
  return (
    g
      ? within(g).getByRole('textbox', { name: label })
      : within(main).getByRole('textbox', { name: label })
  ) as HTMLInputElement
}

function saveButton(main: HTMLElement): HTMLElement {
  return within(main).getByRole('button', { name: en('save-button') })
}

function discardButton(main: HTMLElement): HTMLElement {
  return within(main).getByRole('button', { name: en('save-discard') })
}

/** Clicks Save changes and waits for the PUT of this preset. */
async function save(main: HTMLElement, bff: ReturnType<typeof fakeBff>) {
  fireEvent.click(saveButton(main))
  await waitFor(() => expect(bff.to('PUT', PUT)).toHaveLength(1))
}

/** The save bar's status line (the only role="status" in the settings). */
function status(main: HTMLElement): HTMLElement {
  const h2 = within(main).getByRole('heading', {
    level: 2,
    name: en('preset-settings-title'),
  })
  const all = within(main)
    .queryAllByRole('status')
    .filter((s) => before(h2, s))
  expect(all, 'one status line once the page is loaded').toHaveLength(1)
  return all[0]!
}

/** The bar's alert reads `text` (exact): a role="alert" of its own, after the
 * status line and before the buttons (135, 01's 30a). */
function expectAlert(main: HTMLElement, text: string, what = text) {
  const el = within(main).getByText(text)
  const alert = el.closest('[role="alert"]')
  expect(alert, `${what} is in role="alert"`).not.toBe(null)
  const line = status(main)
  expect(line.contains(alert), `${what}: not inside the status line`).toBe(
    false,
  )
  expect(before(line, alert!), `${what}: after the status line`).toBe(true)
  expect(
    before(alert!, discardButton(main)),
    `${what}: before the buttons`,
  ).toBe(true)
}

function described(el: HTMLElement): string[] {
  return (el.getAttribute('aria-describedby') ?? '')
    .split(/\s+/)
    .filter((id) => id !== '')
    .map((id) => document.getElementById(id)?.textContent ?? `(no #${id})`)
}

/** Asserts `el` is marked invalid with the error `text` under it (142). */
function expectError(el: HTMLElement, text: string, what: string) {
  expect(el.getAttribute('aria-invalid'), `${what} aria-invalid`).toBe('true')
  expect(described(el), `${what} aria-describedby`).toContain(text)
  const error = (el.getAttribute('aria-describedby') ?? '')
    .split(/\s+/)
    .map((id) => document.getElementById(id))
    .find((e) => e !== null && e.textContent === text)
  expect(error, `${what}: the error is shown`).toBeDefined()
  expect(isHidden(error!), `${what}: the error is shown`).toBe(false)
  expect(before(el, error!), `${what}: the error comes after the control`).toBe(
    true,
  )
}

/** The field is not marked (142: the edit cleared it). */
function expectNoError(el: HTMLElement, what: string) {
  expect(el.getAttribute('aria-invalid') ?? 'false', what).toBe('false')
}

// ── 142: a refusal on a field ───────────────────────────────────────────

test('2h.142: a 422 on Pooling marks Pooling with its text, moves focus to it once the error is rendered, and the bar alerts the same text', async () => {
  const { bff, main } = await refused({
    key: 'not-one-of',
    args: ['pooling', 'weird', 'default, none, mean, cls, last, rank'],
    field: 'pooling',
  })
  saveButton(main).focus()
  await save(main, bff)
  const text = en(
    'api-not-one-of',
    'pooling',
    'weird',
    'default, none, mean, cls, last, rank',
  )
  await within(main).findByText(en('save-refused', text))
  const pooling = selectOf(main, POOLING)
  expectError(pooling, text, 'Pooling')
  // Focus moves to the control, once the error is rendered.
  expect(document.activeElement, 'focus on Pooling').toBe(pooling)
  expectAlert(main, en('save-refused', text), 'Pooling refused')
  // The edit and the unsaved state stay (30a).
  expect(selectOf(main, MODE).value, 'the edit stays').toBe('false')
  expect(status(main).textContent).toBe(en('save-unsaved'))
})

test('2h.142: editing the marked field clears its error and leaves the bar alert and the unsaved status', async () => {
  const { bff, main } = await refused({
    key: 'not-one-of',
    args: ['pooling', 'weird', 'default, none, mean, cls, last, rank'],
    field: 'pooling',
  })
  await save(main, bff)
  const text = en(
    'api-not-one-of',
    'pooling',
    'weird',
    'default, none, mean, cls, last, rank',
  )
  await within(main).findByText(en('save-refused', text))
  expectError(selectOf(main, POOLING), text, 'Pooling')
  // The user picks another option: its own error is gone, the alert stays.
  pick(main, POOLING, 'mean')
  const pooling = selectOf(main, POOLING)
  expectNoError(pooling, 'Pooling after the edit')
  expect(within(main).queryByText(text), 'the field error is gone').toBeNull()
  expectAlert(main, en('save-refused', text), 'after the edit')
  expect(status(main).textContent).toBe(en('save-unsaved'))
})

test('2h.142 (150): a 422 with no field marks the field of the fallback table, args[0]', async () => {
  const { bff, main } = await refused({
    key: 'comment-markers',
    args: ['pooling'],
  })
  saveButton(main).focus()
  await save(main, bff)
  const text = en('api-comment-markers', 'pooling')
  await within(main).findByText(en('save-refused', text))
  const pooling = selectOf(main, POOLING)
  expectError(pooling, text, 'Pooling (no field member)')
  expect(document.activeElement, 'focus on Pooling').toBe(pooling)
  expectAlert(main, en('save-refused', text), 'the args[0] fallback')
})

test('2h.142: the field of the reply wins over the fallback table: it marks and focuses that control, not the one args[0] names', async () => {
  // The served bff's field_of answers args[0] for these keys today, so a reply
  // naming another field is the shape 142 gives the rule for: the reply decides.
  const { bff, main } = await refused({
    key: 'not-one-of',
    args: ['pooling', 'mean', 'cls'],
    field: 'embeddings',
  })
  saveButton(main).focus()
  await save(main, bff)
  const text = en('api-not-one-of', 'pooling', 'mean', 'cls')
  await within(main).findByText(en('save-refused', text))
  const mode = selectOf(main, MODE)
  expectError(mode, text, 'Embedding mode (the reply names it)')
  expectNoError(selectOf(main, POOLING), 'Pooling (args[0], the fallback)')
  expect(document.activeElement, 'focus on Embedding mode').toBe(mode)
  expectAlert(main, en('save-refused', text), 'the reply field wins')
})

/** 150: one test per row (a row mounts the page once, so no test mounts ten):
 * the check key, the 422 body, the kind of its control, the control's label,
 * and whether that control is enabled (only an enabled one takes the focus). */
const TABLE_150: [
  string,
  Record<string, unknown>,
  'select' | 'text',
  string,
  boolean,
][] = [
  [
    'not-whole-number-in-range: args[0]',
    {
      key: 'not-whole-number-in-range',
      args: ['n-gpu-layers', '-5', '0', '1024'],
    },
    'text',
    en('preset-gpu-layers'),
    false,
  ],
  [
    'comment-markers: args[0]',
    { key: 'comment-markers', args: ['chat-template-kwargs'] },
    'text',
    en('preset-chat-kwargs'),
    false,
  ],
  [
    'not-a-number: args[0]',
    { key: 'not-a-number', args: ['n-cpu-moe', 'many'] },
    'text',
    en('preset-moe-cpu-layers'),
    false,
  ],
  [
    'not-json-object: args[0]',
    { key: 'not-json-object', args: ['chat-template-kwargs'] },
    'text',
    en('preset-chat-kwargs'),
    false,
  ],
  [
    'min-above-max: args[0]',
    {
      key: 'min-above-max',
      args: ['fit-ctx', '8192', 'ctx-size', '4096'],
    },
    'text',
    en('preset-fit-min-context'),
    false,
  ],
  [
    'fit-target-format: fit-target',
    { key: 'fit-target-format', args: ['2048,x', 'use'] },
    'text',
    en('preset-fit-free-vram'),
    false,
  ],
  [
    'moe-cache-with-tensor-split: moe-cache-mib',
    { key: 'moe-cache-with-tensor-split', args: [] },
    'text',
    en('preset-moe-cache'),
    false,
  ],
  [
    'load-on-startup-over-max: load-on-startup',
    { key: 'load-on-startup-over-max', args: ['3', '2'] },
    'select',
    en('preset-load-on-startup'),
    false,
  ],
  [
    'load-on-startup-unlimited: load-on-startup',
    { key: 'load-on-startup-unlimited', args: ['2'] },
    'select',
    en('preset-load-on-startup'),
    false,
  ],
  [
    'not-one-of: pooling (the one enabled control)',
    {
      key: 'not-one-of',
      args: ['pooling', 'weird', 'default, none, mean'],
    },
    'select',
    en('preset-pooling'),
    true,
  ],
]

it.each(TABLE_150)(
  '2h.150: %s marks the field its row names with that row text, and only an enabled control takes the focus',
  async (what, body, kind, label, enabled) => {
    const { bff, main } = await refused(body)
    saveButton(main).focus()
    await save(main, bff)
    const text = en(`api-${String(body.key)}`, ...(body.args as string[]))
    await within(main).findByText(en('save-refused', text))
    const el = kind === 'select' ? selectOf(main, label) : inputOf(main, label)
    expectError(el, text, what)
    expectAlert(main, en('save-refused', text), what)
    expect(
      document.activeElement,
      `${what}: ${enabled ? 'focus on the enabled control' : 'focus stays on Save changes'}`,
    ).toBe(enabled ? el : saveButton(main))
  },
)

// ── 143: the field is not a control of this page, or there is none ──────

/** 143: one test per row: the field of the refusal is not a control of this
 * page, or there is none, so only the bar's alert answers. */
const TABLE_143: [string, Record<string, unknown>][] = [
  ['preset-no-model (Assets)', { key: 'preset-no-model', args: [ID] }],
  ['override-no-pattern (2j)', { key: 'override-no-pattern', args: ['0'] }],
  [
    'override-no-device (2j)',
    { key: 'override-no-device', args: ['0', 'ffn.*'] },
  ],
  ['invalid-preset-id', { key: 'invalid-preset-id', args: ['Bad name!'] }],
  ['preset-name-taken', { key: 'preset-name-taken', args: ['Qwen3-32B'] }],
  [
    'a key 2h shows nowhere',
    {
      key: 'comment-markers',
      args: ['override-tensor'],
      field: 'override-tensor',
    },
  ],
]

it.each(TABLE_143)(
  '2h.143: %s: a refusal on a field with no control here, or with no field at all, shows only the bar alert and leaves the focus on Save changes',
  async (what, body) => {
    const { bff, main } = await refused(body)
    saveButton(main).focus()
    await save(main, bff)
    const text = en(`api-${String(body.key)}`, ...(body.args as string[]))
    await within(main).findByText(en('save-refused', text))
    expectAlert(main, en('save-refused', text), what)
    const invalid = [...main.querySelectorAll('[aria-invalid="true"]')]
    expect(
      invalid.map((e) => e.outerHTML),
      `${what}: no field marked`,
    ).toEqual([])
    expect(document.activeElement, `${what}: focus stays`).toBe(
      saveButton(main),
    )
    expect(status(main).textContent, what).toBe(en('save-unsaved'))
  },
)

// ── 2h.2's preamble: a 422 with a key the fe does not know ──────────────

test('2h.142 (139): a 422 with a key the fe does not know is api-other with the key in the bar, marks nothing and keeps the edits', async () => {
  const cases: [string, Record<string, unknown>][] = [
    ['no field member', { key: 'frobnicated', args: [] }],
    [
      'a field member',
      { key: 'frobnicated', args: ['pooling'], field: 'pooling' },
    ],
  ]
  for (const [what, body] of cases) {
    const { bff, main } = await refused(body)
    saveButton(main).focus()
    await save(main, bff)
    const text = en('api-other', 'frobnicated')
    await within(main).findByText(en('save-refused', text))
    expectAlert(main, en('save-refused', text), what)
    expect(
      within(main).queryByText(en('save-failed')),
      `${what}: not the generic sentence`,
    ).toBeNull()
    const invalid = [...main.querySelectorAll('[aria-invalid="true"]')]
    expect(
      invalid.map((e) => e.outerHTML),
      `${what}: no field marked, the key names no row of table 150`,
    ).toEqual([])
    expect(document.activeElement, `${what}: focus stays`).toBe(
      saveButton(main),
    )
    expect(selectOf(main, MODE).value, `${what}: the edit is kept`).toBe(
      'false',
    )
    expect(status(main).textContent, `${what}: still dirty`).toBe(
      en('save-unsaved'),
    )
    expect(bff.to('PUT', PUT), `${what}: one write`).toHaveLength(1)
    cleanup()
    vi.unstubAllGlobals()
  }
})

// ── 144: the field's control is disabled ────────────────────────────────

test('2h.144: not-one-of on cache-type-k while the KV cache card is still disabled marks that field, alerts its text, and leaves the focus on Save changes', async () => {
  const { bff, main } = await refused({
    key: 'not-one-of',
    args: ['cache-type-k', 'q8 weird', 'default, f16, q8_0, q4_0'],
    field: 'cache-type-k',
  })
  saveButton(main).focus()
  await save(main, bff)
  const text = en(
    'api-not-one-of',
    'cache-type-k',
    'q8 weird',
    'default, f16, q8_0, q4_0',
  )
  await within(main).findByText(en('save-refused', text))
  const cache = selectOf(main, en('preset-k-cache-type'))
  // The card is not enabled in this build: the control is natively disabled.
  expect(cache.disabled, 'the KV cache select is disabled').toBe(true)
  expectError(cache, text, 'K-cache type (disabled)')
  expectAlert(main, en('save-refused', text), 'the disabled KV cache field')
  // No enabled control of that field, so the focus stays where it was.
  expect(document.activeElement, 'focus stays on Save changes').toBe(
    saveButton(main),
  )
  expect(status(main).textContent).toBe(en('save-unsaved'))
})
