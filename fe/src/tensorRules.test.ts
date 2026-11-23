// Brief 21, sub-slice TP1 (fe/briefs/21-tensor-placement.md), criterion 21 T.1:
// the pure rule module of the fe (`fe/src/tensorRules.ts`), mirroring
// `crates/app/src/tensor_override.rs`. The vectors are copied from that file's
// tests, named in the criterion: the round trip, the whitespace, the first `=`,
// the piece with no device, llama.cpp's own FFN regexes, every kind
// splittable with its block agreeing with its pattern, the layers that do not
// read, a kind change keeping the layers, the shapes that stay Custom, the old
// unanchored embedding pattern and the order of the rules.
//
// The module's shape, as this test pins it:
// - `KINDS`: `{ id, label, pattern, block }[]`, the Rust table's 17 entries in
//   its order, Custom last; `label` is the strings key of the option's text.
// - `ALIASES`: `[from, to][]`, the patterns the table once wrote read as the
//   canonical ones.
// - `parse(value)`: `Rule[]` with `Rule = { pattern, device }`, in file order.
// - `render(rules)`: the value llama.cpp reads.
// - `splitPattern(pattern)`: `[kind index, layers | null]`.
// - `compose(kind index, layers)`: the pattern of that kind for those layers.
// - `layersText(layers)`: the Layers field's text.
// - `parseLayers(text)`: `{ layers }` when it reads (`layers` null for every
//   layer), `{ unreadable: true }` when it does not.
import { describe, expect, test } from 'vitest'
import {
  ALIASES,
  KINDS,
  compose,
  layersText,
  parse,
  parseLayers,
  render,
  splitPattern,
} from './tensorRules.ts'
import { en } from './test/bff.ts'

/** The Rust tests' two constants. */
const EMBD = String.raw`^token_embd\.weight`
const EXPS = String.raw`\.ffn_(up|down|gate|gate_up)_(ch|)exps`

/** The kind's index by its id, as the Rust tests' `idx`. */
function idx(id: string): number {
  const i = KINDS.findIndex((k) => k.id === id)
  expect(i, `the table has a kind ${id}`).toBeGreaterThan(-1)
  return i
}

// Read inside each test, never at the top: with the table not written yet a test
// has to fail on its own assertion, not on the module loading.
const CUSTOM = () => idx('custom')

describe('21 T.1: the kinds table', () => {
  test('the 17 entries, in the Rust file order, Custom last with no pattern of its own', () => {
    expect(KINDS.map((k) => k.id)).toEqual([
      'embd',
      'ple',
      'output',
      'layer',
      'attn',
      'ssm',
      'ffn',
      'ffn_up',
      'ffn_gate',
      'ffn_down',
      'exps',
      'exps_up',
      'exps_gate',
      'exps_down',
      'shexp',
      'router',
      'custom',
    ])
    const last = KINDS[KINDS.length - 1]!
    expect(last.id, 'Custom is last').toBe('custom')
    expect(last.pattern, 'Custom carries no canned pattern').toBe('')
    expect(last.block, 'Custom has no block').toBe('')
    for (const k of KINDS)
      expect(
        en(`model-profile-ot-kind-${k.id}`).length,
        `${k.id}: its label is the strings key of its option's text`,
      ).toBeGreaterThan(0)
  })

  test('the_ffn_kinds_are_llama_cpps_own_regexes: exps and ffn are llama.cpp verbatim, and the trailing dot of ffn is load-bearing', () => {
    expect(KINDS[idx('exps')].pattern).toBe(EXPS)
    expect(KINDS[idx('ffn')].pattern).toBe(String.raw`\.ffn_(up|down|gate)\.`)
    expect(KINDS[idx('exps_up')].pattern).toBe(String.raw`\.ffn_up_(ch|)exps`)
    expect(KINDS[idx('exps_gate')].pattern).toBe(
      String.raw`\.ffn_gate_(ch|)exps`,
    )
    expect(KINDS[idx('exps_down')].pattern).toBe(
      String.raw`\.ffn_down_(ch|)exps`,
    )
    const ffn = KINDS[idx('ffn')]!
    expect(ffn.pattern.endsWith(String.raw`\.`), 'the trailing dot').toBe(true)
    expect(
      ffn.pattern.includes('exps'),
      'the dense kind reaches no expert',
    ).toBe(false)
    expect(KINDS[idx('embd')].pattern).toBe(EMBD)
    expect(KINDS[idx('ple')].pattern).toBe(
      String.raw`^per_layer_token_embd\.weight`,
    )
    expect(KINDS[idx('output')].pattern).toBe(String.raw`^output\.weight`)
    expect(KINDS[idx('layer')].pattern).toBe(String.raw`^blk\.\d+\.`)
    expect(KINDS[idx('shexp')].pattern).toBe(
      String.raw`\.ffn_(up|down|gate)_shexp`,
    )
    expect(KINDS[idx('router')].pattern).toBe(String.raw`\.ffn_gate_inp`)
  })

  test('every_kind_is_splittable_and_its_block_agrees_with_its_pattern: no pattern holds a separator, a block is what follows the number, every canned pattern splits back to its kind', () => {
    for (const k of KINDS) {
      expect(k.pattern, `${k.id}: no comma`).not.toContain(',')
      expect(k.pattern, `${k.id}: no equals`).not.toContain('=')
      if (k.block !== '' && k.id !== 'layer')
        expect(k.pattern, `${k.id}: its block is its pattern`).toBe(k.block)
      if (k.id !== 'custom')
        expect(splitPattern(k.pattern), `${k.id}: its own pattern`).toEqual([
          idx(k.id),
          null,
        ])
    }
    expect(KINDS[idx('layer')].block, 'layer: a block of its own').toBe(
      String.raw`\.`,
    )
    expect(
      splitPattern(KINDS[idx('layer')].pattern),
      'layer: every layer',
    ).toEqual([idx('layer'), null])
    expect(splitPattern(''), 'nothing is Custom').toEqual([CUSTOM(), null])
  })
})

describe('21 T.1: parse and render', () => {
  test('parse_and_render_round_trip_a_multi_rule_value: the experts regex keeps its pipes and parentheses', () => {
    const s = `${EMBD}=ROCm0,${EXPS}=CPU`
    expect(parse(s)).toEqual([
      { pattern: EMBD, device: 'ROCm0' },
      { pattern: EXPS, device: 'CPU' },
    ])
    expect(render(parse(s))).toBe(s)
  })

  test('parse_tolerates_whitespace_and_never_invents_a_rule', () => {
    expect(parse('')).toEqual([])
    expect(parse('  ,  ')).toEqual([])
    expect(parse(' a = CPU , ')).toEqual([{ pattern: 'a', device: 'CPU' }])
  })

  test('a_rule_splits_at_the_first_equals_like_llama_cpp_does', () => {
    expect(parse('a=b=c')).toEqual([{ pattern: 'a', device: 'b=c' }])
  })

  test('a_piece_without_a_device_survives_as_a_rule_with_none', () => {
    expect(parse('token_embd')).toEqual([{ pattern: 'token_embd', device: '' }])
  })

  test('the_old_unanchored_embedding_pattern_reads_as_the_kind: the alias table, and a value of one aliased rule', () => {
    expect(ALIASES).toEqual([[String.raw`token_embd\.weight`, EMBD]])
    expect(parse(String.raw`token_embd\.weight=CUDA0`)).toEqual([
      { pattern: EMBD, device: 'CUDA0' },
    ])
    expect(
      splitPattern(String.raw`token_embd\.weight`),
      'splitPattern mirrors Rust: the alias is unaliased by parse, before the row splits its pattern',
    ).toEqual([CUSTOM(), null])
    expect(parse(String.raw`token_embd\.weight`)).toEqual([
      { pattern: EMBD, device: '' },
    ])
  })

  test("edits_never_reorder_the_rules: the file order is llama.cpp's match order, so a device edit and a pattern edit leave the other rule alone, in place", () => {
    const s = `${EXPS}=CPU,${EMBD}=ROCm0`
    const device = parse(s)
    device[1]!.device = 'CUDA0'
    expect(render(device)).toBe(`${EXPS}=CPU,${EMBD}=CUDA0`)
    const pattern = parse(s)
    pattern[0]!.pattern = 'x'
    expect(render(pattern)).toBe(`x=CPU,${EMBD}=ROCm0`)
    expect(
      parse(s).map((r) => r.pattern),
      'the value itself is untouched',
    ).toEqual([EXPS, EMBD])
  })
})

describe('21 T.1: the layers field', () => {
  test('layersText writes ranges, parseLayers reads them back; blank is every layer', () => {
    expect(layersText(null)).toBe('')
    expect(layersText([3])).toBe('3')
    expect(layersText([0, 1, 2])).toBe('0-2')
    expect(layersText([10, 11, 12, 15])).toBe('10-12, 15')
    expect(layersText([10, 11, 12, 15, 20])).toBe('10-12, 15, 20')
    expect(parseLayers('')).toEqual({ layers: null })
    expect(parseLayers('   ')).toEqual({ layers: null })
    expect(parseLayers('10-12, 15')).toEqual({ layers: [10, 11, 12, 15] })
    expect(parseLayers('10-12 15')).toEqual({ layers: [10, 11, 12, 15] })
    expect(parseLayers('10 12')).toEqual({ layers: [10, 12] })
    expect(parseLayers('15, 10-12')).toEqual({ layers: [10, 11, 12, 15] })
    expect(parseLayers('7')).toEqual({ layers: [7] })
    for (const [text, layers] of [
      ['10-12, 15', [10, 11, 12, 15]],
      ['0-2', [0, 1, 2]],
      ['3', [3]],
      ['10-12, 15, 20', [10, 11, 12, 15, 20]],
    ] as [string, number[]][])
      expect(
        parseLayers(layersText(layers)),
        `${text}: the text of the layers reads back as they are`,
      ).toEqual({ layers })
  })

  test('a range out of order, past 999, or more than 256 layers, does not read', () => {
    for (const text of [
      '10-',
      'x',
      '5-3',
      '1000',
      '0-999',
      '0-256',
      '3-',
      '-3',
    ])
      expect(parseLayers(text), `${text}: unreadable`).toEqual({
        unreadable: true,
      })
    expect(parseLayers('0-255'), '256 layers is the most allowed').toEqual({
      layers: Array.from({ length: 256 }, (_, i) => i),
    })
    expect(parseLayers('999'), '999 is the largest layer').toEqual({
      layers: [999],
    })
  })

  test('half_typed_or_bad_layers_leave_the_value_alone: an unreadable Layers text keeps the pattern, and a kind with no block takes no layers', () => {
    const value = `${EXPS}=CPU`
    for (const bad of ['10-', 'x', '5-3', '1000', '0-999']) {
      const [kind, layers] = splitPattern(parse(value)[0]!.pattern)
      expect(parseLayers(bad), `${bad}: unreadable`).toEqual({
        unreadable: true,
      })
      expect(
        render([{ pattern: compose(kind, layers), device: 'CPU' }]),
        `${bad}: the value is left as it was`,
      ).toBe(value)
    }
    const [embd] = splitPattern(EMBD)
    expect(compose(embd, [3]), 'no layers outside the blocks').toBe(EMBD)
    expect(compose(CUSTOM(), [3]), 'Custom composes nothing').toBe('')
  })

  test("a_new_kind_keeps_the_rows_layers: compose with the row's layers, and a kind with no block ignores them", () => {
    const [exps] = splitPattern(EXPS)
    const layers = [0, 1, 2]
    expect(compose(exps, layers)).toBe(
      String.raw`blk\.(0|1|2)\.ffn_(up|down|gate|gate_up)_(ch|)exps`,
    )
    expect(compose(idx('exps_down'), layers)).toBe(
      String.raw`blk\.(0|1|2)\.ffn_down_(ch|)exps`,
    )
    expect(compose(idx('layer'), layers)).toBe(String.raw`blk\.(0|1|2)\.`)
    expect(compose(idx('output'), layers)).toBe(String.raw`^output\.weight`)
    expect(
      compose(idx('attn'), [7]),
      'one layer is not listed in parentheses',
    ).toBe(String.raw`blk\.7\.attn_`)
    expect(splitPattern(String.raw`blk\.7\.attn_`)).toEqual([idx('attn'), [7]])
    expect(layersText(layers)).toBe('0-2')
    expect(compose(exps, []), 'no layers is every layer').toBe(EXPS)
  })

  test('only_the_composed_layer_form_reads_back_as_a_kind: a hand-written block list stays Custom', () => {
    for (const p of [
      String.raw`blk\.(3|1)\.ffn_up\.`,
      String.raw`blk\.(1|1)\.ffn_up\.`,
      String.raw`blk\.07\.ffn_up\.`,
      String.raw`blk\.15\.ffn_(up|down)\.weight`,
      String.raw`blk\.1[0-9]\.ffn_up\.`,
    ])
      expect(splitPattern(p), `${p}: Custom`).toEqual([CUSTOM(), null])
    expect(
      splitPattern(String.raw`blk\.(1|3)\.ffn_up\.`),
      'the canonical form',
    ).toEqual([idx('ffn_up'), [1, 3]])
  })
})
