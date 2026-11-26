// Brief 21, sub-slice TP3 (fe/briefs/21-tensor-placement.md), criteria 21 T.14
// and 21 T.14a: the tensor list of the model-info read. These are the unit
// tests of `api.ts`'s parse: what `modelFacts` makes of `info.tensors` and
// `info.tensorsTruncated`, and that a list of a bad shape leaves the layer
// facts standing. What the page does with a list it read is
// `TensorPlacementCoverage.test.tsx`.
import { describe, expect, test } from 'vitest'
import { parseModelInfo, type ModelFacts } from './api.ts'

const MODEL = '/var/lib/private-ai/models/Qwen3-27B-Q8_0.gguf'

/** The layer facts of a good reply (brief 19, section 0). */
const LAYERS = { blockCount: 64, moe: true, expertCount: 128, expertUsed: 8 }

/** A 200 body answering facts with `info`. */
function factsBody(info: Record<string, unknown> | null) {
  return { model: MODEL, info, reason: null }
}

/** The facts of a reply answering `info`, read: the read must answer facts. */
function factsOf(info: Record<string, unknown> | null): ModelFacts {
  const read = parseModelInfo(factsBody(info))
  expect(read, 'the read answers').not.toBeNull()
  const facts = (read as { facts?: ModelFacts }).facts
  expect(facts, 'the read answers facts, not a reason').toBeDefined()
  return facts!
}

describe('21 T.14: the tensor list of the read', () => {
  test('a good reply also reads info.tensors and info.tensorsTruncated', () => {
    const facts = factsOf({
      ...LAYERS,
      tensors: [
        { name: 'token_embd.weight', bytes: 1 << 30 },
        { name: 'blk.0.ffn_up_exps.weight', bytes: 1 << 20 },
        { name: 'output.weight', bytes: 0 },
      ],
      tensorsTruncated: true,
    })
    expect(facts.tensors, 'the rows, in the reply order').toEqual([
      { name: 'token_embd.weight', bytes: 1 << 30 },
      { name: 'blk.0.ffn_up_exps.weight', bytes: 1 << 20 },
      { name: 'output.weight', bytes: 0 },
    ])
    expect(facts.tensorsTruncated, 'the cut flag').toBe(true)
  })

  test('tensorsTruncated absent reads false', () => {
    const facts = factsOf({
      ...LAYERS,
      tensors: [{ name: 'output.weight', bytes: 1 << 29 }],
    })
    expect(facts.tensorsTruncated, 'absent is false').toBe(false)
  })

  test('an empty tensors list is an empty list, not no list', () => {
    const facts = factsOf({ ...LAYERS, tensors: [] })
    expect(facts.tensors, 'the empty list reads as itself').toEqual([])
  })

  test('tensors absent is no tensor list, and the layer facts stand', () => {
    const facts = factsOf({ ...LAYERS })
    expect(facts.tensors, 'no list').toBeNull()
    expect(facts.blockCount, 'the layer facts stand').toBe(64)
    expect(facts.moe, 'the layer facts stand').toBe(true)
    expect(facts.expertCount, 'the layer facts stand').toBe(128)
    expect(facts.expertUsed, 'the layer facts stand').toBe(8)
  })

  test('a tensors not of that shape is no tensor list, and the layer facts still count', () => {
    // Each of these is a reply a real manager can answer with: the rows are
    // `{name: string, bytes: whole number of 0 or more}`, nothing else.
    const bad: [string, unknown][] = [
      ['null', null],
      ['a string', 'token_embd.weight'],
      ['a number', 42],
      ['an object, not a list', { name: 'output.weight', bytes: 1 }],
      ['a list with no row shape', [{}]],
      ['a row with no bytes', [{ name: 'output.weight' }]],
      ['a row with a string bytes', [{ name: 'output.weight', bytes: '1' }]],
      ['a row with a null bytes', [{ name: 'output.weight', bytes: null }]],
      ['a row with a negative bytes', [{ name: 'output.weight', bytes: -1 }]],
      ['a row with a fraction', [{ name: 'output.weight', bytes: 1.5 }]],
      ['a row with a name that is not a string', [{ name: 1, bytes: 8 }]],
      ['a row that is a list', [['output.weight', 8]]],
      ['a row that is null', [null]],
      ['a list with one bad row', [{ name: 'a', bytes: 8 }, { bytes: 8 }]],
    ]
    for (const [what, tensors] of bad) {
      const facts = factsOf({ ...LAYERS, tensors })
      expect(facts.tensors, `no tensor list for ${what}`).toBeNull()
      expect(facts.blockCount, `${what}: the layer facts still count`).toBe(64)
      expect(facts.moe, `${what}: the layer facts still count`).toBe(true)
    }
  })

  test('21 T.14a: a tensorsTruncated that is not a boolean reads false', () => {
    for (const value of ['true', 1, null, {}]) {
      const facts = factsOf({
        ...LAYERS,
        tensors: [{ name: 'output.weight', bytes: 8 }],
        tensorsTruncated: value,
      })
      expect(facts.tensorsTruncated, `${JSON.stringify(value)}: false`).toBe(
        false,
      )
    }
  })

  test('a reason reply carries no tensor list', () => {
    const read = parseModelInfo({
      model: MODEL,
      info: null,
      reason: { key: 'model-info-missing', args: [MODEL] },
    })
    expect(read, 'the read answers a reason').toEqual({
      reason: { key: 'model-info-missing', args: [MODEL] },
    })
  })
})
