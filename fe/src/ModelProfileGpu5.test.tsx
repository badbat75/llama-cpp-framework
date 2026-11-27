// Brief 19, sub-slice LS1, criteria 19 G.1 and 19 G.2
// (fe/briefs/19-layer-sliders.md): the read of the model file's layer facts,
// one GET /api/profiles/model/<id>/model-info per arrival beside the profile
// and device reads (its encoding, its 15 s limit, the replies that are a good
// shape, the ones that are not, and that it never changes the session), and
// the facts line of the GPU card: where it sits, what it says for a read
// model, for the four reasons, for a MoE file, and while the read is pending.
// 19 G.3 is in gpuBlocks.test.ts, 19 G.4 to G.12 in ModelProfileGpu6 and
// ModelProfileGpu7.test.tsx. The helpers are those of ModelProfileGpu3.test.tsx,
// repeated here (a test file imports no other test file).
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
  DEVICES,
  en,
  fakeBff,
  held,
  isHidden,
  MODEL_FILE,
  modelProfileDoc,
  reply,
  startAt,
  type BffOptions,
  type ModelInfo,
} from './test/bff.ts'

const ID = 'Qwen3-27B'
const HASH = `#/profiles/model/${ID}`
const PUT = `/api/profiles/model/${ID}`
const INFO = `${PUT}/model-info`
const DEVICES_ROUTE = '/api/node/devices'
const GPU = en('model-profile-gpu-distribution')
const GPUS = en('model-profile-gpus')
const MODE_LABEL = en('model-profile-split-mode')
const READING = en('model-profile-model-facts-reading')
const FAILED_FACTS = en('model-profile-model-facts-failed')
const INTRO_LINES = en('model-profile-gpu-distribution-intro').split('\n')
/** 19 G.1: the read's time limit (fe/DESIGN.md 7). */
const LIMIT = 15_000
/** Ornith-1.0-9B, the brief's file: 33 blocks, so 34 positions. */
const BLOCKS = 33

/** The 200 body of a read that answered the file's facts. */
function readInfo(info: Partial<Record<string, unknown>>): ModelInfo {
  return { model: MODEL_FILE, info: info as ModelInfo['info'], reason: null }
}

/** The 200 body of a read that answered a reason. */
function reasonInfo(key: string, args: (string | number)[]): ModelInfo {
  return { model: MODEL_FILE, info: null, reason: { key, args } }
}

beforeEach(() => {
  startAt(HASH)
})

afterEach(() => {
  cleanup()
  startAt('')
  vi.useRealTimers()
})

// ── Helpers (as ModelProfileGpu3.test.tsx) ──────────────────────────────────

/** Fake timers, the microtask queue left real (ModelProfilePage.test.tsx). */
function fakeClock() {
  vi.useFakeTimers({
    toFake: [
      'setTimeout',
      'clearTimeout',
      'setInterval',
      'clearInterval',
      'Date',
    ],
  })
}

/** Lets every answer and render due now land. */
async function flush() {
  await act(async () => {
    for (let i = 0; i < 4; i++) await vi.advanceTimersByTimeAsync(0)
  })
}

async function advance(ms: number) {
  await act(async () => {
    await vi.advanceTimersByTimeAsync(ms)
  })
  await flush()
}

/** Waits for the page read with the values `values`, the GPU probe `DEVICES`
 * answered and `options` for the bff. A save answers 200 and applies the sent
 * values to the next read, as the bff does.
 *
 * `clock` is for 19 G.1's timed tests: the fake clock goes on BEFORE the
 * render, because the read's limit is armed by the mount, and the mount is
 * then flushed and read with synchronous queries. */
async function form(
  values: Record<string, string> = { device: 'CUDA0,ROCm0' },
  options: BffOptions = {},
  clock = false,
) {
  if (clock) fakeClock()
  const loaded = { ...values }
  const bff = fakeBff({
    devices: DEVICES,
    modelProfile: () =>
      reply(
        200,
        modelProfileDoc({}, { revision: 'r1', values: { ...loaded } }),
      ),
    modelProfileSave: (body) => {
      const sent = (body as { values?: Record<string, unknown> })?.values ?? {}
      for (const [key, value] of Object.entries(sent)) {
        if (value === null || value === undefined) delete loaded[key]
        else loaded[key] = String(value)
      }
      return reply(200, { revision: 'r2', running: false })
    },
    ...options,
  })
  render(<App />)
  const main = screen.getByRole('main')
  if (clock) {
    await flush()
    expect(
      within(main).queryByRole('heading', { level: 1, name: ID }),
      'the page is mounted on the fake clock',
    ).not.toBeNull()
    return { bff, main }
  }
  await within(main).findByRole('heading', { level: 1, name: ID })
  await within(main).findByRole('heading', { level: 3, name: GPU })
  await ready(main)
  return { bff, main }
}

/** Waits for the device read to land with the brief's probe time. */
async function ready(main: HTMLElement): Promise<HTMLElement> {
  const fs = await within(main).findByRole('group', { name: GPUS })
  await within(fs).findByText(asOfDate(DEVICES.probedAt!))
  return fs
}

/** The moment the as-of line shows for `probedAt`. */
function asOfDate(probedAt: string): string {
  return new Date(probedAt).toLocaleString(undefined, {
    dateStyle: 'medium',
    timeStyle: 'short',
  })
}

/** The card's `section`, from its h3 (Card.tsx). */
function cardOf(main: HTMLElement, title: string): HTMLElement {
  const h3 = within(main).getByRole('heading', { level: 3, name: title })
  const card = h3.closest('section')
  expect(card, `${title}: the card element`).not.toBeNull()
  return card as HTMLElement
}

/** The split mode select (GD1.2). */
function modeSelect(main: HTMLElement): HTMLSelectElement {
  return within(cardOf(main, GPU)).getByRole('combobox', {
    name: MODE_LABEL,
  }) as HTMLSelectElement
}

/** The facts line of 19 G.2: the card's one paragraph after its intro and
 * before the split mode field, whatever it says. 22 D4.31 review D1 (2026-11-27)
 * adds the card's About panel, a hidden paragraph of the card, so the line is
 * looked for among the paragraphs the eye sees. */
function factsEl(main: HTMLElement): HTMLElement {
  const card = cardOf(main, GPU)
  const paras = [...card.querySelectorAll('p')].filter((p) => !isHidden(p))
  const intro = paras.filter((p) => INTRO_LINES.includes(p.textContent ?? ''))
  const after = intro.length > 0 ? intro[intro.length - 1]! : card
  const found = paras.filter(
    (p) =>
      p !== after &&
      before(after, p) &&
      before(p, modeSelect(main)) &&
      (p.textContent ?? '').trim() !== '',
  )
  expect(
    found.map((p) => p.textContent),
    '19 G.2: one paragraph between the card intro and the split mode field',
  ).toHaveLength(1)
  return found[0]!
}

/** The facts line's text, spaces collapsed. */
function factsText(main: HTMLElement): string {
  return (factsEl(main).textContent ?? '').replace(/\s+/g, ' ').trim()
}

/** The line the facts line shows for a read that answered `blockCount` and no
 * experts (19 G.2). */
function factsOf(blockCount: number): string {
  return en('model-profile-model-facts', `${blockCount}`)
}

/** The line for a reason: `model-profile-model-facts-none` filled with
 * `api-<key>` filled with its args. */
function factsNone(key: string, ...args: string[]): string {
  return en('model-profile-model-facts-none', en(`api-${key}`, ...args))
}

/** The name a screen reader reads (ModelProfileGpu3.test.tsx's computation,
 * with `aria-label` added). */
function nameOf(el: HTMLElement): string {
  const labelled = el.getAttribute('aria-labelledby')
  if (labelled)
    return labelled
      .split(/\s+/)
      .map((part) => document.getElementById(part)?.textContent ?? '')
      .join(' ')
      .replace(/\s+/g, ' ')
      .trim()
  const label = el.getAttribute('aria-label')
  if (label) return label.replace(/\s+/g, ' ').trim()
  const id = el.getAttribute('id')
  const tied = id ? document.querySelector(`label[for="${id}"]`) : null
  const wrap = el.closest('label')
  return (tied?.textContent ?? wrap?.textContent ?? el.textContent ?? '')
    .replace(/\s+/g, ' ')
    .trim()
}

/** The weight fields of the GPUs fieldset, in page order (GD2.1): their
 * presence is brief 14 as built, so blocks mode is not (19 G.4). */
function weightFields(main: HTMLElement): HTMLInputElement[] {
  const fs = within(main).getByRole('group', { name: GPUS })
  const weight = en('model-profile-gpu-weight')
  return [...fs.querySelectorAll<HTMLInputElement>('input')].filter((el) =>
    nameOf(el).startsWith(`${weight} `),
  )
}

/** The `values` of the PUT. */
function sentValues(
  bff: ReturnType<typeof fakeBff>,
  n = 1,
): Record<string, unknown> {
  const body = bff.to('PUT', PUT)[n - 1]!.body as Record<string, unknown>
  return body.values as Record<string, unknown>
}

// ── 19 G.1: the read ────────────────────────────────────────────────────────

test('19 G.1: arriving sends one GET model-info beside the profile and device reads, named with the id as the profile read names it', async () => {
  const { bff, main } = await form()
  expect(bff.to('GET', INFO).length, '19 G.1: one facts read per arrival').toBe(
    1,
  )
  expect(bff.to('GET', PUT).length, '19 G.1: the profile was read').toBe(1)
  expect(
    bff.to('GET', DEVICES_ROUTE).length,
    '19 G.1: the GPUs were read',
  ).toBe(1)
  expect(factsText(main), '19 G.1: the line states what was read').toBe(
    factsNone('model-info-missing', MODEL_FILE),
  )
})

test.each([
  [ID, ID],
  ['a b', 'a%20b'],
  ['a%b', 'a%25b'],
])(
  '19 G.1: the facts read carries the id percent-encoded exactly as the profile read encodes it (%s)',
  async (id, encoded) => {
    startAt(`#/profiles/model/${encoded}`)
    const bff = fakeBff({
      devices: DEVICES,
      modelProfile: () =>
        reply(200, modelProfileDoc({}, { revision: 'r1', values: {} })),
    })
    render(<App />)
    const main = screen.getByRole('main')
    await within(main).findByRole('heading', { level: 1, name: id })
    await waitFor(() =>
      expect(
        bff.to('GET', `/api/profiles/model/${encoded}`).length,
        `${id}: the profile read went out`,
      ).toBe(1),
    )
    expect(
      bff.to('GET', `/api/profiles/model/${encoded}/model-info`).length,
      `${id}: the facts read of the same id`,
    ).toBe(1)
    cleanup()
  },
)

test('19 G.1: leaving the page and coming back sends another facts read', async () => {
  const { bff } = await form()
  expect(bff.to('GET', INFO).length, '19 G.1: one on arrival').toBe(1)
  cleanup()
  startAt(HASH)
  render(<App />)
  const main = screen.getByRole('main')
  await within(main).findByRole('heading', { level: 1, name: ID })
  await ready(main)
  expect(
    bff.to('GET', INFO).length,
    '19 G.1: another on the next arrival',
  ).toBe(2)
})

test('19 G.1: a save, a Discard and every edit send no facts read, and nothing polls it', async () => {
  const { bff, main } = await form()
  const ctx = document.getElementById('model-profile-ctx-size-control')!
  fireEvent.change(ctx, { target: { value: '65536' } })
  fireEvent.click(within(main).getByRole('button', { name: en('save-button') }))
  await waitFor(() => expect(bff.to('PUT', PUT).length).toBe(1))
  fireEvent.change(ctx, { target: { value: '131072' } })
  fireEvent.click(
    within(main).getByRole('button', { name: en('save-discard') }),
  )
  await waitFor(() =>
    expect(
      (
        document.getElementById(
          'model-profile-ctx-size-control',
        ) as HTMLInputElement
      ).value,
      '19 G.1: Discard restored the saved value',
    ).toBe('65536'),
  )
  expect(
    bff.to('GET', INFO).length,
    '19 G.1: a save, an edit and a Discard send no facts read',
  ).toBe(1)
  // Nothing polls: five minutes of a clock the page owns send no read (the
  // device read's clocked tests in ModelProfileGpu3.test.tsx are the shape).
  await new Promise((r) => setTimeout(r, 1_000))
  expect(bff.to('GET', INFO).length, '19 G.1: nothing polls').toBe(1)
})

test('19 G.1: the facts read keeps the reading line at 14 999 ms of its 15 s limit, and a time-out is a failed read', async () => {
  const h = held()
  const { bff, main } = await form({}, { modelInfo: h.answer }, true)
  expect(factsText(main), '19 G.1: the read is pending').toBe(READING)
  await advance(LIMIT - 1)
  expect(factsText(main), '19 G.1: 14 999 ms: still reading').toBe(READING)
  await advance(1)
  expect(factsText(main), '19 G.1: the time-out is a failed read').toBe(
    FAILED_FACTS,
  )
  expect(
    bff.to('GET', INFO).length,
    '19 G.1: the time-out sends no second read',
  ).toBe(1)
  // The page keeps working beside a read that never answered.
  fireEvent.change(document.getElementById('model-profile-ctx-size-control')!, {
    target: { value: '65536' },
  })
  fireEvent.click(within(main).getByRole('button', { name: en('save-button') }))
  await flush()
  expect(bff.to('PUT', PUT).length, '19 G.1: the page saves beside it').toBe(1)
  expect(sentValues(bff)['ctx-size'], '19 G.1: with its value').toBe('65536')
})

test.each([
  ['401', reply(401, { key: 'session-required' })],
  ['403', reply(403, { key: 'session-not-administrator' })],
])(
  '19 G.1: a facts read of %s shows the failed line, keeps the session and the page, and writes no alert',
  async (_what, modelInfo) => {
    const { bff, main } = await form({}, { modelInfo })
    expect(factsText(main), '19 G.1: a failed read').toBe(FAILED_FACTS)
    const el = factsEl(main)
    expect(el.getAttribute('role'), '19 G.1: not a status').toBeNull()
    expect(el.closest('[role="alert"]'), '19 G.1: not an alert').toBeNull()
    expect(
      within(main).queryAllByRole('alert'),
      '19 G.1: no alert anywhere',
    ).toHaveLength(0)
    expect(
      within(main).queryByText(en('signin-title')),
      '19 G.1: not signed out',
    ).toBeNull()
    expect(
      within(document.body).queryByRole('button', {
        name: en('session-sign-out'),
      }),
      '19 G.1: the session is still there',
    ).not.toBeNull()
    expect(
      bff.calls.filter((c) => c.method !== 'GET'),
      '19 G.1: the read sends no write',
    ).toEqual([])
  },
)

test.each([
  ['a 503 refusal', reply(503, { key: 'session-unavailable' })],
  ['a 404 refusal', reply(404, { key: 'no-model-profile-named' })],
  ['a 504 refusal', reply(504, { key: 'manager-timeout' })],
  ['neither info nor reason', { model: MODEL_FILE, info: null, reason: null }],
  [
    'both info and reason',
    {
      model: MODEL_FILE,
      info: { blockCount: BLOCKS, moe: false },
      reason: { key: 'model-info-missing', args: [MODEL_FILE] },
    },
  ],
  ['no model key', { info: { blockCount: BLOCKS, moe: false }, reason: null }],
  ['blockCount 0', readInfo({ blockCount: 0, moe: false })],
  ['blockCount negative', readInfo({ blockCount: -4, moe: false })],
  ['blockCount not whole', readInfo({ blockCount: 2.5, moe: false })],
  ['blockCount a string', readInfo({ blockCount: `${BLOCKS}`, moe: false })],
  ['blockCount null', readInfo({ blockCount: null, moe: false })],
  ['blockCount absent', readInfo({ moe: false })],
  ['moe not a boolean', readInfo({ blockCount: BLOCKS, moe: 'yes' })],
  ['moe absent', readInfo({ blockCount: BLOCKS })],
  ['info not an object', { model: MODEL_FILE, info: 'x', reason: null }],
  ['a reason with no key', reasonInfo('', [])],
  ['a reason key outside the four', reasonInfo('model-info-no-engine', [])],
  [
    'a reason that is a string',
    { model: MODEL_FILE, info: null, reason: 'gone' },
  ],
  [
    'a reason with args not an array',
    {
      model: MODEL_FILE,
      info: null,
      reason: { key: 'model-info-missing', args: MODEL_FILE },
    },
  ],
  ['an array body', []],
  ['a string body', 'info'],
  ['a null body', null],
])(
  '19 G.1: %s is a failed read: the failed line, and brief 14 as built',
  async (_what, body) => {
    const { main } = await form(
      { device: 'CUDA0,ROCm0', 'tensor-split': '85,15' },
      { modelInfo: body as ModelInfo },
    )
    expect(factsText(main), '19 G.1: the failed line').toBe(FAILED_FACTS)
    expect(
      weightFields(main).length,
      '19 G.1: a failed read leaves the weight fields (19 G.4)',
    ).toBe(2)
  },
)

// ── 19 G.2: the facts line ──────────────────────────────────────────────────

test('19 G.2: the facts line is one plain muted paragraph of the card, after its intro and before the split mode field, not a live region and not a control', async () => {
  const { main } = await form(
    {},
    { modelInfo: readInfo({ blockCount: BLOCKS, moe: false }) },
  )
  const el = factsEl(main)
  expect(el.tagName, '19 G.2: a paragraph').toBe('P')
  expect(el.getAttribute('role'), '19 G.2: not a live region').toBeNull()
  expect(el.closest('[role="status"]'), '19 G.2: not inside one').toBeNull()
  expect(el.className, '19 G.2: the muted line of the card').toContain('muted')
  expect(
    el.querySelector('input, select, textarea, button, a[href], [tabindex]'),
    '19 G.2: no control inside it, and it is not tabbable',
  ).toBeNull()
  expect(el.tabIndex, '19 G.2: out of the tab order').toBe(-1)
  const card = cardOf(main, GPU)
  const intro = [...card.querySelectorAll('p')].filter((p) =>
    INTRO_LINES.includes(p.textContent ?? ''),
  )
  expect(
    before(intro[intro.length - 1]!, el),
    '19 G.2: after the card intro',
  ).toBe(true)
  expect(
    before(el, modeSelect(main)),
    '19 G.2: before the split mode field',
  ).toBe(true)
})

test.each([
  ['a dense file', { blockCount: BLOCKS, moe: false }, factsOf(BLOCKS)],
  [
    'a MoE file with its counts',
    { blockCount: BLOCKS, moe: true, expertCount: 128, expertUsed: 8 },
    en('model-profile-model-facts-moe', `${BLOCKS}`, '128', '8'),
  ],
  [
    'a MoE file with no expert counts',
    { blockCount: BLOCKS, moe: true, expertCount: 0, expertUsed: 0 },
    en('model-profile-model-facts-moe-plain', `${BLOCKS}`),
  ],
  [
    'a MoE file whose expert counts are absent',
    { blockCount: BLOCKS, moe: true },
    en('model-profile-model-facts-moe-plain', `${BLOCKS}`),
  ],
  [
    'a MoE file whose expert counts are nonsense',
    { blockCount: BLOCKS, moe: true, expertCount: -2, expertUsed: 1.5 },
    en('model-profile-model-facts-moe-plain', `${BLOCKS}`),
  ],
])(
  '19 G.2: the facts line says the blocks of %s',
  async (_what, info, text) => {
    const { main } = await form({}, { modelInfo: readInfo(info) })
    expect(factsText(main)).toBe(text)
  },
)

test.each([
  ['model-info-fetched', [`${ID}-hf`]],
  ['model-profile-no-model', [ID]],
  ['model-info-missing', [MODEL_FILE]],
  ['model-info-unreadable', ['/m/x.gguf']],
])(
  '19 G.2: the facts line says the reason %s in its own sentence',
  async (key, args) => {
    const { main } = await form(
      { device: 'CUDA0,ROCm0', 'tensor-split': '85,15' },
      { modelInfo: reasonInfo(key, args) },
    )
    expect(factsText(main), '19 G.2: the reason read').toBe(
      factsNone(key, ...args),
    )
    expect(
      weightFields(main).length,
      '19 G.2: a reason keeps the weight fields (19 G.4)',
    ).toBe(2)
  },
)

test('19 G.2: while the facts read is pending the line says the file is being read, and it is not a live region', async () => {
  const h = held()
  const { main } = await form({}, { modelInfo: h.answer })
  const el = factsEl(main)
  expect((el.textContent ?? '').trim(), '19 G.2: the pending line').toBe(
    READING,
  )
  expect(
    el.getAttribute('role'),
    '19 G.2: the pending line is not a status',
  ).toBeNull()
  expect(el.className, '19 G.2: muted while reading').toContain('muted')
  h.resolve(reply(200, readInfo({ blockCount: BLOCKS, moe: false })))
  await waitFor(() => expect(factsText(main)).toBe(factsOf(BLOCKS)))
})
