// The smallest end-to-end save of a model profile (fe/briefs/02h-model-profile-save.md,
// sub-slice 2h.1, criteria 130 to 134): the Embeddings card's two controls
// become editable and everything else stays disabled; the read needs
// `revision`, and the save sends it back with only the keys the user changed
// (D-a, D-b); the written-as line follows the field's state, and the
// full-value line is only for a disabled control. What is sent is read from
// the body of the PUT the fake bff recorded, asked for by a click on the save
// bar's Save changes. The bar's own DOM, Discard, the pending state, the
// 200's messages, the answers that are not a shaped 200, the leave guard and
// the tab order are 135 to 141, in ModelProfileSave2.test.tsx; the keyed answers
// (409, 422) are 2h.2. The cards, their fields and the reading of a value are
// 2e (ModelProfileCards1 to ModelProfileCards5.test.tsx, ModelProfileFlags.test.tsx).
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from '@testing-library/react'
import { afterEach, beforeEach, expect, test, vi } from 'vitest'
import App from './App.tsx'
import {
  before,
  deferred,
  en,
  fakeBff,
  isHidden,
  modelProfileDoc,
  reply,
  startAt,
  type BffOptions,
} from './test/bff.ts'
import {
  MODEL_PROFILE_CARDS,
  controlEnabled,
  isEditableField,
  specGroupEnabled,
  type Field,
  type Label,
} from './test/modelProfileCards.ts'

const ID = 'Qwen3-27B'
const HASH = `#/profiles/model/${ID}`
const PUT = `/api/profiles/model/${ID}`
const MODE = en('model-profile-embedding-mode')
const POOLING = en('model-profile-pooling')

/** An unlisted Pooling value past the 20 characters a select holds: the extra
 * option of 87, so the field's loaded state is one the select does not list
 * (133: picking it again makes the field unchanged, so it is never sent). */
const EXTRA_POOL = 'Last-Attention-Pool-X'

/** A text past the 20 characters a text field shows: the value 118's
 * full-value line repeats under a disabled input. */
const LONG_TEXT = '{"enable_thinking":false,"keep_past":true}'

/** The values of the read the 130 test uses: both Embeddings keys; the Vision
 * card's file keys (`mmproj` present and `mmproj-offload` checked, 176) and
 * `model-draft` (177, and 2h.9's 190: the same key opens the Speculative
 * decoding card's draft group), so those cards' rules leave their controls
 * enabled; and `n-gpu-layers`, so its default box is unticked and its input
 * enabled (156). No key of the Resource / context card besides these, so every
 * box of that card starts ticked (2h.156) and Unified KV sits disabled
 * (2h.155: Parallel slots at its default). No `spec-type`, so 191 leaves the
 * four n-gram groups of the Speculative decoding card locked. */
const VALUES = {
  embeddings: 'true',
  pooling: 'mean',
  mmproj: '/var/lib/private-ai/.llama.cpp/models/mmproj-BF16.gguf',
  'mmproj-offload': 'true',
  'model-draft':
    '/var/lib/private-ai/.llama.cpp/models/Qwen3-27B-draft-MTP.gguf',
  'n-gpu-layers': '99',
}

/** 2h.9's 189 to 191: the Speculative decoding card's fields, taken from the
 * shared card table of test/modelProfileCards.ts with their row key and their
 * `boxed` flag, so `controlEnabled` answers with the group rule of 190 / 191
 * on top of the default box of 156. A row's field is named by its key, not by
 * its label (the n-gram labels repeat, 2e's 113), and its ids are
 * `model-profile-<key>-box` and `model-profile-<key>-control`. */
const SPEC_BY_ID: Record<string, Field> = Object.fromEntries(
  MODEL_PROFILE_CARDS.find(
    (c) => c.heading === 'model-profile-speculative-decoding',
  )!
    .fields.filter(
      (f): f is { label: Label; key: string; boxed?: boolean } =>
        typeof f !== 'string' && 'key' in f,
    )
    .map((f) => [`model-profile-${f.key}`, f]),
)

/** The 2h.9 group a key belongs to (2e's 112: the key's prefix), `undefined`
 * for any other key: `draft` for the draft group, the literal name 191 lists
 * for an n-gram group. `ngram-map-k4v` is read before `ngram-map-k`, whose
 * prefix it starts. */
function specGroupOf(key: string): string | undefined {
  if (key.startsWith('spec-draft-')) return 'draft'
  return (
    ['ngram-mod', 'ngram-simple', 'ngram-map-k4v', 'ngram-map-k'].find((g) =>
      key.startsWith(`spec-${g}-`),
    ) ?? undefined
  )
}

/** 155, 156, 190, 191: is a field's default box enabled? Every box of an
 * editable card is, and a box of a 2h.9 group follows its group, the whole
 * group enabled or locked together (191). The box's own input, which also
 * needs the box unticked, is `controlEnabled`. */
function boxEnabled(field: Field, values: Record<string, string>): boolean {
  if (!isEditableField(field)) return false
  const group =
    typeof field === 'object' && 'key' in field
      ? specGroupOf(field.key)
      : undefined
  return group === undefined || specGroupEnabled(group, values)
}

/** The fields 2h.1's 130, 2h.4's 155, 2h.5's 164, 2h.6's 170, 2h.7's 175 and
 * 176, 2h.8's 183 and 186 and 2h.9's 189 made editable, keyed by the id of
 * their field (`model-profile-<key>`: the box is `<id>-box`, the input
 * `<id>-control`), so a control's id finds the field the shared table of
 * test/modelProfileCards.ts gives the enable rules to. Most entries are the label's
 * strings key; the Speculative decoding card's are the keyed fields of
 * `SPEC_BY_ID`. */
const EDITABLE_BY_ID: Record<string, Field> = {
  'model-profile-ctx-size': 'model-profile-context-size',
  'model-profile-parallel': 'model-profile-parallel-slots',
  'model-profile-kv-unified': 'model-profile-unified-kv',
  'model-profile-kv-unified-per-slot': 'model-profile-per-slot-context',
  'model-profile-batch-size': 'model-profile-batch-size',
  'model-profile-ubatch-size': 'model-profile-u-batch-size',
  'model-profile-embeddings': 'model-profile-embedding-mode',
  'model-profile-pooling': 'model-profile-pooling',
  'model-profile-cache-type-k': 'model-profile-k-cache-type',
  'model-profile-cache-type-v': 'model-profile-v-cache-type',
  'model-profile-flash-attn': 'model-profile-flash-attention',
  'model-profile-cache-ram': 'model-profile-cache-ram',
  'model-profile-cache-reuse': 'model-profile-cache-reuse',
  'model-profile-ctx-checkpoints': 'model-profile-context-checkpoints',
  'model-profile-checkpoint-min-step': 'model-profile-checkpoint-step',
  'model-profile-temp': 'model-profile-temperature',
  'model-profile-top-k': 'Top-K',
  'model-profile-top-p': 'Top-P',
  'model-profile-min-p': 'Min-P',
  'model-profile-repeat-penalty': 'model-profile-repeat-penalty',
  'model-profile-presence-penalty': 'model-profile-presence-penalty',
  'model-profile-backend-sampling': 'model-profile-backend-sampling',
  'model-profile-load-on-startup': 'model-profile-load-on-startup',
  'model-profile-chat-template-kwargs': 'model-profile-chat-kwargs',
  // 2h.6's 170: the Chat / reasoning card, every field always enabled. The
  // two boxes of the card's boxed numbers are `<id>-box`, their inputs
  // `<id>-control`, and the two checkboxes' inputs are `<id>-control` too.
  'model-profile-jinja': 'model-profile-chat-template',
  'model-profile-prefill-assistant': 'model-profile-prefill-assistant',
  'model-profile-reasoning': 'model-profile-reasoning',
  'model-profile-reasoning-format': 'model-profile-reasoning-format',
  'model-profile-reasoning-effort': 'model-profile-reasoning-effort',
  // The row's key is reasoning-preserve, its label keeps-past-thinking.
  'model-profile-reasoning-preserve': 'model-profile-keep-past-thinking',
  'model-profile-reasoning-budget': 'model-profile-thinking-budget',
  'model-profile-reasoning-budget-message': 'model-profile-budget-message',
  'model-profile-n-predict': 'model-profile-max-output',
  // 2h.7's 176: the Vision card's four, enabled while the read's `mmproj` is
  // present (Vision GPU also needs Vision on GPU checked). The checkbox's
  // input is `<id>-control`.
  'model-profile-mmproj-offload': 'model-profile-vision-on-gpu',
  'model-profile-mmproj-device': 'model-profile-vision-gpu',
  'model-profile-image-min-tokens': 'model-profile-min-image-tokens',
  'model-profile-image-max-tokens': 'model-profile-max-image-tokens',
  // 2h.7's 175: the Hardware card, its seven always-on rows and the draft
  // pair (177's rule). 2h.8's 183 and 186: the Fit trio, `fit` always enabled
  // and its two dependents while its box reads checked. Only the two MoE rows
  // (178) stay out of the list, so the test demands them disabled;
  // `n-cpu-ffn`, `n-gpu-layers` and `fit-ctx` are boxed (156).
  'model-profile-n-gpu-layers': 'model-profile-gpu-layers',
  'model-profile-n-cpu-ffn': 'model-profile-dense-ffn-cpu-layers',
  'model-profile-op-offload': 'model-profile-op-offload',
  'model-profile-repack': 'model-profile-repack',
  'model-profile-no-host': 'model-profile-no-host-buffer',
  'model-profile-lazy-mode': 'model-profile-lazy-tensors',
  'model-profile-load-mode': 'model-profile-load-mode',
  'model-profile-fit': 'model-profile-fit-to-vram',
  'model-profile-fit-ctx': 'model-profile-fit-min-context',
  'model-profile-fit-target': 'model-profile-fit-free-vram',
  'model-profile-device-draft': 'model-profile-draft-device',
  'model-profile-n-gpu-layers-draft': 'model-profile-draft-gpu-layers',
  // 2h.9's 189 to 191: the Speculative decoding card's sixteen fields, by
  // their row keys, each enabled by its group's rule.
  ...SPEC_BY_ID,
}

beforeEach(() => {
  startAt(HASH)
})

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
  startAt('')
})

// ── Helpers (as ModelProfileFlags.test.tsx) ───────────────────────────────────

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

/** An element's text, with the page's runs of whitespace folded. */
function text(el: Element): string {
  return (el.textContent ?? '').replace(/\s+/g, ' ').trim()
}

function selectOf(main: HTMLElement, label: string): HTMLSelectElement {
  return within(main).getByRole('combobox', {
    name: label,
  }) as HTMLSelectElement
}

function pick(main: HTMLElement, label: string, value: string) {
  fireEvent.change(selectOf(main, label), { target: { value } })
}

/** The written-as line of `key` (2e's 124), and the full-value line (2e's 118). */
function writtenOf(main: HTMLElement, key: string): HTMLElement | null {
  return main.querySelector(`#model-profile-written-${key}`)
}

function fullOf(main: HTMLElement, key: string): HTMLElement | null {
  return main.querySelector(`#model-profile-full-${key}`)
}

/** The ids the control's `aria-describedby` names, in order (124b). */
function describedIds(control: Element): string[] {
  return (control.getAttribute('aria-describedby') ?? '')
    .split(/\s+/)
    .filter(Boolean)
}

function saveButton(main: HTMLElement): HTMLElement {
  return within(main).getByRole('button', { name: en('save-button') })
}

function discardButton(main: HTMLElement): HTMLElement {
  return within(main).getByRole('button', { name: en('save-discard') })
}

/** True when the button cannot be used: disabled, or aria-disabled. */
function inactive(el: HTMLElement): boolean {
  return (
    (el as HTMLButtonElement).disabled ||
    el.getAttribute('aria-disabled') === 'true'
  )
}

/** The save bar's status line: the only role="status" below the h2 of the
 * settings (the page's loading status is above it). */
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

/** Clicks Save changes and waits for the n-th PUT of this model profile. */
async function save(main: HTMLElement, bff: ReturnType<typeof fakeBff>, n = 1) {
  fireEvent.click(saveButton(main))
  await waitFor(() => expect(bff.to('PUT', PUT)).toHaveLength(n))
}

/** The `values` object of the n-th recorded PUT body, and its whole body. */
function sentValues(
  bff: ReturnType<typeof fakeBff>,
  n = 1,
): Record<string, unknown> {
  const body = bff.to('PUT', PUT)[n - 1]!.body as {
    values: Record<string, unknown>
  }
  return body.values
}

function sentBody(
  bff: ReturnType<typeof fakeBff>,
  n = 1,
): Record<string, unknown> {
  return bff.to('PUT', PUT)[n - 1]!.body as Record<string, unknown>
}

// ── 132a: the extra option is built from the LOADED value ───────────────

/** 132a's Pooling value: a value the select does not list, short enough that
 * its option shows all of it (no full-value line to read in its place). */
const WEIRD = 'weird-pool'

/** The Pooling options the row lists (2e's Embeddings table), with no extra
 * option added: the list a select whose loaded value is in it keeps. */
const POOL_LISTED = ['default', 'none', 'mean', 'cls', 'last', 'rank']

/** The option values of `label`'s select, in the order the control lists them. */
function optionValues(main: HTMLElement, label: string): string[] {
  return [...selectOf(main, label).options].map((o) => o.value)
}

/** The bar's alert reads `text`: its own role="alert" (01's 30a). */
function expectAlert(main: HTMLElement, text: string) {
  const el = within(main).getByText(text)
  expect(el.closest('[role="alert"]'), `${text}: in a role="alert"`).not.toBe(
    null,
  )
}

/** The loaded page with revision `r1`, `values`, and `answer` for the save. */
function form(
  values: Record<string, string>,
  answer?: BffOptions['modelProfileSave'],
) {
  return loaded({
    modelProfile: () =>
      reply(200, modelProfileDoc({}, { revision: 'r1', values })),
    modelProfileSave: answer,
  })
}

// ── 130: what becomes editable ──────────────────────────────────────────

test('2h.130 (2h.4 155, 156, 2h.5 164, 2h.6 170, 2h.7 175, 176, 177, 2h.8 183, 186, 2h.9 189, 190, 191): the Embeddings selects, the Resource / context card, the KV cache card, the Sampling overrides card, the Advanced card, the Chat / reasoning card, the Vision card, the Hardware card, its Fit trio included and its two MoE rows excepted, and the Speculative decoding card, its draft group open on this read and its four n-gram groups locked, are the only controls of a 2e card that are not disabled, and no other control gains aria-disabled (H11b.1: the Engine select is off the page)', async () => {
  const { main } = await page(VALUES)
  const controls = [
    ...main.querySelectorAll<HTMLElement>('input, select, textarea'),
  ]
  expect(controls.length, 'every field of 2e is on the page').toBeGreaterThan(
    20,
  )
  for (const el of controls) {
    const id = el.getAttribute('id') ?? el.tagName
    // The 2h.4 card's fields by their ids: `model-profile-<key>`, its default box
    // `<id>-box`, its input `<id>-control`.
    const field = EDITABLE_BY_ID[id.replace(/-(box|control)$/, '')]
    // 155: a box is enabled in an editable card, a box of a 2h.9 group with
    // its group; 156: the input follows the box, which a key the file lacks
    // leaves ticked. 130: the Embeddings
    // selects are always enabled. 176: every Vision control while the read's
    // `mmproj` is there, Vision GPU also while Vision on GPU is checked. 177:
    // the draft pair while `model-draft` is there. 190, 191: the Speculative
    // decoding card's fields move with their group, the draft group opened by
    // the same `model-draft`, each n-gram group by its entry of `spec-type`,
    // which this read has none of.
    const enabled =
      field !== undefined &&
      (id.endsWith('-box')
        ? boxEnabled(field, VALUES)
        : controlEnabled(field, VALUES))
    expect((el as HTMLInputElement).disabled, `${id}: enabled`).toBe(!enabled)
    expect(
      el.getAttribute('aria-disabled'),
      `${id}: not aria-disabled, a disabled control is natively disabled`,
    ).toBeNull()
  }
})

// The English of the two read-only sentences the page no longer shows: 2h.1's
// 130 removed `model-profile-read-only` and 2h.10's 194 removed
// `model-profile-partly-editable`. Both keys are out of the strings file (pinned by
// App.test.tsx's 1a), so the test names the text itself.
const READ_ONLY_ENGLISH =
  "Read-only for now: this page cannot change a model profile's settings yet. Change them in pai-config on the server machine."
const PARTLY_EDITABLE_ENGLISH =
  'Some settings cannot be changed here yet: their controls are greyed out. Change those in pai-config on the server machine.'

test('2h.130 (2h.10 194): no read-only Notice on the page: model-profile-read-only went in 2h.1 and model-profile-partly-editable follows it in 2h.10, so a page with an available engine shows no bordered Notice at all', async () => {
  const { main } = await page({})
  expect(
    main.querySelectorAll('.notice').length,
    '194: no Notice, the engine is available',
  ).toBe(0)
  expect(
    main.textContent,
    '2h.1: the read-only string is gone from the page',
  ).not.toContain(READ_ONLY_ENGLISH)
  expect(
    main.textContent,
    '194: the partly-editable string is gone from the page',
  ).not.toContain(PARTLY_EDITABLE_ENGLISH)
})

// ── 131: the revision ───────────────────────────────────────────────────

test('2h.131: a 200 read with no revision, a revision that is not a string, a number, null, shows model-profile-load-failed and no card', async () => {
  for (const [what, extra] of [
    ['absent', { revision: undefined }],
    ['null', { revision: null }],
    ['a number', { revision: 42 }],
    ['an object', { revision: { n: 1 } }],
  ] as const) {
    const { main } = open({
      modelProfile: () =>
        reply(200, modelProfileDoc({}, { values: {}, ...extra })),
    })
    const shown = await within(main).findByText(en('model-profile-load-failed'))
    expect(shown, `revision ${what}: the message`).toBeTruthy()
    expect(
      within(main).queryAllByRole('heading', { level: 3 }).length,
      `revision ${what}: no card`,
    ).toBe(0)
    cleanup()
    vi.unstubAllGlobals()
  }
})

test('2h.131: the save sends the read revision back untouched: the fe never parses it', async () => {
  const opaque = 'R 9/8-∅ x'
  const { bff, main } = await loaded({
    modelProfile: () =>
      reply(
        200,
        modelProfileDoc({}, { values: { pooling: 'mean' }, revision: opaque }),
      ),
  })
  pick(main, POOLING, 'cls')
  await save(main, bff)
  expect(sentBody(bff).revision, 'the same string, as read').toBe(opaque)
})

// ── 132: changed and dirty ──────────────────────────────────────────────

test('2h.132: a fresh page is clean: the status is empty and both bar buttons are inert; a changed Embedding mode makes it dirty, and back to the loaded state makes it clean again', async () => {
  const { main } = await page({ embeddings: 'true', pooling: 'mean' })
  const line = status(main)
  expect(line.textContent, 'clean on load').toBe('')
  expect(inactive(saveButton(main)), 'Save inert while clean').toBe(true)
  expect(inactive(discardButton(main)), 'Discard inert while clean').toBe(true)

  pick(main, MODE, 'false')
  expect(line.textContent, 'dirty').toBe(en('save-unsaved'))
  expect(inactive(saveButton(main)), 'Save live when dirty').toBe(false)
  expect(inactive(discardButton(main)), 'Discard live when dirty').toBe(false)

  // D-b: back to the loaded state, not to what was typed.
  pick(main, MODE, 'true')
  expect(line.textContent, 'clean again').toBe('')
  expect(inactive(saveButton(main)), 'Save inert again').toBe(true)
})

test('2h.132: a Pooling change to the same option it loaded in is not dirty, and an option change is; a hint toggle changes nothing', async () => {
  const { bff, main } = await page({ pooling: 'mean' })
  const line = status(main)
  pick(main, POOLING, 'mean')
  expect(line.textContent, 'the same option is not a change').toBe('')
  pick(main, POOLING, 'none')
  expect(line.textContent, 'another option is a change').toBe(
    en('save-unsaved'),
  )
  pick(main, POOLING, 'mean')
  expect(line.textContent, 'back to the loaded option').toBe('')

  // 132: hint toggles change nothing (the About of an editable and of a
  // disabled field).
  for (const label of [MODE, en('model-profile-context-size')]) {
    fireEvent.click(
      within(main).getByRole('button', {
        name: `${en('field-about')} ${label}`,
      }),
    )
    expect(line.textContent, `About ${label}: the form stays clean`).toBe('')
  }
  expect(bff.to('PUT', PUT).length, 'a hint toggle sends nothing').toBe(0)
})

// ── 132a: the extra option of the loaded value stays listed ─────────────

test('2h.132a: the extra option of an unlisted loaded Pooling value stays last in the list whatever is picked, and picking it again leaves the form clean: a save sends nothing', async () => {
  const { bff, main } = await page({ pooling: WEIRD })
  expect(
    optionValues(main, POOLING).at(-1),
    'the extra option is last on load',
  ).toBe(WEIRD)

  // Pick a listed option: the loaded state stays reachable from the control.
  pick(main, POOLING, 'mean')
  expect(
    optionValues(main, POOLING),
    'the row options, plus the extra option still last',
  ).toEqual([...POOL_LISTED, WEIRD])
  expect(selectOf(main, POOLING).value, 'mean is what is picked now').toBe(
    'mean',
  )

  // Pick the extra option again: the field is back to the state the read gave,
  // so the form is clean and a click on Save sends nothing (132, 133).
  pick(main, POOLING, WEIRD)
  expect(
    selectOf(main, POOLING).value,
    'the extra option is picked again',
  ).toBe(WEIRD)
  expect(status(main).textContent, 'back to the loaded value: clean').toBe('')
  fireEvent.click(saveButton(main))
  await act(async () => {})
  expect(bff.to('PUT', PUT), 'a clean form sends nothing').toHaveLength(0)
})

test('2h.132a: a 200 that makes the loaded value one the select lists drops the extra option, and a select whose loaded value is listed never gains one', async () => {
  const { bff, main } = await page({ pooling: WEIRD })
  pick(main, POOLING, 'mean')
  await save(main, bff)
  await waitFor(() =>
    expect(status(main).textContent, 'the 200 of the default fake').toBe(
      en('save-saved'),
    ),
  )
  expect(
    optionValues(main, POOLING),
    'the loaded value is listed now: the extra option is gone',
  ).toEqual(POOL_LISTED)

  // A loaded value the row lists: no extra option on load, and none appears
  // when the user picks another listed option.
  cleanup()
  vi.unstubAllGlobals()
  const second = await page({ pooling: 'mean' })
  expect(
    optionValues(second.main, POOLING),
    'loaded mean: the row list alone',
  ).toEqual(POOL_LISTED)
  pick(second.main, POOLING, 'cls')
  expect(
    optionValues(second.main, POOLING),
    'picking cls gains no extra option',
  ).toEqual(POOL_LISTED)
})

// ── 133: what a changed field sends ─────────────────────────────────────

test('2h.133: Embedding mode off to on sends "true" as a string, alone in values, and the body is only revision and values', async () => {
  const { bff, main } = await page({ embeddings: 'false', pooling: 'mean' })
  pick(main, MODE, 'true')
  await save(main, bff)
  const body = sentBody(bff)
  expect(
    Object.keys(body).sort(),
    'the body of the contract: revision and values',
  ).toEqual(['revision', 'values'])
  expect(sentValues(bff).embeddings, 'on sends the string "true"').toBe('true')
  expect(
    Object.keys(sentValues(bff)),
    'the unchanged Pooling key is absent',
  ).toEqual(['embeddings'])
})

test('2h.133: Embedding mode on to off sends null, the GUI spelling of a switch that is off', async () => {
  const { bff, main } = await page({ embeddings: 'true' })
  pick(main, MODE, 'false')
  await save(main, bff)
  expect(sentValues(bff).embeddings, 'off removes the key').toBeNull()
  expect(Object.keys(sentValues(bff))).toEqual(['embeddings'])
})

test('2h.133: Pooling default to an option sends the option value; an option back to default sends null', async () => {
  const first = await page({})
  pick(first.main, POOLING, 'rank')
  await save(first.main, first.bff)
  expect(sentValues(first.bff).pooling, 'the option value').toBe('rank')
  expect(Object.keys(sentValues(first.bff))).toEqual(['pooling'])

  cleanup()
  vi.unstubAllGlobals()
  const second = await page({ pooling: 'last' })
  pick(second.main, POOLING, 'default')
  await save(second.main, second.bff)
  expect(sentValues(second.bff).pooling, 'default removes the key').toBeNull()
})

test('2h.133: an unlisted Pooling value is the loaded state: picking the extra option again sends nothing, picking a listed option sends its value', async () => {
  const { bff, main } = await page({
    embeddings: 'false',
    pooling: EXTRA_POOL,
  })
  const select = selectOf(main, POOLING)
  expect(select.value, 'the extra option is selected').toBe(EXTRA_POOL)

  // Pick a listed option: sent.
  pick(main, POOLING, 'last')
  await save(main, bff)
  expect(sentValues(bff).pooling, 'a listed option sends its value').toBe(
    'last',
  )

  // Back to the extra option: the field is unchanged, so a save of another
  // field must not carry pooling.
  pick(main, POOLING, EXTRA_POOL)
  pick(main, MODE, 'true')
  await save(main, bff, 2)
  expect(
    Object.keys(sentValues(bff, 2)),
    'the extra option is never sent',
  ).toEqual(['embeddings'])
})

test('2h.133: a field changed back to its loaded state is absent from the body, and an untouched page sends nothing', async () => {
  const { bff, main } = await page({ embeddings: 'true', pooling: 'mean' })
  pick(main, MODE, 'false')
  pick(main, MODE, 'true')
  pick(main, POOLING, 'cls')
  pick(main, POOLING, 'mean')
  expect(status(main).textContent, 'nothing changed: clean').toBe('')
  // The Save button is inert while clean (135), so no PUT can go out.
  expect(bff.to('PUT', PUT).length, 'a clean form sends nothing').toBe(0)
})

// ── 134: the lines under a control while editing ────────────────────────

test('2h.134: the written-as line of Embedding mode shows while the field is unchanged, hides when it is changed, and the control drops its id', async () => {
  const { main } = await page({ embeddings: '0', pooling: 'mean' })
  const select = selectOf(main, MODE)
  const line = writtenOf(main, 'embeddings')
  expect(line, 'the line of 124 for the file spelling "0"').toBeTruthy()
  expect(text(line!), 'the file spelling').toBe(
    en('model-profile-written-as', '0'),
  )
  expect(describedIds(select), 'the control names the line (124b)').toContain(
    'model-profile-written-embeddings',
  )
  expect(isHidden(line!), 'shown').toBe(false)

  pick(main, MODE, 'true')
  expect(
    isHidden(writtenOf(main, 'embeddings')),
    'a changed field hides the line',
  ).toBe(true)
  expect(
    describedIds(selectOf(main, MODE)),
    'and the control drops its id',
  ).not.toContain('model-profile-written-embeddings')

  pick(main, MODE, 'false')
  expect(
    isHidden(writtenOf(main, 'embeddings')),
    'the file spelling shows again once the field is back off',
  ).toBe(false)
})

test('2h.134 (2h.4 160, 2h.5 164, 2h.6 170, 2h.7 175, 2h.8 186, 2h.9 190): an editable control shows no full-value line, Embeddings, Resource / context, KV cache, Chat / reasoning and Hardware alike, and a control a rule disables keeps its own', async () => {
  const { main } = await page({
    pooling: EXTRA_POOL,
    'ctx-size': '172032000',
    // The Chat / reasoning card, editable since 2h.6's 170: 134 takes its
    // lines, and the number of a box the file unticked is an enabled input.
    'reasoning-effort': EXTRA_POOL,
    'reasoning-budget-message': LONG_TEXT,
    'n-predict': '172032000',
    // The KV cache card, editable since 2h.5's 164: 134 takes its lines.
    'cache-type-k': EXTRA_POOL,
    'cache-ram': '172032000',
    // The Hardware card, editable since 2h.7's 175: its selects are always
    // enabled and the key unticks the number's box, so 134 takes its lines.
    'load-mode': EXTRA_POOL,
    'lazy-mode': EXTRA_POOL,
    'n-gpu-layers': '172032000',
    // 2h.8's 183 and 186 made the Fit trio editable, so its controls are
    // disabled by the rule and not by the card: with the Fit to VRAM box off,
    // 134 keeps the line under the number's input and the text input, as it
    // keeps it under the Speculative decoding card's Draft K-cache type
    // select: 189 made that card editable, but 190 disables its draft group
    // on this read, which has no `model-draft` and no `spec-type`.
    fit: 'off',
    'fit-ctx': '172032000',
    'fit-target': LONG_TEXT,
    'spec-draft-type-k': EXTRA_POOL,
    'spec-draft-n-max': '172032000',
  })
  // An enabled select opens and an enabled input scrolls, so 134 takes the
  // line away from both, and 160 says the Resource card's number never has
  // one: a ticked box's input is empty, an unticked one is enabled.
  for (const key of [
    'pooling',
    'ctx-size',
    'cache-type-k',
    'cache-ram',
    'reasoning-effort',
    'reasoning-budget-message',
    'n-predict',
    'load-mode',
    'lazy-mode',
    'n-gpu-layers',
  ])
    expect(
      fullOf(main, key),
      `${key}: no full-value line under an editable control`,
    ).toBeNull()
  // A control that is disabled now, by 175, 186 or 190, keeps the line of
  // 118: a select, a number's input and a text input.
  for (const key of [
    'spec-draft-type-k',
    'fit-ctx',
    'fit-target',
    'spec-draft-n-max',
  ])
    expect(
      fullOf(main, key),
      `${key}: the disabled control keeps its line`,
    ).toBeTruthy()
  cleanup()
  // The same three fields with the Fit box checked: 186 enables the number's
  // input and the text input, so 134 takes their two lines away and 190 leaves
  // the draft group's select disabled on its own.
  const on = await page({
    fit: 'on',
    'fit-ctx': '172032000',
    'fit-target': LONG_TEXT,
    'spec-draft-type-k': EXTRA_POOL,
  })
  for (const key of ['fit-ctx', 'fit-target'])
    expect(
      fullOf(on.main, key),
      `${key}: 186 enables the control, so no line`,
    ).toBeNull()
  expect(
    fullOf(on.main, 'spec-draft-type-k'),
    'the select 190 disables keeps its line',
  ).toBeTruthy()
})

test("2h.134 (2h.6 170, 2h.9 190): a long value the disabled control cannot show keeps its line: the Speculative decoding card's Draft K-cache type select is the disabled half of the pair (190: this read has no draft), Reasoning effort the enabled half (pinned above)", async () => {
  const { main } = await page({
    'spec-draft-type-k': EXTRA_POOL,
    'reasoning-effort': EXTRA_POOL,
  })
  const line = fullOf(main, 'spec-draft-type-k')!
  expect(text(line), 'the line repeats the value as written').toBe(EXTRA_POOL)
  expect(
    selectOf(main, en('model-profile-draft-k-cache-type')).disabled,
    'the field is a disabled select',
  ).toBe(true)
  expect(
    selectOf(main, en('model-profile-reasoning-effort')).disabled,
    'Reasoning effort: 170 makes its card editable, so the select is enabled',
  ).toBe(false)
  expect(
    fullOf(main, 'reasoning-effort'),
    'and the enabled half shows no line',
  ).toBeNull()
})

// ── 139 with 138: an edit made while the PUT runs, and the save fails ───

test('2h.139, 2h.138: an edit made while the PUT runs survives a failed save: both edits stay in the controls, the form is dirty with the save-failed alert, and the next Save sends both keys with the revision that was read', async () => {
  const gate = deferred()
  const { bff, main } = await form(
    { embeddings: 'true', pooling: 'mean' },
    (_b, _id, n) =>
      n === 1 ? gate.promise : reply(200, { revision: 'r2', running: false }),
  )
  // First edit, then the save; the PUT is held pending.
  pick(main, MODE, 'false')
  fireEvent.click(saveButton(main))
  await waitFor(() => expect(bff.to('PUT', PUT)).toHaveLength(1))
  // Second edit, made while the PUT runs (137: the fields stay editable).
  pick(main, POOLING, 'rank')
  await act(async () => {
    gate.resolve(reply(500, { key: 'secret-detail', args: [] }))
  })
  await within(main).findByText(en('save-failed'))
  expectAlert(main, en('save-failed'))
  expect(
    document.body.textContent,
    'the manager words never reach the page',
  ).not.toContain('secret-detail')
  expect(
    selectOf(main, MODE).value,
    'the edit made before the save stays',
  ).toBe('false')
  expect(
    selectOf(main, POOLING).value,
    'the edit made while it runs stays too',
  ).toBe('rank')
  expect(status(main).textContent, 'the form is dirty').toBe(en('save-unsaved'))
  expect(inactive(saveButton(main)), 'Save is live again').toBe(false)
  // The next save carries both edits and the revision of the read: a failed
  // save leaves the loaded state alone (139).
  await save(main, bff, 2)
  expect(sentBody(bff, 2), 'both keys, the read revision').toEqual({
    revision: 'r1',
    values: { embeddings: null, pooling: 'rank' },
  })
})
