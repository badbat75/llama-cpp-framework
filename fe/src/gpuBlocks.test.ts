// Brief 19, sub-slice LS1, criterion 19 G.3 (fe/briefs/19-layer-sliders.md):
// the block arithmetic of the fe, the mirror of `crates/app/src/gpu_split.rs`'s
// `Layout`, `block_counts`, `block_ranges` and `set_blocks`. The vectors are
// those of that module's unit tests, one test each, so the fe cuts the model
// exactly where the configurator's table does. The range texts are read through
// the brief's section 2 keys, never typed here.
import { describe, expect, test } from 'vitest'
import {
  blockCounts,
  blockRanges,
  layout,
  setBlocks,
  type Layout,
} from './gpuBlocks.ts'
import { en } from './test/bff.ts'

// Ornith-1.0-9B, the GUI's case: 33 blocks (32 of the trunk plus the MTP one),
// so 34 positions: every block and the output layer.
const ORNITH = 33

/** The layout of a whole model offloaded (`ngl` all). */
function full(): Layout {
  const l = layout(ORNITH, -1)
  expect(l, '19 G.3: a model with blocks has a layout').not.toBeNull()
  return l as Layout
}

/** The range label of the blocks `a` to `b`. */
const blocksOf = (a: string, b: string) =>
  en('model-profile-gpu-blocks-range', a, b)
/** The label of the blocks `a` to `b`, the output layer with the last one. */
const blocksPlus = (a: string, b: string) =>
  en('model-profile-gpu-blocks-plus-output', a, b)
/** The label of the one block `a` with the output layer. */
const blockPlus = (a: string) => en('model-profile-gpu-block-plus-output', a)
/** The label of the one block `a`, the output layer elsewhere. */
const oneBlock = (a: string) => en('model-profile-gpu-block-one', a)
/** The label of the output layer alone. */
const outputLayer = () => en('model-profile-gpu-output-layer')
/** The label of a device with no position. */
const nothing = () => en('model-profile-gpu-blocks-nothing')

/** The first position a label names, `null` for the one label with no number
 * in it (the output layer alone), exactly as the GUI's test reads it. */
function firstPosition(label: string): number | null {
  const head = label
    .replace(/^blocks /, '')
    .replace(/^block /, '')
    .split(/[- ]/)[0]
  const n = Number(head)
  return Number.isInteger(n) && head !== '' && /^\d/.test(head!) ? n : null
}

// The whole reason the count is the editable unit: a vector that sums to the
// positions assigns exactly itself, so writing back what the user typed
// reproduces what they typed and there is no inverse to invent.
test('19 G.3: a weight vector that sums to the positions is the block count itself', () => {
  const l = full()
  expect(
    l.count,
    '19 G.3: 33 blocks and the output layer are 34 positions',
  ).toBe(34)
  expect(blockCounts(['29', '5'], l), '19 G.3: 29 and 5').toEqual([29, 5])
  expect(blockCounts(['17', '17'], l), '19 G.3: 17 and 17').toEqual([17, 17])
  expect(blockCounts(['34', '0'], l), '19 G.3: all to the first').toEqual([
    34, 0,
  ])
  expect(blockCounts(['12', '11', '11'], l), '19 G.3: three rows').toEqual([
    12, 11, 11,
  ])
})

// …and the reason a percentage is not the editable unit: three of them are the
// same launch, so two of every three nudges would do nothing at all.
test('19 G.3: neighbouring percentages collapse onto the same cut', () => {
  const l = full()
  for (const pct of [83, 84, 85])
    expect(
      blockCounts([`${pct}`, `${100 - pct}`], l),
      `19 G.3: ${pct}% cuts in the same place`,
    ).toEqual([29, 5])
  expect(blockCounts(['82', '18'], l), '19 G.3: one block less').toEqual([
    28, 6,
  ])
  expect(blockCounts(['86', '14'], l), '19 G.3: one block more').toEqual([
    30, 4,
  ])
})

// Even is a vector of ones, which is NOT one block each: the projection is what
// makes that legible instead of a column of 1s.
test('19 G.3: Even projects to half the positions, not to a weight of one', () => {
  expect(blockCounts(['1', '1'], full()), '19 G.3: 1 and 1 over 34').toEqual([
    17, 17,
  ])
})

// Auto is decided by FREE VRAM at load, so there is no count to show and the
// rows keep the weights rather than printing a made-up one.
test('19 G.3: Auto has no block counts', () => {
  const l = full()
  expect(blockCounts(['0', '0'], l), '19 G.3: an all-zero vector').toBeNull()
  expect(blockCounts([], l), '19 G.3: an empty vector').toBeNull()
})

// The positions are the blocks PLUS the output layer, and a partial offload
// eats them off the front, not the back.
test('19 G.3: the layout counts the output layer and a partial offload shifts the start', () => {
  expect(layout(ORNITH, -1), '19 G.3: every layer on the GPU').toEqual({
    start: 0,
    count: 34,
  })
  expect(layout(ORNITH, -5), '19 G.3: a negative ngl is all').toEqual({
    start: 0,
    count: 34,
  })
  expect(
    layout(ORNITH, 99),
    '19 G.3: an ngl past the end is still all',
  ).toEqual({ start: 0, count: 34 })
  expect(layout(ORNITH, 10), '19 G.3: 10 offloaded are the LAST ten').toEqual({
    start: 24,
    count: 10,
  })
  expect(layout(0, -1), '19 G.3: no model read, no layout').toBeNull()
  expect(
    layout(-1, -1),
    '19 G.3: a negative block count is no model',
  ).toBeNull()
  expect(layout(ORNITH, 0), '19 G.3: nothing on the GPU, no layout').toBeNull()
})

// The label has to name the output layer: it is a position but not a block, and
// it carries the model's biggest single tensor.
test('19 G.3: the range label names the output layer separately', () => {
  const l = full()
  expect(blockRanges([29, 5], l), '19 G.3: 29 and 5').toEqual([
    blocksOf('0', '28'),
    blocksPlus('29', '32'),
  ])
  expect(blockRanges([33, 1], l), '19 G.3: a one-position tail').toEqual([
    blocksOf('0', '32'),
    outputLayer(),
  ])
  expect(blockRanges([32, 2], l), '19 G.3: a two-position tail').toEqual([
    blocksOf('0', '31'),
    blockPlus('32'),
  ])
  expect(blockRanges([34, 0], l), '19 G.3: an empty tail').toEqual([
    blocksPlus('0', '32'),
    nothing(),
  ])
  const partial = layout(ORNITH, 10)
  expect(partial, '19 G.3: the layout of a partial offload').not.toBeNull()
  expect(
    blockRanges([5, 5], partial as Layout),
    '19 G.3: the ranges start where the GPU does',
  ).toEqual([blocksOf('24', '28'), blocksPlus('29', '32')])
})

test('19 G.3: the range labels tile a three device split', () => {
  expect(blockRanges([12, 11, 11], full()), '19 G.3: 12, 11, 11').toEqual([
    blocksOf('0', '11'),
    blocksOf('12', '22'),
    blocksPlus('23', '32'),
  ])
})

test('19 G.3: a label of one block away from the output layer names that block', () => {
  expect(blockRanges([1, 33], full()), '19 G.3: 1 and 33').toEqual([
    oneBlock('0'),
    blocksPlus('1', '32'),
  ])
})

// Editing a count is not like editing a weight: the vector must keep summing to
// the positions, so the rows behind the edited one give the difference up.
test('19 G.3: setting a block count takes the difference from the back', () => {
  const l = full()
  // Typing back the count already shown normalizes the vector and changes
  // nothing about the cut: 85,15 and 29,5 are the same launch.
  expect(setBlocks(['85', '15'], 0, 29, l), '19 G.3: 85,15 at 29').toEqual([
    '29',
    '5',
  ])
  expect(
    blockCounts(['29', '5'], l),
    '19 G.3: the normalized vector cuts where the ratio did',
  ).toEqual(blockCounts(['85', '15'], l))
  // Giving the first device less hands the surplus to the other one.
  expect(setBlocks(['85', '15'], 0, 20, l), '19 G.3: 85,15 at 20').toEqual([
    '20',
    '14',
  ])
  // Three devices: the LAST row absorbs first, so an edit stays local.
  expect(setBlocks(['1', '1', '1'], 0, 20, l), '19 G.3: 1,1,1 at 20').toEqual([
    '20',
    '11',
    '3',
  ])
  // …cascading forward only once the back is exhausted.
  expect(setBlocks(['1', '1', '1'], 0, 34, l), '19 G.3: 1,1,1 at 34').toEqual([
    '34',
    '0',
    '0',
  ])
  // Out of range is clamped, never wrapped or written through.
  expect(setBlocks(['85', '15'], 0, 999, l), '19 G.3: past the places').toEqual(
    ['34', '0'],
  )
  expect(setBlocks(['85', '15'], 0, -5, l), '19 G.3: below 0').toEqual([
    '0',
    '34',
  ])
  // A row that is not there is a no-op, like every other edit here.
  expect(setBlocks(['85', '15'], 7, 4, l), '19 G.3: no such row').toEqual([
    '85',
    '15',
  ])
})

// Editing anything but the head: the tail still pays first, and a row IN FRONT
// of the edited one is touched only once everything behind it is at zero.
test('19 G.3: a middle or last row edit still absorbs from the tail first', () => {
  const l = full()
  expect(blockCounts(['1', '1', '1'], l), '19 G.3: the starting point').toEqual(
    [12, 11, 11],
  )
  // Middle row up: only the tail moves.
  expect(setBlocks(['1', '1', '1'], 1, 20, l), '19 G.3: middle at 20').toEqual([
    '12',
    '20',
    '2',
  ])
  // Middle row up past what the tail holds: the head gives up the rest, or the
  // count asked for cannot be honoured.
  expect(setBlocks(['1', '1', '1'], 1, 30, l), '19 G.3: middle at 30').toEqual([
    '4',
    '30',
    '0',
  ])
  // Last row: the row before it is the tail, and absorbs both directions.
  expect(setBlocks(['1', '1', '1'], 2, 20, l), '19 G.3: last at 20').toEqual([
    '12',
    '2',
    '20',
  ])
  expect(setBlocks(['1', '1', '1'], 2, 0, l), '19 G.3: last at 0').toEqual([
    '12',
    '22',
    '0',
  ])
})

// Auto has no counts to start from, so an edit seeds an even cut, the GUI's
// `even_counts`, and applies to that.
test('19 G.3: an edit of an all-zero vector seeds an even cut', () => {
  const l = full()
  expect(setBlocks(['0', '0'], 0, 20, l), '19 G.3: 0,0 at 20').toEqual([
    '20',
    '14',
  ])
  expect(setBlocks(['', ''], 1, 4, l), '19 G.3: empty weights at 4').toEqual([
    '30',
    '4',
  ])
})

// Every count written back must survive its own projection, or the cell would
// show something else than what the user asked for.
test('19 G.3: a block edit round trips through the projection', () => {
  const l = full()
  for (let n = 0; n <= l.count; n++) {
    const weights = setBlocks(['1', '1'], 0, n, l)
    expect(
      blockCounts(weights, l),
      `19 G.3: ${n} blocks, and the vector is never an Auto one`,
    ).toEqual([n, l.count - n])
    for (const w of weights)
      expect(w, '19 G.3: the counts are written as whole numbers').toMatch(
        /^\d+$/,
      )
  }
})

// The same round trip over three devices and every position of the edit, where
// the cascade has somewhere to go wrong: the vector keeps summing to the
// positions, the edited row got exactly what was asked, and the ranges still
// tile the whole span with no gap and no overlap.
test('19 G.3: every three device edit keeps the budget and the ranges contiguous', () => {
  const l = full()
  for (let i = 0; i < 3; i++) {
    for (let n = 0; n <= l.count; n++) {
      const label = `row ${i} = ${n}`
      const counts = blockCounts(setBlocks(['5', '20', '9'], i, n, l), l)
      expect(counts, `${label}: the vector is never an Auto one`).not.toBeNull()
      const got = counts as number[]
      expect(
        got.reduce((a, b) => a + b, 0),
        `${label}: the budget`,
      ).toBe(34)
      expect(got[i], `${label}: the row shows what it asked for`).toBe(n)
      let pos = l.start
      const texts = blockRanges(got, l)
      for (let j = 0; j < got.length; j++) {
        const c = got[j]!
        const text = texts[j]!
        if (c === 0) {
          expect(text, `${label}: an empty row`).toBe(nothing())
          continue
        }
        const first = firstPosition(text)
        if (first === null) {
          expect(text, `${label}: the lone tail is named`).toBe(outputLayer())
          expect(pos, `${label}: the output layer is the last position`).toBe(
            l.start + l.count - 1,
          )
        } else {
          expect(first, `${label}: ${text} follows the row before it`).toBe(pos)
        }
        pos += c
      }
      expect(pos, `${label}: every position is covered`).toBe(l.start + l.count)
    }
  }
})

// A decimal vector is projected exactly: every weight scaled by 10 to the
// power of the most digits after a dot among them, and the cut compared in
// integers, so `0.6,0.4` and `3,2` are the same launch.
describe('19 G.3: the decimal projection is exact', () => {
  const cases: [string[], string[], number[]][] = [
    [
      ['0.6', '0.4'],
      ['3', '2'],
      [21, 13],
    ],
    [
      ['0.5', '0.5'],
      ['1', '1'],
      [17, 17],
    ],
    [
      ['1.25', '0.75'],
      ['5', '3'],
      [22, 12],
    ],
    [
      ['.5', '0'],
      ['1', '0'],
      [34, 0],
    ],
  ]
  for (const [decimal, whole, counts] of cases)
    test(`${decimal.join(',')}: the same counts as ${whole.join(',')}`, () => {
      const l = full()
      expect(
        blockCounts(decimal, l),
        '19 G.3: the decimal vector',
      ).not.toBeNull()
      expect(
        blockCounts(decimal, l),
        '19 G.3: the same cut as the whole numbers',
      ).toEqual(blockCounts(whole, l))
      expect(
        blockCounts(decimal, l),
        `19 G.3: ${decimal.join(',')} over 34 positions`,
      ).toEqual(counts)
    })

  test('an empty weight is 0, as llama.cpp pads a short vector', () => {
    const l = full()
    expect(blockCounts(['3', ''], l), '19 G.3: 3 and an empty weight').toEqual(
      blockCounts(['3', '0'], l),
    )
    expect(
      blockCounts(['3', ''], l),
      '19 G.3: the whole model goes to the first row',
    ).toEqual([34, 0])
  })
})
