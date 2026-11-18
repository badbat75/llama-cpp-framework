// The keyed answers of a model profile save (fe/briefs/02h-model-profile-save.md, sub-slice
// 2h.2): a 422 on a field (142), a refusal whose field is not a control of
// this page, or has none (143), a refusal on a control that is disabled (144),
// and the table of 150 (each check key, its field and its text). The bar, the
// editable Embeddings selects, the PUT and the plain answers are 135 to 141,
// in ModelProfileSave1.test.tsx and ModelProfileSave2.test.tsx, whose helpers this file
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
  modelProfileDoc,
  reply,
  startAt,
  type BffOptions,
} from './test/bff.ts'

const ID = 'Qwen3-27B'
const HASH = `#/models/${ID}`
const PUT = `/api/profiles/model/${ID}`
const MODE = en('model-profile-embedding-mode')
const POOLING = en('model-profile-pooling')

beforeEach(() => {
  startAt(HASH)
})

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
  startAt('')
})

// ── Helpers (as ModelProfileSave2.test.tsx) ───────────────────────────────────

/** The loaded page with revision `r1`, `values`, and `answer` for the save. */
function form(
  values: Record<string, string>,
  answer?: BffOptions['modelProfileSave'],
) {
  const bff = fakeBff({
    modelProfile: () =>
      reply(200, modelProfileDoc({}, { revision: 'r1', values })),
    modelProfileSave: answer,
  })
  render(<App />)
  const main = screen.getByRole('main')
  return { bff, main }
}

/** Waits for the loaded page, then makes it dirty (Embedding mode off). */
async function loaded(
  values: Record<string, string>,
  answer?: BffOptions['modelProfileSave'],
) {
  const { bff, main } = form(values, answer)
  await within(main).findByRole('heading', { level: 1, name: ID })
  await within(main).findByRole('heading', {
    level: 3,
    name: en('model-profile-assets'),
  })
  return { bff, main }
}

/** The page, dirty: Embedding mode off, and the save answered `body`.
 * `n-gpu-layers` and `fit-ctx` are in the read so the Hardware card's 157 row
 * has an enabled control to mark and focus (2h.7's 175 made GPU layers
 * editable and 2h.8's 183 the Fit min context, and the keys leave their default
 * boxes unticked, 156). `fit-ctx` is the value the `min-above-max` row below
 * names in its args, the number the manager refused. */
async function refused(body: Record<string, unknown>) {
  const { bff, main } = await loaded(
    {
      embeddings: 'true',
      pooling: 'weird',
      'n-gpu-layers': '99',
      'fit-ctx': '8192',
    },
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

/** A number field's default box, named by the box's own text inside the group
 * of its label (87). */
function boxOf(main: HTMLElement, label: string): HTMLInputElement {
  const g = within(main).getByRole('group', { name: label })
  return within(g).getByRole('checkbox', {
    name: en('field-default'),
  }) as HTMLInputElement
}

function saveButton(main: HTMLElement): HTMLElement {
  return within(main).getByRole('button', { name: en('save-button') })
}

function discardButton(main: HTMLElement): HTMLElement {
  return within(main).getByRole('button', { name: en('save-discard') })
}

/** Clicks Save changes and waits for the PUT of this model profile. */
async function save(main: HTMLElement, bff: ReturnType<typeof fakeBff>) {
  fireEvent.click(saveButton(main))
  await waitFor(() => expect(bff.to('PUT', PUT)).toHaveLength(1))
}

/** The save bar's status line (the only role="status" in the settings). */
function status(main: HTMLElement): HTMLElement {
  const h2 = within(main).getByRole('heading', {
    level: 2,
    name: en('model-profile-settings-title'),
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
 * whether that control is enabled (only an enabled one takes the focus), and
 * for 2h.10's 198a the title of the card the field sits on: set where the
 * field's control AND its default box are both disabled, so the focus has
 * nowhere to go and the bar's alert must say where the field is. */
const TABLE_150: [
  string,
  Record<string, unknown>,
  'select' | 'text',
  string,
  boolean,
  string,
][] = [
  [
    'not-whole-number-in-range: args[0]',
    {
      key: 'not-whole-number-in-range',
      args: ['n-gpu-layers', '-5', '0', '1024'],
    },
    'text',
    en('model-profile-gpu-layers'),
    // 2h.7's 175 made GPU layers editable and the read has its key, so the
    // input is enabled and takes the focus (144).
    true,
    '',
  ],
  [
    'comment-markers: args[0]',
    { key: 'comment-markers', args: ['chat-template-kwargs'] },
    'text',
    en('model-profile-chat-kwargs'),
    true,
    '',
  ],
  [
    'not-a-number: args[0]',
    { key: 'not-a-number', args: ['n-cpu-moe', 'many'] },
    'text',
    en('model-profile-moe-cpu-layers'),
    // 178: MoE CPU layers is never editable in 2h, so its control and its
    // default box are both disabled: 198a's where.
    false,
    en('model-profile-hardware-config'),
  ],
  [
    'not-json-object: args[0]',
    { key: 'not-json-object', args: ['chat-template-kwargs'] },
    'text',
    en('model-profile-chat-kwargs'),
    true,
    '',
  ],
  [
    'min-above-max: args[0]',
    {
      key: 'min-above-max',
      args: ['fit-ctx', '8192', 'ctx-size', '4096'],
    },
    'text',
    en('model-profile-fit-min-context'),
    // 2h.8's 183 made Fit min context editable: the read's key leaves its box
    // unticked and 186 leaves the field enabled while Fit to VRAM reads
    // checked, so its input is enabled and takes the focus (144).
    true,
    '',
  ],
  [
    'fit-target-format: fit-target',
    { key: 'fit-target-format', args: ['2048,x', 'use'] },
    'text',
    en('model-profile-fit-free-vram'),
    // 2h.8's 183 made Fit free VRAM editable and 186 leaves it enabled while
    // Fit to VRAM reads checked, which it does with no `fit` in the read
    // (2h.127), so its input is enabled and takes the focus.
    true,
    '',
  ],
  [
    'moe-cache-with-tensor-split: moe-cache-mib',
    { key: 'moe-cache-with-tensor-split', args: [] },
    'text',
    en('model-profile-moe-cache'),
    // 178, as the row above, and 198a's own example: the manager refuses the
    // whole file on it (Split mode is not on the page in 2h).
    false,
    en('model-profile-hardware-config'),
  ],
  [
    'load-on-startup-over-max: load-on-startup',
    { key: 'load-on-startup-over-max', args: ['3', '2'] },
    'select',
    en('model-profile-load-on-startup'),
    true,
    '',
  ],
  [
    'load-on-startup-unlimited: load-on-startup',
    { key: 'load-on-startup-unlimited', args: ['2'] },
    'select',
    en('model-profile-load-on-startup'),
    true,
    '',
  ],
  [
    'not-one-of: pooling (the one enabled control)',
    {
      key: 'not-one-of',
      args: ['pooling', 'weird', 'default, none, mean'],
    },
    'select',
    en('model-profile-pooling'),
    true,
    '',
  ],
]

it.each(TABLE_150)(
  '2h.150 (2h.10 198a): %s marks the field its row names with that row text, and only an enabled control takes the focus; a field with nothing to focus is answered save-refused-at, naming its label and its card',
  async (what, body, kind, label, enabled, card) => {
    const { bff, main } = await refused(body)
    saveButton(main).focus()
    await save(main, bff)
    const text = en(`api-${String(body.key)}`, ...(body.args as string[]))
    const alert = enabled
      ? en('save-refused', text)
      : en('save-refused-at', text, label, card)
    await within(main).findByText(alert)
    const el = kind === 'select' ? selectOf(main, label) : inputOf(main, label)
    expectError(el, text, what)
    expectAlert(main, alert, what)
    if (!enabled) {
      // 198a's scope: nothing in the field can take focus, so the alert names
      // where it is, and never the plain 144 sentence.
      expect(
        boxOf(main, label).disabled,
        `${what}: 198a, the default box is disabled as well`,
      ).toBe(true)
      expect(
        within(main).queryByText(en('save-refused', text)),
        `${what}: 198a, not the alert that names no place`,
      ).toBeNull()
    }
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
  [
    'model-profile-no-model (Assets)',
    { key: 'model-profile-no-model', args: [ID] },
  ],
  ['override-no-pattern (2j)', { key: 'override-no-pattern', args: ['0'] }],
  [
    'override-no-device (2j)',
    { key: 'override-no-device', args: ['0', 'ffn.*'] },
  ],
  [
    'invalid-model-profile-id',
    { key: 'invalid-model-profile-id', args: ['Bad name!'] },
  ],
  [
    'model-profile-name-taken',
    { key: 'model-profile-name-taken', args: ['Qwen3-32B'] },
  ],
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

test('2h.144 (2h.6 170, 2h.10 198a): not-one-of on Reasoning effort, an editable select, marks it and moves focus to it and the bar keeps save-refused; on Unified KV, disabled by 155 with no box of its own, the error goes on the control, the focus stays on Save changes and the alert says on which card the field is', async () => {
  const ARGS = [
    'reasoning-effort',
    'ultra',
    'default, minimal, low, medium, high, xhigh, max',
  ]
  const { bff, main } = await refused({
    key: 'not-one-of',
    args: ARGS,
    field: 'reasoning-effort',
  })
  saveButton(main).focus()
  await save(main, bff)
  const text = en('api-not-one-of', ...ARGS)
  await within(main).findByText(en('save-refused', text))
  const effort = selectOf(main, en('model-profile-reasoning-effort'))
  // 170: the Chat / reasoning card is editable, so 142 applies: the marked
  // control is enabled and takes the focus.
  expect(effort.disabled, 'the Reasoning effort select is enabled').toBe(false)
  expectError(effort, text, 'Reasoning effort (enabled)')
  expect(document.activeElement, 'focus on Reasoning effort').toBe(effort)
  expectAlert(main, en('save-refused', text), 'the Reasoning effort field')
  expect(status(main).textContent).toBe(en('save-unsaved'))
  cleanup()
  vi.unstubAllGlobals()
  // The other half of 144: a control of an editable card that this read
  // leaves disabled (155: Parallel slots at its default keeps Unified KV
  // off), with no enabled control of its own to take the focus. 2h.10's 198a:
  // the field has no default box either, so the bar's alert says where it is.
  const second = await refused({
    key: 'not-one-of',
    args: ['kv-unified', 'ultra', 'auto, on, off'],
    field: 'kv-unified',
  })
  saveButton(second.main).focus()
  await save(second.main, second.bff)
  const text2 = en('api-not-one-of', 'kv-unified', 'ultra', 'auto, on, off')
  const at2 = en(
    'save-refused-at',
    text2,
    en('model-profile-unified-kv'),
    en('model-profile-resource-context'),
  )
  await within(second.main).findByText(at2)
  const unified = selectOf(second.main, en('model-profile-unified-kv'))
  expect(unified.disabled, 'the Unified KV select is disabled').toBe(true)
  expectError(unified, text2, 'Unified KV (disabled)')
  expectAlert(second.main, at2, 'the disabled Unified KV field (198a)')
  expect(
    within(second.main).queryByText(en('save-refused', text2)),
    '198a: not the alert that names no place',
  ).toBeNull()
  // No enabled control of that field (it has no default box), so 144 leaves
  // the focus where it was.
  expect(document.activeElement, 'focus stays on Save changes').toBe(
    saveButton(second.main),
  )
  expect(status(second.main).textContent).toBe(en('save-unsaved'))
})
