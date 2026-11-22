// Brief 19, slice LS1, its LS1 review of 2026-11-22 (fe/briefs/19-layer-sliders.md):
// the two defects the designer found, pinned as the tests of the fix.
//
// D1: from 720 px the GPU list's fieldset is a two-column `.field` grid
// (index.css 1388) and the budget, Auto or malformed paragraph of 19 G.4 and
// 19 G.5 took the second cell of its first row, pushing the device list into
// the 200 px label column and stretching Auto and Even to the list's whole
// height. The fix gives the three paragraphs one class, `gpu-blocks-line`, and
// the 720 px rule places that class and the list in column 2. The CSS half is
// pinned in Reflow.test.ts (19 LS1 review D1); here each of the three carries
// the class and is a direct child of the fieldset that rule names, as its `>`
// needs.
//
// D2: `GpuSplit.tsx` 583 asks `Number(w) === 0` whether the vector is Auto's,
// and JavaScript reads 0 out of `0x0` and `0e1`, so such a vector is Auto and
// malformed at once and both paragraphs show. Malformed wins: the Auto
// paragraph is absent.
//
// The helpers are those of ModelProfileGpu6.test.tsx, repeated here (a test
// file imports no other test file).
import { cleanup, render, screen, within } from '@testing-library/react'
import { afterEach, beforeEach, expect, test } from 'vitest'
import App from './App.tsx'
import {
  DEVICES,
  en,
  fakeBff,
  MODEL_FILE,
  modelProfileDoc,
  reply,
  startAt,
  type BffOptions,
  type ModelInfo,
} from './test/bff.ts'

const ID = 'Qwen3-27B'
const HASH = `#/profiles/model/${ID}`
const GPU = en('model-profile-gpu-distribution')
const GPUS = en('model-profile-gpus')
/** The two GPUs of the brief's split, as the file spells them. */
const A = 'CUDA0'
const B = 'ROCm0'
/** Ornith-1.0-9B, the brief's file: 33 blocks, so 34 places. */
const BLOCK_COUNT = 33
const PLACES = 34
/** The id the 720 px rule of D1 names, on the GPUs fieldset (GD1.4). */
const FIELDSET_ID = 'model-profile-gpu-devices'
const AUTO_SHARE = en('model-profile-gpu-share-auto')
const BUDGET = en(
  'model-profile-gpu-blocks-budget',
  `${PLACES}`,
  `${PLACES - 1}`,
)
const AUTO = en('model-profile-gpu-blocks-auto', `${PLACES}`)
/** The malformed line of 19 G.4 for the checked vector `vector`. */
const malformed = (vector: string): string =>
  en('model-profile-gpu-blocks-malformed', vector)

/** The 200 body of a read that answered `blockCount` blocks and no experts. */
function readInfo(blockCount = BLOCK_COUNT): ModelInfo {
  return { model: MODEL_FILE, info: { blockCount, moe: false }, reason: null }
}

/** The page's options: the brief's probe and a model file of 33 blocks. */
function read(): BffOptions {
  return { devices: DEVICES, modelInfo: readInfo() }
}

/** Waits for the profile page of a section with the keys `values`. */
async function form(values: Record<string, string>) {
  const bff = fakeBff({
    ...read(),
    modelProfile: () =>
      reply(
        200,
        modelProfileDoc({}, { revision: 'r1', values: { ...values } }),
      ),
  })
  render(<App />)
  const main = screen.getByRole('main')
  await within(main).findByRole('heading', { level: 1, name: ID })
  await within(main).findByRole('heading', { level: 3, name: GPU })
  await ready(main)
  return { bff, main }
}

/** Waits for the device read to land with the brief's probe time. */
async function ready(main: HTMLElement): Promise<void> {
  const fs = await within(main).findByRole('group', { name: GPUS })
  await within(fs).findByText(
    new Date(DEVICES.probedAt!).toLocaleString(undefined, {
      dateStyle: 'medium',
      timeStyle: 'short',
    }),
  )
}

/** The GPUs fieldset, named by its legend (GD1.4). */
function fieldsetEl(main: HTMLElement): HTMLElement {
  return within(main).getByRole('group', { name: GPUS })
}

/** The list of rows (GD1.4). */
function listEl(main: HTMLElement): HTMLElement {
  const list = fieldsetEl(main).querySelector('ul[role="list"]')
  expect(list, 'GD1.4: a ul role="list" of the rows').not.toBeNull()
  return list as HTMLElement
}

/** How many of the fieldset's paragraphs have exactly `text`. */
function paragraphCount(main: HTMLElement, text: string): number {
  return [...fieldsetEl(main).querySelectorAll('p')].filter(
    (p) => (p.textContent ?? '').replace(/\s+/g, ' ').trim() === text,
  ).length
}

/** The fieldset's paragraph whose text is exactly `text`, once (19 G.4). */
function paragraph(main: HTMLElement, text: string): HTMLElement {
  const found = [...fieldsetEl(main).querySelectorAll('p')].filter(
    (p) => (p.textContent ?? '').replace(/\s+/g, ' ').trim() === text,
  )
  expect(
    found.map((p) => p.textContent),
    `${text}: once, in the GPU card`,
  ).toHaveLength(1)
  return found[0]!
}

/** The leaf elements of the fieldset reading exactly `auto` (GD2.2). */
function autoShares(main: HTMLElement): HTMLElement[] {
  return [...fieldsetEl(main).querySelectorAll<HTMLElement>('*')].filter(
    (el) =>
      el.children.length === 0 && (el.textContent ?? '').trim() === AUTO_SHARE,
  )
}

beforeEach(() => {
  startAt(HASH)
})

afterEach(() => {
  cleanup()
  startAt('')
})

// ── D1: one class on the three blocks paragraphs ────────────────────────────

/** The three paragraphs of D1, each in the state that shows it: the blocks
 * vector 85,15, the Auto vector, a vector with a word in it. */
const lines: [string, Record<string, string>, string][] = [
  [
    'the budget line of 19 G.5',
    { device: `${A},${B}`, 'tensor-split': '85,15' },
    BUDGET,
  ],
  [
    'the Auto line of 19 G.4',
    { device: `${A},${B}`, 'tensor-split': '0,0' },
    AUTO,
  ],
  [
    'the malformed line of 19 G.4',
    { device: `${A},${B}`, 'tensor-split': '85,x' },
    malformed('85,x'),
  ],
]

test.each(lines)(
  '19 LS1 review D1: %s carries the class gpu-blocks-line and is a direct child of the GPUs fieldset, so the 720 px grid places it, and the list after it, in the control column',
  async (_what, values, text) => {
    const { main } = await form(values)
    const fs = fieldsetEl(main)
    expect(fs.id, 'D1: the fieldset the 720 px rule names by id').toBe(
      FIELDSET_ID,
    )
    const el = paragraph(main, text)
    expect(el.tagName, 'D1: a paragraph').toBe('P')
    expect(
      el.className,
      `D1: ${text}: the class the 720 px rule selects`,
    ).toContain('gpu-blocks-line')
    expect(
      el.className,
      'D1: the paragraph keeps the muted line look of 19 G.2 and 19 G.4',
    ).toContain('muted')
    expect(
      el.parentElement,
      `D1: ${text}: a direct child of the fieldset, as the rule's > needs`,
    ).toBe(fs)
    expect(
      listEl(main).parentElement,
      'D1: the device list is a direct child of the fieldset too',
    ).toBe(fs)
  },
)

// ── D2: malformed wins over Auto ────────────────────────────────────────────

/** The vectors where JavaScript reads 0 out of a weight GD2.6 refuses, so
 * today the Auto paragraph and the malformed one show together. */
const zeroWords: [string, string][] = [
  ['0x0,1', 'the first weight reads 0 and the second reads 1'],
  ['0e1,0', 'every weight reads 0: the vector Auto claims, in words'],
  ['0x0,0', 'the same, the second weight written 0'],
]

test.each(zeroWords)(
  '19 LS1 review D2: tensor-split %s is a malformed vector, not Auto: the malformed paragraph shows and the Auto paragraph is absent (%s)',
  async (vector) => {
    const { main } = await form({ device: `${A},${B}`, 'tensor-split': vector })
    paragraph(main, malformed(vector))
    expect(
      paragraphCount(main, AUTO),
      `D2: ${vector}: malformed wins, so the Auto paragraph of 19 G.4 is absent`,
    ).toBe(0)
    expect(
      paragraphCount(main, BUDGET),
      `D2: ${vector}: no budget line without counts`,
    ).toBe(0)
    expect(
      within(fieldsetEl(main)).queryAllByRole('textbox').length,
      `D2: ${vector}: no editor at all, weight field or blocks input`,
    ).toBe(0)
    expect(
      within(fieldsetEl(main)).queryAllByRole('slider').length,
      `D2: ${vector}: no slider`,
    ).toBe(0)
    expect(
      autoShares(main).length,
      `D2: ${vector}: a malformed vector shows no auto share (19 G.4)`,
    ).toBe(0)
  },
)
