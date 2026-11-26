// Brief 21, sub-slice TP3 (fe/briefs/21-tensor-placement.md), criterion 21 T.15:
// the coverage line's counting, as the GUI's `tensor_picker::coverage` counts
// (crates/app/src/tensor_picker.rs 133) and `fmt_bytes` formats (110). The two
// vectors are copied from the Rust tests `coverage_counts_first_match_wins`
// (661) and `an_unanchored_output_rule_shows_what_it_really_takes` (678), over
// the same fixture (496). The line in the row, its style and its id are
// `TensorPlacementCoverage.test.tsx`.
import { describe, expect, test } from 'vitest'
import { coverage, fmtBytes } from './tensorRules.ts'
import { en } from './test/bff.ts'

/** One tensor of the list the page read (21 T.14). */
type Tensor = { name: string; bytes: number }

const t = (name: string, bytes: number): Tensor => ({ name, bytes })

/** The GUI's fixture (tensor_picker.rs 496): a small hybrid MoE, 4 blocks,
 *  attention and SSM alternating, experts and shared experts everywhere, plus
 *  the tensors outside the blocks. */
function model(): Tensor[] {
  const v: Tensor[] = [
    t('token_embd.weight', 1 << 30),
    t('per_layer_token_embd.weight', 1 << 28),
  ]
  for (let b = 0; b < 4; b++) {
    const mix = b % 2 === 0 ? 'attn_qkv' : 'ssm_out'
    v.push(t(`blk.${b}.attn_norm.weight`, 4096))
    v.push(t(`blk.${b}.${mix}.weight`, 1 << 20))
    v.push(t(`blk.${b}.attn_output.weight`, 1 << 20))
    for (const p of ['up', 'gate', 'down']) {
      v.push(t(`blk.${b}.ffn_${p}_exps.weight`, 1 << 24))
      v.push(t(`blk.${b}.ffn_${p}_shexp.weight`, 1 << 18))
    }
    v.push(t(`blk.${b}.ffn_gate_inp.weight`, 1 << 16))
  }
  v.push(t('output_norm.weight', 4096))
  v.push(t('output.weight', 1 << 29))
  return v
}

/** The names `pattern` matches, as 21 T.15 says the page matches: `new
 *  RegExp(pattern)` with no flags, `test` on the name (llama.cpp's
 *  `regex_search` finds the pattern anywhere in the name). */
function names(tensors: Tensor[], pattern: string): string[] {
  const re = new RegExp(pattern)
  return tensors.filter((one) => re.test(one.name)).map((one) => one.name)
}

describe('21 T.15: the coverage line counts the first match winning', () => {
  test('coverage_counts_first_match_wins (the Rust vector, tensor_picker.rs 661)', () => {
    const m = model()
    const s = String.raw`\.ffn_up_(ch|)exps=CPU,\.ffn_(up|down|gate|gate_up)_(ch|)exps=ROCm0,nothing=CPU,[=CPU`
    const c = coverage(s, m)
    expect(c[0], 'rule 1: the up experts, 4 of them').toBe(
      en('model-profile-ot-tensors-many', '4', '64.0 MiB'),
    )
    expect(c[1], 'rule 2: 8 new, 4 taken by rule 1').toBe(
      en('model-profile-ot-tensors-many', '8', '128.0 MiB') +
        en('model-profile-ot-more-taken', '4'),
    )
    expect(c[2], 'rule 3: nothing matches').toBe(
      en('model-profile-ot-matches-none'),
    )
    expect(c[3], 'rule 4: the regex does not compile').toContain(
      'cannot be read',
    )
    expect(coverage(s, []), 'no tensor list: no line for any rule').toEqual([
      '',
      '',
      '',
      '',
    ])
  })

  test('an unanchored output rule shows what it really takes (the Rust vector, tensor_picker.rs 678)', () => {
    const m = model()
    expect(
      names(m, String.raw`output\.weight`).length,
      'unanchored: every attn_output too',
    ).toBe(5)
    expect(names(m, String.raw`^output\.weight`), 'anchored: the head').toEqual(
      ['output.weight'],
    )
  })

  test('the text of each case: one, many, none, all taken, many with more taken, unreadable, empty', () => {
    const m = model()
    expect(coverage(String.raw`^output\.weight=CPU`, m)[0], 'one tensor').toBe(
      en('model-profile-ot-tensors-one', '512.0 MiB'),
    )
    expect(
      coverage(String.raw`^token_embd\.weight=CPU`, m)[0],
      'one tensor, a GiB',
    ).toBe(en('model-profile-ot-tensors-one', '1.00 GiB'))
    expect(
      coverage(String.raw`\.ffn_up_exps=CPU`, m)[0],
      'several tensors',
    ).toBe(en('model-profile-ot-tensors-many', '4', '64.0 MiB'))
    expect(coverage('nothing=CPU', m)[0], 'no tensor matches').toBe(
      en('model-profile-ot-matches-none'),
    )
    expect(
      coverage(
        `${String.raw`\.ffn_up_exps`}=CPU,blk\\.0\\.ffn_up_exps=CPU`,
        m,
      )[0],
      'rule 1 takes its own',
    ).toBe(en('model-profile-ot-tensors-many', '4', '64.0 MiB'))
    expect(
      coverage(
        `${String.raw`\.ffn_up_exps`}=CPU,blk\\.0\\.ffn_up_exps=CPU`,
        m,
      )[1],
      'rule 2 matches one, taken by rule 1',
    ).toBe(en('model-profile-ot-all-taken'))
    expect(
      coverage(
        `${String.raw`\.ffn_up_exps`}=CPU,${String.raw`blk\.(0|1)\.ffn_(up|down)_exps`}=CPU`,
        m,
      )[1],
      'rule 2: 2 new, 2 taken by rule 1',
    ).toBe(
      en('model-profile-ot-tensors-many', '2', '32.0 MiB') +
        en('model-profile-ot-more-taken', '2'),
    )
    expect(coverage('[=CPU', m)[0], 'the regex does not compile').toBe(
      en('model-profile-ot-unreadable'),
    )
    expect(coverage('=CPU', m), 'an empty pattern has no line').toEqual([''])
    expect(coverage('', m), 'no rule, no line').toEqual([])
  })

  test('the count follows the rules in order: a later rule never counts a tensor an earlier one took', () => {
    const m = model()
    // The same set, in the other order: the second rule of each pair is the
    // experts of every block, so the first rule's tensors are the shadowed ones.
    const first = coverage(
      `${String.raw`blk\.0\.ffn_up_exps`}=CPU,${String.raw`\.ffn_up_exps`}=CPU`,
      m,
    )
    expect(first[0], 'rule 1 takes one').toBe(
      en('model-profile-ot-tensors-one', '16.0 MiB'),
    )
    expect(first[1], 'rule 2: 3 new, 1 taken by rule 1').toBe(
      en('model-profile-ot-tensors-many', '3', '48.0 MiB') +
        en('model-profile-ot-more-taken', '1'),
    )
  })
})

describe('21 T.15: the bytes, as the GUI formats them (fmt_bytes)', () => {
  test('two decimals from 1 GiB, one from 1 MiB, whole KiB rounded up, 1024 based', () => {
    expect(fmtBytes(1 << 30), '1 GiB').toBe('1.00 GiB')
    expect(fmtBytes(1299227607), '1.21 GiB').toBe('1.21 GiB')
    expect(fmtBytes(327155712), '312.0 MiB').toBe('312.0 MiB')
    expect(fmtBytes(1 << 20), '1 MiB, one decimal').toBe('1.0 MiB')
    expect(fmtBytes(1048575), 'one byte under 1 MiB: 1024 KiB').toBe('1024 KiB')
    expect(fmtBytes(16384), '16 KiB').toBe('16 KiB')
    expect(fmtBytes(1025), 'one byte over 1 KiB rounds up').toBe('2 KiB')
    expect(fmtBytes(1), 'one byte is 1 KiB').toBe('1 KiB')
    expect(fmtBytes(0), 'nothing').toBe('0 KiB')
  })
})
