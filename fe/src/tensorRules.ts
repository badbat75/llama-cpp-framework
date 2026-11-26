// Brief 21 (fe/briefs/21-tensor-placement.md), sub-slice TP1, criterion 21 T.1:
// the rule module of the web page, the mirror of
// `crates/app/src/tensor_override.rs`, kept pure so every rule of it is a unit
// test. The vectors of `tensorRules.test.ts` are that module's own, so the card's
// rows read and write the value the configurator's table does.
//
// The grammar, as llama.cpp reads it (that module's header): the value is split
// on `,` and every rule at its FIRST `=`, and neither separator is escapable, so
// a pattern holds neither; the device is looked up exactly among the buffer type
// names the probe prints, `CPU` one of them; a rule's pattern is searched
// anywhere in a tensor's name and the FIRST rule that matches wins, so no edit
// moves a rule.
//
// TP3 adds the counting of that first match: `coverage` answers, per rule, what
// the rule places on the model the page read, as the GUI's
// `crates/app/src/tensor_picker.rs` counts it (its `coverage` 133 and
// `fmt_bytes` 110).
import { t } from './strings.ts'

/** One entry of the kinds table: `label` is the strings key of the option's
 *  text, `pattern` the canned regex and `block` the part after the layer
 *  number. Custom is the last entry and carries neither pattern nor block. */
export type TensorKind = {
  id: string
  label: string
  pattern: string
  block: string
}

/** One rule: a pattern and the device it goes on ("" for none). */
export type TensorRule = { pattern: string; device: string }

/** The layers of a rule: the layer numbers, or null for every layer. */
export type Layers = number[] | null

/** What a Layers text reads as: the layers it lists, or unreadable. */
export type LayersRead = { layers: number[] | null } | { unreadable: true }

/** The id of the free-text escape hatch, always the last entry: it keeps the
 *  row's pattern and carries none of its own. */
export const CUSTOM = 'custom'

/** What a per-layer kind's pattern has in front of the block number, the layer
 *  this key names at most, and the most layers one rule may list: every layer
 *  becomes an alternative of the regex. */
const BLOCK_PREFIX = String.raw`blk\.`
const MAX_LAYER = 999
const MAX_LAYERS = 256

/** The tensor families, in the Rust table's order: the ones outside the blocks
 *  first, anchored with `^` because a pattern is searched anywhere in a name,
 *  then the ones inside them. `exps` and `ffn` are llama.cpp's own regexes, so
 *  "MoE experts to CPU" here spells the same placement as the experts slider;
 *  the Rust file names the engine release each one was read from. None holds a
 *  `,` or an `=`. */
export const KINDS: TensorKind[] = [
  {
    id: 'embd',
    label: 'model-profile-ot-kind-embd',
    pattern: String.raw`^token_embd\.weight`,
    block: '',
  },
  {
    id: 'ple',
    label: 'model-profile-ot-kind-ple',
    pattern: String.raw`^per_layer_token_embd\.weight`,
    block: '',
  },
  {
    id: 'output',
    label: 'model-profile-ot-kind-output',
    pattern: String.raw`^output\.weight`,
    block: '',
  },
  {
    id: 'layer',
    label: 'model-profile-ot-kind-layer',
    pattern: String.raw`^blk\.\d+\.`,
    block: String.raw`\.`,
  },
  {
    id: 'attn',
    label: 'model-profile-ot-kind-attn',
    pattern: String.raw`\.attn_`,
    block: String.raw`\.attn_`,
  },
  {
    id: 'ssm',
    label: 'model-profile-ot-kind-ssm',
    pattern: String.raw`\.ssm_`,
    block: String.raw`\.ssm_`,
  },
  {
    id: 'ffn',
    label: 'model-profile-ot-kind-ffn',
    pattern: String.raw`\.ffn_(up|down|gate)\.`,
    block: String.raw`\.ffn_(up|down|gate)\.`,
  },
  {
    id: 'ffn_up',
    label: 'model-profile-ot-kind-ffn_up',
    pattern: String.raw`\.ffn_up\.`,
    block: String.raw`\.ffn_up\.`,
  },
  {
    id: 'ffn_gate',
    label: 'model-profile-ot-kind-ffn_gate',
    pattern: String.raw`\.ffn_gate\.`,
    block: String.raw`\.ffn_gate\.`,
  },
  {
    id: 'ffn_down',
    label: 'model-profile-ot-kind-ffn_down',
    pattern: String.raw`\.ffn_down\.`,
    block: String.raw`\.ffn_down\.`,
  },
  {
    id: 'exps',
    label: 'model-profile-ot-kind-exps',
    pattern: String.raw`\.ffn_(up|down|gate|gate_up)_(ch|)exps`,
    block: String.raw`\.ffn_(up|down|gate|gate_up)_(ch|)exps`,
  },
  {
    id: 'exps_up',
    label: 'model-profile-ot-kind-exps_up',
    pattern: String.raw`\.ffn_up_(ch|)exps`,
    block: String.raw`\.ffn_up_(ch|)exps`,
  },
  {
    id: 'exps_gate',
    label: 'model-profile-ot-kind-exps_gate',
    pattern: String.raw`\.ffn_gate_(ch|)exps`,
    block: String.raw`\.ffn_gate_(ch|)exps`,
  },
  {
    id: 'exps_down',
    label: 'model-profile-ot-kind-exps_down',
    pattern: String.raw`\.ffn_down_(ch|)exps`,
    block: String.raw`\.ffn_down_(ch|)exps`,
  },
  {
    id: 'shexp',
    label: 'model-profile-ot-kind-shexp',
    pattern: String.raw`\.ffn_(up|down|gate)_shexp`,
    block: String.raw`\.ffn_(up|down|gate)_shexp`,
  },
  {
    id: 'router',
    label: 'model-profile-ot-kind-router',
    pattern: String.raw`\.ffn_gate_inp`,
    block: String.raw`\.ffn_gate_inp`,
  },
  { id: CUSTOM, label: 'model-profile-ot-kind-custom', pattern: '', block: '' },
]

/** `[from, to]`: a pattern this page once wrote, read as the canonical kind's it
 *  meant. Only where the two reach the same tensors on every model the old one
 *  was offered for. */
export const ALIASES: [string, string][] = [
  [String.raw`token_embd\.weight`, String.raw`^token_embd\.weight`],
]

/** The last index, the Custom row an unrecognised pattern lands on. */
const customIndex = KINDS.length - 1

/** A pattern as the rows read it: an `ALIASES` entry becomes its kind's. */
function unalias(pattern: string): string {
  const alias = ALIASES.find(([from]) => from === pattern)
  return alias ? alias[1] : pattern
}

/** The value → its rules, in file order. Tolerant on purpose: a piece with no
 *  `=` (only reachable from a hand-edited file) keeps its text as the pattern
 *  and no device, so the row survives, shows its problem, and can be fixed
 *  instead of vanishing at the next save. */
export function parse(value: string): TensorRule[] {
  return value
    .split(',')
    .map((piece) => piece.trim())
    .filter((piece) => piece !== '')
    .map((piece) => {
      const at = piece.indexOf('=')
      if (at < 0) return { pattern: unalias(piece), device: '' }
      return {
        pattern: unalias(piece.slice(0, at).trim()),
        device: piece.slice(at + 1).trim(),
      }
    })
}

/** The rules → the value llama.cpp reads. */
export function render(rules: TensorRule[]): string {
  return rules.map((r) => `${r.pattern}=${r.device}`).join(',')
}

/** A pattern taken apart: its kind and, for a rule limited to some blocks, the
 *  layer numbers (ascending, no repeats); null for every layer, and for a kind
 *  with no blocks. An unrecognised pattern, a hand-written one included, is
 *  Custom with no layers: only the shape `compose` writes reads back as a kind,
 *  so a leading zero, a repeat or an out-of-order list stays the regex it is. */
export function splitPattern(pattern: string): [number, Layers] {
  const canned = KINDS.findIndex(
    (k) => k.id !== CUSTOM && k.pattern === pattern,
  )
  if (canned >= 0) return [canned, null]
  if (!pattern.startsWith(BLOCK_PREFIX)) return [customIndex, null]
  const rest = pattern.slice(BLOCK_PREFIX.length)
  let nums: string
  let block: string
  if (rest.startsWith('(')) {
    const inner = rest.slice(1)
    const at = inner.indexOf(')')
    if (at < 0) return [customIndex, null]
    nums = inner.slice(0, at)
    block = inner.slice(at + 1)
  } else {
    const digits = rest.search(/[^0-9]/)
    const end = digits < 0 ? rest.length : digits
    nums = rest.slice(0, end)
    block = rest.slice(end)
  }
  const kind = KINDS.findIndex((k) => k.block !== '' && k.block === block)
  if (kind < 0) return [customIndex, null]
  const parts = nums.split('|')
  const layers: number[] = []
  for (const part of parts) {
    if (!/^[0-9]{1,3}$/.test(part)) return [customIndex, null]
    layers.push(Number(part))
  }
  const canonical = parts.every((part, i) => String(layers[i]) === part)
  const ascending = layers.every((n, i) => i === 0 || layers[i - 1] < n)
  if (!canonical || !ascending) return [customIndex, null]
  return [kind, layers]
}

/** The pattern of `kind` limited to `layers` (null, empty, or a kind with no
 *  blocks: that kind's pattern over every layer). */
export function compose(kind: number, layers: Layers): string {
  const entry = KINDS[kind]
  if (entry === undefined) return ''
  if (entry.block === '' || layers === null || layers.length === 0) {
    return entry.pattern
  }
  const nums = layers.map((n) => String(n))
  return nums.length === 1
    ? `${BLOCK_PREFIX}${nums[0]}${entry.block}`
    : `${BLOCK_PREFIX}(${nums.join('|')})${entry.block}`
}

/** The Layers field's text: `10-12, 15`; empty for every layer. */
export function layersText(layers: Layers): string {
  if (layers === null) return ''
  const parts: string[] = []
  let i = 0
  while (i < layers.length) {
    let j = i
    while (j + 1 < layers.length && layers[j + 1] === layers[j] + 1) j += 1
    parts.push(j === i ? String(layers[i]) : `${layers[i]}-${layers[j]}`)
    i = j + 1
  }
  return parts.join(', ')
}

/** One number of a Layers text: digits only (Rust reads a `u32`, which also
 *  takes a leading `+`; the field's own text says numbers and ranges). */
function layerNumber(text: string): number | null {
  return /^[0-9]+$/.test(text) ? Number(text) : null
}

/** The Layers field → its layers, ascending and without repeats; null for every
 *  layer. Unreadable: a half-typed range, a number that is not one, a range out
 *  of order, one past `MAX_LAYER` or wider than the most layers, and more
 *  layers than the most allowed. */
export function parseLayers(text: string): LayersRead {
  const layers = new Set<number>()
  for (const part of text.split(/[, ]/).filter((p) => p !== '')) {
    const at = part.indexOf('-')
    const head = at < 0 ? part : part.slice(0, at).trim()
    const tail = at < 0 ? part : part.slice(at + 1).trim()
    const from = layerNumber(head)
    const to = layerNumber(tail)
    if (
      from === null ||
      to === null ||
      from > to ||
      to > MAX_LAYER ||
      to - from >= MAX_LAYERS
    ) {
      return { unreadable: true }
    }
    for (let n = from; n <= to; n += 1) layers.add(n)
    if (layers.size > MAX_LAYERS) return { unreadable: true }
  }
  return {
    layers: layers.size === 0 ? null : [...layers].sort((a, b) => a - b),
  }
}

/** One tensor of the list the page read (brief 21, 21 T.14): the name a rule is
 *  searched in, and its size in bytes. The shape of `api.ts`'s `ModelTensor`,
 *  named here so the counting stays a unit of its own. */
export type TensorInfo = { name: string; bytes: number }

/** `1.21 GiB`, `312.0 MiB`, `16 KiB`: the GUI's `fmt_bytes` (that file 110).
 *  Two decimals from 1 GiB, one from 1 MiB, whole KiB rounded up below that,
 *  1024 based. */
export function fmtBytes(bytes: number): string {
  const KIB = 1024
  if (bytes >= KIB * KIB * KIB)
    return `${(bytes / (KIB * KIB * KIB)).toFixed(2)} GiB`
  if (bytes >= KIB * KIB) return `${(bytes / (KIB * KIB)).toFixed(1)} MiB`
  return `${Math.ceil(bytes / KIB)} KiB`
}

/** Per rule of the value `s`, the line the row shows under it (brief 21,
 *  21 T.15), as the GUI's `tensor_picker::coverage` counts (that file 133): the
 *  rules in order, a tensor counted for the FIRST rule whose pattern finds it
 *  (llama.cpp applies the first match), `n` newly taken with their bytes summed
 *  and `s` found but taken by an earlier rule. The pattern is searched anywhere
 *  in the name, as `regex_search` does: `new RegExp(pattern)` with no flags.
 *  `''` for a rule with an empty pattern, and for every rule when the page has
 *  no tensor list: the row shows no line. */
export function coverage(s: string, tensors: TensorInfo[]): string[] {
  const rules = parse(s)
  if (tensors.length === 0) return rules.map(() => '')
  const taken = tensors.map(() => false)
  return rules.map((rule) => {
    if (rule.pattern === '') return ''
    let n = 0
    let bytes = 0
    let shadowed = 0
    let re: RegExp
    try {
      re = new RegExp(rule.pattern)
    } catch {
      return t('model-profile-ot-unreadable')
    }
    for (const [i, one] of tensors.entries()) {
      if (!re.test(one.name)) continue
      if (taken[i]) shadowed += 1
      else {
        taken[i] = true
        n += 1
        bytes += one.bytes
      }
    }
    const line =
      n === 0
        ? shadowed === 0
          ? t('model-profile-ot-matches-none')
          : t('model-profile-ot-all-taken')
        : n === 1
          ? t('model-profile-ot-tensors-one', fmtBytes(bytes))
          : t('model-profile-ot-tensors-many', `${n}`, fmtBytes(bytes))
    return n > 0 && shadowed > 0
      ? line + t('model-profile-ot-more-taken', `${shadowed}`)
      : line
  })
}
